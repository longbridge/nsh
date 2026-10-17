//! WebAssembly backend for nsh: runs an app in a browser page and renders
//! it to the DOM: apps published with `nsh package --target web`, the web
//! development host and the website's live demos. `Platform.OS` is `"web"`;
//! the page reports the idiom (phone, tablet or desktop browser), locale
//! and the user's settings ([`nsh_environment`]).
//!
//! - The engine runs on the page's main thread ([`nsh_engine::LocalEngine`]):
//!   the JS runtime (`web/nsh.js`) calls [`nsh_pump`] after input and when
//!   the deadline it returned is reached.
//! - Each commit becomes one JSON batch ([`encode`]) applied by the DOM
//!   mounter: absolutely positioned elements at the frames the core laid
//!   out, CSS made here from the view styles.
//! - Text is laid out here with the browser's font measurements and drawn
//!   run by run ([`text`]); native animations run here on
//!   `requestAnimationFrame` ([`animation`]).
//!
//! - `fetch` runs on the browser's `fetch` ([`net`]).
//!
//! There is no web view and no native components; scroll views use the
//! browser's scrolling. Off WebAssembly
//! this crate is empty.

#![cfg(target_arch = "wasm32")]

mod animation;
mod canvas;
mod encode;
mod ffi;
mod net;
mod text;

use std::cell::{Cell, RefCell};

use nsh_core::{EventKind, EventPayload, MountBatch, MountItem, PayloadValue, ROOT_ID, Size};
use nsh_engine::{
    App, Assets, Bundle, Chunk, ColorScheme, EngineConfig, EngineControl, Environment, HotUpdate,
    Idiom, Limits, LocalEngine, MountSink, Os, Policy,
};

pub use crate::net::{
    nsh_fetch_body_wanted, nsh_fetch_data, nsh_fetch_end, nsh_fetch_failed, nsh_fetch_head,
};
use crate::{animation::UiAnimation, encode::Out, text::WebTextEngine};

thread_local! {
    static APP: RefCell<Option<Running>> = const { RefCell::new(None) };
    /// Batches submitted during a pump, applied after it (the sink cannot
    /// reach the running app while it pumps).
    static BATCHES: RefCell<Vec<MountBatch>> = const { RefCell::new(Vec::new()) };
    /// The bytes of the asset JS hands over in `read_asset`.
    pub(crate) static ASSET: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    static FRAME_REQUESTED: Cell<bool> = const { Cell::new(false) };
    /// The page's URL, which relative request URLs resolve against.
    static BASE_URL: RefCell<Option<String>> = const { RefCell::new(None) };
    /// How the next [`nsh_start`] runs its bundle ([`nsh_bundle_options`]).
    static OPTIONS: RefCell<BundleOptions> = RefCell::new(BundleOptions::default());
    /// A hot update could not be applied in place ([`nsh_hot_failed`]).
    static HOT_FAILED: Cell<bool> = const { Cell::new(false) };
    /// What the page reported last ([`nsh_environment`]), kept for the
    /// next [`nsh_start`].
    static PAGE: Cell<Option<Environment>> = const { Cell::new(None) };
    /// What the last [`nsh_compile`] made: the module or the error.
    #[cfg(feature = "compiler")]
    static COMPILED: RefCell<String> = const { RefCell::new(String::new()) };
}

/// What [`nsh_bundle_options`] set for the next [`nsh_start`].
#[derive(Default)]
struct BundleOptions {
    /// Run as a release: hardened, as packaged apps run (default:
    /// development settings, for the player and the dev server).
    release: bool,
    /// What the app declares (`capabilities.json`).
    capabilities: Option<String>,
    /// The bundle's source map (JSON).
    source_map: Option<String>,
}

struct Running {
    engine: LocalEngine<WebTextEngine>,
    control: EngineControl,
    animation: UiAnimation,
    /// JS asked for a display frame (`requestAnimationFrame`).
    js_frame: bool,
    /// The surface's environment (window, appearance).
    environment: Environment,
}

