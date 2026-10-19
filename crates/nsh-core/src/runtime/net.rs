//! `net`: TCP client sockets, only to the `host:port` pairs
//! `capabilities.network.sockets` grants. Calls check their limits in the
//! JS call (a `TypeError` or `RangeError` there); the connection and its
//! reads and writes are tasks of the core, on its tokio runtime.

use crate::ui::{JsError, Later, System, class, module};

/// The most one read returns and one write sends.
const IO_LIMIT: usize = 1024 * 1024;
/// Bytes queued for writing on one socket: native memory the heap limit
/// does not cover.
#[cfg(not(target_arch = "wasm32"))]
const PENDING_WRITE_LIMIT: usize = 4 * IO_LIMIT;
/// Sockets open or connecting at once in one VM, like WebSockets.
const MAX_SOCKETS: usize = 32;
/// The [`crate::ui::Quotas`] kind sockets count under.
const SOCKETS: &str = "net.sockets";
/// How long resolving and connecting, a read and a write may take.
#[cfg(not(target_arch = "wasm32"))]
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// TCP client sockets.
pub(crate) struct Net;

/// TCP client sockets to granted `host:port` pairs.
#[module(flat, default)]
impl Net {
    /// Connects only to `host:port` pairs granted by
    /// capabilities.network.sockets; at most 32 sockets per VM.
    pub(crate) fn connect(host: String, port: u16, sys: System) -> Result<Later<Socket>, JsError> {
        if !sys.access.allows_socket(&host, port) {
            let host = host.to_ascii_lowercase();
            let host = if host.contains(':') && !host.starts_with('[') {
                format!("[{host}]")
            } else {
                host
            };
            return Err(JsError::Type(format!(
                "a socket to `{host}:{port}` is not granted; add \"{host}:{port}\" to capabilities.network.sockets"
            )));
        }
        if cfg!(target_arch = "wasm32") {
            return Err(JsError::Type("TCP sockets require a native host".into()));
        }
        let held = sys.quotas.take(SOCKETS, MAX_SOCKETS).ok_or_else(|| {
            JsError::Range(format!(
                "At most {MAX_SOCKETS} concurrent sockets are allowed per VM"
            ))
        })?;
        Ok(Later::new(Socket::open(host, port, held)))
    }
}

/// A connected TCP socket.
#[class]
pub(crate) struct Socket {
    #[cfg(not(target_arch = "wasm32"))]
    inner: std::rc::Rc<native::Inner>,
}

#[class]
impl Socket {
    /// Reads at most 1 MiB; resolves null after the peer closes.
    pub(crate) fn read(&self, size: Option<usize>) -> Result<Later<Option<Vec<u8>>>, JsError> {
        let size = size.unwrap_or(64 * 1024);
        if size > IO_LIMIT {
            return Err(JsError::Range(format!(
                "socket read exceeded the {IO_LIMIT} byte limit"
            )));
        }
        #[cfg(not(target_arch = "wasm32"))]
        return Ok(Later::new(native::read(self.inner.clone(), size)));
        #[cfg(target_arch = "wasm32")]
        Err(JsError::Type("TCP sockets require a native host".into()))
    }

    /// Writes at most 1 MiB of UTF-8 data; at most 4 MiB may wait to be sent.
    pub(crate) fn write(&self, data: String) -> Result<Later<()>, JsError> {
        if data.len() > IO_LIMIT {
            return Err(JsError::Range(format!(
                "socket write exceeded the {IO_LIMIT} byte limit"
            )));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let queued = self.inner.pending.get() + data.len();
            if queued > PENDING_WRITE_LIMIT {
                return Err(JsError::Range(format!(
                    "socket writes waiting to be sent exceeded the {PENDING_WRITE_LIMIT} byte limit"
                )));
            }
            self.inner.pending.set(queued);
            Ok(Later::new(native::write(self.inner.clone(), data)))
        }
        #[cfg(target_arch = "wasm32")]
        Err(JsError::Type("TCP sockets require a native host".into()))
    }

    /// Interrupts pending reads and writes and closes the native socket.
    pub(crate) fn close(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.inner.close();
    }
}

