//! Scroll views: offsets, wheel / trackpad / keyboard scrolling,
//! snapping, sticky cells, the core's scroll requests, scroll events.
//!
//! A scroll view only moves along its main axis (zooming is not
//! supported: `zoomTo` and the zoom props are ignored). Its offset lives in
//! [`ScrollState`] (`x` / `y`, 0 showing the start of the content inset)
//! and is shown by its container's bounds origin, as `NSClipView` scrolls.
//! Every change goes through [`moved_to`], which moves the bounds origin,
//! pins the sticky cells, feeds the native scroll value in the same frame
//! and queues a report to the engine. Reports are coalesced to one per
//! frame by the view's frame callback, which also runs animated
//! scrolls, the indicator's fade and the refresh spinner, and stops when
//! none of them is left.
//!
//! Offsets are reported as UIKit's `contentOffset` (minus the leading
//! content inset, clamped to the content), so lists see the same numbers
//! as on iOS. Snapping and paging use [`ScrollBehavior::snap_target`].
//!
//! Trackpads follow the system: a gesture (`phase`) rubber-bands past the
//! edges when `bounces` and springs back when the fingers lift, and the
//! momentum after it is the system's (`momentumPhase` events), not a glide
//! of ours; snapping views snap from the release instead. Mouse wheels (no
//! phases) stop at the edges and settle when the notches stop. Mouse drags
//! never scroll (dragging the content is a touchscreen's way to scroll);
//! the mouse drags the indicator as a scroll bar.
//!
//! Mirrored (right to left) horizontal lists keep their offsets left to
//! right: [`NshView::place`] puts each child at the mirror image of its
//! frame, so the bounds origin is the offset negated, and wheels and the
//! indicator are mirrored here.

use std::{f32::consts::PI, rc::Rc};

use nsh_core::{Command, EventKind, EventPayload, ScrollBehavior, Size, StickyCell, ViewKind};
use objc2::{MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{NSResponder, NSView};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGColor;
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint};
use objc2_quartz_core::CALayer;

use crate::{
    surface::Surface,
    view::{NshView, ScrollState, ViewState},
};

/// A wheel without a gesture (a mouse) has settled after this many ms.
const WHEEL_IDLE: f64 = 150.0;
/// After a trackpad gesture ends, the system's momentum begins within this
/// long (ms), or there is none.
const MOMENTUM_WAIT: f64 = 100.0;
/// The indicator stays this long (ms) after the last scroll...
const INDICATOR_HOLD: f64 = 700.0;
/// ...then fades out in this long.
const INDICATOR_FADE: f64 = 300.0;
/// How far past the top a release must be to trigger a refresh.
const REFRESH_PULL: f32 = 64.0;
/// Space kept above the content while refreshing (the spinner's).
const REFRESH_SPACE: f32 = 56.0;
/// Duration of animated scrolls and bounces back (ms).
const ANIMATION: f64 = 300.0;
/// The release speed (points per ms) a settled mouse wheel snaps with:
/// fast enough to move on to the next snap point.
const WHEEL_SNAP: f32 = 0.6;
/// Space between the indicator and the ends of the viewport.
const INDICATOR_MARGIN: f32 = 2.0;
/// The indicator's thickness.
const INDICATOR_THICK: f32 = 6.0;
/// How close to its edge (points) a mouse press grabs the indicator,
/// which is thinner than a pointer is precise...
const THUMB_GRAB: f32 = 12.0;
/// ...and how far past its ends.
const THUMB_SLOP: f32 = 4.0;

/// What a scroll view is doing.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum Phase {
    /// At rest.
    #[default]
    Idle,
    /// The mouse drags the indicator as a scroll bar's thumb, from offset
    /// `start`.
    Thumb { start: f32 },
    /// Wheel or trackpad input: the last at `last` (ms), `total` points
    /// so far; `gesture` for trackpads (began / ended), not mouse wheels;
    /// `raw` is where the gesture's deltas put the offset before the
    /// rubber band.
    Wheel {
        last: f64,
        total: f32,
        gesture: bool,
        raw: f32,
    },
    /// A trackpad gesture ended at `since` (ms); the system's momentum may
    /// follow.
    AwaitMomentum { since: f64 },
    /// The system's momentum scrolls.
    Momentum,
    /// An eased move from `from` to `to`, started at `start` (ms):
    /// `momentum` for snapping after a fling (momentum events), `end` to
    /// send `onScrollEnd` when it arrives.
    Animate {
        from: f32,
        to: f32,
        start: f64,
        duration: f64,
        momentum: bool,
        end: bool,
    },
}

/// The running state of a scroll view: input phase, the indicator and
/// the refresh spinner ([`ScrollState::live`]).
#[derive(Debug, Default, Clone)]
pub(crate) struct Live {
    phase: Phase,
    /// The view's frame callback runs.
    ticking: bool,
    /// The offset changed since it was last reported.
    report: bool,
    /// When the indicator was last shown (ms).
    shown: f64,
    /// The indicator's opacity.
    alpha: f32,
    /// The surface is dark (the indicator's color).
    dark: bool,
    /// The refresh spinner shows.
    refreshing: bool,
    /// The spinner's angle (radians).
    spin: f32,
    /// The view above the container that holds the indicator and the
    /// spinner.
    overlay: Option<Retained<NSView>>,
    /// The indicator's layer.
    indicator: Option<Retained<CALayer>>,
    /// The refresh spinner's layer, with a sublayer per spoke.
    spinner: Option<Retained<CALayer>>,
    /// Trackpad offsets waiting to be shown ([`Resampled`]).
    resampled: Option<Resampled>,
}

/// Where trackpad events (gestures and their momentum) put the offset, by
/// the events' time (ms), shown at display frames ([`tick`]) rather than
/// as they come. Events come at about the display's rate but not in step
/// with it: shown as they come, some frames get two and some none, and a
/// steady scroll judders (measured on a 120 Hz screen: up to a quarter of
/// the frames of a fling moved twice or not at all, depending on how the
/// events fell between the frames). Each frame shows the offset the
/// events give for a moment one event interval before it, interpolated,
/// so every frame moves by what that interval did.
#[derive(Debug, Default, Clone)]
struct Resampled {
    /// `(time, offset)`, oldest first; the first is the offset shown
    /// when the events began.
    points: std::collections::VecDeque<(f64, f32)>,
    /// The time between events (ms), smoothed.
    interval: f64,
}

impl Resampled {
    /// The newest offset (where the events have put the view).
    fn goal(&self) -> Option<f32> {
        self.points.back().map(|p| p.1)
    }

    /// Adds where an event at `time` put the offset.
    fn push(&mut self, time: f64, pos: f32) {
        match self.points.back() {
            Some(&(last, _)) if time <= last => {
                // The same instant (or out of order): the newest wins.
                self.points.pop_back();
                self.points.push_back((last, pos));
            }
            Some(&(last, _)) => {
                // Gaps beyond a frame at 60 Hz are pauses, not the rate.
                let gap = (time - last).clamp(4.0, 17.0);
                self.interval += (gap - self.interval) * 0.25;
                self.points.push_back((time, pos));
            }
            None => self.points.push_back((time, pos)),
        }
    }

