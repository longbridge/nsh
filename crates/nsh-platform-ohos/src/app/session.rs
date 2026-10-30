//! Host session and system services. Each render attachment owns its
//! engine and reply queue; services retain only weak reply routes.

use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Arc, LazyLock, Mutex, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use napi_derive_ohos::napi;
use napi_ohos::{
    Env, Error, Result, Status,
    bindgen_prelude::FunctionRef,
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use nsh_core::{EventKind, EventPayload, PayloadValue};
use ohos_display_binding::default_display_scaled_density;
use ohos_pasteboard_binding::Pasteboard;
use ohos_resource_manager_binding::{RawFile, ResourceManager};
use ohos_udmf_binding::{UdmfData, UdmfRecord, UdsPlainText};
use rustc_hash::FxHashMap;

use crate::app::{
    environment::NshEnvironment,
    runtime::{Dispatcher, Message},
};

const MAX_RESOURCE: usize = 64 << 20;

static NEXT_SESSION_ID: AtomicU32 = AtomicU32::new(1);
static SESSIONS: LazyLock<Mutex<FxHashMap<u32, Weak<Services>>>> = LazyLock::new(Mutex::default);

struct UiSession {
    apply_batch: Option<FunctionRef<crate::interop::sdk::Batch, ()>>,
    surfaces: Vec<crate::app::surface::LifecycleRoute>,
}

thread_local! {
    static UI_SESSIONS: RefCell<FxHashMap<u32, UiSession>> = RefCell::default();
    static PENDING_SESSION: RefCell<Option<Arc<Services>>> = RefCell::default();
}

#[napi(object)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AvoidRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

#[napi(object)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AvoidArea {
    pub visible: bool,
    pub left_rect: AvoidRect,
    pub top_rect: AvoidRect,
    pub right_rect: AvoidRect,
    pub bottom_rect: AvoidRect,
}

#[napi(object)]
#[derive(Clone, Debug, Default)]
pub(crate) struct HostInit {
    pub sdk_api_version: i32,
    pub device_type: String,
    pub cache_dir: String,
    pub color_mode: i32,
    pub font_size_scale: f64,
    pub language: String,
    pub screen_reader: bool,
    pub reduce_motion: Option<bool>,
}

#[napi(object)]
#[derive(Clone, Debug, Default)]
pub(crate) struct HostCommand {
    pub kind: String,
    pub enabled: Option<bool>,
    pub light_content: Option<bool>,
    pub animated: Option<bool>,
    pub text: Option<String>,
    pub assertive: Option<bool>,
    pub announcement: Option<String>,
    pub element_id: Option<i64>,
    pub open_id: Option<u32>,
    pub limit: Option<u32>,
    pub url: Option<String>,
}

pub(crate) type HostCommandCallback =
    ThreadsafeFunction<HostCommand, (), HostCommand, Status, false>;

pub(crate) fn clear_pending() {
    PENDING_SESSION.with(|slot| {
        slot.borrow_mut().take();
    });
}

pub(crate) fn find_by_window_name(window_name: &str) -> u32 {
    if window_name.is_empty() {
        return 0;
    }
    let found = {
        let mut sessions = SESSIONS.lock().unwrap();
        sessions.retain(|_, weak| weak.strong_count() > 0);
        sessions.values().find_map(|weak| {
            let services = weak.upgrade()?;
            (services.window_name == window_name && services.check().is_ok()).then_some(services)
        })
    };
    if let Some(services) = found {
        let id = services.id;
        PENDING_SESSION.with(|slot| *slot.borrow_mut() = Some(services));
        id
    } else {
        0
    }
}

pub(crate) fn get(session_id: u32) -> Result<Arc<Services>> {
    let services = {
        let mut sessions = SESSIONS.lock().unwrap();
        sessions.retain(|_, weak| weak.strong_count() > 0);
        sessions
            .get(&session_id)
            .and_then(Weak::upgrade)
            .ok_or_else(|| Error::from_reason("OHOS host session is closed"))?
    };
    services.check()?;
    Ok(services)
}

pub(crate) fn take_pending_or_get(session_id: u32) -> Result<Arc<Services>> {
    if let Some(services) = PENDING_SESSION.with(|slot| slot.borrow_mut().take())
        && services.id == session_id
    {
        services.check()?;
        return Ok(services);
    }
    get(session_id)
}

pub(crate) fn any_active() -> Result<Arc<Services>> {
    let mut sessions = SESSIONS.lock().unwrap();
    sessions.retain(|_, weak| weak.strong_count() > 0);
    sessions
        .values()
        .filter_map(Weak::upgrade)
        .find(|services| services.check().is_ok())
        .ok_or_else(|| Error::from_reason("create an NshSurface or pass UIContext first"))
}

