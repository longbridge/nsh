//! Where the document is on screen: the blocks' frames as their views
//! report them, the lines the core laid each textblock's text out in
//! (`onTextLayout`), and the map between a textblock's document positions
//! and the offsets of its rendered text (atoms and widgets are one
//! character there). From them: the caret's box at a position, the boxes
//! of a range, and the position at a point. Document positions and visual
//! places stay apart: a position at a line wrap is two places, which the
//! selection's affinity tells apart.

use std::{collections::HashMap, rc::Rc};

use nsh_core::{TextLine, prelude::*};
use unicode_segmentation::UnicodeSegmentation;

use crate::rich_editor::state::Affinity;

/// The block id of the document itself (its children's parent).
pub(crate) const DOC: u64 = 0;

/// A stretch of a textblock's content and of its rendered text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Run {
    /// Its start in the textblock's content (from its first position).
    pub(crate) doc: usize,
    /// Its length in the document (0 for a widget).
    pub(crate) doc_len: usize,
    /// Its start in the rendered text (UTF-16 units).
    pub(crate) text: u32,
    /// Its length in the rendered text.
    pub(crate) text_len: u32,
    /// For a widget: which side of its position it sticks to.
    pub(crate) side: i32,
}

/// A textblock's content as its text renders it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct InlineMap {
    pub(crate) runs: Vec<Run>,
    /// The content's size in the document.
    pub(crate) size: usize,
    /// The rendered text's length.
    pub(crate) len: u32,
}

impl InlineMap {
    /// The text offset of content offset `d`.
    pub(crate) fn to_text(&self, d: usize) -> u32 {
        let mut t = 0;
        for run in &self.runs {
            if run.doc_len == 0 {
                if run.doc < d || (run.doc == d && run.side < 0) {
                    t = run.text + run.text_len;
                    continue;
                }
                break;
            }
            if d >= run.doc + run.doc_len {
                t = run.text + run.text_len;
                continue;
            }
            if d >= run.doc {
                // Atoms are one unit either way: inside a text run the
                // units match.
                return run.text + (d - run.doc).min(run.text_len as usize) as u32;
            }
            break;
        }
        t
    }

    /// The content offset of text offset `t`.
    pub(crate) fn to_doc(&self, t: u32) -> usize {
        for run in &self.runs {
            if t < run.text + run.text_len {
                if t < run.text {
                    return run.doc;
                }
                return if run.doc_len == 0 {
                    run.doc
                } else if run.text_len as usize != run.doc_len {
                    // A leaf drawn as text (a mention): the nearer side, the
                    // caret never stands inside it.
                    run.doc + usize::from(2 * (t - run.text) >= run.text_len) * run.doc_len
                } else {
                    run.doc + ((t - run.text) as usize).min(run.doc_len)
                };
            }
        }
        self.size
    }
}

/// A textblock's place on screen.
#[derive(Debug, Clone, Default)]
pub(crate) struct TextGeo {
    /// The textblock's first content position in the document.
    pub(crate) start: usize,
    pub(crate) map: InlineMap,
    /// The lines of its text, in the text's box.
    pub(crate) lines: Vec<TextLine>,
    /// The text's frame in its block's box.
    pub(crate) frame: Bounds<Points>,
    /// The box the frame is in (the block's own, else a node view's
    /// content slot).
    pub(crate) owner: Option<u64>,
    /// The box of each inline view and image in its text (in the text's
    /// box, where the text engine placed it), by its offset in the
    /// textblock's content: what its attachment character stands for on a
    /// line. Relative to the block, so an edit in another block leaves
    /// them in place.
    pub(crate) widths: HashMap<usize, Bounds<Points>>,
}

/// Measures a text in a font (the platform's measurer, captured).
pub(crate) type Measure = Rc<dyn Fn(&str, &str) -> Points>;

/// The geometry of a rendered document.
#[derive(Default)]
pub(crate) struct Geometry {
    /// Each block's frame in its parent's box, and its parent; `None` for
    /// a frame measured in the content box directly (a content slot).
    pub(crate) frames: HashMap<u64, (Option<u64>, Bounds<Points>)>,
    /// Where a block's children start in its box, when they sit in a
    /// holder that is not the block's own origin (a task item's content,
    /// after its checkbox): their frames are in the holder, not the block.
    pub(crate) insets: HashMap<u64, Point<Points>>,
    /// The textblocks, by block id.
    pub(crate) texts: HashMap<u64, TextGeo>,
    /// Textblock ids ordered by `TextGeo::start`.
    pub(crate) text_order: Vec<(usize, u64)>,
    /// The atoms and node views edited as units, by block id: their
    /// position.
    pub(crate) atoms: HashMap<u64, usize>,
    /// The font measuring within a line (relative widths only).
    pub(crate) font: String,
    /// Captured on the view's engine thread, independent of runtime entry.
    pub(crate) measure: Option<Measure>,
}

