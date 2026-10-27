//! Terminal presentation and event protocol around the core's text editor.

use nsh_core::{
    Bounds, EventKind, EventPayload, InputConfig, NodeId, PayloadValue, Points, SubmitBehavior,
    TextAlign,
    text_edit::{self, Model, Unit},
};
use nsh_engine::EngineControl;
use ratatui_core::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};
use ratatui_widgets::paragraph::Paragraph;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{geometry::CellBounds, key_scheme::EditAction};

#[derive(Default)]
pub(crate) struct Field {
    pub(crate) model: Model,
    pub(crate) content: Bounds<Points>,
    pub(crate) focused: bool,
    count: u32,
    reported: (u32, u32),
    scroll: (usize, usize),
    goal_column: Option<usize>,
    bounds: Option<CellBounds>,
    visible: Rect,
    cursor: Option<Position>,
    columns: usize,
    layout_key: Option<(usize, bool, bool, TextAlign, bool)>,
    rows: Vec<Row>,
}

#[derive(Default)]
struct Row {
    start: usize,
    end: usize,
    width: usize,
    indent: usize,
    glyphs: Vec<Glyph>,
}

struct Glyph {
    start: usize,
    end: usize,
    text: String,
    width: usize,
}

impl Field {
    pub(crate) fn set_value(&mut self, text: &str, count: u32) {
        if count >= self.count && text != self.model.text() {
            self.model.set_text(text);
            self.layout_key = None;
            self.goal_column = None;
        }
    }

    pub(crate) fn select(&mut self, start: u32, end: u32) {
        let text = self.model.text();
        self.model.select(
            text_edit::utf16_to_byte(text, start),
            text_edit::utf16_to_byte(text, end),
        );
        self.goal_column = None;
    }

    pub(crate) fn report(&mut self, id: NodeId, control: &EngineControl, changed: bool) {
        if changed {
            self.layout_key = None;
            self.count = self.count.wrapping_add(1);
            control.dispatch_event(
                id,
                EventKind::ChangeText,
                EventPayload::Text {
                    text: self.model.text().to_owned(),
                    count: self.count,
                },
            );
        }
        let (start, end) = self.model.selection();
        let selection = (
            text_edit::byte_to_utf16(self.model.text(), start),
            text_edit::byte_to_utf16(self.model.text(), end),
        );
        if selection != self.reported {
            self.reported = selection;
            control.dispatch_event(
                id,
                EventKind::SelectionChange,
                EventPayload::Map(vec![
                    ("start", PayloadValue::Number(selection.0 as f64)),
                    ("end", PayloadValue::Number(selection.1 as f64)),
                ]),
            );
        }
    }

    pub(crate) fn insert(
        &mut self,
        config: &InputConfig,
        text: &str,
        typed: bool,
        id: NodeId,
        control: &EngineControl,
    ) {
        if !config.editable {
            return;
        }
        let inserted = self.model.insert(text, config.max_length);
        if typed
            && let Some(text) = &inserted
            && !text.is_empty()
        {
            key_press(id, control, text_edit::key_of(text));
        }
        self.goal_column = None;
        self.report(id, control, inserted.is_some());
    }

    pub(crate) fn paste(
        &mut self,
        config: &InputConfig,
        text: &str,
        id: NodeId,
        control: &EngineControl,
    ) {
        if !config.editable {
            return;
        }
        let mut event = nsh_core::PasteEvent::default();
        event.text = text.to_owned();
        control.dispatch_event(id, EventKind::Paste, EventPayload::Paste(Box::new(event)));
        if config.paste_behavior == nsh_core::PasteBehavior::Insert {
            self.insert(
                config,
                &text_edit::pasted(text, config.multiline),
                false,
                id,
                control,
            );
        }
    }

