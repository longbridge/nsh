//! `nsh test`: runs an app's test files (`*.test.tsx`, `*.test.ts`, which
//! import `nsh/test`) headless, each on an engine of its own, and says how
//! each test ended (`nsh-testing` runs them). Given an app's entry instead,
//! it opens the app as a test would and takes the steps standard input
//! gives (`await app.press("Add")`): with `--trace`, what `nsh trace`
//! printed.

use std::{
    collections::HashMap,
    io::{IsTerminal as _, Read as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::{Context as _, bail};
use nsh_core::ui::description::SetDescription;
use nsh_engine::Bundle;
use nsh_testing::{Entry, TestResult, TestRun};

use crate::ui;

/// How long the app runs after it opened before the steps of standard
/// input: long enough for the engine's idle collection (every second at
/// most), so what the app keeps alive shows in a trace.
const IDLE_MS: u32 = 1200;

/// Runs the tests `path` names: a test file, the test files in a directory
/// (an app's), or an app's entry with the steps standard input gives.
pub(crate) fn test(path: &Path, filter: Option<String>, trace: bool) -> anyhow::Result<()> {
    let path = dunce::canonicalize(path).with_context(|| format!("{}", path.display()))?;
    let entry = app_entry(&path)?;
    if crate::rust_app::of(&entry)?.is_some() || crate::rust_app::is_root(&entry) {
        bail!(
            "nsh test runs apps whose code is TypeScript; {} has Rust crates of its own \
             (`nsh.json` \"rust\"): trace it with `nsh trace`",
            ui::path(&entry)
        );
    }
    crate::init::write_types_for(&entry);
    let app_dir = entry.parent().unwrap_or(Path::new(".")).to_path_buf();
    let run = TestRun {
        ui_sets: crate::own_sets().to_vec(),
        compile: compiler(),
        work_dir: app_dir.join(".nsh/test"),
        filter,
        trace: trace.then(|| Arc::new(Mutex::new(Box::new(std::io::stdout()) as _))),
    };
    let files = if is_test_file(&path) {
        vec![path.clone()]
    } else if path.is_dir() {
        let mut files = Vec::new();
        find_tests(&path, &mut files);
        files.sort();
        if files.is_empty() {
            bail!(
                "no test files in {}: tests are `*.test.tsx` / `*.test.ts` files importing nsh/test",
                ui::path(&path)
            );
        }
        files
    } else {
        vec![steps_file(&run.work_dir, &path)?]
    };
    let start = Instant::now();
    let (mut passed, mut failed) = (0, 0);
    for file in &files {
        let shown = if file.starts_with(&run.work_dir) {
            ui::path(&path)
        } else {
            ui::path(file)
        };
        let mut report = |result: &TestResult| {
            show(&shown, result);
            if result.failure.is_some() {
                failed += 1;
            } else {
                passed += 1;
            }
        };
        if let Err(e) = nsh_testing::run_file(&run, file, &mut report) {
            failed += 1;
            ui::error(format!("{shown}: {e}"));
        }
    }
    let total = passed + failed;
    let summary = format!(
        "{total} {}: {passed} passed, {failed} failed {}",
        if total == 1 { "test" } else { "tests" },
        ui::dim(format!("({})", ui::duration(start.elapsed())))
    );
    if failed > 0 {
        bail!(summary);
    }
    ui::done(summary);
    Ok(())
}

/// Prints how a test of the file shown as `file` ended: a line, and for a
/// failure what was thrown, where, and what was on screen.
fn show(file: &str, result: &TestResult) {
    let line = format!(
        "{file} › {} {}",
        result.name,
        ui::dim(format!("({})", ui::duration(result.elapsed)))
    );
    let Some(failure) = &result.failure else {
        ui::done(line);
        return;
    };
    ui::plain(format!("{} {line}", ui::red("✗")));
    for text in failure.message.lines() {
        ui::detail(format!("  {text}"));
    }
    if let Some(stack) = &failure.stack {
        // The frames in the test file: the runtime's own say nothing.
        for frame in stack.lines().filter(|l| !l.contains("(native)")) {
            ui::detail(ui::dim(format!("  {}", frame.trim_start())));
        }
    }
    if let Some(screen) = &failure.screen {
        let texts: Vec<String> = screen.iter().map(|t| format!("{t:?}")).collect();
        ui::detail(format!("  on screen: {}", texts.join(", ")));
    }
}

/// The entry of the app `path` is part of: a directory's, a test file's
/// (the nearest directory up with an app), or `path` itself.
fn app_entry(path: &Path) -> anyhow::Result<PathBuf> {
    if path.is_dir() {
        return crate::entry::resolve(path);
    }
    if !is_test_file(path) {
        return Ok(path.to_owned());
    }
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d.join("nsh.json").is_file() || crate::entry::resolve(d).is_ok() {
            return crate::entry::resolve(d);
        }
        dir = d.parent();
    }
    bail!(
        "{}: no app around it (an App.tsx or nsh.json)",
        ui::path(path)
    )
}

fn is_test_file(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".test.tsx") || n.ends_with(".test.ts"))
}

