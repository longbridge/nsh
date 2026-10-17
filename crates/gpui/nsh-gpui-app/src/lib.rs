//! The executable of a packaged nsh app on GPUI (`nsh package --target
//! gpui`): runs the bundle shipped with it (`nsh/bundle.nshc`, assets in
//! `nsh/assets`, the app's name, id and appearance in `nsh/app.json`).
//! The package is laid out as the native backend's of the machine it is
//! made on: on Linux and Windows `nsh/` is next to the executable, in a
//! macOS app bundle it is `Contents/Resources/nsh`.
//!
//! A library so that an app with Rust components of its own (`nsh.json`
//! `"rust"`) builds the release runner with them: its `main` calls
//! `nsh_platform_gpui::set_ui_sets` and then [`main`].

use std::path::{Path, PathBuf};

/// The directory of the app's bundle, assets and `app.json`, for the
/// runner at `exe` (resolved through symlinks).
pub fn resources(exe: &Path) -> PathBuf {
    let dir = exe.parent().unwrap_or(Path::new("."));
    // `<App>.app/Contents/MacOS/<exe>` -> `<App>.app/Contents/Resources/nsh`.
    if dir.file_name().is_some_and(|name| name == "MacOS")
        && let Some(contents) = dir.parent()
        && contents.file_name().is_some_and(|name| name == "Contents")
    {
        return contents.join("Resources/nsh");
    }
    dir.join("nsh")
}

/// Runs the release runner (until its window closes).
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub fn main() -> std::process::ExitCode {
    use nsh_platform_gpui::{AppOptions, AppSource, Appearance, AssetBase, Bundle, run_app};

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

    // A symlink to the executable (say in ~/.local/bin) still finds the
    // package.
    let dir = std::env::current_exe()
        .and_then(|exe| exe.canonicalize())
        .map(|exe| resources(&exe))
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
    run_app(AppSource::Bundle(bundle), options)
}

/// Not on the desktop: says so and exits.
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn main() -> std::process::ExitCode {
    eprintln!(
        "nsh-gpui-app is the executable of packaged GPUI apps; build it on macOS, Windows or Linux"
    );
    std::process::ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn resources_are_next_to_the_runner_or_in_the_app_bundle() {
        assert_eq!(
            super::resources(Path::new("/opt/Counter/Counter")),
            Path::new("/opt/Counter/nsh")
        );
        assert_eq!(
            super::resources(Path::new(
                "/Applications/Counter.app/Contents/MacOS/Counter"
            )),
            Path::new("/Applications/Counter.app/Contents/Resources/nsh")
        );
        // A directory merely named MacOS is not an app bundle.
        assert_eq!(
            super::resources(Path::new("/home/me/MacOS/Counter")),
            Path::new("/home/me/MacOS/nsh")
        );
    }
}
