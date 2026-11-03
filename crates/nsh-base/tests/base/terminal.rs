//! `nsh/base`'s controls in a terminal (`Idiom::Terminal`): each shows its
//! terminal-native form, text the terminal draws (`nsh_base::terminal`),
//! in place of the app's indicator parts; elsewhere they show none of it.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nsh_base::{
    ColorChannel, ColorPickerEvent, ColorSpace, SelectItem, TreeEntry, TreeItem, accordion,
    accordion_header, accordion_item, accordion_panel, accordion_trigger, checkbox, color_area,
    color_picker, color_slider, color_swatch, progress, radio, radio_group, select, slider, switch,
    terminal::{self, PROGRESS_EIGHTHS, PROGRESS_EMPTY, SLIDER_FILL, SLIDER_TRACK},
    text_view, tree,
};
use nsh_core::{
    MountItem, NodeId, Size,
    element::Root,
    geometry::point,
    platform::{Environment, Idiom, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::{Headless, Screen};

fn start<C: Component<Props = ()>>(idiom: Idiom) -> Headless {
    let viewport = Size::new(px(390.0), px(844.0));
    let config = EngineConfig {
        viewport,
        environment: Environment {
            window: viewport,
            os: Os::Linux,
            idiom,
            ..Environment::default()
        },
        ..EngineConfig::new(Root::of::<C>())
    };
    let mut app = Headless::start(config).unwrap();
    app.first_render().unwrap();
    app
}

/// The texts on screen, spaces kept.
fn texts(app: &Headless) -> Vec<String> {
    app.screen()
        .texts()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn shows_exactly(app: &Headless, text: &str) -> bool {
    app.screen().texts().contains(&text)
}

/// A checkbox, a radio set and a switch the app owns the values of.
#[component]
fn Marks() -> impl IntoElement {
    let agree = signal(false);
    let mixed = signal(true);
    let plan = signal("free");
    let wifi = signal(false);
    view()
        .child(
            checkbox()
                .checked(agree)
                .on_change(move |next: &bool| agree.set(*next))
                .indicator_class("w-4 h-4")
                .child(text("Agree")),
        )
        .child(
            checkbox()
                .indeterminate(mixed)
                .checked(move || !mixed.get())
                .on_change(move |_: &bool| mixed.set(false))
                .child(text("All")),
        )
        .child(
            radio_group()
                .child(
                    radio()
                        .checked(move || plan.get() == "free")
                        .on_change(move |_: &bool| plan.set("free"))
                        .child(text("Free")),
                )
                .child(
                    radio()
                        .checked(move || plan.get() == "pro")
                        .on_change(move |_: &bool| plan.set("pro"))
                        .child(text("Pro")),
                ),
        )
        .child(
            switch()
                .checked(wifi)
                .on_change(move |next: &bool| wifi.set(*next))
                .child(text("Wi-Fi")),
        )
}

/// The screen's texts joined in order: a mark's parts are texts of their
/// own (each in its tone), so the marks and their labels read as one row
/// here ("[ ] Agree").
fn joined(app: &Headless) -> String {
    texts(app).concat()
}

#[test]
fn checkboxes_radios_and_switches_show_the_terminals_marks() {
    let mut app = start::<Marks>(Idiom::Terminal);
    // Agree unchecked, All mixed, Free checked, Pro not, Wi-Fi off.
    let shown = joined(&app);
    for row in ["[ ] Agree", "[–] All", "(●) Free", "( ) Pro", "●── Wi-Fi"] {
        assert!(shown.contains(row), "{row} not in {shown:?}");
    }
    app.press("text:Agree").unwrap();
    app.press("text:All").unwrap();
    app.press("text:Pro").unwrap();
    app.press("text:Wi-Fi").unwrap();
    app.wait_for("text:━━").unwrap();
    let shown = joined(&app);
    for row in ["[✓] Agree", "[✓] All", "( ) Free", "(●) Pro", "━━● Wi-Fi"] {
        assert!(shown.contains(row), "{row} not in {shown:?}");
    }
}

#[test]
fn the_marks_take_the_accent_when_on_and_are_muted_when_off() {
    use terminal::{Tone, checkbox_mark, radio_mark, switch_mark};
    for on in [
        checkbox_mark(true, false),
        checkbox_mark(false, true),
        radio_mark(true),
    ] {
        assert_eq!(on[1].1, Tone::Accent, "{on:?}");
    }
    assert_eq!(switch_mark(true)[1].1, Tone::Accent);
    for off in [
        checkbox_mark(false, false),
        radio_mark(false),
        switch_mark(false),
    ] {
        assert!(off.iter().all(|(_, tone)| *tone == Tone::Muted), "{off:?}");
    }
}

#[test]
fn outside_a_terminal_the_controls_show_no_marks() {
    let app = start::<Marks>(Idiom::Desktop);
    let shown = joined(&app);
    for mark in ["[ ]", "[–]", "(●)", "( )", "●──"] {
        assert!(!shown.contains(mark), "{mark} in {shown:?}");
    }
    assert!(app.screen().shows("Agree"));
}

/// A progress bar at a value the app sets with F5.
#[component]
fn Bar() -> impl IntoElement {
    let value = signal(50.0f32);
    on_key("f5", move |_| value.set(100.0));
    view().child(progress().value(value).w(px(200.0)))
}

/// The texts made of `glyphs` only, joined in order (the terminal's bar,
/// whose filled run is a text of its own, or a track).
fn row_of(app: &Headless, glyphs: &[char]) -> String {
    let row: String = texts(app)
        .into_iter()
        .filter(|t| !t.is_empty() && t.chars().all(|c| glyphs.contains(&c)))
        .collect();
    assert!(!row.is_empty(), "no row of {glyphs:?} in {:?}", texts(app));
    row
}

#[test]
fn a_progress_bar_is_a_row_of_eighth_blocks() {
    let mut app = start::<Bar>(Idiom::Terminal);
    let glyphs: Vec<char> = PROGRESS_EIGHTHS
        .into_iter()
        .chain([PROGRESS_EMPTY])
        .collect();
    let bar = row_of(&app, &glyphs);
    let columns = bar.chars().count();
    assert!(columns > 2, "{bar:?}");
    // Half the Cells are full, as the terminal's own bar draws it.
    assert_eq!(bar, terminal::progress_bar(Percentage(0.5), columns));
    app.press_key("f5").unwrap();
    assert_eq!(
        row_of(&app, &glyphs),
        terminal::progress_bar(Percentage(1.0), columns)
    );
}

/// A slider at 25 of 0 to 100.
#[component]
fn Volume() -> impl IntoElement {
    view().child(
        slider()
            .default_value(25.0)
            .accessibility_label("Volume")
            .w(px(200.0)),
    )
}

#[test]
fn a_slider_is_a_track_of_rules_with_a_thumb() {
    let app = start::<Volume>(Idiom::Terminal);
    let track = row_of(&app, &[SLIDER_TRACK, SLIDER_FILL]);
    let columns = track.chars().count();
    assert!(columns > 2, "{track:?}");
    let at = terminal::track_cell(Percentage(0.25), columns);
    assert_eq!(track, terminal::slider_track(columns, 0, at));
    assert!(shows_exactly(&app, terminal::SLIDER_THUMB));
}

/// A select, an accordion item and a tree.
#[component]
fn Lists() -> impl IntoElement {
    let open = signal(Vec::<SharedString>::new());
    view()
        .child(
            select()
                .items(vec![
                    SelectItem::new("apple", "Apple"),
                    SelectItem::new("pear", "Pear"),
                ])
                .accessibility_label("Fruit")
                .trigger(view().child(text("Pick"))),
        )
        .child(
            accordion()
                .value(open)
                .on_change(move |next: &Vec<SharedString>| open.set(next.clone()))
                .child(
                    accordion_item()
                        .value("one")
                        .child(accordion_header().child(accordion_trigger().child(text("One"))))
                        .child(accordion_panel().child(text("One details"))),
                ),
        )
        .child(
            tree()
                .items(vec![
                    TreeItem::new("src", "src").child(TreeItem::new("src/lib.rs", "lib.rs")),
                    TreeItem::new("README.md", "README.md"),
                ])
                .default_expanded(vec![SharedString::from("src")])
                .w(px(300.0))
                .h(px(200.0)),
        )
}

#[test]
fn triggers_disclosures_and_tree_rows_show_the_terminals_marks() {
    let mut app = start::<Lists>(Idiom::Terminal);
    let shown = texts(&app);
    assert!(
        shown.iter().any(|t| t == terminal::SELECT_ARROW),
        "{shown:?}"
    );
    assert!(shown.iter().any(|t| t == "▸ "), "{shown:?}");
    assert!(shown.iter().any(|t| t == "▾ src"), "{shown:?}");
    // A leaf one level down: two Cells of indent, two of no disclosure.
    assert!(shown.iter().any(|t| t == "    lib.rs"), "{shown:?}");
    assert!(shown.iter().any(|t| t == "  README.md"), "{shown:?}");
    app.press("text:One").unwrap();
    app.wait_for("text:One details").unwrap();
    assert!(texts(&app).iter().any(|t| t == "▾ "));
}

/// An accordion item open from the start and a tree whose rows the app
/// draws (`renderItem`), indented by depth as apps do.
#[component]
fn Drawn() -> impl IntoElement {
    let open = signal(vec![SharedString::from("one")]);
    view()
        .child(
            accordion()
                .value(open)
                .on_change(move |next: &Vec<SharedString>| open.set(next.clone()))
                .child(
                    accordion_item()
                        .value("one")
                        .child(
                            accordion_header().child(
                                accordion_trigger()
                                    .gap(px(10.0))
                                    .child(text("Shipping"))
                                    .child(text("1 day")),
                            ),
                        )
                        .child(accordion_panel().child(text("Same day before 2 pm"))),
                ),
        )
        .child(
            tree()
                .items(vec![
                    TreeItem::new("src", "src").child(TreeItem::new("src/lib.rs", "lib.rs")),
                ])
                .default_expanded(vec![SharedString::from("src")])
                .render_item(|e: &TreeEntry| {
                    view()
                        .pl(px(e.depth as f32 * 14.0))
                        .child(text(e.label.clone()))
                })
                .w(px(300.0))
                .h(px(200.0)),
        )
}

/// The text shown just before `label`.
fn before(app: &Headless, label: &str) -> Option<String> {
    let shown = texts(app);
    let at = shown.iter().position(|t| t == label)?;
    at.checked_sub(1).map(|at| shown[at].clone())
}

#[test]
fn an_accordions_disclosure_leads_its_title_and_the_panel_lines_up_under_it() {
    let app = start::<Drawn>(Idiom::Terminal);
    // The trigger's disclosure, then its children a column apart: the
    // trigger's own gap does not part the disclosure from the title.
    assert_eq!(before(&app, "Shipping").as_deref(), Some("▾ "));
    assert_eq!(before(&app, "1 day").as_deref(), Some("Shipping"));
    // The panel's content past the disclosure, under the title.
    assert_eq!(
        before(&app, "Same day before 2 pm").as_deref(),
        Some(terminal::DISCLOSURE_NONE)
    );
    // An app's row is the app's: the terminal's layout leads a folder's.
    assert_eq!(before(&app, "src").as_deref(), Some("Same day before 2 pm"));
    let gui = start::<Drawn>(Idiom::Desktop);
    assert_eq!(before(&gui, "Shipping"), None);
    assert_eq!(before(&gui, "src").as_deref(), Some("Same day before 2 pm"));
}

/// Where each text starts across the window in a terminal, from the
/// frames and parents the batches set.
fn text_starts<C: Component<Props = ()>>() -> std::collections::HashMap<String, f32> {
    let viewport = Size::new(px(390.0), px(844.0));
    let config = EngineConfig {
        viewport,
        environment: Environment {
            window: viewport,
            os: Os::Linux,
            idiom: Idiom::Terminal,
            ..Environment::default()
        },
        ..EngineConfig::new(Root::of::<C>())
    };
    let mut app = Headless::start(config).unwrap();
    type Seen = (
        std::collections::HashMap<NodeId, String>,
        std::collections::HashMap<NodeId, Bounds<Points>>,
        std::collections::HashMap<NodeId, NodeId>,
    );
    let seen = Arc::new(Mutex::new(Seen::default()));
    let into = seen.clone();
    app.observe(move |_, batch| {
        let mut seen = into.lock().unwrap();
        for item in &batch.items {
            match item {
                MountItem::Text { id, text, .. } => {
                    seen.0.insert(*id, text.text.to_string());
                }
                MountItem::Frame { id, frame } => {
                    seen.1.insert(*id, *frame);
                }
                MountItem::Insert { parent, child, .. } => {
                    seen.2.insert(*child, *parent);
                }
                _ => {}
            }
        }
    });
    app.first_render().unwrap();
    let seen = seen.lock().unwrap();
    let x = |mut id: NodeId| {
        let mut x = 0.0;
        loop {
            x += seen.1.get(&id).map_or(0.0, |f| f.origin.x.0);
            match seen.2.get(&id) {
                Some(parent) => id = *parent,
                None => return x,
            }
        }
    };
    seen.0.iter().map(|(id, t)| (t.clone(), x(*id))).collect()
}

#[test]
fn an_accordion_triggers_own_mark_takes_no_disclosure_lead_of_the_cores() {
    // The trigger is a disclosure (a button with `expanded`), but its first
    // text is its mark: the core keeps no columns before it, so the mark
    // starts where the panel's lead does, and the title under the panel's.
    let starts = text_starts::<Drawn>();
    let at = |label: &str| {
        *starts
            .get(label)
            .unwrap_or_else(|| panic!("no {label:?} in {starts:?}"))
    };
    assert_eq!(at("▾ "), at(terminal::DISCLOSURE_NONE), "{starts:?}");
    assert_eq!(at("Shipping"), at("Same day before 2 pm"), "{starts:?}");
}

/// A document with lists, tasks, a footnote and a table.
#[component]
fn Document() -> impl IntoElement {
    text_view().markdown(
        "1. First\n2. Second\n   - nested\n\n- [ ] Open task\n- [x] Done task\n\n\
         Numbers from benchmarks[^1].\n\n\
         | Platform | Views |\n|---|---|\n| iOS | UIKit |\n\n[^1]: A note.",
    )
}

#[test]
fn a_documents_markers_rules_and_notes_are_the_terminals_text() {
    let app = start::<Document>(Idiom::Terminal);
    let shown = joined(&app);
    // Markers a column before their content, nested ones indented by them.
    for row in [
        "1. First",
        "2. Second",
        "◦ nested",
        "[ ] Open task",
        "[✓] Done task",
    ] {
        assert!(shown.contains(row), "{row} not in {shown:?}");
    }
    // The reference apart from its word.
    assert!(shown.contains("benchmarks[1]"), "{shown:?}");
    // The table ruled in box drawing, its cells between bars.
    let lines = texts(&app);
    for (start, end) in [('┌', '┐'), ('├', '┤'), ('└', '┘')] {
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with(start) && l.ends_with(end) && l.contains('─')),
            "no {start}…{end} rule in {lines:?}"
        );
    }
    assert!(shown.contains("│ Platform"), "{shown:?}");
    assert!(shown.contains("│ iOS"), "{shown:?}");
    // Elsewhere, none of it.
    let gui = start::<Document>(Idiom::Desktop);
    let shown = joined(&gui);
    assert!(!shown.contains("[ ]") && !shown.contains('┌'), "{shown:?}");
}

/// A picker of `#3b82f6` (HSB hue 217) with an area, a hue slider and a
/// swatch of the picked color; the color in hex as the app keeps it.
#[component]
fn Colors() -> impl IntoElement {
    let color = signal(Hsla::from(rgb(0x3b82f6)));
    let hex = signal(String::from("#3b82f6"));
    view()
        .child(
            color_picker()
                .value(move || Some(color.get()))
                .on_change(move |e: &ColorPickerEvent| {
                    if let ColorPickerEvent::Change(value, formatted) = e {
                        color.set(*value);
                        hex.set(formatted.to_string());
                    }
                })
                .child(
                    color_area()
                        .accessibility_label("Area")
                        .w(px(200.0))
                        .h(px(200.0)),
                )
                .child(
                    color_slider()
                        .color_space(ColorSpace::Hsb)
                        .channel(ColorChannel::Hue)
                        .accessibility_label("Hue")
                        .w(px(200.0))
                        // What shows the slider has the keyboard focus.
                        .focus(|s| s.opacity(0.5)),
                )
                .child(color_swatch().accessibility_label("Current")),
        )
        .child(text(move || format!("Color {}", hex.get())))
}

/// The hue slider's track, its glyphs joined from the start.
fn hue_track(screen: &Screen) -> String {
    screen
        .texts()
        .into_iter()
        .filter(|t| !t.is_empty() && t.chars().all(|c| ['━', '●', '─'].contains(&c)))
        .collect()
}

#[test]
fn a_color_slider_is_a_row_of_its_colors_with_a_thumb() {
    let mut app = start::<Colors>(Idiom::Terminal);
    let track = hue_track(app.screen());
    let columns = track.chars().count();
    assert!(columns > 2, "{track:?}");
    // One thumb, on the Cell of 217 of 360, `━` before it and `─` after.
    let at = terminal::track_cell(Percentage(217.0 / 360.0), columns);
    let expected: String = (0..columns)
        .map(|i| match i.cmp(&at) {
            std::cmp::Ordering::Less => '━',
            std::cmp::Ordering::Equal => '●',
            std::cmp::Ordering::Greater => '─',
        })
        .collect();
    assert_eq!(track, expected);
    // The slider is one adjustable control; its Cells are pictures hidden
    // from screen readers.
    assert_eq!(app.screen().count("label:Hue").unwrap(), 1);
    // A drag to the start picks the smallest hue, its thumb on the first
    // Cell; End the largest, on the last.
    app.drag("label:Hue", point(px(-400.0), px(0.0))).unwrap();
    let wait = Duration::from_secs(1);
    app.wait_until("the thumb at the start", wait, |s| {
        hue_track(s).starts_with('●')
    })
    .unwrap();
    // A drag gives no keyboard focus: Tab to the slider (its `focus:`
    // style says when it has it), then End: the largest hue, on the last
    // Cell, `━` before it.
    let slider = app
        .screen()
        .press_target("label:Hue")
        .unwrap()
        .expect("the slider takes presses");
    let opacity = Arc::new(Mutex::new(1.0_f32));
    let into = opacity.clone();
    app.observe(move |_, batch| {
        for item in &batch.items {
            if let MountItem::ViewStyle { id, style } = item
                && *id == slider
            {
                *into.lock().unwrap() = style.opacity;
            }
        }
    });
    let mut tabs = 0;
    while *opacity.lock().unwrap() == 1.0 {
        assert!(tabs < 3, "Tab never reached the slider");
        app.press_key("tab").unwrap();
        let o = opacity.clone();
        let _ = app.wait_until(
            "Tab moves the focus",
            Duration::from_millis(200),
            move |_| *o.lock().unwrap() < 1.0,
        );
        tabs += 1;
    }
    app.press_key("end").unwrap();
    let at_end = |s: &Screen| {
        let track = hue_track(s);
        track.ends_with('●') && track.chars().filter(|&c| c == '━').count() == columns - 1
    };
    app.wait_until("the thumb at the end", wait, at_end)
        .unwrap();
}

#[test]
fn a_color_area_is_a_grid_of_cells_with_the_picked_one_marked() {
    let mut app = start::<Colors>(Idiom::Terminal);
    let picked = |app: &Headless| {
        texts(app)
            .iter()
            .filter(|t| t.as_str() == terminal::AREA_PICKED)
            .count()
    };
    assert_eq!(picked(&app), 1, "{:?}", texts(&app));
    // A press at the area's top left: no saturation, full brightness.
    app.press_at("label:Area", point(px(1.0), px(1.0))).unwrap();
    app.wait_for("text:Color #ffffff").unwrap();
    assert_eq!(picked(&app), 1);
    // The swatch: four Cells of the color by itself.
    assert!(shows_exactly(&app, terminal::SWATCH), "{:?}", texts(&app));
}

#[test]
fn outside_a_terminal_the_color_parts_draw_no_cells() {
    let app = start::<Colors>(Idiom::Desktop);
    let shown = joined(&app);
    assert!(!shown.contains(terminal::AREA_PICKED), "{shown:?}");
    assert!(!shown.contains('●'), "{shown:?}");
}

/// A choice card: a checkbox whose children are a title over a
/// description, as the app lays them out.
#[component]
fn Choice() -> impl IntoElement {
    let on = signal(false);
    view().child(
        checkbox()
            .checked(on)
            .on_change(move |next: &bool| on.set(*next))
            .w(px(300.0))
            .child(text("Push"))
            .child(text("As it happens")),
    )
}

#[test]
fn a_checkboxs_children_keep_their_column_beside_the_mark() {
    let viewport = Size::new(px(390.0), px(844.0));
    let config = EngineConfig {
        viewport,
        environment: Environment {
            window: viewport,
            os: Os::Linux,
            idiom: Idiom::Terminal,
            ..Environment::default()
        },
        ..EngineConfig::new(Root::of::<Choice>())
    };
    let mut app = Headless::start(config).unwrap();
    // Each text's frame, in its parent, as the batches set it.
    let seen = Arc::new(Mutex::new((
        std::collections::HashMap::<NodeId, String>::new(),
        std::collections::HashMap::<NodeId, Bounds<Points>>::new(),
    )));
    let into = seen.clone();
    app.observe(move |_, batch| {
        let mut seen = into.lock().unwrap();
        for item in &batch.items {
            match item {
                MountItem::Text { id, text, .. } => {
                    seen.0.insert(*id, text.text.to_string());
                }
                MountItem::Frame { id, frame } => {
                    seen.1.insert(*id, *frame);
                }
                _ => {}
            }
        }
    });
    app.first_render().unwrap();
    let seen = seen.lock().unwrap();
    let frame = |label: &str| {
        let id = seen
            .0
            .iter()
            .find(|(_, t)| t.as_str() == label)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no text {label:?} in {:?}", seen.0));
        seen.1[&id]
    };
    let (title, description) = (frame("Push"), frame("As it happens"));
    // The description under the title, at its start: a column, not a row.
    assert!(
        description.origin.y >= title.origin.y + title.size.height,
        "{title:?} {description:?}"
    );
    assert_eq!(description.origin.x, title.origin.x);
    // The mark is still beside them.
    assert!(joined(&app).starts_with("[ ] Push"), "{:?}", joined(&app));
}
