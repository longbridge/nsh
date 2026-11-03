//! Resizable panes in a terminal: the handle between two panes is the
//! line between them, one Cell across, dragged and stepped a Cell at a
//! time. A dock area's tab dragged over a group outlines where it would
//! land, over what is there.

use nsh_base::{
    dock::{DockAreaLayout, DockLayout, DockNode, DockPlacement, DockRegion, DockTab, dock_area},
    h_resizable, resizable_panel,
};
use nsh_core::prelude::*;
use nsh_platform_tui::{App, Handled, Input, Modifiers, MouseButton, MouseKind, Root};
use ratatui_core::{
    backend::TestBackend,
    style::{Color, Modifier},
    terminal::Terminal,
};

use super::app::{draw_until, mouse, surface};

/// The row of the panes (the status is on row 0).
const ROW: u16 = 2;

/// Two panes side by side: a sidebar of 80 points (10 Cells), the rest
/// for the editor; what the group reports when a drag ends.
#[component]
fn Panes() -> impl IntoElement {
    let reported = signal(String::from("none"));
    view()
        .flex_1()
        .child(text(move || format!("sizes {}", reported.get())))
        .child(
            view().flex_1().child(
                h_resizable()
                    .on_resize(move |sizes: &Vec<Points>| {
                        let sizes: Vec<String> = sizes
                            .iter()
                            .map(|s| (s.0.round() as i32).to_string())
                            .collect();
                        reported.set(sizes.join(","));
                    })
                    .child(
                        resizable_panel()
                            .size(px(80.0))
                            .min_size(px(40.0))
                            .child(text("left\n1\n2\n3")),
                    )
                    .child(resizable_panel().min_size(px(40.0)).child(text("right"))),
            ),
        )
}

fn open() -> (
    nsh_platform_tui::AppView,
    Terminal<TestBackend>,
    crossbeam_channel::Receiver<()>,
) {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(App::Rust(Root::new::<Panes>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "sizes none");
    draw_until(&mut view, &mut terminal, &rx, "│");
    (view, terminal, rx)
}

/// The column of the handle's line on row [`ROW`].
fn handle_column(terminal: &Terminal<TestBackend>) -> u16 {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.width)
        .find(|&x| buffer[(x, ROW)].symbol() == "│")
        .expect("the handle's line is drawn")
}

/// The column where `word` starts on row 1 (the panes' first row).
fn column_of(terminal: &Terminal<TestBackend>, word: &str) -> u16 {
    let buffer = terminal.backend().buffer();
    let row: String = (0..buffer.area.width)
        .map(|x| buffer[(x, 1)].symbol())
        .collect();
    // Cells, not bytes: the handle's `│` is three bytes.
    let at = row.find(word).expect("the word is drawn");
    row[..at].chars().count() as u16
}

/// Draws until the handle's line is at column `x`.
fn draw_until_handle_at(
    view: &mut nsh_platform_tui::AppView,
    terminal: &mut Terminal<TestBackend>,
    rx: &crossbeam_channel::Receiver<()>,
    x: u16,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if terminal.backend().buffer()[(x, ROW)].symbol() == "│" {
            return;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "the handle did not move to column {x}"
        );
    }
}

#[test]
fn the_handle_is_the_line_in_the_cell_between_the_panes() {
    let (_view, terminal, _rx) = open();
    // The sidebar is its 10 Cells, the line the Cell after it, the editor
    // the rest.
    assert_eq!(handle_column(&terminal), 10);
    assert_eq!(column_of(&terminal, "left"), 0);
    assert_eq!(column_of(&terminal, "right"), 11);
    let buffer = terminal.backend().buffer();
    for y in 1..8 {
        assert_eq!(
            buffer[(10, y)].symbol(),
            "│",
            "the line runs down the panes at row {y}"
        );
    }
}

#[test]
fn dragging_the_handle_five_cells_resizes_the_panes_by_five_cells() {
    let (mut view, mut terminal, rx) = open();
    assert_eq!(
        view.handle(mouse(MouseKind::Down(MouseButton::Left), 10, ROW)),
        Handled::Yes
    );
    // The panes follow the pointer Cell by Cell while it moves.
    for x in 11..=15 {
        assert_eq!(view.handle(mouse(MouseKind::Move, x, ROW)), Handled::Yes);
        draw_until_handle_at(&mut view, &mut terminal, &rx, x);
    }
    assert_eq!(
        view.handle(mouse(MouseKind::Up(MouseButton::Left), 15, ROW)),
        Handled::Yes
    );
    // 80 + 5 Cells of 8 points; the editor gave them up.
    draw_until(&mut view, &mut terminal, &rx, "sizes 120,192");
    assert_eq!(handle_column(&terminal), 15);
    assert_eq!(column_of(&terminal, "right"), 16);
}

