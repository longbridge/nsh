//! The HTTP side of the WebDriver server: the routes, the one session and the
//! errors' answers. Handlers run the blocking [`Session`] on tokio's blocking
//! pool.

use std::{
    net::IpAddr,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::Bytes,
    extract::{Path, Request, State},
    http::{Method, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use serde_json::{Value, json};

use super::{
    element::{self, Elements},
    error::WdError,
    session::{Caps, Open, Timeouts},
};
use crate::devtools::Session;

/// How long deleting a session waits for the app to quit before it is closed
/// anyway.
const QUIT: Duration = Duration::from_secs(5);

/// Starts the app a new session drives.
pub(crate) trait Launch: Send + Sync {
    /// Launches the app `caps` name; the session's drop closes it.
    fn launch(&self, caps: &Caps) -> Result<Box<dyn Session>, WdError>;
}

/// The server's state: how it launches apps and the session it holds (one at
/// a time).
pub(crate) struct Server {
    launch: Box<dyn Launch>,
    /// A relative `appium:app` is this directory's.
    app_dir: PathBuf,
    session: Mutex<Option<Open>>,
    /// An app is starting: another session is refused, not queued.
    launching: AtomicBool,
}

impl Server {
    /// A server that starts apps with `launch`.
    pub(crate) fn new(launch: Box<dyn Launch>, app_dir: PathBuf) -> Server {
        Server {
            launch,
            app_dir,
            session: Mutex::new(None),
            launching: AtomicBool::new(false),
        }
    }

    /// The session `sid` names: what a command needs of it, copied out so
    /// the lock is not held while the app answers.
    pub(crate) fn current(&self, sid: &str) -> Result<Current, WdError> {
        match self.open().as_ref().filter(|o| o.id == sid) {
            Some(open) => Ok(Current {
                session: open.session.clone(),
                timeouts: open.timeouts,
                elements: open.elements.clone(),
            }),
            None => Err(WdError::invalid_session_id(sid)),
        }
    }

    fn open(&self) -> MutexGuard<'_, Option<Open>> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ends the session, if one is open: the held input is let go, the app
    /// quits and goes.
    pub(crate) fn close(&self) {
        let open = self.open().take();
        if let Some(open) = open {
            end(open);
        }
    }
}

/// A session's parts a command works with.
pub(crate) struct Current {
    /// The app's protocol.
    pub(crate) session: Arc<dyn Session>,
    /// The client's timeouts when the command arrived.
    pub(crate) timeouts: Timeouts,
    /// The elements the client found.
    pub(crate) elements: Elements,
}

/// Lets go of the input, quits the app and drops the session (which closes
/// the app if it did not quit).
fn end(open: Open) {
    // A button held by a client that went away must not outlive the session.
    let _ = open.session.call("Session.release", Value::Null);
    let _ = open.session.call_within("Session.quit", Value::Null, QUIT);
}

/// Clears `launching` however the launch ends.
struct Launching<'a>(&'a AtomicBool);

impl Drop for Launching<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The server's routes.
pub(crate) fn router(state: Arc<Server>) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/session", axum::routing::post(new_session))
        .route("/session/{sid}", delete(delete_session))
        .route(
            "/session/{sid}/timeouts",
            get(get_timeouts).post(set_timeouts),
        )
        .merge(element::routes())
        .merge(super::actions::routes())
        .fallback(unknown_route)
        .method_not_allowed_fallback(unknown_method)
        .with_state(state)
}

/// `router` behind the checks of a server that listens on `port` of the
/// loopback. A web page in the user's browser is a loopback client too: a
/// request it sends carries an `Origin` header (refused), and a rebound DNS
/// name arrives with another `Host` (refused); a `POST` must say it is JSON,
/// which a cross-site form or `text/plain` fetch cannot do without a
/// preflight the server never answers.
pub(crate) fn guard(router: Router, port: u16) -> Router {
    router.layer(middleware::from_fn_with_state(port, check_request))
}

async fn check_request(State(port): State<u16>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    if headers.contains_key(header::ORIGIN) {
        return forbidden("a request from a web page is refused");
    }
    let host_ok = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| {
            let h = h.to_ascii_lowercase();
            [
                format!("127.0.0.1:{port}"),
                format!("localhost:{port}"),
                format!("[::1]:{port}"),
            ]
            .contains(&h)
        });
    if !host_ok {
        return forbidden("the Host header is not the server's own address");
    }
    if request.method() == Method::POST {
        let json = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"));
        if !json {
            return WdError::invalid_argument("a POST needs Content-Type: application/json")
                .into_response();
        }
    }
    next.run(request).await
}

