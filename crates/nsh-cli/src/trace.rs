use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use anyhow::{Context as _, bail};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use nsh_bundler::BundleOptions;
use nsh_core::{ApproxTextEngine, EventKind, MountBatch, MountItem, NodeId, Size};
use nsh_engine::{Bundle, Engine, EngineConfig, EngineControl, Environment, Insets, MountSink};

use crate::{build::BuildError, ui};

/// The surface's root view: every app view hangs below it.
const ROOT: NodeId = 1;
/// How long a quiet wait ([`TraceOptions::wait`] not given) may take at
/// most: an app with a running timer or animation is never quiet.
const MAX_SETTLE: Duration = Duration::from_secs(10);
/// How long the app must stay quiet after a tap before the next one (without
/// [`TraceOptions::wait`]).
const TAP_QUIET: Duration = Duration::from_millis(100);

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

/// What `nsh trace` does after the first render.
pub(crate) struct TraceOptions {
    /// Taps on the first pressable view (when `tap` is empty).
    pub(crate) taps: u32,
    /// Views to tap, in order ([`Selector`]).
    pub(crate) tap: Vec<String>,
    /// How long the app must be quiet before the taps.
    pub(crate) idle_ms: u64,
    /// Run this long before the first tap and after each one, instead of
    /// waiting for quiet.
    pub(crate) wait: Option<u64>,
    /// Keep running this long after the last tap.
    pub(crate) duration: u64,
    /// Points to scroll the first scroll view by.
    pub(crate) scroll: Option<f32>,
    /// Viewport.
    pub(crate) width: f32,
    /// Viewport.
    pub(crate) height: f32,
    /// The simulated safe area insets.
    pub(crate) safe_area: Insets,
}

