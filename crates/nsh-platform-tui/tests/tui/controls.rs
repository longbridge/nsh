use nsh_core::prelude::*;
use nsh_platform_tui::{App, Handled, Input, Root};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

use super::app::{barred, draw_until, reversed, selected, surface};

fn key(name: &str) -> Input {
    Input::Key {
        key: name.into(),
        modifiers: Default::default(),
        release: false,
    }
}

#[component]
fn OutlinedButton() -> impl IntoElement {
    let pressed = signal(false);
    view().flex_1().child(
        nsh_base::button()
            .absolute()
            .left(px(16.0))
            .top(px(32.0))
            .w(px(80.0))
            .h(px(48.0))
            .focus(|s| {
                s.outline(px(2.0))
                    .outline_offset(px(-2.0))
                    .outline_color(nsh_core::color::rgb(0x123456))
            })
            .on_press(move |_: &()| pressed.set(true))
            .child(text(
                move || if pressed.get() { "pressed" } else { "button" },
            )),
    )
}

#[test]
fn a_focused_control_shows_the_bar_and_its_app_outline_is_not_drawn() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    view.load(App::Rust(Root::new::<OutlinedButton>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "button");
    assert_eq!(view.handle(key("Tab")), Handled::Yes);
    assert_eq!(view.handle(key("Enter")), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "pressed");
    // Focus puts the bar on the text row across the button's own 10 by 3
    // Cell box; its outline is the app's focus style, which the terminal's
    // look leaves out.
    let buffer = terminal.backend().buffer();
    for (x, y) in [(2, 2), (11, 2)] {
        assert!(barred(buffer, x, y), "({x}, {y})");
        assert_eq!(buffer[(x, y)].fg, Color::Reset);
    }
    for (x, y) in [(2, 4), (11, 4)] {
        assert!(!barred(buffer, x, y), "no bar on the blank row ({x}, {y})");
        assert_eq!(buffer[(x, y)].fg, Color::Reset);
    }
    for (x, y) in [(1, 2), (12, 2), (2, 1), (2, 5)] {
        assert!(!barred(buffer, x, y), "({x}, {y})");
        assert_eq!(buffer[(x, y)].symbol(), " ");
    }
    assert!(!terminal.backend().cursor_visible());
}

#[component]
fn ScaledOutline() -> impl IntoElement {
    view().flex_1().child(
        view()
            .absolute()
            .left(px(16.0))
            .top(px(32.0))
            .w(px(80.0))
            .h(px(48.0))
            .origin_top_left()
            .scale(2.0)
            .outline(px(2.0))
            .outline_offset(px(-2.0))
            .outline_color(nsh_core::color::rgb(0x123456))
            .accessibility_state(selected())
            .child(text("scaled")),
    )
}

#[test]
fn a_bar_follows_the_controls_transformed_bounds_without_its_outline() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    view.load(App::Rust(Root::new::<ScaledOutline>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "scaled");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(2, 2)].symbol(), "s");
    assert!(barred(buffer, 2, 2) && barred(buffer, 21, 2));
    assert_eq!(buffer[(21, 2)].symbol(), " ");
    assert!(!barred(buffer, 1, 2) && !barred(buffer, 22, 2));
    assert!(!barred(buffer, 21, 7), "no bar on the blank rows");
}

#[component]
fn ThinProgress() -> impl IntoElement {
    let value = signal(0.0f32);
    view()
        .flex_1()
        .child(
            nsh_base::progress()
                .w(px(160.0))
                .value(value)
                .track_class("h-[6px] bg-[#123456] overflow-hidden")
                .indicator_class("h-[6px] bg-[#abcdef]"),
        )
        .child(
            nsh_base::button()
                .absolute()
                .top(px(32.0))
                .h(px(16.0))
                .on_press(move |_: &()| value.set(50.0))
                .child(text(move || format!("progress {}", value.get()))),
        )
}

#[test]
fn a_subcell_progress_indicator_is_a_rule_and_zero_progress_stays_empty() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    view.load(App::Rust(Root::new::<ThinProgress>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "progress 0");
    assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), " ");
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "progress 50");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(9, 0)].symbol(), "─");
    assert_eq!(buffer[(9, 0)].fg, Color::Blue);
    assert_eq!(buffer[(9, 0)].bg, Color::Reset);
    assert_eq!(
        buffer[(10, 0)].symbol(),
        " ",
        "the rail is a container: no fill"
    );
}

