//! Whether this checkout still offers what a baseline (`api/<version>/`)
//! did, by the rules of docs/dev/compatibility.md:
//!
//! - nothing goes: a module, an export, a member of an object, a class or
//!   an interface, a global;
//! - a runtime value keeps its kind (a function stays callable);
//! - a declaration keeps its text (comments, spacing and parameter names
//!   aside), or widens it: a union gains alternatives, a function gains
//!   overloads or trailing optional parameters, a parameter becomes
//!   optional, a function's or method's `void` result becomes a value (a
//!   callback an app passes in keeps its `void`);
//! - what apps pass in (an interface named `…Props`, `…Style`, `…Options`
//!   or `…Config`) gains optional members only.

use std::collections::BTreeMap;

/// What changed incompatibly between `base` and `now` (`runtime.txt`).
pub(crate) fn runtime(base: &str, now: &str) -> Vec<String> {
    let base = runtime_entries(base);
    let now = runtime_entries(now);
    let mut out = Vec::new();
    for ((section, path), kind) in &base {
        match now.get(&(section.clone(), path.clone())) {
            None => out.push(format!("[{section}] {path}: removed")),
            Some(k) if k != kind && !same_kind(k, kind) => {
                out.push(format!("[{section}] {path}: was {kind}, now {k}"))
            }
            Some(_) => {}
        }
    }
    out
}

/// A native class and a JS class are both functions apps call with `new`.
fn same_kind(a: &str, b: &str) -> bool {
    matches!((a, b), ("function", "class") | ("class", "function"))
}

fn runtime_entries(text: &str) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    let mut section = String::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_owned();
            continue;
        }
        let (path, kind) = line.rsplit_once(' ').expect("`path kind`");
        out.insert((section.clone(), path.to_owned()), kind.to_owned());
    }
    out
}

/// What changed incompatibly between the declaration files `base` and
/// `now` (path, contents).
pub(crate) fn types(base: &[(String, String)], now: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, text) in base {
        let Some((_, current)) = now.iter().find(|(p, _)| p == path) else {
            out.push(format!("{path}: removed"));
            continue;
        };
        let old = declarations(text);
        let new = declarations(current);
        for (name, old) in &old {
            let at = format!("{path}: {name}");
            let Some(new) = new.get(name) else {
                out.push(format!("{at}: removed"));
                continue;
            };
            if old.kind != new.kind {
                out.push(format!("{at}: was a {}, now a {}", old.kind, new.kind));
                continue;
            }
            if !old.header.is_empty() && !header_compatible(&old.header, &new.header) {
                out.push(format!("{at}: was `{}`, now `{}`", old.header, new.header));
            }
            for (member, texts) in &old.members {
                let Some(now) = new.members.get(member) else {
                    out.push(format!("{at}.{member}: removed"));
                    continue;
                };
                for text in texts {
                    if !now.iter().any(|n| compatible(text, n)) {
                        out.push(format!(
                            "{at}.{member}: was `{text}`, now `{}`",
                            now.join("` / `")
                        ));
                    }
                }
            }
            if old.kind == "interface" && takes_input(name) {
                for (member, texts) in &new.members {
                    if !old.members.contains_key(member) && texts.iter().any(|t| required(t)) {
                        out.push(format!("{at}.{member}: added as required (`{}`)", texts[0]));
                    }
                }
            }
        }
    }
    out
}

/// An interface whose objects apps make and pass in.
fn takes_input(name: &str) -> bool {
    ["Props", "Style", "Options", "Config"]
        .iter()
        .any(|s| name.ends_with(s))
}

/// A property without `?` (methods and signatures aside).
fn required(member: &str) -> bool {
    let Some(colon) = top_level(member, ':') else {
        return false;
    };
    let before = &member[..colon];
    !before.ends_with('?') && !before.contains('(') && !before.contains('[')
}

