//! The endpoint: a loopback WebSocket server inside the app.
//!
//! Threads: one accepts, one serves each connection, and one forwards the
//! console to each connection that subscribed. A connection thread owns its
//! socket: answers and events reach it through a channel and it writes them
//! between reads (the read timeout is [`POLL`], so a write waits that long at
//! most). Handlers answer steps from any thread through the `done` callback.
//!
//! Locks, never held across a call into the handler or a socket: `conns`
//! (the registry), `input` (the connection holding the input), `state` (the
//! session's setup and whether it started) and each `Conn`'s sender.

use std::{
    collections::HashMap,
    io::{self, ErrorKind},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use nsh_dev::protocol::{FetchReply, TestEnvironment, TestStep};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tungstenite::{
    Message, WebSocket,
    handshake::server::{ErrorResponse, Request as HttpRequest, Response as HttpResponse},
    http,
};

use crate::{
    ENV_LISTEN, ENV_PORT_FILE, ENV_TOKEN, ENV_WAIT, PATH, PROTOCOL_VERSION,
    console::{Collector, ConsoleQuery},
    description::description,
    methods::{self, Kind, METHODS, PRIVILEGED},
    protocol::{
        ERR_BAD_PARAMS, ERR_INPUT_LOCKED, ERR_NOT_STARTED, ERR_STALE, ERR_STEP_FAILED, ErrorBody,
        Event, Request, Response,
    },
};

/// How long a connection thread waits for a frame before it looks at what is
/// waiting to be sent.
const POLL: Duration = Duration::from_millis(5);

/// What a platform gives the endpoint to act on its app.
pub trait Handler: Send + Sync + 'static {
    /// Applies what the client set up before the app starts (storage, trace,
    /// environment).
    fn setup(&self, setup: SessionSetup) -> Result<(), ErrorBody>;
    /// The client asked the app to start.
    fn start(&self);
    /// Runs a step and calls `done` once, from any thread, with its outcome
    /// value or a failure message.
    fn step(&self, step: TestStep, done: Box<dyn FnOnce(Result<Value, String>) + Send>);
    /// The client's answer to the app's request `id` (`Network.respond`).
    fn fetch_reply(&self, id: u64, reply: FetchReply);
}

/// What `Session.setup` fixes before the app starts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(default)]
pub struct SessionSetup {
    /// Storage entries the app starts with.
    pub storage: Vec<(String, String)>,
    /// Whether the app reports the batches it applies (`Trace.batch`).
    pub trace: bool,
    /// The environment the app sees.
    pub environment: TestEnvironment,
}

/// How an endpoint is set up.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EndpointConfig {
    /// The address to listen on; loopback only.
    pub listen: SocketAddr,
    /// The token clients present; empty makes the endpoint generate one.
    pub token: String,
    /// A file the listening URL is written to.
    pub port_file: Option<PathBuf>,
    /// Whether the app waits for `Session.start` ([`Endpoint::wait_for_start`])
    /// and refuses steps until then.
    pub wait: bool,
    /// How long [`Endpoint::wait_for_start`] waits.
    pub wait_timeout: Duration,
    /// The app's id, for `Session.hello`.
    pub app_id: String,
    /// The platform's name, for `Session.hello`.
    pub platform: String,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        EndpointConfig {
            listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            token: String::new(),
            port_file: None,
            wait: false,
            wait_timeout: Duration::from_secs(60),
            app_id: String::new(),
            platform: std::env::consts::OS.to_string(),
        }
    }
}

