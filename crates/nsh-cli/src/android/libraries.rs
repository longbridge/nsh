//! The Java libraries an Android package carries (`android.libraries` of the
//! app's `nsh.json` and of its crates' `[package.metadata.nsh]`): jar files,
//! which the SDK's `d8` turns into dex; prebuilt dex files; and Android
//! libraries (AARs), whose classes go the jars' way and whose native
//! libraries, assets and manifest the package takes as well. A package
//! without libraries runs none of this.

use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::{
    Apk, MIN_SDK, Sdk,
    library_manifest::{self, Additions, Element, Placeholders},
};

/// Where `nsh.json` lists its libraries, as errors name it.
const NSH_JSON: &str = "nsh.json `android.libraries`";

/// A Java library a package carries.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "PathBuf")]
pub(crate) struct Library {
    /// The file. One `nsh.json` lists is relative to `nsh.json` until
    /// `read_config` resolves it; a crate's is resolved against the
    /// directory of the crate's `Cargo.toml`.
    pub(crate) path: PathBuf,
    /// Who lists it, for errors: `nsh.json `android.libraries`` or
    /// `<crate>: [package.metadata.nsh] `android.libraries``.
    pub(crate) listed: String,
}

impl From<PathBuf> for Library {
    fn from(path: PathBuf) -> Self {
        Library {
            path,
            listed: NSH_JSON.to_owned(),
        }
    }
}

/// What a library file is, by its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Jar,
    Dex,
    Aar,
}

impl Library {
    fn kind(&self) -> anyhow::Result<Kind> {
        let extension = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("jar") => Ok(Kind::Jar),
            Some("dex") => Ok(Kind::Dex),
            Some("aar") => Ok(Kind::Aar),
            _ => bail!(
                "{}: {} is neither a jar (`.jar`), an Android library (`.aar`) nor a dex file (`.dex`)",
                self.listed,
                self.path.display()
            ),
        }
    }

    /// `path`, and who lists it.
    fn describe(&self) -> String {
        format!("{} ({})", self.path.display(), self.listed)
    }
}

/// What the libraries add to the package, after the runtime's.
#[derive(Debug, Default)]
pub(super) struct Contents {
    /// Dex files, numbered in this order after the runtime's
    /// `classes.dex`: `classes2.dex`, `classes3.dex`, ...
    pub(super) dex: Vec<PathBuf>,
    /// The jars' Java resources (`META-INF/services/...` and the like):
    /// (path at the package's root, contents).
    pub(super) resources: Vec<(String, Vec<u8>)>,
    /// The native libraries of the AARs that have any ([`native_for`]).
    pub(super) native: Vec<Native>,
    /// The AARs' assets: (path under `assets/`, contents).
    pub(super) assets: Vec<(String, Vec<u8>)>,
    /// What the manifest gets: the crates' entries and the AARs' manifests.
    pub(super) manifest: Additions,
}

/// The native libraries of an AAR.
#[derive(Debug)]
pub(super) struct Native {
    /// The AAR and who lists it, for errors.
    pub(super) from: String,
    /// Its `jni/<abi>/*.so`: (file name, contents) by ABI.
    pub(super) abis: BTreeMap<String, Vec<(String, Vec<u8>)>>,
}

/// A library file to package, checked: one listed, or a jar of an AAR.
#[derive(Debug)]
struct Checked {
    path: PathBuf,
    /// Who lists it, which errors start with.
    listed: String,
    /// The file and who lists it, which errors name it by.
    shown: String,
    /// The app's own `nsh.json` lists it.
    apps: bool,
    kind: Kind,
    sha256: String,
}

/// The libraries of `apk`, checked and each file once: a missing file or
/// one of another kind is an error naming who lists it; files with the same
/// contents (two crates that carry one jar) are packaged once, as the first
/// that lists them (the app before its crates).
fn checked(libraries: &[Library]) -> anyhow::Result<Vec<Checked>> {
    let mut out: Vec<Checked> = Vec::new();
    for library in libraries {
        let kind = library.kind()?;
        let bytes = fs::read(&library.path)
            .with_context(|| format!("{}: read {}", library.listed, library.path.display()))?;
        if kind == Kind::Dex && !bytes.starts_with(b"dex\n") {
            bail!(
                "{}: {} is not a dex file",
                library.listed,
                library.path.display()
            );
        }
        let sha256 = hex(&Sha256::digest(&bytes));
        if out.iter().any(|c| c.sha256 == sha256) {
            continue;
        }
        out.push(Checked {
            path: library.path.clone(),
            listed: library.listed.clone(),
            shown: library.describe(),
            apps: library.listed == NSH_JSON,
            kind,
            sha256,
        });
    }
    Ok(out)
}

