//! The `TextView` component: a [`Document`] shown with the primitives.

use std::rc::Rc;

use nsh_core::{
    ResizeMode,
    color::Hsla,
    element::{Renderer, Style, StyleRefinement},
    geometry::{DefiniteLength, Length},
    prelude::*,
};

use super::{
    Alert, Block, CodeBlock, ColumnAlign, CustomNode, Document, Image, ImageLength, Inline,
    ListItem, MarkdownExtension, MarkdownOptions, MarkdownPlugins, Parser, Table,
    parts::{TextViewClasses, TextViewPart as Part, TextViewParts, TextViewStyles},
    selection::{Reported, Source, TextSelectionOptions, selected},
};
use crate::{terminal::is_terminal, theme::theme};

/// The props of [`TextView`].
#[derive(Debug, Clone, Copy, PartialEq, Props)]
pub struct TextViewProps {
    /// The document as Markdown (CommonMark and GitHub's: tables, task
    /// lists, footnotes, alerts; formulas; HTML inside). A reply streamed
    /// by an AI model can be passed as it grows (with `options={{
    /// streaming: true }}`): only the blocks that changed are shown again.
    pub markdown: Prop<Option<SharedString>>,
    /// The document as HTML (simple markup as a reader shows it: no CSS or
    /// scripts), when `markdown` is not given.
    pub html: Prop<Option<SharedString>>,
    /// The document as blocks the app made, when neither `markdown` nor
    /// `html` is given.
    pub document: Prop<Option<Document>>,
    /// The Markdown syntax read beyond CommonMark and GitHub's.
    #[prop(default)]
    pub options: Prop<MarkdownOptions>,
    /// Syntax of the app's that becomes custom nodes, shown by
    /// `renderCustom` (mentions, charts, callouts, ...).
    #[prop(default)]
    pub extensions: Prop<Vec<MarkdownExtension>>,
    /// Tailwind classes by part (`{ heading1: "text-2xl font-bold", code:
    /// "font-mono bg-muted" }`), after the part's own styles.
    #[prop(default)]
    pub classes: Prop<TextViewClasses>,
    /// Inline styles by part, over their classes.
    #[prop(default)]
    pub styles: Prop<TextViewStyles>,
    /// Its texts can be selected and copied (a long press, or a drag with
    /// a pointer), each block on its own.
    #[prop(default)]
    pub selectable: Prop<bool>,
    /// What a press on a link does: `"report"` (the default) reports it
    /// (`onLinkPress`) and opens nothing; `"open"` also opens a link to
    /// another app or site outside the app (`openUrl` of `nsh/device`,
    /// under the app's `openUrl` capability; the embedding app may route
    /// it), while anchors in the document (`#…`) are only reported.
    #[prop(default)]
    pub links: Prop<TextViewLinks>,
    /// Whether code blocks show the scroll indicator of their horizontal
    /// scroll view (default true).
    #[prop(default = true)]
    pub code_block_scrollbar: Prop<bool>,
    /// Whether tables show the scroll indicator of their horizontal scroll
    /// view (default true).
    #[prop(default = true)]
    pub table_scrollbar: Prop<bool>,
    /// Shown inline at the end of the last block (a streaming reply's
    /// caret).
    pub streaming_caret: Prop<Option<Slot>>,
    /// Shows a custom node (an extension's or the document's): `node.name`
    /// tells which. Without it, a custom node shows its content.
    pub render_custom: Prop<Option<Renderer<CustomNode>>>,
    /// Shows a code block in place of the default (the code in a
    /// horizontal scroll view): syntax highlighting of the app's.
    pub render_code_block: Prop<Option<Renderer<CodeBlock>>>,
    /// Content of the app's floating over each code block's top right
    /// The icon of a code block's action of the app's (`codeBlockActions`):
    /// the button around it (its size, hover and press backgrounds) is the
    /// text view's, so every action looks alike. Without it, an action
    /// shows its label.
    pub render_code_block_action: Prop<Option<Renderer<TextViewCodeAction>>>,
    /// The buttons over each code block's top right corner, in order:
    /// `{ id: "copy" }` is built in (it copies the code and shows a check
    /// for a moment), other ids are the app's (their icon from
    /// `renderCodeBlockAction`, a press reported by `onCodeBlockAction`).
    /// All are ghost buttons that show while a pointer hovers the block on
    /// desktops, always on touch screens.
    #[prop(default)]
    pub code_block_actions: Prop<Vec<TextViewCodeAction>>,
    /// Content of the app's above each table (a copy or export button).
    pub render_table_actions: Prop<Option<Renderer<Table>>>,
    /// Shows a formula (`$…$` in text, `$$…$$` blocks); without it, its
    /// TeX shows as code.
    pub render_math: Prop<Option<Renderer<TextViewMath>>>,
    /// Shows an image in place of the default: alone in a paragraph, the
    /// image at the size it asks for (`width` / `height`) or its natural
    /// size, at most as wide as the text; in a line of text, the size it
    /// asks for, else three quarters of a line tall. A press on an image in
    /// a link reports the link (`onLinkPress`), on another the image
    /// (`onImagePress`).
    pub render_image: Prop<Option<Renderer<Image>>>,
}

/// A formula of a [`TextView`]'s document, for its `renderMath`.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct TextViewMath {
    /// The formula, in TeX.
    pub tex: SharedString,
    /// A block of its own (`$$…$$`), not inside text.
    pub display: bool,
}

/// What a press on a link of a [`TextView`] does (`links`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum TextViewLinks {
    /// It is reported (`onLinkPress`); the app decides what opens.
    #[default]
    Report,
    /// It is reported, then a URL of another app or site (with a scheme:
    /// `https:`, `mailto:`, ...) opens outside the app
    /// ([`nsh_core::element::open_url`]); an anchor in the document (`#…`) is
    /// only reported.
    Open,
}

/// The events of a [`TextView`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum TextViewEvent {
    /// A link was pressed: its URL (`#anchor` for a heading of the
    /// document, `#footnote-<label>` for a footnote). Nothing opens it
    /// unless `links` is `"open"`: the app decides.
    #[args(event)]
    LinkPress(TextViewLinkPressEvent),
    /// An image that is not in a link was pressed: the image (its `src`,
    /// `alt`, `title`, size), for the app to show it larger (a zoomable
    /// viewer of its own).
    #[args(image)]
    ImagePress(Image),
    /// An action of a code block was pressed (after the built-in `copy`
    /// copied): its id and the code block.
    #[args(event)]
    CodeBlockAction(TextViewCodeActionEvent),
    /// The selection of a `selectable` text view changed: the selected
    /// text, as a copy gives it, empty once nothing is selected (to quote
    /// a part of a reply). Inside another `selectable` view the outer
    /// view's selection is the one there is, and it reports it.
    #[args(event)]
    SelectionChange(TextViewSelectionEvent),
}

/// The methods of a [`TextView`]'s ref.
pub struct TextViewApi {
    document: Memo<Document>,
    props: TextViewProps,
    reported: Rc<std::cell::RefCell<Reported>>,
}

#[methods]
impl TextViewApi {
    /// The selected part of a `selectable` view, in `options.format`: its
    /// text as a copy gives it (`plain`, the default), or the selected
    /// nodes as Markdown or HTML (the source as written when all of it is
    /// selected). Empty when nothing is selected; inside another
    /// `selectable` view, the outer view holds the selection and this
    /// view's is empty.
    pub fn selected_text(&self, options: Option<TextSelectionOptions>) -> String {
        let format = options.unwrap_or_default().format;
        let reported = self.reported.borrow();
        if reported.text.is_empty() {
            return String::new();
        }
        let source = if let Some(md) = self.props.markdown.get_untracked() {
            Source::Markdown(md.to_string())
        } else if let Some(html) = self.props.html.get_untracked() {
            Source::Html(html.to_string())
        } else {
            Source::None
        };
        self.document
            .with_untracked(|doc| selected(doc, &source, &reported, format))
    }
}

/// The selection of a [`TextView`] changed (`onSelectionChange`).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub struct TextViewSelectionEvent {
    /// The selected text, as a copy gives it (blocks joined by line
    /// breaks); empty when nothing is selected.
    pub text: SharedString,
    /// Where it starts in the view's whole text (UTF-16 units), when the
    /// platform knows it.
    pub start: Option<u32>,
}

/// An action of the code blocks of a [`TextView`] (`codeBlockActions`).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct TextViewCodeAction {
    /// Which it is: `"copy"` (built in: copies the code, then shows a check
    /// for a moment), or an id of the app's.
    pub id: SharedString,
    /// What screen readers say for it (default: its id).
    pub label: Option<SharedString>,
}

impl TextViewCodeAction {
    /// Action `id`.
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: None,
        }
    }

    /// With label `label` for screen readers.
    pub fn with_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// A press on a link of a [`TextView`] (`onLinkPress`).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub struct TextViewLinkPressEvent {
    /// The link's URL (`#anchor` for a heading of the document,
    /// `#footnote-<label>` for a footnote).
    pub url: SharedString,
}

/// A press on a code block's action (`onCodeBlockAction`).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct TextViewCodeActionEvent {
    /// The action's id.
    pub id: SharedString,
    /// The code block.
    pub code: CodeBlock,
}

