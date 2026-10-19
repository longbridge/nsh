//! The `nsh` command line tool, as a library: [`main_with`] runs it with
//! the UI sets the hosts serve. `nsh` itself is `main_with` of the mobile
//! UI; an app with Rust components of its own (`nsh.json` `"rust"`) gets a
//! build of it with the app's sets too, one per crate.

mod android;
mod build;
mod check;
mod dev;
mod entry;
mod init;
mod logger;
mod net;
mod package;
mod run;
mod runtime;
mod rust_app;
mod sign;
mod trace;
mod ui;
mod web;
mod windows;

use std::{path::PathBuf, process::ExitCode, sync::OnceLock};

use clap::{Args, Parser, Subcommand};
use nsh_core::ui::{Registry, Root};

/// The UI sets of this build ([`main_with`]).
static UI_SETS: OnceLock<Vec<Registry>> = OnceLock::new();

/// The root of the Rust app this build runs for ([`main_with_root`]).
static ROOT: OnceLock<fn() -> Root> = OnceLock::new();

/// The root of the Rust app this command line was built for (`nsh.json`
/// `"root"`), which `nsh trace` mounts.
pub(crate) fn root() -> Option<Root> {
    ROOT.get().map(|make| make())
}

/// The UI sets the hosts serve: what bundles import natively, what bytecode
/// links against, what `.nsh/types` declares.
pub(crate) fn ui_sets() -> &'static [Registry] {
    UI_SETS.get_or_init(|| vec![nsh_ui_mobile::registry()])
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

#[derive(Subcommand)]
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
        #[arg(long, value_name = "INSETS", value_parser = trace::parse_insets)]
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
        /// by default; `--target macos` only).
        #[arg(long = "window-size", value_name = "WIDTHxHEIGHT")]
        size: Option<String>,
        /// Script steps to run once the window shows (`wait`, `shot`,
        /// `tap`, `quit`, ...; `--target macos` only). With `quit` the
        /// command exits when the script does — snapshots without a
        /// window staying open.
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
#[derive(Args)]
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
            | Command::Revoke { .. } => None,
        }
    }
}

/// [`main_with`] for a Rust app (`nsh.json` `"root"`): `root` makes the
/// app's root (its component, theme, appearance and id), which `nsh trace`
/// mounts headless.
pub fn main_with_root(ui_sets: Vec<Registry>, root: fn() -> Root) -> ExitCode {
    let _ = ROOT.set(root);
    main_with(ui_sets)
}

/// Runs the command line with `ui_sets` (`nsh_ui_mobile::registry()` and
/// the registries of an app's own crates); errors are printed.
pub fn main_with(ui_sets: Vec<Registry>) -> ExitCode {
    let duplicate = duplicate_specifier(&ui_sets);
    let _ = UI_SETS.set(ui_sets);
    let cli = Cli::parse();
    ui::init(cli.verbose);
    logger::init();
    let result = match duplicate {
        // A second set of one specifier would never be imported: refused,
        // whatever the crates' sources looked like when the workspace was
        // written.
        Some(specifier) => Err(anyhow::anyhow!(
            "two UI sets are imported as {specifier:?}: give each crate of the app a `#[native_module(\"…\")]` specifier of its own"
        )),
        None => run(cli.command),
    };
    // A signal stopped the command (`ui::Stoppable`): it ends as the signal
    // would have, and what failed as it stopped (a tool it interrupted) is
    // no news.
    ui::exit_if_stopped();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.downcast_ref::<rust_app::Restart>().is_some() => {
            ExitCode::from(rust_app::RESTART)
        }
        Err(e) => {
            ui::report(&e);
            ExitCode::FAILURE
        }
    }
}

fn run(mut command: Command) -> anyhow::Result<()> {
    let mut types = !matches!(command, Command::Trace { .. });
    if let Some(entry) = command.entry_mut() {
        *entry = entry::resolve(entry)?;
        // A Rust app has no TypeScript to declare things for.
        types &= !rust_app::is_root(entry);
    }
    // An app with Rust components of its own runs in its own build of this.
    if let Some(status) = rust_app::reexec(command.entry_mut().map(|e| e.as_path()))? {
        // As it ended: with its exit code, or by the signal that ended it.
        #[cfg(unix)]
        if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
            ui::exit_by_signal(signal);
        }
        std::process::exit(status.code().unwrap_or(1));
    }
    if types && let Some(entry) = command.entry_mut() {
        init::write_types_for(entry);
    }
    // A Rust app (`nsh.json` "root") has no bundle: it is built, traced,
    // run and packaged.
    let rust_command = matches!(
        command,
        Command::Build { .. } | Command::Trace { .. } | Command::Package(_) | Command::Run { .. }
    );
    if let Some(entry) = command.entry_mut()
        && rust_app::is_root(entry)
    {
        if !rust_command {
            anyhow::bail!(
                "{} declares a Rust app (\"root\"): it is built with `nsh build`, traced with `nsh trace`, \
                 run with `nsh run` and packaged with `nsh package`; the other commands take a TSX entry",
                ui::path(entry)
            );
        }
        if rust_app::current().is_none() {
            anyhow::bail!("nsh.json: \"root\" needs \"rust\", the path of the app's crate");
        }
    }
    dispatch(command)
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
            trace::TraceOptions {
                taps,
                tap,
                idle_ms,
                wait,
                duration,
                scroll,
                width,
                height,
                data_dir,
                os,
                safe_area: safe_area.unwrap_or_default(),
            },
        ),
        Command::Check { entry, tsc, args } => check::check(&entry, tsc.as_deref(), &args),
        Command::Dev {
            entry,
            port,
            release,
        } => {
            let session = dev::DevSession::start(&entry, port, release)?;
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
            trace,
            fps,
            target,
            device,
            name,
            size,
            script,
        } => run::run(
            &entry,
            port,
            release,
            run::HostFlags { trace, fps },
            target,
            device.as_deref(),
            run::HostWindow {
                name: name.as_deref(),
                size: size.as_deref(),
                script: script.as_deref(),
            },
        ),
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
        } => {
            let mut config = package::read_config(&entry)?;
            config.id = id.or(config.id);
            config.version = version.or(config.version);
            config.build = build.or(config.build);
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

/// The first import specifier two of `sets` share.
fn duplicate_specifier(sets: &[Registry]) -> Option<&'static str> {
    let mut seen = std::collections::HashSet::new();
    sets.iter()
        .filter_map(Registry::specifier)
        .find(|specifier| !seen.insert(*specifier))
}

#[cfg(test)]
mod duplicate_tests {
    use super::*;

    #[test]
    fn two_sets_of_one_specifier_are_refused() {
        let a = Registry::ui_set("app/a");
        let b = Registry::ui_set("app/b");
        assert_eq!(duplicate_specifier(&[a.clone(), b.clone()]), None);
        assert_eq!(
            duplicate_specifier(&[a, b, Registry::ui_set("app/a")]),
            Some("app/a")
        );
    }
}