#[test]
fn arrows_step_a_focused_handle_one_cell() {
    let (mut view, mut terminal, rx) = open();
    // The handle is the only stop of the keyboard focus.
    let key = |name: &str| Input::Key {
        key: name.into(),
        modifiers: Modifiers::default(),
        release: false,
    };
    assert_eq!(view.handle(key("Tab")), Handled::Yes);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    // The focus lights the line (the accent) and binds the arrows.
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if terminal.backend().buffer()[(10, ROW)].fg != Color::Reset
            && !terminal.backend().buffer()[(10, ROW)]
                .modifier
                .contains(Modifier::DIM)
        {
            break;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "the handle did not take the focus"
        );
    }
    assert_eq!(view.handle(key("Right")), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "sizes 88,224");
    draw_until_handle_at(&mut view, &mut terminal, &rx, 11);
    assert_eq!(view.handle(key("Left")), Handled::Yes);
    assert_eq!(view.handle(key("Left")), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "sizes 72,240");
    draw_until_handle_at(&mut view, &mut terminal, &rx, 9);
}

#[test]
fn hovering_the_handle_lights_its_line() {
    let (mut view, mut terminal, rx) = open();
    let idle = terminal.backend().buffer()[(10, ROW)].clone();
    view.handle(mouse(MouseKind::Move, 10, ROW));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        let cell = &terminal.backend().buffer()[(10, ROW)];
        if cell.fg != idle.fg && !cell.modifier.contains(Modifier::DIM) {
            break;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "hover did not light the line"
        );
    }
    // Off the handle, it is muted again.
    view.handle(mouse(MouseKind::Move, 30, ROW));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        let cell = &terminal.backend().buffer()[(10, ROW)];
        if cell.fg == idle.fg && cell.modifier == idle.modifier {
            break;
        }
        assert!(rx.recv_deadline(deadline).is_ok(), "the line stayed lit");
    }
}

/// A dock layout as text: `h(..)` / `v(..)` of its children, a tab group
/// `[a *b]` with its displayed tab starred.
fn node_text(node: &DockNode) -> String {
    match node {
        DockNode::Split { axis, children, .. } => {
            let children: Vec<String> = children.iter().map(node_text).collect();
            let axis = if *axis == nsh_core::geometry::Axis::Horizontal {
                "h"
            } else {
                "v"
            };
            format!("{axis}({})", children.join(" "))
        }
        DockNode::Tabs {
            panels,
            active_index,
            ..
        } => {
            let tabs: Vec<String> = panels
                .iter()
                .enumerate()
                .map(|(ix, p)| match ix == *active_index {
                    true => format!("*{p}"),
                    false => p.to_string(),
                })
                .collect();
            format!("[{}]", tabs.join(" "))
        }
    }
}

/// Two groups side by side, `a b` 240 points (30 Cells) wide and `c`; the
/// layout the area reports, shown.
#[component]
fn Docked() -> impl IntoElement {
    let layout = signal(DockAreaLayout::new(
        DockLayout::h_split()
            .child(DockLayout::tabs().panel("a").panel("b"), Some(px(240.0)))
            .child(DockLayout::tabs().panel("c"), None),
    ));
    view()
        .flex_1()
        .child(text(move || {
            format!("Layout {}", node_text(&layout.get().center))
        }))
        .child(
            view().flex_1().child(
                dock_area()
                    .layout(move || layout.get())
                    .compact(Some(false))
                    .tab_bar_class("flex-row gap-2")
                    .render_tab(|tab: &DockTab| text(format!("Tab {}", tab.panel)))
                    // Shows that a drag started (in the dump of a failure).
                    .render_drag_preview(|tab: &DockTab| text(format!("drag {}", tab.panel)))
                    .render_panel(|panel: &nsh_base::dock::DockPanel| {
                        view()
                            .flex_1()
                            .child(text(format!("Panel {}\nmore\nmore", panel.id)))
                    })
                    .on_layout_change(move |next: &DockAreaLayout| layout.set(next.clone()))
                    .size_full(),
            ),
        )
}

/// The buffer's rows, each with its number, and the Cells drawn in a color.
fn dump(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        let colored: Vec<String> = (0..buffer.area.width)
            .filter(|&x| buffer[(x, y)].fg != Color::Reset && buffer[(x, y)].symbol() != " ")
            .map(|x| format!("{x}{}", buffer[(x, y)].symbol()))
            .collect();
        out += &format!("{y:>2}|{row}| {}\n", colored.join(" "));
    }
    out
}

