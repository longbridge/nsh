//! `nsh/storage`: an app's entries kept in a file across launches, its
//! quota.

use std::{path::PathBuf, time::Duration};

use crossbeam_channel::Receiver;
use nsh_core::{ApproxTextEngine, MountBatch, MountItem, Size};
use nsh_engine::{Bundle, Engine, EngineConfig, Environment, FileStorage};

use crate::ChannelSink;

/// A directory of its own under the test's temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nsh-storage-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `app` with `capabilities`, its storage in `data`.
fn start(
    name: &str,
    app: &str,
    capabilities: Option<&str>,
    data: &PathBuf,
) -> (Engine, Receiver<MountBatch>) {
    let dir = scratch(name);
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, app).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: Vec::new(),
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &[]).unwrap();
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
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        storage: Some(FileStorage::open(data)),
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// The text shown once it starts with `prefix` (or the last one, after
/// nothing came for 2 s).
fn shown(rx: &Receiver<MountBatch>, prefix: &str) -> String {
    let mut last = String::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_secs(2)) {
        for item in &batch.items {
            if let MountItem::Text { text, .. } = item {
                last = text.text.clone();
                if last.starts_with(prefix) {
                    return last;
                }
            }
        }
    }
    last
}

const COUNTER: &str = r#"import { mount, signal, Text } from "nsh";
import { Storage } from "nsh/storage";

const shown = signal("waiting");
setTimeout(async () => {
  try {
    const launches = Number((await Storage.getItem("launches")) ?? "0") + 1;
    Storage.setItem("launches", String(launches));
    Storage.setItem("scratch", "x");
    Storage.removeItem("scratch");
    const keys = await Storage.getAllKeys();
    let full = "fits";
    await Storage.setItem("big", "x".repeat(6 * 1024 * 1024)).catch(() => (full = "full"));
    shown.set(`done ${launches} ${keys.join(",")} ${full} ${await Storage.getItem("missing")}`);
  } catch (e) {
    shown.set(`done error ${e}`);
  }
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn entries_last_across_launches() {
    let data = scratch("data");
    for launch in 1..=2 {
        // Capabilities do not cover storage: the app's own data.
        let (engine, rx) = start("counter", COUNTER, Some("{}"), &data);
        assert_eq!(
            shown(&rx, "done"),
            format!("done {launch} launches full null")
        );
        // Stopping the engine drops the storage, which writes what is left.
        drop(engine);
    }
    let file = std::fs::read_to_string(data.join("storage.json")).unwrap();
    assert_eq!(file, r#"{"launches":"2"}"#);
}
