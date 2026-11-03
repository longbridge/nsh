//! Colors: the app's colors drawn as the terminal theme's meanings (the
//! default foreground, dim, or an accent), no fills, and text drawn with its
//! weight and decorations.

use std::sync::Arc;

use nsh_core::{platform::ColorScheme, prelude::*};
use nsh_platform_tui::{App, AppOptions, AppView, ColorDepth, Root};
use ratatui_core::{
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier},
    terminal::Terminal,
};

use super::app::{draw_until, selected};

/// A surface on a terminal with `scheme`'s background that shows `depth`.
fn surface_with(
    scheme: ColorScheme,
    depth: ColorDepth,
) -> (AppView, crossbeam_channel::Receiver<()>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut options = AppOptions::default();
    options.capabilities.background = scheme;
    options.capabilities.colors = depth;
    options.wake = Some(Arc::new(move || {
        let _ = tx.send(());
    }));
    options.cache = nsh_engine::CacheOptions::off();
    (AppView::new(options), rx)
}

fn dim(buffer: &Buffer, x: u16, y: u16) -> bool {
    buffer[(x, y)].modifier.contains(Modifier::DIM)
}

#[component]
fn Card() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x3b82f6))
                .child(text("ink").text_color(nsh_core::color::rgb(0x22c55e))),
        )
        .child(text("ready").text_color(nsh_core::color::rgb(0xfafafa)))
}

#[test]
fn colors_are_the_terminals_meanings_at_every_depth_and_nothing_is_filled() {
    for depth in [
        ColorDepth::TrueColor,
        ColorDepth::Ansi256,
        ColorDepth::Ansi16,
    ] {
        let (mut view, rx) = surface_with(ColorScheme::Dark, depth);
        let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
        view.load(App::Rust(Root::new::<Card>(|| ()))).unwrap();
        draw_until(&mut view, &mut terminal, &rx, "ready");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 0)].symbol(), "i");
        assert_eq!(buffer[(0, 0)].fg, Color::Green, "an accent, {depth:?}");
        for x in [0, 9, 10] {
            assert_eq!(buffer[(x, 0)].bg, Color::Reset, "no fill at {x}, {depth:?}");
        }
        assert_eq!(buffer[(0, 1)].fg, Color::Reset, "the body, {depth:?}");
        assert!(!dim(buffer, 0, 1), "{depth:?}");
        assert_eq!(buffer[(0, 1)].bg, Color::Reset);
    }
}

#[component]
fn Accents() -> impl IntoElement {
    let line =
        |label: &'static str, color: u32| text(label).text_color(nsh_core::color::rgb(color));
    view()
        .flex_1()
        .child(line("red", 0xef4444))
        .child(line("amber", 0xf59e0b))
        .child(line("teal", 0x14b8a6))
        .child(line("violet", 0x8b5cf6))
        .child(line("pink", 0xec4899))
        .child(line("muted", 0x71717a))
        .child(line("ready", 0x09090b))
}

#[test]
fn saturated_colors_take_their_hues_accent_and_low_contrast_ones_are_dim() {
    let (mut view, rx) = surface_with(ColorScheme::Dark, ColorDepth::TrueColor);
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<Accents>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let accents = [
        Color::Red,
        Color::Yellow,
        Color::Cyan,
        Color::Blue,
        Color::Magenta,
    ];
    for (y, accent) in accents.into_iter().enumerate() {
        let y = y as u16;
        assert_eq!(buffer[(0, y)].fg, accent, "row {y}");
        assert!(!dim(buffer, 0, y), "row {y}");
    }
    assert_eq!(buffer[(0, 5)].fg, Color::Reset, "secondary text");
    assert!(dim(buffer, 0, 5), "never a gray of the palette");
    assert_eq!(buffer[(0, 6)].fg, Color::Reset);
    assert!(
        !dim(buffer, 0, 6),
        "black (unstyled text) is the default foreground on a dark page too"
    );
}

#[test]
fn a_light_page_reads_dark_text_as_the_body() {
    let (mut view, rx) = surface_with(ColorScheme::Light, ColorDepth::TrueColor);
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    view.load(App::Rust(Root::new::<Accents>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 6)].fg, Color::Reset);
    assert!(!dim(buffer, 0, 6));
}

#[component]
fn Translucent() -> impl IntoElement {
    view()
        .flex_1()
        .child(text("faint").text_color(nsh_core::color::rgba(0xffffff80)))
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .opacity(0.5)
                .bg(nsh_core::color::rgb(0xffffff))
                .child(text("half").text_color(nsh_core::color::rgb(0x000000))),
        )
        .child(text("gone").text_color(nsh_core::color::rgba(0xffffff00)))
        .child(text("ready"))
}

