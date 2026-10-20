//! Rust components through the generated JS glue: props, events, children,
//! named slots, children placed one by one, and a list of the app's rows;
//! modules (`Alert`, `StatusBar`) and the environment (`SafeAreaView`,
//! `KeyboardAvoidingView`).

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, Command, EventKind, EventPayload, KeyboardAppearance, KeyboardType,
    MountBatch, MountItem, PayloadValue, ROOT_ID, Size, StatusBarStyle,
};
use nsh_engine::{Bundle, Engine, EngineConfig, Environment, Insets};

use crate::ChannelSink;

fn start(app: &str) -> (Engine, Receiver<MountBatch>) {
    start_in(app, Environment::default())
}

/// The app, compiled by nsh and loaded as bytecode.
fn start_in(app: &str, environment: Environment) -> (Engine, Receiver<MountBatch>) {
    start_with(app, environment, None, None)
}

/// [`start_in`] with the capabilities the app declares (JSON, `nsh.json`'s
/// `capabilities` form) and an `nsh.json`.
fn start_with(
    app: &str,
    environment: Environment,
    capabilities: Option<&str>,
    config: Option<&str>,
) -> (Engine, Receiver<MountBatch>) {
    static APPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "nsh-components-{}-{}",
        std::process::id(),
        APPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, app).unwrap();
    if let Some(config) = config {
        std::fs::write(dir.join("nsh.json"), config).unwrap();
    }
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: vec![nsh_ui_mobile::registry().into()],
    })
    .unwrap();
    let bytecode =
        nsh_js::compile_bytecode_with(&bundle.code, false, &[nsh_ui_mobile::registry().into()])
            .unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        // SAFETY: compiled just now by this runtime.
        app: nsh_engine::App::Js(
            unsafe { Bundle::bytecode(bytecode) }
                .with_capabilities_json(capabilities)
                .unwrap(),
        ),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        websocket: None,
        filesystem: None,
        assets: None,
        environment,
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: vec![nsh_ui_mobile::registry()],
        runtime: None,
        data_dir: None,
        storage: None,
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

fn text_ids(batch: &MountBatch, wanted: &str) -> Option<u32> {
    batch.items.iter().find_map(|i| match i {
        MountItem::Text { id, text, .. } if text.text == wanted => Some(*id),
        _ => None,
    })
}

const INSET: &str = r#"import { mount, signal, Text } from "nsh";
import { ListView } from "nsh/ui/mobile";

const rows = signal(["A", "B", "C"]);
setTimeout(() => rows.update((r) => [...r, "D"]), 0);

mount(() => <ListView variant="inset" each={rows()}>{(row) => <Text>{row()}</Text>}</ListView>);
"#;

#[test]
fn rows_know_whether_they_are_last() {
    let (_engine, rx) = start(INSET);
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    let parent_of = |batch: &MountBatch, id| {
        batch.items.iter().find_map(|i| match i {
            MountItem::Insert { parent, child, .. } if *child == id => Some(*parent),
            _ => None,
        })
    };
    let corners = |batch: &MountBatch, id| {
        batch.items.iter().find_map(|i| match i {
            MountItem::ViewStyle { id: v, style } if *v == id => Some(style.corner_radius),
            _ => None,
        })
    };
    let rounded = |c: [Option<f32>; 4]| c.iter().map(|r| r.is_some()).collect::<Vec<_>>();
    let wrapper = |name| parent_of(&first, text_ids(&first, name).unwrap()).unwrap();
    let (a, b, c) = (wrapper("A"), wrapper("B"), wrapper("C"));
    // The first row rounds its top corners, the last its bottom ones.
    assert_eq!(
        corners(&first, a).map(rounded),
        Some(vec![true, true, false, false])
    );
    assert_eq!(
        corners(&first, b).map(rounded),
        Some(vec![false, false, false, false])
    );
    assert_eq!(
        corners(&first, c).map(rounded),
        Some(vec![false, false, true, true])
    );

    // A row appended: C is no longer the last, D is.
    let next = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the new row");
    let d = parent_of(&next, text_ids(&next, "D").expect("D")).unwrap();
    assert_eq!(
        corners(&next, c).map(rounded),
        Some(vec![false, false, false, false])
    );
    assert_eq!(
        corners(&next, d).map(rounded),
        Some(vec![false, false, true, true])
    );
}

#[test]
fn a_stateful_component_steps_while_held() {
    a_stateful_component_steps_while_held_on(start(STEPPER));
}

fn a_stateful_component_steps_while_held_on((engine, rx): (Engine, Receiver<MountBatch>)) {
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    assert!(text_ids(&first, "5").is_some(), "the default value");
    // The + button: the second view listening to presses.
    let plus = first
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Events { id, events } if events.contains(EventKind::PressIn) => Some(*id),
            _ => None,
        })
        .nth(1)
        .expect("the + button");
    let control = engine.control();
    control.dispatch_event(plus, EventKind::PressIn, EventPayload::None);
    // One step at once, more after the hold (400ms, then every 80ms).
    std::thread::sleep(Duration::from_millis(700));
    control.dispatch_event(plus, EventKind::PressOut, EventPayload::None);
    std::thread::sleep(Duration::from_millis(200));
    let mut shown = Vec::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(100)) {
        for item in batch.items {
            if let MountItem::Text { text, .. } = item {
                shown.push(text.text.clone());
            }
        }
    }
    let value: f64 = shown
        .iter()
        .rev()
        .find_map(|t| t.parse().ok())
        .expect("the value changed");
    assert!(value >= 8.0, "held: {value}");
    let changes: u32 = shown
        .iter()
        .rev()
        .find_map(|t| t.strip_prefix("changes ")?.parse().ok())
        .unwrap();
    assert_eq!(changes as f64, value - 5.0, "one onChange per step");
    // Released: no more steps.
    assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
}