/// A document of Markdown, HTML or the app's own blocks (`markdown`,
/// `html` or `document`), shown with texts, views and images: headings,
/// paragraphs, emphasis, links, lists and task lists, quotes and alerts,
/// code, tables, images, formulas, footnotes and disclosures, and the
/// custom nodes of the app's extensions.
///
/// It has a reading typography of its own (15pt text in 25.5pt lines,
/// semibold headings with room above them, tinted alerts, code in bordered
/// boxes, ruled tables), colored with the theme's tokens, so a theme or the
/// dark appearance reaches it natively. The app restyles each part with
/// `classes` or `styles` (or a Rust view's [`TextViewParts`]), its root
/// with `class` and `style` (text styles there pass to every part): they
/// apply over these defaults. Links report presses
/// (`onLinkPress`). Content of the app's goes in with render functions:
/// custom nodes (`renderCustom`), code blocks and their actions, tables'
/// actions, formulas, images. Made for replies streamed by AI models:
/// blocks are kept by position, so only those that changed are made
/// again, and `streaming_caret` follows the end of the text.
#[component(methods = TextViewApi)]
pub fn TextView(props: TextViewProps, cx: Scope<TextViewEvent>) -> impl IntoElement {
    let plugins = use_context::<MarkdownPlugins>().unwrap_or_default();
    let parser_plugins = plugins.clone();
    let document = computed(move || {
        let parser = || {
            Parser::new()
                .options(props.options.get())
                .extensions(props.extensions.get())
                .plugins(&parser_plugins)
        };
        if let Some(markdown) = props.markdown.get() {
            parser().markdown(&markdown)
        } else if let Some(html) = props.html.get() {
            parser().html(&html)
        } else {
            props.document.get().unwrap_or_default()
        }
    });
    let blocks = computed(move || {
        document.with(|d| d.blocks.iter().cloned().enumerate().collect::<Vec<_>>())
    });
    let count = computed(move || blocks.with(Vec::len));
    let footnotes = computed(move || document.with(|d| d.footnotes.clone()));
    let plugins = Rc::new(plugins);
    let row_plugins = plugins.clone();
    let math_loaded = signal(false);
    let reported = Rc::new(std::cell::RefCell::new(Reported::default()));
    cx.methods(TextViewApi {
        document,
        props,
        reported: reported.clone(),
    });
    view()
        .flex_col()
        .gap(px(BLOCK_GAP))
        .text_size(px(BODY_SIZE))
        .line_height(px(BODY_SIZE * BODY_LINE))
        .text_color(move || theme().foreground)
        // Selectable: its texts share one selection, block to block (inside
        // a `selectable` view, the outer one's).
        .selectable(props.selectable)
        .on_text_selection_change(move |e: &nsh_core::element::TextSelectionEvent| {
            *reported.borrow_mut() = Reported {
                text: e.text.clone(),
                start: e.start,
            };
            cx.emit(TextViewEvent::SelectionChange(TextViewSelectionEvent {
                text: e.text.clone().into(),
                start: e.start,
            }));
        })
        .map(|mut el| {
            if let Some(parts) = use_context::<TextViewParts>() {
                let style = std::mem::take(el.style());
                *el.style() = parts.refine(Part::Root, style);
            }
            el
        })
        .app_class(move || props.classes.with(|c| c.root.clone()))
        .inline_style(move || {
            props
                .styles
                .with(|s| s.root.as_ref().map(|s| s.0.clone()).unwrap_or_default())
        })
        .child(each(
            blocks,
            // By position and kind: the block a stream is writing keeps its
            // row, and its row alone is made again.
            |(index, block): &(usize, Block)| (*index, block.kind().to_owned()),
            move |item, index| {
                let plugins = row_plugins.clone();
                // A memo: a block added after this one runs it again only
                // when this one stops being the last.
                let last = computed(move || index.get() + 1 == count.get());
                // A memo as well: what comes before decides a heading's
                // room above it.
                let before = computed(move || {
                    let i = index.get();
                    blocks.with(|b| {
                        Before::of(shown_before(b[..i.min(b.len())].iter().map(|(_, b)| b)))
                    })
                });
                // A node as the row's root: an `Each` keeps rows by their
                // root's key, which a region has none of.
                view().child(dynamic(move || {
                    let cx = Cx::read(&props, cx, plugins.clone(), math_loaded);
                    let (i, block) = item.get();
                    cx.block(&block, &i.to_string(), last.get(), before.get())
                }))
            },
        ))
        .child(dynamic(move || {
            let notes = footnotes.get();
            if notes.is_empty() {
                return None;
            }
            let cx = Cx::read(&props, cx, plugins.clone(), math_loaded);
            // A terminal's blank row before them, as before a block.
            let footnotes = view()
                .flex_col()
                .gap(px(4.0))
                .text_size(px(SMALL_SIZE))
                .line_height(px(SMALL_SIZE * 1.6))
                .text_color(move || theme().muted_foreground);
            let gap = is_terminal().then(|| view().h(crate::terminal::cell_height()));
            Some(
                cx.part(footnotes.child(gap), Part::Footnotes)
                    .children(notes.iter().map(|note| {
                        let key = format!("f{}", note.index);
                        cx.part(view().flex_row().gap(px(6.0)), Part::Footnote)
                            .child(cx.part(text(format!("{}.", note.index)), Part::FootnoteLabel))
                            .child(cx.with_line(SMALL_SIZE * 1.6, || {
                                cx.blocks(
                                    view().flex_col().flex_1().gap(px(4.0)),
                                    &note.children,
                                    &key,
                                    false,
                                )
                            }))
                    })),
            )
        }))
}

/// Syntax highlighting of code blocks, for Rust views: given to the text
/// views below with `provide_context`, in place of the built-in one
/// (feature `highlight`, on by default: syntect, in Allsum's reading palette). The
/// app's (`tree-sitter`, its own theme, ...) returns the styles of byte
/// ranges of the code; the text view shows the code with them. A TS app
/// shows its own highlighted code with `renderCodeBlock`.
///
/// ```ignore
/// provide_context(CodeHighlighter::new(|code: &CodeBlock| my_highlighter(code)));
/// ```
#[derive(Clone)]
pub struct CodeHighlighter(Rc<HighlightFn>);

type HighlightFn = dyn Fn(&CodeBlock) -> Vec<(std::ops::Range<usize>, StyleRefinement)>;

impl CodeHighlighter {
    /// A highlighter returning the styles of byte ranges of a block's
    /// code (in order, not overlapping; what they leave out is plain).
    pub fn new(
        highlight: impl Fn(&CodeBlock) -> Vec<(std::ops::Range<usize>, StyleRefinement)> + 'static,
    ) -> Self {
        Self(Rc::new(highlight))
    }

    /// `code` in spans styled as the highlighter says.
    fn spans(&self, code: &CodeBlock) -> Vec<AnyElement> {
        let source = code.code.as_str();
        let mut out = Vec::new();
        let mut at = 0;
        for (range, style) in (self.0)(code) {
            // Ranges out of order, overlapping or off a character boundary
            // are skipped rather than trusted.
            if range.start < at
                || range.end <= range.start
                || source.get(range.clone()).is_none()
                || !source.is_char_boundary(range.start)
            {
                continue;
            }
            if range.start > at {
                out.push(text(source[at..range.start].to_owned()).into_element());
            }
            let mut span = text(source[range.clone()].to_owned());
            *span.style() = style;
            out.push(span.into_element());
            at = range.end;
        }
        if at < source.len() {
            out.push(text(source[at..].to_owned()).into_element());
        }
        out
    }
}

impl std::fmt::Debug for CodeHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CodeHighlighter")
    }
}

/// What the blocks of one run are made with: the props read.
#[derive(Clone)]
struct Cx {
    classes: TextViewClasses,
    styles: TextViewStyles,
    selectable: bool,
    code_scroll_indicator: bool,
    table_scrollbar: bool,
    caret: Option<Slot>,
    render_custom: Option<Renderer<CustomNode>>,
    render_code_block: Option<Renderer<CodeBlock>>,
    render_code_block_action: Option<Renderer<TextViewCodeAction>>,
    code_block_actions: Vec<TextViewCodeAction>,
    render_table_actions: Option<Renderer<Table>>,
    render_math: Option<Renderer<TextViewMath>>,
    render_image: Option<Renderer<Image>>,
    plugins: Rc<MarkdownPlugins>,
    /// Set when the formula typesetter finished loading (`formula.rs`).
    math_loaded: Signal<bool>,
    typed: TextViewParts,
    /// Whether the caret is placed in this run.
    caret_placed: Rc<std::cell::Cell<bool>>,
    /// How many lists the blocks being made are in (a copy keeps the depth
    /// it was made at, for content made later).
    list_depth: std::cell::Cell<u8>,
    /// The line height of the text being made, which images in it are
    /// sized by.
    line: std::cell::Cell<f32>,
    /// The URL of the link the content being made is in: a press on an
    /// image there is the link's.
    link: std::cell::RefCell<Option<SharedString>>,
    highlighter: Option<CodeHighlighter>,
    links: TextViewLinks,
    scope: Scope<TextViewEvent>,
}

impl Cx {
    fn read(
        props: &TextViewProps,
        scope: Scope<TextViewEvent>,
        plugins: Rc<MarkdownPlugins>,
        math_loaded: Signal<bool>,
    ) -> Self {
        Self {
            classes: props.classes.get(),
            styles: props.styles.get(),
            selectable: props.selectable.get(),
            code_scroll_indicator: props.code_block_scrollbar.get(),
            table_scrollbar: props.table_scrollbar.get(),
            caret: props.streaming_caret.get(),
            render_custom: props.render_custom.get(),
            render_code_block: props.render_code_block.get(),
            render_code_block_action: props.render_code_block_action.get(),
            code_block_actions: props.code_block_actions.get(),
            render_table_actions: props.render_table_actions.get(),
            render_math: props.render_math.get(),
            render_image: props.render_image.get(),
            plugins,
            math_loaded,
            typed: use_context::<TextViewParts>().unwrap_or_default(),
            highlighter: use_context::<CodeHighlighter>(),
            caret_placed: Rc::default(),
            list_depth: std::cell::Cell::new(0),
            line: std::cell::Cell::new(BODY_SIZE * BODY_LINE),
            link: std::cell::RefCell::new(None),
            links: props.links.get(),
            scope,
        }
    }

    /// `tex` typeset, when it can be now.
    fn formula(&self, tex: &str, display: bool, size: f32) -> Option<AnyElement> {
        if !super::formula::ready(self.math_loaded) {
            return None;
        }
        super::formula::formula(tex, display, size)
    }

