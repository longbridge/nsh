//! Native animation values for Rust components (TS: `createNativeValue`,
//! `Animated.spring`, `interpolate`): nodes of the platform's animation
//! graph that styles, drags, scroll offsets and drawings are bound to, so
//! they move on the UI thread with no engine work per frame. Composed
//! animations ([`Animation`], TS: `Animated.sequence` and co.) run there
//! too, step after step.
//!
//! Ids are odd (JS gives its own nodes, animations and composites even
//! ones), so the graph, which indexes nodes by id, stays dense.

use std::{
    cell::Cell,
    fmt,
    rc::{Rc, Weak},
};

use nsh_animation::{
    AnimationId, AnimationOp, Composite, Driver, Easing, Extrapolate, MathOp, Operand, Output,
};

use super::{
    Color,
    task::{Action, Answer, Executor},
};

/// A node of the graph, released when its last handle goes.
pub(crate) struct Node {
    id: nsh_animation::NodeId,
    executor: Weak<Executor>,
    /// The nodes it derives from, alive while it is: a chain such as
    /// `v.mul(TAU).sub(1.0)` keeps no handle of its middle node.
    _inputs: Vec<Rc<Node>>,
}

impl Node {
    fn new(
        executor: &Rc<Executor>,
        op: impl FnOnce(nsh_animation::NodeId) -> AnimationOp,
        inputs: Vec<Rc<Node>>,
    ) -> Rc<Self> {
        let id = executor.next_animation_id();
        executor.act(Action::Animation(op(id)));
        Rc::new(Self {
            id,
            executor: Rc::downgrade(executor),
            _inputs: inputs,
        })
    }

    fn act(&self, op: AnimationOp) {
        if let Some(executor) = self.executor.upgrade() {
            executor.act(Action::Animation(op));
        }
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.act(AnimationOp::Release { node: self.id });
    }
}

/// `op` (a node-making operation) making node `id`.
fn with_node(mut op: AnimationOp, id: nsh_animation::NodeId) -> AnimationOp {
    match &mut op {
        AnimationOp::CreateValue { node, .. }
        | AnimationOp::CreateInterpolate { node, .. }
        | AnimationOp::CreateDiffClamp { node, .. }
        | AnimationOp::CreateMath { node, .. } => *node = id,
        _ => {}
    }
    op
}

/// Derived nodes by the operation that made them (its node id 0), while
/// alive.
#[derive(Default)]
pub(crate) struct Derived(pub(crate) Vec<(AnimationOp, Weak<Node>)>);

impl Derived {
    pub(crate) fn get(&mut self, key: &AnimationOp) -> Option<Rc<Node>> {
        self.0.retain(|(_, n)| n.strong_count() > 0);
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, n)| n.upgrade())
    }
}

/// A value of the graph: [`NativeValue`] or [`NativeNode`], to bind
/// (`Styled::bind_opacity`) or derive from.
pub trait Animated {
    /// The node.
    fn native_node(&self) -> NativeNode;
}

/// A node derived from values (an interpolation, arithmetic): bound like a
/// value, moving when they move.
#[derive(Clone)]
pub struct NativeNode(Rc<Node>);

impl NativeNode {
    pub(crate) fn id(&self) -> nsh_animation::NodeId {
        self.0.id
    }

    fn executor(&self) -> Option<Rc<Executor>> {
        self.0.executor.upgrade()
    }

    /// The node `op` makes from this one: the live one made by the same
    /// operation if there is one (a region or a row made again derives it
    /// again and gets the same node, so its bindings stay as they are).
    fn derive(
        &self,
        op: impl FnOnce(nsh_animation::NodeId) -> AnimationOp,
        other: Option<&NativeNode>,
    ) -> NativeNode {
        let Some(executor) = self.executor() else {
            // The surface is gone: a node nothing reads.
            return self.clone();
        };
        let key = op(0);
        if let Some(node) = executor.derived(&key) {
            return NativeNode(node);
        }
        let mut inputs = vec![self.0.clone()];
        inputs.extend(other.map(|o| o.0.clone()));
        let node = Node::new(&executor, |id| with_node(key.clone(), id), inputs);
        executor.remember_derived(key, &node);
        NativeNode(node)
    }

