//! Animated changes of views: `LayoutAnimation` (frames, insertions,
//! removals), style `transition`s and keyframe `animation`s (`spin`,
//! `ping`, `pulse`, `bounce`), run on the frame clock.
//!
//! A view that animates gets a [`Live`] in [`ViewState::live`] and a frame
//! client ([`Surface::on_frame`]) that runs until nothing is left to
//! animate; other views carry nothing and cost nothing.
//!
//! Frames and styles are animated by writing the in-between values into
//! the view's state (`frame`, `style`) on every tick while [`Live`] keeps
//! the targets: everything that draws reads the current value without
//! knowing about transitions, and a change that arrives mid-animation
//! (the mounter replaces the state with the new target and hands the
//! replaced value over as `old`) starts from where the view is, as
//! UIKit's `beginFromCurrentState`. Native animation values
//! ([`ViewState::anim`]) still override the style.
//!
//! Appearing / disappearing views and keyframes do not touch the style:
//! they multiply the opacity ([`Live::opacity`]) and add a transform after
//! the style's ([`Live::extra`]), which [`ViewState`] applies.
//!
//! [`ViewState`]: crate::view::ViewState
//! [`ViewState::live`]: crate::view::ViewState::live
//! [`ViewState::anim`]: crate::view::ViewState::anim

use std::{rc::Rc, sync::Arc};

use nsh_core::{
    BoxShadow, Color, KeyframeAnimation, Keyframes, LayoutAnimation, LayoutCurve, LayoutProperty,
    LayoutTransition, Rect, Transform, TransitionProps, ViewKind, ViewStyle,
};

use crate::{geom::Mat4, surface::Surface, view::NshView};

/// What animates on one view, and the values drawing reads.
pub(crate) struct Live {
    /// A frame client ticks it (it stops by returning false).
    ticking: bool,
    /// A frame moving to its new place.
    frame: Option<FrameTrack>,
    /// Style values moving to the new style's.
    style: Option<StyleTrack>,
    /// Appearing or disappearing.
    presence: Option<Presence>,
    /// A keyframe animation.
    keyframes: Option<KeyframeRun>,
    /// The view was deleted and stays only while it animates out.
    dying: bool,
    /// Opacity multiplier (appearing / disappearing, keyframes).
    pub(crate) opacity: f32,
    /// Transform applied after the style's.
    extra: Extra,
}

impl Live {
    fn new() -> Self {
        Live {
            ticking: false,
            frame: None,
            style: None,
            presence: None,
            keyframes: None,
            dying: false,
            opacity: 1.0,
            extra: Extra::NONE,
        }
    }

    /// The transform to apply after the style's, if any.
    pub(crate) fn extra(&self) -> Option<Extra> {
        (self.extra != Extra::NONE).then_some(self.extra)
    }

    /// True while the view animates out after its removal: it is drawn but
    /// takes no pointer (hit testing skips it, as GTK's `can-target`).
    pub(crate) fn dying(&self) -> bool {
        self.dying
    }

    fn idle(&self) -> bool {
        self.frame.is_none()
            && self.style.is_none()
            && self.presence.is_none()
            && self.keyframes.is_none()
    }
}

/// A rotation, scale and vertical shift (a fraction of the height)
/// around the transform origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Extra {
    rotate: f32,
    scale: f32,
    shift_y: f32,
}

impl Extra {
    const NONE: Extra = Extra {
        rotate: 0.0,
        scale: 1.0,
        shift_y: 0.0,
    };

    /// `m` followed by this transform (in the view's local space: like
    /// `m`'s own entries, it applies to points before them).
    pub(crate) fn apply(self, mut m: Mat4, height: f32) -> Mat4 {
        if self.shift_y != 0.0 {
            m = m.translate(0.0, self.shift_y * height);
        }
        if self.rotate != 0.0 {
            m = m.rotate(self.rotate);
        }
        if self.scale != 1.0 {
            m = m.scale(self.scale, self.scale);
        }
        m
    }
}

/// An easing curve, from progress in time to progress in value.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Curve {
    Linear,
    /// Cubic Bézier `x1, y1, x2, y2`.
    Bezier([f32; 4]),
    /// A spring with this damping ratio that settles within the duration.
    Spring(f32),
}

impl Curve {
    fn of(curve: LayoutCurve) -> Curve {
        match curve {
            LayoutCurve::Linear => Curve::Linear,
            // UIKit's curves.
            LayoutCurve::EaseIn => Curve::Bezier([0.42, 0.0, 1.0, 1.0]),
            LayoutCurve::EaseOut => Curve::Bezier([0.0, 0.0, 0.58, 1.0]),
            LayoutCurve::EaseInEaseOut => Curve::Bezier([0.42, 0.0, 0.58, 1.0]),
            LayoutCurve::Spring { damping } => Curve::Spring(damping),
        }
    }

