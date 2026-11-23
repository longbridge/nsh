//! `ios.frameworks` and `ios.linkerFlags`, of the app's `nsh.json` and of
//! its crates' `[package.metadata.nsh]`: the third-party libraries an iOS
//! app links (an SDK's `.xcframework` or `.framework`) and the flags its
//! executable is linked with.
//!
//! Every iOS executable built with the app's crates links them: the
//! development hosts of the simulator and Mac Catalyst and the release
//! runner. A build takes each XCFramework's library for its target
//! ([`Framework::library`]) and passes the arguments ([`link`]) through the
//! generated crate's `build.rs` ([`BUILD_RS`]): Cargo gives a dependency's
//! `rustc-link-arg` to that dependency's own targets only, never to the
//! executable that links it. A static library becomes part of the
//! executable; a dynamic framework is linked against, copied into the
//! bundle's `Frameworks/` ([`sync`]) and signed before the app, whose
//! signature seals it ([`sign`]). A dynamic framework keeps its own files,
//! its privacy manifest among them, where App Store Connect reads it. A
//! static library's code is the app's, and so is what its privacy
//! manifest declares: packages add it to the app's own
//! ([`add_static_privacy`]).
//!
//! An entry may be for some targets only (`targets`): an SDK without a
//! Mac Catalyst library is linked for devices and the simulator, and
//! another entry of the same name may serve Catalyst. An entry may be
//! `optional`: an SDK the app fetches before some builds only (it is not
//! in the repository) is left out, with a warning, of the builds it is
//! not there for. Both are settled per build ([`Framework::for_target`]),
//! before `cargo build`, which links what this module resolved.

use std::{
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};
use serde::Deserialize;

/// The variable that carries an iOS build's link arguments to the
/// generated crate's `build.rs`, separated by U+001F.
pub(crate) const LINK_ARGS_VAR: &str = "NSH_IOS_LINK_ARGS";

/// The variable that names the files those arguments link, separated by
/// U+001F: a new build of a library links the executable again.
pub(crate) const LINK_INPUTS_VAR: &str = "NSH_IOS_LINK_INPUTS";

/// The `build.rs` of the generated iOS executables (`ios-host`, `ios-app`,
/// `rust-ios`): the link arguments `nsh` gives the build ([`link_env`]).
pub(crate) const BUILD_RS: &str = r#"//! The app's iOS frameworks and linker flags (nsh.json and its crates'
//! `ios.frameworks`, `ios.linkerFlags`), which `nsh` passes for this
//! executable's link: Cargo keeps a dependency's `rustc-link-arg` to that
//! dependency.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NSH_IOS_LINK_ARGS");
    println!("cargo:rerun-if-env-changed=NSH_IOS_LINK_INPUTS");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("ios") {
        return;
    }
    let list = |name: &str| std::env::var(name).unwrap_or_default();
    for arg in list("NSH_IOS_LINK_ARGS").split('\u{1f}').filter(|a| !a.is_empty()) {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
    for input in list("NSH_IOS_LINK_INPUTS").split('\u{1f}').filter(|i| !i.is_empty()) {
        println!("cargo:rerun-if-changed={input}");
    }
}
"#;

/// An iOS target an entry of `ios.frameworks` is linked for, named as
/// `nsh package --target` and `nsh run --target` name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum Target {
    /// iOS devices.
    #[serde(rename = "ios")]
    Ios,
    /// The iOS simulator.
    #[serde(rename = "ios-simulator")]
    IosSimulator,
    /// Mac Catalyst.
    #[serde(rename = "catalyst")]
    Catalyst,
}

impl Target {
    /// The target of the iOS `triple`.
    fn of(triple: &str) -> anyhow::Result<Target> {
        Ok(match xcframework_target(triple)?.0 {
            None => Target::Ios,
            Some("simulator") => Target::IosSimulator,
            Some(_) => Target::Catalyst,
        })
    }
}

/// One of `ios.frameworks`: a path, or `{ "path": .., "forceLoad": true,
/// "targets": [..], "optional": true }`.
/// It is read from any value, so an entry the reader cannot take is left
/// out by [`read`] where it stands: the others keep the index the app
/// wrote, which errors name.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "serde_json::Value")]
pub(crate) struct Framework {
    /// The `.xcframework` or `.framework`. One `nsh.json` lists is
    /// relative to `nsh.json` until [`read`] resolves it; a crate's is
    /// resolved against the directory of the crate's `Cargo.toml`.
    pub(crate) path: PathBuf,
    /// Links every member of a static library (`-force_load`), not only
    /// those the app's code refers to: the Objective-C categories and
    /// classes an SDK only looks up at run time.
    pub(crate) force_load: bool,
    /// The targets it is linked for; empty: every target.
    pub(crate) targets: Vec<Target>,
    /// A build it is not there for leaves it out, with a warning, rather
    /// than stopping: an SDK a script fetches before some builds only.
    pub(crate) optional: bool,
    /// Who lists it, for errors: `nsh.json `ios.frameworks[0]`` or
    /// `<crate>: [package.metadata.nsh] `ios.frameworks[0]``.
    pub(crate) listed: String,
    /// Why the entry cannot be read, when it cannot.
    invalid: Option<String>,
    /// The entry's keys the reader does not know.
    unknown: Vec<String>,
}

impl From<serde_json::Value> for Framework {
    fn from(value: serde_json::Value) -> Self {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Entry {
            path: PathBuf,
            #[serde(default)]
            force_load: bool,
            targets: Option<Vec<Target>>,
            #[serde(default)]
            optional: bool,
        }
        if let serde_json::Value::String(path) = &value {
            return Framework {
                path: path.into(),
                ..Framework::default()
            };
        }
        let mut unknown = Vec::new();
        let invalid = |why: String| Framework {
            invalid: Some(why),
            ..Framework::default()
        };
        match serde_ignored::deserialize::<_, _, Entry>(&value, |key| unknown.push(key.to_string()))
        {
            Ok(Entry {
                targets: Some(targets),
                ..
            }) if targets.is_empty() => invalid(
                "`targets` names at least one target (`ios`, `ios-simulator`, `catalyst`); \
                 leave it out for all of them"
                    .to_owned(),
            ),
            Ok(entry) => Framework {
                path: entry.path,
                force_load: entry.force_load,
                targets: entry.targets.unwrap_or_default(),
                optional: entry.optional,
                unknown,
                ..Framework::default()
            },
            Err(e) => invalid(format!(
                "a framework is a path or {{ \"path\", \"forceLoad\", \"targets\", \"optional\" }}: {e}"
            )),
        }
    }
}

