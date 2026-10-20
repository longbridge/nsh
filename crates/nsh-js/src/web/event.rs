//! `Event`, `CustomEvent`, `EventTarget`, `AbortController` and
//! `AbortSignal` (DOM Standard). A target has no tree: dispatch reaches
//! the target only (capture listeners first, as the DOM does at the
//! target).

use std::{cell::Cell, rc::Rc};

use super::{
    Class, Key, Object, Spec, data, instance,
    js::{self, Ctx, R, Raw, V, native, trace},
    role,
};

const AT_TARGET: f64 = 2.0;

/// An event (`CustomEvent`s too: `detail`).
pub(crate) struct Event {
    kind: String,
    bubbles: bool,
    cancelable: bool,
    composed: bool,
    trusted: bool,
    /// `defaultPrevented`.
    canceled: bool,
    stop_propagation: bool,
    stop_immediate: bool,
    in_passive: bool,
    dispatching: bool,
    phase: u8,
    target: V,
    current_target: V,
    time: f64,
    detail: V,
}

trace!(Event {
    target,
    current_target,
    detail
});

impl Object for Event {}

/// A listener of a target.
struct Listener {
    kind: String,
    /// The function or `{ handleEvent }` (undefined for an event handler
    /// attribute, `onabort`: its current value is called).
    callback: V,
    capture: bool,
    once: bool,
    passive: bool,
    /// Removed (a dispatch in progress skips it).
    removed: Rc<Cell<bool>>,
}

trace!(Listener { callback });

/// The `EventTarget` part of an object.
#[derive(Default)]
pub(crate) struct Target {
    listeners: Vec<Listener>,
    /// Event handler attributes (`onabort`), by event type.
    handlers: Vec<(String, V)>,
}

trace!(Target {
    listeners,
    handlers
});

impl Object for Target {
    fn target(&mut self) -> Option<&mut Target> {
        Some(self)
    }
}

impl Target {
    fn remove(&mut self, kind: &str, callback: Raw, capture: bool) {
        if let Some(i) = self.listeners.iter().position(|l| {
            l.kind == kind && l.capture == capture && js::same(l.callback.raw(), callback)
        }) {
            let l = self.listeners.remove(i);
            l.removed.set(true);
        }
    }

    /// Sets event handler attribute `kind` (`null` removes it).
    pub(crate) fn set_handler(&mut self, ctx: Ctx, kind: &str, value: Raw) {
        let callable = js::is_object(value);
        match self.handlers.iter_mut().find(|(k, _)| k == kind) {
            Some((_, v)) if callable => *v = V::dup(ctx, value),
            Some(_) => {
                self.handlers.retain(|(k, _)| k != kind);
                if let Some(i) = self
                    .listeners
                    .iter()
                    .position(|l| l.kind == kind && l.callback.is_undefined())
                {
                    self.listeners.remove(i).removed.set(true);
                }
            }
            None if callable => {
                self.handlers.push((kind.to_owned(), V::dup(ctx, value)));
                self.listeners.push(Listener {
                    kind: kind.to_owned(),
                    callback: V::undefined(),
                    capture: false,
                    once: false,
                    passive: false,
                    removed: Rc::default(),
                });
            }
            None => {}
        }
    }

    pub(crate) fn handler(&self, ctx: Ctx, kind: &str) -> V {
        self.handlers
            .iter()
            .find(|(k, _)| k == kind)
            .map_or_else(V::null, |(_, v)| V::dup(ctx, v.raw()))
    }
}

fn target<'a>(ctx: Ctx, v: Raw) -> R<std::cell::RefMut<'a, Target>> {
    role(ctx, v, "EventTarget", |o| o.target())
}

/// `performance.now()`.
fn now(ctx: Ctx) -> f64 {
    let g = js::global(ctx);
    js::get(ctx, g.raw(), "performance")
        .and_then(|p| js::invoke(ctx, p.raw(), "now", &[]))
        .and_then(|n| js::to_number(ctx, n.raw()))
        .unwrap_or(0.0)
}

