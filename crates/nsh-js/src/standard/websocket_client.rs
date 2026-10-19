//! The gpui-shell websocket module contract, on NSH's async VM and policy.
use std::{cell::Cell, collections::VecDeque, rc::Rc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use rquickjs::{
    Ctx, Exception, FromJs, Object, Promise, Result, TypedArray, Value,
    function::{Func, Opt},
};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderMap, HeaderName, HeaderValue},
    protocol::WebSocketConfig,
};
const LIMIT: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
type Reply<T> = oneshot::Sender<std::result::Result<T, String>>;
enum Command {
    Read(Reply<Message>),
    Write(Message, Reply<()>),
    Close(Reply<()>),
}
#[derive(Default)]
struct Options {
    headers: HeaderMap,
}
impl<'js> FromJs<'js> for Options {
    fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        if value.is_null() || value.is_undefined() {
            return Ok(Self::default());
        }
        let object = value.into_object().ok_or_else(|| {
            Exception::throw_type(
                ctx,
                "WebSocket.connect(url, options) expects an object with headers",
            )
        })?;
        for key in object.keys::<String>() {
            let key = key?;
            if key != "headers" {
                return Err(Exception::throw_type(
                    ctx,
                    &format!(
                        "unknown option `{key}` for WebSocket.connect(url, options); expected headers"
                    ),
                ));
            }
        }
        let value: Value = object.get("headers")?;
        if value.is_null() || value.is_undefined() {
            return Ok(Self::default());
        }
        let object = value.into_object().ok_or_else(|| {
            Exception::throw_type(
                ctx,
                "WebSocket.connect(url, options).headers expects a plain object of string values",
            )
        })?;
        let mut headers = HeaderMap::new();
        for entry in object.props::<String, Value>() {
            let (name, value) = entry?;
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
                return Err(Exception::throw_type(
                    ctx,
                    &format!(
                        "WebSocket.connect(url, options).headers may not set handshake-control or credential header `{name}`"
                    ),
                ));
            }
            let name = HeaderName::from_bytes(normalized.as_bytes()).map_err(|_| {
                Exception::throw_type(ctx, "WebSocket.connect received an invalid header name")
            })?;
            let value = String::from_js(ctx, value).map_err(|_| {
                Exception::throw_type(
                    ctx,
                    "WebSocket.connect(url, options).headers expects string header values",
                )
            })?;
            let value = HeaderValue::from_str(&value).map_err(|_| {
                Exception::throw_type(ctx, "WebSocket.connect received an invalid header value")
            })?;
            headers.append(name, value);
        }
        Ok(Self { headers })
    }
}

