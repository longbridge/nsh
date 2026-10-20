//! The Styles demo on a headless engine: its cards render with their
//! shadows, corners, borders and transforms; the clipping tile and the
//! stacking tile take taps; "Flip" lays the surface out right to left and
//! back.

use std::collections::HashMap;

use nsh_core::{
    BorderStyle, EventKind, EventPayload, MountItem, TransformKind, ViewStyle, geometry::px,
};

#[test]
fn styles_render_their_cards() {
    let (_engine, _rx, screen) = crate::common::open("Styles");
    for text in [
        "Corners",
        "Continuous",
        "Top corners",
        "Mixed radii",
        "Per-side borders",
        "Dashed / dotted",
        "Shadows and glow",
        "Glow",
        "Offset shadow",
        "Shadow",
        "Text corners",
        "Elevation",
        "Clipped",
        "overflow: hidden",
        "Stacking order",
        "Front: 1 of 3 · tap",
        "0.45",
        "Perspective",
        "rotateX: 40deg",
        "Skew",
        "Tap to flip",
        "backfaceVisibility",
        "1,111.11",
        "Small Caps",
        "Right to left",
        "isRTL: false",
        "12",
    ] {
        assert!(
            screen.texts.iter().any(|t| t.1 == text),
            "{text}: {:?}",
            screen.texts
        );
    }
}

#[test]
fn styles_send_the_cards_styles() {
    let (engine, rx) = crate::common::start();
    let mut screen = crate::common::Screen::default();
    crate::common::drain(&rx, &mut screen);
    let text = screen.find("Styles").expect("the demo in the index");
    engine.control().dispatch_event(
        screen.press_target(text),
        EventKind::Press,
        EventPayload::None,
    );
    let mut styles: HashMap<u32, ViewStyle> = HashMap::new();
    while let Ok(batch) = rx.recv_timeout(std::time::Duration::from_millis(300)) {
        for item in &batch.items {
            if let MountItem::ViewStyle { id, style } = item {
                styles.insert(*id, (**style).clone());
            }
        }
    }
    let any = |f: &dyn Fn(&ViewStyle) -> bool| styles.values().any(f);
    // "Mixed radii": three corners of their own; "Offset shadow": a
    // colored shadow, offset.
    assert!(any(
        &|s| s.corner_radius.to_array()[..3] == [Some(px(44.0)), Some(px(6.0)), Some(px(28.0))]
    ));
    assert!(any(&|s| s.shadow.opacity == 0.45
        && s.shadow.radius == px(10.0)
        && (s.shadow.offset_x, s.shadow.offset_y) == (px(4.0), px(8.0))));
    // Per-side borders.
    assert!(any(
        &|s| s.borders.widths.to_array() == [6.0, 2.0, 4.0, 1.0].map(px)
    ));
    assert!(any(&|s| s.borders.style == BorderStyle::Dashed));
    assert!(any(&|s| s.borders.style == BorderStyle::Dotted));
    assert!(any(&|s| s.continuous_corners));
    assert!(any(&|s| s.backface_hidden));
    let has =
        |s: &ViewStyle, kind: TransformKind| s.transform.ops().iter().any(|op| op.kind == kind);
    assert!(any(
        &|s| has(s, TransformKind::Perspective) && has(s, TransformKind::RotateY)
    ));
    assert!(any(&|s| has(s, TransformKind::SkewX)));
    assert!(any(
        &|s| s.transform_origin.x == nsh_core::geometry::relative(0.0)
    ));
}

#[test]
fn the_layer_tiles_take_taps() {
    let (engine, rx, mut screen) = crate::common::open("Styles");
    crate::common::press(&engine, &rx, &mut screen, "Tap");
    for text in ["Visible", "overflow: visible"] {
        assert!(
            screen.texts.iter().any(|t| t.1 == text),
            "{text}: {:?}",
            screen.texts
        );
    }
    crate::common::press(&engine, &rx, &mut screen, "Tap");
    assert!(
        screen.texts.iter().any(|t| t.1 == "Clipped"),
        "{:?}",
        screen.texts
    );
    // The stacking tile's stage (its specimen has no text): the pressable
    // view inside the tile the note is in.
    let note = screen.find("Front: 1 of 3 · tap").unwrap();
    let tile = screen.parents[&screen.parents[&note]];
    let inside = |mut id: u32| {
        while let Some(&up) = screen.parents.get(&id) {
            if up == tile {
                return true;
            }
            id = up;
        }
        false
    };
    let stage = *screen
        .events
        .iter()
        .find(|(id, e)| e.contains(EventKind::Press) && inside(**id))
        .expect("the stage takes presses")
        .0;
    engine
        .control()
        .dispatch_event(stage, EventKind::Press, EventPayload::None);
    crate::common::drain(&rx, &mut screen);
    assert!(
        screen.texts.iter().any(|t| t.1 == "Front: 2 of 3 · tap"),
        "{:?}",
        screen.texts
    );
}

#[test]
fn flip_lays_the_surface_out_right_to_left() {
    let (engine, rx, mut screen) = crate::common::open("Styles");
    let flip = screen.press_target(screen.find("Flip").expect("the Flip button"));
    let mut directions = Vec::new();
    let mut press = |screen: &mut crate::common::Screen| {
        engine
            .control()
            .dispatch_event(flip, EventKind::Press, EventPayload::None);
        while let Ok(batch) = rx.recv_timeout(std::time::Duration::from_millis(300)) {
            screen.apply(&batch);
            for item in &batch.items {
                if let MountItem::LayoutDirection { rtl } = item {
                    directions.push(*rtl);
                }
            }
        }
    };
    press(&mut screen);
    assert!(
        screen.texts.iter().any(|t| t.1 == "isRTL: true"),
        "{:?}",
        screen.texts
    );
    press(&mut screen);
    assert!(
        screen.texts.iter().any(|t| t.1 == "isRTL: false"),
        "{:?}",
        screen.texts
    );
    assert_eq!(directions, [true, false]);
}