#[test]
fn translucent_text_reads_over_its_ground_and_opacity_changes_no_color() {
    let (mut view, rx) = surface_with(ColorScheme::Dark, ColorDepth::TrueColor);
    let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
    view.load(App::Rust(Root::new::<Translucent>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert!(dim(buffer, 0, 0), "half white over black is secondary");
    assert_eq!(
        buffer[(0, 1)].fg,
        Color::Reset,
        "black read on its white fill"
    );
    assert!(!dim(buffer, 0, 1), "opacity is not a tint");
    assert_eq!(buffer[(0, 1)].bg, Color::Reset);
    assert!(
        buffer[(0, 2)].modifier.contains(Modifier::HIDDEN),
        "a transparent color hides its text"
    );
}

#[component]
fn Emphasis() -> impl IntoElement {
    view()
        .flex_1()
        .child(text("semibold").font_weight(FontWeight::SEMIBOLD))
        .child(text("medium").font_weight(FontWeight(500)))
        .child(text("italic").italic())
        .child(text("underline").underline())
        .child(text("struck").line_through())
}

#[test]
fn weight_600_and_above_is_bold_and_style_and_decorations_stay() {
    let (mut view, rx) = surface_with(ColorScheme::Dark, ColorDepth::TrueColor);
    let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
    view.load(App::Rust(Root::new::<Emphasis>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "struck");
    let buffer = terminal.backend().buffer();
    assert!(buffer[(0, 0)].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(0, 1)].modifier.contains(Modifier::BOLD));
    assert!(buffer[(0, 2)].modifier.contains(Modifier::ITALIC));
    assert!(buffer[(0, 3)].modifier.contains(Modifier::UNDERLINED));
    assert!(buffer[(0, 4)].modifier.contains(Modifier::CROSSED_OUT));
}

#[component]
fn Gradient() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg_linear_to_r()
                .gradient_from(nsh_core::color::rgb(0x000000))
                .gradient_to(nsh_core::color::rgb(0xffffff)),
        )
        .child(text("ready"))
}

#[test]
fn a_gradient_draws_nothing() {
    let (mut view, rx) = surface_with(ColorScheme::Dark, ColorDepth::TrueColor);
    let mut terminal = Terminal::new(TestBackend::new(20, 3)).unwrap();
    view.load(App::Rust(Root::new::<Gradient>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    for x in [0, 9] {
        assert_eq!(buffer[(x, 0)].bg, Color::Reset, "{x}");
        assert_eq!(buffer[(x, 0)].symbol(), " ", "{x}");
    }
}

/// A surface on a dark terminal whose own colors are known: its surfaces
/// are steps of them.
fn surfaces() -> (AppView, crossbeam_channel::Receiver<()>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut options = AppOptions::default();
    options.capabilities.colors = ColorDepth::TrueColor;
    options.capabilities.background_color = Some([24; 3]);
    options.capabilities.foreground_color = Some([224; 3]);
    options.wake = Some(Arc::new(move || {
        let _ = tx.send(());
    }));
    options.cache = nsh_engine::CacheOptions::off();
    (AppView::new(options), rx)
}

#[component]
fn Layers() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x27272a))
                .child(text("raised")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x3b82f6))
                .child(text("accent").text_color(nsh_core::color::rgb(0xffffff))),
        )
        .child(text("muted").text_color(nsh_core::color::rgb(0x71717a)))
        .child(text("ready"))
}

fn row_of(buffer: &Buffer, word: &str) -> u16 {
    (0..buffer.area.height)
        .find(|&y| {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            row.contains(word)
        })
        .unwrap_or_else(|| panic!("{word} not drawn"))
}

#[test]
fn fills_are_steps_of_the_terminals_own_background() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    view.load(App::Rust(Root::new::<Layers>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let raised = row_of(buffer, "raised");
    assert_eq!(
        buffer[(0, raised)].bg,
        Color::Rgb(38, 38, 38),
        "one step up"
    );
    assert_eq!(
        buffer[(9, raised)].bg,
        Color::Rgb(38, 38, 38),
        "the whole frame"
    );
    assert_eq!(buffer[(10, raised)].bg, Color::Reset);
    let accent = row_of(buffer, "accent");
    assert_eq!(
        buffer[(0, accent)].bg,
        Color::Rgb(42, 77, 135),
        "the accent tinted in, opaque: the solid share"
    );
    let muted = row_of(buffer, "muted");
    assert_eq!(
        buffer[(0, muted)].fg,
        Color::Rgb(124, 124, 124),
        "a fade, not dim"
    );
    assert!(!dim(buffer, 0, muted));
    assert_eq!(
        buffer[(0, row_of(buffer, "ready"))].bg,
        Color::Reset,
        "the page is never painted"
    );
}

#[component]
fn Ragged() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(160.0))
                .bg(nsh_core::color::rgb(0x27272a))
                .child(text("short"))
                .child(text("a longer line")),
        )
        .child(text("ready"))
}

#[test]
fn a_filled_box_is_one_band_behind_texts_of_any_length() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
    view.load(App::Rust(Root::new::<Ragged>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let (short, long) = (row_of(buffer, "short"), row_of(buffer, "longer"));
    for y in short..=long {
        for x in 0..20 {
            assert_eq!(
                buffer[(x, y)].bg,
                Color::Rgb(38, 38, 38),
                "the card's Cell at {x}, {y}"
            );
        }
        assert_eq!(buffer[(20, y)].bg, Color::Reset, "past the card on row {y}");
    }
}

#[component]
fn Paged() -> impl IntoElement {
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x1c1d20))
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x27272a))
                .child(text("card")),
        )
        .child(text("ready"))
}

#[test]
fn the_apps_page_color_is_the_terminals_background() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    view.load(App::Rust(Root::new::<Paged>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(
        buffer[(19, 5)].bg,
        Color::Reset,
        "the page is never painted"
    );
    assert_eq!(buffer[(0, row_of(buffer, "ready"))].bg, Color::Reset);
    let card = row_of(buffer, "card");
    assert_eq!(
        buffer[(0, card)].bg,
        Color::Rgb(38, 38, 38),
        "a step above the page"
    );
}

#[component]
fn Banded() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .accessibility_state(selected())
                .child(text("row")),
        )
        .child(
            view()
                .w(px(80.0))
                .h(px(16.0))
                .on_press(|_: &()| {})
                .child(text("go")),
        )
        .child(text("ready"))
}

