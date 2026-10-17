//! Modules: functions a UI set exports to JS without a component
//! (`Alert.alert(...)`, `Keyboard.dismiss()`), written in Rust with
//! `#[module]` and generated into the set's JS module like its components.
//! A call is an op (ordered with the props set before it); a function
//! returning a value gives JS a `Promise`, an `async fn` runs as a task.

use std::{future::Future, pin::Pin};

use super::{
    DialogAnswer, PropError, TsType, TsTypes,
    task::{self, Action},
};
use crate::{Command, Dialog, JsonValue, LayoutAnimation, ROOT_ID, StatusBarSetting};

/// A JS object of functions (`#[module]` implements it).
pub trait Module {
    /// Its JS name: the type's own (`#[module] impl Alert`: `Alert`), which
    /// `#[module]` writes; JS calls its functions by it (`Alert.alert`).
    const NAME: &'static str;
    /// Documentation (the `impl` block's doc comment).
    const DOC: &'static str;
    /// Its functions.
    const FUNCTIONS: &'static [Function];
    /// Its functions are exported one by one (`setStatusBarBackgroundColor`)
    /// rather than as an object (`#[module(flat)]`).
    const FLAT: bool = false;
}

/// A module function's parameter: its JS name, TypeScript type, and
/// whether it may be left out.
pub type ModuleParam = (&'static str, TsType, bool);

/// The future of an `async` module function: its result, or the error the
/// `Promise` rejects with.
pub type ModuleFuture = Pin<Box<dyn Future<Output = Result<JsonValue, String>>>>;

/// How a module function runs.
pub enum FunctionCall {
    /// At once, with a [`ModuleCx`]; an error rejects the `Promise`.
    Sync(fn(Vec<JsonValue>, &mut ModuleCx) -> Result<JsonValue, PropError>),
    /// As a task (its arguments checked first): it reaches the platform with
    /// the same free functions views use (`dialog`, `clipboard_string`, ...).
    Async(fn(Vec<JsonValue>) -> Result<ModuleFuture, PropError>),
}

/// A function of a module (`#[module]` makes them).
pub struct Function {
    /// Its JS name (`alert`).
    pub name: &'static str,
    /// Documentation.
    pub doc: &'static str,
    /// Its parameters.
    pub params: &'static [ModuleParam],
    /// The TypeScript type of its result; `None`: nothing to wait for.
    pub returns: Option<TsType>,
    /// How it runs.
    pub call: FunctionCall,
}

/// One function, for the glue and its declarations.
#[derive(Debug, Clone)]
pub struct FunctionSpec {
    /// Its JS name.
    pub name: &'static str,
    /// Documentation.
    pub doc: &'static str,
    /// Its parameters: JS name, TypeScript type, optional.
    pub params: Vec<(&'static str, String, bool)>,
    /// The TypeScript type of its result.
    pub returns: Option<String>,
}

/// What a module is to JS.
#[derive(Debug, Clone)]
pub struct ModuleSpec {
    /// Its JS name.
    pub name: &'static str,
    /// Documentation.
    pub doc: &'static str,
    /// Its functions.
    pub functions: Vec<FunctionSpec>,
    /// Exported one by one ([`Module::FLAT`]).
    pub flat: bool,
    /// The named TypeScript types they use.
    pub types: TsTypes,
}

impl ModuleSpec {
    pub(crate) fn of<M: Module>() -> Self {
        let mut types = TsTypes::default();
        let functions = M::FUNCTIONS
            .iter()
            .map(|f| FunctionSpec {
                name: f.name,
                doc: f.doc,
                params: f
                    .params
                    .iter()
                    .map(|(n, ts, optional)| (*n, ts(&mut types), *optional))
                    .collect(),
                returns: f.returns.map(|ts| ts(&mut types)),
            })
            .collect();
        Self {
            name: M::NAME,
            doc: M::DOC,
            functions,
            flat: M::FLAT,
            types,
        }
    }
}

/// The context of a module function that is not `async`: the platform's
/// services, applied in order right after the call (an `async` one uses the
/// free functions, `dialog(..)`, ...).
#[derive(Default)]
pub struct ModuleCx {
    pub(crate) actions: Vec<Action>,
}

impl ModuleCx {
    /// Shows `dialog`; await the answer in an `async` function.
    pub fn dialog(&mut self, dialog: Dialog) -> DialogAnswer {
        let (answer, slot) = DialogAnswer::new();
        self.actions.push(Action::Dialog(dialog, slot));
        answer
    }

    /// Unfocuses the focused input, hiding the software keyboard.
    pub fn dismiss_keyboard(&mut self) {
        self.actions.push(Action::Command(ROOT_ID, Command::Blur));
    }

    /// Changes what functions set for the status bar (the fields `patch`
    /// sets), under the settings components render.
    pub fn set_status_bar(&mut self, patch: StatusBarSetting) {
        self.actions.push(task::status_bar(patch));
    }

    /// Animates the layout changes of the next commit.
    pub fn animate_layout(&mut self, animation: LayoutAnimation) {
        self.actions.push(Action::Command(
            ROOT_ID,
            Command::LayoutAnimation(animation),
        ));
    }

    /// The screen reader reads `message` (TS:
    /// `AccessibilityInfo.announceForAccessibility`).
    pub fn announce_for_accessibility(&mut self, message: impl Into<String>) {
        self.actions.push(super::services::announce(message.into()));
    }

    /// Puts `text` on the system clipboard (TS: `Clipboard.setString`).
    pub fn set_clipboard(&mut self, text: impl Into<String>) {
        self.actions
            .push(super::services::set_clipboard(text.into()));
    }

    /// Shows `message` at the bottom of the screen (Android's toasts);
    /// nothing happens elsewhere.
    pub fn toast(&mut self, message: impl Into<String>, duration: super::ToastDuration) {
        self.actions
            .push(super::services::toast(message.into(), duration));
    }

    /// Shows the surface in `appearance` whatever the system's (TS:
    /// `setColorScheme`): [`crate::ui::prelude::dark`] (the theme's mode),
    /// `dark:` classes and the platform's controls follow it;
    /// [`crate::platform::Appearance::System`] follows the system again.
    pub fn set_color_scheme(&mut self, appearance: crate::platform::Appearance) {
        self.actions.push(Action::Appearance(appearance));
    }

    /// Lays the surface out right to left whatever the user's language
    /// (TS: `I18nManager.forceRTL`); `false` follows the language again.
    pub fn force_rtl(&mut self, force: bool) {
        self.actions
            .push(Action::Direction(super::services::Direction::Force(force)));
    }

    /// Whether a right-to-left language lays the surface out right to left
    /// (TS: `I18nManager.allowRTL`; true by default).
    pub fn allow_rtl(&mut self, allow: bool) {
        self.actions
            .push(Action::Direction(super::services::Direction::Allow(allow)));
    }

    /// Asks the platform for `action` (the runtime's own modules).
    pub(crate) fn act(&mut self, action: Action) {
        self.actions.push(action);
    }
}
