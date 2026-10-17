//! Locates the prebuilt runtime: native executables and libraries that
//! apps are packaged with, so building an app never needs Rust.
//!
//! Distribution layout (`cargo xtask dist`), next to `bin/nsh`:
//!
//! ```text
//! runtime/ios/iphoneos/nsh-ios-app           release runner (device)
//! runtime/ios/iphonesimulator/nsh-ios-app    release runner (simulator)
//! runtime/ios/iphonesimulator/nsh-ios-host   development host (simulator)
//! runtime/ios/maccatalyst/nsh-ios-host       development host (Mac Catalyst)
//! runtime/linux/nsh-linux-host               development host (Linux, GTK)
//! runtime/linux/nsh-linux-app                release runner (Linux)
//! runtime/macos/nsh-macos-host               development host (macOS, AppKit)
//! runtime/macos/nsh-macos-app                release runner (macOS)
//! runtime/windows/nsh-windows-host.exe       development host (Windows)
//! runtime/windows/nsh-windows-app.exe        release runner (Windows)
//! runtime/android/release/             classes.dex + lib/<abi>/libnsh.so
//! runtime/android/dev/                 the same with development support
//! runtime/web/nsh.wasm                 the web runtime (`nsh-platform-web`)
//! runtime/web/nsh.js                   the DOM runtime that loads it
//! ```
//!
//! In a cargo checkout, the same files come from `target/<triple>/<profile>`
//! (with the profile of this CLI; Linux's, macOS's and Windows' from
//! `target/<profile>`, as they are built for this machine), and are built
//! with cargo first, so a run
//! never uses a missing or stale host. The web runtime is always built with
//! the `web` profile (a debug build is several times larger and slower), into
//! `target/wasm32-unknown-unknown/web/`, its `nsh.js` read from the crate.
//! `NSH_RUNTIME` overrides the root.
//! An app with a Rust crate of its own gets hosts built for it instead
//! (`rust_app.rs`).

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};

/// A runtime platform directory and its cargo target triple.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Platform {
    /// iOS devices.
    IphoneOs,
    /// The iOS simulator.
    IphoneSimulator,
    /// Mac Catalyst.
    MacCatalyst,
    /// Linux (GTK), on this machine.
    Linux,
    /// macOS (AppKit), on this machine.
    MacOs,
    /// Windows (Win32), on this machine.
    Windows,
}

impl Platform {
    fn dir(self) -> &'static str {
        match self {
            Platform::IphoneOs => "ios/iphoneos",
            Platform::IphoneSimulator => "ios/iphonesimulator",
            Platform::MacCatalyst => "ios/maccatalyst",
            Platform::Linux => "linux",
            Platform::MacOs => "macos",
            Platform::Windows => "windows",
        }
    }

    /// The cargo target triple (`None`: this machine's).
    pub(crate) fn triple(self) -> Option<&'static str> {
        match self {
            Platform::IphoneOs => Some("aarch64-apple-ios"),
            Platform::IphoneSimulator => Some("aarch64-apple-ios-sim"),
            Platform::MacCatalyst => Some("aarch64-apple-ios-macabi"),
            Platform::Linux | Platform::MacOs | Platform::Windows => None,
        }
    }

    /// The development host (`runner` false) or release runner binary:
    /// its package, and its file without the executable suffix.
    pub(crate) fn binary(self, runner: bool) -> &'static str {
        match (self, runner) {
            (Platform::Linux, false) => "nsh-linux-host",
            (Platform::Linux, true) => "nsh-linux-app",
            (Platform::MacOs, false) => "nsh-macos-host",
            (Platform::MacOs, true) => "nsh-macos-app",
            (Platform::Windows, false) => "nsh-windows-host",
            (Platform::Windows, true) => "nsh-windows-app",
            (_, false) => "nsh-ios-host",
            (_, true) => "nsh-ios-app",
        }
    }

    /// The file of [`Platform::binary`]: with this machine's executable
    /// suffix (`.exe`) when it is built for this machine.
    pub(crate) fn file_name(self, runner: bool) -> String {
        let suffix = if self.triple().is_none() {
            env::consts::EXE_SUFFIX
        } else {
            ""
        };
        format!("{}{suffix}", self.binary(runner))
    }

    /// The development host (`runner` false) or release runner for it, as
    /// steps name it: "the development host for the iOS simulator".
    pub(crate) fn host(self, runner: bool) -> String {
        let what = if runner {
            "release runner"
        } else {
            "development host"
        };
        let platform = match self {
            Platform::IphoneOs => "iOS devices",
            Platform::IphoneSimulator => "the iOS simulator",
            Platform::MacCatalyst => "Mac Catalyst",
            Platform::Linux => "Linux",
            Platform::MacOs => "macOS",
            Platform::Windows => "Windows",
        };
        format!("the {what} for {platform}")
    }
}