    /// Maps the value through `input` breakpoints (ascending) to numbers,
    /// clamped at both ends: `x.interpolate(&[0.0, 300.0], &[1.0, 0.0])`.
    pub fn interpolate(&self, input: &[f32], output: &[f32]) -> NativeNode {
        self.interpolate_with(
            input,
            Output::Numbers(output.to_vec()),
            Interpolation::default(),
        )
    }

    /// Maps the value to colors (`backgroundColor` bindings).
    pub fn interpolate_colors(&self, input: &[f32], colors: &[Color]) -> NativeNode {
        let colors = colors.iter().map(|c| c.0).collect();
        self.interpolate_with(input, Output::Colors(colors), Interpolation::default())
    }

    /// [`Self::interpolate`] with any output, extrapolation on each side
    /// and curve (TS: `interpolate({ extrapolateLeft, extrapolateRight,
    /// easing })`).
    pub fn interpolate_with(
        &self,
        input: &[f32],
        output: Output,
        interpolation: Interpolation,
    ) -> NativeNode {
        let source = self.id();
        let input_range = input.to_vec();
        self.derive(
            move |node| AnimationOp::CreateInterpolate {
                node,
                input: source,
                input_range,
                output,
                extrapolate_left: interpolation.extrapolate_left,
                extrapolate_right: interpolation.extrapolate_right,
                easing: interpolation.easing,
            },
            None,
        )
    }

    fn math(&self, op: MathOp, other: AnimationOperand) -> NativeNode {
        let a = Operand::Node(self.id());
        let b = other.operand;
        self.derive(
            move |node| AnimationOp::CreateMath { node, op, a, b },
            other.node.as_ref(),
        )
    }

    /// The sum with `other`.
    pub fn add(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.math(MathOp::Add, other.into())
    }

    /// The difference with `other`.
    pub fn sub(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.math(MathOp::Sub, other.into())
    }

    /// The product with `other`.
    pub fn mul(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.math(MathOp::Mul, other.into())
    }

    /// The quotient by `other`.
    pub fn div(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.math(MathOp::Div, other.into())
    }

    /// The remainder by `other`.
    pub fn modulo(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.math(MathOp::Mod, other.into())
    }

    /// Follows the value's changes but stays within `min..=max` (a header
    /// hiding as content scrolls up).
    pub fn diff_clamp(&self, min: f32, max: f32) -> NativeNode {
        let input = self.id();
        self.derive(
            move |node| AnimationOp::CreateDiffClamp {
                node,
                input,
                min,
                max,
            },
            None,
        )
    }
}

impl Animated for NativeNode {
    fn native_node(&self) -> NativeNode {
        self.clone()
    }
}

impl PartialEq for NativeNode {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl fmt::Debug for NativeNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NativeNode").field(&self.id()).finish()
    }
}

/// An operand of arithmetic on nodes: a number or another node.
#[derive(Debug, Clone)]
pub struct AnimationOperand {
    operand: Operand,
    /// The node, kept alive by what derives from it.
    node: Option<NativeNode>,
}

impl From<f32> for AnimationOperand {
    fn from(n: f32) -> Self {
        Self {
            operand: Operand::Const(n),
            node: None,
        }
    }
}

impl<A: Animated> From<&A> for AnimationOperand {
    fn from(a: &A) -> Self {
        let node = a.native_node();
        Self {
            operand: Operand::Node(node.id()),
            node: Some(node),
        }
    }
}

/// How [`NativeNode::interpolate_with`] maps: what it does outside the
/// input range on each side, and the curve within each segment (TS:
/// `extrapolateLeft`, `extrapolateRight`, `easing`). By default it clamps
/// on both sides, linearly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interpolation {
    /// Below the first breakpoint.
    pub extrapolate_left: Extrapolate,
    /// Above the last breakpoint.
    pub extrapolate_right: Extrapolate,
    /// The curve within each segment.
    pub easing: Easing,
}

impl Interpolation {
    /// `extrapolate` on both sides, linearly (TS: `extrapolate`).
    pub const fn extrapolate(extrapolate: Extrapolate) -> Self {
        Self {
            extrapolate_left: extrapolate,
            extrapolate_right: extrapolate,
            easing: Easing::Linear,
        }
    }
}

impl Default for Interpolation {
    fn default() -> Self {
        Self::extrapolate(Extrapolate::Clamp)
    }
}

/// A settable, animatable value (made with
/// [`super::ambient::native_value`]): set it, animate it, bind it, derive
/// from it. Clones are the same value.
#[derive(Clone, PartialEq, Debug)]
pub struct NativeValue(NativeNode);

