//! Web views: WebKitGTK 6.0 (`webkit6`, feature `webview`).
//!
//! A `WebKitWebView` is a GTK widget like any other: hosted in its node's
//! view and filling it ([`fill`]), it is drawn inside the view's snapshot
//! (WebKit appends the page as a render node), so it stacks, clips and
//! fades with nsh's content as text fields and videos do. Views mounted
//! after it, modals and their scrim, dialogs, toasts and the development
//! overlays draw over it; `overflow: hidden` ancestors clip it, opacity
//! and transforms apply to it, hidden screens unmap it (WebKit then
//! throttles the page), and a departing screen's `gtk::WidgetPaintable`
//! holds its last frame. No composition surface (a separate window, as
//! WebView2 on Windows) is needed. GTK's picking ignores nsh's clipping,
//! so pointer events that GTK would send to the clipped-off part of a page
//! are kept from it (`crate::input`).
//!
//! As on iOS and macOS, pages talk to the app through
//! `window.nsh.postMessage(data)` (a script message handler,
//! `onMessage`), and `postMessage()` dispatches a `message` event on the
//! page's `window`. Events are the iOS ones: `onLoadStart` and
//! `onNavigationStateChange` when a navigation starts, `onLoad` and
//! `onNavigationStateChange` when it finishes, `onError` when it fails
//! (not for cancelled ones), with WebKit's error page left out.
//!
//! Navigations are decided natively (`decide-policy`) from the app's
//! `webview` capability, never by asking JS. WebKitGTK does not say which
//! frame a navigation is for, so the capability applies to the
//! navigations of frames too (an `iframe` of a host that is not granted
//! stays empty), where iOS and macOS only check the main frame's.
//! Responses WebKit cannot show are refused rather than downloaded.
//!
//! Props: `injectedJavaScriptBeforeContentLoaded` and `injectedJavaScript`
//! are user scripts of the top frame (at the document's start and end);
//! `javaScriptEnabled` is WebKit's `enable-javascript-markup` (the page's
//! own scripts; injected ones still run, as with WebKit on Apple
//! platforms); `scrollable: false` is a user style sheet hiding the
//! top document's overflow (inner scrollable elements still scroll, as
//! with iOS's scroll view), and wheels over such a page go to the scroll
//! views around it; `allowsBackForwardNavigationGestures` is
//! `enable-back-forward-navigation-gestures` (touchpad swipes); the user
//! agent is WebKit's `user-agent` setting. Pages follow the app's color
//! scheme (`prefers-color-scheme`) through GTK's settings, which the
//! surface sets from it (`crate::system`).
//!
//! `contentRules` are WebKit user content filters, which are the same
//! content blocker rules `WKContentRuleList` takes: WebKit compiles each
//! list in its `UserContentFilterStore`
//! (`$XDG_CACHE_HOME/nsh-content-filters`) and matches the page's loads
//! itself, cancelling the blocked ones and hiding what the
//! `css-display-none` rules name. They apply to the loads from when the
//! filter is added; nothing is injected into the page in place of
//! blocking. The rules must be installed before a page they govern loads:
//! a source that arrives while a changed set compiles waits for it, the
//! app's navigation commands (load, reload, back, forward) queue in order
//! until it is in, and a page's own navigations are refused with
//! `onError` meanwhile, so nothing reaches the network with the wrong
//! policy. A set WebKit cannot compile leaves the installed rules (and
//! the held page and commands) as they were and reports `onError`;
//! clearing the rules (or installing another set) releases them. Requests
//! (screenshots, cookies, the inspector) keep working throughout.
//!
//! Website data (cookies, local storage, IndexedDB, the cache) is kept per
//! app id, in a `NetworkSession` of the app's own:
//! `$XDG_DATA_HOME/<app id>/webkit` and `$XDG_CACHE_HOME/<app id>/webkit`
//! (cookies in `cookies.sqlite` there), as the Windows backend keeps a
//! WebView2 profile per app id. The session lives as long as one of the
//! app's web views does, and WebKit's network process with it: an app
//! that has closed its last web view, such as one left running in the
//! tray, doesn't keep that process. The next web view starts a new one,
//! which reads the data from disk; only session cookies (those without an
//! expiry, which WebKit keeps in memory) end with it, as when a browser
//! is closed.
//! An `incognito` web view has an ephemeral `NetworkSession` of its own
//! instead, made with it: its data stays in memory and goes with it.
//!
//! [`Command::WebViewRequest`]s answer with [`EventKind::WebViewReply`]: a
//! screenshot (a base64 PNG of the visible page), the view's session's
//! cookies (read and written, with Unix-millisecond expiries), WebKit's
//! inspector (developer extras, off until asked) and downloads. Page load
//! progress is WebKit's own estimate (`onLoadProgress`).
//!
//! Downloads follow `downloadBehavior` (a Rust prop). `Automatic` (the
//! default) is the app's `onDownload` opt-in: when it listens, WebKit's
//! downloads are saved to `<data dir>/<app id>/downloads/<id>/<filename>`
//! and reported as they start, finish (with their size) or fail; a page
//! the app does not listen to downloads nothing. `Manual` reports each
//! download's choice of destination first (`onDownloadPending`) and
//! writes nothing until the app accepts a path (`AcceptDownload`) or
//! cancels it; an accepted download writes to a hidden staging file
//! beside that path and is moved onto it only once it finished (never
//! replacing an existing file unless `overwrite` was accepted), so a
//! cancel or a failure removes the staging file and leaves the app's
//! file as it was. `CancelDownload` is answered once the transfer stopped
//! and what it wrote is gone; a pending decision nobody answers is
//! cancelled (and reported as failed) after a minute, and one nobody
//! listens to at all is cancelled at once. Accepted destinations must be
//! absolute, in an existing folder, and must not exist unless `overwrite`
//! says so.
//!
//! The URL schemes the app serves (`WebView::url_scheme`) are registered on
//! the web context and answered per web view (`super::web_scheme`).
//!
//! Keys: a focused page gets them first. Escape the page does not use
//! comes back to the window, where it leaves the page as it leaves a text
//! field (`crate::navigation`). Alt+Left, the Back key and the mouse's
//! back button go back in the page's history when it has one, else back
//! in the app; Alt+Right, the Forward key and the forward button go
//! forward in the page (WebKitGTK treats arrows as used, so Alt+Left
//! would never reach the window).

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use nsh_core::{
    Command, ContentRuleList, EventKind, EventPayload, JsonValue, NodeId, PayloadValue, WebProps,
    WebSource,
    webview::{
        self, CookieSameSite, WebCookie, WebDownloadBehavior, WebViewOperation, WebViewRequest,
    },
};
use nsh_engine::Access;
use webkit6::{
    CookiePersistentStorage, Download, DownloadError, LoadEvent, NavigationPolicyDecision,
    NetworkError, NetworkSession, PolicyDecisionType, PolicyError, ResponsePolicyDecision,
    SnapshotOptions, SnapshotRegion, URIRequest, UserContentFilter, UserContentFilterStore,
    UserContentInjectedFrames, UserContentManager, UserMediaPermissionRequest, UserScript,
    UserScriptInjectionTime, UserStyleLevel, UserStyleSheet, WebView, prelude::*, soup,
};

use super::{Emitter, UNSCROLLABLE, error, fill_sized};
use crate::{decor::rgba, surface::Surface, view::NshView};

/// The script message handler the bridge posts to.
const HANDLER: &str = "nsh";

/// Lets pages talk to the app: `window.nsh.postMessage(data)`.
const BRIDGE: &str = "window.nsh = window.nsh || { postMessage: function (data) { window.webkit.messageHandlers.nsh.postMessage(String(data)); } };";

/// `scrollable: false`: the top document does not scroll.
const NO_SCROLL: &str = "html, body { overflow: hidden !important; }";

/// Where WebKit keeps the content blocker filters it compiled.
const FILTERS_DIR: &str = "nsh-content-filters";

/// How long a manual download waits for the app's destination.
const PENDING_TIMEOUT: Duration = Duration::from_secs(60);

type FilterWaiter = Box<dyn FnOnce(Result<&UserContentFilter, String>)>;

/// The content blocker filters WebKit compiled, by their identity, and
/// the views waiting for the ones compiling (with WebKit's error when one
/// cannot be compiled, so a view can report it instead of waiting).
#[derive(Default)]
struct Filters {
    ready: HashMap<String, UserContentFilter>,
    pending: HashMap<String, Vec<FilterWaiter>>,
}

thread_local! {
    /// The filters compiled on this (main) thread.
    static FILTERS: RefCell<Filters> = RefCell::default();
    /// WebKit's filter store: one directory of compiled filters.
    static STORE: RefCell<Option<UserContentFilterStore>> = const { RefCell::new(None) };
}

/// The filter store, made once: WebKit compiles a filter there, keeping
/// it for the next run.
fn store() -> UserContentFilterStore {
    STORE.with(|cell| {
        if let Some(store) = &*cell.borrow() {
            return store.clone();
        }
        let dir = glib::user_cache_dir().join(FILTERS_DIR);
        if let Err(err) = std::fs::create_dir_all(&dir) {
            log::warn!("web view: cannot make the filter store's directory: {err}");
        }
        let store = UserContentFilterStore::new(&dir.to_string_lossy());
        *cell.borrow_mut() = Some(store.clone());
        store
    })
}

/// The filters of one view's wanted rule set: those already compiled,
/// those still compiling, and the first failure.
struct Install {
    surface: Weak<Surface>,
    owner: Weak<Shared>,
    node: NodeId,
    generation: u64,
    remaining: Cell<usize>,
    filters: RefCell<Vec<UserContentFilter>>,
    failed: RefCell<Option<String>>,
}

