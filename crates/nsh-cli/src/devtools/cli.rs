//! `nsh devtools`: a running app's DevTools endpoint from a shell.
//!
//! `launch` starts an app as `nsh test --target` does (empty setup) and stays
//! in the foreground; the other commands connect to such an app and take one
//! step each. Both this file and the MCP server (`mcp.rs`) are thin users of
//! the protocol through [`Session`].
//!
//! A launch writes `.nsh/devtools/<pid>.json` in the app's directory (mode
//! 0600 on unix: it holds the token) and removes it when it ends, on Ctrl-C
//! and SIGTERM too (`ui::Stoppable`). A launcher killed hard leaves its file
//! behind; `ls` and the other commands skip and remove files whose process is
//! gone.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, anyhow, bail};
use base64::Engine as _;
use clap::{Args, Subcommand};
use nsh_devtools::{
    client::{Client, ERR_CLOSED},
    console::ConsoleEntry,
    protocol::ErrorBody,
    tree::{SnapshotNode, render_compact},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{run::Target, ui};

/// How long a command waits for the endpoint to accept it.
const CONNECT: Duration = Duration::from_secs(5);

/// `nsh devtools`.
#[derive(Args, Clone)]
pub(crate) struct DevtoolsArgs {
    #[command(subcommand)]
    pub(crate) command: DevtoolsCommand,
    /// The app launched by this `nsh devtools launch` process (default: the
    /// only one running).
    #[arg(long, global = true, value_name = "PID")]
    session: Option<u32>,
    /// The directory of the app the sessions are looked up in
    /// (`<DIR>/.nsh/devtools`; default: the current directory); `launch` and
    /// the MCP server's `launch_app` take a relative app path from it.
    #[arg(long, global = true, value_name = "DIR")]
    app_dir: Option<PathBuf>,
    /// Print the protocol's answer as JSON on standard output.
    #[arg(long, global = true)]
    json: bool,
}

impl DevtoolsArgs {
    /// The app `launch` starts (resolved by the caller as other commands'
    /// entries are).
    ///
    /// A relative app is looked for in `--app-dir` when it is given, as the
    /// MCP server's `launch_app` does.
    pub(crate) fn entry_mut(&mut self) -> Option<&mut PathBuf> {
        let dir = self.app_dir.clone();
        match &mut self.command {
            DevtoolsCommand::Launch { app, .. } => {
                if let Some(dir) = dir.filter(|_| app.is_relative()) {
                    *app = dir.join(&*app);
                }
                Some(app)
            }
            _ => None,
        }
    }
}

/// What `nsh devtools` does.
#[derive(Subcommand, Clone)]
pub(crate) enum DevtoolsCommand {
    /// Build an app and start it with its DevTools endpoint (`--target macos`
    /// or `windows`), writing `.nsh/devtools/<pid>.json` for the other
    /// commands. Stays in the foreground; Ctrl-C closes the app.
    Launch {
        /// The app's entry, or its directory (default: the current one).
        #[arg(default_value = ".")]
        app: PathBuf,
        /// The platform's development host (default: this machine's).
        #[arg(long, value_enum, default_value_t = Target::native())]
        target: Target,
    },
    /// List the apps `launch` started that are still running.
    Ls,
    /// The app's views as compact text, one per line.
    Tree {
        /// How many levels below the root (default: all).
        #[arg(long, value_name = "N")]
        depth: Option<u32>,
        /// Where the tree starts: a selector (default: the whole screen).
        #[arg(long, value_name = "SEL")]
        root: Option<String>,
    },
    /// Save a PNG of the app's window.
    Shot {
        /// Where to write the PNG.
        path: PathBuf,
        /// Only this view (a selector), cut out of the window by the host.
        #[arg(long, value_name = "SEL")]
        node: Option<String>,
    },
    /// Press the view the selector names.
    Press {
        /// `label:Add`, `id:save`, ...
        selector: String,
    },
    /// Type text into the input the selector names.
    Type {
        /// The input.
        selector: String,
        /// The text.
        text: String,
    },
    /// Wait until a view the selector names shows (or, with `--gone`, goes).
    Wait {
        /// The view.
        selector: String,
        /// Until none matches.
        #[arg(long)]
        gone: bool,
        /// Milliseconds at most.
        #[arg(long, value_name = "MS", default_value_t = 5000)]
        timeout: u64,
    },
    /// The app's log lines.
    Logs {
        /// Keep printing new lines until Ctrl-C.
        #[arg(short, long)]
        follow: bool,
        /// This level or more severe (`error`, `warn`, `info`, `debug`,
        /// `trace`).
        #[arg(long, value_name = "L")]
        level: Option<String>,
        /// Only this log target (`js`, a Rust module path).
        #[arg(long, value_name = "T")]
        target: Option<String>,
        /// Only lines containing this text.
        #[arg(long, value_name = "TEXT")]
        text: Option<String>,
        /// At most this many, the newest (default: 200).
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Quit the app (and the `launch` that started it).
    Close,
    /// Serve the commands to an AI agent as an MCP server on standard input
    /// and output (JSON-RPC, MCP 2025-06-18).
    Mcp,
    /// Serve apps over the W3C WebDriver protocol (Appium and WebDriver
    /// clients; one session at a time; experimental: it may change). Listens
    /// on loopback only; `--app-dir` resolves a relative `appium:app`.
    Webdriver {
        /// The port to listen on.
        #[arg(long, default_value_t = 4723)]
        port: u16,
        /// The loopback address to listen on.
        #[arg(long, default_value = "127.0.0.1", value_name = "IP")]
        host: String,
    },
}

/// What a command or tool talks to: the protocol of one app.
pub(crate) trait Session: Send + Sync {
    /// Calls `method`, waiting `timeout` for the answer.
    fn call_within(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, ErrorBody>;

    /// Calls `method`.
    fn call(&self, method: &str, params: Value) -> Result<Value, ErrorBody> {
        self.call_within(method, params, Duration::from_secs(60))
    }
}

impl Session for Client {
    fn call_within(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, ErrorBody> {
        Client::call_within(self, method, params, timeout)
    }
}

/// How long a launched app may take to render for the first time.
const READY: Duration = Duration::from_secs(60);

/// Waits until the app rendered and is quiet (`Session.ready`, as `nsh test`'s
/// `open` does): before that its tree is empty and nothing is pressable.
/// `output` is the host's captured output, shown when it fails.
pub(crate) fn wait_ready(
    session: &dyn Session,
    output: impl FnOnce() -> String,
) -> Result<(), String> {
    session
        // The app waits up to `READY`; the call allows a little more, so a slow
        // start reports the app's own timeout, with its message.
        .call_within(
            "Session.ready",
            json!({ "timeout_ms": READY.as_millis() as u64 }),
            READY + Duration::from_secs(5),
        )
        .map(|_| ())
        .map_err(|e| {
            format!(
                "the app did not become ready: {}{}",
                e.message,
                crate::test_host::indented(&output())
            )
        })
}

/// `Tree.snapshot` as compact text.
pub(crate) fn snapshot_text(
    session: &dyn Session,
    root: Option<&str>,
    depth: Option<u32>,
) -> Result<String, String> {
    let answer = session
        .call(
            "Tree.snapshot",
            json!({ "root": root, "depth": depth, "hidden": false }),
        )
        .map_err(|e| e.message)?;
    let nodes: Vec<SnapshotNode> = serde_json::from_value(answer).map_err(|e| e.to_string())?;
    Ok(render_compact(&nodes))
}

/// A PNG of the app's window, or of the view the selector `node` names cut
/// out of it by the host.
pub(crate) fn screenshot_png(session: &dyn Session, node: Option<&str>) -> Result<Vec<u8>, String> {
    let params = match node {
        None => Value::Null,
        Some(selector) => {
            let found = session
                .call("Tree.query", json!({ "selector": selector }))
                .map_err(|e| e.message)?;
            let id = found["ids"]
                .as_array()
                .and_then(|ids| ids.first())
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("no view matches {selector}"))?;
            json!({ "node": id })
        }
    };
    let answer = session
        .call("Capture.screenshot", params)
        .map_err(|e| e.message)?;
    let text = answer
        .as_str()
        .ok_or("the screenshot is not a base64 string")?;
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|e| format!("the screenshot is not base64: {e}"))
}

/// One log line as text: `time level [target] message`.
pub(crate) fn console_line(entry: &ConsoleEntry) -> String {
    format!(
        "{} {:<5} [{}] {}",
        entry.time_ms, entry.level, entry.target, entry.message
    )
}

/// A session as printed: the token stays in the session file.
fn public(session: &SessionFile) -> Value {
    json!({
        "pid": session.pid,
        "url": session.url,
        "app": session.app,
        "target": session.target,
        "started": session.started,
    })
}

/// What a launched app leaves for the other commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SessionFile {
    /// The `nsh devtools launch` process.
    pub(crate) pid: u32,
    /// The endpoint's URL.
    pub(crate) url: String,
    /// The endpoint's token.
    pub(crate) token: String,
    /// The app's directory.
    pub(crate) app: String,
    /// The platform it runs on.
    pub(crate) target: String,
    /// Seconds since the Unix epoch.
    pub(crate) started: u64,
}

impl SessionFile {
    /// Connects to the app.
    pub(crate) fn connect(&self) -> Result<Client, String> {
        Client::connect(&self.url, &self.token, CONNECT).map_err(|e| {
            format!(
                "cannot reach the app of session {} ({}): {e}",
                self.pid, self.app
            )
        })
    }
}

/// `<app dir>/.nsh/devtools`.
pub(crate) fn sessions_dir(app_dir: &Path) -> PathBuf {
    app_dir.join(".nsh").join("devtools")
}

/// Whether a process with this id runs.
fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: signal 0 only checks that the process exists.
        let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
        r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        };
        const STILL_ACTIVE: u32 = 259;
        // SAFETY: the handle is closed here, and the exit code is written to
        // a local.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(handle, &mut code);
            CloseHandle(handle);
            ok != 0 && code == STILL_ACTIVE
        }
    }
}

