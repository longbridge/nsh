//! `nsh/test`, the module test files import: `test` declares a test,
//! `open` starts the app on a headless engine of its own and gives an
//! [`App`] to take steps on, `expect` checks values. Written in Rust, its
//! TypeScript face and declarations are generated, as a UI set's are; only
//! the engine `nsh test` runs a test file on serves it.

use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Duration};

use nsh_core::{
    JsonValue,
    ui::{JsHost, JsIn, JsOwned, JsValue, PropError, Registry, TsTypes, prelude::*},
};
use nsh_engine::headless::{Headless, MemoryStorage, StubResponse};

use crate::run::{OpenApp, Session};

/// The specifier test files import.
pub const SPECIFIER: &str = "nsh/test";

/// The module of the entry `nsh test` makes for a test file, which runs
/// its tests once it is loaded: no test file imports it.
pub(crate) const RUNNER: &str = "nsh/test/runner";

/// The `nsh/test` set, for the engine of test files, the bundler and the
/// declarations (`.nsh/types/nsh/test`).
pub fn registry() -> Registry {
    let mut registry = Registry::ui_set(SPECIFIER);
    registry
        .module::<Testing>()
        .class::<App>()
        .class::<Expectation>();
    registry
}

/// The set of the entry `nsh test` makes for a test file.
pub(crate) fn runner_registry() -> Registry {
    let mut registry = Registry::ui_set(RUNNER);
    registry.module::<Runner>();
    registry
}

thread_local! {
    /// The tests the test file declared, in order (the engine's thread is
    /// the test file's).
    static TESTS: RefCell<Vec<(String, Callback)>> = const { RefCell::new(Vec::new()) };
}

/// The tests declared so far, taken.
pub(crate) fn take_tests() -> Vec<(String, Callback)> {
    TESTS.with(|tests| std::mem::take(&mut *tests.borrow_mut()))
}

/// Tests of an app, run headless by `nsh test`: each test opens the app on
/// an engine of its own and takes the steps a user would.
#[module]
pub struct Testing;

#[module(flat)]
impl Testing {
    /// Declares the test `name`: `nsh test` runs `body` after the file is
    /// loaded, the tests of a file one after the other, in order. A test
    /// fails when `body` throws or the promise it returns rejects, and
    /// after 30 seconds. The apps it opened are closed when it ends.
    pub fn test(name: String, body: Callback) {
        TESTS.with(|tests| tests.borrow_mut().push((name, body)));
    }

    /// Starts the app whose entry is `entry` (relative to the test file)
    /// on a phone-sized headless engine of its own, with what `options`
    /// set up, and resolves once it rendered and is quiet. Its requests
    /// fail unless `options.network` lists them; its storage starts with
    /// `options.storage`.
    pub async fn open(entry: String, options: Option<OpenOptions>) -> Result<App, String> {
        let session = Session::current()?;
        let options = options.unwrap_or_default();
        let (app, storage) = blocking(move || session.open(&entry, options))
            .await
            .map_err(|e| e.to_string())??;
        Ok(App { app, storage })
    }

    /// What to expect of `value`, compared as JSON is (`undefined` is
    /// `null`): `expect(app.texts()).toContain("Saved")`.
    pub fn expect(value: JsonValue) -> Expectation {
        Expectation { value, not: false }
    }
}

/// What a test runs: the internal module of the entry `nsh test` makes.
#[module]
pub(crate) struct Runner;

#[module(flat)]
impl Runner {
    /// Runs the tests the test file declared.
    pub(crate) async fn run() {
        crate::run::run_tests().await;
    }
}

/// How `open` starts the app.
#[derive(Debug, Default, JsObject)]
pub struct OpenOptions {
    /// What the app's storage (`nsh/storage`) holds at the start.
    pub storage: Option<Entries>,
    /// The responses its requests get, by `"<METHOD> <URL>"` (a bare URL is
    /// a `GET`); a request not listed fails. `"live"`: the network.
    pub network: Option<NetworkOption>,
    /// The viewport's width in points (390).
    pub width: Option<f64>,
    /// The viewport's height in points (844).
    pub height: Option<f64>,
    /// The safe area insets in points (none).
    pub safe_area: Option<SafeArea>,
}

/// Safe area insets in points.
#[derive(Debug, Default, Clone, Copy, JsObject)]
pub struct SafeArea {
    /// The top inset.
    #[prop(default)]
    pub top: f64,
    /// The right inset.
    #[prop(default)]
    pub right: f64,
    /// The bottom inset.
    #[prop(default)]
    pub bottom: f64,
    /// The left inset.
    #[prop(default)]
    pub left: f64,
}

