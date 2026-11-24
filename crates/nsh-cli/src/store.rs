//! The store listing an app ships with, shared by every `nsh publish <store>`.
//!
//! It is read from `nsh.json` `"store"` and from a `store/` directory beside
//! it; a value in `nsh.json` wins over the file:
//!
//! ```text
//! store/
//!   en-US/
//!     description.txt  keywords.txt  whats_new.txt  ...
//!     screenshots/phone/01.png  screenshots/tablet/01.png
//! ```
//!
//! Locales are BCP 47 tags (`en-US`, `zh-Hans`). Screenshots are grouped by
//! device class (`phone`, `tablet`, `desktop`), which each store maps to its
//! own sizes; a store's own name for a size (`APP_IPHONE_67`) is taken as is
//! by that store and skipped by the others.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail, ensure};
use serde::{Deserialize, Serialize};

/// `nsh.json` `"store"`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoreConfig {
    /// The listing per locale.
    #[serde(default)]
    pub locales: BTreeMap<String, Listing>,
    /// App Store Connect settings.
    #[serde(default)]
    pub ios: IosStore,
    /// Settings of the other stores, read by their own `nsh publish`.
    #[serde(default, rename = "android")]
    _android: Option<serde_json::Value>,
    #[serde(default, rename = "windows")]
    _windows: Option<serde_json::Value>,
    #[serde(default, rename = "linux")]
    _linux: Option<serde_json::Value>,
}

/// One locale's listing. Every field is optional: a store leaves what is
/// not given as it is.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Listing {
    pub name: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<String>,
    pub promotional_text: Option<String>,
    /// Release notes of this version.
    pub whats_new: Option<String>,
    pub support_url: Option<String>,
    pub marketing_url: Option<String>,
    pub privacy_policy_url: Option<String>,
    /// Device class → image paths, relative to `nsh.json`, in display order.
    #[serde(default)]
    pub screenshots: BTreeMap<String, Vec<PathBuf>>,
}

/// `store.ios`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct IosStore {
    /// The app's numeric Apple ID in App Store Connect (App Information →
    /// Apple ID), so that `nsh publish ios` needs no `--app-id`. A string
    /// or a number.
    #[serde(default, deserialize_with = "numeric_id")]
    pub app_id: Option<String>,
    /// Export compliance answer set on every build (Apple asks for it
    /// before a build can be tested or submitted).
    pub uses_non_exempt_encryption: Option<bool>,
    /// TestFlight groups that get each processed build.
    #[serde(default)]
    pub beta_groups: Vec<String>,
    /// How an approved version is released (default: `manual`).
    pub release: Option<ReleaseMode>,
}

/// An id written as `"6835723652"` or `6835723652`.
fn numeric_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Text(String),
        Number(u64),
    }
    Ok(Option::<Id>::deserialize(d)?.map(|id| match id {
        Id::Text(text) => text,
        Id::Number(n) => n.to_string(),
    }))
}

/// How a version goes live once the store approves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReleaseMode {
    /// The developer releases it (in the console or with a later command).
    Manual,
    /// Released as soon as it is approved.
    Auto,
    /// Released on approval to a growing share of users over 7 days.
    Phased,
}

/// The text files `store/<locale>/` may hold, by listing field.
const TEXT_FILES: [&str; 9] = [
    "name",
    "subtitle",
    "description",
    "keywords",
    "promotional_text",
    "whats_new",
    "support_url",
    "marketing_url",
    "privacy_policy_url",
];

impl Listing {
    fn field(&mut self, name: &str) -> &mut Option<String> {
        match name {
            "name" => &mut self.name,
            "subtitle" => &mut self.subtitle,
            "description" => &mut self.description,
            "keywords" => &mut self.keywords,
            "promotional_text" => &mut self.promotional_text,
            "whats_new" => &mut self.whats_new,
            "support_url" => &mut self.support_url,
            "marketing_url" => &mut self.marketing_url,
            _ => &mut self.privacy_policy_url,
        }
    }
}

/// Reads the store listing of the app at `entry`: `nsh.json` `"store"`
/// merged over `store/`, with screenshot paths made absolute and checked.
pub(crate) fn read(entry: &Path) -> anyhow::Result<StoreConfig> {
    let entry = &app_entry(entry);
    let dir = nsh_bundler::app_dir::config_dir_of(entry);
    let config = crate::package::read_config(entry)?;
    let mut store: StoreConfig = match config.store {
        Some(value) => serde_json::from_value(value).context("invalid nsh.json `store`")?,
        None => StoreConfig::default(),
    };
    merge_dir(&mut store, &dir.join("store"))?;
    for (locale, listing) in &mut store.locales {
        ensure!(
            valid_locale(locale),
            "store locale `{locale}` is not a BCP 47 tag like en-US"
        );
        for (class, paths) in &mut listing.screenshots {
            for path in paths {
                if path.is_relative() {
                    *path = dir.join(&*path);
                }
                ensure!(
                    image_kind(path).is_some(),
                    "store screenshot {} ({locale}, {class}) must be a .png or .jpg",
                    path.display()
                );
                ensure!(
                    path.is_file(),
                    "store screenshot {} ({locale}, {class}) does not exist",
                    path.display()
                );
            }
        }
    }
    Ok(store)
}

/// `entry` as `package::read_config` takes it: an app's directory names its
/// `nsh.json`.
pub(crate) fn app_entry(entry: &Path) -> PathBuf {
    if entry.is_dir() {
        entry.join("nsh.json")
    } else {
        entry.to_owned()
    }
}

