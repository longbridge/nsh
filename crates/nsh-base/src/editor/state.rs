//! What an editor holds between frames, outside the reactive graph: the
//! buffer, the selections, the display map, the highlighter and the marks.
//! The view reads it in bindings that subscribe to the revisions the
//! component bumps ([`super::Revisions`]), so a change reaches only what
//! depends on it.

use std::{ops::Range, rc::Rc};

use nsh_core::geometry::{Bounds, Point as Pt, Points, Size, px};

use super::{
    EditorDecoration, EditorDiagnostic, EditorDiagnosticSeverity, EditorPosition, EditorSearch,
    EditorSearchResult, EditorSelection,
    buffer::{Buffer, Change},
    commands::{EditOptions, Editing},
    diagnostics::{self, Mark},
    display::DisplayMap,
    fold::{self, FoldRange},
    highlight::Highlighter,
    language::{self, LanguageConfig},
    movement, search,
    types::{RowLayout, Selection, Selections},
};
use crate::syntax::Token;

/// How many matches a search counts at most.
pub(crate) const MATCH_LIMIT: usize = 10_000;

/// The font the text is drawn and measured in.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Font {
    pub(crate) family: nsh_core::ui::SharedString,
    pub(crate) size: Points,
    pub(crate) line_height: Points,
}

impl Font {
    /// The CSS `font` shorthand `measure_text` takes.
    pub(crate) fn css(&self) -> String {
        let family = self.family.as_str();
        // Generic families stay bare; named ones are quoted.
        let generic = matches!(
            family,
            "monospace" | "sans-serif" | "serif" | "system-ui" | "ui-monospace"
        );
        if generic {
            format!("{}px {}", self.size.0, family)
        } else {
            format!("{}px \"{}\"", self.size.0, family.replace('"', ""))
        }
    }
}

/// A span of a row's text and what styles it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span {
    pub(crate) text: String,
    pub(crate) token: Option<Token>,
    /// The decorations over it (indexes into the editor's decorations).
    pub(crate) decorations: Vec<usize>,
}

/// What one display row shows: its spans, whether a fold ends it (the
/// placeholder after the text) and its line's number when it starts the
/// line.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RowContent {
    pub(crate) spans: Vec<Span>,
    pub(crate) folded: bool,
    /// The decorations whose line styles cover the row's line.
    pub(crate) line_decorations: Vec<usize>,
}

/// What the gutter shows for a display row.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct GutterRow {
    /// The line number to show (from 1), on a row that starts its line.
    pub(crate) number: Option<usize>,
    /// The line has a caret.
    pub(crate) active: bool,
    /// A fold can start on the line: `Some(folded)`.
    pub(crate) fold: Option<bool>,
    /// The most serious diagnostic on the line.
    pub(crate) severity: Option<EditorDiagnosticSeverity>,
}

/// A rectangle to draw, in the coordinates of the rows' box.
pub(crate) type Rect = Bounds<Points>;

/// The editor's state.
pub(crate) struct State {
    pub(crate) buffer: Buffer,
    pub(crate) selections: Selections,
    pub(crate) display: DisplayMap,
    pub(crate) highlighter: Option<Highlighter>,
    pub(crate) language: &'static LanguageConfig,
    pub(crate) options: EditOptions,
    pub(crate) font: Font,
    pub(crate) diagnostics: Vec<Mark<EditorDiagnostic>>,
    /// The decorations' ranges, each carrying its index in `decoration_items`.
    pub(crate) decorations: Vec<Mark<usize>>,
    /// The decorations as the app gave them (classes and styles by index).
    pub(crate) decoration_items: Vec<EditorDecoration>,
    pub(crate) search: Option<EditorSearch>,
    pub(crate) matches: Vec<Range<usize>>,
    pub(crate) search_error: Option<String>,
    /// Where folds can start, found again after edits (lazily).
    fold_candidates: Option<Vec<FoldRange>>,
    /// Folding on.
    pub(crate) folding: bool,
    /// What an input method is composing at the primary caret.
    pub(crate) composition: Option<String>,
    /// The text last reported to the app (`onChange`) or set by it, and
    /// the buffer's version then: an unchanged version is an unchanged
    /// text, with no need to make or compare the whole text.
    pub(crate) reported: Option<(u64, nsh_core::ui::SharedString)>,
    /// The selections last reported (`onSelectionChange`).
    pub(crate) reported_selections: Vec<EditorSelection>,
    /// What a search in the selection searches, carried through edits
    /// (`select::set_search`).
    pub(crate) search_scope: Option<Vec<Mark<()>>>,
}

