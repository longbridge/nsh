//! The in-app DevTools endpoint of development hosts (`nsh-devtools`): a
//! `Handler` over a [`Tester`] of its own, so a client of the endpoint drives
//! the app the way `nsh test` does, and the pieces the rest of the host feeds
//! it (the console collector, the app's errors and panics).
//!
//! Two hosts serve it: development hosts (`dev.rs`, the app comes from the dev
//! server) and a packaged app's runner built with the feature `devtools`
//! ([`load_with_endpoint`]: the endpoint is on when `NSH_DEVTOOLS` is set, and
//! `NSH_DEVTOOLS_WAIT` holds the app's load, so the app's code runs only
//! after `Session.start`).
//!
//! Order matters at the start: `Session.setup` runs [`Tester::start_session`]
//! (which posts the test model to the main queue) and then
//! [`Tester::set_up`]; only after both returned does the endpoint call
//! [`Handler::start`], which posts the gate's opening to the same queue. The
//! queue is first in, first out, so the model is on the surface before the
//! held app loads.

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak as RcWeak},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use dispatch2::DispatchQueue;
use nsh_dev::protocol::{FetchReply, StepOutcome, TestStep, WireRequest};
use nsh_devtools::{
    console::{Collector, DEFAULT_CAPACITY},
    protocol::ErrorBody,
    server::{Endpoint, EndpointConfig, Handler, SessionSetup},
};
use nsh_engine::App;
use nsh_testing::agent::{Link, Native, Tester};
use serde_json::{Value, json};

use crate::app::AppView;

/// The process's console collector: the logger records into it, the endpoint
/// serves it. One per process.
pub(crate) static COLLECTOR: OnceLock<Arc<Collector>> = OnceLock::new();

/// The collector, made on first use.
pub(crate) fn collector() -> &'static Arc<Collector> {
    COLLECTOR.get_or_init(|| Arc::new(Collector::new(DEFAULT_CAPACITY)))
}

/// Records `record` into the console collector, which the endpoint serves:
/// the logger of a runner built with the feature `devtools` calls it for
/// every line. Development hosts' logger does it itself.
pub(crate) fn record(record: &log::Record<'_>) {
    collector().record(record);
}

/// What starts a test session on the surface `key`: the model onto the
/// surface (through the main queue), the platform's input. The dev client's
/// tester and the DevTools endpoint's each run it for their own session.
pub(crate) fn session_starter(
    key: u64,
) -> impl Fn(Arc<nsh_testing::agent::Model>) -> Box<dyn Native> + Send + Sync + Copy + 'static {
    move |model| {
        DispatchQueue::main().exec_async(move || {
            if let Some(surface) = crate::surface::Surface::with_key(key) {
                surface.set_test_model(Some(model));
                crate::system::testing_started(&surface);
            }
        });
        Box::new(crate::agent::MacNative::new(key)) as Box<dyn Native>
    }
}

/// What holds an app back until a DevTools client starts it: while closed,
/// the newest app that came is kept instead of loaded.
#[derive(Debug)]
pub(crate) struct Gate<T> {
    closed: bool,
    held: Option<T>,
}

impl<T> Default for Gate<T> {
    /// An open gate.
    fn default() -> Self {
        Gate::new(false)
    }
}

impl<T> Gate<T> {
    /// A gate, closed or open.
    pub(crate) fn new(closed: bool) -> Self {
        Gate { closed, held: None }
    }

    /// Whether apps are held.
    #[cfg(feature = "dev")]
    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }

    /// Keeps `app` (replacing an older one) when the gate is closed and says
    /// so; an open gate gives `false` and the caller loads it.
    pub(crate) fn hold(&mut self, app: T) -> bool {
        if self.closed {
            self.held = Some(app);
        }
        self.closed
    }

    /// Opens the gate; the held app, once.
    pub(crate) fn open(&mut self) -> Option<T> {
        self.closed = false;
        self.held.take()
    }
}