/// What the libraries of `apk` add to the package: the dex files as they
/// are, in the order listed; the jars, and those of the AARs, each through
/// `d8 --intermediate` (kept in `apk.library_cache` by contents, `d8`
/// version, API level and mode), then merged by one `d8` run into `work`,
/// which fails on a class two libraries define; and the AARs' native
/// libraries, assets and manifests. Without libraries, only the crates'
/// manifest entries.
pub(super) fn prepare(sdk: &Sdk, apk: &Apk<'_>, work: &Path) -> anyhow::Result<Contents> {
    let mut contents = Contents::default();
    let mut files = Vec::new();
    let mut aars = Vec::new();
    for checked in checked(apk.libraries)? {
        if checked.kind == Kind::Aar {
            aars.push(read_aar(checked)?);
        } else {
            files.push(checked);
        }
    }
    let placeholders = Placeholders {
        package: apk.package,
        values: apk.manifest_placeholders,
    };
    let mut manifests = Vec::new();
    let mut warnings = Vec::new();
    // Each asset's AAR, for errors.
    let mut assets: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (i, aar) in choose(aars)?.into_iter().enumerate() {
        let from = aar.checked.shown;
        if let Some(level) = aar.min_compile_sdk
            && level > apk.target_sdk
        {
            bail!(
                "{from} compiles against Android API level {level} and later (its minCompileSdk), \
                 the app against {}: set nsh.json `android.targetSdk` to {level} or more",
                apk.target_sdk
            );
        }
        let parts = library_manifest::resolve(&aar.manifest, &from, &placeholders, &mut warnings)?;
        manifests.push((parts, from.clone()));
        // Its classes go the jars' way, from the work directory.
        let dir = work.join("aar").join(i.to_string());
        for (name, bytes) in aar.jars {
            let sha256 = hex(&Sha256::digest(&bytes));
            if files.iter().any(|c| c.sha256 == sha256) {
                continue;
            }
            let path = dir.join(&name);
            fs::create_dir_all(path.parent().unwrap_or(&dir))?;
            fs::write(&path, &bytes)?;
            files.push(Checked {
                path,
                listed: aar.checked.listed.clone(),
                shown: format!("`{name}` of {from}"),
                apps: aar.checked.apps,
                kind: Kind::Jar,
                sha256,
            });
        }
        for (path, bytes) in aar.assets {
            match assets.get(&path) {
                None => {
                    assets.insert(path.clone(), (contents.assets.len(), from.clone()));
                    contents.assets.push((path, bytes));
                }
                Some((at, _)) if contents.assets[*at].1 == bytes => {}
                Some((_, first)) => bail!(
                    "{first} and {from} both have the asset `{path}`, with different contents: list one of them"
                ),
            }
        }
        if !aar.native.is_empty() {
            contents.native.push(Native {
                from,
                abis: aar.native,
            });
        }
    }
    for warning in warnings {
        crate::ui::warn(warning);
    }
    contents.manifest = library_manifest::additions(apk, manifests)?;

    contents.dex.extend(
        files
            .iter()
            .filter(|c| c.kind == Kind::Dex)
            .map(|c| c.path.clone()),
    );
    let jars: Vec<&Checked> = files.iter().filter(|c| c.kind == Kind::Jar).collect();
    if jars.is_empty() {
        return Ok(contents);
    }
    let android_jar = sdk.android_jar(apk.target_sdk)?;
    // Each intermediate dex, with the library it is made of.
    let mut intermediate: Vec<(PathBuf, &Checked)> = Vec::new();
    for jar in &jars {
        let dir = intermediate_dex(sdk, jar, &android_jar, apk)?;
        for dex in dex_files(&dir)? {
            intermediate.push((dex, jar));
        }
    }
    let merged = work.join("libraries-dex");
    if merged.exists() {
        fs::remove_dir_all(&merged)?;
    }
    fs::create_dir_all(&merged)?;
    let inputs: Vec<PathBuf> = intermediate.iter().map(|(dex, _)| dex.clone()).collect();
    let output = merge_command(sdk, &inputs, apk.dev, &merged)
        .output()
        .context("run the SDK's d8 (is a JDK installed?)")?;
    if !output.status.success() {
        bail!(
            "{}",
            merge_error(
                &format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
                &intermediate,
            )
        );
    }
    contents.dex.extend(dex_files(&merged)?);
    contents.resources = java_resources(&jars)?;
    Ok(contents)
}

/// Where an AAR keeps what its build was made for.
const AAR_METADATA: &str = "META-INF/com/android/build/gradle/aar-metadata.properties";

/// An AAR, read.
struct Aar {
    checked: Checked,
    /// Its manifest's package: what tells the library apart from others,
    /// whichever version of it this is.
    package: String,
    manifest: Element,
    /// `classes.jar` and `libs/*.jar`: (path in the AAR, contents).
    jars: Vec<(String, Vec<u8>)>,
    /// `jni/<abi>/*.so`: (file name, contents) by ABI.
    native: BTreeMap<String, Vec<(String, Vec<u8>)>>,
    /// `assets/`: (path under it, contents).
    assets: Vec<(String, Vec<u8>)>,
    /// The API level an app must compile against (`minCompileSdk`).
    min_compile_sdk: Option<u32>,
}