/// `ios.frameworks` of `nsh.json` as read (`entries`), each relative to
/// `dir`, `nsh.json`'s directory; an entry that cannot be read is left
/// out, reported with the unknown keys in `ignored`, which `read_config`
/// warns of.
pub(crate) fn read(
    entries: Vec<Framework>,
    dir: &Path,
    ignored: &mut Vec<String>,
) -> Vec<Framework> {
    entries
        .into_iter()
        .enumerate()
        .filter_map(|(index, framework)| {
            let at = format!("ios.frameworks[{index}]");
            ignored.extend(
                framework
                    .unknown
                    .iter()
                    .map(|key| format!("unknown key `{at}.{key}`")),
            );
            if let Some(why) = &framework.invalid {
                ignored.push(format!("{at}: {why}"));
                return None;
            }
            Some(Framework {
                path: dir.join(&framework.path),
                listed: format!("nsh.json `{at}`"),
                ..framework
            })
        })
        .collect()
}

/// A crate's `ios.frameworks` (`value`), relative to its directory `dir`;
/// `listed` names the list in errors. An entry of another form is an
/// error, as a crate's other values are; unknown keys are reported in
/// `warnings`.
pub(crate) fn of_crate(
    value: &serde_json::Value,
    dir: &Path,
    listed: &str,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Vec<Framework>> {
    let entries = value
        .as_array()
        .with_context(|| format!("{listed} is a list"))?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let framework = Framework::from(entry.clone());
            let at = format!("{listed}[{index}]");
            if let Some(why) = &framework.invalid {
                bail!("{at}: {why}");
            }
            warnings.extend(
                framework
                    .unknown
                    .iter()
                    .map(|key| format!("{at}: `{key}` is not something this nsh reads: left out")),
            );
            Ok(Framework {
                path: dir.join(&framework.path),
                listed: at,
                ..framework
            })
        })
        .collect()
}

/// `ios.linkerFlags` (`value`): a list of strings, each one argument of
/// the link. `listed` names it in errors.
pub(crate) fn linker_flags(value: &serde_json::Value, listed: &str) -> anyhow::Result<Vec<String>> {
    let flags = value
        .as_array()
        .and_then(|list| {
            list.iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        })
        .with_context(|| format!("{listed} is a list of strings"))?;
    for flag in &flags {
        check_flag(flag).with_context(|| listed.to_owned())?;
    }
    Ok(flags)
}

/// A flag is one argument: not empty, and without the line breaks and
/// separators that would split it on its way to the linker.
pub(crate) fn check_flag(flag: &str) -> anyhow::Result<()> {
    if flag.is_empty() {
        bail!("a flag is not empty");
    }
    if flag.contains(['\n', '\r', '\u{1f}']) {
        bail!("{flag:?} is one argument, on one line");
    }
    Ok(())
}

/// What a framework links for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Library {
    /// A static library (`lib<name>.a`).
    Archive(PathBuf),
    /// A framework bundle, with its executable (`<Name>.framework/<Name>`).
    Framework {
        bundle: PathBuf,
        binary: PathBuf,
        dynamic: bool,
    },
}

impl Library {
    /// The file the linker reads.
    fn input(&self) -> &Path {
        match self {
            Library::Archive(path) => path,
            Library::Framework { binary, .. } => binary,
        }
    }
}

/// The platform variant and processor of an iOS `triple`, as XCFrameworks
/// name them: `None` for devices, `simulator`, `maccatalyst`.
fn xcframework_target(triple: &str) -> anyhow::Result<(Option<&'static str>, &'static str)> {
    let arch = match triple.split('-').next() {
        Some("aarch64") => "arm64",
        Some("x86_64") => "x86_64",
        _ => bail!("{triple} is not an iOS target"),
    };
    let variant = match triple.rsplit_once("-apple-ios") {
        Some((_, "")) if arch == "arm64" => None,
        // The Intel triple of iOS is the simulator's.
        Some((_, "")) => Some("simulator"),
        Some((_, "-sim")) => Some("simulator"),
        Some((_, "-macabi")) => Some("maccatalyst"),
        _ => bail!("{triple} is not an iOS target"),
    };
    Ok((variant, arch))
}

/// What the target of `triple` is called in errors.
fn target_name(triple: &str) -> String {
    match xcframework_target(triple) {
        Ok((None, arch)) => format!("iOS devices ({arch})"),
        Ok((Some("simulator"), arch)) => format!("the iOS simulator ({arch})"),
        Ok((Some(_), arch)) => format!("Mac Catalyst ({arch})"),
        Err(_) => triple.to_owned(),
    }
}

impl Framework {
    /// `path`, and who lists it.
    fn describe(&self) -> String {
        format!("{} ({})", self.path.display(), self.listed)
    }

    /// What it links for `triple`: nothing for a target it is not for
    /// (`targets`), nor when it is `optional` and not there; else
    /// [`Self::library`].
    pub(crate) fn for_target(&self, triple: &str) -> anyhow::Result<Option<Library>> {
        if !self.is_for(triple)? || self.is_missing() {
            return Ok(None);
        }
        self.library(triple).map(Some)
    }

    /// `triple` is one of its `targets`.
    fn is_for(&self, triple: &str) -> anyhow::Result<bool> {
        Ok(self.targets.is_empty() || self.targets.contains(&Target::of(triple)?))
    }

    /// It is `optional` and not there.
    fn is_missing(&self) -> bool {
        // A broken link or one that cannot be read is there: its error
        // tells more than leaving it out would.
        self.optional && fs::symlink_metadata(&self.path).is_err()
    }

    /// What it links for `triple`: an XCFramework's library for that
    /// target, or the framework as it is.
    pub(crate) fn library(&self, triple: &str) -> anyhow::Result<Library> {
        let extension = self.path.extension().and_then(|e| e.to_str());
        if !self.path.is_dir() {
            bail!(
                "{}: {} is not a directory: list an `.xcframework` or a `.framework`",
                self.listed,
                self.path.display()
            );
        }
        let path = match extension {
            Some("xcframework") => self.xcframework_library(triple)?,
            Some("framework") => self.path.clone(),
            _ => bail!(
                "{}: {} is neither an XCFramework (`.xcframework`) nor a framework (`.framework`)",
                self.listed,
                self.path.display()
            ),
        };
        match path.extension().and_then(|e| e.to_str()) {
            Some("a") => Ok(Library::Archive(path)),
            Some("framework") => {
                let name = path
                    .file_stem()
                    .context("a framework has a name")?
                    .to_owned();
                let binary = path.join(&name);
                let dynamic = is_dynamic(&binary)
                    .with_context(|| format!("{}: {}", self.listed, self.path.display()))?;
                Ok(Library::Framework {
                    bundle: path,
                    binary,
                    dynamic,
                })
            }
            _ => bail!(
                "{}: {} is not a static library (`.a`) or a framework: iOS apps carry dynamic \
                 libraries as frameworks",
                self.listed,
                path.display()
            ),
        }
    }

