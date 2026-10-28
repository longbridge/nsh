use nsh_core::prelude::*;
use nsh_platform_tui::{App, Handled, Input, Root};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

use super::app::{draw_until, surface};

#[component]
fn CompactOutlinedRow() -> impl IntoElement {
    let focused = signal(false);
    view()
        .flex_1()
        .child(text(move || format!("focused {}", focused.get())).h(px(16.0)))
        .child(
            view()
                .w(px(160.0))
                .h(px(36.0))
                .justify_center()
                .px(px(16.0))
                .on_press(|_: &()| {})
                .on_focus(move |_: &()| focused.set(true))
                .focus(|style| {
                    style
                        .outline(px(2.0))
                        .outline_offset(px(-2.0))
                        .outline_color(nsh_core::color::rgb(0x123456))
                })
                .child(text("Stocks").h(px(16.0))),
        )
}

#[test]
fn a_compact_focus_outline_preserves_the_entire_text_content_row() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    view.load(App::Rust(Root::new::<CompactOutlinedRow>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Stocks");
    let before = terminal.backend().buffer().clone();
    view.handle(key("Tab"));
    draw_until(&mut view, &mut terminal, &rx, "focused true");
    let buffer = terminal.backend().buffer();
    for column in 2..18 {
        assert_eq!(
            buffer[(column, 2)].symbol(),
            before[(column, 2)].symbol(),
            "outline entered text content at column {column}"
        );
    }
    assert_eq!(buffer[(0, 2)].symbol(), "└");
}

#[cfg(feature = "js")]
#[test]
fn outlines_preserve_wide_text_and_its_blank_cells_in_either_paint_order() {
    for (content_first, background) in [
        (false, "transparent"),
        (true, "transparent"),
        (false, "#abcdef"),
        (true, "#abcdef"),
    ] {
        let mut snapshots = Vec::new();
        for outlined in [false, true] {
            let content =
                r#"<Text style={{ position: "absolute", width: 160, height: 16 }}>中 A</Text>"#;
            let outline = format!(
                r##"<View style={{{{ position: "absolute", width: 160, height: 48, backgroundColor: "{background}",
                outlineStyle: "solid", outlineWidth: {}, outlineOffset: -2, outlineColor: "#123456" }}}} />"##,
                if outlined { 2 } else { 0 }
            );
            let children = if content_first {
                format!("{content}{outline}")
            } else {
                format!("{outline}{content}")
            };
            let source = format!(
                r#"import {{ mount, Text, View }} from "nsh";
                mount(() => <View style={{{{ flex: 1 }}}}>{children}
                <Text style={{{{ position: "absolute", top: 80 }}}}>Ready</Text></View>);"#
            );
            let code = nsh_bundler::compile_module("Outline.tsx", &source, &[]).unwrap();
            let (mut view, rx) = surface();
            let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
            view.load(nsh_platform_tui::Bundle::source(code)).unwrap();
            draw_until(&mut view, &mut terminal, &rx, "Ready");
            snapshots.push(terminal.backend().buffer().clone());
        }
        if content_first && background != "transparent" {
            assert_eq!(
                snapshots[1][(0, 0)].symbol(),
                "┌",
                "a later background replaces the old content mask"
            );
        } else {
            for column in 0..20 {
                assert_eq!(
                    snapshots[0][(column, 0)],
                    snapshots[1][(column, 0)],
                    "content first: {content_first}, column: {column}"
                );
            }
        }
        assert_eq!(snapshots[1][(19, 2)].symbol(), "┘");
    }
}

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
fn controls_draw_the_apps_focus_outline_without_changing_layout() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    view.load(App::Rust(Root::new::<OutlinedButton>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "button");
    assert_eq!(view.handle(key("Tab")), Handled::Yes);
    assert_eq!(view.handle(key("Enter")), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "pressed");
    // The app's inset outline surrounds its original 10 by 3 Cell box.
    assert_eq!(
        terminal.backend().buffer()[(2, 4)].fg,
        Color::Rgb(0x12, 0x34, 0x56)
    );
    assert!(!terminal.backend().cursor_visible());
}

#[component]
fn ScaledOutline() -> impl IntoElement {
    view().flex_1().child(
        nsh_base::button()
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
            .child(text("scaled")),
    )
}

#[test]
fn an_app_outline_follows_the_controls_transformed_bounds() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    view.load(App::Rust(Root::new::<ScaledOutline>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "scaled");
    let corner = &terminal.backend().buffer()[(21, 7)];
    assert_eq!(corner.symbol(), "┘");
    assert_eq!(corner.fg, Color::Rgb(0x12, 0x34, 0x56));
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
fn a_subcell_progress_rail_is_visible_and_zero_progress_stays_empty() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    view.load(App::Rust(Root::new::<ThinProgress>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "progress 0");
    assert_eq!(
        terminal.backend().buffer()[(0, 0)].bg,
        Color::Rgb(0x12, 0x34, 0x56)
    );
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "progress 50");
    assert_eq!(
        terminal.backend().buffer()[(9, 0)].bg,
        Color::Rgb(0xab, 0xcd, 0xef)
    );
    assert_eq!(
        terminal.backend().buffer()[(10, 0)].bg,
        Color::Rgb(0x12, 0x34, 0x56)
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
        assert_eq!(
            terminal.backend().buffer()[(0, 0)].bg,
            Color::Rgb(0x12, 0x34, 0x56)
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

#[test]
fn checkbox_and_switch_project_value_and_disabled_states_to_their_parts() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(64, 8)).unwrap();
    view.load(App::Rust(Root::new::<ValueControls>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "mixed true");
    assert_eq!(
        terminal.backend().buffer()[(0, 0)].bg,
        Color::Rgb(0xab, 0xcd, 0xef)
    );
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "check true mixed false");
    assert_eq!(
        terminal.backend().buffer()[(0, 0)].bg,
        Color::Rgb(0x12, 0x34, 0x56)
    );
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "switch true");
    assert_eq!(
        terminal.backend().buffer()[(0, 1)].bg,
        Color::Rgb(0x65, 0x43, 0x21)
    );
    assert_eq!(
        terminal.backend().buffer()[(1, 1)].bg,
        Color::Rgb(0x12, 0x34, 0x56)
    );
    view.handle(key("Tab"));
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "disabled true");
    assert_eq!(
        terminal.backend().buffer()[(0, 1)].bg,
        Color::Rgb(0xfe, 0xdc, 0xba)
    );
    assert_eq!(
        terminal.backend().buffer()[(1, 1)].bg,
        Color::Rgb(0xab, 0xcd, 0xef)
    );
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
        if terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| cell.bg == Color::Rgb(0x65, 0x43, 0x21))
        {
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
