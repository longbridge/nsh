//! Scroll views: offsets, wheel / touchpad / touch / keyboard scrolling with
//! momentum, snapping, sticky cells, the core's scroll requests, scroll
//! events.
//!
//! A scroll view only moves along its main axis (zooming is not
//! supported: `zoomTo` and the zoom props are ignored). Its offset lives in
//! [`ScrollState`] (`x` / `y`, 0 showing the start of the content inset);
//! [`crate::render`] draws the children at [`ViewState::child_origin`],
//! which follows it. Every change goes through [`moved_to`], which redraws,
//! pins the sticky cells, feeds the native scroll value in the same frame
//! and queues a report to the engine. Reports are coalesced to one per
//! frame by the view's frame client ([`Surface::on_frame`]), which also
//! runs glides, animated scrolls, the indicator's fade and the refresh
//! spinner, and stops when none of them is left.
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
//! right: [`ViewState::child_origin`] places each child at the mirror image
//! of its frame, and drags, wheels, keys and the indicator are mirrored
//! here.

use std::{f32::consts::PI, rc::Rc};

use nsh_core::{
    Color, Command, EventKind, EventPayload, Points, ScrollBehavior, Size, StickyCell, ViewKind,
};

use crate::{
    geom::Mat4,
    input::KeyEvent,
    keys::{VK_DOWN, VK_END, VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT, VK_SPACE, VK_UP},
    render::DrawCx,
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
/// The indicator's thickness.
const INDICATOR_THICK: f32 = 3.0;
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
    /// A touch-style drag: `start` is the main-axis offset it began at,
    /// `moved` how far the pointer went since (content follows it).
    Drag { start: f32, moved: f32 },
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
    /// The view's frame client runs.
    ticking: bool,
    /// The offset changed since it was last reported.
    report: bool,
    /// When the indicator was last shown (ms).
    shown: f64,
    /// The indicator's opacity.
    alpha: f32,
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

/// A gesture or animation in flight when the core moves the content by
/// `d` (`ScrollBy`) moves along, so that it continues from the content it
/// was on.
fn shifted(phase: Phase, d: f32) -> Phase {
    match phase {
        Phase::Drag { start, moved } => Phase::Drag {
            start: start + d,
            moved,
        },
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
    }
}

// Accessors.

fn viewport(state: &ViewState, horizontal: bool) -> f32 {
    if horizontal {
        state.frame.size.width.0
    } else {
        state.frame.size.height.0
    }
}

fn main(s: &ScrollState) -> f32 {
    if s.horizontal { s.x } else { s.y }
}

/// The leading content inset along the main axis.
fn lead(s: &ScrollState) -> f32 {
    if s.horizontal {
        s.behavior.content_inset.left.0
    } else {
        s.behavior.content_inset.top.0
    }
}

/// The rest limits along the main axis (the top is lower while the
/// refresh spinner shows).
fn limits(state: &ViewState) -> (f32, f32) {
    let Some(s) = &state.scroll else {
        return (0.0, 0.0);
    };
    let max = s.max((state.frame.size.width.0, state.frame.size.height.0));
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
    let (mx, my) = s.max((state.frame.size.width.0, state.frame.size.height.0));
    let inset = s.behavior.content_inset;
    (
        s.x.clamp(0.0, mx) - inset.left.0,
        s.y.clamp(0.0, my) - inset.top.0,
    )
}

/// The surface's clock in ms (the times phases keep).
fn now(surface: &Surface) -> f64 {
    surface.clock() * 1000.0
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
    surface.event(
        id,
        kind,
        EventPayload::Scroll(nsh_core::geometry::point(Points(x), Points(y))),
    );
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
            s.live.shown = now(surface);
            s.live.alpha = 1.0;
        }
        s.live.report = true;
        let (x, y) = reported(&state);
        (state.id, x, y)
    };
    view.moved();
    pin_sticky(surface, view);
    if surface.anim.borrow_mut().scrolled(id, x, y) && surface.mounter.try_borrow().is_ok() {
        // While a batch is applied the surface writes animations after it.
        surface.write_animations();
    }
    if view.attached() {
        ensure_ticking(surface, view);
    } else {
        // No frames draw it while detached: report now.
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
    surface.control(|c| c.scroll(id, nsh_core::geometry::point(Points(x), Points(y)), applied));
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
            start: now(surface),
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
        let lead = lead(s);
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
                Points(pos - lead),
                Points(projected(pos, v, rate(&s.behavior)) - lead),
                snap_velocity,
                Points(dim),
                Points(max + dim - lead),
            )
            .map(|t| t.0 + lead);
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
                last: now(surface),
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
    let view = view.clone();
    surface.on_frame(move |surface, now| {
        let busy = tick(surface, &view, now * 1000.0);
        if !busy && let Some(s) = view.state_mut().scroll.as_mut() {
            s.live.ticking = false;
        }
        busy
    });
}

