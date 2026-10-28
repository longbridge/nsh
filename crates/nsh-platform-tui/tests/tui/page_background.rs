use nsh_core::prelude::*;
use nsh_platform_tui::{App, Root};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

use super::app::{draw_until, mouse, surface};

#[component]
fn Page() -> impl IntoElement {
    view().flex_1().bg(nsh_core::color::rgb(0x112233)).child(
        view()
            .w(px(80.0))
            .h(px(48.0))
            .bg(nsh_core::color::rgb(0x884422))
            .child(
                view()
                    .h(px(16.0))
                    .bg(nsh_core::color::rgb(0x112233))
                    .child(text("same")),
            )
            .child(
                view()
                    .h(px(16.0))
                    .bg(nsh_core::color::rgb(0x112234))
                    .child(text("near")),
            )
            .child(text("card")),
    )
}

#[test]
fn page_and_exactly_matching_fills_reset_the_terminal_background() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<Page>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "card");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(12, 0)].bg, Color::Reset);
    assert_eq!(
        buffer[(0, 0)].bg,
        Color::Reset,
        "matching fill resets even over a colored card"
    );
    assert_eq!(buffer[(0, 1)].bg, Color::Rgb(0x11, 0x22, 0x34));
    assert_eq!(buffer[(0, 2)].bg, Color::Rgb(0x88, 0x44, 0x22));
}

#[component]
fn ChangingPage() -> impl IntoElement {
    let changed = signal(false);
    view()
        .flex_1()
        .bg(move || nsh_core::color::rgb(if changed.get() { 0x445566 } else { 0x112233 }))
        .on_press(move |_: &()| changed.set(true))
        .child(
            view()
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x112233))
                .child(text("old")),
        )
        .child(
            view()
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x445566))
                .child(text("new")),
        )
        .child(text(
            move || if changed.get() { "changed" } else { "change" },
        ))
}

#[test]
fn changing_the_page_fill_updates_which_colors_use_the_terminal_background() {
    use nsh_platform_tui::{Handled, MouseButton, MouseKind};
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<ChangingPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "change");
    assert_eq!(terminal.backend().buffer()[(0, 0)].bg, Color::Reset);
    assert_eq!(
        terminal.backend().buffer()[(0, 1)].bg,
        Color::Rgb(0x44, 0x55, 0x66)
    );
    assert_eq!(
        view.handle(mouse(MouseKind::Down(MouseButton::Left), 0, 2)),
        Handled::Yes
    );
    assert_eq!(
        view.handle(mouse(MouseKind::Up(MouseButton::Left), 0, 2)),
        Handled::Yes
    );
    draw_until(&mut view, &mut terminal, &rx, "changed");
    assert_eq!(
        terminal.backend().buffer()[(0, 0)].bg,
        Color::Rgb(0x11, 0x22, 0x33)
    );
    assert_eq!(terminal.backend().buffer()[(0, 1)].bg, Color::Reset);
}

#[component]
fn ScreenPage() -> impl IntoElement {
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x112233))
        .child(
            nsh_base::nav_stack().w(px(80.0)).h(px(48.0)).child(
                nsh_base::nav_page()
                    .bg(nsh_core::color::rgb(0x445566))
                    .child(text("screen")),
            ),
        )
        .child(
            view()
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x445566))
                .child(text("match")),
        )
}

#[test]
fn screen_fills_and_their_matches_use_the_terminal_background() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<ScreenPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "screen");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].bg, Color::Reset);
    assert_eq!(buffer[(0, 3)].bg, Color::Reset);
}

#[component]
fn ModalPage() -> impl IntoElement {
    view().flex_1().bg(nsh_core::color::rgb(0x112233)).child(
        nsh_base::dialog().bg(nsh_core::color::rgb(0x778899)).child(
            view()
                .w(px(80.0))
                .h(px(48.0))
                .bg(nsh_core::color::rgb(0x884422))
                .child(
                    view()
                        .h(px(16.0))
                        .bg(nsh_core::color::rgb(0x778899))
                        .child(text("modal")),
                ),
        ),
    )
}

#[test]
fn modal_fills_and_their_matches_use_the_terminal_background() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<ModalPage>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "modal");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(12, 1)].bg, Color::Reset);
    assert_eq!(buffer[(0, 0)].bg, Color::Reset);
    assert_eq!(buffer[(1, 1)].bg, Color::Rgb(0x88, 0x44, 0x22));
}

#[component]
fn OverlayPage() -> impl IntoElement {
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x112233))
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x24262a))
                .child(text("chip")),
        )
        .child(
            overlay()
                .top(px(32.0))
                .left(px(0.0))
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x24262a))
                .child(text("popup")),
        )
}

#[test]
fn overlay_fills_keep_their_color_and_do_not_reset_the_page() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<OverlayPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "popup");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(12, 0)].bg, Color::Reset);
    assert_eq!(buffer[(0, 0)].bg, Color::Rgb(0x24, 0x26, 0x2a), "page chip");
    assert_eq!(buffer[(0, 2)].bg, Color::Rgb(0x24, 0x26, 0x2a), "overlay");
}