/// The caret's height in the font's ascender and descender: about one em
/// for common fonts, whose ascender and descender span some 1.2 em.
const CARET_EM: f32 = 0.85;

/// A place a point lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    /// A position in a textblock, with the side of a wrap it showed on.
    Text(usize, Affinity),
    /// An atom block (its position).
    Node(usize),
}

impl Geometry {
    fn measure_text(&self, text: &str) -> Points {
        self.measure.as_ref().map_or_else(
            || measure_text(&self.font, text),
            |measure| measure(&self.font, text),
        )
    }

    /// Whether the content point `at` is in a block's box before where its
    /// children start: on a task item's checkbox, or under it.
    pub(crate) fn before_holder(&self, at: Point<Points>) -> bool {
        self.insets.iter().any(|(id, inset)| {
            let (Some(origin), Some((_, frame))) = (self.origin(*id), self.frames.get(id)) else {
                return false;
            };
            at.x >= origin.x
                && at.x < origin.x + inset.x
                && at.y >= origin.y
                && at.y < origin.y + frame.size.height
        })
    }

    /// Block `id`'s top left in the content box.
    pub(crate) fn origin(&self, id: u64) -> Option<Point<Points>> {
        let mut at = Point::default();
        let mut id = Some(id);
        let mut guard = 0;
        while let Some(current) = id {
            if current == DOC {
                return Some(at);
            }
            let (parent, frame) = self.frames.get(&current)?;
            at.x += frame.origin.x;
            at.y += frame.origin.y;
            id = match parent {
                Some(p) => {
                    if let Some(inset) = self.insets.get(p) {
                        at.x += inset.x;
                        at.y += inset.y;
                    }
                    Some(*p)
                }
                None => return Some(at),
            };
            guard += 1;
            if guard > 256 {
                return None;
            }
        }
        Some(at)
    }

    /// Textblock `id`'s text box in the content box.
    fn text_box(&self, id: u64) -> Option<(Point<Points>, &TextGeo)> {
        let geo = self.texts.get(&id)?;
        let origin = self.origin(geo.owner.unwrap_or(id))?;
        Some((
            Point::new(origin.x + geo.frame.origin.x, origin.y + geo.frame.origin.y),
            geo,
        ))
    }

    /// The textblock holding position `pos` (its content's range).
    pub(crate) fn text_at(&self, pos: usize) -> Option<u64> {
        if self.text_order.len() != self.texts.len() {
            return self
                .texts
                .iter()
                .find(|(_, g)| pos >= g.start && pos <= g.start + g.map.size)
                .map(|(id, _)| *id);
        }
        let idx = self.text_order.partition_point(|&(start, _)| start <= pos);
        let &(_, id) = self.text_order.get(idx.checked_sub(1)?)?;
        let g = self.texts.get(&id)?;
        (pos >= g.start && pos <= g.start + g.map.size).then_some(id)
    }

    /// The textblock ids whose content overlaps `from..=to`, in document order.
    pub(crate) fn text_ids_in(&self, from: usize, to: usize) -> Vec<u64> {
        if self.text_order.len() != self.texts.len() {
            let mut ids: Vec<(&u64, &TextGeo)> = self
                .texts
                .iter()
                .filter(|(_, g)| g.start + g.map.size >= from && g.start <= to)
                .collect();
            crate::sort::sort_by_key(&mut ids, |(_, g)| g.start);
            return ids.into_iter().map(|(id, _)| *id).collect();
        }
        let first = self
            .text_order
            .partition_point(|&(start, _)| start < from)
            .saturating_sub(1);
        let end_idx = self.text_order.partition_point(|&(start, _)| start <= to);
        self.text_order[first.min(end_idx)..end_idx]
            .iter()
            .filter_map(|&(_, id)| {
                let g = self.texts.get(&id)?;
                (g.start + g.map.size >= from && g.start <= to).then_some(id)
            })
            .collect()
    }