/// This `nsh`, through symlinks: an installer links `~/.local/bin/nsh` (or
/// `/opt/homebrew/bin/nsh`) to the distribution's `bin/nsh`, whose runtimes
/// are next to the real file. macOS reports the path as it was run, Linux
/// the resolved one; on Windows nothing links it, and canonical paths there
/// are `\\?\` ones that other tools reject.
pub(crate) fn cli_exe() -> std::io::Result<PathBuf> {
    let exe = env::current_exe()?;
    if cfg!(windows) {
        return Ok(exe);
    }
    Ok(exe.canonicalize().unwrap_or(exe))
}

/// The Android runtime directory (`classes.dex`, `lib/<abi>/libnsh.so`),
/// with development support or not.
pub(crate) fn android(dev: bool) -> anyhow::Result<PathBuf> {
    let prebuilt = prebuilt_android(dev)?;
    match crate::rust_app::current() {
        // The app's own library with the prebuilt Java runtime.
        Some(app) => app.android(&prebuilt, dev),
        None => Ok(prebuilt),
    }
}

fn prebuilt_android(dev: bool) -> anyhow::Result<PathBuf> {
    let variant = if dev { "dev" } else { "release" };
    if let Some(root) = env::var_os("NSH_RUNTIME") {
        return android_existing(PathBuf::from(root).join("android").join(variant), dev);
    }
    let exe = cli_exe()?;
    let bin = exe.parent().context("cli has no parent directory")?;
    let dist = bin.join("../runtime/android").join(variant);
    if dist.join("classes.dex").exists() {
        return Ok(dist);
    }
    // Cargo checkout: `cargo xtask android [--release] [--dev]` output.
    let profile = bin
        .file_name()
        .context("cli profile dir")?
        .to_string_lossy();
    if let Some(root) = checkout(bin) {
        let mut args = vec!["xtask", "android"];
        if profile == "release" {
            args.push("--release");
        }
        if dev {
            args.push("--dev");
        }
        let what = if dev {
            "the Android runtime with development support"
        } else {
            "the Android runtime"
        };
        cargo(&root, &args, what)?;
    }
    let name = if dev {
        format!("{profile}-dev")
    } else {
        profile.into_owned()
    };
    let built = bin
        .parent()
        .context("cli is not in a target directory")?
        .join("android")
        .join(name);
    android_existing(built, dev)
}

fn android_existing(dir: PathBuf, dev: bool) -> anyhow::Result<PathBuf> {
    if dir.join("classes.dex").exists() {
        return Ok(dir);
    }
    bail!(
        "the Android runtime is not at {}\nin a nsh checkout, build it with: cargo xtask android{}",
        dir.display(),
        if dev { " --dev" } else { "" }
    )
}

/// The web runtime: the engine (`nsh.wasm`) and the DOM runtime that
/// loads it (`nsh.js`), which only work together.
pub(crate) struct WebRuntime {
    pub(crate) wasm: PathBuf,
    pub(crate) js: PathBuf,
}

