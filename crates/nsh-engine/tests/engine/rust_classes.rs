//! Rust classes (`#[class]`) through an engine: an app's set registers
//! them, its bundle imports them; `async` members run as tasks of the core,
//! their instances crossing in the table the engine shares between the
//! script and the core.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, MountBatch, MountItem, Size,
    ui::{Registry, prelude::*},
};
use nsh_engine::{Bundle, Engine, EngineConfig, Environment};

use crate::ChannelSink;

/// A count kept in Rust.
#[class]
pub(crate) struct Counter {
    count: i64,
}

#[class]
impl Counter {
    /// Starts at `start`.
    pub(crate) fn new(start: i64) -> Self {
        Counter { count: start }
    }

    /// The count.
    #[getter]
    pub(crate) fn count(&self) -> i64 {
        self.count
    }

    /// Adds `by` after `ms` milliseconds; the new count.
    pub(crate) async fn add_later(this: Instance<Self>, by: i64, ms: u32) -> i64 {
        timer(Duration::from_millis(ms.into())).await;
        this.with_mut(|c| {
            c.count += by;
            c.count
        })
    }

    /// Fails.
    pub(crate) async fn fail(this: Instance<Self>) -> Result<(), String> {
        Err(format!("no, at {}", this.with(|c| c.count)))
    }

    /// Panics.
    pub(crate) async fn explode(this: Instance<Self>) {
        let _ = this;
        panic!("boom");
    }

    /// A counter at twice this one's count.
    pub(crate) async fn doubled(this: Instance<Self>) -> Counter {
        Counter {
            count: this.with(|c| c.count) * 2,
        }
    }

    /// Adds `other`'s count; the new count.
    pub(crate) async fn absorb(this: Instance<Self>, other: Instance<Counter>) -> i64 {
        let by = other.with(|o| o.count);
        this.with_mut(|c| {
            c.count += by;
            c.count
        })
    }

    /// Reports progress to `progress` (a post, then a call awaited);
    /// resolves with its last answer.
    pub(crate) async fn fetch(
        this: Instance<Self>,
        progress: Callback<f64, String>,
    ) -> Result<String, String> {
        let _ = this;
        progress.post(0.5);
        progress.call_async(1.0).await
    }

    /// Calls `f` on the spot from a task (which panics: JS is not
    /// running there).
    pub(crate) async fn misuse(this: Instance<Self>, f: Callback) -> Result<(), String> {
        let _ = this;
        f.call(())
    }

    /// A counter at `start`, later.
    pub(crate) async fn make(start: i64) -> Instance<Counter> {
        timer(Duration::from_millis(1)).await;
        Instance::new(Counter { count: start })
    }

    /// `value` back, later.
    pub(crate) async fn echo(value: nsh_core::JsonValue) -> nsh_core::JsonValue {
        value
    }

    /// Which set's class this is.
    pub(crate) async fn set() -> String {
        "test/classes".to_owned()
    }
}

/// Another set's class of the same name.
mod other {
    use nsh_core::ui::prelude::*;

    #[class]
    pub(crate) struct Counter;

    #[class]
    impl Counter {
        /// Which set's class this is.
        pub(crate) async fn set() -> String {
            "test/other".to_owned()
        }
    }
}

/// Functions of the three kinds.
pub(crate) struct Tools;

#[module]
impl Tools {
    /// Twice `n`, at once.
    pub(crate) fn twice(n: f64) -> f64 {
        n * 2.0
    }

    /// `text` back, as an op (a promise).
    pub(crate) fn echo(text: String, cx: &mut ModuleCx) -> String {
        let _ = cx;
        text
    }

    /// What `question` answers to "name?".
    pub(crate) async fn ask(question: Callback<String, String>) -> Result<String, String> {
        question.call_async("name?".to_owned()).await
    }

    /// The count of `counter`, later.
    pub(crate) async fn count_of(counter: Instance<Counter>) -> i64 {
        timer(Duration::from_millis(1)).await;
        counter.with(|c| c.count)
    }

    /// Posts "op" to `heard` when the op is applied.
    pub(crate) fn post_op(heard: Callback<String>, cx: &mut ModuleCx) {
        let _ = cx;
        heard.post("op".to_owned());
    }

    /// Posts "sync" to `heard` in the call.
    pub(crate) fn post_sync(heard: Callback<String>) {
        heard.post("sync".to_owned());
    }

    /// An op that panics.
    pub(crate) fn explode(cx: &mut ModuleCx) -> i64 {
        let _ = cx;
        panic!("op boom");
    }

    /// An op calling `f` on the spot (which panics: JS is not running in
    /// an op).
    pub(crate) fn call_in_op(f: Callback<(), i64>, cx: &mut ModuleCx) -> Result<i64, String> {
        let _ = cx;
        f.call(())
    }
}

fn registry() -> Registry {
    let mut r = Registry::ui_set("test/classes");
    r.class::<Counter>().module::<Tools>();
    r
}

fn other_registry() -> Registry {
    let mut r = Registry::ui_set("test/other");
    r.class::<other::Counter>();
    r
}

