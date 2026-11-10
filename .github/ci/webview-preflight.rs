//! Branch-only native WebView integration verification.
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nsh_core::{
    ContentRuleList,
    prelude::*,
    webview::{
        CookieSameSite, WebCookie, WebDownload, WebDownloadBehavior, WebDownloadDestination,
        WebDownloadPending, WebDownloadState, WebProgress,
    },
};
#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

const CONTENT: &[u8] = b"NSH native WebView download fixture\n";

fn main() -> impl std::process::Termination {
    let dir = PathBuf::from(std::env::var("SMOKE_OUTPUT").expect("SMOKE_OUTPUT"));
    std::fs::create_dir_all(&dir).unwrap();
    let options = platform::AppOptions {
        app_id: "com.nsh.webview.smoke".into(),
        title: "NSH WebView integration smoke".into(),
        size: (1000, 600),
        data_dir: Some(dir.join("app-data")),
        ..Default::default()
    };
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<Smoke>().app_id("com.nsh.webview.smoke")),
        options,
    )
}

fn require(ok: bool, message: impl Into<String>) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}

async fn until(label: &str, mut ready: impl FnMut() -> bool) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready() {
        if Instant::now() >= deadline {
            return Err(format!("timed out: {label}"));
        }
        timer(Duration::from_millis(25)).await;
    }
    Ok(())
}

fn rule(id: &str, rules: &str) -> ContentRuleList {
    let mut result = ContentRuleList::default();
    result.id = id.into();
    result.rules = rules.into();
    result
}

fn valid_rules() -> Vec<ContentRuleList> {
    vec![rule(
        "smoke",
        r#"[
      {"trigger":{"url-filter":"/blocked"},"action":{"type":"block"}},
      {"trigger":{"url-filter":".*"},"action":{"type":"css-display-none","selector":".ad"}}
    ]"#,
    )]
}

fn pixel_matches(image: &image::RgbaImage, x: u32, y: u32, expected: [u8; 3]) -> bool {
    let x = x * image.width() / 320;
    let y = y * image.height() / 240;
    let actual = image.get_pixel(x, y).0;
    (0..3).all(|i| actual[i].abs_diff(expected[i]) <= 12)
}

fn destination(id: &str, path: PathBuf) -> WebDownloadDestination {
    let mut value = WebDownloadDestination::default();
    value.id = id.into();
    value.path = path;
    value
}

