//! Scroll views: offsets, wheel / touchpad / touch / keyboard scrolling with
//! momentum, snapping, sticky cells, the core's scroll requests, scroll
//! events.
//!
//! A scroll view only moves along its main axis (zooming is not
//! supported: `zoomTo` and the zoom props are ignored). Its offset lives in
//! [`ScrollState`] (`x` / `y`, 0 showing the start of the content inset);
//! [`NshView`]'s `size_allocate` places the children by it. Every change
//! goes through [`moved_to`], which re-places the children, pins the
//! sticky cells, feeds the native scroll value in the same frame and
//! queues a report to the engine. Reports are coalesced to one per frame
//! by the view's own tick callback, which also runs glides, animated
//! scrolls, the indicator's fade and the refresh spinner, and stops when
//! none of them is left.
//!
//! Offsets are reported as UIKit's `contentOffset` (minus the leading
//! content inset, clamped to the content), so lists see the same numbers
//! as on iOS. Glides decelerate as UIKit does (velocity kept per
//! millisecond: `behavior.deceleration`, 0.998 by default, 0.99 when
//! snapping); snapping and paging use [`ScrollBehavior::snap_target`].
//! Touch drags rubber-band past the edges when `bounces`; wheels and
//! touchpads stop at the edges, as desktop scrolling does.
//!
//! Mirrored (right to left) horizontal lists keep their offsets left to
//! right: `size_allocate` places each child at the mirror image of its
//! frame, and drags, wheels and the indicator are mirrored here.

use std::{f32::consts::PI, rc::Rc};

use gtk4::{self as gtk, gdk, glib, graphene, prelude::*};
use nsh_core::{Command, EventKind, EventPayload, ScrollBehavior, Size, StickyCell, ViewKind};

use crate::{
    surface::Surface,
    view::{NshView, ScrollState, ViewState},
};

/// Below this release speed (points per ms) a drag does not glide.
const MIN_GLIDE: f32 = 0.05;
/// A glide stops below this speed (points per ms).
const STOP: f32 = 0.01;
/// A wheel without a gesture (a mouse) has settled after this many ms.
const WHEEL_IDLE: f64 = 150.0;
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
/// How close to its edge (points) a mouse press grabs the indicator,
/// which is much thinner than a pointer is precise...
const THUMB_GRAB: f32 = 12.0;
/// ...and how far past its ends.
const THUMB_SLOP: f32 = 4.0;

/// What a scroll view is doing.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum Phase {
    /// At rest.
    #[default]
    Idle,
    /// A touch-style drag: `start` is the main-axis offset it began at.
    Drag { start: f32 },
    /// The mouse drags the indicator as a scroll bar's thumb, from offset
    /// `start`.
    Thumb { start: f32 },
    /// Wheel or touchpad input: the last at `last` (ms), `total` points
    /// so far; `gesture` for touchpads (begin / end), not mouse wheels.
    Wheel {
        last: f64,
        total: f32,
        gesture: bool,
    },
    /// Momentum after a fling: `velocity` points per ms at `last` (ms).
    Glide { velocity: f32, last: f64 },
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
    /// The view's tick callback runs.
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

