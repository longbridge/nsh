//! The executable of a packaged nsh app (`nsh package --target windows`):
//! runs the bundle shipped next to it (`nsh\bundle.nshc`, assets in
//! `nsh\assets`, the app's name, id, appearance and URL schemes in `nsh\app.json`, its
//! icon in `nsh\icon.ico`), or a newer signed package of its code it
//! downloaded (`nsh_engine::updates`).
//!
//! A library so that an app with Rust components of its own (`nsh.json`
//! `"rust"`) builds the release runner with them: its `main` calls
//! `nsh_platform_windows::set_ui_sets` and then [`main`]. A Rust app
//! (`nsh.json` `"root"`) calls [`run_rust`] with its root instead, built
//! without the feature `js`: the runner then links no QuickJS.
//!
//! With the feature `devtools` (`nsh package --devtools`) the runner serves
//! the app's DevTools endpoint when `NSH_DEVTOOLS` is set, records its
//! console for it and takes the arguments a test host takes (`--data-dir`,
//! `--window-size`), which `nsh test --app` gives it.

/// Runs the release runner (until its window closes).
#[cfg(all(windows, feature = "js"))]
pub fn main() -> std::process::ExitCode {
    use nsh_platform_windows::{AppSource, AssetBase, Bundle, run_app};

    let (dir, mut options) = setup();
    // SAFETY: the bytecode is a resource of this app's own package.
    let bundle = match unsafe { Bundle::read_trusted(&dir.join("bundle.nshc")) } {
        Ok(bundle) => bundle,
        Err(err) => {
            nsh_platform_windows::fatal_error(&format!(
                "Cannot read the app bundle in {}: {err}",
                dir.display()
            ));
            return std::process::ExitCode::FAILURE;
        }
    };
    let data_dir = options.data_dir.clone().or_else(nsh_engine::data_dir);
    let launch = nsh_engine::updates::launch(bundle, &dir, data_dir.as_deref());
    options.asset_base = Some(AssetBase::Directory(launch.assets));
    run_app(AppSource::Bundle(launch.bundle), options)
}

/// Runs the Rust app whose root is `root` (until its window closes), with
/// the package's assets, name, id and appearance as [`main`] has them.
#[cfg(windows)]
pub fn run_rust(root: nsh_platform_windows::Root) -> std::process::ExitCode {
    let (_, options) = setup();
    nsh_platform_windows::run_app(nsh_platform_windows::AppSource::Rust(root), options)
}

/// Built without the feature `js`: runs Rust apps only ([`run_rust`]).
#[cfg(all(windows, not(feature = "js")))]
pub fn main() -> std::process::ExitCode {
    nsh_platform_windows::fatal_error(
        "This runner was built without the feature `js`: it runs Rust apps (run_rust).",
    );
    std::process::ExitCode::FAILURE
}

/// Installs the logger and reads the package: its `nsh` directory (next
/// to the executable, or `NSH_APP_DIR`: `nsh run` of a Rust app) and the
/// window's options.
#[cfg(windows)]
fn setup() -> (std::path::PathBuf, nsh_platform_windows::AppOptions) {
    use nsh_platform_windows::{AppOptions, Appearance, AssetBase, Limits};

    /// Run by `nsh run`: the app's own logs go to its terminal too.
    static DEVELOPMENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    // A GUI program has no console: warnings go to the debugger
    // (`OutputDebugString`), where DebugView and Visual Studio show them;
    // run by `nsh run`, everything also goes to the pipe it reads.
    struct Logger;
    impl log::Log for Logger {
        fn enabled(&self, m: &log::Metadata<'_>) -> bool {
            printed(m) || collected(m)
        }
        fn log(&self, r: &log::Record<'_>) {
            #[cfg(feature = "devtools")]
            if collected(r.metadata()) {
                nsh_platform_windows::devtools_record(r);
            }
            if printed(r.metadata()) {
                let line = format!("{} {}: {}\n", r.level(), r.target(), r.args());
                nsh_platform_windows::debug_output(&line);
                if DEVELOPMENT.get() == Some(&true) {
                    eprint!("{line}");
                }
            }
        }
        fn flush(&self) {}
    }
    /// What goes to the debugger.
    fn printed(m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Warn || m.target() == "js" || DEVELOPMENT.get() == Some(&true)
    }
    /// What the DevTools console collects (all of nsh's lines, as a
    /// development host's logger does).
    fn collected(m: &log::Metadata<'_>) -> bool {
        cfg!(feature = "devtools") && (m.target().starts_with("nsh") || printed(m))
    }
    static LOGGER: Logger = Logger;
    let development = std::env::var_os("NSH_APP_DIR");
    let _ = DEVELOPMENT.set(development.is_some());
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);

    let dir = match development {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.join("nsh")))
            .unwrap_or_default(),
    };
    let icon = dir.join("icon.ico");
    let mut options = AppOptions {
        asset_base: Some(AssetBase::Directory(dir.join("assets"))),
        icon: icon.is_file().then_some(icon),
        // An app on its own, not a module embedded in another app.
        limits: Limits::standalone(),
        ..AppOptions::default()
    };
    let info = std::fs::read_to_string(dir.join("app.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let field = |key: &str| {
        info.as_ref()
            .and_then(|info| info.get(key)?.as_str().map(str::to_owned))
    };
    if let Some(id) = field("id") {
        options.app_id = id;
    }
    if let Some(name) = field("name") {
        options.title = name;
    }
    // The URL schemes it registers and opens (`EventKind::UrlScheme`).
    if let Some(schemes) = info
        .as_ref()
        .and_then(|info| info.get("schemes")?.as_array().cloned())
    {
        options.schemes = schemes
            .iter()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect();
    }
    // nsh.json `fileTypes`: the files it opens.
    options.file_types = info
        .as_ref()
        .and_then(|info| info.get("fileTypes"))
        .map(nsh_platform_windows::file_type::file_types_from_json)
        .unwrap_or_default();
    // nsh.json `singleInstance`: a second launch goes to this one.
    options.single_instance = info
        .as_ref()
        .and_then(|info| info.get("singleInstance")?.as_bool())
        .unwrap_or(false);
    // The window opens in the app's appearance (no flash of the system's).
    match field("appearance").as_deref() {
        Some("light") => options.appearance = Appearance::Light,
        Some("dark") => options.appearance = Appearance::Dark,
        _ => {}
    }
    #[cfg(feature = "devtools")]
    test_arguments(&mut options);
    (dir, options)
}

/// The arguments of a test host (`nsh_windows_host`'s), which `nsh test
/// --app` starts the packaged app with: where it keeps its data (fresh for
/// each test) and the window's size.
#[cfg(all(windows, feature = "devtools"))]
fn test_arguments(options: &mut nsh_platform_windows::AppOptions) {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                options.data_dir = args.next().map(std::path::PathBuf::from);
                // Its caches too (downloaded images): a test's app starts
                // with none.
                if let Some(dir) = &options.data_dir {
                    options.cache = nsh_engine::CacheOptions::at(nsh_engine::CacheLocation::Dir(
                        dir.join("cache"),
                    ));
                }
            }
            "--window-size" => {
                if let Some((w, h)) = args.next().as_deref().and_then(|s| s.split_once('x')) {
                    options.size = (w.parse().unwrap_or(1200), h.parse().unwrap_or(800));
                }
            }
            _ => {}
        }
    }
}

/// Not on Windows: says so and exits.
#[cfg(not(windows))]
pub fn main() -> std::process::ExitCode {
    eprintln!("nsh-windows-app is the executable of packaged Windows apps; build it on Windows");
    std::process::ExitCode::FAILURE
}
