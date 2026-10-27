use nsh_core::prelude::*;
use nsh_platform_tui::{App, Handled, MouseButton, MouseKind, Root};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

use super::app::{draw_until, mouse, surface};

#[component]
fn Pages() -> impl IntoElement {
    let pushed = signal(false);
    let hidden_presses = signal(0);
    view()
        .flex_1()
        .child(text(move || format!("hidden {}", hidden_presses.get())).h(px(16.0)))
        .child(
            nsh_base::nav_stack()
                .w(px(160.0))
                .h(px(96.0))
                .child(
                    nsh_base::nav_page()
                        .key("home")
                        .child(
                            view()
                                .h(px(16.0))
                                .on_press(move |_: &()| pushed.set(true))
                                .child(text("push HOME-tail")),
                        )
                        .child(
                            view()
                                .h(px(16.0))
                                .on_press(move |_: &()| hidden_presses.update(|n| *n += 1))
                                .child(text("hidden target")),
                        ),
                )
                .child(when(pushed, move || {
                    nsh_base::nav_page().key("top").child(
                        view()
                            .h(px(16.0))
                            .on_press(move |_: &()| pushed.set(false))
                            .child(text("top")),
                    )
                })),
        )
}

#[test]
fn a_push_conceals_the_previous_page_until_the_app_removes_the_top() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    view.load(App::Rust(Root::new::<Pages>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "push HOME-tail");
    for kind in [
        MouseKind::Down(MouseButton::Left),
        MouseKind::Up(MouseButton::Left),
    ] {
        assert_eq!(view.handle(mouse(kind, 0, 1)), Handled::Yes);
    }
    draw_until(&mut view, &mut terminal, &rx, "top");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 1)].symbol(), "t");
    for column in 3..20 {
        assert_eq!(buffer[(column, 1)].symbol(), " ");
    }
    assert_eq!(buffer[(0, 2)].symbol(), " ");
    assert_eq!(
        buffer[(19, 6)].symbol(),
        " ",
        "navigation containers draw no placeholder"
    );
    for kind in [
        MouseKind::Down(MouseButton::Left),
        MouseKind::Up(MouseButton::Left),
    ] {
        assert_eq!(view.handle(mouse(kind, 0, 2)), Handled::Ignored);
    }
    for kind in [
        MouseKind::Down(MouseButton::Left),
        MouseKind::Up(MouseButton::Left),
    ] {
        assert_eq!(view.handle(mouse(kind, 0, 1)), Handled::Yes);
    }
    draw_until(&mut view, &mut terminal, &rx, "push HOME-tail");
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
            .contains("hidden 0")
    );
    for kind in [
        MouseKind::Down(MouseButton::Left),
        MouseKind::Up(MouseButton::Left),
    ] {
        assert_eq!(view.handle(mouse(kind, 0, 2)), Handled::Yes);
    }
    draw_until(&mut view, &mut terminal, &rx, "hidden 1");
}

#[component]
fn HeaderSlots() -> impl IntoElement {
    let presses = signal(0);
    nsh_base::nav_stack().w(px(160.0)).h(px(96.0)).child(
        nsh_base::nav_page()
            .header(
                view()
                    .h(px(16.0))
                    .on_press(move |_: &()| presses.update(|n| *n += 1))
                    .child(text("HEADER")),
            )
            .header_left(
                view()
                    .h(px(16.0))
                    .on_press(move |_: &()| presses.set(99))
                    .child(text("LEFT")),
            )
            .header_right(text("RIGHT"))
            .header_title(text("TITLE"))
            .child(text(move || format!("presses {}", presses.get())).h(px(16.0)))
            .child(when(
                move || presses.get() > 0,
                || {
                    view()
                        .absolute()
                        .top(px(0.0))
                        .w(px(160.0))
                        .h(px(16.0))
                        .child(text("under header"))
                },
            )),
    )
}

#[test]
fn only_the_selected_header_slot_is_drawn_and_it_stays_above_content() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    view.load(App::Rust(Root::new::<HeaderSlots>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "presses 0");
    let buffer = terminal.backend().buffer();
    let header: String = (0..6).map(|column| buffer[(column, 0)].symbol()).collect();
    assert_eq!(header, "HEADER");
    let contents: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    for other in ["LEFT", "RIGHT", "TITLE", "HeaderItem", "Screen", "Stack"] {
        assert!(!contents.contains(other), "unexpected {other}: {contents}");
    }
    for kind in [
        MouseKind::Down(MouseButton::Left),
        MouseKind::Up(MouseButton::Left),
    ] {
        assert_eq!(view.handle(mouse(kind, 0, 0)), Handled::Yes);
    }
    draw_until(&mut view, &mut terminal, &rx, "presses 1");
    let buffer = terminal.backend().buffer();
    assert_eq!(
        (0..6)
            .map(|column| buffer[(column, 0)].symbol())
            .collect::<String>(),
        "HEADER"
    );
}

#[component]
fn SizedPage() -> impl IntoElement {
    let dimensions = signal(String::new());
    view()
        .flex_1()
        .child(text(move || dimensions.get()).h(px(16.0)))
        .child(
            nsh_base::nav_stack().flex_1().child(
                nsh_base::nav_page()
                    .on_layout(move |frame: &nsh_core::Bounds<nsh_core::Points>| {
                        dimensions.set(format!("{}x{}", frame.size.width.0, frame.size.height.0))
                    })
                    .child(
                        view()
                            .absolute()
                            .right(px(0.0))
                            .bottom(px(0.0))
                            .w(px(8.0))
                            .h(px(16.0))
                            .child(text("X")),
                    ),
            ),
        )
}

#[test]
fn the_shown_screen_tracks_its_stack_size_after_resize() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    view.load(App::Rust(Root::new::<SizedPage>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "240x144");
    assert_eq!(terminal.backend().buffer()[(29, 9)].symbol(), "X");
    terminal.backend_mut().resize(20, 8);
    terminal
        .resize(ratatui_core::layout::Rect::new(0, 0, 20, 8))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "160x112");
    assert_eq!(terminal.backend().buffer()[(19, 7)].symbol(), "X");
}
