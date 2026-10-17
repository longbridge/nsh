//! `WritableStream`, its writer and controller.

use std::{cell::RefMut, collections::VecDeque};

use super::{Algo, QueuingStrategy, SizedQueue, chunk_size, method};
use crate::web::{
    Class, Key, Object, Spec, data, instance, is,
    js::{self, Ctx, Deferred, R, Raw, V, native, trace},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum WState {
    Writable,
    Closed,
    Erroring,
    Errored,
}

struct PendingAbort {
    promise: Deferred,
    reason: V,
    was_already_erroring: bool,
}

trace!(PendingAbort { promise, reason });

pub(crate) struct Stream {
    state: WState,
    stored_error: V,
    writer: V,
    controller: V,
    in_flight_write: Option<Deferred>,
    close_request: Option<Deferred>,
    in_flight_close: Option<Deferred>,
    pending_abort: Option<PendingAbort>,
    write_requests: VecDeque<Deferred>,
    backpressure: bool,
}

trace!(Stream {
    stored_error,
    writer,
    controller,
    in_flight_write,
    close_request,
    in_flight_close,
    pending_abort,
    write_requests
});

impl Object for Stream {}

pub(crate) struct Writer {
    stream: V,
    ready: Deferred,
    closed: Deferred,
}

trace!(Writer {
    stream,
    ready,
    closed
});

impl Object for Writer {}

/// The marker of a queued close.
struct CloseSentinel;

trace!(CloseSentinel {});

impl Object for CloseSentinel {}

pub(crate) struct Controller {
    stream: V,
    queue: SizedQueue,
    signal_controller: V,
    started: bool,
    hwm: f64,
    size: V,
    write: Algo,
    close: Algo,
    abort: Algo,
}

trace!(Controller {
    stream,
    queue,
    signal_controller,
    size,
    write,
    close,
    abort
});

impl Object for Controller {}

fn stream<'a>(ctx: Ctx, v: Raw) -> R<RefMut<'a, Stream>> {
    data::<Stream>(ctx, v, "WritableStream")
}

fn writer<'a>(ctx: Ctx, v: Raw) -> R<RefMut<'a, Writer>> {
    data::<Writer>(ctx, v, "WritableStreamDefaultWriter")
}

fn controller<'a>(ctx: Ctx, v: Raw) -> R<RefMut<'a, Controller>> {
    data::<Controller>(ctx, v, "WritableStreamDefaultController")
}

pub(crate) fn is_stream(ctx: Ctx, v: Raw) -> bool {
    is::<Stream>(ctx, v)
}

pub(crate) fn is_locked(ctx: Ctx, s: Raw) -> R<bool> {
    Ok(!stream(ctx, s)?.writer.is_undefined())
}

pub(crate) fn state_of(ctx: Ctx, s: Raw) -> R<WState> {
    Ok(stream(ctx, s)?.state)
}

pub(crate) fn stored_error(ctx: Ctx, s: Raw) -> R<V> {
    Ok(stream(ctx, s)?.stored_error.clone_ref())
}

pub(crate) fn controller_of(ctx: Ctx, s: Raw) -> R<V> {
    Ok(stream(ctx, s)?.controller.clone_ref())
}

/// WritableStreamCloseQueuedOrInFlight.
pub(crate) fn close_queued_or_in_flight(ctx: Ctx, s: Raw) -> R<bool> {
    let st = stream(ctx, s)?;
    Ok(st.close_request.is_some() || st.in_flight_close.is_some())
}

fn has_operation_in_flight(ctx: Ctx, s: Raw) -> R<bool> {
    let st = stream(ctx, s)?;
    Ok(st.in_flight_write.is_some() || st.in_flight_close.is_some())
}

fn resolved_deferred(ctx: Ctx) -> R<Deferred> {
    let d = Deferred::new(ctx)?;
    d.resolve(ctx, js::UNDEFINED);
    Ok(d)
}

fn rejected_deferred(ctx: Ctx, e: Raw) -> R<Deferred> {
    let d = Deferred::new(ctx)?;
    d.reject(ctx, e);
    js::mark_handled(ctx, d.promise.raw());
    Ok(d)
}

fn is_pending(ctx: Ctx, d: &Deferred) -> bool {
    // SAFETY: a live promise.
    unsafe {
        rquickjs::qjs::JS_PromiseState(ctx, d.promise.raw())
            == rquickjs::qjs::JSPromiseStateEnum_JS_PROMISE_PENDING
    }
}

fn new_stream(ctx: Ctx, new_target: Raw) -> R<V> {
    instance(
        ctx,
        new_target,
        Class::WritableStream,
        Stream {
            state: WState::Writable,
            stored_error: V::undefined(),
            writer: V::undefined(),
            controller: V::undefined(),
            in_flight_write: None,
            close_request: None,
            in_flight_close: None,
            pending_abort: None,
            write_requests: VecDeque::new(),
            backpressure: false,
        },
    )
}

// --- Stream operations ----------------------------------------------------

/// WritableStreamAbort.
pub(crate) fn abort_stream(ctx: Ctx, s: Raw, reason: Raw) -> R<V> {
    let (state, signal_controller) = {
        let st = stream(ctx, s)?;
        let c = st.controller.clone_ref();
        drop(st);
        (
            state_of(ctx, s)?,
            controller(ctx, c.raw())?.signal_controller.clone_ref(),
        )
    };
    if matches!(state, WState::Closed | WState::Errored) {
        return js::resolved(ctx, js::UNDEFINED);
    }
    // Signal the sink's abort first (it may change the state).
    let signal = js::get(ctx, signal_controller.raw(), "signal")?;
    crate::web::event::abort(ctx, signal.raw(), reason)?;
    let state = state_of(ctx, s)?;
    if matches!(state, WState::Closed | WState::Errored) {
        return js::resolved(ctx, js::UNDEFINED);
    }
    if let Some(p) = &stream(ctx, s)?.pending_abort {
        return Ok(p.promise.promise.clone_ref());
    }
    let was_already_erroring = state == WState::Erroring;
    let reason = if was_already_erroring {
        V::undefined()
    } else {
        V::dup(ctx, reason)
    };
    let promise = Deferred::new(ctx)?;
    let result = promise.promise.clone_ref();
    stream(ctx, s)?.pending_abort = Some(PendingAbort {
        promise,
        reason: reason.clone_ref(),
        was_already_erroring,
    });
    if !was_already_erroring {
        start_erroring(ctx, s, reason.raw())?;
    }
    Ok(result)
}

/// WritableStreamClose.
fn close_stream(ctx: Ctx, s: Raw) -> R<V> {
    let state = state_of(ctx, s)?;
    if matches!(state, WState::Closed | WState::Errored) {
        let e = js::type_error_value(ctx, "the stream is closed or errored");
        return js::rejected(ctx, e.raw());
    }
    let promise = Deferred::new(ctx)?;
    let result = promise.promise.clone_ref();
    let (w, backpressure, c) = {
        let mut st = stream(ctx, s)?;
        st.close_request = Some(promise);
        (
            st.writer.clone_ref(),
            st.backpressure,
            st.controller.clone_ref(),
        )
    };
    if !w.is_undefined() && backpressure && state == WState::Writable {
        let resolve = writer(ctx, w.raw())?.ready.resolve.clone_ref();
        js::call(ctx, resolve.raw(), js::UNDEFINED, &[js::UNDEFINED])?;
    }
    controller_close(ctx, c.raw())?;
    Ok(result)
}

/// WritableStreamDealWithRejection.
fn deal_with_rejection(ctx: Ctx, s: Raw, e: Raw) -> R<()> {
    if state_of(ctx, s)? == WState::Writable {
        return start_erroring(ctx, s, e);
    }
    finish_erroring(ctx, s)
}

/// WritableStreamStartErroring.
fn start_erroring(ctx: Ctx, s: Raw, reason: Raw) -> R<()> {
    let (c, w) = {
        let mut st = stream(ctx, s)?;
        st.state = WState::Erroring;
        st.stored_error = V::dup(ctx, reason);
        (st.controller.clone_ref(), st.writer.clone_ref())
    };
    if !w.is_undefined() {
        ensure_ready_rejected(ctx, w.raw(), reason)?;
    }
    if !has_operation_in_flight(ctx, s)? && controller(ctx, c.raw())?.started {
        finish_erroring(ctx, s)?;
    }
    Ok(())
}

/// WritableStreamFinishErroring.
fn finish_erroring(ctx: Ctx, s: Raw) -> R<()> {
    let (c, err, requests, abort) = {
        let mut st = stream(ctx, s)?;
        st.state = WState::Errored;
        (
            st.controller.clone_ref(),
            st.stored_error.clone_ref(),
            std::mem::take(&mut st.write_requests),
            st.pending_abort.take(),
        )
    };
    // The controller's error steps.
    let old = controller(ctx, c.raw())?.queue.reset();
    drop(old);
    for r in requests {
        r.reject(ctx, err.raw());
    }
    let Some(abort) = abort else {
        return reject_close_and_closed_if_needed(ctx, s);
    };
    if abort.was_already_erroring {
        abort.promise.reject(ctx, err.raw());
        return reject_close_and_closed_if_needed(ctx, s);
    }
    let abort_algo = controller(ctx, c.raw())?.abort.clone_ref();
    let p = abort_algo.promise(ctx, &[abort.reason.raw()])?;
    clear_algorithms(ctx, c.raw())?;
    js::upon(
        ctx,
        p.raw(),
        vec![
            V::dup(ctx, s),
            abort.promise.resolve.clone_ref(),
            abort.promise.reject.clone_ref(),
        ],
        |ctx, cap, _| {
            js::call(ctx, cap[1].raw(), js::UNDEFINED, &[js::UNDEFINED])?;
            reject_close_and_closed_if_needed(ctx, cap[0].raw())
        },
        |ctx, cap, e| {
            js::call(ctx, cap[2].raw(), js::UNDEFINED, &[e])?;
            reject_close_and_closed_if_needed(ctx, cap[0].raw())
        },
    )
}

fn finish_in_flight_write(ctx: Ctx, s: Raw) -> R<()> {
    let d = stream(ctx, s)?.in_flight_write.take();
    if let Some(d) = d {
        d.resolve(ctx, js::UNDEFINED);
    }
    Ok(())
}

fn finish_in_flight_write_with_error(ctx: Ctx, s: Raw, e: Raw) -> R<()> {
    let d = stream(ctx, s)?.in_flight_write.take();
    if let Some(d) = d {
        d.reject(ctx, e);
    }
    deal_with_rejection(ctx, s, e)
}

fn finish_in_flight_close(ctx: Ctx, s: Raw) -> R<()> {
    let (close, abort, w) = {
        let mut st = stream(ctx, s)?;
        let close = st.in_flight_close.take();
        let abort = if st.state == WState::Erroring {
            st.stored_error = V::undefined();
            st.pending_abort.take()
        } else {
            None
        };
        st.state = WState::Closed;
        (close, abort, st.writer.clone_ref())
    };
    if let Some(d) = close {
        d.resolve(ctx, js::UNDEFINED);
    }
    if let Some(a) = abort {
        a.promise.resolve(ctx, js::UNDEFINED);
    }
    if !w.is_undefined() {
        let resolve = writer(ctx, w.raw())?.closed.resolve.clone_ref();
        js::call(ctx, resolve.raw(), js::UNDEFINED, &[js::UNDEFINED])?;
    }
    Ok(())
}

fn finish_in_flight_close_with_error(ctx: Ctx, s: Raw, e: Raw) -> R<()> {
    let (close, abort) = {
        let mut st = stream(ctx, s)?;
        (st.in_flight_close.take(), st.pending_abort.take())
    };
    if let Some(d) = close {
        d.reject(ctx, e);
    }
    if let Some(a) = abort {
        a.promise.reject(ctx, e);
    }
    deal_with_rejection(ctx, s, e)
}

/// WritableStreamRejectCloseAndClosedPromiseIfNeeded.
fn reject_close_and_closed_if_needed(ctx: Ctx, s: Raw) -> R<()> {
    let (err, close, w) = {
        let mut st = stream(ctx, s)?;
        (
            st.stored_error.clone_ref(),
            st.close_request.take(),
            st.writer.clone_ref(),
        )
    };
    if let Some(d) = close {
        d.reject(ctx, err.raw());
    }
    if !w.is_undefined() {
        let closed = {
            let wr = writer(ctx, w.raw())?;
            (wr.closed.reject.clone_ref(), wr.closed.promise.clone_ref())
        };
        js::call(ctx, closed.0.raw(), js::UNDEFINED, &[err.raw()])?;
        js::mark_handled(ctx, closed.1.raw());
    }
    Ok(())
}

/// WritableStreamUpdateBackpressure.
fn update_backpressure(ctx: Ctx, s: Raw, backpressure: bool) -> R<()> {
    let (w, current) = {
        let st = stream(ctx, s)?;
        (st.writer.clone_ref(), st.backpressure)
    };
    if !w.is_undefined() && backpressure != current {
        if backpressure {
            let d = Deferred::new(ctx)?;
            let old = std::mem::replace(&mut writer(ctx, w.raw())?.ready, d);
            drop(old);
        } else {
            let resolve = writer(ctx, w.raw())?.ready.resolve.clone_ref();
            js::call(ctx, resolve.raw(), js::UNDEFINED, &[js::UNDEFINED])?;
        }
    }
    stream(ctx, s)?.backpressure = backpressure;
    Ok(())
}

// --- Writer ---------------------------------------------------------------

fn new_writer(ctx: Ctx, new_target: Raw, s: Raw) -> R<V> {
    if !is_stream(ctx, s) {
        return Err(js::type_error(ctx, "not a WritableStream"));
    }
    if is_locked(ctx, s)? {
        return Err(js::type_error(ctx, "the WritableStream is locked"));
    }
    let (state, err, backpressure, queued) = {
        let st = stream(ctx, s)?;
        (
            st.state,
            st.stored_error.clone_ref(),
            st.backpressure,
            st.close_request.is_some() || st.in_flight_close.is_some(),
        )
    };
    let (ready, closed) = match state {
        WState::Writable => {
            let ready = if !queued && backpressure {
                Deferred::new(ctx)?
            } else {
                resolved_deferred(ctx)?
            };
            (ready, Deferred::new(ctx)?)
        }
        WState::Erroring => (rejected_deferred(ctx, err.raw())?, Deferred::new(ctx)?),
        WState::Closed => (resolved_deferred(ctx)?, resolved_deferred(ctx)?),
        WState::Errored => (
            rejected_deferred(ctx, err.raw())?,
            rejected_deferred(ctx, err.raw())?,
        ),
    };
    let w = instance(
        ctx,
        new_target,
        Class::WritableStreamDefaultWriter,
        Writer {
            stream: V::dup(ctx, s),
            ready,
            closed,
        },
    )?;
    stream(ctx, s)?.writer = w.clone_ref();
    Ok(w)
}

/// AcquireWritableStreamDefaultWriter.
pub(crate) fn acquire_writer(ctx: Ctx, s: Raw) -> R<V> {
    new_writer(ctx, js::UNDEFINED, s)
}

pub(crate) fn writer_closed(ctx: Ctx, w: Raw) -> R<V> {
    Ok(writer(ctx, w)?.closed.promise.clone_ref())
}

pub(crate) fn writer_ready(ctx: Ctx, w: Raw) -> R<V> {
    Ok(writer(ctx, w)?.ready.promise.clone_ref())
}

fn ensure_ready_rejected(ctx: Ctx, w: Raw, e: Raw) -> R<()> {
    let pending = is_pending(ctx, &writer(ctx, w)?.ready);
    if pending {
        let (reject, promise) = {
            let wr = writer(ctx, w)?;
            (wr.ready.reject.clone_ref(), wr.ready.promise.clone_ref())
        };
        js::call(ctx, reject.raw(), js::UNDEFINED, &[e])?;
        js::mark_handled(ctx, promise.raw());
    } else {
        let d = rejected_deferred(ctx, e)?;
        let old = std::mem::replace(&mut writer(ctx, w)?.ready, d);
        drop(old);
    }
    Ok(())
}

fn ensure_closed_rejected(ctx: Ctx, w: Raw, e: Raw) -> R<()> {
    let pending = is_pending(ctx, &writer(ctx, w)?.closed);
    if pending {
        let (reject, promise) = {
            let wr = writer(ctx, w)?;
            (wr.closed.reject.clone_ref(), wr.closed.promise.clone_ref())
        };
        js::call(ctx, reject.raw(), js::UNDEFINED, &[e])?;
        js::mark_handled(ctx, promise.raw());
    } else {
        let d = rejected_deferred(ctx, e)?;
        let old = std::mem::replace(&mut writer(ctx, w)?.closed, d);
        drop(old);
    }
    Ok(())
}

/// WritableStreamDefaultWriterRelease.
pub(crate) fn writer_release(ctx: Ctx, w: Raw) -> R<()> {
    let s = writer(ctx, w)?.stream.clone_ref();
    if s.is_undefined() {
        return Ok(());
    }
    let e = js::type_error_value(ctx, "the writer was released");
    ensure_ready_rejected(ctx, w, e.raw())?;
    ensure_closed_rejected(ctx, w, e.raw())?;
    let old = (
        stream(ctx, s.raw())?.writer.take_out(),
        writer(ctx, w)?.stream.take_out(),
    );
    drop(old);
    Ok(())
}

/// WritableStreamDefaultWriterGetDesiredSize.
fn writer_desired_size(ctx: Ctx, s: Raw) -> R<Option<f64>> {
    Ok(match state_of(ctx, s)? {
        WState::Errored | WState::Erroring => None,
        WState::Closed => Some(0.0),
        WState::Writable => {
            let c = controller_of(ctx, s)?;
            Some(controller_desired_size(ctx, c.raw())?)
        }
    })
}

/// WritableStreamDefaultWriterWrite.
pub(crate) fn writer_write(ctx: Ctx, w: Raw, chunk: Raw) -> R<V> {
    let s = writer(ctx, w)?.stream.clone_ref();
    if s.is_undefined() {
        let e = js::type_error_value(ctx, "the writer was released");
        return js::rejected(ctx, e.raw());
    }
    let c = controller_of(ctx, s.raw())?;
    let size = controller_chunk_size(ctx, c.raw(), chunk)?;
    if !js::same(writer(ctx, w)?.stream.raw(), s.raw()) {
        let e = js::type_error_value(ctx, "the writer was released");
        return js::rejected(ctx, e.raw());
    }
    let state = state_of(ctx, s.raw())?;
    if state == WState::Errored {
        let e = stored_error(ctx, s.raw())?;
        return js::rejected(ctx, e.raw());
    }
    if close_queued_or_in_flight(ctx, s.raw())? || state == WState::Closed {
        let e = js::type_error_value(ctx, "the stream is closing or closed");
        return js::rejected(ctx, e.raw());
    }
    if state == WState::Erroring {
        let e = stored_error(ctx, s.raw())?;
        return js::rejected(ctx, e.raw());
    }
    let d = Deferred::new(ctx)?;
    let promise = d.promise.clone_ref();
    stream(ctx, s.raw())?.write_requests.push_back(d);
    controller_write(ctx, c.raw(), chunk, size)?;
    Ok(promise)
}

/// WritableStreamDefaultWriterCloseWithErrorPropagation.
pub(crate) fn writer_close_with_error_propagation(ctx: Ctx, w: Raw) -> R<V> {
    let s = writer(ctx, w)?.stream.clone_ref();
    let state = state_of(ctx, s.raw())?;
    if close_queued_or_in_flight(ctx, s.raw())? || state == WState::Closed {
        return js::resolved(ctx, js::UNDEFINED);
    }
    if state == WState::Errored {
        let e = stored_error(ctx, s.raw())?;
        return js::rejected(ctx, e.raw());
    }
    close_stream(ctx, s.raw())
}

fn writer_stream(ctx: Ctx, w: Raw) -> R<V> {
    let s = writer(ctx, w)?.stream.clone_ref();
    if s.is_undefined() {
        return Err(js::type_error(ctx, "the writer was released"));
    }
    Ok(s)
}

native!(
    fn writer_constructor(ctx, this, args) {
        new_writer(ctx, this, args.get(0))
    }
);

native!(
    fn writer_closed_get(ctx, this, args) {
        writer_closed(ctx, this)
    }
);

native!(
    fn writer_ready_get(ctx, this, args) {
        writer_ready(ctx, this)
    }
);

native!(
    fn writer_desired_size_get(ctx, this, args) {
        let s = writer_stream(ctx, this)?;
        Ok(match writer_desired_size(ctx, s.raw())? {
            Some(n) => V::num(n),
            None => V::null(),
        })
    }
);

native!(
    fn writer_abort(ctx, this, args) {
        js::promise_of(ctx, || {
            let s = writer_stream(ctx, this)?;
            abort_stream(ctx, s.raw(), args.get(0))
        })
    }
);

native!(
    fn writer_close(ctx, this, args) {
        js::promise_of(ctx, || {
            let s = writer_stream(ctx, this)?;
            if close_queued_or_in_flight(ctx, s.raw())? {
                return Err(js::type_error(ctx, "the stream is already closing"));
            }
            close_stream(ctx, s.raw())
        })
    }
);

native!(
    fn writer_release_lock(ctx, this, args) {
        writer_release(ctx, this)?;
        Ok(V::undefined())
    }
);

native!(
    fn writer_write_method(ctx, this, args) {
        js::promise_of(ctx, || {
            writer_stream(ctx, this)?;
            writer_write(ctx, this, args.get(0))
        })
    }
);

// --- Controller -----------------------------------------------------------

fn clear_algorithms(ctx: Ctx, c: Raw) -> R<()> {
    let old = {
        let mut ct = controller(ctx, c)?;
        (
            std::mem::take(&mut ct.write),
            std::mem::take(&mut ct.close),
            std::mem::take(&mut ct.abort),
            ct.size.take_out(),
        )
    };
    drop(old);
    Ok(())
}

fn controller_desired_size(ctx: Ctx, c: Raw) -> R<f64> {
    let ct = controller(ctx, c)?;
    Ok(ct.hwm - ct.queue.total)
}

fn controller_chunk_size(ctx: Ctx, c: Raw, chunk: Raw) -> R<f64> {
    let size = controller(ctx, c)?.size.clone_ref();
    match chunk_size(ctx, &size, chunk) {
        Ok(n) => Ok(n),
        Err(_) => {
            let e = js::take_exception(ctx);
            error_if_needed(ctx, c, e.raw())?;
            Ok(1.0)
        }
    }
}

/// WritableStreamDefaultControllerErrorIfNeeded.
pub(crate) fn error_if_needed(ctx: Ctx, c: Raw, e: Raw) -> R<()> {
    let s = controller(ctx, c)?.stream.clone_ref();
    if state_of(ctx, s.raw())? == WState::Writable {
        controller_error(ctx, c, e)?;
    }
    Ok(())
}

fn controller_error(ctx: Ctx, c: Raw, e: Raw) -> R<()> {
    let s = controller(ctx, c)?.stream.clone_ref();
    clear_algorithms(ctx, c)?;
    start_erroring(ctx, s.raw(), e)
}

fn controller_close(ctx: Ctx, c: Raw) -> R<()> {
    let sentinel = instance(ctx, js::UNDEFINED, Class::Internal, CloseSentinel)?;
    controller(ctx, c)?.queue.enqueue(ctx, sentinel, 0.0)?;
    advance_queue_if_needed(ctx, c)
}

fn controller_write(ctx: Ctx, c: Raw, chunk: Raw, size: f64) -> R<()> {
    let r = controller(ctx, c)?
        .queue
        .enqueue(ctx, V::dup(ctx, chunk), size);
    if r.is_err() {
        let e = js::take_exception(ctx);
        return error_if_needed(ctx, c, e.raw());
    }
    let s = controller(ctx, c)?.stream.clone_ref();
    if !close_queued_or_in_flight(ctx, s.raw())? && state_of(ctx, s.raw())? == WState::Writable {
        let bp = controller_desired_size(ctx, c)? <= 0.0;
        update_backpressure(ctx, s.raw(), bp)?;
    }
    advance_queue_if_needed(ctx, c)
}

/// WritableStreamDefaultControllerAdvanceQueueIfNeeded.
fn advance_queue_if_needed(ctx: Ctx, c: Raw) -> R<()> {
    let (s, started, next) = {
        let ct = controller(ctx, c)?;
        (
            ct.stream.clone_ref(),
            ct.started,
            ct.queue.peek().map(V::clone_ref),
        )
    };
    if !started || stream(ctx, s.raw())?.in_flight_write.is_some() {
        return Ok(());
    }
    let state = state_of(ctx, s.raw())?;
    if matches!(state, WState::Closed | WState::Errored) {
        return Ok(());
    }
    if state == WState::Erroring {
        return finish_erroring(ctx, s.raw());
    }
    let Some(next) = next else { return Ok(()) };
    if is::<CloseSentinel>(ctx, next.raw()) {
        process_close(ctx, c)
    } else {
        process_write(ctx, c, next.raw())
    }
}

fn process_close(ctx: Ctx, c: Raw) -> R<()> {
    let s = controller(ctx, c)?.stream.clone_ref();
    {
        let mut st = stream(ctx, s.raw())?;
        st.in_flight_close = st.close_request.take();
    }
    let (old, close) = {
        let mut ct = controller(ctx, c)?;
        (ct.queue.dequeue(), ct.close.clone_ref())
    };
    drop(old);
    let p = close.promise(ctx, &[])?;
    clear_algorithms(ctx, c)?;
    js::upon(
        ctx,
        p.raw(),
        vec![s],
        |ctx, cap, _| finish_in_flight_close(ctx, cap[0].raw()),
        |ctx, cap, e| finish_in_flight_close_with_error(ctx, cap[0].raw(), e),
    )
}

fn process_write(ctx: Ctx, c: Raw, chunk: Raw) -> R<()> {
    let s = controller(ctx, c)?.stream.clone_ref();
    {
        let mut st = stream(ctx, s.raw())?;
        st.in_flight_write = st.write_requests.pop_front();
    }
    let write = controller(ctx, c)?.write.clone_ref();
    let p = write.promise(ctx, &[chunk, c])?;
    js::upon(
        ctx,
        p.raw(),
        vec![s, V::dup(ctx, c)],
        |ctx, cap, _| {
            let (s, c) = (cap[0].raw(), cap[1].raw());
            finish_in_flight_write(ctx, s)?;
            let old = controller(ctx, c)?.queue.dequeue();
            drop(old);
            if !close_queued_or_in_flight(ctx, s)? && state_of(ctx, s)? == WState::Writable {
                let bp = controller_desired_size(ctx, c)? <= 0.0;
                update_backpressure(ctx, s, bp)?;
            }
            advance_queue_if_needed(ctx, c)
        },
        |ctx, cap, e| {
            let (s, c) = (cap[0].raw(), cap[1].raw());
            if state_of(ctx, s)? == WState::Writable {
                clear_algorithms(ctx, c)?;
            }
            finish_in_flight_write_with_error(ctx, s, e)
        },
    )
}

/// The algorithms of an underlying sink.
pub(crate) struct Sink {
    pub(crate) start: Algo,
    pub(crate) write: Algo,
    pub(crate) close: Algo,
    pub(crate) abort: Algo,
}

/// SetUpWritableStreamDefaultController.
fn setup_controller(ctx: Ctx, s: Raw, sink: Sink, hwm: f64, size: V) -> R<V> {
    let Sink {
        start,
        write,
        close,
        abort,
    } = sink;
    let g = js::global(ctx);
    let ac = js::get(ctx, g.raw(), "AbortController")?;
    let signal_controller = js::construct(ctx, ac.raw(), &[])?;
    let c = instance(
        ctx,
        js::UNDEFINED,
        Class::WritableStreamDefaultController,
        Controller {
            stream: V::dup(ctx, s),
            queue: SizedQueue::default(),
            signal_controller,
            started: false,
            hwm,
            size,
            write,
            close,
            abort,
        },
    )?;
    stream(ctx, s)?.controller = c.clone_ref();
    let bp = controller_desired_size(ctx, c.raw())? <= 0.0;
    update_backpressure(ctx, s, bp)?;
    let start_result = start.call(ctx, &[c.raw()])?;
    let p = js::resolved(ctx, start_result.raw())?;
    js::upon(
        ctx,
        p.raw(),
        vec![c.clone_ref()],
        |ctx, cap, _| {
            controller(ctx, cap[0].raw())?.started = true;
            advance_queue_if_needed(ctx, cap[0].raw())
        },
        |ctx, cap, e| {
            let s = {
                let mut ct = controller(ctx, cap[0].raw())?;
                ct.started = true;
                ct.stream.clone_ref()
            };
            deal_with_rejection(ctx, s.raw(), e)
        },
    )?;
    Ok(c)
}

/// CreateWritableStream with native algorithms: the stream and its
/// controller.
pub(crate) fn new_writable_stream(ctx: Ctx, sink: Sink, hwm: f64, size: V) -> R<(V, V)> {
    let s = new_stream(ctx, js::UNDEFINED)?;
    let c = setup_controller(ctx, s.raw(), sink, hwm, size)?;
    Ok((s, c))
}

native!(
    fn controller_signal(ctx, this, args) {
        let ac = controller(ctx, this)?.signal_controller.clone_ref();
        js::get(ctx, ac.raw(), "signal")
    }
);

native!(
    fn controller_error_method(ctx, this, args) {
        let s = controller(ctx, this)?.stream.clone_ref();
        if state_of(ctx, s.raw())? != WState::Writable {
            return Ok(V::undefined());
        }
        controller_error(ctx, this, args.get(0))?;
        Ok(V::undefined())
    }
);

// --- WritableStream -------------------------------------------------------

native!(
    fn stream_constructor(ctx, this, args) {
        let sink = args.get(0);
        if !js::is_nullish(sink) && !js::is_object(sink) {
            return Err(js::type_error(ctx, "the underlying sink must be an object"));
        }
        let strategy = QueuingStrategy::read(ctx, args.get(1))?;
        let sink_v = V::dup(ctx, sink);
        let abort = method(ctx, sink, "abort")?;
        let close = method(ctx, sink, "close")?;
        let start = method(ctx, sink, "start")?;
        if js::is_object(sink) {
            let t = js::get(ctx, sink, "type")?;
            if !t.is_undefined() {
                return Err(js::range_error(ctx, "an underlying sink has no type"));
            }
        }
        let write = method(ctx, sink, "write")?;
        let hwm = strategy.high_water_mark(ctx, 1.0)?;
        let s = new_stream(ctx, this)?;
        let sink = Sink {
            start: Algo::method(start, &sink_v),
            write: Algo::method(write, &sink_v),
            close: Algo::method(close, &sink_v),
            abort: Algo::method(abort, &sink_v),
        };
        setup_controller(ctx, s.raw(), sink, hwm, strategy.size)?;
        Ok(s)
    }
);

native!(
    fn stream_locked(ctx, this, args) {
        Ok(V::bool(is_locked(ctx, this)?))
    }
);

native!(
    fn stream_abort(ctx, this, args) {
        js::promise_of(ctx, || {
            if is_locked(ctx, this)? {
                return Err(js::type_error(ctx, "the WritableStream is locked"));
            }
            abort_stream(ctx, this, args.get(0))
        })
    }
);

native!(
    fn stream_close(ctx, this, args) {
        js::promise_of(ctx, || {
            if is_locked(ctx, this)? {
                return Err(js::type_error(ctx, "the WritableStream is locked"));
            }
            if close_queued_or_in_flight(ctx, this)? {
                return Err(js::type_error(ctx, "the stream is already closing"));
            }
            close_stream(ctx, this)
        })
    }
);

native!(
    fn stream_get_writer(ctx, this, args) {
        acquire_writer(ctx, this)
    }
);

pub(crate) const SPECS: &[Spec] = &[
    Spec {
        class: Class::WritableStream,
        name: c"WritableStream",
        parent: None,
        constructor: Some((stream_constructor, 0)),
        methods: &[
            (Key::Name(c"abort"), stream_abort, 0),
            (Key::Name(c"close"), stream_close, 0),
            (Key::Name(c"getWriter"), stream_get_writer, 0),
        ],
        accessors: &[(c"locked", stream_locked, None)],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "<W = any>",
            members: r#"constructor(underlyingSink?: UnderlyingSink<W>, strategy?: QueuingStrategy<W>);
readonly locked: boolean;
abort(reason?: any): Promise<void>;
close(): Promise<void>;
getWriter(): WritableStreamDefaultWriter<W>;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::WritableStreamDefaultWriter,
        name: c"WritableStreamDefaultWriter",
        parent: None,
        constructor: Some((writer_constructor, 1)),
        methods: &[
            (Key::Name(c"abort"), writer_abort, 0),
            (Key::Name(c"close"), writer_close, 0),
            (Key::Name(c"releaseLock"), writer_release_lock, 0),
            (Key::Name(c"write"), writer_write_method, 0),
        ],
        accessors: &[
            (c"closed", writer_closed_get, None),
            (c"desiredSize", writer_desired_size_get, None),
            (c"ready", writer_ready_get, None),
        ],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "<W = any>",
            members: r#"constructor(stream: WritableStream<W>);
readonly closed: Promise<void>;
readonly desiredSize: number | null;
readonly ready: Promise<void>;
abort(reason?: any): Promise<void>;
close(): Promise<void>;
releaseLock(): void;
write(chunk?: W): Promise<void>;"#,
        }
        .declared(),
    },
    Spec {
        class: Class::WritableStreamDefaultController,
        name: c"WritableStreamDefaultController",
        parent: None,
        constructor: None,
        methods: &[(Key::Name(c"error"), controller_error_method, 0)],
        accessors: &[(c"signal", controller_signal, None)],
        statics: &[],
        constants: &[],
        global: true,
        ts: crate::web::ClassTs {
            doc: "",
            head: "",
            members: r#"readonly signal: AbortSignal;
error(e?: any): void;"#,
        }
        .declared(),
    },
];

/// The TypeScript types of this file's classes (`globals.d.ts`).
pub(crate) const TYPES: &str = r#"interface UnderlyingSink<W = any> {
  start?(controller: WritableStreamDefaultController): any;
  write?(chunk: W, controller: WritableStreamDefaultController): void | PromiseLike<void>;
  close?(): void | PromiseLike<void>;
  abort?(reason?: any): void | PromiseLike<void>;
  type?: undefined;
}
"#;
