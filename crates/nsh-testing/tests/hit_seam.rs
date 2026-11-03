//! The hit seam (spike): `Screen::hit` / `bounds` / `is_interactive` over a
//! real mounted tree in Headless, with the three planted bug classes from
//! the editor's hit-target showcase. The law: a press must land on a node
//! that is shown (the tree), non-degenerate (the engine's geometry) and
//! interactive (the binding) — each chip breaks one layer only.

use std::path::Path;
use std::time::Duration;

use nsh_core::Points;
use nsh_core::geometry::point;
use nsh_testing::tsx;

#[test]
fn hit_target_law_over_a_mounted_tree() {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/testing/hit/App.tsx");
    let mut app = tsx(&entry, Vec::new()).unwrap();
    app.wait_for("Resume").unwrap();
    app.idle(Duration::from_millis(50)).unwrap();

    let screen = app.screen();
    let centre = |b: nsh_core::Bounds<Points>| {
        point(
            Points(b.origin.x.0 + b.size.width.0 / 2.0),
            Points(b.origin.y.0 + b.size.height.0 / 2.0),
        )
    };

    // The honest control passes every layer.
    let resume = screen
        .press_target("Resume")
        .unwrap()
        .expect("Resume is pressable");
    let bounds = screen.bounds(resume).expect("Resume has bounds");
    assert!(
        bounds.size.width.0 > 0.0 && bounds.size.height.0 > 0.0,
        "{bounds:?}"
    );
    assert_eq!(screen.hit(centre(bounds)), Some(resume));
    assert!(screen.is_interactive(resume));

    // Chip 1 (engine): zero-size — bound in the app, but layout gave it no
    // area: it does not even reach the tree's texts, let alone a press.
    assert!(
        !screen.texts().contains(&"Zero"),
        "zero-size text is culled: {:?}",
        screen.texts()
    );
    let zero = screen.press_target("Zero").unwrap();
    assert_eq!(zero, None, "a zero-size button is not pressable");

    // Chip 2 (engine): occluded — bound and sized, but the hit at its centre
    // is not the button.
    let covered = screen.press_target("Covered").unwrap();
    match covered {
        Some(id) => {
            let b = screen.bounds(id).expect("Covered has bounds");
            assert_ne!(
                screen.hit(centre(b)),
                Some(id),
                "the overlay takes the press"
            );
        }
        None => { /* the resolver itself already refuses it: also a catch */ }
    }

    // Chip 3 (tree): missing binding — geometry is honest, interactivity is not.
    assert!(screen.shows("Unbound"));
    assert_eq!(screen.press_target("Unbound").unwrap(), None);
    let hit = screen
        .hit(point(Points(70.0), Points(262.0)))
        .expect("something is shown there");
    assert!(
        !screen.is_interactive(hit),
        "what is hit there takes no press"
    );
    // NodeId stability across a state-driven re-render: pressing Resume
    // re-renders the footer; the button keeps its id and its hit.
    let digests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = digests.clone();
    app.observe(move |_step, batch| seen.lock().unwrap().push(batch.digest()));
    app.press("Resume").unwrap();
    app.wait_for("Pressed: resume").unwrap();
    assert_eq!(app.screen().press_target("Resume").unwrap(), Some(resume));
    assert_eq!(app.screen().hit(centre(bounds)), Some(resume));
    assert!(!digests.lock().unwrap().is_empty(), "batches were digested");
    assert!(digests.lock().unwrap().iter().all(|d| d.len() == 16));
}