/// Where `word` starts, in Cells.
fn find(terminal: &Terminal<TestBackend>, word: &str) -> Option<(u16, u16)> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height).find_map(|y| {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        row.find(word)
            .map(|at| (row[..at].chars().count() as u16, y))
    })
}

/// The Cells of a frame's top right corner in the accent's place: what
/// only the drop preview draws in this layout.
fn outline_corner(terminal: &Terminal<TestBackend>) -> Option<(u16, u16)> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height).find_map(|y| {
        (0..buffer.area.width)
            .find(|&x| matches!(buffer[(x, y)].symbol(), "╮" | "┐"))
            .map(|x| (x, y))
    })
}

#[test]
fn a_tab_dragged_to_a_group_s_right_edge_outlines_the_half_and_splits_there() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    view.load(App::Rust(Root::new::<Docked>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Panel c");
    assert!(
        outline_corner(&terminal).is_none(),
        "no preview before a drag"
    );
    let (tab_x, tab_y) = find(&terminal, "Tab a").expect("tab a is drawn");
    let (panel_x, panel_y) = find(&terminal, "Panel c").expect("c's panel is drawn");
    // A terminal's pointer is a mouse, so the drag arms at once; it moves
    // to the right edge of c's content.
    view.handle(mouse(MouseKind::Down(MouseButton::Left), tab_x + 1, tab_y));
    let (to_x, to_y) = (78, panel_y + 1);
    for x in (tab_x + 2..=to_x).step_by(4).chain([to_x]) {
        view.handle(mouse(MouseKind::Move, x, to_y));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let corner = loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if let Some(corner) = outline_corner(&terminal) {
            break corner;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "the drop preview's outline did not show (tab a at {:?}, Panel c at {:?}, pointer at {:?}):\n{}",
            (tab_x, tab_y),
            (panel_x, panel_y),
            (to_x, to_y),
            dump(&terminal)
        );
    };
    // The right half of c's content, its corner at the area's right edge.
    assert!(
        corner.0 > panel_x,
        "the outline is over c's right half: {corner:?}"
    );
    let buffer = terminal.backend().buffer();
    assert_ne!(
        buffer[corner].fg,
        Color::Reset,
        "the outline is in the accent"
    );
    // What the group shows stays under the outline: nothing is cleared.
    assert_eq!(find(&terminal, "Panel c"), Some((panel_x, panel_y)));
    view.handle(mouse(MouseKind::Up(MouseButton::Left), to_x, to_y));
    draw_until(&mut view, &mut terminal, &rx, "Layout h([*b] [*c] [*a])");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while outline_corner(&terminal).is_some() {
        assert!(rx.recv_deadline(deadline).is_ok(), "the preview stayed");
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
    }
}

/// A center `c` over a bottom dock `t` of 160 points (10 rows) in a
/// 24-row terminal.
#[component]
fn BottomDocked() -> impl IntoElement {
    let mut bottom = DockRegion::new(DockLayout::tabs().panel("t"));
    bottom.size = Some(px(160.0));
    let layout = signal(
        DockAreaLayout::new(DockLayout::tabs().panel("c")).with_dock(DockPlacement::Bottom, bottom),
    );
    view().flex_1().child(
        dock_area()
            .layout(move || layout.get())
            .compact(Some(false))
            .tab_bar_class("flex-row gap-2")
            .render_tab(|tab: &DockTab| text(format!("Tab {}", tab.panel)))
            .render_panel(|panel: &nsh_base::dock::DockPanel| {
                view()
                    .flex_1()
                    .child(text(format!("Panel {}\nmore", panel.id)))
            })
            .on_layout_change(move |next: &DockAreaLayout| layout.set(next.clone()))
            .size_full(),
    )
}

#[test]
fn a_bottom_dock_keeps_its_size_in_rows() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    view.load(App::Rust(Root::new::<BottomDocked>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Panel t");
    let (_, center) = find(&terminal, "Panel c").expect("the center's panel is drawn");
    let (_, tab) = find(&terminal, "Tab t").expect("the dock's tab is drawn");
    let (_, panel) = find(&terminal, "Panel t").expect("the dock's panel is drawn");
    // 160 points are 10 rows at the bottom of the 24: the dock starts on
    // row 14 with its handle's line, its tab below it, then its panel.
    assert_eq!(
        terminal.backend().buffer()[(5, 14)].symbol(),
        "─",
        "{}",
        dump(&terminal)
    );
    assert_eq!(tab, 15, "the dock's tab row\n{}", dump(&terminal));
    assert_eq!(panel, 16, "the dock's panel row\n{}", dump(&terminal));
    // The center takes the rest above it.
    assert!(center < tab, "{}", dump(&terminal));
    assert!(find(&terminal, "more").is_some_and(|(_, y)| y == center + 1));
}
