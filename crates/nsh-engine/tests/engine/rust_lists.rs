//! Rust components' lists on the engine: a list's own header, footer,
//! empty view and separators, viewable rows, a `ListView` with rows of a
//! Rust component (`ComponentElement::items`) or of the app, and the
//! methods of a nested component called through a `ComponentRef`.

use std::{collections::HashMap, time::Duration};

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, EventKind, EventPayload, MountBatch, MountItem, PayloadValue, Size,
    ui::{Root, prelude::*},
};
use nsh_engine::{App, Engine, EngineConfig, Environment};
use nsh_ui_mobile::{ListView, ListViewProps, Stepper, StepperProps};

use crate::ChannelSink;

fn start(root: Root) -> (Engine, Receiver<MountBatch>) {
    let (tx, rx) = crossbeam_channel::unbounded();
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
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// What the batches built: texts, parents, kinds, and the list cells'
/// slots.
#[derive(Default)]
struct Tree {
    texts: Vec<(u32, String)>,
    parents: HashMap<u32, u32>,
    kinds: HashMap<u32, String>,
    /// Every text shown, in order.
    history: Vec<String>,
    commands: Vec<(u32, nsh_core::Command)>,
}

impl Tree {
    fn apply(&mut self, batch: &MountBatch) {
        for item in &batch.items {
            match item {
                MountItem::Text { id, text, .. } => {
                    self.history.push(text.text.clone());
                    self.texts.retain(|t| t.0 != *id);
                    self.texts.push((*id, text.text.clone()));
                }
                MountItem::Insert { parent, child, .. } => {
                    self.parents.insert(*child, *parent);
                }
                MountItem::Create { id, kind, .. } => {
                    self.kinds.insert(*id, format!("{kind:?}"));
                }
                MountItem::Delete { id } => {
                    self.texts.retain(|t| t.0 != *id);
                    self.parents.remove(id);
                }
                MountItem::Command { id, command } => self.commands.push((*id, command.clone())),
                _ => {}
            }
        }
    }

    fn drain(&mut self, rx: &Receiver<MountBatch>) {
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
            self.apply(&batch);
        }
    }

    fn find(&self, label: &str) -> Option<u32> {
        self.texts.iter().find(|t| t.1 == label).map(|t| t.0)
    }

    fn shows(&self, label: &str) -> bool {
        self.find(label).is_some()
    }

    /// The list `id` is in (its nearest scroll view) and its cell there.
    fn cell(&self, mut id: u32) -> Option<(u32, u32)> {
        loop {
            let parent = *self.parents.get(&id)?;
            if self.kinds.get(&parent).is_some_and(|k| k == "Scroll") {
                return Some((parent, id));
            }
            id = parent;
        }
    }

    /// The depth of `id` below `ancestor` (`None`: not below it).
    fn depth(&self, mut id: u32, ancestor: u32) -> Option<usize> {
        let mut depth = 0;
        while id != ancestor {
            id = *self.parents.get(&id)?;
            depth += 1;
        }
        Some(depth)
    }
}

/// A list of its own rows with a header, a footer, an empty view and
/// separators.
#[derive(Default)]
struct Own {
    count: usize,
    header_taps: u32,
    visible: String,
    /// Emptied once at the end.
    emptied: bool,
}

impl Component for Own {
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), _: &mut Context<'_, Self>) {
        self.count = 50;
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        view()
            .flex_1()
            .child(
                virtual_list(self.count, |i, _| text(format!("row {i}")).h(px(40.0)))
                    .estimated_item_size(40.0)
                    .flex_1()
                    .header(
                        text(format!("header {}", self.header_taps)).on_press(cx.listener(
                            |this: &mut Self, _, cx| {
                                this.header_taps += 1;
                                cx.notify();
                            },
                        )),
                    )
                    .footer(text("footer"))
                    .empty(
                        text("nothing").on_press(cx.listener(|this: &mut Self, _, cx| {
                            this.count = 3;
                            cx.notify();
                        })),
                    )
                    .separator(|| view().h(px(1.0)).bg(rgb(0xeeeeee)))
                    .sticky_header_indices([0])
                    .keyboard_dismiss_mode(KeyboardDismissMode::OnDrag)
                    .end_reached_threshold(0.5)
                    .initial_scroll_index(2)
                    .minimum_view_time(Duration::from_millis(100))
                    .on_viewable_items_changed(cx.listener(
                        |this: &mut Self, rows: &ViewableItems, cx| {
                            let rows = rows.indices();
                            this.visible = format!("{}-{}", rows.start(), rows.end());
                            cx.notify();
                        },
                    ))
                    .on_end_reached(cx.listener(|this: &mut Self, _, cx| {
                        if !this.emptied {
                            this.emptied = true;
                            this.count = 0;
                            cx.notify();
                        }
                    })),
            )
            .child(text(format!("visible {}", self.visible)))
    }
}

