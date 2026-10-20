# TextView

`TextView` (`nsh/base`, `crates/nsh-base/src/text_view/`) shows a document of Markdown, HTML or the app's own blocks
with the primitives: texts with spans, views, scroll views and images. It is the port of gpui-kit's `TextView`, made
for the replies of AI models in chat UIs: Markdown that streams in chunk by chunk, with code, tables, formulas and the
app's own syntax (mentions, charts, callouts). It owns behavior (parsing, links, disclosures, accessibility) and,
unlike the other `nsh/base` components, has a default reading typography (the one exception to "No styles in NSH"),
colored with the theme's tokens; the app restyles each part over it.

| File | What |
|---|---|
| `view.rs` | The component: `TextViewProps`, `TextViewEvent` (`LinkPress`), `TextViewMath`, `CodeHighlighter` (context); the blocks made with primitives (`Cx`) |
| `document.rs` | The document model (`Document`, `Block`, `Inline`, `CodeBlock`, `Table`, `Image`, `CustomNode`, ...): serde data, JSON in JS, its TypeScript declarations (`TextViewDocument`, `TextViewBlock`, ...) |
| `parts.rs` | The parts the app styles: one `parts!` list makes `TextViewClasses`, `TextViewStyles` (each a `PartStyle`, a `Style` declared `StyleProp<TextStyle>`, since most parts are texts) and `TextViewPart`; `TextViewParts` (context): typed `StyleRefinement` functions by part for Rust views |
| `extension.rs` | `MarkdownOptions`, `MarkdownExtension` (declared syntax), `MarkdownPlugin` / `MarkdownPlugins` (Rust), `Parser` |
| `markdown/` | Markdown into the model (markdown-rs's mdast); `container.rs`: `:::` containers, split out before parsing (markdown-rs has no such syntax); `streaming.rs`: streaming repair |
| `html/` | HTML into the model (html5ever, as a reader mode reads a page) |
| `common.rs` | What both parsers share: `pattern` extensions on inline content, headings' anchors |

## How it works

- **Parsed in Rust, once per change.** One `computed` reads `markdown`, else `html`, else `document`, with `options`,
  `extensions` and the `MarkdownPlugins` of the context, into a `Document`. JS only passes strings (or the `document`
  JSON); parsing never runs in QuickJS. The `Parser` is public, so Rust code reads documents without a view.
- **Blocks are rows of an `Each`, keyed by position and kind** (`(index, block.kind())`). A streamed reply grows at its
  end: the blocks before it parse to equal values and keep their nodes, the block being written keeps its row (same
  position, same kind) and only that row is made again. A block that changes kind mid-stream (a paragraph becoming a
  table once its delimiter row arrives) gets a new row. Footnotes follow the blocks in one region of their own.
- **Built on `Text`** (`text.md`): every paragraph, heading, table cell and code block is one native text
  block, so wrapping, font scale, selection, span backgrounds (inline code), pressable
  spans (links) and inline attachments (images, inline custom nodes, the caret) are the text element's, with nothing of
  `TextView`'s own; the website page links to the Text page for them rather than describing them again.
- **Each block is made with primitives** (`Cx::block`): a paragraph or heading is one `text` whose children are spans
  (`Inline` → nested texts, inline images, slots), lists are rows of a marker text and a column, code blocks a column
  with a header row and a horizontal `scroll`, tables one grid in a horizontal `scroll` (below), disclosures a
  pressable summary and an `If`. With `selectable`, every text carries `selectable` and the root is a selection area
  (`Element::selection_area`), so one selection runs across the blocks, and inside a `SelectionArea` on into the
  texts around the view ("Selection areas" in `text.md`).
- **Tables** (`Cx::table`) lay out as gpui-kit's: columns fit their content when there is room, shrink (their cells
  wrapping) down to a floor per column when the view is narrower, and below the floors the table scrolls horizontally
  (`tableScroll`; `tableScrollIndicator` hides its indicator, as `codeBlockScrollIndicator` a code block's). The cells are one CSS grid, so columns line up across rows: each column is a `minmax(<floor>,
  auto)` track (the spare width is shared out as CSS shares it among `auto` tracks), and the grid is `w-full` with
  the floors' sum as its `min-width`, so past the floors it is wider than the scroll view. Taffy has no
  `display: contents` and text has no min-content (longest word) measure, so the floor is computed in the component
  (`column_floor`): about half the column's widest line, estimated from the cells' text (wide characters an em, the
  rest about half), clamped to 10–30 rem, or the whole line plus 2 rem for padding when that is narrower. A row is a
  view behind its cells (`row_start`, `col_span_full`, before them in the grid), so `tableRow` / `tableHead` draw
  backgrounds and borders; their text styles do not reach the cells, which are siblings (`tableCell` /
  `tableHeaderCell` take them).
- **The caret slot** (`caret`, `Option<Slot>`) is placed inline at the end of the last text of the last block
  (descending into quotes, lists and custom nodes' content; after the code of a code block; a table's last cell), else
  on a line of its own after the last block (a rule, a render function's content), so it follows the text as it
  streams.
- **Static functions** (`#[module] impl TextView`, synchronous): `TextView.parse(markdown, options?, extensions?)`,
  `TextView.parseHtml(html, extensions?)` and `TextView.plainText(document)` (`Document::plain_text`: what copying a
  whole reply gives); in Rust the same associated functions. The document types stay under `nsh_base::text_view`
  (short names such as `Block` and `Image` would clash at the crate root); their TS names are `TextView*`.
