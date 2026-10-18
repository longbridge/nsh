//! The desktop layout of wide windows: a sidebar of the demos and the
//! selected one in a pane beside it; the phone layout below 900 pt (700 pt
//! on desktops), what is open carrying over.

use nsh_core::Size;

use crate::common::{Screen, drain, press, shows, start_at};

const STOCKS: &str = "A live watchlist and a stock: candles, depth, the tape, a tick every 50 ms";
const PHOTOS: &str = "A long recycled list: images, pull to refresh, paging";

#[test]
fn the_sidebar_shows_the_selected_demo_in_the_pane() {
    let (engine, rx) = start_at(Size::new(1200.0, 800.0));
    let mut screen = Screen::default();
    drain(&rx, &mut screen);
    for label in [
        "Rust Gallery",
        "APPS",
        "PLATFORM",
        "System",
        "1200 × 800 pt",
        "Narrower than 700 pt: the phone layout",
        // The first demo, under its name, what it shows and its group.
        STOCKS,
        "Apps",
        "Composite",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    // No index to go back to: the pane's screen has no title.
    assert!(screen.titles.is_empty(), "{:?}", screen.titles);
    press(&engine, &rx, &mut screen, "Photos");
    for label in [PHOTOS, "Photo stream"] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    assert!(!shows(&screen, STOCKS), "{:?}", screen.texts);
}

#[test]
fn resizing_switches_layouts_and_keeps_the_open_demo() {
    let (engine, rx) = start_at(Size::new(1200.0, 800.0));
    let mut screen = Screen::default();
    drain(&rx, &mut screen);
    press(&engine, &rx, &mut screen, "Photos");
    // The phone layout: the index with the demo pushed over it.
    engine.control().resize(Size::new(390.0, 844.0));
    drain(&rx, &mut screen);
    for title in ["Rust Gallery", "Photos"] {
        assert!(
            screen.titles.iter().any(|t| t.1 == title),
            "{title}: {:?}",
            screen.titles
        );
    }
    // No sidebar.
    let footer = "Narrower than 700 pt: the phone layout";
    assert!(!shows(&screen, footer), "{:?}", screen.texts);
    // Back to the desktop layout: the demo the phone had open.
    engine.control().resize(Size::new(1200.0, 800.0));
    drain(&rx, &mut screen);
    assert!(screen.titles.is_empty(), "{:?}", screen.titles);
    for label in [footer, PHOTOS, "Photo stream"] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
}