#[test]
fn rust_lists_have_headers_footers_separators_and_empty_views() {
    let (engine, rx) = start(Root::of::<Own>());
    let mut tree = Tree::default();
    let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Their own props, set as JS sets them.
    let list = first
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Create { id, kind, .. } if format!("{kind:?}") == "Scroll" => Some(*id),
            _ => None,
        })
        .expect("a list");
    tree.apply(&first);
    tree.drain(&rx);
    for label in ["header 0", "footer", "row 0", "row 1"] {
        assert!(tree.shows(label), "{label}: {:?}", tree.texts);
    }
    assert!(!tree.shows("nothing"), "the empty view waits for no rows");
    assert!(
        tree.commands
            .iter()
            .any(|(id, c)| *id == list && matches!(c, nsh_core::Command::ScrollTo { .. })),
        "the initial row is scrolled to: {:?}",
        tree.commands
    );
    // The header and the footer are cells of the list, not rows.
    let header = tree.find("header 0").unwrap();
    let (header_list, header_cell) = tree.cell(header).expect("in a cell");
    assert_eq!(header_list, list);
    // A row and its separator share the cell.
    let row = tree.find("row 0").unwrap();
    let (_, cell) = tree.cell(row).unwrap();
    assert_ne!(cell, header_cell);
    // The header is the component's: its state changes, its node stays.
    engine
        .control()
        .dispatch_event(header, EventKind::Press, EventPayload::None);
    tree.drain(&rx);
    assert_eq!(tree.find("header 1"), Some(header), "{:?}", tree.texts);
    // Rows and their separators share the cells, but the last row's.
    let children = |cell: u32| tree.parents.values().filter(|&&p| p == cell).count();
    assert_eq!(children(cell), 2, "a row and its separator");
    // Viewable rows reach the component once they stayed long enough.
    let viewable = |first: f64, last: f64| {
        engine.control().dispatch_event(
            list,
            EventKind::ViewableItemsChanged,
            EventPayload::Map(vec![
                ("first", PayloadValue::Number(first)),
                ("last", PayloadValue::Number(last)),
            ]),
        );
    };
    viewable(2.0, 9.0);
    viewable(3.0, 10.0);
    tree.drain(&rx);
    assert!(tree.shows("visible 3-10"), "{:?}", tree.texts);
    assert!(
        !tree.history.iter().any(|t| t == "visible 2-9"),
        "a range left within the minimum view time is not reported"
    );
    // The end reached empties it: the empty view shows, and is the
    // component's.
    engine
        .control()
        .dispatch_event(list, EventKind::EndReached, EventPayload::None);
    tree.drain(&rx);
    assert!(tree.shows("nothing"), "{:?}", tree.texts);
    let nothing = tree.find("nothing").unwrap();
    engine
        .control()
        .dispatch_event(nothing, EventKind::Press, EventPayload::None);
    tree.drain(&rx);
    assert!(
        tree.shows("row 2") && !tree.shows("nothing"),
        "{:?}",
        tree.texts
    );
}

/// A `ListView` with rows of this component and a header.
#[derive(Default)]
struct Stocks {
    rows: std::rc::Rc<Vec<String>>,
    pressed: String,
    ended: u32,
}

impl Component for Stocks {
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), _: &mut Context<'_, Self>) {
        self.rows = std::rc::Rc::new((0..100).map(|i| format!("stock {i}")).collect());
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        let press = cx.listener(|this: &mut Self, index: &usize, cx| {
            this.pressed = format!("pressed {index}");
            cx.notify();
        });
        view().flex_1().child(
            ListView::element(ListViewProps::default()).items(
                virtual_list(rows.len(), move |i, _| {
                    text(rows[i].clone()).on_press(press.clone().map(move |_: &()| i))
                })
                .keys(0..100)
                .header(text(format!("{} ended {}", self.pressed, self.ended)))
                .on_end_reached(cx.listener(|this: &mut Self, _, cx| {
                    this.ended += 1;
                    cx.notify();
                })),
            ),
        )
    }
}