/// Starts installing `rules` for the view at `node` (generation
/// `generation`): every list is taken from the cache or compiled, and
/// only when they are all ready does [`finish_install`] replace the
/// view's filters at once and load the source that waited. Nothing is
/// removed first: the rules already installed keep applying while these
/// compile. Lists are identified by their identifier *and* a hash of
/// their rules: a reused identifier (`dogo-ads-0`, over changed lists)
/// never gets the compiled filter of the old rules.
fn start_rules(
    surface: Weak<Surface>,
    owner: Weak<Shared>,
    node: NodeId,
    generation: u64,
    rules: &[ContentRuleList],
) {
    let install = Rc::new(Install {
        surface,
        owner,
        node,
        generation,
        remaining: Cell::new(0),
        filters: RefCell::new(Vec::new()),
        failed: RefCell::new(None),
    });
    let mut compiling = Vec::new();
    for rule in rules {
        let identity = rule_identity(rule);
        if let Some(filter) = FILTERS.with(|f| f.borrow().ready.get(&identity).cloned()) {
            install.filters.borrow_mut().push(filter);
        } else {
            install.remaining.set(install.remaining.get() + 1);
            compiling.push((identity, rule.rules.clone()));
        }
    }
    for (identity, rules) in compiling {
        let waiter_install = install.clone();
        let waiter: FilterWaiter = Box::new(move |result| match result {
            Ok(filter) => {
                waiter_install.filters.borrow_mut().push(filter.clone());
                let left = waiter_install.remaining.get() - 1;
                waiter_install.remaining.set(left);
                if left == 0 {
                    finish_install(&waiter_install);
                }
            }
            Err(error) => {
                *waiter_install.failed.borrow_mut() = Some(error);
                let left = waiter_install.remaining.get() - 1;
                waiter_install.remaining.set(left);
                if left == 0 {
                    finish_install(&waiter_install);
                }
            }
        });
        let vacant = FILTERS.with(|f| {
            let mut f = f.borrow_mut();
            match f.pending.entry(identity.clone()) {
                std::collections::hash_map::Entry::Occupied(mut queued) => {
                    queued.get_mut().push(waiter);
                    false
                }
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(vec![waiter]);
                    true
                }
            }
        });
        if vacant {
            compile_filter(identity, rules);
        }
    }
    if install.remaining.get() == 0 {
        // Everything was cached: install after this call returns (the
        // hosts are borrowed while props are applied).
        let install = install.clone();
        glib::idle_add_local_once(move || finish_install(&install));
    }
}

/// Replaces the view's filters with the complete set at once (only when
/// the generation still holds), then loads the source that waited for
/// them and the app's navigations issued meanwhile. A set WebKit could
/// not compile leaves the installed rules and the page alone, keeps the
/// source and commands waiting, and reports the error: only a complete
/// set, or blocking cleared, lets a page out.
///
/// Everything that reaches WebKit (a load, a navigation) runs after the
/// hosts' borrow is dropped: WebKit calls back into this module while it
/// loads, and none of those calls may meet a borrowed hosts map.
fn finish_install(install: &Rc<Install>) {
    /// What to apply once the hosts are free (no WebKit call runs under
    /// a borrow: its filters, then the source and the app's commands).
    struct Apply {
        shared: Rc<Shared>,
        content: UserContentManager,
        filters: Vec<UserContentFilter>,
        web: WebView,
        pending: Rc<RefCell<Option<WebSource>>>,
        source: Option<WebSource>,
        queued: Vec<Command>,
    }
    let Some(owner) = install.owner.upgrade().filter(|owner| owner.live.get()) else {
        return;
    };
    let Some(surface) = install.surface.upgrade() else {
        return;
    };
    let filters = std::mem::take(&mut *install.filters.borrow_mut());
    let failed = install.failed.borrow_mut().take();
    let mut apply = None;
    let mut failure = None;
    {
        let mut hosts = surface.native.hosts.borrow_mut();
        let Some(super::Host::Web(Some(host))) = hosts.get_mut(&install.node) else {
            return;
        };
        if !Rc::ptr_eq(&owner, &host.shared) || host.rules_gen.get() != install.generation {
            return; // the original view or its requested rules have changed
        }
        match failed {
            None => {
                apply = Some(Apply {
                    shared: host.shared.clone(),
                    content: host.content.clone(),
                    filters,
                    web: host.web.clone(),
                    pending: host.pending.clone(),
                    source: host.rules_waiting.take(),
                    queued: std::mem::take(&mut host.navigation),
                });
            }
            Some(error) => {
                // The installed rules keep applying, and the waiting
                // source and commands stay queued: a page must not go out
                // before the rules it asked for are in.
                failure = Some((host.shared.clone(), error));
            }
        }
    }
    if let Some((shared, message)) = failure {
        shared.emit(EventKind::Error, || {
            error(format!("content rules: {message}"))
        });
        return;
    }
    let Some(apply) = apply else { return };
    // The complete set replaces the old one in one step; only then may
    // anything load, so no request escapes the requested policy.
    apply.content.remove_all_filters();
    for filter in &apply.filters {
        apply.content.add_filter(filter);
    }
    apply.shared.rules_ready.set(true);
    // The source that waited loads now, then what the app asked for while
    // the rules compiled, in order.
    if let Some(source) = &apply.source {
        load_into(&apply.web, &apply.pending, source);
    }
    for navigation in apply.queued {
        command(&apply.web, &navigation);
    }
}

/// The identity of a content rule list: its identifier and a hash of its
/// rules. WebKit's filter store and this cache keep compiled filters by
/// it, since an identifier alone may be reused for other rules. WebKit
/// stores each compiled filter under its identifier, so only file-name
/// characters of it are kept, bounded.
fn rule_identity(rule: &ContentRuleList) -> String {
    let mut id: String = rule
        .id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if id.is_empty() {
        id.push('_');
    }
    format!("{id}-{:016x}", rules_hash(&rule.rules))
}

/// A hash of a rule list's JSON: tells the same list from a replaced one
/// of the same identifier.
fn rules_hash(rules: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    rules.hash(&mut hasher);
    hasher.finish()
}

/// Compiles `rules` as the list WebKit knows by `identity`, in WebKit's
/// loop (the store and its compiler live on the main thread), then hands
/// it to the views waiting (or their views the error).
fn compile_filter(identity: String, rules: Arc<str>) {
    glib::idle_add_local_once(move || {
        let bytes = glib::Bytes::from(rules.as_bytes());
        let key = identity.clone();
        store().save(
            &identity,
            &bytes,
            None::<&gio::Cancellable>,
            move |result| {
                let waiters =
                    FILTERS.with(|f| f.borrow_mut().pending.remove(&key).unwrap_or_default());
                match result {
                    Ok(filter) => {
                        FILTERS.with(|f| f.borrow_mut().ready.insert(key, filter.clone()));
                        for waiter in waiters {
                            waiter(Ok(&filter));
                        }
                    }
                    Err(err) => {
                        log::warn!("web view: content rules {key}: {err}");
                        let message = err.to_string();
                        for waiter in waiters {
                            waiter(Err(message.clone()));
                        }
                    }
                }
            },
        );
    });
}

/// What a web view's signal handlers share.
pub(super) struct Shared {
    emitter: Emitter,
    /// Async callbacks may outlive this view or the engine it belonged to.
    live: Cell<bool>,
    /// What the app may show (its `webview` capability); `None`: nothing
    /// is known about the app, so nothing is refused.
    access: Option<Arc<Access>>,
    /// The current navigation failed (WebKit still finishes it).
    failed: Cell<bool>,
    /// Whether the wanted content rules are installed: a source, a
    /// navigation command or a page's navigation waits until they are
    /// (see [`start_rules`]).
    rules_ready: Cell<bool>,
    /// Where automatic downloads are saved (the app's data directory's
    /// `downloads`); `None`: nothing is downloaded.
    downloads_root: Option<PathBuf>,
    /// How this view treats downloads ([`WebProps::download_behavior`]).
    download_behavior: Cell<WebDownloadBehavior>,
    /// Downloads WebKit started for this page, by their id.
    downloads: RefCell<HashMap<String, Rc<DownloadState>>>,
    /// The id the next download is reported with.
    next_download: Cell<u64>,
}

impl Shared {
    fn emit(&self, kind: EventKind, payload: impl FnOnce() -> EventPayload) {
        if self.live.get() {
            self.emitter.emit(kind, payload);
        }
    }

    /// Answers a [`Command::WebViewRequest`] `id`: its `value`, or the
    /// error. Sent even when nothing listens: the core is waiting.
    fn reply(&self, id: u64, result: Result<JsonValue, String>) {
        if !self.live.get() {
            return;
        }
        self.emitter
            .reply(EventKind::WebViewReply, webview::reply(id, result));
    }

    /// Whether this page's downloads are wanted: `onDownload` in automatic
    /// mode (with a directory to save them in), `onDownloadPending` in
    /// manual mode.
    fn wants_downloads(&self) -> bool {
        match self.download_behavior.get() {
            WebDownloadBehavior::Automatic => {
                self.downloads_root.is_some() && self.emitter.listens(EventKind::Download)
            }
            WebDownloadBehavior::Manual => self.emitter.listens(EventKind::DownloadPending),
            // A behavior this backend does not know: nothing may be
            // written without the app's say.
            _ => false,
        }
    }
}

/// A download WebKit started for a page: reported, then either saved to a
/// directory of its own (automatic) or held without one (nothing is
/// written) until the app accepts a destination or cancels.
///
/// A manual download writes to a staging file of its own beside the app's
/// destination and is published there only once it finished; a cancel or
/// a failure removes the staging file, never the destination (which may
/// hold an older file of the app's).
struct DownloadState {
    download: Download,
    /// Handlers on `download`, disconnected when it ends.
    handlers: RefCell<Vec<glib::SignalHandlerId>>,
    /// The id its events use.
    id: String,
    url: String,
    /// The name WebKit suggested, sanitized.
    filename: RefCell<String>,
    /// Where it is reported: the app's destination once accepted (manual)
    /// or its file in the app's data directory (automatic).
    path: RefCell<Option<PathBuf>>,
    /// What WebKit writes: `path` itself for an automatic download, a
    /// hidden file beside it for a manual one until it is published.
    staging: RefCell<Option<PathBuf>>,
    /// The app's destination is a staging file to publish (manual).
    manual: Cell<bool>,
    /// The app asked to replace the destination's file.
    overwrite: Cell<bool>,
    /// The `AcceptDownload` request (its core id) waiting for WebKit to
    /// create the destination file.
    accepting: Cell<Option<u64>>,
    /// The `CancelDownload` request (its core id) waiting for the transfer
    /// to stop, so it can answer once what was written is gone.
    cancelling: Cell<Option<u64>>,
    /// A destination was accepted (it cannot be chosen twice).
    accepted: Cell<bool>,
    /// WebKit finished it: it can no longer be accepted.
    finished: Cell<bool>,
    /// A manual decision was reported (once).
    pending: Cell<bool>,
    /// Its `started` event was reported.
    started: Cell<bool>,
    /// The timeout that withdraws an unanswered manual decision.
    expiry: RefCell<Option<glib::SourceId>>,
}