pub(crate) fn handle_back_any() -> bool {
    let candidates: Vec<Arc<Services>> = {
        let mut sessions = SESSIONS.lock().unwrap();
        sessions.retain(|_, weak| weak.strong_count() > 0);
        sessions
            .values()
            .filter_map(Weak::upgrade)
            .filter(|s| s.check().is_ok() && s.is_foreground())
            .collect()
    };
    candidates.into_iter().any(|host| host.handle_back())
}

pub(crate) fn close_ui_session(session_id: u32) {
    let ui_session = UI_SESSIONS.with(|sessions| sessions.borrow_mut().remove(&session_id));
    if let Some(ui_session) = ui_session {
        for surface in ui_session.surfaces {
            if let Err(error) = surface.dispose() {
                log::error!("OHOS session surface teardown: {error}");
            }
        }
    }
    let services = SESSIONS
        .lock()
        .unwrap()
        .remove(&session_id)
        .and_then(|weak| weak.upgrade());
    if let Some(services) = services {
        services.close_internal(false);
    }
}

enum Operation {
    #[cfg(feature = "dev")]
    DevCopy {
        route: Weak<crate::app::surface::Queue>,
        revision: u64,
        text: String,
    },
    Clipboard {
        route: Weak<Dispatcher>,
        text: Option<String>,
        request: Option<(u32, bool)>,
    },
    PickImages {
        route: Weak<Dispatcher>,
        id: u32,
        request: nsh_core::picker::PickRequest,
        paths: Vec<PathBuf>,
    },
    Stop,
}

type PendingPick = (u32, nsh_core::picker::PickRequest, Weak<Dispatcher>);

pub(crate) struct Services {
    id: u32,
    window_name: String,
    active: AtomicBool,
    foreground: AtomicBool,
    ui_ready: AtomicBool,
    loads: AtomicUsize,
    resource_lock: Mutex<()>,
    resource_manager: Arc<ResourceManager>,
    cache_dir: PathBuf,
    environment: Mutex<NshEnvironment>,
    routes: Mutex<Vec<Weak<Dispatcher>>>,
    focused: Mutex<Weak<Dispatcher>>,
    selected: Mutex<Weak<crate::app::surface::Queue>>,
    overlays: Mutex<Vec<(u64, Weak<Dispatcher>)>>,
    next_overlay: AtomicU64,
    keyboard: Mutex<Option<crate::input::keyboard::KeyboardWindow>>,
    window_id: AtomicU32,
    window_avoid: Mutex<Option<([AvoidArea; 3], AvoidArea)>>,
    command_tsfn: HostCommandCallback,
    next_open_id: AtomicU32,
    open_urls: Mutex<FxHashMap<u32, (u32, Weak<Dispatcher>)>>,
    pick_images: Mutex<FxHashMap<u32, PendingPick>>,
    queue: mpsc::SyncSender<Operation>,
}

