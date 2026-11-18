//! Third-party extensions: Rust crates from Git that export components,
//! modules and classes to the app, as the app's own crates do (`nsh.json`
//! `"rust"`, [`crate::rust_app`]). `nsh add <url> [--tag <tag> | --rev
//! <rev>]` lists one in `nsh.json`:
//!
//! ```json
//! { "extensions": [{ "git": "https://github.com/acme/ui", "tag": "v1.2.0" }] }
//! ```
//!
//! and pins the commit it resolved to in `nsh.lock`, next to `nsh.json`,
//! which the app commits: a build takes the locked commit, never what a tag
//! or branch points to now. An extension is an ordinary Cargo library with
//! a `#[native_module("<module>")]` entry at its root and, in its
//! `Cargo.toml`, what NSH checks before anything compiles:
//!
//! ```toml
//! [package.metadata.nsh]
//! format = 1                   # this metadata's version
//! module = "acme/ui"           # the specifier TSX imports it by, its #[native_module]
//! nsh = "0.6"                  # the NSH versions it builds with (a Cargo version requirement)
//! targets = ["macos", "linux"] # optional: the targets it supports (default: all)
//! ```
//!
//! Each locked commit is checked out once in nsh's cache
//! (`<cache>/extensions/<repository>/<commit>`) and linked into the app's
//! generated workspace like an app crate, so a pure TSX app gets hosts
//! built with it, and a Rust app's runners stay without a JS engine.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::rust_app::{self, AppCrate};

/// The lock file beside `nsh.json`.
pub(crate) const LOCK: &str = "nsh.lock";

/// The version of `[package.metadata.nsh]` this NSH reads.
const FORMAT: u64 = 1;

/// The targets an extension may list in `targets`.
const TARGETS: &[&str] = &[
    "macos", "ios", "android", "windows", "linux", "web", "gpui", "tui",
];

/// An extension as `nsh.json` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    /// The repository's URL.
    pub git: String,
    /// A tag to take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// A commit (or branch) to take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
}

impl Source {
    /// The reference to resolve: the tag, the rev, else the default branch.
    fn reference(&self) -> &str {
        self.tag
            .as_deref()
            .or(self.rev.as_deref())
            .unwrap_or("HEAD")
    }
}

/// `nsh.lock`: the commit each extension resolved to.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Lock {
    version: u64,
    #[serde(default)]
    extensions: Vec<Locked>,
}

/// An extension's entry in `nsh.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Locked {
    #[serde(flatten)]
    source: Source,
    commit: String,
    package: String,
    module: String,
}

/// What `[package.metadata.nsh]` of an extension says.
#[derive(Debug, Default)]
struct Metadata {
    format: Option<u64>,
    module: Option<String>,
    nsh: Option<String>,
    targets: Option<Vec<String>>,
}

/// The extensions `config` (the `nsh.json` in `dir`) lists, each checked
/// out at its locked commit, with how `nsh.json` names it. One not locked
/// yet is resolved and locked (`nsh.lock` written).
pub(crate) fn crates(
    dir: &Path,
    config: &serde_json::Value,
) -> anyhow::Result<Vec<(String, AppCrate)>> {
    let sources = sources(config)?;
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let lock = read_lock(dir)?;
    let mut locked = Vec::new();
    for source in &sources {
        let entry = match lock.extensions.iter().find(|l| l.source == *source) {
            Some(entry) => entry.clone(),
            None => resolve(source)?,
        };
        locked.push(entry);
    }
    let mut crates = Vec::new();
    for entry in &locked {
        let checkout = checkout(&entry.source.git, &entry.commit)?;
        let package = rust_app::package_name(&checkout.join("Cargo.toml"))?;
        check(&checkout, &entry.source.git)?;
        crates.push((entry.source.git.clone(), AppCrate::new(package, checkout)));
    }
    let new = Lock {
        version: 1,
        extensions: locked,
    };
    if new.extensions != lock.extensions {
        write_lock(dir, &new)?;
    }
    Ok(crates)
}