pub(super) fn export<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
    let object = Object::new(ctx.clone())?;
    object.set("connect", Func::from(connect))?;
    Ok(object)
}
fn connect<'js>(ctx: Ctx<'js>, url: String, options: Opt<Options>) -> Result<Promise<'js>> {
    let parsed = url::Url::parse(&url)
        .map_err(|error| Exception::throw_type(&ctx, &format!("invalid WebSocket URL: {error}")))?;
    if !matches!(parsed.scheme(), "ws" | "wss") {
        return Err(Exception::throw_type(
            &ctx,
            "WebSocket.connect(url) expects a ws:// or wss:// URL",
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| Exception::throw_type(&ctx, "WebSocket URL has no host"))?;
    let port = parsed.port_or_known_default().ok_or_else(|| {
        Exception::throw_type(&ctx, "WebSocket URL has no port and no known default")
    })?;
    if !crate::dom::shared(ctx.as_raw().as_ptr())
        .system_access
        .borrow()
        .allows_socket(host, port)
    {
        return Err(Exception::throw_type(
            &ctx,
            &format!(
                "network access to `{host}` is not granted; add it to capabilities.network.hosts"
            ),
        ));
    }
    let host = host.to_owned();
    let headers = options.0.unwrap_or_default().headers;
    let task_ctx = ctx.clone();
    Promise::wrap_future(&ctx, async move {
        let mut request = url.into_client_request().map_err(|error| {
            Exception::throw_message(&task_ctx, &format!("WebSocket handshake failed: {error}"))
        })?;
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
                .map_err(|_| {
                    Exception::throw_message(
                        &task_ctx,
                        &format!("WebSocket connection failed: resolving {operation} timed out"),
                    )
                })?
                .map_err(|error| {
                    Exception::throw_message(
                        &task_ctx,
                        &format!(
                            "WebSocket connection failed: resolving {operation} failed: {error}"
                        ),
                    )
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
                    return Err(Exception::throw_message(
                        &task_ctx,
                        &format!(
                            "WebSocket connection failed: connecting to {operation} timed out"
                        ),
                    ));
                }
            }
        }
        let stream = connected.ok_or_else(|| {
            Exception::throw_message(
                &task_ctx,
                &format!(
                    "WebSocket connection failed: connecting to {operation} failed: {last_error}"
                ),
            )
        })?;
        let (socket, _) = tokio::time::timeout_at(
            deadline,
            tokio_tungstenite::client_async_tls_with_config(request, stream, Some(config), None),
        )
        .await
        .map_err(|_| Exception::throw_message(&task_ctx, "WebSocket handshake timed out"))?
        .map_err(|error| {
            Exception::throw_message(&task_ctx, &format!("WebSocket handshake failed: {error}"))
        })?;
        let (tx, rx) = mpsc::channel(8);
        task_ctx.spawn(actor(socket, rx));
        socket_object(&task_ctx, tx)
    })
}

