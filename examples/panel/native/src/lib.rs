//! The panel example's components (`examples/panel`): the main window's
//! content and the panel window's, both written in Rust.
//!
//! The panel itself is opened by the example's runner (`src/main.rs`):
//! `Panel::open` runs on the main thread, where the runner starts, while
//! components run on the engine thread.

use nsh_core::prelude::*;

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
        .child(text("nsh panel example"))
        .child(text(
            "The panel window floats above this one; drag it by its background.",
        ))
}

/// The panel window's content.
///
/// The runner gives this panel's app launch parameters through its
/// `AppOptions::host` (the app reads them as `host.params` from
/// `nsh/host`). The channel's handlers run on the engine thread, so a TSX
/// app that hides its own panel from one of them hops back to the main
/// thread where the window was opened.
#[component]
pub fn PanelContent() -> impl IntoElement {
    view()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .child(text("This is a panel window."))
        .child(text(
            "It has a title bar here; without one it floats the same.",
        ))
}
