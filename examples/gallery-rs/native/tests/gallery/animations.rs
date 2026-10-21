//! The Animations demo on a headless engine, with the platform's animation
//! graph played here: its cards render, the hero loops by itself, the
//! compositions run natively and report finishing, stopping and resetting
//! (back to where the run started), the race reverses, the interpolation's
//! segments and the layout's buttons follow their state.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    EventKind, EventPayload, MountBatch, MountItem,
    animation::{AnimationValue, Finished, Graph, NodeId},
};
use nsh_engine::Engine;

use crate::common::{Screen, shows};

/// The gallery, what it shows and the platform's graph.
struct Platform {
    engine: Engine,
    rx: Receiver<MountBatch>,
    screen: Screen,
    graph: Graph,
    finished: Vec<Finished>,
    /// The graph's clock, in seconds.
    now: f64,
    /// The nodes bound to views' properties.
    bound: Vec<NodeId>,
}

impl Platform {
    /// The gallery with the Animations demo open, every graph operation
    /// applied.
    fn open() -> Self {
        let (engine, rx) = crate::common::start();
        let mut p = Self {
            engine,
            rx,
            screen: Screen::default(),
            graph: Graph::default(),
            finished: Vec::new(),
            now: 0.0,
            bound: Vec::new(),
        };
        p.pump();
        let row = p
            .screen
            .press_target(p.screen.find("Animations").expect("its row"));
        p.dispatch(row);
        assert!(
            p.screen.titles.iter().any(|t| t.1 == "Animations"),
            "{:?}",
            p.screen.titles
        );
        p
    }

    /// Applies batches until the engine is quiet, handing it the ends of
    /// animations.
    fn pump(&mut self) {
        loop {
            if !self.finished.is_empty() {
                let finished = std::mem::take(&mut self.finished);
                self.engine.control().animations_finished(finished);
            }
            let Ok(batch) = self.rx.recv_timeout(Duration::from_millis(300)) else {
                break;
            };
            self.screen.apply(&batch);
            for item in batch.items {
                match item {
                    MountItem::Animation(op) => self.graph.apply(op, &mut self.finished),
                    MountItem::AnimationBindings { bindings, .. } => {
                        for (_, node) in bindings {
                            if !self.bound.contains(&node) {
                                self.bound.push(node);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn dispatch(&mut self, id: u32) {
        self.engine
            .control()
            .dispatch_event(id, EventKind::Press, EventPayload::None);
        self.pump();
        // What came in starts at the graph's next frame.
        self.graph.tick(self.now, &mut self.finished);
        self.pump();
    }

    /// Presses what the text `label` is in (a button, a chip, a segment).
    fn press(&mut self, label: &str) {
        let text = self
            .screen
            .find(label)
            .unwrap_or_else(|| panic!("no {label}: {:?}", self.screen.texts));
        self.dispatch(self.screen.press_target(text));
    }

    /// Runs the graph's frames (60 Hz) for `secs`.
    fn advance(&mut self, secs: f64) {
        let end = self.now + secs;
        while self.now < end {
            self.now += 1.0 / 60.0;
            self.graph.tick(self.now, &mut self.finished);
        }
        self.pump();
    }

    /// What the bound properties are now (those `still` lists).
    fn bound_values(&self, still: &[NodeId]) -> Vec<AnimationValue> {
        still.iter().map(|&n| self.graph.eval(n)).collect()
    }

    /// The bound nodes that do not move by themselves (the hero's loops
    /// run all the time).
    fn still(&mut self) -> Vec<NodeId> {
        let before: Vec<AnimationValue> = self.bound.iter().map(|&n| self.graph.eval(n)).collect();
        self.advance(0.5);
        let mid: Vec<AnimationValue> = self.bound.iter().map(|&n| self.graph.eval(n)).collect();
        self.advance(0.7);
        self.bound
            .iter()
            .enumerate()
            .filter(|&(i, &n)| before.get(i) == Some(&mid[i]) && mid[i] == self.graph.eval(n))
            .map(|(_, &n)| n)
            .collect()
    }

    fn shows(&self, text: &str) -> bool {
        shows(&self.screen, text)
    }

    /// The first text `label` mounted after the text `after` (in the same
    /// card: the cards' texts are mounted one card after another).
    fn after(&self, label: &str, after: &str) -> u32 {
        let from = self.screen.find(after).expect("the anchor");
        self.screen
            .texts
            .iter()
            .filter(|t| t.1 == label && t.0 > from)
            .map(|t| t.0)
            .min()
            .unwrap_or_else(|| panic!("no {label} after {after}"))
    }

    fn count(&self, text: &str) -> usize {
        self.screen.texts.iter().filter(|t| t.1 == text).count()
    }
}

#[test]
fn animations_render_and_react() {
    let mut p = Platform::open();
    for text in [
        "Native motion",
        "Animated.stagger",
        "Easing",
        "Side by side",
        "Composition",
        "Springs",
        "Interpolation",
        "Hiding header",
        "Layout animation",
        "bounce out",
        "Wobbly 180 / 8",
        "idle",
        "Search",
        "Row 30",
        "diffClamp(0, 52)",
        "t = 0",
        "5 views",
    ] {
        assert!(p.shows(text), "{text}: {:?}", p.screen.texts);
    }
    // The page has come in; the hero's loops keep going.
    p.advance(2.0);
    let still = p.still();
    assert!(still.len() < p.bound.len(), "the hero moves by itself");
    let rest = p.bound_values(&still);

    // A composition runs to its end natively.
    p.press("Stagger");
    assert!(p.shows("stagger running"), "{:?}", p.screen.texts);
    p.advance(3.0);
    assert!(p.shows("stagger finished"), "{:?}", p.screen.texts);
    assert_eq!(p.bound_values(&still), rest);

    // A loop goes on until stopped, where it is; a reset puts it back.
    p.press("Loop");
    p.advance(1.0);
    assert!(p.shows("loop running"), "{:?}", p.screen.texts);
    p.press("Stop");
    assert!(p.shows("loop stopped"), "{:?}", p.screen.texts);
    assert_ne!(p.bound_values(&still), rest, "partway");
    p.press("Reset");
    assert_eq!(p.bound_values(&still), rest);

    // A reset stops a run going on and puts its values back where the run
    // started.
    p.press("Sequence");
    assert!(p.shows("sequence running"), "{:?}", p.screen.texts);
    // Turned, waited, fading.
    p.advance(0.9);
    assert_ne!(p.bound_values(&still), rest, "partway");
    p.press("Reset");
    assert!(p.shows("sequence stopped"), "{:?}", p.screen.texts);
    assert_eq!(p.bound_values(&still), rest);

    // A curve chip plays it; the race reverses.
    let chip = p.after("linear", "Curves are data, evaluated natively");
    p.dispatch(p.screen.press_target(chip));
    let race = p.after("Play", "Animated.parallel");
    p.dispatch(p.screen.press_target(race));
    assert!(p.shows("Reverse"), "{:?}", p.screen.texts);

    p.press("t = 2");
    assert_eq!(p.count("t = 2"), 2, "the badge: {:?}", p.screen.texts);

    p.press("Column");
    assert!(p.shows("Row"), "{:?}", p.screen.texts);
    p.press("Row");
    assert!(p.shows("Column"), "{:?}", p.screen.texts);
    p.press("Add");
    assert!(p.shows("6 views"), "{:?}", p.screen.texts);
    p.press("Remove");
    p.press("Remove");
    assert!(p.shows("4 views"), "{:?}", p.screen.texts);
}
