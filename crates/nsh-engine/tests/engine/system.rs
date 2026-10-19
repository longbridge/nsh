//! Native worker completions must wake the engine, including during initial evaluation.
use std::time::{Duration, Instant};

use nsh_core::{ApproxTextEngine, MountItem, Size, platform::Os};
use nsh_engine::{Bundle, Engine, EngineConfig, Environment};

use crate::ChannelSink;

#[test]
fn filesystem_completion_during_initial_evaluation_is_delivered_without_timers() {
    let dir = std::env::temp_dir().join(format!("nsh-system-startup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data/com.example.system")).unwrap();
    std::fs::write(
        dir.join("data/com.example.system/input.txt"),
        "native file ready",
    )
    .unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(
        &entry,
        r#"import { mount, signal, Text } from "nsh";
import fs from "fs/promises";
const status = signal("waiting");
const request = fs.readFile("${dataDir}/input.txt", "utf8");
// Finish the native operation before the engine finishes evaluating this module.
const until = Date.now() + 100;
while (Date.now() < until) {}
request.then(value => status.set(value), error => status.set(String(error)));
mount(() => <Text>{status()}</Text>);
"#,
    )
    .unwrap();
    let bundled = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: Vec::new(),
    })
    .unwrap();
    let bundle = Bundle::source(bundled.code)
        .with_app_id(Some("com.example.system"))
        .with_capabilities_json(Some(r#"{"fs":{"read":["${dataDir}"]}}"#))
        .unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: nsh_engine::App::Js(bundle),
        viewport: Size::new(400.0, 300.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment {
            os: if cfg!(target_os = "macos") {
                Os::MacOs
            } else {
                Os::Linux
            },
            ..Default::default()
        },
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        data_dir: Some(dir.join("data")),
        storage: None,
    };
    let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut shown = Vec::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(batch) = rx.recv_timeout(remaining) else {
            break;
        };
        shown.extend(batch.items.iter().filter_map(|item| match item {
            MountItem::Text { text, .. } => Some(text.text.clone()),
            _ => None,
        }));
        if shown.iter().any(|text| text == "native file ready") {
            break;
        }
    }
    drop(engine);
    let _ = std::fs::remove_dir_all(dir);
    assert!(
        shown.iter().any(|text| text == "native file ready"),
        "native completion never reached the UI: {shown:?}"
    );
}

#[test]
fn trusted_path_anchors_reach_the_vm_without_application_metadata() {
    let temporary = crate::temporary_root();
    let root = temporary.join(format!("nsh-engine-trusted-paths-{}", std::process::id()));
    let plugin = root.join("plugin");
    let home = root.join("home");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(plugin.join("source.txt"), "plugin").unwrap();
    std::fs::write(home.join("home.txt"), "home").unwrap();
    let entry = plugin.join("App.tsx");
    std::fs::write(
        &entry,
        r#"import {mount, signal, Text} from 'nsh';
import fs from 'fs/promises';
const status = signal('waiting');
Promise.all([
  fs.readFile('source.txt', 'utf8'),
  fs.readFile('${pluginDir}/source.txt', 'utf8'),
  fs.readFile('${homeDir}/home.txt', 'utf8'),
]).then(values => status.set(values.join('|')), error => status.set(String(error)));
mount(() => <Text>{status()}</Text>);"#,
    )
    .unwrap();
    let bundled = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: Vec::new(),
    })
    .unwrap();
    let mut paths = nsh_core::access::TrustedPaths::default();
    paths.plugin_dir = Some(plugin);
    paths.home_dir = Some(home);
    let bundle = Bundle::source(bundled.code)
        .with_app_id(Some("com.example.anchors"))
        .with_system_paths(paths)
        .with_capabilities_json(Some(r#"{"fs":{"read":["${pluginDir}","${homeDir}"]}}"#))
        .unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: nsh_engine::App::Js(bundle),
        viewport: Size::new(400.0, 300.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment {
            os: Os::Linux,
            ..Default::default()
        },
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        data_dir: Some(root.join("data")),
        storage: None,
    };
    let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut shown = Vec::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(batch) = rx.recv_timeout(remaining) else {
            break;
        };
        shown.extend(batch.items.iter().filter_map(|item| match item {
            MountItem::Text { text, .. } => Some(text.text.clone()),
            _ => None,
        }));
        if shown.iter().any(|text| text == "plugin|plugin|home") {
            break;
        }
    }
    drop(engine);
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        shown.iter().any(|text| text == "plugin|plugin|home"),
        "anchors never reached the VM: {shown:?}"
    );
}
