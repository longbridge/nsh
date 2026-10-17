//! `ListView`: a scrolling list of the app's rows, recycled natively.

use nsh_core::ui::prelude::*;

/// A scrolling list of the app's rows, padded and divided: only the rows
/// near the screen exist, cells are reused. The app passes the data and
/// renders a row (`each` and children, like `List`); the look is the
/// list's.
#[derive(Debug, Clone, PartialEq, Props)]
pub struct ListViewProps {
    /// The rows.
    pub items: Items,
    /// `plain` (default): full-width rows; `inset`: rows in a rounded box
    /// with a margin, like grouped settings.
    #[prop(default)]
    pub variant: Prop<ListViewVariant>,
    /// Hairlines between rows.
    #[prop(default = true)]
    pub dividers: Prop<bool>,
}

/// How a [`ListView`] shows its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum ListViewVariant {
    /// Full-width rows on the background.
    #[default]
    Plain,
    /// Rows in a rounded box with a margin around it.
    Inset,
}

/// A scrolling list of rows ([`ListViewProps`]).
#[component]
pub fn ListView(props: ListViewProps, _cx: Scope<NoEvent>) -> impl IntoElement {
    let inset = computed(move || props.variant.get() == ListViewVariant::Inset);
    let dividers = props.dividers;
    // Inset: rows on the background color over a muted page, as grouped
    // settings look.
    view()
        .flex_1()
        .bg(move || {
            if inset.get() {
                theme().muted
            } else {
                theme().background
            }
        })
        .text_color(move || theme().foreground)
        .list(
            props.items,
            move |list| {
                let margin = move || if inset.get() { 16.0 } else { 0.0 };
                list.flex_1()
                    .estimated_item_size(48.0)
                    .content_padding(margin)
                    // The last rows scroll clear of the home indicator.
                    .content_padding_bottom(move || safe_area().bottom + margin())
            },
            move |row, at| {
                let radius = move || px(theme().radius_lg);
                view()
                    .px_4()
                    .py_3()
                    .min_h(px(48.0))
                    .justify_center()
                    .style_when(
                        move || dividers.get() && !at.is_first(),
                        |v| v.border_t_1().border_color(move || theme().border),
                    )
                    .style_when(inset, |v| v.bg(move || theme().background))
                    .style_when(
                        move || inset.get() && at.is_first(),
                        |v| v.rounded_t(radius),
                    )
                    .style_when(move || inset.get() && at.is_last(), |v| v.rounded_b(radius))
                    .child(row)
            },
        )
}
