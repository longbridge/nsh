//! `nsh/shared`: the TSX face of [`nsh_core::shared::SharedStore`], the
//! state several engines share (the windows of an app, the views an
//! embedder shows side by side, a data source's thread).
//!
//! Each VM has a subscriber of its own on the engine's store (the one its
//! core's Rust components use, [`crate::Host::set_shared_store`]), made when
//! the app first watches a key, a prefix or a topic. Its wake function
//! raises a flag and wakes the engine ([`crate::Host::set_system_wake`]); the
//! engine's next turn calls [`pump`] (from `Host::run_system`), which takes
//! the changes and events at once and, in one batch of the JS graph, writes
//! the signals of the keys and prefixes the app watches and then calls its
//! listeners. So a turn sees a consistent version, and a writer never waits
//! for the VM.
//!
//! What a key or prefix has as a signal is shared by all that watch it: it
//! is made with the first user and freed with the last (the owners the
//! users run under: a component, an effect, a root; at a module's top level
//! the module's next run under hot module replacement).
//!
//! Values cross natively, with no JSON text: `null` / `undefined`, booleans,
//! numbers, strings, arrays and objects (own enumerable string keys; a
//! field that is `undefined` is left out, as `JSON.stringify` does),
//! `ArrayBuffer`s and typed arrays (their bytes, copied; read back as an
//! `ArrayBuffer`; a `DataView` is an object without fields). Functions,
//! symbols and BigInts throw a `TypeError`; nesting past [`MAX_DEPTH`] or
//! more than [`MAX_ENTRIES`] entries a `RangeError`. A value Rust shared
//! opaquely reads as `undefined` (a field of it is left out).
//!
//! A write from this VM reaches its own signals like any other: at the next
//! turn, through the store.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use nsh_core::shared::{SharedStore, SharedValue, Subscriber};
use rquickjs::qjs;
use rustc_hash::FxHashMap;

use crate::{
    modules::{Export, Item, Ts},
    web::js::{self, Ctx, R, Raw, Throw, UNDEFINED, V, closure, native, range_error, type_error},
};

/// The deepest nesting of a value written to the store.
pub(crate) const MAX_DEPTH: usize = 64;
/// The most entries (array items, object fields) of one value.
pub(crate) const MAX_ENTRIES: usize = crate::protocol::JSON_ENTRIES;

/// A watched key or prefix: its signal (a JS signal object) and the
/// number of owners using it.
struct Watched {
    signal: V,
    users: usize,
}

/// A prefix's keys and their values.
type PrefixValues = BTreeMap<Arc<str>, SharedValue>;

/// A topic's listeners: (id, function), oldest first.
type Listeners = Vec<(u32, V)>;

/// A context's side of the store.
#[derive(Default)]
pub(crate) struct SharedJs {
    /// The engine's store ([`set_store`]); a store of its own is made when
    /// none was given.
    store: RefCell<Option<SharedStore>>,
    /// This VM's reader, made with the first thing watched.
    sub: RefCell<Option<Subscriber>>,
    /// Raised by the subscriber's wake, from any thread; [`pump`] lowers it.
    woken: Arc<AtomicBool>,
    keys: RefCell<FxHashMap<Arc<str>, Watched>>,
    prefixes: RefCell<FxHashMap<Arc<str>, Watched>>,
    /// The keys and values of each watched prefix, kept up to date with
    /// the changes taken: the object a prefix's signal holds is made from
    /// it, so it is of the same version as the keys' signals.
    prefix_values: RefCell<FxHashMap<Arc<str>, PrefixValues>>,
    listeners: RefCell<FxHashMap<Arc<str>, Listeners>>,
    next_listener: Cell<u32>,
}

impl SharedJs {
    /// Frees the signals and listeners and stops the subscriber (the state
    /// is unregistered).
    pub(crate) fn free_all(&self) {
        // The subscriber first: nothing wakes the engine for a VM that is
        // going.
        drop(self.sub.borrow_mut().take());
        let keys = std::mem::take(&mut *self.keys.borrow_mut());
        drop(keys);
        let prefixes = std::mem::take(&mut *self.prefixes.borrow_mut());
        drop(prefixes);
        self.prefix_values.borrow_mut().clear();
        let listeners = std::mem::take(&mut *self.listeners.borrow_mut());
        drop(listeners);
    }
}

