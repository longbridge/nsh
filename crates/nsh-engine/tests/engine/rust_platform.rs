//! The platform's services for Rust components on the engine: the fonts
//! the UI sets' crates ship are registered before the first render, tasks
//! wait for display frames, components set the layout direction and read
//! the pixel ratio, the device, the locale and whether the app is in use;
//! they speak, move the screen reader and set the status bar with their
//! dialogs.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use nsh_base::dialog;
use nsh_core::{
    ApproxTextEngine, AttributedText, Command, EventKind, EventPayload, MountBatch, MountItem,
    Points, ROOT_ID, Size, StatusBarSetting, StatusBarStyle, TextEngine, TextLayout,
    element::{AssetFile, AssetKind, Registry, Root},
    platform::{
        Idiom, Languages, Lifecycle, Locale, Os, RegionalPreferences, TemperatureUnit, TimeZone,
    },
    prelude::*,
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
        viewport: Size::new(px(390.0), px(844.0)),
        trace: false,
        network: None,
        websocket: None,
        refuse_connections: None,
        filesystem: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (nsh_core::Points(44.0), nsh_core::Points(96.0)),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        data_dir: None,
        cache: nsh_engine::CacheOptions::off(),
        storage: None,
        clock: Default::default(),
        context: Default::default(),
        pinned: Default::default(),
        clipboard: None,
        host: Default::default(),
        picker: None,
        speech: None,
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

    fn measure(&self, prepared: &Self::Prepared, max_width: Option<Points>) -> Size<Points> {
        self.0.measure(prepared, max_width)
    }

    fn layout(&self, prepared: &Self::Prepared, size: Size<Points>) -> TextLayout {
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

/// A Rust app's root, laid out in the surface's direction.
#[component]
fn Direction() -> impl IntoElement {
    view().flex_1().child(text("direction"))
}

#[test]
fn the_layout_direction_follows_the_language() {
    let (engine, rx, _) = start::<Direction>(Environment::default());
    // A Rust app's surface takes the platform's direction...
    let (_, directions, _) = until(&rx, "direction");
    assert_eq!(directions, [false]);
    // ...and a right-to-left language lays it out right to left, at once.
    let directions_after = |environment: Environment| {
        engine.control().set_environment(environment);
        let deadline = Instant::now() + Duration::from_secs(5);
        while let Ok(batch) = rx.recv_deadline(deadline) {
            let found = batch.items.iter().find_map(|item| match item {
                MountItem::LayoutDirection { rtl } => Some(*rtl),
                _ => None,
            });
            if found.is_some() {
                return found;
            }
        }
        None
    };
    let rtl = Environment {
        rtl: true,
        ..Environment::default()
    };
    assert_eq!(directions_after(rtl), Some(true));
    assert_eq!(directions_after(Environment::default()), Some(false));
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
            round_to_nearest_pixel(px(10.3)).0,
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

/// Counts its effect's runs while it shows whether the app is in use, as
/// work that pauses in the background does.
#[component]
fn InUse() -> impl IntoElement {
    let runs = signal(0);
    effect(move || {
        let _ = lifecycle();
        runs.update(|r| *r += 1);
    });
    view().child(text(move || {
        format!("{} after {} runs", lifecycle().name(), runs.get())
    }))
}

#[test]
fn components_follow_whether_the_app_is_in_use() {
    let (engine, rx, _) = start::<InUse>(Environment::default());
    until(&rx, "active after 1 runs");
    for (state, shown) in [
        (Lifecycle::Inactive, "inactive after 2 runs"),
        (Lifecycle::Background, "background after 3 runs"),
        (Lifecycle::Active, "active after 4 runs"),
    ] {
        engine.control().set_environment(Environment {
            lifecycle: state,
            ..Environment::default()
        });
        let (texts, ..) = until(&rx, shown);
        assert_eq!(texts.last().map(|t| t.as_ref()), Some(shown));
    }
    // Another part of the environment changing runs nothing that read only
    // the lifecycle.
    engine.control().set_environment(Environment {
        font_scale: 1.5,
        ..Environment::default()
    });
    std::thread::sleep(Duration::from_millis(100));
    let texts: Vec<_> = rx
        .try_iter()
        .flat_map(|batch| batch.items)
        .filter_map(|item| match item {
            MountItem::Text { text, .. } => Some(text.text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        texts.iter().all(|t| t == "active after 4 runs"),
        "{texts:?}"
    );
}

/// Greets in the first of the user's languages it has, as an app's
/// translated text does.
#[component]
fn Greeting() -> impl IntoElement {
    view().child(text(move || {
        let words = languages()
            .iter()
            .find_map(|l| match l.as_str().split('-').next() {
                Some("de") => Some("Willkommen"),
                Some("fr") => Some("Bienvenue"),
                _ => None,
            })
            .unwrap_or("Welcome");
        format!("{words} ({})", languages().as_slice().len())
    }))
}

#[test]
fn components_follow_the_preferred_languages() {
    let (engine, rx, _) = start::<Greeting>(Environment::default());
    until(&rx, "Welcome (1)");
    for (tags, shown) in [
        (&["ja-JP", "fr-FR", "de-DE"][..], "Bienvenue (3)"),
        (&["de_AT", "en-US"][..], "Willkommen (2)"),
    ] {
        engine.control().set_environment(Environment {
            languages: Languages::parse(tags.iter().copied()).unwrap(),
            ..Environment::default()
        });
        let (texts, ..) = until(&rx, shown);
        assert_eq!(texts.last().map(|t| t.as_ref()), Some(shown));
    }
}

/// Shows a time, a temperature and the week as the user reads them, and
/// counts the runs of an effect that reads the hour cycle alone, as a
/// clock's does.
#[component]
fn Forecast() -> impl IntoElement {
    let clock_runs = signal(0);
    effect(move || {
        let _ = hour_cycle();
        clock_runs.update(|r| *r += 1);
    });
    view().child(text(move || {
        let time = if hour_cycle().twelve_hour() {
            "3 PM"
        } else {
            "15:00"
        };
        let temperature = match temperature_unit() {
            TemperatureUnit::Celsius => "20 °C",
            TemperatureUnit::Fahrenheit => "68 °F",
        };
        format!(
            "{time}, {temperature}, weeks from {}, {} units, clock ran {}",
            first_day_of_week().number(),
            measurement_system().name(),
            clock_runs.get()
        )
    }))
}

#[test]
fn components_follow_the_regional_preferences() {
    let (engine, rx, _) = start::<Forecast>(Environment::default());
    until(&rx, "3 PM, 68 °F, weeks from 7, us units, clock ran 1");
    let celsius = RegionalPreferences {
        temperature_unit: TemperatureUnit::Celsius,
        ..RegionalPreferences::EN_US
    };
    let germany = RegionalPreferences::of(Locale::parse("de-DE").unwrap());
    for (regional, shown) in [
        // Only the unit changes: the clock's effect does not run again.
        (celsius, "3 PM, 20 °C, weeks from 7, us units, clock ran 1"),
        (
            germany,
            "15:00, 20 °C, weeks from 1, metric units, clock ran 2",
        ),
    ] {
        engine.control().set_environment(Environment {
            regional,
            ..Environment::default()
        });
        let (texts, ..) = until(&rx, shown);
        assert_eq!(texts.last().map(|t| t.as_ref()), Some(shown));
    }
}

/// Shows the time zone and counts its effect's runs, as a clock that
/// shows times in it does.
#[component]
fn Zoned() -> impl IntoElement {
    let runs = signal(0);
    effect(move || {
        let _ = time_zone();
        runs.update(|r| *r += 1);
    });
    view().child(text(move || {
        format!("{} after {} runs", time_zone(), runs.get())
    }))
}

#[test]
fn components_follow_the_time_zone() {
    let (engine, rx, _) = start::<Zoned>(Environment::default());
    until(&rx, "UTC after 1 runs");
    for (zone, shown) in [
        ("Europe/Berlin", "Europe/Berlin after 2 runs"),
        (
            "America/Argentina/Buenos_Aires",
            "America/Argentina/Buenos_Aires after 3 runs",
        ),
    ] {
        engine.control().set_environment(Environment {
            time_zone: TimeZone::parse(zone).unwrap(),
            ..Environment::default()
        });
        let (texts, ..) = until(&rx, shown);
        assert_eq!(texts.last().map(|t| t.as_ref()), Some(shown));
    }
}

/// Speaks, moves the screen reader and opens a dialog
/// that sets the status bar.
#[component]
fn Services() -> impl IntoElement {
    let note = element_ref();
    let open = signal(false);
    view()
        .flex_1()
        .child(text("Announce").on_press(|_: &()| announce_for_accessibility("Saved")))
        .child(text("Focus").on_press(move |_: &()| note.set_accessibility_focus()))
        .child(text("Open").on_press(move |_: &()| open.set(true)))
        .child(text("The note").element_ref(&note))
        .child(dynamic(move || {
            open.get().then(|| {
                dialog()
                    .status_bar(|| {
                        StatusBarSetting::default()
                            .foreground(StatusBarStyle::Light)
                            .hidden(true)
                    })
                    .child(text("Close").on_press(move |_: &()| open.set(false)))
            })
        }))
}

#[test]
fn components_speak_move_the_reader_and_set_the_status_bar() {
    let (engine, rx, _) = start::<Services>(Environment::default());
    let mut texts = HashMap::new();
    // Presses the text `label` (none: only the first render) and returns
    // the commands that follow.
    let mut step = |press: Option<&str>| {
        if let Some(label) = press {
            engine
                .control()
                .dispatch_event(texts[label], EventKind::Press, EventPayload::None);
        }
        let mut commands = Vec::new();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
            for item in batch.items {
                match item {
                    MountItem::Text { id, text, .. } => {
                        texts.insert(text.text.clone(), id);
                    }
                    MountItem::Command { id, command } => commands.push((id, command)),
                    _ => {}
                }
            }
        }
        commands
    };
    let status_bars = |commands: &[(u32, Command)]| -> Vec<(StatusBarStyle, bool)> {
        commands
            .iter()
            .filter_map(|(_, c)| match c {
                Command::StatusBar(config) => Some((config.style, config.hidden)),
                _ => None,
            })
            .collect()
    };
    // Nothing sets the status bar until the dialog opens.
    assert_eq!(status_bars(&step(None)), []);
    assert_eq!(
        step(Some("Announce")),
        [(ROOT_ID, Command::Announce("Saved".into()))]
    );
    let focused = step(Some("Focus"));
    assert_eq!(
        focused.iter().map(|c| &c.1).collect::<Vec<_>>(),
        [&Command::AccessibilityFocus]
    );
    // The dialog's setting while it shows; the default again once it
    // closed.
    assert_eq!(
        status_bars(&step(Some("Open"))),
        [(StatusBarStyle::Light, true)]
    );
    assert_eq!(
        status_bars(&step(Some("Close"))),
        [(StatusBarStyle::Default, false)]
    );
    // The reader's focus went to the note's view.
    assert_eq!(focused[0].0, texts["The note"]);
}