/// How a spring moves (TS's defaults: stiffness 170, damping 26, mass 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// Stiffness `k`.
    pub stiffness: f32,
    /// Damping `c`.
    pub damping: f32,
    /// Mass `m`.
    pub mass: f32,
    /// The velocity it starts with (per second); `None` keeps the current
    /// one.
    pub velocity: Option<f32>,
}

impl Default for Spring {
    fn default() -> Self {
        Self {
            stiffness: 170.0,
            damping: 26.0,
            mass: 1.0,
            velocity: None,
        }
    }
}

/// How a timing animation moves (TS's defaults: 300 ms, ease in-out, no
/// delay).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    /// How long it takes, in milliseconds.
    pub duration_ms: f32,
    /// The curve.
    pub easing: Easing,
    /// How long it waits before it starts, in milliseconds.
    pub delay_ms: f32,
}

impl Timing {
    /// `duration_ms` along `easing`, with no delay.
    pub const fn new(duration_ms: f32, easing: Easing) -> Self {
        Self {
            duration_ms,
            easing,
            delay_ms: 0.0,
        }
    }

    fn driver(self, to: f32) -> Driver {
        Driver::Timing {
            to,
            duration_ms: self.duration_ms,
            easing: self.easing,
            delay_ms: self.delay_ms,
        }
    }
}

impl Default for Timing {
    fn default() -> Self {
        Self::new(300.0, Easing::EaseInOut)
    }
}

impl Spring {
    fn driver(self, to: f32) -> Driver {
        Driver::Spring {
            to,
            stiffness: self.stiffness,
            damping: self.damping,
            mass: self.mass,
            velocity: self.velocity,
        }
    }
}

/// The values `driver` moves a value through from `from`, one per frame at
/// `fps` (1 to 1000), until it ends: the exact curve the graph runs.
fn sample(driver: Driver, from: f32, fps: f32) -> Vec<f32> {
    let step = 1.0 / f64::from(fps.clamp(1.0, 1000.0));
    nsh_animation::sample_driver(driver, from, step, nsh_animation::MAX_SAMPLES)
}

/// The values a spring moves a value through from `from` to `to`, one per
/// frame at `fps` (60 on most screens) until it rests (TS:
/// `Animated.sampleSpring`): its curve, to draw it. At most
/// [`nsh_animation::MAX_SAMPLES`].
pub fn sample_spring(from: f32, to: f32, spring: Spring, fps: f32) -> Vec<f32> {
    sample(spring.driver(to), from, fps)
}

/// The values a decay ([`NativeValue::decay`]) moves a value through from
/// `from` at `velocity` (per second), one per frame at `fps` until it
/// stops (TS: `Animated.sampleDecay`). At most
/// [`nsh_animation::MAX_SAMPLES`].
pub fn sample_decay(from: f32, velocity: f32, deceleration: f32, fps: f32) -> Vec<f32> {
    sample(
        Driver::Decay {
            velocity: Some(velocity),
            deceleration,
            clamp: None,
            bounce: 0.0,
        },
        from,
        fps,
    )
}

/// Like [`sample_decay`], within `min..=max` ([`NativeValue::decay_within`]:
/// it stops at a bound, or bounces back keeping `bounce` of its speed).
pub fn sample_decay_within(
    from: f32,
    velocity: f32,
    deceleration: f32,
    (min, max): (f32, f32),
    bounce: f32,
    fps: f32,
) -> Vec<f32> {
    sample(
        Driver::Decay {
            velocity: Some(velocity),
            deceleration,
            clamp: Some((min, max)),
            bounce,
        },
        from,
        fps,
    )
}

/// The end of an animation: `true` when it reached its end, `false` when
/// it was stopped or replaced. The animation runs either way: await this
/// in a task to follow it up, or drop it.
#[derive(Debug)]
pub struct AnimationEnd(Answer<bool>);

impl std::future::IntoFuture for AnimationEnd {
    type Output = bool;
    type IntoFuture = Answer<bool>;

    fn into_future(self) -> Answer<bool> {
        self.0
    }
}

impl NativeValue {
    pub(crate) fn new(executor: &Rc<Executor>, value: f32) -> Self {
        Self(NativeNode(Node::new(
            executor,
            |node| AnimationOp::CreateValue { node, value },
            Vec::new(),
        )))
    }

