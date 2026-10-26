//! `Popover`: content anchored to a trigger, in an overlay.

use nsh_core::{element::Style, prelude::*};

use crate::popup::{OpenChangeReason, OpenState, lifecycle};

/// The props of [`Popover`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct PopoverProps {
    /// Open (controlled): the app owns the value and changes it in
    /// `onOpenChange`. Without it the popover keeps its own state.
    pub open: Prop<Option<bool>>,
    /// Open at first, while `open` is not set.
    #[prop(default)]
    pub default_open: Prop<bool>,
    /// The side of the trigger it goes on (default `"bottom"`); the other
    /// side when only that one has room.
    #[prop(default)]
    pub placement: Prop<Placement>,
    /// How it lines up with the trigger along that side (default
    /// `"center"`).
    #[prop(default)]
    pub align: Prop<Align>,
    /// The gap between the trigger and the popup.
    #[prop(default)]
    pub offset: Prop<Points>,
    /// The distance it keeps from the edges of the safe area.
    #[prop(default)]
    pub margin: Prop<Points>,
    /// Escape and the Android back button close it (default true); either
    /// way `onCancel` hears them.
    #[prop(default = true)]
    pub close_on_escape: Prop<bool>,
    /// A press outside it closes it (default true). On phones and tablets
    /// the press only closes it (a transparent backdrop takes it); on the
    /// desktops it goes on to what is under the pointer.
    #[prop(default = true)]
    pub close_on_press_outside: Prop<bool>,
    /// The classes of the popup: the box its content is in, placed by the
    /// core.
    pub popup_class: Prop<Option<SharedString>>,
    /// The inline style of the popup, over its classes.
    #[prop(default)]
    pub popup_style: Prop<Style>,
    /// What opens it: pressing it toggles the popover, and the popover is
    /// placed against it.
    pub trigger: Prop<Option<Slot>>,
    /// The content, made on each opening.
    #[prop(lazy)]
    pub children: Prop<Children>,
}

/// The events of a [`Popover`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum PopoverEvent {
    /// It opened or closed (by the trigger, a press outside, Escape or
    /// back), with why: set `open` to follow it.
    #[args(open, reason)]
    OpenChange(bool, OpenChangeReason),
    /// It is shown.
    Open,
    /// It is gone (also when the app closed it).
    Close,
    /// The user tried to close it (Escape or back, a press outside),
    /// whether or not `closeOnEscape` / `closeOnPressOutside` let it close.
    Cancel,
}

/// Content anchored to a trigger, shown on top of everything in its root
/// view (the surface, or the dialog it is in) whatever clips the trigger:
/// pressing the trigger toggles it, a press outside, Escape and the Android
/// back button close it. It goes on the `placement` side of the trigger, on
/// the other side when only that one has room, kept inside the safe area,
/// and follows the trigger as it scrolls. It looks like nothing: style the
/// popup with `popupClass` / `popupStyle` and the content as you like.
#[component]
pub fn Popover(props: PopoverProps, cx: Scope<PopoverEvent>) -> impl IntoElement {
    let open = OpenState::new(props.open, props.default_open);
    let change = move |next: bool, reason: OpenChangeReason| {
        if open.request(next) {
            cx.emit(PopoverEvent::OpenChange(next, reason));
        }
    };
    // A press outside or Escape: an attempt, which closes it when allowed.
    let cancel = move |reason: OpenChangeReason, allowed: bool| {
        cx.emit(PopoverEvent::Cancel);
        if allowed {
            change(false, reason);
        }
    };
    lifecycle(
        move || open.get(),
        move || cx.emit(PopoverEvent::Open),
        move || cx.emit(PopoverEvent::Close),
    );
    let anchor = element_ref();
    let desktop = is_desktop();
    view()
        .child(props.trigger.place(move |trigger| {
            trigger
                .element_ref(&anchor)
                .accessibility_state(move || AccessibilityState {
                    expanded: Some(open.get()),
                    ..AccessibilityState::default()
                })
                .on_press(move || {
                    change(!open.get_untracked(), OpenChangeReason::TriggerPress);
                })
        }))
        .child(when(
            move || open.get(),
            move || {
                // Touch screens: the press outside is the popover's.
                let backdrop = when(
                    move || !desktop && props.close_on_press_outside.get(),
                    move || {
                        overlay()
                            .inset_0()
                            .focusable(false)
                            .on_press(move || cancel(OpenChangeReason::PressOutside, true))
                    },
                );
                let popup = overlay()
                    .anchor_content(&anchor)
                    .placement(props.placement)
                    .align(props.align)
                    .offset(props.offset)
                    .margin(props.margin)
                    .app_class(props.popup_class)
                    .inline_style(props.popup_style)
                    .on_press_outside(move |_: &Point<Points>| {
                        // Touch screens: the backdrop took it.
                        if desktop {
                            let allowed = props.close_on_press_outside.get_untracked();
                            cancel(OpenChangeReason::PressOutside, allowed);
                        }
                    })
                    .on_cancel(move || {
                        cancel(
                            OpenChangeReason::Cancel,
                            props.close_on_escape.get_untracked(),
                        )
                    })
                    .child(props.children);
                vec![backdrop.into_element(), popup.into_element()]
            },
        ))
}
