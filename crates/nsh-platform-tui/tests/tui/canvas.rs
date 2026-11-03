use nsh_core::{
    AccessibilityRole, Bounds, Point, Points, Size,
    canvas::{CanvasNode, CanvasTextAlign, CanvasTextBaseline, FillRule},
    prelude::*,
};
use nsh_platform_tui::{App, Root};
use ratatui_core::{backend::TestBackend, buffer::Buffer, style::Color, terminal::Terminal};

use super::app::{draw_until, mouse, surface};

const RED: u32 = 0xff0000;
const GREEN: u32 = 0x00ff00;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Points> {
    Bounds::new(Point::new(px(x), px(y)), Size::new(px(width), px(height)))
}

fn row(buffer: &Buffer, y: u16, columns: std::ops::Range<u16>) -> String {
    columns.map(|x| buffer[(x, y)].symbol()).collect()
}

#[component]
fn Shapes() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            canvas(
                Painter::new()
                    .fill_color(nsh_core::color::rgb(RED))
                    .fill_rect(0.0, 0.0, 40.0, 48.0)
                    .stroke_color(nsh_core::color::rgb(GREEN))
                    .begin_path()
                    .move_to(50.0, 2.0)
                    .line_to(160.0, 2.0)
                    .stroke()
                    .line_dash(&[px(8.0), px(8.0)])
                    .begin_path()
                    .move_to(50.0, 22.0)
                    .line_to(160.0, 22.0)
                    .stroke()
                    .fill_color(nsh_core::color::rgb(GREEN))
                    .text_align(CanvasTextAlign::Center)
                    .text_baseline(CanvasTextBaseline::Top)
                    .fill_text("Hi", 80.0, 32.0),
            )
            .label("drawing")
            .w(px(160.0))
            .h(px(48.0)),
        )
        .child(
            canvas(
                Painter::new()
                    .global_alpha(0.5)
                    .fill_color(nsh_core::color::rgb(0xffffff))
                    .fill_rect(0.0, 0.0, 16.0, 16.0),
            )
            .label("drawing")
            .w(px(64.0))
            .h(px(32.0))
            .bg(nsh_core::color::rgb(0x000000)),
        )
        .child(text("drawn"))
}

#[component]
fn Icon() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            canvas(
                Painter::new()
                    .fill_color(nsh_core::color::rgb(0xffffff))
                    .fill_rect(0.0, 0.0, 16.0, 16.0),
            )
            .w(px(16.0))
            .h(px(16.0)),
        )
        .child(text("ready"))
}

#[test]
fn icon_sized_canvases_draw_nothing() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 3)).unwrap();
    view.load(App::Rust(Root::new::<Icon>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let drawn: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert_eq!(drawn.replace(' ', ""), "ready", "{drawn:?}");
}

#[test]
fn canvas_fills_strokes_dashes_and_text_draw_in_braille_cells() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
    view.load(App::Rust(Root::new::<Shapes>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "drawn");
    let buffer = terminal.backend().buffer();
    for y in 0..3 {
        assert_eq!(
            row(buffer, y, 0..5),
            "⣿⣿⣿⣿⣿",
            "the filled rectangle, row {y}"
        );
        assert_eq!(buffer[(0, y)].fg, Color::Rgb(255, 0, 0));
    }
    assert_eq!(
        row(buffer, 0, 6..20),
        "⠉".repeat(14),
        "a line through the top dots"
    );
    assert_eq!(buffer[(6, 0)].fg, Color::Rgb(0, 255, 0));
    assert_eq!(
        row(buffer, 1, 6..12),
        "⠒ ⠒ ⠒ ",
        "a dash pattern of one Cell on, one off"
    );
    assert_eq!(
        row(buffer, 2, 9..11),
        "Hi",
        "text centered on its x, its top at y"
    );
    assert_eq!(buffer[(9, 2)].fg, Color::Rgb(0, 255, 0));
    assert_eq!(buffer[(0, 3)].symbol(), "⣿");
    assert_eq!(
        buffer[(0, 3)].fg,
        Color::Rgb(128, 128, 128),
        "globalAlpha blends the paint over the color under it"
    );
    assert_eq!(buffer[(0, 3)].bg, Color::Reset, "fills draw nothing");
    let contents: String = buffer.content.iter().map(|c| c.symbol()).collect();
    assert!(!contents.contains("Canvas"), "no placeholder is drawn");
}

#[component]
fn Clipped() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            canvas(
                Painter::new()
                    .begin_path()
                    .rect(rect(0.0, 0.0, 40.0, 32.0))
                    .clip()
                    .fill_color(nsh_core::color::rgb(RED))
                    .fill_rect(0.0, 0.0, 80.0, 32.0)
                    .stroke_color(nsh_core::color::rgb(GREEN))
                    .begin_path()
                    .move_to(0.0, 30.0)
                    .line_to(80.0, 30.0)
                    .stroke(),
            )
            .label("drawing")
            .w(px(80.0))
            .h(px(32.0)),
        )
        .child(
            canvas(
                Painter::new()
                    .fill_color(nsh_core::color::rgb(RED))
                    .begin_path()
                    .rect(rect(0.0, 0.0, 80.0, 16.0))
                    .rect(rect(16.0, 0.0, 48.0, 16.0))
                    .fill_with(FillRule::EvenOdd),
            )
            .label("drawing")
            .w(px(80.0))
            .h(px(16.0)),
        )
        .child(text("clipped"))
}

