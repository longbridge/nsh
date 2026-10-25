# Editor

`Editor` (`nsh/base`, `crates/nsh-base/src/editor/`) is a code editor: syntax highlighting, line numbers, several
carets, folding, soft wrap, find and replace, diagnostics and decorations, with the keys of code editors. It is the
port of GPUI Kit's `Editor` (`gpui-kit/crates/base/src/input/`) to NSH's primitives: the editor draws its text itself,
as one `Text` per display row, and keeps its own model in Rust. It owns behavior (editing, selections, keys, input
methods, folding, search, accessibility) and, as `TextView` does, has a default look of its own built from the
theme's tokens (the second exception to "No styles in NSH", `AGENTS.md`), every part of which the app restyles. The
user-facing page is `website/elements/editor.md`; how each module works inside is in its `//!` docs.

| File | What |
|---|---|
| `mod.rs` | The component: `EditorProps`, `EditorEvent`, `EditorContextMenuEvent`, `EditorCommand` (what `run` takes), `EditorApi` (the ref's methods) |
| `data.rs` | The values the editor and the app pass each other: `EditorPosition` (line, UTF-16 column), `EditorRange`, `EditorSelection`, `EditorSearch`, `EditorSearchResult`, `EditorDiagnostic` / `EditorDiagnosticSeverity`, `EditorDecoration` |
| `parts.rs` | The parts the app styles: one `parts!` list makes `EditorClasses`, `EditorStyles` (each a `PartStyle`, as `TextView`'s) and `EditorPart` |
| `controller.rs` | `Ed`: the state, the revisions bindings subscribe to, and what props, keys, typing, presses, the clipboard and the ref do to them; scrolling the caret into view; scheduling the highlighter |
| `state.rs` | `State`: what the editor holds between frames, outside the reactive graph (buffer, selections, display map, highlighter, marks, matches); what the view draws of each row (`row_content`, `gutter_row`, the rectangles of selections, matches, brackets, diagnostics, indent guides) |
| `view.rs` | The view: the scroll views, the gutter, the rows' box and its layers, the sink |
| `hooks.rs` | What language features are built on: hovers (`onHover` / `onHoverEnd`), `onTextPress`, the ref's geometry and scrolling (`caretBounds`, `rangeBounds`, `revealPosition`, `goToLine`, ...), where the editor is in the root view |
| `buffer.rs` | The text: a `ropey::Rope`, edits as change lists, undo / redo with grouping, UTF-16 positions, the lines each change touched (`LineSpan`) |
| `types.rs` | `Point`, `Selection`, `Selections` (the primary one, the newest, kept apart), the trait `RowLayout` the moves use |
| `movement.rs` | Graphemes, words and their parts (camelCase, snake_case), paragraphs, smart Home / End, vertical moves keeping their x, matching and enclosing brackets |
| `commands.rs` | `Command`, `Editing`: every editing command at every selection, typing (auto-close, step-over, outdent), paste, copy, cut; `commands/selecting.rs` (the previous occurrence, splitting into lines) and `commands/transforms.rs` (sorting lines, case, trailing whitespace, block comments) |
| `select.rs` | The selection history (`undoSelection`), expanding and shrinking selections, block selections, the scope of a search in the selection |
| `keymap.rs` | The default key bindings as data, in `nsh/keymap`'s keystroke syntax, Apple's and the others' |
| `language.rs` | `LanguageConfig`: per-language comments, brackets, auto-close pairs, word characters, indentation, tab size and the syntax hint; the lookup by name, alias, extension or file name |
| `display.rs` | `DisplayMap`: tabs, measured widths, soft wrap and folds as display rows |
| `fold.rs` | Where folds can start, by indentation |
| `search.rs` | Find and replace on `regex`: matches, replacements with groups, the next and previous match |
| `diagnostics.rs` | `Mark<T>`: the app's ranges resolved to byte ranges once and carried through edits |
| `highlight.rs` | `Highlighter`: incremental syntect highlighting, line by line (feature `highlight`) |
| `../syntax.rs` | What `TextView`'s code blocks and the editor share: `Token`, Allsum's palette, the syntaxes and the scope → token mapping |

## How it works

### The model

- **Offsets are byte offsets of the UTF-8 text**, on char boundaries, everywhere inside the editor. Lines end with
  `\n` only: the buffer turns `\r\n` and `\r` into `\n` as text comes in, so a text of `n` line breaks has `n + 1`
  lines. The JS API speaks positions (`EditorPosition`: a line and a UTF-16 column from 0, as JS strings and LSP's
  `Position` do), converted at the edge (`Buffer::position` / `offset_of`, clamped), so a language server's
  diagnostics pass through as they come.
- **An edit is a change list.** `Buffer::edit` takes the edits of one command at once (ranges of the text as it is,
  sorted, not overlapping) with the selections before and after, and returns the `Change`s applied; everything that
  follows the text (the display map, the highlighter, marks, matches) is brought up to date from them in
  `State::changed`, through the one `LineSpan` the list touched.
- **Undo** groups as editors do: runs of typing at one place merge into one step while each edit starts where the
  previous ended, a whitespace typed after a word starts a new step, deleting merges likewise, and a move, a press or
  `setSelections` ends a group (`break_undo_group`). Each step restores the selections it had. At most 1000 steps
  are kept.
- **Selections** are a list with a primary one (the newest: the last one `onSelectionChange` reports and `selections()`
  returns), merged when they overlap. Every command runs at every selection (`commands.rs` gathers each command's edits
  as a plan, applies them in one `Buffer::edit` and carries each selection through all the edits). Each move records
  the selections it leaves (`select.rs`, at most 100), which `undoSelection` / `redoSelection` go back and forth
  through; an edit forgets them, as their offsets no longer mean the same text. `expandSelection` grows each selection
  to the smallest unit around it that is larger, without a syntax tree: the word, a string's contents (the language's
  `string_quotes`, paired on the line), the string, a bracket pair's contents, the pair, the line's text, the whole
  lines, the text; `shrinkSelection` retraces those steps while nothing else moved the selections.
- **The text is controlled or the editor's own.** `value` replaces the text as one undoable edit that changes only
  what differs (the common prefix and suffix stay, so carets and folds away from the change stay put); the value
  `onChange` just reported is recognized and costs nothing. `defaultValue` is the first text of an uncontrolled
  editor.
- **The language** (`language.rs`, static data) is found by name, alias, extension or file name, case-insensitive
  (`"rust"`, `"rs"`, `"main.rs"`, `"tsx"`, `"Dockerfile"`); anything else is plain text. It decides the comment
  tokens, brackets, auto-close pairs, extra word characters (`$` in JS), whether lines indent after (an opening
  bracket at the end, Python's `:`, Ruby's `do`) and outdent (a closing bracket, `end`), and the default tab size and
  hard tabs (`tabSize` and `hardTabs` override them).

### The display map

`DisplayMap` lays the text out in display rows: one vector of rows, with the first row of every line beside it, so
line → row is an index and row → line a field. Tabs advance to the next stop; widths come from the editor's font,
measured per char with the platform's text engine (`measure_text`) and cached, and summed along a row (exact for
monospaced fonts, close for others). Without soft wrap a row is a line and nothing is measured while laying out (a
row's width is measured when the scroll width first needs it; above 20 000 rows only the rows in view are measured,
as they show, and the scroll width grows to the widest seen), so 200 000 lines lay out in about 30 ms. With
`softWrap`, a line breaks after whitespace where the row would overflow, else between graphemes, and its continuation
rows start at its indentation unless that is over half the width. A folded line shows as one row and the lines it
hides as none. An edit lays out only the lines of its `LineSpan` and shifts the rows after them.

