//! Apps with Rust components of their own: `"rust": "<crate dir>"` in
//! `nsh.json` names a library crate with a `#[native_module("<specifier>")]`
//! function at its root, which registers its components and modules into
//! a UI set the app imports by that specifier. The attribute makes the
//! entry the generated code calls ([`ENTRY`]).
//!
//! The prebuilt toolchain cannot know such components, so `nsh` builds the
//! app its own: a Cargo workspace in `.nsh/rust` with the command line
//! (`nsh_cli::main_with` of the mobile UI and the app's set), the iOS,
//! macOS, Linux, Windows and GPUI development hosts and release runners,
//! and the Android library,
//! each linking the app's crate. A command on the app runs again in the
//! app's command line ([`reexec`]), which builds the hosts it needs
//! ([`RustApp::runtime`], [`RustApp::android`]) instead of taking the prebuilt
//! ones. The workspace builds against nsh's sources (the checkout this
//! `nsh` was built in, or `NSH_SOURCE`), sharing their `Cargo.lock` and
//! target directory, so only the app's code and what links it compile.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};

use crate::{runtime::Platform, ui};

/// The function `#[native_module]` makes at the root of the app's crate
/// (`nsh-macros` `native_module.rs`): the app's UI set.
const ENTRY: &str = "__nsh_native_module";

/// Set in the app's command line: which app it is for (its workspace).
const APP_ENV: &str = "NSH_RUST_APP";

/// Set in the app's command line when it runs again after a change
/// ([`RESTART`]): a failed build then waits for the next change instead of
/// ending the development session.
const RESTARTED_ENV: &str = "NSH_RUST_RESTARTED";

/// The exit code of the app's command line when the app's Rust code changed
/// during `nsh dev` / `nsh run`: `nsh` builds it again and runs the command
/// anew ([`reexec`]).
pub(crate) const RESTART: u8 = 75;

/// The error a development session ends with when the app's Rust code
/// changed ([`RESTART`]).
#[derive(Debug)]
pub(crate) struct Restart;

impl std::fmt::Display for Restart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the app's Rust code changed")
    }
}

impl std::error::Error for Restart {}

/// An app with a Rust crate of its own.
#[derive(Debug, Clone)]
pub(crate) struct RustApp {
    /// The generated workspace (`<app>/.nsh/rust`).
    workspace: PathBuf,
    /// nsh's sources.
    source: PathBuf,
    /// The app crate's package name (`gallery-native`).
    package: String,
    /// The app crate's directory.
    crate_dir: PathBuf,
}

/// The `"rust"` crate of the app whose entry is `entry`, if it has one.
pub(crate) fn of(entry: &Path) -> anyhow::Result<Option<RustApp>> {
    let entry = dunce::canonicalize(entry)
        .with_context(|| format!("entry not found: {}", entry.display()))?;
    let dir = entry.parent().context("entry has no directory")?;
    let Ok(text) = fs::read_to_string(dir.join("nsh.json")) else {
        return Ok(None);
    };
    let config: serde_json::Value = serde_json::from_str(&text).context("nsh.json is not JSON")?;
    let Some(rust) = config.get("rust") else {
        return Ok(None);
    };
    let rust = rust
        .as_str()
        .context("nsh.json: \"rust\" is the path of the app's crate")?;
    let crate_dir = dunce::canonicalize(dir.join(rust))
        .with_context(|| format!("nsh.json: \"rust\": no crate at {rust}"))?;
    let package = package_name(&crate_dir.join("Cargo.toml"))?;
    let app = RustApp {
        workspace: dir.join(".nsh/rust"),
        source: source()?,
        package,
        crate_dir,
    };
    app.write_workspace()?;
    Ok(Some(app))
}

/// The app this command line runs for (`None`: the prebuilt `nsh`).
pub(crate) fn current() -> Option<RustApp> {
    let workspace = PathBuf::from(env::var_os(APP_ENV)?);
    let text = fs::read_to_string(workspace.join("app.json")).ok()?;
    let app: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(RustApp {
        workspace,
        source: PathBuf::from(app.get("source")?.as_str()?),
        package: app.get("package")?.as_str()?.to_owned(),
        crate_dir: PathBuf::from(app.get("crate")?.as_str()?),
    })
}