/// Writes `session` for the other commands (readable by its owner only).
fn write_session(dir: &Path, session: &SessionFile) -> anyhow::Result<PathBuf> {
    use std::io::Write as _;

    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let path = dir.join(format!("{}.json", session.pid));
    let temp = dir.join(format!("{}.json.tmp", session.pid));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(&temp)
        .with_context(|| format!("cannot write {}", temp.display()))?;
    file.write_all(serde_json::to_string_pretty(session)?.as_bytes())?;
    drop(file);
    std::fs::rename(&temp, &path).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

/// The sessions of the app in `app_dir` whose launcher still runs, by pid;
/// the files of those that ended are removed.
pub(crate) fn live_sessions(app_dir: &Path) -> Vec<SessionFile> {
    let Ok(entries) = std::fs::read_dir(sessions_dir(app_dir)) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let session = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<SessionFile>(&t).ok());
        match session {
            Some(session) if pid_alive(session.pid) => found.push(session),
            _ => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    found.sort_by_key(|s| s.pid);
    found
}

/// The session `pid` names, or the only one.
fn pick_session(app_dir: &Path, pid: Option<u32>) -> Result<SessionFile, String> {
    let mut sessions = live_sessions(app_dir);
    let dir = sessions_dir(app_dir);
    match (pid, sessions.len()) {
        (Some(pid), _) => sessions
            .into_iter()
            .find(|s| s.pid == pid)
            .ok_or_else(|| format!("no running session {pid} in {}", dir.display())),
        (None, 0) => Err(format!(
            "no app is running with DevTools in {} (start one with `nsh devtools launch`, or name \
             its directory with --app-dir)",
            dir.display()
        )),
        (None, 1) => Ok(sessions.remove(0)),
        (None, _) => {
            let list: Vec<String> = sessions
                .iter()
                .map(|s| format!("{} ({})", s.pid, s.app))
                .collect();
            Err(format!(
                "several apps are running: {}; pick one with --session <pid>",
                list.join(", ")
            ))
        }
    }
}

fn app_dir_of(args: &DevtoolsArgs) -> PathBuf {
    args.app_dir.clone().unwrap_or_else(|| PathBuf::from("."))
}

/// Runs `nsh devtools`.
pub(crate) fn run(args: DevtoolsArgs) -> anyhow::Result<()> {
    let app_dir = app_dir_of(&args);
    let json = args.json;
    match args.command {
        DevtoolsCommand::Launch { app, target } => launch(&app, target, json),
        DevtoolsCommand::Mcp => super::mcp::serve(&app_dir),
        DevtoolsCommand::Webdriver { host, port } => super::webdriver::run(&host, port, &app_dir),
        DevtoolsCommand::Ls => {
            let sessions = live_sessions(&app_dir);
            if json {
                let shown: Vec<Value> = sessions.iter().map(public).collect();
                println!("{}", serde_json::to_string_pretty(&shown)?);
            } else if sessions.is_empty() {
                ui::note(format!(
                    "no app is running with DevTools in {}",
                    sessions_dir(&app_dir).display()
                ));
            } else {
                for s in sessions {
                    println!("{}  {}  {}  {}", s.pid, s.target, s.url, s.app);
                }
            }
            Ok(())
        }
        command => {
            let session = pick_session(&app_dir, args.session).map_err(|e| anyhow!(e))?;
            let client = session.connect().map_err(|e| anyhow!(e))?;
            act(&client, command, json)
        }
    }
}

/// Starts the app, publishes the session and waits for a stop.
fn launch(app: &Path, target: Target, json: bool) -> anyhow::Result<()> {
    if !matches!(target, Target::Macos | Target::Windows) {
        bail!("nsh devtools launch takes --target macos or windows");
    }
    let app_dir = app.parent().unwrap_or(Path::new(".")).to_owned();
    let app_dir = dunce::canonicalize(&app_dir).unwrap_or(app_dir);
    let _stop = ui::Stoppable::new();
    let host_exe = crate::test_host::prepare(target)?;
    let step = ui::Step::start(format!("starting {}", ui::path(app)));
    let mut host = crate::test_host::launch_devtools(
        target,
        crate::test_host::Launchee::Entry {
            host: &host_exe,
            entry: app,
            work: &app_dir.join(".nsh"),
        },
        &Default::default(),
    )
    .map_err(|e| anyhow!(e))?;
    step.done(format!("{} is running", ui::path(app)));
    let session = SessionFile {
        pid: std::process::id(),
        url: host.url().to_owned(),
        token: host.token().to_owned(),
        app: app_dir.display().to_string(),
        target: format!("{target:?}").to_lowercase(),
        started: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    };
    let path = write_session(&sessions_dir(&app_dir), &session)?;
    /// Removes the session file however the launch ends.
    struct Remove(PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _remove = Remove(path);
    if json {
        println!("{}", public(&session));
    } else {
        println!("{}", session.url);
    }
    ui::note(format!(
        "session {}; `nsh devtools tree`, `press`, `shot`, `logs` drive it from another terminal; \
         Ctrl-C closes the app",
        session.pid
    ));
    while !ui::stop_requested() {
        if let Some(clean) = host.exit() {
            // `close` (or the app's own quit) ends it cleanly: not a failure.
            if clean {
                ui::done("the app was closed");
                return Ok(());
            }
            bail!(
                "the app ended{}",
                crate::test_host::indented(&host.output())
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

/// Takes the step `command` is.
fn act(client: &Client, command: DevtoolsCommand, json: bool) -> anyhow::Result<()> {
    let call = |method: &str, params: Value| -> anyhow::Result<Value> {
        client
            .call(method, params)
            .map_err(|e| anyhow!("{method}: {}", e.message))
    };
    let print_answer = |what: String, answer: &Value| -> anyhow::Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(answer)?);
        } else {
            ui::done(what);
        }
        Ok(())
    };
    match command {
        DevtoolsCommand::Tree { depth, root } => {
            if json {
                let answer = call(
                    "Tree.snapshot",
                    json!({ "root": root, "depth": depth, "hidden": false }),
                )?;
                println!("{}", serde_json::to_string_pretty(&answer)?);
            } else {
                let text = snapshot_text(client, root.as_deref(), depth).map_err(|e| anyhow!(e))?;
                print!("{text}");
            }
        }
        DevtoolsCommand::Shot { path, node } => {
            let png = screenshot_png(client, node.as_deref()).map_err(|e| anyhow!(e))?;
            std::fs::write(&path, &png)
                .with_context(|| format!("cannot write {}", path.display()))?;
            print_answer(
                format!("saved {}", ui::path(&path)),
                &json!({ "path": path, "bytes": png.len() }),
            )?;
        }
        DevtoolsCommand::Press { selector } => {
            let answer = call("Input.press", json!({ "selector": selector }));
            let _ = client.call("Session.release", Value::Null);
            let answer = answer?;
            print_answer(format!("pressed {selector}"), &answer)?;
        }
        DevtoolsCommand::Type { selector, text } => {
            let answer = call("Input.type", json!({ "selector": selector, "text": text }));
            let _ = client.call("Session.release", Value::Null);
            let answer = answer?;
            print_answer(format!("typed into {selector}"), &answer)?;
        }
        DevtoolsCommand::Wait {
            selector,
            gone,
            timeout,
        } => {
            let answer = client.call_within(
                "State.waitFor",
                json!({ "selector": selector, "gone": gone, "timeout_ms": timeout }),
                Duration::from_millis(timeout) + Duration::from_secs(10),
            );
            let _ = client.call("Session.release", Value::Null);
            let answer = answer.map_err(|e| anyhow!("State.waitFor: {}", e.message))?;
            let what = if gone { "gone" } else { "shown" };
            print_answer(format!("{selector} is {what}"), &answer)?;
        }
        DevtoolsCommand::Logs {
            follow,
            level,
            target,
            text,
            limit,
        } => logs(client, follow, level, target, text, limit, json)?,
        DevtoolsCommand::Close => {
            match client.call_within("Session.quit", Value::Null, Duration::from_secs(5)) {
                Ok(_) => {}
                // The app went away as it answered.
                Err(e) if e.code == ERR_CLOSED => {}
                Err(e) => bail!("Session.quit: {}", e.message),
            }
            print_answer("closed".into(), &json!({}))?;
        }
        DevtoolsCommand::Launch { .. }
        | DevtoolsCommand::Ls
        | DevtoolsCommand::Mcp
        | DevtoolsCommand::Webdriver { .. } => {
            unreachable!("handled by `run`")
        }
    }
    Ok(())
}

/// Whether `entry` passes the filters `follow` applies to the live lines.
fn passes(
    entry: &ConsoleEntry,
    level: Option<log::Level>,
    target: Option<&str>,
    text: Option<&str>,
) -> bool {
    level.is_none_or(|max| {
        entry
            .level
            .parse::<log::Level>()
            .is_ok_and(|level| level <= max)
    }) && target.is_none_or(|t| entry.target == t)
        && text.is_none_or(|t| entry.message.contains(t))
}

fn logs(
    client: &Client,
    follow: bool,
    level: Option<String>,
    target: Option<String>,
    text: Option<String>,
    limit: Option<usize>,
    json: bool,
) -> anyhow::Result<()> {
    let parsed = level
        .as_deref()
        .map(|l| {
            l.parse::<log::Level>()
                .map_err(|_| anyhow!("--level takes error, warn, info, debug or trace"))
        })
        .transpose()?;
    let events = follow.then(|| client.events());
    if follow {
        client
            .call("Console.subscribe", Value::Null)
            .map_err(|e| anyhow!("Console.subscribe: {}", e.message))?;
    }
    let answer = client
        .call(
            "Console.query",
            json!({ "level": level, "target": target, "text": text, "limit": limit.unwrap_or(200) }),
        )
        .map_err(|e| anyhow!("Console.query: {}", e.message))?;
    let entries: Vec<ConsoleEntry> =
        serde_json::from_value(answer["entries"].clone()).unwrap_or_default();
    let print = |entry: &ConsoleEntry| {
        if json {
            if let Ok(line) = serde_json::to_string(entry) {
                println!("{line}");
            }
        } else {
            println!("{}", console_line(entry));
        }
    };
    if json && !follow {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        entries.iter().for_each(print);
    }
    let Some(events) = events else {
        return Ok(());
    };
    // What the query printed, which a line of the stream may repeat: by time
    // and message, as several lines share a millisecond.
    let shown: std::collections::HashSet<(u64, String)> = entries
        .iter()
        .map(|e| (e.time_ms, e.message.clone()))
        .collect();
    let _stop = ui::Stoppable::new();
    let mut quiet = 0u32;
    while !ui::stop_requested() {
        match events.recv_timeout(Duration::from_millis(200)) {
            Ok(event) if event.method == "Console.entry" => {
                let Ok(entry) = serde_json::from_value::<ConsoleEntry>(event.params) else {
                    continue;
                };
                // The query showed it already.
                if shown.contains(&(entry.time_ms, entry.message.clone()))
                    || !passes(&entry, parsed, target.as_deref(), text.as_deref())
                {
                    continue;
                }
                print(&entry);
            }
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // The receiver outlives the connection: ask the app now and
                // then, so that following ends with it.
                quiet += 1;
                if quiet.is_multiple_of(10)
                    && client
                        .call_within("Session.hello", Value::Null, Duration::from_secs(2))
                        .is_err_and(|e| e.code == ERR_CLOSED)
                {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}
