//! Runs the panel example: opens a panel window, then the app's main
//! window.
//!
//! `Panel::open` runs on the main thread, as `run_app` does, so the runner
//! opens the panel before it starts the app; components run on the engine
//! thread and cannot open windows.

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
    let options = platform::PanelOptions {
        title: "Panel".to_owned(),
        size: Some(Size::new(Points(420.0), Points(300.0))),
        // A title bar to close the example with; the default is none.
        borderless: false,
        movable_by_background: true,
        ..Default::default()
    };
    // What the panel's app reads as `host.params` when it starts
    // (`host.params.quick` in TSX: the host and its app share one channel,
    // `nsh/host`). A launch parameter is read once, when the app starts, so
    // the host sets it before `Panel::open` loads the app.
    let host = HostChannel::new();
    let mut params = serde_json::Map::new();
    params.insert("quick".to_owned(), JsonValue::from(true));
    host.set_params(params);
    // A panel runs an app of its own: its id decides where it keeps data
    // (`<data_dir>/<app id>`), apart from the main window's app.
    let panel_app = platform::AppOptions {
        app_id: format!("{app_id}.panel"),
        host,
        ..Default::default()
    };
    let panel = match platform::Panel::open(
        options,
        panel_app,
        platform::AppSource::Rust(platform::Root::of::<panel_native::PanelContent>()),
    ) {
        Ok(panel) => panel,
        Err(error) => {
            eprintln!("can't open the panel window: {error}");
            std::process::exit(1);
        }
    };
    panel.show();
    let code = platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<panel_native::PanelApp>()),
        platform::AppOptions {
            title: "Panel Example".to_owned(),
            app_id: app_id.to_owned(),
            size: (720, 480),
            ..Default::default()
        },
    );
    drop(panel);
    code
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!(
    "Run this binary on macOS, Windows or Linux; use nsh run for iOS, Android and the web."
);