/// The link of the running session, for the events the host raises outside
/// the tester's path (errors, panics).
static ACTIVE: Mutex<Weak<EndpointLink>> = Mutex::new(Weak::new());

/// Sends event `Console.error` to the clients of the running endpoint, if
/// there is one.
pub(crate) fn console_error(message: &str, stack: Option<&str>) {
    let link = ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).upgrade();
    if let Some(link) = link {
        link.event(
            "Console.error",
            json!({ "message": message, "stack": stack }),
        );
    }
}

/// Chains a panic hook that records the panic at level `Error`, target
/// `panic`, and sends it; the stack goes along, formatted on a thread of its
/// own, when `RUST_BACKTRACE` is set. Installed once.
fn install_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            previous(info);
            let message = info.to_string();
            // Never resolved or formatted here: that takes long enough in a
            // debug build to stall the panicking thread (an engine thread
            // that catches the panic). `capture` costs nothing unless
            // RUST_BACKTRACE or RUST_LIB_BACKTRACE asks for it.
            let backtrace = std::backtrace::Backtrace::capture();
            collector().record(
                &log::Record::builder()
                    .args(format_args!("{message}"))
                    .level(log::Level::Error)
                    .target("panic")
                    .build(),
            );
            if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
                let sent = std::thread::Builder::new()
                    .name("nsh-devtools-panic".into())
                    .spawn({
                        let message = message.clone();
                        move || console_error(&message, Some(&backtrace.to_string()))
                    });
                if sent.is_ok() {
                    return;
                }
            }
            console_error(&message, None);
        }));
    });
}

/// Where the link's events go: the endpoint's clients.
type Sink = Box<dyn Fn(&str, Value) + Send + Sync>;

/// A step's answer, called once.
type Done = Box<dyn FnOnce(Result<Value, String>) + Send>;

/// The tester's way back to the endpoint's clients.
struct EndpointLink {
    pending: Mutex<HashMap<u64, Done>>,
    /// Set once the endpoint runs (a weak handle: the endpoint owns the
    /// handler which owns this link).
    sink: OnceLock<Sink>,
}

impl EndpointLink {
    fn new() -> Arc<EndpointLink> {
        Arc::new(EndpointLink {
            pending: Mutex::new(HashMap::new()),
            sink: OnceLock::new(),
        })
    }

    fn event(&self, method: &str, params: Value) {
        if let Some(sink) = self.sink.get() {
            sink(method, params);
        }
    }
}

impl Link for EndpointLink {
    fn step_done(&self, id: u64, outcome: StepOutcome) {
        let done = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        if let Some(done) = done {
            done(match outcome {
                StepOutcome::Done { value } => Ok(value),
                StepOutcome::Failed { message } => Err(message),
            });
        }
    }

    fn fetch(&self, id: u64, request: WireRequest) {
        self.event(
            "Network.request",
            json!({ "id": id, "request": serde_json::to_value(request).unwrap_or(Value::Null) }),
        );
    }

    fn fetch_cancel(&self, id: u64) {
        self.event("Network.cancel", json!({ "id": id }));
    }

    fn trace(&self, items: Vec<String>) {
        self.event("Trace.batch", json!({ "items": items }));
    }
}

/// The endpoint's handler: steps and setup go to the tester.
struct DevtoolsHandler {
    tester: Arc<Tester>,
    link: Arc<EndpointLink>,
    next_step: AtomicU64,
    /// Lets the held app load (posts to the main queue).
    open_gate: Box<dyn Fn() + Send + Sync>,
}

impl Handler for DevtoolsHandler {
    fn setup(&self, setup: SessionSetup) -> Result<(), ErrorBody> {
        // Starts the session first: the model reaches the surface (through
        // the main queue) before the app can load.
        self.tester.start_session();
        self.tester
            .set_up(setup.storage, setup.trace, setup.environment);
        Ok(())
    }

    fn start(&self) {
        (self.open_gate)();
    }

