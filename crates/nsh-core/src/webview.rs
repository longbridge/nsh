//! A web view's cookie store ([`crate::element::ElementRef::cookies`],
//! [`crate::element::ElementRef::set_cookie`] and
//! [`crate::element::ElementRef::delete_cookie`]).
//!
//! A request goes to the mounted web view with an id the core assigns
//! ([`WebViewRequest`]); the platform answers with
//! [`crate::EventKind::WebViewReply`] and [`crate::EventPayload::Json`]:
//! `{"id": …, "ok": true, "value": …}` or
//! `{"id": …, "ok": false, "error": "…"}` ([`reply`] builds either). The
//! core resolves the typed answer of `ElementRef`'s methods with it,
//! which fail rather than wait forever: when the platform has no cookie
//! controls ([`crate::platform::Environment::webview_requests`]), when the
//! element is not a web view, when it unmounts, and when no answer comes in
//! time ([`REPLY_TIMEOUT`]). The core accepts a reply only from the node the
//! request went to, and drops answers to requests it no longer waits for
//! (a timeout's, a duplicate's).
//!
//! The methods are Rust host only: the page's own scripts never reach the
//! store through them, and no JS API is added.
//!
//! # Platform support
//!
//! | Operation | macOS | Linux with WebKitGTK | Windows with WebView2 | iOS, Android, web, TUI |
//! |---|---|---|---|---|
//! | `cookies` | `WKHTTPCookieStore` | the view's cookie manager | the view's cookie manager | Unsupported error |
//! | `set_cookie` | `WKHTTPCookieStore` | the view's cookie manager | the view's cookie manager | Unsupported error |
//! | `delete_cookie` | `WKHTTPCookieStore` | the view's cookie manager | the view's cookie manager | Unsupported error |
//!
//! Every operation reads and writes the store of the view it is sent to: the
//! app's shared profile, or the in-memory store of an `incognito` view. A
//! store keeps cookies set by the page too; `cookies` reports them as the
//! store holds them.

use std::{fmt, time::Duration};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{EventPayload, JsonValue};

/// A cookie of a web view's store: what
/// [`WebViewOperation::GetCookies`] returns and
/// [`WebViewOperation::SetCookie`] takes.
///
/// `expires` is in Unix milliseconds (each platform converts to and from
/// its own reference date); `None` is a session cookie. A `domain` starting
/// with `.` is a domain cookie, sent to that domain's subdomains; without
/// the dot it is host-only and sent to that exact host only. `name`,
/// `domain` and `path` are required and checked by the core before a cookie
/// is set; a refused cookie reaches no native store.
#[non_exhaustive]
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebCookie {
    /// The name.
    pub name: String,
    /// The value.
    pub value: String,
    /// The domain it is sent to (a leading `.`: a domain cookie).
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
    fn as_str(self) -> &'static str {
        match self {
            CookieSameSite::None => "none",
            CookieSameSite::Lax => "lax",
            CookieSameSite::Strict => "strict",
        }
    }

    /// The value named `name` (case-insensitive), or `None` for one the
    /// core does not know.
    fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "none" => Some(CookieSameSite::None),
            "lax" => Some(CookieSameSite::Lax),
            "strict" => Some(CookieSameSite::Strict),
            _ => None,
        }
    }
}

impl fmt::Debug for WebCookie {
    /// A cookie's value is never printed: logs and errors must not carry
    /// what the store keeps secret.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebCookie")
            .field("name", &self.name)
            .field("value", &"<redacted>")
            .field("domain", &self.domain)
            .field("path", &self.path)
            .field("expires", &self.expires)
            .field("http_only", &self.http_only)
            .field("secure", &self.secure)
            .field("same_site", &self.same_site)
            .finish()
    }
}

impl fmt::Display for CookieSameSite {
    /// Its name in a cookie's reply (`"none"`, `"lax"`, `"strict"`), as a
    /// `Set-Cookie` header spells it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum WebViewOperation {
    /// The cookies of `url`, or every cookie in the store of the web view's
    /// own profile when omitted ([`crate::element::ElementRef::cookies`]).
    GetCookies {
        /// A page URL to filter by; `None` reads the view's whole store.
        url: Option<String>,
    },
    /// Sets one cookie in the web view's store
    /// ([`crate::element::ElementRef::set_cookie`]).
    SetCookie {
        /// The cookie to set.
        cookie: WebCookie,
    },
    /// Deletes the cookie with this name, domain and path from the web
    /// view's store ([`crate::element::ElementRef::delete_cookie`]).
    DeleteCookie {
        /// The cookie's name.
        name: String,
        /// Its domain, as [`WebCookie::domain`] reports it.
        domain: String,
        /// Its path.
        path: String,
    },
}

