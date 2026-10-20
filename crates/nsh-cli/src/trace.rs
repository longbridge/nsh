//! `nsh trace`: runs an app headless and prints its mount batches
//! (`nsh_engine::headless`, which does the run). An app this command line
//! serves runs in its process; an app with Rust crates of its own runs in
//! its headless runner (`rust_app.rs`), with the bytecode compiled here, or
//! for a Rust app's root none.

use std::{
    io::{BufRead as _, BufReader},
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::{Context as _, bail};
use nsh_bundler::BundleOptions;
use nsh_engine::{
    Bundle,
    headless::{DirectoryAssets, RunnerMessage, TraceOptions, TraceRequest},
};

use crate::{build::BuildError, logger, ui};

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
        };
        if root {
            ui::done(format!("mounting the Rust app {}", ui::path(entry)));
        } else {
            let (bytecode, map) = compile(entry)?;
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
    let (bytecode, map) = compile(entry)?;
    // SAFETY: compiled just now by this runtime.
    let bundle = unsafe { Bundle::bytecode(bytecode) };
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
        Box::new(std::io::stdout()),
        &mut warn,
    )
    .map_err(anyhow::Error::msg)
}

/// The app at `entry`, bundled and compiled to bytecode, with its source
/// map.
pub(crate) fn compile(entry: &Path) -> anyhow::Result<(Vec<u8>, Option<String>)> {
    compile_with(entry, &crate::ui_sets(), true)
}

/// The entry `entry` bundled against `sets` and compiled to bytecode, with
/// its source map; `announce`: saying so.
pub(crate) fn compile_with(
    entry: &Path,
    sets: &[nsh_core::ui::description::SetDescription],
    announce: bool,
) -> anyhow::Result<(Vec<u8>, Option<String>)> {
    let bundle = nsh_bundler::bundle(&BundleOptions {
        entry: entry.to_path_buf(),
        minify: false,
        split: false,
        ui_sets: sets.to_vec(),
    })
    .map_err(|e| BuildError::from(e).into_error(entry))?;
    if announce {
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
    }
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, sets)?;
    Ok((bytecode, bundle.source_map))
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
