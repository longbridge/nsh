//! Headless runs (feature `headless`): an app on an engine with no
//! platform, driven by steps a user's hands would take, its mount batches
//! seen as they come. Tests and `nsh trace` both run apps this way.
//!
//! [`Headless`] is the driver: it starts an app on an engine with display
//! frames at 60 Hz ([`start`]), applies its batches to a model of the
//! native views ([`Screen`]) and takes steps on what they show, named by
//! selectors (`id:save`, `label:Close`, `text:+1`, `view:12`, or a bare
//! string): [`Headless::press`], [`Headless::type_text`],
//! [`Headless::submit`], [`Headless::scroll`], [`Headless::wait`],
//! [`Headless::wait_for`]. Each step delivers the events a platform
//! would, in its order, and fails with what is on screen when it cannot
//! be taken or when the app throws. An observer ([`Headless::observe`])
//! sees every batch with the step it came in ([`print_batch`] prints it as
//! `nsh trace` does). [`StubNetwork`] and [`MemoryStorage`] stand in for
//! the device's network and storage.
//!
//! [`trace`] is `nsh trace` on the driver. The prebuilt `nsh` runs it
//! itself for apps it can serve (the UI sets it links). An app with Rust
//! crates of its own is run by a runner `nsh` generates for it (`nsh-cli`
//! `rust_app.rs`), which links the app's crates and calls [`runner`]: `nsh`
//! bundles and compiles the app (or, for a Rust app's root, nothing), then
//! starts the runner with a [`TraceRequest`] as its one argument. The
//! runner prints the batches on its standard output as `nsh trace` does;
//! its log lines, warnings and errors go to standard error as
//! [`RunnerMessage`]s, which `nsh` shows as its own. A Rust app's runner is
//! built without feature `js`: no QuickJS.

mod net;
mod screen;
mod stub;

use std::{
    io::Write,
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use nsh_core::{
    ApproxTextEngine, EventKind, EventPayload, MountBatch, MountItem, NodeId, PayloadValue, Size,
    SubmitBehavior,
    ui::{Registry, Root},
};
use serde_json::{Value, json};

use self::screen::Selector;
pub use self::{
    net::{DirectoryAssets, UreqNetwork},
    screen::Screen,
    stub::{MemoryStorage, StubNetwork, StubResponse},
};
use crate::{
    App, AppError, AppErrorKind, Assets, Engine, EngineConfig, EngineControl, Environment, Insets,
    MountSink,
};

/// How long [`Headless::wait_for`] and [`Headless::wait_until`] wait by
/// default, and a first render at most.
pub const WAIT: Duration = Duration::from_secs(5);
/// How long the app must render nothing after a step for it to be done.
const QUIET: Duration = Duration::from_millis(100);
/// How long a step waits for the app to be quiet at most: an app with a
/// running timer or animation never is.
const SETTLE: Duration = Duration::from_secs(2);
/// How long [`Headless::idle`] may take at most.
const MAX_IDLE: Duration = Duration::from_secs(10);
/// How far a scroll moves per display frame, in points.
const SCROLL_STEP: f32 = 10.0;

/// What a trace does after the first render (`nsh trace`'s flags).
#[derive(Debug, Clone, PartialEq)]
pub struct TraceOptions {
    /// Taps on the first pressable view (when `tap` is empty).
    pub taps: u32,
    /// Views to tap, in order (the steps' selectors).
    pub tap: Vec<String>,
    /// How long the app must be quiet before the taps, in milliseconds.
    pub idle_ms: u64,
    /// Run this long before the first tap and after each one, instead of
    /// waiting for quiet, in milliseconds.
    pub wait: Option<u64>,
    /// Keep running this long after the last tap, in milliseconds.
    pub duration: u64,
    /// Points to scroll the first scroll view by.
    pub scroll: Option<f32>,
    /// Viewport width in points.
    pub width: f32,
    /// Viewport height in points.
    pub height: f32,
    /// The simulated safe area insets.
    pub safe_area: Insets,
}

impl TraceOptions {
    fn to_value(&self) -> Value {
        let insets = &self.safe_area;
        json!({
            "taps": self.taps,
            "tap": self.tap,
            "idle_ms": self.idle_ms,
            "wait": self.wait,
            "duration": self.duration,
            "scroll": self.scroll,
            "width": self.width,
            "height": self.height,
            "safe_area": [insets.top, insets.right, insets.bottom, insets.left],
        })
    }

    fn from_value(value: &Value) -> Option<TraceOptions> {
        let number = |key: &str| value.get(key)?.as_f64();
        let insets: Vec<f32> = value
            .get("safe_area")?
            .as_array()?
            .iter()
            .map(|v| v.as_f64().map(|v| v as f32))
            .collect::<Option<_>>()?;
        let [top, right, bottom, left] = insets[..] else {
            return None;
        };
        Some(TraceOptions {
            taps: u32::try_from(value.get("taps")?.as_u64()?).ok()?,
            tap: value
                .get("tap")?
                .as_array()?
                .iter()
                .map(|t| t.as_str().map(str::to_owned))
                .collect::<Option<_>>()?,
            idle_ms: value.get("idle_ms")?.as_u64()?,
            wait: value.get("wait").and_then(Value::as_u64),
            duration: value.get("duration")?.as_u64()?,
            scroll: number("scroll").map(|v| v as f32),
            width: number("width")? as f32,
            height: number("height")? as f32,
            safe_area: Insets {
                top,
                right,
                bottom,
                left,
            },
        })
    }
}

/// Parses `--safe-area <top,right,bottom,left>` (one value: all four).
pub fn parse_insets(value: &str) -> Result<Insets, String> {
    let parts = value
        .split(',')
        .map(|p| p.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{e}: expected top,right,bottom,left in points"))?;
    match parts[..] {
        [all] => Ok(Insets {
            top: all,
            right: all,
            bottom: all,
            left: all,
        }),
        [top, right, bottom, left] => Ok(Insets {
            top,
            right,
            bottom,
            left,
        }),
        _ => Err("expected top,right,bottom,left in points (or one value for all)".into()),
    }
}

/// Checks `--tap`'s selectors before anything is built.
pub fn check_taps(taps: &[String]) -> Result<(), String> {
    taps.iter()
        .try_for_each(|s| Selector::parse(s).map(|_| ()))
        .map_err(|e| format!("--tap {e}"))
}

struct ChannelSink {
    batches: Sender<MountBatch>,
    errors: Sender<AppError>,
    /// Set once the engine runs; drives `requestAnimationFrame` at 60 Hz.
    control: Arc<OnceLock<EngineControl>>,
}

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.batches.send(batch);
    }

    fn report(&self, error: &AppError) {
        let _ = self.errors.send(error.clone());
    }

    fn request_frame(&self) {
        let control = self.control.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_micros(16_667));
            if let Some(control) = control.get() {
                control.frame();
            }
        });
    }
}