/// One step of a glide: `(distance, new velocity)` after `dt` ms at
/// velocity `v` (points per ms), decaying exponentially by `rate` per ms.
fn glide_step(v: f32, rate: f32, dt: f32) -> (f32, f32) {
    let k = rate.powf(dt);
    (v * (k - 1.0) / rate.ln(), v * k)
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
    // Overscrolling shrinks it, as on iOS.
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

fn now() -> f64 {
    glib::monotonic_time() as f64 / 1000.0
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

/// Ends a glide or a snap a new input interrupts (`onMomentumScrollEnd`,
/// as UIKit sends when a drag catches a glide).
fn interrupt(surface: &Surface, view: &NshView) {
    let momentum = matches!(
        phase(view),
        Phase::Glide { .. } | Phase::Animate { momentum: true, .. }
    );
    set_phase(view, Phase::Idle);
    if momentum {
        emit(surface, view, EventKind::MomentumScrollEnd);
    }
}

// Moving.

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
    view.queue_allocate();
    view.queue_draw();
    pin_sticky(surface, view);
    if surface.anim.borrow_mut().scrolled(id, x, y) && surface.mounter.try_borrow().is_ok() {
        // While a batch is applied the surface writes animations after it.
        surface.write_animations();
    }
    if view.is_mapped() {
        ensure_ticking(surface, view);
    } else {
        // No frames come while unmapped: report now.
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
/// the offset grows): bounces back, snaps, glides or settles. Snapping
/// uses `snap_velocity` when it is not 0 (a settled mouse wheel).
fn release(surface: &Rc<Surface>, view: &NshView, v: f32, snap_velocity: f32) {
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
    } else if v.abs() > MIN_GLIDE && (v > 0.0 && pos < max || v < 0.0 && pos > min) {
        set_phase(
            view,
            Phase::Glide {
                velocity: v,
                last: now(),
            },
        );
        emit(surface, view, EventKind::MomentumScrollBegin);
        ensure_ticking(surface, view);
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
    let weak = Rc::downgrade(surface);
    view.add_tick_callback(move |view, clock| {
        let Some(surface) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let now = clock.frame_time() as f64 / 1000.0;
        if tick(&surface, view, now) {
            glib::ControlFlow::Continue
        } else {
            if let Some(s) = view.state_mut().scroll.as_mut() {
                s.live.ticking = false;
            }
            glib::ControlFlow::Break
        }
    });
}

/// One frame of `view`'s motion; false when there is nothing left to do.
fn tick(surface: &Rc<Surface>, view: &NshView, now: f64) -> bool {
    let (phase, pos, rate, (min, max)) = {
        let state = view.state();
        let Some(s) = &state.scroll else { return false };
        (s.live.phase, main(s), rate(&s.behavior), limits(&state))
    };
    match phase {
        Phase::Glide { velocity, last } => {
            let dt = (now - last).max(0.0) as f32;
            let (d, v) = glide_step(velocity, rate, dt);
            let next = pos + d;
            let stop = v.abs() < STOP || next <= min || next >= max;
            moved_to(surface, view, next.clamp(min, max.max(min)));
            if stop {
                set_phase(view, Phase::Idle);
                emit(surface, view, EventKind::MomentumScrollEnd);
                emit(surface, view, EventKind::ScrollEnd);
            } else {
                set_phase(
                    view,
                    Phase::Glide {
                        velocity: v,
                        last: now,
                    },
                );
            }
        }
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
        Phase::Idle if pos > max.max(min) + 0.5 => moved_to(surface, view, max.max(min)),
        _ => {}
    }
    report(surface, view);
    let mut state = view.state_mut();
    let Some(s) = state.scroll.as_mut() else {
        return false;
    };
    let mut busy = !matches!(
        s.live.phase,
        Phase::Idle | Phase::Drag { .. } | Phase::Thumb { .. } | Phase::Wheel { gesture: true, .. }
    );
    if matches!(s.live.phase, Phase::Thumb { .. }) {
        // A held thumb stays until it is let go (`thumb_end`).
        s.live.shown = now;
    } else if s.live.alpha > 0.0 {
        let alpha = indicator_alpha(now - s.live.shown);
        if alpha != s.live.alpha {
            s.live.alpha = alpha;
            view.queue_draw();
        }
        busy |= alpha > 0.0;
    }
    if !s.horizontal && main(s) < 0.0 && s.live.refreshing {
        s.live.spin = ((now / 1000.0) as f32 * 2.0 * PI) % (2.0 * PI);
        view.queue_draw();
        busy = true;
    }
    busy
}

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
    let offset = main(s) - lead;
    for c in &s.sticky {
        let t = sticky_shift(offset, c.start, c.end);
        let shift = if s.horizontal { (t, 0.0) } else { (0.0, t) };
        with_cell(surface, view, c.cell, |cell| {
            let mut cs = cell.state_mut();
            if cs.shift != shift {
                cs.shift = shift;
                drop(cs);
                cell.moved();
            }
        });
    }
}

/// Keeps the sticky cells last among `view`'s children, so they draw (and
/// take pointers) above the other cells.
fn front_sticky(view: &NshView) {
    let cells = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        if s.sticky.is_empty() {
            return;
        }
        let is_sticky = |w: &gtk::Widget| {
            w.downcast_ref::<NshView>()
                .is_some_and(|v| s.sticky.iter().any(|c| c.cell == v.id()))
        };
        // Reorder only when a sticky cell is not among the last children.
        let mut at_end = 0;
        let mut child = view.last_child();
        while let Some(w) = child.filter(is_sticky) {
            at_end += 1;
            child = w.prev_sibling();
        }
        let mut cells = Vec::new();
        let mut child = view.first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            if is_sticky(&w) {
                cells.push(w);
            }
        }
        if at_end == cells.len() {
            return;
        }
        cells
    };
    for cell in cells {
        cell.insert_before(view, None::<&gtk::Widget>);
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
        // A horizontal view keeps its indicator: it is the mouse's scroll
        // bar, and a mouse without a horizontal wheel has no other way
        // to move it (mouse drags do not scroll).
        s.indicator = indicator || horizontal;
        if !indicator {
            s.live.alpha = 0.0;
        }
        let mirrored = mirrored && horizontal;
        std::mem::replace(&mut s.mirrored, mirrored) != mirrored
    };
    if changed {
        view.queue_allocate();
    }
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
    view.queue_draw();
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
            Phase::Drag { start } => Phase::Drag { start: start + d },
            Phase::Thumb { start } => Phase::Thumb { start: start + d },
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
                cell.moved();
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
        if !behavior.enabled
            && matches!(
                s.live.phase,
                Phase::Drag { .. } | Phase::Thumb { .. } | Phase::Wheel { .. }
            )
        {
            s.live.phase = Phase::Idle;
        }
        inset
    };
    if inset {
        view.queue_allocate();
    }
}

