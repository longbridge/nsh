//! The native animation graph on the UI thread: `Choreographer` ticks,
//! drags and scroll offsets update it; changed view properties go back to
//! Java as `[animating, (view, prop, value)*]` float arrays.

use nsh_core::{
    GestureValues, MAX_TRANSFORMS, MountItem, Transform,
    animation::{AnimationProp, AnimationValue, Finished, Graph, NodeId},
};
use rustc_hash::FxHashMap;

// Java-side property codes (`Mounter.java`).
const OPACITY: f32 = 0.0;
const BACKGROUND: f32 = 7.0;
/// Every border side's color (ARGB bits).
const BORDER_COLOR: f32 = 11.0;
/// A text's color (ARGB bits).
const TEXT_COLOR: f32 = 12.0;
/// How much further a sticky cell sticks (points; `StickyCell::offset`):
/// `ScrollBox.java` pins it again.
const STICKY_OFFSET: f32 = 15.0;
/// Canvas animated value slot `n` is `CANVAS_ARG + n` (a color as ARGB bits).
const CANVAS_ARG: f32 = 1000.0;
/// A native component's animated prop slot `n` holding a number is
/// `COMPONENT_NUMBER + n` (past the canvas slots' codes; exact in an f32).
const COMPONENT_NUMBER: f32 = 70000.0;
/// The same holding a color (ARGB bits) is `COMPONENT_COLOR + n`.
const COMPONENT_COLOR: f32 = 140000.0;
/// A color value as Java's ARGB bits.
fn argb(value: AnimationValue) -> f32 {
    match value {
        AnimationValue::Color(c) => f32::from_bits(c.rotate_right(8)),
        other => other.number(),
    }
}

/// Codes of the [`view_transform`] values, in order.
const TRANSFORM_CODES: [f32; 8] = [1.0, 2.0, 4.0, 5.0, 6.0, 8.0, 9.0, 10.0];

/// A transform list as Android view properties: translation x / y (dp),
/// scale x / y, rotation, rotation x / y (degrees) and the perspective
/// term (`-1 / distance`, 0 for none; Java turns it into a camera
/// distance). `value(i)` overrides entry `i` (an animated value). Skew has
/// no view property and is dropped.
pub(crate) fn view_transform(t: &Transform, value: impl Fn(usize) -> Option<f32>) -> [f32; 8] {
    match t.matrix(value).decompose() {
        Some(d) => [
            d.translate[0].0,
            d.translate[1].0,
            d.scale[0] as f32,
            d.scale[1] as f32,
            d.rotate[2].to_degrees(),
            d.rotate[0].to_degrees(),
            d.rotate[1].to_degrees(),
            d.perspective.map_or(0.0, |distance| -1.0 / distance.0),
        ],
        // A zero scale: invisible.
        None => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    }
}

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
    /// View id -> static transform list, whose animated entries the
    /// bindings replace.
    statics: FxHashMap<u32, Transform>,
    gestures: FxHashMap<u32, Gestures>,
    /// Scroll view id -> (value node, horizontal).
    scrolls: FxHashMap<u32, (NodeId, bool)>,
    /// Vertical scroll view id -> pull value node.
    pulls: FxHashMap<u32, NodeId>,
    /// Scroll view id -> its sticky cells that stick an animated value
    /// further in, with that value's node.
    sticky: FxHashMap<u32, Vec<(u32, NodeId)>>,
    /// The shader views, whose animated slots are written here.
    pub(crate) shaders: crate::shader::Shaders,
    /// Target Choreographer vsync timestamp (seconds) for the next capped-rate
    /// `Graph::tick_frame` call, carrying fractional phase across frames; cleared
    /// when animations or gestures mutate so new work ticks on the next frame.
    next_tick: Option<f64>,
}

impl UiAnimation {
    /// Forgets the running app's state (a new app runs), keeping the
    /// shaders' GPU device.
    pub(crate) fn reset(&mut self) {
        let mut shaders = std::mem::take(&mut self.shaders);
        shaders.clear();
        *self = UiAnimation {
            shaders,
            ..UiAnimation::default()
        };
    }

