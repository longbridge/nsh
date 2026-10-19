//! The runtime's modules written in Rust: `nsh/android`'s `Toast`,
//! `nsh/accessibility`'s commands, all of `nsh/image`, `nsh/clipboard` and
//! `nsh/storage`,
//! `nsh/desktop`'s `Window`, the surface-independent `os` metadata and
//! `net`'s sockets. Their JS face is made
//! from their specs like a UI set's (`nsh-js` `components.rs`), and so are
//! their declarations ([`crate::ui::declarations`]); `nsh-js` adds what has
//! to run in JS (the app's listeners, reactive accessors).

use std::sync::OnceLock;

use crate::ui::Registry;

mod accessibility;
mod android;
mod clipboard;
mod desktop;
mod image;
mod net;
mod os;
mod process;
mod storage;

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
        let mut storage = Registry::runtime("nsh/storage");
        storage.module::<storage::Storage>();
        let mut desktop = Registry::runtime("nsh/desktop");
        desktop.module::<desktop::Window>();
        let mut os = Registry::runtime("os");
        os.module::<os::Os>();
        let mut process = Registry::runtime("process");
        process
            .module::<process::Process>()
            .native_values(&["nextTick"]);
        let mut net = Registry::runtime("net");
        net.module::<net::Net>();
        vec![
            android,
            accessibility,
            image,
            clipboard,
            storage,
            desktop,
            os,
            net,
            process,
        ]
    })
}
