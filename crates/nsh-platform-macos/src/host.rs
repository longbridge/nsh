//! A host window: the host's second window, running an app of its own.
//!
//! The standalone runner's window is `run_app`'s ([`crate::run_app`]); a
//! host window is opened beside it by the host itself, from the setup hook
//! of [`crate::run_app_with`] through the [`crate::AppContext`] it is
//! handed. It shows one [`AppSource`] — Rust code, a bundle, a dev server —
//! with an engine of its own: its [`AppView`](crate::AppView) keeps a
//! surface and an engine apart from the main window's, so the two apps'
//! engines never share state. Rust components run on that engine's thread,
//! not the main thread; opening a window is main-thread work, so the host
//! opens it in the hook (before the event loop takes the thread) or brings
//! the work back with [`AppContext::run`](crate::AppContext::run). Opened
//! anywhere else, [`HostWindow::open`] answers
//! [`HostWindowError::WrongThread`].
//!
//! The geometry comes from the [`WindowOptions`] the host passes — its
//! `kind` picks the window's style (a [`WindowKind::Panel`] gets the
//! utility look, no taskbar entry and no activation by default), its
//! `default_size` and `default_position` place it — while the
//! [`AppOptions`](crate::AppOptions) of the app it runs stay that app's
//! metadata (its name, its appearance).
//!
//! A host window lives exactly as long as its [`HostWindow`]: dropping it
//! (or [`HostWindow::close`]) closes the window and stops the app; the
//! window's own close button does the same; everything after is a no-op.
//! When the window took the focus from another app (it was opened — or
//! shown — activating), closing or hiding it hands the focus back to the
//! app that had it. Losing the focus to a window outside itself, with
//! `close_on_blur` set, only hides it ([`HostWindow::hide`]); showing it
//! again is the host's call.
//!
//! The attributes v1 honors: the title, the frame (`frame: None` for a
//! borderless window), the buttons (`resizable`, `minimizable`,
//! `closable`), `focusable`, `activate`, the `level` the window floats at,
//! `close_on_press`, `close_on_blur`, `taskbar` and `transparent`. The
//! rest of [`WindowAttributes`] — the parent, the modality, the display,
//! the modes, the bounds, `title_bar`, the size limits, `movable`,
//! `workspaces`, `pointer_events`, the backdrop, the shadow, the color
//! scheme, the content protection, the progress, the edited mark, the
//! represented file, the tab group — is a follow-up: of what
//! [`HostWindow::set_attributes`] is handed, only the title and the level
//! change a window that is already open (the rest is set when it opens),
//! and nothing else in it errors.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub use nsh_core::desktop::HostWindowError;
use nsh_core::{
    desktop::{WindowAttributes, WindowKind, WindowOptions, WindowPosition},
    geometry::{Points, Size},
};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject,
};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSAutoresizingMaskOptions, NSPanel, NSRunningApplication,
    NSScreen, NSView, NSWindow, NSWindowDelegate, NSWorkspace,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::app::{AppOptions, AppSource, AppView};

/// A host window: the host's second window, running an app of its own (see
/// the [module](self) doc). Not `Send` or `Sync`: an AppKit window is
/// main-thread state, and the [`AppView`](crate::AppView) it holds keeps
/// the engine that runs its app.
///
/// Dropped while open, it closes its window and stops its app: a window
/// left open would keep the app (and AppKit's loop) alive.
pub struct HostWindow {
    /// The window, kept until it is closed (which AppKit must not do on
    /// its own: `setReleasedWhenClosed(false)` in `make_window`).
    window: Retained<NSWindow>,
    /// The state shared with the window's delegate: the app that stops
    /// when the window closes, the app to hand the focus back to, the
    /// closed mark that makes every later method a no-op.
    state: Rc<HostState>,
    /// The attributes the window opened with, resolved against its kind;
    /// [`HostWindow::set_attributes`] diffs against them.
    attributes: RefCell<WindowAttributes>,
}