    /// The node, to derive from.
    pub fn node(&self) -> &NativeNode {
        &self.0
    }

    /// Sets the value at once, stopping its animation.
    pub fn set(&self, value: f32) {
        self.0.0.act(AnimationOp::Set {
            node: self.0.id(),
            value,
        });
    }

    /// Stops its animation where it is ([`Self::value`] then reads where
    /// that is; TS: `stop()`'s promise).
    pub fn stop(&self) {
        self.0.0.act(AnimationOp::Stop { node: self.0.id() });
    }

    /// Its value now, read on the UI thread where it moves (TS:
    /// `getValue()`): after the changes queued before it (a [`Self::stop`],
    /// a [`Self::set`]). Await it in a task.
    pub fn value(&self) -> Answer<f32> {
        let (answer, slot) = Answer::new();
        if let Some(executor) = self.0.executor() {
            executor.act(Action::ReadValue {
                node: self.0.id(),
                value: slot,
            });
        }
        answer
    }

    fn executor(&self) -> Weak<Executor> {
        self.0.0.executor.clone()
    }

    fn start(&self, driver: Driver) -> AnimationEnd {
        let (end, slot) = Answer::new();
        let end = AnimationEnd(end);
        if let Some(executor) = self.0.executor() {
            let animation = executor.next_animation_id();
            executor.act(Action::Animate {
                op: AnimationOp::Start {
                    node: self.0.id(),
                    animation,
                    driver,
                },
                animation,
                end: slot,
            });
        }
        end
    }

    /// Springs to `to`.
    pub fn spring(&self, to: f32, spring: Spring) -> AnimationEnd {
        self.start(spring.driver(to))
    }

    /// Moves to `to` in `ms` milliseconds along `easing`.
    pub fn timing(&self, to: f32, ms: f32, easing: Easing) -> AnimationEnd {
        self.timing_with(to, Timing::new(ms, easing))
    }

    /// Moves to `to` as `timing` says, after its delay.
    pub fn timing_with(&self, to: f32, timing: Timing) -> AnimationEnd {
        self.start(timing.driver(to))
    }

    /// Glides from `velocity` (per second; `None`: the current one),
    /// slowing down by `deceleration` per millisecond (0.998: a fling).
    pub fn decay(&self, velocity: Option<f32>, deceleration: f32) -> AnimationEnd {
        self.start(Driver::Decay {
            velocity,
            deceleration,
            clamp: None,
            bounce: 0.0,
        })
    }

    /// Like [`Self::decay`], within `min..=max`: it stops at a bound, or
    /// bounces back from it keeping `bounce` (0..1) of its speed.
    pub fn decay_within(
        &self,
        velocity: Option<f32>,
        deceleration: f32,
        (min, max): (f32, f32),
        bounce: f32,
    ) -> AnimationEnd {
        self.start(Driver::Decay {
            velocity,
            deceleration,
            clamp: Some((min, max)),
            bounce,
        })
    }

    /// Sets the offset added to the value (React Native's `setOffset`):
    /// bindings see value + offset, animations and `set` move the value.
    pub fn set_offset(&self, offset: f32) {
        self.0.0.act(AnimationOp::SetOffset {
            node: self.0.id(),
            offset,
        });
    }

    /// Adds the offset to the value and clears it.
    pub fn flatten_offset(&self) {
        self.0
            .0
            .act(AnimationOp::FlattenOffset { node: self.0.id() });
    }

    /// Moves the value into the offset, leaving the value 0 (a drag then
    /// starts from where the last one left it).
    pub fn extract_offset(&self) {
        self.0
            .0
            .act(AnimationOp::ExtractOffset { node: self.0.id() });
    }

    /// See [`NativeNode::interpolate`].
    pub fn interpolate(&self, input: &[f32], output: &[f32]) -> NativeNode {
        self.0.interpolate(input, output)
    }

    /// See [`NativeNode::interpolate_colors`].
    pub fn interpolate_colors(&self, input: &[f32], colors: &[Color]) -> NativeNode {
        self.0.interpolate_colors(input, colors)
    }

    /// See [`NativeNode::interpolate_with`].
    pub fn interpolate_with(
        &self,
        input: &[f32],
        output: Output,
        interpolation: Interpolation,
    ) -> NativeNode {
        self.0.interpolate_with(input, output, interpolation)
    }

