//! Apps built by earlier checkouts run on this one: `api/apps/` keeps what
//! some checkout's `nsh build --release` wrote for some apps
//! (`<app>/bundle.js`; packages always carry their source, which runs when
//! their bytecode is not this runtime's) and a script of what a user does
//! and sees (`<app>/script.txt`), one step per line (with the assets it
//! imports, `<app>/assets/`, and what the build wrote of an app that
//! declares `capabilities` in `nsh.json`: `<app>/capabilities.json` and its
//! id, `<app>/app.json`, which the app then runs with, as a package's do,
//! its data directory a new empty one):
//!
//! - `viewport <width> <height>`: before anything else, the screen's size
//!   (390 × 844 by default);
//! - `platform <os> [desktop]`: before anything else (with `viewport`), the
//!   platform the app runs on: `ios` (the default), `android`, `macos`,
//!   `windows`, `linux` or `web`; `desktop` makes a browser a desktop one.
//!   It decides which of `ios={{…}}`, `android={{…}}`, `desktop={{…}}`
//!   (the desktop OSes and desktop browsers) and `web={{…}}` apply;
//! - `expect <text>`: a text on screen reads `<text>` (waits for it);
//! - `absent <text>`: no text on screen reads `<text>` (after the app is
//!   quiet);
//! - `tap <selector>`: presses the first view on screen `id:<testID>`,
//!   `label:<accessibility label>` or `text:<text>` names, or its nearest
//!   pressable ancestor (as `nsh test`'s `press`);
//! - `wait <ms>`: lets timers and animations run;
//! - `frame <selector> <x> <y> <width> <height>`: the first view on screen
//!   the selector names (itself, pressable or not) is laid out at that
//!   frame, in points relative to its parent (waits for it);
//! - `prop <selector> <name> <value>`: the first view on screen the
//!   selector names has host prop `<name>` (its JS name; a platform's props
//!   with or without their `ios.` / `android.` / `desktop.` / `web.`
//!   prefix) at `<value>` as the platform receives it, written as
//!   compat/values.rs says (waits for it);
//! - `noprop <selector> <name>`: the view has no such prop, or the value a
//!   view of its kind has with none set (after the app is quiet);
//! - `style <selector> <key> <value>`, `nostyle <selector> <key>`: the
//!   same for a style key, as mounted (compat/values.rs: paint keys, and
//!   the text keys of texts and inputs; layout keys show in `frame`s). A
//!   selector `span:<text>` names the first text that contains `<text>`,
//!   and its text keys are those of the run where `<text>` starts (nested
//!   `<Text>`s);
//! - `listens <selector> <event>`: the platform reports event `<event>`
//!   (`onPress`, ...) of the view, so its handler is set (waits for it);
//! - `log <text>`: the app printed a `console` line (any level) containing
//!   `<text>` (waits for it);
//! - `event <selector> <onEvent> [<key>=<value>...]`: the platform
//!   reports event `<onEvent>` of the first view on screen the selector
//!   names, or of its nearest ancestor that listens to it (as `listens`;
//!   it fails when none does), with a payload of the given keys (the
//!   handler's payload keys; a value is a number, `true`, `false` or a
//!   string, quoted when it has spaces or looks like a number), none
//!   without keys. A key given more than once makes a list (`onFileDrop`'s
//!   `uris`). `onHoverMove` and `onContextMenu` get their typed payload:
//!   `pageX` / `pageY` default to the listening view's position on screen
//!   plus `locationX` / `locationY`, the modifier keys to false;
//! - `touch start <selector> <x> <y>`, `touch move <x> <y>`,
//!   `touch end [<x> <y>]`, `touch cancel [<x> <y>]`: one finger goes
//!   down on the first view on screen the selector names at `<x> <y>` in
//!   it (the views around it that listen to touches get it; it fails when
//!   none does), moves or lifts there (positions in the view it went down
//!   on), or the system cancels it. The n-th touch event of a run has
//!   timestamp `16 * n` (ms);
//! - `type <selector> <text>`: the user types `<text>` (the rest of the
//!   line) into the input the selector names, one character at a time:
//!   `onKeyPress` (`key`), `onChange` (the whole text) and
//!   `onSelectionChange` (the caret after it) for each. It types after
//!   the input's text: the value the app last set, or what earlier `type`
//!   steps typed since;
//! - `key <selector> <key>`: `onKeyPress` with `key` (`Backspace`,
//!   `Enter`, ...), the text unchanged;
//! - `scroll <selector> <x> <y>`: the scroll view the selector names
//!   scrolled to content offset `<x> <y>` (`onScroll` then reports it,
//!   with the sizes the core knows, and its `contentOffset` reads it);
//! - `fontscale <scale>`: before anything else (with `viewport`), the
//!   user's text size (1 by default), which scales the fonts of texts and
//!   inputs;
//! - `state <selector> <state>...`: the platform reports the interaction
//!   states of the first view on screen the selector names (itself): it
//!   is now `pressed`, `hovered` and / or `focused`, and in no other
//!   (`none`: in none). A platform reports only the states the core asked
//!   it to track (`activeStyle`, `hover:`, ...), so the step waits for the
//!   view to track the states it names and fails when it does not;
//! - `origin <selector> <x> <y>`: the first view on screen the selector
//!   names is at `<x> <y>` on the surface: its frames' origins added up,
//!   scroll offsets aside (a list's row, whose frame is its cell's), waits
//!   for it;
//! - `keyboard <height> <duration>`: the software keyboard shows at
//!   `<height>` points over the bottom of the surface, animated over
//!   `<duration>` seconds (`0 0`: it hides);
//! - `statusbar <key>=<value>...`: the status bar the platform was last
//!   told to show reads `<value>` for each `<key>` given (`foreground`:
//!   `auto`, `light` or `dark`; `hidden`, `animated`: `true` or `false`;
//!   compat/values.rs), waits for it;
//! - `covers <item>`: no step, a claim: the script checks what `<item>` (a
//!   part of the API as `api/uncovered.txt` names it) does (compat/coverage.rs
//!   verifies it).
//!
//! A selector is one word (`id:<testID>`, `label:<label>`, `text:<text>`,
//! and `scroll:<selector>`, the nearest scroll view around the view the
//! inner selector names: a list, by one of its rows);
//! the steps after `frame` also take the part after `:` quoted, for one
//! with spaces: `text:"Taps: 0"`. Any error the app reports fails it. `NSH_API_APP=<name>` runs the fixtures whose name starts so; `NSH_API_DUMP=1` prints the texts
//! on screen after each step, for writing scripts.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    thread::ThreadId,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use nsh_core::{
    ApproxTextEngine, ContextMenuEvent, EventKind, EventPayload, HoverEvent, MountBatch, MountItem,
    NodeId, PayloadValue, Points, Size, TouchEvent, TouchPhase, TouchPoint,
    geometry::{DevicePixels, point, px},
    keymap::Modifiers,
    platform::{Environment, Idiom, Os},
    tailwind::state,
};
use nsh_engine::{Assets, Bundle, Engine, EngineConfig, EngineControl, MountSink};