    fn at(self, x: f32) -> f32 {
        match self {
            Curve::Linear => x,
            Curve::Bezier(b) => cubic_bezier(b, x),
            Curve::Spring(damping) => spring(damping, x),
        }
    }
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at `x` in `0..=1`.
fn cubic_bezier([x1, y1, x2, y2]: [f32; 4], x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    // Polynomial coefficients of B(t) = ((a t + b) t + c) t.
    let (cx, cy) = (3.0 * x1, 3.0 * y1);
    let (bx, by) = (3.0 * (x2 - x1) - cx, 3.0 * (y2 - y1) - cy);
    let (ax, ay) = (1.0 - cx - bx, 1.0 - cy - by);
    let sample_x = |t: f32| ((ax * t + bx) * t + cx) * t;
    let slope_x = |t: f32| (3.0 * ax * t + 2.0 * bx) * t + cx;
    // Newton's method converges in a few steps on most curves; bisection
    // catches the flat ones.
    let mut t = x;
    let mut solved = false;
    for _ in 0..8 {
        let err = sample_x(t) - x;
        if err.abs() < 1e-6 {
            solved = true;
            break;
        }
        let d = slope_x(t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= err / d;
    }
    if !solved || !(0.0..=1.0).contains(&t) {
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        t = x;
        for _ in 0..32 {
            let v = sample_x(t);
            if (v - x).abs() < 1e-6 {
                break;
            }
            if v < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
    }
    ((ay * t + by) * t + cy) * t
}

/// A damped spring from 0 to 1 at `x` (the fraction of the duration), its
/// stiffness chosen so that it settles by the end, as UIKit's
/// `usingSpringWithDamping` fits a spring into the given duration.
fn spring(damping: f32, x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    // The envelope falls to 1/1000 by the end.
    const SETTLE: f32 = 6.907_755; // ln(1000)
    let zeta = damping.clamp(0.05, 1.0);
    if zeta > 0.999 {
        // Critically damped: (1 + w x) e^(-w x) reaches 1/1000 at w = 9.23.
        let w = 9.233;
        return 1.0 - (1.0 + w * x) * (-w * x).exp();
    }
    let w = SETTLE / zeta;
    let wd = w * (1.0 - zeta * zeta).sqrt();
    let envelope = (-zeta * w * x).exp();
    1.0 - envelope * ((wd * x).cos() + zeta * w / wd * (wd * x).sin())
}

/// When a change runs and how it eases.
#[derive(Debug, Clone, Copy)]
struct Timing {
    /// Frame time (ms) of the first tick.
    start: Option<f64>,
    delay: f32,
    duration: f32,
    curve: Curve,
}

impl Timing {
    fn new(delay: f32, duration: f32, curve: Curve) -> Self {
        Timing {
            start: None,
            delay: delay.max(0.0),
            duration: duration.max(0.0),
            curve,
        }
    }

    fn layout(t: &LayoutTransition) -> Self {
        Timing::new(t.delay_ms, t.duration_ms, Curve::of(t.curve))
    }

    /// Eased progress at `now` (ms): `None` while delayed, then the value
    /// and whether it finished.
    fn at(&mut self, now: f64) -> Option<(f32, bool)> {
        let start = *self.start.get_or_insert(now);
        progress(self.curve, self.delay, self.duration, (now - start) as f32)
    }
}

/// Eased progress `elapsed` ms after a change began.
fn progress(curve: Curve, delay: f32, duration: f32, elapsed: f32) -> Option<(f32, bool)> {
    let t = elapsed - delay;
    if t < 0.0 {
        return None;
    }
    if duration <= 0.0 || t >= duration {
        return Some((1.0, true));
    }
    Some((curve.at(t / duration), false))
}

struct FrameTrack {
    from: Rect,
    to: Rect,
    timing: Timing,
}

struct StyleTrack {
    from: Box<ViewStyle>,
    to: Box<ViewStyle>,
    props: TransitionProps,
    timing: Timing,
}

struct Presence {
    property: LayoutProperty,
    /// Visibility at the start and end (0 hidden, 1 shown).
    from: f32,
    to: f32,
    /// The current visibility.
    value: f32,
    timing: Timing,
}

struct KeyframeRun {
    animation: KeyframeAnimation,
    start: Option<f64>,
}

/// The opacity multiplier and extra transform of `animation`, `elapsed` ms
/// after it was set; `None` once a finite animation ended. During the delay
/// the view shows its own values, as CSS without `animation-fill-mode`.
fn keyframes_at(animation: &KeyframeAnimation, elapsed: f32) -> Option<(f32, Extra)> {
    let t = elapsed - animation.delay;
    if t < 0.0 {
        return Some((1.0, Extra::NONE));
    }
    let d = animation.duration;
    if d <= 0.0 || animation.iterations.is_some_and(|n| t >= n as f32 * d) {
        return None;
    }
    let frac = (t % d) / d;
    let ease = |x: f32| cubic_bezier(animation.easing, x);
    let mut extra = Extra::NONE;
    let mut opacity = 1.0;
    match animation.keyframes {
        Keyframes::Spin => extra.rotate = 360.0 * ease(frac),
        Keyframes::Ping => {
            // 75%, 100% { transform: scale(2); opacity: 0 }
            let q = if frac < 0.75 { ease(frac / 0.75) } else { 1.0 };
            extra.scale = 1.0 + q;
            opacity = 1.0 - q;
        }
        Keyframes::Pulse => {
            // 50% { opacity: .5 }
            let q = if frac < 0.5 {
                ease(frac * 2.0)
            } else {
                1.0 - ease(frac * 2.0 - 1.0)
            };
            opacity = 1.0 - 0.5 * q;
        }
        Keyframes::Bounce => {
            // 0%, 100% { translateY(-25%) } eased in, 50% { none } eased
            // out; the animation's own easing does not apply.
            extra.shift_y = if frac < 0.5 {
                -0.25 * (1.0 - cubic_bezier([0.8, 0.0, 1.0, 1.0], frac * 2.0))
            } else {
                -0.25 * cubic_bezier([0.0, 0.0, 0.2, 1.0], frac * 2.0 - 1.0)
            };
        }
    }
    Some((opacity, extra))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Interpolates colors in premultiplied RGBA, as CSS does, so that
/// transparent ends do not pass through black.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    if a == b {
        return a;
    }
    let t = t.clamp(0.0, 1.0);
    let parts = |c: Color| {
        let (r, g, b, a) = c.to_f64();
        let a = a as f32;
        [r as f32 * a, g as f32 * a, b as f32 * a, a]
    };
    let (pa, pb) = (parts(a), parts(b));
    let p: [f32; 4] = std::array::from_fn(|i| lerp(pa[i], pb[i], t));
    let alpha = p[3];
    if alpha <= 0.0 {
        return Color::TRANSPARENT;
    }
    let channel = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::rgba(
        channel(p[0] / alpha),
        channel(p[1] / alpha),
        channel(p[2] / alpha),
        channel(alpha),
    )
}

/// Interpolates transform lists entry by entry when their kinds match (an
/// empty list stands for the identity of the other); `None` when they
/// cannot be interpolated and the new list applies at once.
fn lerp_transform(a: &Transform, b: &Transform, t: f32) -> Option<Transform> {
    let (ao, bo) = (a.ops(), b.ops());
    let same_kinds = ao.iter().map(|o| o.kind).eq(bo.iter().map(|o| o.kind));
    if !(ao.is_empty() || bo.is_empty() || same_kinds) {
        return None;
    }
    let kinds = if ao.is_empty() { bo } else { ao };
    let mut out = Transform::IDENTITY;
    for (i, op) in kinds.iter().enumerate() {
        let va = ao.get(i).map_or(op.kind.identity(), |o| o.value);
        let vb = bo.get(i).map_or(op.kind.identity(), |o| o.value);
        out.push(op.kind, lerp(va, vb, t));
    }
    Some(out)
}

/// A shadow that draws nothing, to interpolate against `s`.
fn none_like(s: &BoxShadow) -> BoxShadow {
    BoxShadow {
        offset_x: 0.0,
        offset_y: 0.0,
        blur: 0.0,
        spread: 0.0,
        color: Color::TRANSPARENT,
        inset: s.inset,
    }
}

/// Interpolates box shadow lists, the shorter padded with empty shadows
/// (CSS); `None` when an inset and an outer shadow meet.
fn lerp_box_shadows(a: &[BoxShadow], b: &[BoxShadow], t: f32) -> Option<Vec<BoxShadow>> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (sa, sb) = match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) => (*x, *y),
            (Some(x), None) => (*x, none_like(x)),
            (None, Some(y)) => (none_like(y), *y),
            (None, None) => unreachable!("i < the longer length"),
        };
        if sa.inset != sb.inset {
            return None;
        }
        out.push(BoxShadow {
            offset_x: lerp(sa.offset_x, sb.offset_x, t),
            offset_y: lerp(sa.offset_y, sb.offset_y, t),
            blur: lerp(sa.blur, sb.blur, t).max(0.0),
            spread: lerp(sa.spread, sb.spread, t),
            color: lerp_color(sa.color, sb.color, t),
            inset: sa.inset,
        });
    }
    Some(out)
}

