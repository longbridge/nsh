//! OHOS embedding SDK cross-compilation (`libnsh_platform_ohos.so`,
//! `index.d.ts` and `@nsh/ohos` staging) with the OpenHarmony NDK.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{android, term};

const TRIPLE: &str = "aarch64-unknown-linux-ohos";
const CLANG_TARGET: &str = "aarch64-linux-ohos";

/// Locations of the OpenHarmony SDK (`openharmony` directory and `native` NDK).
pub(crate) struct Sdk {
    pub(crate) root: PathBuf,
    pub(crate) native: PathBuf,
}

impl Sdk {
    pub(crate) fn find() -> anyhow::Result<Sdk> {
        let mut candidates = Vec::new();
        for var in [
            "OHOS_NDK_HOME",
            "DEVECO_SDK_HOME",
            "HOS_SDK_HOME",
            "DEVECO_STUDIO_HOME",
            "DEVECO_CONTENTS",
        ] {
            if let Some(val) = env::var_os(var) {
                candidates.push(PathBuf::from(val));
            }
        }
        if cfg!(target_os = "macos") {
            candidates.push(PathBuf::from(
                "/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony",
            ));
            if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
                candidates.push(home.join("Library/OpenHarmony/Sdk"));
                candidates.push(home.join("Library/Huawei/Sdk"));
                if let Ok(entries) = fs::read_dir(home.join(".meat/ide")) {
                    for entry in entries.flatten() {
                        candidates.push(entry.path().join("Contents/sdk/default/openharmony"));
                    }
                }
            }
        } else if cfg!(windows) {
            if let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
                candidates.push(local.join("OpenHarmony/Sdk"));
                candidates.push(local.join("Huawei/Sdk"));
            }
            if let Some(pf) = env::var_os("ProgramFiles").map(PathBuf::from) {
                candidates.push(pf.join("Huawei/DevEco Studio/sdk/default/openharmony"));
            }
            candidates.push(PathBuf::from(
                r"C:\Program Files\Huawei\DevEco Studio\sdk\default\openharmony",
            ));
        } else {
            if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
                candidates.push(home.join("OpenHarmony/Sdk"));
                candidates.push(home.join(".Huawei/Sdk"));
            }
            candidates.push(PathBuf::from("/opt/DevEco-Studio/sdk/default/openharmony"));
            candidates.push(PathBuf::from(
                "/opt/command-line-tools/sdk/default/openharmony",
            ));
        }

        for candidate in candidates {
            if let Some(sdk) = Self::resolve(&candidate) {
                return Ok(sdk);
            }
        }
        bail!(
            "OpenHarmony NDK not found: set OHOS_NDK_HOME to the SDK's `openharmony` directory (containing `native`)"
        )
    }

    fn resolve(path: &Path) -> Option<Sdk> {
        if !path.exists() {
            return None;
        }
        // Direct `<openharmony>/native` path.
        if path.join("llvm/bin").is_dir() && path.join("sysroot").is_dir() {
            let root = path.parent().unwrap_or(path).to_path_buf();
            return Some(Sdk {
                root,
                native: path.to_path_buf(),
            });
        }
        // Standard `<openharmony>` directory containing `native`.
        let native = path.join("native");
        if native.join("llvm/bin").is_dir() && native.join("sysroot").is_dir() {
            return Some(Sdk {
                root: path.to_path_buf(),
                native,
            });
        }
        // DevEco SDK or Studio directory (`default/openharmony` or `sdk/default/openharmony`).
        for sub in [
            "default/openharmony",
            "sdk/default/openharmony",
            "openharmony",
        ] {
            let child = path.join(sub);
            if let Some(sdk) = Self::resolve(&child) {
                return Some(sdk);
            }
        }
        // Versioned SDK directory (`<Sdk>/<version>/openharmony` or `<Sdk>/<version>/native`).
        newest(path).and_then(|v| Self::resolve(&v))
    }
}

/// The highest version-named subdirectory of `dir`.
fn newest(dir: &Path) -> Option<PathBuf> {
    let mut versions: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    let key = |p: &PathBuf| -> Vec<u64> {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .split(['.', '-'])
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    };
    versions.sort_by_key(key);
    versions.pop()
}

