//! Prototype (#1644): the release runner on the core DLL. The module DLLs
//! next to the executable are those `nsh package` copied for the parts
//! the app imports: each one's gate adds its part to `nsh/base`.
#![windows_subsystem = "windows"]

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use nsh_windows_core::{nsh_base, nsh_platform_windows, nsh_windows_app};
    let mut set = nsh_base::registry();
    modules::load(&mut set);
    nsh_platform_windows::set_ui_sets(vec![set]);
    nsh_windows_app::main()
}

#[cfg(windows)]
mod modules {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};

    use nsh_core::element::Registry;

    /// A module DLL and its gate.
    const MODULES: &[(&str, &[u8])] = &[
        ("nsh_windows_plot.dll", b"nsh_part_plot\0"),
        ("nsh_windows_editor.dll", b"nsh_part_editor\0"),
    ];

    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    }

    /// Loads each module DLL beside the executable and runs its gate.
    pub(crate) fn load(set: &mut Registry) {
        let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_owned()))
        else {
            return;
        };
        for (file, gate) in MODULES {
            let path = dir.join(file);
            if !path.exists() {
                continue;
            }
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            // SAFETY: the DLL is a module built with this runner in the
            // same build, so its gate has this signature.
            unsafe {
                let module = LoadLibraryW(wide.as_ptr());
                if module.is_null() {
                    log::warn!("cannot load {file}");
                    continue;
                }
                let f = GetProcAddress(module, gate.as_ptr());
                if f.is_null() {
                    continue;
                }
                let gate: fn(&mut Registry) = std::mem::transmute(f);
                gate(set);
            }
        }
    }
}

#[cfg(not(windows))]
fn main() {}
