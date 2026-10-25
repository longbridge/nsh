//! `Editor`: a code editor (`nsh/base`), the port of GPUI Kit's `Editor`
//! to NSH's primitives.
//!
//! The editor draws its text itself, as texts of display rows, and keeps
//! its own model: a rope ([`buffer`]), selections (several carets),
//! movements, editing commands, undo, a display map (tabs, soft wrap,
//! folds), incremental syntax highlighting, search, diagnostics and
//! decorations. Keys reach it through the keymap while it has the focus;
//! typed text and input methods through a native text input kept at the
//! primary caret, which holds only what was just typed. `docs/dev/editor.md`
//! has the design.

mod buffer;
mod commands;
pub(crate) mod controller;
mod data;
mod diagnostics;
mod display;
mod fold;
pub(crate) mod highlight;
mod keymap;
pub(crate) mod language;
mod movement;
mod parts;
mod search;
pub(crate) mod state;
mod types;
mod view;

pub use data::{
    EditorDecoration, EditorDiagnostic, EditorDiagnosticSeverity, EditorPosition, EditorRange,
    EditorSearch, EditorSearchResult, EditorSelection,
};
use nsh_core::ui::prelude::*;
pub use parts::{EditorClasses, EditorPart, EditorStyles};

use self::controller::Ed;

/// A code editor: syntax highlighting, line numbers, several carets,
/// folding, soft wrap, search and replace, diagnostics and decorations,
/// with the keys of code editors. The text is controlled (`value` and
/// `onChange`) or the editor's own (`defaultValue`). It has a look of its
/// own built from the theme's tokens (a monospaced font, muted line
/// numbers, a faint active line, the built-in syntax colors), every part of
/// which the app restyles (`classes`, `styles`).
#[derive(Clone, Copy, PartialEq, Props)]
pub struct EditorProps {
    /// The text, when the app controls it (set it again from `onChange`).
    /// A new value replaces the text as an edit undo takes back.
    pub value: Prop<Option<SharedString>>,
    /// The first text, when the editor keeps its own.
    pub default_value: Prop<Option<SharedString>>,
    /// The language: a name (`"rust"`, `"typescript"`), an extension
    /// (`"tsx"`) or a file name (`"main.go"`, `"Dockerfile"`). It decides
    /// the highlighting, comments, brackets and indentation (default plain
    /// text).
    pub language: Prop<Option<SharedString>>,
    /// Shown while the text is empty.
    pub placeholder: Prop<Option<SharedString>>,
    /// The text can be selected and copied but not changed.
    #[prop(default)]
    pub read_only: Prop<bool>,
    /// The editor takes the focus when it appears.
    #[prop(default)]
    pub auto_focus: Prop<bool>,
    /// The gutter's line numbers (default true).
    #[prop(default = true)]
    pub line_numbers: Prop<bool>,
    /// Long lines wrap at the editor's width, rather than scrolling
    /// sideways.
    #[prop(default)]
    pub soft_wrap: Prop<bool>,
    /// The columns of a tab and of an indentation level (default the
    /// language's: 4, 2 for web languages).
    pub tab_size: Prop<Option<u32>>,
    /// Indents with tabs (default the language's: Go and Makefiles do).
    pub hard_tabs: Prop<Option<bool>>,
    /// Lines indented under a line fold away with the gutter's markers
    /// (default true).
    #[prop(default = true)]
    pub folding: Prop<bool>,
    /// Brackets and quotes close as they are typed (default true).
    #[prop(default = true)]
    pub auto_close: Prop<bool>,
    /// A new line keeps the indentation, and indents after an opening
    /// bracket (default true).
    #[prop(default = true)]
    pub auto_indent: Prop<bool>,
    /// The rows of the lines with a caret are highlighted (the
    /// `activeLine` part; default true).
    #[prop(default = true)]
    pub active_line: Prop<bool>,
    /// The guides of indentation (default true).
    #[prop(default = true)]
    pub indent_guides: Prop<bool>,
    /// The text scrolls on past its last line, up to a screen (default
    /// true).
    #[prop(default = true)]
    pub scroll_beyond_last_line: Prop<bool>,
    /// The font family (default the theme's `font-mono`, else the
    /// system's monospaced font). The editor measures its text in it, so
    /// set the font here rather than with classes.
    pub font_family: Prop<Option<SharedString>>,
    /// The font size, in points (default 13).
    #[prop(default = Points(13.0))]
    pub font_size: Prop<Points>,
    /// The height of a row, in points (default 20).
    #[prop(default = Points(20.0))]
    pub line_height: Prop<Points>,
    /// What to find: its matches are highlighted, and `findNext`,
    /// `findPrevious`, `replace` and `replaceAll` act on them.
    pub search: Prop<Option<EditorSearch>>,
    /// Problems to mark (a language server's or a linter's), carried
    /// through edits until the app sets new ones.
    #[prop(default)]
    pub diagnostics: Prop<Vec<EditorDiagnostic>>,
    /// Ranges the app styles (semantic highlighting, a statement being
    /// run), carried through edits until the app sets new ones.
    #[prop(default)]
    pub decorations: Prop<Vec<EditorDecoration>>,
    /// Classes of the editor's parts, after their own styles.
    #[prop(default)]
    pub classes: Prop<EditorClasses>,
    /// Inline styles of the editor's parts, over their classes.
    #[prop(default)]
    pub styles: Prop<EditorStyles>,
    /// What screen readers call the editor.
    pub accessibility_label: Prop<Option<SharedString>>,
}

