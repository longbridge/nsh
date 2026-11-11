//! Temporary native verification of the desktop WebView download contract.
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nsh_core::{
    prelude::*,
    webview::{DownloadDestination, WebDownload, WebDownloadPending, WebDownloadState},
};
#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

/// The bytes `/download` serves, in the same order as the fixture.
const CONTENT: &[u8] = b"NSH native WebView download fixture\n";

fn main() -> impl std::process::Termination {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,nsh_platform_windows::native=debug"),
    )
    .init();
    let output = PathBuf::from(std::env::var("SMOKE_OUTPUT").expect("SMOKE_OUTPUT"));
    std::fs::create_dir_all(&output).unwrap();
    let options = platform::AppOptions {
        app_id: "com.nsh.download.smoke".into(),
        title: "NSH WebView download verification".into(),
        size: (1000, 600),
        data_dir: Some(output.join("app-data")),
        ..Default::default()
    };
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<Smoke>().app_id("com.nsh.download.smoke")),
        options,
    )
}

fn require(ok: bool, message: impl Into<String>) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
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

/// Starts a page request that the fixture answers as an attachment. A host
/// navigation is used instead of a page script: script-triggered downloads
/// are separately subject to the browser's download permissions.
fn start_download(web: &ElementRef, base: &str, path: &str) -> Result<(), String> {
    let url = format!("{}{}", base.trim_end_matches('/'), path);
    let load = nsh_core::WebLoad::new(url, Vec::new()).ok_or("fixture download URL")?;
    web.load(load);
    Ok(())
}

