//! Typed root parts reach the document's text before classes and styles;
//! inline links are stops of Tab, pressed with Return; a link's press
//! reports the modifier keys held.

use nsh_base::{
    TextViewClasses, TextViewLinkPressEvent, TextViewPart, TextViewParts, TextViewStyles,
    TextViewTaskPressEvent, text_view,
};
use nsh_core::{
    StyleKey,
    element::{Root, Style},
    platform::{Idiom, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

#[component]
fn AnnotatedCode() -> impl IntoElement {
    use nsh_base::text_view::{
        Block, CodeBlock, CustomNode, Document, Inline, TextViewCodeAction, TextViewCodeActionEvent,
    };
    // Annotations are custom nodes shown as their content: spans of the
    // code's text, styled through the `custom` part.
    provide_context(TextViewParts::new().part(TextViewPart::Custom, |s| {
        s.font_weight(FontWeight::SEMIBOLD)
    }));
    let mut code = CodeBlock::new(None, "prefix 日本語 suffix");
    code.id = Some("annotated".into());
    code.wrap = true;
    code.content = vec![
        Inline::text("prefix "),
        Inline::Custom(CustomNode::new("mark").with_text("日本語")),
        Inline::text(" suffix"),
    ];
    let mut fallback = CodeBlock::new(None, "literal <tag>");
    fallback.id = Some("fallback".into());
    fallback.content = vec![Inline::text("replacement")];
    let pressed = signal(String::new());
    view()
        .child(
            text_view()
                .document(Document::new(vec![
                    Block::CodeBlock(code),
                    Block::CodeBlock(fallback),
                ]))
                .code_block_actions(vec![TextViewCodeAction::new("export").with_label("Export")])
                .on_code_block_action(move |e: &TextViewCodeActionEvent| {
                    pressed.set(format!(
                        "{} {}: {}",
                        e.id,
                        e.code.id.as_deref().unwrap_or_default(),
                        e.code.code
                    ));
                }),
        )
        .child(text(move || format!("Pressed {}", pressed.get())))
}

#[test]
fn annotated_code_shows_its_spans_and_hands_its_block_to_actions() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<AnnotatedCode>())).unwrap();
    app.wait_for("prefix 日本語 suffix").unwrap();
    assert_eq!(
        app.screen().span_style("text:日本語").unwrap().font_weight,
        600
    );
    assert_eq!(
        app.screen().span_style("text:prefix").unwrap().font_weight,
        400
    );
    app.press("label:Export >> nth=0").unwrap();
    app.wait_for("Pressed export annotated: prefix 日本語 suffix")
        .unwrap();
}

#[test]
fn code_content_that_does_not_spell_the_code_is_ignored() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<AnnotatedCode>())).unwrap();
    app.wait_for("literal <tag>").unwrap();
    assert!(!app.shows("replacement"));
}

#[component]
fn TypedRoot() -> impl IntoElement {
    provide_context(TextViewParts::new().part(TextViewPart::Root, |s| s.text_size(px(11.0))));
    text_view().markdown("Hello\n\nAnother paragraph")
}

#[component]
fn RootOverrides() -> impl IntoElement {
    provide_context(TextViewParts::new().part(TextViewPart::Root, |s| s.text_size(px(11.0))));
    let classes = TextViewClasses {
        root: Some("text-[18px]".into()),
        ..Default::default()
    };
    let styles = TextViewStyles {
        root: Some(
            Style::default()
                .with(
                    StyleKey::FontSize,
                    StyleKey::FontSize.number_value(23.0).unwrap(),
                )
                .into(),
        ),
        ..Default::default()
    };
    view()
        .child(text_view().markdown("Class wins").classes(classes.clone()))
        .child(
            text_view()
                .markdown("Style wins")
                .classes(classes)
                .styles(styles),
        )
}

#[component]
fn DefaultRoot() -> impl IntoElement {
    text_view().markdown("Default text")
}

#[test]
fn typed_root_styles_are_inherited_by_paragraphs() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<TypedRoot>())).unwrap();
    app.wait_for("Another paragraph").unwrap();
    for text in ["Hello", "Another paragraph"] {
        assert_eq!(app.screen().span_style(text).unwrap().font_size, px(11.0));
    }
}

