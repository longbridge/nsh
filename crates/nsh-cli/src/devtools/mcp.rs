//! `nsh devtools mcp`: the DevTools protocol as an MCP server for AI agents.
//!
//! Hand-written JSON-RPC 2.0 over standard input and output, one message per
//! line (MCP 2025-06-18's stdio transport): `initialize`,
//! `notifications/initialized`, `ping`, `tools/list` and `tools/call`.
//! Standard output carries nothing else: the CLI's own output goes to
//! standard error, and `ui::reserve_stdout` makes every child process the
//! server starts (cargo builds, the app's host) have its output captured
//! instead of inherited, so `-v` shows nothing in this mode. The dev server
//! runs in this process. Every tool is a thin use of the protocol through
//! [`Session`], so the tests drive the server with a fake one.
//!
//! The server owns the apps `launch_app` starts (closed with `close_app` or
//! when standard input ends) and sees those `nsh devtools launch` started in
//! the app directory.

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use base64::Engine as _;
use clap::ValueEnum as _;
use serde_json::{Map, Value, json};

use super::{
    DevtoolsHost,
    cli::{Session, console_line, live_sessions, screenshot_png, sessions_dir, snapshot_text},
};
use crate::run::Target;

/// The MCP revision the server speaks.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// Text answers are cut at this many characters.
const TEXT_LIMIT: usize = 20_000;

/// The line a cut answer ends with.
const CUT_NOTE: &str = "… cut; ask with depth / root / limit for less";

/// How many log lines `list_console` returns when it is not told.
const DEFAULT_LOG_LIMIT: usize = 200;

/// Runs the server on this process's standard input and output until the
/// input ends.
pub(crate) fn serve(app_dir: &Path) -> anyhow::Result<()> {
    crate::ui::reserve_stdout();
    let mut server = Server::new(app_dir.to_owned());
    let stdin = std::io::stdin().lock();
    let stdout = std::io::stdout().lock();
    run(&mut server, stdin, stdout)?;
    Ok(())
}

