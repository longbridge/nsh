---
title: TextView
description: "The TextView component: Markdown, HTML or a document of blocks shown with native text and views, styled part by part, for AI chat replies, articles and help pages."
order: 11.5
demo: text-view
---

# TextView

`TextView` shows a document written in Markdown, in HTML or as blocks the app builds: headings, paragraphs, emphasis,
links, lists and task lists, quotes and GitHub's alerts, code, tables, images, formulas, footnotes and disclosures. It
is made for the replies of AI models in a chat: the reply streams in as Markdown, and each chunk shows the text with its
formatting in place. Parsing happens in Rust, and the document is drawn with [`Text`](./text.md) elements and views.

It owns behavior and comes with a reading typography of its own: 15pt text in generous 25.5pt lines, semibold
headings with room above them, links underlined in the theme's link color, code in a bordered, rounded box, tables ruled on every side with a tinted header row, quotes with a line at their side and
alerts as tinted boxes with an icon. Its colors are the theme's tokens (`foreground`, `muted.foreground`, `border`,
`muted.background`, `link`, `list.head.background` and the status colors), so a theme and the dark appearance reach
it. `classes` and `styles` restyle each part over these defaults.

```tsx
import { mount, Scroll } from "nsh";
import { TextView } from "nsh/base";

const README = `# Release notes

**Faster lists** and a new \`TextView\`:

- Markdown and HTML
- [x] Streaming replies
- [ ] Your own syntax

> [!TIP]
> Style every part with \`classes\`.`;

function App() {
  return (
    <Scroll class="flex-1" contentClass="p-5">
      <TextView
        markdown={README}
        class="text-base text-neutral-900 dark:text-neutral-100"
        classes={{
          heading1: "text-3xl",
          code: "rounded bg-neutral-100 dark:bg-neutral-800",
          link: "text-blue-600",
          alertTip: "border-l-4 border-green-600 pl-3",
        }}
        onLinkPress={(url) => console.log("open", url)}
      />
    </Scroll>
  );
}

mount(App);
```

## Built on Text

`TextView` is made of [`Text`](./text.md) elements, and its text behaves exactly as the Text page describes. Every
paragraph, heading, table cell and code block is one native `Text`: one block that lays out, wraps, follows the font
scale and, with `selectable`, selects and copies. Strong, emphasis, struck-through and the other inline marks
are its [spans](./text.md#spans); inline code is a span with a [span background](./text.md#span-backgrounds), so
`code: "rounded bg-neutral-100"` gives it a rounded background; links are
[pressable spans](./text.md#links); images and inline custom nodes are
[inline attachments](./text.md#inline-views-and-images). Text styles on the root pass to every part, as
[text styles](./text.md#text-styles) pass from a `Text` to its spans.

With `selectable`, the whole document selects as one, as a web page does: a drag (or, on touch screens, a long press
and the handles) runs from one paragraph, list item, cell or code block to the next, and Copy takes the text of them
all. A `selectable` `TextView` is a [`SelectionArea`](../modules/nsh-base.md#selectionarea) of its own; inside a
`SelectionArea`, its selection also runs on into the texts and other `TextView`s around it.

What `TextView` adds is the rest: reading Markdown, HTML or a document into blocks, the views around the texts (lists,
quotes, tables, code blocks, disclosures, footnotes), streaming, styles by part, render functions and extensions.

## The document

The content is one of three props, read in this order:

- **`markdown`**: CommonMark with GitHub's additions (tables, task lists, strikethrough, autolinks, footnotes, alerts),
  formulas (`$…$` and `$$…$$`) and HTML inside the Markdown.
- **`html`**: HTML as a reader shows it. Text, structure and inline formatting come through; scripts, styles, form
  controls and hidden elements stay out. A fragment or a whole page both work (a page shows its body).
- **`document`**: blocks the app builds or keeps, as plain objects ([The document model](#the-document-model)).

```tsx
<TextView html={`<h2>Shipping</h2><p>Orders ship in <b>2 days</b>. <a href="/faq">FAQ</a></p>`} />

