//! `api/tailwind.txt`: the Tailwind classes apps name, and what each one
//! does. Classes are resolved at run time from the strings in old bundles,
//! so a class that goes, or whose style changes (`p-4` no longer 16pt),
//! breaks those apps silently.
//!
//! What a class does is written as apps write styles ([`style`]): NSH's
//! style keys with CSS's values, `p-4 padding: 16`, then ` | <context>:
//! <decls>` for each context (dark, a window width, a state, a group's
//! state, ...) where the class resolves otherwise (`-`: it sets nothing).
//!
//! The record holds rules, not every value of every utility. It has three
//! sections:
//!
//! - `[scales]`: the values utilities share, each a probe class's value
//!   (`spacing`: what `p-*` sets, `color`: what `bg-*` sets), as
//!   `<scale> <name>=<value>...`; a value that differs per context is
//!   `<name>=<value>;<context>=<value>` (spaces in a context are `_`);
//! - `[rules]`: `<utility>-{<scale>} <line>`: the utility takes every name
//!   of the scale, and its line is `<line>` with `{<scale>}` (and
//!   `{<scale>@<context>}`) as that name's value: `m-{spacing} margin:
//!   {spacing}`. An earlier rule's class wins over a later one's;
//! - `[classes]`: every class whose line the rules do not give, `!` for a
//!   class a rule names that is no class.
//!
//! The rules are inferred, never written by hand, so the record changes
//! when what a class does changes, and only then. A class may be added;
//! one the record held at the base must stay, with the same line. A break
//! is `tailwind <class>: removed|changed (<old> → <new>)`, and the
//! `api/closed.txt` entry `tailwind <class>` closes exactly that class.
//!
//! The classes are enumerated, never listed by hand: the utilities without
//! a value and the prefixes are the string literals of the resolver
//! (`nsh-core/src/tailwind/`), kept when they resolve; each prefix takes
//! every name of the default theme's scales (`Theme::names`), the component
//! theme's tokens (`Theme::token_names`), a scale of numbers and fractions
//! and representative arbitrary values, and the same negated when the sign
//! does something; each variant (the literals of `variant`, the
//! breakpoints, `group-*`) is applied to one utility.

mod style;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
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

struct Resolver {
    theme: Theme,
    default: Style,
    places: Vec<Place>,
}

impl Resolver {
    fn new() -> Self {
        // Apps' classes name the tokens of `nsh/base`'s default theme.
        let mut theme = Theme::default();
        theme.set_tokens(nsh_base::Theme::default().tokens());
        Self {
            theme,
            default: Style::default(),
            places: places(),
        }
    }

    /// The class list of `class`, when every part of it is known.
    fn known(&self, class: &str) -> Option<ClassList> {
        let list = parse(&self.theme, class);
        list.unknown().is_empty().then_some(list)
    }

    /// The style and props `list` gives at `place`.
    fn resolve(&self, list: &ClassList, place: &Place) -> (Style, ClassProps) {
        let group = |name: &str| match place.group {
            Some((g, bits)) if g == name => bits,
            _ => 0,
        };
        let cx = Context {
            env: place.env,
            window: nsh_core::Size::new(
                nsh_core::Points(place.window.0),
                nsh_core::Points(place.window.1),
            ),
            rtl: place.rtl,
            states: place.states,
            position: place.position,
            group: &group,
        };
        let mut style = Style::default();
        let mut props = ClassProps::default();
        list.apply(&cx, &self.theme, &mut style, &mut props, |_| {});
        (style, props)
    }

    /// What `list` sets at `place`, as apps write it.
    fn style(&self, list: &ClassList, place: &Place) -> String {
        let (style, props) = self.resolve(list, place);
        let decls = style::decls(&style, &props, &self.default);
        if decls.is_empty() {
            "-".into()
        } else {
            decls.join("; ")
        }
    }

    /// The places where `list` (of `class`) is resolved: the base, then
    /// those that can change it.
    fn places_of<'a>(&'a self, class: &str, list: &ClassList) -> Vec<&'a Place> {
        let mut out = vec![&self.places[0]];
        for place in &self.places[1..] {
            let applies = match place.deps {
                u16::MAX => class.contains(':'),
                deps => list.deps() & deps != 0,
            };
            if applies {
                out.push(place);
            }
        }
        out
    }

    /// The record's line of `class` (without the class).
    fn line(&self, class: &str, list: &ClassList) -> String {
        let places = self.places_of(class, list);
        let base = self.style(list, places[0]);
        let mut line = base.clone();
        for place in &places[1..] {
            let s = self.style(list, place);
            if s != base {
                let _ = write!(line, " | {}: {s}", place.label);
            }
        }
        if !list.groups().is_empty() {
            let groups: Vec<String> = list
                .groups()
                .iter()
                .map(|g| {
                    if g.is_empty() {
                        "group".to_owned()
                    } else {
                        format!("group/{g}")
                    }
                })
                .collect();
            let _ = write!(line, " | groups: {}", groups.join(", "));
        }
        line
    }

    /// The line of `class`, when it is one.
    fn resolve_line(&self, class: &str) -> Option<String> {
        let list = self.known(class)?;
        Some(self.line(class, &list))
    }
}