/// `nsh add`: lists the extension at `git` (`tag` or `rev`, else the
/// default branch) in the `nsh.json` in `dir`, replacing one of the same
/// repository, locks it, and checks it with the app's crates. On an error
/// `nsh.json` and `nsh.lock` are left as they were.
pub(crate) fn add(
    dir: &Path,
    git: &str,
    tag: Option<String>,
    rev: Option<String>,
) -> anyhow::Result<()> {
    let config_path = dir.join("nsh.json");
    let lock_path = dir.join(LOCK);
    let config_text = fs::read_to_string(&config_path).with_context(|| {
        format!(
            "no nsh.json in {}: run nsh add in the app's directory",
            dir.display()
        )
    })?;
    let lock_text = fs::read_to_string(&lock_path).ok();
    let mut config: serde_json::Value =
        serde_json::from_str(&config_text).context("nsh.json is not JSON")?;
    let source = Source {
        git: git.to_owned(),
        tag,
        rev,
    };
    // `nsh add` again: resolved anew, what the reference points to now.
    if let Ok(mut lock) = read_lock(dir)
        && lock.extensions.iter().any(|l| l.source.git == git)
    {
        lock.extensions.retain(|l| l.source.git != git);
        write_lock(dir, &lock)?;
    }
    let mut sources = sources(&config)?;
    sources.retain(|s| s.git != source.git);
    sources.push(source.clone());
    config["extensions"] = serde_json::to_value(&sources)?;
    let mut manifest_backup: Option<(PathBuf, String)> = None;
    let result = (|| {
        fs::write(&config_path, serde_json::to_string_pretty(&config)? + "\n")?;
        // The app's crates and the extensions together: a clash of
        // packages or modules is reported here, before a build.
        let extensions = crates(dir, &config)?;
        let mut all: Vec<(String, AppCrate)> = match config.get("rust") {
            Some(rust) => rust_crates(dir, rust)?,
            None => Vec::new(),
        };
        let app_crate = all.first().map(|(_, c)| c.dir().join("Cargo.toml"));
        all.extend(extensions);
        rust_app::check_crates(&all)?;
        let lock = read_lock(dir)?;
        // The app's crate uses it from Rust too (`use acme_ui::Gauge`).
        if let Some(manifest) = &app_crate
            && let Some(entry) = lock.extensions.iter().find(|l| l.source == source)
        {
            let text = fs::read_to_string(manifest)?;
            manifest_backup = Some((manifest.clone(), text.clone()));
            if let Some(new) =
                with_dependency(&text, &entry.package, &entry.source.git, &entry.commit)?
            {
                fs::write(manifest, new)?;
            }
        }
        anyhow::Ok(lock)
    })();
    match result {
        Ok(lock) => {
            let entry = lock
                .extensions
                .iter()
                .find(|l| l.source == source)
                .context("the extension is not in nsh.lock")?;
            println!(
                "added {} ({}) at {}: import from \"{}\"",
                entry.package,
                source.git,
                &entry.commit[..entry.commit.len().min(12)],
                entry.module
            );
            Ok(())
        }
        Err(e) => {
            fs::write(&config_path, config_text)?;
            if let Some((path, text)) = manifest_backup {
                fs::write(path, text)?;
            }
            match lock_text {
                Some(text) => fs::write(&lock_path, text)?,
                None => {
                    let _ = fs::remove_file(&lock_path);
                }
            }
            Err(e.context(format!("nsh add {git}: nsh.json and {LOCK} are unchanged")))
        }
    }
}

