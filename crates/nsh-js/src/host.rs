use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nsh_core::{
    EventKind, EventPayload, NodeId, Op, PayloadValue, TouchPhase, js::Limits,
    platform::Environment, time::Instant,
};
use rquickjs::{
    CatchResultExt, Context, Ctx, Function, Module, Persistent, Runtime, Value, function::Opt, qjs,
};

use crate::{
    console,
    error::JsError,
    net::{FetchEvent, NetworkQueue, RequestId},
    protocol::Warnings,
    sandbox::{self, Budget, Phase},
    timers::Timers,
    web,
};

/// Receives uncaught JS exceptions.
pub type ErrorHandler = Box<dyn Fn(&JsError)>;

/// State shared between the host and the native functions installed in JS.
/// Only touched on the JS thread.
#[derive(Default)]
pub(crate) struct Shared {
    pub(crate) ops: RefCell<Vec<Op>>,
    /// Native nodes collected since the last [`Host::take_ops`] (their
    /// finalizers queue here: the op queue may be borrowed meanwhile).
    pub(crate) releases: RefCell<Vec<NodeId>>,
    /// Native animation nodes and composites collected meanwhile.
    pub(crate) animation_releases: RefCell<Vec<nsh_animation::AnimationOp>>,
    timers: RefCell<Timers>,
    pub(crate) warnings: RefCell<Warnings>,
    pub(crate) net: RefCell<NetworkQueue>,
    /// Requests whose response bodies were collected unread (finalizers
    /// queue them here; the next [`Host::take_network`] cancels them).
    pub(crate) dropped_fetches: RefCell<Vec<RequestId>>,
    /// Font assets imported by the app, not yet registered.
    pub(crate) fonts: RefCell<Vec<String>>,
    /// `requestAnimationFrame` callbacks waiting for the next frame.
    frames: RefCell<Vec<(u32, Persistent<Function<'static>>)>>,
    next_frame_id: std::cell::Cell<u32>,
    /// Measures one line of text for `measureText` (the engine's text
    /// engine).
    pub(crate) measurer: RefCell<Option<TextMeasurer>>,
    /// The VM's limits.
    pub(crate) limits: Limits,
    /// Why the VM was stopped (out of memory, over a quota): no JS runs
    /// after that.
    pub(crate) fatal: RefCell<Option<String>>,
}

impl Shared {
    /// Stops the VM for `reason` (the first reason wins).
    pub(crate) fn stop(&self, reason: String) {
        self.fatal.borrow_mut().get_or_insert(reason);
    }
}

/// Measures the width of one line of `text` in `font`, in points.
pub type TextMeasurer = Box<dyn Fn(&nsh_core::canvas::CanvasFont, &str) -> f32>;

/// One QuickJS runtime + context with the nsh host APIs installed.
pub struct Host {
    // Field order matters: the context must drop before the runtime.
    ctx: Context,
    rt: Runtime,
    /// Set by the heap's allocator when it refuses an allocation.
    exhausted: Arc<AtomicBool>,
    shared: Rc<Shared>,
    on_error: Option<ErrorHandler>,
    budget: Budget,
}

/// A context with `JS_NewContext`'s intrinsics. With feature
/// `bytecode-only` without the one that compiles source (`eval`, the
/// function constructors, `Module::declare`): nothing then reaches QuickJS'
/// parser and compiler, which the linker leaves out, and the VM runs
/// bytecode only (the web runtime of published apps).
fn new_context(rt: &Runtime) -> rquickjs::Result<Context> {
    #[cfg(not(feature = "bytecode-only"))]
    {
        Context::full(rt)
    }
    #[cfg(feature = "bytecode-only")]
    {
        use rquickjs::context::intrinsic::{
            Date, Json, MapSet, Performance, Promise, Proxy, RegExp, TypedArrays, WeakRef,
        };
        let ctx = Context::custom::<(
            Date,
            RegExp,
            Json,
            Proxy,
            MapSet,
            TypedArrays,
            Promise,
            WeakRef,
            Performance,
        )>(rt)?;
        // SAFETY: a new, live context; rquickjs has no intrinsic for it.
        ctx.with(|ctx| unsafe { rquickjs::qjs::JS_AddIntrinsicAToB(ctx.as_raw().as_ptr()) });
        Ok(ctx)
    }
}

/// How a VM is set up.
#[derive(Debug, Clone, Default)]
pub struct HostOptions {
    /// Resource limits.
    pub limits: Limits,
    /// Removes `eval` and the function constructors and freezes the shared
    /// prototypes (see `sandbox`). Off for development bundles.
    pub harden: bool,
    /// The hardening as bytecode ([`crate::compile_harden`]), run instead
    /// of compiling it: a runtime without QuickJS' parser
    /// (`bytecode-only`) needs it to harden. Trusted input, like a bundle's
    /// bytecode: only from the CLI that compiled the app.
    pub harden_bytecode: Option<std::sync::Arc<Vec<u8>>>,
    /// The UI sets apps import as native modules
    /// (`nsh_ui_mobile::registry()`, served as `nsh/ui/mobile`).
    pub ui_sets: Vec<nsh_core::ui::Registry>,
    /// The URL relative request URLs (`fetch("/api")`, `new Request("x")`)
    /// resolve against: the page's on the web. `None` (native hosts):
    /// they are invalid, as there is no document they belong to.
    pub base_url: Option<String>,
}

impl Host {
    /// Creates a runtime with the default limits, not hardened, and installs
    /// `console`, timers and the op bridge.
    pub fn new() -> Result<Host, JsError> {
        Self::with_options(HostOptions::default())
    }

