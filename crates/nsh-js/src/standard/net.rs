//! Checked, bounded TCP client sockets. This is NSH's promise-based socket API.
use std::{rc::Rc, time::Duration};

use rquickjs::{
    Ctx, Exception, Object, Promise, Result, TypedArray, Value,
    function::{Func, Opt},
    module::{Declarations, Exports, ModuleDef},
};

const IO_LIMIT: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) struct NetModule;
impl ModuleDef for NetModule {
    fn declare(d: &Declarations) -> Result<()> {
        d.declare("default")?;
        d.declare("connect")?;
        Ok(())
    }
    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
        let net = Object::new(ctx.clone())?;
        net.set("connect", Func::from(connect))?;
        exports.export("connect", net.get::<_, Value>("connect")?)?;
        exports.export("default", net)?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct Socket {
    read: tokio::sync::Mutex<tokio::net::tcp::OwnedReadHalf>,
    write: tokio::sync::Mutex<tokio::net::tcp::OwnedWriteHalf>,
    closer: std::net::TcpStream,
    closed: std::cell::Cell<bool>,
    cancel: tokio::sync::Notify,
}
#[cfg(not(target_arch = "wasm32"))]
impl Socket {
    fn close(&self) {
        self.closed.set(true);
        let _ = self.closer.shutdown(std::net::Shutdown::Both);
        self.cancel.notify_waiters();
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn connect_tcp(host: &str, port: u16) -> std::result::Result<tokio::net::TcpStream, String> {
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    let operation = format!("{host}:{port}");
    let addresses = tokio::time::timeout_at(deadline, tokio::net::lookup_host((host, port)))
        .await
        .map_err(|_| format!("resolving {operation} timed out"))?
        .map_err(|error| format!("resolving {operation} failed: {error}"))?;
    let mut last_error = None;
    for address in addresses {
        match tokio::time::timeout_at(deadline, tokio::net::TcpStream::connect(address)).await {
            Ok(Ok(stream)) => return Ok(stream),
            Ok(Err(error)) => last_error = Some(error),
            Err(_) => return Err(format!("connecting to {operation} timed out")),
        }
    }
    Err(format!(
        "connecting to {operation} failed: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "host resolved to no addresses".into())
    ))
}

fn connect<'js>(ctx: Ctx<'js>, host: String, port: u16) -> Result<Promise<'js>> {
    if !crate::dom::shared(ctx.as_raw().as_ptr())
        .system_access
        .borrow()
        .allows_socket(&host, port)
    {
        return Err(Exception::throw_type(
            &ctx,
            &format!(
                "network access to `{}` is not granted; add it to capabilities.network.hosts",
                host.to_ascii_lowercase()
            ),
        ));
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err(Exception::throw_type(
            &ctx,
            "TCP sockets require a native host",
        ))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let owned = ctx.clone();
        Promise::wrap_future(&ctx, async move {
            let ctx = owned;
            let stream = connect_tcp(&host, port)
                .await
                .map_err(|error| Exception::throw_message(&ctx, &error))?;
            let standard = stream
                .into_std()
                .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
            let closer = standard
                .try_clone()
                .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
            let stream = tokio::net::TcpStream::from_std(standard)
                .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
            let (read, write) = stream.into_split();
            let socket = Rc::new(Socket {
                closer,
                read: tokio::sync::Mutex::new(read),
                write: tokio::sync::Mutex::new(write),
                closed: std::cell::Cell::new(false),
                cancel: tokio::sync::Notify::new(),
            });
            let object = Object::new(ctx.clone())?;
            let reader = socket.clone();
            object.set("read", Func::from(move |ctx: Ctx<'js>, size: Opt<usize>| -> Result<Promise<'js>> {
            let size = size.0.unwrap_or(64 * 1024);
            if size > IO_LIMIT { return Err(Exception::throw_range(&ctx, "socket read exceeded the 1048576 byte limit")); }
            let socket = reader.clone();
            let owned = ctx.clone();
            Promise::wrap_future(&ctx, async move {
                use tokio::io::AsyncReadExt;
                let ctx = owned;
                let mut reader = socket.read.lock().await;
                let mut bytes = vec![0; size];
                let count = if size == 0 || socket.closed.get() { 0 } else {
                    tokio::select! {
                        biased;
                        _ = socket.cancel.notified() => 0,
                        result = tokio::time::timeout(TIMEOUT, reader.read(&mut bytes)) => result
                            .map_err(|_| Exception::throw_message(&ctx, "socket.read(size) timed out"))?
                            .map_err(|e| Exception::throw_message(&ctx, &format!("socket read failed: {e}")))?,
                    }
                };
                if count == 0 { return Ok::<_, rquickjs::Error>(Value::new_null(ctx)); }
                bytes.truncate(count);
                Ok(TypedArray::<u8>::new(ctx, bytes)?.into_value())
            })
        }))?;
            let writer = socket.clone();
            object.set(
                "write",
                Func::from(move |ctx: Ctx<'js>, data: String| -> Result<Promise<'js>> {
                    if data.len() > IO_LIMIT {
                        return Err(Exception::throw_range(
                            &ctx,
                            "socket write exceeded the 1048576 byte limit",
                        ));
                    }
                    let socket = writer.clone();
                    let owned = ctx.clone();
                    Promise::wrap_future(&ctx, async move {
                        use tokio::io::AsyncWriteExt;
                        let ctx = owned;
                        let mut writer = socket.write.lock().await;
                        tokio::time::timeout(TIMEOUT, writer.write_all(data.as_bytes()))
                            .await
                            .map_err(|_| {
                                Exception::throw_message(&ctx, "socket.write(data) timed out")
                            })?
                            .map_err(|e| {
                                Exception::throw_message(&ctx, &format!("socket write failed: {e}"))
                            })
                    })
                }),
            )?;
            object.set("close", Func::from(move || socket.close()))?;
            Ok::<_, rquickjs::Error>(object)
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "net_tests.rs"]
mod tests;

/// The bounded promise-based socket surface, beside its native implementation.
pub(crate) const TYPES: &str = r#"
export interface Socket {
 /** Reads at most 1 MiB; resolves null after the peer closes. */
 read(size?: number): Promise<Uint8Array | null>;
 /** Writes at most 1 MiB of UTF-8 data. */
 write(data: string): Promise<void>;
 /** Interrupts pending reads and writes and closes the native socket. */
 close(): void;
}
/** Connects only to destinations granted by capabilities.network.hosts. */
export function connect(host: string, port: number): Promise<Socket>;
declare const net: { connect: typeof connect };
export default net;
"#;