impl DownloadState {
    /// Stops listening (so the download's handlers do not keep this for
    /// ever).
    fn disconnect(&self) {
        for id in self.handlers.borrow_mut().drain(..) {
            self.download.disconnect(id);
        }
    }

    fn clear_expiry(&self) {
        if let Some(source) = self.expiry.borrow_mut().take() {
            source.remove();
        }
    }

    /// Cancels the download for teardown, keeping its terminal callbacks
    /// connected: what it wrote (a staging file, or the backend's own
    /// directory) goes only once WebKit reports the transfer stopped.
    fn cancel_for_cleanup(&self) {
        self.clear_expiry();
        if !self.finished.get() {
            self.download.cancel();
        }
    }
}

/// A web view: the page filling its node's view.
pub(super) struct WebHost {
    web: WebView,
    content: UserContentManager,
    /// The app's network session (cookies, downloads).
    session: NetworkSession,
    /// What the handlers share (replies, downloads).
    shared: Rc<Shared>,
    /// Handlers on `web` (disconnected when the host goes).
    handlers: Vec<glib::SignalHandlerId>,
    /// The session's `download-started` handler: this view's downloads.
    download_started: Option<glib::SignalHandlerId>,
    /// The script message handler, on `content`.
    message: Option<glib::SignalHandlerId>,
    /// The props applied (`None` before the first).
    props: Option<WebProps>,
    /// Counts the content rule changes, so a filter WebKit compiles for
    /// rules a view no longer has is not applied (see [`start_rules`]).
    rules_gen: Rc<Cell<u64>>,
    /// The surface and node (content rules finish asynchronously and are
    /// applied back to this host).
    surface: Weak<Surface>,
    node: NodeId,
    /// The source that waits for the content rules.
    rules_waiting: Option<WebSource>,
    /// The app's navigation commands issued while the wanted content
    /// rules compile (or failed to): they run in order once the rules are
    /// installed or cleared, and never before.
    navigation: Vec<Command>,
    /// WebKit's own background (white), for views without an opaque one.
    default_background: gdk::RGBA,
    /// A page to load once the web view is mapped ([`WebHost::load`]).
    pending: Rc<RefCell<Option<WebSource>>>,
    /// Serves the URL schemes of the first props while the web view is
    /// here (held for its drop).
    _schemes: Option<super::web_scheme::SchemeServer>,
}

impl WebHost {
    /// Makes the page with its first props: `incognito` gives it an
    /// ephemeral network session of its own, and it serves the app's URL
    /// schemes.
    pub(super) fn new(surface: &Rc<Surface>, view: &NshView, p: &WebProps) -> Self {
        let incognito = p.incognito;
        let shared = Rc::new(Shared {
            emitter: Emitter {
                surface: Rc::downgrade(surface),
                view: view.downgrade(),
                id: view.id(),
            },
            live: Cell::new(true),
            access: surface.native.access.borrow().clone(),
            failed: Cell::new(false),
            rules_ready: Cell::new(true),
            downloads_root: surface.native.downloads.borrow().clone(),
            download_behavior: Cell::new(p.download_behavior),
            downloads: RefCell::default(),
            next_download: Cell::new(0),
        });
        let content = UserContentManager::new();
        content.register_script_message_handler(HANDLER, None);
        let session = if incognito {
            NetworkSession::new_ephemeral()
        } else {
            session(&surface.native.app_id.borrow())
        };
        let web = WebView::builder()
            .network_session(&session)
            .user_content_manager(&content)
            .build();
        let schemes = super::web_scheme::serve(&web, &p.url_schemes);
        let default_background = web.background_color();
        let message = {
            let shared = shared.clone();
            content.connect_script_message_received(Some(HANDLER), move |_, value| {
                let data = value.to_str().to_string();
                shared.emit(EventKind::Message, || {
                    EventPayload::Map(vec![("data", PayloadValue::Str(data))])
                });
            })
        };
        let mut handlers = vec![
            decide_policy(&web, &shared),
            permission_request(&web, &shared),
            load_changed(&web, &shared),
            load_failed(&web, &shared),
            web.connect_estimated_load_progress_notify({
                let shared = shared.clone();
                move |web| {
                    shared.emit(EventKind::WebViewProgress, || progress(web));
                }
            }),
            web.connect_web_process_terminated(|_, reason| {
                log::warn!(target: "js", "web view: the page's process ended ({reason:?})");
            }),
        ];
        let download_started = {
            let shared = shared.clone();
            let page = web.clone();
            session.connect_download_started(move |_, download| {
                // Every page of the app hears the app's session: only the
                // page that started it answers for it.
                if download.web_view().is_some_and(|view| view == page) {
                    started_download(&shared, download);
                }
            })
        };
        let pending: Rc<RefCell<Option<WebSource>>> = Rc::default();
        handlers.push(web.connect_map({
            let pending = pending.clone();
            move |web| {
                let source = pending.borrow_mut().take();
                if let Some(source) = source {
                    load(web, &source);
                }
            }
        }));
        history_keys(&web, surface);
        fill_sized(view, web.upcast_ref());
        WebHost {
            web,
            content,
            session,
            shared,
            handlers,
            download_started: Some(download_started),
            message: Some(message),
            pending,
            props: None,
            rules_gen: Rc::new(Cell::new(0)),
            surface: Rc::downgrade(surface),
            node: view.id(),
            rules_waiting: None,
            navigation: Vec::new(),
            default_background,
            _schemes: schemes,
        }
    }

    /// The page (to call outside the hosts' borrow).
    pub(super) fn web(&self) -> WebView {
        self.web.clone()
    }

    /// The page and what its requests share: a request is answered with
    /// these, outside the hosts' borrow (WebKit may call back at once).
    pub(super) fn request_target(&self) -> (WebView, Rc<Shared>) {
        (self.web.clone(), self.shared.clone())
    }

    pub(super) fn set(&mut self, view: &NshView, p: &WebProps) {
        let web = &self.web;
        let old = self.props.take();
        let first = old.is_none();
        let old = old.unwrap_or_default();
        self.shared.download_behavior.set(p.download_behavior);
        let scripts_changed = first
            || p.injected_js != old.injected_js
            || p.injected_js_before != old.injected_js_before;
        if scripts_changed {
            self.content.remove_all_scripts();
            let add = |source: &str, time: UserScriptInjectionTime| {
                self.content.add_script(&UserScript::new(
                    source,
                    UserContentInjectedFrames::TopFrame,
                    time,
                    &[],
                    &[],
                ));
            };
            add(BRIDGE, UserScriptInjectionTime::Start);
            if let Some(js) = &p.injected_js_before {
                add(js, UserScriptInjectionTime::Start);
            }
            if let Some(js) = &p.injected_js {
                add(js, UserScriptInjectionTime::End);
            }
        }
        if first || p.content_rules != old.content_rules {
            self.rules_gen.set(self.rules_gen.get().wrapping_add(1));
            if p.content_rules.is_empty() {
                // Blocking off: clear at once and let the page load.
                self.content.remove_all_filters();
                self.shared.rules_ready.set(true);
            } else {
                // The rules in place keep applying while these compile;
                // the page must not go out before they are the wanted
                // ones (its first request could be an ad).
                self.shared.rules_ready.set(false);
                start_rules(
                    self.surface.clone(),
                    Rc::downgrade(&self.shared),
                    self.node,
                    self.rules_gen.get(),
                    &p.content_rules,
                );
            }
        }
        if first || p.scrollable != old.scrollable {
            self.content.remove_all_style_sheets();
            if p.scrollable {
                web.remove_css_class(UNSCROLLABLE);
            } else {
                self.content.add_style_sheet(&UserStyleSheet::new(
                    NO_SCROLL,
                    UserContentInjectedFrames::TopFrame,
                    UserStyleLevel::User,
                    &[],
                    &[],
                ));
                web.add_css_class(UNSCROLLABLE);
            }
        }
        if let Some(settings) = WebViewExt::settings(web) {
            settings.set_user_agent(p.user_agent.as_deref());
            settings.set_enable_javascript_markup(p.javascript_enabled);
            settings.set_enable_back_forward_navigation_gestures(p.back_forward_gestures);
        }
        // An opaque background of the view shows until the page paints
        // (and behind pages without one of their own), as on Windows.
        let background = view.state().style.background_color;
        if background.to_f64().3 >= 1.0 {
            web.set_background_color(&rgba(background));
        } else {
            web.set_background_color(&self.default_background);
        }
        // Scripts apply to pages loaded from now on: load again, once the
        // wanted content rules are the ones applied (a page loaded before
        // that would send its first requests unblocked).
        let wants_load = first || p.source != old.source || scripts_changed;
        self.props = Some(p.clone());
        if wants_load {
            if self.shared.rules_ready.get() {
                self.rules_waiting = None;
                self.load(&p.source);
            } else {
                self.rules_waiting = Some(p.source.clone());
            }
        } else if self.shared.rules_ready.get() {
            // The rules are complete now (the user cleared blocking, or
            // changed to rules that installed): the page that waited for
            // them may load. A set that failed leaves them not ready, so
            // its page stays held.
            let source = self.rules_waiting.take();
            if let Some(source) = source {
                self.load(&source);
            }
        }
        // Cleared, or new rules installed: what the app asked to navigate
        // to while they compiled runs now, in order, and never before.
        if self.shared.rules_ready.get() {
            self.release_navigation();
        }
    }

    /// Holds `command` while the wanted content rules are not installed
    /// (or failed to compile): whether it was held (`false`: run it now).
    /// Only the app's navigations wait; scripts, messages and the
    /// requests (screenshots, cookies, the inspector) do not.
    pub(super) fn queue_navigation(&mut self, command: &Command) -> bool {
        if !waits_for_rules(self.shared.rules_ready.get(), command) {
            return false;
        }
        self.navigation.push(command.clone());
        true
    }