fn state<'a>(cx: Ctx) -> &'a SharedJs {
    // SAFETY: a projection of the state's field, by shared reference.
    unsafe { &(*crate::state::state(cx)).store }
}

/// Gives the context the engine's store (before the app runs).
pub(crate) fn set_store(cx: Ctx, store: SharedStore) {
    *state(cx).store.borrow_mut() = Some(store);
}

/// The store: the engine's, or one of the VM's own.
fn store(cx: Ctx) -> SharedStore {
    state(cx)
        .store
        .borrow_mut()
        .get_or_insert_with(SharedStore::default)
        .clone()
}

/// Runs `f` on the subscriber, made first if needed.
fn with_sub<T>(cx: Ctx, f: impl FnOnce(&Subscriber) -> T) -> T {
    let s = state(cx);
    if s.sub.borrow().is_none() {
        let store = store(cx);
        let woken = s.woken.clone();
        let system = crate::dom::shared(cx).system_wake.borrow().clone();
        let sub = store.subscribe(move || {
            woken.store(true, Ordering::Release);
            if let Some(wake) = &system {
                wake();
            }
        });
        *s.sub.borrow_mut() = Some(sub);
    }
    let guard = s.sub.borrow();
    f(guard.as_ref().expect("made above"))
}

// --- Values ---------------------------------------------------------------

/// A JS value as a [`SharedValue`] (the module's docs say what converts).
fn to_shared(cx: Ctx, v: Raw, depth: usize, entries: &mut usize) -> R<SharedValue> {
    if depth > MAX_DEPTH {
        return Err(too_big(cx));
    }
    // SAFETY: inline tag checks and conversions of a live value; the bytes
    // are copied before any JS runs.
    unsafe {
        if js::is_nullish(v) {
            return Ok(SharedValue::Null);
        }
        if qjs::JS_IsBool(v) {
            return Ok(SharedValue::Bool(js::to_bool(cx, v)));
        }
        if qjs::JS_IsNumber(v) {
            return Ok(SharedValue::Number(js::to_number(cx, v)?));
        }
        if qjs::JS_IsString(v) {
            return Ok(SharedValue::String(js::to_string(cx, v)?.into()));
        }
        if qjs::JS_IsSymbol(v) || qjs::JS_IsBigInt(v) || js::is_function(cx, v) {
            return Err(type_error(
                cx,
                "nsh/shared: functions, symbols and BigInts cannot be shared",
            ));
        }
        if qjs::JS_IsArrayBuffer(v) || qjs::JS_GetTypedArrayType(v) >= 0 {
            return Ok(SharedValue::Bytes(
                js::bytes_of(cx, v).unwrap_or_default().into(),
            ));
        }
        if qjs::JS_IsArray(v) {
            let mut len: i64 = 0;
            if qjs::JS_GetLength(cx, v, &mut len) != 0 {
                return Err(Throw);
            }
            let mut items = Vec::with_capacity((len as usize).min(1 << 16));
            for i in 0..len as u32 {
                *entries += 1;
                if *entries > MAX_ENTRIES {
                    return Err(too_big(cx));
                }
                let item = V::checked(cx, qjs::JS_GetPropertyUint32(cx, v, i))?;
                items.push(to_shared(cx, item.raw(), depth + 1, entries)?);
            }
            return Ok(SharedValue::List(items.into()));
        }
    }
    if !js::is_object(v) {
        return Err(type_error(cx, "nsh/shared: this value cannot be shared"));
    }
    let mut fields: Vec<(Arc<str>, SharedValue)> = Vec::new();
    for (key, item) in js::record(cx, v)? {
        *entries += 1;
        if *entries > MAX_ENTRIES {
            return Err(too_big(cx));
        }
        if item.is_undefined() {
            continue;
        }
        fields.push((key.into(), to_shared(cx, item.raw(), depth + 1, entries)?));
    }
    Ok(SharedValue::Map(fields.into()))
}

