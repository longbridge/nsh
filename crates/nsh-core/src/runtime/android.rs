//! `nsh/android`: what only Android has (nothing happens elsewhere).

use crate::{
    Command, ROOT_ID,
    ui::{ModuleCx, ToastDuration, module, task::Action},
};

/// Short messages at the bottom of the screen (Android's toasts).
#[module]
pub(crate) struct Toast;

#[module]
impl Toast {
    /// Shows `message` (`"short"` by default).
    pub(crate) fn show(message: String, duration: Option<ToastDuration>, cx: &mut ModuleCx) {
        cx.toast(message, duration.unwrap_or_default());
    }
}

/// The Android back button: its listeners run after modals and before
/// stacks.
#[module]
pub(crate) struct BackHandler;

#[module]
impl BackHandler {
    /// Leaves the app.
    pub(crate) fn exit_app(cx: &mut ModuleCx) {
        cx.act(Action::Command(ROOT_ID, Command::ExitApp));
    }
}
