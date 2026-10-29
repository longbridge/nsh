use nsh_core::prelude::*;
use nsh_platform_tui::{App, Root};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

use super::app::{draw_until, reversed, surface};

/// The palette color of a leaf fill drawn at `(x, y)`: leaves whose fill is
/// their content show reversed in it.
fn solid(buffer: &ratatui_core::buffer::Buffer, x: u16, y: u16) -> Option<Color> {
    reversed(buffer, x, y).then_some(buffer[(x, y)].fg)
}

#[component]
fn AbsoluteEdges() -> impl IntoElement {
    view().flex_1().children([
        view().absolute().left(px(3.0)).top(px(3.0)).children([
            view()
                .absolute()
                .left(px(3.0))
                .top(px(3.0))
                .child(text("nested").pl(px(3.0)).pt(px(3.0))),
            view()
                .absolute()
                .left(px(16.0))
                .top(px(48.0))
                .w(px(27.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x123456)),
            view()
                .absolute()
                .left(px(51.0))
                .top(px(48.0))
                .w(px(27.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x123456)),
            view()
                .absolute()
                .left(px(86.0))
                .top(px(48.0))
                .w(px(27.0))
                .h(px(16.0))
                .bg(nsh_core::color::rgb(0x123456)),
        ]),
        view().absolute().top(px(112.0)).child(text("ready")),
    ])
}

#[test]
fn absolute_edges_preserve_accumulated_padding_and_equal_sibling_gaps() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    view.load(App::Rust(Root::new::<AbsoluteEdges>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].symbol(), "n");
    let painted: Vec<_> = (0..20)
        .filter(|&x| solid(buffer, x, 3) == Some(Color::Blue))
        .collect();
    assert_eq!(painted, [2, 3, 4, 5, 7, 8, 9, 11, 12, 13, 14]);
}

#[component]
fn NestedBorders() -> impl IntoElement {
    view().flex_1().children([
        view()
            .absolute()
            .left(px(16.0))
            .top(px(32.0))
            .w(px(96.0))
            .h(px(96.0))
            .border_1()
            .rounded(px(8.0))
            .border_color(nsh_core::color::rgb(0xffffff))
            .p(px(1.0))
            .child(
                view()
                    .flex_1()
                    .border_1()
                    .rounded(px(8.0))
                    .border_color(nsh_core::color::rgb(0xffffff))
                    .bg(nsh_core::color::rgb(0x123456))
                    .child(
                        canvas(
                            Painter::new()
                                .fill_color(nsh_core::color::rgb(0xef4444))
                                .fill_rect(0.0, 0.0, 96.0, 96.0),
                        )
                        .label("drawing")
                        .absolute()
                        .inset(px(0.0)),
                    ),
            ),
        view().absolute().top(px(144.0)).child(text("ready")),
    ])
}

#[test]
fn nested_borders_own_distinct_cells_and_clip_descendant_canvas() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 12)).unwrap();
    view.load(App::Rust(Root::new::<NestedBorders>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(2, 2)].symbol(), "┌");
    assert_eq!(buffer[(13, 7)].symbol(), "┘");
    assert_eq!(buffer[(3, 3)].symbol(), "┌");
    assert_eq!(buffer[(12, 6)].symbol(), "┘");
    assert_eq!(buffer[(5, 5)].symbol(), "⣿");
    for y in 3..7 {
        assert_eq!(buffer[(2, y)].symbol(), "│");
        assert_eq!(buffer[(13, y)].symbol(), "│");
    }
}