impl Default for WebViewOperation {
    fn default() -> Self {
        Self::GetCookies { url: None }
    }
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

/// Why a request failed when the platform has no cookie controls.
pub(crate) const UNSUPPORTED: &str = "this platform has no web view cookie store (macOS, Linux with WebKitGTK and Windows with WebView2 do)";

/// Why a request failed when its element is not a web view.
pub(crate) const NOT_A_WEB_VIEW: &str = "the element is not a web view";

/// Why a request failed when no element carries the ref.
pub(crate) const NOT_MOUNTED: &str = "the web view is not mounted";

/// Why a request failed when the platform never answered.
pub(crate) const NO_ANSWER: &str = "the web view did not answer";

/// How long a request waits for the platform's answer
/// ([`NO_ANSWER`] past it): a mounted web view answers a cookie store in
/// moments; one that does not is not worth waiting for, and its answer must
/// not stay pending.
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

/// The cookies a [`WebViewOperation::GetCookies`] reply carries. What a
/// store holds is returned as it is (a store may keep values the core
/// would refuse to set); only a malformed reply fails.
pub(crate) fn cookies(value: Option<&JsonValue>) -> Result<Vec<WebCookie>, String> {
    let value = value.cloned().unwrap_or(JsonValue::Null);
    serde_json::from_value(value)
        .map_err(|error| format!("the web view returned malformed cookies: {error}"))
}

/// Checks `operation` before it goes to a platform: a filter URL that
/// parses as HTTP(S) with a host, a cookie the store accepts
/// ([`check_cookie`]), or a deletion identity that names one.
pub(crate) fn check_operation(operation: &WebViewOperation) -> Result<(), String> {
    match operation {
        WebViewOperation::GetCookies { url: Some(url) } => check_url_filter(url),
        WebViewOperation::GetCookies { url: None } => Ok(()),
        WebViewOperation::SetCookie { cookie } => check_cookie(cookie),
        WebViewOperation::DeleteCookie { name, domain, path } => check_identity(name, domain, path),
    }
}

/// A page URL to filter cookies by: HTTP or HTTPS, with a host.
pub(crate) fn check_url_filter(url: &str) -> Result<(), String> {
    let url =
        url::Url::parse(url).map_err(|_| format!("`{url}` is not a URL to read cookies of"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err(format!(
            "`{url}` is not an HTTP or HTTPS URL to read cookies of"
        ));
    }
    Ok(())
}

/// Whether `cookie` is sent to `url` (RFC 6265's domain, path and secure
/// checks): a host-only cookie matches the exact host, a domain cookie
/// (its domain starting with `.`) that domain and its subdomains, the
/// path matches by RFC 6265 5.1.4, and a `secure` cookie needs HTTPS.
#[doc(hidden)]
pub fn cookie_applies_to(cookie: &WebCookie, url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if cookie.secure && url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let domain = cookie.domain.trim_end_matches('.');
    let (host_only, domain) = match domain.strip_prefix('.') {
        Some(domain) => (false, domain),
        None => (true, domain),
    };
    let domain = domain.to_ascii_lowercase();
    if domain.is_empty() {
        return false;
    }
    let domain_matches = host == domain
        || (!host_only
            && host.len() > domain.len()
            && host.ends_with(&domain)
            && host.as_bytes()[host.len() - domain.len() - 1] == b'.');
    domain_matches && path_matches(&url, cookie_path(cookie))
}

/// Whether a stored cookie is the one a deletion names: its name exactly,
/// its domain (ignoring ASCII case, keeping a leading dot), and its path
/// exactly. A host-only and a domain cookie of the same name and path stay
/// different cookies.
#[doc(hidden)]
pub fn cookie_has_identity(cookie: &WebCookie, name: &str, domain: &str, path: &str) -> bool {
    cookie.name == name && cookie.domain.eq_ignore_ascii_case(domain) && cookie.path == path
}

/// A cookie's path, `/` when it has none.
fn cookie_path(cookie: &WebCookie) -> &str {
    if cookie.path.is_empty() {
        "/"
    } else {
        cookie.path.as_str()
    }
}

/// RFC 6265 5.1.4: the request path equals the cookie path, or the cookie
/// path is a prefix that ends at a path boundary.
fn path_matches(url: &url::Url, cookie_path: &str) -> bool {
    let path = url.path();
    if path == cookie_path {
        return true;
    }
    path.starts_with(cookie_path)
        && (cookie_path.ends_with('/')
            || path.as_bytes().get(cookie_path.len()).copied() == Some(b'/'))
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

/// A cookie's value: RFC 6265's cookie-octet, so it cannot add an attribute
/// to a `Set-Cookie` header and every platform can store it as asked.
fn cookie_value_char(c: char) -> bool {
    c.is_ascii() && !c.is_ascii_control() && !matches!(c, ' ' | '"' | ',' | ';' | '\\')
}

/// A cookie's domain: a hostname or an IP literal, with an optional leading
/// dot for a domain cookie.
fn cookie_domain_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | ':' | '[' | ']' | '%')
}

/// A cookie's path: printable, with no attribute delimiter.
fn cookie_path_char(c: char) -> bool {
    c.is_ascii() && !c.is_ascii_control() && !matches!(c, ' ' | '"' | ',' | ';' | '\\')
}

/// Checks one cookie before it goes to a platform: a name that is a token,
/// a domain and absolute path, a value and path no attribute delimiter can
/// escape, and `SameSite=None` with `Secure` (which browsers send).
pub(crate) fn check_cookie(cookie: &WebCookie) -> Result<(), String> {
    if cookie.name.is_empty() {
        return Err("a cookie needs a name".into());
    }
    if !cookie.name.chars().all(cookie_name_char) {
        return Err(format!(
            "`{}` is not a valid cookie name (control characters and `()<>@,;:\\\"/[]?={{}}` and spaces are not allowed)",
            cookie.name
        ));
    }
    let host = cookie.domain.strip_prefix('.').unwrap_or(&cookie.domain);
    if host.is_empty() {
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
            "the cookie `{}` holds control characters, spaces, `\"`, `,`, `;` or `\\` in its value",
            cookie.name
        ));
    }
    if !cookie.path.starts_with('/') {
        return Err(format!(
            "the cookie `{}` needs an absolute path, such as `/`",
            cookie.name
        ));
    }
    if !cookie.path.chars().all(cookie_path_char) {
        return Err(format!(
            "the cookie `{}` holds control characters, spaces, `\"`, `,`, `;` or `\\` in its path",
            cookie.name
        ));
    }
    if cookie.same_site == Some(CookieSameSite::None) && !cookie.secure {
        return Err(format!(
            "the cookie `{}` asks for SameSite=None without Secure, which browsers do not send",
            cookie.name
        ));
    }
    Ok(())
}

