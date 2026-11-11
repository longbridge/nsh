//! The method table: `nsh_dev::protocol::TestStep` under `Domain.method` names.
//!
//! A method's params are the step's own serde fields, unchanged (snake_case),
//! so the endpoint hands the app the same step `nsh test --target` sends a
//! host and the result is the step's `StepOutcome` value.

use nsh_dev::protocol::TestStep;
use serde_json::{Map, Value, json};

use crate::protocol::{ERR_BAD_PARAMS, ERR_UNKNOWN_METHOD, ErrorBody};

/// What a method does to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Changes the app (input, environment, windows).
    Action,
    /// Only reads.
    Query,
}

impl Kind {
    /// The name the description gives it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Action => "action",
            Kind::Query => "query",
        }
    }
}

/// The capability that opens the methods reaching what an app keeps private.
pub const PRIVILEGED: &str = "privileged";

/// One method of the protocol.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct MethodSpec {
    /// `Domain.method`.
    pub method: &'static str,
    /// The `TestStep` tag it maps to; a scene query is `scene:<query tag>`.
    pub step: &'static str,
    /// Whether it changes the app.
    pub kind: Kind,
    /// The capability it needs, if any.
    pub capability: Option<&'static str>,
    /// What it does.
    pub doc: &'static str,
}

const fn spec(
    method: &'static str,
    step: &'static str,
    kind: Kind,
    capability: Option<&'static str>,
    doc: &'static str,
) -> MethodSpec {
    MethodSpec {
        method,
        step,
        kind,
        capability,
        doc,
    }
}

use Kind::{Action, Query};

/// Every method, in the order the description lists them.
pub const METHODS: &[MethodSpec] = &[
    spec(
        "Session.ready",
        "ready",
        Action,
        None,
        "Waits for the app's first render, then until it is quiet.",
    ),
    spec("Session.quit", "quit", Action, None, "Quits the app."),
    spec(
        "Input.press",
        "press",
        Action,
        None,
        "Presses the view the selector names.",
    ),
    spec(
        "Input.longPress",
        "long_press",
        Action,
        None,
        "Long-presses the view.",
    ),
    spec(
        "Input.type",
        "type",
        Action,
        None,
        "Types text into the input the selector names.",
    ),
    spec("Input.submit", "submit", Action, None, "Submits the input."),
    spec("Input.clear", "clear", Action, None, "Clears the input."),
    spec(
        "Input.scroll",
        "scroll",
        Action,
        None,
        "Scrolls the view by a distance.",
    ),
    spec(
        "Input.drag",
        "drag",
        Action,
        None,
        "Drags the view by x and y.",
    ),
    spec(
        "Input.hover",
        "hover",
        Action,
        None,
        "Moves the pointer over the view.",
    ),
    spec(
        "Input.rightClick",
        "right_click",
        Action,
        None,
        "Right-clicks the view.",
    ),
    spec(
        "Input.paste",
        "paste",
        Action,
        None,
        "Pastes text, HTML, files or URIs at the view.",
    ),
    spec(
        "Input.pressKey",
        "press_key",
        Action,
        None,
        "Presses a key chord.",
    ),
    spec(
        "Input.holdKeys",
        "hold_keys",
        Action,
        None,
        "Holds or releases keys.",
    ),
    spec(
        "Input.pointer",
        "pointer",
        Action,
        None,
        "Takes a sequence of pointer actions (down, move, up, pause) at points of the window.",
    ),
    spec(
        "Input.key",
        "key",
        Action,
        None,
        "Takes a sequence of key actions (down, up, pause) by `nsh/keymap` names.",
    ),
    spec(
        "Input.dragAndHold",
        "drag_and_hold",
        Action,
        None,
        "Drags to a point and holds there.",
    ),
    spec(
        "Input.drop",
        "drop",
        Action,
        None,
        "Drops what a drag holds.",
    ),
    spec(
        "Input.dropFiles",
        "drop_files",
        Action,
        None,
        "Drops files at the view.",
    ),
    spec(
        "Input.back",
        "back",
        Action,
        None,
        "Goes back, as the system's back does.",
    ),
    spec(
        "Environment.menu",
        "menu",
        Query,
        None,
        "The open menu's items.",
    ),
    spec(
        "Environment.chooseMenuItem",
        "choose_menu_item",
        Action,
        None,
        "Chooses an item of the open menu.",
    ),
    spec(
        "Environment.closeMenu",
        "close_menu",
        Action,
        None,
        "Closes the open menu.",
    ),
    spec(
        "Environment.clipboard",
        "clipboard",
        Query,
        None,
        "The text on the clipboard.",
    ),
    spec(
        "Environment.openedUrls",
        "opened_urls",
        Query,
        None,
        "The URLs the app opened outside itself.",
    ),
    spec(
        "Environment.answered",
        "scene:answered",
        Query,
        None,
        "The dialogs and requests answered so far.",
    ),
    spec(
        "Environment.clock",
        "clock",
        Action,
        None,
        "Moves the manual clock.",
    ),
    spec(
        "Environment.setViewport",
        "set_viewport",
        Action,
        None,
        "Sets the viewport size.",
    ),
    spec(
        "Environment.setColorScheme",
        "set_color_scheme",
        Action,
        None,
        "Sets the color scheme.",
    ),
    spec(
        "Environment.dialogs",
        "dialogs",
        Query,
        None,
        "The dialogs on screen.",
    ),
    spec(
        "Environment.pendingDialog",
        "pending_dialog",
        Query,
        None,
        "The dialog waiting for an answer.",
    ),
    spec(
        "Environment.answerDialog",
        "answer_dialog",
        Action,
        None,
        "Answers the pending dialog with a button.",
    ),
    spec(
        "Host.send",
        "host_send",
        Action,
        Some(PRIVILEGED),
        "Sends a message to the app as its embedding app.",
    ),
    spec(
        "Storage.entries",
        "scene:storage",
        Query,
        Some(PRIVILEGED),
        "The app's stored entries.",
    ),
    spec(
        "State.wait",
        "wait",
        Action,
        None,
        "Waits a number of milliseconds.",
    ),
    spec(
        "State.waitFor",
        "wait_for",
        Query,
        None,
        "Waits until the selector's view appears (or goes).",
    ),
    spec(
        "State.nextChange",
        "next_change",
        Query,
        None,
        "Waits for the next change of the app's views.",
    ),
    spec(
        "State.nextFrame",
        "next_frame",
        Action,
        None,
        "Waits for the next frame.",
    ),
    spec(
        "State.activity",
        "scene:activity",
        Query,
        None,
        "Whether the app is still working.",
    ),
    spec("Tree.texts", "texts", Query, None, "The texts on screen."),
    spec(
        "Tree.snapshot",
        "snapshot",
        Query,
        None,
        "The node tree in tree order: ids, kinds, labels, bounds and states.",
    ),
    spec(
        "Tree.query",
        "query",
        Query,
        None,
        "The ids of the views the selector matches, in screen order.",
    ),
    spec(
        "Tree.describe",
        "describe",
        Query,
        None,
        "One view's facts by id; a view gone from the tree is stale, a recycled list cell answers with `recycled: true`.",
    ),
    spec(
        "Tree.count",
        "count",
        Query,
        None,
        "How many views the selector matches.",
    ),
    spec(
        "Tree.shows",
        "shows",
        Query,
        None,
        "Whether the selector's view is shown.",
    ),
    spec(
        "Tree.enabled",
        "enabled",
        Query,
        None,
        "Whether the selector's view is enabled.",
    ),
    spec(
        "Tree.value",
        "value",
        Query,
        None,
        "The value of the selector's view.",
    ),
    spec(
        "Tree.spanStyle",
        "span_style",
        Query,
        None,
        "The resolved style of a character.",
    ),
    spec(
        "Capture.screenshot",
        "screenshot",
        Query,
        None,
        "A screenshot of the app's window; with `node`, cropped to that view.",
    ),
    spec(
        "Capture.windowScreenshot",
        "window_screenshot",
        Query,
        None,
        "A screenshot of a window by id.",
    ),
    spec(
        "Target.windowState",
        "window_state",
        Query,
        None,
        "The state of a window by id.",
    ),
    spec(
        "Target.pressWindowClose",
        "press_window_close",
        Action,
        None,
        "Presses a window's close button.",
    ),
    spec(
        "Console.takeErrors",
        "scene:take_errors",
        Query,
        None,
        "Takes the errors the app logged since the last call.",
    ),
];

