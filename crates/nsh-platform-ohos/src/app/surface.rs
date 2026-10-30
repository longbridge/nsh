//! Instance-owned embedding in a native ContentSlot. Native measurement
//! follows the host layout; every load has a separate engine reply queue.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use napi_derive_ohos::napi;
use napi_ohos::{
    Env, Error, Result, Status,
    bindgen_prelude::{FnArgs, Function, FunctionRef, Object, ObjectRef, PromiseRaw},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use nsh_engine::Bundle;
use ohos_arkui_binding::{
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::Custom,
    },
    types::advanced::LengthMetricUnit,
};
use rustc_hash::FxHashMap;

use crate::{
    app::{
        environment::NshEnvironment,
        options::{Config, NshOptions},
        runtime::{App, Errors, Runtime},
    },
    interop::{attachment::ContentRoot, component::Components},
    render::{
        geometry::Size,
        node::{NativeNode, callback},
    },
};

pub(crate) type Queue = ThreadsafeFunction<Message, (), (), Status, false>;
pub(crate) struct LifecycleRoute {
    state: Weak<RefCell<State>>,
    queue: std::sync::Weak<Queue>,
    disposed: Weak<Cell<bool>>,
}
impl LifecycleRoute {
    pub(crate) fn is_alive(&self) -> bool {
        self.state.strong_count() > 0 && !self.disposed.upgrade().is_some_and(|d| d.get())
    }
    pub(crate) fn dispose(&self) -> Result<()> {
        if let Some(disposed) = self.disposed.upgrade() {
            disposed.set(true);
        }
        if let Some(state) = self.state.upgrade() {
            if let Ok(mut state) = state.try_borrow_mut() {
                return state.close();
            }
            if let Some(queue) = self.queue.upgrade() {
                queue.call(Message::Dispose, ThreadsafeFunctionCallMode::NonBlocking);
            }
        }
        Ok(())
    }
}
pub(crate) enum Message {
    #[cfg(feature = "dev")]
    DevCopied {
        revision: u64,
        result: std::result::Result<(), String>,
    },
    #[cfg(feature = "dev")]
    Dev {
        token: u64,
        event: nsh_dev::DevEvent,
    },
    #[cfg(feature = "dev")]
    DevReload {
        token: u64,
        version: (u64, u64),
    },
    Size(i32, i32),
    Window(bool),
    Visible(bool),
    HostCall(nsh_engine::HostCall),
    OpenUrl(nsh_engine::OpenUrlRequest),
    Dispose,
}

type HostHandler = FunctionRef<FnArgs<(serde_json::Value, NshHostReply)>, ()>;
type OpenUrlHandler = FunctionRef<String, bool>;

/// Reply handle for a `host.call(...)` or `host.post(...)` from the app (`nsh/host`).
#[napi]
pub struct NshHostReply {
    call: Arc<std::sync::Mutex<Option<nsh_engine::HostCall>>>,
    is_post: bool,
}

#[napi]
impl NshHostReply {
    /// True when the app used `host.post(name, data)` (one-way; `resolve` and `reject` are no-ops).
    #[napi(getter)]
    pub fn is_post(&self) -> bool {
        self.is_post
    }

    /// Resolves `host.call(name, data)` with a JSON-serializable value (`null` when omitted).
    #[napi]
    pub fn resolve(
        &self,
        #[napi(ts_arg_type = "string | number | boolean | object | null | undefined")]
        value: Option<serde_json::Value>,
    ) {
        if let Some(call) = self.call.lock().unwrap().take() {
            call.resolve(value.unwrap_or(serde_json::Value::Null));
        }
    }

    /// Rejects `host.call(name, data)` with an error message.
    #[napi]
    pub fn reject(&self, message: String) {
        if let Some(call) = self.call.lock().unwrap().take() {
            call.reject(message);
        }
    }
}

enum SurfacePhase {
    Empty,
    Loading { token: u64 },
    Staged(Box<App>),
    Running(Runtime),
    Closing(Option<Runtime>),
    Closed,
}
impl SurfacePhase {
    fn runtime(&self) -> Option<&Runtime> {
        match self {
            Self::Running(runtime) | Self::Closing(Some(runtime)) => Some(runtime),
            _ => None,
        }
    }
    fn take_runtime(&mut self) -> Option<Runtime> {
        match self {
            Self::Running(_) => {
                let Self::Running(runtime) = std::mem::replace(self, Self::Empty) else {
                    unreachable!()
                };
                Some(runtime)
            }
            Self::Closing(runtime) => runtime.take(),
            _ => None,
        }
    }
}

