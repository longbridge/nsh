//! The runtime's `List` with the engine: rows render, scrolling rebinds
//! recycled cells and `onEndReached` reaches JS.

use std::{sync::mpsc, time::Duration};

use nsh_core::{ApproxTextEngine, MountBatch, MountItem, Size, ViewKind, geometry::px};
use nsh_engine::{Bundle, Engine, EngineConfig, MountSink};

struct Sink(mpsc::Sender<MountBatch>);

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// `List` (`nsh/list`), made in place by the compiler.
const APP: &str = r#"
import { mount, signal, Text, View } from "nsh";
import { List } from "nsh/list";
const rows = signal(Array.from({ length: 100 }, (_, i) => i));
function App() {
  return (
    <List each={rows()} estimatedItemSize={50} style={{ flex: 1 }}
          onEndReached={() => rows.update((r) => [...r, ...Array.from({ length: 100 }, (_, i) => r.length + i)])}>
      {(row) => <View style={{ height: 50 }}><Text>{`row ${row()}`}</Text></View>}
    </List>
  );
}
mount(App);
"#;

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

fn cells_created(batch: &MountBatch) -> usize {
    // Each cell is a View whose child is the row View: count rows via text views.
    batch
        .items
        .iter()
        .filter(|i| {
            matches!(
                i,
                MountItem::Create {
                    kind: ViewKind::Text,
                    ..
                }
            )
        })
        .count()
}

#[test]
fn list_scrolls_with_recycled_cells() {
    scrolls_with_recycled_cells(APP);
}

/// `List` as a value (`const L = List; <L>`) is the same component as the
/// tag the compiler makes in place.
#[test]
fn list_as_a_value_scrolls_with_recycled_cells() {
    let app = APP
        .replace("<List ", "<Rows ")
        .replace("</List>", "</Rows>")
        .replace(
            "import { List } from \"nsh/list\";",
            "import { List } from \"nsh/list\";\nconst Rows = List;",
        );
    assert!(app.contains("<Rows "));
    scrolls_with_recycled_cells(&app);
}

fn scrolls_with_recycled_cells(app: &str) {
    static APPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "nsh-list-{}-{}",
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
        ui_sets: vec![nsh_ui_mobile::registry().into()],
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode(&bundle.code, false).unwrap();

    let (tx, rx) = mpsc::channel();
    let config = EngineConfig {
        // SAFETY: compiled just now by this runtime.
        app: nsh_engine::App::Js(unsafe { Bundle::bytecode(bytecode) }),
        viewport: Size::new(px(320.0), px(500.0)),
        trace: false,
        network: None,
        assets: None,
        environment: Default::default(),
        header_heights: (nsh_core::Points(44.0), nsh_core::Points(96.0)),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        data_dir: None,
        storage: None,
    };
    let engine = Engine::start(config, || ApproxTextEngine, Sink(tx)).unwrap();
    let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // 500 pt viewport + 500 pt overscan at 50 pt per row = 21 rows.
    let first_texts = texts(&first);
    assert_eq!(first_texts.len(), 21, "{first_texts:?}");
    assert_eq!(first_texts[0], "row 0");
    let list = first
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Create {
                id,
                kind: ViewKind::Scroll,
            } => Some(*id),
            _ => None,
        })
        .unwrap();

    // Scroll to row 40: rows 30..=60 are needed; cells are recycled, so the
    // update re-renders text in existing views instead of creating them.
    engine.control().scroll(
        list,
        nsh_core::geometry::point(nsh_core::geometry::px(0.0), nsh_core::geometry::px(2000.0)),
        u32::MAX,
    );
    let batch = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let shown = texts(&batch);
    assert!(shown.contains(&"row 40".to_owned()), "{shown:?}");
    assert!(
        cells_created(&batch) <= 10,
        "created {} text views",
        cells_created(&batch)
    );

    // Near the end: `onEndReached` appends rows; the content grows.
    engine.control().scroll(
        list,
        nsh_core::geometry::point(nsh_core::geometry::px(0.0), nsh_core::geometry::px(4600.0)),
        u32::MAX,
    );
    let mut grew = false;
    while let Ok(batch) = rx.recv_timeout(Duration::from_secs(2)) {
        grew |= batch.items.iter().any(
            |i| matches!(i, MountItem::ScrollContent { size, .. } if size.height >= px(10_000.0)),
        );
        if grew {
            break;
        }
    }
    assert!(grew, "onEndReached did not extend the list");
    let _ = std::fs::remove_dir_all(&dir);
}
