//! `nsh test --target`: the apps tests open run on a real platform's
//! development host instead of a headless engine. Each `open` builds the
//! entry, starts a dev server of its own in a test session
//! (`DevServer::test_session`) serving that build, and starts the host on
//! it; the host's test agent (`nsh_testing::agent`) takes the test's steps
//! at the views the selectors name, with the platform's own input. The
//! host goes when the test closes the app. The web's host is a page in a
//! headless browser, whose agent runs in this process (`test_web.rs`). The
//! macOS host's steps go over its DevTools endpoint (`devtools/`) instead
//! of the dev server's test session. With `--app` the executable that
//! starts is a packaged app built with DevTools (`nsh package --devtools`)
//! instead of a development host: no dev server, the app has its code in
//! its package.

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

/// This process is a `--jobs` run's worker: the run prepared what workers
/// share (the generated workspace, the app's description, the host's build and
/// copy, `.nsh/types`) before it started them, so a worker only reads them
/// and writes nothing shared.
static PREPARED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// This process is a worker of a prepared run ([`PREPARED`]).
pub(crate) fn set_prepared() {
    PREPARED.store(true, std::sync::atomic::Ordering::Release);
}

/// Whether this process is a worker of a prepared run.
pub(crate) fn prepared() -> bool {
    PREPARED.load(std::sync::atomic::Ordering::Acquire)
}

/// The development host `target`'s tests run on, built (and for a Rust app
/// generated and copied) once: what a `--jobs` run does before it starts its
/// workers, which are given the path.
pub(crate) fn prepare(target: Target) -> anyhow::Result<std::path::PathBuf> {
    crate::runtime::test_host(target.platform())
}

/// How long a host may take to start and connect.
const CONNECT: Duration = Duration::from_secs(60);

/// Starts the apps tests open on `target`'s host; `browser` is the one
/// `--target web` runs them in (else the one found on this machine); `fps`
/// has the host log its performance monitor's readings; `device` is the iOS
/// simulator (name or UDID); `display` (one-based)
/// is the display the macOS host opens its windows on (by default a
/// secondary one when there is one); `foreground` lets the macOS host's apps
/// become the active app, which run in the background otherwise; `artifacts`
/// keeps what the apps on a DevTools host leave of a test file's run; `host`
/// is the executable to start, which `packaged` says is a packaged app
/// (`--app`) instead of a development host.
#[allow(
    clippy::too_many_arguments,
    reason = "what starts a host: its target and each flag"
)]
pub(crate) fn starter(
    target: Target,
    browser: Option<&Path>,
    work: &Path,
    fps: bool,
    device: Option<&str>,
    display: Option<usize>,
    foreground: bool,
    artifacts: Option<Arc<crate::devtools::Artifacts>>,
    host: Option<std::path::PathBuf>,
    packaged: bool,
) -> anyhow::Result<StartHost> {
    if browser.is_some() && target != Target::Web {
        anyhow::bail!("--browser goes with --target web");
    }
    if fps
        && !matches!(
            target,
            Target::Linux | Target::Macos | Target::Windows | Target::Ios
        )
    {
        anyhow::bail!("--fps goes with --target linux, macos, windows or ios");
    }
    if device.is_some() && target != Target::Ios {
        anyhow::bail!("--device goes with --target ios");
    }
    if display.is_some() && target != Target::Macos {
        anyhow::bail!("--display goes with --target macos");
    }
    if foreground && target != Target::Macos {
        anyhow::bail!("--foreground goes with --target macos");
    }
    match target {
        Target::Web => return crate::test_web::starter(browser),
        Target::Ios => return crate::test_ios::starter(work, fps, device),
        Target::Android => return crate::test_android::starter(work),
        _ => {}
    }
    if !matches!(
        target,
        Target::Macos | Target::Linux | Target::Windows | Target::Gpui | Target::Tui
    ) {
        anyhow::bail!(
            "nsh test --target takes macos, linux, windows, gpui, tui, web, ios or android"
        );
    }
    // Built (in a checkout) or found once, before the first test.
    let host = match host {
        Some(host) => host,
        None => crate::runtime::test_host(target.platform())?,
    };
    // A worker's icon files go below its own work directory.
    let work = work.to_path_buf();
    Ok(Arc::new(move |entry: &Path, options: &HostOptions| {
        start_host(
            target,
            &host,
            entry,
            options,
            fps,
            display,
            foreground,
            artifacts.as_deref(),
            &work,
            packaged,
        )
        .map(Started::into_host)
    }))
}