    /// What a press on a link to `url` does: reported, and opened outside
    /// the app when `links` says so.
    fn link_press(&self, url: SharedString) -> impl Fn() + Clone + 'static {
        let (scope, links) = (self.scope, self.links);
        move || {
            scope.emit(TextViewEvent::LinkPress(TextViewLinkPressEvent {
                url: url.clone(),
            }));
            if links == TextViewLinks::Open && !url.starts_with('#') {
                // Whether it opened is the platform's to show; a refused
                // URL or capability is logged.
                drop(open_url(url.to_string()));
            }
        }
    }

    /// `el` with part `part`'s classes and style.
    fn part<E: Element>(&self, el: E, part: Part) -> E {
        self.parts(el, &[part])
    }

    /// `el` with the classes and styles of `parts`, in order.
    fn parts<E: Element>(&self, mut el: E, parts: &[Part]) -> E {
        if !self.typed.is_empty() {
            for part in parts {
                let style = std::mem::take(el.style());
                *el.style() = self.typed.refine(*part, style);
            }
        }
        let mut classes = String::new();
        for part in parts {
            if let Some(c) = self.classes.get(*part) {
                if !classes.is_empty() {
                    classes.push(' ');
                }
                classes.push_str(c);
            }
        }
        let el = if classes.is_empty() {
            el
        } else {
            el.app_class(Some(SharedString::from(classes)))
        };
        let styles: Vec<&Style> = parts.iter().filter_map(|p| self.styles.get(*p)).collect();
        match styles.as_slice() {
            [] => el,
            [style] => el.inline_style((*style).clone()),
            // Later parts' declarations win, as later classes do.
            _ => el.inline_style(Style::from_decls(
                styles.iter().flat_map(|s| s.decls().iter().cloned()),
            )),
        }
    }

    /// A text of the block's inline content, selectable when asked.
    fn paragraph(&self, content: &[Inline], key: &str, tail: bool) -> Text {
        let el = text("")
            .when(self.selectable, |t| t.select_text())
            .children(self.inlines(content, key));
        self.caret_after(el, tail)
    }

    /// The caret at the end of `el` when it is the last text.
    fn caret_after<E: ParentElement>(&self, el: E, tail: bool) -> E {
        if tail && !self.caret_placed.replace(true) {
            el.slot(self.caret, |s| s)
        } else {
            el
        }
    }

    /// `blocks` in `column`, each keyed under `key`; the last one is
    /// `tail`'s.
    fn blocks<E: ParentElement + Styled>(
        &self,
        column: E,
        blocks: &[Block],
        key: &str,
        tail: bool,
    ) -> E {
        // A terminal draws nothing for a rule (the blank row between the
        // blocks around it is the break) nor for a paragraph of images
        // without alt text: they take no row, and no blank row either.
        let terminal = is_terminal();
        let shown: Vec<(usize, &Block)> = blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| !terminal || !blank_in_terminal(b))
            .collect();
        let n = shown.len();
        column.children(shown.iter().enumerate().map(|(at, &(i, b))| {
            let before = Before::of(shown_before(shown[..at].iter().map(|(_, b)| *b)));
            self.block(b, &format!("{key}.{i}"), tail && at + 1 == n, before)
        }))
    }

    fn block(&self, block: &Block, key: &str, tail: bool, before: Before) -> AnyElement {
        if is_terminal() && blank_in_terminal(block) {
            return view().into_element();
        }
        let el = self.block_content(block, key, tail, before);
        // A terminal's blocks carry no margins: a block after another, a
        // heading's included, takes one blank row before it, as a Markdown
        // source has. Not in a list (its items' blocks stay together).
        let after_block = !matches!(before, Before::Nothing);
        let el = if is_terminal() && self.list_depth.get() == 0 && after_block {
            view()
                .flex_col()
                .child(view().h(crate::terminal::cell_height()))
                .child(el)
                .into_element()
        } else {
            el
        };
        // A block with no text of its own at its end (a rule, the app's
        // content): the caret goes on a line after it.
        if tail && self.caret.is_some() && !self.caret_placed.get() {
            view()
                .flex_col()
                .child(el)
                .child(self.caret_after(text(""), true))
                .into_element()
        } else {
            el
        }
    }

    fn block_content(&self, block: &Block, key: &str, tail: bool, before: Before) -> AnyElement {
        match block {
            Block::Paragraph { content } => match only_images(content) {
                // Images with no text around them are blocks: as wide as
                // they ask, at most the text's width.
                Some(images) => self
                    .part(self.image_column(&images, key), Part::Paragraph)
                    .into_element(),
                None => self
                    .part(self.paragraph(content, key, tail), Part::Paragraph)
                    .into_element(),
            },
            Block::Heading { level, content, .. } => {
                let part = match level {
                    1 => Part::Heading1,
                    2 => Part::Heading2,
                    3 => Part::Heading3,
                    4 => Part::Heading4,
                    5 => Part::Heading5,
                    _ => Part::Heading6,
                };
                if is_terminal() {
                    return self
                        .parts(
                            self.terminal_heading(*level, content, tail),
                            &[Part::Heading, part],
                        )
                        .into_element();
                }
                let h = Heading::of(*level);
                let el = self
                    .with_line(h.size * h.line, || self.paragraph(content, key, tail))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(h.size))
                    .line_height(px(h.size * h.line))
                    .letter_spacing(px(h.size * h.tracking))
                    // A terminal's blank row before it is the one every
                    // block after another takes (`block`).
                    .mt(px(h.margin_top(before)))
                    .role(AccessibilityRole::Header);
                self.parts(el, &[Part::Heading, part]).into_element()
            }
            Block::Blockquote {
                children,
                alert: None,
            } => {
                let column = view().flex_col().gap(px(BLOCK_GAP));
                // A terminal's quote is a bar of `▎` in the accent down its
                // left side, a Cell before its text, which is muted, as a
                // Markdown source's `>`: a one-sided border has no Cell of
                // its own there.
                let quote = match is_terminal() {
                    true => crate::terminal::quoted(self.blocks(
                        column.text_color(move || theme().muted_foreground),
                        children,
                        key,
                        tail,
                    )),
                    false => self.blocks(
                        column
                            .text_color(move || theme().foreground.opacity(0.8))
                            .border_l(px(2.0))
                            .border_color(strong_border)
                            .ps(px(BODY_SIZE * 0.9)),
                        children,
                        key,
                        tail,
                    ),
                };
                self.part(quote, Part::Blockquote).into_element()
            }
            Block::Blockquote {
                children,
                alert: Some(alert),
            } => self.alert(*alert, children, key, tail),
            Block::List {
                ordered,
                start,
                loose,
                items,
            } => self.list(*ordered, *start, *loose, items, key, tail),
            Block::CodeBlock(code) => self.code_block(code, key, tail),
            Block::Math { tex } => match &self.render_math {
                Some(render) => render
                    .render(
                        format!("math:{key}"),
                        TextViewMath {
                            tex: tex.clone().into(),
                            display: true,
                        },
                    )
                    .into_element(),
                // A terminal's as text in a band as a code block's.
                None if is_terminal() => self
                    .part(
                        view().flex_col().items_start().child(
                            terminal_band(
                                text(super::tex_text::tex_text(tex))
                                    .font_family(mono_font)
                                    .when(self.selectable, |t| t.select_text()),
                            )
                            .bg(move || theme().muted),
                        ),
                        Part::MathBlock,
                    )
                    .into_element(),
                // Typeset, centered, scrolling sideways when wider than
                // the view.
                None if let Some(formula) = self.formula(tex, true, BODY_SIZE * 1.1) => self
                    .part(
                        scroll().horizontal().scrollbar(false).child(
                            view()
                                .flex_row()
                                .justify_center()
                                .min_w(relative(1.0))
                                .py(px(BODY_SIZE * 0.25))
                                .label(tex.clone())
                                .child(formula),
                        ),
                        Part::MathBlock,
                    )
                    .into_element(),
                // TeX it cannot read: in a box as a code block's, as gpui-kit
                // shows a formula it has no renderer for.
                None => self
                    .part(
                        view()
                            .py(px(14.0))
                            .px(px(16.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(move || theme().border)
                            .bg(move || theme().muted)
                            .child(
                                text(tex.clone())
                                    .font_family(mono_font)
                                    .text_size(px(CODE_SIZE))
                                    .line_height(px(CODE_SIZE * CODE_LINE))
                                    .when(self.selectable, |t| t.select_text()),
                            ),
                        Part::MathBlock,
                    )
                    .into_element(),
            },
            Block::Table(table) => self.table(table, key, tail),
            Block::Rule => self
                .part(
                    view()
                        .border_t_1()
                        .border_color(strong_border)
                        // 1.6em in all with the gap.
                        .my(px(BODY_SIZE * 1.6 - BLOCK_GAP)),
                    Part::Rule,
                )
                .into_element(),
            Block::Details {
                open,
                summary,
                children,
            } => self.details(*open, summary, children, key, tail),
            Block::Custom(node) => self.custom_block(node, key, tail),
        }
    }

    /// A heading in a terminal, which has one text size: its text in its
    /// level's weight and color, over any inline styles in it, so the line
    /// reads as one heading (h1 and h2 bold in the accent, h3 bold, the
    /// others bold and muted); h1 has a rule in the border color across
    /// the view under it.
    fn terminal_heading(&self, level: u8, content: &[Inline], tail: bool) -> View {
        let mut line = String::new();
        super::document::inline_text(content, &mut line);
        let color: AdaptiveColor = match level {
            1 | 2 => crate::terminal::ACCENT.into(),
            3 => theme().foreground,
            _ => theme().muted_foreground,
        };
        let heading = self.caret_after(
            text(line)
                .font_weight(FontWeight::BOLD)
                .text_color(color)
                .when(self.selectable, |t| t.select_text())
                .role(AccessibilityRole::Header),
            tail,
        );
        view()
            .flex_col()
            .child(heading)
            .child((level == 1).then(crate::terminal::rule_line))
    }

    fn list(
        &self,
        ordered: bool,
        start: u32,
        loose: bool,
        items: &[ListItem],
        key: &str,
        tail: bool,
    ) -> AnyElement {
        let n = items.len();
        let kind = if ordered {
            Part::OrderedList
        } else {
            Part::UnorderedList
        };
        let depth = self.list_depth.get();
        let terminal = is_terminal();
        // A terminal's numbers right-aligned to the widest, so the items'
        // content stays in line.
        let digits = (start as usize + n.saturating_sub(1)).to_string().len();
        let rows = items.iter().enumerate().map(|(i, item)| {
            let key = format!("{key}.{i}");
            let marker = match item.checked {
                // A ballot box and a space before the content.
                Some(checked) if terminal => {
                    let mut parts = vec![Part::ListMarker, Part::TaskMarker];
                    if checked {
                        parts.push(Part::TaskMarkerChecked);
                    }
                    // A ballot box, as a Markdown reader in a terminal
                    // shows `[ ]` and `[x]`: checked in the accent.
                    let marker = text(if checked { "☑ " } else { "☐ " })
                        .flex_none()
                        .text_color(move || match checked {
                            true => AdaptiveColor::from(crate::terminal::ACCENT),
                            false => theme().muted_foreground,
                        })
                        .role(AccessibilityRole::Checkbox)
                        .accessibility_state(AccessibilityState {
                            checked: Some(if checked {
                                nsh_core::Checked::True
                            } else {
                                nsh_core::Checked::False
                            }),
                            ..AccessibilityState::default()
                        });
                    self.parts(marker, &parts).into_element()
                }
                None if terminal => {
                    let marker = if ordered {
                        format!("{:>digits$}. ", start as usize + i)
                    } else {
                        format!("{} ", bullet(depth))
                    };
                    let marker = text(marker)
                        .flex_none()
                        .text_color(move || theme().muted_foreground);
                    self.part(marker, Part::ListMarker).into_element()
                }
                Some(checked) => {
                    let mut parts = vec![Part::ListMarker, Part::TaskMarker];
                    if checked {
                        parts.push(Part::TaskMarkerChecked);
                    }
                    // A box centered on the first line.
                    let marker = view()
                        .flex_none()
                        .size(px(TASK_BOX))
                        .mt(px((BODY_SIZE * BODY_LINE - TASK_BOX) / 2.0))
                        .rounded(px(4.0))
                        .border(px(1.5))
                        .items_center()
                        .justify_center()
                        .role(AccessibilityRole::Checkbox)
                        .accessibility_state(AccessibilityState {
                            checked: Some(if checked {
                                nsh_core::Checked::True
                            } else {
                                nsh_core::Checked::False
                            }),
                            ..AccessibilityState::default()
                        });
                    let marker = if checked {
                        marker
                            .bg(move || theme().primary)
                            .border_color(move || theme().primary)
                            .child(
                                text("✓")
                                    .text_size(px(11.0))
                                    .line_height(px(13.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(move || theme().primary_foreground),
                            )
                    } else {
                        marker.border_color(move || theme().muted_foreground)
                    };
                    self.parts(marker, &parts).into_element()
                }
                None => {
                    let marker = if ordered {
                        format!("{}.", start as usize + i)
                    } else {
                        bullet(depth).to_owned()
                    };
                    // In the indent before the content, its end against
                    // it, as a browser places it: wider numbers reach
                    // further left and the content stays in line.
                    let marker = text(marker)
                        .absolute()
                        .top(px(0.0))
                        .right(relative(1.0))
                        .w(px(MARKER_BOX))
                        .pe(px(6.0))
                        .text_right()
                        .text_color(move || theme().muted_foreground);
                    self.part(marker, Part::ListMarker).into_element()
                }
            };
            let task = item.checked.is_some();
            let done = item.checked == Some(true);
            let content = view()
                .flex_col()
                .flex_1()
                .gap(px(if loose { BLOCK_GAP } else { ITEM_GAP }))
                .when(done, |v| {
                    v.text_color(move || theme().muted_foreground)
                        .line_through()
                        .text_decoration_color(move || theme().muted_foreground.opacity(0.6))
                });
            let row = view().flex_row().map(|v| {
                if terminal {
                    v
                } else if task {
                    v.gap(px(8.0)).ps(px(1.5))
                } else {
                    v.ps(px(LIST_INDENT))
                }
            });
            let content = self.blocks(content, &item.children, &key, tail && i + 1 == n);
            // Before the content when a box, inside it (placed over the
            // indent) when a bullet or a number.
            // In a terminal the marker is text before the content, always.
            let (before, inside) = if task || terminal {
                (Some(marker), None)
            } else {
                (None, Some(marker))
            };
            self.part(row, Part::ListItem)
                .child(before)
                .child(self.part(content.child(inside), Part::ListItemContent))
        });
        self.list_depth.set(depth.saturating_add(1));
        let rows: Vec<_> = rows.collect();
        self.list_depth.set(depth);
        // Loose or not, items are as close (Allsum's paragraphs in items
        // have no margins); a loose item's own blocks are a gap apart.
        self.parts(view().flex_col().gap(px(ITEM_GAP)), &[Part::List, kind])
            .children(rows)
            .into_element()
    }

    /// A GitHub alert: a box tinted with its kind's color, its icon before
    /// its blocks.
    fn alert(&self, alert: Alert, children: &[Block], key: &str, tail: bool) -> AnyElement {
        let (part, glyph) = match alert {
            Alert::Note => (Part::AlertNote, "i"),
            Alert::Tip => (Part::AlertTip, "✓"),
            Alert::Important => (Part::AlertImportant, "i"),
            Alert::Warning => (Part::AlertWarning, "!"),
            Alert::Caution => (Part::AlertCaution, "!"),
        };
        let tint = move || alert_color(alert);
        if is_terminal() {
            // One band across the text's width (in a row, so it does not
            // reach across a card's padding), a Cell of padding each side
            // and no rows: its icon in its color, its text right after.
            // Where a terminal shows no bands (16 colors, none), its line
            // on the left in its color instead, where a quote's bar is.
            let icon = text(glyph)
                .flex_none()
                .font_weight(FontWeight::BOLD)
                .text_color(tint);
            let band = view()
                .flex_row()
                .flex_1()
                .min_w(px(0.0))
                .px(crate::terminal::cell_width())
                .bg(move || tint().opacity(0.12))
                .border_l(px(1.0))
                .border_color(tint)
                .child(self.part(icon, Part::AlertIcon))
                .child(self.blocks(
                    view().flex_col().flex_1().min_w(px(0.0)),
                    children,
                    key,
                    tail,
                ));
            return view()
                .flex_row()
                .child(self.parts(band, &[Part::Blockquote, Part::Alert, part]))
                .into_element();
        }
        let icon = view()
            .flex_none()
            .size(px(ALERT_ICON))
            .mt(px((BODY_SIZE * BODY_LINE - ALERT_ICON) / 2.0))
            .rounded_full()
            .border(px(1.5))
            .border_color(tint)
            .items_center()
            .justify_center()
            .child(
                text(glyph)
                    .text_size(px(10.0))
                    .line_height(px(12.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(tint),
            );
        let column = view().flex_col().flex_1().gap(px(BLOCK_GAP));
        let row = view()
            .flex_row()
            .gap(px(10.0))
            .py(px(12.0))
            .px(px(16.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(move || tint().opacity(0.3))
            .bg(move || tint().opacity(0.08));
        self.parts(row, &[Part::Blockquote, Part::Alert, part])
            .child(self.part(icon, Part::AlertIcon))
            .child(self.blocks(column, children, key, tail))
            .into_element()
    }

    fn code_block(&self, code: &CodeBlock, key: &str, tail: bool) -> AnyElement {
        if let Some(render) = &self.render_code_block {
            return render
                .render(format!("code:{key}"), code.clone())
                .into_element();
        }
        if is_terminal() {
            return self.terminal_code_block(code, tail);
        }
        // The app's actions float over the top right corner, so the code
        // keeps its first line (no row is kept for them). Where a pointer
        // hovers (desktops, desktop browsers) they show while it is over the
        // block; touch screens have no hover, so there they always show.
        let hover = is_desktop();
        let header = (!self.code_block_actions.is_empty()).then(|| {
            let header = view()
                .absolute()
                .top(px(6.0))
                .right(px(6.0))
                .flex_row()
                .items_center()
                .gap(px(2.0))
                .when(hover, |h| {
                    h.opacity(0.0)
                        .group_hover(|s| s.opacity(1.0))
                        .transition_opacity()
                        .duration(150.0)
                });
            self.part(header, Part::CodeBlockHeader).children(
                self.code_block_actions
                    .iter()
                    .enumerate()
                    .map(|(i, action)| self.code_action(action, code, &format!("{key}.{i}"))),
            )
        });
        // The app's highlighter, else the built-in one (which may color the
        // code a moment after it shows), else plain.
        let body = match &self.highlighter {
            Some(h) => text("").children(h.spans(code)),
            None => match super::highlight::built_in(code) {
                Some(spans) => text("").children(spans),
                None => text(code.code.clone()),
            },
        };
        // The padding is the code's, inside the scroll view: the code
        // scrolls to the edges, and the scroll bar sits in the bottom
        // padding rather than over the last line.
        let body = self.caret_after(
            body.font_family(mono_font)
                .px(px(16.0))
                .py(px(14.0))
                .when(self.selectable, |t| t.select_text()),
            tail,
        );
        let block = view()
            .flex_col()
            .when(hover, |b| b.group())
            .rounded(px(8.0))
            .border_1()
            .border_color(move || theme().border)
            .bg(move || theme().muted)
            .text_size(px(CODE_SIZE))
            .line_height(px(CODE_SIZE * CODE_LINE));
        // The actions last, so they are drawn over the code.
        self.part(block, Part::CodeBlock)
            .child(
                self.part(
                    scroll().horizontal().scrollbar(self.code_scroll_indicator),
                    Part::CodeBlockScroll,
                )
                .child(self.part(body, Part::CodeBlockCode)),
            )
            .child(header)
            .into_element()
    }

    /// A code block in a terminal: a band as wide as its longest line and
    /// a Cell each side (at most the text's width, its longer lines
    /// wrapping), from the text's left edge, with no rows of padding; its
    /// language named above it in reverse (` ts `, bold, on the border
    /// color).
    fn terminal_code_block(&self, code: &CodeBlock, tail: bool) -> AnyElement {
        let body = match &self.highlighter {
            Some(h) => text("").children(h.spans(code)),
            None => match super::highlight::built_in(code) {
                Some(spans) => text("").children(spans),
                None => text(code.code.clone()),
            },
        };
        let body = self.caret_after(
            body.font_family(mono_font)
                .when(self.selectable, |t| t.select_text()),
            tail,
        );
        let language = code
            .language
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| {
                text(format!(" {l} "))
                    .flex_none()
                    .font_weight(FontWeight::BOLD)
                    .text_color(move || theme().background)
                    .bg(move || theme().border)
            });
        let band = terminal_band(self.part(body, Part::CodeBlockCode)).bg(move || theme().muted);
        self.part(
            view().flex_col().items_start().child(language).child(band),
            Part::CodeBlock,
        )
        .into_element()
    }

    /// A table as one grid, so its columns line up across rows: each column
    /// fits its content when there is room (the spare width shared out),
    /// shrinks (its cells wrapping) down to a floor when the view is
    /// narrower, and below the floors the grid keeps them and scrolls
    /// horizontally ([`column_floor`]).
    ///
    /// A row is a view behind its cells (the same grid row, every column),
    /// so its classes draw backgrounds and borders; text styles go on the
    /// cells.
    fn table(&self, table: &Table, key: &str, tail: bool) -> AnyElement {
        if is_terminal() {
            return self.table_cells(table, key, tail);
        }
        self.with_line(TABLE_SIZE * TABLE_LINE, || {
            self.table_grid(table, key, tail)
        })
    }

    /// A table in a terminal: its rows one after the other, each column as
    /// many Cells wide as its widest cell's text, ruled in box drawing
    /// (`┌─┬─┐` above, `│` between the cells, `├─┼─┤` under the header,
    /// `└─┴─┘` below), the header bold. A cell spanning columns takes their
    /// Cells and the rules between them. A table wider than the view
    /// scrolls, as everywhere.
    fn table_cells(&self, table: &Table, key: &str, tail: bool) -> AnyElement {
        let columns = table.columns().max(1);
        let align = |i: usize| table.align.get(i).copied().unwrap_or_default();
        let glyph = measure_text("", "0");
        let natural = Rc::new(column_cells(table, |line| match glyph.0 > 0.0 {
            true => (measure_text("", line).0 / glyph.0 - 0.01).ceil().max(0.0) as usize,
            false => line.chars().count(),
        }));
        let cell = crate::terminal::cell_width();
        // The Cells the text is wide, once laid out: the columns fit them
        // (their cells wrapping), or the table scrolls.
        let room = signal(0usize);
        let widths = move || fit_columns(&natural, room.get());
        let ruled = |ends: [char; 3]| {
            let widths = widths.clone();
            text(move || SharedString::from(table_rule(&widths(), ends)))
                .number_of_lines(1u32)
                .text_color(move || theme().border)
                .into_element()
        };
        let bar = |glyph: &'static str| text(glyph).flex_none().text_color(move || theme().border);
        let last_row = table.rows.len();
        let mut lines = vec![ruled(['┌', '┬', '┐'])];
        let rows = std::iter::once(&table.head).chain(&table.rows);
        for (r, cells) in rows.enumerate() {
            let head = r == 0;
            let row_parts: &[Part] = if head {
                &[Part::TableRow, Part::TableHead]
            } else {
                &[Part::TableRow]
            };
            let cell_parts: &[Part] = if head {
                &[Part::TableCell, Part::TableHeaderCell]
            } else {
                &[Part::TableCell]
            };
            let mut placed = Vec::new();
            let mut col = 0;
            for cell in cells.iter() {
                if col >= columns {
                    break;
                }
                let span = cell.span().min(columns - col);
                placed.push((col, span, cell.content.as_slice()));
                col += span;
            }
            placed.extend((col..columns).map(|c| (c, 1, &[][..])));
            let mut row = view().flex_row().child(bar("│ "));
            for (n, (c, span, content)) in placed.into_iter().enumerate() {
                if n > 0 {
                    row = row.child(bar(" │ "));
                }
                let end = tail && r == last_row && c + span == columns;
                let key = format!("{key}.{r}.{c}");
                // The columns it spans and the ` │ ` between them.
                let widths = widths.clone();
                let cells = move || widths()[c..c + span].iter().sum::<usize>() + 3 * (span - 1);
                let t = self.paragraph(content, &key, end);
                let t = match align(c) {
                    ColumnAlign::Center => t.text_center(),
                    ColumnAlign::Right => t.text_right(),
                    ColumnAlign::Left => t.text_left(),
                    ColumnAlign::None => t,
                };
                let t = t.when(head, |t| t.font_weight(FontWeight::BOLD));
                let cell = view()
                    .flex_none()
                    .w(move || Length::from(cell * cells() as f32));
                row = row.child(self.parts(cell, cell_parts).child(t));
            }
            lines.push(self.parts(row.child(bar(" │")), row_parts).into_element());
            if head {
                lines.push(ruled(['├', '┼', '┤']));
            }
        }
        lines.push(ruled(['└', '┴', '┘']));
        let actions = self
            .render_table_actions
            .as_ref()
            .map(|r| r.render(format!("table:{key}"), table.clone()));
        let table_view = view().flex_col().on_layout(move |frame: &Bounds<Points>| {
            let n = crate::terminal::columns_in(frame.size.width, cell);
            if room.get_untracked() != n {
                room.set(n);
            }
        });
        self.part(table_view, Part::Table)
            .child(actions)
            .child(
                self.part(
                    scroll().horizontal().scrollbar(self.table_scrollbar),
                    Part::TableScroll,
                )
                .child(view().flex_col().items_start().children(lines)),
            )
            .into_element()
    }

    /// A code block's action: a ghost button (its background shows while
    /// hovered or pressed) around the action's icon.
    fn code_action(&self, action: &TextViewCodeAction, code: &CodeBlock, key: &str) -> AnyElement {
        let scope = self.scope;
        let copy = action.id.as_ref() == "copy";
        let label = action.label.clone().unwrap_or_else(|| action.id.clone());
        // 0: the action's icon, 1: the check a copy shows for a moment.
        let done = animated_value(0.0);
        let icon = if copy {
            copy_icon(&done)
        } else if let Some(render) = &self.render_code_block_action {
            render
                .render(format!("action:{key}"), action.clone())
                .into_element()
        } else {
            text(label.clone())
                .text_size(px(11.0))
                .text_color(move || theme().muted_foreground)
                .into_element()
        };
        let event = TextViewCodeActionEvent {
            id: action.id.clone(),
            code: code.clone(),
        };
        let hovered = theme().foreground.opacity(0.08);
        let pressed = theme().foreground.opacity(0.14);
        view()
            .min_w(px(24.0))
            .h(px(24.0))
            .px(px(5.0))
            .rounded(px(6.0))
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(hovered))
            .active(|s| s.bg(pressed))
            .role(AccessibilityRole::Button)
            .label(label)
            .on_press(move || {
                if copy {
                    set_clipboard(event.code.code.clone());
                    let _ = done.spring(
                        1.0,
                        Spring {
                            stiffness: 320.0,
                            damping: 18.0,
                            ..Spring::default()
                        },
                    );
                    set_timeout(std::time::Duration::from_millis(1600), move || {
                        let _ = done.timing(0.0, 180.0, Easing::EaseOut);
                    });
                }
                scope.emit(TextViewEvent::CodeBlockAction(event.clone()));
            })
            .child(icon)
            .into_element()
    }

    fn table_grid(&self, table: &Table, key: &str, tail: bool) -> AnyElement {
        let last_row = table.rows.len();
        let columns = table.columns();
        let align = |i: usize| table.align.get(i).copied().unwrap_or_default();
        // The cells that start at column `c` and span it alone (a spanning
        // cell's width is shared by its columns).
        let at = |row: &'_ [super::TableCell], c: usize| -> Option<usize> {
            let mut col = 0;
            for (i, cell) in row.iter().enumerate() {
                if col == c {
                    return (cell.span() == 1).then_some(i);
                }
                col += cell.span();
                if col > c {
                    return None;
                }
            }
            None
        };
        let floors: Vec<f32> = (0..columns)
            .map(|c| {
                let cells = std::iter::once(&table.head).chain(&table.rows);
                column_floor(cells.filter_map(|row| at(row, c).map(|i| &row[i])))
            })
            .collect();
        let tracks = floors
            .iter()
            .map(|f| format!("minmax({f:.2}rem, auto)"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut grid = Vec::with_capacity((columns + 1) * (last_row + 1));
        let rows = std::iter::once(&table.head).chain(&table.rows);
        for (r, cells) in rows.enumerate() {
            let head = r == 0;
            let line = i16::try_from(r + 1).unwrap_or(i16::MAX);
            let row_parts: &[Part] = if head {
                &[Part::TableRow, Part::TableHead]
            } else {
                &[Part::TableRow]
            };
            let cell_parts: &[Part] = if head {
                &[Part::TableCell, Part::TableHeaderCell]
            } else {
                &[Part::TableCell]
            };
            // First, so the cells are drawn over it.
            let row = view()
                .row_start(line)
                .col_span_full()
                .when(head, |v| v.bg(move || theme().muted));
            grid.push(self.parts(row, row_parts).into_element());
            // Each cell where it starts, as wide as it spans; the columns no
            // cell covers (a short row) are empty cells.
            let mut placed = Vec::new();
            let mut col = 0;
            for cell in cells.iter() {
                if col >= columns {
                    break;
                }
                let span = cell.span().min(columns - col);
                placed.push((col, span, cell.content.as_slice()));
                col += span;
            }
            placed.extend((col..columns).map(|c| (c, 1, &[][..])));
            grid.extend(placed.into_iter().map(|(c, span, content)| {
                let end = tail && r == last_row && c + span == columns;
                let key = format!("{key}.{r}.{c}");
                let t = match only_images(content) {
                    Some(images) => {
                        let column = self.image_column(&images, &key);
                        match align(c) {
                            ColumnAlign::Center => column.items_center(),
                            ColumnAlign::Right => column.items_end(),
                            ColumnAlign::Left | ColumnAlign::None => column,
                        }
                        .into_element()
                    }
                    None => {
                        let t = self.paragraph(content, &key, end);
                        match align(c) {
                            ColumnAlign::Center => t.text_center(),
                            ColumnAlign::Right => t.text_right(),
                            ColumnAlign::Left => t.text_left(),
                            ColumnAlign::None => t,
                        }
                        .into_element()
                    }
                };
                // On the cell, so its classes can set another weight. Each
                // cell draws its right and bottom lines, the grid the top
                // and left ones: no line is drawn twice.
                let cell = view()
                    .row_start(line)
                    // Start and end lines: a span would replace the start.
                    .col_start(i16::try_from(c + 1).unwrap_or(i16::MAX))
                    .col_end(i16::try_from(c + 1 + span).unwrap_or(i16::MAX))
                    .py(px(8.0))
                    .px(px(12.0))
                    .border_r_1()
                    .border_b_1()
                    .border_color(move || theme().border)
                    .when(head, |v| {
                        v.text_size(px(CODE_SIZE))
                            .text_color(move || theme().muted_foreground)
                    });
                self.parts(cell, cell_parts).child(t).into_element()
            }));
        }
        let actions = self
            .render_table_actions
            .as_ref()
            .map(|r| r.render(format!("table:{key}"), table.clone()));
        // As wide as the view, never narrower than the floors: past them,
        // the scroll view moves it.
        let grid = view()
            .grid()
            .grid_template_columns(SharedString::from(tracks))
            .w_full()
            .min_w(rems(floors.iter().sum()))
            .border_t_1()
            .border_l_1()
            .border_color(move || theme().border)
            .children(grid);
        let table = view()
            .flex_col()
            .text_size(px(TABLE_SIZE))
            .line_height(px(TABLE_SIZE * TABLE_LINE));
        self.part(table, Part::Table)
            .child(actions)
            .child(
                self.part(
                    scroll().horizontal().scrollbar(self.table_scrollbar),
                    Part::TableScroll,
                )
                .child(grid),
            )
            .into_element()
    }

    fn details(
        &self,
        open: bool,
        summary: &[Inline],
        children: &[Block],
        key: &str,
        tail: bool,
    ) -> AnyElement {
        let shown = signal(open);
        let marker = text(move || SharedString::from(if shown.get() { "▾ " } else { "▸ " }))
            .text_color(move || theme().muted_foreground);
        let summary = self.part(
            view()
                .flex_row()
                .role(AccessibilityRole::Button)
                .accessibility_state(move || AccessibilityState {
                    expanded: Some(shown.get()),
                    ..AccessibilityState::default()
                })
                .on_press(move || shown.set(!shown.get()))
                .child(self.part(marker, Part::SummaryMarker))
                .child(self.paragraph(summary, &format!("{key}.s"), false).flex_1()),
            Part::Summary,
        );
        // The content is made each time it opens.
        let cx = self.clone();
        let children = children.to_vec();
        let key = key.to_owned();
        let body = move || cx.blocks(view().flex_col().gap(px(BLOCK_GAP)), &children, &key, tail);
        self.part(view().flex_col().gap(px(ITEM_GAP * 2.0)), Part::Details)
            .child(summary)
            .child(when(move || shown.get(), body))
            .into_element()
    }

    fn custom_block(&self, node: &CustomNode, key: &str, tail: bool) -> AnyElement {
        if let Some(el) = self.plugins.render(node) {
            return el;
        }
        if let Some(render) = &self.render_custom {
            return render
                .render(format!("custom:{key}"), node.clone())
                .into_element();
        }
        let el = if !node.children.is_empty() {
            self.blocks(
                view().flex_col().gap(px(BLOCK_GAP)),
                &node.children,
                key,
                tail,
            )
            .into_element()
        } else if !node.content.is_empty() {
            self.paragraph(&node.content, key, tail).into_element()
        } else {
            self.caret_after(
                text(node.text.clone()).when(self.selectable, |t| t.select_text()),
                tail,
            )
            .into_element()
        };
        self.part(view().flex_col().child(el), Part::Custom)
            .into_element()
    }

    /// The spans of `content`.
    fn inlines(&self, content: &[Inline], key: &str) -> Vec<AnyElement> {
        content
            .iter()
            .enumerate()
            .map(|(i, inline)| self.inline(inline, &format!("{key}.{i}")))
            .collect()
    }

    fn span(&self, content: &[Inline], key: &str) -> Text {
        text("").children(self.inlines(content, key))
    }

    fn inline(&self, inline: &Inline, key: &str) -> AnyElement {
        match inline {
            Inline::Text { text: t } => text(t.clone()).into_element(),
            Inline::Strong { content } => self
                .part(
                    self.span(content, key).font_weight(FontWeight::SEMIBOLD),
                    Part::Strong,
                )
                .into_element(),
            Inline::Emphasis { content } => self
                .part(self.span(content, key).italic(), Part::Emphasis)
                .into_element(),
            Inline::Strikethrough { content } => self
                .part(self.span(content, key).line_through(), Part::Strikethrough)
                .into_element(),
            Inline::Underline { content } => self
                .part(self.span(content, key).underline(), Part::Underline)
                .into_element(),
            Inline::Highlight { content, color } => {
                // The document's color, else the view's own.
                let color = color.as_deref().and_then(|c| c.parse::<Hsla>().ok());
                let span = self
                    .span(content, key)
                    .bg(move || color.map_or_else(|| HIGHLIGHT.opacity(0.4), AdaptiveColor::from))
                    .rounded(px(3.0));
                // A solid color of the document's is the same in both
                // appearances: its text takes the ink that reads on it
                // (as Allsum's `--mark-solid-text`), not the page's.
                let span = match color {
                    Some(c) => span.text_color(ink_on(c)),
                    None => span,
                };
                self.part(span, Part::Highlight).into_element()
            }
            Inline::Superscript { content } => self
                .part(
                    self.span(content, key).text_size(rems(0.75)),
                    Part::Superscript,
                )
                .into_element(),
            Inline::Subscript { content } => self
                .part(
                    self.span(content, key).text_size(rems(0.75)),
                    Part::Subscript,
                )
                .into_element(),
            // As a browser's `smaller`: 13 of the body's 15.
            Inline::Small { content } => self
                .part(self.span(content, key).text_size(px(13.0)), Part::Small)
                .into_element(),
            Inline::Code { code } => self
                .part(code_span(text(code.clone())), Part::Code)
                .into_element(),
            Inline::Keyboard { text: keys } => self
                .part(code_span(text(keys.clone())), Part::Keyboard)
                .into_element(),
            Inline::Link {
                url,
                title,
                content,
            } => {
                let url = SharedString::from(url.clone());
                let outer = self.link.replace(Some(url.clone()));
                let span = self.span(content, key);
                self.link.replace(outer);
                let el = span
                    .text_color(move || theme().primary)
                    .underline()
                    .text_decoration_color(move || theme().primary.opacity(0.4))
                    .text_decoration_thickness(px(1.0))
                    // No offset of its own: the native backends clip a
                    // decoration to the text's box, so a fixed one below a
                    // tight line's box hides the underline (#447).
                    .role(AccessibilityRole::Link)
                    .when_some(title.clone(), |t, title| t.hint(title))
                    .on_press(self.link_press(url));
                self.part(el, Part::Link).into_element()
            }
            Inline::Image(image) => {
                let link = self.link.borrow().clone();
                self.image(image, link, ImageFlow::Inline(self.line.get()), key)
            }
            Inline::Math { tex } => match &self.render_math {
                Some(render) => render
                    .render(
                        format!("math:{key}"),
                        TextViewMath {
                            tex: tex.clone().into(),
                            display: false,
                        },
                    )
                    .into_element(),
                // A terminal reads the formula as text (`16.7 ms`), not
                // its source.
                None if is_terminal() => self
                    .part(text(super::tex_text::tex_text(tex)), Part::Math)
                    .into_element(),
                // Typeset in the line, its middle on the text's.
                None => match self.formula(tex, false, BODY_SIZE) {
                    Some(formula) => self
                        .part(
                            view()
                                .flex_row()
                                .align_middle()
                                .label(tex.clone())
                                .child(formula),
                            Part::Math,
                        )
                        .into_element(),
                    None => self
                        .part(code_span(text(tex.clone())), Part::Math)
                        .into_element(),
                },
            },
            Inline::Break => text("\n").into_element(),
            Inline::FootnoteReference { label, index } => {
                let scope = self.scope;
                let url = SharedString::from(format!("#footnote-{label}"));
                // A terminal has one text size: the number in brackets,
                // apart from the word it follows.
                let number = match is_terminal() {
                    true => format!("[{index}]"),
                    false => index.to_string(),
                };
                self.part(
                    text(number)
                        .text_size(rems(0.75))
                        .text_color(move || theme().primary)
                        .role(AccessibilityRole::Link)
                        .on_press(move || {
                            scope.emit(TextViewEvent::LinkPress(TextViewLinkPressEvent {
                                url: url.clone(),
                            }))
                        }),
                    Part::FootnoteReference,
                )
                .into_element()
            }
            Inline::Custom(node) => {
                if let Some(el) = self.plugins.render(node) {
                    return el;
                }
                if let Some(render) = &self.render_custom {
                    return render
                        .render(format!("custom:{key}"), node.clone())
                        .into_element();
                }
                let el = if node.content.is_empty() {
                    text(node.text.clone())
                } else {
                    self.span(&node.content, key)
                };
                self.part(el, Part::Custom).into_element()
            }
        }
    }

    /// `f`'s result, made with images in text sized by line height
    /// `line`.
    fn with_line<R>(&self, line: f32, f: impl FnOnce() -> R) -> R {
        let outer = self.line.replace(line);
        let out = f();
        self.line.set(outer);
        out
    }

    /// Images that stand alone (a paragraph or a cell with no text), one
    /// under another.
    fn image_column(&self, images: &[(&Image, Option<&str>)], key: &str) -> View {
        view()
            .flex_col()
            .items_start()
            .children(images.iter().enumerate().map(|(i, (image, link))| {
                let link = link.map(|l| SharedString::from(l.to_owned()));
                self.image(image, link, ImageFlow::Block, &format!("{key}.{i}"))
            }))
    }

    /// Image `image`, in a link to `link` if it is in one (a press on it is
    /// the link's), sized as `flow` says.
    fn image(
        &self,
        image: &Image,
        link: Option<SharedString>,
        flow: ImageFlow,
        key: &str,
    ) -> AnyElement {
        let scope = self.scope;
        let pressed = image.clone();
        let is_link = link.is_some();
        let link = link.map(|url| self.link_press(url));
        // In a link it is the link's; else a press reports the image.
        let press = Some(move || match &link {
            Some(press) => press(),
            None => scope.emit(TextViewEvent::ImagePress(pressed.clone())),
        });
        let role = if is_link {
            AccessibilityRole::Link
        } else {
            AccessibilityRole::ImageButton
        };
        if let Some(render) = &self.render_image {
            let el = render.render(format!("image:{key}"), image.clone());
            return match press {
                Some(press) => view()
                    .role(role)
                    .label(SharedString::from(image.alt.clone()))
                    .on_press(press)
                    .child(el)
                    .into_element(),
                None => el.into_element(),
            };
        }
        // A terminal's block image reads as its alt text in brackets, as a
        // text browser shows one: a line of text, pressable as the image
        // (`block` leaves out a paragraph of images without alt text).
        if is_terminal() && matches!(flow, ImageFlow::Block) {
            if image.alt.trim().is_empty() {
                return view().into_element();
            }
            let el = text(format!("[{}]", image.alt.trim()))
                .text_color(move || theme().muted_foreground)
                .label(SharedString::from(image.alt.clone()));
            let el = match press {
                Some(press) => el.role(role).on_press(press),
                None => el.role(AccessibilityRole::Image),
            };
            return self.part(el, Part::Image).into_element();
        }
        let (width, height) = (image.width, image.height);
        // Its natural size (a pixel a point, as a browser takes it), once
        // known, for what its own size leaves to its aspect ratio.
        let natural = signal(None::<Size<Points>>);
        let sized =
            matches!(
                (width, height),
                (Some(ImageLength::Points(_)), Some(ImageLength::Points(_)))
            ) || matches!(flow, ImageFlow::Inline(_)) && width.is_some() && height.is_some();
        if !sized {
            let src = image.src.clone();
            spawn(async move {
                if let Ok(size) = image_size(src).await {
                    natural.set(Some(Size::new(
                        px(size.width.0 as f32),
                        px(size.height.0 as f32),
                    )));
                }
            });
        }
        let aspect = move || {
            natural
                .get()
                .filter(|s| s.width > px(0.0) && s.height > px(0.0))
                .map(|s| s.width / s.height)
        };
        let el = nsh_core::element::image(image.src.clone())
            .object_fit(ResizeMode::Contain)
            .label(SharedString::from(image.alt.clone()))
            .when_some(image.title.clone(), |el, title| el.hint(title));
        let el = match flow {
            // At most as wide as the text: as wide as it asks, the height
            // following (at its natural size when it asks for nothing). The
            // width is a box's around it, so the image's height follows
            // the width it ends up with, not the one it asked for.
            ImageFlow::Block => {
                let ratio = match (width, height) {
                    (Some(ImageLength::Points(w)), Some(ImageLength::Points(h))) if h > px(0.0) => {
                        Some(w / h)
                    }
                    _ => None,
                };
                let box_width = move || -> Length {
                    match (width, height) {
                        (Some(w), _) => DefiniteLength::from(w).into(),
                        (None, Some(ImageLength::Points(h))) => {
                            (h * aspect().unwrap_or(1.0)).into()
                        }
                        (None, _) => natural.get().map_or(px(0.0), |s| s.width).into(),
                    }
                };
                let el = el
                    .rounded(px(8.0))
                    .w_full()
                    .aspect_ratio(move || ratio.or_else(aspect).unwrap_or(1.0));
                let el = match press {
                    Some(press) => el.role(role).on_press(press),
                    None => el.role(AccessibilityRole::Image),
                };
                return view()
                    .w(box_width)
                    .max_w(relative(1.0))
                    .child(self.part(el, Part::Image))
                    .into_element();
            }
            // In a line of text: its size, else as tall as most of the
            // line, as gpui-kit's text flow sizes images.
            ImageFlow::Inline(line) => {
                let size = move || inline_image_size(width, height, aspect(), line);
                // An attachment sits on the baseline; moved down (drawing
                // only) so its middle is the middle of the lowercase
                // letters, about 0.35 em above the baseline: centered on
                // the line. The text's size is its line's over 1.7.
                let em = line / 1.7;
                el.w(move || size().width)
                    .h(move || size().height)
                    .translate_y(move || size().height / 2.0 - px(0.35 * em))
            }
        };
        let el = match press {
            Some(press) => el.role(role).on_press(press),
            None => el.role(AccessibilityRole::Image),
        };
        self.part(el, Part::Image).into_element()
    }
}

/// Where an image is laid out.
#[derive(Debug, Clone, Copy)]
enum ImageFlow {
    /// On its own: a block.
    Block,
    /// In text whose lines are this tall.
    Inline(f32),
}

/// The images of content that is only images (each in a link or not), with
/// their links: shown as blocks, as gpui-kit shows a paragraph of images
/// with no text.
fn only_images(content: &[Inline]) -> Option<Vec<(&Image, Option<&str>)>> {
    fn walk<'a>(
        content: &'a [Inline],
        link: Option<&'a str>,
        out: &mut Vec<(&'a Image, Option<&'a str>)>,
    ) -> bool {
        content.iter().all(|inline| match inline {
            Inline::Image(image) => {
                out.push((image, link));
                true
            }
            Inline::Link { url, content, .. } => walk(content, Some(url), out),
            _ => false,
        })
    }
    let mut out = Vec::new();
    (walk(content, None, &mut out) && !out.is_empty()).then_some(out)
}

/// The size of an image in text with lines `line` tall, as gpui-kit's text
/// flow sizes it: its width and height, one of them and the other from its
/// aspect ratio (`aspect`, once known; a line until then), or neither and
/// three quarters of a line tall. A percentage is of the line's height,
/// the length gpui-kit resolves it against in text.
fn inline_image_size(
    width: Option<ImageLength>,
    height: Option<ImageLength>,
    aspect: Option<f32>,
    line: f32,
) -> Size<Points> {
    let resolve = |length: ImageLength| match length {
        ImageLength::Points(points) => points,
        ImageLength::Fraction(fraction) => fraction.of(px(line)),
    };
    let (w, h) = match (width.map(resolve), height.map(resolve)) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, aspect.map_or(px(line), |a| w / a)),
        (None, Some(h)) => (aspect.map_or(h, |a| h * a), h),
        (None, None) => {
            let h = px(line * 0.75);
            (h * aspect.unwrap_or(1.0), h)
        }
    };
    Size::new(w.max(px(1.0)), h.max(px(1.0)))
}

#[module]
impl TextView {
    /// Markdown `markdown` read into a document, as the `markdown` prop
    /// reads it (with `options` and `extensions`).
    pub fn parse(
        markdown: String,
        options: Option<MarkdownOptions>,
        extensions: Option<Vec<MarkdownExtension>>,
    ) -> Document {
        Parser::new()
            .options(options.unwrap_or_default())
            .extensions(extensions.unwrap_or_default())
            .markdown(&markdown)
    }

    /// HTML `html` read into a document, as the `html` prop reads it.
    pub fn parse_html(html: String, extensions: Option<Vec<MarkdownExtension>>) -> Document {
        Parser::new()
            .extensions(extensions.unwrap_or_default())
            .html(&html)
    }

    /// The text of `document` without markup (what copying a whole reply
    /// gives): blocks separated by blank lines, list items and table rows
    /// by line breaks, cells by tabs.
    pub fn plain_text(document: Document) -> String {
        document.plain_text()
    }
}

/// Lines that stand out more than the theme's borders (quotes' and
/// rules'): the text's color, faint.
fn strong_border() -> AdaptiveColor {
    theme().foreground.opacity(0.15)
}

/// Code in text: monospace, smaller, on a rounded background (the text's
/// color, faint, so it shows on any surface).
fn code_span(span: Text) -> Text {
    span.font_family(mono_font)
        .text_size(px(CODE_SIZE))
        .bg(move || theme().foreground.opacity(0.06))
        .rounded(px(4.0))
}

/// The color of an alert of kind `alert`: GitHub's alert colors (shadcn's
/// tokens have no status colors), a caution the theme's `destructive`.
fn alert_color(alert: Alert) -> AdaptiveColor {
    match alert {
        Alert::Note => AdaptiveColor::rgb(0x0969da, 0x4493f8),
        Alert::Tip => AdaptiveColor::rgb(0x1a7f37, 0x3fb950),
        Alert::Important => AdaptiveColor::rgb(0x8250df, 0xab7df8),
        Alert::Warning => AdaptiveColor::rgb(0x9a6700, 0xd29922),
        Alert::Caution => theme().destructive,
    }
}

/// Highlighted text's background, at 40% (`==text==`, `<mark>`): a
/// marker's yellow.
const HIGHLIGHT: AdaptiveColor = AdaptiveColor::rgb(0xfde047, 0xca8a04);

/// The theme's monospaced font (`font-mono`), else the system's.
fn mono_font() -> SharedString {
    theme()
        .font_mono
        .clone()
        .unwrap_or_else(|| SharedString::from("monospace"))
}

// The reading typography (sizes in points), after Allsum's documents.

/// The text's size.
const BODY_SIZE: f32 = 15.0;
/// The text's line height, a multiple of its size.
const BODY_LINE: f32 = 1.7;
/// The size of code.
const CODE_SIZE: f32 = 13.0;
/// The line height of code blocks, a multiple of [`CODE_SIZE`].
const CODE_LINE: f32 = 1.6;
/// How many Cells each of `table`'s columns takes in a terminal: its
/// widest cell's widest line (`cells` measures one), at least one; a
/// spanning cell widens its last column by what its columns and the rules
/// between them lack.
fn column_cells(table: &Table, cells: impl Fn(&str) -> usize) -> Vec<usize> {
    let columns = table.columns().max(1);
    let mut widths = vec![1; columns];
    let mut spans = Vec::new();
    for row in std::iter::once(&table.head).chain(&table.rows) {
        let mut col = 0;
        for cell in row {
            if col >= columns {
                break;
            }
            let span = cell.span().min(columns - col);
            let mut text = String::new();
            super::document::inline_text(&cell.content, &mut text);
            let width = text.lines().map(&cells).max().unwrap_or(0);
            match span {
                1 => widths[col] = widths[col].max(width),
                _ => spans.push((col, span, width)),
            }
            col += span;
        }
    }
    for (col, span, width) in spans {
        let has = widths[col..col + span].iter().sum::<usize>() + 3 * (span - 1);
        if width > has {
            widths[col + span - 1] += width - has;
        }
    }
    widths
}

/// The narrowest a terminal's table narrows a column to: 6 Cells.
const MIN_COLUMN: usize = 6;

/// `natural`'s column widths fitted to `room` Cells (none yet: as they
/// are): the widest column narrowed a Cell at a time, its cells wrapping,
/// until the table and its rules (a `│` before each column and after the
/// last, a Cell each side in each) fit or every column is down to
/// [`MIN_COLUMN`], when it scrolls.
fn fit_columns(natural: &[usize], room: usize) -> Vec<usize> {
    let mut widths = natural.to_vec();
    if room == 0 {
        return widths;
    }
    let budget = room.saturating_sub(3 * widths.len() + 1);
    while widths.iter().sum::<usize>() > budget {
        let widest = widths
            .iter()
            .enumerate()
            .filter(|(_, w)| **w > MIN_COLUMN)
            .max_by_key(|(i, w)| (**w, std::cmp::Reverse(*i)))
            .map(|(i, _)| i);
        let Some(i) = widest else {
            break;
        };
        widths[i] -= 1;
    }
    widths
}

/// A table's rule in a terminal across columns `widths` Cells wide, each
/// padded a Cell on both sides: `ends[0]`, `ends[1]` between two columns,
/// `ends[2]`.
fn table_rule(widths: &[usize], ends: [char; 3]) -> String {
    let mut line = String::from(ends[0]);
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            line.push(ends[1]);
        }
        line.extend(std::iter::repeat_n('─', w + 2));
    }
    line.push(ends[2]);
    line
}

