//! Running a command for an app on another version of nsh. The code an `nsh`
//! generates calls the API of its own version, so an app's crates and the
//! command line must agree; when they don't, the command is handed to the
//! `nsh` of the app's version, taken from its release or built from its
//! checkout, as a toolchain manager would.

use std::{
    ffi::OsString,
    fmt, fs,
    io::{Cursor, Read as _},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use anyhow::{Context as _, bail};
use sha2::{Digest as _, Sha256};

use crate::ui;

const RELEASES: &str = "https://github.com/longbridge/nsh/releases/download";
const CRATE: &str = "https://crates.io/api/v1/crates/nsh-core";

/// The nsh an app's crates want.
#[derive(Debug)]
pub(crate) enum Wanted {
    /// A requirement on crates.io (`=0.3.0`, `0.3`).
    Requirement(String),
    /// A Git checkout Cargo resolved, at this version.
    Git { checkout: PathBuf, version: String },
    /// A checkout the crates depend on by path, which may hold changes.
    Source(PathBuf),
}

/// The error of an app whose crates are on another version of nsh.
#[derive(Debug)]
pub(crate) struct OtherVersion {
    pub(crate) wanted: Wanted,
    pub(crate) message: String,
}

impl fmt::Display for OtherVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OtherVersion {}

impl OtherVersion {
    /// Runs this process's command with the `nsh` the app wants. If that
    /// `nsh` can't be had, the error says why, after what the mismatch is.
    pub(crate) fn run_command(&self) -> anyhow::Result<ExitCode> {
        let nsh = self.nsh().with_context(|| self.message.clone())?;
        run(&nsh, std::env::args_os().skip(1).collect())
    }

    fn nsh(&self) -> anyhow::Result<PathBuf> {
        match &self.wanted {
            Wanted::Requirement(req) => {
                let versions = published_versions()?;
                let version = newest_matching(&versions, req)?;
                released(&version)
            }
            // A branch between releases has no release of its version.
            Wanted::Git { checkout, version } => released(version).or_else(|e| {
                ui::debug(format!("{e:#}"));
                built(checkout)
            }),
            Wanted::Source(checkout) => built(checkout),
        }
    }
}

/// The release asset of this machine, as the install scripts pick it.
fn asset() -> anyhow::Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "nsh-macos-arm64.tar.gz",
        ("linux", "x86_64") => "nsh-linux-x86_64.tar.gz",
        ("windows", "x86_64") => "nsh-windows-x86_64.zip",
        (os, arch) => bail!("no prebuilt nsh for {os}-{arch}"),
    })
}

fn exe_name() -> String {
    format!("nsh{}", std::env::consts::EXE_SUFFIX)
}

/// The `nsh` of release `version`, downloaded into the cache the first time.
fn released(version: &str) -> anyhow::Result<PathBuf> {
    let dir = crate::check::cache()?.join(format!("nsh-{version}"));
    let exe = dir.join("bin").join(exe_name());
    if exe.is_file() {
        return Ok(exe);
    }
    let asset = asset()?;
    let url = format!("{RELEASES}/v{version}/{asset}");
    ui::note(format!("This app uses nsh {version}: downloading it"));
    let archive = download(&url)?;
    let checksum = String::from_utf8(download(&format!("{url}.sha256"))?)
        .with_context(|| format!("{url}.sha256"))?;
    verify(&archive, &checksum).with_context(|| url.clone())?;
    // Unpacked next to its final place, then moved there: a run stopped
    // halfway leaves no partial nsh behind.
    let partial = dir.with_extension(format!("partial-{}", std::process::id()));
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(&partial)?;
    unpack(&archive, asset, &partial).with_context(|| format!("unpack {url}"))?;
    let unpacked = partial.join("nsh");
    if !unpacked.join("bin").join(exe_name()).is_file() {
        bail!("{url} holds no nsh/bin/{}", exe_name());
    }
    if fs::rename(&unpacked, &dir).is_err() && !exe.is_file() {
        bail!("could not move nsh {version} to {}", dir.display());
    }
    let _ = fs::remove_dir_all(&partial);
    Ok(exe)
}