    /// The library of an XCFramework for `triple`, by its `Info.plist`'s
    /// `AvailableLibraries`.
    fn xcframework_library(&self, triple: &str) -> anyhow::Result<PathBuf> {
        let (variant, arch) = xcframework_target(triple)?;
        let info_path = self.path.join("Info.plist");
        let info = plist::Value::from_file(&info_path)
            .with_context(|| format!("{}: cannot read {}", self.listed, info_path.display()))?;
        let libraries = info
            .as_dictionary()
            .and_then(|d| d.get("AvailableLibraries"))
            .and_then(plist::Value::as_array)
            .with_context(|| {
                format!(
                    "{}: {} lists no AvailableLibraries",
                    self.listed,
                    info_path.display()
                )
            })?;
        let text = |library: &plist::Dictionary, key: &str| {
            library
                .get(key)
                .and_then(plist::Value::as_string)
                .map(str::to_owned)
        };
        let mut available = Vec::new();
        for library in libraries.iter().filter_map(plist::Value::as_dictionary) {
            let (Some(identifier), Some(path)) = (
                text(library, "LibraryIdentifier"),
                text(library, "LibraryPath"),
            ) else {
                continue;
            };
            let platform = text(library, "SupportedPlatform");
            let library_variant = text(library, "SupportedPlatformVariant");
            let archs: Vec<&str> = library
                .get("SupportedArchitectures")
                .and_then(plist::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(plist::Value::as_string)
                .collect();
            if platform.as_deref() == Some("ios")
                && library_variant.as_deref() == variant
                && archs.contains(&arch)
            {
                return Ok(self.path.join(identifier).join(path));
            }
            available.push(identifier);
        }
        bail!(
            "{}: {} has no library for {} (it has {}): name the targets it is for in the \
             entry's `targets`",
            self.listed,
            self.path.display(),
            target_name(triple),
            if available.is_empty() {
                "none".to_owned()
            } else {
                available.join(", ")
            }
        )
    }
}

/// Mach-O's file type of a dynamic library (`MH_DYLIB`).
const MH_DYLIB: u32 = 6;

/// Whether the framework executable `binary` is a dynamic library (a
/// Mach-O `MH_DYLIB`, or a universal file of them) rather than a static
/// archive (`!<arch>`, or a universal file of them).
fn is_dynamic(binary: &Path) -> anyhow::Result<bool> {
    let mut head = [0u8; 4096];
    let read = fs::File::open(binary)
        .and_then(|mut file| file.read(&mut head))
        .with_context(|| format!("{} has no executable", binary.display()))?;
    let head = &head[..read];
    let kind = |at: usize| -> Option<bool> {
        let bytes = head.get(at..)?;
        if bytes.starts_with(b"!<arch>\n") {
            return Some(false);
        }
        let magic = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
        // 32- and 64-bit headers, little-endian as every Apple processor.
        if matches!(magic, 0xfeed_face | 0xfeed_facf) {
            let file_type = u32::from_le_bytes(bytes.get(12..16)?.try_into().ok()?);
            return Some(file_type == MH_DYLIB);
        }
        None
    };
    let be = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(head.get(at..at + 4)?.try_into().ok()?))
    };
    let found = match be(0) {
        // A universal file: its first slice tells, the slices being one
        // library built for several processors.
        Some(0xcafe_babe) => be(16).and_then(|offset| {
            let offset = offset as usize;
            if offset + 16 <= head.len() {
                kind(offset)
            } else {
                read_at(binary, offset).ok().flatten()
            }
        }),
        Some(0xcafe_babf) => {
            be(20).and_then(|offset| read_at(binary, offset as usize).ok().flatten())
        }
        _ => kind(0),
    };
    found.with_context(|| {
        format!(
            "{} is neither a static library nor a dynamic one",
            binary.display()
        )
    })
}

/// [`is_dynamic`] of the slice at `offset` of a universal file.
fn read_at(binary: &Path, offset: usize) -> std::io::Result<Option<bool>> {
    use std::io::Seek as _;
    let mut file = fs::File::open(binary)?;
    file.seek(std::io::SeekFrom::Start(offset as u64))?;
    let mut head = [0u8; 16];
    let read = file.read(&mut head)?;
    let head = &head[..read];
    if head.starts_with(b"!<arch>\n") {
        return Ok(Some(false));
    }
    let word = |at: usize| {
        head.get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    Ok(match word(0) {
        Some(0xfeed_face | 0xfeed_facf) => word(12).map(|t| t == MH_DYLIB),
        _ => None,
    })
}

/// The link of an iOS executable: its arguments and the files they link.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Link {
    /// Arguments of the linker driver (`cc`), in order.
    pub(crate) args: Vec<String>,
    /// The libraries they link.
    pub(crate) inputs: Vec<PathBuf>,
    /// The `optional` frameworks left out, not being there, which the
    /// build warns of.
    pub(crate) warnings: Vec<String>,
}

/// The link of an executable for `triple` with `frameworks` and `flags`:
/// the libraries first, then the flags as the app wrote them. Frameworks
/// that are not for `triple`, or `optional` and not there, are left out
/// ([`Framework::for_target`]); two of one name are an error only when
/// both are linked.
pub(crate) fn link(
    frameworks: &[Framework],
    flags: &[String],
    triple: &str,
) -> anyhow::Result<Link> {
    let mut link = Link::default();
    let mut search: Vec<String> = Vec::new();
    let mut names: Vec<(String, &Framework)> = Vec::new();
    let mut dynamic = false;
    for framework in frameworks {
        let Some(library) = framework.for_target(triple)? else {
            if framework.is_missing() && framework.is_for(triple)? {
                link.warnings.push(format!(
                    "{}: {} is not there: this build leaves it out (`optional`)",
                    framework.listed,
                    framework.path.display()
                ));
            }
            continue;
        };
        let name = match &library {
            Library::Archive(path) => path.file_name(),
            Library::Framework { bundle, .. } => bundle.file_name(),
        }
        .context("a library has a name")?
        .to_string_lossy()
        .into_owned();
        // `-framework <Name>` finds one of a name, and `Frameworks/` holds
        // one.
        if let Some((_, earlier)) = names.iter().find(|(n, _)| *n == name) {
            bail!(
                "{} and {} are both {name}",
                earlier.describe(),
                framework.describe()
            );
        }
        names.push((name, framework));
        let path = |p: &Path| p.to_string_lossy().into_owned();
        match &library {
            Library::Archive(archive) if framework.force_load => {
                link.args.extend(["-force_load".to_owned(), path(archive)]);
            }
            Library::Archive(archive) => link.args.push(path(archive)),
            Library::Framework {
                bundle,
                binary,
                dynamic: is_dynamic,
            } => {
                if *is_dynamic && framework.force_load {
                    bail!(
                        "{}: forceLoad links all of a static library, and {} is a dynamic framework",
                        framework.listed,
                        bundle.display()
                    );
                }
                let dir = bundle.parent().context("a framework is in a directory")?;
                let dir = format!("-F{}", path(dir));
                if !search.contains(&dir) {
                    search.push(dir.clone());
                    link.args.push(dir);
                }
                if framework.force_load {
                    link.args.extend(["-force_load".to_owned(), path(binary)]);
                } else {
                    let stem = bundle.file_stem().context("a framework has a name")?;
                    link.args
                        .extend(["-framework".to_owned(), stem.to_string_lossy().into_owned()]);
                }
                dynamic |= is_dynamic;
            }
        }
        link.inputs.push(library.input().to_owned());
    }
    if dynamic {
        // Where the executable finds its frameworks: an iOS bundle's
        // `Frameworks/` is next to it, a Mac Catalyst app's in `Contents/`.
        let (variant, _) = xcframework_target(triple)?;
        link.args.push(if variant == Some("maccatalyst") {
            "-Wl,-rpath,@executable_path/../Frameworks".to_owned()
        } else {
            "-Wl,-rpath,@executable_path/Frameworks".to_owned()
        });
    }
    for flag in flags {
        check_flag(flag)?;
        link.args.push(flag.clone());
    }
    Ok(link)
}

