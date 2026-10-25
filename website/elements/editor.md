---
title: Editor
description: "The Editor component: a code editor with syntax highlighting, line numbers, several carets, folding, soft wrap, find and replace, diagnostics and decorations, and the keys of code editors."
order: 26.2
group: base
demo: el-editor
---

# Editor

`Editor` is a code editor: syntax highlighting for more than fifty languages, line numbers, several carets, folding,
soft wrap, find and replace, diagnostics and decorations, with the keys of code editors (moving and duplicating lines,
comments, indentation, brackets and quotes that close as they are typed). It holds files of any length: only the rows
in view are made, and highlighting follows each edit line by line, so a long file scrolls and types as a short one
does. Typing goes through the platform's input, so input methods (pinyin, kana, dead keys) and soft keyboards work as
they do in any text field.

The editor owns behavior and comes with a look of its own built from the theme's tokens: the theme's `font-mono` at
13pt in 20pt rows, line numbers in `muted-foreground`, a faint `muted` band behind the lines with a caret, selections
in a tint of `primary`, indent guides in `border`, and the built-in syntax colors that [`TextView`](./text-view.md)'s
code blocks use, in light and dark. Each part takes the app's styles over these defaults (`classes` and `styles`).
Rust makes one with `nsh_base::editor()`, TSX with `<Editor>` from `nsh/base`.

```rust tabs
use nsh_base::editor;
use nsh_core::ui::prelude::*;

const MAIN: &str = "fn main() {\n    println!(\"Hello, world!\");\n}\n";

fn code() -> impl IntoElement {
    let source = signal(SharedString::from(MAIN));
    editor()
        .value(move || Some(source.get()))
        .language("rust")
        .on_change(move |value: &String| source.set(value.clone().into()))
        .flex_1()
}
```

```tsx tabs
import { mount, signal } from "nsh";
import { Editor } from "nsh/base";

const MAIN = `fn main() {
    println!("Hello, world!");
}
`;

function App() {
  const source = signal(MAIN);
  return <Editor value={source()} onChange={(value) => source.set(value)} language="rust" class="flex-1" />;
}

mount(App);
```

The editor fills the box the app gives it (`flex-1`, a height) and scrolls its text inside: vertically always,
sideways when lines are longer than the box, or not at all with `softWrap`, where long lines wrap at the editor's
width.

## The text