/// Starts `config`'s app on an engine with no platform that gets display
/// frames at 60 Hz while it asks for them, as on screen (timers,
/// animations and what waits for a frame run), laying text out with
/// [`ApproxTextEngine`]: the engine, and the batches it sends.
pub fn start(config: EngineConfig) -> std::io::Result<(Engine, Receiver<MountBatch>)> {
    launch(config).map(|(engine, batches, _)| (engine, batches))
}

fn launch(
    config: EngineConfig,
) -> std::io::Result<(Engine, Receiver<MountBatch>, Receiver<AppError>)> {
    let (batches, batches_rx) = crossbeam_channel::unbounded();
    let (errors, errors_rx) = crossbeam_channel::unbounded();
    let control = Arc::new(OnceLock::new());
    let sink = ChannelSink {
        batches,
        errors,
        control: control.clone(),
    };
    let engine = Engine::start(config, || ApproxTextEngine, sink)?;
    let _ = control.set(engine.control());
    Ok((engine, batches_rx, errors_rx))
}

/// Sees each batch a [`Headless`] applies, with the step it came in.
type Observer = Box<dyn FnMut(&str, &MountBatch) + Send>;

/// An app on a headless engine ([`start`]) and what it shows ([`Screen`]),
/// driven by steps: press views, type into inputs, scroll, wait for what
/// it shows.
///
/// ```ignore
/// let mut app = Headless::start(EngineConfig::new(Root::of::<my_app::App>())).unwrap();
/// app.wait_for("Count: 0")?;
/// app.press("Tap")?;
/// app.wait_for("Count: 1")?;
/// ```
///
/// A step takes the batches that came since the last one first (they
/// belong to it), then names itself the step of the batches that follow:
/// what [`Headless::observe`] sees. Steps that act on the app return once
/// it rendered nothing for 100 ms (2 s at most), and fail when the app
/// threw meanwhile: an exception that escaped a callback, a bundle that
/// did not load, a VM that was stopped.
pub struct Headless {
    engine: Engine,
    batches: Receiver<MountBatch>,
    errors: Receiver<AppError>,
    screen: Screen,
    step: String,
    applied: u64,
    observer: Option<Observer>,
}

impl Headless {
    /// Starts `config`'s app ([`start`]).
    pub fn start(config: EngineConfig) -> std::io::Result<Headless> {
        let (engine, batches, errors) = launch(config)?;
        Ok(Headless {
            engine,
            batches,
            errors,
            screen: Screen::default(),
            step: "open".to_owned(),
            applied: 0,
            observer: None,
        })
    }

    /// The engine: [`Engine::control`] sends it input.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// What is on screen, from the batches applied so far.
    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// The engine and the batches not applied yet, for a model of the
    /// test's own.
    pub fn into_parts(self) -> (Engine, Receiver<MountBatch>) {
        (self.engine, self.batches)
    }

    /// Calls `observer` with each batch applied from now on and the step
    /// it came in (`open` until the first step).
    pub fn observe(&mut self, observer: impl FnMut(&str, &MountBatch) + Send + 'static) {
        self.observer = Some(Box::new(observer));
    }

    /// Takes the batches that came, then names the step of those that
    /// follow.
    pub fn step(&mut self, name: impl Into<String>) {
        self.drain();
        self.step = name.into();
    }

    /// The batches applied so far.
    pub fn applied(&self) -> u64 {
        self.applied
    }

    fn apply(&mut self, batch: &MountBatch) {
        self.screen.apply(batch);
        self.applied += 1;
        if let Some(observer) = &mut self.observer {
            observer(&self.step, batch);
        }
        // A platform reports where a command moved a scroll view.
        for id in self.screen.take_scrolled() {
            if let Some(scroll) = self.screen.scroll(id) {
                self.engine
                    .control()
                    .scroll(id, scroll.x, scroll.y, scroll.applied);
            }
        }
    }

    /// Applies the batches that came, without waiting.
    fn drain(&mut self) {
        while let Ok(batch) = self.batches.try_recv() {
            self.apply(&batch);
        }
    }

    /// Applies the next batch, waiting `timeout` at most: the batch, or
    /// `None` when none came.
    pub fn next_batch(&mut self, timeout: Duration) -> Option<MountBatch> {
        let batch = self.batches.recv_timeout(timeout).ok()?;
        self.apply(&batch);
        Some(batch)
    }

