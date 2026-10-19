//! `nsh init` (a new app) and the TypeScript declarations editors read
//! (`.nsh/types`, refreshed by every command that builds an app).

use std::{fs, path::Path};

use anyhow::{Context as _, bail};

use crate::ui;

const APP: &str = r##"import { mount, signal, Text, View } from "nsh";
import { SafeAreaView } from "nsh/ui/mobile";

function App() {
  const count = signal(0);
  return (
    <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 16 }}>
      <Text style={{ fontSize: 28, fontWeight: "600" }}>{count()}</Text>
      <View
        onPress={() => count.update((c) => c + 1)}
        activeOpacity={0.6}
        accessibilityRole="button"
        style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 10, backgroundColor: "#0969da" }}
      >
        <Text style={{ color: "white", fontSize: 17 }}>Tap</Text>
      </View>
    </SafeAreaView>
  );
}

mount(App);
"##;

/// What an app's repository leaves out: generated types and workspaces
/// (`.nsh`), bundles (`out`) and Cargo's builds (`target`: an app's own
/// crate, and its hosts once `nsh.json` names a `"rust"` crate).
const GITIGNORE: &str = ".nsh/\nout/\ntarget/\n";

/// Creates an app in `dir`: `App.tsx` (the entry by convention), `nsh.json`, `tsconfig.json`,
/// `.gitignore` and the type declarations. `rust` adds a Cargo project with
/// shared Rust components, depending on nsh's crates on crates.io at this
/// CLI's version, or by path on the nsh checkout `nsh_source`: where the
/// app's hosts take nsh from too (`rust_app::Nsh::of`). Existing Rust app
/// files are reported as conflicts; ordinary TSX initialization keeps
/// existing files.
pub(crate) fn init(
    dir: &Path,
    name: Option<String>,
    rust: bool,
    nsh_source: Option<&Path>,
) -> anyhow::Result<()> {
    let nsh_source = nsh_source
        .map(|source| {
            let source = dunce::canonicalize(source)
                .with_context(|| format!("--nsh-source: no {}", source.display()))?;
            if !source.join("crates/nsh-core/Cargo.toml").is_file() {
                bail!("--nsh-source: {} is not an nsh checkout", source.display());
            }
            Ok(source)
        })
        .transpose()?;
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let run = if dir == Path::new(".") {
        "nsh run".to_owned()
    } else {
        format!("nsh run {}", dir.display())
    };
    let dir = dunce::canonicalize(dir)?;
    let name = name.unwrap_or_else(|| {
        dir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app")
            .to_owned()
    });
    let id_part: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    let mut config = serde_json::json!({
        "name": name,
        "id": format!("com.example.{}", if id_part.is_empty() { "app" } else { &id_part }),
        "version": "1.0.0",
        "build": 1,
    });
    if rust {
        config["rust"] = ".".into();
        // A Rust app: `nsh run` / `nsh package` mount this root with no JS
        // runtime; `nsh run App.tsx` runs the TSX entry instead.
        config["root"] = "App".into();
    }
    let mut files = vec![
        ("App.tsx", if rust { RUST_APP } else { APP }.to_owned()),
        (
            "nsh.json",
            format!("{}\n", serde_json::to_string_pretty(&config)?),
        ),
        ("tsconfig.json", nsh_bundler::TSCONFIG.to_owned()),
        (".gitignore", GITIGNORE.to_owned()),
    ];
    if rust {
        files.extend(rust_files(
            &name,
            config["id"].as_str().unwrap(),
            nsh_source.as_deref(),
        ));
        for file in [
            "Cargo.toml",
            "src/main.rs",
            "src/lib.rs",
            "App.tsx",
            "nsh.json",
        ] {
            if dir.join(file).exists() {
                bail!(
                    "{} already exists; create the Rust app in a new directory",
                    ui::path(&dir.join(file))
                );
            }
        }
    }
    if files.iter().all(|(f, _)| dir.join(f).exists()) {
        bail!("{} already has an app", ui::path(&dir));
    }
    let (mut created, mut kept) = (Vec::new(), Vec::new());
    for (file, contents) in files {
        let path = dir.join(file);
        if path.exists() {
            kept.push(file);
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents).with_context(|| format!("write {}", path.display()))?;
        created.push(file);
    }
    write_types(&dir);
    let kept = if kept.is_empty() {
        String::new()
    } else {
        format!(" {}", ui::dim(format!("(kept {})", kept.join(", "))))
    };
    ui::done(format!(
        "created {}: {}{kept}",
        ui::path(&dir),
        created.join(", ")
    ));
    if rust {
        ui::note(format!(
            "run the Rust app with `cargo run --manifest-path {}`",
            ui::path(&dir.join("Cargo.toml"))
        ));
        ui::note(format!("run the TSX entry with `{run}`"));
    } else {
        ui::note(format!("run it with `{run}`"));
    }
    Ok(())
}

