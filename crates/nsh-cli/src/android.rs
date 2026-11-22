//! Android apps without Gradle: the APK is assembled from the prebuilt
//! runtime (`classes.dex`, `libnsh.so` per ABI) and the app bundle with the
//! SDK's build tools (`aapt2`, `zipalign`, `apksigner`, and `d8` for the
//! Java libraries an app lists).

use std::{
    collections::BTreeMap,
    env, fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};
use serde::Deserialize;

use crate::apple::{Color, LaunchScreen, Themed};

mod libraries;
mod library_manifest;

pub(crate) use libraries::{Library, file_sha256};

/// Minimum Android version (API 26, Android 8.0), as the runtime is built.
pub(crate) const MIN_SDK: u32 = 26;
/// The Android version apps target unless `nsh.json` `android.targetSdk`
/// names another (API 35, Android 15: what Google Play requires).
pub(crate) const DEFAULT_TARGET_SDK: u32 = 35;
/// The ABIs the runtime is built for: `nsh package --abi`'s values.
pub(crate) const ABIS: [&str; 3] = ["arm64-v8a", "armeabi-v7a", "x86_64"];
/// The ABIs `nsh package` puts into an APK unless `--abi` names others:
/// the 64-bit and 32-bit processes of phones (`--run` adds the device's,
/// an emulator's `x86_64`).
pub(crate) const DEFAULT_ABIS: [&str; 2] = ["arm64-v8a", "armeabi-v7a"];

/// The Android SDK.
pub(crate) struct Sdk {
    root: PathBuf,
    build_tools: PathBuf,
}

impl Sdk {
    pub(crate) fn find() -> anyhow::Result<Sdk> {
        let root = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
            .iter()
            .find_map(env::var_os)
            .map(PathBuf::from)
            // Android Studio's default place.
            .or_else(|| {
                if cfg!(windows) {
                    env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Android/Sdk"))
                } else {
                    env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Android/sdk"))
                }
            })
            .filter(|p| p.exists())
            .context("Android SDK not found: install it and set ANDROID_HOME")?;
        let build_tools = newest(&root.join("build-tools"))
            .context("no SDK build-tools: install them with `sdkmanager \"build-tools;35.0.1\"`")?;
        Ok(Sdk { root, build_tools })
    }

    /// The platform an app targeting `level` compiles against.
    fn android_jar(&self, level: u32) -> anyhow::Result<PathBuf> {
        let jar = self
            .root
            .join(format!("platforms/android-{level}/android.jar"));
        if !jar.exists() {
            bail!(
                "{} missing: install it with `sdkmanager \"platforms;android-{level}\"`",
                jar.display()
            );
        }
        Ok(jar)
    }

    pub(crate) fn adb(&self) -> PathBuf {
        self.root.join("platform-tools/adb")
    }

    /// A cargo command set up to cross-compile for `triple` with the NDK
    /// (`ANDROID_NDK_HOME`, else the newest NDK of the SDK): an app's own
    /// `libnsh.so` (`rust_app.rs`).
    pub(crate) fn ndk_cargo(&self, triple: &str) -> anyhow::Result<Command> {
        let toolchain = self.ndk_toolchain()?;
        let bin = toolchain.join("bin");
        // On Windows the NDK's per-target compilers are batch files.
        let script = if cfg!(windows) { ".cmd" } else { "" };
        // ARMv7's compilers are named `armv7a-linux-androideabi`, the
        // others after Rust's triple.
        let clang_target = match triple {
            "armv7-linux-androideabi" => "armv7a-linux-androideabi",
            other => other,
        };
        let clang = bin.join(format!("{clang_target}{MIN_SDK}-clang{script}"));
        let env_triple = triple.replace('-', "_");
        // New NDKs keep the C standard headers their clang carries (stdbool.h
        // &c.) out of the sysroot: whatever libclang bindgen of the C
        // dependencies (quickjs-jit-sys) loads, give it that directory to
        // search as well. Last (`-idirafter`), so the libclang's own headers
        // still win and `#include_next` reaches the sysroot (#1282: `'stdbool.h'
        // file not found`, `unknown type name 'uint32_t'` on Windows).
        let clang_include = fs::read_dir(toolchain.join("lib/clang"))
            .ok()
            .and_then(|versions| {
                versions
                    .filter_map(|version| version.ok().map(|version| version.path()))
                    .map(|version| version.join("include"))
                    .find(|include| include.is_dir())
            });
        // bindgen splits this variable like a shell: quoted, with forward slashes,
        // so that the backslashes and spaces of a Windows path survive.
        let mut extra = format!(
            "--target={clang_target}{MIN_SDK} --sysroot=\"{}\"",
            toolchain
                .join("sysroot")
                .display()
                .to_string()
                .replace('\\', "/")
        );
        if let Some(include) = clang_include {
            let include = include.display().to_string().replace('\\', "/");
            extra.push_str(&format!(" -idirafter \"{include}\""));
        }
        let mut cmd = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        cmd.env(format!("CC_{env_triple}"), &clang)
            .env(format!("AR_{env_triple}"), bin.join("llvm-ar"))
            .env(
                format!("CARGO_TARGET_{}_LINKER", env_triple.to_uppercase()),
                &clang,
            )
            .env(format!("BINDGEN_EXTRA_CLANG_ARGS_{env_triple}"), extra);
        // Use the NDK's own libclang when it carries one (r27+ do not);
        // setting LIBCLANG_PATH otherwise would cut clang-sys's discovery off.
        let ndk_libclang = bin.join(if cfg!(windows) {
            "libclang.dll"
        } else {
            "libclang.so"
        });
        if ndk_libclang.exists() {
            cmd.env("LIBCLANG_PATH", &bin);
        }
        Ok(cmd)
    }

    /// The NDK's LLVM toolchain (`ANDROID_NDK_HOME`, else the newest NDK of
    /// the SDK).
    fn ndk_toolchain(&self) -> anyhow::Result<PathBuf> {
        let ndk = match env::var_os("ANDROID_NDK_HOME") {
            Some(ndk) => PathBuf::from(ndk),
            None => newest(&self.root.join("ndk"))
                .context("no NDK in the SDK: install it with `sdkmanager \"ndk;<version>\"`")?,
        };
        // The NDK's "darwin-x86_64" toolchain is universal.
        let host = if cfg!(target_os = "macos") {
            "darwin-x86_64"
        } else if cfg!(windows) {
            "windows-x86_64"
        } else {
            "linux-x86_64"
        };
        Ok(ndk.join("toolchains/llvm/prebuilt").join(host))
    }

    /// Writes `library` to `to` without the symbols nothing links against
    /// (the dynamic ones, `JNI_OnLoad` and the natives, stay): an app's
    /// release `libnsh.so`, which APKs store uncompressed.
    pub(crate) fn strip(&self, library: &Path, to: &Path) -> anyhow::Result<()> {
        run(Command::new(self.ndk_toolchain()?.join("bin/llvm-strip"))
            .arg("--strip-unneeded")
            .arg("-o")
            .arg(to)
            .arg(library))
    }

    fn tool(&self, name: &str) -> PathBuf {
        // On Windows, the Java tools are batch files.
        if cfg!(windows) && matches!(name, "apksigner" | "d8") {
            return self.build_tools.join(format!("{name}.bat"));
        }
        self.build_tools.join(name)
    }
}

fn newest(dir: &Path) -> Option<PathBuf> {
    let mut versions: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    versions.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .split(['.', '-'])
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>()
    });
    versions.pop()
}

/// What goes into an APK.
pub(crate) struct Apk<'a> {
    pub(crate) package: &'a str,
    pub(crate) label: &'a str,
    pub(crate) version_name: &'a str,
    pub(crate) version_code: u32,
    /// The API level the app targets (`targetSdkVersion`), which it also
    /// compiles against.
    pub(crate) target_sdk: u32,
    /// `com.longbridge.nsh.DevActivity` or `com.longbridge.nsh.AppActivity`.
    pub(crate) activity: &'a str,
    /// Development builds: debuggable, cleartext HTTP (the dev server).
    pub(crate) dev: bool,
    /// Plain HTTP is allowed (`android:usesCleartextTraffic`): development
    /// builds (the dev server's assets) and apps whose `nsh.json` sets
    /// `android.usesCleartextTraffic`.
    pub(crate) cleartext: bool,
    /// The app may show notifications: `POST_NOTIFICATIONS` is declared.
    pub(crate) notifications: bool,
    /// Web pages may use the microphone and the camera
    /// (`webview.media`): `RECORD_AUDIO` and `CAMERA` are declared.
    pub(crate) microphone: bool,
    pub(crate) camera: bool,
    /// The Java libraries of the app and its crates (`android.libraries`):
    /// jar, dex and AAR files, packaged after the runtime's
    /// ([`libraries::prepare`]).
    pub(crate) libraries: &'a [Library],
    /// The values of the placeholders (`${name}`) in the libraries'
    /// manifests and the crates' manifest entries: `nsh.json`
    /// `android.manifestPlaceholders`. `${applicationId}` is `package`.
    /// `None` in the shared test host, which reads no app's `nsh.json`: a
    /// placeholder is left empty there, with a warning.
    pub(crate) manifest_placeholders: Option<&'a BTreeMap<String, String>>,
    /// Where the dex `d8` makes of the jar libraries is kept between builds
    /// (`.nsh/android/libraries`). Unused without jars.
    pub(crate) library_cache: &'a Path,
    /// The permissions the app declares itself (nsh.json
    /// `android.permissions`), in full: declared after NSH's, each once.
    pub(crate) permissions: &'a [String],
    /// Elements the app's crates add to the manifest's `<queries>`
    /// ([`crate::module_meta`]): the other apps they start.
    pub(crate) queries: &'a [String],
    /// Elements the app's crates add to the manifest's `<application>`
    /// ([`crate::module_meta`]): services, receivers, providers, activities
    /// and meta-data. Their placeholders are filled in as the libraries'
    /// ([`Apk::manifest_placeholders`]).
    pub(crate) application: &'a [String],
    /// URL schemes that open the app (nsh.json `schemes`): one
    /// `VIEW` intent filter.
    pub(crate) schemes: &'a [String],
    /// Web links that open the app (nsh.json `links`): an intent filter
    /// Android verifies (App Links) for each, with its host and paths.
    pub(crate) links: &'a [nsh_core::url_scheme::WebLink],
    /// A Rust app: its `libnsh.so` has the root, which `AppActivity` runs
    /// instead of a bundle (manifest meta-data `com.longbridge.nsh.RUST`).
    pub(crate) rust: bool,
    /// `nsh.json` `"appearance": "dark"`: the window's theme is dark from
    /// the start (no light flash before the first frame).
    pub(crate) dark: bool,
    /// `nsh.json` `"appearance"` is `"light"` or `"dark"`: the launch screen
    /// shows that appearance's values, not the system's mode's.
    pub(crate) fixed_appearance: bool,
    /// `classes.dex` and `lib/<abi>/libnsh.so`.
    pub(crate) runtime: &'a Path,
    /// The ABIs whose `libnsh.so` goes in, each of which the runtime must
    /// have (empty: every one it has).
    pub(crate) abis: &'a [String],
    /// Launcher icon.
    pub(crate) icon: Option<Icon<'a>>,
    /// `nsh.json` `android.launchScreen`: the window's background and the
    /// splash screen's color and image ([`launch_resources`]).
    pub(crate) launch_screen: Option<&'a LaunchScreen>,
    /// Files under `assets/` in the APK: (path in the APK, contents).
    pub(crate) assets: Vec<(String, Vec<u8>)>,
    /// Notification sounds, raw resources: (resource name, file).
    pub(crate) sounds: Vec<(String, PathBuf)>,
    /// The app's own Android resources (`android/res/` next to `nsh.json`,
    /// laid out as Gradle's `src/main/res/`): drawables a tab bar names,
    /// for one. None when the app has none.
    pub(crate) res: Option<&'a Path>,
    /// The resource directories of the app's crates
    /// ([`crate::module_meta`]), each laid out as `res`: linked as overlays
    /// below the app's own, so the app's value of a resource wins.
    pub(crate) crate_res: &'a [PathBuf],
    /// The native library directories of the app's crates, each holding
    /// `<abi>/lib*.so`: packaged beside `libnsh.so` with the AARs' native
    /// libraries, under the same rules ([`libraries::native_for`]).
    pub(crate) jni_libs: &'a [NativeDir],
    pub(crate) signing: Signing,
}

/// A crate's native library directory (`android.jniLibs`), holding
/// `<abi>/lib*.so`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDir {
    /// The crate and where in its metadata it is listed, for errors.
    pub(crate) listed: String,
    pub(crate) dir: PathBuf,
}

/// What `nsh package --target android` writes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Format {
    /// An APK: installed directly (`adb install`, sideloading).
    #[default]
    Apk,
    /// An Android App Bundle: uploaded to Google Play, which builds the
    /// APKs each device installs.
    Aab,
}

impl Format {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Format::Apk => "apk",
            Format::Aab => "aab",
        }
    }
}

/// How to sign the APK.
pub(crate) enum Signing {
    /// `~/.android/debug.keystore`, created if missing.
    Debug,
    /// A release keystore.
    Keystore {
        path: PathBuf,
        alias: String,
        password: String,
    },
}

/// The launcher icon of an APK.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Icon<'a> {
    /// One square PNG, which the launcher shows as it is (inside its own
    /// shape, on a plate of its own when the PNG fills the square).
    Image(&'a Path),
    /// Layers that the launcher masks to its own shape.
    Adaptive(&'a AdaptiveIcon),
}

/// `nsh.json` `icon.android` as an object: an adaptive icon, layers of
/// 108×108dp. The middle 72dp is the masked viewport, which the launcher
/// cuts to its shape (circle, squircle, ...); the outer 18dp on each side
/// is room for its visual effects (moving the layers apart when touched).
/// Artwork stays within the central circle of 66dp.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct AdaptiveIcon {
    /// A square PNG with a transparent background: the artwork, within
    /// the central circle of 66dp.
    pub(crate) foreground: PathBuf,
    /// What the foreground is drawn on.
    pub(crate) background: IconBackground,
    /// A square PNG, one color on transparent: the shape the system tints
    /// for themed icons (Android 13 and later). Without it, themed
    /// launchers show the colored icon.
    pub(crate) monochrome: Option<PathBuf>,
    /// The keys of the object it does not know (`nsh.json` warns of them).
    #[serde(skip)]
    pub(crate) unknown: Vec<String>,
}