- **Render functions** (`Option<Renderer<T>>` props, `docs/dev/components.md`) put the app's content in place of a part
  (`renderCodeBlock`, `renderMath`, `renderImage`, `renderCustom`) or beside it (`renderCodeBlockActions` in the code
  block's header, `renderTableActions` above a table). Each render has a key of the block's position (`code:3`,
  `math:2.0.1`, ...); content rendered for a key stays while the value is equal, so a streamed reply renders the app's
  code blocks again only while they are being written.
- **Links report presses**: `onLinkPress(url)`; a heading's anchor is `#<slug>` (GitHub's slugs, `common.rs`), a
  footnote reference `#footnote-<label>`. Nothing opens a URL: the app decides.

## Parts and their default styles

The defaults are a reading typography modeled on Allsum's documents (the constants and `Heading` at the end of
`view.rs`), its sizes in points: text 15 in 25.5 lines (1.7), blocks 11.25 apart (0.75 em); headings semibold, 27 /
20 / 17 / 15 with lines of 1.25 (level 1) or 1.35 and letter spacing of -0.02 em (level 1) or -0.012 em, with room
above them after a block (1.1 / 1.6 / 1.4 em, 0.6 em after a heading, none first: their top margin less the gap,
from `Before`, which the top level's rows keep as a memo); strong text semibold; links underlined 1 pt at 40 % of
their color, 3 pt below; `<mark>` on a rounded yellow; code, keys and TeX at 13 in `monospace` on a faint rounded
background (spans have no padding); lists indented 1.4 em, their markers (`•`, `◦`, `▪` by depth, or numbers) right-aligned against the content in
the indent (so `10.` reaches further left and the items stay in line), items 0.25 em apart, tasks a
15 pt box drawn as a view (filled with a `✓` when done, the item's text then muted and struck); quotes a 2 pt line at
their side (the text's color at 15 %, as rules), their text softer; alerts a tinted, bordered box (radius 8, padding 12 / 16) with a ring icon (`alertIcon`);
code blocks a bordered, rounded box (padding 28 / 16 / 14, 13 in 20.8 lines) with their header over the top right
corner (the language at 11, then the actions); tables at 14 in lines of 1.55, cells padded 8 / 12 with 1 pt lines
(each cell its right and bottom ones, the grid the top and left ones), the header row on the list head color with
13 pt muted text; a rule 1 pt with 1.6 em above and below; images rounded 8; footnotes at 13, muted. Colors are
the theme's (`nsh_base::theme()`: `foreground`, `muted_foreground`, `border`, `muted`, `link`, `list_head`, the
status colors `info` / `success` / `warning` / `danger` for alerts, violet for `[!IMPORTANT]` unless the theme sets
`important.background`), read in bindings, so a theme or the appearance changes them natively. Everything is the
app's to override: `classes` (Tailwind, applied after the part's own styles, so they win) and `styles` (inline, over
the classes), by part, and `class` / `style` on the root, whose text styles every part inherits (the root's defaults
are the text's size, line height and color). The root is also the part `root` (its `styles.root` is typed
`StyleProp<TextStyle>`, which the component's own `style`, a `ViewStyle`, is not). Line heights are points, not
multiples (typed styles have no multiple yet): an app that changes a part's size sets its line height too, as
Tailwind's `text-*` classes do. A node with several parts (`heading` then `heading2`; `blockquote`, `alert`, `alertNote`; `listMarker`,
`taskMarker`, `taskMarkerChecked`; `tableRow`, `tableHead`; `tableCell`, `tableHeaderCell`) gets their classes in that
order and their styles merged, later parts winning. The full list is `parts.rs`; a new part is one line of `parts!`.
Rust views also style parts with typed styles: `provide_context(TextViewParts::new().part(TextViewPart::Code, |s| ..))`
refines a part's `StyleRefinement` (theme colors stay adaptive) before the app's `classes` and `styles` apply.

## Syntax highlighting

Syntax highlighting is the app's: NSH ships no highlighter (no grammars, no themes). A TS app replaces code blocks with
`renderCodeBlock`. A Rust view provides a `CodeHighlighter` (`provide_context`): a function from a `CodeBlock` to
`(Range<usize>, StyleRefinement)` pairs, byte ranges in order (`syntect`, `tree-sitter`, ...); `TextView` turns them
into spans of the code block's text, skipping ranges out of order, overlapping or off a character boundary, and keeps
its header, actions, scroll view and parts. `renderCodeBlock` wins over the highlighter.

## Extensions

- **Declared** (`MarkdownExtension`, a `JsObject`: TS apps and Rust alike) turn syntax into `CustomNode`s named by
  the extension, which the app shows with `renderCustom`. One of: `pattern` (a Rust `regex` matched in text outside
  code, links, keys, formulas and other custom nodes; the match is `text`, groups are attributes `"1"`, ... and by
  name; compiled once per thread, `common.rs`, cache bounded at 256 patterns, an invalid one warns once), `language`
  (fenced code of that language; the code is `text`, the rest of the info string `meta`), `tag` (HTML and MDX elements,
  any case; attributes, content parsed as blocks or inline content) or `container` (`:::name[label]{attrs}` …
  `:::`, as `remark-directive`).
- **Rust plugins** (`MarkdownPlugin`) read mdast nodes themselves: `configure` turns on markdown-rs constructs,
  `block` / `inline` read a node (the first plugin returning `Some` wins; `ParseContext` reads children as the parser
  does), `render` shows a custom node in Rust. A view provides them to every `TextView` below it with
  `provide_context(MarkdownPlugins::new().with(..))`. A plugin's `render` comes before `renderCustom`, which comes
  before the node's content.
- **`MarkdownOptions`**: `math`, `html`, `alerts` (default on), `frontmatter`, `mdx`, `streaming` (default off).
  With `mdx`, JSX elements are custom nodes named by their tag and `{expressions}` custom nodes `expression`; raw HTML
  is then text.

## Streaming

With `options.streaming`, `markdown/streaming.rs` repairs the last block of the text before parsing (the blocks before
it are complete; a text ending inside a fenced code or `$$` block needs nothing): emphasis, strong, strikethrough
(`**`, `*`, `__`, `_`, `~~`), inline code and formulas left open are closed in the reverse order they opened, a marker
with nothing after it yet is removed; `[text` and `[text](partial` show as their text, `![alt` and `![alt](partial`
are removed; an HTML tag being typed (`<di`, `<a href="x`) is removed; a table's header followed by the start of its
delimiter row is completed to a table (and a lone `-` under it is no heading). So a reply never flashes literal `**`
or `[` while the next chunk is on its way. The repair is a `Cow`: text with nothing open is parsed as it is.

## HTML

`html/` reads fragments and whole pages (their body) through html5ever, so unclosed and misplaced tags build the tree a
browser builds. It walks the tree in block or inline context (runs of inline content between blocks become
paragraphs), collapses whitespace as a browser does except in `<pre>`, skips what a reader does not show (`DROPPED`:
metadata, scripts, styles, form controls, embedded media; elements `hidden`, `aria-hidden="true"` or inline
`display: none`, such as KaTeX's HTML beside its MathML), and reads GitHub's own markup back (alerts, task list
checkboxes, `<math>` with a TeX annotation, `language-*` classes of code). Depth is bounded (256) and `colspan` too
(64): the input is untrusted.

## Accessibility

Headings have role `header`, links (and footnote references) role `link` with their title as the hint, task markers
role `checkbox` with their checked state, a disclosure's summary role `button` with its `expanded` state, images role
`image` with their alt text as the label. The texts are real texts, so screen readers read the document in order.

## Performance

- The whole text is parsed again on each change (one `computed`); parsing is Rust and linear, and the rows that did
  not change make no mount items. A streamed reply of N chunks costs N parses of a growing text (no incremental
  parse); a baseline belongs in `performance.md` once measured.
- Rows are made again only when their value changes (`Each` with the position and kind as the key, the block as the
  value), and render functions only when their value changes.
- A disclosure makes its content only while open.
- Images default to their natural size (one `image_size` read each), at most the width of the text.

## From gpui-kit

| gpui-kit | NSH |
|---|---|
| `TextView::markdown` / `::html` | `markdown` / `html` props; `document` for blocks the app builds |
| `TextViewStyle` (colors, code block style, heading sizes) | A default typography in the theme's colors; `classes` / `styles` by part; `TextViewParts` (typed, Rust) |
| Selection across the view | `selectable`: the view is a selection area, one selection across its blocks (drag, double / triple click, Shift+click; long press and handles); `SelectionArea` joins several views and texts |
| Code block actions (copy) | `renderCodeBlockActions` |
| Highlighter (tree-sitter) | The app's: `CodeHighlighter` (Rust, styled ranges) or `renderCodeBlock` (TS) |
| Table actions | `renderTableActions` |
| `MarkdownPlugin` | `MarkdownPlugin` (Rust) and `MarkdownExtension` (declared, from TS too) |
| `image_source` / image loading | `renderImage`; the default loads the URL with the runtime's image loader |
| Link clicks | `onLinkPress(url)` |
| Streaming fade / motion | The app's: the blocks are views it styles and animates (`nsh/animated`, layout animation) |

## Limits

- Anchors are reported, not scrolled to: the app scrolls its `Scroll` to the heading it finds.
- A table column's floor is estimated from its text, not measured (Tables, above): inline images and custom nodes count for nothing.
- Spans take no padding: inline code and `<mark>` backgrounds hug their characters.
