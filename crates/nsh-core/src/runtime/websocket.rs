//! `websocket`: an explicit WebSocket client (`WebSocket.connect(url)`),
//! allowed by the HTTP grants a GET of its URL needs
//! ([`crate::access::Access::allows_websocket`]). Calls check their URL,
//! options, grant and limits in the JS call (a `TypeError` or `RangeError`
//! there); the handshake is a task of the core, and each connection's
//! socket is driven by a task of its own on the engine's tokio runtime,
//! which answers pings and keeps up to eight unread messages.

use std::{cell::Cell, collections::VecDeque, rc::Rc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderName, HeaderValue},
    protocol::WebSocketConfig,
};

use super::{MAX_WEBSOCKETS, WEBSOCKETS};
use crate::{
    JsonValue,
    ui::{
        Held, JsError, JsHost, JsIn, JsOwned, JsValue, Later, PropError, System, TsTypes,
        ambient::{Spawned, spawn},
        class, module,
    },
};

/// The largest message one write sends or one read returns.
const LIMIT: usize = 8 * 1024 * 1024;
/// How long resolving, connecting with the handshake, a write and closing
/// may take.
const TIMEOUT: Duration = Duration::from_secs(30);
/// Messages received while no read waits; one more closes the connection.
const UNREAD: usize = 8;

/// WebSocket client connections to the hosts the HTTP grants allow.
#[module]
pub(crate) struct WebSocket;

#[module]
impl WebSocket {
    /// Connects to a `ws://` or `wss://` URL that capabilities.network allows
    /// a GET of (over `http` / `https`); at most 32 WebSockets per VM.
    pub(crate) fn connect(
        url: String,
        options: Option<ConnectOptions>,
        sys: System,
    ) -> Result<Later<WebSocketConnection>, JsError> {
        let parsed = url::Url::parse(&url)
            .map_err(|error| JsError::Type(format!("invalid WebSocket URL: {error}")))?;
        if !matches!(parsed.scheme(), "ws" | "wss") {
            return Err(JsError::Type(
                "WebSocket.connect(url) expects a ws:// or wss:// URL".into(),
            ));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| JsError::Type("WebSocket URL has no host".into()))?
            .to_owned();
        let port = parsed.port_or_known_default().ok_or_else(|| {
            JsError::Type("WebSocket URL has no port and no known default".into())
        })?;
        // Checked on the parsed URL, which is the one the handshake uses.
        if !sys.access.allows_websocket(parsed.as_str()) {
            return Err(JsError::Type(format!(
                "network access to `{host}` is not granted; add it to capabilities.network.hosts or a GET rule in capabilities.network.http"
            )));
        }
        let held = sys.quotas.take(WEBSOCKETS, MAX_WEBSOCKETS).ok_or_else(|| {
            JsError::Range(format!(
                "At most {MAX_WEBSOCKETS} concurrent WebSockets are allowed per VM"
            ))
        })?;
        let headers = options.map(|o| o.headers).unwrap_or_default();
        Ok(Later::new(open(parsed, host, port, headers, held)))
    }
}

/// `WebSocket.connect`'s options.
#[derive(Debug, Default)]
pub(crate) struct ConnectOptions {
    /// Extra handshake headers, checked in the call.
    headers: Vec<(HeaderName, HeaderValue)>,
}

impl JsValue for ConnectOptions {
    fn ts_type(types: &mut TsTypes) -> String {
        types.declare("WebSocketConnectOptions", |_| {
            "/** `WebSocket.connect`'s options. */\nexport interface WebSocketConnectOptions {\n  /** Extra handshake headers; credentials and handshake-control headers are refused. */\n  headers?: Record<string, string>;\n}".into()
        })
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        let expected = "an object with headers";
        let JsonValue::Object(options) = value.to_json()? else {
            return Err(value.expected(expected));
        };
        if let Some(key) = options.keys().find(|key| *key != "headers") {
            return Err(PropError(format!(
                "unknown option `{key}` for WebSocket.connect(url, options); expected headers"
            )));
        }
        let headers = match options.get("headers") {
            None | Some(JsonValue::Null) => serde_json::Map::new(),
            Some(JsonValue::Object(headers)) => headers.clone(),
            Some(_) => {
                return Err(PropError(
                    "headers: expected a plain object of string values".into(),
                ));
            }
        };
        let mut out = Vec::with_capacity(headers.len());
        for (name, value) in headers {
            let normalized = name.to_ascii_lowercase();
            if matches!(
                normalized.as_str(),
                "authorization"
                    | "proxy-authorization"
                    | "cookie"
                    | "host"
                    | "connection"
                    | "upgrade"
            ) || normalized.starts_with("sec-websocket-")
            {
                return Err(PropError(format!(
                    "headers may not set handshake-control or credential header `{name}`"
                )));
            }
            let name = HeaderName::from_bytes(normalized.as_bytes())
                .map_err(|_| PropError(format!("invalid header name `{name}`")))?;
            let JsonValue::String(value) = value else {
                return Err(PropError("headers: expected string header values".into()));
            };
            let value = HeaderValue::from_str(&value)
                .map_err(|_| PropError(format!("invalid value of header `{name}`")))?;
            out.push((name, value));
        }
        Ok(Self { headers: out })
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        let headers: serde_json::Map<String, JsonValue> = self
            .headers
            .iter()
            .map(|(name, value)| {
                (
                    name.to_string(),
                    String::from_utf8_lossy(value.as_bytes())
                        .into_owned()
                        .into(),
                )
            })
            .collect();
        JsonValue::Object(
            [("headers".to_owned(), headers.into())]
                .into_iter()
                .collect(),
        )
        .to_js(host)
    }
}

