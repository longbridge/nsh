//! Headless tests of an app's TSX entry: the entry built as `nsh run`
//! builds it, against the UI sets its hosts serve (nsh's own and each of
//! the app's crates' `registry()`, which `#[native_module]` makes), and run
//! on an engine without a window ([`nsh_engine::headless::Headless`]).
//!
//! ```ignore
//! let sets = vec![nsh_ui_mobile::registry(), my_app_native::registry()];
//! let mut app = nsh_testing::tsx("../App.tsx", sets).unwrap();
//! app.wait_for("Ready")?;
//! app.press("Save")?;
//! app.wait_for("Saved")?;
//! ```
//!
//! A Rust app's root needs no build: its tests run it on
//! `nsh_engine::headless` directly (nsh-engine's feature `headless`).
//!
//! It also runs the test files of `nsh test` ([`run_file`]): tests written
//! in TypeScript with the `nsh/test` module ([`registry`]), each on an
//! engine of its own, the apps they open on headless engines.

mod module;
mod run;
mod runner;

#[cfg(feature = "bundle")]
use std::{path::Path, sync::Arc};

use nsh_core::ui::Registry;
#[cfg(feature = "bundle")]
use nsh_core::ui::description::SetDescription;
pub use nsh_engine::headless::{Headless, Screen, WAIT};
#[cfg(feature = "bundle")]
use nsh_engine::{Bundle, EngineConfig};

pub use self::{
    module::{
        App, Entries, Expectation, NetworkOption, OpenOptions, SPECIFIER, SafeArea, Testing,
        registry,
    },
    run::{Built, Compile, Entry, Failure, Output, TestResult, TestRun, run_file},
    runner::{PROTOCOL, Reply, RunnerMessage, TestRequest, runner},
};

/// The sets the entry `nsh test` makes for a test file is built against
/// besides the app's: `nsh/test` ([`registry`]) and the module that runs
/// the file's tests.
pub fn test_sets() -> Vec<Registry> {
    vec![registry(), module::runner_registry()]
}

/// The TSX (or TS, JS) entry `entry` bundled with nsh's bundler and
/// compiled to QuickJS bytecode against `ui_sets`, as `nsh run` builds it:
/// the app to run (`EngineConfig::new(bundle)`), or what failed. Build it
/// once for several tests and clone it.
#[cfg(feature = "bundle")]
pub fn compile_tsx(entry: impl AsRef<Path>, ui_sets: &[Registry]) -> Result<Bundle, String> {
    let (app, map) = build(entry.as_ref(), ui_sets)?;
    Ok(match map {
        Some(map) => app.with_source_map(map),
        None => app,
    })
}

/// `entry` built against `ui_sets`: its bytecode and source map.
#[cfg(feature = "bundle")]
fn build(entry: &Path, ui_sets: &[Registry]) -> Result<(Bundle, Option<String>), String> {
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
    Ok((unsafe { Bundle::bytecode(bytecode) }, bundle.source_map))
}

/// Builds the entries of `nsh test` ([`TestRun::compile`]) in this
/// process, against `ui_sets` (and [`test_sets`] for test files).
#[cfg(feature = "bundle")]
pub fn compiler(ui_sets: Vec<Registry>) -> Compile {
    Arc::new(move |entry: &Path, kind: Entry| {
        // An app's directory: its `App.tsx`.
        let entry = &if entry.is_dir() {
            entry.join("App.tsx")
        } else {
            entry.to_owned()
        };
        let (bundle, map) = match kind {
            Entry::App => build(entry, &ui_sets)?,
            Entry::TestFile => {
                let mut sets = ui_sets.clone();
                sets.extend(test_sets());
                build(entry, &sets)?
            }
        };
        Ok(Built::Js(Box::new(bundle), map))
    })
}

/// Starts the TSX entry `entry` built against `ui_sets` ([`compile_tsx`])
/// on a phone-sized headless engine ([`EngineConfig::new`]) serving them.
#[cfg(feature = "bundle")]
pub fn tsx(entry: impl AsRef<Path>, ui_sets: Vec<Registry>) -> Result<Headless, String> {
    let app = compile_tsx(entry, &ui_sets)?;
    Headless::start(EngineConfig {
        ui_sets,
        ..EngineConfig::new(app)
    })
    .map_err(|e| format!("the engine did not start: {e}"))
}
