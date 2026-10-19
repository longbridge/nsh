//! The JS face of Rust components and modules, made from their specs
//! (`nsh_core::ui`).
//!
//! A component is a host view naming it (`component`), its own props
//! converted into their Rust types when they change (one render effect
//! reads them all; a value of the wrong type is a warning, as a host
//! element's, and keeps the prop's value) and set as `Prop::RustProp`, its events delivered to the `on<Event>`
//! handlers, its children inserted when it takes some, and one holder per
//! named slot the app fills. A module's function without a context runs in
//! the JS call (`classes.rs`); one with a context or `async`, and a
//! component's method, converts its arguments in the JS call and sends
//! what it runs to the core as an op, in order with the ops before it
//! (`classes::send`); one with a result returns a promise, settled when
//! the core's events bring the result ([`deliver`]).

use std::{
    cell::{Cell, RefCell},
    ffi::{CString, c_int},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use nsh_core::{
    Command, EventKind, Op, Prop,
    ui::{ComponentSpec, FunctionCall, JsEvent, JsIn, JsResult, ModuleSpec, PropKind, RustCall},
};
use rquickjs::qjs;
use rustc_hash::FxHashMap;

use crate::{
    dom::{self, string_arg},
    reactive::{self, arg},
    web::js::V,
};

type JsValue = qjs::JSValue;
type JsContext = qjs::JSContext;

const UNDEFINED: JsValue = qjs::JS_UNDEFINED;

/// What the props a component's host takes as a view are called.
const HOST_PROPS: &[&str] = &["class", "classList", "style", "testID", "onLayout"];

/// The components' part of a context's state.
#[derive(Default)]
pub(crate) struct Components {
    /// The specs of the components made so far (functions name them by
    /// index).
    specs: RefCell<Vec<Arc<ComponentSpec>>>,
    /// Calls waiting for a result: call number -> (resolve, reject), owned.
    pending: RefCell<FxHashMap<u32, (JsValue, JsValue)>>,
    calls: Cell<u32>,
}

impl Components {
    /// Frees the values the components hold (the state is unregistered).
    pub(crate) fn free_all(&mut self, raw: *mut JsContext) {
        for (_, (resolve, reject)) in self.pending.borrow_mut().drain() {
            // SAFETY: values the table owned; no JS runs from here on.
            unsafe {
                qjs::JS_FreeValue(raw, resolve);
                qjs::JS_FreeValue(raw, reject);
            }
        }
    }
}

fn components<'a>(ctx: *mut JsContext) -> &'a Components {
    // SAFETY: a projection of the state's field, by shared reference.
    unsafe { &(*crate::state::state(ctx)).components }
}

fn new_string(ctx: *mut JsContext, s: &str) -> JsValue {
    // SAFETY: a new string.
    unsafe { qjs::JS_NewStringLen(ctx, s.as_ptr().cast(), s.len() as _) }
}

fn get(ctx: *mut JsContext, obj: JsValue, key: &str) -> JsValue {
    let k = CString::new(key).unwrap_or_default();
    // SAFETY: a property read of a live value (undefined for non-objects).
    unsafe {
        if !qjs::JS_IsObject(obj) {
            return UNDEFINED;
        }
        qjs::JS_GetPropertyStr(ctx, obj, k.as_ptr())
    }
}

fn has(ctx: *mut JsContext, obj: JsValue, key: &str) -> bool {
    // SAFETY: as `get`.
    unsafe {
        if !qjs::JS_IsObject(obj) {
            return false;
        }
        let k = CString::new(key).unwrap_or_default();
        let atom = qjs::JS_NewAtom(ctx, k.as_ptr());
        let r = qjs::JS_HasProperty(ctx, obj, atom);
        qjs::JS_FreeAtom(ctx, atom);
        r == 1
    }
}

/// `press` -> `onPress`.
fn handler_name(event: &str) -> String {
    let mut c = event.chars();
    match c.next() {
        Some(first) => format!("on{}{}", first.to_ascii_uppercase(), c.as_str()),
        None => "on".to_owned(),
    }
}

// --- Calls and their results -----------------------------------------------------------

/// A new call number (never 0: no reply).
pub(crate) fn next_call(ctx: *mut JsContext) -> u32 {
    let c = components(ctx);
    let n = c.calls.get().wrapping_add(1).max(1);
    c.calls.set(n);
    n
}

/// The promise of call `call`'s result, settled when the core's events
/// bring it ([`deliver`]).
pub(crate) fn promise(ctx: *mut JsContext, call: u32) -> JsValue {
    let mut funcs = [UNDEFINED; 2];
    // SAFETY: a new promise; the table owns its functions.
    let promise = unsafe { qjs::JS_NewPromiseCapability(ctx, funcs.as_mut_ptr()) };
    // SAFETY: an inline tag check.
    if unsafe { qjs::JS_IsException(promise) } {
        return promise;
    }
    components(ctx)
        .pending
        .borrow_mut()
        .insert(call, (funcs[0], funcs[1]));
    promise
}

/// `ref.measure()`: a promise of where node `id` is after this turn's
/// layout, which the core answers on the root (`Command::Measure`).
pub(crate) fn measure(ctx: *mut JsContext, id: u32) -> JsValue {
    let call = next_call(ctx);
    if let Err(e) = dom::push_op(
        ctx,
        Op::Command {
            id,
            command: Command::Measure { call },
        },
    ) {
        return e;
    }
    promise(ctx, call)
}

/// Delivers what the core hands JS besides nodes' events: a Rust
/// component's event (to its `on<Event>` handler), whether its lazy
/// children are placed, a call's result (its promise settles, the value
/// converted now). `Err`: the exception a handler threw. The Rust code it
/// runs (conversions, drops) cannot unwind out: a panic there is logged,
/// or rejects the call's promise.
pub(crate) fn deliver(ctx: *mut JsContext, event: JsEvent) -> Result<(), JsValue> {
    match event {
        JsEvent::Event(..) => Ok(()),
        JsEvent::Component { host, event } => {
            let host_js = crate::classes::host_outside_calls(ctx);
            let name = event.name();
            let data = catch_unwind(AssertUnwindSafe(move || {
                event.data_js(&host_js).map(|d| d.into_raw())
            }));
            let data = match data {
                Ok(Ok(data)) => data,
                Ok(Err(e)) => {
                    // What a conversion threw (the error says it).
                    let _ = crate::web::js::take_exception(ctx);
                    log::error!(target: "js", "the data of event `{name}` did not convert: {e}");
                    return Ok(());
                }
                Err(payload) => {
                    let _ = crate::web::js::take_exception(ctx);
                    log::error!(
                        target: "js",
                        "the data of event `{name}` panicked as it converted: {}",
                        crate::classes::panic_message(payload)
                    );
                    return Ok(());
                }
            };
            component_event(ctx, host, name, crate::classes::from_raw(data))
        }
        JsEvent::Children { host, wanted } => {
            let data = if wanted { qjs::JS_TRUE } else { qjs::JS_FALSE };
            component_event(ctx, host, CHILDREN, data)
        }
        JsEvent::Reply { call, result } => settle(ctx, call, result),
    }
}

/// The name of the event placing lazy children (no component's event
/// has a `$`).
const CHILDREN: &str = "$children";

/// Calls the native-event handler of Rust component host `host` with
/// `{ name, data }` (`data` taken).
fn component_event(
    ctx: *mut JsContext,
    host: nsh_core::NodeId,
    name: &str,
    data: JsValue,
) -> Result<(), JsValue> {
    let Some(handler) = dom::handler(ctx, host, EventKind::NativeEvent) else {
        // SAFETY: ours.
        unsafe { qjs::JS_FreeValue(ctx, data) };
        return Ok(());
    };
    // SAFETY: a new object taking the name and the data; the handler and
    // the object are ours, freed after the call.
    unsafe {
        let event = qjs::JS_NewObject(ctx);
        qjs::JS_SetPropertyStr(ctx, event, c"name".as_ptr(), new_string(ctx, name));
        qjs::JS_SetPropertyStr(ctx, event, c"data".as_ptr(), data);
        let mut args = [event];
        let r = qjs::JS_Call(ctx, handler, UNDEFINED, 1, args.as_mut_ptr());
        qjs::JS_FreeValue(ctx, event);
        qjs::JS_FreeValue(ctx, handler);
        if qjs::JS_IsException(r) {
            return Err(r);
        }
        qjs::JS_FreeValue(ctx, r);
    }
    Ok(())
}

/// Settles call `call` with `result`: resolved with the value (converted
/// now), or rejected with its error (a [`nsh_core::ui::JsError`]'s class
/// and properties; an `Error` of the message when the value does not
/// convert, or its conversion panics).
fn settle(ctx: *mut JsContext, call: u32, result: JsResult) -> Result<(), JsValue> {
    let pending = components(ctx).pending.borrow_mut().remove(&call);
    // Nobody waits (the promise's VM state was reset): the value drops,
    // its `Drop` (the app's code) caught.
    let Some((resolve, reject)) = pending else {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(move || drop(result))) {
            log::error!(
                target: "js",
                "a call's result panicked as it dropped: {}",
                crate::classes::panic_message(payload)
            );
        }
        return Ok(());
    };
    // Owned from here: freed whatever happens.
    let (resolve, reject) = (V::own(ctx, resolve), V::own(ctx, reject));
    let host = crate::classes::host_outside_calls(ctx);
    let converted = catch_unwind(AssertUnwindSafe(|| {
        result.and_then(|value| {
            value
                .to_js_value(&host)
                .map(|v| V::own(ctx, crate::classes::from_raw(v.into_raw())))
                .map_err(|e| nsh_core::ui::Thrown::Error(e.0))
        })
    }))
    .unwrap_or_else(|payload| Err(crate::classes::panic_message(payload).into()));
    let (f, value) = match converted {
        Ok(value) => (resolve, value),
        Err(thrown) => {
            host.clear_pending();
            (reject, crate::classes::error_value(ctx, &thrown))
        }
    };
    match crate::web::js::call(ctx, f.raw(), UNDEFINED, &[value.raw()]) {
        Ok(_) => Ok(()),
        Err(_) => Err(qjs::JS_EXCEPTION),
    }
}

