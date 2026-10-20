//! The app's Rust UI: its root component, which `nsh run`, `nsh package`
//! and `cargo run` mount.

use nsh_core::ui::prelude::*;
use nsh_ui_mobile::{Button, ButtonEvent, ButtonProps, SafeAreaView, SafeAreaViewProps};

/// The app's UI set (`nsh.json` `"rust"`): what a TSX entry may import from
/// `__SPECIFIER__` (`README.md`, "Adding a TSX entry").
#[native_module("__SPECIFIER__")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The application's root component.
#[component]
pub fn App() -> impl IntoElement {
    let count = signal(0_u32);
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(
            SafeAreaView::element(SafeAreaViewProps::default())
                .flex_1()
                .child(
                    view()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_4()
                        .child(text(move || format!("Count: {}", count.get())).text_size(px(28.0)))
                        .child(
                            Button::element(ButtonProps {
                                title: "Tap".into(),
                                ..ButtonProps::default()
                            })
                            .on_event(move |_: &ButtonEvent| count.update(|value| *value += 1)),
                        ),
                ),
        )
}
