//! The member `unify` of the generated workspace: one build of what
//! `describe` and the development host share.
//!
//! `nsh` builds `describe` and this machine's development host with a
//! `cargo build -p` each, and Cargo unifies features within one invocation
//! only: each would build `nsh-core`, `nsh-base`, the app's crates and their
//! dependencies with features of its own, and the target directory would
//! keep both builds. `unify` depends on each crate of `describe`'s
//! dependency tree with the features Cargo resolves for the two together,
//! and the two depend on it, so they resolve those crates alike
//! (cargo-hakari's workspace-hack).
//!
//! It covers `describe`'s tree only, so `describe` builds its own crates
//! with more features and nothing more: the host's whole tree would build
//! the JS engine and the platform's backend into it, which `nsh check` waits
//! for. The headless runner (`test`) keeps its own build: the host's
//! features would change what it does (`nsh-core`'s `loader` downloads
//! remote images on Linux and Windows, `svg` draws `svg()`). Cargo's
//! `resolver.feature-unification = "workspace"` replaces it once that is
//! stable (rust-lang/cargo#14774).

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    env,
    path::Path,
    process::Command,
};

use anyhow::{Context as _, bail};

/// The member's directory in the workspace.
pub(super) const DIR: &str = "unify";

/// The `cfg` that switches the member's dependencies off while its own are
/// resolved, so features it requested before do not count.
const OFF: &str = "nsh_unify_off";

/// What a program that depends on the member says, so Cargo does not warn
/// of an unused dependency.
pub(super) const USE: &str =
    "\n// One build of what the programs share (`unify`).\nuse unify as _;\n";

/// The development host of this machine, which builds without `--target`
/// as `describe` does and so can share its build.
pub(super) fn native_host() -> Option<&'static str> {
    match env::consts::OS {
        "macos" => Some("macos-host"),
        "linux" => Some("linux-host"),
        "windows" => Some("windows-host"),
        _ => None,
    }
}

/// A program's dependency on the member `package`.
pub(super) fn dependency(package: &str) -> String {
    format!("unify = {{ package = {package:?}, path = \"../{DIR}\" }}\n")
}

/// The member `package`'s manifest: `target` and `host` are its
/// dependencies' lines, for the programs and for their build scripts and
/// procedural macros (Cargo resolves the two apart).
pub(super) fn manifest(package: &str, target: &str, host: &str) -> String {
    format!(
        "[package]\nname = {package:?}\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\
         build = \"build.rs\"\n\n[lib]\npath = \"lib.rs\"\n\n\
         [target.'cfg(not({OFF}))'.dependencies]\n{target}\n\
         [target.'cfg(not({OFF}))'.build-dependencies]\n{host}"
    )
}

/// Writes the member `package`'s dependencies after the features Cargo
/// resolves for `programs` together, `describe`'s package first, with the
/// builds' target directory `target_dir`.
pub(super) fn update(
    ws: &Path,
    package: &str,
    programs: &[String],
    target_dir: &Path,
) -> anyhow::Result<()> {
    let path = ws.join(DIR).join("Cargo.toml");
    let tree = match tree(ws, programs, target_dir) {
        Ok(tree) => tree,
        // Cargo reads the member's dependencies even when they are off: ones
        // written before (a moved nsh checkout, a version the app's crates
        // now refuse) stop it, so it resolves again without them.
        Err(_) => {
            super::write(&path, &manifest(package, "", ""))?;
            tree(ws, programs, target_dir)?
        }
    };
    let (target, host) = dependencies(&tree, ws, &programs[0])?;
    super::write(&path, &manifest(package, &target, &host))
}

