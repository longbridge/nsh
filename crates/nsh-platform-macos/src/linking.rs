//! Inbound URLs (`nsh`'s `UrlScheme`, `EventKind::UrlScheme`): the URLs the system
//! opens the app with, its schemes from nsh.json `schemes` (the bundle's
//! `CFBundleURLTypes`). Shared by macOS and iOS (the iOS crate includes this
//! file, as it does `notifications.rs`).
//!
//! The app delegate (macOS `application:openURLs:`) or the scene delegate
//! (iOS `scene:openURLContexts:` and the launch's `URLContexts`) hands each
//! URL to [`open`], which sends it to the last root surface whose engine
//! started. A URL that comes before any engine (the one that launched the
//! app) waits in `URLS` until one starts; the core keeps it from there until
//! the app listens.

use std::cell::RefCell;

use nsh_core::{EventKind, EventPayload, PayloadValue, ROOT_ID};
use nsh_engine::EngineControl;

thread_local! {
    /// The engine URLs go to, with its surface's key.
    static TARGET: RefCell<Option<(usize, EngineControl)>> = const { RefCell::new(None) };
    /// URLs before an engine started.
    static URLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A surface's engine started: URLs go to it from now on, the waiting ones
/// first. `owner` identifies the surface for [`stopped`]. On the main thread.
pub(crate) fn started(owner: usize, control: &EngineControl) {
    TARGET.with(|t| *t.borrow_mut() = Some((owner, control.clone())));
    for url in URLS.with(|u| std::mem::take(&mut *u.borrow_mut())) {
        send(control, url);
    }
}

/// The surface `owner`'s engine stopped. On the main thread.
pub(crate) fn stopped(owner: usize) {
    TARGET.with(|t| {
        let mut target = t.borrow_mut();
        if target.as_ref().is_some_and(|(key, _)| *key == owner) {
            *target = None;
        }
    });
}

/// The system opened the app with `url`. On the main thread.
pub(crate) fn open(url: String) {
    let target = TARGET.with(|t| t.borrow().as_ref().map(|(_, c)| c.clone()));
    match target {
        Some(control) => send(&control, url),
        None => URLS.with(|u| u.borrow_mut().push(url)),
    }
}

/// Sends `url` to the root of `control`'s app.
pub(crate) fn send(control: &EngineControl, url: String) {
    control.dispatch_event(
        ROOT_ID,
        EventKind::UrlScheme,
        EventPayload::Map(vec![("url", PayloadValue::Str(url))]),
    );
}