/// The batches until `done` holds for one (or nothing came for 500ms).
fn until(rx: &Receiver<MountBatch>, mut done: impl FnMut(&MountBatch) -> bool) -> Vec<MountBatch> {
    let mut batches = Vec::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(500)) {
        let stop = done(&batch);
        batches.push(batch);
        if stop {
            break;
        }
    }
    batches
}

fn shows(batch: &MountBatch, wanted: &str) -> bool {
    text_ids(batch, wanted).is_some()
}

const LAZY_SHEET: &str = r#"import { mount, signal, untracked, Text } from "nsh";
import { Sheet } from "nsh/ui/mobile";

const open = signal(false);
const count = signal(1);
// Made when the sheet opens: shows the count of that moment.
function Content() {
  const at = untracked(() => count());
  return <Text>{`opened at ${at}`}</Text>;
}
setTimeout(() => { count.set(3); open.set(true); }, 50);
setTimeout(() => open.set(false), 150);
setTimeout(() => { count.set(5); open.set(true); }, 250);

mount(() => <Sheet open={open()}><Content /></Sheet>);
"#;

/// A sheet's content (`#[prop(lazy)]` children) is made when it opens,
/// released when it closes, and made again on the next opening.
#[test]
fn a_sheet_makes_its_content_when_it_opens() {
    let (_engine, rx) = start(LAZY_SHEET);
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    assert!(
        !first
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Text { .. })),
        "closed: no content"
    );
    let batches = until(&rx, |b| shows(b, "opened at 3"));
    assert!(
        batches.iter().any(|b| shows(b, "opened at 3")),
        "made on opening"
    );
    let batches = until(&rx, |b| shows(b, "opened at 5"));
    assert!(
        batches.iter().any(|b| shows(b, "opened at 5")),
        "made again"
    );
}

#[test]
fn rendered_status_bars_win_over_what_functions_set() {
    rendered_status_bars_win_over_what_functions_set_on(start(STATUS_BAR));
}

fn rendered_status_bars_win_over_what_functions_set_on(
    (_engine, rx): (Engine, Receiver<MountBatch>),
) {
    let mut configs = Vec::new();
    until(&rx, |batch| {
        for item in &batch.items {
            if let MountItem::Command {
                id: ROOT_ID,
                command: Command::StatusBar(c),
            } = item
            {
                configs.push(*c);
            }
        }
        configs.len() >= 3
    });
    let red = nsh_core::Color::parse("#ff0000");
    let green = nsh_core::Color::parse("#00ff00");
    let seen: Vec<_> = configs
        .iter()
        .map(|c| (c.style, c.hidden, c.background))
        .collect();
    // One per turn: the functions' and the rendered setting merged, then
    // the hidden bar, then the functions' alone.
    assert_eq!(
        seen,
        [
            (StatusBarStyle::Light, false, green),
            (StatusBarStyle::Light, true, green),
            (StatusBarStyle::Dark, true, red),
        ]
    );
}

fn frame_of(batches: &[MountBatch], id: u32) -> Option<nsh_core::Rect> {
    batches.iter().rev().find_map(|b| {
        b.items.iter().rev().find_map(|i| match i {
            MountItem::Frame { id: f, frame } if *f == id => Some(*frame),
            _ => None,
        })
    })
}

#[test]
fn unhandled_back_presses_go_back_natively() {
    unhandled_back_presses_go_back_natively_on(start(BACK));
}

fn unhandled_back_presses_go_back_natively_on((engine, rx): (Engine, Receiver<MountBatch>)) {
    until(&rx, |b| shows(b, "presses 0"));
    let backs = |batches: &[MountBatch]| {
        batches
            .iter()
            .flat_map(|b| &b.items)
            .filter(|i| {
                matches!(
                    i,
                    MountItem::Command {
                        id: ROOT_ID,
                        command: Command::Back
                    }
                )
            })
            .count()
    };
    let control = engine.control();
    control.dispatch_event(ROOT_ID, EventKind::BackPress, EventPayload::None);
    let first = until(&rx, |b| shows(b, "presses 1"));
    assert_eq!(backs(&first), 0, "handled");
    control.dispatch_event(ROOT_ID, EventKind::BackPress, EventPayload::None);
    let second = until(&rx, |b| backs(std::slice::from_ref(b)) > 0);
    assert_eq!(
        backs(&second),
        1,
        "not handled: the platform goes back (a stack pops, or the app exits)"
    );
}

#[test]
fn a_rust_text_field_clears_after_typing() {
    a_rust_text_field_clears_after_typing_on(start(FIELD));
}