#[test]
fn selection_is_a_select_band_and_bold_and_focus_the_accent_on_surfaces() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<Banded>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let select = Color::Rgb(56, 56, 56);
    assert_eq!(buffer[(0, 0)].bg, select);
    assert_eq!(buffer[(9, 0)].bg, select, "the band spans the row's frame");
    assert!(buffer[(0, 0)].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(0, 0)].modifier.contains(Modifier::REVERSED));
    view.handle(nsh_platform_tui::Input::Key {
        key: "Tab".into(),
        modifiers: Default::default(),
        release: false,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if terminal.backend().buffer()[(0, 1)].bg == Color::Blue {
            break;
        }
        assert!(rx.recv_deadline(deadline).is_ok(), "focus did not show");
    }
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(9, 1)].bg, Color::Blue, "the band spans the row");
    assert!(!buffer[(0, 1)].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(0, 1)].modifier.contains(Modifier::REVERSED));
    assert_eq!(
        buffer[(0, 0)].bg,
        select,
        "the selection keeps its gray band"
    );
}

#[component]
fn Raised() -> impl IntoElement {
    let anchor = element_ref();
    view()
        .flex_1()
        .child(view().w(px(8.0)).h(px(16.0)).element_ref(&anchor))
        .child(text("behind behind behind"))
        .child(text("behind behind behind"))
        .child(text("behind behind behind"))
        .child(
            overlay()
                .anchor(&anchor)
                .placement(nsh_core::overlay::Placement::Bottom)
                .align(nsh_core::overlay::Align::Start)
                .child(
                    view()
                        .w(px(80.0))
                        .h(px(32.0))
                        .bg(nsh_core::color::rgb(0x18181b))
                        .label("Menu")
                        .child(text("pop")),
                ),
        )
        .child(text("ready"))
}

#[test]
fn a_popup_is_raised_framed_brighter_and_casts_no_shadow() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
    view.load(App::Rust(Root::new::<Raised>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "pop");
    let buffer = terminal.backend().buffer();
    let y = row_of(buffer, "pop");
    let rows: Vec<String> = (0..8)
        .map(|y| (0..24).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let row = &rows[usize::from(y)];
    let x = row[..row.find("pop").unwrap()].chars().count() as u16;
    assert_eq!(
        buffer[(x, y)].bg,
        Color::Rgb(38, 38, 38),
        "raised: {rows:#?}"
    );
    assert_eq!(
        buffer[(x + 10, y)].fg,
        Color::Rgb(114, 114, 114),
        "the popup's line"
    );
    assert!(rows[usize::from(y) - 1].contains("─ Menu ─"), "{rows:#?}");
    // Nothing around the frame changes: no shadow covers what is there.
    assert_eq!(
        buffer[(x + 11, y)].bg,
        Color::Reset,
        "right of the frame: {rows:#?}"
    );
    assert_eq!(
        buffer[(x + 4, y + 2)].bg,
        Color::Reset,
        "below the frame: {rows:#?}"
    );
}

fn pane(name: &'static str) -> impl IntoElement {
    view()
        .w(px(88.0))
        .border_1()
        .border_color(nsh_core::color::rgb(0x3f3f46))
        .py(px(16.0))
        .child(view().accessibility_state(selected()).child(text(name)))
        .child(view().on_press(|_: &()| {}).child(text("go")))
}

#[component]
fn Panes() -> impl IntoElement {
    view()
        .flex_1()
        .child(view().flex_row().child(pane("selA")).child(pane("selB")))
        .child(text("ready"))
}

#[test]
fn the_pane_holding_the_focus_shows_its_line_and_selection_in_the_accent() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(26, 8)).unwrap();
    view.load(App::Rust(Root::new::<Panes>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    view.handle(nsh_platform_tui::Input::Key {
        key: "Tab".into(),
        modifiers: Default::default(),
        release: false,
    });
    let find = |buffer: &Buffer, word: &str| {
        let y = row_of(buffer, word);
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        (row.find(word).unwrap() as u16, y)
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let (x, y) = find(buffer, "selA");
        if buffer[(x, y)].bg == Color::Blue {
            break;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "the focused pane's selection"
        );
    }
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..8)
        .map(|y| (0..26).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let (bx, by) = find(buffer, "selB");
    assert_eq!(
        buffer[(bx, by)].bg,
        Color::Rgb(56, 56, 56),
        "elsewhere gray: {rows:#?}"
    );
    let top = rows.iter().position(|r| r.contains('┌')).unwrap() as u16;
    assert_eq!(
        buffer[(0, top)].fg,
        Color::Blue,
        "the focused pane's line: {rows:#?}"
    );
    let second = rows[usize::from(top)].rfind('┌').unwrap();
    let second = rows[usize::from(top)][..second].chars().count() as u16;
    assert_eq!(
        buffer[(second, top)].fg,
        Color::Rgb(68, 68, 68),
        "{rows:#?}"
    );
}

#[component]
fn Swatches() -> impl IntoElement {
    let swatch = |color: u32| {
        view()
            .w(px(16.0))
            .h(px(16.0))
            .role(AccessibilityRole::Image)
            .bg(nsh_core::color::rgb(color))
    };
    view()
        .flex_1()
        .child(
            view()
                .flex_row()
                .child(swatch(0xff0000))
                .child(swatch(0x3b82f6))
                .child(swatch(0x27272a)),
        )
        .child(
            view()
                .flex_row()
                .child(swatch(0x1e3a8a).child(text("dk")))
                .child(swatch(0xfde047).child(text("lt"))),
        )
        .child(text("ready"))
}

/// A surface with the test ramp's background and foreground that shows
/// `depth`.
fn surfaces_at(depth: ColorDepth) -> (AppView, crossbeam_channel::Receiver<()>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut options = AppOptions::default();
    options.capabilities.colors = depth;
    options.capabilities.background_color = Some([24; 3]);
    options.capabilities.foreground_color = Some([224; 3]);
    options.wake = Some(Arc::new(move || {
        let _ = tx.send(());
    }));
    options.cache = nsh_engine::CacheOptions::off();
    (AppView::new(options), rx)
}

fn draw_swatches(view: &mut AppView, rx: &crossbeam_channel::Receiver<()>) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    view.load(App::Rust(Root::new::<Swatches>(|| ()))).unwrap();
    draw_until(view, &mut terminal, rx, "ready");
    terminal.backend().buffer().clone()
}