impl EndpointConfig {
    /// The configuration the environment asks for; `None` when
    /// [`ENV_LISTEN`] is unset or names an address that is not loopback (a
    /// warning says so, never the token). `1` or an empty value listens on a
    /// free loopback port.
    pub fn from_env() -> Option<EndpointConfig> {
        let listen = std::env::var(ENV_LISTEN).ok()?;
        let mut config = EndpointConfig::default();
        let listen = listen.trim();
        if !(listen.is_empty() || listen == "1") {
            match listen.parse::<SocketAddr>() {
                Ok(addr) => config.listen = addr,
                Err(_) => {
                    log::warn!("{ENV_LISTEN}={listen:?} is not an address: DevTools is off");
                    return None;
                }
            }
        }
        if !config.listen.ip().is_loopback() {
            log::warn!(
                "{ENV_LISTEN} names {}, which is not a loopback address: DevTools is off",
                config.listen
            );
            return None;
        }
        config.token = std::env::var(ENV_TOKEN).unwrap_or_default();
        config.port_file = std::env::var_os(ENV_PORT_FILE).map(PathBuf::from);
        config.wait =
            std::env::var(ENV_WAIT).is_ok_and(|v| !matches!(v.trim(), "" | "0" | "false"));
        Some(config)
    }
}

/// [`Endpoint::wait_for_start`] ran out of time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct WaitTimedOut;

impl std::fmt::Display for WaitTimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no DevTools client started the session in time")
    }
}

impl std::error::Error for WaitTimedOut {}

/// The session's state: its setup, until it starts.
#[derive(Default)]
struct State {
    setup: Option<SessionSetup>,
    /// A `Session.start` claimed the start (it may still be running).
    started: bool,
    /// The handler's setup and start returned.
    ready: bool,
}

/// One client.
struct Conn {
    id: u64,
    out: Sender<String>,
    closed: AtomicBool,
    subscribed: AtomicBool,
}

impl Conn {
    /// Queues a text frame; dropped silently once the connection is gone.
    fn send(&self, text: String) {
        if !self.closed.load(Ordering::Acquire) {
            let _ = self.out.send(text);
        }
    }

    fn respond(&self, response: &Response) {
        if let Ok(text) = serde_json::to_string(response) {
            self.send(text);
        }
    }
}

struct Shared {
    handler: Arc<dyn Handler>,
    console: Arc<Collector>,
    config: EndpointConfig,
    token: String,
    stop: AtomicBool,
    next_conn: AtomicU64,
    conns: Mutex<HashMap<u64, Arc<Conn>>>,
    /// The connection holding the input.
    input: Mutex<Option<u64>>,
    state: Mutex<State>,
    started: Condvar,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// A running endpoint; dropping it stops listening and ends the connections.
pub struct Endpoint {
    shared: Arc<Shared>,
    addr: SocketAddr,
    accept: Option<JoinHandle<()>>,
}

impl Endpoint {
    /// Listens on `config.listen`, writes the port file and prints the
    /// announcement line (the URL; the token too when the endpoint made it).
    ///
    /// # Errors
    ///
    /// The address is not loopback, or cannot be bound, or the port file
    /// cannot be written.
    pub fn start(
        config: EndpointConfig,
        handler: Arc<dyn Handler>,
        console: Arc<Collector>,
    ) -> io::Result<Endpoint> {
        if !config.listen.ip().is_loopback() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "the DevTools endpoint listens on loopback addresses only",
            ));
        }
        let listener = TcpListener::bind(config.listen)?;
        let addr = listener.local_addr()?;
        let generated = config.token.is_empty();
        let token = if generated {
            generate_token()?
        } else {
            config.token.clone()
        };
        let url = format!("ws://{addr}{PATH}");
        if let Some(file) = &config.port_file {
            // Written whole, then renamed: a reader never sees half a line.
            let mut tmp = file.clone().into_os_string();
            tmp.push(".tmp");
            let tmp = PathBuf::from(tmp);
            std::fs::write(&tmp, format!("{url}\n"))?;
            std::fs::rename(&tmp, file)?;
        }
        // `println!` panics on a closed stdout (the launcher died): the
        // announcement is best effort.
        {
            use io::Write as _;
            let mut out = io::stdout().lock();
            let _ = if generated {
                writeln!(out, "nsh-devtools listening on {url} token={token}")
            } else {
                writeln!(out, "nsh-devtools listening on {url}")
            };
            let _ = out.flush();
        }
        let shared = Arc::new(Shared {
            handler,
            console,
            config,
            token,
            stop: AtomicBool::new(false),
            next_conn: AtomicU64::new(1),
            conns: Mutex::new(HashMap::new()),
            input: Mutex::new(None),
            state: Mutex::new(State::default()),
            started: Condvar::new(),
        });
        let accept = {
            let shared = shared.clone();
            thread::Builder::new()
                .name("nsh-devtools-accept".into())
                .spawn(move || accept_loop(&listener, &shared))?
        };
        Ok(Endpoint {
            shared,
            addr,
            accept: Some(accept),
        })
    }

    /// The address it listens on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The token clients present (the configured one, or the generated one).
    pub fn token(&self) -> &str {
        &self.shared.token
    }

    /// Sends the event `method` to every connection.
    pub fn event(&self, method: &str, params: Value) {
        let Ok(text) = serde_json::to_string(&Event::new(method, params)) else {
            return;
        };
        for conn in lock(&self.shared.conns).values() {
            conn.send(text.clone());
        }
    }

    /// Blocks until a client sent `Session.start`; at once when the endpoint
    /// was not asked to wait ([`EndpointConfig::wait`]).
    ///
    /// # Errors
    ///
    /// [`WaitTimedOut`] when none did within [`EndpointConfig::wait_timeout`].
    pub fn wait_for_start(&self) -> Result<(), WaitTimedOut> {
        if !self.shared.config.wait {
            return Ok(());
        }
        let deadline = Instant::now() + self.shared.config.wait_timeout;
        let mut state = lock(&self.shared.state);
        while !state.ready {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(WaitTimedOut);
            }
            state = self
                .shared
                .started
                .wait_timeout(state, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        Ok(())
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        // Wakes the accept thread out of its blocking accept.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(500));
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
    }
}