    /// Applies batches until none comes for `quiet`, or for `limit` at
    /// most: false if the limit ended it.
    fn quiet(&mut self, quiet: Duration, limit: Duration) -> bool {
        let end = Instant::now() + limit;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            let timeout = quiet.min(left);
            match self.batches.recv_timeout(timeout) {
                Ok(batch) => self.apply(&batch),
                Err(RecvTimeoutError::Disconnected) => return true,
                Err(RecvTimeoutError::Timeout) => return timeout < left,
            }
            if Instant::now() >= end {
                return false;
            }
        }
    }

    /// Applies batches until none comes for 100 ms, or for 2 s at most (an
    /// app updating all the time never goes quiet): false if the limit
    /// ended it.
    pub fn settle(&mut self) -> bool {
        self.quiet(QUIET, SETTLE)
    }

    /// The step `idle`: applies batches until none comes for `quiet`, or
    /// for 10 s at most (false then). Long enough, it lets the engine's
    /// idle collection run (every second at most), so what the app keeps
    /// alive shows.
    pub fn idle(&mut self, quiet: Duration) -> Result<bool, String> {
        self.step("idle");
        let quiet = self.quiet(quiet, MAX_IDLE);
        self.check().map(|()| quiet)
    }

    /// Fails with the app's first error since the last check: what threw
    /// (an asset's failure is the log's only).
    fn check(&mut self) -> Result<(), String> {
        while let Ok(error) = self.errors.try_recv() {
            failure(error)?;
        }
        Ok(())
    }

    /// Applies the next batch, waiting until `deadline` at most: false
    /// when none came; an error when the app threw first.
    fn next_until(&mut self, deadline: Instant) -> Result<bool, String> {
        let timeout = deadline.saturating_duration_since(Instant::now());
        crossbeam_channel::select_biased! {
            recv(self.errors) -> error => {
                if let Ok(error) = error {
                    failure(error)?;
                }
                Ok(true)
            }
            recv(self.batches) -> batch => match batch {
                Ok(batch) => {
                    self.apply(&batch);
                    Ok(true)
                }
                Err(_) => Ok(false),
            },
            default(timeout) => Ok(false),
        }
    }

    /// Settles after a step and fails if the app threw.
    fn done(&mut self) -> Result<(), String> {
        self.settle();
        self.check()
    }

    /// The texts on screen, for errors.
    fn on_screen(&self) -> String {
        format!("on screen: {:?}", self.screen.texts())
    }

    /// The first render: waits for the app's first batch ([`WAIT`] at
    /// most), then until it is quiet.
    pub fn first_render(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + WAIT;
        while self.applied == 0 {
            if !self.next_until(deadline)? {
                return Err(format!("the app rendered nothing in {} s", WAIT.as_secs()));
            }
        }
        self.done()
    }

    /// The step `wait(<ms>)`: applies batches for `time` (timers and
    /// animations run).
    pub fn wait(&mut self, time: Duration) -> Result<(), String> {
        self.step(format!("wait({})", time.as_millis()));
        let end = Instant::now() + time;
        while let Ok(batch) = self.batches.recv_deadline(end) {
            self.apply(&batch);
        }
        self.check()
    }

    /// Applies batches until `done` holds for the screen, `timeout` at
    /// most; the error names `what` and the texts on screen.
    pub fn wait_until(
        &mut self,
        what: &str,
        timeout: Duration,
        done: impl Fn(&Screen) -> bool,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        self.drain();
        self.check()?;
        while !done(&self.screen) {
            if !self.next_until(deadline)? {
                return Err(format!(
                    "{what} never happened in {} ms; {}",
                    timeout.as_millis(),
                    self.on_screen()
                ));
            }
        }
        Ok(())
    }

    /// The step `waitFor(<selector>)`: applies batches until a view on
    /// screen matches `selector` ([`Screen::matches`]), for [`WAIT`] at
    /// most.
    pub fn wait_for(&mut self, selector: &str) -> Result<(), String> {
        self.wait_for_within(selector, WAIT)
    }

    /// [`Headless::wait_for`], waiting `timeout` at most.
    pub fn wait_for_within(&mut self, selector: &str, timeout: Duration) -> Result<(), String> {
        Selector::parse(selector)?;
        self.step(format!("waitFor({selector})"));
        self.wait_until(&format!("`{selector}` showing"), timeout, |s| {
            s.matches(selector) == Ok(true)
        })
    }

    /// Whether a view on screen shows the text `text`.
    pub fn shows(&self, text: &str) -> bool {
        self.screen.shows(text)
    }

    /// The step `press(<selector>)`: presses what `selector` names, as a
    /// tap does ([`Screen::press_target`]: `"Save"` presses the button
    /// whose test id, label or text it is), sending the view's
    /// `onPressIn`, `onPressOut` and `onPress`.
    pub fn press(&mut self, selector: &str) -> Result<(), String> {
        let parsed = Selector::parse(selector)?;
        self.step(format!("press({selector})"));
        let id = self.screen.pressable(&parsed).ok_or_else(|| {
            format!(
                "no pressable view matches `{selector}`; {}",
                self.on_screen()
            )
        })?;
        for kind in [EventKind::PressIn, EventKind::PressOut, EventKind::Press] {
            self.send(id, kind, EventPayload::None);
        }
        self.done()
    }

    /// The step `type(<selector>, <text>)`: types `text` at the end of the
    /// input `selector` names (the matching view, or the first input in
    /// it; `text:` also matches what an input holds or its placeholder),
    /// as a keyboard does: the input takes the focus, then each character
    /// is a key (`onKeyPress`) and an edit (`onChangeText`); a line break
    /// in a single-line input is the return key ([`Headless::submit`]). A
    /// character past `maxLength` is dropped.
    pub fn type_text(&mut self, selector: &str, text: &str) -> Result<(), String> {
        let id = self.input_step(selector, format!("type({selector}, {text:?})"))?;
        for c in text.chars() {
            let multiline = self.screen.input(id).is_some_and(|i| i.config.multiline);
            if c == '\n' && !multiline {
                self.return_key(id);
            } else {
                self.key(id, c);
            }
        }
        self.done()
    }

    /// The step `submit(<selector>)`: the return key in the input
    /// `selector` names, as its `submitBehavior` says: `onSubmitEditing`
    /// (and the focus leaves it, by default in a single line), or a line
    /// break in a multiline input.
    pub fn submit(&mut self, selector: &str) -> Result<(), String> {
        let id = self.input_step(selector, format!("submit({selector})"))?;
        self.return_key(id);
        self.done()
    }

    /// Starts a step on the input `selector` names and focuses it.
    fn input_step(&mut self, selector: &str, step: String) -> Result<NodeId, String> {
        let parsed = Selector::parse(selector)?;
        self.step(step);
        let id = self
            .screen
            .input_view(&parsed)
            .ok_or_else(|| format!("no input matches `{selector}`; {}", self.on_screen()))?;
        if self.screen.input(id).is_some_and(|i| !i.config.editable) {
            return Err(format!("the input `{selector}` is not editable"));
        }
        if self.screen.focused() != Some(id) {
            if let Some(old) = self.screen.focused() {
                self.send(old, EventKind::Blur, EventPayload::None);
            }
            self.screen.set_focused(Some(id));
            self.send(id, EventKind::Focus, EventPayload::None);
        }
        Ok(id)
    }

    /// Sends `kind` to view `id` if it reports it, as platforms do.
    fn send(&self, id: NodeId, kind: EventKind, payload: EventPayload) {
        if self.screen.hears(id, kind) {
            self.engine.control().dispatch_event(id, kind, payload);
        }
    }

    fn key_press(&self, id: NodeId, key: &str) {
        self.send(
            id,
            EventKind::KeyPress,
            EventPayload::Map(vec![("key", PayloadValue::Str(key.to_owned()))]),
        );
    }

    /// The key `c` in input `id`: its text grows by it, within
    /// `maxLength`.
    fn key(&mut self, id: NodeId, c: char) {
        self.key_press(
            id,
            &if c == '\n' {
                "Enter".into()
            } else {
                c.to_string()
            },
        );
        let Some(input) = self.screen.input_mut(id) else {
            return;
        };
        let length = input.text.encode_utf16().count() + c.len_utf16();
        if input
            .config
            .max_length
            .is_some_and(|max| length > max as usize)
        {
            return;
        }
        input.text.push(c);
        input.count += 1;
        let payload = EventPayload::Text {
            text: input.text.clone(),
            count: input.count,
        };
        // Always reported: the engine keeps the text for layout and for
        // dropping stale values, whether JS listens or not.
        self.engine
            .control()
            .dispatch_event(id, EventKind::ChangeText, payload);
    }

    /// The return key in input `id`.
    fn return_key(&mut self, id: NodeId) {
        let Some(input) = self.screen.input(id) else {
            return;
        };
        let behavior = input.config.submit();
        let payload = EventPayload::Text {
            text: input.text.clone(),
            count: input.count,
        };
        if behavior == SubmitBehavior::Newline {
            self.key(id, '\n');
            return;
        }
        self.key_press(id, "Enter");
        self.send(id, EventKind::SubmitEditing, payload);
        if behavior == SubmitBehavior::BlurAndSubmit {
            self.screen.set_focused(None);
            self.send(id, EventKind::Blur, EventPayload::None);
        }
    }

    /// The step `scroll(<selector>, <distance>)`: drags the scroll view
    /// `selector` names (the matching view, the nearest scroll view around
    /// it, else the first in it) by `distance` points along its axis
    /// (negative: back), 10 points per display frame, within its content.
    /// Fails if it cannot move that way at all.
    pub fn scroll(&mut self, selector: &str, distance: f32) -> Result<(), String> {
        let parsed = Selector::parse(selector)?;
        self.step(format!("scroll({selector}, {distance})"));
        let id = self
            .screen
            .scroll_view(&parsed)
            .ok_or_else(|| format!("no scroll view matches `{selector}`; {}", self.on_screen()))?;
        let at = |screen: &Screen| {
            screen
                .scroll(id)
                .map_or(0.0, |s| if s.horizontal { s.x } else { s.y })
        };
        let start = at(&self.screen);
        let target = (start + distance).clamp(0.0, self.screen.scroll_end(id));
        if (target - start).abs() < 0.5 {
            return Err(format!(
                "`{selector}` cannot scroll by {distance}: it is at {start} of {}",
                self.screen.scroll_end(id)
            ));
        }
        let offset = |screen: &Screen| {
            let s = screen.scroll(id);
            EventPayload::Scroll {
                x: s.map_or(0.0, |s| s.x),
                y: s.map_or(0.0, |s| s.y),
            }
        };
        self.send(id, EventKind::ScrollBeginDrag, offset(&self.screen));
        // The core may shift the offset meanwhile (`ScrollBy`): each frame
        // moves on from where the view is.
        let mut frames = ((target - start).abs() / SCROLL_STEP).ceil() as usize * 2 + 10;
        loop {
            let now = at(&self.screen);
            if (target - now).abs() < 0.5 || frames == 0 {
                break;
            }
            frames -= 1;
            let next = if target > now {
                (now + SCROLL_STEP).min(target)
            } else {
                (now - SCROLL_STEP).max(target)
            };
            let Some(scroll) = self.screen.scroll_mut(id) else {
                break;
            };
            if scroll.horizontal {
                scroll.x = next;
            } else {
                scroll.y = next;
            }
            let (x, y, applied) = (scroll.x, scroll.y, scroll.applied);
            self.engine.control().scroll(id, x, y, applied);
            // A frame that needs nothing produces no batch.
            self.next_batch(Duration::from_millis(50));
        }
        self.send(id, EventKind::ScrollEndDrag, offset(&self.screen));
        self.send(id, EventKind::ScrollEnd, offset(&self.screen));
        self.done()
    }
}