#[test]
fn a_swatch_is_its_own_color_with_text_in_black_or_white() {
    let (mut view, rx) = surfaces();
    let buffer = draw_swatches(&mut view, &rx);
    let bgs: Vec<Color> = (0..6).map(|x| buffer[(x, 0)].bg).collect();
    assert_eq!(
        bgs,
        [
            Color::Rgb(255, 0, 0),
            Color::Rgb(255, 0, 0),
            Color::Rgb(59, 130, 246),
            Color::Rgb(59, 130, 246),
            Color::Rgb(39, 39, 42),
            Color::Rgb(39, 39, 42),
        ],
        "each Cell is the app's color itself, not a ramp step or a tint"
    );
    let dark = row_of(&buffer, "dk");
    assert_eq!(
        buffer[(0, dark)].fg,
        Color::Rgb(255, 255, 255),
        "white on a dark swatch"
    );
    assert_eq!(
        buffer[(2, dark)].fg,
        Color::Rgb(0, 0, 0),
        "black on a light one"
    );

    let (mut view, rx) = surfaces_at(ColorDepth::Ansi256);
    let buffer = draw_swatches(&mut view, &rx);
    assert_eq!(
        buffer[(0, 0)].bg,
        Color::Indexed(196),
        "the nearest of 256 colors"
    );

    let (mut view, rx) = surface_with(ColorScheme::Dark, ColorDepth::TrueColor);
    let buffer = draw_swatches(&mut view, &rx);
    assert_eq!(
        buffer[(0, 0)].bg,
        Color::Reset,
        "the bandless look paints no fill"
    );
}

#[component]
fn Ramp() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(160.0))
                .h(px(16.0))
                .bg_linear_to_r()
                .gradient_from(nsh_core::color::rgb(0x000000))
                .gradient_to(nsh_core::color::rgb(0xffffff)),
        )
        .child(
            view()
                .w(px(160.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x27272a))
                .bg_linear_to_r()
                .gradient_from(nsh_core::color::rgb(0x000000))
                .gradient_to(nsh_core::color::rgb(0xffffff))
                .child(text("finish")),
        )
        .child(text("ready"))
}

#[test]
fn a_gradient_fills_each_cell_with_its_color_there() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(24, 4)).unwrap();
    view.load(App::Rust(Root::new::<Ramp>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let Color::Rgb(left, ..) = buffer[(0, 0)].bg else {
        panic!("the left Cell is filled: {:?}", buffer[(0, 0)].bg);
    };
    let Color::Rgb(right, ..) = buffer[(19, 0)].bg else {
        panic!("the right Cell is filled: {:?}", buffer[(19, 0)].bg);
    };
    assert!(left < 20 && right > 235, "black to white: {left}, {right}");
    assert_eq!(buffer[(20, 0)].bg, Color::Reset, "past the box");
    let finish = row_of(buffer, "finish");
    assert_eq!(
        buffer[(0, finish)].bg,
        Color::Rgb(38, 38, 38),
        "a gradient over a fill is its finish: the fill's step"
    );
    assert_eq!(buffer[(19, finish)].bg, buffer[(0, finish)].bg);

    let (mut view, rx) = surfaces_at(ColorDepth::Ansi256);
    let mut terminal = Terminal::new(TestBackend::new(24, 4)).unwrap();
    view.load(App::Rust(Root::new::<Ramp>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert!(matches!(buffer[(0, 0)].bg, Color::Indexed(_)));
    assert_ne!(
        buffer[(0, 0)].bg,
        buffer[(19, 0)].bg,
        "a ramp in 256 colors"
    );
}

#[component]
fn Fill() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view().w(px(160.0)).h(px(16.0)).child(
                view()
                    .w(px(160.0))
                    .h(px(16.0))
                    .bg(nsh_core::color::rgb(0x3b82f6))
                    .origin_left()
                    .scale_x(0.5),
            ),
        )
        .child(text("ready"))
}

#[test]
fn a_scaled_fill_takes_the_cells_of_its_scaled_box() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(24, 4)).unwrap();
    view.load(App::Rust(Root::new::<Fill>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_ne!(buffer[(0, 0)].bg, Color::Reset, "the fill from the left");
    assert_ne!(buffer[(9, 0)].bg, Color::Reset, "half of it");
    assert_eq!(
        buffer[(10, 0)].bg,
        Color::Reset,
        "the other half is not filled"
    );
}

/// A cell framed on every side, filled a step above the page, its border
/// close to its fill: one-time code cells.
#[component]
fn FramedCell() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(40.0))
                .h(px(48.0))
                .border_1()
                .border_color(nsh_core::color::rgb(0x2e2e2e))
                .rounded(px(6.0))
                .bg(nsh_core::color::rgb(0x262626))
                .child(text("7")),
        )
        .child(text("ready"))
}

