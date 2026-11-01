use nsh_core::{AccessibilityState, prelude::*};
use nsh_platform_tui::{App, Appearance, Handled, Input, Root};
use ratatui_core::{
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier},
    terminal::Terminal,
};

use super::app::{draw_until, selected, surface};

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn reversed_anywhere(buffer: &Buffer) -> bool {
    buffer
        .content
        .iter()
        .any(|cell| cell.modifier.contains(Modifier::REVERSED))
}

/// Whether row `y` carries the soft bar in `color` over `columns`, bold.
fn barred(buffer: &Buffer, y: u16, columns: std::ops::Range<u16>, color: Color) -> bool {
    columns
        .into_iter()
        .all(|x| buffer[(x, y)].bg == color && buffer[(x, y)].modifier.contains(Modifier::BOLD))
}

fn unbarred(buffer: &Buffer, y: u16) -> bool {
    (0..buffer.area.width).all(|x| buffer[(x, y)].bg == Color::Reset)
}

/// A watchlist row: 64 points (four Cell rows), a code and a name on the
/// left, a price and its red change on the right, the two texts centered.
fn quote_row(selection: AccessibilityState) -> impl IntoElement {
    view()
        .w(px(160.0))
        .h(px(64.0))
        .flex_row()
        .items_center()
        .px(px(16.0))
        .accessibility_state(selection)
        .on_press(|_: &()| {})
        .child(
            view()
                .flex_1()
                .child(text("ORBX"))
                .child(text("Orbex").text_color(nsh_core::color::rgb(0x8a8f98))),
        )
        .child(
            view()
                .items_end()
                .child(text("47.35"))
                .child(text("-1.99%").text_color(nsh_core::color::rgb(0xef4444))),
        )
}

#[component]
fn Watchlist() -> impl IntoElement {
    view()
        .flex_1()
        .child(quote_row(selected()))
        .child(quote_row(AccessibilityState::default()))
        .child(
            view()
                .w(px(160.0))
                .h(px(32.0))
                .accessibility_state(selected()),
        )
        .child(text("end"))
}

#[test]
fn a_selected_view_is_a_soft_bar_on_its_text_rows_and_its_content_stays_put() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 12)).unwrap();
    view.load(App::Rust(Root::new::<Watchlist>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "end");
    let buffer = terminal.backend().buffer();
    assert!(!reversed_anywhere(buffer), "no reverse video");
    let text_rows: Vec<u16> = (0..4).filter(|&y| row(buffer, y).trim() != "").collect();
    assert_eq!(
        text_rows.len(),
        2,
        "{:?}",
        (0..4).map(|y| row(buffer, y)).collect::<Vec<_>>()
    );
    for y in 0..4 {
        if text_rows.contains(&y) {
            assert!(barred(buffer, y, 0..20, Color::Black), "the bar on row {y}");
            assert_eq!(buffer[(20, y)].bg, Color::Reset, "only the box's width");
        } else {
            assert!(unbarred(buffer, y), "no bar on the blank row {y}");
        }
    }
    let change = row(buffer, text_rows[1]).find("-1.99%").unwrap() as u16;
    assert_eq!(
        buffer[(change, text_rows[1])].fg,
        Color::Red,
        "its red ink on the bar"
    );
    for y in 0..4 {
        assert_eq!(row(buffer, y), row(buffer, y + 4), "content never moves");
    }
    for y in 4..8 {
        assert!(unbarred(buffer, y), "the unselected row");
    }
    assert!(
        barred(buffer, 8, 0..20, Color::Black),
        "a box without text: its first row"
    );
    assert!(unbarred(buffer, 9));
}

#[component]
fn FocusRows() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            nsh_base::button()
                .w(px(80.0))
                .h(px(48.0))
                .justify_center()
                .on_press(|_: &()| {})
                .child(text("button")),
        )
        .child(text("end"))
}

#[test]
fn keyboard_focus_is_the_bar_in_the_light_appearances_color() {
    let (mut view, rx) = surface();
    view.set_appearance(Appearance::Light);
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    view.load(App::Rust(Root::new::<FocusRows>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "end");
    let before: Vec<_> = (0..4)
        .map(|y| row(terminal.backend().buffer(), y))
        .collect();
    assert_eq!(view.handle(key("Tab")), Handled::Yes);
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(!reversed_anywhere(buffer));
    assert!(unbarred(buffer, 0));
    assert!(
        barred(buffer, 1, 0..10, Color::Gray),
        "{:?}",
        buffer[(0, 1)]
    );
    assert!(unbarred(buffer, 2));
    let after: Vec<_> = (0..4).map(|y| row(buffer, y)).collect();
    assert_eq!(before, after, "content never moves");
}

fn key(name: &str) -> Input {
    Input::Key {
        key: name.into(),
        modifiers: Default::default(),
        release: false,
    }
}
