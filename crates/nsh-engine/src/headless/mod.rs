//! Headless runs (feature `headless`): an app on an engine with no
//! platform, its mount batches printed, views tapped by what they show,
//! timers and animations run for a while. It is what `nsh trace` does.
//!
//! The prebuilt `nsh` runs [`trace`] itself for apps it can serve (the
//! UI sets it links). An app with Rust crates of its own is run by a
//! runner `nsh` generates for it (`nsh-cli` `rust_app.rs`), which links
//! the app's crates and calls [`runner`]: `nsh` bundles and compiles the
//! app (or, for a Rust app's root, nothing), then starts the runner with a
//! [`TraceRequest`] as its one argument. The runner prints the batches on
//! its standard output as `nsh trace` does; its log lines, warnings and
//! errors go to standard error as [`RunnerMessage`]s, which `nsh` shows as
//! its own. A Rust app's runner is built without feature `js`: no QuickJS.
//!
//! Tests run an app the same way ([`Headless`]): on an engine with display
//! frames at 60 Hz ([`start`]), its batches applied to a model of the
//! native views ([`Screen`]), pressed by what they show with `nsh trace`'s
//! selectors (`--tap`'s: `id:save`, `label:Close`, `text:+1`, or a bare
//! string). `nsh-testing` builds a TSX entry for them.

mod net;

use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use nsh_core::{
    ApproxTextEngine, Command, EventKind, EventPayload, MountBatch, MountItem, NodeId, Points,
    Size,
    ui::{Registry, Root},
};
use serde_json::{Value, json};

pub use self::net::{DirectoryAssets, UreqNetwork};
use crate::{App, Assets, Edges, Engine, EngineConfig, EngineControl, Environment, MountSink};

/// The surface's root view: every app view hangs below it.
const ROOT: NodeId = 1;
/// How long a quiet wait ([`TraceOptions::wait`] not given) may take at
/// most: an app with a running timer or animation is never quiet.
const MAX_SETTLE: Duration = Duration::from_secs(10);
/// How long the app must stay quiet after a tap before the next one (without
/// [`TraceOptions::wait`]).
const TAP_QUIET: Duration = Duration::from_millis(100);

/// What a trace does after the first render (`nsh trace`'s flags).
#[derive(Debug, Clone, PartialEq)]
pub struct TraceOptions {
    /// Taps on the first pressable view (when `tap` is empty).
    pub taps: u32,
    /// Views to tap, in order ([`Selector`]).
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
    pub safe_area: Edges<Points>,
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
            safe_area: Edges {
                top: Points(top),
                right: Points(right),
                bottom: Points(bottom),
                left: Points(left),
            },
        })
    }
}

/// Parses `--safe-area <top,right,bottom,left>` (one value: all four).
pub fn parse_insets(value: &str) -> Result<Edges<Points>, String> {
    let parts = value
        .split(',')
        .map(|p| p.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{e}: expected top,right,bottom,left in points"))?;
    match parts[..] {
        [all] => Ok(Edges {
            top: Points(all),
            right: Points(all),
            bottom: Points(all),
            left: Points(all),
        }),
        [top, right, bottom, left] => Ok(Edges {
            top: Points(top),
            right: Points(right),
            bottom: Points(bottom),
            left: Points(left),
        }),
        _ => Err("expected top,right,bottom,left in points (or one value for all)".into()),
    }
}

/// Checks `--tap`'s selectors ([`Selector`]) before anything is built.
pub fn check_taps(taps: &[String]) -> Result<(), String> {
    taps.iter().try_for_each(|s| Selector::parse(s).map(|_| ()))
}

struct ChannelSink {
    batches: Sender<MountBatch>,
    /// Set once the engine runs; drives `requestAnimationFrame` at 60 Hz.
    control: Arc<OnceLock<EngineControl>>,
}

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.batches.send(batch);
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
    let (tx, rx) = crossbeam_channel::unbounded();
    let control = Arc::new(OnceLock::new());
    let sink = ChannelSink {
        batches: tx,
        control: control.clone(),
    };
    let engine = Engine::start(config, || ApproxTextEngine, sink)?;
    let _ = control.set(engine.control());
    Ok((engine, rx))
}

