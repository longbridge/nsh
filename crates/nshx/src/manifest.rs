//! The project side: finding `nsh.json`, reading its toolchain pin, and
//! checking the `nsh-core` revision an app's Rust crate resolves to.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};
use serde::Deserialize;

/// The parts of `nsh.json` nshx reads.
#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    toolchain: Option<Toolchain>,
    #[serde(default)]
    rust: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Toolchain {
    #[serde(default)]
    nsh: Option<NshPin>,
}

#[derive(Debug, Deserialize)]
struct NshPin {
    version: String,
    #[serde(default)]
    repository: Option<String>,
}

/// The exact nsh release a project pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pin {
    /// The version as `nsh.json` writes it, e.g. `0.6.1` or `v0.6.1`.
    pub(crate) version: String,
    /// The release tag the version names, e.g. `v0.6.1`.
    pub(crate) tag: String,
    /// `owner/repo` whose GitHub releases hold the distribution.
    pub(crate) repository: String,
}

/// The app directory holding `nsh.json`: this directory or an ancestor, else
/// a directory or entry named in `args` (the command's app path).
pub(crate) fn find_app(args: &[OsString]) -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir().context("current directory")?;
    if let Some(app) = cwd.ancestors().find(|dir| dir.join("nsh.json").is_file()) {
        return Ok(app.to_owned());
    }
    for arg in args {
        let Some(arg) = arg.to_str() else {
            continue;
        };
        if arg.starts_with('-') {
            continue;
        }
        let path = Path::new(arg);
        if path.is_dir() && path.join("nsh.json").is_file() {
            return Ok(path.to_owned());
        }
        if path.is_file()
            && let Some(parent) = path.parent()
            && parent.join("nsh.json").is_file()
        {
            return Ok(parent.to_owned());
        }
    }
    bail!("no nsh.json found here or in the arguments; run inside an app or pass its directory")
}

/// `nsh.json` in `app`.
pub(crate) fn read_config(app: &Path) -> anyhow::Result<Config> {
    let path = app.join("nsh.json");
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))
}

/// The toolchain pin, or `None` for an app that does not pin one.
pub(crate) fn pin(config: &Config) -> anyhow::Result<Option<Pin>> {
    let Some(toolchain) = &config.toolchain else {
        return Ok(None);
    };
    let Some(nsh) = &toolchain.nsh else {
        return Ok(None);
    };
    let version = nsh.version.trim();
    if version.is_empty() {
        bail!("\"toolchain.nsh.version\" is empty");
    }
    if version.chars().any(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '/' | '\\' | '*' | '^' | '~' | '>' | '<' | '=' | ',' | '|' | ':' | '?' | '#' | '%'
            )
    }) || version.contains("..")
    {
        bail!("\"toolchain.nsh.version\" must be one exact version, not {version:?}");
    }
    let lowered = version.to_ascii_lowercase();
    if lowered == "latest" || lowered == "nightly" {
        bail!("\"toolchain.nsh.version\" must name a release, not {version:?}");
    }
    let tag = if version
        .strip_prefix('v')
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
    {
        version.to_owned()
    } else {
        format!("v{version}")
    };
    let repository = nsh
        .repository
        .as_deref()
        .map(str::trim)
        .filter(|repo| !repo.is_empty())
        .unwrap_or(super::release::DEFAULT_REPOSITORY)
        .to_owned();
    validate_repository(&repository)?;
    Ok(Some(Pin {
        version: version.to_owned(),
        tag,
        repository,
    }))
}

fn validate_repository(repository: &str) -> anyhow::Result<()> {
    let Some((owner, repo)) = repository.split_once('/') else {
        bail!(
            "\"toolchain.nsh.repository\" must be owner/repo, e.g. longbridge/nsh, not {repository:?}"
        );
    };
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        bail!("\"toolchain.nsh.repository\" must be owner/repo, not {repository:?}");
    }
    if repository
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, ':' | '@' | '\\' | '?' | '#' | '~'))
        || repository.contains("..")
    {
        bail!("\"toolchain.nsh.repository\" must be owner/repo, not {repository:?}");
    }
    Ok(())
}