impl Socket {
    #[cfg(not(target_arch = "wasm32"))]
    async fn open(host: String, port: u16, held: crate::ui::Held) -> Result<Socket, String> {
        native::connect(&host, port, held).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn open(_: String, _: u16, _: crate::ui::Held) -> Result<Socket, String> {
        Err("TCP sockets require a native host".into())
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::tcp::{OwnedReadHalf, OwnedWriteHalf},
        sync::{Mutex, Notify},
    };

    use super::{Socket, TIMEOUT};
    use crate::ui::Held;

    pub(super) struct Inner {
        read: Mutex<OwnedReadHalf>,
        write: Mutex<OwnedWriteHalf>,
        /// Shuts the connection down while a read or write holds its half.
        closer: std::net::TcpStream,
        closed: Cell<bool>,
        cancel: Notify,
        /// Bytes of writes not finished yet.
        pub(super) pending: Cell<usize>,
        /// The VM's place for this socket, given back on close.
        held: RefCell<Option<Held>>,
    }

    impl Inner {
        pub(super) fn close(&self) {
            self.closed.set(true);
            let _ = self.closer.shutdown(std::net::Shutdown::Both);
            self.cancel.notify_waiters();
            self.held.borrow_mut().take();
        }
    }

    pub(super) async fn connect(host: &str, port: u16, held: Held) -> Result<Socket, String> {
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        let operation = format!("{host}:{port}");
        let addresses = tokio::time::timeout_at(deadline, tokio::net::lookup_host((host, port)))
            .await
            .map_err(|_| format!("resolving {operation} timed out"))?
            .map_err(|error| format!("resolving {operation} failed: {error}"))?;
        let mut last_error = None;
        let mut connected = None;
        for address in addresses {
            match tokio::time::timeout_at(deadline, tokio::net::TcpStream::connect(address)).await {
                Ok(Ok(stream)) => {
                    connected = Some(stream);
                    break;
                }
                Ok(Err(error)) => last_error = Some(error),
                Err(_) => return Err(format!("connecting to {operation} timed out")),
            }
        }
        let stream = connected.ok_or_else(|| {
            format!(
                "connecting to {operation} failed: {}",
                last_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "host resolved to no addresses".into())
            )
        })?;
        let standard = stream.into_std().map_err(|e| e.to_string())?;
        let closer = standard.try_clone().map_err(|e| e.to_string())?;
        let (read, write) = tokio::net::TcpStream::from_std(standard)
            .map_err(|e| e.to_string())?
            .into_split();
        Ok(Socket {
            inner: Rc::new(Inner {
                read: Mutex::new(read),
                write: Mutex::new(write),
                closer,
                closed: Cell::new(false),
                cancel: Notify::new(),
                pending: Cell::new(0),
                held: RefCell::new(Some(held)),
            }),
        })
    }

    pub(super) async fn read(socket: Rc<Inner>, size: usize) -> Result<Option<Vec<u8>>, String> {
        let mut reader = socket.read.lock().await;
        let mut bytes = vec![0; size];
        let count = if size == 0 || socket.closed.get() {
            0
        } else {
            tokio::select! {
                biased;
                _ = socket.cancel.notified() => 0,
                result = tokio::time::timeout(TIMEOUT, reader.read(&mut bytes)) => result
                    .map_err(|_| "socket.read(size) timed out".to_owned())?
                    .map_err(|e| format!("socket read failed: {e}"))?,
            }
        };
        if count == 0 {
            return Ok(None);
        }
        bytes.truncate(count);
        Ok(Some(bytes))
    }

    pub(super) async fn write(socket: Rc<Inner>, data: String) -> Result<(), String> {
        struct Pending<'a>(&'a Inner, usize);
        impl Drop for Pending<'_> {
            fn drop(&mut self) {
                self.0.pending.set(self.0.pending.get() - self.1);
            }
        }
        let _pending = Pending(&socket, data.len());
        let closed = || "socket is closed".to_owned();
        // `close()` interrupts a write waiting for its turn or for a peer
        // that stopped reading.
        let mut writer = tokio::select! {
            biased;
            _ = socket.cancel.notified() => return Err(closed()),
            writer = socket.write.lock() => writer,
        };
        if socket.closed.get() {
            return Err(closed());
        }
        tokio::select! {
            biased;
            _ = socket.cancel.notified() => Err(closed()),
            result = tokio::time::timeout(TIMEOUT, writer.write_all(data.as_bytes())) => result
                .map_err(|_| "socket.write(data) timed out".to_owned())?
                .map_err(|e| format!("socket write failed: {e}")),
        }
    }
}
