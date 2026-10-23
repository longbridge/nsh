//! Hit testing in a laid-out text: which offset a point is at, where the
//! caret of an offset stands and which boxes a range covers. It works on a
//! layout's lines and the places the caret can stand on each ([`Stop`]),
//! and depends on nothing of the browser, so its tests run off WebAssembly
//! too (`tests/text`).
//!
//! The layout places every piece in logical order from the line's left
//! edge (the browser draws a run as one piece and does not move it), so
//! the stops of a line are in logical and visual order at once.

use std::ops::Range;

use nsh_core::{
    Affinity, Bounds, Point, Points, TextLine, TextPosition,
    geometry::{point, size},
    text_edit::next_cluster,
};

/// A place the caret can stand on a line: between two clusters, at the
/// start or at the end of the drawn text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Stop {
    /// The offset (UTF-16 units).
    pub(crate) offset: u32,
    pub(crate) x: f32,
}

/// Appends the stops of one piece of a line, drawn at `x` and `width`
/// wide, whose first character is at offset `start`: its start, the
/// boundaries between its clusters (`prefix` measures the text before
/// one, letter spacing included) and its end. `text` is `None` for an
/// attachment, one unit that no stop falls inside. A stop at an offset the
/// line has one at already (where two pieces meet) is left out.
pub(crate) fn push_stops(
    stops: &mut Vec<Stop>,
    x: f32,
    width: f32,
    start: u32,
    text: Option<&str>,
    mut prefix: impl FnMut(&str) -> f32,
) {
    let mut push = |offset: u32, x: f32| {
        if stops.last().is_none_or(|s| s.offset < offset) {
            stops.push(Stop { offset, x });
        }
    };
    let end_x = x + width;
    push(start, x);
    let Some(text) = text else {
        push(start.saturating_add(1), end_x);
        return;
    };
    let mut at = 0;
    let mut offset = start;
    while at < text.len() {
        let next = next_cluster(text, at);
        let (Some(cluster), true) = (text.get(at..next), next > at) else {
            break;
        };
        let units = u32::try_from(cluster.encode_utf16().count()).unwrap_or(u32::MAX);
        offset = offset.saturating_add(units);
        match text.get(..next) {
            // The piece's own width ends it, whatever its prefixes add up
            // to; a prefix never goes past it.
            Some(head) if next < text.len() => push(offset, (x + prefix(head)).min(end_x)),
            _ => push(offset, end_x),
        }
        at = next;
    }
}

/// The x of the caret at `offset` on a line with `stops`: its stop, or the
/// one before it where it stands inside a cluster or in the spaces a
/// wrap dropped from the line's end. `line_x` for a line with none.
fn x_of(stops: &[Stop], offset: u32, line_x: f32) -> f32 {
    let after = stops.partition_point(|s| s.offset <= offset);
    after
        .checked_sub(1)
        .and_then(|i| stops.get(i))
        .or_else(|| stops.first())
        .map_or(line_x, |s| s.x)
}

/// The x where the drawn text of a line ends.
fn end_x(stops: &[Stop], line_x: f32) -> f32 {
    stops.last().map_or(line_x, |s| s.x)
}

/// The index of the line a point at height `y` lands on: the one at that
/// height, the first above the text, the last below it.
fn line_at(lines: &[TextLine], y: Points) -> Option<usize> {
    lines
        .iter()
        .position(|l| y < l.y + l.height)
        .or(lines.len().checked_sub(1))
}

/// The caret position nearest `p` (content box): on the line at its
/// height, at the stop nearest its x. Past the end of a line, the line's
/// end offset: [`Affinity::Upstream`] where the line wrapped (the next line
/// starts there), so the caret stays on this line, and before the break
/// character where it ended at one. `stops(i)` are the stops of line `i`.
pub(crate) fn hit_test<'a>(
    lines: &[TextLine],
    stops: impl Fn(usize) -> &'a [Stop],
    p: Point<Points>,
) -> TextPosition {
    let Some(i) = line_at(lines, p.y) else {
        return TextPosition::default();
    };
    let Some(l) = lines.get(i) else {
        return TextPosition::default();
    };
    let stops = stops(i);
    let x = p.x.0;
    if x >= end_x(stops, l.x.0) {
        let wrapped = l.end > l.start && lines.get(i + 1).is_some_and(|next| next.start == l.end);
        let affinity = if wrapped {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        };
        return TextPosition::new(l.end, affinity);
    }
    // The first stop right of `x` exists: `x` is left of the last one.
    let right = stops.partition_point(|s| s.x <= x);
    let left = right.checked_sub(1).and_then(|i| stops.get(i));
    let offset = match (left, stops.get(right)) {
        (Some(left), Some(right)) if x - left.x < right.x - x => left.offset,
        (_, Some(right)) => right.offset,
        (Some(left), None) => left.offset,
        (None, None) => l.start,
    };
    TextPosition::new(offset, Affinity::Downstream)
}

/// The caret at `position`: zero wide, as high as its line, at the stop of
/// the offset. `None` when there are no lines.
pub(crate) fn caret_rect<'a>(
    lines: &[TextLine],
    stops: impl Fn(usize) -> &'a [Stop],
    position: TextPosition,
) -> Option<Bounds<Points>> {
    let i = position.line(lines)?;
    let l = lines.get(i)?;
    let x = x_of(stops(i), position.offset, l.x.0);
    Some(Bounds::new(
        point(Points(x), l.y),
        size(Points::ZERO, l.height),
    ))
}

/// Appends the boxes of the characters of `range`, one per line, line high:
/// from the range's start on the line to its end, or to the end of the
/// line's drawn text where the range goes on past the line. A box with
/// nothing in it (the range covers only spaces a wrap dropped) is left out.
pub(crate) fn range_rects<'a>(
    lines: &[TextLine],
    stops: impl Fn(usize) -> &'a [Stop],
    range: Range<u32>,
    out: &mut Vec<Bounds<Points>>,
) {
    if range.start >= range.end {
        return;
    }
    for (i, l) in lines.iter().enumerate() {
        if l.start >= range.end || range.start >= l.end {
            continue;
        }
        let stops = stops(i);
        let x1 = x_of(stops, range.start, l.x.0);
        let x2 = if range.end > l.end {
            end_x(stops, l.x.0)
        } else {
            x_of(stops, range.end, l.x.0)
        };
        if x2 > x1 {
            out.push(Bounds::new(
                point(Points(x1), l.y),
                size(Points(x2 - x1), l.height),
            ));
        }
    }
}