// --- Modules ------------------------------------------------------------------------

/// A function of a module: one that runs in the JS call, or one sending
/// what it runs to the core.
pub(crate) fn module_function(ctx: *mut JsContext, module: &ModuleSpec, function: &str) -> JsValue {
    let Some(spec) = module.functions.iter().find(|f| f.name == function) else {
        return UNDEFINED;
    };
    let length = spec.params.len();
    let (f, opaque): (qjs::JSCClosure, *const ()) = match spec.call {
        FunctionCall::Sync(f) => return crate::classes::sync_function(ctx, function, f, length),
        FunctionCall::Op(make) => (Some(call_op), make as *const ()),
        FunctionCall::Async(start) => (Some(call_task), start as *const ()),
        FunctionCall::Start(start) => (Some(call_start), start as *const ()),
    };
    let name = CString::new(function).unwrap_or_default();
    // SAFETY: the opaque is the function's conversion (a function pointer,
    // as large as a data pointer on every target), which needs no
    // finalizer; the magic says whether it returns a promise.
    unsafe {
        qjs::JS_NewCClosure(
            ctx,
            f,
            name.as_ptr(),
            None,
            length as c_int,
            c_int::from(spec.returns.is_some()),
            opaque as *mut std::ffi::c_void,
        )
    }
}