    /// The app's navigations that waited for the content rules, in the
    /// order they came. Taken first, so a command's WebKit callbacks (and
    /// a further `set`) cannot reach this queue.
    fn release_navigation(&mut self) {
        let queued = std::mem::take(&mut self.navigation);
        for navigation in queued {
            command(&self.web, &navigation);
        }
    }
}

/// Answers a [`Command::WebViewRequest`] for `web`, whose handlers share
/// `shared`: the id comes back in the reply
/// ([`EventKind::WebViewReply`]) on the node the request arrived at.
pub(super) fn request(web: &WebView, shared: &Rc<Shared>, request: &WebViewRequest) {
    let id = request.id;
    match &request.operation {
        WebViewOperation::CapturePage => capture(web, shared, id),
        WebViewOperation::GetCookies { uri } => {
            get_cookies(web, shared, id, uri.as_deref());
        }
        WebViewOperation::SetCookies { cookies } => {
            set_cookies(web, shared, id, cookies);
        }
        WebViewOperation::OpenDevtools => open_devtools(web, shared, id),
        WebViewOperation::SetInspectable { enabled } => {
            set_inspectable(web, shared, id, *enabled);
        }
        WebViewOperation::AcceptDownload {
            id: download,
            path,
            overwrite,
        } => {
            accept_download(shared, download, id, path, *overwrite);
        }
        WebViewOperation::CancelDownload { id: download } => {
            cancel_download(shared, download, id);
        }
        // A newer core's operation: answered, never left waiting.
        _ => shared.reply(id, Err("unsupported web view operation".into())),
    }
}

impl WebHost {
    /// Loads `source`, once the web view is mapped ([`load_into`]).
    fn load(&self, source: &WebSource) {
        load_into(&self.web, &self.pending, source);
    }
}

/// Loads `source`, once the web view is mapped. With WebKit's DMA-BUF
/// renderer, a page loaded before its web view is mapped sometimes never
/// shows (about one in six, on X11 and Wayland alike): it loads, but the
/// view stays blank for good, whatever happens to it later. Loaded once
/// mapped, as WebKit's MiniBrowser does, it always shows.
fn load_into(web: &WebView, pending: &RefCell<Option<WebSource>>, source: &WebSource) {
    if web.is_mapped() {
        pending.borrow_mut().take();
        load(web, source);
    } else {
        *pending.borrow_mut() = Some(source.clone());
    }
}

fn load(web: &WebView, source: &WebSource) {
    match source {
        WebSource::Uri(uri) => web.load_uri(uri),
        WebSource::Html { html, base_url } => web.load_html(html, base_url.as_deref()),
        WebSource::None => {}
    }
}

impl Drop for WebHost {
    fn drop(&mut self) {
        self.shared.live.set(false);
        // A removal animation may keep the view on screen a little longer:
        // it shows the page as it was, without loading or reporting.
        self.web.stop_loading();
        for id in self.handlers.drain(..) {
            self.web.disconnect(id);
        }
        if let Some(id) = self.download_started.take() {
            self.session.disconnect(id);
        }
        // Downloads of this page still under way: cancelled. Their
        // terminal callbacks stay connected (they only clean up now): what
        // an automatic one wrote in the backend's own directory, or a
        // manual one's staging file, goes once WebKit reports the
        // transfer stopped. The app's destinations are never touched.
        let downloads = std::mem::take(&mut *self.shared.downloads.borrow_mut());
        for download in downloads.into_values() {
            download.cancel_for_cleanup();
        }
        if let Some(id) = self.message.take() {
            self.content.disconnect(id);
        }
        self.content
            .unregister_script_message_handler(HANDLER, None);
    }
}

/// Whether `command` navigates the page: the app-driven navigations,
/// which wait while the wanted content rules are not installed.
fn is_navigation(command: &Command) -> bool {
    matches!(
        command,
        Command::Reload | Command::Load(_) | Command::GoBack | Command::GoForward
    )
}

/// Whether `command` must wait for the requested content rules: nothing
/// may load or navigate before they are installed (or cleared), so a page
/// cannot reach the network with the wrong policy.
fn waits_for_rules(rules_ready: bool, command: &Command) -> bool {
    !rules_ready && is_navigation(command)
}

/// `reload()`, `goBack()`, `goForward()`, `stopLoading()`,
/// `injectJavaScript()`, `postMessage()` and `load()`.
pub(super) fn command(web: &WebView, command: &Command) {
    let script = match command {
        Command::InjectJavaScript(js) => Some(js.clone()),
        Command::PostMessage(data) => Some(post_message(data)),
        _ => None,
    };
    match (command, script) {
        (_, Some(js)) => {
            web.evaluate_javascript(&js, None, None, None::<&gio::Cancellable>, |result| {
                // Scripts whose value cannot be returned still ran.
                if let Err(err) = result {
                    log::debug!("web view: script: {err}");
                }
            })
        }
        (Command::Reload, _) => web.reload(),
        (Command::GoBack, _) => web.go_back(),
        (Command::GoForward, _) => web.go_forward(),
        (Command::StopLoading, _) => web.stop_loading(),
        (Command::Load(load), _) => {
            // Only this request carries the headers; WebKit makes its
            // subresources' and later navigations' requests itself.
            let request = URIRequest::new(&load.src);
            if let Some(headers) = request.http_headers() {
                for (name, value) in &load.headers {
                    headers.append(name, value);
                }
            }
            web.load_request(&request);
        }
        _ => {}
    }
}

/// Navigations stay within the app's `webview` capability, and none
/// starts while the requested content rules are not installed: the app's
/// commands wait in the host's queue, a page's own navigation is refused
/// with `onError` until the rules are in. A response the page cannot
/// show, or sent as an attachment, is downloaded when the app wants
/// downloads and ignored otherwise.
fn decide_policy(web: &WebView, shared: &Rc<Shared>) -> glib::SignalHandlerId {
    let shared = shared.clone();
    web.connect_decide_policy(move |_, decision, kind| match kind {
        PolicyDecisionType::NavigationAction | PolicyDecisionType::NewWindowAction => {
            let url = decision
                .downcast_ref::<NavigationPolicyDecision>()
                .and_then(|d| d.navigation_action())
                .and_then(|a| a.request())
                .and_then(|r| r.uri())
                .map(|u| u.to_string())
                .unwrap_or_default();
            // A page's own navigation (a link, a script, a form) waits for
            // the requested content rules too: an already-loaded page may
            // keep the rules installed before them, but nothing new goes
            // out unfiltered. The app's commands wait in the host's queue
            // (`queue_navigation`).
            if !shared.rules_ready.get() {
                decision.ignore();
                log::warn!(
                    target: "js",
                    "web view navigation to {url} waits for the content rules"
                );
                let message = format!("navigation to {url} waits for the content rules");
                shared.emit(EventKind::Error, || {
                    EventPayload::Map(vec![
                        ("message", PayloadValue::Str(message)),
                        ("url", PayloadValue::Str(url.clone())),
                    ])
                });
                return true;
            }
            if allows(shared.access.as_deref(), &url) {
                // WebKit's own handling: it loads (a new window opens
                // nothing, as there is no `create` handler).
                return false;
            }
            decision.ignore();
            log::warn!(target: "js", "web view navigation to {url} is not granted");
            let message = format!("navigation to {url} is not granted");
            shared.emit(EventKind::Error, || {
                EventPayload::Map(vec![
                    ("message", PayloadValue::Str(message)),
                    (
                        "code",
                        PayloadValue::Str(nsh_core::element::codes::ACCESS_DENIED.into()),
                    ),
                    ("url", PayloadValue::Str(url)),
                ])
            });
            true
        }
        PolicyDecisionType::Response => match decision.downcast_ref::<ResponsePolicyDecision>() {
            Some(response) if is_download(response) => {
                if shared.wants_downloads() {
                    // WebKit hands it to the session's downloads, which
                    // hold it for the destination the host decides.
                    decision.download();
                } else {
                    // Nothing listens: refused like a page WebKit cannot
                    // show, so nothing goes to the system's Downloads.
                    response.ignore();
                }
                true
            }
            Some(response) if !response.is_mime_type_supported() => {
                response.ignore();
                true
            }
            _ => false,
        },
        _ => false,
    })
}

/// The page's `getUserMedia` gets what the app's `webview.media` grants;
/// WebKit's other permission requests keep its own handling.
fn permission_request(web: &WebView, shared: &Rc<Shared>) -> glib::SignalHandlerId {
    use nsh_core::access::WebMedia;
    let allows = |media| {
        shared
            .access
            .as_deref()
            .is_some_and(|a| a.allows_web_media(media))
    };
    let (microphone, camera) = (allows(WebMedia::Microphone), allows(WebMedia::Camera));
    if (microphone || camera)
        && let Some(settings) = WebViewExt::settings(web)
    {
        settings.set_enable_media_stream(true);
    }
    web.connect_permission_request(move |_, request| {
        let Some(media) = request.downcast_ref::<UserMediaPermissionRequest>() else {
            return false;
        };
        let granted = !webkit6::functions::user_media_permission_is_for_display_device(media)
            && (!media.is_for_audio_device() || microphone)
            && (!media.is_for_video_device() || camera);
        if granted {
            request.allow();
        } else {
            log::warn!(target: "js", "web view media capture is not granted: add it to capabilities.webview.media");
            request.deny();
        }
        true
    })
}

/// Whether the app may show `url` in a web view (`access`: its
/// capabilities, `None` when unknown).
fn allows(access: Option<&Access>, url: &str) -> bool {
    let Some(access) = access else { return true };
    match url {
        "" => true,
        // A frame's inline document (`srcdoc`): the page's own content.
        "about:srcdoc" => access.allows_webviews(),
        _ => access.allows_webview(url),
    }
}

/// Whether a response is a file to save: the main frame's, of a type
/// WebKit cannot show or sent as an attachment.
fn is_download(decision: &ResponsePolicyDecision) -> bool {
    if !decision.is_main_frame_main_resource() {
        return false;
    }
    if !decision.is_mime_type_supported() {
        return true;
    }
    decision
        .response()
        .and_then(|response| response.http_headers())
        .and_then(|headers| headers.content_disposition())
        .is_some_and(|(kind, _)| kind.eq_ignore_ascii_case("attachment"))
}