/// One frame of `view`'s motion; false when there is nothing left to do.
fn tick(surface: &Rc<Surface>, view: &NshView, now: f64) -> bool {
    if !view.attached() {
        // Removed: nothing draws it; a later move starts again.
        set_phase(view, Phase::Idle);
        report(surface, view);
        return false;
    }
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
    let mut redraw = false;
    let busy = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return false;
        };
        let mut busy = !matches!(
            s.live.phase,
            Phase::Idle
                | Phase::Drag { .. }
                | Phase::Thumb { .. }
                | Phase::Wheel { gesture: true, .. }
        );
        if matches!(s.live.phase, Phase::Thumb { .. }) {
            // A held thumb stays until it is let go (`thumb_end`).
            s.live.shown = now;
        } else if s.live.alpha > 0.0 {
            let alpha = indicator_alpha(now - s.live.shown);
            if alpha != s.live.alpha {
                s.live.alpha = alpha;
                redraw = true;
            }
            busy |= alpha > 0.0;
        }
        if !s.horizontal && main(s) < 0.0 && s.live.refreshing {
            s.live.spin = ((now / 1000.0) as f32 * 2.0 * PI) % (2.0 * PI);
            redraw = true;
            busy = true;
        }
        busy
    };
    if redraw {
        view.changed();
    }
    busy
}

// Sticky cells.