/// An enumerated class: the utility (`p`, `-m`) and the value it was made
/// of, when it was made of one.
type Enumerated = BTreeMap<String, Option<(String, String)>>;

/// The classes the record holds.
fn classes(resolver: &Resolver) -> Enumerated {
    let theme = &resolver.theme;
    let utilities = source("utilities.rs");
    let resolver_src = source("mod.rs");
    let words = literals(&utilities);
    let prefixes = literals(section(&utilities, "static PREFIXES", "];"));

    let names = theme.names();
    let tokens = theme.token_names();
    let known = |c: &str| parse(theme, c).unknown().is_empty();
    let is_color = |n: &str| known(&format!("bg-{n}"));
    // Colors (the palette's and the tokens') are the same values to every
    // color utility; the rules keep them to one line per utility.
    let mut colors: BTreeSet<String> = ["red-500/50", "red-500/[0.3]", "[#123456]/25"]
        .map(String::from)
        .into();
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
    // A token by name, as a variable and with an opacity; a token that is
    // no color is a variable only where its name is a value.
    let mut variables = BTreeSet::new();
    let mut alphas = 0;
    for t in &tokens {
        if is_color(t) {
            colors.extend([t.to_string(), format!("(--{t})")]);
            // An opacity is the palette's mechanism: a sample.
            if alphas < 3 {
                colors.insert(format!("{t}/50"));
                alphas += 1;
            }
        } else {
            values.insert(t.to_string());
            variables.insert(t.to_string());
        }
    }
    values.retain(|v| !colors.contains(v));

    let mut out = Enumerated::new();
    let mut try_class = |c: String, made: Option<(&str, &str)>| {
        if !known(&c) {
            return;
        }
        // An arbitrary value or a variable a utility ignores is no class
        // worth keeping.
        let ignored = made.is_some_and(|(_, v)| v.starts_with(['[', '(']))
            && resolver.resolve_line(&c).as_deref() == Some("-");
        if !ignored {
            out.insert(c, made.map(|(p, v)| (p.to_owned(), v.to_owned())));
        }
    };
    for w in words.iter().chain(literals(&resolver_src).iter()) {
        try_class(w.clone(), None);
        // Prefixes accepting any value (`will-change-*`): one sample.
        if w.ends_with('-') {
            try_class(format!("{w}auto"), None);
        }
    }
    for p in &prefixes {
        if known(&format!("{p}-zz9-not-a-value")) {
            // It takes any value (`order-*`): a sample.
            for v in ["1", "auto", "[13px]"] {
                try_class(format!("{p}-{v}"), None);
            }
            continue;
        }
        let takes_colors = resolver
            .resolve_line(&format!("{p}-red-500"))
            .is_some_and(|l| l != "-");
        let colors = colors.iter().filter(|_| takes_colors);
        for v in values.iter().chain(colors) {
            // A variable is its token's name in `(--…)`.
            match v.strip_prefix("(--").and_then(|t| t.strip_suffix(')')) {
                Some(t) => try_class(format!("{p}-{v}"), Some((&format!("{p}-(--{{}})"), t))),
                None => try_class(format!("{p}-{v}"), Some((p, v))),
            }
        }
        for t in &variables {
            if known(&format!("{p}-{t}")) {
                try_class(format!("{p}-(--{t})"), Some((&format!("{p}-(--{{}})"), t)));
            }
        }
        // Negative values, where the sign does something: a sign the
        // utility ignores is no API worth keeping.
        let negates = ["4", "1", "[13px]", "1/2", "px", "full"].iter().any(|v| {
            let (pos, neg) = (format!("{p}-{v}"), format!("-{p}-{v}"));
            match (resolver.resolve_line(&pos), resolver.resolve_line(&neg)) {
                (Some(pos), Some(neg)) => pos != neg,
                _ => false,
            }
        });
        if negates {
            let neg = format!("-{p}");
            for v in &values {
                try_class(format!("-{p}-{v}"), Some((&neg, v)));
            }
        }
    }

    // Variants, on one utility each (and on a theme token, whose rules
    // are made per appearance).
    let mut variants: BTreeSet<String> = literals(section(&resolver_src, "fn variant(", "}"))
        .into_iter()
        .collect();
    for n in &names {
        variants.insert(n.to_string());
        variants.insert(format!("max-{n}"));
    }
    for s in literals(section(&resolver_src, "fn variant(", "}")) {
        variants.insert(format!("group-{s}"));
        variants.insert(format!("group-{s}/item"));
    }
    variants.extend(["min-[600px]", "max-[600px]"].map(String::from));
    for v in &variants {
        try_class(format!("{v}:p-4"), None);
    }
    // A theme token, whose rules are made per appearance, under a few.
    if let Some(t) = tokens.first() {
        for v in ["dark", "light", "hover", "group-hover", "md"] {
            try_class(format!("{v}:bg-{t}"), None);
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
        try_class(c.into(), None);
    }
    out
}

/// A value of a scale: what it is at the base, and in each context where
/// it differs (`dark`).
#[derive(Clone, PartialEq)]
struct Value {
    base: String,
    contexts: Vec<(String, String)>,
}

/// A scale: names and their values, in order.
struct Scale {
    name: &'static str,
    entries: Vec<(String, Value)>,
}

/// The scales utilities share: what a probe utility sets for the values
/// it takes, or numbers as they are written.
const SCALES: &[(&str, Source)] = &[
    ("spacing", Source::Probe("p", Values::Any)),
    ("fraction", Source::Probe("w", Values::Fractions)),
    ("size", Source::Probe("w", Values::NotSpacing)),
    ("color", Source::Probe("bg", Values::Colors)),
    ("radius", Source::Probe("rounded", Values::Any)),
    ("blur", Source::Probe("blur", Values::Any)),
    ("weight", Source::Probe("font", Values::Any)),
    ("tracking", Source::Probe("tracking", Values::Any)),
    ("leading", Source::Probe("leading", Values::Any)),
    ("container", Source::Probe("max-w", Values::NotSpacing)),
    ("arbitrary", Source::Probe("w", Values::Arbitrary)),
    ("n", Source::Numbers),
    ("int", Source::Integers),
    ("percent", Source::Hundredths),
    ("ratio", Source::Ratios),
];

#[derive(Clone, Copy)]
enum Source {
    /// What the utility sets for each value it takes.
    Probe(&'static str, Values),
    /// [`NUMBERS`], as written.
    Numbers,
    /// Those of [`NUMBERS`] that are integers.
    Integers,
    /// [`FRACTIONS`], divided.
    Ratios,
    /// [`NUMBERS`] as percentages: hundredths (`50`: 0.5).
    Hundredths,
}

#[derive(Clone, Copy)]
enum Values {
    Any,
    Fractions,
    Colors,
    /// Not a fraction, a color or a value `p-*` takes.
    NotSpacing,
    /// Arbitrary values (`[13px]`).
    Arbitrary,
}

/// The parts of a line: the base, then `(context, decls)`.
fn segments(line: &str) -> (&str, Vec<(&str, &str)>) {
    let mut parts = line.split(" | ");
    let base = parts.next().unwrap_or_default();
    let contexts = parts
        .map(|p| p.split_once(": ").unwrap_or((p, "")))
        .collect();
    (base, contexts)
}

/// What a probe's line says of its value: `(key, value)` when the line
/// sets one key (in each context), to a value a scale can hold.
fn probe_value(line: &str) -> Option<(String, Value)> {
    let fit = |s: &str| !s.is_empty() && !s.contains([' ', ';', '=', '|', '{', '}']);
    let one = |decls: &str| -> Option<(String, String)> {
        let (k, v) = decls.split_once(": ")?;
        (!decls.contains("; ") && fit(v)).then(|| (k.to_owned(), v.to_owned()))
    };
    let (base, contexts) = segments(line);
    let (key, base) = one(base)?;
    let mut value = Value {
        base,
        contexts: Vec::new(),
    };
    for (label, decls) in contexts {
        let (k, v) = one(decls)?;
        if k != key {
            return None;
        }
        value.contexts.push((label.replace(' ', "_"), v));
    }
    Some((key, value))
}

/// The scales, from what their probes set.
fn scales(resolver: &Resolver, enumerated: &Enumerated) -> Vec<Scale> {
    let tokens: BTreeSet<&str> = resolver.theme.token_names().into_iter().collect();
    let values_of = |probe: &str| -> Vec<String> {
        enumerated
            .values()
            .filter_map(|made| {
                let (p, v) = made.as_ref()?;
                (p == probe).then(|| v.clone())
            })
            .collect()
    };
    let mut out = Vec::new();
    for &(name, source) in SCALES {
        let (probe, which) = match source {
            Source::Probe(probe, which) => (probe, which),
            Source::Ratios => {
                let entries = FRACTIONS
                    .iter()
                    .map(|f| {
                        let (a, b) = f.split_once('/').unwrap();
                        let ratio = a.parse::<f32>().unwrap() / b.parse::<f32>().unwrap();
                        let value = Value {
                            base: style::num(ratio),
                            contexts: Vec::new(),
                        };
                        (f.to_string(), value)
                    })
                    .collect();
                out.push(Scale { name, entries });
                continue;
            }
            Source::Numbers | Source::Integers | Source::Hundredths => {
                let entries = NUMBERS
                    .iter()
                    .filter(|n| !matches!(source, Source::Integers) || !n.contains('.'))
                    .map(|n| {
                        let base = match source {
                            Source::Hundredths => style::num(n.parse::<f32>().unwrap() / 100.0),
                            _ => n.to_string(),
                        };
                        let value = Value {
                            base,
                            contexts: Vec::new(),
                        };
                        (n.to_string(), value)
                    })
                    .collect();
                out.push(Scale { name, entries });
                continue;
            }
        };
        let candidates: Vec<String> = values_of(probe)
            .into_iter()
            .filter(|v| {
                // Variables are their tokens' (`bg-(--{token})`), and the
                // tokens a scale of their own.
                if v.starts_with('(') || tokens.contains(v.split('/').next().unwrap_or("")) {
                    return false;
                }
                let fraction = FRACTIONS.contains(&v.as_str());
                let color = resolver.known(&format!("bg-{v}")).is_some();
                let spacing = resolver.known(&format!("p-{v}")).is_some();
                match which {
                    Values::Any => !fraction && !color,
                    Values::Fractions => fraction,
                    Values::Colors => color,
                    Values::NotSpacing => !fraction && !color && !spacing,
                    Values::Arbitrary => v.starts_with('[') && !color,
                }
            })
            .collect();
        let mut candidates = candidates;
        candidates.sort_by(|a, b| order(a).partial_cmp(&order(b)).unwrap());
        let mut entries: Vec<(String, String, Value)> = candidates
            .into_iter()
            .filter_map(|v| {
                let line = resolver.resolve_line(&format!("{probe}-{v}"))?;
                let (key, value) = probe_value(&line)?;
                Some((v, key, value))
            })
            .collect();
        // One key and one shape: the most common.
        let shape = |v: &Value| {
            v.contexts
                .iter()
                .map(|(l, _)| l.clone())
                .collect::<Vec<_>>()
        };
        let mut counts: HashMap<(String, Vec<String>), usize> = HashMap::new();
        for (_, key, value) in &entries {
            *counts.entry((key.clone(), shape(value))).or_default() += 1;
        }
        let Some(best) = counts
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(k, _)| k)
        else {
            continue;
        };
        entries.retain(|(_, key, value)| (key.clone(), shape(value)) == best);
        // Colors with an opacity (`red-500/50`) are a scale of their own:
        // utilities read `/` otherwise (`text-sm/6`).
        let (alpha, entries): (Vec<_>, Vec<_>) =
            entries.into_iter().partition(|(n, ..)| n.contains('/'));
        for (name, entries) in [(name, entries), (alpha_name(name), alpha)] {
            if entries.len() >= 3 {
                out.push(Scale {
                    name,
                    entries: entries.into_iter().map(|(n, _, v)| (n, v)).collect(),
                });
            }
        }
    }
    // Colors that read otherwise in dark mode (the theme's tokens) are a
    // scale of their own.
    // The theme's tokens: those that read otherwise in dark mode, those
    // that do not, and with an opacity.
    let entries: Vec<(String, Value)> = enumerated
        .values()
        .filter_map(|made| {
            let (p, v) = made.as_ref()?;
            (p == "bg" && tokens.contains(v.split('/').next().unwrap_or(""))).then_some(v)
        })
        .filter_map(|v| {
            let (_, value) = probe_value(&resolver.resolve_line(&format!("bg-{v}"))?)?;
            Some((v.clone(), value))
        })
        .collect();
    let mut groups: [(&'static str, Vec<(String, Value)>); 3] = [
        ("token", Vec::new()),
        ("token-flat", Vec::new()),
        ("token-alpha", Vec::new()),
    ];
    for (n, v) in entries {
        let i = if n.contains('/') {
            2
        } else if v.contexts.is_empty() {
            1
        } else {
            0
        };
        groups[i].1.push((n, v));
    }
    for (name, entries) in groups {
        if entries.len() >= 3 {
            out.push(Scale { name, entries });
        }
    }
    out
}

/// Where a name sorts in its scale: numbers by value, then the rest by
/// name.
fn order(name: &str) -> (bool, f32, &str) {
    match name.parse::<f32>() {
        Ok(n) => (false, n, ""),
        Err(_) => (true, 0.0, name),
    }
}

/// The name of the scale of `scale`'s values with an opacity.
fn alpha_name(scale: &'static str) -> &'static str {
    match scale {
        "color" => "color-alpha",
        _ => "alpha",
    }
}

/// `s` with each `pat` that stands alone (not part of a longer number,
/// name or color; a unit may follow it) replaced by `with`.
fn replace_bounded(s: &str, pat: &str, with: &str) -> String {
    if pat.is_empty() {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(pat) {
        let before = rest[..i].chars().next_back().or(out.chars().next_back());
        let after = rest[i + pat.len()..].chars().next();
        let alone = before.is_none_or(|c| !c.is_ascii_alphanumeric() && !".#".contains(c))
            && after.is_none_or(|c| !c.is_ascii_digit() && c != '.');
        out.push_str(&rest[..i]);
        out.push_str(if alone { with } else { pat });
        rest = &rest[i + pat.len()..];
    }
    out.push_str(rest);
    out
}

/// `line` with `value` as placeholders of `scale`.
fn templatize(line: &str, scale: &str, value: &Value) -> String {
    let mut t = replace_bounded(line, &value.base, &format!("{{{scale}}}"));
    for (label, v) in &value.contexts {
        t = replace_bounded(&t, v, &format!("{{{scale}@{label}}}"));
    }
    t
}

/// `template` with the placeholders of `scale` as `value`.
fn substitute(template: &str, scale: &str, value: &Value) -> String {
    let mut out = template.replace(&format!("{{{scale}}}"), &value.base);
    for (label, v) in &value.contexts {
        out = out.replace(&format!("{{{scale}@{label}}}"), v);
    }
    out
}

/// The class `utility` makes of `value`: `<utility>-<value>`, or
/// `utility` with `{}` as the value (`bg-(--{})`).
fn class_of(utility: &str, value: &str) -> String {
    if utility.contains("{}") {
        utility.replace("{}", value)
    } else {
        format!("{utility}-{value}")
    }
}

/// A rule: `<utility>-{<scale>}` and the template of its lines.
struct Rule {
    utility: String,
    scale: &'static str,
    template: String,
}

/// The record's sections, parsed.
#[derive(Default)]
struct Record {
    scales: BTreeMap<String, Vec<(String, Value)>>,
    rules: Vec<(String, String)>,
    classes: BTreeMap<String, String>,
}

impl Record {
    /// Whether `text` is a record of this format (one before it listed
    /// every class, with Rust's `{:?}` of the style).
    fn is_rules(text: &str) -> bool {
        text.lines().any(|l| l == "[rules]")
    }

    fn parse(text: &str) -> Self {
        let mut out = Record::default();
        let mut section = "";
        for line in text.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') && !line.contains(' ') {
                section = line;
                continue;
            }
            let (head, rest) = line.split_once(' ').unwrap_or((line, ""));
            match section {
                "[scales]" => {
                    let scale = out.scales.entry(head.to_owned()).or_default();
                    for entry in rest.split(' ').filter(|e| !e.is_empty()) {
                        let mut parts = entry.split(';');
                        let (name, base) = parts
                            .next()
                            .and_then(|p| p.split_once('='))
                            .unwrap_or_else(|| panic!("a scale's entry `{entry}`"));
                        let contexts = parts
                            .map(|p| {
                                let (l, v) = p
                                    .split_once('=')
                                    .unwrap_or_else(|| panic!("a scale's entry `{entry}`"));
                                (l.to_owned(), v.to_owned())
                            })
                            .collect();
                        scale.push((
                            name.to_owned(),
                            Value {
                                base: base.to_owned(),
                                contexts,
                            },
                        ));
                    }
                }
                "[rules]" => out.rules.push((head.to_owned(), rest.to_owned())),
                "[classes]" => {
                    out.classes.insert(head.to_owned(), rest.to_owned());
                }
                _ => panic!("api/tailwind.txt: a line outside a section: `{line}`"),
            }
        }
        out
    }

    /// Every class and its line.
    fn expand(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (pattern, template) in &self.rules {
            let (open, close) = (pattern.find('{'), pattern.find('}'));
            let (Some(open), Some(close)) = (open, close) else {
                panic!("api/tailwind.txt: a rule without a scale: `{pattern}`");
            };
            let scale = &pattern[open + 1..close];
            let entries = self
                .scales
                .get(scale)
                .unwrap_or_else(|| panic!("api/tailwind.txt: no scale `{scale}`"));
            for (name, value) in entries {
                let class = format!("{}{name}{}", &pattern[..open], &pattern[close + 1..]);
                out.entry(class)
                    .or_insert_with(|| substitute(template, scale, value));
            }
        }
        for (class, line) in &self.classes {
            if line == "!" {
                out.remove(class);
            } else {
                out.insert(class.clone(), line.clone());
            }
        }
        out
    }
}

/// The rules of `utility` (`p`, `-m`), from the classes made of it: each
/// scale whose names it takes, as the scale's values, for most of them.
fn infer(
    utility: &str,
    made: &BTreeSet<String>,
    scales: &[Scale],
    line: &mut dyn FnMut(&str) -> Option<String>,
) -> Vec<Rule> {
    let mut rules = Vec::new();
    // The classes earlier rules name: theirs, whatever later ones say.
    let mut named: BTreeSet<String> = BTreeSet::new();
    let mut remaining = made.clone();
    loop {
        let mut best: Option<(usize, Rule, Vec<String>)> = None;
        for scale in scales {
            let entries: Vec<&(String, Value)> = scale
                .entries
                .iter()
                .filter(|(n, _)| !named.contains(&class_of(utility, n)))
                .collect();
            let seeds: Vec<&&(String, Value)> = entries
                .iter()
                .filter(|(n, _)| remaining.contains(n))
                .collect();
            if seeds.len() < 3 {
                continue;
            }
            let mut tried = BTreeSet::new();
            for (n, v) in seeds.iter().take(8) {
                let Some(l) = line(&class_of(utility, n)) else {
                    continue;
                };
                let template = templatize(&l, scale.name, v);
                if !tried.insert(template.clone()) {
                    continue;
                }
                let covered: Vec<String> = entries
                    .iter()
                    .filter(|(n, v)| {
                        line(&class_of(utility, n))
                            .is_some_and(|l| l == substitute(&template, scale.name, v))
                    })
                    .map(|(n, _)| n.clone())
                    .collect();
                // Each name the rule gets wrong is a line of its own.
                let misses = entries.len() - covered.len();
                if covered.len() < 3 || covered.len() < 4 * misses {
                    continue;
                }
                let score = covered.len() - misses;
                if best.as_ref().is_none_or(|(c, ..)| score > *c) {
                    let rule = Rule {
                        utility: utility.to_owned(),
                        scale: scale.name,
                        template,
                    };
                    let names = entries.iter().map(|(n, _)| n.clone()).collect();
                    best = Some((score, rule, names));
                }
            }
        }
        let Some((_, rule, names)) = best else {
            break;
        };
        for n in names {
            remaining.remove(&n);
            named.insert(class_of(utility, &n));
        }
        rules.push(rule);
    }
    rules
}

/// `api/tailwind.txt` as this checkout makes it: the enumerated classes,
/// and those the record already holds that still resolve, so the record
/// only grows when a change to the resolver hides a class from the
/// enumeration.
pub(super) fn record() -> String {
    let resolver = Resolver::new();
    let enumerated = classes(&resolver);
    let mut lines: HashMap<String, Option<String>> = HashMap::new();
    let mut line = |class: &str| -> Option<String> {
        lines
            .entry(class.to_owned())
            .or_insert_with(|| resolver.resolve_line(class))
            .clone()
    };

    let mut truth: BTreeMap<String, String> = BTreeMap::new();
    for class in enumerated.keys() {
        truth.insert(
            class.clone(),
            line(class).expect("enumerated classes are known"),
        );
    }
    let recorded = std::fs::read_to_string(super::api_dir().join("tailwind.txt"))
        .unwrap_or_default()
        .replace("\r\n", "\n");
    if Record::is_rules(&recorded) {
        for class in Record::parse(&recorded).expand().into_keys() {
            if let Some(l) = line(&class) {
                truth.insert(class, l);
            }
        }
    }

    let scales = scales(&resolver, &enumerated);
    let mut made: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (p, v) in enumerated.values().flatten() {
        made.entry(p).or_default().insert(v.clone());
    }
    let mut rules = Vec::new();
    for (utility, values) in &made {
        rules.extend(infer(utility, values, &scales, &mut line));
    }

    let mut record = Record::default();
    let used: BTreeSet<&str> = rules.iter().map(|r| r.scale).collect();
    for scale in &scales {
        if used.contains(scale.name) {
            record
                .scales
                .insert(scale.name.to_owned(), scale.entries.clone());
        }
    }
    record.rules = rules
        .iter()
        .map(|r| {
            (
                class_of(&r.utility, &format!("{{{}}}", r.scale)),
                r.template.clone(),
            )
        })
        .collect();
    let by_rules = record.expand();
    for class in by_rules.keys() {
        match line(class) {
            Some(real) => {
                truth.insert(class.clone(), real);
            }
            None => {
                record.classes.insert(class.clone(), "!".into());
            }
        }
    }
    for (class, l) in &truth {
        if by_rules.get(class) != Some(l) {
            record.classes.insert(class.clone(), l.clone());
        }
    }
    debug_assert_eq!(record.expand(), truth);
    write(&record, &scales)
}

const HEADER: &str = "\
# The Tailwind classes apps name and what each sets, as apps write styles
# (NSH's style keys, CSS's values), then ` | <context>: ...` where a context
# (dark, a window width, a state, ...) changes it. Generated by the
# engine's `compat` tests (`cargo xtask api`); a class may be added, never
# removed or changed (docs/dev/compatibility.md).
#
# [scales]   <scale> <name>=<value>...: the values utilities share
# [rules]    <utility>-{<scale>} <line>: the utility takes every name of the
#            scale, its line with {<scale>} as the name's value
# [classes]  every other class, `!` for one a rule names that is no class
";

fn write(record: &Record, scales: &[Scale]) -> String {
    let mut out = String::from(HEADER);
    out.push_str("\n[scales]\n");
    for scale in scales {
        let Some(entries) = record.scales.get(scale.name) else {
            continue;
        };
        // Names sharing a stem (`red-50`, `red-100`) on one line, a dozen
        // at most.
        let stem = |n: &str| n.rsplit_once('-').map_or("", |(s, _)| s).to_owned();
        let mut line: Vec<String> = Vec::new();
        let mut current = None;
        let flush = |line: &mut Vec<String>, out: &mut String| {
            if !line.is_empty() {
                let _ = writeln!(out, "{} {}", scale.name, line.join(" "));
                line.clear();
            }
        };
        for (name, value) in entries {
            let s = stem(name);
            let most = if value.contexts.is_empty() { 12 } else { 4 };
            if current.as_ref() != Some(&s) || line.len() >= most {
                flush(&mut line, &mut out);
                current = Some(s);
            }
            let mut entry = format!("{name}={}", value.base);
            for (label, v) in &value.contexts {
                let _ = write!(entry, ";{label}={v}");
            }
            line.push(entry);
        }
        flush(&mut line, &mut out);
    }
    out.push_str("\n[rules]\n");
    for (pattern, template) in &record.rules {
        let _ = writeln!(out, "{pattern} {template}");
    }
    out.push_str("\n[classes]\n");
    for (class, line) in &record.classes {
        let _ = writeln!(out, "{class} {line}");
    }
    out
}

/// The classes of a record and their lines; a record of the format before
/// rules only names its classes (its lines read `?`).
fn lines(text: &str) -> BTreeMap<String, String> {
    if Record::is_rules(text) {
        return Record::parse(text).expand();
    }
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            (
                l.split_once(' ').map_or(l, |(c, _)| c).to_owned(),
                "?".into(),
            )
        })
        .collect()
}

