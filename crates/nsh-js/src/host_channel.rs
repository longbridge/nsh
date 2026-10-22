//! `nsh/host`: the app's side of the message channel with the embedding
//! app (`nsh_core::host_channel`; the engine's `HostChannel` is the
//! embedding app's side).
//!
//! `host.params` is the embedding app's launch parameters, made once per
//! VM from [`HostBridge::params`]. `host.on(name, listener)` hears the
//! embedding app's events: the engine hands each to [`deliver`], which
//! calls the listeners of its name, or keeps it in the VM's inbox until a
//! listener of that name is added (a job then delivers what was kept, at
//! the end of the turn that added it). `host.call(name, data)` is a
//! promise the embedding app's answer settles ([`settle`], by the call's
//! id); `host.post(name, data)` hands data over without one. Data crosses
//! as JSON values converted natively (no `JSON` in JS), and the limits are
//! checked here: names ([`check_name`]), the depth and size of data
//! (`protocol::to_json`), the calls waiting at once
//! ([`MAX_PENDING_CALLS`]) and the events kept ([`HostInbox`]).

use std::{
    cell::{Cell, RefCell},
    ffi::c_int,
    rc::Rc,
};

use nsh_core::{
    JsonValue,
    host_channel::{HostBridge, HostInbox, MAX_PENDING_CALLS, check_name},
};
use rquickjs::qjs;
use rustc_hash::FxHashMap;

use crate::{
    modules::{Export, Item, Ts},
    web::js::{
        self, Args, Ctx, Deferred, R, Raw, Throw, UNDEFINED, V, closure, native, type_error,
    },
};

/// A listener of the embedding app's events.
struct Listener {
    id: u32,
    name: String,
    f: V,
}

/// A context's side of the channel.
#[derive(Default)]
pub(crate) struct HostChannel {
    /// The listeners, oldest first.
    listeners: RefCell<Vec<Listener>>,
    next: Cell<u32>,
    /// The events no listener heard yet.
    inbox: RefCell<HostInbox>,
    /// The calls waiting for the embedding app's answers, by id.
    calls: RefCell<FxHashMap<u64, Deferred>>,
    next_call: Cell<u64>,
}

impl HostChannel {
    /// Frees the listeners and the calls' promises (the state is
    /// unregistered).
    pub(crate) fn free_all(&self) {
        let listeners = std::mem::take(&mut *self.listeners.borrow_mut());
        drop(listeners);
        let calls = std::mem::take(&mut *self.calls.borrow_mut());
        drop(calls);
    }
}

fn channel<'a>(cx: Ctx) -> &'a HostChannel {
    // SAFETY: a projection of the state's field, by shared reference.
    unsafe { &(*crate::state::state(cx)).host_channel }
}

/// The engine's bridge to the embedding app, if this VM has one.
fn bridge(cx: Ctx) -> Option<Rc<dyn HostBridge>> {
    crate::dom::shared(cx).host_bridge.borrow().clone()
}

/// A message name argument of `what`, checked.
fn name_arg(cx: Ctx, v: Raw, what: &str) -> R<String> {
    // SAFETY: an inline tag check.
    if !unsafe { qjs::JS_IsString(v) } {
        return Err(type_error(cx, &format!("{what}: the name is a string")));
    }
    let name = js::to_string(cx, v)?;
    check_name(&name).map_err(|e| type_error(cx, &format!("{what}: {e}")))?;
    Ok(name)
}

/// `v` as JSON (`undefined`: `null`); throws a `RangeError` for data
/// nested too deep or too large.
fn to_json(cx: Ctx, v: Raw) -> R<JsonValue> {
    if js::is_undefined(v) {
        return Ok(JsonValue::Null);
    }
    // SAFETY: a context reference and a dup for the converter.
    let rcx = unsafe { rquickjs::Ctx::from_raw(std::ptr::NonNull::new_unchecked(cx)) };
    let value = unsafe { rquickjs::Value::from_raw(rcx, qjs::JS_DupValue(cx, v)) };
    crate::protocol::to_json(&value).map_err(|_| Throw)
}

