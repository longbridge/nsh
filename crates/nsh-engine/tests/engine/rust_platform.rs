//! The platform's services for Rust components on the engine: the fonts
//! the UI sets' crates ship are registered before the first render, tasks
//! wait for display frames, components set the layout direction and read
//! the pixel ratio, the device and the locale.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{
    ApproxTextEngine, AttributedText, EventKind, EventPayload, MountBatch, MountItem, Size,
    TextEngine, TextLayout,
    platform::{Idiom, Locale, Os},
    ui::{AssetFile, AssetKind, Registry, Root, prelude::*},
};
use nsh_engine::{App, Assets, Engine, EngineConfig, Environment, MountSink};

/// Hands the batches to the test and counts the frames asked for.
struct FrameSink {
    batches: Sender<MountBatch>,
    frames: Arc<AtomicUsize>,
}

impl MountSink for FrameSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.batches.send(batch);
    }

    fn request_frame(&self) {
        self.frames.fetch_add(1, Ordering::SeqCst);
    }
}

fn config(app: App) -> EngineConfig {
    EngineConfig {
        app,
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
    }
}

/// A Rust app of `C`, its batches and the count of frames it asked for.
fn start<C: Component>(environment: Environment) -> (Engine, Receiver<MountBatch>, Arc<AtomicUsize>)
where
    C::Props: Default,
{
    let (tx, rx) = crossbeam_channel::unbounded();
    let frames = Arc::new(AtomicUsize::new(0));
    let config = EngineConfig {
        environment,
        ..config(App::Rust(Root::of::<C>()))
    };
    let sink = FrameSink {
        batches: tx,
        frames: frames.clone(),
    };
    let engine = Engine::start(config, || ApproxTextEngine, sink).unwrap();
    (engine, rx, frames)
}

/// The texts and layout directions the batches carry, until one shows
/// `until` (or 5 seconds passed).
fn until(rx: &Receiver<MountBatch>, until: &str) -> (Vec<String>, Vec<bool>, Option<u32>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let (mut texts, mut directions, mut host) = (Vec::new(), Vec::new(), None);
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            match item {
                MountItem::Text { text, .. } => texts.push(text.text.clone()),
                MountItem::LayoutDirection { rtl } => directions.push(*rtl),
                MountItem::Insert {
                    parent: nsh_core::ROOT_ID,
                    child,
                    ..
                } => host = Some(*child),
                _ => {}
            }
        }
        if texts.iter().any(|t| t == until) {
            break;
        }
    }
    (texts, directions, host)
}

/// Records the fonts registered.
struct Fonts(ApproxTextEngine, Arc<Mutex<Vec<Vec<u8>>>>);

impl TextEngine for Fonts {
    type Prepared = <ApproxTextEngine as TextEngine>::Prepared;

    fn prepare(&self, text: &AttributedText) -> Self::Prepared {
        self.0.prepare(text)
    }

    fn measure(&self, prepared: &Self::Prepared, max_width: Option<f32>) -> Size {
        self.0.measure(prepared, max_width)
    }

    fn layout(&self, prepared: &Self::Prepared, size: Size) -> TextLayout {
        self.0.layout(prepared, size)
    }

    fn register_font(&self, data: &[u8]) -> Result<(), String> {
        self.1.lock().unwrap().push(data.to_vec());
        Ok(())
    }
}

/// The files of a crate: a font, an image.
static FILES: &[AssetFile] = &[
    AssetFile {
        uri: "asset:/@app/fonts/Script.ttf",
        source: "/nowhere/fonts/Script.ttf",
        kind: AssetKind::Font,
    },
    AssetFile {
        uri: "asset:/@app/logo.png",
        source: "/nowhere/logo.png",
        kind: AssetKind::Image,
    },
];

/// Reads the font, and records what was read.
struct CrateAssets(Arc<Mutex<Vec<String>>>);

impl Assets for CrateAssets {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        self.0.lock().unwrap().push(uri.to_owned());
        Ok(uri.as_bytes().to_vec())
    }
}

#[component]
fn Label() -> impl IntoElement {
    view().child(text("ready").font_family("Script"))
}

#[test]
fn the_fonts_ui_sets_ship_are_registered_before_the_first_render() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let read = Arc::new(Mutex::new(Vec::new()));
    let fonts = Arc::new(Mutex::new(Vec::new()));
    let mut set = Registry::ui_set("app/native");
    set.assets(FILES);
    let config = EngineConfig {
        assets: Some(Arc::new(CrateAssets(read.clone()))),
        ui_sets: vec![set],
        ..config(App::Rust(Root::of::<Label>()))
    };
    let registered = fonts.clone();
    let _engine = Engine::start(
        config,
        move || Fonts(ApproxTextEngine, registered),
        FrameSink {
            batches: tx,
            frames: Arc::default(),
        },
    )
    .unwrap();
    until(&rx, "ready");
    // Only the font is read (images load where they show).
    assert_eq!(*read.lock().unwrap(), ["asset:/@app/fonts/Script.ttf"]);
    assert_eq!(
        *fonts.lock().unwrap(),
        [b"asset:/@app/fonts/Script.ttf".to_vec()]
    );
}