/// The conversion of a module function with a context.
type MakeOp = fn(&dyn nsh_core::ui::JsHost) -> Result<nsh_core::ui::OpCall, nsh_core::ui::Thrown>;

unsafe extern "C" fn call_op(
    ctx: *mut JsContext,
    this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    magic: c_int,
    opaque: *mut std::ffi::c_void,
) -> JsValue {
    // SAFETY: `module_function` made the opaque from a `MakeOp`, and
    // QuickJS passes `argc` live arguments.
    let (make, args) = unsafe {
        (
            std::mem::transmute::<*const (), MakeOp>(opaque as *const ()),
            crate::web::js::Args::new(argc, argv),
        )
    };
    crate::classes::send(ctx, this, args, magic != 0, |host| {
        make(host).map(RustCall::Op)
    })
}

unsafe extern "C" fn call_task(
    ctx: *mut JsContext,
    this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    magic: c_int,
    opaque: *mut std::ffi::c_void,
) -> JsValue {
    // SAFETY: `module_function` made the opaque from an `AsyncCall`, and
    // QuickJS passes `argc` live arguments.
    let (start, args) = unsafe {
        (
            std::mem::transmute::<*const (), nsh_core::ui::AsyncCall>(opaque as *const ()),
            crate::web::js::Args::new(argc, argv),
        )
    };
    crate::classes::send(ctx, this, args, magic != 0, |host| {
        start(host).map(RustCall::Task)
    })
}