/// Answers the messages of `input` on `output` until `input` ends.
fn run(server: &mut Server, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => server.handle(&message),
            Err(e) => Some(error(Value::Null, -32700, format!("parse error: {e}"))),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// The app the tools act on.
struct Current {
    pid: u32,
    session: Arc<dyn Session>,
}

/// An app this server started.
struct Owned {
    pid: u32,
    host: DevtoolsHost,
}

struct Server {
    /// Where the apps `nsh devtools launch` started are looked up.
    app_dir: PathBuf,
    current: Option<Current>,
    owned: Vec<Owned>,
}

/// A tool's answer: text, or a picture.
enum Answer {
    Text(String),
    Png(Vec<u8>),
}

impl Server {
    fn new(app_dir: PathBuf) -> Server {
        Server {
            app_dir,
            current: None,
            owned: Vec::new(),
        }
    }

    /// The reply to `message`, none for a notification.
    fn handle(&mut self, message: &Value) -> Option<Value> {
        // A message without an id is a notification (or a response): never
        // answered.
        let id = message.get("id")?.clone();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Some(error(id, -32600, "invalid request: no method".into()));
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        Some(match method {
            "initialize" => result(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "nsh-devtools", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": "Drive a running NSH app: launch_app (or select_app), then \
                        take_snapshot to see its views and click / type / press_key to act on them.",
                }),
            ),
            "ping" => result(id, json!({})),
            "tools/list" => result(id, json!({ "tools": tools() })),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(error(id, -32602, "tools/call needs a name".into()));
                };
                if !tools().iter().any(|t| t["name"] == name) {
                    return Some(error(id, -32602, format!("unknown tool {name}")));
                }
                let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                result(id, tool_result(self.call(name, &args)))
            }
            _ => error(id, -32601, format!("method not found: {method}")),
        })
    }

    fn session(&self) -> Result<&Arc<dyn Session>, String> {
        self.current
            .as_ref()
            .map(|c| &c.session)
            .ok_or_else(|| "no app is selected: launch_app, or select_app one of list_apps".into())
    }

    /// Runs the tool `name` with `args`.
    fn call(&mut self, name: &str, args: &Value) -> Result<Answer, String> {
        let text = |s: String| Ok(Answer::Text(s));
        // The tools that reach into the protocol: a method and its params.
        let act = |server: &Server,
                   method: &str,
                   params: Value,
                   done: String|
         -> Result<Answer, String> {
            let session = server.session()?;
            let outcome = session.call(method, params);
            release(session.as_ref());
            outcome.map_err(|e| e.message)?;
            Ok(Answer::Text(done))
        };
        match name {
            "list_apps" => text(self.list_apps()),
            "launch_app" => self.launch_app(&str_arg(args, "app")?, opt_str(args, "target")),
            "select_app" => self.select_app(uint_arg(args, "pid")? as u32),
            "close_app" => self.close_app(),
            "take_snapshot" => {
                let depth = opt_uint(args, "depth").map(|d| d as u32);
                snapshot_text(
                    self.session()?.as_ref(),
                    opt_str(args, "root").as_deref(),
                    depth,
                )
                .map(Answer::Text)
            }
            "take_screenshot" => {
                screenshot_png(self.session()?.as_ref(), opt_str(args, "node").as_deref())
                    .map(Answer::Png)
            }
            "click" => {
                let selector = str_arg(args, "selector")?;
                act(
                    self,
                    "Input.press",
                    json!({ "selector": selector }),
                    format!("pressed {selector}"),
                )
            }
            "type" => {
                let selector = str_arg(args, "selector")?;
                let text = str_arg(args, "text")?;
                act(
                    self,
                    "Input.type",
                    json!({ "selector": selector, "text": text }),
                    format!("typed into {selector}"),
                )
            }
            "press_key" => {
                let keys = str_arg(args, "keys")?;
                act(
                    self,
                    "Input.pressKey",
                    json!({ "keys": keys }),
                    format!("pressed {keys}"),
                )
            }
            "scroll" => {
                let selector = str_arg(args, "selector")?;
                let distance = num_arg(args, "distance")?;
                act(
                    self,
                    "Input.scroll",
                    json!({ "selector": selector, "distance": distance }),
                    format!("scrolled {selector} by {distance}"),
                )
            }
            "hover" => {
                let selector = str_arg(args, "selector")?;
                act(
                    self,
                    "Input.hover",
                    json!({ "selector": selector }),
                    format!("hovered {selector}"),
                )
            }
            "drag" => {
                let selector = str_arg(args, "selector")?;
                let (x, y) = (num_arg(args, "x")?, num_arg(args, "y")?);
                act(
                    self,
                    "Input.drag",
                    json!({ "selector": selector, "x": x, "y": y }),
                    format!("dragged {selector} by {x}, {y}"),
                )
            }
            "wait_for" => {
                let selector = str_arg(args, "selector")?;
                let gone = args.get("gone").and_then(Value::as_bool).unwrap_or(false);
                let timeout = opt_uint(args, "timeout_ms").unwrap_or(5000);
                let session = self.session()?;
                let outcome = session.call_within(
                    "State.waitFor",
                    json!({ "selector": selector, "gone": gone, "timeout_ms": timeout }),
                    Duration::from_millis(timeout) + Duration::from_secs(10),
                );
                release(session.as_ref());
                outcome.map_err(|e| e.message)?;
                text(format!(
                    "{selector} is {}",
                    if gone { "gone" } else { "shown" }
                ))
            }
            "list_console" => {
                let limit = opt_uint(args, "limit").map_or(DEFAULT_LOG_LIMIT, |l| l as usize);
                let answer = self
                    .session()?
                    .call(
                        "Console.query",
                        json!({
                            "level": opt_str(args, "level"),
                            "target": opt_str(args, "target"),
                            "text": opt_str(args, "text"),
                            "limit": limit,
                        }),
                    )
                    .map_err(|e| e.message)?;
                let entries: Vec<nsh_devtools::console::ConsoleEntry> =
                    serde_json::from_value(answer["entries"].clone()).unwrap_or_default();
                if entries.is_empty() {
                    return text("no log lines".into());
                }
                text(
                    entries
                        .iter()
                        .map(console_line)
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            }
            "list_errors" => {
                let answer = self
                    .session()?
                    .call("Console.takeErrors", Value::Null)
                    .map_err(|e| e.message)?;
                let lines: Vec<String> = answer
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|e| e.as_str().map_or_else(|| e.to_string(), str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                if lines.is_empty() {
                    return text("no errors".into());
                }
                text(lines.join("\n"))
            }
            other => Err(format!("unknown tool {other}")),
        }
    }

    fn list_apps(&self) -> String {
        let selected = self.current.as_ref().map(|c| c.pid);
        let mark = |pid: u32| if selected == Some(pid) { "*" } else { " " };
        let mut lines: Vec<String> = self
            .owned
            .iter()
            .map(|o| format!("{} {}  (launched by this server)", mark(o.pid), o.pid))
            .collect();
        lines.extend(live_sessions(&self.app_dir).iter().map(|s| {
            format!(
                "{} {}  {}  {}  (nsh devtools launch)",
                mark(s.pid),
                s.pid,
                s.target,
                s.app
            )
        }));
        if lines.is_empty() {
            return format!(
                "no apps: launch_app starts one ({} has none from `nsh devtools launch`)",
                sessions_dir(&self.app_dir).display()
            );
        }
        lines.push("* is the selected app".into());
        lines.join("\n")
    }

    /// A relative app path is the app directory's (`--app-dir`, else the
    /// current directory).
    fn app_path(&self, app: &str) -> PathBuf {
        let path = Path::new(app);
        if path.is_relative() {
            self.app_dir.join(path)
        } else {
            path.to_owned()
        }
    }

    fn launch_app(&mut self, app: &str, target: Option<String>) -> Result<Answer, String> {
        let target = match target {
            Some(name) => Target::from_str(&name, true)
                .map_err(|_| format!("unknown target {name}: macos or windows"))?,
            None => Target::native(),
        };
        let entry = crate::entry::resolve(&self.app_path(app)).map_err(|e| format!("{e:#}"))?;
        let host_exe = crate::test_host::prepare(target).map_err(|e| format!("{e:#}"))?;
        let work = entry.parent().unwrap_or(Path::new(".")).join(".nsh");
        let host = crate::test_host::launch_devtools(target, &host_exe, &entry, &work)?;
        let pid = host.pid();
        self.current = Some(Current {
            pid,
            session: host.client(),
        });
        self.owned.push(Owned { pid, host });
        Ok(Answer::Text(format!(
            "launched {app} (pid {pid}); it is the selected app"
        )))
    }

    fn select_app(&mut self, pid: u32) -> Result<Answer, String> {
        if let Some(previous) = &self.current {
            release(previous.session.as_ref());
        }
        if let Some(owned) = self.owned.iter().find(|o| o.pid == pid) {
            self.current = Some(Current {
                pid,
                session: owned.host.client(),
            });
        } else {
            let session = live_sessions(&self.app_dir)
                .into_iter()
                .find(|s| s.pid == pid)
                .ok_or_else(|| format!("no app {pid}: see list_apps"))?;
            self.current = Some(Current {
                pid,
                session: Arc::new(session.connect()?),
            });
        }
        Ok(Answer::Text(format!("selected {pid}")))
    }

    fn close_app(&mut self) -> Result<Answer, String> {
        let current = self.current.take().ok_or("no app is selected")?;
        release(current.session.as_ref());
        if let Some(at) = self.owned.iter().position(|o| o.pid == current.pid) {
            // Its drop quits the app and ends its host.
            drop(self.owned.remove(at));
        } else {
            let _ =
                current
                    .session
                    .call_within("Session.quit", Value::Null, Duration::from_secs(5));
        }
        Ok(Answer::Text(format!("closed {}", current.pid)))
    }
}

/// Lets other clients act on the app: the endpoint gives the input to the
/// connection that acted last, and this one lives as long as the agent's
/// session (the error, an app that is gone, is no news).
fn release(session: &dyn Session) {
    let _ = session.call("Session.release", Value::Null);
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i32, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// `text` cut at [`TEXT_LIMIT`] characters, with the note.
fn cut(text: String) -> String {
    match text.char_indices().nth(TEXT_LIMIT) {
        None => text,
        Some((end, _)) => format!("{}\n{CUT_NOTE}", &text[..end]),
    }
}

/// A `tools/call` result: the answer, or a failure the agent reads and acts
/// on (`isError`, as MCP asks for what a tool itself cannot do).
fn tool_result(outcome: Result<Answer, String>) -> Value {
    match outcome {
        Ok(Answer::Text(text)) => json!({ "content": [{ "type": "text", "text": cut(text) }] }),
        Ok(Answer::Png(png)) => json!({
            "content": [{
                "type": "image",
                "data": base64::engine::general_purpose::STANDARD.encode(png),
                "mimeType": "image/png",
            }]
        }),
        Err(message) => json!({
            "content": [{ "type": "text", "text": cut(message) }],
            "isError": true,
        }),
    }
}

fn str_arg(args: &Value, name: &str) -> Result<String, String> {
    opt_str(args, name).ok_or_else(|| format!("the argument {name} (a string) is missing"))
}

fn opt_str(args: &Value, name: &str) -> Option<String> {
    args.get(name)?.as_str().map(str::to_owned)
}

fn num_arg(args: &Value, name: &str) -> Result<f64, String> {
    args.get(name)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("the argument {name} (a number) is missing"))
}

fn opt_uint(args: &Value, name: &str) -> Option<u64> {
    args.get(name)?.as_u64()
}

fn uint_arg(args: &Value, name: &str) -> Result<u64, String> {
    opt_uint(args, name).ok_or_else(|| format!("the argument {name} (an integer) is missing"))
}

/// One tool's listing: its name, what it does and its input's JSON Schema.
fn tool(name: &str, description: &str, props: &[(&str, &str, &str)], required: &[&str]) -> Value {
    let mut properties = Map::new();
    for (prop, kind, doc) in props {
        properties.insert((*prop).into(), json!({ "type": kind, "description": doc }));
    }
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
        },
    })
}

