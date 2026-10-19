//! Standard modules share NSH's VM, event loop and native permission boundary.
use rquickjs::{Ctx, Error, Module, Result, Value, object::Accessor};

mod console;
pub(crate) mod declarations;
mod os;
pub(crate) mod process;
pub(crate) mod scheduler;
mod url;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod websocket;
mod zlib;

pub(crate) const NAMES: &[&str] = nsh_core::js::STANDARD_MODULES;

/// Only canonical reference specifiers are accepted; aliases stay denied.
pub(crate) fn normalize(name: &str) -> Option<&str> {
    NAMES.iter().copied().find(|candidate| *candidate == name)
}

pub(crate) fn load<'js>(ctx: &Ctx<'js>, name: &str) -> Result<Module<'js>> {
    // LLRT's crypto module expects the classes its global defines.
    // Compiling to bytecode declares modules in a context without NSH's
    // state, and runs none.
    if name == "crypto" && !crate::state::state(ctx.as_raw().as_ptr()).is_null() {
        init_crypto(ctx)?;
    }
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
        "console" => declare!(console::ConsoleModule),
        _ => Err(Error::new_loading(name)),
    }
}

/// Installs one group of [`LAZY`] globals.
type Init = fn(&Ctx<'_>) -> Result<()>;

/// The standard globals made on first use: an app that never touches
/// `process`, `crypto` or `WebSocket` pays nothing for them when its VM
/// starts (`process` and `WebSocket` evaluate JS facades, about 190 µs
/// each in a release build). Each name is an accessor until then; reading
/// or assigning any name of a group installs the whole group, which
/// replaces the accessors with plain data properties.
const LAZY: &[(&[&str], Init)] = &[
    (&["process"], init_process),
    (
        &["crypto", "Crypto", "SubtleCrypto", "CryptoKey"],
        init_crypto,
    ),
    #[cfg(not(target_arch = "wasm32"))]
    (&["WebSocket"], websocket::init),
];

/// Installs `Buffer` and the lazy standard globals. `Buffer` is made now
/// (about 30 µs): LLRT defines its methods by assignment, and over
/// `%TypedArray%.prototype` members assignments throw once hardening has
/// frozen that prototype, so it must exist before [`crate::sandbox::harden`].
pub(crate) fn install<'js>(ctx: &Ctx<'js>) -> Result<()> {
    init_buffer(ctx)?;
    let globals = ctx.globals();
    for &(names, init) in LAZY {
        for &name in names {
            globals.prop(
                name,
                Accessor::new(
                    move |ctx: Ctx<'js>| -> Result<Value<'js>> {
                        init(&ctx)?;
                        ctx.globals().get(name)
                    },
                    move |ctx: Ctx<'js>, value: Value<'js>| -> Result<()> {
                        init(&ctx)?;
                        ctx.globals().set(name, value)
                    },
                )
                .configurable()
                .enumerable(),
            )?;
        }
    }
    Ok(())
}

/// Removes the accessors of the group of `name`, so the installation's own
/// assignments define plain properties; false when the group was installed
/// already.
pub(crate) fn take_lazy(ctx: &Ctx<'_>, name: &str) -> Result<bool> {
    let index = LAZY
        .iter()
        .position(|(names, _)| names.contains(&name))
        .expect("a lazy global");
    let installed = &crate::dom::shared(ctx.as_raw().as_ptr()).lazy_globals;
    let bit = 1 << index;
    if installed.get() & bit != 0 {
        return Ok(false);
    }
    installed.set(installed.get() | bit);
    let globals = ctx.globals();
    for name in LAZY[index].0 {
        globals.remove(*name)?;
    }
    Ok(true)
}

/// `Buffer` and the LLRT state every standard module returning buffers
/// needs. LLRT's initialization also defines its own `Blob`, `File`, `URL`
/// and `URLSearchParams`; NSH's WHATWG globals stay the objects they were.
fn init_buffer(ctx: &Ctx<'_>) -> Result<()> {
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
    Ok(())
}

/// The global `process` (`standard/process.rs`).
fn init_process(ctx: &Ctx<'_>) -> Result<()> {
    if take_lazy(ctx, "process")? {
        process::install(ctx)?;
    }
    Ok(())
}

/// `crypto` and its classes.
fn init_crypto(ctx: &Ctx<'_>) -> Result<()> {
    if take_lazy(ctx, "crypto")? {
        llrt_crypto::init(ctx)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Host, HostOptions};

    fn hardened() -> Host {
        Host::with_options(HostOptions {
            harden: true,
            ..HostOptions::default()
        })
        .unwrap()
    }

    #[test]
    fn standard_globals_are_made_on_first_use() {
        let host = hardened();
        assert!(
            host.eval_script(
                "lazy",
                r#"const lazy = name => typeof Object.getOwnPropertyDescriptor(globalThis, name).get === 'function';
                const names = ['process', 'crypto', 'Crypto', 'SubtleCrypto', 'CryptoKey', 'WebSocket'];
                const web = [URL, Blob, File, TextEncoder, EventTarget, ReadableStream, AbortController];
                const before = names.every(lazy) && !('__nshWebSocketConnect' in globalThis);
                const buffer = Buffer.from('hi').toString('base64') === 'aGk=';
                const proc = typeof process.nextTick === 'function' && !lazy('process') && lazy('crypto') && lazy('WebSocket');
                const crypto = globalThis.crypto.randomBytes(4).length === 4
                    && CryptoKey === globalThis.CryptoKey && !lazy('CryptoKey');
                const socket = typeof WebSocket === 'function' && WebSocket.OPEN === 1 && !lazy('WebSocket');
                const same = [URL, Blob, File, TextEncoder, EventTarget, ReadableStream, AbortController]
                    .every((value, i) => value === web[i]);
                before && buffer && proc && crypto && socket && same
                    && !Object.getOwnPropertyNames(globalThis).some(n => n.startsWith('__nsh') && n !== '__nsh_import')"#
            )
            .unwrap()
        );
    }

    #[test]
    fn standard_globals_can_be_assigned_and_their_modules_load() {
        let host = hardened();
        host.eval_source(
            r#"
            import { randomUUID } from 'crypto';
            import standardProcess from 'process';
            globalThis.crypto = 'mine';
            if (globalThis.crypto !== 'mine' || typeof CryptoKey !== 'function') throw Error('assigned');
            if (randomUUID().length !== 36) throw Error('crypto module');
            if (standardProcess.platform !== process.platform) throw Error('process module');
            "#,
        )
        .unwrap();
    }
}