    /// Creates a runtime set up by `options` and installs `console`, timers
    /// and the op bridge.
    pub fn with_options(options: HostOptions) -> Result<Host, JsError> {
        let limits = options.limits;
        // The allocator enforces the heap limit (`set_memory_limit` does
        // nothing with one) and tells when it was reached.
        let (allocator, exhausted) = sandbox::HeapAllocator::new(limits.memory_bytes);
        let rt = Runtime::new_with_alloc(allocator)?;
        // The engine thread is spawned with a larger stack; keep a margin.
        rt.set_max_stack_size(limits.stack_bytes);
        let budget = Budget::new(limits);
        rt.set_interrupt_handler(Some(budget.interrupt_handler(None)));
        let ctx = new_context(&rt)?;
        let shared = Rc::new(Shared {
            limits,
            ..Shared::default()
        });
        ctx.with(|ctx| {
            crate::state::install(&ctx, shared.clone(), &options.ui_sets);
            web::web(ctx.as_raw().as_ptr()).base_url = options
                .base_url
                .as_deref()
                .and_then(|u| url::Url::parse(u).ok());
            install(&ctx, &shared)?;
            if options.harden {
                match &options.harden_bytecode {
                    // SAFETY: trusted, per the option's contract.
                    Some(bytecode) => unsafe { sandbox::harden_from(&ctx, bytecode)? },
                    None => sandbox::harden(&ctx)?,
                }
            }
            Ok(())
        })
        .map_err(|e| ctx.with(|ctx| JsError::from_engine(&ctx, e)))?;
        Ok(Host {
            ctx,
            rt,
            exhausted,
            shared,
            on_error: None,
            budget,
        })
    }

    /// Aborts running JS (with an uncatchable exception) once `stop` is
    /// set: an engine shutting down never waits for a long-running script.
    pub fn set_stop_flag(&self, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        self.rt
            .set_interrupt_handler(Some(self.budget.interrupt_handler(Some(stop))));
    }

    /// Why the VM was stopped, if it was: it ran out of memory, or the app
    /// went over a quota. No JS runs after that; the views stay as they are.
    pub fn fatal(&self) -> Option<String> {
        self.shared.fatal.borrow().clone()
    }

    /// Stops the VM (e.g. the core refused an op over a quota).
    pub fn stop(&self, reason: String) {
        self.shared.stop(reason);
    }

    /// The VM's limits.
    pub fn limits(&self) -> &Limits {
        self.budget.limits()
    }

    /// Runs `f` (a call into JS) within the budget of `phase`; `None` once
    /// the VM was stopped.
    fn guarded<R>(&self, phase: Phase, f: impl FnOnce() -> R) -> Option<R> {
        if self.shared.fatal.borrow().is_some() {
            return None;
        }
        let started = self.budget.begin(phase);
        let result = f();
        self.budget.end(started);
        // The app may have caught running out (it is an `InternalError`):
        // the VM stops all the same.
        if self.exhausted.load(Ordering::Relaxed) && self.shared.fatal.borrow().is_none() {
            self.uncaught(&JsError::new("out of memory".to_owned()));
        }
        Some(result)
    }

    /// What an error from a call means: an overrun budget or exhausted
    /// memory get a message saying so (exhausted memory also stops the VM).
    fn classify(&self, mut err: JsError) -> JsError {
        if let Some(trip) = self.budget.take_trip() {
            err.message = format!("{trip}\n{}", err.message);
        } else if self.exhausted.load(Ordering::Relaxed) {
            let reason = format!(
                "the script ran out of memory (the JS heap is limited to {} MB) and was stopped. \
                 Keep large data out of JS: load it in pages, or keep it on the native side \
                 (a server, a native module) and hand JS only what it shows.",
                self.budget.limits().memory_bytes >> 20
            );
            self.shared.stop(reason.clone());
            err.message = format!("{reason}\n{}", err.message);
        }
        err
    }

    /// The error for a call refused because the VM was stopped.
    fn stopped(&self) -> JsError {
        JsError::new(self.fatal().unwrap_or_default())
    }

    /// Sets what canvas `measureText` measures with.
    pub fn set_text_measurer(&mut self, measurer: TextMeasurer) {
        *self.shared.measurer.borrow_mut() = Some(measurer);
    }

    /// Sets the handler for uncaught exceptions (in event handlers, timers and
    /// promise jobs). Without a handler they are logged.
    pub fn set_error_handler(&mut self, handler: ErrorHandler) {
        self.on_error = Some(handler);
    }

    /// Evaluates a bundle given as source.
    pub fn eval_source(&self, source: &str) -> Result<(), JsError> {
        self.guarded(Phase::Load, || {
            self.ctx
                .with(|ctx| -> Result<(), JsError> {
                    let module = Module::declare(ctx.clone(), crate::BUNDLE_NAME, source)
                        .catch(&ctx)
                        .map_err(JsError::caught)?;
                    let (_, promise) = module.eval().catch(&ctx).map_err(JsError::caught)?;
                    promise.finish::<()>().catch(&ctx).map_err(JsError::caught)
                })
                .map_err(|e| self.classify(e))?;
            self.run_jobs();
            Ok(())
        })
        .unwrap_or_else(|| Err(self.stopped()))
    }

    /// Evaluates a classic script named `name` (for stack traces) and
    /// returns whether its completion value is `true`: development updates
    /// report whether they could be applied.
    pub fn eval_script(&self, name: &str, source: &str) -> Result<bool, JsError> {
        self.guarded(Phase::Load, || self.eval_script_unguarded(name, source))
            .unwrap_or_else(|| Err(self.stopped()))
    }

    fn eval_script_unguarded(&self, name: &str, source: &str) -> Result<bool, JsError> {
        let applied = self.ctx.with(|ctx| -> Result<bool, JsError> {
            let mut options = rquickjs::context::EvalOptions::default();
            options.global = true;
            options.strict = false;
            options.filename = Some(name.to_owned());
            let value: rquickjs::Value<'_> = ctx
                .eval_with_options(source, options)
                .catch(&ctx)
                .map_err(JsError::caught)?;
            Ok(value.as_bool() == Some(true))
        });
        let applied = applied.map_err(|e| self.classify(e))?;
        self.run_jobs();
        Ok(applied)
    }