/// Every tool.
fn tools() -> Vec<Value> {
    const SEL: &str = "A selector naming a view. A bare text (`Add`, `+1`) matches a test id, \
        else a label, else a shown text, as written. Prefixes pick one kind: `id:<testID>`, \
        `label:<accessibility label>`, `text:<shown text>`, `role:<role>` or \
        `role:<role>[<name>]`, `view:<node id>` (as take_snapshot's `[id]`); `/regex/i` \
        works after `label:` and `text:`. `A >> B` looks for B inside A, `nth=<n>` picks one \
        match. take_snapshot shows ids and labels; `label:` matches only an accessibility \
        label, so when unsure use the bare text.";
    vec![
        tool(
            "list_apps",
            "List the apps this server launched and those `nsh devtools launch` started; the \
             selected one is marked.",
            &[],
            &[],
        ),
        tool(
            "launch_app",
            "Build an app and start it in a window with its DevTools endpoint; it becomes the \
             selected app. Slow the first time.",
            &[
                (
                    "app",
                    "string",
                    "The app's entry, or its directory; a relative path is taken from the \
                     server's --app-dir (else its working directory). Returns once the app \
                     has rendered.",
                ),
                (
                    "target",
                    "string",
                    "`macos` or `windows` (default: this machine's).",
                ),
            ],
            &["app"],
        ),
        tool(
            "select_app",
            "Choose which app the other tools act on.",
            &[("pid", "integer", "A pid from list_apps.")],
            &["pid"],
        ),
        tool(
            "take_snapshot",
            "The selected app's views as compact text, one per line: `[id] kind \"label\" \
             id=testId (x,y w×h)` and flags. Use it to find selectors.",
            &[
                (
                    "root",
                    "string",
                    "Where the tree starts: a selector (default: the whole screen).",
                ),
                (
                    "depth",
                    "integer",
                    "How many levels below the root (default: all).",
                ),
            ],
            &[],
        ),
        tool(
            "take_screenshot",
            "A PNG of the selected app's window.",
            &[(
                "node",
                "string",
                "Only this view (a selector), cut out of the window.",
            )],
            &[],
        ),
        tool(
            "click",
            "Press the view the selector names, with the platform's own input.",
            &[("selector", "string", SEL)],
            &["selector"],
        ),
        tool(
            "type",
            "Type text into the input the selector names.",
            &[
                ("selector", "string", SEL),
                ("text", "string", "The text to type."),
            ],
            &["selector", "text"],
        ),
        tool(
            "press_key",
            "Press a keystroke, such as `mod+k` or `escape`.",
            &[("keys", "string", "The keystroke.")],
            &["keys"],
        ),
        tool(
            "scroll",
            "Scroll the view the selector names.",
            &[
                ("selector", "string", SEL),
                ("distance", "number", "Points; negative scrolls back."),
            ],
            &["selector", "distance"],
        ),
        tool(
            "hover",
            "Move the pointer over the view the selector names.",
            &[("selector", "string", SEL)],
            &["selector"],
        ),
        tool(
            "drag",
            "Drag the view by x and y points.",
            &[
                ("selector", "string", SEL),
                ("x", "number", "Points rightwards."),
                ("y", "number", "Points downwards."),
            ],
            &["selector", "x", "y"],
        ),
        tool(
            "wait_for",
            "Wait until a view the selector names shows, or with gone until none does.",
            &[
                ("selector", "string", SEL),
                (
                    "gone",
                    "boolean",
                    "Wait for it to go instead (default false).",
                ),
                (
                    "timeout_ms",
                    "integer",
                    "Milliseconds at most (default 5000).",
                ),
            ],
            &["selector"],
        ),
        tool(
            "list_console",
            "The app's log lines: `time level [target] message`, the newest last.",
            &[
                (
                    "level",
                    "string",
                    "This level or more severe: error, warn, info, debug, trace.",
                ),
                (
                    "target",
                    "string",
                    "Only this log target (`js`, a Rust module path).",
                ),
                ("text", "string", "Only lines containing this text."),
                (
                    "limit",
                    "integer",
                    "At most this many, the newest (default 200).",
                ),
            ],
            &[],
        ),
        tool(
            "list_errors",
            "The errors the app logged since the last call.",
            &[],
            &[],
        ),
        tool("close_app", "Quit the selected app.", &[], &[]),
    ]
}

