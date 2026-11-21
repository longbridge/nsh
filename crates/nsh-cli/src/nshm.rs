//! nshm (NSH modules): Rust crates from crates.io, Git or a local
//! directory that export components, modules and classes to the app, as
//! the app's own crates do (`nsh.json` `"rust"`, [`crate::rust_app`]).
//! `nsh add <source>` lists one in `nsh.json`; a crate from crates.io is
//! the default:
//!
//! ```json
//! { "modules": [
//!   "acme-ui@1.2",
//!   "github:acme/chart@v0.3.0",
//!   { "git": "https://git.example.com/ui.git", "rev": "4f2a1c0" },
//!   { "path": "../my-ui" }
//! ] }
//! ```
//!
//! Versions are Cargo version requirements with Cargo's meaning: `"1.2"`
//! is `^1.2`, and `"~1.2"`, `"=1.2.3"`, `">=1.2, <2"` and `"*"` work as in
//! `Cargo.toml`. A Git or path entry may add `"version"` (the object form)
//! and, as Cargo does, the package must be such a version.
//!
//! `nsh.json` says what is acceptable; `nsh add` pins what it resolved to
//! (a crate's exact version, a Git commit) in `nsh.lock`, next to `nsh.json`,
//! which the app commits: a build takes the locked commit, never what a tag
//! or branch points to now. An nshm is an ordinary Cargo library with a
//! `#[native_module("<module>")]` entry at its root, the specifier TSX
//! imports it by. The NSH versions it builds with are its own `nsh-core`
//! dependency requirement, checked against this CLI before anything
//! compiles.
//!
//! An entry's `"features"` (`nsh add nsh-extra --features camera`) are the
//! crate's Cargo features the app turns on: a crate of several modules
//! keeps each behind a feature, so an app links only those it uses. What a
//! crate needs of the packaged app (an `Info.plist` key, a manifest entry)
//! it may declare in its `Cargo.toml`, under `[package.metadata.nsh]` or a
//! feature's `[package.metadata.nsh.features.<name>]`; `nsh` merges what
//! the enabled features declare into the app's ([`crate::module_meta`]).
//!
//! Each locked commit is checked out once in nsh's cache
//! (`<cache>/nshm/<repository>/<commit>`) and linked into the app's
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

/// An nshm as `nsh.json` lists it: from Git (`git`, with `tag` or
/// `rev`), from crates.io (`crate`, with a `version` requirement) or from
/// a local directory (`path`, relative to `nsh.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    /// The repository's URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    /// A tag to take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// A commit (or branch) to take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    /// The crate's name on crates.io.
    #[serde(rename = "crate", default, skip_serializing_if = "Option::is_none")]
    pub krate: Option<String>,
    /// The versions of the crate to take (a Cargo version requirement).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// A local directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The crate's Cargo features to turn on: the modules of it the app
    /// uses (`nsh add nsh-extra --features camera`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
}

impl Source {
    /// The reference to resolve: the tag, the rev, else the default branch.
    fn reference(&self) -> &str {
        self.tag
            .as_deref()
            .or(self.rev.as_deref())
            .unwrap_or("HEAD")
    }

    /// What names it: its URL, crate or path. Two sources with the same
    /// one are the same nshm.
    pub(crate) fn id(&self) -> &str {
        self.git
            .as_deref()
            .or(self.krate.as_deref())
            .or(self.path.as_deref())
            .unwrap_or_default()
    }

    /// The same source without its features: what `nsh.lock` keys its
    /// entries by, so turning a feature on resolves nothing again.
    fn place(&self) -> Source {
        Source {
            features: Vec::new(),
            ..self.clone()
        }
    }

    /// The same source with `features` on, each once, in order.
    pub(crate) fn with_features(mut self, features: Vec<String>) -> anyhow::Result<Self> {
        for feature in features {
            if !self.features.contains(&feature) {
                self.features.push(feature);
            }
        }
        self.validate()?;
        Ok(self)
    }

    /// Checks it names exactly one place.
    fn validate(&self) -> anyhow::Result<()> {
        let kinds = [&self.git, &self.krate, &self.path]
            .iter()
            .filter(|k| k.is_some())
            .count();
        if kinds != 1 {
            bail!("nsh.json: \"modules\": each has one of \"git\", \"crate\" or \"path\"");
        }
        if self.tag.is_some() && self.rev.is_some() {
            bail!(
                "nsh.json: \"modules\": {} has both \"tag\" and \"rev\"",
                self.id()
            );
        }
        if self.git.is_none() && (self.tag.is_some() || self.rev.is_some()) {
            bail!(
                "nsh.json: \"modules\": {}: \"tag\" and \"rev\" are for \"git\"",
                self.id()
            );
        }
        if let Some(v) = &self.version {
            semver::VersionReq::parse(v).with_context(|| {
                format!(
                    "nsh.json: \"modules\": {}: {v:?} is not a version requirement",
                    self.id()
                )
            })?;
        }
        for feature in &self.features {
            if !is_feature_name(feature) {
                bail!(
                    "nsh.json: \"modules\": {}: {feature:?} is not a Cargo feature's name",
                    self.id()
                );
            }
        }
        Ok(())
    }

