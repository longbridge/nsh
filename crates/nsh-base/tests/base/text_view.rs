//! Typed root parts reach the document's text before classes and styles;
//! inline links are stops of Tab, pressed with Return.

use nsh_base::{
    TextViewClasses, TextViewLinkPressEvent, TextViewPart, TextViewParts, TextViewStyles, text_view,
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
