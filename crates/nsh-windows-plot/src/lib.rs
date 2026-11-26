//! Prototype (#1644): the charts as a module DLL. It links `nsh-core.dll`
//! for everything it shares with the core, and builds `nsh/base`'s plot
//! sources into itself. The runner loads it when the app imports
//! `nsh/base/plot` and calls [`nsh_part_plot`].
#![cfg(windows)]
#![allow(dead_code, unreachable_pub)]

// Links what the module shares with the core from `nsh-core.dll`: without
// a use, rustc would link those crates into the module again.
extern crate nsh_windows_core;

#[path = "../../nsh-base/src/plot/mod.rs"]
pub mod plot;

mod keys {
    pub(crate) use nsh_base::keys::bind_while;
}

mod terminal {
    pub(crate) use nsh_base::terminal::is_terminal;
}

/// The part's gate, as on the web: registers the charts' module.
#[unsafe(no_mangle)]
pub fn nsh_part_plot(registry: &mut nsh_core::element::Registry) {
    registry.submodule(plot::registry());
}
