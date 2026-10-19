//! `nsh/storage`: an app's entries kept in a file across launches, its
//! quota, and the same entries read and written from Rust.

use std::{path::PathBuf, time::Duration};

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, MountBatch, MountItem, Size,
    ui::{Root, prelude::*},
};
use nsh_engine::{App, Bundle, Engine, EngineConfig, Environment, FileStorage};

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

#[derive(Clone, PartialEq, Props)]
struct ReaderProps {}

/// The Rust entry of an app whose TSX entry is [`COUNTER`]: reads what the
/// JS wrote and writes for it.
#[component]
fn Reader(_props: ReaderProps) -> impl IntoElement {
    let seen = storage_item("launches").unwrap_or_default();
    set_storage_item("launches", "2").unwrap();
    set_storage_item("from_rust", "hi").unwrap();
    let keys = storage_keys().join(",");
    text(move || format!("rust saw {seen} {keys}"))
}

#[test]
fn rust_shares_the_entries_of_js() {
    let data = scratch("shared");
    let (engine, rx) = start("shared", COUNTER, None, &data);
    assert_eq!(shown(&rx, "done"), "done 1 launches full null");
    drop(engine);
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: App::Rust(Root::new::<Reader>(|| ReaderProps {})),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        storage: Some(FileStorage::open(&data)),
    };
    let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    assert_eq!(shown(&rx, "rust"), "rust saw 1 from_rust,launches");
    drop(engine);
    // And the JS sees what Rust wrote.
    let (_engine, rx) = start("shared-again", COUNTER, None, &data);
    assert_eq!(shown(&rx, "done"), "done 3 from_rust,launches full null");
}

const EDGES: &str = r#"import { mount, signal, Text } from "nsh";
import { Storage } from "nsh/storage";

const shown = signal("waiting");
const outcomes: string[] = [];
async function attempt(name: string, f: () => Promise<unknown>) {
  try {
    await f();
    outcomes.push(`${name}:ok`);
  } catch {
    outcomes.push(`${name}:refused`);
  }
}
setTimeout(async () => {
  await attempt("number", () => Storage.setItem("n", 1 as unknown as string));
  await attempt("nokey", () => (Storage.getItem as () => Promise<unknown>)());
  Storage.setItem("a", "1");
  Storage.setItem("b", "2");
  await Storage.clear();
  const cleared = (await Storage.getAllKeys()).length;
  await Storage.setItem("ключ", "值😀");
  shown.set(`done ${outcomes.join(" ")} ${cleared} ${await Storage.getItem("ключ")}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

#[test]
fn js_arguments_are_checked_and_clear_empties() {
    let data = scratch("edges");
    let (engine, rx) = start("edges", EDGES, None, &data);
    assert_eq!(
        shown(&rx, "done"),
        "done number:refused nokey:refused 0 值😀"
    );
    drop(engine);
    let file = std::fs::read_to_string(data.join("storage.json")).unwrap();
    assert_eq!(file, r#"{"ключ":"值😀"}"#);
}

#[derive(Clone, PartialEq, Props)]
struct EditorProps {}

/// Every function Rust has for the app's storage.
#[component]
fn Editor(_props: EditorProps) -> impl IntoElement {
    set_storage_item("a", "1").unwrap();
    set_storage_item("b", "2").unwrap();
    remove_storage_item("a");
    remove_storage_item("missing");
    let after_remove = storage_keys().join(",");
    let full = set_storage_item("big", "x".repeat(nsh_core::storage::QUOTA)).is_err();
    clear_storage();
    let after_clear = storage_keys().len();
    let missing = storage_item("b").is_none();
    text(move || format!("rust {after_remove} {full} {after_clear} {missing}"))
}

#[test]
fn rust_edits_the_storage() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: App::Rust(Root::new::<Editor>(|| EditorProps {})),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        // In memory: the platform gave none.
        storage: None,
    };
    let _engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    assert_eq!(shown(&rx, "rust"), "rust b true 0 true");
}