/// Parses `--safe-area <top,right,bottom,left>` (one value: all four).
pub(crate) fn parse_insets(value: &str) -> Result<Insets, String> {
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

/// Runs the app headless, taps views and prints every mount batch.
pub(crate) fn trace(entry: &Path, options: TraceOptions) -> anyhow::Result<()> {
    let taps = options
        .tap
        .iter()
        .map(|s| Selector::parse(s))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let app = if crate::rust_app::is_root(entry) {
        // A Rust app: its command line was built with the root.
        let root = crate::root().context("this nsh was not built for a Rust app's root")?;
        ui::done(format!("mounting the Rust app {}", ui::path(entry)));
        nsh_engine::App::Rust(root)
    } else {
        let bundle = nsh_bundler::bundle(&BundleOptions {
            entry: entry.to_path_buf(),
            minify: false,
            split: false,
            ui_sets: crate::described_sets().to_vec(),
        })
        .map_err(|e| BuildError::from(e).into_error(entry))?;
        // Exact numbers: traces are compared.
        ui::done(format!(
            "bundled {} {}",
            ui::path(entry),
            ui::dim(format!(
                "({} B in {:.1} ms)",
                bundle.code.len(),
                bundle.elapsed_ms
            ))
        ));
        let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, crate::described_sets())?;
        // SAFETY: compiled just now by this runtime.
        let bytecode = unsafe { Bundle::bytecode(bytecode) };
        nsh_engine::App::Js(match bundle.source_map {
            Some(map) => bytecode.with_source_map(map),
            None => bytecode,
        })
    };

    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app,
        viewport: Size::new(options.width, options.height),
        trace: true,
        network: Some(std::sync::Arc::new(crate::net::UreqNetwork::new())),
        assets: Some(std::sync::Arc::new(crate::net::DirectoryAssets(
            entry.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ))),
        environment: Environment {
            safe_area: options.safe_area,
            ..Default::default()
        },
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: crate::ui_sets().to_vec(),
        runtime: None,
        data_dir: None,
        storage: None,
    };
    let control = Arc::new(OnceLock::new());
    let sink = ChannelSink {
        batches: tx,
        control: control.clone(),
    };
    let engine = Engine::start(config, || ApproxTextEngine, sink)?;
    let _ = control.set(engine.control());

    let mut views = Views::default();
    let first = next(&rx).context("the app rendered nothing")?;
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
    let settle = |views: &mut Views, title: &str, quiet: Duration| match options.wait {
        Some(ms) => {
            views.run(&rx, title, None, Duration::from_millis(ms));
        }
        None => {
            if !views.run(&rx, title, Some(quiet), MAX_SETTLE) {
                ui::warn(format!(
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
    );

    let steps: Vec<Option<&Selector>> = if taps.is_empty() {
        (0..options.taps).map(|_| None).collect()
    } else {
        taps.iter().map(Some).collect()
    };
    for (i, selector) in steps.iter().enumerate() {
        let n = i + 1;
        let (id, title) = match selector {
            Some(selector) => match views.find(selector) {
                Some(id) => (id, format!("tap #{n} ({selector}) on view {id}")),
                None => bail!("tap #{n}: no pressable view matches `{selector}`"),
            },
            None => match views.first_pressable() {
                Some(id) => (id, format!("tap #{n} on view {id}")),
                None => {
                    ui::warn("no pressable view found");
                    break;
                }
            },
        };
        engine
            .control()
            .dispatch_event(id, EventKind::Press, nsh_core::EventPayload::None);
        match next(&rx) {
            Some(batch) => views.show(&title, &batch),
            None => bail!("tap #{n} produced no update"),
        }
        settle(&mut views, &format!("after tap #{n}"), TAP_QUIET);
    }
    if options.duration > 0 {
        views.run(
            &rx,
            "while running",
            None,
            Duration::from_millis(options.duration),
        );
    }
    Ok(())
}

fn next(rx: &Receiver<MountBatch>) -> Option<MountBatch> {
    rx.recv_timeout(Duration::from_secs(5)).ok()
}

/// What `--tap` names: `id:<testID>`, `label:<accessibilityLabel>`,
/// `text:<text>`, `view:<node id>`, or a bare string, matched as each of the
/// first three in turn.
#[derive(Debug, PartialEq)]
enum Selector {
    TestId(String),
    Label(String),
    Text(String),
    View(NodeId),
    Any(String),
}

impl Selector {
    fn parse(s: &str) -> anyhow::Result<Self> {
        Ok(match s.split_once(':') {
            Some(("id", v)) => Self::TestId(v.into()),
            Some(("label", v)) => Self::Label(v.into()),
            Some(("text", v)) => Self::Text(v.into()),
            Some(("view", v)) => Self::View(
                v.parse()
                    .with_context(|| format!("`--tap {s}`: not a view id"))?,
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

/// The native view tree as the mount items built it, for finding what to
/// tap.
#[derive(Default)]
struct Views {
    nodes: HashMap<NodeId, View>,
    /// Children of each view, bottom to top.
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
    /// Prints a batch and applies it.
    fn show(&mut self, title: &str, batch: &MountBatch) {
        print_batch(title, batch);
        for item in &batch.items {
            self.apply(item);
        }
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

    fn apply(&mut self, item: &MountItem) {
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

fn print_batch(title: &str, batch: &MountBatch) {
    println!("── {title}: {} items", batch.items.len());
    for item in &batch.items {
        match item {
            MountItem::Text {
                id, text, content, ..
            } => println!(
                "   Text {{ id: {id}, text: {:?}, content: {}x{} @ {},{} }}",
                text.text, content.width, content.height, content.x, content.y
            ),
            MountItem::Frame { id, frame } => println!(
                "   Frame {{ id: {id}, x: {}, y: {}, w: {}, h: {} }}",
                frame.x, frame.y, frame.width, frame.height
            ),
            other => println!("   {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn insets_parse() {
        let i = parse_insets("47,0,34,0").unwrap();
        assert_eq!((i.top, i.right, i.bottom, i.left), (47.0, 0.0, 34.0, 0.0));
        assert_eq!(parse_insets("10").unwrap().left, 10.0);
        assert!(parse_insets("1,2").is_err());
    }
}
