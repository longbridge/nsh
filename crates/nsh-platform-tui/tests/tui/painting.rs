//! The terminal's own look: borders in box drawing, dim unless the app
//! colored them, no fills, shadows, outlines or rings, scroll bars, content
//! nothing for what the terminal cannot show, the state of selected, focused and disabled views, and
//! overlays over what is behind them.

use nsh_core::{AccessibilityState, BoxShadow, prelude::*};
use nsh_platform_tui::{App, Input, Root};
use ratatui_core::{
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier},
    terminal::Terminal,
};

use super::app::{draw_until, reversed, selected, surface};

fn row(buffer: &Buffer, y: u16, columns: std::ops::Range<u16>) -> String {
    columns.map(|x| buffer[(x, y)].symbol()).collect()
}

fn dim(buffer: &Buffer, x: u16, y: u16) -> bool {
    buffer[(x, y)].modifier.contains(Modifier::DIM)
}

fn key(name: &str) -> Input {
    Input::Key {
        key: name.into(),
        modifiers: Default::default(),
        release: false,
    }
}

/// A blank row of content: what makes a bordered box a container, which
/// the terminal layout gives its border.
fn blank_row() -> impl IntoElement {
    view().child(text(" "))
}

#[component]
fn Borders() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .w(px(48.0))
                .h(px(48.0))
                .border_1()
                .border_t_color(nsh_core::color::rgb(0xff0000))
                .border_r_color(nsh_core::color::rgb(0x00ff00))
                .border_b_color(nsh_core::color::rgb(0x0000ff))
                .border_l_color(nsh_core::color::rgb(0xffffff))
                .bg(nsh_core::color::rgb(0x123456))
                .py(px(16.0))
                .child(blank_row()),
        )
        .child(
            view()
                .absolute()
                .left(px(64.0))
                .top(px(0.0))
                .w(px(48.0))
                .h(px(48.0))
                .border_1()
                .border_dashed()
                .border_color(nsh_core::color::rgb(0xffffff))
                .py(px(16.0))
                .child(blank_row()),
        )
        .child(
            view()
                .absolute()
                .left(px(128.0))
                .top(px(0.0))
                .w(px(48.0))
                .h(px(48.0))
                .border_1()
                .border_dotted()
                .border_color(nsh_core::color::rgb(0xffffff))
                .py(px(16.0))
                .child(blank_row()),
        )
        .child(
            view()
                .absolute()
                .left(px(0.0))
                .top(px(64.0))
                .w(px(80.0))
                .h(px(32.0))
                .border_b(px(1.0))
                .border_color(nsh_core::color::rgb(0xffffff))
                .py(px(16.0))
                .child(blank_row()),
        )
        .child(view().absolute().top(px(112.0)).child(text("ready")))
}

#[test]
fn borders_draw_box_lines_dim_or_in_the_accent_of_each_side() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
    view.load(App::Rust(Root::new::<Borders>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 0, 0..6), "┌────┐");
    assert_eq!(row(buffer, 1, 0..6), "│    │");
    assert_eq!(row(buffer, 2, 0..6), "└────┘");
    assert_eq!(buffer[(2, 0)].fg, Color::Red, "top");
    assert_eq!(buffer[(5, 1)].fg, Color::Green, "right");
    assert_eq!(buffer[(2, 2)].fg, Color::Blue, "bottom");
    assert_eq!(buffer[(0, 1)].fg, Color::Reset, "left");
    assert!(dim(buffer, 0, 1), "a neutral side is dim");
    assert!(!dim(buffer, 2, 0), "an accent is not");
    for (x, y) in [(0, 0), (2, 0), (0, 1), (2, 1), (5, 2)] {
        assert_eq!(buffer[(x, y)].bg, Color::Reset, "no fill at {x},{y}");
    }
}