/// A cargo command configured to cross-compile for `aarch64-unknown-linux-ohos`
/// on macOS, Linux or Windows using the OpenHarmony NDK.
fn ndk_cargo(sdk: &Sdk) -> Command {
    let llvm = sdk.native.join("llvm");
    let bin = llvm.join("bin");
    let sysroot = sdk.native.join("sysroot");
    let exe = env::consts::EXE_SUFFIX;
    let clang = bin.join(format!("clang{exe}"));
    let clang_xx = bin.join(format!("clang++{exe}"));
    let wrapper_c = bin.join(format!("{TRIPLE}-clang"));
    let wrapper_cxx = bin.join(format!("{TRIPLE}-clang++"));
    let (cc, cxx, use_wrapper) = if cfg!(unix) && wrapper_c.exists() && wrapper_cxx.exists() {
        (wrapper_c, wrapper_cxx, true)
    } else {
        (clang.clone(), clang_xx, false)
    };

    let env_triple = TRIPLE.replace('-', "_");
    let mut cmd = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.current_dir(android::root())
        .env("OHOS_NDK_HOME", &sdk.root)
        .env(format!("CC_{env_triple}"), &cc)
        .env(format!("CXX_{env_triple}"), &cxx)
        .env(
            format!("AR_{env_triple}"),
            bin.join(format!("llvm-ar{exe}")),
        )
        .env(
            format!("RANLIB_{env_triple}"),
            bin.join(format!("llvm-ranlib{exe}")),
        )
        .env(
            format!("CARGO_TARGET_{}_LINKER", env_triple.to_uppercase()),
            &clang,
        )
        .env("CLANG_PATH", &clang);

    let sysroot_slash = sysroot.display().to_string().replace('\\', "/");
    if !use_wrapper {
        let cflags = format!("--target={CLANG_TARGET} --sysroot=\"{sysroot_slash}\" -D__MUSL__");
        cmd.env(format!("CFLAGS_{env_triple}"), &cflags)
            .env(format!("CXXFLAGS_{env_triple}"), &cflags);
    }

    // Use unit-separated encoded rustflags so Windows paths with spaces survive.
    let mut rustflags = vec![
        format!("-Clink-arg=--target={CLANG_TARGET}"),
        format!("-Clink-arg=--sysroot={}", sysroot.display()),
        "-Clink-arg=-D__MUSL__".to_owned(),
    ];
    if let Some(existing) = env::var_os("CARGO_ENCODED_RUSTFLAGS") {
        let existing = existing.to_string_lossy();
        if !existing.is_empty() {
            rustflags.insert(0, existing.into_owned());
        }
    }
    cmd.env("CARGO_ENCODED_RUSTFLAGS", rustflags.join("\x1f"));

    if env::var_os("LIBCLANG_PATH").is_none() {
        let libclang_dir = if cfg!(windows) && bin.join("libclang.dll").exists() {
            bin
        } else {
            llvm.join("lib")
        };
        if libclang_dir.is_dir() {
            cmd.env("LIBCLANG_PATH", libclang_dir);
        }
    }

    // bindgen splits this variable like a shell: quoted, with forward slashes,
    // so that the backslashes and spaces of a Windows path survive.
    cmd.env(
        format!("BINDGEN_EXTRA_CLANG_ARGS_{env_triple}"),
        format!("--target={CLANG_TARGET} --sysroot=\"{sysroot_slash}\" -D__MUSL__"),
    );
    cmd
}