    /// The offset at `t` (ms), dropping points before it, and whether
    /// `t` is past the newest (which it then is).
    fn sample(&mut self, t: f64) -> (f32, bool) {
        while self.points.len() > 2 && self.points[1].0 <= t {
            self.points.pop_front();
        }
        let Some(&(t0, p0)) = self.points.front() else {
            return (0.0, true);
        };
        let Some(&(t1, p1)) = self.points.get(1) else {
            return (p0, true);
        };
        if t >= t1 {
            return (p1, true);
        }
        let k = if t1 > t0 {
            ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        (p0 + (p1 - p0) * k as f32, false)
    }
}

// Pure logic.

/// The rubber band of iOS: how far content dragged `over` points past an
/// edge of a `dim`-point viewport moves.
fn rubber(over: f32, dim: f32) -> f32 {
    let dim = dim.max(1.0);
    (1.0 - 1.0 / (over * 0.55 / dim + 1.0)) * dim
}

/// Position on a drag that went to `raw`, with `[min, max]` the limits.
fn dragged(raw: f32, min: f32, max: f32, dim: f32, bounces: bool) -> f32 {
    if raw < min {
        if bounces {
            min - rubber(min - raw, dim)
        } else {
            min
        }
    } else if raw > max {
        if bounces {
            max + rubber(raw - max, dim)
        } else {
            max
        }
    } else {
        raw
    }
}

/// The deceleration rate: velocity kept per ms.
fn rate(behavior: &ScrollBehavior) -> f32 {
    if behavior.deceleration > 0.0 && behavior.deceleration < 1.0 {
        behavior.deceleration
    } else if behavior.snaps() || behavior.paging {
        0.99
    } else {
        0.998
    }
}

/// Where a glide at `v` from `offset` comes to rest.
fn projected(offset: f32, v: f32, rate: f32) -> f32 {
    offset - v / rate.ln()
}

/// How far a sticky cell moves at `offset`.
fn sticky_shift(offset: f32, start: f32, end: f32) -> f32 {
    (offset - start).clamp(0.0, (end - start).max(0.0))
}

/// Cubic ease out.
fn ease_out(t: f64) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (1.0 - (1.0 - t).powi(3)) as f32
}

/// How long an eased move of `distance` that starts at speed `v` (points
/// per ms, in its direction) takes: ease out starts at 3 × distance /
/// duration, which then matches the release.
fn snap_duration(distance: f32, v: f32) -> f64 {
    if distance.abs() < 0.5 {
        return 0.0;
    }
    if v * distance > 0.0 {
        (3.0 * distance.abs() / v.abs()).clamp(180.0, 600.0) as f64
    } else {
        ANIMATION
    }
}

/// The indicator's opacity `since` ms after it was shown.
fn indicator_alpha(since: f64) -> f32 {
    if since < INDICATOR_HOLD {
        1.0
    } else {
        (1.0 - (since - INDICATOR_HOLD) / INDICATOR_FADE).max(0.0) as f32
    }
}

/// The indicator along the main axis: `(start, length)` in a viewport of
/// `viewport` points showing `total` points at `offset`, or `None` when
/// everything fits.
fn indicator_span(viewport: f32, total: f32, offset: f32) -> Option<(f32, f32)> {
    /// Short enough to see where it is in long lists, long enough to see.
    const MIN_LENGTH: f32 = 32.0;
    if total <= viewport + 0.5 || viewport <= 4.0 * INDICATOR_MARGIN {
        return None;
    }
    let track = viewport - 2.0 * INDICATOR_MARGIN;
    let max = total - viewport;
    // Overscrolling shrinks it, as macOS's overlay scrollers do.
    let over = if offset < 0.0 {
        -offset
    } else {
        (offset - max).max(0.0)
    };
    let len = (track * viewport / total - over).max(MIN_LENGTH).min(track);
    let t = (offset / max).clamp(0.0, 1.0);
    Some((INDICATOR_MARGIN + (track - len) * t, len))
}

/// How far the offset moves when the thumb moves by `d` points: the
/// thumb's travel (`track - length`) spans the whole scroll range.
fn thumb_scroll(d: f32, range: f32, travel: f32) -> f32 {
    if travel > 0.0 {
        d * range / travel
    } else {
        0.0
    }
}

// Accessors.

fn viewport(state: &ViewState, horizontal: bool) -> f32 {
    if horizontal {
        state.frame.width
    } else {
        state.frame.height
    }
}

fn main(s: &ScrollState) -> f32 {
    if s.horizontal { s.x } else { s.y }
}

/// The rest limits along the main axis (the top is lower while the
/// refresh spinner shows).
fn limits(state: &ViewState) -> (f32, f32) {
    let Some(s) = &state.scroll else {
        return (0.0, 0.0);
    };
    let max = s.max((state.frame.width, state.frame.height));
    let min = if !s.horizontal && s.live.refreshing {
        -REFRESH_SPACE
    } else {
        0.0
    };
    (min, if s.horizontal { max.0 } else { max.1 })
}

/// The offset reported to the engine and in events: UIKit's
/// `contentOffset`, within the content.
fn reported(state: &ViewState) -> (f32, f32) {
    let Some(s) = &state.scroll else {
        return (0.0, 0.0);
    };
    let (mx, my) = s.max((state.frame.width, state.frame.height));
    let inset = s.behavior.content_inset;
    (
        s.x.clamp(0.0, mx) - inset.left,
        s.y.clamp(0.0, my) - inset.top,
    )
}

/// Milliseconds on the display link's clock.
fn now() -> f64 {
    objc2_quartz_core::CACurrentMediaTime() * 1000.0
}

fn phase(view: &NshView) -> Phase {
    view.state()
        .scroll
        .as_ref()
        .map_or(Phase::Idle, |s| s.live.phase)
}

fn set_phase(view: &NshView, phase: Phase) {
    if let Some(s) = view.state_mut().scroll.as_mut() {
        s.live.phase = phase;
    }
}

/// Whether the user may scroll `view`.
fn enabled(view: &NshView) -> bool {
    view.state()
        .scroll
        .as_ref()
        .is_some_and(|s| s.behavior.enabled)
}

/// Whether `view` is in a window (gets frames).
fn mapped(view: &NshView) -> bool {
    view.ns().window().is_some()
}

/// Whether `view` is a scroll view the user can scroll by `(dx, dy)`
/// (wheel deltas: positive moves the content up / left): it is enabled,
/// scrolls along that axis and is not at that edge yet. Input uses it to
/// pick which of nested scroll views a wheel moves.
pub(crate) fn can_scroll(view: &NshView, dx: f32, dy: f32) -> bool {
    let state = view.state();
    let Some(s) = &state.scroll else {
        return false;
    };
    if !s.behavior.enabled {
        return false;
    }
    let (min, max) = limits(&state);
    if max <= min {
        return false;
    }
    let d = if s.horizontal {
        let d = if dx != 0.0 { dx } else { dy };
        if s.mirrored && dx != 0.0 { -d } else { d }
    } else {
        dy
    };
    let pos = main(s);
    (d > 0.0 && pos < max - 0.5) || (d < 0.0 && pos > min + 0.5)
}

/// Whether a trackpad gesture by `(dx, dy)` may stretch `view` past its
/// edge: it is enabled, bounces and scrolls along the gesture's axis.
/// Input gives it gestures no scroll view can follow ([`can_scroll`]).
pub(crate) fn can_bounce(view: &NshView, dx: f32, dy: f32) -> bool {
    let state = view.state();
    state.scroll.as_ref().is_some_and(|s| {
        s.behavior.enabled && s.behavior.bounces && s.horizontal == (dx.abs() > dy.abs())
    })
}

/// Sends `kind` with the offset if JS listens to it.
fn emit(surface: &Surface, view: &NshView, kind: EventKind) {
    let (id, x, y) = {
        let state = view.state();
        if !state.events.contains(kind) {
            return;
        }
        let (x, y) = reported(&state);
        (state.id, x, y)
    };
    surface.event(id, kind, EventPayload::Scroll { x, y });
}

