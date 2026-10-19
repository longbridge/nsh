//! Import aliases and generated declarations at the public application boundary.

use std::collections::BTreeSet;

use nsh_js::{Host, type_definitions};

use super::run;

const REFERENCE_MODULES: &[&str] = &[
    "buffer",
    "console",
    "crypto",
    "fs/promises",
    "net",
    "os",
    "path",
    "process",
    "url",
    "websocket",
    "zlib",
];

#[test]
fn standard_catalog_is_exact_and_node_aliases_are_unavailable() {
    assert_eq!(nsh_core::js::STANDARD_MODULES, REFERENCE_MODULES);
    for name in REFERENCE_MODULES {
        let host = Host::new().unwrap();
        assert!(
            host.eval_source(&format!("import 'node:{name}';")).is_err(),
            "node:{name} is absent in reference runtime"
        );
    }
}

#[test]
fn private_unsupported_and_noncanonical_imports_do_not_resolve() {
    for name in [
        "node:vm",
        "events",
        "assert",
        "util",
        "string_decoder",
        "fs",
        "node:node:buffer",
        "buffer/",
        "node:buffer/",
        "node:crypto/private",
        "@nsh/llrt-types/buffer",
        "llrt:internal",
    ] {
        let host = Host::new().expect("create Host");
        assert!(
            host.eval_source(&format!("import * as value from '{name}';"))
                .is_err(),
            "unsupported import {name} must not escape the runtime registry"
        );
    }
}

fn projected_values(public: &str, name: &str) -> BTreeSet<String> {
    let marker = format!("declare module \"{name}\" {{");
    let body = public
        .split_once(&marker)
        .expect("public standard module")
        .1;
    let body = body.split("declare module ").next().unwrap();
    let mut names = BTreeSet::new();
    for line in body.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("export { ") {
            let bindings = rest.split_once(" }").expect("export bindings").0;
            for binding in bindings.split(',').map(str::trim) {
                names.insert(binding.split(" as ").last().unwrap().to_owned());
            }
        } else if let Some(rest) = line.strip_prefix("export function ") {
            names.insert(rest.split(['(', '<']).next().unwrap().to_owned());
        } else if let Some(rest) = line.strip_prefix("export const ") {
            names.insert(rest.split(':').next().unwrap().to_owned());
        } else if line.starts_with("export default ") {
            names.insert("default".to_owned());
        }
    }
    names
}

#[test]
fn every_public_standard_value_is_declared_and_every_declared_value_exists() {
    let files = type_definitions(&[]);
    let public = &files
        .iter()
        .find(|(path, _)| path == "standard/index.d.ts")
        .expect("standard declaration index")
        .1;
    let names = REFERENCE_MODULES
        .iter()
        .copied()
        .filter(|name| *name != "websocket" || cfg!(not(target_arch = "wasm32")));
    assert!(!public.contains("declare module \"node:"));
    for removed in ["assert", "events", "util", "string_decoder", "fs"] {
        assert!(!public.contains(&format!("declare module \"{removed}\"")));
    }
    let declared_modules: BTreeSet<_> = public
        .lines()
        .filter_map(|line| {
            line.strip_prefix("declare module \"")
                .and_then(|rest| rest.split('"').next())
        })
        .collect();
    let expected_modules: BTreeSet<_> = REFERENCE_MODULES
        .iter()
        .copied()
        .filter(|name| *name != "websocket" || cfg!(not(target_arch = "wasm32")))
        .collect();
    assert_eq!(
        declared_modules, expected_modules,
        "public declaration catalog must be exact"
    );
    for name in names {
        let actual: String = serde_json::from_str(&run(&format!(
            "import * as standard from '{name}';\nglobalThis.out = Object.keys(standard).sort().join(',');"
        ))).unwrap();
        let declared = projected_values(public, name)
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(actual, declared, "{name} runtime/declaration export parity");
    }
}