pub(crate) fn build(release: bool, dev: bool, mut features: Vec<String>) -> anyhow::Result<()> {
    features.sort();
    features.dedup();
    ensure!(
        features
            .iter()
            .all(|f| matches!(f.as_str(), "api-23" | "api-26")),
        "OHOS features must be api-23 or api-26"
    );
    let minimum_api = if features.iter().any(|f| f == "api-26") {
        26
    } else if features.iter().any(|f| f == "api-23") {
        23
    } else {
        20
    };
    let sdk = Sdk::find()?;
    let root = android::root();
    let cargo_profile = if release { "release" } else { "debug" };
    let profile = if dev {
        format!("{cargo_profile}-dev")
    } else {
        cargo_profile.into()
    };
    let profile = if features.is_empty() {
        profile
    } else {
        format!("{profile}-{}", features.join("-"))
    };
    let out = root.join("target/ohos").join(&profile);
    if dev {
        features.push("dev".into());
    }

    // Resolve the locked feature graph once before compiling.
    let mut metadata = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    metadata.current_dir(&root).args([
        "metadata",
        "--locked",
        "--format-version",
        "1",
        "--filter-platform",
        TRIPLE,
    ]);
    if !features.is_empty() {
        metadata.args([
            "--features",
            &features
                .iter()
                .map(|f| format!("nsh-platform-ohos/{f}"))
                .collect::<Vec<_>>()
                .join(","),
        ]);
    }
    let metadata = metadata
        .output()
        .context("read locked OHOS dependency graph")?;
    ensure!(
        metadata.status.success(),
        "Cargo metadata failed: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: Value = serde_json::from_slice(&metadata.stdout)?;
    let packages = metadata["packages"]
        .as_array()
        .context("Cargo package inventory")?;
    let bindings: BTreeSet<_> = packages
        .iter()
        .filter(|p| {
            p["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("ohos-") && name != "ohos-enum-derive")
        })
        .map(|p| p["source"].as_str().unwrap_or("path"))
        .collect();
    ensure!(
        bindings.len() == 1,
        "OHOS native packages must share one binding source: {bindings:?}"
    );
    let binding_source = *bindings.first().context("OHOS bindings")?;
    let path_development = env::var_os("NSH_ALLOW_PATH_DEPENDENCIES").is_some();
    ensure!(
        path_development
            || binding_source.starts_with("registry+")
            || binding_source.starts_with("git+"),
        "SDK dependencies must use crates.io releases or pinned binding Git revisions"
    );

    let typedef_dir = out.join(".typedef");
    fs::create_dir_all(&typedef_dir)?;
    let typedef_file = typedef_dir.join("nsh-platform-ohos");

    let mut command = ndk_cargo(&sdk);
    command
        .args([
            "build",
            "-p",
            "nsh-platform-ohos",
            "--target",
            TRIPLE,
            "--locked",
        ])
        .env("NAPI_TYPE_DEF_TMP_FOLDER", &typedef_dir);
    if !typedef_file.exists() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        command.env("NAPI_FORCE_BUILD_NSH_PLATFORM_OHOS", stamp);
    }
    if release {
        command.arg("--release");
    }
    if !features.is_empty() {
        command.args(["--features", &features.join(",")]);
    }
    term::cargo(&mut command, "libnsh_platform_ohos.so for arm64-v8a")?;

    let built_so = root
        .join("target")
        .join(TRIPLE)
        .join(cargo_profile)
        .join("libnsh_platform_ohos.so");
    let library_dir = out.join("arm64-v8a");
    fs::create_dir_all(&library_dir)?;
    let library = library_dir.join("libnsh_platform_ohos.so");
    fs::copy(&built_so, &library)
        .with_context(|| format!("copy {} to {}", built_so.display(), library.display()))?;

    let declarations = out.join("index.d.ts");
    write_type_defs(&typedef_file, &declarations)?;
    stage_arkts_deps(&root, &declarations)?;
    fs::write(
        out.join("build-metadata.json"),
        serde_json::to_vec_pretty(&json!({
            "minimumApi": minimum_api, "features": features, "profile": profile,
            "bindingSource": binding_source,
            "bindingGitRevision": binding_source.rsplit('#').next().filter(|_| binding_source.starts_with("git+")),
            "librarySha256": sha256(&library)?, "declarationsSha256": sha256(&declarations)?,
            "pathDevelopment": path_development,
        }))?,
    )?;
    term::done(format!(
        "OHOS API {minimum_api} embedding SDK: {}",
        out.strip_prefix(&root).unwrap_or(&out).display()
    ));
    Ok(())
}

/// Clippy on the OHOS crate across its feature sets (it is empty on other targets).
pub(crate) fn clippy() -> anyhow::Result<()> {
    let sdk = Sdk::find()?;
    for features in [None, Some("dev"), Some("api-23"), Some("api-26")] {
        let mut cmd = ndk_cargo(&sdk);
        cmd.args([
            "clippy",
            "-p",
            "nsh-platform-ohos",
            "--target",
            TRIPLE,
            "--lib",
            "--no-deps",
        ]);
        if let Some(f) = features {
            cmd.args(["--features", f]);
        }
        cmd.args(["--", "-D", "warnings"]);
        let status = cmd.status()?;
        if !status.success() {
            bail!("clippy found problems ({})", features.unwrap_or("default"));
        }
    }
    Ok(())
}

/// Formats `napi-derive-ohos`'s intermediate JSON lines into `index.d.ts`.
fn write_type_defs(typedef_file: &Path, declarations: &Path) -> anyhow::Result<()> {
    let raw = fs::read_to_string(typedef_file).with_context(|| {
        format!(
            "read generated type definitions from {}",
            typedef_file.display()
        )
    })?;
    let mut structs: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut impls: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut other: Vec<(String, String)> = Vec::new();

    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let item: Value = serde_json::from_str(line)?;
        let kind = item["kind"].as_str().unwrap_or_default();
        let name = item["name"].as_str().unwrap_or_default().to_owned();
        let js_doc = item["js_doc"].as_str().unwrap_or_default();
        let def = item["def"].as_str().unwrap_or_default();
        match kind {
            "struct" => {
                structs.insert(name, (js_doc.to_owned(), def.to_owned()));
            }
            "impl" => {
                if !def.is_empty() {
                    impls.entry(name).or_default().push(def.to_owned());
                }
            }
            "interface" => {
                let js_doc = js_doc.replace("\n *", "\n  *");
                let body = def.replace("\\n", "\n").replace('\n', "\n  ");
                other.push((
                    name.clone(),
                    format!("{js_doc}export interface {name} {{\n  {body}\n}}"),
                ));
            }
            "fn" => {
                let js_doc = js_doc.replace("\n *", "\n  *");
                let def = def.replace("\\n", "\n").replace('\n', "\n  ");
                other.push((name, format!("{js_doc}export declare {def}")));
            }
            "enum" => {
                let js_doc = js_doc.replace("\n *", "\n  *");
                let body = def.replace("\\n", "\n").replace('\n', "\n  ");
                other.push((
                    name.clone(),
                    format!("{js_doc}export declare const enum {name} {{\n  {body}\n}}"),
                ));
            }
            "type" | "const" => {
                let js_doc = js_doc.replace("\n *", "\n  *");
                other.push((name, format!("{js_doc}{def}")));
            }
            _ => {}
        }
    }

    other.sort_by(|a, b| a.0.cmp(&b.0));

    let mut defs: Vec<String> = other.into_iter().map(|(_, d)| d).collect();
    for (name, (js_doc, struct_def)) in structs {
        let js_doc = js_doc.replace("\n *", "\n  *");
        let mut parts = Vec::new();
        if !struct_def.is_empty() {
            parts.push(struct_def);
        }
        if let Some(imp) = impls.remove(&name) {
            parts.extend(imp);
        }
        let body = parts
            .join("\n")
            .replace("\\n ", "\n")
            .replace("\\n", "\n")
            .replace("\n ", "\n")
            .trim_start_matches(' ')
            .replace("\n *", "\n  *")
            .replace('\n', "\n  ");
        defs.push(format!(
            "{js_doc}export declare class {name} {{\n  {body}\n}}"
        ));
    }

    let mut content = String::from("/* auto-generated by OHOS-RS */\n/* eslint-disable */\n\n");
    content.push_str(&defs.join("\n\n"));
    content.push('\n');
    fs::write(declarations, content)?;
    Ok(())
}

