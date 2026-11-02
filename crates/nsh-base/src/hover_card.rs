//! `HoverCard`: a preview of what its trigger points at (a profile, a link,
//! a quote), shown in an overlay while the pointer rests on the trigger, as
//! `gpui-base`'s `HoverCard`. The pointer and the delays are followed
//! natively: the trigger's and the card's `onMouseEnter` / `onMouseLeave`
//! are Rust listeners and the delays one timer of the components' executor,
//! so nothing runs in JS and nothing is sent to the platform until the card
//! opens or closes.

use std::time::Duration;

use nsh_core::{element::Style, prelude::*};

use crate::popup::{OpenChangeReason, OpenState, PopupPresence, StateLooks, lifecycle};

/// The props of [`HoverCard`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct HoverCardProps {
    /// Open (controlled): the app owns the value and changes it in
    /// `onOpenChange`. Without it the card keeps its own state.
    pub open: Prop<Option<bool>>,
    /// Open at first, while `open` is not set.
    #[prop(default)]
    pub default_open: Prop<bool>,
    /// Milliseconds the pointer rests on the trigger (or the keyboard
    /// focus stays on it) before the card opens (default 600).
    #[prop(default = 600.0)]
    pub open_delay: Prop<f64>,
    /// Milliseconds the pointer is away from both the trigger and the card
    /// before the card closes (default 300): time to move from one to the
    /// other.
    #[prop(default = 300.0)]
    pub close_delay: Prop<f64>,
    /// The side of the trigger it goes on (default `"bottom"`); the other
    /// side when only that one has room.
    #[prop(default)]
    pub placement: Prop<Placement>,
    /// How it lines up with the trigger along that side (default
    /// `"center"`).
    #[prop(default)]
    pub align: Prop<Align>,
    /// The gap between the trigger and the card.
    #[prop(default)]
    pub offset: Prop<Points>,
    /// The distance it keeps from the edges of the safe area.
    #[prop(default)]
    pub margin: Prop<Points>,
    /// The classes of the popup: the card its content is in, placed by the
    /// core.
    pub popup_class: Prop<Option<SharedString>>,
    /// The inline style of the popup, over its classes.
    #[prop(default)]
    pub popup_style: Prop<Style>,
    /// Classes the popup has in its first frame, which its `transition`
    /// classes animate from (Tailwind's `starting:`, CSS's
    /// `@starting-style`): its entrance. A state's classes apply over
    /// `popupClass` and `popupStyle`, as a state's styles do.
    pub popup_starting_class: Prop<Option<SharedString>>,
    /// The inline style of the popup in its first frame, over
    /// `popupStyle`.
    #[prop(default)]
    pub popup_starting_style: Prop<Style>,
    /// Classes the popup has while it goes: it stays, taking no pointer,
    /// until the transitions it has then ran (`onClose` after), so they
    /// run its exit.
    pub popup_ending_class: Prop<Option<SharedString>>,
    /// The inline style of the popup while it goes, over `popupStyle`.
    #[prop(default)]
    pub popup_ending_style: Prop<Style>,
    /// What it previews: resting the pointer on it opens the card (on
    /// touch screens, pressing it), and the card is placed against it. It
    /// keeps its own role, presses and look.
    pub trigger: Prop<Option<Slot>>,
    /// The content, made on each opening.
    #[prop(lazy)]
    pub children: Prop<Children>,
}

/// The events of a [`HoverCard`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum HoverCardEvent {
    /// It asks to open or close, with why (`"hover"`, `"focus"`,
    /// `"triggerPress"`, `"pressOutside"`, `"cancel"`): set `open` to
    /// follow it.
    #[args(open, reason)]
    OpenChange(bool, OpenChangeReason),
    /// It is shown.
    Open,
    /// It is gone (also when the app closed it), after its exit.
    Close,
    /// The popup was placed: the side of the trigger it went on (the other
    /// side of `placement` when only that one had room), its frame in the
    /// root view and the room on that side.
    #[args(placement, bounds, available)]
    Position(Placement, Bounds<Points>, Size<Points>),
}

/// Where the pointer is, for the close delay: away from both, the card
/// closes.
#[derive(Debug, Clone, Copy, Default)]
struct Over {
    trigger: bool,
    card: bool,
}