#[component]
fn CompactBorders() -> impl IntoElement {
    view().flex_1().children([
        view()
            .absolute()
            .left(px(16.0))
            .top(px(16.0))
            .w(px(80.0))
            .h(px(32.0))
            .border_1()
            .rounded(px(8.0))
            .items_center()
            .justify_center()
            .child(text("chip")),
        view().child(
            text("own")
                .absolute()
                .left(px(16.0))
                .top(px(64.0))
                .w(px(80.0))
                .h(px(16.0))
                .border_1()
                .rounded(px(8.0)),
        ),
        view()
            .absolute()
            .left(px(16.0))
            .top(px(96.0))
            .w(px(16.0))
            .h(px(16.0))
            .border_1()
            .rounded(px(8.0))
            .bg(nsh_core::color::rgb(0x123456)),
        view()
            .absolute()
            .left(px(128.0))
            .top(px(16.0))
            .w(px(80.0))
            .h(px(32.0))
            .border_1()
            .rounded(px(0.0)),
        view().absolute().top(px(144.0)).child(text("ready")),
    ])
}

#[test]
fn compact_boxes_draw_no_border_and_keep_their_labels_in_place() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 12)).unwrap();
    view.load(App::Rust(Root::new::<CompactBorders>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..8)
        .map(|y| (0..16).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    assert!(
        rows[1..3].iter().any(|row| row.contains("chip")),
        "{rows:?}"
    );
    assert!(rows[4].contains("own"), "{rows:?}");
    // No room for a whole border: none at all, and the labels stay at the
    // columns the layout gave them.
    for y in [1, 2, 4] {
        assert!(
            !rows[y as usize].contains(['│', '─', '┌', '┐', '└', '┘']),
            "{rows:?}"
        );
    }
    assert_eq!(rows[4].find("own"), Some(2));
    assert_eq!(buffer[(2, 6)].symbol(), " ");
    assert_eq!(buffer[(3, 6)].symbol(), " ");
    assert_eq!(solid(buffer, 2, 6), Some(Color::Blue));
    assert_eq!(buffer[(16, 1)].symbol(), "┌");
    assert_eq!(buffer[(25, 2)].symbol(), "┘");
}

#[component]
fn RoundedCoverage() -> impl IntoElement {
    view().flex_1().children([
        view()
            .absolute()
            .left(px(16.0))
            .top(px(16.0))
            .w(px(96.0))
            .h(px(96.0))
            .rounded(px(48.0))
            .bg(nsh_core::color::rgb(0x123456)),
        view()
            .absolute()
            .left(px(128.0))
            .top(px(16.0))
            .w(px(96.0))
            .h(px(96.0))
            .rounded_tl(px(48.0))
            .overflow_hidden()
            .child(
                view()
                    .absolute()
                    .inset(px(0.0))
                    .bg(nsh_core::color::rgb(0xef4444)),
            ),
        view()
            .absolute()
            .left(px(16.0))
            .top(px(128.0))
            .w(px(80.0))
            .h(px(64.0))
            .border_1()
            .rounded_tl(px(8.0))
            .rounded_br(px(8.0)),
        view().absolute().top(px(208.0)).child(text("ready")),
    ])
}

#[test]
fn rounded_fills_and_clips_keep_the_apps_radii_and_borders_draw_straight_corners() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(32, 16)).unwrap();
    view.load(App::Rust(Root::new::<RoundedCoverage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    for (x, y) in [(2, 1), (13, 1), (2, 6), (13, 6), (16, 1)] {
        assert_eq!(solid(buffer, x, y), None, "corner {x},{y}");
    }
    assert_eq!(solid(buffer, 7, 1), Some(Color::Blue));
    assert_eq!(solid(buffer, 2, 3), Some(Color::Blue));
    assert_eq!(solid(buffer, 27, 1), Some(Color::Red));
    assert_eq!(solid(buffer, 16, 6), Some(Color::Red));
    assert_eq!(buffer[(2, 8)].symbol(), "┌");
    assert_eq!(buffer[(11, 8)].symbol(), "┐");
    assert_eq!(buffer[(2, 11)].symbol(), "└");
    assert_eq!(buffer[(11, 11)].symbol(), "┘");
}

#[component]
fn CenteredIcons() -> impl IntoElement {
    view()
        .flex_1()
        .children((0..24).map(|index| {
            view()
                .absolute()
                .left(px(index as f32 * 32.0))
                .top(px(index as f32))
                .h(px(36.0))
                .flex_row()
                .items_center()
                .child(
                    canvas(
                        Painter::new()
                            .fill_color(nsh_core::color::rgb(0xffffff))
                            .fill_rect(0.0, 0.0, 8.0, 17.0),
                    )
                    .label("drawing")
                    .w(px(8.0))
                    .h(px(17.0)),
                )
                .child(text("X"))
                .child(
                    view()
                        .w(px(8.0))
                        .h(px(17.0))
                        .rounded(px(4.0))
                        .bg(nsh_core::color::rgb(0x123456)),
                )
        }))
        .child(view().absolute().top(px(120.0)).child(text("ready")))
}

#[test]
fn centered_icons_and_labels_share_a_row_at_every_cell_height_and_phase() {
    use std::sync::Arc;

    use nsh_platform_tui::{AppOptions, AppView, CellSize};
    for height in [16.0, 20.0, 24.0] {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut options = AppOptions::default();
        options.cells = CellSize::new(px(8.0), px(height)).unwrap();
        options.wake = Some(Arc::new(move || {
            let _ = tx.send(());
        }));
        options.cache = nsh_engine::CacheOptions::off();
        let mut view = AppView::new(options);
        let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
        view.load(App::Rust(Root::new::<CenteredIcons>(|| ())))
            .unwrap();
        draw_until(&mut view, &mut terminal, &rx, "ready");
        let buffer = terminal.backend().buffer();
        for column in (0..96).step_by(4) {
            let label_row = (0..4)
                .find(|&y| buffer[(column + 1, y)].symbol() == "X")
                .unwrap();
            let icon_rows: Vec<_> = (0..4)
                .filter(|&y| buffer[(column, y)].symbol() != " ")
                .collect();
            assert_eq!(icon_rows, [label_row], "height {height}, column {column}");
            let indicator_rows: Vec<_> = (0..4)
                .filter(|&y| solid(buffer, column + 2, y) == Some(Color::Blue))
                .collect();
            assert_eq!(
                indicator_rows,
                [label_row],
                "indicator height {height}, column {column}"
            );
        }
    }
}

#[test]
fn search_highlights_the_projected_text_after_accumulated_padding() {
    use nsh_core::keymap::Modifiers;
    use nsh_platform_tui::Input;
    use ratatui_core::style::Modifier;
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
    view.load(App::Rust(Root::new::<AbsoluteEdges>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    view.handle(Input::Key {
        key: "s".into(),
        modifiers: Modifiers {
            ctrl: true,
            ..Default::default()
        },
        release: false,
    });
    view.handle(Input::Key {
        key: "n".into(),
        modifiers: Default::default(),
        release: false,
    });
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    assert!(
        terminal.backend().buffer()[(1, 1)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    assert!(
        !terminal.backend().buffer()[(2, 1)]
            .modifier
            .contains(Modifier::REVERSED)
    );
}

#[component]
fn BorderAdjacentLabels() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .w(px(200.0))
                .h(px(48.0))
                .border_b(px(1.0))
                .justify_center()
                .child(view().child(text("Title")).child(text("Description"))),
        )
        .child(text("ready"))
}

#[test]
fn a_border_adjacent_text_group_keeps_both_lines_inside_the_box() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 12)).unwrap();
    view.load(App::Rust(Root::new::<BorderAdjacentLabels>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "T");
    assert_eq!(buffer[(0, 1)].symbol(), "D");
    assert_eq!(buffer[(0, 2)].symbol(), "─");
}

#[component]
fn AnchoredTooltip() -> impl IntoElement {
    let anchor = element_ref();
    view()
        .flex_1()
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(216.0))
                .w(px(80.0))
                .h(px(40.0))
                .bg(nsh_core::color::rgb(0x123456))
                .element_ref(&anchor),
        )
        .child(
            overlay()
                .anchor(&anchor)
                .placement(nsh_core::overlay::Placement::Bottom)
                .align(nsh_core::overlay::Align::Start)
                .offset(px(4.0))
                .child(text("tooltip")),
        )
        .child(view().absolute().top(px(400.0)).child(text("ready")))
}