use super::values::{self, Mounted};

/// The surface's root view.
const ROOT: NodeId = 1;

/// How long an `expect` waits for its text.
const EXPECT_WAIT: Duration = Duration::from_secs(5);

/// How long a `tap` waits at most for the app to be quiet first (a ticking
/// app never is).
const TAP_WAIT: Duration = Duration::from_millis(1000);

/// How long the app must send nothing to count as quiet.
const QUIET: Duration = Duration::from_millis(300);

struct Sink {
    batches: Sender<MountBatch>,
    errors: Arc<Mutex<Vec<String>>>,
    /// The engine's thread, which runs the app's JS: what it logs is the
    /// app's.
    thread: Arc<OnceLock<ThreadId>>,
}

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        self.thread.get_or_init(|| std::thread::current().id());
        let _ = self.batches.send(batch);
    }

    fn report_error(&self, message: &str) {
        self.errors.lock().unwrap().push(message.to_owned());
    }
}

/// `asset:/<path>` from the app's `assets/`.
struct AppAssets(std::path::PathBuf);

impl Assets for AppAssets {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        let path = uri
            .strip_prefix("asset:/")
            .ok_or_else(|| format!("not an asset URI: {uri}"))?;
        std::fs::read(self.0.join(path)).map_err(|e| format!("{uri}: {e}"))
    }
}

/// The native view tree as the mount items built it.
#[derive(Default)]
struct Views {
    nodes: HashMap<NodeId, View>,
    children: HashMap<NodeId, Vec<NodeId>>,
}

#[derive(Default)]
struct View {
    parent: Option<NodeId>,
    pressable: bool,
    test_id: Option<Arc<str>>,
    label: Option<Arc<str>>,
    text: Option<String>,
    frame: Option<Rect>,
    /// What the platform got for it, as `prop` and `style` read it.
    mounted: Mounted,
}

impl Views {
    fn apply(&mut self, batch: &MountBatch) {
        for item in &batch.items {
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
                    self.nodes.entry(*id).or_default().pressable =
                        events.contains(EventKind::Press);
                }
                MountItem::ViewProps { id, props } => {
                    let view = self.nodes.entry(*id).or_default();
                    view.test_id = props.test_id.clone();
                    view.label = props.accessibility.label.clone();
                }
                MountItem::Text { id, text, .. } => {
                    self.nodes.entry(*id).or_default().text = Some(text.text.clone());
                }
                MountItem::Frame { id, frame, .. } => {
                    self.nodes.entry(*id).or_default().frame = Some(Rect::from(*frame));
                }
                _ => {}
            }
            values::animated(Some(item));
            if let Some(id) = Mounted::target(item) {
                self.nodes.entry(id).or_default().mounted.apply(item);
            }
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

    fn texts(&self) -> Vec<String> {
        self.attached()
            .iter()
            .filter_map(|id| self.nodes.get(id)?.text.as_deref())
            .map(|t| t.trim().to_owned())
            .collect()
    }

    fn shows(&self, text: &str) -> bool {
        self.texts().iter().any(|t| t == text)
    }

    /// The first view on screen `selector` names, or its nearest pressable
    /// ancestor.
    fn find(&self, selector: &str) -> Option<NodeId> {
        self.find_view(selector)
            .and_then(|id| self.pressable_from(id))
    }