pub(crate) struct Overlay {
    host: Weak<Services>,
    id: u64,
}
impl Drop for Overlay {
    fn drop(&mut self) {
        if let Some(host) = self.host.upgrade() {
            host.overlays
                .lock()
                .unwrap()
                .retain(|(id, _)| *id != self.id);
        }
    }
}
pub(crate) struct LoadPermit(Arc<Services>);
impl Drop for LoadPermit {
    fn drop(&mut self) {
        self.0.loads.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Services {
    pub(crate) fn window_id(&self) -> Option<u64> {
        let id = self.window_id.load(Ordering::Acquire);
        (id != 0).then_some(u64::from(id))
    }

    pub(crate) fn video_cache_directory(&self) -> Result<PathBuf> {
        self.check()?;
        if self.cache_dir.as_os_str().is_empty() {
            return Err(Error::from_reason(
                "OHOS host cache directory is not initialized",
            ));
        }
        Ok(self.cache_dir.clone())
    }

    pub(crate) fn apply_batch(
        &self,
        env: &Env,
        batch: crate::interop::sdk::Batch,
    ) -> crate::render::node::Result<()> {
        self.check()?;
        if !self.has_ui_context() {
            return Err(Error::from_reason("UI context is not ready").into());
        }
        UI_SESSIONS.with(|sessions| {
            let borrow = sessions.borrow();
            let applier = borrow
                .get(&self.id)
                .and_then(|slot| slot.apply_batch.as_ref())
                .ok_or_else(|| {
                    crate::render::node::Error::Host(Error::from_reason(
                        "ArkUI batch applier is not attached",
                    ))
                })?;
            let callback = applier.borrow_back(env)?;
            callback.call(batch)?;
            Ok(())
        })
    }

    pub(crate) fn register_surface(&self, route: crate::app::surface::LifecycleRoute) {
        UI_SESSIONS.with(|sessions| {
            if let Some(slot) = sessions.borrow_mut().get_mut(&self.id) {
                slot.surfaces.retain(|surface| surface.is_alive());
                slot.surfaces.push(route);
            }
        });
    }

    fn ensure_keyboard_filter(self: &Arc<Self>, id: i32) {
        if id <= 0 {
            return;
        }
        let mut filter = self.keyboard.lock().unwrap();
        if filter.as_ref().is_none_or(|filter| filter.id() != id) {
            filter.take();
            match crate::input::keyboard::KeyboardWindow::new(id, Arc::downgrade(self)) {
                Ok(window) => *filter = Some(window),
                Err(error) => self.broadcast(|| Message::Error(error.to_string())),
            }
        }
    }

    pub(crate) fn update_window(
        self: &Arc<Self>,
        id: i32,
        visual: [AvoidArea; 3],
        keyboard: AvoidArea,
    ) -> Result<()> {
        self.check()?;
        if !self.has_ui_context() {
            return Ok(());
        }
        if id <= 0 {
            return Err(Error::from_reason("platformWindowId must be positive"));
        }
        let current = self.window_id.load(Ordering::Acquire);
        if current > 0 && current != id as u32 {
            return Err(Error::from_reason(
                "resolved window differs from the session window",
            ));
        }
        self.window_id.store(id as u32, Ordering::Release);
        self.ensure_keyboard_filter(id);
        *self.window_avoid.lock().unwrap() = Some((visual, keyboard));
        self.broadcast(|| Message::HostWindow(visual, keyboard));
        Ok(())
    }

    pub(crate) fn has_ui_context(&self) -> bool {
        self.ui_ready.load(Ordering::Acquire)
    }

    pub(crate) fn is_foreground(&self) -> bool {
        self.foreground.load(Ordering::Acquire)
    }

    pub(crate) fn set_foreground(&self, active: bool) {
        if !active && let Some(keyboard) = self.keyboard.lock().unwrap().as_ref() {
            keyboard.clear();
        }
        self.foreground.store(active, Ordering::Release);
        self.broadcast(|| Message::Active(active));
    }

    pub(crate) fn activate(&self, route: Weak<Dispatcher>) {
        if let Some(route) = route.upgrade() {
            *self.selected.lock().unwrap() = route.owner.clone();
        }
        *self.focused.lock().unwrap() = route;
        self.apply_selected_status();
    }

    pub(crate) fn select_surface(&self, owner: Weak<crate::app::surface::Queue>) {
        *self.selected.lock().unwrap() = owner;
        *self.focused.lock().unwrap() = Weak::new();
        self.apply_selected_status();
    }

    fn selected_route(&self) -> Option<Arc<Dispatcher>> {
        let selected = self.selected.lock().unwrap().clone();
        self.routes
            .lock()
            .unwrap()
            .iter()
            .rev()
            .filter_map(Weak::upgrade)
            .find(|r| r.live.load(Ordering::Acquire) && Weak::ptr_eq(&r.owner, &selected))
    }

    pub(crate) fn keyboard_route(&self) -> Option<Arc<Dispatcher>> {
        if !self.overlays.lock().unwrap().is_empty() {
            return None;
        }
        self.selected_route()
    }

    pub(crate) fn modifiers(&self) -> nsh_core::keymap::Modifiers {
        self.keyboard
            .lock()
            .unwrap()
            .as_ref()
            .map_or_else(Default::default, |keyboard| keyboard.modifiers())
    }

    fn apply_selected_status(&self) {
        let Some(selected) = self.selected_route() else {
            return;
        };
        let settings = *selected.status_bar.lock().unwrap();
        let Some((config, dark)) = settings else {
            return;
        };
        let light_content = match config.style {
            nsh_core::StatusBarStyle::Default => dark,
            nsh_core::StatusBarStyle::Light => true,
            nsh_core::StatusBarStyle::Dark => false,
        };
        if let Err(error) = self.send_host_command(HostCommand {
            kind: "status-bar".into(),
            enabled: Some(!config.hidden && !selected.video_fullscreen.load(Ordering::Acquire)),
            light_content: Some(light_content),
            animated: Some(config.animated),
            ..Default::default()
        }) {
            selected.send(Message::Error(error.to_string()));
        }
    }

    pub(crate) fn status_bar(
        &self,
        route: Weak<Dispatcher>,
        config: nsh_core::StatusBarConfig,
        dark: bool,
    ) -> Result<()> {
        if let Some(route) = route.upgrade() {
            *route.status_bar.lock().unwrap() = Some((config, dark));
            self.apply_selected_status();
        }
        Ok(())
    }

    /// Native video controls own the window bars until fullscreen ends.
    pub(crate) fn video_fullscreen(&self, route: Weak<Dispatcher>, enabled: bool) -> Result<()> {
        if let Some(dispatcher) = route.upgrade() {
            dispatcher
                .video_fullscreen
                .store(enabled, Ordering::Release);
        }
        if enabled {
            self.activate(route);
        }
        let result = self.send_host_command(HostCommand {
            kind: "video-fullscreen".into(),
            enabled: Some(enabled),
            ..Default::default()
        });
        if !enabled {
            self.apply_selected_status();
        }
        result
    }

    pub(crate) fn accessibility_event(
        &self,
        route: Weak<Dispatcher>,
        announcement: Option<String>,
        element_id: Option<i64>,
    ) -> Result<()> {
        self.check()?;
        if route
            .upgrade()
            .is_none_or(|r| !r.live.load(Ordering::Acquire))
        {
            return Ok(());
        }
        self.send_host_command(HostCommand {
            kind: "accessibility-event".into(),
            announcement,
            element_id,
            ..Default::default()
        })
    }

    pub(crate) fn accessibility_live(
        &self,
        route: Weak<Dispatcher>,
        text: String,
        assertive: bool,
    ) -> Result<()> {
        self.check()?;
        if route
            .upgrade()
            .is_none_or(|r| !r.live.load(Ordering::Acquire))
        {
            return Ok(());
        }
        self.send_host_command(HostCommand {
            kind: "accessibility-live".into(),
            text: Some(text),
            assertive: Some(assertive),
            ..Default::default()
        })
    }

    fn send_host_command(&self, command: HostCommand) -> Result<()> {
        self.check()?;
        let status = self
            .command_tsfn
            .call(command, ThreadsafeFunctionCallMode::NonBlocking);
        if status != Status::Ok {
            return Err(Error::from_reason(format!(
                "OHOS host command dispatch failed: {status:?}"
            )));
        }
        Ok(())
    }

    pub(crate) fn overlay(self: &Arc<Self>, route: Weak<Dispatcher>) -> Result<Overlay> {
        let id = self
            .next_overlay
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| Error::from_reason("overlay generation exhausted"))?;
        self.overlays.lock().unwrap().push((id, route));
        Ok(Overlay {
            host: Arc::downgrade(self),
            id,
        })
    }

    pub(crate) fn handle_back(&self) -> bool {
        if !self.is_foreground() {
            return false;
        }
        let overlay = {
            let mut overlays = self.overlays.lock().unwrap();
            overlays.retain(|(_, route)| {
                route
                    .upgrade()
                    .is_some_and(|r| r.live.load(Ordering::Acquire))
            });
            overlays.last().and_then(|(_, route)| route.upgrade())
        };
        let mut route = overlay.or_else(|| {
            self.focused
                .lock()
                .unwrap()
                .upgrade()
                .filter(|r| r.live.load(Ordering::Acquire))
        });
        if route.is_none() {
            let selected = self.selected.lock().unwrap().clone();
            let has_selection = selected.strong_count() > 0;
            route = self
                .routes
                .lock()
                .unwrap()
                .iter()
                .rev()
                .filter_map(Weak::upgrade)
                .find(|r| {
                    r.live.load(Ordering::Acquire)
                        && (!has_selection || Weak::ptr_eq(&r.owner, &selected))
                });
        }
        if let Some(route) = route
            && route.wants_back.load(Ordering::Acquire)
        {
            route.send(Message::Back);
            return true;
        }
        false
    }

    pub(crate) fn reserve_load(self: &Arc<Self>) -> Result<LoadPermit> {
        self.check()?;
        if self.loads.fetch_add(1, Ordering::AcqRel) >= 32 {
            self.loads.fetch_sub(1, Ordering::AcqRel);
            return Err(Error::from_reason(
                "at most 32 resource loads may be pending per Ability",
            ));
        }
        Ok(LoadPermit(self.clone()))
    }

    pub(crate) fn scale(&self) -> Result<f32> {
        let scale = default_display_scaled_density();
        if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
            return Err(Error::from_reason(
                "host pixel ratio must be between 0.25 and 8",
            ));
        }
        Ok(scale)
    }