#[test]
fn a_bottom_tooltip_starts_after_the_anchors_last_cell_row() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 32)).unwrap();
    view.load(App::Rust(Root::new::<AnchoredTooltip>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "tooltip");
    let buffer = terminal.backend().buffer();
    assert_eq!(solid(buffer, 2, 15), Some(Color::Blue));
    assert_eq!(buffer[(2, 15)].symbol(), " ");
    assert_eq!(buffer[(2, 16)].symbol(), "t");
    assert_eq!(solid(buffer, 2, 16), None);
}

#[component]
fn ThinAnchor() -> impl IntoElement {
    let anchor = element_ref();
    view()
        .flex_1()
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(8.0))
                .w(px(80.0))
                .h(px(4.0))
                .bg(nsh_core::color::rgb(0x123456))
                .element_ref(&anchor),
        )
        .child(
            overlay()
                .anchor(&anchor)
                .placement(nsh_core::overlay::Placement::Bottom)
                .align(nsh_core::overlay::Align::Start)
                .offset(px(0.0))
                .h(px(4.0))
                .w(px(80.0))
                .rounded(px(2.0))
                .bg(nsh_core::color::rgb(0xef4444)),
        )
        .child(view().absolute().top(px(96.0)).child(text("ready")))
}

#[test]
fn even_a_subcell_anchor_keeps_its_row_when_a_tooltip_opens_below() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 12)).unwrap();
    view.load(App::Rust(Root::new::<ThinAnchor>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(2, 1)].symbol(), "─");
    assert_eq!(buffer[(2, 1)].fg, Color::Blue);
    assert_eq!(buffer[(2, 2)].fg, Color::Red);
}

