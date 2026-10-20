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

mod animation;
mod net;
mod screen;
mod stub;

use std::{
    io::Write,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{
    ApproxTextEngine, Command, Dialog, DialogButtonStyle, EventKind, EventPayload, MountBatch,
    MountItem, NodeId, PayloadValue, ROOT_ID, SubmitBehavior,
};
use nsh_engine::{AppError, AppErrorKind, Engine, EngineConfig, EngineControl, MountSink};

use self::{
    animation::{Animator, FRAME},
    screen::{Back, Selector},
};
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
/// How far a scroll moves per display frame, in points.
const SCROLL_STEP: f32 = 10.0;

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

/// Answers a dialog ([`Headless::on_dialog`]).
type DialogHandler = Box<dyn FnMut(&Dialog) -> DialogAnswer + Send>;

/// The answer to a dialog (`Alert.alert`, an action sheet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogAnswer {
    /// Press the button with this text.
    Press(String),
    /// Close it without a button (tapping outside, the back button).
    Dismiss,
}

/// Who answers the dialogs a step shows.
enum Dialogs {
    /// [`default_answer`].
    Default,
    /// A test's handler.
    Handler(DialogHandler),
    /// The caller, after the step ([`Headless::answer_dialog`]).
    Caller,
}

/// What [`Headless::pump`] saw.
enum Pumped {
    /// A batch, applied.
    Batch,
    /// The app threw.
    Error,
    /// The deadline passed.
    Timeout,
    /// The app left: nothing more comes.
    Exited,
}

/// An app on a headless engine ([`start`]) and what it shows ([`Screen`]),
/// driven by steps: press views, type into inputs, scroll, go back, wait
/// for what it shows.
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
/// it rendered nothing for 100 ms and no native animation runs (2 s at
/// most), and fail when the app threw meanwhile (an exception that
/// escaped a callback, a bundle that did not load, a VM that was stopped)
/// or asked what only a real platform answers (an image's size).
///
/// It plays the platform's part the NSH protocol defines and nothing
/// else: the native animation graph runs frame by frame (`nsh-animation`),
/// a back pops screens and closes modals, dialogs are answered (by the
/// test, else [`default_answer`]), the clipboard holds text. Images are
/// not loaded: no `onLoad` comes.
pub struct Headless {
    engine: Engine,
    batches: Receiver<MountBatch>,
    errors: Receiver<AppError>,
    /// The app's errors seen while waiting, for the next check.
    seen: Vec<AppError>,
    screen: Screen,
    animator: Animator,
    dialogs: Dialogs,
    step: String,
    applied: u64,
    /// The app left (`process.exit`, `Command::ExitApp`): no more batches
    /// come, and waits end.
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
            seen: Vec::new(),
            screen: Screen::default(),
            animator: Animator::default(),
            dialogs: Dialogs::Default,
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

    /// Answers the dialogs steps show with `handler`, instead of
    /// [`default_answer`].
    pub fn on_dialog(&mut self, handler: impl FnMut(&Dialog) -> DialogAnswer + Send + 'static) {
        self.dialogs = Dialogs::Handler(Box::new(handler));
    }

    /// Leaves the dialogs steps show to the caller: a step returns with
    /// one shown ([`Headless::pending_dialog`]) until
    /// [`Headless::answer_dialog`] answers it.
    pub fn answer_dialogs_myself(&mut self) {
        self.dialogs = Dialogs::Caller;
    }

    /// The oldest dialog shown and not answered yet.
    pub fn pending_dialog(&self) -> Option<&Dialog> {
        self.screen.pending_dialog()
    }

    /// Answers the oldest dialog not answered yet with `answer`, as the
    /// user would, then waits until the app is quiet; an error for a
    /// button it has not, or a dialog that cannot be dismissed.
    pub fn answer_dialog(&mut self, answer: DialogAnswer) -> Result<(), String> {
        self.answer(answer)?;
        self.done()
    }

    /// Puts `text` on the clipboard (`None`: nothing), as the user copying.
    pub fn set_clipboard(&mut self, text: Option<String>) {
        self.screen.set_clipboard(text);
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
        for item in &batch.items {
            self.animator.apply(item);
        }
        self.applied += 1;
        if let Some(observer) = &mut self.observer {
            observer(&self.step, batch);
        }
        self.react();
    }

    /// Does what a platform does after a batch: reports where commands
    /// moved scroll views, tells modals they show, answers clipboard reads,
    /// goes back when the app asked, reports finished animations.
    fn react(&mut self) {
        let control = self.engine.control();
        for id in self.screen.take_scrolled() {
            if let Some(scroll) = self.screen.scroll(id) {
                let (x, y) = (scroll.x, scroll.y);
                control.scroll(id, x, y, scroll.applied);
                self.animator.scrolled(id, x, y);
            }
        }
        for modal in self.screen.take_presented() {
            self.send(modal, EventKind::Show, EventPayload::None);
        }
        for (id, has_only) in self.screen.take_clipboard_reads() {
            let text = self.screen.clipboard().map(str::to_owned);
            let mut fields = vec![
                ("id", PayloadValue::Number(f64::from(id))),
                ("has", PayloadValue::Bool(text.is_some())),
            ];
            if let Some(text) = text.filter(|_| !has_only) {
                fields.push(("text", PayloadValue::Str(text)));
            }
            control.dispatch_event(ROOT_ID, EventKind::Clipboard, EventPayload::Map(fields));
        }
        if self.screen.take_back_asked() {
            self.go_back();
        }
        self.report_finished();
    }

