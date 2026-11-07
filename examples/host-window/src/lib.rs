//! The host-window example's components (`examples/host-window`): the main
//! window's content and the host window's, both written in Rust.
//!
//! The host window itself is opened by the example's runner (`src/main.rs`)
//! in `run_app_with`'s setup hook: on the thread that runs the app's message
//! loop, where the platform's `HostWindow` may be created. Components run on
//! the engine thread and cannot open windows.

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

/// The UI set of the host-window example (`nsh.json` `"rust"`): no assets and
/// no components of its own.
#[native_module("host-window/native")]
fn native(_registry: &mut Registry) {}

/// The main window's content.
#[component]
pub fn App() -> impl IntoElement {
    view()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .bg(PAGE)
        .text_color(TEXT)
        .child(text("nsh host-window example"))
        .child(
            text("The second window is the host's own: it has an app of its own inside.")
                .text_color(MUTED),
        )
}

/// The host window's content.
///
/// The runner gives this window's app launch parameters through its
/// `AppOptions::host` (the app reads them as `host.params` from `nsh/host`).
#[component]
pub fn HostWindowContent() -> impl IntoElement {
    view()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .bg(PAGE)
        .text_color(TEXT)
        .child(text("This is the host's second window."))
        .child(text("It floats above the main one on macOS and Windows.").text_color(MUTED))
}