#[component]
fn ThinAnchorsOnEverySide() -> impl IntoElement {
    use nsh_core::overlay::{Align, Placement};
    let moved = signal(false);
    view()
        .flex_1()
        .children(
            [
                Placement::Top,
                Placement::Bottom,
                Placement::Left,
                Placement::Right,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, side)| {
                let anchor = element_ref();
                view()
                    .child(
                        view()
                            .absolute()
                            .left(px(40.0 + index as f32 * 80.0))
                            .top(move || px(if moved.get() { 96.0 } else { 64.0 }))
                            .w(px(4.0))
                            .h(px(4.0))
                            .element_ref(&anchor),
                    )
                    .child(
                        overlay()
                            .anchor(&anchor)
                            .placement(side)
                            .align(Align::Start)
                            .offset(px(0.0))
                            .child(text("X")),
                    )
            }),
        )
        .child(
            view()
                .absolute()
                .top(px(160.0))
                .on_press(move |_: &()| moved.set(true))
                .child(text(move || if moved.get() { "moved" } else { "move" })),
        )
}

#[test]
fn overlays_follow_moving_subcell_anchors_on_all_four_sides() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(50, 16)).unwrap();
    view.load(App::Rust(Root::new::<ThinAnchorsOnEverySide>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "move");
    for row in [4, 6] {
        let buffer = terminal.backend().buffer();
        for (x, y) in [(5, row - 1), (15, row + 1), (24, row), (36, row)] {
            assert_eq!(buffer[(x, y)].symbol(), "X", "overlay {x},{y}");
        }
        if row == 4 {
            for key in ["Tab", "Enter"] {
                view.handle(nsh_platform_tui::Input::Key {
                    key: key.into(),
                    modifiers: Default::default(),
                    release: false,
                });
            }
            draw_until(&mut view, &mut terminal, &rx, "moved");
        }
    }
}

#[component]
fn CrowdedCard() -> impl IntoElement {
    let phase = signal(0u32);
    view()
        .flex_1()
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(move || px(96.0 + phase.get() as f32))
                .w(px(240.0))
                .border_1()
                .p(px(5.0))
                .child(
                    view()
                        .pt(px(11.0))
                        .pb(px(12.0))
                        .gap(px(2.0))
                        .child(text("Heading"))
                        .child(text("Description")),
                )
                .child(
                    view()
                        .border_1()
                        .p(px(14.0))
                        .child(view().children((0..4).map(|i| {
                            view().py(px(6.0)).child(
                                view()
                                    .gap(px(2.0))
                                    .child(text(format!("Body {i}")))
                                    .child(text(format!("Detail {i}"))),
                            )
                        }))),
                ),
        )
        .child(
            view()
                .absolute()
                .top(px(600.0))
                .on_press(move |_: &()| phase.update(|value| *value += 1))
                .child(text(move || format!("ready{}", phase.get()))),
        )
}

