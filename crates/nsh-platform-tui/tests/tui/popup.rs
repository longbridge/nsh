//! Anchored popups in a terminal: a dropdown, a menu and a date picker's
//! calendar go on the row after their trigger's (or the rows above it
//! when there is no room below), never over it, as tall as their rows; a
//! dialog's popup with `auto` side margins is centered, its top margin
//! kept; a command's search field has a rule under it.

use nsh_base::{CommandItem, SelectItem, combobox, command, date_picker, menu, menu_item};
use nsh_core::{
    geometry::{auto, relative},
    prelude::*,
};
use nsh_platform_tui::{App, Root};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

use super::{
    app::{draw_until, surface},
    dock::{dump, find},
};

fn tags() -> Vec<SelectItem> {
    vec![
        SelectItem::new("rust", "Rust"),
        SelectItem::new("go", "Go"),
        SelectItem::new("ruby", "Ruby"),
        SelectItem::new("swift", "Swift"),
    ]
}

/// An open combobox of four tags with a footer `Done`, its field `Pick`.
fn tag_box() -> impl IntoElement {
    combobox()
        .items(tags())
        .placeholder("Pick")
        .accessibility_label("Tags")
        .default_open(true)
        .footer(text("Done"))
        .w(px(160.0))
}

/// The combobox on row 1, the screen's rows below it.
#[component]
fn DropdownBelow() -> impl IntoElement {
    view().flex_1().child(text("above")).child(tag_box())
}

/// The combobox on the last row: the room is above it.
#[component]
fn DropdownAbove() -> impl IntoElement {
    view()
        .flex_1()
        .child(text("above"))
        .child(view().flex_1())
        .child(tag_box())
}

/// The rows of `words`, in order: where each first shows.
fn rows_of(terminal: &Terminal<TestBackend>, words: &[&str]) -> Vec<u16> {
    words
        .iter()
        .map(|w| {
            find(terminal, w)
                .unwrap_or_else(|| panic!("{w:?} shows\n{}", dump(terminal)))
                .1
        })
        .collect()
}

/// `rows` follow one another, one row each.
fn consecutive(terminal: &Terminal<TestBackend>, rows: &[u16]) {
    for pair in rows.windows(2) {
        assert_eq!(
            pair[1],
            pair[0] + 1,
            "one row each, no rows between: {rows:?}\n{}",
            dump(terminal)
        );
    }
}

#[test]
fn a_dropdown_goes_on_the_rows_below_its_trigger_as_tall_as_its_rows() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    view.load(App::Rust(Root::new::<DropdownBelow>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Swift");
    let (_, trigger) = find(&terminal, "Pick")
        .unwrap_or_else(|| panic!("the trigger shows\n{}", dump(&terminal)));
    let rows = rows_of(&terminal, &["Rust", "Go", "Ruby", "Swift", "Done"]);
    assert!(
        rows[0] > trigger && rows[0] <= trigger + 2,
        "the first row right below the trigger's (row {trigger}): {rows:?}\n{}",
        dump(&terminal)
    );
    consecutive(&terminal, &rows);
}

#[test]
fn a_dropdown_without_room_below_goes_above_its_trigger() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 16)).unwrap();
    view.load(App::Rust(Root::new::<DropdownAbove>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Swift");
    let (_, trigger) = find(&terminal, "Pick")
        .unwrap_or_else(|| panic!("the trigger shows\n{}", dump(&terminal)));
    let rows = rows_of(&terminal, &["Rust", "Go", "Ruby", "Swift", "Done"]);
    let last = *rows.last().unwrap();
    assert!(
        last < trigger && last + 2 >= trigger,
        "the last row right above the trigger's (row {trigger}): {rows:?}\n{}",
        dump(&terminal)
    );
    consecutive(&terminal, &rows);
}

/// An open menu of two items under a trigger `View` on row 1.
#[component]
fn OpenMenu() -> impl IntoElement {
    view().flex_1().child(text("above")).child(
        menu()
            .default_open(true)
            .trigger(view().child(text("View")))
            .child(
                menu_item()
                    .accessibility_label("About")
                    .child(text("About")),
            )
            .child(menu_item().accessibility_label("Quit").child(text("Quit"))),
    )
}

#[test]
fn a_menu_goes_on_the_rows_below_its_trigger() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    view.load(App::Rust(Root::new::<OpenMenu>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Quit");
    let rows = rows_of(&terminal, &["View", "About", "Quit"]);
    assert!(
        rows[1] > rows[0] && rows[1] <= rows[0] + 2,
        "the first item right below the trigger's row: {rows:?}\n{}",
        dump(&terminal)
    );
    consecutive(&terminal, &rows[1..]);
}

/// An open date picker under a trigger `Pick` on row 1.
#[component]
fn OpenPicker() -> impl IntoElement {
    view().flex_1().child(text("above")).child(
        date_picker()
            .accessibility_label("Due")
            .default_open(true)
            .trigger(view().w(px(160.0)).child(text("Pick"))),
    )
}

#[test]
fn a_date_picker_s_calendar_shows_its_days_below_its_trigger() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 24)).unwrap();
    view.load(App::Rust(Root::new::<OpenPicker>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "28");
    let rows = rows_of(&terminal, &["Pick", "‹", "28"]);
    assert!(
        rows[1] > rows[0],
        "the calendar's header below the trigger's row: {rows:?}\n{}",
        dump(&terminal)
    );
    assert!(
        rows[2] > rows[1],
        "its days below its header: {rows:?}\n{}",
        dump(&terminal)
    );
}

