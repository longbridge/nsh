//! Rust classes (`#[class]`) through an engine: an app's set registers
//! them, its bundle imports them; `async` members run as tasks of the core,
//! their instances crossing inside the task (converted in the JS call).

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

    /// Posts "task" to `heard` from a task, after `ms` milliseconds.
    pub(crate) async fn post_later(heard: Callback<String>, ms: u32) {
        timer(Duration::from_millis(ms.into())).await;
        heard.post("task".to_owned());
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

/// Takes a `u32` in every kind of call.
pub(crate) struct Ints;

#[module]
impl Ints {
    /// In the call.
    pub(crate) fn now(n: u32) -> u32 {
        n
    }

    /// As an op.
    pub(crate) fn op(n: u32, cx: &mut ModuleCx) -> u32 {
        let _ = cx;
        n
    }

    /// As a task.
    pub(crate) async fn later(n: u32) -> u32 {
        n
    }
}

/// A `u32` kept in Rust.
#[class]
pub(crate) struct Dial {
    at: u32,
}

#[class]
impl Dial {
    pub(crate) fn new() -> Self {
        Dial { at: 0 }
    }

    /// Sets it, in the call.
    pub(crate) fn set(&mut self, at: u32) -> u32 {
        self.at = at;
        at
    }

    /// Sets it, in a task.
    pub(crate) async fn set_later(this: Instance<Self>, at: u32) -> u32 {
        this.with_mut(|d| d.at = at);
        at
    }
}

/// Shows a `u32` prop.
#[derive(Default)]
pub(crate) struct Gauge;

#[derive(Clone, PartialEq, Props)]
pub(crate) struct GaugeProps {
    #[prop(default)]
    level: nsh_core::ui::Prop<u32>,
}

impl Component for Gauge {
    type Props = GaugeProps;
    type Event = NoEvent;

    fn view(props: GaugeProps, _: Scope<NoEvent>) -> impl IntoElement {
        text(move || format!("level {}", props.level.get()))
    }
}

/// A value whose conversion from JS panics.
#[derive(Clone, PartialEq, Default)]
pub(crate) struct Panicky;

impl JsValue for Panicky {
    fn ts_type(_types: &mut nsh_core::ui::TsTypes) -> String {
        "number".into()
    }

    fn from_js(_value: nsh_core::ui::JsIn<'_>) -> Result<Self, nsh_core::ui::PropError> {
        panic!("panicky");
    }

    fn to_js<'a>(
        &self,
        host: &'a dyn nsh_core::ui::JsHost,
    ) -> Result<nsh_core::ui::JsOwned<'a>, nsh_core::ui::PropError> {
        Ok(nsh_core::ui::JsOwned::new(host, host.undefined()))
    }
}

/// Takes a prop whose conversion panics.
#[derive(Default)]
pub(crate) struct Fragile;

#[derive(Clone, PartialEq, Props)]
pub(crate) struct FragileProps {
    #[prop(default)]
    v: Panicky,
}

impl Component for Fragile {
    type Props = FragileProps;
    type Event = NoEvent;

    fn view(_: FragileProps, _: Scope<NoEvent>) -> impl IntoElement {
        text("fragile")
    }
}

/// A class no set registers by name: a module's function returns it.
#[class]
pub(crate) struct Lonely;

#[class]
impl Lonely {
    /// Its twin, another class registered by nobody.
    pub(crate) fn twin(&self) -> Instance<Twin> {
        Instance::new(Twin)
    }
}

/// Reached through `Lonely` only.
#[class]
pub(crate) struct Twin;

#[class]
impl Twin {
    /// Its name.
    #[getter]
    pub(crate) fn tag(&self) -> String {
        "twin".to_owned()
    }
}

/// Makes `Lonely`s.
pub(crate) struct Maker;

#[module]
impl Maker {
    /// A new one.
    pub(crate) fn make() -> Instance<Lonely> {
        Instance::new(Lonely)
    }
}

/// Applies a style prop to its inner view.
#[derive(Default)]
pub(crate) struct Framed;

#[derive(Clone, PartialEq, Props)]
pub(crate) struct FramedProps {
    #[prop(default)]
    inner: nsh_core::ui::Prop<nsh_core::ui::Style>,
}

impl Component for Framed {
    type Props = FramedProps;
    type Event = NoEvent;

    fn view(props: FramedProps, _: Scope<NoEvent>) -> impl IntoElement {
        view().child(view().inline_style(props.inner).child(text("framed")))
    }
}

