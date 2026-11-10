//! Web views' controls beyond loading a page: screenshots, cookies, the
//! inspector and downloads ([`crate::Command::WebViewRequest`]).
//!
//! A request goes to the mounted web view with an id the core assigns
//! ([`WebViewRequest`]); the platform answers with
//! [`crate::EventKind::WebViewReply`] and [`crate::EventPayload::Json`]:
//! `{"id": …, "ok": true, "value": …}` or
//! `{"id": …, "ok": false, "error": "…"}` ([`reply`] builds either). A
//! screenshot's `value` is its PNG in base64; the cookies' is a JSON array
//! of [`WebCookie`]; the other operations answer no value. The core
//! resolves the typed answer of `ElementRef`'s methods with it
//! (`capture_page`, `get_cookies`, ...), which fail rather than wait
//! forever: when the platform has no web views' controls
//! ([`crate::platform::Environment::webview_requests`]), when the element
//! is not a web view, when it unmounts, and when no answer comes in time
//! ([`REPLY_TIMEOUT`]). The core accepts a reply only from the node the
//! request went to, and drops answers to requests it no longer waits for
//! (a timeout's, a duplicate's).
//!
//! [`crate::EventKind::WebViewProgress`] reports how far a page has loaded
//! ([`WebProgress`]) and [`crate::EventKind::DownloadPending`] a download
//! that waits for the app to accept where the bytes go
//! ([`WebDownloadPending`], [`WebViewOperation::AcceptDownload`]).
//! [`crate::EventKind::Download`] keeps reporting what an accepted
//! download does ([`WebDownload`]).

use std::{fmt, path::PathBuf, time::Duration};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{EventPayload, JsonValue, geometry::Percentage};

/// A cookie of a web view's pages: what
/// [`WebViewOperation::GetCookies`] returns and
/// [`WebViewOperation::SetCookies`] takes.
///
/// `expires` is in Unix milliseconds (each platform converts to and from
/// its own reference date); `None` is a session cookie. `domain` and `name`
/// are required and checked by the core before a cookie is set
/// ([`WebCookie`]'s fields go through [`check_cookies`]).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebCookie {
    /// The name.
    pub name: String,
    /// The value.
    pub value: String,
    /// The domain it is sent to.
    pub domain: String,
    /// The path it is sent with.
    #[serde(default)]
    pub path: String,
    /// When it expires (Unix milliseconds); `None` for a session cookie.
    #[serde(default)]
    pub expires: Option<i64>,
    /// Not readable by the page's scripts.
    #[serde(default)]
    pub http_only: bool,
    /// Only sent over HTTPS.
    #[serde(default)]
    pub secure: bool,
    /// `None`, `Lax` or `Strict`; `None` without the attribute.
    #[serde(default)]
    pub same_site: Option<CookieSameSite>,
}

/// A cookie's `SameSite` attribute.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CookieSameSite {
    /// Sent with cross-site requests too.
    None,
    /// Sent with cross-site navigations that are not of a third site's
    /// subresource.
    Lax,
    /// Sent with same-site requests only.
    Strict,
}

impl CookieSameSite {
    /// Its name in a cookie's reply (`"none"`, `"lax"`, `"strict"`).
    pub fn as_str(self) -> &'static str {
        match self {
            CookieSameSite::None => "none",
            CookieSameSite::Lax => "lax",
            CookieSameSite::Strict => "strict",
        }
    }

    /// The value named `name` (case-insensitive), or `None` for one the
    /// core does not know.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "none" => Some(CookieSameSite::None),
            "lax" => Some(CookieSameSite::Lax),
            "strict" => Some(CookieSameSite::Strict),
            _ => None,
        }
    }
}

impl Serialize for CookieSameSite {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Accepts the lowercase form and any casing a platform reports.
impl<'de> Deserialize<'de> for CookieSameSite {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        CookieSameSite::parse(&name)
            .ok_or_else(|| de::Error::custom(format!("unknown SameSite value `{name}`")))
    }
}

