//! The test agent of a real host (`nsh test --target`, feature `agent`):
//! it takes the steps of a test on the app the host shows, as the
//! headless driver takes them on a model of the views, but acts with the
//! platform's own input at the views the selectors name.
//!
//! The host keeps a [`Model`]: it hands it every batch it applied (on its
//! main thread) and the app's errors, and the model keeps the same
//! [`Screen`] the headless driver does, so selectors find the same views
//! on every platform. The host's dev client hands each [`TestStep`] to an
//! [`Agent`] on a thread of its own, which resolves the selector on the
//! model, asks the platform ([`Native`]) where the view is on screen and
//! acts there (a click or a tap, a drag, typed keys), then waits until the
//! app is quiet: no batch for 100 ms and no native animation running, 2 s
//! at most. [`Native`]'s calls come from that thread; implementations do
//! their work on the main thread and return when it is done.

use std::{
    sync::{Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use nsh_core::{
    MountBatch, NodeId,
    geometry::{Bounds, Point, Points, Size},
};
use nsh_dev::protocol::{StepOutcome, TestStep};

use crate::headless::{Screen, screen::Pick, selector::Selector};

/// How long after the last batch the app is quiet.
const QUIET: Duration = Duration::from_millis(100);
/// How long a step waits for the app to be quiet at most.
const SETTLE: Duration = Duration::from_secs(2);
/// How often waits look again while nothing comes.
const POLL: Duration = Duration::from_millis(20);
/// A long press's hold when the view does not say (`delayLongPress`).
const LONG_PRESS: Duration = Duration::from_millis(500);
/// How many gestures scrolling a view into view takes at most.
const SCROLLS_INTO_VIEW: usize = 20;

/// What a host's app shows, as test steps see it: the batches the host
/// applied and the errors the app reported.
#[derive(Default)]
pub struct Model {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    screen: Screen,
    /// Batches applied so far.
    batches: u64,
    /// When the last one was.
    last: Option<Instant>,
    /// What the app threw since the last step looked.
    errors: Vec<String>,
}

impl Model {
    /// Takes in a batch the host applied (on its main thread, after
    /// applying it).
    pub fn apply(&self, batch: &MountBatch) {
        let mut state = self.lock();
        state.screen.apply(batch);
        state.batches += 1;
        state.last = Some(Instant::now());
        drop(state);
        self.changed.notify_all();
    }

    /// The app threw `message` (an exception that escaped a callback, a
    /// bundle that did not load): the step taken meanwhile fails with it.
    pub fn error(&self, message: impl Into<String>) {
        self.lock().errors.push(message.into());
        self.changed.notify_all();
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Waits until a batch after the `seen`th came, `timeout` at most.
    fn wait_for_batch(&self, seen: u64, timeout: Duration) -> MutexGuard<'_, State> {
        let state = self.lock();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, timeout, |s| s.batches <= seen && s.errors.is_empty())
            .unwrap_or_else(|e| e.into_inner());
        state
    }
}

/// A key the agent presses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Return (Enter).
    Return,
    /// Deletes the character before the caret, or the selection.
    Backspace,
    /// Selects all the text of the focused input (Cmd+A, Ctrl+A).
    SelectAll,
}

