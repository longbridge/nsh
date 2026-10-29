//! `Tree`: a tree of items for screen readers (each with its level,
//! expanded and selected states); a press selects a row and expands or
//! collapses a folder; the tree is one stop of the keyboard focus whose
//! arrows move the selection past disabled items (Right and Left expand,
//! collapse, and go to the first child and the parent), Home and End go to
//! the ends, Return and Space expand and collapse, letters select by
//! typeahead; the state is the app's when it sets it; `revealItem`
//! expands what is above an item; and a key press or an expansion costs
//! the rows it concerns, however many items there are. On macOS, where Tab
//! stops at the tree.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use nsh_base::{TreeApi, TreeEntry, TreeItem, button, tree};
use nsh_core::{MountItem, Size, element::Root, platform::Os, prelude::*};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

/// src (lib.rs, ui (tree.rs, list.rs)), docs (guide.md, old.md disabled),
/// a disabled folder `vendor` (dep.rs), README.md.
fn files() -> Vec<TreeItem> {
    vec![
        TreeItem::new("src", "src")
            .child(TreeItem::new("src/lib.rs", "lib.rs"))
            .child(
                TreeItem::new("src/ui", "ui")
                    .child(TreeItem::new("src/ui/tree.rs", "tree.rs"))
                    .child(TreeItem::new("src/ui/list.rs", "list.rs")),
            ),
        TreeItem::new("docs", "docs")
            .child(TreeItem::new("docs/guide.md", "guide.md"))
            .child(TreeItem::new("docs/old.md", "old.md").disabled(true)),
        TreeItem::new("vendor", "vendor")
            .child(TreeItem::new("vendor/dep.rs", "dep.rs"))
            .disabled(true),
        TreeItem::new("README.md", "README.md"),
    ]
}

#[derive(Clone, PartialEq, Props)]
struct HarnessProps {
    items: Vec<TreeItem>,
    /// The app sets `expanded` and `selected`.
    controlled: bool,
    /// The app follows the events (when it controls).
    follow: bool,
    disabled: bool,
    /// A `renderItem` drawing `▸` / `▾` before a folder's label.
    custom: bool,
}

fn props() -> HarnessProps {
    HarnessProps {
        items: files(),
        controlled: false,
        follow: true,
        disabled: false,
        custom: false,
    }
}

/// A button `Before`, the tree (`src` expanded at first) in a 300 × 400
/// box, a button `After`; what the app heard as `log:…`, the selection
/// and the expanded items as `selected:…` and `expanded:…` (the app's
/// when it controls, else the events'). F5 reveals `src/ui/tree.rs`, F6
/// `nope`; F7 toggles `disabled`.
#[component]
fn Harness(props: HarnessProps) -> impl IntoElement {
    let selected = signal(None::<SharedString>);
    let expanded = signal(vec![SharedString::from("src")]);
    let log = signal(Vec::<String>::new());
    let pressed = signal("none");
    let disabled = signal(props.disabled);
    let push = move |s: String| log.update(|l| l.push(s));
    let r = component_ref::<nsh_base::Tree>();
    on_key("f5", move |_| {
        if let Some(api) = r.api::<TreeApi>() {
            api.reveal_item("src/ui/tree.rs".into());
        }
    });
    on_key("f6", move |_| {
        if let Some(api) = r.api::<TreeApi>() {
            api.reveal_item("nope".into());
        }
    });
    on_key("f7", move |_| disabled.update(|d| *d = !*d));
    let follow = props.follow;
    let mut el = tree()
        .items(props.items.clone())
        .accessibility_label("Files")
        .disabled(disabled)
        .on_expanded_change(move |ids: &Vec<SharedString>| {
            push(format!("expanded={}", ids.join(",")));
            if follow {
                expanded.set(ids.clone());
            }
        })
        .on_selected_change(move |id: &SharedString| {
            push(format!("selected={id}"));
            if follow {
                selected.set(Some(id.clone()));
            }
        })
        .on_expand(move |id: &SharedString| push(format!("expand={id}")))
        .on_collapse(move |id: &SharedString| push(format!("collapse={id}")))
        .component_ref(&r)
        .w(px(300.0))
        .h(px(400.0));
    if props.controlled {
        el = el
            .expanded(move || Some(expanded.get()))
            .selected(move || selected.get());
    } else {
        el = el.default_expanded(vec![SharedString::from("src")]);
    }
    if props.custom {
        el = el.render_item(|e: &TreeEntry| {
            text(format!(
                "{}{}@{}",
                if e.folder { "▸" } else { "" },
                e.label,
                e.depth
            ))
        });
    }
    view()
        .child(
            button()
                .accessibility_label("Before")
                .on_press(move || pressed.set("before"))
                .child(text("Before")),
        )
        .child(el)
        .child(
            button()
                .accessibility_label("After")
                .on_press(move || pressed.set("after"))
                .child(text("After")),
        )
        .child(text(move || format!("pressed:{}", pressed.get())))
        .child(text(move || format!("log:{}", log.get().join(" "))))
        .child(text(move || {
            format!(
                "selected:{}",
                selected.get().as_deref().unwrap_or("none").to_owned()
            )
        }))
        .child(text(move || {
            format!("expanded:{}", expanded.get().join(","))
        }))
}