    /// See [`NativeNode::diff_clamp`].
    pub fn diff_clamp(&self, min: f32, max: f32) -> NativeNode {
        self.0.diff_clamp(min, max)
    }

    /// See [`NativeNode::add`].
    pub fn add(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.0.add(other)
    }

    /// See [`NativeNode::sub`].
    pub fn sub(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.0.sub(other)
    }

    /// See [`NativeNode::mul`].
    pub fn mul(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.0.mul(other)
    }

    /// See [`NativeNode::div`].
    pub fn div(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.0.div(other)
    }

    /// See [`NativeNode::modulo`].
    pub fn modulo(&self, other: impl Into<AnimationOperand>) -> NativeNode {
        self.0.modulo(other)
    }
}

impl Animated for NativeValue {
    fn native_node(&self) -> NativeNode {
        self.0.clone()
    }
}

/// A composed animation (TS: `Animated.timing` / `spring` / `decay` /
/// `sequence` / `parallel` / `stagger` / `delay` / `loop`): a tree of
/// steps over native values, run on the UI thread, each step starting at
/// the exact end of the one before (no round trip to the engine between
/// steps).
///
/// Build it once (in the view, captured by the listeners that run it) and
/// run it any number of times: [`Self::start`] runs it (stopping a run of
/// it going on) and returns its [`AnimationEnd`], [`Self::stop`] stops it
/// where it is, [`Self::reset`] stops it and puts its values back where
/// they were when it last started. Clones are the same animation; building a group from
/// animations makes a new one (its parts are copied in).
///
/// ```ignore
/// let jump = Animation::sequence([
///     Animation::timing(&y, -36.0, Timing::new(220.0, Easing::EaseOut)),
///     Animation::spring(&y, 0.0, Spring::default()),
/// ]);
/// view().on_press(move |_: &()| {
///     let end = jump.start();
/// })
/// ```
#[derive(Clone)]
pub struct Animation(Rc<Composed>);

struct Composed {
    spec: Composite,
    /// The values it animates, kept while it is.
    values: Vec<NativeValue>,
    run: Run,
}

/// An animation's identity in the graph.
struct Run {
    executor: Weak<Executor>,
    /// Its composite key (what resets restore by); 0 until it first runs.
    key: Cell<u32>,
    /// Its last run (0: none).
    animation: Cell<AnimationId>,
}

impl Drop for Run {
    fn drop(&mut self) {
        // A run going on goes on; the graph forgets the values to reset to.
        let key = self.key.get();
        if key != 0
            && let Some(executor) = self.executor.upgrade()
        {
            executor.act(Action::Animation(AnimationOp::ReleaseComposite { key }));
        }
    }
}

impl Animation {
    fn new(spec: Composite, values: Vec<NativeValue>) -> Self {
        let executor = values
            .first()
            .map(NativeValue::executor)
            .unwrap_or_default();
        Self(Rc::new(Composed {
            spec,
            values,
            run: Run {
                executor,
                key: Cell::new(0),
                animation: Cell::new(0),
            },
        }))
    }

    fn run(value: &NativeValue, driver: Driver) -> Self {
        let node = value.0.id();
        Self::new(Composite::Run { node, driver }, vec![value.clone()])
    }

    /// `value` moving to `to` as `timing` says.
    pub fn timing(value: &NativeValue, to: f32, timing: Timing) -> Self {
        Self::run(value, timing.driver(to))
    }

    /// `value` springing to `to`.
    pub fn spring(value: &NativeValue, to: f32, spring: Spring) -> Self {
        Self::run(value, spring.driver(to))
    }

    /// `value` gliding from `velocity` (per second; `None`: the current
    /// one), slowing down by `deceleration` per millisecond (0.998: a
    /// fling).
    pub fn decay(value: &NativeValue, velocity: Option<f32>, deceleration: f32) -> Self {
        Self::run(
            value,
            Driver::Decay {
                velocity,
                deceleration,
                clamp: None,
                bounce: 0.0,
            },
        )
    }

    /// Like [`Self::decay`], within `min..=max`: it stops at a bound, or
    /// bounces back from it keeping `bounce` (0..1) of its speed.
    pub fn decay_within(
        value: &NativeValue,
        velocity: Option<f32>,
        deceleration: f32,
        (min, max): (f32, f32),
        bounce: f32,
    ) -> Self {
        Self::run(
            value,
            Driver::Decay {
                velocity,
                deceleration,
                clamp: Some((min, max)),
                bounce,
            },
        )
    }

