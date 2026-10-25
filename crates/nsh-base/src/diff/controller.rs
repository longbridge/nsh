//! The diff view's controller: the two texts, their model and rows, the
//! revisions bindings subscribe to, and what props and the ref do to them.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ops::Range,
    rc::Rc,
};

use nsh_core::ui::prelude::*;
use ropey::Rope;

use super::{
    DiffChange, DiffExpand, DiffHunk, DiffInline, DiffLayout, DiffLine, DiffSide, DiffStats,
    DiffViewEvent, DiffViewProps,
    layout::{self, Expansions, Header, Layout, Options, Row},
    model::{self, DiffModel, InlineMode, Pair},
    patch,
};
use crate::{
    editor::{
        controller::font_family,
        highlight::{GUESS_CONTEXT, GUESS_DISTANCE, Highlighter, LOOKAHEAD, SLICE, off_thread},
        language,
        state::Font,
    },
    syntax::Token,
};

/// Rows made beyond the viewport on each side.
pub(crate) const OVERSCAN: usize = 8;
/// Columns a tab takes.
const TAB: usize = 4;

/// One side's text: its lines, their highlighting, and how they are
/// numbered.
pub(crate) struct Side {
    pub(crate) text: String,
    rope: Rope,
    /// The start of every line.
    starts: Vec<usize>,
    pub(crate) highlighter: Option<Highlighter>,
    /// The widest line, in columns (tabs expanded).
    pub(crate) columns: usize,
    /// A patch's numbering: `(first line in the text, its number in the
    /// file)` of each hunk, in order. Empty: line `i` is number `i + 1`.
    numbers: Vec<(usize, usize)>,
}

impl Side {
    fn new(text: String, numbers: Vec<(usize, usize)>) -> Side {
        let starts = starts(&text);
        let mut columns = 0;
        for line in 0..starts.len() {
            columns = columns.max(display_columns(line_of(&text, &starts, line)));
        }
        Side {
            rope: Rope::from_str(&text),
            text,
            starts,
            highlighter: None,
            columns,
            numbers,
        }
    }

    /// How many lines it has (a final line break makes none).
    pub(crate) fn lines(&self) -> usize {
        self.starts.len()
    }

    /// Line `line`, without its line break.
    pub(crate) fn line(&self, line: usize) -> &str {
        line_of(&self.text, &self.starts, line)
    }

    /// The number line `line` shows (from 1, as in the file).
    pub(crate) fn number(&self, line: usize) -> usize {
        let at = self.numbers.partition_point(|(first, _)| *first <= line);
        match at.checked_sub(1).map(|i| self.numbers[i]) {
            Some((first, number)) => number + (line - first),
            None => line + 1,
        }
    }

    /// The tokens of `line` (ranges within it), empty while it is not
    /// highlighted.
    fn tokens(&self, line: usize) -> Vec<(Range<usize>, Token)> {
        self.highlighter
            .as_ref()
            .and_then(|h| h.tokens(line))
            .map(<[_]>::to_vec)
            .unwrap_or_default()
    }
}

/// What a side of a row shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RowCell {
    /// The line, `None` for the filler beside a line the other side lacks.
    pub(crate) line: Option<usize>,
    /// Its number in the file.
    pub(crate) number: Option<usize>,
    /// The row's kind on this side.
    pub(crate) kind: Tint,
    /// Its text in spans.
    pub(crate) spans: Vec<Span>,
}

/// How a cell is tinted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tint {
    Context,
    Added,
    Deleted,
    Filler,
}

/// A run of a line's text with one token and whether it changed within
/// the line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span {
    pub(crate) text: String,
    pub(crate) token: Option<Token>,
    pub(crate) changed: bool,
}

/// The changed ranges within a paired old line and new line.
type Changed = (Vec<Range<usize>>, Vec<Range<usize>>);
/// A patch's numbering of each side (`Side::numbers`).
type Numbers = (Vec<(usize, usize)>, Vec<(usize, usize)>);

/// The view's state.
pub(crate) struct State {
    pub(crate) old: Side,
    pub(crate) new: Side,
    pub(crate) model: DiffModel,
    pub(crate) rows: Vec<Row>,
    pub(crate) expansions: Expansions,
    headers: Vec<Header>,
    /// Whether the texts are a patch's (only its hunks' lines).
    pub(crate) patch: bool,
    pub(crate) font: Font,
    /// The width of a column.
    pub(crate) column: Points,
    hint: &'static str,
    /// The changed ranges of paired lines, by `(old, new)` line.
    inline: HashMap<(usize, usize), Changed>,
    /// The pairs of hunks' lines, by hunk (unified rows' words).
    pairs: HashMap<usize, Vec<Pair>>,
}