fn too_big(cx: Ctx) -> Throw {
    range_error(
        cx,
        &format!(
            "nsh/shared: a value is nested deeper than {MAX_DEPTH} or has more than \
             {MAX_ENTRIES} entries"
        ),
    )
}

/// The value of argument `v` (`undefined`: `null`).
fn value_arg(cx: Ctx, v: Raw) -> R<SharedValue> {
    to_shared(cx, v, 0, &mut 0)
}

/// A [`SharedValue`] as a JS value; an opaque one is `undefined`.
fn to_js(cx: Ctx, value: &SharedValue) -> R<V> {
    Ok(match value {
        SharedValue::Null => V::null(),
        SharedValue::Bool(b) => V::bool(*b),
        SharedValue::Number(n) => V::num(*n),
        SharedValue::String(s) => js::string(cx, s)?,
        SharedValue::Bytes(b) => js::array_buffer(cx, b)?,
        SharedValue::List(items) => {
            let items = items
                .iter()
                .map(|item| to_js(cx, item))
                .collect::<R<Vec<_>>>()?;
            js::new_array(cx, items)?
        }
        SharedValue::Map(fields) => {
            let object = js::new_object(cx)?;
            for (key, item) in fields.iter() {
                if matches!(item, SharedValue::Opaque(_)) {
                    continue;
                }
                js::define(cx, object.raw(), key, to_js(cx, item)?, true)?;
            }
            object
        }
        _ => V::undefined(),
    })
}

/// The value of a key: `undefined` for none (or an opaque one).
fn key_value(cx: Ctx, value: Option<&SharedValue>) -> R<V> {
    value.map_or_else(|| Ok(V::undefined()), |v| to_js(cx, v))
}

/// The object of a prefix's keys (full keys) and values.
fn prefix_object(cx: Ctx, values: &PrefixValues) -> R<V> {
    let object = js::new_object(cx)?;
    for (key, value) in values {
        if !matches!(value, SharedValue::Opaque(_)) {
            js::define(cx, object.raw(), key, to_js(cx, value)?, true)?;
        }
    }
    Ok(object)
}

fn key_arg(cx: Ctx, v: Raw, what: &str) -> R<Arc<str>> {
    // SAFETY: an inline tag check.
    if !unsafe { qjs::JS_IsString(v) } {
        return Err(type_error(cx, &format!("{what}: the key is a string")));
    }
    Ok(js::to_string(cx, v)?.into())
}

// --- Reading --------------------------------------------------------------

/// A read-only accessor of signal object `signal`: calling it reads the
/// signal, tracked.
fn accessor(cx: Ctx, signal: &V) -> R<V> {
    closure(cx, vec![signal.clone_ref()], |cx, captured, _| {
        js::call(cx, captured[0].raw(), UNDEFINED, &[])
    })
}

/// Undoes a watch when the current owner goes (or at the module's next run
/// under hot module replacement).
fn release_with_owner(cx: Ctx, release: impl Fn(Ctx) + 'static) -> R<()> {
    let off = closure(cx, Vec::new(), move |cx, _, _| {
        release(cx);
        Ok(V::undefined())
    })?;
    crate::keymap::undo_with_owner(cx, &off);
    Ok(())
}

native! {
    /// `signal(key)`.
    fn signal(cx, _this, args) {
        let key = key_arg(cx, args.get(0), "signal")?;
        let s = state(cx);
        let existing = s.keys.borrow_mut().get_mut(&key).map(|w| {
            w.users += 1;
            w.signal.clone_ref()
        });
        let signal = match existing {
            Some(signal) => signal,
            None => {
                // Watched before it is read: a change in between is marked
                // and written at the next turn.
                let current = with_sub(cx, |sub| {
                    sub.watch(key.clone());
                    sub.store().get(&key)
                });
                let signal = V::own(
                    cx,
                    crate::reactive::new_signal(cx, key_value(cx, current.as_ref())?.raw()),
                );
                s.keys.borrow_mut().insert(
                    key.clone(),
                    Watched {
                        signal: signal.clone_ref(),
                        users: 1,
                    },
                );
                signal
            }
        };
        let reader = accessor(cx, &signal)?;
        release_with_owner(cx, move |cx| release_key(cx, &key))?;
        Ok(reader)
    }
}