/// When this is the prebuilt `nsh` and the command's app has a Rust crate:
/// builds the app's command line and runs the command in it.
pub(crate) fn reexec(entry: Option<&Path>) -> anyhow::Result<Option<std::process::ExitStatus>> {
    if env::var_os(APP_ENV).is_some() {
        return Ok(None);
    }
    let Some(entry) = entry else {
        return Ok(None);
    };
    let Some(app) = of(entry)? else {
        return Ok(None);
    };
    let mut restarted = false;
    loop {
        let what = format!("the Rust crate {}", app.package);
        let cli = match app.build(&app.workspace, &app.bin("cli"), None, false, &[], &what) {
            Ok(cli) => cli,
            Err(e) if restarted => {
                app.wait_after(&e)?;
                continue;
            }
            Err(e) => return Err(e),
        };
        let mut command = Command::new(&cli);
        command
            .args(env::args_os().skip(1))
            .env(APP_ENV, &app.workspace);
        if restarted {
            command.env(RESTARTED_ENV, "1");
        }
        let status = {
            // Ctrl-C reaches both: this one waits while that one undoes
            // what it set up (a stop sent to this one alone is passed on).
            let _stoppable = ui::Stoppable::new();
            let mut child = command
                .spawn()
                .with_context(|| format!("run {}", cli.display()))?;
            // It (and the host it runs) goes with this one, however this
            // one ends.
            #[cfg(windows)]
            let _job = crate::run::KillOnClose::assign(&child);
            let mut passed_on = false;
            loop {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                if !passed_on && ui::stop_requested() {
                    ui::pass_stop_on(&child);
                    passed_on = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        };
        if status.code() != Some(i32::from(RESTART)) || ui::stop_requested() {
            return Ok(Some(status));
        }
        restarted = true;
        ui::note("the app's Rust code changed: building it again");
    }
}

/// nsh's sources: `NSH_SOURCE`, or the checkout this `nsh` was built in.
fn source() -> anyhow::Result<PathBuf> {
    if let Some(dir) = env::var_os("NSH_SOURCE") {
        return Ok(PathBuf::from(dir));
    }
    // target/<profile>/nsh (or target/<triple>/<profile>/...).
    let exe = crate::runtime::cli_exe()?;
    for dir in exe.ancestors().skip(1) {
        if dir.join("crates/nsh-cli/Cargo.toml").exists() {
            return Ok(dir.to_path_buf());
        }
    }
    bail!(
        "an app with a Rust crate (nsh.json \"rust\") builds against nsh's sources: set NSH_SOURCE to a nsh checkout"
    )
}

/// The `name` of the `[package]` of `manifest`.
fn package_name(manifest: &Path) -> anyhow::Result<String> {
    let text = fs::read_to_string(manifest)
        .with_context(|| format!("nsh.json: \"rust\": no {}", manifest.display()))?;
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package
            && let Some(value) = line.strip_prefix("name")
            && let Some(value) = value.trim_start().strip_prefix('=')
        {
            return Ok(value.trim().trim_matches('"').to_owned());
        }
    }
    bail!("{}: no [package] name", manifest.display())
}

/// Writes `path` when its contents differ (Cargo rebuilds on changes).
fn write(path: &Path, contents: &str) -> anyhow::Result<()> {
    if fs::read_to_string(path).is_ok_and(|old| old == contents) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

impl RustApp {
    /// Its binaries' names: unique per app, as they share nsh's target
    /// directory (`nsh-app-gallery-native-cli`).
    fn bin(&self, what: &str) -> String {
        format!("nsh-app-{}-{what}", self.package)
    }

    /// Whether `path` is part of the app's Rust code (its sources and
    /// manifest, and the files of its `assets/` directory: the crate reads
    /// them when it builds).
    pub(crate) fn is_rust(&self, path: &Path) -> bool {
        let Ok(rest) = path.strip_prefix(&self.crate_dir) else {
            return false;
        };
        !rest.starts_with("target")
            && (rest.starts_with("assets")
                || path.extension().is_some_and(|e| e == "rs")
                || path.file_name().is_some_and(|n| n == "Cargo.toml"))
    }

    /// The app crate's directory (watched in development).
    pub(crate) fn crate_dir(&self) -> &Path {
        &self.crate_dir
    }

    /// Reports `error` (a build of the code just saved failed) and waits for
    /// the app's Rust code to change again.
    fn wait_after(&self, error: &anyhow::Error) -> anyhow::Result<()> {
        use notify::{RecursiveMode, Watcher};

        ui::report(error);
        ui::detail(ui::dim("fix the code and save: nsh builds it again"));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event {
                    let _ = tx.send(event);
                }
            })?;
        watcher.watch(&self.crate_dir, RecursiveMode::Recursive)?;
        for event in &rx {
            // Reads (inotify reports them) change nothing.
            if !matches!(event.kind, notify::EventKind::Access(_))
                && event.paths.iter().any(|p| self.is_rust(p))
            {
                // Editors save in several writes: let them settle.
                std::thread::sleep(std::time::Duration::from_millis(100));
                while rx.try_recv().is_ok() {}
                return Ok(());
            }
        }
        bail!("stopped watching {}", self.crate_dir.display())
    }

    /// The generated workspace.
    fn write_workspace(&self) -> anyhow::Result<()> {
        let crate_dir = &self.crate_dir;
        let src = |c: &str| {
            let path = self.source.join("crates").join(c);
            format!("{{ path = {:?} }}", path.to_string_lossy())
        };
        let app = format!(
            "app = {{ package = {:?}, path = {:?} }}",
            self.package,
            crate_dir.to_string_lossy()
        );
        let package = |name: &str| {
            format!(
                "[package]\nname = {:?}\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n",
                self.bin(name)
            )
        };
        let sets = "vec![nsh_ui_mobile::registry(), app_set()]";
        let ws = &self.workspace;
        write(
            &ws.join("Cargo.toml"),
            "# Generated by nsh for the app's Rust crate (nsh.json \"rust\"): do not edit.\n\
             [workspace]
resolver = \"3\"
members = [\"cli\", \"ios-host\", \"ios-app\", \"macos-host\", \"macos-app\", \"linux-host\", \"linux-app\", \"windows-host\", \"windows-app\", \"android\"]
# GPUI's hosts are a workspace of their own, written when they are built
# (`RustApp::gpui_workspace`): other builds never resolve GPUI's crates.
exclude = [\"gpui\"]
",
        )?;
        write(
            &ws.join("app.json"),
            &serde_json::json!({
                "source": self.source.to_string_lossy(),
                "package": self.package,
                "crate": self.crate_dir.to_string_lossy(),
            })
            .to_string(),
        )?;
        // The same versions as nsh's own build.
        let lock = ws.join("Cargo.lock");
        if !lock.exists() {
            // The GPUI workspace's own lock (`crates/gpui`).
            fs::copy(self.source.join("crates/gpui/Cargo.lock"), &lock)
                .context("copy the GPUI workspace's Cargo.lock")?;
        }
        write(
            &ws.join("cli/Cargo.toml"),
            &format!(
                "{}\n[[bin]]\nname = {:?}\npath = \"main.rs\"\n\n[dependencies]\nnsh-cli = {}\nnsh-core = {}\nnsh-ui-mobile = {}\n{app}\n",
                package("cli"),
                self.bin("cli"),
                src("nsh-cli"),
                src("nsh-core"),
                src("nsh-ui-mobile"),
            ),
        )?;
        write(
            &ws.join("cli/main.rs"),
            &format!(
                "//! `nsh` for this app: the mobile UI and the app's Rust components.\n\n\
                 fn main() -> std::process::ExitCode {{\n    nsh_cli::main_with({sets})\n}}\n{}",
                app_set("")
            ),
        )?;
        for (dir, lib, what) in [
            ("ios-host", "nsh-ios-host", "development host"),
            ("ios-app", "nsh-ios-app", "release runner"),
            ("linux-host", "nsh-linux-host", "development host"),
            ("linux-app", "nsh-linux-app", "release runner"),
            ("macos-host", "nsh-macos-host", "development host"),
            ("macos-app", "nsh-macos-app", "release runner"),
            ("windows-host", "nsh-windows-host", "development host"),
            ("windows-app", "nsh-windows-app", "release runner"),
        ] {
            self.write_host(ws, dir, lib, what)?;
        }
        write(
            &ws.join("android/Cargo.toml"),
            &format!(
                "{}\n[lib]\nname = \"nsh_app\"\npath = \"lib.rs\"\ncrate-type = [\"cdylib\"]\n\n[features]\ndev = [\"nsh-platform-android/dev\"]\n\n\
                 [dependencies]\nnsh-core = {}\nnsh-platform-android = {}\nnsh-ui-mobile = {}\n{app}\n",
                package("android"),
                src("nsh-core"),
                src("nsh-platform-android"),
                src("nsh-ui-mobile"),
            ),
        )?;
        write(
            &ws.join("android/lib.rs"),
            &format!(
                "//! The app's `libnsh.so`: the Android runtime with the mobile UI and the\n//! app's Rust components.\n\n\
                 #[cfg(target_os = \"android\")]\nnsh::ui_sets!({sets});\n{}",
                app_set("#[cfg(target_os = \"android\")]\n")
            ),
        )?;
        Ok(())
    }

    /// The workspace of the app's GPUI development host and release
    /// runner (`<workspace>/gpui`), written when one of them is built: GPUI
    /// pulls in crates (wry, GStreamer, ...) the other targets never need,
    /// and a member of the main workspace would make every build resolve
    /// them. It has its own copy of nsh's `Cargo.lock` and shares the
    /// target directory.
    fn gpui_workspace(&self) -> anyhow::Result<PathBuf> {
        let ws = self.workspace.join("gpui");
        write(
            &ws.join("Cargo.toml"),
            "# Generated by nsh for the app's Rust crate (nsh.json \"rust\"): do not edit.\n\
             [workspace]\nresolver = \"3\"\nmembers = [\"gpui-host\", \"gpui-app\"]\n",
        )?;
        let lock = ws.join("Cargo.lock");
        if !lock.exists() {
            // The GPUI workspace's own lock (`crates/gpui`).
            fs::copy(self.source.join("crates/gpui/Cargo.lock"), &lock)
                .context("copy the GPUI workspace's Cargo.lock")?;
        }
        self.write_host(&ws, "gpui-host", "nsh-gpui-host", "development host")?;
        self.write_host(&ws, "gpui-app", "nsh-gpui-app", "release runner")?;
        Ok(ws)
    }

    /// Writes the crate `<ws>/<dir>`: the development host or release runner
    /// `lib` (`nsh-linux-host`, ...) with the mobile UI and the app's set.
    fn write_host(&self, ws: &Path, dir: &str, lib: &str, what: &str) -> anyhow::Result<()> {
        let src = |c: &str| {
            let path = nsh_crate(&self.source, c);
            format!("{{ path = {:?} }}", path.to_string_lossy())
        };
        let app = format!(
            "app = {{ package = {:?}, path = {:?} }}",
            self.package,
            self.crate_dir.to_string_lossy()
        );
        let sets = "vec![nsh_ui_mobile::registry(), app_set()]";
        let exit = " -> std::process::ExitCode";
        let (os, name, platform, exit) = if dir.starts_with("ios") {
            ("target_os = \"ios\"", "iOS", "nsh-platform-ios", "")
        } else if dir.starts_with("macos") {
            ("target_os = \"macos\"", "macOS", "nsh-platform-macos", exit)
        } else if dir.starts_with("windows") {
            (
                "target_os = \"windows\"",
                "Windows",
                "nsh-platform-windows",
                exit,
            )
        } else if dir.starts_with("gpui") {
            (
                "any(target_os = \"linux\", target_os = \"macos\", windows)",
                "GPUI",
                "nsh-platform-gpui",
                exit,
            )
        } else {
            ("target_os = \"linux\"", "Linux", "nsh-platform-linux", exit)
        };
        write(
            &ws.join(dir).join("Cargo.toml"),
            &format!(
                "[package]\nname = {:?}\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
                 [[bin]]\nname = {:?}\npath = \"main.rs\"\n\n[dependencies]\n{lib} = {}\nnsh-core = {}\nnsh-ui-mobile = {}\n{app}\n\n\
                 [target.'cfg({os})'.dependencies]\n{platform} = {}\n",
                self.bin(dir),
                self.bin(dir),
                src(lib),
                src("nsh-core"),
                src("nsh-ui-mobile"),
                src(platform),
            ),
        )?;
        let lib = lib.replace('-', "_");
        let platform = platform.replace('-', "_");
        // A windowed program: no console opens with the app.
        let subsystem = if dir == "windows-app" || dir == "gpui-app" {
            "\n#![cfg_attr(windows, windows_subsystem = \"windows\")]\n"
        } else {
            ""
        };
        write(
            &ws.join(dir).join("main.rs"),
            &format!(
                "//! The {name} {what} with the mobile UI and the app's Rust components.\n{subsystem}\n\
                 fn main(){exit} {{\n    #[cfg({os})]\n    {platform}::set_ui_sets({sets});\n    {lib}::main()\n}}\n{}",
                app_set(&format!("#[cfg({os})]\n"))
            ),
        )
    }

    /// The target directory: nsh's own (its dependencies are built there
    /// already).
    fn target_dir(&self) -> PathBuf {
        self.source.join("target")
    }

    /// `cargo build`s package `package` of the workspace `workspace` (for
    /// `triple`, else the host), the step "building <what>", and returns its
    /// binary (named as the package, with the executable suffix of this
    /// machine when built for it).
    fn build(
        &self,
        workspace: &Path,
        package: &str,
        triple: Option<&str>,
        release: bool,
        extra: &[&str],
        what: &str,
    ) -> anyhow::Result<PathBuf> {
        let suffix = if triple.is_none() {
            env::consts::EXE_SUFFIX
        } else {
            ""
        };
        let artifact = format!("{package}{suffix}");
        let build = Build {
            workspace,
            package,
            triple,
            release,
            extra,
            artifact: &artifact,
            what,
        };
        self.build_with(Command::new(cargo()), &build)
    }

    fn build_with(&self, mut cmd: Command, build: &Build<'_>) -> anyhow::Result<PathBuf> {
        let Build {
            workspace,
            package,
            triple,
            release,
            extra,
            artifact,
            what,
        } = *build;
        cmd.arg("build")
            .arg("--manifest-path")
            .arg(workspace.join("Cargo.toml"))
            .args(["-p", package])
            .args(extra)
            .env("CARGO_TARGET_DIR", self.target_dir());
        if let Some(triple) = triple {
            cmd.args(["--target", triple]);
        }
        if release {
            cmd.arg("--release");
        }
        if let Err(error) = ui::cargo(&mut cmd, what) {
            // A development session that ran: a mistake in the code just
            // saved waits for the fix (the command line runs again).
            if env::var_os(RESTARTED_ENV).is_some() {
                self.wait_after(&error)?;
                return Err(Restart.into());
            }
            return Err(error);
        }
        let mut path = self.target_dir();
        if let Some(triple) = triple {
            path.push(triple);
        }
        path.push(if release { "release" } else { "debug" });
        path.push(artifact);
        Ok(path)
    }

    /// The development host (`runner` false) or release runner of the app,
    /// built for `platform`.
    pub(crate) fn runtime(&self, platform: Platform, runner: bool) -> anyhow::Result<PathBuf> {
        let name = match (platform, runner) {
            (Platform::Linux, false) => "linux-host",
            (Platform::Linux, true) => "linux-app",
            (Platform::MacOs, false) => "macos-host",
            (Platform::MacOs, true) => "macos-app",
            (Platform::Windows, false) => "windows-host",
            (Platform::Windows, true) => "windows-app",
            (Platform::Gpui, false) => "gpui-host",
            (Platform::Gpui, true) => "gpui-app",
            (_, false) => "ios-host",
            (_, true) => "ios-app",
        };
        let what = platform.host(runner);
        let workspace = match platform {
            Platform::Gpui => self.gpui_workspace()?,
            _ => self.workspace.clone(),
        };
        let package = self.bin(name);
        self.build(&workspace, &package, platform.triple(), runner, &[], &what)
    }

    /// The Android runtime of the app: the prebuilt runtime's `classes.dex`
    /// with the app's `libnsh.so` for each of its ABIs.
    pub(crate) fn android(&self, prebuilt: &Path, dev: bool) -> anyhow::Result<PathBuf> {
        let sdk = crate::android::Sdk::find()?;
        let out = self.workspace.join(if dev {
            "android-dev"
        } else {
            "android-release"
        });
        fs::create_dir_all(&out)?;
        install(&prebuilt.join("classes.dex"), &out.join("classes.dex"))?;
        for entry in fs::read_dir(prebuilt.join("lib"))? {
            let abi = entry?.file_name().to_string_lossy().into_owned();
            let triple = match abi.as_str() {
                "arm64-v8a" => "aarch64-linux-android",
                "armeabi-v7a" => "armv7-linux-androideabi",
                "x86_64" => "x86_64-linux-android",
                "x86" => "i686-linux-android",
                other => bail!("unknown Android ABI {other}"),
            };
            let features: &[&str] = if dev { &["--features", "dev"] } else { &[] };
            let build = Build {
                workspace: &self.workspace,
                package: &self.bin("android"),
                triple: Some(triple),
                release: !dev,
                extra: features,
                artifact: "libnsh_app.so",
                what: &format!("libnsh.so of {} for {abi}", self.package),
            };
            let lib = self.build_with(sdk.ndk_cargo(triple)?, &build)?;
            let dir = out.join("lib").join(&abi);
            fs::create_dir_all(&dir)?;
            install(&lib, &dir.join("libnsh.so"))?;
        }
        Ok(out)
    }
}

/// A `cargo build` of a package of the workspace ([`RustApp::build_with`]).
#[derive(Clone, Copy)]
struct Build<'a> {
    /// The workspace it is a package of.
    workspace: &'a Path,
    package: &'a str,
    /// The target (`None`: the host).
    triple: Option<&'a str>,
    release: bool,
    /// More arguments (features).
    extra: &'a [&'a str],
    /// The file it makes, in the target directory.
    artifact: &'a str,
    /// What the step says it builds.
    what: &'a str,
}