/// The variables a `cargo build` of an iOS executable for `triple` sets
/// for [`BUILD_RS`]: none without frameworks or flags. They also carry
/// the libraries' sizes and times, so a build `nsh` keeps
/// (`rust_app::BuiltOnce`, which reads the command's variables) is made
/// again for a new build of a library.
pub(crate) fn link_env(link: &Link) -> Vec<(&'static str, String)> {
    if link.args.is_empty() {
        return Vec::new();
    }
    let inputs: Vec<String> = link
        .inputs
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let stamps: Vec<String> = link.inputs.iter().map(|p| stamp(p)).collect();
    vec![
        (LINK_ARGS_VAR, link.args.join("\u{1f}")),
        (LINK_INPUTS_VAR, inputs.join("\u{1f}")),
        ("NSH_IOS_LINK_STAMPS", stamps.join("\u{1f}")),
    ]
}

/// A file's size and modification time, which a new build of it changes.
fn stamp(path: &Path) -> String {
    let Ok(meta) = fs::metadata(path) else {
        return String::new();
    };
    let time = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!("{}:{time}", meta.len())
}

/// The dynamic frameworks of `frameworks` for `triple`, which a bundle
/// carries: those [`link`] linked.
pub(crate) fn dynamic(frameworks: &[Framework], triple: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut bundles = Vec::new();
    for framework in frameworks {
        if let Some(Library::Framework {
            bundle,
            dynamic: true,
            ..
        }) = framework.for_target(triple)?
        {
            bundles.push(bundle);
        }
    }
    Ok(bundles)
}

/// Makes `dir` (a bundle's `Frameworks/`) hold the dynamic frameworks of
/// `frameworks` for `triple`, copied as they are. `record` (outside the
/// bundle), when given, remembers the copies, so a framework that has not
/// changed is not copied again: a development host's bundle stays from
/// one run to the next, a package's is made anew. Returns the frameworks
/// in `dir` and whether any changed, which the bundle is signed again for.
pub(crate) fn sync(
    frameworks: &[Framework],
    triple: &str,
    dir: &Path,
    record: Option<&Path>,
) -> anyhow::Result<(Vec<PathBuf>, bool)> {
    let bundles = dynamic(frameworks, triple)?;
    let recorded: serde_json::Map<String, serde_json::Value> = record
        .and_then(|record| fs::read_to_string(record).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let mut now = serde_json::Map::new();
    let mut embedded = Vec::new();
    let mut changed = false;
    for bundle in &bundles {
        let name = bundle.file_name().context("a framework has a name")?;
        let dest = dir.join(name);
        let source = format!(
            "{}\n{}",
            bundle.display(),
            stamp(&bundle.join(bundle.file_stem().context("a framework has a name")?))
        );
        let key = name.to_string_lossy().into_owned();
        if !dest.is_dir() || recorded.get(&key).and_then(|v| v.as_str()) != Some(&source) {
            if dest.exists() {
                fs::remove_dir_all(&dest)?;
            }
            fs::create_dir_all(dir)?;
            // `ditto` keeps a framework's symbolic links (Mac Catalyst's
            // are versioned) and permissions.
            let status = Command::new("ditto")
                .arg(bundle)
                .arg(&dest)
                .status()
                .context("run ditto")?;
            if !status.success() {
                bail!("copying {} failed", bundle.display());
            }
            changed = true;
        }
        now.insert(key, source.into());
        embedded.push(dest);
    }
    // A framework the app no longer lists leaves the bundle.
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let known = recorded.contains_key(&*entry.file_name().to_string_lossy());
            if known && !embedded.contains(&path) {
                fs::remove_dir_all(&path)?;
                changed = true;
            }
        }
    }
    if dir.is_dir() && fs::read_dir(dir)?.next().is_none() {
        fs::remove_dir(dir)?;
    }
    match record {
        None => {}
        Some(record) if now.is_empty() => {
            let _ = fs::remove_file(record);
        }
        Some(record) => {
            if let Some(parent) = record.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(record, serde_json::Value::Object(now).to_string())?;
        }
    }
    Ok((embedded, changed))
}

/// Where a static library's privacy manifest is: next to the library in
/// its XCFramework, or in its framework (at its root, or in a versioned
/// framework's `Resources/`).
fn privacy_manifest(library: &Library) -> Option<PathBuf> {
    let candidates = match library {
        Library::Archive(path) => vec![path.parent()?.join("PrivacyInfo.xcprivacy")],
        Library::Framework { bundle, .. } => vec![
            bundle.join("PrivacyInfo.xcprivacy"),
            bundle.join("Resources/PrivacyInfo.xcprivacy"),
        ],
    };
    candidates.into_iter().find(|path| path.is_file())
}

