//! The engine thread of one surface.
//!
//! The engine owns a [`nsh_core::Core`] on a dedicated thread and runs one
//! app ([`App`]): a Rust app's root component, or a JS bundle in a QuickJS
//! VM (`nsh_js::Host`, feature `js`). Each wake-up (input event, timer,
//! resize) runs the app's code (component tasks; JS and its microtasks),
//! applies what JS committed, lays out, and hands one [`MountBatch`] to the
//! platform through a [`MountSink`]. The platform thread never blocks on
//! the engine.

mod access;
pub mod perf;
mod script;
#[cfg(feature = "js")]
mod symbolicate;

use std::{io, rc::Rc, sync::Arc, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{
    ApplyError, Core, EdgeInsets, EventKind, EventPayload, ListBind, MountBatch, NodeId, Op,
    PayloadValue, Size, TextEngine, time::Instant,
};
pub use nsh_core::{
    access::{Access, Capabilities},
    js::Limits,
    platform::{
        ColorScheme, Environment, FetchEvent, FetchOp, HttpBody, HttpHead, HttpRequest, Idiom,
        Insets, Locale, Os,
    },
    ui::{Root, Theme},
};
#[cfg(feature = "js")]
pub use nsh_js::BYTECODE_VERSION;
/// Signed packages, release-key certificates, revocations and trust stores.
#[cfg(feature = "js")]
pub use nsh_package as package;
use rustc_hash::FxHashMap;

use crate::script::{NetworkQueue, RequestId, Script};

#[cfg(not(target_arch = "wasm32"))]
/// Stack size of the engine thread. QuickJS recursion (and deep Solid
/// component trees) needs more than the 512 KiB default of secondary
/// threads on Apple platforms.
const ENGINE_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Minimum time between idle cycle collections. QuickJS frees acyclic
/// garbage immediately; only cycles (such as detached node subtrees, whose
/// parent/child links reference each other) wait for the collector.
const IDLE_GC_INTERVAL: Duration = Duration::from_secs(1);

/// The app an engine runs.
#[derive(Debug, Clone)]
// One per surface, moved once into its engine: its size does not matter.
#[allow(clippy::large_enum_variant)]
pub enum App {
    /// A Rust app: its root component, mounted as the surface's content.
    Rust(Root),
    /// A JS app (feature `js`).
    Js(Bundle),
}

impl From<Bundle> for App {
    fn from(bundle: Bundle) -> Self {
        App::Js(bundle)
    }
}

impl From<Root> for App {
    fn from(root: Root) -> Self {
        App::Rust(root)
    }
}

/// An app bundle: its code plus, in development, a source map.
#[derive(Debug, Clone)]
pub struct Bundle {
    /// The code.
    pub code: BundleCode,
    /// Source map (JSON) used to show app source locations in stack traces.
    pub source_map: Option<Arc<str>>,
    /// What the code may do.
    pub policy: Policy,
    /// What relative request URLs resolve against (the page's URL on the
    /// web); `None`: they are invalid.
    pub base_url: Option<String>,
    /// The chunks of a split bundle that come with it (`import()` loads
    /// them without asking the platform); those missing are asked of
    /// [`MountSink::load_chunk`]. Shared by the bundle's clones.
    pub chunks: Arc<Vec<Chunk>>,
    /// The sandbox's hardening as bytecode (`nsh_js::compile_harden`),
    /// for a runtime without QuickJS' parser (the web runtime of published
    /// apps); `None`: compiled when the policy hardens. Set it with
    /// [`Bundle::with_harden_bytecode`].
    pub harden_bytecode: Option<Arc<Vec<u8>>>,
}

/// A chunk of a split bundle (`nsh_bundler::Chunk`): app code that
/// `import()` loads on demand.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Its id (`Screen.js`), as the bundle imports it.
    pub id: String,
    /// The code.
    pub code: BundleCode,
    /// Source map (JSON), for stack traces.
    pub source_map: Option<Arc<str>>,
}

impl Chunk {
    /// A chunk as JS source.
    pub fn source(id: impl Into<String>, code: String) -> Self {
        Self {
            id: id.into(),
            code: BundleCode::Source(code),
            source_map: None,
        }
    }

    /// A chunk as bytecode (`nsh_js::compile_chunk_bytecode`).
    ///
    /// # Safety
    ///
    /// As [`Bundle::bytecode`].
    pub unsafe fn bytecode(id: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            id: id.into(),
            code: BundleCode::Bytecode(bytes),
            source_map: None,
        }
    }

    /// Attaches a source map.
    pub fn with_source_map(mut self, map: impl Into<Arc<str>>) -> Self {
        self.source_map = Some(map.into());
        self
    }
}

/// What a bundle's code may do.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Run without `eval`, the function constructors and writable shared
    /// prototypes (see `nsh_js::HostOptions::harden`). On by default; off for
    /// development bundles.
    pub harden: bool,
    /// What the app declared (`nsh.json` `capabilities`); `None`: no
    /// declaration, everything.
    pub declared: Option<Capabilities>,
    /// What the embedding app grants at most; `None`: whatever the app
    /// declared.
    pub granted: Option<Capabilities>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            harden: true,
            declared: None,
            granted: None,
        }
    }
}

impl Policy {
    /// Development bundles: `eval` and friends stay (debugging); the
    /// capabilities are still enforced, so a missing one shows during
    /// development.
    pub fn development() -> Self {
        Self {
            harden: false,
            ..Self::default()
        }
    }

    /// What the app may reach under this policy.
    pub fn access(&self) -> Access {
        Access {
            declared: self.declared.clone().unwrap_or_else(Capabilities::all),
            granted: self.granted.clone(),
        }
    }
}

/// The code of a bundle.
#[derive(Debug, Clone)]
pub enum BundleCode {
    /// QuickJS bytecode from `nsh_js::compile_bytecode`.
    Bytecode(Vec<u8>),
    /// JS source; slower to start than bytecode.
    Source(String),
}

impl Bundle {
    /// A bytecode bundle.
    ///
    /// # Safety
    ///
    /// `bytes` must be bytecode from `nsh_js::compile_bytecode` for this
    /// runtime's `BYTECODE_VERSION`, from a trusted source: QuickJS does not
    /// validate bytecode, and loading crafted bytecode corrupts memory. Trusted
    /// are the app's own signed resources, a development server the developer
    /// pointed the host at, and verified packages ([`Bundle::from_package`]).
    pub unsafe fn bytecode(bytes: Vec<u8>) -> Self {
        Self {
            code: BundleCode::Bytecode(bytes),
            source_map: None,
            policy: Policy::default(),
            base_url: None,
            chunks: Arc::default(),
            harden_bytecode: None,
        }
    }

    /// A JS source bundle.
    pub fn source(code: String) -> Self {
        Self {
            code: BundleCode::Source(code),
            source_map: None,
            policy: Policy::default(),
            base_url: None,
            chunks: Arc::default(),
            harden_bytecode: None,
        }
    }

    /// Attaches a source map.
    pub fn with_source_map(mut self, map: impl Into<Arc<str>>) -> Self {
        self.source_map = Some(map.into());
        self
    }

