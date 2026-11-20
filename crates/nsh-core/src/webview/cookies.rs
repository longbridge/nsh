//! WebView cookies: structured cookie items returned by
//! [`ElementRef::get_cookies`](crate::element::ElementRef::get_cookies)
//! (TS: `ref.getCookies(url, options)`), URL matching for stores that
//! list all cookies, and parsing for platforms whose store reports a
//! `Cookie` header string.

use std::time::Duration;

use crate::{
    JsonValue,
    element::{JsError, codes},
};

/// Maximum pending cookie requests allowed in one engine at a time.
pub(crate) const MAX_PENDING: usize = 64;
/// Maximum cookies accepted in one reply.
pub const MAX_COOKIES: usize = 4096;

/// A cookie's `SameSite` policy.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebCookieSameSite {
    /// Sent on third-party requests (`SameSite=None`).
    None,
    /// Sent on top-level cross-site navigations (`SameSite=Lax`).
    Lax,
    /// Sent only on same-site requests (`SameSite=Strict`).
    Strict,
}

/// A cookie belonging to a [`crate::WebView`]'s store (TS: `WebViewCookie`).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebCookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Domain attribute when reported by the platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Path attribute when reported by the platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Expiration timestamp in Unix epoch seconds when reported and non-session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<f64>,
    /// Whether the cookie is marked `HttpOnly` when reported by the platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_only: Option<bool>,
    /// Whether the cookie requires HTTPS when reported by the platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure: Option<bool>,
    /// `SameSite` policy when reported by the platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_site: Option<WebCookieSameSite>,
}

impl WebCookie {
    /// Creates a cookie carrying a `name` and `value`, leaving optional
    /// attributes unset.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            ..Self::default()
        }
    }

    /// Sets [`Self::domain`].
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Sets [`Self::path`].
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Sets [`Self::expires`] in Unix epoch seconds.
    pub fn expires(mut self, expires: f64) -> Self {
        self.expires = expires.is_finite().then_some(expires);
        self
    }

    /// Sets [`Self::http_only`].
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = Some(http_only);
        self
    }

    /// Sets [`Self::secure`].
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = Some(secure);
        self
    }

    /// Sets [`Self::same_site`].
    pub fn same_site(mut self, same_site: WebCookieSameSite) -> Self {
        self.same_site = Some(same_site);
        self
    }
}

/// How [`ElementRef::get_cookies`](crate::element::ElementRef::get_cookies)
/// runs (TS: `ref.getCookies`'s options).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CookieOptions {
    /// How long the platform's cookie store may take to answer; past it
    /// the call fails with [`codes::TIMED_OUT`](crate::element::codes::TIMED_OUT).
    /// 30 seconds by default.
    pub timeout: Duration,
}

impl CookieOptions {
    /// Options with the given timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Default for CookieOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
        }
    }
}

/// Parses an `http` or `https` URL into `(is_https, host, path)` per RFC 6265
/// matching. Returns `None` for any other scheme or an empty host.
fn parse_http_url(url: &str) -> Option<(bool, &str, &str)> {
    let url = url.trim();
    let (is_https, rest) = if url.len() >= 8 && url[..8].eq_ignore_ascii_case("https://") {
        (true, &url[8..])
    } else if url.len() >= 7 && url[..7].eq_ignore_ascii_case("http://") {
        (false, &url[7..])
    } else {
        return None;
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return None;
    }
    let after_userinfo = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(after_bracket) = after_userinfo.strip_prefix('[') {
        let close = after_bracket.find(']')?;
        let inner = &after_bracket[..close];
        if inner.is_empty() {
            return None;
        }
        inner
    } else {
        let h = after_userinfo.split(':').next().unwrap_or("");
        if h.is_empty() {
            return None;
        }
        h
    };
    let path_and_query = &rest[authority_end..];
    let path_end = path_and_query
        .find(['?', '#'])
        .unwrap_or(path_and_query.len());
    let path = &path_and_query[..path_end];
    let path = if path.starts_with('/') { path } else { "/" };
    Some((is_https, host, path))
}

/// Returns `true` when `url` is an `http://` or `https://` URL with a
/// non-empty host.
pub fn is_http_url(url: &str) -> bool {
    parse_http_url(url).is_some()
}

/// Returns `true` when `cookie` applies to `url` per RFC 6265 scheme,
/// domain, and path matching.
pub fn matches_url(cookie: &WebCookie, url: &str) -> bool {
    let Some((is_https, req_host, req_path)) = parse_http_url(url) else {
        return false;
    };
    if cookie.secure == Some(true) && !is_https {
        return false;
    }
    if let Some(domain) = cookie.domain.as_deref() {
        let cookie_domain = domain.trim().trim_start_matches('.');
        if cookie_domain.is_empty() {
            return false;
        }
        let req_host = req_host.trim_end_matches('.');
        let cookie_domain = cookie_domain.trim_end_matches('.');
        if !domain_matches(req_host, cookie_domain) {
            return false;
        }
    }
    if let Some(cookie_path) = cookie.path.as_deref() {
        let cookie_path = if cookie_path.is_empty() {
            "/"
        } else {
            cookie_path
        };
        if !path_matches(req_path, cookie_path) {
            return false;
        }
    }
    true
}

