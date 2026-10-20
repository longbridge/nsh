//! The System demo: the environment, the status bar's settings (following
//! the appearance until one is picked), system dialogs and their answers,
//! a toast, a timer and display frames.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    Command, Dialog, EventKind, EventPayload, MountBatch, MountItem, PayloadValue, ROOT_ID,
};
use nsh_engine::Engine;

use crate::common::shows;

/// Presses the parent of the text `label` and returns the dialog it shows.
fn press_for_dialog(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut crate::common::Screen,
    label: &str,
) -> Dialog {
    let text = screen.find(label).expect(label);
    engine.control().dispatch_event(
        screen.press_target(text),
        EventKind::Press,
        EventPayload::None,
    );
    let mut dialog = None;
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
        screen.apply(&batch);
        dialog = dialog.or_else(|| {
            batch.items.iter().find_map(|item| match item {
                MountItem::Command {
                    id: ROOT_ID,
                    command: Command::Dialog(d),
                } => Some((**d).clone()),
                _ => None,
            })
        });
    }
    dialog.unwrap_or_else(|| panic!("{label} shows no dialog"))
}

/// Answers `dialog` with button `index`.
fn answer(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut crate::common::Screen,
    dialog: &Dialog,
    index: i32,
) {
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::Dialog,
        EventPayload::Map(vec![
            ("id", PayloadValue::Number(dialog.id as f64)),
            ("index", PayloadValue::Number(index as f64)),
        ]),
    );
    crate::common::drain(rx, screen);
}

/// The status bar's setting last sent.
fn status_bar(screen: &crate::common::Screen) -> Option<nsh_core::StatusBarConfig> {
    screen
        .commands
        .iter()
        .rev()
        .find_map(|(_, command)| match command {
            Command::StatusBar(config) => Some(*config),
            _ => None,
        })
}

#[test]
fn system_shows_the_environment_and_the_status_bar() {
    let (engine, rx, mut screen) = crate::common::open("System");
    for text in [
        "Environment",
        "Live",
        "390 × 844 pt",
        "0 pt",
        "1.00×",
        "Safe area insets",
        "Bottom",
        "Light scheme",
        "Status bar",
        "barStyle: \"dark-content\"",
        "Alerts and sheets",
        "None yet",
        "Frames and timers",
        "Network and logs",
        "Three posts from jsonplaceholder.typicode.com",
    ] {
        assert!(shows(&screen, text), "{text}: {:?}", screen.texts);
    }
    // The style follows the appearance until one is picked.
    assert_eq!(
        status_bar(&screen).map(|s| s.style),
        Some(nsh_core::StatusBarStyle::Dark)
    );
    crate::common::press(&engine, &rx, &mut screen, "Light");
    assert!(
        shows(&screen, "barStyle: \"light-content\""),
        "{:?}",
        screen.texts
    );
    assert_eq!(
        status_bar(&screen).map(|s| s.style),
        Some(nsh_core::StatusBarStyle::Light)
    );
    crate::common::press(&engine, &rx, &mut screen, "Default");
    assert!(
        shows(&screen, "barStyle: \"default\""),
        "{:?}",
        screen.texts
    );

    // The toggle next to "Hidden" hides it.
    let hidden = screen.find("Hidden").unwrap();
    let row = screen.parents[&screen.parents[&hidden]];
    let switch = *screen
        .parents
        .iter()
        .find(|&(child, parent)| {
            *parent == row
                && screen
                    .events
                    .get(child)
                    .is_some_and(|e| e.contains(EventKind::Press))
        })
        .expect("the toggle")
        .0;
    engine
        .control()
        .dispatch_event(switch, EventKind::Press, EventPayload::None);
    crate::common::drain(&rx, &mut screen);
    assert_eq!(status_bar(&screen).map(|s| s.hidden), Some(true));
}