fn launch(root: Root) -> Headless {
    let mut config = EngineConfig {
        viewport: Size::new(px(800.0), px(844.0)),
        ..EngineConfig::new(root)
    };
    config.environment.os = Os::MacOs;
    let mut app = Headless::start(config).unwrap();
    app.first_render().unwrap();
    app
}

fn start(props: HarnessProps) -> Headless {
    let mut app = launch(Root::new::<Harness>(move || props.clone()));
    app.wait_for("role:treeitem[src]").unwrap();
    app
}

/// Tab from Before into the tree.
fn enter(app: &mut Headless) {
    app.press_key("tab").unwrap();
    app.press_key("tab").unwrap();
}

fn key(app: &mut Headless, keys: &str) {
    app.press_key(keys).unwrap();
}

fn see(app: &mut Headless, text: &str) {
    app.wait_for(&format!("text:{text}")).unwrap();
}

fn log(app: &Headless) -> String {
    app.screen()
        .texts()
        .into_iter()
        .find_map(|t| t.strip_prefix("log:").map(str::to_owned))
        .unwrap_or_default()
}

fn rows(app: &Headless) -> usize {
    app.screen().count("role:treeitem").unwrap()
}

#[test]
fn screen_readers_hear_a_tree_of_items_with_levels_and_states() {
    let app = start(props());
    let screen = app.screen();
    assert_eq!(screen.count("role:tree[Files]"), Ok(1));
    // src expanded: src, lib.rs, ui, docs, vendor, README.md.
    assert_eq!(screen.count("role:treeitem"), Ok(6));
    assert_eq!(
        screen.accessibility_level("role:treeitem[src]"),
        Ok(Some(1))
    );
    assert_eq!(
        screen.accessibility_level("role:treeitem[lib.rs]"),
        Ok(Some(2))
    );
    let state = |label: &str| {
        screen
            .accessibility_state(&format!("role:treeitem[{label}]"))
            .unwrap()
    };
    assert_eq!(state("src").expanded, Some(true));
    assert_eq!(state("docs").expanded, Some(false));
    assert_eq!(state("README.md").expanded, None);
    assert!(!state("src").selected);
    assert!(!screen.enabled("role:treeitem[vendor]").unwrap());
    assert!(screen.enabled("role:treeitem[docs]").unwrap());
}