/// The manifest `text` depending on `package` from `git` at `commit`, or
/// `None` when it does already; an error when it takes `package` from
/// elsewhere.
fn with_dependency(
    text: &str,
    package: &str,
    git: &str,
    commit: &str,
) -> anyhow::Result<Option<String>> {
    let line = format!("{package} = {{ git = {git:?}, rev = {commit:?} }}");
    let mut out = Vec::new();
    let mut found = false;
    let mut table = String::new();
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            table = t.to_owned();
        }
        if table == "[dependencies]" && t.split(['=', ' ']).next() == Some(package) {
            if toml_has(t, "git", git) {
                // The same repository: at the locked commit, written under
                // the table's header already.
                continue;
            }
            bail!("Cargo.toml: [dependencies] already has {package} from elsewhere: `{t}`");
        }
        out.push(l.to_owned());
        if t == "[dependencies]" && !found {
            out.push(line.clone());
            found = true;
        }
    }
    if !found {
        out.push(String::new());
        out.push("[dependencies]".to_owned());
        out.push(line);
    }
    let new = out.join("\n") + "\n";
    Ok((new != text).then_some(new))
}

/// Whether the TOML `line` sets `key` to `value`.
fn toml_has(line: &str, key: &str, value: &str) -> bool {
    rust_app::toml_string(line, key).as_deref() == Some(value)
}

/// The app's own crates (`"rust"`), named by their paths.
fn rust_crates(dir: &Path, rust: &serde_json::Value) -> anyhow::Result<Vec<(String, AppCrate)>> {
    let paths: Vec<String> = match rust {
        serde_json::Value::String(p) => vec![p.clone()],
        serde_json::Value::Array(ps) => ps
            .iter()
            .filter_map(|p| p.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    };
    paths
        .into_iter()
        .map(|path| {
            let crate_dir = dunce::canonicalize(dir.join(&path))
                .with_context(|| format!("nsh.json: \"rust\": no crate at {path}"))?;
            let package = rust_app::package_name(&crate_dir.join("Cargo.toml"))?;
            Ok((path, AppCrate::new(package, crate_dir)))
        })
        .collect()
}

/// `"extensions"` of `config`.
fn sources(config: &serde_json::Value) -> anyhow::Result<Vec<Source>> {
    match config.get("extensions") {
        None => Ok(Vec::new()),
        Some(value) => {
            let sources: Vec<Source> = serde_json::from_value(value.clone()).context(
                "nsh.json: \"extensions\" is a list of { \"git\": \"<url>\", \"tag\" or \"rev\": \"…\" }",
            )?;
            for s in &sources {
                if s.tag.is_some() && s.rev.is_some() {
                    bail!(
                        "nsh.json: \"extensions\": {} has both \"tag\" and \"rev\"",
                        s.git
                    );
                }
            }
            Ok(sources)
        }
    }
}

fn read_lock(dir: &Path) -> anyhow::Result<Lock> {
    match fs::read_to_string(dir.join(LOCK)) {
        Ok(text) => serde_json::from_str(&text).with_context(|| format!("{LOCK} is not valid")),
        Err(_) => Ok(Lock::default()),
    }
}

fn write_lock(dir: &Path, lock: &Lock) -> anyhow::Result<()> {
    let text = serde_json::to_string_pretty(lock)? + "\n";
    fs::write(dir.join(LOCK), text).with_context(|| format!("writing {LOCK}"))
}

/// Resolves `source` to a commit, checks it out and reads its metadata.
fn resolve(source: &Source) -> anyhow::Result<Locked> {
    let repo = repository(&source.git, true)?;
    let reference = source.reference();
    let spec = match (&source.tag, &source.rev) {
        (Some(tag), _) => format!("refs/tags/{tag}^{{commit}}"),
        (_, Some(rev)) => format!("{rev}^{{commit}}"),
        _ => "HEAD^{commit}".to_owned(),
    };
    let commit = git(Some(&repo), &["rev-parse", "--verify", "--quiet", &spec])
        .or_else(|_| {
            git(
                Some(&repo),
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("origin/{reference}^{{commit}}"),
                ],
            )
        })
        .with_context(|| format!("{}: no {reference}", source.git))?;
    let checkout = checkout(&source.git, &commit)?;
    let package = rust_app::package_name(&checkout.join("Cargo.toml"))?;
    let module = check(&checkout, &source.git)?;
    Ok(Locked {
        source: source.clone(),
        commit,
        package,
        module,
    })
}