/// The platform's part: where views are and the input that acts on them.
/// Called from the agent's thread; each call does its work on the host's
/// main thread and returns once it is done (input posted, not answered).
pub trait Native: Send + Sync {
    /// The view `id`'s bounds in the window's content, in points from its
    /// top-left (`None`: it has no native view on screen).
    fn bounds(&self, id: NodeId) -> Option<Bounds<Points>>;
    /// The size of the window's content, in points.
    fn viewport(&self) -> Size<Points>;
    /// A press (a click, a tap) at `at`.
    fn press(&self, at: Point<Points>);
    /// A press at `at` held for `hold`.
    fn long_press(&self, at: Point<Points>, hold: Duration);
    /// Scrolls what is at `at` by `delta` points (positive: down or right,
    /// towards the content's end), as a trackpad or a finger does.
    fn scroll(&self, at: Point<Points>, delta: Point<Points>);
    /// Types `text` into the focused input, key by key.
    fn type_text(&self, text: &str);
    /// Presses `key`.
    fn key(&self, key: Key);
    /// The text of input `id` as the platform holds it.
    fn input_text(&self, id: NodeId) -> Option<String>;
    /// Goes back as the user does (a back gesture, the back button,
    /// Escape): an error when the platform has none.
    fn back(&self) -> Result<(), String>;
    /// Answers the dialog the app shows now (the oldest): presses its
    /// button with the text `button`, or closes it without one.
    fn answer_dialog(&self, button: Option<&str>) -> Result<(), String>;
    /// Whether a native animation runs now.
    fn animating(&self) -> bool;
    /// A picture of the window, as a PNG.
    fn screenshot(&self) -> Result<Vec<u8>, String>;
    /// Makes the window `size` points.
    fn resize(&self, width: Points, height: Points) -> Result<(), String>;
    /// Switches the appearance the app sees.
    fn set_dark(&self, dark: bool) -> Result<(), String>;
    /// Closes the app: the host exits.
    fn quit(&self);
}

/// Takes test steps on a host's app ([`Model`], [`Native`]).
pub struct Agent {
    model: std::sync::Arc<Model>,
    native: Box<dyn Native>,
}

impl Agent {
    /// An agent on the app `model` sees, acting through `native`.
    pub fn new(model: std::sync::Arc<Model>, native: Box<dyn Native>) -> Agent {
        Agent { model, native }
    }

    /// Takes `step`; blocks until it is done and the app is quiet.
    pub fn take(&self, step: TestStep) -> StepOutcome {
        match self.run(step) {
            Ok(value) => StepOutcome::Done { value },
            Err(message) => StepOutcome::Failed { message },
        }
    }

    fn run(&self, step: TestStep) -> Result<serde_json::Value, String> {
        use serde_json::Value;
        let done = |r: Result<(), String>| r.map(|()| Value::Null);
        match step {
            TestStep::Ready { timeout_ms } => done(self.ready(Duration::from_millis(timeout_ms))),
            TestStep::Press { selector } => done(self.press(&selector)),
            TestStep::LongPress { selector } => done(self.long_press(&selector)),
            TestStep::Type { selector, text } => done(self.type_text(&selector, &text)),
            TestStep::Submit { selector } => done(self.submit(&selector)),
            TestStep::Clear { selector } => done(self.clear(&selector)),
            TestStep::Scroll { selector, distance } => done(self.scroll(&selector, distance)),
            TestStep::Back => done(self.native.back().and_then(|()| self.settled())),
            TestStep::Wait { ms } => {
                std::thread::sleep(Duration::from_millis(ms));
                done(self.check())
            }
            TestStep::WaitFor {
                selector,
                timeout_ms,
                gone,
            } => done(self.wait_for(&selector, Duration::from_millis(timeout_ms), gone)),
            TestStep::NextChange { timeout_ms } => {
                let seen = self.model.lock().batches;
                drop(
                    self.model
                        .wait_for_batch(seen, Duration::from_millis(timeout_ms)),
                );
                done(self.check())
            }
            TestStep::Texts => Ok(Value::from(
                self.model
                    .lock()
                    .screen
                    .texts()
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )),
            TestStep::Count { selector } => {
                Ok(Value::from(self.model.lock().screen.count(&selector)?))
            }
            TestStep::Shows { selector } => {
                Ok(Value::from(self.model.lock().screen.matches(&selector)?))
            }
            TestStep::Enabled { selector } => {
                Ok(Value::from(self.model.lock().screen.enabled(&selector)?))
            }
            TestStep::Value { selector } => {
                let id = self.model.lock().screen.input_id(&selector)?;
                Ok(id
                    .and_then(|id| self.native.input_text(id))
                    .map_or(Value::Null, Value::from))
            }
            TestStep::Dialogs => {
                serde_json::to_value(self.model.lock().screen.dialogs()).map_err(|e| e.to_string())
            }
            TestStep::PendingDialog => {
                serde_json::to_value(self.model.lock().screen.pending_dialog())
                    .map_err(|e| e.to_string())
            }
            TestStep::AnswerDialog { button } => done(self.answer_dialog(button.as_deref())),
            TestStep::Screenshot => {
                use base64::Engine as _;
                let png = self.native.screenshot()?;
                Ok(Value::from(
                    base64::engine::general_purpose::STANDARD.encode(png),
                ))
            }
            TestStep::SetViewport { width, height } => done(
                self.native
                    .resize(Points(width), Points(height))
                    .and_then(|()| self.settled()),
            ),
            TestStep::SetColorScheme { dark } => {
                done(self.native.set_dark(dark).and_then(|()| self.settled()))
            }
            TestStep::Quit => {
                self.native.quit();
                Ok(Value::Null)
            }
        }
    }

