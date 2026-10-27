use nsh_core::prelude::*;
use nsh_platform_tui::{App, Handled, Input, Modifiers, Root};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

use super::app::{draw_until, surface};

fn key(name: &str, modifiers: Modifiers) -> Input {
    Input::Key {
        key: name.into(),
        modifiers,
        release: false,
    }
}
fn ctrl(name: &str) -> Input {
    key(
        name,
        Modifiers {
            ctrl: true,
            ..Default::default()
        },
    )
}

#[component]
fn FocusPage() -> impl IntoElement {
    let focused = signal("none");
    let pressed = signal(String::new());
    let button = move |label: &'static str, x, y| {
        view()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(64.0))
            .h(px(16.0))
            .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
            .on_focus(move |_: &()| focused.set(label))
            .on_press_down(move |_: &()| pressed.update(|s| s.push('D')))
            .on_press_up(move |_: &()| pressed.update(|s| s.push('U')))
            .on_press(move |_: &()| pressed.update(|s| s.push('P')))
            .child(text(label))
    };
    view()
        .flex_1()
        .child(button("B", 80.0, 16.0))
        .child(button("A", 0.0, 16.0))
        .child(button("C", 0.0, 48.0))
        .child(
            text(move || format!("focus {} press {}", focused.get(), pressed.get()))
                .absolute()
                .top(px(80.0)),
        )
}