### Rendering and virtualization

The view is a vertical `Scroll` of a row of the gutter and the text area (a horizontal `Scroll` without soft wrap).
Only the rows in view are made, plus 8 on each side (`OVERSCAN`), and at least 40 rows' height before the viewport is
known: the row list is a memo of the visible range, and `each` keeps the views of rows that stay in view, so
scrolling makes only the rows that come in. Each row is an absolutely placed view holding one `Text` whose spans are
the row's runs of one token and one set of decorations (`State::row_content`). The rows' box draws, from back to
front: the active lines' backgrounds, indent guides, search matches, selections, the matching brackets, the rows,
diagnostics' marks, the placeholder, the carets, the composition and the sink. Each layer is a `dynamic` over the
revisions it depends on.

Revisions (`Signal<u64>` in `Ed`) keep a change to the parts that depend on it: `text_rev` (the text or its layout),
`hl_rev` (highlighting progressed), `sel_rev` (selections), `marks_rev` (diagnostics, decorations, matches). The state
itself lives in an `Rc<RefCell<State>>` outside the reactive graph.

### Typed text and input methods

A native `Input` (the sink), invisible, 1 point wide and a row tall, is kept at the primary caret, so input methods
open their candidate windows there and soft keyboards show. It holds only what was just typed: each `onChange` is
compared with what it held, and the difference becomes Backspaces and typed text at every selection (`sink_changed`),
which is how autocorrection and input methods that replace their text work. While an input method composes, its text
is drawn at the caret over the text (the `composition` part, underlined) and the carets hide; the commit arrives as
typed text. The sink is emptied when the caret moves (unless a composition is under way) and once it holds 256 bytes.
A paste reaches the sink as a paste event (`PasteBehavior::Report`) and goes through `Editing::paste`; text copied
from carets (whole lines) pastes back as whole lines.

