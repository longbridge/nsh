//! Android cookie store smoke: the real Rust root of an app, run by the
//! Android development host (`nsh test --target android`).
//!
//! It proves, on a device or emulator with the platform WebView:
//! - `set_cookie` with every attribute the store can keep (`HttpOnly`,
//!   `SameSite=Lax`, a future expiry) is accepted by `CookieManager`;
//! - an HTTP server in the app process sees the `HttpOnly` cookie in a
//!   later navigation's `Cookie` header, while the page's
//!   `document.cookie` does not;
//! - invalid cookies are refused before touching the store (the valid
//!   cookie of the same identity survives), and an unmounted web view
//!   answers an error instead of a late success;
//! - `cookies` (with and without a URL) and `delete_cookie` answer
//!   explicit platform errors instead of fabricated metadata or a broader
//!   removal, and the cookie still reaches the server afterwards.
//!
//! Packaging: an app directory with the crate this file belongs to and
//! `nsh.json`:
//! ```json
//! {
//!   "name": "Android cookies",
//!   "id": "com.nsh.cookie.android",
//!   "rust": ".",
//!   "root": "App",
//!   "android": { "usesCleartextTraffic": true }
//! }
//! ```
//! The crate's dependencies are `nsh-core` (with `nsh-base` only if the UI
//! uses it; this file does not) and `log`. `nsh package <dir> --target
//! android --run` (or `nsh test --target android`) builds its `libnsh.so`
//! and starts it. `usesCleartextTraffic` lets the fixture's `http://`
//! URL to `127.0.0.1` load; a release app without it blocks the plain
//! HTTP request.
//!
//! Cookie values are never printed; failures name cookies and stages only.

use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, UdpSocket},
    sync::{Arc, LazyLock, Mutex},
    thread,
    time::{Duration, Instant},
};

use nsh_core::{
    WebLoad,
    prelude::*,
    webview::{CookieSameSite, WebCookie},
};

/// One request the fixture served.
struct Seen {
    path: String,
    cookie: Option<String>,
}

/// A local HTTP fixture: every request is recorded, so the test can prove
/// what the WebView actually sent.
struct Fixture {
    port: u16,
    host: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

fn fixture() -> &'static Fixture {
    static FIXTURE: LazyLock<Fixture> = LazyLock::new(|| {
        // Chromium treats loopback as trustworthy even over HTTP.
        // Use the emulator's interface to check Secure transmission.
        let route = UdpSocket::bind(("0.0.0.0", 0)).expect("fixture route socket");
        route.connect(("192.0.2.1", 9)).expect("fixture route");
        let host = route
            .local_addr()
            .expect("fixture address")
            .ip()
            .to_string();
        let listener = TcpListener::bind((host.as_str(), 0)).expect("fixture listener");
        let port = listener.local_addr().expect("fixture address").port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let requests = seen.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Ok(reader) = stream.try_clone() else {
                    continue;
                };
                let mut reader = BufReader::new(reader);
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    continue;
                }
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let mut cookie = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                    if line.to_ascii_lowercase().starts_with("cookie:") {
                        cookie = line
                            .split_once(':')
                            .map(|(_, value)| value.trim().to_owned());
                    }
                }
                requests.lock().expect("fixture").push(Seen {
                    path: path.clone(),
                    cookie,
                });
                let (content_type, body) = (
                    "text/html; charset=utf-8",
                    "<!doctype html><title>cookie fixture</title><p>fixture</p>",
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Fixture { port, host, seen }
    });
    &FIXTURE
}

fn seen_len() -> usize {
    fixture().seen.lock().expect("fixture").len()
}

/// Waits for a request to `path` recorded at or after index `from`, and
/// gives its `Cookie` header (empty when it sent none).
async fn wait_for(path: &str, from: usize) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        {
            let seen = fixture().seen.lock().expect("fixture");
            if let Some(request) = seen.iter().skip(from).find(|request| request.path == path) {
                return Some(request.cookie.clone().unwrap_or_default());
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        timer(Duration::from_millis(50)).await;
    }
}

fn refused(result: Result<(), String>, what: &str) -> Result<(), String> {
    match result {
        Err(_) => Ok(()),
        Ok(()) => Err(format!("{what} was accepted")),
    }
}

