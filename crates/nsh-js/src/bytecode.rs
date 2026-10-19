use rquickjs::{
    CatchResultExt, Context, Module, Runtime,
    module::{WriteOptions, WriteOptionsEndianness},
};

use crate::error::JsError;

/// Identifies the bytecode format produced by [`compile_bytecode`] and
/// accepted by [`crate::Host::eval_bytecode`]: QuickJS bytecode is only
/// valid for the exact engine build. Bump together with the `rquickjs` pin.
pub const BYTECODE_VERSION: &str = "rquickjs-0.14.0/quickjs-ng-0.16.2/le64";

/// Module name every bundle is evaluated under; JS stack frames reference
/// it as `bundle.js:LINE:COL`.
pub const BUNDLE_NAME: &str = nsh_core::js::ENTRY_CHUNK;

/// Compiles a bundle to QuickJS bytecode.
///
/// The bytecode is only valid for the exact QuickJS version linked into
/// this crate, which is why the CLI and the runtime share it.
/// `strip` removes source and debug info (smaller, but stacks lose line
/// numbers): use it for release builds only.
pub fn compile_bytecode(source: &str, strip: bool) -> Result<Vec<u8>, JsError> {
    compile_bytecode_with(source, strip, &[])
}

/// [`compile_bytecode`] for a bundle that may import the native modules of
/// `ui_sets` (as [`crate::HostOptions::ui_sets`]).
pub fn compile_bytecode_with(
    source: &str,
    strip: bool,
    ui_sets: &[nsh_core::ui::Registry],
) -> Result<Vec<u8>, JsError> {
    compile(
        BUNDLE_NAME,
        source,
        crate::modules::ModuleTable::new(ui_sets),
        strip,
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
    strip: bool,
    ui_sets: &[nsh_core::ui::Registry],
) -> Result<Vec<Vec<u8>>, JsError> {
    chunks
        .iter()
        .map(|&(id, source)| {
            let table = crate::modules::ModuleTable::new(ui_sets);
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
    strip: bool,
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
            strip_source: strip,
            strip_debug: strip,
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

    /// Fails when the engine is upgraded without bumping `BYTECODE_VERSION`.
    #[test]
    fn bytecode_version_matches_linked_engine() {
        // SAFETY: returns a static NUL-terminated string.
        let engine = unsafe { CStr::from_ptr(rquickjs::qjs::JS_GetVersion()) }
            .to_str()
            .unwrap();
        assert!(
            BYTECODE_VERSION.contains(&format!("quickjs-ng-{engine}/")),
            "linked QuickJS-NG {engine}, BYTECODE_VERSION says {BYTECODE_VERSION}"
        );
    }
}
