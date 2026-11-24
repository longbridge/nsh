//! Views an app's web build leaves out of its main module (`nsh.wasm`)
//! and loads the first time they show: a page the first screen does not
//! open, or a heavy component on it (a `RichEditor`).
//!
//! The view's code is reached only through a gate the app exports,
//! `nsh_lazy_<name>`, which registers it with [`register_lazy_view`].
//! `nsh package --target web` links each such gate into a side module of
//! its own; [`lazy_view`] shows the placeholder, has the page load the
//! side module (which runs the gate), then shows the view. The registry
//! is a table filled at run time, so the compiler cannot see from the
//! main module's code which view it will hold, and the view's code stays
//! out of it.
//!
//! The native runtimes and a web build published whole have no side
//! modules: the app calls its gates as it starts (outside `wasm32`).

use std::sync::{Mutex, PoisonError};

use super::{AnyElement, IntoElements, dynamic, signal, spawn};

/// What makes a lazy view.
type MakeView = fn() -> AnyElement;

/// The views registered so far, by name. A few at most: a list is enough.
static VIEWS: Mutex<Vec<(&'static str, MakeView)>> = Mutex::new(Vec::new());

/// Registers `view` as the lazy view `name`: call it from the app's gate
/// `nsh_lazy_<name>` (and, outside `wasm32`, as the app starts). What
/// shows [`lazy_view`] `name` from then on, and what showed its
/// placeholder, shows `view`.
pub fn register_lazy_view(name: &'static str, view: MakeView) {
    let mut views = VIEWS.lock().unwrap_or_else(PoisonError::into_inner);
    match views.iter_mut().find(|(n, _)| *n == name) {
        Some(entry) => entry.1 = view,
        None => views.push((name, view)),
    }
}

fn registered(name: &str) -> Option<MakeView> {
    let views = VIEWS.lock().unwrap_or_else(PoisonError::into_inner);
    views.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

/// The lazy view `name` (see the module): `placeholder` until its code
/// loaded, then the view the gate `nsh_lazy_<name>` registered. When it
/// does not load (the network, or no such gate) the placeholder stays and
/// the log says why; the next `lazy_view` of the name tries again.
pub fn lazy_view<E: IntoElements>(
    name: &'static str,
    placeholder: impl Fn() -> E + 'static,
) -> AnyElement {
    if let Some(view) = registered(name) {
        return view();
    }
    let loaded = signal(false);
    spawn(async move {
        let key = format!("lazy_{name}");
        crate::runtime_data::load(&key).await;
        if registered(name).is_some() {
            loaded.set(true);
        } else {
            crate::runtime_data::forget(&key);
            log::warn!("lazy view `{name}` did not load: no gate `nsh_lazy_{name}` registered it");
        }
    });
    dynamic(
        move || match loaded.get().then(|| registered(name)).flatten() {
            Some(view) => vec![view()],
            None => placeholder().into_elements(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{register_lazy_view, registered};
    use crate::element::{AnyElement, IntoElement, text};

    fn one() -> AnyElement {
        text("one").into_element()
    }

    fn two() -> AnyElement {
        text("two").into_element()
    }

    #[test]
    fn a_registered_view_replaces_the_one_of_its_name() {
        assert!(registered("lazy-test").is_none());
        register_lazy_view("lazy-test", one);
        assert!(registered("lazy-test") == Some(one as fn() -> AnyElement));
        register_lazy_view("lazy-test", two);
        assert!(registered("lazy-test") == Some(two as fn() -> AnyElement));
    }
}
