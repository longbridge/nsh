//! A mounted tree as static HTML: [`Document`] applies an app's mount
//! batches as a platform would, keeping what a page shows (its views'
//! kinds, styles, texts, images and props), and [`Document::page`] writes it
//! as markup a browser lays out at any width, with each view's layout as
//! CSS (`nsh_core::Core::layout_css`) instead of the frames the core
//! computed for one viewport.

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    sync::Arc,
};

use nsh_core::{
    AccessibilityRole, AttributedText, MountItem, NodeId, ROOT_ID, ResizeMode, TextAlign, ViewKind,
    ViewProps, ViewStyle,
};

use crate::css::{css_color, object_fit, text_css, view_css};

/// What every page needs besides its views' own classes: views in the
/// flow, the document elements without the browser's look, borders over
/// their views, texts wrapped by the browser. Each rule is under the
/// page's root (`nsh-flow`), so the web runtime's own rules for the same
/// classes (views at their frames), which come once its script runs, do
/// not move the page before the app replaces it.
const BASE_CSS: &str = "\
.nsh-flow{position:relative;display:flex;flex-direction:column;width:100%;height:100%;overflow:clip;-webkit-font-smoothing:antialiased;}
.nsh-flow,.nsh-flow *{box-sizing:border-box;}
.nsh-flow .nsh-v{position:relative;margin:0;padding:0;min-width:0;min-height:0;}
.nsh-flow :is(h1,h2,h3,h4,h5,h6,p,a,ul,ol,li,strong,em,blockquote,figure,figcaption,time,address,nav,header,footer,main,article,section,aside).nsh-v{display:block;font:inherit;color:inherit;text-decoration:none;list-style:none;}
.nsh-flow .nsh-scroll{overflow:auto;}
.nsh-flow .nsh-border{position:absolute;inset:0;pointer-events:none;}
.nsh-flow .nsh-run{position:static;white-space:pre-wrap;}
.nsh-flow .nsh-nowrap>.nsh-run{white-space:pre;}
.nsh-flow .nsh-fill{position:absolute;inset:0;width:100%;height:100%;}
";

/// A view of the tree.
#[derive(Default)]
struct Node {
    kind: Option<ViewKind>,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    style: Option<Box<ViewStyle>>,
    text: Option<Arc<AttributedText>>,
    props: Option<Box<ViewProps>>,
    label: Option<Arc<str>>,
    image: Option<(Arc<str>, ResizeMode)>,
    /// A page's header view (`ScreenConfig::header_view`), shown first.
    header: Option<NodeId>,
    /// A native component's name and props (`nsh/head`'s among them).
    component: Option<Arc<str>>,
    component_props: BTreeMap<Arc<str>, nsh_core::JsonValue>,
}

/// The component `nsh/web`'s `Head` is in an app that is a page of a site
/// (`nsh render`'s; `nsh-js` `web_page.rs`).
const HEAD: &str = "nsh/head";

/// What the mounted `Head`s set ([`Document::head`]).
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct Head {
    /// The document's title.
    pub title: Option<String>,
    /// The page's summary.
    pub description: Option<String>,
    /// Its canonical URL or path, as the app wrote it.
    pub canonical: Option<String>,
    /// Its language.
    pub lang: Option<String>,
    /// More `<meta>`s, each `{ name | property, content }`.
    pub meta: Vec<nsh_core::JsonValue>,
    /// Structured data (JSON-LD).
    pub json_ld: Option<nsh_core::JsonValue>,
}

/// A page written from a [`Document`].
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct Page {
    /// The root view's markup.
    pub html: String,
    /// The rules it needs: the base rules and its views' classes.
    pub css: String,
    /// The `href`s of its links, in document order.
    pub links: Vec<String>,
}

/// The views an app mounted, from its mount batches.
#[non_exhaustive]
pub struct Document {
    nodes: HashMap<NodeId, Node>,
}

