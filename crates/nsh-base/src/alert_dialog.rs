//! `AlertDialog`: the platform's alert or action sheet (UIAlertController,
//! AlertDialog, a list dialog on Android).

use nsh_core::ui::{Dialog, DialogButton, DialogButtonStyle, prelude::*};

/// The system's alert dialog (UIAlertController, AlertDialog).
#[module]
#[derive(Debug)]
pub struct AlertDialog;

/// What an [`AlertDialog`] shows and how it closes.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct AlertDialogOptions {
    /// Above the description.
    pub title: Option<SharedString>,
    /// Below the title.
    pub description: Option<SharedString>,
    /// Its actions, in order (at least one; at most three in an Android
    /// alert).
    pub actions: Vec<AlertDialogAction>,
    /// `"alert"` (default), or `"actionSheet"`: a sheet of actions from the
    /// bottom (a list dialog on Android).
    #[prop(default)]
    pub presentation: AlertDialogPresentation,
    /// Escape and the Android back button close it without an action
    /// (default: false for an alert, true for an action sheet).
    pub close_on_escape: Option<bool>,
    /// A press outside it closes it without an action (default: false for
    /// an alert, true for an action sheet).
    pub close_on_backdrop_press: Option<bool>,
}

/// One action of an [`AlertDialog`].
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct AlertDialogAction {
    /// What it says.
    pub label: SharedString,
    /// What it means: the platform places and draws it by its role.
    #[prop(default)]
    pub role: AlertDialogRole,
    /// What `show` resolves with when it is chosen (default: its label).
    pub id: Option<SharedString>,
}

/// What an [`AlertDialogAction`] means.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum AlertDialogRole {
    /// A regular action.
    #[default]
    Default,
    /// Closes without doing anything.
    Cancel,
    /// Destroys or deletes something.
    Destructive,
}

impl From<AlertDialogRole> for DialogButtonStyle {
    fn from(role: AlertDialogRole) -> Self {
        match role {
            AlertDialogRole::Default => DialogButtonStyle::Default,
            AlertDialogRole::Cancel => DialogButtonStyle::Cancel,
            AlertDialogRole::Destructive => DialogButtonStyle::Destructive,
        }
    }
}

/// The native form of an [`AlertDialog`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum AlertDialogPresentation {
    /// An alert in the middle.
    #[default]
    Alert,
    /// A sheet of actions (UIAlertController's action sheet; a list
    /// dialog on Android).
    ActionSheet,
}

#[module]
impl AlertDialog {
    /// Shows the dialog and resolves with the chosen action's `id` (its
    /// `label` without one), or `null` when it closed without an action.
    /// Rejects without actions.
    pub async fn show(options: AlertDialogOptions) -> Result<Option<SharedString>, String> {
        if options.actions.is_empty() {
            return Err("AlertDialog.show: `actions` is empty".into());
        }
        let sheet = options.presentation == AlertDialogPresentation::ActionSheet;
        let cancelable = options.close_on_escape.unwrap_or(sheet)
            || options.close_on_backdrop_press.unwrap_or(sheet);
        let index = dialog(Dialog {
            id: 0,
            title: options.title.as_ref().map(|t| t.to_string()),
            message: options.description.as_ref().map(|m| m.to_string()),
            buttons: options
                .actions
                .iter()
                .map(|a| DialogButton {
                    text: a.label.to_string(),
                    style: a.role.into(),
                })
                .collect(),
            cancelable,
            sheet,
        })
        .await;
        Ok(usize::try_from(index)
            .ok()
            .and_then(|i| options.actions.get(i))
            .map(|a| a.id.clone().unwrap_or_else(|| a.label.clone())))
    }
}
