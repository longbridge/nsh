//! Temporary native verification of the WebView cookie store on the three
//! desktop backends. Run against `.nsh/review-fixture.py`:
//! `SMOKE_URL=<FIXTURE_URL> SMOKE_OUTPUT=<dir>`.
//!
//! Three real web views share one window: two normal views (the app's
//! shared profile) and one `incognito` view (its own in-memory store).
//! Cookie values are never printed; failures name cookies and stages only.
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nsh_core::{
    prelude::*,
    webview::{CookieSameSite, WebCookie},
};

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

/// The expiry every platform can keep exactly (whole seconds since the
/// Unix epoch, 2030-01-01).
const EXPIRY_MS: i64 = 1_893_456_000_000;

fn main() -> impl std::process::Termination {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let output = PathBuf::from(std::env::var("SMOKE_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let options = platform::AppOptions {
        app_id: "com.nsh.cookie.smoke".into(),
        title: "NSH cookie verification".into(),
        size: (1200, 760),
        data_dir: Some(output.join("app-data")),
        ..Default::default()
    };
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<Smoke>().app_id("com.nsh.cookie.smoke")),
        options,
    )
}

async fn until(label: &str, mut ready: impl FnMut() -> bool) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready() {
        if Instant::now() >= deadline {
            return Err(format!("timed out: {label}"));
        }
        timer(Duration::from_millis(25)).await;
    }
    Ok(())
}

fn cookie(name: &str, domain: &str, path: &str) -> WebCookie {
    let mut cookie = WebCookie::default();
    cookie.name = name.into();
    cookie.value = "fixture".into();
    cookie.domain = domain.into();
    cookie.path = path.into();
    cookie
}

fn names(cookies: &[WebCookie]) -> String {
    cookies
        .iter()
        .map(|c| format!("{}@{}{}", c.name, c.domain, c.path))
        .collect::<Vec<_>>()
        .join(",")
}

fn expect_has(label: &str, cookies: &[WebCookie], name: &str) -> Result<(), String> {
    if cookies.iter().any(|c| c.name == name) {
        Ok(())
    } else {
        Err(format!("{label}: missing `{name}` (saw {})", names(cookies)))
    }
}

fn expect_absent(label: &str, cookies: &[WebCookie], name: &str) -> Result<(), String> {
    if cookies.iter().any(|c| c.name == name) {
        Err(format!("{label}: unexpected `{name}` (saw {})", names(cookies)))
    } else {
        Ok(())
    }
}

fn find<'a>(
    label: &str,
    cookies: &'a [WebCookie],
    name: &str,
) -> Result<&'a WebCookie, String> {
    cookies
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| format!("{label}: missing `{name}` (saw {})", names(cookies)))
}