struct State {
    viewport: Option<NativeNode>,
    viewport_mounted: bool,
    config: Option<Config>,
    host: Rc<RefCell<Option<Arc<crate::app::session::Services>>>>,
    components: Components,
    host_handlers: FxHashMap<String, Rc<HostHandler>>,
    open_url_handler: Option<Rc<OpenUrlHandler>>,
    errors: Errors,
    phase: SurfacePhase,
    size: (i32, i32),
    attached: bool,
    visible: bool,
    disposed: Rc<Cell<bool>>,
    generation: Arc<AtomicU64>,
    queue: std::sync::Weak<Queue>,
    #[cfg(feature = "dev")]
    dev: Option<crate::app::dev::Session>,
    #[cfg(feature = "dev")]
    dev_generation: u64,
    #[cfg(feature = "dev")]
    overlay: Option<Rc<crate::app::dev::Overlay>>,
    // Native users above must retire before the NodeContent owner is released.
    content: Option<ContentRoot>,
}
impl State {
    fn config(&self) -> Result<&Config> {
        self.config
            .as_ref()
            .ok_or_else(|| Error::from_reason("surface is disposed"))
    }
    fn config_mut(&mut self) -> Result<&mut Config> {
        self.config
            .as_mut()
            .ok_or_else(|| Error::from_reason("surface is disposed"))
    }
    #[cfg(feature = "dev")]
    fn disconnect_dev(&mut self) {
        self.dev = None;
        self.errors.overlay(None);
        // Dropping the client disconnects its worker. The next connection gets
        // a new token, so queued replies from the old client cannot replace it.
        if let Some(overlay) = &self.overlay {
            overlay.hide();
        }
    }
    #[cfg(feature = "dev")]
    fn connect_dev(&mut self, address: String) -> Result<()> {
        self.check()?;
        let token = self
            .dev_generation
            .checked_add(1)
            .ok_or_else(|| Error::from_reason("development session generation exhausted"))?;
        let session = crate::app::dev::Session::connect(address, self.queue.clone(), token)?;
        if self.overlay.is_none() {
            let host = self.config()?.host.clone();
            self.overlay = Some(crate::app::dev::Overlay::new(
                self.viewport.as_ref().expect("live surface viewport"),
                host,
                self.queue.clone(),
            )?);
        }
        let size = self.logical_size()?;
        let overlay = self.overlay.as_ref().unwrap();
        overlay.hide();
        overlay.resize(size)?;
        self.errors.overlay(Some(overlay));
        self.next_load()?;
        self.dev = Some(session);
        self.dev_generation = token;
        Ok(())
    }
    #[cfg(feature = "dev")]
    fn development(&mut self, env: Env, token: u64, event: nsh_dev::DevEvent) -> Result<()> {
        use nsh_dev::DevEvent;
        if token != self.dev_generation || self.dev.is_none() {
            return Ok(());
        }
        match event {
            DevEvent::Connected => log::info!("OHOS dev server connected"),
            DevEvent::Disconnected => log::info!("OHOS dev server disconnected"),
            DevEvent::Testing
            | DevEvent::Step { .. }
            | DevEvent::TestSetup { .. }
            | DevEvent::Fetch { .. } => {}
            DevEvent::Root => {
                self.errors.report(
                    "The app is of Rust only, and this host was not built with its root.".into(),
                );
            }
            DevEvent::Building => {}
            DevEvent::BuildFailed(message) | DevEvent::CannotRun { message, .. } => {
                self.errors.report(message)
            }
            DevEvent::Patch {
                session,
                version,
                base,
                code,
                source_map,
            } => {
                let can_patch = self
                    .dev
                    .as_ref()
                    .is_some_and(|dev| dev.running == Some((session, base)))
                    && self.phase.runtime().is_some_and(|runtime| {
                        runtime.hot_update(
                            nsh_engine::HotUpdate {
                                name: format!("hot-{version}.js"),
                                code,
                                source_map: source_map.map(Into::into),
                            },
                            (session, version),
                        )
                    });
                if can_patch {
                    self.errors.clear();
                    if let Some(dev) = &mut self.dev {
                        dev.running = Some((session, version));
                    }
                }
            }
            DevEvent::Bundle {
                session,
                version,
                format,
                bytes,
                source_map,
                capabilities,
                app_id,
                ..
            } => {
                let bundle = crate::app::dev::Session::bundle(
                    format,
                    bytes,
                    source_map,
                    capabilities,
                    app_id,
                )?;
                let dev = self.dev.as_mut().unwrap();
                dev.latest = Some(((session, version), bundle.clone()));
                if dev.running == Some((session, version)) {
                    return Ok(());
                }
                let app = dev.app(bundle);
                dev.running = Some((session, version));
                self.next_load()?;
                self.accept_app(env, app)?;
            }
        }
        Ok(())
    }
    fn check(&self) -> Result<()> {
        if self.disposed.get()
            || matches!(self.phase, SurfacePhase::Closing(_) | SurfacePhase::Closed)
        {
            return Err(Error::from_reason("surface is disposed"));
        }
        self.config()?.host.check()
    }
    fn next_load(&mut self) -> Result<u64> {
        self.check()?;
        let next = self
            .generation
            .load(Ordering::Relaxed)
            .checked_add(1)
            .ok_or_else(|| Error::from_reason("surface load generation exhausted"))?;
        self.generation.store(next, Ordering::Release);
        if matches!(self.phase, SurfacePhase::Loading { .. }) {
            self.phase = SurfacePhase::Empty;
        }
        Ok(next)
    }
    fn accept(
        &mut self,
        env: Env,
        source: String,
        source_map: Option<String>,
        chunks: Option<Arc<crate::app::chunks::ResourceChunks>>,
    ) -> Result<()> {
        let config = self.config()?;
        let mut bundle = Bundle::source(source);
        bundle.policy = config.policy.clone();
        bundle.source_map = source_map.map(Into::into);
        let mut app = App::new(bundle, config.assets.clone());
        app.chunks = chunks;
        self.accept_app(env, app)
    }
    fn accept_app(&mut self, env: Env, mut app: App) -> Result<()> {
        self.check()?;
        if let Some(runtime) = self.phase.runtime() {
            runtime.close()?;
        }
        let granted = self.config()?.policy.granted.clone();
        app.owner = self.queue.clone();
        app.bundle.policy.granted = granted;
        self.phase = SurfacePhase::Staged(Box::new(app));
        self.errors.clear();
        self.start(env)
    }
    fn start(&mut self, env: Env) -> Result<()> {
        if !self.attached
            || self.size.0 <= 0
            || self.size.1 <= 0
            || !matches!(self.phase, SurfacePhase::Staged(_))
        {
            return Ok(());
        }
        self.check()?;
        let size = self.logical_size()?;
        let SurfacePhase::Staged(app) = std::mem::replace(&mut self.phase, SurfacePhase::Empty)
        else {
            unreachable!()
        };
        let config = self.config()?;
        let runtime = Runtime::start(
            env,
            self.viewport
                .as_ref()
                .expect("live surface viewport")
                .clone(),
            *app,
            size,
            config,
            self.components.clone(),
            self.errors.clone(),
        )?;
        runtime.set_attached(self.attached)?;
        runtime.set_visible(self.visible)?;
        self.phase = SurfacePhase::Running(runtime);
        Ok(())
    }
    fn logical_size(&self) -> Result<Size> {
        let scale = self.config()?.host.scale()?;
        let width = self.size.0 as f32 / scale;
        let height = self.size.1 as f32 / scale;
        if width > 16384.0 || height > 16384.0 {
            return Err(Error::from_reason("surface viewport exceeds 16384 vp"));
        }
        Ok(Size::new(width, height))
    }
    fn receive(&mut self, env: Env, message: Message) -> Result<()> {
        if matches!(message, Message::Dispose) {
            return self.close();
        }
        if self.disposed.get()
            || matches!(self.phase, SurfacePhase::Closing(_) | SurfacePhase::Closed)
        {
            return Ok(());
        }
        match message {
            #[cfg(feature = "dev")]
            Message::DevCopied { revision, result } => {
                if let Some(overlay) = &self.overlay {
                    overlay.copied(revision, result);
                }
                Ok(())
            }
            #[cfg(feature = "dev")]
            Message::Dev { token, event } => self.development(env, token, event),
            #[cfg(feature = "dev")]
            Message::DevReload { token, version } => {
                if token != self.dev_generation {
                    return Ok(());
                }
                let Some(dev) = &mut self.dev else {
                    return Ok(());
                };
                if dev.running != Some(version) {
                    return Ok(());
                }
                dev.running = None;
                // A full snapshot can precede the engine's failure notification.
                // Reload it now; otherwise the next snapshot completes recovery.
                if let Some((latest, bundle)) = &dev.latest
                    && *latest == version
                {
                    let app = dev.app(bundle.clone());
                    dev.running = Some(version);
                    self.next_load()?;
                    self.accept_app(env, app)?;
                }
                Ok(())
            }
            Message::Size(width, height) => {
                self.size = (width, height);
                let size = self.logical_size()?;
                #[cfg(feature = "dev")]
                if let Some(overlay) = &self.overlay {
                    overlay.resize(size)?;
                }
                if width > 0
                    && height > 0
                    && let Some(runtime) = self.phase.runtime()
                {
                    runtime.resize(size)?;
                }
                self.start(env)
            }
            Message::Visible(visible) => {
                self.visible = visible;
                if let Some(runtime) = self.phase.runtime() {
                    runtime.set_visible(visible)?;
                }
                Ok(())
            }
            Message::Window(attached) => {
                self.attached = attached;
                if let Some(runtime) = self.phase.runtime() {
                    runtime.set_attached(attached)?;
                }
                // A retained instance keeps its VM while removed from a layout,
                // like an iOS UIView. Only stop/dispose/Ability destruction ends it.
                self.start(env)
            }
            Message::HostCall(mut call) => {
                if let Some(handler) = self.host_handlers.get(call.name()).cloned() {
                    let name = call.name().to_owned();
                    let data = call.take_data();
                    let is_post = call.is_post();
                    let call_slot = Arc::new(std::sync::Mutex::new(Some(call)));
                    let reply = NshHostReply {
                        call: call_slot.clone(),
                        is_post,
                    };
                    let callback = handler.borrow_back(&env)?;
                    if let Err(error) = callback.call((data, reply).into()) {
                        log::error!("OHOS host handler {name}: {error}");
                        if let Some(call) = call_slot.lock().unwrap().take() {
                            call.reject(error.to_string());
                        }
                    }
                }
                Ok(())
            }
            Message::OpenUrl(request) => {
                let handled = if let Some(handler) = self.open_url_handler.clone() {
                    let callback = handler.borrow_back(&env)?;
                    match callback.call(request.url().to_owned()) {
                        Ok(handled) => handled,
                        Err(error) => {
                            log::error!("OHOS openUrl handler: {error}");
                            false
                        }
                    }
                } else {
                    false
                };
                request.answer(handled);
                Ok(())
            }
            Message::Dispose => unreachable!(),
        }
    }
    fn stop(&mut self) -> Result<()> {
        #[cfg(feature = "dev")]
        self.disconnect_dev();
        self.next_load()?;
        if let Some(runtime) = self.phase.runtime() {
            runtime.close()?;
        }
        self.phase = SurfacePhase::Empty;
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        if matches!(self.phase, SurfacePhase::Closed) {
            return Ok(());
        }
        self.disposed.set(true);
        #[cfg(feature = "dev")]
        self.disconnect_dev();
        self.generation.store(0, Ordering::Release);
        let runtime = self.phase.take_runtime();
        self.phase = SurfacePhase::Closing(runtime);
        if self.phase.runtime().is_some_and(Runtime::is_busy) {
            // A host widget may dispose its surface from a mount callback.
            // Keep the tree alive until that callback's native stack returns.
            if let Some(queue) = self.queue.upgrade() {
                queue.call(Message::Dispose, ThreadsafeFunctionCallMode::NonBlocking);
            }
            return Ok(());
        }
        let mut failure = None;
        if let SurfacePhase::Closing(runtime_slot) = &mut self.phase
            && let Some(runtime) = runtime_slot
        {
            match runtime.close() {
                Ok(()) => *runtime_slot = None,
                Err(error) => failure = Some(error),
            }
        }
        #[cfg(feature = "dev")]
        if let Some(overlay) = &self.overlay {
            match overlay.close() {
                Ok(()) => self.overlay = None,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        // Each independent owner gets its cleanup attempt. A failed owner stays
        // retained and keeps the mounted root available for an explicit retry.
        if let Some(error) = failure {
            return Err(error);
        }
        self.components.clear();
        self.host_handlers.clear();
        self.open_url_handler = None;
        self.errors.dispose();
        let content = self.content.as_mut().expect("live surface content");
        content.clear_window_callback().map_err(native_error)?;
        let viewport = self.viewport.take().expect("live surface viewport");
        if !self.viewport_mounted && viewport.borrow().is_owned() {
            let disposed = viewport.borrow_mut().dispose();
            if let Err(error) = disposed {
                self.viewport = Some(viewport);
                return Err(native_error(error));
            }
        }
        let retry = Rc::downgrade(&viewport);
        drop(viewport);
        // ContentRoot is the sole root owner during unmount. Keep only a weak
        // route for restoring State's handle when native teardown needs retry.
        if let Err(error) = content.unmount() {
            self.viewport = retry.upgrade();
            return Err(native_error(error));
        }
        self.viewport_mounted = false;
        self.phase = SurfacePhase::Closed;
        self.config = None;
        self.host.borrow_mut().take();
        Ok(())
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            log::error!("OHOS surface teardown: {error}; retaining failed native owners");
            std::mem::forget((
                self.phase.take_runtime(),
                self.content.take(),
                self.viewport.take(),
                self.components.clone(),
            ));
            #[cfg(feature = "dev")]
            std::mem::forget(self.overlay.take());
        }
    }
}

/// An independently owned Native Shell view. Place `content` in one ContentSlot,
/// load a module, and dispose the instance when its owner releases it.
#[napi]
pub struct NshSurface {
    state: Rc<RefCell<State>>,
    components: Components,
    errors: Errors,
    disposed: Rc<Cell<bool>>,
    host: Rc<RefCell<Option<Arc<crate::app::session::Services>>>>,
    queue: Arc<Queue>,
}

#[napi]
impl NshSurface {
    /// Creates an empty view bound to `uiContext` with validated per-instance limits and capability grants.
    #[napi(constructor)]
    pub fn new(
        env: Env,
        #[napi(ts_arg_type = "import('@ohos.arkui.UIContext').UIContext")] ui_context: Object<'_>,
        options: Option<NshOptions>,
    ) -> Result<Self> {
        crate::init_logging();
        let host = crate::app::entry::bind_session(&env, &ui_context)?;
        let config = Config::new(host.clone(), options.unwrap_or_default())?;
        host.scale()?;
        let content = ContentRoot::new(env)?;
        let viewport = Custom::new().map_err(native_error)?.into_node();
        let components = Components::default();
        let errors = Errors::default();
        let disposed = Rc::new(Cell::new(false));
        let host_slot = Rc::new(RefCell::new(Some(host.clone())));
        let state = Rc::new(RefCell::new(State {
            content: Some(content),
            viewport: Some(viewport),
            viewport_mounted: false,
            config: Some(config),
            host: host_slot.clone(),
            components: components.clone(),
            host_handlers: FxHashMap::default(),
            open_url_handler: None,
            errors: errors.clone(),
            phase: SurfacePhase::Empty,
            size: (0, 0),
            attached: false,
            visible: true,
            disposed: disposed.clone(),
            generation: Arc::new(AtomicU64::new(1)),
            queue: std::sync::Weak::new(),
            #[cfg(feature = "dev")]
            dev: None,
            #[cfg(feature = "dev")]
            dev_generation: 0,
            #[cfg(feature = "dev")]
            overlay: None,
        }));
        // Scope construction handles so State can clean up with sole ownership
        // if any setup step fails before ContentRoot accepts the viewport.
        let queue = (|| -> Result<Arc<Queue>> {
            let viewport = state
                .borrow()
                .viewport
                .as_ref()
                .expect("live surface viewport")
                .clone();
            viewport
                .borrow()
                .set_length_metric_unit(LengthMetricUnit::Px)
                .map_err(native_error)?;
            viewport.borrow().set_clip(true).map_err(native_error)?;
            let ui_state = state.clone();
            let wake = env.create_function_from_closure::<(), (), _>("nshSurface", |_| Ok(()))?;
            let queue = Arc::new(
                wake.build_threadsafe_function::<Message>()
                    .callee_handled::<false>()
                    .build_callback(move |ctx| {
                        let mut state = ui_state.borrow_mut();
                        if let Err(error) = state.receive(ctx.env, ctx.value) {
                            state.errors.report(error.to_string());
                        }
                        Ok(())
                    })?,
            );
            state.borrow_mut().queue = Arc::downgrade(&queue);
            let weak = Arc::downgrade(&queue);
            state
                .borrow_mut()
                .content
                .as_mut()
                .expect("live surface content")
                .on_window_change(move |attached| {
                    if let Some(queue) = weak.upgrade() {
                        queue.call(
                            Message::Window(attached),
                            ThreadsafeFunctionCallMode::NonBlocking,
                        );
                    }
                })
                .map_err(native_error)?;
            let weak = Arc::downgrade(&queue);
            let measured = Cell::new((0, 0));
            let children = Rc::downgrade(&viewport);
            viewport.borrow_mut().on_custom_measure(move |event| {
                callback(|| {
                    let constraint = event.layout_constraint_in_measure().ok_or(
                        crate::render::node::Error::Unsupported("missing surface constraints"),
                    )?;
                    let size = (
                        constraint.max_width().max(0),
                        constraint.max_height().max(0),
                    );
                    event.set_measured_size(size.0, size.1)?;
                    if measured.replace(size) != size
                        && let Some(queue) = weak.upgrade()
                    {
                        queue.call(
                            Message::Size(size.0, size.1),
                            ThreadsafeFunctionCallMode::NonBlocking,
                        );
                    }
                    if let Some(parent) = children.upgrade() {
                        let mut measure = crate::render::layout::ChildMeasure::new(event);
                        for child in parent.borrow().children() {
                            measure.child(&child.borrow())?;
                        }
                    }
                    Ok(())
                })
            });
            viewport.borrow().set_attribute(ohos_arkui_binding::types::attribute::ArkUINodeAttributeType::VisibleAreaChangeRatio, vec![0.0_f32].into()).map_err(native_error)?;
            let visible_queue = Arc::downgrade(&queue);
            viewport.borrow_mut().on_event(
                ohos_arkui_binding::types::event::NodeEventType::EventOnVisibleAreaChange,
                move |event| {
                    let visible = event
                        .component_i32(0)
                        .or_else(|| event.i32_value(0))
                        .unwrap_or(0)
                        != 0;
                    if let Some(queue) = visible_queue.upgrade() {
                        queue.call(
                            Message::Visible(visible),
                            ThreadsafeFunctionCallMode::NonBlocking,
                        );
                    }
                },
            );
            let children = Rc::downgrade(&viewport);
            viewport.borrow_mut().on_custom_layout(move |event| {
                callback(|| {
                    event.set_layout_position(0, 0)?;
                    if let Some(parent) = children.upgrade() {
                        for child in parent.borrow().children() {
                            event.layout_child(&child.borrow(), 0, 0)?;
                        }
                    }
                    Ok(())
                })
            });
            state
                .borrow_mut()
                .content
                .as_mut()
                .expect("live surface content")
                .mount(viewport)
                .map_err(native_error)?;
            state.borrow_mut().viewport_mounted = true;
            Ok(queue)
        })()?;
        host.register_surface(LifecycleRoute {
            state: Rc::downgrade(&state),
            queue: Arc::downgrade(&queue),
            disposed: Rc::downgrade(&disposed),
        });
        Ok(Self {
            state,
            components,
            errors,
            disposed,
            host: host_slot,
            queue,
        })
    }

