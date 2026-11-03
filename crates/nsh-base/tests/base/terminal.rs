//! `nsh/base`'s controls in a terminal (`Idiom::Terminal`): each shows its
//! terminal-native form, text the terminal draws (`nsh_base::terminal`),
//! in place of the app's indicator parts; elsewhere they show none of it.

use nsh_base::{
    SelectItem, TreeItem, accordion, accordion_header, accordion_item, accordion_panel,
    accordion_trigger, checkbox, progress, radio, radio_group, select, slider, switch,
    terminal::{self, PROGRESS_EIGHTHS, PROGRESS_EMPTY, SLIDER_FILL, SLIDER_TRACK},
    tree,
};
use nsh_core::{
    Size,
    element::Root,
    platform::{Environment, Idiom, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

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