impl State {
    fn new(font: Font) -> State {
        let column = measure_text(&font.css(), "0");
        State {
            old: Side::new(String::new(), Vec::new()),
            new: Side::new(String::new(), Vec::new()),
            model: DiffModel::compute("", ""),
            rows: Vec::new(),
            expansions: Expansions::default(),
            headers: Vec::new(),
            patch: false,
            font,
            column,
            hint: "",
            inline: HashMap::new(),
            pairs: HashMap::new(),
        }
    }

    /// Diffs `old` and `new` again (`numbers`: a patch's numbering of each
    /// side, `headers` its hunks' headers).
    fn set_texts(&mut self, old: String, new: String, numbers: Numbers, headers: Vec<Header>) {
        self.model = DiffModel::compute(&old, &new);
        self.old = Side::new(old, numbers.0);
        self.new = Side::new(new, numbers.1);
        self.patch = !headers.is_empty();
        self.headers = headers;
        self.expansions.clear();
        self.inline.clear();
        self.pairs.clear();
        self.make_highlighters();
    }

    fn make_highlighters(&mut self) {
        let ready = Highlighter::ready() && !self.hint.is_empty();
        for side in [&mut self.old, &mut self.new] {
            side.highlighter = ready.then(|| Highlighter::new(self.hint)).flatten();
        }
    }

    /// Lays the rows out again.
    fn relayout(&mut self, props: &DiffViewProps) {
        let options = Options {
            layout: match props.layout.get_untracked() {
                DiffLayout::Split => Layout::Split,
                DiffLayout::Unified => Layout::Unified,
            },
            context: props.context_lines.get_untracked() as usize,
            expand_all: props.expand_all.get_untracked(),
            headers: self.headers.clone(),
        };
        self.rows = layout::rows(
            &self.model,
            &self.old.text,
            &self.new.text,
            &options,
            &self.expansions,
        );
    }

    /// The cells of row `row`: the old side's and the new side's (split),
    /// or the one line it shows (unified, in `.1`, `.0` the filler).
    pub(crate) fn cells(&mut self, row: usize, inline: DiffInline) -> Option<(RowCell, RowCell)> {
        let Row::Line {
            old,
            new,
            kind,
            hunk,
        } = self.rows.get(row).cloned()?
        else {
            return None;
        };
        use layout::Kind;
        let (old_tint, new_tint) = match kind {
            Kind::Context => (Tint::Context, Tint::Context),
            Kind::Added => (Tint::Filler, Tint::Added),
            Kind::Deleted => (Tint::Deleted, Tint::Filler),
            Kind::Modified => (Tint::Deleted, Tint::Added),
        };
        let changed = match (old, new, kind) {
            (Some(o), Some(n), Kind::Modified) => self.inline_of(o, n, inline),
            // Unified rows show a pair's lines one over the other: the
            // words are those of the pair.
            (Some(o), None, Kind::Deleted) => match self.partner(hunk, Some(o), None) {
                Some(n) => self.inline_of(o, n, inline),
                None => Default::default(),
            },
            (None, Some(n), Kind::Added) => match self.partner(hunk, None, Some(n)) {
                Some(o) => self.inline_of(o, n, inline),
                None => Default::default(),
            },
            _ => Default::default(),
        };
        let old_cell = cell(&self.old, old, old_tint, &changed.0);
        let new_cell = cell(&self.new, new, new_tint, &changed.1);
        Some((old_cell, new_cell))
    }

    /// The line paired with old line `old` (or new line `new`) of hunk
    /// `hunk`: the other side's.
    fn partner(
        &mut self,
        hunk: Option<usize>,
        old: Option<usize>,
        new: Option<usize>,
    ) -> Option<usize> {
        let hunk = hunk?;
        let (old_text, new_text) = (&self.old.text, &self.new.text);
        let model = &self.model;
        let pairs = self
            .pairs
            .entry(hunk)
            .or_insert_with(|| model.pairs(hunk, old_text, new_text));
        pairs.iter().find_map(|pair| match (old, new) {
            (Some(o), _) if pair.old == Some(o) => pair.new,
            (_, Some(n)) if pair.new == Some(n) => pair.old,
            _ => None,
        })
    }

