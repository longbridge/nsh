//! WebDriver's actions, screenshots, page source, window and the few
//! `execute` scripts the server knows.
//!
//! Actions are ticks over input sources. A pointer source's actions become
//! `Input.pointer` sequences and a key source's `Input.key` sequences;
//! consecutive actions of one kind go in one call, in tick order. Scripts do
//! not run: `execute/sync` takes the `nsh: …` names of [`execute`] only.

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    response::Response,
    routing::{get, post},
};
use nsh_dev::protocol::{KeyAction, PointerAction};
use nsh_devtools::xml;
use serde_json::{Value, json};

use super::{
    element::{ELEMENT_KEY, check, parse_id, snapshot},
    error::WdError,
    keys::key_name,
    server::{Current, Server, blocking, body_json, value},
};

/// Added to the pauses of a sequence for the app to take its actions.
const SLACK: Duration = Duration::from_secs(30);

/// The routes of this module.
pub(crate) fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/session/{sid}/actions", post(perform).delete(release))
        .route("/session/{sid}/screenshot", get(screenshot))
        .route("/session/{sid}/source", get(source))
        .route("/session/{sid}/window", get(window))
        .route("/session/{sid}/window/handles", get(handles))
        .route("/session/{sid}/window/rect", get(rect).post(set_rect))
        .route("/session/{sid}/execute/sync", post(execute))
        .route("/session/{sid}/execute/async", post(execute_async))
}

/// A run of actions of one input kind.
#[derive(Debug, PartialEq)]
enum Batch {
    Pointer(Vec<PointerAction>),
    Keys(Vec<KeyAction>),
}

impl Batch {
    fn pause(&self) -> u64 {
        let ms = |a: &PointerAction| match a {
            PointerAction::Pause { ms } => *ms,
            _ => 0,
        };
        let key_ms = |a: &KeyAction| match a {
            KeyAction::Pause { ms } => *ms,
            _ => 0,
        };
        match self {
            Batch::Pointer(actions) => actions.iter().map(ms).sum(),
            Batch::Keys(actions) => actions.iter().map(key_ms).sum(),
        }
    }
}

/// An action as it is taken, before runs are made.
enum Item {
    Pointer(PointerAction),
    Key(KeyAction),
}

fn push(batches: &mut Vec<Batch>, item: Item) {
    match (item, batches.last_mut()) {
        (Item::Pointer(a), Some(Batch::Pointer(run))) => run.push(a),
        (Item::Pointer(a), _) => batches.push(Batch::Pointer(vec![a])),
        (Item::Key(a), Some(Batch::Keys(run))) => run.push(a),
        (Item::Key(a), _) => batches.push(Batch::Keys(vec![a])),
    }
}

fn number(action: &Value, key: &str) -> Result<f32, WdError> {
    match action.get(key) {
        None | Some(Value::Null) => Ok(0.0),
        Some(v) => v
            .as_f64()
            .map(|n| n as f32)
            .ok_or_else(|| WdError::invalid_argument(format!("`{key}` is not a number"))),
    }
}

fn duration(action: &Value) -> Result<u64, WdError> {
    match action.get("duration") {
        None | Some(Value::Null) => Ok(0),
        Some(v) => v
            .as_u64()
            .ok_or_else(|| WdError::invalid_argument("`duration` is not a non-negative integer")),
    }
}

/// The point a `pointerMove` goes to.
fn target(cur: &Current, action: &Value, last: (f32, f32)) -> Result<(f32, f32), WdError> {
    let (x, y) = (number(action, "x")?, number(action, "y")?);
    match action.get("origin") {
        None | Some(Value::Null) => Ok((x, y)),
        Some(Value::String(origin)) if origin == "viewport" => Ok((x, y)),
        Some(Value::String(origin)) if origin == "pointer" => Ok((last.0 + x, last.1 + y)),
        Some(Value::Object(origin)) => {
            let eid = origin
                .get(ELEMENT_KEY)
                .and_then(Value::as_str)
                .ok_or_else(|| WdError::invalid_argument("`origin` is not an element reference"))?;
            let node = check(cur, parse_id(eid)?)?;
            let [bx, by, w, h] = node.bounds.ok_or_else(|| {
                WdError::move_target_out_of_bounds(format!("view {} has no bounds", node.id))
            })?;
            Ok((bx + w / 2.0 + x, by + h / 2.0 + y))
        }
        Some(_) => Err(WdError::invalid_argument(
            "`origin` is `viewport`, `pointer` or an element",
        )),
    }
}