    pub(crate) fn create(
        init: HostInit,
        window_name: String,
        platform_window_id: i32,
        resource_manager: ResourceManager,
        apply_batch: FunctionRef<crate::interop::sdk::Batch, ()>,
        command_tsfn: HostCommandCallback,
    ) -> Result<u32> {
        if window_name.is_empty() {
            return Err(Error::from_reason("windowName must not be empty"));
        }
        if platform_window_id < 0 {
            return Err(Error::from_reason("platformWindowId must be nonnegative"));
        }
        let id = NEXT_SESSION_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::from_reason("OHOS host session IDs exhausted"))?;
        let environment = environment_from_init(&init);
        environment
            .clone()
            .apply(nsh_engine::Environment::default())?;
        let (queue, receiver) = mpsc::sync_channel(256);
        let services = Arc::new(Self {
            id,
            window_name,
            active: AtomicBool::new(true),
            foreground: AtomicBool::new(true),
            ui_ready: AtomicBool::new(true),
            loads: AtomicUsize::new(0),
            resource_lock: Mutex::new(()),
            resource_manager: Arc::new(resource_manager),
            cache_dir: PathBuf::from(init.cache_dir),
            environment: Mutex::new(environment),
            routes: Mutex::new(Vec::new()),
            focused: Mutex::new(Weak::new()),
            selected: Mutex::new(Weak::new()),
            overlays: Mutex::default(),
            next_overlay: AtomicU64::new(1),
            keyboard: Mutex::default(),
            window_id: AtomicU32::new(platform_window_id as u32),
            window_avoid: Mutex::default(),
            command_tsfn,
            next_open_id: AtomicU32::new(1),
            open_urls: Mutex::default(),
            pick_images: Mutex::default(),
            queue,
        });
        let weak = Arc::downgrade(&services);
        thread::Builder::new()
            .name(format!("nsh-ohos-host-{id}"))
            .spawn(move || {
                while let Ok(operation) = receiver.recv() {
                    if matches!(operation, Operation::Stop) {
                        break;
                    }
                    let Some(services) = weak.upgrade() else {
                        break;
                    };
                    if services.check().is_err() {
                        break;
                    }
                    match operation {
                        #[cfg(feature = "dev")]
                        Operation::DevCopy {
                            route,
                            revision,
                            text,
                        } => services.copy_development(route, revision, text),
                        Operation::Clipboard {
                            route,
                            text,
                            request,
                        } => services.clipboard(route, text, request),
                        Operation::PickImages {
                            route,
                            id,
                            request,
                            paths,
                        } => {
                            let mut raw = nsh_core::picker::read_files(&paths, &request);
                            for file in &mut raw {
                                if let Some((_, original)) = file.name.split_once("__")
                                    && !original.is_empty()
                                {
                                    file.name = original.to_owned();
                                }
                            }
                            let files = nsh_core::picker::finish(&request, raw);
                            for path in &paths {
                                let _ = std::fs::remove_file(path);
                            }
                            if services.check().is_ok()
                                && let Some(route) = route.upgrade()
                            {
                                route.send(Message::HostEvent(
                                    EventKind::Pick,
                                    EventPayload::Picked { id, files },
                                ));
                            }
                        }
                        Operation::Stop => unreachable!(),
                    }
                }
            })
            .map_err(|error| Error::from_reason(error.to_string()))?;
        if platform_window_id > 0 {
            services.ensure_keyboard_filter(platform_window_id);
        }
        UI_SESSIONS.with(|sessions| {
            sessions.borrow_mut().insert(
                id,
                UiSession {
                    apply_batch: Some(apply_batch),
                    surfaces: Vec::new(),
                },
            );
        });
        {
            let mut sessions = SESSIONS.lock().unwrap();
            sessions.retain(|_, weak| weak.strong_count() > 0);
            sessions.insert(id, Arc::downgrade(&services));
        }
        PENDING_SESSION.with(|slot| *slot.borrow_mut() = Some(services));
        Ok(id)
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.active.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(Error::from_reason("Ability session is closed"))
        }
    }

    pub(crate) fn environment(&self) -> NshEnvironment {
        self.environment.lock().unwrap().clone()
    }

    pub(crate) fn attach(&self, route: Weak<Dispatcher>) -> Result<()> {
        self.check()?;
        let mut routes = self.routes.lock().unwrap();
        routes.retain(|route| route.strong_count() > 0);
        routes.push(route.clone());
        drop(routes);
        let selected = self.selected.lock().unwrap().clone();
        if selected.strong_count() == 0
            || route
                .upgrade()
                .is_some_and(|r| Weak::ptr_eq(&r.owner, &selected))
        {
            self.activate(route.clone());
        }
        if let (Some(route), Some((visual, keyboard))) =
            (route.upgrade(), *self.window_avoid.lock().unwrap())
        {
            route.send(Message::HostWindow(visual, keyboard));
        }
        Ok(())
    }

    pub(crate) fn broadcast(&self, message: impl Fn() -> Message) {
        let routes: Vec<_> = {
            let mut routes = self.routes.lock().unwrap();
            routes.retain(|route| route.strong_count() > 0);
            routes.iter().filter_map(Weak::upgrade).collect()
        };
        for route in routes {
            route.send(message());
        }
    }

    pub(crate) fn update(&self, changes: NshEnvironment) -> Result<()> {
        self.check()?;
        let mut environment = self.environment.lock().unwrap();
        let snapshot = environment
            .clone()
            .apply(nsh_engine::Environment::default())?;
        *environment = NshEnvironment::snapshot(changes.clone().apply(snapshot)?);
        drop(environment);
        self.broadcast(|| Message::HostEnvironment(changes.clone()));
        Ok(())
    }

    fn close_internal(&self, notify_host: bool) {
        self.ui_ready.store(false, Ordering::Release);
        if self.active.swap(false, Ordering::AcqRel) {
            self.keyboard.lock().unwrap().take();
            self.open_urls.lock().unwrap().clear();
            self.pick_images.lock().unwrap().clear();
            self.broadcast(|| Message::HostClosed);
            let _ = self.queue.try_send(Operation::Stop);
            if notify_host {
                let _ = self.command_tsfn.call(
                    HostCommand {
                        kind: "close".into(),
                        ..Default::default()
                    },
                    ThreadsafeFunctionCallMode::NonBlocking,
                );
            }
        }
    }

    pub(crate) fn resource_reader(self: &Arc<Self>, name: &str) -> Result<Box<dyn std::io::Read>> {
        resource_name(name)?;
        self.check()?;
        let _lock = self.resource_lock.lock().unwrap();
        let file = self
            .resource_manager
            .open_raw_file(name)
            .map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Box::new(ResourceReader {
            file: Some(file),
            _manager: self.resource_manager.clone(),
            host: self.clone(),
        }))
    }

    pub(crate) fn resource(&self, name: &str) -> Result<Vec<u8>> {
        resource_name(name)?;
        self.check()?;
        // One manager is shared by every engine and image worker in this host.
        let _lock = self.resource_lock.lock().unwrap();
        self.check()?;
        let file = self
            .resource_manager
            .open_raw_file(name)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        let bytes = file
            .read_bounded(MAX_RESOURCE)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        self.check()?;
        Ok(bytes)
    }

    #[cfg(feature = "dev")]
    pub(crate) fn copy_development_error(
        &self,
        route: Weak<crate::app::surface::Queue>,
        revision: u64,
        text: String,
    ) -> Result<()> {
        self.enqueue(Operation::DevCopy {
            route,
            revision,
            text,
        })
    }

    #[cfg(feature = "dev")]
    fn copy_development(
        &self,
        route: Weak<crate::app::surface::Queue>,
        revision: u64,
        text: String,
    ) {
        if route.strong_count() == 0 {
            return;
        }
        let result = write_clipboard_text(&text).map_err(|error| error.to_string());
        if self.check().is_ok()
            && let Some(route) = route.upgrade()
        {
            route.call(
                crate::app::surface::Message::DevCopied { revision, result },
                ThreadsafeFunctionCallMode::NonBlocking,
            );
        }
    }

    pub(crate) fn enqueue_clipboard(
        &self,
        route: Weak<Dispatcher>,
        text: Option<String>,
        request: Option<(u32, bool)>,
    ) -> Result<()> {
        self.enqueue(Operation::Clipboard {
            route,
            text,
            request,
        })
    }

    pub(crate) fn open_url(&self, route: Weak<Dispatcher>, id: u32, url: String) -> Result<()> {
        self.check()?;
        let open_id = self
            .next_open_id
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::from_reason("openUrl request IDs exhausted"))?;
        self.open_urls
            .lock()
            .unwrap()
            .insert(open_id, (id, route.clone()));
        if let Err(error) = self.send_host_command(HostCommand {
            kind: "open-url".into(),
            open_id: Some(open_id),
            url: Some(url),
            ..Default::default()
        }) {
            self.open_urls.lock().unwrap().remove(&open_id);
            if let Some(route) = route.upgrade() {
                route.send(Message::HostEvent(
                    EventKind::OpenUrl,
                    EventPayload::Map(vec![
                        ("id", PayloadValue::Number(id as f64)),
                        ("opened", PayloadValue::Bool(false)),
                    ]),
                ));
            }
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn complete_open_url(&self, open_id: u32, opened: bool) {
        if self.check().is_err() {
            return;
        }
        if let Some((id, route)) = self.open_urls.lock().unwrap().remove(&open_id)
            && let Some(route) = route.upgrade()
        {
            route.send(Message::HostEvent(
                EventKind::OpenUrl,
                EventPayload::Map(vec![
                    ("id", PayloadValue::Number(id as f64)),
                    ("opened", PayloadValue::Bool(opened)),
                ]),
            ));
        }
    }

    pub(crate) fn pick_images(
        &self,
        route: Weak<Dispatcher>,
        id: u32,
        request: nsh_core::picker::PickRequest,
    ) -> Result<()> {
        self.check()?;
        let pick_id = self
            .next_open_id
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::from_reason("pickImages request IDs exhausted"))?;
        self.pick_images
            .lock()
            .unwrap()
            .insert(pick_id, (id, request, route.clone()));
        if let Err(error) = self.send_host_command(HostCommand {
            kind: "pick-images".into(),
            open_id: Some(pick_id),
            limit: Some(request.limit),
            ..Default::default()
        }) {
            self.pick_images.lock().unwrap().remove(&pick_id);
            if let Some(route) = route.upgrade() {
                route.send(Message::HostEvent(
                    EventKind::Pick,
                    EventPayload::Picked {
                        id,
                        files: Vec::new(),
                    },
                ));
            }
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn complete_pick_images(&self, pick_id: u32, paths: Vec<String>) {
        if self.check().is_err() {
            for path in &paths {
                let _ = std::fs::remove_file(path);
            }
            return;
        }
        let Some((id, request, route)) = self.pick_images.lock().unwrap().remove(&pick_id) else {
            for path in &paths {
                let _ = std::fs::remove_file(path);
            }
            return;
        };
        if paths.is_empty() {
            if let Some(route) = route.upgrade() {
                route.send(Message::HostEvent(
                    EventKind::Pick,
                    EventPayload::Picked {
                        id,
                        files: Vec::new(),
                    },
                ));
            }
            return;
        }
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        if self
            .enqueue(Operation::PickImages {
                route: route.clone(),
                id,
                request,
                paths,
            })
            .is_err()
            && let Some(route) = route.upgrade()
        {
            route.send(Message::HostEvent(
                EventKind::Pick,
                EventPayload::Picked {
                    id,
                    files: Vec::new(),
                },
            ));
        }
    }

    fn enqueue(&self, operation: Operation) -> Result<()> {
        self.check()?;
        self.queue
            .try_send(operation)
            .map_err(|_| Error::from_reason("Ability operation queue is full or closed"))
    }

    fn clipboard(
        &self,
        route: Weak<Dispatcher>,
        text: Option<String>,
        request: Option<(u32, bool)>,
    ) {
        if route.strong_count() == 0 {
            return;
        }
        let result = (|| {
            if let Some(text) = text {
                write_clipboard_text(&text)?;
            }
            let (has, text) = if request.is_some_and(|(_, has_only)| has_only) {
                let (reply, response) = mpsc::sync_channel(1);
                let dispatcher = route
                    .upgrade()
                    .ok_or_else(|| Error::from_reason("surface is closed"))?;
                dispatcher.send(Message::ClipboardProbe(reply));
                let has = response
                    .recv_timeout(Duration::from_secs(10))
                    .map_err(|_| {
                        Error::from_reason("clipboard presence query cancelled or timed out")
                    })?
                    .map_err(Error::from_reason)?;
                (has, String::new())
            } else if request.is_some() {
                let text = read_clipboard_text()?.unwrap_or_default();
                (!text.is_empty(), text)
            } else {
                (false, String::new())
            };
            Ok::<_, Error>((has, text))
        })();
        if self.check().is_err() {
            return;
        }
        if let Some(route) = route.upgrade() {
            if let Err(error) = &result {
                route.send(Message::Error(format!("clipboard: {error}")));
            }
            if let Some((id, _)) = request {
                let (has, text) = result.unwrap_or_default();
                route.send(Message::HostEvent(
                    EventKind::Clipboard,
                    EventPayload::Map(vec![
                        ("id", PayloadValue::Number(id as f64)),
                        ("has", PayloadValue::Bool(has)),
                        ("text", PayloadValue::Str(text)),
                    ]),
                ));
            }
        }
    }
}

fn write_clipboard_text(text: &str) -> Result<()> {
    if text.contains('\0') {
        return Err(Error::from_reason("clipboard text must not contain NUL"));
    }
    let board = Pasteboard::new();
    if text.is_empty() {
        return board
            .clear()
            .map_err(|error| Error::from_reason(error.to_string()));
    }
    let mut data = UdmfData::try_new().map_err(|error| Error::from_reason(error.to_string()))?;
    let record = UdmfRecord::try_new().map_err(|error| Error::from_reason(error.to_string()))?;
    let uds = UdsPlainText::new();
    uds.set_content(text)
        .map_err(|error| Error::from_reason(error.to_string()))?;
    record
        .add(uds.into())
        .map_err(|error| Error::from_reason(error.to_string()))?;
    data.add_record(&record)
        .map_err(|error| Error::from_reason(error.to_string()))?;
    board
        .set_data(&data)
        .map_err(|error| Error::from_reason(error.to_string()))
}

fn read_clipboard_text() -> Result<Option<String>> {
    let board = Pasteboard::new();
    if !board.has_data() {
        return Ok(None);
    }
    let mut data = board.data().map_err(|error| {
        Error::from_reason(format!(
            "native clipboard read failed: {error}; check READ_PASTEBOARD and signing profile ACL"
        ))
    })?;
    let records = data
        .records()
        .map_err(|error| Error::from_reason(error.to_string()))?;
    for record in records {
        if let Ok(plain) = record.plain_text()
            && let Ok(text) = plain.get_content()
        {
            return Ok(Some(text));
        }
    }
    Ok(None)
}

pub(crate) fn idiom(device_type: &str) -> &'static str {
    match device_type.trim() {
        "tablet" | "pad" => "pad",
        "2in1" | "pc" | "desktop" => "desktop",
        "tv" => "tv",
        _ => "phone",
    }
}

