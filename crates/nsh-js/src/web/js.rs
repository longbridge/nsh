//! The toolkit the web APIs are written with: owned values, errors as
//! `Result`, natives, closures, promises and byte access over QuickJS's C
//! API.
//!
//! Rules: a [`V`] owns one reference and frees it when dropped (through the
//! runtime, so values held by objects are freed by their finalizers); a
//! native returns `R<V>`, `Err(Throw)` meaning a JS exception is pending.

use std::{
    ffi::{CStr, CString, c_int, c_void},
    ptr,
};

use rquickjs::qjs;

pub(crate) type Ctx = *mut qjs::JSContext;
pub(crate) type Raw = qjs::JSValue;
pub(crate) type Native = unsafe extern "C" fn(Ctx, Raw, c_int, *mut Raw) -> Raw;

pub(crate) const UNDEFINED: Raw = qjs::JS_UNDEFINED;
pub(crate) const NULL: Raw = qjs::JS_NULL;

/// A JS exception is pending.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Throw;

/// The result of web API code: `Err` means the exception is pending.
pub(crate) type R<T> = Result<T, Throw>;

/// An owned JS value (one reference), freed when dropped.
pub(crate) struct V {
    rt: *mut qjs::JSRuntime,
    raw: Raw,
}

impl Drop for V {
    fn drop(&mut self) {
        // SAFETY: one owned reference; values without one need no runtime.
        unsafe { qjs::JS_FreeValueRT(self.rt, self.raw) };
    }
}

impl V {
    /// Takes ownership of `raw` (a value the caller owns).
    pub(crate) fn own(ctx: Ctx, raw: Raw) -> V {
        // SAFETY: a live context.
        V {
            rt: unsafe { qjs::JS_GetRuntime(ctx) },
            raw,
        }
    }

    /// A new reference to `raw` (borrowed).
    pub(crate) fn dup(ctx: Ctx, raw: Raw) -> V {
        // SAFETY: a live value.
        V::own(ctx, unsafe { qjs::JS_DupValue(ctx, raw) })
    }

    /// `raw` (owned), or the pending exception as `Err`.
    pub(crate) fn checked(ctx: Ctx, raw: Raw) -> R<V> {
        // SAFETY: an inline tag check.
        if unsafe { qjs::JS_IsException(raw) } {
            Err(Throw)
        } else {
            Ok(V::own(ctx, raw))
        }
    }

    pub(crate) const fn undefined() -> V {
        V {
            rt: ptr::null_mut(),
            raw: UNDEFINED,
        }
    }

    pub(crate) const fn null() -> V {
        V {
            rt: ptr::null_mut(),
            raw: NULL,
        }
    }

    pub(crate) fn bool(b: bool) -> V {
        V {
            rt: ptr::null_mut(),
            raw: if b { qjs::JS_TRUE } else { qjs::JS_FALSE },
        }
    }

    pub(crate) fn num(n: f64) -> V {
        V {
            rt: ptr::null_mut(),
            raw: qjs::JS_NewFloat64(n),
        }
    }

    /// The borrowed value.
    #[inline]
    pub(crate) fn raw(&self) -> Raw {
        self.raw
    }

    /// The owned reference, for the caller to hand on.
    pub(crate) fn take(self) -> Raw {
        let raw = self.raw;
        std::mem::forget(self);
        raw
    }

    /// Another reference.
    pub(crate) fn clone_ref(&self) -> V {
        // SAFETY: a live value; the new reference shares the runtime.
        V {
            rt: self.rt,
            raw: unsafe { qjs::JS_DupValueRT(self.rt_or_any(), self.raw) },
        }
    }

    fn rt_or_any(&self) -> *mut qjs::JSRuntime {
        self.rt
    }

    /// Moves the value out, leaving `undefined`.
    pub(crate) fn take_out(&mut self) -> V {
        std::mem::take(self)
    }

    pub(crate) fn is_undefined(&self) -> bool {
        is_undefined(self.raw)
    }

    pub(crate) fn is_nullish(&self) -> bool {
        is_nullish(self.raw)
    }
}

impl Default for V {
    fn default() -> V {
        V::undefined()
    }
}

impl std::fmt::Debug for V {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SAFETY: reading the tag.
        write!(f, "V(tag {})", unsafe { qjs::JS_VALUE_GET_TAG(self.raw) })
    }
}

/// What values an object owns, for the cycle collector (every owned [`V`]
/// must be reported exactly once; one forgotten only keeps cycles alive).
pub(crate) trait Trace {
    fn trace(&self, mark: &mut dyn FnMut(Raw));
}

