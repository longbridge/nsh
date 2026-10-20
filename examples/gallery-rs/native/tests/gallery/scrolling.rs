//! The Scrolling demo on a headless engine: its sections render, the
//! cards snap on a recycled list, drags and offsets are logged, the
//! switches change how the box scrolls, the zoom shows what the scroll
//! view reports, the feed and the keyboard taps choice change what shows.

use crossbeam_channel::Receiver;
use nsh_core::{EventKind, EventPayload, MountBatch, SnapAlignment};
use nsh_engine::Engine;

use crate::common::{Screen, press, shows};

/// Presses the switch in the row labeled `label`.
fn toggle(engine: &Engine, rx: &Receiver<MountBatch>, screen: &mut Screen, label: &str) {
    let text = screen
        .find(label)
        .unwrap_or_else(|| panic!("no {label}: {:?}", screen.texts));
    let labels = screen.parents[&text];
    let row = screen.parents[&labels];
    let switch = screen
        .parents
        .iter()
        .find(|&(&child, &parent)| parent == row && child != labels)
        .map(|(&child, _)| child)
        .expect("a switch");
    engine
        .control()
        .dispatch_event(switch, EventKind::Press, EventPayload::None);
    crate::common::drain(rx, screen);
}

#[test]
fn scrolling_sections_react() {
    let (engine, rx, mut screen) = crate::common::open("Scrolling");
    for label in [
        "Paging",
        "5 pages",
        "pagingEnabled · scrollValue",
        "Revenue · 1 of 5",
        "48.2k +12 %",
        "Errors · 5 of 5",
        "Settled on page 1",
        "Snapping",
        "snapToInterval · snapToAlignment",
        "Card 1",
        "at 0 pt",
        "accent",
        "center",
        "One card per fling",
        "disableIntervalMomentum",
        "Zoom",
        "maximumZoomScale · zoomTo()",
        "1.00 × fit",
        "0.5× to 5×",
        "Tile 1",
        "Events and behavior",
        "0 pt Scroll the box",
        "viewport — pt",
        "No drags yet",
        "Scroll events at most every",
        "Scroll enabled",
        "scrollEnabled",
        "Bounces",
        "Content inset",
        "40 pt at the top and bottom",
        "Bounces past the limits",
        "Prepending keeps the position",
        "Post 1",
        "#0",
        "30 posts",
        "Taps with the keyboard up",
        "Tap me",
        "A tap outside the input only hides the keyboard; the button does not get it.",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }

    // The cards snap on a recycled list; one per fling once toggled.
    let cards = screen
        .scroll_of(screen.find("Card 1").unwrap())
        .expect("the cards");
    let snap = |screen: &crate::common::Screen| {
        let b = &screen.behaviors[&cards];
        (
            b.snap_interval.0,
            b.snap_alignment,
            b.deceleration,
            b.one_step,
        )
    };
    assert_eq!(snap(&screen), (232.0, SnapAlignment::Center, 0.99, false));
    toggle(&engine, &rx, &mut screen, "One card per fling");
    assert!(snap(&screen).3);
    press(&engine, &rx, &mut screen, "start");
    assert_eq!(snap(&screen).1, SnapAlignment::Start);

    // A drag on the events' box is logged.
    // The box: the scroll view of its 30 numbered rows.
    let scroll = screen
        .scroll_of(screen.find("30").expect("row 30"))
        .expect("the events' box");
    engine.control().dispatch_event(
        scroll,
        EventKind::ScrollBeginDrag,
        EventPayload::Scroll(nsh_core::geometry::point(
            nsh_core::geometry::px(0.0),
            nsh_core::geometry::px(12.0),
        )),
    );
    crate::common::drain(&rx, &mut screen);
    assert!(shows(&screen, "begin drag"), "{:?}", screen.texts);
    assert!(shows(&screen, "at 12 pt"), "{:?}", screen.texts);
    // Its offset, as `on_scroll` reports it.
    engine.control().scroll(scroll, 0.0, 40.0, u32::MAX);
    crate::common::drain(&rx, &mut screen);
    assert!(
        screen.texts.iter().any(|t| t.1.starts_with("40 pt / ")),
        "{:?}",
        screen.texts
    );
    assert!(screen.behaviors[&scroll].enabled);
    toggle(&engine, &rx, &mut screen, "Scroll enabled");
    assert!(!screen.behaviors[&scroll].enabled, "scrolling turned off");

    // The zoom label shows the zoom the scroll view reports.
    let tile = screen.find("Tile 1").unwrap();
    let zoom = screen.scroll_of(tile).expect("the zooming scroll view");
    press(&engine, &rx, &mut screen, "2×");
    // The platform reports the zoom, then the offset it moved to.
    engine.control().zoom(zoom, 2.0);
    engine.control().scroll(zoom, 40.0, 30.0, u32::MAX);
    crate::common::drain(&rx, &mut screen);
    assert!(shows(&screen, "2.00 × zoomed in"), "{:?}", screen.texts);
    assert!(screen.behaviors[&zoom].bounces_zoom);
    toggle(&engine, &rx, &mut screen, "Bounces past the limits");
    assert!(!screen.behaviors[&zoom].bounces_zoom);

    press(&engine, &rx, &mut screen, "Add 3 posts on top");
    assert!(shows(&screen, "New 1"), "{:?}", screen.texts);
    assert!(shows(&screen, "new"), "{:?}", screen.texts);
    assert!(shows(&screen, "33 posts"), "{:?}", screen.texts);

    press(&engine, &rx, &mut screen, "handled");
    assert!(
        shows(
            &screen,
            "The button gets the tap and the keyboard stays; a tap on empty space hides it."
        ),
        "{:?}",
        screen.texts
    );

    press(&engine, &rx, &mut screen, "Tap me");
    let tap_me = screen.parents[&screen.find("Tap me").unwrap()];
    assert!(
        screen
            .texts
            .iter()
            .any(|(id, t)| t == "1" && screen.parents.get(id) == Some(&tap_me)),
        "{:?}",
        screen.texts
    );
}
