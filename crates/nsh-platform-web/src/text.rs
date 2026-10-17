//! Text layout for the web: line breaking runs here, widths come from the
//! browser (`CanvasRenderingContext2D.measureText`, the `nsh.measure_text`
//! import), and the DOM draws the result run by run at the positions laid
//! out here, so what was measured is what is drawn (the browser never
//! wraps a run itself).
//!
//! Lines break after spaces, around CJK characters and at `\n`; a word
//! longer than the line breaks between characters. That is a subset of
//! UAX #14, enough for app text.

use std::{cell::RefCell, sync::Arc};

use nsh_core::{
    AttributedText, EllipsizeMode, ResolvedTextStyle, Size, TextAlign, TextEngine, TextLayout,
    TextLine,
};
use rustc_hash::FxHashMap;

use crate::ffi;

/// The system font stack (what `fontFamily` unset means).
const SYSTEM_FONT: &str = "-apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, \
                           \"Helvetica Neue\", Arial, \"PingFang SC\", \"Noto Sans SC\", \
                           \"Microsoft YaHei\", sans-serif";

/// The placeholder character of an inline attachment.
const OBJECT: char = '\u{fffc}';

/// The CSS `font` shorthand of a style.
pub(crate) fn css_font(style: &ResolvedTextStyle) -> String {
    let family = match style.font_family {
        Some(f) => format!("\"{}\", {SYSTEM_FONT}", f.name()),
        None => SYSTEM_FONT.to_owned(),
    };
    format!(
        "{}{} {}px {family}",
        if style.italic { "italic " } else { "" },
        style.font_weight,
        style.font_size
    )
}

#[derive(Default)]
pub(crate) struct WebTextEngine {
    /// Width of `(font, text)`.
    widths: RefCell<FxHashMap<(Arc<str>, String), f32>>,
    /// Ascent and descent of a font.
    metrics: RefCell<FxHashMap<Arc<str>, (f32, f32)>>,
}

impl WebTextEngine {
    /// Forgets the measurements: a font finished loading, so what was
    /// measured with a fallback font is stale.
    pub(crate) fn clear_caches(&self) {
        self.widths.borrow_mut().clear();
        self.metrics.borrow_mut().clear();
    }

    fn width(&self, font: &Arc<str>, text: &str) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let key = (font.clone(), text.to_owned());
        if let Some(&w) = self.widths.borrow().get(&key) {
            return w;
        }
        let w = ffi::measure_text(font, text);
        let mut widths = self.widths.borrow_mut();
        // Bounded: a long-running app measures many distinct strings.
        if widths.len() > 20_000 {
            widths.clear();
        }
        widths.insert(key, w);
        w
    }

    fn metrics(&self, font: &Arc<str>, size: f32) -> (f32, f32) {
        if let Some(&m) = self.metrics.borrow().get(font) {
            return m;
        }
        let (ascent, descent) = ffi::font_metrics(font);
        let m = if ascent > 0.0 {
            (ascent, descent)
        } else {
            (size * 0.93, size * 0.24)
        };
        self.metrics.borrow_mut().insert(font.clone(), m);
        m
    }
}

/// A style of a prepared text: its CSS font and metrics.
#[derive(Debug, Clone)]
pub(crate) struct RunStyle {
    pub(crate) style: ResolvedTextStyle,
    pub(crate) font: Arc<str>,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
}

