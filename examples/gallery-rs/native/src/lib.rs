//! The Rust gallery (`examples/gallery-rs`): the app's TSX mounts one
//! component of this crate, [`Gallery`], and everything it shows is written
//! in Rust: the same demos as the TSX gallery (`examples/gallery`), on
//! nsh's Rust component API. The app imports the set as `gallery/native`
//! (`nsh.json` `"rust"`).

mod assets;
mod demos;
mod nav;
mod ui;

use nsh_core::ui::prelude::*;
use nsh_ui_mobile::{BarStyle, StatusBar, StatusBarProps};

use crate::{nav::Route, ui::colors};

/// The UI set of the gallery: what `App.tsx` imports from `gallery/native`.
#[native_module("gallery/native")]
fn native(registry: &mut Registry) {
    registry.register::<Gallery>();
    registry.assets(assets!());
}

/// The gallery: a native stack of its index and the screens pushed on it,
/// in the gallery's design for the system's appearance.
#[component]
pub fn Gallery() -> impl IntoElement {
    let routes = nav::routes();
    // A screen is made when it is pushed and goes when it is popped.
    let pushed = Each::new(
        move || routes.with(|r| r.0.clone()),
        |route: &Route| route.key.clone(),
        |route, depth| {
            let route = route.get_untracked();
            (route.screen)()
                // The user popped it natively: it and what is above it go.
                .on_dismiss(move |_: &()| nav::truncate(depth.get_untracked()))
        },
    );
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
        .child(stack().flex_1().child(demos::index()).child(pushed))
}
