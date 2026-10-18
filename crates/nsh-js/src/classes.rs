//! The JS face of Rust's classes (`#[class]`, `nsh_core::ui::ClassDef`):
//! one constructor and prototype per class and context, made from its
//! definition when a bundle imports its set (or a member first returns
//! one of its values).
//!
//! An object of a class is an object of the web classes' class
//! (`web/mod.rs`): its opaque is a slot holding a [`ClassObject`], the
//! `Rc` of the Rust instance, so collection, `instanceof`, `new.target`
//! and re-entrancy work as for `URL` or `Blob`. The instance remembers its
//! object while both live (one object per instance: a member returning the
//! same instance twice gives the same object). Members run in the JS call:
//! [`CallHost`] reads the call's arguments and makes its result for the
//! generated code, converting QuickJS values directly.

use std::{
    any::TypeId,
    cell::{Cell, RefCell},
    ffi::{CString, c_int, c_void},
    rc::Rc,
};

use nsh_core::{
    JsonValue,
    ui::{
        ClassDef, ClassInstance, JsHost, JsKind, JsRaw, Member, MemberCall, MemberKind, PropError,
        Thrown,
    },
};
use rquickjs::qjs;
use rustc_hash::FxHashMap;

use crate::web::{
    self, Object,
    js::{self, Ctx, R, Raw, Throw, Trace, UNDEFINED, V},
};

/// The JS objects of the live instances, by the instance's address (not
/// owned: an object leaves when it is finalized).
type Wrappers = Rc<RefCell<FxHashMap<usize, Raw>>>;

/// The classes' part of a context's state.
#[derive(Default)]
pub(crate) struct Classes {
    /// Prototype and constructor of each class made so far.
    built: RefCell<FxHashMap<TypeId, (V, V)>>,
    wrappers: Wrappers,
    /// The classes made, by the index their constructors carry.
    defs: RefCell<Vec<&'static ClassDef>>,
}

impl Classes {
    /// Frees the values held (the state is unregistered).
    pub(crate) fn free_all(&mut self) {
        self.built.borrow_mut().clear();
    }
}

fn classes<'a>(ctx: Ctx) -> &'a Classes {
    // SAFETY: a projection of the state's field, by shared reference.
    unsafe { &(*crate::state::state(ctx)).classes }
}

/// The data of an object of a Rust class: its instance.
pub(crate) struct ClassObject {
    instance: Rc<dyn ClassInstance>,
    wrappers: Wrappers,
}

/// The address an instance is known by.
fn key(instance: &Rc<dyn ClassInstance>) -> usize {
    Rc::as_ptr(instance) as *const () as usize
}

impl Drop for ClassObject {
    fn drop(&mut self) {
        // In a finalizer: the object goes, the instance may stay (Rust
        // holds it) and get another object later.
        if let Ok(mut wrappers) = self.wrappers.try_borrow_mut() {
            wrappers.remove(&key(&self.instance));
        }
    }
}

impl Trace for ClassObject {
    fn trace(&self, _mark: &mut dyn FnMut(Raw)) {}
}

impl Object for ClassObject {}

/// The instance object `v` holds (any class's), if it is one.
fn instance_of(ctx: Ctx, v: Raw) -> Option<Rc<dyn ClassInstance>> {
    web::peek::<ClassObject, _>(ctx, v, |o| o.instance.clone())
}

// --- Making the classes ----------------------------------------------------------------

/// The constructor of class `def` (made on first use).
pub(crate) fn constructor(ctx: Ctx, def: &'static ClassDef) -> R<V> {
    Ok(built(ctx, def)?.1)
}

/// Prototype and constructor of class `def`.
fn built(ctx: Ctx, def: &'static ClassDef) -> R<(V, V)> {
    let id = (def.type_id)();
    if let Some((proto, ctor)) = classes(ctx).built.borrow().get(&id) {
        return Ok((proto.clone_ref(), ctor.clone_ref()));
    }
    let (proto, ctor) = make(ctx, def)?;
    classes(ctx)
        .built
        .borrow_mut()
        .insert(id, (proto.clone_ref(), ctor.clone_ref()));
    Ok((proto, ctor))
}

/// A function calling `f` with `opaque` (a definition, `'static`).
fn native(ctx: Ctx, name: &str, f: qjs::JSCClosure, length: usize, opaque: *const c_void) -> R<V> {
    let name = CString::new(name).unwrap_or_default();
    // SAFETY: the opaque points to static data, so it needs no finalizer.
    V::checked(ctx, unsafe {
        qjs::JS_NewCClosure(
            ctx,
            f,
            name.as_ptr(),
            None,
            length as c_int,
            0,
            opaque as *mut c_void,
        )
    })
}