fn load_changed(web: &WebView, shared: &Rc<Shared>) -> glib::SignalHandlerId {
    let shared = shared.clone();
    web.connect_load_changed(move |web, event| match event {
        LoadEvent::Started => {
            shared.failed.set(false);
            shared.emit(EventKind::LoadStart, || navigation_state(web, true));
            shared.emit(EventKind::NavigationStateChange, || {
                navigation_state(web, true)
            });
            shared.emit(EventKind::WebViewProgress, || progress(web));
        }
        // A failed navigation finishes too: it was reported as an error.
        LoadEvent::Finished if !shared.failed.get() => {
            shared.emit(EventKind::Load, || navigation_state(web, false));
            shared.emit(EventKind::NavigationStateChange, || {
                navigation_state(web, false)
            });
            shared.emit(EventKind::WebViewProgress, || progress(web));
        }
        _ => {}
    })
}

fn load_failed(web: &WebView, shared: &Rc<Shared>) -> glib::SignalHandlerId {
    let shared = shared.clone();
    web.connect_load_failed(move |_, _, _, err| {
        shared.failed.set(true);
        // Stopped, replaced by another navigation, or a response refused
        // above: not errors.
        let cancelled = err.matches(NetworkError::Cancelled)
            || err.matches(PolicyError::FrameLoadInterruptedByPolicyChange);
        if !cancelled {
            let message = err.message().to_owned();
            shared.emit(EventKind::Error, || error(message));
        }
        // The app shows failures itself: no WebKit error page.
        true
    })
}

/// The page's estimated load progress, as WebKit measures it
/// (`onLoadProgress`: `{ progress }`, `null` when unknown).
fn progress(web: &WebView) -> EventPayload {
    webview::progress_payload(Some(nsh_core::geometry::Percentage(
        web.estimated_load_progress() as f32,
    )))
}

/// A PNG of the page as shown, base64-encoded.
fn capture(web: &WebView, shared: &Rc<Shared>, id: u64) {
    let shared = shared.clone();
    web.snapshot(
        SnapshotRegion::Visible,
        SnapshotOptions::NONE,
        None::<&gio::Cancellable>,
        move |result| match result {
            Ok(texture) => {
                let png = texture.save_to_png_bytes();
                shared.reply(id, Ok(JsonValue::String(BASE64.encode(png.as_ref()))));
            }
            Err(err) => shared.reply(id, Err(err.message().to_owned())),
        },
    );
}

/// The app's cookie manager (this view's network session's).
fn cookie_manager(web: &WebView) -> Option<webkit6::CookieManager> {
    web.network_session()?.cookie_manager()
}

/// The cookies of `uri`, or every cookie in this view's session when
/// omitted.
fn get_cookies(web: &WebView, shared: &Rc<Shared>, id: u64, uri: Option<&str>) {
    let Some(manager) = cookie_manager(web) else {
        shared.reply(id, Err("the web view has no cookie manager".into()));
        return;
    };
    let shared = shared.clone();
    let done = move |result: Result<Vec<soup::Cookie>, glib::Error>| match result {
        Ok(cookies) => {
            let cookies: Vec<WebCookie> = cookies.into_iter().map(web_cookie).collect();
            match serde_json::to_value(&cookies) {
                Ok(value) => shared.reply(id, Ok(value)),
                Err(err) => shared.reply(id, Err(err.to_string())),
            }
        }
        Err(err) => shared.reply(id, Err(err.message().to_owned())),
    };
    match uri {
        Some(uri) => manager.cookies(uri, None::<&gio::Cancellable>, done),
        None => manager.all_cookies(None::<&gio::Cancellable>, done),
    }
}

/// A cookie of the view's store, for the core (expiry in Unix
/// milliseconds).
fn web_cookie(mut cookie: soup::Cookie) -> WebCookie {
    let mut out = WebCookie::default();
    out.name = cookie.name().map(|s| s.to_string()).unwrap_or_default();
    out.value = cookie.value().map(|s| s.to_string()).unwrap_or_default();
    out.domain = cookie.domain().map(|s| s.to_string()).unwrap_or_default();
    out.path = cookie.path().map(|s| s.to_string()).unwrap_or_default();
    out.expires = cookie
        .expires()
        .map(|expires| expires.to_unix().saturating_mul(1000));
    out.http_only = cookie.is_http_only();
    out.secure = cookie.is_secure();
    out.same_site = match cookie.same_site_policy() {
        soup::SameSitePolicy::None => Some(CookieSameSite::None),
        soup::SameSitePolicy::Lax => Some(CookieSameSite::Lax),
        soup::SameSitePolicy::Strict => Some(CookieSameSite::Strict),
        // Not a policy WebKit stores.
        _ => None,
    };
    out
}

/// Stores `cookies` in the view's cookie manager.
fn set_cookies(web: &WebView, shared: &Rc<Shared>, id: u64, cookies: &[WebCookie]) {
    let Some(manager) = cookie_manager(web) else {
        shared.reply(id, Err("the web view has no cookie manager".into()));
        return;
    };
    // All converted first: an invalid cookie stores none of them.
    let mut soup_cookies = Vec::with_capacity(cookies.len());
    for cookie in cookies {
        match soup_cookie(cookie) {
            Ok(cookie) => soup_cookies.push(cookie),
            Err(err) => {
                shared.reply(id, Err(err));
                return;
            }
        }
    }
    if soup_cookies.is_empty() {
        shared.reply(id, Ok(JsonValue::Null));
        return;
    }
    // WebKit adds cookies one by one: answered when the last one is in
    // (the first error, if any, is the answer).
    let remaining = Rc::new(Cell::new(soup_cookies.len()));
    let error: Rc<RefCell<Option<String>>> = Rc::default();
    for cookie in soup_cookies {
        let remaining = remaining.clone();
        let error = error.clone();
        let shared = shared.clone();
        manager.add_cookie(&cookie, None::<&gio::Cancellable>, move |result| {
            if let Err(err) = result
                && error.borrow().is_none()
            {
                *error.borrow_mut() = Some(err.message().to_owned());
            }
            let left = remaining.get() - 1;
            remaining.set(left);
            if left == 0 {
                match error.borrow_mut().take() {
                    Some(error) => shared.reply(id, Err(error)),
                    None => shared.reply(id, Ok(JsonValue::Null)),
                }
            }
        });
    }
}

/// A core cookie as the cookie manager's (WebKit's `set_expires` takes
/// seconds).
fn soup_cookie(cookie: &WebCookie) -> Result<soup::Cookie, String> {
    let mut out = soup::Cookie::new(
        &cookie.name,
        &cookie.value,
        &cookie.domain,
        &cookie.path,
        -1,
    );
    if let Some(expires) = cookie.expires {
        let date = glib::DateTime::from_unix_utc(expires.div_euclid(1000))
            .map_err(|err| format!("cookie `{}` expiry: {err}", cookie.name))?;
        out.set_expires(&date);
    }
    out.set_http_only(cookie.http_only);
    out.set_secure(cookie.secure);
    // Without one, the attribute is left out (libsoup's default, `Lax`).
    if let Some(same_site) = cookie.same_site.as_ref() {
        let policy = match same_site {
            CookieSameSite::None => soup::SameSitePolicy::None,
            CookieSameSite::Lax => soup::SameSitePolicy::Lax,
            CookieSameSite::Strict => soup::SameSitePolicy::Strict,
            _ => {
                return Err(format!(
                    "cookie `{}` has an unknown SameSite policy",
                    cookie.name
                ));
            }
        };
        out.set_same_site_policy(policy);
    }
    Ok(out)
}

/// Opens WebKit's inspector (its developer extras on).
fn open_devtools(web: &WebView, shared: &Rc<Shared>, id: u64) {
    let Some(settings) = WebViewExt::settings(web) else {
        shared.reply(id, Err("the web view has no settings".into()));
        return;
    };
    settings.set_enable_developer_extras(true);
    match web.inspector() {
        Some(inspector) => {
            inspector.show();
            shared.reply(id, Ok(JsonValue::Null));
        }
        None => shared.reply(
            id,
            Err("WebKitGTK has no inspector for this web view".into()),
        ),
    }
}

/// Whether the page can be inspected (WebKit's developer extras): off
/// until a command turns it on.
fn set_inspectable(web: &WebView, shared: &Rc<Shared>, id: u64, enabled: bool) {
    let Some(settings) = WebViewExt::settings(web) else {
        shared.reply(id, Err("the web view has no settings".into()));
        return;
    };
    if !enabled && let Some(inspector) = web.inspector() {
        inspector.close();
    }
    settings.set_enable_developer_extras(enabled);
    shared.reply(id, Ok(JsonValue::Null));
}

