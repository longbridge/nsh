# RichEditor

`RichEditor` is `nsh/base`'s rich text editor. Its kernel (`crates/nsh-base/src/rich_editor/`) follows ProseMirror's
architecture: a document whose structure a schema defines, changes as steps grouped into transactions, immutable
editor states, commands, plugins, and adapters to the formats apps store. The kernel is headless: it depends on no view
and is tested on its own; the native view renders its states and turns the platform's input into transactions.

## Modules

| Module | What it holds |
|---|---|
| `model/` | `Schema` (node and mark types from a `SchemaSpec`, content expressions compiled to `ContentMatch` automata), `Node`, `Fragment`, `Mark`, `Slice`, `ResolvedPos`, `NodeRange`; `Node::replace` after ProseMirror's `replace.ts`; ProseMirror's JSON shape |
| `transform/` | `Step` (an object-safe trait: `ReplaceStep`, `ReplaceAroundStep`, `AddMarkStep` / `RemoveMarkStep`, `AddNodeMarkStep` / `RemoveNodeMarkStep`, `AttrStep`, `DocAttrStep`), `StepMap` / `Mapping`, `Transform` and the structure helpers (lift, wrap, split, join, `replace_step`'s fitter) |
| `state/` | `EditorState`, `Transaction`, `Selection` (text, node, all, custom), plugins and decorations |
| `commands/` | prosemirror-commands and prosemirror-schema-list: `Command`s over a state and an optional dispatch, `chain_commands`, `base_keymap` |
| `extensions/` | optional behavior as plugins: `history`, `keymap`, `input_rules`, `markdown` (Markdown shortcuts as you type, Markdown paste and copy, mapped by `MarkdownNames`); `starter`, the optional starter schema (paragraphs, headings, quotes, code, lists, images, the common marks) with its keymap and input rules |
| `formats/` | `DocFormat`: `JsonFormat`, `PlainTextFormat`, `MarkdownFormat` (names mapped by `MarkdownNames`), the clipboard's `slice_to_text` / `slice_from_text` |

## Documents defined by the schema

The kernel knows no node or mark by name. A document's types, their content (`"paragraph+"`, `"inline*"`,
`"(heading | paragraph) block*"`), groups, attributes with defaults and validation, allowed marks and mark exclusion
are the schema's, written in Rust (`SchemaSpec`) or JSON (`SchemaSpec::from_json`). The first block type is the
default one (what `Enter` creates, what fills required content). `starter` is a schema like any app's, not a part of
the kernel; formats and commands take type names or types as parameters.

## Steps and transactions

Every change is a step: it applies to a document or fails, has a `StepMap` that maps positions across it, inverts
against the document it applied to, maps over other changes, may merge with the next one and serializes to JSON. A
`Transform` collects steps, the documents between them and the mapping of all of them; a step that fails leaves the
transform unchanged.

A `Transaction` is a transform with the state's selection, stored marks, metadata and an origin
(`TransactionOrigin`: user, input, paste, drop, remote, history, plugin, API). One user action is one transaction: a
keystroke, a paste, a command, an IME commit. `EditorState::apply_transaction` applies it atomically: plugins'
`filter_transaction` may refuse it, the new document must pass `Node::check` against the schema, and plugins'
`append_transaction` results are applied too, or nothing is (`StateError`).

## Positions

Positions are ProseMirror's linear positions: a text counts its length, a leaf node one, a node with content its two
boundaries. Text lengths are UTF-16 code units, as the platforms' text systems and JS count them (texts are stored as
UTF-8; `model::utf16_to_byte` converts). Commands and the view move by grapheme clusters, never inside one, and never
split a surrogate pair. A text selection's head has an `Affinity` (upstream, downstream) that says on which side of a
line wrap or a bidi boundary the caret shows. Positions shift with every change: anything that must outlive edits
(comments, anchors, collaboration) keeps an id in a node's attributes or a mark, never a position, or maps its
positions through each transaction's `mapping`.

## Editor state

`EditorState` is immutable and cheap to clone. It holds the schema, the document, the selection, the stored marks (the
marks the next typed text gets, set by toggling a mark at a cursor and cleared when the selection moves), the plugins
and their states, and a revision that counts applied transactions. `apply` returns a new state; the old one stays
valid, which is what history, diffing and the view's updates rely on. States read from and write to JSON.

