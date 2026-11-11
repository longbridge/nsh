//! `nsh devtools webdriver`: a W3C WebDriver server (experimental) that
//! Appium and WebDriver clients drive NSH apps through. Each command is a
//! step of the DevTools protocol on the app's session (`Session`); the server
//! holds one app at a time.
//!
//! `error.rs` is WebDriver's error answers, `session.rs` the capabilities and
//! the session, `server.rs` the routes.

mod actions;
mod element;
mod error;
mod keys;
mod server;
mod session;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context as _, anyhow};
use nsh_devtools::client::Client;
use serde_json::Value;

use self::{
    error::WdError,
    server::{Launch, Server, check_host, guard, router},
    session::Caps,
};
use super::{DevtoolsHost, Session};
use crate::{test_host::Launchee, ui};

/// A launched app: its protocol, and the host that quits with it.
struct HostSession {
    client: Arc<Client>,
    /// Dropped after the client: it quits the app and removes its data.
    _host: Mutex<DevtoolsHost>,
}

impl Session for HostSession {
    fn call_within(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, nsh_devtools::protocol::ErrorBody> {
        self.client.call_within(method, params, timeout)
    }
}

/// Starts apps as `nsh test --target` does: an entry on the development
/// host, a packaged app as its own executable.
struct HostLauncher {
    app_dir: PathBuf,
}

/// Whether `app` is a packaged app (a `.app` bundle, an `.exe`, a directory
/// without an `nsh.json`) and not an app's entry.
fn is_packaged(app: &Path) -> bool {
    let ext = app.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext.eq_ignore_ascii_case("app")
        || ext.eq_ignore_ascii_case("exe")
        || (app.is_dir() && !app.join("nsh.json").exists())
}

impl Launch for HostLauncher {
    fn launch(&self, caps: &Caps) -> Result<Box<dyn Session>, WdError> {
        let fail = |message: String| WdError::session_not_created(message);
        let mut options = nsh_testing::HostOptions::default();
        options.storage = caps.storage.clone();
        options.environment = caps.environment.clone();
        let host = if is_packaged(&caps.app) {
            let exe = crate::test_host::packaged_executable(&caps.app)
                .map_err(|e| fail(format!("{e:#}")))?;
            let work = self.app_dir.join(".nsh");
            crate::test_host::launch_devtools(
                caps.target,
                Launchee::Packaged {
                    exe: &exe,
                    work: &work,
                },
                &options,
            )
        } else {
            let entry = crate::entry::resolve(&caps.app).map_err(|e| fail(format!("{e:#}")))?;
            let host =
                crate::test_host::prepare(caps.target).map_err(|e| fail(format!("{e:#}")))?;
            let work = entry.parent().unwrap_or(Path::new(".")).join(".nsh");
            crate::test_host::launch_devtools(
                caps.target,
                Launchee::Entry {
                    host: &host,
                    entry: &entry,
                    work: &work,
                },
                &options,
            )
        }
        .map_err(fail)?;
        Ok(Box::new(HostSession {
            client: host.client(),
            _host: Mutex::new(host),
        }))
    }
}

/// Runs `nsh devtools webdriver` until Ctrl-C, then ends its session.
pub(crate) fn run(host: &str, port: u16, app_dir: &Path) -> anyhow::Result<()> {
    let ip = check_host(host).map_err(|e| anyhow!(e))?;
    let app_dir = dunce::canonicalize(app_dir).unwrap_or_else(|_| app_dir.to_owned());
    let _stop = ui::Stoppable::new();
    // Builds and hosts the server starts end with it, also when it is
    // killed (Windows has no parent-child link otherwise: a killed server
    // left its `cargo build` running).
    #[cfg(windows)]
    if !crate::run::KillOnClose::current_process() {
        ui::warn("cannot put the server in a job: a killed server leaves its builds running");
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("cannot start the server's runtime")?;
    let server = Arc::new(Server::new(
        Box::new(HostLauncher {
            app_dir: app_dir.clone(),
        }),
        app_dir,
    ));
    let served = runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((ip, port))
            .await
            .with_context(|| format!("cannot listen on {ip}:{port}"))?;
        let addr = listener.local_addr()?;
        ui::note(format!(
            "nsh devtools webdriver listening on http://{addr} (experimental)"
        ));
        axum::serve(listener, guard(router(server.clone()), addr.port()))
            .with_graceful_shutdown(async {
                while !ui::stop_requested() {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .context("the server failed")
    });
    // The app of a session its client never deleted closes with the server.
    server.close();
    served
}
