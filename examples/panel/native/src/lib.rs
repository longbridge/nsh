//! The panel example's components (`examples/panel`): the main window's
//! content and the panel window's, both written in Rust.
//!
//! The panel itself is opened by the example's runner (`src/main.rs`), in
//! `run_app_with`'s setup hook: on the thread that runs the app's message
//! loop, where `Panel::open` may be called. Components run on the engine
//! thread and cannot open windows.

use nsh_core::{element::AdaptiveColor, prelude::*};

/// The example's page.
///
/// The example brings no theme (`nsh.json`'s `"theme"`), so text with no
/// color is black: the colors are `AdaptiveColor`s, which the platform
/// switches with the appearance natively, and the page keeps them readable in
/// both.
const PAGE: AdaptiveColor = AdaptiveColor::rgb(0xffffff, 0x18181b);

/// The example's text on [`PAGE`].
const TEXT: AdaptiveColor = AdaptiveColor::rgb(0x18181b, 0xfafafa);

/// The example's secondary text on [`PAGE`].
const MUTED: AdaptiveColor = AdaptiveColor::rgb(0x52525b, 0xa1a1aa);

/// The UI set of the panel example (`nsh.json` `"rust"`): no assets and no
/// components of its own.
#[native_module("panel/native")]
fn native(_registry: &mut Registry) {}

/// The main window's content.
#[component]
pub fn PanelApp() -> impl IntoElement {
    view()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .bg(PAGE)
        .text_color(TEXT)
        .child(text("nsh panel example"))
        .child(
            text("The panel is the app's second window: it stays above this one on macOS and Windows.")
                .text_color(MUTED),
        )
}

/// The panel window's content.
///
/// The runner gives this panel's app launch parameters through its
/// `AppOptions::host` (the app reads them as `host.params` from
/// `nsh/host`). The channel's handlers run on the engine thread, while the
/// panel's window lives on the thread that runs the app's message loop.
#[component]
pub fn PanelContent() -> impl IntoElement {
    view()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .bg(PAGE)
        .text_color(TEXT)
        .child(text("This is a panel window, with an app of its own."))
        .child(text("It has a title bar here; closing it closes the panel.").text_color(MUTED))
}
