//! The Android resources (`res/`) of the AARs a package carries: where they
//! go in `aapt2 link` and the `R` classes their code reads them through.
//!
//! A library's resources are overlays under NSH's own: the package's own
//! resources win over the app's libraries, which win over its crates'.
//! A library's code is compiled against `R` fields its build had no final
//! values for; its `R.txt` lists the ones it uses, and the package gives
//! them the ids `aapt2 link` settled on.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fmt::Write as _,
    path::{Path, PathBuf},
};

/// A symbol of an `R.txt`, by (type, name): `string hello`,
/// `styleable My_View`.
pub(super) type Symbols = BTreeMap<(String, String), Symbol>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Symbol {
    /// `int[]` (a styleable's attributes) rather than `int`.
    pub(super) array: bool,
    /// As `R.txt` has it: `0x7f010000`, `0`, `{ 0x0101014f, 0x7f010000 }`.
    pub(super) value: String,
}

/// Reads an `R.txt`: one `<int|int[]> <type> <name> <value>` a line.
pub(super) fn parse_symbols(text: &str) -> Result<Symbols, String> {
    let mut out = Symbols::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(4, ' ');
        let (Some(kind), Some(kind_of), Some(name), Some(value)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(format!("line {}: `{line}` is no symbol", number + 1));
        };
        let array = match kind {
            "int" => false,
            "int[]" => true,
            _ => return Err(format!("line {}: `{kind}` is no symbol kind", number + 1)),
        };
        if !identifier(kind_of) || !identifier(name) {
            return Err(format!(
                "line {}: `{kind_of} {name}` is no Java name",
                number + 1
            ));
        }
        out.insert(
            (kind_of.to_owned(), name.to_owned()),
            Symbol {
                array,
                value: value.trim().to_owned(),
            },
        );
    }
    Ok(out)
}

/// `R.java` of the library `package`: each of `wanted` (its `R.txt`) with
/// the value `linked` (what `aapt2 link` wrote) gives it. A symbol the
/// package does not have is an error naming it: no library listed defines
/// that resource.
pub(super) fn r_java(package: &str, wanted: &Symbols, linked: &Symbols) -> Result<String, String> {
    if !package.split('.').all(identifier) {
        return Err(format!("`{package}` is no Java package"));
    }
    let mut out = format!("package {package};\n\npublic final class R {{\n");
    let mut open: Option<&str> = None;
    for ((kind_of, name), symbol) in wanted {
        let Some(have) = linked.get(&(kind_of.clone(), name.clone())) else {
            return Err(format!(
                "its code uses `R.{kind_of}.{name}`, which no Android library listed defines"
            ));
        };
        if have.array != symbol.array {
            return Err(format!(
                "its code takes `R.{kind_of}.{name}` as {}, the package has {}",
                java_type(symbol.array),
                java_type(have.array)
            ));
        }
        if open != Some(kind_of) {
            if open.is_some() {
                out.push_str("    }\n");
            }
            let _ = writeln!(out, "    public static final class {kind_of} {{");
            open = Some(kind_of);
        }
        let _ = writeln!(
            out,
            "        public static final {} {name} = {};",
            java_type(have.array),
            have.value
        );
    }
    if open.is_some() {
        out.push_str("    }\n");
    }
    out.push_str("}\n");
    Ok(out)
}

fn java_type(array: bool) -> &'static str {
    if array { "int[]" } else { "int" }
}

/// A Java identifier as `R.txt` names types and fields.
fn identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// The resources `aapt2 link` takes: `own` (the package's) as they are
/// when no library has any; else every one an overlay (`-R`, of which a
/// later one wins), `libraries` lowest first and `own` last.
pub(super) fn link_inputs(own: &[PathBuf], libraries: &[PathBuf]) -> Vec<OsString> {
    if libraries.is_empty() {
        return own.iter().map(|p| p.clone().into_os_string()).collect();
    }
    let mut out = Vec::new();
    for path in libraries.iter().chain(own) {
        out.push("-R".into());
        out.push(path.clone().into_os_string());
    }
    out.push("--auto-add-overlay".into());
    out
}

/// `libraries` (who lists each: `None` for the app, else its crate),
/// lowest priority first, as [`link_inputs`] takes them: the crates' by
/// crate name, then the app's; of one, a later listed below an earlier.
pub(super) fn lowest_first<T>(mut libraries: Vec<(Option<String>, T)>) -> Vec<T> {
    // Stable: the listed order stays within one crate.
    libraries.sort_by(|(a, _), (b, _)| (a.is_some(), a).cmp(&(b.is_some(), b)));
    libraries.into_iter().rev().map(|(_, t)| t).collect()
}