/// The events of an [`Editor`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum EditorEvent {
    /// The text changed (the whole new text).
    #[args(value)]
    Change(SharedString),
    /// The selections changed: every one, the primary (newest) last.
    #[args(selections)]
    SelectionChange(Vec<EditorSelection>),
    /// The editor took the focus.
    Focus,
    /// The editor lost the focus.
    Blur,
    /// The search's matches changed (a new `search`, an edit, a move to
    /// another match).
    #[args(result)]
    SearchChange(EditorSearchResult),
    /// A line number was pressed (a breakpoint's place): the line, from 0.
    #[args(line)]
    LineNumberPress(u32),
    /// The secondary button was pressed over the text (a right click,
    /// Control-click on Apple platforms): where, in the text and in the
    /// root view, to open a menu at (`Menu.popup(items, { x, y })`).
    #[args(event)]
    ContextMenu(EditorContextMenuEvent),
}

/// Where a context menu was asked for in an [`Editor`].
#[derive(Debug, Clone, Copy, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub struct EditorContextMenuEvent {
    /// The position in the text under the pointer.
    pub position: EditorPosition,
    /// The pointer's x in the root view.
    pub x: Points,
    /// The pointer's y in the root view.
    pub y: Points,
}

/// A command of an [`Editor`] (`run(command)`), as its keys run it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PropEnum)]
pub enum EditorCommand {
    /// Left one character.
    MoveLeft,
    /// Right one character.
    MoveRight,
    /// Up one row.
    MoveUp,
    /// Down one row.
    MoveDown,
    /// To the start of the previous word.
    MoveWordLeft,
    /// To the end of the next word.
    MoveWordRight,
    /// To the line's first non-blank character, then its start.
    MoveLineStart,
    /// To the line's end.
    MoveLineEnd,
    /// Up a page.
    MovePageUp,
    /// Down a page.
    MovePageDown,
    /// To the start of the text.
    MoveDocumentStart,
    /// To the end of the text.
    MoveDocumentEnd,
    /// Extends the selections left one character.
    SelectLeft,
    /// Extends the selections right one character.
    SelectRight,
    /// Extends the selections up one row.
    SelectUp,
    /// Extends the selections down one row.
    SelectDown,
    /// Extends the selections to the start of the previous word.
    SelectWordLeft,
    /// Extends the selections to the end of the next word.
    SelectWordRight,
    /// Extends the selections to the line's start.
    SelectLineStart,
    /// Extends the selections to the line's end.
    SelectLineEnd,
    /// Extends the selections up a page.
    SelectPageUp,
    /// Extends the selections down a page.
    SelectPageDown,
    /// Extends the selections to the start of the text.
    SelectDocumentStart,
    /// Extends the selections to the end of the text.
    SelectDocumentEnd,
    /// To the bracket matching the one at the caret.
    MoveToMatchingBracket,
    /// Selects the whole text.
    SelectAll,
    /// Selects the carets' lines (again: the next line too).
    SelectLine,
    /// Selects the word at each caret.
    SelectWord,
    /// Adds a selection at the next occurrence of the selected text (or
    /// selects the word at the caret).
    SelectNextOccurrence,
    /// Selects every occurrence of the selected text.
    SelectAllOccurrences,
    /// Adds a caret on the row above.
    AddCaretAbove,
    /// Adds a caret on the row below.
    AddCaretBelow,
    /// Back to one selection, then to a caret.
    SingleSelection,
    /// Deletes the selection, else the character before the caret.
    DeleteBackward,
    /// Deletes the selection, else the character after the caret.
    DeleteForward,
    /// Deletes to the start of the previous word.
    DeleteWordLeft,
    /// Deletes to the end of the next word.
    DeleteWordRight,
    /// Deletes to the line's start.
    DeleteToLineStart,
    /// Deletes to the line's end.
    DeleteToLineEnd,
    /// Deletes the carets' lines.
    DeleteLine,
    /// A new line at the caret, indented.
    Newline,
    /// A new line below the caret's.
    NewlineBelow,
    /// A new line above the caret's.
    NewlineAbove,
    /// Indents the selected lines (a tab at a caret).
    Indent,
    /// Outdents the selected lines.
    Outdent,
    /// Comments the selected lines out, or back in.
    ToggleComment,
    /// Moves the selected lines up.
    MoveLinesUp,
    /// Moves the selected lines down.
    MoveLinesDown,
    /// Copies the selected lines above them.
    DuplicateLinesUp,
    /// Copies the selected lines below them.
    DuplicateLinesDown,
    /// Joins the selected lines (the next line to the caret's).
    JoinLines,
    /// Swaps the characters around the caret.
    TransposeCharacters,
    /// Takes back the last edit.
    Undo,
    /// Does again the edit undo took back.
    Redo,
    /// Folds the lines under the caret's line.
    Fold,
    /// Unfolds the caret's line.
    Unfold,
    /// Folds every fold.
    FoldAll,
    /// Unfolds everything.
    UnfoldAll,
    /// Selects the next match of the search.
    FindNext,
    /// Selects the previous match of the search.
    FindPrevious,
    /// Copies the selections (the carets' lines).
    Copy,
    /// Cuts the selections (the carets' lines).
    Cut,
    /// Pastes the clipboard's text.
    Paste,
}