/// Reads what the package takes of an AAR: its classes, native
/// libraries, assets and manifest. Resources (`res/`) are an error, which
/// this does not package yet; what only a build reads (`R.txt`,
/// `proguard.txt`, `lint.jar`, ...) is left out.
fn read_aar(checked: Checked) -> anyhow::Result<Aar> {
    let file = fs::File::open(&checked.path)
        .with_context(|| format!("{}: read {}", checked.listed, checked.path.display()))?;
    let mut archive = zip::ZipArchive::new(file).with_context(|| {
        format!(
            "{}: {} is not an Android library (AAR)",
            checked.listed,
            checked.path.display()
        )
    })?;
    let mut manifest = None;
    let mut jars = Vec::new();
    let mut native: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
    let mut assets = Vec::new();
    let mut min_compile_sdk = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.into_owned();
        if entry.enclosed_name().is_none() {
            bail!("{} has `{name}`, a path outside it", checked.shown);
        }
        let mut read = || -> anyhow::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            Ok(bytes)
        };
        let libs_jar = name
            .strip_prefix("libs/")
            .is_some_and(|jar| !jar.contains('/') && jar.ends_with(".jar"));
        if name == "AndroidManifest.xml" {
            manifest = Some(String::from_utf8(read()?).with_context(|| {
                format!("{}: its AndroidManifest.xml is not UTF-8", checked.shown)
            })?);
        } else if name == "classes.jar" || libs_jar {
            jars.push((name, read()?));
        } else if let Some(rest) = name.strip_prefix("jni/") {
            if let Some((abi, so)) = rest.split_once('/')
                && !so.contains('/')
                && so.ends_with(".so")
            {
                let (abi, so) = (abi.to_owned(), so.to_owned());
                native.entry(abi).or_default().push((so, read()?));
            }
        } else if let Some(path) = name.strip_prefix("assets/") {
            if path.starts_with("nsh/") {
                bail!("{} has `{name}`: `assets/nsh/` is NSH's own", checked.shown);
            }
            assets.push((path.to_owned(), read()?));
        } else if name.starts_with("res/") {
            bail!(
                "{} has Android resources (`{name}`), which this nsh does not package yet: \
                 the next version will",
                checked.shown
            );
        } else if name == AAR_METADATA {
            let text = String::from_utf8_lossy(&read()?).into_owned();
            if let Some(value) = text
                .lines()
                .filter_map(|line| line.split_once('='))
                .find(|(key, _)| key.trim() == "minCompileSdk")
                .map(|(_, value)| value.trim().to_owned())
            {
                min_compile_sdk = Some(value.parse().map_err(|_| {
                    anyhow::anyhow!(
                        "{}: its minCompileSdk `{value}` is no API level",
                        checked.shown
                    )
                })?);
            }
        }
    }
    let manifest =
        manifest.with_context(|| format!("{} has no AndroidManifest.xml", checked.shown))?;
    let manifest = library_manifest::parse(&manifest)
        .with_context(|| format!("{}: read its AndroidManifest.xml", checked.shown))?;
    let package = manifest
        .attribute("package")
        .filter(|package| !package.is_empty())
        .with_context(|| format!("{}: its manifest names no package", checked.shown))?
        .to_owned();
    Ok(Aar {
        checked,
        package,
        manifest,
        jars,
        native,
        assets,
        min_compile_sdk,
    })
}

/// One AAR per library (manifest package): the app's own when it lists
/// one, which settles which version of it the package takes; two others
/// are an error naming both. (The same file twice was left out already.)
fn choose(aars: Vec<Aar>) -> anyhow::Result<Vec<Aar>> {
    let mut out: Vec<Aar> = Vec::new();
    for aar in aars {
        let Some(have) = out.iter().find(|have| have.package == aar.package) else {
            out.push(aar);
            continue;
        };
        // The app lists its own first.
        if have.checked.apps && !aar.checked.apps {
            continue;
        }
        let settle = if have.checked.apps {
            "list one of them"
        } else {
            "list the one to package in nsh.json `android.libraries`"
        };
        bail!(
            "{} and {} are two versions of the library `{}`: {settle}",
            have.checked.shown,
            aar.checked.shown,
            aar.package
        );
    }
    Ok(out)
}

/// The native libraries of `native` for `abi`, which the package has:
/// (file name, contents), each name once. An AAR with native libraries but
/// none for `abi` is an error (its code would not load on such a device),
/// as is a library named as NSH's own.
pub(super) fn native_for<'a>(
    native: &'a [Native],
    abi: &str,
) -> anyhow::Result<Vec<(&'a str, &'a [u8])>> {
    let mut out: Vec<(&str, &[u8], &str)> = Vec::new();
    for library in native {
        let Some(files) = library.abis.get(abi) else {
            bail!(
                "{} has native libraries for {}, not for {abi}, which the package has: \
                 package the ABIs it has (`--abi`) or list a build of it for {abi}",
                library.from,
                library
                    .abis
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        };
        for (name, bytes) in files {
            if name == "libnsh.so" {
                bail!(
                    "{} has `jni/{abi}/libnsh.so`, the name of NSH's own library",
                    library.from
                );
            }
            match out.iter().find(|(have, ..)| *have == name) {
                None => out.push((name, bytes, &library.from)),
                Some((_, have, _)) if *have == bytes.as_slice() => {}
                Some((.., first)) => bail!(
                    "{first} and {} both have `{name}` for {abi}, with different contents: list one of them",
                    library.from
                ),
            }
        }
    }
    Ok(out
        .into_iter()
        .map(|(name, bytes, _)| (name, bytes))
        .collect())
}

/// Whether an asset goes into the package uncompressed: the kinds `aapt2`
/// leaves so, which are compressed already, and which an app opens as a
/// file descriptor (`AssetManager.openFd`) only when stored.
pub(super) fn stored_asset(path: &str) -> bool {
    const STORED: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "webp", "wav", "mp2", "mp3", "ogg", "aac", "mpg", "mpeg",
        "mid", "midi", "smf", "jet", "rtttl", "imy", "xmf", "mp4", "m4a", "m4v", "3gp", "3gpp",
        "3g2", "3gpp2", "amr", "awb", "wma", "wmv", "webm", "mkv",
    ];
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| STORED.contains(&e.to_ascii_lowercase().as_str()))
}

