use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::anyhow;
use oxc::span::SourceType;
use rolldown::{
    ModuleType,
    plugin::{
        HookLoadArgs, HookLoadOutput, HookLoadReturn, HookRenderChunkArgs, HookRenderChunkOutput,
        HookRenderChunkReturn, HookResolveIdArgs, HookResolveIdOutput, HookResolveIdReturn,
        HookTransformArgs, HookTransformOutput, HookTransformOutputMap, HookTransformReturn,
        HookUsage, Plugin, PluginContext, SharedLoadPluginContext, SharedTransformPluginContext,
    },
};

use crate::compile::{NativeModules, compile_native};

/// Compiles the app's modules with nsh's JSX compiler against the native
/// modules (kept as imports) and loads its assets.
#[derive(Debug)]
pub(crate) struct NshPlugin {
    /// Paths in error messages are shown relative to this directory.
    pub(crate) root: PathBuf,
    /// The first compile error, already formatted. Rolldown wraps plugin
    /// errors in its own report; the bundler shows this one instead.
    pub(crate) error: Arc<Mutex<Option<String>>>,
    /// Asset files imported by the app (absolute paths).
    pub(crate) assets: Arc<Mutex<Vec<PathBuf>>>,
    /// Development bundle with hot module replacement: components and
    /// contexts of app modules are registered with the hot runtime.
    pub(crate) hot: bool,
    /// The app's themes (`nsh.json`).
    pub(crate) tailwind: Arc<ThemeSetup>,
    /// The entry module (it sets up the themes).
    pub(crate) entry: PathBuf,
    /// The native modules the app imports.
    pub(crate) natives: Arc<Mutex<std::collections::BTreeSet<String>>>,
    /// Static native imports whose namespaces hot factories require immediately.
    pub(crate) preloads: Option<Arc<Mutex<std::collections::BTreeSet<String>>>>,
    /// The native modules there are.
    pub(crate) modules: Arc<NativeModules>,
    /// A split bundle: dynamic imports of chunks call the runtime's chunk
    /// loader (`split.rs`).
    pub(crate) split: bool,
}

/// The `"tailwind"`, `"theme"`, `"appearance"` and `"statusBar"` of the
/// app's `nsh.json`.
#[derive(Debug, Default)]
pub(crate) struct ThemeSetup {
    /// The Tailwind theme as JSON, for the runtime (`None`: the default).
    pub(crate) json: Option<String>,
    /// Parsed, to check classes at build time.
    pub(crate) theme: nsh_core::tailwind::Theme,
    /// The theme of the Rust components (a gpui-kit theme file), checked.
    pub(crate) ui: Option<String>,
    /// `"appearance"`: `"light"` or `"dark"` (`None`: the system's).
    pub(crate) appearance: Option<&'static str>,
    /// `"statusBar"`, checked, as JSON: the status bar's default.
    pub(crate) status_bar: Option<String>,
}

/// File types imported as image assets.
pub(crate) const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "heic"];

/// File types imported as fonts.
pub(crate) const FONT_EXTENSIONS: &[&str] = &["ttf", "otf"];

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// The family name a font registers under: the typographic family (name
/// 16) when present, else the legacy family (name 1).
fn font_family(data: &[u8]) -> Result<String, String> {
    let face = ttf_parser::Face::parse(data, 0).map_err(|e| e.to_string())?;
    let name = |id| {
        face.names()
            .into_iter()
            .filter(|n| n.name_id == id && n.is_unicode())
            .find_map(|n| n.to_string())
    };
    name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
        .or_else(|| name(ttf_parser::name_id::FAMILY))
        .ok_or_else(|| "the font has no family name".to_owned())
}

/// `logo@2x.png` -> 2.0: the pixel density encoded in the file name.
fn density(path: &Path) -> f64 {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    match stem.rsplit_once('@').and_then(|(_, d)| d.strip_suffix('x')) {
        Some(d) => d.parse().unwrap_or(1.0),
        None => 1.0,
    }
}

/// Whether the file name says its density (`logo@2x.png`, `logo@1x.png`).
fn has_density(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    stem.rsplit_once('@')
        .and_then(|(_, d)| d.strip_suffix('x'))
        .is_some_and(|d| d.parse::<f64>().is_ok())
}