#[test]
fn dashed_and_dotted_borders_draw_their_lines() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
    view.load(App::Rust(Root::new::<Borders>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 0, 8..14), "┌╌╌╌╌┐");
    assert_eq!(row(buffer, 1, 8..14), "╎    ╎");
    assert_eq!(row(buffer, 0, 16..22), "┌┈┈┈┈┐");
    assert_eq!(row(buffer, 1, 16..22), "┊    ┊");
    assert_eq!(buffer[(8, 1)].bg, Color::Reset, "a box without a fill");
}

#[test]
fn a_border_on_one_side_is_only_its_line() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
    view.load(App::Rust(Root::new::<Borders>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 4, 0..11), "           ");
    assert_eq!(row(buffer, 5, 0..11), "────────── ");
}

#[component]
fn Shadows() -> impl IntoElement {
    let drop = BoxShadow {
        offset_x: px(8.0),
        offset_y: px(16.0),
        color: nsh_core::color::rgba(0x00000080).into(),
        ..BoxShadow::default()
    };
    let soft = BoxShadow {
        blur: px(24.0),
        color: nsh_core::color::rgba(0x000000ff).into(),
        ..BoxShadow::default()
    };
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0xffffff))
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(16.0))
                .w(px(32.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x123456))
                .shadow([drop]),
        )
        .child(
            view()
                .absolute()
                .left(px(96.0))
                .top(px(16.0))
                .w(px(32.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x123456))
                .shadow([soft]),
        )
        .child(view().absolute().top(px(80.0)).child(text("ready")))
}

#[test]
fn shadows_draw_nothing() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
    view.load(App::Rust(Root::new::<Shadows>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    for y in 0..4 {
        for x in 0..20 {
            let cell = &buffer[(x, y)];
            assert_eq!((cell.symbol(), cell.bg), (" ", Color::Reset), "{x},{y}");
        }
    }
}

#[component]
fn Outlines() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(16.0))
                .w(px(32.0))
                .h(px(16.0))
                .outline(px(2.0))
                .outline_color(nsh_core::color::rgb(0xff0000))
                .child(text("out")),
        )
        .child(
            view()
                .absolute()
                .left(px(96.0))
                .top(px(16.0))
                .w(px(48.0))
                .h(px(48.0))
                .border_1()
                .border_color(nsh_core::color::rgb(0xffffff))
                .ring(px(2.0))
                .ring_color(nsh_core::color::rgb(0x00ff00))
                .py(px(16.0))
                .child(blank_row()),
        )
        .child(view().absolute().top(px(96.0)).child(text("ready")))
}

#[test]
fn outlines_and_rings_draw_nothing_of_their_own() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
    view.load(App::Rust(Root::new::<Outlines>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(
        row(buffer, 0, 1..7),
        "      ",
        "no box around the outline's view"
    );
    assert_eq!(row(buffer, 1, 1..7), " out  ");
    assert_eq!(row(buffer, 1, 12..18), "┌────┐", "the border as it is");
    assert_eq!(buffer[(12, 1)].fg, Color::Reset);
    assert!(dim(buffer, 12, 1), "not the ring's color");
}

#[component]
fn Scrolled() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            scroll()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .w(px(80.0))
                .h(px(32.0))
                .children(
                    ["one", "two", "three", "four"]
                        .map(|name| view().h(px(16.0)).flex_shrink_0().child(text(name))),
                ),
        )
        .child(
            scroll()
                .absolute()
                .left(px(96.0))
                .top(px(0.0))
                .w(px(80.0))
                .h(px(32.0))
                .scrollbar(false)
                .children(
                    ["five", "six", "seven"]
                        .map(|name| view().h(px(16.0)).flex_shrink_0().child(text(name))),
                ),
        )
        .child(view().absolute().top(px(48.0)).child(text("ready")))
}