/// The error a step fails with for `error` of the app: none for an
/// asset's failure, which the log shows.
fn failure(error: AppError) -> Result<(), String> {
    match error.kind {
        AppErrorKind::Asset => Ok(()),
        _ => Err(format!("the app threw: {error}")),
    }
}

/// Prints `batch`, which came in step `step`, as `nsh trace` does: a line
/// naming the step and the number of items, then one line per item.
pub fn print_batch(out: &mut dyn Write, step: &str, batch: &MountBatch) {
    // A closed output (`| head`) ends nothing: the run goes on unseen.
    let _ = writeln!(out, "── {step}: {} items", batch.items.len());
    for item in &batch.items {
        let _ = match item {
            MountItem::Text {
                id, text, content, ..
            } => writeln!(
                out,
                "   Text {{ id: {id}, text: {:?}, content: {}x{} @ {},{} }}",
                text.text, content.width, content.height, content.x, content.y
            ),
            MountItem::Frame { id, frame } => writeln!(
                out,
                "   Frame {{ id: {id}, x: {}, y: {}, w: {}, h: {} }}",
                frame.x, frame.y, frame.width, frame.height
            ),
            other => writeln!(out, "   {other:?}"),
        };
    }
    let _ = out.flush();
}

/// Runs `app` (with the UI sets `ui_sets` and its files from `assets`)
/// headless as `options` say, on [`Headless`]: prints every mount batch
/// to `out` with its step, scrolls, presses views, runs timers. `warn` gets
/// what the run notices without failing (no pressable view, a press that
/// changed nothing, an app that never goes quiet).
pub fn trace(
    app: App,
    ui_sets: Vec<Registry>,
    assets: Arc<dyn Assets>,
    options: &TraceOptions,
    mut out: Box<dyn Write + Send>,
    warn: &mut dyn FnMut(&str),
) -> Result<(), String> {
    check_taps(&options.tap)?;
    let config = EngineConfig {
        viewport: Size::new(options.width, options.height),
        trace: true,
        network: Some(Arc::new(UreqNetwork::new())),
        assets: Some(assets),
        environment: Environment {
            safe_area: options.safe_area,
            ..Default::default()
        },
        ui_sets,
        ..EngineConfig::new(app)
    };
    let mut app = Headless::start(config).map_err(|e| e.to_string())?;
    app.observe(move |step, batch| print_batch(&mut out, step, batch));
    app.first_render()?;
    if let Some(distance) = options.scroll
        && let Some(id) = app.screen.first_scroll_view()
    {
        app.scroll(&format!("view:{id}"), distance)?;
    }
    let settle = |app: &mut Headless, quiet: Duration, warn: &mut dyn FnMut(&str)| {
        match options.wait {
            Some(ms) => app.wait(Duration::from_millis(ms)),
            None => app.idle(quiet).map(|quiet| {
                if !quiet {
                    warn(&format!(
                        "still rendering after {} s (a timer or an animation runs); --wait <ms> bounds it",
                        MAX_IDLE.as_secs()
                    ));
                }
            }),
        }
    };
    settle(&mut app, Duration::from_millis(options.idle_ms), warn)?;
    let taps: Vec<Option<&str>> = if options.tap.is_empty() {
        (0..options.taps).map(|_| None).collect()
    } else {
        options.tap.iter().map(|s| Some(s.as_str())).collect()
    };
    for (i, selector) in taps.into_iter().enumerate() {
        let selector = match selector {
            Some(selector) => selector.to_owned(),
            None => match app.screen.first_pressable() {
                Some(id) => format!("view:{id}"),
                None => {
                    warn("no pressable view found");
                    break;
                }
            },
        };
        let before = app.applied();
        app.press(&selector)
            .map_err(|e| format!("tap #{}: {e}", i + 1))?;
        if app.applied() == before {
            warn(&format!(
                "tap #{} (press({selector})) changed nothing",
                i + 1
            ));
        }
        if let Some(ms) = options.wait {
            app.wait(Duration::from_millis(ms))?;
        }
    }
    if options.duration > 0 {
        app.wait(Duration::from_millis(options.duration))?;
    }
    Ok(())
}

