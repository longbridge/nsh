//! `nsh.json` `ios.extensions`: app extensions (`.appex`: widgets and Live
//! Activities, notification service and content extensions, share
//! extensions) that iOS packages carry in `PlugIns/`.
//!
//! The app builds them itself, with whatever toolchain they need, as it
//! builds `android.dex`; NSH only packages them. Only a package can carry an
//! extension, and only the packaging can sign it, so an nshm cannot add one
//! on its own. Each extension is copied into the app, given the app's
//! version (an extension's version should match its app's: App Store
//! Connect flags a mismatch), and signed before the app, whose signature
//! seals it: on devices with its own provisioning profile and entitlements
//! ([`crate::provisioning`]), in the simulator ad hoc.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::{provisioning::Profile, ui};

/// One of `nsh.json` `ios.extensions`. It is read from any value, so an
/// entry the reader cannot take is left out by [`read`] where it stands:
/// the others keep the index the app wrote, which errors name.
#[derive(Debug, Default, Deserialize)]
#[serde(from = "serde_json::Value")]
pub(crate) struct AppExtension {
    /// Its index in `ios.extensions` as the app wrote it ([`read`]).
    pub(crate) index: usize,
    /// The extension built for devices (`<Name>.appex`), relative to
    /// `nsh.json`.
    pub(crate) path: PathBuf,
    /// The extension built for the simulator, relative to `nsh.json`.
    /// Without it simulator packages carry `path` when it is built for the
    /// simulator, and leave the extension out otherwise.
    pub(crate) simulator_path: Option<PathBuf>,
    /// Entitlements the extension is signed with, as `ios.entitlements` are
    /// the app's: on devices over those its provisioning profile grants.
    pub(crate) entitlements: serde_json::Map<String, serde_json::Value>,
    /// Why the entry cannot be read, when it cannot: [`read`] leaves it out.
    invalid: Option<String>,
    /// The entry's keys the reader does not know, which [`read`] warns of.
    unknown: Vec<String>,
}

impl From<serde_json::Value> for AppExtension {
    fn from(value: serde_json::Value) -> Self {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Entry {
            path: PathBuf,
            simulator_path: Option<PathBuf>,
            #[serde(default)]
            entitlements: serde_json::Map<String, serde_json::Value>,
        }
        let mut unknown = Vec::new();
        match serde_ignored::deserialize::<_, _, Entry>(&value, |key| unknown.push(key.to_string()))
        {
            Ok(entry) => AppExtension {
                path: entry.path,
                simulator_path: entry.simulator_path,
                entitlements: entry.entitlements,
                unknown,
                ..AppExtension::default()
            },
            Err(e) => AppExtension {
                invalid: Some(e.to_string()),
                ..AppExtension::default()
            },
        }
    }
}

/// `ios.extensions` as read (`entries`), each with its index and its paths
/// relative to `dir`, `nsh.json`'s directory; an entry that cannot be read
/// is left out, with its unknown keys added to `ignored`, which
/// `read_config` warns of.
pub(crate) fn read(
    entries: Vec<AppExtension>,
    dir: &Path,
    ignored: &mut Vec<String>,
) -> Vec<AppExtension> {
    entries
        .into_iter()
        .enumerate()
        .filter_map(|(index, extension)| {
            let at = format!("ios.extensions[{index}]");
            ignored.extend(
                extension
                    .unknown
                    .iter()
                    .map(|key| format!("unknown key `{at}.{key}`")),
            );
            if let Some(why) = &extension.invalid {
                ignored.push(format!("{at}: {why}"));
                return None;
            }
            Some(AppExtension {
                index,
                path: dir.join(&extension.path),
                simulator_path: extension.simulator_path.as_ref().map(|p| dir.join(p)),
                ..extension
            })
        })
        .collect()
}

/// An extension copied into a package, to sign.
#[derive(Debug)]
pub(crate) struct Embedded {
    /// Its index in `ios.extensions`.
    index: usize,
    /// The bundle in the package (`<App>.app/PlugIns/<Name>.appex`).
    bundle: PathBuf,
    /// Its bundle identifier.
    id: String,
    /// Its own entitlements, and the `nsh.json` key they come from.
    entitlements: (plist::Dictionary, String),
}