- **Controlled**: pass `value` and set it again in `onChange`, which reports the whole new text. A `value` the app
  changes (a file loaded, a formatter's output) replaces the text as one edit undo takes back, and the carets and
  folds away from what changed stay where they are.
- **Uncontrolled**: pass `defaultValue` and read the text from `onChange` or the ref's `value()`.

Line breaks are `\n`: `\r\n` and `\r` in a text that comes in become `\n`.

## Languages

`language` names the language by its name, an alias, an extension or a file name, in any case: `"rust"`, `"rs"`,
`"typescript"`, `"tsx"`, `"main.go"`, `"Dockerfile"`, `"Makefile"`. It decides the highlighting, the comment that
`mod+/` toggles, the brackets that match and indent, the pairs typing closes, the characters words are made of, and
the default indentation: four spaces, two for web languages (JavaScript, TypeScript, JSON, HTML, CSS, YAML, ...), tabs
for Go and Makefiles. `tabSize` and `hardTabs` set the indentation for every language. Any other name is plain text.

The languages: Rust, JavaScript, JSX, TypeScript, TSX, JSON, Python, Go, C, C++, C#, Java, Kotlin, Swift, Objective-C,
Objective-C++, Ruby, PHP, Lua, Shell, Fish, PowerShell, SQL, HTML, XML, Vue, Svelte, CSS, SCSS, Sass, Less, Markdown,
YAML, TOML, INI, Dockerfile, Makefile, CMake, Dart, Scala, Elixir, Erlang, Haskell, OCaml, F#, Clojure, Zig, Perl, R,
Julia, Nix, Terraform, Protocol Buffers, GraphQL and Diff.

```tsx
<Editor value={source()} onChange={(v) => source.set(v)} language={file().name} tabSize={2} softWrap class="flex-1" />
```

## Editing

The editor works as code editors do:

- **Several carets.** Alt-click (Option-click) adds a caret, `mod+d` selects the next occurrence of the selection,
  `mod+shift+l` all of them, and the add-caret keys add one on the row above or below. Typing, deleting, moving and
  every command act at each one; Escape goes back to one.
- **Indentation.** A new line keeps the line's indentation and adds a level after an opening bracket (Python's `:`,
  Ruby's `do`, ...); Enter between a pair of brackets puts the closing one on its own line. Tab and Shift+Tab indent
  and outdent the selected lines; Backspace in leading spaces goes back to the previous indent stop. `autoIndent` turns
  this off.
- **Brackets and quotes** close as they are typed, wrap a selection, and step over the closing one when it is typed
  again (`autoClose`). The bracket at the caret and its partner are outlined.
- **Undo** takes back whole runs of typing: a word, a deletion, a command. A move or a press starts a new step.
- **Folding.** A line followed by lines indented deeper folds them away with the marker in the gutter: `⌄` to fold,
  `›` and the `⋯` after the line to unfold. A caret that moves into a fold opens it.
- **Presses.** With a pointer, a click places the caret, a double click selects a word, a triple click a line,
  Shift-click extends the selection and a drag selects by what the click selected. On touch screens a tap places the
  caret.
- **Clipboard.** Copy and cut with no selection take the whole lines of the carets, and those paste back as whole
  lines above the caret's line.

`readOnly` keeps the text as it is while it is still selected, searched and copied.

## Keys

The editor's keys work while it has the focus; other inputs keep theirs. `mod` is Command on macOS and iOS and Control
elsewhere (`nsh/keymap`'s keystrokes).

| Keys (macOS) | Keys (Windows, Linux) | Command |
|---|---|---|
| Arrows, with Shift to select | Arrows, with Shift to select | Move by character and row |
| `alt+left` / `alt+right` | `ctrl+left` / `ctrl+right` | Move by word |
| `cmd+left` / `cmd+right`, `home` / `end`, `ctrl+a` / `ctrl+e` | `home` / `end` | The line's first non-blank character (then its start) / the line's end |
| `cmd+up` / `cmd+down` | `ctrl+home` / `ctrl+end` | The start / end of the text |
| `pageup` / `pagedown` | `pageup` / `pagedown` | Up / down a page |
| `alt+backspace` / `alt+delete` | `ctrl+backspace` / `ctrl+delete` | Delete a word left / right |
| `cmd+backspace` / `cmd+delete`, `ctrl+k` | `ctrl+shift+backspace` / `ctrl+shift+delete` | Delete to the line's start / end |
| `cmd+z` / `cmd+shift+z` | `ctrl+z` / `ctrl+shift+z`, `ctrl+y` | Undo / redo |
| `cmd+a` | `ctrl+a` | Select all |
| `cmd+l` | `ctrl+l` | Select the line (again: the next one too) |
| `cmd+d` | `ctrl+d` | Select the next occurrence |
| `cmd+shift+l` | `ctrl+shift+l` | Select every occurrence |
| `cmd+alt+up` / `cmd+alt+down` | `ctrl+alt+up` / `ctrl+alt+down` | Add a caret above / below |
| `escape` | `escape` | Back to one selection |
| `alt+up` / `alt+down` | `alt+up` / `alt+down` | Move the lines up / down |
| `alt+shift+up` / `alt+shift+down` | `alt+shift+up` / `alt+shift+down` | Duplicate the lines up / down |
| `cmd+shift+k` | `ctrl+shift+k` | Delete the lines |
| `cmd+enter` / `cmd+shift+enter` | `ctrl+enter` / `ctrl+shift+enter` | A new line below / above |
| `tab` / `shift+tab`, `cmd+]` / `cmd+[` | `tab` / `shift+tab`, `ctrl+]` / `ctrl+[` | Indent / outdent |
| `cmd+/` | `ctrl+/` | Toggle the line comment |
| `cmd+shift+\` | `ctrl+shift+\` | Go to the matching bracket |
| `ctrl+j` | `ctrl+j` | Join the lines |
| `ctrl+t` | | Swap the characters around the caret |
| `cmd+c` / `cmd+x` / `cmd+v` | `ctrl+c` / `ctrl+x` / `ctrl+v` | Copy / cut / paste |

macOS and iOS also have Cocoa's text keys: `ctrl+b` / `ctrl+f` / `ctrl+p` / `ctrl+n` move, `ctrl+h` / `ctrl+d`
delete.

Find, folding and the app's own commands take the keys the app binds with [`nsh/keymap`](../modules/nsh-keymap.md),
running them through the ref's `run`:

```tsx
import { keymap } from "nsh/keymap";

let editor!: EditorRef;

keymap.bind({
  "mod+g": () => editor.run("findNext"),
  "mod+shift+g": () => editor.run("findPrevious"),
  "mod+alt+[": () => editor.run("fold"),
  "mod+alt+]": () => editor.run("unfold"),
});
```

## Find and replace

`search` is what to find: `{ query, caseSensitive?, wholeWord?, regex? }`. Its matches are highlighted, the one the
selection is on more strongly, and `onSearchChange` reports `{ count, current, error }`: how many matches, which one
the selection is on (from 0), and why a regular expression is not valid. `findNext` and `findPrevious` select the next
and previous match, wrapping around; `replace(text)` replaces the current match and selects the next one, and
`replaceAll(text)` replaces every match as one edit and returns how many. A regular expression's replacement refers to
its groups as `$1` and `${name}`.

The find bar is the app's own:

```tsx
import { Input, signal, Text, View } from "nsh";
import { Button, Editor, type EditorRef, type EditorSearchResult } from "nsh/base";

function Find(props: { source: string; onChange: (value: string) => void }) {
  let editor!: EditorRef;
  const query = signal("");
  const replacement = signal("");
  const result = signal<EditorSearchResult>({ count: 0 });
  const status = () =>
    result().error ?? (result().current != null ? `${result().current! + 1} of ${result().count}` : `${result().count} found`);
  return (
    <View class="flex-1">
      <View class="flex-row items-center gap-2 border-b border-border p-2">
        <Input value={query()} onChange={(q) => query.set(q)} onSubmit={() => editor.run("findNext")} placeholder="Find" class="flex-1" />
        <Text class="text-sm text-muted-foreground">{status()}</Text>
        <Button onPress={() => editor.run("findPrevious")} class="px-2"><Text>↑</Text></Button>
        <Button onPress={() => editor.run("findNext")} class="px-2"><Text>↓</Text></Button>
        <Input value={replacement()} onChange={(r) => replacement.set(r)} placeholder="Replace" class="flex-1" />
        <Button onPress={() => editor.replace(replacement())} class="px-2"><Text>Replace</Text></Button>
        <Button onPress={async () => console.log("replaced", await editor.replaceAll(replacement()))} class="px-2"><Text>All</Text></Button>
      </View>
      <Editor
        ref={editor}
        value={props.source}
        onChange={props.onChange}
        language="ts"
        search={query() ? { query: query() } : undefined}
        onSearchChange={(r) => result.set(r)}
        class="flex-1"
      />
    </View>
  );
}
```

## Diagnostics

`diagnostics` marks the problems a language server or a linter reports: a line under each range in its severity's
color (`error`, `warning`, `info`, `hint`). Positions are lines and UTF-16 columns from 0, as a language server's, so
its diagnostics pass through as they come. The marks stay on their text while the user types, until the app passes
new ones; showing the messages (a list, a hover card at the line) is the app's.

```tsx
const problems = signal<EditorDiagnostic[]>([]);

async function lint(text: string) {
  problems.set(await check(text)); // [{ range, severity, message, source?, code? }]
}

<Editor
  value={source()}
  onChange={(v) => {
    source.set(v);
    lint(v);
  }}
  language="ts"
  diagnostics={problems()}
  classes={{ diagnosticWarning: "bg-orange-500" }}
  class="flex-1"
/>
```

## Decorations

`decorations` style ranges the app chooses: semantic highlighting from a language server, the statement being run, the
lines a review changed. A decoration's `class` and `style` style its text; `lineClass` and `lineStyle` style the rows
of its lines, behind the text. Like diagnostics, they stay on their text through edits until the app sets new ones.

```tsx
const range = (line: number, start: number, end: number) => ({ start: { line, column: start }, end: { line, column: end } });

<Editor
  value={source()}
  language="sql"
  readOnly
  decorations={[
    { range: range(4, 0, 0), lineClass: "bg-green-500/10" },
    { range: range(7, 6, 18), class: "underline text-blue-600" },
  ]}
  class="flex-1"
/>
```

## Breakpoints and context menus

A press on a line number reports `onLineNumberPress(line)`, the place for a breakpoint the app draws as a decoration.
The secondary button (a right click, Control-click on a Mac) reports `onContextMenu({ position, x, y })`: the
position in the text under the pointer and the point in the root view, where the app opens its own menu.

```tsx
const breakpoints = signal<number[]>([]);

<Editor
  value={source()}
  language="py"
  onLineNumberPress={(line) =>
    breakpoints.set(breakpoints().includes(line) ? breakpoints().filter((l) => l !== line) : [...breakpoints(), line])
  }
  decorations={breakpoints().map((line) => ({ range: range(line, 0, 0), lineClass: "bg-red-500/15" }))}
  class="flex-1"
/>
```

## Methods

`ref={editor}` gives the editor's methods (`EditorRef`), for the app's buttons, menus and keys. Methods that return a
value (`value()`, `selections()`, `replaceAll()`, `canUndo()`, `canRedo()`) return a `Promise` of it in JS, so `await`
them: `const text = await editor.value()`.

| Method | Description |
|---|---|
| `focus()`, `blur()` | Takes the focus, or gives it up. |
| `value()` | `Promise<string>`: the text. |
| `setValue(value)` | Replaces the text, as an edit undo takes back. |
| `insert(text)` | Types `text` at each selection, as it is (brackets do not close). |
| `selections()` | `Promise<EditorSelection[]>`: every selection, `{ anchor, head }`, the primary (newest) last. |
| `setSelections(selections)` | Sets the selections (the last one primary) and scrolls the primary one into view. |
| `run(command)` | Runs a command, as its keys would ([Commands](#commands)). |
| `replace(text)` | Replaces the current match and selects the next. |
| `replaceAll(text)` | `Promise<number>`: replaces every match as one edit; how many. |
| `fold(line)`, `unfold(line)` | Folds the lines under line `line` (from 0), or unfolds it. |
| `scrollToLine(line)` | Scrolls line `line` into view, unfolding what hides it. |
| `canUndo()`, `canRedo()` | `Promise<boolean>`: whether there is an edit to take back, or to do again. |

```tsx
let editor!: EditorRef;

<Button onPress={() => editor.run("toggleComment")}><Text>Comment</Text></Button>
<Button onPress={async () => save(await editor.value())}><Text>Save</Text></Button>
<Button onPress={async () => (await editor.canUndo()) && editor.run("undo")}><Text>Undo</Text></Button>
<Button onPress={() => editor.setSelections([{ anchor: { line: 0, column: 0 }, head: { line: 0, column: 0 } }])}>
  <Text>Go to the top</Text>
</Button>
<Editor ref={editor} value={source()} onChange={(v) => source.set(v)} language="rs" class="flex-1" />
```

### Commands

`run` takes the editor's commands by name (`EditorCommand`):

| Commands | What they do |
|---|---|
| `moveLeft`, `moveRight`, `moveUp`, `moveDown`, `moveWordLeft`, `moveWordRight`, `moveLineStart`, `moveLineEnd`, `movePageUp`, `movePageDown`, `moveDocumentStart`, `moveDocumentEnd` | Move the carets. |
| `selectLeft`, `selectRight`, `selectUp`, `selectDown`, `selectWordLeft`, `selectWordRight`, `selectLineStart`, `selectLineEnd`, `selectPageUp`, `selectPageDown`, `selectDocumentStart`, `selectDocumentEnd` | Extend the selections. |
| `moveToMatchingBracket` | To the bracket matching the one at the caret. |
| `selectAll`, `selectLine`, `selectWord`, `selectNextOccurrence`, `selectAllOccurrences` | Select. |
| `addCaretAbove`, `addCaretBelow`, `singleSelection` | Add a caret; back to one selection. |
| `deleteBackward`, `deleteForward`, `deleteWordLeft`, `deleteWordRight`, `deleteToLineStart`, `deleteToLineEnd`, `deleteLine` | Delete. |
| `newline`, `newlineBelow`, `newlineAbove`, `indent`, `outdent`, `toggleComment` | Lines and indentation. |
| `moveLinesUp`, `moveLinesDown`, `duplicateLinesUp`, `duplicateLinesDown`, `joinLines`, `transposeCharacters` | Rearrange. |
| `undo`, `redo` | Undo and redo. |
| `fold`, `unfold`, `foldAll`, `unfoldAll` | Fold at the caret's line, or everything. |
| `findNext`, `findPrevious` | Select the next or previous match of `search`. |
| `copy`, `cut`, `paste` | The clipboard. |

## Styling the parts

`class` and `style` style the editor's box. `classes` takes Tailwind classes by part and `styles` inline styles by
part; a part's classes come after its own default styles, so they win, and its `styles` win over its classes. The font
is set with `fontFamily`, `fontSize` and `lineHeight`, which the editor measures its text in.

```tsx
<Editor
  value={source()}
  onChange={(v) => source.set(v)}
  language="tsx"
  fontFamily="JetBrains Mono"
  fontSize={14}
  lineHeight={22}
  class="flex-1 rounded-lg border border-border"
  classes={{
    gutter: "bg-muted",
    lineNumber: "text-xs",
    activeLine: "bg-yellow-500/10",
    selection: "bg-blue-500/30",
  }}
  styles={{ caret: { backgroundColor: "#f59e0b", width: 3 } }}
/>
```

| Part | What it is | Default |
|---|---|---|
| `gutter` | The column of line numbers and fold markers | 12pt before the numbers, 4pt after the markers |
| `lineNumber` | Every line number | Right-aligned, `muted-foreground` |
| `activeLineNumber` | The numbers of the lines with a caret, after `lineNumber` | `foreground` |
| `foldMarker` | The marker of a line where a fold starts | `⌄`, `›` folded, `muted-foreground` |
| `content` | The text area beside the gutter | 4pt before the text |
| `line` | Every row of text | |
| `activeLine` | Behind the rows of the lines with a caret | `muted`, faint |
| `selection` | Behind selected text | A tint of `primary` |
| `caret` | The carets | 2pt wide, `foreground`, blinking |
| `composition` | The text an input method is composing | Underlined |
| `searchMatch` | The matches of `search` | Translucent amber |
| `currentSearchMatch` | The match the selection is on, after `searchMatch` | Stronger amber |
| `matchingBracket` | The bracket at the caret and its partner | A 1pt border |
| `indentGuide` | A guide per indentation level | A 1pt line of `border` |
| `foldPlaceholder` | What a folded line shows after its text | ` ⋯ `, `muted-foreground` |
| `placeholder` | Shown while the text is empty | `muted-foreground` |
| `diagnosticError` | The line under an error | `destructive` |
| `diagnosticWarning` | The line under a warning | Amber |
| `diagnosticInfo` | The line under an information | Blue |
| `diagnosticHint` | The line under a hint | `muted-foreground` |

## Props

| Prop | Rust | Type | Default | Description |
|---|---|---|---|---|
| `value` | `.value(..)` | `string` | | The text, controlled by the app (set it again from `onChange`). |
| `defaultValue` | `.default_value(..)` | `string` | | The first text, when the editor keeps its own. |
| `language` | `.language(..)` | `string` | plain text | A language's name, alias, extension or file name ([Languages](#languages)). |
| `placeholder` | `.placeholder(..)` | `string` | | Shown while the text is empty. |
| `readOnly` | `.read_only(..)` | `boolean` | `false` | The text is selected and copied, not changed. |
| `autoFocus` | `.auto_focus(..)` | `boolean` | `false` | Takes the focus when it appears. |
| `lineNumbers` | `.line_numbers(..)` | `boolean` | `true` | The gutter's line numbers. |
| `softWrap` | `.soft_wrap(..)` | `boolean` | `false` | Long lines wrap at the editor's width. |
| `tabSize` | `.tab_size(..)` | `number` | the language's | The columns of a tab and of an indentation level. |
| `hardTabs` | `.hard_tabs(..)` | `boolean` | the language's | Indents with tabs. |
| `folding` | `.folding(..)` | `boolean` | `true` | Indented lines fold with the gutter's markers. |
| `autoClose` | `.auto_close(..)` | `boolean` | `true` | Brackets and quotes close as they are typed. |
| `autoIndent` | `.auto_indent(..)` | `boolean` | `true` | A new line keeps the indentation and indents after an opening bracket. |
| `activeLine` | `.active_line(..)` | `boolean` | `true` | The rows of the lines with a caret are highlighted. |
| `indentGuides` | `.indent_guides(..)` | `boolean` | `true` | The guides of indentation. |
| `scrollBeyondLastLine` | `.scroll_beyond_last_line(..)` | `boolean` | `true` | The text scrolls on past its last line, up to a screen. |
| `fontFamily` | `.font_family(..)` | `string` | the theme's `font-mono` | The font the text is drawn and measured in. |
| `fontSize` | `.font_size(..)` | `number` | `13` | The font size, in points. |
| `lineHeight` | `.line_height(..)` | `number` | `20` | The height of a row, in points. |
| `search` | `.search(..)` | `EditorSearch` | | What to find ([Find and replace](#find-and-replace)). |
| `diagnostics` | `.diagnostics(..)` | `EditorDiagnostic[]` | `[]` | Problems to mark ([Diagnostics](#diagnostics)). |
| `decorations` | `.decorations(..)` | `EditorDecoration[]` | `[]` | Ranges the app styles ([Decorations](#decorations)). |
| `classes` | `.classes(..)` | `EditorClasses` | `{}` | Tailwind classes by part. |
| `styles` | `.styles(..)` | `EditorStyles` | `{}` | Inline styles by part, over the classes. |
| `accessibilityLabel` | `.accessibility_label(..)` | `string` | | What screen readers call the editor. |
| `class`, `style` | style methods | `string`, `StyleProp<ViewStyle>` | | The editor's box. |

## Events

| Event | Rust | Description |
|---|---|---|
| `onChange` | `.on_change(\|value: &String\| ..)` | The text changed: the whole new text. |
| `onSelectionChange` | `.on_selection_change(\|selections: &Vec<EditorSelection>\| ..)` | The selections changed: every one, the primary last. |
| `onFocus`, `onBlur` | `.on_focus(\|\| ..)`, `.on_blur(\|\| ..)` | The editor took or lost the focus. |
| `onSearchChange` | `.on_search_change(\|result: &EditorSearchResult\| ..)` | The search's matches changed: `{ count, current, error }`. |
| `onLineNumberPress` | `.on_line_number_press(\|line: &u32\| ..)` | A line number was pressed: the line, from 0. |
| `onContextMenu` | `.on_context_menu(\|event: &EditorContextMenuEvent\| ..)` | The secondary button was pressed over the text: `{ position, x, y }`. |

## Accessibility

Screen readers call the editor by its `accessibilityLabel`; its focus is a text input's, so the platform's text
services (input methods, dictation, soft keyboards) work in it.

## Related

- [TextView](./text-view.md): documents of Markdown and HTML, whose code blocks share the editor's syntax colors.
- [DiffView](./diff-view.md): the changes between two texts, in the editor's syntax colors.
- [Input](./input.md): single and multiline text fields with the platform's own editing.
- [`nsh/keymap`](../modules/nsh-keymap.md): the app's keys for the editor's commands.
- [`nsh/base`](../modules/nsh-base.md): the other base components.
