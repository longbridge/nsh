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
    let raised = nsh_core::color::rgb(0x24262a);
    let row = || {
        view()
            .w(px(80.0))
            .h(px(16.0))
            .px(px(16.0))
            .on_press(|_: &()| {})
    };
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x0c0d0f))
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
        .child(row().bg(raised).child(text("Sell")))
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
            row()
                .border_1()
                .role(AccessibilityRole::Checkbox)
                .child(text("Check")),
        )
        .child(row().px(px(0.0)).border_1().child(text("Flush")))
        .child(
            row()
                .px(px(24.0))
                .bg(raised)
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
        .child(row().child(text("Item")))
        .child(
            row()
                .bg(raised)
                .accessibility_state(super::app::selected())
                .child(text("Chosen")),
        )
        .child(row().bg(nsh_core::color::rgb(0x0c0d0f)).child(text("Same")))
}

#[test]
fn a_boxed_pressable_with_one_row_of_text_reads_as_a_bracketed_label() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 12)).unwrap();
    view.load(App::Rust(Root::new::<Buttons>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Same");
    let buffer = terminal.backend().buffer();
    let lines: Vec<_> = (0..12).map(|y| row(buffer, y)).collect();
    // The two-row bordered button: brackets in its own Cells around the label
    // where the layout put it, no side bars, no fill.
    assert!(
        lines[0..2].iter().any(|line| line == "[ Buy ]"),
        "{lines:?}"
    );
    assert!(
        !lines[0..2].iter().any(|line| line.contains('│')),
        "{lines:?}"
    );
    assert!((0..10).all(|x| !buffer[(x, 0)].modifier.contains(Modifier::REVERSED)));
    assert_eq!(lines[2], "[ Sell ]", "a raised fill is a box");
    assert_eq!(
        lines[3], "┌────────┐",
        "two rows of text keep the app's box"
    );
    assert!(
        lines[3..6].iter().all(|line| !line.contains('[')),
        "{lines:?}"
    );
    assert_eq!(lines[6], "  Check", "other roles keep their own look");
    assert_eq!(
        lines[7], "Flush",
        "no room for both brackets, nor for a border"
    );
    assert_eq!(
        lines[8], "│[ Nav ]",
        "brackets go around the text, not the accent"
    );
    assert_eq!(lines[9], "  Item", "a list item without a box");
    assert_eq!(lines[10], "  Chosen", "selection is reverse, not brackets");
    assert_eq!(
        lines[11], "  Same",
        "a fill like the one under it is no box"
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
                .border_1()
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
                .bg(nsh_core::color::rgb(0x24262a))
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
    assert_eq!(row(buffer, 0), "");
    assert_eq!(
        row(buffer, 1),
        "[ Light appearance ]",
        "its 4 by 2 Cell box and the blank Cells beside it, on its icon's row"
    );
    assert_eq!(row(buffer, 2), "     [ Close ]");
    assert_eq!(
        row(buffer, 3),
        "  icon  ⣿⣿",
        "decorative canvases leave their Cells empty; a labeled one is content"
    );
}

#[cfg(feature = "js")]
#[test]
fn images_without_an_accessibility_label_are_decorative_at_any_size() {
    let code = nsh_bundler::compile_module(
        "App.tsx",
        r#"
        import { mount, View, Text, Image } from "nsh";
        mount(() => <View>
            <Image src="small.png" style={{ width: 16, height: 16 }} />
            <Image src="logo.png" style={{ width: 80, height: 48 }} />
            <Image src="chart.png" accessibilityLabel="Chart" style={{ width: 80, height: 48 }} />
            <Text>ready</Text>
        </View>);
    "#,
        &[],
    )
    .unwrap();
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 10)).unwrap();
    view.load(nsh_platform_tui::Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let lines: Vec<_> = (0..10).map(|y| row(buffer, y)).collect();
    assert!(lines[0..4].iter().all(String::is_empty), "{lines:?}");
    assert_eq!(lines[4], "┌Chart───┐", "{lines:?}");
    assert_eq!(lines[6], "└────────┘", "{lines:?}");
    assert_eq!(lines[7], "ready", "content never moves");
}

#[component]
fn WrappedButtons() -> impl IntoElement {
    let styled = |label: &'static str| {
        view().on_press(|_: &()| {}).child(
            view()
                .w(px(80.0))
                .h(px(32.0))
                .px(px(16.0))
                .border_1()
                .bg(nsh_core::color::rgb(0xc8f26d))
                .justify_center()
                .child(text(label).text_color(nsh_core::color::rgb(0x13160c))),
        )
    };
    view().flex_1().bg(nsh_core::color::rgb(0x0c0d0f)).child(
        view()
            .flex_row()
            .gap(px(8.0))
            .child(styled("Sell"))
            .child(styled("Buy")),
    )
}

