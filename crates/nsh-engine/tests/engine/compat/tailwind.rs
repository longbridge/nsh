//! `api/tailwind.txt`: the Tailwind classes apps name, and what each one
//! does. Classes are resolved at run time from the strings in old bundles,
//! so a class that goes, or whose style changes (`p-4` no longer 16pt),
//! breaks those apps silently.
//!
//! One line per class, `<class> <style>`: the fields of the resolved
//! `Style` and `ClassProps` that differ from their defaults (`-`: none),
//! then ` | <context>: <style>` for each context (dark, a window width, a
//! state, a group's state, ...) where the class resolves otherwise. A class
//! may be added; one recorded at the base must stay, with the same line.
//! A break is `tailwind <class>: removed|changed (<old> → <new>)`, and the
//! `api/closed.txt` entry `tailwind <class>` closes exactly that class.
//!
//! The classes are enumerated, never listed by hand: the utilities without
//! a value and the prefixes are the string literals of the resolver
//! (`nsh-core/src/tailwind/`), kept when they resolve; each prefix takes
//! every name of the default theme's scales (`Theme::names`), the component
//! theme's tokens (`Theme::token_names`), a scale of numbers and fractions
//! and representative arbitrary values; each variant (the literals of
//! `variant`, the breakpoints, `group-*`) is applied to one utility.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use nsh_core::{
    Style,
    tailwind::{
        ClassEnvironment, ClassList, ClassProps, Context, Platform, Theme, deps, parse, position,
        state,
    },
};

