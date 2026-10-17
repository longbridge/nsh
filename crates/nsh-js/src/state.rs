//! The state of a context's natives: the reactive graph and the renderer,
//! reached through the context's opaque pointer (natives get the context)
//! and, for class finalizers and GC marks (which get the runtime), through
//! a per-thread map from runtime to state.
//!
//! Natives take short references to one part at a time and never hold one
//! across a call into JS or a free of a JS value: either may re-enter
//! (another native, a finalizer).

use std::{cell::RefCell, ffi::c_void, ptr, rc::Rc};

use nsh_core::ui::Registry;
use rquickjs::{Ctx, qjs};
use rustc_hash::FxHashMap;

use crate::{
    components::Components, dom::Dom, env::Env, host::Shared, list::Lists, modules::ModuleTable,
    reactive::Graph,
};

pub(crate) struct State {
    pub(crate) graph: Graph,
    pub(crate) dom: Dom,
    pub(crate) env: Env,
    pub(crate) components: Components,
    pub(crate) lists: Lists,
    pub(crate) platform: crate::platform_modules::PlatformModules,
    pub(crate) menus: crate::desktop::Menus,
    pub(crate) touches: crate::touch::Touches,
    pub(crate) animations: crate::animated::Animations,
    pub(crate) canvases: crate::canvas::Canvases,
    pub(crate) web: crate::web::Web,
    pub(crate) hot: crate::hot::Hot,
    pub(crate) keymap: crate::keymap::Keymap,
    pub(crate) chunks: crate::chunks::Chunks,
    /// The native modules the context serves (the loader reads it).
    pub(crate) modules: ModuleTable,
}

thread_local! {
    /// The state of each runtime (one context per runtime), for finalizers.
    static STATES: RefCell<FxHashMap<usize, usize>> = RefCell::default();
}

/// The state of `ctx`, set by [`install`] (callers project the part they
/// use, never a reference to the whole).
#[inline]
pub(crate) fn state(ctx: *mut qjs::JSContext) -> *mut State {
    // SAFETY: reading the opaque pointer of a live context.
    unsafe { qjs::JS_GetContextOpaque(ctx) as *mut State }
}

/// The state of runtime `rt`, if it has one (finalizers may run after
/// [`uninstall`], while the context is freed).
pub(crate) fn of_runtime(rt: *mut qjs::JSRuntime) -> Option<*mut State> {
    STATES.with(|m| m.borrow().get(&(rt as usize)).map(|&p| p as *mut State))
}

/// Makes the state of `ctx`: the graph, the renderer and their classes,
/// the environment, and the module loader of the native modules (with the
/// UI sets `ui_sets`).
pub(crate) fn install(ctx: &Ctx<'_>, shared: Rc<Shared>, ui_sets: &[Registry]) {
    let raw = ctx.as_raw().as_ptr();
    // SAFETY: registering classes on the context's runtime; the state is
    // freed by `uninstall`.
    unsafe {
        let rt = qjs::JS_GetRuntime(raw);
        let graph = Graph::new(raw);
        let dom = Dom::new(raw, shared);
        let state = Box::into_raw(Box::new(State {
            graph,
            dom,
            env: Env::default(),
            components: Components::default(),
            lists: Lists::default(),
            platform: Default::default(),
            menus: Default::default(),
            touches: Default::default(),
            animations: Default::default(),
            canvases: Default::default(),
            web: Default::default(),
            hot: Default::default(),
            keymap: Default::default(),
            chunks: Default::default(),
            modules: ModuleTable::new(ui_sets),
        }));
        qjs::JS_SetContextOpaque(raw, state as *mut c_void);
        STATES.with(|m| m.borrow_mut().insert(rt as usize, state as usize));
        crate::dom::init(raw);
        crate::env::init(raw);
        crate::modules::set_loader(rt, &(*state).modules);
    }
}

/// Frees the state of `ctx` and every JS value it holds: before the
/// context goes (QuickJS checks that nothing is left).
pub(crate) fn uninstall(ctx: &Ctx<'_>) {
    let raw = ctx.as_raw().as_ptr();
    // SAFETY: the state `install` made; no JS runs from here on.
    unsafe {
        let p = qjs::JS_GetContextOpaque(raw) as *mut State;
        if p.is_null() {
            return;
        }
        // Unregistered first: the frees below finalize nodes and graph
        // objects, whose finalizers then only free their own data.
        let rt = qjs::JS_GetRuntime(raw);
        STATES.with(|m| m.borrow_mut().remove(&(rt as usize)));
        qjs::JS_SetContextOpaque(raw, ptr::null_mut());
        let mut state = Box::from_raw(p);
        state.hot.free_all();
        state.chunks.free_all();
        state.keymap.free_all();
        state.web.free_all();
        state.canvases.free_all(raw);
        state.animations.free_all(raw);
        state.platform.free_all(raw);
        state.menus.free_all(raw);
        state.lists.free_all(raw);
        state.components.free_all(raw);
        state.env.free_all(raw);
        state.dom.free_all(raw);
        state.graph.free_all(raw);
    }
}