/// `content` in a terminal's band: a Cell of padding each side, no rows,
/// as wide as `content` up to the text's width, from the text's left
/// edge (not stretched, so it does not reach across the padding of a card
/// around the document, as a band of its own would).
fn terminal_band(content: impl IntoElement) -> View {
    view()
        .flex_none()
        .max_w(relative(1.0))
        .px(crate::terminal::cell_width())
        .child(content)
}

/// The last of `blocks` (those before one) that shows: in a terminal, not
/// one it leaves out ([`blank_in_terminal`]).
fn shown_before<'a>(blocks: impl DoubleEndedIterator<Item = &'a Block>) -> Option<&'a Block> {
    let terminal = is_terminal();
    blocks.rev().find(|b| !terminal || !blank_in_terminal(b))
}

/// Whether a terminal leaves `block` out, its row and the blank row
/// before it with it: a rule (the blank row between the blocks around it
/// is the break) and a paragraph of images with no alt text (a terminal
/// draws no images; one with alt text reads as `[alt]`).
fn blank_in_terminal(block: &Block) -> bool {
    match block {
        Block::Rule => true,
        Block::Paragraph { content } => only_images(content)
            .is_some_and(|images| images.iter().all(|(i, _)| i.alt.trim().is_empty())),
        _ => false,
    }
}

