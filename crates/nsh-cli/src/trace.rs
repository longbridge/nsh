//! `nsh trace`: runs an app headless and prints its mount batches
//! (`nsh_engine::headless`, which does the run).

use std::{path::Path, sync::Arc};

use nsh_bundler::BundleOptions;
use nsh_engine::{
    Bundle,
    headless::{DirectoryAssets, TraceOptions},
};

use crate::{build::BuildError, ui};

/// Runs the app headless, taps views and prints every mount batch.
pub(crate) fn trace(entry: &Path, options: TraceOptions) -> anyhow::Result<()> {
    nsh_engine::headless::check_taps(&options.tap).map_err(anyhow::Error::msg)?;
    let app = if crate::rust_app::is_root(entry) {
        // A Rust app: its command line was built with the root.
        let root = crate::root()
            .ok_or_else(|| anyhow::anyhow!("this nsh was not built for a Rust app's root"))?;
        ui::done(format!("mounting the Rust app {}", ui::path(entry)));
        nsh_engine::App::Rust(root)
    } else {
        nsh_engine::App::Js(bundle(entry)?)
    };
    let dir = entry.parent().unwrap_or(Path::new(".")).to_path_buf();
    let files = crate::build::crate_assets()
        .map(|a| (a.path, a.source))
        .collect();
    let mut warn = |message: &str| ui::warn(message);
    nsh_engine::headless::trace(
        app,
        crate::ui_sets().to_vec(),
        Arc::new(DirectoryAssets::new(dir, files)),
        &options,
        &mut std::io::stdout().lock(),
        &mut warn,
    )
    .map_err(anyhow::Error::msg)
}

/// The app at `entry`, bundled and compiled for a trace.
fn bundle(entry: &Path) -> anyhow::Result<Bundle> {
    let bundle = nsh_bundler::bundle(&BundleOptions {
        entry: entry.to_path_buf(),
        minify: false,
        split: false,
        ui_sets: crate::described_sets().to_vec(),
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
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, crate::described_sets())?;
    // SAFETY: compiled just now by this runtime.
    let bytecode = unsafe { Bundle::bytecode(bytecode) };
    Ok(match bundle.source_map {
        Some(map) => bytecode.with_source_map(map),
        None => bytecode,
    })
}
