//! `TextField`: a labeled text input with a clear button.

use nsh_core::ui::prelude::*;

/// A labeled text input with a clear button. With `value` the app controls
/// the text (it updates `value` from `onChange`); without, the field keeps
/// its own, starting at `defaultValue`.
#[derive(Debug, Clone, PartialEq, Props)]
pub struct TextFieldProps {
    /// Above the input.
    pub label: Prop<Option<SharedString>>,
    /// Shown while it is empty.
    pub placeholder: Prop<Option<SharedString>>,
    /// The text, when the app controls it.
    pub value: Prop<Option<SharedString>>,
    /// The first text, when the app does not control it.
    #[prop(default)]
    pub default_value: Prop<SharedString>,
    /// Hides what is typed (passwords).
    #[prop(default)]
    pub secure: Prop<bool>,
    /// Shows a clear button while there is text (default yes).
    #[prop(default = true)]
    pub clearable: Prop<bool>,
    /// Not editable, dimmed.
    #[prop(default)]
    pub disabled: Prop<bool>,
}

/// The events of a [`TextField`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum TextFieldEvent {
    /// The text changed (the new text).
    Change(String),
    /// The return key was pressed (the text).
    Submit(String),
}

/// A labeled text input ([`TextFieldProps`]).
#[component]
pub fn TextField(props: TextFieldProps, cx: Scope<TextFieldEvent>) -> impl IntoElement {
    // Our own text, when the app does not control it.
    let own = signal(props.default_value.get_untracked().as_str().to_owned());
    let focused = signal(false);
    let field = element_ref();
    let shown = computed(move || match props.value.get() {
        Some(value) => value.as_str().to_owned(),
        None => own.get(),
    });
    let change = move |text: String| {
        if props.value.with_untracked(Option::is_none) {
            own.set(text.clone());
        }
        cx.emit(TextFieldEvent::Change(text));
    };
    view()
        .gap_1()
        .style_when(props.disabled, |v| v.opacity(0.5))
        .child(dynamic(move || {
            props.label.get().map(|label| {
                text(label)
                    .text_sm()
                    .text_color(move || theme().muted_foreground)
            })
        }))
        .child(
            view()
                .flex_row()
                .items_center()
                .min_h(px(44.0))
                .px_3()
                .gap_2()
                .rounded(move || px(theme().radius))
                .bg(move || theme().background)
                .border_1()
                .border_color(move || {
                    if focused.get() {
                        theme().ring
                    } else {
                        theme().input
                    }
                })
                .child(
                    input()
                        .element_ref(&field)
                        .flex_1()
                        .py_2()
                        .text_base()
                        .text_color(move || theme().foreground)
                        .value(move || shown.get())
                        .placeholder(move || {
                            props
                                .placeholder
                                .get()
                                .map_or_else(String::new, |p| p.as_str().to_owned())
                        })
                        .placeholder_color(move || color(theme().muted_foreground))
                        .selection_color(move || color(theme().primary))
                        .secure(props.secure)
                        .editable(move || !props.disabled.get())
                        .on_change_text(move |text: &str| change(text.to_owned()))
                        .on_submit_editing(move |text: &str| {
                            cx.emit(TextFieldEvent::Submit(text.to_owned()));
                        })
                        .on_focus(move |_: &()| focused.set(true))
                        .on_blur(move |_: &()| focused.set(false)),
                )
                .child(If::new(
                    move || {
                        props.clearable.get()
                            && !shown.with(String::is_empty)
                            && !props.disabled.get()
                    },
                    move || {
                        let field = field.clone();
                        view()
                            .size_6()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(move || theme().muted)
                            .active(|s| s.bg(move || theme().secondary_active))
                            .role(AccessibilityRole::Button)
                            .label("Clear")
                            .hit_slop(EdgeInsets::all(8.0))
                            .on_press(move |_: &()| {
                                change(String::new());
                                field.focus();
                            })
                            .child(
                                text("×")
                                    .text_sm()
                                    .text_color(move || theme().muted_foreground),
                            )
                    },
                )),
        )
}
