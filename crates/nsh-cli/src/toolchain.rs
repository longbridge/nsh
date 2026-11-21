//! Launcher: runs the nsh the project pins in `nsh.json`, re-exec'ing this
//! same executable. There is only one nsh command on PATH.
//!
//! A project writes:
//!
//! ```json
//! "toolchain": {
//!   "nsh": { "version": "0.6.1" }
//! }
//! ```
//!
//! or a Git revision:
//!
//! ```json
//! "toolchain": {
//!   "nsh": { "rev": "<40-hex>", "repository": "longbridge/nsh" }
//! }
//! ```
//!
//! On every invocation this CLI checks the pin. If the pin matches the
//! version/revision this CLI was built from, the command runs in place.
//! Otherwise it locates or obtains the pinned CLI and `exec`s it with the
//! original arguments, before any subcommand runs. Cached toolchains live
//! under `$NSH_CACHE/toolchains/<tag-or-rev>/`.
//!
//! Trust: a pin from any repository other than the default `longbridge/nsh`
//! is declined unless acknowledged through `NSH_TOOLCHAIN_ALLOW` (a
//! comma-separated set of `owner/repo` entries) or interactively confirmed
//! on a TTY. Without that, a cloned project could otherwise escalate any
//! `nsh` invocation into executing arbitrary code from a fork — the
//! SHA-256 we verify only proves integrity against that release, not origin.

use std::{
    env, fs, io,
    io::Seek as _,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use anyhow::{Context as _, bail};
use serde::Deserialize;

use crate::ui;

/// Default upstream if the pin does not specify one.
const DEFAULT_REPOSITORY: &str = "longbridge/nsh";

/// The pin in `nsh.json`.
#[derive(Debug, Clone, Deserialize)]
struct Pin {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    rev: Option<String>,
    #[serde(default)]
    repository: Option<String>,
}

/// The toolchain block in `nsh.json`.
#[derive(Debug, Clone, Deserialize)]
struct Toolchain {
    nsh: Option<Pin>,
}

/// What the pin selects.
#[derive(Debug, Clone)]
pub(crate) enum Selection {
    /// The version in `nsh` package version (e.g. `0.6.1`).
    Release { version: String, repository: String },
    /// An exact Git revision.
    GitRev { rev: String, repository: String },
}

/// Where the launcher should land on disk, as the cache layout dictates.
fn toolchain_dir(tag_or_rev: &str, repository: &str) -> anyhow::Result<PathBuf> {
    let root = cache_root()?;
    // Cross-repo collisions avoided: include file-safed owner/repo in the path.
    let owner = repository.replace('/', "__");
    Ok(root.join("toolchains").join(owner).join(tag_or_rev))
}

fn cache_root() -> anyhow::Result<PathBuf> {
    if let Some(dir) = env::var_os("NSH_CACHE").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let home = env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Caches"))
    } else {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".cache")))
    };
    Ok(base
        .context("no cache directory: set NSH_CACHE")?
        .join("nsh"))
}

/// The executable file name for the current platform.
fn exe(name: &str) -> String {
    format!("{name}{}", env::consts::EXE_SUFFIX)
}

/// Reads `toolchain.nsh` in the nearest `nsh.json` (this directory or an
/// ancestor). Returns `None` if there's no file, no `toolchain` key, or an
/// empty pin.
fn read_pin(app_dir: &Path) -> anyhow::Result<Option<Pin>> {
    let mut current = Some(app_dir);
    while let Some(dir) = current {
        let path = dir.join("nsh.json");
        if path.is_file() {
            let text = fs::read_to_string(&path)?;
            // Deserialize into a broad map to ignore other fields.
            let value: serde_json::Value = serde_json::from_str(&text)?;
            let Some(tc) = value.get("toolchain") else {
                return Ok(None);
            };
            let tc: Toolchain = serde_json::from_value(tc.clone())
                .context("nsh.json: toolchain is not an object")?;
            return Ok(tc.nsh);
        }
        current = dir.parent();
    }
    Ok(None)
}

