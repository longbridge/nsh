//! `cargo xtask api`: records the API this checkout offers apps in
//! `api/current/` (the engine's `compat` tests compare it with what the
//! runtime makes). `--freeze [<version>]` also makes the baseline of a
//! release (the workspace's version by default) in `api/v<version>/`: the
//! same surface and the apps of [`APPS`] as this checkout's `nsh build
//! --release` writes them, each with the script of the previous baseline
//! (docs/dev/compatibility.md).

use std::{path::Path, process::Command};

use anyhow::{Context as _, bail};

use crate::{android::root, term};

/// The example apps a baseline keeps: together they use most of the API.
const APPS: &[&str] = &["counter", "desktop", "gallery", "hopper"];

pub(crate) fn api(freeze: Option<Option<String>>) -> anyhow::Result<()> {
    let root = root();
    let freeze = freeze.map(|v| v.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()));
    let mut test = Command::new("cargo");
    test.current_dir(&root)
        .args(["test", "-p", "nsh-engine", "--test", "engine", "--"])
        .args(["--exact", "compat::current_surface_is_recorded"])
        .env("NSH_API_UPDATE", "1");
    let baseline = freeze
        .as_ref()
        .map(|v| root.join("api").join(format!("v{v}")));
    if let Some(dir) = &baseline {
        test.env("NSH_API_FREEZE", dir);
    }
    term::cargo(&mut test, "and recording the API")?;
    term::done("recorded api/current");
    let Some(dir) = baseline else {
        return Ok(());
    };
    term::cargo(
        Command::new("cargo")
            .current_dir(&root)
            .args(["build", "-p", "nsh-cli"]),
        "nsh",
    )?;
    let previous = previous_baseline(&root.join("api"), &dir)?;
    for app in APPS {
        let out = root.join("target/api").join(app);
        let _ = std::fs::remove_dir_all(&out);
        term::run(
            Command::new(root.join("target/debug/nsh"))
                .current_dir(&root)
                .args(["build", "--release", "-o"])
                .arg(&out)
                .arg(Path::new("examples").join(app)),
        )?;
        let kept = dir.join("apps").join(app);
        std::fs::create_dir_all(&kept)?;
        std::fs::copy(out.join("bundle.js"), kept.join("bundle.js"))
            .with_context(|| format!("{app}'s bundle"))?;
        let _ = std::fs::remove_dir_all(kept.join("assets"));
        if out.join("assets").exists() {
            copy_dir(&out.join("assets"), &kept.join("assets"))?;
        }
        let script = kept.join("script.txt");
        match previous
            .as_ref()
            .map(|p| p.join("apps").join(app).join("script.txt"))
        {
            _ if script.exists() => {}
            Some(from) if from.exists() => {
                std::fs::copy(from, &script)?;
            }
            _ => {
                std::fs::write(&script, "# What a user does and sees (compat/apps.rs).\n")?;
                term::warn(format!(
                    "{}: write what a user does and sees",
                    script.strip_prefix(&root).unwrap_or(&script).display()
                ));
            }
        }
    }
    term::done(format!(
        "froze {}",
        dir.strip_prefix(&root).unwrap_or(&dir).display()
    ));
    Ok(())
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

/// The newest baseline before `dir`.
fn previous_baseline(api: &Path, dir: &Path) -> anyhow::Result<Option<std::path::PathBuf>> {
    let version = |name: &str| -> Option<Vec<u32>> {
        name.strip_prefix('v')?
            .split('.')
            .map(|p| p.parse().ok())
            .collect()
    };
    let Some(this) = dir.file_name().and_then(|n| version(&n.to_string_lossy())) else {
        bail!("{}: not a version", dir.display());
    };
    let mut older: Vec<_> = std::fs::read_dir(api)?
        .filter_map(|e| {
            let path = e.ok()?.path();
            let v = version(&path.file_name()?.to_string_lossy())?;
            (v < this).then_some((v, path))
        })
        .collect();
    older.sort();
    Ok(older.pop().map(|(_, p)| p))
}