/// What a web view is asked to do ([`crate::Command::WebViewRequest`]).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq)]
pub enum WebViewOperation {
    /// A PNG of the page as it shows, in base64
    /// ([`crate::element::ElementRef::capture_page`]).
    #[default]
    CapturePage,
    /// The cookies of `uri`, or every cookie in the store of the web
    /// view's own profile when omitted ([`crate::element::ElementRef::get_cookies`]).
    GetCookies {
        /// A page URL to filter by; `None` reads the view's store.
        uri: Option<String>,
    },
    /// Sets `cookies` in the web view's cookie store
    /// ([`crate::element::ElementRef::set_cookies`]).
    SetCookies {
        /// The cookies to set (each by name, domain and path).
        cookies: Vec<WebCookie>,
    },
    /// Opens the web inspector
    /// ([`crate::element::ElementRef::open_devtools`]): only where the
    /// platform has a public API for it; others answer an error.
    OpenDevtools,
    /// Lets the user inspect the page
    /// ([`crate::element::ElementRef::set_inspectable`]): off by default.
    SetInspectable {
        /// Whether the inspector may be opened.
        enabled: bool,
    },
    /// Lets the download `id` write to `path`
    /// ([`crate::element::ElementRef::accept_download`]): the app's
    /// choice, after [`WebDownloadPending`]. Nothing is written before.
    AcceptDownload {
        /// The download ([`WebDownloadPending::id`]).
        id: String,
        /// Where to write its bytes: an absolute path.
        path: PathBuf,
        /// Replace an existing file at `path` (default: refuse). The
        /// platform writes an owned sibling staging file and publishes it
        /// atomically when the transfer finishes: without `overwrite` a
        /// file that appears meanwhile makes the publish fail without
        /// replacing it; failure, cancellation and teardown remove only
        /// the staging file, never an existing file at `path`.
        overwrite: bool,
    },
    /// Cancels the download `id` and discards what it downloaded
    /// ([`crate::element::ElementRef::cancel_download`]).
    CancelDownload {
        /// The download ([`WebDownloadPending::id`]).
        id: String,
    },
}

/// A request on the web view carrying an element ref: its `id` comes back
/// in the platform's reply ([`crate::EventKind::WebViewReply`]).
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

/// A download a web view's page asked for, waiting for the app to accept
/// where the bytes go ([`crate::EventKind::DownloadPending`]).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebDownloadPending {
    /// The download, for accepting or cancelling it
    /// ([`crate::element::ElementRef::accept_download`]).
    pub id: String,
    /// The URL it downloads.
    pub url: String,
    /// The name the page gives it (`Content-Disposition`, or the URL's).
    pub filename: String,
}

/// Where an accepted download writes ([`crate::element::ElementRef::accept_download`]).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebDownloadDestination {
    /// The download ([`WebDownloadPending::id`]).
    pub id: String,
    /// The file's absolute path; its parent directory must exist (the
    /// core refuses relative paths, missing parents and an existing file
    /// unless `overwrite`).
    pub path: PathBuf,
    /// Replace the file at `path` (default: refuse an existing one).
    /// Transfers write an owned sibling staging file and publish it
    /// atomically when they finish, replacing `path` only with
    /// `overwrite`; failure, cancellation and teardown delete only that
    /// staging file, never a file already at `path`.
    pub overwrite: bool,
}

/// What an accepted download did
/// ([`crate::element::EventKind::Download`], the Rust
/// [`WebView::on_download`](crate::element::WebView::on_download)).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebDownload {
    /// Where it is in its life.
    pub state: WebDownloadState,
    /// The download ([`WebDownloadPending::id`] for a manual one).
    pub id: String,
    /// The URL it downloads.
    pub url: String,
    /// The file's name.
    pub filename: String,
    /// Where it is written: the app's data directory's `downloads/<id>`,
    /// or the destination the app accepted.
    pub path: PathBuf,
    /// Its size in bytes, once finished.
    pub bytes: Option<u64>,
    /// Why it failed.
    pub message: Option<String>,
}

/// The state of a download ([`WebDownload`]).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