fn a_rust_text_field_clears_after_typing_on((engine, rx): (Engine, Receiver<MountBatch>)) {
    let first = until(&rx, |b| shows(b, "name Ada"));
    let input = first
        .iter()
        .flat_map(|b| &b.items)
        .find_map(|i| match i {
            MountItem::Create {
                id,
                kind: nsh_core::ViewKind::Input,
            } => Some(*id),
            _ => None,
        })
        .expect("an input");
    let control = engine.control();
    // The user types: two edits.
    for (text, count) in [("Ada G", 1), ("Ada Gr", 2)] {
        control.dispatch_event(
            input,
            EventKind::ChangeText,
            EventPayload::Text {
                text: text.into(),
                count,
            },
        );
    }
    until(&rx, |b| shows(b, "name Ada Gr"));
    // The clear button: the field sends the empty text, based on the last
    // edit (else the platform drops it as stale).
    let clear = first
        .iter()
        .flat_map(|b| &b.items)
        .filter_map(|i| match i {
            MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
            _ => None,
        })
        .next_back()
        .expect("the clear button");
    control.dispatch_event(clear, EventKind::Press, EventPayload::None);
    let after = until(&rx, |b| shows(b, "name "));
    let value = after.iter().flat_map(|b| &b.items).find_map(|i| match i {
        MountItem::InputValue { id, text, count } if *id == input => Some((text.clone(), *count)),
        _ => None,
    });
    assert_eq!(value, Some((String::new(), 2)));
}

#[test]
fn a_sibling_coming_back_does_not_move_a_rust_input() {
    a_sibling_coming_back_does_not_move_a_rust_input_on(start(FIELD));
}

fn a_sibling_coming_back_does_not_move_a_rust_input_on(
    (engine, rx): (Engine, Receiver<MountBatch>),
) {
    let first = until(&rx, |b| shows(b, "name Ada"));
    let items = || first.iter().flat_map(|b| &b.items);
    let input = items()
        .find_map(|i| match i {
            MountItem::Create {
                id,
                kind: nsh_core::ViewKind::Input,
            } => Some(*id),
            _ => None,
        })
        .expect("an input");
    let clear = items()
        .filter_map(|i| match i {
            MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
            _ => None,
        })
        .next_back()
        .expect("the clear button");
    let control = engine.control();
    control.dispatch_event(clear, EventKind::Press, EventPayload::None);
    until(&rx, |b| shows(b, "name "));
    control.dispatch_event(
        input,
        EventKind::ChangeText,
        EventPayload::Text {
            text: "x".into(),
            count: 1,
        },
    );
    let after = until(&rx, |b| shows(b, "name x"));
    let inserts: Vec<_> = after
        .iter()
        .flat_map(|b| &b.items)
        .filter_map(|i| match i {
            MountItem::Insert { child, before, .. } => Some((*child, *before)),
            _ => None,
        })
        .collect();
    assert!(
        inserts.iter().all(|&(child, _)| child != input),
        "{inserts:?}"
    );
    // The new clear button goes after the input, at the end.
    assert!(
        inserts.iter().any(|&(_, before)| before.is_none()),
        "{inserts:?}"
    );
}

// --- Through the native modules ------------------------------------------------

const CARD: &str = r#"import { mount, signal, Text } from "nsh";
import { Button, Card } from "nsh/ui/mobile";

function App() {
  const titled = signal(true);
  return (
    <Card
      header={<If when={titled()}><Text>Title</Text></If>}
      footer={<Button title="Hide title" onPress={() => titled.set(false)} />}
    >
      <Text>Body</Text>
    </Card>
  );
}

mount(App);
"#;

#[test]
fn slots_children_and_events() {
    let (engine, rx) = start(CARD);
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    for wanted in ["Title", "Body", "Hide title"] {
        assert!(text_ids(&first, wanted).is_some(), "{wanted} rendered");
    }
    let parent_of = |id| {
        first.items.iter().find_map(|i| match i {
            MountItem::Insert { parent, child, .. } if *child == id => Some(*parent),
            _ => None,
        })
    };
    let frame = |id| {
        first.items.iter().find_map(|i| match i {
            MountItem::Frame { id: f, frame } if *f == id => Some(*frame),
            _ => None,
        })
    };
    let title_parent = parent_of(text_ids(&first, "Title").unwrap()).unwrap();
    let body_parent = parent_of(text_ids(&first, "Body").unwrap()).unwrap();
    assert!(frame(title_parent).unwrap().y < frame(body_parent).unwrap().y);
    let button = first
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
            _ => None,
        })
        .expect("a pressable button");
    engine
        .control()
        .dispatch_event(button, EventKind::Press, EventPayload::None);
    let batch = rx.recv_timeout(Duration::from_secs(5)).expect("a batch");
    assert!(
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Remove { id } if *id == title_parent)),
        "{:?}",
        batch.items
    );
}

const METHODS: &str = r#"import { mount, signal, Text, View } from "nsh";
import { Stepper, type StepperRef } from "nsh/ui/mobile";

