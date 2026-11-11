//! Web views: WebView2 composition controllers in the window's
//! composition.
//!
//! Each web view is a WebView2 composition controller ("visual hosting")
//! drawing into a visual of its window's [`Composition`] ([`Portal`]): the
//! main window's, or that of the window of the app's own its view is in
//! ([`crate::app_window::compose`]), bound when its view is first in a
//! window:
//! the frame records where the view goes as it draws the tree, the visual
//! is clipped, transformed and faded like the view, and what nsh draws
//! after it goes on a layer above it, so dialogs, sheets, toasts, the dev
//! overlay and later siblings cover the page as they would any view. The
//! window gets the input: nsh hit-tests it as for any view, and what falls
//! on the page (nothing of nsh's is over it there) is forwarded to it as
//! it comes ([`Hosted::raw`]): mouse buttons, moves and wheels through
//! `SendMouseInput`, touch and pen contacts through `SendPointerInput`. The
//! page's cursor comes back through `CursorChanged`. A click on the page
//! gives it the keyboard focus (`MoveFocus`: WebView2 then takes keys and
//! the IME itself), which blurs nsh's text fields; a click on nsh content
//! gives it back to the window, and Tab out of the page
//! (`MoveFocusRequested`) too. The page is rasterized at the window's
//! scale (`RasterizationScale`); its bounds are where its view is in the
//! window's pixels, which WebView2 places pop-ups (select menus) and the
//! IME's candidates from.
//!
//! WebView2 is asynchronous: the environment (one per thread and profile
//! folder, `%LOCALAPPDATA%\<app id>\WebView2`) and then the controller are
//! made in the background, from the first props (`incognito` makes an
//! InPrivate controller, whose website data stays in memory). Props and
//! commands that come before the controller is ready wait for it
//! ([`WebItem::ready`]). With MSVC the WebView2 loader is linked
//! statically (`WebView2LoaderStatic.lib`), so apps do not ship
//! `WebView2Loader.dll` (`create_environment`); the WebView2 runtime itself comes with Windows 11 (and Edge's installer
//! on Windows 10). Without it, a web view draws a neutral placeholder, as
//! on Linux.
//!
//! Pages talk to the app through `window.nsh`: the bridge script
//! defines `window.nsh.postMessage(data)` (`onMessage`), and
//! `postMessage()` dispatches a `message` event on the page's `window`.
//! Top-level navigations stay within the app's `webview` capability.
//! Events and their payloads are the iOS ones: `onLoadStart` and
//! `onNavigationStateChange` when a navigation starts, `onLoad` and
//! `onNavigationStateChange` when it completes, `onError` when it fails.
//!
//! - `source.html` with a `baseUrl` navigates to the base URL and answers
//!   that document's request with the HTML (`WebResourceRequested`): the
//!   page's URL and origin are the base URL's and relative URLs resolve
//!   against it, as with `loadHTMLString:baseURL:`. Without a base URL the
//!   HTML is loaded with `NavigateToString`.
//! - `scrollable={false}`: the wheel goes to the scroll views around the
//!   web view instead of the page, and a style keeps the page's viewport
//!   from scrolling (`overflow: hidden` on its root element) while scripts
//!   still may, as a `UIScrollView` with scrolling disabled.
//! - `desktop={{ allowsBackForwardNavigationGestures }}`: WebView2's swipe navigation
//!   (`IsSwipeNavigationEnabled`, touch swipes).
//!
//! # Downloads
//!
//! An `onDownload` listener makes Automatic downloads save into the app's
//! data directory, `downloads/<id>/<sanitized filename>`, and reports
//! [`EventKind::Download`], as macOS does. Without a listener WebView2's own
//! download UI is left alone. An `onDownloadPending` listener instead asks
//! for Manual downloads: `DownloadStarting` holds the download open with a
//! deferral and reports [`EventKind::DownloadPending`] (`id`, `url`,
//! `filename`); nothing is written until the app accepts a destination
//! (`AcceptDownload`) or cancels (`CancelDownload`). The destination is
//! written through an owned sibling staging file and published only when
//! the transfer finishes (replacing an existing file only when the app
//! asked). A pending download the app never answers is cancelled after
//! [`PENDING_DOWNLOAD_TIMEOUT`]. A WebView2 runtime that cannot report
//! downloads is not allowed to fall back to its own UI when the app asked
//! to choose destinations.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nsh_core::{
    Color, Command, EventKind, EventPayload, JsonValue, NodeId, PayloadValue, ROOT_ID, WebProps,
    WebSource,
    access::WebMedia,
    webview::{self, WebViewOperation, WebViewRequest},
};
use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    CreateCoreWebView2CompositionControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, CursorChangedEventHandler,
    DownloadStartingEventHandler, FocusChangedEventHandler,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_COLOR, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON, COREWEBVIEW2_DOWNLOAD_STATE,
        COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED, COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS,
        COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED, COREWEBVIEW2_MOUSE_EVENT_KIND,
        COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL, COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE,
        COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOUBLE_CLICK,
        COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
        COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP,
        COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOUBLE_CLICK,
        COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOWN,
        COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_UP, COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE,
        COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOUBLE_CLICK,
        COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN,
        COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP, COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL,
        COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_DOUBLE_CLICK,
        COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_DOWN, COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_UP,
        COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS, COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC,
        COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
        COREWEBVIEW2_PERMISSION_STATE_DENY, COREWEBVIEW2_POINTER_EVENT_KIND,
        COREWEBVIEW2_WEB_ERROR_STATUS, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT, ICoreWebView2,
        ICoreWebView2_2, ICoreWebView2_4, ICoreWebView2CompositionController,
        ICoreWebView2Controller, ICoreWebView2Controller2, ICoreWebView2Controller3,
        ICoreWebView2Deferral, ICoreWebView2DownloadOperation,
        ICoreWebView2DownloadStartingEventArgs, ICoreWebView2Environment,
        ICoreWebView2Environment2, ICoreWebView2Environment3, ICoreWebView2Environment10,
        ICoreWebView2ExecuteScriptCompletedHandler, ICoreWebView2Settings2, ICoreWebView2Settings6,
    },
    MoveFocusRequestedEventHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, PermissionRequestedEventHandler, StateChangedEventHandler,
    WebMessageReceivedEventHandler, WebResourceRequestedEventHandler, take_pwstr,
};
use windows::{
    Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::{
            Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
            DirectComposition::IDCompositionVisual2,
            DirectWrite::{
                DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_WORD_WRAPPING_WRAP, IDWriteTextFormat,
            },
            Gdi::ScreenToClient,
        },
        System::Com::CoCreateGuid,
        UI::{
            Input::{
                KeyboardAndMouse::{GetFocus, SetFocus},
                Pointer::{
                    GetPointerDeviceRects, GetPointerInfo, GetPointerPenInfo, GetPointerTouchInfo,
                    POINTER_INFO, POINTER_PEN_INFO, POINTER_TOUCH_INFO,
                },
            },
            Shell::SHCreateMemStream,
            WindowsAndMessaging::{HCURSOR, IsChild, SetCursor},
        },
    },
    core::{BOOL, HRESULT, HSTRING, Interface, PCWSTR, PWSTR, w},
};

use super::{Emitter, error};
use crate::{
    compose::{Placement, Portal},
    gfx,
    input::{Modifiers, PointerKind, WheelEvent},
    render::{DrawCx, rect_f},
    surface::Surface,
    view::{Hosted, ItemPointer, NshView},
};

/// How long a manual download waits for the app to accept or cancel it
/// before it is cancelled and reported failed (nothing is written).
const PENDING_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// Lets pages talk to the app: `window.nsh.postMessage(data)`.
const BRIDGE: &str = "window.nsh = window.nsh || { postMessage: function (data) { window.chrome.webview.postMessage(String(data)); } };";

// `CreateCoreWebView2EnvironmentWithOptions`.
//
// With MSVC the loader is linked statically (`WebView2LoaderStatic.lib`,
// which webview2-com-sys ships and puts on the link path), so apps do not
// ship `WebView2Loader.dll`. Other toolchains (`windows-gnu`) cannot link
// that MSVC library: there the loader DLL is loaded when the first web view
// starts, so an app that never shows one needs neither, and a missing DLL
// draws the placeholder as a missing runtime does.
#[cfg(target_env = "msvc")]
#[link(name = "WebView2LoaderStatic", kind = "static")]
unsafe extern "system" {
    #[link_name = "CreateCoreWebView2EnvironmentWithOptions"]
    fn create_environment(
        browser_folder: PCWSTR,
        user_data_folder: PCWSTR,
        options: *mut std::ffi::c_void,
        handler: *mut std::ffi::c_void,
    ) -> HRESULT;
}

#[cfg(not(target_env = "msvc"))]
unsafe fn create_environment(
    browser_folder: PCWSTR,
    user_data_folder: PCWSTR,
    options: *mut std::ffi::c_void,
    handler: *mut std::ffi::c_void,
) -> HRESULT {
    use windows::{
        Win32::{
            Foundation::E_NOINTERFACE,
            System::LibraryLoader::{GetProcAddress, LoadLibraryW},
        },
        core::{s, w},
    };
    type Create = unsafe extern "system" fn(
        PCWSTR,
        PCWSTR,
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
    ) -> HRESULT;
    // SAFETY: loading a DLL by name and reading an export of the known
    // signature; the module stays loaded for the process.
    unsafe {
        let Ok(module) = LoadLibraryW(w!("WebView2Loader.dll")) else {
            return E_NOINTERFACE;
        };
        let Some(create) = GetProcAddress(module, s!("CreateCoreWebView2EnvironmentWithOptions"))
        else {
            return E_NOINTERFACE;
        };
        let create: Create = std::mem::transmute(create);
        create(browser_folder, user_data_folder, options, handler)
    }
}

/// A download WebView2 started and the app must decide on: its event stays
/// open through a deferral until the app accepts or cancels, and nothing is
/// written until then.
struct PendingDownload {
    args: ICoreWebView2DownloadStartingEventArgs,
    deferral: ICoreWebView2Deferral,
    url: String,
    filename: String,
}

impl PendingDownload {
    /// Cancels the download and answers its event (nothing is written).
    fn cancel(&self) {
        // SAFETY: WebView2 calls on the live event and its deferral; the
        // deferral is completed once, which ends the event.
        unsafe {
            let _ = self.args.SetCancel(true);
            let _ = self.deferral.Complete();
        }
    }

    /// The operation it downloads, while its event is still open.
    fn operation(&self) -> Option<ICoreWebView2DownloadOperation> {
        // SAFETY: a WebView2 getter on the live event.
        unsafe { self.args.DownloadOperation() }.ok()
    }
}

/// A download under way (Automatic, or a Manual one the app accepted):
/// what it reports and what to remove if it does not complete.
struct ActiveDownload {
    operation: ICoreWebView2DownloadOperation,
    id: String,
    url: String,
    filename: String,
    /// The file the app hears about.
    path: PathBuf,
    /// Where WebView2 writes a Manual download until it completes (a
    /// unique sibling of `path`, so a failure never touches an existing
    /// file there).
    staging: Option<PathBuf>,
    /// A Manual download replaces an existing `path` only when the app
    /// asked for it.
    overwrite: bool,
    /// The directory this backend made for an automatic download (only
    /// that is removed whole; an app-chosen destination is not).
    owned_directory: Option<PathBuf>,
    /// The request id waiting for this transfer to stop before its
    /// cancellation is answered.
    cancel_request: Cell<Option<u64>>,
    /// The one terminal lifecycle event was reported.
    terminal: Cell<bool>,
}

impl ActiveDownload {
    /// Its state, when WebView2 reports one.
    fn state(&self) -> Option<COREWEBVIEW2_DOWNLOAD_STATE> {
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS;
        // SAFETY: a WebView2 getter on the live operation.
        unsafe { self.operation.State(&mut state) }
            .ok()
            .map(|()| state)
    }

    /// Stops the transfer (teardown) and removes its partial output. A
    /// transfer that already completed is published instead, so its file
    /// is kept. The app's destination is never deleted.
    fn cancel(&self) {
        if self.state() == Some(COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED) {
            if self.publish().is_err() {
                self.cleanup();
            }
            return;
        }
        // SAFETY: a WebView2 call on the live operation.
        let _ = unsafe { self.operation.Cancel() };
        self.cleanup();
    }

    /// Moves a completed Manual download from its staging file to the
    /// app's destination: without replacement when the app did not ask for
    /// it, atomically replacing otherwise. The old destination is never
    /// removed before the new file is ready.
    fn publish(&self) -> Result<(), String> {
        let Some(staging) = &self.staging else {
            return Ok(()); // an automatic download wrote its final path
        };
        publish_staged(staging, &self.path, self.overwrite)
    }

    /// Removes the partial output this backend wrote, never the app's
    /// existing destination.
    fn cleanup(&self) {
        remove_owned(self.owned_directory.as_ref(), self.staging.as_ref(), false);
    }

    /// Removes the partial output after WebView2 reported that writing
    /// stopped: synchronously when possible (the cancellation answer
    /// follows the deletion), with a retry only for the owned staging file.
    fn cleanup_after_stop(&self) {
        remove_owned(self.owned_directory.as_ref(), self.staging.as_ref(), true);
    }
}

