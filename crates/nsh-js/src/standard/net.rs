//! Checked, bounded TCP client sockets. This is NSH's promise-based socket API.
use std::{rc::Rc, time::Duration};

use rquickjs::{
    Ctx, Exception, Object, Result, TypedArray, Value,
    function::{Async, Func, Opt},
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
        net.set("connect", Func::from(Async(connect)))?;
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
    async fn operation<T>(
        &self,
        future: impl std::future::Future<Output = std::io::Result<T>>,
    ) -> std::result::Result<T, String> {
        if self.closed.get() {
            return Err("socket is closed".into());
        }
        tokio::select! {
            biased;
            _ = self.cancel.notified() => Err("socket is closed".into()),
            result = tokio::time::timeout(TIMEOUT, future) => result.map_err(|_| "socket operation timed out".to_owned())?.map_err(|e| e.to_string()),
        }
    }
}

async fn connect<'js>(ctx: Ctx<'js>, host: String, port: u16) -> Result<Object<'js>> {
    if !crate::dom::shared(ctx.as_raw().as_ptr())
        .system_access
        .borrow()
        .allows_socket(&host, port)
    {
        return Err(Exception::throw_type(
            &ctx,
            "TCP destination is not granted by capabilities.network.hosts",
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
        let stream = tokio::time::timeout(
            TIMEOUT,
            tokio::net::TcpStream::connect((host.as_str(), port)),
        )
        .await
        .map_err(|_| Exception::throw_message(&ctx, "TCP connection timed out"))?
        .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
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
        object.set(
            "read",
            Func::from(Async(move |ctx: Ctx<'js>, size: Opt<usize>| {
                let socket = reader.clone();
                async move {
                    use tokio::io::AsyncReadExt;
                    let size = size.0.unwrap_or(64 * 1024);
                    if size == 0 || size > IO_LIMIT {
                        return Err(Exception::throw_range(
                            &ctx,
                            "socket read size must be between 1 and 1048576",
                        ));
                    }
                    let mut reader = socket.read.try_lock().map_err(|_| {
                        Exception::throw_type(&ctx, "socket already has a pending read")
                    })?;
                    let mut bytes = vec![0; size];
                    let count = socket
                        .operation(async { reader.read(&mut bytes).await })
                        .await
                        .map_err(|e| Exception::throw_message(&ctx, &e))?;
                    if count == 0 {
                        return Ok(Value::new_null(ctx));
                    }
                    bytes.truncate(count);
                    Ok(TypedArray::<u8>::new(ctx, bytes)?.into_value())
                }
            })),
        )?;
        let writer = socket.clone();
        object.set(
            "write",
            Func::from(Async(move |ctx: Ctx<'js>, data: String| {
                let socket = writer.clone();
                async move {
                    use tokio::io::AsyncWriteExt;
                    if data.len() > IO_LIMIT {
                        return Err(Exception::throw_range(
                            &ctx,
                            "socket write exceeded the 1048576 byte limit",
                        ));
                    }
                    let mut writer = socket.write.try_lock().map_err(|_| {
                        Exception::throw_type(&ctx, "socket already has a pending write")
                    })?;
                    socket
                        .operation(async { writer.write_all(data.as_bytes()).await })
                        .await
                        .map_err(|e| Exception::throw_message(&ctx, &e))
                }
            })),
        )?;
        object.set("close", Func::from(move || socket.close()))?;
        Ok(object)
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
