//! Apps with Rust components of their own: `"rust": "<crate dir>"` in
//! `nsh.json` (or a list of them, [`crates`]) names library crates, each
//! with a `#[native_module("<specifier>")]` function at its root, which
//! registers its components and modules into a UI set the app imports by
//! that specifier. The attribute makes the entry the generated code calls
//! ([`ENTRY`]); each crate is linked under a name of its own
//! ([`AppCrate::name`]), so their entries do not clash.
//!
//! The prebuilt toolchain cannot know such components, so `nsh` builds the
//! app its own: a Cargo workspace in `.nsh/rust` with the command line
//! (`nsh_cli::main_with` of the mobile UI and the app's sets), the iOS,
//! macOS, Linux, Windows and GPUI development hosts and release runners,
//! the Android library and the web runtime (a WebAssembly module,
//! [`RustApp::web`]),
//! each linking the app's crates. A command on the app runs again in the
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
/// (`nsh-macros` `native_module.rs`): the crate's UI set.
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

/// An app with Rust crates of its own.
#[derive(Debug, Clone)]
pub(crate) struct RustApp {
    /// The generated workspace (`<app>/.nsh/rust`).
    workspace: PathBuf,
    /// nsh's sources.
    source: PathBuf,
    /// The crates `nsh.json` `"rust"` lists, in its order: each is a UI set
    /// of its own, after the mobile UI.
    crates: Vec<AppCrate>,
    /// The directories of the app's Rust code: the crates' and those of the
    /// local crates they depend on by path ([`path_dependencies`]), watched
    /// in development.
    watched: Vec<PathBuf>,
    /// A Rust app (`nsh.json` `"root"`, the entry is `nsh.json`): what its
    /// runners mount, with no JS.
    root: Option<RootConfig>,
}

/// A crate of the app (`nsh.json` `"rust"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppCrate {
    /// Its package name (`gallery-native`).
    package: String,
    /// Its directory.
    dir: PathBuf,
}

impl AppCrate {
    /// The name the generated code links it by (`app_gallery_native`):
    /// one per crate, so each crate's entry is found in its own.
    fn name(&self) -> String {
        format!("app_{}", self.package.replace('-', "_"))
    }

    /// Its line in the `[dependencies]` of a generated crate.
    fn dependency(&self) -> String {
        format!(
            "{} = {{ package = {:?}, path = {:?} }}",
            self.name(),
            self.package,
            self.dir.to_string_lossy()
        )
    }
}

/// The root component of a Rust app (`nsh.json` `"root"`), as the
/// generated runners make it ([`root_fn`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootConfig {
    /// Its path in the app's crate (`App`, `shell::Gallery`).
    component: String,
    /// The theme's JSON file (`nsh.json` `"theme"`: a file, or its content
    /// written to the workspace), compiled into the runners.
    theme: Option<PathBuf>,
    /// `"light"` / `"dark"`; `None`: the system's.
    appearance: Option<String>,
}

/// Whether `entry` is a Rust app's (`entry::resolve` gives its `nsh.json`
/// when it declares a `"root"`): it runs with no JS.
pub(crate) fn is_root(entry: &Path) -> bool {
    entry
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("nsh.json"))
}

/// The `"rust"` crates of the app whose entry is `entry`, if it has any.
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
    let crates = crates(dir, rust)?;
    let source = source()?;
    let watched = watched(&crates, &source);
    let workspace = dir.join(".nsh/rust");
    let root = if is_root(&entry) {
        Some(root_config(dir, &config, &workspace)?)
    } else {
        None
    };
    let app = RustApp {
        workspace,
        source,
        crates,
        watched,
        root,
    };
    app.write_workspace()?;
    Ok(Some(app))
}

/// The crates `rust` (`nsh.json` `"rust"` of the app in `dir`) lists: a
/// crate's directory or a list of them. Each is checked as the workspace
/// is written: two of the same package or the same specifier
/// (`#[native_module("…")]`, [`specifier_of`]) would make two sets one
/// import cannot tell apart.
fn crates(dir: &Path, rust: &serde_json::Value) -> anyhow::Result<Vec<AppCrate>> {
    const WHAT: &str = "nsh.json: \"rust\" is the path of the app's crate (\"native\") or a list of them (\"native/charts\", \"native/storage\")";
    let paths: Vec<&str> = match rust {
        serde_json::Value::String(path) => vec![path],
        serde_json::Value::Array(paths) if !paths.is_empty() => paths
            .iter()
            .map(|p| p.as_str().context(WHAT))
            .collect::<anyhow::Result<_>>()?,
        _ => bail!(WHAT),
    };
    let mut crates: Vec<(&str, AppCrate)> = Vec::new();
    for path in paths {
        let dir = dunce::canonicalize(dir.join(path))
            .with_context(|| format!("nsh.json: \"rust\": no crate at {path}"))?;
        let package = package_name(&dir.join("Cargo.toml"))?;
        let krate = AppCrate { package, dir };
        if let Some((other, _)) = crates.iter().find(|(_, c)| c.dir == krate.dir) {
            bail!("nsh.json: \"rust\" lists the crate {other} twice ({path})");
        }
        // Cargo takes `-` and `_` in names as one: so do the names the
        // generated code links the crates by.
        if let Some((other, first)) = crates.iter().find(|(_, c)| c.name() == krate.name()) {
            let same = if first.package == krate.package {
                format!("are both the package `{}`", krate.package)
            } else {
                format!(
                    "are the packages `{}` and `{}`, one name to Cargo",
                    first.package, krate.package
                )
            };
            bail!(
                "nsh.json: \"rust\": {other} and {path} {same}: give each crate a [package] name of its own"
            );
        }
        crates.push((path, krate));
    }
    let mut specifiers: Vec<(&str, String)> = Vec::new();
    for (path, krate) in &crates {
        let Some(specifier) = specifier_of(&krate.dir) else {
            continue;
        };
        if let Some((other, _)) = specifiers.iter().find(|(_, s)| *s == specifier) {
            bail!(
                "nsh.json: \"rust\": {other} and {path} both have `#[native_module({specifier:?})]`: give each crate a specifier of its own, the one its TSX imports it by"
            );
        }
        specifiers.push((path, specifier));
    }
    Ok(crates.into_iter().map(|(_, c)| c).collect())
}

/// The specifier of the `#[native_module("…")]` at the root of the crate
/// in `dir` (its `[lib] path`, or `src/lib.rs`), if it has one; the build
/// fails on a crate without one ([`RustApp::app_sets`]).
fn specifier_of(dir: &Path) -> Option<String> {
    let manifest = fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let lib = toml_tables(&manifest)
        .find_map(|(table, line)| {
            (table == "lib")
                .then(|| toml_string(line, "path"))
                .flatten()
        })
        .unwrap_or_else(|| "src/lib.rs".to_owned());
    let code = fs::read_to_string(dir.join(lib)).ok()?;
    code.lines().find_map(|line| {
        const ATTR: &str = "native_module";
        let attr = line.trim().strip_prefix("#[")?;
        let rest = attr[attr.find(ATTR)? + ATTR.len()..].trim_start();
        let rest = rest.strip_prefix('(')?.trim_start().strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_owned())
    })
}