    /// Waits for the first render, `timeout` at most, then until the app
    /// is quiet.
    fn ready(&self, timeout: Duration) -> Result<(), String> {
        let state = self.model.wait_for_batch(0, timeout);
        let rendered = state.batches > 0;
        drop(state);
        self.check()?;
        if !rendered {
            return Err(format!(
                "the app rendered nothing in {} s",
                timeout.as_secs()
            ));
        }
        self.settled()
    }

    /// The one view `pick` gave for `selector` (from [`Self::find`]).
    fn picked(&self, pick: Pick, selector: &str, what: &str) -> Result<NodeId, String> {
        match pick {
            Pick::One(id) => Ok(id),
            Pick::None => Err(format!(
                "no {what} matches `{selector}`; {}",
                self.on_screen()
            )),
            Pick::Many(n) => Err(format!(
                "`{selector}` matches {n} {what}s: say which (`{selector} >> nth=0`), or look inside \
                 a view around it (`id:list >> {selector}`)"
            )),
            Pick::Disabled => Err(format!("`{selector}` is disabled; {}", self.on_screen())),
        }
    }

    /// The view `selector` names as `pick` finds it in the model, and the
    /// middle of it on screen.
    fn find(
        &self,
        selector: &str,
        what: &str,
        pick: impl FnOnce(&Screen, &Selector) -> Pick,
    ) -> Result<(NodeId, Point<Points>), String> {
        let parsed = Selector::parse(selector)?;
        self.settled()?;
        let picked = pick(&self.model.lock().screen, &parsed);
        let id = self.picked(picked, selector, what)?;
        Ok((id, self.scroll_into_view(id, selector)?))
    }

    /// The middle of view `id` on screen, scrolled into view first when it
    /// is not: the innermost scroll view around it that hides it scrolls it
    /// towards its own middle, a gesture at a time, as a user would.
    fn scroll_into_view(&self, id: NodeId, selector: &str) -> Result<Point<Points>, String> {
        let not_on_screen = || {
            format!(
                "`{selector}` is not on screen (it has no native view); {}",
                self.on_screen()
            )
        };
        let scrollers = self.model.lock().screen.scroll_ancestors(id);
        for _ in 0..SCROLLS_INTO_VIEW {
            let at = self.native.bounds(id).ok_or_else(not_on_screen)?.center();
            let viewport = self.native.viewport();
            let window = Bounds::new(Point::new(Points(0.0), Points(0.0)), viewport);
            // The innermost scroll view whose bounds leave the middle out,
            // else the outermost one when the window does.
            let hiding = scrollers.iter().find_map(|&(scroller, horizontal)| {
                let b = self.native.bounds(scroller)?;
                (!b.contains(&at)).then_some((scroller, horizontal, b))
            });
            let hiding = hiding.or_else(|| {
                (!window.contains(&at))
                    .then(|| scrollers.last())
                    .flatten()
                    .and_then(|&(s, h)| Some((s, h, self.native.bounds(s)?)))
            });
            let Some((_, horizontal, scroller)) = hiding else {
                if window.contains(&at) {
                    return Ok(at);
                }
                return Err(not_on_screen());
            };
            let middle = scroller.center();
            let (distance, extent) = if horizontal {
                (at.x.0 - middle.x.0, scroller.size.width.0)
            } else {
                (at.y.0 - middle.y.0, scroller.size.height.0)
            };
            let step = distance.clamp(-extent * 0.8, extent * 0.8);
            let delta = if horizontal {
                Point::new(Points(step), Points(0.0))
            } else {
                Point::new(Points(0.0), Points(step))
            };
            self.native.scroll(middle, delta);
            self.settled()?;
        }
        Err(format!(
            "`{selector}` did not come into view; {}",
            self.on_screen()
        ))
    }

