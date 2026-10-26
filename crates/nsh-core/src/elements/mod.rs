//! The primitives of `nsh` written as Rust components: views that lay out
//! by what the surface reports (`SafeAreaView`, `KeyboardAvoidingView`) or
//! tell the platform what their texts do together (`SelectionArea`). They
//! render a view with no look of their own; their JS face and declarations
//! are made from their specs ([`crate::runtime::registries`]).

mod keyboard_avoiding;
mod safe_area;
mod selection_area;

pub use self::{
    keyboard_avoiding::{
        AvoidBehavior, KeyboardAvoidingView, KeyboardAvoidingViewElement,
        KeyboardAvoidingViewProps, keyboard_avoiding_view,
    },
    safe_area::{Edge, SafeAreaView, SafeAreaViewElement, SafeAreaViewProps, safe_area_view},
    selection_area::{SelectionArea, SelectionAreaElement, SelectionAreaProps, selection_area},
};
