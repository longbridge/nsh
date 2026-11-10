//! The looks an app declares for `nsh-base`'s components: per component, a
//! base look and looks per named variant and size, as typed [`Styled`]
//! closures ([`set_component_style`]) or Tailwind class strings (the
//! `components` of nsh.json, [`set_component_classes`]). A component asks
//! for the looks of the variant and size its page picked by name
//! ([`ComponentStyle::resolve`]); nothing here has a look of its own.
//!
//! The store is a per-surface global signal (as the translations are), so
//! several engines in one process keep their own, reads subscribe and
//! writes notify.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use super::{SharedString, StyleRefinement, global};

/// A typed look: the closure refines the style it is given.
pub(crate) type Refine = Rc<dyn Fn(StyleRefinement) -> StyleRefinement>;

/// One look of a component: typed styles, Tailwind classes, or both.
#[derive(Clone, Default)]
pub(crate) struct Look {
    /// The typed refinement, applied first.
    pub(crate) typed: Option<Refine>,
    /// Tailwind classes, applied after the typed ones.
    pub(crate) classes: Option<SharedString>,
}

/// How one component looks: a base look and looks per named variant and
/// size, with the variant and size used when the page picks none.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct ComponentStyle {
    base: Look,
    variants: BTreeMap<String, Look>,
    sizes: BTreeMap<String, Look>,
    default_variant: Option<String>,
    default_size: Option<String>,
}

impl ComponentStyle {
    /// A style with no looks; add them with the builder methods.
    pub fn new() -> Self {
        Self::default()
    }

    /// The typed look every instance has.
    pub fn base(mut self, f: impl Fn(StyleRefinement) -> StyleRefinement + 'static) -> Self {
        self.base.typed = Some(Rc::new(f));
        self
    }

    /// The typed look of the variant `name`.
    pub fn variant(
        mut self,
        name: &str,
        f: impl Fn(StyleRefinement) -> StyleRefinement + 'static,
    ) -> Self {
        self.variants.entry(name.to_owned()).or_default().typed = Some(Rc::new(f));
        self
    }

    /// The typed look of the size `name`.
    pub fn size(
        mut self,
        name: &str,
        f: impl Fn(StyleRefinement) -> StyleRefinement + 'static,
    ) -> Self {
        self.sizes.entry(name.to_owned()).or_default().typed = Some(Rc::new(f));
        self
    }

    /// The Tailwind classes every instance has.
    pub fn base_classes(mut self, classes: &str) -> Self {
        self.base.classes = Some(classes.to_owned().into());
        self
    }

    /// The Tailwind classes of the variant `name`.
    pub fn variant_classes(mut self, name: &str, classes: &str) -> Self {
        self.variants.entry(name.to_owned()).or_default().classes = Some(classes.to_owned().into());
        self
    }

    /// The Tailwind classes of the size `name`.
    pub fn size_classes(mut self, name: &str, classes: &str) -> Self {
        self.sizes.entry(name.to_owned()).or_default().classes = Some(classes.to_owned().into());
        self
    }

    /// The variant an instance that picks none has.
    pub fn default_variant(mut self, name: &str) -> Self {
        self.default_variant = Some(name.to_owned());
        self
    }

    /// The size an instance that picks none has.
    pub fn default_size(mut self, name: &str) -> Self {
        self.default_size = Some(name.to_owned());
        self
    }