#[test]
fn a_press_selects_a_row_and_expands_or_collapses_a_folder() {
    let mut app = start(props());
    app.press("role:treeitem[docs]").unwrap();
    app.wait_for("role:treeitem[guide.md]").unwrap();
    assert!(app.screen().selected("role:treeitem[docs]").unwrap());
    assert_eq!(
        log(&app),
        "selected=docs expanded=src,docs expand=docs",
        "the selection, then the expanded items, then the item expanded"
    );
    assert_eq!(
        app.screen().accessibility_level("role:treeitem[guide.md]"),
        Ok(Some(2))
    );
    app.press("role:treeitem[docs]").unwrap();
    app.wait_for_gone("role:treeitem[guide.md]", Duration::from_secs(5))
        .unwrap();
    assert!(log(&app).ends_with("expanded=src collapse=docs"));
    // A leaf is selected only.
    app.press("role:treeitem[lib.rs]").unwrap();
    see(&mut app, "selected:src/lib.rs");
    assert!(app.screen().selected("role:treeitem[lib.rs]").unwrap());
    assert!(!app.screen().selected("role:treeitem[docs]").unwrap());
}

#[test]
fn a_disabled_item_takes_no_press() {
    let mut app = start(props());
    assert!(app.press("role:treeitem[vendor]").is_err());
    app.wait(Duration::from_millis(50)).unwrap();
    assert_eq!(log(&app), "");
    assert_eq!(app.screen().count("role:treeitem[dep.rs]"), Ok(0));
}

#[test]
fn up_and_down_move_the_selection_past_disabled_rows_and_around_the_ends() {
    let mut app = start(props());
    enter(&mut app);
    // Nothing selected: Down selects the first row.
    key(&mut app, "down");
    see(&mut app, "selected:src");
    for id in ["src/lib.rs", "src/ui", "docs", "README.md", "src"] {
        key(&mut app, "down");
        see(&mut app, &format!("selected:{id}"));
    }
    // Around the start, past the disabled `vendor`.
    key(&mut app, "up");
    see(&mut app, "selected:README.md");
    key(&mut app, "up");
    see(&mut app, "selected:docs");
    assert!(app.screen().selected("role:treeitem[docs]").unwrap());
}

#[test]
fn home_and_end_go_to_the_first_and_last_enabled_row() {
    let mut app = start(props());
    enter(&mut app);
    key(&mut app, "end");
    see(&mut app, "selected:README.md");
    key(&mut app, "home");
    see(&mut app, "selected:src");
}

#[test]
fn right_expands_then_enters_and_left_collapses_then_leaves() {
    let mut app = start(props());
    enter(&mut app);
    key(&mut app, "down");
    key(&mut app, "down");
    key(&mut app, "down");
    see(&mut app, "selected:src/ui");
    // A collapsed folder expands, an expanded one goes to its first child.
    key(&mut app, "right");
    app.wait_for("role:treeitem[tree.rs]").unwrap();
    see(&mut app, "selected:src/ui");
    key(&mut app, "right");
    see(&mut app, "selected:src/ui/tree.rs");
    // A leaf: Right does nothing, Left goes to the parent.
    key(&mut app, "right");
    app.wait(Duration::from_millis(50)).unwrap();
    see(&mut app, "selected:src/ui/tree.rs");
    key(&mut app, "left");
    see(&mut app, "selected:src/ui");
    // An expanded folder collapses, then goes to its parent.
    key(&mut app, "left");
    app.wait_for_gone("role:treeitem[tree.rs]", Duration::from_secs(5))
        .unwrap();
    key(&mut app, "left");
    see(&mut app, "selected:src");
    key(&mut app, "left");
    app.wait_for_gone("role:treeitem[lib.rs]", Duration::from_secs(5))
        .unwrap();
    see(&mut app, "expanded:");
}

#[test]
fn return_and_space_expand_and_collapse_the_selected_folder() {
    let mut app = start(props());
    enter(&mut app);
    key(&mut app, "end");
    key(&mut app, "up");
    see(&mut app, "selected:docs");
    key(&mut app, "enter");
    app.wait_for("role:treeitem[guide.md]").unwrap();
    key(&mut app, "space");
    app.wait_for_gone("role:treeitem[guide.md]", Duration::from_secs(5))
        .unwrap();
    assert!(log(&app).ends_with("expand=docs expanded=src collapse=docs"));
}

