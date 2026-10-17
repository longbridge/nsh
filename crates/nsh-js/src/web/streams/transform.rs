//! `TransformStream` and its controller.

use std::cell::RefMut;

use super::{
    Algo, QueuingStrategy, method,
    readable::{self, State},
    writable::{self, WState},
};
use crate::web::{
    Class, Key, Object, Spec, data, instance,
    js::{self, Ctx, Deferred, R, Raw, V, native, trace},
};

pub(crate) struct Stream {
    pub(crate) readable: V,
    pub(crate) writable: V,
    backpressure: Option<bool>,
    backpressure_change: Option<Deferred>,
    controller: V,
}

trace!(Stream {
    readable,
    writable,
    backpressure_change,
    controller
});

impl Object for Stream {}

pub(crate) struct Controller {
    stream: V,
    transform: Algo,
    flush: Algo,
    cancel: Algo,
    finish: Option<Deferred>,
}

trace!(Controller {
    stream,
    transform,
    flush,
    cancel,
    finish
});

impl Object for Controller {}

fn stream<'a>(ctx: Ctx, v: Raw) -> R<RefMut<'a, Stream>> {
    data::<Stream>(ctx, v, "TransformStream")
}

fn controller<'a>(ctx: Ctx, v: Raw) -> R<RefMut<'a, Controller>> {
    data::<Controller>(ctx, v, "TransformStreamDefaultController")
}

fn parts(ctx: Ctx, s: Raw) -> R<(V, V, V)> {
    let st = stream(ctx, s)?;
    Ok((
        st.readable.clone_ref(),
        st.writable.clone_ref(),
        st.controller.clone_ref(),
    ))
}

/// The controller of the readable side.
fn readable_controller(ctx: Ctx, s: Raw) -> R<V> {
    let r = stream(ctx, s)?.readable.clone_ref();
    readable::controller_of(ctx, r.raw())
}

fn readable_controller_of(ctx: Ctx, r: Raw) -> R<V> {
    readable::controller_of(ctx, r)
}

/// SetBackpressure.
fn set_backpressure(ctx: Ctx, s: Raw, backpressure: bool) -> R<()> {
    let old = {
        let mut st = stream(ctx, s)?;
        let old = st.backpressure_change.replace(Deferred::new(ctx)?);
        st.backpressure = Some(backpressure);
        old
    };
    if let Some(d) = old {
        d.resolve(ctx, js::UNDEFINED);
    }
    Ok(())
}

fn unblock_write(ctx: Ctx, s: Raw) -> R<()> {
    if stream(ctx, s)?.backpressure == Some(true) {
        set_backpressure(ctx, s, false)?;
    }
    Ok(())
}

fn clear_algorithms(ctx: Ctx, c: Raw) -> R<()> {
    let old = {
        let mut ct = controller(ctx, c)?;
        (
            std::mem::take(&mut ct.transform),
            std::mem::take(&mut ct.flush),
            std::mem::take(&mut ct.cancel),
        )
    };
    drop(old);
    Ok(())
}

/// TransformStreamErrorWritableAndUnblockWrite.
fn error_writable_and_unblock(ctx: Ctx, s: Raw, e: Raw) -> R<()> {
    let (_, w, c) = parts(ctx, s)?;
    clear_algorithms(ctx, c.raw())?;
    let wc = writable::controller_of(ctx, w.raw())?;
    writable::error_if_needed(ctx, wc.raw(), e)?;
    unblock_write(ctx, s)
}

/// TransformStreamError.
fn error_stream(ctx: Ctx, s: Raw, e: Raw) -> R<()> {
    let rc = readable_controller(ctx, s)?;
    readable::dc_error(ctx, rc.raw(), e)?;
    error_writable_and_unblock(ctx, s, e)
}