/// A web view: its visuals and, once made, its controller.
pub(crate) struct WebItem {
    emitter: Emitter,
    surface: Weak<Surface>,
    /// A weak self, for event handlers that outlive the call that made
    /// them.
    me: RefCell<Weak<WebItem>>,
    /// Its view.
    id: NodeId,
    /// The window it is in (`ROOT_ID`: the main one) and its native window,
    /// once its view is in one: its page is composed and parented there.
    host: Cell<Option<(NodeId, HWND)>>,
    /// The visuals the page is composed in (`None` once closed, or when the
    /// window has no composition).
    portal: RefCell<Option<Portal>>,
    env: RefCell<Option<ICoreWebView2Environment>>,
    composition: RefCell<Option<ICoreWebView2CompositionController>>,
    controller: RefCell<Option<ICoreWebView2Controller>>,
    webview: RefCell<Option<ICoreWebView2>>,
    /// No web view can be made (no WebView2 runtime): a placeholder.
    failed: Cell<bool>,
    /// Its environment was asked for (with the first props).
    started: Cell<bool>,
    /// Closed (its view was deleted and is not drawn anymore, or the app
    /// stopped).
    closed: Cell<bool>,
    /// Its view was deleted: it closes once the view is not drawn anymore
    /// (a removal animation keeps it on screen a little longer).
    retired: Cell<bool>,
    /// The props asked for (applied once the controller is ready).
    props: RefCell<Option<WebProps>>,
    /// The props applied to the web view.
    applied: RefCell<Option<WebProps>>,
    /// Commands that came before the controller.
    queued: RefCell<Vec<Command>>,
    /// The ids of the app's document scripts (removed when they change).
    scripts: Rc<RefCell<Vec<String>>>,
    /// The id of the document script that keeps the page from scrolling.
    scroll_script: Rc<RefCell<Option<String>>>,
    /// The engine's own user agent (restored when the app's is removed).
    default_agent: RefCell<Option<String>>,
    /// The HTML served at its base URL (`baseUrl`).
    html: RefCell<Option<(String, Arc<str>)>>,
    /// The document requests are watched (for `html`).
    serving: Cell<bool>,
    /// Where the last frame put it.
    at: Cell<Option<Placement>>,
    /// The controller's bounds applied (window pixels; `None`: hidden).
    bounds: Cell<Option<RECT>>,
    /// The rasterization scale applied.
    raster: Cell<f32>,
    /// The view's background, the page's until it paints.
    background: Cell<Color>,
    /// The page's cursor (an `HCURSOR`).
    cursor: Cell<isize>,
    /// The mouse is over the page.
    hovered: Cell<bool>,
    /// The page has the keyboard focus.
    focused: Cell<bool>,
    /// Mouse buttons held on the page (bits by button).
    buttons: Cell<u8>,
    /// Where automatic downloads are saved (the app's data directory's
    /// `downloads`); `None` without one.
    downloads_root: Option<PathBuf>,
    /// Downloads waiting for the app's answer, by their id (their event
    /// stays open through a deferral until it does).
    downloads: RefCell<HashMap<String, PendingDownload>>,
    /// Downloads under way (Automatic, or accepted Manual), by their id.
    active_downloads: RefCell<HashMap<String, ActiveDownload>>,
    /// This runtime reports downloads (`DownloadStarting`).
    downloads_supported: Cell<bool>,
}

impl WebItem {
    pub(crate) fn new(surface: &Rc<Surface>, view: &NshView) -> Rc<Self> {
        Rc::new(WebItem {
            emitter: Emitter::new(surface, view.id()),
            surface: Rc::downgrade(surface),
            me: RefCell::default(),
            id: view.id(),
            host: Cell::new(None),
            portal: RefCell::new(None),
            env: RefCell::new(None),
            composition: RefCell::new(None),
            controller: RefCell::new(None),
            webview: RefCell::new(None),
            failed: Cell::new(false),
            started: Cell::new(false),
            closed: Cell::new(false),
            retired: Cell::new(false),
            props: RefCell::new(None),
            applied: RefCell::new(None),
            queued: RefCell::new(Vec::new()),
            scripts: Rc::default(),
            scroll_script: Rc::default(),
            default_agent: RefCell::new(None),
            html: RefCell::new(None),
            serving: Cell::new(false),
            at: Cell::new(None),
            bounds: Cell::new(None),
            raster: Cell::new(0.0),
            background: Cell::new(view.state().style.background_color),
            cursor: Cell::new(0),
            hovered: Cell::new(false),
            focused: Cell::new(false),
            buttons: Cell::new(0),
            downloads_root: surface.native.downloads(),
            downloads: RefCell::default(),
            active_downloads: RefCell::default(),
            downloads_supported: Cell::new(false),
        });
        *item.me.borrow_mut() = Rc::downgrade(&item);
        item
    }

    /// The native window its page is in, once bound.
    pub(crate) fn hwnd(&self) -> Option<HWND> {
        self.host.get().map(|(_, hwnd)| hwnd)
    }

    /// Binds it to the window its view is in: its page's visuals go into
    /// that window's composition. False while its view is in no window
    /// (or when the window cannot compose it: the placeholder shows).
    fn bind(&self, surface: &Surface) -> bool {
        if self.host.get().is_some() {
            return true;
        }
        let Some(view) = surface.view(self.id).filter(NshView::attached) else {
            return false;
        };
        let window = crate::app_window::window_of(surface, &view);
        let made = if window == ROOT_ID {
            surface.compose().map(|c| (surface.hwnd(), c))
        } else {
            crate::app_window::compose(surface, window)
        };
        match made.and_then(|(hwnd, c)| Ok((hwnd, Portal::new(c)?))) {
            Ok((hwnd, portal)) => {
                *self.portal.borrow_mut() = Some(portal);
                self.host.set(Some((window, hwnd)));
                true
            }
            Err(err) => {
                log::warn!("web view: no composition: {err}");
                self.unavailable("the window cannot compose it");
                false
            }
        }
    }

    /// Its view changed how it draws: the window it is in draws again.
    fn invalidate(&self) {
        let Some(surface) = self.surface.upgrade() else {
            return;
        };
        match surface.view(self.id) {
            Some(view) => view.changed(),
            None => surface.invalidate(),
        }
    }

    /// Asks for the environment, once the first props say how to make the
    /// controller (`incognito`) and its view is in a window.
    fn start(self: &Rc<Self>, surface: &Surface) {
        if self.started.get() || self.failed.get() || self.closed.get() || !self.bind(surface) {
            return;
        }
        self.started.set(true);
        let folder = profile_folder(&surface.native.app_id.borrow());
        let weak = Rc::downgrade(self);
        with_environment(&folder, move |env| {
            if let Some(item) = weak.upgrade() {
                item.environment_ready(env);
            }
        });
    }

    /// No web view: the placeholder shows, and the app hears why
    /// (`onError`, `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`) to show what it
    /// has instead.
    fn unavailable(&self, why: &str) {
        self.failed.set(true);
        self.portal.borrow_mut().take();
        static WARNED: AtomicBool = AtomicBool::new(false);
        if !WARNED.swap(true, Ordering::Relaxed) {
            log::warn!(
                target: "js",
                "WebView is not available ({why}; is the WebView2 runtime installed?): it shows a placeholder"
            );
        }
        self.emitter.emit(EventKind::Error, || {
            EventPayload::Map(vec![
                (
                    "message",
                    PayloadValue::Str(format!("WebView is not available: {why}")),
                ),
                (
                    "code",
                    PayloadValue::Str(nsh_core::element::codes::UNAVAILABLE.into()),
                ),
            ])
        });
        self.invalidate();
    }

    fn environment_ready(self: &Rc<Self>, env: Option<ICoreWebView2Environment>) {
        if self.closed.get() {
            return;
        }
        let Some(env) = env else {
            return self.unavailable("no WebView2 environment");
        };
        let (Ok(env3), Some(parent)) = (env.cast::<ICoreWebView2Environment3>(), self.hwnd())
        else {
            return self.unavailable("the WebView2 runtime cannot host it in a visual");
        };
        let incognito = self.props.borrow().as_ref().is_some_and(|p| p.incognito);
        // InPrivate needs the controller options of WebView2 101 or later;
        // an older runtime would keep the page's data on disk.
        let private = match incognito.then(|| env.cast::<ICoreWebView2Environment10>()) {
            None => None,
            Some(Ok(env10)) => Some(env10),
            Some(Err(_)) => {
                return self.unavailable(
                    "the WebView2 runtime cannot make an incognito web view; version 101 or later can",
                );
            }
        };
        *self.env.borrow_mut() = Some(env);
        let weak = Rc::downgrade(self);
        let handler = CreateCoreWebView2CompositionControllerCompletedHandler::create(Box::new(
            move |result, controller| {
                let Some(item) = weak.upgrade().filter(|i| !i.closed.get()) else {
                    // The view went away meanwhile.
                    if let Some(c) =
                        controller.and_then(|c| c.cast::<ICoreWebView2Controller>().ok())
                    {
                        // SAFETY: a call on the controller just made.
                        let _ = unsafe { c.Close() };
                    }
                    return Ok(());
                };
                match (result, controller) {
                    (Ok(()), Some(controller)) => item.ready(controller),
                    (result, _) => {
                        log::warn!("web view: no controller: {result:?}");
                        item.unavailable("cannot create a WebView2 controller");
                    }
                }
                Ok(())
            },
        ));
        // SAFETY: calls on a live environment with the live window of this
        // thread.
        let made = unsafe {
            match &private {
                Some(env10) => env10
                    .CreateCoreWebView2ControllerOptions()
                    .and_then(|options| {
                        options.SetIsInPrivateModeEnabled(true)?;
                        env10.CreateCoreWebView2CompositionControllerWithOptions(
                            parent, &options, &handler,
                        )
                    }),
                None => env3.CreateCoreWebView2CompositionController(parent, &handler),
            }
        };
        if let Err(err) = made {
            log::warn!("web view: cannot create a controller: {err}");
            self.unavailable("cannot create a WebView2 controller");
        }
    }

    /// The controller is made: sets it up, applies what waited for it.
    fn ready(self: &Rc<Self>, composition: ICoreWebView2CompositionController) {
        let target = self.portal.borrow().as_ref().map(|p| p.target().clone());
        let (Some(target), Ok(controller)) =
            (target, composition.cast::<ICoreWebView2Controller>())
        else {
            return self.unavailable("no WebView2 controller");
        };
        // SAFETY: WebView2 calls on the live controller, on its thread.
        let webview = match unsafe { controller.CoreWebView2() } {
            Ok(webview) => webview,
            Err(err) => {
                log::warn!("web view: {err}");
                return self.unavailable("no WebView2 core");
            }
        };
        // SAFETY: as above; the visual belongs to this thread's
        // composition.
        unsafe {
            if let Err(err) = composition.SetRootVisualTarget(&target) {
                log::warn!("web view: cannot compose it: {err}");
                let _ = controller.Close();
                return self.unavailable("cannot compose a WebView2 controller");
            }
            if let Ok(c) = controller.cast::<ICoreWebView2Controller3>() {
                // The window's scale, not the monitor's: nsh decides.
                let _ = c.SetShouldDetectMonitorScaleChanges(false);
            }
            let _ = controller.SetIsVisible(false);
            if let Ok(settings) = webview.Settings() {
                let _ = settings.SetIsStatusBarEnabled(false);
                if let Ok(settings) = settings.cast::<ICoreWebView2Settings2>() {
                    let mut agent = PWSTR::null();
                    if settings.UserAgent(&mut agent).is_ok() {
                        *self.default_agent.borrow_mut() = Some(take_pwstr(agent));
                    }
                }
            }
            let _ = webview.AddScriptToExecuteOnDocumentCreated(
                &HSTRING::from(BRIDGE),
                None::<&webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler>,
            );
        }
        self.listen_downloads(&webview);
        self.listen(&webview);
        self.listen_input(&composition, &controller);
        *self.composition.borrow_mut() = Some(composition);
        *self.controller.borrow_mut() = Some(controller);
        *self.webview.borrow_mut() = Some(webview);
        self.paint_background();
        self.bounds.set(None);
        self.place();
        let props = self.props.borrow().clone();
        if let Some(props) = props {
            self.apply(&props);
        }
        let queued = std::mem::take(&mut *self.queued.borrow_mut());
        for command in &queued {
            self.command(command);
        }
    }