impl State {
    pub(crate) fn new(
        text: &str,
        language: &'static LanguageConfig,
        options: EditOptions,
        font: Font,
        measure: Rc<dyn Fn(&str) -> Points>,
    ) -> State {
        let buffer = Buffer::new(text);
        let mut display = DisplayMap::new(measure, options.tab_size);
        display.rebuild(&buffer);
        State {
            buffer,
            selections: Selections::default(),
            display,
            // Made once the syntaxes are loaded (`Ed::load_syntaxes`), never
            // here: loading them blocks for tens of milliseconds.
            highlighter: if Highlighter::ready() {
                Highlighter::new(language.syntax_hint)
            } else {
                None
            },
            language,
            options,
            font,
            diagnostics: Vec::new(),
            decorations: Vec::new(),
            decoration_items: Vec::new(),
            search: None,
            matches: Vec::new(),
            search_error: None,
            fold_candidates: None,
            folding: true,
            composition: None,
            reported: None,
            reported_selections: Vec::new(),
            search_scope: None,
        }
    }

    pub(crate) fn line_height(&self) -> Points {
        self.font.line_height
    }

    /// Runs `f` on an [`Editing`] of the state with `page_rows` rows a page.
    pub(crate) fn editing<R>(
        &mut self,
        page_rows: usize,
        f: impl FnOnce(&mut Editing<'_>) -> R,
    ) -> R {
        let before = self.buffer.snapshot();
        let layout = self.display.layout(&before);
        let mut editing = Editing {
            buffer: &mut self.buffer,
            selections: &mut self.selections,
            language: self.language,
            options: &self.options,
            layout: &layout,
            page_rows,
        };
        f(&mut editing)
    }

    /// Brings what follows the text up to date after `changes` (the
    /// buffer's last change list): the layout, the highlighting, the marks
    /// and the matches.
    pub(crate) fn changed(&mut self, changes: &[Change]) {
        if changes.is_empty() {
            return;
        }
        match self.buffer.last_line_span() {
            Some(span) => {
                self.display.edited(&self.buffer, span);
                if let Some(h) = &mut self.highlighter {
                    h.edited(span.first, span.old_count, span.new_count);
                }
            }
            None => self.relayout(),
        }
        diagnostics::adjust(&mut self.diagnostics, changes);
        diagnostics::adjust(&mut self.decorations, changes);
        if let Some(scope) = &mut self.search_scope {
            diagnostics::adjust(scope, changes);
        }
        self.fold_candidates = None;
        self.find();
        let len = self.buffer.len();
        self.selections.clamp(len);
    }

    /// Lays everything out and highlights again (a new text, font or
    /// language).
    pub(crate) fn relayout(&mut self) {
        self.display.rebuild(&self.buffer);
        if let Some(h) = &mut self.highlighter {
            let lines = self.buffer.line_count();
            h.edited(0, lines, lines);
        }
        self.fold_candidates = None;
    }

    pub(crate) fn set_language(&mut self, language: &'static LanguageConfig) {
        if std::ptr::eq(self.language, language) {
            return;
        }
        self.language = language;
        self.highlighter = Highlighter::ready()
            .then(|| Highlighter::new(language.syntax_hint))
            .flatten();
        if let Some(h) = &mut self.highlighter {
            let lines = self.buffer.line_count();
            h.edited(0, lines, lines);
        }
    }

    // Search.

    /// Finds the search's matches again.
    pub(crate) fn find(&mut self) {
        self.matches.clear();
        self.search_error = None;
        let Some(search) = &self.search else { return };
        if search.query.is_empty() {
            return;
        }
        match search::find_all(&self.buffer, search, MATCH_LIMIT) {
            Ok(matches) => {
                self.matches = match &self.search_scope {
                    Some(scope) => search::within(matches, scope.iter().map(|m| &m.range)),
                    None => matches,
                }
            }
            Err(error) => self.search_error = Some(error),
        }
    }

    /// The match the primary selection is on.
    pub(crate) fn current_match(&self) -> Option<usize> {
        let primary = self.selections.primary().range();
        self.matches.iter().position(|m| *m == primary)
    }

    pub(crate) fn search_result(&self) -> EditorSearchResult {
        EditorSearchResult {
            count: self.matches.len() as u32,
            current: self.current_match().map(|i| i as u32),
            error: self.search_error.clone().map(Into::into),
        }
    }

    // Folds.

    /// Where folds can start.
    pub(crate) fn fold_candidates(&mut self) -> &[FoldRange] {
        if self.fold_candidates.is_none() {
            let ranges = if self.folding {
                fold::fold_ranges(&self.buffer, self.options.tab_size)
            } else {
                Vec::new()
            };
            self.fold_candidates = Some(ranges);
        }
        self.fold_candidates.as_deref().unwrap_or_default()
    }

    /// The fold that can start on `line`.
    pub(crate) fn fold_at(&mut self, line: usize) -> Option<FoldRange> {
        let candidates = self.fold_candidates();
        candidates
            .binary_search_by_key(&line, |f| f.start)
            .ok()
            .map(|i| candidates[i])
    }

    /// Folds (`Some(true)`), unfolds (`Some(false)`) or toggles (`None`)
    /// the fold starting on `line`. Carets inside a new fold move to its
    /// first line's end.
    pub(crate) fn set_fold(&mut self, line: usize, fold: Option<bool>) -> bool {
        let mut folds = self.display.folds().to_vec();
        let existing = folds.iter().position(|f| f.start == line);
        let want = fold.unwrap_or(existing.is_none());
        match (existing, want) {
            (Some(i), false) => {
                folds.remove(i);
            }
            (None, true) => {
                let Some(range) = self.fold_at(line) else {
                    return false;
                };
                // A fold inside the new one goes into it.
                folds.retain(|f| f.start < range.start || f.start > range.end);
                let at = folds.partition_point(|f| f.start < range.start);
                folds.insert(at, range);
                let hidden_start = self.buffer.line_start(range.start + 1);
                let hidden_end = self.buffer.line_end(range.end);
                let end_of_first = self.buffer.line_end(range.start);
                self.selections.map(|s| {
                    let inside = |o: usize| o >= hidden_start && o <= hidden_end;
                    if inside(s.head) || inside(s.anchor) {
                        Selection::caret(end_of_first)
                    } else {
                        s
                    }
                });
            }
            _ => return false,
        }
        self.display.set_folds(folds);
        self.display.rebuild(&self.buffer);
        true
    }

    /// Folds every fold that can start (`true`), or none.
    pub(crate) fn fold_all(&mut self, fold: bool) {
        let folds = if fold {
            // The outermost ones.
            let mut outer: Vec<FoldRange> = Vec::new();
            for f in self.fold_candidates().iter().copied() {
                if outer.last().is_none_or(|o| f.start > o.end) {
                    outer.push(f);
                }
            }
            outer
        } else {
            Vec::new()
        };
        self.display.set_folds(folds);
        self.display.rebuild(&self.buffer);
        if fold {
            let rows = self.display.layout(&self.buffer);
            let _ = rows;
            let positions: Vec<usize> = self.selections.all().iter().map(|s| s.head).collect();
            let mut moved = Vec::new();
            for head in positions {
                let line = self.buffer.line_of(head);
                if self.display.is_hidden(line) {
                    let row = self.display.line_row(line);
                    let first_line = self.display.row(row).line;
                    moved.push(Selection::caret(self.buffer.line_end(first_line)));
                } else {
                    moved.push(Selection::caret(head));
                }
            }
            self.selections = Selections::new(moved, 0);
        }
    }

    /// Unfolds what hides `offset` (a caret moved there, a search match).
    pub(crate) fn reveal(&mut self, offset: usize) {
        let line = self.buffer.line_of(offset);
        if !self.display.is_hidden(line) {
            return;
        }
        let folds: Vec<FoldRange> = self
            .display
            .folds()
            .iter()
            .copied()
            .filter(|f| !(line > f.start && line <= f.end))
            .collect();
        self.display.set_folds(folds);
        self.display.rebuild(&self.buffer);
    }

    // Positions.

    pub(crate) fn position(&self, offset: usize) -> EditorPosition {
        self.buffer.position(offset)
    }

    pub(crate) fn offset_of(&self, position: EditorPosition) -> usize {
        self.buffer.offset_of(position)
    }

    pub(crate) fn editor_selections(&self) -> Vec<EditorSelection> {
        self.selections
            .all()
            .iter()
            .map(|s| EditorSelection::new(self.position(s.anchor), self.position(s.head)))
            .collect()
    }

    pub(crate) fn set_editor_selections(&mut self, selections: &[EditorSelection]) {
        let all: Vec<Selection> = selections
            .iter()
            .map(|s| Selection::new(self.offset_of(s.anchor), self.offset_of(s.head)))
            .collect();
        let primary = all.len().saturating_sub(1);
        self.selections = Selections::new(all, primary);
        self.buffer.break_undo_group();
    }

    /// The offset at `point` of the rows' box (a press).
    pub(crate) fn offset_at(&self, point: Pt<Points>) -> usize {
        let lh = self.line_height().0.max(1.0);
        let rows = self.display.row_count();
        let row = ((point.y.0 / lh).floor().max(0.0) as usize).min(rows.saturating_sub(1));
        let before = self.buffer.snapshot();
        let layout = self.display.layout(&before);
        let x = point.x - self.display.row(row).indent;
        layout.offset_at(row, x.max(px(0.0)))
    }

    /// Where `offset` is drawn: its row's top left and its x.
    pub(crate) fn caret_point(&self, offset: usize) -> Pt<Points> {
        let before = self.buffer.snapshot();
        let layout = self.display.layout(&before);
        let (row, x) = layout.locate(offset);
        let indent = self.display.row(row).indent;
        Pt::new(x + indent, self.line_height() * row as f32)
    }

    /// How many rows a page moves: the viewport's height in rows, less
    /// one kept for context.
    pub(crate) fn page_rows(&self, viewport: Points) -> usize {
        ((viewport.0 / self.line_height().0.max(1.0)) as usize)
            .saturating_sub(1)
            .max(1)
    }

    // What the view draws.

    /// The content of display row `row`.
    pub(crate) fn row_content(&self, row: usize) -> RowContent {
        if row >= self.display.row_count() {
            return RowContent::default();
        }
        let info = self.display.row(row).clone();
        let row_text = self.display.row_text(&self.buffer, row);
        let line_start = self.buffer.line_start(info.line);
        // Boundaries where the style changes, in buffer offsets.
        let mut cuts: Vec<usize> = vec![info.range.start, info.range.end];
        let tokens = self
            .highlighter
            .as_ref()
            .and_then(|h| h.tokens(info.line))
            .unwrap_or_default();
        for (range, _) in tokens {
            cuts.push(line_start + range.start);
            cuts.push(line_start + range.end);
        }
        let decorations: Vec<(usize, Range<usize>)> = self
            .decorations
            .iter()
            .enumerate()
            .filter(|(_, m)| m.range.start < info.range.end && m.range.end > info.range.start)
            .filter(|(_, m)| {
                let d = &self.decoration_items[m.item];
                d.class.is_some() || d.style.is_some()
            })
            .map(|(i, m)| (i, m.range.clone()))
            .collect();
        for (_, r) in &decorations {
            cuts.push(r.start);
            cuts.push(r.end);
        }
        cuts.retain(|c| *c >= info.range.start && *c <= info.range.end);
        cuts.sort_unstable();
        cuts.dedup();
        let mut spans: Vec<Span> = Vec::new();
        for pair in cuts.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if start >= end {
                continue;
            }
            let display = row_text.display_range(start..end);
            let text = row_text.text[display].to_owned();
            if text.is_empty() {
                continue;
            }
            let token = tokens
                .iter()
                .find(|(r, _)| line_start + r.start <= start && line_start + r.end >= end)
                .map(|(_, t)| *t);
            let decorations: Vec<usize> = decorations
                .iter()
                .filter(|(_, r)| r.start <= start && r.end >= end)
                .map(|(i, _)| self.decorations[*i].item)
                .collect();
            match spans.last_mut() {
                Some(last) if last.token == token && last.decorations == decorations => {
                    last.text.push_str(&text);
                }
                _ => spans.push(Span {
                    text,
                    token,
                    decorations,
                }),
            }
        }
        let folded = info.first
            && self
                .display
                .folds()
                .binary_search_by_key(&info.line, |f| f.start)
                .is_ok()
            && self.row_is_last_of_line(row);
        let line_decorations = self
            .decorations
            .iter()
            .filter(|m| {
                let first = self.buffer.line_of(m.range.start);
                let last = self.buffer.line_of(m.range.end);
                info.line >= first && info.line <= last
            })
            .map(|m| m.item)
            .filter(|i| {
                let d = &self.decoration_items[*i];
                d.line_class.is_some() || d.line_style.is_some()
            })
            .collect();
        RowContent {
            spans,
            folded,
            line_decorations,
        }
    }