<TextView document={{
  blocks: [
    { type: "heading", level: 2, content: [{ type: "text", text: "Total" }] },
    { type: "paragraph", content: [{ type: "strong", content: [{ type: "text", text: "$42.00" }] }] },
  ],
}} />
```

`TextView` grows with its content. Put it in a [`Scroll`](./scroll.md) (or a row of a [`List`](./list.md)) for long
documents.

## Styling the parts

`class` and `style` style the root, a column of the blocks; text styles set there (color, size, font) pass to every
part. The root is also the part `root`: `styles.root` takes text styles with their types (`style` is typed as a
view's), and `classes.root` classes. `classes` takes Tailwind classes by part and `styles` inline styles by part. A part's classes come after its own
default styles, so they win; its `styles` win over its classes. Line heights in the defaults are set in points: a
class that changes a part's size sets its line height too (Tailwind's `text-*` classes do).

```tsx
<TextView
  markdown={reply()}
  class="gap-3 text-[15px] leading-6 text-neutral-800"
  classes={{
    heading: "text-neutral-950",
    heading2: "text-xl mt-2",
    blockquote: "border-l-2 border-neutral-300 pl-3 text-neutral-600",
    codeBlock: "rounded-lg bg-neutral-100",
    codeBlockHeader: "px-3 pt-2",
    codeBlockCode: "p-3 text-[13px]",
    table: "rounded-lg border border-neutral-200",
    tableHead: "bg-neutral-50",
    tableCell: "px-3 py-2 border-t border-neutral-200",
    taskMarkerChecked: "text-green-600",
  }}
  styles={{ root: { fontFamily: "Inter" }, link: { color: "#2563eb" } }}
/>
```

Some nodes are several parts, which apply in order: a level 2 heading is `heading` then `heading2`; an alert is
`blockquote`, `alert`, then its kind (`alertNote`, ...); a task marker is `listMarker`, `taskMarker`, then
`taskMarkerChecked` when done; the header row is `tableRow` then `tableHead`, and its cells `tableCell` then
`tableHeaderCell`.

| Part | What it is |
|---|---|
| `root` | The text view itself, a column of its blocks; text styles here pass to every part. |
| `paragraph` | Every paragraph. |
| `heading` | Every heading, before its level's part. |
| `heading1` … `heading6` | Headings of that level (27, 20, 17, then 15pt, semibold). |
| `blockquote` | Quotations and alerts: a column of their blocks with a line at their side. |
| `alert` | Every alert (`> [!NOTE]`), after `blockquote`: a tinted box, a row of its icon and its blocks. |
| `alertNote`, `alertTip`, `alertImportant`, `alertWarning`, `alertCaution` | Alerts of that kind. |
| `alertIcon` | The icon of an alert: a ring with a glyph in the alert's color. |
| `list` | Every list: a column of its items. |
| `orderedList`, `unorderedList` | Numbered and bulleted lists, after `list`. |
| `listItem` | Every item: a row of its marker and its blocks. |
| `listMarker` | The marker of an item (`•`, `◦`, `▪` by depth, or `1.`), right-aligned in the indent before the item. |
| `taskMarker` | The marker of a task item: a box drawn as a view, filled with a `✓` when done; after `listMarker`. |
| `taskMarkerChecked` | The marker of a done task item, after `taskMarker`. |
| `listItemContent` | The column of an item's blocks. |
| `codeBlock` | Code blocks: a bordered, rounded box of the code, with the header over it. |
| `codeBlockHeader` | The row of a code block's actions (`codeBlockActions`), floating over its top right corner. |
| `codeBlockScroll` | The horizontal scroll view around a code block's code. |
| `codeBlockCode` | The code of a code block (monospace, as written). |
| `code` | Code in text (monospace, on a rounded background). |
| `keyboard` | Keys (`<kbd>`, as code). |
| `strong` | Strong text (semibold). |
| `emphasis` | Emphasized text (italic). |
| `strikethrough` | Struck-through text. |
| `underline` | Underlined text (`<u>`, `<ins>`). |
| `highlight` | Highlighted text (`<mark>`, on a rounded yellow). |
| `superscript`, `subscript` | Superscript and subscript (smaller). |
| `link` | Links (in the link color, underlined). |
| `image` | Images. |
| `math` | Formulas in text, shown as their TeX in monospace when there is no `renderMath`. |
| `mathBlock` | Formula blocks, shown as their TeX in monospace when there is no `renderMath`. |
| `table` | Tables: `renderTableActions`' content, then the scroll view of the cells. |
| `tableScroll` | The horizontal scroll view of a table's cells: columns fit their content when there is room, wrap down to a floor when the view is narrower, and past that the table scrolls. |
| `tableRow` | Every row of a table: a view behind its cells, for backgrounds and borders (text styles go on `tableCell`). |
| `tableHead` | The header row, after `tableRow`. |
| `tableCell` | Every cell. |
| `tableHeaderCell` | The header row's cells (smaller, muted), after `tableCell`. |
| `rule` | Thematic breaks: a line drawn as a top border. |
| `details` | Disclosures (`<details>`). |
| `summary` | The summary of a disclosure; a press opens and closes it. |
| `summaryMarker` | The marker of a summary (`▸`, `▾`). |
| `footnotes` | The footnotes after the blocks: a column. |
| `footnote` | A footnote: a row of its number and its blocks. |
| `footnoteLabel` | The number of a footnote. |
| `footnoteReference` | A reference to a footnote in text (its number, smaller). |
| `custom` | Custom nodes shown as their content (when no render function shows them). |

## Streaming AI replies

Pass the reply as it grows, with `options={{ streaming: true }}`. The end of a text that is still arriving often leaves
syntax open (`**bold`, `` `code ``, `[a link](htt`, a table whose header just arrived); streaming shows it as it will
look once closed, so the reader never sees stray `**` or `[`. Blocks are kept by position: the blocks that are complete
keep their views, and only the block being written is made again, so a long reply stays cheap to update.