#[test]
fn typeahead_selects_the_next_row_whose_label_starts_with_what_was_typed() {
    let mut app = start(props());
    enter(&mut app);
    key(&mut app, "r");
    see(&mut app, "selected:README.md");
    // `l` after a pause: lib.rs, then list.rs is hidden, so lib.rs again.
    app.wait(Duration::from_millis(1100)).unwrap();
    key(&mut app, "l");
    see(&mut app, "selected:src/lib.rs");
}

#[test]
fn the_tree_is_one_stop_and_tab_comes_back_to_the_selection() {
    let mut app = start(props());
    enter(&mut app);
    key(&mut app, "down");
    key(&mut app, "down");
    see(&mut app, "selected:src/lib.rs");
    // One Tab leaves the tree, whatever row is selected.
    key(&mut app, "tab");
    key(&mut app, "space");
    see(&mut app, "pressed:after");
    // Back in, the keys go on from the selection.
    key(&mut app, "shift+tab");
    key(&mut app, "down");
    see(&mut app, "selected:src/ui");
}

#[test]
fn a_controlled_state_is_the_apps() {
    let mut app = start(HarnessProps {
        controlled: true,
        follow: false,
        ..props()
    });
    enter(&mut app);
    key(&mut app, "down");
    app.wait_until("the selection reported", Duration::from_secs(5), |s| {
        s.texts().contains(&"log:selected=src")
    })
    .unwrap();
    // Not followed: nothing is selected, and Down reports the same row.
    assert!(!app.screen().selected("role:treeitem[src]").unwrap());
    app.press("role:treeitem[docs]").unwrap();
    app.wait(Duration::from_millis(50)).unwrap();
    assert!(log(&app).ends_with("selected=docs expanded=src,docs expand=docs"));
    assert_eq!(app.screen().count("role:treeitem[guide.md]"), Ok(0));
}

#[test]
fn a_disabled_tree_takes_no_press_and_is_no_stop() {
    let mut app = start(HarnessProps {
        disabled: true,
        ..props()
    });
    assert!(!app.screen().enabled("role:treeitem[src]").unwrap());
    assert!(app.press("role:treeitem[docs]").is_err());
    // Before, then past the tree to After.
    enter(&mut app);
    key(&mut app, "space");
    see(&mut app, "pressed:after");
    assert_eq!(log(&app), "");
    // Enabled again.
    key(&mut app, "f7");
    app.press("role:treeitem[docs]").unwrap();
    see(&mut app, "selected:docs");
}

#[test]
fn reveal_item_expands_the_items_above_it() {
    let mut app = start(props());
    key(&mut app, "f5");
    app.wait_for("role:treeitem[tree.rs]").unwrap();
    assert!(log(&app).ends_with("expanded=src,src/ui expand=src/ui"));
    // An unknown id does nothing.
    key(&mut app, "f6");
    app.wait(Duration::from_millis(50)).unwrap();
    assert!(log(&app).ends_with("expand=src/ui"));
}

#[test]
fn render_item_draws_each_row_with_its_depth() {
    let mut app = start(HarnessProps {
        custom: true,
        ..props()
    });
    see(&mut app, "▸src@0");
    see(&mut app, "lib.rs@1");
    // The row keeps its label and role for screen readers.
    assert_eq!(app.screen().count("role:treeitem[lib.rs]"), Ok(1));
}

/// The `renderItem` calls of each app (by its `run`): tests run side by
/// side, each app on an engine thread of its own.
static RENDERS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// A new run's id, its count at zero.
fn new_run() -> u32 {
    let mut renders = RENDERS.lock().unwrap();
    renders.push(0);
    (renders.len() - 1) as u32
}