/// The directory of `jar`'s intermediate dex in the cache, made when
/// missing.
fn intermediate_dex(
    sdk: &Sdk,
    jar: &Checked,
    android_jar: &Path,
    apk: &Apk<'_>,
) -> anyhow::Result<PathBuf> {
    let d8_version = sdk
        .build_tools
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let key = hex(&Sha256::digest(format!(
        "{}\0{d8_version}\0{MIN_SDK}\0{}\0{}",
        jar.sha256,
        mode(apk.dev),
        apk.target_sdk
    )));
    let dir = apk.library_cache.join(&key[..32]);
    if dir.join("done").exists() {
        return Ok(dir);
    }
    // Written aside and moved in whole: a run stopped half way, or another
    // one at the same time, leaves no partial entry.
    let partial = apk
        .library_cache
        .join(format!("{}.partial-{}", &key[..32], std::process::id()));
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }
    fs::create_dir_all(&partial)?;
    let output = intermediate_command(sdk, &jar.path, android_jar, apk.dev, &partial)
        .output()
        .context("run the SDK's d8 (is a JDK installed?)")?;
    if !output.status.success() {
        let _ = fs::remove_dir_all(&partial);
        bail!(
            "{}: d8 could not read {}:\n{}",
            jar.listed,
            jar.shown,
            d8_message(&format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        );
    }
    fs::write(partial.join("done"), b"")?;
    // Another run may have moved its own copy in meanwhile, the same dex,
    // which it or a third run may be reading: keep it, never remove it.
    if !dir.join("done").exists() && fs::rename(&partial, &dir).is_ok() {
        return Ok(dir);
    }
    let _ = fs::remove_dir_all(&partial);
    if !dir.join("done").exists() {
        bail!(
            "could not keep the dex of {} in {}",
            jar.shown,
            dir.display()
        );
    }
    Ok(dir)
}

fn mode(dev: bool) -> &'static str {
    if dev { "--debug" } else { "--release" }
}

/// `d8` on one jar: dex that a later run merges, desugared for the
/// runtime's minimum API level against the platform's `android.jar`.
fn intermediate_command(
    sdk: &Sdk,
    jar: &Path,
    android_jar: &Path,
    dev: bool,
    out: &Path,
) -> Command {
    let mut cmd = Command::new(sdk.tool("d8"));
    cmd.arg("--intermediate")
        .arg(mode(dev))
        .args(["--min-api", &MIN_SDK.to_string()])
        .arg("--lib")
        .arg(android_jar)
        .arg("--output")
        .arg(out)
        .arg(jar);
    cmd
}

/// `d8` merging the intermediate dex of every jar into `out`
/// (`classes.dex`, and more files when the methods do not fit one).
fn merge_command(sdk: &Sdk, inputs: &[PathBuf], dev: bool, out: &Path) -> Command {
    let mut cmd = Command::new(sdk.tool("d8"));
    cmd.arg(mode(dev))
        .args(["--min-api", &MIN_SDK.to_string()])
        .arg("--output")
        .arg(out)
        .args(inputs);
    cmd
}

/// The dex files `d8` wrote to `dir`, in their order: `classes.dex`,
/// `classes2.dex`, ...
fn dex_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files: Vec<(u32, PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(number) = name
            .strip_prefix("classes")
            .and_then(|n| n.strip_suffix(".dex"))
        else {
            continue;
        };
        let number = if number.is_empty() {
            1
        } else {
            match number.parse() {
                Ok(n) => n,
                Err(_) => continue,
            }
        };
        files.push((number, path));
    }
    files.sort();
    Ok(files.into_iter().map(|(_, path)| path).collect())
}