fn registry() -> Registry {
    let mut r = Registry::ui_set("test/classes");
    r.class::<Counter>()
        .class::<Dial>()
        .module::<Tools>()
        .module::<Ints>()
        .register::<Gauge>()
        .register::<Fragile>()
        .register::<Framed>();
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
        storage: None,
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

const ORDER: &str = r#"import { mount, signal, Text } from "nsh";
import { Tools } from "test/classes";

const shown = signal("waiting");
const heard = [];
const hear = (w) => heard.push(w);
// The task posts at 40 ms, while the timer at 30 ms is still busy after
// posting in a synchronous call: the call posted first runs first.
Tools.postLater(hear, 40);
setTimeout(() => {
  Tools.postSync(hear);
  const end = Date.now() + 40;
  while (Date.now() < end) {}
}, 30);
setTimeout(() => shown.set(`done ${heard.join(",")}`), 200);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn a_post_in_a_synchronous_call_keeps_its_place_before_later_ones() {
    let (_engine, rx) = start(ORDER, vec![registry()]);
    assert_eq!(shown(&rx, "done"), "done sync,task");
}

const INTEGERS: &str = r#"import { mount, signal, Text } from "nsh";
import { Dial, Gauge, Ints } from "test/classes";

const shown = signal("waiting");
const level = signal(1);
const msg = async (f) => {
  try { return String(await f()); } catch (e) { return `${e.name}: ${e.message}`; }
};
setTimeout(async () => {
  const lines = [];
  const dial = new Dial();
  for (const v of [4294967295, 1.5, 4294967296, -1, NaN]) {
    const kinds = [
      await msg(() => Ints.now(v)),
      await msg(() => Ints.op(v)),
      await msg(() => Ints.later(v)),
      await msg(() => dial.set(v)),
      await msg(() => dial.setLater(v)),
      // A prop warns instead (as a host element's), keeping its value.
      await msg(() => { level.set(v); return "set"; }),
    ];
    lines.push(kinds.join(" / "));
  }
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => (
  <>
    <Gauge level={level()} />
    <Text>{shown()}</Text>
  </>
));
"#;

#[test]
fn integers_take_the_same_values_in_every_kind_of_call_and_prop() {
    let (_engine, rx) = start(INTEGERS, vec![registry()]);
    // (The gauge's text may come after it in a batch.)
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut done = String::new();
    while done.is_empty()
        && let Ok(batch) = rx.recv_deadline(deadline)
    {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item
                && text.text.starts_with("done")
            {
                done = text.text.clone();
            }
        }
    }
    let rows: Vec<&str> = done.trim_start_matches("done ").split('|').collect();
    let max = u32::MAX;
    assert_eq!(
        rows[0],
        format!("{max} / {max} / {max} / {max} / {max} / set")
    );
    for (row, v) in rows[1..].iter().zip(["1.5", "4294967296", "-1", "NaN"]) {
        let refused =
            |what: &str| format!("TypeError: {what}: expected an integer that fits u32, got {v}");
        assert_eq!(
            *row,
            [
                refused("Ints.now: `n`"),
                refused("Ints.op: `n`"),
                refused("Ints.later: `n`"),
                refused("Dial.set: `at`"),
                refused("Dial.setLater: `at`"),
                "set".to_owned(),
            ]
            .join(" / ")
        );
    }
    assert_eq!(rows.len(), 5, "{done}");
}

const FRAGILE: &str = r#"import { mount, signal, Text } from "nsh";
import { Fragile } from "test/classes";

const shown = signal("waiting");
const v = signal(undefined);
setTimeout(() => {
  try { v.set(2); shown.set("done set"); } catch (e) { shown.set(`done ${e.name}: ${e.message}`); }
}, 0);

mount(() => (
  <>
    <Text>{shown()}</Text>
    <Fragile v={v()} />
  </>
));
"#;

#[test]
fn a_prop_whose_conversion_panics_throws_where_it_is_set() {
    let (_engine, rx) = start(FRAGILE, vec![registry()]);
    assert_eq!(shown(&rx, "done"), "done Error: Fragile.v: panicky");
}

const LONELY: &str = r#"import { mount, signal, Text } from "nsh";
import { Lonely, Maker, Twin } from "test/maker";

const l = Maker.make();
const t = l.twin();
mount(() => <Text>{`done ${l instanceof Lonely} ${t instanceof Twin} ${t.tag}`}</Text>);
"#;

#[test]
fn classes_a_signature_names_are_exported_with_it() {
    let mut set = Registry::ui_set("test/maker");
    set.module::<Maker>();
    let (_engine, rx) = start(LONELY, vec![set]);
    assert_eq!(shown(&rx, "done"), "done true true twin");
}

const FRAMED: &str = r#"import { mount } from "nsh";
import { createNativeValue } from "nsh/animated";
import { Framed } from "test/classes";

const v = createNativeValue(0.25);
mount(() => (
  <Framed inner={[{ transform: [{ translateX: 7 }, { scale: 2 }] }, { shadowOffset: { width: 1, height: 2 }, opacity: v }]} />
));
"#;

#[test]
fn a_style_prop_takes_what_a_host_elements_style_does() {
    let (_engine, rx) = start(FRAMED, vec![registry()]);
    let (_, items) = mounted(&rx, 500);
    // The transforms and the shadow offset (objects and arrays), and the
    // native value bound to the inner view.
    assert!(
        items.contains(
            "transform: [TransformOp { kind: TranslateX, value: 7.0 }, TransformOp { kind: Scale, value: 2.0 }]"
        ),
        "{items}"
    );
    assert!(items.contains("offset_x: 1.0, offset_y: 2.0"), "{items}");
    assert!(items.contains("bindings: [(Opacity, "), "{items}");
}

const DATA: &str = r#"import { mount, signal, Text } from "nsh";
import { Counter } from "test/classes";
import { Counter as OtherCounter } from "test/other";

const shown = signal("waiting");
const msg = async (p) => { try { return String(await p); } catch (e) { return e.message; } };
setTimeout(async () => {
  const lines = [];
  // Data is data, whatever it looks like; only an instance is one.
  lines.push(JSON.stringify(await Counter.echo({ $instance: 1, h: 2 })));
  const target = new Counter(1), item = new Counter(5);
  const forged = [];
  for (let i = 1; i < 40; i++) forged.push(msg(target.absorb({ $instance: i })));
  const real = target.absorb(item);
  const results = await Promise.all(forged);
  const refused = "Counter.absorb: `other`: expected a Counter, got an object";
  lines.push(`${results.every((r) => r === refused)} ${await real}`);
  // Two sets' classes of one name are called apart.
  lines.push(`${await Counter.set()} ${await OtherCounter.set()}`);
  shown.set(`done ${lines.join("|")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn data_is_data_and_classes_are_their_sets() {
    let (_engine, rx) = start(DATA, vec![registry(), other_registry()]);
    assert_eq!(
        shown(&rx, "done"),
        r#"done {"$instance":1,"h":2}|true 6|test/classes test/other"#
    );
}

/// The texts shown and the mount items, for `ms` milliseconds.
fn mounted(rx: &Receiver<MountBatch>, ms: u64) -> (Vec<String>, String) {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    let (mut texts, mut items) = (Vec::new(), String::new());
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item {
                texts.push(text.text.clone());
            }
            items.push_str(&format!("{item:?}\n"));
        }
    }
    (texts, items)
}

const FIRST: &str = r#"import { mount, signal, Text } from "nsh";
import { Gauge } from "test/classes";

const level = signal(1.5);
setTimeout(() => level.set(3), 50);
mount(() => (
  <>
    <Text>before</Text>
    <Gauge level={level()} testID="gauge" />
    <Text>after</Text>
  </>
));
"#;

#[test]
fn a_prop_that_fails_at_the_first_render_keeps_its_default_and_the_app_mounts() {
    let (_engine, rx) = start(FIRST, vec![registry()]);
    let (texts, items) = mounted(&rx, 500);
    // A warning, as a host element's bad value: everything mounts, the
    // prop at its default, the others set; a valid value later applies.
    assert_eq!(texts, ["before", "level 0", "after", "level 3"]);
    assert!(items.contains("gauge"), "{items}");
}

const LATER: &str = r#"import { mount, signal, Text } from "nsh";
import { Gauge } from "test/classes";

const level = signal(1);
const id = signal("zzb");
const shown = signal("waiting");
setTimeout(() => {
  try { level.set(-1); shown.set("not thrown"); } catch (e) { shown.set(`thrown ${e.name}`); }
  setTimeout(() => id.set("zzc"), 50);
}, 50);

mount(() => (
  <>
    <Text>{shown()}</Text>
    <Gauge level={level()} testID={id()} />
  </>
));
"#;

#[test]
fn a_prop_that_fails_later_keeps_its_value_and_holds_back_no_other_prop() {
    let (_engine, rx) = start(LATER, vec![registry()]);
    let (texts, items) = mounted(&rx, 800);
    assert!(texts.iter().any(|t| t == "not thrown"), "{texts:?}");
    // The props after it still reach the view, right away.
    assert!(items.contains("zzb") && items.contains("zzc"), "{items}");
    assert!(texts.iter().any(|t| t == "level 1"), "{texts:?}");
}