#[test]
fn a_press_handler_around_a_styled_box_is_a_bracketed_button_whose_label_stays_put() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 3)).unwrap();
    view.load(App::Rust(Root::new::<WrappedButtons>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Buy");
    let buffer = terminal.backend().buffer();
    let lines: Vec<_> = (0..3).map(|y| row(buffer, y)).collect();
    let line = lines
        .iter()
        .find(|line| line.contains("Sell"))
        .expect("the labels");
    // Each label keeps the column the layout gave it (16 points into a box at
    // 0 and at 88 points), and its brackets take the Cells around it.
    assert_eq!(line, "[ Sell ]   [ Buy ]", "{lines:?}");
    assert!(
        lines.iter().all(|line| !line.contains(['│', '─', '┌'])),
        "{lines:?}"
    );
    let label = line.find("Buy").unwrap() as u16;
    let y = lines.iter().position(|l| l == line).unwrap() as u16;
    assert!(
        !buffer[(label, y)].modifier.contains(Modifier::DIM),
        "dark text designed on its lime fill"
    );
}

/// An icon tile: a bordered, filled box whose only content is an icon and a
/// status dot.
fn tile() -> View {
    view()
        .w(px(32.0))
        .h(px(32.0))
        .border_1()
        .rounded(px(8.0))
        .bg(nsh_core::color::rgb(0x24262a))
        .border_color(nsh_core::color::rgb(0x8a8f98))
        .items_center()
        .justify_center()
        .child(
            canvas(Painter::new().fill_rect(0.0, 0.0, 16.0, 16.0))
                .w(px(16.0))
                .h(px(16.0)),
        )
        .child(
            view()
                .absolute()
                .right(px(0.0))
                .top(px(0.0))
                .w(px(8.0))
                .h(px(8.0))
                .bg(nsh_core::color::rgb(0x45d49a)),
        )
}

#[component]
fn Tiles() -> impl IntoElement {
    let header = |tile: View| {
        view()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .child(tile)
            .child(text("Title"))
    };
    view()
        .flex_1()
        .child(header(tile()))
        .child(header(tile().accessibility_state(super::app::selected())))
        .child(header(view().w(px(32.0)).h(px(32.0))))
}

#[test]
fn a_box_whose_only_content_was_dropped_is_not_drawn_and_its_neighbors_stay_put() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(16, 6)).unwrap();
    view.load(App::Rust(Root::new::<Tiles>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Title");
    let buffer = terminal.backend().buffer();
    let lines: Vec<_> = (0..6).map(|y| row(buffer, y)).collect();
    // No border, fill, dot or selection bar: the tile takes no glyphs, and
    // the title keeps the column it has beside an empty spacer.
    assert_eq!(lines[0..2], lines[4..6], "{lines:?}");
    assert_eq!(lines[2..4], lines[4..6], "{lines:?}");
    assert!(
        lines.iter().all(|line| !line.contains(['┌', '│', '└'])),
        "{lines:?}"
    );
    assert!(
        buffer
            .content
            .iter()
            .all(|cell| cell.bg == Color::Reset && !cell.modifier.contains(Modifier::REVERSED)),
        "{lines:?}"
    );
}

#[component]
fn CrowdedIconButtons() -> impl IntoElement {
    let icon_button = |label: &'static str| {
        view()
            .w(px(32.0))
            .h(px(32.0))
            .border_1()
            .role(AccessibilityRole::Button)
            .label(label)
            .items_center()
            .justify_center()
            .on_press(|_: &()| {})
            .child(
                canvas(Painter::new().fill_rect(0.0, 0.0, 16.0, 16.0))
                    .w(px(16.0))
                    .h(px(16.0)),
            )
    };
    let header = |button: View| {
        view()
            .flex_row()
            .items_center()
            .gap(px(16.0))
            .child(text("Gallery"))
            .child(button)
            .child(text("NSH"))
    };
    view()
        .flex_1()
        .child(header(icon_button("Light appearance")))
        .child(header(icon_button("Go")))
        .child(header(view().w(px(32.0)).h(px(32.0))))
}

#[test]
fn an_icon_only_button_shows_its_whole_label_or_nothing_and_focus_puts_the_bar_on_it() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    view.load(App::Rust(Root::new::<CrowdedIconButtons>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "NSH");
    let lines: Vec<_> = (0..6)
        .map(|y| row(terminal.backend().buffer(), y))
        .collect();
    // Between two texts the long label has no room: neither a cut label nor
    // the button's box, and the texts keep their columns. The short one
    // takes the blank Cells beside its box, a Cell away from the texts.
    assert_eq!(lines[0..2], lines[4..6], "{lines:?}");
    assert_eq!(lines[2], "", "{lines:?}");
    assert_eq!(lines[3], "Gallery [ Go ] NSH", "{lines:?}");
    assert!(
        lines.iter().all(|line| !line.contains(['…', '┌', '│'])),
        "{lines:?}"
    );

    let key = |name: &str| nsh_platform_tui::Input::Key {
        key: name.into(),
        modifiers: Default::default(),
        release: false,
    };
    assert_eq!(view.handle(key("Tab")), nsh_platform_tui::Handled::Yes);
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let focused = |x: u16, y: u16| buffer[(x, y)].bg != Color::Reset;
    assert!((9..13).all(|x| focused(x, 1)), "the bar on its label's row");
    assert!(!focused(8, 1) && !focused(13, 1) && !focused(9, 0));
    let after: Vec<_> = (0..6).map(|y| row(buffer, y)).collect();
    assert_eq!(lines, after, "content never moves");

    assert_eq!(view.handle(key("Tab")), nsh_platform_tui::Handled::Yes);
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(
        (8..14).all(|x| buffer[(x, 3)].bg != Color::Reset),
        "the bar under the whole label"
    );
    assert!(!(buffer[(7, 3)].bg != Color::Reset) && !(buffer[(14, 3)].bg != Color::Reset));
    assert!((0..20).all(|x| buffer[(x, 2)].bg == Color::Reset));
}