    /// A source written as a string, in `nsh.json` or to `nsh add`: a
    /// crate by default (`acme-ui`, `acme-ui@1.2`), `github:<owner>/<repo>`
    /// (`@<tag>`), or a Git URL. `tag`/`rev` (from `--tag`/`--rev`) apply to
    /// Git only.
    pub(crate) fn short(s: &str, tag: Option<String>, rev: Option<String>) -> anyhow::Result<Self> {
        let source = if let Some(repo) = s.strip_prefix("github:") {
            let (repo, at) = match repo.split_once('@') {
                Some((repo, t)) => (repo, Some(t.to_owned())),
                None => (repo, None),
            };
            if repo.split('/').count() != 2 || repo.split('/').any(str::is_empty) {
                bail!("{s}: write github:<owner>/<repo>");
            }
            if at.is_some() && (tag.is_some() || rev.is_some()) {
                bail!("{s}: give the tag once");
            }
            Source {
                git: Some(github_url(repo)),
                tag: at.or(tag),
                rev,
                ..Default::default()
            }
        } else if s.contains("://") || s.starts_with("git@") || s.ends_with(".git") {
            Source {
                git: Some(s.to_owned()),
                tag,
                rev,
                ..Default::default()
            }
        } else {
            let (name, version) = match s.split_once('@') {
                Some((name, v)) => (name.to_owned(), Some(v.to_owned())),
                None => (s.to_owned(), None),
            };
            if let Some(v) = &version {
                semver::VersionReq::parse(v)
                    .with_context(|| format!("{name}@{v}: not a version requirement"))?;
            }
            if tag.is_some() || rev.is_some() {
                bail!("--tag and --rev are for Git; take a crate's version with {name}@<version>");
            }
            Source {
                krate: Some(name),
                version,
                ..Default::default()
            }
        };
        source.validate()?;
        Ok(source)
    }

    /// The source `nsh add` was given: [`Source::short`], or `path`.
    pub(crate) fn parse(
        source: Option<String>,
        path: Option<String>,
        tag: Option<String>,
        rev: Option<String>,
    ) -> anyhow::Result<Self> {
        match (source, path) {
            (None, Some(path)) => Ok(Source {
                // Written with a separator, never read as a crate's name.
                path: Some(if path.contains(['/', '\\']) || path.starts_with('.') {
                    path
                } else {
                    format!("./{path}")
                }),
                ..Default::default()
            }),
            (Some(s), None) => Source::short(&s, tag, rev),
            _ => bail!(
                "nsh add takes a crate name, github:<owner>/<repo>, a Git URL or --path <dir>"
            ),
        }
    }

    /// How `nsh.json` lists it: a string when it can be one.
    fn entry(&self) -> serde_json::Value {
        use serde_json::{Value, json};
        if !self.features.is_empty() {
            // Only the object form has room for features.
            let mut o = self.object();
            o["features"] = json!(self.features);
            return o;
        }
        if let Some(name) = &self.krate {
            return Value::String(match &self.version {
                Some(v) => format!("{name}@{v}"),
                None => name.clone(),
            });
        }
        if let Some(path) = &self.path {
            let mut o = json!({ "path": path });
            if let Some(v) = &self.version {
                o["version"] = json!(v);
            }
            return o;
        }
        let url = self.git.clone().unwrap_or_default();
        let github = github_repo(&url);
        if let Some(v) = &self.version {
            // A version requirement on Git: only the object form carries it.
            let mut o = match github {
                Some(repo) => json!({ "github": repo }),
                None => json!({ "git": url }),
            };
            if let Some(t) = &self.tag {
                o["tag"] = json!(t);
            }
            if let Some(r) = &self.rev {
                o["rev"] = json!(r);
            }
            o["version"] = json!(v);
            return o;
        }
        match (github, &self.tag, &self.rev) {
            (Some(repo), None, None) => Value::String(format!("github:{repo}")),
            (Some(repo), Some(tag), None) => Value::String(format!("github:{repo}@{tag}")),
            (Some(repo), _, Some(rev)) => json!({ "github": repo, "rev": rev }),
            (None, tag, rev) => {
                let mut o = json!({ "git": url });
                if let Some(t) = tag {
                    o["tag"] = json!(t);
                }
                if let Some(r) = rev {
                    o["rev"] = json!(r);
                }
                o
            }
        }
    }

    /// The object form of its entry: `{ "crate" | "git" | "github" |
    /// "path": ..., "version", "tag", "rev" }`, without its features.
    fn object(&self) -> serde_json::Value {
        use serde_json::json;
        let mut o = if let Some(name) = &self.krate {
            json!({ "crate": name })
        } else if let Some(path) = &self.path {
            json!({ "path": path })
        } else {
            let url = self.git.clone().unwrap_or_default();
            match github_repo(&url) {
                Some(repo) => json!({ "github": repo }),
                None => json!({ "git": url }),
            }
        };
        if let Some(t) = &self.tag {
            o["tag"] = json!(t);
        }
        if let Some(r) = &self.rev {
            o["rev"] = json!(r);
        }
        if let Some(v) = &self.version {
            o["version"] = json!(v);
        }
        o
    }

    /// Reads an entry of `nsh.json` `"modules"`.
    fn from_entry(value: &serde_json::Value) -> anyhow::Result<Self> {
        if let Some(s) = value.as_str() {
            return Source::short(s, None, None);
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Full {
            #[serde(rename = "crate")]
            krate: Option<String>,
            git: Option<String>,
            github: Option<String>,
            tag: Option<String>,
            rev: Option<String>,
            path: Option<String>,
            version: Option<String>,
            #[serde(default)]
            features: Vec<String>,
        }
        let full: Full = serde_json::from_value(value.clone()).context(
            "nsh.json: \"modules\": each is \"<crate>[@<version>]\", \"github:<owner>/<repo>[@<tag>]\", { \"crate\": ..., \"version\" }, { \"git\" | \"github\": ..., \"tag\" | \"rev\" } or { \"path\": \"<dir>\" }, each object with optional \"features\"",
        )?;
        if full.git.is_some() && full.github.is_some() {
            bail!("nsh.json: \"modules\": an entry has both \"git\" and \"github\"");
        }
        let source = Source {
            git: full.git.or(full.github.as_deref().map(github_url)),
            tag: full.tag,
            rev: full.rev,
            path: full.path,
            krate: full.krate,
            version: full.version,
            features: Vec::new(),
        };
        let source = source.with_features(full.features)?;
        source.validate()?;
        Ok(source)
    }
}

/// Whether `name` can be a Cargo feature's: letters, digits, `_`, `-`,
/// `+` and `.`, starting with a letter, a digit or `_`.
fn is_feature_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '.'))
}

