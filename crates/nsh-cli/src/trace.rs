//! `nsh trace`: runs an app headless and prints its mount batches
//! (`nsh_engine::headless`, which does the run). An app this command line
//! serves runs in its process; an app with Rust crates of its own runs in
//! its headless runner (`rust_app.rs`), with the bytecode compiled here, or
//! for a Rust app's root none.

use std::{
    io::{BufRead as _, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::{Context as _, bail};
use nsh_bundler::BundleOptions;
use nsh_engine::{
    Bundle,
    headless::{DirectoryAssets, Release, RunnerMessage, TraceOptions, TraceRequest},
};

use crate::{build::BuildError, logger, ui};

/// The simulated operating system of a headless trace (`--os`).
#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum TraceOs {
    Ios,
    Android,
    Macos,
    Windows,
    Linux,
    Web,
}

impl TraceOs {
    pub(crate) fn platform(self) -> nsh_core::platform::Os {
        use nsh_core::platform::Os;
        match self {
            Self::Ios => Os::Ios,
            Self::Android => Os::Android,
            Self::Macos => Os::MacOs,
            Self::Windows => Os::Windows,
            Self::Linux => Os::Linux,
            Self::Web => Os::Web,
        }
    }
}

/// `--data-dir`, made if missing and canonical: the trace's isolated app
/// data root, never the user's default directory.
pub(crate) fn data_dir(path: PathBuf) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(&path).with_context(|| format!("create {}", path.display()))?;
    Ok(nsh_core::access::canonicalize(&path)?)
}

/// Runs the app headless, taps views and prints every mount batch.
pub(crate) fn trace(entry: &Path, options: TraceOptions) -> anyhow::Result<()> {
    nsh_engine::headless::check_taps(&options.tap).map_err(anyhow::Error::msg)?;
    let dir = entry.parent().unwrap_or(Path::new(".")).to_path_buf();
    if let Some(app) = crate::rust_app::current() {
        let root = crate::rust_app::is_root(entry);
        let runner = app.trace_runner(root)?;
        let mut request = TraceRequest {
            bytecode: None,
            bytecode_version: None,
            source_map: None,
            dir,
            verbose: ui::verbose(),
            options,
            release: Release::default(),
        };
        if root {
            ui::done(format!("mounting the Rust app {}", ui::path(entry)));
        } else {
            let (bytecode, map, release) = compile(entry)?;
            request.release = release;
            let out = app.work_dir("trace");
            std::fs::create_dir_all(&out)?;
            let file = out.join("bundle.nshc");
            std::fs::write(&file, bytecode).with_context(|| format!("write {}", file.display()))?;
            request.bytecode = Some(file);
            request.bytecode_version = Some(nsh_js::BYTECODE_VERSION.to_owned());
            if let Some(map) = map {
                let file = out.join("bundle.js.map");
                std::fs::write(&file, map).with_context(|| format!("write {}", file.display()))?;
                request.source_map = Some(file);
            }
        }
        return run_runner(&runner, &request);
    }
    let (bytecode, map, release) = compile(entry)?;
    // SAFETY: compiled just now by this runtime.
    let bundle = release.apply(unsafe { Bundle::bytecode(bytecode) });
    let app = nsh_engine::App::Js(match map {
        Some(map) => bundle.with_source_map(map),
        None => bundle,
    });
    let files = crate::build::crate_assets()
        .map(|a| (a.path, a.source))
        .collect();
    let mut warn = |message: &str| ui::warn(message);
    nsh_engine::headless::trace(
        app,
        crate::own_sets().to_vec(),
        Arc::new(DirectoryAssets::new(dir, files)),
        &options,
        &mut std::io::stdout().lock(),
        &mut warn,
    )
    .map_err(anyhow::Error::msg)
}

/// The app at `entry`, bundled and compiled to bytecode, with its source
/// map and how it runs as its release build does (hardened: the
/// capabilities it declares apply).
fn compile(entry: &Path) -> anyhow::Result<(Vec<u8>, Option<String>, Release)> {
    let sets = crate::ui_sets();
    let bundle = nsh_bundler::bundle(&BundleOptions {
        entry: entry.to_path_buf(),
        minify: false,
        split: false,
        ui_sets: sets.to_vec(),
    })
    .map_err(|e| BuildError::from(e).into_error(entry))?;
    // Exact numbers: traces are compared.
    ui::done(format!(
        "bundled {} {}",
        ui::path(entry),
        ui::dim(format!(
            "({} B in {:.1} ms)",
            bundle.code.len(),
            bundle.elapsed_ms
        ))
    ));
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &sets)?;
    let release = Release {
        capabilities: bundle.capabilities.clone(),
        app_id: crate::package::app_id(entry).ok(),
        plugin_dir: nsh_core::access::canonicalize(entry)?
            .parent()
            .map(Path::to_path_buf),
    };
    Ok((bytecode, bundle.source_map, release))
}

/// Runs the app's headless `runner` with `request`: its batches go to this
/// command's output as they come, its messages are shown as this
/// command's own ([`RunnerMessage`]), other lines (a panic's) as they are.
fn run_runner(runner: &Path, request: &TraceRequest) -> anyhow::Result<()> {
    let mut child = Command::new(runner)
        .arg(request.to_json())
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("run {}", runner.display()))?;
    let stderr = child.stderr.take().context("the runner's standard error")?;
    let mut failed = None;
    for line in BufReader::new(stderr).lines() {
        let line = line?;
        match RunnerMessage::parse(&line) {
            Some(RunnerMessage::Log {
                level,
                target,
                message,
            }) => logger::forward(level, &target, &message),
            Some(RunnerMessage::Warn(message)) => ui::warn(message),
            Some(RunnerMessage::Error(message)) => failed = Some(message),
            None => ui::plain(line),
        }
    }
    let status = child.wait()?;
    if let Some(message) = failed {
        bail!(message);
    }
    if !status.success() {
        bail!(
            "the headless runner {} exited with {status}",
            ui::path(runner)
        );
    }
    Ok(())
}