/// The directories of the app's Rust code: those of `crates` and of the
/// local crates they depend on by path, transitively, except nsh's own (in
/// `source`).
fn watched(crates: &[AppCrate], source: &Path) -> Vec<PathBuf> {
    let source = dunce::canonicalize(source).unwrap_or_else(|_| source.to_owned());
    let mut dirs: Vec<PathBuf> = crates.iter().map(|c| c.dir.clone()).collect();
    let mut next = 0;
    while let Some(dir) = dirs.get(next) {
        next += 1;
        for dep in path_dependencies(dir) {
            if let Ok(dep) = dunce::canonicalize(dep)
                && !dep.starts_with(&source)
                && !dirs.contains(&dep)
            {
                dirs.push(dep);
            }
        }
    }
    dirs
}

/// The directories of the path dependencies (`path = "../shared"`) of the
/// crate in `dir`, those it builds with (`[dependencies]`,
/// `[build-dependencies]`, per target too).
fn path_dependencies(dir: &Path) -> Vec<PathBuf> {
    let Ok(manifest) = fs::read_to_string(dir.join("Cargo.toml")) else {
        return Vec::new();
    };
    toml_tables(&manifest)
        .filter(|(table, _)| {
            table
                .split('.')
                .any(|part| part == "dependencies" || part == "build-dependencies")
        })
        .filter_map(|(_, line)| toml_string(line, "path"))
        .map(|path| dir.join(path))
        .collect()
}

/// The lines of the TOML document `text` outside table headers, with the
/// name of the table each is in (`dependencies`, `dependencies.shared`,
/// `target.'cfg(unix)'.dependencies`), trimmed.
fn toml_tables(text: &str) -> impl Iterator<Item = (&str, &str)> {
    let mut table = "";
    text.lines().filter_map(move |line| {
        let line = line.trim();
        if line.starts_with('[') {
            table = line.trim_start_matches('[');
            table = table.split(']').next().unwrap_or_default().trim();
            return None;
        }
        Some((table, line))
    })
}

/// The string value of `key` in the TOML `line` (`key = "…"`, also within
/// an inline table: `shared = { path = "../shared" }`).
fn toml_string(line: &str, key: &str) -> Option<String> {
    let mut rest = line;
    while let Some(at) = rest.find(key) {
        let before = rest[..at].trim_end();
        let after = rest[at + key.len()..].trim_start();
        rest = &rest[at + key.len()..];
        if !(before.is_empty() || before.ends_with('{') || before.ends_with(',')) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = &value[1..];
        return Some(value[..value.find(quote)?].to_owned());
    }
    None
}

/// The `"root"` of the app's `config` (its `nsh.json`, in `dir`) with its
/// `"theme"` and `"appearance"`.
fn root_config(
    dir: &Path,
    config: &serde_json::Value,
    workspace: &Path,
) -> anyhow::Result<RootConfig> {
    let component = config
        .get("root")
        .and_then(|r| r.as_str())
        .context("nsh.json: \"root\" is the path of the app's root component in its crate")?
        .trim()
        .to_owned();
    let ident = |s: &str| {
        s.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && s.chars().all(|c| c.is_alphanumeric() || c == '_')
    };
    if !component.split("::").all(ident) {
        bail!("nsh.json: \"root\": {component:?} is not a Rust path (`App`, `shell::Gallery`)");
    }
    let theme = match config.get("theme") {
        None => None,
        Some(serde_json::Value::String(file)) => Some(
            dunce::canonicalize(dir.join(file))
                .with_context(|| format!("nsh.json: theme: no {file}"))?,
        ),
        Some(inline) => {
            let path = workspace.join("theme.json");
            write(&path, &inline.to_string())?;
            Some(path)
        }
    };
    if let Some(theme) = &theme {
        let json = fs::read_to_string(theme)?;
        nsh_core::ui::Theme::from_json(&json)
            .map_err(|e| anyhow::anyhow!("nsh.json: theme: {e}"))?;
    }
    let appearance = match config.get("appearance").and_then(|a| a.as_str()) {
        None | Some("system") => None,
        Some(a @ ("light" | "dark")) => Some(a.to_owned()),
        Some(other) => {
            bail!("nsh.json: appearance: {other:?} is not \"light\", \"dark\" or \"system\"")
        }
    };
    Ok(RootConfig {
        component,
        theme,
        appearance,
    })
}

/// The app this command line runs for (`None`: the prebuilt `nsh`).
pub(crate) fn current() -> Option<RustApp> {
    load(PathBuf::from(env::var_os(APP_ENV)?))
}

