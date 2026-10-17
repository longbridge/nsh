//! The embedding app's theme: sent before the start it is in the first
//! frame, sent later it renders the components again, and it wins over
//! the app's own theme until cleared.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, Color, MountBatch, MountItem, Size,
    ui::{Root, Theme, prelude::*},
};
use nsh_engine::{App, Engine, EngineConfig, Environment};

use crate::ChannelSink;

#[derive(Default)]
struct Swatch;

impl Component for Swatch {
    type Props = ();
    type Event = NoEvent;

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        view().size_10().bg(cx.theme().primary)
    }
}

fn themed(hex: &str) -> Theme {
    Theme::from_json(&format!(
        r#"{{ "themes": [{{ "colors": {{ "primary.background": "{hex}" }} }}] }}"#
    ))
    .unwrap()
}

fn start(app_theme: Option<Theme>, host_theme: Option<Theme>) -> (Engine, Receiver<MountBatch>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut root = Root::new::<Swatch>(|| ());
    if let Some(theme) = app_theme {
        root = root.theme(theme);
    }
    let config = EngineConfig {
        app: App::Rust(root),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
    };
    let engine = Engine::start_with(
        config,
        || ApproxTextEngine,
        ChannelSink(tx),
        |control| {
            if host_theme.is_some() {
                control.set_theme(host_theme);
            }
        },
    )
    .unwrap();
    (engine, rx)
}

/// The backgrounds of the batches within a moment.
fn backgrounds(rx: &Receiver<MountBatch>) -> Vec<Color> {
    let mut seen = Vec::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
        for item in &batch.items {
            if let MountItem::ViewStyle { style, .. } = item
                && style.background_color.0 & 0xff != 0
            {
                seen.push(style.background_color);
            }
        }
    }
    seen
}

#[test]
fn a_host_theme_sent_before_the_start_is_in_the_first_frame() {
    let (_engine, rx) = start(Some(themed("#0000ff")), Some(themed("#00ff00")));
    let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let colors: Vec<Color> = first
        .items
        .iter()
        .filter_map(|item| match item {
            MountItem::ViewStyle { style, .. } => Some(style.background_color),
            _ => None,
        })
        .collect();
    assert_eq!(colors, vec![Color(0x00ff00ff)]);
}

#[test]
fn the_host_theme_rerenders_and_gives_way_to_the_apps_when_cleared() {
    let (engine, rx) = start(Some(themed("#0000ff")), None);
    assert_eq!(backgrounds(&rx), vec![Color(0x0000ffff)]);
    engine.control().set_theme(Some(themed("#00ff00")));
    assert_eq!(backgrounds(&rx), vec![Color(0x00ff00ff)]);
    engine.control().set_theme(None);
    assert_eq!(backgrounds(&rx), vec![Color(0x0000ffff)]);
}