/// The state a [`HostWindow`] shares with its window's delegate. The
/// window keeps it in its ivars, so the delegate acts on the same state as
/// the handle without owning it: the handle owns the window, and a window
/// that owned its handle would never go.
pub(crate) struct HostState {
    /// The app the window shows; stopped when the window closes.
    app: Rc<AppView>,
    /// The app that had the focus when this window (activating) took it:
    /// handed the focus back when the window hides or closes. Forgotten
    /// the moment the window loses the focus: giving the focus back then
    /// would switch to an app the user left behind.
    previous: RefCell<Option<Retained<NSRunningApplication>>>,
    /// Whether the window takes the focus when it shows (`activate`).
    activate: Cell<bool>,
    /// Whether the window hides when it loses the focus to a window
    /// outside itself (`close_on_blur`).
    close_on_blur: Cell<bool>,
    /// Whether the window has closed (or is closing): every later method
    /// of the handle is a no-op.
    closed: Cell<bool>,
    /// The window, for the delegate's hops back to the main queue (it has
    /// no `self` to keep there: AppKit does not retain a window's
    /// delegate).
    window: RefCell<Option<Retained<NSWindow>>>,
}

impl HostState {
    /// The window, while it is open.
    fn window(&self) -> Option<Retained<NSWindow>> {
        self.window.borrow().clone()
    }
    /// Closes `window` and stops its app, once: [`HostWindow::close`] and
    /// dropping the handle both end here, and so does the window's close
    /// button ([`windowShouldClose:`](HostWindowDelegate)).
    fn close_window(&self, window: &NSWindow) {
        if self.closed.replace(true) {
            return;
        }
        self.return_focus();
        window.close();
        self.app.stop();
    }

    /// The window closed with its own button, or AppKit tore it down: the
    /// mark that makes everything later a no-op, and the app stops. The
    /// app in front is forgotten without activating it — closing the
    /// window is not a request to switch back to it.
    fn window_will_close(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.previous.borrow_mut().take();
        self.app.stop();
    }

    /// Hides `window`, handing the focus back to the app that had it when
    /// this window took it.
    fn hide(&self, window: &NSWindow) {
        if self.closed.get() {
            return;
        }
        window.orderOut(None);
        self.return_focus();
    }