/// Fills what `nsh.json` left out from `store/<locale>/`.
fn merge_dir(store: &mut StoreConfig, dir: &Path) -> anyhow::Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(locale) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let listing = store.locales.entry(locale).or_default();
        for name in TEXT_FILES {
            let field = listing.field(name);
            if field.is_none()
                && let Ok(text) = std::fs::read_to_string(entry.path().join(format!("{name}.txt")))
            {
                *field = Some(text.trim().to_owned());
            }
        }
        let Ok(classes) = std::fs::read_dir(entry.path().join("screenshots")) else {
            continue;
        };
        for class in classes {
            let class = class?;
            let Some(name) = class.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !class.file_type()?.is_dir() || listing.screenshots.contains_key(&name) {
                continue;
            }
            let mut images: Vec<PathBuf> = std::fs::read_dir(class.path())?
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|p| image_kind(p).is_some())
                .collect();
            images.sort();
            if !images.is_empty() {
                listing.screenshots.insert(name, images);
            }
        }
    }
    Ok(())
}

pub(crate) fn image_kind(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

fn valid_locale(tag: &str) -> bool {
    let mut parts = tag.split('-');
    parts
        .next()
        .is_some_and(|p| (2..=3).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_alphabetic()))
        && parts.all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_alphanumeric()))
}

/// Where a store has a release, as `nsh publish status` reports it. The exit
/// code lets a script or agent poll without reading the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StoreState {
    /// Live, or approved and waiting only for a manual release: exit 0.
    Live,
    /// Processing or in review: nothing to do but wait. Exit 10.
    Pending,
    /// Being prepared, not submitted: exit 11.
    Draft,
    /// Rejected or invalid: someone has to act. Exit 12.
    ActionNeeded,
}

impl StoreState {
    pub(crate) fn exit_code(self) -> i32 {
        match self {
            Self::Live => 0,
            Self::Pending => 10,
            Self::Draft => 11,
            Self::ActionNeeded => 12,
        }
    }
}

/// Percent-encodes a value for a URL query.
pub(crate) fn query_value(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// One check of `nsh publish doctor`.
pub(crate) struct Check {
    failed: bool,
}

impl Check {
    pub(crate) fn new() -> Self {
        Self { failed: false }
    }

    /// Prints one result; a failure makes the whole check fail.
    pub(crate) fn report(&mut self, what: &str, result: anyhow::Result<String>) {
        match result {
            Ok(detail) => println!(
                "ok    {what}{}",
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            ),
            Err(e) => {
                self.failed = true;
                println!("FAIL  {what}: {e:#}");
            }
        }
    }

    pub(crate) fn finish(self) -> anyhow::Result<()> {
        if self.failed {
            bail!("some checks failed");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nsh_json_wins_over_store_files_and_screenshots_are_found_in_order() {
        let dir = std::env::temp_dir().join(format!("nsh-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let shots = dir.join("store/en-US/screenshots/phone");
        std::fs::create_dir_all(&shots).unwrap();
        std::fs::create_dir_all(dir.join("store/zh-Hans")).unwrap();
        std::fs::write(dir.join("store/en-US/whats_new.txt"), "From file\n").unwrap();
        std::fs::write(dir.join("store/en-US/description.txt"), "Described\n").unwrap();
        std::fs::write(dir.join("store/zh-Hans/whats_new.txt"), "更新").unwrap();
        for name in ["02.png", "01.png", "notes.md"] {
            std::fs::write(shots.join(name), b"x").unwrap();
        }
        std::fs::write(
            dir.join("nsh.json"),
            r#"{"id":"com.example.app","store":{"locales":{"en-US":{"whatsNew":"Inline"}},
               "ios":{"appId":6835723652,"usesNonExemptEncryption":false,"betaGroups":["QA"],"release":"phased"}}}"#,
        )
        .unwrap();
        let store = read(&dir).unwrap();
        let en = &store.locales["en-US"];
        assert_eq!(en.whats_new.as_deref(), Some("Inline"));
        assert_eq!(en.description.as_deref(), Some("Described"));
        assert_eq!(
            en.screenshots["phone"],
            vec![shots.join("01.png"), shots.join("02.png")]
        );
        assert_eq!(store.locales["zh-Hans"].whats_new.as_deref(), Some("更新"));
        assert_eq!(store.ios.app_id.as_deref(), Some("6835723652"));
        assert_eq!(store.ios.uses_non_exempt_encryption, Some(false));
        assert_eq!(store.ios.beta_groups, ["QA"]);
        assert_eq!(store.ios.release, Some(ReleaseMode::Phased));

        std::fs::write(
            dir.join("nsh.json"),
            r#"{"store":{"locales":{"en-US":{"screenshots":{"phone":["missing.png"]}}}}}"#,
        )
        .unwrap();
        assert!(read(&dir).is_err());
        std::fs::write(
            dir.join("nsh.json"),
            r#"{"store":{"locales":{"english":{}}}}"#,
        )
        .unwrap();
        assert!(read(&dir).is_err());
        std::fs::write(dir.join("nsh.json"), r#"{"store":{"typo":1}}"#).unwrap();
        assert!(read(&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn query_values_are_escaped() {
        assert_eq!(query_value("1.2.0"), "1.2.0");
        assert_eq!(query_value("QA & Beta"), "QA%20%26%20Beta");
    }
}