fn new_event_data(
    ctx: Ctx,
    kind: String,
    bubbles: bool,
    cancelable: bool,
    composed: bool,
) -> Event {
    Event {
        kind,
        bubbles,
        cancelable,
        composed,
        trusted: false,
        canceled: false,
        stop_propagation: false,
        stop_immediate: false,
        in_passive: false,
        dispatching: false,
        phase: 0,
        target: V::null(),
        current_target: V::null(),
        time: now(ctx),
        detail: V::null(),
    }
}

/// An `EventInit`'s flags.
fn event_init(ctx: Ctx, init: Raw) -> R<(bool, bool, bool)> {
    if js::is_nullish(init) {
        return Ok((false, false, false));
    }
    if !js::is_object(init) {
        return Err(js::type_error(ctx, "EventInit must be an object"));
    }
    let flag = |name| js::get(ctx, init, name).map(|v| js::to_bool(ctx, v.raw()));
    Ok((flag("bubbles")?, flag("cancelable")?, flag("composed")?))
}

native!(
    fn event_constructor(ctx, this, args) {
        if args.len() == 0 {
            return Err(js::type_error(ctx, "Event requires a type"));
        }
        let kind = js::to_string(ctx, args.get(0))?;
        let (bubbles, cancelable, composed) = event_init(ctx, args.get(1))?;
        let data = new_event_data(ctx, kind, bubbles, cancelable, composed);
        instance(ctx, this, Class::Event, data)
    }
);

native!(
    fn custom_event_constructor(ctx, this, args) {
        if args.len() == 0 {
            return Err(js::type_error(ctx, "CustomEvent requires a type"));
        }
        let kind = js::to_string(ctx, args.get(0))?;
        let init = args.get(1);
        let (bubbles, cancelable, composed) = event_init(ctx, init)?;
        let mut data = new_event_data(ctx, kind, bubbles, cancelable, composed);
        if js::is_object(init) {
            data.detail = js::get(ctx, init, "detail")?;
            if data.detail.is_undefined() {
                data.detail = V::null();
            }
        }
        instance(ctx, this, Class::CustomEvent, data)
    }
);

fn event<'a>(ctx: Ctx, v: Raw) -> R<std::cell::RefMut<'a, Event>> {
    data::<Event>(ctx, v, "Event")
}

macro_rules! event_getter {
    ($name:ident, |$ctx:ident, $e:ident| $body:expr) => {
        native!(
            fn $name($ctx, this, args) {
                let $e = event($ctx, this)?;
                Ok($body)
            }
        );
    };
}

event_getter!(event_type, |ctx, e| js::string(ctx, &e.kind)?);
event_getter!(event_target, |ctx, e| V::dup(ctx, e.target.raw()));
event_getter!(event_current_target, |ctx, e| V::dup(
    ctx,
    e.current_target.raw()
));
event_getter!(event_phase, |ctx, e| V::num(f64::from(e.phase)));
event_getter!(event_bubbles, |ctx, e| V::bool(e.bubbles));
event_getter!(event_cancelable, |ctx, e| V::bool(e.cancelable));
event_getter!(event_default_prevented, |ctx, e| V::bool(e.canceled));
event_getter!(event_composed, |ctx, e| V::bool(e.composed));
event_getter!(event_is_trusted, |ctx, e| V::bool(e.trusted));
event_getter!(event_time_stamp, |ctx, e| V::num(e.time));
event_getter!(event_return_value, |ctx, e| V::bool(!e.canceled));
event_getter!(event_cancel_bubble, |ctx, e| V::bool(e.stop_propagation));
event_getter!(event_detail, |ctx, e| V::dup(ctx, e.detail.raw()));

impl Event {
    fn prevent_default(&mut self) {
        if self.cancelable && !self.in_passive {
            self.canceled = true;
        }
    }
}

