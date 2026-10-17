use std::{
    io,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use tokio::sync::broadcast;

use crate::protocol::{
    BundleFormat, ClientMessage, HostPlatform, LogLevel, PROTOCOL_VERSION, ServerMessage, WS_PATH,
};

/// What a dev host did, for the server's owner to show ([`DevServer::start`]).
#[derive(Debug, Clone)]
pub enum ClientEvent {
    /// A host connected.
    Connected {
        /// Device name from the client's hello.
        device: String,
        /// The build it was sent first: the latest bundle (`None`: none
        /// yet, or the latest build failed).
        build: Option<u64>,
    },
    /// A host cannot run the app: the latest bundle imports UI sets it
    /// does not serve (it was built for another app). It gets no bundle
    /// until one needs only what it has, and shows why. Reported once per
    /// change of what is missing.
    CannotRun {
        /// Device name from the client's hello.
        device: String,
        /// The UI sets the app imports that the host does not serve.
        missing: Vec<String>,
        /// The command that installs the app's host on the host's
        /// platform ([`DevServer::install_commands`]), if there is one.
        install: Option<String>,
    },
    /// A host's connection ended.
    Disconnected {
        /// Device name from the client's hello.
        device: String,
    },
    /// A log line from a host.
    Log(ClientLog),
}

/// A log line received from a dev host.
#[derive(Debug, Clone)]
pub struct ClientLog {
    /// Device name from the client's hello.
    pub device: String,
    /// Severity.
    pub level: LogLevel,
    /// Source (`js` or a Rust module path).
    pub target: String,
    /// The message.
    pub message: String,
}

struct Bundle {
    version: u64,
    bytecode: axum::body::Bytes,
    source: axum::body::Bytes,
    source_map: Option<axum::body::Bytes>,
    capabilities: Option<String>,
    /// The UI sets it imports.
    ui_sets: Arc<[String]>,
}

struct Patch {
    version: u64,
    base: u64,
    code: axum::body::Bytes,
    source_map: Option<axum::body::Bytes>,
    /// The UI sets the bundle of `version` imports.
    ui_sets: Arc<[String]>,
}

#[derive(Clone)]
enum Event {
    Building,
    Failed(Arc<str>),
    Patch(Arc<Patch>),
    Bundle(Arc<Bundle>),
}

struct Shared {
    /// Directory served at `/assets/` (the app's source directory).
    assets_root: Option<std::path::PathBuf>,
    /// Files served at `/assets/<path>` from elsewhere (the files the UI
    /// sets' crates ship), by path.
    files: Mutex<std::collections::HashMap<String, std::path::PathBuf>>,
    /// The web host's files ([`DevServer::serve_web`]), by path (`""`:
    /// the page).
    web: Mutex<std::collections::HashMap<String, axum::body::Bytes>>,
    /// The commands that install the app's host, by platform.
    install: Mutex<std::collections::HashMap<HostPlatform, String>>,
    /// The state a newly connected client must see first.
    latest: Mutex<Option<Event>>,
    events: broadcast::Sender<Event>,
    bytecode_version: String,
    /// This run of the server ([`ServerMessage::Hello`]).
    session: u64,
    /// Hosts connected now.
    clients: AtomicUsize,
    on_event: Box<dyn Fn(ClientEvent) + Send + Sync>,
}

/// An id for this run of the server, different from earlier runs' (the
/// clock and the process).
fn new_session() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    nanos ^ (u64::from(std::process::id()) << 40)
}

/// The dev server: serves the latest bundle to dev hosts and pushes
/// rebuilds to them. Dropping it stops the server.
pub struct DevServer {
    shared: Arc<Shared>,
    addr: SocketAddr,
    next_version: Mutex<u64>,
    _runtime: tokio::runtime::Runtime,
}

