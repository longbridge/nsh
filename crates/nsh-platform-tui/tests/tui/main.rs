//! Public terminal surface and cell text contracts.

mod text;
mod text_input;

#[cfg(feature = "js")]
mod animation;
mod app;

mod canvas;
mod controls;
mod focus_navigation;
mod keyboard;
mod modal_focus;
mod navigation;
mod page_background;
mod scroll;
mod search;