struct Sink;

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        BATCHES.with(|b| b.borrow_mut().push(batch));
    }

    fn report_error(&self, message: &str) {
        ffi::report_error(message);
    }

    fn request_frame(&self) {
        FRAME_REQUESTED.with(|f| f.set(true));
    }

    fn hot_update_failed(&self) {
        HOT_FAILED.with(|f| f.set(true));
    }

    fn load_chunk(&self, id: &str) -> bool {
        ffi::load_chunk(id);
        true
    }
}

/// Bundled files the page preloaded (fonts, images drawn on a canvas).
struct PageAssets;

impl Assets for PageAssets {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        ffi::read_asset(uri).ok_or_else(|| format!("{uri} is not available on the web"))
    }
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            ffi::log(record.level(), &record.args().to_string());
        }
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger;

/// Memory for JS to write strings and bytes into.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Frees what [`nsh_alloc`] returned.
///
/// # Safety
///
/// `ptr` and `len` come from one [`nsh_alloc`] call, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_dealloc(ptr: *mut u8, len: usize) {
    // SAFETY: the caller passes what `nsh_alloc` returned for `len`.
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len.max(1)) });
}

/// # Safety
///
/// `ptr..ptr + len` is memory from [`nsh_alloc`] holding `len` bytes.
unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        return &[];
    }
    // SAFETY: guaranteed by the caller.
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

/// # Safety
///
/// As [`bytes`].
unsafe fn string(ptr: *const u8, len: usize) -> String {
    // SAFETY: guaranteed by the caller.
    String::from_utf8_lossy(unsafe { bytes(ptr, len) }).into_owned()
}

/// Hands the asset JS was asked for (`read_asset`) back.
///
/// # Safety
///
/// `ptr..ptr + len` is memory from [`nsh_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_asset_bytes(ptr: *const u8, len: usize) {
    // SAFETY: guaranteed by the caller.
    let data = unsafe { bytes(ptr, len) }.to_vec();
    ASSET.with(|a| *a.borrow_mut() = Some(data));
}

fn environment(width: f32, height: f32, dark: bool, pixel_ratio: f32) -> Environment {
    Environment {
        window: (width, height),
        color_scheme: if dark {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        },
        pixel_ratio,
        ..page()
    }
}

/// The page's environment as last reported (`os` is the web's).
fn page() -> Environment {
    PAGE.with(Cell::get).unwrap_or(Environment {
        os: Os::Web,
        ..Environment::default()
    })
}

/// What the page knows about the user and the device, as JSON: `idiom`
/// (`"phone"`, `"pad"`, `"desktop"`), `locale` (a BCP 47 tag), `rtl`,
/// `reduceMotion`, `fontScale`, `pixelRatio`, `safeArea` (`[top, right,
/// bottom, left]`) and `keyboard` (the height the on-screen keyboard
/// covers); missing keys keep their value. Called before [`nsh_start`]
/// and on every change; the idiom is the one at start, as on every
/// platform. Returns the pump delay.
///
/// # Safety
///
/// `ptr..ptr + len` is memory from [`nsh_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_environment(ptr: *const u8, len: usize) -> f64 {
    // SAFETY: guaranteed by the caller.
    let json = unsafe { string(ptr, len) };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
        return pump();
    };
    let mut env = page();
    let num = |k: &str| value.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
    let flag = |k: &str| value.get(k).and_then(|v| v.as_bool());
    if let Some(idiom) = value.get("idiom").and_then(|v| v.as_str()) {
        env.idiom = match idiom {
            "desktop" => Idiom::Desktop,
            "pad" => Idiom::Pad,
            _ => Idiom::Phone,
        };
    }
    if let Some(locale) = value
        .get("locale")
        .and_then(|v| v.as_str())
        .and_then(nsh_core::platform::Locale::parse)
    {
        env.locale = locale;
    }
    env.rtl = flag("rtl").unwrap_or(env.rtl);
    env.reduce_motion = flag("reduceMotion").unwrap_or(env.reduce_motion);
    if let Some(scale) = num("fontScale").filter(|s| *s > 0.0) {
        env.font_scale = scale;
    }
    if let Some(ratio) = num("pixelRatio").filter(|r| *r > 0.0) {
        env.pixel_ratio = ratio;
    }
    if let Some([top, right, bottom, left]) = value
        .get("safeArea")
        .and_then(|v| v.as_array())
        .filter(|a| a.len() == 4)
        .map(|a| [0, 1, 2, 3].map(|i| a[i].as_f64().unwrap_or(0.0) as f32))
    {
        env.safe_area = nsh_core::platform::Insets {
            top,
            right,
            bottom,
            left,
        };
    }
    env.keyboard_height = num("keyboard").unwrap_or(env.keyboard_height);
    PAGE.with(|p| p.set(Some(env)));
    send(|app| {
        let e = &mut app.environment;
        e.locale = env.locale;
        e.rtl = env.rtl;
        e.reduce_motion = env.reduce_motion;
        e.font_scale = env.font_scale;
        e.pixel_ratio = env.pixel_ratio;
        e.safe_area = env.safe_area;
        e.keyboard_height = env.keyboard_height;
        app.control.set_environment(app.environment);
    })
}

