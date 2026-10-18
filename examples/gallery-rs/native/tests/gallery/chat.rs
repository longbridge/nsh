//! The AI Chat demo on a headless engine: the empty chat offers its
//! scenarios, a message sent offline waits for the connection, a reply
//! that thinks first folds its reasoning when done, and a failed reply
//! shows its error and retries.

use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use nsh_core::{EventKind, EventPayload, MountBatch};
use nsh_engine::Engine;

use crate::common::{Screen, shows};

/// Applies what comes in until `done` holds (at most `secs` seconds).
fn until(
    rx: &Receiver<MountBatch>,
    screen: &mut Screen,
    secs: u64,
    done: impl Fn(&Screen) -> bool,
) {
    let end = Instant::now() + Duration::from_secs(secs);
    while !done(screen) {
        let left = end
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| panic!("not done in {secs} s: {:?}", screen.texts));
        if let Ok(batch) = rx.recv_timeout(left.min(Duration::from_millis(100))) {
            screen.apply(&batch);
        }
    }
}

fn press(engine: &Engine, screen: &Screen, label: &str) {
    let text = screen
        .find(label)
        .unwrap_or_else(|| panic!("no {label}: {:?}", screen.texts));
    engine.control().dispatch_event(
        screen.press_target(text),
        EventKind::Press,
        EventPayload::None,
    );
}

fn has(screen: &Screen, part: &str) -> bool {
    screen.texts.iter().any(|t| t.1.contains(part))
}

#[test]
fn chat_streams_fails_retries_and_thinks() {
    let (engine, rx, mut screen) = crate::common::open("AI Chat");
    for label in [
        "How can I help?",
        "Explain list virtualization",
        "Think it through",
        "Search the web",
        "Describe a photo",
        "Tell me a short story",
        "Load a long conversation",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }

    // Offline: the message waits for the connection, then the reply
    // comes.
    press(&engine, &screen, "Go offline");
    until(&rx, &mut screen, 3, |s| {
        shows(s, "Waiting for a connection")
    });
    until(&rx, &mut screen, 10, |s| {
        has(s, "Tuesday, November 3") && !shows(s, "Waiting for a connection")
    });
    // The words show as they stream: the reply is done when it can no
    // longer be stopped.
    until(&rx, &mut screen, 5, |s| !s.labelled("Stop generating"));

    // A reply that thinks first: the reasoning streams, then folds into
    // its label once the answer is done; the label opens it again.
    press(&engine, &screen, "Think it through");
    until(&rx, &mut screen, 5, |s| has(s, "The quick answer is 0.10"));
    until(&rx, &mut screen, 10, |s| {
        has(s, "The intuitive answer, 0.10, would make the total 1.20.")
            && !has(s, "The quick answer is 0.10")
    });
    let thought = screen
        .texts
        .iter()
        .find(|t| t.1.starts_with("Thought for "))
        .map(|t| t.1.clone())
        .expect("the reasoning's label");
    press(&engine, &screen, &thought);
    until(&rx, &mut screen, 2, |s| has(s, "The quick answer is 0.10"));

    // A failing reply (from the menu): the words so far, then the error
    // and Retry.
    press(&engine, &screen, "Fail a reply");
    until(&rx, &mut screen, 5, |s| {
        shows(s, "Summarize this week's meeting notes.")
    });
    until(&rx, &mut screen, 8, |s| shows(s, "Retry"));
    assert!(shows(
        &screen,
        "The model is overloaded right now (error 529)."
    ));
    assert!(has(&screen, "The planning session agreed on"));

    // Retried: the reply streams again, in its place.
    press(&engine, &screen, "Retry");
    until(&rx, &mut screen, 10, |s| {
        has(s, "Retro: fewer meetings next sprint.")
    });
    assert!(!shows(&screen, "Retry"), "{:?}", screen.texts);
    assert!(has(&screen, "Planning: the launch moves to November 3."));
}