#[test]
fn fitting_a_card_header_does_not_let_the_next_inset_erase_its_subtitle() {
    use std::sync::Arc;

    use nsh_platform_tui::{AppOptions, AppView, CellSize};
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut options = AppOptions::default();
    options.cells = CellSize::new(px(8.0), px(24.0)).unwrap();
    options.wake = Some(Arc::new(move || {
        let _ = tx.send(());
    }));
    options.cache = nsh_engine::CacheOptions::off();
    let mut view = AppView::new(options);
    let mut terminal = Terminal::new(TestBackend::new(40, 30)).unwrap();
    view.load(App::Rust(Root::new::<CrowdedCard>(|| ())))
        .unwrap();
    for phase in 0..24 {
        draw_until(&mut view, &mut terminal, &rx, &format!("ready{phase}"));
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        for expected in [
            "Heading",
            "Description",
            "Body 0",
            "Detail 0",
            "Body 1",
            "Detail 1",
            "Body 2",
            "Detail 2",
            "Body 3",
            "Detail 3",
        ] {
            assert!(
                text.contains(expected),
                "phase {phase}: missing {expected}: {text}"
            );
        }
        if phase == 0 {
            view.handle(nsh_platform_tui::Input::Key {
                key: "Tab".into(),
                modifiers: Default::default(),
                release: false,
            });
        }
        view.handle(nsh_platform_tui::Input::Key {
            key: "Enter".into(),
            modifiers: Default::default(),
            release: false,
        });
    }
}

#[component]
fn QuantizedSeparators() -> impl IntoElement {
    view()
        .flex_1()
        .children([1.0, 16.0].into_iter().enumerate().map(|(index, height)| {
            view()
                .absolute()
                .left(px(16.0))
                .top(px(64.0 + index as f32 * 64.0))
                .w(px(160.0))
                .h(px(48.0))
                .child(text("Label").absolute().top(px(6.0)))
                .child(
                    view()
                        .absolute()
                        .top(px(0.0))
                        .w(px(160.0))
                        .h(px(height))
                        .bg(nsh_core::color::rgb(0x123456)),
                )
        }))
        .child(view().absolute().top(px(192.0)).child(text("ready")))
}

#[test]
fn a_subcell_separator_keeps_disjoint_text_that_quantizes_to_its_row() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(30, 16)).unwrap();
    view.load(App::Rust(Root::new::<QuantizedSeparators>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(2, 4)].symbol(), "L");
    assert_eq!(buffer[(10, 4)].fg, Color::Blue);
    assert_eq!(buffer[(10, 4)].symbol(), "─");
    assert!(!reversed(buffer, 10, 4));
    assert_eq!(buffer[(2, 8)].symbol(), " ");
    assert_eq!(solid(buffer, 2, 8), Some(Color::Blue));
}

#[component]
fn AdjacentLabels() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(16.0))
                .flex_row()
                .gap(px(2.0))
                .child(text("All"))
                .child(text("22"))
                .child(text("Up")),
        )
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(48.0))
                .flex_row_reverse()
                .gap(px(2.0))
                .child(text("All"))
                .child(text("22"))
                .child(text("Up")),
        )
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(80.0))
                .flex_row()
                .child(text("All"))
                .child(text("22")),
        )
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(128.0))
                .w(px(64.0))
                .h(px(32.0))
                .border_1()
                .flex_row()
                .items_center()
                .gap(px(2.0))
                .child(text("All"))
                .child(text("22"))
                .child(text("Up")),
        )
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(176.0))
                .w(px(128.0))
                .h(px(32.0))
                .border_1()
                .items_center()
                .justify_center()
                .child(
                    view()
                        .absolute()
                        .left(px(24.0))
                        .top(px(3.0))
                        .w(px(80.0))
                        .h(px(26.0))
                        .border_1()
                        .rounded(px(8.0))
                        .bg(nsh_core::color::rgb(0x123456)),
                )
                .child(
                    view()
                        .flex_row()
                        .gap(px(2.0))
                        .child(text("All"))
                        .child(text("22")),
                ),
        )
        .child(view().absolute().top(px(112.0)).child(text("ready")))
}

