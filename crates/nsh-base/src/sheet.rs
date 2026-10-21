//! `Sheet`: a panel at an edge of the screen, over a backdrop, in
//! overlays.

use nsh_core::ui::{Style, prelude::*};

use crate::popup::{OpenChangeReason, OpenState, SheetMotion, lifecycle};

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
    /// It came in: it is all there.
    Open,
    /// It went: it is all gone (also when the app closed it).
    Close,
    /// The user tried to close it (Escape or back, a press on the
    /// backdrop), whether or not `closeOnEscape` / `closeOnBackdropPress`
    /// let it close.
    Cancel,
}

/// A panel at an edge of the screen (`side`) over a backdrop that covers
/// the rest and takes its presses, on top of everything in its root view;
/// the same on every device. It slides in from its edge as its backdrop
/// fades in, and back out as it closes (at once when the user asked for
/// less motion): the backdrop's opacity and the panel's translation are
/// the sheet's, so color the backdrop with its background. A press on the backdrop, Escape and the
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
    let motion = SheetMotion::new(move || open.get(), move || props.side.get());
    lifecycle(
        move || motion.there(),
        move || cx.emit(SheetEvent::Open),
        move || cx.emit(SheetEvent::Close),
    );
    when(motion.shown, move || {
        let backdrop = overlay()
            .inset_0()
            .bind_opacity(&motion.fade)
            .focusable(false)
            .app_class(props.backdrop_class)
            .inline_style(props.backdrop_style)
            .on_press(move || {
                cx.emit(SheetEvent::Cancel);
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
            .bind_translate_x(&motion.x)
            .bind_translate_y(&motion.y)
            .on_layout(move |frame: &Bounds<Points>| motion.laid_out(frame.size))
            .on_cancel(move || {
                cx.emit(SheetEvent::Cancel);
                if props.close_on_escape.get_untracked() {
                    change(false, OpenChangeReason::Cancel);
                }
            })
            .child(props.children);
        vec![backdrop.into_element(), panel.into_element()]
    })
}