/// A container's header (`interface A<T> extends B, C`) may gain bases.
fn header_compatible(old: &str, new: &str) -> bool {
    if old == new {
        return true;
    }
    let split = |h: &str| -> (String, Vec<String>) {
        match h.split_once(" extends ") {
            Some((head, bases)) => (
                head.to_owned(),
                split_top(bases, ',')
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
            None => (h.to_owned(), Vec::new()),
        }
    };
    let (old_head, old_bases) = split(old);
    let (new_head, new_bases) = split(new);
    old_head == new_head && old_bases.iter().all(|b| new_bases.contains(b))
}

/// Whether `new` (a declaration or member, normalized) accepts all `old`
/// did: the same text, a union with more alternatives, or a signature with
/// trailing optional parameters or parameters that widened.
pub(crate) fn compatible(old: &str, new: &str) -> bool {
    let (old, new) = (&erase_names(old), &erase_names(new));
    old == new || widened(old, new) || void_returns_filled(old, new)
}

/// [`compatible`] on texts without parameter names.
fn widened(old: &str, new: &str) -> bool {
    if old == new {
        return true;
    }
    // A function or method signature (not a function type: a callback an
    // app passes in has to accept what it is given): the same before the
    // parameters and after them.
    if let (Some(o), Some(n)) = (top_level(old, '('), top_level(new, '('))
        && old[..o] == new[..n]
        && old[..o].ends_with(|c: char| is_ident(c) || c == '>' || c == '?')
        && let (Some(oc), Some(nc)) = (matching(old, o), matching(new, n))
        && old[oc..] == new[nc..]
    {
        let old_params = split_top(&old[o + 1..oc], ',');
        let new_params = split_top(&new[n + 1..nc], ',');
        return new_params.len() >= old_params.len()
            && old_params
                .iter()
                .zip(&new_params)
                .all(|(o, n)| parameter_compatible(o, n))
            && new_params[old_params.len()..]
                .iter()
                .all(|p| p.starts_with("...") || optional(p));
    }
    union_widened(old, new)
}

fn parameter_compatible(old: &str, new: &str) -> bool {
    if widened(old, new) {
        return true;
    }
    // `a: T` → `a?: T` (or a wider `T`).
    match (top_level(old, ':'), top_level(new, ':')) {
        (Some(o), Some(n)) => {
            let (old_name, new_name) = (&old[..o], &new[..n]);
            (new_name == old_name || new_name.strip_suffix('?') == Some(old_name))
                && type_widened(&old[o + 1..], &new[n + 1..])
        }
        _ => false,
    }
}

/// `old` with each method's or function's `): void` given a value in
/// `new` (`): Promise<void>`), all else the same.
fn void_returns_filled(old: &str, new: &str) -> bool {
    const VOID: &str = "): void";
    fn matches(parts: &[&str], new: &str) -> bool {
        let [first, rest @ ..] = parts else {
            return new.is_empty();
        };
        let Some(after) = new.strip_prefix(first) else {
            return false;
        };
        if rest.is_empty() {
            return after.is_empty();
        }
        let Some(after) = after.strip_prefix("): ") else {
            return false;
        };
        // The result type: the shortest balanced text the rest follows.
        (1..=after.len())
            .filter(|&end| after.is_char_boundary(end))
            .any(|end| balanced(&after[..end]) && matches(rest, &after[end..]))
    }
    let parts: Vec<&str> = old.split(VOID).collect();
    parts.len() > 1 && matches(&parts, new)
}

fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// `text` with every parameter named `_` (`(a?: T, ...b: U)` →
/// `(_?: T, ..._: U)`): the names are not part of a type.
fn erase_names(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(open) = text[at..].find('(').map(|i| at + i) {
        out.push_str(&text[at..=open]);
        let Some(close) = matching(text, open) else {
            out.push_str(&text[open + 1..]);
            return out;
        };
        let params: Vec<String> = split_top(&text[open + 1..close], ',')
            .into_iter()
            .map(|p| {
                let (rest, spread) = match p.strip_prefix("...") {
                    Some(r) => (r, "..."),
                    None => (p, ""),
                };
                let name: String = rest.chars().take_while(|c| is_ident(*c)).collect();
                let after = &rest[name.len()..];
                let after = after.strip_prefix('?').map_or(after, |a| a);
                let optional = if rest[name.len()..].starts_with('?') {
                    "?"
                } else {
                    ""
                };
                match after.strip_prefix(": ") {
                    Some(ty) if !name.is_empty() => {
                        format!("{spread}_{optional}: {}", erase_names(ty))
                    }
                    _ => erase_names(p),
                }
            })
            .collect();
        out.push_str(&params.join(", "));
        out.push(')');
        at = close + 1;
    }
    out.push_str(&text[at..]);
    out
}

fn optional(param: &str) -> bool {
    top_level(param, ':').is_some_and(|c| param[..c].ends_with('?'))
}

/// `name: A | B` → `name: A | B | C`, `type T = A | B` → `… | C`.
fn union_widened(old: &str, new: &str) -> bool {
    let sep = |s: &str| match (top_level(s, '='), top_level(s, ':')) {
        (Some(e), Some(c)) => Some(e.min(c)),
        (e, c) => e.or(c),
    };
    let (Some(o), Some(n)) = (sep(old), sep(new)) else {
        return false;
    };
    old[..=o] == new[..=n] && type_widened(&old[o + 1..], &new[n + 1..])
}

/// The same type, or a union of more alternatives.
fn type_widened(old: &str, new: &str) -> bool {
    let alternatives = |s: &str| -> Vec<String> {
        split_top(s.trim().trim_start_matches('|'), '|')
            .into_iter()
            .map(str::to_owned)
            .collect()
    };
    let (old, new) = (alternatives(old), alternatives(new));
    !old.is_empty() && old.iter().all(|a| new.contains(a))
}

/// A top-level declaration of a file.
#[derive(Debug)]
pub(crate) struct Declaration {
    pub(crate) kind: &'static str,
    /// A container's head (`interface A<T> extends B`), or empty.
    pub(crate) header: String,
    /// Members by name (overloads: several texts); a declaration that is
    /// not a container has itself as its only member, `""`.
    pub(crate) members: BTreeMap<String, Vec<String>>,
}

/// The declarations of a `.d.ts` file by name.
pub(crate) fn declarations(text: &str) -> BTreeMap<String, Declaration> {
    let code = strip_comments(text);
    let mut out: BTreeMap<String, Declaration> = BTreeMap::new();
    for statement in statements(&code) {
        let Some((kind, name)) = head(&statement) else {
            continue;
        };
        // A value and a type of one name (`const X` and `interface X`).
        let key = match out.get(&name) {
            Some(d) if d.kind != kind => format!("{name} ({kind})"),
            _ => name,
        };
        let entry = out.entry(key).or_insert_with(|| Declaration {
            kind,
            header: String::new(),
            members: BTreeMap::new(),
        });
        match kind {
            "interface" | "class" | "namespace" | "enum" => {
                let (header, body) = container(&statement).expect("a body");
                entry.header = header;
                let members = if kind == "namespace" {
                    statements(&body)
                        .into_iter()
                        .map(|s| {
                            let name = head(&s).map_or_else(|| s.clone(), |(_, n)| n);
                            (name, s)
                        })
                        .collect::<Vec<_>>()
                } else {
                    let sep = if kind == "enum" { ',' } else { ';' };
                    split_top(&body, sep)
                        .into_iter()
                        .map(normalize)
                        .filter(|m| !m.is_empty() && m != "#private")
                        .map(|m| (member_name(&m), m))
                        .collect()
                };
                for (name, text) in members {
                    entry.members.entry(name).or_default().push(text);
                }
            }
            _ => {
                entry
                    .members
                    .entry(String::new())
                    .or_default()
                    .push(statement.clone());
            }
        }
    }
    out
}

/// The keyword of a statement, after its modifiers.
fn keyword(statement: &str) -> &str {
    statement
        .split(' ')
        .find(|w| !matches!(*w, "export" | "declare" | "default" | "abstract"))
        .unwrap_or("")
}

/// What a statement declares: (kind, name); `None` for imports.
fn head(statement: &str) -> Option<(&'static str, String)> {
    let mut rest = statement;
    loop {
        let before = rest;
        for word in ["export ", "declare ", "default ", "abstract "] {
            rest = rest.strip_prefix(word).unwrap_or(rest);
        }
        if rest == before {
            break;
        }
    }
    if rest.starts_with("import ") {
        return None;
    }
    if rest.starts_with(['{', '*']) || rest.starts_with("type {") || rest.starts_with("type *") {
        // A re-export: the statement is its own name.
        return Some(("re-export", statement.to_owned()));
    }
    for (word, kind) in [
        ("function ", "function"),
        ("const ", "const"),
        ("let ", "const"),
        ("var ", "const"),
        ("type ", "type"),
        ("interface ", "interface"),
        ("class ", "class"),
        ("namespace ", "namespace"),
        ("module ", "namespace"),
        ("enum ", "enum"),
    ] {
        if let Some(after) = rest.strip_prefix(word) {
            let name: String = after.chars().take_while(|c| is_ident(*c)).collect();
            assert!(!name.is_empty(), "a name in `{statement}`");
            return Some((kind, name));
        }
    }
    panic!("a declaration the baseline does not know: `{statement}`")
}

/// A member's name: `a`, `static a`, `get a`, `()` (a call signature),
/// `new()`, `[index]`, `[Symbol.iterator]`.
fn member_name(member: &str) -> String {
    let mut rest = member;
    let mut prefix = String::new();
    loop {
        let before = rest;
        for word in ["static ", "readonly ", "public ", "protected ", "abstract "] {
            if let Some(r) = rest.strip_prefix(word) {
                if word == "static " {
                    prefix.push_str("static ");
                }
                rest = r;
            }
        }
        for word in ["get ", "set "] {
            if let Some(r) = rest.strip_prefix(word)
                && r.starts_with(|c: char| is_ident(c) || c == '[')
            {
                prefix.push_str(word);
                rest = r;
            }
        }
        if rest == before {
            break;
        }
    }
    let name = if rest.starts_with('(') || rest.starts_with('<') {
        "()".to_owned()
    } else if rest.starts_with("new (") || rest.starts_with("new(") || rest.starts_with("new <") {
        "new()".to_owned()
    } else if rest.starts_with('[') {
        let end = matching(rest, 0).unwrap_or(rest.len());
        if rest[..end].contains(':') {
            "[index]".to_owned()
        } else {
            rest[..end].to_owned()
        }
    } else if rest.starts_with('"') || rest.starts_with('\'') {
        let q = rest.as_bytes()[0] as char;
        rest[..rest[1..].find(q).map_or(rest.len(), |e| e + 2)].to_owned()
    } else {
        rest.chars()
            .take_while(|c| is_ident(*c) || *c == '#')
            .collect()
    };
    format!("{prefix}{name}")
}

/// `(header, body)` of `interface A extends B { … }`.
fn container(statement: &str) -> Option<(String, String)> {
    let close = statement.rfind('}')?;
    let mut depth = 0i32;
    let bytes = statement.as_bytes();
    let mut open = None;
    for i in (0..=close).rev() {
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                depth -= 1;
                if depth == 0 {
                    open = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let open = open?;
    Some((
        normalize(&statement[..open]),
        statement[open + 1..close].to_owned(),
    ))
}

/// The top-level statements of code without comments, each normalized: a
/// statement ends at `;`, or at the `}` closing the body of a container.
fn statements(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let mut quote: Option<char> = None;
    let chars: Vec<(usize, char)> = code.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        if let Some(q) = quote {
            if c == '\\' {
                i += 1;
            } else if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' | '`' => quote = Some(c),
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth -= 1;
                if c == '}' && depth == 0 {
                    let text = normalize(&code[start..=at]);
                    let container = matches!(
                        keyword(&text),
                        "interface" | "class" | "namespace" | "module" | "enum"
                    );
                    if container && !code[at + 1..].trim_start().starts_with(';') {
                        out.push(text);
                        start = at + 1;
                    }
                }
            }
            ';' if depth == 0 => {
                let text = normalize(&code[start..at]);
                if !text.is_empty() {
                    out.push(text);
                }
                start = at + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let rest = normalize(&code[start..]);
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

/// Code without `//` and `/* */` comments (strings kept).
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('/', Some('/')) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for n in chars.by_ref() {
                    if last == '*' && n == '/' {
                        break;
                    }
                    last = n;
                }
                out.push(' ');
            }
            ('"' | '\'' | '`', _) => {
                quote = Some(c);
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Whitespace runs as one space, no trailing separator.
fn normalize(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim_end_matches([';', ',']).trim_end().to_owned()
}

/// The position of the first `c` outside brackets and strings.
fn top_level(text: &str, c: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut prev = ' ';
    for (i, ch) in text.char_indices() {
        if let Some(q) = quote {
            if ch == q && prev != '\\' {
                quote = None;
            }
        } else if ch == c && depth == 0 && !(c == '=' && matches!(prev, '=' | '!' | '<' | '>')) {
            if !(c == '=' && text[i + 1..].starts_with('>')) {
                return Some(i);
            }
        } else {
            match ch {
                '"' | '\'' | '`' => quote = Some(ch),
                '{' | '(' | '[' | '<' => depth += 1,
                '}' | ')' | ']' => depth -= 1,
                '>' if prev != '=' => depth -= 1,
                _ => {}
            }
        }
        prev = ch;
    }
    None
}

/// The position of the bracket closing the one at `open`.
fn matching(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (i, ch) in text[open..].char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `text` split at each top-level `sep`, trimmed, empty parts left out.
fn split_top(text: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = top_level(rest, sep) {
        out.push(rest[..i].trim());
        rest = &rest[i + sep.len_utf8()..];
    }
    out.push(rest.trim());
    out.retain(|p| !p.is_empty());
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widening_is_compatible() {
        assert!(compatible("a?: \"x\" | \"y\"", "a?: \"x\" | \"y\" | \"z\""));
        assert!(!compatible("a?: \"x\" | \"y\"", "a?: \"x\""));
        assert!(compatible(
            "type OS = \"ios\" | \"android\"",
            "type OS = \"ios\" | \"android\" | \"web\""
        ));
        assert!(compatible(
            "export declare function f(a: number): void",
            "export declare function f(a: number, b?: string): void"
        ));
        assert!(compatible(
            "export declare function f(a: number): void",
            "export declare function f(a?: number): void"
        ));
        assert!(compatible(
            "m(a: X): void",
            "m(a: X | Y, ...rest: Z[]): void"
        ));
        assert!(!compatible(
            "export declare function f(a: number): void",
            "export declare function f(a: number, b: string): void"
        ));
        assert!(!compatible("m(a: X): number", "m(a: X): string"));
        assert!(!compatible("m(a: X | Y): void", "m(a: X): void"));
        // Names are not types; a void result may become a value, but not a
        // callback's (the app's to return).
        assert!(compatible(
            "onSelect?: (data: number) => void",
            "onSelect?: (index: number) => void"
        ));
        assert!(compatible("dismiss(): void", "dismiss(): Promise<void>"));
        assert!(compatible(
            "export declare const S: ((p: P) => E) & { a(x: string): void; b(): void }",
            "export declare const S: ((props: P) => E) & { a(text: string): Promise<void>; b(): void }"
        ));
        assert!(!compatible(
            "onPress?: () => void",
            "onPress?: () => Promise<void>"
        ));
        assert!(!compatible("m(): number", "m(): Promise<number>"));
        // A callback's parameters are the app's to accept.
        assert!(!compatible(
            "onPress?: (e: X) => void",
            "onPress?: (e: X | Y) => void"
        ));
        assert!(compatible(
            "onPress?: ((e: X) => void) | null",
            "onPress?: ((e: X) => void) | null | string"
        ));
    }

    #[test]
    fn declarations_and_members() {
        let file = "/** doc */\nexport interface ViewProps extends A, B {\n  /** x */\n  style?: StyleProp<ViewStyle>;\n  onPress?: (e: { x: number; y: number }) => void;\n  get value(): number;\n  (x: number): void;\n  [key: string]: unknown;\n}\nexport declare function f(a: number): void;\nexport declare function f(a: string): void;\nexport type T = { a: 1; b: 2 } | null;\nexport declare namespace JSX {\n  type Element = YNode;\n  interface Attr {\n    children: {};\n  }\n}\nexport type { JSX } from \"./types\";\nimport type { X } from \"./x\";\n";
        let d = declarations(file);
        let view = &d["ViewProps"];
        assert_eq!(view.header, "export interface ViewProps extends A, B");
        assert_eq!(
            view.members.keys().collect::<Vec<_>>(),
            ["()", "[index]", "get value", "onPress", "style"]
        );
        assert_eq!(d["f"].members[""].len(), 2);
        assert_eq!(
            d["T"].members[""],
            ["export type T = { a: 1; b: 2 } | null"]
        );
        assert_eq!(
            d["JSX"].members.keys().collect::<Vec<_>>(),
            ["Attr", "Element"]
        );
        assert!(d.contains_key("export type { JSX } from \"./types\""));
        assert!(!d.contains_key("X"));
    }

    #[test]
    fn breaks_are_found() {
        let base = vec![(
            "nsh/index.d.ts".to_owned(),
            "export interface ButtonProps {\n  title: string;\n  kind?: \"a\" | \"b\";\n}\nexport declare function g(): void;\nexport interface Payload { x: number; }\n".to_owned(),
        )];
        let same = types(&base, &base);
        assert!(same.is_empty(), "{same:?}");
        let now = vec![(
            "nsh/index.d.ts".to_owned(),
            "export interface ButtonProps {\n  title: string;\n  kind?: \"a\";\n  size: number;\n  tone?: string;\n}\nexport interface Payload { x: number; y: number; }\n".to_owned(),
        )];
        let breaks = types(&base, &now);
        assert_eq!(breaks.len(), 3, "{breaks:#?}");
        assert!(breaks[0].contains("ButtonProps.kind"));
        assert!(breaks[1].contains("ButtonProps.size: added as required"));
        assert!(breaks[2].contains("g: removed"));
        assert_eq!(
            runtime(
                "[nsh]\na function\nb object\nb.c function\n",
                "[nsh]\na class\nb object\n"
            ),
            ["[nsh] b.c: removed"]
        );
    }
}