/// TransformStreamDefaultControllerEnqueue.
pub(crate) fn enqueue(ctx: Ctx, c: Raw, chunk: Raw) -> R<()> {
    let s = controller(ctx, c)?.stream.clone_ref();
    let rc = readable_controller(ctx, s.raw())?;
    if !readable::dc_can_close_or_enqueue(ctx, rc.raw())? {
        return Err(js::type_error(
            ctx,
            "the readable side cannot be enqueued to",
        ));
    }
    if readable::dc_enqueue(ctx, rc.raw(), chunk).is_err() {
        let e = js::take_exception(ctx);
        error_writable_and_unblock(ctx, s.raw(), e.raw())?;
        let r = stream(ctx, s.raw())?.readable.clone_ref();
        let stored = readable::stored_error(ctx, r.raw())?;
        return Err(js::throw(ctx, stored));
    }
    let bp = readable::dc_has_backpressure(ctx, rc.raw())?;
    if Some(bp) != stream(ctx, s.raw())?.backpressure {
        set_backpressure(ctx, s.raw(), true)?;
    }
    Ok(())
}

/// TransformStreamDefaultControllerTerminate.
fn terminate(ctx: Ctx, c: Raw) -> R<()> {
    let s = controller(ctx, c)?.stream.clone_ref();
    let rc = readable_controller(ctx, s.raw())?;
    readable::dc_close(ctx, rc.raw())?;
    let e = js::type_error_value(ctx, "the TransformStream was terminated");
    error_writable_and_unblock(ctx, s.raw(), e.raw())
}

/// TransformStreamDefaultControllerPerformTransform.
fn perform_transform(ctx: Ctx, c: Raw, chunk: Raw) -> R<V> {
    let transform = controller(ctx, c)?.transform.clone_ref();
    let p = transform.promise(ctx, &[chunk, c])?;
    let s = controller(ctx, c)?.stream.clone_ref();
    let on_error = js::closure(ctx, vec![s], |ctx, cap, args| {
        error_stream(ctx, cap[0].raw(), args.get(0))?;
        Err(js::throw(ctx, V::dup(ctx, args.get(0))))
    })?;
    js::then(ctx, p.raw(), js::UNDEFINED, on_error.raw())
}

fn sink_write(ctx: Ctx, s: Raw, chunk: Raw) -> R<V> {
    let (backpressure, change, c) = {
        let st = stream(ctx, s)?;
        (
            st.backpressure,
            st.backpressure_change
                .as_ref()
                .map(|d| d.promise.clone_ref()),
            st.controller.clone_ref(),
        )
    };
    if backpressure == Some(true)
        && let Some(change) = change
    {
        let then = js::closure(
            ctx,
            vec![V::dup(ctx, s), c, V::dup(ctx, chunk)],
            |ctx, cap, _| {
                let w = stream(ctx, cap[0].raw())?.writable.clone_ref();
                if writable::state_of(ctx, w.raw())? == WState::Erroring {
                    let e = writable::stored_error(ctx, w.raw())?;
                    return Err(js::throw(ctx, e));
                }
                perform_transform(ctx, cap[1].raw(), cap[2].raw())
            },
        )?;
        return js::then(ctx, change.raw(), then.raw(), js::UNDEFINED);
    }
    perform_transform(ctx, c.raw(), chunk)
}

/// The finish promise of an algorithm that settles the other side.
fn finish_with(
    ctx: Ctx,
    c: Raw,
    run: impl FnOnce(&Controller) -> Algo,
    args: &[Raw],
    on_ok: impl Fn(Ctx, &[V]) -> R<()> + 'static,
    on_err: impl Fn(Ctx, &[V], Raw) -> R<()> + 'static,
    captures: Vec<V>,
) -> R<V> {
    if let Some(d) = &controller(ctx, c)?.finish {
        return Ok(d.promise.clone_ref());
    }
    let d = Deferred::new(ctx)?;
    let result = d.promise.clone_ref();
    let algo = {
        let mut ct = controller(ctx, c)?;
        let a = run(&ct);
        ct.finish = Some(Deferred {
            promise: d.promise.clone_ref(),
            resolve: d.resolve.clone_ref(),
            reject: d.reject.clone_ref(),
        });
        a
    };
    let p = algo.promise(ctx, args)?;
    clear_algorithms(ctx, c)?;
    let mut caps = captures;
    caps.push(d.resolve.clone_ref());
    caps.push(d.reject.clone_ref());
    js::upon(
        ctx,
        p.raw(),
        caps,
        move |ctx, cap, _| {
            let n = cap.len();
            match on_ok(ctx, &cap[..n - 2]) {
                Ok(()) => {
                    js::call(ctx, cap[n - 2].raw(), js::UNDEFINED, &[js::UNDEFINED]).map(drop)
                }
                Err(_) => {
                    let e = js::take_exception(ctx);
                    js::call(ctx, cap[n - 1].raw(), js::UNDEFINED, &[e.raw()]).map(drop)
                }
            }
        },
        move |ctx, cap, e| {
            let n = cap.len();
            on_err(ctx, &cap[..n - 2], e)?;
            js::call(ctx, cap[n - 1].raw(), js::UNDEFINED, &[e]).map(drop)
        },
    )?;
    Ok(result)
}

