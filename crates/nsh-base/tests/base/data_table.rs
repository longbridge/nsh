//! `DataTable` with rows a Rust app makes a column at a time, each of one
//! type: the texts, numbers and booleans the table shows; its column
//! handles are the pointer's alone, in a terminal too.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use nsh_base::{DataTableColumn, DataTableRows, data_table, resize_handle};
use nsh_core::{
    MountItem, NodeId, Size,
    element::Root,
    platform::{Environment, Idiom, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

fn start<C: Component<Props = ()>>() -> Headless {
    let config = EngineConfig {
        viewport: Size::new(px(600.0), px(300.0)),
        ..EngineConfig::new(Root::of::<C>())
    };
    let mut app = Headless::start(config).unwrap();
    app.first_render().unwrap();
    app
}

fn column(key: &str) -> DataTableColumn {
    let mut c = DataTableColumn::default();
    c.key = key.to_owned().into();
    c
}

/// Quotes: a text, two number and a boolean column.
#[component]
fn Quotes() -> impl IntoElement {
    let rows = DataTableRows::new(2)
        .texts("symbol", ["AAPL", "MSFT"])
        .numbers("last", [231.5, 412.0])
        .numbers("change", [1.2, -0.4])
        .bools("open", [true, false]);
    data_table()
        .h(px(200.0))
        .columns(vec![
            column("symbol"),
            column("last"),
            column("change"),
            column("open"),
        ])
        .data(rows)
}

#[test]
fn rows_made_in_rust_show_their_cells() {
    let mut app = start::<Quotes>();
    for shown in [
        "AAPL", "231.5", "1.2", "MSFT", "412", "-0.4", "true", "false",
    ] {
        app.wait_for(shown).unwrap();
    }
}

/// The table of [`Quotes`] beside a lone resize handle (`id:lone` inside),
/// in a terminal.
#[component]
fn Handles() -> impl IntoElement {
    view().child(quotes()).child(
        view()
            .relative()
            .h(px(40.0))
            .child(resize_handle().child(view().test_id("lone").w(px(1.0)))),
    )
}

#[test]
fn a_terminals_column_handles_are_no_stops_of_tab() {
    let viewport = Size::new(px(600.0), px(300.0));
    let config = EngineConfig {
        viewport,
        environment: Environment {
            window: viewport,
            os: Os::Linux,
            idiom: Idiom::Terminal,
            ..Environment::default()
        },
        ..EngineConfig::new(Root::of::<Handles>())
    };
    let mut app = Headless::start(config).unwrap();
    // Each view's parent, test id and `focusable`.
    #[derive(Default)]
    struct Seen {
        parents: HashMap<NodeId, NodeId>,
        ids: HashMap<String, NodeId>,
        focusable: HashMap<NodeId, Option<bool>>,
    }
    let seen = Arc::new(Mutex::new(Seen::default()));
    let into = seen.clone();
    app.observe(move |_, batch| {
        let mut seen = into.lock().unwrap();
        for item in &batch.items {
            match item {
                MountItem::Insert { parent, child, .. } => {
                    seen.parents.insert(*child, *parent);
                }
                MountItem::ViewProps { id, props } => {
                    if let Some(t) = props.test_id.as_deref() {
                        seen.ids.insert(t.to_owned(), *id);
                    }
                    seen.focusable.insert(*id, props.focusable);
                }
                _ => {}
            }
        }
    });
    app.first_render().unwrap();
    app.wait_for("AAPL").unwrap();
    let seen = seen.lock().unwrap();
    // The band a handle's indicator is in.
    let band = |id: &str| {
        let child = seen.ids[id];
        seen.focusable[&seen.parents[&child]]
    };
    assert_eq!(band("lone"), Some(true), "a lone handle is a stop of Tab");
    for key in ["symbol", "last", "change", "open"] {
        assert_eq!(
            band(&format!("resize:{key}")),
            Some(false),
            "the {key} column's handle is the pointer's"
        );
    }
}
