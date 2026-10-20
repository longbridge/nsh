use nsh_core::ui::description::SetDescription;
use rquickjs::{
    CatchResultExt, Context, Module, Runtime,
    module::{WriteOptions, WriteOptionsEndianness},
};

use crate::error::JsError;

/// Identifies the bytecode format produced by [`compile_bytecode`] and
/// accepted by [`crate::Host::eval_bytecode`]: the QuickJS-NG release and
/// revision, its bytecode format (`BC_VERSION`) and the byte order written.
///
/// It names the engine, not the binding: native targets run quickjs-jit and
/// the browser upstream rquickjs, both building QuickJS-NG at this revision
/// with the same serializer (quickjs-jit's patches leave `JS_WriteObject` /
/// `JS_ReadObject` and the compiler as they are), so bytecode the CLI
/// compiles runs in the web runtime. Bump it with either `-sys` pin
/// ([`ENGINES`]).
pub const BYTECODE_VERSION: &str = "quickjs-ng-0.16.2+0fdea21/bc28/le";

/// The `-sys` crates whose QuickJS-NG [`BYTECODE_VERSION`] names: (package,
/// version). The test `engines_build_the_named_quickjs` holds them to the
/// lockfile.
#[cfg(test)]
const ENGINES: &[(&str, &str)] = &[("quickjs-jit-sys", "0.14.1"), ("rquickjs-sys", "0.14.0")];

/// Module name every bundle is evaluated under; JS stack frames reference
/// it as `bundle.js:LINE:COL`.
pub const BUNDLE_NAME: &str = nsh_core::js::ENTRY_CHUNK;

/// What compiled bytecode leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Strip {
    /// Keeps everything: the functions' source text (`Function.prototype
    /// .toString`) and the line and column tables of stack traces.
    #[default]
    Nothing,
    /// Drops the functions' source text but keeps the line and column
    /// tables: stack frames still read `bundle.js:LINE:COL`, which the
    /// bundle's source map turns into app source locations.
    Source,
    /// Drops both: the smallest bytecode, whose stack frames have no
    /// position (`<null>:0:1`).
    All,
}

impl From<bool> for Strip {
    /// `true`: [`Strip::All`] (release builds), `false`: [`Strip::Nothing`].
    fn from(strip: bool) -> Self {
        if strip { Strip::All } else { Strip::Nothing }
    }
}

/// Compiles a bundle to QuickJS bytecode.
///
/// The bytecode is only valid for the exact QuickJS version linked into
/// this crate, which is why the CLI and the runtime share it.
/// `strip` ([`Strip`], or `true` for [`Strip::All`]) removes source and
/// debug info (smaller, but [`Strip::All`] stacks lose line numbers).
pub fn compile_bytecode(source: &str, strip: impl Into<Strip>) -> Result<Vec<u8>, JsError> {
    compile_bytecode_with(source, strip, &[])
}

/// [`compile_bytecode`] for a bundle that may import the native modules of
/// `ui_sets`, the sets the hosts that run it serve
/// ([`crate::HostOptions::ui_sets`]): imports link by name, so their
/// descriptions are all it takes.
pub fn compile_bytecode_with(
    source: &str,
    strip: impl Into<Strip>,
    ui_sets: &[SetDescription],
) -> Result<Vec<u8>, JsError> {
    compile(
        BUNDLE_NAME,
        source,
        crate::modules::ModuleTable::compiling(ui_sets),
        strip.into(),
    )
}

/// Compiles the chunks of a split bundle (`nsh-bundler` `Bundle::chunks`:
/// `(id, source)`) to bytecode, in their order, for
/// [`crate::Host::add_chunk_bytecode`]. A chunk's imports resolve while it
/// compiles, so `entry` (the bundle's code) and the other chunks are
/// compiled with it (each chunk's bytecode holds only its own module).
pub fn compile_chunk_bytecode(
    entry: &str,
    chunks: &[(&str, &str)],
    strip: impl Into<Strip>,
    ui_sets: &[SetDescription],
) -> Result<Vec<Vec<u8>>, JsError> {
    let strip = strip.into();
    chunks
        .iter()
        .map(|&(id, source)| {
            let table = crate::modules::ModuleTable::compiling(ui_sets);
            {
                let mut code = table.chunks.borrow_mut();
                let others = chunks.iter().filter(|(other, _)| *other != id);
                for (other, source) in std::iter::once(&(BUNDLE_NAME, entry)).chain(others) {
                    let source = crate::modules::ChunkCode::Source((*source).to_owned());
                    code.code.insert((*other).to_owned(), source);
                }
            }
            compile(id, source, table, strip)
                .map_err(|e| JsError::new(format!("chunk {id}: {}", e.message)))
        })
        .collect()
}

