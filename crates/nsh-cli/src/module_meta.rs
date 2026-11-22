//! What the app's crates and nshm need of the packaged app: `Info.plist`
//! keys and entitlements on iOS, permissions and manifest entries on
//! Android, which a crate declares in its `Cargo.toml`:
//!
//! ```toml
//! [package.metadata.nsh.ios.infoPlist]      # always, when the crate is linked
//! NSExampleUsageDescription = "..."
//!
//! [package.metadata.nsh.features.camera.ios.infoPlist]   # when `camera` is on
//! NSCameraUsageDescription = "The app takes photos with the camera."
//!
//! [package.metadata.nsh.features.camera.android]
//! permissions = ["android.permission.CAMERA"]
//! queries = ['<intent><action android:name="android.media.action.IMAGE_CAPTURE"/></intent>']
//! application = ['<service android:name="..." android:exported="false"/>']
//! ```
//!
//! Every package in the generated workspace's dependency graph is read, so
//! a crate the app's own crate depends on (by path, Git or crates.io) counts
//! as an nshm `nsh add` lists does, with the features Cargo turns on for it.
//! The crates' entries are merged ([`ModuleMeta::from_metadata`]): arrays
//! are joined without repeats, and a key two crates set to different values
//! is a clash. The app's own `nsh.json` wins over all of them
//! ([`ModuleMeta::apply`]): a key it sets keeps its value (arrays joined),
//! which also settles a clash; a clash the app leaves open is an error
//! naming both crates. Keys NSH sets itself are refused, naming the crate.
//!
//! `ios.infoPlist`, `ios.entitlements` (named as in `nsh.json`),
//! `android.permissions` (checked and spelled out as `nsh.json`'s are),
//! `android.queries`
//! (`<intent>`, `<package>` and `<provider>` elements) and
//! `android.application` (`<service>`, `<receiver>`, `<provider>`,
//! `<activity>` and `<meta-data>` elements), and the crate's Android files
//! `android.dex`, `android.res` and `android.jniLibs` (paths in the crate,
//! read by Android packages and hosts only:
//! [`ModuleMeta::resolve_android_files`]) are read; other keys are
//! reported and left out. Each Android entry is one balanced element of
//! those, so a crate cannot close `<application>` or `<queries>` to write
//! elsewhere in the manifest.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};
use serde_json::{Map, Value};

use crate::{package::AppConfig, ui};

/// The elements `android.queries` takes.
const QUERY_ELEMENTS: &[&str] = &["intent", "package", "provider"];

/// The elements `android.application` takes: components and their
/// metadata, never the `<application>` element's own attributes.
const APPLICATION_ELEMENTS: &[&str] = &["service", "receiver", "provider", "activity", "meta-data"];

/// What the crates of an app declare for its packages.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ModuleMeta {
    /// `Info.plist` keys of iOS packages.
    pub(crate) ios_info: Map<String, Value>,
    /// Entitlements of iOS packages.
    pub(crate) ios_entitlements: Map<String, Value>,
    /// Android permissions (`<uses-permission>`).
    pub(crate) android_permissions: Vec<String>,
    /// Elements of the manifest's `<queries>`.
    pub(crate) android_queries: Vec<String>,
    /// Elements of the manifest's `<application>`.
    pub(crate) android_application: Vec<String>,
    /// The Android files the crates name, read by Android packages and
    /// hosts only ([`ModuleMeta::resolve_android_files`]), so a crate's
    /// Android files never stop another platform.
    android_files: Vec<FileDecl>,
    /// Dex files of the crates' Java code, packaged after the app's.
    pub(crate) android_dex: Vec<PathBuf>,
    /// Resource directories, linked below the app's own.
    pub(crate) android_res: Vec<PathBuf>,
    /// Native library directories, each holding `<abi>/*.so`.
    pub(crate) android_jni_libs: Vec<PathBuf>,
    /// Android libraries, `.aar` and `.jar` files
    /// ([`crate::android_libraries`]).
    pub(crate) android_libraries: Vec<PathBuf>,
    /// Which package set each key, for errors.
    owners: BTreeMap<String, String>,
    /// Keys two crates set to different values: the first crate's value is
    /// kept until the app's `nsh.json` settles it ([`ModuleMeta::apply`]).
    clashes: Vec<Clash>,
}

/// The platform a package or development host is made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    /// iOS, its simulator and Mac Catalyst.
    Ios,
    Android,
}

/// A key two crates set to different values.
#[derive(Debug, Clone, PartialEq)]
struct Clash {
    /// `ios.infoPlist` or `ios.entitlements`, the same in `nsh.json`.
    kind: &'static str,
    key: String,
    /// The crate whose value is kept, and the one that disagrees.
    first: String,
    second: String,
}

impl std::fmt::Display for Clash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} and {} set `{}` ({}) to different values: set it in the app's nsh.json `{}` to choose",
            self.first, self.second, self.key, self.kind, self.kind
        )
    }
}