/// What the record `base` had that `now` removes or changes, less the
/// classes `closed` (the entries of `api/closed.txt`) closes.
pub(super) fn breaks(base: &str, now: &str, closed: &[String]) -> Vec<String> {
    let resolver = Resolver::new();
    breaks_with(base, now, closed, &|class| resolver.resolve_line(class))
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
        let new = match now.get(&class) {
            Some(new) => Some(new.clone()),
            None => resolve(&class),
        };
        match new {
            None => breaks.push(format!("{entry}: removed")),
            // The format before rules kept Rust's `{:?}` of the style: the
            // class must still be one.
            Some(_) if old == "?" => {}
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
    let record = |classes: &str| format!("[scales]\n[rules]\n[classes]\n{classes}");
    let base = record("p-4 padding: 16\nbg-red-500 backgroundColor: red\n");
    assert!(breaks_with(&base, &base, &[], &none).is_empty());
    let more = record("p-4 padding: 16\nbg-red-500 backgroundColor: red\np-5 padding: 20\n");
    assert!(breaks_with(&base, &more, &[], &none).is_empty());
    let now = record("p-4 padding: 17\n");
    assert_eq!(
        breaks_with(&base, &now, &[], &none),
        [
            "tailwind bg-red-500: removed",
            "tailwind p-4: changed (padding: 16 → padding: 17)",
        ]
    );
    let closed = super::closed("tailwind bg-red-500 -- unsafe\ntailwind p -- not p-4\n");
    assert_eq!(
        breaks_with(&base, &now, &closed, &none),
        ["tailwind p-4: changed (padding: 16 → padding: 17)"]
    );
    // A class the enumeration misses is resolved.
    let found = |c: &str| (c == "bg-red-500").then(|| "backgroundColor: red".to_owned());
    assert_eq!(breaks_with(&base, &now, &[], &found).len(), 1);
    // Rules and scales: a scale's value that changes changes every class
    // of every rule that takes it.
    let ruled = |four: &str| {
        format!(
            "[scales]\nspacing 1=4 4={four}\n[rules]\np-{{spacing}} padding: {{spacing}}\n\
             m-{{spacing}} margin: {{spacing}}\n[classes]\nm-1 margin: 5\np-9 !\n"
        )
    };
    assert_eq!(
        lines(&ruled("16")),
        [
            ("m-1", "margin: 5"),
            ("m-4", "margin: 16"),
            ("p-1", "padding: 4"),
            ("p-4", "padding: 16"),
        ]
        .map(|(c, l)| (c.to_owned(), l.to_owned()))
        .into()
    );
    assert!(breaks_with(&ruled("16"), &ruled("16"), &[], &none).is_empty());
    assert_eq!(
        breaks_with(&ruled("16"), &ruled("20"), &[], &none),
        [
            "tailwind m-4: changed (margin: 16 → margin: 20)",
            "tailwind p-4: changed (padding: 16 → padding: 20)",
        ]
    );
    // A record before rules: its classes must stay classes.
    assert!(breaks_with("p-4 layout.padding=16\n", &now, &[], &none).is_empty());
    assert_eq!(
        breaks_with("p-5 layout.padding=20\n", &now, &[], &none),
        ["tailwind p-5: removed"]
    );
    // The real resolver.
    let real = breaks(&record("p-4 padding: 17\n"), "", &[]);
    assert_eq!(real, ["tailwind p-4: changed (padding: 17 → padding: 16)"]);
    assert_eq!(
        breaks(&record("p-nope x\n"), "", &[]),
        ["tailwind p-nope: removed"]
    );
}

/// The record shows what classes do, and the enumeration finds every kind.
#[test]
fn tailwind_record_resolves() {
    let record = record();
    let lines = lines(&record);
    let line = |c: &str| {
        lines
            .get(c)
            .unwrap_or_else(|| panic!("{c} is not recorded"))
            .as_str()
    };
    assert_eq!(line("p-4"), "padding: 16");
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
    // Every recorded class resolves to its line.
    let resolver = Resolver::new();
    for (class, l) in &lines {
        assert_eq!(resolver.resolve_line(class).as_ref(), Some(l), "{class}");
    }
}

/// The record misses nothing a class does: two classes it writes alike
/// set the same fields of `Style` and `ClassProps` (Rust's `{:?}` of them
/// is the oracle here, never in the record), and a class that sets one
/// writes something.
#[test]
fn tailwind_record_is_faithful() {
    let resolver = Resolver::new();
    // -0 is 0 to apps (and to `PartialEq`).
    let zero = |s: &str| {
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(i) = rest.find('-') {
            out.push_str(&rest[..i]);
            let number = &rest[i + 1..];
            let len = number
                .find(|c: char| !c.is_ascii_digit() && c != '.')
                .unwrap_or(number.len());
            if len == 0 || number[..len].parse::<f32>() != Ok(0.0) {
                out.push('-');
            }
            rest = number;
        }
        out.push_str(rest);
        out
    };
    let empty = zero(&format!(
        "{:?} {:?}",
        Style::default(),
        ClassProps::default()
    ));
    let mut seen: HashMap<String, (String, String)> = HashMap::new();
    for class in classes(&resolver).keys() {
        let list = resolver.known(class).unwrap();
        for place in resolver.places_of(class, &list) {
            let (style, props) = resolver.resolve(&list, place);
            let debug = zero(&format!("{style:?} {props:?}"));
            let set = debug != empty;
            let written = resolver.style(&list, place);
            assert!(
                !set || written != "-",
                "{class} at {}: sets {debug} and writes nothing",
                place.label
            );
            let at = format!("{class} at {}", place.label);
            if let Some((other, d)) = seen.get(&written)
                && *d != debug
            {
                panic!("{at} and {other} both write `{written}`, but set\n{debug}\n{d}");
            }
            seen.insert(written, (at, debug));
        }
    }
}