/// The version of the [`TraceRequest`] and [`RunnerMessage`] formats:
/// `nsh` and the runner it built speak the same.
pub const PROTOCOL: u32 = 1;

/// What `nsh` asks of an app's runner ([`runner`]), as its one argument
/// ([`TraceRequest::to_json`]).
#[derive(Debug, Clone, PartialEq)]
pub struct TraceRequest {
    /// The app's bytecode, compiled by `nsh` (`None`: the runner's Rust
    /// root).
    pub bytecode: Option<PathBuf>,
    /// The QuickJS build `nsh` compiled it for (`nsh_js::BYTECODE_VERSION`),
    /// which must be the runner's.
    pub bytecode_version: Option<String>,
    /// The bundle's source map, for errors at source positions.
    pub source_map: Option<PathBuf>,
    /// The app's source directory (its `asset:/` files).
    pub dir: PathBuf,
    /// Whether the runner forwards debug lines (`nsh -v`).
    pub verbose: bool,
    /// What the run does.
    pub options: TraceOptions,
}

impl TraceRequest {
    /// The request as JSON.
    pub fn to_json(&self) -> String {
        let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned());
        json!({
            "protocol": PROTOCOL,
            "bytecode": path(&self.bytecode),
            "bytecode_version": self.bytecode_version,
            "source_map": path(&self.source_map),
            "dir": self.dir.to_string_lossy(),
            "verbose": self.verbose,
            "options": self.options.to_value(),
        })
        .to_string()
    }

    /// Reads a request [`TraceRequest::to_json`] wrote, of this
    /// [`PROTOCOL`].
    pub fn from_json(text: &str) -> Result<TraceRequest, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        let protocol = value.get("protocol").and_then(Value::as_u64);
        if protocol != Some(u64::from(PROTOCOL)) {
            return Err(format!(
                "a request of protocol {}, and this runner speaks {PROTOCOL}: run the nsh it was built for",
                protocol.map_or("none".to_owned(), |p| p.to_string())
            ));
        }
        let path = |key: &str| value.get(key).and_then(Value::as_str).map(PathBuf::from);
        Ok(TraceRequest {
            bytecode: path("bytecode"),
            bytecode_version: value
                .get("bytecode_version")
                .and_then(Value::as_str)
                .map(str::to_owned),
            source_map: path("source_map"),
            dir: path("dir").ok_or("the request has no dir")?,
            verbose: value.get("verbose").and_then(Value::as_bool) == Some(true),
            options: value
                .get("options")
                .and_then(TraceOptions::from_value)
                .ok_or("the request's options are incomplete")?,
        })
    }
}