/// The `nsh` of the checkout, built in it (Cargo does nothing if it is built).
fn built(checkout: &Path) -> anyhow::Result<PathBuf> {
    ui::note(format!(
        "This app uses the nsh in {}: building it",
        ui::path(checkout)
    ));
    let target = checkout.join("target");
    // In the checkout, for its `rust-toolchain.toml`.
    let status = Command::new("cargo")
        .args(["build", "-p", "nsh-cli", "--target-dir"])
        .arg(&target)
        .current_dir(checkout)
        .status()
        .context("run cargo")?;
    if !status.success() {
        bail!("building nsh in {} failed", checkout.display());
    }
    Ok(target.join("debug").join(exe_name()))
}

fn download(url: &str) -> anyhow::Result<Vec<u8>> {
    let mut data = Vec::new();
    ureq::get(url)
        .header("User-Agent", "nsh-cli")
        .call()
        .with_context(|| format!("download {url}"))?
        .into_body()
        .into_reader()
        .read_to_end(&mut data)
        .with_context(|| format!("download {url}"))?;
    Ok(data)
}

/// Checks `data` against a `sha256sum` line.
fn verify(data: &[u8], checksum: &str) -> anyhow::Result<()> {
    let expected = checksum.split_whitespace().next().unwrap_or_default();
    let actual: String = Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("checksum mismatch: expected {expected}, got {actual}");
    }
    Ok(())
}

fn unpack(archive: &[u8], asset: &str, into: &Path) -> anyhow::Result<()> {
    if asset.ends_with(".zip") {
        zip::ZipArchive::new(Cursor::new(archive))?.extract(into)?;
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(archive)).unpack(into)?;
    }
    Ok(())
}

/// The versions of nsh on crates.io, without the yanked.
fn published_versions() -> anyhow::Result<Vec<String>> {
    let data = download(CRATE)?;
    let index: serde_json::Value = serde_json::from_slice(&data).context(CRATE)?;
    Ok(index["versions"]
        .as_array()
        .context(CRATE)?
        .iter()
        .filter(|v| v["yanked"].as_bool() != Some(true))
        .filter_map(|v| v["num"].as_str().map(str::to_owned))
        .collect())
}

/// The newest of `versions` that `req` accepts, as Cargo would resolve it.
fn newest_matching(versions: &[String], req: &str) -> anyhow::Result<String> {
    let parsed = semver::VersionReq::parse(req).with_context(|| format!("requirement {req}"))?;
    versions
        .iter()
        .filter_map(|v| semver::Version::parse(v).ok())
        .filter(|v| parsed.matches(v))
        .max()
        .map(|v| v.to_string())
        .with_context(|| format!("no published nsh matches {req}"))
}

/// Replaces this process with `nsh`, or runs it to the end where a process
/// can't be replaced.
fn run(nsh: &Path, args: Vec<OsString>) -> anyhow::Result<ExitCode> {
    let mut command = Command::new(nsh);
    command.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        let error = command.exec();
        Err(error).with_context(|| format!("run {}", nsh.display()))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .with_context(|| format!("run {}", nsh.display()))?;
        Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_matching_takes_what_cargo_would() {
        let versions = ["0.3.0", "0.3.2", "0.4.0"].map(str::to_owned);
        assert_eq!(newest_matching(&versions, "=0.3.0").unwrap(), "0.3.0");
        assert_eq!(newest_matching(&versions, "0.3").unwrap(), "0.3.2");
        assert!(newest_matching(&versions, "0.9").is_err());
    }

    #[test]
    fn verify_reads_a_sha256sum_line() {
        let sum = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        verify(b"hello", &format!("{sum}  nsh.tar.gz\n")).unwrap();
        assert!(verify(b"other", sum).is_err());
    }
}
