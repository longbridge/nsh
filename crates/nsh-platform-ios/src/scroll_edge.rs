//! `ios={{ scrollEdgeElement }}`: a view floating over an edge of a scroll
//! view (a bottom toolbar, a floating button) shapes that scroll view's
//! edge effect, as a bar does. UIKit learns of it through a
//! `UIScrollEdgeElementContainerInteraction` on the view, naming the scroll
//! view and the edge: the content scrolled under the view then fades out
//! around what the view shows. The interaction came with the edge effect
//! (iOS 26); earlier versions have neither, and nothing is done there.
//!
//! The named edge uses the soft style, the effect bars give on iPhone. Left
//! automatic, UIKit can pick the hard style (a near-opaque band with a
//! dividing line), as it does under a search field floating over a list:
//! that cuts the content the view is meant to let fade out.

use nsh_core::ScrollEdge;
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
};
use objc2_ui_kit::{UIScrollView, UIView};

/// `UIRectEdge` of `edge`.
fn rect_edge(edge: ScrollEdge) -> usize {
    match edge {
        ScrollEdge::Top => 1 << 0,
        // `UIRectEdgeBottom`; `ScrollEdge` may grow.
        _ => 1 << 2,
    }
}

/// Sets the style of `scroll`'s effect at `edge` (`UIRectEdge`):
/// `softStyle` or `automaticStyle`.
fn set_style(scroll: &UIScrollView, edge: usize, soft: bool) {
    let Some(class) = AnyClass::get(c"UIScrollEdgeEffectStyle") else {
        return;
    };
    // SAFETY: UIScrollView's `topEdgeEffect` / `bottomEdgeEffect` (never
    // nil from iOS 26, which `UIScrollEdgeEffectStyle` came with), the
    // style's class properties and the effect's `style` setter.
    unsafe {
        let effect: Option<Retained<AnyObject>> = if edge == rect_edge(ScrollEdge::Top) {
            msg_send![scroll, topEdgeEffect]
        } else {
            msg_send![scroll, bottomEdgeEffect]
        };
        let Some(effect) = effect else { return };
        let style: Retained<AnyObject> = if soft {
            msg_send![class, softStyle]
        } else {
            msg_send![class, automaticStyle]
        };
        let () = msg_send![&*effect, setStyle: &*style];
    }
}

/// Gives the scroll view `interaction` names back its automatic style.
fn release(interaction: &AnyObject) {
    // SAFETY: the interaction's `scrollView` (weak, nil once the scroll
    // view is gone) and `edge` getters.
    let (scroll, edge): (Option<Retained<UIScrollView>>, usize) = unsafe {
        (
            msg_send![interaction, scrollView],
            msg_send![interaction, edge],
        )
    };
    if let Some(scroll) = scroll {
        set_style(&scroll, edge, false);
    }
}

/// Gives `view` the interaction naming `to` (the scroll view and its
/// edge), or takes it away (`None`). `kept` holds the interaction the view
/// has. The named edge is soft while the view names it.
pub(crate) fn set(
    view: &UIView,
    kept: &mut Option<Retained<AnyObject>>,
    to: Option<(&UIScrollView, ScrollEdge)>,
) {
    let Some((scroll, edge)) = to else {
        if let Some(interaction) = kept.take() {
            release(&interaction);
            // SAFETY: UIView's `removeInteraction:` with the interaction
            // added below.
            let () = unsafe { msg_send![view, removeInteraction: &*interaction] };
        }
        return;
    };
    let interaction = match kept {
        Some(interaction) => {
            release(interaction);
            interaction.clone()
        }
        None => {
            let Some(class) = AnyClass::get(c"UIScrollEdgeElementContainerInteraction") else {
                return;
            };
            // SAFETY: the interaction's plain initializer; UIView's
            // `addInteraction:` with a UIInteraction, which the view keeps.
            let interaction: Retained<AnyObject> = unsafe { msg_send![class, new] };
            let () = unsafe { msg_send![view, addInteraction: &*interaction] };
            kept.insert(interaction).clone()
        }
    };
    // SAFETY: the interaction's `scrollView` (held weakly by UIKit) and
    // `edge` (a `UIRectEdge`) setters.
    unsafe {
        let () = msg_send![&*interaction, setScrollView: scroll];
        let () = msg_send![&*interaction, setEdge: rect_edge(edge)];
    }
    set_style(scroll, rect_edge(edge), true);
}
