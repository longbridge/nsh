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

use std::{collections::BTreeMap, rc::Rc};

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

impl Look {
    fn typed(f: impl Fn(StyleRefinement) -> StyleRefinement + 'static) -> Self {
        Self {
            typed: Some(Rc::new(f)),
            classes: None,
        }
    }

    fn classes(classes: &str) -> Self {
        Self {
            typed: None,
            classes: Some(classes.to_owned().into()),
        }
    }
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
        self.variants.entry(name.to_owned()).or_default().typed = Look::typed(f).typed;
        self
    }

    /// The typed look of the size `name`.
    pub fn size(
        mut self,
        name: &str,
        f: impl Fn(StyleRefinement) -> StyleRefinement + 'static,
    ) -> Self {
        self.sizes.entry(name.to_owned()).or_default().typed = Look::typed(f).typed;
        self
    }

    /// The Tailwind classes every instance has.
    pub fn base_classes(mut self, classes: &str) -> Self {
        self.base.classes = Look::classes(classes).classes;
        self
    }

    /// The Tailwind classes of the variant `name`.
    pub fn variant_classes(mut self, name: &str, classes: &str) -> Self {
        self.variants
            .insert(name.to_owned(), Look::classes(classes));
        self
    }

    /// The Tailwind classes of the size `name`.
    pub fn size_classes(mut self, name: &str, classes: &str) -> Self {
        self.sizes.entry(name.to_owned()).or_default().classes = Look::classes(classes).classes;
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
    /// `size` (likewise): the typed closures in that order and the class
    /// strings joined with spaces. A name nothing declares adds nothing.
    pub(crate) fn resolve(
        &self,
        variant: Option<&str>,
        size: Option<&str>,
    ) -> (Vec<Refine>, Option<SharedString>) {
        let variant = variant.or(self.default_variant.as_deref());
        let size = size.or(self.default_size.as_deref());
        let looks = [
            Some(&self.base),
            variant.and_then(|n| self.variants.get(n)),
            size.and_then(|n| self.sizes.get(n)),
        ];
        let mut typed = Vec::new();
        let mut classes: Vec<&str> = Vec::new();
        for look in looks.into_iter().flatten() {
            typed.extend(look.typed.clone());
            classes.extend(look.classes.as_deref());
        }
        let classes = (!classes.is_empty()).then(|| classes.join(" ").into());
        (typed, classes)
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

    /// `self` (typed looks) and `classes` (class looks) as one: each look
    /// has the typed part of the first and the classes of the second.
    fn merged(&self, classes: &Self) -> Self {
        fn zip(
            typed: &BTreeMap<String, Look>,
            classes: &BTreeMap<String, Look>,
        ) -> BTreeMap<String, Look> {
            let mut out = typed.clone();
            for (name, look) in classes {
                out.entry(name.clone()).or_default().classes = look.classes.clone();
            }
            out
        }
        Self {
            base: Look {
                typed: self.base.typed.clone(),
                classes: classes.base.classes.clone(),
            },
            variants: zip(&self.variants, &classes.variants),
            sizes: zip(&self.sizes, &classes.sizes),
            default_variant: self
                .default_variant
                .clone()
                .or_else(|| classes.default_variant.clone()),
            default_size: self
                .default_size
                .clone()
                .or_else(|| classes.default_size.clone()),
        }
    }
}

/// The surface's declared looks, by component.
#[derive(Clone, Default)]
struct Store {
    typed: BTreeMap<String, ComponentStyle>,
    classes: BTreeMap<String, ComponentStyle>,
}

/// Declares how `component` (a component's name, `"button"`) looks, in
/// typed styles. It replaces the typed looks declared for it before, and
/// the components showing it update.
pub fn set_component_style(component: &'static str, style: ComponentStyle) {
    global::<Store>().update(|store| {
        store.typed.insert(component.to_owned(), style);
    });
}

/// The looks declared for `component` (typed and classes), subscribing the
/// reader to changes.
pub(crate) fn component_style_of(component: &str) -> Option<ComponentStyle> {
    global::<Store>().with(|store| {
        match (store.typed.get(component), store.classes.get(component)) {
            (None, None) => None,
            (Some(typed), None) => Some(typed.clone()),
            (None, Some(classes)) => Some(classes.clone()),
            (Some(typed), Some(classes)) => Some(typed.merged(classes)),
        }
    })
}

/// Replaces every class look with those of `json`, the `components` of
/// nsh.json: an object by component name, each with `base` (classes),
/// `variants` and `sizes` (objects of classes by name) and `defaults`
/// (`variant`, `size`). Lenient: what does not fit is left out and named in
/// the warnings returned.
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
    global::<Store>().update(|store| store.classes = classes);
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
}