#[test]
fn root_classes_override_typed_parts_and_inline_styles_override_classes() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<RootOverrides>())).unwrap();
    app.wait_for("Style wins").unwrap();
    assert_eq!(
        app.screen().span_style("Class wins").unwrap().font_size,
        px(18.0)
    );
    assert_eq!(
        app.screen().span_style("Style wins").unwrap().font_size,
        px(23.0)
    );
}

#[test]
fn root_without_typed_parts_keeps_the_default_typography() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<DefaultRoot>())).unwrap();
    app.wait_for("Default text").unwrap();
    assert_eq!(
        app.screen().span_style("Default text").unwrap().font_size,
        px(15.0)
    );
}

/// A paragraph with two links, and a stop after it; what the links
/// reported and where the focus came, on screen.
#[component]
fn Links() -> impl IntoElement {
    let pressed = signal(Vec::<String>::new());
    let after = signal(false);
    view()
        .child(
            text_view()
                .markdown("Read [the guide](https://example.com/guide) or [the notes](#notes).")
                .on_link_press(move |e: &TextViewLinkPressEvent| {
                    let url = e.url.to_string();
                    pressed.update(|p| p.push(url));
                }),
        )
        .child(
            view()
                .focusable(true)
                .on_focus(move || after.set(true))
                .child(text("After")),
        )
        .child(text(move || {
            format!("Pressed: {}", pressed.get().join(" "))
        }))
        .child(text(move || format!("After focused: {}", after.get())))
}

#[test]
fn tab_reaches_a_documents_links_in_order_and_return_presses_them() {
    let mut config = EngineConfig::new(Root::of::<Links>());
    config.environment.os = Os::MacOs;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).unwrap();
    app.wait_for("the guide").unwrap();
    app.press_key("tab").unwrap();
    app.press_key("enter").unwrap();
    app.wait_for("Pressed: https://example.com/guide").unwrap();
    app.press_key("tab").unwrap();
    app.press_key("enter").unwrap();
    app.wait_for("Pressed: https://example.com/guide #notes")
        .unwrap();
    assert!(app.shows("After focused: false"), "the links came first");
    app.press_key("tab").unwrap();
    app.wait_for("After focused: true").unwrap();
    // Back from the stop after it: the last link.
    app.press_key("shift+tab").unwrap();
    app.press_key("enter").unwrap();
    app.wait_for("Pressed: https://example.com/guide #notes #notes")
        .unwrap();
}

/// A link with a title, a long link and a link to a heading of the
/// document, in one paragraph.
#[component]
fn HoveredLinks() -> impl IntoElement {
    text_view().markdown(
        "See [the guide](https://docs.example.com/guide \"Read the guide\"), [the archive](https://example.com/archive/2026/11/09/a-very-long-article-name-that-goes-on-and-on?utm_source=newsletter&page=2) or [the notes](#notes).",
    )
}

/// The archive's link: a path longer than a hint's line.
const LONG: &str = "https://example.com/archive/2026/11/09/a-very-long-article-name-that-goes-on-and-on?utm_source=newsletter&page=2";

fn desktop<C: Component<Props = ()>>() -> Headless {
    let mut config = EngineConfig::new(Root::of::<C>());
    config.clock = nsh_engine::Clock::manual(std::time::SystemTime::UNIX_EPOCH);
    config.environment.os = Os::Linux;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).unwrap();
    app.first_render().unwrap();
    app.wait(std::time::Duration::from_millis(32)).unwrap();
    app
}

