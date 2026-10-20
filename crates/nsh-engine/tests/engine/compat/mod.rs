//! Backward compatibility (docs/dev/compatibility.md): `api/` is one
//! append-only record of what apps depend on, as the web platform keeps
//! its own. `runtime.txt` and `types/` are this checkout's surface, so every
//! change to it shows in review; the fixture apps in `apps/` are bundles
//! some checkout's compiler wrote, never rebuilt, and still run here.
//!
//! There are no copies per version: git's history is the baseline. The
//! record as of the merge base with `main` (on `main`, the parent commit)
//! must still be offered, so by induction nothing that ever reached `main`
//! goes, except the entries `api/closed.txt` closes for security. The
//! record holds from the commit that added `api/in-force.txt` on: a base
//! without it (before that commit reached `main`) holds nothing.
//!
//! `NSH_API_UPDATE=1` rewrites `runtime.txt` and `types/` (`cargo xtask api`
//! runs it); `NSH_API_BASE=<rev>` names another base.

mod apps;
mod compare;
mod coverage;
mod surface;
mod tailwind;
mod values;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

fn api_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../api")
}

/// The files under `dir`, as (path relative to it, contents).
fn read_types(dir: &Path) -> Vec<(String, String)> {
    read_files(dir)
        .into_iter()
        .map(|(path, bytes)| (path, text(bytes)))
        .collect()
}

/// The files under `dir`, as (path relative to it, bytes), sorted.
fn read_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy();
                out.push((rel.replace('\\', "/"), std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// A text file as the comparison sees it: a checkout on Windows may have
/// turned its line ends into CRLF.
fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap().replace("\r\n", "\n")
}

/// Writes this checkout's surface into `dir`: `runtime.txt` and `types/`.
/// The fixture apps and `closed.txt` are never written: they are kept by
/// hand.
fn write_surface(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir.join("types"));
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("runtime.txt"), surface::runtime()).unwrap();
    std::fs::write(dir.join("tailwind.txt"), tailwind::record()).unwrap();
    for (path, text) in surface::types() {
        let path = dir.join("types").join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

/// `api/runtime.txt` and `api/types/` are this checkout's surface.
#[test]
fn surface_is_recorded() {
    let dir = api_dir();
    if std::env::var_os("NSH_API_UPDATE").is_some() {
        write_surface(&dir);
    }
    let runtime = surface::runtime();
    let types = surface::types();
    let recorded_runtime = std::fs::read(dir.join("runtime.txt"))
        .map(text)
        .unwrap_or_default();
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
    let recorded_tailwind = std::fs::read(dir.join("tailwind.txt"))
        .map(text)
        .unwrap_or_default();
    if tailwind::record() != recorded_tailwind {
        stale.push("tailwind.txt");
    }
    assert!(
        stale.is_empty(),
        "the API changed: {stale:?} in api/ differ from this checkout.\n\
         Check that the change only adds (docs/dev/compatibility.md), then record it:\n\
         cargo xtask api"
    );
}

/// Runs git in the repository; its output, or why it failed.
fn git(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(api_dir().join(".."))
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

fn git_line(args: &[&str]) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&git(args)?).trim().to_owned())
}

/// The revision whose record this checkout is held to: `NSH_API_BASE`, or
/// the merge base with `main`; on `main` itself, the parent commit, which
/// is the merge base the change had.
fn base_revision() -> Result<String, String> {
    if let Some(rev) = std::env::var_os("NSH_API_BASE") {
        let rev = rev.to_string_lossy();
        return git_line(&["rev-parse", "--verify", &format!("{rev}^{{commit}}")]);
    }
    let head = git_line(&["rev-parse", "HEAD"])?;
    let base = git_line(&["merge-base", "HEAD", "origin/main"])
        .or_else(|_| git_line(&["merge-base", "HEAD", "main"]))?;
    if base == head {
        git_line(&["rev-parse", "--verify", "HEAD^"])
    } else {
        Ok(base)
    }
}

/// The file of `api/` whose presence at the base revision puts the record
/// in force (docs/dev/compatibility.md). It is read at the base, so a
/// change that deletes it is held all the same.
const IN_FORCE: &str = "in-force.txt";