    /// Returns whether this action leaves the field.
    pub(crate) fn edit(
        &mut self,
        action: EditAction,
        extend: bool,
        config: &InputConfig,
        kill: &mut String,
        clipboard: &dyn nsh_core::clipboard::ClipboardBackend,
        id: NodeId,
        control: &EngineControl,
    ) -> bool {
        use EditAction::*;
        let mut changed = false;
        let mut leave = false;
        if !matches!(action, Vertical(_)) {
            self.goal_column = None;
        }
        match action {
            Move(unit, forward) => self.model.step(unit, forward, extend),
            ReadlineWord(forward) => {
                let to = readline_boundary(&self.model, forward, false);
                self.model.move_to(to, extend);
            }
            Boundary(end) => {
                let (start, finish) = self.model.line_at(self.model.caret());
                self.model.move_to(if end { finish } else { start }, extend);
            }
            Document(end) => self
                .model
                .move_to(if end { self.model.text().len() } else { 0 }, extend),
            Vertical(forward) => {
                self.layout(config);
                let (column, row) = self.caret_cell();
                let column = *self.goal_column.get_or_insert(column);
                let target = if forward {
                    (row + 1).min(self.rows.len() - 1)
                } else {
                    row.saturating_sub(1)
                };
                let at = self.at_cell(column, target);
                self.model.move_to(at, extend);
            }
            Delete(unit, forward) => {
                changed = self.model.delete(unit, forward);
                if !forward {
                    key_press(id, control, "Backspace");
                }
            }
            KillLine(end) => {
                if !self.model.has_selection() {
                    let caret = self.model.caret();
                    let (start, finish) = self.model.line_at(caret);
                    let to = if end {
                        if caret == finish {
                            self.model.boundary(caret, Unit::Char, true)
                        } else {
                            finish
                        }
                    } else {
                        start
                    };
                    self.model.select(caret, to);
                }
                if self.model.has_selection() {
                    *kill = self.model.selected().to_owned();
                    changed = self.model.delete(Unit::Char, false);
                }
            }
            KillWord(_) | KillWhitespaceWord => {
                if !self.model.has_selection() {
                    let caret = self.model.caret();
                    let to = readline_boundary(
                        &self.model,
                        matches!(action, KillWord(true)),
                        matches!(action, KillWhitespaceWord),
                    );
                    self.model.select(caret, to);
                }
                if self.model.has_selection() {
                    *kill = self.model.selected().to_owned();
                    changed = self.model.delete(Unit::Char, false);
                }
            }
            Yank => changed = self.model.insert(kill, config.max_length).is_some(),
            Transpose => {
                let caret = self.model.caret();
                if !self.model.has_selection() && caret > 0 {
                    let middle = if caret == self.model.text().len() {
                        self.model.boundary(caret, Unit::Char, false)
                    } else {
                        caret
                    };
                    let start = self.model.boundary(middle, Unit::Char, false);
                    let end = self.model.boundary(middle, Unit::Char, true);
                    if start < middle
                        && middle < end
                        && !self.model.text()[start..end].contains('\n')
                    {
                        let swapped = format!(
                            "{}{}",
                            &self.model.text()[middle..end],
                            &self.model.text()[start..middle]
                        );
                        self.model.select(start, end);
                        changed = self.model.insert(&swapped, config.max_length).is_some();
                    }
                }
            }
            SelectAll => self.model.select_all(),
            Copy | Cut => {
                if !config.secure && self.model.has_selection() {
                    clipboard.set_text(Some(self.model.selected().to_owned()));
                    if matches!(action, Cut) {
                        changed = self.model.delete(Unit::Char, false);
                    }
                }
            }
            Paste => {
                if let Some(text) = clipboard.text() {
                    self.paste(config, &text, id, control);
                }
                return false;
            }
            Undo => changed = self.model.undo(),
            Redo => changed = self.model.redo(),
            Enter(newline) => {
                if config.multiline && (newline || config.submit() == SubmitBehavior::Newline) {
                    self.insert(config, "\n", true, id, control);
                    return false;
                }
                key_press(id, control, "Enter");
                control.dispatch_event(
                    id,
                    EventKind::SubmitEditing,
                    EventPayload::Text {
                        text: self.model.text().to_owned(),
                        count: self.count,
                    },
                );
                leave = config.submit() == SubmitBehavior::BlurAndSubmit;
            }
            Leave => leave = true,
        }
        self.report(id, control, changed);
        leave
    }

    fn layout(&mut self, config: &InputConfig) {
        let key = (
            self.columns,
            config.secure,
            config.multiline,
            config.align,
            config.rtl,
        );
        if self.layout_key == Some(key) {
            return;
        }
        self.layout_key = Some(key);
        self.rows.clear();
        let mut row = Row::default();
        for (start, grapheme) in self.model.text().grapheme_indices(true) {
            let end = start + grapheme.len();
            if grapheme.contains('\n') {
                row.end = start;
                self.rows.push(row);
                row = Row {
                    start: end,
                    end,
                    ..Default::default()
                };
                continue;
            }
            let text = if config.secure {
                "•"
            } else if grapheme == "\t" {
                " "
            } else {
                grapheme
            };
            let width = text.width();
            if config.multiline && row.width + width > self.columns.max(1) && !row.glyphs.is_empty()
            {
                row.end = start;
                self.rows.push(row);
                row = Row {
                    start,
                    end: start,
                    ..Default::default()
                };
            }
            row.width += width;
            row.end = end;
            row.glyphs.push(Glyph {
                start,
                end,
                text: text.into(),
                width,
            });
        }
        let full_last_line = config.multiline && row.width >= self.columns.max(1);
        self.rows.push(row);
        if full_last_line {
            self.rows.push(Row {
                start: self.model.text().len(),
                end: self.model.text().len(),
                ..Default::default()
            });
        }
        for row in &mut self.rows {
            let spare = self.columns.saturating_sub(row.width);
            row.indent = match config.align {
                TextAlign::Center => spare / 2,
                TextAlign::Right => spare,
                TextAlign::Auto if config.rtl => spare,
                _ => 0,
            };
        }
    }