/// Ends momentum or a snap a new input interrupts (`onMomentumScrollEnd`,
/// as UIKit sends when a drag catches a glide).
fn interrupt(surface: &Rc<Surface>, view: &NshView) {
    settle(surface, view);
    let momentum = matches!(
        phase(view),
        Phase::Momentum | Phase::Animate { momentum: true, .. }
    );
    set_phase(view, Phase::Idle);
    if momentum {
        emit(surface, view, EventKind::MomentumScrollEnd);
    }
}

/// A trackpad event at `time` (ms, on the display link's clock) put the
/// offset at `pos`: shown from the next frame on ([`Resampled`]).
fn feed(surface: &Rc<Surface>, view: &NshView, pos: f32, time: f64) {
    if !mapped(view) {
        return moved_to(surface, view, pos);
    }
    {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        let shown = main(s);
        let r = s.live.resampled.get_or_insert_with(|| Resampled {
            points: Default::default(),
            interval: 1000.0 / 120.0,
        });
        if r.points.is_empty() {
            // The motion starts from what shows, an interval before.
            r.points.push_back((time - r.interval, shown));
        }
        r.push(time, pos);
    }
    ensure_ticking(surface, view);
}

/// The offset trackpad events have put `view` at (ahead of what shows
/// while [`Resampled`] catches up).
fn goal(s: &ScrollState) -> f32 {
    s.live
        .resampled
        .as_ref()
        .and_then(Resampled::goal)
        .unwrap_or_else(|| main(s))
}

/// Shows where trackpad events put `view` at once (something else moves
/// it from there).
fn settle(surface: &Rc<Surface>, view: &NshView) {
    let goal = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        match s.live.resampled.take().and_then(|r| r.goal()) {
            Some(goal) => goal,
            None => return,
        }
    };
    moved_to(surface, view, goal);
}

// Moving.

/// Shows the offset: the container's bounds origin.
fn apply_offset(view: &NshView) {
    let origin = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        let inset = s.behavior.content_inset;
        let x = s.x - inset.left;
        // Mirrored children are placed from the right: the offset runs the
        // other way.
        (if s.mirrored { -x } else { x }, s.y - inset.top)
    };
    view.set_bounds_origin(origin.0, origin.1);
}

/// Moves `view`'s main axis offset to `pos` (not clamped: drags rubber
/// band) and updates everything that follows it.
fn moved_to(surface: &Rc<Surface>, view: &NshView, pos: f32) {
    let (id, x, y) = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        let target = if s.horizontal { &mut s.x } else { &mut s.y };
        if *target == pos {
            return;
        }
        *target = pos;
        if s.indicator {
            s.live.shown = now();
            s.live.alpha = 1.0;
            s.live.dark = surface.environment.get().color_scheme == nsh_engine::ColorScheme::Dark;
        }
        s.live.report = true;
        let (x, y) = reported(&state);
        (state.id, x, y)
    };
    crate::surface::quiet(|| {
        apply_offset(view);
        pin_sticky(surface, view);
        update_overlays(view);
    });
    crate::input::content_moved(surface);
    if surface.anim.borrow_mut().scrolled(id, x, y) && surface.mounter.try_borrow().is_ok() {
        // While a batch is applied the surface writes animations after it.
        surface.write_animations();
    }
    if mapped(view) {
        ensure_ticking(surface, view);
    } else {
        // No frames come while it is not in a window: report now.
        report(surface, view);
    }
}

fn report(surface: &Surface, view: &NshView) {
    let (id, x, y, applied) = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        if !std::mem::take(&mut s.live.report) {
            return;
        }
        let applied = s.applied;
        let (x, y) = reported(&state);
        (state.id, x, y, applied)
    };
    surface.control(|c| c.scroll(id, x, y, applied));
}

/// Eases to `to` (clamped to the rest limits).
fn animate_to(
    surface: &Rc<Surface>,
    view: &NshView,
    to: f32,
    duration: f64,
    momentum: bool,
    end: bool,
) {
    settle(surface, view);
    let (min, max) = limits(&view.state());
    let to = to.clamp(min, max.max(min));
    let from = view.state().scroll.as_ref().map_or(0.0, main);
    if (to - from).abs() < 0.5 || duration <= 0.0 {
        set_phase(view, Phase::Idle);
        moved_to(surface, view, to);
        if momentum {
            emit(surface, view, EventKind::MomentumScrollEnd);
        }
        if end {
            emit(surface, view, EventKind::ScrollEnd);
        }
        return;
    }
    set_phase(
        view,
        Phase::Animate {
            from,
            to,
            start: now(),
            duration,
            momentum,
            end,
        },
    );
    ensure_ticking(surface, view);
}

/// The user let go at `v` points per ms along the main axis (positive:
/// the offset grows): bounces back, snaps or settles. Snapping uses
/// `snap_velocity` when it is not 0 (a settled mouse wheel).
fn release(surface: &Rc<Surface>, view: &NshView, v: f32, snap_velocity: f32) {
    settle(surface, view);
    let (pos, min, max, target) = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        let (min, max) = limits(&state);
        let pos = main(s);
        let lead = if s.horizontal {
            s.behavior.content_inset.left
        } else {
            s.behavior.content_inset.top
        };
        let dim = viewport(&state, s.horizontal);
        let snap_velocity = if snap_velocity == 0.0 {
            v
        } else {
            snap_velocity
        };
        // Snap points are content offsets: UIKit's coordinates.
        let target = s
            .behavior
            .snap_target(
                pos - lead,
                projected(pos, v, rate(&s.behavior)) - lead,
                snap_velocity,
                dim,
                max + dim - lead,
            )
            .map(|t| t + lead);
        (pos, min, max, target)
    };
    if pos < min - 0.5 || pos > max + 0.5 {
        animate_to(
            surface,
            view,
            pos.clamp(min, max.max(min)),
            ANIMATION,
            false,
            true,
        );
    } else if let Some(target) = target {
        let duration = snap_duration(target - pos, v);
        if duration > 0.0 {
            emit(surface, view, EventKind::MomentumScrollBegin);
        }
        animate_to(surface, view, target, duration, duration > 0.0, true);
    } else {
        set_phase(view, Phase::Idle);
        emit(surface, view, EventKind::ScrollEnd);
    }
}

// The frame clock.

fn ensure_ticking(surface: &Rc<Surface>, view: &NshView) {
    {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        if s.live.ticking {
            return;
        }
        s.live.ticking = true;
    }
    let view = view.clone();
    surface.add_frame_callback(move |surface, now| {
        let busy = tick(surface, &view, now * 1000.0);
        if !busy && let Some(s) = view.state_mut().scroll.as_mut() {
            s.live.ticking = false;
        }
        busy
    });
}