    fn row_is_last_of_line(&self, row: usize) -> bool {
        row + 1 >= self.display.row_count() || self.display.row(row + 1).first
    }

    /// What the gutter shows on row `row`.
    pub(crate) fn gutter_row(&mut self, row: usize) -> GutterRow {
        if row >= self.display.row_count() {
            return GutterRow::default();
        }
        let info = self.display.row(row).clone();
        if !info.first {
            return GutterRow::default();
        }
        let line = info.line;
        let start = self.buffer.line_start(line);
        let end = self.buffer.line_end(line);
        let active = self
            .selections
            .all()
            .iter()
            .any(|s| s.head >= start && s.head <= end);
        let folded = self
            .display
            .folds()
            .binary_search_by_key(&line, |f| f.start)
            .is_ok();
        let fold = if folded {
            Some(true)
        } else if self.folding && self.fold_at(line).is_some() {
            Some(false)
        } else {
            None
        };
        let severity =
            diagnostics::line_severities(&self.diagnostics, &self.buffer, line..line + 1)
                .into_iter()
                .next()
                .map(|(_, s)| s);
        GutterRow {
            number: Some(line + 1),
            active,
            fold,
            severity,
        }
    }

    /// The rectangles of `range` on rows `rows`, one per row it covers;
    /// `newline` adds a sliver after a row whose line break is selected.
    pub(crate) fn range_rects(
        &self,
        range: Range<usize>,
        rows: Range<usize>,
        newline: bool,
    ) -> Vec<Rect> {
        let mut out = Vec::new();
        if range.is_empty() && !newline {
            return out;
        }
        let lh = self.line_height();
        let before = self.buffer.snapshot();
        let layout = self.display.layout(&before);
        let (first_row, _) = layout.locate(range.start);
        let (last_row, _) = layout.locate(range.end);
        let space = self.display.space_width();
        for row in first_row.max(rows.start)..=last_row.min(rows.end.saturating_sub(1)) {
            let info = self.display.row(row);
            let start = range.start.max(info.range.start);
            let end = range.end.min(info.range.end);
            let x0 = if start <= info.range.start {
                px(0.0)
            } else {
                self.display.x_of(&self.buffer, start) + info.indent
            };
            let mut x1 = self.display.x_of(&self.buffer, end) + info.indent;
            if row == first_row && start > info.range.start {
                // x0 already exact.
            }
            // The line break, or the wrap, is selected: a sliver past
            // the end.
            if newline && range.end > info.range.end {
                x1 += space;
            }
            if x1 > x0 {
                out.push(Bounds::new(
                    Pt::new(x0, lh * row as f32),
                    Size::new(x1 - x0, lh),
                ));
            }
        }
        out
    }