`caret` follows the end of the document: inline at the end of the last text, wherever the last block ends (a
paragraph, a list item, a code block, a table's last cell), else on a line of its own after the last block (a rule,
content of a render function). Remove it when the reply is done.

```tsx
import { signal, View } from "nsh";
import { TextView } from "nsh/base";

function Reply(props: { stream: AsyncIterable<string> }) {
  const text = signal("");
  const done = signal(false);
  (async () => {
    for await (const chunk of props.stream) text.update((t) => t + chunk);
    done.set(true);
  })();
  return (
    <TextView
      markdown={text()}
      options={{ streaming: !done() }}
      caret={done() ? undefined : <View class="ml-0.5 h-4 w-2 rounded-sm bg-neutral-400" />}
    />
  );
}
```

The caret is the app's own view, so it can blink or pulse with [`nsh/animated`](../modules/nsh-animated.md), and new
blocks can fade in with the app's animations.

## Links

Links are [pressable spans](./text.md#links). A press on one calls `onLinkPress` with its URL; the app decides what it opens (a page of its own, a
[`WebView`](./web-view.md), a scroll to a heading). Headings have anchors (GitHub's: `## Getting started` is
`#getting-started`), so a link to `#getting-started` arrives as that string, and a footnote reference arrives as
`#footnote-<label>`.

```tsx
<TextView
  markdown={doc}
  onLinkPress={(url) => {
    if (url.startsWith("#")) scrollToAnchor(url.slice(1));
    else nav.push({ name: "web", url });
  }}
/>
```

## Render functions

Render functions put the app's own content into the document. Each one takes the value of a part and returns JSX; a
part keeps what was rendered for it while its value stays the same, so during streaming only the part being written is
rendered again.