fn box_shadows(s: &ViewStyle) -> &[BoxShadow] {
    s.effects.as_deref().map_or(&[], |e| &e.box_shadows)
}

/// True if `a` and `b` differ in what `props` animates.
fn differs(a: &ViewStyle, b: &ViewStyle, props: TransitionProps) -> bool {
    (props.has(TransitionProps::COLORS)
        && (a.background_color != b.background_color
            || a.borders.colors != b.borders.colors
            || a.outline.color != b.outline.color
            || a.tint_color != b.tint_color))
        || (props.has(TransitionProps::OPACITY) && a.opacity != b.opacity)
        || (props.has(TransitionProps::TRANSFORM) && a.transform != b.transform)
        || (props.has(TransitionProps::SHADOW)
            && (a.shadow != b.shadow || box_shadows(a) != box_shadows(b)))
        || (props.has(TransitionProps::SHAPE)
            && (a.border_radius != b.border_radius
                || a.corner_radius != b.corner_radius
                || a.borders.widths != b.borders.widths
                || a.outline.width != b.outline.width))
}

/// Writes the values between `from` and `to` that `props` animates into
/// `out` (a copy of `to`).
fn lerp_style(
    out: &mut ViewStyle,
    from: &ViewStyle,
    to: &ViewStyle,
    props: TransitionProps,
    t: f32,
) {
    if props.has(TransitionProps::COLORS) {
        out.background_color = lerp_color(from.background_color, to.background_color, t);
        out.borders.colors =
            std::array::from_fn(|i| lerp_color(from.borders.colors[i], to.borders.colors[i], t));
        out.outline.color = lerp_color(from.outline.color, to.outline.color, t);
        if let (Some(a), Some(b)) = (from.tint_color, to.tint_color) {
            out.tint_color = Some(lerp_color(a, b, t));
        }
    }
    if props.has(TransitionProps::OPACITY) {
        out.opacity = lerp(from.opacity, to.opacity, t).clamp(0.0, 1.0);
    }
    if props.has(TransitionProps::TRANSFORM)
        && let Some(transform) = lerp_transform(&from.transform, &to.transform, t)
    {
        out.transform = transform;
    }
    if props.has(TransitionProps::SHADOW) {
        let (a, b) = (&from.shadow, &to.shadow);
        out.shadow.color = lerp_color(a.color, b.color, t);
        out.shadow.offset_x = lerp(a.offset_x, b.offset_x, t);
        out.shadow.offset_y = lerp(a.offset_y, b.offset_y, t);
        out.shadow.opacity = lerp(a.opacity, b.opacity, t).clamp(0.0, 1.0);
        out.shadow.radius = lerp(a.radius, b.radius, t).max(0.0);
        let (sa, sb) = (box_shadows(from), box_shadows(to));
        if sa != sb
            && let Some(shadows) = lerp_box_shadows(sa, sb, t)
        {
            let mut effects = to.effects.as_deref().cloned().unwrap_or_default();
            effects.box_shadows = shadows;
            out.effects = Some(Arc::new(effects));
        }
    }
    if props.has(TransitionProps::SHAPE) {
        out.border_radius = lerp(from.border_radius, to.border_radius, t).max(0.0);
        out.corner_radius =
            std::array::from_fn(|i| match (from.corner_radius[i], to.corner_radius[i]) {
                (None, None) => None,
                (a, b) => Some(
                    lerp(
                        a.unwrap_or(from.border_radius),
                        b.unwrap_or(to.border_radius),
                        t,
                    )
                    .max(0.0),
                ),
            });
        out.borders.widths =
            std::array::from_fn(|i| lerp(from.borders.widths[i], to.borders.widths[i], t).max(0.0));
        out.outline.width = lerp(from.outline.width, to.outline.width, t).max(0.0);
    }
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect {
        x: lerp(a.x, b.x, t),
        y: lerp(a.y, b.y, t),
        width: lerp(a.width, b.width, t).max(0.0),
        height: lerp(a.height, b.height, t).max(0.0),
    }
}