/// `app_set()`, the app's UI set, for the generated code (each item under
/// `cfg`): the [`ENTRY`] `#[native_module]` makes in the app's crate. The
/// crate is glob-imported in a block, which shadows the module's import of
/// nsh's stand-in, so a crate without the entry fails to build with a
/// message saying what to add rather than "cannot find function".
fn app_set(cfg: &str) -> String {
    format!(
        "\n/// The app's UI set: the entry `#[native_module(\"…\")]` makes in its crate.\n\
         {cfg}fn app_set() -> nsh_core::ui::Registry {{\n    \
         #[allow(unused_imports)]\n    use app::*;\n    {ENTRY}()\n}}\n\n\
         // Without the entry: nsh's stand-in, which fails the build saying what to add.\n\
         {cfg}#[allow(unused_imports)]\nuse nsh_core::ui::__private::no_native_module as {ENTRY};\n"
    )
}

/// Copies `from` to `to` when it is newer.
fn install(from: &Path, to: &Path) -> anyhow::Result<()> {
    let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    if modified(to).is_some() && modified(to) >= modified(from) {
        return Ok(());
    }
    fs::copy(from, to).with_context(|| format!("copy {} to {}", from.display(), to.display()))?;
    Ok(())
}

/// The cargo to run.
fn cargo() -> std::ffi::OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

