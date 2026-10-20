//! `nsh/accessibility`: the screen reader's commands.

use crate::{
    Command,
    ui::{ModuleCx, NodeRef, module, task::Action},
};

/// Screen reader and motion settings.
#[module]
pub(crate) struct AccessibilityInfo;

#[module]
impl AccessibilityInfo {
    /// The screen reader reads `message` (a VoiceOver announcement, TalkBack).
    pub(crate) fn announce_for_accessibility(message: String, cx: &mut ModuleCx) {
        cx.announce_for_accessibility(message);
    }

    /// Moves the screen reader's focus to `node` (a `ref`).
    pub(crate) fn set_accessibility_focus(node: NodeRef, cx: &mut ModuleCx) {
        cx.act(Action::Command(node.0, Command::AccessibilityFocus));
    }
}
