//! The executable of a packaged nsh app (`nsh package --target linux`):
//! runs the bundle shipped next to it (`nsh/bundle.nshc`, assets in
//! `nsh/assets`, the app's name, id and appearance in `nsh/app.json`).
//!
//! A library so that an app with Rust components of its own (`nsh.json`
//! `"rust"`) builds the release runner with them: its `main` calls
//! `nsh_platform_linux::set_ui_sets` and then [`main`].

/// Runs the release runner (until its window closes).
#[cfg(target_os = "linux")]
pub fn main() -> std::process::ExitCode {
    use nsh_platform_linux::{
        AppOptions, AppSource, Appearance, AssetBase, Bundle, Limits, run_app,
    };

    struct Logger;
    impl log::Log for Logger {
        fn enabled(&self, m: &log::Metadata<'_>) -> bool {
            m.level() <= log::Level::Warn || m.target() == "js"
        }
        fn log(&self, r: &log::Record<'_>) {
            if self.enabled(r.metadata()) {
                eprintln!("{} {}: {}", r.level(), r.target(), r.args());
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: Logger = Logger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);

    // The package directory holds the executable (a symlink to it, say in
    // ~/.local/bin, still finds it).
    let dir = std::env::current_exe()
        .and_then(|exe| exe.canonicalize())
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("nsh")))
        .unwrap_or_default();
    // SAFETY: the bytecode is a resource of this app's own package.
    let bundle = match unsafe { Bundle::read_trusted(&dir.join("bundle.nshc")) } {
        Ok(bundle) => bundle,
        Err(err) => {
            log::error!("cannot read the app bundle in {}: {err}", dir.display());
            return std::process::ExitCode::FAILURE;
        }
    };
    let mut options = AppOptions {
        asset_base: Some(AssetBase::Directory(dir.join("assets"))),
        // An app on its own, not a module embedded in another app.
        limits: Limits::standalone_desktop(),
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
    // The window opens in the app's appearance (no flash of the system's).
    match field("appearance").as_deref() {
        Some("light") => options.appearance = Appearance::Light,
        Some("dark") => options.appearance = Appearance::Dark,
        _ => {}
    }
    if run_app(AppSource::Bundle(bundle), options)
        == nsh_platform_linux::gtk4::glib::ExitCode::SUCCESS
    {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

/// Not on Linux: says so and exits.
#[cfg(not(target_os = "linux"))]
pub fn main() -> std::process::ExitCode {
    eprintln!("nsh-linux-app is the executable of packaged Linux apps; build it on Linux");
    std::process::ExitCode::FAILURE
}