    /// Reports the web view's navigations and messages.
    fn listen(self: &Rc<Self>, webview: &ICoreWebView2) {
        let mut token = 0i64;
        let weak = Rc::downgrade(self);
        let starting = NavigationStartingEventHandler::create(Box::new(move |sender, args| {
            let (Some(item), Some(sender), Some(args)) = (weak.upgrade(), sender, args) else {
                return Ok(());
            };
            let mut uri = PWSTR::null();
            // SAFETY: WebView2 calls on the event's live objects.
            let url = unsafe { args.Uri(&mut uri).map(|()| take_pwstr(uri)) }.unwrap_or_default();
            if item.manual_downloads_unavailable() {
                // SAFETY: a WebView2 call on the live event.
                let _ = unsafe { args.SetCancel(true) };
                item.emitter.emit(EventKind::Error, || {
                    error(
                        "manual downloads need a WebView2 runtime that reports downloads"
                            .to_owned(),
                    )
                });
                return Ok(());
            }
            if !item.allows(&url) {
                // SAFETY: as above.
                let _ = unsafe { args.SetCancel(true) };
                log::warn!(target: "js", "web view navigation to {url} is not granted");
                let message = format!("navigation to {url} is not granted");
                item.emitter.emit(EventKind::Error, || {
                    EventPayload::Map(vec![
                        ("message", PayloadValue::Str(message)),
                        (
                            "code",
                            PayloadValue::Str(nsh_core::element::codes::ACCESS_DENIED.into()),
                        ),
                        ("url", PayloadValue::Str(url.clone())),
                    ])
                });
                return Ok(());
            }
            let mut state = navigation(&sender, true);
            state.url = url;
            item.emitter.emit(EventKind::LoadStart, || state.payload());
            item.emitter
                .emit(EventKind::NavigationStateChange, || state.payload());
            Ok(())
        }));
        let weak = Rc::downgrade(self);
        let completed = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
            let (Some(item), Some(sender), Some(args)) = (weak.upgrade(), sender, args) else {
                return Ok(());
            };
            let mut success = BOOL(0);
            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS(0);
            // SAFETY: WebView2 calls on the event's live objects.
            unsafe {
                let _ = args.IsSuccess(&mut success);
                let _ = args.WebErrorStatus(&mut status);
            }
            if success.as_bool() {
                let state = navigation(&sender, false);
                item.emitter.emit(EventKind::Load, || state.payload());
                item.emitter
                    .emit(EventKind::NavigationStateChange, || state.payload());
            } else if let Some(message) = web_error(status.0) {
                item.emitter
                    .emit(EventKind::Error, || error(message.to_owned()));
            }
            Ok(())
        }));
        let weak = Rc::downgrade(self);
        let message = WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            let (Some(item), Some(args)) = (weak.upgrade(), args) else {
                return Ok(());
            };
            let mut text = PWSTR::null();
            // SAFETY: WebView2 calls on the event's live object; a message
            // that is not a string comes as JSON.
            let data = unsafe {
                match args.TryGetWebMessageAsString(&mut text) {
                    Ok(()) => take_pwstr(text),
                    Err(_) => {
                        let mut json = PWSTR::null();
                        args.WebMessageAsJson(&mut json)
                            .map(|()| take_pwstr(json))
                            .unwrap_or_default()
                    }
                }
            };
            item.emitter.emit(EventKind::Message, || {
                EventPayload::Map(vec![("data", PayloadValue::Str(data))])
            });
            Ok(())
        }));
        let weak = Rc::downgrade(self);
        let permission = PermissionRequestedEventHandler::create(Box::new(move |_, args| {
            let (Some(item), Some(args)) = (weak.upgrade(), args) else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            // SAFETY: WebView2 calls on the event's live object.
            unsafe { args.PermissionKind(&mut kind)? };
            let media = match kind {
                COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => WebMedia::Microphone,
                COREWEBVIEW2_PERMISSION_KIND_CAMERA => WebMedia::Camera,
                // Other permissions keep WebView2's own handling.
                _ => return Ok(()),
            };
            let state = if item.allows_media(media) {
                COREWEBVIEW2_PERMISSION_STATE_ALLOW
            } else {
                log::warn!(target: "js", "web view {} capture is not granted: add it to capabilities.webview.media", media.as_str());
                COREWEBVIEW2_PERMISSION_STATE_DENY
            };
            // SAFETY: as above.
            unsafe { args.SetState(state) }
        }));
        // SAFETY: WebView2 calls on the live web view; the handlers hold the
        // item weakly, and go with the web view.
        unsafe {
            let _ = webview.add_PermissionRequested(&permission, &mut token);
            let _ = webview.add_NavigationStarting(&starting, &mut token);
            let _ = webview.add_NavigationCompleted(&completed, &mut token);
            let _ = webview.add_WebMessageReceived(&message, &mut token);
        }
    }

    /// Follows the page's cursor and keyboard focus.
    /// Whether the app asked to choose every download's destination but
    /// this runtime cannot report downloads: its page must not navigate,
    /// so WebView2's own download UI cannot run behind the app's back.
    fn manual_downloads_unavailable(&self) -> bool {
        self.emitter.listens(EventKind::DownloadPending) && !self.downloads_supported.get()
    }

    /// `DownloadStarting`: saves into the app's data directory when the app
    /// listens for downloads, holds a Manual download for the app's
    /// decision, or leaves WebView2's own UI alone.
    fn listen_downloads(self: &Rc<Self>, webview: &ICoreWebView2) {
        let Ok(webview) = webview.cast::<ICoreWebView2_4>() else {
            log::debug!("web view: this WebView2 runtime does not report downloads");
            return;
        };
        let weak = Rc::downgrade(self);
        let started = DownloadStartingEventHandler::create(Box::new(move |_, args| {
            if let (Some(item), Some(args)) = (weak.upgrade(), args) {
                item.download_started(args);
            }
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: WebView2 calls on the live web view; the handler holds
        // the item weakly and goes with the web view.
        let added = unsafe { webview.add_DownloadStarting(&started, &mut token) };
        self.downloads_supported.set(added.is_ok());
        if let Err(error) = added {
            log::debug!("web view: cannot report downloads: {error}");
        }
    }

    /// WebView2 started a download: Automatic (with a listener) writes it
    /// into the app's downloads directory; Manual holds its event open
    /// until the app accepts a destination or cancels it.
    fn download_started(self: &Rc<Self>, args: ICoreWebView2DownloadStartingEventArgs) {
        if self.closed.get() {
            // SAFETY: a WebView2 call on the live event.
            let _ = unsafe { args.SetCancel(true) };
            return;
        }
        let manual = self.emitter.listens(EventKind::DownloadPending);
        if !manual && !self.emitter.listens(EventKind::Download) {
            // No listener: WebView2's own download UI is intact.
            return;
        }
        let operation = {
            // SAFETY: a WebView2 getter on the live event.
            unsafe { args.DownloadOperation() }.ok()
        };
        let (mut url, mut suggested) = (String::new(), String::new());
        if let Some(operation) = &operation {
            // SAFETY: WebView2 getters on the live operation; the strings
            // are freed by `take_pwstr`.
            unsafe {
                let mut uri = PWSTR::null();
                if operation.Uri(&mut uri).is_ok() {
                    url = take_pwstr(uri);
                }
                let mut path = PWSTR::null();
                if operation.ResultFilePath(&mut path).is_ok() {
                    suggested = take_pwstr(path);
                }
            }
        }
        if suggested.is_empty() {
            // SAFETY: a WebView2 getter on the live event.
            unsafe {
                let mut path = PWSTR::null();
                if args.ResultFilePath(&mut path).is_ok() {
                    suggested = take_pwstr(path);
                }
            }
        }
        let filename = download_name(&suggested, &url);
        if manual {
            let id = next_download_id();
            // SAFETY: WebView2 calls on the live event: the default dialog
            // stays away (the app decides), and its deferral keeps it open
            // until the app answers.
            let pending = unsafe {
                match args.GetDeferral() {
                    Ok(deferral) => {
                        let _ = args.SetHandled(true);
                        Some(PendingDownload {
                            args: args.clone(),
                            deferral,
                            url: url.clone(),
                            filename: filename.clone(),
                        })
                    }
                    Err(why) => {
                        log::warn!("web view: cannot hold a download for the app: {why}");
                        self.emitter.emit(EventKind::Error, || {
                            error(format!("cannot hold a download for the app: {why}"))
                        });
                        let _ = args.SetCancel(true);
                        None
                    }
                }
            };
            let Some(pending) = pending else { return };
            self.downloads.borrow_mut().insert(id.clone(), pending);
            self.arm_download_deadline(&id);
            self.emitter.emit(EventKind::DownloadPending, || {
                webview::download_pending_payload(&id, &url, &filename)
            });
            return;
        }
        // Automatic: an app that listens for downloads opts into the app's
        // data directory.
        let Some(root) = self.downloads_root.as_ref() else {
            // SAFETY: a WebView2 call on the live event.
            let _ = unsafe { args.SetCancel(true) };
            let id = next_download_id();
            self.emit_download(
                "failed",
                &id,
                &url,
                &filename,
                Path::new(""),
                None,
                Some("the app has no data directory to download into".to_owned()),
            );
            return;
        };
        let Some(operation) = operation else {
            // SAFETY: a WebView2 call on the live event.
            let _ = unsafe { args.SetCancel(true) };
            let id = next_download_id();
            self.emit_download(
                "failed",
                &id,
                &url,
                &filename,
                Path::new(""),
                None,
                Some("the download has no operation".to_owned()),
            );
            return;
        };
        let id = next_download_id();
        let directory = root.join(&id);
        let path = directory.join(&filename);
        if let Err(error) = std::fs::create_dir_all(&directory) {
            // SAFETY: a WebView2 call on the live event.
            let _ = unsafe { args.SetCancel(true) };
            self.emit_download(
                "failed",
                &id,
                &url,
                &filename,
                &path,
                None,
                Some(format!(
                    "cannot make the download directory {}: {error}",
                    directory.display()
                )),
            );
            remove_later(directory, true);
            return;
        }
        if path.exists() {
            // SAFETY: a WebView2 call on the live event.
            let _ = unsafe { args.SetCancel(true) };
            self.emit_download(
                "failed",
                &id,
                &url,
                &filename,
                &path,
                None,
                Some(format!(
                    "the download path already exists: {}",
                    path.display()
                )),
            );
            remove_later(directory, true);
            return;
        }
        // SAFETY: WebView2 calls on the live event: the file goes to the
        // app's directory (the default dialog stays away).
        let result = unsafe {
            let mut result = args.SetResultFilePath(&HSTRING::from(&*path.to_string_lossy()));
            if result.is_ok() {
                result = args.SetCancel(false);
            }
            if result.is_ok() {
                result = args.SetHandled(true);
            }
            result
        };
        if let Err(error) = result {
            // SAFETY: a WebView2 call on the live event.
            let _ = unsafe { args.SetCancel(true) };
            self.emit_download(
                "failed",
                &id,
                &url,
                &filename,
                &path,
                None,
                Some(format!("cannot save the download: {}", error.message())),
            );
            remove_later(directory, true);
            return;
        }
        self.track_download(
            operation,
            &id,
            &url,
            &filename,
            &path,
            None,
            false,
            Some(directory),
        );
        self.emit_download("started", &id, &url, &filename, &path, None, None);
    }

    /// Keeps a manual download's decision bounded: after the timeout its
    /// event is answered (cancelled) and the app hears it failed.
    fn arm_download_deadline(&self, id: &str) {
        let Some(surface) = self.surface.upgrade() else {
            return;
        };
        let surface_key = surface.key();
        let node = self.emitter.id;
        let id = id.to_owned();
        let ui = crate::window::UiThread::current();
        std::thread::spawn(move || {
            std::thread::sleep(PENDING_DOWNLOAD_TIMEOUT);
            ui.invoke(move || {
                if let Some(surface) = Surface::by_key(surface_key)
                    && let Some(item) = super::web_item(&surface, node)
                {
                    item.expire_download(&id);
                }
            });
        });
    }

    /// A manual download waited too long for the app: cancel it (nothing
    /// is written) and report why.
    fn expire_download(&self, id: &str) {
        let Some(pending) = self.downloads.borrow_mut().remove(id) else {
            return;
        };
        let (url, filename) = (pending.url.clone(), pending.filename.clone());
        pending.cancel();
        self.emit_download(
            "failed",
            id,
            &url,
            &filename,
            Path::new(""),
            None,
            Some(format!(
                "the download was not accepted within {} seconds",
                PENDING_DOWNLOAD_TIMEOUT.as_secs()
            )),
        );
    }

    /// Follows a running download to its completed or interrupted end.
    #[allow(clippy::too_many_arguments)]
    fn track_download(
        &self,
        operation: ICoreWebView2DownloadOperation,
        id: &str,
        url: &str,
        filename: &str,
        path: &Path,
        staging: Option<PathBuf>,
        overwrite: bool,
        owned_directory: Option<PathBuf>,
    ) {
        let weak = self.me.borrow().clone();
        let key = id.to_owned();
        let handler = StateChangedEventHandler::create(Box::new(move |sender, _| {
            if let (Some(item), Some(operation)) = (weak.upgrade(), sender) {
                item.download_state(&key, &operation);
            }
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: a WebView2 call on the live operation; the handler holds
        // the item weakly and goes with the operation.
        unsafe {
            let _ = operation.add_StateChanged(&handler, &mut token);
        }
        self.active_downloads.borrow_mut().insert(
            id.to_owned(),
            ActiveDownload {
                operation,
                id: id.to_owned(),
                url: url.to_owned(),
                filename: filename.to_owned(),
                path: path.to_path_buf(),
                staging,
                overwrite,
                owned_directory,
                cancel_request: Cell::new(None),
                terminal: Cell::new(false),
            },
        );
    }

    /// A tracked download changed state: report its completion or failure.
    fn download_state(&self, id: &str, operation: &ICoreWebView2DownloadOperation) {
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS;
        // SAFETY: a WebView2 getter on the live operation.
        if unsafe { operation.State(&mut state) }.is_err() {
            return;
        }
        if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
            let Some(entry) = self.active_downloads.borrow_mut().remove(id) else {
                return;
            };
            // A cancel that raced the completion wins: the staged bytes are
            // discarded and the app hears `failed`, never a later `finished`.
            if let Some(request) = entry.cancel_request.take() {
                entry.cleanup();
                if take_terminal(&entry.terminal) {
                    self.emit_download(
                        "failed",
                        &entry.id,
                        &entry.url,
                        &entry.filename,
                        &entry.path,
                        None,
                        Some("the download was cancelled".to_owned()),
                    );
                }
                self.reply(request, Ok(JsonValue::Null));
                return;
            }
            let published = entry.publish();
            let bytes = std::fs::metadata(&entry.path).map(|m| m.len()).ok();
            match published {
                Ok(()) if take_terminal(&entry.terminal) => self.emit_download(
                    "finished",
                    &entry.id,
                    &entry.url,
                    &entry.filename,
                    &entry.path,
                    bytes,
                    None,
                ),
                Ok(()) => {}
                Err(message) => {
                    // The app's destination was not replaced: it was never
                    // touched before the publish.
                    entry.cleanup();
                    if !take_terminal(&entry.terminal) {
                        return;
                    }
                    self.emit_download(
                        "failed",
                        &entry.id,
                        &entry.url,
                        &entry.filename,
                        &entry.path,
                        None,
                        Some(message),
                    );
                }
            }
        } else if state == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
            let Some(entry) = self.active_downloads.borrow_mut().remove(id) else {
                return;
            };
            let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON(0);
            // SAFETY: a WebView2 getter on the live operation.
            let _ = unsafe { operation.InterruptReason(&mut reason) };
            // Writing has stopped: only this backend's staging file (or
            // its automatic directory) goes.
            entry.cleanup_after_stop();
            if !take_terminal(&entry.terminal) {
                return;
            }
            self.emit_download(
                "failed",
                &entry.id,
                &entry.url,
                &entry.filename,
                &entry.path,
                None,
                Some(interrupt_message(reason.0)),
            );
            if let Some(request) = entry.cancel_request.take() {
                self.reply(request, Ok(JsonValue::Null));
            }
        }
    }

    /// Reports a download through the app's existing `onDownload`
    /// lifecycle.
    #[allow(clippy::too_many_arguments)]
    fn emit_download(
        &self,
        state: &str,
        id: &str,
        url: &str,
        filename: &str,
        path: &Path,
        bytes: Option<u64>,
        message: Option<String>,
    ) {
        self.emitter.emit(EventKind::Download, || {
            download_payload(state, id, url, filename, path, bytes, message)
        });
    }

    /// Answers web view request `request` (an internal reply event, not a
    /// listener's).
    fn reply(&self, request: u64, result: Result<JsonValue, String>) {
        let Some(surface) = self.surface.upgrade() else {
            return;
        };
        reply(&surface, self.id, request, result);
    }

    /// Handles a [`WebViewOperation`] on this web view.
    fn web_request(&self, request: &WebViewRequest) {
        match &request.operation {
            WebViewOperation::AcceptDownload { id, destination } => {
                let result = self.accept_download(id, &destination.path, destination.overwrite);
                self.reply(request.id, result);
            }
            WebViewOperation::CancelDownload { id } => {
                self.cancel_download(request.id, id);
            }
            // A newer core's operation: answer it rather than leave it
            // pending.
            _ => self.reply(
                request.id,
                Err("this Windows backend does not know the web view operation".into()),
            ),
        }
    }

    /// Writes the waiting download `id` to the app's `path`.
    fn accept_download(&self, id: &str, path: &Path, overwrite: bool) -> Result<JsonValue, String> {
        if !self.downloads.borrow().contains_key(id) {
            return Err(self.no_download(id));
        }
        self.check_destination(path, overwrite)?;
        let staging = staging_path(path, id)?;
        if staging.exists() {
            return Err(format!(
                "the temporary download path already exists: {}",
                staging.display()
            ));
        }
        let Some(pending) = self.downloads.borrow_mut().remove(id) else {
            return Err(self.no_download(id));
        };
        let Some(operation) = pending.operation() else {
            pending.cancel();
            return Err("the download has no operation".to_owned());
        };
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| pending.filename.clone());
        // SAFETY: WebView2 calls on the live event and its deferral: the
        // staged file goes next to the app's path (the default dialog stays
        // away) and completing the deferral ends the event.
        let applied = unsafe {
            let mut applied = pending
                .args
                .SetResultFilePath(&HSTRING::from(&*staging.to_string_lossy()));
            if applied.is_ok() {
                applied = pending.args.SetCancel(false);
            }
            if applied.is_ok() {
                applied = pending.args.SetHandled(true);
            }
            if applied.is_ok() {
                applied = pending.deferral.Complete();
            }
            applied
        };
        if let Err(error) = applied {
            // Not left open: a download the app cannot take is no
            // download.
            pending.cancel();
            remove_later(staging, false);
            return Err(format!("cannot take the download: {}", error.message()));
        }
        self.track_download(
            operation,
            id,
            &pending.url,
            &filename,
            path,
            Some(staging),
            overwrite,
            None,
        );
        self.emit_download("started", id, &pending.url, &filename, path, None, None);
        Ok(JsonValue::Null)
    }

    /// Cancels the waiting or running download `id`: nothing is written
    /// (or what was written is removed). A running transfer's answer waits
    /// for WebView2's state-changed callback, which confirms writing
    /// stopped.
    fn cancel_download(&self, request: u64, id: &str) {
        let mut waiting = false;
        let result = (|| -> Result<JsonValue, String> {
            let pending = self.downloads.borrow_mut().remove(id);
            if let Some(pending) = pending {
                pending.cancel();
                return Ok(JsonValue::Null);
            }
            let completed;
            let operation;
            {
                let active = self.active_downloads.borrow();
                let Some(entry) = active.get(id) else {
                    return Err(self.no_download(id));
                };
                if entry.cancel_request.get().is_some() {
                    return Err(format!("a cancellation is already in progress for {id}"));
                }
                completed = entry.state() == Some(COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED);
                operation = entry.operation.clone();
                if !completed {
                    entry.cancel_request.set(Some(request));
                }
            }
            if completed {
                let entry = self.active_downloads.borrow_mut().remove(id);
                let Some(entry) = entry else {
                    return Err(self.no_download(id));
                };
                // The app's cancel wins even if the bytes all arrived: the
                // staged output is discarded and the one terminal event is
                // `failed`.
                entry.cleanup();
                if take_terminal(&entry.terminal) {
                    self.emit_download(
                        "failed",
                        &entry.id,
                        &entry.url,
                        &entry.filename,
                        &entry.path,
                        None,
                        Some("the download was cancelled".to_owned()),
                    );
                }
                return Ok(JsonValue::Null);
            }
            // SAFETY: a WebView2 call on the live operation.
            if let Err(error) = unsafe { operation.Cancel() } {
                let entry = self.active_downloads.borrow_mut().remove(id);
                if let Some(entry) = entry {
                    entry.cleanup();
                    self.emit_download(
                        "failed",
                        &entry.id,
                        &entry.url,
                        &entry.filename,
                        &entry.path,
                        None,
                        Some(format!("cannot cancel the download: {}", error.message())),
                    );
                }
                return Err(format!("cannot cancel the download: {}", error.message()));
            }
            // The state-changed callback completes this request after the
            // transfer stopped and its staging file is gone.
            waiting = true;
            Ok(JsonValue::Null)
        })();
        if !waiting {
            self.reply(request, result);
        }
    }

    /// Why download `id` is not waiting or running.
    fn no_download(&self, id: &str) -> String {
        if self.downloads_supported.get() {
            format!("no download is waiting with id {id}")
        } else {
            "this WebView2 runtime does not report downloads".to_owned()
        }
    }

    /// Whether `path` is a destination this app can accept: absolute, in an
    /// existing directory, not a directory, not taken (unless `overwrite`),
    /// and not another running download's.
    fn check_destination(&self, path: &Path, overwrite: bool) -> Result<(), String> {
        let active: Vec<PathBuf> = self
            .active_downloads
            .borrow()
            .values()
            .map(|active| active.path.clone())
            .collect();
        destination_error(path, overwrite, &active)
    }

    fn listen_input(
        self: &Rc<Self>,
        composition: &ICoreWebView2CompositionController,
        controller: &ICoreWebView2Controller,
    ) {
        let mut token = 0i64;
        let weak = Rc::downgrade(self);
        let cursor = CursorChangedEventHandler::create(Box::new(move |sender, _| {
            let (Some(item), Some(sender)) = (weak.upgrade(), sender) else {
                return Ok(());
            };
            let mut handle = HCURSOR::default();
            // SAFETY: a WebView2 getter on the event's live sender.
            if unsafe { sender.Cursor(&mut handle) }.is_ok() {
                item.cursor.set(handle.0 as isize);
                if item.hovered.get() && !handle.is_invalid() {
                    // The window asks for the cursor only when the mouse
                    // moves: show the page's now.
                    // SAFETY: a cursor handle WebView2 just gave.
                    unsafe { SetCursor(Some(handle)) };
                }
            }
            Ok(())
        }));
        let weak = Rc::downgrade(self);
        let got = FocusChangedEventHandler::create(Box::new(move |_, _| {
            if let Some(item) = weak.upgrade() {
                item.focused.set(true);
                // The page has the keyboard: nsh's fields give it up, as a
                // text field resigns when a web view becomes first
                // responder.
                if let Some(surface) = item.surface.upgrade() {
                    crate::text_input::blur_all(&surface);
                }
            }
            Ok(())
        }));
        let weak = Rc::downgrade(self);
        let lost = FocusChangedEventHandler::create(Box::new(move |_, _| {
            if let Some(item) = weak.upgrade() {
                item.focused.set(false);
            }
            Ok(())
        }));
        let weak = Rc::downgrade(self);
        let tab_out = MoveFocusRequestedEventHandler::create(Box::new(move |_, args| {
            if let (Some(item), Some(args)) = (weak.upgrade(), args) {
                item.focus_window();
                // SAFETY: a WebView2 call on the event's live object.
                let _ = unsafe { args.SetHandled(true) };
            }
            Ok(())
        }));
        // SAFETY: WebView2 calls on the live controllers; the handlers hold
        // the item weakly.
        unsafe {
            let _ = composition.add_CursorChanged(&cursor, &mut token);
            let _ = controller.add_GotFocus(&got, &mut token);
            let _ = controller.add_LostFocus(&lost, &mut token);
            let _ = controller.add_MoveFocusRequested(&tab_out, &mut token);
        }
    }

    /// Gives the keyboard back to the window it is in.
    fn focus_window(&self) {
        let Some(hwnd) = self.hwnd() else {
            return;
        };
        // SAFETY: plain calls on this thread's window.
        unsafe {
            let focus = GetFocus();
            if focus != hwnd && (focus.is_invalid() || IsChild(hwnd, focus).as_bool()) {
                let _ = SetFocus(Some(hwnd));
            }
        }
        self.focused.set(false);
    }

    /// True when the app may show `url` at the top level.
    fn allows(&self, url: &str) -> bool {
        // Pages the app gave as HTML.
        if url.starts_with("data:")
            || self
                .html
                .borrow()
                .as_ref()
                .is_some_and(|(base, _)| same_document(url, base))
        {
            return true;
        }
        let Some(surface) = self.surface.upgrade() else {
            return false;
        };
        let access = surface.native.access.borrow().clone();
        access.is_none_or(|a: Arc<nsh_engine::Access>| a.allows_webview(url))
    }

    /// Whether the page may capture `media` (`webview.media`): never
    /// without a declaration.
    fn allows_media(&self, media: WebMedia) -> bool {
        let Some(surface) = self.surface.upgrade() else {
            return false;
        };
        let access = surface.native.access.borrow().clone();
        access.is_some_and(|a| a.allows_web_media(media))
    }

    pub(crate) fn set(self: &Rc<Self>, surface: &Rc<Surface>, view: &NshView, p: &WebProps) {
        self.background.set(view.state().style.background_color);
        *self.props.borrow_mut() = Some(p.clone());
        // The core keeps `incognito` from changing once it is made.
        self.start(surface);
        if self.webview.borrow().is_some() {
            self.paint_background();
            self.apply(p);
        }
        self.invalidate();
    }

    /// Applies `p` to the web view (it is ready).
    fn apply(&self, p: &WebProps) {
        let Some(webview) = self.webview.borrow().clone() else {
            return;
        };
        let old = self.applied.borrow_mut().replace(p.clone());
        let scripts_changed = old.as_ref().is_none_or(|o| {
            o.injected_js != p.injected_js || o.injected_js_before != p.injected_js_before
        });
        // SAFETY: WebView2 calls on the live web view, on its thread.
        unsafe {
            if let Ok(settings) = webview.Settings() {
                let _ = settings.SetIsScriptEnabled(p.javascript_enabled);
                if let Ok(settings) = settings.cast::<ICoreWebView2Settings2>() {
                    let agent = p
                        .user_agent
                        .as_deref()
                        .map(str::to_owned)
                        .or_else(|| self.default_agent.borrow().clone());
                    if let Some(agent) = agent {
                        let _ = settings.SetUserAgent(&HSTRING::from(agent));
                    }
                }
                if let Ok(settings) = settings.cast::<ICoreWebView2Settings6>() {
                    let _ = settings.SetIsSwipeNavigationEnabled(p.back_forward_gestures);
                }
            }
            if scripts_changed {
                for id in self.scripts.borrow_mut().drain(..) {
                    let _ = webview.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(id));
                }
                let scripts = [
                    p.injected_js_before.as_deref().map(before_content),
                    p.injected_js.as_deref().map(after_content),
                ];
                for script in scripts.into_iter().flatten() {
                    let ids = Rc::downgrade(&self.scripts);
                    let handler = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(
                        Box::new(move |result, id| {
                            if let (Ok(()), Some(ids)) = (result, ids.upgrade()) {
                                ids.borrow_mut().push(id);
                            }
                            Ok(())
                        }),
                    );
                    let _ = webview
                        .AddScriptToExecuteOnDocumentCreated(&HSTRING::from(script), &handler);
                }
            }
            if old.as_ref().is_none_or(|o| o.scrollable) != p.scrollable {
                self.set_scrolls(&webview, p.scrollable);
            }
            let navigate = scripts_changed || old.as_ref().is_none_or(|o| o.source != p.source);
            if navigate {
                let result = match &p.source {
                    WebSource::Uri(uri) => {
                        self.html.borrow_mut().take();
                        webview.Navigate(&HSTRING::from(&**uri))
                    }
                    WebSource::Html {
                        html,
                        base_url: Some(base),
                    } if !base.is_empty() => {
                        *self.html.borrow_mut() = Some((base.to_string(), html.clone()));
                        self.serve_html(&webview);
                        webview.Navigate(&HSTRING::from(&**base))
                    }
                    WebSource::Html { html, .. } => {
                        self.html.borrow_mut().take();
                        webview.NavigateToString(&HSTRING::from(&**html))
                    }
                    WebSource::None => {
                        self.html.borrow_mut().take();
                        webview.Navigate(w!("about:blank"))
                    }
                };
                if let Err(err) = result {
                    let message = err.message();
                    self.emitter.emit(EventKind::Error, || error(message));
                }
            }
        }
    }

    /// `scrollable`: the page's viewport scrolls, or not (the style is
    /// put in the page now and in every page loaded later).
    fn set_scrolls(&self, webview: &ICoreWebView2, enabled: bool) {
        let none = None::<&ICoreWebView2ExecuteScriptCompletedHandler>;
        // SAFETY: WebView2 calls on the live web view, on its thread.
        unsafe {
            if let Some(id) = self.scroll_script.borrow_mut().take() {
                let _ = webview.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(id));
            }
            if !enabled {
                let slot = Rc::downgrade(&self.scroll_script);
                let handler = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(
                    Box::new(move |result, id| {
                        if let (Ok(()), Some(slot)) = (result, slot.upgrade()) {
                            *slot.borrow_mut() = Some(id);
                        }
                        Ok(())
                    }),
                );
                let _ = webview.AddScriptToExecuteOnDocumentCreated(
                    &HSTRING::from(scroll_style(false)),
                    &handler,
                );
            }
            let _ = webview.ExecuteScript(&HSTRING::from(scroll_style(enabled)), none);
        }
    }

    /// Answers the request of the document at the base URL with the app's
    /// HTML (`baseUrl`).
    fn serve_html(&self, webview: &ICoreWebView2) {
        if self.serving.replace(true) {
            return;
        }
        let item = self.emitter.clone();
        let surface = self.surface.clone();
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let Some(web) = surface.upgrade().and_then(|s| super::web_item(&s, item.id)) else {
                return Ok(());
            };
            let Some((base, html)) = web.html.borrow().clone() else {
                return Ok(());
            };
            let Some(env) = web.env.borrow().clone() else {
                return Ok(());
            };
            // SAFETY: WebView2 calls on the event's live objects; the
            // stream holds a copy of the bytes.
            unsafe {
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&mut uri)?;
                if !same_document(&take_pwstr(uri), &base) {
                    return Ok(());
                }
                let Some(stream) = SHCreateMemStream(Some(html.as_bytes())) else {
                    return Ok(());
                };
                let response = env.CreateWebResourceResponse(
                    &stream,
                    200,
                    w!("OK"),
                    w!("Content-Type: text/html; charset=utf-8"),
                )?;
                args.SetResponse(&response)?;
            }
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: WebView2 calls on the live web view; the handler finds the
        // item through its surface.
        unsafe {
            let _ = webview
                .AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT);
            let _ = webview.add_WebResourceRequested(&handler, &mut token);
        }
    }

    /// `reload()`, `goBack()`, `goForward()`, `stopLoading()`,
    /// `injectJavaScript()`, `postMessage()` and `load()`.
    pub(crate) fn command(&self, command: &Command) {
        if self.manual_downloads_unavailable() && inert_refuses(command) {
            self.emitter.emit(EventKind::Error, || {
                error("manual downloads need a WebView2 runtime that reports downloads".to_owned())
            });
            return;
        }
        if let Command::WebViewRequest(request) = command {
            self.web_request(request);
            return;
        }
        let Some(webview) = self.webview.borrow().clone() else {
            if !self.failed.get() {
                self.queued.borrow_mut().push(command.clone());
            }
            return;
        };
        let none = None::<&ICoreWebView2ExecuteScriptCompletedHandler>;
        // SAFETY: WebView2 calls on the live web view, on its thread.
        let _ = unsafe {
            match command {
                Command::Reload => webview.Reload(),
                Command::GoBack => webview.GoBack(),
                Command::GoForward => webview.GoForward(),
                Command::StopLoading => webview.Stop(),
                Command::InjectJavaScript(js) => webview.ExecuteScript(&HSTRING::from(js), none),
                Command::PostMessage(data) => {
                    webview.ExecuteScript(&HSTRING::from(post_message(data)), none)
                }
                Command::Load(load) => self.load(&webview, load),
                _ => Ok(()),
            }
        };
    }

    /// `load()`: a navigation whose own request carries the app's headers
    /// (WebView2 adds none of them to the page's subresources or later
    /// navigations). Runtimes before `ICoreWebView2_2` cannot: an error.
    fn load(&self, webview: &ICoreWebView2, load: &nsh_core::WebLoad) -> windows::core::Result<()> {
        let env = self.env.borrow().clone();
        let (Some(env2), Ok(webview2)) = (
            env.and_then(|e| e.cast::<ICoreWebView2Environment2>().ok()),
            webview.cast::<ICoreWebView2_2>(),
        ) else {
            self.emitter.emit(EventKind::Error, || {
                error("this WebView2 runtime cannot send request headers".into())
            });
            return Ok(());
        };
        // `Name: value` lines; `WebLoad` has no CR or LF in them.
        let headers: String = load
            .headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect();
        // SAFETY: WebView2 calls on the live environment and web view, on
        // their thread.
        unsafe {
            let request = env2.CreateWebResourceRequest(
                &HSTRING::from(load.src.as_str()),
                w!("GET"),
                None::<&windows::Win32::System::Com::IStream>,
                &HSTRING::from(headers),
            )?;
            webview2.NavigateWithWebResourceRequest(&request)
        }
    }

    /// The view's background behind the page until it paints (when the
    /// view has an opaque one).
    fn paint_background(&self) {
        let color = self.background.get();
        let Some(controller) = self.controller.borrow().clone() else {
            return;
        };
        let (r, g, b, a) = color.to_f64();
        if a < 1.0 {
            return;
        }
        let byte = |v: f64| (v * 255.0).round() as u8;
        // SAFETY: a WebView2 call on the live controller.
        unsafe {
            if let Ok(c) = controller.cast::<ICoreWebView2Controller2>() {
                let _ = c.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                    A: 255,
                    R: byte(r),
                    G: byte(g),
                    B: byte(b),
                });
            }
        }
    }

    /// Gives the controller the bounds, visibility and scale of where the
    /// last frame put the page.
    fn place(&self) {
        let Some(controller) = self.controller.borrow().clone() else {
            return;
        };
        let at = self.at.get();
        let bounds = at.map(|p| p.bounds());
        let before = self.bounds.replace(bounds);
        if before == bounds {
            return;
        }
        // SAFETY: WebView2 calls on the live controller, on its thread.
        unsafe {
            if let Some(b) = bounds {
                let scale = self.scale();
                if self.raster.replace(scale) != scale
                    && let Ok(c) = controller.cast::<ICoreWebView2Controller3>()
                {
                    let _ = c.SetRasterizationScale(scale as f64);
                }
                let _ = controller.SetBounds(b);
                if before.is_none() {
                    let _ = controller.SetIsVisible(true);
                } else {
                    // Pop-ups (select menus, tooltips) follow the page.
                    let _ = controller.NotifyParentWindowPositionChanged();
                }
            } else {
                let _ = controller.SetIsVisible(false);
            }
        }
        if bounds.is_none() && self.focused.get() {
            // A hidden page keeps no keyboard focus.
            self.focus_window();
        }
    }

    /// Its view was deleted: it closes once the view is not drawn anymore
    /// (now, unless a removal animation keeps it on screen).
    pub(crate) fn retire(&self) {
        self.retired.set(true);
        if self.at.get().is_none() {
            self.close();
        }
    }

    /// The page closes (the app stopped, or its view is gone).
    pub(crate) fn close(&self) {
        if self.closed.replace(true) {
            return;
        }
        // Pending decisions are released (nothing is written); running
        // transfers stop, removing only what this backend wrote.
        let pending = std::mem::take(&mut *self.downloads.borrow_mut());
        for (_, pending) in pending {
            pending.cancel();
        }
        let active = std::mem::take(&mut *self.active_downloads.borrow_mut());
        for entry in active.into_values() {
            entry.cancel();
        }
        if self.focused.get() {
            self.focus_window();
        }
        self.webview.borrow_mut().take();
        self.queued.borrow_mut().clear();
        self.composition.borrow_mut().take();
        if let Some(controller) = self.controller.borrow_mut().take() {
            // SAFETY: closes the live controller; it is not used afterwards.
            let _ = unsafe { controller.Close() };
        }
        // Its visuals leave the composition (gone with the last web view).
        self.portal.borrow_mut().take();
        self.invalidate();
    }

    /// Pixels per point of the window it is in.
    fn scale(&self) -> f32 {
        match self.host.get() {
            Some((window, hwnd)) if window != ROOT_ID => crate::window::dpi(hwnd) as f32 / 96.0,
            _ => self.surface.upgrade().map_or(1.0, |s| s.scale()),
        }
    }

    /// A touch or pen contact through `SendPointerInput`, with the
    /// system's data of pointer `id` moved to `point` (the page's pixels).
    /// False when the system has no data for it (synthesized input).
    fn send_pointer(&self, event: &ItemPointer, point: POINT) -> bool {
        let (Some(composition), Some(env), Some(hwnd)) = (
            self.composition.borrow().clone(),
            self.env.borrow().clone(),
            self.hwnd(),
        ) else {
            return false;
        };
        let Some(id) = event.pointer.checked_sub(2) else {
            return false;
        };
        let Ok(env) = env.cast::<ICoreWebView2Environment3>() else {
            return false;
        };
        let kind = match event.kind {
            PointerKind::Down => 0x0246,
            PointerKind::Move => 0x0245,
            PointerKind::Up | PointerKind::Cancel => 0x0247,
            PointerKind::Leave => 0x024A,
        };
        // SAFETY: plain pointer queries into locals, then WebView2 calls on
        // live objects of this thread.
        unsafe {
            let mut info = POINTER_INFO::default();
            if GetPointerInfo(id, &mut info).is_err() {
                return false;
            }
            let Ok(p) = env.CreateCoreWebView2PointerInfo() else {
                return false;
            };
            let mut client = info.ptPixelLocation;
            let _ = ScreenToClient(hwnd, &mut client);
            // From where the contact is in the window to where nsh put it
            // on the page.
            let (dx, dy) = (point.x - client.x, point.y - client.y);
            let mut device = RECT::default();
            let mut display = RECT::default();
            let _ = GetPointerDeviceRects(info.sourceDevice, &mut device, &mut display);
            // Himetric units per pixel.
            let ratio = |d: i32, s: i32| if s != 0 { d as f32 / s as f32 } else { 1.0 };
            let (hx, hy) = (
                ratio(device.right - device.left, display.right - display.left),
                ratio(device.bottom - device.top, display.bottom - display.top),
            );
            let moved = |q: POINT| POINT {
                x: q.x + dx,
                y: q.y + dy,
            };
            let moved_himetric = |q: POINT| POINT {
                x: q.x + (dx as f32 * hx) as i32,
                y: q.y + (dy as f32 * hy) as i32,
            };
            let _ = p.SetPointerKind(info.pointerType.0 as u32);
            let _ = p.SetPointerId(info.pointerId);
            let _ = p.SetFrameId(info.frameId);
            let _ = p.SetPointerFlags(info.pointerFlags.0);
            let _ = p.SetPointerDeviceRect(device);
            let _ = p.SetDisplayRect(display);
            let _ = p.SetPixelLocation(point);
            let _ = p.SetHimetricLocation(moved_himetric(info.ptHimetricLocation));
            let _ = p.SetPixelLocationRaw(moved(info.ptPixelLocationRaw));
            let _ = p.SetHimetricLocationRaw(moved_himetric(info.ptHimetricLocationRaw));
            let _ = p.SetTime(info.dwTime);
            let _ = p.SetHistoryCount(info.historyCount);
            let _ = p.SetInputData(info.InputData);
            let _ = p.SetKeyStates(info.dwKeyStates);
            let _ = p.SetPerformanceCount(info.PerformanceCount);
            let _ = p.SetButtonChangeKind(info.ButtonChangeType.0);
            if event.touch {
                let mut touch = POINTER_TOUCH_INFO::default();
                if GetPointerTouchInfo(id, &mut touch).is_ok() {
                    let shift = |r: RECT| RECT {
                        left: r.left + dx,
                        top: r.top + dy,
                        right: r.right + dx,
                        bottom: r.bottom + dy,
                    };
                    let _ = p.SetTouchFlags(touch.touchFlags);
                    let _ = p.SetTouchMask(touch.touchMask);
                    let _ = p.SetTouchContact(shift(touch.rcContact));
                    let _ = p.SetTouchContactRaw(shift(touch.rcContactRaw));
                    let _ = p.SetTouchOrientation(touch.orientation);
                    let _ = p.SetTouchPressure(touch.pressure);
                }
            } else {
                let mut pen = POINTER_PEN_INFO::default();
                if GetPointerPenInfo(id, &mut pen).is_ok() {
                    let _ = p.SetPenFlags(pen.penFlags);
                    let _ = p.SetPenMask(pen.penMask);
                    let _ = p.SetPenPressure(pen.pressure);
                    let _ = p.SetPenRotation(pen.rotation);
                    let _ = p.SetPenTiltX(pen.tiltX);
                    let _ = p.SetPenTiltY(pen.tiltY);
                }
            }
            composition
                .SendPointerInput(COREWEBVIEW2_POINTER_EVENT_KIND(kind), &p)
                .is_ok()
        }
    }

    /// Sends a mouse event to the page.
    fn send_mouse(
        &self,
        kind: COREWEBVIEW2_MOUSE_EVENT_KIND,
        data: u32,
        point: POINT,
        m: Modifiers,
    ) {
        let Some(composition) = self.composition.borrow().clone() else {
            return;
        };
        // MK_LBUTTON, MK_RBUTTON, MK_MBUTTON, MK_XBUTTON1 and MK_XBUTTON2 by
        // button; MK_SHIFT and MK_CONTROL.
        const BUTTON_KEYS: [i32; 5] = [0x1, 0x2, 0x10, 0x20, 0x40];
        let held = self.buttons.get();
        let mut keys = (0..5)
            .filter(|b| held & (1 << b) != 0)
            .fold(0, |k, b| k | BUTTON_KEYS[b]);
        if m.shift {
            keys |= 0x4;
        }
        if m.ctrl {
            keys |= 0x8;
        }
        // SAFETY: a WebView2 call on the live controller, on its thread.
        let sent = unsafe {
            composition.SendMouseInput(
                kind,
                COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(keys),
                data,
                point,
            )
        };
        if let Err(err) = sent {
            log::debug!("web view: input not sent: {err}");
        }
    }
}

