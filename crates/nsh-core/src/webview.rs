//! A web view's downloads: the files its page saves, and where an app
//! chooses they go.
//!
//! An app that listens for [`crate::EventKind::DownloadPending`]
//! ([`crate::element::WebView::on_download_pending`]) chooses every
//! download's destination itself: the platform defers the download and
//! reports it as a [`WebDownloadPending`], which carries a handle
//! ([`WebDownloadPending::accept`], [`WebDownloadPending::cancel`]). It is
//! the listener's registration that asks for this: without one the
//! platform keeps its automatic behavior (with an `onDownload` listener,
//! the file goes to the app's data directory).
//!
//! A destination is a [`DownloadDestination`]: an absolute path whose
//! parent directory exists. It refuses an existing file unless
//! [`DownloadDestination::overwrite`] says to replace it. The platform
//! writes an owned staging file beside the destination and publishes it
//! only when the transfer finishes, so a failed, cancelled or interrupted
//! download never replaces or corrupts a file at the destination; the
//! platform removes only its own staging file.
//!
//! Accepting and cancelling are requests to the mounted web view
//! ([`crate::Command::WebViewRequest`]), answered with
//! [`crate::EventKind::WebViewReply`]: the answer settles the request, and
//! the transfer's life is reported by [`crate::EventKind::Download`]
//! ([`WebDownload`], the Rust
//! `WebView::on_download`). A pending decision answers an error when the
//! view is gone, the platform has no such controls
//! ([`crate::platform::Environment::webview_requests`]), the download is
//! unknown or already decided, or no answer comes in time
//! ([`REPLY_TIMEOUT`]). The handle is bound to the view, node and download
//! it came from: it cannot reach another view's download, and a clone is
//! the same decision.
//!
//! While an accepted download is under way its handle can cancel it; a
//! completed download's file is the app's and is never removed by the
//! platform.

use std::{fmt, path::PathBuf, rc::Weak, time::Duration};

use crate::{
    EventPayload, JsonValue,
    element::{
        Answer,
        task::{Action, AnswerSlot, Executor},
    },
};

/// Where an accepted download writes.
///
/// The destination's parent directory must exist and its path must be
/// absolute; an existing file is refused unless [`Self::overwrite`] is
/// set. A platform writes an owned sibling staging file and publishes it
/// at the destination when the transfer finishes: without `overwrite` a
/// file that appears meanwhile makes the publish fail without replacing
/// it; failure, cancellation and teardown remove only the staging file,
/// never a file already at the destination.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DownloadDestination {
    /// The file's absolute path; its parent directory must exist.
    pub path: PathBuf,
    /// Replace the file at [`Self::path`] (default: refuse an existing
    /// one). Transfers write an owned sibling staging file and publish it
    /// atomically when they finish, replacing the path only with
    /// `overwrite`; failure, cancellation and teardown delete only that
    /// staging file.
    pub overwrite: bool,
}

impl DownloadDestination {
    /// A destination at `path`, refusing an existing file.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            overwrite: false,
        }
    }

    /// Sets the file's absolute path (the same as [`Self::new`]).
    #[must_use]
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = path.into();
        self
    }

    /// Replaces an existing file at [`Self::path`] when `overwrite` is
    /// true; a file that appears while the download runs is replaced only
    /// then (the default: the publish fails and leaves it untouched).
    #[must_use]
    pub fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }
}

impl From<PathBuf> for DownloadDestination {
    fn from(path: PathBuf) -> Self {
        Self::new(path)
    }
}

/// A download a web view's page asked for, waiting for the app to say
/// where it goes ([`crate::EventKind::DownloadPending`], the Rust
/// `WebView::on_download_pending`).
///
/// The event is the same download to every clone: [`Self::accept`] and
/// [`Self::cancel`] act on the download it was reported for, and never on
/// another view's or another download's. An app can make only the empty
/// [`Default`] value, which names no download: its decisions resolve with
/// an error.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct WebDownloadPending {
    /// The download, for matching its [`WebDownload`] lifecycle.
    pub id: String,
    /// The URL it downloads.
    pub url: String,
    /// The name the page gives it (`Content-Disposition`, or the URL's).
    pub filename: String,
    /// The view, node and download the decision belongs to; `None` for a
    /// value the app made itself ([`Default`]), whose every decision fails.
    pub(crate) handle: Option<DownloadHandle>,
}

