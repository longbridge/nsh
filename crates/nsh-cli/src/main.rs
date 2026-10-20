//! `nsh`, the command line tool. It links the mobile UI; an app with Rust
//! components of its own (`nsh.json` `"rust"`) is known to it by the
//! description its describe program prints (`rust_app.rs`), and runs in the
//! hosts and runners built for it.

mod android;
mod build;
mod check;
mod dev;
mod entry;
mod init;
mod logger;
mod package;
mod run;
mod runtime;
mod rust_app;
mod sign;
mod test;
mod trace;
mod ui;
mod web;
mod windows;

use std::{
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, OnceLock, RwLock},
};

use clap::{Args, Parser, Subcommand};
use nsh_core::ui::{Registry, description::SetDescription};

/// The UI sets this command line links: the mobile UI.
pub(crate) fn own_sets() -> &'static [Registry] {
    static OWN: OnceLock<Vec<Registry>> = OnceLock::new();
    OWN.get_or_init(|| vec![nsh_ui_mobile::registry()])
}

/// The UI sets of the command's app, as data ([`set_app_sets`]).
static UI_SETS: RwLock<Option<Arc<[SetDescription]>>> = RwLock::new(None);

/// The UI sets the hosts serve, as data: what bundles import natively,
/// what bytecode links against, what `.nsh/types` declares. The mobile UI,
/// then an app's own sets ([`set_app_sets`]).
pub(crate) fn ui_sets() -> Arc<[SetDescription]> {
    if let Some(sets) = &*UI_SETS.read().unwrap_or_else(|e| e.into_inner()) {
        return sets.clone();
    }
    let sets: Arc<[SetDescription]> = own_sets().iter().map(SetDescription::from).collect();
    *UI_SETS.write().unwrap_or_else(|e| e.into_inner()) = Some(sets.clone());
    sets
}

/// Makes [`ui_sets`] the mobile UI and `app`, the sets of the app's crates
/// (described again after a change to its Rust code).
pub(crate) fn set_app_sets(app: Vec<SetDescription>) {
    let sets: Arc<[SetDescription]> = own_sets()
        .iter()
        .map(SetDescription::from)
        .chain(app)
        .collect();
    *UI_SETS.write().unwrap_or_else(|e| e.into_inner()) = Some(sets);
}