pub(crate) fn environment_from_config(
    color_mode: i32,
    font_size_scale: f64,
    language: String,
    device_type: Option<&str>,
) -> NshEnvironment {
    NshEnvironment {
        font_scale: (font_size_scale.is_finite() && font_size_scale > 0.0)
            .then_some(font_size_scale),
        color_scheme: match color_mode {
            0 => Some("dark".into()),
            1 => Some("light".into()),
            _ => None,
        },
        rtl: (!language.is_empty()).then(|| {
            matches!(
                language.split(['-', '_']).next(),
                Some("ar" | "he" | "fa" | "ur" | "ps" | "dv" | "yi")
            )
        }),
        idiom: device_type
            .filter(|v| !v.is_empty())
            .map(idiom)
            .or_else(|| ohos_device_binding::device_type().map(idiom))
            .map(str::to_owned),
        locale: (!language.is_empty()).then_some(language),
        ..NshEnvironment::default()
    }
}

fn environment_from_init(init: &HostInit) -> NshEnvironment {
    let mut env = environment_from_config(
        init.color_mode,
        init.font_size_scale,
        init.language.clone(),
        Some(&init.device_type),
    );
    env.api_level = u32::try_from(init.sdk_api_version).ok();
    env.screen_reader = Some(init.screen_reader);
    if crate::get_capabilities().api23 {
        env.reduce_motion = init.reduce_motion;
    }
    env
}

