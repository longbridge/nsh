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
//! same instance twice gives the same object). Members that are not
//! `async` run in the JS call: [`CallHost`] reads the call's arguments and
//! makes its result for the generated code, converting QuickJS values
//! directly. An `async` member converts `this` and its arguments in the
//! JS call too, into the start of its task, which an op carries to the
//! core ([`send`]; a module's functions with a context or `async`, and
//! components' methods, cross the same way): the instances ride in the
//! closure. The result comes back boxed in the core's events and is
//! converted when its promise settles (`components.rs`).
//!
//! A JS function Rust takes is a [`JsCallback`]: a key into the context's
//! table of functions ([`Functions`], freed with the context), called on
//! the spot inside a synchronous call, or queued in the engine's post
//! queue ([`nsh_core::ui::Posts`]) and run by [`run_posted`] in its next
//! turn. An instance whose only handle is its object reports the
//! callbacks it keeps from the object's mark hook, so the collector sees
//! through them. While a finalizer drops Rust values (QuickJS is
//! collecting garbage, `web::finalizing`), a callback runs no JS: `call`
//! fails and `post` queues without an op.

use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    ffi::{CString, c_int, c_void},
    rc::Rc,
};

use nsh_core::{
    JsonValue,
    ui::{
        CallbackHandle, ClassDef, ClassInstance, Exception, JsCall, JsHost, JsIn, JsKind, JsRaw,
        Member, MemberCall, MemberKind, Posts, PropError, RustCall, RustOp, SyncCall, Thrown,
        Tracer,
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
pub(crate) struct Classes {
    /// Prototype and constructor of each class made so far.
    built: RefCell<FxHashMap<TypeId, (V, V)>>,
    wrappers: Wrappers,
    /// The classes made, by the index their constructors carry.
    defs: RefCell<Vec<&'static ClassDef>>,
    /// The engine's queue of the JS calls Rust posts.
    posts: Posts,
    /// The JS functions callbacks hold.
    functions: Rc<Functions>,
    /// Synchronous calls from JS running now (a callback is called on the
    /// spot only inside one).
    depth: Cell<u32>,
    /// An uncatchable exception (the CPU budget's interrupt, running out of
    /// memory) a callback ended with: the synchronous call around it
    /// rethrows it, whatever Rust returned.
    uncatchable: RefCell<Option<V>>,
}

impl Classes {
    pub(crate) fn new(posts: Posts) -> Self {
        Self {
            built: RefCell::default(),
            wrappers: Wrappers::default(),
            defs: RefCell::default(),
            posts,
            functions: Rc::default(),
            depth: Cell::new(0),
            uncatchable: RefCell::default(),
        }
    }

    /// Frees the values held (the state is unregistered).
    pub(crate) fn free_all(&mut self) {
        self.built.borrow_mut().clear();
        self.functions.free_all();
        self.uncatchable.borrow_mut().take();
        // Queued calls are dropped (their replies say JS stopped).
        self.posts.close();
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
        // The last owner: the value drops after this. The functions of the
        // callbacks it keeps alone may be freed in the same collection, so
        // they are released first: its drop calling or posting one finds
        // it gone instead of a freed function.
        if Rc::strong_count(&self.instance) == 1 {
            self.instance.trace(&mut Tracer::new(&mut |handle| {
                if let Some(callback) = (handle as &dyn Any).downcast_ref::<JsCallback>() {
                    callback.release();
                }
            }));
        }
    }
}

/// The callbacks the instance keeps, when the object is its only owner
/// (otherwise a task or another instance keeps them alive, and they are
/// roots).
impl Trace for ClassObject {
    fn trace(&self, mark: &mut dyn FnMut(Raw)) {
        if Rc::strong_count(&self.instance) != 1 {
            return;
        }
        self.instance.trace(&mut Tracer::new(&mut |handle| {
            if let Some(callback) = (handle as &dyn Any).downcast_ref::<JsCallback>()
                && let Some(v) = callback.peek()
            {
                mark(v);
            }
        }));
    }
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

/// A function calling `f` with `def` (static, so it needs no finalizer)
/// and `magic` (a member's index).
fn native(
    ctx: Ctx,
    name: &str,
    f: qjs::JSCClosure,
    length: usize,
    def: &'static ClassDef,
    magic: usize,
) -> R<V> {
    let name = CString::new(name).unwrap_or_default();
    // QuickJS keeps a closure's magic in 16 unsigned bits.
    let Ok(magic) = u16::try_from(magic) else {
        return Err(js::range_error(
            ctx,
            &format!("{}: a class has at most {} members", def.name, u16::MAX),
        ));
    };
    // SAFETY: the opaque points to static data.
    V::checked(ctx, unsafe {
        qjs::JS_NewCClosure(
            ctx,
            f,
            name.as_ptr(),
            None,
            length as c_int,
            c_int::from(magic),
            def as *const ClassDef as *mut c_void,
        )
    })
}

/// The constructor of `def`: a function QuickJS calls only with `new`
/// (`this` is then `new.target`), which finds the class by the index it
/// carries.
fn constructor_function(ctx: Ctx, def: &'static ClassDef, length: usize) -> R<V> {
    // QuickJS keeps a C function's magic in 16 signed bits: an index past
    // them would name another class.
    let index = {
        let mut defs = classes(ctx).defs.borrow_mut();
        let Ok(index) = i16::try_from(defs.len()) else {
            drop(defs);
            return Err(js::range_error(
                ctx,
                &format!(
                    "{}: a context makes at most {} Rust classes",
                    def.name,
                    i16::MAX as usize + 1
                ),
            ));
        };
        defs.push(def);
        index
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
            c_int::from(index),
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
    for (index, member) in def.members.iter().enumerate() {
        match member.kind {
            MemberKind::Constructor => {}
            MemberKind::Method => {
                let f = native(
                    ctx,
                    member.name,
                    Some(call),
                    member.params.len(),
                    def,
                    index,
                )?;
                define_method(ctx, proto.raw(), member.name, f)?;
            }
            MemberKind::Static => {
                let f = native(
                    ctx,
                    member.name,
                    Some(call),
                    member.params.len(),
                    def,
                    index,
                )?;
                define_method(ctx, ctor.raw(), member.name, f)?;
            }
            MemberKind::Getter | MemberKind::Setter => {
                let length = usize::from(member.kind == MemberKind::Setter);
                let f = native(ctx, member.name, Some(call), length, def, index)?;
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
    let dispose = native(ctx, "dispose", Some(dispose), 0, def, 0)?;
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
    magic: c_int,
    opaque: *mut c_void,
) -> Raw {
    // SAFETY: the opaque of members' functions is their class's static
    // definition, the magic the member's index.
    let def = unsafe { &*(opaque as *const ClassDef) };
    let member = &def.members[magic as usize];
    let host = CallHost::new(ctx, this, UNDEFINED, args(argc, argv));
    host.run(member)
}

/// A function running module function `f` in the JS call (owned, or the
/// exception).
pub(crate) fn sync_function(ctx: Ctx, name: &str, f: SyncCall, length: usize) -> Raw {
    let cname = CString::new(name).unwrap_or_default();
    // SAFETY: the opaque is the function's code (a function pointer, as
    // large as a data pointer on every target), which needs no finalizer.
    unsafe {
        qjs::JS_NewCClosure(
            ctx,
            Some(call_sync),
            cname.as_ptr(),
            None,
            length as c_int,
            0,
            f as *const () as *mut c_void,
        )
    }
}

unsafe extern "C" fn call_sync(
    ctx: Ctx,
    this: Raw,
    argc: c_int,
    argv: *mut Raw,
    _magic: c_int,
    opaque: *mut c_void,
) -> Raw {
    // SAFETY: `sync_function` made the opaque from a `SyncCall`.
    let f = unsafe { std::mem::transmute::<*const (), SyncCall>(opaque as *const ()) };
    CallHost::new(ctx, this, UNDEFINED, args(argc, argv)).run_sync(f)
}

/// Sends a call into Rust that runs as an op, with the call's `this` and
/// `args`: `make` converts them in this call into what the op runs (the
/// instances and callbacks among them ride in it). With `promise`, a
/// promise of its result, rejected when they do not convert; else (a
/// call returning `()`) `undefined`, a conversion error throws, and the
/// op asks for no reply.
pub(crate) fn send(
    ctx: Ctx,
    this: Raw,
    args: js::Args<'_>,
    promise: bool,
    make: impl FnOnce(&dyn JsHost) -> Result<RustCall, Thrown>,
) -> Raw {
    CallHost::new(ctx, this, UNDEFINED, args).send(promise, make)
}

/// The pending exception as a rejected promise (an uncatchable one, an
/// interrupt, stays thrown).
fn rejected_with_exception(ctx: Ctx) -> Raw {
    let e = js::take_exception(ctx);
    // SAFETY: a check of a live value.
    if unsafe { qjs::JS_IsUncatchableError(e.raw()) } {
        let _ = js::throw(ctx, e);
        return qjs::JS_EXCEPTION;
    }
    match js::rejected(ctx, e.raw()) {
        Ok(p) => p.take(),
        Err(Throw) => qjs::JS_EXCEPTION,
    }
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
pub(crate) fn throw_error(ctx: Ctx, message: &str) -> Throw {
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
pub(crate) fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
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

    /// Runs `member` (catching a panic): its result, or the exception; an
    /// `async` one's promise.
    fn run(&self, member: &'static Member) -> Raw {
        match member.call {
            MemberCall::Sync(f) => self.run_sync(f),
            MemberCall::Async(start) => self.send(true, |host| start(host).map(RustCall::Task)),
        }
    }

    /// Runs `f` in this call (catching a panic): its result, or the
    /// exception.
    fn run_sync(&self, f: SyncCall) -> Raw {
        let depth = &classes(self.ctx).depth;
        depth.set(depth.get() + 1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self)))
            .unwrap_or_else(|payload| Err(Thrown::Error(panic_message(payload))));
        let depth = &classes(self.ctx).depth;
        depth.set(depth.get() - 1);
        self.finish(result)
    }

    /// Sends the call `make` converts this call's values into as an op
    /// ([`send`]).
    fn send(
        &self,
        promise: bool,
        make: impl FnOnce(&dyn JsHost) -> Result<RustCall, Thrown>,
    ) -> Raw {
        let ctx = self.ctx;
        // The app's conversions: a panic in one fails the call.
        let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| make(self)))
            .unwrap_or_else(|payload| Err(Thrown::Error(panic_message(payload))));
        let run = match made {
            Ok(run) => {
                if self.pending.get() {
                    // Thrown by an operation whose error the code handled.
                    let _ = js::take_exception(ctx);
                }
                run
            }
            Err(e) => {
                // Thrown (or rethrown) as a synchronous call's error, then
                // taken as the promise's.
                let thrown = self.finish(Err(e));
                return if promise {
                    rejected_with_exception(ctx)
                } else {
                    thrown
                };
            }
        };
        // 0: no promise waits, so the core sends no reply.
        let call = if promise {
            crate::components::next_call(ctx)
        } else {
            0
        };
        // Not queued (over the op limit): the call and what it holds go
        // here, before any promise is made.
        if let Err(e) = crate::dom::push_op(ctx, nsh_core::Op::Rust(RustOp { call, run })) {
            return e;
        }
        if promise {
            crate::components::promise(ctx, call)
        } else {
            UNDEFINED
        }
    }

    /// Whether an operation threw (its exception is pending).
    pub(crate) fn has_pending(&self) -> bool {
        self.pending.get()
    }

    /// Clears the exception an operation that failed left pending (its
    /// error was handled).
    pub(crate) fn clear_pending(&self) {
        if self.pending.replace(false) {
            let _ = js::take_exception(self.ctx);
        }
    }

    /// The JS result of a call: its value or what it throws.    /// The JS result of a call: its value or what it throws.
    pub(crate) fn finish(&self, result: Result<JsRaw, Thrown>) -> Raw {
        let ctx = self.ctx;
        // A callback was interrupted: JS must not go on, whatever Rust
        // made of its `Err`.
        let interrupted = classes(ctx).uncatchable.borrow_mut().take();
        if let Some(e) = interrupted {
            if let Ok(v) = result {
                self.free(v);
            }
            if self.pending.get() {
                let _ = js::take_exception(ctx);
            }
            let _ = js::throw(ctx, e);
            return qjs::JS_EXCEPTION;
        }
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

    fn style(&self, v: JsRaw) -> Result<nsh_core::StylePatch, PropError> {
        // SAFETY: a context reference and a dup for the decoder.
        let cx = unsafe { rquickjs::Ctx::from_raw(std::ptr::NonNull::new_unchecked(self.ctx)) };
        let value =
            unsafe { rquickjs::Value::from_raw(cx, qjs::JS_DupValue(self.ctx, from_raw(v))) };
        // Decoding may run JS (getters): no borrow is held meanwhile.
        let mut warnings = crate::protocol::Warnings::deferred();
        let decoded = crate::protocol::style_patch(&value, &mut warnings);
        crate::dom::merge_warnings(self.ctx, warnings);
        decoded.map_err(|_| self.threw())
    }

    fn node(&self, v: JsRaw) -> Option<nsh_core::NodeId> {
        crate::dom::node_id(self.ctx, from_raw(v))
    }

    fn node_value(&self, id: nsh_core::NodeId) -> JsRaw {
        to_raw(crate::dom::node_value(self.ctx, id))
    }

    fn instance(&self, v: JsRaw) -> Option<Rc<dyn ClassInstance>> {
        instance_of(self.ctx, from_raw(v))
    }

    fn callback(&self, v: JsRaw) -> Option<Rc<dyn CallbackHandle>> {
        let v = from_raw(v);
        js::is_function(self.ctx, v).then(|| new_callback(self.ctx, v))
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

// --- Callbacks ----------------------------------------------------------------------

/// The JS functions Rust's callbacks hold, by key: one reference each,
/// freed when the callback goes or, with whatever Rust still holds, with
/// the context.
#[derive(Default)]
pub(crate) struct Functions {
    /// Null once the context is gone.
    ctx: Cell<Ctx>,
    values: RefCell<FxHashMap<u64, Raw>>,
    next: Cell<u64>,
}

impl Functions {
    /// Frees the functions (the context goes): callbacks Rust still holds
    /// do nothing after.
    fn free_all(&self) {
        let ctx = self.ctx.replace(std::ptr::null_mut());
        let values: Vec<Raw> = self.values.borrow_mut().drain().map(|(_, v)| v).collect();
        if !ctx.is_null() {
            for v in values {
                // SAFETY: references the table owned.
                unsafe { qjs::JS_FreeValue(ctx, v) };
            }
        }
    }
}

/// A JS function as Rust keeps it.
pub(crate) struct JsCallback {
    functions: Rc<Functions>,
    key: u64,
    posts: Posts,
}

impl JsCallback {
    /// The function and its context, while the context lives.
    fn value(&self) -> Option<(Ctx, Raw)> {
        let ctx = self.functions.ctx.get();
        if ctx.is_null() {
            return None;
        }
        let v = *self.functions.values.borrow().get(&self.key)?;
        Some((ctx, v))
    }

    /// The function, for the collector (nothing while the table is busy).
    fn peek(&self) -> Option<Raw> {
        self.functions
            .values
            .try_borrow()
            .ok()?
            .get(&self.key)
            .copied()
    }
}

impl Drop for JsCallback {
    fn drop(&mut self) {
        self.release();
    }
}

impl JsCallback {
    /// Lets go of the function (calls after find it gone).
    fn release(&self) {
        let ctx = self.functions.ctx.get();
        // Taken out first: freeing it may finalize objects whose callbacks
        // leave the table too.
        let v = self
            .functions
            .values
            .try_borrow_mut()
            .ok()
            .and_then(|mut values| values.remove(&self.key));
        if let Some(v) = v
            && !ctx.is_null()
        {
            // SAFETY: the reference the table owned.
            unsafe { qjs::JS_FreeValue(ctx, v) };
        }
    }
}

impl CallbackHandle for JsCallback {
    fn call_now(&self, call: JsCall) {
        let Some((ctx, _)) = self.value() else {
            // The function or the VM is gone: the call is dropped (its
            // reply says so).
            return;
        };
        if web::finalizing() {
            // A drop in a finalizer: QuickJS is collecting garbage, and no
            // JS may run in it.
            call.reply(Err(
                "a JS function cannot be called while JS collects garbage (a value dropped \
                 by the collector): use `post`"
                    .to_owned()
                    .into(),
            ));
            return;
        }
        // SAFETY: reading the context's opaque pointer.
        if crate::state::state(ctx).is_null() {
            // The VM is being torn down.
            return;
        }
        assert!(
            classes(ctx).depth.get() > 0,
            "`Callback::call` runs the function inside a synchronous call from JS; \
             elsewhere (in a task) use `post` or `call_async`"
        );
        run_call(ctx, call);
    }

    fn post(&self, call: JsCall) {
        // Posted inside a synchronous call from JS: its place in the queue
        // waits for an op after the ones JS sent before, so it runs after
        // what they post. Not from a finalizer: an op is a JS value's
        // work, and QuickJS is collecting garbage (it queues as it is).
        if !web::finalizing()
            && let Some((ctx, _)) = self.value()
            // SAFETY: reading the context's opaque pointer.
            && !crate::state::state(ctx).is_null()
            && classes(ctx).depth.get() > 0
        {
            let place = self.posts.park(call);
            let op = nsh_core::Op::Rust(RustOp {
                call: 0,
                run: RustCall::Post(place),
            });
            if crate::dom::push_op(ctx, op).is_err() {
                // Over the op limit (the VM stops): the dropped op released
                // the place.
                let _ = js::take_exception(ctx);
            }
            return;
        }
        self.posts.post(call);
    }
}

/// A callback holding JS function `v`.
fn new_callback(ctx: Ctx, v: Raw) -> Rc<dyn CallbackHandle> {
    let c = classes(ctx);
    let functions = c.functions.clone();
    functions.ctx.set(ctx);
    let key = functions.next.get() + 1;
    functions.next.set(key);
    // SAFETY: the table owns the new reference.
    let v = unsafe { qjs::JS_DupValue(ctx, v) };
    functions.values.borrow_mut().insert(key, v);
    Rc::new(JsCallback {
        functions,
        key,
        posts: c.posts.clone(),
    })
}

/// The message of what JS threw.
fn message_of(ctx: Ctx, e: Raw) -> String {
    if js::is_object(e)
        && let Ok(m) = js::get(ctx, e, "message")
        && let Ok(Some(m)) = js::opt_string(ctx, m.raw())
    {
        return m;
    }
    let message = js::to_string(ctx, e).unwrap_or_else(|_| "an exception".to_owned());
    let _ = js::take_exception(ctx);
    message
}

/// What `e`, thrown, is: its message and, for an `Error`, its stack.
fn thrown_of(ctx: Ctx, e: Raw) -> Exception {
    let stack = js::is_object(e)
        .then(|| js::get(ctx, e, "stack").ok())
        .flatten()
        .and_then(|s| js::opt_string(ctx, s.raw()).ok().flatten())
        .filter(|s| !s.is_empty());
    let _ = js::take_exception(ctx);
    Exception {
        message: message_of(ctx, e),
        stack,
    }
}

/// A host for conversions outside a call.
pub(crate) fn host_outside_calls<'a>(ctx: Ctx) -> CallHost<'a> {
    CallHost::new(ctx, UNDEFINED, UNDEFINED, args(0, std::ptr::null_mut()))
}

/// Runs `call` of a callback of this context: its reply gets the result
/// (a promise's, when it settles) or the message of what it threw; without
/// a reply, what it throws is reported as uncaught.
pub(crate) fn run_call(ctx: Ctx, mut call: JsCall) {
    let callback = call.callback().clone();
    let function = (callback.as_ref() as &dyn Any)
        .downcast_ref::<JsCallback>()
        .and_then(JsCallback::value)
        .filter(|(c, _)| *c == ctx);
    let Some((_, function)) = function else {
        call.reply(Err("the function belongs to another VM".to_owned().into()));
        return;
    };
    let function = V::dup(ctx, function);
    let host = host_outside_calls(ctx);
    let args = match call.args(&host) {
        Ok(args) => args
            .into_iter()
            .map(|a| V::own(ctx, from_raw(a.into_raw())))
            .collect::<Vec<V>>(),
        Err(e) => {
            if host.pending.get() {
                let _ = js::take_exception(ctx);
            }
            call.reply(Err(e.0.into()));
            return;
        }
    };
    let raws: Vec<Raw> = args.iter().map(V::raw).collect();
    match js::call(ctx, function.raw(), UNDEFINED, &raws) {
        Ok(result) => {
            if !call.wants_reply() {
                return;
            }
            // SAFETY: a tag check of a live value.
            if call.settles() && unsafe { qjs::JS_IsPromise(result.raw()) } {
                let waiting = Rc::new(RefCell::new(Some(call)));
                let rejected = waiting.clone();
                let reacted = js::upon(
                    ctx,
                    result.raw(),
                    Vec::new(),
                    move |ctx, _, value| {
                        if let Some(call) = waiting.borrow_mut().take() {
                            let host = host_outside_calls(ctx);
                            call.reply(Ok(JsIn::new(&host, to_raw(value))));
                            // What converting the value threw (the reply has
                            // the error).
                            if host.pending.get() {
                                let _ = js::take_exception(ctx);
                            }
                        }
                        Ok(())
                    },
                    move |ctx, _, reason| {
                        if let Some(call) = rejected.borrow_mut().take() {
                            call.reply(Err(thrown_of(ctx, reason)));
                        }
                        Ok(())
                    },
                );
                if reacted.is_err() {
                    js::report_exception(ctx);
                }
                return;
            }
            call.reply(Ok(JsIn::new(&host, to_raw(result.raw()))));
            if host.pending.get() {
                let _ = js::take_exception(ctx);
            }
        }
        Err(Throw) => {
            let e = js::take_exception(ctx);
            // SAFETY: a check of a live value.
            if unsafe { qjs::JS_IsUncatchableError(e.raw()) } {
                // Kept for the call around it to rethrow.
                let message = message_of(ctx, e.raw());
                classes(ctx).uncatchable.borrow_mut().get_or_insert(e);
                call.reply(Err(message.into()));
            } else if call.wants_reply() {
                call.reply(Err(thrown_of(ctx, e.raw())));
            } else {
                js::report(ctx, e);
            }
        }
    }
}

/// Runs the JS calls Rust queued ([`nsh_core::ui::Callback::post`],
/// `call_async`), in order. Those queued meanwhile wait for the next turn.
/// An uncatchable exception (an interrupt) stops them: it is pending when
/// this fails, and the calls left are dropped.
pub(crate) fn run_posted(ctx: Ctx) -> R<()> {
    let calls = classes(ctx).posts.take_calls();
    for call in calls {
        // The app's conversions (the arguments, the result) run here, with
        // nothing around to catch a panic: it is logged, and the call
        // dropped (its reply says so).
        if let Err(payload) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_call(ctx, call)))
        {
            let _ = js::take_exception(ctx);
            log::error!(target: "js", "a posted call panicked: {}", panic_message(payload));
        }
        let interrupted = classes(ctx).uncatchable.borrow_mut().take();
        if let Some(e) = interrupted {
            return Err(js::throw(ctx, e));
        }
    }
    Ok(())
}

/// Whether JS calls Rust queued are waiting.
pub(crate) fn has_posted(ctx: Ctx) -> bool {
    classes(ctx).posts.has_calls()
}

/// No JS runs any more: the queued calls are dropped, and so are those
/// posted later (their replies say JS stopped).
pub(crate) fn close(ctx: Ctx) {
    classes(ctx).posts.close();
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{cell::Cell, rc::Rc};

    use nsh_core::ui::{Answer, PropError, Registry, prelude::*};

    use crate::{Host, HostOptions};

    thread_local! {
        /// The `Tally`s dropped so far on this thread (each test runs on
        /// one, with its own host).
        pub(crate) static DROPPED: Cell<usize> = const { Cell::new(0) };
    }

    thread_local! {
        /// The callbacks `Tally.keep` keeps.
        static KEPT: std::cell::RefCell<Vec<Callback>> = const { std::cell::RefCell::new(Vec::new()) };
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
        on_change: Option<Callback<i64>>,
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
                on_change: None,
            }
        }

        /// Adds `by`; the new count.
        pub(crate) fn add(&mut self, by: i64) -> i64 {
            self.count += by;
            self.log.push(format!("+{by}"));
            if let Some(listener) = &self.on_change {
                let _ = listener.call(self.count);
            }
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

        /// Calls `listener` with each new count.
        pub(crate) fn watch(&mut self, listener: Option<Callback<i64>>) {
            self.on_change = listener;
        }

        /// Tells the listener the count in the next turn.
        pub(crate) fn announce(&self) {
            if let Some(listener) = &self.on_change {
                listener.post(self.count);
            }
        }

        /// `items` sorted by `compare`.
        pub(crate) fn sort(
            mut items: Vec<String>,
            compare: Callback<(String, String), f64>,
        ) -> Result<Vec<String>, String> {
            let mut failed = None;
            items.sort_by(|a, b| match compare.call((a.clone(), b.clone())) {
                Ok(order) => order.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal),
                Err(e) => {
                    failed.get_or_insert(e);
                    std::cmp::Ordering::Equal
                }
            });
            match failed {
                Some(e) => Err(e),
                None => Ok(items),
            }
        }

        /// Keeps `callback` until [`Tally::forget`] (a root).
        pub(crate) fn keep(callback: Callback) {
            KEPT.with(|k| k.borrow_mut().push(callback));
        }

        /// Drops the kept callbacks.
        pub(crate) fn forget() {
            KEPT.with(|k| k.borrow_mut().clear());
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

        fn from_js(_value: nsh_core::ui::JsIn<'_>) -> Result<Self, PropError> {
            Ok(Flaky(true))
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

    thread_local! {
        /// What `Tools.listen` keeps.
        static LISTENER: std::cell::RefCell<Option<Callback<String>>> = const { std::cell::RefCell::new(None) };
    }

    /// Functions JS calls.
    #[module]
    pub(crate) struct Tools;

    #[module]
    impl Tools {
        /// The sum, at once.
        pub(crate) fn sum(values: Vec<f64>) -> f64 {
            values.iter().sum()
        }

        /// `text` as a number; throws on anything else.
        pub(crate) fn parse(text: String) -> Result<i64, String> {
            text.parse().map_err(|_| format!("not a number: {text}"))
        }

        /// Keeps `listener`.
        pub(crate) fn listen(listener: Callback<String>) {
            LISTENER.with(|l| *l.borrow_mut() = Some(listener));
        }

        /// Tells the listener `text` in the next turn.
        pub(crate) fn tell(text: String) {
            LISTENER.with(|l| {
                if let Some(listener) = &*l.borrow() {
                    listener.post(text);
                }
            });
        }

        /// Makes a signal (outside the reactive graph: it panics).
        pub(crate) fn signal_here() {
            let _ = signal(0);
        }

        /// A tally at `count`.
        pub(crate) fn tally(count: i64) -> Instance<Tally> {
            Instance::new(Tally::new(Some(count)))
        }

        /// An op (a promise of the length).
        pub(crate) fn measure(text: String, cx: &mut ModuleCx) -> usize {
            let _ = cx;
            text.len()
        }

        /// An op taking an instance.
        pub(crate) fn keep_op(tally: Instance<Tally>, cx: &mut ModuleCx) {
            let _ = (tally, cx);
        }

        /// Posts a value whose conversion panics to `f`.
        pub(crate) fn post_brittle(f: Callback<Brittle>) {
            f.post(Brittle);
        }
    }

    /// A value whose conversion to JS panics.
    pub(crate) struct Brittle;

    impl JsValue for Brittle {
        fn ts_type(_types: &mut nsh_core::ui::TsTypes) -> String {
            "object".into()
        }

        fn from_js(_value: nsh_core::ui::JsIn<'_>) -> Result<Self, PropError> {
            Ok(Brittle)
        }

        fn to_js<'a>(
            &self,
            _host: &'a dyn nsh_core::ui::JsHost,
        ) -> Result<nsh_core::ui::JsOwned<'a>, PropError> {
            panic!("brittle");
        }
    }

    /// A value whose drop panics.
    pub(crate) struct Grenade;

    impl Drop for Grenade {
        fn drop(&mut self) {
            panic!("grenade");
        }
    }

    impl JsValue for Grenade {
        fn ts_type(_types: &mut nsh_core::ui::TsTypes) -> String {
            "object".into()
        }

        fn from_js(_value: nsh_core::ui::JsIn<'_>) -> Result<Self, PropError> {
            Ok(Grenade)
        }

        fn to_js<'a>(
            &self,
            host: &'a dyn nsh_core::ui::JsHost,
        ) -> Result<nsh_core::ui::JsOwned<'a>, PropError> {
            Ok(nsh_core::ui::JsOwned::new(host, host.null()))
        }
    }

    /// An event whose data panics as it converts.
    struct BrittleEvent;

    impl nsh_core::ui::EventToJs for BrittleEvent {
        fn name(&self) -> &'static str {
            "brittle"
        }

        fn data_js<'a>(
            &self,
            _host: &'a dyn nsh_core::ui::JsHost,
        ) -> Result<nsh_core::ui::JsOwned<'a>, PropError> {
            panic!("brittle event");
        }
    }

    thread_local! {
        /// What `Asker.ask` awaits; what `Noisy`'s drops saw.
        static ANSWERS: std::cell::RefCell<Vec<Answer<Result<i64, String>>>> =
            const { std::cell::RefCell::new(Vec::new()) };
        static NOISE: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
        static WATCHERS: Cell<usize> = const { Cell::new(0) };
    }

    /// A value whose reading panics.
    pub(crate) struct Panicky;

    impl JsValue for Panicky {
        fn ts_type(_types: &mut nsh_core::ui::TsTypes) -> String {
            "unknown".into()
        }

        fn from_js(_value: nsh_core::ui::JsIn<'_>) -> Result<Self, PropError> {
            panic!("panicky");
        }

        fn to_js<'a>(
            &self,
            host: &'a dyn nsh_core::ui::JsHost,
        ) -> Result<nsh_core::ui::JsOwned<'a>, PropError> {
            Ok(nsh_core::ui::JsOwned::new(host, host.undefined()))
        }
    }

    /// Calls JS functions later.
    #[class]
    pub(crate) struct Asker;

    #[class]
    impl Asker {
        /// Asks `question` in the next turn; the answer kept for the test.
        pub(crate) fn ask(question: Callback<(), i64>) {
            let answer = question.call_async(());
            ANSWERS.with(|a| a.borrow_mut().push(answer));
        }

        /// Asks for a change in the next turn.
        pub(crate) fn ask_change(question: Callback<(), Change>) {
            drop(question.call_async(()));
        }

        /// Asks for a value whose reading panics.
        pub(crate) fn ask_panicky(question: Callback<(), Panicky>) {
            drop(question.call_async(()));
        }

        /// Calls `f` at once, and goes on whatever it did.
        pub(crate) fn call_through(f: Callback<(), i64>) -> i64 {
            f.call(()).unwrap_or(-1)
        }
    }

    /// Calls and posts its callback as it drops.
    #[class]
    pub(crate) struct Noisy {
        on_drop: Callback<String>,
    }

    impl Drop for Noisy {
        fn drop(&mut self) {
            let called = self.on_drop.call("drop".to_owned());
            NOISE.with(|n| n.borrow_mut().push(format!("{called:?}")));
            self.on_drop.post("posted".to_owned());
        }
    }

    #[class]
    impl Noisy {
        pub(crate) fn new(on_drop: Callback<String>) -> Self {
            Noisy { on_drop }
        }
    }

    thread_local! {
        /// What `Shouter`s' drops got from calling the kept callbacks.
        static SHOUTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// Calls and posts the callbacks `Tally.keep` keeps as it drops (ones
    /// it does not own: they live on).
    #[class]
    pub(crate) struct Shouter {
        watcher: Option<Callback>,
    }

    impl Drop for Shouter {
        fn drop(&mut self) {
            KEPT.with(|k| {
                for f in k.borrow().iter() {
                    let called = f.call(());
                    SHOUTS.with(|s| s.borrow_mut().push(format!("{called:?}")));
                    f.post(());
                }
            });
        }
    }

    #[class]
    impl Shouter {
        pub(crate) fn new() -> Self {
            Shouter { watcher: None }
        }

        /// Keeps `f`.
        pub(crate) fn watch(&mut self, f: Callback) {
            self.watcher = Some(f);
        }
    }

    /// Shows a `u32` prop.
    #[derive(Default)]
    pub(crate) struct Meter;

    #[derive(Clone, PartialEq, Props)]
    pub(crate) struct MeterProps {
        #[prop(default)]
        level: nsh_core::ui::Prop<u32>,
    }

    impl Component for Meter {
        type Props = MeterProps;
        type Event = NoEvent;

        fn view(props: MeterProps, _: Scope<NoEvent>) -> impl IntoElement {
            text(move || format!("level {}", props.level.get()))
        }
    }

    /// A type named `Instance` that is not a class's.
    mod gpu {
        pub(crate) struct Instance;
    }

    /// An alias of a callback type.
    type Listener = Option<Callback<i64>>;

    /// Keeps callbacks in fields of several types.
    #[class]
    pub(crate) struct Watcher {
        on: Listener,
        _gpu: gpu::Instance,
        shared: Rc<std::cell::RefCell<Vec<Callback>>>,
    }

    impl Drop for Watcher {
        fn drop(&mut self) {
            WATCHERS.with(|w| w.set(w.get() + 1));
        }
    }

    #[class]
    impl Watcher {
        pub(crate) fn new(on: Callback<i64>) -> Self {
            Watcher {
                on: Some(on),
                _gpu: gpu::Instance,
                shared: Rc::default(),
            }
        }

        /// Keeps `f` in the shared list (a root: what it reaches stays).
        pub(crate) fn share(&mut self, f: Callback) {
            self.shared.borrow_mut().push(f);
        }

        /// Calls the listener.
        pub(crate) fn fire(&self, n: i64) {
            if let Some(on) = &self.on {
                let _ = on.call(n);
            }
        }
    }

    /// The set the tests import as `test/classes`.
    pub(crate) fn registry() -> Registry {
        let mut r = Registry::ui_set("test/classes");
        r.class::<Tally>()
            .class::<Token>()
            .class::<Bomb>()
            .class::<Asker>()
            .class::<Noisy>()
            .class::<Watcher>()
            .class::<Shouter>()
            .module::<Tools>()
            .register::<Meter>();
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
            "import {{ text as show }} from \"nsh:rt\";\nimport {{ Tally, Token, Bomb, Asker, Noisy, Watcher, Shouter, Tools }} from \"test/classes\";\n{code}"
        ))
        .map_err(|e| e.message)?;
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        host.apply_posts(&mut ops);
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
                "TypeError: Tally.absorb: `other`: Tally used re-entrantly",
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
    fn callbacks_are_called_on_the_spot_and_later() {
        let host = host();
        let shown = run(
            &host,
            r#"
            const msg = (f) => { try { f(); return "no throw"; } catch (e) { return `${e.name}: ${e.message}`; } };
            show(Tally.sort(["b", "c", "a"], (x, y) => x < y ? -1 : x > y ? 1 : 0).join(""));
            show(msg(() => Tally.sort(["b", "a"], () => { throw new Error("no order"); })));
            show(msg(() => Tally.sort(["b", "a"], 3)));
            const t = new Tally(1);
            const seen = [];
            t.watch((n) => {
                seen.push(n);
                try { t.count; } catch (e) { seen.push(e.message); }
            });
            t.add(2);
            show(seen.join(","));
            t.announce();
            show(String(seen.length));
            globalThis.seen = seen;
            "#,
        )
        .unwrap();
        assert_eq!(
            shown,
            [
                "abc",
                "Error: no order",
                "TypeError: Tally.sort: `compare`: expected a function, got 3",
                "3,Tally used re-entrantly",
                "2",
            ]
        );
        // The post runs in the next turn.
        host.run_posted_without_core();
        assert_eq!(
            run(&host, "show(seen.join(','));").unwrap(),
            ["3,Tally used re-entrantly,3"]
        );
    }

    #[test]
    fn a_cycle_through_a_kept_callback_is_collected() {
        let host = host();
        let before = dropped();
        run(
            &host,
            r#"
            (() => {
                const t = new Tally(1);
                t.watch(() => t.count);
            })();
            "#,
        )
        .unwrap();
        assert_eq!(dropped(), before, "the cycle waits for the collector");
        host.collect_garbage();
        assert_eq!(
            dropped(),
            before + 1,
            "the instance's callback was reported"
        );
        // Kept elsewhere, a callback is a root.
        run(
            &host,
            r#"
            (() => {
                const t = new Tally(1);
                Tally.keep(() => t.count);
            })();
            "#,
        )
        .unwrap();
        host.collect_garbage();
        assert_eq!(
            dropped(),
            before + 1,
            "a kept callback keeps what it reaches"
        );
        run(&host, "Tally.forget();").unwrap();
        host.collect_garbage();
        assert_eq!(dropped(), before + 2);
    }

    /// `answer`, now.
    fn poll(answer: &mut Answer<Result<i64, String>>) -> Option<Result<i64, String>> {
        use std::future::Future;
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        match std::pin::Pin::new(answer).poll(&mut cx) {
            std::task::Poll::Ready(v) => Some(v),
            std::task::Poll::Pending => None,
        }
    }

    #[test]
    fn calls_queued_for_a_stopped_vm_are_dropped_and_say_so() {
        let host = host();
        run(&host, "Asker.ask(() => 1); Tally.keep(() => {});").unwrap();
        host.stop("over a quota".to_owned());
        host.run_posted_without_core();
        let stopped = Some(Err("the JS function was not called (JS stopped)".to_owned()));
        let mut answers = ANSWERS.with(|a| std::mem::take(&mut *a.borrow_mut()));
        assert_eq!(poll(&mut answers[0]), stopped);
        // Posted after it stopped: dropped at once.
        let mut late = KEPT.with(|k| k.borrow()[0].call_async(()));
        use std::future::Future;
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert_eq!(
            std::pin::Pin::new(&mut late).poll(&mut cx),
            std::task::Poll::Ready(Err("the JS function was not called (JS stopped)".to_owned()))
        );
        Tally::forget();
    }

    #[test]
    fn a_reply_that_throws_or_panics_leaves_no_exception_behind() {
        let host = host();
        run(
            &host,
            r#"
            Asker.askChange(async () => ({ get by() { throw new Error("no"); } }));
            Asker.askPanicky(async () => 1);
            "#,
        )
        .unwrap();
        host.run_posted_without_core();
        host.run_jobs();
        assert!(!host.has_exception());
        assert_eq!(run(&host, "show('alive');").unwrap(), ["alive"]);
    }

    #[test]
    fn an_interrupted_callback_stops_the_call_around_it() {
        let host = Host::with_options(HostOptions {
            ui_sets: vec![registry()],
            limits: nsh_core::js::Limits {
                load_budget: std::time::Duration::from_millis(100),
                ..Default::default()
            },
            ..HostOptions::default()
        })
        .unwrap();
        let interrupted = run(
            &host,
            "globalThis.after = Asker.callThrough(() => { for (;;) {} });",
        );
        assert!(interrupted.is_err(), "the script was stopped");
        assert_eq!(
            run(&host, "show(String(globalThis.after));").unwrap(),
            ["undefined"]
        );
    }

    #[test]
    fn a_drop_reaching_its_callbacks_finds_them_gone_when_collected() {
        let host = host();
        run(
            &host,
            r#"
            globalThis.heard = [];
            (() => {
                const n = new Noisy((what) => { heard.push(what); return n; });
            })();
            "#,
        )
        .unwrap();
        host.collect_garbage();
        host.run_posted_without_core();
        assert_eq!(run(&host, "show(heard.join(','));").unwrap(), [""]);
        assert_eq!(
            NOISE.with(|n| std::mem::take(&mut *n.borrow_mut())),
            ["Err(\"the JS function was not called (JS stopped)\")"]
        );
        // Dropped with the VM: the same.
        run(&host, "globalThis.kept = new Noisy(() => {});").unwrap();
        drop(host);
        assert_eq!(
            NOISE.with(|n| std::mem::take(&mut *n.borrow_mut())),
            ["Err(\"the JS function was not called (JS stopped)\")"]
        );
    }

    #[test]
    fn a_drop_in_a_collection_runs_no_js_and_posts_without_an_op() {
        let host = host();
        // Cycles only the collector frees (each through its own callback),
        // then a callback allocating enough for it to run inside a
        // synchronous call: the drops call and post a live function there.
        let shown = run(
            &host,
            r#"
            globalThis.heard = 0;
            Tally.keep(() => { heard++; return 1; });
            for (let i = 0; i < 20; i++) {
                const n = new Shouter();
                n.watch(() => n);
            }
            show(String(Asker.callThrough(() => {
                const junk = [];
                for (let i = 0; i < 300000; i++) junk.push({ i });
                return junk.length;
            })));
            "#,
        )
        .unwrap();
        assert_eq!(shown, ["300000"]);
        let shouts = SHOUTS.with(|s| std::mem::take(&mut *s.borrow_mut()));
        // (The loop's last binding may outlive it.)
        assert!(shouts.len() >= 19, "collected in the call: {shouts:?}");
        assert!(
            shouts
                .iter()
                .all(|s| s.contains("cannot be called while JS collects garbage")),
            "{shouts:?}"
        );
        // The posts were queued as they were (no op), and run now.
        host.run_posted_without_core();
        assert_eq!(
            run(&host, "show(String(heard));").unwrap(),
            [shouts.len().to_string()]
        );
        assert!(!host.has_exception());
        Tally::forget();
    }

    #[test]
    fn a_context_makes_as_many_classes_as_constructors_can_name() {
        use nsh_core::ui::Class as _;
        let host = host();
        // As if 32768 classes were made: the next one's index does not fit
        // its constructor's magic.
        host.with_raw(|ctx| {
            super::classes(ctx)
                .defs
                .borrow_mut()
                .extend(std::iter::repeat_n(Token::DEF, 32768));
        });
        // Importing the set makes its classes: the import fails.
        let message = run(&host, "show(\"imported\");").unwrap_err();
        assert!(
            message.contains(": a context makes at most 32768 Rust classes"),
            "{message}"
        );
    }

    #[test]
    fn fields_are_traced_by_their_types() {
        let host = host();
        let before = WATCHERS.with(Cell::get);
        run(
            &host,
            r#"
            (() => {
                const w = new Watcher(() => w);
                w.fire(1);
            })();
            "#,
        )
        .unwrap();
        host.collect_garbage();
        assert_eq!(
            WATCHERS.with(Cell::get),
            before + 1,
            "the alias's callback was traced"
        );
        // A shared list is not traced: a root, kept with what it reaches.
        run(
            &host,
            r#"
            (() => {
                const w = new Watcher(() => 0);
                w.share(() => w);
            })();
            "#,
        )
        .unwrap();
        host.collect_garbage();
        assert_eq!(
            WATCHERS.with(Cell::get),
            before + 1,
            "the shared callback is a root"
        );
    }

    #[test]
    fn callbacks_rust_keeps_outlive_their_vm_harmlessly() {
        let host = host();
        run(&host, "Tally.keep(() => 1);").unwrap();
        drop(host);
        Tally::forget();
    }

    /// An op that cannot be sent lets go of what it holds.
    #[test]
    fn an_op_that_is_not_sent_drops_what_it_holds() {
        let host = Host::with_options(HostOptions {
            ui_sets: vec![registry()],
            limits: nsh_core::js::Limits {
                max_op_values: 40,
                ..Default::default()
            },
            ..HostOptions::default()
        })
        .unwrap();
        let _ = host.eval_source(
            "import { Tally, Tools } from \"test/classes\";\n\
             let t = new Tally(1); for (let i = 0; i < 100; i++) { try { Tools.keepOp(t); } catch (e) {} }\n\
             t = null;",
        );
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        let sent = ops
            .iter()
            .filter(|op| matches!(op, nsh_core::Op::Rust(_)))
            .count();
        assert!(sent < 100, "the op queue filled up");
        let before = dropped();
        host.collect_garbage();
        assert_eq!(dropped(), before, "the ops sent hold the tally");
        drop(ops);
        assert_eq!(dropped(), before + 1, "nothing else holds it");
    }

    #[test]
    fn panics_in_what_the_core_hands_back_stay_in_rust() {
        use nsh_core::ui::{JsEvent, RustOp};
        let host = host();
        host.eval_source(
            "import { Tools } from \"test/classes\";\n\
             globalThis.seen = [];\n\
             Tools.measure(\"a\").then((v) => seen.push(`resolved ${v}`), (e) => seen.push(e.message));\n\
             Tools.postBrittle(() => seen.push(\"called\"));",
        )
        .unwrap();
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        let call = ops
            .iter()
            .find_map(|op| match op {
                nsh_core::Op::Rust(RustOp { call, .. }) if *call != 0 => Some(*call),
                _ => None,
            })
            .expect("the op's promise");
        host.apply_posts(&mut ops);
        drop(ops);
        // The result's conversion panics: the promise rejects with it.
        host.deliver(JsEvent::Reply {
            call,
            result: Ok(Box::new(Brittle)),
        });
        // Nobody waits for this one: the value drops there, and panics.
        host.deliver(JsEvent::Reply {
            call: 999,
            result: Ok(Box::new(Grenade)),
        });
        // An event's data panics as it converts: logged.
        host.deliver(JsEvent::Component {
            host: 2,
            event: Box::new(BrittleEvent),
        });
        // A posted call's arguments panic as they convert: logged, and the
        // function is not called.
        host.run_posted();
        host.run_jobs();
        assert!(!host.has_exception());
        assert_eq!(run(&host, "show(seen.join(','));").unwrap(), ["brittle"]);
    }

    #[test]
    fn a_post_whose_op_is_lost_holds_back_no_later_post() {
        let host = host();
        host.eval_source(
            "import { Tools } from \"test/classes\";\n\
             globalThis.heard = [];\n\
             Tools.listen((text) => heard.push(text));\n\
             Tools.tell(\"parked\");",
        )
        .unwrap();
        // The op holding the post's place goes without being applied.
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        assert!(ops.iter().any(|op| matches!(
            op,
            nsh_core::Op::Rust(nsh_core::ui::RustOp {
                run: nsh_core::ui::RustCall::Post(_),
                ..
            })
        )));
        drop(ops);
        // Posted later, outside a call: both run, in order.
        LISTENER.with(|l| {
            if let Some(listener) = &*l.borrow() {
                listener.post("later".to_owned());
            }
        });
        host.run_posted();
        assert_eq!(
            run(&host, "show(heard.join(','));").unwrap(),
            ["parked,later"]
        );
        LISTENER.with(|l| l.borrow_mut().take());
    }

    #[test]
    fn a_prop_of_the_wrong_type_is_a_warning_as_a_host_props_is() {
        let host = host();
        host.eval_source(
            "import { mount } from \"nsh\";\n\
             import { Meter } from \"test/classes\";\n\
             mount(() => Meter({ level: 1.5, testID: \"meter\" }));",
        )
        .unwrap();
        assert!(host.warned("Meter.level: expected an integer that fits u32, got 1.5"));
        // The other props are set; the bad one is not sent.
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        assert!(ops.iter().any(|op| matches!(
            op,
            nsh_core::Op::SetProp { prop: nsh_core::Prop::TestId(Some(id)), .. } if &**id == "meter"
        )));
        assert!(!ops.iter().any(|op| matches!(
            op,
            nsh_core::Op::SetProp {
                prop: nsh_core::Prop::RustProp("level", _),
                ..
            }
        )));
        assert!(!host.has_exception());
    }

    #[test]
    fn functions_without_a_context_run_in_the_call() {
        let host = host();
        let shown = run(
            &host,
            r#"
            const msg = (f) => { try { f(); return "no throw"; } catch (e) { return `${e.name}: ${e.message}`; } };
            show(String(Tools.sum([1, 2, 3.5]) + 1));
            show(String(Tools.parse("42") + 1));
            show(msg(() => Tools.parse("x")));
            show(msg(() => Tools.sum("x")));
            const t = Tools.tally(4);
            show(`${t instanceof Tally} ${t.count}`);
            show(String(Tools.measure("abc") instanceof Promise));
            show(msg(() => Tools.signalHere()));
            globalThis.heard = [];
            Tools.listen((text) => heard.push(text));
            Tools.tell("hello");
            show(String(heard.length));
            "#,
        )
        .unwrap();
        assert_eq!(
            shown,
            [
                "7.5",
                "43",
                "Error: not a number: x",
                "TypeError: Tools.sum: `values`: expected an array, got \"x\"",
                "true 4",
                "true",
                "Error: no reactive runtime: signals are used on the engine thread, while components run",
                "0",
            ]
        );
        host.run_posted_without_core();
        assert_eq!(run(&host, "show(heard.join(','));").unwrap(), ["hello"]);
        LISTENER.with(|l| l.borrow_mut().take());
    }

    /// An op returning `()` is a plain call: `undefined`, a wrong argument
    /// throws in the call, and its op asks for no reply (call 0). One
    /// returning a value is a promise, which a wrong argument rejects.
    #[test]
    fn ops_returning_nothing_are_plain_calls() {
        use nsh_core::ui::RustOp;
        let host = host();
        host.eval_source(
            "import { text as show } from \"nsh:rt\";\n\
             import { Tally, Tools } from \"test/classes\";\n\
             const msg = (f) => { try { return String(f()); } catch (e) { return `threw ${e.name}: ${e.message}`; } };\n\
             show(msg(() => Tools.keepOp(new Tally(1))));\n\
             show(msg(() => Tools.keepOp(5)));\n\
             const measured = Tools.measure(5);\n\
             show(String(measured instanceof Promise));\n\
             measured.catch((e) => show(`rejected ${e.name}: ${e.message}`));",
        )
        .unwrap();
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        host.run_jobs();
        host.take_ops(&mut ops);
        let calls: Vec<u32> = ops
            .iter()
            .filter_map(|op| match op {
                nsh_core::Op::Rust(RustOp { call, .. }) => Some(*call),
                _ => None,
            })
            .collect();
        // Only the first `keepOp` is sent (the others did not convert).
        assert_eq!(calls, [0]);
        let shown: Vec<String> = ops
            .iter()
            .filter_map(|op| match op {
                nsh_core::Op::CreateText { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(
            shown,
            [
                "undefined",
                "threw TypeError: Tools.keepOp: `tally`: expected a Tally, got 5",
                "true",
                "rejected TypeError: Tools.measure: `text`: expected a string, got 5",
            ]
        );
        assert!(!host.has_exception());
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