/// `https://github.com/<repo>`.
fn github_url(repo: &str) -> String {
    format!("https://github.com/{repo}")
}

/// `<owner>/<repo>` of a `https://github.com/<owner>/<repo>` URL.
fn github_repo(url: &str) -> Option<&str> {
    let repo = url.strip_prefix("https://github.com/")?;
    (repo.split('/').count() == 2 && !repo.ends_with(".git")).then_some(repo)
}

/// `nsh.lock`: the commit each nshm resolved to.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Lock {
    version: u64,
    #[serde(default)]
    modules: Vec<Locked>,
}

/// An nshm's entry in `nsh.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Locked {
    #[serde(flatten)]
    source: Source,
    /// The commit a Git source resolved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    /// The version a crate resolved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolved: Option<String>,
    package: String,
    module: String,
}

impl Locked {
    /// Where it was resolved to, briefly.
    fn at(&self) -> String {
        match (&self.commit, &self.resolved) {
            (Some(c), _) => c[..c.len().min(12)].to_owned(),
            (_, Some(v)) => v.clone(),
            _ => "the local directory".to_owned(),
        }
    }
}

/// The nshm `config` (the `nsh.json` in `dir`) lists, each checked
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
    let mut crates = Vec::new();
    for source in &sources {
        if let Some(path) = &source.path {
            // A local directory: read as it is now, nothing to lock.
            let checkout = dunce::canonicalize(dir.join(path))
                .with_context(|| format!("nsh.json: \"modules\": no directory {path}"))?;
            let package = rust_app::package_name(&checkout.join("Cargo.toml"))?;
            check(&checkout, path)?;
            check_version(&checkout, source)?;
            let krate = AppCrate::new(package, checkout).with_features(source.features.clone());
            crates.push((path.clone(), krate));
            continue;
        }
        let key = source.place();
        let entry = match lock.modules.iter().find(|l| l.source == key) {
            Some(entry) => entry.clone(),
            None => resolve(&key)?,
        };
        let checkout = place(&entry)?;
        let package = rust_app::package_name(&checkout.join("Cargo.toml"))?;
        check(&checkout, source.id())?;
        check_version(&checkout, source)?;
        let krate = AppCrate::new(package, checkout).with_features(source.features.clone());
        crates.push((source.id().to_owned(), krate));
        locked.push(entry);
    }
    let new = Lock {
        version: 1,
        modules: locked,
    };
    if new.modules != lock.modules {
        write_lock(dir, &new)?;
    }
    Ok(crates)
}

/// The `[patch]` table that sends the app's dependency on the nshm
/// named `id` (as [`crates`] names it) to its checkout: `Some(Some(url))`
/// for Git, `Some(None)` for crates.io, `None` for a directory, which the
/// app's crate takes as it is.
pub(crate) fn patch_table(id: &str) -> Option<Option<&str>> {
    if id.contains("://") || id.starts_with("git@") || id.ends_with(".git") {
        Some(Some(id))
    } else if id.contains(['/', '\\', '.']) {
        None
    } else {
        Some(None)
    }
}

/// The directory of the locked `entry`: a Git checkout or the crate as
/// Cargo downloaded it.
fn place(entry: &Locked) -> anyhow::Result<PathBuf> {
    match (
        &entry.source.git,
        &entry.commit,
        &entry.source.krate,
        &entry.resolved,
    ) {
        (Some(git), Some(commit), _, _) => checkout(git, commit),
        (_, _, Some(name), Some(version)) => Ok(registry(name, &format!("={version}"))?.1),
        _ => bail!(
            "{LOCK}: {} is not resolved: run nsh add again",
            entry.source.id()
        ),
    }
}

/// The app `nsh add` run in `dir` adds to: the nearest directory, `dir`
/// or one above it, with `nsh.json` (as `cargo add` finds `Cargo.toml`).
fn app_dir(dir: &Path) -> anyhow::Result<PathBuf> {
    dir.ancestors()
        .find(|d| d.join("nsh.json").is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "nsh add adds an nshm to an NSH app, but neither {} nor a directory above it \
                 has nsh.json; run it in the app's directory, or create an app first with \
                 `nsh init`",
                dir.display()
            )
        })
}