#[test]
fn a_scroll_view_whose_content_overflows_shows_its_bar() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 4)).unwrap();
    view.load(App::Rust(Root::new::<Scrolled>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    // Half the content is in view: a thumb one Cell long at the top.
    assert_eq!(buffer[(9, 0)].symbol(), "┃");
    assert!(!dim(buffer, 9, 0), "the thumb in the default color");
    assert_eq!(buffer[(9, 1)].symbol(), "│");
    assert!(dim(buffer, 9, 1), "the track is dim");
    assert_eq!(buffer[(0, 2)].symbol(), " ", "clipped to the view");
    for y in 0..2 {
        assert_eq!(buffer[(21, y)].symbol(), " ", "the bar the app hides");
    }
}

#[component]
fn Content() -> impl IntoElement {
    const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAACCAIAAAAW4yFwAAAADUlEQVR4nGP4zwAC/wEIAAH/2ZC7NQAAAABJRU5ErkJggg==";
    view()
        .flex_1()
        .child(
            image(PNG)
                .label("Chart")
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x123456)),
        )
        .child(image(PNG).w(px(80.0)).h(px(16.0)))
        .child(text("ready"))
}

#[test]
fn images_are_left_out() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<Content>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let drawn: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert_eq!(drawn.replace(' ', ""), "ready", "no placeholder: {drawn:?}");
    assert!(buffer.content.iter().all(|cell| cell.bg == Color::Reset));
}

#[component]
fn Small() -> impl IntoElement {
    let bordered = || {
        view()
            .border_1()
            .border_color(nsh_core::color::rgb(0xffffff))
    };
    view()
        .flex_1()
        .flex_row()
        .child(bordered().w(px(16.0)).h(px(32.0)).child(text("A")))
        .child(
            bordered()
                .w(px(32.0))
                .h(px(64.0))
                .rounded(px(16.0))
                .child(text("JD")),
        )
        .child(bordered().w(px(48.0)).h(px(64.0)))
        .child(text("ready"))
}

#[test]
fn tiny_boxes_circles_and_empty_boxes_draw_no_border() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
    view.load(App::Rust(Root::new::<Small>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let lines = "─│┌┐└┘╭╮╰╯";
    for y in 0..6 {
        for x in 0..24 {
            assert!(
                !lines.contains(buffer[(x, y)].symbol()),
                "a border fragment at {x},{y}"
            );
        }
    }
    let drawn: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(drawn.contains('A') && drawn.contains("JD"), "{drawn:?}");
}

#[component]
fn Tile() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(64.0))
                .h(px(48.0))
                .border_1()
                .border_color(nsh_core::color::rgb(0xffffff))
                .py(px(16.0))
                .child(
                    view()
                        .flex_row()
                        .child(text("Until 7:00 tonight").w(px(240.0))),
                ),
        )
        .child(text("ready"))
}

#[test]
fn children_are_clipped_to_their_parents_content_box_with_an_ellipsis() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
    view.load(App::Rust(Root::new::<Tile>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..6).map(|y| row(buffer, y, 0..12)).collect();
    assert_eq!(rows[0], "┌──────┐    ", "{rows:#?}");
    assert!(rows[1].starts_with("│Until"), "{rows:#?}");
    // The text wraps to the box (the layout shrinks it before it overflows);
    // what the box cuts off below ends the last row shown in `…`.
    let bottom = rows
        .iter()
        .position(|r| r.starts_with('└'))
        .expect("the box's bottom");
    let inside = rows[1..bottom].concat();
    assert!(
        inside.contains("tonight") || inside.contains('…'),
        "a cut text shows it was cut: {rows:#?}"
    );
    assert!(
        rows[1..bottom]
            .iter()
            .all(|r| r.chars().nth(7) == Some('│')),
        "{rows:#?}"
    );
}

#[component]
fn States() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .accessibility_state(selected())
                .child(text("row")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .on_press(|_: &()| {})
                .child(text("go")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                // `accessibility_state` leaves `disabled` to `disabled()`.
                .disabled(true)
                .child(text("off").text_color(nsh_core::color::rgb(0xffffff))),
        )
        .child(text("ready"))
}

