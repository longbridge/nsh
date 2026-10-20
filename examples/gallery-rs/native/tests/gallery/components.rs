//! The Components demo on a headless engine: its cards render, the press
//! feedback tiles answer presses, the sheet and
//! the modal open and close, the keyboard avoiding view's screen pushes,
//! the platform's rows and the screen reader's buttons, the input
//! accessory.

use std::time::Duration;

use nsh_core::{Command, EventKind, EventPayload, MountItem, ROOT_ID, ViewKind};

use crate::common::shows;

#[test]
fn components_render_and_answer_presses() {
    let (engine, rx, mut screen) = crate::common::open("Components");
    for label in [
        "Press feedback",
        "Fades while pressed",
        "activeStyle={{ opacity: 0.4 }}",
        "Enabled",
        "On",
        "Last pressed",
        "None yet",
        "Open a sheet",
        "active:scale-[0.97]",
        "Layout animation",
        "Spring",
        "Ease",
        "0 characters",
        "The bar counts what you type",
        "Keyboard avoidance",
        "Modal options",
        "statusBarTranslucent",
        "false",
        "Platform",
        "Platform.OS",
        "ios",
        "Platform.Version",
        "isPad / isTV / isMacCatalyst",
        "false / false / false",
        "ios branch",
        "PixelRatio.get()",
        "roundToNearestPixel(10.3)",
        "10",
        "reduceMotion()",
        "The line above is one device pixel tall",
        "1.000 pt",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    crate::common::press(&engine, &rx, &mut screen, "Fades while pressed");
    assert!(shows(&screen, "Fading tile"), "{:?}", screen.texts);

    // Layout animation: the boxes 1, 2, 3; Add puts 4 first (the index
    // under the demo shows 4s of its own).
    let fours = |screen: &crate::common::Screen| screen.texts.iter().filter(|t| t.1 == "4").count();
    let before = fours(&screen);
    crate::common::press(&engine, &rx, &mut screen, "Add");
    // The box and the count's badge.
    assert_eq!(fours(&screen), before + 2, "{:?}", screen.texts);

    crate::common::press(&engine, &rx, &mut screen, "Remove");
    assert_eq!(fours(&screen), before, "{:?}", screen.texts);

    crate::common::press(&engine, &rx, &mut screen, "Open the modal");
    assert!(
        shows(
            &screen,
            "The olive background starts below the status bar on Android."
        ) && shows(&screen, "opaque bar"),
        "{:?}",
        screen.texts
    );
    crate::common::press(&engine, &rx, &mut screen, "Close");
    assert!(!shows(&screen, "opaque bar"), "{:?}", screen.texts);
}

#[test]
fn components_open_the_sheet() {
    let (engine, rx, mut screen) = crate::common::open("Components");
    crate::common::press(&engine, &rx, &mut screen, "Open a sheet");
    for label in [
        "pageSheet",
        "A sheet",
        "Swipe down (iOS) or press back (Android) to close.",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    crate::common::press(&engine, &rx, &mut screen, "Close");
    assert!(!shows(&screen, "A sheet"), "{:?}", screen.texts);
}

#[test]
fn components_push_the_keyboard_avoiding_view() {
    let (engine, rx, mut screen) = crate::common::open("Components");
    crate::common::press(&engine, &rx, &mut screen, "KeyboardAvoidingView");
    assert!(
        screen.titles.iter().any(|t| t.1 == "KeyboardAvoidingView"),
        "{:?}",
        screen.titles
    );
    assert!(shows(&screen, "behavior=\"padding\""), "{:?}", screen.texts);
    assert!(
        shows(
            &screen,
            "The bottom pads: the tinted background stays under the keyboard."
        ),
        "{:?}",
        screen.texts
    );
    crate::common::press(&engine, &rx, &mut screen, "height");
    assert!(
        shows(&screen, "behavior=\"height\"")
            && shows(
                &screen,
                "The view shrinks: its background ends at the keyboard."
            ),
        "{:?}",
        screen.texts
    );
}

#[test]
fn components_announce_and_move_the_readers_focus() {
    let (engine, rx, mut screen) = crate::common::open("Components");
    let note = screen
        .find("With VoiceOver or TalkBack on, the buttons above speak and move the reader here.")
        .expect("the note");
    let commands = |label: &str, screen: &mut crate::common::Screen| {
        let text = screen.find(label).expect(label);
        engine.control().dispatch_event(
            screen.press_target(text),
            EventKind::Press,
            EventPayload::None,
        );
        let mut commands = Vec::new();
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
            screen.apply(&batch);
            for item in &batch.items {
                if let MountItem::Command { id, command } = item {
                    commands.push((*id, command.clone()));
                }
            }
        }
        commands
    };
    assert!(
        commands("Announce", &mut screen)
            .contains(&(ROOT_ID, Command::Announce("Announcement 1".into()))),
    );
    assert!(
        commands("Announce", &mut screen)
            .contains(&(ROOT_ID, Command::Announce("Announcement 2".into()))),
    );
    assert!(commands("Focus the note", &mut screen).contains(&(note, Command::AccessibilityFocus)));
}

#[test]
fn components_put_the_accessory_bar_over_the_keyboard() {
    let (engine, rx) = crate::common::start();
    let mut screen = crate::common::Screen::default();
    let mut kinds = Vec::new();
    let mut collect = |screen: &mut crate::common::Screen| {
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
            screen.apply(&batch);
            for item in &batch.items {
                if let MountItem::Create { id, kind, .. } = item {
                    kinds.push((*id, *kind));
                }
            }
        }
    };
    collect(&mut screen);
    let text = screen.find("Components").expect("the index's row");
    engine.control().dispatch_event(
        screen.press_target(text),
        EventKind::Press,
        EventPayload::None,
    );
    collect(&mut screen);
    // The bar is an input accessory (iOS): its content is mounted in it.
    let accessory = kinds
        .iter()
        .find(|(_, kind)| *kind == ViewKind::InputAccessory)
        .map(|(id, _)| *id)
        .unwrap_or_else(|| panic!("no input accessory: {kinds:?}"));
    let counter = screen.find("0 characters").expect("the counter");
    let mut parent = screen.parents[&counter];
    while parent != accessory {
        parent = *screen
            .parents
            .get(&parent)
            .unwrap_or_else(|| panic!("the counter is not in the accessory"));
    }
}