impl Drop for WebItem {
    fn drop(&mut self) {
        self.close();
    }
}

/// The mouse event of `button` going down (with its click count) or up.
fn button_event(button: u8, down: bool, clicks: u32) -> (COREWEBVIEW2_MOUSE_EVENT_KIND, u32) {
    // Windows sends a double click as the second press of a pair.
    let double = down && clicks >= 2 && clicks.is_multiple_of(2);
    let kind = match (button, down, double) {
        (0, true, false) => COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
        (0, true, true) => COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOUBLE_CLICK,
        (0, false, _) => COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP,
        (1, true, false) => COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN,
        (1, true, true) => COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOUBLE_CLICK,
        (1, false, _) => COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP,
        (2, true, false) => COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOWN,
        (2, true, true) => COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOUBLE_CLICK,
        (2, false, _) => COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_UP,
        (_, true, false) => COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_DOWN,
        (_, true, true) => COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_DOUBLE_CLICK,
        (_, false, _) => COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_UP,
    };
    // XBUTTON1 is the back button (3), XBUTTON2 the forward one (4).
    let data = match button {
        3 => 1,
        4 => 2,
        _ => 0,
    };
    (kind, data)
}

/// The wheel delta (120 a notch) of a scroll by `points`.
fn wheel_delta(points: f32) -> i32 {
    (points / (crate::window::lines_per_notch() * 16.0) * 120.0).round() as i32
}