impl DevServer {
    /// Starts listening on `addr`. `bytecode_version` is the format of the
    /// bytecode this server publishes; clients reporting another version get
    /// JS source instead. `on_event` receives what hosts do (connecting,
    /// disconnecting, their log lines), on the server's threads.
    pub fn start(
        addr: SocketAddr,
        bytecode_version: impl Into<String>,
        assets_root: Option<std::path::PathBuf>,
        on_event: impl Fn(ClientEvent) + Send + Sync + 'static,
    ) -> io::Result<DevServer> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("nsh-dev-server")
            .enable_all()
            .build()?;
        let shared = Arc::new(Shared {
            assets_root: assets_root.and_then(|p| dunce::canonicalize(p).ok()),
            files: Mutex::default(),
            web: Mutex::default(),
            install: Mutex::default(),
            latest: Mutex::new(None),
            events: broadcast::channel(16).0,
            bytecode_version: bytecode_version.into(),
            session: new_session(),
            clients: AtomicUsize::new(0),
            on_event: Box::new(on_event),
        });
        let listener = runtime.block_on(tokio::net::TcpListener::bind(addr))?;
        let addr = listener.local_addr()?;
        let app = Router::new()
            .route(WS_PATH, get(upgrade))
            .route("/bundle.js", get(bundle_source))
            .route("/assets/{*path}", get(asset))
            .fallback(web_file)
            .with_state(shared.clone());
        runtime.spawn(async move {
            if let Err(err) = axum::serve(listener, app).await {
                log::error!("dev server stopped: {err}");
            }
        });
        Ok(DevServer {
            shared,
            addr,
            next_version: Mutex::new(0),
            _runtime: runtime,
        })
    }

    /// Serves each `(path, file)` at `/assets/<path>` (`asset:/<path>` on
    /// device), besides the assets root: files that live elsewhere, as the
    /// crates of Rust components ship them.
    pub fn serve_files(&self, files: impl IntoIterator<Item = (String, std::path::PathBuf)>) {
        self.shared.files.lock().expect("files lock").extend(files);
    }

    /// Serves the web host: `page` (HTML) at `/` and each `(path,
    /// bytes)` at `/<path>` (the runtime's `nsh.js` and `nsh.wasm`). The
    /// page connects to [`WS_PATH`] as the other hosts do, saying it runs
    /// JS source ([`ClientMessage::Hello`]).
    pub fn serve_web(&self, page: String, files: impl IntoIterator<Item = (String, Vec<u8>)>) {
        let mut web = self.shared.web.lock().expect("web lock");
        web.insert(String::new(), page.into());
        web.extend(files.into_iter().map(|(path, bytes)| (path, bytes.into())));
    }

    /// Whether [`DevServer::serve_web`] was called: browsers can open the
    /// app at `http://<address>/`.
    pub fn serves_web(&self) -> bool {
        !self.shared.web.lock().expect("web lock").is_empty()
    }

    /// Sets the command that installs the app's host, for each platform
    /// there is one for: hosts that cannot run the app show it
    /// ([`ClientEvent::CannotRun`]).
    pub fn install_commands(&self, commands: impl IntoIterator<Item = (HostPlatform, String)>) {
        *self.shared.install.lock().expect("install lock") = commands.into_iter().collect();
    }

    /// The bound address (useful with port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The number of hosts connected now.
    pub fn clients(&self) -> usize {
        self.shared.clients.load(Ordering::Relaxed)
    }

    /// Announces that a rebuild started.
    pub fn building(&self) {
        let _ = self.shared.events.send(Event::Building);
    }

    /// Publishes a new bundle (and its source map) to every client that
    /// serves the `ui_sets` it imports (by specifier); the others are told
    /// they cannot run it ([`ClientEvent::CannotRun`]). With a `patch` (a
    /// hot update script from the previous bundle, and its source map),
    /// clients running the previous bundle update in place.
    pub fn publish(
        &self,
        bytecode: Vec<u8>,
        source: String,
        source_map: Option<String>,
        capabilities: Option<String>,
        patch: Option<(String, Option<String>)>,
        ui_sets: Vec<String>,
    ) {
        let ui_sets: Arc<[String]> = ui_sets.into();
        let version = {
            let mut next = self.next_version.lock().expect("version lock");
            *next += 1;
            *next
        };
        if let Some((code, map)) = patch
            && version > 1
        {
            // Only for clients connected now: new clients get the bundle.
            let _ = self.shared.events.send(Event::Patch(Arc::new(Patch {
                version,
                base: version - 1,
                code: code.into(),
                source_map: map.map(Into::into),
                ui_sets: ui_sets.clone(),
            })));
        }
        self.set(Event::Bundle(Arc::new(Bundle {
            version,
            bytecode: bytecode.into(),
            source: source.into(),
            source_map: source_map.map(Into::into),
            capabilities,
            ui_sets,
        })));
    }

    /// Publishes a build error; clients keep running the last good bundle
    /// and show the error.
    pub fn publish_error(&self, message: impl Into<Arc<str>>) {
        self.set(Event::Failed(message.into()));
    }

    fn set(&self, event: Event) {
        *self.shared.latest.lock().expect("latest lock") = Some(event.clone());
        let _ = self.shared.events.send(event);
    }
}

