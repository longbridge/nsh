//! The native ES modules apps import: nsh's own (`nsh`,
//! `nsh:rt`, `nsh/device`, `nsh/accessibility`, `nsh/keymap`, ...;
//! [`nsh_core::js::NATIVE_MODULES`])
//! and those of registries (`Registry::specifier`): the UI sets a host
//! serves (`nsh/ui/mobile`) and the runtime's (`nsh/android`, ...), whose
//! components, modules and re-exports become exports (`components.rs`).
//! A module of both kinds (`nsh/accessibility`) has the exports of both.
//!
//! The exports are made when a bundle first imports the module. QuickJS-NG
//! resolves imports when it compiles a module, so the bytecode compiler
//! installs the same loader (a module then only declares its export names;
//! nothing runs).
//!
//! The loader also compiles the chunks of a split bundle (`chunks.rs`): the
//! code the host gave for a name that is not a native module.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, c_char, c_int, c_void},
    sync::Arc,
};

use nsh_core::ui::{ComponentSpec, ModuleSpec, Registry};
use rquickjs::qjs;

use crate::{
    animated, canvas, components, desktop, dom, env, keymap, list, platform_modules,
    reactive::{self, Native},
    touch,
};

type JsValue = qjs::JSValue;
type JsContext = qjs::JSContext;

/// A value nsh's own modules export.
#[derive(Clone, Copy)]
pub(crate) enum Export {
    /// A native function and its `length`.
    Function(Native, c_int),
    /// A value made when the module is first imported (owned).
    Value(fn(*mut JsContext) -> JsValue),
}

