//! A toast's announcement: read once as it shows, however often the app
//! rebuilds it under another parent (a stack drawn in the topmost modal
//! while one is open).

use nsh_base::{ToastTransitionStatus, toast};
use nsh_core::{Command, prelude::*};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

type Shown = (u32, &'static str, ToastTransitionStatus);

#[component]
fn App() -> impl IntoElement {
    let in_modal = signal(false);
    let shown = signal(vec![(1, "Saved", ToastTransitionStatus::Starting)]);
    // The toasts, under the parent of the layer that draws them.
    let layer = move |modal: bool| {
        when(
            move || in_modal.get() == modal,
            move || {
                view()
                    .child(text(if modal { "Modal layer" } else { "Root layer" }))
                    .child(each(
                        move || shown.get(),
                        |(id, _, _): &Shown| *id,
                        |item: ReadSignal<Shown>, _| {
                            toast()
                                .transition_status(move || item.get().2)
                                .accessibility_label(move || Some(item.get().1.into()))
                                .child(text(move || item.get().1))
                        },
                    ))
            },
        )
    };
    let button = |label: &'static str, press: Box<dyn Fn()>| {
        view().on_press(move || press()).child(text(label))
    };
    view()
        .child(button(
            "Move",
            Box::new(move || in_modal.update(|m| *m = !*m)),
        ))
        .child(button(
            "Enter",
            Box::new(move || {
                shown.update(|s| {
                    for t in s.iter_mut() {
                        t.2 = ToastTransitionStatus::Present;
                    }
                })
            }),
        ))
        .child(button(
            "Again",
            Box::new(move || {
                shown.update(|s| s.push((2, "Saved", ToastTransitionStatus::Starting)))
            }),
        ))
        .child(button(
            "Other",
            Box::new(move || {
                shown.update(|s| s.push((3, "Not saved", ToastTransitionStatus::Starting)))
            }),
        ))
        .child(layer(false))
        .child(layer(true))
}

fn announced(app: &Headless) -> Vec<String> {
    app.screen()
        .commands()
        .iter()
        .filter_map(|(_, c)| match c {
            Command::Announce(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_toast_moved_to_another_parent_is_announced_once() {
    let mut app = Headless::start(EngineConfig::new(nsh_testing::windowed_root::<App>())).unwrap();
    app.wait_for("Root layer").unwrap();
    assert_eq!(announced(&app), ["Saved"]);
    // Moved while it enters: rebuilt under the modal's parent in one update.
    app.press("Move").unwrap();
    app.wait_for("Modal layer").unwrap();
    assert_eq!(announced(&app), ["Saved"], "moved while entering");
    // Moved back once it is present.
    app.press("Enter").unwrap();
    app.press("Move").unwrap();
    app.wait_for("Root layer").unwrap();
    assert_eq!(announced(&app), ["Saved"], "moved once present");
    // A new toast is read, one with the same words too.
    app.press("Again").unwrap();
    app.press("Other").unwrap();
    app.wait_until(
        "both new toasts read",
        std::time::Duration::from_secs(5),
        |s| {
            s.commands()
                .iter()
                .filter(|(_, c)| matches!(c, Command::Announce(_)))
                .count()
                == 3
        },
    )
    .unwrap();
    assert_eq!(announced(&app), ["Saved", "Saved", "Not saved"]);
    // Both still entering when they move: neither is read again.
    app.press("Move").unwrap();
    app.wait_for("Modal layer").unwrap();
    assert_eq!(announced(&app), ["Saved", "Saved", "Not saved"]);
}