fn domain_matches(req_host: &str, cookie_domain: &str) -> bool {
    if req_host.eq_ignore_ascii_case(cookie_domain) {
        return true;
    }
    if req_host.len() <= cookie_domain.len() {
        return false;
    }
    let prefix_len = req_host.len() - cookie_domain.len();
    req_host.as_bytes()[prefix_len - 1] == b'.'
        && req_host[prefix_len..].eq_ignore_ascii_case(cookie_domain)
}

fn path_matches(req_path: &str, cookie_path: &str) -> bool {
    if req_path == cookie_path {
        return true;
    }
    if let Some(rest) = req_path.strip_prefix(cookie_path) {
        return cookie_path.ends_with('/') || rest.starts_with('/');
    }
    false
}

/// Parses a semicolon-separated HTTP `Cookie` header string (such as
/// Android `CookieManager.getCookie(url)`) into [`WebCookie`] entries.
pub fn parse_cookie_header(header: &str) -> Vec<WebCookie> {
    let mut cookies = Vec::new();
    for pair in header.split(';') {
        let trimmed = pair.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((raw_name, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        let name = raw_name.trim();
        if name.is_empty() {
            continue;
        }
        cookies.push(WebCookie::new(name, raw_value.trim()));
        if cookies.len() >= MAX_COOKIES {
            break;
        }
    }
    cookies
}

/// Decodes a platform's [`EventKind::WebViewReply`](crate::EventKind::WebViewReply)
/// `value` into a list of [`WebCookie`]s.
pub fn decode_reply(value: &JsonValue) -> Result<Vec<WebCookie>, JsError> {
    let items = value.as_array().ok_or_else(|| {
        JsError::Error("the WebView returned an invalid cookie list".into())
            .with_code(codes::WEBVIEW_ABORTED)
    })?;
    if items.len() > MAX_COOKIES {
        return Err(
            JsError::Error(format!("WebView returned more than {MAX_COOKIES} cookies"))
                .with_code(codes::QUOTA_EXCEEDED),
        );
    }
    let mut cookies = Vec::with_capacity(items.len());
    for item in items {
        let mut cookie: WebCookie = serde_json::from_value(item.clone()).map_err(|_| {
            JsError::Error("the WebView returned an invalid cookie entry".into())
                .with_code(codes::WEBVIEW_ABORTED)
        })?;
        if let Some(exp) = cookie.expires
            && !exp.is_finite()
        {
            cookie.expires = None;
        }
        cookies.push(cookie);
    }
    Ok(cookies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_http_and_https_urls_only() {
        assert!(is_http_url("https://example.com"));
        assert!(is_http_url("http://127.0.0.1:8080/login?a=1#frag"));
        assert!(is_http_url("HTTPS://[::1]:8443/path"));
        assert!(!is_http_url(""));
        assert!(!is_http_url("about:blank"));
        assert!(!is_http_url("javascript:alert(1)"));
        assert!(!is_http_url("file:///etc/passwd"));
        assert!(!is_http_url("https://"));
        assert!(!is_http_url("http:///path"));
    }

    #[test]
    fn matches_urls_according_to_rfc6265() {
        let host_cookie = WebCookie::new("sid", "1")
            .domain(".Example.COM")
            .path("/app")
            .http_only(true);
        assert!(matches_url(&host_cookie, "https://sub.example.com/app"));
        assert!(matches_url(&host_cookie, "https://example.com/app/home"));
        assert!(matches_url(&host_cookie, "http://sub.example.com/app?x=1"));
        assert!(!matches_url(&host_cookie, "https://example.com/apple"));
        assert!(!matches_url(&host_cookie, "https://notexample.com/app"));

        let secure_cookie = WebCookie::new("sec", "2")
            .secure(true)
            .domain("example.com");
        assert!(matches_url(&secure_cookie, "https://example.com/"));
        assert!(!matches_url(&secure_cookie, "http://example.com/"));

        let root_path = WebCookie::new("root", "3").path("/");
        assert!(matches_url(&root_path, "https://example.com/anything"));
    }

    #[test]
    fn parses_cookie_header_preserving_equals_in_values() {
        let parsed = parse_cookie_header("a=1; token=abc==def=; =bad; empty=; spaced = val ");
        assert_eq!(
            parsed,
            vec![
                WebCookie::new("a", "1"),
                WebCookie::new("token", "abc==def="),
                WebCookie::new("empty", ""),
                WebCookie::new("spaced", "val"),
            ]
        );
    }

    #[test]
    fn decodes_valid_json_cookie_lists_and_rejects_malformed() {
        let json = serde_json::json!([
            {
                "name": "session",
                "value": "xyz",
                "domain": ".example.com",
                "path": "/",
                "expires": 1800000000.0,
                "httpOnly": true,
                "secure": true,
                "sameSite": "lax"
            }
        ]);
        let decoded = decode_reply(&json).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].name, "session");
        assert_eq!(decoded[0].http_only, Some(true));
        assert_eq!(decoded[0].same_site, Some(WebCookieSameSite::Lax));

        assert_eq!(
            decode_reply(&serde_json::json!({"not": "array"}))
                .unwrap_err()
                .code(),
            Some(codes::WEBVIEW_ABORTED)
        );
        let too_many = JsonValue::Array(vec![
            serde_json::json!({"name":"a","value":"b"});
            MAX_COOKIES + 1
        ]);
        assert_eq!(
            decode_reply(&too_many).unwrap_err().code(),
            Some(codes::QUOTA_EXCEEDED)
        );
    }
}
