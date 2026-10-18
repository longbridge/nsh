//! The runtime's modules written in Rust: `nsh/android`'s `Toast`,
//! `nsh/accessibility`'s commands, all of `nsh/image` and `nsh/clipboard`,
//! `nsh/desktop`'s `Window`. Their JS face is made
//! from their specs like a UI set's (`nsh-js` `components.rs`), and so are
//! their declarations ([`crate::ui::declarations`]); `nsh-js` adds what has
//! to run in JS (the app's listeners, reactive accessors).

use std::sync::OnceLock;

use crate::ui::{Function, Member, Registry};

mod accessibility;
mod android;
mod clipboard;
mod desktop;
mod image;

/// The runtime's Rust modules, one registry per JS module (its
/// [`Registry::namespace`]: `nsh/android`).
pub fn registries() -> &'static [Registry] {
    static REGISTRIES: OnceLock<Vec<Registry>> = OnceLock::new();
    REGISTRIES.get_or_init(|| {
        let mut android = Registry::runtime("nsh/android");
        android
            .module::<android::Toast>()
            .module::<android::BackHandler>()
            .module::<android::Android>();
        let mut accessibility = Registry::runtime("nsh/accessibility");
        accessibility.module::<accessibility::AccessibilityInfo>();
        let mut image = Registry::runtime("nsh/image");
        image.module::<image::Image>();
        let mut clipboard = Registry::runtime("nsh/clipboard");
        clipboard.module::<clipboard::Clipboard>();
        let mut desktop = Registry::runtime("nsh/desktop");
        desktop.module::<desktop::Window>();
        vec![android, accessibility, image, clipboard, desktop]
    })
}

/// The `async` member `name` of a runtime class (`nsh/x:Name#member`).
pub(crate) fn class_member(name: &str) -> Option<&'static Member> {
    registries().iter().find_map(|r| r.class_member(name))
}

/// Function `name` of a runtime module (`nsh/android:Toast.show`).
pub(crate) fn function(name: &str) -> Option<&'static Function> {
    let (namespace, _) = name.split_once(':')?;
    registries()
        .iter()
        .find(|r| r.namespace() == Some(namespace))?
        .function(name)
}
