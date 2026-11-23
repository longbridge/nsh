//! `Toast`: the unstyled semantic root of a toast.

use std::{
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use nsh_core::{
    TouchEvent, TouchPhase,
    prelude::*,
    time::{Instant, now},
};

use super::ToastTransitionStatus;
use crate::popup::important;

/// How far a pointer moves toward the swipe before the toast follows it
/// (its buttons' presses stay presses below that).
const SWIPE_SLOP: f32 = 10.0;
/// A release this fast toward the swipe (points per second) dismisses it
/// short of the threshold: a flick.
const FLICK: f32 = 500.0;
/// How long a toast takes to follow a release (away, or back).
const SETTLE_MS: f32 = 200.0;
/// How long a toast waits for its first display frame before it counts as
/// past it without one (a surface not on screen).
const FIRST_FRAME_MS: u64 = 50;
/// How long a toast taken down while it enters (`starting`) stays the one
/// a toast mounted with its label is: the app rebuilt it under another
/// parent, in the same update.
const MOVED: Duration = Duration::from_millis(50);

/// The direction a toast is swiped away in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum SwipeDirection {
    /// Toward the right edge.
    #[default]
    Right,
    /// Toward the left edge.
    Left,
    /// Toward the top.
    Up,
    /// Toward the bottom.
    Down,
}

/// The props of [`Toast`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct ToastProps {
    /// Its lifecycle phase (a `ToastManager`'s), which the state classes
    /// follow (default `"starting"`).
    #[prop(default)]
    pub transition_status: Prop<ToastTransitionStatus>,
    /// Classes it has in its first frame while `"starting"`, which its
    /// `transition` classes animate from (Tailwind's `starting:`, CSS's
    /// `@starting-style`): its enter motion. The state classes apply over its
    /// `class` and `style`, as a state's styles do.
    pub starting_class: Prop<Option<SharedString>>,
    /// Classes it has while `"ending"`: its exit motion.
    pub ending_class: Prop<Option<SharedString>>,
    /// Classes it has while a pointer swipes it (`transition-none`: it
    /// follows at once).
    pub swiping_class: Prop<Option<SharedString>>,
    /// The direction a finger or the mouse swipes it away in (default
    /// `"right"`); `null`: it is not swiped.
    #[prop(default = Some(SwipeDirection::Right))]
    pub swipe_direction: Prop<Option<SwipeDirection>>,
    /// How far it is swiped before a release asks to dismiss it (default
    /// 50).
    #[prop(default = Points(50.0))]
    pub swipe_threshold: Prop<Points>,
    /// What the screen reader reads as it shows (its title and
    /// description).
    pub accessibility_label: Prop<Option<SharedString>>,
    /// Its content.
    pub children: Prop<Children>,
}

/// The events of a [`Toast`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum ToastEvent {
    /// The user swiped it away: dismiss it (a `ToastManager`'s `dismiss`).
    /// It stays where the swipe took it.
    DismissRequest,
}

/// The surface's toasts that are entering (`starting`), by label.
#[derive(Clone, Default)]
struct Entering(Vec<EnteringToast>);

#[derive(Clone)]
struct EnteringToast {
    id: u64,
    label: SharedString,
    /// When it was taken down; `None`: it is mounted.
    left: Option<Instant>,
}

impl EnteringToast {
    /// Whether it is still one a toast mounted at `at` can be.
    fn current(&self, at: Instant) -> bool {
        self.left
            .is_none_or(|left| at.saturating_duration_since(left) < MOVED)
    }
}

/// Reads `label` as a toast shows, once per toast however often the app
/// rebuilds it (moves it to another parent, as a stack drawn in the
/// topmost modal is when a modal opens or closes). A toast mounted past
/// its entrance (`present`, `ending`) was read before it moved; one
/// entering while a toast with its label enters, or was taken down
/// entering a moment ago, is that toast.
fn announce_once(label: Option<SharedString>, status: Prop<ToastTransitionStatus>) {
    let Some(label) = label else {
        return;
    };
    if status.get_untracked() != ToastTransitionStatus::Starting {
        return;
    }
    // Unique among every surface's toasts, which is all it needs.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let entering = global::<Entering>();
    let at = now();
    let moved = entering.with_untracked(|e| e.0.iter().any(|t| t.label == label && t.current(at)));
    if !moved {
        announce_for_accessibility(label.to_string());
    }
    entering.update(|e| {
        e.0.retain(|t| t.current(at));
        e.0.push(EnteringToast {
            id,
            label,
            left: None,
        });
    });
    // Entered (or ending): a toast with its label mounted later is another.
    effect(move || {
        if status.get() != ToastTransitionStatus::Starting
            && entering.with_untracked(|e| e.0.iter().any(|t| t.id == id))
        {
            entering.update(|e| e.0.retain(|t| t.id != id));
        }
    });
    // Taken down while entering: the toast the app rebuilds elsewhere.
    cleanup(move || {
        if entering.with_untracked(|e| e.0.iter().any(|t| t.id == id)) {
            let left = now();
            entering.update(|e| {
                if let Some(t) = e.0.iter_mut().find(|t| t.id == id) {
                    t.left = Some(left);
                }
            });
        }
    });
}