/// Parses the pin into a [`Selection`]. Errors clearly on invalid input.
fn parse(pin: Pin) -> anyhow::Result<Selection> {
    let repository = pin.repository.unwrap_or_else(|| "longbridge/nsh".into());
    if let Some(rev) = pin.rev {
        // Require 40-hex (full Git SHA).
        if rev.len() != 40 || !rev.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!(
                "toolchain.nsh.rev must be a full 40-hex Git commit, got {:?}",
                rev
            );
        }
        return Ok(Selection::GitRev { rev, repository });
    }
    if let Some(version) = pin.version {
        let v = version.trim();
        if v.is_empty() {
            bail!("toolchain.nsh.version is empty");
        }
        return Ok(Selection::Release {
            version: v.to_string(),
            repository,
        });
    }
    bail!("toolchain.nsh needs either \"version\" or \"rev\"")
}

/// What this CLI was built from.
fn built_rev() -> &'static str {
    env!("NSH_GIT_SHA_FULL")
}

/// Whether the selection matches the running CLI exactly.
fn matches_self(selection: &Selection) -> bool {
    match selection {
        Selection::GitRev { rev, .. } => rev == built_rev(),
        Selection::Release { version, .. } => version == env!("CARGO_PKG_VERSION"),
    }
}

/// Derives a tag for a release (prefixes `v` if missing).
fn tag_of(version: &str) -> String {
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

/// The directory to install this selection into.
fn target_dir(selection: &Selection) -> anyhow::Result<PathBuf> {
    match selection {
        Selection::Release {
            version,
            repository,
        } => toolchain_dir(&tag_of(version), repository),
        Selection::GitRev { rev, repository } => toolchain_dir(rev, repository),
    }
}

/// The built `nsh` in the installed toolchain directory.
fn bin_in(dir: &Path) -> PathBuf {
    dir.join("bin").join(exe("nsh"))
}

/// Replaces this process with the selected CLI.
#[cfg(unix)]
fn exec(bin: &Path, args: &[std::ffi::OsString]) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt as _;
    let error = Command::new(bin).args(args).exec();
    Err(anyhow::Error::new(error).context(format!("exec {}", bin.display())))
}

#[cfg(windows)]
fn exec(bin: &Path, args: &[std::ffi::OsString]) -> anyhow::Result<()> {
    // Windows has no `exec`: spawn the pinned CLI, wait for it, then exit
    // with its status so the caller sees it as the running nsh. Ctrl+C / job
    // object signals propagate to the child; the wrapper does no work.
    let status = Command::new(bin)
        .args(args)
        .status()
        .with_context(|| format!("run {}", bin.display()))?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Install a release from GitHub: download `nsh-<host>.{tar.gz,zip}`, verify
/// SHA-256, unpack into the target directory.
fn install_release(version: &str, repository: &str, dest: &Path) -> anyhow::Result<()> {
    let tag = tag_of(version);
    let host = host()?;
    let asset = format!(
        "nsh-{host}.{}",
        if cfg!(windows) { "zip" } else { "tar.gz" }
    );
    let base = format!("https://github.com/{repository}/releases/download/{tag}");
    let archive = dest.join(format!(".{asset}.part"));

    // Fetch .sha256 to verify the body.
    let sum_url = format!("{base}/{asset}.sha256");
    let sum = download_string(&sum_url)?;
    let expected = sum
        .split_whitespace()
        .next()
        .filter(|s| s.len() == 64)
        .with_context(|| format!("{sum_url}: no SHA-256"))?
        .to_ascii_lowercase();

    // Stream the archive.
    let url = format!("{base}/{asset}");
    get_bytes(&url, &archive)?;
    let actual = {
        use sha2::{Digest as _, Sha256};
        let bytes = fs::read(&archive)?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        format!("{:x}", hasher.finalize())
    };
    if actual != expected {
        bail!("download of {url}: SHA-256 mismatch");
    }
    unpack(&archive, dest)?;
    let _ = fs::remove_file(&archive);
    let _ = mark_executable(&bin_in(dest));
    Ok(())
}

/// Install by building from the cargo git checkout of the revision. The
/// source is whatever Cargo resolved (likely in `~/.cargo/git/checkouts`).
fn install_by_building(rev: &str, repository: &str, dest: &Path) -> anyhow::Result<()> {
    let checkout = cargo_checkout_for(rev, repository)
        .with_context(|| format!("no cargo checkout for {repository}@{rev}"))?;
    ui::warn(format!(
        "toolchain: building nsh from {} at {rev}",
        checkout.display()
    ));
    // Build into the cache so cache cleans don't trigger re-builds.
    let target = dest.join("._build_target");
    fs::create_dir_all(&target)?;
    let status = Command::new(cargo_bin())
        .args(["build", "--release", "-p", "nsh-cli", "--target-dir"])
        .arg(&target)
        .current_dir(&checkout)
        .status()
        .context("cargo build --release -p nsh-cli")?;
    if !status.success() {
        bail!(
            "cargo build --release -p nsh-cli failed at {}",
            checkout.display()
        );
    }
    let bin = target.join("release").join(exe("nsh-cli"));
    if !bin.is_file() {
        bail!("cargo build produced no nsh-cli at {}", bin.display());
    }
    let out_bin = bin_in(dest);
    if let Some(parent) = out_bin.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&bin, &out_bin)?;
    let _ = fs::remove_dir_all(&target);
    let _ = mark_executable(&out_bin);
    Ok(())
}