/// A line a runner writes to standard error for `nsh` to show as its own;
/// other lines (a panic's) are shown as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerMessage {
    /// A log record of the run (the app's `console.*`, `nsh::perf`).
    Log {
        /// Its level.
        level: log::Level,
        /// Its target (`js`, `nsh::perf`).
        target: String,
        /// The message.
        message: String,
    },
    /// Something the run noticed without failing.
    Warn(String),
    /// Why the run failed.
    Error(String),
}

/// The key that marks a runner's lines.
const MESSAGE_KEY: &str = "nsh_trace";

impl RunnerMessage {
    /// The message as one line (no newline).
    pub fn to_line(&self) -> String {
        match self {
            RunnerMessage::Log {
                level,
                target,
                message,
            } => json!({
                MESSAGE_KEY: "log",
                "level": level.as_str(),
                "target": target,
                "message": message,
            }),
            RunnerMessage::Warn(message) => json!({ MESSAGE_KEY: "warn", "message": message }),
            RunnerMessage::Error(message) => json!({ MESSAGE_KEY: "error", "message": message }),
        }
        .to_string()
    }

    /// The message `line` holds, if it is one.
    pub fn parse(line: &str) -> Option<RunnerMessage> {
        if !line.starts_with('{') {
            return None;
        }
        let value: Value = serde_json::from_str(line).ok()?;
        let message = value.get("message")?.as_str()?.to_owned();
        Some(match value.get(MESSAGE_KEY)?.as_str()? {
            "log" => RunnerMessage::Log {
                level: value.get("level")?.as_str()?.parse().ok()?,
                target: value.get("target")?.as_str()?.to_owned(),
                message,
            },
            "warn" => RunnerMessage::Warn(message),
            "error" => RunnerMessage::Error(message),
            _ => return None,
        })
    }
}

/// Forwards log records to standard error as [`RunnerMessage::Log`].
struct ForwardLogger;

impl log::Log for ForwardLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = RunnerMessage::Log {
            level: record.level(),
            target: record.target().to_owned(),
            message: record.args().to_string(),
        }
        .to_line();
        let _ = writeln!(std::io::stderr().lock(), "{line}");
    }

    fn flush(&self) {}
}

/// The `main` of an app's headless runner: runs the [`TraceRequest`] its
/// argument holds with `ui_sets` (the mobile UI and the app's sets), the
/// app's bytecode or, without it, `root` (a Rust app's). The batches go to
/// standard output, everything else to standard error as
/// [`RunnerMessage`]s.
pub fn runner(ui_sets: Vec<Registry>, root: Option<fn() -> Root>) -> ExitCode {
    let fail = |message: String| {
        let _ = writeln!(
            std::io::stderr().lock(),
            "{}",
            RunnerMessage::Error(message).to_line()
        );
        ExitCode::FAILURE
    };
    let Some(arg) = std::env::args().nth(1) else {
        return fail("the runner takes nsh's request as its argument".into());
    };
    let request = match TraceRequest::from_json(&arg) {
        Ok(request) => request,
        Err(e) => return fail(e),
    };
    static LOGGER: ForwardLogger = ForwardLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(if request.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    });
    let app = match (&request.bytecode, root) {
        (Some(bytecode), _) => match js_app(&request, bytecode) {
            Ok(app) => app,
            Err(e) => return fail(e),
        },
        (None, Some(root)) => App::Rust(root()),
        (None, None) => return fail("no bytecode, and this runner has no Rust root".into()),
    };
    let files = ui_sets
        .iter()
        .flat_map(Registry::asset_files)
        .map(|file| (file.path().to_owned(), PathBuf::from(file.source)))
        .collect();
    let assets = Arc::new(DirectoryAssets::new(request.dir.clone(), files));
    let mut warn = |message: &str| {
        let _ = writeln!(
            std::io::stderr().lock(),
            "{}",
            RunnerMessage::Warn(message.to_owned()).to_line()
        );
    };
    let out = Box::new(std::io::stdout());
    match trace(app, ui_sets, assets, &request.options, out, &mut warn) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

