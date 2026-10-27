use nsh_core::prelude::*;
use nsh_platform_tui::{App, AppView, Handled, Input, Modifiers, Root};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

use super::app::{draw_until, surface};

fn key(view: &mut AppView, name: &str, ctrl: bool) -> Handled {
    view.handle(Input::Key {
        key: name.into(),
        modifiers: Modifiers {
            ctrl,
            ..Default::default()
        },
        release: false,
    })
}

#[component]
fn ReturningPage() -> impl IntoElement {
    let detail = signal(false);
    let focused = signal("none");
    let opens = signal(0);
    let visible = signal(true);
    on_key("ctrl+d", move |_| visible.set(false));
    view()
        .flex_1()
        .child(
            text(move || {
                format!(
                    "focus {} opens {} visible {}",
                    focused.get(),
                    opens.get(),
                    visible.get()
                )
            })
            .h(px(16.0)),
        )
        .child(
            nsh_base::nav_stack()
                .w(px(240.0))
                .h(px(96.0))
                .child(
                    nsh_base::nav_page()
                        .key("home")
                        .child(
                            view()
                                .h(px(16.0))
                                .on_focus(move |_: &()| focused.set("A"))
                                .on_press(|_: &()| {})
                                .child(text("home A")),
                        )
                        .child(
                            view()
                                .h(px(16.0))
                                .opacity(move || if visible.get() { 1.0_f32 } else { 0.0_f32 })
                                .on_focus(move |_: &()| focused.set("B"))
                                .on_press(move |_: &()| {
                                    opens.update(|n| *n += 1);
                                    detail.set(true);
                                })
                                .child(text("open detail")),
                        ),
                )
                .child(when(detail, move || {
                    nsh_base::nav_page()
                        .key("detail")
                        .on_dismiss(move |_: &()| detail.set(false))
                        .child(
                            view()
                                .h(px(16.0))
                                .on_focus(move |_: &()| focused.set("detail"))
                                .on_press(move |_: &()| detail.set(false))
                                .child(text("detail button")),
                        )
                })),
        )
}