/// One frame of `view`'s motion; false when there is nothing left to do.
fn tick(surface: &Rc<Surface>, view: &NshView, now: f64) -> bool {
    let sampled = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return false;
        };
        // Events still come in these phases: hold at the newest until the
        // next one, rather than ending.
        let input = matches!(
            s.live.phase,
            Phase::Wheel { gesture: true, .. } | Phase::AwaitMomentum { .. } | Phase::Momentum
        );
        match s.live.resampled.as_mut() {
            Some(r) => {
                // A little more than an interval back: the event after it
                // has come by then, even a little late.
                let (pos, past) = r.sample(now - r.interval - 2.0);
                if past && !input {
                    s.live.resampled = None;
                }
                Some(pos)
            }
            None => None,
        }
    };
    if let Some(pos) = sampled {
        moved_to(surface, view, pos);
    }
    let (phase, pos, (min, max)) = {
        let state = view.state();
        let Some(s) = &state.scroll else { return false };
        (s.live.phase, main(s), limits(&state))
    };
    match phase {
        Phase::Animate {
            from,
            to,
            start,
            duration,
            momentum,
            end,
        } => {
            let t = (now - start) / duration;
            moved_to(surface, view, from + (to - from) * ease_out(t));
            if t >= 1.0 {
                set_phase(view, Phase::Idle);
                if momentum {
                    emit(surface, view, EventKind::MomentumScrollEnd);
                }
                if end {
                    emit(surface, view, EventKind::ScrollEnd);
                }
            }
        }
        Phase::Wheel {
            last,
            total,
            gesture: false,
            ..
        } if now - last > WHEEL_IDLE => {
            // A mouse wheel came to rest: snapping views move on to the
            // next snap point in the wheel's direction.
            set_phase(view, Phase::Idle);
            let snaps = view
                .state()
                .scroll
                .as_ref()
                .is_some_and(|s| s.behavior.snaps() || s.behavior.paging);
            if snaps {
                let v = if total.abs() >= 1.0 {
                    total.signum() * WHEEL_SNAP
                } else {
                    0.0
                };
                release(surface, view, 0.0, v);
            } else {
                emit(surface, view, EventKind::ScrollEnd);
            }
        }
        Phase::AwaitMomentum { since } if now - since > MOMENTUM_WAIT => {
            set_phase(view, Phase::Idle);
            emit(surface, view, EventKind::ScrollEnd);
        }
        Phase::Idle if pos > max.max(min) + 0.5 => moved_to(surface, view, max.max(min)),
        _ => {}
    }
    report(surface, view);
    let (busy, overlays) = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return false;
        };
        let mut busy = !matches!(
            s.live.phase,
            Phase::Idle
                | Phase::Thumb { .. }
                | Phase::Momentum
                | Phase::Wheel { gesture: true, .. }
        ) || s.live.resampled.is_some();
        let mut overlays = false;
        if matches!(s.live.phase, Phase::Thumb { .. }) {
            // A held thumb stays until it is let go (`thumb_end`).
            s.live.shown = now;
        } else if s.live.alpha > 0.0 {
            let alpha = indicator_alpha(now - s.live.shown);
            if alpha != s.live.alpha {
                s.live.alpha = alpha;
                overlays = true;
            }
            busy |= alpha > 0.0;
        }
        if !s.horizontal && main(s) < 0.0 && s.live.refreshing {
            s.live.spin = ((now / 1000.0) as f32 * 2.0 * PI) % (2.0 * PI);
            overlays = true;
            busy = true;
        }
        (busy, overlays)
    };
    if overlays {
        update_overlays(view);
    }
    busy
}

// The indicator and the refresh spinner.

fn rect(x: f32, y: f32, w: f32, h: f32) -> CGRect {
    CGRect::new(
        CGPoint::new(x as f64, y as f64),
        CGSize::new(w.max(0.0) as f64, h.max(0.0) as f64),
    )
}

define_class!(
    /// Holds a scroll view's indicator and refresh spinner above its
    /// container (and so above the content), as macOS's overlay scrollers
    /// are; never takes the mouse.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NshScrollOverlay"]
    struct Overlay;

    unsafe impl NSObjectProtocol for Overlay {}

    impl Overlay {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(wantsUpdateLayer))]
        fn wants_update_layer(&self) -> bool {
            true
        }

        #[unsafe(method(updateLayer))]
        fn update_layer(&self) {}

        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            None
        }
    }
);

/// A layer in `view`'s overlay view (made on first use), which stays the
/// size of the view.
fn overlay_layer(
    view: &NshView,
    overlay: &mut Option<Retained<NSView>>,
) -> Option<Retained<CALayer>> {
    let host = match overlay {
        Some(host) => host.clone(),
        None => {
            // SAFETY: NSView's designated initializer.
            let host: Retained<Overlay> =
                unsafe { msg_send![Overlay::alloc(view.mtm()), initWithFrame: view.ns().bounds()] };
            host.setWantsLayer(true);
            view.ns().addSubview(&host);
            let host = Retained::into_super(host);
            *overlay = Some(host.clone());
            host
        }
    };
    let layer = CALayer::new();
    layer.setAnchorPoint(CGPoint::new(0.0, 0.0));
    host.layer()?.addSublayer(&layer);
    Some(layer)
}

/// The indicator's frame in a `w` × `h` scroll view and how far it can
/// travel along its track, or `None` when the content fits.
fn thumb(s: &ScrollState, w: f32, h: f32) -> Option<(CGRect, f32)> {
    const EDGE: f32 = 2.0;
    let inset = s.behavior.content_inset;
    let (dim, total, offset) = if s.horizontal {
        (w, s.content.0 + inset.left + inset.right, s.x)
    } else {
        (h, s.content.1 + inset.top + inset.bottom, s.y)
    };
    let (start, len) = indicator_span(dim, total, offset)?;
    let frame = if s.horizontal {
        let x = if s.mirrored { w - start - len } else { start };
        rect(x, h - INDICATOR_THICK - EDGE, len, INDICATOR_THICK)
    } else {
        rect(w - INDICATOR_THICK - EDGE, start, INDICATOR_THICK, len)
    };
    Some((frame, dim - 2.0 * INDICATOR_MARGIN - len))
}

/// Updates the indicator and the refresh spinner of `view` to its offset.
fn update_overlays(view: &NshView) {
    let mut state = view.state_mut();
    let (w, h) = (state.frame.width, state.frame.height);
    let refresh_listened = state.events.contains(EventKind::Refresh);
    let Some(s) = state.scroll.as_mut() else {
        return;
    };
    if let Some(overlay) = &s.live.overlay {
        let size = overlay.frame().size;
        if size.width != w as f64 || size.height != h as f64 {
            overlay.setFrame(rect(0.0, 0.0, w, h));
        }
    }
    // The indicator.
    let frame = if s.indicator && s.live.alpha > 0.0 {
        thumb(s, w, h).map(|(frame, _)| frame)
    } else {
        None
    };
    match frame {
        Some(frame) => {
            if s.live.indicator.is_none() {
                s.live.indicator = overlay_layer(view, &mut s.live.overlay);
            }
            if let Some(layer) = &s.live.indicator {
                layer.setBounds(rect(
                    0.0,
                    0.0,
                    frame.size.width as f32,
                    frame.size.height as f32,
                ));
                layer.setPosition(frame.origin);
                layer.setCornerRadius(INDICATOR_THICK as f64 / 2.0);
                let color = if s.live.dark {
                    CGColor::new_srgb(1.0, 1.0, 1.0, 0.55)
                } else {
                    CGColor::new_srgb(0.0, 0.0, 0.0, 0.45)
                };
                layer.setBackgroundColor(Some(&color));
                layer.setOpacity(s.live.alpha);
                layer.setHidden(false);
            }
        }
        None => {
            if let Some(layer) = &s.live.indicator {
                layer.setHidden(true);
            }
        }
    }
    // The refresh spinner.
    let shows = !s.horizontal && s.y < 0.0 && (s.live.refreshing || refresh_listened);
    if !shows {
        if let Some(layer) = &s.live.spinner {
            layer.setHidden(true);
        }
        return;
    }
    if s.live.spinner.is_none() {
        s.live.spinner = overlay_layer(view, &mut s.live.overlay).inspect(|layer| {
            for _ in 0..SPOKES {
                let spoke = CALayer::new();
                spoke.setBounds(rect(0.0, 0.0, 2.5, 5.5));
                spoke.setCornerRadius(1.25);
                layer.addSublayer(&spoke);
            }
        });
    }
    let Some(layer) = &s.live.spinner else { return };
    layer.setHidden(false);
    let progress = if s.live.refreshing {
        1.0
    } else {
        (-s.y / REFRESH_PULL).min(1.0)
    };
    let cy = s.behavior.content_inset.top - s.y - REFRESH_SPACE / 2.0;
    layer.setBounds(rect(0.0, 0.0, 0.0, 0.0));
    layer.setPosition(CGPoint::new((w / 2.0) as f64, cy as f64));
    let shown = ((progress * SPOKES as f32).ceil() as usize).min(SPOKES);
    let step = 2.0 * PI / SPOKES as f32;
    // Turns in whole spokes, as a native spinner does.
    let lead = (s.live.spin / step) as usize % SPOKES;
    let base = if s.live.dark { 1.0 } else { 0.25 };
    // SAFETY: a getter; the spinner's sublayers are the spokes we added,
    // never mutated elsewhere while this runs (main thread).
    let Some(spokes) = (unsafe { layer.sublayers() }) else {
        return;
    };
    for (i, spoke) in spokes.iter().enumerate() {
        let fade = if progress < 1.0 {
            0.8
        } else {
            1.0 - ((lead + SPOKES - i) % SPOKES) as f32 / SPOKES as f32 * 0.85
        };
        // Each spoke 7.25 points from the center, pointing outwards.
        let angle = i as f32 * step;
        let (sin, cos) = angle.sin_cos();
        spoke.setPosition(CGPoint::new((sin * 7.25) as f64, (-cos * 7.25) as f64));
        spoke.setAffineTransform(objc2_core_foundation::CGAffineTransform {
            a: cos as f64,
            b: sin as f64,
            c: -sin as f64,
            d: cos as f64,
            tx: 0.0,
            ty: 0.0,
        });
        let color = CGColor::new_srgb(base, base, base, fade as f64);
        spoke.setBackgroundColor(Some(&color));
        spoke.setHidden(i >= shown);
    }
}