impl Hosted for WebItem {
    fn draw(&self, cx: &mut DrawCx<'_>, size: (f32, f32)) {
        if !self.failed.get() {
            return;
        }
        let Some(format) = placeholder_format() else {
            return;
        };
        let text: Vec<u16> = "WebView is not available on this system"
            .encode_utf16()
            .collect();
        cx.push_clip_rect(0.0, 0.0, size.0, size.1);
        // SAFETY: a Direct2D call between BeginDraw and EndDraw.
        unsafe {
            cx.dc.DrawText(
                &text,
                &format,
                &rect_f(8.0, 0.0, (size.0 - 16.0).max(0.0), size.1),
                cx.solid(Color::rgba(140, 140, 140, 255)),
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
        cx.pop();
    }

    fn hit(&self, x: f32, y: f32, size: (f32, f32)) -> bool {
        // The page takes the pointer where nothing of nsh's covers it.
        self.composed() && x >= 0.0 && y >= 0.0 && x < size.0 && y < size.1
    }

    fn composed(&self) -> bool {
        // Before it is bound too: the first frame that reaches it binds it
        // ([`Hosted::compose_at`]).
        !self.failed.get() && !self.closed.get()
    }

    fn visual(&self) -> Option<IDCompositionVisual2> {
        self.portal.borrow().as_ref().map(|p| p.visual().clone())
    }

    fn compose_at(&self, at: Option<&Placement>) {
        if at.is_some()
            && self.host.get().is_none()
            && let Some(surface) = self.surface.upgrade()
        {
            // Drawn in a window: its page goes there, and the window draws
            // again with it in its composition.
            if let Some(item) = super::web_item(&surface, self.id) {
                if self.props.borrow().is_some() {
                    item.start(&surface);
                } else {
                    self.bind(&surface);
                }
            }
            self.invalidate();
        }
        self.at.set(at.copied());
        if let Some(portal) = &*self.portal.borrow() {
            match at {
                Some(p) => portal.place(p),
                None => portal.hidden(),
            }
        }
        self.place();
        if at.is_none() && self.retired.get() {
            self.close();
        }
    }

    fn raw(&self) -> bool {
        self.composed()
    }

    fn pointer(&self, event: &ItemPointer) -> bool {
        let s = self.scale();
        let point = POINT {
            x: (event.x * s).round() as i32,
            y: (event.y * s).round() as i32,
        };
        if (event.touch || event.pen) && self.send_pointer(event, point) {
            if event.kind == PointerKind::Down && !self.focused.get() {
                self.take_focus();
            }
            return true;
        }
        // Synthesized contacts (and pens without data) come as the left
        // button.
        let button = if event.touch || event.pen {
            0
        } else {
            event.button
        };
        let bit = 1u8 << button.min(7);
        match event.kind {
            PointerKind::Down => {
                let (kind, data) = button_event(button, true, event.clicks);
                self.buttons.set(self.buttons.get() | bit);
                self.send_mouse(kind, data, point, event.modifiers);
                if !self.focused.get() {
                    self.take_focus();
                }
            }
            PointerKind::Up => {
                let (kind, data) = button_event(button, false, 1);
                self.buttons.set(self.buttons.get() & !bit);
                self.send_mouse(kind, data, point, event.modifiers);
            }
            PointerKind::Move => {
                self.hovered.set(true);
                self.send_mouse(
                    COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE,
                    0,
                    point,
                    event.modifiers,
                );
            }
            PointerKind::Cancel | PointerKind::Leave => {
                self.hovered.set(false);
                self.buttons.set(0);
                self.send_mouse(
                    COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE,
                    0,
                    point,
                    event.modifiers,
                );
            }
        }
        true
    }

    fn wheel(&self, x: f32, y: f32, event: &WheelEvent) -> bool {
        if !self.applied.borrow().as_ref().is_none_or(|p| p.scrollable) {
            // The scroll views around it scroll instead.
            return false;
        }
        let s = self.scale();
        let point = POINT {
            x: (x * s).round() as i32,
            y: (y * s).round() as i32,
        };
        // Positive deltas scroll up and right, as `WM_MOUSEWHEEL` and
        // `WM_MOUSEHWHEEL`.
        if event.dy != 0.0 {
            let delta = wheel_delta(-event.dy);
            self.send_mouse(
                COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL,
                delta as u32,
                point,
                event.modifiers,
            );
        }
        if event.dx != 0.0 {
            let delta = wheel_delta(event.dx);
            self.send_mouse(
                COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL,
                delta as u32,
                point,
                event.modifiers,
            );
        }
        true
    }

    fn cursor(&self, _x: f32, _y: f32) -> Option<crate::input::Cursor> {
        let c = self.cursor.get();
        (c != 0 && self.composed()).then_some(crate::input::Cursor::Native(c))
    }
}

impl WebItem {
    /// Gives the page the keyboard focus (a click on it).
    fn take_focus(&self) {
        if let Some(controller) = self.controller.borrow().clone() {
            // SAFETY: a WebView2 call on the live controller.
            let _ = unsafe { controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC) };
        }
    }
}

/// True when `a` and `b` name the same document (fragments aside; a URL
/// with no path is its root's).
fn same_document(a: &str, b: &str) -> bool {
    fn normal(url: &str) -> String {
        let url = url.split('#').next().unwrap_or(url);
        match url.find("://") {
            Some(i) => {
                let rest = &url[i + 3..];
                let (host, path) = match rest.find('/') {
                    Some(j) => (&rest[..j], &rest[j..]),
                    None => (rest, "/"),
                };
                format!(
                    "{}://{}{path}",
                    url[..i].to_ascii_lowercase(),
                    host.to_ascii_lowercase()
                )
            }
            None => url.to_owned(),
        }
    }
    normal(a) == normal(b)
}

/// The script that lets the page's viewport scroll, or keeps it from
/// scrolling (a style on its root element; scripts still scroll it).
fn scroll_style(enabled: bool) -> String {
    format!(
        "(function () {{ var on = {enabled}; var id = 'nsh-no-scroll'; \
         function apply() {{ var s = document.getElementById(id); \
         if (on) {{ if (s) s.remove(); return; }} \
         if (!s && document.documentElement) {{ s = document.createElement('style'); s.id = id; \
         s.textContent = 'html {{ overflow: hidden !important; }}'; document.documentElement.appendChild(s); }} }} \
         if (document.documentElement) apply(); else document.addEventListener('readystatechange', apply, {{ once: true }}); }})();"
    )
}

/// The page's navigation state, as `WebViewNavigation`.
#[derive(Debug, Clone, Default, PartialEq)]
struct Navigation {
    url: String,
    title: String,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
}

impl Navigation {
    fn payload(&self) -> EventPayload {
        EventPayload::Map(vec![
            ("url", PayloadValue::Str(self.url.clone())),
            ("title", PayloadValue::Str(self.title.clone())),
            ("loading", PayloadValue::Bool(self.loading)),
            ("canGoBack", PayloadValue::Bool(self.can_go_back)),
            ("canGoForward", PayloadValue::Bool(self.can_go_forward)),
        ])
    }
}

/// The navigation state of `webview`.
fn navigation(webview: &ICoreWebView2, loading: bool) -> Navigation {
    let mut url = PWSTR::null();
    let mut title = PWSTR::null();
    let (mut back, mut forward) = (BOOL(0), BOOL(0));
    // SAFETY: WebView2 getters on the live web view; the strings are freed
    // by `take_pwstr`.
    unsafe {
        let url = webview
            .Source(&mut url)
            .map(|()| take_pwstr(url))
            .unwrap_or_default();
        let title = webview
            .DocumentTitle(&mut title)
            .map(|()| take_pwstr(title))
            .unwrap_or_default();
        let _ = webview.CanGoBack(&mut back);
        let _ = webview.CanGoForward(&mut forward);
        Navigation {
            url,
            title,
            loading,
            can_go_back: back.as_bool(),
            can_go_forward: forward.as_bool(),
        }
    }
}

/// What a failed navigation reports, `None` for one that was replaced or
/// cancelled (not an error).
fn web_error(status: i32) -> Option<&'static str> {
    Some(match status {
        1 => "The certificate's common name does not match the host.",
        2 => "The certificate has expired.",
        3 => "The client certificate contains errors.",
        4 => "The certificate has been revoked.",
        5 => "The certificate is invalid.",
        6 => "The server is unreachable.",
        7 => "The connection timed out.",
        8 => "The server returned an invalid response.",
        9 => "The connection was aborted.",
        10 => "The connection was reset.",
        11 => "The internet connection was lost.",
        12 => "Cannot connect to the server.",
        13 => "The host name could not be resolved.",
        14 => return None,
        15 => "A redirect failed.",
        17 => "Authentication is required.",
        18 => "Proxy authentication is required.",
        _ => "The page could not be loaded.",
    })
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

/// `injectedJavaScriptBeforeContentLoaded`: in the main frame, before the
/// page's own scripts (WebView2 runs document scripts in every frame).
fn before_content(js: &str) -> String {
    format!("if (window === window.top) {{\n{js}\n}}")
}

/// `injectedJavaScript`: in the main frame, once the document is parsed.
fn after_content(js: &str) -> String {
    format!(
        "if (window === window.top) {{ (function () {{ var run = function () {{\n{js}\n}}; \
         if (document.readyState === 'loading') {{ document.addEventListener('DOMContentLoaded', run, {{ once: true }}); }} \
         else {{ run(); }} }})(); }}"
    )
}

/// Where web views of app `app_id` keep their profile.
fn profile_folder(app_id: &str) -> String {
    let base = std::env::var("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let app = if app_id.is_empty() { "nsh" } else { app_id };
    // An id is a reverse domain name: keep it a single, valid folder name.
    let app: String = app
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    base.join(app)
        .join("WebView2")
        .to_string_lossy()
        .into_owned()
}

/// Waiting for an environment.
type EnvCallback = Box<dyn FnOnce(Option<ICoreWebView2Environment>)>;

enum EnvState {
    Pending(Vec<EnvCallback>),
    Ready(ICoreWebView2Environment),
    Failed,
}

thread_local! {
    /// WebView2 environments by profile folder: one browser process
    /// serves every web view of a profile.
    static ENVIRONMENTS: RefCell<HashMap<String, EnvState>> = RefCell::default();
}

/// Calls `f` with the environment of profile `folder` once it is made
/// (`None` when it cannot be).
fn with_environment(folder: &str, f: impl FnOnce(Option<ICoreWebView2Environment>) + 'static) {
    let known = ENVIRONMENTS.with(|envs| match envs.borrow().get(folder) {
        Some(EnvState::Ready(env)) => Some(Some(env.clone())),
        Some(EnvState::Failed) => Some(None),
        _ => None,
    });
    if let Some(env) = known {
        return f(env);
    }
    let start = ENVIRONMENTS.with(|envs| {
        let mut envs = envs.borrow_mut();
        if let Some(EnvState::Pending(waiting)) = envs.get_mut(folder) {
            waiting.push(Box::new(f));
            false
        } else {
            envs.insert(folder.to_owned(), EnvState::Pending(vec![Box::new(f)]));
            true
        }
    });
    // Outside the borrow: a loader that fails at once calls back here.
    if start {
        start_environment(folder);
    }
}

/// Starts making the environment of profile `folder`.
fn start_environment(folder: &str) {
    let key = folder.to_owned();
    let handler =
        CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |result, env| {
            if let Err(err) = &result {
                log::warn!("web view: no WebView2 environment: {err}");
            }
            let env = result.ok().and(env);
            let waiting = ENVIRONMENTS.with(|envs| {
                let mut envs = envs.borrow_mut();
                let state = match &env {
                    Some(env) => EnvState::Ready(env.clone()),
                    None => EnvState::Failed,
                };
                match envs.insert(key.clone(), state) {
                    Some(EnvState::Pending(waiting)) => waiting,
                    _ => Vec::new(),
                }
            });
            for f in waiting {
                f(env.clone());
            }
            Ok(())
        }));
    let folder = HSTRING::from(folder);
    // SAFETY: valid strings; the handler is an owned COM object the loader
    // keeps until it calls it.
    let result = unsafe {
        create_environment(
            PCWSTR::null(),
            PCWSTR(folder.as_ptr()),
            std::ptr::null_mut(),
            windows::core::Interface::as_raw(&handler),
        )
    };
    if let Err(err) = result.ok() {
        log::warn!("web view: cannot start WebView2: {err}");
        let waiting = ENVIRONMENTS.with(|envs| {
            match envs
                .borrow_mut()
                .insert(folder.to_string(), EnvState::Failed)
            {
                Some(EnvState::Pending(waiting)) => waiting,
                _ => Vec::new(),
            }
        });
        for f in waiting {
            f(None);
        }
    }
}