fn detail_page() -> (
    AppView,
    Terminal<TestBackend>,
    crossbeam_channel::Receiver<()>,
) {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    view.load(App::Rust(Root::new::<ReturningPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "home A");
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus A");
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus B");
    key(&mut view, "Enter", false);
    draw_until(&mut view, &mut terminal, &rx, "detail button");
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus detail");
    (view, terminal, rx)
}

#[test]
fn returning_to_a_screen_restores_the_control_that_opened_the_next_page() {
    for close in ["Escape", "Enter"] {
        let (mut view, mut terminal, rx) = detail_page();
        key(&mut view, close, false);
        draw_until(&mut view, &mut terminal, &rx, "focus B");
        key(&mut view, "Enter", false);
        draw_until(&mut view, &mut terminal, &rx, "opens 2");
        draw_until(&mut view, &mut terminal, &rx, "detail button");
    }
}

#[test]
fn returning_to_a_screen_does_not_restore_an_invisible_control() {
    let (mut view, mut terminal, rx) = detail_page();
    key(&mut view, "d", true);
    draw_until(&mut view, &mut terminal, &rx, "visible false");
    key(&mut view, "Escape", false);
    draw_until(&mut view, &mut terminal, &rx, "home A");
    assert_eq!(key(&mut view, "Enter", false), Handled::Ignored);
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus A");
}

#[component]
fn PagingFocus() -> impl IntoElement {
    let focused = signal(-1);
    let offset = signal(0);
    let pressed = signal(-1);
    view()
        .flex_1()
        .child(
            text(move || {
                format!(
                    "focus {} offset {} pressed {}",
                    focused.get(),
                    offset.get(),
                    pressed.get()
                )
            })
            .h(px(16.0)),
        )
        .child(
            scroll()
                .w(px(160.0))
                .h(px(48.0))
                .on_scroll(move |event: &ScrollEvent| offset.set(event.offset.y.0 as i32))
                .children((0..12).map(|index| {
                    view()
                        .h(px(16.0))
                        .flex_shrink_0()
                        .on_focus(move |_: &()| focused.set(index))
                        .on_press(move |_: &()| pressed.set(index))
                        .child(text(format!("row {index}")))
                })),
        )
}

#[test]
fn paging_keeps_the_relative_focus_row_and_endpoints_choose_first_and_last() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    view.load(App::Rust(Root::new::<PagingFocus>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "focus -1 offset 0");
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus 0 offset 0");
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus 1 offset 0");
    for (name, ctrl, expected) in [
        ("PageDown", false, "focus 4 offset 48"),
        ("Home", false, "focus 0 offset 0"),
        ("End", false, "focus 11 offset 144"),
        ("PageUp", false, "focus 8 offset 96"),
        ("v", true, "focus 11 offset 144"),
    ] {
        key(&mut view, name, ctrl);
        draw_until(&mut view, &mut terminal, &rx, expected);
    }
    key(&mut view, "Enter", false);
    draw_until(&mut view, &mut terminal, &rx, "pressed 11");
}

fn wheel(view: &mut AppView, column: u16, row: u16, x: i16, y: i16, shift: bool) -> Handled {
    let mut input = nsh_platform_tui::MouseInput::default();
    input.column = column;
    input.row = row;
    input.kind = nsh_platform_tui::MouseKind::Wheel { x, y };
    input.modifiers = Modifiers {
        shift,
        ..Default::default()
    };
    view.handle(Input::Mouse(input))
}

#[test]
fn the_wheel_keeps_visible_focus_and_replaces_focus_that_scrolls_out_of_sight() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    view.load(App::Rust(Root::new::<PagingFocus>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "focus -1 offset 0");
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus 0 offset 0");
    assert_eq!(wheel(&mut view, 0, 2, 0, 1, false), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "focus 1 offset 16");
    key(&mut view, "Enter", false);
    draw_until(&mut view, &mut terminal, &rx, "pressed 1");
    key(&mut view, "Down", false);
    draw_until(&mut view, &mut terminal, &rx, "focus 2 offset 16");
    wheel(&mut view, 0, 2, 0, 1, false);
    draw_until(&mut view, &mut terminal, &rx, "focus 2 offset 32");
    wheel(&mut view, 0, 2, 0, -1, false);
    draw_until(&mut view, &mut terminal, &rx, "focus 2 offset 16");
}

#[component]
fn NestedWheels() -> impl IntoElement {
    let inner = signal(0);
    let outer = signal(0);
    view()
        .flex_1()
        .child(text(move || format!("inner {} outer {}", inner.get(), outer.get())).h(px(16.0)))
        .child(
            scroll()
                .w(px(160.0))
                .h(px(48.0))
                .on_scroll(move |event: &ScrollEvent| outer.set(event.offset.y.0 as i32))
                .child(
                    scroll()
                        .h(px(16.0))
                        .flex_shrink_0()
                        .on_scroll(move |event: &ScrollEvent| inner.set(event.offset.y.0 as i32))
                        .child(text("one\ntwo\nend").h(px(48.0)).flex_shrink_0()),
                )
                .child(view().h(px(64.0)).flex_shrink_0().child(text("tail"))),
        )
}

#[test]
fn the_wheel_uses_the_innermost_scroll_then_its_parent_at_the_end() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    view.load(App::Rust(Root::new::<NestedWheels>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "inner 0 outer 0");
    for expected in ["inner 16 outer 0", "inner 32 outer 0", "inner 32 outer 16"] {
        assert_eq!(wheel(&mut view, 0, 1, 0, 1, false), Handled::Yes);
        draw_until(&mut view, &mut terminal, &rx, expected);
    }
}

#[component]
fn HorizontalWheel() -> impl IntoElement {
    let offset = signal(0);
    view()
        .flex_1()
        .child(
            scroll()
                .horizontal()
                .w(px(24.0))
                .h(px(16.0))
                .on_scroll(move |event: &ScrollEvent| offset.set(event.offset.x.0 as i32))
                .child(text("ABCDE").w(px(40.0)).flex_shrink_0()),
        )
        .child(text(move || format!("horizontal {}", offset.get())))
}

#[test]
fn shift_wheel_and_horizontal_events_scroll_by_columns() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    view.load(App::Rust(Root::new::<HorizontalWheel>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "horizontal 0");
    assert_eq!(wheel(&mut view, 0, 0, 0, 1, true), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "horizontal 8");
    assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), "B");
    wheel(&mut view, 0, 0, 1, 0, false);
    draw_until(&mut view, &mut terminal, &rx, "horizontal 16");
    assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), "C");
    assert_eq!(
        key(&mut view, "Enter", false),
        Handled::Ignored,
        "wheel creates no focus when none existed"
    );
}
