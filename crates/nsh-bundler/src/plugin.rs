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
    /// The native modules there are.
    pub(crate) modules: Arc<NativeModules>,
    /// A split bundle: dynamic imports of chunks call the runtime's chunk
    /// loader (`split.rs`).
    pub(crate) split: bool,
}

/// The `"tailwind"` and `"theme"` of the app's `nsh.json`.
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

impl NshPlugin {
    /// The entry's first statement, giving the renderer the app's themes.
    fn prologue(&self) -> Option<String> {
        let json = |v: &Option<String>| match v {
            Some(json) => serde_json::Value::String(json.clone()).to_string(),
            None => "null".to_owned(),
        };
        let setup = &self.tailwind;
        (setup.json.is_some() || setup.ui.is_some() || setup.appearance.is_some()).then(|| {
            format!(
                "_$setup({}, {}, {});",
                json(&setup.json),
                json(&setup.ui),
                json(&setup.appearance.map(str::to_owned))
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
        _ctx: &PluginContext,
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
        _ctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> HookResolveIdReturn {
        let spec = args.specifier;
        if self.modules.contains(spec) {
            self.natives
                .lock()
                .expect("natives lock")
                .insert(spec.to_owned());
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

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
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
        self.assets
            .lock()
            .expect("assets lock")
            .push(path.to_path_buf());
        let code = format!(
            "export default {{ uri: {uri:?}, width: {w}, height: {h} }};",
            uri = format!("asset:/{rel}"),
            w = dim.width as f64 / scale,
            h = dim.height as f64 / scale,
        );
        Ok(Some(HookLoadOutput {
            code: code.into(),
            module_type: Some(ModuleType::Js),
            ..Default::default()
        }))
    }

    async fn transform(
        &self,
        _ctx: SharedTransformPluginContext,
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
        let meta = args.code.contains("import.meta");
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
        let (code, map) = compile_native(
            path,
            source_type,
            args.code,
            prologue.as_deref(),
            hot_key.as_deref(),
            Some(&self.rel(path)),
            &self.modules,
        )
        .map_err(|detail| self.fail(path, &detail))?;
        Ok(Some(HookTransformOutput {
            code: Some(code),
            map: map.into(),
            module_type: Some(ModuleType::Js),
            ..Default::default()
        }))
    }
}