fn renders_of(run: u32) -> usize {
    RENDERS.lock().unwrap()[run as usize]
}

#[derive(Clone, PartialEq, Props)]
struct ManyProps {
    folders: usize,
    files: usize,
    /// The ids expanded at first.
    open: Vec<String>,
    /// The tree's height.
    height: f32,
    /// Where its `renderItem` calls count ([`RENDERS`]).
    run: u32,
}

fn sized(folders: usize, files: usize) -> ManyProps {
    ManyProps {
        folders,
        files,
        open: vec!["f0".into()],
        height: 300.0,
        run: new_run(),
    }
}

/// `folders` folders of `files` files each, `open` expanded, rows drawn by
/// a `renderItem` that counts its calls.
#[component]
fn Many(props: ManyProps) -> impl IntoElement {
    let items: Vec<TreeItem> = (0..props.folders)
        .map(|f| {
            TreeItem::new(format!("f{f}"), format!("Folder {f}")).children(
                (0..props.files).map(|i| TreeItem::new(format!("f{f}/{i}"), format!("File {i}"))),
            )
        })
        .collect();
    let selected = signal(SharedString::from("none"));
    let run = props.run as usize;
    view()
        .child(
            tree()
                .items(items)
                .default_expanded(
                    props
                        .open
                        .iter()
                        .map(|id| SharedString::from(id.clone()))
                        .collect::<Vec<_>>(),
                )
                .on_selected_change(move |id: &SharedString| selected.set(id.clone()))
                .render_item(move |e: &TreeEntry| {
                    RENDERS.lock().unwrap()[run] += 1;
                    text(e.label.clone())
                })
                .h(px(props.height)),
        )
        .child(text(move || format!("selected:{}", selected.get())))
}

/// What a step sent: mount items, views made, views deleted, and calls of
/// `renderItem`.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Cost {
    items: usize,
    made: usize,
    deleted: usize,
    renders: usize,
}

/// Measures what the steps given to it cost.
type Measure = Box<dyn Fn(&mut Headless, &dyn Fn(&mut Headless)) -> Cost>;

/// The app of `props` and a function that measures what the steps given
/// to it cost.
fn measured(props: ManyProps) -> (Headless, Measure) {
    let run = props.run;
    let mut app = launch(Root::new::<Many>(move || props.clone()));
    app.wait_for("role:treeitem[Folder 0]").unwrap();
    let counts = Arc::new([const { AtomicUsize::new(0) }; 3]);
    let into = counts.clone();
    app.observe(move |_, batch| {
        into[0].fetch_add(batch.items.len(), Ordering::Relaxed);
        for item in &batch.items {
            match item {
                MountItem::Create { .. } => into[1].fetch_add(1, Ordering::Relaxed),
                MountItem::Delete { .. } => into[2].fetch_add(1, Ordering::Relaxed),
                _ => 0,
            };
        }
    });
    let measure = move |app: &mut Headless, steps: &dyn Fn(&mut Headless)| {
        app.idle(Duration::from_millis(50)).unwrap();
        for c in counts.iter() {
            c.store(0, Ordering::Relaxed);
        }
        let before = renders_of(run);
        steps(app);
        app.idle(Duration::from_millis(50)).unwrap();
        Cost {
            items: counts[0].load(Ordering::Relaxed),
            made: counts[1].load(Ordering::Relaxed),
            deleted: counts[2].load(Ordering::Relaxed),
            renders: renders_of(run) - before,
        }
    };
    (app, Box::new(measure))
}

/// What one Down costs, from `f0/0` to `f0/1`.
fn cost_of_a_key(folders: usize, files: usize) -> Cost {
    let (mut app, measure) = measured(sized(folders, files));
    app.press_key("tab").unwrap();
    app.press_key("down").unwrap();
    app.press_key("down").unwrap();
    app.wait_for("text:selected:f0/0").unwrap();
    measure(&mut app, &|app| {
        app.press_key("down").unwrap();
        app.wait_for("text:selected:f0/1").unwrap();
    })
}