/// The app, compiled by nsh with the classes' set and loaded as bytecode.
pub(crate) fn start(app: &str, ui_sets: Vec<Registry>) -> (Engine, Receiver<MountBatch>) {
    static APPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "nsh-classes-{}-{}",
        std::process::id(),
        APPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, app).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: ui_sets.clone(),
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &ui_sets).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        // SAFETY: compiled just now by this runtime.
        app: nsh_engine::App::Js(unsafe { Bundle::bytecode(bytecode) }),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets,
        runtime: None,
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// The last text shown once one starts with `prefix` (or after 5 s).
pub(crate) fn shown(rx: &Receiver<MountBatch>, prefix: &str) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut last = String::new();
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item {
                last = text.text.clone();
            }
        }
        if last.starts_with(prefix) {
            break;
        }
    }
    last
}

const ASYNC: &str = r#"import { mount, signal, Text } from "nsh";
import { Counter } from "test/classes";

const shown = signal("waiting");
const msg = async (p) => { try { await p; return "resolved"; } catch (e) { return e.message; } };
setTimeout(async () => {
  const lines = [];
  const c = new Counter(1);
  const added = c.addLater(2, 10);
  lines.push(String(c.count));
  lines.push(String(await added));
  lines.push(await msg(c.fail()));
  lines.push(await msg(c.explode()));
  const d = await c.doubled();
  lines.push(`${d instanceof Counter} ${d.count}`);
  lines.push(String(await c.absorb(d)));
  const made = await Counter.make(5);
  lines.push(`${made instanceof Counter} ${made.count}`);
  // The task keeps its instance after the object goes.
  lines.push(String(await new Counter(10).addLater(1, 10)));
  const gone = new Counter(1);
  const later = gone.addLater(1, 10);
  gone.dispose();
  lines.push(await msg(later));
  lines.push(await msg(Counter.prototype.addLater.call({}, 1, 1)));
  const steps = [];
  const answer = await c.fetch((p) => { steps.push(p); return `at ${p}`; });
  lines.push(`${steps.join(",")} ${answer}`);
  lines.push(await c.fetch(async (p) => { await null; return `later ${p}`; }));
  lines.push(await msg(c.fetch((p) => { if (p === 1) throw new Error("refused"); return ""; })));
  lines.push(await msg(c.fetch(async () => { throw new Error("rejected"); })));
  lines.push(await msg(c.misuse(() => {})));
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn async_members_run_as_tasks_and_resolve_with_their_results() {
    let (_engine, rx) = start(ASYNC, vec![registry()]);
    assert_eq!(
        shown(&rx, "done"),
        "done 1|3|no, at 3|boom|true 6|9|true 5|11|Counter is disposed|\
         Counter.addLater: Illegal invocation: not a Counter|0.5,1 at 1|later 1|refused|rejected|\
         `Callback::call` runs the function inside a synchronous call from JS; elsewhere (in a \
         task) use `post` or `call_async`"
    );
}

const FUNCTIONS: &str = r#"import { mount, signal, Text } from "nsh";
import { Counter, Tools } from "test/classes";

const shown = signal("waiting");
setTimeout(async () => {
  const lines = [];
  lines.push(String(Tools.twice(21)));
  lines.push(await Tools.echo("op"));
  lines.push(await Tools.ask((q) => `answer to ${q}`));
  lines.push(String(await Tools.countOf(new Counter(7))));
  // Posts arrive in program order: the op's before the call's after it.
  const heard = [];
  Tools.postOp((w) => heard.push(w));
  Tools.postSync((w) => heard.push(w));
  await new Promise((r) => setTimeout(r, 20));
  lines.push(heard.join(","));
  // A panic in an op rejects it; the engine goes on.
  const msg = async (p) => { try { return String(await p); } catch (e) { return e.message; } };
  lines.push(await msg(Tools.explode()));
  lines.push((await msg(Tools.callInOp(() => 1))).split(";")[0]);
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn functions_run_in_the_call_as_ops_or_as_tasks() {
    let (_engine, rx) = start(FUNCTIONS, vec![registry()]);
    assert_eq!(
        shown(&rx, "done"),
        "done 42|op|answer to name?|7|op,sync|op boom|\
         `Callback::call` runs the function inside a synchronous call from JS"
    );
}

const DATA: &str = r#"import { mount, signal, Text } from "nsh";
import { Counter } from "test/classes";
import { Counter as OtherCounter } from "test/other";

const shown = signal("waiting");
const msg = async (p) => { try { return String(await p); } catch (e) { return e.message; } };
setTimeout(async () => {
  const lines = [];
  // Data that looks like the runtime's ids is data.
  lines.push(JSON.stringify(await Counter.echo({ $instance: 1, h: 2 })));
  const target = new Counter(1), item = new Counter(5);
  const forged = [];
  for (let i = 1; i < 40; i++) forged.push(msg(target.absorb({ $instance: i })));
  const real = target.absorb(item);
  const results = await Promise.all(forged);
  lines.push(`${results.every((r) => r.startsWith("`other`: expected a Counter"))} ${await real}`);
  // Two sets' classes of one name are called apart.
  lines.push(`${await Counter.set()} ${await OtherCounter.set()}`);
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn held_ids_are_apart_from_the_data_and_classes_from_their_sets() {
    let (_engine, rx) = start(DATA, vec![registry(), other_registry()]);
    assert_eq!(
        shown(&rx, "done"),
        r#"done {"$instance":1,"h":2}|true 6|test/classes test/other"#
    );
}