    /// The caret's box at `pos`: as high as its line, without width.
    pub(crate) fn caret_rect(&self, pos: usize, affinity: Affinity) -> Option<Bounds<Points>> {
        let id = self.text_at(pos)?;
        let (origin, geo) = self.text_box(id)?;
        let t = geo.map.to_text(pos - geo.start);
        let (x, y, h) = match line_of(&geo.lines, t, affinity) {
            Some(i) => {
                let line = &geo.lines[i];
                (line.x + self.x_in_line(geo, line, t), line.y, line.height)
            }
            None => (px(0.0), px(0.0), geo.frame.size.height.max(px(1.0))),
        };
        Some(Bounds::new(
            Point::new(origin.x + x, origin.y + y),
            Size::new(px(0.0), h),
        ))
    }

    /// The box of the inline view or image at `pos` (content box): from
    /// the caret before it, as wide as its view and as high as its line.
    pub(crate) fn inline_rect(&self, pos: usize) -> Option<Bounds<Points>> {
        let id = self.text_at(pos)?;
        let geo = self.texts.get(&id)?;
        let width = geo.widths.get(&pos.checked_sub(geo.start)?)?.size.width;
        let b = self.caret_rect(pos, Affinity::Downstream)?;
        Some(Bounds::new(b.origin, Size::new(width, b.size.height)))
    }

    /// The drawn caret at `pos`: about the font's em tall (the ascender
    /// and descender less their own gap, as gpui-kit's input draws 0.85 of
    /// a normal line), centred in the line, rather than the whole line box
    /// [`Self::caret_rect`] gives.
    pub(crate) fn caret_glyph_rect(
        &self,
        pos: usize,
        affinity: Affinity,
    ) -> Option<Bounds<Points>> {
        let b = self.caret_rect(pos, affinity)?;
        let id = self.text_at(pos)?;
        let (_, geo) = self.text_box(id)?;
        let t = geo.map.to_text(pos - geo.start);
        let Some(line) = line_of(&geo.lines, t, affinity).map(|i| &geo.lines[i]) else {
            return Some(b);
        };
        let glyphs = (line.ascender + line.descender) * CARET_EM;
        if glyphs <= px(0.0) || glyphs >= line.height {
            return Some(b);
        }
        let top = b.origin.y + (line.height - glyphs) * 0.5;
        Some(Bounds::new(
            Point::new(b.origin.x, top),
            Size::new(px(0.0), glyphs),
        ))
    }

    /// The boxes covering `from..to`, one per line of each textblock it
    /// crosses (through a line's end where it goes on).
    pub(crate) fn range_rects(&self, from: usize, to: usize) -> Vec<Bounds<Points>> {
        let mut out = Vec::new();
        for id in self.text_ids_in(from, to) {
            let Some((origin, _)) = self.text_box(id) else {
                continue;
            };
            out.extend(
                self.text_range_rects(id, from, to)
                    .into_iter()
                    .map(|b| Bounds::new(origin + b.origin, b.size)),
            );
        }
        out
    }

    /// The boxes of `from..to` in textblock `id`, in its text's box: one
    /// per line it crosses (through a line's end where it goes on), none
    /// when the range misses the block.
    pub(crate) fn text_range_rects(&self, id: u64, from: usize, to: usize) -> Vec<Bounds<Points>> {
        let mut out = Vec::new();
        let Some(geo) = self.texts.get(&id) else {
            return out;
        };
        let (start, end) = (geo.start, geo.start + geo.map.size);
        if end < from || start > to {
            return out;
        }
        let a = geo.map.to_text(from.max(start) - start);
        let b = geo.map.to_text(to.min(end) - start);
        for line in &geo.lines {
            if line.end < a || line.start > b {
                continue;
            }
            let x0 = if a > line.start {
                self.x_in_line(geo, line, a)
            } else {
                px(0.0)
            };
            let through =
                to > end || b > line.end || (b == line.end && to > start + geo.map.to_doc(b));
            let x1 = if b < line.end || (b == line.end && !through) {
                self.x_in_line(geo, line, b)
            } else {
                line.width + px(4.0)
            };
            if x1 <= x0 && !(through && a <= line.end) {
                continue;
            }
            out.push(Bounds::new(
                Point::new(line.x + x0, line.y),
                Size::new((x1 - x0).max(px(4.0)), line.height),
            ));
        }
        if geo.lines.is_empty() && from <= start && to > end {
            out.push(Bounds::new(
                Point::default(),
                Size::new(px(4.0), geo.frame.size.height),
            ));
        }
        out
    }