/// Starts the app in `bundle` (JS source, an ES module as `nsh build`
/// writes it) in a `width` × `height` surface. Returns the delay (ms) until
/// [`nsh_pump`] is due, or -1.
///
/// # Safety
///
/// `ptr..ptr + len` is memory from [`nsh_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_start(
    ptr: *const u8,
    len: usize,
    width: f32,
    height: f32,
    dark: u32,
    pixel_ratio: f32,
) -> f64 {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Info);
        std::panic::set_hook(Box::new(|info| {
            ffi::report_error(&format!("nsh panicked: {info}"));
        }));
    });
    // SAFETY: guaranteed by the caller.
    let code = unsafe { string(ptr, len) };
    APP.with(|a| a.borrow_mut().take());
    let environment = environment(width, height, dark != 0, pixel_ratio);
    let options = OPTIONS.with(|o| std::mem::take(&mut *o.borrow_mut()));
    let policy = if options.release {
        Policy::default()
    } else {
        Policy::development()
    };
    let mut bundle = Bundle::source(code).with_policy(policy);
    if let Some(map) = options.source_map {
        bundle = bundle.with_source_map(map);
    }
    let mut bundle = match bundle.with_capabilities_json(options.capabilities.as_deref()) {
        Ok(bundle) => bundle,
        Err(err) => {
            ffi::report_error(&err);
            return -1.0;
        }
    };
    bundle.base_url = BASE_URL.with(|b| b.borrow().clone());
    let config = EngineConfig {
        app: App::Js(bundle),
        viewport: Size::new(width, height),
        trace: false,
        network: Some(std::sync::Arc::new(net::WebNetwork)),
        assets: Some(std::sync::Arc::new(PageAssets)),
        environment,
        // A desktop browser's headers are as tall as the desktop backends'
        // (`navigation::HEADER_HEIGHTS` there).
        header_heights: if environment.idiom == Idiom::Desktop {
            (47.0, 94.0)
        } else {
            (44.0, 96.0)
        },
        limits: Limits::default(),
        ui_sets: vec![nsh_ui_mobile::registry()],
        runtime: None,
    };
    let Some(engine) = LocalEngine::start(config, WebTextEngine::default(), Sink) else {
        return -1.0;
    };
    let control = engine.control();
    APP.with(|a| {
        *a.borrow_mut() = Some(Running {
            engine,
            control,
            animation: UiAnimation::default(),
            js_frame: false,
            environment,
        });
    });
    pump()
}

/// Sets the page's URL (`document.baseURI`): relative request URLs
/// (`fetch("/api")`) of the apps started next resolve against it.
///
/// # Safety
///
/// `ptr..ptr + len` is memory from [`nsh_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_set_base_url(ptr: *const u8, len: usize) {
    // SAFETY: guaranteed by the caller.
    let url = unsafe { string(ptr, len) };
    BASE_URL.with(|b| *b.borrow_mut() = Some(url));
}