    /// The first view on screen `selector` names.
    fn find_view(&self, selector: &str) -> Option<NodeId> {
        if let Some(inner) = selector.strip_prefix("scroll:") {
            return self.scroll_around(inner);
        }
        let is = |value: &Option<Arc<str>>, want: &str| value.as_deref() == Some(want);
        let test: Box<dyn Fn(&View) -> bool> = if let Some(want) = selector.strip_prefix("id:") {
            Box::new(move |v| is(&v.test_id, want))
        } else if let Some(want) = selector.strip_prefix("label:") {
            Box::new(move |v| is(&v.label, want))
        } else if let Some(want) = selector.strip_prefix("text:") {
            Box::new(move |v| v.text.as_deref().map(str::trim) == Some(want))
        } else if let Some(want) = selector.strip_prefix("span:") {
            Box::new(move |v| v.text.as_deref().is_some_and(|t| t.contains(want)))
        } else {
            panic!("selector `{selector}`: expected id:, label:, text: or span:")
        };
        self.attached()
            .into_iter()
            .find(|id| self.nodes.get(id).is_some_and(&test))
    }

    /// The nearest scroll view around the first view on screen `selector`
    /// names (a list has no `testID` of its own: its rows name it).
    fn scroll_around(&self, selector: &str) -> Option<NodeId> {
        let mut id = self.find_view(selector)?;
        loop {
            id = self.nodes.get(&id)?.parent?;
            if self.nodes.get(&id)?.mounted.is_scroll() {
                return Some(id);
            }
        }
    }

    /// The mounted state of the first view on screen `selector` names, and
    /// for `span:<text>`, the UTF-16 offset in its text where `<text>`
    /// starts.
    fn mounted(&self, selector: &str) -> Option<(&Mounted, Option<u32>)> {
        let view = self.nodes.get(&self.find_view(selector)?)?;
        let at = selector.strip_prefix("span:").and_then(|want| {
            let text = view.text.as_deref()?;
            let byte = text.find(want)?;
            Some(text[..byte].encode_utf16().count() as u32)
        });
        Some((&view.mounted, at))
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

struct Run {
    _engine: Engine,
    control: EngineControl,
    rx: Receiver<MountBatch>,
    errors: Arc<Mutex<Vec<String>>>,
    thread: Arc<OnceLock<ThreadId>>,
    views: Views,
}

impl Run {
    fn start(
        bundle: Bundle,
        viewport: Size<Points>,
        environment: Environment,
        assets: &Path,
        data_dir: Option<PathBuf>,
    ) -> Run {
        crate::logs::capture();
        let (tx, rx) = crossbeam_channel::unbounded();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let thread = Arc::new(OnceLock::new());
        let config = EngineConfig {
            app: nsh_engine::App::Js(bundle),
            viewport,
            trace: false,
            network: None,
            websocket: None,
            filesystem: None,
            assets: Some(Arc::new(AppAssets(assets.to_path_buf()))),
            environment,
            header_heights: (nsh_core::Points(44.0), nsh_core::Points(96.0)),
            limits: nsh_engine::Limits::default(),
            ui_sets: vec![nsh_base::registry()],
            runtime: None,
            data_dir,
            storage: None,
        };
        let sink = Sink {
            batches: tx,
            errors: errors.clone(),
            thread: thread.clone(),
        };
        let engine = Engine::start(config, || ApproxTextEngine, sink).unwrap();
        Run {
            control: engine.control(),
            _engine: engine,
            rx,
            errors,
            thread,
            views: Views::default(),
        }
    }

    /// Whether the app printed a line containing `text`.
    fn printed(&self, text: &str) -> bool {
        self.thread
            .get()
            .is_some_and(|t| crate::logs::printed_by(*t).iter().any(|l| l.contains(text)))
    }

    /// Lets the app run until `done` holds or `limit` ends.
    fn poll(&mut self, limit: Duration, done: impl Fn(&Run) -> bool) -> bool {
        let end = Instant::now() + limit;
        loop {
            if done(self) {
                return true;
            }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            match self.rx.recv_timeout(left.min(Duration::from_millis(20))) {
                Ok(batch) => self.views.apply(&batch),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => panic!("the engine stopped"),
            }
        }
    }

    /// Applies the batches that arrive until `done` holds or `limit` ends.
    fn until(&mut self, limit: Duration, done: impl Fn(&Views) -> bool) -> bool {
        let end = Instant::now() + limit;
        while !done(&self.views) {
            let left = end.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(batch) => self.views.apply(&batch),
                Err(RecvTimeoutError::Timeout) => return false,
                Err(RecvTimeoutError::Disconnected) => panic!("the engine stopped"),
            }
        }
        true
    }

    /// Applies batches until none came for [`QUIET`] (or `limit` ends).
    fn settle(&mut self, limit: Duration) {
        let end = Instant::now() + limit;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(QUIET.min(left)) {
                Ok(batch) => self.views.apply(&batch),
                Err(_) => return,
            }
        }
    }

    fn errors(&self) -> Vec<String> {
        self.errors.lock().unwrap().clone()
    }
}

/// Runs the app in `dir` (`bundle.js`, `script.txt`); the first failed step
/// is the error.
pub(crate) fn run_app(dir: &Path) -> Result<(), String> {
    let code = std::fs::read_to_string(dir.join("bundle.js")).map_err(|e| e.to_string())?;
    let script = std::fs::read_to_string(dir.join("script.txt")).map_err(|e| e.to_string())?;
    run(code, &script, dir).map_err(|f| f.message)
}

/// `code` as the bundle of the fixture app in `dir`: with the capabilities
/// it declared and its id, when its build wrote them.
fn bundle(code: String, dir: &Path) -> Result<(Bundle, bool), String> {
    let bundle = Bundle::source(code);
    let Ok(json) = std::fs::read_to_string(dir.join("capabilities.json")) else {
        return Ok((bundle, false));
    };
    let id = std::fs::read_to_string(dir.join("app.json"))
        .ok()
        .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
        .and_then(|v| v.get("id")?.as_str().map(str::to_owned));
    let bundle = bundle
        .with_app_id(id.as_deref())
        .with_capabilities_json(Some(&json))
        .map_err(|e| format!("{}: {e}", dir.join("capabilities.json").display()))?;
    Ok((bundle, true))
}