fn reduce_motion(surface: &Surface) -> bool {
    surface.environment.get().reduce_motion
}

/// Views whose frames, appearing and disappearing belong to navigation
/// (stacks, screens, modals, header items) or that are not drawn in their
/// parent.
fn navigated(view: &NshView) -> bool {
    matches!(
        view.kind(),
        ViewKind::Stack
            | ViewKind::Screen
            | ViewKind::Modal
            | ViewKind::HeaderItem
            | ViewKind::InputAccessory
    ) || view
        .parent_view()
        .is_some_and(|p| p.kind() == ViewKind::Stack)
}

/// Runs `f` on the view's [`Live`] (made if needed) and makes sure it
/// ticks while something animates.
fn animate(surface: &Rc<Surface>, view: &NshView, f: impl FnOnce(&mut Live)) {
    let mut s = view.state_mut();
    let live = s.live.get_or_insert_with(|| Box::new(Live::new()));
    f(live);
    if live.ticking {
        // The tick drops it once idle.
    } else if live.idle() {
        s.live = None;
    } else {
        live.ticking = true;
        drop(s);
        let view = view.clone();
        surface.on_frame(move |surface, now| tick(surface, &view, now * 1000.0));
    }
}

/// `view` was inserted into its parent.
pub(crate) fn inserted(surface: &Rc<Surface>, view: &NshView, animation: Option<&LayoutAnimation>) {
    if view.state().live.is_some() {
        // Ticks stop while a view is out of the tree; going on resumes them.
        animate(surface, view, |_| {});
    }
    if view.state().live.as_ref().is_some_and(|l| l.dying) {
        // Back (or moved) before its removal finished animating (which
        // also makes it take the pointer again).
        animate(surface, view, |live| {
            live.dying = false;
            live.presence = None;
            // The next tick adds a keyframe animation's values again.
            live.opacity = 1.0;
            live.extra = Extra::NONE;
        });
        view.moved();
    }
    let Some(t) = animation.and_then(|a| a.create) else {
        return;
    };
    // Views moved to another place were laid out before; new ones were not.
    if view.state().frame != Rect::default() || reduce_motion(surface) || navigated(view) {
        return;
    }
    animate(surface, view, |live| {
        live.presence = Some(Presence {
            property: t.property,
            from: 0.0,
            to: 1.0,
            value: 0.0,
            timing: Timing::layout(&t),
        });
        live.opacity = 0.0;
        live.extra = presence_extra(live.presence.as_ref().expect("set"));
    });
    view.moved();
}