#[test]
fn a_filled_framed_cell_keeps_the_border_its_layout_gave_it() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(12, 6)).unwrap();
    view.load(App::Rust(Root::new::<FramedCell>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..6)
        .map(|y| (0..12).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let top = rows
        .iter()
        .position(|row| row.contains('╭'))
        .unwrap_or_else(|| panic!("no top corners: {rows:#?}"));
    assert!(rows[top].contains('╮'), "{rows:#?}");
    assert!(
        rows.iter()
            .any(|row| row.contains('╰') && row.contains('╯')),
        "{rows:#?}"
    );
}

/// A command in a framed card: its inline list highlights an item for the
/// keys typed in its search field.
#[component]
fn CardCommand() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .border_1()
                .border_color(nsh_core::color::rgb(0x3f3f46))
                .py(px(16.0))
                .child(
                    nsh_base::command()
                        .items(vec![
                            nsh_base::CommandItem::new("a", "Alpha"),
                            nsh_base::CommandItem::new("b", "Beta"),
                        ])
                        .placeholder("Find")
                        .accessibility_label("Commands"),
                ),
        )
        .child(text("ready"))
}

#[test]
fn a_command_s_highlighted_item_in_the_focused_pane_is_a_full_row_accent_band() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(App::Rust(Root::new::<CardCommand>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let at = |buffer: &Buffer, word: &str| {
        let y = row_of(buffer, word);
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        (row[..row.find(word).unwrap()].chars().count() as u16, y)
    };
    {
        let buffer = terminal.backend().buffer();
        let (x, y) = at(buffer, "Alpha");
        assert_ne!(
            buffer[(x, y)].bg,
            Color::Blue,
            "no band before the focus comes"
        );
    }
    view.handle(nsh_platform_tui::Input::Key {
        key: "Tab".into(),
        modifiers: Default::default(),
        release: false,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let (x, y) = at(buffer, "Alpha");
        if buffer[(x, y)].bg == Color::Blue {
            break;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "the highlighted item's band"
        );
    }
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..10)
        .map(|y| (0..40).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let (_, y) = at(buffer, "Alpha");
    // The band runs to the frame's line and stops there: no Cell of it on
    // the line.
    let line = rows[usize::from(y)].rfind('│').unwrap();
    let line = rows[usize::from(y)][..line].chars().count() as u16;
    assert_eq!(
        buffer[(line - 1, y)].bg,
        Color::Blue,
        "to the frame: {rows:#?}"
    );
    assert_ne!(
        buffer[(line, y)].bg,
        Color::Blue,
        "not on the frame: {rows:#?}"
    );
    let (bx, by) = at(buffer, "Beta");
    assert_ne!(buffer[(bx, by)].bg, Color::Blue, "one band: {rows:#?}");
}

/// An avatar's presence dot: a round 10pt box of a green fill pinned to
/// its corner, over a raised chip.
#[component]
fn Presence() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view().flex_row().child(
                view()
                    .w(px(32.0))
                    .h(px(32.0))
                    .rounded(px(16.0))
                    .items_center()
                    .justify_center()
                    .bg(nsh_core::color::rgb(0x333333))
                    .child(text("MC"))
                    .child(
                        view()
                            .absolute()
                            .right(px(-1.0))
                            .bottom(px(-1.0))
                            .w(px(10.0))
                            .h(px(10.0))
                            .rounded(px(5.0))
                            .bg(nsh_core::color::rgb(0x22c55e)),
                    ),
            ),
        )
        .child(text("ready"))
}

#[test]
fn a_dot_is_a_disc_in_its_color_over_what_is_under_it() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<Presence>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..4)
        .map(|y| (0..12).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let (x, y) = (0..4)
        .flat_map(|y| (0..12).map(move |x| (x, y)))
        .find(|&(x, y)| buffer[(x, y)].symbol() == "●")
        .unwrap_or_else(|| panic!("no dot: {rows:#?}"));
    let cell = &buffer[(x, y)];
    assert_eq!(cell.fg, Color::Green, "in its color: {rows:#?}");
    let chip = buffer[(0, row_of(buffer, "MC"))].bg;
    assert_eq!(cell.bg, chip, "on the chip's band, not one of its own");
}

/// A toggle's dot that is off, gray, before its label in a row.
#[component]
fn GrayKey() -> impl IntoElement {
    view().flex_1().child(
        view()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .child(
                view()
                    .w(px(6.0))
                    .h(px(6.0))
                    .rounded(px(3.0))
                    .bg(nsh_core::color::rgb(0x9ca3af)),
            )
            .child(text("Lock")),
    )
}