#[test]
fn a_clip_keeps_fills_and_strokes_inside_and_fill_rules_pick_the_dots() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<Clipped>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "clipped");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 0, 0..10), "⣿⣿⣿⣿⣿     ");
    assert_eq!(
        row(buffer, 1, 5..10),
        "     ",
        "the stroke outside the clip is left out"
    );
    assert_eq!(
        buffer[(4, 1)].fg,
        Color::Rgb(0, 255, 0),
        "the stroke inside it is drawn"
    );
    assert_eq!(
        row(buffer, 2, 0..10),
        "⣿⣿      ⣿⣿",
        "even-odd leaves the inner rectangle out"
    );
}

const RIGHT_HALF: (f32, f32, f32, f32) = (40.0, 0.0, 40.0, 16.0);

#[component]
fn Pads() -> impl IntoElement {
    let presses = signal(0);
    let (x, y, width, height) = RIGHT_HALF;
    view()
        .flex_1()
        .child(
            canvas(
                Painter::new()
                    .fill_color(nsh_core::color::rgb(GREEN))
                    .fill_rect(x, y, width, height)
                    .node(
                        CanvasNode::new(
                            rect(x, y, width, height),
                            AccessibilityRole::Button,
                            "Right",
                        )
                        .test_id("right"),
                    ),
            )
            .label("drawing")
            .w(px(80.0))
            .h(px(16.0))
            .on_press(move || presses.update(|n| *n += 1)),
        )
        .child(text(move || format!("Pressed {}", presses.get())))
}

#[test]
fn a_press_at_a_canvas_nodes_center_reaches_the_canvas() {
    use nsh_platform_tui::{MouseButton, MouseKind};
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 3)).unwrap();
    view.load(App::Rust(Root::new::<Pads>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Pressed 0");
    // A test step presses a canvas node at its center, as a pointer there.
    let (x, y, width, height) = RIGHT_HALF;
    let column = ((x + width / 2.0) / 8.0).floor() as u16;
    let row = ((y + height / 2.0) / 16.0).floor() as u16;
    assert_eq!(terminal.backend().buffer()[(column, row)].symbol(), "⣿");
    view.handle(mouse(MouseKind::Down(MouseButton::Left), column, row));
    view.handle(mouse(MouseKind::Up(MouseButton::Left), column, row));
    draw_until(&mut view, &mut terminal, &rx, "Pressed 1");
}

/// A line through the row of a label on it.
#[component]
fn Labelled() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            canvas(
                Painter::new()
                    .stroke_color(nsh_core::color::rgb(GREEN))
                    .begin_path()
                    .move_to(0.0, 6.0)
                    .line_to(160.0, 6.0)
                    .stroke()
                    .fill_color(nsh_core::color::rgb(RED))
                    .text_align(CanvasTextAlign::Center)
                    .text_baseline(CanvasTextBaseline::Top)
                    .fill_text("20", 80.0, 0.0),
            )
            .label("chart")
            .w(px(160.0))
            .h(px(32.0)),
        )
        .child(text("drawn"))
}

#[test]
fn a_canvas_label_keeps_a_cell_clear_of_dots_each_side() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 4)).unwrap();
    view.load(App::Rust(Root::new::<Labelled>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "drawn");
    let buffer = terminal.backend().buffer();
    let line = row(buffer, 0, 0..20);
    let at = line.find("20").expect("the label is drawn");
    let at = line[..at].chars().count() as u16;
    assert_eq!(buffer[(at - 1, 0)].symbol(), " ", "{line}");
    assert_eq!(buffer[(at + 2, 0)].symbol(), " ", "{line}");
    assert_ne!(
        buffer[(at - 2, 0)].symbol(),
        " ",
        "the line goes on: {line}"
    );
    assert_ne!(
        buffer[(at + 3, 0)].symbol(),
        " ",
        "the line goes on: {line}"
    );
}

/// A ring's track at 13%: a fill too faint to read as a wash.
#[component]
fn FaintTrack() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            canvas(
                Painter::new()
                    .global_alpha(0.13)
                    .fill_color(nsh_core::color::rgb(0xffffff))
                    .fill_rect(0.0, 0.0, 64.0, 32.0),
            )
            .label("drawing")
            .w(px(64.0))
            .h(px(32.0)),
        )
        .child(text("drawn"))
}

