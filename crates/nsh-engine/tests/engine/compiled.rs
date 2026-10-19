//! Apps end to end:
//! one TSX file importing only `nsh`, bundled as an ES module, run on the
//! native modules through bytecode, tapped after the idle GC.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{ApproxTextEngine, EventKind, EventPayload, MountBatch, MountItem, Size};
use nsh_engine::{Bundle, Engine, EngineConfig};

use crate::ChannelSink;

const APP: &str = r#"import { computed, mount, signal, Text, View } from "nsh";

function Counter(props: { label: string }) {
  const count = signal(0);
  const double = computed(() => count() * 2);
  return (
    <View testID="counter" onPress={() => count.update((c) => c + 1)}>
      <Text>{props.label}: {count()} / {double()}</Text>
      <If when={count() > 1} else={<Text>few</Text>}>
        <Text>many</Text>
      </If>
    </View>
  );
}

function App() {
  const items = signal([{ id: "a" }, { id: "b" }]);
  return (
    <View>
      <Counter label="Taps" />
      <View testID="rotate" onPress={() => items.update((i) => [...i.slice(1), i[0]])}>
        <Each of={items()} key={(i) => i.id}>{(item, index) => <Text>{index()}{item().id}</Text>}</Each>
      </View>
    </View>
  );
}

mount(App);
"#;

fn start() -> (Engine, Receiver<MountBatch>) {
    let dir = std::env::temp_dir().join(format!("nsh-compiled-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("App.tsx"), APP).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry: dir.join("App.tsx"),
        minify: true,
        split: false,
        ui_sets: vec![nsh_ui_mobile::registry()],
    })
    .unwrap();
    assert!(!bundle.code.contains("solid"), "only the app's code");
    let bytecode =
        nsh_js::compile_bytecode_with(&bundle.code, true, &[nsh_ui_mobile::registry()]).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        // SAFETY: compiled just now by this runtime.
        app: nsh_engine::App::Js(unsafe { Bundle::bytecode(bytecode) }),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Default::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        storage: None,
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

fn texts(batch: &MountBatch) -> Vec<String> {
    batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Text { text, .. } => Some(text.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_single_file_app_on_the_native_modules() {
    let (engine, rx) = start();
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    let mut shown = texts(&first);
    shown.sort();
    assert_eq!(shown, ["0a", "1b", "Taps: 0 / 0", "few"]);
    let mut pressable: Vec<_> = first
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
            _ => None,
        })
        .collect();
    // Static children are made before dynamic ones: the rotating view
    // comes before the counter component's.
    pressable.sort();
    let [rotate, counter] = pressable[..] else {
        panic!("{pressable:?}")
    };

    // The idle collector runs: the app stays reachable through its views.
    while rx.recv_timeout(Duration::from_millis(1300)).is_ok() {}

    let control = engine.control();
    let tap = |id| {
        control.dispatch_event(id, EventKind::Press, EventPayload::None);
        rx.recv_timeout(Duration::from_secs(5)).expect("an update")
    };
    assert_eq!(texts(&tap(counter)), ["Taps: 1 / 2"]);
    let second = tap(counter);
    assert!(
        texts(&second).contains(&"many".to_owned()),
        "{:?}",
        texts(&second)
    );
    // Rows keep their views and show their new index.
    let rotated = tap(rotate);
    assert!(
        !rotated
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Create { .. })),
        "{:?}",
        rotated.items
    );
    let mut t = texts(&rotated);
    t.sort();
    assert_eq!(t, ["0b", "1a"]);
}

const NAVIGATION: &str = r#"import { mount, signal, Text, View } from "nsh";
import { Modal, Screen, Stack, createNavigator, useSafeAreaInsets } from "nsh/ui/mobile";

type Route = { name: string };
const nav = createNavigator<Route>({ name: "home" });
const open = signal(false);

function Insets() {
  const insets = useSafeAreaInsets();
  return <Text>{`inset ${insets().top}`}</Text>;
}

mount(() => (
  <View style={{ flex: 1 }}>
    <Stack navigator={nav} style={{ flex: 1 }}>
      {(route) => (
        <Screen title={route.name} headerRight={<Text onPress={() => open.set(true)}>open</Text>}>
          <View>
            <Text onPress={() => nav.push({ name: `page${nav.routes().length}` })}>push {route.name}</Text>
            <Insets />
          </View>
        </Screen>
      )}
    </Stack>
    <Modal visible={open()} onRequestClose={() => open.set(false)}>
      <Text>in modal</Text>
    </Modal>
  </View>
));
"#;