#[derive(Parser)]
#[command(name = "nsh", version, about = "Build and run nsh apps")]
struct Cli {
    /// Show everything: log lines with their targets, cargo's output and
    /// the hosts' own.
    #[arg(
        short,
        long,
        global = true,
        env = "NSH_VERBOSE",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
enum Command {
    /// Create an app: in TSX, in Rust, or in TSX with a Rust crate of its
    /// own (`--template`). Generates the app's entry points, configuration
    /// and editor declarations.
    Init {
        /// Directory of the app (created if missing).
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Display name (default: the directory's name).
        #[arg(long)]
        name: Option<String>,
        /// What the app is written in (asked on a terminal; else `ts`).
        #[arg(long, value_enum)]
        template: Option<init::Template>,
        /// With `--template rust` or `mixed`: depend on the nsh crates of
        /// this checkout by path, instead of crates.io's at this CLI's
        /// version (the app's hosts build from the same place).
        #[arg(long, value_name = "DIR")]
        nsh_source: Option<PathBuf>,
    },
    /// Bundle an app and compile it to QuickJS bytecode; a Rust app
    /// (`nsh.json` `"root"`): build its crates and gather the files they ship.
    Build {
        /// The entry module, or the app's directory (default: the current one),
        /// whose `App.tsx` is the entry.
        #[arg(default_value = ".")]
        entry: PathBuf,
        /// Output directory.
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
        /// Minify and strip debug info.
        #[arg(long)]
        release: bool,
        /// Split what `import()` loads into chunk files next to the bundle
        /// (`Screen.js`, ...), loaded on demand: how web apps are published.
        #[arg(long)]
        split: bool,
    },
    /// Run an app headless (a Rust app: its root) and print the mount
    /// instructions and timings.
    Trace {
        /// The entry module, or the app's directory (default: the current one),
        /// whose `App.tsx` is the entry.
        #[arg(default_value = ".")]
        entry: PathBuf,
        /// Number of taps to send to the first pressable view (without
        /// `--tap`).
        #[arg(long, default_value_t = 2)]
        taps: u32,
        /// Tap a view (repeatable, in order): `id:<testID>`,
        /// `label:<accessibilityLabel>`, `text:<text>`, `view:<id>`, or a
        /// bare string matched as a testID, a label, then a text. A match
        /// that is not pressable taps its nearest pressable ancestor.
        #[arg(long, value_name = "SELECTOR")]
        tap: Vec<String>,
        /// Idle time before tapping, in milliseconds: the app must render
        /// nothing for this long (at most 10 s). The default lets the
        /// engine's idle GC run first, so retention bugs show up.
        #[arg(long, default_value_t = 1200)]
        idle_ms: u64,
        /// Run timers and animations this long before the first tap and
        /// after each one, in milliseconds, instead of waiting until the
        /// app is idle.
        #[arg(long, value_name = "MS")]
        wait: Option<u64>,
        /// Keep running this long after the last tap, in milliseconds.
        #[arg(long, value_name = "MS", default_value_t = 0)]
        duration: u64,
        /// Simulated safe area insets in points: `top,right,bottom,left`
        /// (or one value for all four), e.g. `47,0,34,0`.
        #[arg(long, value_name = "INSETS", value_parser = nsh_engine::headless::parse_insets)]
        safe_area: Option<nsh_engine::Insets>,
        /// Scroll the first scroll view / list by this many points, in
        /// 10 pt frames, and print each frame's batch.
        #[arg(long)]
        scroll: Option<f32>,
        /// Viewport width in points.
        #[arg(long, default_value_t = 390.0)]
        width: f32,
        /// Viewport height in points.
        #[arg(long, default_value_t = 844.0)]
        height: f32,
        /// Isolated root for this trace's app data, storage and native file APIs.
        /// Without this flag, trace uses memory-only storage and no filesystem data root.
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
        /// Simulated operating system; desktop system APIs also require a compatible host.
        #[arg(long, value_enum)]
        os: Option<trace::TraceOs>,
    },
    /// Run an app's tests headless: its test files (`*.test.tsx`,
    /// `*.test.ts`, importing `nsh/test`), each test opening the app on an
    /// engine of its own; or, given the app's entry, open it and take the
    /// steps standard input gives (`await app.press("Add")`).
    Test {
        /// A test file, a directory whose test files to run (default: the
        /// current one), or an app's entry.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Run only the tests whose name contains this.
        #[arg(long)]
        filter: Option<String>,
        /// Print every mount batch of the apps the tests open, under the
        /// step it came in, and the time of every commit.
        #[arg(long)]
        trace: bool,
    },
    /// Type check an app with TypeScript's native compiler (TypeScript 7,
    /// downloaded once from the npm registry; no Node needed).
    Check {
        /// The entry module, or the app's directory (default: the current one),
        /// whose `App.tsx` is the entry.
        #[arg(default_value = ".")]
        entry: PathBuf,
        /// The `tsc` to run instead of the pinned TypeScript 7.
        #[arg(long, env = "NSH_TSC")]
        tsc: Option<PathBuf>,
        /// Further arguments for `tsc` (after `--`).
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Serve an app to dev hosts (simulators, devices) and push rebuilds.
    Dev {
        /// The entry module, or the app's directory (default: the current one),
        /// whose `App.tsx` is the entry.
        #[arg(default_value = ".")]
        entry: PathBuf,
        /// Dev server port (the next free one is used if it is taken).
        #[arg(long, default_value_t = nsh_dev::protocol::DEFAULT_PORT)]
        port: u16,
        /// Minify and strip debug info.
        #[arg(long)]
        release: bool,
        /// Run the app as an app embedded in another one would, with the
        /// VM limits such an app sets: `memory` and `stack` (`KB`, `MB`,
        /// `GB`), `load` and `call` (`ms`, `s`) and `nodes`, as in
        /// `--limits memory=128MB,call=500ms`; the rest keep the embedding
        /// defaults (256 MB, 4 MB, 10 s, 2 s, 200000), which `--limits`
        /// alone runs with. Without it the app runs on its own, with none.
        #[arg(long, value_name = "LIMITS", num_args = 0..=1, default_missing_value = "", value_parser = parse_limits)]
        limits: Option<nsh_core::js::HostLimits>,
    },
    /// Package an installable app from the prebuilt runtime (no Rust
    /// needed). Metadata comes from `nsh.json` next to the entry, overridden
    /// by the flags below.
    Package(Box<PackageArgs>),
    /// Like `dev`, and also run the app in a development host: the Mac
    /// Catalyst app (default on macOS), the macOS (AppKit) app, the Linux
    /// app (default on Linux), the Windows app (default on Windows), the
    /// GPUI app (on any of the three), an iOS simulator, an Android device
    /// or emulator, or a browser.
    Run {
        /// The entry module, or the app's directory (default: the current one),
        /// whose `App.tsx` is the entry.
        #[arg(default_value = ".")]
        entry: PathBuf,
        /// Dev server port (the next free one is used if it is taken).
        #[arg(long, default_value_t = nsh_dev::protocol::DEFAULT_PORT)]
        port: u16,
        /// Minify and strip debug info.
        #[arg(long)]
        release: bool,
        /// Run the app as an app embedded in another one would, with the
        /// VM limits such an app sets: `memory` and `stack` (`KB`, `MB`,
        /// `GB`), `load` and `call` (`ms`, `s`) and `nodes`, as in
        /// `--limits memory=128MB,call=500ms`; the rest keep the embedding
        /// defaults (256 MB, 4 MB, 10 s, 2 s, 200000), which `--limits`
        /// alone runs with. Without it the app runs on its own, with none.
        #[arg(long, value_name = "LIMITS", num_args = 0..=1, default_missing_value = "", value_parser = parse_limits)]
        limits: Option<nsh_core::js::HostLimits>,
        /// Log per-commit timings.
        #[arg(long)]
        trace: bool,
        /// Show the performance monitor over the app: frame rate, frame and
        /// commit costs, CPU, GPU, memory (`--target macos`).
        #[arg(long)]
        fps: bool,
        /// Where to run.
        #[arg(long, value_enum, default_value_t = run::Target::native())]
        target: run::Target,
        /// iOS simulator to use (name or UDID); default: a booted one, else
        /// the newest iPhone.
        #[arg(long)]
        device: Option<String>,
        /// The app's name in the host (the menu bar, the window title;
        /// `nsh.json`'s name by default; `--target macos` only).
        #[arg(long)]
        name: Option<String>,
        /// The window's initial size, `WxH` in points (the host's default
        /// by default; the desktop hosts: `--target macos`, `linux`,
        /// `windows`, `gpui`).
        #[arg(long = "window-size", value_name = "WIDTHxHEIGHT")]
        size: Option<String>,
        /// Script steps to run once the window shows (`wait`, `shot`,
        /// `tap`, `quit`, ...; the desktop hosts: `--target macos`,
        /// `linux`, `windows`, `gpui`; an app of Rust only: `macos`,
        /// `linux`, `windows`). With `quit` the command exits when the
        /// script does — snapshots without a window staying open.
        #[arg(long)]
        script: Option<String>,
    },
    /// Create a signing key: `<name>.secret` (keep it offline for a root)
    /// and `<name>.pub` (what apps trust, for a root).
    Keygen {
        /// Path and name of the key files, e.g. `keys/root`.
        name: PathBuf,
    },
    /// Certify a release key with a root key, for some app ids and a time
    /// window; packages carry the certificate.
    Certify {
        /// The root's secret key (`.secret`).
        #[arg(long)]
        root: PathBuf,
        /// The release key's public key (`.pub`).
        #[arg(long)]
        key: PathBuf,
        /// App ids it may sign for: `*`, `com.example.*` or an id (repeatable).
        #[arg(long = "apps", required = true)]
        apps: Vec<String>,
        /// How long it may sign, from now.
        #[arg(long, default_value_t = 365)]
        days: u64,
        /// The certificate to write.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Build an app in release mode and sign it into a package (`.nshpkg`)
    /// for apps that embed nsh to verify and run. Metadata comes from
    /// `nsh.json` next to the entry, overridden by the flags below.
    Publish {
        /// The entry module, or the app's directory (default: the current one),
        /// whose `App.tsx` is the entry.
        #[arg(default_value = ".")]
        entry: PathBuf,
        /// The release key's secret key (`.secret`).
        #[arg(long)]
        key: PathBuf,
        /// Its certificate (`nsh certify`).
        #[arg(long)]
        cert: PathBuf,
        /// The package to write (default: `out/<id>-<build>.nshpkg`).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// App id, e.g. `com.example.app`.
        #[arg(long)]
        id: Option<String>,
        /// Version, e.g. `1.2.0`.
        #[arg(long = "app-version")]
        version: Option<String>,
        /// Build number: must grow with every release.
        #[arg(long)]
        build: Option<u32>,
        /// Where the source map goes: into the package (devices report
        /// errors at app source positions), into a file next to it
        /// (`<package>.map`, for `nsh symbolicate`), or nowhere.
        #[arg(long, value_enum, default_value_t = sign::SourceMapOut::Package)]
        source_map: sign::SourceMapOut,
        /// Leave the line and column tables out of the bytecode: a smaller
        /// package whose error stacks have no positions.
        #[arg(long)]
        strip_debug: bool,
        /// The lowest runtime that runs the package, e.g. `0.2.0` (default:
        /// this CLI's version); older runtimes refuse it.
        #[arg(long, value_name = "VERSION")]
        min_runtime: Option<String>,
    },
    /// Map the positions in a reported error stack (`bundle.js:LINE:COL`)
    /// to the app's sources, with a package's source map.
    Symbolicate {
        /// The source map (`<package>.map`, `bundle.js.map`) or a package
        /// (`.nshpkg`) that holds one.
        map: PathBuf,
        /// The error text (default: standard input).
        input: Option<PathBuf>,
    },
    /// Verify a package as an app trusting the given roots would (without
    /// roots: only its signature).
    Verify {
        /// The package (`.nshpkg`).
        package: PathBuf,
        /// A trusted root's public key (`.pub`; repeatable).
        #[arg(long)]
        root: Vec<PathBuf>,
        /// A revocation list to apply.
        #[arg(long)]
        revocations: Option<PathBuf>,
    },
    /// Sign a revocation list: release keys and packages that must not load
    /// any more.
    Revoke {
        /// The root's secret key (`.secret`).
        #[arg(long)]
        root: PathBuf,
        /// Keep the entries of this list (the last one published).
        #[arg(long)]
        extend: Option<PathBuf>,
        /// The list's number: higher than any published before (default
        /// with `--extend`: that list's plus one).
        #[arg(long)]
        sequence: Option<u64>,
        /// A release key id to revoke (repeatable).
        #[arg(long = "key")]
        keys: Vec<String>,
        /// A package SHA-256 to revoke (repeatable; `nsh verify` prints it).
        #[arg(long = "package")]
        packages: Vec<String>,
        /// The list to write.
        #[arg(short, long)]
        out: PathBuf,
    },
}

/// Arguments of `nsh package`.
#[derive(Args, Clone)]
struct PackageArgs {
    /// The entry module, or the app's directory (default: the current one),
    /// whose `App.tsx` is the entry.
    #[arg(default_value = ".")]
    entry: PathBuf,
    /// What to build for.
    #[arg(long, value_enum)]
    target: package::PackageTarget,
    /// Output directory.
    #[arg(short, long, default_value = "out")]
    out: PathBuf,
    /// Display name.
    #[arg(long)]
    name: Option<String>,
    /// Bundle identifier, e.g. `com.example.app`.
    #[arg(long)]
    id: Option<String>,
    /// Version, e.g. `1.2.0`.
    #[arg(long = "app-version")]
    version: Option<String>,
    /// Build number.
    #[arg(long)]
    build: Option<u32>,
    /// App icon: a square PNG, ideally 1024×1024.
    #[arg(long)]
    icon: Option<PathBuf>,
    /// Code signing identity for devices, e.g. "Apple Development: …".
    #[arg(long)]
    sign: Option<String>,
    /// Provisioning profile for devices (`.mobileprovision`).
    #[arg(long)]
    profile: Option<PathBuf>,
    /// Android release keystore (default: the debug key).
    #[arg(long, requires = "key_alias")]
    keystore: Option<PathBuf>,
    /// Key alias in the keystore.
    #[arg(long)]
    key_alias: Option<String>,
    /// Keystore password (also `NSH_KEYSTORE_PASSWORD`).
    #[arg(long, env = "NSH_KEYSTORE_PASSWORD", hide_env_values = true)]
    keystore_password: Option<String>,
    /// Install and launch the app in the simulator or on the Android device
    /// afterwards (Linux, macOS, Windows: launch it; web: serve it on this
    /// machine at the address it prints).
    #[arg(long)]
    run: bool,
}

impl Command {
    /// The entry argument of the commands that build an app.
    fn entry_mut(&mut self) -> Option<&mut PathBuf> {
        match self {
            Command::Build { entry, .. }
            | Command::Trace { entry, .. }
            | Command::Check { entry, .. }
            | Command::Dev { entry, .. }
            | Command::Run { entry, .. }
            | Command::Publish { entry, .. } => Some(entry),
            Command::Package(args) => Some(&mut args.entry),
            Command::Init { .. }
            | Command::Keygen { .. }
            | Command::Certify { .. }
            | Command::Verify { .. }
            | Command::Symbolicate { .. }
            | Command::Test { .. }
            | Command::Revoke { .. } => None,
        }
    }
}

/// Runs the command line; errors are printed.
fn main() -> ExitCode {
    let cli = Cli::parse();
    ui::init(cli.verbose);
    logger::init();
    let result = run(cli.command);
    // A signal stopped the command (`ui::Stoppable`): it ends as the signal
    // would have, and what failed as it stopped (a tool it interrupted) is
    // no news.
    ui::exit_if_stopped();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            ui::report(&e);
            ExitCode::FAILURE
        }
    }
}

/// `--limits`' value ([`nsh_core::js::HostLimits::parse`]).
fn parse_limits(spec: &str) -> Result<nsh_core::js::HostLimits, String> {
    nsh_core::js::HostLimits::parse(spec)
}

/// Refuses `--limits` for an app of Rust only: they are its JS VM's, and it
/// runs no JS.
fn no_limits_for_rust(
    entry: &std::path::Path,
    limits: Option<nsh_core::js::HostLimits>,
) -> anyhow::Result<()> {
    if limits.is_some() && rust_app::is_root(entry) {
        anyhow::bail!(
            "--limits sets the limits of an app's JS VM, and an app of Rust only runs no JS"
        );
    }
    Ok(())
}

fn run(mut command: Command) -> anyhow::Result<()> {
    let mut types = !matches!(command, Command::Trace { .. });
    if let Some(entry) = command.entry_mut() {
        *entry = entry::resolve(entry)?;
        // A Rust app has no TypeScript to declare things for.
        types &= !rust_app::is_root(entry);
    }
    // A Rust app (`nsh.json` "root") has no bundle: it is built, traced,
    // run and packaged.
    let rust_command = matches!(
        command,
        Command::Build { .. } | Command::Trace { .. } | Command::Package(_) | Command::Run { .. }
    );
    if let Some(entry) = command.entry_mut()
        && rust_app::is_root(entry)
        && !rust_command
    {
        anyhow::bail!(
            "{} declares a Rust app (\"root\"): it is built with `nsh build`, traced with `nsh trace`, \
             run with `nsh run` and packaged with `nsh package`; the other commands take a TSX entry",
            ui::path(entry)
        );
    }
    let app = match command.entry_mut() {
        Some(entry) => rust_app::of(entry)?,
        None => None,
    };
    let Some(app) = app else {
        if let Some(entry) = command.entry_mut() {
            if rust_app::is_root(entry) {
                anyhow::bail!("nsh.json: \"root\" needs \"rust\", the path of the app's crate");
            }
            if types {
                init::write_types_for(entry);
            }
        }
        return dispatch(command);
    };
    // An app with Rust crates of its own: described by the program built
    // from them, its hosts built with them. A development session ends
    // when its Rust code changes, and starts again with it built anew.
    rust_app::set_current(app.clone());
    let mut restarted = false;
    loop {
        match app.describe() {
            Ok(sets) => set_app_sets(sets),
            // Its build failed and waited for the next change already.
            Err(e) if e.downcast_ref::<rust_app::Restart>().is_some() => continue,
            // The code just saved does not describe: wait for the fix.
            Err(e) if restarted => {
                app.wait_after(&e)?;
                continue;
            }
            Err(e) => return Err(e),
        }
        if types && let Some(entry) = command.entry_mut() {
            init::write_types_for(entry);
        }
        match dispatch(command.clone()) {
            Err(e) if e.downcast_ref::<rust_app::Restart>().is_some() && !ui::stop_requested() => {
                restarted = true;
                rust_app::set_restarted();
                ui::note("the app's Rust code changed: building it again");
            }
            result => return result,
        }
    }
}

fn dispatch(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Init {
            dir,
            name,
            template,
            nsh_source,
        } => {
            let template = match template {
                Some(template) => template,
                None => init::Template::ask()?,
            };
            init::init(&dir, name, template, nsh_source.as_deref())
        }
        Command::Build {
            entry,
            out,
            release,
            split,
        } => build::build(&entry, &out, release, split),
        Command::Trace {
            entry,
            taps,
            tap,
            idle_ms,
            wait,
            duration,
            safe_area,
            scroll,
            width,
            height,
            data_dir,
            os,
        } => trace::trace(
            &entry,
            nsh_engine::headless::TraceOptions {
                taps,
                tap,
                idle_ms,
                wait,
                duration,
                scroll,
                width,
                height,
                safe_area: safe_area.unwrap_or_default(),
                data_dir: data_dir.map(trace::data_dir).transpose()?,
                os: os.map(trace::TraceOs::platform),
            },
        ),
        Command::Check { entry, tsc, args } => check::check(&entry, tsc.as_deref(), &args),
        Command::Test {
            path,
            filter,
            trace,
        } => test::test(&path, filter, trace),
        Command::Dev {
            entry,
            port,
            release,
            limits,
        } => {
            no_limits_for_rust(&entry, limits)?;
            let session = dev::DevSession::start(&entry, port, release, limits)?;
            session.print_urls();
            session.watch(|| Ok(false))
        }
        Command::Package(args) => {
            let PackageArgs {
                entry,
                target,
                out,
                name,
                id,
                version,
                build,
                icon,
                sign,
                profile,
                keystore,
                key_alias,
                keystore_password,
                run,
            } = *args;
            let mut config = package::read_config(&entry)?;
            config.name = name.or(config.name);
            config.id = id.or(config.id);
            config.version = version.or(config.version);
            config.build = build.or(config.build);
            config.icon = icon.or(config.icon);
            package::package(package::PackageOptions {
                entry: &entry,
                target,
                out: &out,
                config,
                sign: sign.as_deref(),
                profile: profile.as_deref(),
                keystore: keystore.as_deref().map(|k| {
                    (
                        k,
                        key_alias.as_deref().unwrap_or_default(),
                        keystore_password.as_deref().unwrap_or_default(),
                    )
                }),
                run,
            })
        }
        Command::Run {
            entry,
            port,
            release,
            limits,
            trace,
            fps,
            target,
            device,
            name,
            size,
            script,
        } => {
            no_limits_for_rust(&entry, limits)?;
            run::run(
                &entry,
                port,
                release,
                run::HostFlags { limits, trace, fps },
                target,
                device.as_deref(),
                run::HostWindow {
                    name: name.as_deref(),
                    size: size.as_deref(),
                    script: script.as_deref(),
                },
            )
        }
        Command::Keygen { name } => sign::keygen(&name),
        Command::Certify {
            root,
            key,
            apps,
            days,
            out,
        } => sign::certify_key(&root, &key, &apps, days, &out),
        Command::Publish {
            entry,
            key,
            cert,
            out,
            id,
            version,
            build,
            source_map,
            strip_debug,
            min_runtime,
        } => {
            let mut config = package::read_config(&entry)?;
            config.id = id.or(config.id);
            config.version = version.or(config.version);
            config.build = build.or(config.build);
            config.min_runtime = min_runtime.or(config.min_runtime);
            sign::publish(sign::PublishOptions {
                entry: &entry,
                key: &key,
                cert: &cert,
                out,
                config,
                source_map,
                strip_debug,
            })
        }
        Command::Symbolicate { map, input } => sign::symbolicate(&map, input.as_deref()),
        Command::Verify {
            package,
            root,
            revocations,
        } => sign::verify(&package, &root, revocations.as_deref()),
        Command::Revoke {
            root,
            extend,
            sequence,
            keys,
            packages,
            out,
        } => sign::revoke_list(&root, extend.as_deref(), sequence, &keys, &packages, &out),
    }
}