    /// Where `x` of a line falls: the nearest boundary between graphemes.
    fn offset_in_line(&self, geo: &TextGeo, line: &TextLine, x: Points) -> u32 {
        if x <= px(0.0) || line.text.is_empty() {
            return line.start;
        }
        let widths = self.line_attachments(geo, line);
        if x >= self.line_end(line, &widths) {
            return line.end;
        }
        let mut best = (line.start, Points(f32::MAX));
        let mut units = 0u32;
        let mut boundaries = vec![0u32];
        for g in line.text.graphemes(true) {
            units += g.encode_utf16().count() as u32;
            boundaries.push(units);
        }
        for u in boundaries {
            let d = Points((self.x_at(line, &widths, u) - x).0.abs());
            if d < best.1 {
                best = (line.start + u, d);
            }
        }
        best.0.min(line.end)
    }

    /// The x of text offset `t` in a line (from the line's left).
    fn x_in_line(&self, geo: &TextGeo, line: &TextLine, t: u32) -> Points {
        if t <= line.start || line.text.is_empty() {
            return px(0.0);
        }
        let widths = self.line_attachments(geo, line);
        if t >= line.end {
            return self.line_end(line, &widths);
        }
        self.x_at(line, &widths, t - line.start)
    }

    /// Where a line ends, from its left: its width, or past its last
    /// inline view where the engine gave less (one that reports a line as
    /// wide as its box keeps its width).
    fn line_end(&self, line: &TextLine, attachments: &[(u32, Bounds<Points>)]) -> Points {
        let last = attachments
            .iter()
            .map(|(_, b)| b.origin.x - line.x + b.size.width)
            .fold(px(0.0), |a, b| if b > a { b } else { a });
        if last > line.width { last } else { line.width }
    }

    /// The attachments of a line (its inline views and images) whose box
    /// is known: their offset in the line's text and their box.
    fn line_attachments(&self, geo: &TextGeo, line: &TextLine) -> Vec<(u32, Bounds<Points>)> {
        if geo.widths.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut k = 0u32;
        for c in line.text.chars() {
            if c == nsh_core::ATTACHMENT_CHAR
                && let Some(w) = geo.widths.get(&geo.map.to_doc(line.start + k))
            {
                out.push((k, *w));
            }
            k += c.len_utf16() as u32;
        }
        out
    }

    /// The x of `k` units into a line. An attachment is where the text
    /// engine placed its view, as wide as the view, whatever the font says
    /// of its character: the views' edges are fixed points of the line.
    /// Between two of them (or a view and an end of the line) the font
    /// only gives relative widths (`Self::measure`), so that run of text is
    /// scaled to the room it has there.
    fn x_at(&self, line: &TextLine, attachments: &[(u32, Bounds<Points>)], k: u32) -> Points {
        let units: Vec<u16> = line.text.encode_utf16().collect();
        let k = (k as usize).min(units.len());
        // The fixed points: the line's ends and each view's edges.
        let mut anchors: Vec<(usize, f32)> = vec![(0, 0.0)];
        for (at, b) in attachments {
            let left = (b.origin.x - line.x).0;
            anchors.push((*at as usize, left));
            anchors.push((*at as usize + 1, left + b.size.width.0));
        }
        anchors.push((units.len(), self.line_end(line, attachments).0));
        let at = anchors.partition_point(|(u, _)| *u <= k);
        let (u0, x0) = anchors[at.saturating_sub(1)];
        if u0 == k || at >= anchors.len() {
            return Points(x0);
        }
        let (u1, x1) = anchors[at];
        let run = |end: usize| String::from_utf16_lossy(&units[u0..end]);
        let whole = self.measure_text(&run(u1));
        let share = if whole.0 > 0.0 {
            self.measure_text(&run(k)).0 / whole.0
        } else {
            0.0
        };
        Points(x0 + (x1 - x0).max(0.0) * share)
    }

    /// The place at `p` (content box): an atom it is on, else the nearest
    /// textblock's nearest position.
    pub(crate) fn hit(&self, p: Point<Points>) -> Option<Hit> {
        self.hit_in(p, None)
    }

