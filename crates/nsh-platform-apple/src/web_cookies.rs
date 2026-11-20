//! Reading cookies from a `WKWebView`'s `WKHTTPCookieStore`, shared by
//! the macOS and iOS backends.

use std::{
    cell::RefCell,
    time::{SystemTime, UNIX_EPOCH},
};

use block2::RcBlock;
use nsh_core::{
    JsonValue,
    webview::{WebCookie, WebCookieSameSite, cookies},
};
use objc2::{msg_send, rc::Retained, runtime::AnyObject, sel};
use objc2_foundation::{NSArray, NSDate, NSString};

/// Reads cookies matching `url` from `web`'s `WKHTTPCookieStore` and passes
/// the serialized JSON array (or an error message) to `done`.
pub fn get_cookies(
    web: &AnyObject,
    url: &str,
    done: impl FnOnce(Result<JsonValue, String>) + 'static,
) {
    // SAFETY: `WKWebView.configuration`, `websiteDataStore`, and
    // `httpCookieStore` are documented getters on the main thread.
    let cookie_store: Retained<AnyObject> = unsafe {
        let config: Retained<AnyObject> = msg_send![web, configuration];
        let store: Retained<AnyObject> = msg_send![&config, websiteDataStore];
        msg_send![&store, httpCookieStore]
    };
    let done = RefCell::new(Some(done));
    let url = url.to_owned();
    let block = RcBlock::new(move |list: *mut NSArray<AnyObject>| {
        let Some(done) = done.borrow_mut().take() else {
            return;
        };
        // SAFETY: WebKit lends the `NSArray` for the duration of the
        // completion block.
        let Some(list) = (unsafe { list.as_ref() }) else {
            done(Ok(JsonValue::Array(Vec::new())));
            return;
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs_f64());
        let mut out = Vec::new();
        for raw in list.iter() {
            let entry = convert_cookie(&raw);
            if entry
                .expires
                .zip(now)
                .is_some_and(|(expires, now)| expires <= now)
            {
                continue;
            }
            if !cookies::matches_url(&entry, &url) {
                continue;
            }
            out.push(entry);
            if out.len() > cookies::MAX_COOKIES {
                break;
            }
        }
        done(serde_json::to_value(out).map_err(|e| format!("cannot encode cookies: {e}")));
    });
    // SAFETY: `getAllCookies:` invokes the block with `NSArray<NSHTTPCookie *>`.
    unsafe {
        let _: () = msg_send![&cookie_store, getAllCookies: &*block];
    }
}

fn convert_cookie(cookie: &AnyObject) -> WebCookie {
    // SAFETY: `cookie` is an `NSHTTPCookie` returned by `WKHTTPCookieStore`;
    // `name`, `value`, `domain`, `path`, `isSessionOnly`, `isHTTPOnly`,
    // `isSecure`, `expiresDate`, and `sameSitePolicy` are documented selectors.
    unsafe {
        let name: Retained<NSString> = msg_send![cookie, name];
        let value: Retained<NSString> = msg_send![cookie, value];
        let domain: Retained<NSString> = msg_send![cookie, domain];
        let path: Retained<NSString> = msg_send![cookie, path];
        let session_only: bool = msg_send![cookie, isSessionOnly];
        let http_only: bool = msg_send![cookie, isHTTPOnly];
        let secure: bool = msg_send![cookie, isSecure];

        let mut entry = WebCookie::new(name.to_string(), value.to_string())
            .http_only(http_only)
            .secure(secure);

        let domain = domain.to_string();
        if !domain.is_empty() {
            entry = entry.domain(domain);
        }
        let path = path.to_string();
        if !path.is_empty() {
            entry = entry.path(path);
        }
        if !session_only {
            let expires: Option<Retained<NSDate>> = msg_send![cookie, expiresDate];
            if let Some(date) = expires {
                entry = entry.expires(date.timeIntervalSince1970());
            }
        }
        let has_same_site: bool = msg_send![cookie, respondsToSelector: sel!(sameSitePolicy)];
        if has_same_site {
            let policy: Option<Retained<NSString>> = msg_send![cookie, sameSitePolicy];
            if let Some(policy) = policy {
                match policy.to_string().as_str() {
                    "None" => entry = entry.same_site(WebCookieSameSite::None),
                    "Lax" => entry = entry.same_site(WebCookieSameSite::Lax),
                    "Strict" => entry = entry.same_site(WebCookieSameSite::Strict),
                    _ => {}
                }
            }
        }
        entry
    }
}