| Prop | Value | Replaces or adds |
|---|---|---|
| `renderCodeBlock` | `TextViewCodeBlock` | Replaces a code block (syntax highlighting of your own). |
| `renderCodeBlockAction` | `TextViewCodeAction` | The icon of a code block action of the app's (`codeBlockActions`); the button around it is `TextView`'s. |
| `renderTableActions` | `TextViewTable` | Adds content above a table (copy, export). |
| `renderMath` | `TextViewMath` | Replaces a formula: `{ tex, display }`, `display` for a `$$…$$` block. |
| `renderImage` | `TextViewImage` | Replaces an image (`{ src, alt, title }`): your own loading, sizing, a viewer on press. |
| `renderCustom` | `TextViewCustomNode` | Shows a custom node, by its `name` ([Markdown extensions](#markdown-extensions)). |

Code blocks carry actions over their top right corner, all drawn alike: ghost buttons whose background shows while
hovered or pressed, shown while a pointer is over the block on desktops and always on touch screens. `copy` is built
in: it copies the code and shows a check for a moment. Other actions are the app's: `renderCodeBlockAction` draws
their icon and `onCodeBlockAction` reports their presses (the built-in `copy` too, after it copied). `copy` writes the
clipboard as `nsh/clipboard` does, so it needs the app's `clipboard` capability (`nsh.json`).

```tsx
import { Text } from "nsh";
import { TextView } from "nsh/base";

<TextView
  markdown={reply()}
  codeBlockActions={[
    { id: "copy", label: "Copy code" },
    { id: "run", label: "Run" },
  ]}
  renderCodeBlockAction={(action) => <Text class="text-xs">▶</Text>}
  onCodeBlockAction={({ id, code }) => id === "run" && run(code.code)}
/>
```

Code blocks are highlighted out of the box: for a language named on the fence (`ts`, `tsx`, `rust`, `python`, `bash`,
`json`, `toml`, `yaml`, `diff` and about two hundred more), the code shows in Allsum's code palette, light or dark with the
appearance; a block without a language shows plain. An app with highlighting of its own shows its code with
`renderCodeBlock`, from its own component or one of its Rust crate; a Rust view gives `TextView` its highlighter with
[`CodeHighlighter`](#syntax-highlighting). Here code with highlighting of the app's own, and an image that fills the
width:

```tsx
import { View } from "nsh";
import { Image } from "nsh/image";

<TextView
  markdown={reply()}
  renderCodeBlock={(code) => <Highlighted language={code.language} code={code.code} />}
  renderImage={(image) => (
    <Image src={image.src} accessibilityLabel={image.alt} class="aspect-video w-full rounded-lg" />
  )}
  renderMath={(math) => <MathView tex={math.tex} display={math.display} />}
/>
```

`Highlighted` and `MathView` stand for the app's own components (a highlighter, a formula renderer, or a component of
its Rust crate). Without `renderMath` a formula shows as its TeX in monospace (`math` and `mathBlock` parts).

## Markdown extensions

`extensions` turns syntax of the app's into custom nodes, which `renderCustom` shows. Each extension has a `name`
(the `name` of the nodes it makes) and one of four kinds of syntax:

| Kind | Matches | The node |
|---|---|---|
| `pattern` | A regular expression in text (Rust's `regex` syntax), outside code and links | Inline; `text` is the match, `attributes` its groups: `"1"`, `"2"`, … and named groups by name |
| `language` | Fenced code blocks of that language (```` ```chart ````) | A block; `text` is the code, `attributes.meta` the rest of the info string |
| `tag` | HTML or MDX elements of that tag, any case (`<Callout kind="tip">`) | Its attributes, its content parsed: `children` (blocks) for an element on its own lines, `content` (inline) inside text |
| `container` | Containers `:::name` … `:::` | A block of the Markdown inside (`children`); `:::note[Heads up]` gives `attributes.label`, `{key=value}` the other attributes |

````tsx
import { Text, View } from "nsh";
import { TextView } from "nsh/base";
import type { MarkdownExtension, TextViewCustomNode } from "nsh/base";

const extensions: MarkdownExtension[] = [
  { name: "mention", pattern: "@(\\w+)" },
  { name: "chart", language: "chart" },
  { name: "callout", tag: "callout" },
  { name: "note", container: "note" },
];

const message = `Thanks @ada! Here are the numbers:

\`\`\`chart
12, 19, 7, 25
\`\`\`

<Callout kind="tip">Charts update live.</Callout>

:::note[Heads up]
Numbers are in thousands.
:::`;

function Custom(props: { node: TextViewCustomNode }) {
  const node = props.node;
  switch (node.name) {
    case "mention":
      return <Text class="font-semibold text-blue-600">{node.text}</Text>;
    case "chart":
      return <BarChart values={node.text!.split(",").map(Number)} />;
    case "callout":
      return (
        <View class="rounded-lg bg-amber-50 p-3">
          <TextView document={{ blocks: node.children ?? [] }} />
        </View>
      );
    default:
      return (
        <View class="gap-1 border-l-4 border-blue-500 pl-3">
          <Text class="font-semibold">{node.attributes?.label ?? "Note"}</Text>
          <TextView document={{ blocks: node.children ?? [] }} />
        </View>
      );
  }
}

<TextView markdown={message} extensions={extensions} renderCustom={(node) => <Custom node={node} />} />;
````

`BarChart` stands for the app's own chart. The mention pattern's group is in `node.attributes?.["1"]` (`ada`) and the
whole match in `node.text` (`@ada`). A custom node with no render function shows its content.

### Options

`options` picks the Markdown syntax read beyond CommonMark and GitHub's:

| Key | Type | Default | Description |
|---|---|---|---|
| `math` | `boolean` | `true` | Formulas: `$…$` in text, `$$…$$` blocks. |
| `html` | `boolean` | `true` | HTML inside the Markdown is read as the `html` prop reads it; `false` shows it as text. |
| `alerts` | `boolean` | `true` | GitHub's alerts: `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`. |
| `frontmatter` | `boolean` | `false` | YAML front matter (`---` … `---` first) becomes a custom node `frontmatter` whose `text` is the YAML. |
| `mdx` | `boolean` | `false` | MDX: JSX elements are custom nodes named by their tag, `{expressions}` custom nodes `expression`; raw HTML is then read as text. |
| `streaming` | `boolean` | `false` | The text is still arriving: syntax its end leaves open shows as if it were closed. |

## The document model

The `document` prop and the render functions' values are plain objects, each node tagged by its `type`. The types
come from `nsh/base`: `TextViewDocument`, `TextViewBlock`, `TextViewInline`, `TextViewCodeBlock`, `TextViewTable`,
`TextViewListItem`, `TextViewImage`, `TextViewFootnote`, `TextViewAlert` and `TextViewCustomNode`.

| Block `type` | Fields |
|---|---|
| `paragraph` | `content` |
| `heading` | `level` (1–6), `content`, `anchor?` |
| `blockquote` | `children`, `alert?` (`"note"`, `"tip"`, `"important"`, `"warning"`, `"caution"`) |
| `list` | `ordered?`, `start?`, `loose?`, `items` (`{ checked?, children }`) |
| `codeBlock` | `language?`, `meta?`, `code` |
| `math` | `tex` |
| `table` | `align?` (`"none"`, `"left"`, `"center"`, `"right"` per column), `head` (cells), `rows?` (rows of cells); a cell is `{ content }` |
| `rule` | |
| `details` | `open?`, `summary` (inline), `children` |
| `custom` | `name`, `attributes?`, `text?`, `data?`, `children?`, `content?` |

| Inline `type` | Fields |
|---|---|
| `text` | `text` |
| `strong`, `emphasis`, `strikethrough`, `underline`, `highlight`, `superscript`, `subscript` | `content` |
| `code` | `code` |
| `keyboard` | `text` |
| `link` | `url`, `title?`, `content` |
| `image` | `src`, `alt?`, `title?` |
| `math` | `tex` |
| `break` | |
| `footnoteReference` | `label`, `index` |
| `custom` | as the block |

A document also has `footnotes?`: `{ label, index, children }`, shown after the blocks in the order of their first
reference. An app can store replies as documents, build them from its own data, or mix in custom nodes that
`renderCustom` shows.

### Parsing and plain text

`TextView` also reads documents without showing them, synchronously:

| Function | Returns |
|---|---|
| `TextView.parse(markdown, options?, extensions?)` | The `TextViewDocument` the `markdown` prop would show. |
| `TextView.parseHtml(html, extensions?)` | The `TextViewDocument` the `html` prop would show. |
| `TextView.plainText(document)` | The document's text without markup: blocks separated by blank lines, list items and table rows by line breaks, cells by tabs. |

A copy button for a whole reply:

```tsx
<Button onPress={() => Clipboard.setString(TextView.plainText(TextView.parse(reply())))}>
  <Text>Copy</Text>
</Button>
```

An app can also parse once and keep the document (`document={doc}`), or change it before showing it.

## In Rust

Rust components and Rust apps use the same component from the `nsh-base` crate. Props are `TextViewProps`, events
`TextViewEvent`, and render functions are closures (`Renderer::new`):

```rust
use nsh_base::{MarkdownOptions, TextView, TextViewClasses, TextViewEvent, TextViewProps};
use nsh_base::text_view::CodeBlock;
use nsh_core::ui::{prelude::*, Renderer};

#[derive(Clone, Copy, PartialEq, Props)]
pub struct ReplyProps {
    /// The reply so far.
    pub text: Prop<SharedString>,
    /// More of it is on its way.
    pub streaming: Prop<bool>,
}

#[component]
pub fn Reply(props: ReplyProps) -> impl IntoElement {
    TextView::element(TextViewProps {
        markdown: Prop::bind(move || Some(props.text.get())),
        options: Prop::bind(move || MarkdownOptions { streaming: props.streaming.get(), ..Default::default() }),
        classes: TextViewClasses {
            heading: Some("font-semibold".into()),
            code_block: Some("rounded-lg bg-muted p-3".into()),
            ..Default::default()
        }
        .into(),
        selectable: true.into(),
        code_block_actions: vec![TextViewCodeAction::new("copy").with_label("Copy code")].into(),
        ..Default::default()
    })
    .on_event(|TextViewEvent::LinkPress(url): &TextViewEvent| open_link(url))
}
```

Here `open_link` stands for the app's own function. The functions of
[Parsing and plain text](#parsing-and-plain-text) are associated functions in Rust: `TextView::parse(markdown,
options, extensions)`, `TextView::parse_html(html, extensions)` and `TextView::plain_text(document)`. The document
types are under `nsh_base::text_view` (`Document`, `Block`, `Inline`, `CodeBlock`, `Table`, `Image`, `CustomNode`,
...), where `Parser` also reads documents with plugins (`Parser::new().plugin(..).markdown(source)`), and
`Document::plain_text()` gives a document's text.

### Syntax highlighting

`TextView` highlights code with syntect in Allsum's code palette (`nsh-base`'s feature `highlight`, on by default);
a large block shows plain at once and takes its colors when a background thread has highlighted it.
`CodeHighlighter` is the app's highlighter in its place, given to every `TextView` below a view with `provide_context`.
It gets each code block and returns the styles of byte ranges of its code (in order, not overlapping; what they leave
out is plain); `TextView` shows the code as spans with those styles, inside its usual code block (header, actions,
scroll view, parts). The highlighter can be anything that turns code into styled ranges: `tree-sitter`, syntect with
a theme of the app's, ...

```rust
use std::ops::Range;

use nsh_base::{CodeHighlighter, text_view::CodeBlock};
use nsh_core::ui::prelude::*;

/// The app's highlighter (here with syntect, set up once by the app).
fn highlight(code: &CodeBlock) -> Vec<(Range<usize>, StyleRefinement)> {
    my_syntect::ranges(code.language.as_deref(), &code.code)
        .map(|(range, color, bold)| {
            let style = StyleRefinement::default().text_color(color);
            (range, if bold { style.font_weight(FontWeight::BOLD) } else { style })
        })
        .collect()
}

#[component]
pub fn Chat() -> impl IntoElement {
    provide_context(CodeHighlighter::new(highlight));
    view().child(Messages::element(()))
}
```

`my_syntect::ranges` stands for the app's glue to its highlighter. `renderCodeBlock`, when given, shows the code block
in place of all of this.

### Typed part styles

`TextViewParts` styles parts with typed styles instead of class strings, for Rust views: each `TextViewPart` (one per
part of the [table above](#styling-the-parts)) gets a function refining its `StyleRefinement`. Given with
`provide_context`, they apply to every `TextView` below, before its `classes` and `styles`; theme colors read in them
follow the light and dark appearance.

```rust
use nsh_base::{TextViewPart, TextViewParts, theme};
use nsh_core::ui::prelude::*;

provide_context(
    TextViewParts::new()
        .part(TextViewPart::Heading1, |s| s.text_2xl().font_weight(FontWeight::BOLD))
        .part(TextViewPart::Code, |s| s.bg(theme().muted()).rounded(px(4.0)))
        .part(TextViewPart::Link, |s| s.text_color(theme().primary())),
);
```

### Markdown plugins

A `MarkdownPlugin` reads Markdown syntax in Rust: it gets each node of the syntax tree (markdown-rs's `mdast`) before
the parser reads it, and returns a block or inline content of its own, usually a `CustomNode`; it can also show the
custom nodes it makes. A view provides plugins to every `TextView` below it:

```rust
use nsh_base::text_view::{mdast, CustomNode, Inline, MarkdownPlugin, MarkdownPlugins, ParseContext};
use nsh_core::ui::prelude::*;

struct Tickers;

impl MarkdownPlugin for Tickers {
    fn name(&self) -> &str {
        "tickers"
    }

    /// `$AAPL` in inline code becomes a ticker node.
    fn inline(&self, node: &mdast::Node, _cx: &mut ParseContext<'_>) -> Option<Inline> {
        let mdast::Node::InlineCode(code) = node else { return None };
        let symbol = code.value.strip_prefix('$')?;
        Some(Inline::Custom(CustomNode::new("ticker").with_text(symbol)))
    }

    fn render(&self, node: &CustomNode) -> Option<AnyElement> {
        (node.name == "ticker").then(|| text(node.text.clone()).font_weight(FontWeight::BOLD).into_element())
    }
}

#[component]
pub fn Chat() -> impl IntoElement {
    provide_context(MarkdownPlugins::new().with(Tickers));
    view().child(Messages::element(()))
}
```

A plugin's `render` comes first, then `renderCustom`, then the node's content. `configure` turns on more of
markdown-rs's syntax, and `ParseContext` reads a node's children as the parser does.

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `markdown` | `string` | | The document as Markdown. |
| `html` | `string` | | The document as HTML, when `markdown` is not given. |
| `document` | `TextViewDocument` | | The document as blocks, when neither `markdown` nor `html` is given. |
| `options` | `MarkdownOptions` | all defaults | The Markdown syntax read ([Options](#options)). |
| `extensions` | `MarkdownExtension[]` | `[]` | The app's syntax, as custom nodes ([Markdown extensions](#markdown-extensions)). |
| `classes` | `TextViewClasses` | `{}` | Tailwind classes by part. |
| `styles` | `TextViewStyles` | `{}` | Inline styles by part (each a `StyleProp<TextStyle>`), over the classes. |
| `selectable` | `boolean` | `false` | The document is selectable as one: a selection runs across its paragraphs, headings, cells and code blocks ([SelectionArea](../modules/nsh-base.md#selectionarea)). |
| `codeBlockScrollIndicator` | `boolean` | `true` | Code blocks show their horizontal scroll indicator. |
| `tableScrollIndicator` | `boolean` | `true` | Tables show their horizontal scroll indicator. |
| `caret` | `JSX.Element` | | Shown at the end of the document: inline after the last text, else on its own line. |
| `renderCustom` | `(node: TextViewCustomNode) => JSX.Element` | | Shows custom nodes. |
| `renderCodeBlock` | `(code: TextViewCodeBlock) => JSX.Element` | | Shows code blocks. |
| `codeBlockActions` | `TextViewCodeAction[]` | `[]` | The actions over each code block's top right corner: `{ id: "copy" }` built in, others the app's. |
| `renderCodeBlockAction` | `(action: TextViewCodeAction) => JSX.Element` | | The icon of an action of the app's. |
| `renderTableActions` | `(table: TextViewTable) => JSX.Element` | | Content above each table. |
| `renderMath` | `(math: TextViewMath) => JSX.Element` | | Shows formulas. |
| `renderImage` | `(image: TextViewImage) => JSX.Element` | | Shows images. |
| `onLinkPress` | `(url: string) => void` | | A link was pressed: its URL, `#anchor` for a heading, `#footnote-<label>` for a footnote. |
| `onCodeBlockAction` | `(event: { id: string; code: TextViewCodeBlock }) => void` | | A code block's action was pressed (after `copy` copied). |
| `onImagePress` | `(image: TextViewImage) => void` | | An image not in a link was pressed: the image, for the app to show it larger (a zoomable viewer of its own). |
| `class`, `style` | `string`, `StyleProp<ViewStyle>` | | The root, a column of the blocks; its text styles pass to every part (typed text styles: `styles.root`). |

## Accessibility

Screen readers read the document in order, text by text. `TextView` gives the structure its roles: headings are
headers, links are links (their title is the hint), task markers are checkboxes with their checked state, a
disclosure's summary is a button that says whether it is expanded, and images are read by their alt text.

## Related

- [Text](./text.md): the element every text of `TextView` is, with its spans, links, inline attachments and selection.
- [SelectionArea](../modules/nsh-base.md#selectionarea): one selection across several `TextView`s and texts.
- [`nsh/base`](../modules/nsh-base.md): the other base components.
- [Styling](../docs/styling.md) and [Tailwind](../docs/tailwind.md).
