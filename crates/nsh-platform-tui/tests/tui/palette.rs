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
        Color::Rgb(31, 45, 68),
        "the accent tinted in"
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
fn selection_and_focus_are_select_bands_and_bold_on_surfaces() {
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
        if terminal.backend().buffer()[(0, 1)].bg == select {
            break;
        }
        assert!(rx.recv_deadline(deadline).is_ok(), "focus did not show");
    }
    let buffer = terminal.backend().buffer();
    assert!(buffer[(0, 1)].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(0, 1)].modifier.contains(Modifier::REVERSED));
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
fn a_popup_is_raised_framed_brighter_and_casts_a_shadow() {
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
    let shadow = Color::Rgb(14, 14, 14);
    assert_eq!(
        buffer[(x + 11, y)].bg,
        shadow,
        "right of the frame: {rows:#?}"
    );
    assert_eq!(
        buffer[(x + 4, y + 2)].bg,
        shadow,
        "below the frame: {rows:#?}"
    );
    assert_eq!(buffer[(x + 4, y + 3)].bg, Color::Reset);
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
    assert_ne!(buffer[(0, 0)].bg, buffer[(19, 0)].bg, "a ramp in 256 colors");
}