fn release_key(cx: Ctx, key: &Arc<str>) {
    let s = state(cx);
    let gone = {
        let mut keys = s.keys.borrow_mut();
        match keys.get_mut(key) {
            Some(w) if w.users > 1 => {
                w.users -= 1;
                None
            }
            Some(_) => keys.remove(key),
            None => None,
        }
    };
    if gone.is_some() {
        with_sub(cx, |sub| sub.unwatch(key));
    }
    // Freed with no borrow held.
    drop(gone);
}

native! {
    /// `prefix(prefix)`.
    fn prefix(cx, _this, args) {
        let prefix = key_arg(cx, args.get(0), "prefix")?;
        let s = state(cx);
        let existing = s.prefixes.borrow_mut().get_mut(&prefix).map(|w| {
            w.users += 1;
            w.signal.clone_ref()
        });
        let signal = match existing {
            Some(signal) => signal,
            None => {
                let snapshot = with_sub(cx, |sub| {
                    sub.watch_prefix(prefix.clone());
                    sub.snapshot_prefix(&prefix)
                });
                let values: PrefixValues = snapshot
                    .values
                    .into_iter()
                    .filter_map(|(k, v)| Some((k, v?)))
                    .collect();
                let object = prefix_object(cx, &values)?;
                s.prefix_values.borrow_mut().insert(prefix.clone(), values);
                let signal = V::own(cx, crate::reactive::new_signal(cx, object.raw()));
                s.prefixes.borrow_mut().insert(
                    prefix.clone(),
                    Watched {
                        signal: signal.clone_ref(),
                        users: 1,
                    },
                );
                signal
            }
        };
        let reader = accessor(cx, &signal)?;
        release_with_owner(cx, move |cx| release_prefix(cx, &prefix))?;
        Ok(reader)
    }
}

fn release_prefix(cx: Ctx, prefix: &Arc<str>) {
    let s = state(cx);
    let gone = {
        let mut prefixes = s.prefixes.borrow_mut();
        match prefixes.get_mut(prefix) {
            Some(w) if w.users > 1 => {
                w.users -= 1;
                None
            }
            Some(_) => prefixes.remove(prefix),
            None => None,
        }
    };
    if gone.is_some() {
        s.prefix_values.borrow_mut().remove(prefix);
        with_sub(cx, |sub| sub.unwatch_prefix(prefix));
    }
    drop(gone);
}

native! {
    /// `get(key)`.
    fn get(cx, _this, args) {
        let key = key_arg(cx, args.get(0), "get")?;
        key_value(cx, store(cx).get(&key).as_ref())
    }
}

native! {
    /// `version()`.
    fn version(cx, _this, args) {
        Ok(V::num(store(cx).version() as f64))
    }
}

// --- Writing --------------------------------------------------------------

native! {
    /// `set(key, value)`.
    fn set(cx, _this, args) {
        let key = key_arg(cx, args.get(0), "set")?;
        let value = value_arg(cx, args.get(1))?;
        Ok(V::num(store(cx).set(key, value) as f64))
    }
}

native! {
    /// `remove(key)`.
    fn remove(cx, _this, args) {
        let key = key_arg(cx, args.get(0), "remove")?;
        Ok(V::num(store(cx).delete(key) as f64))
    }
}

/// The changes a JS transaction collected, in order.
type Pending = Rc<RefCell<Vec<(Arc<str>, Option<SharedValue>)>>>;

