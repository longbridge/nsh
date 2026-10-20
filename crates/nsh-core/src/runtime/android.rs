//! `nsh/android`: what only Android has (nothing happens elsewhere).

use crate::{
    Command, ROOT_ID, StatusBarSetting,
    ui::{Color, ModuleCx, ToastDuration, module, task::Action},
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

/// The back button, after modals and stacks had their turn.
#[module]
pub(crate) struct BackHandler;

#[module]
impl BackHandler {
    /// Leaves the app.
    pub(crate) fn exit_app(cx: &mut ModuleCx) {
        cx.act(Action::Command(ROOT_ID, Command::ExitApp));
    }
}

/// `nsh/android`'s functions of their own.
#[module]
pub(crate) struct Android;

#[module(flat)]
impl Android {
    /// The status bar's color (before edge-to-edge, API 35), under the
    /// rendered `<StatusBar android={{ backgroundColor }}>`s.
    pub(crate) fn set_status_bar_background_color(
        background_color: Color,
        animated: Option<bool>,
        cx: &mut ModuleCx,
    ) {
        cx.set_status_bar(StatusBarSetting {
            background: Some(background_color),
            animated,
            ..Default::default()
        });
    }
}
