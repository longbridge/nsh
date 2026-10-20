//! The release runner with `nsh/base` (`nsh_platform_gpui`'s defaults).

// A windowed program: no console opens with it.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    nsh_gpui_app::main()
}