/// A browser's list markers by depth: disc, circle, square.
fn bullet(depth: u8) -> &'static str {
    match depth {
        0 => "•",
        1 => "◦",
        _ => "▪",
    }
}

/// The size of footnotes' text.
const SMALL_SIZE: f32 = 13.0;
/// The size of tables' text.
const TABLE_SIZE: f32 = 14.0;
/// The line height of tables' text, a multiple of [`TABLE_SIZE`].
const TABLE_LINE: f32 = 1.55;
/// The space between blocks (0.75em).
const BLOCK_GAP: f32 = BODY_SIZE * 0.75;
/// The space between list items (0.25em).
const ITEM_GAP: f32 = BODY_SIZE * 0.25;
/// The indent of list items, their markers in it (1.4em).
const LIST_INDENT: f32 = BODY_SIZE * 1.4;
/// The width of the box a list marker is right-aligned in, before the
/// content (its overflow reaches left of the indent, as numbers past 9 do
/// in a browser).
const MARKER_BOX: f32 = BODY_SIZE * 3.0;
/// The size of a task item's box.
const TASK_BOX: f32 = 15.0;
/// The size of an alert's icon.
const ALERT_ICON: f32 = 16.0;

/// What comes before a block in its column, for the room above it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Before {
    /// Nothing: the block is the first.
    Nothing,
    /// A heading.
    Heading,
    /// Another block.
    Block,
    /// An alert (another block).
    Alert,
}