/// How a web view treats a page's downloads ([`crate::mount::WebProps::download_behavior`],
/// Rust [`WebView::download_behavior`](crate::element::WebView::download_behavior)).
/// Read when the download starts.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WebDownloadBehavior {
    /// The platform's behavior with an `onDownload` listener: the file is
    /// written to the app's data directory as it downloads. Without a
    /// listener, the platform's behavior without one.
    #[default]
    Automatic,
    /// Nothing is written until the app accepts a destination for the
    /// [`WebDownloadPending`] it is told of: with no listener, the
    /// download is cancelled.
    Manual,
}

/// Why a request failed when the platform has no web views' controls.
pub(crate) const UNSUPPORTED: &str =
    "this platform has no web view controls (screenshots, cookies, dev tools or downloads)";

/// Why a request failed when its element is not a web view.
pub(crate) const NOT_A_WEB_VIEW: &str = "the element is not a web view";

/// Why a request failed when no element carries the ref.
pub(crate) const NOT_MOUNTED: &str = "the web view is not mounted";

/// Why a request failed when the platform never answered.
pub(crate) const NO_ANSWER: &str = "the web view did not answer";

/// How long a request waits for the platform's answer
/// ([`NO_ANSWER`] past it): a mounted web view answers a screenshot, a
/// cookie store or a download in moments; one that does not is not worth
/// waiting for, and its answer must not stay pending.
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

/// The event payload reporting how far a page loaded
/// ([`crate::EventKind::WebViewProgress`]): `progress` is a fraction from
/// 0 to 1, or `None` for one the platform cannot tell.
pub fn progress_payload(progress: Option<Percentage>) -> EventPayload {
    EventPayload::Json(serde_json::json!({ "progress": progress.map(|value| value.0) }))
}

/// The event payload reporting a download that waits for the app's
/// destination ([`crate::EventKind::DownloadPending`]).
pub fn download_pending_payload(id: &str, url: &str, filename: &str) -> EventPayload {
    EventPayload::Json(serde_json::json!({ "id": id, "url": url, "filename": filename }))
}

/// The PNG a [`WebViewOperation::CapturePage`] reply carries: its base64.
pub(crate) fn capture_png(value: Option<&JsonValue>) -> Result<Vec<u8>, String> {
    let Some(JsonValue::String(data)) = value else {
        return Err("the web view returned no image".into());
    };
    crate::image::base64(data).map_err(|_| "the web view returned a malformed image".to_string())
}

/// The cookies a [`WebViewOperation::GetCookies`] reply carries. What a
/// store holds is returned as it is (a store may keep values the core
/// would refuse to set); only a malformed reply fails.
pub(crate) fn cookies(value: Option<&JsonValue>) -> Result<Vec<WebCookie>, String> {
    let value = value.cloned().unwrap_or(JsonValue::Null);
    serde_json::from_value(value)
        .map_err(|error| format!("the web view returned malformed cookies: {error}"))
}

/// Checks `operation` before it goes to a platform: a filter URL that
/// parses as HTTP(S), cookies the store accepts ([`check_cookies`]) and a
/// destination that can be written ([`check_destination`]). Every
/// [`WebViewOperation::SetCookies`] is checked before any is set.
pub(crate) fn check_operation(operation: &WebViewOperation) -> Result<(), String> {
    match operation {
        WebViewOperation::GetCookies { uri: Some(uri) } => check_url_filter(uri),
        WebViewOperation::GetCookies { uri: None } | WebViewOperation::CapturePage => Ok(()),
        WebViewOperation::SetCookies { cookies } => check_cookies(cookies),
        WebViewOperation::OpenDevtools | WebViewOperation::SetInspectable { .. } => Ok(()),
        WebViewOperation::AcceptDownload {
            id,
            path,
            overwrite,
        } => check_destination(&WebDownloadDestination {
            id: id.clone(),
            path: path.clone(),
            overwrite: *overwrite,
        }),
        WebViewOperation::CancelDownload { id } => {
            if id.is_empty() {
                Err("a download needs an id".into())
            } else {
                Ok(())
            }
        }
    }
}