/// The directory of nsh's crate `name` in the checkout at `source`: the GPUI
/// backend's crates are a workspace of their own, in `crates/gpui`.
fn nsh_crate(source: &Path, name: &str) -> PathBuf {
    match name {
        "nsh-platform-gpui" | "nsh-gpui-host" | "nsh-gpui-app" => {
            source.join("crates/gpui").join(name)
        }
        _ => source.join("crates").join(name),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_crates_assets_are_its_code() {
        let app = super::RustApp {
            workspace: "/app/.nsh/rust".into(),
            source: "/nsh".into(),
            package: "app-native".into(),
            crate_dir: "/app/native".into(),
        };
        for (path, rust) in [
            ("/app/native/src/lib.rs", true),
            ("/app/native/Cargo.toml", true),
            // `asset!` reads them when the crate builds.
            ("/app/native/assets/logo@2x.png", true),
            ("/app/native/assets/fonts/Script.ttf", true),
            ("/app/native/target/debug/build.rs", false),
            ("/app/native/README.md", false),
            ("/app/App.tsx", false),
        ] {
            assert_eq!(app.is_rust(std::path::Path::new(path)), rust, "{path}");
        }
    }

    #[test]
    fn package_names_come_from_the_package_table() {
        let dir = std::env::temp_dir().join(format!("nsh-rust-app-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[workspace]\nname = \"no\"\n\n[package]\nname = \"gallery-native\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert_eq!(super::package_name(&manifest).unwrap(), "gallery-native");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_generated_code_calls_the_entry_of_the_crate() {
        let dir = std::env::temp_dir().join(format!("nsh-rust-app-ws-{}", std::process::id()));
        let app = super::RustApp {
            workspace: dir.join(".nsh/rust"),
            source: std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
            package: "app-native".into(),
            crate_dir: dir.join("native"),
        };
        app.write_workspace().unwrap();
        let read = |file: &str| std::fs::read_to_string(app.workspace.join(file)).unwrap();
        // GPUI's hosts are written only for a GPUI build, in a workspace of
        // their own, so other builds never resolve GPUI's crates.
        let manifest = read("Cargo.toml");
        assert!(!manifest.contains("gpui-"), "{manifest}");
        assert!(manifest.contains("exclude = [\"gpui\"]"), "{manifest}");
        assert!(!app.workspace.join("gpui").exists());
        assert_eq!(app.gpui_workspace().unwrap(), app.workspace.join("gpui"));
        assert!(read("gpui/Cargo.toml").contains("members = [\"gpui-host\", \"gpui-app\"]"));
        assert!(app.workspace.join("gpui/Cargo.lock").exists());
        for (file, cfg) in [
            ("cli/main.rs", ""),
            ("ios-host/main.rs", "#[cfg(target_os = \"ios\")]\n"),
            ("ios-app/main.rs", "#[cfg(target_os = \"ios\")]\n"),
            ("linux-host/main.rs", "#[cfg(target_os = \"linux\")]\n"),
            ("linux-app/main.rs", "#[cfg(target_os = \"linux\")]\n"),
            ("macos-host/main.rs", "#[cfg(target_os = \"macos\")]\n"),
            ("macos-app/main.rs", "#[cfg(target_os = \"macos\")]\n"),
            ("windows-host/main.rs", "#[cfg(target_os = \"windows\")]\n"),
            ("windows-app/main.rs", "#[cfg(target_os = \"windows\")]\n"),
            (
                "gpui/gpui-host/main.rs",
                "#[cfg(any(target_os = \"linux\", target_os = \"macos\", windows))]\n",
            ),
            (
                "gpui/gpui-app/main.rs",
                "#[cfg(any(target_os = \"linux\", target_os = \"macos\", windows))]\n",
            ),
            ("android/lib.rs", "#[cfg(target_os = \"android\")]\n"),
        ] {
            let code = read(file);
            for line in [
                "vec![nsh_ui_mobile::registry(), app_set()]".to_owned(),
                format!(
                    "{cfg}fn app_set() -> nsh_core::ui::Registry {{\n    #[allow(unused_imports)]\n    use app::*;\n    __nsh_native_module()\n}}"
                ),
                format!(
                    "{cfg}#[allow(unused_imports)]\nuse nsh_core::ui::__private::no_native_module as __nsh_native_module;"
                ),
            ] {
                assert!(code.contains(&line), "{file}: no {line:?} in\n{code}");
            }
        }
        for dir in [
            "cli",
            "ios-host",
            "ios-app",
            "linux-host",
            "linux-app",
            "macos-host",
            "macos-app",
            "windows-host",
            "windows-app",
            "gpui/gpui-host",
            "gpui/gpui-app",
            "android",
        ] {
            assert!(
                read(&format!("{dir}/Cargo.toml")).contains("\nnsh-core = "),
                "{dir}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