/// Copies the app's extensions (`extensions`) into `app`'s `PlugIns/`, the
/// builds for the simulator when `simulator`, each given the version and
/// build of the app's `Info.plist` (`info`).
pub(crate) fn embed(
    extensions: &[AppExtension],
    app: &Path,
    info: &plist::Dictionary,
    simulator: bool,
) -> anyhow::Result<Vec<Embedded>> {
    let text = |key: &str| info.get(key).and_then(plist::Value::as_string);
    let app_id = text("CFBundleIdentifier").context("the app's Info.plist has no identifier")?;
    let (platform, other) = if simulator {
        ("iPhoneSimulator", "iPhoneOS")
    } else {
        ("iPhoneOS", "iPhoneSimulator")
    };
    let mut embedded: Vec<Embedded> = Vec::new();
    for extension in extensions {
        let index = extension.index;
        let (source, key) = match &extension.simulator_path {
            Some(path) if simulator => (path, "simulatorPath"),
            _ => (&extension.path, "path"),
        };
        let at = format!("nsh.json ios.extensions[{index}].{key}");
        if source.extension().is_none_or(|e| e != "appex") || !source.is_dir() {
            bail!(
                "{at}: {} is not an app extension (a `.appex` directory)",
                source.display()
            );
        }
        let mut extension_info = read_info(source).with_context(|| at.clone())?;
        let built_for = extension_info
            .get("CFBundleSupportedPlatforms")
            .and_then(plist::Value::as_array)
            .map(|platforms| {
                platforms
                    .iter()
                    .filter_map(plist::Value::as_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !built_for.is_empty() && !built_for.contains(&platform) {
            if simulator && extension.simulator_path.is_none() {
                ui::warn(format!(
                    "{} is built for {other}, so the simulator package leaves it out: \
                     set nsh.json ios.extensions[{index}].simulatorPath to its build for the simulator",
                    source.display()
                ));
                continue;
            }
            bail!(
                "{at}: {} is built for {}, not {platform}",
                source.display(),
                built_for.join(", ")
            );
        }
        let id = extension_info
            .get("CFBundleIdentifier")
            .and_then(plist::Value::as_string)
            .with_context(|| format!("{at}: {} has no CFBundleIdentifier", source.display()))?
            .to_owned();
        // iOS installs an extension only under its app's identifier.
        if id
            .strip_prefix(app_id)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_none_or(str::is_empty)
        {
            bail!("{at}: its identifier {id} does not start with the app's, `{app_id}.`");
        }
        let executable = extension_info
            .get("CFBundleExecutable")
            .and_then(plist::Value::as_string)
            .with_context(|| format!("{at}: {} has no CFBundleExecutable", source.display()))?;
        if !source.join(executable).is_file() {
            bail!(
                "{at}: {} has no executable `{executable}`",
                source.display()
            );
        }
        if let Some(earlier) = embedded.iter().find(|e| e.id == id) {
            bail!(
                "{at}: ios.extensions[{}] has the identifier {id} too",
                earlier.index
            );
        }
        let name = source.file_name().context("extension name")?;
        let bundle = app.join("PlugIns").join(name);
        if bundle.exists() {
            bail!(
                "{at}: another extension is named {} too",
                name.to_string_lossy()
            );
        }
        let entitlements = crate::apple::extension_entitlements(extension)?;
        fs::create_dir_all(app.join("PlugIns"))?;
        // `ditto` keeps the bundle's symbolic links and permissions.
        let status = Command::new("ditto")
            .arg(source)
            .arg(&bundle)
            .status()
            .context("run ditto")?;
        if !status.success() {
            bail!("copying {} failed", source.display());
        }
        for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
            if let Some(value) = info.get(key) {
                extension_info.insert(key.to_owned(), value.clone());
            }
        }
        plist::Value::Dictionary(extension_info).to_file_xml(bundle.join("Info.plist"))?;
        embedded.push(Embedded {
            index,
            bundle,
            id,
            entitlements,
        });
    }
    Ok(embedded)
}

/// The `Info.plist` of the bundle at `bundle`, XML or binary.
fn read_info(bundle: &Path) -> anyhow::Result<plist::Dictionary> {
    let path = bundle.join("Info.plist");
    plist::Value::from_file(&path)
        .with_context(|| format!("cannot read {}", path.display()))?
        .into_dictionary()
        .with_context(|| format!("{} is not a dictionary", path.display()))
}

/// The code a bundle carries besides its executable: the frameworks and
/// dynamic libraries in its `Frameworks/`, in name order.
fn nested_code(bundle: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let dir = bundle.join("Frameworks");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut code = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|e| e == "framework" || e == "dylib")
        {
            code.push(path);
        }
    }
    code.sort();
    Ok(code)
}

/// Signs `extension` with `identity` and the entitlements file at
/// `entitlements`. `codesign` signs a bundle's executable and seals the rest,
/// leaving the code inside it with the signature it came with (none, or a
/// development one), so that code is signed first, with the same identity
/// and no entitlements, as Xcode does.
fn sign(extension: &Embedded, identity: &str, entitlements: Option<&Path>) -> anyhow::Result<()> {
    for code in nested_code(&extension.bundle)? {
        crate::package::codesign(&code, identity, None)?;
    }
    crate::package::codesign(&extension.bundle, identity, entitlements)
}

