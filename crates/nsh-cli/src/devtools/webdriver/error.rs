//! WebDriver's error answers: `{"value":{"error","message","stacktrace"}}`
//! and the HTTP status each kind has.

use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use nsh_devtools::{
    client::ERR_TIMEOUT,
    protocol::{ERR_BAD_PARAMS, ERR_STALE, ERR_STEP_FAILED, ErrorBody},
};
use serde_json::json;

/// An error answer: WebDriver's error code, a message and the status.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WdError {
    /// WebDriver's error code (`"invalid session id"`).
    pub(crate) error: &'static str,
    /// What went wrong.
    pub(crate) message: String,
    /// The HTTP status.
    pub(crate) status: StatusCode,
}

impl WdError {
    fn new(error: &'static str, status: StatusCode, message: impl Into<String>) -> WdError {
        WdError {
            error,
            message: message.into(),
            status,
        }
    }

    /// A request that does not decode.
    pub(crate) fn invalid_argument(message: impl Into<String>) -> WdError {
        WdError::new("invalid argument", StatusCode::BAD_REQUEST, message)
    }

    /// A session that cannot be created (bad capabilities, the app does not
    /// start, a session is open already).
    pub(crate) fn session_not_created(message: impl Into<String>) -> WdError {
        WdError::new(
            "session not created",
            StatusCode::INTERNAL_SERVER_ERROR,
            message,
        )
    }

    /// A session id the server does not have.
    pub(crate) fn invalid_session_id(id: &str) -> WdError {
        WdError::new(
            "invalid session id",
            StatusCode::NOT_FOUND,
            format!("no session {id}"),
        )
    }

    /// A route the server does not have.
    pub(crate) fn unknown_command(message: impl Into<String>) -> WdError {
        WdError::new("unknown command", StatusCode::NOT_FOUND, message)
    }

    /// A route that does not take the method.
    pub(crate) fn unknown_method(message: impl Into<String>) -> WdError {
        WdError::new("unknown method", StatusCode::METHOD_NOT_ALLOWED, message)
    }

    /// No element matches, or the element id is not one this session knows.
    pub(crate) fn no_such_element(message: impl Into<String>) -> WdError {
        WdError::new("no such element", StatusCode::NOT_FOUND, message)
    }

    /// An element whose view is gone from the tree (or now shows other
    /// content).
    pub(crate) fn stale_element(message: impl Into<String>) -> WdError {
        WdError::new("stale element reference", StatusCode::NOT_FOUND, message)
    }

    /// A locator the server cannot evaluate.
    pub(crate) fn invalid_selector(message: impl Into<String>) -> WdError {
        WdError::new("invalid selector", StatusCode::BAD_REQUEST, message)
    }

    /// A command the server does not do (window resizing, scripts).
    pub(crate) fn unsupported_operation(message: impl Into<String>) -> WdError {
        WdError::new(
            "unsupported operation",
            StatusCode::INTERNAL_SERVER_ERROR,
            message,
        )
    }

    /// An action that points outside the window or at a view without bounds.
    pub(crate) fn move_target_out_of_bounds(message: impl Into<String>) -> WdError {
        WdError::new(
            "move target out of bounds",
            StatusCode::INTERNAL_SERVER_ERROR,
            message,
        )
    }

    /// What does not fit another kind.
    pub(crate) fn unknown_error(message: impl Into<String>) -> WdError {
        WdError::new("unknown error", StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl From<ErrorBody> for WdError {
    fn from(body: ErrorBody) -> WdError {
        let message = body.message;
        // The message embeds the app's selector in backticks: only the
        // fixed words around it say what failed.
        let lower = outside_backticks(&message).to_lowercase();
        match body.code.as_str() {
            ERR_STALE => WdError::new("stale element reference", StatusCode::NOT_FOUND, message),
            ERR_BAD_PARAMS => WdError::invalid_argument(message),
            ERR_TIMEOUT => WdError::new("timeout", StatusCode::INTERNAL_SERVER_ERROR, message),
            // The step's own failures carry their kind in the message.
            ERR_STEP_FAILED if lower.contains("timed out") || lower.contains("timeout") => {
                WdError::new("timeout", StatusCode::INTERNAL_SERVER_ERROR, message)
            }
            ERR_STEP_FAILED if lower.contains("selector") => {
                WdError::new("invalid selector", StatusCode::BAD_REQUEST, message)
            }
            _ => WdError::unknown_error(message),
        }
    }
}

/// `text` without what is between backticks.
fn outside_backticks(text: &str) -> String {
    text.split('`').step_by(2).collect::<Vec<_>>().join(" ")
}

impl IntoResponse for WdError {
    fn into_response(self) -> Response {
        let body = json!({
            "value": { "error": self.error, "message": self.message, "stacktrace": "" }
        });
        (
            self.status,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            body.to_string(),
        )
            .into_response()
    }
}

#[cfg(test)]
mod devtools_webdriver {
    use super::*;

    fn map(code: &str, message: &str) -> (&'static str, StatusCode) {
        let e = WdError::from(ErrorBody::new(code, message));
        (e.error, e.status)
    }

    #[test]
    fn protocol_errors_map_to_webdriver_errors() {
        assert_eq!(
            map(ERR_STALE, "gone"),
            ("stale element reference", StatusCode::NOT_FOUND)
        );
        assert_eq!(
            map(ERR_BAD_PARAMS, "x"),
            ("invalid argument", StatusCode::BAD_REQUEST)
        );
        assert_eq!(
            map(ERR_STEP_FAILED, "`a:b`: unknown selector prefix `a:`"),
            ("invalid selector", StatusCode::BAD_REQUEST)
        );
        assert_eq!(
            map(ERR_STEP_FAILED, "no view matches `label:Timeout settings`"),
            ("unknown error", StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert_eq!(
            map(ERR_STEP_FAILED, "no view matches `id:selector`"),
            ("unknown error", StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert_eq!(
            map(ERR_STEP_FAILED, "wait timed out after 5000 ms"),
            ("timeout", StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert_eq!(
            map(ERR_TIMEOUT, "Input.press timed out"),
            ("timeout", StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert_eq!(
            map("ERR_OTHER", "boom"),
            ("unknown error", StatusCode::INTERNAL_SERVER_ERROR)
        );
    }
}
