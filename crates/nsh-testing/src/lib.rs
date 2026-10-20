//! Headless tests of an app's TSX entry: the entry built as `nsh run`
//! builds it, against the UI sets its hosts serve (nsh's own and each of
//! the app's crates' `registry()`, which `#[native_module]` makes), and run
//! on an engine without a window ([`nsh_engine::headless::Headless`]).
//!
//! ```ignore
//! let sets = vec![nsh_ui_mobile::registry(), my_app_native::registry()];
//! let mut app = nsh_testing::tsx("../App.tsx", sets).unwrap();
//! app.wait_for("Ready");
//! app.press("Save");
//! app.wait_for("Saved");
//! ```
//!
//! A Rust app's root needs no build: its tests run it on
//! `nsh_engine::headless` directly (nsh-engine's feature `headless`).

use std::path::Path;

use nsh_core::ui::{Registry, description::SetDescription};
pub use nsh_engine::headless::{Headless, Screen, WAIT};
use nsh_engine::{Bundle, EngineConfig};

/// The TSX (or TS, JS) entry `entry` bundled with nsh's bundler and
/// compiled to QuickJS bytecode against `ui_sets`, as `nsh run` builds it:
/// the app to run (`EngineConfig::new(bundle)`), or what failed. Build it
/// once for several tests and clone it.
pub fn compile_tsx(entry: impl AsRef<Path>, ui_sets: &[Registry]) -> Result<Bundle, String> {
    let entry = entry.as_ref();
    let sets: Vec<SetDescription> = ui_sets.iter().map(SetDescription::from).collect();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry: entry.to_path_buf(),
        minify: false,
        split: false,
        ui_sets: sets.clone(),
    })
    .map_err(|e| format!("{}: {e}", entry.display()))?;
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &sets)
        .map_err(|e| format!("{}: {e}", entry.display()))?;
    // SAFETY: compiled by this runtime just now, from the app's sources.
    Ok(unsafe { Bundle::bytecode(bytecode) })
}

/// Starts the TSX entry `entry` built against `ui_sets` ([`compile_tsx`])
/// on a phone-sized headless engine ([`EngineConfig::new`]) serving them.
pub fn tsx(entry: impl AsRef<Path>, ui_sets: Vec<Registry>) -> Result<Headless, String> {
    let app = compile_tsx(entry, &ui_sets)?;
    Headless::start(EngineConfig {
        ui_sets,
        ..EngineConfig::new(app)
    })
    .map_err(|e| format!("the engine did not start: {e}"))
}