    /// Platform NodeContent for `ContentSlot(surface.content)`. Keep the surface
    /// in an ordinary host field; one instance can occupy one slot at a time.
    #[napi(getter, ts_return_type = "import('@ohos.arkui.node').NodeContent")]
    pub fn content<'env>(&self, env: &'env Env) -> Result<Object<'env>> {
        self.check()?;
        self.state
            .try_borrow()
            .map_err(reentry)?
            .content
            .as_ref()
            .expect("live surface content")
            .object(env)
    }

    /// Loads trusted source, replacing this instance's app. Loading before layout
    /// stages the source until its ContentSlot has a positive native size.
    #[napi]
    pub fn load_source(&self, env: Env, source: String, source_map: Option<String>) -> Result<()> {
        self.check()?;
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        #[cfg(feature = "dev")]
        state.disconnect_dev();
        state.next_load()?;
        let result = state.accept(env, source, source_map, None);
        if let Err(error) = &result {
            self.errors.report(error.to_string());
        }
        result
    }

    /// Consumes a verified package and starts it with this instance's grants and limits.
    /// Assets and declared capabilities come from the signed package.
    #[napi]
    pub fn run_package(&self, env: Env, package: &crate::app::trust::NshPackage) -> Result<()> {
        let host = self.host()?;
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        if state.phase.runtime().is_some_and(Runtime::is_busy) {
            return Err(reentry("engine callback"));
        }
        let prepared = package.take()?;
        state.next_load()?;
        #[cfg(feature = "dev")]
        state.disconnect_dev();
        let result = state.accept_app(
            env,
            App::new(
                prepared.bundle,
                Arc::new(crate::services::assets::HostAssets::new(
                    host,
                    Some(prepared.assets),
                )),
            ),
        );
        if let Err(error) = &result {
            self.errors.report(error.to_string());
        }
        result
    }

    /// Reads a source or precompiled HAP resource and its companion capabilities.
    /// The promise resolves when accepted; JS evaluation errors use the error
    /// handler. A later load, stop, dispose or Ability teardown cancels this load.
    #[napi(ts_return_type = "Promise<void>")]
    pub fn load_resource<'env>(
        &self,
        env: &'env Env,
        name: String,
        source_map: Option<String>,
    ) -> Result<PromiseRaw<'env, ()>> {
        let host = self.host()?;
        crate::app::session::resource_name(&name)?;
        let permit = host.reserve_load()?;
        let (token, generation, has_override) = {
            let mut state = self.state.try_borrow_mut().map_err(reentry)?;
            #[cfg(feature = "dev")]
            state.disconnect_dev();
            if let Some(runtime) = state.phase.runtime() {
                runtime.close()?;
            }
            let token = state.next_load()?;
            let has_override = state.config()?.policy.granted.is_some();
            state.phase = SurfacePhase::Loading { token };
            (token, state.generation.clone(), has_override)
        };
        let chunks = crate::app::chunks::ResourceChunks::new(host.clone(), &name);
        let entry_chunks_loader = chunks.clone();
        let weak = Rc::downgrade(&self.state);
        let future = async move {
            let result = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                if generation.load(Ordering::Acquire) != token {
                    return Err(cancelled());
                }
                let bytes = host.resource(&name)?;
                if generation.load(Ordering::Acquire) != token {
                    return Err(cancelled());
                }
                let mut bundle = if nsh_engine::is_bytecode(&bytes) {
                    // SAFETY: HAP raw resources are supplied by the embedding host,
                    // with the same trusted-bytecode contract as its executable code.
                    unsafe { Bundle::bytecode(bytes) }
                } else {
                    Bundle::source(
                        String::from_utf8(bytes)
                            .map_err(|_| Error::from_reason("source resource must be UTF-8"))?,
                    )
                };
                let app_json = name.rsplit_once('/').map_or_else(
                    || "app.json".to_owned(),
                    |(parent, _)| format!("{parent}/app.json"),
                );
                let mut manifest_specs = Vec::new();
                if let Ok(bytes) = host.resource(&app_json)
                    && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
                {
                    if let Some(id) = value.get("id").and_then(serde_json::Value::as_str) {
                        bundle.app_id = Some(id.to_owned());
                    }
                    if let Some(specs) = value
                        .get("entryChunks")
                        .and_then(serde_json::Value::as_array)
                    {
                        manifest_specs.extend(
                            specs
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .map(str::to_owned),
                        );
                    }
                }
                let initial_chunks = entry_chunks_loader
                    .collect_entry_chunks(&bundle, &manifest_specs)
                    .map_err(Error::from_reason)?;
                if !initial_chunks.is_empty() {
                    bundle = bundle.with_chunks(initial_chunks);
                }
                if !has_override {
                    let companion = name.rsplit_once('/').map_or_else(
                        || "capabilities.json".to_owned(),
                        |(parent, _)| format!("{parent}/capabilities.json"),
                    );
                    if let Ok(bytes) = host.resource(&companion) {
                        let value = serde_json::from_slice(&bytes)
                            .map_err(|e| Error::from_reason(e.to_string()))?;
                        bundle.policy.declared = Some(
                            nsh_engine::Capabilities::from_json(&value)
                                .map_err(Error::from_reason)?,
                        );
                    }
                }
                Ok(bundle)
            })
            .await
            .map_err(|error| Error::from_reason(error.to_string()))
            .and_then(|result| result);
            Ok(result)
        };
        let promise = env.spawn_future_with_callback(future, move |env, result| {
            let state = weak.upgrade().ok_or_else(cancelled)?;
            let mut state = state.try_borrow_mut().map_err(reentry)?;
            if state.generation.load(Ordering::Acquire) != token
                || !matches!(state.phase, SurfacePhase::Loading { token: active } if active == token)
            {
                return Err(cancelled());
            }
            state.phase = SurfacePhase::Empty;
            let result = state.check().and(result).and_then(|mut bundle| {
                let config = state.config()?;
                bundle.policy.granted = config.policy.granted.clone();
                bundle.source_map = source_map.map(Into::into);
                let mut app = App::new(bundle, config.assets.clone());
                app.chunks = Some(chunks);
                state.accept_app(*env, app)
            });
            if let Err(error) = &result {
                state.errors.report(error.to_string());
            }
            result
        });
        if promise.is_err() {
            let mut state = self.state.borrow_mut();
            if matches!(state.phase, SurfacePhase::Loading { token: active } if active == token) {
                state.phase = SurfacePhase::Empty;
            }
        }
        promise
    }

    /// Stops the app and cancels pending loads, retaining this view and its factories.
    /// A subsequent load starts a fresh engine in the same native view.
    #[napi]
    pub fn stop(&self) -> Result<()> {
        self.check()?;
        self.state.try_borrow_mut().map_err(reentry)?.stop()
    }

    /// Selects this view as the target for system back handling. Pointer/focus
    /// input selects it automatically; hosts may select it when switching tabs.
    #[napi]
    pub fn activate(&self) -> Result<()> {
        let host = self.host()?;
        host.select_surface(Arc::downgrade(&self.queue));
        Ok(())
    }

    /// Routes a system back press to the window session owning this surface.
    /// Returns `true` if a running Native Shell app consumed the back action.
    #[napi]
    pub fn on_back_press(&self) -> bool {
        !self.disposed.get()
            && self
                .host
                .borrow()
                .as_ref()
                .is_some_and(|host| host.handle_back())
    }

    /// Connects this development view to a server, preserving state through hot updates.
    /// Available in SDK builds with the `dev` feature.
    #[napi]
    pub fn connect_dev_server(&self, address: String) -> Result<()> {
        self.check()?;
        #[cfg(feature = "dev")]
        let result = self
            .state
            .try_borrow_mut()
            .map_err(reentry)?
            .connect_dev(address);
        #[cfg(not(feature = "dev"))]
        let result = {
            let _ = address;
            Err(Error::from_reason(
                "connectDevServer requires the development SDK",
            ))
        };
        if let Err(error) = &result {
            self.errors.report(error.to_string());
        }
        result
    }
    /// Disconnects the development client, retaining the last loaded module.
    #[napi]
    pub fn disconnect_dev_server(&self) -> Result<()> {
        self.check()?;
        #[cfg(feature = "dev")]
        self.state
            .try_borrow_mut()
            .map_err(reentry)?
            .disconnect_dev();
        Ok(())
    }

    /// Sets the launch parameters (`host.params` in `nsh/host`) for apps started from now on.
    /// A running app keeps the parameters it started with.
    #[napi]
    pub fn set_params(
        &self,
        #[napi(ts_arg_type = "object | null | undefined")] params: Option<serde_json::Value>,
    ) -> Result<()> {
        self.check()?;
        let map = match params {
            None | Some(serde_json::Value::Null) => serde_json::Map::new(),
            Some(serde_json::Value::Object(map)) => map,
            Some(_) => {
                return Err(Error::from_reason(
                    "params must be a JSON object, null, or undefined",
                ));
            }
        };
        let state = self.state.try_borrow().map_err(reentry)?;
        state.config()?.channel.set_params(map);
        Ok(())
    }

    /// Sends event `name` with `data` to the running app (`host.on(name, listener)` in `nsh/host`).
    /// Events sent before the app listens or starts are queued up to the channel limit.
    #[napi]
    pub fn send(
        &self,
        name: String,
        #[napi(ts_arg_type = "string | number | boolean | object | null | undefined")] data: Option<
            serde_json::Value,
        >,
    ) -> Result<()> {
        self.check()?;
        let state = self.state.try_borrow().map_err(reentry)?;
        state
            .config()?
            .channel
            .send(name, data.unwrap_or(serde_json::Value::Null))
            .map_err(Error::from_reason)
    }

    /// Registers a handler for `host.call(name, data)` and `host.post(name, data)` (`nsh/host`).
    /// Pass `null` or `undefined` to remove the handler for `name`.
    #[napi]
    pub fn handle(
        &self,
        name: String,
        #[napi(
            ts_arg_type = "((data: string | number | boolean | object | null, reply: NshHostReply) => void) | null | undefined"
        )]
        handler: Option<HostHandler>,
    ) -> Result<()> {
        self.check()?;
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        if let Some(handler) = handler {
            let weak = state.queue.clone();
            state.config()?.channel.handle(name.clone(), move |call| {
                if let Some(queue) = weak.upgrade() {
                    let status = queue.call(
                        Message::HostCall(call),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                    if status != Status::Ok {
                        log::error!("OHOS host call dispatch failed: {status:?}");
                    }
                }
            });
            state.host_handlers.insert(name, Rc::new(handler));
        } else {
            state.config()?.channel.remove_handler(&name);
            state.host_handlers.remove(&name);
        }
        Ok(())
    }

    /// Removes a `nsh/host` call/post handler previously registered with `handle`.
    #[napi]
    pub fn remove_handler(&self, name: String) -> Result<()> {
        self.check()?;
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        state.config()?.channel.remove_handler(&name);
        state.host_handlers.remove(&name);
        Ok(())
    }

    /// Intercepts URLs opened by the app (`openUrl` in `nsh/core` and Markdown links)
    /// after scheme checks pass. Return `true` if handled by the host, or `false` to
    /// fall back to the system URL handler. Pass `null` or `undefined` to remove.
    #[napi]
    pub fn set_open_url_handler(
        &self,
        #[napi(ts_arg_type = "((url: string) => boolean) | null | undefined")] handler: Option<
            OpenUrlHandler,
        >,
    ) -> Result<()> {
        self.check()?;
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        if let Some(handler) = handler {
            let weak = state.queue.clone();
            state.config()?.channel.handle_open_url(move |request| {
                if let Some(queue) = weak.upgrade() {
                    let status = queue.call(
                        Message::OpenUrl(request),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                    if status != Status::Ok {
                        log::error!("OHOS openUrl dispatch failed: {status:?}");
                    }
                }
            });
            state.open_url_handler = Some(Rc::new(handler));
        } else {
            state.config()?.channel.remove_open_url_handler();
            state.open_url_handler = None;
        }
        Ok(())
    }

    /// Applies the embedding app's theme overrides (a shadcn/ui `cssVars` theme JSON string,
    /// or `null`/`undefined` to restore the app's own theme), taking effect immediately and
    /// for all subsequent loads on this surface.
    #[napi]
    pub fn set_theme_overrides(&self, json: Option<String>) -> Result<()> {
        self.check()?;
        let tokens = match json {
            None => None,
            Some(json) => Some(
                nsh_base::theme_overrides(&json)
                    .map_err(|err| Error::from_reason(format!("invalid theme: {err}")))?,
            ),
        };
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        if let Some(runtime) = state.phase.runtime() {
            runtime.set_tokens(tokens.clone())?;
        }
        state.config_mut()?.host_tokens = tokens;
        Ok(())
    }

    /// Forwards per-instance environment observations, including before the first load.
    #[napi]
    pub fn set_environment(&self, changes: NshEnvironment) -> Result<()> {
        self.check()?;
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        changes.clone().apply(nsh_engine::Environment::default())?;
        if let Some(runtime) = state.phase.runtime() {
            runtime.set_environment(changes.clone())?;
        }
        state.config_mut()?.environment.merge(changes);
        Ok(())
    }

    /// Receives asynchronous load, engine and native mount errors for this instance.
    #[napi]
    pub fn set_error_handler(&self, handler: Function<'_, String, ()>) -> Result<()> {
        self.check()?;
        self.errors.set_handler(handler)
    }

    /// Registers a factory on this instance, before or after a module is loaded.
    #[napi]
    pub fn register_component(
        &self,
        name: String,
        #[napi(ts_arg_type = "(id: number) => object")] factory: FunctionRef<u32, ObjectRef>,
    ) -> Result<()> {
        self.check()?;
        self.components.register(name, factory);
        Ok(())
    }

    /// Removes a factory for future mounts; already mounted widgets keep their owner.
    #[napi]
    pub fn unregister_component(&self, name: String) -> Result<()> {
        self.check()?;
        self.components.unregister(&name);
        Ok(())
    }

    /// Emits to the live component instance identified by its factory handle.
    /// Handles from stopped or replaced engines are rejected.
    #[napi]
    pub fn emit_component_event(
        &self,
        handle: u32,
        name: String,
        #[napi(ts_arg_type = "string | number | boolean | object | null")] data: serde_json::Value,
    ) -> Result<()> {
        self.check()?;
        let context = self.components.context(handle)?;
        context.control.dispatch_event(
            context.node,
            nsh_core::EventKind::NativeEvent,
            nsh_core::EventPayload::Native {
                name: name.into(),
                data,
            },
        );
        Ok(())
    }

    /// Reports a live native component's intrinsic dimensions in vp.
    #[napi]
    pub fn set_component_size(&self, handle: u32, width: f64, height: f64) -> Result<()> {
        self.check()?;
        if [width, height]
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=16384.0).contains(v))
        {
            return Err(Error::from_reason(
                "component size must be finite and between 0 and 16384",
            ));
        }
        let context = self.components.context(handle)?;
        context
            .control
            .set_component_size(context.node, Size::new(width as f32, height as f32).into());
        Ok(())
    }

    /// Permanently releases this view and its engine. Repeated calls are harmless.
    #[napi]
    pub fn dispose(&self) -> Result<()> {
        self.disposed.set(true);
        match self.state.try_borrow_mut() {
            Ok(mut state) => state.close(),
            Err(_) => {
                self.queue
                    .call(Message::Dispose, ThreadsafeFunctionCallMode::NonBlocking);
                Ok(())
            }
        }
    }

    /// Whether the current module's engine is running.
    #[napi(getter)]
    pub fn is_running(&self) -> Result<bool> {
        Ok(!self.disposed.get()
            && self
                .state
                .try_borrow()
                .map_err(reentry)?
                .phase
                .runtime()
                .is_some_and(Runtime::is_running))
    }
    /// Whether this instance has been permanently released.
    #[napi(getter)]
    pub fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    /// Whether source is being read or waiting for an attached, positive-size slot.
    /// Evaluation starts asynchronously after this becomes false.
    #[napi(getter)]
    pub fn is_loading(&self) -> Result<bool> {
        if self.disposed.get() {
            return Ok(false);
        }
        let state = self.state.try_borrow().map_err(reentry)?;
        Ok(matches!(
            state.phase,
            SurfacePhase::Loading { .. } | SurfacePhase::Staged(_)
        ))
    }
    /// Latest load, engine or native mount error; cleared by a new accepted source.
    #[napi(getter)]
    pub fn last_error(&self) -> String {
        self.errors.last()
    }
}
impl NshSurface {
    fn host(&self) -> Result<Arc<crate::app::session::Services>> {
        if self.disposed.get() {
            return Err(Error::from_reason("surface is disposed"));
        }
        let host = self
            .host
            .borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| Error::from_reason("surface is disposed"))?;
        host.check()?;
        Ok(host)
    }
    fn check(&self) -> Result<()> {
        self.host().map(|_| ())
    }
}
impl Drop for NshSurface {
    fn drop(&mut self) {
        if let Err(error) = self.dispose() {
            log::error!("OHOS surface finalization: {error}");
        }
    }
}
fn native_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}
fn reentry(_: impl std::fmt::Display) -> Error {
    Error::from_reason("defer surface mutation until the next host turn")
}
fn cancelled() -> Error {
    Error::from_reason("surface load was cancelled")
}

/// Installs the optional HiLog logger for a development host.
#[napi]
pub fn init_logging() {
    #[cfg(feature = "dev")]
    crate::app::dev::init_logging();
    #[cfg(not(feature = "dev"))]
    ohos_hilog_binding::log::init_once(
        ohos_hilog_binding::log::Config::default()
            .with_tag("nsh")
            .with_max_level(log::LevelFilter::Info),
    );
}