/// The web runtime ([`WebRuntime`]). In a checkout it is built first when
/// `build` is set; without it, the build that is there, if any (`None`).
pub(crate) fn web(build: bool) -> anyhow::Result<Option<WebRuntime>> {
    const WASM: &str = "wasm32-unknown-unknown";
    let found = |dir: PathBuf| {
        let runtime = WebRuntime {
            wasm: dir.join("nsh.wasm"),
            js: dir.join("nsh.js"),
        };
        (runtime.wasm.exists() && runtime.js.exists()).then_some(runtime)
    };
    if let Some(root) = env::var_os("NSH_RUNTIME") {
        let dir = PathBuf::from(root).join("web");
        return match found(dir.clone()) {
            Some(runtime) => Ok(Some(runtime)),
            None if build => bail!("the web runtime is not in {}", dir.display()),
            None => Ok(None),
        };
    }
    let exe = cli_exe()?;
    let bin = exe.parent().context("cli has no parent directory")?;
    if let Some(runtime) = found(bin.join("../runtime/web")) {
        return Ok(Some(runtime));
    }
    let Some(root) = checkout(bin) else {
        if build {
            bail!(
                "the web runtime is not at {}",
                bin.join("../runtime/web").display()
            );
        }
        return Ok(None);
    };
    if build {
        let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
        let mut command = Command::new(&cargo);
        command
            .args(["build", "--profile", "web", "-p", "nsh-platform-web"])
            .args(["--target", WASM])
            .current_dir(&root);
        // QuickJS' archive needs an `ar` that knows WebAssembly objects.
        let ar = format!("AR_{}", WASM.replace('-', "_"));
        if env::var_os(&ar).is_none() && env::var_os("AR").is_none() {
            command.env(&ar, "llvm-ar");
        }
        crate::ui::cargo(&mut command, "the web runtime")?;
    }
    let wasm = bin
        .parent()
        .context("cli is not in a target directory")?
        .join(WASM)
        .join("web/nsh_platform_web.wasm");
    let runtime = WebRuntime {
        wasm,
        js: root.join("crates/nsh-platform-web/web/nsh.js"),
    };
    if runtime.wasm.exists() {
        return Ok(Some(runtime));
    }
    if build {
        bail!(
            "the web runtime is not at {}\nin a nsh checkout, build it with: cargo build --profile web -p nsh-platform-web --target {WASM}",
            runtime.wasm.display()
        );
    }
    Ok(None)
}

/// Path of the development host (`runner` false) or release runner for
/// `platform` ([`Platform::binary`]): for an app with a Rust crate, its own
/// build.
pub(crate) fn file(platform: Platform, runner: bool) -> anyhow::Result<PathBuf> {
    if let Some(app) = crate::rust_app::current() {
        return app.runtime(platform, runner);
    }
    let name = platform.binary(runner);
    let file_name = platform.file_name(runner);
    if let Some(root) = env::var_os("NSH_RUNTIME") {
        return existing(
            PathBuf::from(root).join(platform.dir()).join(&file_name),
            platform,
            name,
        );
    }
    let exe = cli_exe()?;
    let bin = exe.parent().context("cli has no parent directory")?;
    // Distribution: bin/nsh + runtime/.
    let dist = bin.join("../runtime").join(platform.dir()).join(&file_name);
    if dist.exists() {
        return Ok(dist);
    }
    // Cargo checkout: target/<profile>/nsh -> target/<triple>/<profile>/<name>.
    let profile = bin.file_name().context("cli profile dir")?;
    if let Some(root) = checkout(bin) {
        let mut args = vec!["build", "-p", name];
        if let Some(triple) = platform.triple() {
            args.extend(["--target", triple]);
        }
        match profile.to_str() {
            Some("debug") => {}
            Some("release") => args.push("--release"),
            Some(other) => args.extend(["--profile", other]),
            None => bail!("cli profile dir is not UTF-8"),
        }
        // The runner is "release" as opposed to the development host; the
        // build follows the CLI's own profile, so say which one it is.
        let what = match profile.to_str() {
            Some("release") => platform.host(runner),
            Some(other) => format!("{} ({other} build)", platform.host(runner)),
            None => platform.host(runner),
        };
        cargo(&root, &args, &what)?;
    }
    let mut built = bin
        .parent()
        .context("cli is not in a target directory")?
        .to_path_buf();
    if let Some(triple) = platform.triple() {
        built.push(triple);
    }
    built.push(profile);
    built.push(file_name);
    existing(built, platform, name)
}

/// The workspace of a cargo checkout of nsh this CLI was built in
/// (`<root>/target/<profile>/nsh`), if it is one.
fn checkout(bin: &Path) -> Option<PathBuf> {
    let root = bin.parent()?.parent()?;
    root.join("crates/nsh-ios-host/Cargo.toml")
        .exists()
        .then(|| root.to_owned())
}

/// Runs cargo in the checkout `root` (the one that runs this CLI under
/// `cargo run`, else `cargo` from the path): the step "building <what>".
fn cargo(root: &Path, args: &[&str], what: &str) -> anyhow::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    crate::ui::cargo(Command::new(&cargo).args(args).current_dir(root), what)
}

fn existing(path: PathBuf, platform: Platform, name: &str) -> anyhow::Result<PathBuf> {
    if path.exists() {
        return Ok(path);
    }
    let target = platform
        .triple()
        .map(|triple| format!(" --target {triple}"))
        .unwrap_or_default();
    bail!(
        "{name} for {} not found at {}\nin a nsh checkout, build it with: cargo build -p {name}{target}",
        platform.dir(),
        path.display(),
    )
}
