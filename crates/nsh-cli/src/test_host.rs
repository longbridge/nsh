//! `nsh test --target`: the apps tests open run on a real platform's
//! development host instead of a headless engine. Each `open` builds the
//! entry, starts a dev server of its own in a test session
//! (`DevServer::test_session`) serving that build, and starts the host on
//! it; the host's test agent (`nsh_testing::agent`) takes the test's steps
//! at the views the selectors name, with the platform's own input. The
//! host goes when the test closes the app.

use std::{
    path::Path,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use nsh_dev::{
    AppInfo, ClientEvent, DevServer,
    protocol::{LogLevel, WS_PATH},
};
use nsh_testing::{Host, HostOptions, StartHost};

use crate::{build, run::Target, ui};

/// How long a host may take to start and connect.
const CONNECT: Duration = Duration::from_secs(60);

/// Starts the apps tests open on `target`'s host.
pub(crate) fn starter(target: Target) -> anyhow::Result<StartHost> {
    if target != Target::Macos {
        anyhow::bail!(
            "nsh test --target runs on macos for now (the other platforms' test agents follow)"
        );
    }
    // Built (in a checkout) or found once, before the first test.
    let host = crate::runtime::file(target.platform(), false)?;
    Ok(Arc::new(move |entry: &Path, options: &HostOptions| {
        start(&host, entry, options).map(|h| Box::new(h) as Box<dyn Host>)
    }))
}

/// A host running one app for a test.
struct TestHost {
    server: DevServer,
    child: Child,
    /// The app's data directory, fresh for it and removed after it.
    data: std::path::PathBuf,
}

impl Host for TestHost {
    fn step(
        &self,
        step: nsh_dev::protocol::TestStep,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        self.server.step(step, timeout)
    }
}

impl Drop for TestHost {
    fn drop(&mut self) {
        // It quits after the test's `Quit`; a host that does not goes.
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.data);
    }
}

/// Builds `entry`, serves it in a test session and starts `host` on it.
fn start(host: &Path, entry: &Path, options: &HostOptions) -> Result<TestHost, String> {
    let built = build::compile(entry, false).map_err(|e| e.plain)?;
    let dir = entry.parent().unwrap_or(Path::new("."));
    let server = DevServer::start(
        "127.0.0.1:0".parse().expect("an address"),
        nsh_js::BYTECODE_VERSION,
        Some(dir.to_path_buf()),
        |event| {
            // The app's log lines, as `nsh run` shows them; the host's own
            // coming and going is the test's business.
            if let ClientEvent::Log(log) = event {
                let level = match log.level {
                    LogLevel::Error => log::Level::Error,
                    LogLevel::Warn => log::Level::Warn,
                    LogLevel::Info => log::Level::Info,
                    LogLevel::Debug => log::Level::Debug,
                    LogLevel::Trace => log::Level::Trace,
                };
                crate::logger::forward(level, &log.target, &log.message);
            }
        },
    )
    .map_err(|e| format!("cannot start a dev server: {e}"))?;
    server.serve_files(build::crate_assets().map(|a| (a.path, a.source)));
    server.test_session();
    let capabilities = built.capabilities_json();
    server.publish(
        built.bytecode,
        built.code,
        built.source_map,
        AppInfo {
            capabilities,
            id: crate::package::app_id(entry).ok(),
        },
        None,
        built.ui_sets,
    );
    let url = format!("ws://{}{WS_PATH}", server.local_addr());
    // A fresh data directory: what the app keeps (its storage) starts empty
    // in every test, as headless runs' does.
    let data = std::env::temp_dir().join(format!(
        "nsh-test-{}-{}",
        std::process::id(),
        NEXT_APP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&data).map_err(|e| format!("cannot make {}: {e}", data.display()))?;
    let mut command = Command::new(host);
    command
        .env("NSH_DEV_APP_DIR", dir)
        .args(["--dev-server", &url])
        .arg("--data-dir")
        .arg(&data)
        // A window restored from an earlier run would not be this one.
        .args(["-ApplePersistenceIgnoreState", "YES"])
        .stdin(Stdio::null());
    if let Some(size) = options.size {
        command.args([
            "--window-size",
            &format!("{}x{}", size.width.0, size.height.0),
        ]);
    }
    let (mut child, output) =
        ui::quiet(&mut command).map_err(|e| format!("cannot start {}: {e}", host.display()))?;
    let deadline = Instant::now() + CONNECT;
    while !server.has_tester() {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "the host exited with {status} before it connected{}",
                indented(&output.text())
            ));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err(format!(
                "the host did not connect in {} s{}",
                CONNECT.as_secs(),
                indented(&output.text())
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(TestHost {
        server,
        child,
        data,
    })
}

/// The apps started so far: each gets a data directory of its own.
static NEXT_APP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// `text` (the host's output) on lines of its own, if there is any.
fn indented(text: &str) -> String {
    if text.trim().is_empty() {
        String::new()
    } else {
        format!(":\n{}", text.trim_end())
    }
}