/// The placeholder's text format, made once per thread.
fn placeholder_format() -> Option<IDWriteTextFormat> {
    thread_local! {
        static FORMAT: Option<IDWriteTextFormat> = {
            // SAFETY: plain DirectWrite calls with valid strings.
            unsafe {
                gfx::dwrite()
                    .CreateTextFormat(
                        &HSTRING::from("Segoe UI"),
                        None,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        14.0,
                        &HSTRING::from(""),
                    )
                    .ok()
                    .inspect(|f| {
                        let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                        let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                        let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
                    })
            }
        };
    }
    FORMAT.with(Clone::clone)
}

/// The [`EventKind::WebViewReply`] envelope of request `request`:
/// `{id, ok: true, value}` or `{id, ok: false, error}`.
fn reply_value(request: u64, result: Result<JsonValue, String>) -> JsonValue {
    match result {
        Ok(value) => serde_json::json!({"id": request, "ok": true, "value": value}),
        Err(error) => serde_json::json!({"id": request, "ok": false, "error": error}),
    }
}

/// Answers request `request` of `node` (an internal reply event, not a
/// listener's).
fn reply(surface: &Surface, node: NodeId, request: u64, result: Result<JsonValue, String>) {
    surface.event(
        node,
        EventKind::WebViewReply,
        EventPayload::Json(reply_value(request, result)),
    );
}