impl ModuleMeta {
    /// What the packages of the generated workspace `manifest` declare, as
    /// Cargo resolves it for `triple` (`None`: every platform).
    pub(crate) fn of_workspace(manifest: &Path, triple: Option<&str>) -> anyhow::Result<Self> {
        let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        cmd.args(["metadata", "--format-version", "1", "--manifest-path"])
            .arg(manifest);
        if let Some(triple) = triple {
            cmd.args(["--filter-platform", triple]);
        }
        let out = cmd
            .output()
            .context("running cargo metadata for the app's crates")?;
        if !out.status.success() {
            bail!(
                "cargo metadata failed for the app's crates:\n{}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let json: Value =
            serde_json::from_slice(&out.stdout).context("cargo metadata printed no JSON")?;
        let (meta, warnings) = Self::from_metadata(&json)?;
        for warning in warnings {
            ui::warn(warning);
        }
        Ok(meta)
    }

    /// What `cargo metadata`'s output `json` declares: each resolved
    /// package's `[package.metadata.nsh]`, its sections for the features
    /// Cargo turned on; and the warnings about what was left out.
    pub(crate) fn from_metadata(json: &Value) -> anyhow::Result<(Self, Vec<String>)> {
        let packages: BTreeMap<&str, &Value> = json["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| Some((p["id"].as_str()?, p)))
            .collect();
        let mut meta = ModuleMeta::default();
        let mut warnings = Vec::new();
        let mut nodes: Vec<&Value> = json["resolve"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        // One order whatever Cargo printed: errors and arrays stay stable.
        nodes.sort_by_key(|n| n["id"].as_str().unwrap_or_default());
        for node in nodes {
            let Some(package) = node["id"].as_str().and_then(|id| packages.get(id)) else {
                continue;
            };
            let Some(nsh) = package["metadata"]["nsh"].as_object() else {
                continue;
            };
            let name = package["name"].as_str().unwrap_or("?");
            // Where the crate's files are: paths in its metadata are
            // relative to its `Cargo.toml`.
            let dir = package["manifest_path"]
                .as_str()
                .and_then(|m| Path::new(m).parent());
            let crate_ = Crate { name, dir };
            meta.merge_section(&crate_, "[package.metadata.nsh]", nsh, &mut warnings)?;
            let features = nsh.get("features").and_then(Value::as_object);
            let enabled = node["features"].as_array().into_iter().flatten();
            for feature in enabled.filter_map(Value::as_str) {
                if let Some(section) = features
                    .and_then(|f| f.get(feature))
                    .and_then(Value::as_object)
                {
                    let at = format!("[package.metadata.nsh.features.{feature}]");
                    meta.merge_section(&crate_, &at, section, &mut warnings)?;
                }
            }
        }
        Ok((meta, warnings))
    }

    /// Merges one section (`at`, of package `name`): its `ios` and
    /// `android` tables.
    fn merge_section(
        &mut self,
        crate_: &Crate<'_>,
        at: &str,
        section: &Map<String, Value>,
        warnings: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        let name = crate_.name;
        for (key, value) in section {
            match key.as_str() {
                // Read by itself: a feature's section is merged only when on.
                "features" if at == "[package.metadata.nsh]" => {}
                "ios" => self.merge_ios(name, at, value, warnings)?,
                "android" => self.merge_android(crate_, at, value, warnings)?,
                _ => warnings.push(format!(
                    "{name}: {at} `{key}` is not something this nsh reads: left out"
                )),
            }
        }
        Ok(())
    }

    fn merge_ios(
        &mut self,
        name: &str,
        at: &str,
        ios: &Value,
        warnings: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        let ios = ios
            .as_object()
            .with_context(|| format!("{name}: {at} `ios` is a table"))?;
        for (key, value) in ios {
            let (target, kind) = match key.as_str() {
                "infoPlist" => (&mut self.ios_info, "ios.infoPlist"),
                "entitlements" => (&mut self.ios_entitlements, "ios.entitlements"),
                _ => {
                    warnings.push(format!(
                        "{name}: {at} `ios.{key}` is not something this nsh reads: left out"
                    ));
                    continue;
                }
            };
            let entries = value
                .as_object()
                .with_context(|| format!("{name}: {at} `{kind}` is a table"))?;
            for (entry, value) in entries {
                let reserved = match kind {
                    "ios.infoPlist" => crate::apple::is_reserved_ios_info(entry),
                    _ => crate::apple::is_reserved_ios_entitlement(entry),
                };
                if reserved {
                    bail!("{name}: {at} `{kind}.{entry}` is a key NSH sets itself");
                }
                let owner = format!("{kind}.{entry}");
                match target.get_mut(entry) {
                    None => {
                        target.insert(entry.clone(), value.clone());
                        self.owners.insert(owner, name.to_owned());
                    }
                    Some(Value::Array(have)) if value.is_array() => {
                        join(have, value.as_array().into_iter().flatten().cloned());
                    }
                    Some(have) if have == value => {}
                    Some(_) => {
                        let first = self
                            .owners
                            .get(&owner)
                            .map_or("another crate", |o| o)
                            .to_owned();
                        self.clashes.push(Clash {
                            kind,
                            key: entry.clone(),
                            first,
                            second: name.to_owned(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn merge_android(
        &mut self,
        crate_: &Crate<'_>,
        at: &str,
        android: &Value,
        warnings: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        let name = crate_.name;
        let android = android
            .as_object()
            .with_context(|| format!("{name}: {at} `android` is a table"))?;
        for (key, value) in android {
            let strings = || -> anyhow::Result<Vec<String>> {
                value
                    .as_array()
                    .and_then(|list| {
                        list.iter()
                            .map(|v| v.as_str().map(str::to_owned))
                            .collect::<Option<Vec<_>>>()
                    })
                    .with_context(|| format!("{name}: {at} `android.{key}` is a list of strings"))
            };
            match key.as_str() {
                "permissions" => {
                    let at = format!("{name}: {at} `android.permissions`");
                    let names = strings()?;
                    warnings.extend(
                        names
                            .iter()
                            .filter_map(|p| crate::package::lower_case_android_warning(p, &at)),
                    );
                    let names = names
                        .iter()
                        .map(|p| crate::package::android_permission(p, &at))
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    join_strings(&mut self.android_permissions, names);
                }
                "queries" => {
                    let elements = strings()?;
                    for e in &elements {
                        check_element(e, QUERY_ELEMENTS)
                            .with_context(|| format!("{name}: {at} `android.queries`"))?;
                    }
                    join_strings(&mut self.android_queries, elements);
                }
                "application" => {
                    let elements = strings()?;
                    for e in &elements {
                        check_element(e, APPLICATION_ELEMENTS)
                            .with_context(|| format!("{name}: {at} `android.application`"))?;
                    }
                    join_strings(&mut self.android_application, elements);
                }
                "dex" | "libraries" => {
                    for path in strings()? {
                        self.android_files.push(crate_.decl(at, key, path));
                    }
                }
                "res" | "jniLibs" => {
                    let path = value
                        .as_str()
                        .with_context(|| format!("{name}: {at} `android.{key}` is a path"))?;
                    self.android_files
                        .push(crate_.decl(at, key, path.to_owned()));
                }
                _ => warnings.push(format!(
                    "{name}: {at} `android.{key}` is not something this nsh reads: left out"
                )),
            }
        }
        Ok(())
    }

    /// Finds and checks the Android files the crates name
    /// (`android.dex`, `android.res`, `android.jniLibs`): each inside its
    /// crate, of its kind; no resource of a name NSH generates; no
    /// resource two crates both declare. Android packages and the Android
    /// host call it; other platforms never read the files.
    pub(crate) fn resolve_android_files(&mut self) -> anyhow::Result<()> {
        // Which crate (by its directory: two versions of one crate are two)
        // declares each resource.
        let mut owners: BTreeMap<String, (&str, Option<&Path>)> = BTreeMap::new();
        for decl in &self.android_files {
            let FileDecl { name, at, key, .. } = decl;
            let path = &decl.path;
            let file = decl.file()?;
            match key.as_str() {
                "libraries" => {
                    let kind = file
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or_default();
                    if !file.is_file() || !matches!(kind, "aar" | "jar") {
                        bail!(
                            "{name}: {at} `android.libraries`: {path} is no `.aar` or `.jar` file"
                        );
                    }
                    if !self.android_libraries.contains(&file) {
                        self.android_libraries.push(file);
                    }
                }
                "dex" => {
                    let mut magic = [0u8; 4];
                    std::io::Read::read_exact(
                        &mut std::fs::File::open(&file)
                            .with_context(|| format!("{name}: {at} `android.dex`: read {path}"))?,
                        &mut magic,
                    )
                    .ok();
                    if &magic != b"dex\n" {
                        bail!(
                            "{name}: {at} `android.dex`: {path} is not a dex file (make one with the SDK's `d8`)"
                        );
                    }
                    if !self.android_dex.contains(&file) {
                        self.android_dex.push(file);
                    }
                }
                _ => {
                    if !file.is_dir() {
                        bail!("{name}: {at} `android.{key}`: {path} is not a directory");
                    }
                    decl.check_inside(&file)?;
                    if key == "res" {
                        for resource in resource_names(&file)
                            .with_context(|| format!("{name}: {at} `android.res`: read {path}"))?
                        {
                            let bare = resource.split_once('/').map_or(&*resource, |(_, n)| n);
                            if RESERVED_RESOURCE_PREFIXES
                                .iter()
                                .any(|p| bare.starts_with(p))
                            {
                                bail!(
                                    "{name}: {at} `android.res`: `{resource}` takes a name of NSH's own resources ({})",
                                    RESERVED_RESOURCE_PREFIXES
                                        .iter()
                                        .map(|p| format!("`{p}…`"))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                );
                            }
                            let owner = (name.as_str(), decl.dir.as_deref());
                            if let Some((other, other_dir)) = owners.insert(resource.clone(), owner)
                                && other_dir != owner.1
                            {
                                bail!(
                                    "{other} and {name} both declare the Android resource `{resource}`: \
                                     only the app's own `android/res` may replace another's"
                                );
                            }
                        }
                        if !self.android_res.contains(&file) {
                            self.android_res.push(file);
                        }
                    } else {
                        for entry in std::fs::read_dir(&file).with_context(|| {
                            format!("{name}: {at} `android.jniLibs`: read {path}")
                        })? {
                            let abi = entry?.file_name().to_string_lossy().into_owned();
                            if !ANDROID_ABIS.contains(&abi.as_str()) && !abi.starts_with('.') {
                                bail!(
                                    "{name}: {at} `android.jniLibs`: {path}/{abi} is no Android ABI ({})",
                                    ANDROID_ABIS.join(", ")
                                );
                            }
                        }
                        if !self.android_jni_libs.contains(&file) {
                            self.android_jni_libs.push(file);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Adds what the crates declare to the app's `config` (its `nsh.json`):
    /// a key the app sets keeps the app's value, its arrays joined with
    /// the crates' entries; permissions are joined. For an iOS `platform`, a
    /// key two crates set to different values that the app does not set is
    /// an error naming both.
    pub(crate) fn apply(&self, config: &mut AppConfig, platform: Platform) -> anyhow::Result<()> {
        // The clashes are all iOS keys: an Android package has none of them.
        if platform == Platform::Ios {
            for clash in &self.clashes {
                let app = match clash.kind {
                    "ios.infoPlist" => &config.ios.info_plist,
                    _ => &config.ios.entitlements,
                };
                if !app.contains_key(&clash.key) {
                    bail!("{clash}");
                }
            }
        }
        under(&mut config.ios.info_plist, &self.ios_info);
        under(&mut config.ios.entitlements, &self.ios_entitlements);
        join_strings(
            &mut config.android.permissions,
            self.android_permissions.clone(),
        );
        join_strings(&mut config.android.queries, self.android_queries.clone());
        join_strings(
            &mut config.android.application,
            self.android_application.clone(),
        );
        for dex in &self.android_dex {
            if !config.android.dex.contains(dex) {
                config.android.dex.push(dex.clone());
            }
        }
        config.android.crate_res.clone_from(&self.android_res);
        config.android.jni_libs.clone_from(&self.android_jni_libs);
        config.android.libraries.clone_from(&self.android_libraries);
        Ok(())
    }
}

/// What the current app's crates declare for a development host built for
/// `triple`: nothing when the app has no crates; a failure is reported and
/// nothing taken, so the session still runs.
pub(crate) fn for_dev_host(triple: &str, platform: Platform) -> ModuleMeta {
    let Some(app) = crate::rust_app::current() else {
        return ModuleMeta::default();
    };
    let meta = ModuleMeta::of_workspace(&app.workspace().join("Cargo.toml"), Some(triple))
        .unwrap_or_else(|e| {
            ui::warn(format!(
                "the development host leaves out what the app's crates declare: {e:#}"
            ));
            ModuleMeta::default()
        });
    // The host is the same for every app: it takes the first crate's value,
    // where a package takes the app's. Only iOS hosts have the keys.
    for clash in meta.clashes.iter().filter(|_| platform == Platform::Ios) {
        ui::warn(format!(
            "{clash}; the development host takes {}'s",
            clash.first
        ));
    }
    meta
}

/// Adds `from` to `app`: the app's value wins, arrays are joined.
pub(crate) fn under(app: &mut Map<String, Value>, from: &Map<String, Value>) {
    for (key, value) in from {
        match app.get_mut(key) {
            None => {
                app.insert(key.clone(), value.clone());
            }
            Some(Value::Array(have)) => {
                join(have, value.as_array().into_iter().flatten().cloned());
            }
            Some(_) => {}
        }
    }
}

/// Appends each of `more` that `list` does not have yet.
fn join(list: &mut Vec<Value>, more: impl IntoIterator<Item = Value>) {
    for value in more {
        if !list.contains(&value) {
            list.push(value);
        }
    }
}

fn join_strings(list: &mut Vec<String>, more: Vec<String>) {
    for value in more {
        if !list.contains(&value) {
            list.push(value);
        }
    }
}

/// The Android ABIs a crate's `jniLibs` may hold.
const ANDROID_ABIS: &[&str] = &["arm64-v8a", "armeabi-v7a", "x86", "x86_64"];

/// A package whose metadata is read: its name, for errors, and its
/// directory, which the paths it names are relative to.
struct Crate<'a> {
    name: &'a str,
    dir: Option<&'a Path>,
}

impl Crate<'_> {
    /// A file or directory the crate names (`android.<key>` in section
    /// `at`), found later by [`ModuleMeta::resolve_android_files`].
    fn decl(&self, at: &str, key: &str, path: String) -> FileDecl {
        FileDecl {
            name: self.name.to_owned(),
            at: at.to_owned(),
            key: key.to_owned(),
            dir: self.dir.map(Path::to_path_buf),
            path,
        }
    }
}

/// A path a crate names in its metadata.
#[derive(Debug, Clone, PartialEq)]
struct FileDecl {
    /// The crate, and where in its metadata (for errors).
    name: String,
    at: String,
    /// `dex`, `res` or `jniLibs`.
    key: String,
    /// The crate's directory, which `path` is relative to.
    dir: Option<PathBuf>,
    path: String,
}

impl FileDecl {
    /// The crate's directory, its links followed.
    fn root(&self) -> anyhow::Result<PathBuf> {
        let FileDecl { name, at, key, .. } = self;
        let dir = self.dir.as_deref().with_context(|| {
            format!("{name}: {at} `android.{key}`: cargo metadata names no directory")
        })?;
        dunce::canonicalize(dir)
            .with_context(|| format!("{name}: {at} `android.{key}`: {}", dir.display()))
    }

    /// The file as a path in the crate's directory: relative, and inside it
    /// once links are followed, so a crate packages only its own files.
    fn file(&self) -> anyhow::Result<PathBuf> {
        let FileDecl {
            name,
            at,
            key,
            path,
            ..
        } = self;
        if Path::new(path).is_absolute() {
            bail!(
                "{name}: {at} `android.{key}`: {path} is absolute; name it relative to the crate's Cargo.toml, with `/`"
            );
        }
        let root = self.root()?;
        let file = dunce::canonicalize(root.join(path))
            .with_context(|| format!("{name}: {at} `android.{key}`: {path} is missing"))?;
        if !file.starts_with(&root) {
            bail!("{name}: {at} `android.{key}`: {path} is outside the crate");
        }
        Ok(file)
    }

    /// Checks every file in the directory `dir` is inside the crate too,
    /// links followed.
    fn check_inside(&self, dir: &Path) -> anyhow::Result<()> {
        let root = self.root()?;
        let FileDecl { name, at, key, .. } = self;
        for entry in walk(dir).with_context(|| format!("{name}: {at} `android.{key}`"))? {
            let real = dunce::canonicalize(&entry).with_context(|| {
                format!(
                    "{name}: {at} `android.{key}`: {} leads nowhere",
                    entry.display()
                )
            })?;
            if !real.starts_with(&root) {
                bail!(
                    "{name}: {at} `android.{key}`: {} leads outside the crate",
                    entry.display()
                );
            }
        }
        Ok(())
    }
}

/// The files under `dir`, dot files (`.DS_Store`) left out. A link to a
/// folder is refused: `aapt2` and the packaging would follow it where the
/// checks of the crate's files could not.
fn walk(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    entries.sort();
    for entry in entries {
        if entry
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let link = std::fs::symlink_metadata(&entry)
            .with_context(|| format!("read {}", entry.display()))?;
        if link.is_dir() {
            files.extend(walk(&entry)?);
        } else if link.file_type().is_symlink() && entry.is_dir() {
            bail!(
                "{} links to a folder; put the folder itself there",
                entry.display()
            );
        } else {
            files.push(entry);
        }
    }
    Ok(files)
}

/// The prefixes of the resource names NSH generates (the launcher icon,
/// the launch screen's theme, color and drawables), which a crate's
/// resources may not take.
const RESERVED_RESOURCE_PREFIXES: &[&str] = &["ic_launcher", "nsh_", "NshLaunch"];

/// The resources a resource directory `res` declares, as `type/name`:
/// each file under `<type>[-<qualifiers>]/` by its name, and each element
/// of a `values*/` file's `<resources>` by its `name` attribute.
fn resource_names(res: &Path) -> anyhow::Result<std::collections::BTreeSet<String>> {
    let mut names = std::collections::BTreeSet::new();
    for file in walk(res)? {
        let Some(folder) = file.parent().and_then(Path::file_name) else {
            continue;
        };
        let folder = folder.to_string_lossy();
        let kind = folder.split('-').next().unwrap_or_default();
        let stem = file
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        // As `aapt2` names it: up to the last dot, `.9.png` whole.
        let stem = stem
            .strip_suffix(".9.png")
            .or_else(|| stem.rsplit_once('.').map(|(name, _)| name))
            .unwrap_or(&stem);
        if kind == "values" {
            let text = std::fs::read_to_string(&file)?;
            names.extend(values_names(&text));
        } else {
            names.insert(format!("{kind}/{stem}"));
        }
    }
    Ok(names)
}

/// The `type/name` of each resource a values file declares: each element
/// directly in its `<resources>` by its `name` (`<string name="a">` is
/// `string/a`, `<item type="id" name="b">` is `id/b`,
/// `<declare-styleable name="c">` is `styleable/c`), an `<attr>` with a
/// `format` inside a `<declare-styleable>` (`attr/…`), and an `<enum>` or
/// `<flag>` of an `<attr>` (`id/…`). A style's or an array's items are
/// their parent's; `<public>`, `<java-symbol>`, `<overlayable>` and
/// `<eat-comment>` declare nothing.
fn values_names(xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    // The open elements, outermost first.
    let mut open: Vec<String> = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        if let Some(after) = rest.strip_prefix("<![CDATA[") {
            rest = after.find("]]>").map_or("", |end| &after[end + 3..]);
            continue;
        }
        // The tag, to its `>` outside quotes.
        let mut quote = None;
        let Some(end) = rest.char_indices().find_map(|(i, c)| match quote {
            Some(q) if c == q => {
                quote = None;
                None
            }
            Some(_) => None,
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                None
            }
            None => (c == '>').then_some(i),
        }) else {
            break;
        };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if tag.starts_with('/') {
            open.pop();
            continue;
        }
        let element = tag
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default()
            .to_owned();
        let attrs = attributes(tag);
        let attr = |key: &str| attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        let parent = open.last().map(String::as_str);
        let depth = open.len();
        let kind = match (depth, element.as_str(), parent) {
            (1, "public" | "java-symbol" | "overlayable" | "eat-comment", _) => None,
            (1, "item", _) => Some(attr("type").unwrap_or_else(|| "item".into())),
            (1, "declare-styleable", _) => Some("styleable".into()),
            (1, "string-array" | "integer-array", _) => Some("array".into()),
            (1, other, _) => Some(other.to_owned()),
            (2, "attr", Some("declare-styleable")) if attr("format").is_some() => {
                Some("attr".into())
            }
            (_, "enum" | "flag", Some("attr")) => Some("id".into()),
            _ => None,
        };
        if let (Some(kind), Some(name)) = (kind, attr("name")) {
            names.push(format!("{kind}/{name}"));
        }
        if !tag.ends_with('/') {
            open.push(element);
        }
    }
    names
}

/// The attributes of a tag (`string name="a" tools:x='b'`): names with
/// their prefix, values without quotes.
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    // Past the element's name.
    let mut rest = tag.trim_start().trim_end_matches('/');
    rest = rest.find(char::is_whitespace).map_or("", |at| &rest[at..]);
    loop {
        rest = rest.trim_start();
        let Some(eq) = rest.find('=') else { break };
        let key = rest[..eq].trim().to_owned();
        let value = rest[eq + 1..].trim_start();
        let Some(quote) = value.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let Some(close) = value[1..].find(quote) else {
            break;
        };
        attrs.push((key, value[1..1 + close].to_owned()));
        rest = &value[close + 2..];
    }
    attrs
}

/// A digest of `paths` (files, and the files of directories): each one's
/// path, size and time of change, what a development host is built from,
/// so a changed file rebuilds it without reading every file each run.
pub(crate) fn digest(paths: &[&Path]) -> anyhow::Result<String> {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    for path in paths {
        let files = if std::fs::symlink_metadata(path)?.is_dir() {
            walk(path)?
        } else {
            vec![path.to_path_buf()]
        };
        for file in files {
            let meta =
                std::fs::metadata(&file).with_context(|| format!("read {}", file.display()))?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            let name = file.to_string_lossy();
            hasher.update((name.len() as u64).to_le_bytes());
            hasher.update(name.as_bytes());
            hasher.update(meta.len().to_le_bytes());
            hasher.update(modified.to_le_bytes());
        }
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Checks `xml` is one element, balanced, whose name is one of `elements`
/// (what it holds is the element's own), with nothing but whitespace and
/// comments around it.
fn check_element(xml: &str, elements: &[&str]) -> anyhow::Result<()> {
    let allowed = || {
        elements
            .iter()
            .map(|e| format!("<{e}>"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut open: Vec<&str> = Vec::new();
    let mut root: Option<&str> = None;
    let mut rest = xml;
    loop {
        // Text: only inside the element.
        let at = rest.find('<').unwrap_or(rest.len());
        if (open.is_empty()) && !rest[..at].trim().is_empty() {
            bail!("{xml:?} has text outside its element");
        }
        rest = &rest[at..];
        if rest.is_empty() {
            break;
        }
        if let Some(comment) = rest.strip_prefix("<!--") {
            let end = comment
                .find("-->")
                .with_context(|| format!("{xml:?} has an unclosed comment"))?;
            rest = &comment[end + 3..];
            continue;
        }
        // The tag, to its `>` outside quotes.
        let mut quote = None;
        let end = rest
            .char_indices()
            .find(|&(_, c)| match quote {
                Some(q) => {
                    if c == q {
                        quote = None;
                    }
                    false
                }
                None if c == '"' || c == '\'' => {
                    quote = Some(c);
                    false
                }
                None => c == '>',
            })
            .map(|(i, _)| i)
            .with_context(|| format!("{xml:?} has an unclosed tag"))?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim();
            if open.pop() != Some(name) {
                bail!("{xml:?} closes <{name}> that it did not open");
            }
            continue;
        }
        let name = tag
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_:.".contains(c))
        {
            bail!("{xml:?} has a tag that is not an element: <{tag}>");
        }
        if open.is_empty() {
            if root.is_some() {
                bail!("{xml:?} is more than one element: write one per entry");
            }
            if !elements.contains(&name) {
                bail!("{xml:?} is not one of the elements it takes: {}", allowed());
            }
            root = Some(name);
        }
        if !tag.trim_end().ends_with('/') {
            open.push(name);
        }
    }
    if let Some(name) = open.last() {
        bail!("{xml:?} does not close <{name}>");
    }
    if root.is_none() {
        bail!("{xml:?} is not one of the elements it takes: {}", allowed());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// `cargo metadata`'s output for `packages` (name, metadata, features
    /// on).
    fn metadata(packages: &[(&str, Value, &[&str])]) -> Value {
        let id = |name: &str| format!("path+file:///work/{name}#0.1.0");
        json!({
            "packages": packages.iter().map(|(name, metadata, _)| json!({
                "id": id(name),
                "name": name,
                "metadata": metadata,
            })).collect::<Vec<_>>(),
            "resolve": { "nodes": packages.iter().map(|(name, _, features)| json!({
                "id": id(name),
                "features": features,
            })).collect::<Vec<_>>() },
        })
    }

    /// A crate of modules behind features, as nsh-extra declares them.
    fn extra() -> Value {
        json!({ "nsh": { "features": { "camera": {
            "ios": { "infoPlist": { "NSCameraUsageDescription": "The app takes photos with the camera." } },
            "android": { "queries": [
                "<intent><action android:name=\"android.media.action.IMAGE_CAPTURE\"/></intent>"
            ] },
        } } } })
    }

    #[test]
    fn only_the_features_on_count() {
        // The app's crate depends on nsh-extra (by path, Git or crates.io:
        // the graph is the same) with `camera` on.
        let on = metadata(&[
            ("app-native", Value::Null, &[]),
            ("nsh-extra", extra(), &["camera", "default"]),
        ]);
        let (meta, warnings) = ModuleMeta::from_metadata(&on).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(
            meta.ios_info["NSCameraUsageDescription"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
        );
        assert_eq!(meta.android_queries.len(), 1);
        assert!(meta.android_queries[0].contains("IMAGE_CAPTURE"));
        // Linked without the feature: nothing of the camera's.
        let off = metadata(&[("nsh-extra", extra(), &[])]);
        let (meta, _) = ModuleMeta::from_metadata(&off).unwrap();
        assert_eq!(meta, ModuleMeta::default());
    }

    #[test]
    fn crates_merge_and_clash() {
        let a = json!({ "nsh": {
            "ios": { "infoPlist": { "UIBackgroundModes": ["remote-notification"], "A": "1" } },
            "android": { "permissions": ["android.permission.CAMERA"] },
        } });
        let b = json!({ "nsh": {
            "ios": {
                "infoPlist": { "UIBackgroundModes": ["remote-notification", "audio"], "A": "1" },
                "entitlements": { "aps-environment": "development" },
            },
            "android": {
                "permissions": ["android.permission.CAMERA", "android.permission.POST_NOTIFICATIONS"],
                "application": ["<service android:name=\"x.Push\" android:exported=\"false\"/>"],
            },
        } });
        let (meta, _) =
            ModuleMeta::from_metadata(&metadata(&[("a", a.clone(), &[]), ("b", b, &[])])).unwrap();
        assert_eq!(
            meta.ios_info["UIBackgroundModes"],
            json!(["remote-notification", "audio"])
        );
        assert_eq!(meta.ios_entitlements["aps-environment"], "development");
        assert_eq!(
            meta.android_permissions,
            [
                "android.permission.CAMERA",
                "android.permission.POST_NOTIFICATIONS"
            ]
        );
        assert_eq!(meta.android_application.len(), 1);
    }

    #[test]
    fn the_app_settles_what_crates_disagree_on() {
        // Two crates with their own camera wording: nsh-extra's `camera` and
        // a scanner nshm.
        let scanner = json!({ "nsh": { "ios": {
            "infoPlist": { "NSCameraUsageDescription": "Scans the codes you point the camera at." },
        } } });
        let (meta, _) = ModuleMeta::from_metadata(&metadata(&[
            ("nsh-extra", extra(), &["camera"]),
            ("scanner", scanner, &[]),
        ]))
        .unwrap();
        // The app has not chosen: the package stops, naming both crates.
        let error = meta
            .apply(&mut AppConfig::default(), Platform::Ios)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("nsh-extra and scanner set `NSCameraUsageDescription` (ios.infoPlist)")
                && error.contains("`ios.infoPlist`"),
            "{error}"
        );
        // The app's own wording settles it: it packages, with the app's.
        let mut config = AppConfig::default();
        config.ios.info_plist.insert(
            "NSCameraUsageDescription".into(),
            json!("Photos of receipts."),
        );
        meta.apply(&mut config, Platform::Ios).unwrap();
        assert_eq!(
            config.ios.info_plist["NSCameraUsageDescription"],
            "Photos of receipts."
        );
        assert!(config.android.queries[0].contains("IMAGE_CAPTURE"));
        // An Android package has no iOS keys: the clash does not stop it.
        let mut android = AppConfig::default();
        meta.apply(&mut android, Platform::Android).unwrap();
        assert!(android.android.queries[0].contains("IMAGE_CAPTURE"));
    }

    #[test]
    fn crates_permissions_are_checked_and_spelled_out() {
        let permissions = |list: Value| {
            ModuleMeta::from_metadata(&metadata(&[(
                "scanner",
                json!({ "nsh": { "android": { "permissions": list } } }),
                &[],
            )]))
        };
        let (meta, warnings) =
            permissions(json!(["CAMERA", "com.example.permission.SCAN"])).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            meta.android_permissions,
            ["android.permission.CAMERA", "com.example.permission.SCAN"]
        );
        // A bare name in lower case is most likely a typo: warned of, with
        // the crate's name.
        let (_, warnings) = permissions(json!(["camera"])).unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].starts_with("scanner: "), "{warnings:?}");
        // Only a name reaches the manifest, and the error names the crate.
        let error = permissions(json!(["CAMERA\"/><application android:debuggable=\"true"]))
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("scanner: [package.metadata.nsh] `android.permissions`"),
            "{error}"
        );
    }

    #[test]
    fn elements_and_keys_are_checked() {
        let bad = |android: Value| {
            ModuleMeta::from_metadata(&metadata(&[(
                "x",
                json!({ "nsh": { "android": android } }),
                &[],
            )]))
        };
        // `<application>`'s own attributes are not the crates'.
        assert!(bad(json!({ "application": ["<application android:name=\"x.App\"/>"] })).is_err());
        assert!(bad(json!({ "queries": ["<service android:name=\"x\"/>"] })).is_err());
        assert!(
            bad(json!({ "application": ["<meta-data android:name=\"a\" android:value=\"b\"/>"] }))
                .is_ok()
        );
        assert!(bad(json!({ "queries": ["<package android:name=\"com.example\" />"] })).is_ok());
        assert!(bad(json!({ "permissions": "CAMERA" })).is_err());
        // One balanced element per entry: nothing that closes the element it
        // goes in to write elsewhere in the manifest.
        for escape in [
            "<service android:name=\"a\"/></application><application android:debuggable=\"true\">",
            "<service android:name=\"a\"/><uses-permission android:name=\"b\"/>",
            "<service android:name=\"a\">",
            "<service android:name=\"a\"></activity>",
            "text <service android:name=\"a\"/>",
            "<!DOCTYPE x><service/>",
        ] {
            assert!(bad(json!({ "application": [escape] })).is_err(), "{escape}");
        }
        assert!(
            bad(json!({ "application": [
                "<receiver android:name=\"a.B\" android:exported=\"false\"><!-- push --><intent-filter><action android:name=\"a > b\"/></intent-filter></receiver>"
            ] }))
            .is_ok()
        );
        // Keys NSH sets itself, named with the crate.
        for reserved in [
            json!({ "nsh": { "ios": { "infoPlist": { "CFBundleIdentifier": "x" } } } }),
            json!({ "nsh": { "ios": { "entitlements": { "application-identifier": "x" } } } }),
        ] {
            let error = ModuleMeta::from_metadata(&metadata(&[("x", reserved, &[])]))
                .unwrap_err()
                .to_string();
            assert!(
                error.starts_with("x: ") && error.contains("NSH sets itself"),
                "{error}"
            );
        }
        // Keys a later nsh reads: reported, left out.
        let (meta, warnings) = ModuleMeta::from_metadata(&metadata(&[(
            "x",
            json!({ "nsh": { "android": { "maven": ["a:b:1"] }, "ios": { "privacy": {} } } }),
            &[],
        )]))
        .unwrap();
        assert_eq!(meta, ModuleMeta::default());
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    /// `cargo metadata`'s output for crates with directories: (name, its
    /// directory, its `android` table).
    fn crates_with_files(crates: &[(&str, &Path, Value)]) -> Value {
        let id = |name: &str| format!("path+file:///work/{name}#0.1.0");
        json!({
            "packages": crates.iter().map(|(name, dir, android)| json!({
                "id": id(name),
                "name": name,
                "manifest_path": dir.join("Cargo.toml"),
                "metadata": { "nsh": { "android": android } },
            })).collect::<Vec<_>>(),
            "resolve": { "nodes": crates.iter().map(|(name, _, _)| json!({
                "id": id(name),
                "features": [],
            })).collect::<Vec<_>>() },
        })
    }

    #[test]
    fn crates_android_files_are_their_own() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-crate-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let krate = dir.join("ocr");
        std::fs::create_dir_all(krate.join("android/res/values")).unwrap();
        std::fs::create_dir_all(krate.join("android/jniLibs/arm64-v8a")).unwrap();
        std::fs::create_dir_all(krate.join("android/bad-jni/mips")).unwrap();
        std::fs::write(krate.join("android/ocr.dex"), b"dex\n035\0ocr").unwrap();
        std::fs::write(krate.join("android/ocr.jar"), b"PK\x03\x04").unwrap();
        std::fs::write(krate.join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.join("secret.dex"), b"dex\n035\0x").unwrap();
        let read = |android: Value| {
            let (mut meta, warnings) =
                ModuleMeta::from_metadata(&crates_with_files(&[("ocr", &krate, android)]))?;
            assert!(warnings.is_empty(), "{warnings:?}");
            meta.resolve_android_files().map(|()| meta)
        };
        let meta = read(json!({
            "dex": ["android/ocr.dex"],
            "res": "android/res",
            "jniLibs": "android/jniLibs",
        }))
        .unwrap();
        let canonical = |p: &str| dunce::canonicalize(krate.join(p)).unwrap();
        assert_eq!(meta.android_dex, [canonical("android/ocr.dex")]);
        assert_eq!(meta.android_res, [canonical("android/res")]);
        assert_eq!(meta.android_jni_libs, [canonical("android/jniLibs")]);
        // After the app's own dex; the crates' resources and libraries
        // beside the app's.
        let mut config = AppConfig::default();
        config.android.dex.push(dir.join("app.dex"));
        meta.apply(&mut config, Platform::Android).unwrap();
        assert_eq!(
            config.android.dex,
            [dir.join("app.dex"), canonical("android/ocr.dex")]
        );
        assert_eq!(config.android.crate_res, [canonical("android/res")]);
        assert_eq!(config.android.jni_libs, [canonical("android/jniLibs")]);
        // A crate packages only its own files, of the right kind.
        let mut cases = vec![
            (json!({ "dex": ["../secret.dex"] }), "outside the crate"),
            (json!({ "dex": [dir.join("secret.dex")] }), "is absolute"),
            (json!({ "dex": ["android/missing.dex"] }), "is missing"),
            (json!({ "dex": ["android/ocr.jar"] }), "is not a dex file"),
            (json!({ "res": "android/ocr.dex" }), "is not a directory"),
            (json!({ "jniLibs": "android/bad-jni" }), "is no Android ABI"),
        ];
        // A file in the crate's resources that links outside it (links
        // need no privilege on Unix).
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                dir.join("secret.dex"),
                krate.join("android/res/values/out.xml"),
            )
            .unwrap();
            cases.push((json!({ "res": "android/res" }), "leads outside the crate"));
        }
        for (android, says) in cases {
            let error = format!("{:#}", read(android.clone()).unwrap_err());
            assert!(
                error.starts_with("ocr: [package.metadata.nsh] `android.") && error.contains(says),
                "{android}: {error}"
            );
        }
        let error = format!(
            "{:#}",
            read(json!({ "dex": "android/ocr.dex" })).unwrap_err()
        );
        assert!(error.contains("is a list of strings"), "{error}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn crates_android_libraries_are_aar_or_jar_files_of_theirs() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-crate-libs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("libs")).unwrap();
        std::fs::write(dir.join("libs/ocr.aar"), b"PK").unwrap();
        std::fs::write(dir.join("libs/kotlin.jar"), b"PK").unwrap();
        std::fs::write(dir.join("libs/notes.txt"), b"x").unwrap();
        let read = |android: Value| {
            let (mut meta, _) =
                ModuleMeta::from_metadata(&crates_with_files(&[("ocr", &dir, android)]))?;
            meta.resolve_android_files().map(|()| meta)
        };
        let meta =
            read(json!({ "libraries": ["libs/ocr.aar", "libs/kotlin.jar", "libs/ocr.aar"] }))
                .unwrap();
        let canonical = |p: &str| dunce::canonicalize(dir.join(p)).unwrap();
        assert_eq!(
            meta.android_libraries,
            [canonical("libs/ocr.aar"), canonical("libs/kotlin.jar")]
        );
        let mut config = AppConfig::default();
        meta.apply(&mut config, Platform::Android).unwrap();
        assert_eq!(config.android.libraries.len(), 2);
        let error = format!(
            "{:#}",
            read(json!({ "libraries": ["libs/notes.txt"] })).unwrap_err()
        );
        assert!(error.contains("is no `.aar` or `.jar` file"), "{error}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn other_platforms_never_read_a_crates_android_files() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-crate-ios-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (mut meta, _) = ModuleMeta::from_metadata(&crates_with_files(&[(
            "ocr",
            &dir,
            json!({ "dex": ["missing.dex"], "permissions": ["CAMERA"] }),
        )]))
        .unwrap();
        // An iOS package applies what the crates declare without the files.
        meta.apply(&mut AppConfig::default(), Platform::Ios)
            .unwrap();
        // Android finds the file missing, and the rest is still there.
        assert!(meta.resolve_android_files().is_err());
        assert_eq!(meta.android_permissions, ["android.permission.CAMERA"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn crates_resources_take_no_name_nsh_or_another_crate_has() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-crate-res-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let res = |krate: &str, file: &str, text: &str| {
            let path = dir.join(krate).join("res").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
            dir.join(krate)
        };
        let a = res(
            "a",
            "values/strings.xml",
            r#"<?xml version="1.0"?>
<resources>
    <!-- <string name="commented">x</string> -->
    <string name="greeting">Hi</string>
    <style name="Scanner"><item name="android:windowBackground">@null</item></style>
</resources>"#,
        );
        res("a", "drawable-hdpi/scan.png", "png");
        let b = res(
            "b",
            "values/s.xml",
            r#"<resources><string name="farewell">Bye</string><item type="id" name="scan"/></resources>"#,
        );
        let both = |b_android: Value| {
            let (mut meta, _) = ModuleMeta::from_metadata(&crates_with_files(&[
                ("a", &a, json!({ "res": "res" })),
                ("b", &b, b_android),
            ]))
            .unwrap();
            meta.resolve_android_files().map(|()| meta)
        };
        // `drawable/scan` and `id/scan` differ; a style's items are not
        // resources of their own.
        let meta = both(json!({ "res": "res" })).unwrap();
        assert_eq!(meta.android_res.len(), 2);
        res(
            "b",
            "values/more.xml",
            r#"<resources><string name="greeting">Hey</string></resources>"#,
        );
        let error = format!("{:#}", both(json!({ "res": "res" })).unwrap_err());
        assert!(
            error.contains("a and b both declare the Android resource `string/greeting`"),
            "{error}"
        );
        std::fs::remove_file(dir.join("b/res/values/more.xml")).unwrap();
        for (file, text, resource) in [
            ("mipmap-hdpi/ic_launcher.png", "png", "mipmap/ic_launcher"),
            (
                "values/c.xml",
                r#"<resources><color name="nsh_launch_background">#fff</color></resources>"#,
                "color/nsh_launch_background",
            ),
        ] {
            res("b", file, text);
            let error = format!("{:#}", both(json!({ "res": "res" })).unwrap_err());
            assert!(
                error.contains(&format!("`{resource}` takes a name of NSH's own resources")),
                "{error}"
            );
            std::fs::remove_file(dir.join("b/res").join(file)).unwrap();
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn values_files_declare_what_aapt2_reads() {
        let names = values_names(
            r#"<?xml version="1.0"?>
<resources xmlns:tools="http://schemas.android.com/tools">
    <!-- <string name="commented">x</string> -->
    <string name='single'>a</string>
    <string name = "spaced">b</string>
    <string tools:name="z" name="real">c</string>
    <item name="b" type="dimen" format="x>y">1dp</item>
    <string name="after">d</string>
    <style name="Scanner"><item name="colorAccent">#f00</item></style>
    <declare-styleable name="Gauge">
        <attr name="tint" format="color" />
        <attr name="android:text" />
        <attr name="mode"><enum name="ring" value="0" /></attr>
    </declare-styleable>
    <public type="string" name="single" />
    <eat-comment />
    <string-array name="list"><item>x</item></string-array>
</resources>"#,
        );
        assert_eq!(
            names,
            [
                "string/single",
                "string/spaced",
                "string/real",
                "dimen/b",
                "string/after",
                "style/Scanner",
                "styleable/Gauge",
                "attr/tint",
                "id/ring",
                "array/list",
            ]
        );
    }

    #[test]
    fn resource_files_are_named_as_aapt2_names_them() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-res-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for file in [
            "raw/model.v1.bin",
            "drawable-hdpi/frame.9.png",
            "drawable-night-v31/icon.xml",
        ] {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        assert_eq!(
            resource_names(&dir)
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
            ["drawable/frame", "drawable/icon", "raw/model.v1"]
        );
        // A link to a folder is not followed: refused.
        #[cfg(unix)]
        {
            std::fs::create_dir_all(dir.join("shared/values")).unwrap();
            std::os::unix::fs::symlink(dir.join("shared/values"), dir.join("res-values")).unwrap();
            let error = format!("{:#}", walk(&dir).unwrap_err());
            assert!(error.contains("links to a folder"), "{error}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_digest_follows_what_files_hold() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-digest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("res/values")).unwrap();
        std::fs::write(dir.join("res/values/a.xml"), "<resources/>").unwrap();
        std::fs::write(dir.join("a.dex"), b"dex\n1").unwrap();
        let paths = [dir.join("a.dex"), dir.join("res")];
        let paths: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        let first = digest(&paths).unwrap();
        assert_eq!(digest(&paths).unwrap(), first);
        std::fs::write(dir.join("res/values/a.xml"), "<resources></resources>").unwrap();
        assert_ne!(digest(&paths).unwrap(), first);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_app_wins() {
        let mut config = AppConfig::default();
        config
            .ios
            .info_plist
            .insert("NSCameraUsageDescription".into(), json!("Scan receipts."));
        config
            .ios
            .info_plist
            .insert("UIBackgroundModes".into(), json!(["audio"]));
        config.android.permissions.push("CAMERA".into());
        let mut meta = ModuleMeta::default();
        meta.ios_info
            .insert("NSCameraUsageDescription".into(), json!("Takes photos."));
        meta.ios_info
            .insert("UIBackgroundModes".into(), json!(["remote-notification"]));
        meta.ios_entitlements
            .insert("aps-environment".into(), json!("development"));
        meta.android_permissions
            .push("android.permission.POST_NOTIFICATIONS".into());
        meta.android_queries
            .push("<package android:name=\"a\"/>".into());
        meta.apply(&mut config, Platform::Ios).unwrap();
        assert_eq!(
            config.ios.info_plist["NSCameraUsageDescription"],
            "Scan receipts."
        );
        assert_eq!(
            config.ios.info_plist["UIBackgroundModes"],
            json!(["audio", "remote-notification"])
        );
        assert_eq!(config.ios.entitlements["aps-environment"], "development");
        assert_eq!(
            config.android.permissions,
            ["CAMERA", "android.permission.POST_NOTIFICATIONS"]
        );
        assert_eq!(config.android.queries.len(), 1);
    }
}
