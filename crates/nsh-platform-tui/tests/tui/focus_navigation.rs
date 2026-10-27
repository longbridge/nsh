use nsh_core::prelude::*;
use nsh_platform_tui::{App, AppView, Handled, Input, Modifiers, Root};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

use super::app::{draw_until, surface};

fn key(view: &mut AppView, name: &str, ctrl: bool) -> Handled {
    view.handle(Input::Key {
        key: name.into(),
        modifiers: Modifiers {
            ctrl,
            ..Default::default()
        },
        release: false,
    })
}

#[component]
fn ReturningPage() -> impl IntoElement {
    let detail = signal(false);
    let focused = signal("none");
    let opens = signal(0);
    let visible = signal(true);
    on_key("ctrl+d", move |_| visible.set(false));
    view()
        .flex_1()
        .child(
            text(move || {
                format!(
                    "focus {} opens {} visible {}",
                    focused.get(),
                    opens.get(),
                    visible.get()
                )
            })
            .h(px(16.0)),
        )
        .child(
            nsh_base::nav_stack()
                .w(px(240.0))
                .h(px(96.0))
                .child(
                    nsh_base::nav_page()
                        .key("home")
                        .child(
                            view()
                                .h(px(16.0))
                                .on_focus(move |_: &()| focused.set("A"))
                                .on_press(|_: &()| {})
                                .child(text("home A")),
                        )
                        .child(
                            view()
                                .h(px(16.0))
                                .opacity(move || if visible.get() { 1.0_f32 } else { 0.0_f32 })
                                .on_focus(move |_: &()| focused.set("B"))
                                .on_press(move |_: &()| {
                                    opens.update(|n| *n += 1);
                                    detail.set(true);
                                })
                                .child(text("open detail")),
                        ),
                )
                .child(when(detail, move || {
                    nsh_base::nav_page()
                        .key("detail")
                        .on_dismiss(move |_: &()| detail.set(false))
                        .child(
                            view()
                                .h(px(16.0))
                                .on_focus(move |_: &()| focused.set("detail"))
                                .on_press(move |_: &()| detail.set(false))
                                .child(text("detail button")),
                        )
                })),
        )
}

fn detail_page() -> (
    AppView,
    Terminal<TestBackend>,
    crossbeam_channel::Receiver<()>,
) {
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    view.load(App::Rust(Root::new::<ReturningPage>(|| ())))
        .unwrap();
    draw_until(&mut view, &mut terminal, &rx, "home A");
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus A");
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus B");
    key(&mut view, "Enter", false);
    draw_until(&mut view, &mut terminal, &rx, "detail button");
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus detail");
    (view, terminal, rx)
}

#[test]
fn returning_to_a_screen_restores_the_control_that_opened_the_next_page() {
    for close in ["Escape", "Enter"] {
        let (mut view, mut terminal, rx) = detail_page();
        key(&mut view, close, false);
        draw_until(&mut view, &mut terminal, &rx, "focus B");
        key(&mut view, "Enter", false);
        draw_until(&mut view, &mut terminal, &rx, "opens 2");
        draw_until(&mut view, &mut terminal, &rx, "detail button");
    }
}

#[test]
fn returning_to_a_screen_does_not_restore_an_invisible_control() {
    let (mut view, mut terminal, rx) = detail_page();
    key(&mut view, "d", true);
    draw_until(&mut view, &mut terminal, &rx, "visible false");
    key(&mut view, "Escape", false);
    draw_until(&mut view, &mut terminal, &rx, "home A");
    assert_eq!(key(&mut view, "Enter", false), Handled::Ignored);
    key(&mut view, "Tab", false);
    draw_until(&mut view, &mut terminal, &rx, "focus A");
}
