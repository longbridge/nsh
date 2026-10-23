//! The graph on a desktop backend's main thread, without the views: the
//! backend's frame clock ticks it, drags and scroll offsets update it, and
//! [`Animator::write`] hands each bound view its [`AnimValues`] (the
//! Android animator's design, `nsh-platform-android/src/anim.rs`). The
//! backends store the values in their views and redraw them.

use rustc_hash::FxHashMap;

use super::{AnimationProp, AnimationValue, Finished, Graph, NodeId};
use crate::{
    Color, GestureValues, MAX_TRANSFORMS, MountItem,
    geometry::{Point, Points},
};

/// Values native animations set on a view; they win over the style's.
#[non_exhaustive]
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct AnimValues {
    /// Animated opacity.
    pub opacity: Option<f32>,
    /// Animated background color.
    pub background: Option<Color>,
    /// Animated entries of the transform list (by position).
    pub transform: [Option<f32>; MAX_TRANSFORMS],
    /// Animated border color (every side).
    pub border_color: Option<Color>,
    /// Animated text color (a text view's runs, an input's text).
    pub color: Option<Color>,
    /// Animated shadow color.
    pub shadow_color: Option<Color>,
    /// Animated shadow opacity.
    pub shadow_opacity: Option<f32>,
    /// Animated shadow blur radius.
    pub shadow_radius: Option<Points>,
}

#[derive(Default)]
struct Gestures {
    values: GestureValues,
    /// Values when the gesture began: pan x, pan y, pinch, rotation.
    start: [f32; 4],
}

/// The graph of one surface, with what binds it to views, gestures and
/// scroll views.
#[non_exhaustive]
#[derive(Default)]
pub struct Animator {
    graph: Graph,
    /// Animations that finished, for the backend to report to JS.
    pub finished: Vec<Finished>,
    /// View id -> bound properties.
    bindings: FxHashMap<u32, Vec<(AnimationProp, NodeId)>>,
    gestures: FxHashMap<u32, Gestures>,
    /// Scroll view id -> (value node, horizontal).
    scrolls: FxHashMap<u32, (NodeId, bool)>,
    /// Vertical scroll view id -> pull value node.
    pulls: FxHashMap<u32, NodeId>,
    /// A view's canvas slot values while they are written (reused: written
    /// on every frame).
    canvas_args: Vec<(u16, AnimationValue)>,
}

impl Animator {
    /// Looks at one item of a batch: animation ops and bindings, gesture
    /// and scroll values, and deletions (which drop the view's entries).
    pub fn apply(&mut self, item: &MountItem) {
        match item {
            MountItem::Animation(op) => self.graph.apply(op.clone(), &mut self.finished),
            MountItem::AnimationBindings { id, bindings } => {
                if bindings.is_empty() {
                    self.bindings.remove(id);
                } else {
                    self.bindings.insert(*id, bindings.clone());
                }
            }
            MountItem::Gestures { id, values } => {
                if values.is_empty() {
                    self.gestures.remove(id);
                } else {
                    self.gestures.entry(*id).or_default().values = *values;
                }
            }
            MountItem::ScrollConfig {
                id,
                horizontal,
                scroll_value,
                pull_value,
                ..
            } => {
                match scroll_value {
                    Some(node) => {
                        self.scrolls.insert(*id, (*node, *horizontal));
                    }
                    None => {
                        self.scrolls.remove(id);
                    }
                }
                match pull_value.filter(|_| !*horizontal) {
                    Some(node) => {
                        self.pulls.insert(*id, node);
                    }
                    None => {
                        self.pulls.remove(id);
                    }
                }
            }
            MountItem::Delete { id } => {
                self.bindings.remove(id);
                self.gestures.remove(id);
                self.scrolls.remove(id);
                self.pulls.remove(id);
            }
            _ => {}
        }
    }

    /// Advances the graph to `now` (the graph's clock); true while it
    /// animates.
    pub fn tick(&mut self, now: f64) -> bool {
        self.graph.tick(now, &mut self.finished)
    }

    /// True while a driver of the graph runs.
    pub fn animating(&self) -> bool {
        self.graph.is_animating()
    }

    /// A scroll view moved to `offset`: feeds its scroll value. False if no
    /// value is bound to it.
    pub fn scrolled(&mut self, id: u32, offset: Point<Points>) -> bool {
        let Some(&(node, horizontal)) = self.scrolls.get(&id) else {
            return false;
        };
        let value = if horizontal { offset.x } else { offset.y };
        self.graph.set(node, value.0, &mut self.finished);
        true
    }