fn sink_abort(ctx: Ctx, s: Raw, reason: Raw) -> R<V> {
    let (r, _, c) = parts(ctx, s)?;
    finish_with(
        ctx,
        c.raw(),
        |ct| ct.cancel.clone_ref(),
        &[reason],
        |ctx, cap| {
            let r = cap[0].raw();
            if readable::state_of(ctx, r)? == State::Errored {
                let e = readable::stored_error(ctx, r)?;
                return Err(js::throw(ctx, e));
            }
            let rc = readable_controller_of(ctx, r)?;
            readable::dc_error(ctx, rc.raw(), cap[1].raw())
        },
        |ctx, cap, e| {
            let rc = readable_controller_of(ctx, cap[0].raw())?;
            readable::dc_error(ctx, rc.raw(), e)
        },
        vec![r, V::dup(ctx, reason)],
    )
}

fn sink_close(ctx: Ctx, s: Raw) -> R<V> {
    let (r, _, c) = parts(ctx, s)?;
    finish_with(
        ctx,
        c.raw(),
        |ct| ct.flush.clone_ref(),
        &[c.raw()],
        |ctx, cap| {
            let r = cap[0].raw();
            if readable::state_of(ctx, r)? == State::Errored {
                let e = readable::stored_error(ctx, r)?;
                return Err(js::throw(ctx, e));
            }
            let rc = readable_controller_of(ctx, r)?;
            readable::dc_close(ctx, rc.raw())
        },
        |ctx, cap, e| {
            let rc = readable_controller_of(ctx, cap[0].raw())?;
            readable::dc_error(ctx, rc.raw(), e)
        },
        vec![r],
    )
}

fn source_cancel(ctx: Ctx, s: Raw, reason: Raw) -> R<V> {
    let (_, w, c) = parts(ctx, s)?;
    finish_with(
        ctx,
        c.raw(),
        |ct| ct.cancel.clone_ref(),
        &[reason],
        |ctx, cap| {
            let w = cap[0].raw();
            if writable::state_of(ctx, w)? == WState::Errored {
                let e = writable::stored_error(ctx, w)?;
                return Err(js::throw(ctx, e));
            }
            let wc = writable::controller_of(ctx, w)?;
            writable::error_if_needed(ctx, wc.raw(), cap[2].raw())?;
            unblock_write(ctx, cap[1].raw())
        },
        |ctx, cap, e| {
            let wc = writable::controller_of(ctx, cap[0].raw())?;
            writable::error_if_needed(ctx, wc.raw(), e)?;
            unblock_write(ctx, cap[1].raw())
        },
        vec![w, V::dup(ctx, s), V::dup(ctx, reason)],
    )
}

fn source_pull(ctx: Ctx, s: Raw) -> R<V> {
    set_backpressure(ctx, s, false)?;
    let change = stream(ctx, s)?
        .backpressure_change
        .as_ref()
        .map(|d| d.promise.clone_ref());
    match change {
        Some(p) => Ok(p),
        None => js::resolved(ctx, js::UNDEFINED),
    }
}

/// The algorithms of a transformer.
pub(crate) struct Transformer {
    pub(crate) transform: Algo,
    pub(crate) flush: Algo,
    pub(crate) cancel: Algo,
}

