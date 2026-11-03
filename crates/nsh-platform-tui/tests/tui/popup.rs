//! Anchored popups in a terminal: a dropdown, a menu and a date picker's
//! calendar go on the row after their trigger's (or the rows above it
//! when there is no room below), never over it, as tall as their rows; a
//! dialog's popup with `auto` side margins is centered, its top margin
//! kept; a command's search field has a rule under it, and so has a
//! searchable select's. A popup the terminal frames outside its box is
//! placed with that frame: beside its trigger, never on its row, lined up
//! with its start or end. A sheet is a solid panel the height of the
//! screen over a cleared page; a toast at the bottom shows its whole frame.

use nsh_base::{
    Anchor, CommandItem, SelectItem, combobox, command, date_picker, menu, menu_item, select,
    sheet, toast, toast_stack,
};
use nsh_core::{
    geometry::{auto, relative},
    overlay::{Align, Placement},
    prelude::*,
};
use nsh_platform_tui::{App, MouseKind, Root};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

use super::{
    app::{draw_until, draw_until_still, mouse, surface},
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
    let (_, trigger) =
        find(&terminal, "Pick").unwrap_or_else(|| panic!("the trigger shows\n{}", dump(&terminal)));
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
    let (_, trigger) =
        find(&terminal, "Pick").unwrap_or_else(|| panic!("the trigger shows\n{}", dump(&terminal)));
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

/// The open menu with its first item filled (a highlighted row's look,
/// the app's): the item is not the popup's box.
#[component]
fn FilledFirstItem() -> impl IntoElement {
    view().flex_1().child(text("above")).child(
        menu()
            .default_open(true)
            .trigger(view().child(text("View")))
            .child(
                menu_item()
                    .accessibility_label("About")
                    .bg(nsh_core::color::rgb(0x2563eb))
                    .child(text("About")),
            )
            .child(menu_item().accessibility_label("Quit").child(text("Quit"))),
    )
}

#[test]
fn a_menu_s_filled_first_item_leaves_the_frame_around_the_whole_menu() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    view.load(App::Rust(Root::new::<FilledFirstItem>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Quit");
    let rows = rows_of(&terminal, &["View", "About", "Quit"]);
    let (_, top) =
        find(&terminal, "╭").unwrap_or_else(|| panic!("the menu is framed\n{}", dump(&terminal)));
    let (_, bottom) = find(&terminal, "╰").unwrap();
    assert!(
        top > rows[0] && top < rows[1] && bottom > rows[2],
        "one frame around both items, below the trigger's row: {rows:?}, \
         frame rows {top}..={bottom}\n{}",
        dump(&terminal)
    );
    consecutive(&terminal, &rows[1..]);
}

/// An open menu of a group of choices, its items rows with a gap of the
/// app's: the first checked.
#[component]
fn CheckedMenu() -> impl IntoElement {
    let item = |label: &'static str, on: bool| {
        menu_item()
            .checked(Some(on))
            .accessibility_label(label)
            .flex_row()
            .gap(px(8.0))
            .child(text(label))
    };
    view().flex_1().child(text("above")).child(
        menu()
            .default_open(true)
            .trigger(view().child(text("Block")))
            .child(item("Paragraph", true))
            .child(item("Heading", false)),
    )
}

#[test]
fn a_menu_s_checked_and_unchecked_labels_line_up() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    view.load(App::Rust(Root::new::<CheckedMenu>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Heading");
    let (check, _) = find(&terminal, "✓ Paragraph")
        .unwrap_or_else(|| panic!("the check, a space, the label\n{}", dump(&terminal)));
    let (heading, _) = find(&terminal, "Heading").unwrap();
    assert_eq!(
        heading,
        check + 2,
        "the labels in one column, the gap after the mark's space\n{}",
        dump(&terminal)
    );
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
    view.load(App::Rust(Root::new::<OpenPicker>(|| ())))
        .unwrap();
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
    assert!(
        (4..=7).contains(&y),
        "below its margin: y {y}\n{}",
        dump(&terminal)
    );
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
    assert_eq!(
        rows[0],
        field + 2,
        "the rows under the rule\n{}",
        dump(&terminal)
    );
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
    let rows = rows_of(
        &terminal,
        &["Pick", "row 0", "row 1", "row 2", "row 3", "Done"],
    );
    consecutive(&terminal, &rows);
}

/// A trigger `Pick` 20 Cells wide after a margin (two Cells: a margin is
/// spacing in a terminal, [`framed`] reports where it starts), on row 1 or
/// (`low`) the last row, and an anchored overlay with a fill of its own (no border or
/// padding: the terminal frames it outside its box) 10 Cells wide,
/// aligned with the trigger's `align` edge.
fn framed_popup(align: Align, low: bool) -> impl IntoElement {
    let anchor = element_ref();
    view()
        .flex_1()
        .child(text("above"))
        .child(view().when(low, |v| v.flex_1()))
        .child(
            view()
                .ml(px(80.0))
                .w(px(160.0))
                .element_ref(&anchor)
                .child(text("Pick")),
        )
        .child(
            overlay()
                .anchor(&anchor)
                .placement(Placement::Bottom)
                .align(align)
                .w(px(80.0))
                .bg(nsh_core::color::rgb(0x333333))
                .child(text("Item")),
        )
}

#[component]
fn FramedStart() -> impl IntoElement {
    framed_popup(Align::Start, false)
}

#[component]
fn FramedEnd() -> impl IntoElement {
    framed_popup(Align::End, false)
}

#[component]
fn FramedLow() -> impl IntoElement {
    framed_popup(Align::Start, true)
}

/// Draws `root` on a 40 × 16 terminal until `Item` shows: the trigger's
/// Cell, and the terminal.
fn framed(root: Root) -> ((u16, u16), Terminal<TestBackend>) {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 16)).unwrap();
    view.load(App::Rust(root)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Item");
    let trigger = find(&terminal, "Pick")
        .unwrap_or_else(|| panic!("the trigger's row shows\n{}", dump(&terminal)));
    (trigger, terminal)
}

#[test]
fn a_framed_popup_s_frame_goes_below_its_trigger_s_row_at_its_start() {
    let ((x, y), terminal) = framed(Root::new::<FramedStart>(|| ()));
    assert_eq!(x, 2, "the trigger after its margin\n{}", dump(&terminal));
    let corner = find(&terminal, "╭").unwrap_or_else(|| panic!("a frame\n{}", dump(&terminal)));
    assert_eq!(
        corner,
        (x, y + 1),
        "the frame's corner on the row after the trigger's, at its start\n{}",
        dump(&terminal)
    );
    let (_, item) = find(&terminal, "Item").unwrap();
    assert_eq!(
        item,
        y + 2,
        "the content inside the frame\n{}",
        dump(&terminal)
    );
}

#[test]
fn a_framed_popup_aligned_with_its_trigger_s_end_ends_its_frame_there() {
    let ((x, y), terminal) = framed(Root::new::<FramedEnd>(|| ()));
    let corner = find(&terminal, "╮").unwrap_or_else(|| panic!("a frame\n{}", dump(&terminal)));
    assert_eq!(
        corner,
        (x + 19, y + 1),
        "the frame's corner in the trigger's last column\n{}",
        dump(&terminal)
    );
}

#[test]
fn a_framed_popup_without_room_below_ends_its_frame_above_its_trigger_s_row() {
    let ((x, y), terminal) = framed(Root::new::<FramedLow>(|| ()));
    let corner = find(&terminal, "╰").unwrap_or_else(|| panic!("a frame\n{}", dump(&terminal)));
    assert_eq!(
        corner,
        (x, y - 1),
        "the frame's bottom on the row before the trigger's\n{}",
        dump(&terminal)
    );
}

/// A trigger `Pick` 20 Cells wide after a margin on row 3 and, beside it
/// at its right aligned with its start, an overlay with a fill of its own
/// (a submenu beside its item) of two rows, `Item` and `Next`.
#[component]
fn FramedBeside() -> impl IntoElement {
    let anchor = element_ref();
    view()
        .flex_1()
        .child(text("above"))
        .child(text("more"))
        .child(text("again"))
        .child(
            view()
                .ml(px(80.0))
                .w(px(160.0))
                .element_ref(&anchor)
                .child(text("Pick")),
        )
        .child(
            overlay()
                .anchor(&anchor)
                .placement(Placement::Right)
                .align(Align::Start)
                .w(px(64.0))
                .bg(nsh_core::color::rgb(0x333333))
                .child(text("Item"))
                .child(text("Next")),
        )
}

#[test]
fn a_framed_popup_beside_its_trigger_starts_its_rows_on_the_trigger_s_row() {
    let ((start, y), terminal) = framed(Root::new::<FramedBeside>(|| ()));
    let (x, item) = find(&terminal, "Item").unwrap();
    assert_eq!(
        item,
        y,
        "the first row on the trigger's row, the frame on the row above\n{}",
        dump(&terminal)
    );
    assert!(
        x > start + 20,
        "beside the trigger (its 20 columns)\n{}",
        dump(&terminal)
    );
    let corner = find(&terminal, "╭").unwrap_or_else(|| panic!("a frame\n{}", dump(&terminal)));
    assert_eq!(
        corner,
        (start + 20, y - 1),
        "the frame's corner after the trigger's last column, a row above it\n{}",
        dump(&terminal)
    );
}

/// A page with a row of `zzz` words across the screen, under a sheet at the right
/// with a panel 10 Cells wide: a fill, a line on its leading edge.
#[component]
fn RightSheet() -> impl IntoElement {
    view()
        .flex_1()
        .child(text("top"))
        // Words, as an overlay's box leaves out a word it cuts: one word
        // across the screen would go whole.
        .child(text("zzz ".repeat(10)))
        .child(
            sheet()
                .open(|| Some(true))
                .side(Placement::Right)
                .popup_style(|s| {
                    s.w(px(80.0))
                        .p(px(16.0))
                        .border_l_1()
                        .bg(nsh_core::color::rgb(0x333333))
                })
                .child(text("Panel")),
        )
}

#[test]
fn a_sheet_is_a_solid_panel_the_height_of_the_screen_over_a_cleared_page() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(App::Rust(Root::new::<RightSheet>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Panel");
    // It slides in from the edge: look once it is in place.
    super::app::draw_until_still(&mut view, &mut terminal, &rx);
    let (panel, _) = find(&terminal, "Panel").unwrap();
    let buffer = terminal.backend().buffer();
    // Its leading line: the column before its text where every row has
    // one.
    let edge = (0..panel)
        .rev()
        .find(|&x| (0..12).all(|y| buffer[(x, y)].symbol() == "│"))
        .unwrap_or_else(|| panic!("a line down the panel's edge\n{}", dump(&terminal)));
    assert!(edge >= 28, "the panel at the right\n{}", dump(&terminal));
    for y in 0..12 {
        for x in edge..40 {
            assert_ne!(
                buffer[(x, y)].symbol(),
                "z",
                "the page cleared under the panel at {x},{y}\n{}",
                dump(&terminal)
            );
        }
    }
    let (_, row) = find(&terminal, "zzz ")
        .unwrap_or_else(|| panic!("the page's row beside the panel\n{}", dump(&terminal)));
    assert!(
        (edge - 4..edge).any(|x| buffer[(x, row)].symbol() == "z"),
        "the page beside it\n{}",
        dump(&terminal)
    );
}

/// A toast in a stack at the bottom right of the screen, in an overlay
/// along the bottom edge: a fill, no border, so the terminal frames it
/// outside its box.
#[component]
fn BottomToast() -> impl IntoElement {
    view().flex_1().child(text("page")).child(
        overlay().left_0().right_0().bottom_0().items_end().child(
            toast_stack()
                .placement(Anchor::BottomRight)
                .w(px(160.0))
                .child(
                    toast()
                        .bg(nsh_core::color::rgb(0x333333))
                        .px(px(8.0))
                        .child(text("Saved")),
                ),
        ),
    )
}

#[test]
fn a_toast_at_the_bottom_shows_with_its_whole_frame() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(App::Rust(Root::new::<BottomToast>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Saved");
    let (_, saved) = find(&terminal, "Saved").unwrap();
    let (_, top) = find(&terminal, "╭").unwrap_or_else(|| panic!("a frame\n{}", dump(&terminal)));
    let (_, bottom) =
        find(&terminal, "╰").unwrap_or_else(|| panic!("the frame's bottom\n{}", dump(&terminal)));
    assert!(
        top < saved && saved < bottom && bottom <= 11,
        "the toast inside its frame, on the screen: {top} {saved} {bottom}\n{}",
        dump(&terminal)
    );
}

/// An open searchable select of four tags, its search field `Search tags`.
#[component]
fn SearchSelect() -> impl IntoElement {
    view().flex_1().child(text("above")).child(
        select()
            .items(tags())
            .searchable(true)
            .search_placeholder("Search tags")
            .accessibility_label("Tags")
            .default_open(true)
            .w(px(200.0)),
    )
}

#[test]
fn a_searchable_select_s_search_field_has_a_rule_under_it() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    view.load(App::Rust(Root::new::<SearchSelect>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Swift");
    let (x, field) = find(&terminal, "Search tags")
        .unwrap_or_else(|| panic!("the search field\n{}", dump(&terminal)));
    assert_eq!(
        terminal.backend().buffer()[(x, field + 1)].symbol(),
        "─",
        "a rule under the field\n{}",
        dump(&terminal)
    );
    let (_, first) = find(&terminal, "Rust").unwrap();
    assert_eq!(
        first,
        field + 2,
        "the rows under the rule\n{}",
        dump(&terminal)
    );
}

/// An open select of four tags at its natural width, `Go` chosen: its
/// trigger `Go ▾` is narrower than the other titles.
#[component]
fn ChosenSelect() -> impl IntoElement {
    view().flex_1().child(text("above")).child(
        view().flex_row().child(
            select()
                .items(tags())
                .default_value(SharedString::from("go"))
                .accessibility_label("Tags")
                .default_open(true),
        ),
    )
}

#[test]
fn a_select_s_chosen_row_has_a_check_and_its_list_fits_the_widest_title() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    view.load(App::Rust(Root::new::<ChosenSelect>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Swift");
    let (mark, chosen) = find(&terminal, "✓ Go")
        .unwrap_or_else(|| panic!("the chosen row's check\n{}", dump(&terminal)));
    let (swift, row) = find(&terminal, "Swift").unwrap();
    assert_eq!(
        swift,
        mark + 2,
        "the other titles after two spaces, in the chosen title's column\n{}",
        dump(&terminal)
    );
    assert!(
        row > chosen,
        "Swift on a row of its own\n{}",
        dump(&terminal)
    );
    let end = terminal.backend().buffer()[(swift + 5, row)]
        .symbol()
        .to_owned();
    assert_eq!(
        end,
        "│",
        "Swift whole, the frame right after the widest row\n{}",
        dump(&terminal)
    );
}

#[test]
fn a_select_s_highlighted_row_is_an_accent_band() {
    // The option is focused only as a part of the list (the keys stay
    // with the select): the core mounts that for the terminal to mark. The
    // accent band is the look with surfaces (the terminal's background
    // known); bandless, the row is reversed.
    let (mut view, rx) = {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut options = nsh_platform_tui::AppOptions::default();
        options.capabilities.colors = nsh_platform_tui::ColorDepth::TrueColor;
        options.capabilities.background_color = Some([24; 3]);
        options.capabilities.foreground_color = Some([224; 3]);
        options.wake = Some(std::sync::Arc::new(move || {
            let _ = tx.send(());
        }));
        options.cache = nsh_engine::CacheOptions::off();
        (nsh_platform_tui::AppView::new(options), rx)
    };
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    view.load(App::Rust(Root::new::<ChosenSelect>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Swift");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        // The row's title, not the trigger's above it.
        let (x, y) = find(&terminal, "✓ Go").unwrap();
        if terminal.backend().buffer()[(x, y)].bg == ratatui_core::style::Color::Blue {
            break;
        }
        assert!(
            rx.recv_deadline(deadline).is_ok(),
            "the chosen row, highlighted as the list opens, on the accent\n{}",
            dump(&terminal)
        );
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
    }
    let (x, y) = find(&terminal, "Swift").unwrap();
    assert_ne!(
        terminal.backend().buffer()[(x, y)].bg,
        ratatui_core::style::Color::Blue,
        "the other rows off it\n{}",
        dump(&terminal)
    );
}

/// A box in an overlay over rows of page text it cuts across, centered
/// (a margin is at most two Cells in a terminal: spacing, not a place).
#[component]
fn OverWords() -> impl IntoElement {
    let mut page = view().flex_1();
    for _ in 0..6 {
        page = page.child(text("alpha bravo charlie delta echo foxtrot"));
    }
    page.child(
        overlay().left_0().right_0().top_0().child(
            view().mt(px(32.0)).flex_row().justify_center().child(
                view()
                    .w(px(120.0))
                    .bg(nsh_core::color::rgb(0x333333))
                    .child(text("Box")),
            ),
        ),
    )
}

#[test]
fn an_overlay_s_box_leaves_out_the_words_it_cuts_at_its_sides() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(App::Rust(Root::new::<OverWords>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Box");
    let (_, y) = find(&terminal, "Box").unwrap();
    let buffer = terminal.backend().buffer();
    let line: String = (0..40).map(|x| buffer[(x, y)].symbol()).collect();
    let cells: Vec<&str> = (0..40).map(|x| buffer[(x, y)].symbol()).collect();
    let first = cells.iter().position(|c| *c == "│").expect(&line) as u16;
    let last = cells.iter().rposition(|c| *c == "│").expect(&line) as u16;
    assert!(first > 0 && last + 1 < 40, "{line}");
    for x in [first - 1, last + 1] {
        assert_eq!(
            buffer[(x, y)].symbol(),
            " ",
            "no half word on the frame: {line}"
        );
    }
    assert!(
        line.starts_with("alpha"),
        "whole words before it stay: {line}"
    );
}

/// A box in an overlay over rows of one long unbroken run (a URL), which
/// it cuts in the middle.
#[component]
fn OverUrl() -> impl IntoElement {
    let mut page = view().flex_1();
    for _ in 0..6 {
        page = page.child(text("https://example.com/abcdefghijklmnop/q"));
    }
    page.child(
        overlay().left_0().right_0().top_0().child(
            view().mt(px(32.0)).flex_row().justify_center().child(
                view()
                    .w(px(80.0))
                    .bg(nsh_core::color::rgb(0x333333))
                    .child(text("Box")),
            ),
        ),
    )
}

#[test]
fn an_overlay_s_box_keeps_the_shown_part_of_a_long_run_it_cuts_with_an_ellipsis() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(App::Rust(Root::new::<OverUrl>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Box");
    let (_, y) = find(&terminal, "Box").unwrap();
    let buffer = terminal.backend().buffer();
    let line: String = (0..40).map(|x| buffer[(x, y)].symbol()).collect();
    let cells: Vec<&str> = (0..40).map(|x| buffer[(x, y)].symbol()).collect();
    let first = cells.iter().position(|c| *c == "│").expect(&line) as u16;
    let last = cells.iter().rposition(|c| *c == "│").expect(&line) as u16;
    assert!(
        line.starts_with("https://exam"),
        "the part before the box stays: {line}"
    );
    assert_eq!(
        buffer[(first - 1, y)].symbol(),
        "…",
        "ending in an ellipsis by the frame: {line}"
    );
    assert_eq!(
        buffer[(last + 1, y)].symbol(),
        "…",
        "and starting in one after it: {line}"
    );
    assert_ne!(
        buffer[(last + 2, y)].symbol(),
        " ",
        "the part after the box stays: {line}"
    );
}

/// An overlay whose box holds a filled chip before a name (a tooltip's
/// avatar and reviewer).
#[component]
fn ChipInOverlay() -> impl IntoElement {
    view().flex_1().child(text("page")).child(
        overlay().left_0().right_0().top_0().child(
            view().mt(px(32.0)).flex_row().justify_center().child(
                view()
                    .bg(nsh_core::color::rgb(0x333333))
                    .flex_row()
                    .child(view().bg(nsh_core::color::rgb(0x555555)).child(text("MC")))
                    .child(text(" Mira Chen")),
            ),
        ),
    )
}

#[test]
fn a_surface_inside_an_overlay_cuts_no_word_of_the_overlay_s_own() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(App::Rust(Root::new::<ChipInOverlay>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Chen");
    let (_, y) = find(&terminal, "Chen").unwrap();
    let buffer = terminal.backend().buffer();
    let line: String = (0..40).map(|x| buffer[(x, y)].symbol()).collect();
    assert!(
        line.contains("MC Mira Chen"),
        "the name stays whole: {line}"
    );
}