/// Adds what the privacy manifests of the static libraries of
/// `frameworks` [`link`] linked for `triple` declare to the app's (`manifest`, `nsh.json`
/// `ios.privacyManifest`): their code is part of the app's executable, so
/// App Store Connect counts the APIs it calls as the app's. Tracking is
/// the app's when any says so; domains, collected data and the reasons
/// for each API are joined. What a manifest has that is not in its form
/// is left out with a warning.
pub(crate) fn add_static_privacy(
    frameworks: &[Framework],
    triple: &str,
    manifest: &mut serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<()> {
    for framework in frameworks {
        let Some(library) = framework.for_target(triple)? else {
            continue;
        };
        if matches!(library, Library::Framework { dynamic: true, .. }) {
            continue;
        }
        let Some(path) = privacy_manifest(&library) else {
            continue;
        };
        let read: serde_json::Value = plist::from_file(&path)
            .with_context(|| format!("{}: cannot read {}", framework.listed, path.display()))?;
        let serde_json::Value::Object(mut declared) = read else {
            bail!(
                "{}: {} is not a dictionary",
                framework.listed,
                path.display()
            );
        };
        let mut ignored = Vec::new();
        crate::apple::keep_privacy_manifest(&mut declared, &mut ignored);
        for why in ignored {
            crate::ui::warn(format!("{}: {why}", path.display()));
        }
        merge_privacy(manifest, declared);
    }
    Ok(())
}

/// Adds the privacy manifest `from` to `into` ([`add_static_privacy`]).
fn merge_privacy(
    into: &mut serde_json::Map<String, serde_json::Value>,
    from: serde_json::Map<String, serde_json::Value>,
) {
    use serde_json::Value;
    /// Appends each of `more` that `list` does not have yet.
    fn join(list: &mut Value, more: &Value) {
        let (Value::Array(list), Some(more)) = (list, more.as_array()) else {
            return;
        };
        for value in more {
            if !list.contains(value) {
                list.push(value.clone());
            }
        }
    }
    /// Entries of one kind (`key`) are one: their lists joined and their
    /// flags true when either is.
    fn join_entries(list: &mut Value, more: &Value, key: &str) {
        let (Value::Array(list), Some(more)) = (list, more.as_array()) else {
            return;
        };
        for entry in more.iter().filter_map(Value::as_object) {
            let same = list
                .iter_mut()
                .filter_map(Value::as_object_mut)
                .find(|have| have.get(key) == entry.get(key));
            let Some(have) = same else {
                list.push(Value::Object(entry.clone()));
                continue;
            };
            for (name, value) in entry {
                match (have.get_mut(name), value) {
                    (None, _) => {
                        have.insert(name.clone(), value.clone());
                    }
                    (Some(have @ Value::Array(_)), more) => join(have, more),
                    (Some(Value::Bool(have)), Value::Bool(more)) => *have |= *more,
                    _ => {}
                }
            }
        }
    }
    for (key, value) in from {
        let Some(have) = into.get_mut(&key) else {
            into.insert(key, value);
            continue;
        };
        match key.as_str() {
            "NSPrivacyTracking" => {
                if value == Value::Bool(true) {
                    *have = Value::Bool(true);
                }
            }
            "NSPrivacyTrackingDomains" => join(have, &value),
            "NSPrivacyCollectedDataTypes" => {
                join_entries(have, &value, "NSPrivacyCollectedDataType");
            }
            "NSPrivacyAccessedAPITypes" => join_entries(have, &value, "NSPrivacyAccessedAPIType"),
            _ => {}
        }
    }
}

/// Signs the frameworks a bundle carries (`embedded`) with `identity` and
/// no entitlements, before the bundle itself, whose signature seals them:
/// `codesign` leaves the code inside a bundle with the signature it came
/// with (a vendor's, or none).
pub(crate) fn sign(embedded: &[PathBuf], identity: &str) -> anyhow::Result<()> {
    for framework in embedded {
        crate::package::codesign(framework, identity, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary directory of the test's own.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsh-ios-fw-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A thin 64-bit Mach-O header of `file_type`, little-endian.
    fn mach_o(file_type: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(0xfeed_facf_u32.to_le_bytes());
        bytes.extend(0x0100_000c_u32.to_le_bytes()); // CPU_TYPE_ARM64
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(file_type.to_le_bytes());
        bytes.extend([0u8; 16]);
        bytes
    }

    /// A universal file of one slice, `slice`, at offset 4096.
    fn universal(slice: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(0xcafe_babe_u32.to_be_bytes());
        bytes.extend(1u32.to_be_bytes());
        bytes.extend(0x0100_000c_u32.to_be_bytes());
        bytes.extend(0u32.to_be_bytes());
        bytes.extend(4096u32.to_be_bytes());
        bytes.extend((slice.len() as u32).to_be_bytes());
        bytes.extend(14u32.to_be_bytes());
        bytes.resize(4096, 0);
        bytes.extend(slice);
        bytes
    }

    fn archive() -> Vec<u8> {
        b"!<arch>\n__.SYMDEF SORTED/      0           0     0     644     8         `\n".to_vec()
    }

    /// `<dir>/<name>.framework` with the executable `binary`.
    fn framework(dir: &Path, name: &str, binary: &[u8]) -> PathBuf {
        let bundle = dir.join(format!("{name}.framework"));
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join(name), binary).unwrap();
        fs::write(bundle.join("PrivacyInfo.xcprivacy"), "<plist/>").unwrap();
        bundle
    }

    /// `<dir>/<name>.xcframework` with a library for each of `libraries`:
    /// (identifier, the library's file name, variant, `binary`).
    fn xcframework(
        dir: &Path,
        name: &str,
        libraries: &[(&str, &str, Option<&str>, Vec<u8>)],
    ) -> PathBuf {
        let root = dir.join(format!("{name}.xcframework"));
        let mut available = Vec::new();
        for (identifier, file, variant, binary) in libraries {
            let slice = root.join(identifier);
            fs::create_dir_all(&slice).unwrap();
            if let Some(stem) = file.strip_suffix(".framework") {
                framework(&slice, stem, binary);
            } else {
                fs::write(slice.join(file), binary).unwrap();
            }
            let mut library = plist::Dictionary::new();
            library.insert("LibraryIdentifier".into(), (*identifier).into());
            library.insert("LibraryPath".into(), (*file).into());
            library.insert(
                "SupportedArchitectures".into(),
                plist::Value::Array(if variant.is_some() {
                    vec!["arm64".into(), "x86_64".into()]
                } else {
                    vec!["arm64".into()]
                }),
            );
            library.insert("SupportedPlatform".into(), "ios".into());
            if let Some(variant) = variant {
                library.insert("SupportedPlatformVariant".into(), (*variant).into());
            }
            available.push(plist::Value::Dictionary(library));
        }
        let mut info = plist::Dictionary::new();
        info.insert("AvailableLibraries".into(), plist::Value::Array(available));
        info.insert("CFBundlePackageType".into(), "XFWK".into());
        info.insert("XCFrameworkFormatVersion".into(), "1.0".into());
        plist::Value::Dictionary(info)
            .to_file_xml(root.join("Info.plist"))
            .unwrap();
        root
    }

    fn listed(path: PathBuf) -> Framework {
        Framework {
            path,
            listed: "nsh.json `ios.frameworks[0]`".into(),
            ..Framework::default()
        }
    }

    /// Entries are paths or objects, relative to `nsh.json`; one of
    /// another form is left out with the others keeping their index.
    #[test]
    fn frameworks_are_paths_or_objects_relative_to_nsh_json() {
        let entries: Vec<Framework> = serde_json::from_value(serde_json::json!([
            "ios/Vendor/Sdk.xcframework",
            42,
            { "path": "ios/Vendor/Ads.xcframework", "forceLoad": true, "embed": true },
        ]))
        .unwrap();
        let mut ignored = Vec::new();
        let read = read(entries, Path::new("/app"), &mut ignored);
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].path, Path::new("/app/ios/Vendor/Sdk.xcframework"));
        assert!(!read[0].force_load);
        assert_eq!(read[0].listed, "nsh.json `ios.frameworks[0]`");
        assert_eq!(read[1].path, Path::new("/app/ios/Vendor/Ads.xcframework"));
        assert!(read[1].force_load);
        assert_eq!(read[1].listed, "nsh.json `ios.frameworks[2]`");
        assert_eq!(ignored.len(), 2, "{ignored:?}");
        assert!(ignored[0].starts_with("ios.frameworks[1]: a framework is a path"));
        assert_eq!(ignored[1], "unknown key `ios.frameworks[2].embed`");
    }

    /// `targets` names the targets as `nsh package` and `nsh run` do;
    /// one the reader does not know, or none at all, leaves the entry out
    /// of `nsh.json` and is an error in a crate.
    #[test]
    fn frameworks_name_their_targets_and_may_be_optional() {
        let entries: Vec<Framework> = serde_json::from_value(serde_json::json!([
            { "path": "Sdk.xcframework", "targets": ["ios", "ios-simulator"], "optional": true },
            { "path": "Mac.xcframework", "targets": ["catalyst"] },
            { "path": "Typo.xcframework", "targets": ["maccatalyst"] },
            { "path": "None.xcframework", "targets": [] },
        ]))
        .unwrap();
        let mut ignored = Vec::new();
        let read = read(entries, Path::new("/app"), &mut ignored);
        assert_eq!(read.len(), 2, "{ignored:?}");
        assert_eq!(read[0].targets, [Target::Ios, Target::IosSimulator]);
        assert!(read[0].optional);
        assert_eq!(read[1].targets, [Target::Catalyst]);
        assert!(!read[1].optional);
        assert_eq!(ignored.len(), 2, "{ignored:?}");
        assert!(ignored[0].starts_with("ios.frameworks[2]: "), "{ignored:?}");
        assert!(ignored[0].contains("maccatalyst"), "{ignored:?}");
        assert!(ignored[1].starts_with("ios.frameworks[3]: "), "{ignored:?}");
        assert!(ignored[1].contains("at least one target"), "{ignored:?}");

        let at = "sdk: [package.metadata.nsh] `ios.frameworks`";
        let crate_entries = serde_json::json!([{ "path": "Sdk.xcframework", "targets": ["tv"] }]);
        let error = of_crate(&crate_entries, Path::new("/sdk"), at, &mut Vec::new())
            .unwrap_err()
            .to_string();
        assert!(error.starts_with(&format!("{at}[0]: ")), "{error}");
    }

    /// An entry limited to some targets links nothing for the others,
    /// whether its XCFramework has a library for them or not; a missing
    /// one names `targets` as the way out.
    #[test]
    fn frameworks_link_only_for_their_targets() {
        let dir = temp("targets");
        // An SDK without a Mac Catalyst library, as many are.
        let sdk = xcframework(
            &dir,
            "Sdk",
            &[
                ("ios-arm64", "libSdk.a", None, archive()),
                (
                    "ios-arm64_x86_64-simulator",
                    "libSdk.a",
                    Some("simulator"),
                    archive(),
                ),
            ],
        );
        let error = link(&[listed(sdk.clone())], &[], "aarch64-apple-ios-macabi")
            .unwrap_err()
            .to_string();
        assert!(error.contains("`targets`"), "{error}");

        let mobile = Framework {
            targets: vec![Target::Ios, Target::IosSimulator],
            ..listed(sdk.clone())
        };
        let catalyst = link(
            std::slice::from_ref(&mobile),
            &["-ObjC".to_owned()],
            "aarch64-apple-ios-macabi",
        )
        .unwrap();
        assert_eq!(catalyst.args, ["-ObjC"]);
        assert!(catalyst.inputs.is_empty());
        assert!(catalyst.warnings.is_empty(), "{:?}", catalyst.warnings);
        for (triple, slice) in [
            ("aarch64-apple-ios", "ios-arm64"),
            ("aarch64-apple-ios-sim", "ios-arm64_x86_64-simulator"),
            ("x86_64-apple-ios", "ios-arm64_x86_64-simulator"),
        ] {
            let linked = link(std::slice::from_ref(&mobile), &[], triple).unwrap();
            assert_eq!(
                linked.inputs,
                [sdk.join(slice).join("libSdk.a")],
                "{triple}"
            );
        }

        // Devices only: the simulator links nothing of it either.
        let devices = Framework {
            targets: vec![Target::Ios],
            ..listed(sdk.clone())
        };
        assert!(
            link(std::slice::from_ref(&devices), &[], "aarch64-apple-ios-sim")
                .unwrap()
                .args
                .is_empty()
        );

        // One name per target: each target takes the entry that is its.
        let ui_mobile = xcframework(
            &dir.join("mobile"),
            "Ui",
            &[("ios-arm64", "Ui.framework", None, mach_o(MH_DYLIB))],
        );
        let ui_mac = framework(&dir.join("mac"), "Ui", &mach_o(MH_DYLIB));
        let both = [
            Framework {
                targets: vec![Target::Ios],
                ..listed(ui_mobile.clone())
            },
            Framework {
                targets: vec![Target::Catalyst],
                ..listed(ui_mac.clone())
            },
        ];
        let device = link(&both, &[], "aarch64-apple-ios").unwrap();
        assert_eq!(device.inputs, [ui_mobile.join("ios-arm64/Ui.framework/Ui")]);
        let mac = link(&both, &[], "aarch64-apple-ios-macabi").unwrap();
        assert_eq!(mac.inputs, [ui_mac.join("Ui")]);
        assert_eq!(
            dynamic(&both, "aarch64-apple-ios-macabi").unwrap(),
            [ui_mac]
        );
        assert!(dynamic(&both[1..], "aarch64-apple-ios").unwrap().is_empty());

        // Nor does a target it is not for take its privacy manifest.
        fs::write(
            sdk.join("ios-arm64/PrivacyInfo.xcprivacy"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>NSPrivacyTracking</key><true/>
<key>NSPrivacyTrackingDomains</key><array><string>sdk.example.com</string></array>
</dict></plist>"#,
        )
        .unwrap();
        let mut manifest = serde_json::Map::new();
        add_static_privacy(
            std::slice::from_ref(&devices),
            "aarch64-apple-ios-sim",
            &mut manifest,
        )
        .unwrap();
        assert!(manifest.is_empty());
        add_static_privacy(&[devices], "aarch64-apple-ios", &mut manifest).unwrap();
        assert_eq!(manifest["NSPrivacyTracking"], true);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// An optional entry whose path is not there (an SDK a script fetches
    /// for some builds) is left out of the link with a warning; once
    /// there, it links as any. Only a missing path is optional: one that
    /// is there and wrong is still an error.
    #[test]
    fn optional_frameworks_that_are_not_there_are_left_out() {
        let dir = temp("optional");
        let path = dir.join("Vendor/Push.xcframework");
        let push = Framework {
            optional: true,
            ..listed(path.clone())
        };
        let flags = ["-lsqlite3".to_owned()];
        let left_out = link(std::slice::from_ref(&push), &flags, "aarch64-apple-ios").unwrap();
        assert_eq!(left_out.args, ["-lsqlite3"]);
        assert!(left_out.inputs.is_empty());
        assert_eq!(left_out.warnings.len(), 1, "{:?}", left_out.warnings);
        assert!(
            left_out.warnings[0].starts_with("nsh.json `ios.frameworks[0]`: "),
            "{:?}",
            left_out.warnings
        );
        assert!(
            left_out.warnings[0].contains(&path.display().to_string()),
            "{:?}",
            left_out.warnings
        );
        assert!(
            dynamic(std::slice::from_ref(&push), "aarch64-apple-ios")
                .unwrap()
                .is_empty()
        );
        let mut manifest = serde_json::Map::new();
        add_static_privacy(
            std::slice::from_ref(&push),
            "aarch64-apple-ios",
            &mut manifest,
        )
        .unwrap();
        assert!(manifest.is_empty());

        // Nor does a target it is not for warn of it.
        let devices = Framework {
            targets: vec![Target::Ios],
            ..push.clone()
        };
        let other = link(&[devices], &[], "aarch64-apple-ios-macabi").unwrap();
        assert!(other.warnings.is_empty(), "{:?}", other.warnings);

        // Not optional, the same entry stops the build.
        assert!(link(&[listed(path.clone())], &[], "aarch64-apple-ios").is_err());

        // Fetched: it links, without a warning.
        xcframework(
            &dir.join("Vendor"),
            "Push",
            &[("ios-arm64", "libPush.a", None, archive())],
        );
        let linked = link(std::slice::from_ref(&push), &[], "aarch64-apple-ios").unwrap();
        assert_eq!(linked.inputs, [path.join("ios-arm64/libPush.a")]);
        assert!(linked.warnings.is_empty(), "{:?}", linked.warnings);
        // There, but without the target's library: an error still.
        assert!(link(std::slice::from_ref(&push), &[], "aarch64-apple-ios-sim").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A flag is one argument on one line.
    #[test]
    fn linker_flags_are_single_arguments() {
        let at = "nsh.json `ios.linkerFlags`";
        assert_eq!(
            linker_flags(&serde_json::json!(["-ObjC", "-lz"]), at).unwrap(),
            ["-ObjC", "-lz"]
        );
        for bad in [
            serde_json::json!("-ObjC"),
            serde_json::json!([1]),
            serde_json::json!([""]),
            serde_json::json!(["-ObjC\n-lz"]),
        ] {
            assert!(linker_flags(&bad, at).is_err(), "{bad}");
        }
    }

    /// Mach-O dynamic libraries and archives are told apart, thin or
    /// universal; anything else is neither.
    #[test]
    fn framework_executables_are_static_or_dynamic() {
        let dir = temp("kinds");
        let check = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            fs::write(&path, bytes).unwrap();
            is_dynamic(&path)
        };
        assert!(check("dylib", &mach_o(MH_DYLIB)).unwrap());
        assert!(!check("object", &mach_o(1)).unwrap());
        assert!(!check("archive", &archive()).unwrap());
        assert!(check("fat-dylib", &universal(&mach_o(MH_DYLIB))).unwrap());
        assert!(!check("fat-archive", &universal(&archive())).unwrap());
        assert!(check("text", b"not code").is_err());
        assert!(is_dynamic(&dir.join("missing")).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Each target takes its XCFramework library; a target the
    /// XCFramework has no library for is an error naming what it has.
    #[test]
    fn xcframeworks_give_each_target_its_library() {
        let dir = temp("slices");
        let sdk = xcframework(
            &dir,
            "Sdk",
            &[
                ("ios-arm64", "libSdk.a", None, archive()),
                (
                    "ios-arm64_x86_64-simulator",
                    "libSdk.a",
                    Some("simulator"),
                    archive(),
                ),
            ],
        );
        let sdk = listed(sdk);
        assert_eq!(
            sdk.library("aarch64-apple-ios").unwrap(),
            Library::Archive(sdk.path.join("ios-arm64/libSdk.a"))
        );
        for simulator in ["aarch64-apple-ios-sim", "x86_64-apple-ios"] {
            assert_eq!(
                sdk.library(simulator).unwrap(),
                Library::Archive(sdk.path.join("ios-arm64_x86_64-simulator/libSdk.a"))
            );
        }
        let error = sdk
            .library("aarch64-apple-ios-macabi")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("no library for Mac Catalyst (arm64)"),
            "{error}"
        );
        assert!(
            error.contains("ios-arm64, ios-arm64_x86_64-simulator"),
            "{error}"
        );

        let ui = xcframework(
            &dir,
            "Ui",
            &[
                ("ios-arm64", "Ui.framework", None, mach_o(MH_DYLIB)),
                (
                    "ios-arm64_x86_64-maccatalyst",
                    "Ui.framework",
                    Some("maccatalyst"),
                    universal(&mach_o(MH_DYLIB)),
                ),
            ],
        );
        let ui = listed(ui);
        let bundle = ui.path.join("ios-arm64_x86_64-maccatalyst/Ui.framework");
        assert_eq!(
            ui.library("aarch64-apple-ios-macabi").unwrap(),
            Library::Framework {
                binary: bundle.join("Ui"),
                bundle,
                dynamic: true,
            }
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// What is not an XCFramework or a framework is refused, naming who
    /// lists it.
    #[test]
    fn other_paths_are_refused() {
        let dir = temp("refused");
        fs::create_dir_all(dir.join("Sdk.bundle")).unwrap();
        fs::write(dir.join("libSdk.a"), archive()).unwrap();
        for path in [
            dir.join("Sdk.bundle"),
            dir.join("libSdk.a"),
            dir.join("Missing.xcframework"),
        ] {
            let error = listed(path).library("aarch64-apple-ios").unwrap_err();
            assert!(
                error
                    .to_string()
                    .starts_with("nsh.json `ios.frameworks[0]`: "),
                "{error}"
            );
        }
        let dylib = xcframework(
            &dir,
            "Dylib",
            &[("ios-arm64", "libDylib.dylib", None, mach_o(MH_DYLIB))],
        );
        let error = listed(dylib)
            .library("aarch64-apple-ios")
            .unwrap_err()
            .to_string();
        assert!(error.contains("as frameworks"), "{error}");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Static libraries are linked in, force-loaded when asked; dynamic
    /// frameworks are linked against, found in the bundle's
    /// `Frameworks/`; the flags come last, as written.
    #[test]
    fn the_link_names_the_libraries_then_the_flags() {
        let dir = temp("link");
        let ads = xcframework(&dir, "Ads", &[("ios-arm64", "libAds.a", None, archive())]);
        let analytics = framework(&dir.join("static"), "Analytics", &archive());
        let ui = framework(&dir.join("dynamic"), "Ui", &mach_o(MH_DYLIB));
        let frameworks = [
            Framework {
                force_load: true,
                ..listed(ads.clone())
            },
            listed(analytics.clone()),
            listed(ui.clone()),
        ];
        let flags = ["-ObjC".to_owned(), "-lsqlite3".to_owned()];
        let link = link(&frameworks, &flags, "aarch64-apple-ios").unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        assert_eq!(
            link.args,
            [
                "-force_load".to_owned(),
                s(&ads.join("ios-arm64").join("libAds.a")),
                format!("-F{}", s(&dir.join("static"))),
                "-framework".to_owned(),
                "Analytics".to_owned(),
                format!("-F{}", s(&dir.join("dynamic"))),
                "-framework".to_owned(),
                "Ui".to_owned(),
                "-Wl,-rpath,@executable_path/Frameworks".to_owned(),
                "-ObjC".to_owned(),
                "-lsqlite3".to_owned(),
            ]
        );
        assert_eq!(
            link.inputs,
            [
                ads.join("ios-arm64/libAds.a"),
                analytics.join("Analytics"),
                ui.join("Ui"),
            ]
        );
        let env = link_env(&link);
        assert_eq!(env[0].0, LINK_ARGS_VAR);
        assert_eq!(env[0].1.split('\u{1f}').count(), link.args.len());
        assert_eq!(env[1].0, LINK_INPUTS_VAR);
        assert!(link_env(&Link::default()).is_empty());

        // Mac Catalyst finds them in `Contents/Frameworks`.
        let catalyst = super::link(&[listed(ui.clone())], &[], "aarch64-apple-ios-macabi").unwrap();
        assert_eq!(
            catalyst.args.last().unwrap(),
            "-Wl,-rpath,@executable_path/../Frameworks"
        );

        // A dynamic framework has nothing to force-load; two libraries of
        // one name would be one in `Frameworks/`.
        let forced = Framework {
            force_load: true,
            ..listed(ui.clone())
        };
        assert!(super::link(&[forced], &[], "aarch64-apple-ios").is_err());
        let twice = super::link(&[listed(ui.clone()), listed(ui)], &[], "aarch64-apple-ios")
            .unwrap_err()
            .to_string();
        assert!(twice.contains("are both Ui.framework"), "{twice}");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A static library's privacy manifest joins the app's; a dynamic
    /// framework keeps its own.
    #[test]
    fn static_libraries_declare_their_privacy_in_the_apps_manifest() {
        let dir = temp("privacy");
        let manifest = |tracking: bool, domain: &str, reason: &str| {
            let text = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>NSPrivacyTracking</key><{tracking}/>
<key>NSPrivacyTrackingDomains</key><array><string>{domain}</string></array>
<key>NSPrivacyAccessedAPITypes</key><array><dict>
<key>NSPrivacyAccessedAPIType</key><string>NSPrivacyAccessedAPICategoryUserDefaults</string>
<key>NSPrivacyAccessedAPITypeReasons</key><array><string>{reason}</string></array>
</dict></array>
</dict></plist>"#
            );
            text
        };
        let ads = xcframework(&dir, "Ads", &[("ios-arm64", "libAds.a", None, archive())]);
        fs::write(
            ads.join("ios-arm64/PrivacyInfo.xcprivacy"),
            manifest(true, "ads.example.com", "CA92.1"),
        )
        .unwrap();
        let ui = framework(&dir.join("dynamic"), "Ui", &mach_o(MH_DYLIB));
        fs::write(
            ui.join("PrivacyInfo.xcprivacy"),
            manifest(true, "ui.example.com", "1C8F.1"),
        )
        .unwrap();
        let mut app: serde_json::Map<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "NSPrivacyTracking": false,
                "NSPrivacyAccessedAPITypes": [{
                    "NSPrivacyAccessedAPIType": "NSPrivacyAccessedAPICategoryUserDefaults",
                    "NSPrivacyAccessedAPITypeReasons": ["1C8F.1"],
                }],
            }))
            .unwrap();
        add_static_privacy(&[listed(ads), listed(ui)], "aarch64-apple-ios", &mut app).unwrap();
        assert_eq!(
            serde_json::Value::Object(app),
            serde_json::json!({
                "NSPrivacyTracking": true,
                "NSPrivacyTrackingDomains": ["ads.example.com"],
                "NSPrivacyAccessedAPITypes": [{
                    "NSPrivacyAccessedAPIType": "NSPrivacyAccessedAPICategoryUserDefaults",
                    "NSPrivacyAccessedAPITypeReasons": ["1C8F.1", "CA92.1"],
                }],
            })
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Only dynamic frameworks go into the bundle, with their own files
    /// (their privacy manifests); one that has not changed is not copied
    /// again, one the app no longer lists leaves.
    #[cfg(target_os = "macos")]
    #[test]
    fn dynamic_frameworks_go_into_the_bundle() {
        let dir = temp("sync");
        let ui = framework(&dir.join("vendor"), "Ui", &mach_o(MH_DYLIB));
        let analytics = framework(&dir.join("vendor"), "Analytics", &archive());
        let frameworks = [listed(ui.clone()), listed(analytics)];
        let app = dir.join("App.app");
        let into = app.join("Frameworks");
        let record = dir.join("frameworks.json");
        let triple = "aarch64-apple-ios-sim";

        let (embedded, changed) = sync(&frameworks, triple, &into, Some(&record)).unwrap();
        assert_eq!(embedded, [into.join("Ui.framework")]);
        assert!(changed);
        assert!(into.join("Ui.framework/Ui").is_file());
        assert!(into.join("Ui.framework/PrivacyInfo.xcprivacy").is_file());
        assert!(!into.join("Analytics.framework").exists());

        let (_, changed) = sync(&frameworks, triple, &into, Some(&record)).unwrap();
        assert!(!changed, "an unchanged framework is not copied again");

        // A new build of the framework is copied again.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut binary = mach_o(MH_DYLIB);
        binary.extend([1u8; 8]);
        fs::write(ui.join("Ui"), &binary).unwrap();
        let (_, changed) = sync(&frameworks, triple, &into, Some(&record)).unwrap();
        assert!(changed);
        assert_eq!(fs::read(into.join("Ui.framework/Ui")).unwrap(), binary);

        let (embedded, changed) = sync(&frameworks[1..], triple, &into, Some(&record)).unwrap();
        assert!(embedded.is_empty());
        assert!(changed);
        assert!(!into.exists(), "an empty Frameworks/ leaves the bundle");
        assert!(!record.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