native!(
    fn event_set_return_value(ctx, this, args) {
        if !js::to_bool(ctx, args.get(0)) {
            event(ctx, this)?.prevent_default();
        }
        Ok(V::undefined())
    }
);

native!(
    fn event_set_cancel_bubble(ctx, this, args) {
        if js::to_bool(ctx, args.get(0)) {
            event(ctx, this)?.stop_propagation = true;
        }
        Ok(V::undefined())
    }
);

native!(
    fn event_prevent_default(ctx, this, args) {
        event(ctx, this)?.prevent_default();
        Ok(V::undefined())
    }
);

native!(
    fn event_stop_propagation(ctx, this, args) {
        event(ctx, this)?.stop_propagation = true;
        Ok(V::undefined())
    }
);

native!(
    fn event_stop_immediate_propagation(ctx, this, args) {
        let mut e = event(ctx, this)?;
        e.stop_propagation = true;
        e.stop_immediate = true;
        Ok(V::undefined())
    }
);

native!(
    fn event_composed_path(ctx, this, args) {
        let current = {
            let e = event(ctx, this)?;
            (!e.current_target.is_nullish()).then(|| V::dup(ctx, e.current_target.raw()))
        };
        js::new_array(ctx, current.into_iter().collect())
    }
);

native!(
    /// `initEvent(type, bubbles, cancelable)` (legacy).
    fn event_init_event(ctx, this, args) {
        let kind = js::to_string(ctx, args.get(0))?;
        let mut e = event(ctx, this)?;
        if !e.dispatching {
            e.kind = kind;
            e.bubbles = js::to_bool(ctx, args.get(1));
            e.cancelable = js::to_bool(ctx, args.get(2));
            e.canceled = false;
            e.stop_propagation = false;
            e.stop_immediate = false;
            e.trusted = false;
            e.target = V::null();
        }
        Ok(V::undefined())
    }
);

// --- EventTarget ----------------------------------------------------------

native!(
    fn target_constructor(ctx, this, args) {
        instance(ctx, this, Class::EventTarget, Target::default())
    }
);

/// The flags of `addEventListener`'s options: capture, once, passive,
/// signal.
fn listener_options(ctx: Ctx, options: Raw) -> R<(bool, bool, bool, Option<V>)> {
    if !js::is_object(options) {
        return Ok((js::to_bool(ctx, options), false, false, None));
    }
    let flag = |name| js::get(ctx, options, name).map(|v| js::to_bool(ctx, v.raw()));
    let (capture, once, passive) = (flag("capture")?, flag("once")?, flag("passive")?);
    let signal = js::get(ctx, options, "signal")?;
    let signal = if signal.is_undefined() {
        None
    } else if super::is::<Signal>(ctx, signal.raw()) {
        Some(signal)
    } else {
        return Err(js::type_error(ctx, "signal is not an AbortSignal"));
    };
    Ok((capture, once, passive, signal))
}

/// `removeEventListener`'s capture flag.
fn capture_option(ctx: Ctx, options: Raw) -> R<bool> {
    if js::is_object(options) {
        let c = js::get(ctx, options, "capture")?;
        Ok(js::to_bool(ctx, c.raw()))
    } else {
        Ok(js::to_bool(ctx, options))
    }
}

