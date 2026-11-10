//! Awaitable JavaScript in an app-owned WebView. The page's result is
//! untrusted JSON, separate from its public message channel. Native hosts
//! poll only returned Promises; synchronous results need one evaluation.

use std::{
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll},
};

use crate::{
    JsonValue, NodeId,
    element::{
        Answer, JsError,
        task::{Action, AnswerSlot, Executor},
    },
};

/// Maximum UTF-8 bytes in a serialized evaluation result (1 MiB).
pub const MAX_RESULT_BYTES: usize = 1_048_576;
/// Maximum evaluations waiting in one engine.
pub const MAX_PENDING: usize = 64;
/// Maximum time an evaluation waits, in milliseconds.
pub const TIMEOUT_MS: u64 = 30_000;

/// The result of evaluating a WebView script. Dropping it cancels the
/// native request; await it on the engine thread, as a ref's `measure`.
#[non_exhaustive]
#[derive(Debug)]
#[must_use = "await the evaluation to receive its result"]
pub struct Evaluation {
    answer: Answer<Result<JsonValue, JsError>>,
    executor: Weak<Executor>,
}

impl Default for Evaluation {
    fn default() -> Self {
        let (answer, slot) = Answer::new();
        slot.borrow_mut().deliver(Err(error(
            "ERR_NSH_WEBVIEW_DISPOSED",
            "the WebView ref is not mounted",
        )));
        Self {
            answer,
            executor: Weak::new(),
        }
    }
}

impl Future for Evaluation {
    type Output = Result<JsonValue, JsError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.answer).poll(cx)
    }
}

impl Drop for Evaluation {
    fn drop(&mut self) {
        if let Some(executor) = self.executor.upgrade() {
            executor.act(Action::ForgetWebEvaluations);
        }
    }
}

pub(crate) fn request(executor: &Rc<Executor>, node: NodeId, script: String) -> Evaluation {
    let (answer, slot) = Answer::new();
    executor.act(Action::EvaluateJavaScript(node, script, slot));
    Evaluation {
        answer,
        executor: Rc::downgrade(executor),
    }
}

pub(crate) fn error(code: &str, message: &str) -> JsError {
    JsError::Error(message.into()).with_code(code)
}

pub(crate) type Slot = AnswerSlot<Result<JsonValue, JsError>>;

/// Decodes a final native reply. Size checks are repeated in Rust: a page
/// can replace its own globals and every result from it remains untrusted.
pub(crate) fn decode(data: &str) -> Result<JsonValue, JsError> {
    let invalid = || {
        error(
            "ERR_NSH_WEBVIEW_RESULT",
            "the WebView returned an invalid result",
        )
    };
    if data.len() > MAX_RESULT_BYTES + 1024 {
        return Err(error(
            "ERR_NSH_QUOTA_EXCEEDED",
            "the WebView result exceeds 1 MiB",
        ));
    }
    let envelope: JsonValue = serde_json::from_str(data).map_err(|_| invalid())?;
    match envelope["status"].as_str() {
        Some("ok") => {
            let value = envelope.get("value").ok_or_else(invalid)?;
            fn bounded(value: &JsonValue, depth: usize) -> bool {
                depth <= 64
                    && match value {
                        JsonValue::Array(values) => values.iter().all(|v| bounded(v, depth + 1)),
                        JsonValue::Object(values) => values.values().all(|v| bounded(v, depth + 1)),
                        _ => true,
                    }
            }
            if !bounded(value, 0) {
                return Err(invalid());
            }
            Ok(value.clone())
        }
        Some("error") => {
            let code = envelope["code"].as_str().ok_or_else(invalid)?;
            if !matches!(
                code,
                "ERR_NSH_WEBVIEW_JAVASCRIPT"
                    | "ERR_NSH_WEBVIEW_RESULT"
                    | "ERR_NSH_WEBVIEW_NAVIGATION"
                    | "ERR_NSH_WEBVIEW_DISPOSED"
                    | "ERR_NSH_WEBVIEW_NATIVE"
                    | "ERR_NSH_QUOTA_EXCEEDED"
                    | "ETIMEDOUT"
            ) {
                return Err(invalid());
            }
            Err(error(
                code,
                envelope["message"]
                    .as_str()
                    .unwrap_or("WebView evaluation failed"),
            ))
        }
        _ => Err(invalid()),
    }
}

/// Starts a native evaluation, returning a JSON envelope string. A Promise
/// stores its eventual envelope under the native host's per-request key.
pub fn start_script(script: &str, key: &str) -> String {
    include_str!("web_evaluation.js")
        .replace("__NSH_KEY__", &serde_json::to_string(key).expect("string"))
        .replace(
            "__NSH_SOURCE__",
            &serde_json::to_string(script).expect("string"),
        )
}

/// Reads a Promise's envelope and removes a settled page cell.
pub fn poll_script(key: &str) -> String {
    let key = serde_json::to_string(key).expect("string");
    format!(
        r#"(() => {{ const key = {key}; const cell = globalThis[key]; if (!cell) return JSON.stringify({{status:"error",code:"ERR_NSH_WEBVIEW_RESULT",message:"evaluation context was lost"}}); if (cell.result === null) return '{{"status":"pending"}}'; delete globalThis[key]; return cell.result; }})()"#
    )
}

/// Releases a cancelled Promise's page cell without adding a message bridge.
pub fn cleanup_script(key: &str) -> String {
    format!(
        "delete globalThis[{}]",
        serde_json::to_string(key).expect("string")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replies_preserve_json_and_distinguish_null_from_failure() {
        for value in [
            serde_json::json!(null),
            serde_json::json!([1, true, {"x":"\n日本語"}]),
            serde_json::json!(42),
        ] {
            let reply = serde_json::json!({"status":"ok", "value":value});
            assert_eq!(decode(&reply.to_string()).unwrap(), value);
        }
        assert_eq!(
            decode(r#"{"status":"error","code":"ERR_NSH_WEBVIEW_JAVASCRIPT","message":"boom"}"#)
                .unwrap_err()
                .code(),
            Some("ERR_NSH_WEBVIEW_JAVASCRIPT")
        );
        for bad in [
            "null",
            "{}",
            r#"{"status":"ok"}"#,
            r#"{"status":"pending"}"#,
        ] {
            assert_eq!(
                decode(bad).unwrap_err().code(),
                Some("ERR_NSH_WEBVIEW_RESULT")
            );
        }
    }
    #[test]
    fn oversized_native_replies_are_rejected() {
        assert_eq!(
            decode(&"x".repeat(MAX_RESULT_BYTES + 1025))
                .unwrap_err()
                .code(),
            Some("ERR_NSH_QUOTA_EXCEEDED")
        );
    }
    #[test]
    fn page_supplied_replies_cannot_bypass_the_nesting_limit() {
        let nested = format!("{}0{}", "[".repeat(65), "]".repeat(65));
        let reply = format!(r#"{{"status":"ok","value":{nested}}}"#);
        assert_eq!(
            decode(&reply).unwrap_err().code(),
            Some("ERR_NSH_WEBVIEW_RESULT")
        );
    }
}
