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

## The native view (`rich_editor/view/`)

`RichEditor` (`nsh/base`) is a Rust component over an `EditorSession` (`view/session.rs`): the one live editor the
view and the app share (`Arc<Mutex<…>>`, `Send + Sync`). The session holds the state, applies transactions
(`dispatch`, or the app's `set_dispatch_transaction` committing with `update_state`), runs commands (`run`, `can`,
`is_active`) and notifies subscribers; `focus` / `blur` are requests to the views showing it. The JS class crosses
through `SessionJs` (`set_session_js`), the `editor` prop's decoder. Without an `editor`, the view makes its own
session from `schema` / `content` JSON and built-in `plugins` by name (`starter`, `history`, `baseKeymap`,
`markdown`).

Data flows one way. The view subscribes to the session (on the engine thread; changes made on other threads show
with the next one made there) and on each new state: gives the document's blocks ids (`controller.rs` `Tree`: a block
keeps its id while it is the same node, or the node at its index with its type), collects the plugins' decorations
and reports `change` (the revision only: the document is read on demand, `docJson()`) and `selectionChange`.

Rendering (`render.rs`): a scroll view of the content box, holding the selection's boxes, the blocks, the
placeholder, the caret and the composition. A block is a view keyed by id whose classes and styles are its node's
part (`classes.nodes` / `styles.nodes` by type name, then `type[attr=value]`), node decorations' `class` and the
`selectedNode` part. Containers hold their children's views; the children of a node whose spec's `extra.listMarker`
is `"bullet"` or `"ordered"` (the starter's lists) carry a marker the view draws in the `listMarker` part, absolutely
placed in the indent before the item (`•`, `◦`, `▪` by bulleted depth, or the number from the list's
`extra.listMarkerStart` attribute, `order` by default), as TextView places its markers; it is not in the document and
takes no positions; a textblock is one text whose spans are its texts with
their marks' parts (`classes.marks`), split at inline decorations (their `class`); inline leaves are their one-unit
`leafText` (a line break) or attachments (their node view, else a box of their part); widget decorations are
attachments with their spec's `class`. A node view (`node_view.rs`: `NodeView`, registered in `NodeViews` or a
plugin's `node_views` as an `Arc<dyn NodeView>`) renders a node type's blocks: `create` makes the element, which
follows `cx.node()` while `update` returns true; `editable_content_slot` puts the editor's rendering of the content
(`cx.take_content()`) inside it, measured in the content box; otherwise the node is an atom selected and moved over
as a unit. The only looks of the view's own are the caret (2 points, the theme's `foreground`), the selection (the
theme's selection color), the placeholder (`muted-foreground`) and the composition (underlined), each a part the app
restyles.

Geometry (`geometry.rs`): blocks report their frames (`onLayout`), each textblock's text its lines as the core laid
them out (`onTextLayout`), and an `InlineMap` maps the textblock's positions to the text's offsets (atoms and
widgets are one character). Positions within a line are placed by the relative widths of grapheme prefixes, so the
caret and hit tests follow the app's fonts (classes included) line by line. Document positions and places stay
apart: a position at a wrap shows at the upper line's end with `Affinity::Upstream`.

Input (`controller.rs`): as the code `Editor`, an invisible native input at the caret (the sink) takes typed text
and input methods. Typed text goes to plugins' `handle_text_input` (input rules), else is inserted with the stored
marks (origin `Input`). While an input method composes, its text shows at the caret and the document waits; the
commit is one transaction tagged with the composition's id (`composition` meta, which the history groups). Keys: the
view binds, while focused, every keystroke a keymap could use (`keys.rs`) and offers each as a `KeyEvent` to the
selected node's view, the plugins' `handle_key_down`, the base keymap, then copy / cut / select all and the caret
movement (graphemes, words, line ends, lines and pages by geometry, keeping the goal x, the document's ends).
Presses: a node view's `handle_event`, the plugins' click hooks (`handle_click_on` per ancestor, then
`handle_click`, double and triple likewise), then a caret (Shift extends), a word, the textblock or a node selection;
a drag extends through `create_selection_between`. Paste: `transform_pasted_text`, `clipboard_text_parser` (else
`slice_from_text`), `transform_pasted`, `handle_paste`, else a replace of the selection (origin `Paste`). Copy:
`clipboard_text_serializer`, else `slice_to_text`.

## The JS API (`rich_editor/js/`)

```tsx
import { createRichEditor, RichEditor, history, keymap, inputRules, markdownRules, markdown, toggleMark } from "nsh/base";

const editor = createRichEditor({ schema, doc, plugins: [history(), keymap({ "Mod-b": toggleMark("bold") }), inputRules(markdownRules()), markdown()] });
editor.run(toggleMark("bold")); editor.can(toggleMark("bold")); editor.isActive("bold");
editor.dispatchTransaction = (tr) => editor.applyTransaction(tr);

<RichEditor editor={editor} class="…" />;
```

- `RichEditorSession` (`#[class]`) holds an `EditorSession` and the schema; `createRichEditor` makes one, and the
  session's `SessionJs` bridge (registered by `js::register`) turns an instance into the view's `editor` prop.
  States, documents, selections and transactions cross as JSON only when the app asks, typed by declared interfaces
  (`RichNodeJSON`, `RichTransactionJSON`, ...; `json.rs`). `defineSchema` reads `nodes` / `marks` objects in the JS
  object's key order (`JsIn::keys`), since serde_json's maps sort their keys.
- Commands and kernel plugins are data (`RichCommand`, `RichPluginSpec`, `RichInputRule`: `JsObject`s made by
  `toggleMark(..)`, `history(..)`, `markRule(..)`, ...), resolved against the editor's schema when they run or the
  editor is made (`commands.rs`, `plugins.rs`). `chain` runs commands on the states each leaves and dispatches their
  steps as one transaction; `first` is ProseMirror's `chainCommands`.
- `RichEditorPlugin` (`#[class]`) is a plugin whose behavior is the app's. Its callbacks live in a thread-local table
  of the engine thread (`HookRef`), since the kernel's plugins and the session are `Send + Sync`. Captured keys are a
  native keymap consumed while `capturing` is set, reported to JS with `post`; decorations are data a transaction
  carries under the plugin's key and the plugin maps natively; `filterTransaction` / `appendTransaction` are called
  with `Callback::call` only inside the app's own synchronous calls (`CallGuard`), so the platform's input never
  waits for JS. `dispatchTransaction` is called in the app's calls and posted for input; the Rust transaction waits
  under its `id` until `applyTransaction` takes it, keeping its Rust metadata (history).
- Methods that may run JS hooks (`run`, `dispatch`, `setContent`, ...) take `this: Instance<Self>` rather than a
  borrow, so a hook calling the editor again (`applyTransaction` from `dispatchTransaction`) is not re-entrant.

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