#[test]
fn selected_views_are_bands_focused_ones_reversed_labels_disabled_ones_dim() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 5)).unwrap();
    view.load(App::Rust(Root::new::<States>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    for x in 0..10 {
        assert!(reversed(buffer, x, 0), "the band's full width at {x}");
    }
    assert!(!reversed(buffer, 10, 0), "outside the selected view");
    assert!(!reversed(buffer, 0, 1), "nothing is focused yet");
    let rows: Vec<String> = (0..5).map(|y| row(buffer, y, 0..12)).collect();
    assert!(dim(buffer, 0, 2), "disabled: {rows:#?}");
    view.handle(key("Tab"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if reversed(terminal.backend().buffer(), 0, 1) {
            break;
        }
        assert!(rx.recv_deadline(deadline).is_ok(), "focus did not show");
    }
    let buffer = terminal.backend().buffer();
    assert!(reversed(buffer, 1, 1), "the label");
    assert!(
        !reversed(buffer, 2, 1),
        "only the label, not the view's width"
    );
}

#[component]
fn Buttons() -> impl IntoElement {
    let button = |label: &'static str, fill: u32| {
        view()
            .w(px(80.0))
            .h(px(16.0))
            .border_1()
            .border_color(nsh_core::color::rgb(0xffffff))
            .rounded(px(4.0))
            .bg(nsh_core::color::rgb(fill))
            .on_press(|_: &()| {})
            .child(text(label).text_color(nsh_core::color::rgb(0xffffff)))
    };
    view()
        .flex_1()
        .child(button("Save", 0x2563eb))
        .child(button("Cancel", 0x18181b))
        .child(text("ready"))
}

#[test]
fn controls_are_their_label_rows_and_a_standing_out_fill_is_bold_in_its_accent() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<Buttons>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(
        row(buffer, 0, 0..10),
        "Save      ",
        "no border on a control"
    );
    assert_eq!(row(buffer, 1, 0..10), "Cancel    ");
    assert_eq!(buffer[(0, 0)].fg, Color::Blue, "the fill's accent");
    assert!(buffer[(0, 0)].modifier.contains(Modifier::BOLD));
    assert!(!reversed(buffer, 0, 0), "distinct from focus");
    assert_eq!(buffer[(0, 0)].bg, Color::Reset);
    assert_eq!(buffer[(0, 1)].fg, Color::Reset, "a quiet fill");
    assert!(!buffer[(0, 1)].modifier.contains(Modifier::BOLD));
}

#[test]
fn an_unchanged_frame_changes_no_cell() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
    view.load(App::Rust(Root::new::<Borders>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let before = terminal.backend().buffer().clone();
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    assert_eq!(terminal.backend().buffer(), &before);
}

#[component]
fn Popup() -> impl IntoElement {
    let anchor = element_ref();
    view()
        .flex_1()
        .child(view().w(px(8.0)).h(px(16.0)).element_ref(&anchor))
        .child(text("behind behind behind"))
        .child(text("behind behind behind"))
        .child(text("behind behind behind"))
        .child(
            overlay()
                .anchor(&anchor)
                .placement(nsh_core::overlay::Placement::Bottom)
                .align(nsh_core::overlay::Align::Start)
                .child(
                    view()
                        .w(px(80.0))
                        .h(px(32.0))
                        .bg(nsh_core::color::rgb(0x123456))
                        .label("Menu")
                        .child(text("pop")),
                ),
        )
        .child(text("ready"))
}

#[test]
fn an_overlays_box_clears_what_is_behind_it_and_is_framed_with_its_title() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
    view.load(App::Rust(Root::new::<Popup>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "pop");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..8).map(|y| row(buffer, y, 0..24)).collect();
    let y = rows.iter().position(|r| r.contains("pop")).unwrap();
    let x = rows[y].find("pop").unwrap() as u16;
    let y = y as u16;
    assert_eq!(row(buffer, y, x..x + 10), "pop       ", "{rows:#?}");
    // The box is its content's row; its frame stands one Cell around it,
    // over what was behind.
    assert_eq!(
        buffer[(x + 10, y)].symbol(),
        "│",
        "a frame around the box: {rows:#?}"
    );
    assert_eq!(row(buffer, y + 1, x..x + 10), "──────────", "{rows:#?}");
    assert!(
        rows[usize::from(y) - 1].contains("Menu"),
        "its title: {rows:#?}"
    );
}