/// A function starting its task in the JS call (`Later`): what the start
/// throws throws.
unsafe extern "C" fn call_start(
    ctx: *mut JsContext,
    this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    opaque: *mut std::ffi::c_void,
) -> JsValue {
    // SAFETY: `module_function` made the opaque from an `AsyncCall`, and
    // QuickJS passes `argc` live arguments.
    let (start, args) = unsafe {
        (
            std::mem::transmute::<*const (), nsh_core::ui::AsyncCall>(opaque as *const ()),
            crate::web::js::Args::new(argc, argv),
        )
    };
    crate::classes::start(ctx, this, args, |host| start(host).map(RustCall::Task))
}

/// A module's object: its functions.
pub(crate) fn module(ctx: *mut JsContext, module: &ModuleSpec) -> JsValue {
    // SAFETY: a new object taking the functions.
    let obj = unsafe { qjs::JS_NewObject(ctx) };
    add_functions(ctx, obj, module);

    obj
}

fn add_functions(ctx: *mut JsContext, obj: JsValue, module: &ModuleSpec) {
    for f in &module.functions {
        let func = module_function(ctx, module, f.name);
        let name = CString::new(f.name).unwrap_or_default();
        if f.getter {
            // SAFETY: an accessor of the object, which takes the getter.
            unsafe {
                let atom = qjs::JS_NewAtom(ctx, name.as_ptr());
                qjs::JS_DefinePropertyGetSet(
                    ctx,
                    obj,
                    atom,
                    func,
                    UNDEFINED,
                    (qjs::JS_PROP_ENUMERABLE | qjs::JS_PROP_CONFIGURABLE) as c_int,
                );
                qjs::JS_FreeAtom(ctx, atom);
            }
            continue;
        }
        // SAFETY: the object takes the function.
        unsafe { qjs::JS_SetPropertyStr(ctx, obj, name.as_ptr(), func) };
    }
    for constant in &module.constants {
        use nsh_core::ui::ConstantValue;
        let value = match constant.value {
            ConstantValue::Undefined => UNDEFINED,
            ConstantValue::String(value) => new_string(ctx, value),
            ConstantValue::Number(value) => qjs::JS_NewFloat64(value),
            ConstantValue::Bool(value) => {
                if value {
                    qjs::JS_TRUE
                } else {
                    qjs::JS_FALSE
                }
            }
        };
        let name = CString::new(constant.name).unwrap_or_default();
        // SAFETY: this live module object takes ownership of the newly
        // created primitive; its constant name is NUL-terminated.
        unsafe {
            qjs::JS_SetPropertyStr(ctx, obj, name.as_ptr(), value);
        }
    }
}

/// The object of module `module` of the runtime registry `specifier`
/// (owned), if there is one.
pub(crate) fn runtime_module(
    ctx: *mut JsContext,
    specifier: &str,
    module: &str,
) -> Option<JsValue> {
    let registry = nsh_core::runtime::registries()
        .iter()
        .find(|r| r.specifier() == Some(specifier))?;
    let spec = registry
        .module_specs()
        .into_iter()
        .find(|m| m.name == module)?;
    Some(self::module(ctx, &spec))
}

/// Puts the functions of module `module` of the runtime registry
/// `specifier` on `obj` (`Image.getSize` on `nsh/image`'s component).
pub(crate) fn add_runtime_functions(
    ctx: *mut JsContext,
    obj: JsValue,
    specifier: &str,
    module: &str,
) {
    let Some(registry) = nsh_core::runtime::registries()
        .iter()
        .find(|r| r.specifier() == Some(specifier))
    else {
        return;
    };
    if let Some(spec) = registry
        .module_specs()
        .into_iter()
        .find(|m| m.name == module)
    {
        add_functions(ctx, obj, &spec);
    }
}

// --- Components --------------------------------------------------------------------

