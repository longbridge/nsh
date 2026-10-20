//! Headless runs: an app on an engine with no platform, driven by steps a
//! user's hands would take, its mount batches seen as they come.
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
//! `nsh test --trace` does). [`StubNetwork`] and [`MemoryStorage`] stand in for
//! the device's network and storage.
//!
//! `nsh test` runs apps this way (feature `js`: [`crate::run_file`]),
//! printing the batches under their steps with `--trace`; so do Rust tests
//! of an app's crate, which need no QuickJS for a Rust root.

mod net;
mod screen;
mod stub;

use std::{
    io::Write,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use nsh_core::{
    ApproxTextEngine, Command, EventKind, EventPayload, MountBatch, MountItem, NodeId,
    PayloadValue, Point, Points, SubmitBehavior,
    geometry::{point, px},
};
use nsh_engine::{AppError, AppErrorKind, Engine, EngineConfig, EngineControl, MountSink};

use self::screen::Selector;
pub use self::{
    net::{DirectoryAssets, UreqNetwork},
    screen::Screen,
    stub::{MemoryStorage, StubNetwork, StubResponse},
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
/// A display frame at 60 Hz.
const FRAME: Duration = Duration::from_micros(16_667);
/// How far a scroll moves per display frame, in points.
const SCROLL_STEP: Points = Points(10.0);

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
            std::thread::sleep(FRAME);
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
    exited: bool,
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
            exited: false,
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
        self.exited |= batch.items.iter().any(|item| {
            matches!(
                item,
                MountItem::Command {
                    command: Command::ExitApp,
                    ..
                }
            )
        });
        self.applied += 1;
        if let Some(observer) = &mut self.observer {
            observer(&self.step, batch);
        }
        // A platform reports where a command moved a scroll view.
        for id in self.screen.take_scrolled() {
            if let Some(scroll) = self.screen.scroll(id) {
                self.engine
                    .control()
                    .scroll(id, point(scroll.x, scroll.y), scroll.applied);
            }
        }
    }

    /// Applies the batches that came, without waiting: the screen as it
    /// is now.
    pub fn catch_up(&mut self) {
        self.drain();
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
        if self.exited {
            return None;
        }
        let batch = self.batches.recv_timeout(timeout).ok()?;
        self.apply(&batch);
        Some(batch)
    }

    /// Applies batches until none comes for `quiet`, or for `limit` at
    /// most: false if the limit ended it.
    fn quiet(&mut self, quiet: Duration, limit: Duration) -> bool {
        if self.exited {
            return true;
        }
        let end = Instant::now() + limit;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            let timeout = quiet.min(left);
            match self.batches.recv_timeout(timeout) {
                Ok(batch) => {
                    self.apply(&batch);
                    if self.exited {
                        return true;
                    }
                }
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
        if self.exited {
            return Ok(false);
        }
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
        while !self.exited {
            let Ok(batch) = self.batches.recv_deadline(end) else {
                break;
            };
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
    pub fn scroll(&mut self, selector: &str, distance: Points) -> Result<(), String> {
        let parsed = Selector::parse(selector)?;
        self.step(format!("scroll({selector}, {distance})"));
        let id = self
            .screen
            .scroll_view(&parsed)
            .ok_or_else(|| format!("no scroll view matches `{selector}`; {}", self.on_screen()))?;
        let at = |screen: &Screen| {
            screen
                .scroll(id)
                .map_or(Points::ZERO, |s| if s.horizontal { s.x } else { s.y })
        };
        let start = at(&self.screen);
        let target = (start + distance).clamp(Points::ZERO, self.screen.scroll_end(id));
        if (target - start).abs() < px(0.5) {
            return Err(format!(
                "`{selector}` cannot scroll by {}: it is at {} of {}",
                distance.0,
                start.0,
                self.screen.scroll_end(id).0
            ));
        }
        let offset = |screen: &Screen| {
            let s = screen.scroll(id);
            EventPayload::Scroll(s.map_or(Point::default(), |s| point(s.x, s.y)))
        };
        self.send(id, EventKind::ScrollBeginDrag, offset(&self.screen));
        // The core may shift the offset meanwhile (`ScrollBy`): each frame
        // moves on from where the view is.
        let mut frames = ((target - start).abs() / SCROLL_STEP).ceil() as usize * 2 + 10;
        loop {
            if self.exited {
                break;
            }
            let now = at(&self.screen);
            if (target - now).abs() < px(0.5) || frames == 0 {
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
            self.engine.control().scroll(id, point(x, y), applied);
            // One display frame per step; a frame that needs nothing
            // produces no batch, and a late one is applied with the next.
            self.next_batch(FRAME);
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

/// Prints `batch`, which came in step `step`, as `nsh test --trace` does: a line
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
                text.text,
                content.size.width.0,
                content.size.height.0,
                content.origin.x.0,
                content.origin.y.0
            ),
            MountItem::Frame { id, frame } => writeln!(
                out,
                "   Frame {{ id: {id}, x: {}, y: {}, w: {}, h: {} }}",
                frame.origin.x.0, frame.origin.y.0, frame.size.width.0, frame.size.height.0
            ),
            other => writeln!(out, "   {other:?}"),
        };
    }
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use nsh_core::ui::{Root, prelude::*};

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
                    .on_scroll(move |e: &ScrollEvent| at.set(e.offset.y.0))
                    .children((0..100).map(|i| view().h(px(50.0)).child(text(format!("Row {i}"))))),
            )
    }

    #[test]
    fn scrolling_moves_a_scroll_view_within_its_content() {
        let mut app = Headless::start(EngineConfig::new(Root::of::<Rows>())).unwrap();
        app.first_render().unwrap();
        // A row names the scroll view it is in.
        app.scroll("Row 3", px(1000.0)).unwrap();
        app.wait_for("At 1000").unwrap();
        app.scroll("id:rows", px(-400.0)).unwrap();
        app.wait_for("At 600").unwrap();
        // Not past the end: 100 rows of 50 in what the screen leaves.
        app.scroll("id:rows", px(100_000.0)).unwrap();
        let end = app.screen().scroll_end(
            app.screen()
                .scroll_view(&Selector::TestId("rows".into()))
                .unwrap(),
        );
        assert!(end > px(4000.0) && end < px(5000.0), "{end}");
        app.wait_for(&format!("At {}", end.0)).unwrap();
        let error = app.scroll("id:rows", px(10.0)).unwrap_err();
        assert!(
            error.starts_with("`id:rows` cannot scroll by 10: it is at "),
            "{error}"
        );
    }

    #[cfg(feature = "js")]
    #[test]
    fn a_step_fails_when_the_app_throws() {
        let app = nsh_engine::Bundle::source("throw new Error('boom')".into());
        let mut app = Headless::start(EngineConfig::new(app)).unwrap();
        let error = app.first_render().unwrap_err();
        assert!(error.starts_with("the app threw: Error: boom"), "{error}");
    }

    #[cfg(feature = "js")]
    #[test]
    fn process_exit_ends_a_timed_headless_wait() {
        use nsh_core::platform::Os;

        let bundle = nsh_engine::Bundle::source(
            "import process from 'process'; setTimeout(() => process.exit(0), 10);".into(),
        )
        .with_capabilities_json(Some(r#"{"process":{"exit":true}}"#))
        .unwrap();
        let mut config = EngineConfig::new(bundle);
        config.environment.os = Os::MacOs;
        let mut app = Headless::start(config).unwrap();
        let started = Instant::now();
        app.wait(Duration::from_secs(5)).unwrap();
        assert!(app.screen().sent(&Command::ExitApp));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