    fn step(&self, step: TestStep, done: Done) {
        let id = self.next_step.fetch_add(1, Ordering::Relaxed);
        self.link
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, done);
        self.tester.take(id, step);
    }

    fn fetch_reply(&self, id: u64, reply: FetchReply) {
        self.tester.fetch_reply(id, reply);
    }
}

/// A host's running endpoint; dropping it stops it.
pub(crate) struct DevtoolsSession {
    _endpoint: Arc<Endpoint>,
    _tester: Arc<Tester>,
    _link: Arc<EndpointLink>,
}

impl DevtoolsSession {
    /// Starts the endpoint. `start_native` builds the test session's model
    /// attachment and platform part (as the dev client's tester's does);
    /// `open_gate` lets the held app load, and runs on the endpoint's thread.
    ///
    /// # Errors
    ///
    /// The endpoint cannot listen.
    pub(crate) fn start(
        mut config: EndpointConfig,
        start_native: impl Fn(Arc<nsh_testing::agent::Model>) -> Box<dyn Native> + Send + Sync + 'static,
        open_gate: impl Fn() + Send + Sync + 'static,
    ) -> std::io::Result<DevtoolsSession> {
        config.platform = "macos".to_owned();
        let tester = Tester::new(start_native);
        let link = EndpointLink::new();
        // Before anything can start a session: the link is snapshotted then.
        let as_link: Arc<dyn Link> = link.clone();
        tester.set_link(Arc::downgrade(&as_link));
        let handler = Arc::new(DevtoolsHandler {
            tester: tester.clone(),
            link: link.clone(),
            next_step: AtomicU64::new(1),
            open_gate: Box::new(open_gate),
        });
        install_panic_hook();
        let endpoint = Arc::new(Endpoint::start(config, handler, collector().clone())?);
        let weak = Arc::downgrade(&endpoint);
        let _ = link.sink.set(Box::new(move |method, params| {
            if let Some(endpoint) = weak.upgrade() {
                endpoint.event(method, params);
            }
        }));
        *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = Arc::downgrade(&link);
        Ok(DevtoolsSession {
            _endpoint: endpoint,
            _tester: tester,
            _link: link,
        })
    }
}

/// A packaged app's endpoint: the gate that holds its app, the view it loads
/// into and the running session.
struct Runner {
    gate: Gate<App>,
    view: RcWeak<AppView>,
    session: Option<DevtoolsSession>,
}

thread_local! {
    /// The runner's endpoint of this (the main) thread.
    static RUNNER: RefCell<Option<Runner>> = const { RefCell::new(None) };
}

/// Loads `app` into `view`: at once, or, when `NSH_DEVTOOLS` asks for an
/// endpoint, behind it. The endpoint then serves the app's DevTools and, with
/// `NSH_DEVTOOLS_WAIT`, the app is kept until a client sends `Session.start`
/// (after the test session's model is on the surface).
pub(crate) fn load_with_endpoint(view: &Rc<AppView>, app: App) {
    let Some(config) = EndpointConfig::from_env() else {
        return view.load(app);
    };
    let key = view.surface().key();
    view.surface().set_error_handler(Box::new(|message| {
        console_error(message, None);
    }));
    RUNNER.with(|r| {
        *r.borrow_mut() = Some(Runner {
            gate: Gate::new(config.wait),
            view: Rc::downgrade(view),
            session: None,
        });
    });
    // Posted behind `Session.setup`'s model (same queue, first in, first
    // out): the model is on the surface before the held app loads.
    let open = || DispatchQueue::main().exec_async(open_runner_gate);
    match DevtoolsSession::start(config, session_starter(key), open) {
        Ok(session) => RUNNER.with(|r| {
            if let Some(runner) = r.borrow_mut().as_mut() {
                runner.session = Some(session);
            }
        }),
        Err(err) => {
            log::error!("cannot start the DevTools endpoint: {err}");
            RUNNER.with(|r| {
                if let Some(runner) = r.borrow_mut().as_mut() {
                    runner.gate.open();
                }
            });
        }
    }
    let held = RUNNER.with(|r| {
        r.borrow_mut()
            .as_mut()
            .is_some_and(|runner| runner.gate.hold(app.clone()))
    });
    if !held {
        view.load(app);
    }
}