/// `view` is removed from its parent or deleted: true if it animates out
/// (and detaches itself when done), false to detach it now.
///
/// Removed views are animated, as UIKit's backend does: a view leaving the
/// tree is removed in the commit that configured the animation, while its
/// deletion follows whenever JS releases it.
pub(crate) fn removing(
    surface: &Rc<Surface>,
    view: &NshView,
    animation: Option<&LayoutAnimation>,
) -> bool {
    let Some(t) = animation.and_then(|a| a.delete) else {
        return false;
    };
    if view.parent_view().is_none() || reduce_motion(surface) || navigated(view) {
        return false;
    }
    if view.state().live.as_ref().is_some_and(|l| l.dying) {
        // Deleted while its removal animates.
        return true;
    }
    animate(surface, view, |live| {
        let value = live.presence.as_ref().map_or(1.0, |p| p.value);
        live.dying = true;
        live.presence = Some(Presence {
            property: t.property,
            from: value,
            to: 0.0,
            value,
            timing: Timing::layout(&t),
        });
    });
    true
}

/// The frame a running layout animation of `view` is heading to, if any.
pub(crate) fn frame_target(view: &NshView) -> Option<Rect> {
    view.state().live.as_ref()?.frame.as_ref().map(|t| t.to)
}

/// `view`'s frame changed from `old` (the new one is in its state).
pub(crate) fn frame_changed(
    surface: &Rc<Surface>,
    view: &NshView,
    old: Rect,
    animation: Option<&LayoutAnimation>,
) {
    // A view's first frame (from the zero rect) does not animate.
    let update = animation
        .and_then(|a| a.update)
        .filter(|_| old != Rect::default() && !reduce_motion(surface) && !navigated(view));
    let Some(t) = update else {
        // The new frame applies at once, stopping a running animation.
        if let Some(live) = view.state_mut().live.as_deref_mut() {
            live.frame = None;
        }
        return;
    };
    // The old frame shows until the first tick.
    let to = std::mem::replace(&mut view.state_mut().frame, old);
    animate(surface, view, |live| {
        live.frame = Some(FrameTrack {
            from: old,
            to,
            timing: Timing::layout(&t),
        });
    });
}

/// `view`'s style changed from `old` (the new one is in its state).
pub(crate) fn style_changed(surface: &Rc<Surface>, view: &NshView, old: &ViewStyle) {
    let reduce = reduce_motion(surface);
    let (transition, animation, has_live) = {
        let s = view.state();
        (s.style.transition, s.style.animation, s.live.is_some())
    };
    // Keyframes restart only when the animation itself changes, as in CSS.
    let keyframes_changed = animation != old.animation;
    let transition = transition.filter(|t| {
        !reduce
            && t.duration + t.delay > 0.0
            && differs(old, &view.state().style, t.properties)
            // A view's first style does not transition from the defaults.
            && *old != ViewStyle::default()
    });
    if transition.is_none() && !keyframes_changed {
        if has_live && let Some(live) = view.state_mut().live.as_deref_mut() {
            live.style = None;
        }
        return;
    }
    let track = transition.map(|t| {
        let to = view.state().style.clone();
        // The old values show until the first tick.
        lerp_style(&mut view.state_mut().style, old, &to, t.properties, 0.0);
        StyleTrack {
            from: Box::new(old.clone()),
            to,
            props: t.properties,
            timing: Timing::new(t.delay, t.duration, Curve::Bezier(t.easing)),
        }
    });
    animate(surface, view, |live| {
        live.style = track;
        if keyframes_changed {
            live.keyframes = animation.filter(|_| !reduce).map(|animation| KeyframeRun {
                animation,
                start: None,
            });
            if live.keyframes.is_none() {
                live.opacity = live.presence.as_ref().map_or(1.0, presence_opacity);
                live.extra = live.presence.as_ref().map_or(Extra::NONE, presence_extra);
            }
        }
    });
}

fn presence_opacity(p: &Presence) -> f32 {
    match p.property {
        LayoutProperty::Opacity => p.value.clamp(0.0, 1.0),
        LayoutProperty::Scale => 1.0,
    }
}

