//! Standard modules share NSH's VM, event loop and native permission boundary.
use rquickjs::{Ctx, Error, Module, Result, Value};

pub(crate) mod declarations;
mod console;
pub(crate) mod fs;
pub(crate) mod net;
mod os;
pub(crate) mod process;
pub(crate) mod scheduler;
mod url;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod websocket;
mod zlib;

pub(crate) const NAMES: &[&str] = nsh_core::js::STANDARD_MODULES;

/// Only one optional node: prefix is accepted. Private and unknown modules stay denied.
pub(crate) fn normalize(name: &str) -> Option<&str> {
    let canonical = name.strip_prefix("node:").unwrap_or(name);
    NAMES
        .iter()
        .copied()
        .find(|candidate| *candidate == canonical)
}

pub(crate) fn load<'js>(ctx: &Ctx<'js>, name: &str) -> Result<Module<'js>> {
    macro_rules! declare {
        ($ty:ty) => {
            Module::declare_def::<$ty, _>(ctx.clone(), name)
        };
    }
    match name {
        "buffer" => declare!(llrt_buffer::BufferModule),
        "crypto" => declare!(llrt_crypto::CryptoModule),
        "path" => declare!(llrt_path::PathModule),
        "url" => declare!(url::UrlModule),
        "zlib" => declare!(zlib::ZlibModule),
        "events" => declare!(llrt_events::EventsModule),
        "assert" => declare!(llrt_assert::AssertModule),
        "util" => declare!(llrt_util::UtilModule),
        "string_decoder" => declare!(llrt_string_decoder::StringDecoderModule),
        "process" => declare!(process::ProcessModule),
        "fs" | "fs/promises" => declare!(fs::FsModule),
        "net" => declare!(net::NetModule),
        "os" => declare!(os::OsModule),
        "console" => declare!(console::ConsoleModule),
        #[cfg(not(target_arch = "wasm32"))]
        "websocket" => declare!(websocket::WebSocketModule),
        _ => Err(Error::new_loading(name)),
    }
}

/// Add Buffer and crypto without replacing NSH's existing WHATWG object brands.
pub(crate) fn install(ctx: &Ctx<'_>) -> Result<()> {
    let globals = ctx.globals();
    let preserved = [
        "Blob",
        "File",
        "URL",
        "URLSearchParams",
        "TextEncoder",
        "TextDecoder",
        "atob",
        "btoa",
        "Event",
        "CustomEvent",
        "EventTarget",
        "ReadableStream",
        "WritableStream",
        "TransformStream",
        "AbortController",
        "AbortSignal",
    ]
    .into_iter()
    .map(|name| Ok((name, globals.get::<_, Value>(name)?)))
    .collect::<Result<Vec<_>>>()?;
    llrt_buffer::init(ctx)?;
    // LLRT URL helper classes are registered once; public constructors remain NSH's.
    #[cfg(not(target_arch = "wasm32"))]
    llrt_url::init(ctx)?;
    for (name, value) in preserved {
        globals.set(name, value)?;
    }
    llrt_crypto::init(ctx)?;
    process::install(ctx)?;
    #[cfg(not(target_arch = "wasm32"))]
    websocket::install(ctx)?;
    Ok(())
}
