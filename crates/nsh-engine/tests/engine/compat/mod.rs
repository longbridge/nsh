//! Backward compatibility (docs/dev/compatibility.md): what each release
//! offered apps is kept in `api/v<version>/`, and every later checkout
//! still offers it and still runs the apps it built. `api/current/` is this
//! checkout's own surface, so every change to it shows in review.
//!
//! `NSH_API_UPDATE=1` rewrites `api/current/` and `NSH_API_FREEZE=<dir>`
//! writes the surface into `<dir>`: `cargo xtask api` and
//! `cargo xtask api --freeze` run them.

mod apps;
mod compare;
mod surface;

use std::path::{Path, PathBuf};

/// The first release kept: versions before it make no promises.
const FIRST: (u32, u32, u32) = (0, 1, 2);

fn api_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../api")
}

fn version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.strip_prefix('v')?.split('.').map(|p| p.parse().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// The baselines of releases, oldest first.
fn baselines() -> Vec<(String, PathBuf)> {
    let mut out: Vec<_> = std::fs::read_dir(api_dir())
        .unwrap()
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().into_string().ok()?;
            Some((version(&name)?, name, e.path()))
        })
        .collect();
    out.sort();
    out.into_iter().map(|(_, n, p)| (n, p)).collect()
}

/// The files under `dir`, as (path relative to it, contents).
fn read_types(dir: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy();
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((rel.replace('\\', "/"), text.replace("\r\n", "\n")));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// Writes this checkout's surface into `dir`: `runtime.txt` and `types/`.
fn write_surface(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir.join("types"));
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("runtime.txt"), surface::runtime()).unwrap();
    for (path, text) in surface::types() {
        let path = dir.join("types").join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

/// `api/current/` is this checkout's surface.
#[test]
fn current_surface_is_recorded() {
    let dir = api_dir().join("current");
    if std::env::var_os("NSH_API_UPDATE").is_some() {
        write_surface(&dir);
    }
    if let Some(dir) = std::env::var_os("NSH_API_FREEZE") {
        write_surface(Path::new(&dir));
    }
    let runtime = surface::runtime();
    let types = surface::types();
    let recorded_runtime = std::fs::read_to_string(dir.join("runtime.txt"))
        .unwrap_or_default()
        .replace("\r\n", "\n");
    let recorded_types = read_types(&dir.join("types"));
    let mut stale: Vec<&str> = types
        .iter()
        .filter(|f| !recorded_types.contains(f))
        .chain(recorded_types.iter().filter(|f| !types.contains(f)))
        .map(|(p, _)| p.as_str())
        .collect();
    stale.sort_unstable();
    stale.dedup();
    if runtime != recorded_runtime {
        stale.push("runtime.txt");
    }
    assert!(
        stale.is_empty(),
        "the API changed: {stale:?} in api/current differ from this checkout.\n\
         Check that the change only adds (docs/dev/compatibility.md), then record it:\n\
         cargo xtask api"
    );
}

/// Everything a release offered is still offered, by the rules of
/// `compare`.
#[test]
fn releases_are_still_offered() {
    let runtime = surface::runtime();
    let types = surface::types();
    let mut breaks = Vec::new();
    for (name, dir) in baselines() {
        let base_runtime = std::fs::read_to_string(dir.join("runtime.txt")).unwrap();
        for b in compare::runtime(&base_runtime, &runtime) {
            breaks.push(format!("{name} runtime {b}"));
        }
        for b in compare::types(&read_types(&dir.join("types")), &types) {
            breaks.push(format!("{name} types {b}"));
        }
    }
    assert!(
        breaks.is_empty(),
        "apps written for these releases break (docs/dev/compatibility.md):\n{}",
        breaks.join("\n")
    );
}

/// The apps each release built still start and work.
#[test]
fn apps_of_releases_still_run() {
    let mut failed = Vec::new();
    for (name, dir) in baselines() {
        let Ok(apps) = std::fs::read_dir(dir.join("apps")) else {
            continue;
        };
        let mut apps: Vec<_> = apps.map(|e| e.unwrap().path()).collect();
        apps.sort();
        for app in apps {
            if let Err(e) = apps::run_app(&app) {
                let app = app.file_name().unwrap().to_string_lossy().into_owned();
                failed.push(format!("{name} {app}: {e}"));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n\n"));
}

/// A release has its baseline: bumping the version freezes the API.
#[test]
fn the_version_has_a_baseline() {
    let first = baselines().first().and_then(|(n, _)| version(n));
    assert_eq!(first, Some(FIRST), "the first baseline");
    let current = format!("v{}", env!("CARGO_PKG_VERSION"));
    if version(&current).unwrap() >= FIRST {
        assert!(
            api_dir().join(&current).join("runtime.txt").exists(),
            "{current} has no baseline: cargo xtask api --freeze"
        );
    }
}

/// The comparison understands every declaration this checkout makes (an
/// unknown form panics) and names every member.
#[test]
fn declarations_are_understood() {
    for (path, text) in surface::types() {
        for (name, d) in compare::declarations(&text) {
            if matches!(d.kind, "interface" | "class") {
                assert!(
                    d.members.keys().all(|k| !k.is_empty()),
                    "{path}: {name}: a member without a name: {:?}",
                    d.members
                );
            }
        }
    }
}