/// JSON as a JS value.
fn from_json(cx: Ctx, json: &JsonValue) -> V {
    V::own(cx, crate::host::json_to_js(cx, json))
}

/// A new `Error(message)`.
fn error(cx: Ctx, message: &str) -> R<V> {
    let global = js::global(cx);
    let class = js::get(cx, global.raw(), "Error")?;
    let message = js::string(cx, message)?;
    js::construct(cx, class.raw(), &[message.raw()])
}

/// The `host` object.
fn make(cx: Ctx) -> Raw {
    let made = (|| -> R<V> {
        let obj = js::new_object(cx)?;
        let params = bridge(cx)
            .map(|b| b.params())
            .filter(JsonValue::is_object)
            .unwrap_or_else(|| JsonValue::Object(Default::default()));
        js::define(cx, obj.raw(), "params", from_json(cx, &params), true)?;
        js::define(cx, obj.raw(), "on", js::function(cx, c"on", on, 2)?, true)?;
        js::define(
            cx,
            obj.raw(),
            "call",
            js::function(cx, c"call", call, 2)?,
            true,
        )?;
        js::define(
            cx,
            obj.raw(),
            "post",
            js::function(cx, c"post", post, 2)?,
            true,
        )?;
        Ok(obj)
    })();
    made.map_or(qjs::JS_EXCEPTION, V::take)
}

native! {
    /// `on(name, listener)`: a function that stops listening, which goes by
    /// itself with the owner it was added under (as `keymap.bind`'s
    /// bindings do).
    fn on(cx, _this, args) {
        let name = name_arg(cx, args.get(0), "host.on")?;
        if !js::is_function(cx, args.get(1)) {
            return Err(type_error(cx, "host.on: the listener is not a function"));
        }
        let state = channel(cx);
        let id = state.next.get() + 1;
        state.next.set(id);
        state.listeners.borrow_mut().push(Listener {
            id,
            name: name.clone(),
            f: V::dup(cx, args.get(1)),
        });
        // What came before a listener did goes to it at the end of this
        // turn: after the code adding it (and its neighbors) ran.
        if state.inbox.borrow().has(&name) {
            let name = js::string(cx, &name)?;
            let mut job_args = [name.raw()];
            // SAFETY: the job keeps its own reference to the argument.
            if unsafe { qjs::JS_EnqueueJob(cx, Some(deliver_kept_job), 1, job_args.as_mut_ptr()) } < 0 {
                return Err(Throw);
            }
        }
        let off = closure(cx, Vec::new(), move |cx, _, _| {
            let gone = {
                let mut listeners = channel(cx).listeners.borrow_mut();
                listeners
                    .iter()
                    .position(|l| l.id == id)
                    .map(|i| listeners.remove(i))
            };
            // Freed with no borrow held.
            drop(gone);
            Ok(V::undefined())
        })?;
        crate::keymap::undo_with_owner(cx, &off);
        Ok(off)
    }
}

native! {
    /// `call(name, data)`: a promise of the embedding app's answer.
    fn call(cx, _this, args) {
        match start_call(cx, args) {
            Ok(promise) => Ok(promise),
            Err(Throw) => js::rejected_with_exception(cx),
        }
    }
}

fn start_call(cx: Ctx, args: Args<'_>) -> R<V> {
    let name = name_arg(cx, args.get(0), "host.call")?;
    let data = to_json(cx, args.get(1))?;
    let state = channel(cx);
    if state.calls.borrow().len() >= MAX_PENDING_CALLS {
        return Err(js::range_error(
            cx,
            &format!(
                "host.call(\"{name}\"): {MAX_PENDING_CALLS} calls already wait for the embedding app's answers"
            ),
        ));
    }
    let deferred = Deferred::new(cx)?;
    let promise = deferred.promise.clone_ref();
    let id = state.next_call.get() + 1;
    state.next_call.set(id);
    let started = match bridge(cx) {
        Some(bridge) => bridge.call(id, &name, data),
        None => Err(no_handler(&name)),
    };
    match started {
        Ok(()) => {
            state.calls.borrow_mut().insert(id, deferred);
        }
        Err(message) => {
            let e = error(cx, &message)?;
            deferred.reject(cx, e.raw());
        }
    }
    Ok(promise)
}