impl Before {
    fn of(block: Option<&Block>) -> Self {
        match block {
            None => Self::Nothing,
            Some(Block::Heading { .. }) => Self::Heading,
            Some(Block::Blockquote { alert: Some(_), .. }) => Self::Alert,
            Some(_) => Self::Block,
        }
    }
}

/// A heading level's typography.
struct Heading {
    size: f32,
    /// The line height, a multiple of the size.
    line: f32,
    /// The letter spacing, in ems.
    tracking: f32,
    /// The room above it after a block, in ems.
    above: f32,
}

impl Heading {
    fn of(level: u8) -> Self {
        match level {
            1 => Self {
                size: 27.0,
                line: 1.25,
                tracking: -0.02,
                above: 1.1,
            },
            2 => Self {
                size: 20.0,
                line: 1.35,
                tracking: -0.012,
                above: 1.6,
            },
            3 => Self {
                size: 17.0,
                line: 1.35,
                tracking: -0.012,
                above: 1.4,
            },
            _ => Self {
                size: BODY_SIZE,
                line: 1.35,
                tracking: -0.012,
                above: 1.4,
            },
        }
    }

    /// Its top margin: its room above it less the column's gap (none
    /// first, less after another heading).
    fn margin_top(&self, before: Before) -> f32 {
        let above = match before {
            Before::Nothing => return 0.0,
            Before::Heading => 0.6,
            Before::Block | Before::Alert => self.above,
        };
        (self.size * above - BLOCK_GAP).max(0.0)
    }
}

