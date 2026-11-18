//! `AlertDialog`: the app takes an alert back without the user, as when
//! what it asks about went away. Its answer comes at once as closed without
//! an action, and the platform takes it off the screen.

use std::{cell::RefCell, rc::Rc};

use nsh_base::{alert_dialog, button};
use nsh_core::{Size, element::Root, prelude::*};
use nsh_engine::EngineConfig;
use nsh_testing::{DialogAnswer, Headless};

fn start<C: Component<Props = ()>>() -> Headless {
    let config = EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        ..EngineConfig::new(Root::of::<C>())
    };
    let mut app = Headless::start(config).unwrap();
    app.first_render().unwrap();
    app.answer_dialogs_myself();
    app
}

/// Asks to delete; "Close" takes the question back; the answer shows.
#[component]
fn Asking() -> impl IntoElement {
    let answer = signal(String::from("waiting"));
    let closer: Rc<RefCell<Option<DialogCloser>>> = Rc::default();
    view()
        .child(button().test_id("ask").child(text("Delete")).on_press({
            let closer = closer.clone();
            move || {
                let shown = alert_dialog()
                    .title("Delete the photo?")
                    .cancel("cancel", "Cancel")
                    .destructive("delete", "Delete")
                    .show();
                *closer.borrow_mut() = shown.closer();
                spawn(async move {
                    let action = shown.await.map(|r| r.action);
                    answer.set(format!("{action:?}"));
                });
            }
        }))
        .child(
            button()
                .test_id("close")
                .child(text("Close"))
                .on_press(move || {
                    if let Some(closer) = closer.borrow().as_ref() {
                        closer.close();
                    }
                }),
        )
        .child(text(move || answer.get()).test_id("answer"))
}

#[test]
fn the_app_closes_an_alert_it_showed() {
    let mut app = start::<Asking>();
    app.press("id:ask").unwrap();
    let id = app.pending_dialog().expect("the alert").id;
    app.press("id:close").unwrap();
    assert!(app.pending_dialog().is_none(), "no longer up");
    assert!(app.screen().dialog_closed(id));
    assert!(app.screen().shows("Ok(None)"), "answered as closed");
}

#[test]
fn closing_an_answered_alert_does_nothing() {
    let mut app = start::<Asking>();
    app.press("id:ask").unwrap();
    let id = app.pending_dialog().expect("the alert").id;
    app.answer_dialog(DialogAnswer::Press("Delete".into()))
        .unwrap();
    assert!(app.screen().shows("Ok(Some(\"delete\"))"));
    app.press("id:close").unwrap();
    assert!(!app.screen().dialog_closed(id), "nothing to close");
    assert!(app.screen().shows("Ok(Some(\"delete\"))"));
}
