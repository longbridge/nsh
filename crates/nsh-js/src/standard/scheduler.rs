//! Engine-thread bridge for LLRT futures. I/O wakes the engine; idle VMs never poll.
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context as TaskContext, Poll, Wake, Waker},
};

use rquickjs::{AsyncContext, AsyncRuntime, Ctx};

type WakeCallback = Arc<dyn Fn() + Send + Sync>;
#[derive(Default)]
struct EngineWake(Mutex<Option<WakeCallback>>);
impl Wake for EngineWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        // Do not hold the mutex while invoking a host callback.
        let callback = self.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(callback) = callback {
            callback();
        }
    }
}

pub(crate) struct Scheduler {
    wake: Arc<EngineWake>,
    #[cfg(not(target_arch = "wasm32"))]
    io: Option<tokio::runtime::Runtime>,
}
impl Scheduler {
    pub(crate) fn new() -> std::io::Result<Rc<Self>> {
        Ok(Rc::new(Self {
            wake: Arc::new(EngineWake::default()),
            #[cfg(not(target_arch = "wasm32"))]
            io: Some(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .thread_name("nsh-js-io")
                    .enable_all()
                    .build()?,
            ),
        }))
    }
    pub(crate) fn set_wake(&self, callback: WakeCallback) {
        *self.wake.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(callback);
    }
    pub(crate) fn disable(&self) {
        *self.wake.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    pub(crate) fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        #[cfg(not(target_arch = "wasm32"))]
        let _guard = self.io.as_ref().expect("live scheduler").enter();
        f()
    }
    /// Runtime configuration and context closures never await user code. A pending
    /// lock means a host bug (nested entry), not work that can be blocked on.
    pub(crate) fn ready<F: Future>(&self, future: F) -> F::Output {
        self.enter(
            || match pin!(future).poll(&mut TaskContext::from_waker(&self.waker())) {
                Poll::Ready(value) => value,
                Poll::Pending => panic!("nested asynchronous JS runtime entry"),
            },
        )
    }
    fn waker(&self) -> Waker {
        Waker::from(self.wake.clone())
    }
    pub(crate) fn poll_job(&self, runtime: &AsyncRuntime) -> Poll<Result<bool, AsyncContext>> {
        self.enter(|| {
            pin!(runtime.execute_pending_job())
                .poll(&mut TaskContext::from_waker(&self.waker()))
                .map(|result| result.map_err(|err| err.0))
        })
    }
}
impl Drop for Scheduler {
    fn drop(&mut self) {
        self.disable();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(io) = self.io.take() {
            io.shutdown_background();
        }
    }
}

/// Synchronous NSH entry surface backed by the async runtime's local context.
/// The JS values and their futures remain on the owning engine thread.
pub(crate) struct Context {
    inner: RefCell<Option<AsyncContext>>,
    scheduler: Rc<Scheduler>,
}
impl Context {
    #[cfg(not(feature = "bytecode-only"))]
    pub(crate) fn full(runtime: &AsyncRuntime, scheduler: Rc<Scheduler>) -> rquickjs::Result<Self> {
        let inner = scheduler.ready(AsyncContext::full(runtime))?;
        Ok(Self {
            inner: RefCell::new(Some(inner)),
            scheduler,
        })
    }
    #[cfg(any(feature = "bytecode-only", test))]
    pub(crate) fn custom<I: rquickjs::context::Intrinsic>(
        runtime: &AsyncRuntime,
        scheduler: Rc<Scheduler>,
    ) -> rquickjs::Result<Self> {
        let inner = scheduler.ready(AsyncContext::custom::<I>(runtime))?;
        Ok(Self {
            inner: RefCell::new(Some(inner)),
            scheduler,
        })
    }
    pub(crate) fn is_live(&self) -> bool {
        self.inner.borrow().is_some()
    }
    pub(crate) fn retire(&self) {
        self.inner.borrow_mut().take();
    }
    pub(crate) fn with<F, R>(&self, f: F) -> R
    where
        F: for<'js> FnOnce(Ctx<'js>) -> R,
    {
        let inner = self.inner.borrow();
        self.scheduler
            .ready(inner.as_ref().expect("live JS context").with(f))
    }
}