### Keys

The bindings of `keymap.rs` are registered with `on_key_with` in an effect that runs while the editor has the focus
(`focused`), so arrows and Tab reach other inputs when it does not. They are data: `default_bindings(apple)` gives
macOS and iOS their conventions (Option moves by words, Command to line and text ends, Cocoa's Emacs keys) and the
others theirs (Control by words, Home / End, Control+Home / End), on a common VS Code set. The commands the
controller runs itself (find next / previous, folding, `findWithSelection`, the selection history, expanding) are bound
from `editor_bindings(apple)`, as the `EditorCommand`s `run` takes: VS Code's keys (`cmd+g` / `cmd+shift+g` on Apple
platforms and `f3` / `shift+f3` everywhere, `cmd+alt+[` / `]` or `ctrl+shift+[` / `]` to fold, `cmd+e`, `cmd+u` or
`ctrl+u`, `ctrl+shift+cmd+right` / `left` or `shift+alt+right` / `left`). Copy and cut (`mod+c`, `mod+x`) are the
controller's, through the clipboard. `run(command)` runs the same commands from the app, plus those that have no
default keys (fold all, sorting lines, changing case, trimming trailing whitespace, paragraph moves, paste from the
clipboard), for the app's own bindings and menus.

### Presses

On the desktops a press (`PressDown`) places the caret, a double press selects a word, a triple press a line, Shift
extends the primary selection, Alt (Option) adds a selection; dragging extends what the press selected by its unit
(`drag_to`). An Alt+Shift press (on Linux also Control+Alt, where window managers keep Alt drags) drags a block: one
selection per display row between the press and the pointer, each from the press's x to the pointer's (`select::block`),
so the block keeps its columns past the end of short rows, which select what they have; the pointer's row is primary. On touch screens a tap places the caret. A secondary press reports `onContextMenu` with the position in
the text and the point in the root view, for the app's `Menu.popup`. A press on a line number reports
`onLineNumberPress` (a breakpoint's place); a press on a fold marker or a fold's placeholder toggles the fold.

### Language features

The editor draws no completion menu, hover card or code action list: it gives the app what gpui-base's `lsp` hooks
use, and the app shows its own popups (`hooks.rs`). The mouse resting 150 ms (`HOVER_DELAY`) over a character reports
`onHover`: the character's position and UTF-16 offset, its word's range, the pointer and the word's box in the root
view, the modifiers and the diagnostics there. While a hover is shown, moving inside its word reports nothing and
moving to another word reports the next one at once; leaving the text, an edit, a new layout or scrolling reports
`onHoverEnd`. One task waits for the pointer to rest, whatever the number of moves. A press on a character reports
`onTextPress` (position, word, modifiers, clicks) after the editor placed its caret: Mod-click is the app's go to
definition. The ref places popups and moves: `caretBounds()` and `rangeBounds(range)` give boxes in the editor (its
scrolling applied) and in the root view, `visibleRange()` what the viewport shows, `revealPosition` / `goToLine`
scroll (centered on request), `offsetToPosition` / `positionToOffset` convert UTF-16 offsets of `value()`.

Where the text is in the root view: the editor and its rows' box are measured (`ElementRef::measure`) after their
layouts and when the gutter changes width, and every pointer event over the rows tells where they are then (its
`pagePosition` less its `position`). The rows move in the editor only with its own scrolling, which the controller
knows, so a box is placed with what was last seen and the scrolling since: an editor moved by a scroll view around it
is placed right again on the next pointer event or layout.

### Highlighting

`highlight.rs` parses with syntect (the syntaxes and scope mapping of `crate::syntax`, shared with `TextView`), line by
line. The tokens stay on the engine thread; the parser's state is kept every 16 lines (and on lines that kept one
before an edit). An edit marks the lines from its first one dirty; parsing starts at the nearest state kept before it
and stops as soon as a line ends in the state the next line started with before, so typing in a function costs a few
lines. Only an edit that changes what follows (opening a block comment or a string) parses on. Parsing runs off the
engine thread: the controller hands a `Job` (a snapshot of the rope, the state to start from, 25 ms of work) to a
blocking thread, applies what it returns and hands the next, up to 1000 lines past the last one in view. Edits made
while a job is out are kept and replayed on its result, which is dropped from the first edited line on. When the lines
in view are more than 500 lines past the parse (a jump to the end of a long file), a `Guess` parses them from a fresh
state, starting 40 lines before them, and shows that until the parse from the top reaches them. Lines not parsed again
yet keep the tokens they had, so colors do not flicker. The syntaxes load on a blocking thread the first time a
language needs them. On WebAssembly there are no threads: jobs run in place, 4 ms per frame. Without feature
`highlight`, the text is plain.