/// Whether `base` (the files of `api/` at the base revision) holds this
/// checkout: the record was in force there.
fn in_force(base: &BTreeMap<String, Vec<u8>>) -> bool {
    base.contains_key(IN_FORCE)
}

/// The files of `api/` at `rev`, as (path relative to `api/`, bytes).
fn record_at(rev: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let names = git(&["ls-tree", "-r", "--name-only", rev, "--", "api/"])?;
    let mut out = BTreeMap::new();
    for path in String::from_utf8_lossy(&names).lines() {
        let bytes = git(&["show", &format!("{rev}:{path}")])?;
        out.insert(path.trim_start_matches("api/").to_owned(), bytes);
    }
    Ok(out)
}

/// The entries of `closed.txt`: one per line, `<entry> -- <reason>`, where
/// an entry is what a break reported by `compare` starts with: `[<module>]
/// <path>` (a name of `runtime.txt`), `<file>: <Name>` or `<file>:
/// <Name>.<member>` (a declaration of `types/`), or `<file>` (all of it).
/// It closes the name and everything under it.
fn closed(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (entry, reason) = l
                .split_once(" -- ")
                .unwrap_or_else(|| panic!("api/closed.txt: `{l}` has no ` -- <reason>`"));
            assert!(
                !reason.trim().is_empty(),
                "api/closed.txt: `{l}` has no reason"
            );
            entry.trim().to_owned()
        })
        .collect()
}

/// Whether `entry` closes the break `b`: the break is about the entry or a
/// member of it (`a` closes `a.b` and `a#.m`, not `ab`).
fn is_closed(entry: &str, b: &str) -> bool {
    b.strip_prefix(entry)
        .is_some_and(|rest| rest.starts_with([':', '.', '#']))
}

/// Whether `base`'s steps are all still in `now`, in order: scripts may
/// gain steps, never lose or reorder them.
fn script_kept(base: &str, now: &str) -> bool {
    let steps = |s: &str| -> Vec<String> {
        s.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect()
    };
    let mut now = steps(now).into_iter();
    steps(base).iter().all(|step| now.any(|n| n == *step))
}

/// What `base` (the files of a record) offered that `now` (this checkout:
/// its surface and its `api/apps/` files) no longer does, less what
/// `closed` closes.
fn record_breaks(
    base: &BTreeMap<String, Vec<u8>>,
    runtime: &str,
    types: &[(String, String)],
    apps: &BTreeMap<String, Vec<u8>>,
    closed: &[String],
) -> Vec<String> {
    let open = |b: &String| !closed.iter().any(|e| is_closed(e, b));
    let mut breaks = Vec::new();
    if let Some(base_runtime) = base.get("runtime.txt") {
        let base_runtime = text(base_runtime.clone());
        for b in compare::runtime(&base_runtime, runtime)
            .iter()
            .filter(|b| open(b))
        {
            breaks.push(format!("runtime {b}"));
        }
    }
    let base_types: Vec<_> = base
        .iter()
        .filter_map(|(p, bytes)| Some((p.strip_prefix("types/")?.to_owned(), text(bytes.clone()))))
        .collect();
    for b in compare::types(&base_types, types)
        .iter()
        .filter(|b| open(b))
    {
        breaks.push(format!("types {b}"));
    }
    for (path, bytes) in base {
        let Some(rel) = path.strip_prefix("apps/") else {
            continue;
        };
        match apps.get(rel) {
            None => breaks.push(format!("apps/{rel}: removed")),
            Some(now) if rel.ends_with("/script.txt") => {
                if !script_kept(&text(bytes.clone()), &text(now.clone())) {
                    breaks.push(format!("apps/{rel}: lost or reordered steps"));
                }
            }
            Some(now) if now != bytes => breaks.push(format!("apps/{rel}: changed")),
            Some(_) => {}
        }
    }
    breaks
}