const RUST_APP: &str = r#"import { mount } from "nsh";
import { App } from "app/native";

mount(App);
"#;

/// The Cargo project: nsh's crates on crates.io at this CLI's version (or
/// by path in the checkout `source`), with platform features chosen so
/// `cargo run` runs the Rust root without linking QuickJS.
fn rust_files(name: &str, id: &str, source: Option<&Path>) -> Vec<(&'static str, String)> {
    let package = rust_package_name(name);
    let version = env!("CARGO_PKG_VERSION");
    let dep = |name: &str, extra: &str| match source {
        Some(source) => format!(
            "{{ path = {:?}{extra} }}",
            source.join("crates").join(name).to_string_lossy()
        ),
        None if extra.is_empty() => format!("{version:?}"),
        None => format!("{{ version = {version:?}{extra} }}"),
    };
    let core = dep("nsh-core", "");
    let mobile = dep("nsh-ui-mobile", "");
    let mut manifest = format!(
        r#"[package]
name = {package:?}
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
nsh-core = {core}
nsh-ui-mobile = {mobile}
env_logger = "0.11"
"#
    );
    for platform in ["linux", "macos", "windows"] {
        manifest.push_str(&format!(
            r#"
[target.'cfg(target_os = {platform:?})'.dependencies]
nsh-platform-{platform} = {}
"#,
            dep(
                &format!("nsh-platform-{platform}"),
                ", default-features = false"
            )
        ));
    }
    let main = include_str!("templates/rust/main.rs")
        .replace("__APP_CRATE__", &package.replace('-', "_"))
        .replace("__APP_TITLE__", &format!("{name:?}"))
        .replace("__APP_ID__", &format!("{id:?}"));
    vec![
        ("Cargo.toml", manifest),
        ("src/main.rs", main),
        (
            "src/lib.rs",
            include_str!("templates/rust/lib.rs").to_owned(),
        ),
        (
            "README.md",
            include_str!("templates/rust/README.md").to_owned(),
        ),
    ]
}

/// A valid, non-reserved Cargo name, even for display names containing
/// whitespace, punctuation, Unicode or a leading digit.
fn rust_package_name(name: &str) -> String {
    let slug = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-");
    format!("nsh-app-{}", if slug.is_empty() { "app" } else { &slug })
}

/// Writes `.nsh/types` in the app directory `dir` (files whose contents did
/// not change are left alone, so editors do not reload them). Files nsh no
/// longer generates (a module that moved) are removed.
pub(crate) fn write_types(dir: &Path) {
    let types = dir.join(".nsh/types");
    let mut definitions = nsh_js::type_definitions(crate::ui_sets());
    definitions.extend(nsh_bundler::type_definitions());
    remove_stale(&types, &types, &definitions);
    for (path, contents) in definitions {
        let path = types.join(path);
        if fs::read(&path).is_ok_and(|old| old == contents.as_bytes()) {
            continue;
        }
        let written = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, contents));
        if let Err(err) = written {
            ui::warn(format!("cannot write {}: {err}", ui::path(&path)));
            return;
        }
    }
}