impl Default for Document {
    /// An empty document: only the root view.
    fn default() -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(
            ROOT_ID,
            Node {
                kind: Some(ViewKind::View),
                ..Node::default()
            },
        );
        Self { nodes }
    }
}

impl Document {
    /// An empty document: only the root view.
    pub fn new() -> Self {
        Self::default()
    }

    fn node(&mut self, id: NodeId) -> &mut Node {
        self.nodes.entry(id).or_default()
    }

    /// Applies one mount item; what a static page does not show (frames,
    /// events, animations, scroll offsets) is left out.
    pub fn apply(&mut self, item: &MountItem) {
        match item {
            MountItem::Create { id, kind } => {
                self.nodes.insert(
                    *id,
                    Node {
                        kind: Some(*kind),
                        ..Node::default()
                    },
                );
            }
            MountItem::Insert {
                parent,
                child,
                before,
            } => {
                self.detach(*child);
                self.node(*child).parent = Some(*parent);
                // A header is its page's (`ScreenConfig`), not a child.
                if self.nodes.get(child).and_then(|n| n.kind) == Some(ViewKind::HeaderItem) {
                    return;
                }
                let children = &mut self.node(*parent).children;
                let at = before
                    .and_then(|b| children.iter().position(|c| *c == b))
                    .unwrap_or(children.len());
                children.insert(at, *child);
            }
            MountItem::Remove { id } => self.detach(*id),
            MountItem::Delete { id } => {
                self.detach(*id);
                self.nodes.remove(id);
            }
            MountItem::ViewStyle { id, style } => self.node(*id).style = Some(style.clone()),
            MountItem::Text { id, text, .. } => self.node(*id).text = Some(text.clone()),
            MountItem::ViewProps { id, props } => self.node(*id).props = Some(props.clone()),
            MountItem::AccessibilityLabel { id, label } => self.node(*id).label = label.clone(),
            MountItem::Image {
                id,
                source,
                resize_mode,
                ..
            } => {
                self.node(*id).image = source.as_ref().map(|s| (s.uri.clone(), *resize_mode));
            }
            MountItem::ScreenConfig { id, config } => self.node(*id).header = config.header_view,
            MountItem::Component { id, name } => self.node(*id).component = Some(name.clone()),
            MountItem::ComponentProps { id, props } => {
                let node = self.node(*id);
                for (name, value) in props {
                    match value {
                        Some(v) => node.component_props.insert(name.clone(), v.clone()),
                        None => node.component_props.remove(name),
                    };
                }
            }
            _ => {}
        }
    }

    fn detach(&mut self, id: NodeId) {
        if let Some(parent) = self.nodes.get(&id).and_then(|n| n.parent)
            && let Some(p) = self.nodes.get_mut(&parent)
        {
            p.children.retain(|c| *c != id);
        }
    }

    /// The ids of the views a page of `root` would show, for their layout
    /// (`EngineControl::layout_css`).
    pub fn views(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.visit(ROOT_ID, &mut |id| out.push(id));
        out
    }

    /// The children a page shows of `id`: a stack's top page only, a page's
    /// header first.
    fn shown_children(&self, id: NodeId) -> Vec<NodeId> {
        let Some(node) = self.nodes.get(&id) else {
            return Vec::new();
        };
        let mut children: Vec<NodeId> = node
            .children
            .iter()
            .copied()
            .filter(|c| {
                // Dialogs are what the app opens later, not the URL's page.
                self.nodes.get(c).and_then(|n| n.kind) != Some(ViewKind::Modal)
            })
            .collect();
        if node.kind == Some(ViewKind::Stack) {
            children.retain(|c| self.nodes.get(c).and_then(|n| n.kind) == Some(ViewKind::Screen));
            children = children.last().copied().into_iter().collect();
        }
        if let Some(header) = node.header.filter(|h| self.nodes.contains_key(h)) {
            children.retain(|c| *c != header);
            children.insert(0, header);
        }
        children
    }