fn generate_token() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>) {
    let mut failing = false;
    for stream in listener.incoming() {
        if shared.stop.load(Ordering::Acquire) {
            return;
        }
        let stream = match stream {
            Ok(stream) => stream,
            Err(err) => {
                // Out of file descriptors and the like: do not spin.
                if !failing {
                    log::warn!("DevTools: accept failed: {err}");
                    failing = true;
                }
                thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        failing = false;
        let shared = shared.clone();
        let spawned = thread::Builder::new()
            .name("nsh-devtools-conn".into())
            .spawn(move || serve(&shared, stream));
        if let Err(err) = spawned {
            log::warn!("DevTools: cannot serve a connection: {err}");
        }
    }
}

/// Whether the request's query carries `token`.
fn token_matches(query: Option<&str>, token: &str) -> bool {
    let given = query
        .unwrap_or("")
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
        .unwrap_or("");
    // Compared without stopping at the first difference.
    given.len() == token.len()
        && given
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

fn refuse(status: u16) -> ErrorResponse {
    let mut response = http::Response::new(None);
    *response.status_mut() = http::StatusCode::from_u16(status).unwrap_or_default();
    response
}

fn serve(shared: &Arc<Shared>, stream: TcpStream) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    // A client that stops reading fails its next write instead of letting
    // the queued frames grow: the connection ends and `Cleanup` closes it.
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    // The closure's signature is tungstenite's callback type.
    #[allow(clippy::result_large_err)]
    let check =
        |request: &HttpRequest, response: HttpResponse| -> Result<HttpResponse, ErrorResponse> {
            if request.uri().path() != PATH {
                return Err(refuse(404));
            }
            if !token_matches(request.uri().query(), &shared.token) {
                return Err(refuse(401));
            }
            Ok(response)
        };
    let Ok(mut ws) = tungstenite::accept_hdr(stream, check) else {
        return;
    };
    let _ = ws.get_ref().set_read_timeout(Some(POLL));
    let (out, queue) = channel();
    let conn = Arc::new(Conn {
        id: shared.next_conn.fetch_add(1, Ordering::Relaxed),
        out,
        closed: AtomicBool::new(false),
        subscribed: AtomicBool::new(false),
    });
    lock(&shared.conns).insert(conn.id, conn.clone());
    /// Cleans up when the connection ends, a panicking handler included.
    struct Cleanup<'a>(&'a Shared, &'a Conn);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            self.1.closed.store(true, Ordering::Release);
            lock(&self.0.conns).remove(&self.1.id);
            release_input(self.0, self.1.id);
        }
    }
    let _cleanup = Cleanup(shared, &conn);
    run(shared, &conn, &mut ws, &queue);
}