impl Trace for V {
    fn trace(&self, mark: &mut dyn FnMut(Raw)) {
        mark(self.raw);
    }
}

/// Plain Rust data holds no values.
macro_rules! no_values {
    ($($ty:ty),*) => {$(
        impl Trace for $ty {
            fn trace(&self, _: &mut dyn FnMut(Raw)) {}
        }
    )*};
}

no_values!(String, bool, u8, u16, u32, u64, usize, i32, i64, f64);

impl<T: Trace> Trace for Option<T> {
    fn trace(&self, mark: &mut dyn FnMut(Raw)) {
        if let Some(t) = self {
            t.trace(mark);
        }
    }
}

impl<T: Trace> Trace for Vec<T> {
    fn trace(&self, mark: &mut dyn FnMut(Raw)) {
        for t in self {
            t.trace(mark);
        }
    }
}

impl<T: Trace> Trace for std::collections::VecDeque<T> {
    fn trace(&self, mark: &mut dyn FnMut(Raw)) {
        for t in self {
            t.trace(mark);
        }
    }
}

impl<A: Trace, B: Trace> Trace for (A, B) {
    fn trace(&self, mark: &mut dyn FnMut(Raw)) {
        self.0.trace(mark);
        self.1.trace(mark);
    }
}

impl<A: Trace, B: Trace, C: Trace> Trace for (A, B, C) {
    fn trace(&self, mark: &mut dyn FnMut(Raw)) {
        self.0.trace(mark);
        self.1.trace(mark);
        self.2.trace(mark);
    }
}

/// Implements [`Trace`] for a struct by its fields holding values.
macro_rules! trace {
    ($ty:ty { $($field:ident),* $(,)? }) => {
        impl $crate::web::js::Trace for $ty {
            #[allow(unused_variables)]
            fn trace(&self, mark: &mut dyn FnMut($crate::web::js::Raw)) {
                $($crate::web::js::Trace::trace(&self.$field, mark);)*
            }
        }
    };
}
pub(crate) use trace;

// --- Values ---------------------------------------------------------------

#[inline]
pub(crate) fn is_undefined(v: Raw) -> bool {
    // SAFETY: an inline tag check.
    unsafe { qjs::JS_IsUndefined(v) }
}

#[inline]
pub(crate) fn is_nullish(v: Raw) -> bool {
    // SAFETY: inline tag checks.
    unsafe { qjs::JS_IsUndefined(v) || qjs::JS_IsNull(v) }
}

#[inline]
pub(crate) fn is_object(v: Raw) -> bool {
    // SAFETY: an inline tag check.
    unsafe { qjs::JS_IsObject(v) }
}

pub(crate) fn is_function(ctx: Ctx, v: Raw) -> bool {
    // SAFETY: a live value.
    unsafe { qjs::JS_IsFunction(ctx, v) }
}

pub(crate) fn same(a: Raw, b: Raw) -> bool {
    // SAFETY: SameValue of live values (no side effects).
    unsafe { qjs::JS_IsSameValue(ptr::null_mut(), a, b) }
}

/// A new string.
pub(crate) fn string(ctx: Ctx, s: &str) -> R<V> {
    // SAFETY: a new string from valid UTF-8.
    V::checked(ctx, unsafe {
        qjs::JS_NewStringLen(ctx, s.as_ptr().cast(), s.len() as _)
    })
}

/// `ToString(v)` as a USVString (throws for symbols): lone surrogates,
/// which QuickJS hands out as 3-byte WTF-8 sequences, become one U+FFFD
/// each.
pub(crate) fn to_string(ctx: Ctx, v: Raw) -> R<String> {
    // SAFETY: a live value; the C string is freed below.
    unsafe {
        let mut len = 0usize;
        let p = qjs::JS_ToCStringLen(ctx, &mut len, v);
        if p.is_null() {
            return Err(Throw);
        }
        let bytes = std::slice::from_raw_parts(p.cast::<u8>(), len);
        let s = match std::str::from_utf8(bytes) {
            Ok(s) => s.to_owned(),
            Err(_) => from_wtf8(bytes),
        };
        qjs::JS_FreeCString(ctx, p);
        Ok(s)
    }
}