/// Runs `app` (with the UI sets `ui_sets` and its files from `assets`)
/// headless as `options` say: prints every mount batch to `out`, taps
/// views, runs timers. `warn` gets what the run notices without failing
/// (no pressable view, an app that never goes quiet).
pub fn trace(
    app: App,
    ui_sets: Vec<Registry>,
    assets: Arc<dyn Assets>,
    options: &TraceOptions,
    out: &mut dyn Write,
    warn: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let taps = options
        .tap
        .iter()
        .map(|s| Selector::parse(s))
        .collect::<Result<Vec<_>, _>>()?;
    let config = EngineConfig {
        viewport: Size::new(Points(options.width), Points(options.height)),
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
    let (engine, rx) = start(config).map_err(|e| e.to_string())?;

    let mut views = Views::new(out);
    let first = next(&rx).ok_or("the app rendered nothing")?;
    views.show("initial render", &first);
    let scroll_view = first.items.iter().find_map(|item| match item {
        MountItem::Create {
            id,
            kind: nsh_core::ViewKind::Scroll,
        } => Some(*id),
        _ => None,
    });
    if let (Some(distance), Some(id)) = (options.scroll, scroll_view) {
        let mut y = 0.0;
        while y < distance {
            y += 10.0;
            // The simulated platform applied every scroll the core asked for.
            engine.control().scroll(id, 0.0, y, u32::MAX);
            // Frames that need nothing produce no batch.
            if let Ok(batch) = rx.recv_timeout(Duration::from_millis(50)) {
                views.show(&format!("scroll to {y}"), &batch);
            }
        }
    }
    // Batches emitted meanwhile (timers, animations, releases after GC) are
    // printed too.
    let settle = |views: &mut Views<'_>,
                  title: &str,
                  quiet: Duration,
                  warn: &mut dyn FnMut(&str)| match options.wait {
        Some(ms) => {
            views.run(&rx, title, None, Duration::from_millis(ms));
        }
        None => {
            if !views.run(&rx, title, Some(quiet), MAX_SETTLE) {
                warn(&format!(
                    "still rendering after {} s (a timer or an animation runs); --wait <ms> bounds it",
                    MAX_SETTLE.as_secs()
                ));
            }
        }
    };
    settle(
        &mut views,
        "while idle",
        Duration::from_millis(options.idle_ms),
        warn,
    );

    let steps: Vec<Option<&Selector>> = if taps.is_empty() {
        (0..options.taps).map(|_| None).collect()
    } else {
        taps.iter().map(Some).collect()
    };
    for (i, selector) in steps.iter().enumerate() {
        let n = i + 1;
        let (id, title) = match selector {
            Some(selector) => match views.screen.find(selector) {
                Some(id) => (id, format!("tap #{n} ({selector}) on view {id}")),
                None => return Err(format!("tap #{n}: no pressable view matches `{selector}`")),
            },
            None => match views.screen.first_pressable() {
                Some(id) => (id, format!("tap #{n} on view {id}")),
                None => {
                    warn("no pressable view found");
                    break;
                }
            },
        };
        engine
            .control()
            .dispatch_event(id, EventKind::Press, EventPayload::None);
        match next(&rx) {
            Some(batch) => views.show(&title, &batch),
            None => return Err(format!("tap #{n} produced no update")),
        }
        settle(&mut views, &format!("after tap #{n}"), TAP_QUIET, warn);
    }
    if options.duration > 0 {
        views.run(
            &rx,
            "while running",
            None,
            Duration::from_millis(options.duration),
        );
    }
    let _ = views.out.flush();
    Ok(())
}

fn next(rx: &Receiver<MountBatch>) -> Option<MountBatch> {
    rx.recv_timeout(Duration::from_secs(5)).ok()
}

/// What `--tap` names: `id:<testID>`, `label:<accessibilityLabel>`,
/// `text:<text>`, `view:<node id>`, or a bare string, matched as each of the
/// first three in turn. A match that is not pressable taps its nearest
/// pressable ancestor.
#[derive(Debug, PartialEq)]
enum Selector {
    TestId(String),
    Label(String),
    Text(String),
    View(NodeId),
    Any(String),
}

impl Selector {
    fn parse(s: &str) -> Result<Self, String> {
        Ok(match s.split_once(':') {
            Some(("id", v)) => Self::TestId(v.into()),
            Some(("label", v)) => Self::Label(v.into()),
            Some(("text", v)) => Self::Text(v.into()),
            Some(("view", v)) => Self::View(
                v.parse()
                    .map_err(|e| format!("`--tap {s}`: not a view id: {e}"))?,
            ),
            _ => Self::Any(s.into()),
        })
    }
}

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TestId(v) => write!(f, "id:{v}"),
            Self::Label(v) => write!(f, "label:{v}"),
            Self::Text(v) => write!(f, "text:{v}"),
            Self::View(v) => write!(f, "view:{v}"),
            Self::Any(v) => f.write_str(v),
        }
    }
}

