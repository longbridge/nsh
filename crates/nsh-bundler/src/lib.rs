//! JS bundler for nsh apps, written in Rust end to end (no Node.js).
//!
//! Each `.tsx`/`.jsx` module is compiled by nsh's JSX compiler ([`jsx`],
//! TypeScript removed in the same pass) against the native modules (`nsh`,
//! `nsh:rt`, ...), which stay imports; rolldown bundles the app into one
//! ES module holding only the app's code. The runtime is native, so apps
//! need no `node_modules` for it.

#[cfg(feature = "bundle")]
mod bundle;
#[cfg(feature = "bundle")]
mod classes;
mod compile;
mod hot;
mod jsx;
#[cfg(feature = "bundle")]
mod minify;
#[cfg(feature = "bundle")]
mod plugin;
#[cfg(feature = "bundle")]
mod split;
#[cfg(feature = "bundle")]
mod types;

#[cfg(feature = "bundle")]
pub use bundle::*;
#[cfg(feature = "bundle")]
pub use minify::minify_module;
#[cfg(feature = "bundle")]
pub use types::{TSCONFIG, type_definitions};

/// Compiles one module (TSX, JSX, TS or JS, as `name`'s extension says) to
/// the ES module the runtime runs: TypeScript removed, JSX compiled, imports
/// kept (only nsh's native modules resolve at run time, so a single module
/// importing nothing else is a whole app). For hosts that cannot bundle,
/// such as the website's playground in WebAssembly; errors come back as
/// text with a code frame.
pub fn compile_module(
    name: &str,
    source: &str,
    ui_sets: &[nsh_core::ui::Registry],
) -> Result<String, String> {
    let path = std::path::Path::new(name);
    let source_type = oxc::span::SourceType::from_path(path).map_err(|e| e.to_string())?;
    let modules = compile::NativeModules::new(ui_sets);
    compile::compile_native(path, source_type, source, None, None, &modules)
        .map(|(code, _)| code)
        .map_err(|e| compile::strip_ansi(&e))
}

#[cfg(test)]
mod tests {
    #[test]
    fn compile_module_makes_a_runnable_module() {
        let code = super::compile_module(
            "App.tsx",
            "import { mount, signal, View } from \"nsh\";\n\
             import { Button } from \"nsh/ui/mobile\";\n\
             const n: number = 1;\n\
             function App() { const c = signal(n); return <View><Button title=\"x\" onPress={() => c.set(2)} /></View>; }\n\
             mount(App);\n",
            &[nsh_ui_mobile::registry()],
        )
        .unwrap();
        assert!(code.contains("from \"nsh:rt\""), "{code}");
        assert!(code.contains("from \"nsh/ui/mobile\""), "{code}");
        assert!(!code.contains(": number"), "types are removed: {code}");
        assert!(!code.contains("<View"), "JSX is compiled: {code}");
    }

    #[test]
    fn compile_module_errors_have_a_code_frame_without_colors() {
        let err = super::compile_module("App.tsx", "const = ;", &[]).unwrap_err();
        assert!(err.contains("const = ;"), "{err}");
        assert!(!err.contains('\u{1b}'), "{err}");
    }
}
