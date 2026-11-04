//! The toolchain's side of app updates (`docs/dev/update.md`): `nsh.json`'s
//! `"update"` goes into the package, packages also leave an archive named
//! for their platform, and `nsh update-manifest` writes the manifest
//! describing the archives of one version.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};
use nsh_update::{Asset, CONFIG_ASSET, Config, MANIFEST, Manifest, SIGNATURE};
use serde::Deserialize;

use crate::{package::AppConfig, ui};

/// `nsh.json`'s `"update"`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdateConfig {
    /// `owner/repo`: updates from its GitHub Releases.
    pub(crate) github: Option<String>,
    /// The manifest's URL on a host of the app's own.
    pub(crate) url: Option<String>,
    /// The manifests' Ed25519 key (`nsh keygen`'s `.pub`).
    pub(crate) public_key: Option<String>,
}

/// The app's version, as packages have it.
fn version(config: &AppConfig) -> &str {
    config.version.as_deref().unwrap_or("1.0.0")
}

/// The checked configuration of an app with `"update"`.
fn runtime_config(config: &AppConfig, id: &str) -> anyhow::Result<Option<Config>> {
    let Some(update) = &config.update else {
        return Ok(None);
    };
    let runtime = Config {
        id: id.to_owned(),
        version: version(config).to_owned(),
        github: update.github.clone(),
        url: update.url.clone(),
        public_key: update.public_key.clone(),
    };
    runtime.validate().context("nsh.json")?;
    Ok(Some(runtime))
}

/// Adds the configuration to the package's assets, where `nsh/update` reads
/// it on every platform.
pub(crate) fn add_config(
    config: &AppConfig,
    id: &str,
    out: &Path,
    built: &mut crate::build::Built,
) -> anyhow::Result<()> {
    let Some(runtime) = runtime_config(config, id)? else {
        return Ok(());
    };
    fs::create_dir_all(out)?;
    let source = out.join(format!(".{CONFIG_ASSET}"));
    fs::write(&source, serde_json::to_vec(&runtime)?)?;
    built.assets.push(crate::build::Asset {
        path: CONFIG_ASSET.to_owned(),
        source,
    });
    Ok(())
}

/// `<name>-<version>-<os>-<arch>.<extension>`: what a release publishes, one
/// per platform (desktop packages are built on the platform they are for).
fn archive_name(name: &str, version: &str, extension: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("{name}-{version}-{}.{extension}", nsh_update::platform())
}

/// Copies the package's archive under its release name, for an app with
/// `"update"`.
pub(crate) fn copy_archive(
    config: &AppConfig,
    name: &str,
    archive: &Path,
    out: &Path,
) -> anyhow::Result<()> {
    if config.update.is_none() {
        return Ok(());
    }
    let extension = match archive.extension().and_then(|e| e.to_str()) {
        Some("zip") => "zip",
        _ => "tar.gz",
    };
    let release = out.join(archive_name(name, version(config), extension));
    fs::copy(archive, &release).with_context(|| format!("copy {}", archive.display()))?;
    ui::done(format!("update archive {}", ui::path(&release)));
    Ok(())
}

