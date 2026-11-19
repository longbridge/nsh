//! `ios={{ scrollEdgeElement }}`: a view floating over an edge of a scroll
//! view (a bottom toolbar, a floating button) shapes that scroll view's
//! edge effect, as a bar does. UIKit learns of it through a
//! `UIScrollEdgeElementContainerInteraction` on the view, naming the scroll
//! view and the edge: the content scrolled under the view then fades out
//! around what the view shows. The interaction came with the edge effect
//! (iOS 26); earlier versions have neither, and nothing is done there.

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

/// Gives `view` the interaction naming `to` (the scroll view and its
/// edge), or takes it away (`None`). `kept` holds the interaction the view
/// has.
pub(crate) fn set(
    view: &UIView,
    kept: &mut Option<Retained<AnyObject>>,
    to: Option<(&UIScrollView, ScrollEdge)>,
) {
    let Some((scroll, edge)) = to else {
        if let Some(interaction) = kept.take() {
            // SAFETY: UIView's `removeInteraction:` with the interaction
            // added below.
            let () = unsafe { msg_send![view, removeInteraction: &*interaction] };
        }
        return;
    };
    let interaction = match kept {
        Some(interaction) => interaction.clone(),
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
}