#[test]
fn positive_gaps_between_sibling_labels_keep_a_cell_in_either_row_direction() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 14)).unwrap();
    view.load(App::Rust(Root::new::<AdjacentLabels>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let row = |y| (0..40).map(|x| buffer[(x, y)].symbol()).collect::<String>();
    assert!(row(1).contains("All 22 Up"), "{}", row(1));
    assert!(row(3).contains("Up 22 All"), "{}", row(3));
    assert!(row(5).contains("All22"), "{}", row(5));
    assert!(row(9).contains("All 22"), "{}", row(9));
    assert!(row(12).contains("All 22"), "{}", row(12));
}

#[component]
fn AdjacentCanvasLabels() -> impl IntoElement {
    view()
        .flex_1()
        .child(
            canvas(
                Painter::new()
                    .fill_color(nsh_core::color::rgb(0xffffff))
                    .text_baseline(nsh_core::canvas::CanvasTextBaseline::Top)
                    .fill_text("09:09", 0.0, 0.0)
                    .fill_text("10:15", 42.0, 0.0)
                    .fill_text("10:15", 42.0, 32.0)
                    .fill_text("09:09", 0.0, 32.0)
                    .fill_text("09:09", 0.0, 64.0)
                    .fill_text("10:15", 40.0, 64.0),
            )
            .label("drawing")
            .absolute()
            .left(px(16.0))
            .top(px(16.0))
            .w(px(200.0))
            .h(px(96.0)),
        )
        .child(view().absolute().top(px(128.0)).child(text("ready")))
}

#[test]
fn positive_gaps_between_canvas_labels_keep_a_cell_without_spacing_graphemes() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(App::Rust(Root::new::<AdjacentCanvasLabels>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let row = |y| (0..40).map(|x| buffer[(x, y)].symbol()).collect::<String>();
    assert!(row(1).contains("09:09 10:15"), "{}", row(1));
    assert!(row(3).contains("09:09 10:15"), "{}", row(3));
    assert!(row(5).contains("09:0910:15"), "{}", row(5));
}

#[component]
fn HairlineFills() -> impl IntoElement {
    let color = nsh_core::color::rgb(0x123456);
    view()
        .flex_1()
        .bg(nsh_core::color::rgb(0x101113))
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(16.0))
                .w(px(80.0))
                .h(px(1.0))
                .bg(color),
        )
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(48.0))
                .w(px(3.0))
                .h(px(32.0))
                .rounded(px(1.5))
                .bg(color),
        )
        .child(
            view()
                .absolute()
                .left(px(16.0))
                .top(px(96.0))
                .w(px(80.0))
                .h(px(48.0))
                .border_t(px(1.0))
                .border_color(color),
        )
        .child(
            view()
                .absolute()
                .left(px(128.0))
                .top(px(16.0))
                .w(px(80.0))
                .h(px(8.0))
                .bg(color),
        )
        .child(
            view()
                .absolute()
                .left(px(128.0))
                .top(px(48.0))
                .w(px(80.0))
                .h(px(1.0))
                .bg(color)
                .child(text("child")),
        )
        .child(
            view()
                .absolute()
                .left(px(128.0))
                .top(px(80.0))
                .w(px(80.0))
                .h(px(1.0))
                .bg(color)
                .opacity(0.5),
        )
        .child(
            view()
                .absolute()
                .left(px(128.0))
                .top(px(112.0))
                .w(px(80.0))
                .h(px(1.0))
                .bg(nsh_core::color::rgb(0xff0000))
                .bg_linear_to_r()
                .gradient_from(nsh_core::color::rgba(0x0000ff80))
                .gradient_to(nsh_core::color::rgba(0x0000ff80)),
        )
        .child(
            view()
                .absolute()
                .left(px(224.0))
                .top(px(112.0))
                .w(px(80.0))
                .h(px(1.0))
                .bg_linear_to_r()
                .gradient_from(nsh_core::color::rgb(0x00ff00))
                .gradient_to(nsh_core::color::rgb(0x00ff00)),
        )
        .child(view().absolute().top(px(160.0)).child(text("ready")))
}