    /// Hands the focus back to the app that had it when this window
    /// (activating) took it; when the app itself had it, this keeps it
    /// there.
    fn return_focus(&self) {
        if let Some(previous) = self.previous.borrow_mut().take() {
            let _ = previous.activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }
}

impl HostWindow {
    /// Opens a host window running `source` with an engine of its own, as
    /// `options` places it and `app` configures its app. Must be called on
    /// the app's thread (the one that runs AppKit's message loop).
    ///
    /// # Errors
    ///
    /// [`HostWindowError::WrongThread`] off the app's thread;
    /// [`HostWindowError::Invalid`] when `options.default_size` is not
    /// positive.
    pub fn open(
        options: WindowOptions,
        app: AppOptions,
        source: AppSource,
    ) -> Result<Self, HostWindowError> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(HostWindowError::WrongThread);
        };
        let visible = NSScreen::mainScreen(mtm).map(|screen| screen.visibleFrame());
        let (width, height) = host_size(options.default_size, visible)?;
        // `resolve` fills what the kind defaults and the host left out, so
        // every field the window opens with is set here.
        let attributes = WindowAttributes::resolve(options.kind, &options.attributes);
        let style = crate::window::style_mask(&crate::window::StyleInputs {
            kind: options.kind,
            frame: attributes.frame.unwrap_or_default(),
            title_bar_hidden: false,
            resizable: attributes.resizable.unwrap_or(true),
            minimizable: attributes.minimizable.unwrap_or_default(),
            closable: attributes.closable.unwrap_or(true),
            activate: attributes.activate.unwrap_or(true),
        });
        // The app first: the window's state, shared with its delegate,
        // stops it when the window closes.
        let view = AppView::new(mtm, app.clone());
        let state = Rc::new(HostState {
            app: view.clone(),
            previous: RefCell::new(None),
            activate: Cell::new(attributes.activate.unwrap_or(true)),
            close_on_blur: Cell::new(attributes.close_on_blur.unwrap_or_default()),
            closed: Cell::new(false),
            window: RefCell::new(None),
        });
        let window = crate::window::make_window(
            mtm,
            options.kind,
            style,
            NSSize::new(width, height),
            attributes.focusable.unwrap_or(true),
        );
        // A panel stays when the app is not in front.
        if let Some(panel) = window.downcast_ref::<NSPanel>() {
            panel.setHidesOnDeactivate(false);
            panel.setFloatingPanel(false);
            panel.setBecomesKeyOnlyIfNeeded(options.kind == WindowKind::Overlay);
        }
        let delegate: Retained<HostWindowDelegate> = {
            let d = HostWindowDelegate::alloc(mtm).set_ivars(state.clone());
            // SAFETY: NSObject's initializer.
            unsafe { msg_send![super(d), init] }
        };
        debug_assert!(std::rc::Rc::ptr_eq(delegate.ivars(), &state));
        window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        if let Some(title) = &attributes.title {
            window.setTitle(&NSString::from_str(title));
        }
        window.setLevel(crate::window::ns_level(
            attributes.level.unwrap_or_default(),
        ));
        crate::system::open_window(&window, app.appearance);
        // The container dance of `open_window`: the view fills the window,
        // resized with it, and the root holds the keyboard (nothing is
        // focused until the user or the app focuses it).
        let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
        let container = NSView::initWithFrame(NSView::alloc(mtm), content);
        container.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        container.setWantsLayer(true);
        view.view().setFrame(content);
        view.view().setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        container.addSubview(&view.view());
        window.setContentView(Some(&container));
        window.setInitialFirstResponder(Some(&view.view()));
        window.makeFirstResponder(Some(&view.view()));
        place(&window, options.default_position, mtm);
        match source {
            AppSource::Rust(root) => view.load(nsh_engine::App::Rust(root)),
            AppSource::Bundle(bundle) => view.load(nsh_engine::App::Js(bundle)),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => crate::dev::start(&view, url),
        }
        // Shown as its `activate` says: activating, it takes the focus
        // (and remembers whose to give back); not, it comes up in front
        // without taking it — the default for a panel.
        if attributes.activate.unwrap_or(true) {
            *state.previous.borrow_mut() = NSWorkspace::sharedWorkspace().frontmostApplication();
            window.makeKeyAndOrderFront(None);
        } else {
            window.orderFrontRegardless();
            window.makeFirstResponder(Some(&view.view()));
        }
        *state.window.borrow_mut() = Some(window.clone());
        Ok(Self {
            window,
            state,
            attributes: RefCell::new(attributes),
        })
    }

    /// Shows the window. When it activates (its `activate`), it takes the
    /// focus — remembering the app that had it, to hand it back when it
    /// hides or closes; not, it comes up in front without taking it. A
    /// no-op once closed.
    pub fn show(&self) {
        if self.state.closed.get() {
            return;
        }
        if self.state.activate.get() {
            *self.state.previous.borrow_mut() =
                NSWorkspace::sharedWorkspace().frontmostApplication();
            self.window.makeKeyAndOrderFront(None);
        } else {
            self.window.orderFrontRegardless();
        }
    }

    /// Hides the window, handing the focus back to the app that had it
    /// when this window took it. A no-op once closed.
    pub fn hide(&self) {
        self.state.hide(&self.window);
    }

    /// Whether the window is on screen. Always `false` once closed.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        !self.state.closed.get() && self.window.isVisible()
    }

    /// Applies the attributes a window that is already open takes: the
    /// title, the level and `close_on_blur`. Everything else — the
    /// geometry, the style, the buttons, whether the window activates — is
    /// set when the window opens, and what this is handed of it waits for a
    /// window that opens with it. Fields v1 does not honor are ignored,
    /// silently: the [`WindowAttributes`] a host window is opened with is
    /// the wider surface, and a window that is already open is narrower.
    pub fn set_attributes(&self, attrs: &WindowAttributes) {
        let mut stored = self.attributes.borrow_mut();
        let Some(changed) = stored.diff(attrs) else {
            return;
        };
        if let Some(title) = &changed.title {
            self.window.setTitle(&NSString::from_str(title));
        }
        if let Some(level) = changed.level {
            self.window.setLevel(crate::window::ns_level(level));
        }
        if let Some(close_on_blur) = changed.close_on_blur {
            self.state.close_on_blur.set(close_on_blur);
        }
        stored.merge(attrs);
    }

    /// Closes the window and stops its app. Every later method of the
    /// handle is a no-op.
    pub fn close(self) {
        self.state.close_window(&self.window);
    }
}

