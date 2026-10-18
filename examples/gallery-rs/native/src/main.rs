//! Runs the Rust application directly on the machine's native backend.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> impl std::process::Termination {
    env_logger::init();
    let gallery = gallery_native::__nsh_native_module();
    let assets = asset_directory(&gallery).expect("copy the gallery's registered assets");
    platform::set_ui_sets(vec![nsh_ui_mobile::registry(), gallery]);
    let theme = nsh_core::ui::Theme::from_json(include_str!("../../theme.json"))
        .expect("the gallery theme is valid");
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<gallery_native::Gallery>().theme(theme)),
        platform::AppOptions {
            title: "Rust Gallery".to_owned(),
            asset_base: Some(platform::AssetBase::Directory(assets)),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            app_id: "com.example.rustgallery".to_owned(),
            size: (1200, 800),
            script: std::env::var("NSH_SCRIPT").ok(),
            ..Default::default()
        },
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!(
    "Run this binary on macOS, Windows or Linux; use the TSX entry with nsh run for iOS and Android."
);

// Registered asset URIs keep the same layout as the TSX host's served assets.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn asset_directory(registry: &nsh_core::ui::Registry) -> std::io::Result<std::path::PathBuf> {
    let executable = std::env::current_exe()?;
    let base = executable
        .parent()
        .expect("the executable has a directory")
        .join("gallery-rs-assets");
    for file in registry.asset_files() {
        let destination = base.join(file.path());
        std::fs::create_dir_all(
            destination
                .parent()
                .expect("an asset has a parent directory"),
        )?;
        std::fs::copy(file.source, destination)?;
    }
    Ok(base)
}