#[test]
fn system_shows_what_dialogs_answer() {
    let (engine, rx, mut screen) = crate::common::open("System");
    let dialog = press_for_dialog(&engine, &rx, &mut screen, "Confirm");
    assert_eq!(dialog.title.as_deref(), Some("Delete the photo?"));
    assert_eq!(dialog.buttons.len(), 2);
    assert!(dialog.cancelable && !dialog.sheet);
    answer(&engine, &rx, &mut screen, &dialog, 1);
    assert!(shows(&screen, "Delete"), "{:?}", screen.texts);

    let sheet = press_for_dialog(&engine, &rx, &mut screen, "Action sheet");
    assert!(sheet.sheet);
    assert_eq!(sheet.buttons[2].text, "Save");
    answer(&engine, &rx, &mut screen, &sheet, 2);
    assert!(shows(&screen, "Save"), "{:?}", screen.texts);

    // Dismissing the sheet is its cancel action.
    let sheet = press_for_dialog(&engine, &rx, &mut screen, "Action sheet");
    answer(&engine, &rx, &mut screen, &sheet, -1);
    assert!(shows(&screen, "Cancel"), "{:?}", screen.texts);
}

#[test]
fn system_counts_seconds() {
    let (_engine, rx, mut screen) = crate::common::open("System");
    std::thread::sleep(Duration::from_millis(1100));
    crate::common::drain(&rx, &mut screen);
    assert!(
        screen.texts.iter().any(|t| t
            .1
            .strip_suffix(" every 1 s")
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| n >= 1)),
        "{:?}",
        screen.texts
    );
}

#[test]
fn system_shows_a_toast() {
    let (engine, rx, mut screen) = crate::common::open("System");
    let text = screen.find("Toast").expect("the toast's button");
    engine.control().dispatch_event(
        screen.press_target(text),
        EventKind::Press,
        EventPayload::None,
    );
    let mut toast = None;
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
        screen.apply(&batch);
        toast = toast.or_else(|| {
            batch.items.iter().find_map(|item| match item {
                MountItem::Command {
                    id: ROOT_ID,
                    command: Command::Toast { text, long },
                } => Some((text.clone(), *long)),
                _ => None,
            })
        });
    }
    assert_eq!(toast, Some(("Hello from a toast".to_owned(), true)));
}

#[test]
fn system_counts_frames() {
    let (engine, rx, mut screen) = crate::common::open("System");
    assert!(shows(&screen, "– fps stopped"), "{:?}", screen.texts);
    crate::common::press(&engine, &rx, &mut screen, "Count frames");
    assert!(
        shows(&screen, "0 fps live") && shows(&screen, "Stop counting frames"),
        "{:?}",
        screen.texts
    );
    // The platform's display frames, for a little over a second.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(1200) {
        engine.control().frame();
        std::thread::sleep(Duration::from_millis(16));
    }
    crate::common::drain(&rx, &mut screen);
    assert!(
        screen.texts.iter().any(|t| t
            .1
            .strip_suffix(" fps live")
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| n > 0)),
        "{:?}",
        screen.texts
    );
    crate::common::press(&engine, &rx, &mut screen, "Stop counting frames");
    assert!(shows(&screen, "– fps stopped"), "{:?}", screen.texts);
}

#[test]
fn the_network_card_loads_and_shows_how_the_request_went() {
    let (engine, rx, mut screen) = crate::common::open("System");
    assert!(shows(&screen, "fetch()"), "{:?}", screen.texts);
    // Headless, there is no network: the request fails, and says why.
    crate::common::press(&engine, &rx, &mut screen, "fetch()");
    for _ in 0..50 {
        if shows(&screen, "this host has no network stack") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        crate::common::drain(&rx, &mut screen);
    }
    assert!(
        shows(&screen, "this host has no network stack"),
        "{:?}",
        screen.texts
    );
    // Done: the button is back, and pressing it loads again.
    assert!(shows(&screen, "fetch()"), "{:?}", screen.texts);
    crate::common::press(&engine, &rx, &mut screen, "fetch()");
    for _ in 0..50 {
        if shows(&screen, "this host has no network stack") && shows(&screen, "fetch()") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        crate::common::drain(&rx, &mut screen);
    }
    assert!(shows(&screen, "this host has no network stack"));
}