/// What `target`'s host honors of `open`'s options, which `open` checks
/// them against: the platform the apps run on there, and whether the host
/// opens them at the size `open` gives (a desktop's window, a browser's
/// page) or they fill a device's screen.
pub(crate) fn host_target(target: Target) -> nsh_testing::HostTarget {
    use nsh_testing::Platform;
    // This machine's desktop: the GPUI and terminal hosts'.
    let desktop = if cfg!(target_os = "macos") {
        Platform::Macos
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };
    let (name, platform, sized) = match target {
        Target::Ios | Target::Catalyst => ("ios", Platform::Ios, false),
        Target::Android => ("android", Platform::Android, false),
        Target::Macos => ("macos", Platform::Macos, true),
        Target::Windows => ("windows", Platform::Windows, true),
        Target::Linux => ("linux", Platform::Linux, true),
        Target::Web => ("web", Platform::Web, true),
        Target::Gpui => ("gpui", desktop, true),
        // The terminal reports this machine's OS, in the terminal idiom.
        Target::Tui => ("tui", desktop, true),
    };
    let mut host = nsh_testing::HostTarget::default();
    host.name = name.to_owned();
    host.platform = platform;
    host.sized = sized;
    host
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

/// What [`start_host`] started.
enum Started {
    /// Driven over its DevTools endpoint.
    Devtools(crate::devtools::DevtoolsHost),
    /// Driven through the dev server's test session.
    Test(TestHost),
}

impl Started {
    fn into_host(self) -> Box<dyn Host> {
        match self {
            Started::Devtools(host) => Box::new(host),
            Started::Test(host) => Box::new(host),
        }
    }
}

/// What [`launch_devtools`] starts.
pub(crate) enum Launchee<'a> {
    /// The app at `entry`, built and served to the development host `host`.
    Entry {
        /// The platform's development host.
        host: &'a Path,
        /// The app's entry.
        entry: &'a Path,
        /// Where the host's generated files go.
        work: &'a Path,
    },
    /// A packaged app (`nsh package --devtools`, as `nsh test --app` runs
    /// it): `exe` is its executable, which has its code and assets itself.
    Packaged {
        /// The packaged app's executable.
        exe: &'a Path,
        /// Where the host's generated files go.
        work: &'a Path,
    },
}

/// Starts the app `launchee` names on `target` over its DevTools endpoint,
/// set up as `options` say, and returns the host that holds it: `nsh devtools
/// launch`, its MCP server and its WebDriver server keep it open for clients.
pub(crate) fn launch_devtools(
    target: Target,
    launchee: Launchee<'_>,
    options: &HostOptions,
) -> Result<crate::devtools::DevtoolsHost, String> {
    if !matches!(target, Target::Macos | Target::Windows) {
        return Err("nsh devtools launch takes --target macos or windows".into());
    }
    let (host, entry, work, packaged) = match launchee {
        Launchee::Entry { host, entry, work } => (host, entry, work, false),
        // Nothing serves a packaged app's entry: the executable stands for it.
        Launchee::Packaged { exe, work } => (exe, exe, work, true),
    };
    let started = start_host(
        target, host, entry, options, false, None, false, None, work, packaged,
    )?;
    let Started::Devtools(host) = started else {
        return Err("the host is not driven over DevTools".into());
    };
    // Dropped on a failure: the host quits with it.
    crate::devtools::wait_ready(host.client().as_ref(), || host.output())?;
    Ok(host)
}