    /// Sets the URL relative request URLs resolve against.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Hardens with `bytes` ([`Bundle::harden_bytecode`]).
    ///
    /// # Safety
    ///
    /// As [`Bundle::bytecode`]: `bytes` comes from `nsh_js::compile_harden`
    /// of this runtime's `BYTECODE_VERSION`, from a trusted source.
    pub unsafe fn with_harden_bytecode(mut self, bytes: Vec<u8>) -> Self {
        self.harden_bytecode = Some(Arc::new(bytes));
        self
    }

    /// Replaces the policy.
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// Adds chunks of a split bundle (see [`Bundle::chunks`]).
    pub fn with_chunks(mut self, chunks: impl IntoIterator<Item = Chunk>) -> Self {
        Arc::make_mut(&mut self.chunks).extend(chunks);
        self
    }

    /// Reads a JS source bundle written by `nsh build` (`bundle.js`), plus
    /// `bundle.js.map` and the app's `capabilities.json` next to it if
    /// present. Bytecode (`bundle.nshc`) is refused: files can be replaced, and
    /// only [`Bundle::read_trusted`] (the app's own resources) or a verified
    /// package may load bytecode.
    pub fn read(path: &std::path::Path) -> io::Result<Bundle> {
        if path.extension().is_some_and(|e| e == "nshc") {
            return Err(io::Error::other(
                "bytecode only loads from the app's own resources or a signed package (.nshpkg); load bundle.js",
            ));
        }
        // SAFETY: not bytecode.
        unsafe { Self::read_trusted(path) }
    }

    /// Like [`Bundle::read`], also reading `bundle.nshc` as bytecode.
    ///
    /// # Safety
    ///
    /// A `.nshc` file must be trusted as [`Bundle::bytecode`] requires (e.g.
    /// inside the app's own signed bundle).
    pub unsafe fn read_trusted(path: &std::path::Path) -> io::Result<Bundle> {
        let bytes = std::fs::read(path)?;
        let bundle = if path.extension().is_some_and(|e| e == "nshc") {
            // SAFETY: guaranteed by the caller.
            unsafe { Bundle::bytecode(bytes) }
        } else {
            Bundle::source(String::from_utf8(bytes).map_err(io::Error::other)?)
        };
        let mut bundle = match std::fs::read_to_string(path.with_file_name("bundle.js.map")) {
            Ok(map) if !map.is_empty() => bundle.with_source_map(map),
            _ => bundle,
        };
        if let Ok(json) = std::fs::read_to_string(path.with_file_name("capabilities.json")) {
            bundle.policy.declared = Some(parse_capabilities(&json).map_err(io::Error::other)?);
        }
        Ok(bundle)
    }
}

#[cfg(feature = "js")]
impl Bundle {
    /// The bundle of a verified package: its bytecode when it was compiled
    /// for this runtime (else its source), its source map and its declared
    /// capabilities.
    pub fn from_package(package: &nsh_package::VerifiedPackage) -> Result<Bundle, String> {
        let bundle = match package.bytecode() {
            // SAFETY: verified: signed by a certified release key, hashed,
            // and compiled for this runtime.
            Some(bytecode) => unsafe { Bundle::bytecode(bytecode.to_vec()) },
            None => Bundle::source(package.source.clone()),
        };
        let mut bundle = match &package.source_map {
            Some(map) => bundle.with_source_map(map.clone()),
            None => bundle,
        };
        bundle.policy.declared = package
            .capabilities
            .as_ref()
            .map(Capabilities::from_json)
            .transpose()?;
        Ok(bundle)
    }
}

/// A verified package, unpacked ([`load_package`]).
#[cfg(feature = "js")]
#[derive(Debug)]
pub struct LoadedPackage {
    /// Its bundle.
    pub bundle: Bundle,
    /// Where its assets are (`asset:/…`).
    pub assets: std::path::PathBuf,
    /// Its app.
    pub app: nsh_package::App,
}

/// Verifies the package `bytes` with `trust`, writes its assets under
/// `assets_root` (`<root>/<app id>/<package hash>/`, once per package) and
/// records its build (older builds of the app are refused from now on).
#[cfg(feature = "js")]
pub fn load_package(
    trust: &mut nsh_package::TrustStore,
    bytes: &[u8],
    assets_root: &std::path::Path,
) -> Result<LoadedPackage, String> {
    let package = trust
        .verify(bytes, BYTECODE_VERSION)
        .map_err(|e| e.to_string())?;
    let bundle = Bundle::from_package(&package)?;
    let dir = assets_root.join(&package.app.id).join(&package.sha256);
    let done = dir.join(".complete");
    if !done.exists() {
        let assets = dir.join("assets");
        for (path, data) in &package.assets {
            // Paths were checked (relative, no `..`) when the package was read.
            let dest = assets.join(path);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&dest, data).map_err(|e| format!("{}: {e}", dest.display()))?;
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(&done, b"").map_err(|e| e.to_string())?;
    }
    trust.accept(&package).map_err(|e| e.to_string())?;
    Ok(LoadedPackage {
        bundle,
        assets: dir.join("assets"),
        app: package.app,
    })
}

/// Parses capabilities in their JSON form (`capabilities.json`, the dev
/// protocol, an embedder's grant).
pub fn parse_capabilities(json: &str) -> Result<Capabilities, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("capabilities: {e}"))?;
    Capabilities::from_json(&value)
}

impl Bundle {
    /// The declared capabilities from their JSON form, if any.
    pub fn with_capabilities_json(mut self, json: Option<&str>) -> Result<Self, String> {
        self.policy.declared = json.map(parse_capabilities).transpose()?;
        Ok(self)
    }
}

/// Receives mount batches on the engine thread. Implementations forward
/// them to the UI thread.
pub trait MountSink: Send + 'static {
    /// Called once per commit, in order.
    fn submit(&self, batch: MountBatch);

    /// A fatal app error: the bundle failed to load, or an exception escaped
    /// a JS callback. Errors are always logged; development hosts also show
    /// them on screen.
    fn report_error(&self, _message: &str) {}

    /// JS or a component's task asked for a display frame
    /// (`requestAnimationFrame`, `AsyncCx::next_frame`): call
    /// [`EngineControl::frame`] at the next vsync. Requests are not repeated
    /// until that call.
    fn request_frame(&self) {}

    /// A development update ([`EngineControl::hot_update`]) could not be
    /// applied in place: reload the latest full bundle.
    fn hot_update_failed(&self) {}

    /// The engine laid the app out at `size` ([`EngineControl::resize`]):
    /// called after the commit of the turn that applied the resize (if it
    /// changed anything, its batch came before), once per turn, with the
    /// latest size. A platform that waits for the layout at a new size, to
    /// show it in the frame the window takes the size, stops waiting here.
    fn resized(&self, _size: Size) {}

    /// The app imports chunk `id` of a split bundle (`import()`), which its
    /// [`Bundle`] does not hold: load it and hand it over with
    /// [`EngineControl::chunk_loaded`] (or [`EngineControl::chunk_failed`]),
    /// from any thread, and return true. False (the default): this host
    /// has no chunks to load, and the import fails.
    fn load_chunk(&self, _id: &str) -> bool {
        false
    }
}