#[component]
fn Smoke() -> impl IntoElement {
    let base = std::env::var("SMOKE_URL").unwrap();
    let output = PathBuf::from(std::env::var("SMOKE_OUTPUT").unwrap());
    let shared_a = element_ref();
    let shared_b = element_ref();
    let incognito = element_ref();
    let mounted = signal(true);
    let ready = signal(0usize);
    let errors = signal(Vec::<String>::new());

    let base_for_task = base.clone();
    spawn(async move {
        let result: Result<(), String> = async {
            until("three native pages", || ready.get() >= 3).await?;
            println!("PASS three native views loaded");

            // The fixture host, and URLs used only as cookie filters.
            let http_base = base_for_task;
            let https_base = http_base.replacen("http://", "https://", 1);
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_millis() as i64);
            // An ordinary persistent lifetime: isolate platform policy for
            // far-future dates, while still checking exact Unix ms.
            let primary_expiry = (now_ms / 1000 + 3600) * 1000;
            let deep_http = format!("{http_base}/deep/x");
            let deep_https = format!("{https_base}/deep/x");
            let other_http = format!("{http_base}/other");
            let path_a = format!("{http_base}/a");
            let path_b = format!("{http_base}/b");
            let domain_url = "https://sub.example.com/deep/x";

            // Start from a known set even if the data directory survives a
            // previous run. Deletion of an absent identity is success.
            for (name, domain, path) in [
                ("hostonly", "127.0.0.1", "/"),
                ("domainwide", ".example.com", "/"),
                ("pathed", "127.0.0.1", "/deep"),
                ("secureone", "127.0.0.1", "/"),
                ("attrs", "127.0.0.1", "/"),
                ("far", "127.0.0.1", "/"),
                ("twin", "127.0.0.1", "/a"),
                ("twin", "127.0.0.1", "/b"),
                ("incog", "127.0.0.1", "/"),
            ] {
                shared_a.delete_cookie(name, domain, path).await?;
                incognito.delete_cookie(name, domain, path).await?;
            }
            println!("PASS delete of absent cookies succeeds");

            // One cookie per set call. A host-only cookie and a domain
            // cookie have different names (their identity is name/domain/
            // path, not a host-only flag).
            shared_a
                .set_cookie(cookie("hostonly", "127.0.0.1", "/"))
                .await?;
            shared_a
                .set_cookie(cookie("domainwide", ".example.com", "/"))
                .await?;
            shared_a
                .set_cookie(cookie("pathed", "127.0.0.1", "/deep"))
                .await?;
            let mut secure = cookie("secureone", "127.0.0.1", "/");
            secure.secure = true;
            shared_a.set_cookie(secure).await?;
            let mut attrs = cookie("attrs", "127.0.0.1", "/");
            attrs.expires = Some(primary_expiry);
            attrs.same_site = Some(CookieSameSite::Strict);
            attrs.http_only = true;
            shared_a.set_cookie(attrs).await?;

            // The second normal view shares the app's store: what the first
            // set is there without being set again.
            let shared = shared_b.cookies(None).await?;
            expect_has("shared store", &shared, "hostonly")?;
            expect_has("shared store", &shared, "domainwide")?;
            println!("PASS two normal views share one store");

            // Host-only cookies match their exact host; domain cookies
            // match their domain and subdomains.
            let fixture_host = shared_a.cookies(Some(deep_http.as_str())).await?;
            expect_has("host-only filter", &fixture_host, "hostonly")?;
            expect_absent("domain cookie on another host", &fixture_host, "domainwide")?;
            let domain = shared_b.cookies(Some(domain_url)).await?;
            expect_has("domain filter", &domain, "domainwide")?;
            expect_absent("host-only cookie on a domain", &domain, "hostonly")?;
            println!("PASS host-only and domain cookies filter by host");

            // Paths match at a path boundary; root-path cookies still
            // apply. Secure cookies need HTTPS.
            expect_has("deep path", &fixture_host, "pathed")?;
            let other = shared_b.cookies(Some(other_http.as_str())).await?;
            expect_absent("shallow path", &other, "pathed")?;
            expect_has("root path", &other, "hostonly")?;
            let http_deep = shared_a.cookies(Some(deep_http.as_str())).await?;
            expect_absent("secure over http", &http_deep, "secureone")?;
            let https_deep = shared_a.cookies(Some(deep_https.as_str())).await?;
            expect_has("secure over https", &https_deep, "secureone")?;
            expect_has("path over https", &https_deep, "pathed")?;
            println!("PASS path and secure cookie selection");

            // Expiry crosses the boundary in Unix milliseconds; the
            // attributes survive a set and a read.
            let all = shared_a.cookies(None).await?;
            let seen = find("attributes", &all, "attrs")?;
            if seen.expires != Some(primary_expiry) {
                return Err(format!("attrs expiry: {:?}", seen.expires));
            }
            if seen.same_site != Some(CookieSameSite::Strict) {
                return Err(format!("attrs same_site: {:?}", seen.same_site));
            }
            if !seen.http_only {
                return Err("attrs http_only was not kept".into());
            }
            let secure_seen = find("secure attribute", &all, "secureone")?;
            if !secure_seen.secure {
                return Err("secure attribute was not kept".into());
            }

            // A far-future date is platform policy, not the ordinary
            // conversion above: record what the native store did instead
            // of silently dropping the case.
            let mut far = cookie("far", "127.0.0.1", "/");
            far.expires = Some(EXPIRY_MS);
            match shared_a.set_cookie(far).await {
                Ok(()) => {
                    let stored = shared_a.cookies(None).await?;
                    match find("far-future", &stored, "far")?.expires {
                        Some(ms) if ms == EXPIRY_MS => {
                            println!("PASS far-future expiry kept exactly")
                        }
                        Some(ms) if ms > now_ms => println!(
                            "LIMITATION far-future expiry capped by the platform: requested {EXPIRY_MS} ms, stored {ms} ms"
                        ),
                        Some(ms) => {
                            return Err(format!("far-future expiry was in the past: {ms} ms"))
                        }
                        None => return Err("far-future cookie lost its expiry".into()),
                    }
                }
                Err(error) => {
                    println!("LIMITATION far-future expiry rejected by the platform: {error}")
                }
            }
            println!("PASS expiry milliseconds, SameSite and HttpOnly");

            // Deleting one identity leaves the same name at another path.
            shared_a
                .set_cookie(cookie("twin", "127.0.0.1", "/a"))
                .await?;
            shared_a
                .set_cookie(cookie("twin", "127.0.0.1", "/b"))
                .await?;
            expect_has("twin /a before", &shared_b.cookies(Some(path_a.as_str())).await?, "twin")?;
            expect_has("twin /b before", &shared_b.cookies(Some(path_b.as_str())).await?, "twin")?;
            shared_a.delete_cookie("twin", "127.0.0.1", "/a").await?;
            expect_absent(
                "twin /a after delete",
                &shared_b.cookies(Some(path_a.as_str())).await?,
                "twin",
            )?;
            expect_has(
                "twin /b after other-path delete",
                &shared_b.cookies(Some(path_b.as_str())).await?,
                "twin",
            )?;
            shared_a
                .delete_cookie("twin", "127.0.0.1", "/missing")
                .await?;
            expect_has(
                "twin /b after absent delete",
                &shared_b.cookies(Some(path_b.as_str())).await?,
                "twin",
            )?;
            shared_a
                .delete_cookie("never-set", "127.0.0.1", "/")
                .await?;
            println!("PASS delete is exact and absent deletion succeeds");

            // An incognito view has its own in-memory store.
            incognito
                .set_cookie(cookie("incog", "127.0.0.1", "/"))
                .await?;
            let shared_after = shared_a.cookies(None).await?;
            expect_absent("incognito cookie in shared store", &shared_after, "incog")?;
            let private = incognito.cookies(None).await?;
            expect_has("incognito store", &private, "incog")?;
            expect_absent("shared cookie in incognito store", &private, "hostonly")?;
            println!("PASS incognito and shared stores are isolated");

            // A ref whose view unmounted fails instead of reading another
            // view's store.
            mounted.set(false);
            timer(Duration::from_millis(250)).await;
            if shared_a.cookies(None).await.is_ok() {
                return Err("a disposed web view read cookies".into());
            }
            let surviving = shared_b.cookies(None).await?;
            expect_has("surviving shared view", &surviving, "hostonly")?;
            println!("PASS a disposed ref fails and the other view survives");

            // Keep the output directory part of the run (the app's data is
            // already there).
            std::fs::write(output.join("smoke-status.txt"), "PASS\n")
                .map_err(|error| error.to_string())?;
            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                println!("WEBVIEW_SMOKE_PASS");
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!(
                    "WEBVIEW_SMOKE_FAIL: {error}\nNative errors: {:?}",
                    errors.get()
                );
                std::process::exit(1);
            }
        }
    });

    let a_url = format!("{base}/a");
    let b_url = format!("{base}/b");
    let c_url = format!("{base}/c");
    view().flex_1().child(view().flex_row().children([
        when(
            move || mounted.get(),
            move || {
                web_view(a_url.clone())
                    .element_ref(&shared_a)
                    .w(px(380.0))
                    .h(px(300.0))
                    .on_load(move |_: &WebNavigation| ready.update(|n| *n += 1))
                    .on_error(move |e: &str| errors.update(|v| v.push(e.into())))
                    .on_unavailable(move |e: &str| errors.update(|v| v.push(e.into())))
            },
        )
        .into_element(),
        web_view(b_url)
            .element_ref(&shared_b)
            .w(px(380.0))
            .h(px(300.0))
            .on_load(move |_: &WebNavigation| ready.update(|n| *n += 1))
            .on_error(move |e: &str| errors.update(|v| v.push(e.into())))
            .on_unavailable(move |e: &str| errors.update(|v| v.push(e.into())))
            .into_element(),
        web_view(c_url)
            .element_ref(&incognito)
            .incognito(true)
            .w(px(380.0))
            .h(px(300.0))
            .on_load(move |_: &WebNavigation| ready.update(|n| *n += 1))
            .on_error(move |e: &str| errors.update(|v| v.push(e.into())))
            .on_unavailable(move |e: &str| errors.update(|v| v.push(e.into())))
            .into_element(),
    ]))
}
