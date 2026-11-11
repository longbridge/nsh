//! A client of the endpoint: what the CLI and tests drive an app with.
//!
//! One reader thread owns the socket; calls queue their frames to it and
//! wait for the response of their id, events go to the receivers
//! [`Client::events`] handed out.

use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{TcpStream, ToSocketAddrs},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

use serde_json::Value;
use tungstenite::{Message, client::IntoClientRequest};

use crate::protocol::{ErrorBody, Event, Request, Response};

/// The connection is gone (a call's error code).
pub const ERR_CLOSED: &str = "ERR_DEVTOOLS_CLOSED";
/// No answer came in time (a call's error code).
pub const ERR_TIMEOUT: &str = "ERR_DEVTOOLS_TIMEOUT";

/// How long the reader waits for a frame before it sends what is queued.
const POLL: Duration = Duration::from_millis(5);

/// Why a connection could not be made.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// The endpoint refused the token.
    Unauthorized,
    /// The URL is not a `ws://` URL with a host and port.
    BadUrl(String),
    /// The endpoint could not be reached or did not complete the handshake.
    Connect(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Unauthorized => f.write_str("the DevTools token was refused"),
            ClientError::BadUrl(url) => write!(f, "not a DevTools URL: {url}"),
            ClientError::Connect(why) => write!(f, "cannot connect to DevTools: {why}"),
        }
    }
}

impl std::error::Error for ClientError {}

type Pending = Arc<Mutex<HashMap<u64, Sender<Response>>>>;

/// A connection to an app's endpoint.
pub struct Client {
    out: Sender<String>,
    next_id: AtomicU64,
    pending: Pending,
    events: Arc<Mutex<Vec<Sender<Event>>>>,
    closed: Arc<AtomicBool>,
    call_timeout: Duration,
}

impl Client {
    /// Connects to `url` (`ws://127.0.0.1:<port>/devtools`) with `token`;
    /// `timeout` bounds the connection and the handshake.
    ///
    /// # Errors
    ///
    /// [`ClientError`]: a refused token, a bad URL or an unreachable endpoint.
    pub fn connect(url: &str, token: &str, timeout: Duration) -> Result<Client, ClientError> {
        let url = format!("{url}?token={token}");
        let request = url
            .as_str()
            .into_client_request()
            .map_err(|_| ClientError::BadUrl(url.clone()))?;
        let uri = request.uri();
        let host = uri.host().ok_or_else(|| ClientError::BadUrl(url.clone()))?;
        let port = uri.port_u16().unwrap_or(80);
        let addr = (host, port)
            .to_socket_addrs()
            .ok()
            .and_then(|mut a| a.next())
            .ok_or_else(|| ClientError::BadUrl(url.clone()))?;
        let stream = TcpStream::connect_timeout(&addr, timeout)
            .map_err(|e| ClientError::Connect(e.to_string()))?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        let (mut ws, _) = tungstenite::client(request, stream).map_err(|e| match e {
            tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))
                if response.status() == 401 =>
            {
                ClientError::Unauthorized
            }
            other => ClientError::Connect(other.to_string()),
        })?;
        let _ = ws.get_ref().set_read_timeout(Some(POLL));

        let (out, queue) = channel::<String>();
        let pending: Pending = Arc::default();
        let events: Arc<Mutex<Vec<Sender<Event>>>> = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));
        {
            let (pending, events, closed) = (pending.clone(), events.clone(), closed.clone());
            thread::Builder::new()
                .name("nsh-devtools-client".into())
                .spawn(move || {
                    'read: while !closed.load(Ordering::Acquire) {
                        while let Ok(text) = queue.try_recv() {
                            if ws.send(Message::text(text)).is_err() {
                                break 'read;
                            }
                        }
                        match ws.read() {
                            Ok(Message::Text(text)) => route(&text, &pending, &events),
                            Ok(Message::Close(_)) => break,
                            Ok(_) => {}
                            Err(tungstenite::Error::Io(e))
                                if matches!(
                                    e.kind(),
                                    ErrorKind::WouldBlock | ErrorKind::TimedOut
                                ) => {}
                            Err(_) => break,
                        }
                    }
                    closed.store(true, Ordering::Release);
                    // Waiting calls see their senders drop.
                    pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    let _ = ws.get_ref().shutdown(std::net::Shutdown::Both);
                })
                .map_err(|e| ClientError::Connect(e.to_string()))?;
        }
        Ok(Client {
            out,
            next_id: AtomicU64::new(1),
            pending,
            events,
            closed,
            call_timeout: Duration::from_secs(60),
        })
    }

    /// Sets how long a call waits for its answer (60 s at first).
    pub fn set_call_timeout(&mut self, timeout: Duration) {
        self.call_timeout = timeout;
    }

    /// Calls `method` and waits for its answer.
    ///
    /// # Errors
    ///
    /// The app's [`ErrorBody`], or [`ERR_CLOSED`] / [`ERR_TIMEOUT`] when the
    /// connection went away or no answer came.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, ErrorBody> {
        self.call_within(method, params, self.call_timeout)
    }

    /// Calls `method` like [`Client::call`], waiting `timeout` for the answer
    /// instead of the connection's.
    ///
    /// # Errors
    ///
    /// As [`Client::call`].
    pub fn call_within(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, ErrorBody> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);
        let closed = || ErrorBody::new(ERR_CLOSED, "the connection is closed");
        // Checked after registering, so a close in between is not missed.
        if self.closed.load(Ordering::Acquire) {
            return Err(closed());
        }
        let text = serde_json::to_string(&Request::new(id, method, params))
            .map_err(|e| ErrorBody::new(crate::protocol::ERR_BAD_PARAMS, e.to_string()))?;
        if self.out.send(text).is_err() {
            return Err(closed());
        }
        match rx.recv_timeout(timeout) {
            Ok(response) => match response.error {
                Some(error) => Err(error),
                None => Ok(response.result.unwrap_or(Value::Null)),
            },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                Err(ErrorBody::new(ERR_TIMEOUT, format!("{method} timed out")))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(closed()),
        }
    }

    /// A receiver of the events that arrive from now on.
    pub fn events(&self) -> Receiver<Event> {
        let (tx, rx) = channel();
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tx);
        rx
    }

    /// Closes the connection.
    pub fn close(self) {
        drop(self);
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
    }
}

fn route(text: &str, pending: &Pending, events: &Mutex<Vec<Sender<Event>>>) {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if value.get("id").is_some() {
        if let Ok(response) = serde_json::from_value::<Response>(value) {
            let waiting = pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&response.id);
            if let Some(tx) = waiting {
                let _ = tx.send(response);
            }
        }
    } else if let Ok(event) = serde_json::from_value::<Event>(value) {
        events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|tx| tx.send(event.clone()).is_ok());
    }
}