/// The queuing of the two sides: high water marks and size algorithms.
pub(crate) struct Sides {
    pub(crate) writable_hwm: f64,
    pub(crate) writable_size: V,
    pub(crate) readable_hwm: f64,
    pub(crate) readable_size: V,
}

impl Sides {
    /// The defaults (`new TransformStream()` without strategies).
    pub(crate) fn default_sides() -> Sides {
        Sides {
            writable_hwm: 1.0,
            writable_size: V::undefined(),
            readable_hwm: 0.0,
            readable_size: V::undefined(),
        }
    }
}

/// InitializeTransformStream and the controller's setup: a transform
/// stream of `transformer`, started when the returned promise resolves.
pub(crate) fn new_transform(
    ctx: Ctx,
    new_target: Raw,
    class: Class,
    transformer: Transformer,
    sides: Sides,
) -> R<(V, V, Deferred)> {
    let Transformer {
        transform,
        flush,
        cancel,
    } = transformer;
    let Sides {
        writable_hwm,
        writable_size,
        readable_hwm,
        readable_size,
    } = sides;
    let s = instance(
        ctx,
        new_target,
        class,
        Stream {
            readable: V::undefined(),
            writable: V::undefined(),
            backpressure: None,
            backpressure_change: None,
            controller: V::undefined(),
        },
    )?;
    let start = Deferred::new(ctx)?;
    let start_promise = start.promise.clone_ref();
    let start_algo = js::closure(ctx, vec![start_promise], |_, c, _| Ok(c[0].clone_ref()))?;
    let write = js::closure(ctx, vec![s.clone_ref()], |ctx, c, args| {
        sink_write(ctx, c[0].raw(), args.get(0))
    })?;
    let abort = js::closure(ctx, vec![s.clone_ref()], |ctx, c, args| {
        sink_abort(ctx, c[0].raw(), args.get(0))
    })?;
    let close = js::closure(ctx, vec![s.clone_ref()], |ctx, c, _| {
        sink_close(ctx, c[0].raw())
    })?;
    let sink = writable::Sink {
        start: Algo::native(start_algo.clone_ref()),
        write: Algo::native(write),
        close: Algo::native(close),
        abort: Algo::native(abort),
    };
    let (w, _) = writable::new_writable_stream(ctx, sink, writable_hwm, writable_size)?;
    let pull = js::closure(ctx, vec![s.clone_ref()], |ctx, c, _| {
        source_pull(ctx, c[0].raw())
    })?;
    let cancel_source = js::closure(ctx, vec![s.clone_ref()], |ctx, c, args| {
        source_cancel(ctx, c[0].raw(), args.get(0))
    })?;
    let (r, _) = readable::new_default_stream(
        ctx,
        Algo::native(start_algo),
        Algo::native(pull),
        Algo::native(cancel_source),
        readable_hwm,
        readable_size,
    )?;
    {
        let mut st = stream(ctx, s.raw())?;
        st.readable = r;
        st.writable = w;
    }
    set_backpressure(ctx, s.raw(), true)?;
    let c = instance(
        ctx,
        js::UNDEFINED,
        Class::TransformStreamDefaultController,
        Controller {
            stream: s.clone_ref(),
            transform,
            flush,
            cancel,
            finish: None,
        },
    )?;
    stream(ctx, s.raw())?.controller = c.clone_ref();
    Ok((s, c, start))
}

native!(
    /// The default transform: chunks pass through.
    fn identity_transform(ctx, this, args) {
        enqueue(ctx, args.get(1), args.get(0))?;
        Ok(V::undefined())
    }
);