native! {
    /// `transaction(fn)`.
    fn transaction(cx, _this, args) {
        let f = args.get(0);
        if !js::is_function(cx, f) {
            return Err(type_error(cx, "transaction: expected a function"));
        }
        let pending = Pending::default();
        let open = Rc::new(Cell::new(true));
        let tx = js::new_object(cx)?;
        let (changes, live) = (pending.clone(), open.clone());
        let set_fn = closure(cx, Vec::new(), move |cx, _, args| {
            if !live.get() {
                return Err(type_error(cx, "transaction: it is finished"));
            }
            let key = key_arg(cx, args.get(0), "tx.set")?;
            let value = value_arg(cx, args.get(1))?;
            changes.borrow_mut().push((key, Some(value)));
            Ok(V::undefined())
        })?;
        js::define(cx, tx.raw(), "set", set_fn, true)?;
        let (changes, live) = (pending.clone(), open.clone());
        let remove_fn = closure(cx, Vec::new(), move |cx, _, args| {
            if !live.get() {
                return Err(type_error(cx, "transaction: it is finished"));
            }
            let key = key_arg(cx, args.get(0), "tx.remove")?;
            changes.borrow_mut().push((key, None));
            Ok(V::undefined())
        })?;
        js::define(cx, tx.raw(), "remove", remove_fn, true)?;
        let ran = js::call(cx, f, UNDEFINED, &[tx.raw()]);
        open.set(false);
        // A throw commits nothing and reaches the caller.
        ran?;
        let changes = std::mem::take(&mut *pending.borrow_mut());
        let version = store(cx).transaction(|t| {
            for (key, value) in changes {
                match value {
                    Some(value) => t.set(key, value),
                    None => t.delete(key),
                }
            }
        });
        Ok(V::num(version as f64))
    }
}

native! {
    /// `emit(topic, value)`.
    fn emit(cx, _this, args) {
        let topic = key_arg(cx, args.get(0), "emit")?;
        let value = value_arg(cx, args.get(1))?;
        store(cx).emit(topic, value);
        Ok(V::undefined())
    }
}

native! {
    /// `on(topic, fn)`.
    fn on(cx, _this, args) {
        let topic = key_arg(cx, args.get(0), "on")?;
        if !js::is_function(cx, args.get(1)) {
            return Err(type_error(cx, "on: the listener is not a function"));
        }
        let s = state(cx);
        let id = s.next_listener.get() + 1;
        s.next_listener.set(id);
        let first = {
            let mut listeners = s.listeners.borrow_mut();
            let list = listeners.entry(topic.clone()).or_default();
            list.push((id, V::dup(cx, args.get(1))));
            list.len() == 1
        };
        if first {
            with_sub(cx, |sub| sub.listen(topic.clone()));
        }
        let stop = closure(cx, Vec::new(), move |cx, _, _| {
            stop_listening(cx, &topic, id);
            Ok(V::undefined())
        })?;
        crate::keymap::undo_with_owner(cx, &stop);
        Ok(stop)
    }
}

fn stop_listening(cx: Ctx, topic: &Arc<str>, id: u32) {
    let s = state(cx);
    let (gone, last) = {
        let mut listeners = s.listeners.borrow_mut();
        let Some(list) = listeners.get_mut(topic) else {
            return;
        };
        let gone = list
            .iter()
            .position(|(i, _)| *i == id)
            .map(|at| list.remove(at));
        let last = list.is_empty();
        if last {
            listeners.remove(topic);
        }
        (gone, last)
    };
    if gone.is_some() && last {
        with_sub(cx, |sub| sub.unlisten(topic));
    }
    drop(gone);
}

// --- The turn -------------------------------------------------------------