/// The `nsh-core` revision the app's Rust crates pin, if any: every
/// `Cargo.lock` of its `rust` crates that takes nsh from Git must name one
/// revision.
pub(crate) fn expected_rev(app: &Path, config: &Config) -> anyhow::Result<Option<String>> {
    let Some(rust) = &config.rust else {
        return Ok(None);
    };
    let dirs: Vec<&str> = match rust {
        serde_json::Value::String(dir) => vec![dir.as_str()],
        serde_json::Value::Array(items) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .context("\"rust\" must be a string or an array of strings")
            })
            .collect::<anyhow::Result<_>>()?,
        _ => bail!("\"rust\" must be a string or an array of strings"),
    };
    let mut revisions: Vec<String> = Vec::new();
    for dir in dirs {
        let crate_dir = app.join(dir);
        let Some(lock_dir) = crate_dir
            .ancestors()
            .find(|dir| dir.join("Cargo.lock").is_file())
        else {
            eprintln!(
                "nshx: no Cargo.lock under {}; its nsh-core revision is not checked",
                crate_dir.display()
            );
            continue;
        };
        let lock = lock_dir.join("Cargo.lock");
        let text = fs::read_to_string(&lock).with_context(|| format!("read {}", lock.display()))?;
        let value: toml::Value =
            toml::from_str(&text).with_context(|| format!("{} is not TOML", lock.display()))?;
        let packages = value
            .get("package")
            .and_then(toml::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for package in packages {
            if package.get("name").and_then(toml::Value::as_str) != Some("nsh-core") {
                continue;
            }
            let Some(source) = package.get("source").and_then(toml::Value::as_str) else {
                continue;
            };
            if !source.starts_with("git+") {
                continue;
            }
            let Some((_, revision)) = source.rsplit_once('#') else {
                continue;
            };
            if revision.is_empty() {
                continue;
            }
            revisions.push(revision.to_owned());
        }
    }
    revisions.sort();
    revisions.dedup();
    match revisions.as_slice() {
        [] => Ok(None),
        [revision] => Ok(Some(revision.clone())),
        many => bail!(
            "the app's crates pin nsh-core at different revisions: {}",
            many.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(json: &str) -> Config {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_pin_is_one_exact_release() {
        let pinned = pin(&config(
            r#"{"toolchain":{"nsh":{"version":"0.6.1","repository":"acme/nsh"}}}"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(pinned.version, "0.6.1");
        assert_eq!(pinned.tag, "v0.6.1");
        assert_eq!(pinned.repository, "acme/nsh");
        let default = pin(&config(r#"{"toolchain":{"nsh":{"version":"v0.6.1"}}}"#))
            .unwrap()
            .unwrap();
        assert_eq!(default.tag, "v0.6.1");
        assert_eq!(
            default.repository,
            super::super::release::DEFAULT_REPOSITORY
        );
    }

    #[test]
    fn ranges_and_channels_are_refused() {
        for version in [">=0.6", "0.6.*", "latest", "nightly", "0.6.1 latest"] {
            let json = format!(r#"{{"toolchain":{{"nsh":{{"version":{version:?}}}}}}}"#);
            assert!(pin(&config(&json)).is_err(), "{version}");
        }
        assert!(
            pin(&config(
                r#"{"toolchain":{"nsh":{"version":"0.6.1","repository":"../x"}}}"#
            ))
            .is_err()
        );
        assert!(
            pin(&config(
                r#"{"toolchain":{"nsh":{"version":"0.6.1","repository":"a/b/c"}}}"#
            ))
            .is_err()
        );
    }

    #[test]
    fn no_toolchain_means_no_pin() {
        assert!(pin(&config("{}")).unwrap().is_none());
        assert!(pin(&config(r#"{"toolchain":{}}"#)).unwrap().is_none());
    }

    #[test]
    fn the_rust_crates_lock_names_the_core_revision() {
        let app = std::env::temp_dir().join(format!(
            "nshx-manifest-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&app);
        fs::create_dir_all(app.join("native")).unwrap();
        fs::write(
            app.join("native/Cargo.lock"),
            r#"
version = 4
[[package]]
name = "nsh-core"
version = "0.6.0"
source = "git+https://github.com/longbridge/nsh?rev=abcdef#abcdef0123456789abcdef0123456789abcdef01"
[[package]]
name = "serde"
version = "1.0.0"
"#,
        )
        .unwrap();
        let config = config(r#"{"rust":"native"}"#);
        assert_eq!(
            expected_rev(&app, &config).unwrap().as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef01")
        );
        fs::write(
            app.join("native/Cargo.lock"),
            "version = 4\n[[package]]\nname = \"nsh-core\"\nversion = \"0.6.0\"\n",
        )
        .unwrap();
        assert_eq!(expected_rev(&app, &config).unwrap(), None);
        let _ = fs::remove_dir_all(&app);
    }
}