/// The key a `keyDown` / `keyUp` value names: a Private Use key or one
/// character. `None` for the null key.
fn key_of(action: &Value) -> Result<Option<String>, WdError> {
    let text = action
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| WdError::invalid_argument("`value` is not a string"))?;
    let mut chars = text.chars();
    let first = chars
        .next()
        .ok_or_else(|| WdError::invalid_argument("`value` is empty"))?;
    Ok(match key_name(first) {
        Some("null") => None,
        Some(name) => Some(name.to_owned()),
        None => Some(text.to_owned()),
    })
}

/// The actions of a request, in the order they are taken: tick by tick,
/// sources in request order within a tick.
fn plan(cur: &Current, body: &Value) -> Result<Vec<Batch>, WdError> {
    let sources = body
        .get("actions")
        .and_then(Value::as_array)
        .ok_or_else(|| WdError::invalid_argument("`actions` is not an array"))?;
    struct Source<'a> {
        kind: &'a str,
        actions: &'a [Value],
        last: (f32, f32),
    }
    let mut parsed = Vec::new();
    for source in sources {
        let kind = source
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| WdError::invalid_argument("an input source has no `type`"))?;
        let actions = source
            .get("actions")
            .and_then(Value::as_array)
            .ok_or_else(|| WdError::invalid_argument("an input source has no `actions`"))?;
        match kind {
            "pointer" => {
                // Mouse, pen and touch all press the left button.
                let pointer_type = source
                    .get("parameters")
                    .and_then(|p| p.get("pointerType"))
                    .and_then(Value::as_str)
                    .unwrap_or("mouse");
                if !matches!(pointer_type, "mouse" | "pen" | "touch") {
                    return Err(WdError::invalid_argument(format!(
                        "pointerType `{pointer_type}` is not `mouse`, `pen` or `touch`"
                    )));
                }
            }
            "key" | "none" => {}
            "wheel" => {
                return Err(WdError::unsupported_operation(
                    "wheel input sources are not supported",
                ));
            }
            other => {
                return Err(WdError::invalid_argument(format!(
                    "the input source type `{other}` is not known"
                )));
            }
        }
        parsed.push(Source {
            kind,
            actions,
            last: (0.0, 0.0),
        });
    }
    let ticks = parsed.iter().map(|s| s.actions.len()).max().unwrap_or(0);
    let mut batches = Vec::new();
    for tick in 0..ticks {
        for source in &mut parsed {
            let Some(action) = source.actions.get(tick) else {
                continue;
            };
            let kind = action
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| WdError::invalid_argument("an action has no `type`"))?;
            match (source.kind, kind) {
                (_, "pause") => {
                    let ms = duration(action)?;
                    match source.kind {
                        "key" => push(&mut batches, Item::Key(KeyAction::Pause { ms })),
                        _ => push(&mut batches, Item::Pointer(PointerAction::Pause { ms })),
                    }
                }
                ("pointer", "pointerDown") => {
                    let (x, y) = source.last;
                    push(&mut batches, Item::Pointer(PointerAction::Down { x, y }));
                }
                ("pointer", "pointerUp") => {
                    let (x, y) = source.last;
                    push(&mut batches, Item::Pointer(PointerAction::Up { x, y }));
                }
                ("pointer", "pointerMove") => {
                    let (x, y) = target(cur, action, source.last)?;
                    // No interpolation: the move waits its duration, then
                    // arrives.
                    let ms = duration(action)?;
                    if ms > 0 {
                        push(&mut batches, Item::Pointer(PointerAction::Pause { ms }));
                    }
                    push(&mut batches, Item::Pointer(PointerAction::Move { x, y }));
                    source.last = (x, y);
                }
                ("pointer", "pointerCancel") => {}
                ("key", "keyDown") => {
                    if let Some(key) = key_of(action)? {
                        push(&mut batches, Item::Key(KeyAction::Down { key }));
                    }
                }
                ("key", "keyUp") => {
                    if let Some(key) = key_of(action)? {
                        push(&mut batches, Item::Key(KeyAction::Up { key }));
                    }
                }
                (source, other) => {
                    return Err(WdError::invalid_argument(format!(
                        "the action `{other}` does not belong to a `{source}` input source"
                    )));
                }
            }
        }
    }
    Ok(batches)
}