#[component]
fn Smoke() -> impl IntoElement {
    let url = std::env::var("SMOKE_URL").expect("SMOKE_URL");
    let output = PathBuf::from(std::env::var("SMOKE_OUTPUT").expect("SMOKE_OUTPUT"));
    let primary = element_ref();
    let shared = element_ref();
    let private = element_ref();
    let messages = signal(Vec::<String>::new());
    let errors = signal(Vec::<String>::new());
    let downloads = signal(Vec::<WebDownload>::new());
    let pending = signal(Vec::<WebDownloadPending>::new());
    let progress = signal(Vec::<WebProgress>::new());
    let mode = signal(WebDownloadBehavior::Automatic);
    let filters = signal(valid_rules());
    let mounted = signal(true);
    let status = signal(String::from("Running real native WebView checks"));

    let run_url = url.clone();
    spawn(async move {
        let result: Result<(), String> = async {
            until("three loaded pages", || messages.get().iter().filter(|s| s.as_str() == "ready").count() >= 3).await?;
            until("load completion", || progress.get().iter().any(|p| p.progress.is_some_and(|n| n.0 >= 1.0))).await?;
            println!("PASS native load and progress");

            let capturing = primary.capture_page();
            let reading_private = private.get_cookies(None);
            let png = capturing.await?;
            reading_private.await?;
            let shot = image::load_from_memory(&png).map_err(|e| e.to_string())?.into_rgba8();
            require(shot.width() >= 240 && shot.height() >= 120, "screenshot dimensions")?;
            require(pixel_matches(&shot, 50, 50, [0, 200, 80]), "visible green marker was not captured")?;
            require(pixel_matches(&shot, 160, 50, [20, 40, 60]), "CSS content rule did not hide red marker")?;
            std::fs::write(output.join("filtered.png"), &png).map_err(|e| e.to_string())?;
            println!("PASS native PNG capture and cosmetic filtering");

            let mut cookie = WebCookie::default();
            cookie.name = "nsh_smoke".into(); cookie.value = "native".into();
            cookie.domain = "127.0.0.1".into(); cookie.path = "/".into();
            cookie.http_only = true; cookie.same_site = Some(CookieSameSite::Lax);
            let expiry = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64 / 1000 + 3600) * 1000;
            cookie.expires = Some(expiry);
            primary.set_cookies(vec![cookie.clone()]).await?;
            let got = primary.get_cookies(Some(&run_url)).await?;
            let found = got.iter().find(|c| c.name == cookie.name).ok_or("native cookie missing")?;
            require(found.value == "native" && found.http_only && found.same_site == Some(CookieSameSite::Lax), "cookie attributes did not round trip")?;
            require(found.expires.is_some_and(|n| n.abs_diff(expiry) <= 1000), "cookie expiry lost its millisecond/epoch semantics")?;
            require(shared.get_cookies(None).await?.iter().any(|c| c.name == cookie.name), "same-store view cannot read cookie")?;
            require(!private.get_cookies(None).await?.iter().any(|c| c.name == cookie.name), "cookie leaked into incognito store")?;
            primary.inject_java_script("window.nsh.postMessage('cookie:' + document.cookie)");
            until("page cookie report", || messages.get().iter().any(|m| m.starts_with("cookie:"))).await?;
            require(messages.get().iter().filter(|m| m.starts_with("cookie:")).all(|m| !m.contains("nsh_smoke")), "HttpOnly cookie exposed to page")?;
            let mut host_cookie = cookie.clone();
            host_cookie.name = "host_scope_smoke".into();
            host_cookie.domain = "example.com".into();
            host_cookie.secure = true;
            primary.set_cookies(vec![host_cookie]).await?;
            require(primary.get_cookies(Some("https://example.com/")).await?.iter().any(|c| c.name == "host_scope_smoke"), "host-only cookie missing from its exact host")?;
            require(!primary.get_cookies(Some("https://sub.example.com/")).await?.iter().any(|c| c.name == "host_scope_smoke"), "host-only cookie leaked into subdomain filter")?;
            let mut valid = cookie.clone(); valid.name = "must_not_partially_set".into();
            let mut invalid = cookie; invalid.name = "bad\nname".into();
            require(primary.set_cookies(vec![valid, invalid]).await.is_err(), "invalid cookie accepted")?;
            require(!primary.get_cookies(None).await?.iter().any(|c| c.name == "must_not_partially_set"), "invalid batch partially changed store")?;
            println!("PASS cookie attributes, shared/incognito isolation, HttpOnly and prevalidation");

            primary.set_inspectable(true).await?;
            #[cfg(not(target_os = "macos"))]
            primary.open_devtools().await?;
            #[cfg(target_os = "macos")]
            require(primary.open_devtools().await.is_err(), "unsupported macOS devtools open reported success")?;
            primary.set_inspectable(false).await?;
            println!("PASS inspection enable/disable and platform open-tools contract");

            primary.inject_java_script("fetch('/blocked').catch(()=>{}).then(()=>fetch('/counts')).then(r=>r.json()).then(v=>window.nsh.postMessage('blocked-count:'+String(v['/blocked']||0)))");
            until("network filtering report", || messages.get().iter().any(|m| m.starts_with("blocked-count:"))).await?;
            require(messages.get().iter().any(|m| m == "blocked-count:0"), "blocked request reached fixture server")?;
            println!("PASS native network request blocking");

            primary.inject_java_script("location.href='/download?mode=auto'");
            shared.inject_java_script("location.href='/download?mode=concurrent-auto'");
            until("concurrent automatic downloads", || downloads.get().iter().filter(|d| d.state == WebDownloadState::Finished).count() >= 2).await?;
            let automatic: Vec<_> = downloads.get().into_iter().filter(|d| d.state == WebDownloadState::Finished).collect();
            require(automatic[0].id != automatic[1].id && automatic[0].path != automatic[1].path, "two views reused a download id or destination")?;
            for download in automatic {
                require(download.path.starts_with(output.join("app-data")), format!("download outside app data: {:?}", download.path))?;
                require(std::fs::read(&download.path).map_err(|e| e.to_string())? == CONTENT, "automatic download content mismatch")?;
                require(download.bytes == Some(CONTENT.len() as u64), "automatic download byte count")?;
                require(downloads.get().iter().any(|d| d.id == download.id && d.state == WebDownloadState::Started), "missing automatic start event")?;
            }
            println!("PASS concurrent automatic downloads and app-data isolation");

            mode.set(WebDownloadBehavior::Manual);
            primary.measure().await;
            primary.inject_java_script("location.href='/download?mode=manual'");
            until("pending manual download", || !pending.get().is_empty()).await?;
            let first = pending.get()[0].clone();
            require(!downloads.get().iter().any(|d| d.id == first.id), "manual download started without acceptance")?;
            let dest = output.join("accepted.txt");
            require(!dest.exists(), "manual destination already exists")?;
            primary.accept_download(destination(&first.id, dest.clone())).await?;
            until("accepted download finished", || downloads.get().iter().any(|d| d.id == first.id && d.state == WebDownloadState::Finished)).await?;
            require(std::fs::read(&dest).map_err(|e| e.to_string())? == CONTENT, "accepted file content mismatch")?;
            require(primary.accept_download(destination(&first.id, output.join("duplicate.txt"))).await.is_err(), "duplicate acceptance succeeded")?;
            println!("PASS manual accept, content and duplicate-id rejection");

            let previous = pending.get().len();
            primary.inject_java_script("location.href='/download?mode=reject-overwrite'");
            until("second pending download", || pending.get().len() > previous).await?;
            let second = pending.get().last().unwrap().clone();
            require(primary.accept_download(destination(&second.id, dest.clone())).await.is_err(), "existing destination overwritten without permission")?;
            require(std::fs::read(&dest).map_err(|e| e.to_string())? == CONTENT, "failed acceptance changed existing file")?;
            let _ = primary.cancel_download(second.id.clone()).await;
            let previous = pending.get().len();
            primary.inject_java_script("location.href='/download?mode=cancel'");
            until("cancel candidate", || pending.get().len() > previous).await?;
            let cancelled = pending.get().last().unwrap().clone();
            primary.cancel_download(cancelled.id.clone()).await?;
            require(primary.accept_download(destination(&cancelled.id, output.join("cancelled.txt"))).await.is_err(), "cancelled download accepted")?;
            require(!output.join("cancelled.txt").exists(), "cancelled download wrote destination")?;
            println!("PASS no-overwrite policy and explicit cancellation");

            let before_pending = pending.get().len();
            primary.inject_java_script("location.href='/slow-download?mode=active-cancel'");
            until("active cancellation candidate", || pending.get().len() > before_pending).await?;
            let active = pending.get().last().unwrap().clone();
            let active_path = output.join("active-cancel.txt");
            primary.accept_download(destination(&active.id, active_path.clone())).await?;
            primary.cancel_download(active.id.clone()).await?;
            until("active partial cleanup", || !active_path.exists()).await?;
            require(primary.cancel_download(active.id).await.is_err(), "completed cancellation id remained active")?;
            require(std::fs::read(&dest).map_err(|e| e.to_string())? == CONTENT, "cancelling another transfer changed completed output")?;
            println!("PASS active-transfer cancellation and completed-file preservation");

            let before_errors = errors.get().len();
            filters.set(vec![rule("invalid", "not JSON")]);
            until("filter compilation rejection", || errors.get().len() > before_errors).await?;
            let png = primary.capture_page().await?;
            let shot = image::load_from_memory(&png).map_err(|e| e.to_string())?.into_rgba8();
            require(pixel_matches(&shot, 160, 50, [20, 40, 60]), "invalid rule replacement removed active policy")?;
            filters.set(Vec::new());
            primary.measure().await;
            let before_ready = messages.get().iter().filter(|s| s.as_str() == "ready").count();
            primary.reload();
            until("reload after clearing filters", || messages.get().iter().filter(|s| s.as_str() == "ready").count() > before_ready).await?;
            let png = primary.capture_page().await?;
            let shot = image::load_from_memory(&png).map_err(|e| e.to_string())?.into_rgba8();
            require(pixel_matches(&shot, 160, 50, [240, 20, 20]), "cleared cosmetic rule still active")?;
            std::fs::write(output.join("unfiltered.png"), &png).map_err(|e| e.to_string())?;
            println!("PASS failed filter replacement retention and clearing");

            let before_pending = pending.get().len();
            primary.inject_java_script("location.href='/slow-download?mode=teardown'");
            until("download pending before teardown", || pending.get().len() > before_pending).await?;
            mounted.set(false);
            timer(Duration::from_millis(300)).await;
            require(primary.capture_page().await.is_err(), "disposed ref captured a page")?;
            require(primary.get_cookies(None).await.is_err(), "disposed ref read cookies")?;
            let surviving = image::load_from_memory(&private.capture_page().await?).map_err(|e| e.to_string())?.into_rgba8();
            require(pixel_matches(&surviving, 50, 50, [0, 200, 80]), "teardown broke unrelated view's content")?;
            println!("PASS teardown with pending download and unrelated-view survival");
            Ok(())
        }.await;
        match result {
            Ok(()) => {
                status.set("All native WebView smoke checks passed".into());
                println!("WEBVIEW_SMOKE_PASS");
                timer(Duration::from_millis(100)).await;
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!(
                    "WEBVIEW_SMOKE_FAIL: {error}\nNative errors: {:?}\nDownloads: {:?}",
                    errors.get(),
                    downloads.get()
                );
                std::process::exit(1);
            }
        }
    });

    let primary_url = url.clone();
    view().flex_1().child(text(move || status.get())).child(
        view().flex_row().children([
            when(
                move || mounted.get(),
                move || {
                    web_view(primary_url.clone())
                        .element_ref(&primary)
                        .w(px(320.0))
                        .h(px(240.0))
                        .content_rules(filters)
                        .download_behavior(mode)
                        .on_message(move |s: &str| messages.update(|v| v.push(s.to_owned())))
                        .on_error(move |s: &str| errors.update(|v| v.push(s.to_owned())))
                        .on_progress(move |e: &WebProgress| progress.update(|v| v.push(*e)))
                        .on_download(move |e: &WebDownload| downloads.update(|v| v.push(e.clone())))
                        .on_download_pending(move |e: &WebDownloadPending| {
                            pending.update(|v| v.push(e.clone()))
                        })
                },
            )
            .into_element(),
            web_view(url.clone())
                .element_ref(&shared)
                .w(px(320.0))
                .h(px(240.0))
                .on_download(move |e: &WebDownload| downloads.update(|v| v.push(e.clone())))
                .on_message(move |s: &str| messages.update(|v| v.push(s.to_owned())))
                .into_element(),
            web_view(url)
                .incognito(true)
                .element_ref(&private)
                .w(px(320.0))
                .h(px(240.0))
                .on_message(move |s: &str| messages.update(|v| v.push(s.to_owned())))
                .into_element(),
        ]),
    )
}
