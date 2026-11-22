//! The Java libraries an Android package carries (`android.libraries` of the
//! app's `nsh.json` and of its crates' `[package.metadata.nsh]`): jar files,
//! which the SDK's `d8` turns into dex, and prebuilt dex files. A package
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

use super::{Apk, MIN_SDK, Sdk};

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
            Some("aar") => bail!(
                "{}: {} is an Android library (AAR), which this nsh does not package yet: list jar and dex files",
                self.listed,
                self.path.display()
            ),
            _ => bail!(
                "{}: {} is neither a jar (`.jar`) nor a dex file (`.dex`)",
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

/// The Java code [`super::add_files`] packages after the runtime's.
#[derive(Debug, Default)]
pub(super) struct JavaCode {
    /// Dex files, numbered in this order after the runtime's
    /// `classes.dex`: `classes2.dex`, `classes3.dex`, ...
    pub(super) dex: Vec<PathBuf>,
    /// The jars' Java resources (`META-INF/services/...` and the like):
    /// (path at the package's root, contents).
    pub(super) resources: Vec<(String, Vec<u8>)>,
}

/// A library to package, checked.
struct Checked<'a> {
    library: &'a Library,
    kind: Kind,
    sha256: String,
}

/// The libraries of `apk`, checked and each file once: a missing file or
/// one of another kind is an error naming who lists it; files with the same
/// contents (two crates that carry one jar) are packaged once.
fn checked(libraries: &[Library]) -> anyhow::Result<Vec<Checked<'_>>> {
    let mut out: Vec<Checked<'_>> = Vec::new();
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
            library,
            kind,
            sha256,
        });
    }
    Ok(out)
}

/// Turns the libraries of `apk` into the dex files and Java resources the
/// package carries: dex files as they are, in the order listed; the jars
/// each through `d8 --intermediate` (kept in `apk.library_cache` by
/// contents, `d8` version, API level and mode), then merged by one `d8` run
/// into `work`, which fails on a class two libraries define.
pub(super) fn java_code(sdk: &Sdk, apk: &Apk<'_>, work: &Path) -> anyhow::Result<JavaCode> {
    if apk.libraries.is_empty() {
        return Ok(JavaCode::default());
    }
    let libraries = checked(apk.libraries)?;
    let mut code = JavaCode::default();
    let jars: Vec<&Checked<'_>> = libraries.iter().filter(|c| c.kind == Kind::Jar).collect();
    code.dex.extend(
        libraries
            .iter()
            .filter(|c| c.kind == Kind::Dex)
            .map(|c| c.library.path.clone()),
    );
    if jars.is_empty() {
        return Ok(code);
    }
    let android_jar = sdk.android_jar(apk.target_sdk)?;
    // Each intermediate dex, with the library it is made of.
    let mut intermediate: Vec<(PathBuf, &Library)> = Vec::new();
    for jar in &jars {
        let dir = intermediate_dex(sdk, jar, &android_jar, apk)?;
        for dex in dex_files(&dir)? {
            intermediate.push((dex, jar.library));
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
    code.dex.extend(dex_files(&merged)?);
    code.resources = java_resources(&jars.iter().map(|c| c.library).collect::<Vec<_>>())?;
    Ok(code)
}

/// The directory of `jar`'s intermediate dex in the cache, made when
/// missing.
fn intermediate_dex(
    sdk: &Sdk,
    jar: &Checked<'_>,
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
    let output = intermediate_command(sdk, &jar.library.path, android_jar, apk.dev, &partial)
        .output()
        .context("run the SDK's d8 (is a JDK installed?)")?;
    if !output.status.success() {
        let _ = fs::remove_dir_all(&partial);
        bail!(
            "{}: d8 could not read {}:\n{}",
            jar.library.listed,
            jar.library.path.display(),
            d8_message(&format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        );
    }
    fs::write(partial.join("done"), b"")?;
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    if fs::rename(&partial, &dir).is_err() {
        // Another run moved its own copy in first: the same dex.
        let _ = fs::remove_dir_all(&partial);
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
fn merge_error(output: &str, intermediate: &[(PathBuf, &Library)]) -> String {
    let library = |path: &str| {
        intermediate
            .iter()
            .find(|(dex, _)| dex.to_string_lossy() == path.trim())
            .map(|(_, library)| library.describe())
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
        message = message.replace(&*dex.to_string_lossy(), &library.describe());
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
fn java_resources(jars: &[&Library]) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    // Each path's index in `out`, and the jar it came from.
    let mut seen: BTreeMap<String, (usize, &Library)> = BTreeMap::new();
    for jar in jars {
        let file = fs::File::open(&jar.path)
            .with_context(|| format!("{}: read {}", jar.listed, jar.path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("{}: {} is not a jar", jar.listed, jar.path.display()))?;
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
                    jar.path.display()
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
                    first.describe(),
                    jar.describe()
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

    /// A jar at `path` with `entries` (name, contents).
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
        let aar = kind("libs/sdk.aar").unwrap_err().to_string();
        assert!(
            aar.starts_with("scanner: [package.metadata.nsh] `android.libraries`: libs/sdk.aar")
                && aar.contains("does not package yet"),
            "{aar}"
        );
        let other = kind("libs/sdk.zip").unwrap_err().to_string();
        assert!(other.contains("neither a jar"), "{other}");
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
        assert_eq!(checked_[0].library.path, a);
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
        let a = library(
            Path::new("/work/scanner/libs/glue.jar"),
            "scanner: [package.metadata.nsh] `android.libraries`",
        );
        let b = library(Path::new("/app/libs/glue-copy.jar"), NSH_JSON);
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
            library(&a, NSH_JSON),
            library(&b, "x: [package.metadata.nsh] `android.libraries`"),
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
}