/// A page URL to filter cookies by: HTTP or HTTPS, with a host.
pub(crate) fn check_url_filter(uri: &str) -> Result<(), String> {
    let url =
        url::Url::parse(uri).map_err(|_| format!("`{uri}` is not a URL to read cookies of"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err(format!(
            "`{uri}` is not an HTTP or HTTPS URL to read cookies of"
        ));
    }
    Ok(())
}

/// A cookie's name: the RFC 6265 token characters (what a `Set-Cookie`
/// header's name may be, so a name cannot add an attribute).
fn cookie_name_char(c: char) -> bool {
    c.is_ascii()
        && !c.is_ascii_control()
        && !matches!(
            c,
            '(' | ')'
                | '<'
                | '>'
                | '@'
                | ','
                | ';'
                | ':'
                | '\\'
                | '"'
                | '/'
                | '['
                | ']'
                | '?'
                | '='
                | '{'
                | '}'
                | ' '
                | '\t'
        )
}

/// A cookie's value: no control characters and no `;` (which would add
/// attributes to a `Set-Cookie` header).
fn cookie_value_char(c: char) -> bool {
    !c.is_control() && c != ';'
}

/// A cookie's domain: what a host name or `.domain` may hold.
fn cookie_domain_char(c: char) -> bool {
    !c.is_control() && !c.is_whitespace() && !matches!(c, ';' | ',' | '=' | '"' | '\\' | '/' | '@')
}

/// Checks every cookie of a [`WebViewOperation::SetCookies`] before any is
/// set: a name and a domain that are not empty, no control characters or
/// attribute delimiters, and a path a cookie can carry.
pub(crate) fn check_cookies(cookies: &[WebCookie]) -> Result<(), String> {
    for cookie in cookies {
        if cookie.name.is_empty() {
            return Err("a cookie needs a name".into());
        }
        if !cookie.name.chars().all(cookie_name_char) {
            return Err(format!(
                "`{}` is not a valid cookie name (control characters and `()<>@,;:\\\"/[]?={{}}` are not allowed)",
                cookie.name
            ));
        }
        if cookie.domain.is_empty() {
            return Err(format!("the cookie `{}` needs a domain", cookie.name));
        }
        if !cookie.domain.chars().all(cookie_domain_char) {
            return Err(format!(
                "`{}` is not a valid domain for the cookie `{}`",
                cookie.domain, cookie.name
            ));
        }
        if !cookie.value.chars().all(cookie_value_char) {
            return Err(format!(
                "the cookie `{}` holds control characters or `;` in its value",
                cookie.name
            ));
        }
        if !cookie.path.chars().all(cookie_value_char) {
            return Err(format!(
                "the cookie `{}` holds control characters or `;` in its path",
                cookie.name
            ));
        }
    }
    Ok(())
}

/// Checks a download's destination: an absolute path with a file name
/// whose parent is a directory, not an existing file unless `overwrite`
/// (and never a directory).
pub(crate) fn check_destination(destination: &WebDownloadDestination) -> Result<(), String> {
    if destination.id.is_empty() {
        return Err("a download needs an id".into());
    }
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

    fn cookie(name: &str, domain: &str) -> WebCookie {
        WebCookie {
            name: name.into(),
            value: "v".into(),
            domain: domain.into(),
            path: "/".into(),
            ..WebCookie::default()
        }
    }

    #[test]
    fn decodes_capture_and_cookies() {
        let png = JsonValue::String("aGk=".into());
        assert_eq!(capture_png(Some(&png)).unwrap(), b"hi");
        assert!(capture_png(None).is_err());
        assert!(capture_png(Some(&JsonValue::Null)).is_err());
        assert!(capture_png(Some(&JsonValue::String("!!".into()))).is_err());

        let value = serde_json::json!([
            {
                "name": "session",
                "value": "abc",
                "domain": "example.com",
                "path": "/",
                "expires": 1_893_456_000_000_i64,
                "http_only": true,
                "secure": true,
                "same_site": "Lax"
            }
        ]);
        let decoded = cookies(Some(&value)).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].name, "session");
        assert_eq!(decoded[0].expires, Some(1_893_456_000_000));
        assert_eq!(decoded[0].same_site, Some(CookieSameSite::Lax));

        // Missing optional attributes: a session cookie without SameSite.
        let value = serde_json::json!([{
            "name": "a", "value": "b", "domain": "d", "path": "/",
            "http_only": false, "secure": false
        }]);
        let decoded = cookies(Some(&value)).unwrap();
        assert_eq!(decoded[0].expires, None);
        assert_eq!(decoded[0].same_site, None);

        // A malformed value, and a cookie the core would refuse to set.
        assert!(cookies(Some(&serde_json::json!({ "name": "a" }))).is_err());
        assert!(cookies(None).is_err());
        assert!(cookies(Some(&serde_json::json!([{ "value": "b", "domain": "d" }]))).is_err());
        let value = serde_json::json!([{
            "name": "a", "value": "b", "domain": "d", "same_site": "sometimes"
        }]);
        assert!(cookies(Some(&value)).is_err());
    }

    #[test]
    fn cookies_round_trip_in_milliseconds_and_lowercase_same_site() {
        let cookie = WebCookie {
            name: "a".into(),
            value: "b c".into(),
            domain: "example.com".into(),
            path: "/x".into(),
            expires: Some(42),
            http_only: true,
            secure: true,
            same_site: Some(CookieSameSite::Strict),
        };
        let json = serde_json::to_value(&cookie).unwrap();
        assert_eq!(json["expires"], 42);
        assert_eq!(json["same_site"], "strict");
        assert_eq!(serde_json::from_value::<WebCookie>(json).unwrap(), cookie);
        assert_eq!(CookieSameSite::None.as_str(), "none");
        assert_eq!(CookieSameSite::parse("LAX"), Some(CookieSameSite::Lax));
        assert_eq!(CookieSameSite::parse("sometimes"), None);
    }

    #[test]
    fn cookies_are_checked_before_any_is_set() {
        assert!(check_cookies(&[cookie("a", "example.com")]).is_ok());
        // An empty list is nothing to set, not an error.
        assert!(check_cookies(&[]).is_ok());
        for bad in [
            "a;b",
            "a b",
            "a,b",
            "a=b",
            "a\r\nSet-Cookie: x=y",
            "",
            "a\tb",
        ] {
            assert!(
                check_cookies(&[cookie(bad, "example.com")]).is_err(),
                "{bad:?}"
            );
        }
        assert!(check_cookies(&[cookie("a", "")]).is_err());
        for bad in ["exa mple.com", "a;b", "a\r\nb", "a/b", "a@b"] {
            assert!(check_cookies(&[cookie("a", bad)]).is_err(), "{bad:?}");
        }
        let mut with_value = cookie("a", "example.com");
        with_value.value = "x; Path=/evil".into();
        assert!(check_cookies(&[with_value]).is_err());
        let mut with_path = cookie("a", "example.com");
        with_path.path = "/x; Secure".into();
        assert!(check_cookies(&[with_path]).is_err());
        // One bad cookie refuses the whole list.
        assert!(check_cookies(&[cookie("a", "example.com"), cookie("", "example.com")]).is_err());
    }

    #[test]
    fn url_filters_must_be_http_urls_with_a_host() {
        assert!(check_url_filter("https://example.com/app").is_ok());
        assert!(check_url_filter("http://example.com").is_ok());
        assert!(check_url_filter("ftp://example.com").is_err());
        assert!(check_url_filter("file:///etc/passwd").is_err());
        assert!(check_url_filter("not a url").is_err());
        assert!(check_url_filter("https://").is_err());
    }

    #[test]
    fn destinations_are_absolute_files_in_existing_directories() {
        let dir = std::env::temp_dir();
        let ok = WebDownloadDestination {
            id: "d".into(),
            path: dir.join("nsh-webview-test-download"),
            overwrite: false,
        };
        let _ = std::fs::remove_file(&ok.path);
        assert!(check_destination(&ok).is_ok());

        let relative = WebDownloadDestination {
            path: PathBuf::from("downloads/a"),
            ..ok.clone()
        };
        assert!(check_destination(&relative).is_err());

        let no_id = WebDownloadDestination {
            id: String::new(),
            ..ok.clone()
        };
        assert!(check_destination(&no_id).is_err());

        let missing = WebDownloadDestination {
            path: dir.join("nsh-webview-no-such-dir/a"),
            ..ok.clone()
        };
        assert!(check_destination(&missing).is_err());

        let directory = WebDownloadDestination {
            path: dir.clone(),
            overwrite: true,
            ..ok.clone()
        };
        assert!(check_destination(&directory).is_err());

        // An existing file: refused unless overwrite (a broken symlink is
        // an entry too).
        let file = dir.join("nsh-webview-test-download");
        std::fs::write(&file, b"x").unwrap();
        assert!(check_destination(&ok).is_err());
        assert!(
            check_destination(&WebDownloadDestination {
                overwrite: true,
                ..ok.clone()
            })
            .is_ok()
        );
        std::fs::remove_file(&file).unwrap();

        #[cfg(unix)]
        {
            let link = dir.join("nsh-webview-test-download-link");
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(dir.join("nowhere"), &link).unwrap();
            let destination = WebDownloadDestination {
                id: "d".into(),
                path: link.clone(),
                overwrite: false,
            };
            assert!(check_destination(&destination).is_err());
            std::fs::remove_file(&link).unwrap();
        }
    }

    #[test]
    fn operations_are_checked_before_they_are_sent() {
        assert!(check_operation(&WebViewOperation::CapturePage).is_ok());
        assert!(check_operation(&WebViewOperation::GetCookies { uri: None }).is_ok());
        assert!(
            check_operation(&WebViewOperation::GetCookies {
                uri: Some("https://example.com".into())
            })
            .is_ok()
        );
        assert!(
            check_operation(&WebViewOperation::GetCookies {
                uri: Some("nonsense".into())
            })
            .is_err()
        );
        assert!(check_operation(&WebViewOperation::SetCookies { cookies: vec![] }).is_ok());
        let destination = WebDownloadDestination {
            id: "d".into(),
            path: std::env::temp_dir().join("nsh-webview-test-operation"),
            overwrite: false,
        };
        let _ = std::fs::remove_file(&destination.path);
        assert!(
            check_operation(&WebViewOperation::AcceptDownload {
                id: destination.id.clone(),
                path: destination.path.clone(),
                overwrite: false,
            })
            .is_ok()
        );
        assert!(check_operation(&WebViewOperation::CancelDownload { id: String::new() }).is_err());
        assert!(check_operation(&WebViewOperation::CancelDownload { id: "d".into() }).is_ok());
    }

    #[test]
    fn envelopes_carry_the_request_and_its_answer() {
        let ok = reply(7, Ok(JsonValue::String("aGk=".into())));
        let EventPayload::Json(value) = ok else {
            panic!("a JSON payload");
        };
        assert_eq!(value["id"], 7);
        assert_eq!(value["ok"], true);
        assert_eq!(value["value"], "aGk=");
        let EventPayload::Json(value) = reply(7, Err("no".into())) else {
            panic!("a JSON payload");
        };
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"], "no");
        assert!(value.get("value").is_none());

        let EventPayload::Json(value) = progress_payload(Some(Percentage(0.5))) else {
            panic!("a JSON payload");
        };
        assert_eq!(value["progress"], 0.5);
        let EventPayload::Json(value) = progress_payload(None) else {
            panic!("a JSON payload");
        };
        assert!(value["progress"].is_null());

        let EventPayload::Json(value) = download_pending_payload("1", "https://a/b", "b") else {
            panic!("a JSON payload");
        };
        assert_eq!(value["id"], "1");
        assert_eq!(value["url"], "https://a/b");
        assert_eq!(value["filename"], "b");
    }
}