impl WebDownloadPending {
    /// Writes the download to `destination` (a [`PathBuf`] refuses an
    /// existing file; use [`DownloadDestination`] for `overwrite`).
    ///
    /// Await the answer in a task: it settles when the destination is
    /// accepted (or refused), not when the transfer finishes
    /// ([`crate::EventKind::Download`] reports that). Nothing is written
    /// before the platform accepts the destination.
    pub fn accept(
        &self,
        destination: impl Into<DownloadDestination>,
    ) -> Answer<Result<(), String>> {
        match &self.handle {
            Some(handle) => handle.accept(destination.into()),
            None => unbound(),
        }
    }

    /// Cancels the download: a pending one writes nothing; an accepted one
    /// stops, and the platform removes only the staging file it wrote.
    /// Await the answer in a task.
    pub fn cancel(&self) -> Answer<Result<(), String>> {
        match &self.handle {
            Some(handle) => handle.cancel(),
            None => unbound(),
        }
    }
}

impl PartialEq for WebDownloadPending {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.url == other.url
            && self.filename == other.filename
            && match (&self.handle, &other.handle) {
                (Some(a), Some(b)) => a.same_as(b),
                (None, None) => true,
                _ => false,
            }
    }
}

impl Eq for WebDownloadPending {}

/// The decision handle of a [`WebDownloadPending`]: the view's node, the
/// executor that reaches it, and the download's id. Only the core makes
/// one (from the event it dispatched to the view's listener), so a
/// decision cannot be forged into another view.
#[derive(Clone, Debug)]
pub(crate) struct DownloadHandle {
    identity: std::rc::Rc<DownloadIdentity>,
}

#[derive(Debug)]
struct DownloadIdentity {
    node: crate::NodeId,
    executor: Weak<Executor>,
    id: String,
}

impl DownloadHandle {
    /// A handle for download `id` of the web view at `node`, reached
    /// through `executor`.
    pub(crate) fn new(node: crate::NodeId, executor: Weak<Executor>, id: String) -> Self {
        Self {
            identity: std::rc::Rc::new(DownloadIdentity { node, executor, id }),
        }
    }

    fn same_as(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.identity, &other.identity)
    }

    /// Sends `operation` to the view and resolves its answer. A view whose
    /// executor is gone (a disposed component) answers at once rather than
    /// waiting.
    /// Accepts a destination for the download this handle was made for.
    fn accept(&self, destination: DownloadDestination) -> Answer<Result<(), String>> {
        self.request(WebViewOperation::AcceptDownload {
            id: self.identity.id.clone(),
            destination,
        })
    }

    /// Cancels the download this handle was made for.
    fn cancel(&self) -> Answer<Result<(), String>> {
        self.request(WebViewOperation::CancelDownload {
            id: self.identity.id.clone(),
        })
    }

    fn request(&self, operation: WebViewOperation) -> Answer<Result<(), String>> {
        let (answer, slot) = Answer::new();
        let Some(executor) = self.identity.executor.upgrade() else {
            deliver_error(slot, NOT_MOUNTED);
            return answer;
        };
        executor.act(Action::WebViewRequest {
            node: self.identity.node,
            operation,
            answer: slot,
        });
        answer
    }
}

/// Delivers `error` to a request's answer (it never reaches a platform).
fn deliver_error(slot: AnswerSlot<Result<(), String>>, error: &str) {
    slot.borrow_mut().deliver(Err(error.to_owned()));
}

/// A decision from a [`WebDownloadPending`] no event made ([`Default`]):
/// it names no download, so it fails rather than guess one.
fn unbound() -> Answer<Result<(), String>> {
    let (answer, slot) = Answer::new();
    deliver_error(slot, UNBOUND);
    answer
}

/// What a web view is asked to do ([`crate::Command::WebViewRequest`]).
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebViewOperation {
    /// Writes the pending download `id` at `destination`.
    AcceptDownload {
        /// The download ([`WebDownloadPending::id`]).
        id: String,
        /// Where it writes.
        destination: DownloadDestination,
    },
    /// Cancels the download `id`: a pending decision writes nothing; an
    /// accepted transfer stops and its staging file is removed.
    CancelDownload {
        /// The download.
        id: String,
    },
}

