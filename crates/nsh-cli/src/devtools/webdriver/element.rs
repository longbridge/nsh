//! Finding views and acting on them: WebDriver's element commands on the
//! DevTools protocol.
//!
//! An element reference is a view id; every step on it names the view as
//! `view:<id>`. A view that left the tree is stale (`Tree.describe` says so).
//! A list cell is recycled with its id kept, so the `testID` and text seen at
//! find time are remembered, and a recycled cell showing other ones is stale
//! as well: a client never acts on whatever now sits in the cell.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    response::Response,
    routing::{get, post},
};
use nsh_devtools::{tree::SnapshotNode, xml};
use serde_json::{Value, json};

use super::{
    error::WdError,
    keys::Sending,
    server::{Current, Server, blocking, body_json, value},
};
use crate::devtools::Session;

/// The key of an element reference (W3C WebDriver).
pub(super) const ELEMENT_KEY: &str = "element-6066-11e4-a52e-4f735466cecf";

/// How often a find that waits for its element looks again.
const POLL: Duration = Duration::from_millis(50);

/// What an element showed when the client found it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Recorded {
    test_id: Option<String>,
    text: Option<String>,
}

/// The elements a session's client found, by view id.
pub(crate) type Elements = Arc<Mutex<HashMap<u32, Recorded>>>;

/// The element reference of view `id`.
fn reference(id: u32) -> Value {
    json!({ ELEMENT_KEY: id.to_string() })
}

/// The view an element id names.
pub(super) fn parse_id(eid: &str) -> Result<u32, WdError> {
    eid.parse()
        .map_err(|_| WdError::no_such_element(format!("`{eid}` is not an element of this session")))
}

/// The selector of a view.
pub(super) fn view_selector(id: u32) -> String {
    format!("view:{id}")
}

pub(super) fn decode(answer: Value) -> Result<SnapshotNode, WdError> {
    serde_json::from_value(answer)
        .map_err(|e| WdError::unknown_error(format!("the app's answer is not a view: {e}")))
}

/// The view as the app describes it now, if it is the element the client
/// found: a view gone from the tree is stale, and so is a recycled list cell
/// whose `testID` or text differs from what was found.
pub(super) fn check(cur: &Current, id: u32) -> Result<SnapshotNode, WdError> {
    let node = decode(cur.session.call("Tree.describe", json!({ "id": id }))?)?;
    if node.recycled {
        let elements = cur.elements.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = elements.get(&id)
            && (found.test_id != node.test_id || found.text != node.text)
        {
            return Err(WdError::stale_element(format!(
                "view {id} is a list cell that shows other content now"
            )));
        }
    }
    Ok(node)
}

/// Remembers what `ids` among `nodes` show.
fn record(cur: &Current, nodes: &[SnapshotNode], ids: &[u32]) {
    let mut elements = cur.elements.lock().unwrap_or_else(PoisonError::into_inner);
    for node in nodes.iter().filter(|n| ids.contains(&n.id)) {
        elements.insert(
            node.id,
            Recorded {
                test_id: node.test_id.clone(),
                text: node.text.clone(),
            },
        );
    }
}

/// Every view, hidden ones too, in tree order.
pub(super) fn snapshot(session: &dyn Session) -> Result<Vec<SnapshotNode>, WdError> {
    let answer = session.call(
        "Tree.snapshot",
        json!({ "root": null, "depth": null, "hidden": true }),
    )?;
    serde_json::from_value(answer)
        .map_err(|e| WdError::unknown_error(format!("the app's snapshot is not a node list: {e}")))
}

/// The views under `id` (not itself) in `nodes`, or `None` when `id` is not
/// in the tree.
fn descendants(nodes: &[SnapshotNode], id: u32) -> Option<HashSet<u32>> {
    let at = nodes.iter().position(|n| n.id == id)?;
    let depth = nodes[at].depth;
    Some(
        nodes[at + 1..]
            .iter()
            .take_while(|n| n.depth > depth)
            .map(|n| n.id)
            .collect(),
    )
}

/// How a request locates elements.
enum Strategy {
    /// An `nsh/test` selector.
    Selector(String),
    /// Snapshot nodes whose kind is this.
    Class(String),
    /// An XPath expression on the snapshot's XML.
    XPath(String),
}