/// `Refreshing`: shows the spinner above the content (scrolling to it
/// when at the top) or hides it.
pub(crate) fn refreshing(surface: &Rc<Surface>, view: &NshView, refreshing: bool) {
    let (pos, dragging) = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        if s.horizontal || s.live.refreshing == refreshing {
            return;
        }
        s.live.refreshing = refreshing;
        (main(s), matches!(s.live.phase, Phase::Drag { .. }))
    };
    view.queue_draw();
    if dragging {
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

// Input.

/// A touchscreen drag on `view` began (the mouse never drags content).
pub(crate) fn drag_begin(surface: &Rc<Surface>, view: &NshView) {
    if !enabled(view) {
        return;
    }
    interrupt(surface, view);
    let start = view.state().scroll.as_ref().map_or(0.0, main);
    set_phase(view, Phase::Drag { start });
    emit(surface, view, EventKind::ScrollBeginDrag);
}

/// The drag moved by `(dx, dy)` points since it began.
pub(crate) fn drag_move(surface: &Rc<Surface>, view: &NshView, dx: f32, dy: f32) {
    let Phase::Drag { start } = phase(view) else {
        return;
    };
    let pos = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        let (min, max) = limits(&state);
        let d = if !s.horizontal {
            -dy
        } else if s.mirrored {
            dx
        } else {
            -dx
        };
        dragged(
            start + d,
            min,
            max.max(min),
            viewport(&state, s.horizontal),
            s.behavior.bounces,
        )
    };
    moved_to(surface, view, pos);
}