/// The function rendering component `spec` (with the functions of a module
/// of the same name on it).
pub(crate) fn component(
    ctx: *mut JsContext,
    spec: Arc<ComponentSpec>,
    module: Option<Arc<ModuleSpec>>,
) -> JsValue {
    let c = components(ctx);
    let index = {
        let mut specs = c.specs.borrow_mut();
        specs.push(spec);
        specs.len() - 1
    };
    let mut data = [qjs::JS_NewFloat64(index as f64)];
    // SAFETY: the function keeps a dup of `data`.
    let f = unsafe { qjs::JS_NewCFunctionData(ctx, Some(render), 1, 0, 1, data.as_mut_ptr()) };
    if let Some(module) = module {
        add_functions(ctx, f, &module);
    }
    f
}

fn spec_of(ctx: *mut JsContext, data: *mut JsValue) -> Arc<ComponentSpec> {
    let mut index = 0f64;
    // SAFETY: one data value, a number (`component`).
    unsafe { qjs::JS_ToFloat64(ctx, &mut index, *data) };
    components(ctx).specs.borrow()[index as usize].clone()
}

/// A function returning `props[key]` (read when the renderer calls it).
fn prop_reader(ctx: *mut JsContext, props: JsValue, key: &str) -> JsValue {
    let mut data = [props, new_string(ctx, key)];
    // SAFETY: the function keeps dups of `data`; our string is freed.
    unsafe {
        let f = qjs::JS_NewCFunctionData(ctx, Some(read_prop), 0, 0, 2, data.as_mut_ptr());
        qjs::JS_FreeValue(ctx, data[1]);
        f
    }
}

unsafe extern "C" fn read_prop(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values (`prop_reader`).
    let (props, key) = unsafe { (*data, *data.add(1)) };
    match string_arg(ctx, key) {
        Some(key) => get(ctx, props, &key),
        None => qjs::JS_EXCEPTION,
    }
}

/// Renders a Rust component: `render(props)` is the component's function.
unsafe extern "C" fn render(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    let spec = spec_of(ctx, data);
    let props = arg(argc, argv, 0);
    match render_component(ctx, &spec, props, data) {
        Ok(node) => reactive::finish_value(ctx, node),
        Err(e) => e,
    }
}