/// Spokes of the refresh spinner.
const SPOKES: usize = 8;

// Sticky cells.

/// Calls `f` with `view`'s child of node `id`.
fn with_cell(surface: &Surface, view: &NshView, id: nsh_core::NodeId, f: impl FnOnce(&NshView)) {
    if let Ok(mounter) = surface.mounter.try_borrow() {
        if let Some(cell) = mounter.views.get(&id) {
            f(cell);
        }
        return;
    }
    // While a batch is applied: look among the children.
    if let Some(cell) = view.child_views().into_iter().find(|c| c.id() == id) {
        f(&cell);
    }
}

/// Translates the sticky cells to the offset.
fn pin_sticky(surface: &Surface, view: &NshView) {
    let (cells, offset, horizontal) = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        if s.sticky.is_empty() {
            return;
        }
        let lead = if s.horizontal {
            s.behavior.content_inset.left
        } else {
            s.behavior.content_inset.top
        };
        (s.sticky.clone(), main(s) - lead, s.horizontal)
    };
    for c in &cells {
        let t = sticky_shift(offset, c.start, c.end);
        let shift = if horizontal { (t, 0.0) } else { (0.0, t) };
        with_cell(surface, view, c.cell, |cell| {
            let changed = {
                let mut cs = cell.state_mut();
                std::mem::replace(&mut cs.shift, shift) != shift
            };
            if changed {
                cell.place();
            }
        });
    }
}

/// Keeps the sticky cells last among `view`'s children, so they draw (and
/// take pointers) above the other cells.
fn front_sticky(view: &NshView) {
    let sticky: Vec<nsh_core::NodeId> = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        s.sticky.iter().map(|c| c.cell).collect()
    };
    if sticky.is_empty() {
        return;
    }
    let children = view.child_views();
    let is_sticky = |v: &NshView| sticky.contains(&v.id());
    let at_end = children.iter().rev().take_while(|v| is_sticky(v)).count();
    let cells: Vec<&NshView> = children.iter().filter(|v| is_sticky(v)).collect();
    if at_end == cells.len() {
        return;
    }
    let container: Retained<NSView> = view.container();
    for cell in cells {
        // Re-adding moves it on top of its siblings (hosted views are not
        // in a scroll view's container).
        crate::surface::quiet(|| container.addSubview(cell.ns()));
    }
}

/// A child was inserted into scroll view `view` (mounting): new cells must
/// not cover the pinned ones.
pub(crate) fn child_inserted(view: &NshView) {
    front_sticky(view);
}

// Mount items.

/// `ScrollConfig`.
pub(crate) fn config(
    _surface: &Rc<Surface>,
    view: &NshView,
    horizontal: bool,
    indicator: bool,
    mirrored: bool,
) {
    let changed = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        s.horizontal = horizontal;
        s.indicator = indicator;
        if !indicator {
            s.live.alpha = 0.0;
        }
        let mirrored = mirrored && horizontal;
        std::mem::replace(&mut s.mirrored, mirrored) != mirrored
    };
    if changed {
        // The children's places depend on the mirroring.
        for child in view.child_views() {
            child.place();
        }
    }
    apply_offset(view);
    update_overlays(view);
}

/// `ScrollContent`. Content that shrank below the offset pulls it back on
/// the next frame, once the rest of the batch (the `ScrollBy` that keeps
/// the visible content in place) is applied.
pub(crate) fn content(surface: &Rc<Surface>, view: &NshView, size: Size) {
    let past = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        s.content = (size.width, size.height);
        let pos = main(s);
        pos > limits(&state).1 + 0.5
    };
    if past {
        ensure_ticking(surface, view);
    }
    update_overlays(view);
}

/// `ScrollBy`: the core keeps the visible content in place (items above
/// the viewport changed size); not clamped, the core knows its content.
pub(crate) fn scroll_by(surface: &Rc<Surface>, view: &NshView, dx: f32, dy: f32) {
    let (pos, d) = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        s.applied = s.applied.wrapping_add(1);
        // The off axis would only push the content out of the viewport.
        let d = if s.horizontal { dx } else { dy };
        // A gesture or animation in flight moves along.
        s.live.phase = match s.live.phase {
            Phase::Thumb { start } => Phase::Thumb { start: start + d },
            Phase::Wheel {
                last,
                total,
                gesture,
                raw,
            } => Phase::Wheel {
                last,
                total,
                gesture,
                raw: raw + d,
            },
            Phase::Animate {
                from,
                to,
                start,
                duration,
                momentum,
                end,
            } => Phase::Animate {
                from: from + d,
                to: to + d,
                start,
                duration,
                momentum,
                end,
            },
            other => other,
        };
        if let Some(r) = s.live.resampled.as_mut() {
            for p in &mut r.points {
                p.1 += d;
            }
        }
        s.live.report = true;
        (main(s), d)
    };
    if d != 0.0 {
        moved_to(surface, view, pos + d);
    }
    report(surface, view);
}

/// `StickyCells`.
pub(crate) fn sticky(surface: &Rc<Surface>, view: &NshView, cells: Vec<StickyCell>) {
    let old = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        std::mem::replace(&mut s.sticky, cells)
    };
    // Cells no longer sticky go back to their frames.
    for c in old {
        let still = view
            .state()
            .scroll
            .as_ref()
            .is_some_and(|s| s.sticky.iter().any(|n| n.cell == c.cell));
        if !still {
            with_cell(surface, view, c.cell, |cell| {
                cell.state_mut().shift = (0.0, 0.0);
                cell.place();
            });
        }
    }
    front_sticky(view);
    pin_sticky(surface, view);
}

/// `ScrollBehavior`.
pub(crate) fn behavior(_surface: &Rc<Surface>, view: &NshView, behavior: &ScrollBehavior) {
    let inset = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        let inset = s.behavior.content_inset != behavior.content_inset;
        s.behavior = behavior.clone();
        if !behavior.enabled && matches!(s.live.phase, Phase::Thumb { .. } | Phase::Wheel { .. }) {
            s.live.phase = Phase::Idle;
        }
        inset
    };
    if inset {
        apply_offset(view);
        update_overlays(view);
    }
}