fn forbidden(message: &str) -> Response {
    let mut e = WdError::unknown_error(message);
    e.status = StatusCode::FORBIDDEN;
    e.into_response()
}

/// `{"value": value}` as JSON.
pub(crate) fn value(value: Value) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        json!({ "value": value }).to_string(),
    )
        .into_response()
}

/// Runs `work` on the blocking pool.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, WdError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| WdError::unknown_error(format!("the command failed: {e}")))
}

pub(crate) fn body_json(body: &Bytes) -> Result<Value, WdError> {
    serde_json::from_slice(body)
        .map_err(|e| WdError::invalid_argument(format!("the body is not JSON: {e}")))
}

async fn unknown_route(uri: Uri) -> WdError {
    WdError::unknown_command(format!("no command at {}", uri.path()))
}

async fn unknown_method(uri: Uri) -> WdError {
    WdError::unknown_method(format!("{} does not take this method", uri.path()))
}

async fn status(State(server): State<Arc<Server>>) -> Response {
    let ready = server.open().is_none() && !server.launching.load(Ordering::SeqCst);
    value(json!({
        "ready": ready,
        "message": if ready {
            "NSH WebDriver server: ready for a session"
        } else {
            "NSH WebDriver server: a session is open (one at a time)"
        },
    }))
}

async fn new_session(State(server): State<Arc<Server>>, body: Bytes) -> Result<Response, WdError> {
    let caps = Caps::parse(&body_json(&body)?, &server.app_dir)?;
    let busy = || WdError::session_not_created("a session is open: the server takes one at a time");
    if server
        .launching
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(busy());
    }
    let launching = Launching(&server.launching);
    if server.open().is_some() {
        return Err(busy());
    }
    let launched = {
        let server = server.clone();
        let caps = caps.clone();
        blocking(move || server.launch.launch(&caps)).await??
    };
    let id = new_id();
    *server.open() = Some(Open {
        id: id.clone(),
        session: Arc::from(launched),
        timeouts: Timeouts::default(),
        elements: Elements::default(),
    });
    drop(launching);
    Ok(value(
        json!({ "sessionId": id, "capabilities": caps.answer() }),
    ))
}

async fn delete_session(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    let open = {
        let mut open = server.open();
        if open.as_ref().is_none_or(|o| o.id != sid) {
            return Err(WdError::invalid_session_id(&sid));
        }
        open.take()
    };
    if let Some(open) = open {
        blocking(move || end(open)).await?;
    }
    Ok(value(Value::Null))
}

async fn get_timeouts(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
) -> Result<Response, WdError> {
    let open = server.open();
    match open.as_ref().filter(|o| o.id == sid) {
        Some(open) => Ok(value(open.timeouts.to_json())),
        None => Err(WdError::invalid_session_id(&sid)),
    }
}

async fn set_timeouts(
    State(server): State<Arc<Server>>,
    Path(sid): Path<String>,
    body: Bytes,
) -> Result<Response, WdError> {
    let body = body_json(&body)?;
    let mut open = server.open();
    match open.as_mut().filter(|o| o.id == sid) {
        Some(open) => {
            open.timeouts.set(&body)?;
            Ok(value(Value::Null))
        }
        None => Err(WdError::invalid_session_id(&sid)),
    }
}