/// `nsh add`: lists the nshm at `git` (`tag` or `rev`, else the
/// default branch) in the `nsh.json` in `dir`, replacing one of the same
/// repository, locks it, and checks it with the app's crates. On an error
/// `nsh.json` and `nsh.lock` are left as they were.
pub(crate) fn add(dir: &Path, source: Source) -> anyhow::Result<()> {
    let dir = &app_dir(dir)?;
    let config_path = dir.join("nsh.json");
    let lock_path = dir.join(LOCK);
    let config_text = fs::read_to_string(&config_path)
        .with_context(|| format!("can't read {}", config_path.display()))?;
    let lock_text = fs::read_to_string(&lock_path).ok();
    let mut config: serde_json::Value =
        serde_json::from_str(&config_text).context("nsh.json is not JSON")?;
    let id = source.id().to_owned();
    let mut manifest_backup: Option<(PathBuf, String)> = None;
    let asked = source.features.clone();
    let result = (|| {
        // `nsh add` again: resolved anew, what the reference points to now.
        if let Ok(mut lock) = read_lock(dir)
            && lock.modules.iter().any(|l| l.source.id() == id)
        {
            lock.modules.retain(|l| l.source.id() != id);
            write_lock(dir, &lock)?;
        }
        let mut sources = sources(&config)?;
        // `--features` turns features on: those listed already stay.
        let source = match sources.iter().find(|s| s.id() == id) {
            Some(listed) => Source {
                features: listed.features.clone(),
                ..source.clone()
            }
            .with_features(source.features.clone())?,
            None => source.clone(),
        };
        sources.retain(|s| s.id() != id);
        sources.push(source.clone());
        config["modules"] = serde_json::Value::Array(sources.iter().map(Source::entry).collect());
        fs::write(&config_path, serde_json::to_string_pretty(&config)? + "\n")?;
        // The app's crates and its nshm together: a clash of
        // packages or modules is reported here, before a build.
        let modules = crates(dir, &config)?;
        let mut all: Vec<(String, AppCrate)> = match config.get("rust") {
            Some(rust) => rust_crates(dir, rust)?,
            None => Vec::new(),
        };
        let app_crate = all.first().map(|(_, c)| c.dir().to_owned());
        let added = modules
            .iter()
            .find(|(name, _)| *name == id)
            .map(|(_, c)| c.clone())
            .context("the nshm is not listed")?;
        let known = crate_features(added.dir())?;
        if let Some(unknown) = asked.iter().find(|f| !known.contains(f)) {
            bail!(
                "`{unknown}` is not a feature of {id} (its features: {})",
                if known.is_empty() {
                    "none".to_owned()
                } else {
                    known.join(", ")
                }
            );
        }
        all.extend(modules);
        rust_app::check_crates(&all)?;
        let lock = read_lock(dir)?;
        let place = source.place();
        let entry = lock.modules.iter().find(|l| l.source == place).cloned();
        // The app's crate uses it from Rust too (`use acme_ui::Gauge`).
        if let Some(crate_dir) = &app_crate {
            let manifest = crate_dir.join("Cargo.toml");
            let package = rust_app::package_name(&added.dir().join("Cargo.toml"))?;
            let place = match &entry {
                Some(Locked {
                    commit: Some(commit),
                    ..
                }) => Place::Git {
                    url: id.clone(),
                    commit: commit.clone(),
                },
                Some(Locked {
                    resolved: Some(version),
                    ..
                }) => Place::Crate {
                    version: version.clone(),
                },
                _ => Place::Path {
                    path: relative(crate_dir, added.dir()),
                },
            };
            let dependency = Dependency {
                place,
                features: source.features.clone(),
            };
            let text = fs::read_to_string(&manifest)?;
            manifest_backup = Some((manifest.clone(), text.clone()));
            if let Some(new) = with_dependency(&text, &package, &dependency)? {
                fs::write(&manifest, new)?;
            }
        }
        let module = check(added.dir(), &id)?;
        let at = entry
            .map(|e| e.at())
            .unwrap_or_else(|| "the local directory".into());
        anyhow::Ok((module, at))
    })();
    match result {
        Ok((module, at)) => {
            status("Added", format!("{id} at {at}"));
            println!("import from \"{module}\"");
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
            Err(e.context(format!("nsh add {id}: nsh.json and {LOCK} are unchanged")))
        }
    }
}

/// How the app's crate depends on an nshm: from where, with which
/// features.
#[derive(Debug)]
struct Dependency {
    place: Place,
    features: Vec<String>,
}

/// Where the app's crate takes an nshm from.
#[derive(Debug, Clone)]
enum Place {
    Git { url: String, commit: String },
    Crate { version: String },
    Path { path: String },
}

impl Dependency {
    /// Its value in `[dependencies]`, without the crate's default features
    /// when `default_features` is false.
    fn value_with(&self, default_features: bool) -> String {
        let mut features = String::new();
        if !default_features {
            features.push_str(", default-features = false");
        }
        features.push_str(&rust_app::features_value(&self.features));
        match &self.place {
            Place::Git { url, commit } => {
                format!("{{ git = {url:?}, rev = {commit:?}{features} }}")
            }
            Place::Crate { version } if features.is_empty() => format!("{version:?}"),
            Place::Crate { version } => format!("{{ version = {version:?}{features} }}"),
            Place::Path { path } => format!("{{ path = {path:?}{features} }}"),
        }
    }

    /// Whether the dependency `line` takes the crate from the same place.
    fn same_place(&self, line: &str) -> bool {
        let git = rust_app::toml_string(line, "git");
        let path = rust_app::toml_string(line, "path");
        match &self.place {
            Place::Git { url, .. } => git.as_deref() == Some(url.as_str()),
            Place::Crate { .. } => git.is_none() && path.is_none(),
            Place::Path { .. } => path.is_some(),
        }
    }
}