#[test]
fn resting_on_a_link_shows_its_title_over_its_url() {
    let mut app = desktop::<HoveredLinks>();
    app.wait_for("the guide").unwrap();
    app.hover("the guide").unwrap();
    app.wait(std::time::Duration::from_millis(600)).unwrap();
    // One line of two parts: the scheme and host, then the rest (a URL
    // needs `text:`: its `https:` would be read as a selector's prefix).
    app.wait_for("text:https://docs.example.com").unwrap();
    assert!(app.shows("/guide"), "the path, after the host");
    assert!(app.shows("Read the guide"), "the title, above the URL");
    // Over a link to the document's own heading: the anchor shows too,
    // and the guide's hint is gone.
    app.hover("the notes").unwrap();
    app.wait(std::time::Duration::from_millis(1000)).unwrap();
    app.wait_for("text:#notes").unwrap();
    assert!(!app.shows("https://docs.example.com"));
    assert!(!app.shows("Read the guide"), "no stale title");
}

#[test]
fn a_long_links_hint_keeps_its_host_and_the_end_of_its_url() {
    let mut app = desktop::<HoveredLinks>();
    app.wait_for("the archive").unwrap();
    app.hover("the archive").unwrap();
    // 64 characters: the hint's budget.
    let (head, rest) = nsh_core::open_url::hint_url(LONG, 64).unwrap();
    assert_eq!(head, "https://example.com");
    assert!(rest.starts_with("/archive/"), "{rest}");
    assert!(rest.ends_with("page=2") && rest.contains('…'), "{rest}");
    app.wait(std::time::Duration::from_millis(600)).unwrap();
    app.wait_for(&format!("text:{head}")).unwrap();
    // `shows` compares texts as written (no selector): the rest's slashes
    // are not read as a regex.
    assert!(app.shows(&rest), "{rest}");
}

#[test]
fn a_phone_shows_no_hint_for_a_link() {
    let mut config = EngineConfig::new(Root::of::<HoveredLinks>());
    config.environment.os = Os::Web;
    config.environment.idiom = Idiom::Phone;
    let mut app = Headless::start(config).unwrap();
    app.wait_for("the guide").unwrap();
    app.hover("the guide").unwrap();
    app.wait(std::time::Duration::from_millis(1000)).unwrap();
    assert!(!app.shows("https://docs.example.com"));
}

/// An image in a link that has a title, on its own and in a line of text.
#[component]
fn LinkedImages() -> impl IntoElement {
    text_view().html(
        "<p><a href=\"https://example.com/home\" title=\"Home page\"><img src=\"https://example.com/logo.png\" alt=\"Logo\" width=\"40\" height=\"40\"></a></p>\
         <p>Go <a href=\"https://example.com/shop\"><img src=\"https://example.com/cart.png\" alt=\"Cart\" width=\"16\" height=\"16\"></a> now</p>",
    )
}

#[test]
fn resting_on_a_linked_image_shows_the_links_destination() {
    let mut app = desktop::<LinkedImages>();
    // A block image: the image taking the link's press is the anchor.
    app.wait_for("label:Logo").unwrap();
    app.hover("label:Logo").unwrap();
    app.wait(std::time::Duration::from_millis(600)).unwrap();
    app.wait_for("text:https://example.com").unwrap();
    assert!(app.shows("/home"), "the path, after the host");
    assert!(app.shows("Home page"), "the outer link's title");
    // An image in a line of text: the same, with its own link's URL.
    app.hover("label:Cart").unwrap();
    app.wait(std::time::Duration::from_millis(1000)).unwrap();
    assert!(app.shows("/shop"), "the inline image's link");
    assert!(!app.shows("/home"), "no stale destination");
    assert!(!app.shows("Home page"), "no stale title");
}

/// A link and a footnote reference; each press, with whether Command was
/// held, on screen.
#[component]
fn LinkModifiers() -> impl IntoElement {
    let pressed = signal(Vec::<String>::new());
    view()
        .child(
            text_view()
                .markdown("Read [the guide](https://example.com/guide)[^n].\n\n[^n]: A note.")
                .on_link_press(move |e: &TextViewLinkPressEvent| {
                    let held = if e.modifiers.cmd { "cmd" } else { "plain" };
                    let press = format!("{}:{held}", e.url);
                    pressed.update(|p| p.push(press));
                }),
        )
        .child(text(move || {
            format!("Pressed: {}", pressed.get().join(" "))
        }))
}

