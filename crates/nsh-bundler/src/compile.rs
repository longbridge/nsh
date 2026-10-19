//! One module to the JS the runtime runs: TypeScript removed and the JSX
//! compiled by nsh's compiler ([`crate::jsx`]) in one oxc pass. The bundler
//! runs it on every module; without feature `bundle` it is all there is
//! ([`crate::compile_module`], e.g. for the website's playground in
//! WebAssembly, where rolldown does not build).

use std::path::Path;

use oxc::{
    allocator::Allocator,
    codegen::{Codegen, CodegenOptions},
    parser::Parser,
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{JsxOptions, TransformOptions, Transformer},
};
use oxc_sourcemap::SourceMap;

/// What the bundler knows of the native modules a host serves: nsh's own,
/// the runtime's registries and the UI sets (`BundleOptions::ui_sets`).
#[derive(Debug, Default)]
pub(crate) struct NativeModules {
    specifiers: std::collections::BTreeSet<String>,
    /// The UI sets' components (export, module), for hints about tags.
    pub(crate) components: Vec<(String, String)>,
}

impl NativeModules {
    pub(crate) fn new(ui_sets: &[nsh_core::ui::Registry]) -> Self {
        let mut out = Self {
            specifiers: nsh_core::js::NATIVE_MODULES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            ..Self::default()
        };
        for name in nsh_core::js::STANDARD_MODULES {
            out.specifiers.insert((*name).to_owned());
        }
        for r in nsh_core::runtime::registries().iter().chain(ui_sets) {
            let Some(spec) = r.specifier() else {
                continue;
            };
            out.specifiers.insert(spec.to_owned());
        }
        for r in ui_sets {
            let Some(spec) = r.specifier() else {
                continue;
            };
            out.components.extend(
                r.specs()
                    .iter()
                    .map(|s| (s.export.clone(), spec.to_owned())),
            );
        }
        out
    }

    /// Whether `spec` is a native module.
    #[cfg(feature = "bundle")]
    pub(crate) fn contains(&self, spec: &str) -> bool {
        self.specifiers.contains(spec)
    }
}

/// TSX -> JS with nsh's JSX compiler ([`crate::jsx`]), TypeScript removed
/// in the same pass; `prologue` runs first; with `hot_key` the module's
/// components are hot ([`crate::hot`]). Errors are rendered with a code
/// frame.
pub(crate) fn compile_native(
    path: &Path,
    source_type: SourceType,
    source: &str,
    prologue: Option<&str>,
    hot_key: Option<&str>,
    modules: &NativeModules,
) -> Result<(String, SourceMap<'static>), String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if let Some(err) = parsed.diagnostics.into_iter().next() {
        return Err(err.render_with_source_code(source.to_owned()));
    }
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new()
        .build(&program)
        .semantic
        .into_scoping();
    let options = TransformOptions {
        jsx: JsxOptions::disable(),
        // Imports are elided once the JSX is compiled
        // (`elide_unused_imports`): only `import type` goes here.
        typescript: oxc::transformer::TypeScriptOptions {
            only_remove_type_imports: true,
            ..Default::default()
        },
        ..TransformOptions::default()
    };
    let ret =
        Transformer::new(&allocator, path, &options).build_with_scoping(scoping, &mut program);
    if let Some(err) = ret.diagnostics.into_iter().next() {
        return Err(err.render_with_source_code(source.to_owned()));
    }
    if let Some(key) = hot_key {
        // Before the JSX goes: components are the functions with JSX.
        crate::hot::transform(&allocator, &mut program, source, key);
    }
    crate::jsx::transform(
        &allocator,
        &mut program,
        &ret.scoping,
        source,
        prologue,
        modules,
    )?;
    elide_unused_imports(&mut program);
    let out = Codegen::new()
        .with_options(CodegenOptions {
            source_map_path: Some(path.to_path_buf()),
            ..CodegenOptions::default()
        })
        .build(&program);
    Ok((
        out.code,
        out.map.expect("source map requested").into_owned(),
    ))
}

/// TypeScript's import elision, once the JSX is compiled (a primitive's
/// tag made in place no longer references its import): import specifiers
/// nothing references go (an import of types only, without `type`, or of a
/// primitive only its tags use), and declarations left without any.
fn elide_unused_imports(program: &mut oxc::ast::ast::Program<'_>) {
    use oxc::ast::ast::{ImportDeclarationSpecifier as S, Statement};
    let scoping = SemanticBuilder::new()
        .build(program)
        .semantic
        .into_scoping();
    let used = |local: &oxc::ast::ast::BindingIdentifier<'_>| {
        local
            .symbol_id
            .get()
            .is_none_or(|id| !scoping.get_resolved_reference_ids(id).is_empty())
    };
    program.body.retain_mut(|statement| {
        let Statement::ImportDeclaration(import) = statement else {
            return true;
        };
        let Some(specifiers) = &mut import.specifiers else {
            // `import "x"`: for its effects.
            return true;
        };
        if specifiers.is_empty() {
            return true;
        }
        specifiers.retain(|s| match s {
            S::ImportSpecifier(s) => used(&s.local),
            S::ImportDefaultSpecifier(s) => used(&s.local),
            S::ImportNamespaceSpecifier(s) => used(&s.local),
        });
        !specifiers.is_empty()
    });
}

/// Removes ANSI escape sequences (`ESC [ ... letter`).
pub(crate) fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