    fn report_finished(&mut self) {
        let finished = self.animator.take_finished();
        if !finished.is_empty() {
            self.engine.control().animations_finished(finished);
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
        self.animator.tick();
        self.report_finished();
    }

    /// Applies the next batch, waiting until `deadline` at most, while the
    /// native animations run frame by frame; the app's errors are kept for
    /// the next check.
    fn pump(&mut self, deadline: Instant) -> Pumped {
        loop {
            if self.exited {
                return Pumped::Exited;
            }
            let wake = self
                .animator
                .next_frame()
                .map_or(deadline, |frame| frame.min(deadline));
            let timeout = wake.saturating_duration_since(Instant::now());
            crossbeam_channel::select_biased! {
                recv(self.errors) -> error => {
                    if let Ok(error) = error {
                        self.seen.push(error);
                        return Pumped::Error;
                    }
                }
                recv(self.batches) -> batch => match batch {
                    Ok(batch) => {
                        self.apply(&batch);
                        return Pumped::Batch;
                    }
                    Err(_) => return Pumped::Timeout,
                },
                default(timeout) => {}
            }
            self.animator.tick();
            self.report_finished();
            if Instant::now() >= deadline {
                return Pumped::Timeout;
            }
        }
    }

    /// Applies the next batch, waiting `timeout` at most: the batch, or
    /// `None` when none came.
    pub fn next_batch(&mut self, timeout: Duration) -> Option<MountBatch> {
        if self.exited {
            return None;
        }
        let deadline = Instant::now() + timeout;
        let wait = deadline.saturating_duration_since(Instant::now());
        let batch = self.batches.recv_timeout(wait).ok()?;
        self.apply(&batch);
        Some(batch)
    }

    /// Applies batches until none comes for `quiet` and no native
    /// animation runs, or for `limit` at most: false if the limit ended it.
    fn quiet(&mut self, quiet: Duration, limit: Duration) -> bool {
        let end = Instant::now() + limit;
        let mut last = Instant::now();
        loop {
            match self.pump((last + quiet).min(end)) {
                Pumped::Batch => last = Instant::now(),
                Pumped::Error | Pumped::Exited => return true,
                Pumped::Timeout => {
                    let now = Instant::now();
                    if !self.animator.running() && now >= last + quiet {
                        return true;
                    }
                    if now >= end {
                        return false;
                    }
                }
            }
        }
    }

    /// Applies batches until none comes for 100 ms and no native animation
    /// runs, or for 2 s at most (an app updating all the time never goes
    /// quiet): false if the limit ended it.
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

    /// Fails with the app's first error since the last check (an asset's
    /// failure is the log's only), or with what it asked that only a real
    /// platform answers.
    fn check(&mut self) -> Result<(), String> {
        while let Ok(error) = self.errors.try_recv() {
            self.seen.push(error);
        }
        for error in std::mem::take(&mut self.seen) {
            failure(error)?;
        }
        if let Some(what) = self.screen.take_unsupported().first() {
            return Err(format!(
                "the app asked for {what}, which only a real platform answers: headless runs have none"
            ));
        }
        Ok(())
    }

    /// Settles after a step, answers the dialogs it showed (unless the
    /// caller answers them), and fails if the app threw.
    fn done(&mut self) -> Result<(), String> {
        self.settle();
        self.check()?;
        loop {
            let answer = match (&mut self.dialogs, self.screen.pending_dialog()) {
                (_, None) | (Dialogs::Caller, _) => return Ok(()),
                (Dialogs::Default, Some(dialog)) => default_answer(dialog)?,
                (Dialogs::Handler(handler), Some(dialog)) => handler(dialog),
            };
            self.answer(answer)?;
            self.settle();
            self.check()?;
        }
    }

    /// Answers the oldest dialog not answered yet.
    fn answer(&mut self, answer: DialogAnswer) -> Result<(), String> {
        let dialog = self.screen.pending_dialog().ok_or("no dialog to answer")?;
        let index = match &answer {
            DialogAnswer::Press(text) => dialog
                .buttons
                .iter()
                .position(|b| b.text == *text)
                .ok_or_else(|| {
                    let buttons: Vec<&str> =
                        dialog.buttons.iter().map(|b| b.text.as_str()).collect();
                    format!(
                        "the dialog {} has no button {text:?}: {buttons:?}",
                        describe(dialog)
                    )
                })? as f64,
            DialogAnswer::Dismiss if dialog.cancelable => -1.0,
            DialogAnswer::Dismiss => {
                return Err(format!(
                    "the dialog {} cannot be dismissed: press one of its buttons",
                    describe(dialog)
                ));
            }
        };
        let id = f64::from(dialog.id);
        self.screen.answered();
        self.engine.control().dispatch_event(
            ROOT_ID,
            EventKind::Dialog,
            EventPayload::Map(vec![
                ("id", PayloadValue::Number(id)),
                ("index", PayloadValue::Number(index)),
            ]),
        );
        Ok(())
    }

    /// The step `back`: goes back as the user does (a header's back button,
    /// swiping a sheet down): in the newest modal, else the whole screen,
    /// the innermost stack that can go back pops its top screen (`onDismiss`
    /// to it), or asks it (`onBackRequest`) when it prevents going back;
    /// without one, the newest modal asks to close (`onRequestClose`, and
    /// `onDismiss` when the user may close it). Fails when there is nothing
    /// to go back from.
    pub fn back(&mut self) -> Result<(), String> {
        self.step("back");
        if !self.go_back() {
            return Err(format!("nothing to go back from; {}", self.on_screen()));
        }
        self.done()
    }

    /// Goes back as a platform does; false when nothing could.
    fn go_back(&mut self) -> bool {
        let control = self.engine.control();
        match self.screen.back_target() {
            Back::Pop(screen) => {
                self.screen.pop(screen);
                control.dispatch_event(screen, EventKind::Dismiss, EventPayload::None);
            }
            Back::Ask(screen) => {
                control.dispatch_event(screen, EventKind::BackRequest, EventPayload::None);
            }
            Back::Close { modal, dismissible } => {
                control.dispatch_event(modal, EventKind::RequestClose, EventPayload::None);
                if dismissible {
                    self.screen.close(modal);
                    control.dispatch_event(modal, EventKind::Dismiss, EventPayload::None);
                }
            }
            Back::Nothing => return false,
        }
        true
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
            match self.pump(deadline) {
                Pumped::Batch => {}
                Pumped::Error => self.check()?,
                Pumped::Exited => return self.check(),
                Pumped::Timeout => {
                    if Instant::now() >= deadline {
                        self.check()?;
                        return Err(format!("the app rendered nothing in {} s", WAIT.as_secs()));
                    }
                }
            }
        }
        self.done()
    }

