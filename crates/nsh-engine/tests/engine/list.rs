//! The runtime's `List` with the engine: rows render, scrolling rebinds
//! recycled cells and `onLoadMore` reaches JS.

use std::{sync::mpsc, time::Duration};

use nsh_core::{ApproxTextEngine, MountBatch, MountItem, Size, ViewKind, geometry::px};
use nsh_engine::{Bundle, Engine, EngineConfig, MountSink};

struct Sink(mpsc::Sender<MountBatch>);

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// `List`, made in place by the compiler.
const APP: &str = r#"
import { List, mount, signal, Text, View } from "nsh";
const rows = signal(Array.from({ length: 100 }, (_, i) => i));
function App() {
  return (
    <List each={rows()} estimatedItemSize={50} style={{ flex: 1 }}
          onLoadMore={() => rows.update((r) => [...r, ...Array.from({ length: 100 }, (_, i) => r.length + i)])}>
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
            "mount, signal, Text, View } from \"nsh\";",
            "mount, signal, Text, View } from \"nsh\";\nconst Rows = List;",
        );
    assert!(app.contains("<Rows "));
    scrolls_with_recycled_cells(&app);
}

/// Bundles `app` and starts it on a 320 x 500 viewport: the engine, its
/// batches and the app's directory (the caller removes it).
fn start(app: &str) -> (Engine, mpsc::Receiver<MountBatch>, std::path::PathBuf) {
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
        ui_sets: vec![nsh_base::registry().into()],
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
        websocket: None,
        filesystem: None,
        assets: None,
        environment: Default::default(),
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
    let engine = Engine::start(config, || ApproxTextEngine, Sink(tx)).unwrap();
    (engine, rx, dir)
}

fn scrolls_with_recycled_cells(app: &str) {
    let (engine, rx, dir) = start(app);
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

    // Near the end: `onLoadMore` appends rows; the content grows.
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
    assert!(grew, "onLoadMore did not extend the list");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `List` in its parent's viewport: no scroll view of its own, its
/// rows follow the page's scroll view around it.
#[test]
fn a_list_in_a_pages_viewport_follows_the_page() {
    let app = r#"
import { List, mount, Scroll, Text, View } from "nsh";
const rows = Array.from({ length: 100 }, (_, i) => i);
function App() {
  return (
    <Scroll style={{ flex: 1 }}>
      <View style={{ height: 200 }}><Text>header</Text></View>
      <List viewport="parent" each={rows} estimatedItemSize={50}>
        {(row) => <View style={{ height: 50 }}><Text>{`row ${row()}`}</Text></View>}
      </List>
    </Scroll>
  );
}
mount(App);
"#;
    let (engine, rx, dir) = start(app);
    let mut items = Vec::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(500)) {
        items.extend(batch.items);
    }
    let scrolls: Vec<u32> = items
        .iter()
        .filter_map(|i| match i {
            MountItem::Create {
                id,
                kind: ViewKind::Scroll,
            } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(scrolls.len(), 1, "the page's scroll view only");
    let page = scrolls[0];
    let shown: Vec<String> = items
        .iter()
        .filter_map(|i| match i {
            MountItem::Text { text, .. } => Some(text.text.clone()),
            _ => None,
        })
        .collect();
    // From the list's start, 200 down, to 500 below the viewport's end:
    // rows 0 to 16.
    assert!(shown.contains(&"row 16".to_owned()), "{shown:?}");
    assert!(!shown.contains(&"row 17".to_owned()), "{shown:?}");
    // The page's content covers the whole list.
    assert!(items.iter().any(|i| matches!(
        i,
        MountItem::ScrollContent { id, size } if *id == page && size.height == px(200.0 + 100.0 * 50.0)
    )));
    engine
        .control()
        .scroll(page, nsh_core::geometry::point(px(0.0), px(2000.0)), 0);
    let mut shown = Vec::new();
    let mut created = 0;
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(500)) {
        shown.extend(texts(&batch));
        created += cells_created(&batch);
    }
    // 1800 into the list: rows 26 to 56, in recycled cells.
    assert!(shown.contains(&"row 40".to_owned()), "{shown:?}");
    assert!(shown.contains(&"row 56".to_owned()), "{shown:?}");
    assert!(!shown.contains(&"row 57".to_owned()), "{shown:?}");
    assert!(created <= 14, "created {created} text views");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The props `List` does not use itself reach its scroll view, as they do
/// a `<Scroll>`'s: the test id and accessibility (UI tests find the list),
/// classes, touch props and layout events; and they follow their signals.
/// (Imported from `nsh/list`, where `List` was first: it still is.)
#[test]
fn list_passes_host_props_to_its_scroll_view() {
    let app = r#"
import { mount, signal, Text, View } from "nsh";
import { List } from "nsh/list";
const label = signal("Feed");
const size = signal("none");
function App() {
  return (
    <View style={{ flex: 1 }}>
      <Text>{`size ${size()}`}</Text>
      <List each={[1, 2, 3]} testID="feed" accessibilityLabel={label()} role="list"
            accessibilityHint="Posts" class="bg-red-500" pointerEvents="box-none" style={{ flex: 1 }}
            onLayout={(e) => { size.set(`${e.size.width}x${e.size.height}`); label.set("Feed 2"); }}>
        {(row) => <View style={{ height: 50 }}><Text>{`row ${row()}`}</Text></View>}
      </List>
    </View>
  );
}
mount(App);
"#;
    let (_engine, rx, dir) = start(app);
    let mut items = Vec::new();
    let mut shown = Vec::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_secs(2)) {
        shown.extend(texts(&batch));
        items.extend(batch.items);
        if shown.iter().any(|t| t.starts_with("size 320")) {
            break;
        }
    }
    let list = items
        .iter()
        .find_map(|i| match i {
            MountItem::Create {
                id,
                kind: ViewKind::Scroll,
            } => Some(*id),
            _ => None,
        })
        .expect("a scroll view");
    let props = items
        .iter()
        .filter_map(|i| match i {
            MountItem::ViewProps { id, props } if *id == list => Some(props),
            _ => None,
        })
        .next_back()
        .expect("the list's view props");
    assert_eq!(props.test_id.as_deref(), Some("feed"));
    assert_eq!(props.accessibility.hint.as_deref(), Some("Posts"));
    assert_eq!(props.accessibility.role, nsh_core::AccessibilityRole::List);
    assert_eq!(props.pointer_events, nsh_core::PointerEvents::BoxNone);
    // The label follows its signal (set by `onLayout`).
    let labels: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            MountItem::ViewProps { id, props } if *id == list => props.accessibility.label.clone(),
            MountItem::AccessibilityLabel { id, label } if *id == list => label.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(labels.first().map(|l| &**l), Some("Feed"), "{labels:?}");
    assert_eq!(labels.last().map(|l| &**l), Some("Feed 2"), "{labels:?}");
    // The class styles the scroll view.
    let red = items.iter().any(|i| {
        matches!(i, MountItem::ViewStyle { id, style }
            if *id == list && style.background_color != nsh_core::Color::TRANSPARENT)
    });
    assert!(red, "the class did not style the list");
    // `onLayout` reached JS with the list's size (under a 17 pt text).
    assert!(shown.iter().any(|t| t == "size 320x483"), "{shown:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