/// Counts display frames while a task awaits them.
#[component]
fn Frames() -> impl IntoElement {
    let frames = signal(0);
    // The frame `request_animation_frame` called back with.
    let called = signal(false);
    let start = Instant::now();
    request_animation_frame(move |at| called.set(at >= start));
    spawn(async move {
        for _ in 0..3 {
            next_frame().await;
            frames.update(|f| *f += 1);
        }
    });
    view().child(text(move || {
        format!("{} frames, called {}", frames.get(), called.get())
    }))
}

#[test]
fn tasks_wait_for_display_frames() {
    let (engine, rx, frames) = start::<Frames>(Environment::default());
    until(&rx, "0 frames, called false");
    // The engine asked the platform for a frame (once until it comes).
    let deadline = Instant::now() + Duration::from_secs(5);
    while frames.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(frames.load(Ordering::SeqCst), 1);
    for n in 1..=3 {
        engine.control().frame();
        let (texts, ..) = until(&rx, &format!("{n} frames, called true"));
        assert_eq!(
            texts.last().map(String::as_str),
            Some(format!("{n} frames, called true").as_str())
        );
    }
    // Each frame asked for the next; none is asked for once the task ended.
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(frames.load(Ordering::SeqCst), 3);
}

/// Flips the surface's direction on presses.
#[component]
fn Direction() -> impl IntoElement {
    let presses = stored(0_u32);
    view()
        .flex_1()
        .on_press(move |_: &()| {
            let n = presses.with(|p| {
                *p += 1;
                *p
            });
            match n {
                1 => force_rtl(true),
                2 => force_rtl(false),
                _ => allow_rtl(false),
            }
        })
        .child(text(|| if rtl() { "rtl" } else { "ltr" }))
}

#[test]
fn components_set_the_layout_direction() {
    let (engine, rx, _) = start::<Direction>(Environment::default());
    // A Rust app's surface takes the platform's direction.
    let (_, directions, host) = until(&rx, "ltr");
    assert_eq!(directions, [false]);
    let host = host.expect("the root component");
    let press = || {
        engine
            .control()
            .dispatch_event(host, EventKind::Press, EventPayload::None)
    };
    press();
    let (_, directions, _) = until(&rx, "rtl");
    assert_eq!(directions, [true]);
    press();
    let (_, directions, _) = until(&rx, "ltr");
    assert_eq!(directions, [false]);
    // A right-to-left language lays out right to left...
    engine.control().set_environment(Environment {
        rtl: true,
        ..Environment::default()
    });
    let (_, directions, _) = until(&rx, "rtl");
    assert_eq!(directions, [true]);
    // ...unless the app does not allow it.
    press();
    let (_, directions, _) = until(&rx, "ltr");
    assert_eq!(directions, [false]);
}

/// Shows the pixel ratio, the device and the locale.
#[component]
fn Device() -> impl IntoElement {
    view().child(text(|| {
        let (major, minor) = os_version();
        format!(
            "{} {major}.{minor} {:?} ×{} {} {}",
            os().name(),
            idiom(),
            pixel_ratio(),
            round_to_nearest_pixel(10.3),
            locale(),
        )
    }))
}

#[test]
fn components_read_the_pixel_ratio_the_device_and_the_locale() {
    let environment = Environment {
        pixel_ratio: 3.0,
        os: Os::Android,
        os_version: (34, 0),
        idiom: Idiom::Pad,
        locale: Locale::parse("de_DE").unwrap(),
        ..Environment::default()
    };
    let (engine, rx, _) = start::<Device>(environment);
    let shown = "android 34.0 Pad ×3 10.333333 de-DE";
    let (texts, ..) = until(&rx, shown);
    assert!(texts.iter().any(|t| t == shown), "{texts:?}");
    // The window moved to another screen: what reads the pixel ratio
    // follows.
    engine.control().set_environment(Environment {
        pixel_ratio: 2.0,
        ..environment
    });
    let shown = "android 34.0 Pad ×2 10.5 de-DE";
    let (texts, ..) = until(&rx, shown);
    assert!(texts.iter().any(|t| t == shown), "{texts:?}");
    // The user changed the language: what reads the locale follows.
    engine.control().set_environment(Environment {
        pixel_ratio: 2.0,
        locale: Locale::parse("zh-Hans_CN").unwrap(),
        ..environment
    });
    let shown = "android 34.0 Pad ×2 10.5 zh-Hans-CN";
    let (texts, ..) = until(&rx, shown);
    assert!(texts.iter().any(|t| t == shown), "{texts:?}");
}
