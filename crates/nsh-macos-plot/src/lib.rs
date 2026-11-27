//! Prototype (#1644): the charts as a static part of the macOS
//! runner. It builds `nsh/base`'s plot sources into itself; `nsh package`
//! links it when the app imports `nsh/base/plot`, and the runner calls
//! [`nsh_part_plot`].
#![cfg(target_os = "macos")]
#![allow(dead_code, unreachable_pub)]

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
