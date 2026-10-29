use nsh_core::{AccessibilityState, Checked, prelude::*};
use nsh_platform_tui::{App, Appearance, Handled, Input, Root};
use ratatui_core::{
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier},
    terminal::Terminal,
};

use super::app::{draw_until, surface};

fn key(name: &str) -> Input {
    Input::Key {
        key: name.into(),
        modifiers: Default::default(),
        release: false,
    }
}

fn dim(buffer: &Buffer, x: u16, y: u16) -> bool {
    buffer[(x, y)].modifier.contains(Modifier::DIM)
}

fn reversed(buffer: &Buffer, x: u16, y: u16) -> bool {
    buffer[(x, y)].modifier.contains(Modifier::REVERSED)
}

#[component]
fn Inks() -> impl IntoElement {
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x0a0a0a))
        .child(text("primary").text_color(nsh_core::color::rgb(0xfafafa)))
        .child(text("muted").text_color(nsh_core::color::rgb(0x8d9198)))
        .child(text("up").text_color(nsh_core::color::rgb(0x45d49a)))
        .child(text("down").text_color(nsh_core::color::rgb(0xef4444)))
        .child(text("info").text_color(nsh_core::color::rgb(0x7ea8ff)))
        .child(text("warn").text_color(nsh_core::color::rgb(0xf59e0b)))
        .child(text("faint").text_color(nsh_core::color::rgba(0xfafafa40)))
        .child(
            view()
                .opacity(0.5)
                .child(text("half").text_color(nsh_core::color::rgb(0xfafafa))),
        )
}

#[test]
fn app_colors_take_the_terminal_palettes_meanings() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<Inks>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "half");
    let buffer = terminal.backend().buffer();
    let reads = |y: u16| (buffer[(0, y)].fg, dim(buffer, 0, y));
    assert_eq!(reads(0), (Color::Reset, false), "primary text");
    assert_eq!(reads(1), (Color::Reset, true), "secondary text");
    assert_eq!(reads(2), (Color::Green, false), "up");
    assert_eq!(reads(3), (Color::Red, false), "down");
    assert_eq!(reads(4), (Color::Blue, false), "link");
    assert_eq!(reads(5), (Color::Yellow, false), "warning");
    assert_eq!(reads(6), (Color::Reset, true), "translucent text is dim");
    assert_eq!(reads(7), (Color::Reset, false), "opacity keeps the color");
    for cell in &buffer.content {
        assert_eq!(cell.bg, Color::Reset);
    }
}

#[component]
fn LightInks() -> impl IntoElement {
    view()
        .flex_1()
        .child(text("primary").text_color(nsh_core::color::rgb(0x0a0a0a)))
        .child(text("muted").text_color(nsh_core::color::rgb(0x5f636a)))
        .child(text("white").text_color(nsh_core::color::rgb(0xfafafa)))
}

#[test]
fn the_appearance_decides_which_neutral_colors_are_dim() {
    let (mut view, rx) = surface();
    view.set_appearance(Appearance::Light);
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    view.load(App::Rust(Root::new::<LightInks>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "white");
    let buffer = terminal.backend().buffer();
    assert!(!dim(buffer, 0, 0), "dark text on a light page");
    assert!(dim(buffer, 0, 1));
    assert!(dim(buffer, 0, 2), "light text on a light page");
}

#[component]
fn Emphasis() -> impl IntoElement {
    view()
        .flex_1()
        .child(text("semibold").font_weight(FontWeight::SEMIBOLD))
        .child(text("medium").font_weight(FontWeight(500)))
        .child(text("italic").italic())
        .child(text("underline").underline())
}

#[test]
fn weight_600_and_above_is_bold_and_italic_and_underline_stay() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    view.load(App::Rust(Root::new::<Emphasis>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "underline");
    let buffer = terminal.backend().buffer();
    assert!(buffer[(0, 0)].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(0, 1)].modifier.contains(Modifier::BOLD));
    assert!(buffer[(0, 2)].modifier.contains(Modifier::ITALIC));
    assert!(buffer[(0, 3)].modifier.contains(Modifier::UNDERLINED));
}

#[component]
fn Fills() -> impl IntoElement {
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x112233))
        .child(
            view()
                .h(px(32.0))
                .bg(nsh_core::color::rgb(0x884422))
                .child(text("card")),
        )
        .child(text("under"))
        .child(
            view()
                .absolute()
                .top(px(32.0))
                .left(px(0.0))
                .w(px(24.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x223344))
                .child(text("pop")),
        )
        .child(text("scrim"))
        .child(
            view()
                .absolute()
                .top(px(48.0))
                .left(px(0.0))
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgba(0x00000080)),
        )
        .child(
            view()
                .w(px(40.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x22c55e)),
        )
}

#[test]
fn fills_draw_no_color_and_opaque_ones_clear_what_is_under_them() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    view.load(App::Rust(Root::new::<Fills>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "pop");
    let buffer = terminal.backend().buffer();
    for cell in &buffer.content {
        assert_eq!(cell.bg, Color::Reset);
    }
    assert_eq!(buffer[(0, 0)].symbol(), "c");
    assert_eq!(buffer[(0, 2)].symbol(), "p", "the popup covers the text");
    assert_eq!(buffer[(3, 2)].symbol(), "e", "beside the popup");
    assert_eq!(buffer[(3, 2)].symbol(), "e");
    assert_eq!(
        buffer[(0, 3)].symbol(),
        "s",
        "a translucent scrim draws nothing"
    );
    // A leaf whose fill is its content shows in its palette color.
    assert_eq!(buffer[(0, 4)].fg, Color::Green);
    assert!(reversed(buffer, 4, 4));
    assert!(!reversed(buffer, 5, 4));
}