/// Signs the extensions of a simulator package ad hoc, with their own
/// entitlements as they are; `out` holds the entitlements files.
pub(crate) fn sign_ad_hoc(embedded: &[Embedded], out: &Path) -> anyhow::Result<()> {
    for extension in embedded {
        let (entitlements, _) = &extension.entitlements;
        let mut path = None;
        if !entitlements.is_empty() {
            let file = out.join(format!("{}.entitlements", extension.id));
            plist::Value::Dictionary(entitlements.clone()).to_file_xml(&file)?;
            path = Some(file);
        }
        sign(extension, "-", path.as_deref())?;
    }
    Ok(())
}

/// Signs the extensions of a device package with `identity`, each with the
/// profile of `profiles` for its identifier, which it carries as
/// `embedded.mobileprovision`, and the entitlements it grants with the
/// extension's own; `out` holds the entitlements files.
pub(crate) fn sign_device(
    embedded: &[Embedded],
    identity: &str,
    profiles: &[Profile],
    out: &Path,
) -> anyhow::Result<()> {
    for extension in embedded {
        let profile = Profile::choose(profiles, &extension.id, true)?;
        fs::copy(
            &profile.path,
            extension.bundle.join("embedded.mobileprovision"),
        )
        .with_context(|| format!("copy {}", profile.path.display()))?;
        let (requested, section) = &extension.entitlements;
        let entitlements = profile.entitlements(&extension.id, requested.clone(), section)?;
        let path = out.join(format!("{}.entitlements", extension.id));
        entitlements.to_file_xml(&path)?;
        sign(extension, identity, Some(&path))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary directory of the test's own.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsh-appex-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A built extension `<name>.appex` in `dir` with identifier `id`, built
    /// for `platform`.
    fn appex(dir: &Path, name: &str, id: &str, platform: &str) -> PathBuf {
        let bundle = dir.join(format!("{name}.appex"));
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join(name), b"binary").unwrap();
        let mut info = plist::Dictionary::new();
        info.insert("CFBundleIdentifier".into(), id.into());
        info.insert("CFBundleExecutable".into(), name.into());
        info.insert("CFBundlePackageType".into(), "XPC!".into());
        info.insert("CFBundleShortVersionString".into(), "0.1".into());
        info.insert("CFBundleVersion".into(), "7".into());
        info.insert(
            "CFBundleSupportedPlatforms".into(),
            plist::Value::Array(vec![platform.into()]),
        );
        plist::Value::Dictionary(info)
            .to_file_binary(bundle.join("Info.plist"))
            .unwrap();
        bundle
    }

    // `ditto` is macOS's.
    #[cfg(target_os = "macos")]
    fn app_info() -> plist::Dictionary {
        let mut info = plist::Dictionary::new();
        info.insert("CFBundleIdentifier".into(), "com.example.app".into());
        info.insert("CFBundleShortVersionString".into(), "2.3.0".into());
        info.insert("CFBundleVersion".into(), "41".into());
        info
    }

    /// The frameworks and dynamic libraries in an extension's
    /// `Frameworks/` are signed before it; resources and other files are
    /// not code.
    #[test]
    fn an_extensions_frameworks_are_its_nested_code() {
        let dir = temp("nested");
        let bundle = appex(&dir, "Widget", "com.example.app.Widget", "iPhoneOS");
        assert!(nested_code(&bundle).unwrap().is_empty());
        let frameworks = bundle.join("Frameworks");
        fs::create_dir_all(frameworks.join("Shared.framework")).unwrap();
        fs::create_dir_all(frameworks.join("Charts.framework")).unwrap();
        fs::write(frameworks.join("libswift_Concurrency.dylib"), b"code").unwrap();
        fs::write(frameworks.join("notes.txt"), b"text").unwrap();
        assert_eq!(
            nested_code(&bundle).unwrap(),
            [
                frameworks.join("Charts.framework"),
                frameworks.join("Shared.framework"),
                frameworks.join("libswift_Concurrency.dylib"),
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// An entry the reader cannot take is left out where it stands: the
    /// entries after it keep the index the app wrote.
    #[test]
    fn entries_keep_the_index_the_app_wrote() {
        let entries: Vec<AppExtension> = serde_json::from_value(serde_json::json!([
            { "simulatorPath": "sim/A.appex" },
            "B.appex",
            { "path": "C.appex", "entitlments": {} }
        ]))
        .unwrap();
        let mut ignored = Vec::new();
        let read = read(entries, Path::new("/app"), &mut ignored);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].index, 2);
        assert_eq!(read[0].path, Path::new("/app/C.appex"));
        assert_eq!(ignored.len(), 3, "{ignored:?}");
        assert!(
            ignored[0].starts_with("ios.extensions[0]: missing field `path`"),
            "{ignored:?}"
        );
        assert!(ignored[1].starts_with("ios.extensions[1]: "), "{ignored:?}");
        assert_eq!(ignored[2], "unknown key `ios.extensions[2].entitlments`");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn extensions_go_into_plugins_with_the_apps_version() {
        let dir = temp("embed");
        let device = appex(&dir, "Widget", "com.example.app.Widget", "iPhoneOS");
        let simulator = dir.join("sim");
        fs::create_dir_all(&simulator).unwrap();
        let simulator = appex(
            &simulator,
            "Widget",
            "com.example.app.Widget",
            "iPhoneSimulator",
        );
        let extensions = [AppExtension {
            path: device,
            simulator_path: Some(simulator),
            ..AppExtension::default()
        }];
        for (simulator, platform) in [(false, "iPhoneOS"), (true, "iPhoneSimulator")] {
            let app = dir.join(format!("{platform}/Demo.app"));
            fs::create_dir_all(&app).unwrap();
            let embedded = embed(&extensions, &app, &app_info(), simulator).unwrap();
            assert_eq!(embedded.len(), 1);
            assert_eq!(embedded[0].id, "com.example.app.Widget");
            assert_eq!(embedded[0].bundle, app.join("PlugIns/Widget.appex"));
            assert!(app.join("PlugIns/Widget.appex/Widget").is_file());
            let info = read_info(&embedded[0].bundle).unwrap();
            assert_eq!(info["CFBundleShortVersionString"], "2.3.0".into());
            assert_eq!(info["CFBundleVersion"], "41".into());
            assert_eq!(
                info["CFBundleSupportedPlatforms"],
                plist::Value::Array(vec![platform.into()])
            );
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A simulator package without the extension's simulator build leaves
    /// it out; a device package refuses a simulator build.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_build_for_another_platform_is_left_out_or_refused() {
        let dir = temp("platform");
        let device = appex(&dir, "Widget", "com.example.app.Widget", "iPhoneOS");
        let app = dir.join("Demo.app");
        fs::create_dir_all(&app).unwrap();
        let only_device = [AppExtension {
            path: device.clone(),
            ..AppExtension::default()
        }];
        assert!(
            embed(&only_device, &app, &app_info(), true)
                .unwrap()
                .is_empty()
        );
        assert!(!app.join("PlugIns").exists());
        let swapped = [AppExtension {
            index: 3,
            path: device.clone(),
            simulator_path: Some(device),
            ..AppExtension::default()
        }];
        let error = embed(&swapped, &app, &app_info(), true)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("ios.extensions[3].simulatorPath")
                && error.contains("is built for iPhoneOS, not iPhoneSimulator"),
            "{error}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_extension_takes_an_identifier_under_the_apps() {
        let dir = temp("identifier");
        let app = dir.join("Demo.app");
        fs::create_dir_all(&app).unwrap();
        for id in [
            "com.example.other.Widget",
            "com.example.app",
            "com.example.appWidget",
        ] {
            let bundle = appex(&dir, "Widget", id, "iPhoneOS");
            let extensions = [AppExtension {
                path: bundle.clone(),
                ..AppExtension::default()
            }];
            let error = embed(&extensions, &app, &app_info(), false)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("does not start with the app's, `com.example.app.`"),
                "{id}: {error}"
            );
            fs::remove_dir_all(&bundle).unwrap();
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_path_that_is_not_an_extension_is_refused() {
        let dir = temp("not-appex");
        let app = dir.join("Demo.app");
        fs::create_dir_all(&app).unwrap();
        let extensions = [AppExtension {
            path: dir.join("Missing.appex"),
            ..AppExtension::default()
        }];
        let error = embed(&extensions, &app, &app_info(), false)
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("nsh.json ios.extensions[0].path: ")
                && error.contains("is not an app extension"),
            "{error}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn two_extensions_with_one_identifier_are_refused() {
        let dir = temp("duplicate");
        let first = dir.join("a");
        let second = dir.join("b");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let extensions = [
            AppExtension {
                path: appex(&first, "Widget", "com.example.app.Widget", "iPhoneOS"),
                ..AppExtension::default()
            },
            AppExtension {
                index: 1,
                path: appex(&second, "Other", "com.example.app.Widget", "iPhoneOS"),
                ..AppExtension::default()
            },
        ];
        let app = dir.join("Demo.app");
        fs::create_dir_all(&app).unwrap();
        let error = embed(&extensions, &app, &app_info(), false)
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("nsh.json ios.extensions[1].path: ")
                && error
                    .contains("ios.extensions[0] has the identifier com.example.app.Widget too"),
            "{error}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
