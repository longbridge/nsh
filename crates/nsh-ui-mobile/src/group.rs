//! `Group`: rows in a rounded box with dividers (settings-style).

use nsh_core::ui::prelude::*;

/// Rows in a rounded box, divided by hairlines: each JSX child is a row.
#[derive(Debug, Clone, PartialEq, Props)]
pub struct GroupProps {
    /// Above the box: a short caption.
    pub header: Prop<Option<Slot>>,
    /// Below the box: an explanation.
    pub footer: Prop<Option<Slot>>,
    /// The rows, one per child.
    pub children: Prop<Children>,
}

/// Rows in a rounded box ([`GroupProps`]).
#[component]
pub fn Group(props: GroupProps) -> impl IntoElement {
    view()
        .gap_2()
        .child(props.header.place(|header| {
            header
                .px_4()
                .text_sm()
                .text_color(move || theme().muted_foreground)
        }))
        .child(
            view()
                .rounded(move || px(theme().radius_lg))
                .bg(move || theme().group_box)
                .text_color(move || theme().group_box_foreground)
                .overflow_hidden()
                .children_each(props.children, move |row, at| {
                    view()
                        .px_4()
                        .py_3()
                        .min_h(px(44.0))
                        .justify_center()
                        .style_when(
                            move || !at.is_first(),
                            |s| s.border_t_1().border_color(move || theme().border),
                        )
                        .child(row)
                }),
        )
        .child(props.footer.place(|footer| {
            footer
                .px_4()
                .text_xs()
                .text_color(move || theme().muted_foreground)
        }))
}
