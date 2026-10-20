//! The Storage demo and the demo the gallery opens at launch: what one run
//! stores, the next reads back (from `storage.json` in the app's data
//! directory).

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crossbeam_channel::Receiver;
use nsh_core::{
    Command, EventKind, EventPayload, MountBatch, MountItem, PayloadValue, ROOT_ID, Size,
};
use nsh_engine::Engine;

use crate::common::{Screen, drain, press, shows, start_with};

const PHONE: Size = Size::new(390.0, 844.0);
const WINDOW: Size = Size::new(1200.0, 800.0);

/// An empty data directory of the test's own, under the temporary
/// directory (never the user's).
fn data_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gallery-rs-storage-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The gallery keeping its data in `dir`, once it has settled.
fn run(viewport: Size, dir: &Path) -> (Engine, Receiver<MountBatch>, Screen) {
    let (engine, rx) = start_with(viewport, Some(dir.to_path_buf()));
    let mut screen = Screen::default();
    drain(&rx, &mut screen);
    (engine, rx, screen)
}

/// The note's input: the view listening to text changes.
fn note_input(screen: &Screen) -> u32 {
    screen
        .events
        .iter()
        .find(|(_, events)| events.contains(EventKind::ChangeText))
        .map(|(id, _)| *id)
        .expect("the note's input")
}

/// Types `text` into the note.
fn type_note(engine: &Engine, rx: &Receiver<MountBatch>, screen: &mut Screen, text: &str) {
    engine.control().dispatch_event(
        note_input(screen),
        EventKind::ChangeText,
        EventPayload::Text {
            text: text.into(),
            count: 1,
        },
    );
    drain(rx, screen);
}

/// Presses "Clear all" and answers its dialog with button `index`.
fn clear_all(engine: &Engine, rx: &Receiver<MountBatch>, screen: &mut Screen, index: i32) {
    let text = screen.find("Clear all").expect("Clear all");
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
    let dialog = dialog.expect("Clear all asks first");
    assert_eq!(dialog.title.as_deref(), Some("Clear the storage?"));
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::Dialog,
        EventPayload::Map(vec![
            ("id", PayloadValue::Number(dialog.id as f64)),
            ("index", PayloadValue::Number(index as f64)),
        ]),
    );
    drain(rx, screen);
}

#[test]
fn storage_shows_launches_the_note_and_the_keys() {
    let (engine, rx, mut screen) = crate::common::open("Storage");
    for label in [
        "Launches",
        "Kept",
        "1 opening",
        "set_item(\"launches\", \"1\")",
        "Note",
        "0 chars",
        "Stored keys",
        "launches",
        "1 chars",
        // The gallery's own: the demo open.
        "lastDemo",
        "7 chars",
        "2 keys",
        "24 chars of 5 MB",
        "Clear all",
        "The data stays on this device and survives restarts, under the app's id from nsh.json.",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    assert!(!shows(&screen, "Saved"), "{:?}", screen.texts);
    // The note is saved as it is typed, and listed.
    type_note(&engine, &rx, &mut screen, "Hello");
    for label in ["Saved", "5 chars", "note", "3 keys", "33 chars of 5 MB"] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    // Emptied: the key goes.
    type_note(&engine, &rx, &mut screen, "");
    assert!(!shows(&screen, "note"), "{:?}", screen.texts);
    type_note(&engine, &rx, &mut screen, "Hello");
    // Cancelled: everything stays.
    clear_all(&engine, &rx, &mut screen, 0);
    assert!(shows(&screen, "note"), "{:?}", screen.texts);
    // Cleared: nothing is left.
    clear_all(&engine, &rx, &mut screen, 1);
    for label in ["Nothing stored", "0 keys", "0 openings", "0 chars"] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    for label in ["launches", "note", "lastDemo", "Saved"] {
        assert!(!shows(&screen, label), "{label}: {:?}", screen.texts);
    }
}

#[test]
fn what_one_run_stores_the_next_reads_back() {
    let dir = data_dir("phone");
    {
        let (engine, rx, mut screen) = run(PHONE, &dir);
        press(&engine, &rx, &mut screen, "Storage");
        assert!(shows(&screen, "1 opening"), "{:?}", screen.texts);
        type_note(&engine, &rx, &mut screen, "Kept");
    }
    // Written in the app's directory when the engine stopped.
    let file = dir
        .join("com.longbridge.nsh.galleryrs")
        .join("storage.json");
    let kept: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).expect("storage.json")).unwrap();
    assert_eq!(kept["note"], "Kept");
    assert_eq!(kept["lastDemo"], "Storage");
    // The next run opens the demo again: one launch more, the note back.
    let (_engine, _rx, screen) = run(PHONE, &dir);
    assert!(
        screen.titles.iter().any(|t| t.1 == "Storage"),
        "{:?}",
        screen.titles
    );
    for label in ["2 openings", "4 chars", "note"] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    drop(_engine);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_sidebar_opens_the_demo_last_selected() {
    let dir = data_dir("desktop");
    const PHOTOS: &str = "A long recycled list: images, pull to refresh, paging";
    {
        let (engine, rx, mut screen) = run(WINDOW, &dir);
        press(&engine, &rx, &mut screen, "Photos");
        assert!(shows(&screen, PHOTOS), "{:?}", screen.texts);
    }
    let (engine, _rx, screen) = run(WINDOW, &dir);
    assert!(shows(&screen, PHOTOS), "{:?}", screen.texts);
    drop(engine);
    // A name no demo has: the first demo.
    let file = dir
        .join("com.longbridge.nsh.galleryrs")
        .join("storage.json");
    std::fs::write(&file, r#"{"lastDemo":"Nowhere"}"#).unwrap();
    let (engine, _rx, screen) = run(WINDOW, &dir);
    assert!(shows(&screen, "Composite"), "{:?}", screen.texts);
    drop(engine);
    // Opened once at launch.
    std::fs::write(&file, r#"{"lastDemo":"Storage"}"#).unwrap();
    let (engine, _rx, screen) = run(WINDOW, &dir);
    assert!(shows(&screen, "1 opening"), "{:?}", screen.texts);
    drop(engine);
    let _ = std::fs::remove_dir_all(&dir);
}
