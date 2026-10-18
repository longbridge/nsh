//! The JS face of Rust components and modules, made from their specs
//! (`nsh_core::ui`).
//!
//! A component is a host view naming it (`component`), its own props sent
//! as `$name` when they change (one render effect reads them all), its
//! events delivered to the `on<Event>` handlers, its children inserted when
//! it takes some, and one holder per named slot the app fills. A module's
//! function without a context runs in the JS call (`classes.rs`); one with
//! a context or `async` sends a call to the core, in order with the ops
//! before it (instances and functions among its arguments held in the
//! table the core shares); one with a result returns a promise, settled by
//! the core's `$return` event.

use std::{
    cell::{Cell, RefCell},
    ffi::{CString, c_int},
    ptr,
    sync::Arc,
};

use nsh_core::{
    Command, EventKind, Op,
    ui::{ComponentSpec, ModuleSpec, PropKind},
};
use rquickjs::{Ctx, Value, qjs};
use rustc_hash::FxHashMap;

use crate::{
    dom::{self, string_arg},
    reactive::{self, arg},
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
    /// The root listens to `$return` (module calls are answered there).
    root_listens: Cell<bool>,
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

/// The JSON a call sends: `{ c, a }`, nodes in `a` as `{ $node: id }`.
fn call_args(
    ctx: *mut JsContext,
    call: u32,
    args: &[JsValue],
) -> Result<nsh_core::JsonValue, JsValue> {
    let mut a = Vec::with_capacity(args.len());
    for &v in args {
        a.push(match dom::node_id(ctx, v) {
            Some(id) => serde_json::json!({ "$node": id }),
            None => {
                // SAFETY: a context reference and a dup for the converter.
                let cx = unsafe { Ctx::from_raw(ptr::NonNull::new_unchecked(ctx)) };
                let value = unsafe { Value::from_raw(cx, qjs::JS_DupValue(ctx, v)) };
                match crate::protocol::to_json(&value) {
                    Ok(json) => json,
                    Err(_) => return Err(qjs::JS_EXCEPTION),
                }
            }
        });
    }
    Ok(serde_json::json!({ "c": call, "a": a }))
}

/// Sends call `name` to node `id` (1: a module's); with `returns`, returns
/// a promise of its result.
fn send(ctx: *mut JsContext, id: u32, name: &str, args: &[JsValue], returns: bool) -> JsValue {
    let call = if returns { next_call(ctx) } else { 0 };
    let json = match call_args(ctx, call, args) {
        Ok(json) => json,
        Err(e) => return e,
    };
    let op = Op::Command {
        id,
        command: Command::Native {
            name: name.into(),
            args: json,
        },
    };
    if let Err(e) = dom::push_op(ctx, op) {
        return e;
    }
    if !returns {
        return UNDEFINED;
    }
    promise(ctx, call, id == 1)
}

/// Sends a call of `async` member `name` (`app/native:Tally#save`) to the
/// core: `this` and the instances among `args` are ids in the table the
/// core shares (`held`: argument index and id). A promise of its result.
pub(crate) fn send_member(
    ctx: *mut JsContext,
    name: &str,
    this: Option<u64>,
    args: Vec<nsh_core::JsonValue>,
    held: &[(usize, u64)],
) -> JsValue {
    let call = next_call(ctx);
    let mut json = serde_json::json!({ "c": call, "a": args, "held": held });
    if let Some(this) = this {
        json["this"] = this.into();
    }
    let op = Op::Command {
        id: 1,
        command: Command::Native {
            name: name.into(),
            args: json,
        },
    };
    if let Err(e) = dom::push_op(ctx, op) {
        return e;
    }
    promise(ctx, call, true)
}

/// A new call number (never 0: no reply).
fn next_call(ctx: *mut JsContext) -> u32 {
    let c = components(ctx);
    let n = c.calls.get().wrapping_add(1).max(1);
    c.calls.set(n);
    n
}

/// The promise of call `call`'s result; `on_root`: answered on the root,
/// which then listens to `$return`.
fn promise(ctx: *mut JsContext, call: u32, on_root: bool) -> JsValue {
    let c = components(ctx);
    if on_root && !c.root_listens.replace(true) {
        let settle = reactive::new_function(ctx, c"settle", settle_root, 1);
        let r = dom::set_handler(ctx, 1, EventKind::NativeEvent, settle);
        // SAFETY: ours.
        unsafe { qjs::JS_FreeValue(ctx, settle) };
        if let Err(e) = r {
            return e;
        }
    }
    let mut funcs = [UNDEFINED; 2];
    // SAFETY: a new promise; the table owns its functions.
    let promise = unsafe { qjs::JS_NewPromiseCapability(ctx, funcs.as_mut_ptr()) };
    c.pending.borrow_mut().insert(call, (funcs[0], funcs[1]));
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
    promise(ctx, call, true)
}

/// Settles the call a `$return` event answers (`{ c, v }` or `{ c, e }`);
/// false for other events.
fn settle(ctx: *mut JsContext, event: JsValue) -> Result<bool, JsValue> {
    let name = get(ctx, event, "name");
    let is_return = string_arg(ctx, name).as_deref() == Some("$return");
    // SAFETY: ours.
    unsafe { qjs::JS_FreeValue(ctx, name) };
    if !is_return {
        return Ok(false);
    }
    let data = get(ctx, event, "data");
    let c = get(ctx, data, "c");
    let mut call = 0f64;
    // SAFETY: converting live values; every value got is freed.
    unsafe {
        qjs::JS_ToFloat64(ctx, &mut call, c);
        qjs::JS_FreeValue(ctx, c);
    }
    let pending = components(ctx).pending.borrow_mut().remove(&(call as u32));
    let result = match pending {
        Some((resolve, reject)) => unsafe {
            let (f, value) = if has(ctx, data, "e") {
                let message = get(ctx, data, "e");
                let error = qjs::JS_NewError(ctx);
                qjs::JS_SetPropertyStr(ctx, error, c"message".as_ptr(), message);
                (reject, error)
            } else {
                (resolve, crate::classes::reply_value(ctx, data))
            };
            let mut args = [value];
            let r = qjs::JS_Call(ctx, f, UNDEFINED, 1, args.as_mut_ptr());
            qjs::JS_FreeValue(ctx, value);
            qjs::JS_FreeValue(ctx, resolve);
            qjs::JS_FreeValue(ctx, reject);
            if qjs::JS_IsException(r) {
                Err(r)
            } else {
                qjs::JS_FreeValue(ctx, r);
                Ok(true)
            }
        },
        None => Ok(true),
    };
    // SAFETY: ours.
    unsafe { qjs::JS_FreeValue(ctx, data) };
    result
}

unsafe extern "C" fn settle_root(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    settle(ctx, arg(argc, argv, 0)).err().unwrap_or(UNDEFINED)
}

// --- Modules ------------------------------------------------------------------------

/// The name a module's function is called by (`nsh/android:Toast.show`,
/// `Alert.alert`).
fn call_name(module: &ModuleSpec, namespace: Option<&str>, function: &str) -> String {
    match namespace {
        Some(ns) => format!("{ns}:{}.{function}", module.name),
        None => format!("{}.{function}", module.name),
    }
}

/// A function of a module: one that runs in the JS call, or one sending
/// its call.
pub(crate) fn module_function(
    ctx: *mut JsContext,
    module: &ModuleSpec,
    namespace: Option<&str>,
    function: &str,
) -> JsValue {
    let spec = module.functions.iter().find(|f| f.name == function);
    if let Some((f, spec)) = spec.and_then(|s| s.sync.map(|f| (f, s))) {
        return crate::classes::sync_function(ctx, function, f, spec.params.len());
    }
    let returns = spec.is_some_and(|f| f.returns.is_some());
    let mut data = [
        new_string(ctx, &call_name(module, namespace, function)),
        if returns { qjs::JS_TRUE } else { qjs::JS_FALSE },
    ];
    // SAFETY: the function keeps dups of `data`; ours are freed.
    unsafe {
        let f = qjs::JS_NewCFunctionData(ctx, Some(call_function), 0, 0, 2, data.as_mut_ptr());
        qjs::JS_FreeValue(ctx, data[0]);
        f
    }
}

unsafe extern "C" fn call_function(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values (`module_function`) and `argc` arguments.
    let (name, returns) = unsafe { (*data, qjs::JS_ToBool(ctx, *data.add(1)) == 1) };
    let Some(name) = string_arg(ctx, name) else {
        return qjs::JS_EXCEPTION;
    };
    let args: Vec<JsValue> = (0..argc.max(0) as usize)
        .map(|i| arg(argc, argv, i))
        .collect();
    // Instances and functions cross in the table the core shares.
    let Ok((args, held)) = crate::classes::wire_args(ctx, &args) else {
        return qjs::JS_EXCEPTION;
    };
    let call = if returns { next_call(ctx) } else { 0 };
    let op = Op::Command {
        id: 1,
        command: Command::Native {
            name: name.into(),
            args: serde_json::json!({ "c": call, "a": args, "held": held }),
        },
    };
    if let Err(e) = dom::push_op(ctx, op) {
        // Not sent: what it holds goes.
        crate::classes::release(ctx, &held);
        return e;
    }
    if !returns {
        return UNDEFINED;
    }
    promise(ctx, call, true)
}

/// A module's object: its functions.
pub(crate) fn module(ctx: *mut JsContext, module: &ModuleSpec, namespace: Option<&str>) -> JsValue {
    // SAFETY: a new object taking the functions.
    let obj = unsafe { qjs::JS_NewObject(ctx) };
    add_functions(ctx, obj, module, namespace);
    obj
}

fn add_functions(ctx: *mut JsContext, obj: JsValue, module: &ModuleSpec, namespace: Option<&str>) {
    for f in &module.functions {
        let func = module_function(ctx, module, namespace, f.name);
        let name = CString::new(f.name).unwrap_or_default();
        // SAFETY: the object takes the function.
        unsafe { qjs::JS_SetPropertyStr(ctx, obj, name.as_ptr(), func) };
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
    Some(self::module(ctx, &spec, registry.namespace()))
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
        add_functions(ctx, obj, &spec, registry.namespace());
    }
}

// --- Components --------------------------------------------------------------------

/// The function rendering component `spec` (with the functions of a module
/// of the same name on it).
pub(crate) fn component(
    ctx: *mut JsContext,
    spec: Arc<ComponentSpec>,
    module: Option<(Arc<ModuleSpec>, Option<&'static str>)>,
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
    if let Some((module, namespace)) = module {
        add_functions(ctx, f, &module, namespace);
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
        for m in &spec.methods {
            let mut mdata = [
                node,
                new_string(ctx, m.name),
                if m.returns.is_some() {
                    qjs::JS_TRUE
                } else {
                    qjs::JS_FALSE
                },
            ];
            let name = CString::new(m.name).unwrap_or_default();
            // SAFETY: the function keeps dups of `mdata`; the object takes it.
            unsafe {
                let f =
                    qjs::JS_NewCFunctionData(ctx, Some(call_method), 0, 0, 3, mdata.as_mut_ptr());
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
    if handles_events || lazy.is_some() || spec.methods.iter().any(|m| m.returns.is_some()) {
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
        .map(|p| (p.name, true))
        .chain(HOST_PROPS.iter().map(|&h| (h, false)));
    for (i, (key, own)) in keys.enumerate() {
        let value = get(ctx, props, key);
        // SAFETY: live values; the array keeps a dup of the new value.
        unsafe {
            if qjs::JS_IsException(value) {
                return value;
            }
            let old = qjs::JS_GetPropertyUint32(ctx, prev, i as u32);
            let same = qjs::JS_IsStrictEqual(ctx, old, value);
            let result = if same {
                Ok(())
            } else {
                let wire = if own {
                    format!("${key}")
                } else {
                    key.to_owned()
                };
                dom::set_prop_with_prev(ctx, node, &wire, value, old)
            };
            qjs::JS_FreeValue(ctx, old);
            qjs::JS_SetPropertyUint32(ctx, prev, i as u32, value);
            if let Err(e) = result {
                return e;
            }
        }
    }
    UNDEFINED
}

unsafe extern "C" fn call_method(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: three data values (`render_component`).
    let (node, name, returns) =
        unsafe { (*data, *data.add(1), qjs::JS_ToBool(ctx, *data.add(2)) == 1) };
    let (Some(id), Some(name)) = (dom::node_id(ctx, node), string_arg(ctx, name)) else {
        return qjs::JS_EXCEPTION;
    };
    let args: Vec<JsValue> = (0..argc.max(0) as usize)
        .map(|i| arg(argc, argv, i))
        .collect();
    send(ctx, id, &name, &args, returns)
}

/// A component's native events: `$return` settles a method call, others
/// go to the `on<Event>` handler of the props.
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
    match settle(ctx, event) {
        Ok(true) => return UNDEFINED,
        Ok(false) => {}
        Err(e) => return e,
    }
    let name = get(ctx, event, "name");
    if string_arg(ctx, name).as_deref() == Some("$children") {
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