/// Receives what happens to a request started by [`Network::fetch`], in
/// order, on any thread. After [`FetchTask::cancel`] it may still be called
/// (the engine then ignores it).
pub type FetchEvents = Box<dyn FnMut(FetchEvent) + Send>;

/// A request in flight, driven from the engine thread.
pub trait FetchTask: Send {
    /// The next chunk of a streamed body ([`HttpBody::Stream`]), after
    /// [`FetchEvent::BodyWanted`].
    fn send(&mut self, chunk: Vec<u8>);
    /// A streamed body is complete.
    fn finish(&mut self);
    /// Stops (`true`) or resumes (`false`) reading the response body: JS
    /// does not read what arrived (back pressure). Bytes already on their
    /// way may still arrive.
    fn pause(&mut self, paused: bool);
    /// Aborts the request.
    fn cancel(self: Box<Self>);
}

/// The platform's HTTP stack, used by JS `fetch`.
pub trait Network: Send + Sync + 'static {
    /// Starts `request`: its response head, then its body in chunks as they
    /// arrive, are reported to `events`.
    fn fetch(&self, request: HttpRequest, events: FetchEvents) -> Box<dyn FetchTask>;
}

/// Reads the app's bundled files (`asset:/…` URIs): from the app's
/// resources, or from the dev server during development.
pub trait Assets: Send + Sync + 'static {
    /// The bytes of `uri`. Blocking; called on the engine thread.
    fn read(&self, uri: &str) -> Result<Vec<u8>, String>;
}

/// Engine settings.
#[derive(Clone)]
pub struct EngineConfig {
    /// The app.
    pub app: App,
    /// Initial size of the root view.
    pub viewport: Size,
    /// Log per-commit timings (target `nsh::perf`, level info).
    pub trace: bool,
    /// The HTTP stack behind `fetch`; without one, requests fail.
    pub network: Option<Arc<dyn Network>>,
    /// Source of bundled files such as fonts; without one, they fail to load.
    pub assets: Option<Arc<dyn Assets>>,
    /// The surface's initial environment (its `window` is `viewport`).
    pub environment: Environment,
    /// Height of the platform's screen header bar, normal and with a large
    /// title (status bar excluded).
    pub header_heights: (f32, f32),
    /// The JS VM's resource limits (a JS app's; Rust code has none).
    pub limits: Limits,
    /// The UI sets JS imports (`nsh_ui_mobile::registry()` on mobile, the
    /// app's own): their Rust components and modules. A Rust app's
    /// components need no registration.
    pub ui_sets: Vec<nsh_core::ui::Registry>,
    /// The tokio runtime the components' async tasks use (timers, IO,
    /// background work); `None`: one shared by the engines of the process.
    pub runtime: Option<tokio::runtime::Handle>,
}

#[cfg(not(target_arch = "wasm32"))]
/// The runtime of components' tasks when the embedder gives none: a few
/// threads, made on first use, shared by every engine (like a thread pool,
/// it holds no engine's state).
fn shared_runtime() -> tokio::runtime::Handle {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("nsh-tasks")
                .enable_all()
                .build()
                .expect("the tasks' runtime starts")
        })
        .handle()
        .clone()
}

impl std::fmt::Debug for EngineConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineConfig")
            .field("app", &self.app)
            .field("viewport", &self.viewport)
            .field("trace", &self.trace)
            .field("network", &self.network.is_some())
            .field("assets", &self.assets.is_some())
            .field("environment", &self.environment)
            .field("header_heights", &self.header_heights)
            .field("limits", &self.limits)
            .field("ui_sets", &self.ui_sets)
            .finish()
    }
}

enum Message {
    Event(NodeId, EventKind, EventPayload),
    Scroll(NodeId, f32, f32, u32),
    Zoom(NodeId, f32),
    ComponentSize(NodeId, Size),
    AnimationsFinished(Vec<nsh_animation::Finished>),
    Resize(Size),
    /// A component's task was woken: it runs in the loop.
    Wake,
    Environment(Environment),
    RootSize(NodeId, Size, EdgeInsets),
    ScreenHeader(NodeId, Option<f32>),
    Frame,
    States(NodeId, u8),
    FontsChanged,
    /// An event of a request (by the hop it is for: events of a hop the
    /// engine left are dropped).
    Fetch(RequestId, u64, FetchEvent),
    Hot(HotUpdate),
    /// A chunk [`MountSink::load_chunk`] was asked for, or why it could
    /// not be loaded (its id and the message).
    Chunk(Result<Chunk, (String, String)>),
    Perf(Option<Arc<perf::PerfMonitor>>),
    /// Sent by a dropped [`Engine`] (there is none in WebAssembly).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Shutdown,
}

/// A development update that replaces the app's changed modules in place,
/// keeping the JS context, the views and the state of everything else
/// (made by `nsh_bundler::HotBundle::patch`).
#[derive(Debug, Clone)]
pub struct HotUpdate {
    /// Script name in stack traces, unique per update (e.g. `hot-3.js`).
    pub name: String,
    /// The script.
    pub code: String,
    /// Its source map (JSON).
    pub source_map: Option<Arc<str>>,
}

/// Rounds of "list asks for items -> JS renders them -> layout" folded into
/// one frame. Bounded so a list that never settles cannot stall the engine;
/// leftovers continue next frame.
const MAX_LIST_ROUNDS: usize = 8;

/// A cheap, cloneable handle for sending input to the engine from any thread.
#[derive(Clone)]
pub struct EngineControl {
    tx: Sender<Message>,
}

impl EngineControl {
    /// Delivers an input event (and its data) to a node.
    pub fn dispatch_event(&self, id: NodeId, event: EventKind, payload: EventPayload) {
        let _ = self.tx.send(Message::Event(id, event, payload));
    }

    /// Reports the content offset of a scroll view or list, and how many
    /// scrolls the core asked for (`MountItem::ScrollBy` items and
    /// `Command::ScrollTo` commands for the view) were applied before it.
    /// Cheap to call on every scroll frame: offsets are coalesced per
    /// engine frame.
    pub fn scroll(&self, id: NodeId, x: f32, y: f32, applied: u32) {
        let _ = self.tx.send(Message::Scroll(id, x, y, applied));
    }

    /// Reports the zoom scale of a zoomable scroll view (before the offset
    /// the zoom changed); `onScroll` carries it.
    pub fn zoom(&self, id: NodeId, scale: f32) {
        let _ = self.tx.send(Message::Zoom(id, scale));
    }

    /// A registered native component's own size (points): it is laid out at
    /// it where its style leaves a dimension open.
    pub fn set_component_size(&self, id: NodeId, size: Size) {
        let _ = self.tx.send(Message::ComponentSize(id, size));
    }

    /// Reports native animations that finished or were interrupted.
    pub fn animations_finished(&self, finished: Vec<nsh_animation::Finished>) {
        let _ = self.tx.send(Message::AnimationsFinished(finished));
    }