    fn inline_of(&mut self, old: usize, new: usize, inline: DiffInline) -> Changed {
        let mode = match inline {
            DiffInline::WordAlt => InlineMode::WordAlt,
            DiffInline::Word => InlineMode::Word,
            DiffInline::Char => InlineMode::Char,
            DiffInline::None => return Default::default(),
        };
        let (o, n) = (self.old.line(old), self.new.line(new));
        self.inline
            .entry((old, new))
            .or_insert_with(|| DiffModel::inline(o, n, mode))
            .clone()
    }

    /// The hunks, as the ref gives them.
    pub(crate) fn hunks(&self) -> Vec<DiffHunk> {
        self.model
            .hunks
            .iter()
            .enumerate()
            .map(|(index, h)| DiffHunk {
                index: index as u32,
                old_start: h.old.start as u32,
                old_lines: h.old.len() as u32,
                new_start: h.new.start as u32,
                new_lines: h.new.len() as u32,
            })
            .collect()
    }

    /// The width of a side's text box.
    pub(crate) fn text_width(&self, side: DiffSide) -> Points {
        let columns = match side {
            DiffSide::Old => self.old.columns,
            DiffSide::New => self.new.columns,
        };
        self.column * (columns + 2) as f32
    }

    /// The width of a number column.
    pub(crate) fn number_width(&self) -> Points {
        let last = self
            .old
            .number(self.old.lines())
            .max(self.new.number(self.new.lines()));
        let digits = (last.max(1).ilog10() as usize + 1).max(2);
        self.column * digits as f32
    }
}

/// The cell of `line` of `side`.
fn cell(side: &Side, line: Option<usize>, tint: Tint, changed: &[Range<usize>]) -> RowCell {
    let Some(line) = line else {
        return RowCell {
            line: None,
            number: None,
            kind: Tint::Filler,
            spans: Vec::new(),
        };
    };
    let text = side.line(line);
    RowCell {
        line: Some(line),
        number: Some(side.number(line)),
        kind: tint,
        spans: spans(text, &side.tokens(line), changed),
    }
}

/// `text` cut where its tokens or its changed ranges begin or end, tabs
/// expanded.
pub(crate) fn spans(
    text: &str,
    tokens: &[(Range<usize>, Token)],
    changed: &[Range<usize>],
) -> Vec<Span> {
    let mut cuts = vec![0, text.len()];
    for (range, _) in tokens {
        cuts.extend([range.start, range.end]);
    }
    for range in changed {
        cuts.extend([range.start, range.end]);
    }
    cuts.retain(|c| *c <= text.len() && text.is_char_boundary(*c));
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<Span> = Vec::new();
    let mut column = 0;
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let token = tokens
            .iter()
            .find(|(r, _)| r.start <= start && end <= r.end)
            .map(|(_, t)| *t);
        let is_changed = changed.iter().any(|r| r.start <= start && end <= r.end);
        let mut piece = String::with_capacity(end - start);
        for c in text[start..end].chars() {
            if c == '\t' {
                let n = TAB - column % TAB;
                piece.extend(std::iter::repeat_n(' ', n));
                column += n;
            } else {
                piece.push(c);
                column += 1;
            }
        }
        match out.last_mut() {
            Some(last) if last.token == token && last.changed == is_changed => {
                last.text.push_str(&piece)
            }
            _ => out.push(Span {
                text: piece,
                token,
                changed: is_changed,
            }),
        }
    }
    out
}

/// The start of every line of `text` (a final line break starts none).
fn starts(text: &str) -> Vec<usize> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0];
    out.extend(
        text.match_indices('\n')
            .map(|(i, _)| i + 1)
            .filter(|i| *i < text.len()),
    );
    out
}

fn line_of<'a>(text: &'a str, starts: &[usize], line: usize) -> &'a str {
    let Some(&start) = starts.get(line) else {
        return "";
    };
    let end = starts.get(line + 1).map_or(text.len(), |next| next - 1);
    let line = &text[start..end.max(start)];
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

fn display_columns(line: &str) -> usize {
    let mut column = 0;
    for c in line.chars() {
        column += if c == '\t' { TAB - column % TAB } else { 1 };
    }
    column
}

