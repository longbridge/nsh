//! Prototype (#1644): the core of the Windows release runner (JS engine,
//! layout, rendering, the core parts of `nsh/base`) as one DLL that the
//! runner and every feature module link.
#[cfg(windows)]
pub use {nsh_base, nsh_platform_windows, nsh_windows_app};
#[cfg(target_os = "linux")]
pub use {nsh_base, nsh_linux_app, nsh_platform_linux};
