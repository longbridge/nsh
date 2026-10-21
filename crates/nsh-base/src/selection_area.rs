//! `SelectionArea`: texts that select together, across views.

use nsh_core::ui::prelude::*;

/// The props of [`SelectionArea`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SelectionAreaProps {
    /// The content.
    pub children: Prop<Children>,
}

/// A view whose texts share one selection, as a browser page's do: a
/// selection runs from a place in one text to a place in another, across
/// views and `TextView`s. On desktops it is made by dragging, a double click
/// (a word) or a triple click (a paragraph), and extended by Shift-clicks; on
/// touch screens by a long press, then the handles. Copy and Select All are
/// in the menu, Ctrl / Cmd+A and Ctrl / Cmd+C select all and copy, Escape
/// clears. Texts with `selectable={false}` stay out of it; an area inside
/// another is part of the outer one. It has no look of its own: the
/// highlight is the platform's, in each text's `selectionColor`.
#[component]
pub fn SelectionArea(props: SelectionAreaProps, _cx: Scope<NoEvent>) -> impl IntoElement {
    view().selection_area(true).child(props.children)
}