/// Builds `entry`, serves it and starts `host` on it: macOS tests are driven
/// over the host's DevTools endpoint (`devtools/`), the other hosts take
/// their steps from the dev server's test session.
#[allow(
    clippy::too_many_arguments,
    reason = "what starts a host: its target, entry and each flag"
)]
fn start_host(
    target: Target,
    host: &Path,
    entry: &Path,
    options: &HostOptions,
    fps: bool,
    display: Option<usize>,
    foreground: bool,
    artifacts: Option<&crate::devtools::Artifacts>,
    work: &Path,
    packaged: bool,
) -> Result<Started, String> {
    let devtools = matches!(target, Target::Macos | Target::Windows);
    // A packaged app has its code and assets in its package: nothing serves
    // them.
    let server = if packaged {
        None
    } else {
        Some(serve_with(entry, options, !devtools)?)
    };
    // A packaged app has no entry to resolve.
    let entry = if packaged {
        entry.to_path_buf()
    } else {
        resolve(entry)?
    };
    if !packaged
        && matches!(
            target,
            Target::Macos | Target::Linux | Target::Windows | Target::Gpui
        )
    {
        crate::package::note_hidden_main_window(&entry);
    }
    let dir = entry.parent().unwrap_or(Path::new("."));
    // A fresh data directory: what the app keeps (its storage) starts empty
    // in every test, as headless runs' does.
    let data = fresh_data_dir()?;
    let mut command = Command::new(host);
    if let Some(server) = &server {
        let url = format!("ws://{}{WS_PATH}", server.local_addr());
        command
            .env("NSH_DEV_APP_DIR", dir)
            .args(["--dev-server", &url]);
    }
    command.arg("--data-dir").arg(&data).stdin(Stdio::null());
    if target == Target::Tui {
        command.arg("--headless");
        command.arg("--log-file").arg(data.join("host.log"));
        let config = crate::package::read_config(&entry).map_err(|e| e.to_string())?;
        if let Some(cells) = config.tui.points_per_cell {
            crate::run::parse_points_per_cell(&cells)?;
            command.args(["--points-per-cell", &cells]);
        }
    }
    if fps {
        command.arg("--fps");
    }
    if target == Target::Windows && !packaged {
        // The window and its notifications show the app's name and icon.
        // A worker's own directory when workers run at once: the icon file
        // is written for each app, and the host reads it as it starts.
        let icon_dir = if prepared() {
            work.join("windows-icon")
        } else {
            dir.join(".nsh")
        };
        command.args(crate::windows::host_args(&entry, &icon_dir));
    }
    if target == Target::Macos {
        // A window AppKit restored from an earlier run would not be this one.
        command.args(["-ApplePersistenceIgnoreState", "YES"]);
        // Off the main screen, which the user works on: the display
        // `--display` names, else a secondary one when there is one (the
        // host knows the screens).
        let display = display.map_or_else(|| "secondary".to_owned(), |n| n.to_string());
        command.args(["--display", &display]);
        // At the size the test opens the app at, on a display shorter than
        // it too: AppKit would shrink the window to the screen, and the
        // app would lay out in less room than the test expects.
        command.arg("--keep-window-size");
        // Never the active app: the keyboard stays with the app the user
        // types in, and the agent's input goes to the window directly.
        if !foreground {
            command.arg("--background");
        }
        // As the app runs (`--background` takes precedence): a menu bar app
        // run `--foreground` has no Dock icon either. As `nsh run` does, a
        // test runs as a regular app when `nsh.json` does not read (the
        // keys only packages use are checked there too).
        if !packaged {
            let config = crate::package::read_config(&entry).unwrap_or_default();
            command.args(config.macos.host_args());
        }
    }
    if target == Target::Gpui && cfg!(target_os = "linux") && std::env::var_os("DISPLAY").is_some()
    {
        // Screenshots read the window from the X server: under X11
        // (Xwayland on a Wayland desktop).
        command.env_remove("WAYLAND_DISPLAY");
    }
    if let Some(size) = options.size {
        command.args([
            "--window-size",
            &format!("{}x{}", size.width.0, size.height.0),
        ]);
    }
    if devtools {
        let token = crate::devtools::token();
        let port_file = data.join("devtools.port");
        command
            .env(nsh_devtools::ENV_LISTEN, "127.0.0.1:0")
            .env(nsh_devtools::ENV_WAIT, "1")
            .env(nsh_devtools::ENV_TOKEN, &token)
            .env(nsh_devtools::ENV_PORT_FILE, &port_file);
        let launched = crate::devtools::launch(crate::devtools::LaunchSpec {
            command,
            port_file,
            token,
            connect_timeout: CONNECT,
        });
        let launched = match launched {
            Ok(launched) => launched,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&data);
                return Err(e);
            }
        };
        return crate::devtools::DevtoolsHost::start(
            server,
            launched,
            data,
            options,
            artifacts.and_then(|a| a.current()),
        )
        .map(Started::Devtools);
    }
    let Some(server) = server else {
        return Err("a packaged app is driven over DevTools (--target macos or windows)".into());
    };
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
    Ok(Started::Test(TestHost {
        server,
        child,
        data,
    }))
}

