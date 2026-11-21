//! The release side: this machine's asset, its URL, the release's
//! `nsh-release.json`, and the verified download.

use std::{
    fs,
    io::{Read as _, Write as _},
    path::Path,
};

use anyhow::{Context as _, bail};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

/// Where releases are published unless `nsh.json` names another repository.
pub(crate) const DEFAULT_REPOSITORY: &str = "longbridge/nsh";

/// The release's description of itself, an asset of the release.
pub(crate) const RELEASE_INFO: &str = "nsh-release.json";

/// GitHub's origin; the release URLs hang off it.
pub(crate) const BASE: &str = "https://github.com";

const USER_AGENT: &str = concat!("nshx/", env!("CARGO_PKG_VERSION"));

/// What `nsh-release.json` says.
#[derive(Debug, Deserialize)]
pub(crate) struct Info {
    #[serde(default)]
    pub(crate) version: Option<String>,
    #[serde(default)]
    pub(crate) commit: Option<String>,
    #[serde(default)]
    pub(crate) tag: Option<String>,
}

/// This machine as the release assets name it (`macos-arm64`).
pub(crate) fn host() -> anyhow::Result<&'static str> {
    host_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn host_for(os: &str, arch: &str) -> anyhow::Result<&'static str> {
    Ok(match (os, arch) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-arm64",
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-arm64",
        (os, arch) => bail!("no prebuilt nsh for {os} {arch}"),
    })
}

/// The distribution archive this machine downloads.
pub(crate) fn asset(host: &str) -> String {
    asset_for(cfg!(windows), host)
}

fn asset_for(windows: bool, host: &str) -> String {
    if windows {
        format!("nsh-{host}.zip")
    } else {
        format!("nsh-{host}.tar.gz")
    }
}

/// `base/repository/releases/download/tag/name`.
pub(crate) fn url(base: &str, repository: &str, tag: &str, name: &str) -> String {
    format!(
        "{}/{repository}/releases/download/{tag}/{name}",
        base.trim_end_matches('/')
    )
}

/// The release's `nsh-release.json`, or `None` when the release does not
/// carry one (releases from before nshx did).
pub(crate) fn fetch(repository: &str, tag: &str) -> anyhow::Result<Option<Info>> {
    fetch_at(BASE, repository, tag)
}

/// [`fetch`] from another origin (a mirror, or a test's local host).
pub(crate) fn fetch_at(base: &str, repository: &str, tag: &str) -> anyhow::Result<Option<Info>> {
    let agent = agent();
    let url = url(base, repository, tag, RELEASE_INFO);
    let response = get(&agent, &url)?;
    let status = response.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        bail!("GET {url}: {status}");
    }
    let body = response
        .into_body()
        .read_to_string()
        .with_context(|| format!("GET {url}"))?;
    let info = serde_json::from_str(&body)
        .with_context(|| format!("{url} is not a release description"))?;
    Ok(Some(info))
}

/// Downloads `name` of the release to `to`, checked against the release's
/// `.sha256` beside it.
pub(crate) fn download(
    base: &str,
    repository: &str,
    tag: &str,
    name: &str,
    to: &Path,
) -> anyhow::Result<()> {
    let agent = agent();
    let url = url(base, repository, tag, name);
    let sum_url = format!("{url}.sha256");
    let response = get(&agent, &sum_url)?;
    if !response.status().is_success() {
        bail!("GET {sum_url}: {}", response.status());
    }
    let sum = response
        .into_body()
        .read_to_string()
        .with_context(|| format!("GET {sum_url}"))?;
    let expected = sum
        .split_whitespace()
        .next()
        .filter(|hash| hash.len() == 64)
        .with_context(|| format!("{sum_url} holds no SHA-256"))?
        .to_ascii_lowercase();

    let response = get(&agent, &url)?;
    if !response.status().is_success() {
        bail!("GET {url}: {}", response.status());
    }
    let mut reader = response.into_body().into_reader();
    let mut file = fs::File::create(to).with_context(|| format!("create {}", to.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 256 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("GET {url}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .with_context(|| format!("write {}", to.display()))?;
    }
    file.sync_all()?;
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        bail!("the download of {url} does not match its SHA-256");
    }
    Ok(())
}

/// Errors as statuses; [`get`] follows redirects itself.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into()
}

/// Follows redirects (GitHub serves release assets from another host).
fn get(agent: &ureq::Agent, url: &str) -> anyhow::Result<ureq::http::Response<ureq::Body>> {
    let mut url = url.to_owned();
    for _ in 0..10 {
        let response = agent
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .call()
            .with_context(|| format!("GET {url}"))?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .with_context(|| format!("GET {url}: redirect without Location"))?;
        if !location.starts_with("http://") && !location.starts_with("https://") {
            bail!("GET {url}: redirect to {location:?} is not absolute");
        }
        url = location.to_owned();
    }
    bail!("GET {url}: too many redirects")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platforms_are_named_as_the_releases_name_them() {
        assert_eq!(host_for("macos", "aarch64").unwrap(), "macos-arm64");
        assert_eq!(host_for("windows", "x86_64").unwrap(), "windows-x86_64");
        assert!(host_for("freebsd", "x86_64").is_err());
    }

    #[test]
    fn assets_and_urls_follow_the_release_layout() {
        assert_eq!(asset_for(false, "macos-arm64"), "nsh-macos-arm64.tar.gz");
        assert_eq!(asset_for(true, "windows-x86_64"), "nsh-windows-x86_64.zip");
        assert_eq!(
            url(
                "https://github.com/",
                "longbridge/nsh",
                "v0.6.1",
                "nsh-macos-arm64.tar.gz"
            ),
            "https://github.com/longbridge/nsh/releases/download/v0.6.1/nsh-macos-arm64.tar.gz"
        );
    }
}
