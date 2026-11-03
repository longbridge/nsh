use nsh_core::prelude::*;
use nsh_platform_tui::{App, Handled, Input, Root};
use ratatui_core::{
    backend::TestBackend,
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

/// Whether the focus mark covers `(x, y)`: a reversed label, or a bold,
/// underlined one where the view is already a band.
fn focus_marked(buffer: &ratatui_core::buffer::Buffer, x: u16, y: u16) -> bool {
    let modifier = buffer[(x, y)].modifier;
    modifier.contains(Modifier::REVERSED)
        || modifier.contains(Modifier::BOLD | Modifier::UNDERLINED)
}

/// Waits for the slider's thumb (`●`) to turn the accent: the slider marks
/// its focus itself (a terminal marks no Adjustable view's), in the batch
/// that also binds its keys.
pub(super) fn wait_for_thumb_focus(
    view: &mut nsh_platform_tui::AppView,
    terminal: &mut Terminal<TestBackend>,
    rx: &crossbeam_channel::Receiver<()>,
    idle: ratatui_core::style::Color,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if thumb_color(terminal).is_some_and(|fg| fg != idle) {
            return;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "slider focus did not render"
        );
    }
}

/// The color of the slider's thumb (`●`), if one is drawn.
pub(super) fn thumb_color(terminal: &Terminal<TestBackend>) -> Option<ratatui_core::style::Color> {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .find(|cell| cell.symbol() == "●")
        .map(|cell| cell.fg)
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
        let (row, other) = if name == "first" { (1, 2) } else { (2, 1) };
        assert!(focus_marked(buffer, 0, row), "{name} is marked");
        assert!(!focus_marked(buffer, 0, other), "only {name} is marked");
        assert_eq!(
            buffer[(0, 0)].bg,
            Color::Reset,
            "the group's fill draws nothing"
        );
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

/// The fills of the first `width` Cells of row `y`: the terminal's own
/// background, whatever the parts' state styles fill.
fn unfilled(buffer: &ratatui_core::buffer::Buffer, y: u16, width: u16) -> bool {
    (0..width).all(|x| buffer[(x, y)].bg == Color::Reset)
}

#[test]
fn checkbox_and_switch_project_value_and_disabled_states_to_their_parts() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(64, 8)).unwrap();
    view.load(App::Rust(Root::new::<ValueControls>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "mixed true");
    // The parts' state styles fill nothing in the terminal; the controls
    // keep their behavior.
    assert!(unfilled(terminal.backend().buffer(), 0, 4));
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "check true mixed false");
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "switch true");
    assert!(unfilled(terminal.backend().buffer(), 1, 4));
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "disabled true");
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
    let idle = thumb_color(&terminal).expect("the thumb is drawn");
    view.handle(key("Tab"));
    wait_for_thumb_focus(&mut view, &mut terminal, &rx, idle);
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
