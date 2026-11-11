//! Android's cookie store: what its public API can and cannot do.
//!
//! `android.webkit.CookieManager` can set one cookie through the
//! `Set-Cookie` header form, and reports whether the store accepted it
//! through a `ValueCallback<Boolean>`. It cannot enumerate a store:
//! `getCookie(url)` returns only the `Cookie` request header's
//! `name=value` pairs, without domain, path, expiry, `HttpOnly`, `Secure`
//! or `SameSite`, and there is no API that lists cookies without a URL.
//! It cannot delete one cookie either: the public removals are
//! `removeAllCookies` and `removeSessionCookies`.
//!
//! Android keeps one WebView cookie store per app, shared by every view,
//! `incognito` included (that flag only skips the HTTP cache and saved
//! form data). A set from any view therefore reaches that shared store,
//! which is the platform's existing cookie behaviour, not a per-view
//! store invented here.
//!
//! So reads and deletes
//! ([`nsh_core::webview::WebViewOperation`]) answer explicit errors
//! instead of inventing metadata or wiping a broader store;
//! [`set_cookie`] validates the cookie, builds the `Set-Cookie` value and
//! the URL it is parsed against, and leaves the store's completion answer
//! to `WebBox.java`.

use nsh_core::webview::{CookieSameSite, WebCookie};

/// Why reading cookies answers an error on Android.
pub(crate) const GET_COOKIES_UNSUPPORTED: &str = "Android's CookieManager cannot list a web view's cookies: getCookie(url) returns only `name=value` pairs in Cookie-header form, without domain, path, expiry, HttpOnly, Secure or SameSite, and no API lists the store without a URL";

/// Why deleting one cookie answers an error on Android.
pub(crate) const DELETE_COOKIE_UNSUPPORTED: &str = "Android's CookieManager has no per-cookie delete: its only removals are removeAllCookies and removeSessionCookies, which would remove more than the cookie this request names";

/// What `CookieManager.setCookie(url, value, …)` needs for `cookie`: the URL
/// whose host supplies a host-only cookie's domain and whose scheme satisfies
/// `Secure`, and the `Set-Cookie` value with every attribute.
pub(crate) struct CookieTarget {
    pub(crate) url: String,
    pub(crate) header: String,
}

/// The URL and `Set-Cookie` value that store `cookie` faithfully, or why it
/// cannot be written.
///
/// The core validates a cookie before it reaches a platform; these checks
/// keep the header itself unbreakable at the boundary as well. `value` is
/// documented to follow `Set-Cookie` (MDN's attributes), so every attribute
/// the contract carries is written as a header field: `Domain` only for a
/// domain cookie (a leading dot; without it the URL's host makes the cookie
/// host-only), `Path`, `Expires`, `Secure`, `HttpOnly` and `SameSite`.
pub(crate) fn set_cookie(cookie: &WebCookie) -> Result<CookieTarget, String> {
    let name = &cookie.name;
    if name.is_empty() || name.contains(['\r', '\n', ';', '=']) {
        return Err(format!(
            "cookie name `{name}` cannot be sent in a `Set-Cookie` header"
        ));
    }
    if cookie.value.contains(['\r', '\n', ';']) {
        return Err(format!(
            "cookie `{name}` has a value that cannot be sent in a `Set-Cookie` header"
        ));
    }
    let path = &cookie.path;
    if !path.starts_with('/') || path.contains(['\r', '\n', ';']) {
        return Err(format!(
            "cookie `{name}` has a path that cannot be sent in a `Set-Cookie` header (`{path}`)"
        ));
    }
    // The host of the URL a host-only cookie takes, and the `Domain` value of
    // a domain cookie: a hostname or a bracketed IP literal, nothing that
    // could end the authority.
    let domain = &cookie.domain;
    let host = domain.strip_prefix('.').unwrap_or(domain.as_str());
    if host.is_empty()
        || host.contains(['/', '?', '#', '@'])
        || host.starts_with('[') != host.ends_with(']')
        || (host.contains(':') && !(host.starts_with('[') && host.ends_with(']')))
    {
        return Err(format!(
            "cookie `{name}` has a domain that is not a host or a bracketed IP literal (`{domain}`)"
        ));
    }
    if cookie.same_site == Some(CookieSameSite::None) && !cookie.secure {
        return Err(format!(
            "cookie `{name}` asks for SameSite=None without Secure, which browsers do not send"
        ));
    }

    let mut header = format!("{name}={}", cookie.value);
    // A leading dot is how the contract spells a domain cookie. Without it
    // the URL's host makes the cookie host-only.
    if domain.starts_with('.') {
        header.push_str("; Domain=");
        header.push_str(domain);
    }
    header.push_str("; Path=");
    header.push_str(path);
    if let Some(expires) = cookie.expires {
        // `Set-Cookie`'s date has one-second resolution.
        let date = http_date(expires.div_euclid(1000)).ok_or_else(|| {
            format!("cookie `{name}` has an expiry date outside the representable range")
        })?;
        header.push_str("; Expires=");
        header.push_str(&date);
    }
    if cookie.secure {
        header.push_str("; Secure");
    }
    if cookie.http_only {
        header.push_str("; HttpOnly");
    }
    if let Some(same_site) = cookie.same_site {
        header.push_str("; SameSite=");
        header.push_str(match same_site {
            CookieSameSite::None => "None",
            CookieSameSite::Lax => "Lax",
            CookieSameSite::Strict => "Strict",
            // `CookieSameSite` is non-exhaustive.
            _ => {
                return Err(format!("cookie `{name}` has an unknown SameSite policy"));
            }
        });
    }

    // `Secure` requires an HTTPS URL (CookieManager documents it); other
    // cookies are set over HTTP, as the web itself sets them.
    let scheme = if cookie.secure { "https" } else { "http" };
    Ok(CookieTarget {
        url: format!("{scheme}://{host}/"),
        header,
    })
}