    fn caret_cell(&self) -> (usize, usize) {
        let caret = self.model.caret();
        let row = self
            .rows
            .iter()
            .rposition(|row| row.start <= caret)
            .unwrap_or(0);
        let line = &self.rows[row];
        (
            line.indent
                + line
                    .glyphs
                    .iter()
                    .take_while(|g| g.end <= caret)
                    .map(|g| g.width)
                    .sum::<usize>(),
            row,
        )
    }

    fn at_cell(&self, column: usize, row: usize) -> usize {
        let row = &self.rows[row.min(self.rows.len() - 1)];
        let mut x = row.indent;
        for glyph in &row.glyphs {
            if column < x + glyph.width.div_ceil(2) {
                return glyph.start;
            }
            x += glyph.width;
        }
        row.end
    }

    pub(crate) fn click(&mut self, config: &InputConfig, position: Position, extend: bool) {
        let Some(bounds) = self.bounds else {
            return;
        };
        self.layout(config);
        let column = (i32::from(position.x) - bounds.left).max(0) as usize + self.scroll.0;
        let row = (i32::from(position.y) - bounds.top).max(0) as usize + self.scroll.1;
        let at = self.at_cell(column, row);
        self.model.move_to(at, extend);
        self.goal_column = None;
    }

    pub(crate) fn cursor(&self) -> Option<Position> {
        self.cursor
    }

    pub(crate) fn draw(
        &mut self,
        config: &InputConfig,
        bounds: CellBounds,
        clip: Rect,
        buffer: &mut Buffer,
        opacity: f32,
    ) {
        self.bounds = Some(bounds);
        self.visible = bounds.clipped(clip);
        self.columns = (bounds.right - bounds.left).max(1) as usize;
        self.layout(config);
        let (column, row) = self.caret_cell();
        let height = (bounds.bottom - bounds.top).max(1) as usize;
        if self.focused {
            self.scroll.0 = self
                .scroll
                .0
                .min(column)
                .max((column + 1).saturating_sub(self.columns));
            self.scroll.1 = self.scroll.1.min(row).max((row + 1).saturating_sub(height));
        } else {
            self.scroll = (0, 0);
        }
        let x = bounds.left + column as i32 - self.scroll.0 as i32;
        let y = bounds.top + row as i32 - self.scroll.1 as i32;
        self.cursor = u16::try_from(x)
            .ok()
            .zip(u16::try_from(y).ok())
            .map(|(x, y)| Position::new(x, y))
            .filter(|at| self.visible.contains(*at));
        if self.visible.is_empty() {
            return;
        }
        let placeholder = self.model.text().is_empty();
        let color = if placeholder {
            config.placeholder_color.unwrap_or(config.style.color)
        } else {
            config.style.color
        };
        let mut style = Style::default().fg(crate::render::blend(
            color,
            opacity,
            buffer[(self.visible.x, self.visible.y)].bg,
        ));
        if config.style.font_weight >= 600 {
            style = style.add_modifier(Modifier::BOLD);
        }
        if config.style.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        let selection = self.model.selection();
        let lines: Vec<Line<'_>> = if placeholder {
            config
                .placeholder
                .lines()
                .map(|line| Line::styled(line, style))
                .collect()
        } else {
            self.rows
                .iter()
                .map(|row| {
                    let mut spans = vec![Span::styled(" ".repeat(row.indent), style)];
                    for glyph in &row.glyphs {
                        let selected =
                            self.focused && glyph.start < selection.1 && glyph.end > selection.0;
                        spans.push(Span::styled(
                            glyph.text.as_str(),
                            if selected {
                                style.add_modifier(Modifier::REVERSED)
                            } else {
                                style
                            },
                        ));
                    }
                    Line::from(spans)
                })
                .collect()
        };
        let scroll = (
            (self.scroll.1 + (i32::from(self.visible.y) - bounds.top).max(0) as usize)
                .min(u16::MAX as usize) as u16,
            (self.scroll.0 + (i32::from(self.visible.x) - bounds.left).max(0) as usize)
                .min(u16::MAX as usize) as u16,
        );
        Paragraph::new(lines)
            .scroll(scroll)
            .render(self.visible, buffer);
    }
}

fn key_press(id: NodeId, control: &EngineControl, key: &str) {
    control.dispatch_event(
        id,
        EventKind::KeyPress,
        EventPayload::Map(vec![("key", PayloadValue::Str(key.to_owned()))]),
    );
}

// Readline's M-f/M-b stop at the far edge of an alphanumeric word; C-w
// uses whitespace instead. Desktop Unit::Word also consumes trailing spaces.
fn readline_boundary(model: &Model, forward: bool, whitespace: bool) -> usize {
    let text = model.text();
    let mut at = model.caret();
    for inside in [false, true] {
        loop {
            let next = model.boundary(at, Unit::Char, forward);
            if next == at {
                break;
            }
            let start = if forward { at } else { next };
            let Some(character) = text[start..].chars().next() else {
                break;
            };
            let word = if whitespace {
                !character.is_whitespace()
            } else {
                character.is_alphanumeric()
            };
            if word != inside {
                break;
            }
            at = next;
        }
    }
    at
}