fn strategy(body: &Value) -> Result<Strategy, WdError> {
    let using = body
        .get("using")
        .and_then(Value::as_str)
        .ok_or_else(|| WdError::invalid_argument("`using` is not a string"))?;
    let locator = body
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| WdError::invalid_argument("`value` is not a string"))?
        .to_owned();
    Ok(match using {
        "id" => Strategy::Selector(format!("id:{locator}")),
        "accessibility id" => Strategy::Selector(format!("label:{locator}")),
        "-nsh selector" => Strategy::Selector(locator),
        "class name" => Strategy::Class(locator),
        "xpath" => Strategy::XPath(locator),
        other => {
            return Err(WdError::invalid_argument(format!(
                "the locator strategy `{other}` is not supported: use `id`, `accessibility id`, \
                 `class name`, `xpath` or `-nsh selector`"
            )));
        }
    })
}

/// One look for the views `strategy` finds under `scope`, in screen order
/// (tree order for the snapshot's strategies); what they show is remembered.
fn find_once(cur: &Current, scope: Option<u32>, strategy: &Strategy) -> Result<Vec<u32>, WdError> {
    let (ids, nodes) = match strategy {
        Strategy::Selector(selector) => {
            let answer = cur
                .session
                .call("Tree.query", json!({ "selector": selector, "root": scope }))?;
            let ids: Vec<u32> = answer["ids"]
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_u64)
                        .filter_map(|id| u32::try_from(id).ok())
                        .collect()
                })
                .unwrap_or_default();
            let nodes = if ids.is_empty() {
                Vec::new()
            } else {
                snapshot(cur.session.as_ref())?
            };
            (ids, nodes)
        }
        Strategy::Class(kind) => {
            let nodes = snapshot(cur.session.as_ref())?;
            let inside = scoped(&nodes, scope)?;
            let ids = nodes
                .iter()
                .filter(|n| n.kind == *kind && inside.as_ref().is_none_or(|s| s.contains(&n.id)))
                .map(|n| n.id)
                .collect();
            (ids, nodes)
        }
        Strategy::XPath(expr) => {
            let nodes = snapshot(cur.session.as_ref())?;
            let inside = scoped(&nodes, scope)?;
            let ids = xml::xpath_ids(&xml::render_xml(&nodes), expr)
                .map_err(|e| WdError::invalid_selector(e.to_string()))?
                .into_iter()
                .filter(|id| inside.as_ref().is_none_or(|s| s.contains(id)))
                .collect();
            (ids, nodes)
        }
    };
    record(cur, &nodes, &ids);
    Ok(ids)
}

/// The views under `scope`, `None` for the whole tree; a scope that left the
/// tree is stale.
fn scoped(nodes: &[SnapshotNode], scope: Option<u32>) -> Result<Option<HashSet<u32>>, WdError> {
    match scope {
        None => Ok(None),
        Some(id) => descendants(nodes, id)
            .map(Some)
            .ok_or_else(|| WdError::stale_element(format!("view {id} is no longer in the tree"))),
    }
}

/// Looks until something is found or the implicit timeout is over. `one`:
/// no element is an error, else an empty list answers.
fn find(cur: &Current, scope: Option<u32>, body: &Value, one: bool) -> Result<Vec<u32>, WdError> {
    let strategy = strategy(body)?;
    if let Some(id) = scope {
        check(cur, id)?;
    }
    let deadline = Instant::now() + Duration::from_millis(cur.timeouts.implicit);
    loop {
        let ids = find_once(cur, scope, &strategy)?;
        if !ids.is_empty() {
            return Ok(if one { ids[..1].to_vec() } else { ids });
        }
        if Instant::now() >= deadline {
            return if one {
                Err(WdError::no_such_element(
                    "no view matches the locator (the implicit timeout is over)",
                ))
            } else {
                Ok(ids)
            };
        }
        std::thread::sleep(POLL);
    }
}

