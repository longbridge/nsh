//! `Card`: a surface with an optional header and footer around its content.

use nsh_core::ui::prelude::*;

/// A rounded surface with an optional header and footer around its content.
#[derive(Debug, Clone, PartialEq, Props)]
pub struct CardProps {
    /// `elevated` (default): a border and a soft shadow; `outlined`: a
    /// border only.
    #[prop(default)]
    pub variant: Prop<CardVariant>,
    /// Above the content: a title (its texts are semibold), an avatar row.
    pub header: Prop<Option<Slot>>,
    /// Below the content, separated by a divider: actions, at the end.
    pub footer: Prop<Option<Slot>>,
    /// The content.
    pub children: Prop<Children>,
}

/// How a [`Card`] stands out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum CardVariant {
    /// A border and a soft shadow.
    #[default]
    Elevated,
    /// A border only.
    Outlined,
}

/// A rounded surface ([`CardProps`]).
#[component]
pub fn Card(props: CardProps) -> impl IntoElement {
    let has_header = move || props.header.with(Option::is_some);
    view()
        .rounded(move || px(theme().radius_lg))
        .bg(move || theme().group_box)
        .text_color(move || theme().group_box_foreground)
        .border_1()
        .border_color(move || theme().border)
        .style_when(
            move || props.variant.get() == CardVariant::Elevated,
            |v| v.shadow_sm(),
        )
        .child(props.header.place(|header| {
            header
                .px_4()
                .pt_4()
                .text_size(move || px(theme().font_size + 1.0))
                .font_weight(FontWeight::SEMIBOLD)
        }))
        .child(
            view()
                .gap_2()
                .style_with(move |v| {
                    if has_header() {
                        v.px_4().pt_2().pb_4()
                    } else {
                        v.p_4()
                    }
                })
                .child(props.children),
        )
        .child(props.footer.place(|footer| {
            footer
                .flex_row()
                .justify_end()
                .gap_2()
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(move || theme().border)
        }))
}