const shown = signal("waiting");
let stepper!: StepperRef;
setTimeout(async () => {
  let refused = "";
  // A method returning nothing is a plain call: `undefined`, and a wrong
  // argument throws in the call.
  try { (stepper.setValue as any)("x"); refused = "no throw"; } catch (e) { refused = `${e.name}: ${e.message}`; }
  const set = stepper.setValue(42.4);
  if (set !== undefined) refused = `returned ${set}`;
  // One returning a value is a promise, settled after the call before it ran.
  const value = await stepper.value();
  shown.set(`got ${value} | ${refused}`);
}, 0);

mount(() => (
  <View>
    <Stepper ref={stepper} defaultValue={1} max={50} />
    <Text>{shown()}</Text>
  </View>
));
"#;

#[test]
fn calls_of_a_components_methods() {
    let (_engine, rx) = start(METHODS);
    let mut shown = Vec::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(500)) {
        for item in batch.items {
            if let MountItem::Text { text, .. } = item {
                shown.push(text.text.clone());
            }
        }
        if shown.iter().any(|t| t.starts_with("got")) {
            break;
        }
    }
    assert!(shown.contains(&"42".to_owned()), "{shown:?}");
    // A wrong argument's error names the component.
    assert!(
        shown.contains(
            &"got 42 | TypeError: Stepper.setValue: `value`: expected a number, got \"x\""
                .to_owned()
        ),
        "{shown:?}"
    );
}

const ALERT: &str = r#"import { mount, signal, Text } from "nsh";
import { Alert } from "nsh/ui/mobile";