/// The pixel-density variants of image `path` (a file without a density
/// suffix): its siblings `<stem>@<scale>x.<ext>` for the scales of
/// [`nsh_core::IMAGE_SCALES`] above 1 that exist. Stat calls only: no
/// directory listing.
pub(crate) fn variants(path: &Path) -> Vec<(f32, PathBuf)> {
    if has_density(path) {
        return Vec::new();
    }
    let (Some(stem), Some(ext)) = (
        path.file_stem().and_then(|s| s.to_str()),
        path.extension().and_then(|e| e.to_str()),
    ) else {
        return Vec::new();
    };
    nsh_core::IMAGE_SCALES
        .into_iter()
        .filter(|s| *s != 1.0)
        .map(|s| (s, path.with_file_name(format!("{stem}@{s}x.{ext}"))))
        .filter(|(_, p)| p.is_file())
        .collect()
}

/// Why the `scale` variant `rel` of an image of `points` (its size in
/// points) does not fit it: its `pixels` are not the points times the
/// scale (to the pixel).
fn variant_mismatch(
    rel: &str,
    points: (f64, f64),
    scale: f32,
    pixels: (usize, usize),
) -> Option<String> {
    let scale = f64::from(scale);
    let expected = (points.0 * scale, points.1 * scale);
    let off = |want: f64, got: usize| (want - got as f64).abs() >= 1.0;
    (off(expected.0, pixels.0) || off(expected.1, pixels.1)).then(|| {
        format!(
            "{rel}: {}x{} pixels, expected {}x{} ({}x{} points at @{scale}x)",
            pixels.0, pixels.1, expected.0, expected.1, points.0, points.1
        )
    })
}

/// The module of an image import: its URI and size in points and, when
/// it has variants (their `scales`, above 1), the scales it ships in, 1
/// first. Without variants it is `{ uri, width, height }`.
fn image_module(uri: &str, (w, h): (f64, f64), scales: &[f32]) -> String {
    let scales = if scales.is_empty() {
        String::new()
    } else {
        let list: Vec<String> = std::iter::once(1.0)
            .chain(scales.iter().copied())
            .map(|s| s.to_string())
            .collect();
        format!(", scales: [{}]", list.join(", "))
    };
    format!("export default {{ uri: {uri:?}, width: {w}, height: {h}{scales} }};")
}

impl NshPlugin {
    /// The entry's first statement, giving the renderer the app's themes,
    /// appearance and status bar.
    fn prologue(&self) -> Option<String> {
        let json = |v: &Option<String>| match v {
            Some(json) => serde_json::Value::String(json.clone()).to_string(),
            None => "null".to_owned(),
        };
        let setup = &self.tailwind;
        (setup.json.is_some()
            || setup.ui.is_some()
            || setup.appearance.is_some()
            || setup.status_bar.is_some())
        .then(|| {
            format!(
                "_$setup({}, {}, {}, {});",
                json(&setup.json),
                json(&setup.ui),
                json(&setup.appearance.map(str::to_owned)),
                json(&setup.status_bar)
            )
        })
    }

    /// Records `message` as the build's error (shown as it is).
    fn refuse(&self, message: String) -> anyhow::Error {
        self.error
            .lock()
            .expect("error lock")
            .get_or_insert_with(|| message.clone());
        anyhow!(message)
    }

    /// `path` relative to the app's directory with `/` separators, as asset
    /// URLs, module ids and messages name it on every system.
    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn fail(&self, path: &Path, detail: &str) -> anyhow::Error {
        let message = format!("{}\n{detail}", self.rel(path));
        self.error
            .lock()
            .expect("error lock")
            .get_or_insert_with(|| message.clone());
        anyhow!(message)
    }
}