    /// The place at `p` among the atoms and textblocks in `range` (all
    /// without one): a table cell's, which its neighbors beside it must
    /// not take a press from.
    pub(crate) fn hit_in(&self, p: Point<Points>, range: Option<(usize, usize)>) -> Option<Hit> {
        let within = |pos: usize| range.is_none_or(|(from, to)| pos >= from && pos < to);
        for (id, pos) in &self.atoms {
            if within(*pos)
                && let (Some(origin), Some((_, frame))) = (self.origin(*id), self.frames.get(id))
            {
                let b = Bounds::new(origin, frame.size);
                if p.x >= b.origin.x
                    && p.x < b.origin.x + b.size.width
                    && p.y >= b.origin.y
                    && p.y < b.origin.y + b.size.height
                {
                    return Some(Hit::Node(*pos));
                }
            }
        }
        // The textblock nearest vertically (then horizontally).
        let mut best: Option<(f32, f32, u64)> = None;
        for (id, text) in &self.texts {
            if !within(text.start) {
                continue;
            }
            let Some((origin, geo)) = self.text_box(*id) else {
                continue;
            };
            let (top, bottom) = match (geo.lines.first(), geo.lines.last()) {
                (Some(a), Some(b)) => (origin.y + a.y, origin.y + b.y + b.height),
                _ => (origin.y, origin.y + geo.frame.size.height.max(px(1.0))),
            };
            let dy = if p.y < top {
                (top - p.y).0
            } else if p.y >= bottom {
                (p.y - bottom).0 + 0.001
            } else {
                0.0
            };
            let (left, right) = (origin.x, origin.x + geo.frame.size.width);
            let dx = if p.x < left {
                (left - p.x).0
            } else if p.x > right {
                (p.x - right).0
            } else {
                0.0
            };
            if best.is_none_or(|(by, bx, _)| dy < by || (dy == by && dx < bx)) {
                best = Some((dy, dx, *id));
            }
        }
        let (_, _, id) = best?;
        let (origin, geo) = self.text_box(id)?;
        let local = Point::new(p.x - origin.x, p.y - origin.y);
        if geo.lines.is_empty() {
            return Some(Hit::Text(geo.start, Affinity::Downstream));
        }
        let index = geo
            .lines
            .iter()
            .position(|l| local.y < l.y + l.height)
            .unwrap_or(geo.lines.len() - 1);
        let line = &geo.lines[index];
        let t = self.offset_in_line(geo, line, local.x - line.x);
        // The end of a wrapped line shows on that line.
        let affinity = if t == line.end && index + 1 < geo.lines.len() {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        };
        Some(Hit::Text(geo.start + geo.map.to_doc(t), affinity))
    }

    /// The line `pos` shows on in its textblock, and how many lines the
    /// textblock has.
    pub(crate) fn line_index(&self, pos: usize, affinity: Affinity) -> Option<(usize, usize)> {
        let id = self.text_at(pos)?;
        let geo = self.texts.get(&id)?;
        let t = geo.map.to_text(pos - geo.start);
        Some((line_of(&geo.lines, t, affinity)?, geo.lines.len()))
    }

    /// The start or end of the line `pos` shows on.
    pub(crate) fn line_edge(&self, pos: usize, affinity: Affinity, end: bool) -> Option<usize> {
        let id = self.text_at(pos)?;
        let geo = self.texts.get(&id)?;
        let t = geo.map.to_text(pos - geo.start);
        let line = &geo.lines[line_of(&geo.lines, t, affinity)?];
        let edge = if end {
            // A wrapped line's end is the next one's start: before the
            // space it broke at.
            let text_end = line.start + line.text.encode_utf16().count() as u32;
            let trimmed = line.text.trim_end_matches(['\n', ' ']);
            if line.end < geo.map.len && trimmed.len() < line.text.len() {
                line.start + trimmed.encode_utf16().count() as u32
            } else {
                text_end.min(line.end)
            }
        } else {
            line.start
        };
        Some(geo.start + geo.map.to_doc(edge))
    }
}

