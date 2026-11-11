//! A `WKWebView`'s cookie store, shared by the macOS and iOS backends:
//! `WKHTTPCookieStore` of the view's own `WKWebsiteDataStore` (the app's
//! profile, or the in-memory store of an `incognito` view).
//!
//! Each operation answers through [`Answer`] when WebKit has carried it
//! out, and only while the view is alive: a completion that outlives its
//! host must not reach a restarted engine's node with the same id.
//! `get_cookies` filters by [`nsh_core::webview::cookie_applies_to`] and
//! `delete_cookie` finds the cookies a deletion names by
//! [`nsh_core::webview::cookie_has_identity`], so both backends share one
//! interpretation of a cookie's URL, identity and attributes.
//!
//! The cookie written is made by parsing a synthesized `Set-Cookie` header
//! and checked attribute for attribute: `NSHTTPCookie`'s property dictionary
//! cannot keep `HttpOnly` or `SameSite=None`, and a cookie that silently
//! lost an attribute would hand the app a false assurance.

use std::{
    cell::Cell,
    ffi::CStr,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use block2::RcBlock;
use nsh_core::{
    JsonValue,
    webview::{CookieSameSite, WebCookie, cookie_applies_to, cookie_has_identity},
};
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
};
use objc2_foundation::{NSArray, NSHTTPCookie, NSString, NSURL};

/// Hands a cookie operation's answer to the caller once: `Ok` with the
/// request's value (`null` for a write or a deletion), or why it failed.
pub type Answer = Rc<dyn Fn(Result<JsonValue, String>)>;

/// An [`Answer`] from `reply`.
pub fn answer(reply: impl Fn(Result<JsonValue, String>) + 'static) -> Answer {
    Rc::new(reply)
}

/// Answers `answer` only while the web view is alive.
fn reply_if_alive(alive: &Arc<AtomicBool>, answer: &Answer, result: Result<JsonValue, String>) {
    if alive.load(Ordering::Acquire) {
        answer(result);
    }
}

fn class(name: &CStr) -> Option<&'static AnyClass> {
    AnyClass::get(name)
}

/// The web view's cookie store (`WKHTTPCookieStore`, scoped to the view's
/// own `WKWebsiteDataStore`).
fn store(web: &AnyObject) -> Option<Retained<AnyObject>> {
    // SAFETY: WebKit's documented WKWebView / WKWebsiteDataStore API.
    unsafe {
        let configuration: Option<Retained<AnyObject>> = msg_send![web, configuration];
        let store: Option<Retained<AnyObject>> = msg_send![&*configuration?, websiteDataStore];
        let jar: Option<Retained<AnyObject>> = msg_send![&*store?, httpCookieStore];
        jar
    }
}

/// Reads the store's cookies, optionally only those sent to `url`: a
/// host-only cookie matches its exact host, a domain cookie (a leading
/// dot) that domain and its subdomains, by path (RFC 6265) and `secure`.
pub fn get_cookies(web: &AnyObject, url: Option<&str>, alive: Arc<AtomicBool>, answer: Answer) {
    let Some(jar) = store(web) else {
        reply_if_alive(
            &alive,
            &answer,
            Err("this web view has no cookie store".into()),
        );
        return;
    };
    let scope = url.map(str::to_owned);
    let done = RcBlock::new(move |cookies: *mut NSArray<NSHTTPCookie>| {
        // SAFETY: WebKit passes the array of the store's cookies.
        let cookies = unsafe { cookies.as_ref() }
            .map(|cookies| cookies.to_vec())
            .unwrap_or_default();
        let list: Vec<WebCookie> = cookies
            .iter()
            .map(|cookie| web_cookie(cookie))
            .filter(|cookie| {
                scope
                    .as_deref()
                    .is_none_or(|url| cookie_applies_to(cookie, url))
            })
            .collect();
        let result = serde_json::to_value(list).map_err(|error| error.to_string());
        reply_if_alive(&alive, &answer, result);
    });
    // SAFETY: WebKit's documented WKHTTPCookieStore API.
    let _: () = unsafe { msg_send![&jar, getAllCookies: &*done] };
}