/// A card that previews what its trigger points at, shown on top of
/// everything next to the trigger, as `gpui-base`'s `HoverCard`. On the
/// desktops it opens once the pointer rested on the trigger for
/// `openDelay` (or the keyboard focus came to it), stays open while the
/// pointer moves between the trigger and the card, and closes `closeDelay`
/// after the pointer left both (or the focus left the trigger); Escape and
/// a press outside close it at once. On phones and tablets, which have no
/// pointer to rest, pressing the trigger opens and closes it and a press
/// outside closes it. The card takes the pointer and presses (links and
/// buttons in it work) and keeps no focus of its own. It looks like
/// nothing: style the popup with `popupClass` / `popupStyle` and the
/// content as you like; its entrance and exit are the app's `transition`
/// classes between the popup's look and its starting and ending looks
/// (`popupStartingClass`, `popupEndingClass`).
#[component]
pub fn HoverCard(props: HoverCardProps, cx: Scope<HoverCardEvent>) -> impl IntoElement {
    let open = OpenState::new(props.open, props.default_open);
    let change = move |next: bool, reason: OpenChangeReason| {
        open.request(next, || cx.emit(HoverCardEvent::OpenChange(next, reason)));
    };
    let over = stored(Over::default());
    // The one pending open or close: a new one replaces it.
    let pending = stored(None::<Spawned<()>>);
    let stop = move || {
        if let Some(task) = pending.replace(None) {
            task.cancel();
        }
    };
    // Opens (or closes) after its delay, unless something else happens
    // first; closing waits for the pointer to be away from both parts.
    let after_delay = move |next: bool, reason: OpenChangeReason| {
        stop();
        if open.get_untracked() == next {
            return;
        }
        let delay = if next {
            props.open_delay.get_untracked()
        } else {
            props.close_delay.get_untracked()
        };
        let fire = move || {
            pending.replace(None);
            if !next && over.with(|o| o.trigger || o.card) {
                return;
            }
            change(next, reason);
        };
        if delay <= 0.0 {
            fire();
            return;
        }
        let task = set_timeout(Duration::from_secs_f64(delay / 1000.0), fire);
        pending.replace(Some(task));
    };
    // At once: Escape, a press outside, a press on a touch screen.
    let now = move |next: bool, reason: OpenChangeReason| {
        stop();
        change(next, reason);
    };
    let presence = PopupPresence::new(
        move || open.get(),
        StateLooks {
            class: Some(props.popup_class),
            style: Some(props.popup_style),
            starting_class: props.popup_starting_class,
            starting_style: props.popup_starting_style,
            ending_class: props.popup_ending_class,
            ending_style: props.popup_ending_style,
        },
    );
    lifecycle(
        move || presence.mounted(),
        move || cx.emit(HoverCardEvent::Open),
        move || cx.emit(HoverCardEvent::Close),
    );
    let anchor = element_ref();
    let desktop = is_desktop();
    view()
        .child(props.trigger.place(move |trigger| {
            let trigger = trigger.element_ref(&anchor);
            if !desktop {
                // No pointer to rest: a press opens and closes it, and the
                // trigger's own press runs as well.
                return trigger
                    .on_press(move || now(!open.get_untracked(), OpenChangeReason::TriggerPress));
            }
            trigger
                .on_mouse_enter(move |_: &MouseEvent| {
                    over.with(|o| o.trigger = true);
                    after_delay(true, OpenChangeReason::Hover);
                })
                .on_mouse_leave(move |_: &MouseEvent| {
                    over.with(|o| o.trigger = false);
                    if !over.with(|o| o.card) {
                        after_delay(false, OpenChangeReason::Hover);
                    }
                })
                .on_focus(move || after_delay(true, OpenChangeReason::Focus))
                .on_blur(move || {
                    if !over.with(|o| o.trigger || o.card) {
                        after_delay(false, OpenChangeReason::Focus);
                    }
                })
        }))
        .child(when(
            move || presence.mounted(),
            move || {
                // The card leaves with the pointer on it: it hears no
                // leave.
                cleanup(move || over.with(|o| o.card = false));
                overlay()
                    .anchor_content(&anchor)
                    .placement(props.placement)
                    .align(props.align)
                    .offset(props.offset)
                    .margin(props.margin)
                    .element_ref(&presence.popup())
                    .app_class(move || presence.class())
                    .state_inline_style(move || presence.style())
                    .when(desktop, |card| {
                        card.on_mouse_enter(move |_: &MouseEvent| {
                            over.with(|o| o.card = true);
                            // Reached in time: it stays open.
                            if open.get_untracked() {
                                stop();
                            }
                        })
                        .on_mouse_leave(move |_: &MouseEvent| {
                            over.with(|o| o.card = false);
                            if !over.with(|o| o.trigger) {
                                after_delay(false, OpenChangeReason::Hover);
                            }
                        })
                    })
                    .on_position(move |p: &OverlayPosition| {
                        cx.emit(HoverCardEvent::Position(p.placement, p.bounds, p.available))
                    })
                    .on_press_outside(move |_: &Point<Points>| {
                        if open.get_untracked() {
                            now(false, OpenChangeReason::PressOutside);
                        }
                    })
                    .on_cancel(move || {
                        if open.get_untracked() {
                            now(false, OpenChangeReason::Cancel);
                        }
                    })
                    .child(props.children)
            },
        ))
}
