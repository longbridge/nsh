//! The Stocks demo on a headless engine: the watchlist ticks every 50 ms,
//! the strip opens breadth, the filters re-sort, a stock opens its screen
//! whose quote, book and tape follow the feed, the book shows ten levels,
//! the market pauses and resumes, and the ticket places an order.

use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use nsh_core::{EventKind, EventPayload, MountBatch, ROOT_ID, TouchEvent, TouchPhase, TouchPoint};
use nsh_engine::Engine;

use crate::common::{Screen, shows};

/// Applies what comes in for `ms` milliseconds (the feed never goes quiet).
fn pump(rx: &Receiver<MountBatch>, screen: &mut Screen, ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        if let Ok(batch) = rx.recv_timeout(left) {
            screen.apply(&batch);
        }
    }
}

/// Presses what the text `label` is in, then lets the feed run a while.
fn press(engine: &Engine, rx: &Receiver<MountBatch>, screen: &mut Screen, label: &str) {
    let text = screen
        .find(label)
        .unwrap_or_else(|| panic!("no {label}: {:?}", screen.texts));
    let target = screen.press_target(text);
    engine
        .control()
        .dispatch_event(target, EventKind::Press, EventPayload::None);
    pump(rx, screen, 400);
}

/// One finger on the chart at `x`.
fn scrub(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut Screen,
    chart: u32,
    phase: TouchPhase,
    x: f32,
    activated: Option<u32>,
) {
    let event = TouchEvent {
        phase,
        timestamp: 0.0,
        activated,
        points: vec![TouchPoint {
            id: 1,
            target: chart,
            path: if phase == TouchPhase::Start {
                vec![chart]
            } else {
                Vec::new()
            },
            page: nsh_core::geometry::point(
                nsh_core::geometry::px(x),
                nsh_core::geometry::px(400.0),
            ),
            location: nsh_core::geometry::point(
                nsh_core::geometry::px(x),
                nsh_core::geometry::px(60.0),
            ),
            changed: true,
        }],
    };
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::Touch,
        EventPayload::Touch(Box::new(event)),
    );
    pump(rx, screen, 200);
}

/// The market clocks shown (`HH:MM:SS`).
fn clocks(screen: &Screen) -> Vec<String> {
    screen
        .texts
        .iter()
        .map(|t| t.1.clone())
        .filter(|t| t.len() == 8 && t.as_bytes()[2] == b':' && t.as_bytes()[5] == b':')
        .collect()
}

#[test]
fn stocks_tick_open_a_stock_and_place_an_order() {
    let (engine, rx) = crate::common::start();
    let mut screen = Screen::default();
    crate::common::drain(&rx, &mut screen);
    press(&engine, &rx, &mut screen, "Stocks");
    for label in [
        "Composite",
        "All",
        "Gainers",
        "ORBX",
        "Orbex Freight",
        "FLKS",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    // The clock moves a market second per tick.
    let before = clocks(&screen);
    assert_eq!(before.len(), 1, "{before:?}");
    pump(&rx, &mut screen, 500);
    let after = clocks(&screen);
    assert_ne!(before, after, "the feed ticks");

    // The strip opens breadth and the sectors.
    press(&engine, &rx, &mut screen, "Composite");
    assert!(shows(&screen, "Technology"), "{:?}", screen.texts);
    assert!(screen.texts.iter().any(|t| t.1.ends_with("issues")));

    // Gainers only: the losers leave.
    press(&engine, &rx, &mut screen, "Gainers");
    assert!(!shows(&screen, "ORBX"), "{:?}", screen.texts);
    press(&engine, &rx, &mut screen, "All");

    // A stock: its quote, chart, book, tape and flow, live.
    press(&engine, &rx, &mut screen, "ORBX");
    assert!(
        screen.titles.iter().any(|t| t.1 == "ORBX"),
        "{:?}",
        screen.titles
    );
    for label in [
        "Industrials",
        "Prev close",
        "Intraday",
        "Order book",
        "5 → 10",
        "A5",
        "B5",
        "Time & sales",
        "Depth",
        "Trade flow",
        "Ranges",
        "Sell",
        "Buy",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    let before = clocks(&screen);
    pump(&rx, &mut screen, 500);
    assert_ne!(
        before,
        clocks(&screen),
        "the stock's screen follows the feed"
    );

    // Scrubbing the chart: the platform activates its pan; the readout
    // shows the minute under the finger, and goes when it lifts.
    let chart = *screen
        .touch_listeners()
        .last()
        .expect("the chart listens to touches");
    for (phase, x, activated) in [
        (TouchPhase::Start, 100.0, None),
        (TouchPhase::Move, 120.0, Some(chart)),
        (TouchPhase::Move, 160.0, None),
    ] {
        scrub(&engine, &rx, &mut screen, chart, phase, x, activated);
    }
    assert!(
        screen.texts.iter().any(|t| t.1.starts_with("avg ")),
        "the readout: {:?}",
        screen.texts
    );
    scrub(
        &engine,
        &rx,
        &mut screen,
        chart,
        TouchPhase::End,
        160.0,
        None,
    );
    assert!(!screen.texts.iter().any(|t| t.1.starts_with("avg ")));

    // Ten levels.
    press(&engine, &rx, &mut screen, "5 → 10");
    assert!(shows(&screen, "10 → 5"), "{:?}", screen.texts);
    assert!(shows(&screen, "A10") && shows(&screen, "B10"));
    let before = clocks(&screen);
    pump(&rx, &mut screen, 500);
    assert_ne!(before, clocks(&screen), "still ticking");

    // The ticket: review and place a sell order.
    press(&engine, &rx, &mut screen, "Sell");
    assert!(shows(&screen, "Sell ORBX"), "{:?}", screen.texts);
    press(&engine, &rx, &mut screen, "Review sell");
    assert!(shows(&screen, "LIMIT · DAY"), "{:?}", screen.texts);
    press(&engine, &rx, &mut screen, "Place sell order");
    assert!(shows(&screen, "Sell order placed"), "{:?}", screen.texts);
}
