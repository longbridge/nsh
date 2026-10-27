use nsh_core::prelude::*;
use nsh_platform_tui::{App, Root};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

use super::app::{draw_until, surface};

#[component]
fn ScrollPage() -> impl IntoElement {
    view().flex_1().child(
        scroll()
            .absolute()
            .left(px(8.0))
            .top(px(16.0))
            .w(px(80.0))
            .h(px(32.0))
            .child(view().h(px(16.0)).flex_shrink_0().child(text("first")))
            .child(view().h(px(16.0)).flex_shrink_0().child(text("second")))
            .child(
                view()
                    .h(px(16.0))
                    .flex_shrink_0()
                    .bg(nsh_core::color::rgb(0x884422))
                    .child(text("hidden")),
            ),
    )
}

#[test]
fn scroll_content_is_visible_at_zero_offset_and_clipped_to_its_view() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<ScrollPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "second");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].symbol(), "f");
    assert_eq!(buffer[(1, 2)].symbol(), "s");
    assert_eq!(
        buffer[(10, 1)].symbol(),
        " ",
        "no Scroll placeholder border"
    );
    assert_eq!(buffer[(1, 3)].symbol(), " ");
    assert_eq!(buffer[(1, 3)].bg, Color::Reset);
}

#[component]
fn CommandScrollPage() -> impl IntoElement {
    let target = element_ref();
    let reported = signal(0);
    let selected = signal(0);
    view()
        .flex_1()
        .child(
            view()
                .h(px(16.0))
                .on_press(move |_: &()| {
                    target.scroll_to(nsh_core::Point::new(px(0.0), px(16.0)), false)
                })
                .child(text("scroll")),
        )
        .child(text(move || {
            format!("offset {} selected {}", reported.get(), selected.get())
        }))
        .child(
            scroll()
                .element_ref(&target)
                .absolute()
                .left(px(8.0))
                .top(px(32.0))
                .w(px(80.0))
                .h(px(32.0))
                .on_scroll(move |event: &ScrollEvent| reported.set(event.offset.y.0 as i32))
                .child(
                    view()
                        .h(px(16.0))
                        .flex_shrink_0()
                        .on_press(move |_: &()| selected.set(1))
                        .child(text("first")),
                )
                .child(
                    view()
                        .h(px(16.0))
                        .flex_shrink_0()
                        .on_press(move |_: &()| selected.set(2))
                        .child(text("second")),
                )
                .child(
                    view()
                        .h(px(16.0))
                        .flex_shrink_0()
                        .on_press(move |_: &()| selected.set(3))
                        .child(text("third")),
                ),
        )
}

#[test]
fn core_scroll_commands_move_content_and_its_clipped_hit_areas() {
    use super::app::mouse;
    use nsh_platform_tui::{Handled, MouseButton, MouseKind};
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    view.load(App::Rust(Root::new::<CommandScrollPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "offset 0 selected 0");
    assert_eq!(terminal.backend().buffer()[(1, 2)].symbol(), "f");
    for kind in [
        MouseKind::Down(MouseButton::Left),
        MouseKind::Up(MouseButton::Left),
    ] {
        assert_eq!(view.handle(mouse(kind, 0, 0)), Handled::Yes);
    }
    draw_until(&mut view, &mut terminal, &rx, "offset 16");
    assert_eq!(terminal.backend().buffer()[(1, 2)].symbol(), "s");
    assert_eq!(terminal.backend().buffer()[(1, 3)].symbol(), "t");
    assert_eq!(terminal.backend().buffer()[(1, 4)].symbol(), " ");
    for kind in [
        MouseKind::Down(MouseButton::Left),
        MouseKind::Up(MouseButton::Left),
    ] {
        assert_eq!(view.handle(mouse(kind, 1, 2)), Handled::Yes);
    }
    draw_until(&mut view, &mut terminal, &rx, "selected 2");
    assert_eq!(
        view.handle(mouse(MouseKind::Down(MouseButton::Left), 1, 4)),
        Handled::Ignored
    );
}

#[component]
fn InitialOffsets() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            scroll()
                .absolute()
                .left(px(8.0))
                .top(px(16.0))
                .w(px(24.0))
                .h(px(16.0))
                .horizontal()
                .content_offset(nsh_core::Point::new(px(8.0), px(0.0)))
                .child(text("ABCDE").w(px(40.0)).flex_shrink_0()),
        )
        .child(
            scroll()
                .absolute()
                .left(px(8.0))
                .top(px(48.0))
                .w(px(24.0))
                .h(px(32.0))
                .content_offset(nsh_core::Point::new(px(0.0), px(16.0)))
                .child(text("one\ntwo\nend").h(px(48.0)).flex_shrink_0()),
        )
}

#[test]
fn initial_offsets_clip_text_on_both_axes() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<InitialOffsets>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "BCD");
    draw_until(&mut view, &mut terminal, &rx, "two");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].symbol(), "B");
    assert_eq!(buffer[(3, 1)].symbol(), "D");
    assert_eq!(buffer[(4, 1)].symbol(), " ");
    assert_eq!(buffer[(1, 3)].symbol(), "t");
    assert_eq!(buffer[(1, 4)].symbol(), "e");
    assert_eq!(buffer[(1, 5)].symbol(), " ");
}
