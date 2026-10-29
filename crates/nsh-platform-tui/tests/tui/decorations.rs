use nsh_core::{AccessibilityRole, prelude::*};
use nsh_platform_tui::{App, Root};
use ratatui_core::{
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier},
    terminal::Terminal,
};

use super::app::{draw_until, surface};

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[component]
fn Borders() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(80.0))
                .h(px(48.0))
                .border_1()
                .rounded(px(8.0))
                .border_color(nsh_core::color::rgb(0x2c2e33))
                .shadow_lg()
                .child(text("card")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(48.0))
                .border_1()
                .rounded(px(8.0))
                .border_color(nsh_core::color::rgb(0xef4444))
                .child(text("invalid")),
        )
}

#[test]
fn borders_are_straight_single_lines_in_their_palette_color() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    view.load(App::Rust(Root::new::<Borders>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "invalid");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 0), "┌────────┐");
    assert_eq!(row(buffer, 2), "└────────┘");
    assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
    assert_eq!(row(buffer, 3), "┌────────┐");
    assert_eq!(
        buffer[(0, 3)].fg,
        Color::Red,
        "an accent border keeps its meaning"
    );
    assert_eq!(row(buffer, 5), "└────────┘");
}

#[component]
fn Buttons() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            nsh_base::button()
                .w(px(80.0))
                .h(px(32.0))
                .px(px(16.0))
                .border_1()
                .rounded(px(8.0))
                .bg(nsh_core::color::rgb(0xc8f26d))
                .flex_row()
                .items_center()
                .on_press(|_: &()| {})
                .child(text("Buy")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .px(px(16.0))
                .on_press(|_: &()| {})
                .child(text("Sell")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(48.0))
                .px(px(16.0))
                .border_1()
                .on_press(|_: &()| {})
                .child(text("Row"))
                .child(text("details")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .px(px(16.0))
                .role(AccessibilityRole::Checkbox)
                .on_press(|_: &()| {})
                .child(text("Check")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .on_press(|_: &()| {})
                .child(text("Flush")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .px(px(24.0))
                .on_press(|_: &()| {})
                .child(
                    view()
                        .absolute()
                        .left(px(0.0))
                        .w(px(3.0))
                        .h(px(16.0))
                        .bg(nsh_core::color::rgb(0x45d49a)),
                )
                .child(text("Nav")),
        )
}

#[test]
fn a_pressable_with_one_row_of_text_reads_as_a_bracketed_label() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 10)).unwrap();
    view.load(App::Rust(Root::new::<Buttons>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Flush");
    let buffer = terminal.backend().buffer();
    let lines: Vec<_> = (0..10).map(|y| row(buffer, y)).collect();
    // The two-row bordered button: brackets in its own Cells, no side bars,
    // no fill.
    assert!(
        lines[0..2].iter().any(|line| line == "[ Buy ]"),
        "{lines:?}"
    );
    assert!(
        !lines[0..2].iter().any(|line| line.contains('│')),
        "{lines:?}"
    );
    assert!((0..10).all(|x| !buffer[(x, 0)].modifier.contains(Modifier::REVERSED)));
    assert_eq!(lines[2], "[ Sell ]", "a pressable without a role");
    assert_eq!(
        lines[3], "┌────────┐",
        "two rows of text keep the app's box"
    );
    assert!(
        lines[3..6].iter().all(|line| !line.contains('[')),
        "{lines:?}"
    );
    assert_eq!(lines[6], "  Check", "other roles keep their own look");
    assert_eq!(lines[7], "Flush", "no room for both brackets");
    assert_eq!(
        lines[8], "│[ Nav ]",
        "brackets go around the text, not the accent"
    );
}

#[component]
fn IconButtons() -> impl IntoElement {
    let icon = || {
        canvas(Painter::new().fill_rect(0.0, 0.0, 16.0, 16.0))
            .w(px(16.0))
            .h(px(16.0))
    };
    view()
        .flex_1()
        .child(
            view()
                .w(px(32.0))
                .h(px(32.0))
                .role(AccessibilityRole::Button)
                .label("Light appearance")
                .items_center()
                .justify_center()
                .on_press(|_: &()| {})
                .child(icon()),
        )
        .child(
            view()
                .w(px(160.0))
                .h(px(16.0))
                .role(AccessibilityRole::Button)
                .label("Close")
                .on_press(|_: &()| {})
                .child(icon()),
        )
        .child(
            view()
                .flex_row()
                .child(icon())
                .child(text("icon"))
                .child(icon().role(AccessibilityRole::Image).label("logo"))
                .child(icon().label("chart")),
        )
}

#[test]
fn icons_are_not_drawn_and_an_icon_only_button_shows_its_label() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    view.load(App::Rust(Root::new::<IconButtons>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "icon");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 0), "[L…]", "cut to its 4 by 2 Cell box");
    assert_eq!(row(buffer, 1), "");
    assert_eq!(row(buffer, 2), "     [ Close ]");
    assert_eq!(
        row(buffer, 3),
        "  icon  ⣿⣿",
        "decorative canvases leave their Cells empty; a labeled one is content"
    );
}

#[cfg(feature = "js")]
#[test]
fn image_placeholders_smaller_than_three_by_two_cells_are_decorative() {
    let code = nsh_bundler::compile_module(
        "App.tsx",
        r#"
        import { mount, View, Text, Image } from "nsh";
        mount(() => <View>
            <Image src="small.png" style={{ width: 16, height: 16 }} />
            <Image src="large.png" style={{ width: 80, height: 48 }} />
            <Text>ready</Text>
        </View>);
    "#,
        &[],
    )
    .unwrap();
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    view.load(nsh_platform_tui::Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 0), "");
    assert!(row(buffer, 1).starts_with('┌'), "{:?}", row(buffer, 1));
}