/// A request on a web view, carrying the core's request id so the
/// platform's reply names it ([`crate::EventKind::WebViewReply`]).
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebViewRequest {
    /// The request's id, assigned by the core.
    pub id: u64,
    /// What to do.
    pub operation: WebViewOperation,
}

/// What an accepted download did ([`crate::EventKind::Download`], the
/// Rust `WebView::on_download`).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebDownload {
    /// Where it is in its life.
    pub state: WebDownloadState,
    /// The download ([`WebDownloadPending::id`] for one the app accepted).
    pub id: String,
    /// The URL it downloads.
    pub url: String,
    /// The file's name.
    pub filename: String,
    /// Where it is written: the app's data directory's
    /// `downloads/<id>/<filename>` for an automatic download, or the
    /// destination the app accepted.
    pub path: PathBuf,
    /// Its size in bytes, once finished.
    pub bytes: Option<u64>,
    /// Why it failed.
    pub message: Option<String>,
}

/// The state of a download ([`WebDownload`]).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WebDownloadState {
    /// It started writing.
    #[default]
    Started,
    /// It finished writing.
    Finished,
    /// It failed or was cancelled.
    Failed,
}

impl WebDownloadState {
    /// The state named `name` in a platform's payload (`"started"`,
    /// `"finished"`, `"failed"`).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "started" => Some(WebDownloadState::Started),
            "finished" => Some(WebDownloadState::Finished),
            "failed" => Some(WebDownloadState::Failed),
            _ => None,
        }
    }
}

impl fmt::Display for WebDownloadState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WebDownloadState::Started => "started",
            WebDownloadState::Finished => "finished",
            WebDownloadState::Failed => "failed",
        })
    }
}

/// Why a request failed when the platform has no web views' controls.
pub(crate) const UNSUPPORTED: &str =
    "this platform has no web view controls (a pending download cannot be answered)";

/// Why a request failed when its element is not a web view.
pub(crate) const NOT_A_WEB_VIEW: &str = "the element is not a web view";

/// Why a request failed when no element carries the ref.
pub(crate) const NOT_MOUNTED: &str = "the web view is not mounted";

/// Why a request failed when the platform never answered.
pub(crate) const NO_ANSWER: &str = "the web view did not answer";

/// Why a decision failed when it did not come from a pending-download
/// event (a [`WebDownloadPending`] made with [`Default`]).
pub(crate) const UNBOUND: &str = "this download decision is not bound to a pending download";

/// How long a request waits for the platform's answer ([`NO_ANSWER`] past
/// it): a mounted web view answers a download decision in moments; one
/// that does not is not worth waiting for, and its answer must not stay
/// pending.
pub(crate) const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// The event payload answering [`WebViewRequest`] `id` with `result`
/// ([`crate::EventKind::WebViewReply`]).
pub fn reply(id: u64, result: Result<JsonValue, String>) -> EventPayload {
    let value = match result {
        Ok(value) => serde_json::json!({ "id": id, "ok": true, "value": value }),
        Err(error) => serde_json::json!({ "id": id, "ok": false, "error": error }),
    };
    EventPayload::Json(value)
}

/// The event payload reporting a download that waits for the app's
/// destination ([`crate::EventKind::DownloadPending`]).
pub fn download_pending_payload(id: &str, url: &str, filename: &str) -> EventPayload {
    EventPayload::Json(serde_json::json!({ "id": id, "url": url, "filename": filename }))
}

/// Checks `operation` before it goes to a platform: a nonempty download
/// id and a destination that can be written ([`check_destination`]).
pub(crate) fn check_operation(operation: &WebViewOperation) -> Result<(), String> {
    match operation {
        WebViewOperation::AcceptDownload { id, destination } => {
            if id.is_empty() {
                return Err("a download needs an id".into());
            }
            check_destination(destination)
        }
        WebViewOperation::CancelDownload { id } => {
            if id.is_empty() {
                Err("a download needs an id".into())
            } else {
                Ok(())
            }
        }
    }
}