#[component]
fn Rules() -> impl IntoElement {
    view()
        .flex_1()
        .child(text("above"))
        .child(view().h(px(1.0)).bg(nsh_core::color::rgba(0xffffff1a)))
        .child(
            view()
                .mt(px(15.0))
                .w(px(80.0))
                .h(px(48.0))
                .border_1()
                .border_color(nsh_core::color::rgb(0x2c2e33))
                .child(text("box")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(48.0))
                .border_1()
                .border_color(nsh_core::color::rgb(0xef4444))
                .child(text("error")),
        )
}

#[test]
fn separators_and_borders_take_their_palette_color() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<Rules>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "error");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 1)].symbol(), "─");
    assert!(dim(buffer, 0, 1));
    assert_eq!(buffer[(0, 2)].symbol(), "┌");
    assert!(dim(buffer, 0, 2));
    assert_eq!(buffer[(0, 2)].fg, Color::Reset);
    assert_eq!(buffer[(0, 5)].symbol(), "┌");
    assert_eq!(buffer[(0, 5)].fg, Color::Red);
    assert!(!dim(buffer, 0, 5));
}

#[component]
fn States() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .accessibility_state(AccessibilityState {
                    selected: true,
                    ..Default::default()
                })
                .child(text("tab")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .accessibility_state(AccessibilityState {
                    checked: Some(Checked::True),
                    ..Default::default()
                })
                .child(text("checked")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .accessibility_state(AccessibilityState {
                    checked: Some(Checked::False),
                    ..Default::default()
                })
                .child(text("unchecked")),
        )
        .child(
            nsh_base::button()
                .w(px(80.0))
                .h(px(16.0))
                .focus(|s| {
                    s.outline(px(2.0))
                        .outline_offset(px(2.0))
                        .outline_color(nsh_core::color::rgb(0x123456))
                        .bg(nsh_core::color::rgb(0x123456))
                })
                .on_press(|_: &()| {})
                .child(text("button")),
        )
        .child(
            nsh_base::input()
                .w(px(80.0))
                .h(px(16.0))
                .placeholder("field"),
        )
}

#[test]
fn selected_checked_and_focused_views_are_reversed_without_app_focus_outlines() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<States>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "field");
    let buffer = terminal.backend().buffer();
    assert!(reversed(buffer, 0, 0) && reversed(buffer, 9, 0), "selected");
    assert!(reversed(buffer, 0, 1) && reversed(buffer, 9, 1), "checked");
    assert!(!reversed(buffer, 0, 2), "unchecked");
    assert!(!reversed(buffer, 0, 3), "not focused yet");
    assert!(!reversed(buffer, 10, 1), "outside the box");

    assert_eq!(view.handle(key("Tab")), Handled::Yes);
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "button");
    let buffer = terminal.backend().buffer();
    assert!(reversed(buffer, 0, 3) && reversed(buffer, 9, 3), "focused");
    for x in 0..20 {
        for y in [2, 4] {
            assert_eq!(buffer[(x, y)].fg, Color::Reset, "no outline at ({x}, {y})");
            assert!(!"┌─┐└┘│".contains(buffer[(x, y)].symbol()));
        }
    }

    assert_eq!(view.handle(key("Tab")), Handled::Yes);
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(!reversed(buffer, 0, 3), "focus left the button");
    assert!(
        !reversed(buffer, 0, 4),
        "a focused field shows its caret instead"
    );
    assert!(dim(buffer, 0, 4), "placeholder");
}

#[component]
fn DesignedOnFills() -> impl IntoElement {
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x0c0d0f))
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0xc8f26d))
                .child(text("Buy").text_color(nsh_core::color::rgb(0x13160c))),
        )
        .child(text("bare").text_color(nsh_core::color::rgb(0x13160c)))
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgba(0xc8f26d80))
                .child(text("tint").text_color(nsh_core::color::rgb(0x13160c))),
        )
}

#[test]
fn neutral_text_is_read_against_the_opaque_fill_it_was_designed_on() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    view.load(App::Rust(Root::new::<DesignedOnFills>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "tint");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].fg, Color::Reset);
    assert!(!dim(buffer, 0, 0), "dark text designed on a lime fill");
    assert!(dim(buffer, 0, 1), "the same text on the dark page");
    assert!(dim(buffer, 0, 2), "a translucent fill is not a ground");
    assert_eq!(
        buffer[(0, 0)].bg,
        Color::Reset,
        "the fill is still not drawn"
    );
}

#[component]
fn SelectedChartRow() -> impl IntoElement {
    view().flex_1().child(
        view()
            .w(px(160.0))
            .h(px(16.0))
            .flex_row()
            .accessibility_state(AccessibilityState {
                selected: true,
                ..Default::default()
            })
            .child(text("ORBX").w(px(48.0)))
            .child(
                canvas(
                    Painter::new()
                        .fill_color(nsh_core::color::rgb(0xef4444))
                        .fill_rect(0.0, 0.0, 16.0, 16.0),
                )
                .label("trend")
                .w(px(16.0))
                .h(px(16.0)),
            ),
    )
}

#[test]
fn canvas_cells_inside_a_reversed_view_keep_their_palette_color_unreversed() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 2)).unwrap();
    view.load(App::Rust(Root::new::<SelectedChartRow>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ORBX");
    let buffer = terminal.backend().buffer();
    assert!(reversed(buffer, 0, 0), "the row's text");
    assert!(reversed(buffer, 9, 0), "the row's empty Cells");
    for x in 6..8 {
        assert_eq!(buffer[(x, 0)].symbol(), "⣿");
        assert_eq!(buffer[(x, 0)].fg, Color::Red);
        assert!(!reversed(buffer, x, 0), "the chart at {x}");
    }
}