/// `ToString(v)` as WTF-8: UTF-8 with lone surrogates as 3-byte
/// sequences (for code that pairs surrogates across strings).
pub(crate) fn to_wtf8(ctx: Ctx, v: Raw) -> R<Vec<u8>> {
    // SAFETY: a live value; the C string is freed below.
    unsafe {
        let mut len = 0usize;
        let p = qjs::JS_ToCStringLen(ctx, &mut len, v);
        if p.is_null() {
            return Err(Throw);
        }
        let bytes = std::slice::from_raw_parts(p.cast::<u8>(), len).to_vec();
        qjs::JS_FreeCString(ctx, p);
        Ok(bytes)
    }
}

/// WTF-8 (UTF-8 with encoded surrogates) as UTF-8, a U+FFFD for each
/// surrogate.
pub(crate) fn from_wtf8(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0xED
            && i + 2 < bytes.len()
            && (0xA0..=0xBF).contains(&bytes[i + 1])
            && (0x80..=0xBF).contains(&bytes[i + 2])
        {
            out.extend_from_slice("\u{FFFD}".as_bytes());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `ToString(v)` unless `v` is undefined (then `None`).
pub(crate) fn opt_string(ctx: Ctx, v: Raw) -> R<Option<String>> {
    if is_undefined(v) {
        Ok(None)
    } else {
        to_string(ctx, v).map(Some)
    }
}

/// `ToNumber(v)`.
pub(crate) fn to_number(ctx: Ctx, v: Raw) -> R<f64> {
    let mut n = 0f64;
    // SAFETY: converting a live value.
    if unsafe { qjs::JS_ToFloat64(ctx, &mut n, v) } < 0 {
        return Err(Throw);
    }
    Ok(n)
}

/// `ToBoolean(v)`.
pub(crate) fn to_bool(ctx: Ctx, v: Raw) -> bool {
    // SAFETY: converting a live value (never throws).
    unsafe { qjs::JS_ToBool(ctx, v) == 1 }
}

/// WebIDL `unsigned long long` (enforce range off): a non-negative integer.
pub(crate) fn to_index(ctx: Ctx, v: Raw) -> R<u64> {
    let n = to_number(ctx, v)?;
    Ok(if n.is_finite() && n > 0.0 {
        n.trunc() as u64
    } else {
        0
    })
}

pub(crate) fn new_object(ctx: Ctx) -> R<V> {
    // SAFETY: a new object.
    V::checked(ctx, unsafe { qjs::JS_NewObject(ctx) })
}

pub(crate) fn new_array(ctx: Ctx, items: Vec<V>) -> R<V> {
    let mut raws: Vec<Raw> = items.into_iter().map(V::take).collect();
    // SAFETY: the array takes the items.
    V::checked(ctx, unsafe {
        qjs::JS_NewArrayFrom(ctx, raws.len() as c_int, raws.as_mut_ptr())
    })
}

pub(crate) fn global(ctx: Ctx) -> V {
    // SAFETY: a new reference to the global object.
    V::own(ctx, unsafe { qjs::JS_GetGlobalObject(ctx) })
}

fn cstr(key: &str) -> CString {
    CString::new(key).unwrap_or_default()
}

/// `obj[key]`.
pub(crate) fn get(ctx: Ctx, obj: Raw, key: &str) -> R<V> {
    let k = cstr(key);
    // SAFETY: a live object (getters may run JS).
    V::checked(ctx, unsafe { qjs::JS_GetPropertyStr(ctx, obj, k.as_ptr()) })
}

/// `obj[atom]`.
pub(crate) fn get_atom(ctx: Ctx, obj: Raw, atom: qjs::JSAtom) -> R<V> {
    // SAFETY: a live object (getters may run JS).
    V::checked(ctx, unsafe { qjs::JS_GetProperty(ctx, obj, atom) })
}

/// Defines data property `key` (writable, configurable; `enumerable`).
pub(crate) fn define(ctx: Ctx, obj: Raw, key: &str, value: V, enumerable: bool) -> R<()> {
    let k = cstr(key);
    let mut flags = qjs::JS_PROP_WRITABLE | qjs::JS_PROP_CONFIGURABLE;
    if enumerable {
        flags |= qjs::JS_PROP_ENUMERABLE;
    }
    // SAFETY: the property takes the value.
    let r = unsafe {
        qjs::JS_DefinePropertyValueStr(ctx, obj, k.as_ptr(), value.take(), flags as c_int)
    };
    if r < 0 { Err(Throw) } else { Ok(()) }
}

/// `{ value, done }`.
pub(crate) fn iter_result(ctx: Ctx, value: V, done: bool) -> R<V> {
    let o = new_object(ctx)?;
    define(ctx, o.raw(), "value", value, true)?;
    define(ctx, o.raw(), "done", V::bool(done), true)?;
    Ok(o)
}

/// `f.call(this, ...args)`.
pub(crate) fn call(ctx: Ctx, f: Raw, this: Raw, args: &[Raw]) -> R<V> {
    let mut args = args.to_vec();
    // SAFETY: borrowed arguments; the result is owned.
    V::checked(ctx, unsafe {
        qjs::JS_Call(ctx, f, this, args.len() as c_int, args.as_mut_ptr())
    })
}

/// `obj[name](...args)`.
pub(crate) fn invoke(ctx: Ctx, obj: Raw, name: &str, args: &[Raw]) -> R<V> {
    let f = get(ctx, obj, name)?;
    call(ctx, f.raw(), obj, args)
}

/// `new f(...args)`.
pub(crate) fn construct(ctx: Ctx, f: Raw, args: &[Raw]) -> R<V> {
    let mut args = args.to_vec();
    // SAFETY: borrowed arguments; the result is owned.
    V::checked(ctx, unsafe {
        qjs::JS_CallConstructor(ctx, f, args.len() as c_int, args.as_mut_ptr())
    })
}

/// `GetMethod(obj, name)`: the function, `None` when undefined or null;
/// throws when it is something else.
pub(crate) fn get_method(ctx: Ctx, obj: Raw, name: &str) -> R<Option<V>> {
    let f = get(ctx, obj, name)?;
    if f.is_nullish() {
        return Ok(None);
    }
    if !is_function(ctx, f.raw()) {
        return Err(type_error(ctx, &format!("{name} is not a function")));
    }
    Ok(Some(f))
}

// --- Errors ---------------------------------------------------------------

pub(crate) fn type_error(ctx: Ctx, message: &str) -> Throw {
    let m = cstr(message);
    // SAFETY: throws on a live context; `%s` keeps the message literal.
    unsafe { qjs::JS_ThrowTypeError(ctx, c"%s".as_ptr(), m.as_ptr()) };
    Throw
}

pub(crate) fn range_error(ctx: Ctx, message: &str) -> Throw {
    let m = cstr(message);
    // SAFETY: as above.
    unsafe { qjs::JS_ThrowRangeError(ctx, c"%s".as_ptr(), m.as_ptr()) };
    Throw
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn syntax_error(ctx: Ctx, message: &str) -> Throw {
    let m = cstr(message);
    // SAFETY: as above.
    unsafe { qjs::JS_ThrowSyntaxError(ctx, c"%s".as_ptr(), m.as_ptr()) };
    Throw
}

/// Throws `value`.
pub(crate) fn throw(ctx: Ctx, value: V) -> Throw {
    // SAFETY: the exception takes the value.
    unsafe { qjs::JS_Throw(ctx, value.take()) };
    Throw
}

/// The pending exception, cleared.
pub(crate) fn take_exception(ctx: Ctx) -> V {
    // SAFETY: an owned value (the exception, or uninitialized when none).
    V::own(ctx, unsafe { qjs::JS_GetException(ctx) })
}

/// A new `DOMException(message, name)`.
pub(crate) fn dom_exception(ctx: Ctx, message: &str, name: &str) -> R<V> {
    let g = global(ctx);
    let class = get(ctx, g.raw(), "DOMException")?;
    let (m, n) = (string(ctx, message)?, string(ctx, name)?);
    construct(ctx, class.raw(), &[m.raw(), n.raw()])
}

/// Throws `DOMException(message, name)`.
pub(crate) fn throw_dom(ctx: Ctx, message: &str, name: &str) -> Throw {
    match dom_exception(ctx, message, name) {
        Ok(e) => throw(ctx, e),
        Err(t) => t,
    }
}

unsafe extern "C" fn rethrow(ctx: Ctx, argc: c_int, argv: *mut Raw) -> Raw {
    // SAFETY: the job's one argument, dup'd for the throw.
    unsafe {
        let e = Args::new(argc, argv).get(0);
        qjs::JS_Throw(ctx, qjs::JS_DupValue(ctx, e))
    }
}

/// Reports `error` as uncaught without interrupting the caller (the
/// specs' "report an exception": a listener that threw, ...).
pub(crate) fn report(ctx: Ctx, error: V) {
    let mut args = [error.raw()];
    // SAFETY: the job keeps its own reference to the argument.
    unsafe { qjs::JS_EnqueueJob(ctx, Some(rethrow), 1, args.as_mut_ptr()) };
}

/// Reports the pending exception.
pub(crate) fn report_exception(ctx: Ctx) {
    report(ctx, take_exception(ctx));
}

/// A `TypeError(message)` value (not thrown).
pub(crate) fn type_error_value(ctx: Ctx, message: &str) -> V {
    let _ = type_error(ctx, message);
    take_exception(ctx)
}

// --- Natives --------------------------------------------------------------

/// The arguments of a native call.
#[derive(Clone, Copy)]
pub(crate) struct Args<'a>(&'a [Raw]);

impl<'a> Args<'a> {
    /// # Safety
    ///
    /// `argv` holds `argc` live values for the call.
    pub(crate) unsafe fn new(argc: c_int, argv: *mut Raw) -> Args<'a> {
        if argc <= 0 || argv.is_null() {
            Args(&[])
        } else {
            // SAFETY: per the contract.
            Args(unsafe { std::slice::from_raw_parts(argv, argc as usize) })
        }
    }

    /// Argument `i` (undefined when missing).
    #[inline]
    pub(crate) fn get(&self, i: usize) -> Raw {
        self.0.get(i).copied().unwrap_or(UNDEFINED)
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

/// Defines native `$name` from a body returning `R<V>`.
macro_rules! native {
    ($(#[$m:meta])* fn $name:ident($ctx:ident, $this:ident, $args:ident) $body:block) => {
        $(#[$m])*
        pub(crate) unsafe extern "C" fn $name(
            ctx: $crate::web::js::Ctx,
            this: $crate::web::js::Raw,
            argc: std::ffi::c_int,
            argv: *mut $crate::web::js::Raw,
        ) -> $crate::web::js::Raw {
            #[allow(unused_variables)]
            fn body(
                $ctx: $crate::web::js::Ctx,
                $this: $crate::web::js::Raw,
                $args: $crate::web::js::Args<'_>,
            ) -> $crate::web::js::R<$crate::web::js::V> $body
            // SAFETY: QuickJS passes `argc` live arguments.
            let args = unsafe { $crate::web::js::Args::new(argc, argv) };
            match body(ctx, this, args) {
                Ok(v) => v.take(),
                Err(_) => rquickjs::qjs::JS_EXCEPTION,
            }
        }
    };
}
pub(crate) use native;

/// A plain function `name`.
pub(crate) fn function(ctx: Ctx, name: &CStr, f: Native, length: c_int) -> R<V> {
    // SAFETY: a plain C function.
    V::checked(ctx, unsafe {
        qjs::JS_NewCFunction2(
            ctx,
            Some(f),
            name.as_ptr(),
            length,
            qjs::JSCFunctionEnum_JS_CFUNC_generic,
            0,
        )
    })
}

// --- Closures -------------------------------------------------------------

/// A closure's code: its captured values and the call's arguments.
pub(crate) type ClosureFn = Box<dyn Fn(Ctx, &[V], Args<'_>) -> R<V>>;

/// The data of a closure object.
pub(crate) struct Closure {
    pub(crate) f: ClosureFn,
    /// JS values it uses (marked; Rust state goes in `f`'s captures).
    pub(crate) captures: Vec<V>,
}

trace!(Closure { captures });

/// A function running `f` with `captures`.
pub(crate) fn closure(
    ctx: Ctx,
    captures: Vec<V>,
    f: impl Fn(Ctx, &[V], Args<'_>) -> R<V> + 'static,
) -> R<V> {
    super::new_closure(
        ctx,
        Closure {
            f: Box::new(f),
            captures,
        },
    )
}

// --- Promises -------------------------------------------------------------

/// A new promise and its resolving functions.
pub(crate) struct Deferred {
    pub(crate) promise: V,
    pub(crate) resolve: V,
    pub(crate) reject: V,
}

trace!(Deferred {
    promise,
    resolve,
    reject
});

impl Deferred {
    pub(crate) fn new(ctx: Ctx) -> R<Deferred> {
        let mut funcs = [UNDEFINED; 2];
        // SAFETY: a new promise; the functions are owned.
        let promise = V::checked(ctx, unsafe {
            qjs::JS_NewPromiseCapability(ctx, funcs.as_mut_ptr())
        })?;
        Ok(Deferred {
            promise,
            resolve: V::own(ctx, funcs[0]),
            reject: V::own(ctx, funcs[1]),
        })
    }

    /// Resolves with `value` (later calls do nothing).
    pub(crate) fn resolve(&self, ctx: Ctx, value: Raw) {
        let _ = call(ctx, self.resolve.raw(), UNDEFINED, &[value]);
    }

    /// Rejects with `reason`.
    pub(crate) fn reject(&self, ctx: Ctx, reason: Raw) {
        let _ = call(ctx, self.reject.raw(), UNDEFINED, &[reason]);
    }
}

/// `Promise.resolve(value)` (a new promise, also for promises).
pub(crate) fn resolved(ctx: Ctx, value: Raw) -> R<V> {
    let d = Deferred::new(ctx)?;
    d.resolve(ctx, value);
    Ok(d.promise)
}

/// A promise rejected with `reason`.
pub(crate) fn rejected(ctx: Ctx, reason: Raw) -> R<V> {
    let d = Deferred::new(ctx)?;
    d.reject(ctx, reason);
    Ok(d.promise)
}

/// A promise rejected with the pending exception.
pub(crate) fn rejected_with_exception(ctx: Ctx) -> R<V> {
    let e = take_exception(ctx);
    rejected(ctx, e.raw())
}

/// The result of `f` as a promise: its value, or rejected with what it
/// threw.
pub(crate) fn promise_of(ctx: Ctx, f: impl FnOnce() -> R<V>) -> R<V> {
    match f() {
        Ok(v) => resolved(ctx, v.raw()),
        Err(Throw) => rejected_with_exception(ctx),
    }
}

/// Marks `promise` handled (no unhandled-rejection report).
pub(crate) fn mark_handled(ctx: Ctx, promise: Raw) {
    // SAFETY: a live promise (no-op for other values).
    unsafe { qjs::JS_PromiseMarkAsHandled(ctx, promise) };
}

/// `promise.then(on_ok, on_err)`, through the intrinsic `then` (as the
/// specs' "react to" does, unaffected by an overridden `then`).
pub(crate) fn then(ctx: Ctx, promise: Raw, on_ok: Raw, on_err: Raw) -> R<V> {
    let p = if unsafe { qjs::JS_IsPromise(promise) } {
        V::dup(ctx, promise)
    } else {
        resolved(ctx, promise)?
    };
    let then = super::intrinsic_then(ctx)?;
    call(ctx, then.raw(), p.raw(), &[on_ok, on_err])
}

/// Reacts to `promise` with Rust code (its result discarded and handled).
pub(crate) fn upon(
    ctx: Ctx,
    promise: Raw,
    captures: Vec<V>,
    on_ok: impl Fn(Ctx, &[V], Raw) -> R<()> + 'static,
    on_err: impl Fn(Ctx, &[V], Raw) -> R<()> + 'static,
) -> R<()> {
    let ok = closure(
        ctx,
        captures.iter().map(V::clone_ref).collect(),
        move |ctx, c, args| {
            on_ok(ctx, c, args.get(0))?;
            Ok(V::undefined())
        },
    )?;
    let err = closure(ctx, captures, move |ctx, c, args| {
        on_err(ctx, c, args.get(0))?;
        Ok(V::undefined())
    })?;
    let p = then(ctx, promise, ok.raw(), err.raw())?;
    mark_handled(ctx, p.raw());
    Ok(())
}

// --- Iteration ------------------------------------------------------------

/// Calls `f` with each value `iterable` yields (sync iteration protocol).
pub(crate) fn for_each(ctx: Ctx, iterable: Raw, mut f: impl FnMut(V) -> R<()>) -> R<()> {
    let method = get_atom(ctx, iterable, qjs::JS_ATOM_Symbol_iterator as _)?;
    if !is_function(ctx, method.raw()) {
        return Err(type_error(ctx, "value is not iterable"));
    }
    let iterator = call(ctx, method.raw(), iterable, &[])?;
    if !is_object(iterator.raw()) {
        return Err(type_error(ctx, "iterator is not an object"));
    }
    let next = get(ctx, iterator.raw(), "next")?;
    loop {
        let r = call(ctx, next.raw(), iterator.raw(), &[])?;
        if !is_object(r.raw()) {
            return Err(type_error(ctx, "iterator result is not an object"));
        }
        let done = get(ctx, r.raw(), "done")?;
        if to_bool(ctx, done.raw()) {
            return Ok(());
        }
        let value = get(ctx, r.raw(), "value")?;
        if let Err(t) = f(value) {
            // Close the iterator, keeping the first error.
            let e = take_exception(ctx);
            if let Ok(Some(ret)) = get_method(ctx, iterator.raw(), "return") {
                let _ = call(ctx, ret.raw(), iterator.raw(), &[]);
            }
            let _ = take_exception(ctx);
            let _ = throw(ctx, e);
            return Err(t);
        }
    }
}

/// Whether `v` has a callable `Symbol.iterator`.
pub(crate) fn is_iterable(ctx: Ctx, v: Raw) -> R<bool> {
    if !is_object(v) {
        return Ok(false);
    }
    let m = get_atom(ctx, v, qjs::JS_ATOM_Symbol_iterator as _)?;
    Ok(is_function(ctx, m.raw()))
}

/// The own enumerable string keys of `obj`, with their values (a WebIDL
/// record).
pub(crate) fn record(ctx: Ctx, obj: Raw) -> R<Vec<(String, V)>> {
    let mut tab: *mut qjs::JSPropertyEnum = ptr::null_mut();
    let mut len: u32 = 0;
    let flags = qjs::JS_GPN_STRING_MASK | qjs::JS_GPN_ENUM_ONLY;
    // SAFETY: enumerating a live object; the table is freed below.
    if unsafe { qjs::JS_GetOwnPropertyNames(ctx, &mut tab, &mut len, obj, flags as c_int) } < 0 {
        return Err(Throw);
    }
    let mut out = Vec::with_capacity(len as usize);
    let mut result = Ok(());
    for i in 0..len as usize {
        // SAFETY: `len` entries.
        let atom = unsafe { (*tab.add(i)).atom };
        let key = V::own(ctx, unsafe { qjs::JS_AtomToString(ctx, atom) });
        let value = get_atom(ctx, obj, atom);
        match (to_string(ctx, key.raw()), value) {
            (Ok(k), Ok(v)) => out.push((k, v)),
            _ => {
                result = Err(Throw);
                break;
            }
        }
    }
    // SAFETY: the table from above.
    unsafe { qjs::JS_FreePropertyEnum(ctx, tab, len) };
    result.map(|()| out)
}

// --- Bytes ----------------------------------------------------------------

/// The bytes of an ArrayBuffer or a view (borrowed until JS runs).
///
/// # Safety
///
/// The slice is valid until the buffer is detached, resized or freed: use
/// it before running any JS.
pub(crate) unsafe fn bytes_of<'a>(ctx: Ctx, v: Raw) -> Option<&'a [u8]> {
    // SAFETY: reads of a live value; per the contract for the slice.
    unsafe {
        if qjs::JS_IsArrayBuffer(v) {
            let mut len: qjs::size_t = 0;
            let p = qjs::JS_GetArrayBuffer(ctx, &mut len, v);
            if p.is_null() {
                let _ = take_exception(ctx);
                return Some(&[]);
            }
            return Some(std::slice::from_raw_parts(p, len as usize));
        }
        let view = view(ctx, v)?;
        let mut len: qjs::size_t = 0;
        let p = qjs::JS_GetArrayBuffer(ctx, &mut len, view.buffer.raw());
        if p.is_null() {
            let _ = take_exception(ctx);
            return Some(&[]);
        }
        let len = len as usize;
        let end = view.offset.saturating_add(view.length).min(len);
        Some(std::slice::from_raw_parts(
            p.add(view.offset.min(end)),
            end - view.offset.min(end),
        ))
    }
}

/// The bytes of an ArrayBuffer or a view, writable.
///
/// # Safety
///
/// As [`bytes_of`], and no other slice of the same bytes is in use.
pub(crate) unsafe fn bytes_mut<'a>(ctx: Ctx, v: Raw) -> Option<&'a mut [u8]> {
    // SAFETY: per the contract.
    unsafe {
        bytes_of(ctx, v).map(|b| std::slice::from_raw_parts_mut(b.as_ptr().cast_mut(), b.len()))
    }
}

/// Copies the bytes of a BufferSource (`None`: not one).
pub(crate) fn buffer_source(ctx: Ctx, v: Raw) -> Option<Vec<u8>> {
    // SAFETY: copied before any JS runs.
    unsafe { bytes_of(ctx, v) }.map(<[u8]>::to_vec)
}

/// An ArrayBufferView's buffer, byte offset and length; its element size
/// and typed array kind (`None` for a DataView).
pub(crate) struct View {
    pub(crate) buffer: V,
    pub(crate) offset: usize,
    pub(crate) length: usize,
    pub(crate) element: usize,
    pub(crate) kind: Option<c_int>,
}

/// `v` as an ArrayBufferView.
pub(crate) fn view(ctx: Ctx, v: Raw) -> Option<View> {
    if !is_object(v) {
        return None;
    }
    // SAFETY: inline checks and reads of a live value.
    unsafe {
        let kind = qjs::JS_GetTypedArrayType(v);
        if kind >= 0 {
            let (mut offset, mut length, mut element): (qjs::size_t, qjs::size_t, qjs::size_t) =
                (0, 0, 0);
            let buffer =
                qjs::JS_GetTypedArrayBuffer(ctx, v, &mut offset, &mut length, &mut element);
            if qjs::JS_IsException(buffer) {
                let _ = take_exception(ctx);
                return None;
            }
            return Some(View {
                buffer: V::own(ctx, buffer),
                offset: offset as usize,
                length: length as usize,
                element: element as usize,
                kind: Some(kind),
            });
        }
    }
    // A DataView: by its getters.
    let g = global(ctx);
    let data_view = get(ctx, g.raw(), "DataView").ok()?;
    // SAFETY: a live object and constructor.
    if unsafe { qjs::JS_IsInstanceOf(ctx, v, data_view.raw()) } != 1 {
        return None;
    }
    let buffer = get(ctx, v, "buffer").ok()?;
    let offset = get(ctx, v, "byteOffset").ok()?;
    let length = get(ctx, v, "byteLength").ok()?;
    Some(View {
        buffer,
        offset: to_number(ctx, offset.raw()).ok()? as usize,
        length: to_number(ctx, length.raw()).ok()? as usize,
        element: 1,
        kind: None,
    })
}

/// A new ArrayBuffer holding `bytes`.
pub(crate) fn array_buffer(ctx: Ctx, bytes: &[u8]) -> R<V> {
    // SAFETY: a copy into a new buffer.
    V::checked(ctx, unsafe {
        qjs::JS_NewArrayBufferCopy(ctx, bytes.as_ptr(), bytes.len() as _)
    })
}

/// A new Uint8Array holding `bytes`.
pub(crate) fn uint8_array(ctx: Ctx, bytes: &[u8]) -> R<V> {
    // SAFETY: a copy into a new array.
    V::checked(ctx, unsafe {
        qjs::JS_NewUint8ArrayCopy(ctx, bytes.as_ptr(), bytes.len() as _)
    })
}

/// A new typed array of `kind` over `buffer[offset..]` with `length`
/// elements (`kind` `None`: a DataView of `length` bytes).
pub(crate) fn new_view(
    ctx: Ctx,
    kind: Option<c_int>,
    buffer: Raw,
    offset: usize,
    length: usize,
) -> R<V> {
    let (o, l) = (V::num(offset as f64), V::num(length as f64));
    match kind {
        Some(kind) => {
            let mut args = [buffer, o.raw(), l.raw()];
            // SAFETY: borrowed arguments; a new view.
            V::checked(ctx, unsafe {
                qjs::JS_NewTypedArray(ctx, 3, args.as_mut_ptr(), kind as qjs::JSTypedArrayEnum)
            })
        }
        None => {
            let g = global(ctx);
            let data_view = get(ctx, g.raw(), "DataView")?;
            construct(ctx, data_view.raw(), &[buffer, o.raw(), l.raw()])
        }
    }
}

/// Moves `buffer`'s bytes to a new ArrayBuffer and detaches it (the
/// specs' TransferArrayBuffer).
pub(crate) fn transfer(ctx: Ctx, buffer: Raw) -> R<V> {
    // SAFETY: copied before detaching; nothing else runs in between.
    let bytes = unsafe { bytes_of(ctx, buffer) }
        .ok_or_else(|| type_error(ctx, "not an ArrayBuffer"))?
        .to_vec();
    if is_detached(ctx, buffer) {
        return Err(type_error(ctx, "the ArrayBuffer is detached"));
    }
    // SAFETY: a live ArrayBuffer.
    unsafe { qjs::JS_DetachArrayBuffer(ctx, buffer) };
    array_buffer(ctx, &bytes)
}

/// Whether ArrayBuffer `buffer` is detached.
pub(crate) fn is_detached(ctx: Ctx, buffer: Raw) -> bool {
    get(ctx, buffer, "detached")
        .map(|d| to_bool(ctx, d.raw()))
        .unwrap_or(false)
}

/// Raw pointer to an object's opaque data, of either web class.
pub(crate) fn opaque(v: Raw, class: qjs::JSClassID) -> *mut c_void {
    // SAFETY: reading the opaque of a live value (null for other classes).
    unsafe { qjs::JS_GetOpaque(v, class) }
}