fn presence_extra(p: &Presence) -> Extra {
    match p.property {
        LayoutProperty::Opacity => Extra::NONE,
        LayoutProperty::Scale => Extra {
            // Not quite zero: a singular matrix cannot be inverted for
            // picking.
            scale: p.value.max(0.01),
            ..Extra::NONE
        },
    }
}

/// True if an ancestor of `view` is animating out (and takes `view` with
/// it).
fn ancestor_dying(view: &NshView) -> bool {
    let mut parent = view.parent_view();
    while let Some(p) = parent {
        if p.state().live.as_ref().is_some_and(|l| l.dying) {
            return true;
        }
        parent = p.parent_view();
    }
    false
}

/// One frame of `view`'s animations at `now` (ms): false once nothing is
/// left to animate (the frame client is dropped).
fn tick(surface: &Rc<Surface>, view: &NshView, now: f64) -> bool {
    if !view.attached() {
        // Out of the tree: a deleted subtree stops for good (dropping the
        // client releases it); a view only moved out resumes when it is
        // inserted again. Views of a subtree being built (registered, with
        // a parent, not attached yet) go on.
        let id = view.id();
        let registered = surface
            .mounter
            .try_borrow()
            .ok()
            .is_none_or(|m| m.views.get(id).as_ref() == Some(view));
        if !registered || view.parent_view().is_none() {
            if let Some(live) = view.state_mut().live.as_deref_mut() {
                live.ticking = false;
            }
            return false;
        }
    }
    // A view deleted inside a deleted subtree leaves with its ancestor
    // instead of fading on its own (deletions arrive in any order, so this
    // is decided once the batch is applied).
    let starting_out = view.state().live.as_ref().is_some_and(|l| {
        l.dying
            && l.presence
                .as_ref()
                .is_some_and(|p| p.timing.start.is_none())
    });
    let inherited = starting_out && ancestor_dying(view);

    let mut s = view.state_mut();
    let Some(mut live) = s.live.take() else {
        return false;
    };
    if inherited {
        live.presence = None;
        live.dying = false;
    }
    let mut removed = false;

    if let Some(track) = &mut live.frame
        && let Some((p, done)) = track.timing.at(now)
    {
        let frame = if done {
            track.to
        } else {
            lerp_rect(track.from, track.to, p)
        };
        s.frame = frame;
        if done {
            live.frame = None;
        }
    }

    if let Some(track) = &mut live.style
        && let Some((p, done)) = track.timing.at(now)
    {
        if done {
            s.style = std::mem::take(&mut track.to);
            live.style = None;
        } else {
            lerp_style(&mut s.style, &track.from, &track.to, track.props, p);
        }
    }

    let (mut opacity, mut extra) = (1.0, Extra::NONE);
    if let Some(presence) = &mut live.presence {
        let step = presence.timing.at(now);
        if let Some((p, done)) = step {
            presence.value = if done {
                presence.to
            } else {
                lerp(presence.from, presence.to, p)
            };
        }
        if step.is_some_and(|(_, done)| done) {
            removed = live.dying;
            live.presence = None;
        } else {
            opacity = presence_opacity(presence);
            extra = presence_extra(presence);
        }
    }

    if let Some(run) = &mut live.keyframes {
        let start = *run.start.get_or_insert(now);
        match keyframes_at(&run.animation, (now - start) as f32) {
            Some((o, e)) => {
                opacity *= o;
                extra = Extra {
                    rotate: extra.rotate + e.rotate,
                    scale: extra.scale * e.scale,
                    shift_y: extra.shift_y + e.shift_y,
                };
            }
            None => live.keyframes = None,
        }
    }
    live.opacity = opacity;
    live.extra = extra;

    let finished = removed || live.idle();
    if finished {
        // Returning false drops the frame client.
        live.ticking = false;
    }
    if !live.idle() {
        s.live = Some(live);
    }
    drop(s);

    if removed {
        // Taken out as an immediate removal would be (the mounter is
        // borrowed only while a batch is applied, which never ticks).
        match surface.mounter.try_borrow() {
            Ok(mounter) => mounter.detach(surface, view),
            Err(_) => view.unparent(),
        }
        return false;
    }
    view.moved();
    !finished
}

#[cfg(test)]
mod tests {
    use nsh_core::TransformKind;

    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn bezier_matches_css() {
        let ease = [0.25, 0.1, 0.25, 1.0];
        assert_eq!(cubic_bezier(ease, 0.0), 0.0);
        assert_eq!(cubic_bezier(ease, 1.0), 1.0);
        // Reference values of CSS `ease`.
        assert!(
            close(cubic_bezier(ease, 0.5), 0.8024),
            "{}",
            cubic_bezier(ease, 0.5)
        );
        assert!(
            close(cubic_bezier(ease, 0.25), 0.4094),
            "{}",
            cubic_bezier(ease, 0.25)
        );
        assert!(close(cubic_bezier([0.0, 0.0, 1.0, 1.0], 0.37), 0.37));
        let in_out = [0.42, 0.0, 0.58, 1.0];
        let mut last = 0.0;
        for i in 1..=100 {
            let v = cubic_bezier(in_out, i as f32 / 100.0);
            assert!(v >= last, "monotonic");
            last = v;
        }
        assert!(close(cubic_bezier(in_out, 0.5), 0.5));
        // A flat start (x1 = 1) still solves.
        assert!(cubic_bezier([0.8, 0.0, 1.0, 1.0], 0.5) < 0.3);
    }