/// What `d8` printed, without the Java stack trace that follows its error.
fn d8_message(output: &str) -> String {
    output
        .lines()
        .take_while(|line| !line.starts_with("Exception in thread"))
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The error of the merging `d8` run, about the libraries: a class two of
/// them define names both files and who lists them, not the intermediate
/// dex `d8` saw.
fn merge_error(output: &str, intermediate: &[(PathBuf, &Checked)]) -> String {
    let library = |path: &str| {
        intermediate
            .iter()
            .find(|(dex, _)| dex.to_string_lossy() == path.trim())
            .map(|(_, library)| library.shown.clone())
    };
    let mut twice = Vec::new();
    for line in output.lines() {
        let Some((class, files)) = line
            .trim()
            .strip_prefix("Type ")
            .and_then(|rest| rest.split_once(" is defined multiple times: "))
        else {
            continue;
        };
        let files: Vec<String> = files
            .split(", ")
            .map(|path| library(path).unwrap_or_else(|| path.trim().to_owned()))
            .collect();
        let line = format!("`{class}` is in {}", files.join(" and "));
        if !twice.contains(&line) {
            twice.push(line);
        }
    }
    if !twice.is_empty() {
        return format!(
            "the Java libraries define a class more than once:\n{}\nlist one of the files that define it",
            twice.join("\n")
        );
    }
    let mut message = d8_message(output);
    for (dex, library) in intermediate {
        message = message.replace(&*dex.to_string_lossy(), &library.shown);
    }
    format!("d8 could not merge the Java libraries:\n{message}")
}

/// Entries of a jar that are no Java resource of the app: its manifest
/// and signature, and the license and build files Android's Gradle plugin
/// leaves out as well, of which every library has its own.
fn left_out(name: &str) -> bool {
    let Some(meta) = name.strip_prefix("META-INF/") else {
        return false;
    };
    if meta.contains('/') {
        return ["maven/", "proguard/", "com.android.tools/"]
            .iter()
            .any(|dir| meta.starts_with(dir));
    }
    let upper = meta.to_ascii_uppercase();
    upper == "MANIFEST.MF"
        || upper == "INDEX.LIST"
        || upper.starts_with("SIG-")
        || [".SF", ".RSA", ".DSA", ".EC"]
            .iter()
            .any(|e| upper.ends_with(e))
        || ["LICENSE", "NOTICE", "DEPENDENCIES", "AL2.0", "LGPL2.1"]
            .iter()
            .any(|n| upper == *n || upper.starts_with(&format!("{n}.")))
}

/// Paths at a package's root that are the package's own.
fn reserved(name: &str) -> bool {
    ["lib/", "assets/", "res/", "dex/", "manifest/", "root/"]
        .iter()
        .any(|dir| name.starts_with(dir))
        || matches!(name, "AndroidManifest.xml" | "resources.arsc")
        || (!name.contains('/') && name.starts_with("classes") && name.ends_with(".dex"))
}

/// The Java resources of `jars`: every file but classes and what
/// [`left_out`] names, each path once. Service files
/// (`META-INF/services/<interface>`) of several jars are joined, as
/// `ServiceLoader` reads every provider they list; another path two jars
/// have with different contents is an error naming both.
fn java_resources(jars: &[&Checked]) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    // Each path's index in `out`, and the jar it came from.
    let mut seen: BTreeMap<String, (usize, &Checked)> = BTreeMap::new();
    for jar in jars {
        let file = fs::File::open(&jar.path)
            .with_context(|| format!("{}: read {}", jar.listed, jar.shown))?;
        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("{}: {} is not a jar", jar.listed, jar.shown))?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name()?.into_owned();
            if name.ends_with(".class") || left_out(&name) {
                continue;
            }
            if entry.enclosed_name().is_none() || reserved(&name) {
                bail!(
                    "{}: {} has `{name}`, a path the package keeps for its own files",
                    jar.listed,
                    jar.shown
                );
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            match seen.get(&name) {
                None => {
                    seen.insert(name.clone(), (out.len(), jar));
                    out.push((name, bytes));
                }
                Some((at, _)) if out[*at].1 == bytes => {}
                Some((at, _)) if name.starts_with("META-INF/services/") => {
                    let have = &mut out[*at].1;
                    if !have.ends_with(b"\n") {
                        have.push(b'\n');
                    }
                    have.extend_from_slice(&bytes);
                }
                Some((_, first)) => bail!(
                    "{} and {} both have `{name}`, with different contents: list one of them",
                    first.shown,
                    jar.shown
                ),
            }
        }
    }
    Ok(out)
}