#[test]
fn a_gray_dot_before_its_label_is_a_disc_not_a_band_cell() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(12, 3)).unwrap();
    view.load(App::Rust(Root::new::<GrayKey>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Lock");
    let buffer = terminal.backend().buffer();
    let row: String = (0..12).map(|x| buffer[(x, 0)].symbol()).collect();
    let x = (0..12)
        .find(|&x| buffer[(x, 0)].symbol() == "●")
        .unwrap_or_else(|| panic!("no dot: {row:?}"));
    assert_eq!(
        buffer[(x, 0)].bg,
        buffer[(x + 2, 0)].bg,
        "on what is under it, no band of its own: {row:?}"
    );
    assert_ne!(buffer[(x, 0)].fg, Color::Reset, "in its gray: {row:?}");
}

/// A four-digit code in filled, framed cells, reporting its focus.
#[component]
fn FilledCode() -> impl IntoElement {
    let focused = signal("no");
    view()
        .flex_1()
        .child(text(move || format!("focus {}", focused.get())))
        .child(
            view().flex_row().child(
                nsh_base::otp_input()
                    .length(4u32)
                    .group_class("flex-row gap-2")
                    .cell_class(
                        "w-12 h-12 rounded-lg border border-zinc-700 bg-zinc-800 items-center \
                         justify-center",
                    )
                    .on_focus(move || focused.set("yes")),
            ),
        )
}

#[test]
fn a_codes_next_cell_shows_focus_with_its_frame_and_no_band() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(48, 8)).unwrap();
    view.load(App::Rust(Root::new::<FilledCode>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "focus no");
    view.handle(nsh_platform_tui::Input::Key {
        key: "Tab".into(),
        modifiers: Default::default(),
        release: false,
    });
    draw_until(&mut view, &mut terminal, &rx, "focus yes");
    let corners = |buffer: &Buffer| -> Vec<(u16, u16)> {
        (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buffer[(x, y)].symbol() == "╭")
            .collect()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let at = corners(buffer);
        if at.len() == 4 && buffer[at[0]].fg != buffer[at[1]].fg {
            break;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "the next cell's frame in the focus color\n{}",
            super::dock::dump(&terminal)
        );
    }
    let buffer = terminal.backend().buffer();
    let at = corners(buffer);
    assert_eq!(at.len(), 4, "every cell keeps its frame");
    let (x, y) = at[0];
    for dx in 0..3 {
        assert_ne!(
            buffer[(x + dx, y + 1)].bg,
            Color::Blue,
            "nothing filled in the accent\n{}",
            super::dock::dump(&terminal)
        );
    }
    assert_eq!(
        buffer[(x + 1, y + 1)].bg,
        buffer[(at[1].0 + 1, at[1].1 + 1)].bg,
        "the same fill as the other cells"
    );
}

/// A pane whose selected row holds a shortcut's key chip, a fill of its own.
#[component]
fn ChipRow() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(160.0))
                .border_1()
                .border_color(nsh_core::color::rgb(0x3f3f46))
                .py(px(16.0))
                .child(
                    view()
                        .flex_row()
                        .justify_between()
                        .accessibility_state(selected())
                        .child(text("Inbox"))
                        .child(
                            view()
                                .px(px(4.0))
                                .bg(nsh_core::color::rgb(0x333333))
                                .child(text("K1")),
                        ),
                )
                .child(view().on_press(|_: &()| {}).child(text("go"))),
        )
        .child(text("ready"))
}

#[test]
fn an_accent_band_spans_its_whole_row_over_a_child_s_fill() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    view.load(App::Rust(Root::new::<ChipRow>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    view.handle(nsh_platform_tui::Input::Key {
        key: "Tab".into(),
        modifiers: Default::default(),
        release: false,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let y = row_of(buffer, "Inbox");
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        let x = row[..row.find("Inbox").unwrap()].chars().count() as u16;
        if buffer[(x, y)].bg == Color::Blue {
            break;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "the selected row's band"
        );
    }
    let buffer = terminal.backend().buffer();
    let y = row_of(buffer, "Inbox");
    let row: Vec<&str> = (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect();
    let left = row.iter().position(|&s| s == "│").unwrap() as u16;
    let right = row.iter().rposition(|&s| s == "│").unwrap() as u16;
    for x in left + 1..right {
        assert_eq!(
            buffer[(x, y)].bg,
            Color::Blue,
            "column {x} of the row on the band: {row:?}"
        );
    }
}

/// A selected row's olive fill as light as the muted gray, a muted size on
/// it, and the same size on the page.
#[component]
fn FadedOnBand() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .bg(nsh_core::color::rgb(0x565f44))
                .child(text("3.8 KB").text_color(nsh_core::color::rgb(0x71717a))),
        )
        .child(text("1.4 KB").text_color(nsh_core::color::rgb(0x71717a)))
        .child(text("ready"))
}

#[test]
fn a_fade_as_light_as_its_band_reads_at_full_strength() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(16, 4)).unwrap();
    view.load(App::Rust(Root::new::<FadedOnBand>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let band = row_of(buffer, "3.8");
    let page = row_of(buffer, "1.4");
    assert_ne!(buffer[(0, band)].bg, Color::Reset, "the row's band");
    assert_eq!(
        buffer[(0, band)].fg,
        Color::Reset,
        "full strength on the band"
    );
    assert_ne!(buffer[(0, page)].fg, Color::Reset, "muted on the page");
}

/// A selected row's translucent tint (the app's accent at a quarter), a
/// faint size on it, and the same size on the page.
#[component]
fn FadedOnTint() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .bg(nsh_core::color::rgba(0xc8f26d40))
                .child(text("3.8 KB").text_color(nsh_core::color::rgb(0x52525b))),
        )
        .child(text("1.4 KB").text_color(nsh_core::color::rgb(0x52525b)))
        .child(text("ready"))
}

/// CIE L* of a Cell's color, the terminal's own colors for `Reset`.
fn cell_lightness(color: Color, reset: u8) -> f32 {
    let [r, g, b] = match color {
        Color::Rgb(r, g, b) => [r, g, b],
        _ => [reset; 3],
    };
    let linear = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let y = 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
    if y > 216.0 / 24389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24389.0 / 27.0
    }
}