#[test]
fn terminal_keys_move_focus_in_reading_order_and_enter_presses_it() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(App::Rust(Root::new::<FocusPage>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "focus none");
    for (input, focused, cursor) in [
        (ctrl("n"), "A", (0, 1)),
        (key("Right", Default::default()), "B", (10, 1)),
        (key("Down", Default::default()), "C", (0, 3)),
        (ctrl("p"), "B", (10, 1)),
        (key("Left", Default::default()), "A", (0, 1)),
        (key("Tab", Default::default()), "B", (10, 1)),
        (
            key(
                "Tab",
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            ),
            "A",
            (0, 1),
        ),
    ] {
        assert_eq!(view.handle(input), Handled::Yes);
        draw_until(&mut view, &mut terminal, &rx, &format!("focus {focused}"));
        assert!(!terminal.backend().cursor_visible());
        assert_eq!(
            terminal.backend().buffer()[cursor].bg,
            Color::Rgb(0x12, 0x34, 0x56)
        );
    }
    assert_eq!(view.handle(key("Enter", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "press DUP");
    assert_eq!(
        view.handle(Input::Key {
            key: "Enter".into(),
            modifiers: Default::default(),
            release: true
        }),
        Handled::Ignored
    );
    view.set_focused(false);
    assert_eq!(view.handle(ctrl("n")), Handled::Ignored);
}

#[component]
fn ShortcutPage() -> impl IntoElement {
    let shortcuts = signal(0);
    let focus_count = signal(0);
    on_key("ctrl+n", move |_| shortcuts.update(|n| *n += 1));
    view()
        .flex_1()
        .child(
            view()
                .h(px(16.0))
                .on_press(|_: &()| {})
                .on_focus(move |_: &()| focus_count.update(|n| *n += 1))
                .child(text("button")),
        )
        .child(text(move || {
            format!("shortcuts {} focus {}", shortcuts.get(), focus_count.get())
        }))
}

#[test]
fn app_shortcuts_win_over_the_terminal_scheme() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(App::Rust(Root::new::<ShortcutPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "shortcuts 0 focus 0");
    assert_eq!(view.handle(ctrl("n")), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "shortcuts 1 focus 0");
    assert_eq!(view.handle(key("Tab", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "shortcuts 1 focus 1");
}

#[component]
fn ScrollKeys() -> impl IntoElement {
    let target = element_ref();
    let offset = signal(0);
    let chosen = signal(-1);
    on_key("ctrl+r", move |_| {
        target.scroll_to(nsh_core::Point::default(), false)
    });
    view()
        .flex_1()
        .child(text(move || format!("offset {} chosen {}", offset.get(), chosen.get())).h(px(16.0)))
        .child(
            scroll()
                .element_ref(&target)
                .w(px(160.0))
                .h(px(32.0))
                .on_scroll(move |event: &ScrollEvent| offset.set(event.offset.y.0 as i32))
                .children((0..6).map(|index| {
                    view()
                        .h(px(16.0))
                        .flex_shrink_0()
                        .on_press(move |_: &()| chosen.set(index))
                        .child(text(format!("Item{index}")))
                })),
        )
}

#[test]
fn focus_reveals_scroll_content_and_page_keys_keep_the_core_offset_protocol() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(App::Rust(Root::new::<ScrollKeys>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "offset 0 chosen -1");
    for _ in 0..3 {
        assert_eq!(view.handle(ctrl("n")), Handled::Yes);
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
    }
    assert_eq!(view.handle(key("Enter", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "chosen 2");
    assert!((1..3).any(|row| terminal.backend().buffer()[(0, row)].symbol() == "I"));
    for (input, expected) in [
        (key("Home", Default::default()), "offset 0"),
        (ctrl("v"), "offset 32"),
        (key("PageUp", Default::default()), "offset 0"),
        (
            key(
                ">",
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
            ),
            "offset 64",
        ),
        (
            key(
                "<",
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
            ),
            "offset 0",
        ),
        (key("PageDown", Default::default()), "offset 32"),
        (
            key(
                "v",
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
            ),
            "offset 0",
        ),
        (key("End", Default::default()), "offset 64"),
        (ctrl("r"), "offset 0"),
    ] {
        assert_eq!(view.handle(input), Handled::Yes);
        // Render before looking for a value that may equal the preceding frame's.
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        draw_until(&mut view, &mut terminal, &rx, expected);
    }
}

#[component]
fn BackPage() -> impl IntoElement {
    let requests = signal(0);
    let dismisses = signal(0);
    let prevent = signal(true);
    view()
        .flex_1()
        .child(
            text(move || {
                format!(
                    "back {} dismiss {} prevent {}",
                    requests.get(),
                    dismisses.get(),
                    prevent.get()
                )
            })
            .h(px(16.0)),
        )
        .child(
            nsh_base::nav_stack()
                .w(px(160.0))
                .h(px(64.0))
                .child(nsh_base::nav_page().key("home").child(text("home")))
                .child(
                    nsh_base::nav_page()
                        .key("top")
                        .pop_on_back(move || !prevent.get())
                        .on_back_press(move |_: &()| requests.update(|n| *n += 1))
                        .on_dismiss(move |_: &()| dismisses.update(|n| *n += 1))
                        .child(
                            view()
                                .h(px(16.0))
                                .on_press(move |_: &()| prevent.set(false))
                                .child(text("allow back")),
                        ),
                ),
        )
}

#[test]
fn cancel_reports_back_requests_respects_prevent_back_and_dismisses_once() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(App::Rust(Root::new::<BackPage>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "allow back");
    assert_eq!(view.handle(key("Escape", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "back 1 dismiss 0");
    assert_eq!(view.handle(ctrl("n")), Handled::Yes);
    assert_eq!(view.handle(key("Enter", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "prevent false");
    assert_eq!(view.handle(ctrl("g")), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "back 2 dismiss 1");
    draw_until(&mut view, &mut terminal, &rx, "home");
    assert_eq!(view.handle(ctrl("g")), Handled::Ignored);
}

#[component]
fn RootBack() -> impl IntoElement {
    let intercepted = signal(0);
    on_back_press(move || {
        intercepted.update(|n| *n += 1);
        true
    });
    view()
        .flex_1()
        .child(text(move || format!("intercepted {}", intercepted.get())).h(px(16.0)))
        .child(
            nsh_base::nav_stack()
                .w(px(160.0))
                .h(px(64.0))
                .child(nsh_base::nav_page().child(text("home")))
                .child(nsh_base::nav_page().child(text("still top"))),
        )
}

#[test]
fn the_app_back_handler_wins_before_a_stack_pop() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(App::Rust(Root::new::<RootBack>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "still top");
    assert_eq!(view.handle(key("Escape", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "intercepted 1");
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
            .contains("still top")
    );
}

#[component]
fn ModalKeys() -> impl IntoElement {
    let root_pressed = signal(0);
    let modal_pressed = signal(0);
    let cancelled = signal(0);
    let dismissed = signal(0);
    let close = signal(false);
    view()
        .flex_1()
        .child(
            view()
                .h(px(16.0))
                .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
                .on_press(move |_: &()| root_pressed.update(|n| *n += 1))
                .child(text("root button")),
        )
        .child(text(move || {
            format!(
                "root {} cancelled {} dismissed {}",
                root_pressed.get(),
                cancelled.get(),
                dismissed.get()
            )
        }))
        .child(
            nsh_base::dialog()
                .close_on_escape(close)
                .on_cancel(move |_: &()| cancelled.update(|n| *n += 1))
                .on_close(move |_: &()| dismissed.update(|n| *n += 1))
                .child(
                    view()
                        .absolute()
                        .top(px(48.0))
                        .h(px(16.0))
                        .w(px(160.0))
                        .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
                        .on_press(move |_: &()| {
                            modal_pressed.update(|n| *n += 1);
                            close.set(true);
                        })
                        .child(text("modal button")),
                )
                .child(
                    text(move || {
                        format!("modal {} cancel {}", modal_pressed.get(), cancelled.get())
                    })
                    .absolute()
                    .top(px(80.0)),
                ),
        )
}

#[test]
fn modal_focus_is_scoped_and_cancel_honors_close_on_escape() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(50, 10)).unwrap();
    view.load(App::Rust(Root::new::<ModalKeys>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "modal button");
    assert_eq!(view.handle(key("Escape", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "modal 0 cancel 1");
    assert_eq!(view.handle(ctrl("n")), Handled::Yes);
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    draw_focused_cell(&mut view, &mut terminal, &rx, (0, 3));
    assert!(!terminal.backend().cursor_visible());
    assert_eq!(view.handle(key("Enter", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "modal 1 cancel 1");
    assert_eq!(view.handle(ctrl("g")), Handled::Yes);
    draw_until(
        &mut view,
        &mut terminal,
        &rx,
        "root 0 cancelled 2 dismissed 1",
    );
    assert_eq!(view.handle(key("Tab", Default::default())), Handled::Yes);
    assert_eq!(view.handle(key("Enter", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "root 1");
}

#[test]
fn a_known_key_repeat_does_not_refire_a_nonrepeating_app_shortcut() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(App::Rust(Root::new::<ShortcutPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "shortcuts 0 focus 0");
    view.handle(ctrl("n"));
    draw_until(&mut view, &mut terminal, &rx, "shortcuts 1 focus 0");
    assert_eq!(
        view.handle(Input::KeyRepeat {
            key: "n".into(),
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            }
        }),
        Handled::Yes
    );
    view.handle(key("Tab", Default::default()));
    draw_until(&mut view, &mut terminal, &rx, "shortcuts 1 focus 1");
}

#[component]
fn EditableFocus() -> impl IntoElement {
    let focus = signal("none");
    view()
        .flex_1()
        .child(
            view()
                .h(px(16.0))
                .disabled(true)
                .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
                .on_press(|_: &()| {})
                .child(text("disabled")),
        )
        .child(
            nsh_base::input()
                .h(px(32.0))
                .on_focus(move |_: &()| focus.set("input")),
        )
        .child(
            view()
                .h(px(16.0))
                .focusable(false)
                .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
                .on_press(|_: &()| {})
                .child(text("skip")),
        )
        .child(
            view()
                .h(px(16.0))
                .on_focus(move |_: &()| focus.set("button"))
                .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
                .on_press(|_: &()| {})
                .child(text("button")),
        )
        .child(text(move || format!("focus {0}", focus.get())))
}

#[test]
fn focus_visits_editable_fields_and_skips_disabled_or_opted_out_views() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(App::Rust(Root::new::<EditableFocus>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "focus none");
    view.handle(ctrl("n"));
    draw_until(&mut view, &mut terminal, &rx, "focus input");
    terminal.backend_mut().assert_cursor_position((0, 1));
    assert_eq!(
        view.handle(key("Enter", Default::default())),
        Handled::Ignored
    );
    view.handle(key("Down", Default::default()));
    draw_until(&mut view, &mut terminal, &rx, "focus button");
    assert!(!terminal.backend().cursor_visible());
}

#[component]
fn NestedBack() -> impl IntoElement {
    let app = signal(0);
    let outer = signal(0);
    let inner = signal(0);
    on_back_press(move || {
        app.update(|n| *n += 1);
        false
    });
    view()
        .flex_1()
        .child(
            text(move || format!("app{} outer{} inner{}", app.get(), outer.get(), inner.get()))
                .h(px(16.0)),
        )
        .child(
            nsh_base::nav_stack()
                .w(px(160.0))
                .h(px(96.0))
                .child(
                    nsh_base::nav_page()
                        .key("outer-base")
                        .child(text("outer base")),
                )
                .child(
                    nsh_base::nav_page()
                        .key("outer-top")
                        .on_dismiss(move |_: &()| outer.update(|n| *n += 1))
                        .child(text("outer top").h(px(16.0)))
                        .child(
                            nsh_base::nav_stack()
                                .w(px(160.0))
                                .h(px(64.0))
                                .child(
                                    nsh_base::nav_page()
                                        .key("inner-base")
                                        .child(text("inner base")),
                                )
                                .child(
                                    nsh_base::nav_page()
                                        .key("inner-top")
                                        .on_dismiss(move |_: &()| inner.update(|n| *n += 1))
                                        .child(text("inner top")),
                                ),
                        ),
                ),
        )
}

#[test]
fn an_unhandled_app_back_request_pops_the_innermost_stack_then_its_parent() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(App::Rust(Root::new::<NestedBack>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "inner top");
    view.handle(ctrl("g"));
    draw_until(&mut view, &mut terminal, &rx, "app1 outer0 inner1");
    draw_until(&mut view, &mut terminal, &rx, "inner base");
    view.handle(key("Escape", Default::default()));
    draw_until(&mut view, &mut terminal, &rx, "app2 outer1 inner1");
    draw_until(&mut view, &mut terminal, &rx, "outer base");
}

#[component]
fn TwoModals() -> impl IntoElement {
    let closed = signal(0);
    view()
        .flex_1()
        .child(text(move || format!("closed {}", closed.get())))
        .child(
            nsh_base::dialog().child(
                view()
                    .absolute()
                    .top(px(48.0))
                    .w(px(80.0))
                    .h(px(16.0))
                    .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
                    .on_press(|_: &()| {})
                    .child(text("first")),
            ),
        )
        .child(
            nsh_base::dialog()
                .on_close(move |_: &()| closed.set(1))
                .child(
                    view()
                        .absolute()
                        .top(px(80.0))
                        .w(px(80.0))
                        .h(px(16.0))
                        .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
                        .on_press(|_: &()| {})
                        .child(text("second")),
                ),
        )
}

#[test]
fn only_the_newest_modal_supplies_focus_candidates() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(App::Rust(Root::new::<TwoModals>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "second");
    view.handle(ctrl("n"));
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    draw_focused_cell(&mut view, &mut terminal, &rx, (0, 5));
    assert!(!terminal.backend().cursor_visible());
    view.handle(ctrl("g"));
    draw_until(&mut view, &mut terminal, &rx, "closed 1");
    view.handle(ctrl("n"));
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    draw_focused_cell(&mut view, &mut terminal, &rx, (0, 3));
    assert!(!terminal.backend().cursor_visible());
}

#[cfg(feature = "js")]
#[test]
fn enter_reports_a_press_at_the_focused_views_middle() {
    let code = nsh_bundler::compile_module("App.tsx", r#"
        import { mount, signal, View, Text } from "nsh";
        const result = signal("waiting");
        mount(() => <View style={{ flex: 1 }}>
            <View style={{ position: "absolute", left: 16, top: 16, width: 80, height: 32 }}
                onPress={(e) => result.set(`${e.position.x},${e.position.y} at ${e.pagePosition.x},${e.pagePosition.y}`)}>
                <Text>button</Text>
            </View>
            <Text style={{ position: "absolute", top: 80 }}>{result()}</Text>
        </View>);
    "#, &[]).unwrap();
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(nsh_platform_tui::Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "waiting");
    view.handle(ctrl("n"));
    assert_eq!(view.handle(key("Enter", Default::default())), Handled::Yes);
    draw_until(&mut view, &mut terminal, &rx, "40,16 at 56,32");
}

#[component]
fn LaterModal() -> impl IntoElement {
    let later = signal(false);
    view()
        .flex_1()
        .child(when(later, || {
            nsh_base::dialog().child(
                view()
                    .absolute()
                    .top(px(80.0))
                    .w(px(80.0))
                    .h(px(16.0))
                    .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
                    .on_press(|_: &()| {})
                    .child(text("new modal")),
            )
        }))
        .child(
            nsh_base::dialog().child(
                view()
                    .absolute()
                    .top(px(48.0))
                    .w(px(80.0))
                    .h(px(16.0))
                    .focus(|style| style.bg(nsh_core::color::rgb(0x123456)))
                    .on_press(move |_: &()| later.set(true))
                    .child(text("open modal")),
            ),
        )
}

#[test]
fn a_later_modal_takes_focus_even_when_inserted_before_the_existing_one() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(App::Rust(Root::new::<LaterModal>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "open modal");
    view.handle(ctrl("n"));
    view.handle(key("Enter", Default::default()));
    draw_until(&mut view, &mut terminal, &rx, "new modal");
    view.handle(ctrl("n"));
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    draw_focused_cell(&mut view, &mut terminal, &rx, (0, 5));
    assert!(!terminal.backend().cursor_visible());
}

#[cfg(feature = "dev")]
#[test]
fn the_diagnostics_panel_owns_repeated_keys_and_preserves_app_focus() {
    let path = std::env::temp_dir().join(format!(
        "nsh-keyboard-diagnostics-{}.log",
        std::process::id()
    ));
    std::fs::write(
        &path,
        (0..30)
            .map(|line| format!("Log{line:02}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let (mut view, rx) = surface();
    view.set_dev_log_path(path.clone());
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    view.load(App::Rust(Root::new::<FocusPage>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "focus none");
    view.handle(ctrl("n"));
    draw_until(&mut view, &mut terminal, &rx, "focus A");
    view.handle(ctrl("h"));
    view.handle(key("e", Default::default()));
    draw_until(&mut view, &mut terminal, &rx, "Logs and errors");
    view.handle(key("Home", Default::default()));
    draw_until(&mut view, &mut terminal, &rx, "Log00");
    view.handle(Input::KeyRepeat {
        key: "Down".into(),
        modifiers: Default::default(),
    });
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    assert_eq!(
        (0..5)
            .map(|x| terminal.backend().buffer()[(x, 1)].symbol())
            .collect::<String>(),
        "Log01"
    );
    view.handle(key("Escape", Default::default()));
    draw_until(&mut view, &mut terminal, &rx, "focus A");
    assert!(!terminal.backend().cursor_visible());
    assert_eq!(
        terminal.backend().buffer()[(0, 1)].bg,
        Color::Rgb(0x12, 0x34, 0x56)
    );
    view.stop();
    std::fs::remove_file(path).unwrap();
}

fn draw_focused_cell(
    view: &mut nsh_platform_tui::AppView,
    terminal: &mut Terminal<TestBackend>,
    rx: &crossbeam_channel::Receiver<()>,
    at: (u16, u16),
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if terminal.backend().buffer()[at].bg == Color::Rgb(0x12, 0x34, 0x56) {
            return;
        }
        rx.recv_deadline(deadline).expect("app focus style");
    }
}

#[component]
fn CursorPage() -> impl IntoElement {
    let focused = signal("none");
    view()
        .flex_1()
        .child(
            view()
                .h(px(16.0))
                .on_focus(move |_: &()| focused.set("button"))
                .on_press(|_: &()| {})
                .child(text("button")),
        )
        .child(
            nsh_base::input()
                .h(px(32.0))
                .on_focus(move |_: &()| focused.set("field")),
        )
        .child(
            nsh_base::input()
                .h(px(32.0))
                .caret(false)
                .on_focus(move |_: &()| focused.set("hidden caret")),
        )
        .child(text(move || format!("focused {}", focused.get())))
}

#[test]
fn only_an_editable_field_with_a_visible_caret_shows_the_terminal_cursor() {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    view.load(App::Rust(Root::new::<CursorPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "focused none");
    view.handle(ctrl("n"));
    draw_until(&mut view, &mut terminal, &rx, "focused button");
    assert!(!terminal.backend().cursor_visible());
    view.handle(ctrl("n"));
    draw_until(&mut view, &mut terminal, &rx, "focused field");
    assert!(terminal.backend().cursor_visible());
    terminal.backend_mut().assert_cursor_position((0, 1));
    view.handle(ctrl("n"));
    draw_until(&mut view, &mut terminal, &rx, "focused hidden caret");
    assert!(!terminal.backend().cursor_visible());
}