    /// The looks of an instance, in the order they apply: the base look,
    /// the look of `variant` (the default variant when `None`), the look of
    /// `size` (likewise). A name nothing declares adds nothing.
    fn looks<'a>(
        &'a self,
        variant: Option<&'a str>,
        size: Option<&'a str>,
    ) -> impl Iterator<Item = &'a Look> {
        let variant = variant.or(self.default_variant.as_deref());
        let size = size.or(self.default_size.as_deref());
        [
            Some(&self.base),
            variant.and_then(|n| self.variants.get(n)),
            size.and_then(|n| self.sizes.get(n)),
        ]
        .into_iter()
        .flatten()
    }

    /// The typed closures of an instance's looks ([`Self::looks`]), in the
    /// order they apply.
    pub(crate) fn typed_looks<'a>(
        &'a self,
        variant: Option<&'a str>,
        size: Option<&'a str>,
    ) -> impl Iterator<Item = &'a Refine> {
        self.looks(variant, size)
            .filter_map(|look| look.typed.as_ref())
    }

    /// The class strings of an instance's looks ([`Self::looks`]) joined
    /// with spaces; a single string is shared, not copied.
    pub(crate) fn classes(
        &self,
        variant: Option<&str>,
        size: Option<&str>,
    ) -> Option<SharedString> {
        let mut strings = self
            .looks(variant, size)
            .filter_map(|look| look.classes.as_ref());
        let first = strings.next()?;
        let Some(second) = strings.next() else {
            return Some(first.clone());
        };
        let mut joined = format!("{} {}", &**first, &**second);
        for more in strings {
            joined.push(' ');
            joined.push_str(more);
        }
        Some(joined.into())
    }

    /// [`Self::typed_looks`] and [`Self::classes`] at once.
    #[cfg(test)]
    fn resolve(
        &self,
        variant: Option<&str>,
        size: Option<&str>,
    ) -> (Vec<Refine>, Option<SharedString>) {
        let typed = self.typed_looks(variant, size).cloned().collect();
        (typed, self.classes(variant, size))
    }

    /// Warns, once per surface for each component and name, of a variant
    /// or size an instance picks (or the defaults name) that `self` does
    /// not declare: it applies nothing for that name.
    pub(crate) fn warn_undeclared(
        &self,
        component: &str,
        variant: Option<&str>,
        size: Option<&str>,
    ) {
        let variant = variant.or(self.default_variant.as_deref());
        let size = size.or(self.default_size.as_deref());
        let missing = [
            (
                "variant",
                variant.filter(|n| !self.variants.contains_key(*n)),
            ),
            ("size", size.filter(|n| !self.sizes.contains_key(*n))),
        ];
        if missing.iter().all(|(_, name)| name.is_none()) {
            return;
        }
        let warned = global::<Warned>().with_untracked(|w| w.0.clone());
        for (kind, name) in missing {
            let Some(name) = name else { continue };
            let fresh = warned
                .borrow_mut()
                .insert((component.to_owned(), kind, name.to_owned()));
            if fresh {
                log::warn!("`{component}` declares no {kind} `{name}`: it applies no look");
            }
        }
    }

    /// Whether it declares nothing.
    fn is_empty(&self) -> bool {
        self.base.typed.is_none()
            && self.base.classes.is_none()
            && self.variants.is_empty()
            && self.sizes.is_empty()
            && self.default_variant.is_none()
            && self.default_size.is_none()
    }

    /// `self` (the app's Rust declaration) and `json` (nsh.json's) as one.
    /// Per name both apply: the typed closures, then the classes, Rust's
    /// string followed by the JSON's (the later wins at equal rank). The
    /// JSON's defaults win when it has them.
    fn merged(&self, json: &Self) -> Self {
        fn join(rust: &Look, json: &Look) -> Look {
            let classes = match (&rust.classes, &json.classes) {
                (Some(r), Some(j)) => Some(format!("{r} {j}").into()),
                (r, j) => r.clone().or_else(|| j.clone()),
            };
            Look {
                typed: rust.typed.clone().or_else(|| json.typed.clone()),
                classes,
            }
        }
        fn zip(
            rust: &BTreeMap<String, Look>,
            json: &BTreeMap<String, Look>,
        ) -> BTreeMap<String, Look> {
            let mut out = rust.clone();
            for (name, look) in json {
                let merged = join(&out.get(name).cloned().unwrap_or_default(), look);
                out.insert(name.clone(), merged);
            }
            out
        }
        Self {
            base: join(&self.base, &json.base),
            variants: zip(&self.variants, &json.variants),
            sizes: zip(&self.sizes, &json.sizes),
            default_variant: json
                .default_variant
                .clone()
                .or_else(|| self.default_variant.clone()),
            default_size: json
                .default_size
                .clone()
                .or_else(|| self.default_size.clone()),
        }
    }
}

/// The surface's declared looks, by component: the Rust declarations, the
/// nsh.json ones, and the two merged, made again only when either is
/// written, so a component's reads share one copy.
#[derive(Clone, Default)]
struct Store {
    typed: BTreeMap<String, ComponentStyle>,
    classes: BTreeMap<String, ComponentStyle>,
    merged: BTreeMap<String, Rc<ComponentStyle>>,
}

impl Store {
    /// Makes `component`'s merged look again from both declarations.
    fn merge(&mut self, component: &str) {
        let merged = match (self.typed.get(component), self.classes.get(component)) {
            (None, None) => None,
            (Some(typed), None) => Some(typed.clone()),
            (None, Some(json)) => Some(json.clone()),
            (Some(rust), Some(json)) => Some(rust.merged(json)),
        };
        match merged {
            Some(style) => {
                self.merged.insert(component.to_owned(), Rc::new(style));
            }
            None => {
                self.merged.remove(component);
            }
        }
    }
}

