use std::sync::Arc;

use nsh_core::{
    AttributedText, EllipsizeMode, Points, Size, TextAlign, TextEngine, TextLayout, TextLine,
};
use ratatui_core::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::CellSize;

/// Text measured on a fixed grid, independent of the requested font size.
#[derive(Default)]
#[non_exhaustive]
pub struct CellTextEngine {
    cells: CellSize,
}

impl CellTextEngine {
    /// Uses the same Cell dimensions as the surface that draws the text.
    pub fn new(cells: CellSize) -> Self {
        Self { cells }
    }
}

#[derive(Clone)]
struct Glyph {
    text: Arc<str>,
    start: u32,
    end: u32,
    width: usize,
    style: Style,
}

/// Opaque text prepared once for the core's repeated layout measurements.
#[derive(Default)]
#[non_exhaustive]
pub struct PreparedText {
    text: AttributedText,
    glyphs: Vec<Glyph>,
}

pub(crate) struct CellLayout {
    pub(crate) paragraph: Paragraph<'static>,
    metrics: Vec<TextLine>,
}

fn glyphs(text: &AttributedText) -> Vec<Glyph> {
    let mut offset = 0;
    text.text
        .graphemes(true)
        .map(|grapheme| {
            let start = offset;
            offset += grapheme.encode_utf16().count() as u32;
            let mut style = Style::default();
            if let Some(span) = text.span_at(start) {
                let color = span.style.color.0;
                style = style.fg(Color::Rgb(
                    (color >> 24) as u8,
                    (color >> 16) as u8,
                    (color >> 8) as u8,
                ));
                if span.style.font_weight >= 600 {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if span.style.italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if span.style.decoration.underline {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if span.style.decoration.line_through {
                    style = style.add_modifier(Modifier::CROSSED_OUT);
                }
            }
            Glyph {
                text: Arc::from(grapheme),
                start,
                end: offset,
                width: grapheme.width(),
                style,
            }
        })
        .collect()
}

fn wrap(glyphs: &[Glyph], columns: usize) -> Vec<Vec<Glyph>> {
    let mut rows = Vec::new();
    let mut start = 0;
    while start < glyphs.len() {
        let mut end = start;
        let mut width = 0;
        let mut space = None;
        while end < glyphs.len() && !glyphs[end].text.contains('\n') {
            if width + glyphs[end].width > columns && end > start {
                break;
            }
            width += glyphs[end].width;
            if glyphs[end].text.as_ref() == " " {
                space = Some(end);
            }
            end += 1;
        }
        let mut next = end;
        if end < glyphs.len() && glyphs[end].text.contains('\n') {
            next += 1;
        } else if end < glyphs.len() && glyphs[end].text.as_ref() == " " {
            while next < glyphs.len() && glyphs[next].text.as_ref() == " " {
                next += 1;
            }
        } else if end < glyphs.len()
            && let Some(space) = space.filter(|&s| s > start)
        {
            end = space;
            next = space + 1;
        }
        rows.push(glyphs[start..end].to_vec());
        start = next;
    }
    if glyphs.is_empty() || glyphs.last().is_some_and(|g| g.text.contains('\n')) {
        rows.push(Vec::new());
    }
    rows
}

impl CellTextEngine {
    fn rows(&self, prepared: &PreparedText, width: Option<Points>) -> Vec<Vec<Glyph>> {
        let text = &prepared.text;
        let columns = if text.no_wrap {
            usize::MAX
        } else {
            width.map_or(usize::MAX, |w| {
                ((w / self.cells.width() + 0.01).floor() as usize).max(1)
            })
        };
        let glyphs = &prepared.glyphs;
        let mut rows = wrap(glyphs, columns);
        if text.max_lines > 0 {
            let limit = text.max_lines as usize;
            let truncated = rows.len() > limit;
            rows.truncate(limit);
            if truncated
                && matches!(text.ellipsize, EllipsizeMode::Head | EllipsizeMode::Middle)
                && let Some(row) = rows.last_mut()
            {
                let start = row.first().map_or(0, |glyph| glyph.start);
                *row = glyphs
                    .iter()
                    .filter(|glyph| glyph.start >= start && !glyph.text.contains('\n'))
                    .cloned()
                    .collect();
            }
            let available = width.map_or(usize::MAX, |w| {
                (w / self.cells.width() + 0.01).floor() as usize
            });
            if let Some(row) = rows.last_mut()
                && (truncated || row.iter().map(|g| g.width).sum::<usize>() > available)
            {
                ellipsize(row, available, text.ellipsize);
            }
        }
        rows
    }
}

impl TextEngine for CellTextEngine {
    type Prepared = PreparedText;

    fn prepare(&self, text: &AttributedText) -> Self::Prepared {
        PreparedText {
            text: text.clone(),
            glyphs: glyphs(text),
        }
    }

    fn measure(&self, text: &Self::Prepared, width: Option<Points>) -> Size<Points> {
        let rows = self.rows(text, width);
        let columns = rows
            .iter()
            .map(|r| r.iter().map(|g| g.width).sum::<usize>())
            .max()
            .unwrap_or(0);
        Size::new(
            self.cells.width() * columns as f32,
            self.cells.height() * rows.len() as f32,
        )
    }

    fn layout(&self, text: &Self::Prepared, size: Size<Points>) -> TextLayout {
        let rows = self.rows(text, Some(size.width));
        let glyphs = &text.glyphs;
        let text = &text.text;
        let mut lines = Vec::new();
        let mut metrics = Vec::new();
        let mut offset = 0;
        for (index, row) in rows.into_iter().enumerate() {
            let width = row.iter().map(|g| g.width).sum::<usize>();
            let start = row.first().map_or(offset, |g| g.start);
            let end = row.last().map_or(start, |g| g.end);
            offset = glyphs
                .binary_search_by_key(&end, |glyph| glyph.start)
                .ok()
                .and_then(|index| glyphs.get(index))
                .filter(|glyph| glyph.text.contains('\n'))
                .map_or(end, |glyph| glyph.end);
            let columns = (size.width / self.cells.width() + 0.01).floor() as usize;
            let spare = columns.saturating_sub(width);
            let indent = match text.align {
                TextAlign::Center => spare / 2,
                TextAlign::Right => spare,
                TextAlign::Auto if text.rtl => spare,
                _ => 0,
            };
            metrics.push(TextLine {
                text: String::new(),
                start,
                end,
                x: self.cells.width() * indent as f32,
                y: self.cells.height() * index as f32,
                width: self.cells.width() * width as f32,
                height: self.cells.height(),
                ascender: self.cells.height(),
                descender: Points::ZERO,
                cap_height: self.cells.height(),
                x_height: self.cells.height(),
            });
            let mut spans = Vec::with_capacity(row.len() + 1);
            if indent > 0 {
                spans.push(Span::raw(" ".repeat(indent)));
            }
            spans.extend(
                row.into_iter()
                    .map(|g| Span::styled(g.text.to_string(), g.style)),
            );
            lines.push(Line::from(spans));
        }
        TextLayout(Arc::new(CellLayout {
            paragraph: Paragraph::new(lines),
            metrics,
        }))
    }

    fn lines(&self, layout: &TextLayout) -> Vec<TextLine> {
        layout
            .downcast::<CellLayout>()
            .map_or_else(Vec::new, |l| l.metrics.clone())
    }

    fn register_font(&self, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

fn ellipsize(row: &mut Vec<Glyph>, columns: usize, mode: EllipsizeMode) {
    let marker = usize::from(mode != EllipsizeMode::Clip && columns > 0);
    let budget = columns.saturating_sub(marker);
    let original = std::mem::take(row);
    let mut left = 0;
    let mut right = original.len();
    let mut used = 0;
    let left_budget = match mode {
        EllipsizeMode::Head => 0,
        EllipsizeMode::Middle => budget.div_ceil(2),
        _ => budget,
    };
    while left < original.len() && used + original[left].width <= left_budget {
        used += original[left].width;
        left += 1;
    }
    if matches!(mode, EllipsizeMode::Head | EllipsizeMode::Middle) {
        while right > left && used + original[right - 1].width <= budget {
            right -= 1;
            used += original[right].width;
        }
    }
    row.extend_from_slice(&original[..left]);
    if marker > 0 {
        let at = original.get(left).or_else(|| original.last());
        let offset = original
            .get(left)
            .map_or_else(|| original.last().map_or(0, |g| g.end), |g| g.start);
        row.push(Glyph {
            text: "…".into(),
            start: offset,
            end: offset,
            width: 1,
            style: at.map_or_else(Style::default, |g| g.style),
        });
    }
    row.extend_from_slice(&original[right..]);
}