    pub(crate) fn apply(&mut self, item: MountItem) {
        match item {
            MountItem::Animation(op) => {
                self.next_tick = None;
                self.graph.apply(op, &mut self.finished);
            }
            MountItem::AnimationBindings { id, bindings } => {
                if bindings.is_empty() {
                    self.bindings.remove(&id);
                } else {
                    self.bindings.insert(id, bindings);
                }
            }
            MountItem::ViewStyle { id, style } => {
                self.statics.insert(id, style.transform);
            }
            MountItem::Gestures { id, values } => {
                if values.is_empty() {
                    self.gestures.remove(&id);
                } else {
                    self.gestures.entry(id).or_default().values = values;
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
                        self.scrolls.insert(id, (node, horizontal));
                    }
                    None => {
                        self.scrolls.remove(&id);
                    }
                }
                match pull_value.filter(|_| !horizontal) {
                    Some(node) => {
                        self.pulls.insert(id, node);
                    }
                    None => {
                        self.pulls.remove(&id);
                    }
                }
            }
            MountItem::StickyCells { id, cells } => {
                let animated: Vec<(u32, NodeId)> = cells
                    .iter()
                    .filter_map(|c| Some((c.cell, c.offset?)))
                    .collect();
                if animated.is_empty() {
                    self.sticky.remove(&id);
                } else {
                    self.sticky.insert(id, animated);
                }
            }
            MountItem::Shader { id, layer } => self.shaders.set(id, layer),
            MountItem::NavShader { id, shader } => self.shaders.set_nav(id, shader),
            MountItem::Delete { id } => {
                self.shaders.delete(id);
                self.bindings.remove(&id);
                self.statics.remove(&id);
                self.gestures.remove(&id);
                self.scrolls.remove(&id);
                self.pulls.remove(&id);
                self.sticky.remove(&id);
            }
            _ => {}
        }
    }

    /// Whether the `Choreographer` frame at vsync timestamp `vsync` (seconds)
    /// should advance the animation graph. When every running animation asks
    /// for a capped `frameRate` (and no shader view wants a frame),
    /// intermediate vsyncs are skipped so Java does not touch view properties
    /// or trigger a Skia frame.
    pub(crate) fn should_tick(&self, vsync: f64) -> bool {
        if self.shaders.wants_frame() {
            return true;
        }
        let Some(hz) = self.graph.frame_rate() else {
            return true;
        };
        let Some(next) = self.next_tick else {
            return true;
        };
        let period = 1.0 / f64::from(hz);
        let slack = 0.002_f64.min(period * 0.25);
        // Tick once the scheduled deadline arrives (within vsync slack), or if
        // the vsync clock moves backwards past the previous period.
        vsync >= next - slack || vsync < next - period * 2.0
    }

    /// A `Choreographer` frame, the only thing that ticks it
    /// ([`Graph::tick_frame`]). `now` is the host epoch time used by the graph;
    /// `vsync` is the Choreographer frame timestamp used for `frameRate` pacing.
    pub(crate) fn tick(&mut self, now: f64, vsync: f64) -> bool {
        let prev_rate = self.graph.frame_rate();
        let changed = self.graph.tick_frame(now, &mut self.finished);
        self.next_tick = self.graph.frame_rate().map(|hz| {
            let period = 1.0 / f64::from(hz);
            let slack = 0.002_f64.min(period * 0.25);
            // Carry the fractional deadline across accepted vsyncs so rates that
            // do not divide the display refresh rate (e.g. 24 Hz on 60 Hz, 60 Hz
            // on 144 Hz) preserve their average cadence without burst catch-up.
            let base = self
                .next_tick
                .filter(|&prev| {
                    prev_rate == Some(hz) && prev <= vsync + slack && vsync - prev < period
                })
                .unwrap_or(vsync);
            base + period
        });
        changed
    }

    pub(crate) fn animating(&self) -> bool {
        self.graph.is_animating()
    }

    /// Whether the graph runs an animation that ends by itself (not one
    /// looping forever, which no test step waits for).
    #[cfg(feature = "dev")]
    pub(crate) fn finishing(&self) -> bool {
        self.graph.is_finishing()
    }

    /// Scroll view `id` came to rest: the snapping values that follow its
    /// scroll value settle. True if one starts moving.
    pub(crate) fn scroll_ended(&mut self, id: u32) -> bool {
        let Some(&(node, _)) = self.scrolls.get(&id) else {
            return false;
        };
        self.next_tick = None;
        self.graph.settle(node)
    }

    /// A scroll view moved: feeds its scroll value.
    pub(crate) fn scrolled(&mut self, id: u32, x: f32, y: f32) -> bool {
        let Some(&(node, horizontal)) = self.scrolls.get(&id) else {
            return false;
        };
        self.next_tick = None;
        self.graph
            .set(node, if horizontal { x } else { y }, &mut self.finished);
        true
    }

    /// A vertical scroll view is pulled `distance` points past its top:
    /// feeds its pull value.
    pub(crate) fn pulled(&mut self, id: u32, distance: f32) -> bool {
        let Some(&node) = self.pulls.get(&id) else {
            return false;
        };
        self.next_tick = None;
        self.graph.set(node, distance.max(0.0), &mut self.finished);
        true
    }

    /// Whether view `id` is dragged by animated values.
    pub(crate) fn has_pan(&self, id: u32) -> bool {
        self.gestures
            .get(&id)
            .is_some_and(|g| g.values.pan_x.is_some() || g.values.pan_y.is_some())
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
        self.next_tick = None;
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
        self.next_tick = None;
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

    /// Writes the shader views' animated slots from the graph.
    fn write_shader_slots(&mut self) {
        for (id, view) in self.shaders.views_mut() {
            let Some(bindings) = self.bindings.get(&id) else {
                continue;
            };
            for &(prop, node) in bindings {
                if let AnimationProp::CanvasArg(k) = prop {
                    view.set_slot(k, self.graph.eval(node));
                }
            }
        }
    }

    /// `[animating, (view, prop, value)*]` for every bound property; the
    /// shader views' bound values are written into them instead, and keep
    /// frames coming (`animating`) while one has a frame to render.
    /// Component slots accepted by `take_component_slot` are handled in
    /// Rust and skipped from the Java float stream.
    pub(crate) fn outputs(
        &mut self,
        mut take_component_slot: impl FnMut(u32, u16, AnimationValue) -> bool,
    ) -> Vec<f32> {
        self.write_shader_slots();
        let mut out = Vec::with_capacity(1 + self.bindings.len() * 12);
        out.push(if self.animating() || self.shaders.wants_frame() {
            1.0
        } else {
            0.0
        });
        for (&view, bindings) in &self.bindings {
            let v = f32::from_bits(view);
            let mut overrides = [None; MAX_TRANSFORMS];
            let mut transformed = false;
            for &(prop, node) in bindings {
                match prop {
                    AnimationProp::BorderColor | AnimationProp::Color => {
                        let code = if prop == AnimationProp::Color {
                            TEXT_COLOR
                        } else {
                            BORDER_COLOR
                        };
                        out.extend_from_slice(&[v, code, argb(self.graph.eval(node))]);
                    }
                    AnimationProp::Opacity => {
                        out.extend_from_slice(&[v, OPACITY, self.graph.eval(node).number()]);
                    }
                    AnimationProp::BackgroundColor => {
                        let value = match self.graph.eval(node) {
                            AnimationValue::Color(c) => f32::from_bits(c.rotate_right(8)),
                            other => other.number(),
                        };
                        out.extend_from_slice(&[v, BACKGROUND, value]);
                    }
                    AnimationProp::CanvasArg(_) if self.shaders.contains(view) => {}
                    AnimationProp::CanvasArg(k) => {
                        let value = match self.graph.eval(node) {
                            AnimationValue::Color(c) => f32::from_bits(c.rotate_right(8)),
                            other => other.number(),
                        };
                        out.extend_from_slice(&[v, CANVAS_ARG + k as f32, value]);
                    }
                    AnimationProp::Transform(i) => {
                        if let Some(o) = overrides.get_mut(i as usize) {
                            *o = Some(self.graph.eval(node).number());
                            transformed = true;
                        }
                    }
                    // The scroll view's (`sticky` below), never bound.
                    AnimationProp::StickyTop | AnimationProp::StickyLeft => {}
                    // Handled directly in Rust for Rust-crate components, or via Java for Java ones.
                    AnimationProp::ComponentSlot(k) => {
                        let value = self.graph.eval(node);
                        if !take_component_slot(view, k, value) {
                            match value {
                                AnimationValue::Color(c) => out.extend_from_slice(&[
                                    v,
                                    COMPONENT_COLOR + k as f32,
                                    f32::from_bits(c.rotate_right(8)),
                                ]),
                                other => out.extend_from_slice(&[
                                    v,
                                    COMPONENT_NUMBER + k as f32,
                                    other.number(),
                                ]),
                            }
                        }
                    }
                }
            }
            if transformed {
                let statics = self
                    .statics
                    .get(&view)
                    .copied()
                    .unwrap_or(Transform::IDENTITY);
                let values = view_transform(&statics, |i| overrides[i]);
                for (code, value) in TRANSFORM_CODES.iter().zip(values) {
                    out.extend_from_slice(&[v, *code, value]);
                }
            }
        }
        // Sticky cells that stick an animated value further in.
        for cells in self.sticky.values() {
            for &(cell, node) in cells {
                let value = self.graph.eval(node).number();
                out.extend_from_slice(&[f32::from_bits(cell), STICKY_OFFSET, value]);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::{
        MountItem,
        animation::{AnimationOp, Driver, Easing},
    };

    use super::UiAnimation;

    fn timing(node: u32, frame_rate: Option<f32>) -> MountItem {
        MountItem::Animation(AnimationOp::Start {
            node,
            driver: Driver::Timing {
                to: 1.0,
                duration_ms: 1000.0,
                easing: Easing::Linear,
                delay_ms: 0.0,
                frame_rate,
            },
            animation: node,
        })
    }

    #[test]
    fn throttles_capped_animations_and_preempts_for_full_rate() {
        let mut anim = UiAnimation::default();
        anim.apply(MountItem::Animation(AnimationOp::CreateValue {
            node: 1,
            value: 0.0,
        }));
        anim.apply(MountItem::Animation(AnimationOp::CreateValue {
            node: 2,
            value: 0.0,
        }));
        anim.apply(timing(1, Some(30.0)));

        // First frame always ticks immediately.
        assert!(anim.should_tick(1.000));
        anim.tick(1.000, 1.000);

        // Intermediate 120 Hz vsyncs (8.33 ms apart) are skipped until ~33.3 ms.
        assert!(!anim.should_tick(1.0083));
        assert!(!anim.should_tick(1.0167));
        assert!(!anim.should_tick(1.0250));
        assert!(anim.should_tick(1.0333));
        // Even if the callback is dispatched 3 ms late (now = 1.0363), pacing
        // records the vsync timestamp (1.0333) so the next on-time vsync at
        // 1.0667 is not skipped.
        anim.tick(1.0363, 1.0333);
        assert!(anim.should_tick(1.0667));

        // Starting an uncapped animation immediately restores full-rate ticking.
        anim.apply(timing(2, None));
        assert!(anim.should_tick(1.0340));
        anim.tick(1.0340, 1.0340);
        assert!(anim.should_tick(1.0423));
    }

    #[test]
    fn preserves_average_cadence_for_non_divisible_rates() {
        for (display_hz, requested_hz, expected_min, expected_max) in [
            (60u32, 24.0f32, 238u32, 242u32),
            (144, 60.0, 598, 602),
            (120, 30.0, 299, 301),
            (60, 60.0, 599, 601),
        ] {
            let mut anim = UiAnimation::default();
            anim.apply(MountItem::Animation(AnimationOp::CreateValue {
                node: 1,
                value: 0.0,
            }));
            anim.apply(MountItem::Animation(AnimationOp::Start {
                node: 1,
                driver: Driver::Timing {
                    to: 1.0,
                    duration_ms: 20_000.0,
                    easing: Easing::Linear,
                    delay_ms: 0.0,
                    frame_rate: Some(requested_hz),
                },
                animation: 1,
            }));
            let mut ticks = 0u32;
            let total_vsyncs = display_hz * 10;
            for i in 0..total_vsyncs {
                let vsync = 1.0 + f64::from(i) / f64::from(display_hz);
                if anim.should_tick(vsync) {
                    anim.tick(vsync, vsync);
                    ticks += 1;
                }
            }
            assert!(
                (expected_min..=expected_max).contains(&ticks),
                "display={display_hz} Hz requested={requested_hz} Hz produced {ticks} ticks in 10s"
            );
        }
    }
}
