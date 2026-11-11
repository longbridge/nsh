//! The envelope: requests, responses and events, JSON text frames.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The caller's token is missing or wrong.
pub const ERR_UNAUTHORIZED: &str = "ERR_DEVTOOLS_UNAUTHORIZED";
/// No method of that name.
pub const ERR_UNKNOWN_METHOD: &str = "ERR_DEVTOOLS_UNKNOWN_METHOD";
/// The method's params do not decode.
pub const ERR_BAD_PARAMS: &str = "ERR_DEVTOOLS_BAD_PARAMS";
/// Another client holds the input.
pub const ERR_INPUT_LOCKED: &str = "ERR_DEVTOOLS_INPUT_LOCKED";
/// The app's step failed.
pub const ERR_STEP_FAILED: &str = "ERR_DEVTOOLS_STEP_FAILED";
/// The app has not started yet.
pub const ERR_NOT_STARTED: &str = "ERR_DEVTOOLS_NOT_STARTED";
/// The method needs a capability the endpoint was not given.
pub const ERR_CAPABILITY: &str = "ERR_DEVTOOLS_CAPABILITY";

/// The view is no longer in the tree.
pub const ERR_STALE: &str = "ERR_DEVTOOLS_STALE";

/// Every error code, for the description.
pub const ERROR_CODES: &[(&str, &str)] = &[
    (ERR_UNAUTHORIZED, "The token is missing or wrong."),
    (ERR_UNKNOWN_METHOD, "No method of that name."),
    (ERR_BAD_PARAMS, "The method's params do not decode."),
    (ERR_INPUT_LOCKED, "Another client holds the input."),
    (ERR_STEP_FAILED, "The app's step failed."),
    (ERR_NOT_STARTED, "The app has not started yet."),
    (ERR_STALE, "The view is no longer in the tree."),
    (
        ERR_CAPABILITY,
        "The method needs a capability the endpoint was not given.",
    ),
];

/// A call from a client.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Request {
    /// The client's id for it, echoed by the response.
    pub id: u64,
    /// `Domain.method`.
    pub method: String,
    /// The method's params.
    #[serde(default)]
    pub params: Value,
}

impl Request {
    /// A request `id` of `method` with `params`.
    pub fn new(id: u64, method: impl Into<String>, params: Value) -> Request {
        Request {
            id,
            method: method.into(),
            params,
        }
    }
}

/// The answer to a [`Request`]: a result or an error.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Response {
    /// The request's id.
    pub id: u64,
    /// What the method gave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Why it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Response {
    /// The success answer to request `id`.
    pub fn ok(id: u64, result: Value) -> Response {
        Response {
            id,
            result: Some(result),
            error: None,
        }
    }

    /// The failure answer to request `id`.
    pub fn err(id: u64, error: ErrorBody) -> Response {
        Response {
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// A message the app sends on its own.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Event {
    /// `Domain.event`.
    pub method: String,
    /// The event's payload.
    #[serde(default)]
    pub params: Value,
}

impl Event {
    /// An event `method` with `params`.
    pub fn new(method: impl Into<String>, params: Value) -> Event {
        Event {
            method: method.into(),
            params,
        }
    }
}

/// A failure: a stable code and a message (messages are not API).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ErrorBody {
    /// One of the `ERR_*` codes.
    pub code: String,
    /// What went wrong.
    pub message: String,
}

impl ErrorBody {
    /// An error `code` with `message`.
    pub fn new(code: &str, message: impl Into<String>) -> ErrorBody {
        ErrorBody {
            code: code.to_string(),
            message: message.into(),
        }
    }
}