/// A dialog whose popup is 30 Cells wide, centered by `auto` side
/// margins, 12% of the width below the top.
#[component]
fn CenteredDialog() -> impl IntoElement {
    view().flex_1().child(text("page")).child(
        nsh_base::dialog().child(
            view()
                .mx(auto())
                .mt(relative(0.12))
                .w(px(240.0))
                .child(text("Palette")),
        ),
    )
}

#[test]
fn a_dialog_s_popup_with_auto_margins_is_centered_below_its_top_margin() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(80, 40)).unwrap();
    view.load(App::Rust(Root::new::<CenteredDialog>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Palette");
    let (x, y) = find(&terminal, "Palette").unwrap();
    // 25 Cells each side of 30 in 80 (the popup's border or padding
    // before its text at most).
    assert!(
        (25..=28).contains(&x),
        "centered: x {x}\n{}",
        dump(&terminal)
    );
    // 12% of 640 points is 76.8: about 5 rows, not the one row a margin
    // between two boxes of a column takes.
    assert!((4..=7).contains(&y), "below its margin: y {y}\n{}", dump(&terminal));
}

/// A command over two items with its search field `Find`.
#[component]
fn InlineCommand() -> impl IntoElement {
    view().flex_1().child(
        command()
            .items(vec![
                CommandItem::new("a", "Alpha"),
                CommandItem::new("b", "Beta"),
            ])
            .placeholder("Find")
            .accessibility_label("Commands"),
    )
}

#[test]
fn a_command_s_search_field_has_a_rule_under_it() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(App::Rust(Root::new::<InlineCommand>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Beta");
    let (x, field) = find(&terminal, "Find").unwrap();
    let rows = rows_of(&terminal, &["Alpha", "Beta"]);
    assert_eq!(
        terminal.backend().buffer()[(x, field + 1)].symbol(),
        "─",
        "a rule under the field\n{}",
        dump(&terminal)
    );
    assert_eq!(rows[0], field + 2, "the rows under the rule\n{}", dump(&terminal));
}

/// A dropdown's frame without a component: an overlay under `Pick` holding
/// a scroll view over a list of four rows virtualized against it, and a
/// footer; the heights each reports, in points.
#[component]
fn BarePopup() -> impl IntoElement {
    let anchor = element_ref();
    let heights = signal([0.0f32; 3]);
    let report = move |at: usize| {
        move |frame: &Bounds<Points>| heights.update(|h| h[at] = frame.size.height.0)
    };
    view()
        .flex_1()
        .child(text(move || {
            let [o, s, l] = heights.get();
            format!("heights {o} {s} {l}")
        }))
        .child(view().w(px(160.0)).element_ref(&anchor).child(text("Pick")))
        .child(
            overlay()
                .anchor(&anchor)
                .placement(nsh_core::overlay::Placement::Bottom)
                .offset(px(4.0))
                .w(px(160.0))
                .max_h(px(400.0))
                .on_layout(report(0))
                .child(
                    scroll()
                        .flex_shrink(1.0)
                        .min_h(px(0.0))
                        .on_layout(report(1))
                        .child(
                            virtual_list(4usize, |at| text(format!("row {}", at.index())))
                                .viewport(ListViewport::Parent)
                                .on_layout(report(2)),
                        ),
                )
                .child(text("Done")),
        )
}

#[test]
fn a_popup_s_virtualized_list_is_as_tall_as_its_rows() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    view.load(App::Rust(Root::new::<BarePopup>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "row 3");
    let rows = rows_of(&terminal, &["Pick", "row 0", "row 1", "row 2", "row 3", "Done"]);
    consecutive(&terminal, &rows);
}
