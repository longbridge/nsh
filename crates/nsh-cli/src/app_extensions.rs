//! `nsh.json` `ios.extensions`: app extensions (`.appex`: widgets and Live
//! Activities, notification service and content extensions, share
//! extensions) that iOS packages carry in `PlugIns/`.
//!
//! The app builds them itself, with whatever toolchain they need, as it
//! builds `android.dex`; NSH only packages them. Only a package can carry an
//! extension, and only the packaging can sign it, so an nshm cannot add one
//! on its own. Each extension is copied into the app, given the app's
//! version (the App Store refuses an extension whose version differs from
//! its app's), and signed before the app, whose signature seals it: on
//! devices with its own provisioning profile and entitlements
//! ([`crate::provisioning`]), in the simulator ad hoc.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::{apple::AppleConfig, provisioning::Profile, ui};

/// One of `nsh.json` `ios.extensions`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppExtension {
    /// The extension built for devices (`<Name>.appex`), relative to
    /// `nsh.json`.
    pub(crate) path: PathBuf,
    /// The extension built for the simulator, relative to `nsh.json`.
    /// Without it simulator packages carry `path` when it is built for the
    /// simulator, and leave the extension out otherwise.
    pub(crate) simulator_path: Option<PathBuf>,
    /// Entitlements the extension is signed with, as `ios.entitlements` are
    /// the app's: on devices over those its provisioning profile grants.
    #[serde(default)]
    pub(crate) entitlements: serde_json::Map<String, serde_json::Value>,
}

impl AppExtension {
    /// Its paths made relative to `dir`, `nsh.json`'s directory.
    pub(crate) fn relative_to(self, dir: &Path) -> Self {
        AppExtension {
            path: dir.join(self.path),
            simulator_path: self.simulator_path.map(|path| dir.join(path)),
            ..self
        }
    }
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
}

/// Copies the app's extensions (`config.extensions`) into `app`'s
/// `PlugIns/`, the builds for the simulator when `simulator`, each given
/// the version and build of the app's `Info.plist` (`info`).
pub(crate) fn embed(
    config: &AppleConfig,
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
    for (index, extension) in config.extensions.iter().enumerate() {
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
        embedded.push(Embedded { index, bundle, id });
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

/// Signs the extensions of a simulator package ad hoc, with their own
/// entitlements as they are; `out` holds the entitlements files.
pub(crate) fn sign_ad_hoc(
    config: &AppleConfig,
    embedded: &[Embedded],
    out: &Path,
) -> anyhow::Result<()> {
    for extension in embedded {
        let (entitlements, _) = config.extension_entitlements(extension.index)?;
        let mut path = None;
        if !entitlements.is_empty() {
            let file = out.join(format!("{}.entitlements", extension.id));
            plist::Value::Dictionary(entitlements).to_file_xml(&file)?;
            path = Some(file);
        }
        crate::package::codesign(&extension.bundle, "-", path.as_deref())?;
    }
    Ok(())
}

/// Signs the extensions of a device package with `identity`, each with the
/// profile of `profiles` for its identifier, which it carries as
/// `embedded.mobileprovision`, and the entitlements it grants with the
/// extension's own; `out` holds the entitlements files.
pub(crate) fn sign_device(
    config: &AppleConfig,
    embedded: &[Embedded],
    identity: &str,
    profiles: &[Profile],
    out: &Path,
) -> anyhow::Result<()> {
    for extension in embedded {
        let profile = Profile::choose(profiles, &extension.id)?;
        fs::copy(
            &profile.path,
            extension.bundle.join("embedded.mobileprovision"),
        )
        .with_context(|| format!("copy {}", profile.path.display()))?;
        let (requested, section) = config.extension_entitlements(extension.index)?;
        let entitlements = profile.entitlements(&extension.id, requested, &section)?;
        let path = out.join(format!("{}.entitlements", extension.id));
        entitlements.to_file_xml(&path)?;
        crate::package::codesign(&extension.bundle, identity, Some(&path))?;
    }
    Ok(())
}

// `ditto` is macOS's.
#[cfg(all(test, target_os = "macos"))]
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

    fn app_info() -> plist::Dictionary {
        let mut info = plist::Dictionary::new();
        info.insert("CFBundleIdentifier".into(), "com.example.app".into());
        info.insert("CFBundleShortVersionString".into(), "2.3.0".into());
        info.insert("CFBundleVersion".into(), "41".into());
        info
    }

    fn config(extensions: Vec<AppExtension>) -> AppleConfig {
        AppleConfig {
            extensions,
            ..AppleConfig::default()
        }
    }

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
        let config = config(vec![AppExtension {
            path: device,
            simulator_path: Some(simulator),
            ..AppExtension::default()
        }]);
        for (simulator, platform) in [(false, "iPhoneOS"), (true, "iPhoneSimulator")] {
            let app = dir.join(format!("{platform}/Demo.app"));
            fs::create_dir_all(&app).unwrap();
            let embedded = embed(&config, &app, &app_info(), simulator).unwrap();
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
    #[test]
    fn a_build_for_another_platform_is_left_out_or_refused() {
        let dir = temp("platform");
        let device = appex(&dir, "Widget", "com.example.app.Widget", "iPhoneOS");
        let app = dir.join("Demo.app");
        fs::create_dir_all(&app).unwrap();
        let only_device = config(vec![AppExtension {
            path: device.clone(),
            ..AppExtension::default()
        }]);
        assert!(
            embed(&only_device, &app, &app_info(), true)
                .unwrap()
                .is_empty()
        );
        assert!(!app.join("PlugIns").exists());
        let swapped = config(vec![AppExtension {
            path: device.clone(),
            simulator_path: Some(device),
            ..AppExtension::default()
        }]);
        let error = embed(&swapped, &app, &app_info(), true)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("ios.extensions[0].simulatorPath")
                && error.contains("is built for iPhoneOS, not iPhoneSimulator"),
            "{error}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

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
            let config = config(vec![AppExtension {
                path: bundle.clone(),
                ..AppExtension::default()
            }]);
            let error = embed(&config, &app, &app_info(), false)
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

    #[test]
    fn a_path_that_is_not_an_extension_is_refused() {
        let dir = temp("not-appex");
        let app = dir.join("Demo.app");
        fs::create_dir_all(&app).unwrap();
        let config = config(vec![AppExtension {
            path: dir.join("Missing.appex"),
            ..AppExtension::default()
        }]);
        let error = embed(&config, &app, &app_info(), false)
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("nsh.json ios.extensions[0].path: ")
                && error.contains("is not an app extension"),
            "{error}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn two_extensions_with_one_identifier_are_refused() {
        let dir = temp("duplicate");
        let first = dir.join("a");
        let second = dir.join("b");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let config = config(vec![
            AppExtension {
                path: appex(&first, "Widget", "com.example.app.Widget", "iPhoneOS"),
                ..AppExtension::default()
            },
            AppExtension {
                path: appex(&second, "Other", "com.example.app.Widget", "iPhoneOS"),
                ..AppExtension::default()
            },
        ]);
        let app = dir.join("Demo.app");
        fs::create_dir_all(&app).unwrap();
        let error = embed(&config, &app, &app_info(), false)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("ios.extensions[0] has the identifier com.example.app.Widget too"),
            "{error}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
