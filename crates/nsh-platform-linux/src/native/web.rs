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
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use nsh_core::{
    Command, EventKind, EventPayload, JsonValue, PayloadValue, WebProps, WebSource,
    webview::{self, WebViewOperation, WebViewRequest},
};
use nsh_engine::Access;
use webkit6::{
    CookiePersistentStorage, Download, DownloadError, LoadEvent, NavigationPolicyDecision,
    NetworkError, NetworkSession, PolicyDecisionType, PolicyError, ResponsePolicyDecision,
    URIRequest, UserContentInjectedFrames, UserContentManager, UserMediaPermissionRequest,
    UserScript, UserScriptInjectionTime, UserStyleLevel, UserStyleSheet, WebView, prelude::*,
};

use super::{Emitter, UNSCROLLABLE, error, fill_sized};
use crate::{decor::rgba, surface::Surface, view::NshView};

/// The script message handler the bridge posts to.
const HANDLER: &str = "nsh";

/// Lets pages talk to the app: `window.nsh.postMessage(data)`.
const BRIDGE: &str = "window.nsh = window.nsh || { postMessage: function (data) { window.webkit.messageHandlers.nsh.postMessage(String(data)); } };";

/// `scrollable: false`: the top document does not scroll.
const NO_SCROLL: &str = "html, body { overflow: hidden !important; }";

/// How long a manual download waits for the app's destination.
const PENDING_TIMEOUT: Duration = Duration::from_secs(60);

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
    /// Where automatic downloads are saved (the app's data directory's
    /// `downloads`); `None`: nothing is downloaded.
    downloads_root: Option<PathBuf>,
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

    /// Answers a [`Command::WebViewRequest`] `id`: its value, or the
    /// error. Sent even when nothing listens: the core is waiting.
    fn reply(&self, id: u64, result: Result<JsonValue, String>) {
        if !self.live.get() {
            return;
        }
        if let Some(surface) = self.emitter.surface.upgrade() {
            surface.event(
                self.emitter.id,
                EventKind::WebViewReply,
                webview::reply(id, result),
            );
        }
    }

    /// Whether this page's downloads are wanted: an `onDownloadPending`
    /// listener (the app chooses every destination), or an `onDownload`
    /// listener with a directory to save automatic downloads in.
    fn wants_downloads(&self) -> bool {
        self.emitter.listens(EventKind::DownloadPending)
            || (self.downloads_root.is_some() && self.emitter.listens(EventKind::Download))
    }

    /// Whether this page's downloads wait for the app's destination.
    fn manual_downloads(&self) -> bool {
        self.emitter.listens(EventKind::DownloadPending)
    }
}

/// A download WebKit started for a page: reported, then either saved to a
/// directory of its own (automatic) or held without one (nothing is
/// written) until the app accepts a destination or cancels.
///
/// A manual download writes to a staging file of its own beside the app's
/// destination and is published there only once it finished; a cancel or a
/// failure removes the staging file, never the destination (which may hold
/// an older file of the app's).
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
    /// A terminal `onDownload` event was reported (at most one).
    terminal: Cell<bool>,
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
            downloads_root: surface.native.downloads.borrow().clone(),
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
            web.connect_web_process_terminated(|_, reason| {
                log::warn!(target: "js", "web view: the page's process ended ({reason:?})");
            }),
        ];
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
        // Scripts apply to pages loaded from now on: load again.
        if first || p.source != old.source || scripts_changed {
            self.load(&p.source);
        }
        self.props = Some(p.clone());
    }
}