/// Sets how the next [`nsh_start`] runs its bundle: as a release
/// (`release` 1: hardened, as `nsh package --target web` ships apps) or
/// with development settings (0, the default), under the capabilities the
/// app declares (`capabilities.json`; empty: no declaration) and with the
/// bundle's source map (empty: none).
///
/// # Safety
///
/// Both ranges are memory from [`nsh_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_bundle_options(
    release: u32,
    capabilities: *const u8,
    capabilities_len: usize,
    map: *const u8,
    map_len: usize,
) {
    // SAFETY: guaranteed by the caller.
    let (capabilities, map) =
        unsafe { (string(capabilities, capabilities_len), string(map, map_len)) };
    OPTIONS.with(|o| {
        *o.borrow_mut() = BundleOptions {
            release: release != 0,
            capabilities: (!capabilities.is_empty()).then_some(capabilities),
            source_map: (!map.is_empty()).then_some(map),
        }
    });
}

/// Applies a development update (a script from the dev server's hot
/// updates, named `name` in stack traces, with its source map or none)
/// to the running app. Returns the pump delay; [`nsh_hot_failed`] then
/// tells whether the app must be loaded again instead.
///
/// # Safety
///
/// The three ranges are memory from [`nsh_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_hot_update(
    name: *const u8,
    name_len: usize,
    code: *const u8,
    code_len: usize,
    map: *const u8,
    map_len: usize,
) -> f64 {
    // SAFETY: guaranteed by the caller.
    let (name, code, map) = unsafe {
        (
            string(name, name_len),
            string(code, code_len),
            string(map, map_len),
        )
    };
    HOT_FAILED.with(|f| f.set(false));
    let update = HotUpdate {
        name,
        code,
        source_map: (!map.is_empty()).then(|| map.into()),
    };
    send(|app| app.control.hot_update(update))
}

/// Whether the last [`nsh_hot_update`] could not be applied in place (1):
/// the app must be started again with the new bundle.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_hot_failed() -> u32 {
    HOT_FAILED.with(|f| f.replace(false)) as u32
}

/// Hands over chunk `id` of a split bundle that the page fetched after
/// `load_chunk` (`ok` 1: `code` is its source; 0: `code` says why it could
/// not be loaded). Returns the pump delay.
///
/// # Safety
///
/// Both ranges are memory from [`nsh_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_chunk(
    id: *const u8,
    id_len: usize,
    code: *const u8,
    code_len: usize,
    ok: u32,
) -> f64 {
    // SAFETY: guaranteed by the caller.
    let (id, code) = unsafe { (string(id, id_len), string(code, code_len)) };
    send(|app| match ok {
        0 => app.control.chunk_failed(id, code),
        _ => app.control.chunk_loaded(Chunk::source(id, code)),
    })
}

/// Compiles one module (`name`'s extension says TSX, TS, JSX or JS) to the
/// ES module [`nsh_start`] runs, for the playground. Returns 1 when it
/// compiled, 0 for an error; [`nsh_compiled_ptr`] / [`nsh_compiled_len`]
/// give the module or the error text.
///
/// # Safety
///
/// Both ranges are memory from [`nsh_alloc`].
#[cfg(feature = "compiler")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_compile(
    name: *const u8,
    name_len: usize,
    ptr: *const u8,
    len: usize,
) -> u32 {
    // SAFETY: guaranteed by the caller.
    let (name, source) = unsafe { (string(name, name_len), string(ptr, len)) };
    let result = nsh_bundler::compile_module(&name, &source, &[nsh_ui_mobile::registry()]);
    let ok = result.is_ok();
    COMPILED.with(|c| *c.borrow_mut() = result.unwrap_or_else(|e| e));
    ok as u32
}

/// Where the text of the last [`nsh_compile`] starts.
#[cfg(feature = "compiler")]
#[unsafe(no_mangle)]
pub extern "C" fn nsh_compiled_ptr() -> *const u8 {
    COMPILED.with(|c| c.borrow().as_ptr())
}

/// The length of the text of the last [`nsh_compile`] in bytes.
#[cfg(feature = "compiler")]
#[unsafe(no_mangle)]
pub extern "C" fn nsh_compiled_len() -> usize {
    COMPILED.with(|c| c.borrow().len())
}