/// The drag ended at velocity `(vx, vy)` points per second.
pub(crate) fn drag_end(surface: &Rc<Surface>, view: &NshView, vx: f32, vy: f32) {
    if !matches!(phase(view), Phase::Drag { .. }) {
        return;
    }
    let (v, refresh) = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        let v = if !s.horizontal {
            -vy
        } else if s.mirrored {
            vx
        } else {
            -vx
        } / 1000.0;
        let refresh = !s.horizontal
            && !s.live.refreshing
            && state.events.contains(EventKind::Refresh)
            && main(s) <= -REFRESH_PULL;
        (v, refresh)
    };
    set_phase(view, Phase::Idle);
    emit(surface, view, EventKind::ScrollEndDrag);
    if refresh {
        // Shown right away, as UIRefreshControl does; JS turns it off.
        if let Some(s) = view.state_mut().scroll.as_mut() {
            s.live.refreshing = true;
        }
        surface.event(view.id(), EventKind::Refresh, EventPayload::None);
    }
    release(surface, view, v, 0.0);
}

/// The indicator's thickness.
const INDICATOR_THICK: f32 = 3.0;

/// The indicator's rectangle in its view and how far it can travel along
/// its track, or `None` when the content fits.
fn thumb(state: &ViewState) -> Option<(graphene::Rect, f32)> {
    const EDGE: f32 = 2.0;
    let s = state.scroll.as_ref()?;
    let (w, h) = (state.frame.width, state.frame.height);
    let inset = s.behavior.content_inset;
    let (dim, total, offset) = if s.horizontal {
        (w, s.content.0 + inset.left + inset.right, s.x)
    } else {
        (h, s.content.1 + inset.top + inset.bottom, s.y)
    };
    let (start, len) = indicator_span(dim, total, offset)?;
    let rect = if s.horizontal {
        let x = if s.mirrored { w - start - len } else { start };
        graphene::Rect::new(x, h - INDICATOR_THICK - EDGE, len, INDICATOR_THICK)
    } else {
        graphene::Rect::new(w - INDICATOR_THICK - EDGE, start, INDICATOR_THICK, len)
    };
    Some((rect, dim - 2.0 * INDICATOR_MARGIN - len))
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
    let Some((rect, _)) = thumb(&state) else {
        return false;
    };
    let (along, across, start, len, edge) = if s.horizontal {
        (x, y, rect.x(), rect.width(), state.frame.height)
    } else {
        (y, x, rect.y(), rect.height(), state.frame.width)
    };
    (edge - THUMB_GRAB..=edge).contains(&across)
        && (start - THUMB_SLOP..=start + len + THUMB_SLOP).contains(&along)
}