    /// Evaluates a bundle compiled with [`crate::compile_bytecode`].
    pub fn eval_bytecode(&self, bytecode: &[u8]) -> Result<(), JsError> {
        self.guarded(Phase::Load, || {
            self.ctx
                .with(|ctx| -> Result<(), JsError> {
                    // SAFETY: bytecode comes from `compile_bytecode` with the same
                    // QuickJS build; QuickJS does not validate arbitrary input.
                    let module = unsafe { Module::load(ctx.clone(), bytecode) }
                        .catch(&ctx)
                        .map_err(JsError::caught)?;
                    let (_, promise) = module.eval().catch(&ctx).map_err(JsError::caught)?;
                    promise.finish::<()>().catch(&ctx).map_err(JsError::caught)
                })
                .map_err(|e| self.classify(e))?;
            self.run_jobs();
            Ok(())
        })
        .unwrap_or_else(|| Err(self.stopped()))
    }

    /// Gives the VM chunk `id` of a split bundle (an ES module as
    /// `nsh-bundler` makes it), before the bundle is evaluated or when it
    /// was asked for ([`Host::take_chunk_requests`]); the imports waiting
    /// for it continue.
    pub fn add_chunk_source(&self, id: &str, source: String) {
        self.add_chunk(id, crate::modules::ChunkCode::Source(source));
    }

    /// [`Host::add_chunk_source`] for a chunk compiled with
    /// [`crate::compile_chunk_bytecode`].
    ///
    /// # Safety
    ///
    /// As [`Host::eval_bytecode`]: bytecode from this QuickJS build, from a
    /// trusted source (QuickJS does not validate it).
    pub unsafe fn add_chunk_bytecode(&self, id: &str, bytecode: Vec<u8>) {
        self.add_chunk(id, crate::modules::ChunkCode::Bytecode(bytecode));
    }

    fn add_chunk(&self, id: &str, code: crate::modules::ChunkCode) {
        self.guarded(Phase::Call, || {
            self.ctx
                .with(|ctx| crate::chunks::add(ctx.as_raw().as_ptr(), id, code));
            self.run_jobs();
        });
    }

    /// Chunk `id` could not be loaded: the imports waiting for it reject
    /// with `message`. A later import asks for it again.
    pub fn chunk_failed(&self, id: &str, message: &str) {
        self.guarded(Phase::Call, || {
            self.ctx
                .with(|ctx| crate::chunks::failed(ctx.as_raw().as_ptr(), id, message));
            self.run_jobs();
        });
    }

    /// Moves the ids of the chunks the app imports and the VM does not have
    /// (since the last call) into `out`: the host loads each and hands it
    /// over with [`Host::add_chunk_source`] or [`Host::chunk_failed`].
    pub fn take_chunk_requests(&self, out: &mut Vec<String>) {
        self.ctx.with(|ctx| {
            // SAFETY: the context's state, for this call.
            unsafe { &(*crate::state::state(ctx.as_raw().as_ptr())).chunks }.take_requests(out)
        });
    }

    /// Delivers a native event (and its data) to the node's JS handler.
    pub fn dispatch_event(&self, id: NodeId, event: EventKind, payload: &EventPayload) {
        self.guarded(Phase::Call, || {
            self.ctx.with(|ctx| {
                // Touches: the responder system is native.
                if let (EventKind::Touch, EventPayload::Touch(touch)) = (event, payload) {
                    if crate::touch::dispatch(ctx.as_raw().as_ptr(), touch).is_err() {
                        self.report::<()>(&ctx, Err(rquickjs::Error::Exception));
                    }
                    return;
                }
                let payload = match (event, payload) {
                    // `onChangeText` receives the text itself, as in React Native.
                    (EventKind::ChangeText, EventPayload::Text { text, .. }) => {
                        rquickjs::String::from_str(ctx.clone(), text).map(|s| s.into_value())
                    }
                    (_, EventPayload::Value(value)) => payload_value(&ctx, value),
                    _ => payload_object(&ctx, payload),
                };
                let raw = ctx.as_raw().as_ptr();
                let result = payload.and_then(|payload| {
                    // Nodes hold their handlers natively.
                    let Some(handler) = crate::dom::handler(raw, id, event) else {
                        return Ok(());
                    };
                    // SAFETY: an owned function value of this context.
                    let handler = unsafe { Value::from_raw(ctx.clone(), handler) };
                    match handler.into_function() {
                        Some(f) => f.call::<_, ()>((payload,)),
                        None => Ok(()),
                    }
                });
                self.report(&ctx, result);
            });
            self.run_jobs();
        });
    }

    /// Asks a `List` to render items into cells: `(slot, item index)`
    /// pairs.
    pub fn bind_list(&self, list: NodeId, binds: &[(u32, u32)]) {
        self.guarded(Phase::Call, || {
            self.ctx.with(|ctx| {
                if let Some(Err(_)) = crate::list::bind(ctx.as_raw().as_ptr(), list, binds) {
                    self.report::<()>(&ctx, Err(rquickjs::Error::Exception));
                }
            });
            self.run_jobs();
        });
    }

    /// Resolves the promises of finished (or interrupted) native animations.
    pub fn animations_finished(&self, finished: &[nsh_animation::Finished]) {
        self.guarded(Phase::Call, || {
            if finished.is_empty() {
                return;
            }
            self.ctx.with(|ctx| {
                let raw = ctx.as_raw().as_ptr();
                for f in finished {
                    if crate::animated::finished(raw, f).is_err() {
                        self.report::<()>(&ctx, Err(rquickjs::Error::Exception));
                    }
                }
            });
            self.run_jobs();
        });
    }

    /// Moves the ops committed by JS since the last call into `out`.
    pub fn take_ops(&self, out: &mut Vec<Op>) {
        let mut ops = self.shared.ops.borrow_mut();
        if out.is_empty() {
            std::mem::swap(out, &mut ops);
        } else {
            out.append(&mut ops);
        }
        drop(ops);
        let mut releases = self.shared.releases.borrow_mut();
        out.extend(releases.drain(..).map(|id| Op::Release { id }));
        drop(releases);
        out.extend(
            self.shared
                .animation_releases
                .borrow_mut()
                .drain(..)
                .map(Op::Animation),
        );
    }