/// A new, empty data directory for one run of an app, removed with it.
struct DataDir(PathBuf);

impl DataDir {
    fn new() -> DataDir {
        static RUNS: AtomicUsize = AtomicUsize::new(0);
        let dir = crate::temporary_root().join(format!(
            "nsh-compat-data-{}-{}",
            std::process::id(),
            RUNS.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        DataDir(dir)
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Why a script failed.
pub(crate) struct Failure {
    /// The step that failed, when one of its checks did (not an error the
    /// app reported, nor a malformed script).
    pub(crate) check: Option<String>,
    /// Whether the app reported errors.
    pub(crate) errors: bool,
    pub(crate) message: String,
}

impl From<String> for Failure {
    fn from(message: String) -> Failure {
        Failure {
            check: None,
            errors: false,
            message,
        }
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Failure {
        message.to_owned().into()
    }
}

/// Runs the bundle `code` of the fixture app in `dir` (its assets and
/// declared capabilities) through `script`.
pub(crate) fn run(code: String, script: &str, dir: &Path) -> Result<(), Failure> {
    let dump = std::env::var_os("NSH_API_DUMP").is_some();
    let mut lines = script
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .peekable();
    let mut viewport = Size::new(nsh_core::geometry::px(390.0), nsh_core::geometry::px(844.0));
    let mut environment = Environment::default();
    while let Some(line) = lines.peek() {
        if let Some(size) = line.strip_prefix("viewport ") {
            let (w, h) = size.split_once(' ').ok_or("viewport <width> <height>")?;
            viewport = Size::new(px(w.parse().unwrap()), px(h.parse().unwrap()));
        } else if let Some(platform) = line.strip_prefix("platform ") {
            let mut words = platform.split_whitespace();
            environment.os = match words.next() {
                Some("ios") => Os::Ios,
                Some("android") => Os::Android,
                Some("macos") => Os::MacOs,
                Some("windows") => Os::Windows,
                Some("linux") => Os::Linux,
                Some("web") => Os::Web,
                _ => {
                    return Err(format!(
                        "`{line}`: platform ios|android|macos|windows|linux|web [desktop]"
                    )
                    .into());
                }
            };
            let desktop = match words.next() {
                None => environment.os.is_desktop(),
                Some("desktop") => true,
                Some(other) => return Err(format!("`{line}`: `{other}`, not `desktop`").into()),
            };
            if desktop {
                environment.idiom = Idiom::Desktop;
            }
        } else if let Some(scale) = line.strip_prefix("fontscale ") {
            environment.font_scale = scale
                .trim()
                .parse()
                .map_err(|_| format!("`{line}`: fontscale <scale>"))?;
        } else {
            break;
        }
        lines.next();
    }
    values::animated(None);
    let (bundle, declares) = bundle(code, dir)?;
    // Declared before the run, so dropped after its engine, which may
    // still write there.
    let data = declares.then(DataDir::new);
    let mut run = Run::start(
        bundle,
        viewport,
        environment,
        &dir.join("assets"),
        data.as_ref().map(|d| d.0.clone()),
    );
    if !run.until(EXPECT_WAIT, |v| !v.attached().is_empty()) {
        return Err(Failure {
            check: None,
            errors: !run.errors().is_empty(),
            message: format!("the app rendered nothing; errors: {:?}", run.errors()),
        });
    }
    if dump {
        run.settle(EXPECT_WAIT);
        println!("(start)\n  {:?}", run.views.texts());
    }
    let mut shown = run.views.texts();
    let mut device = Device::default();
    for (n, line) in lines.enumerate() {
        let (step, arg) = line.split_once(' ').unwrap_or((line, ""));
        let failed = |what: String, run: &Run| {
            let errors = run.errors();
            Err(Failure {
                check: (what != "the app reported errors").then(|| line.to_owned()),
                errors: !errors.is_empty(),
                message: format!(
                    "step {} `{line}`: {what}\non screen: {:?}\nerrors: {errors:?}",
                    n + 1,
                    run.views.texts(),
                ),
            })
        };
        match step {
            "expect" => {
                if !run.until(EXPECT_WAIT, |v| v.shows(arg)) {
                    return failed("not shown".into(), &run);
                }
            }
            "absent" => {
                run.settle(EXPECT_WAIT);
                if run.views.shows(arg) {
                    return failed("still shown".into(), &run);
                }
            }
            "tap" => {
                run.settle(TAP_WAIT);
                let Some(id) = run.views.find(arg) else {
                    return failed("nothing to press".into(), &run);
                };
                run.control
                    .dispatch_event(id, EventKind::Press, EventPayload::None);
            }
            "wait" => {
                let ms: u64 = arg.parse().map_err(|_| format!("wait `{arg}`"))?;
                run.until(Duration::from_millis(ms), |_| false);
            }
            "frame" => {
                let mut parts = arg.split(' ');
                let selector = parts.next().unwrap_or_default();
                let want: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
                let [x, y, w, h] = want[..] else {
                    return Err(
                        format!("frame `{arg}`: <selector> <x> <y> <width> <height>").into(),
                    );
                };
                let at = |v: &Views| v.find_view(selector).and_then(|id| v.nodes.get(&id)?.frame);
                let fits = |v: &Views| {
                    at(v).is_some_and(|f| {
                        [f.x - x, f.y - y, f.width - w, f.height - h]
                            .iter()
                            .all(|d| d.abs() < 0.5)
                    })
                };
                if !run.until(EXPECT_WAIT, fits) {
                    let what = match at(&run.views) {
                        Some(f) => format!("laid out at {} {} {} {}", f.x, f.y, f.width, f.height),
                        None => "no such view".to_owned(),
                    };
                    return failed(what, &run);
                }
            }
            "prop" | "style" => {
                let (selector, rest) = selector(arg);
                let (name, want) = rest
                    .split_once(' ')
                    .ok_or_else(|| format!("`{line}`: {step} <selector> <name> <value>"))?;
                let (key, want) = (values::key(name), want.trim());
                let all = |v: &Views| {
                    let (m, at) = v.mounted(&selector)?;
                    Some(if step == "prop" {
                        m.props()
                    } else {
                        m.styles(at)
                    })
                };
                let read = |v: &Views| all(v)?.remove(key);
                if !run.until(EXPECT_WAIT, |v| read(v).as_deref() == Some(want)) {
                    let what = match all(&run.views) {
                        None => "no such view".to_owned(),
                        Some(all) => {
                            // What the view has, for writing the step.
                            let has: Vec<String> =
                                all.iter().map(|(k, v)| format!("{k} {v}")).collect();
                            let got = match all.get(key) {
                                None => format!("no {step} `{key}`"),
                                Some(got) => format!("`{key}` is `{got}`"),
                            };
                            format!("{got}\nthe view's {step}s:\n  {}", has.join("\n  "))
                        }
                    };
                    return failed(what, &run);
                }
            }
            "noprop" | "nostyle" => {
                let (selector, name) = selector(arg);
                let key = values::key(name.trim());
                run.settle(EXPECT_WAIT);
                let Some((m, at)) = run.views.mounted(&selector) else {
                    return failed("no such view".into(), &run);
                };
                let (got, unset) = if step == "noprop" {
                    (m.props().remove(key), m.blank().props().remove(key))
                } else {
                    (m.styles(at).remove(key), m.blank().styles(at).remove(key))
                };
                if let Some(got) = got.filter(|g| Some(g) != unset.as_ref()) {
                    return failed(format!("`{key}` is `{got}`"), &run);
                }
            }
            "listens" => {
                let (selector, event) = selector(arg);
                let event = event.trim();
                let kind =
                    event_kind(event).ok_or_else(|| format!("`{line}`: `{event}` is no event"))?;
                let listens =
                    |v: &Views| v.mounted(&selector).is_some_and(|(m, _)| m.listens(kind));
                if !run.until(EXPECT_WAIT, listens) {
                    return failed(format!("`{event}` not reported"), &run);
                }
            }
            "log" => {
                if !run.poll(EXPECT_WAIT, |r| r.printed(arg)) {
                    return failed("not printed".into(), &run);
                }
            }
            "event" => {
                let (selector, rest) = selector(arg);
                let rest = rest.trim();
                let (name, fields) = rest.split_once(' ').unwrap_or((rest, ""));
                let kind =
                    event_kind(name).ok_or_else(|| format!("`{line}`: `{name}` is no event"))?;
                let fields = parse_fields(fields).map_err(|e| format!("`{line}`: {e}"))?;
                run.settle(TAP_WAIT);
                let Some(id) = run
                    .views
                    .find_view(&selector)
                    .and_then(|id| listener(&run.views, id, kind))
                else {
                    return failed(format!("nothing there reports `{name}`"), &run);
                };
                let payload = event_payload(&run.views, id, kind, fields, &mut device)
                    .map_err(|e| format!("`{line}`: {e}"))?;
                run.control.dispatch_event(id, kind, payload);
            }
            "touch" => {
                let (phase, rest) = arg.split_once(' ').unwrap_or((arg, ""));
                run.settle(TAP_WAIT);
                let touch = match phase {
                    "start" => {
                        let (selector, at) = selector(rest);
                        let at = numbers::<2>(at)
                            .ok_or_else(|| format!("`{line}`: touch start <selector> <x> <y>"))?;
                        let Some(target) = run.views.find_view(&selector) else {
                            return failed("no such view".into(), &run);
                        };
                        let path = touch_path(&run.views, target);
                        if path.is_empty() {
                            return failed("nothing there listens to touches".into(), &run);
                        }
                        device.finger = Some((target, path, at));
                        device.touch(&run.views, TouchPhase::Start)
                    }
                    "move" | "end" | "cancel" => {
                        let Some(finger) = device.finger.as_mut() else {
                            return Err(format!("`{line}`: no finger is down").into());
                        };
                        if let Some(at) = numbers::<2>(rest) {
                            finger.2 = at;
                        } else if phase == "move" || !rest.trim().is_empty() {
                            return Err(format!("`{line}`: touch {phase} <x> <y>").into());
                        }
                        let phase = match phase {
                            "move" => TouchPhase::Move,
                            "end" => TouchPhase::End,
                            _ => TouchPhase::Cancel,
                        };
                        let touch = device.touch(&run.views, phase);
                        if phase != TouchPhase::Move {
                            device.finger = None;
                        }
                        touch
                    }
                    _ => return Err(format!("`{line}`: touch start|move|end|cancel").into()),
                };
                run.control.dispatch_event(
                    ROOT,
                    EventKind::Touch,
                    EventPayload::Touch(Box::new(touch)),
                );
            }
            "type" | "key" => {
                let (selector, text) = selector(arg);
                run.settle(TAP_WAIT);
                let Some(id) = run.views.find_view(&selector) else {
                    return failed("no such view".into(), &run);
                };
                let key =
                    |key: &str| EventPayload::Map(vec![("key", PayloadValue::Str(key.to_owned()))]);
                if step == "key" {
                    run.control
                        .dispatch_event(id, EventKind::KeyPress, key(text.trim()));
                } else {
                    let value = run
                        .views
                        .nodes
                        .get(&id)
                        .and_then(|v| v.mounted.props().remove("value"));
                    let typed = device.typed.entry(id).or_default();
                    // A value the app set since replaces what was typed.
                    if typed.1 != value {
                        typed.0 = value.clone().unwrap_or_default();
                        typed.1 = value;
                    }
                    for c in text.chars() {
                        typed.0.push(c);
                        typed.2 += 1;
                        let caret = PayloadValue::Number(typed.0.encode_utf16().count() as f64);
                        run.control
                            .dispatch_event(id, EventKind::KeyPress, key(&c.to_string()));
                        run.control.dispatch_event(
                            id,
                            EventKind::ChangeText,
                            EventPayload::Text {
                                text: typed.0.clone(),
                                count: typed.2,
                            },
                        );
                        run.control.dispatch_event(
                            id,
                            EventKind::SelectionChange,
                            EventPayload::Map(vec![("start", caret.clone()), ("end", caret)]),
                        );
                    }
                }
            }
            "scroll" => {
                let (selector, at) = selector(arg);
                let [x, y] = numbers::<2>(at)
                    .ok_or_else(|| format!("`{line}`: scroll <selector> <x> <y>"))?;
                run.settle(TAP_WAIT);
                let Some(id) = run.views.find_view(&selector) else {
                    return failed("no such view".into(), &run);
                };
                run.control.scroll(id, point(px(x), px(y)), 0);
                if let Some(view) = run.views.nodes.get_mut(&id) {
                    view.mounted.scrolled(x, y);
                }
            }
            "origin" => {
                let (selector, at) = selector(arg);
                let [x, y] = numbers::<2>(at)
                    .ok_or_else(|| format!("`{line}`: origin <selector> <x> <y>"))?;
                let at = |v: &Views| v.find_view(&selector).map(|id| page_origin(v, id));
                let fits = |v: &Views| {
                    at(v).is_some_and(|(left, top)| (left - x).abs() < 0.5 && (top - y).abs() < 0.5)
                };
                if !run.until(EXPECT_WAIT, fits) {
                    let what = match at(&run.views) {
                        Some((left, top)) => format!("at {left} {top}"),
                        None => "no such view".to_owned(),
                    };
                    return failed(what, &run);
                }
            }
            "state" => {
                let (selector, names) = selector(arg);
                let mut states = 0;
                for name in names.split_whitespace() {
                    states |= match name {
                        "pressed" => state::PRESSED,
                        "hovered" => state::HOVERED,
                        "focused" => state::FOCUSED,
                        "none" => 0,
                        _ => {
                            return Err(format!(
                                "`{line}`: state <selector> pressed|hovered|focused|none..."
                            )
                            .into());
                        }
                    };
                }
                let tracks = |v: &Views| {
                    v.find_view(&selector)
                        .filter(|id| v.nodes.get(id).is_some_and(|n| n.mounted.tracks(states)))
                };
                if !run.until(EXPECT_WAIT, |v| tracks(v).is_some()) {
                    return failed(format!("the platform does not track `{names}`"), &run);
                }
                let id = tracks(&run.views).unwrap();
                run.control.set_states(id, states);
            }
            "keyboard" => {
                let [height, duration] = numbers::<2>(arg)
                    .ok_or_else(|| format!("`{line}`: keyboard <height> <duration>"))?;
                environment.keyboard_height = px(height);
                environment.keyboard_duration = duration;
                run.control.set_environment(environment);
            }
            "statusbar" => {
                let want = parse_fields(arg).map_err(|e| format!("`{line}`: {e}"))?;
                if want.is_empty() {
                    return Err(format!("`{line}`: statusbar <key>=<value>...").into());
                }
                let want: Vec<(String, String)> = want
                    .into_iter()
                    .map(|(k, f)| {
                        let v = match f {
                            Field::Bool(b) => b.to_string(),
                            Field::Str(s) => s,
                            Field::Number(n) => values::num(n as f32),
                        };
                        (k, v)
                    })
                    .collect();
                for (key, _) in &want {
                    if !["foreground", "hidden", "animated"].contains(&key.as_str()) {
                        return Err(format!("`{line}`: no status bar key `{key}`").into());
                    }
                }
                let read = |v: &Views| v.nodes.get(&ROOT)?.mounted.status_bar();
                let fits = |v: &Views| {
                    read(v).is_some_and(|got| {
                        want.iter()
                            .all(|(k, v)| got.get(k.as_str()).is_some_and(|g| g == v))
                    })
                };
                if !run.until(EXPECT_WAIT, fits) {
                    let what = match read(&run.views) {
                        None => "the platform was told no status bar".to_owned(),
                        Some(got) => {
                            let got: Vec<String> =
                                got.iter().map(|(k, v)| format!("{k}={v}")).collect();
                            format!("the status bar is {}", got.join(" "))
                        }
                    };
                    return failed(what, &run);
                }
            }
            "covers" => continue,
            other => return Err(format!("unknown step `{other}`").into()),
        }
        if dump {
            run.settle(EXPECT_WAIT);
            let now = run.views.texts();
            let added: Vec<_> = now.iter().filter(|t| !shown.contains(t)).collect();
            let gone: Vec<_> = shown.iter().filter(|t| !now.contains(t)).collect();
            println!("{line}\n  + {added:?}\n  - {gone:?}");
            shown = now;
        }
        if !run.errors().is_empty() {
            return failed("the app reported errors".into(), &run);
        }
    }
    run.settle(EXPECT_WAIT);
    match run.errors() {
        errors if errors.is_empty() => Ok(()),
        errors => Err(Failure {
            check: None,
            errors: true,
            message: format!("the app reported errors: {errors:?}"),
        }),
    }
}

/// Splits a step's argument into its selector and the rest: the selector
/// is one word, or a prefix and a quoted value (`text:"Taps: 0"`).
fn selector(arg: &str) -> (String, &str) {
    if let Some((prefix, quoted)) = arg.split_once(":\"")
        && !prefix.contains(' ')
        && let Some((value, rest)) = quoted.split_once('"')
    {
        return (format!("{prefix}:{value}"), rest.trim_start());
    }
    match arg.split_once(' ') {
        Some((selector, rest)) => (selector.to_owned(), rest),
        None => (arg.to_owned(), ""),
    }
}

/// What the steps that stand in for the platform's input keep between
/// steps.
#[derive(Default)]
struct Device {
    /// The finger down: the view it went down on, the views around it
    /// that listen to touches (innermost first) and where it is in the
    /// view.
    finger: Option<(NodeId, Vec<NodeId>, [f32; 2])>,
    /// Touch events so far.
    touches: u32,
    /// Per input: its text, the value the app had set when it was read,
    /// and the edits reported.
    typed: HashMap<NodeId, (String, Option<String>, u32)>,
}

impl Device {
    /// The finger's touch event of `phase`.
    fn touch(&mut self, views: &Views, phase: TouchPhase) -> TouchEvent {
        let (target, path, [x, y]) = self.finger.clone().expect("a finger down");
        self.touches += 1;
        let (left, top) = page_origin(views, target);
        TouchEvent {
            phase,
            timestamp: 16.0 * self.touches as f64,
            activated: None,
            points: vec![TouchPoint {
                id: 1,
                target,
                path,
                page: point(px(left + x), px(top + y)),
                location: point(px(x), px(y)),
                changed: true,
            }],
        }
    }
}

/// A value of an `event` step's payload.
enum Field {
    Number(f64),
    Bool(bool),
    Str(String),
}

impl Field {
    fn number(&self) -> Option<f32> {
        match self {
            Field::Number(n) => Some(*n as f32),
            _ => None,
        }
    }

    fn value(self) -> PayloadValue {
        match self {
            Field::Number(n) => PayloadValue::Number(n),
            Field::Bool(b) => PayloadValue::Bool(b),
            Field::Str(s) => PayloadValue::Str(s),
        }
    }
}

/// `key=value key="a value" ...`.
fn parse_fields(text: &str) -> Result<Vec<(String, Field)>, String> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let (key, after) = rest
            .split_once('=')
            .ok_or_else(|| format!("`{rest}`: expected <key>=<value>"))?;
        let (field, after) = if let Some(quoted) = after.strip_prefix('"') {
            let (value, after) = quoted
                .split_once('"')
                .ok_or_else(|| format!("`{after}`: unterminated quote"))?;
            (Field::Str(value.to_owned()), after)
        } else {
            let (value, after) = after.split_once(' ').unwrap_or((after, ""));
            let field = match value {
                "true" => Field::Bool(true),
                "false" => Field::Bool(false),
                _ => match value.parse::<f64>() {
                    Ok(n) => Field::Number(n),
                    Err(_) => Field::Str(value.to_owned()),
                },
            };
            (field, after)
        };
        out.push((key.trim().to_owned(), field));
        rest = after.trim_start();
    }
    Ok(out)
}

/// The event a script's `on…` name reports: a host element's handler as
/// apps write it (`onChange`), a handler of `nsh/base`'s `NavPage` and
/// `Dialog` (the runtime's own listeners of the platform's events), or the
/// platform protocol's name (`onChangeText`).
fn event_kind(name: &str) -> Option<EventKind> {
    EventKind::from_handler(name)
        .or(match name {
            // `NavPage`.
            "onDismiss" => Some(EventKind::Dismiss),
            "onBackPress" => Some(EventKind::BackRequest),
            // `Dialog`.
            "onCancel" | "onOpenChange" => Some(EventKind::RequestClose),
            "onOpen" => Some(EventKind::Show),
            "onClose" => Some(EventKind::Dismiss),
            _ => None,
        })
        .or_else(|| EventKind::from_prop(name))
}

/// The payload of an `event` step, as the platforms make it for `kind`.
fn event_payload(
    views: &Views,
    id: NodeId,
    kind: EventKind,
    fields: Vec<(String, Field)>,
    device: &mut Device,
) -> Result<EventPayload, String> {
    let get = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, f)| f);
    let num = |key: &str| get(key).and_then(Field::number);
    let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
    let only = |want: &[&str]| keys.len() == want.len() && want.iter().all(|k| keys.contains(k));
    Ok(match kind {
        // Both payloads are a position and the modifiers.
        EventKind::HoverMove | EventKind::ContextMenu => {
            let (left, top) = page_origin(views, id);
            let (location_x, location_y) = (
                num("locationX").unwrap_or(0.0),
                num("locationY").unwrap_or(0.0),
            );
            let flag = |key: &str| matches!(get(key), Some(Field::Bool(true)));
            let (page_x, page_y) = (
                num("pageX").unwrap_or(left + location_x),
                num("pageY").unwrap_or(top + location_y),
            );
            let modifiers = Modifiers {
                ctrl: flag("ctrlKey"),
                alt: flag("altKey"),
                shift: flag("shiftKey"),
                cmd: flag("metaKey"),
            };
            if kind == EventKind::HoverMove {
                EventPayload::Hover(HoverEvent {
                    location: point(px(location_x), px(location_y)),
                    page: point(px(page_x), px(page_y)),
                    modifiers,
                })
            } else {
                EventPayload::ContextMenu(ContextMenuEvent {
                    location: point(px(location_x), px(location_y)),
                    page: point(px(page_x), px(page_y)),
                    modifiers,
                })
            }
        }
        _ if fields.is_empty() => EventPayload::None,
        EventKind::ChangeText => {
            let Some(Field::Str(text)) = get("text") else {
                return Err("onChange takes text=<string>".into());
            };
            let typed = device.typed.entry(id).or_default();
            typed.0 = text.clone();
            typed.2 += 1;
            EventPayload::Text {
                text: text.clone(),
                count: typed.2,
            }
        }
        EventKind::FileDrop if keys.iter().all(|k| *k == "uris") => EventPayload::FileDrop(
            fields
                .into_iter()
                .map(|(_, f)| match f {
                    Field::Str(s) => s,
                    Field::Number(n) => n.to_string(),
                    Field::Bool(b) => b.to_string(),
                })
                .collect(),
        ),
        EventKind::PanStart | EventKind::PanEnd => EventPayload::Pan {
            translation: point(
                px(num("translationX").unwrap_or(0.0)),
                px(num("translationY").unwrap_or(0.0)),
            ),
            velocity_x: num("velocityX").unwrap_or(0.0),
            velocity_y: num("velocityY").unwrap_or(0.0),
            value_x: num("valueX").map(px),
            value_y: num("valueY").map(px),
        },
        EventKind::ScrollEnd
        | EventKind::ScrollBeginDrag
        | EventKind::ScrollEndDrag
        | EventKind::MomentumScrollBegin
        | EventKind::MomentumScrollEnd
            if only(&["x", "y"]) =>
        {
            EventPayload::Scroll(point(
                px(num("x").unwrap_or(0.0)),
                px(num("y").unwrap_or(0.0)),
            ))
        }
        EventKind::Load if only(&["width", "height"]) => EventPayload::Image(Size::new(
            DevicePixels(num("width").unwrap_or(0.0) as i32),
            DevicePixels(num("height").unwrap_or(0.0) as i32),
        )),
        EventKind::Error if only(&["message"]) => match get("message") {
            Some(Field::Str(message)) => EventPayload::Error {
                message: message.clone(),
            },
            _ => return Err("onError takes message=<string>".into()),
        },
        _ => EventPayload::Map(
            fields
                .into_iter()
                // The payload's keys are static: a test's few leak.
                .map(|(k, f)| (&*Box::leak(k.into_boxed_str()), f.value()))
                .collect(),
        ),
    })
}

/// `id`, or its nearest ancestor, that the platform reports `kind` of.
fn listener(views: &Views, mut id: NodeId, kind: EventKind) -> Option<NodeId> {
    loop {
        let view = views.nodes.get(&id)?;
        if view.mounted.listens(kind) {
            return Some(id);
        }
        id = view.parent?;
    }
}

/// `id` and the views around it that listen to touches, innermost first.
fn touch_path(views: &Views, id: NodeId) -> Vec<NodeId> {
    let mut path = Vec::new();
    let mut at = Some(id);
    while let Some(id) = at {
        let Some(view) = views.nodes.get(&id) else {
            break;
        };
        if view.mounted.listens(EventKind::Touch) {
            path.push(id);
        }
        at = view.parent;
    }
    path
}

/// A view's frame in points relative to its parent, as the steps read it.
#[derive(Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl From<nsh_core::Bounds<Points>> for Rect {
    fn from(b: nsh_core::Bounds<Points>) -> Self {
        Rect {
            x: b.origin.x.0,
            y: b.origin.y.0,
            width: b.size.width.0,
            height: b.size.height.0,
        }
    }
}

/// Where view `id` is on screen: its frames' origins added up.
fn page_origin(views: &Views, mut id: NodeId) -> (f32, f32) {
    let (mut x, mut y) = (0.0, 0.0);
    while let Some(view) = views.nodes.get(&id) {
        if let Some(frame) = view.frame {
            x += frame.x;
            y += frame.y;
        }
        match view.parent {
            Some(parent) => id = parent,
            None => break,
        }
    }
    (x, y)
}

/// `N` numbers separated by spaces.
fn numbers<const N: usize>(text: &str) -> Option<[f32; N]> {
    let parsed: Vec<f32> = text
        .split_whitespace()
        .map(|w| w.parse().ok())
        .collect::<Option<_>>()?;
    parsed.try_into().ok()
}