/// The undeclared names already warned of on the surface: a set behind a
/// shared cell, so recording one notifies nothing.
#[derive(Clone, Default)]
struct Warned(Rc<RefCell<BTreeSet<(String, &'static str, String)>>>);

/// Declares how `component` (a component's name, `"button"`) looks, in
/// typed styles. It replaces the typed looks declared for it before.
/// Declare before the first render: a later declaration reaches the
/// components built while a look was declared for `component` (they
/// update), not those built when none was, which take no declared look.
pub fn set_component_style(component: &'static str, style: ComponentStyle) {
    global::<Store>().update(|store| {
        store.typed.insert(component.to_owned(), style);
        store.merge(component);
    });
}

/// The looks declared for `component` (typed and classes, merged),
/// subscribing the reader to changes: one shared copy until either
/// declaration is written again.
pub(crate) fn component_style_of(component: &str) -> Option<Rc<ComponentStyle>> {
    global::<Store>().with(|store| store.merged.get(component).cloned())
}

/// Whether any look is declared for `component`, without subscribing.
pub(crate) fn has_component_style(component: &str) -> bool {
    global::<Store>().with_untracked(|store| store.merged.contains_key(component))
}

/// Replaces every class look with those of `json`, the `components` of
/// nsh.json: an object by component name, each with `base` (classes),
/// `variants` and `sizes` (objects of classes by name) and `defaults`
/// (`variant`, `size`). Lenient: what does not fit is left out and named in
/// the warnings returned. A malformed top level (not JSON, not an object)
/// clears all class looks and returns a warning.
pub(crate) fn set_component_classes(json: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut classes = BTreeMap::new();
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Object(components)) => {
            for (name, spec) in components {
                if let Some(style) = parse_component(&name, &spec, &mut warnings)
                    && !style.is_empty()
                {
                    classes.insert(name, style);
                }
            }
        }
        Ok(_) => warnings.push("components: expected an object".into()),
        Err(e) => warnings.push(format!("components: {e}")),
    }
    global::<Store>().update(|store| {
        let old = std::mem::replace(&mut store.classes, classes);
        let touched: BTreeSet<String> = old
            .into_keys()
            .chain(store.classes.keys().cloned())
            .collect();
        for component in touched {
            store.merge(&component);
        }
    });
    warnings
}

