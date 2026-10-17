//! `SelectList`: a scrolling list of options, one of them chosen. The rows
//! are the component's own (a virtual list rendered in Rust), so thousands
//! of options cost what the visible ones do.

use nsh_core::ui::prelude::*;

/// A list of options ([`SelectListProps`]).
#[component]
pub fn SelectList(props: SelectListProps, cx: Scope<SelectListEvent>) -> impl IntoElement {
    // Our own choice, when the app does not control it.
    let own = signal(None::<u32>);
    let chosen = computed(move || props.value.get().or(own.get()));
    let options = props.options;
    view().flex_1().child(
        virtual_list(
            move || options.with(Vec::len),
            move |at| {
                let on = computed(move || chosen.get() == Some(at.index() as u32));
                view()
                    .flex_row()
                    .items_center()
                    .min_h(px(48.0))
                    .px_4()
                    .gap_3()
                    .bg(move || theme().background)
                    .active(|s| s.bg(move || theme().list_active))
                    .style_when(
                        move || !at.is_last(),
                        |v| v.border_b_1().border_color(move || theme().border),
                    )
                    .role(AccessibilityRole::Button)
                    .accessibility_state(move || AccessibilityState {
                        selected: on.get(),
                        ..Default::default()
                    })
                    .on_press(move |_: &()| {
                        let index = at.index() as u32;
                        if props.value.with_untracked(Option::is_none) {
                            own.set(Some(index));
                        }
                        cx.emit(SelectListEvent::Select(index));
                    })
                    .child(
                        text(move || {
                            options.with(|o| o.get(at.index()).cloned().unwrap_or_default())
                        })
                        .flex_1()
                        .text_base()
                        .text_color(move || theme().foreground),
                    )
                    .child(If::new(on, || {
                        text("✓").text_base().text_color(move || theme().primary)
                    }))
            },
        )
        .estimated_item_size(48.0)
        .keys(move || options.get())
        .flex_1(),
    )
}

/// A scrolling list of options with a check mark on the chosen one. With
/// `value` the app controls the choice (it updates `value` from
/// `onSelect`); without, the list keeps its own.
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SelectListProps {
    /// The options.
    pub options: Prop<Vec<SharedString>>,
    /// The index of the chosen option, when the app controls it.
    pub value: Prop<Option<u32>>,
}

/// The events of a [`SelectList`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum SelectListEvent {
    /// An option was chosen (its index).
    Select(u32),
}
