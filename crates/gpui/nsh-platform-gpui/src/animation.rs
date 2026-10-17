//! The native animation graph on the main thread: the frame ticker
//! ticks it, drags and scroll offsets update it, and bound properties are
//! written into the views' [`crate::view::AnimValues`] (the Android
//! animator's design, `nsh-platform-android/src/anim.rs`).

use nsh_animation::{AnimationProp, AnimationValue, Finished, Graph, NodeId};
use nsh_core::{Color, GestureValues, MountItem};
use rustc_hash::FxHashMap;

use crate::{mount::Mounter, view::AnimValues};

#[derive(Default)]
struct Gestures {
    values: GestureValues,
    /// Values when the gesture began: pan x, pan y, pinch, rotation.
    start: [f32; 4],
}

#[derive(Default)]
pub(crate) struct UiAnimation {
    graph: Graph,
    pub(crate) finished: Vec<Finished>,
    /// View id -> bound properties.
    bindings: FxHashMap<u32, Vec<(AnimationProp, NodeId)>>,
    gestures: FxHashMap<u32, Gestures>,
    /// Scroll view id -> (value node, horizontal).
    scrolls: FxHashMap<u32, (NodeId, bool)>,
    /// A view's canvas slot values while they are written (reused: written
    /// on every frame).
    canvas_args: Vec<(u16, AnimationValue)>,
}

impl UiAnimation {
    /// Looks at one item of a batch: animation ops and bindings, gesture
    /// and scroll values, and deletions (which drop the view's entries).
    pub(crate) fn apply(&mut self, item: &MountItem) {
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
                ..
            } => match scroll_value {
                Some(node) => {
                    self.scrolls.insert(*id, (*node, *horizontal));
                }
                None => {
                    self.scrolls.remove(id);
                }
            },
            MountItem::Delete { id } => {
                self.bindings.remove(id);
                self.gestures.remove(id);
                self.scrolls.remove(id);
            }
            _ => {}
        }
    }

    pub(crate) fn tick(&mut self, now: f64) -> bool {
        self.graph.tick(now, &mut self.finished)
    }

    pub(crate) fn animating(&self) -> bool {
        self.graph.is_animating()
    }

    /// A scroll view moved: feeds its scroll value.
    pub(crate) fn scrolled(&mut self, id: u32, x: f32, y: f32) -> bool {
        let Some(&(node, horizontal)) = self.scrolls.get(&id) else {
            return false;
        };
        self.graph
            .set(node, if horizontal { x } else { y }, &mut self.finished);
        true
    }

    /// A drag began: grabbing an animating value stops it where it is.
    pub(crate) fn pan_began(&mut self, id: u32) {
        let graph = &self.graph;
        if let Some(g) = self.gestures.get_mut(&id) {
            g.start[0] = g.values.pan_x.and_then(|n| graph.value(n)).unwrap_or(0.0);
            g.start[1] = g.values.pan_y.and_then(|n| graph.value(n)).unwrap_or(0.0);
        }
    }

    /// The drag moved by `(tx, ty)` points since it began.
    pub(crate) fn pan_moved(&mut self, id: u32, tx: f32, ty: f32) {
        let Some(g) = self.gestures.get(&id) else {
            return;
        };
        let (x, y, start) = (g.values.pan_x, g.values.pan_y, g.start);
        if let Some(n) = x {
            self.graph.set(n, start[0] + tx, &mut self.finished);
        }
        if let Some(n) = y {
            self.graph.set(n, start[1] + ty, &mut self.finished);
        }
    }

    /// The values the drag on `id` moves (`pan.x`, `pan.y`).
    pub(crate) fn pan_values(&self, id: u32) -> (Option<f32>, Option<f32>) {
        let Some(g) = self.gestures.get(&id) else {
            return (None, None);
        };
        (
            g.values.pan_x.and_then(|n| self.graph.value(n)),
            g.values.pan_y.and_then(|n| self.graph.value(n)),
        )
    }

    /// A pinch (`rotation` false) or two-finger rotation began.
    pub(crate) fn transform_began(&mut self, id: u32, rotation: bool) {
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
    pub(crate) fn transform_moved(&mut self, id: u32, rotation: bool, amount: f32) {
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

    /// Writes every bound property's current value into its view.
    pub(crate) fn write(&mut self, mounter: &Mounter) {
        let canvas = &mut self.canvas_args;
        for (&id, bindings) in &self.bindings {
            let Some(view) = &mounter.views.get(id) else {
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
                    AnimationProp::ShadowRadius => values.shadow_radius = Some(value.number()),
                    AnimationProp::Transform(i) => {
                        if let Some(t) = values.transform.get_mut(i as usize) {
                            *t = Some(value.number());
                        }
                    }
                    AnimationProp::CanvasArg(k) => canvas.push((k, value)),
                }
            }
            let moved = {
                let mut s = view.state_mut();
                let moved = s.anim.transform != values.transform;
                let changed = moved || s.anim != values;
                s.anim = values;
                changed.then_some(moved)
            };
            match moved {
                Some(true) => view.moved(),
                Some(false) => view.changed(),
                None => {}
            }
            if moved.is_some() {
                crate::text_input::animated(view);
            }
            for &(k, value) in canvas.iter() {
                crate::canvas::set_arg(view, k, value);
            }
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
