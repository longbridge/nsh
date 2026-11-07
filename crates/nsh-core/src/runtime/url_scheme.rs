//! `nsh`'s `UrlScheme`: the links that open the app ([`crate::url_scheme`]),
//! URLs of the schemes it claims in `nsh.json`, and the app's address
//! (`url`, `setUrl`): on the web the page's. The core keeps the address
//! (Rust's `app_url`); `url()` reads the VM's copy, which follows the
//! core's. Needs no capability: the app's own claim is what lets them in.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use crate::element::{CallCx, Callback, JsError, JsObject, Local, PropEnum, Stop, System, module};

/// The links that open the app.
#[module]
pub(crate) struct UrlScheme;

#[module]
impl UrlScheme {
    /// Listens to `event`. `open`: the system handed the app a link of a
    /// scheme it claims (`nsh.json` `"schemes": [...]`),
    /// at launch or while it runs; links that came while nothing listened,
    /// the one that launched the app included, go to the next listener (at
    /// most 16, the oldest dropped). Nothing navigates on its own: what a
    /// link opens is the app's. Anyone can make a link of the app's scheme,
    /// so its URL is untrusted input: check it before acting on it.
    /// Returns a function that stops listening.
    pub(crate) fn on(
        event: UrlSchemeEvent,
        #[args(open)] listener: Callback<UrlSchemeOpen>,
        sys: System,
        cx: crate::element::CallCx<'_>,
    ) -> Result<Stop, JsError> {
        let UrlSchemeEvent::Open = event;
        let links = sys.locals.get::<Links>();
        let id = links.next.get();
        links.next.set(id.wrapping_add(1));
        for url in links.unheard.take() {
            listener.post(UrlSchemeOpen { url });
        }
        links.listeners.borrow_mut().push((id, listener));
        links.start(&cx);
        let links = Rc::downgrade(&links);
        Ok(Stop::new(move || {
            if let Some(links) = links.upgrade() {
                links.listeners.borrow_mut().retain(|(i, _)| *i != id);
            }
        }))
    }

    /// The app's address: on the web the page's; on the native platforms
    /// the last link that opened the app (the link that launched the app
    /// also comes as the first `open`); the URL `setUrl` or Rust's
    /// `set_app_url` set after it; `null` when there is none. The VM's
    /// copy of the core's: what changes in the core shows from the next
    /// turn on, what `setUrl` sets at once.
    pub(crate) fn url(sys: System, cx: CallCx<'_>) -> Option<String> {
        let links = sys.locals.get::<Links>();
        links.follow(&cx);
        links.current.borrow().clone()
    }

    /// Sets the current page's address: on the web it replaces the
    /// current history entry's (as `history.replaceState`); the native
    /// platforms keep it for `url()`. The app's pages stay as they are.
    pub(crate) fn set_url(url: String, sys: System, cx: CallCx<'_>) -> Result<(), JsError> {
        crate::url_scheme::check_address(&url).map_err(JsError::Type)?;
        // Synchronous, so `url()` reads it back at once; the core takes it
        // in order with what JS sent before (the stack's changes).
        let links = sys.locals.get::<Links>();
        *links.current.borrow_mut() = Some(url.clone());
        let js = links.sets.get() + 1;
        links.sets.set(js);
        cx.start(async move { crate::element::set_app_url_from_js(url, js) });
        links.follow(&cx);
        Ok(())
    }
}

/// `UrlScheme.on`'s events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub(crate) enum UrlSchemeEvent {
    /// The system handed the app a link.
    #[default]
    Open,
}

/// A link that opened the app (the `open` event).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub(crate) struct UrlSchemeOpen {
    /// The URL, as the system handed it.
    url: String,
}

/// The VM's `open` listeners.
#[derive(Default)]
struct Links {
    listeners: RefCell<Vec<(u32, Callback<UrlSchemeOpen>)>>,
    next: Cell<u32>,
    /// The task waiting for the core's links started; it lasts as the VM
    /// does.
    started: Cell<bool>,
    /// The task following the core's address started (`follow`).
    following: Cell<bool>,
    /// `setUrl`'s calls so far, numbered for the core.
    sets: Cell<u64>,
    /// Links that came while no listener was left, for the next one.
    unheard: RefCell<Vec<String>>,
    /// The VM's copy of the core's address (`url()`).
    current: RefCell<Option<String>>,
}

impl Links {
    /// Starts taking the core's links, once.
    fn start(self: &Rc<Self>, cx: &CallCx<'_>) {
        if !self.started.replace(true) {
            cx.start(deliver(Rc::downgrade(self)));
        }
    }

    /// Starts following the core's address, once.
    fn follow(self: &Rc<Self>, cx: &CallCx<'_>) {
        if !self.following.replace(true) {
            cx.start(follow(Rc::downgrade(self)));
        }
    }
}

/// Keeps the VM's copy of the app's address, while the VM lasts.
async fn follow(links: Weak<Links>) {
    let mut known = 0;
    loop {
        let state = crate::element::watch_app_url(known).await;
        let Some(links) = links.upgrade() else {
            return;
        };
        known = state.version;
        // What `setUrl` set since is newer than this.
        if state.js == links.sets.get() {
            *links.current.borrow_mut() = state.address;
        }
    }
}

impl Local for Links {
    fn close(&self) {
        self.listeners.borrow_mut().clear();
    }
}

/// Posts the core's links to the listeners, while the VM lasts.
async fn deliver(links: Weak<Links>) {
    loop {
        let url = crate::prelude::next_url_scheme_open().await;
        let Some(links) = links.upgrade() else {
            return;
        };
        let listeners = links.listeners.borrow();
        if listeners.is_empty() {
            let mut unheard = links.unheard.borrow_mut();
            if unheard.len() == crate::url_scheme::MAX_PENDING {
                unheard.remove(0);
            }
            unheard.push(url);
            continue;
        }
        for (_, listener) in &*listeners {
            listener.post(UrlSchemeOpen { url: url.clone() });
        }
    }
}