/// The directory of the extensions from `url` in nsh's cache.
fn cache_of(url: &str) -> anyhow::Result<PathBuf> {
    let hash = Sha256::digest(url.as_bytes());
    let name: String = hash[..8].iter().map(|b| format!("{b:02x}")).collect();
    let last = url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("repo")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();
    Ok(crate::check::cache()?
        .join("extensions")
        .join(format!("{last}-{name}")))
}

/// A clone of `url` in the cache, fetched again when `fetch`.
fn repository(url: &str, fetch: bool) -> anyhow::Result<PathBuf> {
    let repo = cache_of(url)?.join("repo");
    if repo.join(".git").is_dir() {
        if fetch {
            git(
                Some(&repo),
                &["fetch", "--quiet", "--tags", "--force", "origin"],
            )?;
            git(Some(&repo), &["remote", "set-head", "origin", "--auto"]).ok();
        }
    } else {
        fs::create_dir_all(repo.parent().unwrap())?;
        let _ = fs::remove_dir_all(&repo);
        git(
            None,
            &[
                "clone",
                "--quiet",
                "--no-checkout",
                url,
                &repo.to_string_lossy(),
            ],
        )
        .with_context(|| format!("cloning {url}"))?;
    }
    Ok(repo)
}

/// The checkout of `commit` of `url` in the cache, made when missing (from
/// a clone fetched again when it lacks the commit).
fn checkout(url: &str, commit: &str) -> anyhow::Result<PathBuf> {
    let dir = cache_of(url)?.join(commit);
    if dir.join("Cargo.toml").is_file() {
        return Ok(dir);
    }
    let mut repo = repository(url, false)?;
    if git(
        Some(&repo),
        &["cat-file", "-e", &format!("{commit}^{{commit}}")],
    )
    .is_err()
    {
        repo = repository(url, true)?;
    }
    let _ = fs::remove_dir_all(&dir);
    git(Some(&repo), &["worktree", "prune"]).ok();
    git(
        Some(&repo),
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            &dir.to_string_lossy(),
            commit,
        ],
    )
    .with_context(|| format!("{url}: no commit {commit}"))?;
    if !dir.join("Cargo.toml").is_file() {
        bail!("{url}: no Cargo.toml at the repository's root: an extension is a Cargo library");
    }
    Ok(dunce::canonicalize(&dir)?)
}

/// Checks the extension in `dir` (from `url`) can link into this NSH: its
/// `[package.metadata.nsh]` against its `#[native_module]`; its module.
fn check(dir: &Path, url: &str) -> anyhow::Result<String> {
    let manifest = fs::read_to_string(dir.join("Cargo.toml"))?;
    let meta = metadata(&manifest);
    let what = format!("{url}: Cargo.toml [package.metadata.nsh]");
    match meta.format {
        Some(FORMAT) => {}
        Some(other) => bail!("{what}: format {other} needs a newer nsh (this one reads {FORMAT})"),
        None => bail!("{what}: no `format = {FORMAT}`: is this an NSH extension?"),
    }
    let module = meta
        .module
        .with_context(|| format!("{what}: no `module`"))?;
    match rust_app::specifier_of(dir) {
        Some(specifier) if specifier == module => {}
        Some(specifier) => {
            bail!("{what}: `module = {module:?}`, but its #[native_module] is {specifier:?}")
        }
        None => bail!("{what}: no #[native_module({module:?})] at the crate's root"),
    }
    if let Some(req) = &meta.nsh {
        let req =
            semver::VersionReq::parse(req).with_context(|| format!("{what}: `nsh = {req:?}`"))?;
        let version = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
        if !req.matches(&version) {
            bail!("{what}: builds with nsh {req}, this is {version}");
        }
    }
    for target in meta.targets.iter().flatten() {
        if !TARGETS.contains(&target.as_str()) {
            bail!(
                "{what}: unknown target {target:?} (one of {})",
                TARGETS.join(", ")
            );
        }
    }
    Ok(module)
}