    /// The step `wait(<ms>)`: applies batches for `time` (timers and
    /// animations run).
    pub fn wait(&mut self, time: Duration) -> Result<(), String> {
        self.step(format!("wait({})", time.as_millis()));
        let end = Instant::now() + time;
        while Instant::now() < end {
            match self.pump(end) {
                Pumped::Error => self.check()?,
                Pumped::Exited => break,
                Pumped::Batch | Pumped::Timeout => {}
            }
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
            match self.pump(deadline) {
                Pumped::Batch => {}
                Pumped::Error => self.check()?,
                Pumped::Exited => {
                    return Err(format!(
                        "{what} never happened: the app left; {}",
                        self.on_screen()
                    ));
                }
                Pumped::Timeout => {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "{what} never happened in {} ms; {}",
                            timeout.as_millis(),
                            self.on_screen()
                        ));
                    }
                }
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
            self.animator.scrolled(id, x, y);
            // One display frame per step; a frame that needs nothing
            // produces no batch, and a late one is applied with the next.
            let frame = Instant::now() + FRAME;
            while Instant::now() < frame {
                if let Pumped::Batch | Pumped::Exited = self.pump(frame) {
                    break;
                }
            }
        }
        self.send(id, EventKind::ScrollEndDrag, offset(&self.screen));
        self.send(id, EventKind::ScrollEnd, offset(&self.screen));
        self.done()
    }
}

/// How a dialog is answered when the test says nothing, as a user backing
/// out does: its cancel button, else closed when it can be; a dialog that
/// can only be answered by one of its other buttons needs the test's
/// answer ([`Headless::on_dialog`]).
pub fn default_answer(dialog: &Dialog) -> Result<DialogAnswer, String> {
    if let Some(cancel) = dialog
        .buttons
        .iter()
        .find(|b| b.style == DialogButtonStyle::Cancel)
    {
        return Ok(DialogAnswer::Press(cancel.text.clone()));
    }
    if dialog.cancelable {
        return Ok(DialogAnswer::Dismiss);
    }
    Err(format!(
        "the dialog {} needs an answer: say which button to press (`onDialog`)",
        describe(dialog)
    ))
}

/// A dialog, as errors name it: its title, else its message.
fn describe(dialog: &Dialog) -> String {
    format!(
        "{:?}",
        dialog
            .title
            .as_deref()
            .or(dialog.message.as_deref())
            .unwrap_or("")
    )
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
        let end = app.screen().scroll_end(
            app.screen()
                .scroll_view(&Selector::TestId("rows".into()))
                .unwrap(),
        );
        assert!(end > 4000.0 && end < 5000.0, "{end}");
        app.wait_for(&format!("At {end}")).unwrap();
        let error = app.scroll("id:rows", 10.0).unwrap_err();
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