fn cargo_bin() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

fn mark_executable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut p = fs::metadata(path)?.permissions();
        p.set_mode(0o755);
        fs::set_permissions(path, p)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Locates the locked checkout of `repository@rev` in Cargo's git cache.
fn cargo_checkout_for(rev: &str, repository: &str) -> Option<PathBuf> {
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))?;
    let git_checkouts = cargo_home.join("git").join("checkouts");
    let read = fs::read_dir(&git_checkouts).ok()?;
    // Find a directory whose canonical URL-ish name matches the repository.
    // Cargo's checkout naming: "<name>-<hash>".
    let needle = repository
        .rsplit('/')
        .next()
        .unwrap_or("nsh")
        .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
    for entry in read.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with(&format!("{needle}-")) && name_str != needle {
            continue;
        }
        // Each entry has REV-{short} subdirs matching the locked commit.
        if let Ok(revisions) = fs::read_dir(entry.path()) {
            for rev_dir in revisions.flatten() {
                let rev_name = rev_dir.file_name().to_string_lossy().to_string();
                if !rev.starts_with(&rev_name) {
                    continue;
                }
                // Verify it's the full pinned revision.
                let checked = Command::new("git")
                    .arg("-C")
                    .arg(rev_dir.path())
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()?;
                if !checked.status.success() {
                    continue;
                }
                let got_rev = String::from_utf8_lossy(&checked.stdout).trim().to_string();
                if got_rev == rev {
                    return Some(rev_dir.path());
                }
            }
        }
    }
    None
}

fn host() -> anyhow::Result<&'static str> {
    Ok(match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-arm64",
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-arm64",
        (os, arch) => bail!("no prebuilt nsh for {os} {arch}"),
    })
}

fn download_string(url: &str) -> anyhow::Result<String> {
    let bytes = download_bytes(url)?;
    String::from_utf8(bytes).context(format!("{url}: not UTF-8"))
}

fn download_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(true)
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", concat!("nsh/", env!("CARGO_PKG_VERSION")))
        .call()
        .context(format!("GET {url}"))?;
    Ok(response.body_mut().read_to_vec()?)
}

fn get_bytes(url: &str, dest: &Path) -> anyhow::Result<()> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(true)
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", concat!("nsh/", env!("CARGO_PKG_VERSION")))
        .call()
        .context(format!("GET {url}"))?;
    let mut file = fs::File::create(dest)?;
    io::copy(&mut response.body_mut().as_reader(), &mut file)?;
    file.sync_all()?;
    Ok(())
}