impl Drop for Services {
    fn drop(&mut self) {
        self.close_internal(true);
    }
}

pub(crate) fn resource_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains(['\0', '\\'])
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(Error::from_reason(
            "resource path must contain relative normal components",
        ));
    }
    Ok(())
}

// RawFile stays on the caller's IO worker. Every manager operation, including
// closing the file, shares the host's serialization lock.
struct ResourceReader {
    file: Option<RawFile>,
    _manager: Arc<ResourceManager>,
    host: Arc<Services>,
}
impl std::io::Read for ResourceReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.host.check().map_err(std::io::Error::other)?;
        let _lock = self.host.resource_lock.lock().unwrap();
        let file = self.file.as_ref().unwrap();
        let remaining = file.remain();
        if remaining < 0 {
            return Err(std::io::Error::other("invalid raw resource length"));
        }
        let count = buffer.len().min(remaining as usize).min(64 << 10);
        if count == 0 {
            return Ok(0);
        }
        let read = file
            .read_into(&mut buffer[..count])
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        if read == 0 {
            return Err(std::io::Error::other("raw resource read failed"));
        }
        Ok(read)
    }
}
impl Drop for ResourceReader {
    fn drop(&mut self) {
        let _lock = self.host.resource_lock.lock().unwrap();
        self.file.take();
    }
}