native!(
    fn add_event_listener(ctx, this, args) {
        let kind = js::to_string(ctx, args.get(0))?;
        let callback = args.get(1);
        let (capture, once, passive, signal) = listener_options(ctx, args.get(2))?;
        if js::is_nullish(callback) {
            return Ok(V::undefined());
        }
        if !js::is_object(callback) {
            return Err(js::type_error(ctx, "the listener is not an object"));
        }
        if let Some(s) = &signal
            && aborted(ctx, s.raw())?
        {
            return Ok(V::undefined());
        }
        {
            let mut t = target(ctx, this)?;
            let exists = t.listeners.iter().any(|l| {
                l.kind == kind && l.capture == capture && js::same(l.callback.raw(), callback)
            });
            if exists {
                return Ok(V::undefined());
            }
            t.listeners.push(Listener {
                kind: kind.clone(),
                callback: V::dup(ctx, callback),
                capture,
                once,
                passive,
                removed: Rc::default(),
            });
        }
        if let Some(s) = signal {
            // Aborting the signal removes the listener.
            let captures = vec![V::dup(ctx, this), V::dup(ctx, callback)];
            let remove = js::closure(ctx, captures, move |ctx, c, _| {
                target(ctx, c[0].raw())?.remove(&kind, c[1].raw(), capture);
                Ok(V::undefined())
            })?;
            add_algorithm(ctx, s.raw(), remove)?;
        }
        Ok(V::undefined())
    }
);

native!(
    fn remove_event_listener(ctx, this, args) {
        let kind = js::to_string(ctx, args.get(0))?;
        let capture = capture_option(ctx, args.get(2))?;
        target(ctx, this)?.remove(&kind, args.get(1), capture);
        Ok(V::undefined())
    }
);

native!(
    fn dispatch_event(ctx, this, args) {
        let e = args.get(0);
        if !super::is::<Event>(ctx, e) {
            return Err(js::type_error(ctx, "dispatchEvent needs an Event"));
        }
        event(ctx, e)?.trusted = false;
        dispatch(ctx, this, e).map(V::bool)
    }
);

/// A listener as a dispatch saw it: callback, capture, once, passive, and
/// its removed flag.
type Snapshot = (V, bool, bool, bool, Rc<Cell<bool>>);

/// Dispatches event `e` at `target`: whether it was not canceled.
pub(crate) fn dispatch(ctx: Ctx, target_obj: Raw, e: Raw) -> R<bool> {
    let kind = {
        let mut ev = event(ctx, e)?;
        if ev.dispatching {
            return Err(js::throw_dom(
                ctx,
                "The event is already being dispatched.",
                "InvalidStateError",
            ));
        }
        ev.dispatching = true;
        ev.target = V::dup(ctx, target_obj);
        ev.current_target = V::dup(ctx, target_obj);
        ev.phase = AT_TARGET as u8;
        ev.kind.clone()
    };
    // The listeners as they are now; removals during dispatch still apply.
    let snapshot: Vec<Snapshot> = target(ctx, target_obj)?
        .listeners
        .iter()
        .filter(|l| l.kind == kind)
        .map(|l| {
            (
                l.callback.clone_ref(),
                l.capture,
                l.once,
                l.passive,
                l.removed.clone(),
            )
        })
        .collect();
    'phases: for capture_phase in [true, false] {
        for (callback, capture, once, passive, removed) in &snapshot {
            if *capture != capture_phase || removed.get() {
                continue;
            }
            if *once {
                target(ctx, target_obj)?.remove(&kind, callback.raw(), *capture);
            }
            let callback = if callback.is_undefined() {
                // An event handler attribute: its current value.
                target(ctx, target_obj)?.handler(ctx, &kind)
            } else {
                callback.clone_ref()
            };
            event(ctx, e)?.in_passive = *passive;
            let result = if js::is_function(ctx, callback.raw()) {
                js::call(ctx, callback.raw(), target_obj, &[e])
            } else if js::is_object(callback.raw()) {
                js::get(ctx, callback.raw(), "handleEvent").and_then(|h| {
                    if js::is_function(ctx, h.raw()) {
                        js::call(ctx, h.raw(), callback.raw(), &[e])
                    } else {
                        Err(js::type_error(ctx, "handleEvent is not a function"))
                    }
                })
            } else {
                Ok(V::undefined())
            };
            if result.is_err() {
                js::report_exception(ctx);
            }
            let mut ev = event(ctx, e)?;
            ev.in_passive = false;
            if ev.stop_immediate {
                break 'phases;
            }
        }
    }
    let mut ev = event(ctx, e)?;
    ev.dispatching = false;
    ev.phase = 0;
    ev.current_target = V::null();
    ev.stop_propagation = false;
    ev.stop_immediate = false;
    Ok(!ev.canceled)
}