/// The executable `nsh test --app <path>` starts: `path` itself when it is
/// a file; a macOS app bundle's (`<App>.app`, its `Contents/MacOS/<exe>`, the
/// `CFBundleExecutable` of its `Info.plist`, else its only file there); a
/// directory's only `.exe` (a Windows package).
pub(crate) fn packaged_executable(path: &Path) -> anyhow::Result<std::path::PathBuf> {
    use anyhow::{Context, bail};

    let shown = path.display();
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    if !path.is_dir() {
        bail!("--app {shown}: no such app (package one with `nsh package --devtools`)");
    }
    let contents = path.join("Contents");
    if contents.is_dir() {
        let macos = contents.join("MacOS");
        let named = plist::Value::from_file(contents.join("Info.plist"))
            .ok()
            .and_then(|info| {
                info.as_dictionary()?
                    .get("CFBundleExecutable")?
                    .as_string()
                    .map(|name| macos.join(name))
            })
            .filter(|exe| exe.is_file());
        if let Some(exe) = named {
            return Ok(exe);
        }
        return only_file(&macos, |_| true)
            .with_context(|| format!("--app {shown}: no executable in Contents/MacOS"));
    }
    only_file(path, |file| {
        file.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    })
    .with_context(|| format!("--app {shown}: no executable (`.exe`) in it"))
}

/// The one file of `dir` that `keep` accepts.
fn only_file(dir: &Path, keep: impl Fn(&Path) -> bool) -> anyhow::Result<std::path::PathBuf> {
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && keep(path))
        .collect();
    if found.len() != 1 {
        anyhow::bail!("{} files in {}, not one", found.len(), dir.display());
    }
    Ok(found.remove(0))
}

/// The entry `open` names (a file, or an app's directory: its entry, or
/// for an app of Rust only its `nsh.json`).
fn resolve(entry: &Path) -> Result<std::path::PathBuf, String> {
    crate::entry::resolve(entry).map_err(|e| format!("{e:#}"))
}

/// A dev server in a test session serving `entry`'s build, as `nsh run`
/// serves it (the app's assets, its log lines shown as `nsh run` shows
/// them); for an app of Rust only, the host's own root, which its
/// development host is built with.
pub(crate) fn serve(entry: &Path, options: &HostOptions) -> Result<DevServer, String> {
    serve_with(entry, options, true)
}

/// [`serve`], in a test session only when `test_session` (else a plain
/// session that delivers the bundle and the root: the test's steps, storage,
/// network and environment go to the app another way, over DevTools).
pub(crate) fn serve_with(
    entry: &Path,
    options: &HostOptions,
    test_session: bool,
) -> Result<DevServer, String> {
    let entry = &resolve(entry)?;
    let root = crate::rust_app::is_root(entry);
    let built = if root {
        None
    } else {
        Some(build::compile(entry, false).map_err(|e| e.plain)?)
    };
    // The media files a literal `asset:/` URL names, from where the build
    // named the imported ones: the app's directory, as a package has them.
    let dir = built
        .as_ref()
        .and_then(|built| built.asset_root.clone())
        .unwrap_or_else(|| entry.parent().unwrap_or(Path::new(".")).to_path_buf());
    let trace = options.trace.clone();
    let server = DevServer::start(
        "127.0.0.1:0".parse().expect("an address"),
        nsh_js::BYTECODE_VERSION,
        Some(dir),
        move |event| match event {
            // The app's log lines, as `nsh run` shows them (a traced app's
            // commit timings among them); the host's own coming and going
            // is the test's business.
            ClientEvent::Log(log) => {
                let level = match log.level {
                    LogLevel::Error => log::Level::Error,
                    LogLevel::Warn => log::Level::Warn,
                    LogLevel::Info => log::Level::Info,
                    LogLevel::Debug => log::Level::Debug,
                    LogLevel::Trace => log::Level::Trace,
                };
                crate::logger::forward(level, &log.target, &log.message);
            }
            ClientEvent::Trace { items } => {
                if let Some(trace) = &trace {
                    trace(items);
                }
            }
            _ => {}
        },
    )
    .map_err(|e| format!("cannot start a dev server: {e}"))?;
    server.serve_files(build::crate_assets().map(|a| (a.path, a.source)));
    if test_session {
        server.test_session();
        // What the app starts with and what answers its requests: the test's.
        server.set_test_storage(options.storage.clone());
        server.set_test_trace(options.trace.is_some());
        server.set_test_environment(options.environment.clone());
        if let Some(network) = &options.network {
            server.set_test_network(network.clone());
        }
    }
    let Some(built) = built else {
        server.publish_root();
        return Ok(server);
    };
    server.serve_files(
        built
            .assets
            .iter()
            .map(|a| (a.path.clone(), a.source.clone())),
    );
    let capabilities = built.capabilities_json();
    server.publish(
        built.bytecode,
        built.code,
        built.source_map,
        AppInfo {
            capabilities,
            id: crate::package::app_id(entry).ok(),
            window: crate::package::window_json(entry),
        },
        None,
        built.ui_sets,
    );
    Ok(server)
}

