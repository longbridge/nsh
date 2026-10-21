//! `nsh/base`: the behavior and infrastructure foundation of NSH apps' UI,
//! written in Rust, with no look of its own (as `gpui-base` is GPUI Kit's):
//! controls (`Button`, `Checkbox`, `Radio` in a `RadioGroup`, `Switch`),
//! navigation (stacks and pages), dialogs, popups on the core's overlays
//! (`Popover`, `Menu`, `ContextMenu`, `Tooltip`, `Sheet`), safe areas,
//! selection areas,
//! the system's dialogs and the software keyboard, and the theme's semantic
//! tokens (`Theme`, as `gpui-base`'s).
//!
//! What it renders lays out or tells the platform, never styles: the look
//! of an app is the app's, drawn with nsh's primitives or the components of
//! its own crate. Its JS face (the `nsh/base` export and its TypeScript
//! declarations) is generated from the components' props and events and the
//! modules' functions. Every host serves it.

mod alert_dialog;
mod button;
mod checkbox;
mod keyboard;
mod menu;
mod navigation;
mod popover;
mod popup;
mod radio;
mod safe_area;
mod selection_area;
mod sheet;
mod switch;
pub mod text_view;
mod theme;
mod tooltip;

use std::sync::Arc;

use nsh_core::ui::{ComponentSpec, Registry};

pub use crate::{
    alert_dialog::{
        AlertDialog, AlertDialogAction, AlertDialogOptions, AlertDialogPresentation,
        AlertDialogRole, alert_dialog,
    },
    button::{Button, ButtonElement, ButtonEvent, ButtonProps, button},
    checkbox::{Checkbox, CheckboxElement, CheckboxEvent, CheckboxProps, checkbox},
    keyboard::{
        AvoidBehavior, Keyboard, KeyboardAvoidingView, KeyboardAvoidingViewElement,
        KeyboardAvoidingViewProps, keyboard_avoiding_view,
    },
    menu::{
        ContextMenu, ContextMenuElement, ContextMenuProps, Menu, MenuElement, MenuEvent, MenuItem,
        MenuItemElement, MenuItemEvent, MenuItemProps, MenuProps, MenuSeparator,
        MenuSeparatorElement, MenuSeparatorProps, Submenu, SubmenuElement, SubmenuProps,
        context_menu, menu, menu_item, menu_separator, submenu,
    },
    navigation::{dialog, nav_page, nav_stack},
    popover::{Popover, PopoverElement, PopoverEvent, PopoverProps, popover},
    popup::OpenChangeReason,
    radio::{
        Radio, RadioElement, RadioEvent, RadioGroup, RadioGroupElement, RadioGroupProps,
        RadioProps, radio, radio_group,
    },
    safe_area::{Edge, SafeAreaView, SafeAreaViewElement, SafeAreaViewProps, safe_area_view},
    selection_area::{SelectionArea, SelectionAreaElement, SelectionAreaProps, selection_area},
    sheet::{Sheet, SheetElement, SheetEvent, SheetProps, sheet},
    switch::{Switch, SwitchElement, SwitchEvent, SwitchProps, switch},
    text_view::{
        CodeHighlighter, MarkdownExtension, MarkdownOptions, MarkdownPlugin, MarkdownPlugins,
        TextView, TextViewClasses, TextViewCodeAction, TextViewCodeActionEvent, TextViewElement,
        TextViewEvent, TextViewLinkPressEvent, TextViewMath, TextViewPart, TextViewParts,
        TextViewProps, TextViewStyles, text_view,
    },
    theme::{DEFAULT_THEME, Theme, ThemeError, theme, tokens_of},
    tooltip::{Tooltip, TooltipElement, TooltipEvent, TooltipProps, tooltip},
};

/// The JS module apps import the foundation as.
pub const SPECIFIER: &str = "nsh/base";

/// Every component and module of `nsh/base`, for an engine to serve.
pub fn registry() -> Registry {
    let mut registry = Registry::ui_set(SPECIFIER);
    registry
        .register::<Button>()
        .register::<ContextMenu>()
        .register::<Menu>()
        .register::<MenuItem>()
        .register::<MenuSeparator>()
        .register::<Submenu>()
        .register::<Popover>()
        .register::<Sheet>()
        .register::<Tooltip>()
        .register::<SafeAreaView>()
        .register::<KeyboardAvoidingView>()
        .register::<SelectionArea>()
        .register::<Checkbox>()
        .register::<Radio>()
        .register::<RadioGroup>()
        .register::<Switch>()
        .register::<TextView>()
        .module::<AlertDialog>()
        .module::<Keyboard>()
        .module::<TextView>()
        // The environment's accessors and hooks, as they are.
        .reexport(&["safeArea", "safeAreaInsets", "keyboardFrame"], &["Edges"])
        // Navigation: made by `nsh-js` (`navigation.rs`), declared by hand.
        .native_values(&["NavStack", "NavPage", "Dialog", "createNavStack"])
        // The theme: the app's over the default, as design tokens.
        .tokens_from(tokens_of);
    registry
}

/// The specs of `nsh/base`, for the generated JS glue.
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
                "Checkbox",
                "ContextMenu",
                "KeyboardAvoidingView",
                "Menu",
                "MenuItem",
                "MenuSeparator",
                "Popover",
                "Radio",
                "RadioGroup",
                "SafeAreaView",
                "SelectionArea",
                "Sheet",
                "Submenu",
                "Switch",
                "TextView",
                "Tooltip"
            ]
        );
        let modules: Vec<_> = registry.module_specs().iter().map(|m| m.name).collect();
        assert_eq!(modules, ["AlertDialog", "Keyboard", "TextView"]);
    }

    /// Every export TS sees has a doc comment (the declarations show it).
    #[test]
    fn every_export_is_documented() {
        assert_eq!(super::registry().undocumented(), Vec::<String>::new());
    }
}