/// Where the batches of a trace are printed, and the views they built.
struct Views<'o> {
    out: &'o mut dyn Write,
    screen: Screen,
}

impl<'o> Views<'o> {
    fn new(out: &'o mut dyn Write) -> Self {
        Self {
            out,
            screen: Screen::default(),
        }
    }

    /// Prints a batch and applies it.
    fn show(&mut self, title: &str, batch: &MountBatch) {
        print_batch(self.out, title, batch);
        self.screen.apply(batch);
    }

    /// Prints the batches that arrive for `limit`, or until none came for
    /// `quiet`; false if the limit ended it.
    fn run(
        &mut self,
        rx: &Receiver<MountBatch>,
        title: &str,
        quiet: Option<Duration>,
        limit: Duration,
    ) -> bool {
        let end = Instant::now() + limit;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            let timeout = quiet.map_or(left, |q| q.min(left));
            match rx.recv_timeout(timeout) {
                Ok(batch) => self.show(title, &batch),
                Err(RecvTimeoutError::Disconnected) => return true,
                Err(RecvTimeoutError::Timeout) => return quiet.is_some() && timeout < left,
            }
            if Instant::now() >= end {
                return false;
            }
        }
    }
}

/// The native views as the mount items built them: their tree, what they
/// show (text, test id, accessibility label), which hear presses, and the
/// commands sent (the clipboard, focus, scrolling, ...).
#[derive(Debug, Default)]
pub struct Screen {
    nodes: HashMap<NodeId, View>,
    /// Children of each view, bottom to top.
    children: HashMap<NodeId, Vec<NodeId>>,
    commands: Vec<(NodeId, Command)>,
}

#[derive(Debug, Default)]
struct View {
    parent: Option<NodeId>,
    pressable: bool,
    test_id: Option<Arc<str>>,
    label: Option<Arc<str>>,
    text: Option<String>,
}

impl Screen {
    /// Applies `batch`.
    pub fn apply(&mut self, batch: &MountBatch) {
        for item in &batch.items {
            self.apply_item(item);
        }
    }

    fn apply_item(&mut self, item: &MountItem) {
        match item {
            MountItem::Create { id, .. } => {
                self.nodes.insert(*id, View::default());
            }
            MountItem::Insert {
                parent,
                child,
                before,
            } => {
                self.detach(*child);
                let siblings = self.children.entry(*parent).or_default();
                let at = before
                    .and_then(|b| siblings.iter().position(|s| *s == b))
                    .unwrap_or(siblings.len());
                siblings.insert(at, *child);
                self.nodes.entry(*child).or_default().parent = Some(*parent);
            }
            MountItem::Remove { id } => self.detach(*id),
            MountItem::Delete { id } => {
                self.detach(*id);
                self.nodes.remove(id);
                self.children.remove(id);
            }
            MountItem::Events { id, events } => {
                self.nodes.entry(*id).or_default().pressable = events.contains(EventKind::Press);
            }
            MountItem::ViewProps { id, props } => {
                let view = self.nodes.entry(*id).or_default();
                view.test_id = props.test_id.clone();
                view.label = props.accessibility.label.clone();
            }
            MountItem::Text { id, text, .. } => {
                self.nodes.entry(*id).or_default().text = Some(text.text.clone());
            }
            MountItem::Command { id, command } => self.commands.push((*id, command.clone())),
            _ => {}
        }
    }

    fn detach(&mut self, id: NodeId) {
        if let Some(parent) = self.nodes.get_mut(&id).and_then(|v| v.parent.take())
            && let Some(siblings) = self.children.get_mut(&parent)
        {
            siblings.retain(|s| *s != id);
        }
    }