fn run(
    shared: &Arc<Shared>,
    conn: &Arc<Conn>,
    ws: &mut WebSocket<TcpStream>,
    queue: &Receiver<String>,
) {
    while !shared.stop.load(Ordering::Acquire) {
        while let Ok(text) = queue.try_recv() {
            if ws.send(Message::text(text)).is_err() {
                return;
            }
        }
        match ws.read() {
            Ok(Message::Text(text)) => match serde_json::from_str::<Request>(&text) {
                Ok(request) => {
                    if let Some(response) = dispatch(shared, conn, request) {
                        conn.respond(&response);
                    }
                }
                Err(err) => log::warn!("DevTools: a frame is not a request: {err}"),
            },
            Ok(Message::Close(_)) => {
                let _ = ws.flush();
                return;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(err))
                if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
    }
}

fn release_input(shared: &Shared, conn: u64) {
    let held = {
        let mut input = lock(&shared.input);
        let held = *input == Some(conn);
        if held {
            *input = None;
        }
        held
    };
    // A client that dies mid-drag leaves the button down: let go of it
    // before anyone else's input.
    if held {
        shared
            .handler
            .step(TestStep::ReleaseInput, Box::new(|_| {}));
    }
}

fn bad_params(method: &str, err: impl std::fmt::Display) -> ErrorBody {
    ErrorBody::new(ERR_BAD_PARAMS, format!("{method}: {err}"))
}

/// Answers `request`; `None` when a step will answer later.
fn dispatch(shared: &Arc<Shared>, conn: &Arc<Conn>, request: Request) -> Option<Response> {
    let Request { id, method, params } = request;
    let result = match method.as_str() {
        "Session.hello" => Ok(hello(shared)),
        "Session.setup" => session_setup(shared, params),
        "Session.start" => session_start(shared),
        "Session.release" => {
            release_input(shared, conn.id);
            Ok(json!({}))
        }
        "Console.query" => serde_json::from_value::<ConsoleQuery>(params)
            .map(|q| json!({ "entries": shared.console.query(&q) }))
            .map_err(|e| bad_params(&method, e)),
        "Console.subscribe" => {
            subscribe(shared, conn);
            Ok(json!({}))
        }
        "Network.respond" => network_respond(shared, params),
        "Protocol.describe" => Ok(description()),
        _ => return step(shared, conn, id, &method, params),
    };
    Some(match result {
        Ok(value) => Response::ok(id, value),
        Err(error) => Response::err(id, error),
    })
}

fn hello(shared: &Shared) -> Value {
    const BUILT_IN: &[&str] = &[
        "Session.hello",
        "Session.setup",
        "Session.start",
        "Session.release",
        "Console.query",
        "Console.subscribe",
        "Network.respond",
        "Protocol.describe",
    ];
    let methods: Vec<&str> = BUILT_IN
        .iter()
        .copied()
        .chain(METHODS.iter().map(|m| m.method))
        .collect();
    json!({
        "version": PROTOCOL_VERSION,
        "appId": shared.config.app_id,
        "platform": shared.config.platform,
        "stability": "experimental",
        "methods": methods,
        "capabilities": [PRIVILEGED],
    })
}

fn session_setup(shared: &Shared, params: Value) -> Result<Value, ErrorBody> {
    let setup = if params.is_null() {
        SessionSetup::default()
    } else {
        serde_json::from_value(params).map_err(|e| bad_params("Session.setup", e))?
    };
    let mut state = lock(&shared.state);
    if state.started {
        return Err(ErrorBody::new(
            ERR_BAD_PARAMS,
            "Session.setup: the session has started",
        ));
    }
    state.setup = Some(setup);
    Ok(json!({}))
}

/// Hands the setup (empty when none came) to the handler, then starts the
/// app. A second start does nothing.
fn session_start(shared: &Shared) -> Result<Value, ErrorBody> {
    let setup = {
        let mut state = lock(&shared.state);
        // Another start in flight: wait for its outcome, then claim or return.
        while state.started {
            if state.ready {
                return Ok(json!({}));
            }
            state = shared
                .started
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
        state.started = true;
        // Kept until setup succeeds, so a retry runs the same setup.
        state.setup.clone().unwrap_or_default()
    };
    /// Gives the claim back unless the start finished (failure or panic).
    struct Claim<'a>(&'a Shared, bool);
    impl Drop for Claim<'_> {
        fn drop(&mut self) {
            if !self.1 {
                lock(&self.0.state).started = false;
                self.0.started.notify_all();
            }
        }
    }
    let mut claim = Claim(shared, false);
    shared.handler.setup(setup)?;
    shared.handler.start();
    {
        let mut state = lock(&shared.state);
        state.setup = None;
        state.ready = true;
    }
    claim.1 = true;
    shared.started.notify_all();
    Ok(json!({}))
}