/// The diff view's controller: a clone shares everything.
#[derive(Clone)]
pub(crate) struct Dv {
    pub(crate) st: Rc<RefCell<State>>,
    pub(crate) props: DiffViewProps,
    cx: Scope<DiffViewEvent>,
    /// The rows changed (texts, layout, expansions).
    pub(crate) rev: Signal<u64>,
    /// Highlighting progressed.
    pub(crate) hl_rev: Signal<u64>,
    pub(crate) scroll_y: Signal<Points>,
    pub(crate) viewport: Signal<Size<Points>>,
    highlighting: Rc<Cell<bool>>,
    /// The stats reported last (`onStatsChange`).
    reported: Rc<Cell<Option<DiffStats>>>,
    pub(crate) vscroll: ElementRef,
}

impl Dv {
    pub(crate) fn new(props: DiffViewProps, cx: Scope<DiffViewEvent>) -> Dv {
        let font = Font {
            family: font_family(props.font_family.get_untracked()),
            size: props.font_size.get_untracked(),
            line_height: props.line_height.get_untracked(),
        };
        let dv = Dv {
            st: Rc::new(RefCell::new(State::new(font))),
            props,
            cx,
            rev: signal(0),
            hl_rev: signal(0),
            scroll_y: signal(px(0.0)),
            viewport: signal(Size::default()),
            highlighting: Rc::new(Cell::new(false)),
            reported: Rc::default(),
            vscroll: element_ref(),
        };
        dv.follow_props();
        dv
    }

    fn bump(&self) {
        self.rev.update(|r| *r += 1);
    }

    fn follow_props(&self) {
        let dv = self.clone();
        effect(move || {
            let font = Font {
                family: font_family(dv.props.font_family.get()),
                size: dv.props.font_size.get(),
                line_height: dv.props.line_height.get(),
            };
            {
                let mut st = dv.st.borrow_mut();
                st.column = measure_text(&font.css(), "0");
                st.font = font;
            }
            dv.bump();
        });
        let dv = self.clone();
        effect(move || {
            let old = dv.props.old_value.get();
            let new = dv.props.new_value.get();
            let patch_text = dv.props.patch.get();
            let language = dv.props.language.get();
            let (old, new, numbers, headers, path) = match (old, new, patch_text) {
                (None, None, Some(text)) => from_patch(&text),
                (old, new, _) => (
                    old.map(|s| s.to_string()).unwrap_or_default(),
                    new.map(|s| s.to_string()).unwrap_or_default(),
                    Default::default(),
                    Vec::new(),
                    None,
                ),
            };
            let name = language.map(|l| l.to_string()).or(path);
            {
                let mut st = dv.st.borrow_mut();
                st.hint = language::language(name.as_deref().unwrap_or("")).syntax_hint;
                st.set_texts(old, new, numbers, headers);
                st.relayout(&dv.props);
            }
            dv.bump();
            dv.report_stats();
            dv.load_syntaxes();
        });
        let dv = self.clone();
        effect(move || {
            dv.props.layout.get();
            dv.props.context_lines.get();
            dv.props.expand_all.get();
            dv.st.borrow_mut().relayout(&dv.props);
            dv.bump();
        });
    }

    /// The rows in view, and a few around them.
    pub(crate) fn visible_rows(&self) -> Range<usize> {
        let st = self.st.borrow();
        let lh = st.font.line_height.0.max(1.0);
        let count = st.rows.len();
        let y = self.scroll_y.get().0.max(0.0);
        let height = self.viewport.get().height.0.max(lh * 40.0);
        let first = ((y / lh).floor() as usize).saturating_sub(OVERSCAN);
        let last = (((y + height) / lh).ceil() as usize + OVERSCAN).min(count);
        first.min(last)..last
    }

    // The ref.

    pub(crate) fn hunks(&self) -> Vec<DiffHunk> {
        self.st.borrow().hunks()
    }

    pub(crate) fn stats(&self) -> DiffStats {
        let st = self.st.borrow();
        let (added, deleted) = st.model.stats();
        DiffStats {
            added: added as u32,
            deleted: deleted as u32,
            hunks: st.model.hunks.len() as u32,
        }
    }

    /// Reports the stats when they changed.
    fn report_stats(&self) {
        let stats = self.stats();
        if self.reported.replace(Some(stats)) != Some(stats) {
            self.cx.emit(DiffViewEvent::StatsChange(stats));
        }
    }