#[cfg(test)]
mod devtools_mcp {
    use std::{io::Cursor, sync::Mutex};

    use nsh_devtools::protocol::ErrorBody;

    use super::*;

    /// Records the calls and answers the snapshot with `snapshot`.
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<(String, Value)>>,
        snapshot: Vec<Value>,
    }

    impl Session for Fake {
        fn call_within(
            &self,
            method: &str,
            params: Value,
            _timeout: Duration,
        ) -> Result<Value, ErrorBody> {
            self.calls.lock().unwrap().push((method.to_owned(), params));
            Ok(match method {
                "Tree.snapshot" => Value::Array(self.snapshot.clone()),
                _ => json!({}),
            })
        }
    }

    fn server(fake: Arc<Fake>) -> Server {
        let mut server = Server::new(PathBuf::from("."));
        server.current = Some(Current {
            pid: 0,
            session: fake,
        });
        server
    }

    /// The replies to `requests`, one JSON-RPC message per line.
    fn talk(server: &mut Server, requests: &[Value]) -> Vec<Value> {
        let input: String = requests.iter().map(|r| format!("{r}\n")).collect();
        let mut out = Vec::new();
        run(server, Cursor::new(input), &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn call(id: u32, name: &str, arguments: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": { "name": name, "arguments": arguments } })
    }

    #[test]
    fn tools_list_names_every_tool() {
        let mut server = server(Arc::default());
        let replies = talk(
            &mut server,
            &[
                json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                    "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "test", "version": "0" } } }),
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            ],
        );
        // The notification gets no reply.
        assert_eq!(replies.len(), 2, "{replies:?}");
        assert_eq!(replies[0]["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(replies[0]["result"]["serverInfo"]["name"], "nsh-devtools");
        let names: Vec<&str> = replies[1]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "list_apps",
                "launch_app",
                "select_app",
                "take_snapshot",
                "take_screenshot",
                "click",
                "type",
                "press_key",
                "scroll",
                "hover",
                "drag",
                "wait_for",
                "list_console",
                "list_errors",
                "close_app",
            ]
        );
        for tool in replies[1]["result"]["tools"].as_array().unwrap() {
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        }
    }

    #[test]
    fn long_answers_are_cut_with_a_note() {
        let nodes = (0..2000)
            .map(|i| {
                json!({ "id": i, "depth": 0, "kind": "View", "label": "x".repeat(40),
                        "visible": true, "enabled": true, "focused": false,
                        "recycled": false })
            })
            .collect();
        let mut server = server(Arc::new(Fake {
            snapshot: nodes,
            ..Fake::default()
        }));
        let replies = talk(&mut server, &[call(1, "take_snapshot", json!({}))]);
        let text = replies[0]["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.ends_with(CUT_NOTE), "{}", &text[text.len() - 80..]);
        assert!(text.chars().count() <= TEXT_LIMIT + CUT_NOTE.chars().count() + 1);
        assert!(text.chars().count() > TEXT_LIMIT);
    }

    #[test]
    fn click_maps_to_input_press() {
        let fake = Arc::new(Fake::default());
        let mut server = server(fake.clone());
        let replies = talk(
            &mut server,
            &[call(1, "click", json!({ "selector": "label:Add" }))],
        );
        assert!(replies[0]["result"]["isError"].is_null(), "{replies:?}");
        assert_eq!(
            fake.calls.lock().unwrap()[0],
            ("Input.press".to_owned(), json!({ "selector": "label:Add" }))
        );
        // A missing argument is the agent's to fix: an error result, no call.
        let replies = talk(&mut server, &[call(2, "click", json!({}))]);
        assert_eq!(replies[0]["result"]["isError"], true);
        assert_eq!(fake.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn actions_release_the_input_lock() {
        let fake = Arc::new(Fake::default());
        let mut server = server(fake.clone());
        talk(
            &mut server,
            &[
                call(1, "click", json!({ "selector": "label:Add" })),
                call(2, "type", json!({ "selector": "id:n", "text": "x" })),
            ],
        );
        let methods: Vec<String> = fake
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|(m, _)| m.clone())
            .collect();
        assert_eq!(
            methods,
            [
                "Input.press",
                "Session.release",
                "Input.type",
                "Session.release"
            ]
        );
    }

    #[test]
    fn launch_waits_for_ready() {
        use crate::devtools::cli::wait_ready;
        let fake = Fake::default();
        wait_ready(&fake, String::new).unwrap();
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls[0].0, "Session.ready");
        // The params must be what the endpoint decodes into the `Ready` step.
        assert!(nsh_devtools::methods::step_of("Session.ready", calls[0].1.clone()).is_ok());
    }

    #[test]
    fn a_relative_app_is_taken_from_the_app_dir() {
        let server = Server::new(PathBuf::from("examples/counter"));
        assert_eq!(
            server.app_path("App.tsx"),
            PathBuf::from("examples/counter/App.tsx")
        );
        let absolute = std::env::temp_dir().join("App.tsx");
        assert_eq!(server.app_path(absolute.to_str().unwrap()), absolute);
    }

    #[test]
    fn a_request_without_a_method_is_invalid() {
        let mut server = server(Arc::default());
        let replies = talk(&mut server, &[json!({ "jsonrpc": "2.0", "id": 7 })]);
        assert_eq!(replies[0]["error"]["code"], -32600);
    }
}