#[test]
fn a_fade_on_a_translucent_tint_reads_against_the_band_drawn_there() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(16, 4)).unwrap();
    view.load(App::Rust(Root::new::<FadedOnTint>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let band = row_of(buffer, "3.8");
    let cell = &buffer[(0, band)];
    assert_ne!(cell.bg, Color::Reset, "the row's tint");
    let apart = (cell_lightness(cell.fg, 224) - cell_lightness(cell.bg, 24)).abs();
    assert!(
        apart >= 15.0,
        "the size reads on its tint: {cell:?}, {apart}"
    );
}

/// A sidebar with only its right line: its first row, the box's top,
/// holds a selected toggle button (the gallery's "Sidebar").
#[component]
fn SidebarToggle() -> impl IntoElement {
    view()
        .flex_1()
        .flex_row()
        .child(
            view()
                .w(px(120.0))
                .border_r_1()
                .border_color(nsh_core::color::rgb(0x3f3f46))
                .child(
                    view()
                        .on_press(|_: &()| {})
                        .accessibility_state(selected())
                        .child(text("Sidebar")),
                )
                .child(text("Inbox")),
        )
        .child(text("ready"))
}

#[test]
fn a_selected_button_on_the_accent_band_reads_in_the_band_s_contrast() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(30, 6)).unwrap();
    view.load(App::Rust(Root::new::<SidebarToggle>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    view.handle(nsh_platform_tui::Input::Key {
        key: "Tab".into(),
        modifiers: Default::default(),
        release: false,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let at = |buffer: &Buffer| {
        let y = row_of(buffer, "Sidebar");
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        (
            row[..row.find("Sidebar").unwrap()].chars().count() as u16,
            y,
        )
    };
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        if buffer[at(buffer)].bg == Color::Blue {
            break;
        }
        assert!(rx.recv_deadline(deadline).is_ok(), "the button's band");
    }
    let buffer = terminal.backend().buffer();
    let (x, y) = at(buffer);
    for x in x..x + 7 {
        assert_eq!(buffer[(x, y)].bg, Color::Blue);
        assert_ne!(
            buffer[(x, y)].fg,
            Color::Blue,
            "column {x}: the label reads on its band"
        );
    }
}

/// Tabs once and draws until `shown` holds of the buffer.
fn tab_until(
    view: &mut AppView,
    terminal: &mut Terminal<TestBackend>,
    rx: &crossbeam_channel::Receiver<()>,
    what: &str,
    shown: impl Fn(&Buffer) -> bool,
) {
    view.handle(nsh_platform_tui::Input::Key {
        key: "Tab".into(),
        modifiers: Default::default(),
        release: false,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if shown(terminal.backend().buffer()) {
            return;
        }
        assert!(rx.recv_deadline(deadline).is_ok(), "{what}");
    }
}

/// The Cell where `word` starts.
fn cell_of(buffer: &Buffer, word: &str) -> (u16, u16) {
    let y = row_of(buffer, word);
    let row: String = (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect();
    (row[..row.find(word).unwrap()].chars().count() as u16, y)
}

/// A pressable whose only child is a chip of its own fill (the list
/// demo's "Jump to 60"), and a swatch: a pressable holding nothing but a
/// color.
#[component]
fn CoveredControls() -> impl IntoElement {
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x222222))
        .child(
            view().on_press(|_: &()| {}).child(
                view()
                    .flex_row()
                    .px(px(8.0))
                    .bg(nsh_core::color::rgb(0x3a3a3a))
                    .child(text("Jump")),
            ),
        )
        .child(
            view().flex_row().child(text("color ")).child(
                view().on_press(|_: &()| {}).child(
                    view()
                        .w(px(32.0))
                        .h(px(16.0))
                        .role(AccessibilityRole::Image)
                        .bg(nsh_core::color::rgb(0x22c55e)),
                ),
            ),
        )
        .child(text("ready"))
}

#[test]
fn a_control_covered_by_its_children_s_fills_shows_focus_with_an_accent_underline() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    view.load(App::Rust(Root::new::<CoveredControls>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let chip = {
        let buffer = terminal.backend().buffer();
        let at = cell_of(buffer, "Jump");
        assert!(
            !buffer[at].modifier.contains(Modifier::UNDERLINED),
            "unfocused, the chip is plain"
        );
        buffer[at].bg
    };
    tab_until(
        &mut view,
        &mut terminal,
        &rx,
        "the chip's focus",
        |buffer| {
            buffer[cell_of(buffer, "Jump")]
                .modifier
                .contains(Modifier::UNDERLINED)
        },
    );
    let buffer = terminal.backend().buffer();
    let (x, y) = cell_of(buffer, "Jump");
    for x in x..x + 4 {
        assert_eq!(buffer[(x, y)].underline_color, Color::Blue);
        assert_eq!(buffer[(x, y)].bg, chip, "the chip keeps its fill");
    }
    // The swatch: a color and no glyph, underlined under each of its Cells.
    let swatch = |buffer: &Buffer| {
        let (x, y) = cell_of(buffer, "color ");
        (x + 6, y)
    };
    let color = buffer[swatch(buffer)].bg;
    assert!(
        !buffer[swatch(buffer)]
            .modifier
            .contains(Modifier::UNDERLINED)
    );
    tab_until(
        &mut view,
        &mut terminal,
        &rx,
        "the swatch's focus",
        |buffer| {
            buffer[swatch(buffer)]
                .modifier
                .contains(Modifier::UNDERLINED)
        },
    );
    let buffer = terminal.backend().buffer();
    let (x, y) = swatch(buffer);
    for x in x..x + 4 {
        assert_eq!(buffer[(x, y)].underline_color, Color::Blue);
        assert_eq!(buffer[(x, y)].bg, color, "the swatch keeps its color");
    }
    let (x, y) = cell_of(buffer, "Jump");
    assert!(
        !buffer[(x, y)].modifier.contains(Modifier::UNDERLINED),
        "the chip's focus left with it"
    );
}

/// A framed box of swatches: the chosen one (selected) first.
#[component]
fn ChosenSwatch() -> impl IntoElement {
    let swatch = |color: u32| {
        view().on_press(|_: &()| {}).child(
            view()
                .w(px(24.0))
                .h(px(16.0))
                .role(AccessibilityRole::Image)
                .bg(nsh_core::color::rgb(color)),
        )
    };
    view()
        .flex_1()
        .child(
            view()
                .border_1()
                .border_color(nsh_core::color::rgb(0x3f3f46))
                .flex_row()
                .child(text("color "))
                .child(swatch(0x3b82f6).accessibility_state(selected()))
                .child(text(" "))
                .child(swatch(0x22c55e)),
        )
        .child(text("ready"))
}

#[test]
fn a_selected_swatch_in_the_focused_pane_keeps_its_color() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
    view.load(App::Rust(Root::new::<ChosenSwatch>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let chosen = |buffer: &Buffer| {
        let (x, y) = cell_of(buffer, "color ");
        (x + 6, y)
    };
    let color = terminal.backend().buffer()[chosen(terminal.backend().buffer())].bg;
    assert_ne!(color, Color::Blue, "its own color, not the accent");
    {
        // Unfocused, the chosen swatch shows a check in its middle, in a
        // color that reads on it; the other has none.
        let buffer = terminal.backend().buffer();
        let (x, y) = chosen(buffer);
        let check = &buffer[(x + 1, y)];
        assert_eq!(check.symbol(), "✓", "the chosen swatch's check");
        assert_eq!(check.bg, color);
        assert_ne!(check.fg, color);
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        assert_eq!(row.matches('✓').count(), 1, "only the chosen one: {row}");
    }
    tab_until(
        &mut view,
        &mut terminal,
        &rx,
        "the chosen swatch's focus",
        |buffer| {
            buffer[chosen(buffer)]
                .modifier
                .contains(Modifier::UNDERLINED)
        },
    );
    let buffer = terminal.backend().buffer();
    let (x, y) = chosen(buffer);
    for x in x..x + 3 {
        assert_eq!(
            buffer[(x, y)].bg,
            color,
            "the chosen swatch keeps its color in the focused pane"
        );
        assert_eq!(buffer[(x, y)].underline_color, Color::Blue);
    }
}

/// A framed bar of toggles, the first on (a toolbar's Bold).
#[component]
fn ToggleBar() -> impl IntoElement {
    let toggle = |label: &'static str, on: bool| {
        view()
            .on_press(|_: &()| {})
            .accessibility_state(nsh_core::AccessibilityState {
                pressed: on.then_some(nsh_core::Checked::True),
                ..Default::default()
            })
            .child(text(label))
    };
    view()
        .flex_1()
        .child(
            view()
                .border_1()
                .border_color(nsh_core::color::rgb(0x3f3f46))
                .flex_row()
                .gap(px(8.0))
                .child(toggle("Bold", true))
                .child(toggle("Italic", false)),
        )
        .child(text("ready"))
}

#[test]
fn a_toggle_that_is_on_is_bold_on_a_gray_band_and_the_focus_is_the_accent() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
    view.load(App::Rust(Root::new::<ToggleBar>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let at = |buffer: &Buffer, word: &str| buffer[cell_of(buffer, word)].clone();
    let gray = at(terminal.backend().buffer(), "Bold").bg;
    tab_until(&mut view, &mut terminal, &rx, "Bold's focus", |buffer| {
        at(buffer, "Bold").bg == Color::Blue
    });
    let bold = at(terminal.backend().buffer(), "Bold");
    assert!(bold.modifier.contains(Modifier::BOLD), "on, and focused");
    // The focus moves on: the toggle that is on keeps its gray band and
    // bold in the focused pane, never the accent; the focused one, off,
    // takes the accent without bold.
    tab_until(&mut view, &mut terminal, &rx, "Italic's focus", |buffer| {
        at(buffer, "Italic").bg == Color::Blue
    });
    let buffer = terminal.backend().buffer();
    let bold = at(buffer, "Bold");
    assert_ne!(bold.bg, Color::Blue, "on is not the accent");
    assert_eq!(bold.bg, gray, "on keeps its gray band");
    assert!(bold.modifier.contains(Modifier::BOLD));
    assert!(!at(buffer, "Italic").modifier.contains(Modifier::BOLD));
}

/// A color area's grid: an adjustable view whose picked Cell is `◆` on a
/// color.
#[component]
fn AreaMark() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .focusable(true)
                .accessible(true)
                .role(AccessibilityRole::Adjustable)
                .flex_row()
                .child(
                    view()
                        .w(px(8.0))
                        .role(AccessibilityRole::Image)
                        .bg(nsh_core::color::rgb(0x3b82f6))
                        .child(text(" ")),
                )
                .child(
                    view()
                        .w(px(8.0))
                        .role(AccessibilityRole::Image)
                        .bg(nsh_core::color::rgb(0x2563eb))
                        .child(text("◆")),
                ),
        )
        .child(text("ready"))
}

#[test]
fn a_color_area_s_picked_cell_is_reversed_while_it_has_the_focus() {
    let (mut view, rx) = surfaces();
    let mut terminal = Terminal::new(TestBackend::new(12, 4)).unwrap();
    view.load(App::Rust(Root::new::<AreaMark>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let mark = |buffer: &Buffer| buffer[cell_of(buffer, "◆")].modifier;
    assert!(!mark(terminal.backend().buffer()).contains(Modifier::REVERSED));
    tab_until(
        &mut view,
        &mut terminal,
        &rx,
        "the picked Cell's focus",
        |buffer| mark(buffer).contains(Modifier::REVERSED),
    );
}