/// The resolver's sources, whose string literals name the utilities and
/// the variants.
fn source(file: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../nsh-core/src/tailwind")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The string literals of `text` (Rust source) that could be (part of) a
/// class; comments and character literals are skipped.
fn literals(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                i += text[i..].find('\n').unwrap_or(text.len() - i);
            }
            b'\'' if b.get(i + 2) == Some(&b'\'') => i += 3,
            b'\'' if b.get(i + 1) == Some(&b'\\') && b.get(i + 3) == Some(&b'\'') => i += 4,
            b'"' => {
                let start = i + 1;
                i = start;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                let lit = &text[start..i.min(b.len())];
                if !lit.is_empty()
                    && lit
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-./".contains(c))
                {
                    out.insert(lit.to_owned());
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// The text of `text` from `start` to the first line that is exactly `end`.
fn section<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("`{start}` not found"));
    let rest = &text[from..];
    let to = rest
        .find(&format!("\n{end}\n"))
        .unwrap_or_else(|| panic!("the end of `{start}` not found"));
    &rest[..to]
}

/// Numbers the numeric scales take (Tailwind's spacing, opacity, scale,
/// rotation, duration, ... steps).
const NUMBERS: &[&str] = &[
    "0", "0.5", "1", "1.5", "2", "2.5", "3", "3.5", "4", "5", "6", "7", "8", "9", "10", "11", "12",
    "14", "15", "16", "20", "24", "25", "28", "30", "32", "35", "36", "40", "44", "45", "48", "50",
    "52", "55", "56", "60", "64", "65", "70", "72", "75", "80", "85", "90", "95", "96", "100",
    "105", "110", "125", "150", "180", "200", "300", "500", "700", "1000",
];

const FRACTIONS: &[&str] = &[
    "1/2", "1/3", "2/3", "1/4", "3/4", "1/5", "2/5", "3/5", "4/5", "1/6", "5/6", "1/12", "11/12",
];

/// Arbitrary values, one of each kind, tried with every prefix.
const ARBITRARY: &[&str] = &[
    "[13px]",
    "[1.5rem]",
    "[50%]",
    "[2]",
    "[0.5]",
    "[45deg]",
    "[200ms]",
    "[#123456]",
    "[#12345680]",
    "[rgb(1_2_3)]",
    "[color:#abcdef]",
    "[length:13px]",
    "[number:2]",
    "[repeat(3,1fr)]",
    "[3_0_50%]",
    "[0_2px_4px_#000]",
];

/// Where each class is resolved: the first is the base, the others each
/// change one thing.
struct Place {
    label: String,
    /// The [`deps`] bits of the classes this place can change: the core
    /// resolves classes again only when what they depend on changes. The
    /// platform is fixed per engine and only variants read it: its places
    /// (`u16::MAX`) count for every class with a variant.
    deps: u16,
    env: ClassEnvironment,
    window: (f32, f32),
    rtl: bool,
    states: u8,
    position: u8,
    group: Option<(&'static str, u8)>,
}

fn places() -> Vec<Place> {
    let base = || Place {
        label: "base".into(),
        deps: u16::MAX,
        env: ClassEnvironment::default(),
        window: (390.0, 844.0),
        rtl: false,
        states: 0,
        position: 0,
        group: None,
    };
    let mut out = vec![base()];
    let mut add = |label: &str, deps: u16, f: &dyn Fn(&mut Place)| {
        let mut p = base();
        p.label = label.into();
        p.deps = deps;
        f(&mut p);
        out.push(p);
    };
    add("dark", deps::DARK | deps::THEME, &|p| p.env.dark = true);
    add("android", u16::MAX, &|p| p.env.platform = Platform::Android);
    add("web", u16::MAX, &|p| p.env.platform = Platform::Web);
    add("reduce-motion", deps::MOTION, &|p| {
        p.env.reduce_motion = true
    });
    add("rtl", deps::RTL, &|p| p.rtl = true);
    add("landscape", deps::WIDTH | deps::HEIGHT, &|p| {
        p.window = (844.0, 390.0)
    });
    for w in [700.0, 900.0, 1100.0, 1400.0, 1700.0] {
        add(&format!("width {w}"), deps::WIDTH | deps::HEIGHT, &|p| {
            p.window = (w, 3000.0)
        });
    }
    let states = [
        ("pressed", state::PRESSED),
        ("hovered", state::HOVERED),
        ("focused", state::FOCUSED),
        ("disabled", state::DISABLED),
        ("focus-within", state::FOCUS_WITHIN),
    ];
    for (name, bit) in states {
        add(name, deps::STATE, &|p| p.states = bit);
    }
    for (name, bit) in [
        ("first", position::FIRST),
        ("last", position::LAST),
        ("odd", position::ODD),
        ("even", position::EVEN),
        ("only", position::ONLY),
    ] {
        add(name, deps::POSITION, &|p| p.position = bit);
    }
    for (group, label) in [("", "group"), ("item", "group/item")] {
        for (name, bit) in states {
            add(&format!("{label} {name}"), deps::GROUP, &|p| {
                p.group = Some((group, bit))
            });
        }
    }
    out
}

/// `value` with taffy's lengths, whose `{:?}` is a tagged pointer, as
/// what they mean (`length 16`, `percent 0.5`, `auto`).
fn lengths(value: &str) -> String {
    const OPEN: &str = "CompactLength(CompactLengthInner { tagged_ptr: 0x";
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(i) = rest.find(OPEN) {
        out.push_str(&rest[..i]);
        let after = &rest[i + OPEN.len()..];
        let end = after.find(" })").expect("a tagged pointer");
        let bits = u64::from_str_radix(&after[..end], 16).expect("a tagged pointer");
        let n = f32::from_bits((bits >> 32) as u32);
        let _ = match bits as usize & 0xff {
            taffy::CompactLength::LENGTH_TAG => write!(out, "length {n}"),
            taffy::CompactLength::PERCENT_TAG => write!(out, "percent {n}"),
            taffy::CompactLength::AUTO_TAG => write!(out, "auto"),
            taffy::CompactLength::FR_TAG => write!(out, "fr {n}"),
            taffy::CompactLength::CONTENT_TAG => write!(out, "content"),
            taffy::CompactLength::MIN_CONTENT_TAG => write!(out, "min-content"),
            taffy::CompactLength::MAX_CONTENT_TAG => write!(out, "max-content"),
            taffy::CompactLength::FIT_CONTENT_PX_TAG => write!(out, "fit-content {n}"),
            taffy::CompactLength::FIT_CONTENT_PERCENT_TAG => write!(out, "fit-content percent {n}"),
            taffy::CompactLength::FIT_CONTENT_KEYWORD_TAG => write!(out, "fit-content"),
            taffy::CompactLength::STRETCH_TAG => write!(out, "stretch"),
            tag => panic!("an unknown length tag {tag:#x}"),
        };
        rest = &after[end + 3..];
    }
    out.push_str(rest);
    colors(&out)
}

/// `value` with `Color(<u32>)` as `Color(#rrggbbaa)`.
fn colors(value: &str) -> String {
    const OPEN: &str = "Color(";
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(i) = rest.find(OPEN) {
        let after = &rest[i + OPEN.len()..];
        let digits = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        match after[..digits].parse::<u32>() {
            Ok(rgba) if after[digits..].starts_with(')') => {
                let _ = write!(out, "{}Color(#{rgba:08x}", &rest[..i]);
            }
            _ => out.push_str(&rest[..i + OPEN.len() + digits]),
        }
        rest = &after[digits..];
    }
    out.push_str(rest);
    out
}

/// The fields of `value` (a struct's `{:?}`), or `None` when it is not a
/// struct (a number, an enum, an option, a list).
fn fields(value: &str) -> Option<Vec<(&str, &str)>> {
    let open = value.find(" { ")?;
    let name = &value[..open];
    let body = value[open + 3..].strip_suffix(" }")?;
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_:<>".contains(c))
        || !closes_at_end(&value[open + 1..])
    {
        return None;
    }
    Some(
        split_top(body)
            .into_iter()
            .map(|field| {
                field
                    .split_once(": ")
                    .unwrap_or_else(|| panic!("a field without a name: `{field}` in `{value}`"))
            })
            .collect(),
    )
}

/// Where `value` and `default` (`{:?}` of the same type) differ, as
/// `path=value`: struct fields are followed, anything else is one leaf.
fn diff(path: &str, value: &str, default: &str, out: &mut Vec<String>) {
    if value == default {
        return;
    }
    if let (Some(fv), Some(fd)) = (fields(value), fields(default))
        && fv.len() == fd.len()
    {
        for ((name, v), (_, d)) in fv.into_iter().zip(fd) {
            let path = if path.is_empty() {
                name.to_owned()
            } else {
                format!("{path}.{name}")
            };
            diff(&path, v, d, out);
        }
        return;
    }
    out.push(format!("{path}={}", lengths(value)));
}

/// Whether the bracket `s` starts with closes at its last character.
fn closes_at_end(s: &str) -> bool {
    let mut depth = 0i32;
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if quoted {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => quoted = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return i + c.len_utf8() == s.len();
                }
            }
            _ => {}
        }
    }
    false
}