const shown = signal("waiting");
setTimeout(async () => {
  const i = await Alert.alert("Delete the photo?", "It is gone then.", [
    { text: "Cancel", style: "cancel" },
    { text: "Delete", style: "destructive" },
  ], { cancelable: true });
  shown.set(`answer ${i}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn alert_resolves_with_the_button_pressed() {
    let (engine, rx) = start(ALERT);
    let mut dialog = None;
    until(&rx, |batch| {
        dialog = dialog.take().or_else(|| {
            batch.items.iter().find_map(|i| match i {
                MountItem::Command {
                    id: ROOT_ID,
                    command: Command::Dialog(d),
                } => Some(d.clone()),
                _ => None,
            })
        });
        dialog.is_some()
    });
    let dialog = dialog.expect("the alert");
    assert_eq!(dialog.title.as_deref(), Some("Delete the photo?"));
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::Dialog,
        EventPayload::Map(vec![
            ("id", PayloadValue::Number(dialog.id as f64)),
            ("index", PayloadValue::Number(1.0)),
        ]),
    );
    let batches = until(&rx, |b| shows(b, "answer 1"));
    assert!(batches.iter().any(|b| shows(b, "answer 1")));
}

const SAFE_AREA: &str = r#"import { mount, Text } from "nsh";
import { KeyboardAvoidingView, SafeAreaView, safeArea } from "nsh/ui/mobile";

mount(() => (
  <KeyboardAvoidingView style={{ flex: 1 }}>
    <SafeAreaView style={{ flex: 1, padding: 4 }} edges={["top", "bottom"]}>
      <Text>inside</Text>
      <Text>{`top ${safeArea().top}`}</Text>
    </SafeAreaView>
  </KeyboardAvoidingView>
));
"#;

#[test]
fn safe_area_and_keyboard_avoiding_views() {
    let environment = Environment {
        window: (390.0, 844.0),
        safe_area: Insets {
            top: 47.0,
            right: 10.0,
            bottom: 34.0,
            left: 10.0,
        },
        ..Default::default()
    };
    let (engine, rx) = start_in(SAFE_AREA, environment);
    let first = until(&rx, |b| shows(b, "inside"));
    let text = first
        .iter()
        .find_map(|b| text_ids(b, "inside"))
        .expect("the text");
    let frame = frame_of(&first, text).expect("laid out");
    assert_eq!((frame.x, frame.y), (4.0, 51.0));
    assert!(
        first.iter().any(|b| shows(b, "top 47")),
        "the environment's accessor"
    );
    engine.control().set_environment(Environment {
        keyboard_height: 300.0,
        safe_area: Insets {
            top: 20.0,
            ..environment.safe_area
        },
        ..environment
    });
    let after = until(&rx, |b| shows(b, "top 20"));
    assert!(
        after.iter().any(|b| shows(b, "top 20")),
        "the accessor follows"
    );
    assert!(
        after
            .iter()
            .flat_map(|b| &b.items)
            .any(|i| matches!(i, MountItem::Frame { frame, .. } if frame.height == 844.0 - 300.0)),
        "the safe area view above the keyboard"
    );
}

const RUNTIME_MODULES: &str = r#"import { mount, signal, Text } from "nsh";
import { AccessibilityInfo } from "nsh/accessibility";
import { Platform } from "nsh/device";
import { Toast } from "nsh/android";
import { Image } from "nsh/image";

const shown = signal("waiting");
let note: unknown;
setTimeout(async () => {
  Toast.show("hello", "long");
  AccessibilityInfo.setAccessibilityFocus(note);
  const size = await Image.getSize({ uri: "https://example.com/a.png" });
  let failed = "no";
  try {
    await Image.getSize("https://example.com/missing.png");
  } catch (e) {
    failed = String((e as Error).message);
  }
  shown.set(`${size.width}x${size.height} ${failed} ${Platform.OS} ${await AccessibilityInfo.isReduceMotionEnabled()}`);
}, 0);

mount(() => <Text ref={note}>{shown()}</Text>);
"#;

#[test]
fn runtime_modules() {
    let (engine, rx) = start(RUNTIME_MODULES);
    let note = rx
        .recv_timeout(Duration::from_secs(5))
        .ok()
        .and_then(|b| text_ids(&b, "waiting"))
        .expect("first frame");
    let mut toast = None;
    let mut focused = None;
    let mut requests = Vec::new();
    let control = engine.control();
    until(&rx, |batch| {
        for item in &batch.items {
            match item {
                MountItem::Command {
                    id: ROOT_ID,
                    command: Command::Toast { text, long },
                } => toast = Some((text.clone(), *long)),
                MountItem::Command {
                    id,
                    command: Command::AccessibilityFocus,
                } => focused = Some(*id),
                MountItem::Command {
                    id: ROOT_ID,
                    command: Command::ImageInfo { id, uri },
                } => {
                    requests.push(uri.clone());
                    let payload = if requests.len() == 1 {
                        vec![
                            ("id", PayloadValue::Number(*id as f64)),
                            ("width", PayloadValue::Number(640.0)),
                            ("height", PayloadValue::Number(480.0)),
                        ]
                    } else {
                        vec![
                            ("id", PayloadValue::Number(*id as f64)),
                            ("error", PayloadValue::Str("not found".into())),
                        ]
                    };
                    control.dispatch_event(
                        ROOT_ID,
                        EventKind::ImageInfo,
                        EventPayload::Map(payload),
                    );
                }
                _ => {}
            }
        }
        shows(batch, "640x480 not found ios false")
    });
    assert_eq!(toast, Some(("hello".to_owned(), true)));
    assert_eq!(focused, Some(note));
}

const MEASURE: &str = r#"import { mount, signal, Scroll, Text, View, type YNode } from "nsh";

const shown = signal("waiting");
let box!: YNode;
let ghost!: YNode;
const detached = <View ref={(n: YNode) => (ghost = n)} />;
setTimeout(async () => {
  const m = (await box.measure())!;
  const none = await ghost.measure();
  shown.set(`at ${m.x},${m.y} ${m.width}x${m.height} page ${m.pageX},${m.pageY} ${none}`);
}, 0);

mount(() => (
  <View>
    <Text>{shown()}</Text>
    <Scroll
      style={{ marginTop: 50, height: 300 }}
      onScroll={async () => shown.set(`scrolled ${(await box.measure())!.pageY}`)}
    >
      <View style={{ padding: 20, height: 1000 }}>
        <View ref={box} style={{ marginTop: 500, height: 30 }} />
      </View>
    </Scroll>
  </View>
));
void detached;
"#;

/// `ref.measure()` answers after layout, with the frame in the parent and
/// the position in the root view, following the scroll view's offset.
#[test]
fn measure_reports_frames_and_page_positions() {
    let (engine, rx) = start(MEASURE);
    let mut scroll = None;
    let batches = until(&rx, |batch| {
        for item in &batch.items {
            if let MountItem::Create {
                id,
                kind: nsh_core::ViewKind::Scroll,
            } = item
            {
                scroll = Some(*id);
            }
        }
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Text { text, .. } if text.text.starts_with("at ")))
    });
    let shown: Vec<_> = batches
        .iter()
        .flat_map(|b| &b.items)
        .filter_map(|i| match i {
            MountItem::Text { text, .. } => Some(text.text.to_string()),
            _ => None,
        })
        .collect();
    // The text above the scroll view is one line (17 points with the approximate text engine) tall.
    assert!(
        shown
            .iter()
            .any(|t| t == "at 20,520 350x30 page 20,587 null"),
        "{shown:?}"
    );
    engine
        .control()
        .scroll(scroll.expect("scroll view"), 0.0, 400.0, 0);
    let after = until(&rx, |b| shows(b, "scrolled 187"));
    assert!(after.iter().any(|b| shows(b, "scrolled 187")));
}

const CLIPBOARD: &str = r#"import { mount, signal, Text } from "nsh";
import { Clipboard } from "nsh/clipboard";

const shown = signal("waiting");
setTimeout(async () => {
  const before = await Clipboard.hasString();
  Clipboard.setString("copied");
  const has = await Clipboard.hasString();
  const text = await Clipboard.getString();
  shown.set(`${before} ${has} ${text}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

/// A platform's clipboard: answers the core's reads from what it set.
fn serve_clipboard(engine: &Engine, rx: &Receiver<MountBatch>, wanted: &str) -> Vec<Command> {
    let control = engine.control();
    let mut board: Option<String> = None;
    let mut commands = Vec::new();
    until(rx, |batch| {
        for item in &batch.items {
            let MountItem::Command {
                id: ROOT_ID,
                command,
            } = item
            else {
                continue;
            };
            match command {
                Command::SetClipboard(text) => board = Some(text.clone()),
                Command::ReadClipboard { id, has_only } => {
                    let mut payload = vec![
                        ("id", PayloadValue::Number(*id as f64)),
                        ("has", PayloadValue::Bool(board.is_some())),
                    ];
                    if let (false, Some(text)) = (has_only, &board) {
                        payload.push(("text", PayloadValue::Str(text.clone())));
                    }
                    control.dispatch_event(
                        ROOT_ID,
                        EventKind::Clipboard,
                        EventPayload::Map(payload),
                    );
                }
                _ => continue,
            }
            commands.push(command.clone());
        }
        shows(batch, wanted)
    });
    commands
}

#[test]
fn the_clipboard_round_trips_through_the_platform() {
    let (engine, rx) = start(CLIPBOARD);
    let commands = serve_clipboard(&engine, &rx, "false true copied");
    assert_eq!(
        commands,
        [
            Command::ReadClipboard {
                id: 1,
                has_only: true
            },
            Command::SetClipboard("copied".into()),
            Command::ReadClipboard {
                id: 2,
                has_only: true
            },
            Command::ReadClipboard {
                id: 3,
                has_only: false
            },
        ]
    );
}

#[test]
fn the_clipboard_needs_its_capability() {
    let (engine, rx) = start_with(CLIPBOARD, Environment::default(), Some("{}"), None);
    // Nothing reaches the platform, and reads find nothing.
    let commands = serve_clipboard(&engine, &rx, "false false ");
    assert!(commands.is_empty(), "{commands:?}");
    let (engine, rx) = start_with(
        CLIPBOARD,
        Environment::default(),
        Some(r#"{ "clipboard": true }"#),
        None,
    );
    serve_clipboard(&engine, &rx, "false true copied");
}

const CLIPBOARD_IMAGE: &str = r#"import { mount, signal, Text } from "nsh";
import { Clipboard } from "nsh/clipboard";

const shown = signal("waiting");
setTimeout(async () => {
  const before = await Clipboard.hasImage();
  Clipboard.setImage("data:image/png;base64,aGk=");
  const image = await Clipboard.getImage();
  const bytes = new Uint8Array(await (await fetch(image!.uri)).arrayBuffer());
  const files = await Clipboard.getFiles();
  shown.set(`${before} ${image!.width}x${image!.height} ${bytes.join(",")} ${files.join(" ")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn clipboard_images_and_files_reach_js() {
    let (engine, rx) = start(CLIPBOARD_IMAGE);
    let control = engine.control();
    let mut board: Option<String> = None;
    let mut commands = Vec::new();
    until(&rx, |batch| {
        for item in &batch.items {
            let MountItem::Command {
                id: ROOT_ID,
                command,
            } = item
            else {
                continue;
            };
            let payload = match command {
                Command::SetClipboardImage(uri) => {
                    board = Some(uri.clone());
                    None
                }
                Command::ReadClipboardImage { id, has_only } => {
                    let mut payload = vec![
                        ("id", PayloadValue::Number(*id as f64)),
                        ("has", PayloadValue::Bool(board.is_some())),
                    ];
                    if let (false, Some(uri)) = (has_only, &board) {
                        payload.push(("uri", PayloadValue::Str(uri.clone())));
                        payload.push(("width", PayloadValue::Number(2.0)));
                        payload.push(("height", PayloadValue::Number(1.0)));
                    }
                    Some(payload)
                }
                Command::ReadClipboardFiles { id } => Some(vec![
                    ("id", PayloadValue::Number(*id as f64)),
                    ("has", PayloadValue::Bool(true)),
                    (
                        "files",
                        PayloadValue::Str("file:///tmp/a%20b.txt\nfile:///tmp/c/".into()),
                    ),
                ]),
                _ => continue,
            };
            if let Some(payload) = payload {
                control.dispatch_event(ROOT_ID, EventKind::Clipboard, EventPayload::Map(payload));
            }
            commands.push(command.clone());
        }
        shows(
            batch,
            "false 2x1 104,105 file:///tmp/a%20b.txt file:///tmp/c/",
        )
    });
    assert_eq!(
        commands,
        [
            Command::ReadClipboardImage {
                id: 1,
                has_only: true
            },
            Command::SetClipboardImage("data:image/png;base64,aGk=".into()),
            Command::ReadClipboardImage {
                id: 2,
                has_only: false
            },
            Command::ReadClipboardFiles { id: 3 },
        ]
    );
}

const INPUT_TRAITS: &str = r##"import { mount, Input, Text } from "nsh";

mount(() => (
  <>
    <Input keyboardType="numeric" ios={{ keyboardAppearance: "dark" }} />
    <Input keyboardType="numbers-and-punctuation" />
    <Text selectable selectionColor="#ff0000">Pick me</Text>
  </>
));
"##;

#[test]
fn inputs_and_selectable_text_carry_their_traits() {
    let (_engine, rx) = start(INPUT_TRAITS);
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    let inputs: Vec<_> = first
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Input { config, .. } => Some((config.keyboard, config.keyboard_appearance)),
            _ => None,
        })
        .collect();
    assert_eq!(
        inputs,
        [
            (KeyboardType::Numeric, KeyboardAppearance::Dark),
            (
                KeyboardType::NumbersAndPunctuation,
                KeyboardAppearance::Default
            ),
        ]
    );
    let text = first
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Text { text, .. } if text.text == "Pick me" => Some(text.clone()),
            _ => None,
        })
        .expect("the text");
    assert!(text.is_selectable());
    assert_eq!(
        text.selection_color,
        Some(nsh_core::Color::rgba(255, 0, 0, 255))
    );
}

const ROWS: &str = r#"import { mount, Text } from "nsh";
import { Group, ListView } from "nsh/ui/mobile";

const rows = Array.from({ length: 100 }, (_, i) => ({ id: i, label: `Row ${i}` }));

mount(() => (
  <ListView
    each={rows}
    ListHeaderComponent={() => (
      <Group>
        <Text>First</Text>
        <Text>Second</Text>
      </Group>
    )}
  >
    {(row) => <Text>{row().label}</Text>}
  </ListView>
));
"#;

#[test]
fn components_place_the_apps_children_and_rows() {
    let (_engine, rx) = start(ROWS);
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    let parent_of = |id| {
        first.items.iter().find_map(|i| match i {
            MountItem::Insert { parent, child, .. } if *child == id => Some(*parent),
            _ => None,
        })
    };
    let frame = |id| {
        first.items.iter().find_map(|i| match i {
            MountItem::Frame { id: f, frame } if *f == id => Some(*frame),
            _ => None,
        })
    };
    let top_border = |id| {
        first.items.iter().any(|i| {
            matches!(i, MountItem::ViewStyle { id: v, style } if *v == id && style.borders.widths[0] == 1.0)
        })
    };
    let a = parent_of(text_ids(&first, "First").expect("First")).unwrap();
    let b = parent_of(text_ids(&first, "Second").expect("Second")).unwrap();
    assert!(a % 2 == 1 && b % 2 == 1 && a != b);
    assert!(!top_border(a) && top_border(b));
    let row0 = parent_of(text_ids(&first, "Row 0").expect("Row 0")).unwrap();
    let row1 = parent_of(text_ids(&first, "Row 1").expect("Row 1")).unwrap();
    assert!(row0 % 2 == 1 && row1 % 2 == 1);
    assert_eq!(
        frame(text_ids(&first, "Row 0").unwrap()).map(|f| f.x),
        Some(16.0)
    );
    assert!(!top_border(row0) && top_border(row1));
    assert!(text_ids(&first, "Row 99").is_none());
}

const STEPPER: &str = r#"import { mount, signal, Text, View } from "nsh";
import { Stepper } from "nsh/ui/mobile";

const changes = signal(0);
mount(() => (
  <View>
    <Stepper defaultValue={5} min={0} max={100} onChange={() => changes.update((c) => c + 1)} />
    <Text>{`changes ${changes()}`}</Text>
  </View>
));
"#;

const STATUS_BAR: &str = r##"import { signal, mount } from "nsh";
import { setStatusBarBackgroundColor } from "nsh/android";
import { StatusBar } from "nsh/ui/mobile";

const rendered = signal(true);
setStatusBarBackgroundColor("#ff0000");
StatusBar.setBarStyle("dark-content");
setTimeout(() => StatusBar.setHidden(true), 50);
setTimeout(() => rendered.set(false), 100);

mount(() => (
  <If when={rendered()}>
    <StatusBar barStyle="light-content" android={{ backgroundColor: "#00ff00" }} />
  </If>
));
"##;

const BACK: &str = r#"import { mount, signal, Text } from "nsh";
import { BackHandler } from "nsh/android";

const presses = signal(0);
BackHandler.addEventListener("hardwareBackPress", () => {
  presses.update((p) => p + 1);
  // The first press is handled, the second one leaves the app.
  return presses() === 1;
});

mount(() => <Text>{`presses ${presses()}`}</Text>);
"#;

const FIELD: &str = r#"import { mount, signal, Text, View } from "nsh";
import { TextField } from "nsh/ui/mobile";

const name = signal("Ada");
mount(() => (
  <View>
    <TextField value={name()} onChange={(v: string) => name.set(v)} />
    <Text>{`name ${name()}`}</Text>
  </View>
));
"#;

const APPEARANCE: &str = r#"import { mount, Text, View } from "nsh";
import { colorScheme, setColorScheme } from "nsh/device";

setTimeout(() => setColorScheme("light"), 20);

mount(() => (
  <View class="bg-white dark:bg-black">
    <Text>{`scheme ${colorScheme()}`}</Text>
  </View>
));
"#;

#[test]
fn the_apps_appearance_overrides_the_systems() {
    let (_engine, rx) = start_with(
        APPEARANCE,
        Environment::default(),
        None,
        Some(r#"{ "appearance": "dark" }"#),
    );
    let background = |batches: &[MountBatch]| {
        batches
            .iter()
            .flat_map(|b| &b.items)
            .rev()
            .find_map(|i| match i {
                MountItem::ViewStyle { style, .. } if style.background_color.0 & 0xff == 0xff => {
                    Some(style.background_color)
                }
                _ => None,
            })
    };
    // Dark from the start, though the system is light.
    let first = until(&rx, |b| shows(b, "scheme dark"));
    assert!(first.iter().any(|b| shows(b, "scheme dark")));
    assert_eq!(background(&first), Some(nsh_core::Color(0x0000_00ff)));
    assert!(first.iter().flat_map(|b| &b.items).any(|i| matches!(
        i,
        MountItem::Appearance {
            appearance: nsh_core::platform::Appearance::Dark
        }
    )));
    // `setColorScheme` switches at once.
    let later = until(&rx, |b| shows(b, "scheme light"));
    assert!(later.iter().any(|b| shows(b, "scheme light")), "{later:?}");
    assert_eq!(background(&later), Some(nsh_core::Color(0xffff_ffff)));
}

const MENU: &str = r#"import { mount, signal, Text } from "nsh";
import { Menu } from "nsh/desktop";

const chosen = signal("none");
Menu.setApplicationMenu([
  false,
  {
    label: "File",
    submenu: [
      { label: "New", shortcut: "mod+n", onSelect: () => chosen.set("new") },
      { type: "separator" },
      { role: "quit" },
    ],
  },
]);

Menu.popup([{ label: "Copy link", onSelect: () => chosen.set("copied") }]).then(() =>
  chosen.set(`${chosen()} closed`),
);

mount(() => <Text>{chosen()}</Text>);
"#;

/// `Menu.setApplicationMenu` reaches the platform as items with ids and
/// shortcuts (a role's by default), and the platform's `onMenu` calls the
/// chosen item's function; mobile platforms get nothing.
#[test]
fn menus_reach_the_platform_and_call_back() {
    use nsh_core::desktop::{DesktopCommand, Key, MenuAction, MenuItem, MenuRole, Shortcut};

    let (engine, rx) = start_in(
        MENU,
        Environment {
            os: nsh_engine::Os::Windows,
            ..Environment::default()
        },
    );
    let mut menu = None;
    until(&rx, |batch| {
        for item in &batch.items {
            if let MountItem::Command {
                id: ROOT_ID,
                command: Command::Desktop(command),
            } = item
                && let DesktopCommand::SetMenu(Some(items)) = &**command
            {
                menu = Some(items.clone());
            }
        }
        shows(batch, "none")
    });
    let menu = menu.expect("the menu");
    let new = MenuAction {
        id: 1,
        label: "New".into(),
        shortcut: Some(Shortcut {
            ctrl: true,
            ..Shortcut::key(Key::Char('n'))
        }),
        enabled: true,
        checked: None,
        role: None,
    };
    let quit = MenuAction {
        id: 0,
        label: String::new(),
        shortcut: MenuRole::Quit.shortcut(false),
        enabled: true,
        checked: None,
        role: Some(MenuRole::Quit),
    };
    assert_eq!(
        &*menu,
        [MenuItem::Submenu {
            label: "File".into(),
            enabled: true,
            items: vec![
                MenuItem::Action(new),
                MenuItem::Separator,
                MenuItem::Action(quit)
            ],
        }]
    );
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::Menu,
        EventPayload::Map(vec![("id", PayloadValue::Number(1.0))]),
    );
    assert!(
        until(&rx, |batch| shows(batch, "new"))
            .iter()
            .any(|b| shows(b, "new"))
    );

    // A context menu: its item, then its promise once it closes.
    let control = engine.control();
    control.dispatch_event(
        ROOT_ID,
        EventKind::Menu,
        EventPayload::Map(vec![("id", PayloadValue::Number(2.0))]),
    );
    control.dispatch_event(
        ROOT_ID,
        EventKind::Menu,
        EventPayload::Map(vec![
            ("id", PayloadValue::Number(0.0)),
            ("closed", PayloadValue::Bool(true)),
        ]),
    );
    assert!(
        until(&rx, |batch| shows(batch, "copied closed"))
            .iter()
            .any(|b| shows(b, "copied closed"))
    );

    // On a phone, nothing is sent.
    let (_engine, rx) = start(MENU);
    let batches = until(&rx, |batch| shows(batch, "none"));
    assert!(!batches.iter().flat_map(|b| &b.items).any(|i| matches!(
        i,
        MountItem::Command {
            command: Command::Desktop(_),
            ..
        }
    )));
}

const LOCALE: &str = r#"import { mount, Text } from "nsh";
import { fontScale, locale } from "nsh/device";

mount(() => (
  <Text>{`${locale()} ${new Intl.NumberFormat().format(1234.5)} ${fontScale()}`}</Text>
));
"#;

#[test]
fn locale_and_font_scale_follow_the_environment() {
    let (engine, rx) = start(LOCALE);
    let first = until(&rx, |b| shows(b, "en-US 1,234.5 1"));
    assert!(first.iter().any(|b| shows(b, "en-US 1,234.5 1")));
    // What an embedding app sets in place of the system's.
    let mut environment = Environment::default();
    nsh_engine::HostEnvironment {
        locale: nsh_engine::Locale::parse("de-DE"),
        font_scale: Some(1.5),
    }
    .apply(&mut environment);
    engine.control().set_environment(environment);
    let after = until(&rx, |b| shows(b, "de-DE 1.234,5 1.5"));
    assert!(after.iter().any(|b| shows(b, "de-DE 1.234,5 1.5")));
}