/// Everything the record offered at the base revision is still offered,
/// by the rules of `compare`, and its fixture apps are kept as they were.
#[test]
fn nothing_merged_is_removed() {
    let base = base_revision().and_then(|rev| Ok((record_at(&rev)?, rev)));
    let (base, rev) = match base {
        Ok(base) => base,
        Err(e) if std::env::var_os("CI").is_some() => {
            panic!("the record's base: {e} (CI needs git's history and `main`)")
        }
        Err(e) => {
            eprintln!("note: nothing_merged_is_removed skipped: no base record ({e})");
            return;
        }
    };
    if !in_force(&base) {
        eprintln!("note: nothing_merged_is_removed: the record was not in force at {rev}");
        return;
    }
    let dir = api_dir();
    let closed = closed(
        &std::fs::read(dir.join("closed.txt"))
            .map(text)
            .unwrap_or_default(),
    );
    let apps = read_files(&dir.join("apps")).into_iter().collect();
    let mut breaks = record_breaks(
        &base,
        &surface::runtime(),
        &surface::types(),
        &apps,
        &closed,
    );
    if let Some(base_tailwind) = base.get("tailwind.txt") {
        let base_tailwind = text(base_tailwind.clone());
        breaks.extend(tailwind::breaks(
            &base_tailwind,
            &tailwind::record(),
            &closed,
        ));
    }
    assert!(
        breaks.is_empty(),
        "what api/ recorded at {rev} is no longer offered (docs/dev/compatibility.md):\n{}",
        breaks.join("\n")
    );
}

/// The comparison of records reports what goes and what changes, and
/// `closed.txt` exempts what it names.
#[test]
fn record_breaks_are_reported() {
    let file = |p: &str, t: &str| (p.to_owned(), t.as_bytes().to_vec());
    let runtime = "[nsh]\nView function\nText function\n";
    let types = "export declare function a(x: number): void;\n";
    let mut base: BTreeMap<_, _> = [
        file("runtime.txt", runtime),
        file("types/nsh/index.d.ts", types),
        file("apps/counter/bundle.js", "call(1);\n"),
        file(
            "apps/counter/script.txt",
            "# steps\nexpect 0\ntap id:inc\nexpect 1\n",
        ),
    ]
    .into();
    // Before the record is in force, a base holds nothing.
    assert!(!in_force(&base));
    base.insert(IN_FORCE.into(), b"# in force\n".to_vec());
    assert!(in_force(&base));
    let now_types = [("nsh/index.d.ts".to_owned(), types.to_owned())];
    let mut apps: BTreeMap<_, _> = base
        .iter()
        .filter_map(|(p, b)| Some((p.strip_prefix("apps/")?.to_owned(), b.clone())))
        .collect();
    assert!(record_breaks(&base, runtime, &now_types, &apps, &[]).is_empty());

    apps.insert(
        "counter/script.txt".into(),
        b"expect 0\nwait 10\ntap id:inc\nexpect 1\nexpect 2\n".to_vec(),
    );
    assert!(
        record_breaks(&base, runtime, &now_types, &apps, &[]).is_empty(),
        "scripts may gain steps"
    );

    let fewer = "[nsh]\nView function\n";
    apps.insert("counter/bundle.js".into(), b"call(2);\n".to_vec());
    apps.insert(
        "counter/script.txt".into(),
        b"tap id:inc\nexpect 0\nexpect 1\n".to_vec(),
    );
    let breaks = record_breaks(&base, fewer, &[], &apps, &[]);
    assert_eq!(
        breaks,
        [
            "runtime [nsh] Text: removed",
            "types nsh/index.d.ts: removed",
            "apps/counter/bundle.js: changed",
            "apps/counter/script.txt: lost or reordered steps",
        ]
    );

    let closed = closed("# closed\n[nsh] Text -- unsafe\nnsh/index.d.ts -- unsafe\n");
    assert_eq!(record_breaks(&base, fewer, &[], &apps, &closed).len(), 2);
    assert!(is_closed("[nsh] Text", "[nsh] Text#.m: removed"));
    assert!(!is_closed("[nsh] Text", "[nsh] TextInput: removed"));
}

/// The fixture apps still start and work.
#[test]
fn apps_still_run() {
    let mut apps: Vec<_> = std::fs::read_dir(api_dir().join("apps"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    apps.sort();
    let mut failed = Vec::new();
    for app in apps {
        if let Err(e) = apps::run_app(&app) {
            let app = app.file_name().unwrap().to_string_lossy().into_owned();
            failed.push(format!("{app}: {e}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n\n"));
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
