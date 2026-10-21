//! The Lists demo on a headless engine: its tabs switch, the grid's
//! header, footer and visible rows, the sticky section headers, the chat
//! sends, dismisses the keyboard and loads older messages, the empty list
//! fills and clears.

use nsh_core::{EventKind, EventPayload, KeyboardDismissMode, MountItem, PayloadValue};

use crate::common::{drain, press, shows};

#[test]
fn lists_switch_tabs_and_change_their_rows() {
    let (engine, rx, mut screen) = crate::common::open("Lists");
    for label in [
        "Grid",
        "Sections",
        "Chat",
        "More",
        "Viewable items (at least half shown)",
        "90 items, 3 columns",
        "Jump to 60",
        "End",
        "0",
        "84 pt",
        "1",
        "106 pt",
        "End of the list",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    // The core reports the rows it shows.
    assert!(
        screen.texts.iter().any(|t| t.1.starts_with("0–")),
        "{:?}",
        screen.texts
    );
    // The header scrolls with the rows: it is in the grid's list.
    let grid = screen
        .scroll_of(screen.find("84 pt").unwrap())
        .expect("the grid");
    assert_eq!(
        screen.scroll_of(screen.find("Jump to 60").unwrap()),
        Some(grid)
    );
    engine.control().dispatch_event(
        grid,
        EventKind::ViewableItemsChanged,
        EventPayload::Map(vec![
            ("first", PayloadValue::Number(0.0)),
            ("last", PayloadValue::Number(8.0)),
        ]),
    );
    drain(&rx, &mut screen);
    assert!(shows(&screen, "0–8  9 visible"), "{:?}", screen.texts);

    press(&engine, &rx, &mut screen, "Sections");
    for label in [
        "A",
        "2",
        "AL",
        "Ada Lovelace",
        "Analytical Engine notes",
        "AK",
        "B",
        "Barbara Liskov",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    assert!(!shows(&screen, "Jump to 60"), "the grid is gone");
    let contacts = screen
        .scroll_of(screen.find("Ada Lovelace").unwrap())
        .unwrap();
    assert!(screen.sticky.contains(&contacts), "the letters stick");

    // The chat: type, then send.
    press(&engine, &rx, &mut screen, "Chat");
    for label in [
        "Release room",
        "30 messages, 0 older pages",
        "Did the build go out?",
        "09:00",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    let input = *screen
        .events
        .iter()
        .find(|(_, e)| e.contains(EventKind::SubmitEditing))
        .expect("the message input")
        .0;
    engine.control().dispatch_event(
        input,
        EventKind::ChangeText,
        EventPayload::Text {
            text: "Hello there".into(),
            count: 1,
        },
    );
    crate::common::drain(&rx, &mut screen);
    engine.control().dispatch_event(
        input,
        EventKind::SubmitEditing,
        EventPayload::Text {
            text: "Hello there".into(),
            count: 1,
        },
    );
    drain(&rx, &mut screen);
    assert!(shows(&screen, "Hello there"), "{:?}", screen.texts);
    assert!(
        shows(&screen, "31 messages, 0 older pages"),
        "{:?}",
        screen.texts
    );
    let chat = screen
        .scroll_of(screen.find("Did the build go out?").unwrap())
        .unwrap();
    assert!(
        matches!(
            screen.scrolls.get(&chat),
            Some(MountItem::ScrollConfig {
                keyboard_dismiss: KeyboardDismissMode::Interactive,
                ..
            })
        ),
        "{:?}",
        screen.scrolls.get(&chat)
    );
    // Near its end (the oldest messages), older ones load.
    engine
        .control()
        .dispatch_event(chat, EventKind::EndReached, EventPayload::None);
    drain(&rx, &mut screen);
    engine.control().scroll(
        chat,
        nsh_core::geometry::point(nsh_core::geometry::px(0.0), nsh_core::geometry::px(0.0)),
        u32::MAX,
    );
    drain(&rx, &mut screen);
    assert!(
        screen
            .texts
            .iter()
            .any(|t| t.1.starts_with("Older message")),
        "{:?}",
        screen.texts
    );
    assert!(
        shows(&screen, "51 messages, 1 older pages")
            || shows(&screen, "71 messages, 2 older pages"),
        "{:?}",
        screen.texts
    );

    // The empty list: Add fills it, Clear empties it.
    press(&engine, &rx, &mut screen, "More");
    for label in [
        "Pull to refresh",
        "Pull the page down",
        "refreshes",
        "Horizontal scroll view",
        "This week",
        "Today",
        "Horizontal list",
        "1,000 recycled cards",
        "Card 1",
        "0 %",
        "An empty list",
        "0 items",
        "Nothing here yet: tap Add",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    // A range chip selects itself: the note follows.
    press(&engine, &rx, &mut screen, "Yesterday");
    assert!(
        screen.texts.iter().filter(|t| t.1 == "Yesterday").count() == 2,
        "{:?}",
        screen.texts
    );
    press(&engine, &rx, &mut screen, "Add");
    press(&engine, &rx, &mut screen, "Add");
    assert!(shows(&screen, "2 items"), "{:?}", screen.texts);
    assert!(shows(&screen, "accent") && shows(&screen, "info"));
    assert!(shows(&screen, "Item 1") && shows(&screen, "Item 2"));
    assert!(!shows(&screen, "Nothing here yet: tap Add"));
    // A separator after the first row, in its cell.
    let row = screen.parents[&screen.find("Item 1").unwrap()];
    let first = screen.parents[&row];
    let in_first = screen.parents.values().filter(|&&p| p == first).count();
    assert_eq!(in_first, 2, "the row and a separator");
    press(&engine, &rx, &mut screen, "Clear");
    assert!(shows(&screen, "0 items"), "{:?}", screen.texts);
    assert!(shows(&screen, "Nothing here yet: tap Add"));
}