impl WebHost {
    /// Loads `source`, once the web view is mapped. With WebKit's DMA-BUF
    /// renderer, a page loaded before its web view is mapped sometimes
    /// never shows (about one in six, on X11 and Wayland alike): it
    /// loads, but the view stays blank for good, whatever happens to it
    /// later. Loaded once mapped, as WebKit's MiniBrowser does, it always
    /// shows.
    fn load(&self, source: &WebSource) {
        if self.web.is_mapped() {
            self.pending.borrow_mut().take();
            load(&self.web, source);
        } else {
            *self.pending.borrow_mut() = Some(source.clone());
        }
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
        // manual one's staging file, goes once WebKit reports the transfer
        // stopped. The app's destinations are never touched.
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

/// Navigations stay within the app's `webview` capability; responses
/// WebKit cannot show are not downloaded.
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

/// Whether a response is a file to save: the main resource's, of a type
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

fn load_changed(web: &WebView, shared: &Rc<Shared>) -> glib::SignalHandlerId {
    let shared = shared.clone();
    web.connect_load_changed(move |web, event| match event {
        LoadEvent::Started => {
            shared.failed.set(false);
            shared.emit(EventKind::LoadStart, || navigation_state(web, true));
            shared.emit(EventKind::NavigationStateChange, || {
                navigation_state(web, true)
            });
        }
        // A failed navigation finishes too: it was reported as an error.
        LoadEvent::Finished if !shared.failed.get() => {
            shared.emit(EventKind::Load, || navigation_state(web, false));
            shared.emit(EventKind::NavigationStateChange, || {
                navigation_state(web, false)
            });
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

/// Answers a [`Command::WebViewRequest`] for `web`, whose handlers share
/// `shared`: the id comes back in the reply
/// ([`EventKind::WebViewReply`]) on the node the request arrived at.
pub(super) fn web_request(web: &WebView, shared: &Rc<Shared>, request: &WebViewRequest) {
    let id = request.id;
    match &request.operation {
        WebViewOperation::AcceptDownload {
            id: download,
            destination,
        } => {
            accept_download(
                shared,
                download,
                id,
                &destination.path,
                destination.overwrite,
            );
        }
        WebViewOperation::CancelDownload { id: download } => {
            cancel_download(shared, download, id);
        }
        // A newer core's operation: answered, never left waiting.
        _ => {
            let _ = web;
            shared.reply(id, Err("unsupported web view operation".into()));
        }
    }
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
        terminal: Cell::new(false),
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
                // Cleanup happened above, before the acknowledgement and
                // the lifecycle event. A started download has a terminal
                // `failed` event even when the platform calls it a user
                // cancellation; a pending one never started.
                if take_terminal(state.started.get(), &state.terminal) {
                    let failed = failed_event(&state, err.message().to_owned());
                    shared.emit(EventKind::Download, || failed);
                }
                shared.reply(request, Ok(JsonValue::Null));
                return;
            }
            if cancelled {
                // A cancellation with no request (teardown): a started
                // download still reaches its terminal state exactly once.
                if take_terminal(state.started.get(), &state.terminal) {
                    let failed = failed_event(&state, err.message().to_owned());
                    shared.emit(EventKind::Download, || failed);
                }
                return;
            }
            log::warn!("web view: download failed: {err}");
            if !state.terminal.replace(true) {
                let failed = failed_event(&state, err.message().to_owned());
                shared.emit(EventKind::Download, || failed);
            }
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
                // it wrote is discarded, nothing is published, and a
                // started download reports `failed` rather than `finished`.
                cleanup(&shared, &state);
                if let Some(accepting) = state.accepting.take() {
                    shared.reply(accepting, Err("download cancelled".into()));
                }
                if take_terminal(state.started.get(), &state.terminal) {
                    let failed = failed_event(&state, "download cancelled".into());
                    shared.emit(EventKind::Download, || failed);
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
                    if take_terminal(state.started.get(), &state.terminal) {
                        let failed = failed_event(&state, error);
                        shared.emit(EventKind::Download, || failed);
                    }
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
            state.terminal.set(true);
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

/// WebKit asks where a download goes: an automatic one is saved in the
/// app's data directory, a manual one reports the choice and holds it.
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
    if shared.manual_downloads() {
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
        return;
    }
    let Some(root) = shared.downloads_root.clone() else {
        // No directory to save in (the app started without one).
        state.finished.set(true);
        state.disconnect();
        shared.downloads.borrow_mut().remove(&state.id);
        download.cancel();
        return;
    };
    // A directory of its own: two files of the same name never meet, and a
    // failed one takes its partial file with it.
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
    // The answer waits for WebKit: `created-destination` (ok) or `failed`
    // (its error). `set_destination` may call back at once.
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
    cleanup_output(state.manual.get(), staging.as_deref(), dir.as_deref());
}

/// Removes what the download owns: its staging file, or the automatic
/// directory this backend made. Never the app's destination.
fn cleanup_output(manual: bool, staging: Option<&Path>, automatic_dir: Option<&Path>) {
    if manual {
        if let Some(staging) = staging {
            let _ = std::fs::remove_file(staging);
        }
    } else if let Some(dir) = automatic_dir {
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

/// Takes a download's one terminal event. A pending download never
/// started, so a cancellation before acceptance reports no lifecycle; a
/// started one reports `failed` once, never a later `finished`.
fn take_terminal(started: bool, terminal: &Cell<bool>) -> bool {
    started && !terminal.replace(true)
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
    fn download_scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsh-webview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        dir
    }

    #[test]
    fn names_download_file_names_safely() {
        assert_eq!(file_name("report.pdf"), "report.pdf");
        assert_eq!(file_name("../../etc/passwd"), "passwd");
        assert_eq!(file_name(r"..\..\boot.ini"), "boot.ini");
        assert_eq!(file_name(".profile"), "profile");
        assert_eq!(file_name("a\u{0}b\u{7}.txt"), "ab.txt");
        assert_eq!(file_name("   "), "download");
        assert_eq!(file_name(""), "download");
        assert_eq!(file_name("/"), "download");
        assert_eq!(file_name("报告.pdf"), "报告.pdf");
        let long = format!("{}.gz", "é".repeat(400));
        let name = file_name(&long);
        assert!(name.len() <= 200, "{} bytes", name.len());
        assert!(name.ends_with(".gz"), "{name}");
        assert_eq!(file_name("invoice\u{202e}fdp.exe"), "invoicefdp.exe");
    }

    #[test]
    fn names_download_directories_without_traversal() {
        let id = download_id(0x1234_5678_9abc_def0, 42);
        assert!(!id.is_empty());
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(id, download_id(0x1234_5678_9abc_def0, 43));
        assert_ne!(id, download_id(0x1234_5678_9abc_def1, 42));
        assert_eq!(Path::new(&id).components().count(), 1);
    }

    #[test]
    fn checks_download_destinations() {
        let dir = download_scratch("destinations");
        let target = dir.join("file.txt");
        assert!(check_destination(Path::new("relative.txt"), false).is_err());
        assert!(check_destination(Path::new(""), false).is_err());
        assert!(check_destination(&dir.join("missing").join("file"), false).is_err());
        assert!(check_destination(&target, false).is_ok());
        std::fs::write(&target, b"kept").unwrap();
        assert!(check_destination(&target, false).is_err());
        assert!(check_destination(&target, true).is_ok());
        assert!(check_destination(&dir, true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn publishes_staged_downloads_without_clobbering() {
        let dir = download_scratch("publish");
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
        // Only the staging file belongs to the download.
        cleanup_output(true, Some(&staging), None);
        assert!(!staging.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        // With overwrite, the staged file replaces the destination.
        std::fs::write(&staging, b"newer").unwrap();
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
    fn cleanup_removes_owned_output_only() {
        let dir = download_scratch("cleanup");
        let keep = dir.join("file.txt");
        std::fs::write(&keep, b"completed").unwrap();
        // A manual download owns its staging file.
        let staging = dir.join(".nsh-download-id.part");
        std::fs::write(&staging, b"partial").unwrap();
        cleanup_output(true, Some(&staging), Some(&dir.join("unused")));
        assert!(!staging.exists());
        assert_eq!(std::fs::read(&keep).unwrap(), b"completed");
        // An automatic download owns its directory of its own.
        let owned = dir.join("id");
        std::fs::create_dir_all(&owned).unwrap();
        std::fs::write(owned.join("file.txt"), b"partial").unwrap();
        cleanup_output(false, Some(&owned.join("file.txt")), Some(&owned));
        assert!(!owned.exists());
        assert_eq!(std::fs::read(&keep).unwrap(), b"completed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancellation_reports_one_failed_terminal_only_after_start() {
        let terminal = Cell::new(false);
        // A pending download never started: cancelling it has no lifecycle.
        assert!(!take_terminal(false, &terminal));
        assert!(!take_terminal(false, &terminal));
        assert!(!terminal.get());
        // A started download fails once; no later event is reported.
        assert!(take_terminal(true, &terminal));
        assert!(!take_terminal(true, &terminal));
    }
}
