//! Development host on GPUI: runs any nsh app in a GPUI window, on macOS,
//! Windows and Linux.
//!
//! - `nsh-gpui-host --dev-server ws://127.0.0.1:8790/ws [--trace]` loads
//!   bundles pushed by `nsh dev` / `nsh run` and reloads on every rebuild.
//! - `nsh-gpui-host --bundle <path> [--trace]` runs a built bundle (`.nshc`
//!   is QuickJS bytecode, anything else JS source).
//! - `--script "wait 800; shot a.png; tap 100 200; quit"` runs steps once
//!   the window shows: screenshots and simulated input (on Linux under
//!   X11, which shots need).
//! - `--window-size 1200x800` sets the window size.
//! - `--fps` logs frame statistics once a second (`nsh::perf fps …`).
//! - `--app-id <id>` sets the application id (`com.example.app`); `nsh run`
//!   passes the app's (`nsh.json` `"id"`). The app's data (`nsh/storage`)
//!   is kept under the id its bundle carries, in the system's data
//!   directory.
//!
//! Without arguments it connects to the dev server on this machine.
//!
//! A library so that an app with Rust components of its own (`nsh.json`
//! `"rust"`) builds the development host with them: its `main` calls
//! `nsh_platform_gpui::set_ui_sets` and then [`main`].

/// Runs the development host (until its window closes).
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub fn main() -> std::process::ExitCode {
    use std::{env, path::PathBuf};

    use nsh_platform_gpui::{AppOptions, AppSource, AssetBase, Bundle, Limits, dev, run_app};

    dev::init_logging();
    let mut source = None;
    // As the release runner runs the app: on its own.
    let mut options = AppOptions {
        limits: Limits::standalone(),
        ..AppOptions::default()
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dev-server" => source = args.next().map(AppSource::DevServer),
            "--bundle" => {
                let path = PathBuf::from(args.next().unwrap_or_default());
                // SAFETY: the developer names the file to run on the
                // command line of this development host.
                let bundle = match unsafe { Bundle::read_trusted(&path) } {
                    Ok(bundle) => bundle,
                    Err(err) => {
                        log::error!("cannot read bundle {}: {err}", path.display());
                        return std::process::ExitCode::FAILURE;
                    }
                };
                // `nsh build` copies imported assets next to the bundle.
                options.asset_base = path
                    .parent()
                    .map(|dir| AssetBase::Directory(dir.join("assets")));
                source = Some(AppSource::Bundle(bundle));
            }
            "--trace" => options.trace = true,
            "--app-id" => {
                if let Some(id) = args.next().filter(|id| !id.is_empty()) {
                    options.app_id = id;
                }
            }
            "--fps" => options.fps = true,
            "--script" => options.script = args.next(),
            "--window-size" => {
                if let Some((w, h)) = args.next().as_deref().and_then(|s| s.split_once('x')) {
                    options.size = (w.parse().unwrap_or(1200), h.parse().unwrap_or(800));
                }
            }
            other => log::warn!("unknown argument {other}"),
        }
    }
    let source = source.unwrap_or_else(|| {
        use nsh_dev::protocol::{DEFAULT_PORT, WS_PATH};
        AppSource::DevServer(format!("ws://127.0.0.1:{DEFAULT_PORT}{WS_PATH}"))
    });
    options.title = "nsh".to_owned();
    // Script shots read the window from the X server on Linux: run under
    // X11 (Xwayland on a Wayland desktop).
    #[cfg(target_os = "linux")]
    if options.script.is_some() && env::var_os("DISPLAY").is_some() {
        // SAFETY: no other thread runs yet.
        unsafe { env::remove_var("WAYLAND_DISPLAY") };
    }
    run_app(source, options)
}

/// Not on the desktop: says so and exits.
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn main() -> std::process::ExitCode {
    eprintln!("nsh-gpui-host runs on macOS, Windows and Linux");
    std::process::ExitCode::FAILURE
}