/// A cookie as core's [`WebCookie`] (`expires` in Unix milliseconds).
fn web_cookie(cookie: &NSHTTPCookie) -> WebCookie {
    let mut out = WebCookie::default();
    out.name = cookie.name().to_string();
    out.value = cookie.value().to_string();
    out.domain = cookie.domain().to_string();
    out.path = cookie.path().to_string();
    out.expires = cookie
        .expiresDate()
        .map(|date| (date.timeIntervalSince1970() * 1000.0).round() as i64);
    out.http_only = cookie.isHTTPOnly();
    out.secure = cookie.isSecure();
    out.same_site = same_site_of(cookie);
    out
}

/// Writes `cookie` to the store (`setCookie:`), answering when WebKit has
/// it.
pub fn set_cookie(web: &AnyObject, cookie: &WebCookie, alive: Arc<AtomicBool>, answer: Answer) {
    let Some(jar) = store(web) else {
        reply_if_alive(
            &alive,
            &answer,
            Err("this web view has no cookie store".into()),
        );
        return;
    };
    let cookie = match http_cookie(cookie) {
        Ok(cookie) => cookie,
        Err(error) => {
            reply_if_alive(&alive, &answer, Err(error));
            return;
        }
    };
    let done = RcBlock::new(move || {
        reply_if_alive(&alive, &answer, Ok(JsonValue::Null));
    });
    // SAFETY: WebKit's documented WKHTTPCookieStore API.
    let _: () = unsafe { msg_send![&jar, setCookie: &*cookie, completionHandler: &*done] };
}

/// Deletes the cookie with this name, domain and path, and no other: the
/// store's matching cookies are found first and each deleted. Nothing
/// matching is success (the store deletes nothing).
pub fn delete_cookie(
    web: &AnyObject,
    name: &str,
    domain: &str,
    path: &str,
    alive: Arc<AtomicBool>,
    answer: Answer,
) {
    let Some(jar) = store(web) else {
        reply_if_alive(
            &alive,
            &answer,
            Err("this web view has no cookie store".into()),
        );
        return;
    };
    let name = name.to_owned();
    let domain = domain.to_owned();
    let path = path.to_owned();
    let jar_for_list = jar.clone();
    let list = RcBlock::new(move |cookies: *mut NSArray<NSHTTPCookie>| {
        if !alive.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: WebKit passes the array of the store's cookies.
        let cookies = unsafe { cookies.as_ref() }
            .map(|cookies| cookies.to_vec())
            .unwrap_or_default();
        let matching: Vec<Retained<NSHTTPCookie>> = cookies
            .into_iter()
            .filter(|cookie| cookie_has_identity(&web_cookie(cookie), &name, &domain, &path))
            .collect();
        if matching.is_empty() {
            reply_if_alive(&alive, &answer, Ok(JsonValue::Null));
            return;
        }
        // WebKit deletes one at a time: answered when the last is gone.
        let remaining = Rc::new(Cell::new(matching.len()));
        for cookie in matching {
            let remaining = remaining.clone();
            let alive = alive.clone();
            let answer = answer.clone();
            let done = RcBlock::new(move || {
                let left = remaining.get().saturating_sub(1);
                remaining.set(left);
                if left == 0 {
                    reply_if_alive(&alive, &answer, Ok(JsonValue::Null));
                }
            });
            // SAFETY: WebKit's documented WKHTTPCookieStore API.
            let _: () = unsafe {
                msg_send![&jar_for_list, deleteCookie: &*cookie, completionHandler: &*done]
            };
        }
    });
    // SAFETY: WebKit's documented WKHTTPCookieStore API.
    let _: () = unsafe { msg_send![&jar, getAllCookies: &*list] };
}

/// The `same_site` a parsed cookie reports (`None` without the attribute).
fn same_site_of(cookie: &NSHTTPCookie) -> Option<CookieSameSite> {
    let policy = cookie.sameSitePolicy()?.to_string().to_ascii_lowercase();
    match policy.as_str() {
        "none" => Some(CookieSameSite::None),
        "lax" => Some(CookieSameSite::Lax),
        "strict" => Some(CookieSameSite::Strict),
        _ => None,
    }
}

