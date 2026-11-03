//! An `Editor` in a terminal: its rows and line numbers sit on absolute
//! layers over its scroll views, inside a box sized only by its column,
//! and the Cells draw their text, alone and as a dock's panel.

use nsh_base::{
    dock::{DockAreaLayout, DockLayout, DockPanel, DockPlacement, DockRegion, dock_area},
    editor::editor,
};
use nsh_core::prelude::*;
use nsh_platform_tui::{App, Root};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

use super::{
    app::{draw_until, surface},
    dock::dump,
};

const TEXT: &str = "let total = 42;\nreturn total;";

/// A status line over an editor that takes the rest of the column.
#[component]
fn Source() -> impl IntoElement {
    view()
        .flex_1()
        .child(text("Source"))
        .child(editor().default_value(SharedString::from(TEXT)).flex_1())
}

#[test]
fn an_editors_rows_are_drawn_in_its_column() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(48, 12)).unwrap();
    view.load(App::Rust(Root::new::<Source>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Source");
    draw_until(&mut view, &mut terminal, &rx, "let total = 42;");
    draw_until(&mut view, &mut terminal, &rx, "return total;");
    let text = dump(&terminal);
    let first = text.find("let total").unwrap();
    let second = text.find("return total").unwrap();
    assert!(first < second, "{text}");
}

/// The editor as the gallery shows it: the center panel of a dock area
/// with a bottom dock.
#[component]
fn DockedSource() -> impl IntoElement {
    let mut bottom = DockRegion::new(DockLayout::tabs().panel("problems"));
    bottom.size = Some(px(96.0));
    let layout = signal(
        DockAreaLayout::new(DockLayout::tabs().panel("editor"))
            .with_dock(DockPlacement::Bottom, bottom),
    );
    view().flex_1().child(
        dock_area()
            .layout(move || layout.get())
            .compact(Some(false))
            .tab_bar_class("flex-row gap-2")
            .render_tab(|tab: &nsh_base::dock::DockTab| text(format!("Tab {}", tab.panel)))
            .render_panel(|panel: &DockPanel| match panel.id.as_ref() {
                "editor" => view()
                    .flex_1()
                    .child(editor().default_value(SharedString::from(TEXT)).flex_1())
                    .into_element(),
                _ => view().flex_1().child(text("No problems")).into_element(),
            })
            .on_layout_change(move |next: &DockAreaLayout| layout.set(next.clone()))
            .size_full(),
    )
}

#[test]
fn an_editor_in_a_dock_panel_draws_its_rows() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
    view.load(App::Rust(Root::new::<DockedSource>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "No problems");
    draw_until(&mut view, &mut terminal, &rx, "let total = 42;");
}