/// Checks a deletion's identity before it goes to a platform: the name is a
/// cookie token, the domain is a hostname or IP literal, and the path is
/// non-empty with no control character. The path is matched as the store
/// reports it, so a cookie a page set with an unusual path can still be
/// deleted.
pub(crate) fn check_identity(name: &str, domain: &str, path: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("a cookie needs a name".into());
    }
    if !name.chars().all(cookie_name_char) {
        return Err(format!(
            "`{name}` is not a valid cookie name (control characters and `()<>@,;:\\\"/[]?={{}}` and spaces are not allowed)"
        ));
    }
    let host = domain.strip_prefix('.').unwrap_or(domain);
    if host.is_empty() {
        return Err(format!("the cookie `{name}` needs a domain"));
    }
    if !domain.chars().all(cookie_domain_char) {
        return Err(format!(
            "`{domain}` is not a valid domain for the cookie `{name}`"
        ));
    }
    if path.is_empty() {
        return Err(format!("the cookie `{name}` needs a path"));
    }
    if path.chars().any(char::is_control) {
        return Err(format!(
            "the cookie `{name}` holds control characters in its path"
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

    fn at(cookie: &WebCookie, url: &str) -> bool {
        cookie_applies_to(cookie, url)
    }

    #[test]
    fn a_url_filter_selects_by_host_domain_path_and_secure() {
        let host = cookie("sid", "example.com");
        assert!(at(&host, "https://example.com/"));
        assert!(at(&host, "https://example.com/app"));
        assert!(!at(&host, "https://www.example.com/"));
        assert!(!at(&host, "https://notexample.com/"));
        assert!(!at(&host, "https://example.com.evil/"));
        assert!(!at(&host, "https://example.org/"));

        // A domain cookie (a leading dot) reaches the domain and its
        // subdomains, never a host that only ends with its name.
        let domain = cookie("sid", ".example.com");
        assert!(at(&domain, "https://example.com/"));
        assert!(at(&domain, "https://www.example.com/"));
        assert!(at(&domain, "https://a.b.example.com/"));
        assert!(!at(&domain, "https://notexample.com/"));
        assert!(!at(&domain, "https://example.com.evil/"));

        // Paths match at a path boundary.
        let mut path = cookie("sid", "example.com");
        path.path = "/app".into();
        assert!(at(&path, "https://example.com/app"));
        assert!(at(&path, "https://example.com/app/"));
        assert!(at(&path, "https://example.com/app/x"));
        assert!(!at(&path, "https://example.com/application"));
        assert!(!at(&path, "https://example.com/apple/x"));
        path.path = "/app/".into();
        assert!(at(&path, "https://example.com/app/"));
        assert!(at(&path, "https://example.com/app/x"));
        assert!(!at(&path, "https://example.com/app"));
        assert!(!at(&path, "https://example.com/application"));

        // Secure cookies only go over HTTPS; a non-secure one over both.
        let mut secure = cookie("sid", "example.com");
        secure.secure = true;
        assert!(at(&secure, "https://example.com/"));
        assert!(!at(&secure, "http://example.com/"));

        // A cookie without a path is the root path.
        let mut root = cookie("sid", "example.com");
        root.path = String::new();
        assert!(at(&root, "https://example.com/anything"));
        assert!(at(&root, "https://example.com/x/y"));
    }

    #[test]
    fn a_deletion_names_one_cookie_identity() {
        let host = cookie("sid", "example.com");
        assert!(cookie_has_identity(&host, "sid", "example.com", "/"));
        assert!(cookie_has_identity(&host, "sid", "EXAMPLE.com", "/"));
        assert!(!cookie_has_identity(&host, "sid", ".example.com", "/"));
        assert!(!cookie_has_identity(&host, "other", "example.com", "/"));
        assert!(!cookie_has_identity(&host, "sid", "example.com", "/app"));
        let domain = cookie("sid", ".example.com");
        assert!(cookie_has_identity(&domain, "sid", ".example.com", "/"));
        assert!(!cookie_has_identity(&domain, "sid", "example.com", "/"));
    }

    #[test]
    fn cookies_are_checked_before_any_native_change() {
        assert!(check_cookie(&cookie("a", "example.com")).is_ok());
        assert!(check_cookie(&cookie("a", ".example.com")).is_ok());
        assert!(check_cookie(&cookie("a", "[::1]")).is_ok());
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
                check_cookie(&cookie(bad, "example.com")).is_err(),
                "{bad:?}"
            );
        }
        assert!(check_cookie(&cookie("a", "")).is_err());
        assert!(check_cookie(&cookie("a", ".")).is_err());
        for bad in ["exa mple.com", "a;b", "a\r\nb", "a/b", "a@b", "例え.jp"] {
            assert!(check_cookie(&cookie("a", bad)).is_err(), "{bad:?}");
        }
        for bad in ["x; Path=/evil", "a b", "a,b", "a\\b", "a\"b", "a\r\nb"] {
            let mut with_value = cookie("a", "example.com");
            with_value.value = bad.into();
            assert!(check_cookie(&with_value).is_err(), "{bad:?}");
        }
        for bad in ["relative", "", "/x; Secure", "/a b", "/a,b", "/a\\b"] {
            let mut with_path = cookie("a", "example.com");
            with_path.path = bad.into();
            assert!(check_cookie(&with_path).is_err(), "{bad:?}");
        }
        let mut none = cookie("a", "example.com");
        none.same_site = Some(CookieSameSite::None);
        assert!(check_cookie(&none).is_err());
        none.secure = true;
        assert!(check_cookie(&none).is_ok());
    }

    #[test]
    fn deletions_are_checked_like_sets() {
        assert!(check_identity("sid", "example.com", "/").is_ok());
        assert!(check_identity("sid", ".example.com", "/app").is_ok());
        // A store may report a path a set would not take; it can still be
        // deleted by that identity.
        assert!(check_identity("sid", "example.com", "relative").is_ok());
        assert!(check_identity("sid", "example.com", "/a b").is_ok());
        assert!(check_identity("", "example.com", "/").is_err());
        assert!(check_identity("sid", "", "/").is_err());
        assert!(check_identity("sid", "example.com", "").is_err());
        assert!(check_identity("sid", "example.com", "/a\nb").is_err());
        assert!(check_identity("a;b", "example.com", "/").is_err());
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
    fn operations_are_checked_before_they_are_sent() {
        assert!(check_operation(&WebViewOperation::GetCookies { url: None }).is_ok());
        assert!(
            check_operation(&WebViewOperation::GetCookies {
                url: Some("https://example.com".into())
            })
            .is_ok()
        );
        assert!(
            check_operation(&WebViewOperation::GetCookies {
                url: Some("nonsense".into())
            })
            .is_err()
        );
        assert!(
            check_operation(&WebViewOperation::SetCookie {
                cookie: cookie("a", "example.com")
            })
            .is_ok()
        );
        assert!(
            check_operation(&WebViewOperation::SetCookie {
                cookie: cookie("", "example.com")
            })
            .is_err()
        );
        assert!(
            check_operation(&WebViewOperation::DeleteCookie {
                name: "a".into(),
                domain: "example.com".into(),
                path: "/".into(),
            })
            .is_ok()
        );
        assert!(
            check_operation(&WebViewOperation::DeleteCookie {
                name: "a".into(),
                domain: "example.com".into(),
                path: String::new(),
            })
            .is_err()
        );
    }
}