impl Drop for HostWindow {
    fn drop(&mut self) {
        self.state.close_window(&self.window);
    }
}

/// Places `window` as `position` says: `At` from the top-left of the
/// primary screen (y flipped: the top of the screen is where a window's
/// frame is measured from), `Center` on the primary screen, and the
/// default — no position — centered by AppKit.
fn place(window: &NSWindow, position: Option<WindowPosition>, mtm: MainThreadMarker) {
    match position {
        Some(WindowPosition::At(p)) => {
            let primary = crate::window_state::primary_height(mtm);
            window.setFrameTopLeftPoint(NSPoint::new(f64::from(p.x.0), primary - f64::from(p.y.0)));
        }
        Some(WindowPosition::Center | WindowPosition::ParentCenter) => {
            if let Some(screen) = NSScreen::mainScreen(mtm) {
                crate::window_state::center_on(window, &screen);
            }
        }
        None => window.center(),
    }
}

/// The size a host window opens with: the one it is given, or seventy
/// percent of the visible area of the primary screen — a window beside the
/// app's own leaves its user a way past it.
fn host_size(
    size: Option<Size<Points>>,
    visible: Option<NSRect>,
) -> Result<(f64, f64), HostWindowError> {
    match size {
        Some(size) => {
            let (width, height) = (f64::from(size.width.0), f64::from(size.height.0));
            if width > 0.0 && height > 0.0 {
                Ok((width, height))
            } else {
                Err(HostWindowError::Invalid(
                    "a host window's size must be positive",
                ))
            }
        }
        None => {
            let frame = visible.unwrap_or(NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(1280.0, 800.0),
            ));
            Ok((frame.size.width * 0.7, frame.size.height * 0.7))
        }
    }
}

/// A window carried across a hop to the main queue — main thread to main
/// thread. [`Retained<NSWindow>`] is not `Send` (AppKit windows are
/// main-thread state), so it goes as the raw pointer it is, taken back on
/// the same thread it left.
struct SameThread(*mut NSWindow);

impl SameThread {
    /// Carries `window`'s pointer.
    fn new(window: Retained<NSWindow>) -> Self {
        Self(Retained::into_raw(window))
    }

    /// Takes the window back. SAFETY: the pointer was retained on this
    /// (the main) thread, and is only consumed here.
    unsafe fn take(self) -> Retained<NSWindow> {
        unsafe { Retained::from_raw(self.0).expect("the pointer was retained") }
    }
}

// SAFETY: the pointer never leaves the main thread: both ends of every
// hop it takes are the main queue.
unsafe impl Send for SameThread {}