async fn perform(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
    body: Bytes,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let body = body_json(&body)?;
    blocking(move || {
        // A sequence that does not decode sent nothing: its own error.
        let batches = plan(&cur, &body)?;
        for batch in batches {
            let wait = Duration::from_millis(batch.pause()) + SLACK;
            let (method, params) = match batch {
                Batch::Pointer(actions) => ("Input.pointer", json!({ "actions": actions })),
                Batch::Keys(actions) => ("Input.key", json!({ "actions": actions })),
            };
            if let Err(err) = cur.session.call_within(method, params, wait) {
                // A button or modifier the failed sequence left down would
                // stay down until the client thought of DELETE /actions;
                // best effort, the sequence's own error is the answer.
                let _ = cur.session.call("Session.release", Value::Null);
                return Err(WdError::from(err));
            }
        }
        Ok::<_, WdError>(())
    })
    .await??;
    Ok(value(Value::Null))
}

async fn release(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    blocking(move || cur.session.call("Session.release", Value::Null))
        .await?
        .map_err(WdError::from)?;
    Ok(value(Value::Null))
}

async fn screenshot(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let png = blocking(move || cur.session.call("Capture.screenshot", Value::Null))
        .await?
        .map_err(WdError::from)?;
    Ok(value(png))
}

async fn source(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let xml = blocking(move || {
        let nodes = snapshot(cur.session.as_ref())?;
        Ok::<_, WdError>(xml::render_xml(&nodes))
    })
    .await??;
    Ok(value(Value::String(xml)))
}

async fn window(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    server.current(&sid)?;
    Ok(value(json!("main")))
}

async fn handles(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    server.current(&sid)?;
    Ok(value(json!(["main"])))
}

async fn rect(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let [_, _, width, height] = blocking(move || {
        let nodes = snapshot(cur.session.as_ref())?;
        Ok::<_, WdError>(nodes.first().and_then(|n| n.bounds).unwrap_or_default())
    })
    .await??;
    Ok(value(
        json!({ "x": 0, "y": 0, "width": width, "height": height }),
    ))
}

async fn set_rect(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    server.current(&sid)?;
    Err(WdError::unsupported_operation(
        "the window's size is the app's: set the viewport in the session's capabilities",
    ))
}

/// What `execute/sync` runs: an exact `script`, its first argument an object.
fn run(cur: &Current, body: &Value) -> Result<Value, WdError> {
    let script = body
        .get("script")
        .and_then(Value::as_str)
        .ok_or_else(|| WdError::invalid_argument("`script` is not a string"))?;
    let args = match body
        .get("args")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        None | Some(Value::Null) => json!({}),
        Some(args @ Value::Object(_)) => args.clone(),
        Some(_) => {
            return Err(WdError::invalid_argument(
                "the first argument is not an object",
            ));
        }
    };
    match script {
        "nsh: console" => {
            let mut params = serde_json::Map::new();
            for key in ["level", "text", "limit"] {
                if let Some(v) = args.get(key).filter(|v| !v.is_null()) {
                    params.insert(key.to_owned(), v.clone());
                }
            }
            let answer = cur.session.call("Console.query", Value::Object(params))?;
            Ok(answer.get("entries").cloned().unwrap_or(answer))
        }
        "nsh: errors" => Ok(cur.session.call("Console.takeErrors", json!({}))?),
        "nsh: ready" => {
            let ms = match args.get("timeoutMs") {
                None | Some(Value::Null) => 10_000,
                Some(v) => v.as_u64().ok_or_else(|| {
                    WdError::invalid_argument("`timeoutMs` is not a non-negative integer")
                })?,
            };
            cur.session.call_within(
                "Session.ready",
                json!({ "timeout_ms": ms }),
                Duration::from_millis(ms) + SLACK,
            )?;
            Ok(Value::Null)
        }
        "nsh: colorScheme" => {
            let dark = match args.get("scheme").and_then(Value::as_str) {
                Some("dark") => true,
                Some("light") => false,
                _ => {
                    return Err(WdError::invalid_argument("`scheme` is `dark` or `light`"));
                }
            };
            cur.session
                .call("Environment.setColorScheme", json!({ "dark": dark }))?;
            Ok(Value::Null)
        }
        _ => Err(WdError::unsupported_operation(
            "scripts do not run: execute one of `nsh: console`, `nsh: errors`, `nsh: ready` or \
             `nsh: colorScheme`",
        )),
    }
}