/// A `Set-Cookie` header field for `cookie`, or why it cannot be written.
///
/// The header is synthesized (rather than `NSHTTPCookie`'s property
/// dictionary) because that is the only public way to keep `HttpOnly` and
/// `SameSite=None`; every field is validated so a value or attribute
/// cannot inject a second cookie or attribute (no CR/LF, `;`, `,` or
/// invalid name).
fn set_cookie_header(cookie: &WebCookie) -> Result<String, String> {
    let name = &cookie.name;
    if name.is_empty() || !name.bytes().all(is_token_byte) {
        return Err(format!("cookie name `{name}` is not a valid cookie name"));
    }
    if cookie
        .value
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | ',' | ';' | '\\'))
    {
        return Err(format!(
            "cookie `{name}` has a value that cannot be sent in a `Set-Cookie` header"
        ));
    }
    let domain = &cookie.domain;
    let host = domain.strip_prefix('.').unwrap_or(domain.as_str());
    // A hostname (or an IP literal): nothing that could end the header
    // field or start a quoted string.
    if host.is_empty()
        || host.chars().any(|c| {
            !(c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | ':' | '[' | ']' | '%'))
        })
    {
        return Err(format!("cookie `{name}` has an invalid domain `{domain}`"));
    }
    if cookie.path.is_empty() || !cookie.path.starts_with('/') {
        return Err(format!(
            "cookie `{name}` has no absolute path (`{}`)",
            cookie.path
        ));
    }
    if cookie
        .path
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | ';' | ',' | '\\'))
    {
        return Err(format!(
            "cookie `{name}` has a path that cannot be sent in a `Set-Cookie` header"
        ));
    }
    if cookie.same_site == Some(CookieSameSite::None) && !cookie.secure {
        return Err(format!(
            "cookie `{name}` asks for SameSite=None without Secure, which browsers do not send"
        ));
    }

    let mut header = format!("{name}={}", cookie.value);
    // A leading dot is how Foundation spells a cookie for subdomains; without
    // one the cookie is host-only, which the URL it is parsed against
    // supplies.
    if domain.starts_with('.') {
        header.push_str("; Domain=");
        header.push_str(domain);
    }
    header.push_str("; Path=");
    header.push_str(&cookie.path);
    if let Some(millis) = cookie.expires {
        // The header has one-second resolution; the platform reports and
        // verifies milliseconds.
        let seconds = millis as f64 / 1000.0;
        let date = http_date(seconds).ok_or_else(|| {
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
    if let Some(policy) = cookie.same_site {
        header.push_str("; SameSite=");
        header.push_str(policy.to_string().as_str());
    }
    // Every field was validated above; this is the last line of defence
    // against a header that ends early or carries a second one.
    if header.contains('\r') || header.contains('\n') {
        return Err(format!(
            "cookie `{name}` cannot be sent in a `Set-Cookie` header"
        ));
    }
    Ok(header)
}

/// `Sun, 06 Nov 1994 08:49:37 GMT` (RFC 1123) for `seconds` since the Unix
/// epoch; `None` outside years 1000–9999.
fn http_date(seconds: f64) -> Option<String> {
    if !seconds.is_finite() {
        return None;
    }
    let seconds = seconds.floor();
    // A generous bound (about ±6 million years) keeps the day arithmetic
    // below from overflowing; the four-digit year check rejects the rest.
    if !(-2.0e14..=2.0e14).contains(&seconds) {
        return None;
    }
    let seconds = seconds as i64;
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

/// RFC 6265's `token`: what a cookie name may contain.
fn is_token_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
    ) || byte.is_ascii_alphanumeric()
}

/// Checks that `parsed` — what Foundation made of the synthesized
/// `Set-Cookie` header — carries every attribute `cookie` asked for. Making a
/// cookie that loses one (a missing `HttpOnly`, a weaker `SameSite`, another
/// expiry) would hand the app a false assurance, so that is an error.
fn verify_cookie(parsed: &NSHTTPCookie, cookie: &WebCookie) -> Result<(), String> {
    let name = &cookie.name;
    let mismatch = |attribute: &str| -> Result<(), String> {
        Err(format!(
            "the cookie store does not keep the `{attribute}` attribute of cookie `{name}`"
        ))
    };
    if parsed.name().to_string() != name.as_str()
        || parsed.value().to_string() != cookie.value.as_str()
    {
        return mismatch("name or value");
    }
    if !parsed
        .domain()
        .to_string()
        .trim_start_matches('.')
        .eq_ignore_ascii_case(cookie.domain.trim_start_matches('.'))
    {
        return mismatch("domain");
    }
    if parsed.path().to_string() != cookie.path.as_str() {
        return mismatch("path");
    }
    // The header carries seconds, so a millisecond expiry is kept to the
    // second Foundation parsed it back with.
    let expected_expiry = cookie.expires.map(|millis| millis.div_euclid(1000));
    let parsed_expiry = parsed
        .expiresDate()
        .map(|date| date.timeIntervalSince1970().round() as i64);
    if parsed_expiry != expected_expiry {
        return Err(format!(
            "the cookie store does not keep the expires attribute of cookie `{name}`: requested {expected_expiry:?}, parsed {parsed_expiry:?} (Unix seconds)"
        ));
    }
    if parsed.isSecure() != cookie.secure {
        return mismatch("secure");
    }
    if parsed.isHTTPOnly() != cookie.http_only {
        return mismatch("http_only");
    }
    if same_site_of(parsed) != cookie.same_site {
        return mismatch("same_site");
    }
    Ok(())
}

/// An `NSHTTPCookie` for `cookie`, made by parsing a synthesized
/// `Set-Cookie` header (the only public way to keep `HttpOnly` and
/// `SameSite=None`), verified attribute for attribute.
fn http_cookie(cookie: &WebCookie) -> Result<Retained<NSHTTPCookie>, String> {
    let name = &cookie.name;
    let header = set_cookie_header(cookie)?;
    let scheme = if cookie.secure { "https" } else { "http" };
    let host = cookie
        .domain
        .strip_prefix('.')
        .unwrap_or(cookie.domain.as_str());
    // The explicit `Path` attribute sets the cookie's path; the URL only
    // supplies the origin (host-only cookies take its host).
    let url = NSURL::URLWithString(&NSString::from_str(&format!("{scheme}://{host}/")))
        .ok_or_else(|| format!("cookie `{name}` has an invalid domain `{}`", cookie.domain))?;
    let Some(cookie_class) = class(c"NSHTTPCookie") else {
        return Err("NSHTTPCookie is unavailable".into());
    };
    let Some(dictionary_class) = class(c"NSMutableDictionary") else {
        return Err("NSMutableDictionary is unavailable".into());
    };
    // SAFETY: Foundation's documented `cookiesWithResponseHeaderFields:`
    // parser; the header was validated to hold exactly one cookie.
    let parsed = unsafe {
        let fields: Retained<AnyObject> = msg_send![dictionary_class, dictionary];
        let key = NSString::from_str("Set-Cookie");
        let value = NSString::from_str(&header);
        let _: () = msg_send![&fields, setObject: &*value, forKey: &*key];
        let cookies: Option<Retained<NSArray<NSHTTPCookie>>> = msg_send![
            cookie_class,
            cookiesWithResponseHeaderFields: &*fields,
            forURL: &*url
        ];
        cookies
    };
    let Some(cookies) = parsed else {
        return Err(format!(
            "the platform cannot make cookie `{name}` from its `Set-Cookie` header"
        ));
    };
    let mut cookies = cookies.to_vec();
    if cookies.len() != 1 {
        return Err(format!(
            "cookie `{name}` made {} cookies instead of one",
            cookies.len()
        ));
    }
    let parsed = cookies.remove(0);
    verify_cookie(&parsed, cookie)?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesizes_cookie_headers_that_keep_every_attribute() {
        let mut cookie = WebCookie::default();
        cookie.name = "sid".into();
        cookie.value = "v".into();
        cookie.domain = "example.com".into();
        cookie.path = "/app".into();
        // Without a leading dot the cookie is host-only: no Domain attribute.
        assert_eq!(set_cookie_header(&cookie).unwrap(), "sid=v; Path=/app");
        cookie.domain = ".example.com".into();
        cookie.expires = Some(1_700_000_000_123);
        cookie.http_only = true;
        cookie.secure = true;
        cookie.same_site = Some(CookieSameSite::Strict);
        assert_eq!(
            set_cookie_header(&cookie).unwrap(),
            "sid=v; Domain=.example.com; Path=/app; Expires=Tue, 14 Nov 2023 22:13:20 GMT; Secure; HttpOnly; SameSite=strict"
        );

        // A value or field that could end the header, or add a second
        // cookie or attribute, is refused before any native call.
        for (field, value) in [
            ("name", "a;b"),
            ("value", "v; Path=/evil"),
            ("value", "a b"),
            ("domain", "example.com; Secure"),
            ("path", "relative"),
            ("path", "/x; Secure"),
        ] {
            let mut bad = cookie.clone();
            match field {
                "name" => bad.name = value.into(),
                "value" => bad.value = value.into(),
                "domain" => bad.domain = value.into(),
                "path" => bad.path = value.into(),
                _ => unreachable!(),
            }
            assert!(set_cookie_header(&bad).is_err(), "{field}={value:?}");
        }
        let mut none = cookie.clone();
        none.same_site = Some(CookieSameSite::None);
        none.secure = false;
        assert!(set_cookie_header(&none).is_err());
    }

    #[test]
    fn writes_http_dates_at_second_resolution() {
        // The core's milliseconds are floored to the header's seconds.
        assert_eq!(http_date(0.0).unwrap(), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(
            http_date(1_700_000_000.999).unwrap(),
            "Tue, 14 Nov 2023 22:13:20 GMT"
        );
        assert_eq!(
            http_date(1_893_456_000.0).unwrap(),
            "Tue, 01 Jan 2030 00:00:00 GMT"
        );
        assert!(http_date(f64::NAN).is_none());
        assert!(http_date(f64::INFINITY).is_none());
        assert!(http_date(-1.0e15).is_none());
    }

    #[test]
    fn foundation_keeps_the_attributes_that_were_written() {
        let mut cookie = WebCookie::default();
        cookie.name = "attrs".into();
        cookie.value = "fixture".into();
        cookie.domain = ".example.com".into();
        cookie.path = "/app".into();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        cookie.expires = Some((now as i64 + 3600) * 1000);
        cookie.http_only = true;
        cookie.secure = true;
        cookie.same_site = Some(CookieSameSite::Strict);

        let parsed = http_cookie(&cookie).expect("the synthesized header parses");
        assert!(parsed.isHTTPOnly());
        assert!(parsed.isSecure());
        assert_eq!(parsed.path().to_string(), "/app");

        // Reading it back uses the same conversion the backends use.
        let back = web_cookie(&parsed);
        assert_eq!(back.name, "attrs");
        assert_eq!(back.value, "fixture");
        assert_eq!(back.domain.trim_start_matches('.'), "example.com");
        assert_eq!(back.path, "/app");
        assert_eq!(back.expires, cookie.expires);
        assert!(back.http_only);
        assert!(back.secure);
        assert_eq!(back.same_site, Some(CookieSameSite::Strict));
    }

    #[test]
    fn keeps_the_lifecycle_fence_before_answering() {
        let alive = Arc::new(AtomicBool::new(false));
        let answered = Rc::new(Cell::new(false));
        let seen = answered.clone();
        reply_if_alive(
            &alive,
            &answer(move |_| seen.set(true)),
            Ok(JsonValue::Null),
        );
        assert!(!answered.get());
        alive.store(true, Ordering::Release);
        let flag = answered.clone();
        reply_if_alive(
            &alive,
            &answer(move |_| flag.set(true)),
            Ok(JsonValue::Null),
        );
        assert!(answered.get());
    }
}