define_class!(
    /// The delegate of a host window: AppKit's asks of the window — the
    /// close button's press, the focus it lost, the teardown — answered
    /// from the [`HostState`] the window's handle shares with it. Kept
    /// alive by the handle (AppKit does not retain a window's delegate).
    #[unsafe(super(NSObject,))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Rc<HostState>]
    #[name = "NshHostWindowDelegate"]
    pub(crate) struct HostWindowDelegate;

    unsafe impl NSObjectProtocol for HostWindowDelegate {}

    unsafe impl NSWindowDelegate for HostWindowDelegate {
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, window: &NSWindow) -> objc2::runtime::Bool {
            self.ivars().close_window(window);
            objc2::runtime::Bool::YES
        }

        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            self.ivars().window_will_close();
        }

        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            let state = self.ivars();
            if state.closed.get() {
                return;
            }
            // Every host window forgets the app that had the focus before
            // it showed, the moment it loses the focus: a later hide or
            // drop does not activate an app the user left behind.
            state.previous.borrow_mut().take();
            if state.close_on_blur.get() {
                // Hide the window once AppKit has picked the next key
                // window: hiding it before that would hand the focus to
                // whoever is under it, not the app the user switched to.
                // The window goes as a raw pointer — main queue to main
                // queue, the same thread both sides — re-retained there,
                // which `setReleasedWhenClosed(false)` keeps valid even
                // once closed (the handle may already be gone).
                let window = state.window().map(SameThread::new);
                dispatch2::DispatchQueue::main().exec_async(move || {
                    if let Some(window) = window.map(|w| unsafe { w.take() }) {
                        window.orderOut(None);
                    }
                });
            }
        }
    }
);

#[cfg(test)]
mod tests {
    use nsh_core::geometry::{px, size};

    use super::*;

    /// A host window takes the size it is given, or seventy percent of the
    /// visible screen.
    #[test]
    fn a_host_window_takes_its_size_or_seventy_percent_of_the_screen() {
        let visible = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1000.0, 500.0));
        assert_eq!(
            host_size(Some(size(px(400.0), px(300.0))), Some(visible)),
            Ok((400.0, 300.0))
        );
        assert_eq!(host_size(None, Some(visible)), Ok((700.0, 350.0)));
        // No screen (a headless test, say): a window still opens.
        assert_eq!(host_size(None, None), Ok((1280.0 * 0.7, 800.0 * 0.7)));
        assert_eq!(
            host_size(Some(size(px(0.0), px(300.0))), Some(visible)),
            Err(HostWindowError::Invalid(
                "a host window's size must be positive"
            ))
        );
        assert_eq!(
            host_size(Some(size(px(-1.0), px(1.0))), Some(visible)),
            Err(HostWindowError::Invalid(
                "a host window's size must be positive"
            ))
        );
    }

    /// The attributes a host window that is already open takes: the diff
    /// carries what changed, and the window applies the title, the level
    /// and `close_on_blur` of it — the style is set when it opens.
    #[test]
    fn a_host_window_applies_the_attributes_it_honors() {
        // What opening sets, as `resolve` leaves it: every field of the
        // kind's defaults.
        let mut own = WindowAttributes::default();
        own.title = Some("Tools".into());
        let stored = WindowAttributes::resolve(WindowKind::Panel, &own);
        let mut next = stored.clone();
        next.title = Some("Panels".into());
        next.level = Some(nsh_core::desktop::WindowLevel::AboveDock);
        next.resizable = Some(false);
        next.close_on_blur = Some(true);
        let changed = stored.diff(&next).expect("four fields change");
        assert_eq!(changed.title.as_deref(), Some("Panels"));
        assert_eq!(
            changed.level,
            Some(nsh_core::desktop::WindowLevel::AboveDock)
        );
        assert_eq!(changed.close_on_blur, Some(true));
        // The diff carries it, but a window that is already open does not
        // take a style change.
        assert_eq!(changed.resizable, Some(false));
        let mut applied = stored.clone();
        applied.merge(&changed);
        assert_eq!(applied.title.as_deref(), Some("Panels"));
        assert_eq!(
            applied.level,
            Some(nsh_core::desktop::WindowLevel::AboveDock)
        );
        assert!(stored.diff(&stored).is_none());
    }
}