/// Untar / unzip the archive `archive` into `into`, flattening a single
/// top-level `nsh/` wrapper if present.
fn unpack(archive: &Path, into: &Path) -> anyhow::Result<()> {
    let stage = into.join("._unpack");
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    fs::create_dir_all(&stage)?;
    let mut file = fs::File::open(archive)?;
    let mut magic = [0u8; 2];
    io::Read::read_exact(&mut file, &mut magic)?;
    file.rewind()?;
    match &magic {
        [0x1f, 0x8b] => {
            let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
            tar.unpack(&stage)?;
        }
        b"PK" => {
            zip::ZipArchive::new(file)?.extract(&stage)?;
        }
        _ => bail!("{}: not a tar.gz or zip", archive.display()),
    }
    // Move a single top-level `nsh/` wrapper up so `dest/bin/nsh` exists.
    let wrapper = stage.join("nsh");
    let contents: Vec<PathBuf> = fs::read_dir(&stage)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    let top_is_single_nsh = contents.len() == 1 && wrapper.exists();
    if top_is_single_nsh {
        for entry in fs::read_dir(&wrapper)? {
            let entry = entry?;
            let dst = into.join(entry.file_name());
            if dst.exists() {
                let _ = fs::remove_dir_all(&dst);
            }
            fs::rename(entry.path(), &dst)?;
        }
    } else {
        for entry in contents {
            let dst = into.join(entry.file_name().unwrap());
            if dst.exists() {
                let _ = fs::remove_dir_all(&dst);
            }
            fs::rename(&entry, &dst)?;
        }
    }
    let _ = fs::remove_dir_all(&stage);
    Ok(())
}

/// The launcher's main entry: decide whether to run in place or re-exec.
///
/// `app_dir` is the directory containing (or nearest to) `nsh.json`.
/// `args` are the original nsh arguments (without the program name).
pub(crate) fn run(app_dir: &Path, args: &[std::ffi::OsString]) -> Option<ExitCode> {
    let pin = match read_pin(app_dir) {
        Ok(Some(p)) => p,
        Ok(None) | Err(_) => return None,
    };
    let selection = match parse(pin) {
        Ok(s) => s,
        Err(e) => {
            ui::error(format!("toolchain pin: {e}"));
            return Some(ExitCode::FAILURE);
        }
    };
    if matches_self(&selection) {
        // Running it in place: the rest of nsh proceeds.
        return None;
    }
    if let Err(e) = permission_to_install(&selection) {
        ui::error(format!("{e}"));
        return Some(ExitCode::FAILURE);
    }
    let dir = match target_dir(&selection) {
        Ok(d) => d,
        Err(e) => {
            ui::error(format!("toolchain cache dir: {e}"));
            return Some(ExitCode::FAILURE);
        }
    };
    let bin = bin_in(&dir);
    if !bin.is_file() {
        ui::warn(format!("toolchain: installing {}", dir.display()));
        if let Err(e) = install(&selection, &dir) {
            ui::error(format!(
                "cannot obtain the pinned nsh at {}: {e}",
                dir.display()
            ));
            return Some(ExitCode::FAILURE);
        }
    }
    // argv[0] is set by `Command::new`; pass the original user args as the tail.
    let mut target_args: Vec<std::ffi::OsString> = Vec::with_capacity(args.len());
    target_args.extend(args.iter().cloned());
    match exec(&bin, &target_args) {
        Ok(()) => unreachable!(),
        Err(e) => {
            ui::error(format!("cannot execute {}: {e}", bin.display()));
            Some(ExitCode::FAILURE)
        }
    }
}

fn install(selection: &Selection, dest: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dest)?;
    match selection {
        Selection::Release {
            version,
            repository,
        } => install_release(version, repository, dest),
        Selection::GitRev { rev, repository } => install_by_building(rev, repository, dest),
    }
}