/// Adds the test files under `dir` to `out`, outside hidden directories
/// (`.nsh`), `node_modules` and `target`.
fn find_tests(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !name.starts_with('.') && name != "node_modules" && name != "target" {
                find_tests(&path, out);
            }
        } else if is_test_file(&path) {
            out.push(path);
        }
    }
}

/// A test file that opens the app at `entry` on the network, lets it run
/// for a while, then takes the steps standard input gives (none from a
/// terminal), with `app` the app.
fn steps_file(work_dir: &Path, entry: &Path) -> anyhow::Result<PathBuf> {
    let mut steps = String::new();
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        stdin
            .lock()
            .read_to_string(&mut steps)
            .context("read the steps from standard input")?;
    }
    let name = entry
        .file_name()
        .map_or("app".into(), |n| n.to_string_lossy().into_owned());
    let entry_js = serde_json::to_string(&entry.to_string_lossy().replace('\\', "/"))?;
    let code = format!(
        "// Made by `nsh test {name}`: the steps of standard input.\n\
         import {{ expect, open, test }} from \"nsh/test\";\n\n\
         test(\"steps\", async () => {{\n\
         const app = await open({entry_js}, {{ network: \"live\" }});\n\
         await app.wait({IDLE_MS});\n\
         {steps}\n\
         }});\n"
    );
    std::fs::create_dir_all(work_dir).with_context(|| format!("{}", work_dir.display()))?;
    let file = work_dir.join(format!("{name}.steps.test.ts"));
    std::fs::write(&file, code).with_context(|| format!("write {}", file.display()))?;
    Ok(file)
}

/// Builds the apps tests open against the UI sets this command line
/// serves (each entry once), and the entries made for test files against
/// them and `nsh/test`.
fn compiler() -> nsh_testing::Compile {
    type Built = (Vec<u8>, Option<String>);
    let built: Mutex<HashMap<PathBuf, Built>> = Mutex::new(HashMap::new());
    Arc::new(move |entry: &Path, kind: Entry| {
        let (bytecode, map) = match kind {
            Entry::App => {
                let cached = built
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(entry)
                    .cloned();
                match cached {
                    Some(built) => built,
                    None => {
                        let out = crate::trace::compile(entry).map_err(|e| format!("{e:#}"))?;
                        built
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(entry.to_owned(), out.clone());
                        out
                    }
                }
            }
            Entry::TestFile => {
                let mut sets = crate::ui_sets().to_vec();
                sets.extend(nsh_testing::test_sets().iter().map(SetDescription::from));
                crate::trace::compile_with(entry, &sets, false).map_err(|e| format!("{e:#}"))?
            }
        };
        // SAFETY: compiled just now by this runtime.
        Ok((unsafe { Bundle::bytecode(bytecode) }, map))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_files_are_found_where_the_app_keeps_them_and_know_their_app() {
        let dir = std::env::temp_dir().join(format!("nsh-test-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for file in [
            "App.tsx",
            "App.test.tsx",
            "src/cart/Cart.test.ts",
            "src/cart/Cart.tsx",
            ".nsh/test/App.test.tsx.run.js",
            ".nsh/test/Old.test.ts",
            "node_modules/x/x.test.ts",
        ] {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
        }
        let dir = dunce::canonicalize(&dir).unwrap();
        let mut found = Vec::new();
        find_tests(&dir, &mut found);
        found.sort();
        assert_eq!(
            found,
            [dir.join("App.test.tsx"), dir.join("src/cart/Cart.test.ts")]
        );
        // A test file's app is the nearest one up; a directory's is its own;
        // any other file is an entry.
        let entry = dir.join("App.tsx");
        assert_eq!(
            app_entry(&dir.join("src/cart/Cart.test.ts")).unwrap(),
            entry
        );
        assert_eq!(app_entry(&dir).unwrap(), entry);
        assert_eq!(
            app_entry(&dir.join("src/cart/Cart.tsx")).unwrap(),
            dir.join("src/cart/Cart.tsx")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
