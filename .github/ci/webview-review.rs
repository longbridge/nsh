//! Temporary native verification for progress, capture and reactive inspection.
use std::{path::PathBuf, time::{Duration, Instant}};
use nsh_core::{prelude::*, webview::WebProgress};
#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

fn main() -> impl std::process::Termination {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let output = PathBuf::from(std::env::var("SMOKE_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let options = platform::AppOptions {
        app_id: "com.nsh.capture.smoke".into(),
        title: "NSH capture verification".into(),
        size: (800, 600),
        data_dir: Some(output.join("app-data")),
        ..Default::default()
    };
    platform::run_app(platform::AppSource::Rust(platform::Root::of::<Smoke>().app_id("com.nsh.capture.smoke")), options)
}
async fn until(label: &str, mut ready: impl FnMut() -> bool) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready() {
        if Instant::now() >= deadline { return Err(format!("timed out: {label}")); }
        timer(Duration::from_millis(25)).await;
    }
    Ok(())
}
fn check_capture(png: &[u8], expected: [u8; 3]) -> Result<(), String> {
    let image = image::load_from_memory(png).map_err(|e| e.to_string())?.into_rgba8();
    if image.width() < 240 || image.height() < 120 { return Err("capture dimensions".into()); }
    let pixel = image.get_pixel(image.width()/2, image.height()/2).0;
    if !(0..3).all(|i| pixel[i].abs_diff(expected[i]) <= 12) { return Err(format!("wrong capture content: {pixel:?}")); }
    Ok(())
}
#[component]
fn Smoke() -> impl IntoElement {
    let url = std::env::var("SMOKE_URL").unwrap();
    let output = PathBuf::from(std::env::var("SMOKE_OUTPUT").unwrap());
    let primary = element_ref();
    let other = element_ref();
    let mounted = signal(true);
    let inspectable = signal(false);
    let ready = signal(0usize);
    let progress = signal(Vec::<Option<Percentage>>::new());
    let errors = signal(Vec::<String>::new());
    spawn(async move {
        let result: Result<(), String> = async {
            until("two native pages", || ready.get() >= 2).await?;
            until("load completion", || progress.get().iter().any(|p| p.is_some_and(|p| p.0 == 1.0))).await?;
            if !progress.get().iter().any(|p| p.is_none_or(|p| p.0 < 1.0)) { return Err("missing initial progress".into()); }
            println!("PASS native load progress");
            let first = primary.capture_page();
            let second = other.capture_page();
            let a = first.await?;
            let b = second.await?;
            check_capture(&a, [20, 40, 60])?;
            check_capture(&b, [90, 70, 30])?;
            std::fs::write(output.join("primary.png"), a).map_err(|e| e.to_string())?;
            std::fs::write(output.join("other.png"), b).map_err(|e| e.to_string())?;
            println!("PASS concurrent native captures stay correlated");
            inspectable.set(true);
            primary.measure().await;
            #[cfg(target_os = "macos")]
            if primary.open_devtools().await.is_ok() { return Err("macOS opened unsupported devtools".into()); }
            #[cfg(not(target_os = "macos"))]
            primary.open_devtools().await?;
            inspectable.set(false);
            primary.measure().await;
            println!("PASS reactive inspection and platform open-tools contract");
            mounted.set(false);
            timer(Duration::from_millis(200)).await;
            if primary.capture_page().await.is_ok() { return Err("disposed view captured".into()); }
            check_capture(&other.capture_page().await?, [90, 70, 30])?;
            println!("PASS disposed capture fails and unrelated view survives");
            Ok(())
        }.await;
        match result {
            Ok(()) => { println!("WEBVIEW_SMOKE_PASS"); std::process::exit(0); }
            Err(error) => { eprintln!("WEBVIEW_SMOKE_FAIL: {error}\nNative errors: {:?}", errors.get()); std::process::exit(1); }
        }
    });
    let primary_url = format!("{url}/a");
    view().flex_1().child(view().flex_row().children([
        when(move || mounted.get(), move || web_view(primary_url.clone())
            .element_ref(&primary).w(px(320.0)).h(px(240.0)).inspectable(inspectable)
            .on_load(move |_: &WebNavigation| ready.update(|n| *n += 1))
            .on_load_progress(move |event: &WebProgress| progress.update(|p| p.push(event.progress)))
            .on_error(move |e: &str| errors.update(|v| v.push(e.into())))
            .on_unavailable(move |e: &str| errors.update(|v| v.push(e.into())))).into_element(),
        web_view(format!("{url}/b")).element_ref(&other).w(px(320.0)).h(px(240.0))
            .on_load(move |_: &WebNavigation| ready.update(|n| *n += 1))
            .on_unavailable(move |e: &str| errors.update(|v| v.push(e.into()))).into_element(),
    ]))
}