/// The text after `key =` in the TOML `line`, `key` being a whole key
/// (`features` is not the end of `default-features`).
fn toml_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(found) = line[from..].find(key) {
        let at = from + found;
        from = at + key.len();
        let before = line[..at].trim_end();
        if !(before.is_empty() || before.ends_with('{') || before.ends_with(',')) {
            continue;
        }
        if let Some(value) = line[at + key.len()..].trim_start().strip_prefix('=') {
            let value = value.trim_start();
            // An array runs to its `]`; a scalar to the next `,` or `}`.
            return Some(if value.starts_with('[') {
                value
            } else {
                value
                    .split([',', '}'])
                    .next()
                    .unwrap_or_default()
                    .trim_end()
            });
        }
    }
    None
}

/// The features of the crate in `dir`, as Cargo reads them: its
/// `[features]` and the optional dependencies no feature names as
/// `dep:<name>`, in whichever form its manifest writes them.
fn crate_features(dir: &Path) -> anyhow::Result<Vec<String>> {
    let manifest = dir.join("Cargo.toml");
    let output = Command::new(rust_app::cargo())
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .context("run cargo metadata (is Rust installed?)")?;
    if !output.status.success() {
        bail!(
            "cargo cannot read {}:\n{}",
            manifest.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("cargo metadata's output")?;
    let package = rust_app::package_name(&manifest)?;
    Ok(features_of(&metadata, &package))
}

/// The features of `package` in the `cargo metadata` output `metadata`
/// (a crate of a workspace lists the workspace's packages), sorted.
fn features_of(metadata: &serde_json::Value, package: &str) -> Vec<String> {
    metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"].as_str() == Some(package))
        .and_then(|p| p["features"].as_object())
        .map(|features| {
            let mut names: Vec<String> = features.keys().cloned().collect();
            names.sort_unstable();
            names
        })
        .unwrap_or_default()
}

/// The strings of the inline array `key = [...]` in the TOML `line`.
fn toml_strings(line: &str, key: &str) -> Vec<String> {
    let Some(list) = toml_value(line, key).and_then(|v| v.strip_prefix('[')) else {
        return Vec::new();
    };
    let list = list.split(']').next().unwrap_or_default();
    list.split(',')
        .map(|item| item.trim().trim_matches(['"', '\'']).to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

/// `to` relative to `from`, with `/`.
fn relative(from: &Path, to: &Path) -> String {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); from.len() - common];
    parts.extend(
        to[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

/// The manifest `text` depending on `package` as `dependency` says, or
/// `None` when it does already; an error when it takes `package` from
/// elsewhere.
fn with_dependency(
    text: &str,
    package: &str,
    dependency: &Dependency,
) -> anyhow::Result<Option<String>> {
    // Features the manifest turns on already stay on, and so does turning
    // the default ones off.
    let mut features = dependency.features.clone();
    let mut default_features = true;
    let mut table = String::new();
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            table = t.to_owned();
        }
        if table == "[dependencies]"
            && t.split(['=', ' ']).next() == Some(package)
            && dependency.same_place(t)
        {
            for feature in toml_strings(t, "features") {
                if !features.contains(&feature) {
                    features.push(feature);
                }
            }
            if toml_value(t, "default-features") == Some("false") {
                default_features = false;
            }
        }
    }
    let dependency = &Dependency {
        place: dependency.place.clone(),
        features,
    };
    let line = format!("{package} = {}", dependency.value_with(default_features));
    let mut out = Vec::new();
    let mut found = false;
    let mut table = String::new();
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            table = t.to_owned();
        }
        if table == "[dependencies]" && t.split(['=', ' ']).next() == Some(package) {
            if dependency.same_place(t) {
                // The same place: as now, written under the table's header.
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

/// `"modules"` of `config`.
fn sources(config: &serde_json::Value) -> anyhow::Result<Vec<Source>> {
    // `"extensions"` was this list's name before any release: an app that
    // still has it must not build without its nshm.
    if config.get("extensions").is_some() {
        bail!("nsh.json: \"extensions\" is now \"modules\"");
    }
    match config.get("modules") {
        None => Ok(Vec::new()),
        Some(value) => {
            let list = value.as_array().context(
                "nsh.json: \"modules\" is a list, e.g. [\"acme-ui@1.2\", \"github:acme/ui\"]",
            )?;
            let sources = list
                .iter()
                .map(Source::from_entry)
                .collect::<anyhow::Result<Vec<_>>>()?;
            for s in &sources {
                s.validate()?;
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

/// Resolves `source` (from Git or crates.io) and reads its metadata.
/// A Cargo-style status line on stderr: `    Updating crates.io index`.
fn status(verb: &str, what: impl std::fmt::Display) {
    crate::ui::plain(format!(
        "{} {what}",
        crate::ui::green(format!("{verb:>12}"))
    ));
}

fn resolve(source: &Source) -> anyhow::Result<Locked> {
    if let Some(name) = &source.krate {
        let req = source.version.as_deref().unwrap_or("*");
        status("Updating", "crates.io index");
        status("Downloading", format!("{name} {req}"));
        let (version, dir) = registry(name, req)?;
        status("Downloaded", format!("{name} v{version}"));
        let package = rust_app::package_name(&dir.join("Cargo.toml"))?;
        let module = check(&dir, name)?;
        return Ok(Locked {
            source: source.clone(),
            commit: None,
            resolved: Some(version),
            package,
            module,
        });
    }
    let url = source.git.as_deref().context("nothing to resolve")?;
    status("Updating", format!("git repository `{url}`"));
    let repo = repository(url, true)?;
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
        .with_context(|| format!("{url}: no {reference}"))?;
    status(
        "Locking",
        format!("{url}#{}", &commit[..commit.len().min(9)]),
    );
    let checkout = checkout(url, &commit)?;
    let package = rust_app::package_name(&checkout.join("Cargo.toml"))?;
    let module = check(&checkout, url)?;
    Ok(Locked {
        source: source.clone(),
        commit: Some(commit),
        resolved: None,
        package,
        module,
    })
}

/// The crate `name` from crates.io, its newest version matching `req` as
/// Cargo resolves it: that version and the directory Cargo downloaded it
/// to.
fn registry(name: &str, req: &str) -> anyhow::Result<(String, PathBuf)> {
    let probe = crate::check::cache()?.join("modules").join("registry");
    fs::create_dir_all(&probe)?;
    fs::write(
        probe.join("Cargo.toml"),
        format!(
            "[package]\nname = \"nshm-probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n\n[lib]\npath = \"lib.rs\"\n\n[workspace]\n\n[dependencies]\n{name} = {req:?}\n"
        ),
    )?;
    fs::write(probe.join("lib.rs"), "")?;
    // Resolved anew each time: the requirement decides, not a past lock.
    let _ = fs::remove_file(probe.join("Cargo.lock"));
    let out = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(probe.join("Cargo.toml"))
        .output()
        .context("running cargo")?;
    if !out.status.success() {
        bail!(
            "{name} {req} from crates.io: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let package = meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| {
            p["name"] == name
                && p["source"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("registry+"))
        })
        .with_context(|| format!("{name}: not on crates.io"))?;
    let version = package["version"].as_str().unwrap_or_default().to_owned();
    let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap_or_default());
    let dir = manifest.parent().context("no crate directory")?.to_owned();
    Ok((version, dunce::canonicalize(dir)?))
}

/// The directory of the nshm from `url` in nsh's cache.
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
        .join("modules")
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
        bail!("{url}: no Cargo.toml at the repository's root: an nshm is a Cargo library");
    }
    Ok(dunce::canonicalize(&dir)?)
}

/// Checks the nshm in `dir` (from `url`) can link into this NSH:
/// a `#[native_module]` at its root, and its `nsh-core` requirement (if
/// any) matching this NSH. Its module.
fn check(dir: &Path, url: &str) -> anyhow::Result<String> {
    let manifest = fs::read_to_string(dir.join("Cargo.toml"))?;
    let module = rust_app::specifier_of(dir).with_context(|| {
        format!("{url}: no #[native_module(\"...\")] at the crate's root: is this an nshm?")
    })?;
    if let Some(req) = nsh_requirement(&manifest) {
        let req = semver::VersionReq::parse(&req)
            .with_context(|| format!("{url}: Cargo.toml: nsh-core = {req:?}"))?;
        let version = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
        if !req.matches(&version) {
            bail!("{url}: depends on nsh-core {req}, this is nsh {version}");
        }
    }
    Ok(module)
}

/// The version requirement of `nsh-core` in the `[dependencies]` of the
/// manifest `text` (`nsh-core = "0.6"` or `nsh-core = { version = "0.6", .. }`).
fn nsh_requirement(text: &str) -> Option<String> {
    for (table, line) in rust_app::toml_tables(text) {
        if table != "dependencies" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "nsh-core" {
            continue;
        }
        let value = value.trim();
        return if value.starts_with('{') {
            rust_app::toml_string(value, "version")
        } else {
            Some(
                value
                    .split(" #")
                    .next()?
                    .trim()
                    .trim_matches('"')
                    .to_owned(),
            )
        };
    }
    None
}

/// Checks the package in `dir` is a version `source` requires (its
/// `"version"`, a Cargo version requirement), as Cargo checks a Git or
/// path dependency's `version`. A crate from crates.io is resolved to one.
fn check_version(dir: &Path, source: &Source) -> anyhow::Result<()> {
    let Some(req) = &source.version else {
        return Ok(());
    };
    let req = semver::VersionReq::parse(req)?;
    let manifest = fs::read_to_string(dir.join("Cargo.toml"))?;
    let version = package_version(&manifest)
        .with_context(|| format!("{}: Cargo.toml has no [package] version", source.id()))?;
    let version = semver::Version::parse(&version)
        .with_context(|| format!("{}: version {version:?}", source.id()))?;
    if !req.matches(&version) {
        bail!(
            "nsh.json: \"modules\": {} requires version {req}, but it is {version}",
            source.id()
        );
    }
    Ok(())
}

/// The `[package]` version of the manifest `text`.
fn package_version(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package && line.starts_with("version") {
            return rust_app::toml_string(line, "version");
        }
    }
    None
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
    fn nsh_requirement_is_read() {
        let req = |t: &str| nsh_requirement(t);
        assert_eq!(
            req("[dependencies]\nnsh-core = \"0.6\"\n").as_deref(),
            Some("0.6")
        );
        assert_eq!(
            req("[dependencies]\nnsh-core = { version = \">=0.6, <0.8\", features = [\"x\"] }\n")
                .as_deref(),
            Some(">=0.6, <0.8")
        );
        assert_eq!(req("[dev-dependencies]\nnsh-core = \"0.6\"\n"), None);
        assert_eq!(
            req("[dependencies]\nnsh-core = { path = \"../core\" }\n"),
            None
        );
    }

    #[test]
    fn the_app_crate_depends_on_the_nshm() {
        let manifest = "[package]\nname = \"app\"\n\n[dependencies]\nnsh-core = \"0.6\"\n";
        let at = |commit: &str| Dependency {
            place: Place::Git {
                url: "https://x/ui".into(),
                commit: commit.into(),
            },
            features: Vec::new(),
        };
        let new = with_dependency(manifest, "acme-ui", &at("abc"))
            .unwrap()
            .unwrap();
        assert!(
            new.contains(
                "[dependencies]\nacme-ui = { git = \"https://x/ui\", rev = \"abc\" }\nnsh-core"
            ),
            "{new}"
        );
        // Again at another commit: the line is replaced.
        let newer = with_dependency(&new, "acme-ui", &at("def"))
            .unwrap()
            .unwrap();
        assert!(
            newer.contains("rev = \"def\"") && !newer.contains("abc"),
            "{newer}"
        );
        assert_eq!(newer.matches("acme-ui").count(), 1, "{newer}");
        assert!(
            with_dependency(&newer, "acme-ui", &at("def"))
                .unwrap()
                .is_none()
        );
        // Taken from elsewhere: an error, not a second source.
        let other = "[dependencies]\nacme-ui = \"1\"\n";
        assert!(with_dependency(other, "acme-ui", &at("abc")).is_err());
        // From crates.io and a directory.
        let krate = Dependency {
            place: Place::Crate {
                version: "1.2.0".into(),
            },
            features: Vec::new(),
        };
        let new = with_dependency(manifest, "acme-ui", &krate)
            .unwrap()
            .unwrap();
        assert!(new.contains("acme-ui = \"1.2.0\""), "{new}");
        let path = Dependency {
            place: Place::Path {
                path: "../ui".into(),
            },
            features: Vec::new(),
        };
        assert!(with_dependency(&new, "acme-ui", &path).is_err());
        let new = with_dependency(manifest, "acme-ui", &path)
            .unwrap()
            .unwrap();
        assert!(new.contains("acme-ui = { path = \"../ui\" }"), "{new}");
        // With the features the app turns on.
        let featured = Dependency {
            place: Place::Crate {
                version: "0.6.0".into(),
            },
            features: vec!["camera".into()],
        };
        let new = with_dependency(manifest, "nsh-extra", &featured)
            .unwrap()
            .unwrap();
        assert!(
            new.contains("nsh-extra = { version = \"0.6.0\", features = [\"camera\"] }"),
            "{new}"
        );
        let featured = Dependency {
            place: Place::Git {
                url: "https://x/extra".into(),
                commit: "abc".into(),
            },
            features: vec!["camera".into(), "push".into()],
        };
        // Features the manifest has already stay on.
        let manifest =
            "[dependencies]\nnsh-extra = { version = \"0.6.0\", features = [\"push\"] }\n";
        let more = Dependency {
            place: Place::Crate {
                version: "0.6.0".into(),
            },
            features: vec!["camera".into()],
        };
        let new = with_dependency(manifest, "nsh-extra", &more)
            .unwrap()
            .unwrap();
        assert!(
            new.contains("nsh-extra = { version = \"0.6.0\", features = [\"camera\", \"push\"] }"),
            "{new}"
        );
        assert_eq!(
            toml_strings(r#"x = { git = "u", features = ['a', "b"] }"#, "features"),
            ["a", "b"]
        );
        // `default-features` is not `features`, and stays off.
        let manifest = "[dependencies]\nacme-ui = { version = \"1.0.0\", default-features = false, features = [\"push\"] }\n";
        let camera = Dependency {
            place: Place::Crate {
                version: "1.0.0".into(),
            },
            features: vec!["camera".into()],
        };
        let new = with_dependency(manifest, "acme-ui", &camera)
            .unwrap()
            .unwrap();
        assert!(
            new.contains(
                "acme-ui = { version = \"1.0.0\", default-features = false, features = [\"camera\", \"push\"] }"
            ),
            "{new}"
        );
        assert!(toml_strings("x = { default-features = false }", "features").is_empty());
        assert_eq!(
            featured.value_with(true),
            "{ git = \"https://x/extra\", rev = \"abc\", features = [\"camera\", \"push\"] }"
        );
    }

    #[test]
    fn sources_are_checked() {
        let config = serde_json::json!({ "modules": [
            "acme-ui", "acme-ui@1.2", "github:acme/ui@v1",
            { "github": "acme/ui", "rev": "abc" },
            { "git": "https://x/a", "tag": "v1" },
            { "path": "../ui" },
            { "github": "acme/ui", "tag": "v1", "version": ">=1.2, <2" },
            { "path": "../ui", "version": "~1.2" },
            "acme-ui@=1.2.3"
        ] });
        let list = sources(&config).unwrap();
        assert_eq!(list[6].version.as_deref(), Some(">=1.2, <2"));
        assert_eq!(list[7].version.as_deref(), Some("~1.2"));
        assert_eq!(list[1].krate.as_deref(), Some("acme-ui"));
        assert_eq!(list[1].version.as_deref(), Some("1.2"));
        assert_eq!(list[2].git.as_deref(), Some("https://github.com/acme/ui"));
        assert_eq!(list[2].tag.as_deref(), Some("v1"));
        assert_eq!(list[3].rev.as_deref(), Some("abc"));
        // Written back as it was.
        let back: Vec<_> = list.iter().map(Source::entry).collect();
        assert_eq!(serde_json::Value::Array(back), config["modules"]);
        let both = serde_json::json!({ "modules": [{ "git": "g", "tag": "v1", "rev": "abc" }] });
        assert!(sources(&both).unwrap_err().to_string().contains("both"));
        let unknown = serde_json::json!({ "modules": [{ "git": "g", "branch": "main" }] });
        assert!(sources(&unknown).is_err());
        assert!(sources(&serde_json::json!({})).unwrap().is_empty());
        let krate = serde_json::json!({ "modules": [{ "crate": "a", "version": "1" }] });
        assert_eq!(sources(&krate).unwrap()[0].krate.as_deref(), Some("a"));
        let two = serde_json::json!({ "modules": [{ "github": "a/b", "path": "../a" }] });
        assert!(sources(&two).is_err());
        let bad = serde_json::json!({ "modules": ["github:acme"] });
        assert!(sources(&bad).is_err());
        let req = serde_json::json!({ "modules": [{ "path": "../a", "version": "one" }] });
        assert!(sources(&req).is_err());
        let renamed = serde_json::json!({ "extensions": ["acme-ui@1.2"] });
        assert!(sources(&renamed).is_err());
    }

    #[test]
    fn features_are_listed_and_locked_apart() {
        let config = serde_json::json!({ "modules": [
            { "crate": "nsh-extra", "version": "0.6", "features": ["camera"] },
            { "github": "acme/ui", "tag": "v1", "features": ["charts", "maps"] },
            { "path": "../extra", "features": ["camera"] }
        ] });
        let list = sources(&config).unwrap();
        assert_eq!(list[0].features, ["camera"]);
        assert_eq!(list[1].features, ["charts", "maps"]);
        // Written back as it was: only the object form carries features.
        let back: Vec<_> = list.iter().map(Source::entry).collect();
        assert_eq!(serde_json::Value::Array(back), config["modules"]);
        // `nsh.lock` keys an entry by its place: a feature turned on
        // resolves nothing again.
        assert_eq!(
            list[0].place(),
            Source::short("nsh-extra@0.6", None, None).unwrap()
        );
        // Each feature once, and only names Cargo takes.
        let more = list[0]
            .clone()
            .with_features(vec!["camera".into(), "push".into()])
            .unwrap();
        assert_eq!(more.features, ["camera", "push"]);
        for bad in ["", "a b", "dep:x", "a/b", "-a"] {
            let config = serde_json::json!({ "modules": [{ "crate": "x", "features": [bad] }] });
            assert!(sources(&config).is_err(), "{bad:?}");
        }
        assert!(
            Source::short("nsh-extra", None, None)
                .unwrap()
                .with_features(vec!["dep:tokio".into()])
                .is_err()
        );
    }

    #[test]
    fn versions_follow_cargo() {
        let dir = std::env::temp_dir().join(format!("nsh-ext-version-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"acme-ui\"\nversion = \"1.2.5\"\n",
        )
        .unwrap();
        let with = |v: &str| Source {
            path: Some("../ui".into()),
            version: Some(v.into()),
            ..Default::default()
        };
        // "1.2" is ^1.2, as in Cargo.
        for ok in ["1.2", "^1", "~1.2", ">=1.2, <2", "=1.2.5", "*"] {
            check_version(&dir, &with(ok)).unwrap();
        }
        for no in ["1.3", "=1.2.4", "~1.1", "2"] {
            assert!(check_version(&dir, &with(no)).is_err(), "{no}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_crates_features_are_read_from_its_metadata() {
        // `cargo metadata` lists a workspace's packages, implicit features
        // of optional dependencies included.
        let metadata = serde_json::json!({ "packages": [
            { "name": "acme-core", "features": { "fast": [] } },
            { "name": "acme-ui", "features": {
                "default": [],
                "camera": ["dep:tokio"],
                "serde": ["dep:serde"],
            } },
        ] });
        assert_eq!(
            features_of(&metadata, "acme-ui"),
            ["camera", "default", "serde"]
        );
        assert!(features_of(&metadata, "acme-gauge").is_empty());
        assert!(features_of(&serde_json::json!({}), "acme-ui").is_empty());
    }

    #[test]
    fn add_arguments_are_parsed() {
        let git = Source::parse(
            Some("https://github.com/acme/ui".into()),
            None,
            Some("v1".into()),
            None,
        )
        .unwrap();
        assert_eq!(git.git.as_deref(), Some("https://github.com/acme/ui"));
        let gh =
            Source::parse(Some("github:acme/ui".into()), None, Some("v1".into()), None).unwrap();
        assert_eq!(gh, git);
        assert_eq!(gh.entry(), serde_json::json!("github:acme/ui@v1"));
        assert!(
            Source::parse(
                Some("github:acme/ui@v1".into()),
                None,
                Some("v2".into()),
                None
            )
            .is_err()
        );
        let krate = Source::parse(Some("acme-ui@1.2".into()), None, None, None).unwrap();
        assert_eq!(
            (krate.krate.as_deref(), krate.version.as_deref()),
            (Some("acme-ui"), Some("1.2"))
        );
        assert!(Source::parse(Some("acme-ui".into()), None, Some("v1".into()), None).is_err());
        assert!(Source::parse(Some("acme-ui@nope".into()), None, None, None).is_err());
        let path = Source::parse(None, Some("../ui".into()), None, None).unwrap();
        assert_eq!(path.path.as_deref(), Some("../ui"));
    }

    #[test]
    fn add_finds_the_app_above() {
        let root = std::env::temp_dir().join(format!("nsh-app-dir-{}", std::process::id()));
        let src = root.join("app/src/ui");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("app/nsh.json"), "{}").unwrap();
        assert_eq!(app_dir(&src).unwrap(), root.join("app"));
        let err = app_dir(&root).unwrap_err().to_string();
        assert!(err.contains("nsh init"), "{err}");
        assert!(!err.contains("os error"), "{err}");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn relative_paths() {
        assert_eq!(
            relative(Path::new("/a/app/rust"), Path::new("/a/ext/ui")),
            "../../ext/ui"
        );
        assert_eq!(relative(Path::new("/a"), Path::new("/a/b")), "b");
    }
}
