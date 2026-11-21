//! `nsh-launcher` runs the nsh CLI a project pins in `nsh.json`.
//!
//! A project writes:
//!
//! ```json
//! "toolchain": { "nsh": { "version": "0.6.1", "repository": "longbridge/nsh" } }
//! ```
//!
//! The launcher reads that pin, downloads the matching prebuilt distribution
//! once, verifies its SHA-256, caches it and execs its `nsh`. It never
//! compiles nsh and never rewrites the app.
//!
//! Install it as `nsh` ahead of a distribution's `bin`, or run it as
//! `nsh-launcher`. Its own commands start with `--launcher-`:
//! `--launcher-status`, `--launcher-verify`, `--launcher-prefetch`,
//! `--launcher-which`, `--launcher-help` and `--launcher-version`.

mod archive;
mod cache;
mod exec;
mod manifest;
mod release;

use std::{ffi::OsString, path::Path, process::ExitCode};

use anyhow::Context as _;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nsh-launcher: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if let Some(command) = args
        .first()
        .and_then(|arg| arg.to_str())
        .and_then(|arg| arg.strip_prefix("--launcher-"))
    {
        return launcher_command(command, &args[1..]);
    }
    let app = match manifest::find_app(&args) {
        Ok(app) => app,
        Err(_) => return exec::fallback(&args),
    };
    let config = manifest::read_config(&app)?;
    let Some(pin) = manifest::pin(&config)? else {
        return exec::fallback(&args);
    };
    let bin = cache::ensure(&app, &config, &pin)?;
    exec::exec(&bin, &args)
}

/// The launcher's own commands, which never reach the pinned CLI.
fn launcher_command(command: &str, args: &[OsString]) -> anyhow::Result<()> {
    match command {
        "help" => {
            print_help();
            Ok(())
        }
        "version" => {
            println!("nsh-launcher {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "status" | "verify" | "prefetch" | "which" => {
            let app = manifest::find_app(args)?;
            let config = manifest::read_config(&app)?;
            let pin = manifest::pin(&config)?.with_context(|| {
                format!(
                    "{}: nsh.json has no toolchain.nsh pin",
                    app.join("nsh.json").display()
                )
            })?;
            match command {
                "status" => status(&app, &config, &pin),
                "verify" => verify(&app, &config, &pin),
                "prefetch" => {
                    println!("{}", cache::ensure(&app, &config, &pin)?.display());
                    Ok(())
                }
                _ => {
                    let host = release::host()?;
                    println!(
                        "{}",
                        cache::toolchain_dir(&pin, host)?
                            .join("bin")
                            .join(exec::exe("nsh"))
                            .display()
                    );
                    Ok(())
                }
            }
        }
        other => anyhow::bail!("unknown launcher command --launcher-{other}; try --launcher-help"),
    }
}

fn status(app: &Path, config: &manifest::Config, pin: &manifest::Pin) -> anyhow::Result<()> {
    let host = release::host()?;
    let dir = cache::toolchain_dir(pin, host)?;
    let bin = dir.join("bin").join(exec::exe("nsh"));
    println!("app: {}", app.display());
    println!("pin: {} ({})", pin.version, pin.repository);
    println!("tag: {}", pin.tag);
    println!("host: {host}");
    println!("toolchain: {}", dir.display());
    println!(
        "selected: {} ({})",
        bin.display(),
        if bin.is_file() { "present" } else { "missing" }
    );
    println!(
        "nsh-core: {}",
        manifest::expected_rev(app, config)?
            .as_deref()
            .unwrap_or("not pinned (no Rust crate)")
    );
    Ok(())
}

fn verify(app: &Path, config: &manifest::Config, pin: &manifest::Pin) -> anyhow::Result<()> {
    let expected = manifest::expected_rev(app, config)?;
    let info = release::fetch(&pin.repository, &pin.tag)?;
    match (expected.as_deref(), info) {
        (Some(rev), Some(info)) => {
            if let Some(tag) = info.tag.as_deref()
                && tag != pin.tag
            {
                anyhow::bail!(
                    "{} describes tag {tag}, not {}",
                    release::RELEASE_INFO,
                    pin.tag
                );
            }
            let commit = info
                .commit
                .as_deref()
                .with_context(|| format!("{} has no commit in its nsh-release.json", pin.tag))?;
            if commit != rev {
                anyhow::bail!(
                    "{} is built from {commit}, but the app pins nsh-core at {rev}",
                    pin.tag
                );
            }
            println!(
                "ok: {} (nsh {}) is {rev}",
                pin.tag,
                info.version.as_deref().unwrap_or("unknown")
            );
        }
        (Some(rev), None) => anyhow::bail!(
            "{} has no nsh-release.json; cannot verify the pinned nsh-core {rev}",
            pin.tag
        ),
        (None, Some(_)) => println!("ok: {} (the app has no Rust crate to verify)", pin.tag),
        (None, None) => println!(
            "ok: {} (no Rust crate; the release has no nsh-release.json)",
            pin.tag
        ),
    }
    Ok(())
}

fn print_help() {
    println!(
        "\
nsh-launcher {}

Runs the nsh CLI a project pins in nsh.json.

Usage:
  nsh-launcher [nsh arguments...]          run the pinned CLI
  nsh-launcher --launcher-status [app]     show the pin, cache and selected CLI
  nsh-launcher --launcher-verify [app]     check the pin against the release and Cargo.lock
  nsh-launcher --launcher-prefetch [app]   download the pinned CLI now
  nsh-launcher --launcher-which [app]      print the selected CLI's path
  nsh-launcher --launcher-help
  nsh-launcher --launcher-version

The pin, in nsh.json:
  \"toolchain\": {{
    \"nsh\": {{ \"version\": \"0.6.1\", \"repository\": \"longbridge/nsh\" }}
  }}

`version` is an exact release version (no ranges or `latest`); `repository`
defaults to longbridge/nsh. The launcher downloads that version's prebuilt
distribution for this machine from the repository's GitHub releases, verifies
its SHA-256, caches it under `$NSH_CACHE` (or the platform's cache directory)
and runs it. A Rust app's `nsh-core` revision in Cargo.lock must match the
release's `nsh-release.json`, which `--launcher-verify` checks.

Install the launcher as `nsh` ahead of a distribution's `bin`, or keep the
distribution's `nsh` and run `nsh-launcher` explicitly.",
        env!("CARGO_PKG_VERSION")
    );
}