/// WebKit reported a download of this page: reported and either saved
/// (automatic) or held without a destination (manual) until the app
/// decides.
fn started_download(shared: &Rc<Shared>, download: &Download) {
    let n = shared.next_download.get();
    shared.next_download.set(n + 1);
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let state = Rc::new(DownloadState {
        download: download.clone(),
        handlers: RefCell::default(),
        id: download_id(at, n),
        url: download
            .request()
            .and_then(|request| request.uri())
            .map(|uri| uri.to_string())
            .unwrap_or_default(),
        filename: RefCell::default(),
        path: RefCell::default(),
        staging: RefCell::default(),
        manual: Cell::new(false),
        overwrite: Cell::new(false),
        accepting: Cell::new(None),
        cancelling: Cell::new(None),
        accepted: Cell::new(false),
        finished: Cell::new(false),
        pending: Cell::new(false),
        started: Cell::new(false),
        expiry: RefCell::default(),
    });
    shared
        .downloads
        .borrow_mut()
        .insert(state.id.clone(), state.clone());
    let mut handlers = Vec::new();
    {
        let shared = shared.clone();
        let state = state.clone();
        handlers.push(
            download.connect_decide_destination(move |download, suggested| {
                // `true` in every case: WebKit never chooses a destination of
                // its own (`~/Downloads`); the host does, or cancels.
                decide_destination(&shared, &state, download, suggested);
                true
            }),
        );
    }
    {
        let shared = shared.clone();
        let state = state.clone();
        handlers.push(download.connect_created_destination(move |_, _| {
            // The app's destination is created: answer its request, then
            // report the download as started.
            let Some(request) = state.accepting.take() else {
                return;
            };
            shared.reply(request, Ok(JsonValue::Null));
            if !state.started.replace(true) {
                let started = download_event("started", &state, None);
                shared.emit(EventKind::Download, || started);
            }
        }));
    }
    {
        let shared = shared.clone();
        let state = state.clone();
        handlers.push(download.connect_failed(move |_, err| {
            let cancelled = err.matches(DownloadError::CancelledByUser);
            state.finished.set(true);
            state.clear_expiry();
            state.disconnect();
            shared.downloads.borrow_mut().remove(&state.id);
            // The transfer stopped: what it wrote is gone (a staging file,
            // or the backend's own directory).
            cleanup(&shared, &state);
            if let Some(request) = state.accepting.take() {
                shared.reply(
                    request,
                    Err(if cancelled {
                        "download cancelled".into()
                    } else {
                        err.message().to_owned()
                    }),
                );
            }
            if let Some(request) = state.cancelling.take() {
                // The app's cancel is answered now that nothing of it
                // remains.
                shared.reply(request, Ok(JsonValue::Null));
                return;
            }
            if cancelled {
                return;
            }
            log::warn!("web view: download failed: {err}");
            let failed = failed_event(&state, err.message().to_owned());
            shared.emit(EventKind::Download, || failed);
        }));
    }
    {
        let shared = shared.clone();
        let state = state.clone();
        handlers.push(download.connect_finished(move |_| {
            state.finished.set(true);
            state.clear_expiry();
            state.disconnect();
            shared.downloads.borrow_mut().remove(&state.id);
            if let Some(request) = state.cancelling.take() {
                // A cancel arrived before the last bytes were taken: what
                // it wrote is discarded, and nothing is published.
                cleanup(&shared, &state);
                if let Some(accepting) = state.accepting.take() {
                    shared.reply(accepting, Err("download cancelled".into()));
                }
                shared.reply(request, Ok(JsonValue::Null));
                return;
            }
            // A manual download's staging file becomes the app's file
            // only now; an automatic one is already at its path.
            if state.manual.get() {
                let staging = state.staging.borrow().clone();
                let destination = state.path.borrow().clone();
                if let (Some(staging), Some(destination)) = (staging, destination)
                    && let Err(error) = publish(&staging, &destination, state.overwrite.get())
                {
                    cleanup(&shared, &state);
                    if let Some(request) = state.accepting.take() {
                        shared.reply(request, Err(error.clone()));
                    }
                    let failed = failed_event(&state, error);
                    shared.emit(EventKind::Download, || failed);
                    return;
                }
                state.staging.borrow_mut().take();
            }
            // WebKit creates the destination before it finishes; a reply
            // still waiting resolves here anyway.
            if let Some(request) = state.accepting.take() {
                shared.reply(request, Ok(JsonValue::Null));
            }
            if !state.started.get() {
                return;
            }
            let bytes = state
                .path
                .borrow()
                .as_ref()
                .and_then(|path| std::fs::metadata(path).ok())
                .map(|meta| meta.len());
            let finished = download_event("finished", &state, bytes);
            shared.emit(EventKind::Download, || finished);
        }));
    }
    *state.handlers.borrow_mut() = handlers;
}

/// WebKit asks where a download goes: `Automatic` saves it in the app's
/// data directory, `Manual` reports the choice and holds it.
fn decide_destination(
    shared: &Rc<Shared>,
    state: &Rc<DownloadState>,
    download: &Download,
    suggested: &str,
) {
    if !shared.wants_downloads() {
        // No one could accept it: nothing may be written.
        state.finished.set(true);
        state.clear_expiry();
        state.disconnect();
        shared.downloads.borrow_mut().remove(&state.id);
        download.cancel();
        return;
    }
    let filename = file_name(suggested);
    *state.filename.borrow_mut() = filename.clone();
    match shared.download_behavior.get() {
        WebDownloadBehavior::Automatic => {
            let Some(root) = shared.downloads_root.clone() else {
                // No directory to save in (the app started without one).
                state.finished.set(true);
                state.disconnect();
                shared.downloads.borrow_mut().remove(&state.id);
                download.cancel();
                return;
            };
            // A directory of its own: two files of the same name never
            // meet, and a failed one takes its partial file with it.
            let dir = root.join(&state.id);
            let path = dir.join(&filename);
            if let Err(err) = std::fs::create_dir_all(&dir) {
                log::warn!("web view download into {}: {err}", dir.display());
                let _ = std::fs::remove_dir_all(&dir);
                state.finished.set(true);
                state.disconnect();
                shared.downloads.borrow_mut().remove(&state.id);
                let failed = failed_event(state, err.to_string());
                shared.emit(EventKind::Download, || failed);
                download.cancel();
                return;
            }
            *state.path.borrow_mut() = Some(path.clone());
            *state.staging.borrow_mut() = Some(path.clone());
            state.started.set(true);
            let started = download_event("started", state, None);
            shared.emit(EventKind::Download, || started);
            download.set_destination(&path.to_string_lossy());
        }
        WebDownloadBehavior::Manual => {
            // Reported once; the app answers with AcceptDownload or
            // CancelDownload. An unanswered one is withdrawn.
            if state.pending.replace(true) {
                return;
            }
            let pending = webview::download_pending_payload(&state.id, &state.url, &filename);
            shared.emit(EventKind::DownloadPending, || pending);
            let shared = shared.clone();
            let pending_state = state.clone();
            let expiry = glib::timeout_add_local_once(PENDING_TIMEOUT, move || {
                expire_pending(&shared, &pending_state);
            });
            *state.expiry.borrow_mut() = Some(expiry);
        }
        // A behavior this backend does not know: cancelled, nothing
        // written.
        _ => {
            state.finished.set(true);
            state.disconnect();
            shared.downloads.borrow_mut().remove(&state.id);
            download.cancel();
        }
    }
}

/// A manual download nobody chose a destination for: cancelled, and
/// reported as failed (nothing was written, so there is nothing to
/// remove).
fn expire_pending(shared: &Rc<Shared>, state: &Rc<DownloadState>) {
    state.clear_expiry();
    if state.accepted.get() || state.finished.get() {
        return;
    }
    // Nothing was accepted yet: WebKit holds it without a destination, so
    // the terminal `failed` removes only the tracked state.
    state.download.cancel();
    shared.downloads.borrow_mut().remove(&state.id);
    let failed = failed_event(state, "no download destination was accepted in time".into());
    shared.emit(EventKind::Download, || failed);
}

/// Writes download `id` to the app's chosen `path`, through a staging
/// file of its own that only a finished transfer publishes there.
fn accept_download(shared: &Rc<Shared>, id: &str, request: u64, path: &Path, overwrite: bool) {
    let Some(state) = shared.downloads.borrow().get(id).cloned() else {
        shared.reply(request, Err(format!("unknown download {id}")));
        return;
    };
    if state.finished.get() {
        shared.reply(request, Err(format!("download {id} already finished")));
        return;
    }
    if state.accepted.get() {
        shared.reply(request, Err(format!("download {id} is already accepted")));
        return;
    }
    if state.cancelling.get().is_some() {
        shared.reply(request, Err(format!("download {id} is being cancelled")));
        return;
    }
    if state.started.get() {
        // An automatic download already writes where the platform chose.
        shared.reply(
            request,
            Err(format!("download {id} is already writing its file")),
        );
        return;
    }
    if let Err(error) = check_destination(path, overwrite) {
        shared.reply(request, Err(error));
        return;
    }
    let staging = match stage_path(path, id) {
        Ok(staging) => staging,
        Err(error) => {
            shared.reply(request, Err(error));
            return;
        }
    };
    state.manual.set(true);
    state.overwrite.set(overwrite);
    state.accepted.set(true);
    state.clear_expiry();
    *state.path.borrow_mut() = Some(path.to_path_buf());
    *state.staging.borrow_mut() = Some(staging.clone());
    state.accepting.set(Some(request));
    // The staging file is new, so it must not be replaced: only the
    // finished file goes to `path`, where `overwrite` decides.
    state.download.set_allow_overwrite(false);
    // The answer waits for WebKit: `created-destination` (ok) or
    // `failed` (its error). `set_destination` may call back at once.
    state.download.set_destination(&staging.to_string_lossy());
}

/// Cancels download `id`: answered once WebKit reports the transfer
/// stopped and what it wrote is gone.
fn cancel_download(shared: &Rc<Shared>, id: &str, request: u64) {
    let Some(state) = shared.downloads.borrow().get(id).cloned() else {
        shared.reply(request, Err(format!("unknown download {id}")));
        return;
    };
    if state.finished.get() {
        shared.reply(request, Err(format!("download {id} already finished")));
        return;
    }
    if state.cancelling.get().is_some() {
        shared.reply(request, Err(format!("download {id} is being cancelled")));
        return;
    }
    state.clear_expiry();
    state.cancelling.set(Some(request));
    // The terminal `failed` (or a race's `finished`) removes the staging
    // file or the backend's directory, then answers both this request and
    // a waiting acceptance.
    state.download.cancel();
}

/// A safe id for a download: hex digits only, so it names a directory
/// without escaping it. Fixed widths, so no two differ only in where the
/// clock's digits end.
fn download_id(at: u128, n: u64) -> String {
    format!("{at:032x}{n:016x}")
}

/// A file name for a server's suggested one: its last component, without
/// separators or a leading dot, at most 200 bytes; `download` when nothing
/// is left.
fn file_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name: String = last
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(c, ':' | '\u{061c}' | '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{206f}')
        })
        .collect::<String>()
        .trim()
        .trim_start_matches('.')
        .to_owned();
    if name.len() > 200 {
        // Preserve a normal extension while shortening the stem on UTF-8
        // boundaries.
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| format!(".{ext}"))
            .filter(|ext| ext.len() + name.chars().next().map_or(0, char::len_utf8) <= 200);
        if let Some(extension) = &extension {
            name.truncate(name.len() - extension.len());
        }
        let limit = 200 - extension.as_ref().map_or(0, String::len);
        while name.len() > limit {
            name.pop();
        }
        if let Some(extension) = extension {
            name.push_str(&extension);
        }
    }
    if name.is_empty() {
        "download".to_owned()
    } else {
        name
    }
}