/// The constructor of `def`: a function QuickJS calls only with `new`
/// (`this` is then `new.target`), which finds the class by the index it
/// carries.
fn constructor_function(ctx: Ctx, def: &'static ClassDef, length: usize) -> R<V> {
    let index = {
        let mut defs = classes(ctx).defs.borrow_mut();
        defs.push(def);
        defs.len() - 1
    };
    let name = CString::new(def.name).unwrap_or_default();
    type Magic = unsafe extern "C" fn(Ctx, Raw, c_int, *mut Raw, c_int) -> Raw;
    // SAFETY: QuickJS calls a `JS_CFUNC_constructor_magic` function with the
    // magic signature (its C type is a union of the signatures).
    let f = unsafe {
        std::mem::transmute::<Magic, unsafe extern "C" fn(Ctx, Raw, c_int, *mut Raw) -> Raw>(
            construct,
        )
    };
    // SAFETY: a new constructor-only C function.
    V::checked(ctx, unsafe {
        qjs::JS_NewCFunction2(
            ctx,
            Some(f),
            name.as_ptr(),
            length as c_int,
            qjs::JSCFunctionEnum_JS_CFUNC_constructor_magic,
            index as c_int,
        )
    })
}

/// Defines method `func` as `name` on `obj` (writable, configurable).
fn define_method(ctx: Ctx, obj: Raw, name: &str, func: V) -> R<()> {
    js::define(ctx, obj, name, func, false)
}

fn make(ctx: Ctx, def: &'static ClassDef) -> R<(V, V)> {
    let proto = js::new_object(ctx)?;
    let length = def.constructor().map_or(0, |m| m.params.len());
    let ctor = constructor_function(ctx, def, length)?;
    // SAFETY: live objects: `prototype` / `constructor` link both ways.
    unsafe { qjs::JS_SetConstructor(ctx, ctor.raw(), proto.raw()) };
    // Accessors by name: getter and setter.
    let mut accessors: Vec<(&'static str, Option<V>, Option<V>)> = Vec::new();
    for member in def.members {
        let opaque = member as *const Member as *const c_void;
        match member.kind {
            MemberKind::Constructor => {}
            MemberKind::Method => {
                let f = native(ctx, member.name, Some(call), member.params.len(), opaque)?;
                define_method(ctx, proto.raw(), member.name, f)?;
            }
            MemberKind::Static => {
                let f = native(ctx, member.name, Some(call), member.params.len(), opaque)?;
                define_method(ctx, ctor.raw(), member.name, f)?;
            }
            MemberKind::Getter | MemberKind::Setter => {
                let length = usize::from(member.kind == MemberKind::Setter);
                let f = native(ctx, member.name, Some(call), length, opaque)?;
                let at = match accessors.iter().position(|a| a.0 == member.name) {
                    Some(at) => at,
                    None => {
                        accessors.push((member.name, None, None));
                        accessors.len() - 1
                    }
                };
                match member.kind {
                    MemberKind::Getter => accessors[at].1 = Some(f),
                    _ => accessors[at].2 = Some(f),
                }
            }
        }
    }
    for (name, get, set) in accessors {
        let cname = CString::new(name).unwrap_or_default();
        let (get, set) = (get.unwrap_or_default(), set.unwrap_or_default());
        // SAFETY: the property takes the functions.
        unsafe {
            let atom = qjs::JS_NewAtom(ctx, cname.as_ptr());
            qjs::JS_DefinePropertyGetSet(
                ctx,
                proto.raw(),
                atom,
                get.take(),
                set.take(),
                (qjs::JS_PROP_CONFIGURABLE | qjs::JS_PROP_ENUMERABLE) as c_int,
            );
            qjs::JS_FreeAtom(ctx, atom);
        }
    }
    // `dispose()` and `[Symbol.dispose]()`: one function.
    let dispose = native(
        ctx,
        "dispose",
        Some(dispose),
        0,
        def as *const ClassDef as *const c_void,
    )?;
    // SAFETY: the property takes its own reference.
    unsafe {
        qjs::JS_DefinePropertyValue(
            ctx,
            proto.raw(),
            qjs::JS_ATOM_Symbol_dispose as _,
            qjs::JS_DupValue(ctx, dispose.raw()),
            (qjs::JS_PROP_WRITABLE | qjs::JS_PROP_CONFIGURABLE) as c_int,
        );
    }
    define_method(ctx, proto.raw(), "dispose", dispose)?;
    let tag = js::string(ctx, def.name)?;
    // SAFETY: the property takes the string.
    unsafe {
        qjs::JS_DefinePropertyValue(
            ctx,
            proto.raw(),
            qjs::JS_ATOM_Symbol_toStringTag as _,
            tag.take(),
            qjs::JS_PROP_CONFIGURABLE as c_int,
        );
    }
    Ok((proto, ctor))
}

// --- Calls -------------------------------------------------------------------------

/// The arguments of a native call.
fn args<'a>(argc: c_int, argv: *mut Raw) -> js::Args<'a> {
    // SAFETY: QuickJS passes `argc` live arguments.
    unsafe { js::Args::new(argc, argv) }
}

unsafe extern "C" fn construct(
    ctx: Ctx,
    new_target: Raw,
    argc: c_int,
    argv: *mut Raw,
    magic: c_int,
) -> Raw {
    // QuickJS calls it with `new` only (`JS_CFUNC_constructor_magic`).
    let def = classes(ctx).defs.borrow()[magic as usize];
    let Some(member) = def.constructor() else {
        let _ = js::type_error(
            ctx,
            &format!("Illegal constructor: {} is made by its members", def.name),
        );
        return qjs::JS_EXCEPTION;
    };
    let host = CallHost::new(ctx, UNDEFINED, new_target, args(argc, argv));
    host.run(member)
}

unsafe extern "C" fn call(
    ctx: Ctx,
    this: Raw,
    argc: c_int,
    argv: *mut Raw,
    _magic: c_int,
    opaque: *mut c_void,
) -> Raw {
    // SAFETY: the opaque of members' functions is their static member.
    let member = unsafe { &*(opaque as *const Member) };
    let host = CallHost::new(ctx, this, UNDEFINED, args(argc, argv));
    host.run(member)
}

unsafe extern "C" fn dispose(
    ctx: Ctx,
    this: Raw,
    _argc: c_int,
    _argv: *mut Raw,
    _magic: c_int,
    opaque: *mut c_void,
) -> Raw {
    // SAFETY: the opaque of `dispose` is its class's static definition.
    let def = unsafe { &*(opaque as *const ClassDef) };
    // This class's objects only (a subclass's are).
    let Some(instance) = instance_of(ctx, this).filter(|i| (i.def().type_id)() == (def.type_id)())
    else {
        let _ = js::type_error(
            ctx,
            &format!(
                "{}.dispose: Illegal invocation: not a {}",
                def.name, def.name
            ),
        );
        return qjs::JS_EXCEPTION;
    };
    // The value's drop is the app's code: a panic in it throws.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| instance.dispose())) {
        Ok(Ok(())) => UNDEFINED,
        Ok(Err(message)) => {
            let _ = js::type_error(ctx, &message);
            qjs::JS_EXCEPTION
        }
        Err(payload) => {
            let _ = throw_error(ctx, &panic_message(payload));
            qjs::JS_EXCEPTION
        }
    }
}

