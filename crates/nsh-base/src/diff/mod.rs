//! `DiffView`: the changes between two texts (`nsh/base`), as code review
//! tools show them: side by side or one over the other, the words changed
//! within lines highlighted, unchanged lines between changes collapsed
//! into separators that expand, highlighted with the editor's syntaxes.
//! `docs/dev/diff.md` has the design.

mod controller;
mod layout;
mod model;
mod parts;
mod patch;
mod view;

use nsh_core::ui::prelude::*;
pub use parts::{DiffViewClasses, DiffViewPart, DiffViewStyles};

use self::controller::Dv;

/// How a [`DiffView`] lays the two texts out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PropEnum)]
pub enum DiffLayout {
    /// The old text on the left, the new on the right, their lines aligned.
    #[default]
    Split,
    /// The new text, the lines deleted above where they were.
    Unified,
}

/// What a [`DiffView`] highlights within changed lines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PropEnum)]
pub enum DiffInline {
    /// Words, joining those one character apart, so a phrase reads as one
    /// change.
    #[default]
    WordAlt,
    /// Each word.
    Word,
    /// Characters.
    Char,
    /// Nothing: the lines' tints only.
    None,
}

/// How a [`DiffView`] marks changed lines beside their numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PropEnum)]
pub enum DiffIndicators {
    /// A bar at the gutter's edge.
    #[default]
    Bars,
    /// `+` and `−` before the text.
    Classic,
    /// Their backgrounds alone.
    None,
}

/// A side of a diff.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PropEnum)]
pub enum DiffSide {
    /// The old text.
    Old,
    /// The new text.
    #[default]
    New,
}

/// A change of a [`DiffView`]: lines of the old text that became lines of
/// the new one. Lines count from 0; a side's count is 0 for a pure
/// insertion or deletion (its start is then where the lines would be).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct DiffHunk {
    /// Its index among the changes.
    pub index: u32,
    /// The first old line it replaces.
    pub old_start: u32,
    /// How many old lines it replaces (deleted lines).
    pub old_lines: u32,
    /// The first new line it puts there.
    pub new_start: u32,
    /// How many new lines it puts there (added lines).
    pub new_lines: u32,
}

/// What a [`DiffView`]'s texts differ by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct DiffStats {
    /// Lines added.
    pub added: u32,
    /// Lines deleted.
    pub deleted: u32,
    /// Changes (hunks).
    pub hunks: u32,
}

/// A separator of a [`DiffView`] (`renderSeparator`): unchanged lines
/// collapsed between changes, or a patch's hunk header.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct DiffSeparator {
    /// Which gap of unchanged lines it collapses: 0 before the first
    /// change, `i` between changes `i - 1` and `i` (`expand(gap, ..)`).
    pub gap: u32,
    /// How many lines it hides (0 for a patch's hunk header).
    pub lines: u32,
    /// It is before the first change (it expands upward only).
    pub first: bool,
    /// It is after the last change (it expands downward only).
    pub last: bool,
    /// A patch's hunk header (what follows its `@@ … @@`: a function's
    /// name).
    #[prop(skip_none)]
    pub header: Option<SharedString>,
}

/// Which way an expansion of a separator shows lines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PropEnum)]
pub enum DiffExpand {
    /// Every line it hides.
    #[default]
    All,
    /// Lines above it: those after the change above (they show under that
    /// change).
    Up,
    /// Lines below it: those before the change below.
    Down,
}

/// The texts after a change was reverted or kept (`onChange`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct DiffChange {
    /// The old text.
    pub old_value: String,
    /// The new text.
    pub new_value: String,
}

/// A line of a side (`onLineNumberPress`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct DiffLine {
    /// The side.
    pub side: DiffSide,
    /// The line, from 0.
    pub line: u32,
}