/// Whether `path` may be a download's destination: absolute, in an
/// existing folder, a file (replaced only when `overwrite` says so).
fn check_destination(path: &Path, overwrite: bool) -> Result<(), String> {
    let Some(text) = path.to_str() else {
        return Err("a download destination must be a UTF-8 path".into());
    };
    if text.is_empty() || text.contains('\0') {
        return Err("a download destination must be a valid path".into());
    }
    if !path.is_absolute() {
        return Err(format!("download destination {text} is not absolute"));
    }
    if path.file_name().is_none() {
        return Err(format!("download destination {text} has no file name"));
    }
    let Some(parent) = path.parent() else {
        return Err(format!("download destination {text} has no folder"));
    };
    if !parent.is_dir() {
        return Err(format!(
            "the download destination's folder {} does not exist",
            parent.display()
        ));
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            Err(format!("download destination {text} is a folder"))
        }
        Ok(_) if !overwrite => Err(format!(
            "download destination {text} exists (accept with overwrite to replace it)"
        )),
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("cannot use download destination {text}: {err}")),
    }
}

/// Where a manual download writes before it is published: a hidden file
/// of its own beside the app's destination, so a cancel or a failure
/// never leaves a part of it there.
fn stage_path(destination: &Path, id: &str) -> Result<PathBuf, String> {
    let Some(parent) = destination.parent() else {
        return Err(format!(
            "download destination {} has no folder",
            destination.display()
        ));
    };
    let mut staging = parent.join(format!(".nsh-download-{id}.part"));
    if staging == destination {
        staging = parent.join(format!(".nsh-download-{id}.part.tmp"));
    }
    match std::fs::symlink_metadata(&staging) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(staging),
        Ok(_) => Err(format!(
            "the download's staging file {} exists",
            staging.display()
        )),
        Err(err) => Err(format!("cannot use {}: {err}", staging.display())),
    }
}

/// Moves a finished manual download from its staging file to the app's
/// destination: replacing it only when the app said so (`overwrite`), else
/// never clobbering a file that appeared meanwhile.
fn publish(staging: &Path, destination: &Path, overwrite: bool) -> Result<(), String> {
    if overwrite {
        return std::fs::rename(staging, destination)
            .map_err(|err| format!("cannot replace {}: {err}", destination.display()));
    }
    match std::fs::hard_link(staging, destination) {
        Ok(()) => {
            if let Err(err) = std::fs::remove_file(staging) {
                log::warn!("web view: cannot remove {}: {err}", staging.display());
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
            "{} exists (accept with overwrite to replace it)",
            destination.display()
        )),
        Err(err) => Err(format!(
            "cannot publish {} without replacing an existing file: {err}",
            destination.display()
        )),
    }
}