/// Fires a trusted event `kind` at `target`.
pub(crate) fn fire(ctx: Ctx, target_obj: Raw, kind: &str) -> R<bool> {
    let e = trusted(ctx, kind)?;
    dispatch(ctx, target_obj, e.raw())
}

/// A new trusted event `kind`, not dispatched yet.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn trusted(ctx: Ctx, kind: &str) -> R<V> {
    let mut data = new_event_data(ctx, kind.to_owned(), false, false, false);
    data.trusted = true;
    instance(ctx, js::UNDEFINED, Class::Event, data)
}

// --- AbortSignal ----------------------------------------------------------

/// An abort signal.
#[derive(Default)]
pub(crate) struct Signal {
    target: Target,
    aborted: bool,
    reason: V,
    /// What runs on abort (the specs' abort algorithms), by id.
    algorithms: Vec<(u32, V)>,
    next_algorithm: u32,
    /// `AbortSignal.any`'s: made from other signals.
    dependent: bool,
    sources: Vec<V>,
    /// `WeakRef`s of the signals made from this one.
    dependents: Vec<V>,
}

trace!(Signal {
    target,
    reason,
    algorithms,
    sources,
    dependents
});

impl Object for Signal {
    fn target(&mut self) -> Option<&mut Target> {
        Some(&mut self.target)
    }
}

fn signal<'a>(ctx: Ctx, v: Raw) -> R<std::cell::RefMut<'a, Signal>> {
    data::<Signal>(ctx, v, "AbortSignal")
}

/// A new signal, not aborted.
pub(crate) fn new_signal(ctx: Ctx) -> R<V> {
    instance(ctx, js::UNDEFINED, Class::AbortSignal, Signal::default())
}

/// Whether `v` is an AbortSignal.
pub(crate) fn is_signal(ctx: Ctx, v: Raw) -> bool {
    super::is::<Signal>(ctx, v)
}

/// Whether signal `s` is aborted.
pub(crate) fn aborted(ctx: Ctx, s: Raw) -> R<bool> {
    Ok(signal(ctx, s)?.aborted)
}

/// Signal `s`'s abort reason.
pub(crate) fn reason(ctx: Ctx, s: Raw) -> R<V> {
    Ok(signal(ctx, s)?.reason.clone_ref())
}

/// Runs `f` when signal `s` aborts; its id, to remove it.
pub(crate) fn add_algorithm(ctx: Ctx, s: Raw, f: V) -> R<u32> {
    let mut sig = signal(ctx, s)?;
    if sig.aborted {
        return Ok(u32::MAX);
    }
    sig.next_algorithm += 1;
    let id = sig.next_algorithm;
    sig.algorithms.push((id, f));
    Ok(id)
}

/// Removes abort algorithm `id` of `s`.
pub(crate) fn remove_algorithm(ctx: Ctx, s: Raw, id: u32) -> R<()> {
    let removed = {
        let mut sig = signal(ctx, s)?;
        sig.algorithms
            .iter()
            .position(|(i, _)| *i == id)
            .map(|i| sig.algorithms.remove(i))
    };
    // Freed after the borrow (a finalizer may run).
    drop(removed);
    Ok(())
}

fn default_reason(ctx: Ctx) -> R<V> {
    js::dom_exception(ctx, "signal is aborted without reason", "AbortError")
}