/// Calls `f` with `view`'s child of node `id`.
fn with_cell(surface: &Surface, view: &NshView, id: nsh_core::NodeId, f: impl FnOnce(&NshView)) {
    if let Ok(mounter) = surface.mounter.try_borrow() {
        if let Some(cell) = &mounter.views.get(id) {
            f(cell);
        }
        return;
    }
    // While a batch is applied: look among the children.
    let cell = view.with_children(|c| c.iter().find(|c| c.id() == id).cloned());
    if let Some(cell) = cell {
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
    let offset = main(s) - lead(s);
    for c in &s.sticky {
        let t = sticky_shift(offset, c.start.0, c.end.0);
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
        view.with_children(|children| {
            let is_sticky = |v: &NshView| s.sticky.iter().any(|c| c.cell == v.id());
            // Reorder only when a sticky cell is not among the last children.
            let at_end = children.iter().rev().take_while(|c| is_sticky(c)).count();
            let cells: Vec<NshView> = children.iter().filter(|c| is_sticky(c)).cloned().collect();
            if at_end == cells.len() {
                Vec::new()
            } else {
                cells
            }
        })
    };
    for cell in cells {
        cell.insert_before(view, None);
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
        view.moved();
    }
}

/// `ScrollContent`. Content that shrank below the offset pulls it back on
/// the next frame, once the rest of the batch (the `ScrollBy` that keeps
/// the visible content in place) is applied.
pub(crate) fn content(surface: &Rc<Surface>, view: &NshView, size: Size<Points>) {
    let past = {
        let mut state = view.state_mut();
        let Some(s) = state.scroll.as_mut() else {
            return;
        };
        s.content = (size.width.0, size.height.0);
        let pos = main(s);
        pos > limits(&state).1 + 0.5
    };
    if past {
        ensure_ticking(surface, view);
    }
    view.changed();
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
        s.live.phase = shifted(s.live.phase, d);
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
        view.moved();
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
    view.changed();
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
                    x.0 + inset.left.0
                } else {
                    y.0 + inset.top.0
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

/// A finger or pen drag starts scrolling `view` (the mouse never drags
/// content).
pub(crate) fn drag_begin(surface: &Rc<Surface>, view: &NshView) {
    if !enabled(view) {
        return;
    }
    interrupt(surface, view);
    let start = view.state().scroll.as_ref().map_or(0.0, main);
    set_phase(view, Phase::Drag { start, moved: 0.0 });
    emit(surface, view, EventKind::ScrollBeginDrag);
}

/// The drag moved by `(dx, dy)` points since the last move (content
/// follows the pointer: positive scrolls toward the start).
pub(crate) fn drag_move(surface: &Rc<Surface>, view: &NshView, dx: f32, dy: f32) {
    let Phase::Drag { start, moved } = phase(view) else {
        return;
    };
    let (pos, moved) = {
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
        // Accumulated, not added to the offset: the rubber band applies to
        // where the pointer went, not to where the content was.
        let moved = moved + d;
        let pos = dragged(
            start + moved,
            min,
            max.max(min),
            viewport(&state, s.horizontal),
            s.behavior.bounces,
        );
        (pos, moved)
    };
    set_phase(view, Phase::Drag { start, moved });
    moved_to(surface, view, pos);
}

/// The drag ended with a velocity (points per second): a glide follows.
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
    let Some(((tx, ty, tw, th), _)) = thumb(&state) else {
        return false;
    };
    let (along, across, start, len, edge) = if s.horizontal {
        (x, y, tx, tw, state.frame.size.height.0)
    } else {
        (y, x, ty, th, state.frame.size.width.0)
    };
    (edge - THUMB_GRAB..=edge).contains(&across)
        && (start - THUMB_SLOP..=start + len + THUMB_SLOP).contains(&along)
}

/// The mouse moved over `view`: its indicator shows (and fades again
/// once the mouse rests), as overlay scroll bars do, so that a mouse user
/// sees it can scroll and where its scroll bar is.
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
        s.live.shown = now(surface);
        if s.live.alpha == 1.0 {
            return;
        }
        s.live.alpha = 1.0;
    }
    view.changed();
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
        s.live.shown = now(surface);
        s.live.alpha = 1.0;
    }
    view.changed();
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
        s.live.shown = now(surface);
    }
    ensure_ticking(surface, view);
    emit(surface, view, EventKind::ScrollEndDrag);
    release(surface, view, 0.0, 0.0);
}

/// The indicator's rectangle `(x, y, width, height)` in its view and how
/// far it can travel along its track, or `None` when the content fits.
fn thumb(state: &ViewState) -> Option<((f32, f32, f32, f32), f32)> {
    const EDGE: f32 = 2.0;
    let s = state.scroll.as_ref()?;
    let (w, h) = (state.frame.size.width.0, state.frame.size.height.0);
    let inset = s.behavior.content_inset;
    let (dim, total, offset) = if s.horizontal {
        (w, s.content.0 + inset.left.0 + inset.right.0, s.x)
    } else {
        (h, s.content.1 + inset.top.0 + inset.bottom.0, s.y)
    };
    let (start, len) = indicator_span(dim, total, offset)?;
    let rect = if s.horizontal {
        let x = if s.mirrored { w - start - len } else { start };
        (x, h - INDICATOR_THICK - EDGE, len, INDICATOR_THICK)
    } else {
        (w - INDICATOR_THICK - EDGE, start, INDICATOR_THICK, len)
    };
    Some((rect, dim - 2.0 * INDICATOR_MARGIN - len))
}

