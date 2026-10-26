---
title: RichEditor
description: "The rich text editor of nsh/base: documents defined by the app's schema, commands, plugins, history, keymaps, input rules and Markdown, after ProseMirror's architecture, with its state in Rust."
order: 26.25
group: base
---

# RichEditor

`RichEditor` is a rich text editor built after ProseMirror's architecture. The app defines what a document is made
of with a schema (paragraphs, headings, lists, mentions, whatever node and mark types it needs), and the editor keeps
every document valid against it. Changes are steps grouped into transactions, so they undo, map positions and travel
as JSON; behavior comes from plugins (history, keymaps, input rules, Markdown shortcuts, the app's own); commands are
values a toolbar runs and asks about. The editor's state, its commands and its plugins run in Rust: typing, key
bindings and input rules never wait for JS, and the app reads the state as JSON when it asks for it.

An editor has two parts: the session (`createRichEditor`), which owns the state and which the app's code drives, and
the view (`<RichEditor editor={…} />`), which renders the session natively and turns the platform's input into
transactions. Several views can show one session.

```tsx
import { mount } from "nsh";
import { RichEditor, createRichEditor, history, inputRules, keymap, markdown, markdownRules, starterKeymap, baseKeymap, toggleMark } from "nsh/base";

const editor = createRichEditor({
  content: "# Notes\n\nStart typing…",
  format: "markdown",
  plugins: [
    history(),
    keymap({ "Mod-b": toggleMark("bold"), "Mod-i": toggleMark("italic") }),
    starterKeymap(),
    baseKeymap(),
    inputRules(markdownRules()),
    markdown(),
  ],
});

function App() {
  return <RichEditor editor={editor} class="flex-1" />;
}

mount(App);
```

Without `schema` an editor uses the starter schema (`starterSchema()`); without `plugins` it gets the starter kit
(`starterKit()`: the starter's input rules and key bindings, the base key bindings and the history). An empty
`plugins` array makes an editor with no plugins at all.

## Styling

The editor draws the document with the app's look: `classes` and `styles` style its parts (`root`, `content`, `caret`,
`selection`, `placeholder`, `composition`, `selectedNode`, `listMarker`), its nodes by type (`nodes: { heading:
"text-2xl font-bold", "heading[level=2]": "text-xl" }`) and its marks (`marks: { bold: "font-bold" }`). The items of
lists whose node spec sets `extra.listMarker` to `"bullet"` or `"ordered"` (the starter schema's lists) show a marker
in the `listMarker` part: `•`, `◦`, `▪` by depth, or numbers from the list's `start`, placed before the item, in the list's indent (`pl-6` on the list, as a browser indents `ul`).

```tsx
<RichEditor
  editor={editor}
  classes={{
    nodes: { heading: "text-2xl font-bold", bulletList: "pl-6", codeBlock: "font-mono bg-muted p-3 rounded-md" },
    marks: { bold: "font-bold", italic: "italic", code: "font-mono bg-muted" },
    listMarker: "text-muted-foreground",
  }}
/>
```

## Schemas

`defineSchema(spec)` reads a schema in ProseMirror's shape and throws with what is wrong in it. Node types name their
`content` (a content expression: `"inline*"`, `"paragraph+"`, `"(heading | paragraph) block*"`), their `group`,
whether they are `inline` or an `atom`, the `marks` they allow and their `attrs` with defaults and validation
(`"string"`, `"number"`, `"boolean"`). The first block type is the default one: what Enter creates and what fills
required content. Keys a spec does not know are kept as the type's extra data, for frameworks built on the editor.

```tsx
import { createRichEditor, defineSchema } from "nsh/base";

const schema = defineSchema({
  nodes: {
    doc: { content: "block+" },
    paragraph: { content: "inline*", group: "block" },
    mention: { group: "inline", inline: true, atom: true, attrs: { id: {}, label: { default: "" } } },
    text: { group: "inline" },
  },
  marks: { bold: {}, link: { attrs: { href: {} }, inclusive: false } },
});

const editor = createRichEditor({ schema, doc: schema.parse("Hello") });
```

A `RichSchema` reads and writes documents: `parse(content, format?)` (plain text by default, `"markdown"`, or JSON),
`parseMarkdown(markdown)`, `serialize(doc, format?)` (Markdown by default, `"text"`, `"json"`) and `check(doc)`, which
returns the document with its attribute defaults filled in. `nodes`, `marks` and `topNode` name its types. Markdown
maps to the starter schema's names (`paragraph`, `heading`, `blockquote`, `codeBlock`, `bulletList`,
`orderedList`, `listItem`, `bold`, `italic`, `code`, `link`, `strike`).

## Commands

A command is a value: `toggleMark("bold")` makes an object the app keeps, binds to keys and passes to the editor.
`editor.run(command)` runs it and returns whether it applied; `editor.can(command)` asks without changing anything,
which is how a toolbar enables its buttons. `editor.isActive(name, attrs?)` tells whether a mark or a node is active
at the selection (`isActive("heading", { level: 2 })`).

| Command | What it does |
| --- | --- |
| `toggleMark(name, attrs?)` | Adds the mark to the selection, or removes it where present; at a cursor, for the text typed next |
| `setMark(name, attrs?)` / `unsetMark(name)` | Adds or removes a mark |
| `setBlockType(name, attrs?)` | Makes the selected textblocks of a type (`setBlockType("heading", { level: 2 })`) |
| `wrapIn(name, attrs?)` / `lift()` | Wraps the selected blocks in a node (a quote) / lifts them out of their parent |
| `wrapInList(name, attrs?)` | Wraps the selected blocks in a list |
| `splitListItem(name)` / `sinkListItem(name)` / `liftListItem(name)` | Splits, indents or outdents list items |
| `undo()` / `redo()` | Undoes or redoes a change (with `history()`) |
| `selectAll()`, `setTextSelection(anchor, head?)`, `setNodeSelection(pos)` | Selects |
| `insertText(text)`, `insertContent(content, format?)` | Inserts text or content over the selection |
| `setNodeAttribute(pos, attr, value)` | Sets an attribute of the node at a position, as one undoable change |
| `command(name)` | A command without parameters by name: `"splitBlock"`, `"joinBackward"`, `"exitCode"`, `"newlineInCode"`, `"selectParentNode"`, … |
| `chain(commands)` | Runs commands one after another as one transaction (one undo step); applies only when every one does |
| `first(commands)` | Runs the first command that applies |

```tsx
editor.run(chain([setBlockType("heading", { level: 1 }), toggleMark("bold")]));
const canIndent = editor.can(sinkListItem("listItem"));
```

## The session

| Member | What it is |
| --- | --- |
| `state`, `doc`, `selection` | The state, the document and the selection as JSON, made when read |
| `revision` | How many transactions the state has applied |
| `isEmpty`, `undoDepth`, `redoDepth`, `composing` | What a UI shows about the editor |
| `setContent(content, format?)` | Replaces the document, as one undoable change |
| `insertContent(content, format?)` | Inserts content over the selection |
| `getJSON()`, `getMarkdown()`, `getText()`, `textBetween(from?, to?, separator?)` | The document in a format |
| `dispatch(tr)` | Applies a transaction given as JSON (its steps against the current document) |
| `subscribe(listener)` / `unsubscribe(id)` | Hears every change, from any source, with the new revision |
| `focus()`, `blur()` | Moves the keyboard focus to or from the views showing the editor |

Positions count as ProseMirror's do: a text counts its UTF-16 code units, a leaf node one, a node with content its
two boundaries. Anything that must outlive edits keeps an id in a node's attributes or a mark.

### Taking over transactions

`editor.dispatchTransaction = (tr) => { … }` makes the app commit every transaction itself: the handler gets each
one as JSON and commits it with `editor.applyTransaction(tr)`, or drops it. Collaboration, logging and transactions
that must pass the app's checks use it; `null` hands committing back to the editor. The handler hears typing in JS's
next turn, and the document changes when it commits.

## Plugins

| Factory | What it adds |
| --- | --- |
| `history({ depth?, newGroupDelay? })` | Undo history; changes within `newGroupDelay` milliseconds (500) undo together |
| `keymap(bindings)` | Commands by key name: `"Mod-b"` (Cmd on Apple platforms, Ctrl elsewhere), `"Shift-Enter"`, `"Alt-ArrowUp"` |
| `baseKeymap()` | Enter, Backspace, Delete, select all and the platform's text editing keys |
| `starterKeymap()` | The starter schema's bindings: marks, headings, lists, code blocks, hard breaks |
| `inputRules(rules)` | Text typed before the cursor that matches a rule turns into what the rule makes; Backspace right after undoes it |
| `markdown(options?)` | Markdown as you type, Markdown paste and Markdown copy (`blockRules`, `inlineRules`, `paste`, `copyAsMarkdown`) |
| `starterKit()` | The starter's input rules and bindings, the base bindings and the history |

A plugin's place in the list is its priority: keymaps and input rules earlier in the list handle a key or a text
first. Input rules come from `markdownRules()` (headings, quotes, lists, code fences, rules, the common marks),
`typographyRules()` (smart quotes, ellipsis, em dash), `textRule(match, replacement)`, `markRule(match, mark)`,
`wrappingRule(match, node)` and `textblockRule(match, node)`. A rule's `match` is a regular expression ending in `$`,
matched against the text before the cursor.

### The app's plugins

`new RichEditorPlugin(key)` makes a plugin whose behavior is the app's, set with chained calls:

```tsx
import { RichEditorPlugin, createRichEditor, starterKit } from "nsh/base";

const suggestions = new RichEditorPlugin("suggestions")
  .captureKeys(["ArrowUp", "ArrowDown", "Enter", "Escape"], (key) => moveSuggestion(key))
  .filterTransaction((tr) => tr.meta?.locked !== true)
  .appendTransaction((trs, state) => null);

const editor = createRichEditor({ plugins: [starterKit(), suggestions] });

// While the suggestion list is open, its keys are the list's.
suggestions.capturing = true;
editor.setDecorations("suggestions", [{ from: 5, to: 9, kind: "inline", attrs: { class: "bg-accent" } }]);
```

- `captureKeys(keys, handler)`: while `capturing` is true the editor consumes these keys and tells `handler` which
  one was pressed, right after; the keyboard stays as quick as with no plugin.
- `editor.setDecorations(key, decorations)`: inline attributes, node attributes and widgets the view draws over the
  document without changing it. They follow every change until set again.
- `filterTransaction(hook)` and `appendTransaction(hook)` see the transactions the app's own calls make (`run`,
  `dispatch`, `setContent`, …) in that call: `filterTransaction` may drop one, `appendTransaction` may return one
  more to apply with it. `subscribe` hears every change, typing included.