/// Refuses a pin that moves the CLI's trust root away from the default
/// `longbridge/nsh` without an explicit acknowledgment.
///
/// `NSH_TOOLCHAIN_ALLOW=owner/repo,other/repo` accepts on a non-interactive
/// shell; on a TTY and without an ack, we ask. CI (no TTY, no ack, non-default
/// repository) is an error.
fn permission_to_install(selection: &Selection) -> anyhow::Result<()> {
    let repository = match selection {
        Selection::Release { repository, .. } | Selection::GitRev { repository, .. } => {
            repository.as_str()
        }
    };
    if repository == DEFAULT_REPOSITORY {
        return Ok(());
    }
    if env::var_os("NSH_TOOLCHAIN_ALLOW")
        .map(|list| {
            list.to_string_lossy()
                .split(',')
                .any(|r| r.trim() == repository)
        })
        .unwrap_or(false)
    {
        return Ok(());
    }
    // Last resort: interactive confirm only when on a terminal.
    use io::IsTerminal as _;
    if io::stdin().is_terminal() {
        ui::warn(format!(
            "the project pins nsh to {repository}, which is not the default {DEFAULT_REPOSITORY}"
        ));
        ui::warn(format!(
            "continuing will download and execute a CLI from {repository}"
        ));
        print!("proceed? [y/N] ");
        let _ = io::Write::flush(&mut io::stdout());
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if answer.trim().eq_ignore_ascii_case("y") {
            return Ok(());
        }
    }
    bail!(
        "toolchain pin to {repository} is not the default {DEFAULT_REPOSITORY}; \
         set NSH_TOOLCHAIN_ALLOW={repository} (or run on a TTY and confirm) to trust it"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_pin_is_parsed() {
        let pin = Pin {
            version: Some("0.6.1".into()),
            rev: None,
            repository: None,
        };
        let s = parse(pin).unwrap();
        match s {
            Selection::Release {
                version,
                repository,
            } => {
                assert_eq!(version, "0.6.1");
                assert_eq!(repository, "longbridge/nsh");
            }
            Selection::GitRev { .. } => panic!("expected release"),
        }
    }

    #[test]
    fn a_git_rev_requires_full_40_hex() {
        for rev in [
            "abc".to_string(),
            "deadbee".to_string(),
            "abcdef0123456789abcdef0123456789abcdef01".to_string(),
        ] {
            let pin = Pin {
                version: None,
                rev: Some(rev.clone()),
                repository: None,
            };
            if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
                assert!(parse(pin).is_ok());
            } else {
                assert!(parse(pin).is_err(), "{rev}");
            }
        }
    }

    #[test]
    fn a_release_pin_selects_the_running_version() {
        let pin = Pin {
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            rev: None,
            repository: None,
        };
        let s = parse(pin).unwrap();
        assert!(matches_self(&s));
    }

    #[test]
    fn a_git_rev_pin_matches_the_current_build() {
        if env!("NSH_GIT_SHA_FULL") == "unknown" {
            return;
        }
        let pin = Pin {
            version: None,
            rev: Some(env!("NSH_GIT_SHA_FULL").to_string()),
            repository: None,
        };
        let s = parse(pin).unwrap();
        assert!(matches_self(&s));
    }

    #[test]
    fn a_release_pin_v_prefix_is_added() {
        assert_eq!(tag_of("0.6.1"), "v0.6.1");
        assert_eq!(tag_of("v0.6.1"), "v0.6.1");
    }

    #[test]
    fn toolchain_dir_is_under_the_cache_root() {
        let dir = toolchain_dir("v0.6.1", "longbridge/nsh").unwrap();
        // Must be under some user-specific cache root, not absolute /.
        assert!(
            dir.ends_with("toolchains/longbridge__nsh/v0.6.1"),
            "{dir:?}"
        );
    }

    #[test]
    fn a_non_default_repository_requires_an_ack() {
        let pin = Pin {
            version: Some("0.6.1".into()),
            rev: None,
            repository: Some("acme/nsh".into()),
        };
        let selection = parse(pin).unwrap();
        // Without NSH_TOOLCHAIN_ALLOW and no TTY we expect an error.
        let err = permission_to_install(&selection).unwrap_err();
        assert!(err.to_string().contains("acme/nsh"), "{err}");
    }

    #[test]
    fn the_default_repository_is_implicitly_allowed() {
        let pin = Pin {
            version: Some("0.6.1".into()),
            rev: None,
            repository: None,
        };
        assert!(permission_to_install(&parse(pin).unwrap()).is_ok());
    }
}
