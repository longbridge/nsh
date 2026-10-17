//! GPUI backend for nsh: apps drawn by GPUI (GPUI Fast), on macOS, Windows
//! and Linux alike, and embeddable in any GPUI or GPUI Kit app.
//!
//! - Each node is an [`view::NshView`], a retained view the core placed at
//!   its frame. A surface is one GPUI element ([`render::SurfaceElement`])
//!   that paints the whole tree with GPUI's primitives (quads, shadows,
//!   paths, glyphs, images): shadows and background, the content (text,
//!   image, canvas), the children, the border, then the items the view
//!   hosts (text editors, headers).
//! - The engine runs on its own thread ([`nsh_engine::Engine`]); each
//!   commit is one closure sent to GPUI's main thread ([`rt`],
//!   [`surface`]), which notifies the surface's view.
//! - Text is shaped by GPUI's text system on the engine thread and broken
//!   into lines by nsh; the main thread paints the glyphs it placed
//!   ([`text`]).
//! - Native animations and JS frame requests tick on the window's frames
//!   while something animates.
//!
//! [`AppView`] is one surface as a GPUI view (`Entity<AppView>`), to put
//! anywhere in a GPUI app's element tree; [`run_app`] opens one in a
//! window. The input, scrolling, transition and editing logic is the
//! Windows backend's (its virtual key codes included, [`keys`]). Off the
//! desktop this crate is empty.

#![cfg(any(target_os = "linux", target_os = "macos", windows))]

mod access;
mod animation;
mod app;
mod canvas;
mod chrome;
mod decor;
#[cfg(feature = "dev")]
pub mod dev;
mod events;
mod geom;
mod image;
mod input;
mod keymap;
mod keys;
mod mount;
mod native;
mod navigation;
mod net;
mod render;
mod rt;
mod script;
mod scroll;
mod stats;
mod surface;
mod system;
mod text;
mod text_input;
mod transition;
mod view;

pub use gpui;
pub use nsh_core::{
    platform::Appearance,
    ui::{AdaptiveColor, Color, Theme, ThemeColors, ThemeError},
};
pub use nsh_engine::{App, Bundle, Root};

pub use crate::{
    app::{AppOptions, AppSource, AppView, open_app_window, run_app, set_ui_sets},
    image::AssetBase,
    native::{ComponentContext, ComponentFactory, Components, NativeComponent},
};
