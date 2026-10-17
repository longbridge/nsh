//! `Button`: a press target with a label.

use nsh_core::ui::prelude::*;

/// A press target with a label, the same on iOS and Android.
#[derive(Debug, Clone, PartialEq, Props)]
pub struct ButtonProps {
    /// The label.
    pub title: Prop<SharedString>,
    /// `primary` (default): filled; `secondary`: tinted; `danger`: a
    /// destructive action; `plain`: text only.
    #[prop(default)]
    pub variant: Prop<ButtonVariant>,
    /// No presses, no feedback, dimmed.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// For screen readers, when `title` is not enough.
    pub accessibility_label: Prop<Option<SharedString>>,
}

/// How a [`Button`] looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum ButtonVariant {
    /// Filled with the primary color.
    #[default]
    Primary,
    /// Filled with the secondary color.
    Secondary,
    /// Filled with the danger color.
    Danger,
    /// Text only; a background while pressed.
    Plain,
}

/// The events of a [`Button`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum ButtonEvent {
    /// The button was pressed.
    Press,
}

/// The background, the background while pressed and the label's color of
/// `variant`.
fn colors(variant: ButtonVariant) -> (AdaptiveColor, AdaptiveColor, AdaptiveColor) {
    let theme = theme();
    match variant {
        ButtonVariant::Primary => (
            theme.primary,
            theme.primary_active,
            theme.primary_foreground,
        ),
        ButtonVariant::Secondary => (
            theme.secondary,
            theme.secondary_active,
            theme.secondary_foreground,
        ),
        ButtonVariant::Danger => (theme.danger, theme.danger_active, theme.danger_foreground),
        ButtonVariant::Plain => (AdaptiveColor::TRANSPARENT, theme.accent, theme.foreground),
    }
}

/// A press target with a label ([`ButtonProps`]).
#[component]
pub fn Button(props: ButtonProps, cx: Scope<ButtonEvent>) -> impl IntoElement {
    let colors = computed(move || colors(props.variant.get()));
    view()
        .flex_row()
        .items_center()
        .justify_center()
        .px_4()
        .py_3()
        .rounded(move || px(theme().radius))
        .bg(move || colors.get().0)
        .active(|s| s.bg(move || colors.get().1))
        .style_when(props.disabled, |v| v.opacity(0.5))
        .disabled(props.disabled)
        .role(AccessibilityRole::Button)
        .label(move || {
            props
                .accessibility_label
                .get()
                .unwrap_or_else(|| props.title.get())
        })
        .on_press(move |_: &()| cx.emit(ButtonEvent::Press))
        .child(
            text(props.title)
                .text_size(move || px(theme().font_size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || colors.get().2),
        )
}