#[component]
fn Hidden() -> impl IntoElement {
    view()
        .flex_1()
        .child(view().invisible().child(text("gone")))
        .child(view().opacity(0.0).child(text("clear")))
        .child(text("ready"))
}

#[test]
fn invisible_and_fully_transparent_views_draw_nothing() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<Hidden>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let contents: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(!contents.contains("gone"));
    assert!(!contents.contains("clear"));
}

#[component]
fn IconButton() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view().label(" More ").on_press(|_: &()| {}).child(
                canvas(Painter::new().fill_rect(0.0, 0.0, 16.0, 16.0))
                    .w(px(16.0))
                    .h(px(16.0)),
            ),
        )
        .child(text("ready"))
}

#[test]
fn an_icon_button_shows_its_accessibility_label() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<IconButton>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 0, 0..6), "More  ", "the label, trimmed");
    assert_eq!(buffer[(0, 0)].fg, Color::Reset);
    view.handle(key("Tab"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if reversed(terminal.backend().buffer(), 0, 0) {
            break;
        }
        assert!(rx.recv_deadline(deadline).is_ok(), "focus did not show");
    }
    assert!(
        reversed(terminal.backend().buffer(), 3, 0),
        "focused, reversed"
    );
}

#[component]
fn CrowdedIcons() -> impl IntoElement {
    let icon = |label: &'static str| {
        view().label(label).on_press(|_: &()| {}).child(
            canvas(Painter::new().fill_rect(0.0, 0.0, 16.0, 16.0))
                .w(px(16.0))
                .h(px(16.0)),
        )
    };
    view()
        .flex_1()
        .child(
            view()
                .w(px(80.0))
                .flex_row()
                .child(text("Title"))
                .child(icon("Add item"))
                .child(icon("Remove")),
        )
        .child(text("ready"))
}

#[test]
fn an_icon_button_short_of_room_cuts_its_label_with_an_ellipsis() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(16, 3)).unwrap();
    view.load(App::Rust(Root::new::<CrowdedIcons>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let first = row(buffer, 0, 0..16);
    assert!(first.starts_with("Title"), "{first:?}");
    assert!(
        first.contains('…'),
        "a cut label ends in an ellipsis: {first:?}"
    );
    assert!(
        !first[..first.find('…').unwrap()].ends_with(' '),
        "{first:?}"
    );
}

#[component]
fn Disclosures() -> impl IntoElement {
    let trigger = |label: &'static str, state: AccessibilityState| {
        view()
            .h(px(16.0))
            .flex_row()
            .role(AccessibilityRole::Button)
            .accessibility_state(state)
            .on_press(|_: &()| {})
            .child(view().w(px(16.0)))
            .child(text(label))
    };
    view()
        .flex_1()
        .child(trigger(
            "Storage",
            AccessibilityState {
                expanded: Some(false),
                ..AccessibilityState::default()
            },
        ))
        .child(trigger(
            "Advanced",
            AccessibilityState {
                expanded: Some(true),
                ..AccessibilityState::default()
            },
        ))
        .child(trigger(
            "Options",
            AccessibilityState {
                expanded: Some(false),
                has_popup: true,
                ..AccessibilityState::default()
            },
        ))
        .child(text("ready"))
}

#[test]
fn a_disclosure_says_which_way_it_goes_and_a_popup_trigger_does_not() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
    view.load(App::Rust(Root::new::<Disclosures>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 0, 0..9), "▸ Storage");
    assert_eq!(row(buffer, 1, 0..10), "▾ Advanced");
    assert_eq!(
        row(buffer, 2, 0..9),
        "  Options",
        "a popup's trigger has no disclosure"
    );
}