/// The methods of an [`Editor`]'s ref.
pub struct EditorApi {
    ed: Ed,
}

#[methods]
impl EditorApi {
    /// Takes the focus.
    pub fn focus(&self) {
        self.ed.focus();
    }

    /// Gives the focus up.
    pub fn blur(&self) {
        self.ed.blur();
    }

    /// The text.
    pub fn value(&self) -> String {
        self.ed.text()
    }

    /// Replaces the text, as an edit undo takes back.
    pub fn set_value(&self, value: String) {
        self.ed.set_text(&value);
    }

    /// Types `text` at each selection (as typing it would, without the
    /// closing of brackets).
    pub fn insert(&self, text: String) {
        self.ed.insert_plain(&text);
    }

    /// The selections, the primary (newest) last.
    pub fn selections(&self) -> Vec<EditorSelection> {
        self.ed.selections()
    }

    /// Sets the selections (the last one primary) and shows the primary
    /// one.
    pub fn set_selections(&self, selections: Vec<EditorSelection>) {
        self.ed.set_selections(&selections);
    }

    /// Runs `command`, as its keys would.
    pub fn run(&self, command: EditorCommand) {
        self.ed.run_public(command);
    }

    /// Replaces the match the primary selection is on with `text` and
    /// selects the next one (`$1`, `${name}` refer to a regular
    /// expression's groups).
    pub fn replace(&self, text: String) {
        self.ed.replace_current(&text);
    }

    /// Replaces every match with `text`, as one edit: how many.
    pub fn replace_all(&self, text: String) -> u32 {
        self.ed.replace_all(&text)
    }

    /// Folds the lines under line `line` (from 0).
    pub fn fold(&self, line: u32) {
        self.ed.fold(line as usize, Some(true));
    }

    /// Unfolds line `line`.
    pub fn unfold(&self, line: u32) {
        self.ed.fold(line as usize, Some(false));
    }

    /// Scrolls line `line` (from 0) into view.
    pub fn scroll_to_line(&self, line: u32) {
        self.ed.scroll_to_line(line as usize);
    }

    /// Whether there is an edit to take back.
    pub fn can_undo(&self) -> bool {
        self.ed.can_undo()
    }

    /// Whether there is an edit to do again.
    pub fn can_redo(&self) -> bool {
        self.ed.can_redo()
    }
}

/// A code editor: the text drawn row by row with the built-in syntax
/// colors (or the app's decorations), a gutter of line numbers and fold
/// markers, carets and selections, with the keys and commands of code
/// editors (several carets, moving and duplicating lines, comments,
/// indentation, brackets that close), folding, soft wrap, search and
/// replace, and diagnostics. Only the rows in view are made, so long files
/// scroll as short ones do.
#[component(methods = EditorApi)]
pub fn Editor(props: EditorProps, cx: Scope<EditorEvent>) -> impl IntoElement {
    let ed = Ed::new(props, cx);
    cx.methods(EditorApi { ed: ed.clone() });
    view::render(ed)
}