/// Draws a scroll view's indicator, over its content (inside its clip),
/// and the refresh spinner; the context is in the view's coordinates.
pub(crate) fn draw_indicator(cx: &mut DrawCx<'_>, state: &ViewState) {
    let Some(s) = &state.scroll else { return };
    let w = state.frame.size.width.0;
    if !s.horizontal
        && s.y < 0.0
        && (s.live.refreshing || state.events.contains(EventKind::Refresh))
    {
        let progress = if s.live.refreshing {
            1.0
        } else {
            (-s.y / REFRESH_PULL).min(1.0)
        };
        let cy = s.behavior.content_inset.top.0 - s.y - REFRESH_SPACE / 2.0;
        draw_spinner(cx, w / 2.0, cy, progress, s.live.spin);
    }
    if !s.indicator || s.live.alpha <= 0.0 {
        return;
    }
    let Some(((x, y, rw, rh), _)) = thumb(state) else {
        return;
    };
    // A knob with a thin border of the opposite tone, as UIKit's default
    // indicator style: the appearance says what the window is, not what
    // the content under the knob is, which may be light in a dark app.
    let a = s.live.alpha;
    let (knob, border) = if cx.dark {
        (
            Color::rgba(255, 255, 255, (140.0 * a) as u8),
            Color::rgba(0, 0, 0, (70.0 * a) as u8),
        )
    } else {
        (
            Color::rgba(0, 0, 0, (115.0 * a) as u8),
            Color::rgba(255, 255, 255, (110.0 * a) as u8),
        )
    };
    const RIM: f32 = 0.75;
    fill_rounded(
        cx,
        x - RIM,
        y - RIM,
        rw + 2.0 * RIM,
        rh + 2.0 * RIM,
        INDICATOR_THICK / 2.0 + RIM,
        border,
    );
    fill_rounded(cx, x, y, rw, rh, INDICATOR_THICK / 2.0, knob);
}

/// Fills a rounded rectangle.
fn fill_rounded(cx: &mut DrawCx<'_>, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
    cx.fill_rounded(x, y, w, h, radius, color);
}

/// An activity spinner of eight spokes at `(x, y)`; `progress` of them
/// show while pulling.
fn draw_spinner(cx: &mut DrawCx<'_>, x: f32, y: f32, progress: f32, spin: f32) {
    const SPOKES: usize = 8;
    let shown = ((progress * SPOKES as f32).ceil() as usize).min(SPOKES);
    let step = 2.0 * PI / SPOKES as f32;
    // Turns in whole spokes, as a native spinner does.
    let lead = (spin / step) as usize % SPOKES;
    let base = if cx.dark { 255.0 } else { 64.0 };
    let world = cx.transform;
    for i in 0..shown {
        let fade = if progress < 1.0 {
            0.8
        } else {
            1.0 - ((lead + SPOKES - i) % SPOKES) as f32 / SPOKES as f32 * 0.85
        };
        let m = Mat4::new()
            .translate(x, y)
            .rotate((i as f32 * step).to_degrees())
            .then(&world);
        cx.set_transform(m);
        let gray = base as u8;
        fill_rounded(
            cx,
            -1.25,
            -10.0,
            2.5,
            5.5,
            1.25,
            Color::rgba(gray, gray, gray, (fade * 255.0) as u8),
        );
    }
    cx.set_transform(world);
}

/// A wheel or touchpad scroll starts on `view` (a touchpad gesture: it
/// ends with [`wheel_end`]).
pub(crate) fn wheel_begin(surface: &Rc<Surface>, view: &NshView) {
    if !enabled(view) {
        return;
    }
    interrupt(surface, view);
    set_phase(
        view,
        Phase::Wheel {
            last: now(surface),
            total: 0.0,
            gesture: true,
        },
    );
    emit(surface, view, EventKind::ScrollBeginDrag);
}