    /// The views on screen (attached below the root), in tree order.
    fn attached(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![ROOT];
        while let Some(id) = stack.pop() {
            if id != ROOT {
                out.push(id);
            }
            if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().rev());
            }
        }
        out
    }

    /// The texts on screen, in tree order.
    pub fn texts(&self) -> Vec<&str> {
        self.attached()
            .into_iter()
            .filter_map(|id| self.nodes.get(&id)?.text.as_deref())
            .collect()
    }

    /// Whether a view on screen shows the text `text` (spaces around it
    /// aside).
    pub fn shows(&self, text: &str) -> bool {
        self.texts().iter().any(|t| t.trim() == text)
    }

    /// The commands sent so far, in order, with the view each went to.
    pub fn commands(&self) -> &[(NodeId, Command)] {
        &self.commands
    }

    /// Whether the command `command` was sent.
    pub fn sent(&self, command: &Command) -> bool {
        self.commands.iter().any(|(_, c)| c == command)
    }

    /// The view a press on `selector` goes to (`nsh trace --tap`'s:
    /// `id:<testID>`, `label:<label>`, `text:<text>`, `view:<id>`, or a
    /// bare string matched as each of the first three in turn): the first
    /// matching view on screen, or its nearest ancestor that hears presses
    /// (a text inside a button). `None` when none matches; an error for a
    /// selector that does not parse.
    pub fn press_target(&self, selector: &str) -> Result<Option<NodeId>, String> {
        Selector::parse(selector).map(|s| self.find(&s))
    }

    fn first_pressable(&self) -> Option<NodeId> {
        self.attached()
            .into_iter()
            .find(|id| self.nodes.get(id).is_some_and(|v| v.pressable))
    }

    /// The first view on screen the selector matches, or its nearest
    /// pressable ancestor (a text inside a button).
    fn find(&self, selector: &Selector) -> Option<NodeId> {
        let attached = self.attached();
        let matching = |test: &dyn Fn(&View) -> bool| {
            attached.iter().find_map(|id| {
                self.nodes
                    .get(id)
                    .filter(|v| test(v))
                    .and_then(|_| self.pressable_from(*id))
            })
        };
        let is = |value: &Option<Arc<str>>, want: &str| value.as_deref() == Some(want);
        let text = |v: &View, want: &str| v.text.as_deref().map(str::trim) == Some(want);
        match selector {
            Selector::TestId(want) => matching(&|v| is(&v.test_id, want)),
            Selector::Label(want) => matching(&|v| is(&v.label, want)),
            Selector::Text(want) => matching(&|v| text(v, want)),
            Selector::View(id) => attached.contains(id).then_some(*id),
            Selector::Any(want) => matching(&|v| is(&v.test_id, want))
                .or_else(|| matching(&|v| is(&v.label, want)))
                .or_else(|| matching(&|v| text(v, want))),
        }
    }

    fn pressable_from(&self, mut id: NodeId) -> Option<NodeId> {
        loop {
            let view = self.nodes.get(&id)?;
            if view.pressable {
                return Some(id);
            }
            id = view.parent?;
        }
    }
}

/// How long [`Headless::wait_for`] and [`Headless::wait_until`] wait.
pub const WAIT: Duration = Duration::from_secs(5);

/// An app on a headless engine ([`start`]) and what it shows ([`Screen`]),
/// for tests: wait for what it shows, press views as a tap does.
///
/// ```ignore
/// let mut app = Headless::start(EngineConfig::new(Root::of::<my_app::App>())).unwrap();
/// app.wait_for("Count: 0");
/// app.press("Tap");
/// app.wait_for("Count: 1");
/// ```
pub struct Headless {
    engine: Engine,
    batches: Receiver<MountBatch>,
    screen: Screen,
}

