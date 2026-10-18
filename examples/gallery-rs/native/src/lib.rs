//! The Rust gallery (`examples/gallery-rs`): the app's TSX mounts one
//! component of this crate, [`Gallery`], and everything it shows is written
//! in Rust: the same demos as the TSX gallery (`examples/gallery`), on
//! nsh's Rust component API. The app imports the set as `gallery/native`
//! (`nsh.json` `"rust"`).

mod assets;
mod demos;
mod nav;
mod shell;
mod ui;

use nsh_core::ui::prelude::*;
use nsh_ui_mobile::{BarStyle, StatusBar, StatusBarProps};

use crate::ui::{colors, desktop};

/// The UI set of the gallery: what `App.tsx` imports from `gallery/native`.
#[native_module("gallery/native")]
fn native(registry: &mut Registry) {
    registry.register::<Gallery>();
    registry.assets(assets!());
}

/// The gallery, in the gallery's design for the system's appearance: on
/// phones a native stack of its index and the screens pushed on it; in wide
/// windows a sidebar of the demos and the selected one beside it.
#[component]
pub fn Gallery() -> impl IntoElement {
    view()
        .flex_1()
        .bg(colors::BACKGROUND)
        // The status bar's content contrasts with the page in either
        // appearance.
        .child(StatusBar::element(StatusBarProps {
            bar_style: Prop::bind(|| {
                Some(if dark() {
                    BarStyle::LightContent
                } else {
                    BarStyle::DarkContent
                })
            }),
            ..StatusBarProps::default()
        }))
        // Made again when the window crosses the threshold, as TS remounts
        // its shell: what is open carries over.
        .child(If::new(desktop, shell::desktop_layout).otherwise(shell::phone))
}
