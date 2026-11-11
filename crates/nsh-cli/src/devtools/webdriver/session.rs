//! A WebDriver session: what the client asked for (capabilities), the
//! timeouts it set and the app it holds.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use nsh_dev::protocol::TestEnvironment;
use serde_json::{Map, Value, json};

use super::error::WdError;
use crate::{devtools::Session, run::Target};

/// What the new-session request asked for: `alwaysMatch` merged with the first
/// `firstMatch`.
#[derive(Debug, Clone)]
pub(crate) struct Caps {
    /// The platform's development host or packaged app runs on.
    pub(crate) target: Target,
    /// `platformName` as the client wrote it.
    pub(crate) platform_name: String,
    /// `appium:app` as the client wrote it.
    pub(crate) app_name: String,
    /// `appium:app` resolved against `--app-dir`.
    pub(crate) app: PathBuf,
    /// `nsh:options.storage`: what the app's storage starts with.
    pub(crate) storage: Vec<(String, String)>,
    /// `nsh:options.environment`: what the app's environment is fixed to.
    pub(crate) environment: TestEnvironment,
}

impl Caps {
    /// The capabilities of a new-session request body.
    pub(crate) fn parse(body: &Value, app_dir: &Path) -> Result<Caps, WdError> {
        let requested = body
            .get("capabilities")
            .and_then(Value::as_object)
            .ok_or_else(|| WdError::invalid_argument("the request has no `capabilities`"))?;
        let mut merged = match requested.get("alwaysMatch") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(always)) => always.clone(),
            Some(_) => return Err(WdError::invalid_argument("`alwaysMatch` is not an object")),
        };
        match requested.get("firstMatch") {
            None | Some(Value::Null) => {}
            Some(Value::Array(options)) => {
                if let Some(first) = options.first() {
                    let first = first.as_object().ok_or_else(|| {
                        WdError::invalid_argument("a `firstMatch` entry is not an object")
                    })?;
                    for (key, value) in first {
                        merged.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
            }
            Some(_) => return Err(WdError::invalid_argument("`firstMatch` is not an array")),
        }

        let platform_name = merged
            .get("platformName")
            .and_then(Value::as_str)
            .ok_or_else(|| WdError::session_not_created("`platformName` is required"))?
            .to_owned();
        let target = match platform_name.trim().to_lowercase().as_str() {
            "mac" | "macos" => Target::Macos,
            "windows" => Target::Windows,
            other => {
                return Err(WdError::session_not_created(format!(
                    "platformName `{other}`: NSH apps run on `mac` or `windows`"
                )));
            }
        };
        let app_name = merged
            .get("appium:app")
            .and_then(Value::as_str)
            .filter(|app| !app.is_empty())
            .ok_or_else(|| {
                WdError::session_not_created(
                    "`appium:app` is required: an app's entry (`.tsx`) or a packaged app",
                )
            })?
            .to_owned();
        let app = Path::new(&app_name);
        let app = if app.is_relative() {
            app_dir.join(app)
        } else {
            app.to_owned()
        };

        let mut storage = Vec::new();
        let mut environment = TestEnvironment::default();
        match merged.get("nsh:options") {
            None | Some(Value::Null) => {}
            Some(Value::Object(options)) => {
                match options.get("storage") {
                    None | Some(Value::Null) => {}
                    Some(Value::Object(entries)) => {
                        for (key, value) in entries {
                            let value = value.as_str().ok_or_else(|| {
                                WdError::session_not_created(format!(
                                    "`nsh:options.storage.{key}` is not a string"
                                ))
                            })?;
                            storage.push((key.clone(), value.to_owned()));
                        }
                    }
                    Some(_) => {
                        return Err(WdError::session_not_created(
                            "`nsh:options.storage` is not an object",
                        ));
                    }
                }
                match options.get("environment") {
                    None | Some(Value::Null) => {}
                    Some(value) => {
                        environment = serde_json::from_value(value.clone()).map_err(|e| {
                            WdError::session_not_created(format!("`nsh:options.environment`: {e}"))
                        })?;
                    }
                }
            }
            Some(_) => {
                return Err(WdError::session_not_created(
                    "`nsh:options` is not an object",
                ));
            }
        }
        Ok(Caps {
            target,
            platform_name,
            app_name,
            app,
            storage,
            environment,
        })
    }

    /// The capabilities the new-session answer reports.
    pub(crate) fn answer(&self) -> Value {
        json!({
            "platformName": self.platform_name,
            "appium:app": self.app_name,
            "appium:automationName": "NSH",
        })
    }
}

/// The session's timeouts in milliseconds (WebDriver's defaults). `implicit`
/// is how long a find waits for the element to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Timeouts {
    pub(crate) implicit: u64,
    pub(crate) page_load: u64,
    pub(crate) script: u64,
}

impl Default for Timeouts {
    fn default() -> Timeouts {
        Timeouts {
            implicit: 0,
            page_load: 300_000,
            script: 30_000,
        }
    }
}

impl Timeouts {
    /// As `GET /timeouts` answers them.
    pub(crate) fn to_json(self) -> Value {
        json!({ "implicit": self.implicit, "pageLoad": self.page_load, "script": self.script })
    }

    /// Takes the timeouts a `POST /timeouts` body sets; the others stay. All
    /// are checked before any is kept.
    pub(crate) fn set(&mut self, body: &Value) -> Result<(), WdError> {
        let body = body
            .as_object()
            .ok_or_else(|| WdError::invalid_argument("the body is not an object"))?;
        let mut next = *self;
        for (key, value) in body {
            let slot = match key.as_str() {
                "implicit" => &mut next.implicit,
                "pageLoad" => &mut next.page_load,
                "script" => {
                    // `null` turns the script timeout off; scripts do not run.
                    if value.is_null() {
                        continue;
                    }
                    &mut next.script
                }
                _ => continue,
            };
            *slot = value.as_u64().ok_or_else(|| {
                WdError::invalid_argument(format!("`{key}` is not a non-negative integer"))
            })?;
        }
        *self = next;
        Ok(())
    }
}

/// The session a client holds: its id and its app.
pub(crate) struct Open {
    /// The session id the client uses in paths.
    pub(crate) id: String,
    /// The app's DevTools protocol.
    pub(crate) session: Arc<dyn Session>,
    /// The client's timeouts.
    pub(crate) timeouts: Timeouts,
    /// What the elements the client found showed, to tell a recycled list
    /// cell's new content from the element found.
    pub(crate) elements: super::element::Elements,
}
