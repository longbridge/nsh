//! The development host with `nsh/base` (all of `nsh/base`).

fn main() -> std::process::ExitCode {
    // Runs any app's JS: every part of `nsh/base` it may import.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    nsh_platform_gpui::set_ui_sets(vec![nsh_base::registry()]);
    nsh_gpui_host::main()
}