/// The JS app of `request`: the bytecode `nsh` compiled, at `bytecode`.
#[cfg(feature = "js")]
fn js_app(request: &TraceRequest, bytecode: &std::path::Path) -> Result<App, String> {
    if request.bytecode_version.as_deref() != Some(crate::BYTECODE_VERSION) {
        return Err(format!(
            "the bytecode is for {}, and this runner's QuickJS is {}",
            request.bytecode_version.as_deref().unwrap_or("no QuickJS"),
            crate::BYTECODE_VERSION
        ));
    }
    let bytes = std::fs::read(bytecode).map_err(|e| format!("read {}: {e}", bytecode.display()))?;
    // SAFETY: `nsh` compiled it for this runner just now, for the QuickJS
    // build it links (checked above).
    let bundle = unsafe { crate::Bundle::bytecode(bytes) };
    let map = request
        .source_map
        .as_ref()
        .and_then(|map| std::fs::read_to_string(map).ok());
    Ok(App::Js(match map {
        Some(map) => bundle.with_source_map(map),
        None => bundle,
    }))
}

/// A runner without feature `js` runs a Rust root only.
#[cfg(not(feature = "js"))]
fn js_app(_: &TraceRequest, _: &std::path::Path) -> Result<App, String> {
    Err("this runner has no JS runtime: it runs the app's Rust root".into())
}

#[cfg(test)]
mod tests {
    use nsh_core::ui::prelude::*;

    use super::*;

    /// A count and a button that adds one.
    #[component]
    fn Tally() -> impl IntoElement {
        let count = signal(0_u32);
        view()
            .child(text(move || format!("Count: {}", count.get())))
            .child(
                view()
                    .on_press(move || count.update(|c| *c += 1))
                    .child(text("Add")),
            )
    }

    #[test]
    fn steps_press_what_a_text_is_in_and_wait_for_what_it_shows() {
        let mut app = Headless::start(EngineConfig::new(Root::of::<Tally>())).unwrap();
        let steps = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = steps.clone();
        app.observe(move |step, _| seen.lock().unwrap().push(step.to_owned()));
        app.first_render().unwrap();
        app.wait_for("Count: 0").unwrap();
        // The text is in the pressable view: the press goes to the view.
        app.press("Add").unwrap();
        app.wait_for("text:Count: 1").unwrap();
        app.press("text:Add").unwrap();
        app.wait_for("Count: 2").unwrap();
        assert!(!app.shows("Count: 1"));
        assert_eq!(app.screen().texts(), ["Count: 2", "Add"]);
        assert_eq!(app.screen().press_target("Nothing"), Ok(None));
        assert!(app.screen().press_target("view:x").is_err());
        let error = app.press("Nothing").unwrap_err();
        assert_eq!(
            error,
            "no pressable view matches `Nothing`; on screen: [\"Count: 2\", \"Add\"]"
        );
        let error = app
            .wait_for_within("Count: 9", Duration::from_millis(50))
            .unwrap_err();
        assert!(error.starts_with("`Count: 9` showing never happened in 50 ms; on screen: "));
        let steps = steps.lock().unwrap();
        assert_eq!(steps.first().map(String::as_str), Some("open"));
        assert!(steps.iter().any(|s| s == "press(Add)"), "{steps:?}");
        assert!(steps.iter().any(|s| s == "press(text:Add)"), "{steps:?}");
    }

    /// A name field that says hello and keeps what was sent.
    #[component]
    fn Form() -> impl IntoElement {
        let name = signal(String::new());
        let sent = signal(String::new());
        let keys = signal(0_u32);
        view()
            .child(
                input()
                    .test_id("name")
                    .placeholder("Your name")
                    .max_length(5_u32)
                    .value(move || name.get())
                    .on_change_text(move |text: &str| name.set(text.to_owned()))
                    .on_key_press(move |_: &str| keys.update(|k| *k += 1))
                    .on_submit_editing(move |text: &str| {
                        sent.set(text.to_owned());
                        name.set(String::new());
                    }),
            )
            .child(text(move || format!("Hello, {}", name.get())))
            .child(text(move || format!("Sent: {}", sent.get())))
            .child(text(move || format!("Keys: {}", keys.get())))
            .child(input().test_id("note").editable(false))
    }

    #[test]
    fn typing_edits_an_input_key_by_key_and_submits_it() {
        let mut app = Headless::start(EngineConfig::new(Root::of::<Form>())).unwrap();
        app.first_render().unwrap();
        // By its placeholder, while it is empty.
        app.type_text("text:Your name", "Ada").unwrap();
        app.wait_for("Hello, Ada").unwrap();
        assert_eq!(app.screen().input_text("id:name"), Ok(Some("Ada")));
        // At the end of what it holds, cut at maxLength.
        app.type_text("id:name", " Lovelace").unwrap();
        app.wait_for("Hello, Ada L").unwrap();
        app.wait_for("Keys: 12").unwrap();
        app.submit("id:name").unwrap();
        app.wait_for("Sent: Ada L").unwrap();
        // The app cleared it: the value it set wins.
        app.wait_for("Hello, ").unwrap();
        assert_eq!(app.screen().input_text("id:name"), Ok(Some("")));
        // A line break in a single line is the return key.
        app.type_text("id:name", "Bo\n").unwrap();
        app.wait_for("Sent: Bo").unwrap();
        assert_eq!(
            app.type_text("id:note", "x").unwrap_err(),
            "the input `id:note` is not editable"
        );
        assert!(
            app.type_text("id:none", "x")
                .unwrap_err()
                .starts_with("no input matches `id:none`")
        );
    }

    /// A list of rows taller than the screen, saying where it is.
    #[component]
    fn Rows() -> impl IntoElement {
        let at = signal(0.0_f32);
        view()
            .flex_1()
            .child(text(move || format!("At {}", at.get())))
            .child(
                scroll()
                    .test_id("rows")
                    .flex_1()
                    .on_scroll(move |e: &ScrollEvent| at.set(e.y))
                    .children((0..100).map(|i| view().h(px(50.0)).child(text(format!("Row {i}"))))),
            )
    }

