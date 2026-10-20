//! Benchmark of Rust components against the same component written in TSX
//! (`examples/bench`): the first frame of 200 Buttons, one update of all their
//! titles, and scrolling a recycled list with a Button per row.
//!
//! Run with `cargo test --release -p nsh-engine --test engine bench_components -- --ignored --nocapture`;
//! the numbers are in `docs/dev/performance.md`. `profile_components` loops
//! the Rust Grid for a profiler (`samply record <test binary>
//! component_bench::profile_components --ignored --exact`).

use std::time::{Duration, Instant};

use nsh_core::{
    ApproxTextEngine, Core, EventKind, EventPayload, ListBind, MountItem, Size, ViewKind,
};
use nsh_js::Host;

struct App {
    host: Host,
    core: Core<ApproxTextEngine>,
    ops: Vec<nsh_core::Op>,
    binds: Vec<ListBind>,
    /// JS time and core time of the last `turn`.
    js: Duration,
    native: Duration,
    ops_applied: usize,
    items: usize,
}

fn bytecode(entry: &str) -> Vec<u8> {
    let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/bench")
        .join(entry);
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: vec![nsh_ui_mobile::registry().into()],
    })
    .unwrap();
    nsh_js::compile_bytecode_with(&bundle.code, true, &[nsh_ui_mobile::registry().into()]).unwrap()
}

impl App {
    fn new() -> Self {
        let host = Host::with_options(nsh_js::HostOptions {
            ui_sets: vec![nsh_ui_mobile::registry()],
            ..Default::default()
        })
        .unwrap();
        // The runtime reads the environment while it loads.
        host.set_environment(&nsh_js::Environment {
            window: (390.0, 844.0),
            ..Default::default()
        });
        let mut core = Core::new(ApproxTextEngine, Size::new(390.0, 844.0));
        core.set_components(nsh_ui_mobile::registry());
        Self {
            host,
            core,
            ops: Vec::new(),
            binds: Vec::new(),
            js: Duration::ZERO,
            native: Duration::ZERO,
            ops_applied: 0,
            items: 0,
        }
    }

    /// Runs `js`, then applies its ops and commits, with bind rounds like the
    /// engine's frame. Returns the mount items of the turn.
    fn turn(&mut self, js: impl FnOnce(&Host)) -> Vec<MountItem> {
        let t = Instant::now();
        js(&self.host);
        self.host.run_jobs();
        self.js = t.elapsed();
        self.native = Duration::ZERO;
        self.ops_applied = 0;
        let mut items = Vec::new();
        for _ in 0..8 {
            let t = Instant::now();
            self.host.take_ops(&mut self.ops);
            self.ops_applied += self.ops.len();
            for op in self.ops.drain(..) {
                self.core.apply(op).unwrap();
            }
            if let Some(batch) = self.core.commit() {
                items.extend(batch.items);
            }
            self.core.take_list_binds(&mut self.binds);
            self.native += t.elapsed();
            if self.binds.is_empty() {
                break;
            }
            let t = Instant::now();
            for b in self.binds.drain(..) {
                self.host.bind_list(b.list, &b.binds);
            }
            self.js += t.elapsed();
        }
        self.items = items.len();
        items
    }
}

/// `min / median`: the minimum is the code path's cost, the median shows
/// how noisy the machine was.
fn stats(mut v: Vec<Duration>) -> String {
    v.sort();
    format!("{:?} / {:?}", v[0], v[v.len() / 2])
}

fn first_frame(entry: &str) -> (App, Vec<MountItem>) {
    let code = bytecode(entry);
    let mut runs = Vec::new();
    let mut last = None;
    for _ in 0..40 {
        let mut app = App::new();
        let items = app.turn(|h| h.eval_bytecode(&code).unwrap());
        runs.push((app.js, app.native));
        last = Some((app, items));
    }
    let (app, items) = last.unwrap();
    println!(
        "  first frame: js {}, core {} ({} ops, {} mount items)",
        stats(runs.iter().map(|r| r.0).collect()),
        stats(runs.iter().map(|r| r.1).collect()),
        app.ops_applied,
        app.items,
    );
    (app, items)
}

fn grid(entry: &str) {
    println!("{entry}");
    let (mut app, items) = first_frame(entry);
    // The first press target is the "Bump" view: it updates all 200 titles.
    let bump = items
        .iter()
        .find_map(|i| match i {
            MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
            _ => None,
        })
        .unwrap();
    let (mut js, mut native) = (Vec::new(), Vec::new());
    for _ in 0..300 {
        app.turn(|h| h.dispatch_event(bump, EventKind::Press, &EventPayload::None));
        js.push(app.js);
        native.push(app.native);
    }
    println!(
        "  update 200 titles: js {}, core {} ({} ops, {} mount items)",
        stats(js),
        stats(native),
        app.ops_applied,
        app.items
    );
}

fn rows(entry: &str) {
    println!("{entry}");
    let (mut app, items) = first_frame(entry);
    let list = items
        .iter()
        .find_map(|i| match i {
            MountItem::Create {
                id,
                kind: ViewKind::Scroll,
            } => Some(*id),
            _ => None,
        })
        .unwrap();
    let (mut js, mut native, mut n) = (Duration::ZERO, Duration::ZERO, 0u32);
    let mut y = 0.0;
    for _ in 0..4000 {
        y += 8.0;
        app.core.set_scroll_offset(list, 0.0, y, u32::MAX);
        app.turn(|_| {});
        if app.ops_applied > 0 {
            js += app.js;
            native += app.native;
            n += 1;
        }
    }
    println!(
        "  scrolling: {n} rebinding frames, js {:?}/frame, core {:?}/frame",
        js / n,
        native / n
    );
}

#[test]
#[ignore]
fn bench_components() {
    for entry in ["GridRust.tsx", "GridTs.tsx"] {
        grid(entry);
    }
    for entry in ["RowsRust.tsx", "RowsTs.tsx"] {
        rows(entry);
    }
}

/// For profilers: the first frame and the title updates of the Rust Grid in
/// a loop (`NSH_BENCH_ENTRY` picks another entry).
#[test]
#[ignore]
fn profile_components() {
    let entry = std::env::var("NSH_BENCH_ENTRY").unwrap_or_else(|_| "GridRust.tsx".into());
    let code = bytecode(&entry);
    for _ in 0..60 {
        let mut app = App::new();
        let items = app.turn(|h| h.eval_bytecode(&code).unwrap());
        let bump = items.iter().find_map(|i| match i {
            MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
            _ => None,
        });
        if let Some(bump) = bump {
            for _ in 0..100 {
                app.turn(|h| h.dispatch_event(bump, EventKind::Press, &EventPayload::None));
            }
        }
    }
}