/// `cargo tree` of `programs`, resolved together without the member's
/// dependencies: without `--target`, as the programs build, so `RUSTFLAGS`'
/// `cfg` applies to build scripts too. Flags from Cargo's configuration do
/// not count then; a dependency they select is left to the programs.
fn tree(ws: &Path, programs: &[String], target_dir: &Path) -> anyhow::Result<String> {
    let mut cmd = Command::new(super::cargo());
    cmd.arg("tree")
        .arg("--manifest-path")
        .arg(ws.join("Cargo.toml"))
        .args(["-e", "normal,build", "--charset", "ascii", "-f", "{p}|{f}"])
        .env("CARGO_TARGET_DIR", target_dir);
    for program in programs {
        cmd.args(["-p", program]);
    }
    if let Some(mut flags) = env::var_os("CARGO_ENCODED_RUSTFLAGS") {
        if !flags.is_empty() {
            flags.push("\x1f");
        }
        flags.push(format!("--cfg\x1f{OFF}"));
        cmd.env("CARGO_ENCODED_RUSTFLAGS", flags);
    } else {
        let mut flags = env::var_os("RUSTFLAGS").unwrap_or_default();
        if !flags.is_empty() {
            flags.push(" ");
        }
        flags.push(format!("--cfg {OFF}"));
        cmd.env("RUSTFLAGS", flags);
    }
    let out = cmd.output().context("run cargo tree")?;
    if !out.status.success() {
        bail!(
            "cargo tree failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout).context("cargo tree printed no UTF-8")
}

/// Which side of the build a crate is on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Side {
    Target,
    /// Build scripts, procedural macros and what they use.
    Host,
}

/// A crate as `cargo tree -f "{p}|{f}"` prints it, without its
/// `(proc-macro)` mark.
struct Node<'a> {
    package: String,
    features: &'a str,
    proc_macro: bool,
}

/// The dependency lines (target, host) of the crates in `root`'s tree in
/// `tree` (`cargo tree --charset ascii -f "{p}|{f}" -e normal,build`), the
/// workspace `ws`'s members left out.
fn dependencies(tree: &str, ws: &Path, root: &str) -> anyhow::Result<(String, String)> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut ids: HashMap<(String, &str), usize> = HashMap::new();
    let mut edges: Vec<Vec<(usize, bool)>> = Vec::new();
    // The node at each depth, and whether its children so far are build
    // dependencies (after its `[build-dependencies]` line).
    let mut stack: Vec<(usize, bool)> = Vec::new();
    let mut roots = Vec::new();
    for line in tree.lines() {
        let content = line.trim_start_matches(['|', '`', '-', ' ']);
        if content.is_empty() {
            continue;
        }
        let depth = (line.len() - content.len()) / 4;
        if content.starts_with('[') {
            // A section of the node above: `[build-dependencies]`.
            if let Some(parent) = stack.get_mut(depth) {
                parent.1 = content == "[build-dependencies]";
            }
            continue;
        }
        let (package, features) = content
            .rsplit_once('|')
            .with_context(|| format!("cargo tree printed {line:?}"))?;
        let features = features.trim_end_matches(" (*)");
        // The mark comes before the source: `name v1.0.0 (proc-macro) (/path)`.
        let (package, proc_macro) = match package.split_once(" (proc-macro)") {
            Some((name, source)) => (format!("{name}{source}"), true),
            None => (package.to_owned(), false),
        };
        let id = match ids.get(&(package.clone(), features)) {
            Some(&id) => id,
            None => {
                ids.insert((package.clone(), features), nodes.len());
                nodes.push(Node {
                    package,
                    features,
                    proc_macro,
                });
                edges.push(Vec::new());
                nodes.len() - 1
            }
        };
        stack.truncate(depth);
        match stack.last() {
            Some(&(parent, build)) if !edges[parent].contains(&(id, build)) => {
                edges[parent].push((id, build));
            }
            Some(_) => {}
            None => roots.push(id),
        }
        stack.push((id, false));
    }
    let prefix = format!("{root} v");
    let root = *roots
        .iter()
        .find(|&&r| nodes[r].package.starts_with(&prefix))
        .with_context(|| format!("cargo tree printed no {root}"))?;
    let mut seen = HashSet::from([(root, Side::Target)]);
    let mut queue = vec![(root, Side::Target)];
    let mut crates: BTreeMap<(Side, &str), BTreeSet<&str>> = BTreeMap::new();
    while let Some((id, side)) = queue.pop() {
        for &(child, build) in &edges[id] {
            let side = if build || nodes[child].proc_macro {
                Side::Host
            } else {
                side
            };
            if seen.insert((child, side)) {
                let features = crates.entry((side, &nodes[child].package)).or_default();
                features.extend(nodes[child].features.split(',').filter(|f| !f.is_empty()));
                queue.push((child, side));
            }
        }
    }
    let parsed: Vec<_> = crates
        .iter()
        .filter_map(|(&(side, package), features)| {
            let (name, version, spec) = spec(package, ws)?;
            Some((side, name, version, spec, features))
        })
        .collect();
    let keys = keys(
        parsed
            .iter()
            .map(|(_, name, version, spec, _)| (*name, *version, spec)),
    );
    let mut lines = (String::new(), String::new());
    for (side, name, version, spec, features) in &parsed {
        let key = &keys[&(*name, *version, spec)];
        let rename = if key == name {
            String::new()
        } else {
            format!(", package = {name:?}")
        };
        let features: Vec<String> = features.iter().map(|f| format!("{f:?}")).collect();
        let out = match side {
            Side::Target => &mut lines.0,
            Side::Host => &mut lines.1,
        };
        out.push_str(&format!(
            "{key} = {{ {spec}{rename}, default-features = false, features = [{}] }}\n",
            features.join(", ")
        ));
    }
    Ok(lines)
}