/// Loads the held app (`Session.start`).
fn open_runner_gate() {
    let opened = RUNNER.with(|r| {
        let mut r = r.borrow_mut();
        let runner = r.as_mut()?;
        Some((runner.gate.open()?, runner.view.upgrade()?))
    });
    if let Some((app, view)) = opened {
        view.load(app);
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use nsh_core::{
        NodeId,
        geometry::{Bounds, Point, Points, Size},
    };
    use nsh_testing::agent::Key;

    use super::*;

    /// A platform part that does nothing.
    struct Stub;

    impl Native for Stub {
        fn bounds(&self, _: NodeId) -> Option<Bounds<Points>> {
            None
        }
        fn viewport(&self) -> Size<Points> {
            Size::default()
        }
        fn press(&self, _: Point<Points>) {}
        fn long_press(&self, _: Point<Points>, _: Duration) {}
        fn scroll(&self, _: Point<Points>, _: Point<Points>) {}
        fn drag(&self, _: Point<Points>, _: Point<Points>) {}
        fn type_text(&self, _: &str) {}
        fn key(&self, _: Key) {}
        fn input_text(&self, _: NodeId) -> Option<String> {
            None
        }
        fn back(&self) -> Result<(), String> {
            Ok(())
        }
        fn answer_dialog(&self, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn animating(&self) -> bool {
            false
        }
        fn screenshot(&self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn resize(&self, _: Points, _: Points) -> Result<(), String> {
            Ok(())
        }
        fn set_dark(&self, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn quit(&self) {}
    }

    #[test]
    fn gate_keeps_the_newest_app_and_opens_once() {
        let mut gate = Gate::new(true);
        assert!(gate.hold(1));
        assert!(gate.hold(2));
        assert_eq!(gate.open(), Some(2));
        assert_eq!(gate.open(), None);
        assert!(!gate.hold(3));
        assert_eq!(gate.open(), None);
        assert!(!Gate::<u8>::default().hold(1));
    }

    #[test]
    fn handler_sets_up_before_the_engine_starts() {
        let calls: Arc<Mutex<Vec<&'static str>>> = Arc::default();
        let model: Arc<Mutex<Option<Arc<nsh_testing::agent::Model>>>> = Arc::default();
        let tester = {
            let (calls, model) = (calls.clone(), model.clone());
            Tester::new(move |m| {
                calls.lock().unwrap().push("start_session");
                *model.lock().unwrap() = Some(m);
                Box::new(Stub)
            })
        };
        let link = EndpointLink::new();
        // Setting up with `trace` makes the model report each batch it
        // applies: the first one proves `set_up` ran.
        {
            let calls = calls.clone();
            let _ = link.sink.set(Box::new(move |method, _| {
                if method == "Trace.batch" {
                    calls.lock().unwrap().push("set_up");
                }
            }));
        }
        let as_link: Arc<dyn Link> = link.clone();
        tester.set_link(Arc::downgrade(&as_link));
        let handler = {
            let calls = calls.clone();
            DevtoolsHandler {
                tester,
                link: link.clone(),
                next_step: AtomicU64::new(1),
                open_gate: Box::new(move || calls.lock().unwrap().push("gate_opened")),
            }
        };

        let mut setup = SessionSetup::default();
        setup.trace = true;
        handler.setup(setup).expect("setup");
        let model = model.lock().unwrap().clone().expect("the session started");
        model.apply(&nsh_core::MountBatch::default());
        handler.start();
        assert_eq!(
            *calls.lock().unwrap(),
            ["start_session", "set_up", "gate_opened"]
        );

        let (tx, rx) = mpsc::channel();
        handler.step(
            TestStep::Wait { ms: 0 },
            Box::new(move |result| tx.send(result).unwrap()),
        );
        let answer = rx.recv_timeout(Duration::from_secs(2)).expect("answered");
        assert!(answer.is_ok(), "{answer:?}");
    }
}
