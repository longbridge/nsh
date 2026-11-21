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
//! `<activity>` and `<meta-data>` elements) are read; other keys are
//! reported and left out. Each Android entry is one balanced element of
//! those, so a crate cannot close `<application>` or `<queries>` to write
//! elsewhere in the manifest.

use std::{collections::BTreeMap, path::Path, process::Command};

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
            meta.merge_section(name, "[package.metadata.nsh]", nsh, &mut warnings)?;
            let features = nsh.get("features").and_then(Value::as_object);
            let enabled = node["features"].as_array().into_iter().flatten();
            for feature in enabled.filter_map(Value::as_str) {
                if let Some(section) = features
                    .and_then(|f| f.get(feature))
                    .and_then(Value::as_object)
                {
                    let at = format!("[package.metadata.nsh.features.{feature}]");
                    meta.merge_section(name, &at, section, &mut warnings)?;
                }
            }
        }
        Ok((meta, warnings))
    }

    /// Merges one section (`at`, of package `name`): its `ios` and
    /// `android` tables.
    fn merge_section(
        &mut self,
        name: &str,
        at: &str,
        section: &Map<String, Value>,
        warnings: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        for (key, value) in section {
            match key.as_str() {
                // Read by itself: a feature's section is merged only when on.
                "features" if at == "[package.metadata.nsh]" => {}
                "ios" => self.merge_ios(name, at, value, warnings)?,
                "android" => self.merge_android(name, at, value, warnings)?,
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
        name: &str,
        at: &str,
        android: &Value,
        warnings: &mut Vec<String>,
    ) -> anyhow::Result<()> {
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
                _ => warnings.push(format!(
                    "{name}: {at} `android.{key}` is not something this nsh reads: left out"
                )),
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
            json!({ "nsh": { "android": { "dex": ["a.dex"] }, "ios": { "privacy": {} } } }),
            &[],
        )]))
        .unwrap();
        assert_eq!(meta, ModuleMeta::default());
        assert_eq!(warnings.len(), 2, "{warnings:?}");
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