async fn run(
    base: String,
    web: ElementRef,
    mounted: Signal<bool>,
    loads: Signal<u32>,
    start: usize,
) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    if wait_for("/", start).await.is_none() {
        return Err("the fixture page never loaded".into());
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while loads.get() == 0 {
        if Instant::now() >= deadline {
            return Err("the fixture page never finished loading".into());
        }
        timer(Duration::from_millis(50)).await;
    }
    lines.push("PASS page loaded".into());

    let mut sid = WebCookie::default();
    sid.name = "sid".into();
    sid.value = "fixture".into();
    sid.domain = fixture().host.clone();
    sid.path = "/".into();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    sid.expires = Some((now as i64 + 3600) * 1000);
    sid.http_only = true;
    sid.same_site = Some(CookieSameSite::Lax);
    web.set_cookie(sid.clone())
        .await
        .map_err(|error| format!("set_cookie: {error}"))?;
    lines.push("PASS set_cookie with HttpOnly, SameSite=Lax and an expiry".into());

    // A Secure cookie takes the HTTPS URL and must not travel over HTTP.
    let mut secure = sid.clone();
    secure.name = "sec".into();
    secure.secure = true;
    secure.same_site = None;
    web.set_cookie(secure)
        .await
        .map_err(|error| format!("set_cookie (Secure): {error}"))?;
    lines.push("PASS set_cookie with Secure".into());

    // Invalid cookies with the same identity as `sid` must not replace it:
    // the core refuses each before it reaches CookieManager.
    let mut bad = sid.clone();
    bad.value = "bad;value".into();
    refused(web.set_cookie(bad).await, "a value with `;`")?;
    let mut bad = sid.clone();
    bad.path = "relative".into();
    refused(web.set_cookie(bad).await, "a relative path")?;
    let mut bad = sid.clone();
    bad.same_site = Some(CookieSameSite::None);
    refused(web.set_cookie(bad).await, "SameSite=None without Secure")?;
    let mut bad = sid.clone();
    bad.name.clear();
    refused(web.set_cookie(bad).await, "an empty name")?;
    lines.push("PASS invalid cookies refused without mutating the store".into());

    let error = web
        .cookies(Some(base.as_str()))
        .await
        .expect_err("Android must not fabricate cookie metadata");
    if !error.contains("CookieManager") || !error.contains("getCookie") {
        return Err(format!(
            "cookies error did not name the platform limit: {error}"
        ));
    }
    web.cookies(None)
        .await
        .expect_err("Android has no whole-store enumeration");
    let error = web
        .delete_cookie("sid", &fixture().host, "/")
        .await
        .expect_err("Android must not claim an exact per-cookie delete");
    if !error.contains("removeAllCookies") {
        return Err(format!(
            "delete_cookie error did not name the platform limit: {error}"
        ));
    }
    lines.push("PASS cookies and delete_cookie answer explicit platform errors".into());

    let evaluation = web
        .evaluate("document.cookie", EvaluateOptions::default())
        .await
        .map_err(|error| format!("evaluate document.cookie: {error:?}"))?;
    let visible = evaluation.value.as_str().unwrap_or_default();
    if visible
        .split(';')
        .any(|field| matches!(field.trim().split_once('='), Some((name, _)) if name == "sid" || name == "sec"))
    {
        return Err("an HttpOnly cookie was visible to document.cookie".into());
    }
    lines.push("PASS HttpOnly cookie hidden from the page's scripts".into());

    let from = seen_len();
    let load = WebLoad::new(format!("{base}check"), Vec::new())
        .ok_or_else(|| "the check URL is not a valid load".to_owned())?;
    web.load(load);
    let cookie = wait_for("/check", from)
        .await
        .ok_or_else(|| "the check navigation never reached the fixture".to_owned())?;
    let fields: Vec<&str> = cookie.split(';').map(str::trim).collect();
    if !fields.contains(&"sid=fixture") {
        return Err("the HTTP server did not receive the HttpOnly cookie".into());
    }
    if fields.contains(&"sec=fixture") {
        return Err("a Secure cookie was sent over HTTP".into());
    }
    lines.push("PASS the HTTP server received the HttpOnly cookie".into());

    // Disposal fences a late request: the ref no longer names a mounted
    // web view, so the answer is an error instead of a silent success.
    mounted.set(false);
    timer(Duration::from_millis(500)).await;
    if web.set_cookie(sid).await.is_ok() {
        return Err("set_cookie after the view unmounted succeeded".into());
    }
    lines.push("PASS a disposed web view answers an error".into());

    Ok(lines)
}

/// The app's UI set (`nsh.json` `"rust"`), as the generated workspace
/// calls it: this smoke registers no components, modules or files of its
/// own, but a Rust app's crate needs the entry.
#[native_module("mobile-cookie/native")]
fn native(_registry: &mut Registry) {}

/// The app's root: one mounted web view and the smoke's report.
#[component]
pub fn App() -> impl IntoElement {
    let web = element_ref();
    let mounted = signal(true);
    let loads = signal(0_u32);
    let report = signal(Vec::<String>::new());
    let base = format!("http://{}:{}/", fixture().host, fixture().port);
    // Requests recorded before this run's web view mounts (a hot reload
    // keeps the fixture): the run waits for one after them.
    let start = seen_len();

    let task_base = base.clone();
    spawn(async move {
        match run(task_base, web, mounted, loads, start).await {
            Ok(lines) => {
                for line in &lines {
                    log::info!("{line}");
                }
                log::info!("WEBVIEW_SMOKE_PASS");
                report.set(lines);
            }
            Err(error) => {
                log::error!("FAIL: {error}");
                report.update(|lines| lines.push(format!("FAIL: {error}")));
            }
        }
    });

    view()
        .p(px(16.0))
        .gap(px(8.0))
        .child(text("Android cookie store smoke").text_size(px(16.0)))
        .child(view().w_full().h(px(320.0)).child(when(
            move || mounted.get(),
            move || {
                let page = base.clone();
                let loaded = page.clone();
                web_view(page)
                    .element_ref(&web)
                    .java_script_enabled(true)
                    .on_load(move |event: &WebNavigation| {
                        if event.url.starts_with(loaded.as_str()) {
                            loads.update(|n| *n += 1);
                        }
                    })
                    .w_full()
                    .h_full()
            },
        )))
        .child(text(move || report.get().join("\n")).text_size(px(12.0)))
}