impl AdaptiveIcon {
    /// The same, with its PNGs relative to `dir`.
    pub(crate) fn relative_to(self, dir: &Path) -> Self {
        AdaptiveIcon {
            foreground: dir.join(self.foreground),
            background: match self.background {
                IconBackground::Image(path) => IconBackground::Image(dir.join(path)),
                color => color,
            },
            monochrome: self.monochrome.map(|path| dir.join(path)),
            unknown: self.unknown,
        }
    }
}

/// An adaptive icon's background: `"#rrggbb"` / `"#rrggbbaa"`, or a
/// square PNG.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) enum IconBackground {
    Color(Color),
    Image(PathBuf),
}

impl TryFrom<String> for IconBackground {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        if text.starts_with('#') {
            Color::try_from(text).map(IconBackground::Color)
        } else {
            Ok(IconBackground::Image(text.into()))
        }
    }
}

/// The densities launcher icons are drawn for, with the pixels of 1dp.
const DENSITIES: [(&str, f32); 5] = [
    ("mdpi", 1.0),
    ("hdpi", 1.5),
    ("xhdpi", 2.0),
    ("xxhdpi", 3.0),
    ("xxxhdpi", 4.0),
];

/// An adaptive icon's canvas, in dp.
const ADAPTIVE_CANVAS_DP: f32 = 108.0;

/// Writes the launcher icon's resources under `res` and returns their
/// files, to be compiled by `aapt2`. One PNG goes to
/// `mipmap-xxxhdpi/ic_launcher.png`; an adaptive icon is
/// `mipmap-anydpi-v26/ic_launcher.xml` (and `ic_launcher_round.xml`, the
/// same) naming its layers, each a PNG per density (a color background: a
/// color resource).
fn write_icon(icon: Icon<'_>, res: &Path) -> anyhow::Result<Vec<PathBuf>> {
    // aapt2 parses the file's path itself: the components joined in one
    // go (`join("res/mipmap-xxxhdpi")`) leave a mix of `/` and `\` on
    // Windows, which it calls no valid file path. Joined by component
    // the path comes out uniform.
    let mut files = Vec::new();
    let adaptive = match icon {
        Icon::Image(image) => {
            let dir = res.join("mipmap-xxxhdpi");
            fs::create_dir_all(&dir)?;
            let path = dir.join("ic_launcher.png");
            resize_png(image, &path, 192)?;
            files.push(path);
            return Ok(files);
        }
        Icon::Adaptive(adaptive) => adaptive,
    };
    let mut layers = vec![("ic_launcher_foreground", &adaptive.foreground)];
    let background = match &adaptive.background {
        IconBackground::Image(image) => {
            layers.push(("ic_launcher_background", image));
            "@mipmap/ic_launcher_background"
        }
        IconBackground::Color(Color([r, g, b, a])) => {
            let dir = res.join("values");
            fs::create_dir_all(&dir)?;
            let path = dir.join("ic_launcher_background.xml");
            fs::write(
                &path,
                format!(
                    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    \
                     <color name=\"ic_launcher_background\">#{a:02X}{r:02X}{g:02X}{b:02X}</color>\n\
                     </resources>\n"
                ),
            )?;
            files.push(path);
            "@color/ic_launcher_background"
        }
    };
    if let Some(monochrome) = &adaptive.monochrome {
        layers.push(("ic_launcher_monochrome", monochrome));
    }
    for (density, scale) in DENSITIES {
        let dir = res.join(format!("mipmap-{density}"));
        fs::create_dir_all(&dir)?;
        let size = (ADAPTIVE_CANVAS_DP * scale).round() as u32;
        for (name, source) in &layers {
            let path = dir.join(format!("{name}.png"));
            resize_png(source, &path, size)?;
            files.push(path);
        }
    }
    let monochrome = if adaptive.monochrome.is_some() {
        "\n    <monochrome android:drawable=\"@mipmap/ic_launcher_monochrome\" />"
    } else {
        ""
    };
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <adaptive-icon xmlns:android=\"http://schemas.android.com/apk/res/android\">\n    \
         <background android:drawable=\"{background}\" />\n    \
         <foreground android:drawable=\"@mipmap/ic_launcher_foreground\" />{monochrome}\n\
         </adaptive-icon>\n"
    );
    let dir = res.join("mipmap-anydpi-v26");
    fs::create_dir_all(&dir)?;
    for name in ["ic_launcher", "ic_launcher_round"] {
        let path = dir.join(format!("{name}.xml"));
        fs::write(&path, &xml)?;
        files.push(path);
    }
    Ok(files)
}

pub(crate) fn build_apk(sdk: &Sdk, apk: &Apk<'_>, out: &Path) -> anyhow::Result<()> {
    build(sdk, apk, Format::Apk, out)
}

/// Writes `apk` to `out` as an APK or an Android App Bundle. A bundle's
/// manifest and resources are linked in the protobuf format and its files
/// laid out as Google Play reads them (module `base`): the same as Gradle's
/// `bundleRelease`, without Gradle or bundletool.
pub(crate) fn build(sdk: &Sdk, apk: &Apk<'_>, format: Format, out: &Path) -> anyhow::Result<()> {
    let work = out.with_extension(format!("{}-work", format.extension()));
    if work.exists() {
        fs::remove_dir_all(&work)?;
    }
    fs::create_dir_all(&work)?;

    // The libraries first: a missing file stops the build early.
    let mut java = libraries::prepare(sdk, apk, &work)?;
    // The crates' native libraries join the AARs'.
    java.native.extend(crate_native(
        apk.jni_libs,
        apk.target_sdk >= 35 && !apk.dev,
    )?);

    // Resources: the launcher icon, and the launch screen's theme below.
    let mut flat = Vec::new();
    if let Some(icon) = apk.icon {
        let compiled = work.join("compiled");
        fs::create_dir_all(&compiled)?;
        for file in write_icon(icon, &work.join("res"))? {
            run(Command::new(sdk.tool("aapt2"))
                .arg("compile")
                .arg(&file)
                .arg("-o")
                .arg(&compiled))?;
        }
        for entry in fs::read_dir(&compiled)? {
            flat.push(entry?.path());
        }
    }

    // The launch screen: the theme the manifest names and what it uses.
    if let Some(screen) = apk.launch_screen {
        let res = work.join("res");
        let mut files = Vec::new();
        for file in launch_resources(screen, apk.dark, apk.fixed_appearance, apk.target_sdk)? {
            let dir = res.join(file.dir);
            fs::create_dir_all(&dir)?;
            let path = dir.join(&file.name);
            match file.contents {
                ResContents::Text(text) => fs::write(&path, text)?,
                ResContents::Copy(source) => {
                    fs::copy(&source, &path).map(drop).with_context(|| {
                        format!(
                            "copy {} (nsh.json android.launchScreen.image)",
                            source.display()
                        )
                    })?
                }
            }
            files.push(path);
        }
        flat.extend(compile_resources(
            sdk,
            &files,
            &work.join("compiled-launch"),
        )?);
    }

    // Notification sounds: raw resources, which a channel's sound names.
    if !apk.sounds.is_empty() {
        let raw = work.join("res").join("raw");
        fs::create_dir_all(&raw)?;
        let compiled = work.join("compiled-raw");
        fs::create_dir_all(&compiled)?;
        for (name, source) in &apk.sounds {
            let extension = source.extension().and_then(|e| e.to_str()).unwrap_or("ogg");
            let file = raw.join(format!("{name}.{}", extension.to_ascii_lowercase()));
            fs::copy(source, &file).with_context(|| format!("copy {}", source.display()))?;
            run(Command::new(sdk.tool("aapt2"))
                .arg("compile")
                .arg(&file)
                .arg("-o")
                .arg(&compiled))?;
        }
        for entry in fs::read_dir(&compiled)? {
            flat.push(entry?.path());
        }
    }

    // The app's own resources, compiled as a whole: `aapt2 compile --dir`
    // reads `<type>[-<qualifiers>]/<name>` as Gradle does.
    let mut app_res = None;
    if let Some(res) = apk.res {
        let zip = work.join("compiled-app.zip");
        run(Command::new(sdk.tool("aapt2"))
            .arg("compile")
            .arg("--dir")
            .arg(res)
            .arg("-o")
            .arg(&zip))
        .with_context(|| format!("compile the app's resources in {}", res.display()))?;
        app_res = Some(zip);
    }
    // The crates' resources, each compiled the same way.
    let mut crate_res = Vec::new();
    for (i, res) in apk.crate_res.iter().enumerate() {
        let zip = work.join(format!("compiled-crate-{i}.zip"));
        run(Command::new(sdk.tool("aapt2"))
            .arg("compile")
            .arg("--dir")
            .arg(res)
            .arg("-o")
            .arg(&zip))
        .with_context(|| format!("compile a crate's resources in {}", res.display()))?;
        crate_res.push(zip);
    }
    let overlays = resource_layers(&mut flat, app_res, crate_res);

    let manifest = work.join("AndroidManifest.xml");
    fs::write(&manifest, manifest_xml(apk, &java.manifest))?;
    let base = work.join("base.apk");
    let mut link = Command::new(sdk.tool("aapt2"));
    link.arg("link")
        .arg("-o")
        .arg(&base)
        .arg("-I")
        .arg(sdk.android_jar(apk.target_sdk)?)
        .arg("--manifest")
        .arg(&manifest)
        .args(["--min-sdk-version", &MIN_SDK.to_string()])
        .args(["--target-sdk-version", &apk.target_sdk.to_string()])
        .args(["--version-code", &apk.version_code.to_string()])
        .args(["--version-name", apk.version_name]);
    if apk.dev {
        link.arg("--debug-mode");
    }
    if format == Format::Aab {
        link.arg("--proto-format");
    }
    link.args(&flat);
    if !overlays.is_empty() {
        link.arg("--auto-add-overlay");
        for overlay in &overlays {
            link.arg("-R").arg(overlay);
        }
    }
    run(&mut link)?;

    if format == Format::Aab {
        let unsigned = work.join("unsigned.aab");
        write_bundle(&base, &unsigned, apk, &java)?;
        sign_bundle(&apk.signing, &unsigned)?;
        fs::rename(&unsigned, out)?;
        fs::remove_dir_all(&work)?;
        return Ok(());
    }

    // Code, native libraries and app files.
    let unaligned = work.join("unaligned.apk");
    add_files(&base, &unaligned, apk, &java, &Layout::APK)?;
    let aligned = work.join("aligned.apk");
    // `-P 16`: shared libraries on 16 KB pages (Android 15 devices).
    run(Command::new(sdk.tool("zipalign"))
        .args(["-P", "16", "-f", "4"])
        .arg(&unaligned)
        .arg(&aligned))?;
    sign(sdk, &apk.signing, &aligned)?;
    fs::rename(&aligned, out)?;
    fs::remove_dir_all(&work)?;
    Ok(())
}

/// Where the app's and its crates' compiled resources go in `aapt2 link`:
/// the app's always with NSH's own (`flat`), so a name both declare stays
/// an error, as it was before crates had resources; and, when crates have
/// some, every crate's then the app's again as overlays (returned, in
/// order), so the app's value of a name a crate declares wins.
/// `--auto-add-overlay` adds the names the base does not declare.
fn resource_layers(
    flat: &mut Vec<PathBuf>,
    app: Option<PathBuf>,
    crates: Vec<PathBuf>,
) -> Vec<PathBuf> {
    flat.extend(app.clone());
    if crates.is_empty() {
        return Vec::new();
    }
    let mut overlays = crates;
    overlays.extend(app);
    overlays
}

/// The theme of the application: the launch theme ([`LAUNCH_THEME`]) when
/// the app has a launch screen, the platform's Material theme otherwise.
fn theme(apk: &Apk<'_>) -> String {
    if apk.launch_screen.is_some() {
        format!("@style/{LAUNCH_THEME}")
    } else {
        material_theme(apk.dark).to_owned()
    }
}

/// The platform theme apps start in: dark with `"appearance": "dark"`.
fn material_theme(dark: bool) -> &'static str {
    if dark {
        "@android:style/Theme.Material.NoActionBar"
    } else {
        "@android:style/Theme.Material.Light.NoActionBar"
    }
}

/// The theme a launch screen generates, which the manifest names.
const LAUNCH_THEME: &str = "NshLaunch";
/// The splash screen's icon area, in dp. The image's insets are fractions
/// of it, so they hold whatever size the system draws the icon at.
const SPLASH_ICON_DP: f32 = 288.0;

/// A resource file a package generates: `res/<dir>/<name>`.
struct ResFile {
    dir: &'static str,
    name: String,
    contents: ResContents,
}

enum ResContents {
    /// An XML file written as it is.
    Text(String),
    /// A file copied from the app (an image).
    Copy(PathBuf),
}