/// Aborts signal `s` with `reason` (`undefined`: an `AbortError`).
pub(crate) fn abort(ctx: Ctx, s: Raw, reason: Raw) -> R<()> {
    let reason = if js::is_undefined(reason) {
        default_reason(ctx)?
    } else {
        V::dup(ctx, reason)
    };
    let weak = {
        let mut sig = signal(ctx, s)?;
        if sig.aborted {
            return Ok(());
        }
        sig.aborted = true;
        sig.reason = reason.clone_ref();
        std::mem::take(&mut sig.dependents)
    };
    let mut dependents = Vec::new();
    for w in weak {
        let d = js::invoke(ctx, w.raw(), "deref", &[])?;
        if js::is_undefined(d.raw()) {
            continue;
        }
        let mut dep = signal(ctx, d.raw())?;
        if !dep.aborted {
            dep.aborted = true;
            dep.reason = reason.clone_ref();
            drop(dep);
            dependents.push(d);
        }
    }
    run_abort_steps(ctx, s)?;
    for d in dependents {
        run_abort_steps(ctx, d.raw())?;
    }
    Ok(())
}

fn run_abort_steps(ctx: Ctx, s: Raw) -> R<()> {
    let algorithms = std::mem::take(&mut signal(ctx, s)?.algorithms);
    for (_, f) in algorithms {
        if js::call(ctx, f.raw(), js::UNDEFINED, &[]).is_err() {
            js::report_exception(ctx);
        }
    }
    fire(ctx, s, "abort")?;
    Ok(())
}

native!(
    fn signal_aborted(ctx, this, args) {
        Ok(V::bool(signal(ctx, this)?.aborted))
    }
);

native!(
    fn signal_reason(ctx, this, args) {
        Ok(signal(ctx, this)?.reason.clone_ref())
    }
);

native!(
    fn signal_throw_if_aborted(ctx, this, args) {
        let sig = signal(ctx, this)?;
        if sig.aborted {
            let reason = sig.reason.clone_ref();
            drop(sig);
            return Err(js::throw(ctx, reason));
        }
        Ok(V::undefined())
    }
);

native!(
    fn signal_onabort(ctx, this, args) {
        Ok(target(ctx, this)?.handler(ctx, "abort"))
    }
);

native!(
    fn signal_set_onabort(ctx, this, args) {
        target(ctx, this)?.set_handler(ctx, "abort", args.get(0));
        Ok(V::undefined())
    }
);

native!(
    fn signal_abort(ctx, this, args) {
        let s = new_signal(ctx)?;
        abort(ctx, s.raw(), args.get(0))?;
        Ok(s)
    }
);

native!(
    fn signal_timeout(ctx, this, args) {
        let ms = js::to_number(ctx, args.get(0))?;
        if !ms.is_finite() || ms < 0.0 {
            return Err(js::type_error(ctx, "the timeout must be a non-negative number"));
        }
        let s = new_signal(ctx)?;
        let fire = js::closure(ctx, vec![s.clone_ref()], |ctx, c, _| {
            let reason = js::dom_exception(ctx, "signal timed out", "TimeoutError")?;
            abort(ctx, c[0].raw(), reason.raw())?;
            Ok(V::undefined())
        })?;
        let g = js::global(ctx);
        let set_timeout = js::get(ctx, g.raw(), "setTimeout")?;
        js::call(ctx, set_timeout.raw(), js::UNDEFINED, &[fire.raw(), V::num(ms).raw()])?;
        Ok(s)
    }
);