    /// The selections' rectangles on `rows`.
    pub(crate) fn selection_rects(&self, rows: Range<usize>) -> Vec<Rect> {
        let mut out = Vec::new();
        for s in self.selections.all() {
            if s.is_empty() {
                continue;
            }
            out.extend(self.range_rects(s.range(), rows.clone(), true));
        }
        out
    }

    /// The carets on `rows`: their top left (the caret is 2 points wide and
    /// a line tall).
    pub(crate) fn caret_points(&self, rows: Range<usize>) -> Vec<Pt<Points>> {
        let lh = self.line_height();
        self.selections
            .all()
            .iter()
            .map(|s| self.caret_point(s.head))
            .filter(|p| {
                let row = (p.y.0 / lh.0.max(1.0)).round() as usize;
                rows.contains(&row)
            })
            .collect()
    }

    /// The rows the carets are on (the active lines' rows: every row of
    /// their lines).
    pub(crate) fn active_rows(&self, rows: Range<usize>) -> Vec<usize> {
        let mut out = Vec::new();
        for s in self.selections.all() {
            if !s.is_empty() {
                continue;
            }
            let line = self.buffer.line_of(s.head);
            let first = self.display.line_row(line);
            let mut row = first;
            while row < self.display.row_count() && (row == first || !self.display.row(row).first) {
                if rows.contains(&row) && !out.contains(&row) {
                    out.push(row);
                }
                row += 1;
            }
        }
        out
    }

