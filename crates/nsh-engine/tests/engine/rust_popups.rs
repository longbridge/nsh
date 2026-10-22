//! `nsh/base`'s popups used from Rust: a `Popover` filled with a trigger
//! and content shows its content in an overlay of the surface's root view,
//! placed against the trigger, outside the box that clips it.

use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use nsh_base::popover;
use nsh_core::{
    ApproxTextEngine, Bounds, MountBatch, MountItem, NodeId, Points, ROOT_ID, Size,
    ui::{Root, prelude::*},
};
use nsh_engine::{App, Engine, EngineConfig, Environment};

use crate::ChannelSink;

#[derive(Clone, PartialEq, Props)]
struct ScreenProps {}

#[component]
fn Screen(_props: ScreenProps) -> impl IntoElement {
    view().pt(px(100.0)).pl(px(40.0)).child(
        view().w(px(200.0)).h(px(30.0)).overflow_hidden().child(
            popover()
                .default_open(true)
                .placement(Placement::Bottom)
                .align(Align::Start)
                .offset(px(4.0))
                .trigger(view().h(px(30.0)).child(text("Trigger")))
                .child(view().w(px(120.0)).h(px(50.0)).child(text("Body"))),
        ),
    )
}

fn start() -> (Engine, Receiver<MountBatch>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: App::Rust(Root::new::<Screen>(|| ScreenProps {})),
        viewport: Size::new(px(390.0), px(844.0)),
        trace: false,
        network: None,
        websocket: None,
        filesystem: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (Points(44.0), Points(96.0)),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        data_dir: None,
        storage: None,
        clock: Default::default(),
        context: Default::default(),
        pinned: Default::default(),
        clipboard: None,
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

#[test]
fn a_popover_shows_its_content_over_the_box_that_clips_its_trigger() {
    let (_engine, rx) = start();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut parents: Vec<(NodeId, NodeId)> = Vec::new();
    let mut frames: Vec<(NodeId, Bounds<Points>)> = Vec::new();
    let mut body = None;
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            match item {
                MountItem::Insert { parent, child, .. } => parents.push((*parent, *child)),
                MountItem::Frame { id, frame } => frames.push((*id, *frame)),
                MountItem::Text { id, text, .. } if &*text.text == "Body" => body = Some(*id),
                _ => {}
            }
        }
        if body.is_some() {
            break;
        }
    }
    let body = body.expect("the content is shown");
    let parent = |id: NodeId| {
        parents
            .iter()
            .rev()
            .find(|(_, c)| *c == id)
            .map(|(p, _)| *p)
    };
    // The text, in its 120 × 50 box, in the overlay, in the root view.
    let content = parent(body).unwrap();
    let overlay = parent(content).unwrap();
    assert_eq!(parent(overlay), Some(ROOT_ID));
    let frame = frames
        .iter()
        .rev()
        .find(|(id, _)| *id == overlay)
        .unwrap()
        .1;
    // Below the trigger (40, 100, 200 × 30), 4 points away, from its start.
    assert_eq!(frame.origin, point(px(40.0), px(134.0)));
    assert_eq!(frame.size, Size::new(px(120.0), px(50.0)));
}
