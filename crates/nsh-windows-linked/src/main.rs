//! Prototype (#1644): the release runner, built once with every part, and
//! linked again by `nsh package` from the same prebuilt objects with only
//! the parts the app imports. The gates are resolved at link time: a part
//! left out of the link resolves to [`nsh_part_none`]
//! (`/ALTERNATENAME`), so none of its code is in the executable.
#![windows_subsystem = "windows"]

#[cfg(windows)]
use nsh_core::element::Registry;
// Puts the parts' objects on the full link: `nsh package` drops them.
#[cfg(windows)]
use {nsh_windows_editor as _, nsh_windows_plot as _};

#[cfg(windows)]
unsafe extern "Rust" {
    fn nsh_part_editor(set: &mut Registry);
    fn nsh_part_plot(set: &mut Registry);
}

/// The gate of a part that is not linked.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub fn nsh_part_none(_: &mut Registry) {}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use nsh_windows_core::{nsh_base, nsh_platform_windows, nsh_windows_app};
    let mut set = nsh_base::registry();
    // SAFETY: each gate is a part's `nsh_part_*` or `nsh_part_none`, all
    // with this signature.
    unsafe {
        nsh_part_editor(&mut set);
        nsh_part_plot(&mut set);
    }
    nsh_platform_windows::set_ui_sets(vec![set]);
    nsh_windows_app::main()
}

#[cfg(not(windows))]
fn main() {}