/// Compiles module `name` with the loader of `modules`.
fn compile(
    name: &str,
    source: &str,
    modules: crate::modules::ModuleTable,
    strip: Strip,
) -> Result<Vec<u8>, JsError> {
    let rt = Runtime::new()?;
    rt.set_loader(crate::modules::Resolver, crate::modules::Loader);
    let ctx = Context::full(&rt)?;
    ctx.with(|ctx| {
        // Imports resolve while compiling: the native modules declare
        // their exports.
        // The table outlives this context and runtime; compilation only
        // declares imports, without initializing host or LLRT globals.
        crate::modules::install(&ctx, &modules);
        let module = Module::declare(ctx.clone(), name, source)
            .catch(&ctx)
            .map_err(JsError::caught)?;
        let options = WriteOptions {
            endianness: WriteOptionsEndianness::Little,
            strip_source: strip != Strip::Nothing,
            strip_debug: strip == Strip::All,
            ..WriteOptions::default()
        };
        module
            .write(options)
            .map_err(|e| JsError::from_engine(&ctx, e))
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::*;

    #[test]
    fn native_and_standard_imports_compile_together_without_host_initialization() {
        let bytes = compile_bytecode(
            r#"import { Text } from 'nsh';
               import { Buffer } from 'buffer';
               import path from 'path';
               globalThis.result = [Text, Buffer, path];"#,
            false,
        )
        .unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn standard_and_native_modules_share_runtime_loader() {
        let host = crate::Host::new().unwrap();
        host.eval_source(
            r#"
            import * as bare from 'buffer';
            import * as second from 'buffer';
            import { Text } from 'nsh';
            if (bare !== second || typeof Text !== 'function') throw Error('module identity');
        "#,
        )
        .unwrap();
        assert!(host.eval_source("import 'node:buffer';").is_err());
        assert!(host.eval_source("import 'events';").is_err());
        assert!(host.eval_source("import 'node:vm';").is_err());
        assert!(host.eval_source("import 'node:node:buffer';").is_err());
    }

    /// Fails when the engine is upgraded without bumping `BYTECODE_VERSION`:
    /// its release, and the format byte bytecode starts with.
    #[test]
    fn bytecode_version_matches_linked_engine() {
        // SAFETY: returns a static NUL-terminated string.
        let engine = unsafe { CStr::from_ptr(rquickjs::qjs::JS_GetVersion()) }
            .to_str()
            .unwrap();
        assert!(
            BYTECODE_VERSION.starts_with(&format!("quickjs-ng-{engine}+")),
            "linked QuickJS-NG {engine}, BYTECODE_VERSION says {BYTECODE_VERSION}"
        );
        let bytecode = compile_bytecode("export const x = 1;", true).unwrap();
        assert!(
            BYTECODE_VERSION.contains(&format!("/bc{}/", bytecode[0])),
            "bytecode format {}, BYTECODE_VERSION says {BYTECODE_VERSION}",
            bytecode[0]
        );
    }

    /// Native targets and the browser link different `-sys` crates; both
    /// must be the versions that build `BYTECODE_VERSION`'s QuickJS-NG
    /// revision, or bytecode compiled by the CLI would not be the web
    /// runtime's.
    #[test]
    fn engines_build_the_named_quickjs() {
        let lock = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
        )
        .unwrap();
        for (package, version) in ENGINES {
            let entry = format!("name = \"{package}\"\nversion = \"{version}\"\n");
            assert!(
                lock.contains(&entry),
                "Cargo.lock has no {package} {version}: check its QuickJS-NG against BYTECODE_VERSION ({BYTECODE_VERSION}), then update ENGINES"
            );
        }
    }
}