/// The width in rems below which a table column stops shrinking (and the
/// table scrolls): about half its content, so long text wraps to two lines
/// or so, clamped to 10 to 30 rems so a huge column cannot push the scroll
/// threshold arbitrarily far; a narrower column keeps its content width
/// plus room for the cells' padding, so it never wraps. The content is
/// estimated from the cells' text, as text is measured on the platforms.
fn column_floor<'a>(cells: impl Iterator<Item = &'a super::TableCell>) -> f32 {
    const MIN: f32 = 10.0;
    const MAX: f32 = 30.0;
    const PADDING: f32 = 2.0;
    let content = cells
        .map(|cell| {
            let mut text = String::new();
            super::document::inline_text(&cell.content, &mut text);
            text.lines().map(text_width).fold(0.0, f32::max)
        })
        .fold(0.0, f32::max);
    (content + PADDING).min((content / 2.0).clamp(MIN, MAX))
}

/// The width of `line` in ems, estimated: wide (East Asian) characters
/// are an em, spaces a third, the rest a little over half.
fn text_width(line: &str) -> f32 {
    line.chars()
        .map(|c| match c as u32 {
            0x20 => 0.3,
            0x1100..=0x115f
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe30..=0xfe4f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0x1f300..=0x1faff
            | 0x20000..=0x3fffd => 1.0,
            _ => 0.6,
        })
        .sum()
}