impl Plugin for NshPlugin {
    fn name(&self) -> Cow<'static, str> {
        "nsh".into()
    }

    fn register_hook_usage(&self) -> HookUsage {
        let usage = HookUsage::ResolveId | HookUsage::Load | HookUsage::Transform;
        if self.split {
            usage | HookUsage::RenderChunk
        } else {
            usage
        }
    }

    async fn render_chunk(
        &self,
        _cx: &PluginContext,
        args: &HookRenderChunkArgs<'_>,
    ) -> HookRenderChunkReturn {
        // What each chunk imports statically, by file name.
        let imports: std::collections::HashMap<&str, Vec<String>> = args
            .chunks
            .values()
            .map(|c| {
                let own = c.filename.as_str();
                let imports =
                    crate::bundle::chunk_imports(c.imports.iter().map(|i| i.as_str()), own);
                (own, imports)
            })
            .collect();
        let rewritten = crate::split::rewrite_imports(&args.code, &args.chunk.filename, |id| {
            imports.get(id).map(Vec::as_slice)
        });
        Ok(rewritten.map(|(code, map)| HookRenderChunkOutput {
            code,
            map: HookTransformOutputMap::Sourcemap(Box::new(map)),
        }))
    }

    async fn resolve_id(
        &self,
        _cx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> HookResolveIdReturn {
        let spec = args.specifier;
        if self.modules.contains(spec) {
            // Hot factories keep external import() expressions. Preloading
            // their namespaces would turn an optional native API into an
            // eager dependency, including on hosts where it is unavailable.
            self.natives
                .lock()
                .expect("natives lock")
                .insert(spec.to_owned());
            if !args.kind.is_dynamic()
                && let Some(preloads) = &self.preloads
            {
                preloads
                    .lock()
                    .expect("preloads lock")
                    .insert(spec.to_owned());
            }
            return Ok(Some(HookResolveIdOutput {
                external: Some(true.into()),
                ..HookResolveIdOutput::from_id(spec)
            }));
        }
        let from = args
            .importer
            .map(|i| format!("{}: ", self.rel(Path::new(i))))
            .unwrap_or_default();
        if spec == "solid-js" || spec.starts_with("solid-js/") {
            return Err(self.refuse(format!(
                "{from}`{spec}` is not part of nsh apps: import signal, computed, effect, batch, untracked, cleanup, context and mount from \"nsh\""
            )));
        }
        // Rolldown may otherwise replace unsupported Node builtins with browser
        // shims. These are absent from the reference runtime catalog.
        if ["events", "assert", "util", "string_decoder", "fs"].contains(&spec)
            || spec.starts_with("node:")
            || spec.starts_with("@nsh/llrt-types/")
            || spec.starts_with("nsh/")
            || spec.starts_with("nsh:")
        {
            return Err(self.refuse(format!("{from}nsh has no module `{spec}`")));
        }
        Ok(None)
    }

    async fn load(&self, _cx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        let path = Path::new(args.id);
        let rel = self.rel(path);
        if has_extension(path, FONT_EXTENSIONS) {
            // A font import is its family name; the runtime registers the
            // font before the first frame.
            let data = std::fs::read(path).map_err(|e| self.fail(path, &e.to_string()))?;
            let family = font_family(&data)
                .map_err(|e| self.fail(path, &format!("cannot read font: {e}")))?;
            self.assets
                .lock()
                .expect("assets lock")
                .push(path.to_path_buf());
            let code = format!(
                "import {{ font }} from \"nsh:rt\";\nexport default font({uri:?}, {family:?});",
                uri = format!("asset:/{rel}"),
            );
            return Ok(Some(HookLoadOutput {
                code: code.into(),
                module_type: Some(ModuleType::Js),
                ..Default::default()
            }));
        }
        if !has_extension(path, IMAGE_EXTENSIONS) {
            return Ok(None);
        }
        // An image import is an asset descriptor; the natural size (in
        // points) lets the image lay out before it is decoded.
        let dim = imagesize::size(path)
            .map_err(|e| self.fail(path, &format!("cannot read image: {e}")))?;
        let scale = density(path);
        let (w, h) = (dim.width as f64 / scale, dim.height as f64 / scale);
        // Its pixel-density variants ship too; the core loads the one for
        // the screen.
        let variants = variants(path);
        for (scale, variant) in &variants {
            let d = imagesize::size(variant)
                .map_err(|e| self.fail(variant, &format!("cannot read image: {e}")))?;
            if let Some(warning) =
                variant_mismatch(&self.rel(variant), (w, h), *scale, (d.width, d.height))
            {
                log::warn!(target: "nsh::build", "{warning}");
            }
        }
        {
            let mut assets = self.assets.lock().expect("assets lock");
            assets.push(path.to_path_buf());
            assets.extend(variants.iter().map(|(_, p)| p.clone()));
        }
        let scales: Vec<f32> = variants.iter().map(|(s, _)| *s).collect();
        let code = image_module(&format!("asset:/{rel}"), (w, h), &scales);
        Ok(Some(HookLoadOutput {
            code: code.into(),
            module_type: Some(ModuleType::Js),
            ..Default::default()
        }))
    }

    async fn transform(
        &self,
        _cx: SharedTransformPluginContext,
        args: &HookTransformArgs<'_>,
    ) -> HookTransformReturn {
        let jsx_module = matches!(args.module_type, ModuleType::Tsx | ModuleType::Jsx);
        // Rolldown's own helpers.
        if args.id.starts_with('\0') {
            return Ok(None);
        }
        let path = Path::new(args.id);
        let entry = path == self.entry;
        let script = matches!(args.module_type, ModuleType::Ts | ModuleType::Js);
        // A module that reads `import.meta` gets its own URL (`metaUrl`).
        // (JSON, text and CSS modules are not code, whatever they say.)
        let meta = script && args.code.contains("import.meta");
        if !(jsx_module || entry || self.hot && script || meta) {
            return Ok(None);
        }
        let hot_key = self.hot.then(|| self.rel(path));
        // By module type, not file name: asset modules (`logo.png`) are JS.
        let source_type = match args.module_type {
            ModuleType::Tsx => SourceType::tsx(),
            ModuleType::Jsx => SourceType::jsx(),
            ModuleType::Ts => SourceType::ts(),
            _ => SourceType::mjs(),
        };
        if jsx_module {
            let rel = self.rel(path);
            for u in crate::classes::check(args.code, source_type, &self.tailwind.theme) {
                log::warn!(target: "nsh::build", "{rel}:{}:{}: unknown class `{}`", u.line, u.column, u.class);
            }
        }
        let prologue = if entry { self.prologue() } else { None };
        let compiled = compile_native(
            path,
            source_type,
            args.code,
            prologue.as_deref(),
            hot_key.as_deref(),
            Some(&self.rel(path)),
            &self.modules,
        )
        .map_err(|detail| self.fail(path, &detail))?;
        if !compiled.destructured.is_empty() {
            let rel = self.rel(path);
            for d in &compiled.destructured {
                log::warn!(target: "nsh::build", "{rel}:{}:{}: {}", d.line, d.column, d.message());
            }
        }
        Ok(Some(HookTransformOutput {
            code: Some(compiled.code),
            map: compiled.map.into(),
            module_type: Some(ModuleType::Js),
            ..Default::default()
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::{BundleOptions, bundle};

    /// The header of a `w`×`h` PNG: all the image size reader looks at.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        out.extend(w.to_be_bytes());
        out.extend(h.to_be_bytes());
        out.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        out
    }

    /// An app importing `import` (from `img/`), with `files` (name, pixel
    /// size) in `img/`.
    fn app(name: &str, import: &str, files: &[(&str, (u32, u32))]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsh-variants-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("img")).unwrap();
        for (file, (w, h)) in files {
            std::fs::write(dir.join("img").join(file), png(*w, *h)).unwrap();
        }
        std::fs::write(
            dir.join("App.tsx"),
            format!(
                "import {{ mount }} from \"nsh\";\nimport {{ Image }} from \"nsh/image\";\nimport icon from \"./img/{import}\";\nmount(() => <Image src={{icon}} />);\n"
            ),
        )
        .unwrap();
        dir
    }

    fn try_bundle(dir: &Path) -> Result<crate::Bundle, crate::BundleError> {
        bundle(&BundleOptions {
            entry: dir.join("App.tsx"),
            minify: false,
            split: false,
            ui_sets: Vec::new(),
        })
    }

    fn bundled(dir: &Path) -> crate::Bundle {
        try_bundle(dir).unwrap()
    }

    fn assets(bundle: &crate::Bundle) -> Vec<String> {
        bundle
            .assets
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn variant_probe_lists_the_scales_in_the_image_module() {
        let module = |scales: &[f32]| image_module("asset:/img/flag.png", (40.0, 30.0), scales);
        assert_eq!(
            module(&[2.0, 3.0]),
            "export default { uri: \"asset:/img/flag.png\", width: 40, height: 30, scales: [1, 2, 3] };"
        );
        assert_eq!(
            module(&[1.5, 4.0]),
            "export default { uri: \"asset:/img/flag.png\", width: 40, height: 30, scales: [1, 1.5, 4] };"
        );
        // Without variants: `{ uri, width, height }`.
        assert_eq!(
            module(&[]),
            "export default { uri: \"asset:/img/flag.png\", width: 40, height: 30 };"
        );
    }

    #[test]
    fn variant_probe_ships_the_siblings_and_lists_their_scales() {
        let dir = app(
            "ships",
            "flag.png",
            &[
                ("flag.png", (40, 30)),
                ("flag@2x.png", (80, 60)),
                ("flag@3x.png", (120, 90)),
                ("other@2x.png", (2, 2)),
            ],
        );
        let bundle = bundled(&dir);
        assert!(bundle.code.contains("scales"), "{}", bundle.code);
        assert_eq!(
            assets(&bundle),
            ["img/flag.png", "img/flag@2x.png", "img/flag@3x.png"]
        );
    }

    #[test]
    fn variant_probe_finds_every_scale_and_skips_gaps() {
        let dir = app(
            "every",
            "a.webp",
            &[
                ("a.webp", (10, 10)),
                ("a@1.5x.webp", (15, 15)),
                ("a@4x.webp", (40, 40)),
            ],
        );
        let variants: Vec<f32> = variants(&dir.join("img/a.webp"))
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        assert_eq!(variants, [1.5, 4.0]);
        let bundle = bundled(&dir);
        assert!(bundle.code.contains("1.5"), "{}", bundle.code);
        assert_eq!(
            assets(&bundle),
            ["img/a.webp", "img/a@1.5x.webp", "img/a@4x.webp"]
        );
    }

    #[test]
    fn variant_probe_leaves_an_nx_import_as_it_was() {
        let dir = app(
            "suffixed",
            "flag@3x.png",
            &[
                ("flag.png", (40, 30)),
                ("flag@2x.png", (80, 60)),
                ("flag@3x.png", (120, 90)),
            ],
        );
        assert!(variants(&dir.join("img/flag@3x.png")).is_empty());
        assert!(variants(&dir.join("img/flag@1x.png")).is_empty());
        let bundle = bundled(&dir);
        assert!(!bundle.code.contains("scales"), "{}", bundle.code);
        assert!(bundle.code.contains("asset:/img/flag@3x.png"));
        assert_eq!(assets(&bundle), ["img/flag@3x.png"]);
    }

    /// An app without variants ships its image alone, in a module without
    /// `scales` (`variant_probe_lists_the_scales_in_the_image_module` holds
    /// the module's text).
    #[test]
    fn variant_probe_changes_nothing_without_variants() {
        let dir = app("none", "flag.png", &[("flag.png", (40, 30))]);
        let bundle = bundled(&dir);
        assert!(
            bundle.code.contains("asset:/img/flag.png"),
            "{}",
            bundle.code
        );
        assert!(!bundle.code.contains("scales"), "{}", bundle.code);
        assert_eq!(assets(&bundle), ["img/flag.png"]);
    }

    #[test]
    fn variant_probe_warns_when_a_variant_has_the_wrong_size() {
        let fits = |scale, pixels| variant_mismatch("img/a@2x.png", (40.0, 30.0), scale, pixels);
        assert_eq!(fits(2.0, (80, 60)), None);
        assert_eq!(fits(3.0, (120, 90)), None);
        let warning = fits(2.0, (120, 90)).expect("a warning");
        assert!(
            warning.starts_with("img/a@2x.png: 120x90 pixels, expected 80x60"),
            "{warning}"
        );
        // 1.5x of an odd size: either rounding fits.
        let odd = |pixels| variant_mismatch("b@1.5x.png", (41.0, 31.0), 1.5, pixels);
        assert_eq!(odd((61, 46)), None);
        assert_eq!(odd((62, 47)), None);
        assert!(odd((60, 46)).is_some());
        // The bundle still builds, with the variant.
        let dir = app(
            "mismatch",
            "a.png",
            &[("a.png", (40, 30)), ("a@2x.png", (120, 90))],
        );
        assert_eq!(assets(&bundled(&dir)), ["img/a.png", "img/a@2x.png"]);
    }

    /// A variant that is no image fails the build, as the imported file
    /// does, naming it.
    #[test]
    fn variant_probe_fails_on_an_unreadable_variant() {
        let dir = app("unreadable", "a.png", &[("a.png", (40, 30))]);
        std::fs::write(dir.join("img/a@3x.png"), b"not an image").unwrap();
        let Err(error) = try_bundle(&dir) else {
            panic!("the build fails");
        };
        let error = error.to_string();
        assert!(
            error.starts_with("img/a@3x.png\ncannot read image: "),
            "{error}"
        );
    }
}