/// The file of `package`'s `R.java` under `dir`.
pub(super) fn r_java_path(dir: &Path, package: &str) -> PathBuf {
    let mut path = dir.to_owned();
    path.extend(package.split('.'));
    path.join("R.java")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_are_read_by_type_and_name() {
        let symbols = parse_symbols(
            "int attr tint 0x7f010000\n\
             int[] styleable My_View { 0x0101014f, 0x7f010000 }\n\
             \n\
             int styleable My_View_tint 1\n",
        )
        .unwrap();
        let get =
            |kind_of: &str, name: &str| symbols[&(kind_of.to_owned(), name.to_owned())].clone();
        assert_eq!(
            get("attr", "tint"),
            Symbol {
                array: false,
                value: "0x7f010000".into()
            }
        );
        assert_eq!(
            get("styleable", "My_View"),
            Symbol {
                array: true,
                value: "{ 0x0101014f, 0x7f010000 }".into()
            }
        );
        assert_eq!(get("styleable", "My_View_tint").value, "1");
        for (text, error) in [
            ("int attr tint", "line 1: `int attr tint` is no symbol"),
            ("long attr tint 1", "line 1: `long` is no symbol kind"),
            (
                "int string a;static{} 1",
                "line 1: `string a;static{}` is no Java name",
            ),
        ] {
            assert_eq!(parse_symbols(text).unwrap_err(), error);
        }
    }

    #[test]
    fn a_librarys_r_class_has_the_fields_it_lists_with_the_ids_the_package_gave() {
        // `aapt2 link --output-text-symbols`: every resource of the package,
        // styleable indices in the order of the attributes' ids.
        let linked = parse_symbols(
            "int attr tint 0x7f010000\n\
             int id root 0x7f020000\n\
             int layout main 0x7f040000\n\
             int string hello 0x7f050000\n\
             int string other 0x7f050001\n\
             int[] styleable My_View { 0x0101014f, 0x7f010000 }\n\
             int styleable My_View_android_text 0\n\
             int styleable My_View_tint 1\n",
        )
        .unwrap();
        // The library's own `R.txt`: its build's values, which do not count.
        let wanted = parse_symbols(
            "int string hello 0x7f0b0001\n\
             int layout main 0x7f080000\n\
             int[] styleable My_View { 0x7f030001, 0x0101014f }\n\
             int styleable My_View_tint 0\n\
             int styleable My_View_android_text 1\n",
        )
        .unwrap();
        assert_eq!(
            r_java("com.example.lib", &wanted, &linked).unwrap(),
            "package com.example.lib;

public final class R {
    public static final class layout {
        public static final int main = 0x7f040000;
    }
    public static final class string {
        public static final int hello = 0x7f050000;
    }
    public static final class styleable {
        public static final int[] My_View = { 0x0101014f, 0x7f010000 };
        public static final int My_View_android_text = 0;
        public static final int My_View_tint = 1;
    }
}
"
        );
        assert_eq!(
            r_java("com.example.lib", &Symbols::new(), &linked).unwrap(),
            "package com.example.lib;\n\npublic final class R {\n}\n"
        );
        let missing = parse_symbols("int string gone 0x7f0b0002").unwrap();
        assert_eq!(
            r_java("com.example.lib", &missing, &linked).unwrap_err(),
            "its code uses `R.string.gone`, which no Android library listed defines"
        );
        let kind = parse_symbols("int styleable My_View 0").unwrap();
        assert_eq!(
            r_java("com.example.lib", &kind, &linked).unwrap_err(),
            "its code takes `R.styleable.My_View` as int, the package has int[]"
        );
        assert_eq!(
            r_java("com.example-lib", &wanted, &linked).unwrap_err(),
            "`com.example-lib` is no Java package"
        );
        assert_eq!(
            r_java_path(Path::new("src"), "com.example.lib"),
            Path::new("src/com/example/lib/R.java")
        );
    }

    #[test]
    fn libraries_are_overlays_under_the_packages_own_resources() {
        let paths = |names: &[&str]| -> Vec<PathBuf> { names.iter().map(PathBuf::from).collect() };
        let own = paths(&["icon.flat", "compiled-app.zip"]);
        // Without libraries' resources, `aapt2 link` takes the same
        // arguments as before.
        assert_eq!(
            link_inputs(&own, &[]),
            [OsString::from("icon.flat"), "compiled-app.zip".into()]
        );
        // `aapt2 link`: of two overlays with one resource, the later wins.
        let libraries = lowest_first(vec![
            (None, "app-first.zip"),
            (Some("zeta".to_owned()), "zeta.zip"),
            (None, "app-second.zip"),
            (Some("alpha".to_owned()), "alpha-first.zip"),
            (Some("alpha".to_owned()), "alpha-second.zip"),
        ]);
        assert_eq!(
            libraries,
            [
                "zeta.zip",
                "alpha-second.zip",
                "alpha-first.zip",
                "app-second.zip",
                "app-first.zip",
            ]
        );
        let args: Vec<String> = link_inputs(&own, &paths(&libraries))
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args.join(" "),
            "-R zeta.zip -R alpha-second.zip -R alpha-first.zip -R app-second.zip \
             -R app-first.zip -R icon.flat -R compiled-app.zip --auto-add-overlay"
        );
    }
}
