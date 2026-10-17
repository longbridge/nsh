//! `Alert` and `ActionSheet`: the platform's dialogs
//! (UIAlertController, AlertDialog).

use nsh_core::ui::{Dialog, DialogButton, DialogButtonStyle, prelude::*};

/// Native alerts.
#[derive(Debug)]
pub struct Alert;

/// A button of an alert.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct AlertButton {
    /// Its label (`"OK"` by default).
    pub text: Option<SharedString>,
    /// How it looks.
    #[prop(default)]
    pub style: AlertButtonStyle,
}

/// How an alert's button looks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum AlertButtonStyle {
    /// A regular button.
    #[default]
    Default,
    /// The cancel button.
    Cancel,
    /// A destructive action.
    Destructive,
}

impl From<AlertButtonStyle> for DialogButtonStyle {
    fn from(style: AlertButtonStyle) -> Self {
        match style {
            AlertButtonStyle::Default => DialogButtonStyle::Default,
            AlertButtonStyle::Cancel => DialogButtonStyle::Cancel,
            AlertButtonStyle::Destructive => DialogButtonStyle::Destructive,
        }
    }
}

/// Options of an alert.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct AlertOptions {
    /// Tapping outside or the back button dismisses it (Android; default
    /// false).
    #[prop(default)]
    pub cancelable: bool,
}

/// Native alerts (UIAlertController / AlertDialog).
#[module]
impl Alert {
    /// Shows an alert (at most three buttons on Android; one "OK" when
    /// none is given) and resolves with the index of the button pressed,
    /// -1 when it was dismissed.
    pub async fn alert(
        title: SharedString,
        message: Option<SharedString>,
        buttons: Option<Vec<AlertButton>>,
        options: Option<AlertOptions>,
    ) -> i32 {
        let buttons = buttons
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| vec![AlertButton::default()]);
        dialog(Dialog {
            id: 0,
            title: Some(title.to_string()),
            message: message.map(|m| m.to_string()),
            buttons: buttons
                .iter()
                .map(|b| DialogButton {
                    text: b.text.as_deref().unwrap_or("OK").to_owned(),
                    style: b.style.into(),
                })
                .collect(),
            cancelable: options.is_some_and(|o| o.cancelable),
            sheet: false,
        })
        .await
    }
}

/// A sheet of actions.
#[derive(Debug)]
pub struct ActionSheet;

/// What an action sheet shows.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct ActionSheetOptions {
    /// The actions.
    pub options: Vec<SharedString>,
    /// The index of the cancel action.
    pub cancel_button_index: Option<u32>,
    /// The indexes of destructive actions.
    pub destructive_button_indexes: Option<Vec<u32>>,
    /// Above the actions.
    pub title: Option<SharedString>,
    /// Below the title.
    pub message: Option<SharedString>,
}

/// A sheet of actions (UIAlertController action sheet; a list dialog on
/// Android).
#[module]
impl ActionSheet {
    /// Shows the sheet and resolves with the index of the action chosen
    /// (the cancel index, or -1, when it was dismissed).
    pub async fn show(options: ActionSheetOptions) -> i32 {
        let destructive = options
            .destructive_button_indexes
            .clone()
            .unwrap_or_default();
        let index = dialog(Dialog {
            id: 0,
            title: options.title.as_ref().map(|t| t.to_string()),
            message: options.message.as_ref().map(|m| m.to_string()),
            buttons: options
                .options
                .iter()
                .enumerate()
                .map(|(i, text)| DialogButton {
                    text: text.to_string(),
                    style: if Some(i as u32) == options.cancel_button_index {
                        DialogButtonStyle::Cancel
                    } else if destructive.contains(&(i as u32)) {
                        DialogButtonStyle::Destructive
                    } else {
                        DialogButtonStyle::Default
                    },
                })
                .collect(),
            cancelable: true,
            sheet: true,
        })
        .await;
        if index >= 0 {
            index
        } else {
            options.cancel_button_index.map_or(-1, |i| i as i32)
        }
    }
}