    /// The search matches' rectangles on `rows`, with whether each is the
    /// current one.
    pub(crate) fn match_rects(&self, rows: Range<usize>) -> Vec<(Rect, bool)> {
        let current = self.current_match();
        let (start, end) = self.rows_offsets(rows.clone());
        let first = self.matches.partition_point(|m| m.end < start);
        let mut out = Vec::new();
        for (i, m) in self.matches[first..].iter().enumerate() {
            if m.start > end {
                break;
            }
            for r in self.range_rects(m.clone(), rows.clone(), false) {
                out.push((r, current == Some(first + i)));
            }
        }
        out
    }

    /// The diagnostics' marks on `rows`: a rectangle under the text and its
    /// severity.
    pub(crate) fn diagnostic_rects(
        &self,
        rows: Range<usize>,
    ) -> Vec<(Rect, EditorDiagnosticSeverity)> {
        let (start, end) = self.rows_offsets(rows.clone());
        let mut out = Vec::new();
        for mark in &self.diagnostics {
            if mark.range.end < start || mark.range.start > end {
                continue;
            }
            let mut range = mark.range.clone();
            if range.is_empty() {
                // The char after it, or the end of its line.
                let next = movement::next_grapheme(&self.buffer, range.start);
                let line_end = self.buffer.line_end(self.buffer.line_of(range.start));
                range.end = next.min(line_end.max(range.start));
                if range.is_empty() && range.start > 0 {
                    range.start = movement::prev_grapheme(&self.buffer, range.start);
                }
            }
            for r in self.range_rects(range, rows.clone(), false) {
                out.push((r, mark.item.severity));
            }
        }
        out
    }