#[test]
fn a_link_press_reports_the_modifier_keys_held() {
    let mut config = EngineConfig::new(Root::of::<LinkModifiers>());
    config.environment.os = Os::MacOs;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).unwrap();
    app.wait_for("the guide").unwrap();
    app.press("the guide").unwrap();
    app.wait_for("Pressed: https://example.com/guide:plain")
        .unwrap();
    app.hold_keys("mod", true).unwrap();
    app.press("the guide").unwrap();
    app.wait_for("Pressed: https://example.com/guide:plain https://example.com/guide:cmd")
        .unwrap();
    app.press("1").unwrap();
    app.wait_for(
        "Pressed: https://example.com/guide:plain https://example.com/guide:cmd #footnote-n:cmd",
    )
    .unwrap();
    // The keyboard holds no pointer's modifiers: Return presses with none.
    app.hold_keys("mod", false).unwrap();
    app.press_key("tab").unwrap();
    app.press_key("enter").unwrap();
    app.wait_for(
        "Pressed: https://example.com/guide:plain https://example.com/guide:cmd \
         #footnote-n:cmd https://example.com/guide:plain",
    )
    .unwrap();
}

/// An image at the end of a paragraph that wraps in a narrow column.
#[component]
fn WrappedInlineImage() -> impl IntoElement {
    view().w(px(120.0)).child(text_view().html(
        "<p>A paragraph long enough to wrap onto several lines <img src=\"https://example.com/dot.png\" alt=\"Dot\" width=\"10\" height=\"10\"></p>",
    ))
}

#[test]
fn an_image_in_a_wrapped_line_is_inside_the_column() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<WrappedInlineImage>())).unwrap();
    app.wait_for("label:Dot").unwrap();
    let id = app.screen().id("label:Dot").unwrap();
    let nodes = app.screen().layout_nodes();
    let dot = nodes.iter().find(|n| n.id == id).unwrap().bounds;
    let text = nodes
        .iter()
        .find(|n| n.text.is_some())
        .expect("the paragraph")
        .bounds;
    assert!(dot.origin.x < px(120.0), "{dot:?}");
    // On the last line, not the first.
    assert!(
        dot.origin.y > text.origin.y + px(25.0),
        "{dot:?} in {text:?}"
    );
}

/// An app's own image as wide as it asks, at most its box: 400 points
/// wide in a column of 300, on its own and in a link.
fn wide_card(image: &nsh_base::text_view::Image) -> View {
    view()
        .test_id(&format!("card:{}", image.alt))
        .w(px(400.0))
        .max_w(relative(1.0))
        .h(px(40.0))
}

#[component]
fn WideCustomImages() -> impl IntoElement {
    view().w(px(300.0)).child(
        text_view()
            .markdown(
                "![Alone](https://example.com/a.png)\n\n\
                 [![Linked](https://example.com/b.png)](https://example.com)\n\n\
                 ![Full](https://example.com/d.png)\n\n\
                 ![Small](https://example.com/e.png)\n\n\
                 Text ![Inline](https://example.com/c.png) after\n\n\
                 | A | B |\n|---|---|\n| ![Cell](https://example.com/f.png) | x |",
            )
            .render_image(|image: &nsh_base::text_view::Image| {
                if image.alt == "Inline" {
                    view().test_id("card:Inline").w(px(20.0)).h(px(10.0))
                } else if image.alt == "Full" {
                    view().test_id("card:Full").w_full().h(px(40.0))
                } else if image.alt == "Cell" {
                    view().test_id("card:Cell").w(px(60.0)).h(px(20.0))
                } else if image.alt == "Small" {
                    view().test_id("card:Small").w(px(100.0)).h(px(40.0))
                } else {
                    wide_card(image)
                }
            }),
    )
}

/// A wide image of the document's own, on its own.
#[component]
fn WideImage() -> impl IntoElement {
    view().w(px(300.0)).child(text_view().html(
        "<p><img src=\"https://example.com/wide.png\" alt=\"Wide\" width=\"1200\" height=\"300\"></p>",
    ))
}