/// Throws an `Error` with `message`.
fn throw_error(ctx: Ctx, message: &str) -> Throw {
    // SAFETY: a new error the exception takes.
    unsafe {
        let error = qjs::JS_NewError(ctx);
        if let Ok(m) = js::string(ctx, message) {
            qjs::JS_DefinePropertyValueStr(
                ctx,
                error,
                c"message".as_ptr(),
                m.take(),
                (qjs::JS_PROP_WRITABLE | qjs::JS_PROP_CONFIGURABLE) as c_int,
            );
        }
        qjs::JS_Throw(ctx, error);
    }
    Throw
}

/// The message of a panic's payload.
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(s) => (*s).to_owned(),
            Err(_) => "a panic".to_owned(),
        },
    }
}

// --- The host ------------------------------------------------------------------------

/// A QuickJS value as the core's opaque [`JsRaw`].
pub(crate) fn to_raw(v: Raw) -> JsRaw {
    const { assert!(size_of::<Raw>() <= size_of::<JsRaw>()) };
    let mut out = JsRaw([0; 2]);
    // SAFETY: `Raw` is plain data that fits (checked above).
    unsafe { std::ptr::write_unaligned(&mut out as *mut JsRaw as *mut Raw, v) };
    out
}

/// The QuickJS value of a [`JsRaw`] made by [`to_raw`].
pub(crate) fn from_raw(v: JsRaw) -> Raw {
    // SAFETY: `to_raw` wrote a `Raw` there.
    unsafe { std::ptr::read_unaligned(&v as *const JsRaw as *const Raw) }
}

/// The JS side of a call into Rust (and of conversions outside one): the
/// context, the call's `this`, `new.target` and arguments. An operation
/// that throws leaves the exception pending and fails with a `PropError`;
/// the call then rethrows it.
pub(crate) struct CallHost<'a> {
    ctx: Ctx,
    this: Raw,
    new_target: Raw,
    args: js::Args<'a>,
    /// A JS exception is pending (from an operation that threw).
    pending: Cell<bool>,
    /// The array items and fields read so far: a call reads at most
    /// [`crate::protocol::JSON_ENTRIES`], as JSON conversion does.
    entries: Cell<usize>,
}

impl<'a> CallHost<'a> {
    pub(crate) fn new(ctx: Ctx, this: Raw, new_target: Raw, args: js::Args<'a>) -> Self {
        Self {
            ctx,
            this,
            new_target,
            args,
            pending: Cell::new(false),
            entries: Cell::new(0),
        }
    }

    /// Counts `n` more entries read: an error past the budget.
    fn read(&self, n: usize) -> Result<(), PropError> {
        let total = self.entries.get().saturating_add(n);
        self.entries.set(total);
        if total > crate::protocol::JSON_ENTRIES {
            return Err(PropError(format!(
                "the value has more than {} entries",
                crate::protocol::JSON_ENTRIES
            )));
        }
        Ok(())
    }

    /// The `PropError` of an operation that threw (its exception stays
    /// pending).
    fn threw(&self) -> PropError {
        self.pending.set(true);
        PropError("a JS exception was thrown".to_owned())
    }

    fn checked(&self, r: R<V>) -> Result<JsRaw, PropError> {
        r.map(|v| to_raw(v.take())).map_err(|Throw| self.threw())
    }