/// Takes in what changed in the store since the last turn: writes the
/// signals of the watched keys and prefixes (as one batch of the graph, so
/// what reads several sees them together) and calls the listeners of the
/// events, in order. A listener that throws is reported, the others still
/// run. Cheap when nothing changed: a flag the subscriber's wake raised.
pub(crate) fn pump(cx: Ctx) {
    let s = state(cx);
    if !s.woken.swap(false, Ordering::AcqRel) {
        return;
    }
    let (changes, events) = {
        let sub = s.sub.borrow();
        let Some(sub) = sub.as_ref() else {
            return;
        };
        (sub.take_changes(), sub.take_events())
    };
    // The prefixes' keys brought to the changes' version (not read from the
    // store again: it may be at a later one by now).
    let mut snapshots: Vec<(Arc<str>, PrefixValues)> = Vec::new();
    for (prefix, values) in s.prefix_values.borrow_mut().iter_mut() {
        let mut hit = false;
        for (key, value) in &changes.values {
            if !key.starts_with(&**prefix) {
                continue;
            }
            hit = true;
            match value {
                Some(value) => {
                    values.insert(key.clone(), value.clone());
                }
                None => {
                    values.remove(key);
                }
            }
        }
        if hit {
            snapshots.push((prefix.clone(), values.clone()));
        }
    }
    // The values as JS, before the graph's batch: a value that cannot be
    // made (never for what the store holds) leaves its signal as it was.
    let mut writes: Vec<(V, V)> = Vec::new();
    for (key, value) in &changes.values {
        let signal = s.keys.borrow().get(key).map(|w| w.signal.clone_ref());
        if let Some(signal) = signal {
            match key_value(cx, value.as_ref()) {
                Ok(value) => writes.push((signal, value)),
                Err(Throw) => js::report_exception(cx),
            }
        }
    }
    for (prefix, snapshot) in &snapshots {
        let signal = s
            .prefixes
            .borrow()
            .get(prefix)
            .map(|w| w.signal.clone_ref());
        if let Some(signal) = signal {
            match prefix_object(cx, snapshot) {
                Ok(value) => writes.push((signal, value)),
                Err(Throw) => js::report_exception(cx),
            }
        }
    }
    crate::reactive::batch_with(cx, || {
        for (signal, value) in writes {
            crate::reactive::set_signal(cx, signal.raw(), value.take());
        }
        for event in events {
            dispatch(cx, &event.topic, &event.value);
        }
    });
    let failed = crate::reactive::finish_value(cx, UNDEFINED);
    // SAFETY: an inline tag check.
    if unsafe { qjs::JS_IsException(failed) } {
        js::report_exception(cx);
    }
}

/// Calls the listeners of `topic` with `value`, oldest first (those still
/// there when their turn comes).
fn dispatch(cx: Ctx, topic: &Arc<str>, value: &SharedValue) {
    let s = state(cx);
    let called: Vec<(u32, V)> = s
        .listeners
        .borrow()
        .get(topic)
        .map(|list| list.iter().map(|(i, f)| (*i, f.clone_ref())).collect())
        .unwrap_or_default();
    if called.is_empty() {
        return;
    }
    let Ok(arg) = to_js(cx, value) else {
        js::report_exception(cx);
        return;
    };
    for (id, f) in called {
        let present = s
            .listeners
            .borrow()
            .get(topic)
            .is_some_and(|list| list.iter().any(|(i, _)| *i == id));
        if present && js::call(cx, f.raw(), UNDEFINED, &[arg.raw()]).is_err() {
            js::report_exception(cx);
        }
    }
}

// --- The module -----------------------------------------------------------

