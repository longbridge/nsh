//! Development host on macOS: runs any nsh app in an AppKit window.
//!
//! - `nsh-macos-host --dev-server ws://127.0.0.1:8790/ws [--trace]` loads
//!   bundles pushed by `nsh dev` / `nsh run` and reloads on every rebuild.
//! - `nsh-macos-host --bundle <path> [--trace]` runs a built bundle (`.nshc`
//!   is QuickJS bytecode, anything else JS source).
//!
//! - `--script "wait 800; shot a.png; tap 100 200; wait 300; shot b.png;
//!   quit"` runs steps once the window shows: window screenshots and
//!   simulated input, to check the backend without touching the screen.
//! - `--fps` shows the performance monitor (frame rate, frame and commit
//!   costs, CPU, GPU, memory) at the top right.
//! - `--size 1200x800` sets the window size (1200×800 by default; 390x844
//!   shows an app's phone layout).
//! - `--name <name>` names the app (the menu bar, the window title; `nsh
//!   run` passes the app's name), "nsh" by default.
//!
//! Without arguments it connects to the dev server on this machine.
//!
//! A library so that an app with Rust components of its own (`nsh.json`
//! `"rust"`) builds the development host with them: its `main` calls
//! `nsh_platform_macos::set_ui_sets` and then [`main`].

/// Runs the development host (until its window closes).
#[cfg(target_os = "macos")]
pub fn main() -> std::process::ExitCode {
    use std::{env, path::PathBuf};

    use nsh_platform_macos::{AppOptions, AppSource, AssetBase, Bundle, Limits, dev, run_app};

    dev::init_logging();
    let mut source = None;
    // Named "nsh" unless `--name` names the app.
    // As the release runner runs the app: on its own.
    let mut options = AppOptions {
        limits: Limits::standalone_desktop(),
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
            "--name" => {
                if let Some(name) = args.next().filter(|n| !n.is_empty()) {
                    options.title = name;
                }
            }
            // The performance monitor over the app.
            "--fps" => options.fps = true,
            // Steps run once the window shows (`nsh_platform_macos`'s
            // `script` module): window screenshots and simulated input.
            "--script" => options.script = args.next(),
            "--size" => {
                if let Some((w, h)) = args.next().as_deref().and_then(|s| s.split_once('x')) {
                    options.size = (w.parse().unwrap_or(1200), h.parse().unwrap_or(800));
                }
            }
            // AppKit's own (`-ApplePersistenceIgnoreState YES`).
            other if other.starts_with('-') && !other.starts_with("--") => {
                args.next();
            }
            other => log::warn!("unknown argument {other}"),
        }
    }
    let source = source.unwrap_or_else(|| {
        use nsh_dev::protocol::{DEFAULT_PORT, WS_PATH};
        AppSource::DevServer(format!("ws://127.0.0.1:{DEFAULT_PORT}{WS_PATH}"))
    });
    run_app(source, options)
}

/// Not on macOS: says so and exits.
#[cfg(not(target_os = "macos"))]
pub fn main() -> std::process::ExitCode {
    eprintln!("nsh-macos-host runs on macOS");
    std::process::ExitCode::FAILURE
}