fn stage_arkts_deps(root: &Path, declarations: &Path) -> anyhow::Result<()> {
    let dest_root = root.join("target/ohos/arkts-deps");
    if dest_root.exists() {
        fs::remove_dir_all(&dest_root)?;
    }
    let nsh_dir = dest_root.join("nsh");
    fs::create_dir_all(&nsh_dir)?;
    copy_dir_filtered(&root.join("crates/nsh-platform-ohos/arkts"), &nsh_dir)?;
    let types_dir = nsh_dir.join("src/main/cpp/types/libnsh_platform_ohos");
    fs::create_dir_all(&types_dir)?;
    fs::copy(declarations, types_dir.join("Index.d.ts"))?;
    fs::write(
        types_dir.join("oh-package.json5"),
        serde_json::to_vec_pretty(&json!({
            "name": "libnsh_platform_ohos.so",
            "types": "./Index.d.ts",
            "version": "0.1.0",
            "description": "Native Shell OHOS embedding SDK"
        }))?,
    )?;
    Ok(())
}

fn copy_dir_filtered(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(
            name.to_str(),
            Some("node_modules" | "oh_modules" | ".preview" | "build")
        ) {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir_filtered(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn sha256(path: &Path) -> anyhow::Result<String> {
    let mut reader = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
