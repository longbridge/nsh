//! `cargo xtask api`: records the API this checkout offers apps in
//! `api/runtime.txt` and `api/types/` (the engine's `compat` tests compare
//! them with what the runtime makes). The record is append-only, as the web
//! platform's is: what a merged change recorded stays, and nothing is frozen
//! per release.
//!
//! It also removes from `api/uncovered.txt` what the fixture apps exercise
//! now (the list only shrinks).
//!
//! `--keep <app>...` adds fixtures, `api/apps/<app>-<version>/`: apps as
//! this checkout's `nsh build --release` writes them, for when the JSX
//! compiler's output changes (new `nsh:rt` calls), so the record keeps what
//! the new compiler writes next to what the older ones wrote, and for a new
//! API, which comes with a fixture that uses it. An app is an example's
//! name (`counter`) or the path of an app's directory or entry module, as
//! the small cases of `api/cases/<case>.tsx` (docs/dev/compatibility.md).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};

use crate::{android::root, term};

pub(crate) fn api(keep: &[String]) -> anyhow::Result<()> {
    let root = root();
    let mut record = Command::new("cargo");
    record
        .current_dir(&root)
        .args(["test", "-p", "nsh-engine", "--test", "engine", "--"])
        .args(["--exact", "compat::surface_is_recorded"])
        .env("NSH_API_UPDATE", "1");
    // A new API's fixture must exist before its coverage can be checked.
    // `--keep` creates it with a script to complete; the next plain `api`
    // run verifies that script and removes the newly covered entries.
    if keep.is_empty() {
        record.arg("compat::coverage::every_api_is_tested");
    }
    term::cargo(&mut record, "and recording the API")?;
    term::done(if keep.is_empty() {
        "recorded api/runtime.txt, api/types and api/uncovered.txt"
    } else {
        "recorded api/runtime.txt and api/types"
    });
    if keep.is_empty() {
        return Ok(());
    }
    let apps = root.join("api/apps");
    let version = env!("CARGO_PKG_VERSION");
    // Check every app first, so a refused one leaves no fixture half added.
    let kept: Vec<_> = keep
        .iter()
        .map(|app| {
            let (name, source) = source(&root, app)?;
            let dir = apps.join(format!("{name}-{version}"));
            if dir.exists() {
                // Fixtures are what apps built then call: never rewritten.
                bail!("{}: already kept", shown(&root, &dir));
            }
            Ok((name, source, dir))
        })
        .collect::<anyhow::Result<_>>()?;
    term::cargo(
        Command::new("cargo")
            .current_dir(&root)
            .args(["build", "-p", "nsh-cli"]),
        "nsh",
    )?;
    for (app, source, dir) in kept {
        let app = app.as_str();
        let out = root.join("target/api").join(app);
        let _ = std::fs::remove_dir_all(&out);
        term::run(
            Command::new(root.join("target/debug/nsh"))
                .current_dir(&root)
                .args(["build", "--release", "-o"])
                .arg(&out)
                .arg(&source),
        )?;
        // The script comes before the new fixture exists, so it is the
        // newest older one's.
        let previous = newest_fixture(&apps, app)?;
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(out.join("bundle.js"), dir.join("bundle.js"))
            .with_context(|| format!("{app}'s bundle"))?;
        if out.join("assets").exists() {
            copy_dir(&out.join("assets"), &dir.join("assets"))?;
        }
        let script = dir.join("script.txt");
        match previous.map(|p| p.join("script.txt")) {
            Some(from) if from.exists() => {
                std::fs::copy(from, &script)?;
            }
            _ => {
                std::fs::write(&script, "# What a user does and sees (compat/apps.rs).\n")?;
                term::warn(format!(
                    "{}: write what a user does and sees",
                    shown(&root, &script)
                ));
            }
        }
        term::done(format!("kept {}", shown(&root, &dir)));
    }
    term::done(
        "run `cargo test -p nsh-engine --test engine compat::apps_still_run` to check the fixtures",
    );
    Ok(())
}

/// The fixture's name and the app `keep` names: an example of `examples/`
/// by its name, or the path of an app's directory or entry module (from
/// the current directory or the repository's root), named after the
/// directory or the module's file stem.
fn source(root: &Path, keep: &str) -> anyhow::Result<(String, PathBuf)> {
    let example = root.join("examples").join(keep);
    if !keep.contains(['/', '\\']) && example.is_dir() {
        return Ok((keep.to_owned(), example));
    }
    let path = [Path::new(keep).to_path_buf(), root.join(keep)]
        .into_iter()
        .find(|p| p.exists())
        .with_context(|| format!("{keep}: no such example app, directory or module"))?;
    let path = path.canonicalize()?;
    let name = if path.is_dir() {
        path.file_name()
    } else {
        path.file_stem()
    };
    let name = name
        .and_then(|n| n.to_str())
        .filter(|n| {
            !n.is_empty()
                && n.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        })
        .with_context(|| {
            format!(
                "{}: a fixture's name is lower case letters, digits, `-` and `_`",
                path.display()
            )
        })?;
    Ok((name.to_owned(), path))
}

fn shown(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn copy_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let path = entry?.path();
        let dest = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

/// The fixture of `app` with the latest version: `<app>-<version>`, where a
/// bare `<app>` (the first fixtures, kept before they had versions) is the
/// oldest.
fn newest_fixture(apps: &Path, app: &str) -> anyhow::Result<Option<PathBuf>> {
    if !apps.exists() {
        return Ok(None);
    }
    let version = |name: &str| -> Option<Vec<u32>> {
        if name == app {
            return Some(Vec::new());
        }
        // `gallery-rs` is not a version of `gallery`: only numbers count.
        name.strip_prefix(app)?
            .strip_prefix('-')?
            .split('.')
            .map(|p| p.parse().ok())
            .collect()
    };
    let mut fixtures: Vec<_> = std::fs::read_dir(apps)?
        .filter_map(|e| {
            let path = e.ok()?.path();
            let v = version(&path.file_name()?.to_string_lossy())?;
            path.is_dir().then_some((v, path))
        })
        .collect();
    fixtures.sort();
    Ok(fixtures.pop().map(|(_, p)| p))
}