/// The TypeScript declaration of an export (`.nsh/types`, assembled by
/// `declarations.rs`): documentation as plain text, the rest as it follows
/// the export's name.
#[derive(Clone, Copy)]
pub(crate) enum Ts {
    /// A function and its signatures (overloads): `<T>(value: T): Signal<T>`.
    Function(&'static str, &'static [&'static str]),
    /// A value and its type.
    Const(&'static str, &'static str),
    /// A class: ` extends Base { ... }` or ` { ... }`.
    Class(&'static str, &'static str),
    /// Not declared: what only compiled code or development bundles import.
    Internal,
}

/// An export: its name, how it is made and its declaration.
pub(crate) type Item = (&'static str, Export, Ts);

/// One of nsh's own modules.
pub(crate) struct NativeModule {
    pub(crate) name: &'static str,
    pub(crate) exports: &'static [Item],
    /// The module's own TypeScript types (interfaces, type aliases), next to
    /// the declarations of its exports.
    pub(crate) types: &'static str,
}

/// `nsh`: the API apps write against.
const NSH: &[Item] = &[
    (
        "signal",
        Export::Function(reactive::signal, 2),
        Ts::Function(
            "A signal holding `value`.",
            &[
                "<T>(value: T, options?: SignalOptions<T>): Signal<T>",
                "<T = undefined>(): Signal<T | undefined>",
            ],
        ),
    ),
    (
        "computed",
        Export::Function(reactive::computed, 2),
        Ts::Function(
            "A value derived from signals, computed again when they change (and read).",
            &["<T>(fn: (previous: T | undefined) => T, options?: SignalOptions<T>): Accessor<T>"],
        ),
    ),
    (
        "effect",
        Export::Function(reactive::effect, 1),
        Ts::Function(
            "Runs `fn` after rendering, and again when what it read changes.",
            &["(fn: () => void): void"],
        ),
    ),
    (
        "batch",
        Export::Function(reactive::batch, 1),
        Ts::Function(
            "Runs `fn`, applying its writes together at the end.",
            &["<T>(fn: () => T): T"],
        ),
    ),
    (
        "untracked",
        Export::Function(reactive::untrack, 1),
        Ts::Function(
            "Runs `fn` without tracking what it reads.",
            &["<T>(fn: () => T): T"],
        ),
    ),
    (
        "cleanup",
        Export::Function(reactive::on_cleanup, 1),
        Ts::Function(
            "Runs `fn` before the current effect or component runs again or goes away.",
            &["(fn: () => void): void"],
        ),
    ),
    (
        "context",
        Export::Function(dom::context, 1),
        Ts::Function(
            "A context with a default value.",
            &["<T>(defaultValue: T): Context<T>"],
        ),
    ),
    (
        "mount",
        Export::Function(dom::mount, 2),
        Ts::Function(
            "Renders `app` into the surface; returns a function that removes it.",
            &["<P extends object>(app: Component<P>, props?: P): () => void"],
        ),
    ),
    (
        "split",
        Export::Function(dom::split, 2),
        Ts::Function(
            "`[picked, rest]`: the props named in `keys` and the others, both still\nfollowing what they read (a component passing the rest on: `<Input {...rest} />`).",
            &[
                "<T extends object, K extends readonly (keyof T)[]>(props: T, keys: K): [Pick<T, K[number]>, Omit<T, K[number]>]",
            ],
        ),
    ),
    (
        "View",
        Export::Value(|ctx| dom::host_component(ctx, "view", true)),
        Ts::Function(
            "A view: the box layout, styles, touches and accessibility work on\n(UIView / ViewGroup).",
            &["(props: ViewProps): YNode"],
        ),
    ),
    (
        "Text",
        Export::Value(|ctx| dom::host_component(ctx, "text", true)),
        Ts::Function(
            "A paragraph of text; a `<Text>` in it is a span of the same paragraph, other\nelements are inline attachments.",
            &["(props: TextProps): YNode"],
        ),
    ),
    (
        "Scroll",
        Export::Value(|ctx| dom::host_component(ctx, "scroll", true)),
        Ts::Function(
            "A native scroll view (UIScrollView / ScrollView), zoomable; for long\nlists, `List` (`nsh/list`).",
            &["(props: ScrollProps): YNode"],
        ),
    ),
    (
        "Input",
        Export::Value(|ctx| dom::host_component(ctx, "input", true)),
        Ts::Function(
            "A native text input (UITextField / UITextView, EditText); `ref` has\n`focus()`, `blur()` and `setSelection(start, end)`.",
            &["(props: InputProps): YNode"],
        ),
    ),
];

/// `nsh`'s types; its element and style types and `JSX` are `types.d.ts`'s.
const NSH_TYPES: &str = r#"export type * from "./types";

/** A value that changes over time, read by calling it (tracked). */
export interface Accessor<T> {
  (): T;
}

/** A signal: `count()` reads it, `count.set(1)` and `count.update((n) => n + 1)` write it. */
export interface Signal<T> extends Accessor<T> {
  set(value: T): void;
  update(fn: (value: T) => T): void;
}

export interface SignalOptions<T> {
  /** When a new value counts as a change (default `===`); `false`: always. */
  equals?: false | ((previous: T, next: T) => boolean);
}

/** A value components provide to what they render: `Theme()` reads the nearest one. */
export interface Context<T> extends Accessor<T> {
  Provider: (props: { value: T; children?: JSX.Element }) => JSX.Element;
}

/** A component: a function of its props rendering elements. */
export type Component<P = {}> = (props: P) => JSX.Element;
"#;

pub(crate) const MODULES: &[NativeModule] = &[
    NativeModule {
        name: "nsh",
        exports: NSH,
        types: NSH_TYPES,
    },
    NativeModule {
        name: "nsh:rt",
        exports: dom::RT_EXPORTS,
        types: "",
    },
    NativeModule {
        name: "nsh/device",
        exports: env::DEVICE_EXPORTS,
        types: env::DEVICE_TYPES,
    },
    NativeModule {
        name: "nsh/accessibility",
        exports: env::ACCESSIBILITY_EXPORTS,
        types: env::ACCESSIBILITY_TYPES,
    },
    NativeModule {
        name: "nsh/android",
        exports: platform_modules::ANDROID_EXPORTS,
        types: platform_modules::ANDROID_TYPES,
    },
    NativeModule {
        name: "nsh/ios",
        exports: platform_modules::IOS_EXPORTS,
        types: platform_modules::IOS_TYPES,
    },
    NativeModule {
        name: "nsh/desktop",
        exports: desktop::EXPORTS,
        types: desktop::TYPES,
    },
    NativeModule {
        name: "nsh/webview",
        exports: platform_modules::WEBVIEW_EXPORTS,
        types: platform_modules::WEBVIEW_TYPES,
    },
    NativeModule {
        name: "nsh/video",
        exports: platform_modules::VIDEO_EXPORTS,
        types: platform_modules::VIDEO_TYPES,
    },
    NativeModule {
        name: "nsh/native",
        exports: platform_modules::NATIVE_EXPORTS,
        types: platform_modules::NATIVE_TYPES,
    },
    NativeModule {
        name: "nsh/gesture",
        exports: touch::GESTURE_EXPORTS,
        types: touch::GESTURE_TYPES,
    },
    NativeModule {
        name: "nsh/animated",
        exports: animated::EXPORTS,
        types: animated::TYPES,
    },
    NativeModule {
        name: "nsh/canvas",
        exports: canvas::EXPORTS,
        types: canvas::TYPES,
    },
    NativeModule {
        name: "nsh/list",
        exports: list::EXPORTS,
        types: list::TYPES,
    },
    NativeModule {
        name: "nsh/image",
        exports: IMAGE_EXPORTS,
        types: "",
    },
    NativeModule {
        name: "nsh/keymap",
        exports: keymap::EXPORTS,
        types: keymap::TYPES,
    },
];

/// `nsh/image`: `Image`, the image element as a component carrying the
/// functions of the runtime's module (`getSize`, `prefetch`).
const IMAGE_EXPORTS: &[Item] = &[(
    "Image",
    Export::Value(|ctx| {
        let image = dom::host_component(ctx, "image", true);
        components::add_runtime_functions(ctx, image, "nsh/image", "Image");
        image
    }),
    Ts::Const(
        "An image (a URL or `import logo from \"./logo.png\"`), decoded and cached\nnatively; `Image.getSize(uri)` and `Image.prefetch(uri)` ask about one\nwithout showing it.",
        "((props: ImageProps) => YNode) & ImageModule",
    ),
)];

/// One export of a module, as it is made.
enum Made {
    Own(Export),
    /// A Rust component, with the functions of a module of the same name
    /// (and that module's namespace).
    Component(
        Arc<ComponentSpec>,
        Option<(Arc<ModuleSpec>, Option<&'static str>)>,
    ),
    /// A module object (its namespace for calls).
    Module(Arc<ModuleSpec>, Option<&'static str>),
    /// One function of a flat module.
    Function(Arc<ModuleSpec>, Option<&'static str>, &'static str),
    /// A runtime value a registry exports as it is (`safeArea`).
    Reexport(&'static str),
}

/// The modules a VM (or the bytecode compiler) serves: nsh's own, the
/// runtime's registries and the host's UI sets, and the chunks of a split
/// bundle.
pub(crate) struct ModuleTable {
    registries: Vec<Registry>,
    pub(crate) chunks: RefCell<ChunkCodes>,
}

/// The code of a chunk.
pub(crate) enum ChunkCode {
    Source(String),
    /// From `compile_bytecode`, trusted (as `Host::eval_bytecode`'s).
    Bytecode(Vec<u8>),
}

/// The chunks of a split bundle a VM knows of, by id (module name).
#[derive(Default)]
pub(crate) struct ChunkCodes {
    /// Given, not imported yet: the loader compiles them (and drops the
    /// code).
    pub(crate) code: HashMap<String, ChunkCode>,
    /// Compiled (QuickJS has the modules).
    pub(crate) loaded: HashSet<String>,
}

impl ChunkCodes {
    /// Whether chunk `id` can be imported now.
    pub(crate) fn has(&self, id: &str) -> bool {
        id == nsh_core::js::ENTRY_CHUNK || self.loaded.contains(id) || self.code.contains_key(id)
    }
}

impl ModuleTable {
    pub(crate) fn new(ui_sets: &[Registry]) -> ModuleTable {
        let mut registries: Vec<Registry> = nsh_core::runtime::registries().to_vec();
        registries.extend(ui_sets.iter().filter(|r| r.specifier().is_some()).cloned());
        ModuleTable {
            registries,
            chunks: RefCell::default(),
        }
    }

    /// The exports of module `name`, if it is one.
    fn exports(&self, name: &str) -> Option<Vec<(String, Made)>> {
        let mut out: Vec<(String, Made)> = Vec::new();
        let mut found = false;
        if name == crate::hot::SPECIFIER {
            let own = crate::hot::EXPORTS.iter();
            return Some(own.map(|&(n, e, _)| (n.to_owned(), Made::Own(e))).collect());
        }
        if let Some(m) = MODULES.iter().find(|m| m.name == name) {
            found = true;
            out.extend(
                m.exports
                    .iter()
                    .map(|&(n, e, _)| (n.to_owned(), Made::Own(e))),
            );
        }
        for registry in self
            .registries
            .iter()
            .filter(|r| r.specifier() == Some(name))
        {
            found = true;
            let namespace = registry.namespace();
            let modules = registry.module_specs();
            let specs = registry.specs();
            for spec in &specs {
                let module = modules
                    .iter()
                    .find(|m| m.name == spec.export)
                    .map(|m| (m.clone(), namespace));
                out.push((spec.export.clone(), Made::Component(spec.clone(), module)));
            }
            for m in &modules {
                if specs.iter().any(|s| s.export == m.name) {
                    continue;
                }
                if m.flat {
                    for f in &m.functions {
                        out.push((
                            f.name.to_owned(),
                            Made::Function(m.clone(), namespace, f.name),
                        ));
                    }
                } else {
                    out.push((m.name.to_owned(), Made::Module(m.clone(), namespace)));
                }
            }
            for &value in registry
                .reexports()
                .0
                .iter()
                .chain(registry.native_value_names())
            {
                out.push((value.to_owned(), Made::Reexport(value)));
            }
        }
        // nsh's own export wins over a registry's of the same name (it
        // builds on it: `AccessibilityInfo`).
        let mut seen = std::collections::HashSet::new();
        out.retain(|(name, _)| seen.insert(name.clone()));
        found.then_some(out)
    }
}

/// Makes runtime `rt` load the modules of `table` (which outlives it).
pub(crate) fn set_loader(rt: *mut qjs::JSRuntime, table: *const ModuleTable) {
    // SAFETY: a plain C loader; the default name normalization keeps bare
    // specifiers as they are.
    unsafe {
        qjs::JS_SetModuleLoaderFunc(rt, None, Some(load), table as *mut c_void);
    }
}

unsafe extern "C" fn load(
    ctx: *mut JsContext,
    name: *const c_char,
    opaque: *mut c_void,
) -> *mut qjs::JSModuleDef {
    // SAFETY: QuickJS passes a NUL-terminated module name; the table is the
    // one `set_loader` was given.
    let (cname, table) = unsafe { (CStr::from_ptr(name), &*(opaque as *const ModuleTable)) };
    let exports = cname.to_str().ok().and_then(|n| table.exports(n));
    // Taken first: compiling it loads its imports, here again.
    let chunk = match (&exports, cname.to_str()) {
        (None, Ok(id)) => table.chunks.borrow_mut().code.remove(id).map(|c| (id, c)),
        _ => None,
    };
    if let Some((id, code)) = chunk {
        // SAFETY: a live context; `code` is trusted as the host's API
        // requires.
        let m = unsafe { compile_chunk(ctx, name, code) };
        if !m.is_null() {
            table.chunks.borrow_mut().loaded.insert(id.to_owned());
        }
        return m;
    }
    let Some(exports) = exports else {
        // SAFETY: throws on a live context; `%s` keeps the name literal.
        unsafe {
            qjs::JS_ThrowReferenceError(
                ctx,
                c"could not load module '%s' (only nsh's own modules can be imported at run time)"
                    .as_ptr(),
                name,
            )
        };
        return std::ptr::null_mut();
    };
    // SAFETY: a new C module declaring its exports (set by `init`).
    unsafe {
        let m = qjs::JS_NewCModule(ctx, name, Some(init));
        if m.is_null() {
            return m;
        }
        for (export, _) in &exports {
            let export = CString::new(export.as_str()).unwrap_or_default();
            qjs::JS_AddModuleExport(ctx, m, export.as_ptr());
        }
        m
    }
}

/// Compiles chunk `name` (its imports resolved by the loader, recursively);
/// null with the exception pending when it does not compile.
///
/// # Safety
///
/// `ctx` is live; bytecode is trusted.
unsafe fn compile_chunk(
    ctx: *mut JsContext,
    name: *const c_char,
    code: ChunkCode,
) -> *mut qjs::JSModuleDef {
    // SAFETY: per the contract; QuickJS reads source up to the NUL it
    // requires after it.
    let module = unsafe {
        match code {
            ChunkCode::Source(mut source) => {
                let len = source.len();
                source.push('\0');
                qjs::JS_Eval(
                    ctx,
                    source.as_ptr() as *const c_char,
                    len as _,
                    name,
                    (qjs::JS_EVAL_TYPE_MODULE | qjs::JS_EVAL_FLAG_COMPILE_ONLY) as c_int,
                )
            }
            ChunkCode::Bytecode(bytes) => qjs::JS_ReadObject(
                ctx,
                bytes.as_ptr(),
                bytes.len() as _,
                qjs::JS_READ_OBJ_BYTECODE as c_int,
            ),
        }
    };
    // SAFETY: inspecting and freeing the value just made.
    unsafe {
        if qjs::JS_IsException(module) {
            return std::ptr::null_mut();
        }
        if qjs::JS_VALUE_GET_TAG(module) != qjs::JS_TAG_MODULE {
            qjs::JS_FreeValue(ctx, module);
            qjs::JS_ThrowTypeError(ctx, c"chunk '%s' is not a module".as_ptr(), name);
            return std::ptr::null_mut();
        }
        let m = qjs::JS_VALUE_GET_PTR(module) as *mut qjs::JSModuleDef;
        // The context keeps the module: the value is one more reference.
        qjs::JS_FreeValue(ctx, module);
        m
    }
}

unsafe extern "C" fn init(ctx: *mut JsContext, m: *mut qjs::JSModuleDef) -> c_int {
    // SAFETY: the module's name atom, freed below; exports are new values
    // the module takes; the state's table lives as long as the context.
    unsafe {
        let atom = qjs::JS_GetModuleName(ctx, m);
        let name = qjs::JS_AtomToCStringLen(ctx, std::ptr::null_mut(), atom);
        qjs::JS_FreeAtom(ctx, atom);
        if name.is_null() {
            return -1;
        }
        let state = crate::state::state(ctx);
        let exports = CStr::from_ptr(name)
            .to_str()
            .ok()
            .and_then(|n| (*state).modules.exports(n));
        qjs::JS_FreeCString(ctx, name);
        let Some(exports) = exports else {
            return -1;
        };
        for (export, made) in exports {
            let value = match made {
                Made::Own(Export::Function(f, length)) => {
                    let cname = CString::new(export.as_str()).unwrap_or_default();
                    reactive::new_function(ctx, &cname, f, length)
                }
                Made::Own(Export::Value(make)) => make(ctx),
                Made::Component(spec, module) => components::component(ctx, spec, module),
                Made::Module(spec, namespace) => components::module(ctx, &spec, namespace),
                Made::Function(spec, namespace, f) => {
                    components::module_function(ctx, &spec, namespace, f)
                }
                Made::Reexport(value) => env::reexport(ctx, value),
            };
            if qjs::JS_IsException(value) {
                return -1;
            }
            let cname = CString::new(export.as_str()).unwrap_or_default();
            if qjs::JS_SetModuleExport(ctx, m, cname.as_ptr(), value) != 0 {
                return -1;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    #[test]
    fn nshs_own_modules_are_the_ones_the_bundler_knows() {
        let names: Vec<&str> = super::MODULES.iter().map(|m| m.name).collect();
        assert_eq!(names, nsh_core::js::NATIVE_MODULES);
    }
}