    /// Resizes the root view.
    pub fn resize(&self, size: Size) {
        let _ = self.tx.send(Message::Resize(size));
    }

    /// Reports a change of the surface's environment (safe area, keyboard,
    /// appearance...).
    pub fn set_environment(&self, environment: Environment) {
        let _ = self.tx.send(Message::Environment(environment));
    }

    /// The platform presents root `id` (a modal) at `size`, with `insets`
    /// of it covered by system UI.
    pub fn set_root_size(&self, id: NodeId, size: Size, insets: EdgeInsets) {
        let _ = self.tx.send(Message::RootSize(id, size, insets));
    }

    /// The platform measured where the content of screen `id` starts below
    /// its header (`None`: let the core estimate it).
    pub fn set_screen_header(&self, id: NodeId, height: Option<f32>) {
        let _ = self.tx.send(Message::ScreenHeader(id, height));
    }

    /// Applies a development update in place. When it cannot be applied,
    /// [`MountSink::hot_update_failed`] asks the platform to reload.
    pub fn hot_update(&self, update: HotUpdate) {
        let _ = self.tx.send(Message::Hot(update));
    }

    /// Hands over the chunk [`MountSink::load_chunk`] asked for: the
    /// dynamic imports waiting for it continue.
    pub fn chunk_loaded(&self, chunk: Chunk) {
        let _ = self.tx.send(Message::Chunk(Ok(chunk)));
    }

    /// Chunk `id` could not be loaded: the dynamic imports waiting for it
    /// fail with `message` (a later import asks for it again).
    pub fn chunk_failed(&self, id: String, message: String) {
        let _ = self.tx.send(Message::Chunk(Err((id, message))));
    }

    /// A display frame began, after [`MountSink::request_frame`]: runs the
    /// `requestAnimationFrame` callbacks and the tasks waiting for it.
    pub fn frame(&self) {
        let _ = self.tx.send(Message::Frame);
    }

    /// Reports the interaction states of view `id` that the core asked to
    /// track ([`nsh_core::MountItem::TrackStates`]; bits of
    /// [`nsh_core::tailwind::state`]). Class styles that depend on them
    /// (`active:`, `hover:`, `focus:`) update without JS.
    pub fn set_states(&self, id: NodeId, states: u8) {
        let _ = self.tx.send(Message::States(id, states));
    }

    /// Fonts the platform loads asynchronously became usable (on the web,
    /// a `FontFace` finished loading): text measured with a fallback font
    /// is measured again.
    pub fn fonts_changed(&self) {
        let _ = self.tx.send(Message::FontsChanged);
    }

