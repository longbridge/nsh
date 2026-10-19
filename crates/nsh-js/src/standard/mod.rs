//! Standard modules share NSH's VM, event loop and native permission boundary.
use rquickjs::{Ctx, Error, Module, Result, Value};

mod console;
pub(crate) mod declarations;
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

/// One of a VM's limited connections; dropping it frees the place.
pub(crate) struct Slot(std::rc::Rc<std::cell::Cell<usize>>);

impl Slot {
    /// Takes a place in `count` when fewer than `max` are taken.
    pub(crate) fn acquire(count: &std::rc::Rc<std::cell::Cell<usize>>, max: usize) -> Option<Self> {
        (count.get() < max).then(|| {
            count.set(count.get() + 1);
            Self(count.clone())
        })
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

/// Only canonical reference specifiers are accepted; aliases stay denied.
pub(crate) fn normalize(name: &str) -> Option<&str> {
    NAMES.iter().copied().find(|candidate| *candidate == name)
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
        "process" => declare!(process::ProcessModule),
        "fs/promises" => declare!(fs::FsModule),
        "net" => declare!(net::NetModule),
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