    /// The bracket at the primary caret and its partner: their
    /// rectangles.
    pub(crate) fn bracket_rects(&self, rows: Range<usize>) -> Vec<Rect> {
        let primary = self.selections.primary();
        if !primary.is_empty() {
            return Vec::new();
        }
        let Some((open, close)) =
            movement::matching_bracket(&self.buffer, primary.head, self.language.brackets)
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for at in [open, close] {
            let end = movement::next_grapheme(&self.buffer, at);
            out.extend(self.range_rects(at..end, rows.clone(), false));
        }
        out
    }

    /// The indent guides on `rows`: their x and the rows they span (one
    /// guide per level of the lines' indentation, blank lines taking the
    /// next line's).
    pub(crate) fn indent_guides(&self, rows: Range<usize>) -> Vec<(Points, Range<usize>)> {
        let tab = self.options.tab_size.max(1) as usize;
        let space = self.display.space_width();
        let mut columns: Vec<Option<usize>> = Vec::new();
        for row in rows.clone() {
            if row >= self.display.row_count() {
                columns.push(Some(0));
                continue;
            }
            let info = self.display.row(row);
            if !info.first {
                columns.push(None);
                continue;
            }
            columns.push(Some(self.indent_columns(info.line)));
        }
        // Blank lines take the indentation around them.
        for i in 0..columns.len() {
            if columns[i] == Some(usize::MAX) {
                let next = columns[i..]
                    .iter()
                    .flatten()
                    .find(|c| **c != usize::MAX)
                    .copied()
                    .unwrap_or(0);
                let prev = columns[..i]
                    .iter()
                    .rev()
                    .flatten()
                    .find(|c| **c != usize::MAX)
                    .copied()
                    .unwrap_or(0);
                columns[i] = Some(next.min(prev));
            }
        }
        let mut out: Vec<(Points, Range<usize>)> = Vec::new();
        let mut open: Vec<(usize, usize)> = Vec::new(); // (level, first row)
        let mut last_indent = 0usize;
        for (i, row) in rows.clone().enumerate() {
            let indent = match columns.get(i).copied().flatten() {
                Some(c) => c,
                None => last_indent,
            };
            last_indent = indent;
            let levels = indent / tab;
            while open.len() > levels {
                let (level, first) = open.pop().unwrap_or_default();
                out.push((space * (level * tab) as f32, first..row));
            }
            while open.len() < levels {
                open.push((open.len(), row));
            }
        }
        for (level, first) in open {
            out.push((space * (level * tab) as f32, first..rows.end));
        }
        out.retain(|(_, r)| !r.is_empty());
        out
    }

