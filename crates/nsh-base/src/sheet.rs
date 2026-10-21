//! `Sheet`: a panel at an edge of the screen, over a backdrop, in
//! overlays.

use nsh_core::ui::{Style, prelude::*};

use crate::popup::{OpenChangeReason, OpenState};

/// The props of [`Sheet`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SheetProps {
    /// Open (controlled): the app owns the value and changes it in
    /// `onOpenChange`. Without it the sheet keeps its own state.
    pub open: Prop<Option<bool>>,
    /// Open at first, while `open` is not set.
    #[prop(default)]
    pub default_open: Prop<bool>,
    /// The edge it is on (default `"bottom"`): as wide as the screen at the
    /// top and bottom, as tall at the left and right.
    #[prop(default)]
    pub side: Prop<Placement>,
    /// Escape and the Android back button close it (default true); either
    /// way `onCancel` hears them.
    #[prop(default = true)]
    pub close_on_escape: Prop<bool>,
    /// A press on the backdrop closes it (default true).
    #[prop(default = true)]
    pub close_on_backdrop_press: Prop<bool>,
    /// The classes of the panel.
    pub popup_class: Prop<Option<SharedString>>,
    /// The inline style of the panel, over its classes.
    #[prop(default)]
    pub popup_style: Prop<Style>,
    /// The classes of the backdrop behind the panel, which covers the
    /// screen and takes its presses.
    pub backdrop_class: Prop<Option<SharedString>>,
    /// The inline style of the backdrop, over its classes.
    #[prop(default)]
    pub backdrop_style: Prop<Style>,
    /// The content, made on each opening.
    #[prop(lazy)]
    pub children: Prop<Children>,
}

/// The events of a [`Sheet`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum SheetEvent {
    /// It closed (a press on the backdrop, Escape or back), with why: set
    /// `open` to follow it.
    #[args(open, reason)]
    OpenChange(bool, OpenChangeReason),
    /// Escape or the Android back button while it is open, whether or not
    /// `closeOnEscape` closed it.
    Cancel,
}

/// A panel at an edge of the screen (`side`) over a backdrop that covers
/// the rest and takes its presses, on top of everything in its root view;
/// the same on every device. A press on the backdrop, Escape and the
/// Android back button close it. It looks like nothing: style the panel
/// with `popupClass` / `popupStyle` and the backdrop with `backdropClass`
/// / `backdropStyle` (a translucent color).
#[component]
pub fn Sheet(props: SheetProps, cx: Scope<SheetEvent>) -> impl IntoElement {
    let open = OpenState::new(props.open, props.default_open);
    let change = move |next: bool, reason: OpenChangeReason| {
        if open.request(next) {
            cx.emit(SheetEvent::OpenChange(next, reason));
        }
    };
    If::new(
        move || open.get(),
        move || {
            let backdrop = overlay()
                .inset_0()
                .focusable(false)
                .app_class(props.backdrop_class)
                .inline_style(props.backdrop_style)
                .on_press(move || {
                    if props.close_on_backdrop_press.get_untracked() {
                        change(false, OpenChangeReason::BackdropPress);
                    }
                });
            let panel = overlay()
                .style_with(move |s| match props.side.get() {
                    Placement::Bottom => s.left_0().right_0().bottom_0(),
                    Placement::Top => s.left_0().right_0().top_0(),
                    Placement::Left => s.top_0().bottom_0().left_0(),
                    Placement::Right => s.top_0().bottom_0().right_0(),
                })
                .app_class(props.popup_class)
                .inline_style(props.popup_style)
                .on_cancel(move || {
                    cx.emit(SheetEvent::Cancel);
                    if props.close_on_escape.get_untracked() {
                        change(false, OpenChangeReason::Cancel);
                    }
                })
                .child(props.children);
            vec![backdrop.into_element(), panel.into_element()]
        },
    )
}