    fn visit(&self, id: NodeId, f: &mut impl FnMut(NodeId)) {
        f(id);
        for child in self.shown_children(id) {
            self.visit(child, f);
        }
    }

    /// What the `Head`s of the page set: those it shows (not those of the
    /// pages under a stack's top one), in document order, a later one
    /// winning for each field it sets.
    pub fn head(&self) -> Head {
        let mut head = Head::default();
        let mut meta: Vec<(String, nsh_core::JsonValue)> = Vec::new();
        self.visit(ROOT_ID, &mut |id| {
            let Some(node) = self.nodes.get(&id) else {
                return;
            };
            if node.component.as_deref() != Some(HEAD) {
                return;
            }
            let get = |key: &str| node.component_props.get(key).filter(|v| !v.is_null());
            let text = |key: &str| get(key).and_then(|v| v.as_str()).map(str::to_owned);
            for (field, key) in [
                (&mut head.title, "title"),
                (&mut head.description, "description"),
                (&mut head.canonical, "canonical"),
                (&mut head.lang, "lang"),
            ] {
                if let Some(v) = text(key) {
                    *field = Some(v);
                }
            }
            if let Some(v) = get("jsonLd") {
                head.json_ld = Some(v.clone());
            }
            for m in get("meta").and_then(|v| v.as_array()).into_iter().flatten() {
                // One without a content sets nothing and replaces none, as
                // on the web (`web/browser/page.js`).
                if m.get("content").is_none_or(|c| c.is_null()) {
                    continue;
                }
                let key = match (m.get("property"), m.get("name")) {
                    (Some(p), _) => format!("property:{}", p.as_str().unwrap_or("")),
                    (None, Some(n)) => format!("name:{}", n.as_str().unwrap_or("")),
                    _ => continue,
                };
                meta.retain(|(k, _)| *k != key);
                meta.push((key, m.clone()));
            }
        });
        head.meta = meta.into_iter().map(|(_, m)| m).collect();
        head
    }

    /// The page: the root view's markup, each view laid out with its
    /// `layouts` entry (its layout as CSS).
    pub fn page(&self, layouts: &HashMap<NodeId, String>) -> Page {
        let mut w = Writer {
            doc: self,
            layouts,
            classes: BTreeMap::new(),
            links: Vec::new(),
            html: String::new(),
        };
        w.view(ROOT_ID);
        let mut css = String::from(BASE_CSS);
        for (text, name) in &w.classes {
            let _ = writeln!(css, ".{name}{{{text}}}");
        }
        Page {
            html: w.html,
            css,
            links: w.links,
        }
    }
}

struct Writer<'a> {
    doc: &'a Document,
    layouts: &'a HashMap<NodeId, String>,
    /// CSS text → class name: each distinct style once.
    classes: BTreeMap<String, String>,
    links: Vec<String>,
    html: String,
}