/// The SHA-256 of `path`'s contents, in hex.
pub(crate) fn file_sha256(path: &Path) -> anyhow::Result<String> {
    Ok(hex(&Sha256::digest(fs::read(path)?)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nsh-cli-android-libraries-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A jar (or an AAR) at `path` with `entries` (name, contents).
    fn jar(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn library(path: &Path, listed: &str) -> Library {
        Library {
            path: path.to_owned(),
            listed: listed.to_owned(),
        }
    }

    /// `library(path, listed)`, checked.
    fn checked_file(path: &Path, listed: &str) -> Checked {
        Checked {
            path: path.to_owned(),
            listed: listed.to_owned(),
            shown: library(path, listed).describe(),
            apps: listed == NSH_JSON,
            kind: Kind::Jar,
            sha256: String::new(),
        }
    }

    #[test]
    fn kinds_go_by_extension_and_name_who_lists_them() {
        let kind = |name: &str| {
            library(
                Path::new(name),
                "scanner: [package.metadata.nsh] `android.libraries`",
            )
            .kind()
        };
        assert_eq!(kind("libs/sdk.jar").unwrap(), Kind::Jar);
        assert_eq!(kind("libs/SDK.JAR").unwrap(), Kind::Jar);
        assert_eq!(kind("java/classes.dex").unwrap(), Kind::Dex);
        assert_eq!(kind("libs/sdk.aar").unwrap(), Kind::Aar);
        let other = kind("libs/sdk.zip").unwrap_err().to_string();
        assert!(
            other.starts_with("scanner: [package.metadata.nsh] `android.libraries`: libs/sdk.zip")
                && other.contains("neither a jar"),
            "{other}"
        );
        assert!(kind("libs/sdk").is_err());
    }

    #[test]
    fn missing_and_false_files_are_errors_and_copies_go_once() {
        let dir = dir("checked");
        let a = dir.join("a.jar");
        let b = dir.join("sub/b.jar");
        fs::create_dir_all(b.parent().unwrap()).unwrap();
        jar(&a, &[("x/A.class", b"a")]);
        fs::copy(&a, &b).unwrap();
        let listed = [
            library(&a, NSH_JSON),
            library(&b, "scanner: [package.metadata.nsh] `android.libraries`"),
        ];
        // The same contents, from two places: one library.
        let checked_ = checked(&listed).unwrap();
        assert_eq!(checked_.len(), 1);
        assert_eq!(checked_[0].path, a);
        assert!(checked_[0].apps);
        // A missing file names who lists it.
        let missing = [library(
            &dir.join("gone.jar"),
            "scanner: [package.metadata.nsh.features.scan] `android.libraries`",
        )];
        let error = checked(&missing).err().unwrap().to_string();
        assert!(
            error.starts_with(
                "scanner: [package.metadata.nsh.features.scan] `android.libraries`: read "
            ) && error.contains("gone.jar"),
            "{error}"
        );
        // A dex file that is no dex fails the build, not a class lookup on
        // the device.
        let fake = dir.join("fake.dex");
        fs::write(&fake, b"PK\x03\x04 a jar").unwrap();
        let error = checked(&[library(&fake, NSH_JSON)])
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("is not a dex file"), "{error}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn d8_turns_each_jar_into_intermediate_dex_then_merges() {
        let sdk = Sdk {
            root: PathBuf::from("/sdk"),
            build_tools: PathBuf::from("/sdk/build-tools/36.1.0"),
        };
        let args = |cmd: &Command| {
            cmd.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        let one = intermediate_command(
            &sdk,
            Path::new("/app/libs/sdk.jar"),
            Path::new("/sdk/platforms/android-35/android.jar"),
            false,
            Path::new("/cache/key"),
        );
        let d8 = if cfg!(windows) { "d8.bat" } else { "d8" };
        assert_eq!(one.get_program(), sdk.build_tools.join(d8));
        assert_eq!(
            args(&one),
            [
                "--intermediate",
                "--release",
                "--min-api",
                "26",
                "--lib",
                "/sdk/platforms/android-35/android.jar",
                "--output",
                "/cache/key",
                "/app/libs/sdk.jar",
            ]
        );
        let merge = merge_command(
            &sdk,
            &[
                PathBuf::from("/cache/a/classes.dex"),
                PathBuf::from("/cache/b/classes.dex"),
            ],
            true,
            Path::new("/work/libraries-dex"),
        );
        assert_eq!(
            args(&merge),
            [
                "--debug",
                "--min-api",
                "26",
                "--output",
                "/work/libraries-dex",
                "/cache/a/classes.dex",
                "/cache/b/classes.dex",
            ]
        );
    }

    #[test]
    fn a_class_defined_twice_names_the_libraries() {
        let a = checked_file(
            Path::new("/work/scanner/libs/glue.jar"),
            "scanner: [package.metadata.nsh] `android.libraries`",
        );
        let b = checked_file(Path::new("/app/libs/glue-copy.jar"), NSH_JSON);
        let intermediate = [
            (PathBuf::from("/cache/aaaa/classes.dex"), &a),
            (PathBuf::from("/cache/bbbb/classes.dex"), &b),
        ];
        // As d8 prints it.
        let output = "Error in /cache/bbbb/classes.dex:\n\
            Type com.example.glue.Hello is defined multiple times: /cache/bbbb/classes.dex, /cache/aaaa/classes.dex\n\
            Compilation failed\n\
            Exception in thread \"main\" java.lang.RuntimeException: ...\n\
            Caused by: com.android.tools.r8.internal.g: Type com.example.glue.Hello is defined multiple times: /cache/bbbb/classes.dex, /cache/aaaa/classes.dex\n";
        let error = merge_error(output, &intermediate);
        assert_eq!(
            error,
            "the Java libraries define a class more than once:\n\
             `com.example.glue.Hello` is in /app/libs/glue-copy.jar (nsh.json `android.libraries`) and \
             /work/scanner/libs/glue.jar (scanner: [package.metadata.nsh] `android.libraries`)\n\
             list one of the files that define it"
        );
        // Another error: d8's words, without the stack trace, about the
        // libraries.
        let error = merge_error(
            "Error in /cache/aaaa/classes.dex:\nSomething else\nException in thread \"main\" x\n\tat y",
            &intermediate,
        );
        assert!(error.contains("glue.jar (scanner:"), "{error}");
        assert!(
            !error.contains("Exception") && !error.contains("/cache/"),
            "{error}"
        );
    }

    #[test]
    fn dex_files_are_in_their_order() {
        let dir = dir("order");
        for name in [
            "classes10.dex",
            "classes2.dex",
            "classes.dex",
            "other.txt",
            "classesx.dex",
        ] {
            fs::write(dir.join(name), b"").unwrap();
        }
        let names: Vec<String> = dex_files(&dir)
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["classes.dex", "classes2.dex", "classes10.dex"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn jars_bring_their_java_resources() {
        let dir = dir("resources");
        let a = dir.join("a.jar");
        let b = dir.join("b.jar");
        jar(
            &a,
            &[
                ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n"),
                ("META-INF/SIGNER.SF", b"sig"),
                ("META-INF/SIGNER.RSA", b"sig"),
                ("META-INF/LICENSE.txt", b"license a"),
                ("META-INF/maven/a/a/pom.xml", b"<project/>"),
                (
                    "META-INF/services/com.example.Greeter",
                    b"com.example.a.Hello\n",
                ),
                ("com/example/a/Hello.class", b"class"),
                ("com/example/a/messages.properties", b"hi=a"),
                (
                    "kotlin/collections/collections.kotlin_builtins",
                    b"builtins",
                ),
            ],
        );
        jar(
            &b,
            &[
                ("META-INF/LICENSE.txt", b"license b"),
                (
                    "META-INF/services/com.example.Greeter",
                    b"com.example.b.Hello",
                ),
                (
                    "kotlin/collections/collections.kotlin_builtins",
                    b"builtins",
                ),
            ],
        );
        let (la, lb) = (
            checked_file(&a, NSH_JSON),
            checked_file(&b, "x: [package.metadata.nsh] `android.libraries`"),
        );
        let resources = java_resources(&[&la, &lb]).unwrap();
        let names: Vec<&str> = resources.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "META-INF/services/com.example.Greeter",
                "com/example/a/messages.properties",
                "kotlin/collections/collections.kotlin_builtins",
            ]
        );
        // Both jars' providers.
        assert_eq!(resources[0].1, b"com.example.a.Hello\ncom.example.b.Hello");
        // Another file the two have with different contents: an error
        // naming both.
        jar(&b, &[("com/example/a/messages.properties", b"hi=b")]);
        let error = java_resources(&[&la, &lb]).unwrap_err().to_string();
        assert!(
            error.contains("a.jar (nsh.json `android.libraries`) and")
                && error.contains("b.jar (x: [package.metadata.nsh] `android.libraries`)")
                && error.contains("`com/example/a/messages.properties`"),
            "{error}"
        );
        // A path the package keeps for its own files.
        for name in [
            "lib/arm64-v8a/libx.so",
            "classes2.dex",
            "assets/x",
            "AndroidManifest.xml",
        ] {
            jar(&b, &[(name, b"x")]);
            let error = java_resources(&[&lb]).unwrap_err().to_string();
            assert!(error.contains("keeps for its own files"), "{name}: {error}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The manifest of an AAR of `package`, with `application`'s children.
    fn aar_manifest(package: &str, application: &str) -> Vec<u8> {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="{package}">
    <uses-sdk android:minSdkVersion="21" />
    <uses-permission android:name="android.permission.WAKE_LOCK" />
    <uses-permission android:name="android.permission.INTERNET" />
    <application>{application}</application>
</manifest>"#
        )
        .into_bytes()
    }

    const CRATE: &str = "push: [package.metadata.nsh] `android.libraries`";

    #[test]
    fn an_aar_brings_its_classes_native_libraries_assets_and_manifest() {
        let dir = dir("aar");
        let path = dir.join("push.aar");
        let manifest = aar_manifest(
            "com.example.push",
            r#"<provider android:name=".PushInit" android:authorities="${applicationId}.push-init" android:exported="false" />"#,
        );
        jar(
            &path,
            &[
                ("AndroidManifest.xml", &manifest),
                ("classes.jar", b"classes"),
                ("libs/vendor.jar", b"vendor"),
                ("libs/deeper/not.jar", b"x"),
                ("jni/arm64-v8a/libpush.so", b"arm64"),
                ("jni/x86_64/libpush.so", b"x86_64"),
                ("jni/arm64-v8a/notes.txt", b"x"),
                ("assets/push/config.json", b"{}"),
                ("R.txt", b""),
                ("proguard.txt", b"-keep class x"),
                ("lint.jar", b"lint"),
                (AAR_METADATA, b"aarFormatVersion=1.0\nminCompileSdk=34\n"),
            ],
        );
        let aar = read_aar(checked(&[library(&path, CRATE)]).unwrap().remove(0)).unwrap();
        assert_eq!(aar.package, "com.example.push");
        let jars: Vec<&str> = aar.jars.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(jars, ["classes.jar", "libs/vendor.jar"]);
        let abis: Vec<&str> = aar.native.keys().map(String::as_str).collect();
        assert_eq!(abis, ["arm64-v8a", "x86_64"]);
        assert_eq!(
            aar.native["arm64-v8a"],
            [("libpush.so".into(), b"arm64".to_vec())]
        );
        assert_eq!(aar.assets, [("push/config.json".into(), b"{}".to_vec())]);
        assert_eq!(aar.min_compile_sdk, Some(34));
        // What the package cannot take yet, or that is NSH's.
        for (entry, error) in [
            (
                "res/values/values.xml",
                "has Android resources (`res/values/values.xml`), which this nsh does not package yet",
            ),
            ("assets/nsh/bundle.nshc", "`assets/nsh/` is NSH's own"),
        ] {
            jar(&path, &[("AndroidManifest.xml", &manifest), (entry, b"x")]);
            let found = read_aar(checked(&[library(&path, CRATE)]).unwrap().remove(0))
                .err()
                .unwrap()
                .to_string();
            assert!(
                found.starts_with(&format!("{} ({CRATE})", path.display()))
                    && found.contains(error),
                "{found}"
            );
        }
        jar(&path, &[("classes.jar", b"x")]);
        let found = read_aar(checked(&[library(&path, CRATE)]).unwrap().remove(0))
            .err()
            .unwrap()
            .to_string();
        assert!(found.contains("has no AndroidManifest.xml"), "{found}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn one_version_of_an_aar_goes_in_and_the_app_settles_which() {
        let dir = dir("aar-versions");
        let aar = |name: &str, package: &str, asset: &[u8], listed: &str| {
            let path = dir.join(name);
            jar(
                &path,
                &[
                    ("AndroidManifest.xml", &aar_manifest(package, "")),
                    ("assets/version.txt", asset),
                ],
            );
            library(&path, listed)
        };
        let chosen = |libraries: &[Library]| -> anyhow::Result<Vec<String>> {
            let aars = checked(libraries)?
                .into_iter()
                .map(read_aar)
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(choose(aars)?
                .into_iter()
                .map(|aar| aar.checked.shown)
                .collect())
        };
        let other = "maps: [package.metadata.nsh] `android.libraries`";
        let one = aar("core-1.aar", "androidx.core", b"1", CRATE);
        let same = aar("core-1-copy.aar", "androidx.core", b"1", other);
        let two = aar("core-2.aar", "androidx.core", b"2", other);
        let apps = aar("core-3.aar", "androidx.core", b"3", NSH_JSON);
        let maps = aar("maps.aar", "com.example.maps", b"m", other);
        // The same file twice: once.
        assert_eq!(chosen(&[one.clone(), same, maps.clone()]).unwrap().len(), 2);
        // Two versions from crates: an error naming both.
        let error = chosen(&[one.clone(), two.clone()]).unwrap_err().to_string();
        assert!(
            error.contains("core-1.aar (push:")
                && error.contains("core-2.aar (maps:")
                && error.contains("two versions of the library `androidx.core`")
                && error.contains("list the one to package in nsh.json `android.libraries`"),
            "{error}"
        );
        // The app's own wins.
        let found = chosen(&[apps.clone(), one, two, maps]).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found[0].contains("core-3.aar (nsh.json"), "{found:?}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn native_libraries_go_per_abi_the_package_has() {
        let native = |from: &str, abis: &[(&str, &str, &[u8])]| {
            let mut map: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
            for (abi, name, bytes) in abis {
                map.entry((*abi).into())
                    .or_default()
                    .push(((*name).into(), bytes.to_vec()));
            }
            Native {
                from: from.into(),
                abis: map,
            }
        };
        let libraries = [
            native(
                "push.aar",
                &[
                    ("arm64-v8a", "libpush.so", b"a"),
                    ("x86_64", "libpush.so", b"x"),
                    ("arm64-v8a", "libc++_shared.so", b"c"),
                ],
            ),
            native("maps.aar", &[("arm64-v8a", "libc++_shared.so", b"c")]),
        ];
        let names = |abi: &str| -> anyhow::Result<Vec<String>> {
            Ok(native_for(&libraries, abi)?
                .into_iter()
                .map(|(name, _)| name.to_owned())
                .collect())
        };
        assert_eq!(
            names("arm64-v8a").unwrap(),
            ["libpush.so", "libc++_shared.so"]
        );
        // An ABI the package has and a library has not.
        let error = names("x86_64").unwrap_err().to_string();
        assert!(
            error.starts_with("maps.aar has native libraries for arm64-v8a, not for x86_64"),
            "{error}"
        );
        // Two libraries of one name.
        let clash = [
            native("push.aar", &[("arm64-v8a", "libc++_shared.so", b"1")]),
            native("maps.aar", &[("arm64-v8a", "libc++_shared.so", b"2")]),
        ];
        let error = native_for(&clash, "arm64-v8a").unwrap_err().to_string();
        assert!(
            error.contains("push.aar and maps.aar both have `libc++_shared.so` for arm64-v8a"),
            "{error}"
        );
        let nsh = [native("push.aar", &[("arm64-v8a", "libnsh.so", b"1")])];
        let error = native_for(&nsh, "arm64-v8a").unwrap_err().to_string();
        assert!(error.contains("the name of NSH's own library"), "{error}");
        assert!(stored_asset("sounds/ding.OGG") && !stored_asset("config.json"));
    }
}