fn parse_component(
    name: &str,
    spec: &serde_json::Value,
    warnings: &mut Vec<String>,
) -> Option<ComponentStyle> {
    let Some(spec) = spec.as_object() else {
        warnings.push(format!("components.{name}: expected an object"));
        return None;
    };
    let mut style = ComponentStyle::default();
    for (key, value) in spec {
        match key.as_str() {
            "base" => match value.as_str() {
                Some(c) => style = style.base_classes(c),
                None => warnings.push(format!("components.{name}.base: expected a string")),
            },
            "variants" | "sizes" => {
                let Some(map) = value.as_object() else {
                    warnings.push(format!("components.{name}.{key}: expected an object"));
                    continue;
                };
                for (entry, classes) in map {
                    let Some(classes) = classes.as_str() else {
                        warnings.push(format!(
                            "components.{name}.{key}.{entry}: expected a string"
                        ));
                        continue;
                    };
                    style = if key == "variants" {
                        style.variant_classes(entry, classes)
                    } else {
                        style.size_classes(entry, classes)
                    };
                }
            }
            "defaults" => {
                let Some(map) = value.as_object() else {
                    warnings.push(format!("components.{name}.defaults: expected an object"));
                    continue;
                };
                for (entry, value) in map {
                    match (entry.as_str(), value.as_str()) {
                        ("variant", Some(v)) => style = style.default_variant(v),
                        ("size", Some(v)) => style = style.default_size(v),
                        ("variant" | "size", None) => warnings.push(format!(
                            "components.{name}.defaults.{entry}: expected a string"
                        )),
                        _ => warnings
                            .push(format!("components.{name}.defaults: unknown key `{entry}`")),
                    }
                }
            }
            other => warnings.push(format!("components.{name}: unknown key `{other}`")),
        }
    }
    Some(style)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;
    use crate::element::{Runtime, create_root, effect};

    fn run(check: impl FnOnce()) {
        let runtime = Runtime::default();
        let _entered = runtime.enter();
        check();
    }

    /// Applies the closures in order and reads which ran, by the tags
    /// they pushed.
    fn applied(closures: Vec<Refine>, log: &RefCell<Vec<&'static str>>) -> Vec<&'static str> {
        log.borrow_mut().clear();
        for f in closures {
            f(StyleRefinement::default());
        }
        log.borrow().clone()
    }

    fn tagged(
        log: &Rc<RefCell<Vec<&'static str>>>,
        tag: &'static str,
    ) -> impl Fn(StyleRefinement) -> StyleRefinement + 'static {
        let log = log.clone();
        move |s| {
            log.borrow_mut().push(tag);
            s
        }
    }

    #[test]
    fn resolves_base_variant_and_size_in_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let style = ComponentStyle::new()
            .size("lg", tagged(&log, "size"))
            .variant("primary", tagged(&log, "variant"))
            .base(tagged(&log, "base"))
            .base_classes("a")
            .variant_classes("primary", "b")
            .size_classes("lg", "c");
        let (typed, classes) = style.resolve(Some("primary"), Some("lg"));
        assert_eq!(applied(typed, &log), ["base", "variant", "size"]);
        assert_eq!(classes.as_deref(), Some("a b c"));
    }

    #[test]
    fn defaults_fill_an_unset_variant_and_size() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let style = ComponentStyle::new()
            .variant("primary", tagged(&log, "variant"))
            .size("md", tagged(&log, "size"))
            .default_variant("primary")
            .default_size("md");
        let (typed, _) = style.resolve(None, None);
        assert_eq!(applied(typed, &log), ["variant", "size"]);
    }

    #[test]
    fn an_unknown_name_resolves_nothing_for_it() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let style = ComponentStyle::new()
            .base(tagged(&log, "base"))
            .variant("primary", tagged(&log, "variant"))
            .default_variant("primary");
        let (typed, classes) = style.resolve(Some("nope"), Some("huge"));
        assert_eq!(applied(typed, &log), ["base"]);
        assert_eq!(classes, None);
    }

    #[test]
    fn reading_subscribes_and_setting_notifies() {
        let runtime = Runtime::default();
        let _entered = runtime.enter();
        let runs = Rc::new(Cell::new(0));
        let seen = Rc::new(Cell::new(false));
        let (owner, ()) = create_root({
            let (runs, seen) = (runs.clone(), seen.clone());
            move || {
                effect(move || {
                    runs.set(runs.get() + 1);
                    seen.set(component_style_of("button").is_some());
                });
            }
        });
        assert_eq!((runs.get(), seen.get()), (1, false));
        set_component_style("button", ComponentStyle::new().base_classes("x"));
        assert_eq!((runs.get(), seen.get()), (2, true));
        owner.dispose();
    }

    #[test]
    fn reads_share_one_merged_look_until_a_write() {
        run(|| {
            set_component_style("button", ComponentStyle::new().base_classes("rust"));
            set_component_classes(r#"{"button":{"base":"json"}}"#);
            let first = component_style_of("button").unwrap();
            let second = component_style_of("button").unwrap();
            assert!(Rc::ptr_eq(&first, &second));
            assert!(has_component_style("button"));
            assert_eq!(first.classes(None, None).as_deref(), Some("rust json"));

            set_component_classes("{}");
            let third = component_style_of("button").unwrap();
            assert!(!Rc::ptr_eq(&first, &third));
            assert_eq!(third.classes(None, None).as_deref(), Some("rust"));
            assert!(!has_component_style("chip"));
        });
    }

    #[test]
    fn classes_json_warns_on_unknown_keys() {
        run(|| {
            let warnings = set_component_classes(r#"{"button":{"varients":{}}}"#);
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0].contains("varients"));
            assert!(component_style_of("button").is_none());

            let warnings = set_component_classes(
                r#"{"button":{"base":"a","variants":{"p":"b"},"defaults":{"variant":"p","sise":"x"}}}"#,
            );
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0].contains("sise"));
            let (_, classes) = component_style_of("button").unwrap().resolve(None, None);
            assert_eq!(classes.as_deref(), Some("a b"));
        });
    }

    #[test]
    fn rust_and_json_looks_of_one_name_both_apply() {
        run(|| {
            let log = Rc::new(RefCell::new(Vec::new()));
            set_component_style(
                "button",
                ComponentStyle::new()
                    .variant("primary", tagged(&log, "variant"))
                    .variant_classes("primary", "rust"),
            );
            set_component_classes(r#"{"button":{"variants":{"primary":"json"}}}"#);
            let (typed, classes) = component_style_of("button")
                .unwrap()
                .resolve(Some("primary"), None);
            assert_eq!(applied(typed, &log), ["variant"]);
            assert_eq!(classes.as_deref(), Some("rust json"));
        });
    }

    #[test]
    fn json_defaults_win_over_rust_defaults() {
        run(|| {
            set_component_style(
                "button",
                ComponentStyle::new()
                    .variant_classes("a", "ra")
                    .variant_classes("b", "rb")
                    .default_variant("a"),
            );
            set_component_classes(
                r#"{"button":{"variants":{"b":"jb"},"defaults":{"variant":"b"}}}"#,
            );
            let (_, classes) = component_style_of("button").unwrap().resolve(None, None);
            assert_eq!(classes.as_deref(), Some("rb jb"));
        });
    }

    #[test]
    fn a_component_with_json_keeps_rust_base_classes() {
        run(|| {
            set_component_style("button", ComponentStyle::new().base_classes("rust"));
            set_component_classes(r#"{"button":{"sizes":{"lg":"big"}}}"#);
            let (_, classes) = component_style_of("button")
                .unwrap()
                .resolve(None, Some("lg"));
            assert_eq!(classes.as_deref(), Some("rust big"));
        });
    }
}
