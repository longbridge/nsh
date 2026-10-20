//! The Mobile UI demo on a headless engine: the header's components and
//! the first rows of the `ListView` render, the header's buttons and
//! steppers change its state, which stays while the list scrolls far
//! away, and the header asks a stepper for its value through its ref.

use nsh_core::{EventKind, EventPayload};

use crate::common::{drain, shows};

#[test]
fn mobile_ui_renders_and_answers_presses() {
    let (engine, rx, mut screen) = crate::common::open("Mobile UI");
    for label in [
        "Rust",
        "nsh-ui-mobile",
        "TextField",
        "Name",
        "Hello, Ada",
        "× clears and focuses",
        "SelectList",
        "500 rows of Rust data",
        "none",
        "City 1",
        "Settings",
        "Notifications",
        "Controlled: 2",
        "Ask the stepper, then reset it",
        "Tap (0)",
        "Inset rows",
        "variant=\"plain\"",
        "Below: a ListView of 300 rows the app renders, recycled natively",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    // The first rows below the header (a screen and more down).
    let list = screen
        .scroll_of(screen.find("Notifications").unwrap())
        .expect("the list");
    engine.control().scroll(
        list,
        nsh_core::geometry::point(nsh_core::geometry::px(0.0), nsh_core::geometry::px(1200.0)),
        u32::MAX,
    );
    drain(&rx, &mut screen);
    for label in ["SYM000", "No. 1", "-2.50%"] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    engine.control().scroll(
        list,
        nsh_core::geometry::point(nsh_core::geometry::px(0.0), nsh_core::geometry::px(0.0)),
        u32::MAX,
    );
    drain(&rx, &mut screen);
    // The header's buttons are nested components: their events change the
    // header's state.
    crate::common::press(&engine, &rx, &mut screen, "Tap (0)");
    crate::common::press(&engine, &rx, &mut screen, "Tap (1)");
    assert!(shows(&screen, "Tap (2)"), "{:?}", screen.texts);
    let taps = screen.find("Taps").unwrap();
    let row = screen.parents[&taps];
    let count = screen
        .texts
        .iter()
        .find(|t| t.0 != taps && screen.parents.get(&t.0) == Some(&row))
        .map(|t| t.1.clone());
    assert_eq!(count.as_deref(), Some("2"), "{:?}", screen.texts);

    // The select list's rows are the select list's own.
    crate::common::press(&engine, &rx, &mut screen, "City 3");
    assert!(shows(&screen, "City 3"), "{:?}", screen.texts);
    assert!(
        !shows(&screen, "none"),
        "the badge names it: {:?}",
        screen.texts
    );

    // The stepper with its own value: one step up, then asked for its
    // value and reset through its ref.
    // (The index under the demo shows a 5 of its own.)
    let (five, plus) = screen
        .texts
        .iter()
        .filter(|t| t.1 == "5")
        .find_map(|five| {
            let stepper = screen.parents[&five.0];
            let plus = screen
                .texts
                .iter()
                .filter(|t| t.1 == "+")
                .map(|t| screen.parents[&t.0])
                .find(|button| screen.parents.get(button) == Some(&stepper))?;
            Some((five.0, plus))
        })
        .expect("the own stepper's value and its + button");
    // Released at once: `drain` waits for quiet, and on a slow machine a
    // press held through it outlasts the hold (400ms) and repeats.
    for kind in [EventKind::PressIn, EventKind::PressOut] {
        engine
            .control()
            .dispatch_event(plus, kind, EventPayload::None);
    }
    drain(&rx, &mut screen);
    assert_eq!(
        screen
            .texts
            .iter()
            .find(|t| t.0 == five)
            .map(|t| t.1.as_str()),
        Some("6")
    );
    crate::common::press(&engine, &rx, &mut screen, "Ask the stepper, then reset it");
    assert!(shows(&screen, "It was 6; reset to 5"), "{:?}", screen.texts);
    assert_eq!(
        screen
            .texts
            .iter()
            .find(|t| t.0 == five)
            .map(|t| t.1.as_str()),
        Some("5")
    );

    // The header is the list's own (not a recycled row): its state stays
    // while the list scrolls far away and back.
    engine.control().scroll(
        list,
        nsh_core::geometry::point(nsh_core::geometry::px(0.0), nsh_core::geometry::px(12000.0)),
        u32::MAX,
    );
    drain(&rx, &mut screen);
    assert!(
        screen.texts.iter().any(|t| t.1.starts_with("SYM2")),
        "rows far down: {:?}",
        screen.texts
    );
    engine.control().scroll(
        list,
        nsh_core::geometry::point(nsh_core::geometry::px(0.0), nsh_core::geometry::px(0.0)),
        u32::MAX,
    );
    drain(&rx, &mut screen);
    assert!(shows(&screen, "It was 6; reset to 5"), "{:?}", screen.texts);

    // The toggle in the row switches the list's variant (its press target
    // is the row's sibling of the texts).
    let label = screen.find("Inset rows").unwrap();
    let row = screen.parents[&screen.parents[&label]];
    let toggle = screen
        .events
        .iter()
        .filter(|(_, e)| e.contains(EventKind::Press))
        .map(|(id, _)| *id)
        .find(|id| screen.parents.get(id) == Some(&row))
        .expect("the toggle");
    engine
        .control()
        .dispatch_event(toggle, EventKind::Press, EventPayload::None);
    drain(&rx, &mut screen);
    assert!(shows(&screen, "variant=\"inset\""), "{:?}", screen.texts);
}