/// `s` split at the `, ` outside brackets and strings.
fn split_top(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    let (mut quoted, mut escaped) = (false, false);
    let bytes = s.as_bytes();
    for (i, &c) in bytes.iter().enumerate() {
        if quoted {
            match c {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => quoted = false,
                _ => {}
            }
            continue;
        }
        match c {
            b'"' => quoted = true,
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            b',' if depth == 0 && bytes.get(i + 1) == Some(&b' ') => {
                parts.push(&s[start..i]);
                start = i + 2;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

struct Resolver {
    theme: Theme,
    /// `{:?}` of the default `Style` and `ClassProps`.
    defaults: (String, String),
    places: Vec<Place>,
}

impl Resolver {
    fn new() -> Self {
        let defaults = (
            format!("{:?}", Style::default()),
            format!("{:?}", ClassProps::default()),
        );
        Self {
            theme: Theme::default(),
            defaults,
            places: places(),
        }
    }

    /// The class list of `class`, when every part of it is known.
    fn known(&self, class: &str) -> Option<ClassList> {
        let list = parse(&self.theme, class);
        list.unknown().is_empty().then_some(list)
    }

    /// What `list` sets at `place`, as stable text.
    fn style(&self, list: &ClassList, place: &Place) -> String {
        let group = |name: &str| match place.group {
            Some((g, bits)) if g == name => bits,
            _ => 0,
        };
        let cx = Context {
            env: place.env,
            window: place.window,
            rtl: place.rtl,
            states: place.states,
            position: place.position,
            group: &group,
        };
        let mut style = Style::default();
        let mut props = ClassProps::default();
        list.apply(&cx, &self.theme, &mut style, &mut props, |_| {});
        let mut set = Vec::new();
        diff("", &format!("{style:?}"), &self.defaults.0, &mut set);
        diff("props", &format!("{props:?}"), &self.defaults.1, &mut set);
        set.sort_unstable();
        if set.is_empty() {
            "-".into()
        } else {
            set.join(" ")
        }
    }

    /// The record's line of `class` (without the class).
    fn line(&self, class: &str, list: &ClassList) -> String {
        let base = self.style(list, &self.places[0]);
        let mut line = base.clone();
        for place in &self.places[1..] {
            let applies = match place.deps {
                u16::MAX => class.contains(':'),
                deps => list.deps() & deps != 0,
            };
            if !applies {
                continue;
            }
            let s = self.style(list, place);
            if s != base {
                line.push_str(&format!(" | {}: {s}", place.label));
            }
        }
        if !list.groups().is_empty() {
            line.push_str(&format!(" | groups: {:?}", list.groups()));
        }
        line
    }
}

/// The classes the record holds.
fn classes(theme: &Theme) -> BTreeSet<String> {
    let utilities = source("utilities.rs");
    let resolver = source("mod.rs");
    let words = literals(&utilities);
    let prefixes = literals(section(&utilities, "static PREFIXES", "];"));

    let names = theme.names();
    let tokens = theme.token_names();
    let known = |c: &str| parse(theme, c).unknown().is_empty();
    // Colors (the palette's and the tokens') are the same values to every
    // color utility: all of them on `bg-*` and `text-*`, a sample on the
    // others.
    let is_color = |n: &str| known(&format!("bg-{n}"));
    let mut colors = BTreeSet::new();
    let mut values: BTreeSet<String> = NUMBERS
        .iter()
        .chain(FRACTIONS)
        .chain(ARBITRARY)
        .map(|v| v.to_string())
        .chain(words.iter().filter(|w| !w.contains('/')).cloned())
        .collect();
    for n in &names {
        if is_color(n) {
            colors.insert(n.to_string());
        } else {
            values.insert(n.to_string());
        }
    }
    for t in &tokens {
        for v in [t.to_string(), format!("(--{t})"), format!("{t}/50")] {
            if is_color(&v) {
                colors.insert(v);
            } else {
                values.insert(v);
            }
        }
    }
    values.retain(|v| !colors.contains(v));
    let first_token = tokens.iter().find(|t| is_color(t)).copied().unwrap_or("");
    let sample_colors: Vec<String> = [
        "black",
        "white",
        "transparent",
        "current",
        "red-500",
        "red-500/50",
        "red-500/[0.3]",
        "[#123456]/25",
    ]
    .map(String::from)
    .into_iter()
    .chain([first_token.to_owned(), format!("(--{first_token})")])
    .collect();

    let mut out = BTreeSet::new();
    let mut try_class = |c: String| {
        if known(&c) {
            out.insert(c);
        }
    };
    for w in words.iter().chain(literals(&resolver).iter()) {
        try_class(w.clone());
        // Prefixes accepting any value (`will-change-*`): one sample.
        if w.ends_with('-') {
            try_class(format!("{w}auto"));
        }
    }
    for p in &prefixes {
        if known(&format!("{p}-zz9-not-a-value")) {
            // It takes any value (`order-*`): a sample.
            for v in ["1", "auto", "[13px]"] {
                try_class(format!("{p}-{v}"));
            }
            continue;
        }
        let all_colors = matches!(p.as_str(), "bg" | "text");
        let colors = colors.iter().filter(|_| all_colors).chain(&sample_colors);
        for v in values.iter().chain(colors) {
            try_class(format!("{p}-{v}"));
        }
        // Negative values are the same values, negated: a sample.
        for v in ["0", "0.5", "1", "4", "1/2", "px", "full", "[13px]"] {
            try_class(format!("-{p}-{v}"));
        }
    }

    // Variants, on one utility each (and on a theme token, whose rules
    // are made per appearance).
    let mut variants: BTreeSet<String> = literals(section(&resolver, "fn variant(", "}"))
        .into_iter()
        .collect();
    for n in &names {
        variants.insert(n.to_string());
        variants.insert(format!("max-{n}"));
    }
    for s in literals(section(&resolver, "fn variant(", "}")) {
        variants.insert(format!("group-{s}"));
        variants.insert(format!("group-{s}/item"));
    }
    variants.extend(["min-[600px]", "max-[600px]"].map(String::from));
    for v in &variants {
        try_class(format!("{v}:p-4"));
        if let Some(t) = tokens.first() {
            try_class(format!("{v}:bg-{t}"));
        }
    }
    for c in [
        "!p-4",
        "p-4!",
        "hover:!p-4",
        "placeholder:text-red-500",
        "group",
        "group/item",
    ] {
        try_class(c.into());
    }
    out
}

/// `api/tailwind.txt` as this checkout makes it: the enumerated classes,
/// and those the record already holds that still resolve, so the record
/// only grows when a change to the resolver hides a class from the
/// enumeration.
pub(super) fn record() -> String {
    let resolver = Resolver::new();
    let mut classes = classes(&resolver.theme);
    let recorded = std::fs::read_to_string(super::api_dir().join("tailwind.txt"))
        .unwrap_or_default()
        .replace("\r\n", "\n");
    for class in lines(&recorded).into_keys() {
        if resolver.known(class).is_some() {
            classes.insert(class.to_owned());
        }
    }
    let mut out = String::new();
    for class in classes {
        let list = resolver
            .known(&class)
            .expect("enumerated classes are known");
        out.push_str(&class);
        out.push(' ');
        out.push_str(&resolver.line(&class, &list));
        out.push('\n');
    }
    out
}

/// The lines of a record, by class.
fn lines(record: &str) -> BTreeMap<&str, &str> {
    record
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.split_once(' ').unwrap_or((l, "")))
        .collect()
}

/// What the record `base` had that `now` removes or changes, less the
/// classes `closed` (the entries of `api/closed.txt`) closes.
pub(super) fn breaks(base: &str, now: &str, closed: &[String]) -> Vec<String> {
    let resolver = Resolver::new();
    breaks_with(base, now, closed, &|class| {
        let list = resolver.known(class)?;
        Some(resolver.line(class, &list))
    })
}

/// [`breaks`], with `resolve` giving the line of a class `now` lacks (a
/// class the enumeration no longer finds may still resolve).
fn breaks_with(
    base: &str,
    now: &str,
    closed: &[String],
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let now = lines(now);
    let mut breaks = Vec::new();
    for (class, old) in lines(base) {
        let entry = format!("tailwind {class}");
        if closed.contains(&entry) {
            continue;
        }
        let new = match now.get(class) {
            Some(new) => Some((*new).to_owned()),
            None => resolve(class),
        };
        match new {
            None => breaks.push(format!("{entry}: removed")),
            Some(new) if new != old => breaks.push(format!("{entry}: changed ({old} → {new})")),
            Some(_) => {}
        }
    }
    breaks
}

/// A class may be added, never removed or changed, unless closed.
#[test]
fn tailwind_breaks_are_reported() {
    let none = |_: &str| None;
    let base = "p-4 padding=16\nbg-red-500 color=red\n";
    assert!(breaks_with(base, base, &[], &none).is_empty());
    let more = "p-4 padding=16\nbg-red-500 color=red\np-5 padding=20\n";
    assert!(breaks_with(base, more, &[], &none).is_empty());
    let now = "p-4 padding=17\n";
    assert_eq!(
        breaks_with(base, now, &[], &none),
        [
            "tailwind bg-red-500: removed",
            "tailwind p-4: changed (padding=16 → padding=17)",
        ]
    );
    let closed = super::closed("tailwind bg-red-500 -- unsafe\ntailwind p -- not p-4\n");
    assert_eq!(
        breaks_with(base, now, &closed, &none),
        ["tailwind p-4: changed (padding=16 → padding=17)"]
    );
    // A class the enumeration misses is resolved.
    let found = |c: &str| (c == "bg-red-500").then(|| "color=red".to_owned());
    assert_eq!(breaks_with(base, now, &[], &found).len(), 1);
    // The real resolver.
    let real = breaks("p-4 padding=16\n", "", &[]);
    assert!(
        real[0].starts_with("tailwind p-4: changed (padding=16 → layout."),
        "{real:?}"
    );
    assert_eq!(breaks("p-nope x\n", "", &[]), ["tailwind p-nope: removed"]);
}

/// The record shows what classes do, and the enumeration finds every kind.
#[test]
fn tailwind_record_resolves() {
    let record = record();
    let lines = lines(&record);
    let line = |c: &str| {
        *lines
            .get(c)
            .unwrap_or_else(|| panic!("{c} is not recorded"))
    };
    assert!(line("p-4").contains("16"), "{}", line("p-4"));
    for c in [
        "flex-row",
        "bg-red-500",
        "w-[13px]",
        "bg-[#123456]",
        "-mt-1",
        "w-1/2",
        "text-lg",
        "rounded-lg",
        "dark:p-4",
        "active:p-4",
        "ios:p-4",
        "md:p-4",
        "group-hover:p-4",
        "bg-primary",
    ] {
        line(c);
    }
    assert!(
        line("dark:p-4").starts_with("- | dark: "),
        "{}",
        line("dark:p-4")
    );
    assert!(!lines.contains_key("p-nope"));
}