impl Writer<'_> {
    fn class(&mut self, css: &str) -> String {
        if let Some(name) = self.classes.get(css) {
            return name.clone();
        }
        let name = format!("nsh-s{}", self.classes.len());
        self.classes.insert(css.to_owned(), name.clone());
        name
    }

    fn view(&mut self, id: NodeId) {
        let Some(node) = self.doc.nodes.get(&id) else {
            return;
        };
        if node.component.as_deref() == Some(HEAD) {
            return;
        }
        let kind = node.kind.unwrap_or(ViewKind::View);
        let props = node.props.as_deref();
        let tag = props
            .filter(|_| matches!(kind, ViewKind::View | ViewKind::Text))
            .map_or(Cow::Borrowed("div"), element_tag);
        let mut classes = vec![if id == ROOT_ID { "nsh-flow" } else { "nsh-v" }.to_owned()];
        if kind == ViewKind::Scroll {
            classes.push("nsh-scroll".into());
        }
        let mut border = None;
        if let Some(style) = node.style.as_deref() {
            let (css, b) = view_css(style);
            if !css.is_empty() {
                classes.push(self.class(&css));
            }
            border = b.map(|b| self.class(&b));
        }
        let mut style = if id == ROOT_ID {
            String::new()
        } else {
            self.layouts.get(&id).cloned().unwrap_or_default()
        };
        // Pages and their headers take the size the platform gave the core
        // (the window's): here, their stack's. A page's header is in the flow
        // above its content, which made room for it on top.
        if matches!(kind, ViewKind::Screen | ViewKind::HeaderItem) {
            style.push_str("width:100%;");
        }
        if kind == ViewKind::Screen {
            style.push_str("height:100%;");
            if node.header.is_some() {
                style.push_str("padding-top:0;");
            }
        }
        let text = node.text.as_deref();
        if let Some(text) = text {
            // A text out of the layout (`display: none`, which the core
            // still mounts with its text) stays out: the displays below
            // must not show it again.
            let shown = !style.contains("display:none;");
            // Lines as tall as the runs make them: the element's own line
            // (the strut) adds nothing.
            if shown {
                style.push_str("display:block;");
            }
            style.push_str("font-size:0;line-height:0;");
            if let Some(align) = text_align(text.align, text.rtl) {
                let _ = write!(style, "text-align:{align};");
            }
            if text.no_wrap {
                classes.push("nsh-nowrap".into());
            }
            // `numberOfLines` / `line-clamp-*`: the browser cuts the lines it
            // wraps, as the core cut the ones it laid out.
            if text.max_lines > 0 && shown {
                let _ = write!(
                    style,
                    "display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:{};overflow:hidden;",
                    text.max_lines
                );
            }
            if let Some(color) = text.uniform_color() {
                let _ = write!(style, "color:{};", css_color(color));
            }
        }
        let _ = write!(self.html, "<{tag} class=\"{}\"", classes.join(" "));
        if !style.is_empty() {
            let _ = write!(self.html, " style=\"{}\"", attr(&style));
        }
        if let Some(href) = props.and_then(|p| p.href.as_deref()) {
            let _ = write!(self.html, " href=\"{}\"", attr(href));
            self.links.push(href.to_owned());
        }
        let label = props
            .and_then(|p| p.accessibility.label.clone())
            .or_else(|| node.label.clone());
        if let Some(label) = label.as_deref().filter(|_| text.is_none()) {
            let _ = write!(self.html, " aria-label=\"{}\"", attr(label));
        }
        self.html.push('>');
        if let Some(text) = text {
            self.runs(text);
        }
        if let Some((uri, mode)) = &node.image {
            let _ = write!(
                self.html,
                "<img class=\"nsh-fill\" src=\"{}\" alt=\"{}\" style=\"object-fit:{}\">",
                attr(uri),
                attr(label.as_deref().unwrap_or("")),
                object_fit(*mode)
            );
        }
        for child in self.doc.shown_children(id) {
            self.view(child);
        }
        if let Some(border) = border {
            let _ = write!(self.html, "<div class=\"nsh-border {border}\"></div>");
        }
        let _ = write!(self.html, "</{tag}>");
    }

    /// A text's style runs, each a span the browser wraps.
    fn runs(&mut self, text: &AttributedText) {
        let units: Vec<u16> = text.text.encode_utf16().collect();
        let uniform = text.uniform_color().is_some();
        for span in &text.spans {
            let (start, end) = (span.start as usize, (span.end as usize).min(units.len()));
            if start >= end {
                continue;
            }
            let run = String::from_utf16_lossy(&units[start..end]);
            let class = self.class(&text_css(&span.style, text.shadow.as_ref(), uniform));
            let line = if span.style.line_height.0 > 0.0 {
                format!("{}px", span.style.line_height.0)
            } else {
                "normal".into()
            };
            let _ = write!(
                self.html,
                "<span class=\"nsh-run {class}\" style=\"line-height:{line}\">{}</span>",
                escape(&run)
            );
        }
    }
}