    /// The columns of `line`'s indentation, `usize::MAX` for a blank line.
    fn indent_columns(&self, line: usize) -> usize {
        let tab = self.options.tab_size.max(1) as usize;
        let mut columns = 0;
        for ch in self.buffer.chars_at(self.buffer.line_start(line)) {
            match ch {
                ' ' => columns += 1,
                '\t' => columns = (columns / tab + 1) * tab,
                '\n' => return usize::MAX,
                _ => return columns,
            }
        }
        usize::MAX
    }

    /// The text offsets rows `rows` cover.
    fn rows_offsets(&self, rows: Range<usize>) -> (usize, usize) {
        let count = self.display.row_count();
        if count == 0 {
            return (0, 0);
        }
        let first = self.display.row(rows.start.min(count - 1)).range.start;
        let last = self
            .display
            .row(rows.end.saturating_sub(1).min(count - 1))
            .range
            .end;
        (first, last)
    }

    /// The size of the rows' box: the widest row (without soft wrap) and
    /// every row's height.
    pub(crate) fn content_size(&self) -> Size<Points> {
        let lh = self.line_height();
        Size::new(
            self.display.widest_row() + self.display.space_width() * 2.0,
            lh * self.display.row_count() as f32,
        )
    }

    /// Clears what the app set that refers to the old text (a new value
    /// from the app).
    pub(crate) fn primary_head(&self) -> usize {
        self.selections.primary().head
    }

    /// The language a name or path names.
    pub(crate) fn language_for(name: Option<&str>) -> &'static LanguageConfig {
        language::language(name.unwrap_or(""))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::editor::commands::Outcome;

