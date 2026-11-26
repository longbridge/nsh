//! Prototype (#1644): `Editor` and `DiffView` as a module DLL. It links
//! `nsh-core.dll` for everything it shares with the core, and builds
//! `nsh/base`'s editor sources (and the small helpers they use) into
//! itself. The runner loads it when the app makes an `Editor` and calls
//! [`nsh_part_editor`].
#![cfg(windows)]
#![allow(dead_code, unreachable_pub, unused_imports, missing_docs)]

// Links what the module shares with the core from `nsh-core.dll`: without
// a use, rustc would link those crates into the module again.
extern crate nsh_windows_core;

#[path = "../../nsh-base/src/autoscroll.rs"]
mod autoscroll;
#[path = "../../nsh-base/src/diff/mod.rs"]
pub mod diff;
#[path = "../../nsh-base/src/editor/mod.rs"]
pub mod editor;
#[path = "../../nsh-base/src/sink_text.rs"]
mod sink_text;
#[path = "../../nsh-base/src/sort.rs"]
mod sort;
#[path = "../../nsh-base/src/syntax.rs"]
mod syntax;
#[path = "../../nsh-base/src/window.rs"]
mod window;

pub(crate) use nsh_base::theme;
pub(crate) use nsh_core::element::input;

mod terminal {
    pub(crate) use nsh_base::terminal::{is_terminal, row_height};
}

mod text_view {
    pub(crate) use nsh_base::text_view::{PartStyle, merge_styles};
}

/// The part's gate, as on the web: registers `Editor` and `DiffView`.
#[unsafe(no_mangle)]
pub fn nsh_part_editor(registry: &mut nsh_core::element::Registry) {
    registry
        .register::<editor::Editor>()
        .register::<diff::DiffView>();
}
