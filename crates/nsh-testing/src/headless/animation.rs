//! The native animations of a headless app: the graph platforms run on
//! their UI thread (`nsh-animation`, platform-free), run here frame by
//! frame while anything animates, finished animations reported to the
//! engine as a platform reports them. Nothing is drawn: the values only
//! move, so what waits for an animation's end goes on.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use nsh_animation::{Finished, Graph, NodeId as ValueId};
use nsh_core::{MountItem, NodeId, Point, Points};

/// A display frame at 60 Hz.
pub(crate) const FRAME: Duration = Duration::from_micros(16_667);

pub(crate) struct Animator {
    graph: Graph,
    finished: Vec<Finished>,
    /// Scroll view -> (its offset's value, horizontal).
    scrolls: HashMap<NodeId, (ValueId, bool)>,
    /// When the graph's clock started (its times are seconds since).
    epoch: Instant,
    /// When the next frame is due, while anything animates.
    next: Option<Instant>,
}

impl Default for Animator {
    fn default() -> Self {
        Self {
            graph: Graph::default(),
            finished: Vec::new(),
            scrolls: HashMap::new(),
            epoch: Instant::now(),
            next: None,
        }
    }
}

impl Animator {
    /// Takes in one item of a batch: animation ops and scroll values.
    pub(crate) fn apply(&mut self, item: &MountItem) {
        match item {
            MountItem::Animation(op) => self.graph.apply(op.clone(), &mut self.finished),
            MountItem::ScrollConfig {
                id,
                horizontal,
                scroll_value,
                ..
            } => match scroll_value {
                Some(value) => {
                    self.scrolls.insert(*id, (*value, *horizontal));
                }
                None => {
                    self.scrolls.remove(id);
                }
            },
            MountItem::Delete { id } => {
                self.scrolls.remove(id);
            }
            _ => return,
        }
        self.wake();
    }

    /// A scroll view moved: feeds its offset's value.
    pub(crate) fn scrolled(&mut self, id: NodeId, offset: Point<Points>) {
        if let Some(&(value, horizontal)) = self.scrolls.get(&id) {
            let along = if horizontal { offset.x } else { offset.y };
            self.graph.set(value, along.0, &mut self.finished);
            self.wake();
        }
    }

    /// Starts the frames when something animates.
    fn wake(&mut self) {
        if self.graph.is_animating() && self.next.is_none() {
            self.next = Some(Instant::now());
        }
    }

    /// Whether anything animates.
    pub(crate) fn running(&self) -> bool {
        self.next.is_some()
    }

    /// When the next frame is due, while anything animates.
    pub(crate) fn next_frame(&self) -> Option<Instant> {
        self.next
    }

    /// Runs the frames that are due.
    pub(crate) fn tick(&mut self) {
        let Some(next) = self.next else {
            return;
        };
        let now = Instant::now();
        if now < next {
            return;
        }
        self.graph
            .tick((now - self.epoch).as_secs_f64(), &mut self.finished);
        self.next = self.graph.is_animating().then(|| (next + FRAME).max(now));
    }

    /// The animations that finished since the last call.
    pub(crate) fn take_finished(&mut self) -> Vec<Finished> {
        std::mem::take(&mut self.finished)
    }
}
