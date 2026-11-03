//! Checkboxes, radios and switches show their state with terminal glyphs.

use nsh_core::{AccessibilityRole, AccessibilityState, Checked, prelude::*};
use nsh_platform_tui::{App, AppView, Handled, Input, Root};
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

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height).map(|y| row(buffer, y)).collect()
}

fn reversed_anywhere(buffer: &Buffer) -> bool {
    buffer
        .content
        .iter()
        .any(|cell| cell.modifier.contains(Modifier::REVERSED))
}

#[component]
fn Checkboxes() -> impl IntoElement {
    let states = [(false, false), (true, false), (false, true)];
    view().flex_1().children(states.map(|(checked, mixed)| {
        nsh_base::checkbox()
            .checked(checked)
            .indeterminate(mixed)
            .flex_row()
            .items_center()
            .pl(px(8.0))
            .gap(px(8.0))
            .h(px(16.0))
            .indicator_class("w-4 h-4 border border-[#8d9198] checked:bg-[#c8f26d]")
            .child(text(match (checked, mixed) {
                (true, _) => "Photos",
                (_, true) => "Select all",
                _ => "Videos",
            }))
    }))
}

#[test]
fn checkboxes_draw_their_state_in_place_of_the_indicator() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 3)).unwrap();
    view.load(App::Rust(Root::new::<Checkboxes>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Select all");
    let buffer = terminal.backend().buffer();
    // The glyph ends one Cell before the label, moving left into the
    // control's blank padding; the label keeps its column.
    assert_eq!(rows(buffer), ["[ ] Videos", "[x] Photos", "[-] Select all"]);
    assert!(
        !reversed_anywhere(buffer),
        "checked is a glyph, not reverse"
    );
    assert_eq!(buffer[(1, 1)].fg, Color::Green, "the checked box's color");
    assert!(
        buffer[(1, 0)].modifier.contains(Modifier::DIM)
            || buffer[(0, 0)].modifier.contains(Modifier::DIM)
    );
}

#[component]
fn Radios() -> impl IntoElement {
    let plan = signal("Plus");
    let option = move |name: &'static str| {
        nsh_base::radio()
            .checked(move || plan.get() == name)
            .on_change(move |_: &bool| plan.set(name))
            .flex_row()
            .items_center()
            .gap(px(16.0))
            .h(px(32.0))
            .child(
                view()
                    .size(px(16.0))
                    .rounded_full()
                    .border_1()
                    .border_color(nsh_core::color::rgb(0x8d9198))
                    .items_center()
                    .justify_center()
                    .child(
                        view()
                            .size(px(8.0))
                            .rounded_full()
                            .bg(nsh_core::color::rgb(0x7ea8ff))
                            .opacity(move || if plan.get() == name { 1.0_f32 } else { 0.0 }),
                    ),
            )
            .child(view().child(text(name)).child(text("detail")))
    };
    nsh_base::radio_group()
        .child(option("Free"))
        .child(option("Plus"))
        .child(option("Team"))
}

#[test]
fn radios_put_their_glyphs_in_one_column_and_focus_puts_the_bar_on_them() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    view.load(App::Rust(Root::new::<Radios>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Team");
    let buffer = terminal.backend().buffer();
    // The ring is centred on its two-line label; the glyph takes the label's
    // first row.
    let titles = [
        "( ) Free",
        "    detail",
        "(•) Plus",
        "    detail",
        "( ) Team",
        "    detail",
    ];
    assert_eq!(rows(buffer), titles);
    assert!(!reversed_anywhere(buffer));
    // Tab enters the group at the checked radio; the arrow moves the focus
    // to the one before and checks it.
    assert_eq!(view.handle(key("Tab")), Handled::Yes);
    // The group binds its arrows once the focus reached the radio.
    settle(&mut view, &mut terminal, &rx);
    assert_eq!(view.handle(key("Up")), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "(•) Free");
    // The focus follows the radio the arrow checked.
    settle(&mut view, &mut terminal, &rx);
    let buffer = terminal.backend().buffer();
    assert_eq!(rows(buffer)[0..3], ["(•) Free", "    detail", "( ) Plus"]);
    assert!(!reversed_anywhere(buffer));
    for y in 0..2 {
        assert!(
            (0..10).all(|x| buffer[(x, y)].bg != ratatui_core::style::Color::Reset
                && buffer[(x, y)].modifier.contains(Modifier::BOLD)),
            "keyboard focus: the bar on the glyph's and the label's rows"
        );
    }
}

/// Draws what the engine sends until it has been quiet for a moment.
fn settle(
    view: &mut AppView,
    terminal: &mut Terminal<TestBackend>,
    rx: &crossbeam_channel::Receiver<()>,
) {
    while rx
        .recv_timeout(std::time::Duration::from_millis(300))
        .is_ok()
    {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
    }
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
}

#[component]
fn Switches() -> impl IntoElement {
    let switch = |on: bool, track: f32, label: &'static str| {
        nsh_base::switch()
            .checked(on)
            .flex_row()
            .items_center()
            .gap(px(if track > 16.0 { 16.0 } else { 0.0 }))
            .h(px(16.0))
            .track_class(if track > 16.0 {
                "w-[32px] h-4 bg-[#2c2e33] checked:bg-[#45d49a]"
            } else {
                "w-4 h-4 bg-[#2c2e33] checked:bg-[#45d49a]"
            })
            .thumb_class("w-2 h-4 bg-[#fafafa]")
            .child(text(label))
    };
    view()
        .flex_1()
        .child(switch(true, 32.0, "Wi-Fi"))
        .child(switch(false, 32.0, "Bluetooth"))
        .child(switch(true, 16.0, " Tight"))
}

#[test]
fn switches_read_on_and_off_and_shorten_without_room() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 3)).unwrap();
    view.load(App::Rust(Root::new::<Switches>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Tight");
    let buffer = terminal.backend().buffer();
    // A 4-Cell track 2 Cells before its label holds the full glyph and a
    // blank; a 2-Cell track one blank before it, only the short form.
    assert_eq!(rows(buffer), ["[on ] Wi-Fi", "[off] Bluetooth", "on Tight"]);
    assert_eq!(buffer[(1, 0)].fg, Color::Green);
    assert!(!reversed_anywhere(buffer));
}

#[component]
fn Bare() -> impl IntoElement {
    let checked = |state| AccessibilityState {
        checked: Some(state),
        ..Default::default()
    };
    view()
        .flex_1()
        .child(
            view()
                .h(px(16.0))
                .pl(px(40.0))
                .role(AccessibilityRole::Checkbox)
                .accessibility_state(checked(Checked::True))
                .on_press(|_: &()| {})
                .child(text("Roomy")),
        )
        .child(
            view()
                .h(px(16.0))
                .pl(px(16.0))
                .role(AccessibilityRole::Checkbox)
                .accessibility_state(checked(Checked::True))
                .on_press(|_: &()| {})
                .child(text("Short")),
        )
        .child(
            view()
                .h(px(16.0))
                .role(AccessibilityRole::Checkbox)
                .accessibility_state(checked(Checked::True))
                .on_press(|_: &()| {})
                .child(text("Flush")),
        )
}

#[test]
fn without_an_indicator_the_glyph_takes_the_blank_cells_before_the_label() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 3)).unwrap();
    view.load(App::Rust(Root::new::<Bare>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Flush");
    let buffer = terminal.backend().buffer();
    assert_eq!(rows(buffer), [" [x] Roomy", "x Short", "Flush"]);
    assert!(!reversed_anywhere(buffer));
}