/// An XML attribute of the node (as `render_xml` writes it), as a string.
fn attribute(node: &SnapshotNode, name: &str) -> Option<String> {
    let bound = |i: usize| node.bounds.map(|b| (b[i].round() as i64).to_string());
    match name {
        "id" => Some(node.id.to_string()),
        "name" | "testID" => node.test_id.clone(),
        "label" => node.label.clone(),
        "text" => node.text.clone(),
        "value" => node.value.clone(),
        "placeholder" => node.placeholder.clone(),
        "role" => node.role.clone(),
        "x" => bound(0),
        "y" => bound(1),
        "width" => bound(2),
        "height" => bound(3),
        "visible" => Some(node.visible.to_string()),
        "enabled" => Some(node.enabled.to_string()),
        "focused" => Some(node.focused.to_string()),
        "selected" => node.selected.map(|s| s.to_string()),
        "checked" => node.checked.clone(),
        _ => None,
    }
}

/// What the element's text is: its text, else its value, else its label.
fn text_of(node: &SnapshotNode) -> String {
    node.text
        .as_ref()
        .or(node.value.as_ref())
        .or(node.label.as_ref())
        .cloned()
        .unwrap_or_default()
}

/// The routes of the element commands.
pub(crate) fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/session/{sid}/element", post(find_element))
        .route("/session/{sid}/elements", post(find_elements))
        .route("/session/{sid}/element/active", get(active))
        .route(
            "/session/{sid}/element/{eid}/element",
            post(find_in_element),
        )
        .route(
            "/session/{sid}/element/{eid}/elements",
            post(find_all_in_element),
        )
        .route("/session/{sid}/element/{eid}/click", post(click))
        .route("/session/{sid}/element/{eid}/clear", post(clear))
        .route("/session/{sid}/element/{eid}/value", post(send_value))
        .route("/session/{sid}/element/{eid}/text", get(text))
        .route("/session/{sid}/element/{eid}/name", get(name))
        .route("/session/{sid}/element/{eid}/attribute/{name}", get(attr))
        .route("/session/{sid}/element/{eid}/property/{name}", get(attr))
        .route("/session/{sid}/element/{eid}/rect", get(rect))
        .route("/session/{sid}/element/{eid}/displayed", get(displayed))
        .route("/session/{sid}/element/{eid}/enabled", get(enabled))
        .route("/session/{sid}/element/{eid}/selected", get(selected))
        .route("/session/{sid}/element/{eid}/screenshot", get(screenshot))
}

async fn find_element(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
    body: Bytes,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let body = body_json(&body)?;
    let ids = blocking(move || find(&cur, None, &body, true)).await??;
    Ok(value(reference(ids[0])))
}

async fn find_elements(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
    body: Bytes,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let body = body_json(&body)?;
    let ids = blocking(move || find(&cur, None, &body, false)).await??;
    Ok(value(ids.into_iter().map(reference).collect()))
}

async fn find_in_element(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let scope = parse_id(&eid)?;
    let body = body_json(&body)?;
    let ids = blocking(move || find(&cur, Some(scope), &body, true)).await??;
    Ok(value(reference(ids[0])))
}

async fn find_all_in_element(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let scope = parse_id(&eid)?;
    let body = body_json(&body)?;
    let ids = blocking(move || find(&cur, Some(scope), &body, false)).await??;
    Ok(value(ids.into_iter().map(reference).collect()))
}

async fn active(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let id = blocking(move || {
        let nodes = snapshot(cur.session.as_ref())?;
        let id = nodes
            .iter()
            .find(|n| n.focused)
            .map(|n| n.id)
            .ok_or_else(|| WdError::no_such_element("no view has the focus"))?;
        record(&cur, &nodes, &[id]);
        Ok::<_, WdError>(id)
    })
    .await??;
    Ok(value(reference(id)))
}

/// Runs `read` on the element's view as it is now.
async fn read(
    server: Arc<Server>,
    sid: String,
    eid: String,
    read: impl FnOnce(&SnapshotNode) -> Value + Send + 'static,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let id = parse_id(&eid)?;
    let node = blocking(move || check(&cur, id)).await??;
    Ok(value(read(&node)))
}