/// Why a call to `name` fails at once when nothing handles it.
pub(crate) fn no_handler(name: &str) -> String {
    format!("host.call(\"{name}\"): the embedding app handles no \"{name}\"")
}

native! {
    /// `post(name, data)`: hands `data` to the embedding app, with no
    /// answer.
    fn post(cx, _this, args) {
        let name = name_arg(cx, args.get(0), "host.post")?;
        let data = to_json(cx, args.get(1))?;
        if let Some(bridge) = bridge(cx) {
            bridge.post(&name, data);
        }
        Ok(V::undefined())
    }
}

/// The job `on` queues: delivers the events of its name kept so far.
unsafe extern "C" fn deliver_kept_job(cx: Ctx, argc: c_int, argv: *mut Raw) -> Raw {
    // SAFETY: the job's one argument, the name.
    let args = unsafe { Args::new(argc, argv) };
    let Ok(name) = js::to_string(cx, args.get(0)) else {
        return qjs::JS_EXCEPTION;
    };
    if !listens(cx, &name) {
        return UNDEFINED;
    }
    let kept = channel(cx).inbox.borrow_mut().take(&name);
    for data in kept {
        dispatch(cx, &name, &data);
    }
    UNDEFINED
}

/// Whether a listener of `name` is there.
fn listens(cx: Ctx, name: &str) -> bool {
    channel(cx)
        .listeners
        .borrow()
        .iter()
        .any(|l| l.name == name)
}

/// Calls the listeners of `name` with `data`, oldest first; one that
/// throws is reported and the others still run.
fn dispatch(cx: Ctx, name: &str, data: &JsonValue) {
    let listeners: Vec<V> = channel(cx)
        .listeners
        .borrow()
        .iter()
        .filter(|l| l.name == name)
        .map(|l| l.f.clone_ref())
        .collect();
    let data = from_json(cx, data);
    for f in listeners {
        if js::call(cx, f.raw(), UNDEFINED, &[data.raw()]).is_err() {
            js::report_exception(cx);
        }
    }
}

/// The embedding app's event `name`: to its listeners, or kept until one
/// is added.
pub(crate) fn deliver(cx: Ctx, name: &str, data: JsonValue) {
    if listens(cx, name) {
        dispatch(cx, name, &data);
        return;
    }
    let dropped = channel(cx).inbox.borrow_mut().push(name.to_owned(), data);
    if let Some((name, _)) = dropped {
        log::warn!(
            target: "js",
            "nsh/host: the app did not listen to the embedding app's events; the oldest kept (\"{name}\") was dropped"
        );
    }
}

/// Settles call `id` with the embedding app's answer (or why it failed).
pub(crate) fn settle(cx: Ctx, id: u64, result: Result<JsonValue, String>) {
    let Some(deferred) = channel(cx).calls.borrow_mut().remove(&id) else {
        return;
    };
    match result {
        Ok(value) => {
            let value = from_json(cx, &value);
            deferred.resolve(cx, value.raw());
        }
        Err(message) => match error(cx, &message) {
            Ok(e) => deferred.reject(cx, e.raw()),
            Err(Throw) => {
                let e = js::take_exception(cx);
                deferred.reject(cx, e.raw());
            }
        },
    }
}

/// `nsh/host`'s exports.
pub(crate) const EXPORTS: &[Item] = &[(
    "host",
    Export::Value(make),
    Ts::value(
        "The channel to the app this one is embedded in: its launch parameters\n(`host.params`), its events (`host.on`), and calls (`host.call`) and posts\n(`host.post`) to it.",
        "Host",
    ),
)];