async fn execute(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
    body: Bytes,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let body = body_json(&body)?;
    let answer = blocking(move || run(&cur, &body)).await??;
    Ok(value(answer))
}

async fn execute_async(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    server.current(&sid)?;
    Err(WdError::unsupported_operation(
        "asynchronous scripts do not run",
    ))
}

#[cfg(test)]
mod devtools_webdriver {
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use nsh_devtools::protocol::ErrorBody;
    use tower::ServiceExt as _;

    use super::*;
    use crate::devtools::{
        Session,
        webdriver::{server::Launch, session::Caps},
    };

    type Handler = Arc<dyn Fn(&str, &Value) -> Result<Value, ErrorBody> + Send + Sync>;
    type Calls = Arc<Mutex<Vec<(String, Value)>>>;

    struct Fake(Handler, Calls);

    impl Session for Fake {
        fn call_within(
            &self,
            method: &str,
            params: Value,
            _timeout: Duration,
        ) -> Result<Value, ErrorBody> {
            self.1
                .lock()
                .unwrap()
                .push((method.to_owned(), params.clone()));
            (self.0)(method, &params)
        }
    }

    struct Launcher(Handler, Calls);

    impl Launch for Launcher {
        fn launch(&self, _caps: &Caps) -> Result<Box<dyn Session>, WdError> {
            Ok(Box::new(Fake(self.0.clone(), self.1.clone())))
        }
    }

    struct Rig {
        router: Router,
        calls: Calls,
        sid: String,
    }

    impl Rig {
        async fn send(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
            let uri = format!("/session/{}{path}", self.sid);
            send(&self.router, method, &uri, body).await
        }