    /// Records the engine's commits into `monitor` from now on (`None`:
    /// stops). Measuring costs nothing while no monitor is installed.
    pub fn set_perf_monitor(&self, monitor: Option<Arc<perf::PerfMonitor>>) {
        let _ = self.tx.send(Message::Perf(monitor));
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// A running engine. Dropping it stops the thread.
pub struct Engine {
    control: EngineControl,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Interrupts running JS when the engine stops.
    stop: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Engine {
    /// Starts the engine thread. `make_text_engine` runs on that thread.
    pub fn start<M, F, S>(config: EngineConfig, make_text_engine: F, sink: S) -> io::Result<Engine>
    where
        M: TextEngine + 'static,
        F: FnOnce() -> M + Send + 'static,
        S: MountSink,
    {
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine_tx = tx.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let engine_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("nsh-engine".into())
            .stack_size(ENGINE_STACK_SIZE)
            .spawn(move || run(config, make_text_engine(), sink, engine_tx, rx, engine_stop))?;
        Ok(Engine {
            control: EngineControl { tx },
            thread: Some(thread),
            stop,
        })
    }

    /// A handle for sending input from other threads.
    pub fn control(&self) -> EngineControl {
        self.control.clone()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Engine {
    fn drop(&mut self) {
        // Dropped on the platform's main thread: a script still running is
        // aborted rather than waited for.
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.control.tx.send(Message::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// The engine thread turns input into frames: it is as latency-critical as
/// the UI thread. Without this, Apple platforms may run it on efficiency
/// cores at low clock speeds.
fn raise_thread_priority() {
    #[cfg(target_vendor = "apple")]
    // SAFETY: changes the QoS class of the calling thread only.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run<M: TextEngine + 'static, S: MountSink>(
    config: EngineConfig,
    text_engine: M,
    sink: S,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    raise_thread_priority();
    let Some(mut driver) = Driver::start(config, text_engine, Rc::new(sink), tx, stop) else {
        return;
    };
    loop {
        let message = match driver.deadline() {
            Some(deadline) => match rx.recv_deadline(deadline) {
                Ok(message) => Some(message),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            },
            None => match rx.recv() {
                Ok(message) => Some(message),
                Err(_) => return,
            },
        };
        let started = driver.state.perf.is_some().then(Instant::now);
        if let Some(message) = message {
            if !driver.handle(message) {
                return;
            }
            // Coalesce everything already queued into this frame.
            while let Ok(message) = rx.try_recv() {
                if !driver.handle(message) {
                    return;
                }
            }
        }
        driver.turn();
        driver.state.record_perf(started);
        if rx.is_empty() {
            driver.collect_if_idle();
        }
    }
}

/// An engine driven by its host's event loop instead of a thread of its
/// own: the browser's main thread in WebAssembly, where there are no
/// threads to block. Input sent through its [`EngineControl`] is handled
/// at the next [`LocalEngine::pump`].
pub struct LocalEngine<M: TextEngine + 'static> {
    driver: Driver<M>,
    rx: Receiver<Message>,
    control: EngineControl,
}

impl<M: TextEngine + 'static> LocalEngine<M> {
    /// Starts the app (its first frame goes to `sink` before this returns);
    /// `None` if it could not start (reported to `sink`).
    pub fn start<S: MountSink>(config: EngineConfig, text_engine: M, sink: S) -> Option<Self> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let control = EngineControl { tx: tx.clone() };
        let driver = Driver::start(config, text_engine, Rc::new(sink), tx, stop)?;
        Some(LocalEngine {
            driver,
            rx,
            control,
        })
    }

    /// A handle for sending input.
    pub fn control(&self) -> EngineControl {
        self.control.clone()
    }

    /// Handles the input sent since the last call, runs what is due
    /// (timers, tasks) and commits. Returns when to call again at the
    /// latest (sooner if input arrives); `None`: only for input.
    pub fn pump(&mut self) -> Option<Instant> {
        // Bounded: messages sent while handling (a task waking another)
        // are handled in the same call, a loop that never settles is not.
        for _ in 0..4 {
            let started = self.driver.state.perf.is_some().then(Instant::now);
            while let Ok(message) = self.rx.try_recv() {
                self.driver.handle(message);
            }
            self.driver.turn();
            self.driver.state.record_perf(started);
            if self.rx.is_empty() {
                break;
            }
        }
        self.driver.collect_if_idle();
        self.driver.deadline()
    }
}

/// One engine's state and the work of one wake-up, whoever drives it.
struct Driver<M: TextEngine + 'static> {
    state: State<Rc<M>>,
    script: Option<Script>,
    sink: Rc<dyn MountSink>,
    assets: Option<Arc<dyn Assets>>,
    /// A VM stopped for good (out of memory, over a quota) is reported
    /// once; its views stay as they were and no JS runs any more.
    stopped: bool,
    last_gc: Instant,
}

impl<M: TextEngine + 'static> Driver<M> {
    /// Sets up the core, starts the app and submits its first frame.
    fn start(
        config: EngineConfig,
        text_engine: M,
        sink: Rc<dyn MountSink>,
        tx: Sender<Message>,
        stop: Arc<std::sync::atomic::AtomicBool>,
    ) -> Option<Self> {
        let text_engine = Rc::new(text_engine);
        let access = Arc::new(match &config.app {
            App::Js(bundle) => bundle.policy.access(),
            // Native code is trusted: it reaches what the platform allows.
            App::Rust(_) => Policy::default().access(),
        });
        let mut core = Core::new(text_engine.clone(), config.viewport);
        core.set_components(nsh_core::ui::Registry::merged(&config.ui_sets));
        if let App::Js(_) = &config.app {
            // Only JS is limited (Rust code is as trusted as the platform).
            core.set_max_nodes(config.limits.max_nodes);
        }
        core.set_access(access.clone());
        core.set_font_scale(config.environment.font_scale);
        core.set_class_environment(class_environment(&config.environment));
        core.set_header_heights(config.header_heights.0, config.header_heights.1);
        core.set_safe_area(insets(&config.environment));
        core.set_environment(ui_environment(&config.environment, config.viewport));
        // WebAssembly has no threads for a runtime: tasks' timers are
        // `nsh_core::time`'s there, fired by the driver.
        #[cfg(not(target_arch = "wasm32"))]
        core.set_task_runtime(config.runtime.clone().unwrap_or_else(shared_runtime));
        // A Rust app's theme and fonts come with its root (a JS app's from
        // its bundle).
        if let App::Rust(root) = &config.app {
            if let Some(theme) = root.theme_ref() {
                core.set_theme(theme.clone());
            }
            for font in root.fonts() {
                if let Err(err) = core.register_font(font) {
                    log::error!("font failed: {err}");
                    sink.report_error(&format!("font failed: {err}"));
                }
            }
        }
        // The fonts the UI sets' crates ship, before anything is measured.
        let fonts = config
            .ui_sets
            .iter()
            .flat_map(nsh_core::ui::Registry::asset_files)
            .filter(|file| file.kind == nsh_core::ui::AssetKind::Font);
        for font in fonts {
            let result = match config.assets.as_deref() {
                Some(assets) => assets.read(font.uri),
                None => Err("this host cannot load assets".to_owned()),
            }
            .and_then(|data| core.register_font(&data));
            if let Err(err) = result {
                log::error!("font failed: {}: {err}", font.uri);
                sink.report_error(&format!("font failed: {}: {err}", font.uri));
            }
        }
        // A task woken on any thread wakes the loop.
        let wake = tx.clone();
        core.set_task_notify(Arc::new(move || {
            let _ = wake.send(Message::Wake);
        }));
        let state = State {
            core,
            frame_requested: false,
            ops: Vec::new(),
            binds: Vec::new(),
            events: Vec::new(),
            trace: config.trace,
            perf: None,
            perf_commit: perf::CommitTiming::default(),
            committed_since_gc: false,
            hot: Vec::new(),
            scroll_events: FxHashMap::default(),
            touch_move: None,
            resized: None,
            net: Net {
                network: config.network.clone(),
                tx,
                queue: NetworkQueue::default(),
                inflight: FxHashMap::default(),
                access: access.clone(),
                hops: 0,
            },
        };

        let mut environment = config.environment;
        environment.window = (config.viewport.width, config.viewport.height);
        let start = Instant::now();
        let mut driver = Driver {
            state,
            script: None,
            sink: sink.clone(),
            assets: config.assets.clone(),
            stopped: false,
            last_gc: Instant::now(),
        };
        let eval_us = match &config.app {
            App::Js(bundle) => {
                let (script, eval_us) = Script::start(
                    bundle,
                    config.limits,
                    &config.ui_sets,
                    &environment,
                    text_engine,
                    stop,
                    sink,
                )?;
                driver.script = Some(script);
                eval_us
            }
            App::Rust(root) => {
                driver.state.core.mount_root(root);
                0
            }
        };
        let Driver {
            state,
            script,
            sink,
            assets,
            ..
        } = &mut driver;
        let script = script.as_ref();
        state.load_fonts(script, assets.as_deref(), &**sink);
        if let Some(batch) = state.commit(script) {
            if state.trace {
                log::info!(
                    target: "nsh::perf",
                    "first frame: eval {eval_us}µs, total {}µs",
                    start.elapsed().as_micros()
                );
            }
            sink.submit(batch);
        }
        state.net.pump(script);
        state.load_chunks(script, &**sink);
        state.request_frame(script, &**sink);
        driver.check_stopped();
        Some(driver)
    }

    /// When the next timer, idle collection or throttled scroll event is
    /// due.
    fn deadline(&self) -> Option<Instant> {
        let script = self.script.as_ref();
        let gc_deadline = (script.is_some() && self.state.committed_since_gc)
            .then(|| self.last_gc + IDLE_GC_INTERVAL);
        #[cfg(target_arch = "wasm32")]
        let task_timer = nsh_core::time::next_timer();
        #[cfg(not(target_arch = "wasm32"))]
        let task_timer = None;
        [
            script.and_then(Script::next_timer),
            gc_deadline,
            self.state.next_scroll_event(),
            task_timer,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Handles one message; false on shutdown.
    fn handle(&mut self, message: Message) -> bool {
        handle(self.script.as_ref(), &mut self.state, message)
    }

    /// Runs what the handled messages and the time made due, and submits
    /// the frame.
    fn turn(&mut self) {
        let Driver {
            state,
            script,
            sink,
            assets,
            ..
        } = self;
        let script = script.as_ref();
        let turn_start = state.trace.then(Instant::now);
        state.dispatch_touch_move(script);
        state.dispatch_scroll_events(script);
        let hot = std::mem::take(&mut state.hot);
        if let Some(script) = script {
            for update in hot {
                script.hot_update(update, &**sink);
            }
            script.run_timers(Instant::now());
        }
        #[cfg(target_arch = "wasm32")]
        nsh_core::time::fire_timers(Instant::now());
        state.core.run_component_tasks();
        // What JS timers and Rust component tasks took; a Rust component
        // renders in the commit.
        let tasks = turn_start.map(|s| s.elapsed());
        state.load_fonts(script, assets.as_deref(), &**sink);
        if let Some(batch) = state.commit(script) {
            sink.submit(batch);
            if let (Some(start), Some(tasks)) = (turn_start, tasks) {
                log::info!(
                    target: "nsh::perf",
                    "turn: {}µs (timers and tasks {}µs)",
                    start.elapsed().as_micros(),
                    tasks.as_micros(),
                );
            }
        }
        if let Some(size) = state.resized.take() {
            sink.resized(size);
        }
        state.net.pump(script);
        state.load_chunks(script, &**sink);
        state.request_frame(script, &**sink);
        self.check_stopped();
    }

    /// Collects cycles when the engine has been idle long enough, so
    /// released nodes free their views.
    fn collect_if_idle(&mut self) {
        if let Some(script) = &self.script
            && self.state.committed_since_gc
            && self.last_gc.elapsed() >= IDLE_GC_INTERVAL
        {
            script.collect_garbage();
            self.last_gc = Instant::now();
            self.state.committed_since_gc = false;
            if let Some(batch) = self.state.commit(Some(script)) {
                self.sink.submit(batch);
            }
        }
    }

    fn check_stopped(&mut self) {
        if let Some(script) = &self.script
            && !self.stopped
            && let Some(reason) = script.fatal()
        {
            self.stopped = true;
            script.report(&*self.sink, "stopped", &reason);
        }
    }
}

struct ScrollEvent {
    x: f32,
    y: f32,
    content: Size,
    viewport: Size,
    /// The zoom scale.
    zoom: f32,
    throttle: Duration,
    last: Option<Instant>,
    /// An offset not reported yet.
    pending: bool,
}

struct State<M: TextEngine> {
    core: Core<M>,
    /// A display frame was requested and has not begun yet.
    frame_requested: bool,
    ops: Vec<Op>,
    /// List render requests not yet sent to JS.
    binds: Vec<ListBind>,
    /// Events raised by the core (e.g. `onEndReached`, `onLayout`).
    events: Vec<(NodeId, EventKind, EventPayload)>,
    trace: bool,
    /// Where commit timings go ([`EngineControl::set_perf_monitor`]).
    perf: Option<Arc<perf::PerfMonitor>>,
    /// The core's share of the wake-up being measured (`layout`, `ops`,
    /// `items`; `js` is the rest of the wake-up).
    perf_commit: perf::CommitTiming,
    committed_since_gc: bool,
    net: Net,
    /// Development updates to apply.
    hot: Vec<HotUpdate>,
    /// `onScroll` events: the latest offset per scroll view, sent once per
    /// frame at most (and not more often than the view's throttle).
    scroll_events: FxHashMap<NodeId, ScrollEvent>,
    /// Touch moves not delivered yet: those queued in one frame become one.
    touch_move: Option<(NodeId, Box<nsh_core::TouchEvent>)>,
    /// The viewport size a resize set this turn ([`MountSink::resized`]).
    resized: Option<Size>,
}

/// Requests between JS and the platform's network stack.
struct Net {
    network: Option<Arc<dyn Network>>,
    tx: Sender<Message>,
    queue: NetworkQueue,
    inflight: FxHashMap<RequestId, Inflight>,
    /// What the app may reach.
    access: Arc<nsh_core::access::Access>,
    /// Hops started so far (the id of the next).
    hops: u64,
}

/// A request in flight.
struct Inflight {
    task: Box<dyn FetchTask>,
    /// The hop the task is.
    hop: u64,
    /// Requests whose redirects the engine follows (to check each hop).
    followed: Option<access::Followed>,
}

impl Net {
    /// Hands what JS asked of its requests to the network.
    fn pump(&mut self, script: Option<&Script>) {
        let Some(script) = script else { return };
        script.take_network(&mut self.queue);
        for (id, op) in std::mem::take(&mut self.queue.ops) {
            match op {
                FetchOp::Start(request) => self.request(script, id, request),
                FetchOp::Send(chunk) => {
                    if let Some(i) = self.inflight.get_mut(&id) {
                        i.task.send(chunk);
                    }
                }
                FetchOp::Finish => {
                    if let Some(i) = self.inflight.get_mut(&id) {
                        i.task.finish();
                    }
                }
                FetchOp::Pause(paused) => {
                    if let Some(i) = self.inflight.get_mut(&id) {
                        i.task.pause(paused);
                    }
                }
                FetchOp::Cancel => {
                    if let Some(i) = self.inflight.remove(&id) {
                        i.task.cancel();
                    }
                }
            }
        }
    }

    /// Makes request `id`, if the app may.
    fn request(&mut self, script: &Script, id: RequestId, request: HttpRequest) {
        if self.access.is_unrestricted() {
            self.start(id, request, None);
        } else if !self.access.allows_http(&request.method, &request.url) {
            let refused = access::refused(&request.method, &request.url);
            script.fetch_event(id, FetchEvent::Failed(refused));
        } else if request.follow_redirects {
            let hop = access::hop(&request);
            let followed = access::Followed { request, hops: 0 };
            self.start(id, hop, Some(followed));
        } else {
            self.start(id, request, None);
        }
    }

    /// Hands one request (or redirect hop) to the network.
    fn start(&mut self, id: RequestId, request: HttpRequest, followed: Option<access::Followed>) {
        self.hops += 1;
        let hop = self.hops;
        let Some(network) = &self.network else {
            let failed = FetchEvent::Failed("this host has no network stack".to_owned());
            let _ = self.tx.send(Message::Fetch(id, hop, failed));
            return;
        };
        let tx = self.tx.clone();
        let events: FetchEvents = Box::new(move |event| {
            let _ = tx.send(Message::Fetch(id, hop, event));
        });
        let task = network.fetch(request, events);
        self.inflight.insert(
            id,
            Inflight {
                task,
                hop,
                followed,
            },
        );
    }

    /// An event of hop `hop` of request `id`: what JS is told, if anything
    /// (`None` for events of hops left behind, and redirects the engine
    /// follows).
    fn event(&mut self, id: RequestId, hop: u64, event: FetchEvent) -> Option<FetchEvent> {
        if self.network.is_none() {
            return Some(event);
        }
        let inflight = self.inflight.get_mut(&id).filter(|i| i.hop == hop)?;
        match event {
            FetchEvent::Head(head) => {
                let Some(followed) = &mut inflight.followed else {
                    return Some(FetchEvent::Head(head));
                };
                match access::next(&self.access, followed, head) {
                    access::Next::Done(head) => Some(FetchEvent::Head(head)),
                    access::Next::Fail(message) => {
                        if let Some(i) = self.inflight.remove(&id) {
                            i.task.cancel();
                        }
                        Some(FetchEvent::Failed(message))
                    }
                    access::Next::Hop(request) => {
                        let i = self.inflight.remove(&id)?;
                        let followed = i.followed;
                        i.task.cancel();
                        self.start(id, request, followed);
                        None
                    }
                }
            }
            FetchEvent::End | FetchEvent::Failed(_) => {
                self.inflight.remove(&id);
                Some(event)
            }
            event => Some(event),
        }
    }
}

impl<M: TextEngine> State<M> {
    /// Ends the measured wake-up that began at `started`
    /// ([`EngineControl::set_perf_monitor`]).
    fn record_perf(&mut self, started: Option<Instant>) {
        let (Some(started), Some(monitor)) = (started, &self.perf) else {
            return;
        };
        let busy = started.elapsed();
        monitor.record_busy(busy);
        let mut commit = std::mem::take(&mut self.perf_commit);
        if commit.ops > 0 || commit.items > 0 {
            commit.js = busy.saturating_sub(commit.layout);
            monitor.record_commit(commit);
        }
    }

    /// Sends the pending touch move.
    fn dispatch_touch_move(&mut self, script: Option<&Script>) {
        if let Some((id, touch)) = self.touch_move.take()
            && let Some(script) = script
        {
            script.dispatch_event(id, EventKind::Touch, &EventPayload::Touch(touch));
        }
    }

    /// When a throttled `onScroll` is due.
    fn next_scroll_event(&self) -> Option<Instant> {
        self.scroll_events
            .values()
            .filter(|e| e.pending)
            .filter_map(|e| e.last.map(|t| t + e.throttle))
            .min()
    }

    /// Sends the `onScroll` events that are due.
    fn dispatch_scroll_events(&mut self, script: Option<&Script>) {
        if self.scroll_events.is_empty() {
            return;
        }
        let now = Instant::now();
        // Views that stopped listening (or are gone).
        let core = &self.core;
        self.scroll_events
            .retain(|&id, _| core.scroll_listener(id).is_some());
        for (&id, e) in &mut self.scroll_events {
            if !e.pending || e.last.is_some_and(|t| now < t + e.throttle) {
                continue;
            }
            e.pending = false;
            e.last = Some(now);
            let n = |v: f32| PayloadValue::Number(v as f64);
            let payload = EventPayload::Map(vec![
                ("x", n(e.x)),
                ("y", n(e.y)),
                ("contentWidth", n(e.content.width)),
                ("contentHeight", n(e.content.height)),
                ("width", n(e.viewport.width)),
                ("height", n(e.viewport.height)),
                ("zoomScale", n(e.zoom)),
            ]);
            // A Rust component's scroll view is the component's.
            if !self.core.component_event(id, EventKind::Scroll, &payload)
                && let Some(script) = script
            {
                script.dispatch_event(id, EventKind::Scroll, &payload);
            }
        }
    }

    /// Asks the platform for the chunks JS imports and does not have.
    fn load_chunks(&mut self, script: Option<&Script>, sink: &dyn MountSink) {
        let Some(script) = script else { return };
        let mut ids = Vec::new();
        script.take_chunk_requests(&mut ids);
        for id in ids {
            if !sink.load_chunk(&id) {
                let message = "this host does not load chunks (the app was built split)".to_owned();
                let _ = self.net.tx.send(Message::Chunk(Err((id, message))));
            }
        }
    }

    /// Asks the platform for a frame when JS or a component's task waits
    /// for one.
    fn request_frame(&mut self, script: Option<&Script>, sink: &dyn MountSink) {
        if !self.frame_requested
            && (script.is_some_and(Script::wants_frame) || self.core.wants_frame())
        {
            self.frame_requested = true;
            sink.request_frame();
        }
    }

    /// Registers fonts imported by JS. Blocking: text using them must not be
    /// measured with a fallback font first.
    fn load_fonts(
        &mut self,
        script: Option<&Script>,
        assets: Option<&dyn Assets>,
        sink: &dyn MountSink,
    ) {
        let Some(script) = script else { return };
        let mut fonts = Vec::new();
        script.take_fonts(&mut fonts);
        for uri in fonts {
            let result = match assets {
                Some(assets) => assets.read(&uri),
                None => Err("this host cannot load assets".to_owned()),
            }
            .and_then(|data| self.core.register_font(&data));
            if let Err(err) = result {
                script.report(sink, "font failed", &format!("{uri}: {err}"));
            }
        }
    }

    /// Produces one frame: applies JS ops, lays out, and lets lists pull
    /// the items they need, merging everything into a single batch.
    fn commit(&mut self, script: Option<&Script>) -> Option<MountBatch> {
        let start = Instant::now();
        let mut merged: Option<MountBatch> = None;
        let mut op_count = 0;
        let mut js = std::time::Duration::ZERO;
        for _ in 0..MAX_LIST_ROUNDS {
            // Requests from the previous round (or frame) first.
            let js_start = Instant::now();
            // Without JS, lists bind nowhere (Rust components' lists bind in
            // the core).
            let binds = self.binds.drain(..);
            if let Some(script) = script {
                for bind in binds {
                    script.bind_list(bind.list, &bind.binds);
                }
            } else {
                drop(binds);
            }
            // The core's events (layouts, ends reached, ...) on a Rust
            // component's nodes are the component's; the rest are JS's.
            for (id, event, payload) in self.events.drain(..) {
                if !self.core.component_event(id, event, &payload)
                    && let Some(script) = script
                {
                    script.dispatch_event(id, event, &payload);
                }
            }
            if let Some(script) = script {
                script.take_ops(&mut self.ops);
            }
            js += js_start.elapsed();
            op_count += self.ops.len();
            // A stop drops the rest of the ops (`drain` clears them).
            for op in self.ops.drain(..) {
                match self.core.apply(op) {
                    Ok(()) => {}
                    Err(err) if err.is_fatal() => {
                        let Some(script) = script else { break };
                        script.stop(match err {
                            ApplyError::TooManyNodes(max) => {
                                format!("the app created more than {max} nodes and was stopped")
                            }
                            other => {
                                format!("the app sent an invalid op ({other:?}) and was stopped")
                            }
                        });
                        break;
                    }
                    Err(err) => log::error!("invalid op: {err:?}"),
                }
            }
            if let Some(batch) = self.core.commit() {
                match &mut merged {
                    Some(m) => extend_frame(&mut m.items, batch.items),
                    None => merged = Some(batch),
                }
            }
            self.core.take_list_binds(&mut self.binds);
            self.core.take_events(&mut self.events);
            if self.binds.is_empty() && self.events.is_empty() {
                break;
            }
        }
        self.committed_since_gc |= op_count > 0;
        if self.perf.is_some() && (op_count > 0 || merged.is_some()) {
            let c = &mut self.perf_commit;
            c.layout += start.elapsed().saturating_sub(js);
            c.ops += op_count as u32;
            c.items += merged.as_ref().map_or(0, |b| b.items.len() as u32);
        }
        if self.trace && (op_count > 0 || merged.is_some()) {
            log::info!(
                target: "nsh::perf",
                "commit: {op_count} ops, {}µs (list js {}µs), {} mount items, {} nodes",
                start.elapsed().as_micros(),
                js.as_micros(),
                merged.as_ref().map_or(0, |b| b.items.len()),
                self.core.node_count(),
            );
        }
        merged
    }
}

fn insets(env: &Environment) -> EdgeInsets {
    let a = env.safe_area;
    EdgeInsets {
        top: a.top,
        right: a.right,
        bottom: a.bottom,
        left: a.left,
    }
}

/// What Rust components read from the environment.
fn ui_environment(env: &Environment, window: Size) -> nsh_core::ui::Environment {
    nsh_core::ui::Environment {
        window,
        safe_area: insets(env),
        keyboard_height: env.keyboard_height,
        keyboard_duration: env.keyboard_duration,
        dark: env.color_scheme == ColorScheme::Dark,
        font_scale: env.font_scale,
        reduce_motion: env.reduce_motion,
        screen_reader: env.screen_reader,
        rtl: env.rtl,
        pixel_ratio: env.pixel_ratio,
        os: env.os,
        os_version: env.os_version,
        idiom: env.idiom,
        locale: env.locale,
    }
}

/// What class variants read from the environment.
fn class_environment(env: &Environment) -> nsh_core::tailwind::ClassEnvironment {
    nsh_core::tailwind::ClassEnvironment {
        dark: env.color_scheme == ColorScheme::Dark,
        platform: if env.os == Os::Web {
            nsh_core::tailwind::Platform::Web
        } else if cfg!(target_os = "android") {
            nsh_core::tailwind::Platform::Android
        } else {
            nsh_core::tailwind::Platform::Ios
        },
        reduce_motion: env.reduce_motion,
    }
}

/// Returns false on shutdown.
fn handle<M: TextEngine>(script: Option<&Script>, state: &mut State<M>, message: Message) -> bool {
    match message {
        Message::Event(id, event, payload) => {
            if state.trace {
                log::info!(target: "nsh::perf", "event {event:?} on view {id}");
            }
            // The core tracks input text for layout and to recognize stale
            // values (from JS and from Rust components alike).
            if let (EventKind::ChangeText, EventPayload::Text { text, count }) = (event, &payload) {
                state.core.input_changed(id, text, *count);
            }
            // Events on a Rust component's parts are the component's.
            if state.core.component_event(id, event, &payload) {
                return true;
            }
            if let EventPayload::Touch(touch) = payload {
                if touch.phase == nsh_core::TouchPhase::Move {
                    match &mut state.touch_move {
                        Some((_, pending)) => pending.merge_move(*touch),
                        None => state.touch_move = Some((id, touch)),
                    }
                    return true;
                }
                // Moves before a start, end or cancel go first.
                state.dispatch_touch_move(script);
                if let Some(script) = script {
                    script.dispatch_event(id, event, &EventPayload::Touch(touch));
                }
                return true;
            }
            let Some(script) = script else { return true };
            let start = state.trace.then(Instant::now);
            script.dispatch_event(id, event, &payload);
            if let Some(start) = start {
                // The handler, its microtasks and the decode of what it
                // committed.
                log::info!(target: "nsh::perf", "event js: {}µs", start.elapsed().as_micros());
            }
        }
        Message::Scroll(id, x, y, applied) => {
            state.core.set_scroll_offset(id, x, y, applied);
            if let Some((throttle_ms, content, viewport, zoom)) = state.core.scroll_listener(id) {
                let event = state.scroll_events.entry(id).or_insert(ScrollEvent {
                    x,
                    y,
                    content,
                    viewport,
                    zoom,
                    throttle: Duration::from_secs_f32(throttle_ms / 1000.0),
                    last: None,
                    pending: true,
                });
                (event.x, event.y, event.content, event.viewport, event.zoom) =
                    (x, y, content, viewport, zoom);
                event.throttle = Duration::from_secs_f32(throttle_ms / 1000.0);
                event.pending = true;
            }
        }
        Message::Zoom(id, scale) => state.core.set_zoom(id, scale),
        Message::ComponentSize(id, size) => state.core.set_component_size(id, size),
        Message::AnimationsFinished(mut finished) => {
            // The components' animations end in the core, JS's in JS.
            state.core.animations_finished(&mut finished);
            if let Some(script) = script {
                script.animations_finished(&finished);
            }
        }
        Message::Resize(size) => {
            state.core.set_viewport(size);
            state.resized = Some(size);
        }
        Message::Wake => {}
        Message::Environment(env) => {
            state.core.set_font_scale(env.font_scale);
            state.core.set_class_environment(class_environment(&env));
            state.core.set_safe_area(insets(&env));
            let window = Size::new(env.window.0, env.window.1);
            state.core.set_environment(ui_environment(&env, window));
            if let Some(script) = script {
                script.set_environment(&env);
            }
        }
        Message::RootSize(id, size, insets) => state.core.set_root_size(id, size, insets),
        Message::ScreenHeader(id, height) => state.core.set_screen_header(id, height),
        Message::Frame => {
            state.frame_requested = false;
            state.core.run_frame(Instant::now());
            if let Some(script) = script {
                script.run_frame();
            }
        }
        Message::States(id, states) => state.core.set_states(id, states),
        Message::FontsChanged => state.core.fonts_changed(),
        Message::Fetch(id, hop, event) => {
            if let Some(event) = state.net.event(id, hop, event)
                && let Some(script) = script
            {
                script.fetch_event(id, event);
            }
        }
        Message::Hot(update) => state.hot.push(update),
        Message::Chunk(chunk) => {
            if let Some(script) = script {
                script.chunk(chunk);
            }
        }
        Message::Perf(monitor) => {
            state.perf = monitor;
            state.perf_commit = perf::CommitTiming::default();
        }
        Message::Shutdown => return false,
    }
    true
}

/// Appends a commit's items to the frame's. A scroll view's content size
/// that the frame already sets is updated to the later size (unless the
/// view scrolls in between): a row relaid over two commits must not send
/// the platform a transient size, which clamps the offset before the
/// content grows back (a list jumping when a row expands).
fn extend_frame(frame: &mut Vec<nsh_core::MountItem>, items: Vec<nsh_core::MountItem>) {
    use nsh_core::MountItem;
    let start = frame.len();
    frame.extend(items);
    for k in start..frame.len() {
        let MountItem::ScrollContent { id, size } = frame[k] else {
            continue;
        };
        let earlier = frame[..k].iter().rposition(|m| {
            matches!(m, MountItem::ScrollContent { id: e, .. } | MountItem::ScrollBy { id: e, .. } if *e == id)
        });
        if let Some(j) = earlier
            && let MountItem::ScrollContent { size: s, .. } = &mut frame[j]
        {
            *s = size;
        }
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::{MountItem, Size};

    #[test]
    fn a_frame_sends_a_scroll_views_last_content_size() {
        let content = |height| MountItem::ScrollContent {
            id: 2,
            size: Size::new(390.0, height),
        };
        let heights = |frame: &[MountItem]| -> Vec<f32> {
            frame
                .iter()
                .filter_map(|m| match m {
                    MountItem::ScrollContent { size, .. } => Some(size.height),
                    _ => None,
                })
                .collect()
        };
        let mut frame = vec![content(313.0)];
        super::extend_frame(&mut frame, vec![content(544.0)]);
        assert_eq!(heights(&frame), [544.0, 544.0]);
        // A scroll in between keeps the size it scrolled against.
        let mut frame = vec![
            content(313.0),
            MountItem::ScrollBy {
                id: 2,
                dx: 0.0,
                dy: 10.0,
            },
        ];
        super::extend_frame(&mut frame, vec![content(544.0)]);
        assert_eq!(heights(&frame), [313.0, 544.0]);
    }
}