#[component]
fn FocusGroup() -> impl IntoElement {
    let focused = signal("none");
    view().flex_1().child(
        view()
            .group()
            .child(
                view()
                    .h(px(16.0))
                    .w(px(80.0))
                    .group_focus_within(|s| s.bg(nsh_core::color::rgb(0x123456))),
            )
            .children(["first", "second"].map(|name| {
                view()
                    .h(px(16.0))
                    .w(px(80.0))
                    .focus(|s| s.bg(nsh_core::color::rgb(0xabcdef)))
                    .on_press(|_: &()| {})
                    .on_focus(move |_: &()| focused.set(name))
                    .child(text(name))
            }))
            .child(text(move || format!("focus {}", focused.get()))),
    )
}

#[test]
fn moving_back_between_controls_keeps_the_groups_focus_within_style() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    view.load(App::Rust(Root::new::<FocusGroup>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "focus none");
    for (name, backwards) in [("first", false), ("second", false), ("first", true)] {
        let mut input = key("Tab");
        if let Input::Key { modifiers, .. } = &mut input {
            modifiers.shift = backwards;
        }
        view.handle(input);
        draw_until(&mut view, &mut terminal, &rx, &format!("focus {name}"));
        let buffer = terminal.backend().buffer();
        assert!(reversed(buffer, 0, 0));
        assert_eq!(buffer[(0, 0)].fg, Color::Blue);
    }
}

#[component]
fn ValueControls() -> impl IntoElement {
    let checked = signal(false);
    let mixed = signal(true);
    let switched = signal(false);
    let disabled = signal(false);
    view()
        .flex_1()
        .child(
            nsh_base::checkbox()
                .checked(checked)
                .indeterminate(mixed)
                .h(px(16.0))
                .w(px(80.0))
                .flex_row()
                .indicator_class(
                    "w-2 h-4 bg-[#111111] checked:bg-[#123456] indeterminate:bg-[#abcdef]",
                )
                .on_change(move |next: &bool| {
                    checked.set(*next);
                    mixed.set(false);
                })
                .child(text("check")),
        )
        .child(
            nsh_base::switch()
                .checked(switched)
                .disabled(disabled)
                .h(px(16.0))
                .w(px(80.0))
                .flex_row()
                .track_class("w-4 h-4 bg-[#111111] checked:bg-[#123456] disabled:bg-[#abcdef]")
                .thumb_class("w-2 h-4 bg-[#333333] checked:bg-[#654321] disabled:bg-[#fedcba]")
                .on_change(move |next: &bool| switched.set(*next))
                .child(text("switch")),
        )
        .child(
            nsh_base::button()
                .h(px(16.0))
                .on_press(move |_: &()| disabled.set(true))
                .child(text("disable")),
        )
        .child(text(move || {
            format!(
                "check {} mixed {} switch {} disabled {}",
                checked.get(),
                mixed.get(),
                switched.get(),
                disabled.get()
            )
        }))
}

#[test]
fn checkbox_and_switch_project_value_and_disabled_states_to_their_parts() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(64, 8)).unwrap();
    view.load(App::Rust(Root::new::<ValueControls>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "mixed true");
    // The 1-Cell indicators show the glyphs' short forms in their palette
    // colors; checked is a glyph, not reverse.
    let line = |buffer: &ratatui_core::buffer::Buffer, y: u16| -> String {
        (0..12).map(|x| buffer[(x, y)].symbol()).collect()
    };
    let buffer = terminal.backend().buffer();
    assert!(
        line(buffer, 0).starts_with("-check"),
        "{:?}",
        line(buffer, 0)
    );
    assert_eq!(buffer[(0, 0)].fg, Color::Blue);
    assert!(!reversed(buffer, 2, 0));
    assert!(line(buffer, 1).starts_with("  switch"), "no room for off");
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "check true mixed false");
    view.handle(key("Tab"));
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(
        line(buffer, 0).starts_with("xcheck"),
        "{:?}",
        line(buffer, 0)
    );
    assert!(!reversed(buffer, 2, 0), "checked is not reversed");
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "switch true");
    let buffer = terminal.backend().buffer();
    assert!(
        line(buffer, 1).starts_with("onswitch"),
        "{:?}",
        line(buffer, 1)
    );
    assert_eq!(buffer[(0, 1)].fg, Color::Blue, "the checked track's color");
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "disabled true");
    let buffer = terminal.backend().buffer();
    assert!(
        line(buffer, 1).starts_with("onswitch"),
        "a disabled switch stays on"
    );
    assert!(!reversed(buffer, 3, 1));
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(
        &mut view,
        &mut terminal,
        &rx,
        "check false mixed false switch true disabled true",
    );
}