impl RunStyle {
    /// The height of a line of this style alone, and where the glyphs'
    /// top sits in it (CSS half-leading).
    fn line(&self) -> (f32, f32) {
        let natural = self.ascent + self.descent;
        if self.style.line_height > 0.0 {
            let h = self.style.line_height;
            (h, (h - natural) / 2.0)
        } else {
            (natural, 0.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AtomKind {
    /// Text; the line may break after it.
    Text,
    /// Spaces (do not count at the end of a line).
    Space,
    /// A forced line break.
    Newline,
    /// An inline attachment (`width` × `height`).
    Attachment(f32, f32),
}

/// A piece of text that is never broken across lines (except words wider
/// than a line).
#[derive(Debug, Clone)]
struct Atom {
    text: String,
    style: u16,
    width: f32,
    kind: AtomKind,
    /// UTF-16 offsets in the text.
    start: u32,
    end: u32,
}

/// What [`WebTextEngine::prepare`] makes.
#[derive(Debug)]
pub(crate) struct Prepared {
    atoms: Vec<Atom>,
    styles: Arc<[RunStyle]>,
    align: TextAlign,
    max_lines: u32,
    ellipsize: EllipsizeMode,
    no_wrap: bool,
    rtl: bool,
}

/// One drawn run.
#[derive(Debug, Clone)]
pub(crate) struct Run {
    pub(crate) x: f32,
    /// Top of the glyph box (ascent above the baseline).
    pub(crate) y: f32,
    pub(crate) text: String,
    pub(crate) style: u16,
}

/// A final layout.
#[derive(Debug)]
pub(crate) struct WebLayout {
    pub(crate) runs: Vec<Run>,
    pub(crate) styles: Arc<[RunStyle]>,
    lines: Vec<TextLine>,
    attachments: Vec<Option<(f32, f32)>>,
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF |
        0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

struct LineBox {
    /// Atoms `start..end`.
    start: usize,
    end: usize,
    /// Width without trailing spaces.
    width: f32,
    height: f32,
    /// The largest ascent (with half-leading) above the baseline.
    above: f32,
}

impl WebTextEngine {
    /// Lines of `p` at `max_width` (unbounded: `None`).
    fn break_lines(&self, p: &Prepared, max_width: Option<f32>) -> (Vec<Atom>, Vec<LineBox>) {
        let max = if p.no_wrap { None } else { max_width };
        // Words wider than a line are split into characters first.
        let mut atoms: Vec<Atom> = Vec::with_capacity(p.atoms.len());
        for atom in &p.atoms {
            match max {
                Some(max) if atom.kind == AtomKind::Text && atom.width > max + 0.01 => {
                    let font = &p.styles[atom.style as usize].font;
                    let mut at = atom.start;
                    for c in atom.text.chars() {
                        let s = c.to_string();
                        let len = c.len_utf16() as u32;
                        atoms.push(Atom {
                            width: self.width(font, &s)
                                + p.styles[atom.style as usize].style.letter_spacing,
                            text: s,
                            style: atom.style,
                            kind: AtomKind::Text,
                            start: at,
                            end: at + len,
                        });
                        at += len;
                    }
                }
                _ => atoms.push(atom.clone()),
            }
        }
        let mut lines = Vec::new();
        let mut line = LineBox {
            start: 0,
            end: 0,
            width: 0.0,
            height: 0.0,
            above: 0.0,
        };
        // Width including trailing spaces.
        let mut advance = 0.0f32;
        let finish = |line: &mut LineBox, atoms: &[Atom], end: usize, lines: &mut Vec<LineBox>| {
            line.end = end;
            // Metrics of the styles on the line (of the previous atom's
            // style for an empty line).
            let mut height = 0.0f32;
            let mut above = 0.0f32;
            let range = if line.start < end {
                line.start..end
            } else {
                end.saturating_sub(1)..end.min(atoms.len())
            };
            for atom in &atoms[range] {
                let s = &p.styles[atom.style as usize];
                let (h, lead) = s.line();
                height = height.max(h);
                above = above.max(lead + s.ascent);
                if let AtomKind::Attachment(_, ah) = atom.kind {
                    above = above.max(ah);
                    height = height.max(ah);
                }
            }
            if height == 0.0 && !p.styles.is_empty() {
                let s = &p.styles[0];
                let (h, lead) = s.line();
                height = h;
                above = lead + s.ascent;
            }
            line.height = height;
            line.above = above;
            lines.push(LineBox {
                start: line.start,
                end,
                width: line.width,
                height,
                above,
            });
            line.start = end;
            line.width = 0.0;
        };
        let mut i = 0;
        while i < atoms.len() {
            let atom = &atoms[i];
            match atom.kind {
                AtomKind::Newline => {
                    finish(&mut line, &atoms, i, &mut lines);
                    // The newline belongs to no line.
                    line.start = i + 1;
                    advance = 0.0;
                }
                AtomKind::Space => {
                    advance += atom.width;
                }
                AtomKind::Text | AtomKind::Attachment(..) => {
                    let w = match atom.kind {
                        AtomKind::Attachment(w, _) => w,
                        _ => atom.width,
                    };
                    if let Some(max) = max
                        && line.start < i
                        && advance + w > max + 0.01
                    {
                        finish(&mut line, &atoms, i, &mut lines);
                        advance = 0.0;
                    }
                    advance += w;
                    line.width = advance;
                }
            }
            i += 1;
        }
        finish(&mut line, &atoms, atoms.len(), &mut lines);
        (atoms, lines)
    }
}

impl TextEngine for WebTextEngine {
    type Prepared = Prepared;

    fn prepare(&self, text: &AttributedText) -> Prepared {
        let mut styles: Vec<RunStyle> = Vec::with_capacity(text.spans.len());
        let mut atoms = Vec::new();
        // UTF-16 offset -> attachment size.
        let attachments: FxHashMap<u32, (f32, f32)> = text
            .attachments
            .iter()
            .map(|a| (a.index, (a.width, a.height)))
            .collect();
        let units: Vec<u16> = text.text.encode_utf16().collect();
        for span in &text.spans {
            let style = span.style;
            let font: Arc<str> = css_font(&style).into();
            let (ascent, descent) = self.metrics(&font, style.font_size);
            let index = styles.len() as u16;
            styles.push(RunStyle {
                style,
                font: font.clone(),
                ascent,
                descent,
            });
            let start = span.start as usize;
            let end = (span.end as usize).min(units.len());
            if start >= end {
                continue;
            }
            let s = String::from_utf16_lossy(&units[start..end]);
            let spacing = style.letter_spacing;
            let mut at = span.start;
            let mut word = String::new();
            let mut word_start = at;
            let push = |atoms: &mut Vec<Atom>, text: String, kind, start, end| {
                let width = match kind {
                    AtomKind::Text | AtomKind::Space => {
                        self.width(&font, &text) + spacing * text.chars().count() as f32
                    }
                    // Its placeholder's room on the line.
                    AtomKind::Attachment(w, _) => w,
                    AtomKind::Newline => 0.0,
                };
                atoms.push(Atom {
                    text,
                    style: index,
                    width,
                    kind,
                    start,
                    end,
                });
            };
            let mut chars = s.chars().peekable();
            while let Some(c) = chars.next() {
                let len = c.len_utf16() as u32;
                let attachment = (c == OBJECT).then(|| attachments.get(&at)).flatten();
                if c == '\n' || c == ' ' || c == '\t' || is_cjk(c) || attachment.is_some() {
                    if !word.is_empty() {
                        push(
                            &mut atoms,
                            std::mem::take(&mut word),
                            AtomKind::Text,
                            word_start,
                            at,
                        );
                    }
                    if c == '\n' {
                        push(&mut atoms, String::new(), AtomKind::Newline, at, at + len);
                    } else if let Some(&(w, h)) = attachment {
                        push(
                            &mut atoms,
                            String::new(),
                            AtomKind::Attachment(w, h),
                            at,
                            at + len,
                        );
                    } else if is_cjk(c) {
                        push(&mut atoms, c.to_string(), AtomKind::Text, at, at + len);
                    } else {
                        // A run of spaces is one atom.
                        let mut spaces = c.to_string();
                        let mut end = at + len;
                        while let Some(&n) = chars.peek()
                            && (n == ' ' || n == '\t')
                        {
                            spaces.push(n);
                            end += 1;
                            chars.next();
                        }
                        push(&mut atoms, spaces, AtomKind::Space, at, end);
                        at = end;
                        word_start = at;
                        continue;
                    }
                    at += len;
                    word_start = at;
                } else {
                    if word.is_empty() {
                        word_start = at;
                    }
                    word.push(c);
                    at += len;
                    // A hyphen ends a word (the line may break after it).
                    if c == '-' && chars.peek().is_some_and(|n| n.is_alphanumeric()) {
                        push(
                            &mut atoms,
                            std::mem::take(&mut word),
                            AtomKind::Text,
                            word_start,
                            at,
                        );
                        word_start = at;
                    }
                }
            }
            if !word.is_empty() {
                push(&mut atoms, word, AtomKind::Text, word_start, at);
            }
        }
        if styles.is_empty() {
            let style = ResolvedTextStyle::default();
            let font: Arc<str> = css_font(&style).into();
            let (ascent, descent) = self.metrics(&font, style.font_size);
            styles.push(RunStyle {
                style,
                font,
                ascent,
                descent,
            });
        }
        Prepared {
            atoms,
            styles: styles.into(),
            align: text.align,
            max_lines: text.max_lines,
            ellipsize: text.ellipsize,
            no_wrap: text.no_wrap,
            rtl: text.writing_direction.unwrap_or(text.rtl),
        }
    }

    fn measure(&self, prepared: &Prepared, max_width: Option<f32>) -> Size {
        let (_, lines) = self.break_lines(prepared, max_width);
        let shown = match prepared.max_lines {
            0 => lines.len(),
            n => lines.len().min(n as usize),
        };
        let lines = &lines[..shown];
        let width = lines.iter().fold(0.0f32, |w, l| w.max(l.width));
        let height: f32 = lines.iter().map(|l| l.height).sum();
        let width = match max_width {
            Some(max) => width.min(max),
            None => width,
        };
        Size::new(width.ceil(), height.ceil())
    }

    fn layout(&self, prepared: &Prepared, size: Size) -> TextLayout {
        let (atoms, mut lines) = self.break_lines(prepared, Some(size.width));
        let mut truncated = false;
        if prepared.max_lines > 0 && lines.len() > prepared.max_lines as usize {
            lines.truncate(prepared.max_lines as usize);
            truncated = true;
        }
        let styles = prepared.styles.clone();
        let mut runs: Vec<Run> = Vec::new();
        let mut text_lines = Vec::with_capacity(lines.len());
        let mut attachments = vec![
            None;
            atoms
                .iter()
                .filter(|a| matches!(a.kind, AtomKind::Attachment(..)))
                .count()
        ];
        let mut attachment_index = 0;
        let mut y = 0.0f32;
        let count = lines.len();
        for (n, line) in lines.iter().enumerate() {
            let last = n + 1 == count;
            // Atoms of the line, without trailing spaces.
            let mut end = line.end;
            while end > line.start && atoms[end - 1].kind == AtomKind::Space {
                end -= 1;
            }
            let mut pieces: Vec<(String, u16, f32, AtomKind)> = atoms[line.start..end]
                .iter()
                .map(|a| (a.text.clone(), a.style, a.width, a.kind))
                .collect();
            let mut width = line.width;
            if last && truncated && prepared.ellipsize != EllipsizeMode::Clip {
                let style = pieces.last().map_or(0, |p| p.1);
                let font = &styles[style as usize].font;
                let ellipsis = self.width(font, "…");
                // Drop characters from the end until the ellipsis fits.
                while width + ellipsis > size.width + 0.01 && !pieces.is_empty() {
                    let (text, style, w, kind) = pieces.pop().expect("not empty");
                    width -= w;
                    if kind == AtomKind::Text && text.chars().count() > 1 {
                        let mut t = text;
                        t.pop();
                        let f = &styles[style as usize].font;
                        let nw = self.width(f, &t);
                        width += nw;
                        pieces.push((t, style, nw, kind));
                    }
                }
                // Trailing spaces before the ellipsis go.
                while pieces.last().is_some_and(|p| p.3 == AtomKind::Space) {
                    width -= pieces.pop().expect("not empty").2;
                }
                pieces.push(("…".to_owned(), style, ellipsis, AtomKind::Text));
                width += ellipsis;
            }
            let free = (size.width - width).max(0.0);
            let align = match prepared.align {
                TextAlign::Auto | TextAlign::Justify if prepared.rtl => TextAlign::Right,
                TextAlign::Auto | TextAlign::Justify => TextAlign::Left,
                a => a,
            };
            let mut x = match align {
                TextAlign::Center => free / 2.0,
                TextAlign::Right => free,
                _ => 0.0,
            };
            let x0 = x;
            let baseline = y + line.above;
            // Neighbouring pieces of one style draw as one run.
            let mut merge = false;
            for (text, style, w, kind) in pieces {
                if let AtomKind::Attachment(_, h) = kind {
                    if let Some(slot) = attachments.get_mut(attachment_index) {
                        *slot = Some((x, baseline - h));
                    }
                    attachment_index += 1;
                    x += w;
                    merge = false;
                    continue;
                }
                let s = &styles[style as usize];
                match runs.last_mut() {
                    Some(r) if merge && r.style == style => r.text.push_str(&text),
                    _ => runs.push(Run {
                        x,
                        y: baseline - s.ascent,
                        text,
                        style,
                    }),
                }
                merge = true;
                x += w;
            }
            let first = &styles[atoms.get(line.start).map_or(0, |a| a.style) as usize];
            text_lines.push(TextLine {
                text: String::new(),
                start: atoms.get(line.start).map_or(0, |a| a.start),
                end: atoms.get(line.end.saturating_sub(1)).map_or(0, |a| a.end),
                x: x0,
                y,
                width,
                height: line.height,
                ascender: first.ascent,
                descender: first.descent,
                cap_height: first.style.font_size * 0.7,
                x_height: first.style.font_size * 0.5,
            });
            y += line.height;
        }
        TextLayout(Arc::new(WebLayout {
            runs,
            styles,
            lines: text_lines,
            attachments,
        }))
    }

    fn lines(&self, layout: &TextLayout) -> Vec<TextLine> {
        layout
            .downcast::<WebLayout>()
            .map_or_else(Vec::new, |l| l.lines.clone())
    }

    fn attachments(&self, layout: &TextLayout) -> Vec<Option<(f32, f32)>> {
        layout
            .downcast::<WebLayout>()
            .map_or_else(Vec::new, |l| l.attachments.clone())
    }

    fn register_font(&self, data: &[u8]) -> Result<(), String> {
        let face = ttf_parser::Face::parse(data, 0).map_err(|e| e.to_string())?;
        let name = |id| {
            face.names()
                .into_iter()
                .filter(|n| n.name_id == id && n.is_unicode())
                .find_map(|n| n.to_string())
        };
        let family = name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
            .or_else(|| name(ttf_parser::name_id::FAMILY))
            .ok_or("the font has no family name")?;
        ffi::register_font(&family, data, face.weight().to_number(), face.is_italic());
        // Widths measured with a fallback font are stale now (and again
        // when the face finishes loading: `nsh_fonts_loaded`).
        self.clear_caches();
        Ok(())
    }
}