/// Runs `step` once the element is known to be there and is the one found.
async fn act(
    server: Arc<Server>,
    sid: String,
    eid: String,
    step: impl FnOnce(&Current, u32) -> Result<(), WdError> + Send + 'static,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let id = parse_id(&eid)?;
    blocking(move || {
        check(&cur, id)?;
        step(&cur, id)
    })
    .await??;
    Ok(value(Value::Null))
}

async fn click(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<Response, WdError> {
    act(server, sid, eid, |cur, id| {
        cur.session
            .call("Input.press", json!({ "selector": view_selector(id) }))?;
        Ok(())
    })
    .await
}

async fn clear(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<Response, WdError> {
    act(server, sid, eid, |cur, id| {
        cur.session
            .call("Input.clear", json!({ "selector": view_selector(id) }))?;
        Ok(())
    })
    .await
}

/// The text a send-keys body carries: `text`, or the characters of the
/// legacy `value` array.
fn keys_of(body: &Value) -> Result<String, WdError> {
    if let Some(text) = body.get("text").and_then(Value::as_str) {
        return Ok(text.to_owned());
    }
    match body.get("value").and_then(Value::as_array) {
        Some(parts) => Ok(parts.iter().filter_map(Value::as_str).collect()),
        None => Err(WdError::invalid_argument("`text` is not a string")),
    }
}

async fn send_value(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, WdError> {
    let text = keys_of(&body_json(&body)?)?;
    // Text is typed; the keys WebDriver writes as Private Use characters are
    // pressed (`keys.rs`).
    let plan = super::keys::plan(&text);
    act(server, sid, eid, move |cur, id| {
        // `Input.key` goes to whatever has the focus, so a plan that starts
        // with keys focuses the element first, as W3C's Element Send Keys
        // does; `Input.type` with no text focuses the input and types nothing.
        if matches!(plan.first(), Some(Sending::Keys(_))) {
            cur.session.call(
                "Input.type",
                json!({ "selector": view_selector(id), "text": "" }),
            )?;
        }
        for send in plan {
            match send {
                Sending::Type(text) => {
                    cur.session.call(
                        "Input.type",
                        json!({ "selector": view_selector(id), "text": text }),
                    )?;
                }
                Sending::Keys(actions) => {
                    cur.session
                        .call("Input.key", json!({ "actions": actions }))?;
                }
            }
        }
        Ok(())
    })
    .await
}

async fn text(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<Response, WdError> {
    read(server, sid, eid, |n| json!(text_of(n))).await
}

async fn name(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<Response, WdError> {
    read(server, sid, eid, |n| json!(n.kind)).await
}

async fn attr(
    State(server): State<Arc<Server>>,
    Path((sid, eid, name)): Path<(String, String, String)>,
) -> Result<Response, WdError> {
    read(server, sid, eid, move |n| {
        attribute(n, &name).map_or(Value::Null, Value::String)
    })
    .await
}

async fn rect(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<Response, WdError> {
    read(server, sid, eid, |n| {
        let [x, y, width, height] = n.bounds.unwrap_or_default();
        json!({ "x": x, "y": y, "width": width, "height": height })
    })
    .await
}

async fn displayed(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<Response, WdError> {
    read(server, sid, eid, |n| json!(n.visible)).await
}

async fn enabled(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<Response, WdError> {
    read(server, sid, eid, |n| json!(n.enabled)).await
}

async fn selected(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<Response, WdError> {
    read(server, sid, eid, |n| {
        // `checked` is `false`, `true` or `mixed`.
        json!(n.selected == Some(true) || matches!(n.checked.as_deref(), Some("true" | "checked")))
    })
    .await
}

async fn screenshot(
    State(server): State<Arc<Server>>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<Response, WdError> {
    let cur = server.current(&sid)?;
    let id = parse_id(&eid)?;
    let png = blocking(move || {
        check(&cur, id)?;
        Ok::<_, WdError>(
            cur.session
                .call("Capture.screenshot", json!({ "node": id }))?,
        )
    })
    .await??;
    Ok(value(png))
}

#[cfg(test)]
mod devtools_webdriver {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use nsh_devtools::protocol::{ERR_STALE, ErrorBody};
    use tower::ServiceExt as _;

    use super::*;
    use crate::devtools::webdriver::{server::Launch, session::Caps};

    type Handler = Arc<dyn Fn(&str, &Value) -> Result<Value, ErrorBody> + Send + Sync>;
    type Calls = Arc<Mutex<Vec<(String, Value)>>>;

    /// An app that answers with `handler` and records what it was asked.
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

    /// A snapshot node as the app answers it.
    fn node(id: u32, depth: u32, kind: &str) -> Value {
        json!({
            "id": id, "depth": depth, "kind": kind,
            "visible": true, "enabled": true, "focused": false,
        })
    }

    fn with(mut node: Value, key: &str, value: Value) -> Value {
        node[key] = value;
        node
    }

    fn el(id: u32) -> Value {
        json!({ ELEMENT_KEY: id.to_string() })
    }

    #[tokio::test]
    async fn find_by_id_maps_to_an_id_selector() {
        let rig = rig(|method, _| match method {
            "Tree.query" => Ok(json!({ "ids": [7] })),
            "Tree.snapshot" => Ok(json!([node(7, 0, "view")])),
            other => panic!("{other}"),
        })
        .await;
        let body = json!({ "using": "id", "value": "go" });
        let (status, answer) = rig.send("POST", "/element", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["value"], el(7));
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls[0].0, "Tree.query");
        assert_eq!(calls[0].1["selector"], "id:go");
        assert!(calls[0].1["root"].is_null());
    }

    #[tokio::test]
    async fn find_by_accessibility_id_maps_to_label() {
        let rig = rig(|method, _| match method {
            "Tree.query" => Ok(json!({ "ids": [3, 9] })),
            "Tree.snapshot" => Ok(json!([node(3, 0, "view"), node(9, 0, "view")])),
            other => panic!("{other}"),
        })
        .await;
        let body = json!({ "using": "accessibility id", "value": "Add" });
        let (status, answer) = rig.send("POST", "/elements", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["value"], json!([el(3), el(9)]));
        assert_eq!(rig.calls.lock().unwrap()[0].1["selector"], "label:Add");
    }

    #[tokio::test]
    async fn find_by_xpath_uses_the_snapshot() {
        let rig = rig(|method, _| match method {
            "Tree.snapshot" => Ok(json!([
                node(1, 0, "view"),
                with(node(3, 1, "view"), "test_id", json!("submit")),
                with(node(4, 1, "text"), "text", json!("it's \"x\" & <y>")),
            ])),
            other => panic!("{other}"),
        })
        .await;
        let body = json!({ "using": "xpath", "value": "//view[@name='submit']" });
        let (status, answer) = rig.send("POST", "/element", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["value"], el(3));
        assert_eq!(rig.methods(), ["Tree.snapshot"]);
        let body = json!({ "using": "xpath", "value": "//[" });
        let (status, answer) = rig.send("POST", "/element", Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(answer["value"]["error"], "invalid selector");
    }

    #[tokio::test]
    async fn class_name_and_scope_filter_the_snapshot() {
        let rig = rig(|method, _| match method {
            "Tree.describe" => Ok(node(2, 1, "view")),
            "Tree.snapshot" => Ok(json!([
                node(1, 0, "text"),
                node(2, 1, "view"),
                node(3, 2, "text"),
                node(4, 1, "text"),
            ])),
            other => panic!("{other}"),
        })
        .await;
        let body = json!({ "using": "class name", "value": "text" });
        let (_, all) = rig.send("POST", "/elements", Some(body.clone())).await;
        assert_eq!(all["value"], json!([el(1), el(3), el(4)]));
        let (_, scoped) = rig.send("POST", "/element/2/elements", Some(body)).await;
        assert_eq!(scoped["value"], json!([el(3)]));
    }

    #[tokio::test]
    async fn find_nothing_is_no_such_element() {
        let rig = rig(|_, _| Ok(json!({ "ids": [] }))).await;
        let body = json!({ "using": "id", "value": "nope" });
        let (status, answer) = rig.send("POST", "/element", Some(body.clone())).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(answer["value"]["error"], "no such element");
        let (status, answer) = rig.send("POST", "/elements", Some(body)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(answer["value"], json!([]));
    }

    #[tokio::test]
    async fn implicit_wait_retries_until_found() {
        let asked = Arc::new(AtomicUsize::new(0));
        let counted = asked.clone();
        let rig = rig(move |method, _| match method {
            "Tree.query" => {
                let ids = if counted.fetch_add(1, Ordering::SeqCst) < 2 {
                    json!([])
                } else {
                    json!([5])
                };
                Ok(json!({ "ids": ids }))
            }
            "Tree.snapshot" => Ok(json!([node(5, 0, "view")])),
            other => panic!("{other}"),
        })
        .await;
        let (status, _) = rig
            .send("POST", "/timeouts", Some(json!({ "implicit": 1000 })))
            .await;
        assert_eq!(status, StatusCode::OK);
        let body = json!({ "using": "id", "value": "late" });
        let (status, answer) = rig.send("POST", "/element", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["value"], el(5));
        assert_eq!(asked.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn click_sends_input_press_on_view_selector() {
        let rig = rig(|method, _| match method {
            "Tree.describe" => Ok(node(7, 0, "view")),
            "Input.press" => Ok(Value::Null),
            other => panic!("{other}"),
        })
        .await;
        let (status, answer) = rig.send("POST", "/element/7/click", Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer, json!({ "value": null }));
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls[1].0, "Input.press");
        assert_eq!(calls[1].1, json!({ "selector": "view:7" }));
    }

    #[tokio::test]
    async fn click_on_a_stale_element_is_404_stale() {
        let rig = rig(|method, _| match method {
            "Tree.describe" => Err(ErrorBody::new(ERR_STALE, "stale: view 7 is gone")),
            other => panic!("{other}"),
        })
        .await;
        let (status, answer) = rig.send("POST", "/element/7/click", Some(json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(answer["value"]["error"], "stale element reference");
        // Nothing was pressed: not that view, not a neighbour.
        assert_eq!(rig.methods(), ["Tree.describe"]);
    }

    #[tokio::test]
    async fn recycled_cell_with_new_content_is_stale() {
        let shown = Arc::new(Mutex::new("Row A".to_owned()));
        let now = shown.clone();
        let rig = rig(move |method, _| {
            let cell = || {
                let mut cell = with(node(9, 1, "view"), "test_id", json!("row"));
                cell["text"] = json!(now.lock().unwrap().clone());
                cell["recycled"] = json!(true);
                cell
            };
            match method {
                "Tree.query" => Ok(json!({ "ids": [9] })),
                "Tree.snapshot" => Ok(json!([cell()])),
                "Tree.describe" => Ok(cell()),
                "Input.press" => Ok(Value::Null),
                other => panic!("{other}"),
            }
        })
        .await;
        let body = json!({ "using": "id", "value": "row" });
        let (_, found) = rig.send("POST", "/element", Some(body)).await;
        assert_eq!(found["value"], el(9));
        // Same content: the cell is the element found.
        let (status, _) = rig.send("POST", "/element/9/click", Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK);
        // The list recycled the cell for another row.
        *shown.lock().unwrap() = "Row Z".to_owned();
        let (status, answer) = rig.send("POST", "/element/9/click", Some(json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(answer["value"]["error"], "stale element reference");
        let presses = rig.methods().iter().filter(|m| *m == "Input.press").count();
        assert_eq!(presses, 1);
    }

    #[tokio::test]
    async fn text_falls_back_to_value_then_label() {
        let rig = rig(|method, params| {
            assert_eq!(method, "Tree.describe");
            Ok(match params["id"].as_u64().unwrap() {
                1 => {
                    let n = with(node(1, 0, "text"), "text", json!("shown"));
                    with(with(n, "value", json!("typed")), "label", json!("named"))
                }
                2 => with(
                    with(node(2, 0, "input"), "value", json!("typed")),
                    "label",
                    json!("named"),
                ),
                3 => with(node(3, 0, "view"), "label", json!("named")),
                _ => node(4, 0, "view"),
            })
        })
        .await;
        for (id, expected) in [(1, "shown"), (2, "typed"), (3, "named"), (4, "")] {
            let (status, answer) = rig.send("GET", &format!("/element/{id}/text"), None).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(answer["value"], expected, "element {id}");
        }
    }

    #[tokio::test]
    async fn rect_is_the_bounds() {
        let rig = rig(|_, _| {
            Ok(with(
                with(
                    node(1, 0, "view"),
                    "bounds",
                    json!([10.0, 20.0, 30.5, 40.0]),
                ),
                "test_id",
                json!("box"),
            ))
        })
        .await;
        let (_, answer) = rig.send("GET", "/element/1/rect", None).await;
        assert_eq!(
            answer["value"],
            json!({ "x": 10.0, "y": 20.0, "width": 30.5, "height": 40.0 })
        );
        let (_, answer) = rig.send("GET", "/element/1/attribute/name", None).await;
        assert_eq!(answer["value"], "box");
        let (_, answer) = rig.send("GET", "/element/1/attribute/width", None).await;
        assert_eq!(answer["value"], "31");
        let (_, answer) = rig.send("GET", "/element/1/property/nothing", None).await;
        assert_eq!(answer["value"], Value::Null);
        let (_, answer) = rig.send("GET", "/element/1/name", None).await;
        assert_eq!(answer["value"], "view");
        let (_, answer) = rig.send("GET", "/element/1/displayed", None).await;
        assert_eq!(answer["value"], true);
    }

    #[tokio::test]
    async fn unknown_strategy_is_invalid_argument() {
        let rig = rig(|method, _| panic!("{method}")).await;
        let body = json!({ "using": "css selector", "value": ".x" });
        let (status, answer) = rig.send("POST", "/element", Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(answer["value"]["error"], "invalid argument");
        assert!(rig.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn value_types_and_clear_clears() {
        let rig = rig(|method, _| match method {
            "Tree.describe" => Ok(node(5, 0, "input")),
            _ => Ok(Value::Null),
        })
        .await;
        let (status, _) = rig
            .send("POST", "/element/5/value", Some(json!({ "text": "héllo" })))
            .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = rig.send("POST", "/element/5/clear", Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK);
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls[1].1, json!({ "selector": "view:5", "text": "héllo" }));
        assert_eq!(calls[3].0, "Input.clear");
    }

    #[tokio::test]
    async fn send_keys_types_text_and_presses_keys() {
        let rig = rig(|method, _| match method {
            "Tree.describe" => Ok(node(5, 0, "input")),
            _ => Ok(Value::Null),
        })
        .await;
        let body = json!({ "text": "ab\u{E007}c" });
        let (status, _) = rig.send("POST", "/element/5/value", Some(body)).await;
        assert_eq!(status, StatusCode::OK);
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls[1].0, "Input.type");
        assert_eq!(calls[1].1, json!({ "selector": "view:5", "text": "ab" }));
        assert_eq!(calls[2].0, "Input.key");
        assert_eq!(
            calls[2].1,
            json!({ "actions": [
                { "type": "down", "key": "enter" },
                { "type": "up", "key": "enter" },
            ] })
        );
        assert_eq!(calls[3].1, json!({ "selector": "view:5", "text": "c" }));
    }

    #[tokio::test]
    async fn send_keys_special_key_first_focuses_element() {
        let rig = rig(|method, _| match method {
            "Tree.describe" => Ok(node(5, 0, "input")),
            _ => Ok(Value::Null),
        })
        .await;
        let body = json!({ "text": "\u{E007}" });
        let (status, _) = rig.send("POST", "/element/5/value", Some(body)).await;
        assert_eq!(status, StatusCode::OK);
        let calls = rig.calls.lock().unwrap();
        assert_eq!(calls[1].0, "Input.type");
        assert_eq!(calls[1].1, json!({ "selector": "view:5", "text": "" }));
        assert_eq!(calls[2].0, "Input.key");
    }

    #[tokio::test]
    async fn active_element_is_the_focused_node() {
        let rig = rig(|_, _| {
            Ok(json!([
                node(1, 0, "view"),
                with(node(2, 1, "input"), "focused", json!(true)),
            ]))
        })
        .await;
        let (_, answer) = rig.send("GET", "/element/active", None).await;
        assert_eq!(answer["value"], el(2));
    }
}