/// Removes the files under `dir` (inside `types`) that are not among
/// `definitions`, and directories left empty.
fn remove_stale(types: &Path, dir: &Path, definitions: &[(String, String)]) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_stale(types, &path, definitions);
            let _ = fs::remove_dir(&path);
        } else if let Ok(relative) = path.strip_prefix(types) {
            let relative = relative.to_string_lossy().replace('\\', "/");
            if !definitions.iter().any(|(p, _)| *p == relative) {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// [`write_types`] for the app whose entry is `entry`.
pub(crate) fn write_types_for(entry: &Path) {
    if let Some(dir) = dunce::canonicalize(entry)
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        write_types(&dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "nsh-init-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn rust_projects_have_two_entries_and_preserve_existing_projects() {
        let dir = directory("rust");
        init(&dir, Some("123 My App".into()), true, None).unwrap();
        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("nsh.json")).unwrap()).unwrap();
        assert_eq!(config["rust"], ".");
        assert_eq!(config["root"], "App");
        assert_eq!(config["name"], "123 My App");
        assert!(
            fs::read_to_string(dir.join("Cargo.toml"))
                .unwrap()
                .contains("nsh-app-123-my-app")
        );
        assert!(
            fs::read_to_string(dir.join("src/main.rs"))
                .unwrap()
                .contains("nsh_app_123_my_app::App")
        );
        assert!(
            fs::read_to_string(dir.join("App.tsx"))
                .unwrap()
                .contains("app/native")
        );
        fs::write(dir.join("src/lib.rs"), "user code").unwrap();
        assert!(init(&dir, None, true, None).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("src/lib.rs")).unwrap(),
            "user code"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rust_projects_take_nsh_from_crates_io_or_a_checkout() {
        let version = env!("CARGO_PKG_VERSION");
        let manifest = |source: Option<&Path>| {
            rust_files("App", "com.example.app", source)
                .into_iter()
                .find(|(name, _)| *name == "Cargo.toml")
                .unwrap()
                .1
        };
        let published = manifest(None);
        assert!(
            published.contains(&format!("nsh-core = {version:?}")),
            "{published}"
        );
        assert!(published.contains(&format!(
            "nsh-platform-macos = {{ version = {version:?}, default-features = false }}"
        )));
        let checkout =
            dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
        let local = manifest(Some(&checkout));
        let core = checkout.join("crates/nsh-core");
        assert!(
            local.contains(&format!(
                "nsh-core = {{ path = {:?} }}",
                core.to_string_lossy()
            )),
            "{local}"
        );
        assert!(local.contains("default-features = false }"), "{local}");
        assert!(!local.contains(&format!("{version:?}")), "{local}");
        // Only a checkout.
        let dir = directory("not-a-checkout");
        let error = init(&dir.join("app"), None, true, Some(&dir))
            .unwrap_err()
            .to_string();
        assert!(error.contains("not an nsh checkout"), "{error}");
        assert!(!dir.join("app").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rust_init_refuses_conflicts_before_writing_app_files() {
        let dir = directory("conflict");
        fs::write(dir.join("Cargo.toml"), "existing manifest").unwrap();
        assert!(init(&dir, None, true, None).is_err());
        assert!(!dir.join("App.tsx").exists());
        assert!(!dir.join("nsh.json").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tsx_init_keeps_its_existing_behavior() {
        let dir = directory("tsx");
        fs::write(dir.join("App.tsx"), "user entry").unwrap();
        init(&dir, None, false, None).unwrap();
        assert!(!dir.join("Cargo.toml").exists());
        assert_eq!(
            fs::read_to_string(dir.join("App.tsx")).unwrap(),
            "user entry"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn display_names_produce_valid_cargo_names_and_escaped_rust_literals() {
        assert_eq!(rust_package_name("你好"), "nsh-app-app");
        assert_eq!(rust_package_name("123 APP / demo"), "nsh-app-123-app-demo");
        let files = rust_files("a\"b\nc", "com.example.abc", None);
        let main = &files
            .iter()
            .find(|(name, _)| *name == "src/main.rs")
            .unwrap()
            .1;
        assert!(main.contains(r#"title: "a\"b\nc".to_owned()"#));
    }
}