/// A session id: a version 4 UUID's shape from the endpoint's token source.
fn new_id() -> String {
    let hex = crate::devtools::token();
    format!(
        "{}-{}-4{}-a{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}

/// The address to listen on: loopback only, as the DevTools endpoint is.
pub(crate) fn check_host(host: &str) -> Result<IpAddr, String> {
    let ip: IpAddr = if host.eq_ignore_ascii_case("localhost") {
        IpAddr::from([127, 0, 0, 1])
    } else {
        host.parse()
            .map_err(|_| format!("--host {host}: not an IP address"))?
    };
    if !ip.is_loopback() {
        return Err(format!(
            "--host {host}: the WebDriver server listens on loopback only (it drives an app \
             without a login)"
        ));
    }
    Ok(ip)
}

#[cfg(test)]
mod devtools_webdriver {
    use axum::{body::Body, http::Request};
    use nsh_devtools::protocol::ErrorBody;
    use tower::ServiceExt as _;

    use super::*;

    type Calls = Arc<Mutex<Vec<String>>>;

    /// An app that records the methods called on it.
    struct FakeSession(Calls);

    impl Session for FakeSession {
        fn call_within(
            &self,
            method: &str,
            _params: Value,
            _timeout: Duration,
        ) -> Result<Value, ErrorBody> {
            self.0.lock().unwrap().push(method.to_owned());
            Ok(json!({}))
        }
    }

    /// Launches fake apps, keeping the capabilities asked for.
    #[derive(Default)]
    struct FakeLaunch {
        calls: Calls,
        asked: Arc<Mutex<Vec<Caps>>>,
    }

    impl Launch for FakeLaunch {
        fn launch(&self, caps: &Caps) -> Result<Box<dyn Session>, WdError> {
            self.asked.lock().unwrap().push(caps.clone());
            Ok(Box::new(FakeSession(self.calls.clone())))
        }
    }

    struct Rig {
        router: Router,
        calls: Calls,
        asked: Arc<Mutex<Vec<Caps>>>,
    }

    fn rig() -> Rig {
        let launch = FakeLaunch::default();
        let calls = launch.calls.clone();
        let asked = launch.asked.clone();
        let server = Server::new(Box::new(launch), PathBuf::from("/apps"));
        Rig {
            router: router(Arc::new(server)),
            calls,
            asked,
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

    fn new_session_body(platform: &str) -> Value {
        json!({ "capabilities": { "alwaysMatch": {
            "platformName": platform,
            "appium:app": "counter/App.tsx",
            "appium:automationName": "NSH",
        } } })
    }

    async fn open(rig: &Rig) -> String {
        let (status, body) = send(
            &rig.router,
            "POST",
            "/session",
            Some(new_session_body("mac")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["value"]["sessionId"].as_str().unwrap().to_owned()
    }

    async fn guarded(method: &str, uri: &str, headers: &[(&str, &str)]) -> (StatusCode, Value) {
        let rig = rig();
        let router = guard(rig.router, 4723);
        let mut request = Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            request = request.header(*k, *v);
        }
        let body = if method == "POST" {
            Body::from(new_session_body("mac").to_string())
        } else {
            Body::empty()
        };
        let response = router.oneshot(request.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn a_web_page_cannot_reach_the_server() {
        let (status, body) = guarded(
            "GET",
            "/status",
            &[
                ("host", "127.0.0.1:4723"),
                ("origin", "https://example.com"),
            ],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["value"]["error"], "unknown error");
        let (status, _) = guarded("GET", "/status", &[("host", "evil.example:4723")]).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = guarded("GET", "/status", &[("host", "127.0.0.1:9")]).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) = guarded(
            "POST",
            "/session",
            &[("host", "127.0.0.1:4723"), ("content-type", "text/plain")],
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["value"]["error"], "invalid argument");
        for host in ["127.0.0.1:4723", "localhost:4723", "[::1]:4723"] {
            let (status, _) = guarded("GET", "/status", &[("host", host)]).await;
            assert_eq!(status, StatusCode::OK, "{host}");
        }
        let (status, body) = guarded(
            "POST",
            "/session",
            &[
                ("host", "localhost:4723"),
                ("content-type", "application/json; charset=utf-8"),
            ],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    #[tokio::test]
    async fn status_is_ready_without_a_session() {
        let rig = rig();
        let (status, body) = send(&rig.router, "GET", "/status", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["value"]["ready"], true);
        open(&rig).await;
        let (_, body) = send(&rig.router, "GET", "/status", None).await;
        assert_eq!(body["value"]["ready"], false);
    }

    #[tokio::test]
    async fn new_session_launches_and_answers_capabilities() {
        let rig = rig();
        let body = json!({ "capabilities": {
            "alwaysMatch": { "platformName": "mac", "appium:app": "counter/App.tsx" },
            "firstMatch": [{
                "appium:automationName": "Whatever",
                "nsh:options": { "storage": { "k": "v" }, "environment": { "clipboard": "hi" } },
            }],
        } });
        let (status, answer) = send(&rig.router, "POST", "/session", Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let value = &answer["value"];
        assert_eq!(value["sessionId"].as_str().unwrap().len(), 36);
        assert_eq!(
            value["capabilities"],
            json!({
                "platformName": "mac",
                "appium:app": "counter/App.tsx",
                "appium:automationName": "NSH",
            })
        );
        let asked = rig.asked.lock().unwrap();
        // A relative app is the app directory's.
        assert_eq!(asked[0].app, PathBuf::from("/apps/counter/App.tsx"));
        assert_eq!(asked[0].storage, vec![("k".to_owned(), "v".to_owned())]);
        assert_eq!(asked[0].environment.clipboard.as_deref(), Some("hi"));
    }

    #[tokio::test]
    async fn one_session_at_a_time() {
        let rig = rig();
        open(&rig).await;
        let (status, body) = send(
            &rig.router,
            "POST",
            "/session",
            Some(new_session_body("mac")),
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["value"]["error"], "session not created");
        assert_eq!(rig.asked.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unknown_session_is_404() {
        let rig = rig();
        let (status, body) = send(&rig.router, "GET", "/session/nope/timeouts", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["value"]["error"], "invalid session id");
        let (status, body) = send(&rig.router, "DELETE", "/session/nope", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["value"]["error"], "invalid session id");
        // A wrong id while another session is open is as unknown.
        open(&rig).await;
        let (status, _) = send(&rig.router, "DELETE", "/session/nope", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(rig.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_route_is_unknown_command() {
        let rig = rig();
        let (status, body) = send(&rig.router, "GET", "/nothing/here", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["value"]["error"], "unknown command");
        assert_eq!(body["value"]["stacktrace"], "");
        let (status, body) = send(&rig.router, "PUT", "/status", None).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(body["value"]["error"], "unknown method");
    }

    #[tokio::test]
    async fn delete_session_quits_the_app() {
        let rig = rig();
        let sid = open(&rig).await;
        let (status, body) = send(&rig.router, "DELETE", &format!("/session/{sid}"), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "value": null }));
        let calls = rig.calls.lock().unwrap().clone();
        // Input is let go before the app quits.
        assert_eq!(calls, ["Session.release", "Session.quit"]);
        // The server takes a new session again.
        let (_, body) = send(&rig.router, "GET", "/status", None).await;
        assert_eq!(body["value"]["ready"], true);
    }

    #[tokio::test]
    async fn platform_name_is_required() {
        let rig = rig();
        let body = json!({ "capabilities": { "alwaysMatch": { "appium:app": "a.tsx" } } });
        let (status, answer) = send(&rig.router, "POST", "/session", Some(body)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(answer["value"]["error"], "session not created");
        let (status, answer) = send(
            &rig.router,
            "POST",
            "/session",
            Some(new_session_body("linux")),
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(answer["value"]["error"], "session not created");
        // `appium:app` is required too.
        let body = json!({ "capabilities": { "alwaysMatch": { "platformName": "windows" } } });
        let (_, answer) = send(&rig.router, "POST", "/session", Some(body)).await;
        assert_eq!(answer["value"]["error"], "session not created");
        assert!(rig.asked.lock().unwrap().is_empty());
        // A launch failed, so the server is free.
        open(&rig).await;
    }

    #[tokio::test]
    async fn timeouts_are_kept() {
        let rig = rig();
        let sid = open(&rig).await;
        let uri = format!("/session/{sid}/timeouts");
        let (_, body) = send(&rig.router, "GET", &uri, None).await;
        assert_eq!(
            body["value"],
            json!({ "implicit": 0, "pageLoad": 300000, "script": 30000 })
        );
        let (status, _) = send(&rig.router, "POST", &uri, Some(json!({ "implicit": 2500 }))).await;
        assert_eq!(status, StatusCode::OK);
        let (_, body) = send(&rig.router, "GET", &uri, None).await;
        assert_eq!(body["value"]["implicit"], 2500);
        assert_eq!(body["value"]["pageLoad"], 300000);
        let (status, body) = send(&rig.router, "POST", &uri, Some(json!({ "implicit": -1 }))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["value"]["error"], "invalid argument");
    }

    #[test]
    fn non_loopback_host_is_refused() {
        assert_eq!(check_host("127.0.0.1"), Ok(IpAddr::from([127, 0, 0, 1])));
        assert!(check_host("::1").is_ok());
        assert!(check_host("localhost").is_ok());
        assert!(check_host("0.0.0.0").is_err());
        assert!(check_host("192.168.1.5").is_err());
        assert!(check_host("example.com").is_err());
    }
}