/// `nsh/shared`'s exports.
pub(crate) const EXPORTS: &[Item] = &[
    (
        "signal",
        Export::Function(signal, 1),
        Ts::function(
            "The value of the shared key `key`, as an accessor that follows the shared store (the\nstate several windows, views and threads share): it runs again when a transaction changes\nthe key, at the surface's next turn, together with the other keys that changed with it.\n`undefined` while the key has no value or holds a value only Rust reads. Watched while the\ncurrent owner (a component, an effect) lives.",
            &["<T = unknown>(key: string): Accessor<T | undefined>"],
        ),
    ),
    (
        "prefix",
        Export::Function(prefix, 1),
        Ts::function(
            "The shared keys starting with `prefix` and their values, as an accessor: a new object\n(full keys as its fields) each time one of them changes. Watched while the current owner\nlives.",
            &["<T = unknown>(prefix: string): Accessor<Record<string, T>>"],
        ),
    ),
    (
        "get",
        Export::Function(get, 1),
        Ts::function(
            "The value of the shared key `key` now (not tracked), `undefined` when it has none.",
            &["<T = unknown>(key: string): T | undefined"],
        ),
    ),
    (
        "set",
        Export::Function(set, 2),
        Ts::function(
            "Sets the shared key `key` to `value` (a transaction of one change); returns the store's\nversion after. A value is `null`, `undefined` (as `null`), a boolean, a number, a string,\nan array or object of values, or an `ArrayBuffer` or typed array (its bytes, read back as\nan `ArrayBuffer`); functions and symbols throw a `TypeError`.",
            &["(key: string, value: unknown): number"],
        ),
    ),
    (
        "remove",
        Export::Function(remove, 1),
        Ts::function(
            "Deletes the shared key `key` (a transaction of one change); returns the store's version\nafter.",
            &["(key: string): number"],
        ),
    ),
    (
        "transaction",
        Export::Function(transaction, 1),
        Ts::function(
            "Runs `fn` with a transaction and commits what it set and removed as one version, once\n`fn` returns: readers see all of it or none. If `fn` throws, nothing is committed and the\nerror propagates. Returns the store's version after.",
            &["(fn: (tx: SharedTransaction) => void): number"],
        ),
    ),
    (
        "version",
        Export::Function(version, 0),
        Ts::function(
            "The version of the shared store's last transaction (0: none yet).",
            &["(): number"],
        ),
    ),
    (
        "emit",
        Export::Function(emit, 2),
        Ts::function(
            "Sends `value` to every listener of `topic` in every surface that shares the store (this\none too), in the order emitted. Events are one-off messages, not state: they are not kept\nfor listeners that come later.",
            &["(topic: string, value?: unknown): void"],
        ),
    ),
    (
        "on",
        Export::Function(on, 2),
        Ts::function(
            "Listens to the events of `topic`: `fn` gets each value, at the surface's next turn, in\norder. Returns a function that stops listening; it also stops with the current owner.",
            &["<T = unknown>(topic: string, fn: (value: T) => void): () => void"],
        ),
    ),
];

/// `nsh/shared`'s types.
pub(crate) const TYPES: &str = r#"/** The changes of one `transaction`, committed together when its function returns. */
export interface SharedTransaction {
  /** Sets `key` to `value`. */
  set(key: string, value: unknown): void;
  /** Deletes `key`. */
  remove(key: string): void;
}
"#;

#[cfg(test)]
mod tests {
    use nsh_core::shared::SharedStore;

    use crate::Host;

    /// Keys, prefixes and events the store changes reach the app's accessors
    /// and listeners when the engine's turn runs `run_system`, and the app's
    /// writes are the store's at once.
    #[test]
    fn the_store_reaches_accessors_and_listeners_at_the_turn() {
        let store = SharedStore::new();
        store.set("row/z", 26.0);
        let host = Host::new().unwrap();
        host.set_shared_store(store.clone());
        host.eval_source(
            r#"
            import { signal, prefix, on, set } from "nsh/shared";
            globalThis.k = signal("k");
            globalThis.rows = prefix("row/");
            globalThis.heard = [];
            on("ping", (v) => heard.push(v));
            set("from-js", { a: [1, "b"] });
        "#,
        )
        .unwrap();
        assert!(
            store.get("from-js").is_some(),
            "a write is the store's at once"
        );
        store.transaction(|tx| {
            tx.set("k", 7.0);
            tx.set("row/a", 1.0);
        });
        store.emit("ping", "hi");
        // Nothing before the turn takes them in.
        host.eval_source(r#"if (k() !== undefined) throw Error("k changed before the turn");"#)
            .unwrap();
        host.run_system();
        host.eval_source(
            r#"
            if (k() !== 7) throw Error("k " + k());
            if (Object.keys(rows()).sort().join() !== "row/a,row/z") throw Error("rows");
            if (heard.join() !== "hi") throw Error("heard " + heard);
        "#,
        )
        .unwrap();
    }
}