`onChange` carries the text as a `SharedString`, built once per version of the buffer: the same text is not built
again for the echo of a controlled `value`. Building it is the one cost of an edit that grows with the text.

### Search

`search` is a prop: its matches (at most 10 000) are found again on every change of it and every edit, highlighted
(`searchMatch`, `currentSearchMatch` for the one the primary selection is on), and reported with `onSearchChange`
(`{ count, current, error }`). A plain query is escaped into a regular expression, so both kinds run on `regex`:
Unicode case folding, linear time whatever the query, `^` / `$` at every line's start and end, whole words by word
boundaries. `findNext` / `findPrevious` (commands) select the next / previous match from the primary selection,
wrapping; `replace(text)` replaces the current match and selects the next, `replaceAll(text)` replaces every match as
one edit and returns how many. Replacements expand `$1` and `${name}` for regular expressions. `inSelection` keeps
the matches inside a scope: what was selected when the search turned to the selection (the carets' lines when nothing
was), kept while it stays on and carried through edits as marks are (`State::set_search`). The find bar is the app's;
`findWithSelection` (`cmd+e`) reports the selected text (a caret: its word) with `onFindWithSelection` for it to take
as its query.

### Diagnostics and decorations

`diagnostics` and `decorations` are resolved into byte ranges once when the app sets them (`diagnostics::resolve`)
and carried through every edit (`adjust`, through `map_offset`), so a mark stays on its text while the user types and
the linter has not answered yet. A diagnostic draws a 2 point mark under its range, in its severity's part; an empty
range marks the character after it. A decoration's `class` / `style` style its text (the spans are cut at its
bounds), its `lineClass` / `lineStyle` the rows of its lines behind the text.

