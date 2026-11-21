//! Android apps without Gradle: the APK is assembled from the prebuilt
//! runtime (`classes.dex`, `libnsh.so` per ABI) and the app bundle with the
//! SDK's build tools (`aapt2`, `zipalign`, `apksigner`).

use std::{
    env, fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};
use serde::Deserialize;

use crate::apple::{Color, LaunchScreen, Themed};

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
        if cfg!(windows) && name == "apksigner" {
            return self.build_tools.join("apksigner.bat");
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
    /// The app's own dex files (nsh.json `android.dex`), after the
    /// runtime's: `classes2.dex`, `classes3.dex`, ...
    pub(crate) dex: &'a [PathBuf],
    /// The permissions the app declares itself (nsh.json
    /// `android.permissions`), in full: declared after NSH's, each once.
    pub(crate) permissions: &'a [String],
    /// Elements the app's crates add to the manifest's `<queries>`
    /// ([`crate::module_meta`]): the other apps they start.
    pub(crate) queries: &'a [String],
    /// Elements the app's crates add to the manifest's `<application>`
    /// ([`crate::module_meta`]): services, receivers, providers, activities
    /// and meta-data. `${applicationId}` in them is the app's package.
    pub(crate) application: &'a [String],
    /// URL schemes that open the app (nsh.json `schemes`): one
    /// `VIEW` intent filter.
    pub(crate) schemes: &'a [String],
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
    pub(crate) signing: Signing,
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

    let manifest = work.join("AndroidManifest.xml");
    fs::write(&manifest, manifest_xml(apk))?;
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
    run(&mut link)?;

    if format == Format::Aab {
        let unsigned = work.join("unsigned.aab");
        write_bundle(&base, &unsigned, apk)?;
        sign_bundle(&apk.signing, &unsigned)?;
        fs::rename(&unsigned, out)?;
        fs::remove_dir_all(&work)?;
        return Ok(());
    }

    // Code, native libraries and app files.
    let unaligned = work.join("unaligned.apk");
    add_files(&base, &unaligned, apk, &Layout::APK)?;
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

fn manifest_xml(apk: &Apk<'_>) -> String {
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
    // NSH's own, then the app's (`android.permissions`), each once.
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
    let permissions: String = permissions
        .iter()
        .map(|name| format!("\n    <uses-permission android:name=\"{name}\" />"))
        .collect();
    // A link opens the running app (`onNewIntent`), not another activity in
    // the caller's task.
    let launch_mode = if apk.schemes.is_empty() {
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
    let elements = |list: &[String], indent: &str| -> String {
        list.iter()
            .map(|e| {
                format!(
                    "\n{indent}{}",
                    e.trim().replace("${applicationId}", apk.package)
                )
            })
            .collect()
    };
    let queries = elements(apk.queries, "        ");
    let components = elements(apk.application, "        ");
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
            </intent-filter>{schemes}
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
}

impl Layout {
    const APK: Layout = Layout {
        linked: "",
        dex: "classes.dex",
        lib: "lib",
        assets: "assets",
    };
    /// The bundle's one module, `base`.
    const BUNDLE: Layout = Layout {
        linked: "base/",
        dex: "base/dex/classes.dex",
        lib: "base/lib",
        assets: "base/assets",
    };
}

/// An Android App Bundle: the module `base` from the proto-format `linked`,
/// [`add_files`]' code, libraries and assets, the libraries' ABIs
/// (`base/native.pb`) and `BundleConfig.pb`.
fn write_bundle(linked: &Path, out: &Path, apk: &Apk<'_>) -> anyhow::Result<()> {
    let files = work_file(out, "files.zip");
    let abis = add_files(linked, &files, apk, &Layout::BUNDLE)?;
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
    zip.write_all(&bundle_config_pb())?;
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

/// `BundleConfig { bundletool { version } }` (bundletool's `config.proto`).
fn bundle_config_pb() -> Vec<u8> {
    let mut bundletool = Vec::new();
    pb::bytes(&mut bundletool, 2, BUNDLETOOL_VERSION.as_bytes());
    let mut config = Vec::new();
    pb::bytes(&mut config, 1, &bundletool);
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

/// Copies `base` (manifest, resources) and adds the dex, the libraries
/// (stored: the system maps them in place) and the assets, where `layout`
/// keeps them; returns the ABIs of the libraries.
fn add_files(
    base: &Path,
    out: &Path,
    apk: &Apk<'_>,
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
    // The app's own after the runtime's: Android loads `classes<N>.dex`
    // from 2 up, until one is missing. The numbers assume the runtime is
    // one dex; a runtime of more would collide with the app's.
    if !apk.dex.is_empty() && apk.runtime.join("classes2.dex").exists() {
        bail!(
            "the Android runtime in {} has more than one dex file, which nsh.json android.dex cannot follow yet",
            apk.runtime.display()
        );
    }
    let dex_dir = layout
        .dex
        .strip_suffix("classes.dex")
        .context("dex layout")?;
    for (i, path) in apk.dex.iter().enumerate() {
        let bytes = fs::read(path)
            .with_context(|| format!("nsh.json android.dex: read {}", path.display()))?;
        if !bytes.starts_with(b"dex\n") {
            bail!(
                "nsh.json android.dex: {} is not a dex file (make one with the SDK's `d8`)",
                path.display()
            );
        }
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
    }
    for (path, bytes) in &apk.assets {
        // Bytecode and images are compressed already or small.
        zip.start_file(format!("{}/{path}", layout.assets), deflate)?;
        zip.write_all(bytes)?;
    }
    zip.finish()?;
    Ok(abis.to_vec())
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
            dex: &[],
            permissions: &[],
            queries: &[],
            application: &[],
            schemes: &[],
            rust,
            dark: false,
            fixed_appearance: false,
            runtime: std::path::Path::new("."),
            abis: &[],
            icon: None,
            launch_screen: None,
            assets: Vec::new(),
            sounds: Vec::new(),
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
        let manifest = super::manifest_xml(&super::Apk {
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
        let manifest = super::manifest_xml(&super::Apk {
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
        let manifest = super::manifest_xml(&apk(true, false));
        assert!(manifest.contains(flag), "{manifest}");
        let application = manifest.find("<application").unwrap();
        let activity = manifest.find("<activity").unwrap();
        let at = manifest.find(flag).unwrap();
        assert!(application < at && at < activity, "{manifest}");
        assert!(!super::manifest_xml(&apk(false, false)).contains("com.longbridge.nsh.RUST"));
    }

    #[test]
    fn a_packaged_app_declares_the_files_provider() {
        // `SystemUi.openPath` hands the system's viewer a `content:` URI
        // of this authority; without the declaration the provider is not
        // resolved and the file cannot be opened.
        let manifest = super::manifest_xml(&apk(false, false));
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
        assert!(super::manifest_xml(&apk(false, true)).contains(permission));
        assert!(!super::manifest_xml(&apk(false, false)).contains(permission));
    }

    #[test]
    fn only_an_app_whose_pages_capture_media_declares_its_permissions() {
        let manifest = super::manifest_xml(&super::Apk {
            microphone: true,
            ..apk(false, false)
        });
        assert!(manifest.contains("android.permission.RECORD_AUDIO"));
        assert!(!manifest.contains("android.permission.CAMERA"));
        assert!(!super::manifest_xml(&apk(false, false)).contains("RECORD_AUDIO"));
    }

    #[test]
    fn the_apps_dex_files_follow_the_runtimes() {
        use std::io::Read as _;
        let dir = std::env::temp_dir().join(format!("nsh-cli-android-dex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let runtime = dir.join("runtime");
        std::fs::create_dir_all(runtime.join("lib/arm64-v8a")).unwrap();
        std::fs::write(runtime.join("classes.dex"), b"dex\n035\0runtime").unwrap();
        std::fs::write(runtime.join("lib/arm64-v8a/libnsh.so"), b"so").unwrap();
        let base = dir.join("base.zip");
        zip::ZipWriter::new(std::fs::File::create(&base).unwrap())
            .finish()
            .unwrap();
        let app = [dir.join("a.dex"), dir.join("b.dex")];
        std::fs::write(&app[0], b"dex\n035\0a").unwrap();
        std::fs::write(&app[1], b"dex\n035\0b").unwrap();
        let read = |zip: &std::path::Path, name: &str| {
            let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
            let mut file = archive.by_name(name).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            bytes
        };
        for (layout, prefix) in [
            (&super::Layout::APK, ""),
            (&super::Layout::BUNDLE, "base/dex/"),
        ] {
            let out = dir.join("out.zip");
            super::add_files(
                &base,
                &out,
                &super::Apk {
                    runtime: &runtime,
                    dex: &app,
                    ..apk(false, false)
                },
                layout,
            )
            .unwrap();
            assert_eq!(
                read(&out, &format!("{prefix}classes.dex")),
                b"dex\n035\0runtime"
            );
            assert_eq!(read(&out, &format!("{prefix}classes2.dex")), b"dex\n035\0a");
            assert_eq!(read(&out, &format!("{prefix}classes3.dex")), b"dex\n035\0b");
        }
        // A file that is no dex fails the build instead of a class lookup on the device.
        std::fs::write(&app[1], b"PK\x03\x04 a jar").unwrap();
        let err = super::add_files(
            &base,
            &dir.join("out.zip"),
            &super::Apk {
                runtime: &runtime,
                dex: &app,
                ..apk(false, false)
            },
            &super::Layout::APK,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("is not a dex file"), "{err}");
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
        let manifest = super::manifest_xml(&super::Apk {
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
        assert!(!super::manifest_xml(&apk(false, false)).contains("ACCESS_NETWORK_STATE"));
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
        let manifest = super::manifest_xml(&super::Apk {
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
        assert!(!super::manifest_xml(&apk(false, false)).contains("IMAGE_CAPTURE"));
    }

    #[test]
    fn only_an_app_that_asks_allows_cleartext_traffic() {
        let attribute = r#"android:usesCleartextTraffic="true""#;
        assert!(!super::manifest_xml(&apk(false, false)).contains(attribute));
        let manifest = super::manifest_xml(&super::Apk {
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
        let manifest = super::manifest_xml(&super::Apk {
            launch_screen: Some(&screen),
            ..apk(false, false)
        });
        assert!(
            manifest.contains(r#"android:theme="@style/NshLaunch""#),
            "{manifest}"
        );
        let manifest = super::manifest_xml(&apk(false, false));
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
        assert_eq!(super::bundle_config_pb(), b"\x0a\x08\x12\x061.18.1");
        let abis = ["arm64-v8a".to_owned(), "armeabi-v7a".to_owned()];
        let mut expected = b"\x0a\x15\x0a\x0dlib/arm64-v8a\x12\x04\x0a\x02\x08\x03".to_vec();
        expected.extend_from_slice(b"\x0a\x17\x0a\x0flib/armeabi-v7a\x12\x04\x0a\x02\x08\x02");
        assert_eq!(super::native_pb(&abis).unwrap(), expected);
    }
}