native!(
    fn signal_any(ctx, this, args) {
        let mut signals = Vec::new();
        js::for_each(ctx, args.get(0), |s| {
            if !is_signal(ctx, s.raw()) {
                return Err(js::type_error(ctx, "AbortSignal.any takes AbortSignals"));
            }
            signals.push(s);
            Ok(())
        })?;
        let result = new_signal(ctx)?;
        for s in &signals {
            let sig = signal(ctx, s.raw())?;
            if sig.aborted {
                let reason = sig.reason.clone_ref();
                drop(sig);
                let mut r = signal(ctx, result.raw())?;
                r.aborted = true;
                r.reason = reason;
                drop(r);
                return Ok(result);
            }
        }
        signal(ctx, result.raw())?.dependent = true;
        let g = js::global(ctx);
        let weak_ref = js::get(ctx, g.raw(), "WeakRef")?;
        for s in &signals {
            // A dependent signal's sources stand for it.
            let sources = {
                let sig = signal(ctx, s.raw())?;
                if sig.dependent {
                    sig.sources.iter().map(V::clone_ref).collect()
                } else {
                    vec![s.clone_ref()]
                }
            };
            for source in sources {
                let already = signal(ctx, result.raw())?
                    .sources
                    .iter()
                    .any(|x| js::same(x.raw(), source.raw()));
                if already {
                    continue;
                }
                let weak = js::construct(ctx, weak_ref.raw(), &[result.raw()])?;
                let stale = {
                    let mut src = signal(ctx, source.raw())?;
                    src.dependents.push(weak);
                    // Dependents that were collected: dropped now and then.
                    src.dependents.len() > 32
                };
                if stale {
                    prune(ctx, source.raw())?;
                }
                signal(ctx, result.raw())?.sources.push(source);
            }
        }
        Ok(result)
    }
);

/// Drops the collected dependents of `s`.
fn prune(ctx: Ctx, s: Raw) -> R<()> {
    let weak = std::mem::take(&mut signal(ctx, s)?.dependents);
    let mut alive = Vec::with_capacity(weak.len());
    for w in weak {
        let d = js::invoke(ctx, w.raw(), "deref", &[])?;
        if !js::is_undefined(d.raw()) {
            alive.push(w);
        }
    }
    signal(ctx, s)?.dependents.extend(alive);
    Ok(())
}

// --- AbortController ------------------------------------------------------

struct Controller {
    signal: V,
}

trace!(Controller { signal });

impl Object for Controller {}

native!(
    fn controller_constructor(ctx, this, args) {
        let signal = new_signal(ctx)?;
        instance(ctx, this, Class::AbortController, Controller { signal })
    }
);

native!(
    fn controller_signal(ctx, this, args) {
        Ok(data::<Controller>(ctx, this, "AbortController")?.signal.clone_ref())
    }
);

native!(
    fn controller_abort(ctx, this, args) {
        let s = data::<Controller>(ctx, this, "AbortController")?.signal.clone_ref();
        abort(ctx, s.raw(), args.get(0))?;
        Ok(V::undefined())
    }
);

const EVENT_ACCESSORS: &[(&std::ffi::CStr, js::Native, Option<js::Native>)] = &[
    (c"type", event_type, None),
    (c"target", event_target, None),
    (c"srcElement", event_target, None),
    (c"currentTarget", event_current_target, None),
    (c"eventPhase", event_phase, None),
    (c"bubbles", event_bubbles, None),
    (c"cancelable", event_cancelable, None),
    (c"defaultPrevented", event_default_prevented, None),
    (c"composed", event_composed, None),
    (c"isTrusted", event_is_trusted, None),
    (c"timeStamp", event_time_stamp, None),
    (
        c"returnValue",
        event_return_value,
        Some(event_set_return_value),
    ),
    (
        c"cancelBubble",
        event_cancel_bubble,
        Some(event_set_cancel_bubble),
    ),
];