/// `onDownload`'s payload: `state`, `id`, `url`, `filename`, `path` and,
/// once finished, `bytes`; `message` when it failed.
fn download_payload(
    state: &str,
    id: &str,
    url: &str,
    filename: &str,
    path: &Path,
    bytes: Option<u64>,
    message: Option<String>,
) -> EventPayload {
    let mut fields = vec![
        ("state", PayloadValue::Str(state.to_owned())),
        ("id", PayloadValue::Str(id.to_owned())),
        ("url", PayloadValue::Str(url.to_owned())),
        ("filename", PayloadValue::Str(filename.to_owned())),
        (
            "path",
            PayloadValue::Str(path.to_string_lossy().into_owned()),
        ),
    ];
    if let Some(bytes) = bytes {
        fields.push(("bytes", PayloadValue::Number(bytes as f64)));
    }
    if let Some(message) = message {
        fields.push(("message", PayloadValue::Str(message)));
    }
    EventPayload::Map(fields)
}

/// A safe file name for a download: the server's suggestion, or the URL's
/// last segment when it has none.
fn download_name(suggested: &str, url: &str) -> String {
    let name = Path::new(suggested)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = if name.is_empty() {
        url_file_name(url).to_owned()
    } else {
        name
    };
    sanitize_file_name(&name)
}

/// A file name Windows accepts: its last path component without a
/// separator, control character or reserved character, no leading or
/// trailing dot/space, no reserved device name (with or without an
/// extension, and however the stem ends), at most 200 bytes; `download`
/// when nothing is left.
fn sanitize_file_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name: String = last
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*')
                && !matches!(
                    c,
                    '\u{061c}'
                        | '\u{200e}'..='\u{200f}'
                        | '\u{202a}'..='\u{202e}'
                        | '\u{2066}'..='\u{206f}'
                )
        })
        .collect::<String>()
        .trim()
        .trim_end_matches(['.', ' '])
        .to_owned();
    name = name.trim_start_matches('.').to_owned();
    if name.is_empty() {
        return "download".to_owned();
    }
    if is_reserved_name(&name) {
        name.insert(0, '_');
    }
    if name.len() > 200 {
        // Preserve a normal extension while shortening the stem on UTF-8
        // boundaries.
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| format!(".{ext}"))
            .filter(|ext| ext.len() < 200);
        if let Some(extension) = &extension {
            name.truncate(name.len() - extension.len());
        }
        let limit = 200 - extension.as_ref().map_or(0, String::len);
        while name.len() > limit {
            name.pop();
        }
        name = name.trim_end_matches(['.', ' ']).to_owned();
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

/// Whether `name` is a Windows device name, which no file (with or without
/// an extension) may be called: `CON`, `PRN`, `AUX`, `NUL`, `CONIN$`,
/// `CONOUT$`, the `COM`/`LPT` families (0 through 9, and the superscript
/// forms Windows also reserves). Win32 ignores trailing dots and spaces in
/// a name component, so `CON .txt` is `CON` too.
fn is_reserved_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    let family = |prefix: &str| {
        let digits = stem.strip_prefix(prefix).unwrap_or_default();
        matches!(
            digits,
            "0" | "1"
                | "2"
                | "3"
                | "4"
                | "5"
                | "6"
                | "7"
                | "8"
                | "9"
                | "\u{b9}"
                | "\u{b2}"
                | "\u{b3}"
        )
    };
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || family("COM")
        || family("LPT")
}

/// The last path segment of `url`, without its query or fragment.
fn url_file_name(url: &str) -> &str {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    url[..end].rsplit('/').next().unwrap_or_default()
}

/// An id for a download: a random GUID in lowercase hex, so the directory
/// it names (`downloads/<id>`) cannot collide with another transfer's or
/// leave the directory (it has no separators).
fn next_download_id() -> String {
    static FALLBACK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    // SAFETY: `CoCreateGuid` writes into a local GUID.
    match unsafe { CoCreateGuid() } {
        Ok(guid) => format!(
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            guid.data1,
            guid.data2,
            guid.data3,
            guid.data4[0],
            guid.data4[1],
            guid.data4[2],
            guid.data4[3],
            guid.data4[4],
            guid.data4[5],
            guid.data4[6],
            guid.data4[7]
        ),
        Err(_) => {
            // OLE is unavailable (it is not on this thread): a time and a
            // counter are still unique enough, and contain no separator.
            let n = FALLBACK.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            format!("{nanos:x}-{n:x}")
        }
    }
}

/// Takes a transfer's one terminal lifecycle event: a cancel racing a
/// completion reports `failed` once, and a later callback reports nothing.
fn take_terminal(terminal: &Cell<bool>) -> bool {
    !terminal.replace(true)
}

/// Publishes a finished Manual download's staging file at `path`:
/// `overwrite` replaces a file there atomically; otherwise a create-new
/// link publishes only when nothing is there (a file that appeared
/// meanwhile is left untouched and the publish fails).
fn publish_staged(staging: &Path, path: &Path, overwrite: bool) -> Result<(), String> {
    if overwrite {
        return std::fs::rename(staging, path)
            .map_err(|error| format!("cannot move the download to {}: {error}", path.display()));
    }
    match std::fs::hard_link(staging, path) {
        Ok(()) => {
            if std::fs::remove_file(staging).is_err() {
                remove_later(staging.to_path_buf(), false);
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
            "the download path already exists: {}",
            path.display()
        )),
        Err(error) => Err(format!(
            "cannot publish {} without replacing an existing file: {error}",
            path.display()
        )),
    }
}

/// Removes the output this backend owns (an automatic download's
/// directory, or a Manual download's staging file), never the app's
/// destination. `now`: try synchronously first (a cancellation answer
/// follows the deletion); otherwise retry while WebView2 releases its
/// handle.
fn remove_owned(directory: Option<&PathBuf>, staging: Option<&PathBuf>, now: bool) {
    if let Some(directory) = directory {
        if now && std::fs::remove_dir_all(directory).is_ok() {
            return;
        }
        remove_later(directory.clone(), true);
    } else if let Some(staging) = staging {
        if now {
            match std::fs::remove_file(staging) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => {}
            }
        }
        remove_later(staging.clone(), false);
    }
}

/// Whether a view whose requested manual downloads this runtime cannot
/// honor must refuse `command`: a navigation, or anything that executes
/// app script.
fn inert_refuses(command: &Command) -> bool {
    matches!(
        command,
        Command::Reload
            | Command::GoBack
            | Command::GoForward
            | Command::Load(_)
            | Command::InjectJavaScript(_)
            | Command::PostMessage(_)
    )
}

/// The unique sibling a Manual download writes into until it finishes:
/// beside the app's destination (same volume, so publishing is a link or
/// rename), named only by the transfer's id so nothing about the final
/// name can collide with another transfer.
fn staging_path(final_path: &Path, id: &str) -> Result<PathBuf, String> {
    let parent = final_path.parent().ok_or_else(|| {
        format!(
            "the download path has no directory: {}",
            final_path.display()
        )
    })?;
    let name = final_path
        .file_name()
        .ok_or_else(|| {
            format!(
                "the download path has no file name: {}",
                final_path.display()
            )
        })?
        .to_string_lossy();
    let first = format!(".nsh-download-{id}.part");
    let first_path = parent.join(&first);
    if !name.eq_ignore_ascii_case(&first) && !first_path.exists() {
        return Ok(first_path);
    }
    // The destination itself is the first candidate, or it is occupied: a
    // second sibling must be free and distinct (Windows compares fixed
    // names without regard to ASCII case).
    let second = format!(".nsh-download-{id}.part.1");
    if name.eq_ignore_ascii_case(&second) {
        return Err(format!(
            "cannot choose a temporary download path distinct from {}",
            final_path.display()
        ));
    }
    let second_path = parent.join(second);
    if second_path.exists() {
        return Err(format!(
            "the temporary download path already exists: {}",
            second_path.display()
        ));
    }
    Ok(second_path)
}

