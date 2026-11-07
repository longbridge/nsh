//! `nsh`'s `UrlScheme`: the links that open the app ([`crate::url_scheme`]),
//! URLs of the schemes it claims in `nsh.json`, and the app's address
//! (`url`, `setUrl`): on the web the page's. The core keeps the address
//! (Rust's `app_url`), and `url()` reads it where it is (`System::app_url`).
//! Needs no capability: the app's own claim is what lets them in.

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
    /// `set_app_url` set after it; `null` when there is none. The core's
    /// own, read as it is.
    pub(crate) fn url(sys: System) -> Option<String> {
        sys.app_url.get()
    }

    /// Sets the current page's address: on the web it replaces the
    /// current history entry's (as `history.replaceState`); the native
    /// platforms keep it for `url()`. The app's pages stay as they are.
    pub(crate) fn set_url(url: String, sys: System, cx: CallCx<'_>) -> Result<(), JsError> {
        crate::url_scheme::check_address(&url).map_err(JsError::Type)?;
        // Written at once, so `url()` reads it back; the web's history
        // entry follows in order with what JS sent before (the stack's
        // changes).
        sys.app_url.set(url.clone());
        cx.start(async move { crate::element::set_app_url_from_js(url) });
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
    /// Links that came while no listener was left, for the next one.
    unheard: RefCell<Vec<String>>,
}

impl Links {
    /// Starts taking the core's links, once.
    fn start(self: &Rc<Self>, cx: &CallCx<'_>) {
        if !self.started.replace(true) {
            cx.start(deliver(Rc::downgrade(self)));
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