/// `Refreshing`: shows the spinner above the content (scrolling to it
/// when at the top) or hides it.
pub(crate) fn refreshing(surface: &Rc<Surface>, view: &NshView, refreshing: bool) {
    let (pos, held) = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        if s.horizontal || s.live.refreshing == refreshing {
            return;
        }
        s.live.refreshing = refreshing;
        (
            main(s),
            matches!(s.live.phase, Phase::Wheel { gesture: true, .. }),
        )
    };
    update_overlays(view);
    if held {
        return;
    }
    if refreshing && pos <= 0.5 {
        animate_to(surface, view, -REFRESH_SPACE, ANIMATION, false, false);
    } else if !refreshing && pos < 0.0 {
        animate_to(surface, view, 0.0, ANIMATION, false, false);
    }
}

/// `scrollTo` and `scrollToEnd` (the core resolves `scrollToIndex` and,
/// usually, `scrollToEnd` into `scrollTo`); `zoomTo` is not supported.
pub(crate) fn command(surface: &Rc<Surface>, view: &NshView, command: &Command) {
    let (target, animated) = {
        let mut state = view.state_mut();
        let (min, max) = limits(&state);
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        let inset = s.behavior.content_inset;
        let (target, animated) = match *command {
            Command::ScrollTo { x, y, animated } => (
                if s.horizontal {
                    x + inset.left
                } else {
                    y + inset.top
                },
                animated,
            ),
            Command::ScrollToEnd { animated } => (max, animated),
            Command::ScrollToIndex { .. } | Command::ZoomTo { .. } => {
                log::debug!("scroll: {command:?} is not supported");
                return;
            }
            _ => return,
        };
        s.applied = s.applied.wrapping_add(1);
        s.live.report = true;
        (target.clamp(min, max.max(min)), animated)
    };
    interrupt(surface, view);
    if animated {
        animate_to(surface, view, target, ANIMATION, false, true);
    } else {
        moved_to(surface, view, target);
    }
    report(surface, view);
}

// Pointer drags.

/// The mouse went down in `view`: a snap after a fling stops where it is,
/// as a finger catches one on iOS. Whether it caught one.
pub(crate) fn catch(surface: &Rc<Surface>, view: &NshView) -> bool {
    let moving = matches!(phase(view), Phase::Animate { momentum: true, .. });
    if moving {
        interrupt(surface, view);
        emit(surface, view, EventKind::ScrollEnd);
    }
    moving
}

/// Whether a release of `view` at its offset starts a refresh (pulled far
/// enough past the top and JS listens), marking it refreshing.
fn pulled_to_refresh(surface: &Surface, view: &NshView) -> bool {
    let refresh = {
        let state = view.state();
        let Some(s) = &state.scroll else { return false };
        !s.horizontal
            && !s.live.refreshing
            && state.events.contains(EventKind::Refresh)
            && main(s) <= -REFRESH_PULL
    };
    if refresh {
        // Shown right away, as UIRefreshControl does; JS turns it off.
        if let Some(s) = view.state_mut().scroll.as_mut() {
            s.live.refreshing = true;
        }
        surface.event(view.id(), EventKind::Refresh, EventPayload::None);
    }
    refresh
}

/// Whether a mouse press at `(x, y)` (view coordinates) grabs `view`'s
/// indicator, faded out or not: on the desktop it is a scroll bar.
pub(crate) fn grabs_thumb(view: &NshView, x: f32, y: f32) -> bool {
    let state = view.state();
    let Some(s) = &state.scroll else {
        return false;
    };
    if !s.indicator || !s.behavior.enabled {
        return false;
    }
    let (w, h) = (state.frame.width, state.frame.height);
    let Some((frame, _)) = thumb(s, w, h) else {
        return false;
    };
    let (along, across, start, len, edge) = if s.horizontal {
        (x, y, frame.origin.x as f32, frame.size.width as f32, h)
    } else {
        (y, x, frame.origin.y as f32, frame.size.height as f32, w)
    };
    (edge - THUMB_GRAB..=edge).contains(&across)
        && (start - THUMB_SLOP..=start + len + THUMB_SLOP).contains(&along)
}

/// The mouse grabbed `view`'s indicator ([`grabs_thumb`]).
pub(crate) fn thumb_begin(surface: &Rc<Surface>, view: &NshView) {
    interrupt(surface, view);
    {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        s.live.phase = Phase::Thumb { start: main(s) };
        s.live.shown = now();
        s.live.alpha = 1.0;
        s.live.dark = surface.environment.get().color_scheme == nsh_engine::ColorScheme::Dark;
    }
    update_overlays(view);
    emit(surface, view, EventKind::ScrollBeginDrag);
}

/// The grabbed indicator was dragged by `(dx, dy)` points since it was
/// grabbed: the content moves the other way, so the thumb follows the
/// pointer (a content drag moves the content with it).
pub(crate) fn thumb_move(surface: &Rc<Surface>, view: &NshView, dx: f32, dy: f32) {
    let Phase::Thumb { start } = phase(view) else {
        return;
    };
    let pos = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        let Some((_, travel)) = thumb(s, state.frame.width, state.frame.height) else {
            return;
        };
        let (min, max) = limits(&state);
        let d = if !s.horizontal {
            dy
        } else if s.mirrored {
            -dx
        } else {
            dx
        };
        (start + thumb_scroll(d, max, travel)).clamp(min, max.max(min))
    };
    moved_to(surface, view, pos);
}

/// The grabbed indicator was let go: it fades from now, snapping views
/// settle on a snap point.
pub(crate) fn thumb_end(surface: &Rc<Surface>, view: &NshView) {
    if !matches!(phase(view), Phase::Thumb { .. }) {
        return;
    }
    set_phase(view, Phase::Idle);
    if let Some(s) = view.state_mut().scroll.as_mut() {
        s.live.shown = now();
    }
    ensure_ticking(surface, view);
    emit(surface, view, EventKind::ScrollEndDrag);
    release(surface, view, 0.0, 0.0);
}

// Wheels and trackpads.

/// The main-axis share of wheel deltas `(dx, dy)` for `s` (positive: the
/// offset grows).
fn wheel_delta(s: &ScrollState, dx: f32, dy: f32) -> f32 {
    if !s.horizontal {
        dy
    } else if dx != 0.0 {
        if s.mirrored { -dx } else { dx }
    } else {
        // Vertical wheels scroll horizontal views too.
        dy
    }
}

/// A trackpad scroll gesture over `view` began (also fingers resting on
/// it: that stops what moves).
pub(crate) fn wheel_begin(surface: &Rc<Surface>, view: &NshView) {
    if !enabled(view) {
        return;
    }
    interrupt(surface, view);
    let raw = view.state().scroll.as_ref().map_or(0.0, main);
    set_phase(
        view,
        Phase::Wheel {
            last: now(),
            total: 0.0,
            gesture: true,
            raw,
        },
    );
    emit(surface, view, EventKind::ScrollBeginDrag);
}