/// What one press costs, selecting `File 3` while `File 1` is.
fn cost_of_a_press(folders: usize, files: usize) -> Cost {
    let (mut app, measure) = measured(sized(folders, files));
    app.press("role:treeitem[File 1]").unwrap();
    app.wait_for("text:selected:f0/1").unwrap();
    measure(&mut app, &|app| {
        app.press("role:treeitem[File 3]").unwrap();
        app.wait_for("text:selected:f0/3").unwrap();
    })
}

/// The most mount items one Down sends: the states of the row left and of the
/// row reached (accessibility and projected), no other row's (measured,
/// for 18 items and for 10 200).
const KEY_ITEMS: usize = 3;

/// The most mount items one press on a row sends, for any number of items
/// (measured, for 18 items and for 10 200).
const PRESS_ITEMS: usize = 8;

#[test]
fn a_key_press_changes_the_row_left_and_the_row_reached_only() {
    let few = cost_of_a_key(3, 5);
    let many = cost_of_a_key(200, 50);
    assert_eq!(
        few, many,
        "the cost of a key press does not grow with the items"
    );
    assert_eq!(
        (few.made, few.deleted),
        (0, 0),
        "a key press makes and deletes no view"
    );
    assert_eq!(few.renders, 0, "a key press calls no `renderItem`");
    assert!(few.items <= KEY_ITEMS, "{} mount items", few.items);
}

#[test]
fn a_press_changes_the_row_left_and_the_row_pressed_only() {
    let few = cost_of_a_press(3, 5);
    let many = cost_of_a_press(200, 50);
    assert_eq!(
        few, many,
        "the cost of a press does not grow with the items"
    );
    assert_eq!(
        (few.made, few.deleted),
        (0, 0),
        "a press on a file makes and deletes no view"
    );
    assert_eq!(few.renders, 0, "a press calls no `renderItem`");
    assert!(few.items <= PRESS_ITEMS, "{} mount items", few.items);
}

/// What expanding `Folder 1` (of `files` files) costs, in a tree tall
/// enough for every row.
fn cost_of_expanding(files: usize) -> Cost {
    let (mut app, measure) = measured(ManyProps {
        open: Vec::new(),
        height: 800.0,
        ..sized(3, files)
    });
    measure(&mut app, &|app| {
        app.press("role:treeitem[Folder 1]").unwrap();
        app.wait_for("role:treeitem[File 0]").unwrap();
    })
}

#[test]
fn expanding_makes_the_rows_of_the_children_only() {
    let three = cost_of_expanding(3);
    let six = cost_of_expanding(6);
    // Each child's row is drawn once; no other row is drawn again.
    assert_eq!((three.renders, six.renders), (3, 6));
    // The views made are the children's rows, as many for each: the
    // folder's row and the others keep theirs.
    assert!(three.made > 0);
    assert_eq!(
        six.made,
        2 * three.made,
        "the views made grow with the children only"
    );
    assert_eq!((three.deleted, six.deleted), (0, 0), "no row is deleted");
}

#[test]
fn a_large_tree_makes_only_the_rows_in_view() {
    let props = sized(200, 50);
    let mut app = launch(Root::new::<Many>(move || props.clone()));
    app.wait_for("role:treeitem[Folder 0]").unwrap();
    // 10 200 items, 249 rows shown (Folder 0's 50 files): only those near
    // the 300-point viewport are made.
    let shown = rows(&app);
    assert!(shown < 60, "{shown} rows made");
    // Collapsing Folder 0 shows the folders, as few.
    app.press("role:treeitem[Folder 0]").unwrap();
    app.wait_for_gone("role:treeitem[File 0]", Duration::from_secs(5))
        .unwrap();
    let after = rows(&app);
    assert!(after < 60, "{after} rows made");
}