/// Checks a download's destination: an absolute path naming a file whose
/// parent is a directory, not an existing entry unless `overwrite` (and
/// never a directory).
pub(crate) fn check_destination(destination: &DownloadDestination) -> Result<(), String> {
    let path = &destination.path;
    if !path.is_absolute() {
        return Err(format!("`{}` is not an absolute path", path.display()));
    }
    if !matches!(path.file_name(), Some(name) if !name.is_empty()) {
        return Err(format!("`{}` does not name a file", path.display()));
    }
    // A directory entry of any kind (a broken symlink too): writing would
    // replace or follow it.
    if path.symlink_metadata().is_ok() {
        if path.is_dir() {
            return Err(format!("`{}` is a directory", path.display()));
        }
        if !destination.overwrite {
            return Err(format!(
                "`{}` exists; pass overwrite to replace it",
                path.display()
            ));
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("`{}` has no directory", path.display()))?;
    if !parent.is_dir() {
        return Err(format!(
            "`{}` is not a directory to write into",
            parent.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destinations_are_absolute_files_in_existing_directories() {
        let dir = std::env::temp_dir();
        let ok = DownloadDestination::new(dir.join("nsh-webview-test-download"));
        let _ = std::fs::remove_file(&ok.path);
        assert!(check_destination(&ok).is_ok());

        let relative = DownloadDestination::new("downloads/a");
        assert!(check_destination(&relative).is_err());

        let no_file = DownloadDestination::new(dir.clone());
        assert!(check_destination(&no_file).is_err());

        let missing = DownloadDestination::new(dir.join("nsh-webview-no-such-dir/a"));
        assert!(check_destination(&missing).is_err());

        let file = dir.join("nsh-webview-test-download");
        std::fs::write(&file, b"x").unwrap();
        assert!(check_destination(&ok).is_err());
        assert!(check_destination(&ok.clone().overwrite(true)).is_ok());
        std::fs::remove_file(&file).unwrap();

        #[cfg(unix)]
        {
            let link = dir.join("nsh-webview-test-download-link");
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(dir.join("nowhere"), &link).unwrap();
            let destination = DownloadDestination::new(link.clone());
            assert!(check_destination(&destination).is_err());
            std::fs::remove_file(&link).unwrap();
        }
    }

    #[test]
    fn operations_are_checked_before_they_are_sent() {
        let destination =
            DownloadDestination::new(std::env::temp_dir().join("nsh-webview-test-operation"));
        let _ = std::fs::remove_file(&destination.path);
        assert!(
            check_operation(&WebViewOperation::AcceptDownload {
                id: "d".into(),
                destination: destination.clone(),
            })
            .is_ok()
        );
        assert!(
            check_operation(&WebViewOperation::AcceptDownload {
                id: String::new(),
                destination,
            })
            .is_err()
        );
        assert!(check_operation(&WebViewOperation::CancelDownload { id: String::new() }).is_err());
        assert!(check_operation(&WebViewOperation::CancelDownload { id: "d".into() }).is_ok());
    }

    #[test]
    fn a_destination_path_defaults_to_refusing_an_existing_file() {
        let destination: DownloadDestination = PathBuf::from("/tmp/a").into();
        assert!(!destination.overwrite);
        assert_eq!(&destination.path, std::path::Path::new("/tmp/a"));
        let built = DownloadDestination::default()
            .path("/tmp/b")
            .overwrite(true);
        assert!(built.overwrite);
        assert_eq!(&built.path, std::path::Path::new("/tmp/b"));
    }

    #[test]
    fn envelopes_carry_the_request_and_its_answer() {
        let ok = reply(7, Ok(JsonValue::Null));
        let EventPayload::Json(value) = ok else {
            panic!("a JSON payload");
        };
        assert_eq!(value["id"], 7);
        assert_eq!(value["ok"], true);
        assert!(value["value"].is_null());
        let EventPayload::Json(value) = reply(7, Err("no".into())) else {
            panic!("a JSON payload");
        };
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"], "no");
        assert!(value.get("value").is_none());

        let EventPayload::Json(value) = download_pending_payload("1", "https://a/b", "b") else {
            panic!("a JSON payload");
        };
        assert_eq!(value["id"], "1");
        assert_eq!(value["url"], "https://a/b");
        assert_eq!(value["filename"], "b");
    }

    #[test]
    fn states_round_trip_their_wire_names() {
        for state in [
            WebDownloadState::Started,
            WebDownloadState::Finished,
            WebDownloadState::Failed,
        ] {
            assert_eq!(WebDownloadState::parse(&state.to_string()), Some(state));
        }
        assert_eq!(WebDownloadState::parse("paused"), None);
    }
}
