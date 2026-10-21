//! `Tooltip`: a hint about a trigger, in an overlay.

use std::time::Duration;

use nsh_core::{
    TouchEvent, TouchPhase,
    ui::{Style, prelude::*},
};

use crate::popup::{OpenChangeReason, OpenState};

/// How long a finger stays on the trigger before the tooltip shows (touch
/// screens; less than the platforms' long press, so it shows while the
/// finger is still down), and how long it stays after the finger lifts.
const HOLD: Duration = Duration::from_millis(400);
const LINGER: Duration = Duration::from_millis(1500);

/// The props of [`Tooltip`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct TooltipProps {
    /// Shown (controlled): the app owns the value and changes it in
    /// `onOpenChange`. Without it the tooltip keeps its own state.
    pub open: Prop<Option<bool>>,
    /// Shown at first, while `open` is not set.
    #[prop(default)]
    pub default_open: Prop<bool>,
    /// The side of the trigger it goes on (default `"top"`); the other side
    /// when only that one has room.
    #[prop(default = Placement::Top)]
    pub placement: Prop<Placement>,
    /// How it lines up with the trigger along that side (default
    /// `"center"`).
    #[prop(default)]
    pub align: Prop<Align>,
    /// The gap between the trigger and the tooltip.
    #[prop(default)]
    pub offset: Prop<Points>,
    /// The distance it keeps from the edges of the safe area.
    #[prop(default)]
    pub margin: Prop<Points>,
    /// Milliseconds the pointer rests on the trigger before it shows
    /// (desktops; default 500).
    #[prop(default = 500.0)]
    pub delay: Prop<f64>,
    /// No tooltip: it neither shows nor opens.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// The classes of the popup the content is in.
    pub popup_class: Prop<Option<SharedString>>,
    /// The inline style of the popup, over its classes.
    #[prop(default)]
    pub popup_style: Prop<Style>,
    /// What it says.
    pub content: Prop<Option<Slot>>,
    /// The trigger: what it is about.
    pub children: Prop<Children>,
}

/// The events of a [`Tooltip`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum TooltipEvent {
    /// It showed or hid, with why: set `open` to follow it.
    #[args(open, reason)]
    OpenChange(bool, OpenChangeReason),
}

/// A hint about its children, shown next to them on top of everything:
/// on the desktops while the pointer rests on them (after `delay`), on
/// touch screens while a finger is held on them and a moment after it
/// lifts. It never takes the pointer or presses, so the trigger works as
/// it does without it. It looks like nothing: style the popup with
/// `popupClass` / `popupStyle` and the `content`.
#[component]
pub fn Tooltip(props: TooltipProps, cx: Scope<TooltipEvent>) -> impl IntoElement {
    let open = OpenState::new(props.open, props.default_open);
    let change = move |next: bool, reason: OpenChangeReason| {
        if next && props.disabled.get_untracked() {
            return;
        }
        if open.request(next) {
            cx.emit(TooltipEvent::OpenChange(next, reason));
        }
    };
    let anchor = element_ref();
    let pending = stored(None::<Spawned<()>>);
    let wait = move |after: Duration, next: bool, reason: OpenChangeReason| {
        let task = set_timeout(after, move || change(next, reason));
        if let Some(old) = pending.replace(Some(task)) {
            old.cancel();
        }
    };
    let stop = move || {
        if let Some(old) = pending.replace(None) {
            old.cancel();
        }
    };
    let delay = move || Duration::from_secs_f64(props.delay.get_untracked().max(0.0) / 1000.0);
    view()
        .element_ref(&anchor)
        .on_hover_in(move || wait(delay(), true, OpenChangeReason::Hover))
        .on_hover_out(move || {
            stop();
            change(false, OpenChangeReason::Hover);
        })
        // Observed, never claimed: the trigger's own presses go on.
        .on_touch(move |e: &TouchEvent| match e.phase {
            TouchPhase::Start if !is_desktop() => wait(HOLD, true, OpenChangeReason::LongPress),
            // A press on the desktops hides it, as it starts something.
            TouchPhase::Start => {
                stop();
                change(false, OpenChangeReason::TriggerPress);
            }
            TouchPhase::Move => {}
            TouchPhase::End | TouchPhase::Cancel => {
                if open.get_untracked() && !is_desktop() {
                    wait(LINGER, false, OpenChangeReason::LongPress);
                } else if !is_desktop() {
                    stop();
                }
            }
        })
        .child(props.children)
        .child(when(
            move || open.get() && !props.disabled.get(),
            move || {
                overlay()
                    .anchor(&anchor)
                    .placement(props.placement)
                    .align(props.align)
                    .offset(props.offset)
                    .margin(props.margin)
                    .pointer_events(PointerEvents::None)
                    .app_class(props.popup_class)
                    .inline_style(props.popup_style)
                    .child(props.content.place(|c| c))
            },
        ))
}
