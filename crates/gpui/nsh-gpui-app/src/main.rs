//! The release runner with the mobile UI (`nsh_platform_gpui`'s default UI sets).

// A windowed program: no console opens with it.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    nsh_gpui_app::main()
}