async fn bundle_source(State(shared): State<Arc<Shared>>) -> Response {
    let latest = shared.latest.lock().expect("latest lock").clone();
    match latest {
        Some(Event::Bundle(bundle)) => (
            [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
            bundle.source.clone(),
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Serves an app asset (`asset:/<path>` on device). Only the files
/// [`DevServer::serve_files`] named and files inside the assets root are
/// reachable.
async fn asset(
    State(shared): State<Arc<Shared>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    let named = shared.files.lock().expect("files lock").get(&path).cloned();
    let file = match named {
        Some(file) => file,
        None => {
            let Some(root) = &shared.assets_root else {
                return StatusCode::NOT_FOUND.into_response();
            };
            let Ok(file) = dunce::canonicalize(root.join(&path)) else {
                return StatusCode::NOT_FOUND.into_response();
            };
            if !file.starts_with(root) {
                return StatusCode::FORBIDDEN.into_response();
            }
            file
        }
    };
    match tokio::fs::read(&file).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, content_type(&file))], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The web host's page and runtime ([`DevServer::serve_web`]); never
/// cached, as they change with the runtime.
async fn web_file(State(shared): State<Arc<Shared>>, uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path == "index.html" { "" } else { path };
    let bytes = shared.web.lock().expect("web lock").get(path).cloned();
    match bytes {
        Some(bytes) => {
            let content_type = match path {
                "" => "text/html; charset=utf-8",
                path => content_type(std::path::Path::new(path)),
            };
            (
                [
                    (header::CONTENT_TYPE, content_type),
                    (header::CACHE_CONTROL, "no-cache"),
                ],
                bytes,
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The media type of a file, by its extension: what browsers need to
/// load scripts, WebAssembly (compiled while it streams only as
/// `application/wasm`), images, fonts and media.
pub(crate) fn content_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("txt") => "text/plain; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("heic") => "image/heic",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("mp4" | "m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("wav") => "audio/wav",
        _ => "application/octet-stream",
    }
}

async fn upgrade(ws: WebSocketUpgrade, State(shared): State<Arc<Shared>>) -> Response {
    // Bundles are ~100 KB of bytecode; allow generous frames.
    ws.max_message_size(64 << 20)
        .on_upgrade(move |socket| async move {
            if let Err(err) = serve_client(socket, shared).await {
                log::debug!("dev client disconnected: {err}");
            }
        })
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn serve_client(mut socket: WebSocket, shared: Arc<Shared>) -> Result<(), BoxError> {
    let hello = tokio::time::timeout(Duration::from_secs(5), socket.recv())
        .await
        .map_err(|_| "no hello")?
        .ok_or("closed before hello")??;
    let Message::Text(text) = hello else {
        return Err("expected hello".into());
    };
    let ClientMessage::Hello {
        protocol,
        bytecode_version,
        device,
        platform,
        ui_sets,
    } = serde_json::from_str(&text)?
    else {
        return Err("expected hello".into());
    };
    if protocol != PROTOCOL_VERSION {
        return Err(
            format!("{device}: protocol {protocol}, server speaks {PROTOCOL_VERSION}").into(),
        );
    }
    let format = if bytecode_version == shared.bytecode_version {
        BundleFormat::Bytecode
    } else if bytecode_version.is_empty() {
        BundleFormat::Source
    } else {
        log::warn!(
            "{device}: bytecode {bytecode_version} != {}, sending JS source",
            shared.bytecode_version
        );
        BundleFormat::Source
    };
    send(
        &mut socket,
        &ServerMessage::Hello {
            protocol: PROTOCOL_VERSION,
            session: shared.session,
        },
    )
    .await?;

    let mut host = Host {
        device,
        platform,
        ui_sets,
        format,
        refused: None,
    };

    // Subscribe before reading the latest state so no event is missed.
    let mut events = shared.events.subscribe();
    let latest = shared.latest.lock().expect("latest lock").clone();
    let build = match &latest {
        Some(Event::Bundle(bundle)) if host.missing(&bundle.ui_sets).is_empty() => {
            Some(bundle.version)
        }
        _ => None,
    };
    let device = host.device.clone();
    let _connection = Connection::new(&shared, &device, build);
    if let Some(event) = latest {
        send_event(&mut socket, &shared, &mut host, &event).await?;
    }
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(event) => send_event(&mut socket, &shared, &mut host, &event).await?,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let latest = shared.latest.lock().expect("latest lock").clone();
                    if let Some(event) = latest {
                        send_event(&mut socket, &shared, &mut host, &event).await?;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(ClientMessage::Log { level, target, message }) = serde_json::from_str(&text) {
                        (shared.on_event)(ClientEvent::Log(ClientLog { device: device.clone(), level, target, message }));
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(err)) => return Err(err.into()),
                None => return Ok(()),
            },
        }
    }
}

/// What the server knows of a connected host.
struct Host {
    device: String,
    platform: Option<HostPlatform>,
    /// The UI sets it serves (`None`: it does not say).
    ui_sets: Option<Vec<String>>,
    /// The format of the bundles it gets.
    format: BundleFormat,
    /// What it was last told it lacks, while it gets no bundles.
    refused: Option<Vec<String>>,
}

impl Host {
    /// The UI sets of `needed` the host does not serve.
    fn missing(&self, needed: &[String]) -> Vec<String> {
        match &self.ui_sets {
            Some(has) => needed
                .iter()
                .filter(|s| !has.contains(s))
                .cloned()
                .collect(),
            None => Vec::new(),
        }
    }
}

/// What a host that cannot run the app shows (as prose: hosts wrap it to
/// their screen).
fn cannot_run_message(missing: &[String], install: Option<&str>) -> String {
    let sets = match missing {
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        [] => String::new(),
    };
    let mut prose = format!(
        "The app imports {sets}, which this host does not have: it was built for another app."
    );
    if install.is_some() {
        prose.push_str(" To install this app's host, run");
    }
    let mut message = prose;
    if let Some(install) = install {
        message.push_str(&format!("\n\n  {install}"));
    }
    message
}

/// A host connected after its hello: counted and reported until its
/// session ends, however it ends.
struct Connection<'a> {
    shared: &'a Shared,
    device: &'a str,
}

impl<'a> Connection<'a> {
    fn new(shared: &'a Shared, device: &'a str, build: Option<u64>) -> Self {
        shared.clients.fetch_add(1, Ordering::Relaxed);
        (shared.on_event)(ClientEvent::Connected {
            device: device.to_owned(),
            build,
        });
        Connection { shared, device }
    }
}

impl Drop for Connection<'_> {
    fn drop(&mut self) {
        self.shared.clients.fetch_sub(1, Ordering::Relaxed);
        (self.shared.on_event)(ClientEvent::Disconnected {
            device: self.device.to_owned(),
        });
    }
}

async fn send(socket: &mut WebSocket, msg: &ServerMessage) -> Result<(), BoxError> {
    socket
        .send(Message::Text(serde_json::to_string(msg)?.into()))
        .await?;
    Ok(())
}

async fn send_event(
    socket: &mut WebSocket,
    shared: &Shared,
    host: &mut Host,
    event: &Event,
) -> Result<(), BoxError> {
    match event {
        Event::Building => send(socket, &ServerMessage::Building).await,
        // It cannot run the last good build either: it keeps showing why.
        Event::Failed(_) if host.refused.is_some() => Ok(()),
        Event::Failed(message) => {
            send(
                socket,
                &ServerMessage::BuildFailed {
                    message: message.to_string(),
                },
            )
            .await
        }
        // The bundle that follows says why.
        Event::Patch(patch) if !host.missing(&patch.ui_sets).is_empty() => Ok(()),
        Event::Patch(patch) => {
            send(
                socket,
                &ServerMessage::Patch {
                    version: patch.version,
                    base: patch.base,
                    source_map: patch.source_map.is_some(),
                },
            )
            .await?;
            socket.send(Message::Binary(patch.code.clone())).await?;
            if let Some(map) = &patch.source_map {
                socket.send(Message::Binary(map.clone())).await?;
            }
            Ok(())
        }
        Event::Bundle(bundle) => {
            let missing = host.missing(&bundle.ui_sets);
            if !missing.is_empty() {
                if host.refused.as_ref() == Some(&missing) {
                    return Ok(());
                }
                let install = host.platform.and_then(|p| {
                    shared
                        .install
                        .lock()
                        .expect("install lock")
                        .get(&p)
                        .cloned()
                });
                let message = cannot_run_message(&missing, install.as_deref());
                send(
                    socket,
                    &ServerMessage::CannotRun {
                        missing: missing.clone(),
                        message,
                    },
                )
                .await?;
                (shared.on_event)(ClientEvent::CannotRun {
                    device: host.device.clone(),
                    missing: missing.clone(),
                    install,
                });
                host.refused = Some(missing);
                return Ok(());
            }
            host.refused = None;
            let format = host.format;
            send(
                socket,
                &ServerMessage::Bundle {
                    version: bundle.version,
                    format,
                    source_map: bundle.source_map.is_some(),
                    capabilities: bundle.capabilities.clone(),
                },
            )
            .await?;
            let payload = match format {
                BundleFormat::Bytecode => bundle.bytecode.clone(),
                BundleFormat::Source => bundle.source.clone(),
            };
            socket.send(Message::Binary(payload)).await?;
            if let Some(map) = &bundle.source_map {
                socket.send(Message::Binary(map.clone())).await?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_host_that_cannot_run_the_app_is_told_why() {
        let message = super::cannot_run_message(
            &["gallery/native".to_owned()],
            Some("nsh run examples/gallery-rs --target android"),
        );
        assert_eq!(
            message,
            "The app imports gallery/native, which this host does not have: it was built for \
             another app. To install this app's host, run\n\n  nsh run examples/gallery-rs \
             --target android"
        );
        let two = super::cannot_run_message(&["a/x".to_owned(), "b/y".to_owned()], None);
        assert!(
            two.starts_with("The app imports a/x and b/y, which"),
            "{two}"
        );
        assert!(!two.contains("run"), "{two}");
    }
}