fn render_component(
    ctx: *mut JsContext,
    spec: &ComponentSpec,
    props: JsValue,
    spec_data: *mut JsValue,
) -> Result<JsValue, JsValue> {
    let node = dom::create_element(ctx, nsh_core::Tag::View)?;
    let guard = Owned(ctx, node);
    let name = new_string(ctx, spec.name);
    let r = dom::set_prop_value(ctx, node, "component", name);
    // SAFETY: ours.
    unsafe { qjs::JS_FreeValue(ctx, name) };
    r?;
    // One holder per slot the app fills (props are fixed per JSX site).
    let mut anchor: Option<Owned> = None;
    for slot in spec.slots() {
        if !has(ctx, props, slot) {
            continue;
        }
        let holder = Owned(ctx, dom::create_element(ctx, nsh_core::Tag::View)?);
        let slot_name = new_string(ctx, slot);
        let r = dom::set_prop_value(ctx, holder.1, "componentSlot", slot_name);
        unsafe { qjs::JS_FreeValue(ctx, slot_name) };
        r?;
        let reader = Owned(ctx, prop_reader(ctx, props, slot));
        dom::insert_value_at(ctx, holder.1, reader.1, None)?;
        dom::insert_node(ctx, node, holder.1, None)?;
        if anchor.is_none() {
            anchor = Some(holder);
        }
    }
    // A component with rows: a list of the app's data and row renderer as
    // that slot (only what the app passes: the list checks which slots
    // exist); the component lays it out and wraps the rows.
    if let Some(items) = spec.items() {
        // SAFETY: a new object with getters reading the app's props.
        let forwarded = Owned(ctx, unsafe { qjs::JS_NewObject(ctx) });
        for key in crate::list::ITEMS_PROPS {
            if !has(ctx, props, key) {
                continue;
            }
            let getter = prop_reader(ctx, props, key);
            let k = CString::new(*key).unwrap_or_default();
            unsafe {
                let atom = qjs::JS_NewAtom(ctx, k.as_ptr());
                qjs::JS_DefineProperty(
                    ctx,
                    forwarded.1,
                    atom,
                    UNDEFINED,
                    getter,
                    UNDEFINED,
                    (qjs::JS_PROP_HAS_GET | qjs::JS_PROP_HAS_ENUMERABLE | qjs::JS_PROP_ENUMERABLE)
                        as c_int,
                );
                qjs::JS_FreeAtom(ctx, atom);
                qjs::JS_FreeValue(ctx, getter);
            }
        }
        let holder = Owned(ctx, crate::list::render(ctx, forwarded.1)?);
        let slot_name = new_string(ctx, items);
        let r = dom::set_prop_value(ctx, holder.1, "componentSlot", slot_name);
        unsafe { qjs::JS_FreeValue(ctx, slot_name) };
        r?;
        dom::insert_node(ctx, node, holder.1, None)?;
        if anchor.is_none() {
            anchor = Some(holder);
        }
    }
    // The ref of a component with methods: an object calling them.
    let api = if spec.methods.is_empty() {
        None
    } else {
        // SAFETY: a new object holding the node and the methods.
        let api = Owned(ctx, unsafe { qjs::JS_NewObject(ctx) });
        unsafe {
            qjs::JS_SetPropertyStr(ctx, api.1, c"node".as_ptr(), qjs::JS_DupValue(ctx, node));
        }
        for (i, m) in spec.methods.iter().enumerate() {
            // The node, the spec's index, the method's.
            let mut mdata = [
                node,
                // SAFETY: the spec's index (a number).
                unsafe { qjs::JS_DupValue(ctx, *spec_data) },
                qjs::JS_NewFloat64(i as f64),
            ];
            let name = CString::new(m.name).unwrap_or_default();
            // SAFETY: the function keeps dups of `mdata`; the object takes it.
            unsafe {
                let f = qjs::JS_NewCFunctionData(
                    ctx,
                    Some(call_method),
                    m.params.len() as c_int,
                    0,
                    3,
                    mdata.as_mut_ptr(),
                );
                qjs::JS_FreeValue(ctx, mdata[1]);
                qjs::JS_SetPropertyStr(ctx, api.1, name.as_ptr(), f);
            }
        }
        Some(api)
    };
    let handles_events = spec
        .events
        .iter()
        .any(|e| has(ctx, props, &handler_name(e.name)));
    // Lazy children: made while this signal (the core's `$children`) is
    // true.
    let lazy = (spec.lazy_children && spec.takes_children() && has(ctx, props, "children"))
        .then(|| Owned(ctx, reactive::new_signal(ctx, qjs::JS_FALSE)));
    if handles_events || lazy.is_some() {
        let mut edata = [props, lazy.as_ref().map_or(UNDEFINED, |l| l.1)];
        // SAFETY: the function keeps dups of `edata`.
        let on = Owned(ctx, unsafe {
            qjs::JS_NewCFunctionData(ctx, Some(on_native_event), 1, 0, 2, edata.as_mut_ptr())
        });
        dom::set_handler(
            ctx,
            dom::node_id(ctx, node).unwrap_or(0),
            EventKind::NativeEvent,
            on.1,
        )?;
    }
    // One effect reads every prop and sends those that changed.
    let mut pdata = [
        node,
        props,
        // SAFETY: a new array of the previous values (filled by the runs).
        unsafe { qjs::JS_NewArray(ctx) },
        // SAFETY: the spec's index (a number).
        unsafe { qjs::JS_DupValue(ctx, *spec_data) },
    ];
    // SAFETY: the function keeps dups of `pdata`; ours are freed.
    let func = unsafe {
        let f = qjs::JS_NewCFunctionData(ctx, Some(props_run), 0, 0, 4, pdata.as_mut_ptr());
        qjs::JS_FreeValue(ctx, pdata[2]);
        f
    };
    let r = reactive::render_effect(ctx, func);
    // SAFETY: the effect's value is unused.
    unsafe { qjs::JS_FreeValue(ctx, r) };
    let ref_ = get(ctx, props, "ref");
    // SAFETY: a live value; called untracked with the ref.
    if unsafe { qjs::JS_IsFunction(ctx, ref_) } {
        let mut args = [api.as_ref().map_or(node, |a| a.1)];
        let r = reactive::untracked_with(ctx, || unsafe {
            qjs::JS_Call(ctx, ref_, UNDEFINED, 1, args.as_mut_ptr())
        });
        unsafe {
            qjs::JS_FreeValue(ctx, ref_);
            if qjs::JS_IsException(r) {
                return Err(r);
            }
            qjs::JS_FreeValue(ctx, r);
        }
    } else {
        unsafe { qjs::JS_FreeValue(ctx, ref_) };
    }
    // Before the slots' holders: clearing the children must not remove
    // them.
    if spec.takes_children() && has(ctx, props, "children") {
        let reader = Owned(ctx, prop_reader(ctx, props, "children"));
        let children = match &lazy {
            Some(shown) => {
                let c = Owned(ctx, dom::cond_value(ctx, shown.1, reader.1));
                // SAFETY: a live value.
                if unsafe { qjs::JS_IsException(c.1) } {
                    return Err(c.take());
                }
                c
            }
            None => reader,
        };
        dom::insert_value_at(ctx, node, children.1, anchor.as_ref().map(|a| a.1))?;
    }
    Ok(guard.take())
}