    #[test]
    fn scrolling_moves_a_scroll_view_within_its_content() {
        let mut app = Headless::start(EngineConfig::new(Root::of::<Rows>())).unwrap();
        app.first_render().unwrap();
        // A row names the scroll view it is in.
        app.scroll("Row 3", 1000.0).unwrap();
        app.wait_for("At 1000").unwrap();
        app.scroll("id:rows", -400.0).unwrap();
        app.wait_for("At 600").unwrap();
        // Not past the end: 100 rows of 50 in what the screen leaves.
        app.scroll("id:rows", 100_000.0).unwrap();
        let end = app
            .screen()
            .scroll_end(app.screen().first_scroll_view().unwrap());
        assert!(end > 4000.0 && end < 5000.0, "{end}");
        app.wait_for(&format!("At {end}")).unwrap();
        let error = app.scroll("id:rows", 10.0).unwrap_err();
        assert!(
            error.starts_with("`id:rows` cannot scroll by 10: it is at "),
            "{error}"
        );
    }

    #[test]
    fn a_step_fails_when_the_app_throws() {
        let app = crate::Bundle::source("throw new Error('boom')".into());
        let mut app = Headless::start(EngineConfig::new(app)).unwrap();
        let error = app.first_render().unwrap_err();
        assert!(error.starts_with("the app threw: Error: boom"), "{error}");
    }

    #[test]
    fn insets_parse() {
        let i = parse_insets("47,0,34,0").unwrap();
        assert_eq!((i.top, i.right, i.bottom, i.left), (47.0, 0.0, 34.0, 0.0));
        assert_eq!(parse_insets("10").unwrap().left, 10.0);
        assert!(parse_insets("1,2").is_err());
    }

    /// A counter in Rust: what a Rust app's runner mounts.
    #[nsh_core::ui::component]
    fn Counter() -> impl nsh_core::ui::IntoElement {
        use nsh_core::ui::prelude::*;
        let count = signal(0_u32);
        view()
            .on_press(move || count.update(|c| *c += 1))
            .child(text(move || format!("Count {}", count.get())))
    }

    #[test]
    fn a_rust_root_is_traced_with_its_taps() {
        let options = TraceOptions {
            taps: 1,
            tap: Vec::new(),
            idle_ms: 0,
            wait: None,
            duration: 0,
            scroll: None,
            width: 390.0,
            height: 844.0,
            safe_area: Insets::default(),
        };
        let out = Buffer::default();
        let mut warnings = Vec::new();
        trace(
            App::Rust(Root::of::<Counter>()),
            Vec::new(),
            Arc::new(DirectoryAssets::new(PathBuf::from("."), Vec::new())),
            &options,
            Box::new(out.clone()),
            &mut |w| warnings.push(w.to_owned()),
        )
        .unwrap();
        let out = out.text();
        assert!(out.starts_with("── open: "), "{out}");
        assert!(out.contains("text: \"Count 0\""), "{out}");
        assert!(out.contains("── press(view:"), "{out}");
        assert!(out.contains("text: \"Count 1\""), "{out}");
        assert!(warnings.is_empty(), "{warnings:?}");
        // An unknown view fails the run, saying which tap.
        let options = TraceOptions {
            tap: vec!["label:Nope".into()],
            ..options
        };
        let error = trace(
            App::Rust(Root::of::<Counter>()),
            Vec::new(),
            Arc::new(DirectoryAssets::new(PathBuf::from("."), Vec::new())),
            &options,
            Box::new(std::io::sink()),
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(
            error,
            "tap #1: no pressable view matches `label:Nope`; on screen: [\"Count 0\"]"
        );
    }

    /// Output shared with the test that reads it.
    #[derive(Clone, Default)]
    struct Buffer(Arc<std::sync::Mutex<Vec<u8>>>);

    impl Buffer {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn requests_and_messages_survive_their_lines() {
        let request = TraceRequest {
            bytecode: Some(PathBuf::from("C:\\app\\.nsh\\rust\\trace\\bundle.nshc")),
            bytecode_version: Some("rquickjs-0.14.0/quickjs-ng-0.16.2/le64".into()),
            source_map: None,
            dir: PathBuf::from("/apps/charts"),
            verbose: true,
            options: TraceOptions {
                taps: 2,
                tap: vec!["label:Plan".into(), "text:\"quoted\"".into()],
                idle_ms: 1200,
                wait: Some(500),
                duration: 0,
                scroll: Some(3000.0),
                width: 390.0,
                height: 844.0,
                safe_area: Insets {
                    top: 47.0,
                    right: 0.0,
                    bottom: 34.0,
                    left: 0.0,
                },
            },
        };
        assert_eq!(
            TraceRequest::from_json(&request.to_json()).unwrap(),
            request
        );
        let mut other: Value = serde_json::from_str(&request.to_json()).unwrap();
        other["protocol"] = (PROTOCOL + 1).into();
        let error = TraceRequest::from_json(&other.to_string()).unwrap_err();
        assert!(
            error.contains(&format!("protocol {}", PROTOCOL + 1)),
            "{error}"
        );
        for message in [
            RunnerMessage::Log {
                level: log::Level::Info,
                target: "js".into(),
                message: "line one\nline two".into(),
            },
            RunnerMessage::Warn("no pressable view found".into()),
            RunnerMessage::Error("the app rendered nothing".into()),
        ] {
            let line = message.to_line();
            assert!(!line.contains('\n'), "{line}");
            assert_eq!(RunnerMessage::parse(&line), Some(message));
        }
        assert_eq!(
            RunnerMessage::parse("thread 'main' panicked at src/lib.rs:1:1"),
            None
        );
        assert_eq!(RunnerMessage::parse("{\"other\": 1}"), None);
    }
}