    /// Earliest timer deadline, if any timer is pending.
    pub fn next_timer(&self) -> Option<Instant> {
        self.shared.timers.borrow_mut().next_deadline()
    }

    /// Runs all timers due at `now`.
    pub fn run_timers(&self, now: Instant) {
        loop {
            // Release the borrow before calling into JS: callbacks may add timers.
            let Some(callback) = self.shared.timers.borrow_mut().pop_due(now) else {
                break;
            };
            // Each callback, with the microtasks it queues, has a budget of its own.
            let ran = self.guarded(Phase::Call, || {
                self.ctx.with(|ctx| {
                    let result = callback.restore(&ctx).and_then(|f| f.call::<_, ()>(()));
                    self.report(&ctx, result);
                });
                self.run_jobs();
            });
            if ran.is_none() {
                break;
            }
        }
    }

    /// Runs pending promise jobs and microtasks until the queue is empty.
    pub fn run_jobs(&self) {
        self.guarded(Phase::Call, || {
            while self.rt.is_job_pending() {
                if let Err(err) = self.rt.execute_pending_job() {
                    let err = err.0.with(|ctx| {
                        let caught = ctx.catch();
                        let mut err = JsError::new(describe(caught.clone()));
                        err.null = caught.is_null();
                        err
                    });
                    self.uncaught(&err);
                }
            }
        });
    }

    /// Moves the network requests and cancellations made since the last
    /// call into `out`.
    pub fn take_network(&self, out: &mut NetworkQueue) {
        if !self.shared.dropped_fetches.borrow().is_empty() {
            self.ctx
                .with(|ctx| crate::web::forget_dropped_fetches(ctx.as_raw().as_ptr()));
        }
        out.ops.append(&mut self.shared.net.borrow_mut().ops);
    }

    /// Publishes the surface's environment to JS. Call once before
    /// evaluating the bundle (the runtime reads the initial value), then on
    /// every change.
    pub fn set_environment(&self, env: &Environment) {
        self.guarded(Phase::Call, || {
            self.ctx.with(|ctx| {
                let result = (|| {
                    let raw = ctx.as_raw().as_ptr();
                    // SAFETY: the state of this host's context.
                    unsafe { &(*crate::state::state(raw)).dom }.set_platform(env.os, env.desktop());
                    if crate::env::set_environment(raw, env).is_err() {
                        return Err(rquickjs::Error::Exception);
                    }
                    Ok(())
                })();
                self.report(&ctx, result);
            });
            self.run_jobs();
        });
    }

    /// True if JS asked for a display frame (`requestAnimationFrame`).
    pub fn wants_frame(&self) -> bool {
        !self.shared.frames.borrow().is_empty()
    }

    /// Runs the `requestAnimationFrame` callbacks queued so far, with the
    /// current `performance.now()`. Callbacks requested meanwhile wait for
    /// the next frame.
    pub fn run_frame(&self) {
        self.guarded(Phase::Call, || {
            let callbacks = std::mem::take(&mut *self.shared.frames.borrow_mut());
            if callbacks.is_empty() {
                return;
            }
            self.ctx.with(|ctx| {
                let now = (|| -> rquickjs::Result<f64> {
                    let performance: rquickjs::Object = ctx.globals().get("performance")?;
                    performance.get::<_, Function>("now")?.call(())
                })()
                .unwrap_or(0.0);
                for (_, callback) in callbacks {
                    let result = callback.restore(&ctx).and_then(|f| f.call::<_, ()>((now,)));
                    self.report(&ctx, result);
                }
            });
            self.run_jobs();
        });
    }

    /// Moves the font assets (`asset:/…` URIs) imported since the last call
    /// into `out`. They must be registered before the next layout.
    pub fn take_fonts(&self, out: &mut Vec<String>) {
        out.append(&mut self.shared.fonts.borrow_mut());
    }

    /// Delivers an event of request `id` (what the network stack reported).
    pub fn fetch_event(&self, id: RequestId, event: FetchEvent) {
        self.guarded(Phase::Call, || {
            self.ctx.with(|ctx| {
                let raw = ctx.as_raw().as_ptr();
                let result =
                    crate::web::fetch_event(raw, id, event).map_err(|_| rquickjs::Error::Exception);
                self.report(&ctx, result);
            });
            self.run_jobs();
        });
    }

    /// The number of live JS objects (diagnostics: leak checks).
    pub fn object_count(&self) -> i64 {
        self.rt.memory_usage().obj_count
    }

    /// Runs the cycle collector. Call when idle so released nodes are
    /// reported promptly.
    pub fn collect_garbage(&self) {
        self.guarded(Phase::Call, || {
            self.rt.run_gc();
            self.run_jobs();
        });
    }
}

impl Host {
    fn report<T>(&self, ctx: &Ctx<'_>, result: rquickjs::Result<T>) {
        if let Err(err) = result.catch(ctx) {
            self.uncaught(&JsError::caught(err));
        }
    }