/// Scrolls `view` by `(dx, dy)` points (positive toward the end). Without
/// [`wheel_begin`] it is a mouse wheel, which settles when idle.
pub(crate) fn wheel(surface: &Rc<Surface>, view: &NshView, dx: f32, dy: f32) {
    if !enabled(view) {
        return;
    }
    let (gesture, total) = match phase(view) {
        Phase::Wheel { gesture, total, .. } => (gesture, total),
        _ => {
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
            last: now(surface),
            total,
            gesture,
        },
    );
    moved_to(surface, view, pos);
    ensure_ticking(surface, view);
}

/// The wheel scroll ended (velocity in points per second; 0 for wheels
/// without momentum).
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
fn key_move(event: &KeyEvent) -> Option<(bool, KeyMove)> {
    let m = event.modifiers;
    if !event.down || m.alt || m.meta {
        return None;
    }
    // Ctrl+Home / Ctrl+End go to the edges, as in browsers; other Ctrl,
    // Alt and Windows key combinations are shortcuts (Alt+Left goes back).
    if m.ctrl {
        return match event.vk {
            vk if vk == VK_HOME => Some((false, KeyMove::Start)),
            vk if vk == VK_END => Some((false, KeyMove::End)),
            _ => None,
        };
    }
    Some(match event.vk {
        vk if vk == VK_UP => (false, KeyMove::Points(-KEY_LINE)),
        vk if vk == VK_DOWN => (false, KeyMove::Points(KEY_LINE)),
        vk if vk == VK_LEFT => (true, KeyMove::Points(-KEY_LINE)),
        vk if vk == VK_RIGHT => (true, KeyMove::Points(KEY_LINE)),
        vk if vk == VK_PRIOR => (false, KeyMove::Pages(-1.0)),
        vk if vk == VK_NEXT => (false, KeyMove::Pages(1.0)),
        vk if vk == VK_SPACE && m.shift => (false, KeyMove::Pages(-1.0)),
        vk if vk == VK_SPACE => (false, KeyMove::Pages(1.0)),
        vk if vk == VK_HOME => (false, KeyMove::Start),
        vk if vk == VK_END => (false, KeyMove::End),
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
/// as iOS and Android do for the focused field: text inputs call it when
/// they get the focus.
pub(crate) fn reveal(surface: &Rc<Surface>, target: &NshView) {
    let Some(bounds) = target.to_root().and_then(|m| {
        let s = target.state();
        m.bounds(0.0, 0.0, s.frame.size.width.0, s.frame.size.height.0)
    }) else {
        return;
    };
    // Content moved by the scroll views inside, which bounds measured now
    // do not show yet.
    let (mut moved_x, mut moved_y) = (0.0, 0.0);
    let mut current = target.parent_view();
    while let Some(view) = current {
        current = view.parent_view();
        if view.kind() != ViewKind::Scroll {
            continue;
        }
        let Some((ox, oy)) = view.to_root().and_then(|m| m.map(0.0, 0.0)) else {
            continue;
        };
        let (horizontal, base, pos, dim, min, max) = {
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
                main(s),
                viewport(&state, s.horizontal),
                min,
                max,
            )
        };
        let (start, len) = if horizontal {
            (bounds.0 - ox - moved_x, bounds.2)
        } else {
            (bounds.1 - oy - moved_y, bounds.3)
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

/// Keyboard scrolling (arrows by a line, Page Up / Page Down and Space /
/// Shift+Space by a page, Home / End to the edges) of the innermost
/// scroll view under `(x, y)` (root coordinates; the middle of the window
/// when the pointer is outside it) that can move that way. Fields keep
/// their keys, also those they do not use: Down in a one-line field is no
/// scroll. True when handled.
pub(crate) fn key(surface: &Rc<Surface>, event: &KeyEvent, x: f32, y: f32) -> bool {
    let Some((horizontal, m)) = key_move(event) else {
        return false;
    };
    if crate::text_input::focused(surface).is_some() {
        return false;
    }
    let root = &surface.root;
    let (w, h) = surface.size();
    let (x, y) = if x >= 0.0 && y >= 0.0 && x < w && y < h {
        (x, y)
    } else {
        (w / 2.0, h / 2.0)
    };
    let sign = match m {
        KeyMove::Points(d) => d.signum(),
        KeyMove::Pages(n) => n.signum(),
        KeyMove::Start => -1.0,
        KeyMove::End => 1.0,
    };
    let Some(view) = pick(root, x, y).and_then(|v| key_scroll_view(&v, horizontal, sign)) else {
        return false;
    };
    let (base, viewport, min, max, flip) = {
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
        // Left and Right follow what they show in a right-to-left list.
        let flip = horizontal && s.mirrored;
        (base, viewport(&state, s.horizontal), min, max, flip)
    };
    let m = match m {
        KeyMove::Points(d) if flip => KeyMove::Points(-d),
        m => m,
    };
    let to = key_target(m, base, viewport, min, max);
    interrupt(surface, &view);
    animate_to(surface, &view, to, KEY_ANIMATION, false, true);
    true
}

/// The innermost scroll view from `view` outwards that a key moving
/// towards `sign` along an axis can move; for vertical keys one along
/// that axis first, then a horizontal one (as vertical wheels move those).
fn key_scroll_view(view: &NshView, horizontal: bool, sign: f32) -> Option<NshView> {
    let mut chain = Vec::new();
    let mut current = Some(view.clone());
    while let Some(v) = current {
        current = v.parent_view();
        if v.kind() != ViewKind::Scroll {
            continue;
        }
        let (dx, dy) = {
            let state = v.state();
            let mirrored = state.scroll.as_ref().is_some_and(|s| s.mirrored);
            if horizontal {
                // can_scroll mirrors horizontal deltas; keys are mirrored
                // in `key`, so undo it here.
                (if mirrored { -sign } else { sign }, 0.0)
            } else {
                (0.0, sign)
            }
        };
        if can_scroll(&v, dx, dy) {
            chain.push(v);
        }
    }
    let along = |v: &NshView| v.state().scroll.as_ref().is_some_and(|s| s.horizontal) == horizontal;
    let first = chain.iter().position(along).unwrap_or(0);
    chain.into_iter().nth(first)
}

/// The innermost view at `(x, y)` (`view`'s coordinates).
fn pick(view: &NshView, x: f32, y: f32) -> Option<NshView> {
    let state = view.state();
    if state.style.hidden || state.concealed {
        return None;
    }
    let inside =
        x >= 0.0 && y >= 0.0 && x < state.frame.size.width.0 && y < state.frame.size.height.0;
    if !inside && (state.clips() || state.overflow.is_none()) {
        return None;
    }
    let hit = view.with_children(|children| {
        children.iter().rev().find_map(|child| {
            let local = {
                let cs = child.state();
                let (ox, oy) = state.child_origin(cs.frame, cs.shift);
                let m = Mat4::new().translate(ox, oy);
                match cs.transform() {
                    Some(t) => Mat4::mul(&t, &m),
                    None => m,
                }
            };
            let (cx, cy) = local.unproject(x, y)?;
            pick(child, cx, cy)
        })
    });
    hit.or_else(|| inside.then(|| view.clone()))
}

#[cfg(test)]
mod tests {
    use nsh_core::{EdgeInsets, Points};

    use super::*;
    use crate::input::Modifiers;

    fn scroll_view(w: f32, h: f32, content: (f32, f32), horizontal: bool) -> NshView {
        let view = NshView::new(2, ViewKind::Scroll);
        {
            let mut state = view.state_mut();
            state.frame = nsh_core::Bounds {
                origin: nsh_core::Point {
                    x: Points(0.0),
                    y: Points(0.0),
                },
                size: nsh_core::Size {
                    width: Points(w),
                    height: Points(h),
                },
            };
            let s = state.scroll.as_mut().unwrap();
            s.content = content;
            s.horizontal = horizontal;
        }
        view
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
        // Never more than 55% of the viewport.
        assert!(rubber(1e6, 400.0) < 400.0);
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
    fn snapping_and_paging_targets() {
        // Paging in a 400 point viewport over 1600 points of content: a
        // slow release past half a page moves on, a fling moves one page.
        let b = ScrollBehavior {
            paging: true,
            ..Default::default()
        };
        let r = rate(&b);
        let pt = Points;
        let t = b.snap_target(
            pt(250.0),
            pt(projected(250.0, 0.0, r)),
            0.0,
            pt(400.0),
            pt(1600.0),
        );
        assert_eq!(t, Some(pt(400.0)));
        let t = b.snap_target(
            pt(100.0),
            pt(projected(100.0, 1.0, r)),
            1.0,
            pt(400.0),
            pt(1600.0),
        );
        assert_eq!(t, Some(pt(400.0)));
        let t = b.snap_target(
            pt(300.0),
            pt(projected(300.0, -1.0, r)),
            -1.0,
            pt(400.0),
            pt(1600.0),
        );
        assert_eq!(t, Some(pt(0.0)));
        let b = ScrollBehavior {
            snap_interval: pt(100.0),
            ..Default::default()
        };
        let t = b.snap_target(pt(140.0), pt(140.0), 0.0, pt(400.0), pt(1600.0));
        assert_eq!(t, Some(pt(100.0)));
        assert_eq!(
            ScrollBehavior::default().snap_target(pt(140.0), pt(140.0), 0.0, pt(400.0), pt(1600.0)),
            None
        );
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
    fn snap_durations() {
        assert_eq!(snap_duration(0.2, 1.0), 0.0);
        assert_eq!(snap_duration(100.0, -1.0), ANIMATION);
        assert_eq!(snap_duration(300.0, 1.0), 600.0);
        assert_eq!(snap_duration(300.0, 3.0), 300.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert_eq!(ease_out(2.0), 1.0);
    }

    #[test]
    fn insets_clamp_and_report() {
        let view = scroll_view(300.0, 400.0, (300.0, 1000.0), false);
        {
            let mut state = view.state_mut();
            let s = state.scroll.as_mut().unwrap();
            s.behavior.content_inset = EdgeInsets {
                top: Points(50.0),
                bottom: Points(20.0),
                ..Default::default()
            };
        }
        assert_eq!(limits(&view.state()), (0.0, 670.0));
        // The offset 0 shows the top inset: UIKit's contentOffset is -50.
        assert_eq!(reported(&view.state()), (0.0, -50.0));
        view.state_mut().scroll.as_mut().unwrap().y = 900.0;
        assert_eq!(reported(&view.state()), (0.0, 620.0));
        // Children are drawn up by the offset, down by the inset.
        assert_eq!(view.state().scroll.as_ref().unwrap().origin(), (0.0, 850.0));
        // Refreshing lowers the top limit.
        view.state_mut().scroll.as_mut().unwrap().live.refreshing = true;
        assert_eq!(limits(&view.state()).0, -REFRESH_SPACE);
    }

    #[test]
    fn can_scroll_follows_edges_and_mirroring() {
        let view = scroll_view(300.0, 400.0, (300.0, 1000.0), false);
        view.state_mut().scroll.as_mut().unwrap().behavior.enabled = true;
        assert!(can_scroll(&view, 0.0, 10.0));
        assert!(!can_scroll(&view, 0.0, -10.0));
        view.state_mut().scroll.as_mut().unwrap().y = 600.0;
        assert!(!can_scroll(&view, 0.0, 10.0));
        view.state_mut().scroll.as_mut().unwrap().behavior.enabled = false;
        assert!(!can_scroll(&view, 0.0, -10.0));

        let row = scroll_view(300.0, 100.0, (1000.0, 100.0), true);
        {
            let mut state = row.state_mut();
            let s = state.scroll.as_mut().unwrap();
            s.behavior.enabled = true;
            s.mirrored = true;
        }
        // Right to left: content past the start is on the left.
        assert!(can_scroll(&row, -10.0, 0.0));
        assert!(!can_scroll(&row, 10.0, 0.0));
        // Vertical wheels move horizontal lists, unmirrored.
        assert!(can_scroll(&row, 0.0, 10.0));
        assert_eq!(key_scroll_view(&row, true, -1.0), None);
        assert_eq!(key_scroll_view(&row, true, 1.0), Some(row.clone()));
    }

    #[test]
    fn scroll_by_moves_gestures_along() {
        let drag = shifted(
            Phase::Drag {
                start: 10.0,
                moved: 5.0,
            },
            30.0,
        );
        assert_eq!(
            drag,
            Phase::Drag {
                start: 40.0,
                moved: 5.0
            }
        );
        let anim = Phase::Animate {
            from: 0.0,
            to: 100.0,
            start: 1.0,
            duration: 300.0,
            momentum: false,
            end: true,
        };
        let Phase::Animate { from, to, .. } = shifted(anim, -20.0) else {
            panic!()
        };
        assert_eq!((from, to), (-20.0, 80.0));
        assert_eq!(shifted(Phase::Idle, 5.0), Phase::Idle);
        assert_eq!(
            shifted(Phase::Thumb { start: 10.0 }, 30.0),
            Phase::Thumb { start: 40.0 }
        );
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

    fn key(vk: u16, modifiers: Modifiers) -> KeyEvent {
        KeyEvent {
            vk,
            down: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn scroll_keys() {
        let none = Modifiers::default();
        let shift = Modifiers {
            shift: true,
            ..none
        };
        let alt = Modifiers { alt: true, ..none };
        assert_eq!(
            key_move(&key(VK_DOWN, none)),
            Some((false, KeyMove::Points(KEY_LINE)))
        );
        assert_eq!(
            key_move(&key(VK_LEFT, none)),
            Some((true, KeyMove::Points(-KEY_LINE)))
        );
        assert_eq!(
            key_move(&key(VK_SPACE, shift)),
            Some((false, KeyMove::Pages(-1.0)))
        );
        assert_eq!(
            key_move(&key(VK_NEXT, none)),
            Some((false, KeyMove::Pages(1.0)))
        );
        assert_eq!(key_move(&key(VK_END, none)), Some((false, KeyMove::End)));
        assert_eq!(key_move(&key(VK_LEFT, alt)), None);
        assert_eq!(key_move(&key(b'A' as u16, none)), None);
        let ctrl = Modifiers { ctrl: true, ..none };
        assert_eq!(key_move(&key(VK_HOME, ctrl)), Some((false, KeyMove::Start)));
        assert_eq!(key_move(&key(VK_DOWN, ctrl)), None);
        let up = KeyEvent {
            down: false,
            ..key(VK_DOWN, none)
        };
        assert_eq!(key_move(&up), None);
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

    #[test]
    fn pick_finds_scrolled_children() {
        let root = NshView::new(0, ViewKind::View);
        root.state_mut().frame = nsh_core::Bounds {
            origin: nsh_core::Point {
                x: Points(0.0),
                y: Points(0.0),
            },
            size: nsh_core::Size {
                width: Points(300.0),
                height: Points(400.0),
            },
        };
        let scroll = scroll_view(300.0, 400.0, (300.0, 1000.0), false);
        scroll.insert_before(&root, None);
        let cell = NshView::new(4, ViewKind::View);
        cell.state_mut().frame = nsh_core::Bounds {
            origin: nsh_core::Point {
                x: Points(0.0),
                y: Points(500.0),
            },
            size: nsh_core::Size {
                width: Points(300.0),
                height: Points(50.0),
            },
        };
        cell.insert_before(&scroll, None);
        assert_eq!(pick(&root, 10.0, 10.0), Some(scroll.clone()));
        scroll.state_mut().scroll.as_mut().unwrap().y = 480.0;
        assert_eq!(pick(&root, 10.0, 30.0), Some(cell.clone()));
    }
}
