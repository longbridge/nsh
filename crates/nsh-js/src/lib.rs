//! QuickJS host for nsh.
//!
//! [`Host`] owns one QuickJS runtime and context, installs the globals
//! apps use (`console`, timers, web APIs such as `fetch`), serves nsh's
//! native modules (`nsh`, `nsh:rt`, `nsh/*`: the reactive graph, the
//! renderer, components), queues what the app changes as
//! [`nsh_core::Op`]s and dispatches native events back into JS.
//!
//! A host is single-threaded (`!Send`): create and use it on the engine
//! thread.

// QuickJS is quickjs-jit on native targets and upstream rquickjs in the
// browser: the same API and QuickJS-NG build, so every `rquickjs::` path
// (and the macros' own) names whichever this target links.
#[cfg(not(target_arch = "wasm32"))]
extern crate rquickjs_jit as rquickjs;

mod animated;
mod bytecode;
mod canvas;
mod chunks;
mod classes;
mod components;
mod console;
mod declarations;
mod desktop;
mod dom;
mod env;
mod error;
mod host;
mod hot;
mod keymap;
mod list;
mod modules;
mod navigation;
mod net;
mod platform_modules;
mod protocol;
mod reactive;
mod rejections;
mod resource;
mod sandbox;
mod standard;
mod state;
mod timers;
mod touch;
mod web;

pub use nsh_core::{
    js::Limits,
    platform::{ColorScheme, Environment, Idiom, Insets, Os},
};

pub use crate::{
    bytecode::{
        BUNDLE_NAME, BYTECODE_VERSION, Strip, compile_bytecode, compile_bytecode_with,
        compile_chunk_bytecode,
    },
    declarations::type_definitions,
    error::JsError,
    host::{ErrorHandler, Host, HostOptions, TextMeasurer},
    net::{FetchEvent, FetchOp, HttpBody, HttpHead, HttpRequest, NetworkQueue, RequestId},
    sandbox::compile_harden,
};

/// The names native module `specifier` exports, if it is one: nsh's own
/// modules, the runtime's and those of `ui_sets` (a bundler checks named
/// imports against them).
pub fn native_module_exports(
    specifier: &str,
    ui_sets: &[nsh_core::ui::description::SetDescription],
) -> Option<Vec<String>> {
    modules::ModuleTable::compiling(ui_sets).export_names(specifier)
}