/// String entries by key: TS `Record<string, string>`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Entries(pub BTreeMap<String, String>);

impl JsValue for Entries {
    fn ts_type(_types: &mut TsTypes) -> String {
        "Record<string, string>".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        let JsonValue::Object(map) = value.to_json()? else {
            return Err(value.expected("an object of strings"));
        };
        map.into_iter()
            .map(|(key, v)| match v {
                JsonValue::String(s) => Ok((key, s)),
                other => Err(PropError::expected("a string", &other)),
            })
            .collect::<Result<_, _>>()
            .map(Entries)
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        let map = self
            .0
            .iter()
            .map(|(k, v)| (k.clone(), JsonValue::String(v.clone())))
            .collect();
        Ok(JsOwned::new(host, host.json(&JsonValue::Object(map))?))
    }
}

/// The network of an app a test opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkOption {
    /// The network itself (`"live"`).
    Live,
    /// The responses listed, by `"<METHOD> <URL>"`.
    Stub(Vec<(String, StubResponse)>),
}

impl JsValue for NetworkOption {
    fn ts_type(types: &mut TsTypes) -> String {
        let response = types.declare("StubResponse", |_| {
            "/** A response `open`'s `network` gives a request. */\n\
             export interface StubResponse {\n  \
             /** The status code (200). */\n  status?: number;\n  \
             /** Header names and values. */\n  headers?: Record<string, string>;\n  \
             /** The body: a string, or a value sent as JSON (with `content-type: \
             application/json` unless `headers` name one). */\n  body?: unknown;\n}"
                .to_owned()
        });
        format!("\"live\" | Record<string, {response}>")
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        match value.to_json()? {
            JsonValue::String(s) if s == "live" => Ok(NetworkOption::Live),
            JsonValue::Object(map) => map
                .into_iter()
                .map(|(key, v)| {
                    stub_response(&v)
                        .map(|r| (key.clone(), r))
                        .map_err(|e| PropError(format!("`{key}`: {}", e.0)))
                })
                .collect::<Result<_, _>>()
                .map(NetworkOption::Stub),
            _ => Err(value.expected("\"live\" or an object of responses")),
        }
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        let json = match self {
            NetworkOption::Live => JsonValue::String("live".into()),
            NetworkOption::Stub(responses) => JsonValue::Object(
                responses
                    .iter()
                    .map(|(key, r)| {
                        let headers = r
                            .headers
                            .iter()
                            .map(|(k, v)| (k.clone(), JsonValue::String(v.clone())))
                            .collect();
                        let body = String::from_utf8_lossy(&r.body).into_owned();
                        let response = serde_json::json!({
                            "status": r.status,
                            "headers": JsonValue::Object(headers),
                            "body": body,
                        });
                        (key.clone(), response)
                    })
                    .collect(),
            ),
        };
        Ok(JsOwned::new(host, host.json(&json)?))
    }
}

/// A response as a test writes it.
fn stub_response(value: &JsonValue) -> Result<StubResponse, PropError> {
    let JsonValue::Object(map) = value else {
        return Err(PropError::expected("a response object", value));
    };
    let status = match map.get("status") {
        None | Some(JsonValue::Null) => 200,
        Some(JsonValue::Number(n)) => n
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .ok_or_else(|| PropError(format!("`status`: not a status code: {n}")))?,
        Some(other) => return Err(PropError::expected("a number", other)),
    };
    let mut headers = Vec::new();
    match map.get("headers") {
        None | Some(JsonValue::Null) => {}
        Some(JsonValue::Object(h)) => {
            for (name, v) in h {
                let JsonValue::String(v) = v else {
                    return Err(PropError(format!(
                        "`headers.{name}`: {}",
                        PropError::expected("a string", v).0
                    )));
                };
                headers.push((name.clone(), v.clone()));
            }
        }
        Some(other) => return Err(PropError::expected("an object of strings", other)),
    }
    let body = match map.get("body") {
        None | Some(JsonValue::Null) => Vec::new(),
        Some(JsonValue::String(s)) => s.clone().into_bytes(),
        Some(json) => {
            if !headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            {
                headers.push(("content-type".into(), "application/json".into()));
            }
            json.to_string().into_bytes()
        }
    };
    Ok(StubResponse {
        status,
        headers,
        body,
    })
}