impl Headless {
    /// Starts `config`'s app ([`start`]).
    pub fn start(config: EngineConfig) -> std::io::Result<Headless> {
        let (engine, batches) = start(config)?;
        Ok(Headless {
            engine,
            batches,
            screen: Screen::default(),
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

    /// Applies the next batch, waiting `timeout` at most: the batch, or
    /// `None` when none came.
    pub fn next_batch(&mut self, timeout: Duration) -> Option<MountBatch> {
        let batch = self.batches.recv_timeout(timeout).ok()?;
        self.screen.apply(&batch);
        Some(batch)
    }

    /// Applies batches until none comes for 300 ms, or for 2 s at most (an
    /// app updating all the time never goes quiet).
    pub fn settle(&mut self) {
        let end = Instant::now() + Duration::from_secs(2);
        while Instant::now() < end && self.next_batch(Duration::from_millis(300)).is_some() {}
    }

    /// Applies batches until `done` holds for the screen.
    ///
    /// # Panics
    ///
    /// When it does not within [`WAIT`]; the message names `what` and the
    /// texts on screen.
    pub fn wait_until(&mut self, what: &str, done: impl Fn(&Screen) -> bool) {
        let deadline = Instant::now() + WAIT;
        while !done(&self.screen) {
            match self.batches.recv_deadline(deadline) {
                Ok(batch) => self.screen.apply(&batch),
                Err(_) => panic!("{what} never happens; on screen: {:?}", self.screen.texts()),
            }
        }
    }

    /// Applies batches until a view shows the text `text`.
    ///
    /// # Panics
    ///
    /// When none does within [`WAIT`].
    pub fn wait_for(&mut self, text: &str) {
        self.wait_until(&format!("{text:?} showing"), |s| s.shows(text));
    }

    /// Whether a view on screen shows the text `text`.
    pub fn shows(&self, text: &str) -> bool {
        self.screen.shows(text)
    }

    /// Presses what `selector` names, as a tap does: `nsh trace --tap`'s
    /// selectors ([`Screen::press_target`]), so `"Save"` presses the
    /// button whose text, label or test id it is. Does not wait for what
    /// the press changes ([`Headless::wait_for`]).
    ///
    /// # Panics
    ///
    /// When nothing on screen that hears presses matches.
    pub fn press(&mut self, selector: &str) {
        let target = self
            .screen
            .press_target(selector)
            .unwrap_or_else(|e| panic!("{e}"))
            .unwrap_or_else(|| {
                panic!(
                    "no pressable view matches `{selector}`; on screen: {:?}",
                    self.screen.texts()
                )
            });
        self.engine
            .control()
            .dispatch_event(target, EventKind::Press, EventPayload::None);
    }
}

fn print_batch(out: &mut dyn Write, title: &str, batch: &MountBatch) {
    // A closed output (`| head`) ends nothing: the run goes on unseen.
    let _ = writeln!(out, "── {title}: {} items", batch.items.len());
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
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut warn = |message: &str| {
        let _ = writeln!(
            std::io::stderr().lock(),
            "{}",
            RunnerMessage::Warn(message.to_owned()).to_line()
        );
    };
    match trace(app, ui_sets, assets, &request.options, &mut out, &mut warn) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = out.flush();
            fail(e)
        }
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
    fn a_test_presses_what_a_text_is_in_and_waits_for_what_it_shows() {
        let mut app = Headless::start(EngineConfig::new(Root::of::<Tally>())).unwrap();
        app.wait_for("Count: 0");
        // The text is in the pressable view: the press goes to the view.
        app.press("Add");
        app.wait_for("Count: 1");
        app.press("text:Add");
        app.wait_for("Count: 2");
        assert!(!app.shows("Count: 1"));
        assert_eq!(app.screen().texts(), ["Count: 2", "Add"]);
        assert_eq!(app.screen().press_target("Nothing"), Ok(None));
        assert!(app.screen().press_target("view:x").is_err());
    }

    #[test]
    fn selectors_parse() {
        assert_eq!(
            Selector::parse("id:save").unwrap(),
            Selector::TestId("save".into())
        );
        assert_eq!(
            Selector::parse("label:Close").unwrap(),
            Selector::Label("Close".into())
        );
        assert_eq!(
            Selector::parse("text:+1").unwrap(),
            Selector::Text("+1".into())
        );
        assert_eq!(Selector::parse("view:20").unwrap(), Selector::View(20));
        assert_eq!(Selector::parse("a:b").unwrap(), Selector::Any("a:b".into()));
        assert!(Selector::parse("view:x").is_err());
        assert_eq!(
            check_taps(&["id:a".into(), "view:x".into()]).unwrap_err(),
            "`--tap view:x`: not a view id: invalid digit found in string"
        );
    }

    #[test]
    fn insets_parse() {
        let i = parse_insets("47,0,34,0").unwrap();
        assert_eq!(
            (i.top.0, i.right.0, i.bottom.0, i.left.0),
            (47.0, 0.0, 34.0, 0.0)
        );
        assert_eq!(parse_insets("10").unwrap().left, Points(10.0));
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
            safe_area: Edges::default(),
        };
        let mut out = Vec::new();
        let mut warnings = Vec::new();
        trace(
            App::Rust(Root::of::<Counter>()),
            Vec::new(),
            Arc::new(DirectoryAssets::new(PathBuf::from("."), Vec::new())),
            &options,
            &mut out,
            &mut |w| warnings.push(w.to_owned()),
        )
        .unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.starts_with("── initial render: "), "{out}");
        assert!(out.contains("text: \"Count 0\""), "{out}");
        assert!(out.contains("── tap #1 on view "), "{out}");
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
            &mut Vec::new(),
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(error, "tap #1: no pressable view matches `label:Nope`");
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
                safe_area: Edges {
                    top: Points(47.0),
                    right: Points(0.0),
                    bottom: Points(34.0),
                    left: Points(0.0),
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
