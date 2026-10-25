//! The values an [`super::Editor`] and the app pass each other: positions,
//! ranges, selections, searches, diagnostics and decorations. Positions are
//! lines and UTF-16 columns from 0, as a language server's (LSP's
//! `Position`), so its diagnostics pass through as they come.

use nsh_core::ui::{Style, prelude::*};

/// A position in an editor's text: a line and a column, both from 0. The
/// column counts UTF-16 code units, as JS strings and language servers do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, JsObject)]
#[non_exhaustive]
pub struct EditorPosition {
    /// The line, from 0.
    pub line: u32,
    /// The UTF-16 code units before it in its line.
    pub column: u32,
}

impl EditorPosition {
    /// Line `line`, column `column`.
    pub fn new(line: u32, column: u32) -> EditorPosition {
        EditorPosition { line, column }
    }
}

/// A range of an editor's text, from `start` to `end` (`end` not
/// included).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct EditorRange {
    /// Where it starts.
    pub start: EditorPosition,
    /// Where it ends, not included.
    pub end: EditorPosition,
}

impl EditorRange {
    /// From `start` to `end`.
    pub fn new(start: EditorPosition, end: EditorPosition) -> EditorRange {
        EditorRange { start, end }
    }
}

/// A selection of an editor: from `anchor`, where it started, to `head`,
/// where the caret is. Equal, it is a caret.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct EditorSelection {
    /// Where the selection started.
    pub anchor: EditorPosition,
    /// Where the caret is.
    pub head: EditorPosition,
}

impl EditorSelection {
    /// From `anchor` to `head`.
    pub fn new(anchor: EditorPosition, head: EditorPosition) -> EditorSelection {
        EditorSelection { anchor, head }
    }

    /// A caret at `at`.
    pub fn caret(at: EditorPosition) -> EditorSelection {
        EditorSelection {
            anchor: at,
            head: at,
        }
    }
}

/// What an editor finds (`search`): its matches are highlighted, and
/// `findNext` / `findPrevious` / `replace` / `replaceAll` act on them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct EditorSearch {
    /// The text to find (a regular expression with `regex`). Empty finds
    /// nothing.
    pub query: SharedString,
    /// Matches only text of the same case (default: any case).
    #[prop(default)]
    pub case_sensitive: bool,
    /// Matches only whole words.
    #[prop(default)]
    pub whole_word: bool,
    /// `query` is a regular expression (Rust's `regex` syntax; a
    /// replacement refers to its groups as `$1` and `${name}`).
    #[prop(default)]
    pub regex: bool,
    /// Matches only inside what was selected when the search turned to the
    /// selection (the carets' lines when nothing was): find in selection.
    /// The scope stays while this stays on, carried through edits.
    #[prop(default)]
    pub in_selection: bool,
}

impl EditorSearch {
    /// Finds `query` as it is written, in any case.
    pub fn new(query: impl Into<SharedString>) -> EditorSearch {
        EditorSearch {
            query: query.into(),
            ..Default::default()
        }
    }
}

/// What an editor's search found: how many matches, and which of them the
/// selection is on (`onSearchChange`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct EditorSearchResult {
    /// How many matches there are (at most 10 000 are counted).
    pub count: u32,
    /// The match the primary selection is on, from 0 (`null`: none).
    #[prop(skip_none)]
    pub current: Option<u32>,
    /// The query is not a valid regular expression: why.
    #[prop(skip_none)]
    pub error: Option<SharedString>,
}

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PropEnum)]
pub enum EditorDiagnosticSeverity {
    /// An error.
    #[default]
    Error,
    /// A warning.
    Warning,
    /// Information.
    Info,
    /// A hint.
    Hint,
}

/// A problem in an editor's text, as a language server or linter reports
/// it: its range is marked (the `diagnostic` parts) and its line's number
/// too.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, JsObject)]
#[non_exhaustive]
pub struct EditorDiagnostic {
    /// What it is about. An empty range marks the character after it (or
    /// the end of its line).
    pub range: EditorRange,
    /// How serious it is (default `error`).
    #[prop(default)]
    pub severity: EditorDiagnosticSeverity,
    /// What it says.
    pub message: SharedString,
    /// Who reported it (`"rustc"`, `"eslint"`).
    #[prop(skip_none)]
    pub source: Option<SharedString>,
    /// Its code (`"E0308"`).
    #[prop(skip_none)]
    pub code: Option<SharedString>,
}

/// A range of an editor's text the app styles: semantic highlights, a
/// statement being run, the changes of a review. Its classes and style
/// apply to the range's text, and its `lineClass` / `lineStyle` to the
/// rows of its lines, behind the text.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub struct EditorDecoration {
    /// What it styles.
    pub range: EditorRange,
    /// Tailwind classes of the range's text (`text-*`, `font-*`,
    /// `underline`, `bg-*`).
    #[prop(skip_none)]
    pub class: Option<SharedString>,
    /// The inline style of the range's text, over `class`.
    #[prop(skip_none)]
    pub style: Option<Style>,
    /// Tailwind classes of the rows of the lines it covers (a background).
    #[prop(skip_none)]
    pub line_class: Option<SharedString>,
    /// The inline style of those rows, over `lineClass`.
    #[prop(skip_none)]
    pub line_style: Option<Style>,
}