/// Stops the app.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_stop() {
    APP.with(|a| a.borrow_mut().take());
    BATCHES.with(|b| b.borrow_mut().clear());
}

/// Applies the submitted batches: animation items to the animator, the
/// rest to the DOM.
fn flush() {
    let batches = BATCHES.with(|b| std::mem::take(&mut *b.borrow_mut()));
    if batches.is_empty() {
        request_frames();
        return;
    }
    APP.with(|a| {
        let mut app = a.borrow_mut();
        let Some(app) = app.as_mut() else { return };
        let mut out = Out::new();
        for batch in &batches {
            for item in &batch.items {
                app.animation.apply(item);
                if !matches!(
                    item,
                    MountItem::Animation(_) | MountItem::AnimationBindings { .. }
                ) {
                    encode::encode(&mut out, item);
                }
            }
        }
        // Bound properties follow the styles just applied.
        app.animation.outputs(&mut out);
        if !out.is_empty() {
            ffi::submit(&out.finish());
        }
        finish_animations(app);
    });
    request_frames();
}

fn finish_animations(app: &mut Running) {
    if !app.animation.finished.is_empty() {
        let finished = std::mem::take(&mut app.animation.finished);
        app.control.animations_finished(finished);
    }
}

fn request_frames() {
    let wanted = FRAME_REQUESTED.with(|f| f.replace(false));
    let animating = APP.with(|a| {
        let mut app = a.borrow_mut();
        let Some(app) = app.as_mut() else {
            return false;
        };
        app.js_frame |= wanted;
        app.js_frame || app.animation.animating()
    });
    if animating {
        ffi::request_frame();
    }
}

/// Runs the engine; the delay (ms) until it is due again, or -1.
fn pump() -> f64 {
    let deadline = APP.with(|a| {
        let mut app = a.borrow_mut();
        app.as_mut().map(|app| app.engine.pump())
    });
    flush();
    match deadline.flatten() {
        Some(at) => {
            at.saturating_duration_since(nsh_core::time::Instant::now())
                .as_secs_f64()
                * 1000.0
        }
        None => -1.0,
    }
}

/// Runs what is due; returns the delay (ms) until it is due again, or -1.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_pump() -> f64 {
    pump()
}

fn send(f: impl FnOnce(&mut Running)) -> f64 {
    APP.with(|a| {
        if let Some(app) = a.borrow_mut().as_mut() {
            f(app);
        }
    });
    pump()
}

/// A display frame (`requestAnimationFrame`, `now` in ms): ticks the
/// native animations and runs JS's frame callbacks. Returns the pump
/// delay.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_frame(now: f64) -> f64 {
    APP.with(|a| {
        let mut app = a.borrow_mut();
        let Some(app) = app.as_mut() else { return };
        if app.animation.animating() {
            app.animation.tick(now);
            let mut out = Out::new();
            app.animation.outputs(&mut out);
            if !out.is_empty() {
                ffi::submit(&out.finish());
            }
            finish_animations(app);
        }
        if std::mem::take(&mut app.js_frame) {
            app.control.frame();
        }
    });
    pump()
}