pub(crate) const SPECS: &[Spec] = &[
    Spec {
        class: Class::EventTarget,
        name: c"EventTarget",
        parent: None,
        constructor: Some((target_constructor, 0)),
        methods: &[
            (Key::Name(c"addEventListener"), add_event_listener, 2),
            (Key::Name(c"removeEventListener"), remove_event_listener, 2),
            (Key::Name(c"dispatchEvent"), dispatch_event, 1),
        ],
        accessors: &[],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "",
            members: r#"constructor();
addEventListener(
  type: string,
  listener: EventListenerOrEventListenerObject | null,
  options?: AddEventListenerOptions | boolean,
): void;
removeEventListener(
  type: string,
  listener: EventListenerOrEventListenerObject | null,
  options?: { capture?: boolean } | boolean,
): void;
dispatchEvent(event: Event): boolean;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::Event,
        name: c"Event",
        parent: None,
        constructor: Some((event_constructor, 1)),
        methods: &[
            (Key::Name(c"composedPath"), event_composed_path, 0),
            (Key::Name(c"stopPropagation"), event_stop_propagation, 0),
            (
                Key::Name(c"stopImmediatePropagation"),
                event_stop_immediate_propagation,
                0,
            ),
            (Key::Name(c"preventDefault"), event_prevent_default, 0),
            (Key::Name(c"initEvent"), event_init_event, 1),
        ],
        accessors: EVENT_ACCESSORS,
        statics: &[],
        constants: &[
            (c"NONE", 0.0),
            (c"CAPTURING_PHASE", 1.0),
            (c"AT_TARGET", AT_TARGET),
            (c"BUBBLING_PHASE", 3.0),
        ],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "",
            members: r#"constructor(type: string, init?: EventInit);
readonly type: string;
readonly target: EventTarget | null;
readonly currentTarget: EventTarget | null;
readonly eventPhase: number;
readonly bubbles: boolean;
readonly cancelable: boolean;
readonly defaultPrevented: boolean;
readonly composed: boolean;
readonly isTrusted: boolean;
readonly timeStamp: number;
returnValue: boolean;
/** `target` (legacy). */
readonly srcElement: EventTarget | null;
/** Sets `type`, `bubbles` and `cancelable` of an event not dispatched yet (legacy). */
initEvent(type: string, bubbles?: boolean, cancelable?: boolean): void;
cancelBubble: boolean;
composedPath(): EventTarget[];
preventDefault(): void;
stopPropagation(): void;
stopImmediatePropagation(): void;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::CustomEvent,
        name: c"CustomEvent",
        parent: Some(Class::Event),
        constructor: Some((custom_event_constructor, 1)),
        methods: &[],
        accessors: &[(c"detail", event_detail, None)],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "<T = any> extends Event",
            members: r#"constructor(type: string, init?: EventInit & { detail?: T });
readonly detail: T;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::AbortSignal,
        name: c"AbortSignal",
        parent: Some(Class::EventTarget),
        constructor: None,
        methods: &[(Key::Name(c"throwIfAborted"), signal_throw_if_aborted, 0)],
        accessors: &[
            (c"aborted", signal_aborted, None),
            (c"reason", signal_reason, None),
            (c"onabort", signal_onabort, Some(signal_set_onabort)),
        ],
        statics: &[
            (c"abort", signal_abort, 0),
            (c"timeout", signal_timeout, 1),
            (c"any", signal_any, 1),
        ],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: " extends EventTarget",
            members: r#"readonly aborted: boolean;
readonly reason: any;
onabort: ((event: Event) => void) | null;
throwIfAborted(): void;
static abort(reason?: any): AbortSignal;
static timeout(milliseconds: number): AbortSignal;
static any(signals: Iterable<AbortSignal>): AbortSignal;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::AbortController,
        name: c"AbortController",
        parent: None,
        constructor: Some((controller_constructor, 0)),
        methods: &[(Key::Name(c"abort"), controller_abort, 0)],
        accessors: &[(c"signal", controller_signal, None)],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "",
            members: r#"readonly signal: AbortSignal;
abort(reason?: any): void;"#,
        }
        .declared(),
    },
];

/// The TypeScript types of this file's classes (`globals.d.ts`).
pub(crate) const TYPES: &str = r#"type EventListenerOrEventListenerObject = ((event: Event) => void) | { handleEvent(event: Event): void };

interface AddEventListenerOptions {
  capture?: boolean;
  once?: boolean;
  passive?: boolean;
  signal?: AbortSignal;
}

interface EventInit {
  bubbles?: boolean;
  cancelable?: boolean;
  composed?: boolean;
}
"#;
