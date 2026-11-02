//! Runs the panel example: a main window and the panel window beside it.
//!
//! The panel is opened in `run_app_with`'s setup hook, which runs on the
//! thread that runs the app's message loop (the main thread on macOS and
//! Linux, the app thread `run_app` spawns on Windows). A panel is not `Send`,
//! so it cannot be opened first and moved to that thread: the hook creates
//! it there, and the host keeps it until the app ends.
//!
//! Components run on the engine thread and cannot open windows, so
//! `nsh run examples/panel` shows only the main window: its root component
//! is `PanelApp`. Run this binary for the panel: `cargo run -p panel-native`.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use nsh_core::geometry::{Points, Size};
use nsh_engine::{HostChannel, JsonValue};

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> impl std::process::Termination {
    // nsh.json's id: the main window's app and where it keeps its data.
    let app_id = "com.longbridge.nsh.panel";
    platform::run_app_with(
        platform::AppSource::Rust(platform::Root::of::<panel_native::PanelApp>()),
        platform::AppOptions {
            title: "Panel Example".to_owned(),
            app_id: app_id.to_owned(),
            size: (720, 480),
            ..Default::default()
        },
        |_| match open_panel(app_id) {
            Ok(panel) => {
                // The host keeps the panels the hook returns; one that is
                // never shown stays hidden.
                panel.show();
                vec![panel]
            }
            Err(error) => {
                eprintln!("can't open the panel window: {error}");
                Vec::new()
            }
        },
    )
}

/// Opens the example's panel: a titled window that floats above the Dock or
/// the taskbar, over the main window.
///
/// Runs in `run_app_with`'s setup hook, on the thread that runs the app's
/// message loop; `Panel::open` anywhere else fails with
/// [`platform::PanelError::WrongThread`].
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn open_panel(app_id: &str) -> Result<platform::Panel, platform::PanelError> {
    let options = platform::PanelOptions::new()
        .with_title("Panel")
        .with_size(Size::new(Points(420.0), Points(300.0)))
        // A title bar to close the example with; the default is none.
        .with_borderless(false)
        .with_level(platform::WindowLevel::AboveDock)
        // macOS moves it by its background; Windows and Linux ignore it.
        .with_movable_by_background(true);
    // What the panel's app reads as `host.params` when it starts
    // (`host.params.quick` in TSX: the host and its app share one channel,
    // `nsh/host`). A launch parameter is read once, when the app starts, so
    // the host sets it before `Panel::open` loads the app.
    let host = HostChannel::new();
    let mut params = serde_json::Map::new();
    params.insert("quick".to_owned(), JsonValue::from(true));
    host.set_params(params);
    // A panel runs an app of its own: its id decides where it keeps data
    // (`<data_dir>/<app id>`), apart from the main window's app. The
    // window's title is `PanelOptions::title` above, not this app's.
    let app = platform::AppOptions {
        app_id: format!("{app_id}.panel"),
        host,
        ..Default::default()
    };
    platform::Panel::open(
        options,
        app,
        platform::AppSource::Rust(platform::Root::of::<panel_native::PanelContent>()),
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!(
    "Run this binary on macOS, Windows or Linux; use nsh run for iOS, Android and the web."
);
