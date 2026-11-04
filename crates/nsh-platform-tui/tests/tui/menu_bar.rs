//! The application menu as a menu bar on the surface's top row.

use nsh_platform_tui::{Bundle, Handled, Input, Modifiers, MouseButton, MouseKind};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

use super::app::{draw_until, mouse, surface};

/// An app with `File`, `Edit` and `View` menus, which writes what was
/// chosen under `top` (its first row).
fn app(menu: bool) -> Bundle {
    let set = if menu {
        r#"Menu.setApplicationMenu([
            { label: "File", submenu: [
                { label: "Open", shortcut: "ctrl+o", onSelect: () => log.set("open") },
                { type: "separator" },
                { role: "minimize" },
                { label: "Save", disabled: true },
            ] },
            { label: "Edit", submenu: [{ label: "Find", onSelect: () => log.set("find") }] },
            { label: "View", submenu: [{ label: "Zoom In", onSelect: () => log.set("zoom") }] },
        ]);"#
    } else {
        ""
    };
    let code = nsh_bundler::compile_module(
        "App.tsx",
        &format!(
            r#"
        import {{ mount, signal, View, Text }} from "nsh";
        import {{ Menu }} from "nsh/desktop";
        const log = signal("none");
        {set}
        mount(() => <View>
            <Text>top</Text>
            <Text>chose {{log()}}</Text>
        </View>);
    "#
        ),
        &[],
    )
    .unwrap();
    Bundle::source(code)
}

fn key(name: &str) -> Input {
    Input::Key {
        key: name.into(),
        modifiers: Default::default(),
        release: false,
    }
}

fn row(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| row(terminal, y) + "\n")
        .collect()
}

#[test]
fn an_application_menu_is_a_bar_on_the_top_row_above_the_app() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(app(true)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "File");
    draw_until(&mut view, &mut terminal, &rx, "chose none");
    assert!(row(&terminal, 0).starts_with("  File  Edit  View "), "{}", screen(&terminal));
    assert!(row(&terminal, 1).starts_with("top"), "{}", screen(&terminal));
    // Without one the app has every row.
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(app(false)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "chose none");
    assert!(row(&terminal, 0).starts_with("top"), "{}", screen(&terminal));
}

#[test]
fn f10_gives_the_bar_the_keys_and_its_menus_open_below_their_titles() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(app(true)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "chose none");
    assert_eq!(view.handle(key("F10")), Handled::Yes);
    // Right to Edit, Down opens its menu below the title.
    view.handle(key("ArrowRight"));
    view.handle(key("ArrowDown"));
    draw_until(&mut view, &mut terminal, &rx, "Find");
    assert_eq!(terminal.backend().buffer()[(7, 1)].symbol(), "╭", "{}", screen(&terminal));
    // Left opens File's: its shortcut at the right, the window's roles
    // left out.
    view.handle(key("ArrowLeft"));
    draw_until(&mut view, &mut terminal, &rx, "Open");
    let shortcut = if cfg!(target_os = "macos") { "⌃O" } else { "Ctrl+O" };
    let open = screen(&terminal)
        .lines()
        .find(|line| line.contains("Open"))
        .unwrap()
        .to_string();
    assert!(open.contains(shortcut), "{}", screen(&terminal));
    assert!(!screen(&terminal).contains("Minimize"), "{}", screen(&terminal));
    // Escape closes the menu and the bar keeps the keys; Enter chooses
    // in the menu Down opened again.
    view.handle(key("Escape"));
    draw_until(&mut view, &mut terminal, &rx, "top");
    assert!(!screen(&terminal).contains("Open"), "{}", screen(&terminal));
    view.handle(key("ArrowDown"));
    draw_until(&mut view, &mut terminal, &rx, "Open");
    view.handle(key("Enter"));
    draw_until(&mut view, &mut terminal, &rx, "chose open");
    assert!(!screen(&terminal).contains("Open"), "{}", screen(&terminal));
    // A title's initial opens it; Escape twice gives the keys back.
    view.handle(key("F10"));
    view.handle(key("v"));
    draw_until(&mut view, &mut terminal, &rx, "Zoom In");
    view.handle(key("Escape"));
    view.handle(key("Escape"));
    draw_until(&mut view, &mut terminal, &rx, "top");
    assert!(!screen(&terminal).contains("Zoom In"));
    view.handle(key("ArrowDown"));
    draw_until(&mut view, &mut terminal, &rx, "top");
    assert!(!screen(&terminal).contains("Open"), "the bar let the keys go");
}

#[test]
fn a_press_on_a_title_opens_its_menu_and_its_shortcut_chooses_without_it() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(app(true)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "chose none");
    let click = |view: &mut nsh_platform_tui::AppView, x, y| {
        view.handle(mouse(MouseKind::Down(MouseButton::Left), x, y));
        view.handle(mouse(MouseKind::Up(MouseButton::Left), x, y));
    };
    // ` Edit ` is Cells 7 to 12.
    click(&mut view, 8, 0);
    draw_until(&mut view, &mut terminal, &rx, "Find");
    // The pointer over View opens its menu; a press on its row chooses.
    view.handle(mouse(MouseKind::Move, 14, 0));
    draw_until(&mut view, &mut terminal, &rx, "Zoom In");
    let (x, y) = screen(&terminal)
        .lines()
        .enumerate()
        .find_map(|(y, line)| line.find("Zoom").map(|_| (14 + 2, y as u16)))
        .unwrap();
    click(&mut view, x, y);
    draw_until(&mut view, &mut terminal, &rx, "chose zoom");
    // A press on the open menu's title closes it.
    click(&mut view, 2, 0);
    draw_until(&mut view, &mut terminal, &rx, "Open");
    click(&mut view, 2, 0);
    draw_until(&mut view, &mut terminal, &rx, "chose zoom");
    assert!(!screen(&terminal).contains("Open"), "{}", screen(&terminal));
    // Ctrl+O chooses Open with no menu open.
    view.handle(Input::Key {
        key: "o".into(),
        modifiers: Modifiers {
            ctrl: true,
            ..Default::default()
        },
        release: false,
    });
    draw_until(&mut view, &mut terminal, &rx, "chose open");
}