/// `nsh/host`'s types.
pub(crate) const TYPES: &str = r#"/** Data that crosses between the app and the embedding app: a JSON value. */
export type HostValue = null | boolean | number | string | HostValue[] | { [key: string]: HostValue };

/** The embedding app's launch parameters. */
export type HostParams = { readonly [key: string]: HostValue };

/** The channel to the app this one is embedded in (the iOS, Android and
 * desktop SDKs' `NshAppView`, `NshView` and `AppView`). Data crosses as JSON
 * values: what `JSON.stringify` keeps of it (functions and `undefined`
 * fields are left out). */
export interface Host {
  /** The parameters the embedding app set before it started this app (what
   * to open, who the user is), `{}` when it set none. They are fixed while
   * the app runs; later requests come as events. */
  readonly params: HostParams;
  /** Listens to the embedding app's event `name`: `listener` gets its data
   * (`null` when it sent none), the listeners of a name in the order they
   * were added. Events the embedding app sends while the app has no listener
   * of their name are kept (up to 256, then the oldest go) and come right
   * after the first listener is added, once the code adding it ran. Returns
   * a function that stops listening; a listener added while a component
   * renders stops with the component. */
  on<T = HostValue>(name: string, listener: (data: T) => void): () => void;
  /** Calls the embedding app's handler of `name` with `data`: the promise
   * resolves with its answer, or rejects with an `Error` with the message the
   * handler gave, or at once when the embedding app handles no `name`. At
   * most 1024 calls wait for answers at once; one more rejects with a
   * `RangeError`. */
  call<T = HostValue>(name: string, data?: unknown): Promise<T>;
  /** Hands `data` to the embedding app's handler of `name`, without an
   * answer; it is dropped when the embedding app handles no `name`. */
  post(name: string, data?: unknown): void;
}
"#;

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use nsh_core::{JsonValue, Op, host_channel::HostBridge};
    use serde_json::json;

    use crate::Host;

    /// The embedding app's side, recording what reaches it.
    #[derive(Default)]
    struct Bridge {
        calls: RefCell<Vec<(u64, String, JsonValue)>>,
        posts: RefCell<Vec<(String, JsonValue)>>,
    }

    impl HostBridge for Bridge {
        fn params(&self) -> JsonValue {
            json!({ "conversation": "c-1", "user": { "id": 7 } })
        }

        fn call(&self, id: u64, name: &str, data: JsonValue) -> Result<(), String> {
            if name == "missing" {
                return Err("no such handler".into());
            }
            self.calls.borrow_mut().push((id, name.into(), data));
            Ok(())
        }

        fn post(&self, name: &str, data: JsonValue) {
            self.posts.borrow_mut().push((name.into(), data));
        }
    }

    const REPORT: &str = r#"import { text } from "nsh:rt";
