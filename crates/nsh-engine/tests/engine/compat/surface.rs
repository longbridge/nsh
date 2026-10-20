//! What this checkout offers apps, as the text files `api/` records:
//!
//! - `runtime.txt`: every module's exports as the runtime makes them (the
//!   module `nsh:rt` compiled JSX calls included) and the globals, one line
//!   per name with its kind, members of objects and classes included. This
//!   is what compiled bundles link against: a name missing from it is an
//!   app that no longer starts.
//! - `types/`: the TypeScript declarations of `.nsh/types`, what the
//!   sources of apps are checked against.
//!
//! Only public API of `nsh-js` is used, so the same file makes the
//! record of an older checkout too.

use std::fmt::Write as _;

/// How deep [`WALK`] follows objects: a module's export (or a global), its
/// members, theirs.
const DEPTH: u32 = 3;

// quickjs-stdlib owns these modules' declarations and behavior. NSH tests
// their integration and sandbox boundary, without duplicating its API suite.
const UPSTREAM_MODULES: &[&str] = &["buffer", "crypto", "path", "url", "zlib"];
const UPSTREAM_GLOBALS: &[&str] = &["Buffer", "crypto", "Crypto", "CryptoKey", "SubtleCrypto"];

/// Lists `value`'s kind and members as `path kind` lines. Property
/// descriptors are read, never getters run (a prototype's getters throw
/// without an instance); functions list their statics and prototype.
const WALK: &str = r#"
const lines = [];
// The objects being walked: cycles end, shared objects are listed at
// every path that reaches them.
const walking = new Set();
function kind(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  if (typeof v === "function") return /^class[\s{]/.test(Function.prototype.toString.call(v)) ? "class" : "function";
  return typeof v;
}
const SKIP_STATIC = new Set(["length", "name", "prototype", "arguments", "caller"]);
function members(path, v, depth) {
  if (depth <= 0 || v === null || (typeof v !== "object" && typeof v !== "function")) return;
  if (walking.has(v)) return;
  walking.add(v);
  for (const key of Reflect.ownKeys(v)) {
    const name = typeof key === "symbol" ? `[${key.description}]` : key;
    if (typeof v === "function" && SKIP_STATIC.has(name)) continue;
    if (name.startsWith("__")) continue;
    const d = Reflect.getOwnPropertyDescriptor(v, key);
    const at = `${path}.${name}`;
    if (d.get || d.set) {
      lines.push(`${at} accessor`);
    } else {
      lines.push(`${at} ${kind(d.value)}`);
      members(at, d.value, depth - 1);
    }
  }
  if (typeof v === "function" && v.prototype && typeof v.prototype === "object") {
    members(`${path}#`, v.prototype, depth - 1);
  }
  walking.delete(v);
}
function walk(path, v, depth) {
  lines.push(`${path} ${kind(v)}`);
  members(path, v, depth);
}
"#;

/// The specifiers apps import: `nsh:rt` (what compiled JSX calls) and
/// every module the declarations describe.
fn specifiers(types: &[(String, String)]) -> Vec<String> {
    let mut out = vec!["nsh:rt".to_owned()];
    for (path, text) in types {
        let Some(module) = path.strip_suffix(".d.ts") else {
            continue;
        };
        let specifier = match module {
            "nsh/index" => "nsh",
            "nsh/types" | "nsh/jsx-runtime" | "nsh/globals" => continue,
            // NSH-owned standard adapters use ambient modules. Upstream
            // declarations are excluded from this compatibility record.
            "standard/index" => {
                out.extend(
                    text.lines()
                        .filter_map(|l| l.strip_prefix("declare module \""))
                        .filter_map(|l| l.split_once('"'))
                        .map(|(name, _)| name.to_owned()),
                );
                continue;
            }
            "standard/llrt-sources" => continue,
            s => s,
        };
        out.push(specifier.to_owned());
    }
    out.sort();
    out
}

/// Runs `body` (after [`WALK`]) in `host` and returns the text it passes to
/// `text()`.
fn run(host: &nsh_js::Host, imports: &str, body: &str) -> String {
    host.eval_source(&format!(
        "import {{ text as __text }} from \"nsh:rt\";\n{imports}\n{WALK}\n{body}\n__text(lines.join(\"\\n\"));"
    ))
    .unwrap_or_else(|e| panic!("{e}\n{imports}"));
    let mut ops = Vec::new();
    host.take_ops(&mut ops);
    ops.into_iter()
        .find_map(|op| match op {
            nsh_core::Op::CreateText { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .expect("the walk's text")
}

/// `runtime.txt` of this checkout.
pub(crate) fn runtime() -> String {
    let types = types();
    // Released apps run hardened: what hardening removes is not theirs.
    let host = nsh_js::Host::with_options(nsh_js::HostOptions {
        ui_sets: vec![nsh_ui_mobile::registry()],
        harden: true,
        ..Default::default()
    })
    .unwrap();
    let mut out = String::from(
        "# What apps link against: each module's exports and the globals,\n\
         # as `path kind` (`a.b`: a member, `A#.m`: a prototype's).\n\
         # Generated by the engine's `compat` tests: do not edit.\n",
    );
    for specifier in specifiers(&types)
        .into_iter()
        .chain(UPSTREAM_MODULES.iter().map(|s| (*s).to_owned()))
    {
        let _ = writeln!(out, "\n[{specifier}]");
        if UPSTREAM_MODULES.contains(&specifier.as_str()) {
            out.push_str("# API and compatibility maintained by quickjs-stdlib.\n");
            continue;
        }
        let text = run(
            &host,
            &format!("import * as __m from \"{specifier}\";"),
            &format!(
                "for (const k of Object.keys(__m).sort()) walk(k, __m[k], {});",
                DEPTH - 1
            ),
        );
        sorted(&mut out, &text);
    }
    out.push_str("\n[globals]\n");
    let text = run(
        &host,
        "",
        &format!(
            "for (const k of Object.getOwnPropertyNames(globalThis).sort()) {{ if (!k.startsWith(\"__\") && k !== \"globalThis\" && !{:?}.includes(k)) walk(k, globalThis[k], {}); }}",
            UPSTREAM_GLOBALS,
            DEPTH - 1
        ),
    );
    sorted(&mut out, &text);
    out
}

fn sorted(out: &mut String, text: &str) {
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    lines.sort_unstable();
    lines.dedup();
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
}

/// The files of `types/` of this checkout: `.nsh/types` without the files
/// the bundler adds (asset imports) and the standard library's upstream
/// declarations, whose compatibility quickjs-stdlib owns.
pub(crate) fn types() -> Vec<(String, String)> {
    let mut files = nsh_js::type_definitions(&[nsh_ui_mobile::registry().into()]);
    files.retain(|(path, _)| path != "standard/llrt-sources.d.ts");
    for (path, text) in &mut files {
        if path == "standard/index.d.ts" {
            // The console adapter is NSH-owned; the preceding ambient
            // modules project quickjs-stdlib's declarations unchanged.
            let start = text
                .find("declare module \"console\"")
                .expect("console adapter");
            let body = &text[start..];
            *text = body[body.find('{').unwrap() + 1..body.rfind('}').unwrap()]
                .trim()
                .to_owned()
                + "\n";
            *path = "console.d.ts".into();
        } else if path == "nsh/globals.d.ts" {
            *text = text
                .lines()
                .filter(|line| {
                    !UPSTREAM_GLOBALS.iter().any(|name| {
                        line.starts_with(&format!("declare var {name}:"))
                            || line.starts_with(&format!("declare const {name}:"))
                            || line.starts_with(&format!("type {name} ="))
                    })
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
        }
    }
    files.sort();
    files
}

#[test]
fn upstream_standard_api_is_owned_upstream() {
    let files = types();
    assert!(
        !files
            .iter()
            .any(|(path, _)| path == "standard/llrt-sources.d.ts")
    );
    let standard = &files
        .iter()
        .find(|(path, _)| path == "console.d.ts")
        .unwrap()
        .1;
    assert!(standard.contains("export default console"));
    assert!(!standard.contains("@nsh/llrt-types/"));
    let globals = &files
        .iter()
        .find(|(path, _)| path == "nsh/globals.d.ts")
        .unwrap()
        .1;
    assert!(globals.contains("declare var process:"));
    assert!(!globals.contains("declare var Buffer:"));
    let runtime = runtime();
    for name in UPSTREAM_MODULES {
        assert!(runtime.contains(&format!(
            "[{name}]\n# API and compatibility maintained by quickjs-stdlib."
        )));
    }
    assert!(!runtime.lines().any(|line| line.starts_with("Buffer#.")));
    assert!(runtime.contains("[process]\nChildProcess function"));
}