fn text_align(align: TextAlign, rtl: bool) -> Option<&'static str> {
    Some(match align {
        TextAlign::Auto => return rtl.then_some("right"),
        TextAlign::Left => "left",
        TextAlign::Right => "right",
        TextAlign::Center => "center",
        TextAlign::Justify => "justify",
    })
}

/// Text as HTML.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// An attribute's value as HTML (in double quotes).
fn attr(s: &str) -> String {
    escape(s).replace('"', "&quot;")
}

/// The HTML element a view or text is drawn as, as the web runtime draws
/// it (`nsh.js` `elementTag`): the element `web={{ element }}` names, else a
/// heading for `role="header"` (`h1` to `h6` by its level, `h2` without
/// one), a link for `role="link"` with an `href` (a `Link`'s), else a
/// `div`.
fn element_tag(props: &ViewProps) -> Cow<'static, str> {
    if let Some(element) = props.web_element {
        return Cow::Borrowed(nsh_core::element::PropEnum::js_name(&element));
    }
    match props.accessibility.role {
        AccessibilityRole::Header => Cow::Owned(format!(
            "h{}",
            props.accessibility.level.unwrap_or(2).clamp(1, 6)
        )),
        AccessibilityRole::Link if props.href.is_some() => Cow::Borrowed("a"),
        _ => Cow::Borrowed("div"),
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::{JsonValue, WebElement};

    use super::*;

    fn props(
        element: Option<WebElement>,
        role: AccessibilityRole,
        href: Option<&str>,
    ) -> Box<ViewProps> {
        let mut props = ViewProps {
            web_element: element,
            href: href.map(Arc::from),
            ..ViewProps::default()
        };
        props.accessibility.role = role;
        Box::new(props)
    }

    #[test]
    fn headings_and_links_come_from_their_role_and_the_rest_from_web_element() {
        let tag = |element, role, level, href: Option<&str>| {
            let mut p = props(element, role, href);
            p.accessibility.level = level;
            element_tag(&p).into_owned()
        };
        assert_eq!(tag(None, AccessibilityRole::Header, Some(1), None), "h1");
        assert_eq!(tag(None, AccessibilityRole::Header, None, None), "h2");
        assert_eq!(tag(None, AccessibilityRole::Header, Some(9), None), "h6");
        assert_eq!(tag(None, AccessibilityRole::Link, None, Some("/")), "a");
        assert_eq!(tag(None, AccessibilityRole::Link, None, None), "div");
        assert_eq!(
            tag(Some(WebElement::Nav), AccessibilityRole::None, None, None),
            "nav"
        );
        assert_eq!(
            tag(
                Some(WebElement::P),
                AccessibilityRole::Header,
                Some(1),
                None
            ),
            "p"
        );
    }

    #[test]
    fn views_are_written_as_their_document_elements_with_their_layout() {
        let mut doc = Document::new();
        for item in [
            MountItem::Create {
                id: 2,
                kind: ViewKind::View,
            },
            MountItem::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            MountItem::ViewProps {
                id: 2,
                props: props(Some(WebElement::Nav), AccessibilityRole::None, None),
            },
            MountItem::Create {
                id: 4,
                kind: ViewKind::View,
            },
            MountItem::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            MountItem::ViewProps {
                id: 4,
                props: props(None, AccessibilityRole::Link, Some("/notes/a")),
            },
        ] {
            doc.apply(&item);
        }
        assert_eq!(doc.views(), vec![ROOT_ID, 2, 4]);
        let layouts = HashMap::from([(4, "display:flex;".to_owned())]);
        let page = doc.page(&layouts);
        assert!(page.html.contains("<nav class=\"nsh-v\">"), "{}", page.html);
        assert!(
            page.html
                .contains("<a class=\"nsh-v\" style=\"display:flex;\" href=\"/notes/a\"></a>"),
            "{}",
            page.html
        );
        assert_eq!(page.links, ["/notes/a"]);
    }

    /// A text view `id` under the root with `runs`, each a text and its
    /// line height.
    fn text_view(doc: &mut Document, id: NodeId, runs: &[(&str, f32)], max_lines: u32) {
        let mut text = AttributedText {
            max_lines,
            ..AttributedText::default()
        };
        for (run, line_height) in runs {
            let start = text.text.encode_utf16().count() as u32;
            text.text.push_str(run);
            text.spans.push(nsh_core::TextSpan {
                start,
                end: text.text.encode_utf16().count() as u32,
                style: nsh_core::ResolvedTextStyle {
                    line_height: nsh_core::Points(*line_height),
                    ..Default::default()
                },
            });
        }
        for item in [
            MountItem::Create {
                id,
                kind: ViewKind::Text,
            },
            MountItem::Insert {
                parent: ROOT_ID,
                child: id,
                before: None,
            },
            MountItem::Text {
                id,
                text: Arc::new(text),
                layout: nsh_core::TextLayout(Arc::new(())),
                content: Default::default(),
            },
        ] {
            doc.apply(&item);
        }
    }

    #[test]
    fn a_text_is_its_runs_escaped_each_with_its_line_height() {
        let mut doc = Document::new();
        // A surrogate pair before the second run: spans count UTF-16 units.
        text_view(
            &mut doc,
            2,
            &[("1 < 2 & \"三\" 😀", 20.0), (" $& >", 0.0)],
            0,
        );
        let page = doc.page(&HashMap::from([(2, "display:flex;".to_owned())]));
        assert!(
            page.html.contains(
                "<span class=\"nsh-run nsh-s0\" style=\"line-height:20px\">1 &lt; 2 &amp; \"三\" 😀</span>\
                 <span class=\"nsh-run nsh-s0\" style=\"line-height:normal\"> $&amp; &gt;</span>"
            ),
            "{}",
            page.html
        );
        // The element's own line adds nothing; the runs share one class.
        assert!(
            page.html.contains(
                "style=\"display:flex;display:block;font-size:0;line-height:0;color:#000000;\""
            ),
            "{}",
            page.html
        );
        assert!(page.css.contains("\n.nsh-s0{"), "{}", page.css);
        assert!(!page.css.contains(".nsh-s1{"), "{}", page.css);
    }

    #[test]
    fn a_text_out_of_the_layout_stays_hidden() {
        let mut doc = Document::new();
        text_view(&mut doc, 2, &[("✓", 0.0)], 0);
        text_view(&mut doc, 4, &[("clamped", 0.0)], 2);
        let hidden = "display:none;position:relative;".to_owned();
        let page = doc.page(&HashMap::from([(2, hidden.clone()), (4, hidden)]));
        // Nothing after the layout's `display: none` shows the text again.
        assert_eq!(page.html.matches("display:").count(), 2, "{}", page.html);
        assert!(!page.html.contains("display:block"), "{}", page.html);
        assert!(!page.html.contains("-webkit-box"), "{}", page.html);
        // Shown, a clamped text is cut by the browser.
        let page = doc.page(&HashMap::from([(4, "display:flex;".to_owned())]));
        assert!(
            page.html
                .contains("display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:2;"),
            "{}",
            page.html
        );
    }

    #[test]
    fn an_image_is_an_img_with_its_label_and_fit_and_attributes_are_escaped() {
        let mut doc = Document::new();
        for item in [
            MountItem::Create {
                id: 2,
                kind: ViewKind::Image,
            },
            MountItem::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            MountItem::Image {
                id: 2,
                source: Some(nsh_core::ImageSource {
                    uri: Arc::from("https://img.example.com/a.png?w=1&h=\"2\""),
                    size: None,
                    scales: Default::default(),
                    data: None,
                }),
                resize_mode: ResizeMode::Contain,
                options: Default::default(),
            },
            MountItem::AccessibilityLabel {
                id: 2,
                label: Some(Arc::from("A \"chart\" <1>")),
            },
        ] {
            doc.apply(&item);
        }
        let page = doc.page(&HashMap::new());
        assert!(
            page.html.contains(
                "<div class=\"nsh-v\" aria-label=\"A &quot;chart&quot; &lt;1&gt;\">\
                 <img class=\"nsh-fill\" src=\"https://img.example.com/a.png?w=1&amp;h=&quot;2&quot;\" \
                 alt=\"A &quot;chart&quot; &lt;1&gt;\" style=\"object-fit:contain\"></div>"
            ),
            "{}",
            page.html
        );
        // A cleared source leaves the view.
        doc.apply(&MountItem::Image {
            id: 2,
            source: None,
            resize_mode: ResizeMode::Cover,
            options: Default::default(),
        });
        assert!(!doc.page(&HashMap::new()).html.contains("<img"));
    }

    #[test]
    fn a_default_document_is_a_new_one_with_its_root() {
        for doc in [Document::default(), Document::new()] {
            assert_eq!(doc.views(), vec![ROOT_ID]);
            assert_eq!(
                doc.page(&HashMap::new()).html,
                "<div class=\"nsh-flow\"></div>"
            );
        }
    }

    #[test]
    fn a_meta_without_a_content_sets_nothing_and_replaces_none() {
        let mut doc = Document::new();
        let head = |id: NodeId, meta: JsonValue| {
            [
                MountItem::Create {
                    id,
                    kind: ViewKind::Native,
                },
                MountItem::Component {
                    id,
                    name: Arc::from(HEAD),
                },
                MountItem::ComponentProps {
                    id,
                    props: vec![(Arc::from("meta"), Some(meta))],
                },
                MountItem::Insert {
                    parent: ROOT_ID,
                    child: id,
                    before: None,
                },
            ]
        };
        let robots = |content: JsonValue| -> JsonValue {
            JsonValue::Object(
                [("name", JsonValue::from("robots")), ("content", content)]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v))
                    .collect(),
            )
        };
        let first = JsonValue::from(vec![robots("noindex".into()), robots("index".into())]);
        let second = JsonValue::from(vec![robots(JsonValue::Null)]);
        for item in head(2, first).into_iter().chain(head(4, second)) {
            doc.apply(&item);
        }
        // The later one of a name wins; one without a content is not one.
        assert_eq!(doc.head().meta, vec![robots("index".into())]);
    }

    #[test]
    fn a_stack_shows_its_top_page_and_its_heads_the_later_one_winning() {
        let mut doc = Document::new();
        let head = |id: NodeId, title: &str| {
            let props = vec![(Arc::from("title"), Some(JsonValue::from(title)))];
            [
                MountItem::Create {
                    id,
                    kind: ViewKind::Native,
                },
                MountItem::Component {
                    id,
                    name: Arc::from(HEAD),
                },
                MountItem::ComponentProps { id, props },
            ]
        };
        for item in [
            MountItem::Create {
                id: 2,
                kind: ViewKind::Stack,
            },
            MountItem::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            MountItem::Create {
                id: 4,
                kind: ViewKind::Screen,
            },
            MountItem::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            MountItem::Create {
                id: 6,
                kind: ViewKind::Screen,
            },
            MountItem::Insert {
                parent: 2,
                child: 6,
                before: None,
            },
        ] {
            doc.apply(&item);
        }
        for item in head(8, "Home").into_iter().chain(head(10, "Note")) {
            doc.apply(&item);
        }
        doc.apply(&MountItem::Insert {
            parent: 4,
            child: 8,
            before: None,
        });
        doc.apply(&MountItem::Insert {
            parent: 6,
            child: 10,
            before: None,
        });
        // The page under the top one is not this URL's.
        assert_eq!(doc.views(), vec![ROOT_ID, 2, 6, 10]);
        let head = doc.head();
        assert_eq!(head.title.as_deref(), Some("Note"));
        // A head takes no room in the page.
        assert!(
            !doc.page(&HashMap::new())
                .html
                .contains("nsh-v\" style=\"\"")
        );
    }
}