#[test]
fn a_fill_fainter_than_a_wash_is_left_out() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<FaintTrack>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "drawn");
    let buffer = terminal.backend().buffer();
    let first = row(buffer, 0, 0..8);
    assert!(
        first.chars().all(|c| c == ' '),
        "no dots: {first:?}"
    );
}

#[component]
fn Wash() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            canvas(
                Painter::new()
                    .global_alpha(0.3)
                    .fill_color(nsh_core::color::rgb(0xffffff))
                    .fill_rect(0.0, 0.0, 64.0, 32.0),
            )
            .label("drawing")
            .w(px(64.0))
            .h(px(32.0)),
        )
        .child(text("drawn"))
}

#[test]
fn a_translucent_fill_is_a_sparse_floor_of_dots() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<Wash>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "drawn");
    let buffer = terminal.backend().buffer();
    let first = row(buffer, 0, 0..4);
    assert!(
        first.chars().all(|c| c == '⡀'),
        "one dot per Cell, not a block: {first:?}"
    );
}

/// Hatching at a few percent over a zone, and a line drawn over it.
#[component]
fn Hatched() -> impl IntoElement {
    let mut painter = Painter::new()
        .stroke_color(nsh_core::color::rgba(0xffffff0f))
        .begin_path();
    for i in (-32..64).step_by(7) {
        painter = painter
            .move_to(i as f32, 32.0)
            .line_to(i as f32 + 32.0, 0.0);
    }
    view()
        .flex_1()
        .child(
            canvas(
                painter
                    .stroke()
                    .stroke_color(nsh_core::color::rgb(GREEN))
                    .begin_path()
                    .move_to(0.0, 2.0)
                    .line_to(64.0, 2.0)
                    .stroke(),
            )
            .label("drawing")
            .w(px(64.0))
            .h(px(32.0)),
        )
        .child(text("drawn"))
}

#[test]
fn a_nearly_transparent_stroke_is_texture_left_out() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<Hatched>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "drawn");
    let buffer = terminal.backend().buffer();
    assert!(
        row(buffer, 0, 0..4).chars().all(|c| c != ' '),
        "the line is drawn"
    );
    assert_eq!(row(buffer, 1, 0..4), "    ", "no hatching under it");
}

/// A label under a canvas drawn after it, as a ring's value in its middle.
#[component]
fn Under() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(160.0))
                .h(px(32.0))
                .child(text("49").absolute().top(px(0.0)).left(px(64.0)))
                .child(
                    canvas(
                        Painter::new()
                            .fill_color(nsh_core::color::rgb(GREEN))
                            .fill_rect(0.0, 0.0, 160.0, 16.0),
                    )
                    .label("ring")
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .w(px(160.0))
                    .h(px(32.0)),
                ),
        )
        .child(text("drawn"))
}

#[test]
fn a_canvas_over_a_text_leaves_it_and_a_cell_each_side_clear() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(24, 4)).unwrap();
    view.load(App::Rust(Root::new::<Under>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "drawn");
    let buffer = terminal.backend().buffer();
    let line = row(buffer, 0, 0..20);
    assert_eq!(row(buffer, 0, 7..11), " 49 ", "{line}");
    assert_eq!(row(buffer, 0, 5..7), "⣿⣿", "the fill goes on: {line}");
    assert_eq!(row(buffer, 0, 11..13), "⣿⣿", "the fill goes on: {line}");
}

/// A filled card over a canvas, as a photo card over a stage's dots.
#[component]
fn Covered() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(96.0))
                .h(px(32.0))
                .child(
                    canvas(
                        Painter::new()
                            .fill_color(nsh_core::color::rgb(GREEN))
                            .fill_rect(0.0, 0.0, 96.0, 32.0),
                    )
                    .label("dots")
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .w(px(96.0))
                    .h(px(32.0)),
                )
                .child(
                    view()
                        .absolute()
                        .top(px(0.0))
                        .left(px(32.0))
                        .w(px(32.0))
                        .h(px(32.0))
                        .bg(nsh_core::color::rgb(0x27272a))
                        .border_1()
                        .border_color(nsh_core::color::rgb(0x52525b)),
                ),
        )
        .child(text("drawn"))
}

#[test]
fn an_opaque_box_over_a_canvas_hides_its_dots() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(16, 4)).unwrap();
    view.load(App::Rust(Root::new::<Covered>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "drawn");
    let buffer = terminal.backend().buffer();
    assert_eq!(row(buffer, 0, 0..4), "⣿⣿⣿⣿", "the dots beside it");
    assert!(
        row(buffer, 0, 4..8)
            .chars()
            .all(|c| !('\u{2801}'..='\u{28ff}').contains(&c)),
        "no dots under the box: {:?}",
        row(buffer, 0, 0..12)
    );
    assert_eq!(row(buffer, 0, 8..12), "⣿⣿⣿⣿", "the dots past it");
}