/// The mouse moved over `view`: its indicator shows (and fades again
/// once the mouse rests), as GTK's overlay scroll bars do, so that a
/// mouse user sees it can scroll and where its scroll bar is.
pub(crate) fn hovered(surface: &Rc<Surface>, view: &NshView) {
    {
        let mut state = view.state_mut();
        if thumb(&state).is_none() {
            return;
        }
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        if !s.indicator || !s.behavior.enabled {
            return;
        }
        s.live.shown = now();
        if s.live.alpha == 1.0 {
            return;
        }
        s.live.alpha = 1.0;
        s.live.dark = surface.environment.get().color_scheme == nsh_engine::ColorScheme::Dark;
    }
    view.queue_draw();
    ensure_ticking(surface, view);
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
    view.queue_draw();
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
        let Some((_, travel)) = thumb(&state) else {
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

/// Draws a scroll view's indicator, over its content (inside its clip),
/// and the refresh spinner.
pub(crate) fn draw_indicator(snapshot: &gtk::Snapshot, state: &ViewState) {
    let Some(s) = &state.scroll else { return };
    let w = state.frame.width;
    if !s.horizontal
        && s.y < 0.0
        && (s.live.refreshing || state.events.contains(EventKind::Refresh))
    {
        let progress = if s.live.refreshing {
            1.0
        } else {
            (-s.y / REFRESH_PULL).min(1.0)
        };
        let cy = s.behavior.content_inset.top - s.y - REFRESH_SPACE / 2.0;
        draw_spinner(snapshot, w / 2.0, cy, progress, s.live.spin, s.live.dark);
    }
    if !s.indicator || s.live.alpha <= 0.0 {
        return;
    }
    let Some((rect, _)) = thumb(state) else {
        return;
    };
    let color = if s.live.dark {
        gdk::RGBA::new(1.0, 1.0, 1.0, 0.55 * s.live.alpha)
    } else {
        gdk::RGBA::new(0.0, 0.0, 0.0, 0.45 * s.live.alpha)
    };
    snapshot.push_rounded_clip(&crate::view::rounded(rect, [INDICATOR_THICK / 2.0; 4]));
    snapshot.append_color(&color, &rect);
    snapshot.pop();
}

/// An activity spinner of eight spokes at `(cx, cy)`; `progress` of them
/// show while pulling.
fn draw_spinner(snapshot: &gtk::Snapshot, cx: f32, cy: f32, progress: f32, spin: f32, dark: bool) {
    const SPOKES: usize = 8;
    let shown = ((progress * SPOKES as f32).ceil() as usize).min(SPOKES);
    let step = 2.0 * PI / SPOKES as f32;
    // Turns in whole spokes, as a native spinner does.
    let lead = (spin / step) as usize % SPOKES;
    let base = if dark { 1.0 } else { 0.25 };
    for i in 0..shown {
        let fade = if progress < 1.0 {
            0.8
        } else {
            1.0 - ((lead + SPOKES - i) % SPOKES) as f32 / SPOKES as f32 * 0.85
        };
        snapshot.save();
        snapshot.translate(&graphene::Point::new(cx, cy));
        snapshot.rotate((i as f32 * step).to_degrees());
        let rect = graphene::Rect::new(-1.25, -10.0, 2.5, 5.5);
        snapshot.push_rounded_clip(&crate::view::rounded(rect, [1.25; 4]));
        snapshot.append_color(&gdk::RGBA::new(base, base, base, fade), &rect);
        snapshot.pop();
        snapshot.restore();
    }
}

/// A wheel or touchpad scroll gesture over `view` began.
pub(crate) fn wheel_begin(surface: &Rc<Surface>, view: &NshView) {
    if !enabled(view) {
        return;
    }
    interrupt(surface, view);
    set_phase(
        view,
        Phase::Wheel {
            last: now(),
            total: 0.0,
            gesture: true,
        },
    );
    emit(surface, view, EventKind::ScrollBeginDrag);
}

/// A wheel or touchpad scroll by `(dx, dy)` points (positive: content
/// moves up / left, as GTK reports wheel deltas).
pub(crate) fn wheel(surface: &Rc<Surface>, view: &NshView, dx: f32, dy: f32) {
    if !enabled(view) {
        return;
    }
    let (gesture, total) = match phase(view) {
        Phase::Wheel { gesture, total, .. } => (gesture, total),
        _ => {
            // A mouse wheel: no begin, it settles when idle.
            interrupt(surface, view);
            (false, 0.0)
        }
    };
    let (pos, total) = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        let (min, max) = limits(&state);
        let d = if !s.horizontal {
            dy
        } else if dx != 0.0 {
            if s.mirrored { -dx } else { dx }
        } else {
            // Vertical wheels scroll horizontal views too.
            dy
        };
        let pos = main(s);
        // Past an edge (the refresh space) wheels only move back.
        let lo = min.min(pos);
        let hi = max.max(min).max(pos);
        ((pos + d).clamp(lo, hi), total + d)
    };
    set_phase(
        view,
        Phase::Wheel {
            last: now(),
            total,
            gesture,
        },
    );
    moved_to(surface, view, pos);
    ensure_ticking(surface, view);
}

/// The touchpad gesture ended with velocity `(vx, vy)` points per second
/// (0 for wheels without momentum).
pub(crate) fn wheel_end(surface: &Rc<Surface>, view: &NshView, vx: f32, vy: f32) {
    let Phase::Wheel { gesture, total, .. } = phase(view) else {
        return;
    };
    let v = {
        let state = view.state();
        let Some(s) = &state.scroll else { return };
        if !s.horizontal {
            vy
        } else if vx != 0.0 {
            if s.mirrored { -vx } else { vx }
        } else {
            vy
        }
    } / 1000.0;
    set_phase(view, Phase::Idle);
    if gesture {
        emit(surface, view, EventKind::ScrollEndDrag);
    }
    let snap = if v == 0.0 && total.abs() >= 1.0 {
        total.signum() * WHEEL_SNAP
    } else {
        0.0
    };
    release(surface, view, v, snap);
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
enum KeyMove {
    /// By this many points (positive: towards the end).
    Points(f32),
    /// By this many viewports.
    Pages(f32),
    /// To the start.
    Start,
    /// To the end.
    End,
}

/// What a key scrolls: whether along the horizontal axis, and how.
fn key_move(key: gdk::Key, modifiers: gdk::ModifierType) -> Option<(bool, KeyMove)> {
    use gdk::Key;
    // Ctrl / Alt / Super combinations are shortcuts (Alt+Left goes back).
    let shortcuts = gdk::ModifierType::CONTROL_MASK
        | gdk::ModifierType::ALT_MASK
        | gdk::ModifierType::SUPER_MASK;
    if modifiers.intersects(shortcuts) {
        return None;
    }
    let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
    Some(match key {
        Key::Up | Key::KP_Up => (false, KeyMove::Points(-KEY_LINE)),
        Key::Down | Key::KP_Down => (false, KeyMove::Points(KEY_LINE)),
        Key::Left | Key::KP_Left => (true, KeyMove::Points(-KEY_LINE)),
        Key::Right | Key::KP_Right => (true, KeyMove::Points(KEY_LINE)),
        Key::Page_Up | Key::KP_Page_Up => (false, KeyMove::Pages(-1.0)),
        Key::Page_Down | Key::KP_Page_Down => (false, KeyMove::Pages(1.0)),
        Key::space if shift => (false, KeyMove::Pages(-1.0)),
        Key::space => (false, KeyMove::Pages(1.0)),
        Key::Home | Key::KP_Home => (false, KeyMove::Start),
        Key::End | Key::KP_End => (false, KeyMove::End),
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

/// Lets the keyboard scroll the surface's scroll views: arrows by a line,
/// Page Up / Page Down and Space / Shift+Space by a page, Home / End to
/// the edges. The keys go to the window (they reach it when nothing
/// focused took them) and move the innermost scroll view around the
/// focused widget, or else under the pointer, that can move that way.
/// A widget that gets the focus (Tab, a click, `focus()`) is scrolled
/// into view.
pub(crate) fn install_keyboard(surface: &Rc<Surface>) {
    let weak = Rc::downgrade(surface);
    let installed = std::cell::RefCell::new(glib::WeakRef::<gtk::Window>::new());
    surface.root.connect_realize(move |root| {
        let Some(window) = root.root().and_downcast::<gtk::Window>() else {
            return;
        };
        if installed.borrow().upgrade().as_ref() == Some(&window) {
            return;
        }
        installed.borrow().set(Some(&window));
        let keys = gtk::EventControllerKey::new();
        let pressed = weak.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| match pressed.upgrade() {
            Some(surface) if key_pressed(&surface, key, modifiers) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        });
        window.add_controller(keys);
        let weak = weak.clone();
        window.connect_focus_widget_notify(move |window| {
            if let (Some(surface), Some(focus)) = (weak.upgrade(), GtkWindowExt::focus(window)) {
                reveal(&surface, &focus);
            }
        });
    });
}

/// Room kept around a widget scrolled into view.
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

/// Scrolls the scroll views around the newly focused `focus` (animated)
/// so that the view hosting it shows, as iOS and Android do for the
/// focused field.
fn reveal(surface: &Rc<Surface>, focus: &gtk::Widget) {
    let root = surface.root.upcast_ref::<gtk::Widget>();
    if focus == root || !focus.is_ancestor(root) {
        return;
    }
    let Some(target) = nearest_view(focus) else {
        return;
    };
    // Content moved by the scroll views inside, which bounds measured
    // now do not show yet.
    let (mut moved_x, mut moved_y) = (0.0, 0.0);
    let mut current = target.parent_view();
    while let Some(view) = current {
        current = view.parent_view();
        if view.kind() != ViewKind::Scroll || !view.is_mapped() {
            continue;
        }
        let Some(bounds) = target.compute_bounds(&view) else {
            continue;
        };
        let (horizontal, base, dim, min, max) = {
            let state = view.state();
            let Some(s) = &state.scroll else { continue };
            let base = match s.live.phase {
                Phase::Animate { to, .. } => to,
                _ => main(s),
            };
            let (min, max) = limits(&state);
            (s.horizontal, base, viewport(&state, s.horizontal), min, max)
        };
        let (start, len) = if horizontal {
            (bounds.x() - moved_x, bounds.width())
        } else {
            (bounds.y() - moved_y, bounds.height())
        };
        // Pending animations count from where they are headed.
        let start = start - (base - view.state().scroll.as_ref().map_or(base, main));
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

/// A key reached the window: whether it scrolled a view.
fn key_pressed(surface: &Rc<Surface>, key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
    let Some((horizontal, m)) = key_move(key, modifiers) else {
        return false;
    };
    let root = &surface.root;
    if !root.is_mapped() {
        return false;
    }
    let focus = root.root().and_then(|r| r.focus());
    // Fields (and hosted widgets that scroll themselves) keep their keys,
    // also those they do not use: Down in a one-line field is no scroll.
    if focus
        .as_ref()
        .is_some_and(|f| f.is::<gtk::Text>() || f.is::<gtk::TextView>() || native_owner(f))
    {
        return false;
    }
    let sign = match m {
        KeyMove::Points(d) => d.signum(),
        KeyMove::Pages(n) => n.signum(),
        KeyMove::Start => -1.0,
        KeyMove::End => 1.0,
    };
    let (dx, dy) = if horizontal { (sign, 0.0) } else { (0.0, sign) };
    let from_focus = focus
        .filter(|f| f.is_ancestor(root) || f == root.upcast_ref::<gtk::Widget>())
        .and_then(|f| nearest_view(&f));
    let target = from_focus
        .and_then(|v| key_scroll_view(&v, horizontal, dx, dy))
        .or_else(|| {
            let (x, y) = pointer(root)
                .unwrap_or_else(|| (root.width() as f32 / 2.0, root.height() as f32 / 2.0));
            let widget = root.pick(x as f64, y as f64, gtk::PickFlags::DEFAULT)?;
            key_scroll_view(&nearest_view(&widget)?, horizontal, dx, dy)
        });
    let Some(view) = target else {
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

/// Whether `widget` belongs to a hosted widget (a web view, a video's
/// controls), which handles the keyboard itself.
fn native_owner(widget: &gtk::Widget) -> bool {
    !widget.is::<NshView>() && widget.ancestor(NshView::static_type()).is_some()
}

/// `widget` if it is a view, or the view it is hosted in.
fn nearest_view(widget: &gtk::Widget) -> Option<NshView> {
    widget
        .downcast_ref::<NshView>()
        .cloned()
        .or_else(|| widget.ancestor(NshView::static_type()).and_downcast())
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

/// The pointer in `root`'s coordinates, if it is over its window.
fn pointer(root: &NshView) -> Option<(f32, f32)> {
    let native = root.native()?;
    let pointer = root.display().default_seat()?.pointer()?;
    let (x, y, _) = native.surface()?.device_position(&pointer)?;
    let (tx, ty) = native.surface_transform();
    let p = native.compute_point(
        root,
        &graphene::Point::new((x - tx) as f32, (y - ty) as f32),
    )?;
    let inside =
        p.x() >= 0.0 && p.y() >= 0.0 && p.x() < root.width() as f32 && p.y() < root.height() as f32;
    inside.then(|| (p.x(), p.y()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn glide_integrates_to_projection() {
        let rate = 0.998;
        let (mut pos, mut v) = (0.0f32, 2.0f32);
        for _ in 0..2000 {
            let (d, nv) = glide_step(v, rate, 16.0);
            pos += d;
            v = nv;
        }
        let end = projected(0.0, 2.0, rate);
        assert!((pos - end).abs() < 1.0, "{pos} vs {end}");
        // Frame rate does not change the path.
        let (a, _) = glide_step(2.0, rate, 32.0);
        let (b1, v1) = glide_step(2.0, rate, 16.0);
        let (b2, _) = glide_step(v1, rate, 16.0);
        assert!((a - (b1 + b2)).abs() < 1e-3);
    }

    #[test]
    fn sticky_shift_clamps() {
        assert_eq!(sticky_shift(0.0, 100.0, 300.0), 0.0);
        assert_eq!(sticky_shift(150.0, 100.0, 300.0), 50.0);
        assert_eq!(sticky_shift(500.0, 100.0, 300.0), 200.0);
        assert_eq!(sticky_shift(500.0, 100.0, f32::INFINITY), 400.0);
    }

    #[test]
    fn rates_follow_behavior() {
        let mut b = ScrollBehavior::default();
        assert_eq!(rate(&b), 0.998);
        b.paging = true;
        assert_eq!(rate(&b), 0.99);
        b.deceleration = 0.95;
        assert_eq!(rate(&b), 0.95);
    }

    #[test]
    fn indicator_geometry() {
        assert_eq!(indicator_span(400.0, 400.0, 0.0), None);
        let (s0, l0) = indicator_span(400.0, 1600.0, 0.0).unwrap();
        assert_eq!(s0, 2.0);
        assert!((l0 - 99.0).abs() < 0.01);
        let (s1, l1) = indicator_span(400.0, 1600.0, 1200.0).unwrap();
        assert!((s1 + l1 - 398.0).abs() < 0.01);
        let (_, l2) = indicator_span(400.0, 1600.0, -50.0).unwrap();
        assert!(l2 < l0);
        assert_eq!(indicator_alpha(0.0), 1.0);
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
    fn snap_durations() {
        assert_eq!(snap_duration(0.2, 1.0), 0.0);
        assert_eq!(snap_duration(100.0, -1.0), ANIMATION);
        assert_eq!(snap_duration(300.0, 1.0), 600.0);
        assert_eq!(snap_duration(300.0, 3.0), 300.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert_eq!(ease_out(2.0), 1.0);
    }
    #[test]
    fn scroll_keys() {
        use gdk::{Key, ModifierType as M};
        assert_eq!(
            key_move(Key::Down, M::empty()),
            Some((false, KeyMove::Points(KEY_LINE)))
        );
        assert_eq!(
            key_move(Key::Left, M::empty()),
            Some((true, KeyMove::Points(-KEY_LINE)))
        );
        assert_eq!(
            key_move(Key::space, M::SHIFT_MASK),
            Some((false, KeyMove::Pages(-1.0)))
        );
        assert_eq!(key_move(Key::End, M::empty()), Some((false, KeyMove::End)));
        assert_eq!(key_move(Key::Left, M::ALT_MASK), None);
        assert_eq!(key_move(Key::a, M::empty()), None);
        assert_eq!(
            key_target(KeyMove::Points(40.0), 10.0, 400.0, 0.0, 1000.0),
            50.0
        );
        assert_eq!(
            key_target(KeyMove::Pages(1.0), 0.0, 400.0, 0.0, 1000.0),
            350.0
        );
        assert_eq!(
            key_target(KeyMove::Pages(-1.0), 100.0, 400.0, 0.0, 1000.0),
            0.0
        );
        assert_eq!(key_target(KeyMove::End, 0.0, 400.0, 0.0, 1000.0), 1000.0);
        assert_eq!(
            key_target(KeyMove::Start, 500.0, 400.0, -56.0, 1000.0),
            -56.0
        );
    }
    #[test]
    fn reveal_deltas() {
        assert_eq!(reveal_delta(100.0, 140.0, 400.0), 0.0);
        assert_eq!(reveal_delta(-30.0, 10.0, 400.0), -46.0);
        assert_eq!(reveal_delta(380.0, 420.0, 400.0), 36.0);
        // Taller than the viewport: its start shows.
        assert_eq!(reveal_delta(200.0, 900.0, 400.0), 184.0);
    }
}