#[test]
fn rust_components_fill_a_list_views_rows() {
    let (engine, rx) = start(Root::of::<Stocks>());
    let mut tree = Tree::default();
    tree.drain(&rx);
    for label in ["stock 0", "stock 1", " ended 0"] {
        assert!(tree.shows(label), "{label}: {:?}", tree.texts);
    }
    // The rows are in the list view's own wrappers (its padding), inside
    // its list: cell > wrapper > the row's node > the text.
    let row = tree.find("stock 1").unwrap();
    let (list, cell) = tree.cell(row).expect("in a cell");
    assert_eq!(tree.depth(row, cell), Some(3), "{:?}", tree.parents);
    // The header is the list's too.
    assert_eq!(
        tree.cell(tree.find(" ended 0").unwrap()).map(|c| c.0),
        Some(list)
    );
    // A row's press is the row component's (the list's owner).
    engine
        .control()
        .dispatch_event(row, EventKind::Press, EventPayload::None);
    tree.drain(&rx);
    assert!(tree.shows("pressed 1 ended 0"), "{:?}", tree.texts);
    // Its list's events are the component's too.
    engine
        .control()
        .dispatch_event(list, EventKind::EndReached, EventPayload::None);
    tree.drain(&rx);
    assert!(tree.shows("pressed 1 ended 1"), "{:?}", tree.texts);
}

/// The app, compiled by nsh and loaded as bytecode.
fn start_js(app: &str) -> (Engine, Receiver<MountBatch>) {
    let dir = std::env::temp_dir().join(format!("nsh-rust-lists-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, app).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: vec![nsh_ui_mobile::registry()],
    })
    .unwrap();
    let bytecode =
        nsh_js::compile_bytecode_with(&bundle.code, false, &[nsh_ui_mobile::registry()]).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        // SAFETY: compiled just now by this runtime.
        app: App::Js(unsafe { nsh_engine::Bundle::bytecode(bytecode) }),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: vec![nsh_ui_mobile::registry()],
        runtime: None,
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

const APP_LIST_VIEW: &str = r#"import { mount, signal, Text, View } from "nsh";
import { ListView } from "nsh/ui/mobile";

const ends = signal(0);
const rows = Array.from({ length: 100 }, (_, i) => `row ${i}`);

mount(() => (
  <View style={{ flex: 1 }}>
    <Text>{`ends ${ends()}`}</Text>
    <ListView each={rows} onEndReached={() => ends.set(ends() + 1)}>
      {(row) => <Text>{row()}</Text>}
    </ListView>
  </View>
));
"#;

#[test]
fn the_apps_handlers_on_a_list_views_rows_run() {
    let (engine, rx) = start_js(APP_LIST_VIEW);
    let mut tree = Tree::default();
    tree.drain(&rx);
    assert!(tree.shows("ends 0"), "{:?}", tree.texts);
    let row = tree.find("row 1").expect("rows");
    let (list, _) = tree.cell(row).expect("in a list");
    // The list is a slot of the component (its node is JS's): the app's
    // `onEndReached` gets the event.
    engine
        .control()
        .dispatch_event(list, EventKind::EndReached, EventPayload::None);
    tree.drain(&rx);
    assert!(tree.shows("ends 1"), "{:?}", tree.texts);
}

/// A stepper it asks for its value, then sets.
#[derive(Default)]
struct Asker {
    asked: String,
    stepper: Option<ComponentRef<Stepper>>,
    ask: Option<Task<()>>,
}

impl Component for Asker {
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), cx: &mut Context<'_, Self>) {
        self.stepper = Some(cx.component_ref());
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let stepper = self.stepper.as_ref().expect("set up");
        view()
            .flex_1()
            .child(text(format!("asked {}", self.asked)).on_press(cx.listener(
                |this: &mut Self, _, cx| {
                    let stepper = this.stepper.clone().expect("set up");
                    let value = stepper.update(|s, cx| {
                        let value = s.value();
                        s.set_value(5.0, cx);
                        value
                    });
                    this.ask = Some(cx.spawn(|this, _| async move {
                        let value = value.await;
                        let _ = this
                            .update(move |this, cx| {
                                this.asked = format!("{value:?}");
                                cx.notify();
                            })
                            .await;
                    }));
                },
            )))
            .child(
                Stepper::element(StepperProps {
                    default_value: 3.0,
                    ..StepperProps::default()
                })
                .component_ref(stepper),
            )
    }
}

#[test]
fn rust_components_call_the_methods_of_components_they_render() {
    let (engine, rx) = start(Root::of::<Asker>());
    let mut tree = Tree::default();
    tree.drain(&rx);
    assert!(tree.shows("3"), "{:?}", tree.texts);
    let ask = tree.find("asked ").unwrap();
    engine
        .control()
        .dispatch_event(ask, EventKind::Press, EventPayload::None);
    tree.drain(&rx);
    assert!(tree.shows("asked Some(3.0)"), "{:?}", tree.texts);
    assert!(
        tree.shows("5"),
        "the stepper shows its new value: {:?}",
        tree.texts
    );
}