/// An unstyled semantic toast root (`role` alert), as `gpui-base`'s
/// `Toast`: the app owns its presentation and motion, by its
/// `transitionStatus` and the state classes (`startingClass`,
/// `endingClass`, `swipingClass`) its `class` and `style` add to. A pointer
/// dragged toward `swipeDirection` moves it, and a release past
/// `swipeThreshold` or a flick asks to dismiss it (`onDismissRequest`);
/// short of it, it goes back. The screen reader reads its
/// `accessibilityLabel` as it shows, once: rebuilt under another parent
/// (moved), it is not read again. Placed in a `ToastStack`, which lays
/// toasts out. In a terminal it shows and goes in one frame: its state
/// classes do not apply.
#[component]
pub fn Toast(props: ToastProps, cx: Scope<ToastEvent>) -> impl IntoElement {
    let status = props.transition_status;
    announce_once(props.accessibility_label.get_untracked(), status);

    // Past its first frame: the next display frame, or a frame's time where
    // none comes, whichever is first.
    let entered = signal(reduce_motion());
    if !entered.get_untracked() {
        let enter = Rc::new(move || {
            if !entered.get_untracked() {
                entered.set(true);
            }
        });
        let first = enter.clone();
        request_animation_frame(move |_| first());
        set_timeout(Duration::from_millis(FIRST_FRAME_MS), move || enter());
    }

    // The swipe: it follows the pointer toward its direction.
    let x = animated_value(0.0);
    let y = animated_value(0.0);
    let swiping = signal(false);
    let size = stored(Size::<Points>::default());
    // Where the pointer went down: it follows from there, not from where
    // it claimed the touches.
    let down = stored(None::<Point<Points>>);
    let direction = move || props.swipe_direction.get_untracked();
    let along = move |d: Point<Points>| -> f32 {
        match direction() {
            Some(SwipeDirection::Right) => d.x.0,
            Some(SwipeDirection::Left) => -d.x.0,
            Some(SwipeDirection::Down) => d.y.0,
            Some(SwipeDirection::Up) => -d.y.0,
            None => 0.0,
        }
    };
    let horizontal = move || {
        matches!(
            direction(),
            Some(SwipeDirection::Right | SwipeDirection::Left)
        )
    };
    let to = move |distance: f32| -> (f32, f32) {
        match direction() {
            Some(SwipeDirection::Right) => (distance, 0.0),
            Some(SwipeDirection::Left) => (-distance, 0.0),
            Some(SwipeDirection::Down) => (0.0, distance),
            Some(SwipeDirection::Up) => (0.0, -distance),
            None => (0.0, 0.0),
        }
    };
    let moved = move |e: &ResponderEvent| {
        let from = down.with(|d| d.unwrap_or(e.gesture.origin));
        point(e.page.x - from.x, e.page.y - from.y)
    };
    let back = move || {
        x.timing(0.0, SETTLE_MS, Easing::EaseOut);
        y.timing(0.0, SETTLE_MS, Easing::EaseOut);
    };
    let release = move |e: &ResponderEvent| {
        swiping.set(false);
        let distance = along(moved(e));
        let speed = along(point(px(e.gesture.vx), px(e.gesture.vy)));
        if distance >= props.swipe_threshold.get_untracked().0 || speed >= FLICK {
            let far = size.with(|s| if horizontal() { s.width.0 } else { s.height.0 });
            let (tx, ty) = to(far.max(distance) + 16.0);
            x.timing(tx, SETTLE_MS, Easing::EaseOut);
            y.timing(ty, SETTLE_MS, Easing::EaseOut);
            cx.emit(ToastEvent::DismissRequest);
        } else {
            back();
        }
    };

    // The state's classes win over the app's `class` and `style`, as a
    // state's styles do (`activeStyle`) and as CSS's state selectors do by
    // their specificity: applied as `!` utilities.
    // A terminal's toast comes and goes in one frame: no state's motion.
    let terminal = crate::terminal::is_terminal();
    let class = move || {
        if terminal {
            return None;
        }
        let state = if status.get() == ToastTransitionStatus::Ending {
            props.ending_class.get()
        } else if swiping.get() {
            props.swiping_class.get()
        } else if status.get() == ToastTransitionStatus::Starting && !entered.get() {
            props.starting_class.get()
        } else {
            None
        };
        state.map(|c| important(&c))
    };

    view()
        .role(AccessibilityRole::Alert)
        .optional_label(props.accessibility_label)
        .app_class(class)
        .bind_translate_x(&x)
        .bind_translate_y(&y)
        .on_layout(move |frame: &Bounds<Points>| {
            size.replace(frame.size);
        })
        .on_touch(move |e: &TouchEvent| {
            if e.phase == TouchPhase::Start {
                down.replace(e.points.first().map(|p| p.page));
            }
        })
        .claim_touches(TouchClaim::on_move().ask(move |request: &ClaimRequest| {
            if direction().is_none() || status.get() == ToastTransitionStatus::Ending {
                return;
            }
            let d = request.event.gesture.delta;
            let (main, cross) = if horizontal() {
                (d.x.0, d.y.0)
            } else {
                (d.y.0, d.x.0)
            };
            if along(d) >= SWIPE_SLOP && main.abs() > cross.abs() {
                request.claim();
            }
        }))
        .keep_touches(true)
        .on_responder_grant(move |_: &ResponderEvent| swiping.set(true))
        .on_responder_move(move |e: &ResponderEvent| {
            // Toward the direction only.
            let (dx, dy) = to(along(moved(e)).max(0.0));
            x.set(dx);
            y.set(dy);
        })
        .on_responder_release(release)
        .on_responder_terminate(move |_: &ResponderEvent| {
            swiping.set(false);
            back();
        })
        .child(props.children)
}