native!(
    fn stream_constructor(ctx, this, args) {
        let transformer = args.get(0);
        if !js::is_nullish(transformer) && !js::is_object(transformer) {
            return Err(js::type_error(ctx, "the transformer must be an object"));
        }
        let writable_strategy = QueuingStrategy::read(ctx, args.get(1))?;
        let readable_strategy = QueuingStrategy::read(ctx, args.get(2))?;
        let t = V::dup(ctx, transformer);
        let cancel = method(ctx, transformer, "cancel")?;
        let flush = method(ctx, transformer, "flush")?;
        if js::is_object(transformer) {
            for key in ["readableType", "writableType"] {
                let v = js::get(ctx, transformer, key)?;
                if !v.is_undefined() {
                    return Err(js::range_error(ctx, &format!("{key} is not supported")));
                }
            }
        }
        let start = method(ctx, transformer, "start")?;
        let transform = method(ctx, transformer, "transform")?;
        let readable_hwm = readable_strategy.high_water_mark(ctx, 0.0)?;
        let writable_hwm = writable_strategy.high_water_mark(ctx, 1.0)?;
        let transform = if transform.is_undefined() {
            Algo::native(js::function(ctx, c"transform", identity_transform, 2)?)
        } else {
            Algo::method(transform, &t)
        };
        let transformer = Transformer {
            transform,
            flush: Algo::method(flush, &t),
            cancel: Algo::method(cancel, &t),
        };
        let sides = Sides {
            writable_hwm,
            writable_size: writable_strategy.size,
            readable_hwm,
            readable_size: readable_strategy.size,
        };
        let (s, c, start_promise) = new_transform(ctx, this, Class::TransformStream, transformer, sides)?;
        let started = Algo::method(start, &t).call(ctx, &[c.raw()])?;
        start_promise.resolve(ctx, started.raw());
        Ok(s)
    }
);

native!(
    fn stream_readable(ctx, this, args) {
        Ok(stream(ctx, this)?.readable.clone_ref())
    }
);

native!(
    fn stream_writable(ctx, this, args) {
        Ok(stream(ctx, this)?.writable.clone_ref())
    }
);

native!(
    fn controller_desired_size(ctx, this, args) {
        let s = controller(ctx, this)?.stream.clone_ref();
        let rc = readable_controller(ctx, s.raw())?;
        Ok(match readable::dc_desired_size(ctx, rc.raw())? {
            Some(n) => V::num(n),
            None => V::null(),
        })
    }
);

native!(
    fn controller_enqueue(ctx, this, args) {
        enqueue(ctx, this, args.get(0))?;
        Ok(V::undefined())
    }
);

native!(
    fn controller_error(ctx, this, args) {
        let s = controller(ctx, this)?.stream.clone_ref();
        error_stream(ctx, s.raw(), args.get(0))?;
        Ok(V::undefined())
    }
);

native!(
    fn controller_terminate(ctx, this, args) {
        terminate(ctx, this)?;
        Ok(V::undefined())
    }
);

pub(crate) const SPECS: &[Spec] = &[
    Spec {
        class: Class::TransformStream,
        name: c"TransformStream",
        parent: None,
        constructor: Some((stream_constructor, 0)),
        methods: &[],
        accessors: &[
            (c"readable", stream_readable, None),
            (c"writable", stream_writable, None),
        ],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "<I = any, O = any>",
            members: r#"constructor(transformer?: Transformer<I, O>, writableStrategy?: QueuingStrategy<I>, readableStrategy?: QueuingStrategy<O>);
readonly readable: ReadableStream<O>;
readonly writable: WritableStream<I>;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::TransformStreamDefaultController,
        name: c"TransformStreamDefaultController",
        parent: None,
        constructor: None,
        methods: &[
            (Key::Name(c"enqueue"), controller_enqueue, 0),
            (Key::Name(c"error"), controller_error, 0),
            (Key::Name(c"terminate"), controller_terminate, 0),
        ],
        accessors: &[(c"desiredSize", controller_desired_size, None)],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "<O = any>",
            members: r#"readonly desiredSize: number | null;
enqueue(chunk?: O): void;
error(reason?: any): void;
terminate(): void;"#,
        }
        .declared(),
    },
];

/// The TypeScript types of this file's classes (`globals.d.ts`).
pub(crate) const TYPES: &str = r#"interface Transformer<I = any, O = any> {
  start?(controller: TransformStreamDefaultController<O>): any;
  transform?(chunk: I, controller: TransformStreamDefaultController<O>): void | PromiseLike<void>;
  flush?(controller: TransformStreamDefaultController<O>): void | PromiseLike<void>;
  cancel?(reason?: any): void | PromiseLike<void>;
}
"#;
