//! What the popups of `nsh/base` share (`Popover`, `Menu`, `ContextMenu`,
//! `Tooltip`, `Sheet`): their open state, controlled or not, and why it
//! changed. They show their content in the core's overlays
//! (`nsh_core::ui::overlay`), which place it and report presses outside,
//! Escape and the back button; deciding to close is theirs.
//!
//! How a popup presents itself follows the device (`auto`, the only
//! presentation for now): on the desktops (and desktop browsers) a menu is
//! a popup at its trigger or the pointer, with cascading submenus, and a
//! press outside closes a popup and goes on to what is under it; on phones
//! and tablets a menu is a sheet at the bottom of the screen whose
//! submenus replace its items, and a press outside a popup only closes it
//! (a transparent backdrop takes it). A popover stays anchored to its
//! trigger everywhere: what it shows belongs next to it.

use nsh_core::ui::prelude::*;

/// Why a popup opened or closed (`onOpenChange`'s `reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub enum OpenChangeReason {
    /// Its trigger was pressed.
    TriggerPress,
    /// A press went down outside it.
    PressOutside,
    /// Its backdrop was pressed (a sheet's).
    BackdropPress,
    /// Escape or the Android back button.
    Cancel,
    /// An item was chosen (a menu's).
    Select,
    /// The pointer came over its trigger or left it (a tooltip's).
    Hover,
    /// A finger was held on its trigger (a tooltip's, a context menu's on
    /// touch screens), or lifted.
    LongPress,
    /// The secondary button was pressed on its trigger (a context menu's).
    ContextMenu,
}

/// Whether a popup is open: the app's `open` when it sets one, else its own
/// state (from `defaultOpen`).
#[derive(Clone, Copy)]
pub(crate) struct OpenState {
    controlled: Prop<Option<bool>>,
    own: Signal<bool>,
}

impl OpenState {
    pub(crate) fn new(open: Prop<Option<bool>>, default_open: Prop<bool>) -> Self {
        Self {
            controlled: open,
            own: signal(default_open.get_untracked()),
        }
    }

    /// Whether it is open (tracked).
    pub(crate) fn get(self) -> bool {
        self.controlled.get().unwrap_or_else(|| self.own.get())
    }

    pub(crate) fn get_untracked(self) -> bool {
        self.controlled
            .get_untracked()
            .unwrap_or_else(|| self.own.get_untracked())
    }

    /// The user asks for it to be `open`: its own state follows when the
    /// app does not control it; whether it is a change to report.
    pub(crate) fn request(self, open: bool) -> bool {
        if self.get_untracked() == open {
            return false;
        }
        if self.controlled.get_untracked().is_none() {
            self.own.set(open);
        }
        true
    }
}