    pub(crate) fn go_to_hunk(&self, index: usize) {
        let y = {
            let st = self.st.borrow();
            let Some(row) = layout::hunk_row(&st.rows, index) else {
                return;
            };
            st.font.line_height * row as f32
        };
        self.vscroll.scroll_to(point(px(0.0), y), true);
    }

    pub(crate) fn revert_hunk(&self, index: usize) {
        let change = {
            let st = self.st.borrow();
            let Some(hunk) = st.model.hunks.get(index) else {
                return;
            };
            DiffChange {
                old_value: st.old.text.clone(),
                new_value: model::revert_hunk(&st.old.text, &st.new.text, hunk),
            }
        };
        self.changed(change);
    }

    pub(crate) fn accept_hunk(&self, index: usize) {
        let change = {
            let st = self.st.borrow();
            let Some(hunk) = st.model.hunks.get(index) else {
                return;
            };
            DiffChange {
                old_value: model::accept_hunk(&st.old.text, &st.new.text, hunk),
                new_value: st.new.text.clone(),
            }
        };
        self.changed(change);
    }

    /// Shows `change`'s texts at once (the app sets them as the props
    /// after `onChange`, which changes nothing then) and reports them.
    fn changed(&self, change: DiffChange) {
        if self.st.borrow().patch {
            // A patch's texts are excerpts: there is nothing to revert in.
            return;
        }
        {
            let mut st = self.st.borrow_mut();
            st.set_texts(
                change.old_value.clone(),
                change.new_value.clone(),
                Default::default(),
                Vec::new(),
            );
            st.relayout(&self.props);
        }
        self.bump();
        self.load_syntaxes();
        self.cx.emit(DiffViewEvent::Change(change));
        self.report_stats();
    }

    pub(crate) fn expand(&self, gap: usize, direction: DiffExpand, lines: usize) {
        {
            let mut st = self.st.borrow_mut();
            match direction {
                DiffExpand::All => st.expansions.expand_gap(gap),
                DiffExpand::Up => st.expansions.expand(gap, true, lines),
                DiffExpand::Down => st.expansions.expand(gap, false, lines),
            }
            st.relayout(&self.props);
        }
        self.bump();
        self.schedule_highlight();
    }

    pub(crate) fn expand_everything(&self) {
        {
            let mut st = self.st.borrow_mut();
            let gaps = st.model.hunks.len() + 1;
            for gap in 0..gaps {
                st.expansions.expand_gap(gap);
            }
            st.relayout(&self.props);
        }
        self.bump();
        self.schedule_highlight();
    }

    pub(crate) fn collapse_all(&self) {
        {
            let mut st = self.st.borrow_mut();
            st.expansions.clear();
            st.relayout(&self.props);
        }
        self.bump();
    }

    pub(crate) fn line_number_press(&self, side: DiffSide, line: usize) {
        self.cx.emit(DiffViewEvent::LineNumberPress(DiffLine {
            side,
            line: line as u32,
        }));
    }

    // Highlighting.