    fn press(&self, selector: &str) -> Result<(), String> {
        let (_, at) = self.find(selector, "pressable view", |s, p| s.pressable(p))?;
        self.native.press(at);
        self.settled()
    }

    fn long_press(&self, selector: &str) -> Result<(), String> {
        let (id, at) = self.find(selector, "pressable view", |s, p| s.long_pressable(p))?;
        let hold = self
            .model
            .lock()
            .screen
            .long_press_delay(id)
            .map_or(LONG_PRESS, |ms| {
                Duration::from_secs_f64(f64::from(ms.max(0.0)) / 1000.0)
            });
        // A little past the delay, as a finger holds on.
        self.native
            .long_press(at, hold + Duration::from_millis(100));
        self.settled()
    }

    /// Focuses the input `selector` names by pressing it.
    fn focus(&self, selector: &str) -> Result<(), String> {
        let (_, at) = self.find(selector, "input", |s, p| s.input_view(p))?;
        self.native.press(at);
        self.settled()
    }

    fn type_text(&self, selector: &str, text: &str) -> Result<(), String> {
        self.focus(selector)?;
        // A line break is the return key, as the headless driver types it.
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                self.native.key(Key::Return);
            }
            if !line.is_empty() {
                self.native.type_text(line);
            }
        }
        self.settled()
    }

    fn submit(&self, selector: &str) -> Result<(), String> {
        self.focus(selector)?;
        self.native.key(Key::Return);
        self.settled()
    }

    fn clear(&self, selector: &str) -> Result<(), String> {
        self.focus(selector)?;
        self.native.key(Key::SelectAll);
        self.native.key(Key::Backspace);
        self.settled()
    }

    fn scroll(&self, selector: &str, distance: f32) -> Result<(), String> {
        let (id, at) = self.find(selector, "scroll view", |s, p| s.scroll_view(p))?;
        let horizontal = self
            .model
            .lock()
            .screen
            .scroll(id)
            .is_some_and(|s| s.horizontal);
        let delta = if horizontal {
            Point::new(Points(distance), Points(0.0))
        } else {
            Point::new(Points(0.0), Points(distance))
        };
        self.native.scroll(at, delta);
        self.settled()
    }

    fn answer_dialog(&self, button: Option<&str>) -> Result<(), String> {
        if self.model.lock().screen.pending_dialog().is_none() {
            return Err("no dialog to answer".into());
        }
        self.native.answer_dialog(button)?;
        self.model.lock().screen.answered();
        self.settled()
    }

    fn wait_for(&self, selector: &str, timeout: Duration, gone: bool) -> Result<(), String> {
        Selector::parse(selector)?;
        let deadline = Instant::now() + timeout;
        loop {
            let (holds, seen) = {
                let state = self.model.lock();
                (state.screen.matches(selector)? != gone, state.batches)
            };
            self.check()?;
            if holds {
                return Ok(());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                let what = if gone { "gone" } else { "showing" };
                return Err(format!(
                    "`{selector}` {what} never happened in {} ms; {}",
                    timeout.as_millis(),
                    self.on_screen()
                ));
            }
            drop(self.model.wait_for_batch(seen, left.min(POLL * 5)));
        }
    }

    /// Waits until the app is quiet ([`QUIET`], no native animation),
    /// [`SETTLE`] at most, then fails if it threw meanwhile.
    fn settled(&self) -> Result<(), String> {
        let end = Instant::now() + SETTLE;
        let start = Instant::now();
        loop {
            let (last, seen) = {
                let state = self.model.lock();
                (state.last.unwrap_or(start).max(start), state.batches)
            };
            let now = Instant::now();
            if now >= last + QUIET && !self.native.animating() || now >= end {
                break;
            }
            drop(self.model.wait_for_batch(seen, POLL));
        }
        self.check()
    }

    /// Fails with the app's first error since the last look.
    fn check(&self) -> Result<(), String> {
        let errors = std::mem::take(&mut self.model.lock().errors);
        match errors.into_iter().next() {
            Some(error) => Err(format!("the app threw: {error}")),
            None => Ok(()),
        }
    }

    fn on_screen(&self) -> String {
        format!("on screen: {:?}", self.model.lock().screen.texts())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nsh_core::{
        EventKind, EventPayload,
        geometry::{Size, point},
        ui::{Root, prelude::*},
    };
    use nsh_engine::{EngineConfig, EngineControl};

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

    /// A platform whose views are where their ids say (`x` is the id), and
    /// whose presses go to the view they are at, as the engine's events.
    struct Fake(EngineControl, Arc<Mutex<Vec<String>>>);

    impl Native for Fake {
        fn bounds(&self, id: NodeId) -> Option<Bounds<Points>> {
            Some(Bounds::new(
                point(Points(id as f32 - 0.5), Points(0.0)),
                Size::new(Points(1.0), Points(1.0)),
            ))
        }
        fn viewport(&self) -> Size<Points> {
            Size::new(Points(10_000.0), Points(10_000.0))
        }
        fn press(&self, at: Point<Points>) {
            let id = at.x.0.round() as NodeId;
            for kind in [EventKind::PressDown, EventKind::PressUp, EventKind::Press] {
                self.0.dispatch_event(id, kind, EventPayload::None);
            }
        }
        fn long_press(&self, _: Point<Points>, _: Duration) {}
        fn scroll(&self, _: Point<Points>, _: Point<Points>) {}
        fn type_text(&self, text: &str) {
            self.1.lock().unwrap().push(format!("type {text}"));
        }
        fn key(&self, key: Key) {
            self.1.lock().unwrap().push(format!("{key:?}"));
        }
        fn input_text(&self, _: NodeId) -> Option<String> {
            None
        }
        fn back(&self) -> Result<(), String> {
            Err("no back here".into())
        }
        fn answer_dialog(&self, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn animating(&self) -> bool {
            false
        }
        fn screenshot(&self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn resize(&self, _: Points, _: Points) -> Result<(), String> {
            Ok(())
        }
        fn set_dark(&self, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn quit(&self) {}
    }

    #[test]
    fn the_agent_finds_views_in_the_model_and_acts_where_they_are() {
        let (engine, batches) = crate::start(EngineConfig::new(Root::of::<Tally>())).unwrap();
        let model = Arc::new(Model::default());
        let feeding = model.clone();
        std::thread::spawn(move || {
            while let Ok(batch) = batches.recv() {
                feeding.apply(&batch);
            }
        });
        let typed = Arc::new(Mutex::new(Vec::new()));
        let agent = Agent::new(model, Box::new(Fake(engine.control(), typed.clone())));
        let ok = StepOutcome::Done {
            value: serde_json::Value::Null,
        };
        assert_eq!(agent.take(TestStep::Ready { timeout_ms: 5000 }), ok);
        assert_eq!(
            agent.take(TestStep::Texts),
            StepOutcome::Done {
                value: serde_json::json!(["Count: 0", "Add"])
            }
        );
        assert_eq!(
            agent.take(TestStep::Press {
                selector: "Add".into()
            }),
            ok
        );
        assert_eq!(
            agent.take(TestStep::WaitFor {
                selector: "Count: 1".into(),
                timeout_ms: 2000,
                gone: false
            }),
            ok
        );
        assert_eq!(
            agent.take(TestStep::Press {
                selector: "Nothing".into()
            }),
            StepOutcome::Failed {
                message: "no pressable view matches `Nothing`; on screen: [\"Count: 1\", \"Add\"]"
                    .into()
            }
        );
        assert_eq!(
            agent.take(TestStep::Back),
            StepOutcome::Failed {
                message: "no back here".into()
            }
        );
        // No input to type into.
        assert!(matches!(
            agent.take(TestStep::Type { selector: "Add".into(), text: "x".into() }),
            StepOutcome::Failed { message } if message.starts_with("no input matches `Add`")
        ));
        assert!(typed.lock().unwrap().is_empty());
        drop(engine);
    }
}