    /// `n` lines of Rust-like code.
    fn source(n: usize) -> String {
        let mut text = String::from("fn main() {\n");
        for i in 0..n {
            text.push_str(&format!(
                "    let value_{i} = compute({i}, \"row {i}\") + offset; // line {i}\n"
            ));
        }
        text.push_str("}\n");
        text
    }

    fn state(text: &str) -> State {
        let font = Font {
            family: "monospace".into(),
            size: px(13.0),
            line_height: px(20.0),
        };
        let measure: Rc<dyn Fn(&str) -> Points> =
            Rc::new(|s: &str| px(s.chars().count() as f32 * 8.0));
        State::new(
            text,
            State::language_for(Some("rust")),
            EditOptions::default(),
            font,
            measure,
        )
    }

    fn time<R>(label: &str, f: impl FnOnce() -> R) -> R {
        let start = Instant::now();
        let r = f();
        eprintln!("{label}: {:?}", start.elapsed());
        r
    }

    /// The costs of a 200 000 line text: opening it, typing in it, finding
    /// in it and highlighting it.
    #[test]
    #[ignore]
    fn bench_large_text() {
        let text = time("source", || source(200_000));
        eprintln!("{} bytes", text.len());
        let mut st = time("State::new", || state(&text));
        time("buffer.text", || st.buffer.text());
        time("fold candidates", || st.fold_candidates().len());
        time("content size", || st.content_size());
        let end = st.buffer.len();
        st.selections = Selections::single(Selection::caret(end / 2));
        for round in 0..3 {
            time(&format!("type {round}"), || {
                if let Outcome::Edited(changes) = st.editing(40, |e| e.insert("x")) {
                    st.changed(&changes);
                }
            });
        }
        st.search = Some(EditorSearch {
            query: "offset".into(),
            ..Default::default()
        });
        time("find", || st.find());
        time("type with a search", || {
            if let Outcome::Edited(changes) = st.editing(40, |e| e.insert("y")) {
                st.changed(&changes);
            }
        });
        Highlighter::load();
        let mut h = Highlighter::new("rs").unwrap();
        let rope = st.buffer.rope().clone();
        {
            use syntect::parsing::{ParseState, ScopeStack};
            let syntax = crate::syntax::find_syntax("rs").unwrap();
            let set = crate::syntax::syntaxes();
            let lines: Vec<String> = text
                .split_inclusive('\n')
                .take(20_000)
                .map(String::from)
                .collect();
            time("raw syntect, 20 000 lines", || {
                let mut parser = ParseState::new(syntax);
                let mut scopes = ScopeStack::new();
                for line in &lines {
                    for (_, op) in parser.parse_line(line, set).unwrap() {
                        let _ = scopes.apply(&op);
                    }
                }
            });
            time("raw syntect + state clones, 20 000 lines", || {
                let mut parser = ParseState::new(syntax);
                let mut scopes = ScopeStack::new();
                let mut kept = Vec::new();
                for line in &lines {
                    for (_, op) in parser.parse_line(line, set).unwrap() {
                        let _ = scopes.apply(&op);
                    }
                    kept.push((parser.clone(), scopes.clone()));
                }
                kept.len()
            });
            let mut h = Highlighter::new("rs").unwrap();
            let rope = st.buffer.rope().clone();
            time("ours, 20 000 lines", || {
                h.highlight(&rope, 19_999, Duration::from_secs(100))
            });
        }
        time("highlight the first screen", || {
            h.highlight(&rope, 60, Duration::from_secs(10))
        });
        let mut far = Highlighter::new("rs").unwrap();
        let first = 199_000;
        let guess = far
            .guess(
                &rope,
                first - 40,
                st.buffer.line_start(first - 40),
                first + 60,
            )
            .unwrap();
        time("guess 100 lines at line 199 000", || {
            far.guessed(guess.run())
        });
        if std::env::var_os("NSH_BENCH_ALL").is_some() {
            time("highlight to the end", || {
                h.highlight(&rope, st.buffer.line_count() - 1, Duration::from_secs(100))
            });
        }
    }
}