/// The bounds of the view with `test_id`.
fn bounds_of(app: &Headless, test_id: &str) -> Bounds<Points> {
    app.screen()
        .layout_nodes()
        .into_iter()
        .find(|n| n.test_id.as_deref() == Some(test_id))
        .unwrap_or_else(|| panic!("a view {test_id}"))
        .bounds
}

#[test]
fn an_apps_image_on_its_own_is_at_most_as_wide_as_the_text() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<WideCustomImages>())).unwrap();
    app.wait_for("label:Inline").unwrap();
    for card in ["card:Alone", "card:Linked"] {
        let bounds = bounds_of(&app, card);
        assert_eq!(bounds.size.width, px(300.0), "{card}: {bounds:?}");
        assert!(
            bounds.origin.x + bounds.size.width <= px(300.0),
            "{card}: {bounds:?}"
        );
    }
    // One as wide as its box is as wide as the text, a narrow one keeps
    // its width.
    assert_eq!(bounds_of(&app, "card:Full").size.width, px(300.0));
    assert_eq!(bounds_of(&app, "card:Small").size.width, px(100.0));
    // In a table's cell, as wide as it asks.
    assert_eq!(bounds_of(&app, "card:Cell").size.width, px(60.0));
    // An image in a line of text keeps the size it asks for.
    let inline = bounds_of(&app, "card:Inline");
    assert_eq!(inline.size.width, px(20.0), "{inline:?}");
    assert_eq!(inline.size.height, px(10.0), "{inline:?}");
}

#[test]
fn a_wide_image_on_its_own_is_at_most_as_wide_as_the_text() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<WideImage>())).unwrap();
    app.wait_for("label:Wide").unwrap();
    let id = app.screen().id("label:Wide").unwrap();
    let bounds = app
        .screen()
        .layout_nodes()
        .into_iter()
        .find(|n| n.id == id)
        .unwrap()
        .bounds;
    assert_eq!(bounds.size.width, px(300.0), "{bounds:?}");
    assert_eq!(
        bounds.size.height,
        px(75.0),
        "the aspect ratio kept: {bounds:?}"
    );
}

const TASKS: &str = "- [ ] first\n- [x] second\n  - [ ] nested\n\n<details>\n<summary>More</summary>\n\n\
                     - [x] hidden\n\n</details>\n\n> - [ ] last";

/// Task items at the top, nested, in a closed disclosure and in a quote
/// after it; what their boxes' presses reported.
fn tasks(pressable: bool) -> impl IntoElement {
    let pressed = signal(Vec::<String>::new());
    view()
        .child(
            text_view()
                .markdown(TASKS)
                .pressable_tasks(pressable)
                .on_task_press(move |e: &TextViewTaskPressEvent| {
                    let report = format!("{}:{}", e.index, e.checked);
                    pressed.update(|p| p.push(report));
                }),
        )
        .child(text(move || {
            format!("Pressed: {}", pressed.get().join(" "))
        }))
}

#[component]
fn PressableTasks() -> impl IntoElement {
    tasks(true)
}

#[component]
fn ShownTasks() -> impl IntoElement {
    tasks(false)
}

#[test]
fn a_pressed_task_box_reports_its_place_in_the_document_and_the_state_asked_for() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<PressableTasks>())).unwrap();
    app.wait_for("first").unwrap();
    app.press("label:second").unwrap();
    app.press("label:nested").unwrap();
    // The closed disclosure's task keeps its place before the one after it.
    app.press("label:last").unwrap();
    app.wait_for("Pressed: 1:false 2:true 4:true").unwrap();
    // Opened, closed and opened again, its task is counted from the same place.
    app.press("More").unwrap();
    app.press("label:hidden").unwrap();
    app.press("More").unwrap();
    app.press("More").unwrap();
    app.press("label:hidden").unwrap();
    app.press("label:first").unwrap();
    app.wait_for("Pressed: 1:false 2:true 4:true 3:false 3:false 0:true")
        .unwrap();
}

#[test]
fn task_boxes_only_show_their_state_unless_made_pressable() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<ShownTasks>())).unwrap();
    app.wait_for("first").unwrap();
    assert!(app.press("label:first").is_err());
    assert!(!app.shows("0:true"));
}
