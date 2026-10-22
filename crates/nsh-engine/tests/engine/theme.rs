//! The embedding app's theme: sent before the start it is in the first
//! frame, sent later it updates what read the theme, and it wins over the
//! app's own theme until cleared.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_base::{Theme, theme};
use nsh_core::{
    ApproxTextEngine, Color, MountBatch, MountItem, Size,
    ui::{Root, prelude::*},
};
use nsh_engine::{App, Engine, EngineConfig, Environment};

use crate::ChannelSink;

/// A square of the theme's primary color.
#[component]
fn Swatch() -> impl IntoElement {
    view().size_10().bg(move || theme().primary)
}

/// The primary color of a theme that sets none: `nsh-base`'s default.
fn default_primary() -> Color {
    Color::from(Theme::default().primary.light)
}

fn themed(hex: &str) -> Theme {
    Theme::from_json(&format!(
        r#"{{ "cssVars": {{ "light": {{ "primary": "{hex}" }} }} }}"#
    ))
    .unwrap()
}

fn start(app_theme: Option<Theme>, host_theme: Option<Theme>) -> (Engine, Receiver<MountBatch>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut root = Root::new::<Swatch>(|| ());
    if let Some(theme) = app_theme {
        root = root.tokens(theme.into_tokens());
    }
    let config = EngineConfig {
        app: App::Rust(root),
        viewport: Size::new(px(390.0), px(844.0)),
        trace: false,
        network: None,
        websocket: None,
        filesystem: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (nsh_core::Points(44.0), nsh_core::Points(96.0)),
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
    let engine = Engine::start_with(
        config,
        || ApproxTextEngine,
        ChannelSink(tx),
        |control| {
            if let Some(theme) = host_theme {
                control.set_tokens(Some(theme.into_tokens()));
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
fn the_host_theme_updates_the_views_and_gives_way_to_the_apps_when_cleared() {
    let (engine, rx) = start(Some(themed("#0000ff")), None);
    assert_eq!(backgrounds(&rx), vec![Color(0x0000ffff)]);
    engine
        .control()
        .set_tokens(Some(themed("#00ff00").into_tokens()));
    assert_eq!(backgrounds(&rx), vec![Color(0x00ff00ff)]);
    engine.control().set_tokens(None);
    assert_eq!(backgrounds(&rx), vec![Color(0x0000ffff)]);
}

#[test]
fn a_tsx_apps_token_classes_follow_the_host_theme() {
    let dir = std::env::temp_dir().join(format!("nsh-theme-tokens-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(
        &entry,
        "import { mount, View } from \"nsh\";\n\
         mount(() => <View class=\"h-10 w-10 bg-primary\" />);\n",
    )
    .unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: Vec::new(),
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode(&bundle.code, false).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        // SAFETY: compiled just now by this runtime.
        app: App::Js(unsafe { nsh_engine::Bundle::bytecode(bytecode) }),
        viewport: Size::new(px(390.0), px(844.0)),
        trace: false,
        network: None,
        websocket: None,
        filesystem: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (nsh_core::Points(44.0), nsh_core::Points(96.0)),
        limits: nsh_engine::Limits::default(),
        ui_sets: vec![nsh_base::registry()],
        runtime: None,
        data_dir: None,
        storage: None,
        clock: Default::default(),
        context: Default::default(),
        pinned: Default::default(),
        clipboard: None,
    };
    let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    assert_eq!(backgrounds(&rx), vec![default_primary()]);
    engine
        .control()
        .set_tokens(Some(themed("#00ff00").into_tokens()));
    assert_eq!(backgrounds(&rx), vec![Color(0x00ff00ff)]);
    engine.control().set_tokens(None);
    assert_eq!(backgrounds(&rx), vec![default_primary()]);
    let _ = std::fs::remove_dir_all(&dir);
}