import { host } from "nsh/host";
const report = (v) => text(String(v));
"#;

    /// The texts reported since the last call.
    fn reported(host: &Host) -> Vec<String> {
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        ops.into_iter()
            .filter_map(|op| match op {
                Op::CreateText { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    fn pending_calls(host: &Host) -> usize {
        host.with_raw(|cx| super::channel(cx).calls.borrow().len())
    }

    #[test]
    fn without_an_embedding_app_calls_fail_and_params_are_empty() {
        let host = Host::new().unwrap();
        host.eval_source(&format!(
            r#"{REPORT}
report(JSON.stringify(host.params));
host.post("seen", {{ a: 1 }});
host.call("profile").catch((e) => report(e.message));
"#
        ))
        .unwrap();
        host.run_jobs();
        assert_eq!(
            reported(&host),
            [
                "{}",
                "host.call(\"profile\"): the embedding app handles no \"profile\""
            ]
        );
    }

    #[test]
    fn calls_settle_with_the_answers_by_id() {
        let host = Host::new().unwrap();
        let bridge = Rc::new(Bridge::default());
        host.set_host_bridge(bridge.clone());
        host.eval_source(&format!(
            r#"{REPORT}
report(host.params.conversation + " " + host.params.user.id);
host.call("profile", {{ id: 7, skip: undefined }}).then((v) => report("profile " + v.name));
host.call("save", [1, 2]).catch((e) => report("save " + e.message));
host.call("missing").catch((e) => report("missing " + e.message));
host.post("seen", "home");
"#
        ))
        .unwrap();
        host.run_jobs();
        assert_eq!(reported(&host), ["c-1 7", "missing no such handler"]);
        let calls = bridge.calls.borrow().clone();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1, "profile");
        assert_eq!(calls[0].2, json!({ "id": 7 }));
        assert_eq!(calls[1].2, json!([1, 2]));
        assert_eq!(*bridge.posts.borrow(), [("seen".to_owned(), json!("home"))]);
        assert_eq!(pending_calls(&host), 2);
        host.host_reply(calls[1].0, Err("disk full".into()));
        host.host_reply(calls[0].0, Ok(json!({ "name": "Ada" })));
        // A second answer, or one for no call, is ignored.
        host.host_reply(calls[0].0, Ok(json!(null)));
        host.host_reply(999, Ok(json!(null)));
        assert_eq!(reported(&host), ["save disk full", "profile Ada"]);
        assert_eq!(pending_calls(&host), 0);
    }

    #[test]
    fn events_wait_for_a_listener_of_their_name() {
        let host = Host::new().unwrap();
        host.set_host_bridge(Rc::new(Bridge::default()));
        host.eval_source(&format!(
            r#"{REPORT}
globalThis.listen = () => {{
  host.on("open", (d) => report("a " + d.id));
  host.on("open", (d) => report("b " + d.id));
  report("listening");
}};
const off = host.on("tick", (n) => report("tick " + n));
globalThis.stop = () => off();
"#
        ))
        .unwrap();
        host.host_event("open", json!({ "id": 1 }));
        host.host_event("open", json!({ "id": 2 }));
        host.host_event("tick", json!(1));
        assert_eq!(reported(&host), ["tick 1"]);
        // Kept until the listeners are added, then theirs in order, after
        // the code adding them.
        host.eval_source("listen();").unwrap();
        host.run_jobs();
        assert_eq!(reported(&host), ["listening", "a 1", "b 1", "a 2", "b 2"]);
        host.eval_source("stop();").unwrap();
        host.host_event("tick", json!(2));
        assert_eq!(reported(&host), Vec::<String>::new());
    }

    #[test]
    fn names_data_and_waiting_calls_are_limited() {
        let host = Host::new().unwrap();
        host.set_host_bridge(Rc::new(Bridge::default()));
        host.eval_source(&format!(
            r#"{REPORT}
const fails = (f) => {{ try {{ f(); return "no"; }} catch (e) {{ return e.name; }} }};
report(fails(() => host.on("", () => {{}})));
report(fails(() => host.on("x".repeat(257), () => {{}})));
report(fails(() => host.on("x", 1)));
report(fails(() => host.post(1)));
let deep = 0;
for (let i = 0; i < 100; i++) deep = [deep];
report(fails(() => host.post("deep", deep)));
for (let i = 0; i < {max}; i++) host.call("wait");
host.call("wait").catch((e) => report(e.name));
"#,
            max = nsh_core::host_channel::MAX_PENDING_CALLS
        ))
        .unwrap();
        host.run_jobs();
        assert_eq!(
            reported(&host),
            [
                "TypeError",
                "TypeError",
                "TypeError",
                "TypeError",
                "RangeError",
                "RangeError"
            ]
        );
        assert_eq!(
            pending_calls(&host),
            nsh_core::host_channel::MAX_PENDING_CALLS
        );
    }
}