    #[test]
    fn spring_settles_and_overshoots() {
        assert_eq!(spring(0.4, 0.0), 0.0);
        assert_eq!(spring(0.4, 1.0), 1.0);
        let max = (1..100)
            .map(|i| spring(0.4, i as f32 / 100.0))
            .fold(0.0, f32::max);
        assert!(max > 1.1, "a loose spring overshoots: {max}");
        assert!((spring(0.4, 0.99) - 1.0).abs() < 0.01);
        // A critically damped spring does not overshoot.
        assert!((1..100).all(|i| spring(1.0, i as f32 / 100.0) <= 1.0));
        assert!((spring(1.0, 0.98) - 1.0).abs() < 0.01);
    }

    #[test]
    fn progress_waits_for_delay() {
        assert_eq!(progress(Curve::Linear, 100.0, 200.0, 50.0), None);
        assert_eq!(
            progress(Curve::Linear, 100.0, 200.0, 200.0),
            Some((0.5, false))
        );
        assert_eq!(
            progress(Curve::Linear, 100.0, 200.0, 300.0),
            Some((1.0, true))
        );
        assert_eq!(progress(Curve::Linear, 0.0, 0.0, 0.0), Some((1.0, true)));
    }

    #[test]
    fn presets_time_as_configured() {
        let mut t = Timing::layout(&LayoutAnimation::EASE_IN_EASE_OUT.update.unwrap());
        assert_eq!(t.at(1000.0), Some((0.0, false)));
        assert!(close(t.at(1150.0).unwrap().0, 0.5));
        assert_eq!(t.at(1300.0), Some((1.0, true)));
        let mut t = Timing::layout(&LayoutAnimation::LINEAR.create.unwrap());
        t.at(0.0);
        assert!(close(t.at(125.0).unwrap().0, 0.25));
        assert_eq!(t.at(500.0), Some((1.0, true)));
        let spring = LayoutAnimation::SPRING;
        let mut t = Timing::layout(&spring.update.unwrap());
        t.at(0.0);
        // The frame overshoots at some point before settling at 700 ms.
        let over = (1..70).any(|i| t.at(i as f64 * 10.0).unwrap().0 > 1.0);
        assert!(over);
        assert_eq!(t.at(700.0), Some((1.0, true)));
        assert_eq!(spring.create.unwrap().property, LayoutProperty::Opacity);
        assert_eq!(Curve::of(spring.delete.unwrap().curve), Curve::Linear);
    }

    #[test]
    fn rects_interpolate_and_never_go_negative() {
        let a = Rect {
            x: 0.0,
            y: 10.0,
            width: 100.0,
            height: 40.0,
        };
        let b = Rect {
            x: 50.0,
            y: 20.0,
            width: 0.0,
            height: 80.0,
        };
        let r = lerp_rect(a, b, 0.5);
        assert_eq!((r.x, r.y, r.width, r.height), (25.0, 15.0, 50.0, 60.0));
        // A spring past its end does not make a negative size.
        assert_eq!(lerp_rect(a, b, 1.2).width, 0.0);
    }

    #[test]
    fn extra_applies_before_the_style_transform() {
        let extra = Extra {
            rotate: 0.0,
            scale: 2.0,
            shift_y: -0.25,
        };
        // A style translate by (10, 0): the extra's scale and shift apply
        // to points first, then the translation.
        let m = extra.apply(Mat4::new().translate(10.0, 0.0), 100.0);
        let (x, y) = m.map(1.0, 0.0).unwrap();
        assert!(close(x, 12.0) && close(y, -25.0), "{x} {y}");
        let spin = Extra {
            rotate: 90.0,
            ..Extra::NONE
        };
        let (x, y) = spin.apply(Mat4::new(), 10.0).map(1.0, 0.0).unwrap();
        assert!(close(x, 0.0) && close(y, 1.0), "{x} {y}");
        assert_eq!(Extra::NONE.apply(Mat4::new(), 10.0), Mat4::new());
    }

    #[test]
    fn presence_values() {
        let p = Presence {
            property: LayoutProperty::Scale,
            from: 0.0,
            to: 1.0,
            value: 0.0,
            timing: Timing::new(0.0, 100.0, Curve::Linear),
        };
        assert_eq!(presence_opacity(&p), 1.0);
        assert_eq!(presence_extra(&p).scale, 0.01);
        let p = Presence {
            property: LayoutProperty::Opacity,
            value: 0.4,
            ..p
        };
        assert_eq!(presence_opacity(&p), 0.4);
        assert_eq!(presence_extra(&p), Extra::NONE);
    }