/// A JS value owned until dropped (or taken).
struct Owned(*mut JsContext, JsValue);

impl Owned {
    fn take(self) -> JsValue {
        let v = self.1;
        std::mem::forget(self);
        v
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: ours.
        unsafe { qjs::JS_FreeValue(self.0, self.1) };
    }
}

/// A run of a component's props effect: sends the props that changed.
unsafe extern "C" fn props_run(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: four data values (`render_component`).
    let (node, props, prev, index) = unsafe { (*data, *data.add(1), *data.add(2), data.add(3)) };
    let spec = spec_of(ctx, index);
    let keys = spec
        .props
        .iter()
        .filter(|p| p.kind == PropKind::Value)
        .map(|p| (p.name, p.decode))
        .chain(HOST_PROPS.iter().map(|&h| (h, None)));
    // Every prop is read and set whatever another one did, so each stays
    // tracked and the good ones reach the view; the first error is thrown
    // at the end (an uncatchable one, an interrupt, at once).
    let mut first: Option<V> = None;
    let failed = |first: &mut Option<V>| -> bool {
        let e = crate::web::js::take_exception(ctx);
        // SAFETY: a check of a live value.
        if unsafe { qjs::JS_IsUncatchableError(e.raw()) } {
            let _ = crate::web::js::throw(ctx, e);
            return true;
        }
        first.get_or_insert(e);
        false
    };
    for (i, (key, decode)) in keys.enumerate() {
        let value = get(ctx, props, key);
        // SAFETY: live values; the array keeps a dup of the new value.
        unsafe {
            if qjs::JS_IsException(value) {
                if failed(&mut first) {
                    return qjs::JS_EXCEPTION;
                }
                continue;
            }
            let old = qjs::JS_GetPropertyUint32(ctx, prev, i as u32);
            let same = qjs::JS_IsStrictEqual(ctx, old, value);
            let result = if same {
                Ok(())
            } else if let Some(decode) = decode {
                set_rust_prop(ctx, node, &spec, key, decode, value)
            } else {
                dom::set_prop_with_prev(ctx, node, key, value, old)
            };
            qjs::JS_FreeValue(ctx, old);
            // The previous value is what the view has: one that was not
            // sent (it did not convert) leaves it, so assigning it again
            // throws again and `undefined` resets the prop.
            if result.is_err() {
                qjs::JS_FreeValue(ctx, value);
                if failed(&mut first) {
                    return qjs::JS_EXCEPTION;
                }
                continue;
            }
            qjs::JS_SetPropertyUint32(ctx, prev, i as u32, value);
        }
    }
    match first {
        Some(e) => {
            let _ = crate::web::js::throw(ctx, e);
            qjs::JS_EXCEPTION
        }
        None => UNDEFINED,
    }
}