/// A wheel or trackpad scroll by `(dx, dy)` points (positive: the content
/// moves up / left). Without [`wheel_begin`] it is a mouse wheel, which
/// settles by itself once it stops.
pub(crate) fn wheel(surface: &Rc<Surface>, view: &NshView, dx: f32, dy: f32, time: f64) {
    if !enabled(view) {
        return;
    }
    let (gesture, total, raw) = match phase(view) {
        Phase::Wheel {
            gesture,
            total,
            raw,
            ..
        } => (gesture, total, raw),
        _ => {
            interrupt(surface, view);
            let raw = view.state().scroll.as_ref().map_or(0.0, main);
            (false, 0.0, raw)
        }
    };
    let (pos, total, raw) = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        let (min, max) = limits(&state);
        let d = wheel_delta(s, dx, dy);
        let pos = main(s);
        if gesture {
            // Trackpads stretch past the edges, as macOS does.
            let raw = raw + d;
            let to = dragged(
                raw,
                min,
                max.max(min),
                viewport(&state, s.horizontal),
                s.behavior.bounces,
            );
            (to, total + d, raw)
        } else {
            // Past an edge (the refresh space) wheels only move back.
            let lo = min.min(pos);
            let hi = max.max(min).max(pos);
            let to = (pos + d).clamp(lo, hi);
            (to, total + d, to)
        }
    };
    set_phase(
        view,
        Phase::Wheel {
            last: now(),
            total,
            gesture,
            raw,
        },
    );
    if gesture {
        feed(surface, view, pos, time);
    } else {
        moved_to(surface, view, pos);
    }
    ensure_ticking(surface, view);
}

/// The trackpad gesture ended with velocity `(vx, vy)` points per second:
/// springs back from past an edge, snaps, or waits for the system's
/// momentum.
pub(crate) fn wheel_end(surface: &Rc<Surface>, view: &NshView, vx: f32, vy: f32) {
    let Phase::Wheel { gesture, total, .. } = phase(view) else {
        return;
    };
    let (v, snaps) = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        (
            wheel_delta(s, vx, vy) / 1000.0,
            s.behavior.snaps() || s.behavior.paging,
        )
    };
    set_phase(view, Phase::Idle);
    if gesture {
        emit(surface, view, EventKind::ScrollEndDrag);
        pulled_to_refresh(surface, view);
    }
    // After the refresh check: refreshing lowers the top limit.
    let (pos, (min, max)) = {
        let state = view.state();
        (state.scroll.as_ref().map_or(0.0, goal), limits(&state))
    };
    let outside = pos < min - 0.5 || pos > max.max(min) + 0.5;
    if gesture && !outside && !snaps {
        // The system's momentum may follow ([`momentum_begin`]).
        set_phase(view, Phase::AwaitMomentum { since: now() });
        ensure_ticking(surface, view);
        return;
    }
    let snap = if v == 0.0 && total.abs() >= 1.0 {
        total.signum() * WHEEL_SNAP
    } else {
        0.0
    };
    release(surface, view, v, snap);
}

/// The system's momentum after a trackpad gesture began on `view`: whether
/// the view takes it (not when it springs back or snaps instead).
pub(crate) fn momentum_begin(surface: &Rc<Surface>, view: &NshView) -> bool {
    if !matches!(phase(view), Phase::AwaitMomentum { .. }) {
        return false;
    }
    set_phase(view, Phase::Momentum);
    emit(surface, view, EventKind::MomentumScrollBegin);
    true
}

/// The system's momentum moved `view` by `(dx, dy)` points (as
/// [`wheel`]); it stops at the edges.
pub(crate) fn momentum(surface: &Rc<Surface>, view: &NshView, dx: f32, dy: f32, time: f64) {
    if phase(view) != Phase::Momentum {
        return;
    }
    let pos = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        let (min, max) = limits(&state);
        let pos = goal(s);
        let lo = min.min(pos);
        let hi = max.max(min).max(pos);
        (pos + wheel_delta(s, dx, dy)).clamp(lo, hi)
    };
    feed(surface, view, pos, time);
}

/// The system's momentum on `view` ended.
pub(crate) fn momentum_end(surface: &Rc<Surface>, view: &NshView) {
    if phase(view) != Phase::Momentum {
        return;
    }
    set_phase(view, Phase::Idle);
    emit(surface, view, EventKind::MomentumScrollEnd);
    emit(surface, view, EventKind::ScrollEnd);
}

// The keyboard.

/// Points an arrow key scrolls, as browsers do.
const KEY_LINE: f32 = 40.0;
/// The share of the viewport a page key scrolls (the rest stays in view).
const KEY_PAGE: f32 = 0.875;
/// Duration of keyboard scrolls (ms); repeats retarget the running one.
const KEY_ANIMATION: f64 = 200.0;

/// Where a scroll key moves along an axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum KeyMove {
    /// By this many points (positive: towards the end).
    Points(f32),
    /// By this many viewports.
    Pages(f32),
    /// To the start.
    Start,
    /// To the end.
    End,
}

/// Virtual key codes of the keys that scroll.
mod keys {
    pub(crate) const SPACE: u16 = 49;
    pub(crate) const HOME: u16 = 115;
    pub(crate) const PAGE_UP: u16 = 116;
    pub(crate) const END: u16 = 119;
    pub(crate) const PAGE_DOWN: u16 = 121;
    pub(crate) const LEFT: u16 = 123;
    pub(crate) const RIGHT: u16 = 124;
    pub(crate) const DOWN: u16 = 125;
    pub(crate) const UP: u16 = 126;
}

/// What key `code` scrolls: whether along the horizontal axis, and how.
/// `shortcut` is Command, Control or Option held (shortcuts never scroll).
pub(crate) fn key_move(code: u16, shift: bool, shortcut: bool) -> Option<(bool, KeyMove)> {
    if shortcut {
        return None;
    }
    Some(match code {
        keys::UP => (false, KeyMove::Points(-KEY_LINE)),
        keys::DOWN => (false, KeyMove::Points(KEY_LINE)),
        keys::LEFT => (true, KeyMove::Points(-KEY_LINE)),
        keys::RIGHT => (true, KeyMove::Points(KEY_LINE)),
        keys::PAGE_UP => (false, KeyMove::Pages(-1.0)),
        keys::PAGE_DOWN => (false, KeyMove::Pages(1.0)),
        keys::SPACE if shift => (false, KeyMove::Pages(-1.0)),
        keys::SPACE => (false, KeyMove::Pages(1.0)),
        keys::HOME => (false, KeyMove::Start),
        keys::END => (false, KeyMove::End),
        _ => return None,
    })
}

/// The offset a key move from `base` aims at, within `[min, max]`.
fn key_target(m: KeyMove, base: f32, viewport: f32, min: f32, max: f32) -> f32 {
    let max = max.max(min);
    match m {
        KeyMove::Points(d) => base + d,
        KeyMove::Pages(n) => base + n * (viewport * KEY_PAGE).max(KEY_LINE),
        KeyMove::Start => min,
        KeyMove::End => max,
    }
    .clamp(min, max)
}

/// Scrolls the innermost scroll view from `from` outwards that the key
/// move `(horizontal, m)` can move, animated; whether one moved.
pub(crate) fn key_scroll(
    surface: &Rc<Surface>,
    from: &NshView,
    horizontal: bool,
    m: KeyMove,
) -> bool {
    let sign = match m {
        KeyMove::Points(d) => d.signum(),
        KeyMove::Pages(n) => n.signum(),
        KeyMove::Start => -1.0,
        KeyMove::End => 1.0,
    };
    let (dx, dy) = if horizontal { (sign, 0.0) } else { (0.0, sign) };
    let Some(view) = key_scroll_view(from, horizontal, dx, dy) else {
        return false;
    };
    let (base, viewport, min, max) = {
        let state = view.state();
        let Some(s) = &state.scroll else {
            return false;
        };
        let (min, max) = limits(&state);
        // A repeat continues from where the running scroll goes.
        let base = match s.live.phase {
            Phase::Animate { to, .. } => to,
            _ => main(s),
        };
        (base, viewport(&state, s.horizontal), min, max)
    };
    let to = key_target(m, base, viewport, min, max);
    interrupt(surface, &view);
    animate_to(surface, &view, to, KEY_ANIMATION, false, true);
    true
}

