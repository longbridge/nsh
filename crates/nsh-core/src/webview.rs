//! Web views' controls beyond loading a page: screenshots and the
//! inspector ([`crate::Command::WebViewRequest`]).
//!
//! A request goes to the mounted web view with an id the core assigns
//! ([`WebViewRequest`]); the platform answers with
//! [`crate::EventKind::WebViewReply`] and [`crate::EventPayload::Json`]:
//! `{"id": …, "ok": true, "value": …}` or
//! `{"id": …, "ok": false, "error": "…"}` ([`reply`] builds either). A
//! screenshot's `value` is its PNG in base64; the other operation answers
//! no value. The core resolves the typed answer of `ElementRef`'s methods
//! with it (`capture_page`, `open_devtools`), which fail rather than wait
//! forever: when the platform has no web views' controls
//! ([`crate::platform::Environment::webview_requests`]), when the element
//! is not a web view, when it unmounts, and when no answer comes in time
//! ([`REPLY_TIMEOUT`]). The core accepts a reply only from the node the
//! request went to, and drops answers to requests it no longer waits for
//! (a timeout's, a duplicate's).
//!
//! [`crate::EventKind::WebViewProgress`] reports how far a page has loaded
//! ([`WebProgress`]).

use std::time::Duration;

use crate::{EventPayload, JsonValue, geometry::Percentage};

/// What a web view is asked to do ([`crate::Command::WebViewRequest`]).
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq)]
pub enum WebViewOperation {
    /// A PNG of the page as it shows, in base64
    /// ([`crate::element::ElementRef::capture_page`]).
    #[default]
    CapturePage,
    /// Opens the web inspector
    /// ([`crate::element::ElementRef::open_devtools`]): only where the
    /// platform has a public API for it; others answer an error.
    OpenDevtools,
}

/// A request on the web view carrying an element ref: its `id` comes back
/// in the platform's reply ([`crate::EventKind::WebViewReply`]).
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WebViewRequest {
    /// The request's id, assigned by the core.
    pub id: u64,
    /// What to do.
    pub operation: WebViewOperation,
}

/// How far a web view's page has loaded
/// ([`crate::EventKind::WebViewProgress`]).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WebProgress {
    /// The fraction loaded (0 to 1), `None` when the platform cannot tell
    /// (a page about to load, or one whose size it does not know).
    pub progress: Option<Percentage>,
}

/// Why a request failed when the platform has no web views' controls.
pub(crate) const UNSUPPORTED: &str =
    "this platform has no web view controls (screenshots or dev tools)";

/// Why a request failed when its element is not a web view.
pub(crate) const NOT_A_WEB_VIEW: &str = "the element is not a web view";

/// Why a request failed when no element carries the ref.
pub(crate) const NOT_MOUNTED: &str = "the web view is not mounted";

/// Why a request failed when the platform never answered.
pub(crate) const NO_ANSWER: &str = "the web view did not answer";

/// How long a request waits for the platform's answer
/// ([`NO_ANSWER`] past it): a mounted web view answers a screenshot or the
/// inspector in moments; one that does not is not worth waiting for, and
/// its answer must not stay pending.
pub(crate) const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// The event payload answering [`WebViewRequest`] `id` with `result`
/// ([`crate::EventKind::WebViewReply`]).
#[doc(hidden)]
pub fn reply(id: u64, result: Result<JsonValue, String>) -> EventPayload {
    let value = match result {
        Ok(value) => serde_json::json!({ "id": id, "ok": true, "value": value }),
        Err(error) => serde_json::json!({ "id": id, "ok": false, "error": error }),
    };
    EventPayload::Json(value)
}

/// The event payload reporting how far a page loaded
/// ([`crate::EventKind::WebViewProgress`]): `progress` is a fraction from
/// 0 to 1, or `None` for one the platform cannot tell.
#[doc(hidden)]
pub fn progress_payload(progress: Option<Percentage>) -> EventPayload {
    EventPayload::Json(serde_json::json!({ "progress": progress.map(|value| value.0) }))
}

/// The PNG a [`WebViewOperation::CapturePage`] reply carries: its base64.
pub(crate) fn capture_png(value: Option<&JsonValue>) -> Result<Vec<u8>, String> {
    let Some(JsonValue::String(data)) = value else {
        return Err("the web view returned no image".into());
    };
    crate::image::base64(data).map_err(|_| "the web view returned a malformed image".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_capture() {
        let png = JsonValue::String("aGk=".into());
        assert_eq!(capture_png(Some(&png)).unwrap(), b"hi");
        assert!(capture_png(None).is_err());
        assert!(capture_png(Some(&JsonValue::Null)).is_err());
        assert!(capture_png(Some(&JsonValue::String("!!".into()))).is_err());
    }
}
