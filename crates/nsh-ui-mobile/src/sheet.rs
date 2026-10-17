//! `Sheet`: a panel that slides up from the bottom over a dimmed backdrop,
//! dragged down to close. The drag and the springs run natively: the panel
//! follows the finger with no engine work per frame.

use nsh_core::ui::prelude::*;

/// A panel that slides up from the bottom over a dimmed backdrop while
/// `open`; dragging it down, tapping the backdrop or the back button closes
/// it (`onClose`: set `open` to false).
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SheetProps {
    /// Shown.
    #[prop(default)]
    pub open: Prop<bool>,
    /// The content: made when the sheet opens and released when it has
    /// closed, so it starts fresh each time.
    #[prop(lazy)]
    pub children: Children,
}

/// The events of a [`Sheet`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum SheetEvent {
    /// The user closed it (it slid away).
    Close,
}

/// A drag further down than this, or faster, closes it.
const CLOSE_DISTANCE: f32 = 120.0;
const CLOSE_VELOCITY: f32 = 900.0;

/// How the panel springs back to where it rests.
fn settle(velocity: Option<f32>) -> Spring {
    Spring {
        stiffness: 280.0,
        damping: 30.0,
        velocity,
        ..Spring::default()
    }
}

/// A bottom sheet ([`SheetProps`]).
#[component]
pub fn Sheet(props: SheetProps, cx: Scope<SheetEvent>) -> impl IntoElement {
    // How far down the panel is from where it rests.
    let offset = native_value(0.0);
    // Closing: the panel slides away, then the app is told.
    let closing = stored(None::<Spawned<()>>);
    // Slides in from below the screen.
    let open = {
        let offset = offset.clone();
        move || {
            if let Some(task) = closing.replace(None) {
                task.cancel();
            }
            offset.set(window().height);
            offset.spring(0.0, settle(None));
        }
    };
    // Slides away, then tells the app.
    let close = {
        let offset = offset.clone();
        move || {
            if closing.with(|c| c.is_some()) {
                return;
            }
            let end = offset.timing(untracked(window).height, 220.0, Easing::EaseIn);
            closing.replace(Some(spawn(async move {
                // Ended or interrupted: closed either way.
                let _ = end.await;
                closing.replace(None);
                cx.emit(SheetEvent::Close);
            })));
        }
    };
    effect(move || {
        if props.open.get() {
            untracked(&open);
        }
    });
    let children = props.children;
    view().child(If::new(props.open, move || {
        let height = untracked(window).height;
        let (close_backdrop, close_drag, close_back) =
            (close.clone(), close.clone(), close.clone());
        let drag = offset.clone();
        modal()
            .transparent(true)
            .animation(ModalAnimation::None)
            .on_request_close(move |_: &()| close_back())
            .child(
                // The backdrop fades with the panel's position.
                view()
                    .absolute()
                    .inset_0()
                    .bg(move || theme().overlay)
                    .bind_opacity(&offset.interpolate(&[0.0, height], &[1.0, 0.0]))
                    .label("Close")
                    .on_press(move |_: &()| close_backdrop()),
            )
            .child(
                view()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .pt_2()
                    .pb(move || px(safe_area().bottom + 16.0))
                    .px_4()
                    .gap_3()
                    .rounded_t(move || px(theme().radius_lg.max(16.0)))
                    .bg(move || theme().background)
                    .shadow_lg()
                    .bind_translate_y(&offset)
                    .pan_values(None, Some(&offset))
                    .on_pan_end(move |e: &PanEvent| {
                        if e.translation_y > CLOSE_DISTANCE || e.velocity_y > CLOSE_VELOCITY {
                            close_drag();
                        } else {
                            drag.spring(0.0, settle(Some(e.velocity_y)));
                        }
                    })
                    // The grabber.
                    .child(
                        view()
                            .w(px(36.0))
                            .h(px(5.0))
                            .self_center()
                            .rounded_full()
                            .bg(move || theme().muted),
                    )
                    .child(children),
            )
    }))
}
