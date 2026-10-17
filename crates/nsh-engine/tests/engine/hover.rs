//! `onHoverIn` / `onHoverOut` in a TSX app: the views' event masks tell
//! the platform which views listen (the desktops raise the events from
//! their hover tracking), and the payload-less events reach the handlers.

use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, EventKind, EventMask, EventPayload, MountBatch, MountItem, NodeId, Size,
};
use nsh_engine::{Bundle, Engine, EngineConfig};

use crate::ChannelSink;

const APP: &str = r#"import { mount, signal, Text, View } from "nsh";

function App() {
  const log = signal<string[]>([]);
  const note = (s: string) => log.update((l) => [...l, s]);
  return (
    <View>
      <View testID="outer" onHoverIn={() => note("in outer")} onHoverOut={() => note("out outer")}>
        <View testID="inner" style={{ height: 40 }} onHoverIn={() => note("in inner")} />
      </View>
      <Text>{"log " + log().join("|")}</Text>
    </View>
  );
}

mount(App);
"#;

fn start() -> (Engine, Receiver<MountBatch>) {
    let dir = std::env::temp_dir().join(format!("nsh-hover-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("App.tsx"), APP).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry: dir.join("App.tsx"),
        minify: false,
        ui_sets: vec![nsh_ui_mobile::registry()],
        split: false,
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode(&bundle.code, false).unwrap();
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
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// Waits until the log text is `want`, within a few seconds.
fn until_log(rx: &Receiver<MountBatch>, want: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last = String::new();
    while let Ok(batch) = rx.recv_deadline(deadline) {
        if let Some(text) = batch.items.iter().find_map(|i| match i {
            MountItem::Text { text, .. } if text.text.starts_with("log ") => Some(&text.text),
            _ => None,
        }) {
            last = text.clone();
        }
        if last == want {
            return;
        }
    }
    panic!("the log never became {want:?}: {last:?}");
}

#[test]
fn hover_handlers_are_in_the_event_masks_and_hear_their_events() {
    let (engine, rx) = start();
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    let id = |wanted: &str| -> NodeId {
        first
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::ViewProps { id, props } if props.test_id.as_deref() == Some(wanted) => {
                    Some(*id)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{wanted} is mounted"))
    };
    let events = |id: NodeId| {
        first
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Events { id: i, events } if *i == id => Some(*events),
                _ => None,
            })
            .unwrap_or_default()
    };
    let (outer, inner) = (id("outer"), id("inner"));
    let mask = |kinds: &[EventKind]| {
        let mut m = EventMask::default();
        for &k in kinds {
            m.set(k, true);
        }
        m
    };
    assert_eq!(
        events(outer),
        mask(&[EventKind::HoverIn, EventKind::HoverOut])
    );
    assert_eq!(events(inner), mask(&[EventKind::HoverIn]));

    // As a desktop raises them: out from the inside, in from the outside.
    let control = engine.control();
    control.dispatch_event(outer, EventKind::HoverIn, EventPayload::None);
    control.dispatch_event(inner, EventKind::HoverIn, EventPayload::None);
    until_log(&rx, "log in outer|in inner");
    control.dispatch_event(outer, EventKind::HoverOut, EventPayload::None);
    until_log(&rx, "log in outer|in inner|out outer");
}