/// `[package.metadata.nsh]` of the manifest `text`.
fn metadata(text: &str) -> Metadata {
    let mut meta = Metadata::default();
    for (table, line) in rust_app::toml_tables(text) {
        if table != "package.metadata.nsh" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.split(" #").next().unwrap_or_default().trim();
        match key.trim() {
            "format" => meta.format = value.parse().ok(),
            "module" => meta.module = rust_app::toml_string(line, "module"),
            "nsh" => meta.nsh = rust_app::toml_string(line, "nsh"),
            "targets" => {
                meta.targets = Some(
                    value
                        .trim_start_matches('[')
                        .trim_end_matches(']')
                        .split(',')
                        .map(|t| t.trim().trim_matches('"').to_owned())
                        .filter(|t| !t.is_empty())
                        .collect(),
                )
            }
            _ => {}
        }
    }
    meta
}

/// `git` with `args`, in `dir`; its trimmed output.
fn git(dir: Option<&Path>, args: &[&str]) -> anyhow::Result<String> {
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    let out = cmd
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_read() {
        let meta = metadata(
            "[package]\nname = \"acme-ui\"\n\n[package.metadata.nsh]\nformat = 1\nmodule = \"acme/ui\" # its specifier\nnsh = \">=0.6\"\ntargets = [\"macos\", \"linux\"]\n\n[dependencies]\nmodule = \"x\"\n",
        );
        assert_eq!(meta.format, Some(1));
        assert_eq!(meta.module.as_deref(), Some("acme/ui"));
        assert_eq!(meta.nsh.as_deref(), Some(">=0.6"));
        assert_eq!(meta.targets, Some(vec!["macos".into(), "linux".into()]));
    }

    #[test]
    fn the_app_crate_depends_on_the_extension() {
        let manifest = "[package]\nname = \"app\"\n\n[dependencies]\nnsh-core = \"0.6\"\n";
        let new = with_dependency(manifest, "acme-ui", "https://x/ui", "abc")
            .unwrap()
            .unwrap();
        assert!(
            new.contains(
                "[dependencies]\nacme-ui = { git = \"https://x/ui\", rev = \"abc\" }\nnsh-core"
            ),
            "{new}"
        );
        // Again at another commit: the line is replaced.
        let newer = with_dependency(&new, "acme-ui", "https://x/ui", "def")
            .unwrap()
            .unwrap();
        assert!(
            newer.contains("rev = \"def\"") && !newer.contains("abc"),
            "{newer}"
        );
        assert_eq!(newer.matches("acme-ui").count(), 1, "{newer}");
        assert!(
            with_dependency(&newer, "acme-ui", "https://x/ui", "def")
                .unwrap()
                .is_none()
        );
        // Taken from elsewhere: an error, not a second source.
        let other = "[dependencies]\nacme-ui = \"1\"\n";
        assert!(with_dependency(other, "acme-ui", "https://x/ui", "abc").is_err());
    }

    #[test]
    fn sources_are_checked() {
        let config = serde_json::json!({ "extensions": [{ "git": "https://x/a", "tag": "v1" }] });
        assert_eq!(
            sources(&config).unwrap(),
            [Source {
                git: "https://x/a".into(),
                tag: Some("v1".into()),
                rev: None
            }]
        );
        let both = serde_json::json!({ "extensions": [{ "git": "g", "tag": "v1", "rev": "abc" }] });
        assert!(sources(&both).unwrap_err().to_string().contains("both"));
        let unknown = serde_json::json!({ "extensions": [{ "git": "g", "branch": "main" }] });
        assert!(sources(&unknown).is_err());
        assert!(sources(&serde_json::json!({})).unwrap().is_empty());
    }
}