    fn uncaught(&self, err: &JsError) {
        let err = self.classify(err.clone());
        match &self.on_error {
            Some(handler) => handler(&err),
            None => log::error!(target: "js", "uncaught exception: {err}"),
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Persistent JS values must be freed before the runtime.
        self.shared.timers.borrow_mut().clear();
        self.shared.frames.borrow_mut().clear();
        self.ctx.with(|ctx| crate::state::uninstall(&ctx));
    }
}

/// The JS object handed to event handlers.
fn payload_object<'js>(ctx: &Ctx<'js>, payload: &EventPayload) -> rquickjs::Result<Value<'js>> {
    let obj = rquickjs::Object::new(ctx.clone())?;
    match payload {
        EventPayload::None => return Ok(Value::new_undefined(ctx.clone())),
        EventPayload::Value(value) => return payload_value(ctx, value),
        EventPayload::Map(fields) => {
            for (key, value) in fields {
                obj.set(*key, payload_value(ctx, value)?)?;
            }
        }
        EventPayload::Native { name, data } => {
            obj.set("name", &**name)?;
            obj.set("data", json_value(ctx, data)?)?;
        }
        EventPayload::TextLayout(lines) => {
            let list = rquickjs::Array::new(ctx.clone())?;
            for (i, l) in lines.iter().enumerate() {
                let line = rquickjs::Object::new(ctx.clone())?;
                line.set("text", l.text.as_str())?;
                line.set("x", l.x)?;
                line.set("y", l.y)?;
                line.set("width", l.width)?;
                line.set("height", l.height)?;
                line.set("ascender", l.ascender)?;
                line.set("descender", l.descender)?;
                line.set("capHeight", l.cap_height)?;
                line.set("xHeight", l.x_height)?;
                list.set(i, line)?;
            }
            obj.set("lines", list)?;
        }
        EventPayload::Layout {
            x,
            y,
            width,
            height,
        } => {
            obj.set("x", *x)?;
            obj.set("y", *y)?;
            obj.set("width", *width)?;
            obj.set("height", *height)?;
        }
        EventPayload::Pan {
            translation_x,
            translation_y,
            velocity_x,
            velocity_y,
            value_x,
            value_y,
        } => {
            obj.set("translationX", *translation_x)?;
            obj.set("translationY", *translation_y)?;
            obj.set("velocityX", *velocity_x)?;
            obj.set("velocityY", *velocity_y)?;
            if let Some(v) = value_x {
                obj.set("valueX", *v)?;
            }
            if let Some(v) = value_y {
                obj.set("valueY", *v)?;
            }
        }
        EventPayload::Scroll { x, y } => {
            obj.set("x", *x)?;
            obj.set("y", *y)?;
        }
        EventPayload::Image { width, height } => {
            obj.set("width", *width)?;
            obj.set("height", *height)?;
        }
        EventPayload::Error { message } => obj.set("message", message.as_str())?,
        EventPayload::Text { text, .. } => obj.set("text", text.as_str())?,
        EventPayload::Touch(touch) => {
            obj.set(
                "phase",
                match touch.phase {
                    TouchPhase::Start => "start",
                    TouchPhase::Move => "move",
                    TouchPhase::End => "end",
                    TouchPhase::Cancel => "cancel",
                },
            )?;
            obj.set("timestamp", touch.timestamp)?;
            let points = rquickjs::Array::new(ctx.clone())?;
            for (i, p) in touch.points.iter().enumerate() {
                let o = rquickjs::Object::new(ctx.clone())?;
                o.set("identifier", p.id)?;
                o.set("target", p.target)?;
                let path = rquickjs::Array::new(ctx.clone())?;
                for (j, id) in p.path.iter().enumerate() {
                    path.set(j, *id)?;
                }
                o.set("path", path)?;
                o.set("pageX", p.page_x)?;
                o.set("pageY", p.page_y)?;
                o.set("locationX", p.location_x)?;
                o.set("locationY", p.location_y)?;
                o.set("changed", p.changed)?;
                o.set("timestamp", touch.timestamp)?;
                points.set(i, o)?;
            }
            obj.set("points", points)?;
        }
    }
    Ok(obj.into_value())
}

/// JSON as a JS value (owned), for natives.
pub(crate) fn json_to_js(raw: *mut qjs::JSContext, json: &nsh_core::JsonValue) -> qjs::JSValue {
    // SAFETY: a context reference; the value's ownership moves out.
    let cx = unsafe { Ctx::from_raw(std::ptr::NonNull::new_unchecked(raw)) };
    match json_value(&cx, json) {
        // A value of our own; the wrapper (and its context reference) go.
        Ok(v) => unsafe { qjs::JS_DupValue(raw, v.as_raw()) },
        Err(_) => qjs::JS_UNDEFINED,
    }
}

/// JSON (a native component's event data) as a JS value.
fn json_value<'js>(ctx: &Ctx<'js>, value: &nsh_core::JsonValue) -> rquickjs::Result<Value<'js>> {
    use nsh_core::JsonValue as J;
    Ok(match value {
        J::Null => Value::new_null(ctx.clone()),
        J::Bool(b) => Value::new_bool(ctx.clone(), *b),
        J::Number(n) => Value::new_float(ctx.clone(), n.as_f64().unwrap_or(0.0)),
        J::String(s) => rquickjs::String::from_str(ctx.clone(), s)?.into_value(),
        J::Array(items) => {
            let array = rquickjs::Array::new(ctx.clone())?;
            for (i, item) in items.iter().enumerate() {
                array.set(i, json_value(ctx, item)?)?;
            }
            array.into_value()
        }
        J::Object(fields) => {
            let object = rquickjs::Object::new(ctx.clone())?;
            for (key, item) in fields {
                object.set(key.as_str(), json_value(ctx, item)?)?;
            }
            object.into_value()
        }
    })
}

fn payload_value<'js>(ctx: &Ctx<'js>, value: &PayloadValue) -> rquickjs::Result<Value<'js>> {
    Ok(match value {
        PayloadValue::Number(n) => Value::new_float(ctx.clone(), *n),
        PayloadValue::Bool(b) => Value::new_bool(ctx.clone(), *b),
        PayloadValue::Str(s) => rquickjs::String::from_str(ctx.clone(), s)?.into_value(),
        PayloadValue::Null => Value::new_null(ctx.clone()),
    })
}

fn describe(value: Value<'_>) -> String {
    match value.as_exception() {
        Some(e) => e.to_string(),
        None => format!("{value:?}"),
    }
}

/// The declarations of the globals besides the web classes and functions
/// (`web/`): the host's (console, timers, frames, below) and QuickJS-NG's
/// that apps use. A test checks that each exists, and that every global
/// the host adds is declared.
pub(crate) const GLOBALS: &str = r#"declare var console: {
  log(...data: unknown[]): void;
  info(...data: unknown[]): void;
  debug(...data: unknown[]): void;
  warn(...data: unknown[]): void;
  error(...data: unknown[]): void;
};

declare var performance: {
  /** Milliseconds since the JS context started, with sub-millisecond precision. */
  now(): number;
};