#[test]
fn childless_hairline_fills_draw_rules_in_their_palette_color() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(App::Rust(Root::new::<HairlineFills>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    for column in 2..12 {
        assert_eq!(buffer[(column, 1)].symbol(), "─");
        assert_eq!(buffer[(column, 1)].fg, Color::Blue);
        assert_eq!(buffer[(column, 1)].bg, Color::Reset);
        assert_eq!(buffer[(column, 1)].symbol(), buffer[(column, 6)].symbol());
        assert_eq!(buffer[(column, 1)].fg, buffer[(column, 6)].fg);
    }
    for row in 3..5 {
        assert_eq!(buffer[(2, row)].symbol(), "│");
        assert_eq!(buffer[(2, row)].fg, Color::Blue);
    }
    assert_eq!(buffer[(16, 1)].symbol(), " ");
    assert_eq!(
        solid(buffer, 16, 1),
        Some(Color::Blue),
        "half a Cell is a solid leaf"
    );
    assert_eq!(buffer[(16, 3)].symbol(), "c");
    assert_eq!(solid(buffer, 16, 3), None, "a container draws no fill");
    assert_eq!(buffer[(16, 5)].symbol(), "─");
    assert_eq!(buffer[(16, 5)].fg, Color::Blue, "opacity keeps the color");
    assert_eq!(buffer[(16, 7)].symbol(), "─");
    assert_eq!(buffer[(16, 7)].fg, Color::Red, "gradients are left out");
    assert_eq!(buffer[(28, 7)].symbol(), " ");
}

#[component]
fn SidebarRows() -> impl IntoElement {
    view()
        .flex_1()
        .child(text("APPS").h(px(20.0)))
        .child(view().gap(px(2.0)).children(
            ["Stocks", "AI Chat", "Photos", "Lists", "Scrolling"].map(|name| {
                view()
                    .h(px(36.0))
                    .flex_row()
                    .items_center()
                    .px(px(10.0))
                    .on_press(|_: &()| {})
                    .child(text(name))
            }),
        ))
        .child(text("ready").absolute().top(px(240.0)))
}

#[test]
fn equal_rows_stacked_in_a_column_keep_one_cell_pitch() {
    use nsh_platform_tui::{Handled, MouseButton, MouseKind};

    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(20, 16)).unwrap();
    view.load(App::Rust(Root::new::<SidebarRows>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    let buffer = terminal.backend().buffer();
    let rows: Vec<u16> = ["S", "A", "P", "L", "S"]
        .iter()
        .scan(0, |from, first| {
            let row = (*from..16).find(|&y| buffer[(1, y)].symbol() == *first)?;
            *from = row + 1;
            Some(row)
        })
        .collect();
    // 36-point rows 2 points apart start 20 points down: absolute edges alone
    // would put them 2, 3, 2 and 3 rows apart.
    assert_eq!(rows.len(), 5, "{rows:?}");
    assert!(
        rows.windows(2).all(|pair| pair[1] - pair[0] == 2),
        "{rows:?}"
    );
    for kind in [
        MouseKind::Down(MouseButton::Left),
        MouseKind::Up(MouseButton::Left),
    ] {
        assert_eq!(
            view.handle(super::app::mouse(kind, 1, rows[4])),
            Handled::Yes,
            "the last row takes presses where it is drawn"
        );
    }
}