/// The built-in copy action's icon (the gallery's copy glyph), which fades
/// and shrinks away as a check springs in once `done` goes to 1.
fn copy_icon(done: &AnimatedValue) -> AnyElement {
    let muted = theme().muted_foreground;
    let ok = theme().foreground;
    // The gallery's copy and check glyphs, drawn on a 20 unit grid scaled
    // to the icon's 14 pt.
    const K: f32 = 14.0 / 20.0;
    let polyline = |painter: Painter, points: &[f32]| {
        let mut painter = painter.begin_path().move_to(points[0] * K, points[1] * K);
        for p in points[2..].chunks(2) {
            painter = painter.line_to(p[0] * K, p[1] * K);
        }
        painter.stroke()
    };
    let pen = |painter: Painter, c| {
        painter
            .stroke_style(c)
            .line_width(1.6 * K)
            .line_cap(nsh_core::canvas::LineCap::Round)
            .line_join(nsh_core::canvas::LineJoin::Round)
    };
    let sheets = canvas(move || {
        let painter = pen(Painter::new(), color(muted))
            .begin_path()
            .round_rect(
                bounds(
                    point(px(7.0 * K), px(7.0 * K)),
                    size(px(10.0 * K), px(10.0 * K)),
                ),
                px(2.5 * K),
            )
            .stroke();
        polyline(
            painter,
            &[
                13.0, 4.5, 13.0, 4.0, 12.0, 3.0, 4.0, 3.0, 3.0, 4.0, 3.0, 12.0, 4.0, 13.0, 4.5,
                13.0,
            ],
        )
    });
    let check = canvas(move || {
        polyline(
            pen(Painter::new(), color(ok)),
            &[4.5, 10.5, 8.5, 14.0, 15.5, 6.0],
        )
    });
    let layer = |v: View| v.absolute().inset(px(0.0));
    view()
        .size(px(14.0))
        .child(
            layer(view())
                .bind_opacity(&done.interpolate(&[0.0, 1.0], &[1.0, 0.0]))
                .bind_scale(&done.interpolate(&[0.0, 1.0], &[1.0, 0.4]))
                .child(sheets.size(px(14.0))),
        )
        .child(
            layer(view())
                .bind_opacity(&done.interpolate(&[0.0, 1.0], &[0.0, 1.0]))
                .bind_scale(&done.interpolate(&[0.0, 1.0], &[0.4, 1.0]))
                .child(check.size(px(14.0))),
        )
        .into_element()
}

/// The text color that reads on background `bg`: near black on a light
/// color, near white on a dark one (by relative luminance).
fn ink_on(bg: Hsla) -> Hsla {
    let c = nsh_core::color::Rgba::from(bg);
    let lin = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b);
    if luminance > 0.18 {
        rgb(0x1f1e1c).into()
    } else {
        rgb(0xf4f3f0).into()
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::geometry::{Percentage, Points};

    use super::*;

    fn image(src: &str) -> Inline {
        Inline::Image(Image::new(src))
    }

    #[test]
    fn a_terminals_table_columns_are_their_widest_cells_and_spans_widen_the_last() {
        let cell = |t: &str| super::super::TableCell::new(vec![Inline::Text { text: t.into() }]);
        let mut wide = cell("A cell spanning both");
        wide.colspan = Some(2);
        let table = Table {
            head: vec![cell("Platform"), cell("Views")],
            rows: vec![vec![cell("iOS"), cell("UIKit")], vec![wide]],
            ..Table::default()
        };
        let widths = column_cells(&table, |l| l.chars().count());
        // "Platform" 8; "Views" 5, widened so the span's 20 fit 8 + 3 + 9.
        assert_eq!(widths, vec![8, 9]);
        assert_eq!(table_rule(&[2, 1], ['┌', '┬', '┐']), "┌────┬───┐");
    }

    #[test]
    fn content_of_images_alone_is_shown_as_blocks() {
        let link = |content| Inline::Link {
            url: "https://x.y".into(),
            title: None,
            content,
        };
        let content = [image("a.png"), link(vec![image("b.png")])];
        let images = only_images(&content).unwrap();
        let found: Vec<_> = images.iter().map(|(i, l)| (i.src.as_str(), *l)).collect();
        assert_eq!(found, [("a.png", None), ("b.png", Some("https://x.y"))]);
        // Text, even a space, puts the images in its lines, as gpui-kit's
        // text flow does.
        assert!(only_images(&[image("a.png"), Inline::text(" "), image("b.png")]).is_none());
        assert!(only_images(&[link(vec![image("a.png"), Inline::text("x")])]).is_none());
        assert!(only_images(&[]).is_none());
    }

    #[test]
    fn images_in_text_are_sized_as_gpui_kits_flow_sizes_them() {
        let pt = |p: f32| Some(ImageLength::Points(Points(p)));
        let size = |w: f32, h: f32| Size::new(px(w), px(h));
        let line = 24.0;
        // Both given.
        assert_eq!(
            inline_image_size(pt(32.0), pt(20.0), Some(2.0), line),
            size(32.0, 20.0)
        );
        // One given: the other from the aspect ratio, a line until known.
        assert_eq!(
            inline_image_size(pt(40.0), None, Some(2.0), line),
            size(40.0, 20.0)
        );
        assert_eq!(
            inline_image_size(pt(40.0), None, None, line),
            size(40.0, 24.0)
        );
        assert_eq!(
            inline_image_size(None, pt(10.0), Some(2.0), line),
            size(20.0, 10.0)
        );
        assert_eq!(
            inline_image_size(None, pt(10.0), None, line),
            size(10.0, 10.0)
        );
        // Neither: three quarters of the line tall.
        assert_eq!(
            inline_image_size(None, None, Some(2.0), line),
            size(36.0, 18.0)
        );
        assert_eq!(inline_image_size(None, None, None, line), size(18.0, 18.0));
        // A percentage is of the line's height.
        let half = Some(ImageLength::Fraction(Percentage(0.5)));
        assert_eq!(inline_image_size(half, half, None, line), size(12.0, 12.0));
        // Never below a point.
        assert_eq!(
            inline_image_size(pt(0.0), pt(0.0), None, line),
            size(1.0, 1.0)
        );
    }

    /// Render props take closures of the value they render, events their
    /// data, as the setters of other props take their values (built, not
    /// run: an element is made in a component's reactive scope; the core's
    /// tests render a render prop set so).
    #[test]
    fn builds_with_render_closures_and_listeners() {
        let _build = || {
            text_view()
                .markdown("# Title")
                .selectable(true)
                .render_image(|image: &Image| text(image.alt.clone()))
                .render_code_block(|code: &CodeBlock| text(code.code.clone()))
                .on_link_press(|_event: &TextViewLinkPressEvent| {})
                .into_element()
        };
    }
}