declare function setTimeout(handler: (...args: any[]) => void, timeout?: number, ...args: any[]): number;
declare function setInterval(handler: (...args: any[]) => void, timeout?: number, ...args: any[]): number;
declare function clearTimeout(id: number | undefined): void;
declare function clearInterval(id: number | undefined): void;
declare function queueMicrotask(callback: () => void): void;
/** Runs `callback` at the next display frame with its time in milliseconds. */
declare function requestAnimationFrame(callback: (time: number) => void): number;
declare function cancelAnimationFrame(id: number): void;
declare function atob(data: string): string;
declare function btoa(data: string): string;

/** `<If when={…} else={…}>…</If>`: renders its children while `when` is
 * truthy, else `else`. A global (no import), like `Each`. */
declare function If(props: {
  when: unknown;
  else?: import("./types").JSX.Element;
  children?: import("./types").JSX.Element;
}): import("./types").JSX.Element;
/** `<Each of={items} key={…}>{(item, index) => …}</Each>`: renders
 * `children(item, index)` for each item of `of` (`item()` and `index()`
 * follow the row); rows are kept by `key` (default: the item; objects by
 * identity, other values by value). `key="index"` keeps rows by position:
 * a changed item updates its row's `item()` instead of making a new row
 * (a list of strings streamed in, edited in place). A global. */
declare function Each<T>(props: {
  of: readonly T[] | null | undefined;
  key?: "index" | ((item: T, index: number) => unknown);
  children: (item: () => T, index: () => number) => import("./types").JSX.Element;
}): import("./types").JSX.Element;

declare class DOMException extends Error {
  constructor(message?: string, name?: string);
  readonly name: string;
  readonly message: string;
  readonly code: number;
}
"#;