fn enqueue<T>(
    tx: mpsc::Sender<Command>,
    command: Command,
    reply: oneshot::Receiver<std::result::Result<T, String>>,
) -> impl std::future::Future<Output = std::result::Result<T, String>> {
    let result = tx.try_send(command).map_err(|error| match error {
        mpsc::error::TrySendError::Full(_) => {
            "WebSocket command queue is full; wait for an outstanding operation".to_owned()
        }
        mpsc::error::TrySendError::Closed(_) => "WebSocket connection is closed".to_owned(),
    });
    async move {
        result?;
        reply
            .await
            .map_err(|_| "WebSocket connection is closed".to_owned())?
    }
}
fn socket_object<'js>(ctx: &Ctx<'js>, tx: mpsc::Sender<Command>) -> Result<Object<'js>> {
    let object = Object::new(ctx.clone())?;
    let writer = tx.clone();
    object.set(
        "write",
        Func::from(
            move |ctx: Ctx<'js>, value: Value<'js>| -> Result<Promise<'js>> {
                let message = if value.is_string() {
                    let text = String::from_js(&ctx, value)?;
                    if text.len() > LIMIT {
                        return Err(Exception::throw_range(
                            &ctx,
                            &format!("WebSocket write exceeded the {LIMIT} byte limit"),
                        ));
                    }
                    Message::Text(text.into())
                } else {
                    let bytes = TypedArray::<u8>::from_js(&ctx, value).map_err(|_| {
                        Exception::throw_type(
                            &ctx,
                            "WebSocket.write(data) expects a string or Uint8Array",
                        )
                    })?;
                    // SAFETY: no JS executes while copying this live typed array.
                    let bytes = unsafe { bytes.as_bytes() }.ok_or_else(|| {
                        Exception::throw_type(
                            &ctx,
                            "WebSocket.write(data) received a detached Uint8Array",
                        )
                    })?;
                    if bytes.len() > LIMIT {
                        return Err(Exception::throw_range(
                            &ctx,
                            &format!("WebSocket write exceeded the {LIMIT} byte limit"),
                        ));
                    }
                    Message::Binary(bytes.to_vec().into())
                };
                let (send, receive) = oneshot::channel();
                let operation = enqueue(writer.clone(), Command::Write(message, send), receive);
                let task_ctx = ctx.clone();
                Promise::wrap_future(&ctx, async move {
                    operation
                        .await
                        .map_err(|error| Exception::throw_message(&task_ctx, &error))
                })
            },
        ),
    )?;
    let reader = tx.clone();
    let outstanding = Rc::new(Cell::new(false));
    object.set(
        "read",
        Func::from(move |ctx: Ctx<'js>| -> Result<Promise<'js>> {
            if outstanding.replace(true) {
                return Err(Exception::throw_type(
                    &ctx,
                    "WebSocket.read() already has an outstanding read",
                ));
            }
            struct ReadGuard(Rc<Cell<bool>>);
            impl Drop for ReadGuard {
                fn drop(&mut self) {
                    self.0.set(false);
                }
            }
            let guard = ReadGuard(outstanding.clone());
            let (send, receive) = oneshot::channel();
            let operation = enqueue(reader.clone(), Command::Read(send), receive);
            let task_ctx = ctx.clone();
            Promise::wrap_future(&ctx, async move {
                let _guard = guard;
                let message = operation
                    .await
                    .map_err(|error| Exception::throw_message(&task_ctx, &error))?;
                match message {
                    Message::Text(text) => Ok::<_, rquickjs::Error>(
                        rquickjs::String::from_str(task_ctx, &text)?.into_value(),
                    ),
                    Message::Binary(bytes) => {
                        Ok(TypedArray::<u8>::new(task_ctx, bytes.to_vec())?.into_value())
                    }
                    _ => unreachable!("actor returns data messages only"),
                }
            })
        }),
    )?;
    object.set(
        "close",
        Func::from(move |ctx: Ctx<'js>| -> Result<Promise<'js>> {
            let (send, receive) = oneshot::channel();
            let operation = enqueue(tx.clone(), Command::Close(send), receive);
            let task_ctx = ctx.clone();
            Promise::wrap_future(&ctx, async move {
                operation
                    .await
                    .map_err(|error| Exception::throw_message(&task_ctx, &error))
            })
        }),
    )?;
    Ok(object)
}
async fn actor(
    mut socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut commands: mpsc::Receiver<Command>,
) {
    let mut messages = VecDeque::new();
    let mut read: Option<Reply<Message>> = None;
    let error = loop {
        tokio::select! {
            biased;
            command = commands.recv() => match command {
                None => break "WebSocket connection is closed".to_owned(),
                Some(Command::Read(reply)) => if let Some(message) = messages.pop_front() { let _ = reply.send(Ok(message)); } else { read = Some(reply); },
                Some(Command::Write(message, reply)) => {
                    let result = tokio::time::timeout(TIMEOUT, socket.send(message)).await.map_err(|_| "WebSocket write timed out".to_owned()).and_then(|result| result.map_err(|error| format!("WebSocket write failed: {error}")));
                    let failure = result.as_ref().err().cloned(); let _ = reply.send(result);
                    if let Some(error) = failure { break error; }
                },
                Some(Command::Close(reply)) => {
                    let result = tokio::time::timeout(TIMEOUT, socket.close(None)).await.map_err(|_| "WebSocket close timed out".to_owned()).and_then(|result| result.map_err(|error| format!("WebSocket close failed: {error}")));
                    let _ = reply.send(result); break "WebSocket connection is closed".to_owned();
                },
            },
            incoming = socket.next() => match incoming {
                Some(Ok(message @ (Message::Text(_) | Message::Binary(_)))) => {
                    if let Some(reply) = read.take() { let _ = reply.send(Ok(message)); }
                    else if messages.len() < 8 { messages.push_back(message); }
                    else { let _ = tokio::time::timeout(TIMEOUT, socket.close(None)).await; break "WebSocket connection is closed".to_owned(); }
                },
                Some(Ok(Message::Ping(_))) => if let Err(error) = socket.flush().await { break format!("WebSocket pong failed: {error}"); },
                Some(Ok(Message::Close(_))) => { let _ = socket.flush().await; break "WebSocket closed by peer".to_owned(); },
                Some(Ok(_)) => {},
                Some(Err(error)) => break format!("WebSocket read failed: {error}"),
                None => break "WebSocket connection is closed".to_owned(),
            }
        }
    };
    if let Some(reply) = read {
        let _ = reply.send(Err(error));
    }
}