/// An event on view `id`, named by its prop (`onPress`), with its payload
/// as JSON (an object or null).
///
/// # Safety
///
/// `name..name + name_len` and `ptr..ptr + len` are memory from
/// [`nsh_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_event(
    id: u32,
    name: *const u8,
    name_len: usize,
    ptr: *const u8,
    len: usize,
) -> f64 {
    // SAFETY: guaranteed by the caller.
    let name = unsafe { string(name, name_len) };
    let Some(kind) = EventKind::from_prop(&name) else {
        return pump();
    };
    // SAFETY: guaranteed by the caller.
    let json = unsafe { string(ptr, len) };
    let value: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    let num = |k: &str| value.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let text = |k: &str| {
        value
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let payload = match kind {
        EventKind::ScrollEnd
        | EventKind::ScrollBeginDrag
        | EventKind::ScrollEndDrag
        | EventKind::MomentumScrollBegin
        | EventKind::MomentumScrollEnd => EventPayload::Scroll {
            x: num("x"),
            y: num("y"),
        },
        EventKind::Load => EventPayload::Image {
            width: num("width"),
            height: num("height"),
        },
        EventKind::Error => EventPayload::Error {
            message: text("message"),
        },
        EventKind::ChangeText | EventKind::SubmitEditing => EventPayload::Text {
            text: text("text"),
            count: num("count") as u32,
        },
        _ => match &value {
            serde_json::Value::Object(map) => EventPayload::Map(
                map.iter()
                    .filter_map(|(k, v)| {
                        // Payload keys are static in the core: intern the
                        // known ones.
                        let key: &'static str = match k.as_str() {
                            "key" => "key",
                            "start" => "start",
                            "end" => "end",
                            "id" => "id",
                            "index" => "index",
                            "data" => "data",
                            "actionName" => "actionName",
                            "currentTime" => "currentTime",
                            "duration" => "duration",
                            "isBuffering" => "isBuffering",
                            "width" => "width",
                            "height" => "height",
                            "top" => "top",
                            "right" => "right",
                            "bottom" => "bottom",
                            "left" => "left",
                            "text" => "text",
                            "has" => "has",
                            "error" => "error",
                            _ => return None,
                        };
                        let v = match v {
                            serde_json::Value::Number(n) => {
                                PayloadValue::Number(n.as_f64().unwrap_or(0.0))
                            }
                            serde_json::Value::Bool(b) => PayloadValue::Bool(*b),
                            serde_json::Value::String(s) => PayloadValue::Str(s.clone()),
                            _ => return None,
                        };
                        Some((key, v))
                    })
                    .collect(),
            ),
            _ => EventPayload::None,
        },
    };
    send(|app| app.control.dispatch_event(id, kind, payload))
}

/// Touches, as JSON `{ phase, time, points: [{ id, target, path, pageX,
/// pageY, x, y, changed }] }` (`phase`: 0 start, 1 move, 2 end, 3
/// cancel).
///
/// # Safety
///
/// `ptr..ptr + len` is memory from [`nsh_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_touch(ptr: *const u8, len: usize) -> f64 {
    // SAFETY: guaranteed by the caller.
    let json = unsafe { string(ptr, len) };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
        return pump();
    };
    let f = |v: &serde_json::Value, k: &str| v.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let phase = match f(&value, "phase") as u32 {
        0 => nsh_core::TouchPhase::Start,
        1 => nsh_core::TouchPhase::Move,
        2 => nsh_core::TouchPhase::End,
        _ => nsh_core::TouchPhase::Cancel,
    };
    let points = value
        .get("points")
        .and_then(|p| p.as_array())
        .map(|points| {
            points
                .iter()
                .map(|p| nsh_core::TouchPoint {
                    id: f(p, "id") as u32,
                    target: f(p, "target") as u32,
                    path: p
                        .get("path")
                        .and_then(|p| p.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_u64())
                                .map(|v| v as u32)
                                .collect()
                        })
                        .unwrap_or_default(),
                    page_x: f(p, "pageX") as f32,
                    page_y: f(p, "pageY") as f32,
                    location_x: f(p, "x") as f32,
                    location_y: f(p, "y") as f32,
                    changed: p.get("changed").and_then(|v| v.as_bool()).unwrap_or(true),
                })
                .collect()
        })
        .unwrap_or_default();
    let event = nsh_core::TouchEvent {
        phase,
        timestamp: f(&value, "time"),
        points,
        activated: value
            .get("activated")
            .and_then(|v| v.as_u64())
            .map(|v| v as nsh_core::NodeId),
    };
    send(|app| {
        app.control.dispatch_event(
            ROOT_ID,
            EventKind::Touch,
            EventPayload::Touch(Box::new(event)),
        )
    })
}

/// A scroll view's offset changed (`applied`: the scrolls the core asked
/// for that were applied before it).
#[unsafe(no_mangle)]
pub extern "C" fn nsh_scroll(id: u32, x: f32, y: f32, applied: u32) -> f64 {
    send(|app| {
        if app.animation.scrolled(id, x, y) {
            let mut out = Out::new();
            app.animation.outputs(&mut out);
            if !out.is_empty() {
                ffi::submit(&out.finish());
            }
        }
        app.control.scroll(id, x, y, applied);
    })
}