/// One message: text or bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Data {
    /// A text message.
    Text(String),
    /// A binary message.
    Bytes(Vec<u8>),
}

impl JsValue for Data {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string | Uint8Array".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if let Some(text) = value.as_string() {
            return Ok(Self::Text(text));
        }
        value
            .as_bytes()
            .map(Self::Bytes)
            .ok_or_else(|| value.expected("a string or Uint8Array"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self {
            Self::Text(text) => text.to_js(host),
            Self::Bytes(bytes) => bytes.to_js(host),
        }
    }
}

type Reply<T> = oneshot::Sender<Result<T, String>>;

enum Command {
    Read(Reply<Message>),
    Write(Message, Reply<()>),
    Close(Reply<()>),
}

/// An open WebSocket connection: one read and up to eight commands may
/// wait at once.
#[class]
pub(crate) struct WebSocketConnection {
    commands: mpsc::Sender<Command>,
    reading: Rc<Cell<bool>>,
    /// The task driving the socket; dropping it closes the connection.
    _driver: Spawned<()>,
}

#[class]
impl WebSocketConnection {
    /// Sends a text (string) or binary (Uint8Array) message of at most 8 MiB.
    pub(crate) fn write(&self, data: Data) -> Result<Later<()>, JsError> {
        let message = match data {
            Data::Text(text) => Message::Text(text.into()),
            Data::Bytes(bytes) => Message::Binary(bytes.into()),
        };
        if message.len() > LIMIT {
            return Err(JsError::Range(format!(
                "WebSocket write exceeded the {LIMIT} byte limit"
            )));
        }
        let (send, receive) = oneshot::channel();
        Ok(Later::new(enqueue(
            &self.commands,
            Command::Write(message, send),
            receive,
        )))
    }