fn install(ctx: &Ctx<'_>, shared: &Rc<Shared>) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    console::install(ctx)?;
    web::install(ctx.as_raw().as_ptr())?;
    crate::chunks::install(ctx.as_raw().as_ptr()).map_err(|_| rquickjs::Error::Exception)?;
    crate::dom::install_globals(ctx.as_raw().as_ptr());

    for (name, repeat) in [("setTimeout", false), ("setInterval", true)] {
        let s = shared.clone();
        globals.set(
            name,
            Function::new(
                ctx.clone(),
                move |callback: Function<'_>, delay: Opt<f64>| -> u32 {
                    let ms = delay.0.unwrap_or(0.0);
                    let delay = Duration::from_secs_f64(if ms.is_finite() && ms > 0.0 {
                        ms / 1000.0
                    } else {
                        0.0
                    });
                    let callback = Persistent::save(&callback.ctx().clone(), callback);
                    s.timers.borrow_mut().add(callback, delay, repeat)
                },
            )?,
        )?;
    }
    let s = shared.clone();
    globals.set(
        "requestAnimationFrame",
        Function::new(ctx.clone(), move |callback: Function<'_>| -> u32 {
            let id = s.next_frame_id.get().wrapping_add(1).max(1);
            s.next_frame_id.set(id);
            let callback = Persistent::save(&callback.ctx().clone(), callback);
            s.frames.borrow_mut().push((id, callback));
            id
        })?,
    )?;
    let s = shared.clone();
    globals.set(
        "cancelAnimationFrame",
        Function::new(ctx.clone(), move |id: Opt<u32>| {
            if let Some(id) = id.0 {
                s.frames.borrow_mut().retain(|(i, _)| *i != id);
            }
        })?,
    )?;
    for name in ["clearTimeout", "clearInterval"] {
        let s = shared.clone();
        globals.set(
            name,
            Function::new(ctx.clone(), move |id: Opt<u32>| {
                if let Some(id) = id.0 {
                    s.timers.borrow_mut().cancel(id);
                }
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nsh_core::{EventKind, Op};

    use super::*;

    fn ops(host: &Host) -> Vec<Op> {
        let mut out = Vec::new();
        host.take_ops(&mut out);
        out
    }

    /// Reports `v` as a text node (`report(v)`; the native renderer's ops
    /// are what tests observe).
    const REPORT: &str = r#"import { text } from "nsh:rt";
const report = (v) => text(String(v));
"#;

    /// The text nodes created since the last call.
    fn reported(host: &Host) -> Vec<String> {
        ops(host)
            .into_iter()
            .filter_map(|op| match op {
                Op::CreateText { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn styles_decode_shadows_and_corners() {
        let host = Host::new().unwrap();
        host.eval_source(
            r##"import { element, prop } from "nsh:rt";
            prop(element("view"), "style", {
                flexDirection: "row", backgroundColor: "#ff0000", bogus: 1,
                shadowColor: "#000", shadowOffset: { width: 2, height: 6 },
                shadowOpacity: 0.25, shadowRadius: 10,
                borderRadius: 12, borderBottomLeftRadius: 0,
            });
            prop(element("view"), "style", { shadowOffset: null });"##,
        )
        .unwrap();
        let styles: Vec<_> = ops(&host)
            .into_iter()
            .filter_map(|op| match op {
                Op::SetStyle { style, .. } => Some(style),
                _ => None,
            })
            .collect();
        let style = &styles[0];
        assert_eq!(format!("{:?}", style.layout.flex_direction), "Row");
        assert_eq!(style.view.background_color, nsh_core::Color(0xff00_00ff));
        let shadow = style.view.shadow;
        assert_eq!((shadow.offset_x, shadow.offset_y), (2.0, 6.0));
        assert_eq!((shadow.opacity, shadow.radius), (0.25, 10.0));
        assert_eq!(style.view.radii().to_array(), [12.0, 12.0, 12.0, 0.0]);
        assert_eq!(styles[1].view.shadow, nsh_core::Shadow::DEFAULT);
    }

    /// `style={[base, props.style]}` where `props.style` is itself a list:
    /// nested lists merge left to right, falsy entries skipped.
    #[test]
    fn styles_flatten_nested_lists() {
        let host = Host::new().unwrap();
        host.eval_source(
            r##"import { element, prop } from "nsh:rt";
            const inner = [{ opacity: 0.5 }, false, [null, { backgroundColor: "#00ff00" }]];
            prop(element("view"), "style", [
                { flexDirection: "row", backgroundColor: "#ff0000", opacity: 1 },
                inner,
                undefined,
            ]);"##,
        )
        .unwrap();
        let style = ops(&host)
            .into_iter()
            .find_map(|op| match op {
                Op::SetStyle { style, .. } => Some(style),
                _ => None,
            })
            .unwrap();
        assert_eq!(format!("{:?}", style.layout.flex_direction), "Row");
        assert_eq!(style.view.background_color, nsh_core::Color(0x00ff_00ff));
        assert_eq!(style.view.opacity, 0.5);
    }

    /// `Intl` and the `toLocaleString`s format natively, in the device's
    /// locale by default.
    #[test]
    fn intl_formats_numbers_and_dates() {
        let host = Host::new().unwrap();
        host.eval_source(&format!(
            r#"{REPORT}
            report((1234567.891).toLocaleString());
            report((1234.5).toLocaleString("de-DE", {{ style: "currency", currency: "EUR" }}));
            report((0.256).toLocaleString("en-US", {{ style: "percent", maximumFractionDigits: 1 }}));
            report(12345678901234567890n.toLocaleString("en-US"));
            const compact = new Intl.NumberFormat("en", {{ notation: "compact" }});
            report([1234, 1500000, 999999].map(compact.format).join(" "));
            report(new Intl.NumberFormat("en", {{ signDisplay: "always", minimumFractionDigits: 2 }}).format(3));
            report(new Intl.NumberFormat("en", {{ useGrouping: false }}).format(12345));
            report(JSON.stringify(new Intl.NumberFormat("en", {{ style: "currency", currency: "USD" }}).formatToParts(-1.5)));
            report(new Intl.NumberFormat("fr").resolvedOptions().locale);
            const at = Date.UTC(2026, 9, 16, 15, 4, 5);
            const utc = {{ timeZone: "UTC" }};
            report(new Intl.DateTimeFormat("en-US", {{ ...utc, hour: "2-digit", minute: "2-digit", hour12: false }}).format(at));
            report(new Date(at).toLocaleString("en-US", utc));
            report(new Date(at).toLocaleDateString("en-US", {{ ...utc, dateStyle: "full" }}));
            report(new Date(at).toLocaleTimeString("ko-KR", {{ timeZone: "+09:00" }}));
            report(new Intl.DateTimeFormat("en-US", {{ timeZone: "UTC", dateStyle: "medium", timeStyle: "short" }}).format(new Date(at)));
            report(new Intl.DateTimeFormat().resolvedOptions().locale);
            report(new Date(NaN).toLocaleString());
            try {{ new Intl.NumberFormat("en", {{ style: "currency" }}); }} catch (e) {{ report(e.name); }}
            try {{ new Intl.DateTimeFormat("en", {{ timeZone: "Mars/Olympus" }}); }} catch (e) {{ report(e.name); }}
            report(Intl.NumberFormat.supportedLocalesOf(["xx", "ja-jp"]).join());
            report(Object.prototype.toString.call(Intl));
            "#
        ))
        .unwrap();
        assert_eq!(
            reported(&host),
            [
                "1,234,567.891",
                "1.234,50\u{a0}€",
                "25.6%",
                "12,345,678,901,234,567,890",
                "1.2K 1.5M 1M",
                "+3.00",
                "12345",
                r#"[{"type":"minusSign","value":"-"},{"type":"currency","value":"$"},{"type":"integer","value":"1"},{"type":"decimal","value":"."},{"type":"fraction","value":"50"}]"#,
                "fr",
                "15:04",
                "10/16/2026, 3:04:05 PM",
                "Friday, October 16, 2026",
                "오전 12:04:05",
                "Oct 16, 2026, 3:04 PM",
                "en-US",
                "Invalid Date",
                "TypeError",
                "RangeError",
                "ja-JP",
                "[object Intl]",
            ]
        );
    }

    #[test]
    fn timers_fire_in_order_and_cancel() {
        let host = Host::new().unwrap();
        host.eval_source(
            r#"
            globalThis.log = [];
            setTimeout(() => log.push("b"), 20);
            setTimeout(() => log.push("a"), 0);
            const c = setTimeout(() => log.push("cancelled"), 10);
            clearTimeout(c);
            Promise.resolve().then(() => log.push("micro"));
            "#,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_millis(30);
        while let Some(next) = host.next_timer() {
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
            host.run_timers(Instant::now());
            assert!(Instant::now() < deadline + Duration::from_secs(1));
        }
        host.eval_source(&format!("{REPORT}report(log.join(\",\"));"))
            .unwrap();
        assert_eq!(reported(&host), ["micro,a,b"]);
    }

    fn sandboxed(limits: Limits) -> Host {
        Host::with_options(HostOptions {
            limits,
            harden: true,
            ..HostOptions::default()
        })
        .unwrap()
    }

    #[test]
    fn calls_over_budget_are_interrupted_and_the_vm_goes_on() {
        let mut host = sandboxed(Limits {
            call_budget: Duration::from_millis(50),
            ..Limits::default()
        });
        let errors = Rc::new(RefCell::new(Vec::new()));
        let seen = errors.clone();
        host.set_error_handler(Box::new(move |e| seen.borrow_mut().push(e.message.clone())));
        // Views 2 and 4: the first loops when pressed.
        host.eval_source(
            r#"import { element, prop } from "nsh:rt";
            globalThis.views = [true, false].map((loops) => {
                const view = element("view");
                prop(view, "onPress", () => {
                    // try/catch cannot swallow the interrupt.
                    try { if (loops) for (;;) {} } catch (e) {}
                    element("view");
                });
                return view;
            });"#,
        )
        .unwrap();
        ops(&host);
        let start = Instant::now();
        host.dispatch_event(2, EventKind::Press, &EventPayload::None);
        assert!(start.elapsed() < Duration::from_secs(2));
        let errors = errors.borrow().clone();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("budget of 50 ms"), "{}", errors[0]);
        assert!(ops(&host).is_empty());
        // The next call runs normally.
        host.dispatch_event(4, EventKind::Press, &EventPayload::None);
        let created = ops(&host)
            .into_iter()
            .filter(|op| matches!(op, Op::Create { .. }))
            .count();
        assert_eq!(created, 1);
        assert!(host.fatal().is_none());
    }

    #[test]
    fn running_out_of_memory_stops_the_vm() {
        let host = sandboxed(Limits {
            memory_bytes: 16 << 20,
            ..Limits::default()
        });
        let err = host
            .eval_source("const keep = []; for (;;) keep.push(new Array(100000).fill(1));")
            .unwrap_err();
        assert!(err.message.contains("limited to 16 MB"), "{}", err.message);
        assert!(host.fatal().is_some());
        // Nothing runs any more.
        assert!(
            host.eval_source(r#"import { element } from "nsh:rt"; element("view");"#)
                .is_err()
        );
        assert!(ops(&host).is_empty());
    }

    #[test]
    fn running_out_of_memory_in_a_timer_stops_the_vm() {
        let mut host = sandboxed(Limits {
            memory_bytes: 16 << 20,
            ..Limits::default()
        });
        let errors = Rc::new(RefCell::new(Vec::new()));
        let seen = errors.clone();
        host.set_error_handler(Box::new(move |e| seen.borrow_mut().push(e.message.clone())));
        // JSON.parse running out throws a null exception, not the usual
        // InternalError.
        host.eval_source(
            r#"const keep = [];
            setTimeout(() => {
                for (let n = 0; ; n++) {
                    const parts = [];
                    for (let i = 0; i < 1000; i++) parts.push(`{"path":"/a/b/c-${n}-${i}","children":[]}`);
                    keep.push(JSON.parse("[" + parts.join(",") + "]"));
                }
            }, 0);"#,
        )
        .unwrap();
        host.run_timers(Instant::now() + Duration::from_millis(1));
        let errors = errors.borrow().clone();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("limited to 16 MB"), "{}", errors[0]);
        assert!(host.fatal().is_some());
    }

    #[test]
    fn running_out_of_memory_stops_the_vm_even_when_caught() {
        let mut host = sandboxed(Limits {
            memory_bytes: 16 << 20,
            ..Limits::default()
        });
        let errors = Rc::new(RefCell::new(Vec::new()));
        let seen = errors.clone();
        host.set_error_handler(Box::new(move |e| seen.borrow_mut().push(e.message.clone())));
        host.eval_source(
            r#"setTimeout(() => {
                try {
                    const keep = [];
                    for (;;) keep.push(new Array(100000).fill(1));
                } catch (e) {}
            }, 0);"#,
        )
        .unwrap();
        host.run_timers(Instant::now() + Duration::from_millis(1));
        let errors = errors.borrow().clone();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains("limited to 16 MB") && errors[0].contains("in pages"),
            "{}",
            errors[0]
        );
        assert!(host.fatal().is_some());
    }

    #[test]
    fn too_many_changes_at_once_stop_the_vm() {
        let host = sandboxed(Limits {
            max_op_values: 1000,
            ..Limits::default()
        });
        assert!(
            host.eval_source(
                r#"import { element } from "nsh:rt"; for (let i = 0; i < 2000; i++) element("view");"#
            )
            .is_err()
        );
        assert!(host.fatal().unwrap().contains("more than 1000 changes"));
    }

    #[test]
    fn hardened_vms_run_no_code_from_strings() {
        // Hardened from source, and from the bytecode the CLI ships to
        // runtimes without a parser.
        let from_bytecode = Host::with_options(HostOptions {
            harden: true,
            harden_bytecode: Some(crate::compile_harden().unwrap().into()),
            ..HostOptions::default()
        })
        .unwrap();
        for host in [sandboxed(Limits::default()), from_bytecode] {
            hardened_vm_runs_no_code_from_strings(&host);
        }
    }

    fn hardened_vm_runs_no_code_from_strings(host: &Host) {
        host.eval_source(&(REPORT.to_owned()
            + r#"
            const results = [];
            const attempt = (f) => { try { f(); results.push("ran"); } catch (e) { results.push(e.constructor.name); } };
            attempt(() => eval("1"));
            attempt(() => new Function("return 1")());
            attempt(() => (function () {}).constructor("return 1")());
            attempt(() => (async function () {}).constructor("return 1"));
            attempt(() => (function* () {}).constructor("yield 1"));
            attempt(() => { "use strict"; Array.prototype.map = null; });
            attempt(() => { "use strict"; Object.prototype.polluted = 1; });
            // Ordinary code is unaffected.
            class MyError extends Error { constructor() { super("x"); this.name = "MyError"; } }
            attempt(() => { const e = new MyError(); if (e.name !== "MyError") throw new Error(); });
            attempt(() => [1, 2].map((x) => x * 2));
            // Members of the frozen prototypes can still be overridden on
            // other objects (no override mistake), not on the prototypes.
            attempt(() => {
                "use strict";
                function A() {}
                A.prototype = Object.create(Object.prototype);
                A.prototype.constructor = A;
                const o = { toString: undefined };
                o.toString = () => "mine";
                const d = new Date(0);
                d.toJSON = () => "j";
                if (new A().constructor !== A || String(o) !== "mine" || JSON.stringify(d) !== '"j"') throw new Error();
                if (({}).constructor !== Object || [].toString() !== "") throw new Error();
            });
            attempt(() => { "use strict"; Object.prototype.toString = null; });
            report(results.join(","));
            "#))
        .unwrap();
        assert_eq!(
            reported(host),
            [
                "ReferenceError,TypeError,TypeError,TypeError,TypeError,TypeError,TypeError,ran,ran,ran,TypeError"
            ]
        );
    }

    #[test]
    fn development_vms_keep_eval() {
        let host = Host::new().unwrap();
        host.eval_source(&format!("{REPORT}report(eval(\"1 + 1\"));"))
            .unwrap();
        assert_eq!(reported(&host), ["2"]);
    }
}
