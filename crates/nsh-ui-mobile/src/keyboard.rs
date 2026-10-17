//! The software keyboard: `Keyboard` and `KeyboardAvoidingView`.

use nsh_core::ui::{
    LayoutAnimation, LayoutCurve, LayoutProperty, LayoutTransition, Style, prelude::*,
};

/// The software keyboard.
#[derive(Debug)]
pub struct Keyboard;

/// The software keyboard.
#[module]
impl Keyboard {
    /// Unfocuses the focused input, hiding the keyboard.
    pub fn dismiss(cx: &mut ModuleCx) {
        cx.dismiss_keyboard();
    }
}

/// A view that keeps its content above the software keyboard, animated
/// with the keyboard. For content outside scroll views, such as a message
/// bar at the bottom of a screen (inputs inside scroll views are kept
/// visible natively). It assumes it reaches the bottom of the surface, as
/// a screen's content does. Where the window shrinks for the keyboard
/// instead (an Android host that resizes), nothing moves.
#[derive(Debug, Clone, PartialEq, Props)]
pub struct KeyboardAvoidingViewProps {
    /// Avoid the keyboard (default true).
    #[prop(default = true)]
    pub enabled: Prop<bool>,
    /// `padding` (default) pads the bottom by the keyboard's height;
    /// `height` shrinks the view instead (a bottom margin, so its
    /// background ends at the keyboard); `position` moves an inner content
    /// view (`contentContainerStyle`) up.
    #[prop(default)]
    pub behavior: Prop<AvoidBehavior>,
    /// Added to the keyboard's height while it shows (a tab bar below the
    /// view, say).
    #[prop(default)]
    pub keyboard_vertical_offset: Prop<f32>,
    /// The inner view's style with `behavior="position"`.
    #[prop(default)]
    pub content_container_style: Prop<Style>,
    /// The content.
    pub children: Prop<Children>,
}

/// How a [`KeyboardAvoidingView`] makes room.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum AvoidBehavior {
    /// Pads its bottom.
    #[default]
    Padding,
    /// Shrinks.
    Height,
    /// Moves its content up.
    Position,
}

/// A view keeping its content above the keyboard ([`KeyboardAvoidingViewProps`]).
#[component]
pub fn KeyboardAvoidingView(props: KeyboardAvoidingViewProps) -> impl IntoElement {
    // Moves with the keyboard: the frame change animates natively.
    let shown = std::cell::Cell::new(None);
    effect(move || {
        let height = keyboard_height();
        let previous = shown.replace(Some(height));
        let duration = untracked(keyboard_duration);
        if previous.is_some_and(|p| p != height)
            && untracked(|| props.enabled.get())
            && duration > 0.0
        {
            animate_layout(LayoutAnimation {
                create: None,
                update: Some(LayoutTransition {
                    curve: LayoutCurve::EaseInEaseOut,
                    property: LayoutProperty::Opacity,
                    duration_ms: duration * 1000.0,
                    delay_ms: 0.0,
                }),
                delete: None,
            });
        }
    });
    let lift = computed(move || {
        let height = keyboard_height();
        if !props.enabled.get() || height == 0.0 {
            0.0
        } else {
            (height + props.keyboard_vertical_offset.get()).max(0.0)
        }
    });
    // The room it makes, by `behavior`.
    let room = move |behavior: AvoidBehavior| {
        move || EdgeInsets {
            bottom: if props.behavior.get() == behavior {
                lift.get()
            } else {
                0.0
            },
            ..EdgeInsets::default()
        }
    };
    let children = props.children.get_untracked();
    view()
        .inset_padding(room(AvoidBehavior::Padding))
        .inset_margin(room(AvoidBehavior::Height))
        .child(dynamic(move || {
            if props.behavior.get() == AvoidBehavior::Position {
                view()
                    .inline_style(props.content_container_style)
                    .relative()
                    .bottom(move || px(lift.get()))
                    .child(children)
                    .into_element()
            } else {
                children.into_element()
            }
        }))
}