/// Sets prop `key` of the Rust component on `node` to `value` (borrowed),
/// converted into the prop's type now. A value of another type is a
/// warning, as a host element's is (`Gauge.level: expected ...`), and the
/// prop keeps its value (its default in the first render); `undefined`
/// resets it to its default. A getter that throws, or a decoder's panic,
/// throws.
fn set_rust_prop(
    ctx: *mut JsContext,
    node: JsValue,
    spec: &ComponentSpec,
    key: &'static str,
    decode: nsh_core::ui::PropDecode,
    value: JsValue,
) -> Result<(), JsValue> {
    let Some(id) = dom::node_id(ctx, node) else {
        return Ok(());
    };
    // SAFETY: an inline tag check.
    let decoded = if unsafe { qjs::JS_IsUndefined(value) } {
        None
    } else {
        let host = crate::classes::host_outside_calls(ctx);
        // The app's conversion: a panic in it throws, as in a call.
        let decoded = catch_unwind(AssertUnwindSafe(|| {
            decode(JsIn::new(&host, crate::classes::to_raw(value)))
        }));
        let decoded = match decoded {
            Ok(decoded) => decoded,
            Err(payload) => {
                host.clear_pending();
                let message = crate::classes::panic_message(payload);
                let _ =
                    crate::classes::throw_error(ctx, &format!("{}.{key}: {message}", spec.export));
                return Err(qjs::JS_EXCEPTION);
            }
        };
        match decoded {
            Ok(v) => {
                host.clear_pending();
                Some(v)
            }
            // A getter threw: its exception goes on.
            Err(_) if host.has_pending() => return Err(qjs::JS_EXCEPTION),
            Err(e) => {
                dom::warn(ctx, format!("{}.{key}: {e}", spec.export));
                return Ok(());
            }
        }
    };
    dom::push_op(
        ctx,
        Op::SetProp {
            id,
            prop: Prop::RustProp(key, decoded),
        },
    )
}

unsafe extern "C" fn call_method(
    ctx: *mut JsContext,
    this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: three data values (`render_component`): the node, the
    // spec's index and the method's.
    let (node, spec, index) = unsafe { (*data, data.add(1), *data.add(2)) };
    let spec = spec_of(ctx, spec);
    let mut i = 0f64;
    // SAFETY: a number (`render_component`).
    unsafe { qjs::JS_ToFloat64(ctx, &mut i, index) };
    let (Some(id), Some(method)) = (dom::node_id(ctx, node), spec.methods.get(i as usize)) else {
        return qjs::JS_EXCEPTION;
    };
    // SAFETY: QuickJS passes `argc` live arguments.
    let args = unsafe { crate::web::js::Args::new(argc, argv) };
    let make = method.call;
    // A wrong argument's error names the component (`Stepper.setValue`).
    crate::classes::send(ctx, this, args, method.returns.is_some(), |host| {
        make(host)
            .map(|call| RustCall::Method(id, call))
            .map_err(|e| match e {
                nsh_core::ui::Thrown::Type(m) => {
                    nsh_core::ui::Thrown::Type(format!("{}.{m}", spec.export))
                }
                other => other,
            })
    })
}

/// A component's events ([`deliver`]): `$children` places its lazy
/// children, others go to the `on<Event>` handler of the props.
unsafe extern "C" fn on_native_event(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values (the props, the lazy children's signal).
    let (props, lazy) = unsafe { (*data, *data.add(1)) };
    let event = arg(argc, argv, 0);
    let name = get(ctx, event, "name");
    if string_arg(ctx, name).as_deref() == Some(CHILDREN) {
        // SAFETY: ours; the signal takes the data.
        unsafe { qjs::JS_FreeValue(ctx, name) };
        if !unsafe { qjs::JS_IsUndefined(lazy) } {
            reactive::set_signal(ctx, lazy, get(ctx, event, "data"));
        }
        return UNDEFINED;
    }
    let handler = string_arg(ctx, name).map(|n| get(ctx, props, &handler_name(&n)));
    // SAFETY: values got are freed; the handler is called with the data.
    unsafe {
        qjs::JS_FreeValue(ctx, name);
        let Some(handler) = handler else {
            return qjs::JS_EXCEPTION;
        };
        if !qjs::JS_IsFunction(ctx, handler) {
            qjs::JS_FreeValue(ctx, handler);
            return UNDEFINED;
        }
        let mut args = [get(ctx, event, "data")];
        let r = qjs::JS_Call(ctx, handler, UNDEFINED, 1, args.as_mut_ptr());
        qjs::JS_FreeValue(ctx, args[0]);
        qjs::JS_FreeValue(ctx, handler);
        r
    }
}
