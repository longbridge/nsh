//! The Rust entry's real component runs and handles input without a JS runtime.

use std::{sync::mpsc, time::Duration};

use nsh_core::{ApproxTextEngine, EventKind, EventPayload, MountBatch, MountItem, Size, ui::Root};
use nsh_engine::{App, Engine, EngineConfig, MountSink};

struct Sink(mpsc::Sender<MountBatch>);

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        self.0.send(batch).unwrap();
    }
}

#[test]
fn the_rust_root_renders_and_handles_a_press() {
    let (tx, rx) = mpsc::channel();
    let engine = Engine::start(
        EngineConfig {
            app: App::Rust(Root::of::<nsh_rust_app::App>()),
            viewport: Size::new(480.0, 640.0),
            trace: false,
            network: None,
            assets: None,
            environment: Default::default(),
            header_heights: (44.0, 96.0),
            limits: Default::default(),
            ui_sets: Vec::new(),
            runtime: None,
        },
        || ApproxTextEngine,
        Sink(tx),
    )
    .unwrap();
    let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        first
            .items
            .iter()
            .any(|item| matches!(item, MountItem::Text { text, .. } if text.text == "Count: 0"))
    );
    let counter = first
        .items
        .iter()
        .find_map(|item| match item {
            MountItem::Text { id, text, .. } if text.text == "Count: 0" => Some(*id),
            _ => None,
        })
        .unwrap();
    assert!(
        first.items.iter().any(
            |item| matches!(item, MountItem::Frame { id, frame } if *id == counter && frame.y > 200.0)
        ),
        "the Rust root fills the viewport and centers its content"
    );
    let button = first
        .items
        .iter()
        .find_map(|item| match item {
            MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
            _ => None,
        })
        .expect("the native button listens for presses");
    engine
        .control()
        .dispatch_event(button, EventKind::Press, EventPayload::None);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let batch = rx
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .unwrap();
        if batch
            .items
            .iter()
            .any(|item| matches!(item, MountItem::Text { text, .. } if text.text == "Count: 1"))
        {
            break;
        }
    }
}
