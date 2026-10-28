use nsh_core::prelude::*;
use nsh_platform_tui::{App, Root};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

use super::app::{draw_until, surface};

#[component]
fn Gradients() -> impl IntoElement {
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x112233))
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg_linear_to_r()
                .gradient_from(nsh_core::color::rgb(0x0000ff))
                .gradient_via(nsh_core::color::rgb(0x00ff00))
                .gradient_to(nsh_core::color::rgb(0xff0000))
                .child(text("three")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x884422))
                .bg_radial()
                .gradient_from(nsh_core::color::rgb(0x000000))
                .gradient_via(nsh_core::color::rgba(0xffffff80))
                .gradient_to(nsh_core::color::rgb(0x000000))
                .child(text("over")),
        )
}

#[test]
fn view_gradients_draw_their_middle_stop_over_the_fill() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    view.load(App::Rust(Root::new::<Gradients>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "over");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].bg, Color::Rgb(0x00, 0xff, 0x00));
    assert_eq!(buffer[(9, 0)].bg, Color::Rgb(0x00, 0xff, 0x00));
    assert_eq!(buffer[(0, 1)].bg, Color::Rgb(196, 162, 145));
    assert_eq!(buffer[(12, 0)].bg, Color::Reset);
}
