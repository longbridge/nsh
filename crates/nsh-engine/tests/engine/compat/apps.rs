//! Apps built by released versions run on this one: `api/<version>/apps/`
//! keeps what that version's `nsh build --release` wrote for some apps
//! (`<app>/bundle.js`; packages always carry their source, which runs when
//! their bytecode is not this runtime's) and a script of what a user does
//! and sees (`<app>/script.txt`), one step per line (with the assets it
//! imports, `<app>/assets/`):
//!
//! - `viewport <width> <height>`: before anything else, the screen's size
//!   (390 × 844 by default);
//! - `expect <text>`: a text on screen reads `<text>` (waits for it);
//! - `absent <text>`: no text on screen reads `<text>` (after the app is
//!   quiet);
//! - `tap <selector>`: presses the first view on screen `id:<testID>`,
//!   `label:<accessibility label>` or `text:<text>` names, or its nearest
//!   pressable ancestor (as `nsh trace --tap`);
//! - `wait <ms>`: lets timers and animations run.
//!
//! Any error the app reports fails it. `NSH_API_DUMP=1` prints the texts
//! on screen after each step, for writing scripts.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use nsh_core::{ApproxTextEngine, EventKind, EventPayload, MountBatch, MountItem, NodeId, Size};
use nsh_engine::{Assets, Bundle, Engine, EngineConfig, EngineControl, MountSink};

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
}

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
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
                _ => {}
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
        let is = |value: &Option<Arc<str>>, want: &str| value.as_deref() == Some(want);
        let test: Box<dyn Fn(&View) -> bool> = if let Some(want) = selector.strip_prefix("id:") {
            Box::new(move |v| is(&v.test_id, want))
        } else if let Some(want) = selector.strip_prefix("label:") {
            Box::new(move |v| is(&v.label, want))
        } else if let Some(want) = selector.strip_prefix("text:") {
            Box::new(move |v| v.text.as_deref().map(str::trim) == Some(want))
        } else {
            panic!("selector `{selector}`: expected id:, label: or text:")
        };
        self.attached().into_iter().find_map(|id| {
            self.nodes
                .get(&id)
                .filter(|v| test(v))
                .and_then(|_| self.pressable_from(id))
        })
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
    views: Views,
}

impl Run {
    fn start(bundle: Bundle, viewport: Size, assets: &Path) -> Run {
        let (tx, rx) = crossbeam_channel::unbounded();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let config = EngineConfig {
            app: nsh_engine::App::Js(bundle),
            viewport,
            trace: false,
            network: None,
            assets: Some(Arc::new(AppAssets(assets.to_path_buf()))),
            environment: Default::default(),
            header_heights: (44.0, 96.0),
            limits: nsh_engine::Limits::default(),
            ui_sets: vec![nsh_ui_mobile::registry()],
            runtime: None,
            data_dir: None,
            storage: None,
        };
        let sink = Sink {
            batches: tx,
            errors: errors.clone(),
        };
        let engine = Engine::start(config, || ApproxTextEngine, sink).unwrap();
        Run {
            control: engine.control(),
            _engine: engine,
            rx,
            errors,
            views: Views::default(),
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
    let dump = std::env::var_os("NSH_API_DUMP").is_some();
    let mut lines = script
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .peekable();
    let mut viewport = Size::new(390.0, 844.0);
    if let Some(line) = lines.peek()
        && let Some(size) = line.strip_prefix("viewport ")
    {
        let (w, h) = size.split_once(' ').ok_or("viewport <width> <height>")?;
        viewport = Size::new(w.parse().unwrap(), h.parse().unwrap());
        lines.next();
    }
    let mut run = Run::start(Bundle::source(code), viewport, &dir.join("assets"));
    if !run.until(EXPECT_WAIT, |v| !v.attached().is_empty()) {
        return Err(format!(
            "the app rendered nothing; errors: {:?}",
            run.errors()
        ));
    }
    if dump {
        run.settle(EXPECT_WAIT);
        println!("(start)\n  {:?}", run.views.texts());
    }
    let mut shown = run.views.texts();
    for (n, line) in lines.enumerate() {
        let (step, arg) = line.split_once(' ').unwrap_or((line, ""));
        let failed = |what: String, run: &Run| {
            Err(format!(
                "step {} `{line}`: {what}\non screen: {:?}\nerrors: {:?}",
                n + 1,
                run.views.texts(),
                run.errors()
            ))
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
            other => return Err(format!("unknown step `{other}`")),
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
        errors => Err(format!("the app reported errors: {errors:?}")),
    }
}