    /// A wait of `ms` milliseconds (in a sequence).
    pub fn delay(ms: f32) -> Self {
        Self::new(Composite::Delay(ms), Vec::new())
    }

    /// The spec and values of this animation.
    fn into_parts(self) -> (Composite, Vec<NativeValue>) {
        match Rc::try_unwrap(self.0) {
            Ok(Composed { spec, values, .. }) => (spec, values),
            Err(shared) => (shared.spec.clone(), shared.values.clone()),
        }
    }

    fn group(
        animations: impl IntoIterator<Item = Animation>,
        make: impl FnOnce(Vec<Composite>) -> Composite,
    ) -> Self {
        let mut specs = Vec::new();
        let mut values: Vec<NativeValue> = Vec::new();
        for animation in animations {
            let (spec, of) = animation.into_parts();
            specs.push(spec);
            for value in of {
                if !values.contains(&value) {
                    values.push(value);
                }
            }
        }
        Self::new(make(specs), values)
    }

    /// `animations` one after the other; it stops at the first that is
    /// stopped.
    pub fn sequence(animations: impl IntoIterator<Item = Animation>) -> Self {
        Self::group(animations, Composite::Sequence)
    }

    /// `animations` all at once; one stopped stops the others (see
    /// [`Self::stop_together`]).
    pub fn parallel(animations: impl IntoIterator<Item = Animation>) -> Self {
        Self::group(animations, |children| Composite::Parallel {
            children,
            stop_together: true,
        })
    }

    /// `animations` all at once, each starting `ms` milliseconds after the
    /// one before.
    pub fn stagger(ms: f32, animations: impl IntoIterator<Item = Animation>) -> Self {
        let staged = animations
            .into_iter()
            .enumerate()
            .map(|(i, a)| Self::sequence([Self::delay(ms * i as f32), a]));
        Self::parallel(staged)
    }

    /// This animation over and over: `iterations` times, or until stopped
    /// with `None`; its values go back to where they were before each run
    /// (see [`Self::reset_before_iteration`]).
    pub fn looped(self, iterations: Option<u32>) -> Self {
        let (spec, values) = self.into_parts();
        Self::new(
            Composite::Loop {
                child: Box::new(spec),
                iterations,
                reset: true,
            },
            values,
        )
    }

    /// For a group made with [`Self::parallel`] or [`Self::stagger`]:
    /// whether one of its animations stopped stops the others (by
    /// default it does). Other animations stay as they are.
    pub fn stop_together(self, stop_together: bool) -> Self {
        let (mut spec, values) = self.into_parts();
        if let Composite::Parallel {
            stop_together: s, ..
        } = &mut spec
        {
            *s = stop_together;
        }
        Self::new(spec, values)
    }

    /// For a loop made with [`Self::looped`]: whether its values go back
    /// to where they were before each run (by default they do). Other
    /// animations stay as they are.
    pub fn reset_before_iteration(self, reset: bool) -> Self {
        let (mut spec, values) = self.into_parts();
        if let Composite::Loop { reset: r, .. } = &mut spec {
            *r = reset;
        }
        Self::new(spec, values)
    }

    /// Runs it, stopping a run of it going on (which ends `false`). It
    /// runs either way: await the end in a task to follow it up, or drop
    /// it. An animation of no values (only delays) has nothing to run on
    /// and ends at once.
    pub fn start(&self) -> AnimationEnd {
        let (end, slot) = Answer::new();
        let run = &self.0.run;
        let Some(executor) = run.executor.upgrade() else {
            if self.0.values.is_empty() {
                slot.borrow_mut().deliver(true);
            }
            return AnimationEnd(end);
        };
        self.stop();
        if run.key.get() == 0 {
            run.key.set(executor.next_animation_id());
        }
        let animation = executor.next_animation_id();
        run.animation.set(animation);
        executor.act(Action::Animate {
            op: AnimationOp::StartComposite {
                animation,
                key: run.key.get(),
                spec: self.0.spec.clone(),
            },
            animation,
            end: slot,
        });
        AnimationEnd(end)
    }

    /// Stops its run where it is (the run ends `false`).
    pub fn stop(&self) {
        let run = &self.0.run;
        let animation = run.animation.replace(0);
        if animation != 0
            && let Some(executor) = run.executor.upgrade()
        {
            executor.act(Action::Animation(AnimationOp::StopAnimation { animation }));
        }
    }