/// The resources of a launch screen: a theme ([`LAUNCH_THEME`], on the
/// platform's Material theme) whose window background is the screen's
/// color, so the window looks like the app's first frame from the tap on
/// the icon until that frame and behind it; and on Android 12+, the system
/// splash screen in the same color with the screen's image in the middle
/// (drawn for 3x screens as on iOS: a 288 px PNG is 96 dp). Without an
/// image the splash screen shows the launcher icon. The image must be a
/// PNG, checked even when the app targets a platform before Android 12,
/// which has no splash screen to show it. The dark values go
/// to the `-night` qualifiers, which the system picks in dark mode; an app
/// of one appearance (`fixed`, dark or not) has only that appearance's.
fn launch_resources(
    screen: &LaunchScreen,
    dark: bool,
    fixed: bool,
    target_sdk: u32,
) -> anyhow::Result<Vec<ResFile>> {
    // The default value and the night one.
    fn pick<T>(themed: &Themed<T>, dark: bool, fixed: bool) -> (&T, Option<&T>) {
        match themed.variants() {
            (light, Some(night)) if fixed => (if dark { night } else { light }, None),
            variants => variants,
        }
    }
    let xml = |body: String| {
        ResContents::Text(format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n{body}</resources>\n"
        ))
    };
    let style = |items: &[(&str, &str)]| {
        let items: String = items
            .iter()
            .map(|(name, value)| format!("        <item name=\"android:{name}\">{value}</item>\n"))
            .collect();
        format!(
            "    <style name=\"{LAUNCH_THEME}\" parent=\"{}\">\n{items}    </style>\n",
            material_theme(dark)
        )
    };
    let color = |color: &Color| {
        let [r, g, b, a] = color.0;
        format!("    <color name=\"nsh_launch_background\">#{a:02x}{r:02x}{g:02x}{b:02x}</color>\n")
    };
    let mut files = Vec::new();
    let background = screen
        .background
        .as_ref()
        .map(|background| pick(background, dark, fixed));
    let window = [("windowBackground", "@color/nsh_launch_background")];
    let window: &[(&str, &str)] = if background.is_some() { &window } else { &[] };
    files.push(ResFile {
        dir: "values",
        name: "nsh_launch.xml".into(),
        contents: xml(format!(
            "{}{}",
            background
                .map(|(light, _)| color(light))
                .unwrap_or_default(),
            style(window)
        )),
    });
    if let Some((_, Some(dark))) = background {
        files.push(ResFile {
            dir: "values-night",
            name: "nsh_launch.xml".into(),
            contents: xml(color(dark)),
        });
    }
    // The image for each appearance (the night one only when it differs)
    // and its size in pixels.
    let mut images = Vec::new();
    if let Some(image) = &screen.image {
        let (day, night) = pick(image, dark, fixed);
        for (path, png_dir, xml_dir) in [
            (Some(day), "drawable-nodpi", "drawable"),
            (night, "drawable-night-nodpi", "drawable-night"),
        ] {
            let Some(path) = path else { continue };
            images.push((path, png_size(path)?, png_dir, xml_dir));
        }
    }
    // The splash screen's attributes are Android 12's (API 31): an app that
    // compiles against an older platform keeps its window background only.
    if target_sdk < 31 {
        return Ok(files);
    }
    let mut splash = window.to_vec();
    if background.is_some() {
        splash.push((
            "windowSplashScreenBackground",
            "@color/nsh_launch_background",
        ));
    }
    if !images.is_empty() {
        splash.push(("windowSplashScreenAnimatedIcon", "@drawable/nsh_launch"));
    }
    for (path, (width, height), png_dir, xml_dir) in images {
        // Centered at a third of its pixels in dp: each side insets the
        // rest of the icon area, as a fraction of it.
        let inset = |px: u32| {
            let fraction = ((1.0 - px as f32 / 3.0 / SPLASH_ICON_DP) / 2.0).max(0.0);
            format!("{:.3}%", fraction * 100.0)
        };
        files.push(ResFile {
            dir: png_dir,
            name: "nsh_launch_image.png".into(),
            contents: ResContents::Copy(path.clone()),
        });
        files.push(ResFile {
            dir: xml_dir,
            name: "nsh_launch.xml".into(),
            contents: ResContents::Text(format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<inset xmlns:android="http://schemas.android.com/apk/res/android"
    android:drawable="@drawable/nsh_launch_image"
    android:insetLeft="{x}" android:insetRight="{x}"
    android:insetTop="{y}" android:insetBottom="{y}" />
"#,
                x = inset(width),
                y = inset(height),
            )),
        });
    }
    files.push(ResFile {
        dir: "values-v31",
        name: "nsh_launch.xml".into(),
        contents: xml(style(&splash)),
    });
    Ok(files)
}

/// The size of the launch screen's image, which must be a PNG (it is copied
/// into the package as one).
fn png_size(path: &Path) -> anyhow::Result<(u32, u32)> {
    let read = || {
        format!(
            "read {} (nsh.json android.launchScreen.image)",
            path.display()
        )
    };
    let reader = image::ImageReader::open(path)
        .with_context(read)?
        .with_guessed_format()
        .with_context(read)?;
    if reader.format() != Some(image::ImageFormat::Png) {
        bail!(
            "{} is no PNG (nsh.json android.launchScreen.image)",
            path.display()
        );
    }
    reader.into_dimensions().with_context(read)
}

/// Compiles resource files (each under `res/<type>/`) with `aapt2` into
/// `out`, returning what it wrote for `aapt2 link`.
fn compile_resources(sdk: &Sdk, files: &[PathBuf], out: &Path) -> anyhow::Result<Vec<PathBuf>> {
    fs::create_dir_all(out)?;
    for file in files {
        run(Command::new(sdk.tool("aapt2"))
            .arg("compile")
            .arg(file)
            .arg("-o")
            .arg(out))?;
    }
    let mut flat = Vec::new();
    for entry in fs::read_dir(out)? {
        flat.push(entry?.path());
    }
    Ok(flat)
}

/// `value` as the text of a manifest attribute (in double quotes).
fn xml_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        // An attribute in single quotes (a crate's entry) holds it too.
        .replace('\'', "&apos;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        // Kept: a parser turns a raw line break or tab into a space.
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
        .replace('\t', "&#9;")
}

/// The permissions NSH declares: its own, then the app's
/// (`android.permissions`), each once.
fn declared_permissions(apk: &Apk<'_>) -> Vec<String> {
    let mut permissions = vec!["android.permission.INTERNET".to_owned()];
    for (declared, name) in [
        (apk.notifications, "POST_NOTIFICATIONS"),
        (apk.microphone, "RECORD_AUDIO"),
        (apk.camera, "CAMERA"),
    ] {
        if declared {
            permissions.push(format!("android.permission.{name}"));
        }
    }
    for name in apk.permissions {
        if !permissions.contains(name) {
            permissions.push(name.clone());
        }
    }
    permissions
}

/// The manifest of `apk`, with what its crates and libraries add
/// (`additions`, [`library_manifest::additions`]).
fn manifest_xml(apk: &Apk<'_>, additions: &library_manifest::Additions) -> String {
    let icon = match apk.icon {
        Some(Icon::Image(_)) => r#" android:icon="@mipmap/ic_launcher""#,
        Some(Icon::Adaptive(_)) => {
            r#" android:icon="@mipmap/ic_launcher" android:roundIcon="@mipmap/ic_launcher_round""#
        }
        None => "",
    };
    let cleartext = if apk.cleartext {
        r#" android:usesCleartextTraffic="true""#
    } else {
        ""
    };
    let rust = if apk.rust {
        "\n        <meta-data android:name=\"com.longbridge.nsh.RUST\" android:value=\"true\" />"
    } else {
        ""
    };
    let label = apk
        .label
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;");
    // NSH's own, then the app's (`android.permissions`), then the
    // libraries'.
    let permissions: String = declared_permissions(apk)
        .iter()
        .map(|name| format!("\n    <uses-permission android:name=\"{name}\" />"))
        .collect();
    let permissions = format!("{permissions}{}", additions.top);
    // A link opens the running app (`onNewIntent`), not another activity in
    // the caller's task.
    let launch_mode = if apk.schemes.is_empty() && apk.links.is_empty() {
        ""
    } else {
        r#" android:launchMode="singleTask""#
    };
    let schemes = if apk.schemes.is_empty() {
        String::new()
    } else {
        let data: String = apk
            .schemes
            .iter()
            .map(|s| format!("\n                <data android:scheme=\"{s}\" />"))
            .collect();
        format!(
            "\n            <intent-filter>\n                <action android:name=\"android.intent.action.VIEW\" />\n                <category android:name=\"android.intent.category.DEFAULT\" />\n                <category android:name=\"android.intent.category.BROWSABLE\" />{data}\n            </intent-filter>"
        )
    };
    // App Links: Android checks each host's `/.well-known/assetlinks.json`
    // as the app installs, and opens their verified links in the app. A
    // filter's `data` combine (each host with each path), so each link has
    // its own: one host's paths are not another's.
    let links: String = apk
        .links
        .iter()
        .map(|link| {
            let paths: String = link
                .paths()
                .iter()
                .map(|path| {
                    let (attribute, value) = match path.strip_suffix('*') {
                        Some(start) => ("pathPrefix", start),
                        None => ("path", path.as_str()),
                    };
                    format!(
                        "\n                <data android:{attribute}=\"{}\" />",
                        xml_attribute(value)
                    )
                })
                .collect();
            format!(
                "\n            <intent-filter android:autoVerify=\"true\">\n                <action android:name=\"android.intent.action.VIEW\" />\n                <category android:name=\"android.intent.category.DEFAULT\" />\n                <category android:name=\"android.intent.category.BROWSABLE\" />\n                <data android:scheme=\"https\" />\n                <data android:host=\"{}\" />{paths}\n            </intent-filter>",
                xml_attribute(link.host())
            )
        })
        .collect();
    let queries = &additions.queries;
    let components = &additions.application;
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="{package}">{permissions}
    <!-- Text actions (Translate and the like) the selection menu offers,
        as TextView's: Android 11+ hides other apps' activities otherwise. -->
    <queries>
        <intent>
            <action android:name="android.intent.action.PROCESS_TEXT" />
            <data android:mimeType="text/plain" />
        </intent>{queries}
    </queries>
    <application android:label="{label}"{icon}{cleartext}
        android:extractNativeLibs="false"
        android:enableOnBackInvokedCallback="true"
        android:supportsRtl="true"
        android:theme="{theme}">{rust}
        <!-- Serves the app's own files to the apps that open them
            (`SystemUi.openPath`): the authority matches what
            `FilesProvider.uriFor` builds. -->
        <provider android:name="com.longbridge.nsh.FilesProvider"
            android:authorities="{package}.files"
            android:exported="false"
            android:grantUriPermissions="true" />
        <activity android:name="{activity}" android:exported="true"{launch_mode}
            android:windowSoftInputMode="adjustResize"
            android:configChanges="orientation|screenSize|screenLayout|smallestScreenSize|keyboard|keyboardHidden|navigation|uiMode|density|fontScale|locale|layoutDirection">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>{schemes}{links}
        </activity>{components}
    </application>
</manifest>
"#,
        package = apk.package,
        activity = apk.activity,
        theme = theme(apk),
    )
}

/// Where an APK or a bundle keeps what [`add_files`] puts in.
struct Layout {
    /// Prefix of what `aapt2 link` wrote; a bundle's manifest goes to
    /// `manifest/` under it.
    linked: &'static str,
    dex: &'static str,
    lib: &'static str,
    assets: &'static str,
    /// Prefix of the files at the installed APK's root: the Java resources
    /// of the libraries.
    root: &'static str,
}

impl Layout {
    const APK: Layout = Layout {
        linked: "",
        dex: "classes.dex",
        lib: "lib",
        assets: "assets",
        root: "",
    };
    /// The bundle's one module, `base`.
    const BUNDLE: Layout = Layout {
        linked: "base/",
        dex: "base/dex/classes.dex",
        lib: "base/lib",
        assets: "base/assets",
        root: "base/root/",
    };
}

/// An Android App Bundle: the module `base` from the proto-format `linked`,
/// [`add_files`]' code, libraries and assets, the libraries' ABIs
/// (`base/native.pb`) and `BundleConfig.pb`.
fn write_bundle(
    linked: &Path,
    out: &Path,
    apk: &Apk<'_>,
    java: &libraries::Contents,
) -> anyhow::Result<()> {
    let files = work_file(out, "files.zip");
    let abis = add_files(linked, &files, apk, java, &Layout::BUNDLE)?;
    use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};
    let mut input = ZipArchive::new(fs::File::open(&files)?)?;
    let mut zip = ZipWriter::new(fs::File::create(out)?);
    for i in 0..input.len() {
        zip.raw_copy_file(input.by_index_raw(i)?)?;
    }
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("base/native.pb", deflate)?;
    zip.write_all(&native_pb(&abis)?)?;
    zip.start_file("BundleConfig.pb", deflate)?;
    let stored: Vec<String> = java
        .assets
        .iter()
        .filter(|(path, _)| libraries::stored_asset(path))
        .map(|(path, _)| format!("assets/{path}"))
        .collect();
    zip.write_all(&bundle_config_pb(&stored))?;
    zip.finish()?;
    fs::remove_file(&files)?;
    Ok(())
}

fn work_file(next_to: &Path, name: &str) -> PathBuf {
    next_to.with_file_name(name)
}

