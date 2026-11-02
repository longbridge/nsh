//! Touch gestures keep their native position when the table already has
//! room to scroll, including a fractional final offset.

use std::sync::{Arc, Mutex};

use nsh_base::{DataTableColumn, DataTableRange, data_table};
use nsh_core::{Command, MountItem, Size, element::Root, platform::Os, prelude::*};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

#[component]
fn Table() -> impl IntoElement {
    let first = signal(0);
    let mut column = DataTableColumn::default();
    column.key = "id".into();
    view()
        .test_id("table")
        .size_full()
        .child(
            data_table()
                .flex_1()
                .columns(vec![column])
                .row_count(1_000_000)
                .row_height(px(28.0))
                .on_visible_rows_change(move |range: &DataTableRange| first.set(range.start)),
        )
        .child(text(move || format!("First {}", first.get() + 1)))
}

#[test]
fn short_touch_scrolls_end_without_a_corrective_native_scroll() {
    for os in [Os::Ios, Os::Android] {
        let mut config = EngineConfig {
            viewport: Size::new(px(390.0), px(844.0)),
            ..EngineConfig::new(Root::of::<Table>())
        };
        config.environment.os = os;
        config.environment.pixel_ratio = 2.625;
        let mut app = Headless::start(config).unwrap();
        app.wait_for("text:First 1").unwrap();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let observed = commands.clone();
        app.observe(move |_, batch| {
            for item in &batch.items {
                if let MountItem::Command {
                    command: Command::ScrollTo { x, y, .. },
                    ..
                } = item
                {
                    observed.lock().unwrap().push((*x, *y));
                }
            }
        });
        app.scroll("id:table", px(1372.125)).unwrap();
        app.wait_for("text:First 50").unwrap();
        app.scroll("id:table", px(-200.0)).unwrap();
        app.wait_for("text:First 42").unwrap();
        app.scroll("id:table", px(200.0)).unwrap();
        app.wait_for("text:First 50").unwrap();
        assert!(
            commands.lock().unwrap().is_empty(),
            "{os:?}: unnecessary scroll correction"
        );
    }
}
