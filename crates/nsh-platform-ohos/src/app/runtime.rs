//! One engine generation. Closing it invalidates all outstanding batches and replies.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Weak},
};

use napi_ohos::{
    Env, Status,
    bindgen_prelude::Function,
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use nsh_core::MountBatch;
use nsh_engine::{Bundle, Engine, EngineConfig, Environment, MountSink, Os};
use ohos_vsync_binding::Vsync;

use crate::{
    app::{environment::NshEnvironment, options::Config},
    render::{
        geometry::Size,
        mount::Mounter,
        node::{Error, NativeNode, Result},
    },
    text::typography::OhosTextEngine,
};

type ErrorCallback = ThreadsafeFunction<String, (), String, Status, false>;
#[derive(Clone, Default)]
pub(crate) struct Errors(Rc<RefCell<ErrorState>>);
#[derive(Default)]
struct ErrorState {
    last: String,
    callback: Option<ErrorCallback>,
    #[cfg(feature = "dev")]
    overlay: Option<std::rc::Weak<crate::app::dev::Overlay>>,
}
impl Errors {
    pub(crate) fn report(&self, message: String) {
        log::error!("OHOS surface: {message}");
        let mut errors = self.0.borrow_mut();
        errors.last = message.clone();
        #[cfg(feature = "dev")]
        if let Some(overlay) = errors.overlay.as_ref().and_then(std::rc::Weak::upgrade) {
            overlay.show(&message);
        }
        if let Some(callback) = &errors.callback {
            callback.call(message, ThreadsafeFunctionCallMode::NonBlocking);
        }
    }
    pub(crate) fn clear(&self) {
        self.0.borrow_mut().last.clear();
        #[cfg(feature = "dev")]
        if let Some(overlay) = self
            .0
            .borrow()
            .overlay
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
        {
            overlay.hide();
        }
    }
    #[cfg(feature = "dev")]
    pub(crate) fn overlay(&self, overlay: Option<&Rc<crate::app::dev::Overlay>>) {
        self.0.borrow_mut().overlay = overlay.map(Rc::downgrade);
    }
    pub(crate) fn last(&self) -> String {
        self.0.borrow().last.clone()
    }
    pub(crate) fn set_handler(&self, handler: Function<'_, String, ()>) -> napi_ohos::Result<()> {
        self.0.borrow_mut().callback = Some(
            handler
                .build_threadsafe_function::<String>()
                .callee_handled::<false>()
                .build()?,
        );
        Ok(())
    }
    pub(crate) fn dispose(&self) {
        let mut state = self.0.borrow_mut();
        state.callback = None;
        #[cfg(feature = "dev")]
        {
            state.overlay = None;
        }
    }
}

pub(crate) enum Message {
    Arkui,
    ScrollReports,
    #[cfg(feature = "dev")]
    HotFailed,
    Chunk(std::result::Result<nsh_engine::Chunk, (String, String)>),
    Active(bool),
    Back,
    /// Native controls changed Back priority outside an engine commit.
    UpdateBack,
    DismissKeyboard,
    ReconcileNavigation,
    ScrollPosition {
        id: u32,
        x: f32,
        y: f32,
        animated: bool,
    },
    DialogResult {
        id: u32,
        index: i32,
    },
    HostEnvironment(NshEnvironment),
    HostWindow(
        [crate::app::session::AvoidArea; 3],
        crate::app::session::AvoidArea,
    ),
    HostEvent(nsh_core::EventKind, nsh_core::EventPayload),
    TextSelection(
        nsh_core::NodeId,
        crate::text::selection::controller::Command,
    ),
    Composite {
        id: nsh_core::NodeId,
        generation: u32,
    },
    Scene,
    /// The shader thread left frames (`render::shader`).
    Shader,
    #[cfg(feature = "api-23")]
    AccessibilityEvent(nsh_core::NodeId, ohos_accessibility_binding::EventType),
    HostClosed,
    ClipboardProbe(std::sync::mpsc::SyncSender<std::result::Result<bool, String>>),
    Close,
    Image(crate::media::image::Ready),
    ImageProgress(crate::media::image::Progress),
    Batch(MountBatch),
    Error(String),
    RequestFrame,
    Frame,
    FrameSynced,
    Reclaim(u64),
    VideoRetired(u64),
    VideoUi(u64, crate::media::video::UiState),
}
pub(crate) struct Dispatcher {
    pub(crate) owner: std::sync::Weak<crate::app::surface::Queue>,
    queue: ThreadsafeFunction<Message, (), (), Status, false>,
    pub(crate) wants_back: std::sync::atomic::AtomicBool,
    /// Keeps mounted status-bar changes hidden during native video fullscreen.
    pub(crate) video_fullscreen: std::sync::atomic::AtomicBool,
    pub(crate) active: std::sync::atomic::AtomicBool,
    pub(crate) live: std::sync::atomic::AtomicBool,
    pub(crate) status_bar: std::sync::Mutex<Option<(nsh_core::StatusBarConfig, bool)>>,
    pub(crate) keyboard: std::sync::Mutex<crate::input::keyboard::Snapshot>,
}
impl Dispatcher {
    pub(crate) fn request_frame(&self) {
        self.send(Message::RequestFrame);
    }
    pub(crate) fn image(&self, ready: crate::media::image::Ready) {
        self.send(Message::Image(ready));
    }
    pub(crate) fn image_progress(&self, progress: crate::media::image::Progress) {
        self.send(Message::ImageProgress(progress));
    }
    pub(crate) fn send(&self, message: Message) {
        let status = self
            .queue
            .call(message, ThreadsafeFunctionCallMode::NonBlocking);
        if status != Status::Ok {
            log::error!("OHOS UI dispatch failed: {status:?}");
        }
    }
}
struct Sink(
    Arc<Dispatcher>,
    Option<Arc<crate::app::chunks::ResourceChunks>>,
);
impl MountSink for Sink {
    fn load_chunk(&self, id: &str) -> bool {
        self.1
            .as_ref()
            .is_some_and(|chunks| chunks.load(id, Arc::downgrade(&self.0)))
    }
    fn submit(&self, mut batch: MountBatch) {
        // Native paragraphs belong to the engine. As on Linux, the UI receives
        // completed drawing data, not the text engine's layout machinery.
        for item in &mut batch.items {
            if let nsh_core::MountItem::Text { layout, .. } = item
                && let Some(text) = layout
                    .0
                    .downcast_ref::<crate::text::typography::EngineText>()
            {
                *layout = nsh_core::TextLayout(text.native.clone());
            }
        }
        self.0.send(Message::Batch(batch));
    }
    fn report_error(&self, message: &str) {
        self.0.send(Message::Error(message.into()));
    }
    fn request_frame(&self) {
        self.0.send(Message::RequestFrame);
    }
    #[cfg(feature = "dev")]
    fn hot_update_failed(&self) {
        self.0.send(Message::HotFailed);
    }
}

struct VsyncVote {
    remaining: std::sync::atomic::AtomicU8,
    armed: std::sync::atomic::AtomicBool,
}

fn arm_vsync_vote(vsync: &Arc<Vsync>, vote: &Arc<VsyncVote>) {
    use std::sync::atomic::Ordering;
    let weak = Arc::downgrade(vsync);
    let vote_cb = vote.clone();
    if vsync.request_frame_once(move |_| {
        let prev = vote_cb
            .remaining
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                Some(n.saturating_sub(1))
            })
            .unwrap_or(0);
        if prev > 1
            && let Some(vsync) = weak.upgrade()
        {
            arm_vsync_vote(&vsync, &vote_cb);
        } else {
            vote_cb.armed.store(false, Ordering::Release);
            if vote_cb.remaining.load(Ordering::Acquire) > 0
                && let Some(vsync) = weak.upgrade()
                && vote_cb
                    .armed
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                arm_vsync_vote(&vsync, &vote_cb);
            }
        }
    }) != 0
    {
        vote.armed.store(false, Ordering::Release);
    }
}