/// Protobuf wire format, as much as the bundle's two files need.
mod pb {
    pub(super) fn varint(out: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            out.push(value as u8 | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    /// A length-delimited field: a string or a message.
    pub(super) fn bytes(out: &mut Vec<u8>, field: u32, value: &[u8]) {
        varint(out, (u64::from(field) << 3) | 2);
        varint(out, value.len() as u64);
        out.extend_from_slice(value);
    }

    /// A varint field: an enum.
    pub(super) fn enumeration(out: &mut Vec<u8>, field: u32, value: u64) {
        varint(out, u64::from(field) << 3);
        varint(out, value);
    }
}

/// The bundletool version the bundle says built it, which picks the
/// defaults Google Play builds the APKs with (native libraries stay
/// uncompressed, as `extractNativeLibs="false"` needs).
const BUNDLETOOL_VERSION: &str = "1.18.1";

/// `BundleConfig { bundletool { version } compression { uncompressed_glob } }`
/// (bundletool's `config.proto`). Google Play compresses the APKs' assets
/// as it decides, not as the bundle stores them: the module's `uncompressed`
/// paths (`assets/...`) are named here to stay stored.
fn bundle_config_pb(uncompressed: &[String]) -> Vec<u8> {
    let mut bundletool = Vec::new();
    pb::bytes(&mut bundletool, 2, BUNDLETOOL_VERSION.as_bytes());
    let mut config = Vec::new();
    pb::bytes(&mut config, 1, &bundletool);
    if !uncompressed.is_empty() {
        let mut compression = Vec::new();
        for path in uncompressed {
            // A glob of the path itself: Java's glob syntax.
            let mut glob = String::with_capacity(path.len());
            for c in path.chars() {
                if matches!(c, '\\' | '*' | '?' | '[' | ']' | '{' | '}') {
                    glob.push('\\');
                }
                glob.push(c);
            }
            pb::bytes(&mut compression, 1, glob.as_bytes());
        }
        pb::bytes(&mut config, 3, &compression);
    }
    config
}

/// `NativeLibraries`: one `lib/<abi>` directory per ABI with its targeting
/// (bundletool's `files.proto`), from which Google Play makes one split per
/// ABI.
fn native_pb(abis: &[String]) -> anyhow::Result<Vec<u8>> {
    let mut libraries = Vec::new();
    for abi in abis {
        // `targeting.proto`'s `Abi.AbiAlias`.
        let alias = match abi.as_str() {
            "armeabi-v7a" => 2,
            "arm64-v8a" => 3,
            "x86" => 4,
            "x86_64" => 5,
            other => bail!("no bundle targeting for ABI {other}"),
        };
        let mut abi_message = Vec::new();
        pb::enumeration(&mut abi_message, 1, alias);
        let mut targeting = Vec::new();
        pb::bytes(&mut targeting, 1, &abi_message);
        let mut directory = Vec::new();
        pb::bytes(&mut directory, 1, format!("lib/{abi}").as_bytes());
        pb::bytes(&mut directory, 2, &targeting);
        pb::bytes(&mut libraries, 1, &directory);
    }
    Ok(libraries)
}

/// Signs a bundle with `jarsigner` (JAR signing: what Google Play checks
/// an upload against; `apksigner` signs only APKs).
fn sign_bundle(signing: &Signing, bundle: &Path) -> anyhow::Result<()> {
    let (keystore, alias, password) = keystore(signing)?;
    run(Command::new(java_tool("jarsigner"))
        .args(["-sigalg", "SHA256withRSA", "-digestalg", "SHA-256"])
        .arg("-keystore")
        .arg(&keystore)
        .args(["-storepass", &password])
        .args(["-keypass", &password])
        .arg(bundle)
        .arg(&alias))
    .context("sign the bundle (is a JDK installed?)")
}

/// A JDK tool: in `JAVA_HOME`, else on the path.
fn java_tool(name: &str) -> PathBuf {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin").join(&exe))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from(exe))
}

/// Copies `base` (manifest, resources) and adds the dex (the runtime's,
/// then the libraries' `java`), the native libraries (the runtime's and the
/// libraries', stored: the system maps them in place), the assets (the
/// app's and the libraries') and the libraries' Java resources, where
/// `layout` keeps them; returns the ABIs of the native libraries.
fn add_files(
    base: &Path,
    out: &Path,
    apk: &Apk<'_>,
    java: &libraries::Contents,
    layout: &Layout,
) -> anyhow::Result<Vec<String>> {
    use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};
    let mut input = ZipArchive::new(fs::File::open(base)?)?;
    let mut zip = ZipWriter::new(fs::File::create(out)?);
    for i in 0..input.len() {
        let file = input.by_index_raw(i)?;
        let name = file.name()?.into_owned();
        if layout.linked.is_empty() {
            zip.raw_copy_file(file)?;
        } else if name == "AndroidManifest.xml" {
            zip.raw_copy_file_rename(file, format!("{}manifest/{name}", layout.linked))?;
        } else {
            zip.raw_copy_file_rename(file, format!("{}{name}", layout.linked))?;
        }
    }
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(layout.dex, deflate)?;
    let mut dex = Vec::new();
    fs::File::open(apk.runtime.join("classes.dex"))
        .context("runtime classes.dex missing")?
        .read_to_end(&mut dex)?;
    zip.write_all(&dex)?;
    // The libraries' after the runtime's: Android loads `classes<N>.dex`
    // from 2 up, until one is missing. The numbers assume the runtime is
    // one dex; a runtime of more would collide with the libraries'.
    if !java.dex.is_empty() && apk.runtime.join("classes2.dex").exists() {
        bail!(
            "the Android runtime in {} has more than one dex file, which `android.libraries` cannot follow yet",
            apk.runtime.display()
        );
    }
    let dex_dir = layout
        .dex
        .strip_suffix("classes.dex")
        .context("dex layout")?;
    for (i, path) in java.dex.iter().enumerate() {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        zip.start_file(format!("{dex_dir}classes{}.dex", i + 2), deflate)?;
        zip.write_all(&bytes)?;
    }
    let libs = apk.runtime.join("lib");
    let mut available = Vec::new();
    for entry in fs::read_dir(&libs).with_context(|| format!("read {}", libs.display()))? {
        let abi_dir = entry?.path();
        if abi_dir.join("libnsh.so").exists() {
            available.push(
                abi_dir
                    .file_name()
                    .context("abi")?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    available.sort();
    if available.is_empty() {
        bail!("no libnsh.so in {}", libs.display());
    }
    let abis = if apk.abis.is_empty() {
        &available[..]
    } else {
        apk.abis
    };
    for abi in abis {
        if !available.contains(abi) {
            bail!(
                "the Android runtime has no libnsh.so for {abi} (it has {})",
                available.join(", ")
            );
        }
        let so = libs.join(abi).join("libnsh.so");
        zip.start_file(format!("{}/{abi}/libnsh.so", layout.lib), stored)?;
        zip.write_all(&fs::read(&so)?)?;
        for (name, bytes) in libraries::native_for(&java.native, abi)? {
            zip.start_file(format!("{}/{abi}/{name}", layout.lib), stored)?;
            zip.write_all(bytes)?;
        }
    }
    for (path, bytes) in &apk.assets {
        // Bytecode and images are compressed already or small.
        zip.start_file(format!("{}/{path}", layout.assets), deflate)?;
        zip.write_all(bytes)?;
    }
    for (path, bytes) in &java.assets {
        let options = if libraries::stored_asset(path) {
            stored
        } else {
            deflate
        };
        zip.start_file(format!("{}/{path}", layout.assets), options)?;
        zip.write_all(bytes)?;
    }
    for (path, bytes) in &java.resources {
        zip.start_file(format!("{}{path}", layout.root), deflate)?;
        zip.write_all(bytes)?;
    }
    zip.finish()?;
    Ok(abis.to_vec())
}

/// The native libraries of the crates' `jniLibs` directories, as the
/// AARs' are: `lib*.so` per ABI folder, each checked to be a shared
/// library of its ABI ([`check_library`]; `page_size_required`: a 64-bit
/// one not aligned to 16 KB is an error rather than a warning).
fn crate_native(
    dirs: &[NativeDir],
    page_size_required: bool,
) -> anyhow::Result<Vec<libraries::Native>> {
    let mut native = Vec::new();
    for NativeDir { listed, dir } in dirs {
        let mut abis = std::collections::BTreeMap::new();
        for abi_dir in sorted_entries(dir)? {
            if !abi_dir.is_dir() {
                continue;
            }
            let abi = abi_dir
                .file_name()
                .context("abi")?
                .to_string_lossy()
                .into_owned();
            let mut files = Vec::new();
            for file in sorted_entries(&abi_dir)? {
                let name = file
                    .file_name()
                    .context("library name")?
                    .to_string_lossy()
                    .into_owned();
                // What the system loads: `lib<name>.so` files.
                if !file.is_file() || !name.starts_with("lib") || !name.ends_with(".so") {
                    continue;
                }
                let bytes = fs::read(&file).with_context(|| format!("read {}", file.display()))?;
                check_library(&bytes, &abi, page_size_required)
                    .with_context(|| format!("{listed}: {}", file.display()))?;
                files.push((name, bytes));
            }
            if !files.is_empty() {
                abis.insert(abi, files);
            }
        }
        if !abis.is_empty() {
            native.push(libraries::Native {
                from: listed.clone(),
                abis,
            });
        }
    }
    Ok(native)
}

/// The entries of `dir`, sorted.
fn sorted_entries(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    entries.sort();
    Ok(entries)
}

/// Checks a shared library `elf` is one for `abi`: an ELF shared object of
/// its class and machine. A 64-bit one must load on devices with 16 KB
/// pages (Android 15): every loadable segment aligned to 16 KB at least,
/// which Google Play requires of a release from target SDK 35
/// (`required`; otherwise, a warning). 32-bit ABIs run on 4 KB pages.
fn check_library(elf: &[u8], abi: &str, required: bool) -> anyhow::Result<()> {
    let (class, machine) = match abi {
        "arm64-v8a" => (2, 183),
        "x86_64" => (2, 62),
        "armeabi-v7a" => (1, 40),
        "x86" => (1, 3),
        other => bail!("no ABI {other}"),
    };
    let u16_at = |at: usize| -> Option<u16> {
        Some(u16::from_le_bytes(
            elf.get(at..at.checked_add(2)?)?.try_into().ok()?,
        ))
    };
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            elf.get(at..at.checked_add(4)?)?.try_into().ok()?,
        ))
    };
    let u64_at = |at: usize| -> Option<u64> {
        Some(u64::from_le_bytes(
            elf.get(at..at.checked_add(8)?)?.try_into().ok()?,
        ))
    };
    // Little-endian, as every Android ABI is.
    if elf.get(..4) != Some(b"\x7fELF".as_slice()) || elf.get(5) != Some(&1) {
        bail!("not a little-endian ELF shared library");
    }
    if elf.get(4) != Some(&class) || u16_at(0x12) != Some(machine) {
        bail!("not a library for {abi} (its ELF class or machine is another's)");
    }
    // ET_DYN.
    if u16_at(0x10) != Some(3) {
        bail!("not a shared library (its ELF type is another)");
    }
    if class == 1 {
        return Ok(());
    }
    let (Some(phoff), Some(phentsize), Some(phnum)) = (u64_at(0x20), u16_at(0x36), u16_at(0x38))
    else {
        bail!("a truncated ELF header");
    };
    if phentsize != 0x38 {
        bail!("an ELF header with program headers of {phentsize} bytes, not 56");
    }
    for i in 0..usize::from(phnum) {
        let at = usize::try_from(phoff)
            .ok()
            .and_then(|phoff| phoff.checked_add(i.checked_mul(0x38)?));
        let header = at.and_then(|at| Some((u32_at(at)?, u64_at(at.checked_add(0x30)?)?)));
        let Some((kind, align)) = header else {
            bail!("a truncated ELF program header");
        };
        // PT_LOAD.
        if kind == 1 && align < 0x4000 {
            let problem = format!(
                "a loadable segment is aligned to {align:#x}, below the 16 KB that {abi} devices \
                 with 16 KB pages need: link it with `-Wl,-z,max-page-size=16384`"
            );
            if required {
                bail!("{problem} (Google Play requires it from target SDK 35)");
            }
            crate::ui::warn(problem);
            return Ok(());
        }
    }
    Ok(())
}

fn sign(sdk: &Sdk, signing: &Signing, apk: &Path) -> anyhow::Result<()> {
    let (keystore, alias, password) = keystore(signing)?;
    run(Command::new(sdk.tool("apksigner"))
        .args(["sign", "--ks"])
        .arg(&keystore)
        .args(["--ks-key-alias", &alias])
        .args(["--ks-pass", &format!("pass:{password}")])
        .args(["--key-pass", &format!("pass:{password}")])
        .arg(apk))
}

/// The keystore, key alias and password of `signing`.
fn keystore(signing: &Signing) -> anyhow::Result<(PathBuf, String, String)> {
    Ok(match signing {
        Signing::Debug => (
            debug_keystore()?,
            "androiddebugkey".to_owned(),
            "android".to_owned(),
        ),
        Signing::Keystore {
            path,
            alias,
            password,
        } => (path.clone(), alias.clone(), password.clone()),
    })
}

/// The standard debug keystore, created like Android Studio does.
fn debug_keystore() -> anyhow::Result<PathBuf> {
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let dir = PathBuf::from(env::var_os(home).context(home)?).join(".android");
    let path = dir.join("debug.keystore");
    if !path.exists() {
        fs::create_dir_all(&dir)?;
        run(Command::new("keytool")
            .args(["-genkeypair", "-keystore"])
            .arg(&path)
            .args([
                "-storepass",
                "android",
                "-alias",
                "androiddebugkey",
                "-keypass",
                "android",
                "-keyalg",
                "RSA",
                "-keysize",
                "2048",
                "-validity",
                "10000",
                "-dname",
                "CN=Android Debug,O=Android,C=US",
            ]))
        .context("create the debug keystore (is a JDK installed?)")?;
    }
    Ok(path)
}

fn resize_png(src: &Path, dest: &Path, size: u32) -> anyhow::Result<()> {
    let image = image::open(src).with_context(|| format!("read {}", src.display()))?;
    image
        .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
        .save(dest)?;
    Ok(())
}

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    let output = cmd
        .output()
        .with_context(|| format!("run {:?}", cmd.get_program()))?;
    if !output.status.success() {
        bail!(
            "{} failed:\n{}{}",
            Path::new(cmd.get_program()).display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// The environment variable adb itself reads for the device to talk to.
const SERIAL_ENV: &str = "ANDROID_SERIAL";

/// The serial of the device or emulator to use: the one `wanted` names
/// (`--device`), else the one `ANDROID_SERIAL` names, else the first one
/// `adb devices` lists as ready, with a warning when it lists several. A
/// device named but not ready is an error: another one would get the app.
pub(crate) fn device(sdk: &Sdk, wanted: Option<&str>) -> anyhow::Result<String> {
    let from_env = env::var(SERIAL_ENV).ok();
    let wanted = match wanted {
        Some(serial) => Some(Wanted {
            serial,
            from: "--device",
        }),
        None => from_env
            .as_deref()
            .map(str::trim)
            .filter(|serial| !serial.is_empty())
            .map(|serial| Wanted {
                serial,
                from: SERIAL_ENV,
            }),
    };
    let output = Command::new(sdk.adb())
        .arg("devices")
        .output()
        .context("run adb devices")?;
    let devices = parse_devices(&String::from_utf8_lossy(&output.stdout));
    let chosen = choose_device(&devices, wanted)?;
    if let Some(warning) = chosen.warning {
        crate::ui::warn(warning);
    }
    Ok(chosen.serial)
}

/// A device or emulator `adb devices` lists.
#[derive(Debug, PartialEq)]
struct AdbDevice {
    serial: String,
    /// `device` when it is ready; `unauthorized`, `offline`, ... otherwise.
    state: String,
}

/// The device asked for and where it was named (`--device`, `ANDROID_SERIAL`).
#[derive(Clone, Copy)]
struct Wanted<'a> {
    serial: &'a str,
    from: &'static str,
}

/// The device [`choose_device`] chose, and what to tell the user about it.
#[derive(Debug, PartialEq)]
struct Chosen {
    serial: String,
    warning: Option<String>,
}

/// The devices in `adb devices`' output: a line per device, its serial and
/// state separated by a tab, after a header (and the daemon's messages,
/// starting with `*`, when it had to start).
fn parse_devices(text: &str) -> Vec<AdbDevice> {
    text.lines()
        .filter(|line| !line.starts_with('*'))
        .filter_map(|line| line.split_once('\t'))
        .map(|(serial, state)| AdbDevice {
            serial: serial.trim().to_owned(),
            state: state.trim().to_owned(),
        })
        .filter(|device| !device.serial.is_empty())
        .collect()
}

/// The device to use among `devices`: `wanted` when it is ready, else the
/// first ready one when none is wanted.
fn choose_device(devices: &[AdbDevice], wanted: Option<Wanted<'_>>) -> anyhow::Result<Chosen> {
    let listed = || {
        if devices.is_empty() {
            "none is connected (see `adb devices`)".to_owned()
        } else {
            let list: Vec<String> = devices
                .iter()
                .map(|d| {
                    if d.state == "device" {
                        d.serial.clone()
                    } else {
                        format!("{} ({})", d.serial, d.state)
                    }
                })
                .collect();
            format!("connected: {}", list.join(", "))
        }
    };
    if let Some(Wanted { serial, from }) = wanted {
        return match devices.iter().find(|d| d.serial == serial) {
            Some(d) if d.state == "device" => Ok(Chosen {
                serial: d.serial.clone(),
                warning: None,
            }),
            Some(d) => bail!(
                "the Android device {serial} ({from}) is {}, not ready; {}",
                d.state,
                listed()
            ),
            None => bail!(
                "no Android device {serial} ({from}) is connected; {}",
                listed()
            ),
        };
    }
    let ready: Vec<&AdbDevice> = devices.iter().filter(|d| d.state == "device").collect();
    let Some(first) = ready.first() else {
        if devices.is_empty() {
            bail!("no Android device or emulator connected (see `adb devices`)");
        }
        bail!("no Android device or emulator is ready; {}", listed());
    };
    let warning = (ready.len() > 1).then(|| {
        format!(
            "{} Android devices are connected, using {}: choose one with --device <serial> or {SERIAL_ENV} ({})",
            ready.len(),
            first.serial,
            ready
                .iter()
                .map(|d| d.serial.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    Ok(Chosen {
        serial: first.serial.clone(),
        warning,
    })
}

/// The ABIs `serial` runs, preferred first (`ro.product.cpu.abilist`).
pub(crate) fn device_abis(sdk: &Sdk, serial: &str) -> anyhow::Result<Vec<String>> {
    let list = adb(sdk, serial, &["shell", "getprop", "ro.product.cpu.abilist"])?;
    Ok(list
        .trim()
        .split(',')
        .filter(|abi| !abi.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The ABI to build an app for device `serial`, whose ABIs are `device`
/// (preferred first): the first one NSH builds for.
pub(crate) fn preferred_abi(serial: &str, device: &[String]) -> anyhow::Result<String> {
    device
        .iter()
        .find(|abi| ABIS.contains(&abi.as_str()))
        .cloned()
        .with_context(|| format!("{serial} runs none of {}", ABIS.join(", ")))
}

/// A port of the device forwarded to this machine (`adb reverse`), removed
/// when dropped, however the command ends (Ctrl-C too: it is
/// [`ui::Stoppable`] meanwhile): a dev host left on the device would
/// otherwise reach the next dev server on that port, of whatever app.
pub(crate) struct Reverse {
    adb: PathBuf,
    serial: String,
    port: String,
    _stoppable: crate::ui::Stoppable,
}

impl Reverse {
    pub(crate) fn add(sdk: &Sdk, serial: &str, port: u16) -> anyhow::Result<Reverse> {
        let port = format!("tcp:{port}");
        adb(sdk, serial, &["reverse", &port, &port])?;
        Ok(Reverse {
            adb: sdk.adb(),
            serial: serial.to_owned(),
            port,
            _stoppable: crate::ui::Stoppable::new(),
        })
    }
}

impl Drop for Reverse {
    fn drop(&mut self) {
        let removed = Command::new(&self.adb)
            .args(["-s", &self.serial, "reverse", "--remove", &self.port])
            .output();
        if !removed.is_ok_and(|o| o.status.success()) {
            crate::ui::debug(format!(
                "could not remove the adb reverse of {} on {}",
                self.port, self.serial
            ));
        }
    }
}

pub(crate) fn adb(sdk: &Sdk, serial: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new(sdk.adb())
        .args(["-s", serial])
        .args(args)
        .output()
        .context("run adb")?;
    if !output.status.success() {
        bail!(
            "adb {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::{AdbDevice, Chosen, Wanted, choose_device, parse_devices};

    fn devices(list: &[(&str, &str)]) -> Vec<AdbDevice> {
        list.iter()
            .map(|(serial, state)| AdbDevice {
                serial: (*serial).to_owned(),
                state: (*state).to_owned(),
            })
            .collect()
    }

    fn flag(serial: &str) -> Option<Wanted<'_>> {
        Some(Wanted {
            serial,
            from: "--device",
        })
    }

    fn error(result: anyhow::Result<Chosen>) -> String {
        result.err().map(|e| e.to_string()).unwrap_or_default()
    }

    /// `adb devices` lists a device per line after its header, with the
    /// daemon's messages first when it had to start; blank lines and the
    /// header are no devices.
    #[test]
    fn adb_devices_output_is_parsed() {
        let text = "* daemon not running; starting now at tcp:5037\n\
                    * daemon started successfully\n\
                    List of devices attached\n\
                    R5CR00000AA\tdevice\n\
                    emulator-5554\tdevice\n\
                    192.168.1.7:5555\toffline\n\
                    0123456789\tunauthorized\n\
                    \n";
        assert_eq!(
            parse_devices(text),
            devices(&[
                ("R5CR00000AA", "device"),
                ("emulator-5554", "device"),
                ("192.168.1.7:5555", "offline"),
                ("0123456789", "unauthorized"),
            ])
        );
        assert_eq!(parse_devices("List of devices attached\r\n\r\n"), []);
        assert_eq!(
            parse_devices("List of devices attached\r\nemulator-5554\tdevice\r\n"),
            devices(&[("emulator-5554", "device")])
        );
    }

    /// The device named (`--device`, `ANDROID_SERIAL`) is used when it is
    /// ready, whichever place it has in the list.
    #[test]
    fn the_device_named_is_used() {
        let list = devices(&[("R5CR00000AA", "device"), ("emulator-5554", "device")]);
        assert_eq!(
            choose_device(&list, flag("emulator-5554")).unwrap(),
            Chosen {
                serial: "emulator-5554".to_owned(),
                warning: None,
            }
        );
        let env = Some(Wanted {
            serial: "R5CR00000AA",
            from: "ANDROID_SERIAL",
        });
        assert_eq!(choose_device(&list, env).unwrap().serial, "R5CR00000AA");
    }

    /// A device named but missing or not ready is an error listing what is
    /// connected, never another device.
    #[test]
    fn a_device_named_but_not_ready_is_an_error() {
        let list = devices(&[("R5CR00000AA", "device"), ("0123456789", "unauthorized")]);
        assert_eq!(
            error(choose_device(&list, flag("emulator-5556"))),
            "no Android device emulator-5556 (--device) is connected; connected: R5CR00000AA, 0123456789 (unauthorized)"
        );
        assert_eq!(
            error(choose_device(&list, flag("0123456789"))),
            "the Android device 0123456789 (--device) is unauthorized, not ready; connected: R5CR00000AA, 0123456789 (unauthorized)"
        );
        let env = Some(Wanted {
            serial: "emulator-5554",
            from: "ANDROID_SERIAL",
        });
        assert_eq!(
            error(choose_device(&[], env)),
            "no Android device emulator-5554 (ANDROID_SERIAL) is connected; none is connected (see `adb devices`)"
        );
    }

    /// With none named, the first ready device is used; when several are
    /// ready, a warning says which one and how to choose another.
    #[test]
    fn without_a_name_the_first_ready_device_is_used() {
        let one = devices(&[("0123456789", "unauthorized"), ("emulator-5554", "device")]);
        assert_eq!(
            choose_device(&one, None).unwrap(),
            Chosen {
                serial: "emulator-5554".to_owned(),
                warning: None,
            }
        );
        let two = devices(&[
            ("R5CR00000AA", "device"),
            ("0123456789", "offline"),
            ("emulator-5554", "device"),
        ]);
        assert_eq!(
            choose_device(&two, None).unwrap(),
            Chosen {
                serial: "R5CR00000AA".to_owned(),
                warning: Some(
                    "2 Android devices are connected, using R5CR00000AA: choose one with --device <serial> or ANDROID_SERIAL (R5CR00000AA, emulator-5554)"
                        .to_owned()
                ),
            }
        );
        assert_eq!(
            error(choose_device(&[], None)),
            "no Android device or emulator connected (see `adb devices`)"
        );
        assert_eq!(
            error(choose_device(
                &devices(&[("emulator-5554", "offline")]),
                None
            )),
            "no Android device or emulator is ready; connected: emulator-5554 (offline)"
        );
    }

    /// An app is built for the device's preferred ABI that NSH builds for;
    /// a device that runs none of them is an error.
    #[test]
    fn an_app_is_built_for_the_devices_preferred_abi() {
        let abi = |list: &[&str]| {
            let device: Vec<String> = list.iter().map(|abi| abi.to_string()).collect();
            super::preferred_abi("emulator-5554", &device).ok()
        };
        assert_eq!(
            abi(&["arm64-v8a", "armeabi-v7a", "armeabi"]).as_deref(),
            Some("arm64-v8a")
        );
        assert_eq!(abi(&["x86_64", "arm64-v8a"]).as_deref(), Some("x86_64"));
        assert_eq!(abi(&["x86", "armeabi-v7a"]).as_deref(), Some("armeabi-v7a"));
        assert_eq!(abi(&["x86"]), None);
        assert_eq!(abi(&[]), None);
    }

    static NO_PLACEHOLDERS: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    /// The manifest of `apk`, which lists no libraries.
    fn manifest_of(apk: &super::Apk<'_>) -> String {
        super::manifest_xml(
            apk,
            &super::library_manifest::additions(
                apk,
                &super::library_manifest::Placeholders::new(apk.package, apk.manifest_placeholders),
                Vec::new(),
            )
            .unwrap(),
        )
    }

    fn apk(rust: bool, notifications: bool) -> super::Apk<'static> {
        super::Apk {
            package: "com.example.app",
            label: "App",
            version_name: "1.0",
            version_code: 1,
            target_sdk: super::DEFAULT_TARGET_SDK,
            activity: "com.longbridge.nsh.AppActivity",
            dev: false,
            cleartext: false,
            notifications,
            microphone: false,
            camera: false,
            libraries: &[],
            library_cache: std::path::Path::new("."),
            manifest_placeholders: Some(&NO_PLACEHOLDERS),
            permissions: &[],
            queries: &[],
            application: &[],
            schemes: &[],
            links: &[],
            rust,
            dark: false,
            fixed_appearance: false,
            runtime: std::path::Path::new("."),
            abis: &[],
            icon: None,
            launch_screen: None,
            assets: Vec::new(),
            sounds: Vec::new(),
            res: None,
            crate_res: &[],
            jni_libs: &[],
            signing: super::Signing::Debug,
        }
    }

    /// A square PNG of `size` pixels in `dir`.
    fn png(dir: &std::path::Path, name: &str, size: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        image::RgbaImage::from_pixel(size, size, image::Rgba([0x1f, 0x1e, 0x1c, 255]))
            .save(&path)
            .unwrap();
        path
    }

    fn png_size(path: &std::path::Path) -> (u32, u32) {
        image::image_dimensions(path).unwrap()
    }

    #[test]
    fn one_png_stays_the_launcher_icon_as_it_was() {
        let dir = std::env::temp_dir().join(format!("nsh-android-icon-png-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let icon = png(&dir, "icon.png", 1024);
        let res = dir.join("res");
        let files = super::write_icon(super::Icon::Image(&icon), &res).unwrap();
        let launcher = res.join("mipmap-xxxhdpi").join("ic_launcher.png");
        assert_eq!(files, std::slice::from_ref(&launcher));
        assert_eq!(png_size(&launcher), (192, 192));
        assert!(!res.join("mipmap-anydpi-v26").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_adaptive_icon_names_its_layers_drawn_for_each_density() {
        let dir =
            std::env::temp_dir().join(format!("nsh-android-icon-adaptive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let adaptive = super::AdaptiveIcon {
            foreground: png(&dir, "fg.png", 432),
            background: super::IconBackground::Color(crate::apple::Color([0xff, 0xff, 0xff, 0xff])),
            monochrome: Some(png(&dir, "mono.png", 432)),
            unknown: Vec::new(),
        };
        let res = dir.join("res");
        let files = super::write_icon(super::Icon::Adaptive(&adaptive), &res).unwrap();
        for (density, size) in [
            ("mdpi", 108),
            ("hdpi", 162),
            ("xhdpi", 216),
            ("xxhdpi", 324),
            ("xxxhdpi", 432),
        ] {
            for layer in ["ic_launcher_foreground", "ic_launcher_monochrome"] {
                let path = res
                    .join(format!("mipmap-{density}"))
                    .join(format!("{layer}.png"));
                assert!(files.contains(&path), "{}", path.display());
                assert_eq!(png_size(&path), (size, size), "{}", path.display());
            }
            // A color background is a resource, not a PNG.
            assert!(
                !res.join(format!("mipmap-{density}"))
                    .join("ic_launcher_background.png")
                    .exists()
            );
        }
        let colors =
            std::fs::read_to_string(res.join("values").join("ic_launcher_background.xml")).unwrap();
        assert!(
            colors.contains(r#"<color name="ic_launcher_background">#FFFFFFFF</color>"#),
            "{colors}"
        );
        let xml =
            std::fs::read_to_string(res.join("mipmap-anydpi-v26").join("ic_launcher.xml")).unwrap();
        for layer in [
            r#"<background android:drawable="@color/ic_launcher_background" />"#,
            r#"<foreground android:drawable="@mipmap/ic_launcher_foreground" />"#,
            r#"<monochrome android:drawable="@mipmap/ic_launcher_monochrome" />"#,
        ] {
            assert!(xml.contains(layer), "{xml}");
        }
        let round =
            std::fs::read_to_string(res.join("mipmap-anydpi-v26").join("ic_launcher_round.xml"))
                .unwrap();
        assert_eq!(round, xml);
        assert!(!res.join("mipmap-xxxhdpi").join("ic_launcher.png").exists());

        // A PNG background is a layer like the foreground; no monochrome
        // layer without one.
        let res = dir.join("res-image");
        let adaptive = super::AdaptiveIcon {
            background: super::IconBackground::Image(png(&dir, "bg.png", 432)),
            monochrome: None,
            ..adaptive
        };
        super::write_icon(super::Icon::Adaptive(&adaptive), &res).unwrap();
        assert_eq!(
            png_size(&res.join("mipmap-hdpi").join("ic_launcher_background.png")),
            (162, 162)
        );
        assert!(!res.join("values").exists());
        assert!(
            !res.join("mipmap-hdpi")
                .join("ic_launcher_monochrome.png")
                .exists()
        );
        let xml =
            std::fs::read_to_string(res.join("mipmap-anydpi-v26").join("ic_launcher.xml")).unwrap();
        assert!(
            xml.contains(r#"<background android:drawable="@mipmap/ic_launcher_background" />"#),
            "{xml}"
        );
        assert!(!xml.contains("monochrome"), "{xml}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_an_adaptive_icon_declares_a_round_icon() {
        let adaptive = super::AdaptiveIcon {
            foreground: "fg.png".into(),
            background: super::IconBackground::Color(crate::apple::Color([0, 0, 0, 255])),
            monochrome: None,
            unknown: Vec::new(),
        };
        let manifest = manifest_of(&super::Apk {
            icon: Some(super::Icon::Adaptive(&adaptive)),
            ..apk(false, false)
        });
        assert!(
            manifest.contains(r#"android:icon="@mipmap/ic_launcher""#),
            "{manifest}"
        );
        assert!(
            manifest.contains(r#"android:roundIcon="@mipmap/ic_launcher_round""#),
            "{manifest}"
        );
        let manifest = manifest_of(&super::Apk {
            icon: Some(super::Icon::Image(std::path::Path::new("icon.png"))),
            ..apk(false, false)
        });
        assert!(
            manifest.contains(r#"android:icon="@mipmap/ic_launcher""#),
            "{manifest}"
        );
        assert!(!manifest.contains("roundIcon"), "{manifest}");
    }

    #[test]
    fn a_rust_app_runs_its_root() {
        // `AppActivity` reads it from the application's meta-data.
        let flag = r#"<meta-data android:name="com.longbridge.nsh.RUST" android:value="true" />"#;
        let manifest = manifest_of(&apk(true, false));
        assert!(manifest.contains(flag), "{manifest}");
        let application = manifest.find("<application").unwrap();
        let activity = manifest.find("<activity").unwrap();
        let at = manifest.find(flag).unwrap();
        assert!(application < at && at < activity, "{manifest}");
        assert!(!manifest_of(&apk(false, false)).contains("com.longbridge.nsh.RUST"));
    }

    #[test]
    fn a_packaged_app_declares_the_files_provider() {
        // `SystemUi.openPath` hands the system's viewer a `content:` URI
        // of this authority; without the declaration the provider is not
        // resolved and the file cannot be opened.
        let manifest = manifest_of(&apk(false, false));
        let provider = "<provider android:name=\"com.longbridge.nsh.FilesProvider\"";
        assert!(manifest.contains(provider), "{manifest}");
        assert!(
            manifest.contains("android:exported=\"false\""),
            "{manifest}"
        );
        assert!(
            manifest.contains("android:grantUriPermissions=\"true\""),
            "{manifest}"
        );
        let application = manifest.find("<application").unwrap();
        let activity = manifest.find("<activity").unwrap();
        let at = manifest.find(provider).unwrap();
        assert!(application < at && at < activity, "{manifest}");
    }

    #[test]
    fn only_an_app_granted_notifications_declares_their_permission() {
        let permission = "android.permission.POST_NOTIFICATIONS";
        assert!(manifest_of(&apk(false, true)).contains(permission));
        assert!(!manifest_of(&apk(false, false)).contains(permission));
    }

    #[test]
    fn only_an_app_whose_pages_capture_media_declares_its_permissions() {
        let manifest = manifest_of(&super::Apk {
            microphone: true,
            ..apk(false, false)
        });
        assert!(manifest.contains("android.permission.RECORD_AUDIO"));
        assert!(!manifest.contains("android.permission.CAMERA"));
        assert!(!manifest_of(&apk(false, false)).contains("RECORD_AUDIO"));
    }

    #[test]
    fn each_web_link_has_a_filter_android_verifies_with_its_host_and_paths() {
        let links = nsh_core::url_scheme::AppLinks::from_json(&serde_json::json!({
            "links": [
                { "host": "example.com", "paths": ["/orders/*", "/about", "/a&b"] },
                { "host": "*.shop.example" }
            ]
        }));
        let manifest = manifest_of(&super::Apk {
            links: links.web(),
            ..apk(false, false)
        });
        let filters: Vec<&str> = manifest
            .split("<intent-filter android:autoVerify=\"true\">")
            .skip(1)
            .map(|rest| rest.split("</intent-filter>").next().unwrap())
            .collect();
        assert_eq!(filters.len(), 2, "{manifest}");
        for filter in &filters {
            for line in [
                r#"<action android:name="android.intent.action.VIEW" />"#,
                r#"<category android:name="android.intent.category.DEFAULT" />"#,
                r#"<category android:name="android.intent.category.BROWSABLE" />"#,
                r#"<data android:scheme="https" />"#,
            ] {
                assert!(filter.contains(line), "{line} in {filter}");
            }
            // Only `https`: Android verifies no other.
            assert!(!filter.contains(r#"android:scheme="http""#), "{filter}");
        }
        // The first host with its paths, and none of another host's.
        assert!(filters[0].contains(r#"<data android:host="example.com" />"#));
        assert!(filters[0].contains(r#"<data android:pathPrefix="/orders/" />"#));
        assert!(filters[0].contains(r#"<data android:path="/about" />"#));
        assert!(filters[0].contains(r#"<data android:path="/a&amp;b" />"#));
        assert!(!filters[0].contains("shop.example"));
        // Every path of the other.
        assert!(filters[1].contains(r#"<data android:host="*.shop.example" />"#));
        assert!(!filters[1].contains("android:path"), "{}", filters[1]);
        // A link opens the running app, as a scheme's does.
        assert!(manifest.contains(r#"android:launchMode="singleTask""#));
        let none = manifest_of(&apk(false, false));
        assert!(!none.contains("autoVerify"), "{none}");
        assert!(!none.contains("singleTask"), "{none}");
    }

    /// A runtime of one dex and one ABI and what `aapt2 link` writes, in a
    /// new `dir`.
    fn runtime_and_base(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        use std::io::Write as _;
        let dir =
            std::env::temp_dir().join(format!("nsh-cli-android-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let runtime = dir.join("runtime");
        std::fs::create_dir_all(runtime.join("lib/arm64-v8a")).unwrap();
        std::fs::write(runtime.join("classes.dex"), b"dex\n035\0runtime").unwrap();
        std::fs::write(runtime.join("lib/arm64-v8a/libnsh.so"), b"so").unwrap();
        let base = dir.join("base.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&base).unwrap());
        for name in ["AndroidManifest.xml", "resources.arsc"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"linked").unwrap();
        }
        zip.finish().unwrap();
        (dir, base)
    }

    fn entries(zip: &std::path::Path) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        (0..archive.len())
            .map(|i| {
                archive
                    .by_index_raw(i)
                    .unwrap()
                    .name()
                    .unwrap()
                    .into_owned()
            })
            .collect()
    }

    fn read(zip: &std::path::Path, name: &str) -> Vec<u8> {
        use std::io::Read as _;
        let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        let mut file = archive.by_name(name).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }

    /// An SDK that is not there: a build that needed one of its tools fails.
    fn no_sdk() -> super::Sdk {
        super::Sdk {
            root: "/nsh-no-sdk".into(),
            build_tools: "/nsh-no-sdk/build-tools/36.1.0".into(),
        }
    }

    #[test]
    fn the_libraries_dex_and_java_resources_follow_the_runtimes() {
        let (dir, base) = runtime_and_base("libraries");
        let runtime = dir.join("runtime");
        let java = super::libraries::Contents {
            dex: vec![dir.join("a.dex"), dir.join("b.dex")],
            resources: vec![(
                "META-INF/services/com.example.Greeter".to_owned(),
                b"com.example.Hello\n".to_vec(),
            )],
            ..Default::default()
        };
        std::fs::write(&java.dex[0], b"dex\n035\0a").unwrap();
        std::fs::write(&java.dex[1], b"dex\n035\0b").unwrap();
        for (layout, prefix, root) in [
            (&super::Layout::APK, "", ""),
            (&super::Layout::BUNDLE, "base/dex/", "base/root/"),
        ] {
            let out = dir.join("out.zip");
            let apk = super::Apk {
                runtime: &runtime,
                ..apk(false, false)
            };
            super::add_files(&base, &out, &apk, &java, layout).unwrap();
            assert_eq!(
                read(&out, &format!("{prefix}classes.dex")),
                b"dex\n035\0runtime"
            );
            assert_eq!(read(&out, &format!("{prefix}classes2.dex")), b"dex\n035\0a");
            assert_eq!(read(&out, &format!("{prefix}classes3.dex")), b"dex\n035\0b");
            // At the root of the installed APK, where `ServiceLoader` finds it.
            assert_eq!(
                read(
                    &out,
                    &format!("{root}META-INF/services/com.example.Greeter")
                ),
                b"com.example.Hello\n"
            );
        }
        // A runtime of two dex files would collide with the libraries'.
        std::fs::write(runtime.join("classes2.dex"), b"dex\n035\0more").unwrap();
        let apk = super::Apk {
            runtime: &runtime,
            ..apk(false, false)
        };
        let err = super::add_files(
            &base,
            &dir.join("out.zip"),
            &apk,
            &java,
            &super::Layout::APK,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("more than one dex file"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dex_libraries_are_numbered_as_listed_without_d8() {
        let (dir, _) = runtime_and_base("dex-libraries");
        let library = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            super::Library {
                path,
                listed: "nsh.json `android.libraries`".into(),
            }
        };
        let libraries = [
            library("b.dex", b"dex\n035\0b"),
            library("a.dex", b"dex\n035\0a"),
            // The same file from another crate: packaged once.
            library("b-copy.dex", b"dex\n035\0b"),
        ];
        let cache = dir.join("cache");
        let apk = super::Apk {
            libraries: &libraries,
            library_cache: &cache,
            ..apk(false, false)
        };
        // Dex files only: no SDK tool runs, no cache is made.
        let java = super::libraries::prepare(&no_sdk(), &apk, &dir).unwrap();
        assert_eq!(java.dex, [dir.join("b.dex"), dir.join("a.dex")]);
        assert!(java.resources.is_empty());
        assert!(!cache.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn without_libraries_the_package_is_as_before() {
        let (dir, base) = runtime_and_base("no-libraries");
        let runtime = dir.join("runtime");
        let cache = dir.join("cache");
        let apk = super::Apk {
            runtime: &runtime,
            library_cache: &cache,
            assets: vec![("nsh/bundle.nshc".into(), b"bundle".to_vec())],
            ..apk(false, false)
        };
        // Nothing for d8: no SDK tool runs, no cache is made.
        let java = super::libraries::prepare(&no_sdk(), &apk, &dir).unwrap();
        assert!(java.dex.is_empty() && java.resources.is_empty());
        assert!(java.native.is_empty() && java.assets.is_empty());
        assert!(!cache.exists());
        // The files of the package, as they were before libraries.
        for (layout, expected) in [
            (
                &super::Layout::APK,
                &[
                    "AndroidManifest.xml",
                    "resources.arsc",
                    "classes.dex",
                    "lib/arm64-v8a/libnsh.so",
                    "assets/nsh/bundle.nshc",
                ],
            ),
            (
                &super::Layout::BUNDLE,
                &[
                    "base/manifest/AndroidManifest.xml",
                    "base/resources.arsc",
                    "base/dex/classes.dex",
                    "base/lib/arm64-v8a/libnsh.so",
                    "base/assets/nsh/bundle.nshc",
                ],
            ),
        ] {
            let out = dir.join("out.zip");
            super::add_files(&base, &out, &apk, &java, layout).unwrap();
            assert_eq!(entries(&out), expected);
        }
        // The manifest, word for word: libraries add nothing to it.
        assert_eq!(super::manifest_xml(&apk, &java.manifest), MANIFEST);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An AAR at `path` with `entries` besides its manifest of `package`,
    /// whose `<application>` has `application`.
    fn aar(path: &std::path::Path, package: &str, application: &str, entries: &[(&str, &[u8])]) {
        use std::io::Write as _;
        let manifest = format!(
            r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:tools="http://schemas.android.com/tools" package="{package}">
    <uses-sdk android:minSdkVersion="23" />
    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.WAKE_LOCK" />
    <application android:appComponentFactory="x.Factory">{application}</application>
</manifest>"#
        );
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in [("AndroidManifest.xml", manifest.as_bytes())]
            .into_iter()
            .chain(entries.iter().copied())
        {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn an_aar_adds_its_native_libraries_assets_and_manifest() {
        let (dir, base) = runtime_and_base("aar");
        let runtime = dir.join("runtime");
        let push = dir.join("push.aar");
        aar(
            &push,
            "com.example.push",
            r#"<provider android:name="androidx.startup.InitializationProvider"
                   android:authorities="${applicationId}.androidx-startup" android:exported="false"
                   tools:node="merge"><meta-data android:name=".PushInit" android:value="androidx.startup" /></provider>
               <meta-data android:name="push.key" android:value="${PUSH_KEY}" />"#,
            &[
                ("jni/arm64-v8a/libpush.so", b"push arm64"),
                ("jni/x86_64/libpush.so", b"push x86_64"),
                ("assets/push/config.json", b"{}"),
                ("assets/push/ding.ogg", b"ogg"),
            ],
        );
        let libraries = [super::Library {
            path: push.clone(),
            listed: "push: [package.metadata.nsh] `android.libraries`".into(),
        }];
        // A crate adds its own initializer to the provider the library
        // declares.
        let application = [r#"<provider android:name="androidx.startup.InitializationProvider" android:authorities="${applicationId}.androidx-startup"><meta-data android:name="com.example.glue.Init" android:value="androidx.startup"/></provider>"#.to_owned()];
        let placeholders =
            std::collections::BTreeMap::from([("PUSH_KEY".to_owned(), "k-1".to_owned())]);
        let cache = dir.join("cache");
        let with =
            |abis: &'static [String],
             placeholders: &'static std::collections::BTreeMap<String, String>| {
                super::Apk {
                    runtime: &runtime,
                    libraries: &libraries,
                    library_cache: &cache,
                    manifest_placeholders: Some(placeholders),
                    application: &application,
                    abis,
                    ..apk(false, false)
                }
            };
        let placeholders: &'static _ = Box::leak(Box::new(placeholders));
        let apk = with(&[], placeholders);
        // No classes: no SDK tool runs.
        let java = super::libraries::prepare(&no_sdk(), &apk, &dir).unwrap();
        assert!(java.dex.is_empty() && !cache.exists());
        for (layout, lib, assets) in [
            (&super::Layout::APK, "lib/", "assets/"),
            (&super::Layout::BUNDLE, "base/lib/", "base/assets/"),
        ] {
            let out = dir.join("out.zip");
            // The runtime's one ABI: the library's x86_64 stays out.
            let abis = super::add_files(&base, &out, &apk, &java, layout).unwrap();
            assert_eq!(abis, ["arm64-v8a"]);
            let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
            let mut method = |name: &str| archive.by_name(name).unwrap().compression();
            assert_eq!(
                method(&format!("{lib}arm64-v8a/libpush.so")),
                zip::CompressionMethod::Stored
            );
            assert_eq!(
                method(&format!("{assets}push/config.json")),
                zip::CompressionMethod::Deflated
            );
            assert_eq!(
                method(&format!("{assets}push/ding.ogg")),
                zip::CompressionMethod::Stored
            );
            assert!(!entries(&out).iter().any(|e| e.contains("x86_64")));
        }
        let manifest = super::manifest_xml(&apk, &java.manifest);
        // The library's permission once NSH declares it already, its other
        // one after NSH's.
        assert_eq!(manifest.matches("android.permission.INTERNET").count(), 1);
        assert!(
            manifest.contains(
                "<uses-permission android:name=\"android.permission.INTERNET\" />\n    \
             <uses-permission android:name=\"android.permission.WAKE_LOCK\" />\n    <!--"
            ),
            "{manifest}"
        );
        // One provider with both initializers, the placeholders filled in.
        assert!(
            manifest.contains(
                "        <provider android:name=\"androidx.startup.InitializationProvider\" \
                 android:authorities=\"com.example.app.androidx-startup\" android:exported=\"false\">\n            \
                 <meta-data android:name=\"com.example.glue.Init\" android:value=\"androidx.startup\" />\n            \
                 <meta-data android:name=\".PushInit\" android:value=\"androidx.startup\" />\n        \
                 </provider>\n        \
                 <meta-data android:name=\"push.key\" android:value=\"k-1\" />\n    \
                 </application>"
            ),
            "{manifest}"
        );
        assert!(!manifest.contains("tools:") && !manifest.contains("x.Factory"));

        // An ABI the package has and the library has not.
        let both: &'static [String] =
            Box::leak(Box::new(["arm64-v8a".to_owned(), "armeabi-v7a".to_owned()]));
        std::fs::create_dir_all(runtime.join("lib/armeabi-v7a")).unwrap();
        std::fs::write(runtime.join("lib/armeabi-v7a/libnsh.so"), b"so").unwrap();
        let error = super::add_files(
            &base,
            &dir.join("out.zip"),
            &with(both, placeholders),
            &java,
            &super::Layout::APK,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("has native libraries for arm64-v8a, x86_64, not for armeabi-v7a"),
            "{error}"
        );
        // A placeholder nsh.json does not set.
        let error = super::libraries::prepare(&no_sdk(), &with(&[], &NO_PLACEHOLDERS), &dir)
            .unwrap_err()
            .to_string();
        assert!(error.contains("the placeholder `${PUSH_KEY}`"), "{error}");
        // A library built for a newer platform than the app compiles against.
        aar(
            &push,
            "com.example.push",
            "",
            &[(
                "META-INF/com/android/build/gradle/aar-metadata.properties",
                b"minCompileSdk=36\n",
            )],
        );
        let error = super::libraries::prepare(&no_sdk(), &apk, &dir)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(
                "(its minCompileSdk), the app against 35: set nsh.json `android.targetSdk` to 36"
            ),
            "{error}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The manifest of `apk(false, false)`, as packages had it before
    /// libraries.
    const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.example.app">
    <uses-permission android:name="android.permission.INTERNET" />
    <!-- Text actions (Translate and the like) the selection menu offers,
        as TextView's: Android 11+ hides other apps' activities otherwise. -->
    <queries>
        <intent>
            <action android:name="android.intent.action.PROCESS_TEXT" />
            <data android:mimeType="text/plain" />
        </intent>
    </queries>
    <application android:label="App"
        android:extractNativeLibs="false"
        android:enableOnBackInvokedCallback="true"
        android:supportsRtl="true"
        android:theme="@android:style/Theme.Material.Light.NoActionBar">
        <!-- Serves the app's own files to the apps that open them
            (`SystemUi.openPath`): the authority matches what
            `FilesProvider.uriFor` builds. -->
        <provider android:name="com.longbridge.nsh.FilesProvider"
            android:authorities="com.example.app.files"
            android:exported="false"
            android:grantUriPermissions="true" />
        <activity android:name="com.longbridge.nsh.AppActivity" android:exported="true"
            android:windowSoftInputMode="adjustResize"
            android:configChanges="orientation|screenSize|screenLayout|smallestScreenSize|keyboard|keyboardHidden|navigation|uiMode|density|fontScale|locale|layoutDirection">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#;

    /// An arm64 ELF shared library whose one loadable segment is aligned
    /// to `align`.
    fn elf(align: u64) -> Vec<u8> {
        elf_for(183, align)
    }

    /// A 64-bit little-endian ELF shared library for `machine` whose one
    /// loadable segment is aligned to `align`.
    fn elf_for(machine: u16, align: u64) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x40 + 0x38];
        bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
        bytes[0x10..0x12].copy_from_slice(&3u16.to_le_bytes()); // e_type: ET_DYN
        bytes[0x12..0x14].copy_from_slice(&machine.to_le_bytes()); // e_machine
        bytes[0x20..0x28].copy_from_slice(&0x40u64.to_le_bytes()); // e_phoff
        bytes[0x36..0x38].copy_from_slice(&0x38u16.to_le_bytes()); // e_phentsize
        bytes[0x38..0x3a].copy_from_slice(&1u16.to_le_bytes()); // e_phnum
        bytes[0x40..0x44].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
        bytes[0x40 + 0x30..0x40 + 0x38].copy_from_slice(&align.to_le_bytes());
        bytes
    }

    #[test]
    fn libraries_are_checked_as_elf_of_their_abi() {
        assert!(super::check_library(&elf(0x4000), "arm64-v8a", true).is_ok());
        assert!(super::check_library(&elf_for(62, 0x4000), "x86_64", true).is_ok());
        assert!(super::check_library(&elf(0x1000), "arm64-v8a", false).is_ok());
        for (bytes, says) in [
            (elf(0x1000), "16 KB"),
            (b"\x7fELF\x02\x01".to_vec(), "not a library for arm64-v8a"),
            (elf_for(62, 0x4000), "not a library for arm64-v8a"),
            (b"MZ not an elf".to_vec(), "not a little-endian ELF"),
        ] {
            let err = format!(
                "{:#}",
                super::check_library(&bytes, "arm64-v8a", true).unwrap_err()
            );
            assert!(err.contains(says), "{says}: {err}");
        }
        // A program header table at an offset that overflows, or past the
        // end, is truncated, never a panic.
        for phoff in [u64::MAX, 0x1000] {
            let mut far = elf(0x4000);
            far[0x20..0x28].copy_from_slice(&phoff.to_le_bytes());
            let err = format!(
                "{:#}",
                super::check_library(&far, "arm64-v8a", true).unwrap_err()
            );
            assert!(err.contains("truncated"), "{err}");
        }
        let mut executable = elf(0x4000);
        executable[0x10] = 2; // ET_EXEC
        let err = format!(
            "{:#}",
            super::check_library(&executable, "arm64-v8a", true).unwrap_err()
        );
        assert!(err.contains("not a shared library"), "{err}");
        let mut big_endian = elf(0x4000);
        big_endian[5] = 2;
        assert!(super::check_library(&big_endian, "arm64-v8a", true).is_err());
    }

    #[test]
    fn the_apps_resources_overlay_its_crates() {
        let p = std::path::PathBuf::from;
        // No crate resources: the app's link with NSH's, as before.
        let mut flat = vec![p("icon.flat")];
        let overlays = super::resource_layers(&mut flat, Some(p("app.zip")), Vec::new());
        assert_eq!(flat, [p("icon.flat"), p("app.zip")]);
        assert!(overlays.is_empty());
        // With them: NSH's and the app's stay the base (a name both
        // declare is an error, as before); the crates', then the app's
        // again, on top, so the app's value of a name a crate declares wins.
        let mut flat = vec![p("icon.flat")];
        let overlays =
            super::resource_layers(&mut flat, Some(p("app.zip")), vec![p("a.zip"), p("b.zip")]);
        assert_eq!(flat, [p("icon.flat"), p("app.zip")]);
        assert_eq!(overlays, [p("a.zip"), p("b.zip"), p("app.zip")]);
        let mut flat = Vec::new();
        assert_eq!(
            super::resource_layers(&mut flat, None, vec![p("a.zip")]),
            [p("a.zip")]
        );
    }

    #[test]
    fn crates_native_libraries_join_libnsh() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-android-jni-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let runtime = dir.join("runtime");
        for abi in ["arm64-v8a", "armeabi-v7a"] {
            std::fs::create_dir_all(runtime.join("lib").join(abi)).unwrap();
            std::fs::write(runtime.join("lib").join(abi).join("libnsh.so"), b"so").unwrap();
        }
        std::fs::write(runtime.join("classes.dex"), b"dex\n035\0runtime").unwrap();
        let base = dir.join("base.zip");
        zip::ZipWriter::new(std::fs::File::create(&base).unwrap())
            .finish()
            .unwrap();
        let jni = dir.join("jni");
        std::fs::create_dir_all(jni.join("arm64-v8a")).unwrap();
        std::fs::create_dir_all(jni.join("armeabi-v7a")).unwrap();
        std::fs::write(jni.join("arm64-v8a/libocr.so"), elf(0x4000)).unwrap();
        // 32-bit ABIs run on 4 KB pages: their alignment is not checked.
        let mut arm32 = vec![0u8; 0x34];
        arm32[..6].copy_from_slice(b"\x7fELF\x01\x01");
        arm32[0x10..0x12].copy_from_slice(&3u16.to_le_bytes());
        arm32[0x12..0x14].copy_from_slice(&40u16.to_le_bytes());
        std::fs::write(jni.join("armeabi-v7a/libocr.so"), &arm32).unwrap();
        // Only `lib*.so` files are libraries the system loads.
        std::fs::write(jni.join("arm64-v8a/README.txt"), b"not a library").unwrap();
        std::fs::write(jni.join("arm64-v8a/ocr.so"), elf(0x4000)).unwrap();
        // A second crate with the same library, byte for byte (two SDKs
        // each bundling `libc++_shared.so`), for both ABIs: packaged once.
        let other = dir.join("other");
        std::fs::create_dir_all(other.join("arm64-v8a")).unwrap();
        std::fs::create_dir_all(other.join("armeabi-v7a")).unwrap();
        std::fs::write(other.join("arm64-v8a/libocr.so"), elf(0x4000)).unwrap();
        std::fs::write(other.join("armeabi-v7a/libocr.so"), &arm32).unwrap();
        let dirs = [
            super::NativeDir {
                listed: "ocr: [package.metadata.nsh] `android.jniLibs`".into(),
                dir: jni.clone(),
            },
            super::NativeDir {
                listed: "scan: [package.metadata.nsh] `android.jniLibs`".into(),
                dir: other.clone(),
            },
        ];
        let both: &'static [String] =
            Box::leak(Box::new(["arm64-v8a".to_owned(), "armeabi-v7a".to_owned()]));
        // Packaged as the AARs' native libraries are, from what
        // `crate_native` reads (`required`: 16 KB pages).
        let add = |layout: &super::Layout, required: bool| {
            let java = super::libraries::Contents {
                native: super::crate_native(&dirs, required)?,
                ..Default::default()
            };
            let out = dir.join("out.zip");
            super::add_files(
                &base,
                &out,
                &super::Apk {
                    runtime: &runtime,
                    abis: both,
                    ..apk(false, false)
                },
                &java,
                layout,
            )
            .map(|_| entries(&out))
        };
        for (layout, lib) in [
            (&super::Layout::APK, "lib/"),
            (&super::Layout::BUNDLE, "base/lib/"),
        ] {
            let names = add(layout, true).unwrap();
            for name in [
                "arm64-v8a/libnsh.so",
                "arm64-v8a/libocr.so",
                "armeabi-v7a/libocr.so",
            ] {
                let name = format!("{lib}{name}");
                assert_eq!(
                    names.iter().filter(|n| **n == name).count(),
                    1,
                    "{name}: {names:?}"
                );
            }
            assert!(
                !names
                    .iter()
                    .any(|n| n.ends_with("README.txt") || n.ends_with("/ocr.so")),
                "{names:?}"
            );
        }
        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(dir.join("out.zip")).unwrap()).unwrap();
        assert_eq!(
            archive
                .by_name("base/lib/arm64-v8a/libocr.so")
                .unwrap()
                .compression(),
            zip::CompressionMethod::Stored
        );
        // A 64-bit library on 4 KB pages would not load on 16 KB devices:
        // an error when required (a release from target SDK 35, as Google
        // Play requires), a warning otherwise.
        std::fs::write(jni.join("arm64-v8a/libocr.so"), elf(0x1000)).unwrap();
        std::fs::write(other.join("arm64-v8a/libocr.so"), elf(0x1000)).unwrap();
        let err = format!("{:#}", add(&super::Layout::APK, true).unwrap_err());
        assert!(err.contains("16 KB") && err.starts_with("ocr: "), "{err}");
        assert!(add(&super::Layout::APK, false).is_ok());
        // Two crates' libraries of one name with other contents.
        std::fs::write(jni.join("arm64-v8a/libocr.so"), elf(0x8000)).unwrap();
        std::fs::write(other.join("arm64-v8a/libocr.so"), elf(0x4000)).unwrap();
        let err = format!("{:#}", add(&super::Layout::APK, true).unwrap_err());
        assert!(
            err.contains("both have `libocr.so` for arm64-v8a, with different contents"),
            "{err}"
        );
        // An x86_64 library in `arm64-v8a`.
        std::fs::write(jni.join("arm64-v8a/libocr.so"), elf_for(62, 0x4000)).unwrap();
        let err = format!("{:#}", add(&super::Layout::APK, true).unwrap_err());
        assert!(err.contains("not a library for arm64-v8a"), "{err}");
        // A library may not replace NSH's.
        std::fs::write(jni.join("arm64-v8a/libocr.so"), elf(0x4000)).unwrap();
        std::fs::write(other.join("arm64-v8a/libocr.so"), elf(0x4000)).unwrap();
        std::fs::write(jni.join("arm64-v8a/libnsh.so"), elf(0x4000)).unwrap();
        let err = format!("{:#}", add(&super::Layout::APK, true).unwrap_err());
        assert!(err.contains("NSH's own library"), "{err}");
        std::fs::remove_file(jni.join("arm64-v8a/libnsh.so")).unwrap();
        // A crate without libraries for an ABI the package has: its code
        // would not load on such a device.
        std::fs::remove_dir_all(other.join("armeabi-v7a")).unwrap();
        let err = format!("{:#}", add(&super::Layout::APK, true).unwrap_err());
        assert!(
            err.contains("scan: [package.metadata.nsh] `android.jniLibs` has native libraries for arm64-v8a, not for armeabi-v7a"),
            "{err}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_app_declares_the_permissions_it_lists_once() {
        let listed = [
            "android.permission.ACCESS_NETWORK_STATE".to_owned(),
            "android.permission.POST_NOTIFICATIONS".to_owned(),
            "com.example.permission.READ".to_owned(),
            "android.permission.INTERNET".to_owned(),
        ];
        let manifest = manifest_of(&super::Apk {
            permissions: &listed,
            ..apk(false, true)
        });
        let declared = |name: &str| {
            manifest
                .matches(&format!("<uses-permission android:name=\"{name}\" />"))
                .count()
        };
        assert_eq!(
            declared("android.permission.ACCESS_NETWORK_STATE"),
            1,
            "{manifest}"
        );
        assert_eq!(declared("com.example.permission.READ"), 1, "{manifest}");
        // NSH's own stay, once, even when the app lists them too.
        assert_eq!(declared("android.permission.INTERNET"), 1, "{manifest}");
        assert_eq!(
            declared("android.permission.POST_NOTIFICATIONS"),
            1,
            "{manifest}"
        );
        // Permissions go before the application, as Android reads them.
        let application = manifest.find("<application").unwrap();
        let at = manifest.find("ACCESS_NETWORK_STATE").unwrap();
        assert!(at < application, "{manifest}");
        assert!(!manifest_of(&apk(false, false)).contains("ACCESS_NETWORK_STATE"));
    }

    #[test]
    fn the_crates_add_queries_and_components() {
        let queries = [
            r#"<intent><action android:name="android.media.action.IMAGE_CAPTURE"/></intent>"#
                .to_owned(),
        ];
        let application = [
            r#"<provider android:name="x.Init" android:authorities="${applicationId}.init" android:exported="false"/>"#
                .to_owned(),
        ];
        let manifest = manifest_of(&super::Apk {
            queries: &queries,
            application: &application,
            ..apk(false, false)
        });
        let queries_end = manifest.find("</queries>").unwrap();
        let capture = manifest.find("IMAGE_CAPTURE").expect(&manifest);
        assert!(manifest.find("<queries>").unwrap() < capture && capture < queries_end);
        // Inside `<application>`, after NSH's own activity, the package filled in.
        let provider = manifest.find("x.Init").expect(&manifest);
        assert!(manifest.find("</activity>").unwrap() < provider);
        assert!(provider < manifest.find("</application>").unwrap());
        assert!(
            manifest.contains(r#"android:authorities="com.example.app.init""#),
            "{manifest}"
        );
        assert!(!manifest_of(&apk(false, false)).contains("IMAGE_CAPTURE"));
    }

    #[test]
    fn only_an_app_that_asks_allows_cleartext_traffic() {
        let attribute = r#"android:usesCleartextTraffic="true""#;
        assert!(!manifest_of(&apk(false, false)).contains(attribute));
        let manifest = manifest_of(&super::Apk {
            cleartext: true,
            ..apk(false, false)
        });
        let application = manifest.find("<application").unwrap();
        let activity = manifest.find("<activity").unwrap();
        let at = manifest.find(attribute).expect(&manifest);
        assert!(application < at && at < activity, "{manifest}");
    }

    fn text(files: &[super::ResFile], dir: &str) -> String {
        let file = files
            .iter()
            .find(|file| file.dir == dir)
            .unwrap_or_else(|| panic!("no res/{dir}"));
        match &file.contents {
            super::ResContents::Text(text) => text.clone(),
            super::ResContents::Copy(path) => panic!("res/{dir} copies {}", path.display()),
        }
    }

    #[test]
    fn a_launch_screen_names_its_theme_in_the_manifest() {
        let screen = crate::apple::LaunchScreen::default();
        let manifest = manifest_of(&super::Apk {
            launch_screen: Some(&screen),
            ..apk(false, false)
        });
        assert!(
            manifest.contains(r#"android:theme="@style/NshLaunch""#),
            "{manifest}"
        );
        let manifest = manifest_of(&apk(false, false));
        assert!(
            manifest.contains(r#"android:theme="@android:style/Theme.Material.Light.NoActionBar""#),
            "{manifest}"
        );
    }

    /// The window background and the splash screen take the screen's color,
    /// the dark one in night mode; the splash icon's own background is the
    /// same color, so it shows no plate around the icon.
    #[test]
    fn a_launch_screen_colors_the_window_and_the_splash_screen() {
        use crate::apple::{Color, LaunchScreen, Themed};
        let screen = LaunchScreen {
            background: Some(Themed::Each {
                light: Color([0xff, 0xff, 0xff, 0xff]),
                dark: Color([0x13, 0x13, 0x12, 0xff]),
            }),
            image: None,
        };
        let files = super::launch_resources(&screen, false, false, 35).unwrap();
        let values = text(&files, "values");
        assert!(
            values.contains(r#"<color name="nsh_launch_background">#ffffffff</color>"#),
            "{values}"
        );
        assert!(
            values.contains(r#"parent="@android:style/Theme.Material.Light.NoActionBar""#),
            "{values}"
        );
        assert!(
            values.contains(
                r#"<item name="android:windowBackground">@color/nsh_launch_background</item>"#
            ),
            "{values}"
        );
        let night = text(&files, "values-night");
        assert!(night.contains("#ff131312"), "{night}");
        let splash = text(&files, "values-v31");
        for item in ["windowBackground", "windowSplashScreenBackground"] {
            assert!(
                splash.contains(&format!(
                    r#"<item name="android:{item}">@color/nsh_launch_background</item>"#
                )),
                "{splash}"
            );
        }
        assert!(
            !splash.contains("windowSplashScreenAnimatedIcon"),
            "{splash}"
        );
        // The system shows an adaptive launcher icon's foreground alone only
        // while no icon background is set.
        assert!(
            !splash.contains("windowSplashScreenIconBackgroundColor"),
            "{splash}"
        );
        // Compiled against a platform before Android 12: no splash
        // attributes to link.
        let files = super::launch_resources(&screen, false, false, 30).unwrap();
        assert!(files.iter().all(|file| file.dir != "values-v31"));
        // An app that is always dark shows the dark color in either mode.
        let files = super::launch_resources(&screen, true, true, 35).unwrap();
        assert!(text(&files, "values").contains("#ff131312"));
        assert!(files.iter().all(|file| file.dir != "values-night"));
    }

    /// The image is the splash screen's icon, centered at a third of its
    /// pixels in dp (as iOS draws it), one for each appearance.
    #[test]
    fn a_launch_screen_image_is_the_splash_icon() {
        use crate::apple::{LaunchScreen, Themed};
        let dir = std::env::temp_dir().join(format!("nsh-android-launch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (light, dark) = (dir.join("light.png"), dir.join("dark.png"));
        image::RgbaImage::new(288, 288).save(&light).unwrap();
        image::RgbaImage::new(144, 96).save(&dark).unwrap();
        let screen = LaunchScreen {
            background: None,
            image: Some(Themed::Each {
                light: light.clone(),
                dark: dark.clone(),
            }),
        };
        let files = super::launch_resources(&screen, true, false, 35).unwrap();
        let splash = text(&files, "values-v31");
        assert!(
            splash.contains(
                r#"<item name="android:windowSplashScreenAnimatedIcon">@drawable/nsh_launch</item>"#
            ),
            "{splash}"
        );
        assert!(
            splash.contains(r#"parent="@android:style/Theme.Material.NoActionBar""#),
            "{splash}"
        );
        // No color: the platform's window background stays.
        assert!(!splash.contains("windowBackground"), "{splash}");
        // 288 px is 96 dp: a third of the 288 dp icon area, a third of it
        // inset on each side.
        let icon = text(&files, "drawable");
        assert!(
            icon.contains(r#"android:drawable="@drawable/nsh_launch_image""#),
            "{icon}"
        );
        assert!(icon.contains(r#"android:insetLeft="33.333%""#), "{icon}");
        assert!(icon.contains(r#"android:insetTop="33.333%""#), "{icon}");
        // Not square: its width and its height inset apart.
        let night = text(&files, "drawable-night");
        assert!(night.contains(r#"android:insetLeft="41.667%""#), "{night}");
        assert!(night.contains(r#"android:insetTop="44.444%""#), "{night}");
        let copied = |dir: &str| {
            files.iter().find_map(|file| match &file.contents {
                super::ResContents::Copy(path) if file.dir == dir => Some(path.clone()),
                _ => None,
            })
        };
        assert_eq!(copied("drawable-nodpi"), Some(light));
        assert_eq!(copied("drawable-night-nodpi"), Some(dark));
        // Checked before Android 12 too, and only a PNG.
        let jpeg = dir.join("launch.jpg");
        std::fs::write(&jpeg, b"\xff\xd8\xff\xe0not a png").unwrap();
        for path in [jpeg, dir.join("missing.png")] {
            let screen = LaunchScreen {
                background: None,
                image: Some(Themed::Both(path.clone())),
            };
            assert!(
                super::launch_resources(&screen, false, false, 30).is_err(),
                "{}",
                path.display()
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bundle_metadata_is_what_bundletool_writes() {
        // `bundletool build-bundle` 1.18.1's own `BundleConfig.pb` and, for
        // a module with these two ABIs, `base/native.pb`.
        assert_eq!(super::bundle_config_pb(&[]), b"\x0a\x08\x12\x061.18.1");
        // Libraries' assets of the kinds `aapt2` stores stay stored, by
        // their own paths, escaped as globs.
        let stored = ["assets/a[1].mp3".to_owned()];
        assert_eq!(
            super::bundle_config_pb(&stored),
            b"\x0a\x08\x12\x061.18.1\x1a\x13\x0a\x11assets/a\\[1\\].mp3"
        );
        let abis = ["arm64-v8a".to_owned(), "armeabi-v7a".to_owned()];
        let mut expected = b"\x0a\x15\x0a\x0dlib/arm64-v8a\x12\x04\x0a\x02\x08\x03".to_vec();
        expected.extend_from_slice(b"\x0a\x17\x0a\x0flib/armeabi-v7a\x12\x04\x0a\x02\x08\x02");
        assert_eq!(super::native_pb(&abis).unwrap(), expected);
    }
}