#[component]
fn KeyboardSlider() -> impl IntoElement {
    let value = signal(nsh_base::SliderValue::Single(60.0));
    let ended = signal(0.0);
    let presses = signal(0);
    view()
        .flex_1()
        .child(
            nsh_base::slider()
                .w(px(160.0))
                .value(move || Some(value.get()))
                .step(5.0)
                .track_class("h-8")
                .indicator_class("h-2 bg-[#123456]")
                .thumb_class("w-4 h-2 bg-[#abcdef] focus:bg-[#654321]")
                .on_change(move |next: &nsh_base::SliderValue| value.set(*next))
                .on_change_end(move |next: &nsh_base::SliderValue| ended.set(next.end())),
        )
        .child(
            nsh_base::button()
                .h(px(16.0))
                .on_press(move |_: &()| presses.update(|n| *n += 1))
                .child(text("after slider")),
        )
        .child(text(move || {
            format!(
                "value {} ended {} presses {}",
                value.get().end(),
                ended.get(),
                presses.get()
            )
        }))
}

#[test]
fn a_focused_slider_handles_arrows_pages_and_endpoints_then_releases_them_on_tab() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 8)).unwrap();
    view.load(App::Rust(Root::new::<KeyboardSlider>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "value 60");
    view.handle(key("Tab"));
    // Enter is a no-op thumb press, allowing its focus and keymap batch to arrive.
    view.handle(key("Enter"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if terminal.backend().buffer().content.iter().any(|cell| {
            cell.fg == Color::Yellow
                && cell
                    .modifier
                    .contains(ratatui_core::style::Modifier::REVERSED)
        }) {
            break;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "slider focus did not render"
        );
    }
    for (name, value) in [
        ("Right", 65),
        ("Left", 60),
        ("PageUp", 70),
        ("PageDown", 60),
        ("End", 100),
        ("Home", 0),
    ] {
        assert_eq!(view.handle(key(name)), Handled::Yes);
        draw_until(
            &mut view,
            &mut terminal,
            &rx,
            &format!("value {value} ended {value}"),
        );
    }
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "value 0 ended 0 presses 1");
    assert_eq!(view.handle(key("Home")), Handled::Ignored);
}

#[component]
fn SelectedAccentBars() -> impl IntoElement {
    let selected = signal(0usize);
    let accent = nsh_core::color::rgb(0xc8f26d);
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x101113))
        .children((0..2).map(|index| {
            view()
                .absolute()
                .left(px(16.0))
                .top(px(16.0 + index as f32 * 48.0))
                .w(px(160.0))
                .h(px(36.0))
                .rounded(px(8.0))
                .bg(nsh_core::color::rgb(0x24262a))
                .flex_row()
                .items_center()
                .px(px(10.0))
                .on_press(move |_: &()| selected.set(index))
                .child(
                    view()
                        .absolute()
                        .left(px(0.0))
                        .top(px(9.0))
                        .w(px(3.0))
                        .h(px(18.0))
                        .rounded(px(1.5))
                        .bg(accent)
                        .opacity(move || {
                            if selected.get() == index {
                                1.0_f32
                            } else {
                                0.0_f32
                            }
                        }),
                )
                .child(text(format!("Sidebar {index}")))
        }))
        .child(
            view()
                .absolute()
                .left(px(216.0))
                .top(px(16.0))
                .w(px(160.0))
                .h(px(64.0))
                .bg(nsh_core::color::rgb(0x24262a))
                .flex_row()
                .items_center()
                .px(px(16.0))
                .child(
                    view()
                        .absolute()
                        .left(px(0.0))
                        .top(px(14.0))
                        .bottom(px(14.0))
                        .w(px(3.0))
                        .rounded(px(1.5))
                        .bg(accent),
                )
                .child(text("Watchlist")),
        )
        .child(
            view()
                .absolute()
                .top(px(128.0))
                .child(text(move || format!("active {}", selected.get()))),
        )
}

#[test]
fn sidebar_and_watchlist_accents_remain_visible_with_their_authored_heights() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    view.load(App::Rust(Root::new::<SelectedAccentBars>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "active 0");
    let marked =
        |cell: &ratatui_core::buffer::Cell| cell.fg == Color::Green && cell.symbol() == "│";
    let buffer = terminal.backend().buffer();
    assert!(marked(&buffer[(2, 2)]));
    assert!(!marked(&buffer[(2, 1)]));
    assert!(!marked(&buffer[(2, 3)]));
    assert!(!marked(&buffer[(2, 5)]));
    assert!(marked(&buffer[(27, 2)]));
    assert!(marked(&buffer[(27, 3)]));
    assert!(!marked(&buffer[(27, 1)]));
    assert!(!marked(&buffer[(27, 4)]));
    view.handle(key("Tab"));
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "active 1");
    let buffer = terminal.backend().buffer();
    assert!(!marked(&buffer[(2, 2)]));
    assert!(marked(&buffer[(2, 5)]));
    assert!(marked(&buffer[(27, 2)]));
    assert!(marked(&buffer[(27, 3)]));
}