fn start_app(app: &str) -> (Engine, Receiver<MountBatch>) {
    let dir = std::env::temp_dir().join(format!(
        "nsh-compiled-nav-{}-{}",
        std::process::id(),
        app.len()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("App.tsx"), app).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry: dir.join("App.tsx"),
        minify: false,
        split: false,
        ui_sets: vec![nsh_ui_mobile::registry()],
    })
    .unwrap();
    let bytecode =
        nsh_js::compile_bytecode_with(&bundle.code, false, &[nsh_ui_mobile::registry()]).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        // SAFETY: compiled just now by this runtime.
        app: nsh_engine::App::Js(unsafe { Bundle::bytecode(bytecode) }),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Default::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: vec![nsh_ui_mobile::registry()],
        runtime: None,
        storage: None,
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// The next batch `wanted` accepts (releases of popped screens' views may
/// come in batches of their own first).
fn next(rx: &Receiver<MountBatch>, wanted: impl Fn(&MountBatch) -> bool) -> MountBatch {
    loop {
        let batch = rx.recv_timeout(Duration::from_secs(5)).expect("a batch");
        if wanted(&batch) {
            return batch;
        }
    }
}

fn text_id(batch: &MountBatch, wanted: &str) -> Option<u32> {
    batch.items.iter().find_map(|i| match i {
        MountItem::Text { id, text, .. } if text.text == wanted => Some(*id),
        _ => None,
    })
}

fn titles(batch: &MountBatch) -> Vec<(u32, String)> {
    batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::ScreenConfig { id, config } => {
                Some((*id, config.title.as_deref()?.to_owned()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn navigation_stacks_screens_and_modals() {
    let (engine, rx) = start_app(NAVIGATION);
    let control = engine.control();
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    let home = titles(&first);
    assert_eq!(home.last().map(|t| t.1.as_str()), Some("home"), "{home:?}");
    let home_screen = home[0].0;
    assert!(text_id(&first, "inset 0").is_some(), "{:?}", texts(&first));
    // The screen reports its insets: what it renders sees them.
    let insets = |top: f64| {
        EventPayload::Map(vec![
            ("top", nsh_core::PayloadValue::Number(top)),
            ("right", nsh_core::PayloadValue::Number(0.0)),
            ("bottom", nsh_core::PayloadValue::Number(34.0)),
            ("left", nsh_core::PayloadValue::Number(0.0)),
        ])
    };
    control.dispatch_event(home_screen, EventKind::Insets, insets(100.0));
    let batch = rx.recv_timeout(Duration::from_secs(5)).expect("insets");
    assert!(
        texts(&batch).contains(&"inset 100".to_owned()),
        "{:?}",
        texts(&batch)
    );
    // Push: a second screen.
    let push = text_id(&first, "push home").unwrap();
    control.dispatch_event(push, EventKind::Press, EventPayload::None);
    let batch = rx.recv_timeout(Duration::from_secs(5)).expect("pushed");
    let page = titles(&batch);
    assert_eq!(page.last().map(|t| t.1.as_str()), Some("page1"), "{page:?}");
    let page_screen = page[0].0;
    // The user pops it natively: its route goes, so the next push is page1
    // again.
    control.dispatch_event(page_screen, EventKind::Dismiss, EventPayload::None);
    while rx.recv_timeout(Duration::from_millis(200)).is_ok() {}
    control.dispatch_event(push, EventKind::Press, EventPayload::None);
    let batch = next(&rx, |b| !titles(b).is_empty());
    assert_eq!(titles(&batch).last().map(|t| t.1.as_str()), Some("page1"));
    // The header's button opens the modal.
    let open = text_id(&first, "open").expect("a header item");
    control.dispatch_event(open, EventKind::Press, EventPayload::None);
    let batch = next(&rx, |b| {
        b.items.iter().any(|i| {
            matches!(
                i,
                MountItem::Create {
                    kind: nsh_core::ViewKind::Modal,
                    ..
                }
            )
        })
    });
    assert!(text_id(&batch, "in modal").is_some());
}

const MODAL_DISMISS: &str = r#"import { mount, signal, Text, View } from "nsh";
import { Modal } from "nsh/ui/mobile";

const open = signal(true);
const status = signal("waiting");

mount(() => (
  <View style={{ flex: 1 }}>
    <Text>{status()}</Text>
    <Modal visible={open()} onRequestClose={() => open.set(false)} onDismiss={() => status.set("dismissed")}>
      <Text>in modal</Text>
    </Modal>
  </View>
));
"#;

/// An app removes its modal when asked to close it; the platform reports
/// the dismissal once the modal has gone, and the app still hears it.
#[test]
fn removed_modal_hears_its_dismissal() {
    let (engine, rx) = start_app(MODAL_DISMISS);
    let control = engine.control();
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    let modal = first
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Create {
                id,
                kind: nsh_core::ViewKind::Modal,
                ..
            } => Some(*id),
            _ => None,
        })
        .expect("a modal");
    control.dispatch_event(modal, EventKind::RequestClose, EventPayload::None);
    next(&rx, |b| {
        b.items
            .iter()
            .any(|i| matches!(i, MountItem::Remove { id } if *id == modal))
    });
    control.dispatch_event(modal, EventKind::Dismiss, EventPayload::None);
    let batch = next(&rx, |b| text_id(b, "dismissed").is_some());
    assert!(text_id(&batch, "dismissed").is_some());
}