/// A drag on a view with native pan values (`phase`: 0 start, 1 move, 2
/// end), translated `(tx, ty)` since it began, at velocity `(vx, vy)`
/// points per second.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_pan(id: u32, phase: u32, tx: f32, ty: f32, vx: f32, vy: f32) -> f64 {
    send(|app| {
        match phase {
            0 => app.animation.pan_began(id),
            _ => app.animation.pan_moved(id, tx, ty),
        }
        let mut out = Out::new();
        app.animation.outputs(&mut out);
        if !out.is_empty() {
            ffi::submit(&out.finish());
        }
        let (value_x, value_y) = app.animation.pan_values(id);
        let payload = EventPayload::Pan {
            translation_x: tx,
            translation_y: ty,
            velocity_x: vx,
            velocity_y: vy,
            value_x,
            value_y,
        };
        match phase {
            0 => app.control.dispatch_event(id, EventKind::PanStart, payload),
            2 => app.control.dispatch_event(id, EventKind::PanEnd, payload),
            _ => {}
        }
    })
}

/// A pinch (`rotation` 0; `amount`: the scale since it began) or rotation
/// (1; degrees) on a view with native values (`phase`: 0 start, 1 move, 2
/// end).
#[unsafe(no_mangle)]
pub extern "C" fn nsh_transform_gesture(id: u32, rotation: u32, phase: u32, amount: f32) -> f64 {
    let rotation = rotation != 0;
    send(|app| {
        match phase {
            0 => app.animation.transform_began(id, rotation),
            _ => app.animation.transform_moved(id, rotation, amount),
        }
        let mut out = Out::new();
        app.animation.outputs(&mut out);
        if !out.is_empty() {
            ffi::submit(&out.finish());
        }
        let key = if rotation { "rotation" } else { "scale" };
        let payload = EventPayload::Map(vec![
            (key, PayloadValue::Number(amount as f64)),
            ("velocity", PayloadValue::Number(0.0)),
        ]);
        let kind = match (rotation, phase) {
            (false, 0) => Some(EventKind::PinchStart),
            (false, 2) => Some(EventKind::PinchEnd),
            (true, 0) => Some(EventKind::RotationStart),
            (true, 2) => Some(EventKind::RotationEnd),
            _ => None,
        };
        if let Some(kind) = kind {
            app.control.dispatch_event(id, kind, payload);
        }
    })
}

/// The platform presents root `id` (a screen or modal) at `width` ×
/// `height`, with insets covered by system UI.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_root_size(
    id: u32,
    width: f32,
    height: f32,
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
) -> f64 {
    send(|app| {
        let insets = nsh_core::EdgeInsets {
            top,
            right,
            bottom,
            left,
        };
        app.control
            .set_root_size(id, Size::new(width, height), insets);
        let n = |v: f32| PayloadValue::Number(v as f64);
        app.control.dispatch_event(
            id,
            EventKind::Insets,
            EventPayload::Map(vec![
                ("top", n(top)),
                ("right", n(right)),
                ("bottom", n(bottom)),
                ("left", n(left)),
            ]),
        );
    })
}

/// The interaction states of view `id` (`active:`, `hover:`, `focus:`
/// classes).
#[unsafe(no_mangle)]
pub extern "C" fn nsh_states(id: u32, states: u32) -> f64 {
    send(|app| app.control.set_states(id, states as u8))
}

/// The surface was resized (points).
#[unsafe(no_mangle)]
pub extern "C" fn nsh_resize(width: f32, height: f32) -> f64 {
    send(|app| {
        app.environment.window = (width, height);
        app.control.resize(Size::new(width, height));
        app.control.set_environment(app.environment);
    })
}

/// The page switched between light and dark.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_set_dark(dark: u32) -> f64 {
    send(|app| {
        app.environment.color_scheme = if dark != 0 {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        };
        app.control.set_environment(app.environment);
    })
}