        fn methods(&self) -> Vec<String> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .map(|(m, _)| m.clone())
                .collect()
        }
    }

    async fn send(
        router: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    async fn rig(
        handler: impl Fn(&str, &Value) -> Result<Value, ErrorBody> + Send + Sync + 'static,
    ) -> Rig {
        let calls = Calls::default();
        let launcher = Launcher(Arc::new(handler), calls.clone());
        let server = Server::new(Box::new(launcher), PathBuf::from("/apps"));
        let router = crate::devtools::webdriver::server::router(Arc::new(server));
        let body = json!({ "capabilities": { "alwaysMatch": {
            "platformName": "mac", "appium:app": "a/App.tsx",
        } } });
        let (status, answer) = send(&router, "POST", "/session", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        Rig {
            router,
            calls,
            sid: answer["value"]["sessionId"].as_str().unwrap().to_owned(),
        }
    }

    fn node(id: u32, depth: u32, kind: &str) -> Value {
        json!({
            "id": id, "depth": depth, "kind": kind,
            "visible": true, "enabled": true, "focused": false,
        })
    }

    #[tokio::test]
    async fn pointer_actions_become_one_pointer_sequence() {
        let rig = rig(|_, _| Ok(Value::Null)).await;
        let body = json!({ "actions": [{
            "type": "pointer", "id": "mouse", "parameters": { "pointerType": "touch" },
            "actions": [
                { "type": "pointerMove", "x": 10, "y": 20 },
                { "type": "pointerDown", "button": 0 },
                { "type": "pointerMove", "duration": 100, "x": 20, "y": 20, "origin": "pointer" },
                { "type": "pause", "duration": 50 },
                { "type": "pointerUp", "button": 0 },
            ],
        }] });
        let (status, answer) = rig.send("POST", "/actions", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let calls = rig.calls.lock().unwrap();
        // One call: the whole sequence, a timed move as a pause, then a move.
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "Input.pointer");
        assert_eq!(
            calls[0].1,
            json!({ "actions": [
                { "type": "move", "x": 10.0, "y": 20.0 },
                { "type": "down", "x": 10.0, "y": 20.0 },
                { "type": "pause", "ms": 100 },
                { "type": "move", "x": 30.0, "y": 40.0 },
                { "type": "pause", "ms": 50 },
                { "type": "up", "x": 30.0, "y": 40.0 },
            ] })
        );
    }

    #[tokio::test]
    async fn element_origin_is_its_centre() {
        let rig = rig(|method, _| match method {
            "Tree.describe" => {
                let mut n = node(7, 0, "view");
                n["bounds"] = json!([10.0, 20.0, 40.0, 60.0]);
                Ok(n)
            }
            _ => Ok(Value::Null),
        })
        .await;
        let origin = json!({ ELEMENT_KEY: "7" });
        let body = json!({ "actions": [{
            "type": "pointer", "id": "m",
            "actions": [
                { "type": "pointerMove", "origin": origin, "x": 0, "y": 0 },
                { "type": "pointerDown", "button": 0 },
                { "type": "pointerMove", "origin": origin, "x": 5, "y": -5 },
                { "type": "pointerUp", "button": 0 },
            ],
        }] });
        let (status, answer) = rig.send("POST", "/actions", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let calls = rig.calls.lock().unwrap();
        let sent = calls.iter().find(|(m, _)| m == "Input.pointer").unwrap();
        assert_eq!(
            sent.1,
            json!({ "actions": [
                { "type": "move", "x": 30.0, "y": 50.0 },
                { "type": "down", "x": 30.0, "y": 50.0 },
                { "type": "move", "x": 35.0, "y": 45.0 },
                { "type": "up", "x": 35.0, "y": 45.0 },
            ] })
        );
    }

    #[tokio::test]
    async fn key_actions_map_special_keys() {
        let rig = rig(|_, _| Ok(Value::Null)).await;
        let body = json!({ "actions": [{
            "type": "key", "id": "kb",
            "actions": [
                { "type": "keyDown", "value": "\u{E008}" },
                { "type": "keyDown", "value": "a" },
                { "type": "keyUp", "value": "a" },
                { "type": "keyUp", "value": "\u{E008}" },
            ],
        }] });
        let (status, _) = rig.send("POST", "/actions", Some(body)).await;
        assert_eq!(status, StatusCode::OK);
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls[0].0, "Input.key");
        assert_eq!(
            calls[0].1,
            json!({ "actions": [
                { "type": "down", "key": "shift" },
                { "type": "down", "key": "a" },
                { "type": "up", "key": "a" },
                { "type": "up", "key": "shift" },
            ] })
        );
    }

    #[tokio::test]
    async fn a_failed_sequence_lets_go_of_the_input() {
        let rig = rig(|method, _| match method {
            "Input.key" => Err(ErrorBody::new("ERR_STEP_FAILED", "boom")),
            _ => Ok(Value::Null),
        })
        .await;
        let body = json!({ "actions": [{
            "type": "key", "id": "kb",
            "actions": [{ "type": "keyDown", "value": "\u{E008}" }],
        }] });
        let (status, _) = rig.send("POST", "/actions", Some(body)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(rig.methods(), ["Input.key", "Session.release"]);
    }

    #[tokio::test]
    async fn an_unknown_source_is_invalid_argument() {
        let rig = rig(|method, _| panic!("{method}")).await;
        let body = json!({ "actions": [{ "type": "gamepad", "id": "g", "actions": [] }] });
        let (status, answer) = rig.send("POST", "/actions", Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(answer["value"]["error"], "invalid argument");
    }

    #[tokio::test]
    async fn delete_actions_releases_input() {
        let rig = rig(|_, _| Ok(json!({}))).await;
        let (status, answer) = rig.send("DELETE", "/actions", None).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(rig.methods(), ["Session.release"]);
    }

    #[tokio::test]
    async fn delete_session_releases_input() {
        let rig = rig(|_, _| Ok(json!({}))).await;
        let uri = format!("/session/{}", rig.sid);
        let (status, _) = send(&rig.router, "DELETE", &uri, None).await;
        assert_eq!(status, StatusCode::OK);
        // A held button is let go before the app quits.
        assert_eq!(rig.methods(), ["Session.release", "Session.quit"]);
    }

    #[tokio::test]
    async fn source_is_the_xml() {
        let rig = rig(|_, _| {
            let mut n = node(3, 0, "text");
            n["text"] = json!("it's \"x\" & <y> 你好 😀");
            Ok(json!([n]))
        })
        .await;
        let (status, answer) = rig.send("GET", "/source", None).await;
        assert_eq!(status, StatusCode::OK);
        let xml = answer["value"].as_str().unwrap();
        assert!(xml.starts_with("<?xml"), "{xml}");
        assert!(xml.contains("<text id=\"3\""), "{xml}");
        assert!(xml.contains("&amp;") && xml.contains("&lt;y&gt;"), "{xml}");
        assert!(xml.contains("你好 😀"), "{xml}");
    }

    #[tokio::test]
    async fn screenshot_is_the_base64_png() {
        let rig = rig(|_, _| Ok(json!("iVBORw0KGgo="))).await;
        let (status, answer) = rig.send("GET", "/screenshot", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(answer["value"], "iVBORw0KGgo=");
        assert_eq!(rig.methods(), ["Capture.screenshot"]);
    }

    #[tokio::test]
    async fn window_is_main_with_the_roots_size() {
        let rig = rig(|_, _| {
            let mut root = node(1, 0, "view");
            root["bounds"] = json!([0.0, 0.0, 390.0, 844.0]);
            Ok(json!([root]))
        })
        .await;
        let (_, answer) = rig.send("GET", "/window", None).await;
        assert_eq!(answer["value"], "main");
        let (_, answer) = rig.send("GET", "/window/handles", None).await;
        assert_eq!(answer["value"], json!(["main"]));
        let (_, answer) = rig.send("GET", "/window/rect", None).await;
        assert_eq!(
            answer["value"],
            json!({ "x": 0, "y": 0, "width": 390.0, "height": 844.0 })
        );
        let (status, answer) = rig
            .send("POST", "/window/rect", Some(json!({ "width": 100 })))
            .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(answer["value"]["error"], "unsupported operation");
    }

    #[tokio::test]
    async fn execute_whitelist_only() {
        let rig = rig(|method, _| match method {
            "Console.takeErrors" => Ok(json!(["boom"])),
            "Console.query" => Ok(json!({ "entries": [] })),
            _ => Ok(Value::Null),
        })
        .await;
        let (status, answer) = rig
            .send(
                "POST",
                "/execute/sync",
                Some(json!({ "script": "return 1", "args": [] })),
            )
            .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(answer["value"]["error"], "unsupported operation");
        assert!(rig.calls.lock().unwrap().is_empty());

        let (status, answer) = rig
            .send(
                "POST",
                "/execute/sync",
                Some(json!({ "script": "nsh: errors", "args": [{}] })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["value"], json!(["boom"]));
        assert_eq!(rig.methods(), ["Console.takeErrors"]);

        let (status, answer) = rig
            .send(
                "POST",
                "/execute/sync",
                Some(
                    json!({ "script": "nsh: console", "args": [{ "level": "warn", "limit": 5 }] }),
                ),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["value"], json!([]));
        assert_eq!(
            rig.calls.lock().unwrap()[1].1,
            json!({ "level": "warn", "limit": 5 })
        );

        let (_, _) = rig
            .send(
                "POST",
                "/execute/sync",
                Some(json!({ "script": "nsh: colorScheme", "args": [{ "scheme": "dark" }] })),
            )
            .await;
        assert_eq!(
            rig.calls.lock().unwrap()[2],
            (
                "Environment.setColorScheme".to_owned(),
                json!({ "dark": true })
            )
        );
        let (status, answer) = rig
            .send(
                "POST",
                "/execute/async",
                Some(json!({ "script": "nsh: errors" })),
            )
            .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(answer["value"]["error"], "unsupported operation");
    }
}