/// The innermost scroll view from `view` outwards that a key moving by
/// `(dx, dy)` can move; for vertical keys one along that axis first, then
/// a horizontal one (as vertical wheels move those).
fn key_scroll_view(view: &NshView, horizontal: bool, dx: f32, dy: f32) -> Option<NshView> {
    let mut chain = Vec::new();
    let mut current = Some(view.clone());
    while let Some(v) = current {
        current = v.parent_view();
        if v.kind() == ViewKind::Scroll && can_scroll(&v, dx, dy) {
            chain.push(v);
        }
    }
    let along = |v: &NshView| v.state().scroll.as_ref().is_some_and(|s| s.horizontal) == horizontal;
    let first = chain.iter().position(along).unwrap_or(0);
    chain.into_iter().nth(first)
}

/// Room kept around a view scrolled into view.
const REVEAL_MARGIN: f32 = 16.0;

/// How far a scroll view must move along an axis so that the span
/// `[start, end)` (viewport coordinates) shows in `[0, viewport)`, with
/// a margin; the start wins when the span does not fit.
fn reveal_delta(start: f32, end: f32, viewport: f32) -> f32 {
    let margin = REVEAL_MARGIN.min(viewport / 4.0);
    if start < margin {
        start - margin
    } else if end > viewport - margin {
        (end - (viewport - margin)).min(start - margin)
    } else {
        0.0
    }
}

/// Scrolls the scroll views around `target` (animated) so that it shows,
/// as iOS and Android do for the focused field. For the text fields: call
/// it when one gets the keyboard focus.
pub(crate) fn reveal(surface: &Rc<Surface>, target: &NshView) {
    // Content moved by the scroll views inside, which rectangles measured
    // now do not show yet.
    let (mut moved_x, mut moved_y) = (0.0, 0.0);
    let mut current = target.parent_view();
    while let Some(view) = current {
        current = view.parent_view();
        if view.kind() != ViewKind::Scroll || !mapped(&view) {
            continue;
        }
        let bounds = target
            .ns()
            .convertRect_toView(target.ns().bounds(), Some(view.ns()));
        let (horizontal, base, dim, min, max, pos) = {
            let state = view.state();
            let Some(s) = &state.scroll else { continue };
            let base = match s.live.phase {
                Phase::Animate { to, .. } => to,
                _ => main(s),
            };
            let (min, max) = limits(&state);
            (
                s.horizontal,
                base,
                viewport(&state, s.horizontal),
                min,
                max,
                main(s),
            )
        };
        let (start, len) = if horizontal {
            (bounds.origin.x as f32 - moved_x, bounds.size.width as f32)
        } else {
            (bounds.origin.y as f32 - moved_y, bounds.size.height as f32)
        };
        // Pending animations count from where they are headed.
        let start = start - (base - pos);
        let delta = reveal_delta(start, start + len, dim);
        let to = (base + delta).clamp(min, max.max(min));
        if (to - base).abs() < 0.5 {
            continue;
        }
        if horizontal {
            moved_x += to - base;
        } else {
            moved_y += to - base;
        }
        interrupt(surface, &view);
        animate_to(surface, &view, to, ANIMATION, false, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_moves_evenly_between_events() {
        let mut r = Resampled {
            points: Default::default(),
            interval: 8.0,
        };
        r.points.push_back((0.0, 0.0));
        // Events every 8 ms, 10 points each; frames sample at other times.
        for i in 1..=4 {
            r.push(i as f64 * 8.0, i as f32 * 10.0);
        }
        assert!((r.interval - 8.0).abs() < 1e-9);
        assert_eq!(r.goal(), Some(40.0));
        assert_eq!(r.sample(4.0), (5.0, false));
        assert_eq!(r.sample(12.0), (15.0, false));
        assert_eq!(r.sample(20.0), (25.0, false));
        // Past the newest event: it holds there.
        assert_eq!(r.sample(50.0), (40.0, true));
        // An event at the same instant replaces the newest.
        r.push(32.0, 42.0);
        assert_eq!(r.goal(), Some(42.0));
    }

    #[test]
    fn rubber_band_resists() {
        assert_eq!(dragged(50.0, 0.0, 100.0, 400.0, true), 50.0);
        let over = dragged(-100.0, 0.0, 100.0, 400.0, true);
        assert!(over < 0.0 && over > -100.0, "{over}");
        let more = dragged(-400.0, 0.0, 100.0, 400.0, true);
        assert!(more < over && more > -400.0 * 0.55);
        assert_eq!(dragged(-100.0, 0.0, 100.0, 400.0, false), 0.0);
        assert_eq!(dragged(300.0, 0.0, 100.0, 400.0, false), 100.0);
        assert!(dragged(150.0, 0.0, 100.0, 400.0, true) > 100.0);
    }

    #[test]
    fn sticky_shift_clamps() {
        assert_eq!(sticky_shift(0.0, 100.0, 300.0), 0.0);
        assert_eq!(sticky_shift(150.0, 100.0, 300.0), 50.0);
        assert_eq!(sticky_shift(500.0, 100.0, f32::INFINITY), 400.0);
    }

    #[test]
    fn indicator_geometry() {
        assert_eq!(indicator_span(400.0, 400.0, 0.0), None);
        let (s0, l0) = indicator_span(400.0, 1600.0, 0.0).unwrap();
        assert_eq!(s0, 2.0);
        assert!((l0 - 99.0).abs() < 0.01);
        let (_, l2) = indicator_span(400.0, 1600.0, -50.0).unwrap();
        assert!(l2 < l0);
        assert_eq!(indicator_alpha(INDICATOR_HOLD + INDICATOR_FADE), 0.0);
    }

    #[test]
    fn thumb_follows_the_pointer() {
        // The thumb's travel spans the scroll range: moving it by its
        // whole travel scrolls to the end, and it ends that far along.
        let (s0, l0) = indicator_span(400.0, 1600.0, 0.0).unwrap();
        let travel = 400.0 - 2.0 * INDICATOR_MARGIN - l0;
        let end = thumb_scroll(travel, 1200.0, travel);
        assert!((end - 1200.0).abs() < 0.01);
        let (s1, _) = indicator_span(400.0, 1600.0, end).unwrap();
        assert!((s1 - s0 - travel).abs() < 0.01);
        assert!(thumb_scroll(-10.0, 1200.0, travel) < 0.0);
        assert_eq!(thumb_scroll(10.0, 1200.0, 0.0), 0.0);
    }

    #[test]
    fn scroll_keys() {
        assert_eq!(
            key_move(keys::DOWN, false, false),
            Some((false, KeyMove::Points(KEY_LINE)))
        );
        assert_eq!(
            key_move(keys::SPACE, true, false),
            Some((false, KeyMove::Pages(-1.0)))
        );
        assert_eq!(key_move(keys::LEFT, false, true), None);
        assert_eq!(key_move(0, false, false), None);
        assert_eq!(
            key_target(KeyMove::Pages(1.0), 0.0, 400.0, 0.0, 1000.0),
            350.0
        );
        assert_eq!(key_target(KeyMove::End, 0.0, 400.0, 0.0, 1000.0), 1000.0);
    }

    #[test]
    fn reveal_deltas() {
        assert_eq!(reveal_delta(100.0, 140.0, 400.0), 0.0);
        assert_eq!(reveal_delta(-30.0, 10.0, 400.0), -46.0);
        assert_eq!(reveal_delta(200.0, 900.0, 400.0), 184.0);
    }
}