    /// Runs `member` (catching a panic): its result, or the exception.
    fn run(&self, member: &'static Member) -> Raw {
        let MemberCall::Sync(f) = member.call;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self)))
            .unwrap_or_else(|payload| Err(Thrown::Error(panic_message(payload))));
        self.finish(result)
    }

    /// The JS result of a call: its value or what it throws.
    pub(crate) fn finish(&self, result: Result<JsRaw, Thrown>) -> Raw {
        let ctx = self.ctx;
        match result {
            Ok(v) => {
                if self.pending.get() {
                    // Thrown by an operation whose error the code handled.
                    let _ = js::take_exception(ctx);
                }
                from_raw(v)
            }
            Err(_) if self.pending.get() => qjs::JS_EXCEPTION,
            Err(Thrown::Type(message)) => {
                let _ = js::type_error(ctx, &message);
                qjs::JS_EXCEPTION
            }
            Err(Thrown::Error(message)) => {
                let _ = throw_error(ctx, &message);
                qjs::JS_EXCEPTION
            }
        }
    }

    /// The object of `instance` with prototype `proto`, remembered.
    fn new_object(&self, proto: Raw, instance: Rc<dyn ClassInstance>) -> Result<JsRaw, PropError> {
        let wrappers = classes(self.ctx).wrappers.clone();
        let k = key(&instance);
        let obj = web::instance_of_proto(
            self.ctx,
            proto,
            ClassObject {
                instance,
                wrappers: wrappers.clone(),
            },
        )
        .map_err(|Throw| self.threw())?;
        wrappers.borrow_mut().insert(k, obj.raw());
        Ok(to_raw(obj.take()))
    }
}