    /// Stops its run (which ends `false`) and puts its values back where
    /// they were when it last started.
    pub fn reset(&self) {
        let run = &self.0.run;
        run.animation.set(0);
        let key = run.key.get();
        if key != 0
            && let Some(executor) = run.executor.upgrade()
        {
            executor.act(Action::Animation(AnimationOp::ResetComposite { key }));
        }
    }
}

impl fmt::Debug for Animation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Animation").field(&self.0.spec).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn released(executor: &Executor) -> usize {
        executor
            .take_actions()
            .into_iter()
            .filter(|a| matches!(a, Action::Animation(AnimationOp::Release { .. })))
            .count()
    }

    #[test]
    fn a_derived_chain_keeps_its_middle_nodes() {
        let executor = Rc::new(Executor::default());
        let v = NativeValue::new(&executor, 0.5);
        // The product has no handle of its own once the difference exists.
        let end = v.node().mul(2.0).sub(1.0);
        assert_eq!(released(&executor), 0, "the product lives with the end");
        drop(end);
        assert_eq!(released(&executor), 2, "then both go");
        drop(v);
        assert_eq!(released(&executor), 1);
    }

    /// The graph operations `executor` was asked for (animations' ends
    /// dropped).
    fn ops(executor: &Executor) -> Vec<AnimationOp> {
        executor
            .take_actions()
            .into_iter()
            .filter_map(|a| match a {
                Action::Animation(op) | Action::Animate { op, .. } => Some(op),
                _ => None,
            })
            .collect()
    }

    fn timing(to: f32, ms: f32) -> Driver {
        Timing::new(ms, Easing::Linear).driver(to)
    }

    #[test]
    fn composites_describe_their_steps() {
        let executor = Rc::new(Executor::default());
        let (a, b) = (
            NativeValue::new(&executor, 0.0),
            NativeValue::new(&executor, 1.0),
        );
        let (na, nb) = (a.node().id(), b.node().id());
        let run = |node, to| Composite::Run {
            node,
            driver: timing(to, 100.0),
        };
        let step =
            |v: &NativeValue, to| Animation::timing(v, to, Timing::new(100.0, Easing::Linear));
        let staggered = Animation::stagger(50.0, [step(&a, 1.0), step(&b, 2.0)]);
        let delayed = |ms, c| Composite::Sequence(vec![Composite::Delay(ms), c]);
        assert_eq!(
            staggered.0.spec,
            Composite::Parallel {
                children: vec![delayed(0.0, run(na, 1.0)), delayed(50.0, run(nb, 2.0))],
                stop_together: true,
            }
        );
        assert_eq!(staggered.0.values, [a.clone(), b.clone()]);

        let looped = Animation::sequence([step(&a, 1.0), Animation::delay(10.0), step(&a, 0.0)])
            .looped(Some(3))
            .reset_before_iteration(false);
        assert_eq!(
            looped.0.spec,
            Composite::Loop {
                child: Box::new(Composite::Sequence(vec![
                    run(na, 1.0),
                    Composite::Delay(10.0),
                    run(na, 0.0)
                ])),
                iterations: Some(3),
                reset: false,
            }
        );
        // Each value once.
        assert_eq!(looped.0.values, std::slice::from_ref(&a));
        let apart = Animation::parallel([step(&a, 1.0)]).stop_together(false);
        assert!(matches!(
            apart.0.spec,
            Composite::Parallel {
                stop_together: false,
                ..
            }
        ));
        // A delay on its own driver.
        let delayed = Animation::timing(
            &a,
            1.0,
            Timing {
                delay_ms: 40.0,
                ..Timing::default()
            },
        );
        assert!(matches!(
            delayed.0.spec,
            Composite::Run {
                driver: Driver::Timing {
                    delay_ms: 40.0,
                    duration_ms: 300.0,
                    easing: Easing::EaseInOut,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn composites_start_stop_reset_and_release_by_key() {
        let executor = Rc::new(Executor::default());
        let x = NativeValue::new(&executor, 0.0);
        let _ = ops(&executor);
        let jump = Animation::sequence([
            Animation::timing(&x, 10.0, Timing::default()),
            Animation::spring(&x, 0.0, Spring::default()),
        ]);
        // Nothing until it runs.
        jump.stop();
        jump.reset();
        assert_eq!(ops(&executor), []);

        let _ = jump.start();
        let [
            AnimationOp::StartComposite {
                animation,
                key,
                spec,
            },
        ] = &ops(&executor)[..]
        else {
            panic!("a composite start");
        };
        assert_eq!((animation % 2, key % 2), (1, 1), "the core's ids are odd");
        assert_eq!(spec, &jump.0.spec);
        let (first, key) = (*animation, *key);

        // Again: the run going on stops first; the key stays.
        let _ = jump.clone().start();
        let again = ops(&executor);
        assert!(matches!(
            &again[..],
            [
                AnimationOp::StopAnimation { animation: a },
                AnimationOp::StartComposite { animation: b, key: k, .. },
            ] if *a == first && *b != first && *k == key
        ));
        jump.stop();
        assert!(matches!(
            &ops(&executor)[..],
            [AnimationOp::StopAnimation { .. }]
        ));
        jump.stop();
        assert_eq!(ops(&executor), [], "stopped once");
        jump.reset();
        assert_eq!(ops(&executor), [AnimationOp::ResetComposite { key }]);

        drop(jump);
        assert_eq!(ops(&executor), [AnimationOp::ReleaseComposite { key }]);
    }

    #[test]
    fn delays_alone_end_at_once() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let end = Animation::sequence([Animation::delay(100.0)]).start();
        assert!(runtime.block_on(end.into_future()));
    }

    #[test]
    fn values_derive_like_their_nodes() {
        let executor = Rc::new(Executor::default());
        let t = NativeValue::new(&executor, 0.0);
        let right = Interpolation {
            extrapolate_right: Extrapolate::Extend,
            ..Interpolation::default()
        };
        let angle = t.interpolate_with(&[0.0, 1.0], Output::Numbers(vec![0.0, 180.0]), right);
        let [
            ..,
            AnimationOp::CreateInterpolate {
                extrapolate_left,
                extrapolate_right,
                easing,
                ..
            },
        ] = &ops(&executor)[..]
        else {
            panic!("an interpolation");
        };
        assert_eq!(
            (*extrapolate_left, *extrapolate_right, *easing),
            (Extrapolate::Clamp, Extrapolate::Extend, Easing::Linear)
        );
        // The same node as through `node()`.
        assert_eq!(
            angle,
            t.node()
                .interpolate_with(&[0.0, 1.0], Output::Numbers(vec![0.0, 180.0]), right)
        );
        assert_eq!(t.diff_clamp(0.0, 44.0), t.node().diff_clamp(0.0, 44.0));
        assert_eq!(t.mul(-1.0), t.node().mul(-1.0));
    }

    #[test]
    fn curves_sample_as_the_graph_runs_them() {
        // A spring from 0 to 1, one value a frame at 60 Hz, at rest at its
        // end; at 120 Hz about twice the frames.
        let spring = sample_spring(0.0, 1.0, Spring::default(), 60.0);
        assert_eq!(spring[0], 0.0);
        assert!((spring.last().unwrap() - 1.0).abs() < 1e-3, "{spring:?}");
        let fast = sample_spring(0.0, 1.0, Spring::default(), 120.0);
        assert!(fast.len() > spring.len() * 3 / 2, "{}", fast.len());
        // A spring that never rests stops at the cap.
        let undamped = Spring {
            damping: 0.0,
            ..Spring::default()
        };
        assert_eq!(
            sample_spring(0.0, 1.0, undamped, 60.0).len(),
            nsh_animation::MAX_SAMPLES
        );
        // A fling glides on and slows down; within bounds it stops at one.
        let decay = sample_decay(0.0, 1000.0, 0.998, 60.0);
        assert!(decay.windows(2).all(|w| w[1] >= w[0]), "{decay:?}");
        assert!(*decay.last().unwrap() > 400.0, "{decay:?}");
        let within = sample_decay_within(0.0, 1000.0, 0.998, (0.0, 100.0), 0.0, 60.0);
        assert!(within.iter().all(|&x| (0.0..=100.0).contains(&x)));
        assert_eq!(*within.last().unwrap(), 100.0);
        // An easing's curve: both ends.
        let curve = super::super::sample_easing(Easing::EaseInOut, 5);
        assert_eq!((curve[0], curve[4], curve.len()), (0.0, 1.0, 5));
    }
}
