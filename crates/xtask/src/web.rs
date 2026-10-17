//! The web runtime and the website's demos: `nsh.wasm` (`nsh-platform-web`)
//! plus its JS runtime and player page, and each demo of
//! `website/demos/*.tsx` bundled by `nsh build`.
//!
//! ```text
//! <out>/index.html    the player: runs demos/<name>.js for `#<name>`
//! <out>/nsh.js        the DOM runtime (crates/nsh-platform-web/web)
//! <out>/nsh.wasm      the engine, the core and QuickJS
//! <out>/nsh-playground.wasm   the same plus the TSX compiler (the playground)
//! <out>/demos/<name>.js
//! ```

use std::{fs, path::Path};

use anyhow::Context as _;

use crate::{
    android,
    dist::{cargo, copy},
    term::{self, run},
};

const WASM: &str = "wasm32-unknown-unknown";

pub(crate) fn build(out: &Path) -> anyhow::Result<()> {
    let root = android::root();
    // Relative to the workspace, wherever `cargo xtask` runs from.
    let out = root.join(out);
    cargo(
        &[
            "build",
            "--profile",
            "web",
            "-p",
            "nsh-platform-web",
            "--target",
            WASM,
        ],
        "nsh.wasm",
    )?;
    // Its own target directory: switching the feature would rebuild the
    // other one every time.
    cargo(
        &[
            "build",
            "--profile",
            "web",
            "-p",
            "nsh-platform-web",
            "--features",
            "compiler",
            "--target",
            WASM,
            "--target-dir",
            "target/web-playground",
        ],
        "nsh-playground.wasm",
    )?;
    cargo(&["build", "--release", "-p", "nsh-cli"], "nsh")?;
    fs::create_dir_all(out.join("demos"))?;
    copy(
        &root
            .join("target")
            .join(WASM)
            .join("web/nsh_platform_web.wasm"),
        &out.join("nsh.wasm"),
    )?;
    copy(
        &root
            .join("target/web-playground")
            .join(WASM)
            .join("web/nsh_platform_web.wasm"),
        &out.join("nsh-playground.wasm"),
    )?;
    let web = root.join("crates/nsh-platform-web/web");
    for file in ["nsh.js", "index.html"] {
        copy(&web.join(file), &out.join(file))?;
    }

    let nsh = root.join(format!(
        "target/release/nsh{}",
        std::env::consts::EXE_SUFFIX
    ));
    let demos = root.join("website/demos");
    let tmp = root.join("target/web-demos");
    let step = term::Step::start("bundling the website's demos");
    let mut demos_built = 0;
    for entry in fs::read_dir(&demos).context("website/demos")? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "tsx") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("demo name")?
            .to_owned();
        let dir = tmp.join(&name);
        run(std::process::Command::new(&nsh)
            .args(["build", "--release", "-o"])
            .arg(&dir)
            .arg(&path))
        .with_context(|| format!("bundling {name}"))?;
        copy(
            &dir.join("bundle.js"),
            &out.join("demos").join(format!("{name}.js")),
        )?;
        demos_built += 1;
    }
    if demos.join("assets").is_dir() {
        for entry in fs::read_dir(demos.join("assets"))? {
            let path = entry?.path();
            if path.is_file() {
                copy(
                    &path,
                    &out.join("assets").join(path.file_name().context("name")?),
                )?;
            }
        }
    }
    step.done(format!("bundled {demos_built} demos"));
    term::note(out.strip_prefix(&root).unwrap_or(&out).display());
    Ok(())
}
