//! The Physics demo on a headless engine: a release springs the card back
//! with the drag's velocity, coasts or snaps to a corner and is counted, a
//! tap pulses it, a long press resets the figures.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    EventKind, EventPayload, MountBatch, MountItem,
    nsh_animation::{AnimationOp, Driver},
};
use nsh_engine::Engine;

use crate::common::shows;

/// Sends `kind` with `payload` to the parent of the text `label`; the
/// drivers the animations it started have.
fn dispatch(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut crate::common::Screen,
    label: &str,
    kind: EventKind,
    payload: EventPayload,
) -> Vec<Driver> {
    let text = screen.find(label).unwrap_or_else(|| panic!("no {label}"));
    let target = screen.press_target(text);
    engine.control().dispatch_event(target, kind, payload);
    let mut started = Vec::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
        screen.apply(&batch);
        for item in &batch.items {
            if let MountItem::Animation(AnimationOp::Start { driver, .. }) = item {
                started.push(*driver);
            }
        }
    }
    started
}

/// The value of the stat labelled `label` (the text next to it).
fn stat(screen: &crate::common::Screen, label: &str) -> String {
    let id = screen.find(label).unwrap_or_else(|| panic!("no {label}"));
    let parent = screen.parents[&id];
    screen
        .texts
        .iter()
        .find(|(t, _)| *t != id && screen.parents.get(t) == Some(&parent))
        .map(|(_, text)| text.clone())
        .unwrap_or_else(|| panic!("no value of {label}: {:?}", screen.texts))
}

#[test]
fn physics_springs_coasts_and_snaps() {
    let (engine, rx, mut screen) = crate::common::open("Physics");
    for label in [
        "Drag and release",
        "Tap to pulse, long press to reset",
        "UI thread",
        "Springs home with the release velocity",
        "Spring",
        "Coast",
        "Snap",
        "Bouncy",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    assert_eq!(stat(&screen, "Releases"), "0");
    assert_eq!(stat(&screen, "Speed"), "0 pt/s");

    let release = EventPayload::Pan {
        translation: nsh_core::geometry::point(
            nsh_core::geometry::px(80.0),
            nsh_core::geometry::px(20.0),
        ),
        velocity_x: 300.0,
        velocity_y: -400.0,
        value_x: Some(nsh_core::geometry::px(80.0)),
        value_y: Some(nsh_core::geometry::px(20.0)),
    };
    let springs = dispatch(
        &engine,
        &rx,
        &mut screen,
        "Drag me",
        EventKind::PanEnd,
        release.clone(),
    );
    // The lift settles, the card springs home with the drag's velocity
    // and the bouncy feel.
    let spring = |to, velocity| Driver::Spring {
        to,
        stiffness: 180.0,
        damping: 11.0,
        mass: 1.0,
        velocity: Some(velocity),
    };
    assert_eq!(springs[1..], [spring(0.0, 300.0), spring(0.0, -400.0)]);
    assert_eq!(stat(&screen, "Releases"), "1");
    assert_eq!(stat(&screen, "Speed"), "500 pt/s");
    assert_eq!(stat(&screen, "Spring"), "180 / 11");

    // Coast: a decay with the velocity.
    crate::common::press(&engine, &rx, &mut screen, "Coast");
    assert!(shows(&screen, "Coasts to a stop, then springs home"));
    assert!(shows(&screen, "decay"));
    let coast = dispatch(
        &engine,
        &rx,
        &mut screen,
        "Drag me",
        EventKind::PanEnd,
        release.clone(),
    );
    assert!(
        matches!(
            coast[1..],
            [
                Driver::Decay {
                    velocity: Some(300.0),
                    ..
                },
                Driver::Decay {
                    velocity: Some(-400.0),
                    ..
                },
                ..
            ]
        ),
        "{coast:?}"
    );

    // Snap, firm: to the corner the fling points at (up and right).
    crate::common::press(&engine, &rx, &mut screen, "Snap");
    crate::common::press(&engine, &rx, &mut screen, "Firm");
    dispatch(
        &engine,
        &rx,
        &mut screen,
        "Drag me",
        EventKind::PanEnd,
        release,
    );
    assert_eq!(stat(&screen, "Corner"), "top right");
    assert_eq!(stat(&screen, "Releases"), "3");

    // A tap pulses (natively; the count stays): up first.
    let pulse = dispatch(
        &engine,
        &rx,
        &mut screen,
        "Drag me",
        EventKind::Press,
        EventPayload::None,
    );
    assert!(
        matches!(
            pulse[..],
            [Driver::Timing {
                to: 1.0,
                duration_ms: 120.0,
                ..
            }]
        ),
        "{pulse:?}"
    );
    assert_eq!(stat(&screen, "Releases"), "3");

    // A long press resets the figures and springs home.
    dispatch(
        &engine,
        &rx,
        &mut screen,
        "Drag me",
        EventKind::LongPress,
        EventPayload::None,
    );
    assert_eq!(stat(&screen, "Releases"), "0");
    assert_eq!(stat(&screen, "Speed"), "0 pt/s");
    assert_eq!(stat(&screen, "Corner"), "—");
}
