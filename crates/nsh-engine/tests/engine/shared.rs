//! `nsh/shared`: a JS app follows keys, prefixes and events of the store
//! its engine was given (written from another thread, a transaction's keys
//! seen together), and what its JS writes is in that store for Rust.

use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, MountBatch, MountItem, Size,
    element::EngineContext,
    geometry::px,
    shared::{SharedStore, SharedValue},
};
use nsh_engine::{Bundle, Engine, EngineConfig};

use crate::ChannelSink;

fn bundle(app: &str) -> Bundle {
    static APPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = crate::temporary_root().join(format!(
        "nsh-shared-{}-{}",
        std::process::id(),
        APPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
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
    // SAFETY: compiled just now by this runtime.
    unsafe { Bundle::bytecode(bytecode) }
}

fn start(app: &str, store: &SharedStore) -> (Engine, Receiver<MountBatch>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        viewport: Size::new(px(400.0), px(600.0)),
        context: EngineContext::default().with(store.clone()),
        ..EngineConfig::new(bundle(app))
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// Reads batches until a shown text contains every one of `parts`; returns
/// every text shown on the way (the last one has them). Panics after a few
/// seconds.
fn wait_for(rx: &Receiver<MountBatch>, parts: &[&str]) -> Vec<String> {
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let Ok(batch) = rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        for item in batch.items {
            if let MountItem::Text { text, .. } = item {
                seen.push(text.text.to_string());
            }
        }
        if seen
            .last()
            .is_some_and(|last| parts.iter().all(|p| last.contains(p)))
        {
            return seen;
        }
    }
    panic!("no text with {parts:?}; seen: {seen:?}");
}

const FOLLOW: &str = r#"import { mount, signal, Text } from "nsh";
import { signal as shared, prefix, on } from "nsh/shared";

const key = shared<number>("k");
const rows = prefix<number>("row/");
const log = signal<string[]>([]);
on<string>("ping", (v) => log.set([...log(), `ping ${v}`]));

mount(() => (
  <Text>{`k=${key()} rows=${Object.keys(rows()).sort().map((r) => `${r}:${rows()[r]}`).join(",")} log=${log().join(";")}`}</Text>
));
"#;

#[test]
fn a_js_app_follows_what_other_threads_write() {
    let store = SharedStore::new();
    // A value there before the app runs is its first.
    store.set("row/z", 26.0);
    let (_engine, rx) = start(FOLLOW, &store);
    wait_for(&rx, &["k=undefined", "rows=row/z:26", "log="]);

    let writer = store.clone();
    std::thread::spawn(move || {
        writer.set("k", 7.0);
    })
    .join()
    .unwrap();
    wait_for(&rx, &["k=7 "]);

    // One transaction: its keys are seen together, never a mix.
    let writer = store.clone();
    std::thread::spawn(move || {
        writer.transaction(|tx| {
            tx.set("row/a", 1.0);
            tx.set("row/b", 2.0);
            tx.set("k", 8.0);
        });
    })
    .join()
    .unwrap();
    let seen = wait_for(&rx, &["k=8 ", "row/a:1,row/b:2,row/z:26"]);
    for text in &seen {
        assert_eq!(
            text.contains("k=8 "),
            text.contains("row/b:2"),
            "a transaction is seen whole: {text}"
        );
    }

    store.delete("row/a");
    wait_for(&rx, &["rows=row/b:2,row/z:26"]);

    store.emit("ping", "hi");
    store.emit("ping", "again");
    wait_for(&rx, &["log=ping hi;ping again"]);
}

const WRITE: &str = r#"import { mount, Text } from "nsh";
import { set, remove, transaction, get, version } from "nsh/shared";

set("out/number", 1.5);
set("out/object", { a: 1, b: [true, "s", null], c: undefined, d: { e: "f" } });
set("out/bytes", new Uint8Array([1, 2, 3]));
set("out/gone", "x");
transaction((tx) => {
  tx.set("out/tx1", "one");
  tx.set("out/tx2", "two");
  tx.remove("out/gone");
});
try {
  transaction((tx) => {
    tx.set("out/never", 1);
    throw new Error("no");
  });
} catch (e) {
  set("out/threw", (e as Error).message);
}
try {
  set("out/fn", () => 1);
} catch (e) {
  set("out/error", (e as Error).name);
}
set("out/read", `${typeof get("out/number")} ${version() > 0}`);
remove("out/missing");
set("out/done", true);

mount(() => <Text>written</Text>);
"#;

#[test]
fn what_js_writes_is_in_the_store_for_rust() {
    let store = SharedStore::new();
    let (_engine, _rx) = start(WRITE, &store);
    let deadline = Instant::now() + Duration::from_secs(5);
    while store.get("out/done").is_none() {
        assert!(Instant::now() < deadline, "the app wrote nothing");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(store.get("out/number").and_then(|v| v.as_f64()), Some(1.5));
    let object = store.get("out/object").expect("an object");
    assert_eq!(object.get("a").and_then(SharedValue::as_f64), Some(1.0));
    let list = object
        .get("b")
        .and_then(SharedValue::as_list)
        .expect("a list");
    assert_eq!(list[0].as_bool(), Some(true));
    assert_eq!(list[1].as_str(), Some("s"));
    assert!(matches!(list[2], SharedValue::Null));
    assert!(object.get("c").is_none(), "an undefined field is left out");
    assert_eq!(
        object
            .get("d")
            .and_then(|d| d.get("e"))
            .and_then(SharedValue::as_str),
        Some("f")
    );
    match store.get("out/bytes") {
        Some(SharedValue::Bytes(bytes)) => assert_eq!(&*bytes, &[1, 2, 3]),
        other => panic!("bytes expected, got {other:?}"),
    }
    assert_eq!(
        store.get("out/tx1").as_ref().and_then(SharedValue::as_str),
        Some("one")
    );
    assert_eq!(
        store.get("out/tx2").as_ref().and_then(SharedValue::as_str),
        Some("two")
    );
    assert!(
        store.get("out/gone").is_none(),
        "removed in the transaction"
    );
    assert!(
        store.get("out/never").is_none(),
        "a throwing transaction commits nothing"
    );
    assert_eq!(
        store
            .get("out/threw")
            .as_ref()
            .and_then(SharedValue::as_str),
        Some("no")
    );
    assert_eq!(
        store
            .get("out/error")
            .as_ref()
            .and_then(SharedValue::as_str),
        Some("TypeError")
    );
    assert_eq!(
        store.get("out/read").as_ref().and_then(SharedValue::as_str),
        Some("number true")
    );
}