/// A data directory of its own for an app a test opens: what the app
/// keeps (its storage) starts empty in every test, as headless runs' does.
pub(crate) fn fresh_data_dir() -> Result<std::path::PathBuf, String> {
    let data = std::env::temp_dir().join(format!(
        "nsh-test-{}-{}",
        std::process::id(),
        NEXT_APP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&data).map_err(|e| format!("cannot make {}: {e}", data.display()))?;
    Ok(data)
}

/// The apps started so far: each gets a data directory of its own.
static NEXT_APP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// `text` (the host's output) on lines of its own, if there is any.
pub(crate) fn indented(text: &str) -> String {
    if text.trim().is_empty() {
        String::new()
    } else {
        format!(":\n{}", text.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--foreground` goes with the macOS host alone: the other targets
    /// refuse it before anything is built or started.
    #[test]
    fn foreground_is_for_the_macos_host_only() {
        for target in [
            Target::Linux,
            Target::Windows,
            Target::Gpui,
            Target::Web,
            Target::Ios,
            Target::Android,
        ] {
            let err = starter(
                target,
                None,
                Path::new("."),
                false,
                None,
                None,
                true,
                None,
                None,
                false,
            )
            .err();
            assert_eq!(
                err.map(|e| e.to_string()).as_deref(),
                Some("--foreground goes with --target macos"),
                "{target:?}"
            );
        }
    }

    /// `--device` names an iOS simulator: the other targets refuse it before
    /// anything is built or started.
    #[test]
    fn device_is_for_the_ios_host_only() {
        for target in [
            Target::Macos,
            Target::Linux,
            Target::Windows,
            Target::Gpui,
            Target::Tui,
            Target::Web,
            Target::Android,
        ] {
            let err = starter(
                target,
                None,
                Path::new("."),
                false,
                Some("iPhone 16"),
                None,
                false,
                None,
                None,
                false,
            )
            .err();
            assert_eq!(
                err.map(|e| e.to_string()).as_deref(),
                Some("--device goes with --target ios"),
                "{target:?}"
            );
        }
    }

    /// A scratch directory of this test.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nsh-app-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An `.app` bundle's executable is the one `Info.plist` names, else the
    /// only file of `Contents/MacOS`; a file is itself; a Windows package's
    /// is its `.exe`.
    #[test]
    fn packaged_executable_is_found_in_each_layout() {
        let dir = scratch("layouts");
        let macos = dir.join("Gallery.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(macos.join("Gallery"), "").unwrap();
        std::fs::write(macos.join("helper"), "").unwrap();
        // Two files and no plist: ambiguous.
        assert!(packaged_executable(&dir.join("Gallery.app")).is_err());
        let mut info = plist::Dictionary::new();
        info.insert("CFBundleExecutable".into(), "Gallery".into());
        plist::Value::Dictionary(info)
            .to_file_xml(dir.join("Gallery.app/Contents/Info.plist"))
            .unwrap();
        assert_eq!(
            packaged_executable(&dir.join("Gallery.app")).unwrap(),
            macos.join("Gallery")
        );
        // Without the plist, the only file.
        std::fs::remove_file(macos.join("helper")).unwrap();
        std::fs::remove_file(dir.join("Gallery.app/Contents/Info.plist")).unwrap();
        assert_eq!(
            packaged_executable(&dir.join("Gallery.app")).unwrap(),
            macos.join("Gallery")
        );
        // A file is itself.
        assert_eq!(
            packaged_executable(&macos.join("Gallery")).unwrap(),
            macos.join("Gallery")
        );
        // A Windows package's directory.
        let win = dir.join("win");
        std::fs::create_dir_all(win.join("nsh")).unwrap();
        std::fs::write(win.join("Gallery.exe"), "").unwrap();
        assert_eq!(packaged_executable(&win).unwrap(), win.join("Gallery.exe"));
        // Nothing there.
        assert!(packaged_executable(&dir.join("missing.app")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