    /// The next message: a string for text, a Uint8Array for bytes.
    pub(crate) fn read(&self) -> Result<Later<Data>, JsError> {
        if self.reading.replace(true) {
            return Err(JsError::Type(
                "WebSocket.read() already has an outstanding read".into(),
            ));
        }
        struct Reading(Rc<Cell<bool>>);
        impl Drop for Reading {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let reading = Reading(self.reading.clone());
        let (send, receive) = oneshot::channel();
        let message = enqueue(&self.commands, Command::Read(send), receive);
        Ok(Later::new(async move {
            let _reading = reading;
            Ok::<_, String>(match message.await? {
                Message::Text(text) => Data::Text(text.as_str().to_owned()),
                Message::Binary(bytes) => Data::Bytes(bytes.to_vec()),
                _ => unreachable!("the driver returns data messages only"),
            })
        }))
    }

    /// Sends a close frame and ends the connection.
    pub(crate) fn close(&self) -> Later<()> {
        let (send, receive) = oneshot::channel();
        Later::new(enqueue(&self.commands, Command::Close(send), receive))
    }
}

/// Queues `command` now (a full queue fails the call's promise) and waits
/// for its reply.
fn enqueue<T: 'static>(
    commands: &mpsc::Sender<Command>,
    command: Command,
    reply: oneshot::Receiver<Result<T, String>>,
) -> impl Future<Output = Result<T, String>> + 'static {
    let queued = commands.try_send(command).map_err(|error| match error {
        mpsc::error::TrySendError::Full(_) => {
            "WebSocket command queue is full; wait for an outstanding operation".to_owned()
        }
        mpsc::error::TrySendError::Closed(_) => "WebSocket connection is closed".to_owned(),
    });
    async move {
        queued?;
        reply
            .await
            .map_err(|_| "WebSocket connection is closed".to_owned())?
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Resolves, connects and shakes hands within [`TIMEOUT`].
async fn open(
    url: url::Url,
    host: String,
    port: u16,
    headers: Vec<(HeaderName, HeaderValue)>,
    held: Held,
) -> Result<WebSocketConnection, String> {
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|error| format!("WebSocket handshake failed: {error}"))?;
    request.headers_mut().extend(headers);
    request
        .headers_mut()
        .entry("User-Agent")
        .or_insert(HeaderValue::from_static(concat!(
            "nsh/",
            env!("CARGO_PKG_VERSION")
        )));
    let config = WebSocketConfig::default()
        .max_message_size(Some(LIMIT))
        .max_frame_size(Some(LIMIT));
    let operation = format!("WebSocket host {host}:{port}");
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    let addresses =
        tokio::time::timeout_at(deadline, tokio::net::lookup_host((host.as_str(), port)))
            .await
            .map_err(|_| format!("WebSocket connection failed: resolving {operation} timed out"))?
            .map_err(|error| {
                format!("WebSocket connection failed: resolving {operation} failed: {error}")
            })?;
    let mut connected = None;
    let mut last_error = "host resolved to no addresses".to_owned();
    for address in addresses {
        match tokio::time::timeout_at(deadline, tokio::net::TcpStream::connect(address)).await {
            Ok(Ok(stream)) => {
                connected = Some(stream);
                break;
            }
            Ok(Err(error)) => last_error = error.to_string(),
            Err(_) => {
                return Err(format!(
                    "WebSocket connection failed: connecting to {operation} timed out"
                ));
            }
        }
    }
    let stream = connected.ok_or_else(|| {
        format!("WebSocket connection failed: connecting to {operation} failed: {last_error}")
    })?;
    let (socket, _) = tokio::time::timeout_at(
        deadline,
        tokio_tungstenite::client_async_tls_with_config(request, stream, Some(config), None),
    )
    .await
    .map_err(|_| "WebSocket handshake timed out".to_owned())?
    .map_err(|error| format!("WebSocket handshake failed: {error}"))?;
    let (commands, receiver) = mpsc::channel(UNREAD);
    let driver = spawn(async move {
        let _held = held;
        drive(socket, receiver).await;
    });
    Ok(WebSocketConnection {
        commands,
        reading: Rc::default(),
        _driver: driver,
    })
}

/// Runs the connection's commands and keeps what arrives until it is read.
async fn drive(mut socket: Socket, mut commands: mpsc::Receiver<Command>) {
    let mut messages = VecDeque::new();
    let mut read: Option<Reply<Message>> = None;
    let error = loop {
        tokio::select! {
            biased;
            command = commands.recv() => match command {
                None => break "WebSocket connection is closed".to_owned(),
                Some(Command::Read(reply)) => match messages.pop_front() {
                    Some(message) => {
                        let _ = reply.send(Ok(message));
                    }
                    None => read = Some(reply),
                },
                Some(Command::Write(message, reply)) => {
                    let result = tokio::time::timeout(TIMEOUT, socket.send(message))
                        .await
                        .map_err(|_| "WebSocket write timed out".to_owned())
                        .and_then(|result| result.map_err(|error| format!("WebSocket write failed: {error}")));
                    let failure = result.as_ref().err().cloned();
                    let _ = reply.send(result);
                    if let Some(error) = failure {
                        break error;
                    }
                }
                Some(Command::Close(reply)) => {
                    let result = tokio::time::timeout(TIMEOUT, socket.close(None))
                        .await
                        .map_err(|_| "WebSocket close timed out".to_owned())
                        .and_then(|result| result.map_err(|error| format!("WebSocket close failed: {error}")));
                    let _ = reply.send(result);
                    break "WebSocket connection is closed".to_owned();
                }
            },
            incoming = socket.next() => match incoming {
                Some(Ok(message @ (Message::Text(_) | Message::Binary(_)))) => {
                    if let Some(reply) = read.take() {
                        let _ = reply.send(Ok(message));
                    } else if messages.len() < UNREAD {
                        messages.push_back(message);
                    } else {
                        let _ = tokio::time::timeout(TIMEOUT, socket.close(None)).await;
                        break "WebSocket connection is closed".to_owned();
                    }
                }
                Some(Ok(Message::Ping(_))) => {
                    if let Err(error) = socket.flush().await {
                        break format!("WebSocket pong failed: {error}");
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    let _ = socket.flush().await;
                    break "WebSocket closed by peer".to_owned();
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => break format!("WebSocket read failed: {error}"),
                None => break "WebSocket connection is closed".to_owned(),
            }
        }
    };
    if let Some(reply) = read {
        let _ = reply.send(Err(error));
    }
}