## Selections

`Selection` is `TextSelection` (anchor and head in inline content), `NodeSelection` (one selectable node),
`AllSelection` (the whole document) or a custom kind an extension implements (`CustomSelection`: table cells, gap
cursors). Every kind maps through changes, gives its content as a `Slice`, replaces it, and has a bookmark that maps
without a document. `Selection::near` / `find_from` find a valid selection where a position is not one.

## Plugins

A plugin is an `Arc<dyn PluginSpec>` with a unique `PluginKey`; two plugins with one key are an error when the state is
created. Its hooks:

- State: `init_state` when the state is created, `apply_state` for each applied transaction, in plugin order.
- `filter_transaction`: any plugin may refuse a transaction before it applies.
- `append_transaction`: after a transaction (or a round of them) applies, each plugin in order may return one more,
  seeing only the transactions it has not seen. Rounds repeat until no plugin appends; more than 64 rounds is
  `StateError::AppendLoop`, so two plugins answering each other cannot hang the editor.
- Events: `handle_key_down`, `handle_text_input`, `handle_paste` are asked in plugin order and the first that returns
  `true` consumes the event; the view's default handling runs only when none does. Keymaps and input rules are plugins
  of this kind, so their order in the plugin list is their priority.
- `decorations`: inline, widget and node decorations the view draws without changing the document.
- `view_created` / `view_updated` / `view_destroyed`: the plugin's part of a view's life.

Asynchronous work (a link preview, a spell check, a server's answer) never edits a stored state: when the result comes
back it builds a new transaction against the state current then, maps positions it remembered through the changes
since (or looks nodes up by id), checks that what it targets still exists, and dispatches.

## Node views

A node type can have a node view: native UI for a node (an image with a caption, an embed, a math block, a checkbox in
a task item). Its contract:

- `create(node, decorations)`: the view for a node.
- `update(node, decorations) -> bool`: the node changed; `false` has the view recreated.
- `editable_content_slot`: where the node's content renders and stays editable, if it has any; without one the node is
  an atom the view treats as one unit.
- `select` / `deselect`: a node selection takes or leaves the node.
- `handle_event(event) -> bool`: events inside the view; `true` keeps them from the editor.
- `destroy`: the view is gone.

A node view never mutates its node: an attribute change (a resized image, a toggled checkbox) is a transaction
(`set_node_attribute`), so it is undoable, mapped, collaborated and checked like any other change.

## The native view

The view is a Rust component on the core's primitives. Data flows one way: the state renders to views (blocks,
text runs with their marks' styles, decorations, the selection and caret); input from the platform becomes a
transaction; the transaction applies to the state; the new state renders, patching only what changed. The main thread
never waits for JS: key handling, text input, input rules and commands run natively; JS hears of changes afterwards
(`dispatchTransaction`, change events).

IME composition: while the platform composes, the marked text is shown as the platform asks but not committed to the
document; the composition's range maps through transactions that arrive meanwhile (remote changes), and input rules
and keymaps do not run on composing text. The commit is one transaction with origin `Input`. A composition the
editor's own change invalidates (its range deleted) is cancelled with the platform's API, never left out of sync.

## The planned public API

```tsx
import { createRichEditor, RichEditor } from "nsh/base";

const editor = createRichEditor({ schema, content, plugins, onTransaction });
editor.run("toggleMark", "bold"); // a command; false when it cannot apply
editor.can("lift");
editor.isActive("heading", { level: 2 });
editor.dispatchTransaction(tr);

<RichEditor editor={editor} class="…" />;
```

The editor object owns the state in Rust; JS calls it, reads derived values and hears transactions. Parts and styles
follow the other `nsh/base` components: no look of NSH's own, every part styled by the app.

## Phases

1. The headless kernel: model, transform, state, commands, history, keymaps, input rules, the starter schema, formats.
   Done.
2. The native view: rendering, selection and caret, input and IME on each platform, node views, decorations.
3. The base extensions as components (lists, links, images, tables, mentions), `createRichEditor` and `<RichEditor>`
   with their declarations and compat cases; the gallery and website examples.

## Future work: collaboration

Steps are already what collaboration needs: they serialize, map and rebase, transactions carry a `Remote` origin and
the history skips remote changes while mapping its own over them. A collaboration extension (prosemirror-collab's
central authority, or a CRDT binding) comes after the view.