fn subscribe(shared: &Arc<Shared>, conn: &Arc<Conn>) {
    if conn.subscribed.swap(true, Ordering::AcqRel) {
        return;
    }
    let entries = shared.console.subscribe();
    let (shared, conn) = (shared.clone(), conn.clone());
    let spawned = thread::Builder::new()
        .name("nsh-devtools-console".into())
        .spawn(move || {
            loop {
                match entries.recv_timeout(Duration::from_millis(50)) {
                    Ok(entry) => {
                        if let Ok(text) = serde_json::to_string(&Event::new(
                            "Console.entry",
                            serde_json::to_value(entry).unwrap_or_default(),
                        )) {
                            conn.send(text);
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                if conn.closed.load(Ordering::Acquire) || shared.stop.load(Ordering::Acquire) {
                    return;
                }
            }
        });
    if let Err(err) = spawned {
        log::warn!("DevTools: cannot forward the console: {err}");
    }
}

fn network_respond(shared: &Shared, params: Value) -> Result<Value, ErrorBody> {
    #[derive(Deserialize)]
    struct Params {
        id: u64,
        reply: FetchReply,
    }
    let Params { id, reply } =
        serde_json::from_value(params).map_err(|e| bad_params("Network.respond", e))?;
    shared.handler.fetch_reply(id, reply);
    Ok(json!({}))
}

/// Takes the input for `conn` when the method acts on the app.
fn take_input(shared: &Shared, conn: u64, method: &str) -> Result<(), ErrorBody> {
    let acts = METHODS
        .iter()
        .find(|m| m.method == method)
        .is_some_and(|m| m.kind == Kind::Action);
    if !acts {
        return Ok(());
    }
    let mut input = lock(&shared.input);
    match *input {
        Some(holder) if holder != conn => Err(ErrorBody::new(
            ERR_INPUT_LOCKED,
            "another client holds the input; it sends Session.release or disconnects",
        )),
        _ => {
            *input = Some(conn);
            Ok(())
        }
    }
}

/// Runs a method of the table as its step; the answer comes from `done`.
fn step(
    shared: &Arc<Shared>,
    conn: &Arc<Conn>,
    id: u64,
    method: &str,
    params: Value,
) -> Option<Response> {
    let step = match methods::step_of(method, params) {
        Ok(step) => step,
        Err(error) => return Some(Response::err(id, error)),
    };
    if shared.config.wait && !lock(&shared.state).ready {
        return Some(Response::err(
            id,
            ErrorBody::new(ERR_NOT_STARTED, "send Session.start first"),
        ));
    }
    if let Err(error) = take_input(shared, conn.id, method) {
        return Some(Response::err(id, error));
    }
    let answer = conn.clone();
    shared.handler.step(
        step,
        Box::new(move |outcome| {
            // A connection that closed before the answer gets none.
            answer.respond(&match outcome {
                Ok(value) => Response::ok(id, value),
                Err(message) => {
                    // The agent marks a view gone from the tree with `stale: `.
                    let code = if message.starts_with("stale: ") {
                        ERR_STALE
                    } else {
                        ERR_STEP_FAILED
                    };
                    Response::err(id, ErrorBody::new(code, message))
                }
            });
        }),
    );
    None
}
