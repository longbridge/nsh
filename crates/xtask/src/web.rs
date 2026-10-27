//! The web runtime and the website's demos: `nsh.wasm` (`nsh-platform-web`)
//! plus its JS runtime and player page, each demo of `website/demos/*.tsx`
//! compiled by `nsh build`, and the gallery (`examples/gallery`) published
//! with `nsh package --target web`, the website's showcase.
//!
//! ```text
//! <out>/index.html    the player: runs demos/<name>.nshc for `#<name>`
//! <out>/nsh-playground.wasm   the engine plus the TSX compiler (the playground)
//! <out>/demos/<name>.nshc     a demo compiled to bytecode
//! <out>/gallery/      the gallery as `nsh package --target web` writes it,
//!                     with the runtime the player loads too:
//! <out>/gallery/nsh.<digest>.js     the DOM runtime (minified)
//! <out>/gallery/nsh.<digest>.wasm   the engine, the core and QuickJS
//! ```
//!
//! The player and the gallery share one runtime, so the site carries one
//! `nsh.wasm`, the runtime of published apps (no QuickJS parser: it runs
//! bytecode only): the player's page is written with the gallery's file
//! names. Only the playground, which compiles in the page, has a parser.
//! (`astro dev` serves the player from its sources instead,
//! `website/src/lib/play-dev.js`.)

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
    // `nsh.wasm` is built by `nsh package` (the gallery's, below).
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
            .join("target/web-playground")
            .join(WASM)
            .join("web/nsh_platform_web.wasm"),
        &out.join("nsh-playground.wasm"),
    )?;
    let web = root.join("crates/nsh-platform-web/web");
    // From before the player shared the gallery's runtime.
    for file in ["nsh.js", "nsh.wasm"] {
        let _ = fs::remove_file(out.join(file));
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
            &dir.join("bundle.nshc"),
            &out.join("demos").join(format!("{name}.nshc")),
        )?;
        let _ = fs::remove_file(out.join("demos").join(format!("{name}.js")));
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

    // The gallery, as apps are published: its own page and content-addressed
    // files, all relative, so it works under any base path of the site.
    let step = term::Step::start("packaging the gallery for the web");
    let packaged = tmp.join("gallery");
    if packaged.exists() {
        fs::remove_dir_all(&packaged)?;
    }
    run(std::process::Command::new(&nsh)
        .args(["package", "--target", "web", "-o"])
        .arg(&packaged)
        .arg(root.join("examples/gallery")))
    .context("packaging the gallery")?;
    let gallery = out.join("gallery");
    if gallery.exists() {
        fs::remove_dir_all(&gallery)?;
    }
    // `<packaged>/<the app's name>/`, the one directory there.
    let site = fs::read_dir(&packaged)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .context("nsh package wrote no site")?;
    fs::rename(&site, &gallery).or_else(|_| copy_dir(&site, &gallery))?;
    step.done("packaged the gallery");

    // The player loads the gallery's runtime and its syntaxes.
    let runtime = |stem: &str, ext: &str| -> anyhow::Result<String> {
        fs::read_dir(&gallery)?
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .find(|name| name.starts_with(&format!("{stem}.")) && name.ends_with(ext))
            .with_context(|| format!("the gallery has no {stem}.<digest>{ext}"))
    };
    let mut player = fs::read_to_string(web.join("index.html"))?;
    for (file, stem, ext) in [
        ("./nsh.js", "nsh", ".js"),
        ("./nsh.wasm", "nsh", ".wasm"),
        ("./nsh-syntaxes.bin", "nsh-syntaxes", ".bin"),
    ] {
        let quoted = format!("\"{file}\"");
        anyhow::ensure!(player.contains(&quoted), "the player does not load {file}");
        player = player.replace(&quoted, &format!("\"./gallery/{}\"", runtime(stem, ext)?));
    }
    fs::write(out.join("index.html"), player)?;
    term::note(out.strip_prefix(&root).unwrap_or(&out).display());
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            copy(&entry.path(), &dest)?;
        }
    }
    Ok(())
}