/// The app a test opened (`open`), on a headless engine of its
/// own: its steps (`press`, `type`, `scroll`, ...) take the views a
/// selector names, `id:<testID>`, `label:<accessibilityLabel>`,
/// `text:<text>`, `view:<id>` or a bare string (a test id, a label, then a
/// text), where a user would look first (the top modal, the top screen of
/// a stack). A step resolves once the app is quiet and rejects, with what
/// is on screen, when it cannot be taken or the app threw.
#[class]
pub struct App {
    app: OpenApp,
    storage: Arc<MemoryStorage>,
}

/// What a step on an app that was closed says.
const CLOSED: &str = "the app is closed: the test that opened it ended";

#[class]
impl App {
    /// Presses what `selector` names (or the nearest view around it that
    /// hears presses, as a tap on a button's text does).
    pub async fn press(this: Instance<Self>, selector: String) -> Result<(), String> {
        step(&this, move |app| app.press(&selector)).await
    }

    /// Types `text` at the end of the input `selector` names (or the first
    /// input in it; `text:` matches an input's text or placeholder too),
    /// key by key; a line break in a single line is the return key.
    pub async fn r#type(
        this: Instance<Self>,
        selector: String,
        text: String,
    ) -> Result<(), String> {
        step(&this, move |app| app.type_text(&selector, &text)).await
    }

    /// The return key in the input `selector` names, as its
    /// `submitBehavior` says.
    pub async fn submit(this: Instance<Self>, selector: String) -> Result<(), String> {
        step(&this, move |app| app.submit(&selector)).await
    }

    /// Drags the scroll view `selector` names (or the one around it, or the
    /// first in it) by `distance` points along its axis (negative: back),
    /// within its content.
    pub async fn scroll(
        this: Instance<Self>,
        selector: String,
        distance: f64,
    ) -> Result<(), String> {
        step(&this, move |app| app.scroll(&selector, distance as f32)).await
    }

    /// Lets the app run for `ms` milliseconds (timers and animations).
    pub async fn wait(this: Instance<Self>, ms: f64) -> Result<(), String> {
        let time = Duration::from_secs_f64(ms.max(0.0) / 1000.0);
        step(&this, move |app| app.wait(time)).await
    }

    /// Resolves once a view on screen matches `selector`; rejects after
    /// `timeout` milliseconds (5000).
    pub async fn wait_for(
        this: Instance<Self>,
        selector: String,
        timeout: Option<f64>,
    ) -> Result<(), String> {
        let timeout = timeout.map_or(nsh_engine::headless::WAIT, |ms| {
            Duration::from_secs_f64(ms.max(0.0) / 1000.0)
        });
        step(&this, move |app| app.wait_for_within(&selector, timeout)).await
    }

    /// Whether a view on screen matches `selector` now.
    pub fn shows(&self, selector: String) -> Result<bool, String> {
        self.with(|app| app.screen().matches(&selector))
    }

    /// The texts on screen now, in order.
    pub fn texts(&self) -> Result<Vec<String>, String> {
        self.with(|app| {
            Ok(app
                .screen()
                .texts()
                .into_iter()
                .map(str::to_owned)
                .collect())
        })
    }

    /// The text of the input `selector` names, `null` when none matches.
    pub fn value(&self, selector: String) -> Result<Option<String>, String> {
        self.with(|app| Ok(app.screen().input_text(&selector)?.map(str::to_owned)))
    }

    /// What the app's storage holds now.
    pub fn storage(&self) -> Entries {
        Entries(self.storage.entries())
    }

    /// `f` with the app as it is now (the batches that came applied).
    fn with<R>(&self, f: impl FnOnce(&mut Headless) -> Result<R, String>) -> Result<R, String> {
        let mut app = self.app.lock().unwrap_or_else(|e| e.into_inner());
        let app = app.as_mut().ok_or(CLOSED)?;
        app.catch_up();
        f(app)
    }
}