/// Removes what a failed or cancelled download wrote: its staging file,
/// or the backend's own directory in the app's data directory. Never the
/// app's destination, which may hold an older file of its own.
fn cleanup(shared: &Shared, state: &DownloadState) {
    let dir = automatic_dir(shared, state);
    let staging = state.staging.borrow_mut().take();
    if state.manual.get() {
        if let Some(staging) = staging {
            let _ = std::fs::remove_file(staging);
        }
    } else if let Some(dir) = dir {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The folder an automatic download made for itself (`downloads/<id>`),
/// which only this backend writes in; `None` for a manual download.
fn automatic_dir(shared: &Shared, state: &DownloadState) -> Option<PathBuf> {
    if state.manual.get() {
        return None;
    }
    let root = shared.downloads_root.as_ref()?;
    let staging = state.staging.borrow();
    let dir = staging.as_ref()?.parent()?;
    (dir == root.join(&state.id)).then(|| dir.to_path_buf())
}

/// `onDownload`'s payload for a download that failed, with why.
fn failed_event(state: &DownloadState, message: String) -> EventPayload {
    let mut event = download_event("failed", state, None);
    if let EventPayload::Map(fields) = &mut event {
        fields.push(("message", PayloadValue::Str(message)));
    }
    event
}

/// `onDownload`'s payload: `state`, `id`, `url`, `filename`, `path` and,
/// once finished, `bytes`.
fn download_event(state: &str, download: &DownloadState, bytes: Option<u64>) -> EventPayload {
    let path = download
        .path
        .borrow()
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut fields = vec![
        ("state", PayloadValue::Str(state.to_owned())),
        ("id", PayloadValue::Str(download.id.clone())),
        ("url", PayloadValue::Str(download.url.clone())),
        (
            "filename",
            PayloadValue::Str(download.filename.borrow().clone()),
        ),
        ("path", PayloadValue::Str(path)),
    ];
    if let Some(bytes) = bytes {
        fields.push(("bytes", PayloadValue::Number(bytes as f64)));
    }
    EventPayload::Map(fields)
}

/// `{ url, title, loading, canGoBack, canGoForward }` of a page.
fn navigation_state(web: &WebView, loading: bool) -> EventPayload {
    EventPayload::Map(vec![
        (
            "url",
            PayloadValue::Str(web.uri().map(|u| u.to_string()).unwrap_or_default()),
        ),
        (
            "title",
            PayloadValue::Str(web.title().map(|t| t.to_string()).unwrap_or_default()),
        ),
        ("loading", PayloadValue::Bool(loading)),
        ("canGoBack", PayloadValue::Bool(web.can_go_back())),
        ("canGoForward", PayloadValue::Bool(web.can_go_forward())),
    ])
}

/// Which way a history key or button goes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum History {
    Back,
    Forward,
}

/// The history key `key` with `modifiers` is, if it is one.
fn history_key(key: gdk::Key, modifiers: gdk::ModifierType) -> Option<History> {
    let mods = modifiers
        & (gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::SUPER_MASK);
    let alt = mods == gdk::ModifierType::ALT_MASK;
    match key {
        gdk::Key::Back if mods.is_empty() => Some(History::Back),
        gdk::Key::Forward if mods.is_empty() => Some(History::Forward),
        gdk::Key::Left | gdk::Key::KP_Left if alt => Some(History::Back),
        gdk::Key::Right | gdk::Key::KP_Right if alt => Some(History::Forward),
        _ => None,
    }
}

/// Goes `way` in the page's history, or back in the app when the page
/// has none: whether something took it.
fn go(web: &WebView, surface: &std::rc::Weak<Surface>, way: History) -> bool {
    match way {
        History::Back if web.can_go_back() => web.go_back(),
        History::Forward if web.can_go_forward() => web.go_forward(),
        History::Back => {
            return surface
                .upgrade()
                .is_some_and(|s| crate::navigation::go_back(&s));
        }
        History::Forward => return false,
    }
    true
}

/// History keys and mouse buttons over the page, before WebKit sees them
/// (it keeps arrow keys, used or not).
fn history_keys(web: &WebView, surface: &Rc<Surface>) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = Rc::downgrade(surface);
    keys.connect_key_pressed(move |keys, key, _, modifiers| {
        let taken = keys
            .widget()
            .and_downcast::<WebView>()
            .zip(history_key(key, modifiers))
            .is_some_and(|(web, way)| go(&web, &weak, way));
        if taken {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    web.add_controller(keys);
    let buttons = gtk::GestureClick::new();
    // Any button: only the back (8) and forward (9) ones are taken.
    buttons.set_button(0);
    buttons.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = Rc::downgrade(surface);
    buttons.connect_pressed(move |gesture, _, _, _| {
        let way = match gesture.current_button() {
            8 => History::Back,
            9 => History::Forward,
            _ => return,
        };
        if let Some(web) = gesture.widget().and_downcast::<WebView>()
            && go(&web, &weak, way)
        {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    web.add_controller(buttons);
}

/// A JS string literal for `s`.
fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The script `postMessage(data)` runs in the page.
fn post_message(data: &str) -> String {
    format!(
        "window.dispatchEvent(new MessageEvent('message', {{ data: {} }}));",
        js_string(data)
    )
}

/// Where web views of app `app_id` keep their data and cache.
fn folders(app_id: &str) -> (PathBuf, PathBuf) {
    let app = if app_id.is_empty() { "nsh" } else { app_id };
    // An id is a reverse domain name: keep it a single folder name.
    let app: String = app
        .chars()
        .map(|c| if c == '/' || c == '\0' { '_' } else { c })
        .collect();
    let app = if app == "." || app == ".." {
        "nsh".to_owned()
    } else {
        app
    };
    (
        glib::user_data_dir().join(&app).join("webkit"),
        glib::user_cache_dir().join(&app).join("webkit"),
    )
}

thread_local! {
    /// Network sessions by app id: one network process (and one set of
    /// cookies and storage) serves every web view of an app. Weak: each
    /// web view holds its session, which ends with the app's last one.
    static SESSIONS: RefCell<HashMap<String, glib::WeakRef<NetworkSession>>> = RefCell::default();
}

/// The network session of app `app_id`.
fn session(app_id: &str) -> NetworkSession {
    shared_session(app_id, || {
        let (data, cache) = folders(app_id);
        if let Err(err) = std::fs::create_dir_all(&data) {
            log::warn!("web view: cannot create {}: {err}", data.display());
        }
        let session = NetworkSession::new(data.to_str(), cache.to_str());
        // Cookies outlive the app, as other website data does.
        if let Some(cookies) = session.cookie_manager() {
            cookies.set_persistent_storage(
                &data.join("cookies.sqlite").to_string_lossy(),
                CookiePersistentStorage::Sqlite,
            );
        }
        session
    })
}

/// The session app `app_id`'s web views use, else the one `new` makes.
fn shared_session(app_id: &str, new: impl FnOnce() -> NetworkSession) -> NetworkSession {
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        if let Some(session) = sessions.get(app_id).and_then(|weak| weak.upgrade()) {
            return session;
        }
        let session = new();
        sessions.insert(app_id.to_owned(), session.downgrade());
        session
    })
}

#[cfg(test)]
mod tests {
    use nsh_engine::Capabilities;

    use super::*;

    #[test]
    fn escapes_js_strings() {
        assert_eq!(js_string("a\"b\\c\nd\u{1}"), r#""a\"b\\c\nd\u0001""#);
        assert_eq!(
            post_message("hi #1"),
            r#"window.dispatchEvent(new MessageEvent('message', { data: "hi #1" }));"#
        );
    }

    #[test]
    fn names_folders() {
        let (data, cache) = folders("com.example.app");
        assert!(data.ends_with("com.example.app/webkit"), "{data:?}");
        assert!(cache.ends_with("com.example.app/webkit"), "{cache:?}");
        assert!(folders("a/b").0.ends_with("a_b/webkit"));
        assert!(folders("..").0.ends_with("nsh/webkit"));
        assert!(folders("").0.ends_with("nsh/webkit"));
    }

    #[test]
    fn web_views_share_a_session_that_ends_with_the_last() {
        crate::gtk_test::run(|| {
            let ephemeral = || NetworkSession::new_ephemeral();
            let first = shared_session("sessions.test", ephemeral);
            let web = WebView::builder().network_session(&first).build();
            let ended = first.downgrade();
            drop(first);
            // A second web view of the app gets the first one's session.
            let second = shared_session("sessions.test", ephemeral);
            assert_eq!(web.network_session().as_ref(), Some(&second));
            drop(second);
            drop(web);
            // Nothing holds it once its last web view is gone, so WebKit
            // ends its network process.
            assert!(ended.upgrade().is_none());
        });
    }

    #[test]
    fn decides_navigations() {
        let caps =
            |json: &str| Capabilities::from_json(&serde_json::from_str(json).unwrap()).unwrap();
        let access = Access {
            declared: caps(r#"{ "webview": { "hosts": ["example.com"] } }"#),
            granted: None,
        };
        assert!(allows(None, "https://anything.test/"));
        assert!(allows(Some(&access), "https://example.com/a"));
        assert!(!allows(Some(&access), "https://other.test/"));
        assert!(allows(Some(&access), "about:blank"));
        assert!(allows(Some(&access), "about:srcdoc"));
        assert!(allows(Some(&access), ""));
        let none = Access {
            declared: caps("{}"),
            granted: None,
        };
        assert!(!allows(Some(&none), "about:blank"));
        assert!(!allows(Some(&none), "about:srcdoc"));
    }

    #[test]
    fn reads_history_keys() {
        use gdk::ModifierType as M;
        assert_eq!(
            history_key(gdk::Key::Left, M::ALT_MASK),
            Some(History::Back)
        );
        assert_eq!(
            history_key(gdk::Key::KP_Right, M::ALT_MASK),
            Some(History::Forward)
        );
        assert_eq!(history_key(gdk::Key::Back, M::empty()), Some(History::Back));
        assert_eq!(history_key(gdk::Key::Left, M::empty()), None);
        assert_eq!(
            history_key(gdk::Key::Left, M::ALT_MASK | M::SHIFT_MASK),
            None
        );
        // Lock keys and mouse buttons held do not count.
        assert_eq!(
            history_key(gdk::Key::Left, M::ALT_MASK | M::LOCK_MASK),
            Some(History::Back)
        );
    }

    #[test]
    fn filter_identity_follows_the_rules() {
        // dogo's `__adblock` names its lists `dogo-ads-0`, `dogo-ads-1`,
        // … again for other filters: a compiled filter is only good for
        // the rules it was compiled from, not for a reused identifier.
        let list = |id: &str, rules: &str| {
            let mut list = ContentRuleList::default();
            list.id = id.into();
            list.rules = rules.into();
            list
        };
        let rules_a = list("dogo-ads-0", r#"[{"trigger":{"url-filter":"a"}}]"#);
        let rules_b = list("dogo-ads-0", r#"[{"trigger":{"url-filter":"b"}}]"#);
        let same = list("dogo-ads-0", r#"[{"trigger":{"url-filter":"a"}}]"#);
        let other = list("dogo-ads-1", r#"[{"trigger":{"url-filter":"a"}}]"#);
        assert_eq!(rule_identity(&rules_a), rule_identity(&same));
        assert_ne!(rule_identity(&rules_a), rule_identity(&rules_b));
        assert_ne!(rule_identity(&rules_a), rule_identity(&other));
        assert!(rule_identity(&rules_a).starts_with("dogo-ads-0-"));
        // WebKit names the compiled filter's file after the identifier.
        let unsafe_id = list("a/b:c d", "[]");
        let identity = rule_identity(&unsafe_id);
        assert!(identity.starts_with("a_b_c_d-"), "{identity}");
        assert!(!identity.contains('/'), "{identity}");
    }

    #[test]
    fn names_download_file_names_safely() {
        assert_eq!(file_name("report.pdf"), "report.pdf");
        // A server can suggest a path: only its last part is a name.
        assert_eq!(file_name("../../etc/passwd"), "passwd");
        assert_eq!(file_name(r"..\..\boot.ini"), "boot.ini");
        // A leading dot hides it, control characters break terminals.
        assert_eq!(file_name(".profile"), "profile");
        assert_eq!(file_name("a\u{0}b\u{7}.txt"), "ab.txt");
        assert_eq!(file_name("   "), "download");
        assert_eq!(file_name(""), "download");
        assert_eq!(file_name("/"), "download");
        // Bounded, keeping a normal extension.
        let long = format!("{}.gz", "é".repeat(400));
        let name = file_name(&long);
        assert!(name.len() <= 200, "{} bytes", name.len());
        assert!(name.ends_with(".gz"), "{name}");
        // Right-to-left overrides could disguise the extension.
        assert_eq!(file_name("invoice\u{202e}fdp.exe"), "invoicefdp.exe");
    }

    #[test]
    fn names_download_directories_without_traversal() {
        let id = download_id(0x1234_5678_9abc_def0, 42);
        assert!(!id.is_empty());
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        // Different downloads of one run never share a directory.
        assert_ne!(id, download_id(0x1234_5678_9abc_def0, 43));
        assert_ne!(id, download_id(0x1234_5678_9abc_def1, 42));
        assert_eq!(Path::new(&id).components().count(), 1);
    }

    #[test]
    fn checks_download_destinations() {
        let dir = std::env::temp_dir().join(download_id(u128::from(std::process::id()), 7));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("file.txt");
        assert!(check_destination(Path::new("relative.txt"), false).is_err());
        assert!(check_destination(Path::new(""), false).is_err());
        assert!(check_destination(&dir.join("missing").join("file"), false).is_err());
        assert!(check_destination(&target, false).is_ok());
        std::fs::write(&target, b"kept").unwrap();
        // Never replaced without saying so.
        assert!(check_destination(&target, false).is_err());
        assert!(check_destination(&target, true).is_ok());
        // A folder is not a file to write.
        assert!(check_destination(&dir, true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn publishes_staged_downloads_without_clobbering() {
        let dir = std::env::temp_dir().join(download_id(u128::from(std::process::id()), 8));
        std::fs::create_dir_all(&dir).unwrap();
        let destination = dir.join("file.txt");
        let staging = stage_path(&destination, "abc123").unwrap();
        assert_eq!(
            staging.file_name().and_then(|name| name.to_str()),
            Some(".nsh-download-abc123.part")
        );
        assert_eq!(staging.parent(), Some(dir.as_path()));
        // A fresh destination: the staged bytes become the file.
        std::fs::write(&staging, b"new").unwrap();
        assert!(publish(&staging, &destination, false).is_ok());
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert!(!staging.exists());
        // A file that appeared meanwhile is never replaced without
        // `overwrite`, and the staging file stays for cleanup.
        std::fs::write(&staging, b"newer").unwrap();
        assert!(publish(&staging, &destination, false).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert_eq!(std::fs::read(&staging).unwrap(), b"newer");
        // With it, the staged file replaces the destination.
        assert!(publish(&staging, &destination, true).is_ok());
        assert_eq!(std::fs::read(&destination).unwrap(), b"newer");
        assert!(!staging.exists());
        // A legitimate destination can itself use the usual staging name.
        let collision = dir.join(".nsh-download-collision.part");
        let separate = stage_path(&collision, "collision").unwrap();
        assert_ne!(separate, collision);
        std::fs::write(&separate, b"complete").unwrap();
        assert!(!collision.exists());
        publish(&separate, &collision, false).unwrap();
        assert_eq!(std::fs::read(&collision).unwrap(), b"complete");
        assert!(!separate.exists());
        // A staging file is never reused, and each download has its own.
        std::fs::write(&staging, b"x").unwrap();
        assert!(stage_path(&destination, "abc123").is_err());
        assert_ne!(stage_path(&destination, "def456").unwrap(), staging);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn navigation_waits_for_requested_content_rules() {
        let load =
            Command::Load(nsh_core::WebLoad::new("https://example.test/file", Vec::new()).unwrap());
        // Every app-driven navigation waits while the requested rules are
        // not installed (or failed), and none does once they are.
        for command in [Command::Reload, load, Command::GoBack, Command::GoForward] {
            assert!(is_navigation(&command));
            assert!(waits_for_rules(false, &command));
            assert!(!waits_for_rules(true, &command));
        }
        // What does not navigate (scripts, messages, stopping) runs even
        // while the rules compile, as do requests (capture, cookies).
        for command in [
            Command::InjectJavaScript("1".into()),
            Command::PostMessage("hi".into()),
            Command::StopLoading,
        ] {
            assert!(!is_navigation(&command));
            assert!(!waits_for_rules(false, &command));
        }
    }

    #[test]
    fn translates_cookie_expiries_and_same_site() {
        let date = glib::DateTime::from_unix_utc(1_700_000_000).unwrap();
        let mut cookie = soup::Cookie::new("sid", "v", "example.com", "/", -1);
        cookie.set_expires(&date);
        cookie.set_http_only(true);
        cookie.set_secure(true);
        cookie.set_same_site_policy(soup::SameSitePolicy::Strict);
        let web = web_cookie(cookie);
        // The core speaks Unix milliseconds.
        assert_eq!(web.expires, Some(1_700_000_000_000));
        assert!(web.http_only && web.secure);
        assert!(matches!(
            web.same_site.as_ref(),
            Some(CookieSameSite::Strict)
        ));
        // And WebKit takes seconds back: milliseconds are floored.
        let mut web = web;
        web.expires = Some(1_700_000_000_999);
        let mut out = soup_cookie(&web).unwrap();
        assert_eq!(
            out.expires().map(|date| date.to_unix()),
            Some(1_700_000_000)
        );
        assert!(matches!(
            out.same_site_policy(),
            soup::SameSitePolicy::Strict
        ));
        assert!(out.is_http_only() && out.is_secure());
    }
}