### Folds

Folds are by indentation, as VS Code's indentation folding (`fold.rs`): a line followed by lines indented deeper
starts a fold that hides them; blank lines inside join it, and the line that ends the block at the start line's
indentation (a closing bracket, `end`) stays visible. Where folds can start is computed once per edit, when the gutter
first asks, in one pass over the rope's chunks (about 4 ms for 200 000 lines). Folding moves carets inside the fold to the end of its first line; a caret moving into a fold (a move, a
search match, `scrollToLine`) unfolds it. `folding={false}` removes the markers and the folds.

## Parts and their defaults

The defaults are the theme's tokens (`crate::theme()`), so a theme and the dark appearance reach them; a part's
`classes` come after its own styles, its `styles` over its classes.

| Part | Default |
|---|---|
| root | `foreground` text in the editor's font: `fontFamily` (default the theme's `font-mono`, else the system's `monospace`), 13 points, rows of 20 points |
| `gutter` | 12 points before the numbers, 4 after the markers |
| `lineNumber` | Right-aligned, as wide as the widest number (at least 2 digits), `muted-foreground` |
| `activeLineNumber` | `foreground` |
| `foldMarker` | `⌄` (open) or `›` (folded), a row's height wide, `muted-foreground` |
| `content` | 4 points before the text |
| `line` | Every row (nothing by default) |
| `activeLine` | `muted` at 60 % behind the rows of lines with a caret |
| `selection` | `primary` at 22 % |
| `caret` | 2 points wide, `foreground`, blinking every 530 ms while focused |
| `composition` | Underlined, on `background` |
| `searchMatch` / `currentSearchMatch` | Amber (`#fbbf24` / `#d97706` dark) at 25 % / 55 % |
| `matchingBracket` | A 1 point border, `muted-foreground` at 60 %, while focused |
| `indentGuide` | A 1 point line of `border` per level |
| `foldPlaceholder` | ` ⋯ ` after a folded line, `muted-foreground` |
| `placeholder` | `muted-foreground` |
| `diagnosticError` / `Warning` / `Info` / `Hint` | A 2 point line: `destructive` / amber / blue / `muted-foreground` |

Syntax colors are `crate::syntax`'s `Token` styles, Allsum's palette in light and dark (the table in `text-view.md`
"Syntax highlighting"). The font is a prop, not a class: the editor measures its text in it.

## Accessibility

The sink carries `accessibilityLabel`; the rows are real texts, read in order.

## From GPUI Kit

| GPUI Kit | NSH |
|---|---|
| `InputState` with `code_editor(language)` | `<Editor value language>`: controlled text, the model inside the component |
| Tree-sitter highlighting, themes | Syntect with Allsum's palette (`crate::syntax`), incremental per line; the app's `decorations` for semantic highlighting |
| `DisplayMap` (wrap, folds) | `display.rs` on measured char widths; folds by indentation |
| Search panel | `search` prop, `onSearchChange`, `findNext` / `replace` / `replaceAll`: the find bar is the app's |
| Diagnostics with hover popovers | `diagnostics` marked and carried through edits; `onHover` carries those under the mouse, the app shows them |
| `lsp` providers (hover, completion, definition, code actions) | `onHover` / `onHoverEnd`, `onTextPress`, `caretBounds` / `rangeBounds`: the app asks its language server and draws the popups |
| Key bindings as GPUI actions | `keymap.rs` data on `nsh/keymap`, bound while focused; `run(command)` |

## Limits

- Widths are sums of per-char widths: ligatures and kerning are not applied, and rows lay out left to right.
- Highlighting parses from the top down: after a jump to the end of a long file, the lines in view show a guess
  (parsed from 40 lines before them) until the parse reaches them, which can differ inside a long comment or string.
- At most 10 000 search matches are found and 1000 undo steps kept.
- Folds come from indentation only.
- Touch screens place the caret with a tap; selecting is by the commands and keys.
