//! `nsh`'s `UrlScheme`: URLs the system opens the app with, of the schemes
//! the app registers in nsh.json (`"schemes": ["myapp"]` makes
//! `myapp://...` open the app). The platforms register the schemes when
//! they package or run the app and report each URL as
//! [`crate::EventKind::UrlScheme`] on the root; the one that started the app
//! waits for the first listener.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use crate::element::{Callback, JsError, JsObject, Local, PropEnum, Stop, System, module};

/// The URLs the system opens the app with.
#[module]
pub(crate) struct UrlScheme;

#[module]
impl UrlScheme {
    /// Listens to `event`. `url`: the system opened the app with a URL of a
    /// scheme nsh.json's `schemes` registers (a link pressed in the browser,
    /// another app's `openUrl`), the one that started the app included,
    /// which waits for the first listener. Returns a function that stops
    /// listening.
    pub(crate) fn on(
        event: UrlSchemeEvent,
        #[args(url)] listener: Callback<UrlSchemeOpen>,
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
        if !links.started.replace(true) {
            cx.start(deliver(Rc::downgrade(&links)));
        }
        let links = Rc::downgrade(&links);
        Ok(Stop::new(move || {
            if let Some(links) = links.upgrade() {
                links.listeners.borrow_mut().retain(|(i, _)| *i != id);
            }
        }))
    }
}

/// `UrlScheme.on`'s events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub(crate) enum UrlSchemeEvent {
    /// The system opened the app with a URL.
    #[default]
    Open,
}

/// A URL the system opened the app with (the `open` event).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub(crate) struct UrlSchemeOpen {
    /// The whole URL, its scheme one of nsh.json's `schemes`.
    url: String,
}

/// The VM's `url` listeners.
#[derive(Default)]
struct Links {
    listeners: RefCell<Vec<(u32, Callback<UrlSchemeOpen>)>>,
    next: Cell<u32>,
    /// The task waiting for the core's URLs started; it lasts as the VM
    /// does.
    started: Cell<bool>,
    /// URLs that came with no listener, for the next one.
    unheard: RefCell<Vec<String>>,
}

impl Local for Links {
    fn close(&self) {
        self.listeners.borrow_mut().clear();
    }
}

/// Posts the core's URLs to the listeners, while the VM lasts.
async fn deliver(links: Weak<Links>) {
    loop {
        let url = crate::prelude::next_url_scheme_open().await;
        let Some(links) = links.upgrade() else {
            return;
        };
        let listeners = links.listeners.borrow();
        if listeners.is_empty() {
            links.unheard.borrow_mut().push(url);
            continue;
        }
        for (_, listener) in &*listeners {
            listener.post(UrlSchemeOpen { url: url.clone() });
        }
    }
}