/// The app whose generated workspace is `workspace`, as its `app.json`
/// describes it ([`RustApp::write_workspace`]).
fn load(workspace: PathBuf) -> Option<RustApp> {
    let text = fs::read_to_string(workspace.join("app.json")).ok()?;
    let app: serde_json::Value = serde_json::from_str(&text).ok()?;
    let root = app.get("root").and_then(|root| {
        Some(RootConfig {
            component: root.get("component")?.as_str()?.to_owned(),
            theme: root.get("theme")?.as_str().map(PathBuf::from),
            appearance: root.get("appearance")?.as_str().map(str::to_owned),
        })
    });
    let paths = |key: &str| -> Option<Vec<PathBuf>> {
        app.get(key)?
            .as_array()?
            .iter()
            .map(|p| p.as_str().map(PathBuf::from))
            .collect()
    };
    let crates = app
        .get("crates")?
        .as_array()?
        .iter()
        .map(|c| {
            Some(AppCrate {
                package: c.get("package")?.as_str()?.to_owned(),
                dir: PathBuf::from(c.get("dir")?.as_str()?),
            })
        })
        .collect::<Option<_>>()?;
    Some(RustApp {
        workspace,
        source: PathBuf::from(app.get("source")?.as_str()?),
        crates,
        watched: paths("watched")?,
        root,
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
        let what = match app.crates.as_slice() {
            [one] => format!("the Rust crate {}", one.package),
            _ => format!("the Rust crates {}", app.packages()),
        };
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
    /// directory (`nsh-app-gallery-native-cli`; the packages of all its
    /// crates: `nsh-app-charts-storage-cli`).
    fn bin(&self, what: &str) -> String {
        let packages: Vec<&str> = self.crates.iter().map(|c| c.package.as_str()).collect();
        format!("nsh-app-{}-{what}", packages.join("-"))
    }

    /// Its crates' packages, for what the steps say they build
    /// (`charts, storage`).
    fn packages(&self) -> String {
        let packages: Vec<&str> = self.crates.iter().map(|c| c.package.as_str()).collect();
        packages.join(", ")
    }

    /// Whether `path` is part of the app's Rust code (the sources and
    /// manifest of a crate it builds with, and the files of its `assets/`
    /// directory: the crate reads them when it builds).
    pub(crate) fn is_rust(&self, path: &Path) -> bool {
        self.watched.iter().any(|dir| {
            let Ok(rest) = path.strip_prefix(dir) else {
                return false;
            };
            !rest.starts_with("target")
                && !rest.starts_with(".nsh")
                && !rest.starts_with(".git")
                && (rest.starts_with("assets")
                    || path.extension().is_some_and(|e| e == "rs")
                    || path.file_name().is_some_and(|n| n == "Cargo.toml"))
        })
    }

    /// The directories of the app's Rust code (watched in development): its
    /// crates' and those of the local crates they depend on.
    pub(crate) fn watched(&self) -> &[PathBuf] {
        &self.watched
    }

    /// Watches the app's Rust code with `watcher`.
    fn watch_with(&self, watcher: &mut impl notify::Watcher) -> anyhow::Result<()> {
        for dir in &self.watched {
            watcher
                .watch(dir, notify::RecursiveMode::Recursive)
                .with_context(|| format!("watch {}", dir.display()))?;
        }
        Ok(())
    }

    /// Reports `error` (a build of the code just saved failed) and waits for
    /// the app's Rust code to change again.
    fn wait_after(&self, error: &anyhow::Error) -> anyhow::Result<()> {
        ui::report(error);
        ui::detail(ui::dim("fix the code and save: nsh builds it again"));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event {
                    let _ = tx.send(event);
                }
            })?;
        self.watch_with(&mut watcher)?;
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
        bail!("stopped watching the app's Rust code")
    }

    /// The generated workspace.
    fn write_workspace(&self) -> anyhow::Result<()> {
        let src = |c: &str| {
            let path = self.source.join("crates").join(c);
            format!("{{ path = {:?} }}", path.to_string_lossy())
        };
        let app = self.dependencies();
        let package = |name: &str| {
            format!(
                "[package]\nname = {:?}\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n",
                self.bin(name)
            )
        };
        let sets = self.sets();
        let ws = &self.workspace;
        // A Rust app's runners too (`nsh.json` "root"), only then: they
        // name its root component.
        let rust = if self.root.is_some() {
            ", \"rust-linux\", \"rust-macos\", \"rust-windows\", \"rust-ios\", \"rust-android\", \"rust-web\""
        } else {
            ""
        };
        self.write_workspace_manifest(
            ws,
            &format!("# Generated by nsh for the app's Rust crate (nsh.json \"rust\"): do not edit.\n\
             [workspace]
resolver = \"3\"
members = [\"cli\", \"ios-host\", \"ios-app\", \"macos-host\", \"macos-app\", \"linux-host\", \"linux-app\", \"windows-host\", \"windows-app\", \"android\", \"web\"{rust}]
# GPUI's hosts are a workspace of their own, written when they are built
# (`RustApp::gpui_workspace`): other builds never resolve GPUI's crates.
exclude = [\"gpui\"]

# The web runtime, as nsh builds its own (nsh's `Cargo.toml`): Rust for
# size, QuickJS at -O3.
[profile.web]
inherits = \"release\"
opt-level = \"z\"
lto = \"fat\"
codegen-units = 1
strip = true
panic = \"abort\"
[profile.web.package.rquickjs-sys]
opt-level = 3
"),
            false,
        )?;
        write(
            &ws.join("app.json"),
            &serde_json::json!({
                "source": self.source.to_string_lossy(),
                "crates": self.crates.iter().map(|c| serde_json::json!({
                    "package": c.package,
                    "dir": c.dir.to_string_lossy(),
                })).collect::<Vec<_>>(),
                "watched": self.watched.iter().map(|d| d.to_string_lossy()).collect::<Vec<_>>(),
                "root": self.root.as_ref().map(|root| serde_json::json!({
                    "component": root.component,
                    "theme": root.theme.as_ref().map(|t| t.to_string_lossy()),
                    "appearance": root.appearance,
                })),
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
                self.app_sets("")
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
        self.write_web(ws)?;
        if let Some(root) = &self.root {
            self.write_rust_runners(ws, root)?;
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
                self.app_sets("#[cfg(target_os = \"android\")]\n")
            ),
        )?;
        Ok(())
    }

    /// Writes a Rust app's runners (`<ws>/rust-<platform>`): the release
    /// runners of the desktops and iOS ([`Self::write_rust_runner`]), the
    /// Android library and the web runtime, each mounting the app's root
    /// with no JS runtime linked (their nsh crates without the feature
    /// `js`).
    fn write_rust_runners(&self, ws: &Path, root: &RootConfig) -> anyhow::Result<()> {
        for dir in ["rust-linux", "rust-macos", "rust-windows", "rust-ios"] {
            self.write_rust_runner(ws, dir, root)?;
        }
        let src = |c: &str| nsh_crate(&self.source, c).to_string_lossy().into_owned();
        let app = self.dependencies();
        let sets = self.sets();
        let android = "#[cfg(target_os = \"android\")]\n";
        write(
            &ws.join("rust-android/Cargo.toml"),
            &format!(
                "[package]\nname = {:?}\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
                 [lib]\nname = \"nsh_rust_app\"\npath = \"lib.rs\"\ncrate-type = [\"cdylib\"]\n\n\
                 [dependencies]\nnsh-core = {{ path = {:?} }}\nnsh-ui-mobile = {{ path = {:?} }}\n{app}\n\n\
                 [target.'cfg(target_os = \"android\")'.dependencies]\n\
                 nsh-platform-android = {{ path = {:?}, default-features = false }}\n",
                self.bin("rust-android"),
                src("nsh-core"),
                src("nsh-ui-mobile"),
                src("nsh-platform-android"),
            ),
        )?;
        write(
            &ws.join("rust-android/lib.rs"),
            &format!(
                "//! The Rust app's `libnsh.so`: its root, which `AppActivity` runs, with\n\
                 //! the mobile UI and the app's components; no JS runtime.\n\n\
                 {android}nsh::ui_sets!({sets});\n{android}nsh::main!(app_root());\n\n\
                 // Off Android it is empty, as nsh-platform-android is.\n\
                 #[cfg(not(target_os = \"android\"))]\nuse nsh_core as _;\n\
                 #[cfg(not(target_os = \"android\"))]\nuse nsh_ui_mobile as _;\n{}{}",
                self.app_sets(android),
                self.root_fn(android, root),
            ),
        )?;
        let wasm = "#[cfg(target_arch = \"wasm32\")]\n";
        write(
            &ws.join("rust-web/Cargo.toml"),
            &format!(
                "[package]\nname = {:?}\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
                 [lib]\nname = {:?}\npath = \"lib.rs\"\ncrate-type = [\"cdylib\"]\n\n\
                 [dependencies]\nnsh-core = {{ path = {:?} }}\nnsh-ui-mobile = {{ path = {:?} }}\n{app}\n\n\
                 [target.'cfg(target_arch = \"wasm32\")'.dependencies]\n\
                 nsh-platform-web = {{ path = {:?}, default-features = false }}\n",
                self.bin("rust-web"),
                self.rust_web_lib(),
                src("nsh-core"),
                src("nsh-ui-mobile"),
                src("nsh-platform-web"),
            ),
        )?;
        write(&ws.join("rust-web/build.rs"), WEB_BUILD_RS)?;
        write(
            &ws.join("rust-web/lib.rs"),
            &format!(
                "//! The Rust app's web runtime: its root, which `nsh.js` runs, with the\n\
                 //! mobile UI and the app's components; no QuickJS.\n\n\
                 {wasm}nsh_platform_web::rust_app!({sets}, app_root());\n\n\
                 // Off WebAssembly it is empty, as nsh-platform-web is.\n\
                 #[cfg(not(target_arch = \"wasm32\"))]\nuse nsh_core as _;\n\
                 #[cfg(not(target_arch = \"wasm32\"))]\nuse nsh_ui_mobile as _;\n{}{}",
                self.app_sets(wasm),
                self.root_fn(wasm, root),
            ),
        )
    }

    /// Writes the crate `<ws>/<dir>` (`rust-linux`, `rust-macos`,
    /// `rust-windows`, `rust-ios`, `rust-gpui`): the platform's release
    /// runner (`nsh-linux-app`, ...) without the feature `js`, whose
    /// `run_rust` mounts the app's root.
    fn write_rust_runner(&self, ws: &Path, dir: &str, root: &RootConfig) -> anyhow::Result<()> {
        let src = |c: &str| nsh_crate(&self.source, c).to_string_lossy().into_owned();
        let app = self.dependencies();
        let sets = self.sets();
        let (os, lib, platform, features, exit) = match dir {
            "rust-linux" => (
                "target_os = \"linux\"",
                "nsh-linux-app",
                "nsh-platform-linux",
                ", features = [\"webview\"]",
                true,
            ),
            "rust-macos" => (
                "target_os = \"macos\"",
                "nsh-macos-app",
                "nsh-platform-macos",
                "",
                true,
            ),
            "rust-windows" => (
                "windows",
                "nsh-windows-app",
                "nsh-platform-windows",
                "",
                true,
            ),
            "rust-gpui" => (
                "any(target_os = \"linux\", target_os = \"macos\", windows)",
                "nsh-gpui-app",
                "nsh-platform-gpui",
                "",
                true,
            ),
            _ => (
                "target_os = \"ios\"",
                "nsh-ios-app",
                "nsh-platform-ios",
                "",
                false,
            ),
        };
        write(
            &ws.join(dir).join("Cargo.toml"),
            &format!(
                "[package]\nname = {:?}\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
                 [[bin]]\nname = {:?}\npath = \"main.rs\"\n\n\
                 [dependencies]\n{lib} = {{ path = {:?}, default-features = false{features} }}\n\
                 nsh-core = {{ path = {:?} }}\nnsh-ui-mobile = {{ path = {:?} }}\n{app}\n\n\
                 [target.'cfg({os})'.dependencies]\n{platform} = {{ path = {:?}, default-features = false }}\n",
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
        let cfg = format!("#[cfg({os})]\n");
        // A windowed program: no console opens with the app.
        let subsystem = if dir == "rust-windows" || dir == "rust-gpui" {
            "\n#![cfg_attr(windows, windows_subsystem = \"windows\")]\n"
        } else {
            ""
        };
        let (ret, tail) = if exit {
            (" -> std::process::ExitCode", "")
        } else {
            ("", ";")
        };
        write(
            &ws.join(dir).join("main.rs"),
            &format!(
                "//! The Rust app's executable: its root with the mobile UI and the app's\n\
                 //! components, on {lib}'s runner; no JS runtime.\n{subsystem}\n\
                 fn main(){ret} {{\n    \
                 #[cfg({os})]\n    {{\n        \
                 {platform}::set_ui_sets({sets});\n        \
                 {lib}::run_rust(app_root()){tail}\n    }}\n    \
                 #[cfg(not({os}))]\n    {lib}::main(){tail}\n}}\n{}{}",
                self.app_sets(&cfg),
                self.root_fn(&cfg, root),
            ),
        )
    }

    /// The Rust app's web runtime's library name ([`Self::web_lib`]).
    fn rust_web_lib(&self) -> String {
        self.bin("rust-web").replace('-', "_")
    }

    /// Writes the crate `<ws>/web`: the web runtime (`nsh-platform-web`, a
    /// WebAssembly module) with the mobile UI and the app's set, which
    /// `nsh.js` registers through the module's `nsh_app_init`.
    fn write_web(&self, ws: &Path) -> anyhow::Result<()> {
        let src = |c: &str| {
            let path = nsh_crate(&self.source, c);
            format!("{{ path = {:?} }}", path.to_string_lossy())
        };
        let app = self.dependencies();
        let sets = self.sets();
        let wasm = "#[cfg(target_arch = \"wasm32\")]\n";
        write(
            &ws.join("web/Cargo.toml"),
            &format!(
                "[package]\nname = {:?}\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
                 [lib]\nname = {:?}\npath = \"lib.rs\"\ncrate-type = [\"cdylib\"]\n\n\
                 [features]\n# The runtime of published apps: bytecode only (nsh-platform-web).\n\
                 bytecode-only = [\"nsh-platform-web/bytecode-only\"]\n\n\
                 [dependencies]\nnsh-core = {}\nnsh-ui-mobile = {}\n{app}\n\n\
                 [target.'cfg(target_arch = \"wasm32\")'.dependencies]\nnsh-platform-web = {}\n",
                self.bin("web"),
                self.web_lib(),
                src("nsh-core"),
                src("nsh-ui-mobile"),
                src("nsh-platform-web"),
            ),
        )?;
        // nsh's `.cargo/config.toml` gives its own web runtime this stack
        // (QuickJS recursion, deep component trees); the app's build may
        // not run under it, and a link argument of this package leaves the
        // dependencies' builds shared with nsh's.
        write(&ws.join("web/build.rs"), WEB_BUILD_RS)?;
        write(
            &ws.join("web/lib.rs"),
            &format!(
                "//! The app's web runtime: `nsh.wasm` with the mobile UI and the app's\n//! Rust components.\n\n\
                 {wasm}nsh_platform_web::ui_sets!({sets});\n{}\n\
                 // Off WebAssembly it is empty, as nsh-platform-web is.\n\
                 #[cfg(not(target_arch = \"wasm32\"))]\nuse nsh_core as _;\n\
                 #[cfg(not(target_arch = \"wasm32\"))]\nuse nsh_ui_mobile as _;\n",
                self.app_sets(wasm)
            ),
        )
    }

    /// The web runtime's library name (its module is `<name>.wasm`):
    /// unique per app, as the target directory is shared.
    fn web_lib(&self) -> String {
        self.bin("web").replace('-', "_")
    }

    /// The app's web runtime of `kind` (its `nsh.wasm` / `nsh-dev.wasm`, and
    /// nsh's `nsh.js`), built for `wasm32-unknown-unknown` with the `web`
    /// profile as nsh builds its own (`runtime::web`). Without `build`, the
    /// one built before, if any.
    pub(crate) fn web(
        &self,
        kind: crate::runtime::WebKind,
        build: bool,
    ) -> anyhow::Result<Option<crate::runtime::WebRuntime>> {
        const WASM: &str = "wasm32-unknown-unknown";
        let out = self.workspace.join("web-runtime");
        let runtime = crate::runtime::WebRuntime {
            wasm: if self.root.is_some() {
                out.join("nsh-rust.wasm")
            } else {
                out.join(kind.file())
            },
            js: self.source.join("crates/nsh-platform-web/web/nsh.js"),
        };
        if !build {
            return Ok(runtime.wasm.exists().then_some(runtime));
        }
        let mut cmd = Command::new(cargo());
        // QuickJS' archive needs an `ar` that knows WebAssembly objects.
        let ar = format!("AR_{}", WASM.replace('-', "_"));
        if env::var_os(&ar).is_none() && env::var_os("AR").is_none() {
            cmd.env(&ar, "llvm-ar");
        }
        // A Rust app's runtime is one: no QuickJS, so neither the parser
        // nor hot updates.
        let rust = self.root.is_some();
        let features: &[&str] = match kind {
            _ if rust => &[],
            crate::runtime::WebKind::Release => &["--features", "bytecode-only"],
            crate::runtime::WebKind::Dev => &[],
        };
        let (package, artifact) = if rust {
            (
                self.bin("rust-web"),
                format!("{}.wasm", self.rust_web_lib()),
            )
        } else {
            (self.bin("web"), format!("{}.wasm", self.web_lib()))
        };
        let build = Build {
            workspace: &self.workspace,
            package: &package,
            triple: Some(WASM),
            release: false,
            profile: Some("web"),
            extra: features,
            artifact: &artifact,
            what: &format!("the web runtime of {}", self.packages()),
        };
        let built = self.build_with(cmd, &build)?;
        fs::create_dir_all(&out)?;
        // Both kinds build to the same file: each keeps a copy of its own.
        fs::copy(&built, &runtime.wasm)
            .with_context(|| format!("copy {} to {}", built.display(), runtime.wasm.display()))?;
        Ok(Some(runtime))
    }

    /// The workspace of the app's GPUI development host and release
    /// runner (`<workspace>/gpui`), written when one of them is built: GPUI
    /// pulls in crates (wry, GStreamer, ...) the other targets never need,
    /// and a member of the main workspace would make every build resolve
    /// them. It has its own copy of nsh's `Cargo.lock` and shares the
    /// target directory.
    fn gpui_workspace(&self) -> anyhow::Result<PathBuf> {
        let ws = self.workspace.join("gpui");
        let rust = if self.root.is_some() {
            ", \"rust-gpui\""
        } else {
            ""
        };
        self.write_workspace_manifest(
            &ws,
            &format!(
                "# Generated by nsh for the app's Rust crate (nsh.json \"rust\"): do not edit.\n\
                 [workspace]\nresolver = \"3\"\nmembers = [\"gpui-host\", \"gpui-app\"{rust}]\n"
            ),
            true,
        )?;
        let lock = ws.join("Cargo.lock");
        if !lock.exists() {
            // The GPUI workspace's own lock (`crates/gpui`).
            fs::copy(self.source.join("crates/gpui/Cargo.lock"), &lock)
                .context("copy the GPUI workspace's Cargo.lock")?;
        }
        self.write_host(&ws, "gpui-host", "nsh-gpui-host", "development host")?;
        self.write_host(&ws, "gpui-app", "nsh-gpui-app", "release runner")?;
        if let Some(root) = &self.root {
            self.write_rust_runner(&ws, "rust-gpui", root)?;
        }
        Ok(ws)
    }

    /// Registry dependencies in an independent app must resolve to the same
    /// core as the generated hosts, or their component and registry types differ.
    fn write_workspace_manifest(&self, ws: &Path, base: &str, gpui: bool) -> anyhow::Result<()> {
        let path = ws.join("Cargo.toml");
        let mut manifest = base.to_owned();
        manifest.push_str("\n[patch.crates-io]\n");
        for name in [
            "nsh-animation",
            "nsh-macros",
            "nsh-core",
            "nsh-ui-mobile",
            "nsh-package",
            "nsh-js",
            "nsh-engine",
            "nsh-bundler",
            "nsh-dev",
            "nsh-cli",
            "nsh-platform-ios",
            "nsh-platform-android",
            "nsh-platform-linux",
            "nsh-platform-macos",
            "nsh-platform-windows",
            "nsh-platform-web",
        ]
        .into_iter()
        .chain(gpui.then_some("nsh-platform-gpui"))
        {
            manifest.push_str(&format!(
                "{name} = {{ path = {:?} }}\n",
                nsh_crate(&self.source, name).to_string_lossy()
            ));
        }
        write(&path, &manifest)
    }

    /// Writes the crate `<ws>/<dir>`: the development host or release runner
    /// `lib` (`nsh-linux-host`, ...) with the mobile UI and the app's set.
    fn write_host(&self, ws: &Path, dir: &str, lib: &str, what: &str) -> anyhow::Result<()> {
        let src = |c: &str| {
            let path = nsh_crate(&self.source, c);
            format!("{{ path = {:?} }}", path.to_string_lossy())
        };
        let app = self.dependencies();
        let sets = self.sets();
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
                self.app_sets(&format!("#[cfg({os})]\n"))
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
            profile: None,
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
            profile,
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
        if let Some(profile) = profile {
            cmd.args(["--profile", profile]);
        } else if release {
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
        path.push(match profile {
            Some(profile) => profile,
            None if release => "release",
            None => "debug",
        });
        path.push(artifact);
        Ok(path)
    }

    /// The development host (`runner` false) or release runner of the app,
    /// built for `platform`.
    pub(crate) fn runtime(&self, platform: Platform, runner: bool) -> anyhow::Result<PathBuf> {
        if self.root.is_some() {
            return self.rust_runner(platform, runner);
        }
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

    /// A Rust app's runner for `platform` (`rust-<platform>`): a release
    /// build for packages (`release`), a debug one for `nsh run`. Both run
    /// the app's root with no JS runtime.
    fn rust_runner(&self, platform: Platform, release: bool) -> anyhow::Result<PathBuf> {
        let (dir, workspace) = match platform {
            Platform::Linux => ("rust-linux", self.workspace.clone()),
            Platform::MacOs => ("rust-macos", self.workspace.clone()),
            Platform::Windows => ("rust-windows", self.workspace.clone()),
            Platform::Gpui => ("rust-gpui", self.gpui_workspace()?),
            Platform::IphoneOs | Platform::IphoneSimulator | Platform::MacCatalyst => {
                ("rust-ios", self.workspace.clone())
            }
        };
        let what = format!(
            "the {} runner of {}{}",
            match platform {
                Platform::Linux => "Linux",
                Platform::MacOs => "macOS",
                Platform::Windows => "Windows",
                Platform::Gpui => "GPUI",
                Platform::IphoneOs => "iOS",
                Platform::IphoneSimulator => "iOS simulator",
                Platform::MacCatalyst => "Mac Catalyst",
            },
            self.packages(),
            if release { "" } else { " (debug)" }
        );
        self.build(
            &workspace,
            &self.bin(dir),
            platform.triple(),
            release,
            &[],
            &what,
        )
    }

    /// Waits until `stop` returns true (or the user stops the command);
    /// a change to the app's Rust code ends it with [`Restart`] (a Rust
    /// app's `nsh run`: `nsh` builds the app again and runs it anew).
    pub(crate) fn watch(
        &self,
        mut stop: impl FnMut() -> anyhow::Result<bool>,
    ) -> anyhow::Result<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event {
                    let _ = tx.send(event);
                }
            })?;
        self.watch_with(&mut watcher)?;
        loop {
            if ui::stop_requested() || stop()? {
                return Ok(());
            }
            let Ok(event) = rx.recv_timeout(std::time::Duration::from_millis(200)) else {
                continue;
            };
            // Reads (inotify reports them) change nothing.
            if !matches!(event.kind, notify::EventKind::Access(_))
                && event.paths.iter().any(|p| self.is_rust(p))
            {
                return Err(Restart.into());
            }
        }
    }

    /// The Android runtime of the app: the prebuilt runtime's `classes.dex`
    /// with the app's `libnsh.so` for each of its ABIs.
    pub(crate) fn android(&self, prebuilt: &Path, dev: bool) -> anyhow::Result<PathBuf> {
        let sdk = crate::android::Sdk::find()?;
        let rust = self.root.is_some();
        let out = self.workspace.join(match (rust, dev) {
            (true, true) => "android-rust-dev",
            (true, false) => "android-rust-release",
            (false, true) => "android-dev",
            (false, false) => "android-release",
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
            // A Rust app's library has no development support (it has no
            // JS to reload): `nsh run` builds it in debug.
            let features: &[&str] = if dev && !rust {
                &["--features", "dev"]
            } else {
                &[]
            };
            let (package, artifact) = if rust {
                (self.bin("rust-android"), "libnsh_rust_app.so")
            } else {
                (self.bin("android"), "libnsh_app.so")
            };
            let build = Build {
                workspace: &self.workspace,
                package: &package,
                triple: Some(triple),
                release: !dev,
                profile: None,
                extra: features,
                artifact,
                what: &format!("libnsh.so of {} for {abi}", self.packages()),
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
    /// A profile of its own (`web`), instead of `release`'s.
    profile: Option<&'a str>,
    /// More arguments (features).
    extra: &'a [&'a str],
    /// The file it makes, in the target directory.
    artifact: &'a str,
    /// What the step says it builds.
    what: &'a str,
}

/// The `build.rs` of the web runtimes: the 8 MiB stack nsh's own has.
const WEB_BUILD_RS: &str = "//! The web runtime's stack: 8 MiB, as nsh's own (`.cargo/config.toml`).\n\n\
    fn main() {\n    if std::env::var(\"CARGO_CFG_TARGET_ARCH\").as_deref() == Ok(\"wasm32\") {\n        \
    println!(\"cargo:rustc-link-arg=-zstack-size=8388608\");\n    }\n}\n";

impl RustApp {
    /// The `[dependencies]` lines of its crates, for a generated crate.
    fn dependencies(&self) -> String {
        let lines: Vec<String> = self.crates.iter().map(AppCrate::dependency).collect();
        lines.join("\n")
    }

    /// The UI sets of the generated code: the mobile UI, then the app's,
    /// one per crate ([`Self::app_sets`]).
    fn sets(&self) -> String {
        let mut sets = vec!["nsh_ui_mobile::registry()".to_owned()];
        sets.extend(self.crates.iter().map(|c| format!("{}()", set_fn(c))));
        format!("vec![{}]", sets.join(", "))
    }

    /// A function per crate (`set_app_charts()`), for the generated code
    /// (each item under `cfg`): the crate's UI set, the [`ENTRY`]
    /// `#[native_module]` makes in it. Each glob-imports its crate in a
    /// block, which shadows the module's import of nsh's stand-in, so a
    /// crate without the entry fails to build with a message saying what to
    /// add rather than "cannot find function", on the line of that crate.
    fn app_sets(&self, cfg: &str) -> String {
        let mut code = String::new();
        for krate in &self.crates {
            code.push_str(&format!(
                "\n/// The UI set of the crate {}: the entry `#[native_module(\"…\")]` makes in it.\n\
                 {cfg}fn {}() -> nsh_core::ui::Registry {{\n    \
                 #[allow(unused_imports)]\n    use {}::*;\n    \
                 {ENTRY}() // the crate {} (nsh.json \"rust\")\n}}\n",
                krate.package,
                set_fn(krate),
                krate.name(),
                krate.package,
            ));
        }
        code.push_str(&format!(
            "\n// Without the entry: nsh's stand-in, which fails the build saying what to add.\n\
             {cfg}#[allow(unused_imports)]\nuse nsh_core::ui::__private::no_native_module as {ENTRY};\n"
        ));
        code
    }

    /// `app_root()`, a Rust app's root (`nsh.json` `"root"`, `"theme"`,
    /// `"appearance"`: a component of the first crate `"rust"` lists), for
    /// the generated code, under `cfg`.
    fn root_fn(&self, cfg: &str, root: &RootConfig) -> String {
        let krate = self.crates.first().map(AppCrate::name).unwrap_or_default();
        root_fn(cfg, &krate, root)
    }
}

/// The function of the generated code that makes the UI set of `krate`
/// ([`RustApp::app_sets`]).
fn set_fn(krate: &AppCrate) -> String {
    format!("set_{}", krate.name())
}

/// `app_root()`, a Rust app's root (`nsh.json` `"root"`, `"theme"`,
/// `"appearance"`) in the crate the generated code links as `krate`, under
/// `cfg`.
fn root_fn(cfg: &str, krate: &str, root: &RootConfig) -> String {
    let mut make = format!("nsh_core::ui::Root::of::<{krate}::{}>()", root.component);
    if let Some(theme) = &root.theme {
        make.push_str(&format!(
            "\n        .theme(\n            nsh_core::ui::Theme::from_json(include_str!({:?}))\n                \
             .expect(\"nsh.json: theme\"),\n        )",
            theme.to_string_lossy()
        ));
    }
    if let Some(appearance) = &root.appearance {
        let variant = if appearance == "dark" {
            "Dark"
        } else {
            "Light"
        };
        make.push_str(&format!(
            "\n        .appearance(nsh_core::ui::Appearance::{variant})"
        ));
    }
    format!(
        "\n/// The app's root (`nsh.json` `\"root\"`), with its theme and appearance.\n\
         {cfg}fn app_root() -> nsh_core::ui::Root {{\n    {make}\n}}\n"
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
    use std::path::{Path, PathBuf};

    use super::{AppCrate, RootConfig, RustApp};

    /// An app in `dir` with `crates` (package, directory), built against
    /// this checkout.
    fn app(dir: &Path, crates: &[(&str, PathBuf)], root: Option<RootConfig>) -> RustApp {
        let crates: Vec<AppCrate> = crates
            .iter()
            .map(|(package, dir)| AppCrate {
                package: (*package).into(),
                dir: dir.clone(),
            })
            .collect();
        RustApp {
            workspace: dir.join(".nsh/rust"),
            source: Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
            watched: crates.iter().map(|c| c.dir.clone()).collect(),
            crates,
            root,
        }
    }

    /// Writes the crate `package` in `dir` with `lib` as its `src/lib.rs`
    /// and `deps` in its `[dependencies]`.
    fn write_crate(dir: &Path, package: &str, lib: &str, deps: &str) {
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{package}\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}"
            ),
        )
        .unwrap();
        std::fs::write(dir.join("src/lib.rs"), lib).unwrap();
    }

    #[test]
    fn the_crates_assets_are_its_code() {
        let mut app = app(
            Path::new("/app"),
            &[("app-native", "/app/native".into())],
            None,
        );
        app.watched.push("/shared/ui".into());
        for (path, rust) in [
            ("/app/native/src/lib.rs", true),
            ("/app/native/Cargo.toml", true),
            // `asset!` reads them when the crate builds.
            ("/app/native/assets/logo@2x.png", true),
            ("/app/native/assets/fonts/Script.ttf", true),
            ("/app/native/target/debug/build.rs", false),
            ("/app/native/.nsh/rust/cli/main.rs", false),
            ("/app/native/.git/hooks/build.rs", false),
            ("/app/native/README.md", false),
            ("/app/App.tsx", false),
            // A local crate the app's crates depend on.
            ("/shared/ui/src/card.rs", true),
            ("/shared/other.rs", false),
        ] {
            assert_eq!(app.is_rust(Path::new(path)), rust, "{path}");
        }
    }

    #[test]
    fn rust_is_a_crate_or_a_list_of_them() {
        let dir = std::env::temp_dir().join(format!("nsh-rust-crates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_crate(
            &dir.join("native/charts"),
            "charts",
            "#[native_module(\"app/charts\")]\nfn native(r: &mut Registry) {}\n",
            "",
        );
        write_crate(
            &dir.join("native/storage"),
            "storage",
            "use nsh_core::ui::prelude::*;\n\n#[nsh_core::ui::native_module( \"app/storage\" )]\nfn native(r: &mut Registry) {}\n",
            "",
        );
        let json = |text: &str| serde_json::from_str::<serde_json::Value>(text).unwrap();
        let packages = |rust: &str| -> Vec<String> {
            super::crates(&dir, &json(rust))
                .unwrap()
                .into_iter()
                .map(|c| c.package)
                .collect()
        };
        assert_eq!(packages(r#""native/charts""#), ["charts"]);
        assert_eq!(
            packages(r#"["native/charts", "native/storage"]"#),
            ["charts", "storage"]
        );
        let crates = super::crates(&dir, &json(r#"["native/storage"]"#)).unwrap();
        assert_eq!(
            crates[0].dir,
            dunce::canonicalize(dir.join("native/storage")).unwrap()
        );
        assert_eq!(crates[0].name(), "app_storage");
        let error = |rust: &str| super::crates(&dir, &json(rust)).unwrap_err().to_string();
        for bad in ["[]", "1", "[\"native/charts\", 2]", "{}"] {
            assert!(error(bad).contains("a list of them"), "{bad}");
        }
        assert!(error(r#"["native/nothing"]"#).contains("no crate at native/nothing"));
        // The same crate twice, by two paths.
        assert!(
            error(r#"["native/charts", "native/../native/charts"]"#)
                .contains("lists the crate native/charts twice")
        );
        // Two packages of one name (Cargo takes `-` as `_`).
        write_crate(&dir.join("native/charts-2"), "charts", "", "");
        write_crate(&dir.join("native/my-ui"), "my-ui", "", "");
        write_crate(&dir.join("native/my_ui"), "my_ui", "", "");
        let same = error(r#"["native/charts", "native/charts-2"]"#);
        assert!(
            same.contains("native/charts and native/charts-2 are both the package `charts`"),
            "{same}"
        );
        assert!(
            error(r#"["native/my-ui", "native/my_ui"]"#)
                .contains("are the packages `my-ui` and `my_ui`, one name to Cargo")
        );
        // Two crates of one specifier: one import could not tell them apart.
        write_crate(
            &dir.join("native/charts-copy"),
            "charts-copy",
            "#[native_module(\"app/charts\")]\nfn native(r: &mut Registry) {}\n",
            "",
        );
        let same = error(r#"["native/charts", "native/charts-copy"]"#);
        assert!(
            same.contains(
                "native/charts and native/charts-copy both have `#[native_module(\"app/charts\")]`"
            ),
            "{same}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn specifiers_and_path_dependencies_come_from_the_crates() {
        let dir = std::env::temp_dir().join(format!("nsh-rust-deps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let nsh = dir.join("nsh");
        write_crate(&nsh.join("crates/nsh-core"), "nsh-core", "", "");
        write_crate(&dir.join("shared/base"), "base", "", "");
        write_crate(
            &dir.join("shared/ui"),
            "ui",
            "",
            "base = { version = \"1\", path = '../base' }\n",
        );
        write_crate(
            &dir.join("app/charts"),
            "charts",
            "// #[native_module(\"commented/out\")]\n#[native_module(\"app/charts\")]\nfn native(r: &mut Registry) {}\n",
            &format!(
                "nsh-core = {{ path = {:?} }}\nui = {{ path = \"../../shared/ui\" }}\nlog = \"0.4\"\n\n\
                 [dev-dependencies]\ntest-only = {{ path = \"../test-only\" }}\n",
                nsh.join("crates/nsh-core").to_string_lossy()
            ),
        );
        write_crate(
            &dir.join("app/storage"),
            "storage",
            "",
            "\n[target.'cfg(unix)'.dependencies.ui]\npath = \"../../shared/ui\"\n",
        );
        std::fs::write(
            dir.join("app/storage/Cargo.toml"),
            std::fs::read_to_string(dir.join("app/storage/Cargo.toml")).unwrap()
                + "\n[lib]\npath = \"lib/root.rs\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("app/storage/lib")).unwrap();
        std::fs::write(
            dir.join("app/storage/lib/root.rs"),
            "#[native_module(\"app/storage\")]\nfn native(r: &mut Registry) {}\n",
        )
        .unwrap();
        assert_eq!(
            super::specifier_of(&dir.join("app/charts")).as_deref(),
            Some("app/charts")
        );
        assert_eq!(
            super::specifier_of(&dir.join("app/storage")).as_deref(),
            Some("app/storage")
        );
        assert_eq!(super::specifier_of(&dir.join("shared/ui")), None);
        let crates =
            super::crates(&dir, &serde_json::json!(["app/charts", "app/storage"])).unwrap();
        // The crates, then the local crates they build with (not nsh's, not
        // their dev-dependencies), each once.
        let canonical = |p: &str| dunce::canonicalize(dir.join(p)).unwrap();
        assert_eq!(
            super::watched(&crates, &nsh),
            [
                canonical("app/charts"),
                canonical("app/storage"),
                canonical("shared/ui"),
                canonical("shared/base"),
            ]
        );
        for (line, value) in [
            ("path = \"../a\"", Some("../a")),
            ("x = { version = \"1\", path = \"../a\" }", Some("../a")),
            ("x = {path='../a'}", Some("../a")),
            ("xpath = \"../a\"", None),
            ("path-x = \"../a\"", None),
            ("x = \"1\"", None),
        ] {
            assert_eq!(super::toml_string(line, "path").as_deref(), value, "{line}");
        }
        let _ = std::fs::remove_dir_all(&dir);
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
    fn a_rust_app_declares_its_root_theme_and_appearance() {
        let dir = std::env::temp_dir().join(format!("nsh-rust-root-{}", std::process::id()));
        let ws = dir.join(".nsh/rust");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(dir.join("theme.json"), r#"{ "name": "t", "colors": {} }"#).ok();
        let config = |json: &str| serde_json::from_str::<serde_json::Value>(json).unwrap();
        let root = super::root_config(
            &dir,
            &config(r#"{ "root": "shell::Gallery", "appearance": "dark" }"#),
            &ws,
        )
        .unwrap();
        assert_eq!(root.component, "shell::Gallery");
        assert_eq!(root.appearance.as_deref(), Some("dark"));
        assert_eq!(root.theme, None);
        // `"system"` is the system's appearance: nothing to set.
        let root = super::root_config(
            &dir,
            &config(r#"{ "root": "App", "appearance": "system" }"#),
            &ws,
        )
        .unwrap();
        assert_eq!(root.appearance, None);
        for bad in [
            r#"{ "root": "my app" }"#,
            r#"{ "root": "App<T>" }"#,
            r#"{ "root": 1 }"#,
        ] {
            assert!(
                super::root_config(&dir, &config(bad), &ws).is_err(),
                "{bad}"
            );
        }
        assert!(
            super::root_config(
                &dir,
                &config(r#"{ "root": "App", "appearance": "dim" }"#),
                &ws
            )
            .is_err()
        );
        let code = super::root_fn(
            "#[cfg(x)]\n",
            "app_native",
            &super::RootConfig {
                component: "App".into(),
                theme: Some("/app/theme.json".into()),
                appearance: Some("dark".into()),
            },
        );
        for line in [
            "#[cfg(x)]\nfn app_root() -> nsh_core::ui::Root {",
            "nsh_core::ui::Root::of::<app_native::App>()",
            "include_str!(\"/app/theme.json\")",
            ".appearance(nsh_core::ui::Appearance::Dark)",
        ] {
            assert!(code.contains(line), "no {line:?} in\n{code}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rust_app_gets_runners_without_js() {
        let dir = std::env::temp_dir().join(format!("nsh-rust-runners-{}", std::process::id()));
        let app = app(
            &dir,
            &[("app-native", dir.join("native"))],
            Some(super::RootConfig {
                component: "App".into(),
                theme: None,
                appearance: None,
            }),
        );
        app.write_workspace().unwrap();
        let read = |file: &str| std::fs::read_to_string(app.workspace.join(file)).unwrap();
        let manifest = read("Cargo.toml");
        assert!(manifest.contains("\"rust-linux\", \"rust-macos\", \"rust-windows\", \"rust-ios\", \"rust-android\", \"rust-web\"]"), "{manifest}");
        for (dir, lib, platform) in [
            ("rust-linux", "nsh-linux-app", "nsh-platform-linux"),
            ("rust-macos", "nsh-macos-app", "nsh-platform-macos"),
            ("rust-windows", "nsh-windows-app", "nsh-platform-windows"),
            ("rust-ios", "nsh-ios-app", "nsh-platform-ios"),
        ] {
            let cargo = read(&format!("{dir}/Cargo.toml"));
            // No `js`: the runner links no QuickJS.
            assert!(cargo.contains(&format!("{lib} = {{ path =")), "{cargo}");
            assert!(
                cargo.contains(&format!("{platform} = {{ path =")),
                "{cargo}"
            );
            assert_eq!(
                cargo.matches("default-features = false").count(),
                2,
                "{cargo}"
            );
            let main = read(&format!("{dir}/main.rs"));
            assert!(
                main.contains(&format!("{}::run_rust(app_root())", lib.replace('-', "_"))),
                "{main}"
            );
            assert!(
                main.contains("nsh_core::ui::Root::of::<app_app_native::App>()"),
                "{main}"
            );
        }
        assert!(read("rust-linux/Cargo.toml").contains("features = [\"webview\"]"));
        let android = read("rust-android/lib.rs");
        assert!(android.contains("nsh::main!(app_root());"), "{android}");
        assert!(read("rust-android/Cargo.toml").contains("default-features = false"));
        let web = read("rust-web/lib.rs");
        assert!(web.contains("nsh_platform_web::rust_app!(vec![nsh_ui_mobile::registry(), set_app_app_native()], app_root());"), "{web}");
        assert!(read("rust-web/Cargo.toml").contains("default-features = false"));
        assert!(read("rust-web/build.rs").contains("-zstack-size=8388608"));
        // The root reaches the app's command line (`current`).
        assert!(read("app.json").contains("\"component\":\"App\""));
        app.gpui_workspace().unwrap();
        assert!(read("gpui/Cargo.toml").contains("\"rust-gpui\""));
        assert!(read("gpui/rust-gpui/main.rs").contains("nsh_gpui_app::run_rust(app_root())"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tsx_app_has_no_rust_runners() {
        let dir = std::env::temp_dir().join(format!("nsh-rust-tsx-{}", std::process::id()));
        let app = app(&dir, &[("app-native", dir.join("native"))], None);
        app.write_workspace().unwrap();
        let manifest = std::fs::read_to_string(app.workspace.join("Cargo.toml")).unwrap();
        assert!(!manifest.contains("rust-"), "{manifest}");
        assert!(!app.workspace.join("rust-web").exists());
        assert!(super::is_root(Path::new("/app/nsh.json")));
        assert!(!super::is_root(Path::new("/app/App.tsx")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_generated_code_calls_the_entry_of_the_crate() {
        let dir = std::env::temp_dir().join(format!("nsh-rust-app-ws-{}", std::process::id()));
        let app = app(&dir, &[("app-native", dir.join("native"))], None);
        app.write_workspace().unwrap();
        let read = |file: &str| std::fs::read_to_string(app.workspace.join(file)).unwrap();
        // GPUI's hosts are written only for a GPUI build, in a workspace of
        // their own, so other builds never resolve GPUI's crates.
        let manifest = read("Cargo.toml");
        assert!(!manifest.contains("gpui-"), "{manifest}");
        assert!(manifest.contains("[patch.crates-io]"), "{manifest}");
        assert!(manifest.contains("nsh-core = { path ="), "{manifest}");
        assert!(manifest.contains("exclude = [\"gpui\"]"), "{manifest}");
        // The web runtime is built as nsh builds its own.
        assert!(manifest.contains("\"web\"]"), "{manifest}");
        assert!(manifest.contains("[profile.web]"), "{manifest}");
        let web = read("web/Cargo.toml");
        assert!(web.contains("crate-type = [\"cdylib\"]"), "{web}");
        assert!(web.contains("name = \"nsh_app_app_native_web\""), "{web}");
        assert!(
            web.contains("bytecode-only = [\"nsh-platform-web/bytecode-only\"]"),
            "{web}"
        );
        assert!(read("web/build.rs").contains("-zstack-size=8388608"));
        assert!(read("web/lib.rs").contains("nsh_platform_web::ui_sets!("));
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
            ("web/lib.rs", "#[cfg(target_arch = \"wasm32\")]\n"),
        ] {
            let code = read(file);
            for line in [
                "vec![nsh_ui_mobile::registry(), set_app_app_native()]".to_owned(),
                format!(
                    "{cfg}fn set_app_app_native() -> nsh_core::ui::Registry {{\n    #[allow(unused_imports)]\n    use app_app_native::*;\n    __nsh_native_module() // the crate app-native (nsh.json \"rust\")\n}}"
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
            "web",
        ] {
            assert!(
                read(&format!("{dir}/Cargo.toml")).contains("\nnsh-core = "),
                "{dir}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_generated_code_calls_the_entry_of_each_crate() {
        let dir = std::env::temp_dir().join(format!("nsh-rust-two-{}", std::process::id()));
        let app = app(
            &dir,
            &[
                ("charts", dir.join("native/charts")),
                ("app-storage", dir.join("native/storage")),
            ],
            Some(super::RootConfig {
                component: "shell::Home".into(),
                theme: None,
                appearance: None,
            }),
        );
        app.write_workspace().unwrap();
        app.gpui_workspace().unwrap();
        let read = |file: &str| std::fs::read_to_string(app.workspace.join(file)).unwrap();
        let sets = "vec![nsh_ui_mobile::registry(), set_app_charts(), set_app_app_storage()]";
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
            "web",
            "rust-linux",
            "rust-macos",
            "rust-windows",
            "rust-ios",
            "rust-android",
            "rust-web",
            "gpui/rust-gpui",
        ] {
            let cargo = read(&format!("{dir}/Cargo.toml"));
            for (name, package, path) in [
                ("app_charts", "charts", "native/charts"),
                ("app_app_storage", "app-storage", "native/storage"),
            ] {
                let line = format!(
                    "\n{name} = {{ package = \"{package}\", path = {:?} }}\n",
                    dir_of(&app, path)
                );
                assert!(cargo.contains(&line), "{dir}: no {line:?} in\n{cargo}");
            }
            let file = if dir == "android" || dir.ends_with("web") || dir == "rust-android" {
                "lib.rs"
            } else {
                "main.rs"
            };
            let code = read(&format!("{dir}/{file}"));
            assert!(code.contains(sets), "{dir}: no {sets:?} in\n{code}");
            // Each crate's entry in its own function; one stand-in.
            for (name, package) in [("app_charts", "charts"), ("app_app_storage", "app-storage")] {
                let set = format!(
                    "fn set_{name}() -> nsh_core::ui::Registry {{\n    #[allow(unused_imports)]\n    use {name}::*;\n    __nsh_native_module() // the crate {package} (nsh.json \"rust\")\n}}"
                );
                assert!(code.contains(&set), "{dir}: no {set:?} in\n{code}");
            }
            assert_eq!(
                code.matches("no_native_module as __nsh_native_module;")
                    .count(),
                1,
                "{dir}"
            );
            // The root is a component of the first crate listed.
            if dir.contains("rust-") {
                assert!(
                    code.contains("nsh_core::ui::Root::of::<app_charts::shell::Home>()"),
                    "{dir}: {code}"
                );
            }
        }
        // Named after all its crates: another app's crates build apart.
        assert!(read("cli/Cargo.toml").contains("name = \"nsh-app-charts-app-storage-cli\""));
        // The app's command line finds them again (`current`).
        let loaded = super::load(app.workspace.clone()).unwrap();
        assert_eq!(loaded.crates, app.crates);
        assert_eq!(loaded.watched, app.watched);
        assert_eq!(loaded.root, app.root);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The directory `path` of the app's crates, as the generated code
    /// names it.
    fn dir_of(app: &RustApp, path: &str) -> String {
        app.workspace
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .join(path)
            .to_string_lossy()
            .into_owned()
    }
}