struct State {
    self_ref: std::rc::Weak<RefCell<Self>>,
    parent: NativeNode,
    running: Rc<Cell<bool>>,
    engine: Option<Engine>,
    mounter: Option<Mounter>,
    dispatcher: Weak<Dispatcher>,
    vsync: Arc<Vsync>,
    vsync_vote: Arc<VsyncVote>,
    environment: Environment,
    errors: Errors,
    frame_pending: bool,
    foreground: bool,
    attached: bool,
    visible: bool,
    frame_wanted: bool,
    #[cfg(feature = "dev")]
    reload: Option<(u64, std::sync::Weak<crate::app::surface::Queue>)>,
    #[cfg(feature = "dev")]
    hot_version: Option<(u64, u64)>,
}
impl State {
    fn receive(&mut self, message: Message) -> Result<()> {
        if matches!(message, Message::Close | Message::HostClosed) {
            return self.close();
        }
        if let Message::Reclaim(epoch) = message {
            if let Some(mounter) = &mut self.mounter {
                mounter.reclaim(epoch);
            }
            return Ok(());
        }
        if let Message::VideoRetired(token) = message {
            crate::media::video::retired(token);
            return Ok(());
        }
        if let Message::VideoUi(token, state) = message {
            crate::media::video::update(token, state);
            return Ok(());
        }
        if self.engine.is_none() {
            return Ok(());
        }
        match message {
            Message::Scene => self.mounter.as_ref().unwrap().scene_ready()?,
            Message::Shader => self.mounter.as_ref().unwrap().shaders.collect()?,
            Message::Arkui => self.mounter.as_ref().unwrap().flush_arkui()?,
            Message::ScrollReports => {
                self.pulse_frame_rate_vote();
                self.mounter.as_ref().unwrap().flush_scroll_reports()?;
            }
            Message::TextSelection(id, command) => {
                self.mounter.as_ref().unwrap().text_selection(id, command)?
            }
            Message::Composite { id, generation } => self
                .mounter
                .as_ref()
                .unwrap()
                .composite_ready(id, generation)?,
            #[cfg(feature = "dev")]
            Message::HotFailed => {
                if let Some((token, queue)) = &self.reload
                    && let Some(queue) = queue.upgrade()
                    && let Some(version) = self.hot_version
                {
                    queue.call(
                        crate::app::surface::Message::DevReload {
                            token: *token,
                            version,
                        },
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }
            }
            Message::Chunk(result) => {
                let control = self.engine.as_ref().unwrap().control();
                match result {
                    Ok(chunk) => control.chunk_loaded(chunk),
                    Err((id, error)) => control.chunk_failed(id, error),
                }
            }
            Message::Active(active) => {
                self.foreground = active;
                if !active {
                    self.mounter.as_ref().unwrap().cancel_input()?;
                }
                if active {
                    self.mounter.as_ref().unwrap().shaders.wake();
                    self.request_frame()?;
                }
            }
            Message::Back => {
                self.mounter.as_mut().unwrap().back()?;
            }
            Message::UpdateBack => {}
            Message::DismissKeyboard => {
                self.mounter.as_ref().unwrap().dismiss_keyboard()?;
            }
            Message::ScrollPosition { id, x, y, animated } => self
                .mounter
                .as_ref()
                .unwrap()
                .scroll_to(id, x, y, animated)?,
            Message::ReconcileNavigation => {
                self.mounter.as_mut().unwrap().reconcile_navigation()?;
            }
            Message::DialogResult { id, index } => {
                self.mounter.as_mut().unwrap().dialog_result(id, index)?;
            }
            Message::ClipboardProbe(reply) => {
                let result = ohos_pasteboard_binding::Pasteboard::new()
                    .has_type(ohos_udmf_binding::UdsPlainText::new())
                    .map_err(|error| error.to_string());
                let _ = reply.send(result);
            }
            Message::HostEnvironment(changes) => self.update_environment(changes)?,
            Message::HostWindow(visual, keyboard) => {
                let changes = self
                    .mounter
                    .as_ref()
                    .unwrap()
                    .window_environment(visual, keyboard)?;
                self.update_environment(changes)?;
            }
            Message::HostEvent(kind, payload) => self
                .engine
                .as_ref()
                .unwrap()
                .control()
                .dispatch_event(nsh_core::ROOT_ID, kind, payload),
            #[cfg(feature = "api-23")]
            Message::AccessibilityEvent(id, event) => {
                self.mounter
                    .as_ref()
                    .unwrap()
                    .accessibility_event(id, event)?;
            }
            Message::Image(ready) => self.mounter.as_ref().unwrap().image_ready(ready)?,
            Message::ImageProgress(progress) => {
                self.mounter.as_ref().unwrap().image_progress(progress)
            }
            Message::Batch(batch) => {
                self.pulse_frame_rate_vote();
                let mounter = self
                    .mounter
                    .as_mut()
                    .expect("running surface has a mounter");
                let result = mounter.apply(batch);
                self.retire()?;
                if let Err(error) = result {
                    if matches!(error, crate::render::node::Error::Unsupported(_)) {
                        log::warn!("OHOS mount resource skipped: {error}");
                    } else {
                        // Stop a partially applied generation. A new load can recover
                        // the instance; later batches cannot mutate a broken tree.
                        let cleanup = self.close();
                        if let Err(cleanup) = cleanup {
                            log::error!("OHOS failed-generation cleanup: {cleanup}");
                        }
                        return Err(error);
                    }
                }
                if self.mounter.as_mut().unwrap().frame()? {
                    self.request_frame()?;
                }
            }
            Message::Error(error) => {
                self.report(error);
            }
            Message::RequestFrame => self.request_frame()?,
            Message::Frame | Message::FrameSynced => {
                self.step_frame(matches!(message, Message::Frame))?;
                return Ok(());
            }
            Message::Reclaim(_)
            | Message::VideoRetired(_)
            | Message::VideoUi(..)
            | Message::Close
            | Message::HostClosed => unreachable!(),
        }
        self.update_back();
        Ok(())
    }
    fn is_active(&self) -> bool {
        self.running.get() && self.attached && self.visible && self.foreground
    }
    fn update_back(&self) {
        let active = self.is_active();
        if let Some(dispatcher) = self.dispatcher.upgrade() {
            dispatcher
                .active
                .store(active, std::sync::atomic::Ordering::Release);
            dispatcher.wants_back.store(
                active && self.mounter.as_ref().is_some_and(Mounter::wants_back),
                std::sync::atomic::Ordering::Release,
            );
        }
    }
    fn update_environment(&mut self, changes: NshEnvironment) -> Result<()> {
        let environment = changes.apply(self.environment)?;
        if let Some(engine) = &self.engine {
            engine.control().set_environment(environment);
        }
        self.environment = environment;
        if let Some(mounter) = &self.mounter {
            mounter.environment(environment)?;
        }
        Ok(())
    }
    fn report(&self, error: String) {
        self.errors.report(error);
    }
    fn pulse_frame_rate_vote(&self) {
        use std::sync::atomic::Ordering;
        if !self.is_active() {
            return;
        }
        // Keep the window-associated VSync active for ~33 ms (4 frames at 120 Hz)
        // across scroll batches and live ticks so LTPO stays at the preferred
        // refresh rate without waking the UI thread when idle.
        self.vsync_vote.remaining.store(4, Ordering::Release);
        if self
            .vsync_vote
            .armed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            arm_vsync_vote(&self.vsync, &self.vsync_vote);
        }
    }
    fn step_frame(&mut self, wake_engine: bool) -> Result<()> {
        self.frame_pending = false;
        if !self.is_active() {
            self.frame_wanted = true;
            self.update_back();
            return Ok(());
        }
        if self.engine.is_none() {
            self.update_back();
            return Ok(());
        }
        if wake_engine {
            self.engine.as_ref().unwrap().control().frame();
        }
        if self.mounter.as_mut().unwrap().frame()? || self.frame_wanted {
            self.request_frame()?;
        }
        self.retire()?;
        self.update_back();
        Ok(())
    }
    fn request_frame(&mut self) -> Result<()> {
        self.frame_wanted = true;
        if !self.is_active() {
            return Ok(());
        }
        self.pulse_frame_rate_vote();
        if !self.frame_pending {
            self.frame_wanted = false;
            let state = self.self_ref.clone();
            // Run UI-thread frame updates inside ArkUI's UIVsyncTask before
            // layout/render flush via OH_ArkUI_PostFrameCallback, falling back
            // to NativeVSync if the root node is not yet attached to a context.
            if self
                .parent
                .borrow()
                .post_frame_callback(move |_, _| {
                    let Some(state) = state.upgrade() else {
                        return;
                    };
                    if let Ok(mut state) = state.try_borrow_mut() {
                        if let Err(error) = state.step_frame(true) {
                            state.report(error.to_string());
                            log::error!("OHOS surface frame: {error}");
                        }
                    } else if let Some(dispatcher) =
                        state.try_borrow().ok().and_then(|s| s.dispatcher.upgrade())
                    {
                        dispatcher.send(Message::Frame);
                    }
                })
                .is_err()
            {
                self.schedule(Message::Frame)?;
            }
            self.frame_pending = true;
        }
        Ok(())
    }
    fn schedule(&self, message: Message) -> Result<()> {
        let dispatcher = self.dispatcher.upgrade();
        let control = matches!(message, Message::Frame)
            .then(|| self.engine.as_ref().map(|e| e.control()))
            .flatten();
        let mut message = Some(message);
        // NativeVSync is available on API 12. Its callback may run off the UI
        // thread, so it only posts a message and never accesses ArkUI state.
        let status = self.vsync.request_frame_once(move |_| {
            if let (Some(dispatcher), Some(message)) = (&dispatcher, message.take()) {
                let wake_engine = matches!(message, Message::Frame)
                    && dispatcher.active.load(std::sync::atomic::Ordering::Acquire);
                let message = if wake_engine {
                    Message::FrameSynced
                } else {
                    message
                };
                dispatcher.send(message);
                if wake_engine && let Some(control) = &control {
                    control.frame();
                }
            }
        });
        if status != 0 {
            return Err(Error::Vsync(status));
        }
        Ok(())
    }
    fn retire(&mut self) -> Result<()> {
        if let Some(epoch) = self.mounter.as_mut().and_then(Mounter::freeze_retired) {
            // Disposing within a layout/draw callback is unsafe. The frame
            // callback posts a new UI turn, and only that turn releases nodes.
            self.schedule(Message::Reclaim(epoch))?;
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.running.set(false);
        if let Some(dispatcher) = self.dispatcher.upgrade() {
            dispatcher
                .active
                .store(false, std::sync::atomic::Ordering::Release);
            dispatcher
                .wants_back
                .store(false, std::sync::atomic::Ordering::Release);
            dispatcher
                .live
                .store(false, std::sync::atomic::Ordering::Release);
        }
        self.engine.take();
        if let Some(mounter) = &mut self.mounter {
            mounter.close()?;
            self.retire()?;
        }
        Ok(())
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.engine.take();
        self.mounter.take();
    }
}

pub(crate) struct Runtime {
    running: Rc<Cell<bool>>,
    state: Rc<RefCell<State>>,
    _dispatcher: Arc<Dispatcher>,
}
pub(crate) struct App {
    pub(crate) owner: std::sync::Weak<crate::app::surface::Queue>,
    pub(crate) bundle: Bundle,
    pub(crate) assets: Arc<dyn nsh_engine::Assets>,
    pub(crate) chunks: Option<Arc<crate::app::chunks::ResourceChunks>>,
    #[cfg(feature = "dev")]
    pub(crate) reload: Option<(u64, std::sync::Weak<crate::app::surface::Queue>)>,
}
impl App {
    pub(crate) fn new(bundle: Bundle, assets: Arc<dyn nsh_engine::Assets>) -> Self {
        Self {
            owner: std::sync::Weak::new(),
            bundle,
            assets,
            chunks: None,
            #[cfg(feature = "dev")]
            reload: None,
        }
    }
}
impl Runtime {
    pub(crate) fn start(
        env: Env,
        parent: NativeNode,
        app: App,
        size: Size,
        config: &Config,
        components: crate::interop::component::Components,
        errors: Errors,
    ) -> napi_ohos::Result<Self> {
        config.host.check()?;
        let scale = config.host.scale()?;
        let vsync = Arc::new(
            config
                .host
                .window_id()
                .and_then(|id| Vsync::try_new_for_associated_window(id, "nsh"))
                .or_else(|| Vsync::try_new("nsh"))
                .ok_or_else(|| {
                    napi_ohos::Error::from_reason("failed to create OHOS native VSync")
                })?,
        );
        // Like CADisplayLink's preferred range, explicitly request the rendering
        // rate for the associated window; the display service selects a supported
        // rate. A surface still requests no callbacks while idle.
        let preferred = ohos_display_binding::default_display_refresh_rate().clamp(60, 144) as i32;
        let mut status = vsync.set_expected_frame_rate_range(
            ohos_vsync_binding::OH_NativeVSync_ExpectedRateRange {
                min: preferred,
                max: preferred,
                expected: preferred,
            },
        );
        if status != 0 {
            status = vsync.set_expected_frame_rate_range(
                ohos_vsync_binding::OH_NativeVSync_ExpectedRateRange {
                    min: 0,
                    max: preferred,
                    expected: preferred,
                },
            );
        }
        if status != 0 {
            return Err(napi_ohos::Error::from_reason(format!(
                "failed to configure OHOS native VSync rate: {status}"
            )));
        }
        let running = Rc::new(Cell::new(true));
        let vsync_vote = Arc::new(VsyncVote {
            remaining: std::sync::atomic::AtomicU8::new(0),
            armed: std::sync::atomic::AtomicBool::new(false),
        });
        // The shader thread paces itself on the surface's VSync too.
        let shader_vsync = vsync.clone();
        let state = Rc::new_cyclic(|weak| {
            RefCell::new(State {
                self_ref: weak.clone(),
                parent: parent.clone(),
                running: running.clone(),
                engine: None,
                mounter: None,
                dispatcher: Weak::new(),
                vsync,
                vsync_vote,
                errors,
                frame_pending: false,
                foreground: config.host.is_foreground(),
                attached: true,
                visible: true,
                frame_wanted: false,
                environment: Environment::default(),
                #[cfg(feature = "dev")]
                reload: app.reload,
                #[cfg(feature = "dev")]
                hot_version: None,
            })
        });
        let ui_state = state.clone();
        let wake = env.create_function_from_closure::<(), (), _>("nshCommit", |_| Ok(()))?;
        let dispatcher = Arc::new(Dispatcher {
            owner: app.owner,
            queue: wake
                .build_threadsafe_function::<Message>()
                .callee_handled::<false>()
                .build_callback(move |ctx| {
                    let mut state = ui_state.borrow_mut();
                    if let Err(error) = state.receive(ctx.value) {
                        state.report(error.to_string());
                        log::error!("OHOS surface: {error}");
                    }
                    Ok(())
                })?,
            wants_back: std::sync::atomic::AtomicBool::new(false),
            video_fullscreen: std::sync::atomic::AtomicBool::new(false),
            active: std::sync::atomic::AtomicBool::new(config.host.is_foreground()),
            status_bar: std::sync::Mutex::new(None),
            keyboard: std::sync::Mutex::default(),
            live: std::sync::atomic::AtomicBool::new(true),
        });
        state.borrow_mut().dispatcher = Arc::downgrade(&dispatcher);
        let environment = Environment {
            os: Os::Ohos,
            pixel_ratio: scale,
            window: size.into(),
            ..Environment::default()
        };
        let environment = config.host.environment().apply(environment)?;
        let environment = config.environment.clone().apply(environment)?;
        state.borrow_mut().environment = environment;
        let images = crate::media::image::Loader::new(
            app.assets.clone(),
            app.bundle.policy.access(),
            Arc::downgrade(&dispatcher),
        );
        let access = Arc::new(app.bundle.policy.access());
        let assets = app.assets.clone();
        let cache_root = config.host.video_cache_directory().ok();
        let data_dir = cache_root
            .as_ref()
            .and_then(|dir| dir.parent().map(|parent| parent.join("files/nsh")))
            .or_else(|| cache_root.as_ref().map(|dir| dir.join("nsh-data")));
        let mut cache = nsh_engine::CacheOptions::default();
        if let Some(dir) = &cache_root {
            cache.location = nsh_engine::CacheLocation::Dir(dir.join("nsh"));
        }
        let engine_config = EngineConfig {
            app: app.bundle.into(),
            viewport: size.into(),
            trace: false,
            network: Some(crate::services::net::network()),
            websocket: None,
            refuse_connections: None,
            filesystem: None,
            assets: Some(app.assets),
            environment,
            header_heights: (nsh_core::Points(52.0), nsh_core::Points(52.0)),
            limits: config.limits,
            ui_sets: vec![nsh_base::registry()],
            runtime: None,
            data_dir,
            cache,
            storage: None,
            clock: Default::default(),
            context: Default::default(),
            pinned: Default::default(),
            clipboard: None,
            host: config.channel.clone(),
            picker: None,
        };
        let host_tokens = config.host_tokens.clone();
        let engine = Engine::start_with(
            engine_config,
            move || OhosTextEngine::new(scale),
            Sink(dispatcher.clone(), app.chunks),
            move |control| {
                if let Some(tokens) = host_tokens {
                    control.set_tokens(Some(tokens));
                }
            },
        )
        .map_err(|error| napi_ohos::Error::from_reason(error.to_string()))?;
        images.set_disk_cache(engine.image_cache());
        let control = engine.control();
        let context = Rc::new(crate::render::mount::UiContext {
            shaders: crate::render::shader::Shaders::new(shader_vsync),
            fullscreen_video: RefCell::default(),
            scroll_reports: RefCell::default(),
            flushing_batch: std::cell::Cell::new(false),
            scene: RefCell::default(),
            compositor: std::cell::RefCell::default(),
            composite_nodes: std::cell::RefCell::default(),
            texture_posting: std::cell::Cell::new(false),
            selected_text: RefCell::default(),
            accessibility_dirty: RefCell::default(),
            accessibility_focus: RefCell::default(),
            accessibility_live_regions: std::cell::Cell::new(0),
            accessibility_wrappers: std::cell::Cell::new(0),
            leaving_nodes: std::cell::Cell::new(0),
            arkui: crate::render::arkui::empty_batch(),
            arkui_views: Default::default(),
            host: config.host.clone(),
            dispatcher: Arc::downgrade(&dispatcher),
            env,
            scale,
            control: control.clone(),
            animator: crate::render::animation::Animator::new(control, Arc::downgrade(&dispatcher)),
            images,
            access,
            assets,
            components,
            touches: RefCell::default(),
            canvas_bytes: Rc::new(Cell::new(0)),
            platform: Cell::new(environment),
            appearance: Cell::new(nsh_core::platform::Appearance::System),
        });
        let mounter = Mounter::new(parent, size, context).map_err(napi_error)?;
        state.borrow_mut().mounter = Some(mounter);
        state.borrow_mut().engine = Some(engine);
        let runtime = Self {
            running,
            state,
            _dispatcher: dispatcher,
        };
        // Keep rollback ownership before registering with the shared session.
        // A rejected route must still close the engine and detach its root.
        config.host.attach(Arc::downgrade(&runtime._dispatcher))?;
        Ok(runtime)
    }
    pub(crate) fn resize(&self, size: Size) -> napi_ohos::Result<()> {
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        if state.engine.is_none() {
            return Ok(());
        }
        state.environment.window = size.into();
        if let Some(mounter) = &state.mounter {
            mounter.resize(size).map_err(napi_error)?;
        }
        Ok(())
    }
    pub(crate) fn set_attached(&self, attached: bool) -> napi_ohos::Result<()> {
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        if state.engine.is_none() {
            return Ok(());
        }
        let changed = state.attached != attached;
        state.attached = attached;
        if let Some(mounter) = &state.mounter {
            if !attached {
                mounter.cancel_input().map_err(napi_error)?;
            }
            if changed {
                mounter.native_context(attached).map_err(napi_error)?;
            }
        }
        state.update_back();
        if state.is_active() && state.frame_wanted {
            state.request_frame().map_err(napi_error)?;
        }
        Ok(())
    }
    pub(crate) fn set_visible(&self, visible: bool) -> napi_ohos::Result<()> {
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        if state.engine.is_none() {
            return Ok(());
        }
        state.visible = visible;
        if !visible && let Some(mounter) = &state.mounter {
            mounter.cancel_input().map_err(napi_error)?;
        }
        state.update_back();
        if state.is_active() && state.frame_wanted {
            state.request_frame().map_err(napi_error)?;
        }
        Ok(())
    }
    #[cfg(feature = "dev")]
    pub(crate) fn hot_update(&self, update: nsh_engine::HotUpdate, version: (u64, u64)) -> bool {
        if let Ok(mut state) = self.state.try_borrow_mut()
            && state.engine.is_some()
        {
            state.hot_version = Some(version);
            state.engine.as_ref().unwrap().control().hot_update(update);
            return true;
        }
        false
    }
    pub(crate) fn set_environment(&self, changes: NshEnvironment) -> napi_ohos::Result<()> {
        let mut state = self.state.try_borrow_mut().map_err(reentry)?;
        if state.engine.is_none() {
            return Ok(());
        }
        state.update_environment(changes).map_err(napi_error)
    }
    pub(crate) fn set_tokens(&self, tokens: Option<nsh_engine::Tokens>) -> napi_ohos::Result<()> {
        let state = self.state.try_borrow().map_err(reentry)?;
        if let Some(engine) = &state.engine {
            engine.control().set_tokens(tokens);
        }
        Ok(())
    }
    pub(crate) fn is_running(&self) -> bool {
        self.running.get()
    }
    pub(crate) fn is_busy(&self) -> bool {
        self.state.try_borrow_mut().is_err()
    }
    pub(crate) fn close(&self) -> napi_ohos::Result<()> {
        self.state
            .try_borrow_mut()
            .map_err(reentry)?
            .close()
            .map_err(napi_error)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            log::error!("OHOS engine close: {error}");
            self._dispatcher.send(Message::Close);
        }
    }
}
fn napi_error(error: crate::render::node::Error) -> napi_ohos::Error {
    napi_ohos::Error::from_reason(error.to_string())
}
fn reentry(_: impl std::fmt::Display) -> napi_ohos::Error {
    napi_ohos::Error::from_reason("defer engine mutation until the next host turn")
}