/// The dependency key of each crate (name, version, specification): its
/// name, else for a name of several crates (versions, or sources of one
/// version) the name and version, numbered after the first of a source. One
/// key per crate in both tables: Cargo refuses a crate under two names.
fn keys<'a>(
    crates: impl Iterator<Item = (&'a str, &'a str, &'a String)>,
) -> HashMap<(&'a str, &'a str, &'a String), String> {
    let mut by_name: BTreeMap<&str, BTreeSet<(&str, &String)>> = BTreeMap::new();
    for (name, version, spec) in crates {
        by_name.entry(name).or_default().insert((version, spec));
    }
    let mut keys = HashMap::new();
    for (name, crates) in by_name {
        let mut used = HashSet::new();
        for &(version, spec) in &crates {
            let key = if crates.len() == 1 {
                name.to_owned()
            } else {
                let base = format!("{name}-{}", version.replace(['.', '+'], "_"));
                let mut key = base.clone();
                let mut n = 1;
                while !used.insert(key.clone()) {
                    n += 1;
                    key = format!("{base}-{n}");
                }
                key
            };
            keys.insert((name, version, spec), key);
        }
    }
    keys
}

/// The name, version and dependency specification of `package` as
/// `cargo tree` prints it (`name v1.2.3`, then its path or Git source), or
/// `None` for one the member leaves to the programs: a member of `ws`, or
/// one of another registry than crates.io.
fn spec<'a>(package: &'a str, ws: &Path) -> Option<(&'a str, &'a str, String)> {
    let (name, rest) = package.split_once(" v")?;
    let (version, source) = match rest.split_once(" (") {
        Some((version, source)) => (version, source.strip_suffix(')')),
        None => (rest, None),
    };
    let spec = match source {
        None => {
            // Build metadata is ignored in requirements (Cargo warns).
            let version = version.split('+').next().unwrap_or(version);
            format!("version = {version:?}")
        }
        Some(path) if Path::new(path).is_absolute() => {
            if Path::new(path).starts_with(ws) {
                return None;
            }
            format!("path = {path:?}")
        }
        Some(git) if git.contains("://") => {
            let url = git.split('#').next()?;
            let (url, reference) = match url.split_once('?') {
                // Cargo writes the reference URL-encoded (`branch=a%2Fb`).
                Some((url, query)) => {
                    let (kind, value) = url::form_urlencoded::parse(query.as_bytes()).next()?;
                    if !matches!(&*kind, "branch" | "tag" | "rev") {
                        return None;
                    }
                    (url, format!(", {kind} = {value:?}"))
                }
                None => (url, String::new()),
            };
            format!("git = {url:?}{reference}")
        }
        Some(_) => return None,
    };
    Some((name, version, spec))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo tree`'s output for a workspace `/ws` under `root` (`C:` on
    /// Windows, where paths need a drive to be absolute).
    fn tree(root: &str) -> String {
        format!(
            "\
app-describe v0.0.0 ({root}/ws/describe)|
|-- unify v0.0.0 ({root}/ws/unify)|
|-- app v0.1.0 ({root}/apps/app)|
|   |-- nsh-core v0.4.0 ({root}/nsh/crates/nsh-core)|declarations,shader,svg
|   |   |-- nsh-macros v0.4.0 (proc-macro) ({root}/nsh/crates/nsh-macros)|
|   |   |   `-- quote v1.0.47|default,proc-macro
|   |   |-- smallvec v1.16.5|const_generics,union
|   |   `-- serde_derive v1.0.229 (proc-macro)|default
|   |       `-- syn v2.0.1|default,full
|   |   [build-dependencies]
|   |   |-- cc v1.11.1|parallel
|   |   `-- hashbrown v0.17.1|default
|   |-- dup v1.0.0|
|   |-- dup v1.0.0 ({root}/forks/dup)|
|   `-- hashbrown v0.16.1|default
|-- hashbrown v0.17.1|default,inline-more
`-- gitdep v0.2.0 (https://github.com/o/gitdep?branch=release%2F1.0#abc123)|
    `-- smallvec v1.16.5|const_generics,union (*)

app-macos-host v0.0.0 ({root}/ws/macos-host)|
`-- nsh-engine v0.4.0 ({root}/nsh/crates/nsh-engine)|shader
    `-- nsh-core v0.4.0 ({root}/nsh/crates/nsh-core)|declarations,shader,svg (*)
"
        )
    }

    const ROOT: &str = if cfg!(windows) { "C:" } else { "" };

    #[test]
    fn describe_s_tree_with_the_programs_features_one_key_per_crate() {
        let ws = format!("{ROOT}/ws");
        let (target, host) = dependencies(&tree(ROOT), Path::new(&ws), "app-describe").unwrap();
        assert_eq!(
            target,
            format!(
                "app = {{ path = \"{ROOT}/apps/app\", default-features = false, features = [] }}\n\
                 dup-1_0_0-2 = {{ version = \"1.0.0\", package = \"dup\", default-features = false, features = [] }}\n\
                 dup-1_0_0 = {{ path = \"{ROOT}/forks/dup\", package = \"dup\", default-features = false, features = [] }}\n\
                 gitdep = {{ git = \"https://github.com/o/gitdep\", branch = \"release/1.0\", default-features = false, features = [] }}\n\
                 hashbrown-0_16_1 = {{ version = \"0.16.1\", package = \"hashbrown\", default-features = false, features = [\"default\"] }}\n\
                 hashbrown-0_17_1 = {{ version = \"0.17.1\", package = \"hashbrown\", default-features = false, features = [\"default\", \"inline-more\"] }}\n\
                 nsh-core = {{ path = \"{ROOT}/nsh/crates/nsh-core\", default-features = false, features = [\"declarations\", \"shader\", \"svg\"] }}\n\
                 smallvec = {{ version = \"1.16.5\", default-features = false, features = [\"const_generics\", \"union\"] }}\n"
            )
        );
        assert_eq!(
            host,
            format!(
                "cc = {{ version = \"1.11.1\", default-features = false, features = [\"parallel\"] }}\n\
                 hashbrown-0_17_1 = {{ version = \"0.17.1\", package = \"hashbrown\", default-features = false, features = [\"default\"] }}\n\
                 nsh-macros = {{ path = \"{ROOT}/nsh/crates/nsh-macros\", default-features = false, features = [] }}\n\
                 quote = {{ version = \"1.0.47\", default-features = false, features = [\"default\", \"proc-macro\"] }}\n\
                 serde_derive = {{ version = \"1.0.229\", default-features = false, features = [\"default\"] }}\n\
                 syn = {{ version = \"2.0.1\", default-features = false, features = [\"default\", \"full\"] }}\n"
            )
        );
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let ws = format!("{ROOT}/ws");
        assert!(dependencies(&tree(ROOT), Path::new(&ws), "app-test").is_err());
    }
}