/// The staging files a download of `id` owns, whatever platform suffix the
/// implementation chose.
fn staging_files(dir: &Path, id: &str) -> Vec<PathBuf> {
    let prefix = format!(".nsh-download-{id}.part");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect()
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

#[component]
fn Smoke() -> impl IntoElement {
    let base = std::env::var("SMOKE_URL").expect("SMOKE_URL");
    let output = PathBuf::from(std::env::var("SMOKE_OUTPUT").expect("SMOKE_OUTPUT"));
    let automatic = element_ref();
    let manual = element_ref();
    let mounted = signal(true);
    let loaded = signal(0usize);
    let automatic_downloads = signal(Vec::<WebDownload>::new());
    let manual_downloads = signal(Vec::<WebDownload>::new());
    let pending = signal(Vec::<WebDownloadPending>::new());
    let errors = signal(Vec::<String>::new());
    let status = signal(String::from("Running native download checks"));
    let page = format!("{}/page", base.trim_end_matches('/'));
    let run_base = base.clone();

    spawn(async move {
        let result: Result<(), String> = async {
            until("two loaded pages", || loaded.get() >= 2).await?;
            println!("PASS native pages loaded");

            // Automatic: only on_download, no pending listener.
            start_download(&automatic, &run_base, "/download")?;
            until("automatic download finished", || {
                automatic_downloads
                    .get()
                    .iter()
                    .any(|download| download.state == WebDownloadState::Finished)
            })
            .await?;
            let automatic_file = automatic_downloads
                .get()
                .into_iter()
                .find(|download| download.state == WebDownloadState::Finished)
                .ok_or("automatic download event")?;
            require(
                automatic_file.path.starts_with(output.join("app-data")),
                format!("automatic download outside app data: {:?}", automatic_file.path),
            )?;
            require(
                automatic_file.bytes == Some(CONTENT.len() as u64),
                "automatic download byte count",
            )?;
            require(
                read(&automatic_file.path)? == CONTENT,
                "automatic download content",
            )?;
            println!("PASS automatic download path, bytes and content");

            // Manual: the on_download_pending registration itself opts out of
            // the automatic policy, even with on_download also registered.
            let before = pending.get().len();
            start_download(&manual, &run_base, "/download")?;
            until("first pending download", || pending.get().len() > before).await?;
            let first = pending.get().last().cloned().ok_or("first pending")?;
            let unwritten = output.join("not-accepted.txt");
            let _ = std::fs::remove_file(&unwritten);
            timer(Duration::from_millis(300)).await;
            require(!unwritten.exists(), "a pending download wrote a destination")?;
            require(
                !manual_downloads
                    .get()
                    .iter()
                    .any(|download| download.id == first.id),
                "a pending download started automatically",
            )?;
            require(
                staging_files(&output, &first.id).is_empty(),
                "a pending download wrote a staging file",
            )?;
            first.cancel().await?;
            println!("PASS pending download writes nothing before acceptance");

            let before = pending.get().len();
            start_download(&manual, &run_base, "/download")?;
            until("accepted pending download", || pending.get().len() > before).await?;
            let accepted = pending.get().last().cloned().ok_or("accepted pending")?;
            let accepted_path = output.join("accepted.txt");
            let _ = std::fs::remove_file(&accepted_path);
            accepted.accept(accepted_path.clone()).await?;
            until("accepted download finished", || {
                manual_downloads.get().iter().any(|download| {
                    download.id == accepted.id && download.state == WebDownloadState::Finished
                })
            })
            .await?;
            let accepted_event = manual_downloads
                .get()
                .into_iter()
                .find(|download| {
                    download.id == accepted.id && download.state == WebDownloadState::Finished
                })
                .ok_or("accepted finished event")?;
            require(
                accepted_event.path == accepted_path,
                "accepted download path",
            )?;
            require(
                accepted_event.bytes == Some(CONTENT.len() as u64),
                "accepted download byte count",
            )?;
            require(read(&accepted_path)? == CONTENT, "accepted download content")?;
            require(
                accepted
                    .accept(output.join("duplicate.txt"))
                    .await
                    .is_err(),
                "duplicate acceptance succeeded",
            )?;
            require(
                !output.join("duplicate.txt").exists(),
                "duplicate acceptance wrote a destination",
            )?;
            println!("PASS pending accept, content, bytes and duplicate rejection");

            // The public id is for matching only; the private handle keeps
            // the original download's identity.
            let before = pending.get().len();
            start_download(&manual, &run_base, "/download")?;
            until("id-tamper pending download", || pending.get().len() > before).await?;
            let original = pending.get().last().cloned().ok_or("id-tamper pending")?;
            let mut forged = original.clone();
            forged.id = "forged-id".into();
            let forged_path = output.join("private-identity.txt");
            let _ = std::fs::remove_file(&forged_path);
            forged.accept(forged_path.clone()).await?;
            until("private-identity download finished", || {
                manual_downloads.get().iter().any(|download| {
                    download.id == original.id && download.state == WebDownloadState::Finished
                })
            })
            .await?;
            require(
                !manual_downloads
                    .get()
                    .iter()
                    .any(|download| download.id == "forged-id"),
                "the edited public id addressed another download",
            )?;
            require(
                read(&forged_path)? == CONTENT,
                "private-identity download content",
            )?;
            println!("PASS edited public id keeps the private download identity");

            let existing = output.join("existing.txt");
            std::fs::write(&existing, b"keep").map_err(|e| e.to_string())?;
            let before = pending.get().len();
            start_download(&manual, &run_base, "/download")?;
            until("no-overwrite pending download", || pending.get().len() > before).await?;
            let refused = pending.get().last().cloned().ok_or("no-overwrite pending")?;
            require(
                refused.accept(existing.clone()).await.is_err(),
                "an existing destination was accepted without overwrite",
            )?;
            require(read(&existing)? == b"keep", "refused acceptance changed the file")?;
            refused.cancel().await?;
            println!("PASS existing destination preserved without overwrite");

            let before = pending.get().len();
            start_download(&manual, &run_base, "/download")?;
            until("overwrite pending download", || pending.get().len() > before).await?;
            let overwrite = pending.get().last().cloned().ok_or("overwrite pending")?;
            overwrite
                .accept(DownloadDestination::new(existing.clone()).overwrite(true))
                .await?;
            until("overwrite download finished", || {
                manual_downloads.get().iter().any(|download| {
                    download.id == overwrite.id && download.state == WebDownloadState::Finished
                })
            })
            .await?;
            require(read(&existing)? == CONTENT, "explicit overwrite content")?;
            println!("PASS explicit overwrite");

            // An accepted slow transfer is cancelled while it is active: the
            // owned staging file must disappear, not merely the destination
            // that was never published.
            let before = pending.get().len();
            start_download(&manual, &run_base, "/slow-download")?;
            until("active-cancel pending download", || pending.get().len() > before).await?;
            let active = pending.get().last().cloned().ok_or("active pending")?;
            let active_path = output.join("active-cancel.txt");
            let _ = std::fs::remove_file(&active_path);
            active.accept(active_path.clone()).await?;
            until("active staging file", || {
                !staging_files(&output, &active.id).is_empty()
            })
            .await?;
            require(
                !active_path.exists(),
                "an unfinished transfer published its destination",
            )?;
            active.cancel().await?;
            until("active staging cleanup", || {
                staging_files(&output, &active.id).is_empty()
            })
            .await?;
            require(
                !active_path.exists(),
                "a cancelled transfer left its destination",
            )?;
            until("cancelled download failure event", || {
                manual_downloads.get().iter().any(|download| {
                    download.id == active.id && download.state == WebDownloadState::Failed
                })
            })
            .await?;
            println!("PASS active transfer cancellation removes the staging file");

            // Removing the view tears down its pending work and staging,
            // leaving completed files alone.
            let before = pending.get().len();
            start_download(&manual, &run_base, "/slow-download")?;
            until("teardown pending download", || pending.get().len() > before).await?;
            let torn = pending.get().last().cloned().ok_or("teardown pending")?;
            let torn_path = output.join("teardown.txt");
            let _ = std::fs::remove_file(&torn_path);
            torn.accept(torn_path.clone()).await?;
            until("teardown staging file", || {
                !staging_files(&output, &torn.id).is_empty()
            })
            .await?;
            mounted.set(false);
            until("teardown staging cleanup", || {
                staging_files(&output, &torn.id).is_empty()
            })
            .await?;
            require(!torn_path.exists(), "teardown published a destination")?;
            require(
                read(&accepted_path)? == CONTENT,
                "teardown changed a completed accepted file",
            )?;
            require(
                read(&existing)? == CONTENT,
                "teardown changed a completed overwritten file",
            )?;
            require(
                read(&automatic_file.path)? == CONTENT,
                "teardown changed an automatic download",
            )?;
            println!("PASS teardown cleans staging and preserves completed files");

            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                status.set("All native download checks passed".into());
                println!("WEBVIEW_SMOKE_PASS");
                timer(Duration::from_millis(100)).await;
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!(
                    "WEBVIEW_SMOKE_FAIL: {error}\nNative errors: {:?}\nPending: {:?}\nDownloads: {:?}",
                    errors.get(),
                    pending.get().iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
                    manual_downloads.get(),
                );
                std::process::exit(1);
            }
        }
    });

    view().flex_1().child(text(move || status.get())).child(
        view().flex_row().children([
            web_view(page.clone())
                .element_ref(&automatic)
                .w(px(400.0))
                .h(px(300.0))
                .on_load(move |_: &WebNavigation| loaded.update(|n| *n += 1))
                .on_download(move |event: &WebDownload| {
                    automatic_downloads.update(|v| v.push(event.clone()))
                })
                .on_error(move |message: &str| errors.update(|v| v.push(message.to_owned())))
                .on_unavailable(move |message: &str| {
                    errors.update(|v| v.push(format!("automatic unavailable: {message}")))
                })
                .into_element(),
            when(
                move || mounted.get(),
                move || {
                    web_view(page.clone())
                        .element_ref(&manual)
                        .w(px(400.0))
                        .h(px(300.0))
                        .on_load(move |_: &WebNavigation| loaded.update(|n| *n += 1))
                        .on_download(move |event: &WebDownload| {
                            manual_downloads.update(|v| v.push(event.clone()))
                        })
                        .on_download_pending(move |event: &WebDownloadPending| {
                            pending.update(|v| v.push(event.clone()))
                        })
                        .on_error(move |message: &str| {
                            errors.update(|v| v.push(message.to_owned()))
                        })
                        .on_unavailable(move |message: &str| {
                            errors.update(|v| v.push(format!("manual unavailable: {message}")))
                        })
                },
            )
            .into_element(),
        ]),
    )
}