    fn load_syntaxes(&self) {
        if self.st.borrow().hint.is_empty() {
            return;
        }
        if Highlighter::ready() {
            self.st.borrow_mut().make_highlighters();
            self.schedule_highlight();
            return;
        }
        let dv = self.clone();
        spawn(async move {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = blocking(Highlighter::load).await;
            }
            #[cfg(target_arch = "wasm32")]
            {
                next_frame().await;
                Highlighter::load();
            }
            dv.st.borrow_mut().make_highlighters();
            dv.schedule_highlight();
        });
    }

    /// Highlights both sides up to the last lines in view (and on past
    /// them), off the engine thread; lines in view far past the parse are
    /// guessed first.
    pub(crate) fn schedule_highlight(&self) {
        if self.highlighting.replace(true) {
            return;
        }
        let dv = self.clone();
        spawn(async move {
            loop {
                let work = {
                    let rows = untracked(|| dv.visible_rows());
                    let mut st = dv.st.borrow_mut();
                    let st = &mut *st;
                    // The first and last line of each side in view.
                    let (mut old_lines, mut new_lines) = ((usize::MAX, 0), (usize::MAX, 0));
                    for row in &st.rows[rows.start.min(st.rows.len())..rows.end.min(st.rows.len())]
                    {
                        if let Row::Line { old, new, .. } = row {
                            for (line, lines) in [(old, &mut old_lines), (new, &mut new_lines)] {
                                if let Some(line) = *line {
                                    lines.0 = lines.0.min(line);
                                    lines.1 = lines.1.max(line);
                                }
                            }
                        }
                    }
                    let mut work = Vec::new();
                    for (index, (side, lines)) in
                        [(&mut st.old, old_lines), (&mut st.new, new_lines)]
                            .into_iter()
                            .enumerate()
                    {
                        let last = lines.1;
                        let first = lines.0.min(last);
                        let start = first.saturating_sub(GUESS_CONTEXT);
                        let start_offset = side.starts.get(start).copied().unwrap_or(0);
                        if let Some(h) = &mut side.highlighter {
                            let guess = (first > h.frontier() + GUESS_DISTANCE)
                                .then(|| h.guess(&side.rope, start, start_offset, last))
                                .flatten();
                            let job = h.job(&side.rope, last + LOOKAHEAD, SLICE);
                            if guess.is_some() || job.is_some() {
                                work.push((index, guess, job));
                            }
                        }
                    }
                    work
                };
                if work.is_empty() {
                    break;
                }
                let done = off_thread(move || {
                    work.into_iter()
                        .map(|(index, guess, job)| {
                            (index, guess.map(|g| g.run()), job.map(|j| j.run()))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
                let mut changed = false;
                {
                    let mut st = dv.st.borrow_mut();
                    let st = &mut *st;
                    for (index, guessed, done) in done {
                        let side = if index == 0 { &mut st.old } else { &mut st.new };
                        // A highlighter made since for other texts drops
                        // what was made for the one before.
                        let Some(h) = &mut side.highlighter else {
                            continue;
                        };
                        if let Some(guessed) = guessed {
                            changed |= !h.guessed(guessed).is_empty();
                        }
                        if let Some(done) = done {
                            changed |= !h.finish(done).0.is_empty();
                        }
                    }
                }
                if changed {
                    dv.hl_rev.update(|r| *r += 1);
                }
                #[cfg(target_arch = "wasm32")]
                next_frame().await;
            }
            dv.highlighting.set(false);
        });
    }
}

/// The texts, numbering, headers and file path of `text`'s first file.
#[allow(clippy::type_complexity)]
fn from_patch(text: &str) -> (String, String, Numbers, Vec<Header>, Option<String>) {
    let files = match patch::parse_patch(text) {
        Ok(files) => files,
        Err(e) => {
            log::warn!("DiffView: the patch is not a unified diff: {e}");
            return Default::default();
        }
    };
    let Some(file) = files.into_iter().next() else {
        return Default::default();
    };
    let (old, new, firsts) = file.texts();
    let mut headers = Vec::with_capacity(file.hunks.len());
    let (mut old_numbers, mut new_numbers) = (Vec::new(), Vec::new());
    let places = file.hunks.iter().zip(firsts).zip(file.text_starts());
    for ((hunk, (old_first, new_first)), (old_at, new_at)) in places {
        headers.push(Header {
            old: old_at,
            new: new_at,
            text: hunk.header.trim().to_string(),
        });
        // The files' lines count from 0 here, the numbers from 1.
        old_numbers.push((old_at, old_first + 1));
        new_numbers.push((new_at, new_first + 1));
    }
    let path = file.new_path.or(file.old_path);
    (old, new, (old_numbers, new_numbers), headers, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_spans() {
        let text = "a\n\tb\n";
        let starts = starts(text);
        assert_eq!(starts, vec![0, 2]);
        assert_eq!(line_of(text, &starts, 1), "\tb");
        assert_eq!(display_columns("\tb"), 5);
        let changed = vec![Range { start: 1, end: 2 }];
        let spans = spans("ab\tc", &[], &changed);
        let texts: Vec<_> = spans.iter().map(|s| (s.text.as_str(), s.changed)).collect();
        assert_eq!(texts, vec![("a", false), ("b", true), ("  c", false)]);
    }

    #[test]
    fn numbers_follow_a_patch() {
        let side = Side::new("a\nb\nc\n".to_string(), vec![(0, 10), (2, 40)]);
        assert_eq!(side.number(0), 10);
        assert_eq!(side.number(1), 11);
        assert_eq!(side.number(2), 40);
        let plain = Side::new("a\nb".to_string(), Vec::new());
        assert_eq!(plain.lines(), 2);
        assert_eq!(plain.number(1), 2);
    }
}