/// The changes between two texts, as code review tools show them: side by
/// side (`layout="split"`) or one over the other (`"unified"`), the words
/// changed within lines highlighted, the unchanged lines between changes
/// collapsed into separators that expand, highlighted with the editor's
/// syntaxes. The texts are `oldValue` and `newValue`, or a file of a
/// unified `patch` (`git diff`'s, a pull request's). Changes revert or keep
/// one at a time through the ref (`revertHunk`, `acceptHunk`), which the
/// app's `renderHunkActions` calls; `onChange` reports the texts then.
#[derive(Clone, Copy, PartialEq, Props)]
pub struct DiffViewProps {
    /// The old text.
    pub old_value: Prop<Option<SharedString>>,
    /// The new text.
    pub new_value: Prop<Option<SharedString>>,
    /// A unified diff of one file (`git diff`, `diff -u`), shown without
    /// the files: its hunks' lines, numbered as in the files, each hunk
    /// under its header. Used when `oldValue` and `newValue` are not set.
    pub patch: Prop<Option<SharedString>>,
    /// The language, as the editor's `language` (a name, an extension or a
    /// file name); default plain text (with a `patch`, its file's).
    pub language: Prop<Option<SharedString>>,
    /// Side by side (default) or one over the other.
    #[prop(default)]
    pub layout: Prop<DiffLayout>,
    /// What is highlighted within changed lines.
    #[prop(default)]
    pub inline: Prop<DiffInline>,
    /// How changed lines are marked beside their numbers.
    #[prop(default)]
    pub indicators: Prop<DiffIndicators>,
    /// Unchanged lines shown around each change (default 3).
    #[prop(default = 3)]
    pub context_lines: Prop<u32>,
    /// Every unchanged line shows: nothing is collapsed.
    #[prop(default)]
    pub expand_all: Prop<bool>,
    /// The line numbers (default true).
    #[prop(default = true)]
    pub line_numbers: Prop<bool>,
    /// Changed lines are tinted (default true).
    #[prop(default = true)]
    pub background: Prop<bool>,
    /// The font family (default the theme's `font-mono`, else the
    /// system's monospaced font).
    pub font_family: Prop<Option<SharedString>>,
    /// The font size, in points (default 13).
    #[prop(default = Points(13.0))]
    pub font_size: Prop<Points>,
    /// The height of a row, in points (default 20).
    #[prop(default = Points(20.0))]
    pub line_height: Prop<Points>,
    /// The separators of collapsed lines, drawn by the app (by default a
    /// row of `⋯` and the count, which expands them all when pressed).
    pub render_separator: Prop<Option<Renderer<DiffSeparator>>>,
    /// Actions on a change (Revert, Keep), drawn by the app at the top
    /// right of each change in view; they call the ref's `revertHunk` /
    /// `acceptHunk` with the hunk's `index`.
    pub render_hunk_actions: Prop<Option<Renderer<DiffHunk>>>,
    /// Classes of the view's parts, after their own styles.
    #[prop(default)]
    pub classes: Prop<DiffViewClasses>,
    /// Inline styles of the view's parts, over their classes.
    #[prop(default)]
    pub styles: Prop<DiffViewStyles>,
}

/// The events of a [`DiffView`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum DiffViewEvent {
    /// A change was reverted or kept (`revertHunk`, `acceptHunk`): the
    /// texts now. Set them as `oldValue` and `newValue`.
    #[args(change)]
    Change(DiffChange),
    /// A line number was pressed (a comment's place).
    #[args(line)]
    LineNumberPress(DiffLine),
    /// The texts were diffed: the lines added and deleted and the changes,
    /// when they differ from those reported last (first when the view is
    /// made).
    #[args(stats)]
    StatsChange(DiffStats),
}

/// The methods of a [`DiffView`]'s ref.
pub struct DiffViewApi {
    dv: Dv,
}

#[methods]
impl DiffViewApi {
    /// The changes, in order.
    pub fn hunks(&self) -> Vec<DiffHunk> {
        self.dv.hunks()
    }

    /// The lines added and deleted, and the changes.
    pub fn stats(&self) -> DiffStats {
        self.dv.stats()
    }

    /// Scrolls change `index` into view (its first row at the top).
    pub fn go_to_hunk(&self, index: u32) {
        self.dv.go_to_hunk(index as usize);
    }

    /// Puts change `index`'s old lines back in the new text, and reports
    /// the texts (`onChange`).
    pub fn revert_hunk(&self, index: u32) {
        self.dv.revert_hunk(index as usize);
    }

    /// Makes change `index`'s new lines the old text's, and reports the
    /// texts (`onChange`).
    pub fn accept_hunk(&self, index: u32) {
        self.dv.accept_hunk(index as usize);
    }

    /// Shows lines separator `gap` hides: all of them, or `lines` (default
    /// 20) above or below it.
    pub fn expand(&self, gap: u32, direction: Option<DiffExpand>, lines: Option<u32>) {
        self.dv.expand(
            gap as usize,
            direction.unwrap_or_default(),
            lines.unwrap_or(20) as usize,
        );
    }

    /// Shows every unchanged line.
    pub fn expand_all(&self) {
        self.dv.expand_everything();
    }

    /// Collapses the unchanged lines again.
    pub fn collapse_all(&self) {
        self.dv.collapse_all();
    }
}

/// The changes between two texts, side by side or one over the other, the
/// words changed within lines highlighted and unchanged lines collapsed;
/// only the rows in view are made, so long files scroll as short ones do.
#[component(methods = DiffViewApi)]
pub fn DiffView(props: DiffViewProps, cx: Scope<DiffViewEvent>) -> impl IntoElement {
    let dv = Dv::new(props, cx);
    cx.methods(DiffViewApi { dv: dv.clone() });
    view::render(dv)
}
