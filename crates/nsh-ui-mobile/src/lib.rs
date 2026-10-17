//! The components of `nsh/ui/mobile`, written in Rust.
//!
//! Each component renders nsh's elements with typed styles and theme colors
//! ([`nsh_core::ui`]): one implementation draws the same on iOS and Android,
//! lays out in the core's pass and handles its own interaction natively.
//! Its JS face (the `nsh/ui/mobile` export and its TypeScript declarations)
//! is generated from its props and events by the bundler. See README.md for
//! how to write one.

mod button;
mod card;
mod group;
mod keyboard;
mod list_view;
mod safe_area;
mod select_list;
mod sheet;
mod status_bar;
mod stepper;
mod system;
mod text_field;

use std::sync::Arc;

use nsh_core::ui::{ComponentSpec, Registry};

pub use crate::{
    button::{Button, ButtonEvent, ButtonProps, ButtonVariant},
    card::{Card, CardProps, CardVariant},
    group::{Group, GroupProps},
    keyboard::{AvoidBehavior, Keyboard, KeyboardAvoidingView, KeyboardAvoidingViewProps},
    list_view::{ListView, ListViewProps, ListViewVariant},
    safe_area::{Edge, SafeAreaView, SafeAreaViewProps},
    select_list::{SelectList, SelectListEvent, SelectListProps},
    sheet::{Sheet, SheetEvent, SheetProps},
    status_bar::{BarAnimation, BarStyle, StatusBar, StatusBarProps},
    stepper::{Stepper, StepperApi, StepperEvent, StepperProps},
    system::{ActionSheet, ActionSheetOptions, Alert, AlertButton, AlertButtonStyle, AlertOptions},
    text_field::{TextField, TextFieldEvent, TextFieldProps},
};

/// The JS module apps import the mobile UI as.
pub const SPECIFIER: &str = "nsh/ui/mobile";

/// Every component of the mobile set, for an engine to render.
pub fn registry() -> Registry {
    let mut registry = Registry::ui_set(SPECIFIER);
    registry
        .register::<Button>()
        .register::<Card>()
        .register::<Group>()
        .register::<ListView>()
        .register::<Stepper>()
        .register::<TextField>()
        .register::<Sheet>()
        .register::<SelectList>()
        .register::<SafeAreaView>()
        .register::<KeyboardAvoidingView>()
        .register::<StatusBar>()
        .module::<Alert>()
        .module::<ActionSheet>()
        .module::<Keyboard>()
        .module::<StatusBar>()
        // The runtime's accessors and hooks of the mobile UI, as they are.
        .reexport(&["safeArea", "keyboard", "useSafeAreaInsets"], &["Insets"])
        // Navigation: made by `nsh-js` (`navigation.rs`), declared by hand.
        .native_values(&["Stack", "Screen", "Modal", "createNavigator"]);
    registry
}

/// The specs of the mobile set, for the generated JS glue.
pub fn specs() -> Vec<Arc<ComponentSpec>> {
    registry().specs()
}

#[cfg(test)]
mod tests {
    /// The exports are named after the types (`std::any::type_name`): a
    /// renamed type, or a compiler that formats type names differently,
    /// changes what apps import.
    #[test]
    fn the_exports_are_pinned() {
        let registry = super::registry();
        let components: Vec<_> = registry.specs().iter().map(|s| s.export.clone()).collect();
        assert_eq!(
            components,
            [
                "Button",
                "Card",
                "Group",
                "KeyboardAvoidingView",
                "ListView",
                "SafeAreaView",
                "SelectList",
                "Sheet",
                "StatusBar",
                "Stepper",
                "TextField",
            ]
        );
        let modules: Vec<_> = registry.module_specs().iter().map(|m| m.name).collect();
        assert_eq!(modules, ["ActionSheet", "Alert", "Keyboard", "StatusBar"]);
    }
}