/// Runs the step `f` of the app `this` on a blocking thread.
async fn step(
    this: &Instance<App>,
    f: impl FnOnce(&mut Headless) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    let app = this.with(|a| a.app.clone());
    blocking(move || {
        let mut app = app.lock().unwrap_or_else(|e| e.into_inner());
        f(app.as_mut().ok_or(CLOSED)?)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What a test expects of a value (`expect`); each check throws
/// when it does not hold.
#[class]
pub struct Expectation {
    value: JsonValue,
    not: bool,
}

#[class]
impl Expectation {
    /// The opposite checks: `expect(texts).not.toContain("Error")`.
    #[getter]
    pub fn not(&self) -> Expectation {
        Expectation {
            value: self.value.clone(),
            not: !self.not,
        }
    }

    /// The value is `expected`.
    pub fn to_be(&self, expected: JsonValue) -> Result<(), String> {
        self.check(same(&self.value, &expected), "to be", Some(&expected))
    }

    /// The value equals `expected`, member by member.
    pub fn to_equal(&self, expected: JsonValue) -> Result<(), String> {
        self.check(same(&self.value, &expected), "to equal", Some(&expected))
    }

    /// The value (an array) holds `item`, or (a string) contains it.
    pub fn to_contain(&self, item: JsonValue) -> Result<(), String> {
        let holds = match (&self.value, &item) {
            (JsonValue::Array(items), _) => items.iter().any(|i| same(i, &item)),
            (JsonValue::String(s), JsonValue::String(part)) => s.contains(part.as_str()),
            _ => false,
        };
        self.check(holds, "to contain", Some(&item))
    }

    /// The value is truthy, as JS sees it.
    pub fn to_be_truthy(&self) -> Result<(), String> {
        self.check(truthy(&self.value), "to be truthy", None)
    }

    /// The value is falsy, as JS sees it.
    pub fn to_be_falsy(&self) -> Result<(), String> {
        self.check(!truthy(&self.value), "to be falsy", None)
    }

    fn check(&self, holds: bool, what: &str, expected: Option<&JsonValue>) -> Result<(), String> {
        if holds != self.not {
            return Ok(());
        }
        let not = if self.not { "not " } else { "" };
        Err(match expected {
            Some(expected) => format!("expected {} {not}{what} {expected}", self.value),
            None => format!("expected {} {not}{what}", self.value),
        })
    }
}

/// Whether `a` and `b` are the same JSON value (numbers by value).
fn same(a: &JsonValue, b: &JsonValue) -> bool {
    match (a, b) {
        (JsonValue::Number(a), JsonValue::Number(b)) => a.as_f64() == b.as_f64(),
        (JsonValue::Array(a), JsonValue::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (JsonValue::Object(a), JsonValue::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// JS's truthiness of `value`.
fn truthy(value: &JsonValue) -> bool {
    match value {
        JsonValue::Null => false,
        JsonValue::Bool(b) => *b,
        JsonValue::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        JsonValue::String(s) => !s.is_empty(),
        JsonValue::Array(_) | JsonValue::Object(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn every_export_is_documented() {
        assert_eq!(registry().undocumented(), Vec::<String>::new());
    }

    #[test]
    fn values_compare_as_json_and_checks_say_what_failed() {
        assert!(same(&json!(1), &json!(1.0)));
        assert!(same(&json!({"a": [1, "b"]}), &json!({"a": [1.0, "b"]})));
        assert!(!same(&json!([1]), &json!([1, 2])));
        let texts = Testing::expect(json!(["Buy milk", "1 item left"]));
        assert_eq!(texts.to_contain(json!("1 item left")), Ok(()));
        assert_eq!(
            texts.to_contain(json!("2 items left")),
            Err(r#"expected ["Buy milk","1 item left"] to contain "2 items left""#.into())
        );
        assert_eq!(texts.not().to_contain(json!("Error")), Ok(()));
        assert_eq!(
            texts.not().to_contain(json!("Buy milk")),
            Err(r#"expected ["Buy milk","1 item left"] not to contain "Buy milk""#.into())
        );
        assert_eq!(
            Testing::expect(json!("Saved!")).to_contain(json!("Saved")),
            Ok(())
        );
        assert_eq!(Testing::expect(json!(0)).to_be_falsy(), Ok(()));
        assert_eq!(
            Testing::expect(json!("")).to_be_truthy(),
            Err(r#"expected "" to be truthy"#.into())
        );
    }

    #[test]
    fn responses_read_as_tests_write_them() {
        let r = stub_response(&json!({ "body": { "items": [] } })).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(
            r.headers,
            [("content-type".to_owned(), "application/json".to_owned())]
        );
        assert_eq!(r.body, br#"{"items":[]}"#);
        let r = stub_response(&json!({ "status": 404, "body": "missing" })).unwrap();
        assert_eq!((r.status, r.body.as_slice()), (404, b"missing".as_slice()));
        assert_eq!(
            stub_response(&json!({ "status": 99999 })).unwrap_err().0,
            "`status`: not a status code: 99999"
        );
    }
}