/// Why `path` is not a destination an accepted download can use: it must
/// be absolute, in an existing directory, not a directory, not taken
/// (unless `overwrite`), and not another running download's.
fn destination_error(path: &Path, overwrite: bool, active: &[PathBuf]) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("the download path is empty".to_owned());
    }
    if !path.is_absolute() {
        return Err(format!(
            "the download path is not absolute: {}",
            path.display()
        ));
    }
    if path.to_string_lossy().contains('\0') {
        return Err("the download path has a NUL".to_owned());
    }
    let Some(name) = path.file_name() else {
        return Err(format!(
            "the download path has no file name: {}",
            path.display()
        ));
    };
    if name.is_empty() {
        return Err("the download path has no file name".to_owned());
    }
    let Some(parent) = path.parent() else {
        return Err(format!(
            "the download directory does not exist: {}",
            path.display()
        ));
    };
    if !parent.is_dir() {
        return Err(format!(
            "the download directory does not exist: {}",
            parent.display()
        ));
    }
    if path.is_dir() {
        return Err(format!(
            "the download path is a directory: {}",
            path.display()
        ));
    }
    if !overwrite && path.exists() {
        return Err(format!(
            "the download path already exists: {}",
            path.display()
        ));
    }
    if active.iter().any(|active| same_path(active, path)) {
        return Err(format!("another download is writing to {}", path.display()));
    }
    Ok(())
}

/// Removes a file or directory this backend wrote, retrying briefly while
/// WebView2 closes its handle after a cancellation.
fn remove_later(path: PathBuf, directory: bool) {
    std::thread::spawn(move || {
        for attempt in 0..100 {
            let result = if directory {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match result {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error) if attempt == 99 => {
                    log::warn!(
                        "web view: cannot remove a cancelled download's output {}: {error}",
                        path.display()
                    );
                    return;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    });
}

/// Whether two paths name the same file (Windows treats `/` and `\` alike
/// and compares without regard to ASCII case).
fn same_path(a: &Path, b: &Path) -> bool {
    let normal = |path: &Path| {
        path.to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase()
    };
    normal(a) == normal(b)
}

/// Why a download was interrupted, for `onDownload`'s message.
fn interrupt_message(reason: i32) -> String {
    let message = match reason {
        1 => "the file could not be written",
        2 => "access to the file was denied",
        3 => "the disk is full",
        4 => "the file name is too long",
        5 => "the file is too large",
        6 => "the file was found to be malicious",
        7 => "the file had a transient error",
        8 => "the file was blocked by policy",
        9 => "the file failed a security check",
        10 => "the file was too short",
        11 => "the file's hash did not match",
        12 => "the network request failed",
        13 => "the network request timed out",
        14 => "the network connection was lost",
        15 => "the server is down",
        16 => "the network request was invalid",
        17 => "the server failed",
        18 => "the server does not support ranges",
        19 => "the server returned bad content",
        20 => "the server requires authentication",
        21 => "the server's certificate has a problem",
        22 => "the server forbade the download",
        23 => "the server sent an unexpected response",
        24 => "the content length did not match",
        25 => "the server redirected across origins",
        26 => "the download was cancelled",
        27 => "the app is shutting down",
        _ => "the download was interrupted",
    };
    message.to_owned()
}

#[cfg(test)]
mod tests {
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
    fn wraps_injected_scripts() {
        assert!(before_content("x()").starts_with("if (window === window.top) {\nx()"));
        let after = after_content("y()");
        assert!(after.contains("DOMContentLoaded") && after.contains("\ny()\n"));
    }

    #[test]
    fn makes_navigation_payloads() {
        let state = Navigation {
            url: "https://example.com/".into(),
            title: "Example".into(),
            loading: false,
            can_go_back: true,
            can_go_forward: false,
        };
        assert_eq!(
            state.payload(),
            EventPayload::Map(vec![
                ("url", PayloadValue::Str("https://example.com/".into())),
                ("title", PayloadValue::Str("Example".into())),
                ("loading", PayloadValue::Bool(false)),
                ("canGoBack", PayloadValue::Bool(true)),
                ("canGoForward", PayloadValue::Bool(false)),
            ])
        );
    }

    #[test]
    fn words_web_errors() {
        assert_eq!(web_error(13), Some("The host name could not be resolved."));
        assert_eq!(web_error(14), None);
        assert_eq!(web_error(99), Some("The page could not be loaded."));
    }

    #[test]
    fn names_profile_folders() {
        let folder = profile_folder("com.example.app");
        assert!(folder.ends_with(r"com.example.app\WebView2"), "{folder}");
        assert!(profile_folder("a:b").contains("a_b"));
    }

    #[test]
    fn matches_base_documents() {
        assert!(same_document("https://Example.com", "https://example.com/"));
        assert!(same_document(
            "https://example.com/a/b.html#top",
            "https://example.com/a/b.html"
        ));
        assert!(!same_document(
            "https://example.com/a/",
            "https://example.com/a/b.html"
        ));
        assert!(!same_document(
            "https://example.com/A",
            "https://example.com/a"
        ));
    }

    #[test]
    fn maps_buttons_to_mouse_events() {
        assert_eq!(
            button_event(0, true, 1),
            (COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN, 0)
        );
        assert_eq!(
            button_event(0, true, 2),
            (COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOUBLE_CLICK, 0)
        );
        assert_eq!(
            button_event(0, true, 3),
            (COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN, 0)
        );
        assert_eq!(
            button_event(1, false, 1),
            (COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP, 0)
        );
        assert_eq!(
            button_event(4, true, 1),
            (COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_DOWN, 2)
        );
    }

    #[test]
    fn styles_scrolling() {
        assert!(scroll_style(false).contains("var on = false"));
        assert!(scroll_style(true).contains("var on = true"));
        assert!(scroll_style(false).contains("overflow: hidden !important"));
    }
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsh-webview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        dir
    }

    #[test]
    fn sanitizes_download_file_names() {
        assert_eq!(sanitize_file_name(r"..\..\evil.txt"), "evil.txt");
        assert_eq!(sanitize_file_name(r"C:\dir\a:b?.txt"), "ab.txt");
        assert_eq!(sanitize_file_name("  ..  "), "download");
        assert_eq!(sanitize_file_name(""), "download");
        assert_eq!(sanitize_file_name("...hidden"), "hidden");
        assert_eq!(sanitize_file_name("trailing. "), "trailing");
        assert_eq!(sanitize_file_name("a. . . "), "a");
        assert_eq!(sanitize_file_name("报告.pdf"), "报告.pdf");
        assert_eq!(
            sanitize_file_name("invoice\u{202e}fdp.exe"),
            "invoicefdp.exe"
        );
        assert_eq!(sanitize_file_name("a\u{0}b\u{7}.txt"), "ab.txt");
    }

    #[test]
    fn sanitizes_reserved_device_names_but_not_ordinary_stems() {
        for reserved in [
            "CON",
            "con",
            "CON.txt",
            "PRN",
            "AUX",
            "NUL",
            "NUL.tar.gz",
            "COM0",
            "COM1",
            "COM9",
            "LPT0",
            "LPT9",
            "COM\u{b9}",
            "LPT\u{b3}",
            "CONIN$",
            "CONOUT$",
        ] {
            let name = sanitize_file_name(reserved);
            assert!(
                name.starts_with('_'),
                "{reserved:?} must be made distinct, got {name:?}"
            );
            assert!(!is_reserved_name(&name), "{name:?}");
        }
        // Trailing dots and spaces are ignored by Win32 in a name component.
        for disguised in ["CON .txt", "NUL. .txt", "PRN...", "AUX . ."] {
            let name = sanitize_file_name(disguised);
            assert!(name.starts_with('_'), "{disguised:?} -> {name:?}");
        }
        // Ordinary names that merely contain or resemble one stay as they are.
        for ordinary in ["console.txt", "connection", "COM10", "LPT10", "my-CON.txt"] {
            assert_eq!(sanitize_file_name(ordinary), ordinary);
            assert!(!is_reserved_name(ordinary));
        }
        // A long name keeps a fitting extension on UTF-8 boundaries.
        let long = format!("{}.txt", "é".repeat(300));
        let name = sanitize_file_name(&long);
        assert!(name.len() <= 200, "{}", name.len());
        assert!(name.ends_with(".txt"), "{name}");
    }

    #[test]
    fn names_downloads_from_the_url_when_the_server_suggests_nothing() {
        assert_eq!(
            download_name("", "https://example.test/files/report.pdf?token=1#page"),
            "report.pdf"
        );
        assert_eq!(download_name("", "https://example.test/"), "download");
        assert_eq!(
            download_name(r"C:\Users\a\Downloads\photo.webp", "https://example.test/"),
            "photo.webp"
        );
        assert_eq!(
            download_name("CON.txt", "https://example.test/"),
            "_CON.txt"
        );
    }

    #[test]
    fn validates_download_destinations() {
        let dir = scratch("destinations");
        let target = dir.join("report.pdf");
        assert!(destination_error(&target, false, &[]).is_ok());
        std::fs::write(&target, b"old").unwrap();
        assert!(destination_error(&target, false, &[]).is_err());
        assert!(destination_error(&target, true, &[]).is_ok());
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert!(destination_error(Path::new("relative.pdf"), true, &[]).is_err());
        assert!(destination_error(Path::new(""), true, &[]).is_err());
        assert!(destination_error(&dir.join("missing/report.pdf"), true, &[]).is_err());
        assert!(destination_error(&dir, true, &[]).is_err());
        assert!(destination_error(&target, true, &[target.clone()]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn publishes_staged_downloads_without_clobbering() {
        let dir = scratch("publish");
        let destination = dir.join("report.pdf");
        let staging = staging_path(&destination, "abc-123").unwrap();
        assert_eq!(staging.parent(), Some(dir.as_path()));
        assert_ne!(staging, destination);

        // A fresh destination takes the staged bytes.
        std::fs::write(&staging, b"new").unwrap();
        publish_staged(&staging, &destination, false).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert!(!staging.exists());

        // A file that appeared meanwhile is never replaced without
        // `overwrite`; the staging file stays for cleanup.
        std::fs::write(&staging, b"newer").unwrap();
        assert!(publish_staged(&staging, &destination, false).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert_eq!(std::fs::read(&staging).unwrap(), b"newer");
        remove_owned(None, Some(&staging), true);
        assert!(!staging.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");

        // With overwrite, the staged file replaces the destination.
        std::fs::write(&staging, b"replacement").unwrap();
        publish_staged(&staging, &destination, true).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"replacement");
        assert!(!staging.exists());

        // The destination may itself use the first staging name.
        let collision = dir.join(".nsh-download-collision.part");
        let separate = staging_path(&collision, "collision").unwrap();
        assert_ne!(separate, collision);
        std::fs::write(&separate, b"complete").unwrap();
        publish_staged(&separate, &collision, false).unwrap();
        assert_eq!(std::fs::read(&collision).unwrap(), b"complete");
        assert!(!separate.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleans_only_owned_output_never_an_existing_destination() {
        let dir = scratch("cleanup");
        let keep = dir.join("report.pdf");
        std::fs::write(&keep, b"completed").unwrap();
        let staging = dir.join(".nsh-download-id.part");
        std::fs::write(&staging, b"partial").unwrap();
        remove_owned(None, Some(&staging), true);
        assert!(!staging.exists());
        assert_eq!(std::fs::read(&keep).unwrap(), b"completed");

        // An automatic download owns its directory alone.
        let owned = dir.join("uuid");
        std::fs::create_dir_all(&owned).unwrap();
        std::fs::write(owned.join("report.pdf"), b"partial").unwrap();
        remove_owned(Some(&owned), None, true);
        assert!(!owned.exists());
        assert_eq!(std::fs::read(&keep).unwrap(), b"completed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn describes_download_interruptions() {
        assert_eq!(interrupt_message(26), "the download was cancelled");
        assert_eq!(interrupt_message(999), "the download was interrupted");
    }

    #[test]
    fn makes_download_payloads() {
        let payload = download_payload(
            "started",
            "id-1",
            "https://example.test/a.zip",
            "a.zip",
            Path::new(r"C:\temp\a.zip"),
            None,
            None,
        );
        let EventPayload::Map(fields) = payload else {
            panic!("a download event is a map");
        };
        let field = |name: &str| {
            fields
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(field("state"), Some(PayloadValue::Str("started".into())));
        assert_eq!(field("id"), Some(PayloadValue::Str("id-1".into())));
        assert_eq!(
            field("path"),
            Some(PayloadValue::Str(r"C:\temp\a.zip".into()))
        );
        assert_eq!(field("bytes"), None);

        let failed = download_payload(
            "failed",
            "id-1",
            "https://example.test/a.zip",
            "a.zip",
            Path::new(""),
            None,
            Some("no room".into()),
        );
        let EventPayload::Map(fields) = failed else {
            panic!("a download event is a map");
        };
        assert_eq!(
            fields
                .iter()
                .find(|(key, _)| *key == "message")
                .map(|(_, value)| value.clone()),
            Some(PayloadValue::Str("no room".into()))
        );
    }

    #[test]
    fn cancellation_terminal_is_reported_once() {
        let terminal = Cell::new(false);
        assert!(take_terminal(&terminal));
        assert!(!take_terminal(&terminal));
    }
}