impl JsHost for CallHost<'_> {
    fn kind(&self, v: JsRaw) -> JsKind {
        let v = from_raw(v);
        // SAFETY: inline tag checks of a live value.
        unsafe {
            if qjs::JS_IsUndefined(v) {
                JsKind::Undefined
            } else if qjs::JS_IsNull(v) {
                JsKind::Null
            } else if qjs::JS_IsBool(v) {
                JsKind::Bool
            } else if qjs::JS_IsNumber(v) {
                JsKind::Number
            } else if qjs::JS_IsString(v) {
                JsKind::String
            } else if qjs::JS_IsArray(v) {
                JsKind::Array
            } else if qjs::JS_IsFunction(self.ctx, v) {
                JsKind::Function
            } else if qjs::JS_IsObject(v) {
                JsKind::Object
            } else {
                JsKind::Other
            }
        }
    }

    fn bool(&self, v: JsRaw) -> Option<bool> {
        let v = from_raw(v);
        // SAFETY: a tag check and a conversion that cannot throw.
        unsafe { qjs::JS_IsBool(v).then(|| qjs::JS_ToBool(self.ctx, v) == 1) }
    }

    fn number(&self, v: JsRaw) -> Option<f64> {
        let v = from_raw(v);
        // SAFETY: a tag check of a live value.
        if !unsafe { qjs::JS_IsNumber(v) } {
            return None;
        }
        js::to_number(self.ctx, v).ok()
    }

    fn string(&self, v: JsRaw) -> Option<String> {
        let v = from_raw(v);
        // SAFETY: a tag check of a live value.
        if !unsafe { qjs::JS_IsString(v) } {
            return None;
        }
        js::to_string(self.ctx, v).ok()
    }

    fn len(&self, v: JsRaw) -> Result<Option<usize>, PropError> {
        let v = from_raw(v);
        // SAFETY: a tag check of a live value, then its length.
        let len = unsafe {
            if !qjs::JS_IsArray(v) {
                return Ok(None);
            }
            let mut len = 0i64;
            if qjs::JS_GetLength(self.ctx, v, &mut len) < 0 {
                return Err(self.threw());
            }
            len.max(0) as usize
        };
        // The whole array counts at once: a sparse one is as long as it
        // says.
        self.read(len)?;
        Ok(Some(len))
    }

    fn index(&self, v: JsRaw, i: usize) -> Result<JsRaw, PropError> {
        // SAFETY: a property read of a live value (getters may run JS).
        let item = unsafe { qjs::JS_GetPropertyUint32(self.ctx, from_raw(v), i as u32) };
        self.checked(V::checked(self.ctx, item))
    }

    fn field(&self, v: JsRaw, key: &str) -> Result<JsRaw, PropError> {
        self.read(1)?;
        let v = from_raw(v);
        if js::is_nullish(v) {
            return Ok(to_raw(UNDEFINED));
        }
        self.checked(js::get(self.ctx, v, key))
    }

    fn to_json(&self, v: JsRaw) -> Result<JsonValue, PropError> {
        // SAFETY: a context reference and a dup for the converter.
        let cx = unsafe { rquickjs::Ctx::from_raw(std::ptr::NonNull::new_unchecked(self.ctx)) };
        let value =
            unsafe { rquickjs::Value::from_raw(cx, qjs::JS_DupValue(self.ctx, from_raw(v))) };
        crate::protocol::to_json(&value).map_err(|_| self.threw())
    }

    fn describe(&self, v: JsRaw) -> String {
        match self.kind(v) {
            JsKind::Undefined => "undefined".to_owned(),
            JsKind::Null => "null".to_owned(),
            JsKind::Bool => format!("{}", self.bool(v).unwrap_or(false)),
            JsKind::Number => match self.number(v).unwrap_or(f64::NAN) {
                n if n == f64::INFINITY => "Infinity".to_owned(),
                n if n == f64::NEG_INFINITY => "-Infinity".to_owned(),
                n => format!("{n}"),
            },
            JsKind::String => {
                let mut s = self.string(v).unwrap_or_default();
                if s.chars().count() > 40 {
                    s = s.chars().take(40).collect::<String>() + "…";
                }
                format!("{s:?}")
            }
            JsKind::Array => "an array".to_owned(),
            JsKind::Function => "a function".to_owned(),
            JsKind::Object => match instance_of(self.ctx, from_raw(v)) {
                Some(i) => format!("a {}", i.def().name),
                None => "an object".to_owned(),
            },
            JsKind::Other => "a symbol or bigint".to_owned(),
        }
    }

    fn node(&self, v: JsRaw) -> Option<nsh_core::NodeId> {
        crate::dom::node_id(self.ctx, from_raw(v))
    }

    fn instance(&self, v: JsRaw) -> Option<Rc<dyn ClassInstance>> {
        instance_of(self.ctx, from_raw(v))
    }

    fn free(&self, v: JsRaw) {
        // SAFETY: an owned value.
        unsafe { qjs::JS_FreeValue(self.ctx, from_raw(v)) };
    }

    fn undefined(&self) -> JsRaw {
        to_raw(UNDEFINED)
    }

    fn null(&self) -> JsRaw {
        to_raw(js::NULL)
    }

    fn new_bool(&self, b: bool) -> JsRaw {
        to_raw(V::bool(b).take())
    }

    fn new_number(&self, n: f64) -> JsRaw {
        to_raw(V::num(n).take())
    }

    fn new_string(&self, s: &str) -> Result<JsRaw, PropError> {
        self.checked(js::string(self.ctx, s))
    }

    fn new_array(&self, items: Vec<JsRaw>) -> Result<JsRaw, PropError> {
        let items = items
            .into_iter()
            .map(|i| V::own(self.ctx, from_raw(i)))
            .collect();
        self.checked(js::new_array(self.ctx, items))
    }

    fn new_object(&self) -> Result<JsRaw, PropError> {
        self.checked(js::new_object(self.ctx))
    }

    fn set_field(&self, obj: JsRaw, key: &str, value: JsRaw) -> Result<(), PropError> {
        let value = V::own(self.ctx, from_raw(value));
        js::define(self.ctx, from_raw(obj), key, value, true).map_err(|Throw| self.threw())
    }

    fn json(&self, v: &JsonValue) -> Result<JsRaw, PropError> {
        Ok(to_raw(crate::host::json_to_js(self.ctx, v)))
    }

    fn wrap(&self, instance: Rc<dyn ClassInstance>) -> Result<JsRaw, PropError> {
        let existing = classes(self.ctx)
            .wrappers
            .borrow()
            .get(&key(&instance))
            .copied();
        if let Some(obj) = existing {
            // SAFETY: a live object (it leaves the table when finalized).
            return Ok(to_raw(unsafe { qjs::JS_DupValue(self.ctx, obj) }));
        }
        let (proto, _) = built(self.ctx, instance.def()).map_err(|Throw| self.threw())?;
        self.new_object(proto.raw(), instance)
    }

    fn arg(&self, i: usize) -> JsRaw {
        to_raw(self.args.get(i))
    }

    fn this(&self) -> JsRaw {
        to_raw(self.this)
    }

    fn construct(&self, instance: Rc<dyn ClassInstance>) -> Result<JsRaw, PropError> {
        let (proto, _) = built(self.ctx, instance.def()).map_err(|Throw| self.threw())?;
        // `new.target`'s prototype: a subclass's objects are its own.
        let target = if js::is_object(self.new_target) {
            js::get(self.ctx, self.new_target, "prototype").map_err(|Throw| self.threw())?
        } else {
            V::undefined()
        };
        let proto = if js::is_object(target.raw()) {
            target
        } else {
            proto
        };
        self.new_object(proto.raw(), instance)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::Cell;

    use nsh_core::ui::{PropError, Registry, prelude::*};

    use crate::{Host, HostOptions};

    thread_local! {
        /// The `Tally`s dropped so far on this thread (each test runs on
        /// one, with its own host).
        pub(crate) static DROPPED: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn dropped() -> usize {
        DROPPED.with(Cell::get)
    }

    /// A tally kept in Rust.
    #[class]
    pub(crate) struct Tally {
        count: i64,
        log: Vec<String>,
        badge: Option<Instance<Token>>,
    }

    impl Drop for Tally {
        fn drop(&mut self) {
            DROPPED.with(|d| d.set(d.get() + 1));
        }
    }

    #[derive(Clone, Copy, PartialEq, Debug, PropEnum)]
    pub(crate) enum Step {
        Up,
        Down,
    }

    /// A change.
    #[derive(Clone, PartialEq, Debug, JsObject)]
    pub(crate) struct Change {
        /// By how much.
        by: i64,
        step: Option<Step>,
    }

    #[class]
    impl Tally {
        /// Starts at `start` (0 when left out).
        pub(crate) fn new(start: Option<i64>) -> Self {
            Tally {
                count: start.unwrap_or(0),
                log: Vec::new(),
                badge: None,
            }
        }

        /// Adds `by`; the new count.
        pub(crate) fn add(&mut self, by: i64) -> i64 {
            self.count += by;
            self.log.push(format!("+{by}"));
            self.count
        }

        /// Applies the changes; the log.
        pub(crate) fn apply(&mut self, changes: Vec<Change>) -> Vec<String> {
            for c in changes {
                let by = if c.step == Some(Step::Down) {
                    -c.by
                } else {
                    c.by
                };
                self.add(by);
            }
            self.log.clone()
        }

        /// The last change, as a change.
        pub(crate) fn last(&self) -> Option<Change> {
            self.log.last().map(|l| Change {
                by: l.trim_start_matches('+').parse().unwrap_or(0),
                step: Some(Step::Up),
            })
        }

        /// The count.
        #[getter]
        pub(crate) fn count(&self) -> i64 {
            self.count
        }

        #[setter]
        pub(crate) fn set_count(&mut self, count: i64) {
            self.count = count;
        }

        /// A tally from `"12"`; throws on anything else.
        pub(crate) fn parse(text: String) -> Result<Tally, String> {
            Ok(Tally::new(Some(
                text.trim()
                    .parse()
                    .map_err(|_| format!("not a number: {text}"))?,
            )))
        }

        /// Panics.
        pub(crate) fn explode(&self) {
            panic!("boom");
        }

        /// Adds `other`'s count; the new count.
        pub(crate) fn absorb(&mut self, other: &Tally) -> i64 {
            self.add(other.count)
        }

        /// Keeps `token`.
        pub(crate) fn pin(&mut self, token: Option<Instance<Token>>) {
            self.badge = token;
        }

        /// The token kept.
        #[getter]
        pub(crate) fn badge(&self) -> Option<Instance<Token>> {
            self.badge.clone()
        }

        /// Raises `token`'s id.
        pub(crate) fn bump(token: &mut Token) -> u32 {
            token.id += 1;
            token.id
        }

        /// Values whose third does not convert.
        pub(crate) fn flaky(&self) -> Vec<Flaky> {
            vec![Flaky(true), Flaky(true), Flaky(false)]
        }
    }

    /// A value that converts to an object, or fails to.
    pub(crate) struct Flaky(bool);

    impl JsValue for Flaky {
        fn ts_type(_types: &mut nsh_core::ui::TsTypes) -> String {
            "object".into()
        }

        fn from_json(_value: nsh_core::JsonValue) -> Result<Self, PropError> {
            Ok(Flaky(true))
        }

        fn to_json(&self) -> nsh_core::JsonValue {
            nsh_core::JsonValue::Null
        }

        fn to_js<'a>(
            &self,
            host: &'a dyn nsh_core::ui::JsHost,
        ) -> Result<nsh_core::ui::JsOwned<'a>, PropError> {
            if !self.0 {
                return Err(PropError("flaky".to_owned()));
            }
            Ok(nsh_core::ui::JsOwned::new(host, host.new_object()?))
        }
    }

    /// Its drop panics.
    #[class]
    pub(crate) struct Bomb;

    impl Drop for Bomb {
        fn drop(&mut self) {
            panic!("bomb drop");
        }
    }

    #[class]
    impl Bomb {
        pub(crate) fn new() -> Self {
            Bomb
        }
    }

    /// Made by its members only.
    #[class]
    pub(crate) struct Token {
        id: u32,
    }

    #[class]
    impl Token {
        /// A token.
        pub(crate) fn mint(id: u32) -> Token {
            Token { id }
        }

        #[getter]
        pub(crate) fn id(&self) -> u32 {
            self.id
        }
    }

    /// The set the tests import as `test/classes`.
    pub(crate) fn registry() -> Registry {
        let mut r = Registry::ui_set("test/classes");
        r.class::<Tally>().class::<Token>().class::<Bomb>();
        r
    }

    pub(crate) fn host() -> Host {
        Host::with_options(HostOptions {
            ui_sets: vec![registry()],
            ..HostOptions::default()
        })
        .unwrap()
    }

    /// Runs `code` (a module importing the classes); the texts it shows
    /// with `show(...)`.
    pub(crate) fn run(host: &Host, code: &str) -> Result<Vec<String>, String> {
        host.eval_source(&format!(
            "import {{ text as show }} from \"nsh:rt\";\nimport {{ Tally, Token, Bomb }} from \"test/classes\";\n{code}"
        ))
        .map_err(|e| e.message)?;
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        Ok(ops
            .into_iter()
            .filter_map(|op| match op {
                nsh_core::Op::CreateText { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect())
    }

    #[test]
    fn construct_call_get_and_set() {
        let host = host();
        let shown = run(
            &host,
            r#"
            const t = new Tally(3);
            show(String(t.add(2) + 1));
            show(String(t.count));
            t.count = 10;
            show(String(t.add(1)));
            show(String(new Tally().count));
            show(String(t instanceof Tally));
            show(Object.prototype.toString.call(t));
            show(JSON.stringify(t.apply([{ by: 2 }, { by: 5, step: "down" }])));
            show(JSON.stringify(t.last()));
            const p = Tally.parse(" 12 ");
            show(String(p instanceof Tally) + " " + p.count);
            const k = Token.mint(7);
            show(String(k.id) + " " + (k instanceof Token));
            "#,
        )
        .unwrap();
        assert_eq!(
            shown,
            [
                "6",
                "5",
                "11",
                "0",
                "true",
                "[object Tally]",
                r#"["+2","+1","+2","+-5"]"#,
                r#"{"by":-5,"step":"up"}"#,
                "true 12",
                "7 true",
            ]
        );
    }

    #[test]
    fn wrong_arguments_errors_and_panics_throw() {
        let host = host();
        let shown = run(
            &host,
            r#"
            const t = new Tally(1);
            const msg = (f) => { try { f(); return "no throw"; } catch (e) { return `${e.name}: ${e.message}`; } };
            show(msg(() => t.add("x")));
            show(msg(() => t.apply([{ by: "2" }])));
            show(msg(() => t.apply([{ step: "up" }])));
            show(msg(() => t.apply([{ by: 1, step: "sideways" }])));
            show(msg(() => Tally.parse("abc")));
            show(msg(() => t.explode()));
            show(msg(() => Tally(1)));
            show(msg(() => Tally.call({}, 6)));
            show(msg(() => Tally.call(Token, 7)));
            show(msg(() => new Token()));
            show(msg(() => Tally.prototype.add.call({}, 1)));
            show(msg(() => { t.count = "x"; }));
            show(String(t.count));
            "#,
        )
        .unwrap();
        assert_eq!(
            shown,
            [
                "TypeError: Tally.add: `by`: expected a number, got \"x\"",
                "TypeError: Tally.apply: `changes`: [0]: `by`: expected a number, got \"2\"",
                "TypeError: Tally.apply: `changes`: [0]: `by` is missing",
                "TypeError: Tally.apply: `changes`: [0]: `step`: expected one of \"up\" | \"down\", got \"sideways\"",
                "Error: not a number: abc",
                "Error: boom",
                "TypeError: must be called with new",
                "TypeError: must be called with new",
                "TypeError: must be called with new",
                "TypeError: Illegal constructor: Token is made by its members",
                "TypeError: Tally.add: Illegal invocation: not a Tally",
                "TypeError: Tally.count: `count`: expected a number, got \"x\"",
                "1",
            ]
        );
    }

    #[test]
    fn instances_are_arguments_and_results() {
        let host = host();
        let shown = run(
            &host,
            r#"
            const msg = (f) => { try { f(); return "no throw"; } catch (e) { return `${e.name}: ${e.message}`; } };
            const a = new Tally(2), b = new Tally(3);
            show(String(a.absorb(b)));
            show(msg(() => a.absorb(a)));
            const k = Token.mint(7);
            a.pin(k);
            show(String(a.badge === k) + " " + a.badge.id);
            show(String(Tally.bump(k)) + " " + a.badge.id);
            show(msg(() => a.absorb(k)));
            show(msg(() => a.pin({})));
            a.pin(Token.mint(9));
            globalThis.a = a;
            a.pin(undefined);
            show(String(a.badge));
            "#,
        )
        .unwrap();
        assert_eq!(
            shown,
            [
                "5",
                "TypeError: Tally used re-entrantly",
                "true 7",
                "8 8",
                "TypeError: Tally.absorb: `other`: expected a Tally, got a Token",
                "TypeError: Tally.pin: `token`: expected a Token, got an object",
                "null",
            ]
        );
    }

    #[test]
    fn an_instance_rust_keeps_gets_a_new_object_after_its_own_is_collected() {
        let host = host();
        run(
            &host,
            r#"
            globalThis.a = new Tally(1);
            let k = Token.mint(9);
            k.extra = "on the object";
            a.pin(k);
            k = null;
            show(String(a.badge.extra));
            "#,
        )
        .unwrap();
        host.collect_garbage();
        let shown = run(
            &host,
            "show(`${a.badge.id} ${a.badge.extra} ${a.badge === a.badge}`);",
        )
        .unwrap();
        assert_eq!(shown, ["9 undefined true"]);
    }

    #[test]
    fn integers_take_whole_numbers_in_their_range() {
        let host = host();
        let shown = run(
            &host,
            r#"
            const msg = (f) => { try { return String(f()); } catch (e) { return `${e.name}: ${e.message}`; } };
            const t = new Tally(0);
            show(msg(() => t.add(1.5)));
            show(msg(() => t.add(NaN)));
            show(msg(() => t.add(Infinity)));
            show(msg(() => t.add(2 ** 63)));
            show(msg(() => t.add(-(2 ** 53))));
            show(msg(() => Token.mint(-1)));
            show(msg(() => Token.mint(2 ** 40)));
            show(msg(() => Token.mint(4294967295).id));
            "#,
        )
        .unwrap();
        assert_eq!(
            shown,
            [
                "TypeError: Tally.add: `by`: expected an integer that fits i64, got 1.5",
                "TypeError: Tally.add: `by`: expected an integer that fits i64, got NaN",
                "TypeError: Tally.add: `by`: expected an integer that fits i64, got Infinity",
                "TypeError: Tally.add: `by`: expected an integer that fits i64, got 9223372036854776000",
                "-9007199254740992",
                "TypeError: Token.mint: `id`: expected an integer that fits u32, got -1",
                "TypeError: Token.mint: `id`: expected an integer that fits u32, got 1099511627776",
                "4294967295",
            ]
        );
    }

    #[test]
    fn a_call_reads_a_bounded_number_of_entries() {
        let host = host();
        let started = std::time::Instant::now();
        let shown = run(
            &host,
            r#"
            const t = new Tally(0);
            const sparse = [];
            sparse.length = 3e7;
            try { t.apply(sparse); show("no throw"); } catch (e) { show(`${e.name}: ${e.message}`); }
            "#,
        )
        .unwrap();
        assert_eq!(
            shown,
            ["TypeError: Tally.apply: `changes`: the value has more than 1048576 entries"]
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn a_result_that_fails_to_convert_frees_what_it_made() {
        let host = host();
        run(&host, "globalThis.t = new Tally(0);").unwrap();
        let count = |host: &Host| {
            host.collect_garbage();
            host.object_count()
        };
        let before = count(&host);
        run(
            &host,
            "for (let i = 0; i < 200; i++) { try { t.flaky(); } catch (e) {} }",
        )
        .unwrap();
        let grown = count(&host) - before;
        assert!(grown < 50, "{grown} objects more");
    }

    #[test]
    fn dispose_takes_its_own_classs_objects_only() {
        let host = host();
        let shown = run(
            &host,
            r#"
            const k = Token.mint(3);
            try { Tally.prototype.dispose.call(k); show("no throw"); } catch (e) { show(`${e.name}: ${e.message}`); }
            show(String(k.id));
            "#,
        )
        .unwrap();
        assert_eq!(
            shown,
            [
                "TypeError: Tally.dispose: Illegal invocation: not a Tally",
                "3"
            ]
        );
    }

    #[test]
    fn a_drop_that_panics_throws_on_dispose_and_is_logged_when_collected() {
        let host = host();
        let shown = run(
            &host,
            r#"
            try { new Bomb().dispose(); show("no throw"); } catch (e) { show(`${e.name}: ${e.message}`); }
            new Bomb();
            globalThis.cycle = { bomb: new Bomb() };
            cycle.self = cycle;
            cycle = null;
            "#,
        )
        .unwrap();
        assert_eq!(shown, ["Error: bomb drop"]);
        // Collected: the panics are logged, the VM goes on.
        host.collect_garbage();
        assert_eq!(
            run(&host, "show(String(new Tally(2).count));").unwrap(),
            ["2"]
        );
    }

    #[test]
    fn dispose_frees_the_value_and_the_object_throws_after() {
        let host = host();
        let before = dropped();
        let shown = run(
            &host,
            r#"
            const t = new Tally(1);
            t.dispose();
            const msg = (f) => { try { f(); return "no throw"; } catch (e) { return `${e.name}: ${e.message}`; } };
            show(msg(() => t.add(1)));
            show(msg(() => t.count));
            t.dispose();
            const u = new Tally(2);
            u[Symbol.dispose]();
            show(msg(() => u.count));
            {
                using v = new Tally(3);
                show(String(v.count));
            }
            "#,
        )
        .unwrap();
        assert_eq!(
            shown,
            [
                "TypeError: Tally is disposed",
                "TypeError: Tally is disposed",
                "TypeError: Tally is disposed",
                "3",
            ]
        );
        // Dropped at once (the objects are still in scope).
        assert_eq!(dropped(), before + 3);
    }

    #[test]
    fn collected_objects_drop_their_values() {
        let host = host();
        let before = dropped();
        let shown = run(
            &host,
            r#"
            globalThis.keep = new Tally(5);
            globalThis.cycle = { tally: new Tally(1) };
            cycle.self = cycle;
            for (let i = 0; i < 10; i++) new Tally(i).add(1);
            show(String(keep.count));
            "#,
        )
        .unwrap();
        assert_eq!(shown, ["5"]);
        assert_eq!(dropped(), before + 10, "unreferenced objects go at once");
        run(&host, "cycle = null;").unwrap();
        assert_eq!(dropped(), before + 10, "a cycle waits for the collector");
        host.collect_garbage();
        assert_eq!(dropped(), before + 11, "the cycle was collected");
        assert_eq!(run(&host, "show(String(keep.add(1)));").unwrap(), ["6"]);
        drop(host);
        assert_eq!(dropped(), before + 12, "the host's end drops the rest");
    }

    #[test]
    fn subclasses_construct_their_own_objects() {
        let host = host();
        let shown = run(
            &host,
            r#"
            class Double extends Tally {
                twice() { return this.add(this.count); }
            }
            const d = new Double(4);
            show(String(d.twice()) + " " + (d instanceof Tally) + " " + (d instanceof Double));
            "#,
        )
        .unwrap();
        assert_eq!(shown, ["8 true true"]);
    }
}