    #[test]
    fn colors_interpolate_premultiplied() {
        let red = Color::rgba(255, 0, 0, 255);
        let blue = Color::rgba(0, 0, 255, 255);
        assert_eq!(lerp_color(red, blue, 0.5), Color::rgba(128, 0, 128, 255));
        // From transparent the color stays red while it fades in.
        assert_eq!(
            lerp_color(Color::TRANSPARENT, red, 0.5),
            Color::rgba(255, 0, 0, 128)
        );
        assert_eq!(lerp_color(red, blue, 1.0), blue);
    }

    #[test]
    fn transforms_interpolate_by_position() {
        let mut a = Transform::IDENTITY;
        a.push(TransformKind::TranslateX, 10.0);
        a.push(TransformKind::Scale, 1.0);
        let mut b = Transform::IDENTITY;
        b.push(TransformKind::TranslateX, 30.0);
        b.push(TransformKind::Scale, 2.0);
        let m = lerp_transform(&a, &b, 0.5).unwrap();
        assert_eq!(m.ops()[0].value, 20.0);
        assert_eq!(m.ops()[1].value, 1.5);
        // From none: each entry's identity.
        let m = lerp_transform(&Transform::IDENTITY, &b, 0.5).unwrap();
        assert_eq!(m.ops()[0].value, 15.0);
        assert_eq!(m.ops()[1].value, 1.5);
        let mut c = Transform::IDENTITY;
        c.push(TransformKind::Rotate, 90.0);
        assert!(lerp_transform(&a, &c, 0.5).is_none());
    }

    #[test]
    fn style_lerp_touches_only_its_groups() {
        let from = ViewStyle {
            opacity: 0.0,
            background_color: Color::rgba(0, 0, 0, 255),
            ..ViewStyle::default()
        };
        let to = ViewStyle {
            opacity: 1.0,
            background_color: Color::rgba(255, 255, 255, 255),
            border_radius: 10.0,
            ..ViewStyle::default()
        };
        let mut out = to.clone();
        lerp_style(
            &mut out,
            &from,
            &to,
            TransitionProps(TransitionProps::OPACITY),
            0.5,
        );
        assert_eq!(out.opacity, 0.5);
        assert_eq!(out.background_color, to.background_color);
        assert_eq!(out.border_radius, 10.0);
        assert!(differs(
            &from,
            &to,
            TransitionProps(TransitionProps::COLORS)
        ));
        assert!(!differs(&from, &to, TransitionProps(0)));
    }

    #[test]
    fn box_shadows_pad_with_none() {
        let s = BoxShadow {
            offset_x: 0.0,
            offset_y: 10.0,
            blur: 20.0,
            spread: 0.0,
            color: Color::rgba(0, 0, 0, 100),
            inset: false,
        };
        let out = lerp_box_shadows(&[], &[s], 0.5).unwrap();
        assert_eq!(out[0].offset_y, 5.0);
        assert_eq!(out[0].blur, 10.0);
        assert_eq!(out[0].color, Color::rgba(0, 0, 0, 50));
        let inset = BoxShadow { inset: true, ..s };
        assert!(lerp_box_shadows(&[inset], &[s], 0.5).is_none());
    }

    #[test]
    fn keyframes() {
        let anim = |keyframes, iterations| KeyframeAnimation {
            keyframes,
            duration: 1000.0,
            delay: 0.0,
            easing: [0.0, 0.0, 1.0, 1.0],
            iterations,
        };
        let spin = anim(Keyframes::Spin, None);
        assert!(close(keyframes_at(&spin, 250.0).unwrap().1.rotate, 90.0));
        assert!(close(keyframes_at(&spin, 1250.0).unwrap().1.rotate, 90.0));
        let ping = anim(Keyframes::Ping, Some(1));
        let (o, e) = keyframes_at(&ping, 375.0).unwrap();
        assert!(close(o, 0.5) && close(e.scale, 1.5));
        assert!(close(keyframes_at(&ping, 900.0).unwrap().0, 0.0));
        assert!(keyframes_at(&ping, 1000.0).is_none());
        let pulse = anim(Keyframes::Pulse, None);
        assert!(close(keyframes_at(&pulse, 500.0).unwrap().0, 0.5));
        assert!(close(keyframes_at(&pulse, 0.0).unwrap().0, 1.0));
        let bounce = anim(Keyframes::Bounce, None);
        assert!(close(keyframes_at(&bounce, 0.0).unwrap().1.shift_y, -0.25));
        assert!(close(keyframes_at(&bounce, 500.0).unwrap().1.shift_y, 0.0));
        // Delayed: the view's own values.
        let delayed = KeyframeAnimation {
            delay: 300.0,
            ..spin
        };
        assert_eq!(keyframes_at(&delayed, 100.0), Some((1.0, Extra::NONE)));
    }
}
