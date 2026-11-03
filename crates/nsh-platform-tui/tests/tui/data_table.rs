//! A `DataTable` in a terminal: its header and rows sit on absolute
//! layers inside a bordered root that grows into its column, and the
//! Cells draw its titles and values, not only its frame.

use nsh_base::{DataTableColumn, DataTableRows, data_table};
use nsh_core::prelude::*;
use nsh_platform_tui::{App, Root};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

use super::app::{draw_until, surface};

fn column(key: &str, title: &str) -> DataTableColumn {
    let mut c = DataTableColumn::default();
    c.key = key.to_owned().into();
    c.title = Some(title.to_owned().into());
    c
}

/// A status line over a table that takes the rest of the column, laid
/// out as the gallery's demo lays out its own.
#[component]
fn Quotes() -> impl IntoElement {
    let rows = DataTableRows::new(3)
        .texts("symbol", ["AAPL", "MSFT", "NVDA"])
        .numbers("last", [231.5, 412.0, 118.25]);
    view().flex_1().child(text("Quotes")).child(
        data_table()
            .flex_1()
            .rounded(px(8.0))
            .border_1()
            .columns(vec![column("symbol", "Symbol"), column("last", "Last")])
            .data(rows),
    )
}

#[test]
fn a_tables_header_and_rows_are_drawn_inside_its_border() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(48, 12)).unwrap();
    view.load(App::Rust(Root::new::<Quotes>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Quotes");
    for shown in ["Symbol", "Last", "AAPL", "231.5", "MSFT", "NVDA"] {
        draw_until(&mut view, &mut terminal, &rx, shown);
    }
}
