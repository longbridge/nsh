//! The official theme capability, shared by native and browser modules.

use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

use crate::{
    JsonValue, Op, Prop, ROOT_ID,
    element::{
        Arg, CallCx, JsError, JsIn, JsKind, Plugin, PropError, RuntimeUpdate, System, TsTypes,
        module,
    },
};

/// The official theme service. User modules can depend on `nsh.theme`
/// and call it through the same synchronous runtime contract as `setTheme`.
#[derive(Default)]
#[non_exhaustive]
pub struct ThemePlugin {
    built: RefCell<Option<String>>,
    active: Cell<bool>,
}

impl Plugin for ThemePlugin {
    const ID: &'static str = "nsh.theme";

    fn start(&self, _system: &System) -> Result<(), String> {
        self.active.set(true);
        Ok(())
    }

    fn stop(&self) {
        self.active.set(false);
        self.built.take();
    }
}

impl ThemePlugin {
    /// Records the bundle's setup theme, which a later reset restores.
    #[doc(hidden)]
    pub fn remember_built(&self, json: &str) {
        self.built.replace(Some(json.to_owned()));
    }

    /// Replaces the app's theme, or restores the built theme for `None`.
    /// Validation completes before any operation or token publication;
    /// the calling runtime keeps host overrides and reactive readers.
    pub fn set(&self, theme: Option<&JsonValue>, cx: CallCx<'_>) -> Result<(), JsError> {
        if !self.active.get() {
            return Err(JsError::Error("the theme plugin is unavailable".into())
                .with_code(crate::element::codes::PLUGIN_UNAVAILABLE));
        }
        let instance = cx.system().plugin::<Self>()?;
        if !std::ptr::eq(self, instance.as_ref()) {
            return Err(
                JsError::Error("the theme plugin belongs to another VM".into())
                    .with_code(crate::element::codes::PLUGIN_UNAVAILABLE),
            );
        }
        let source = cx.theme_tokens();
        let (json, tokens) = match theme {
            Some(theme) => {
                if !theme.is_object() {
                    return Err(JsError::Type(
                        "setTheme: expected a theme ({ cssVars }) or null".into(),
                    ));
                }
                let json = theme.to_string();
                let tokens = source
                    .checked(&json)
                    .transpose()
                    .map_err(|error| JsError::Type(format!("setTheme: {error}")))?;
                (json, tokens)
            }
            None => {
                let json = self
                    .built
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| r#"{"cssVars":{}}"#.into());
                let tokens = source.of(&json);
                (json, tokens)
            }
        };
        let mut update = RuntimeUpdate::default().operation(Op::SetProp {
            id: ROOT_ID,
            prop: Prop::UiTheme(json.into()),
        });
        update.tokens = tokens.map(Arc::new);
        cx.apply_runtime(update)
    }
}

pub(crate) struct ThemeInput(Option<JsonValue>);

impl Arg for ThemeInput {
    fn ts_type(_types: &mut TsTypes) -> String {
        "Theme | null".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if value.is_nullish() {
            return Ok(Self(None));
        }
        if !matches!(value.kind(), JsKind::Object | JsKind::Array) {
            return Err(value.expected("a theme ({ cssVars }) or null"));
        }
        value.to_json().map(|json| Self(Some(json)))
    }
}

/// The app's runtime theme.
#[module]
pub(crate) struct ThemeRuntime;

#[module(flat)]
impl ThemeRuntime {
    /// Makes `theme` the app's theme while it runs, in place of `nsh.json`'s
    /// theme. `token()` reads it at once; classes and components follow in
    /// the next frame. Host overrides keep winning. `null` restores the
    /// built theme. An unreadable value throws a `TypeError` and changes nothing.
    pub(crate) fn set_theme(
        theme: ThemeInput,
        system: System,
        cx: CallCx<'_>,
    ) -> Result<(), JsError> {
        system.plugin::<ThemePlugin>()?.set(theme.0.as_ref(), cx)
    }
}
