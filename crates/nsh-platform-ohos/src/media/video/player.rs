//! Every source owns a player and immutable callback generation on this worker.
use std::{
    ffi::{CStr, CString, c_void},
    os::fd::AsFd,
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use nsh_core::{EventKind, EventPayload, PayloadValue, VideoProps};
use nsh_engine::{Assets, EngineControl};
use ohos_avplayer_binding::{self as av, AvPlayer, InfoField, InfoRef};
use ohos_native_window_binding::NativeWindow;

use super::{Operation, UiState, source::SourceFile};
use crate::{
    app::{
        runtime::{Dispatcher, Message},
        session::Services,
    },
    render::node::{Node, Result},
};

struct Callbacks {
    sender: mpsc::Sender<Notice>,
    epoch: u64,
    generation: u64,
    position_queued: AtomicBool,
    wake: mpsc::SyncSender<Operation>,
}
struct Notice {
    epoch: u64,
    generation: u64,
    kind: NoticeKind,
}
enum NoticeKind {
    State(i32),
    Position,
    Seek,
    End,
    Buffer(i32),
    Error(String),
}
struct Playback {
    player: AvPlayer,
    _callbacks: Box<Callbacks>,
    _window: NativeWindow,
    // Surface replacement is not a buffer-retirement barrier. Keep the old
    // window references with the player until synchronous close succeeds.
    _retired_windows: Vec<NativeWindow>,
    _source: Option<SourceFile>,
    generation: u64,
    initialized: bool,
    prepared: bool,
    applied_rate: f32,
}
pub(super) struct Worker {
    host: Arc<Services>,
    route: Arc<Dispatcher>,
    control: EngineControl,
    node: u32,
    token: u64,
    assets: Arc<dyn Assets>,
    access: Arc<nsh_engine::Access>,
    agent: ureq::Agent,
    active: Arc<AtomicBool>,
    source_epoch: Arc<AtomicU64>,
    epoch: u64,
    props: Option<VideoProps>,
    paused: bool,
    window: Option<NativeWindow>,
    playback: Option<Playback>,
    generation: u64,
    ui_ready: bool,
    failed: bool,
    notice_send: mpsc::Sender<Notice>,
    notices: mpsc::Receiver<Notice>,
    ui: UiState,
    last_progress: Instant,
    progress_failed: bool,
    wake: mpsc::SyncSender<Operation>,
}
impl Worker {
    pub(super) fn new(
        owner: &Node,
        token: u64,
        active: Arc<AtomicBool>,
        source_epoch: Arc<AtomicU64>,
        wake: mpsc::SyncSender<Operation>,
    ) -> Result<Self> {
        let route = owner
            .dispatcher
            .upgrade()
            .ok_or_else(|| napi_ohos::Error::from_reason("Video UI dispatcher is closed"))?;
        let (notice_send, notices) = mpsc::channel();
        Ok(Self {
            host: owner.host.clone(),
            route,
            control: owner.control.clone(),
            node: owner.id,
            token,
            assets: owner.assets.clone(),
            access: owner.access.clone(),
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .max_redirects(0)
                .max_redirects_will_error(false)
                .timeout_connect(Some(Duration::from_secs(30)))
                .timeout_recv_response(Some(Duration::from_secs(60)))
                .build()
                .into(),
            active,
            source_epoch,
            epoch: 0,
            props: None,
            paused: true,
            window: None,
            playback: None,
            generation: 0,
            ui_ready: owner.host.has_ui_context(),
            failed: false,
            notice_send,
            notices,
            ui: UiState::default(),
            last_progress: Instant::now(),
            progress_failed: false,
            wake,
        })
    }
    fn live(&self) -> bool {
        self.active.load(Ordering::Acquire) && self.route.live.load(Ordering::Acquire)
    }
    fn load_live(&self) -> bool {
        self.live() && self.ui_ready && self.epoch == self.source_epoch.load(Ordering::Acquire)
    }
    pub(super) fn run(mut self, receiver: mpsc::Receiver<Operation>) {
        while self.live() && !self.failed {
            let result = match receiver.recv_timeout(self.next_progress_wait()) {
                Ok(Operation::Close) => break,
                Ok(operation) => self.operation(operation),
                Err(mpsc::RecvTimeoutError::Timeout) => Ok(()),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if let Err(error) = result
                && self.load_live()
            {
                self.error(error);
            }
            while let Ok(notice) = self.notices.try_recv() {
                if matches!(notice.kind, NoticeKind::Position)
                    && let Some(playback) = self
                        .playback
                        .as_ref()
                        .filter(|p| p.generation == notice.generation)
                {
                    playback
                        ._callbacks
                        .position_queued
                        .store(false, Ordering::Release);
                }
                if !self.accepts(&notice) {
                    continue;
                }
                if let Err(error) = self.notice(notice.kind)
                    && self.load_live()
                {
                    self.error(error);
                }
            }
            if self.load_live()
                && let Err(error) = self.progress(false)
            {
                self.error(error);
            }
        }
        if self.retire_player() && !self.failed {
            // This strong dispatcher remains alive through queued UI acknowledgement,
            // which State handles even after its engine has stopped.
            self.route.send(Message::VideoRetired(self.token));
        }
    }
    fn accepts(&self, notice: &Notice) -> bool {
        self.load_live()
            && notice.epoch == self.epoch
            && self
                .playback
                .as_ref()
                .is_some_and(|player| player.generation == notice.generation)
    }
    fn operation(&mut self, operation: Operation) -> std::result::Result<(), String> {
        match operation {
            Operation::Configure(props, epoch) => {
                if epoch != self.source_epoch.load(Ordering::Acquire) {
                    return Ok(());
                }
                let changed = self
                    .props
                    .as_ref()
                    .is_none_or(|old| old.source != props.source)
                    || self.epoch != epoch;
                self.epoch = epoch;
                validate_props(&props)?;
                if self
                    .props
                    .as_ref()
                    .is_none_or(|old| old.paused != props.paused)
                    || changed
                {
                    self.paused = props.paused;
                }
                self.props = Some(*props);
                if changed || self.playback.is_none() {
                    self.load()?;
                } else {
                    self.apply()?;
                }
            }
            Operation::Context(ready) => {
                self.ui_ready = ready;
                if !ready {
                    if !self.retire_player() {
                        return Err("Video context retirement failed".into());
                    }
                    self.window = None;
                    self.ui.playing = false;
                    self.publish_ui();
                } else if self.playback.is_none() {
                    self.load()?;
                } else {
                    self.apply()?;
                }
            }
            Operation::Window(window) => {
                let same =
                    self.window.as_ref().map(|w| w.raw()) == window.as_ref().map(|w| w.raw());
                if same {
                    return Ok(());
                }
                self.window = window;
                if self.playback.is_none() {
                    self.load()?;
                } else {
                    self.bind_window()?;
                }
            }
            Operation::WindowGone(raw) => {
                if self
                    .window
                    .as_ref()
                    .is_some_and(|window| window.raw() as usize == raw)
                {
                    // Reparenting may recreate the XComponent's surface. The
                    // same player keeps its position and paused/playing state
                    // while waiting for the replacement window.
                    self.window = None;
                }
            }
            Operation::Wake => {}
            Operation::Seek(seconds) => {
                let milliseconds = seek_time(seconds)?;
                if let Some(playback) = self.playback.as_mut().filter(|p| p.prepared) {
                    playback
                        .player
                        .seek(milliseconds, av::SeekMode::Closest)
                        .map_err(|e| e.to_string())?;
                }
            }
            Operation::Toggle => {
                // What the user means is the opposite of what they see: the
                // player's own state, not the glyph's last update.
                let playing = self
                    .playback
                    .as_mut()
                    .filter(|playback| playback.prepared)
                    .and_then(|playback| playback.player.state().ok())
                    .map_or(self.ui.playing, |state| state == av::STATE_PLAYING);
                self.paused = playing;
                self.apply()?;
            }
            Operation::Close => {}
        }
        Ok(())
    }
    fn bind_window(&mut self) -> std::result::Result<(), String> {
        let Some(window) = self.window.as_ref() else {
            return Ok(());
        };
        let Some(playback) = self.playback.as_mut() else {
            return Ok(());
        };
        if playback._window.raw() == window.raw() || (playback.initialized && !playback.prepared) {
            // SetVideoSurface cannot run during Prepare. Its prepared notice
            // applies the latest window before starting playback.
            return Ok(());
        }
        let raw = NonNull::new(window.raw().cast()).ok_or("Video window is null")?;
        // SAFETY: this worker owns the window reference; the complete UI owner
        // survives until player release acknowledgement, including reparenting.
        let window = unsafe { NativeWindow::retain_from_ptr(raw) }.map_err(|e| e.to_string())?;
        if playback.initialized {
            // SAFETY: both windows remain retained until synchronous close;
            // surface switching and player operations run only on this worker.
            if let Err(error) = unsafe { playback.player.set_video_surface(raw) } {
                playback._retired_windows.push(window);
                return Err(error.to_string());
            }
            playback
                ._retired_windows
                .push(std::mem::replace(&mut playback._window, window));
        } else {
            playback._window = window;
        }
        self.progress_failed = false;
        Ok(())
    }
    fn load(&mut self) -> std::result::Result<(), String> {
        if !self.retire_player() {
            return Err("Video player retirement failed; retaining native owners".into());
        }
        if !self.load_live() || self.window.is_none() {
            return Ok(());
        }
        let Some(source) = self.props.as_ref().and_then(|props| props.source.clone()) else {
            self.ui = UiState::default();
            self.publish_ui();
            return Ok(());
        };
        self.control
            .dispatch_event(self.node, EventKind::LoadStart, EventPayload::None);
        let _permit = self.host.reserve_load().map_err(|e| e.to_string())?;
        let (file, url) = self.read_source(&source)?;
        if !self.load_live() {
            return Ok(());
        }
        let player = AvPlayer::new().map_err(|e| e.to_string())?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("Video player generation exhausted")?;
        let mut callbacks = Box::new(Callbacks {
            sender: self.notice_send.clone(),
            epoch: self.epoch,
            generation: self.generation,
            position_queued: AtomicBool::new(false),
            wake: self.wake.clone(),
        });
        let userdata = std::ptr::from_mut(callbacks.as_mut()).cast::<c_void>();
        let window = self.window.as_ref().unwrap();
        let raw = NonNull::new(window.raw().cast()).ok_or("Video window is null")?;
        // SAFETY: this worker already owns a live reference, and the UI producer
        // remains retained until successful player release and its UI ack.
        let window = unsafe { NativeWindow::retain_from_ptr(raw) }.map_err(|e| e.to_string())?;
        let mut playback = Playback {
            player,
            _callbacks: callbacks,
            _window: window,
            _retired_windows: Vec::new(),
            _source: file,
            generation: self.generation,
            initialized: false,
            prepared: false,
            applied_rate: 1.0,
        };
        let configured = (|| {
            // SAFETY: immutable callback generation and Box storage survive until
            // successful explicit clear; callbacks only copy metadata and enqueue.
            unsafe { playback.player.set_info_callback(Some(info), userdata) }
                .map_err(|e| e.to_string())?;
            // SAFETY: same stable synchronized callback owner as info registration.
            unsafe { playback.player.set_error_callback(Some(error), userdata) }
                .map_err(|e| e.to_string())?;
            if let Some(url) = &url {
                playback.player.set_url(url).map_err(|e| e.to_string())?;
            } else if let Some(source) = &playback._source {
                let size =
                    i64::try_from(source.file().metadata().map_err(|e| e.to_string())?.len())
                        .map_err(|_| "Video source is too large")?;
                playback
                    .player
                    .set_fd(source.file().as_fd(), 0, size)
                    .map_err(|e| e.to_string())?;
            }
            if matches!(playback.player.state(), Ok(av::STATE_INITIALIZED)) {
                Self::initialize_playback(&mut playback)?;
            }
            Ok(())
        })();
        self.playback = Some(playback);
        if let Err(error) = configured {
            self.retire_player();
            return Err(error);
        }
        self.ui = UiState::default();
        self.publish_ui();
        Ok(())
    }
    fn initialize_playback(playback: &mut Playback) -> std::result::Result<(), String> {
        if playback.initialized {
            return Ok(());
        }
        playback.initialized = true;
        // SAFETY: playback owns its window reference and UI retains the producer
        // through successful synchronous close; release runs on this worker.
        unsafe {
            playback
                .player
                .set_video_surface(NonNull::new(playback._window.raw().cast()).unwrap())
        }
        .map_err(|e| e.to_string())?;
        playback.player.prepare().map_err(|e| e.to_string())
    }
    fn apply(&mut self) -> std::result::Result<(), String> {
        self.progress_failed = false;
        let Some(props) = self.props.as_ref() else {
            return Ok(());
        };
        let Some(playback) = self.playback.as_mut().filter(|p| p.prepared) else {
            return Ok(());
        };
        playback
            .player
            .volume(
                if props.muted { 0.0 } else { props.volume },
                if props.muted { 0.0 } else { props.volume },
            )
            .map_err(|e| e.to_string())?;
        playback
            .player
            .looping(props.repeat)
            .map_err(|e| e.to_string())?;
        if props.rate > 0.0 {
            let rate = props.rate.clamp(0.125, 4.0);
            if (rate - playback.applied_rate).abs() > 1e-4 {
                playback
                    .player
                    .playback_rate(rate)
                    .map_err(|e| e.to_string())?;
                playback.applied_rate = rate;
            }
        }
        let paused = self.paused || props.rate == 0.0 || !self.ui_ready;
        let state = playback.player.state().map_err(|e| e.to_string())?;
        if paused {
            if state == av::STATE_PLAYING {
                playback.player.pause().map_err(|e| e.to_string())?;
            }
        } else if state != av::STATE_PLAYING {
            playback.player.play().map_err(|e| e.to_string())?;
        }
        self.ui.playing = !paused;
        self.publish_ui();
        Ok(())
    }
    fn notice(&mut self, notice: NoticeKind) -> std::result::Result<(), String> {
        if matches!(notice, NoticeKind::State(_)) {
            self.progress_failed = false;
        }
        match notice {
            NoticeKind::State(state) if state == av::STATE_INITIALIZED as i32 => {
                if let Some(playback) = self.playback.as_mut() {
                    Self::initialize_playback(playback)?;
                }
            }
            NoticeKind::State(state) if state == av::STATE_PREPARED as i32 => {
                if let Some(playback) = self.playback.as_mut() {
                    playback.prepared = true;
                }
                self.bind_window()?;
                let playback = self.playback.as_mut().unwrap();
                self.ui.duration = playback
                    .player
                    .duration()
                    .map_err(|e| e.to_string())?
                    .max(0);
                (self.ui.width, self.ui.height) =
                    playback.player.video_size().map_err(|e| e.to_string())?;
                self.control.dispatch_event(
                    self.node,
                    EventKind::Load,
                    EventPayload::Map(vec![
                        (
                            "duration",
                            PayloadValue::Number(f64::from(self.ui.duration) / 1000.0),
                        ),
                        ("width", PayloadValue::Number(f64::from(self.ui.width))),
                        ("height", PayloadValue::Number(f64::from(self.ui.height))),
                    ]),
                );
                self.apply()?;
            }
            NoticeKind::State(state) => {
                self.ui.playing = state == av::STATE_PLAYING as i32;
                self.publish_ui();
            }
            NoticeKind::Position => {
                self.progress(false)?;
            }
            NoticeKind::Seek => {
                self.progress(true)?;
            }
            NoticeKind::End => {
                // With `repeat` the player starts over and keeps playing: only
                // the last run's end stops it.
                if !self.props.as_ref().is_some_and(|props| props.repeat) {
                    self.ui.playing = false;
                }
                self.control
                    .dispatch_event(self.node, EventKind::End, EventPayload::None);
                self.publish_ui();
            }
            NoticeKind::Buffer(kind)
                if kind == av::BUFFERING_START as i32 || kind == av::BUFFERING_END as i32 =>
            {
                self.control.dispatch_event(
                    self.node,
                    EventKind::Buffer,
                    EventPayload::Map(vec![(
                        "isBuffering",
                        PayloadValue::Bool(kind == av::BUFFERING_START as i32),
                    )]),
                );
            }
            NoticeKind::Buffer(_) => {}
            NoticeKind::Error(message) => {
                self.progress_failed = true;
                return Err(message);
            }
        }
        Ok(())
    }
    fn progress(&mut self, force: bool) -> std::result::Result<(), String> {
        if self.progress_failed && !force {
            return Ok(());
        }
        let Some(playback) = self.playback.as_mut().filter(|p| p.prepared) else {
            return Ok(());
        };
        let interval = self
            .props
            .as_ref()
            .map_or(250.0, |p| p.progress_interval_ms)
            .max(16.0);
        if !force
            && (!self.ui.playing
                || self.last_progress.elapsed()
                    < Duration::try_from_secs_f32(interval / 1000.0)
                        .map_err(|_| "Video progress interval is not representable")?)
        {
            return Ok(());
        }
        // Advance before querying native state; failures must not leave an expired
        // zero-duration deadline and spin the worker/error stream.
        self.last_progress = Instant::now();
        let queried = (|| -> std::result::Result<(), String> {
            self.ui.time = playback
                .player
                .current_time()
                .map_err(|e| e.to_string())?
                .max(0);
            self.ui.duration = playback
                .player
                .duration()
                .map_err(|e| e.to_string())?
                .max(0);
            (self.ui.width, self.ui.height) =
                playback.player.video_size().map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(error) = queried {
            self.progress_failed = true;
            return Err(error);
        }
        self.control.dispatch_event(
            self.node,
            EventKind::Progress,
            EventPayload::Map(vec![
                (
                    "currentTime",
                    PayloadValue::Number(f64::from(self.ui.time) / 1000.0),
                ),
                (
                    "duration",
                    PayloadValue::Number(f64::from(self.ui.duration) / 1000.0),
                ),
            ]),
        );
        self.publish_ui();
        Ok(())
    }
    fn next_progress_wait(&self) -> Duration {
        progress_wait(
            self.load_live(),
            self.playback.as_ref().is_some_and(|p| p.prepared),
            self.ui.playing,
            self.progress_failed,
            self.props
                .as_ref()
                .map_or(250.0, |p| p.progress_interval_ms),
            self.last_progress.elapsed(),
        )
    }
    fn publish_ui(&self) {
        self.route.send(Message::VideoUi(self.token, self.ui));
    }
    fn retire_player(&mut self) -> bool {
        self.ui.playing = false;
        if let Some(mut playback) = self.playback.take()
            && let Err(error) = playback.player.close_sync()
        {
            self.failed = true;
            self.error(format!("{error}; retaining Video native owners"));
            // The failed native release may still hold its producer and userdata.
            // Keep the complete group; absence of an ack also retains the UI owner.
            std::mem::forget(playback);
            return false;
        }
        true
    }
    fn error(&self, message: String) {
        self.control.dispatch_event(
            self.node,
            EventKind::Error,
            EventPayload::Map(vec![("message", PayloadValue::Str(message))]),
        );
    }
    fn read_source(
        &self,
        source: &str,
    ) -> std::result::Result<(Option<SourceFile>, Option<CString>), String> {
        if let Some(path) = source.strip_prefix("file://") {
            if !self.access.is_unrestricted() {
                return Err("local video files require an unrestricted host policy".into());
            }
            use std::os::unix::ffi::OsStringExt;
            let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(
                percent_encoding::percent_decode_str(path).collect(),
            ));
            if !path.is_absolute() {
                return Err("local video URL must have an absolute path".into());
            }
            return SourceFile::open(&path)
                .map(|file| (Some(file), None))
                .map_err(|e| e.to_string());
        }
        if !self.access.allows_media(source) {
            return Err("video URL is not granted by capabilities.network".into());
        }
        if source.starts_with("asset:/") {
            if let Some(file) = self.assets.open_file(source) {
                let file = file?;
                return SourceFile::from_file(file, std::path::PathBuf::new())
                    .map(|file| (Some(file), None))
                    .map_err(|e| e.to_string());
            }
            let directory = self
                .host
                .video_cache_directory()
                .map_err(|e| e.to_string())?;
            let reader = self.assets.reader(source)?;
            return SourceFile::copy(&directory, reader, || self.load_live())
                .map(|file| (Some(file), None))
                .map_err(|e| e.to_string());
        }
        if !source.starts_with("https://") && !source.starts_with("http://") {
            return Err("videos require asset:/, file:// or HTTP(S) URLs".into());
        }
        // Native streaming can follow redirects and HLS segment URLs. Restricted
        // capabilities retain Rust-checked redirects and a streaming disk cache.
        if self.access.is_unrestricted() {
            return CString::new(source)
                .map(|url| (None, Some(url)))
                .map_err(|e| e.to_string());
        }
        let proxy = crate::services::net::SystemProxy::read().map_err(|e| e.to_string())?;
        let mut url = source.to_owned();
        for hop in 0..=20 {
            if !self.load_live() {
                return Err("video load was cancelled".into());
            }
            let mut response = self
                .agent
                .get(&url)
                .config()
                .proxy(proxy.value())
                .build()
                .call()
                .map_err(|e| e.to_string())?;
            let status = response.status().as_u16();
            if matches!(status, 301 | 302 | 303 | 307 | 308)
                && let Some(location) = response.headers().get("location")
            {
                if hop == 20 {
                    return Err("too many video redirects".into());
                }
                url = self
                    .access
                    .media_redirect(&url, location.to_str().map_err(|e| e.to_string())?)?;
                continue;
            }
            if !(200..300).contains(&status) {
                return Err(format!("video HTTP status {status}"));
            }
            let directory = self
                .host
                .video_cache_directory()
                .map_err(|e| e.to_string())?;
            let reader = response.body_mut().with_config().limit(u64::MAX).reader();
            return SourceFile::copy(&directory, reader, || self.load_live())
                .map(|file| (Some(file), None))
                .map_err(|e| e.to_string());
        }
        Err("no video response".into())
    }
}
fn progress_wait(
    live: bool,
    prepared: bool,
    playing: bool,
    failed: bool,
    interval: f32,
    elapsed: Duration,
) -> Duration {
    if !live || !prepared || !playing || failed {
        return Duration::from_secs(60);
    }
    Duration::try_from_secs_f32(interval / 1000.0)
        .unwrap_or(Duration::from_secs(60))
        .saturating_sub(elapsed)
}
fn validate_props(props: &VideoProps) -> std::result::Result<(), String> {
    if !props.volume.is_finite()
        || !(0.0..=1.0).contains(&props.volume)
        || !props.rate.is_finite()
        || props.rate < 0.0
        || !props.progress_interval_ms.is_finite()
        || props.progress_interval_ms < 16.0
        || Duration::try_from_secs_f32(props.progress_interval_ms / 1000.0).is_err()
    {
        return Err("Invalid API20 video settings".into());
    }
    Ok(())
}
fn seek_time(seconds: f64) -> std::result::Result<i32, String> {
    let value = seconds * 1000.0;
    if !value.is_finite() || value < 0.0 || value > f64::from(i32::MAX) {
        return Err("Video seek must be a finite nonnegative native millisecond position".into());
    }
    Ok(value.round() as i32)
}
unsafe extern "C" fn info(
    _: *mut av::RawPlayer,
    kind: av::InfoType,
    body: *mut av::RawFormat,
    userdata: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: successful explicit clear drains native delivery before Box drop.
        let Some(callbacks) = (unsafe { userdata.cast::<Callbacks>().as_ref() }) else {
            return;
        };
        // SAFETY: metadata is borrowed only within this native callback; copy scalars now.
        let body = unsafe { InfoRef::from_raw(body) };
        let kind = match kind {
            av::INFO_STATE_CHANGE => body
                .as_ref()
                .and_then(|b| b.int(InfoField::State))
                .map(NoticeKind::State),
            av::INFO_POSITION => (!callbacks.position_queued.swap(true, Ordering::AcqRel))
                .then_some(NoticeKind::Position),
            av::INFO_SEEK_DONE => Some(NoticeKind::Seek),
            av::INFO_END => Some(NoticeKind::End),
            av::INFO_BUFFERING => body
                .as_ref()
                .and_then(|b| b.int(InfoField::BufferingType))
                .map(NoticeKind::Buffer),
            _ => None,
        };
        if let Some(kind) = kind {
            let _ = callbacks.sender.send(Notice {
                epoch: callbacks.epoch,
                generation: callbacks.generation,
                kind,
            });
            let _ = callbacks.wake.try_send(Operation::Wake);
        }
    }));
}
unsafe extern "C" fn error(
    _: *mut av::RawPlayer,
    code: i32,
    message: *const std::ffi::c_char,
    userdata: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: successful explicit clear drains delivery before userdata is freed.
        let Some(callbacks) = (unsafe { userdata.cast::<Callbacks>().as_ref() }) else {
            return;
        };
        let message = if message.is_null() {
            format!("AVPlayer error {code}")
        } else {
            // SAFETY: the SDK error string remains live and NUL terminated for this callback.
            unsafe { CStr::from_ptr(message) }
                .to_string_lossy()
                .into_owned()
        };
        let _ = callbacks.sender.send(Notice {
            epoch: callbacks.epoch,
            generation: callbacks.generation,
            kind: NoticeKind::Error(message),
        });
        let _ = callbacks.wake.try_send(Operation::Wake);
    }));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_load_or_progress_query_never_produces_a_busy_deadline() {
        assert_eq!(
            progress_wait(true, false, true, false, 16.0, Duration::from_secs(10)),
            Duration::from_secs(60)
        );
        assert_eq!(
            progress_wait(true, true, true, true, 16.0, Duration::from_secs(10)),
            Duration::from_secs(60)
        );
        assert_eq!(
            progress_wait(false, true, true, false, 16.0, Duration::from_secs(10)),
            Duration::from_secs(60)
        );
        assert_eq!(
            progress_wait(true, true, true, false, 250.0, Duration::from_millis(50)),
            Duration::from_millis(200)
        );
    }
    #[test]
    fn invalid_native_settings_are_errors() {
        let mut props = VideoProps::default();
        for interval in [f32::NAN, f32::INFINITY, f32::MAX, 0.0] {
            props.progress_interval_ms = interval;
            assert!(validate_props(&props).is_err());
        }
        props = VideoProps::default();
        props.volume = f32::NAN;
        assert!(validate_props(&props).is_err());
        props = VideoProps::default();
        props.rate = -1.0;
        assert!(validate_props(&props).is_err());
        for rate in [0.01, 0.3, 1.1, 3.0, 8.0] {
            props.rate = rate;
            assert!(validate_props(&props).is_ok());
        }
        assert!(validate_props(&VideoProps::default()).is_ok());
    }
    #[test]
    fn checked_seek_rejects_non_native_positions() {
        assert_eq!(seek_time(1.25).unwrap(), 1250);
        for seconds in [-1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            assert!(seek_time(seconds).is_err());
        }
    }
}