/// The line text offset `t` shows on.
fn line_of(lines: &[TextLine], t: u32, affinity: Affinity) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    for (i, line) in lines.iter().enumerate() {
        let last = i + 1 == lines.len();
        if t < line.end || last {
            return Some(i);
        }
        if t == line.end {
            // At a wrap: the upper line upstream, else the next one.
            let next_starts = lines.get(i + 1).is_some_and(|n| n.start == t);
            if affinity == Affinity::Upstream || !next_starts {
                return Some(i);
            }
        }
    }
    Some(lines.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A task item's content holds its blocks after the checkbox: their
    /// frames are in the holder, so the holder's offset is part of where
    /// they are (the caret and the selection of a task's text).
    #[test]
    fn a_holders_inset_is_part_of_its_children_origin() {
        let mut geo = Geometry::default();
        let at =
            |x: f32, y: f32| Bounds::new(Point::new(px(x), px(y)), Size::new(px(100.0), px(20.0)));
        geo.frames.insert(1, (Some(DOC), at(0.0, 40.0)));
        geo.frames.insert(2, (Some(1), at(0.0, 0.0)));
        assert_eq!(geo.origin(2), Some(Point::new(px(0.0), px(40.0))));
        geo.insets.insert(1, Point::new(px(24.0), px(0.0)));
        assert_eq!(geo.origin(2), Some(Point::new(px(24.0), px(40.0))));
        // The item itself is in its own parent's box: no inset of its own.
        assert_eq!(geo.origin(1), Some(Point::new(px(0.0), px(40.0))));
    }

    /// The checkbox is in the item's box before its content: a press there
    /// is the checkbox's, wherever the item is in the document.
    #[test]
    fn a_point_before_a_holder_is_on_the_items_checkbox() {
        let mut geo = Geometry::default();
        let at =
            |x: f32, y: f32| Bounds::new(Point::new(px(x), px(y)), Size::new(px(100.0), px(20.0)));
        geo.frames.insert(1, (Some(DOC), at(10.0, 40.0)));
        geo.frames.insert(2, (Some(DOC), at(10.0, 60.0)));
        geo.insets.insert(1, Point::new(px(24.0), px(0.0)));
        let on = |x: f32, y: f32| geo.before_holder(Point::new(px(x), px(y)));
        assert!(on(10.0, 40.0) && on(33.0, 59.0));
        // The item's text, the margin beside it, and a block without a
        // holder are not.
        assert!(!on(34.0, 50.0) && !on(9.0, 50.0) && !on(20.0, 70.0));
    }

    /// "ab", an inline view 80 wide, "cd", in a font of 10 a character:
    /// the view's character stands for its width, not a character's.
    #[test]
    fn an_inline_view_is_as_wide_as_its_view_on_its_line() {
        let mut geo = Geometry {
            measure: Some(Rc::new(|_: &str, t: &str| {
                px(10.0 * t.chars().count() as f32)
            })),
            ..Geometry::default()
        };
        geo.frames.insert(
            7,
            (
                Some(DOC),
                Bounds::new(Point::default(), Size::new(px(400.0), px(20.0))),
            ),
        );
        let run = |doc, doc_len, text, text_len| Run {
            doc,
            doc_len,
            text,
            text_len,
            side: 0,
        };
        geo.texts.insert(
            7,
            TextGeo {
                start: 1,
                map: InlineMap {
                    runs: vec![run(0, 2, 0, 2), run(2, 1, 2, 1), run(3, 2, 3, 2)],
                    size: 5,
                    len: 5,
                },
                lines: vec![TextLine {
                    text: "ab\u{FFFC}cd".into(),
                    start: 0,
                    end: 5,
                    width: px(120.0),
                    height: px(20.0),
                    ..TextLine::default()
                }],
                frame: Bounds::new(Point::default(), Size::new(px(400.0), px(20.0))),
                owner: None,
                // The view is at position 3 (content offset 2), placed
                // after `ab`.
                widths: HashMap::from([(
                    2,
                    Bounds::new(Point::new(px(20.0), px(0.0)), Size::new(px(80.0), px(20.0))),
                )]),
            },
        );
        let x = |pos| {
            geo.caret_rect(pos, Affinity::Downstream)
                .map(|b| b.origin.x.0)
        };
        assert_eq!(x(1), Some(0.0));
        assert_eq!(x(2), Some(10.0));
        assert_eq!(x(3), Some(20.0), "before the view");
        assert_eq!(x(4), Some(100.0), "after the view");
        assert_eq!(x(5), Some(110.0));
        let at = |x: f32| match geo.hit(Point::new(px(x), px(5.0))) {
            Some(Hit::Text(pos, _)) => pos,
            other => panic!("{other:?}"),
        };
        assert_eq!(at(19.0), 3);
        assert_eq!(at(101.0), 4);
        assert_eq!(at(108.0), 5);
    }

    #[test]
    fn an_inline_view_stays_where_the_engine_placed_it() {
        // An engine that reports the line as wide as its box (a test's):
        // the view's own place still fixes the positions around it, so a
        // point on it is not taken for the text before it.
        let mut geo = Geometry {
            measure: Some(Rc::new(|_: &str, t: &str| {
                px(10.0 * t.chars().count() as f32)
            })),
            ..Geometry::default()
        };
        let run = |doc, doc_len, text, text_len| Run {
            doc,
            doc_len,
            text,
            text_len,
            side: 0,
        };
        geo.frames.insert(
            7,
            (
                Some(DOC),
                Bounds::new(Point::default(), Size::new(px(358.0), px(17.0))),
            ),
        );
        geo.texts.insert(
            7,
            TextGeo {
                start: 1,
                map: InlineMap {
                    runs: vec![run(0, 4, 0, 4), run(4, 1, 4, 1), run(5, 4, 5, 4)],
                    size: 9,
                    len: 9,
                },
                lines: vec![TextLine {
                    text: "Buy \u{FFFC} now".into(),
                    start: 0,
                    end: 9,
                    width: px(358.0),
                    height: px(17.0),
                    ..TextLine::default()
                }],
                frame: Bounds::new(Point::default(), Size::new(px(358.0), px(17.0))),
                owner: None,
                widths: HashMap::from([(
                    4,
                    Bounds::new(
                        Point::new(px(31.0), px(0.0)),
                        Size::new(px(105.0), px(17.0)),
                    ),
                )]),
            },
        );
        let x = |pos| {
            geo.caret_rect(pos, Affinity::Downstream)
                .map(|b| b.origin.x.0)
        };
        assert_eq!(x(5), Some(31.0), "before the view");
        assert_eq!(x(6), Some(136.0), "after the view");
        assert_eq!(
            geo.range_rects(5, 6)
                .first()
                .map(|b| (b.origin.x.0, b.size.width.0)),
            Some((31.0, 105.0))
        );
        match geo.hit(Point::new(px(60.0), px(8.0))) {
            Some(Hit::Text(pos, _)) => assert!(pos == 5 || pos == 6, "{pos}"),
            other => panic!("{other:?}"),
        }
    }

    fn map() -> InlineMap {
        // "ab", an atom, a widget before "cd".
        InlineMap {
            runs: vec![
                Run {
                    doc: 0,
                    doc_len: 2,
                    text: 0,
                    text_len: 2,
                    side: 0,
                },
                Run {
                    doc: 2,
                    doc_len: 1,
                    text: 2,
                    text_len: 1,
                    side: 0,
                },
                Run {
                    doc: 3,
                    doc_len: 0,
                    text: 3,
                    text_len: 1,
                    side: -1,
                },
                Run {
                    doc: 3,
                    doc_len: 2,
                    text: 4,
                    text_len: 2,
                    side: 0,
                },
            ],
            size: 5,
            len: 6,
        }
    }

    #[test]
    fn positions_and_text_offsets_map_both_ways() {
        let m = map();
        assert_eq!(m.to_text(0), 0);
        assert_eq!(m.to_text(2), 2);
        // After the widget that sticks before position 3.
        assert_eq!(m.to_text(3), 4);
        assert_eq!(m.to_text(5), 6);
        assert_eq!(m.to_doc(3), 3);
        assert_eq!(m.to_doc(5), 4);
        assert_eq!(m.to_doc(6), 5);
    }

    #[test]
    fn a_leaf_drawn_as_text_is_one_position() {
        // "ab", a mention drawn as "@Alice", "c".
        let run = |doc, doc_len, text, text_len| Run {
            doc,
            doc_len,
            text,
            text_len,
            side: 0,
        };
        let m = InlineMap {
            runs: vec![run(0, 2, 0, 2), run(2, 1, 2, 6), run(3, 1, 8, 1)],
            size: 4,
            len: 9,
        };
        assert_eq!(m.to_text(2), 2);
        assert_eq!(m.to_text(3), 8);
        // Inside its text: the nearer side.
        assert_eq!(m.to_doc(4), 2);
        assert_eq!(m.to_doc(5), 3);
        assert_eq!(m.to_doc(8), 3);
    }
}
