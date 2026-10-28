//! Public terminal surface and cell text contracts.

mod text;
mod text_input;

#[cfg(feature = "js")]
mod animation;
mod app;

mod canvas;
mod controls;
mod fills;
mod focus_navigation;
mod geometry;
mod keyboard;
mod modal_focus;
mod navigation;
mod page_background;
mod performance;
mod pointer;
mod scroll;
mod search;