fn spec_of_step(step: &str) -> Option<&'static MethodSpec> {
    METHODS.iter().find(|m| m.step == step)
}

/// The method that carries `step` and its params, or `None` for a step the
/// protocol does not offer.
pub fn method_of(step: &TestStep) -> Option<(&'static str, Value)> {
    let Value::Object(mut fields) = serde_json::to_value(step).ok()? else {
        return None;
    };
    let tag = fields.remove("step")?;
    let tag = tag.as_str()?;
    if tag == "scene" {
        let Value::Object(mut query) = fields.remove("query")? else {
            return None;
        };
        let name = query.remove("query")?;
        let spec = spec_of_step(&format!("scene:{}", name.as_str()?))?;
        return Some((spec.method, Value::Object(query)));
    }
    let spec = spec_of_step(tag)?;
    Some((spec.method, Value::Object(fields)))
}

/// The step `method` stands for, with `params` as its fields.
pub fn step_of(method: &str, params: Value) -> Result<TestStep, ErrorBody> {
    let spec = METHODS
        .iter()
        .find(|m| m.method == method)
        .ok_or_else(|| ErrorBody::new(ERR_UNKNOWN_METHOD, format!("unknown method {method}")))?;
    let mut fields = match params {
        Value::Object(fields) => fields,
        Value::Null => Map::new(),
        _ => return Err(ErrorBody::new(ERR_BAD_PARAMS, "params must be an object")),
    };
    let tagged = match spec.step.strip_prefix("scene:") {
        Some(query) => {
            fields.insert("query".into(), json!(query));
            json!({ "step": "scene", "query": Value::Object(fields) })
        }
        None => {
            fields.insert("step".into(), json!(spec.step));
            Value::Object(fields)
        }
    };
    serde_json::from_value(tagged)
        .map_err(|e| ErrorBody::new(ERR_BAD_PARAMS, format!("{method}: {e}")))
}