    /// A vertical scroll view is pulled `distance` past its top (0 when
    /// not): feeds its pull value. False if no value is bound to it.
    pub fn pulled(&mut self, id: u32, distance: Points) -> bool {
        let Some(&node) = self.pulls.get(&id) else {
            return false;
        };
        self.graph
            .set(node, distance.0.max(0.0), &mut self.finished);
        true
    }

    /// A drag began: grabbing an animating value stops it where it is.
    pub fn pan_began(&mut self, id: u32) {
        let graph = &self.graph;
        if let Some(g) = self.gestures.get_mut(&id) {
            g.start[0] = g.values.pan_x.and_then(|n| graph.value(n)).unwrap_or(0.0);
            g.start[1] = g.values.pan_y.and_then(|n| graph.value(n)).unwrap_or(0.0);
        }
    }

    /// The drag moved by `translation` since it began.
    pub fn pan_moved(&mut self, id: u32, translation: Point<Points>) {
        let Some(g) = self.gestures.get(&id) else {
            return;
        };
        let (x, y, start) = (g.values.pan_x, g.values.pan_y, g.start);
        if let Some(n) = x {
            self.graph
                .set(n, start[0] + translation.x.0, &mut self.finished);
        }
        if let Some(n) = y {
            self.graph
                .set(n, start[1] + translation.y.0, &mut self.finished);
        }
    }

    /// The values the drag on `id` moves (`pan.x`, `pan.y`).
    pub fn pan_values(&self, id: u32) -> (Option<f32>, Option<f32>) {
        let Some(g) = self.gestures.get(&id) else {
            return (None, None);
        };
        (
            g.values.pan_x.and_then(|n| self.graph.value(n)),
            g.values.pan_y.and_then(|n| self.graph.value(n)),
        )
    }

    /// A pinch (`rotation` false) or two-finger rotation began.
    pub fn transform_began(&mut self, id: u32, rotation: bool) {
        let graph = &self.graph;
        if let Some(g) = self.gestures.get_mut(&id) {
            let (node, slot, default) = if rotation {
                (g.values.rotation, 3, 0.0)
            } else {
                (g.values.pinch, 2, 1.0)
            };
            g.start[slot] = node.and_then(|n| graph.value(n)).unwrap_or(default);
        }
    }

    /// The pinch scale (or rotation in degrees) since the gesture began.
    pub fn transform_moved(&mut self, id: u32, rotation: bool, amount: f32) {
        let Some(g) = self.gestures.get(&id) else {
            return;
        };
        if rotation {
            if let Some(n) = g.values.rotation {
                let v = g.start[3] + amount;
                self.graph.set(n, v, &mut self.finished);
            }
        } else if let Some(n) = g.values.pinch {
            let v = g.start[2] * amount;
            self.graph.set(n, v, &mut self.finished);
        }
    }

    /// Evaluates every bound property: for each view `find` knows, `set`
    /// gets its values and the values of its canvas slots.
    pub fn write<V>(
        &mut self,
        mut find: impl FnMut(u32) -> Option<V>,
        mut set: impl FnMut(V, AnimValues, &[(u16, AnimationValue)]),
    ) {
        let canvas = &mut self.canvas_args;
        for (&id, bindings) in &self.bindings {
            let Some(view) = find(id) else {
                continue;
            };
            let mut values = AnimValues::default();
            canvas.clear();
            for &(prop, node) in bindings {
                let value = self.graph.eval(node);
                match prop {
                    AnimationProp::Opacity => values.opacity = Some(value.number()),
                    AnimationProp::BackgroundColor => values.background = Some(color(value)),
                    AnimationProp::BorderColor => values.border_color = Some(color(value)),
                    AnimationProp::Color => values.color = Some(color(value)),
                    AnimationProp::ShadowColor => values.shadow_color = Some(color(value)),
                    AnimationProp::ShadowOpacity => values.shadow_opacity = Some(value.number()),
                    AnimationProp::ShadowRadius => {
                        values.shadow_radius = Some(Points(value.number()))
                    }
                    AnimationProp::Transform(i) => {
                        if let Some(t) = values.transform.get_mut(i as usize) {
                            *t = Some(value.number());
                        }
                    }
                    AnimationProp::CanvasArg(k) => canvas.push((k, value)),
                }
            }
            set(view, values, canvas);
        }
    }
}

/// A bound color: animated colors are `0xRRGGBBAA` values.
fn color(value: AnimationValue) -> Color {
    match value {
        AnimationValue::Color(c) => Color(c),
        other => Color(other.number() as u32),
    }
}
