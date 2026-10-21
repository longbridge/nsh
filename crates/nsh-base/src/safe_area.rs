//! `SafeAreaView`: content kept clear of system UI and headers.

use nsh_core::ui::prelude::*;

/// The props of [`SafeAreaView`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SafeAreaViewProps {
    /// The edges to pad (all by default).
    pub edges: Prop<Option<Vec<Edge>>>,
    /// The content.
    pub children: Prop<Children>,
}

/// An edge of a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub enum Edge {
    /// The top edge.
    Top,
    /// The right edge.
    Right,
    /// The bottom edge.
    Bottom,
    /// The left edge.
    Left,
}

/// A view padded by the insets of system UI or headers where it renders
/// (the surface's safe area, a modal's, or a screen's, below its header),
/// on top of its own padding.
#[component]
pub fn SafeAreaView(props: SafeAreaViewProps, cx: Scope<NoEvent>) -> impl IntoElement {
    view()
        .inset_padding(move || {
            let insets = cx.insets();
            let pick = |edge: Edge, v: Points| {
                let on = props
                    .edges
                    .with(|e| e.as_ref().is_none_or(|e| e.contains(&edge)));
                if on { v } else { Points::ZERO }
            };
            EdgeInsets {
                top: pick(Edge::Top, insets.top),
                right: pick(Edge::Right, insets.right),
                bottom: pick(Edge::Bottom, insets.bottom),
                left: pick(Edge::Left, insets.left),
            }
        })
        .child(props.children)
}