/// Archives a macOS `.app` under its release name, for an app with
/// `"update"`. `tar` keeps the bundle's links and modes, which its
/// signature covers.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn archive_macos_app(
    config: &AppConfig,
    name: &str,
    app: &Path,
    out: &Path,
) -> anyhow::Result<()> {
    if config.update.is_none() {
        return Ok(());
    }
    let release = out.join(archive_name(name, version(config), "tar.gz"));
    let output = Command::new("tar")
        .arg("-czf")
        .arg(&release)
        .arg("-C")
        .arg(out)
        .arg(app.file_name().context("the app has no name")?)
        .output()
        .context("run tar")?;
    if !output.status.success() {
        bail!(
            "creating {} failed: {}",
            release.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    ui::done(format!("update archive {}", ui::path(&release)));
    Ok(())
}

/// Options of `nsh update-manifest`.
pub(crate) struct ManifestOptions<'a> {
    pub(crate) archives: &'a [PathBuf],
    pub(crate) entry: &'a Path,
    pub(crate) key: Option<&'a Path>,
    pub(crate) notes: Option<&'a Path>,
    pub(crate) links: &'a [String],
    pub(crate) base_url: Option<&'a str>,
    pub(crate) out: &'a Path,
}

/// The platform in an archive's name, after its version.
fn archive_platform(file: &str, version: &str) -> Option<String> {
    let marker = format!("-{version}-");
    let start = file.rfind(&marker)? + marker.len();
    let rest = &file[start..];
    let platform = rest
        .strip_suffix(".tar.gz")
        .or_else(|| rest.strip_suffix(".zip"))?;
    (!platform.is_empty()).then(|| platform.to_owned())
}

/// `nsh update-manifest`.
pub(crate) fn manifest(options: ManifestOptions<'_>) -> anyhow::Result<()> {
    let entry = dunce::canonicalize(options.entry)?;
    let entry = if entry.is_dir() {
        entry.join("App.tsx")
    } else {
        entry
    };
    let config = crate::package::read_config(&entry)?;
    let (_, id) = crate::package::release_name_and_id(&entry, &config)?;
    let Some(runtime) = runtime_config(&config, &id)? else {
        bail!("nsh.json has no \"update\"");
    };
    let version = version(&config);
    let mut platforms = std::collections::BTreeMap::new();
    for archive in options.archives {
        let file = archive
            .file_name()
            .and_then(|f| f.to_str())
            .with_context(|| format!("{} has no file name", archive.display()))?;
        let Some(platform) = archive_platform(file, version) else {
            bail!(
                "{file}: expected <name>-{version}-<os>-<arch>.tar.gz or .zip, as `nsh package` names them"
            );
        };
        let bytes = fs::read(archive).with_context(|| format!("read {}", archive.display()))?;
        let url = match options.base_url {
            Some(base) => format!("{}/{file}", base.trim_end_matches('/')),
            None => file.to_owned(),
        };
        platforms.insert(
            platform,
            Asset {
                url,
                sha256: Some(nsh_update::sha256(&bytes)),
                size: Some(bytes.len() as u64),
            },
        );
    }
    for link in options.links {
        let Some((platform @ ("ios" | "android"), url)) = link.split_once('=') else {
            bail!("--link {link}: expected ios=<url> or android=<url>");
        };
        platforms.insert(
            platform.to_owned(),
            Asset {
                url: url.to_owned(),
                sha256: None,
                size: None,
            },
        );
    }
    let notes = options
        .notes
        .map(fs::read_to_string)
        .transpose()
        .context("read the notes")?;
    let manifest = Manifest {
        id,
        version: version.to_owned(),
        notes,
        platforms,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    fs::create_dir_all(options.out)?;
    let path = options.out.join(MANIFEST);
    fs::write(&path, &bytes)?;
    ui::done(format!("wrote {}", ui::path(&path)));
    match (options.key, &runtime.public_key) {
        (Some(key), Some(public)) => {
            let secret =
                fs::read_to_string(key).with_context(|| format!("read {}", key.display()))?;
            let signature = nsh_update::sign(&secret, &bytes)?;
            // Signed with the wrong key, every app would refuse the release.
            nsh_update::verify(public, &bytes, signature.as_bytes())
                .context("--key is not the key of nsh.json's update.publicKey")?;
            let path = options.out.join(SIGNATURE);
            fs::write(&path, format!("{signature}\n"))?;
            ui::done(format!("wrote {}", ui::path(&path)));
        }
        (None, Some(_)) => {
            bail!("nsh.json has update.publicKey: pass --key, or apps refuse this manifest")
        }
        (Some(_), None) => bail!("--key without update.publicKey in nsh.json: apps check nothing"),
        (None, None) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_name_their_platform() {
        assert_eq!(
            archive_platform("My-App-1.2.0-linux-x86_64.tar.gz", "1.2.0").as_deref(),
            Some("linux-x86_64")
        );
        assert_eq!(
            archive_platform("App-1.2.0-windows-aarch64.zip", "1.2.0").as_deref(),
            Some("windows-aarch64")
        );
        assert_eq!(
            archive_platform("App-1.1.0-linux-x86_64.tar.gz", "1.2.0"),
            None
        );
        assert_eq!(archive_platform("App-1.2.0-linux.dmg", "1.2.0"), None);
    }

    #[test]
    fn release_names_are_safe_in_urls() {
        let name = archive_name("My App", "1.2.0", "zip");
        assert!(name.starts_with("My-App-1.2.0-"), "{name}");
        assert_eq!(
            archive_platform(&name, "1.2.0").as_deref(),
            Some(nsh_update::platform().as_str())
        );
    }
}
