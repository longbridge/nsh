//! Prototype (#1644): the macOS release runner, built once with every
//! part, and linked again by `nsh package` from the same prebuilt objects
//! with only the parts the app imports. A part left out of the link has
//! its gate aliased to [`nsh_part_none`] (ld64 `-alias`), so none of its
//! code is in the executable.

#[cfg(target_os = "macos")]
use nsh_core::element::Registry;
// Puts the parts' objects on the full link: `nsh package` drops them.
#[cfg(target_os = "macos")]
use {nsh_macos_editor as _, nsh_macos_plot as _};

#[cfg(target_os = "macos")]
unsafe extern "Rust" {
    fn nsh_part_editor(set: &mut Registry);
    fn nsh_part_plot(set: &mut Registry);
}

/// The gate of a part that is not linked.
#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub fn nsh_part_none(_: &mut Registry) {}

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    let mut set = nsh_base::registry();
    // SAFETY: each gate is a part's `nsh_part_*` or `nsh_part_none`, all
    // with this signature.
    unsafe {
        nsh_part_editor(&mut set);
        nsh_part_plot(&mut set);
    }
    nsh_platform_macos::set_ui_sets(vec![set]);
    nsh_macos_app::main()
}

#[cfg(not(target_os = "macos"))]
fn main() {}