/// `Sun, 06 Nov 1994 08:49:37 GMT` (RFC 1123) for `seconds` since the Unix
/// epoch; `None` outside years 1000–9999.
fn http_date(seconds: i64) -> Option<String> {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (rest / 3_600, rest % 3_600 / 60, rest % 60);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + (if month <= 2 { 1 } else { 0 });
    if !(1000..=9999).contains(&year) {
        return None;
    }
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let weekday = WEEKDAYS[days.rem_euclid(7) as usize];
    Some(format!(
        "{weekday}, {day:02} {} {year:04} {hour:02}:{minute:02}:{second:02} GMT",
        MONTHS[(month - 1) as usize]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(name: &str, domain: &str, path: &str) -> WebCookie {
        let mut cookie = WebCookie::default();
        cookie.name = name.into();
        cookie.value = "fixture".into();
        cookie.domain = domain.into();
        cookie.path = path.into();
        cookie
    }

    #[test]
    fn a_domain_cookie_keeps_every_attribute() {
        let mut cookie = cookie("sid", ".example.com", "/app");
        cookie.expires = Some(1_893_456_000_000);
        cookie.http_only = true;
        cookie.secure = true;
        cookie.same_site = Some(CookieSameSite::Strict);
        let target = set_cookie(&cookie).unwrap();
        assert_eq!(target.url, "https://example.com/");
        assert_eq!(
            target.header,
            "sid=fixture; Domain=.example.com; Path=/app; \
             Expires=Tue, 01 Jan 2030 00:00:00 GMT; Secure; HttpOnly; SameSite=Strict"
        );
    }

    #[test]
    fn a_host_only_session_cookie_omits_domain_and_expires() {
        let target = set_cookie(&cookie("sid", "example.com", "/")).unwrap();
        assert_eq!(target.url, "http://example.com/");
        assert_eq!(target.header, "sid=fixture; Path=/");
    }

    #[test]
    fn same_site_none_is_secure_and_https() {
        let mut cookie = cookie("sid", "example.com", "/");
        cookie.secure = true;
        cookie.same_site = Some(CookieSameSite::None);
        let target = set_cookie(&cookie).unwrap();
        assert_eq!(target.url, "https://example.com/");
        assert_eq!(target.header, "sid=fixture; Path=/; Secure; SameSite=None");
    }

    #[test]
    fn an_unbracketed_ipv6_domain_is_refused() {
        assert!(set_cookie(&cookie("sid", "::1", "/")).is_err());
        let target = set_cookie(&cookie("sid", "[::1]", "/")).unwrap();
        assert_eq!(target.url, "http://[::1]/");
    }

    #[test]
    fn malformed_fields_are_refused() {
        let mut bad = cookie("sid", "example.com", "/");
        bad.value = "a;b".into();
        assert!(set_cookie(&bad).is_err());

        let mut bad = cookie("sid", "example.com", "relative");
        assert!(set_cookie(&bad).is_err());
        bad.path = "/\r\n".into();
        assert!(set_cookie(&bad).is_err());

        let mut bad = cookie("sid", "example.com", "/");
        bad.expires = Some(i64::MAX);
        assert!(set_cookie(&bad).is_err());
    }

    #[test]
    fn http_dates_are_rfc1123_at_second_resolution() {
        assert_eq!(http_date(0).unwrap(), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(
            http_date(1_893_456_000).unwrap(),
            "Tue, 01 Jan 2030 00:00:00 GMT"
        );
        assert!(http_date(4_000_000_000_000).is_none());
    }
}
