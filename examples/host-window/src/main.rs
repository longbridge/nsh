//! Runs the app directly on the machine's native backend (`cargo run`).
//!
//! The runner opens the host's second window in `run_app_with`'s setup hook —
//! on the thread that runs the app's message loop, the only thread where the
//! platform's `HostWindow` may be created — keeps the window in a thread
//! local so it lives until the process exits, and shows it beside the main
//! window. `HostWindow` is not `Send`: it never leaves that thread.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use nsh_core::{
    desktop::{HostWindowError, WindowAttributes, WindowKind, WindowOptions},
    prelude::*,
};
use nsh_engine::{HostChannel, JsonValue};
#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
std::thread_local! {
    /// The host's second window, kept alive for the whole run.
    static HOST_WINDOW: std::cell::RefCell<Option<platform::HostWindow>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> impl std::process::Termination {
    env_logger::init();
    platform::run_app_with(
        platform::AppSource::Rust(platform::Root::of::<host_window::App>()),
        platform::AppOptions {
            title: "Host Window".to_owned(),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            app_id: "com.example.host-window".to_owned(),
            size: (720, 480),
            ..Default::default()
        },
        |context| match open_host_window(context) {
            Ok(window) => {
                window.show();
                HOST_WINDOW.with(|slot| *slot.borrow_mut() = Some(window));
            }
            Err(HostWindowError::WrongThread) => {
                eprintln!("can't open the host window off the app's thread");
            }
            Err(HostWindowError::Invalid(what)) => {
                eprintln!("can't open the host window: {what}");
            }
            Err(_) => eprintln!("can't open the host window"),
        },
    )
}

/// Opens the host's second window: a panel-kind window of its own, with its
/// own app inside (a fresh engine, its own host channel).
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn open_host_window(
    context: &platform::AppContext,
) -> Result<platform::HostWindow, HostWindowError> {
    let mut options = WindowOptions::new("host");
    options.kind = WindowKind::Panel;
    options.default_size = Some(Size::new(Points(420.0), Points(300.0)));
    let mut attributes = WindowAttributes::default();
    attributes.title = Some("The host's window".to_owned());
    attributes.closable = Some(true);
    options.attributes = attributes;
    let host = HostChannel::new();
    let mut params = serde_json::Map::new();
    params.insert("quick".to_owned(), JsonValue::from(true));
    host.set_params(params);
    let app = platform::AppOptions {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        app_id: "com.example.host-window.window".to_owned(),
        host,
        ..Default::default()
    };
    context.open_host_window(
        options,
        app,
        platform::AppSource::Rust(platform::Root::of::<host_window::HostWindowContent>()),
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!(
    "Run this binary on macOS, Windows or Linux; use nsh run for iOS, Android and the web."
);
