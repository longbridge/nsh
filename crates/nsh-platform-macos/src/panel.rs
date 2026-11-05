//! Panel windows: a second window of the app, beside [`run_app`](crate::run_app)'s.
//!
//! A panel is a window of its own, as a floating palette or a popover is:
//! it shows an [`AppSource`] of its own (a Rust root, a bundle or a dev
//! server), so it can run a different app than the main window, with an
//! engine of its own ([`AppView`]). It has no title bar by default and can
//! float above other apps' windows ([`WindowLevel`]).
//!
//! A panel is opened on the thread that runs the app's message loop: the
//! main thread, where [`run_app`](crate::run_app) runs, from the setup hook of
//! [`run_app_with`](crate::run_app_with), which hands the host the app's
//! [`AppContext`](crate::AppContext); panels are opened with
//! [`AppContext::open_panel`](crate::AppContext::open_panel). That hook
//! belongs to the host, which keeps the panels it opens for as long as the
//! app runs; [`Panel::open`] anywhere else fails with
//! [`PanelError::WrongThread`]. Rust components run on the engine thread,
//! so a panel is opened by the host's main-thread code (as an app's runner
//! does), not from a component's event handler.
//!
//! A panel lives exactly as long as its [`Panel`] does: dropping it closes
//! the window and stops its app. A decorated panel the user closes (its
//! close button) stops its app too, and the panel does not touch the window
//! again.
//!
//! A [`PanelOptions::position`] places the window's top-left corner from
//! the primary screen's top-left one, as [`WindowFrame`]'s `x` and `y` do.
//! [`PanelOptions::hides_on_focus_loss`] hides the window when it loses the
//! keyboard's focus (`windowDidResignKey:`): a click on another window or
//! app takes it off screen.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub use nsh_core::desktop::{PanelError, PanelOptions, WindowLevel};
use nsh_core::{
    desktop::WindowFrame,
    geometry::{Points, Size},
};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject,
};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBackingStoreType, NSFloatingWindowLevel, NSNormalWindowLevel,
    NSPanel, NSResponder, NSRunningApplication, NSScreen, NSWindow, NSWindowDelegate,
    NSWindowLevel, NSWindowStyleMask, NSWorkspace,
};
use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey, kCGDesktopIconWindowLevel};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::{App, AppOptions, AppSource, AppView};

/// A panel window (see the [module](self)'s documentation): its own window
/// and app, closed and stopped when it is dropped. Open and drop it on the
/// main thread, where the app's event loop runs.
///
/// A panel whose window closed (its close button, or the app's
/// `Window.close`) stops its app, and the panel does nothing more to the
/// window: showing it, hiding it or changing its frame has no effect.
///
/// [`Panel::open`] must run on the main thread, as `run_app` does (Rust
/// components run on the engine thread). A host opens its panels from the
/// setup hook of [`run_app_with`](crate::run_app_with)
/// ([`AppContext::open_panel`](crate::AppContext::open_panel)) and keeps them
/// for as long as the app runs. The JS side's `Window.openPanel`
/// needs a `DesktopCommand` and a hop back to the main thread, which is
/// future work: a panel is opened from Rust only for now.
///
/// The `app`'s [`AppOptions`] are the app's own, not the window's: its
/// `data_dir`, `limits` and `components` come from the host, as they do for
/// the [main window](crate::run_app)'s app, and every app keeps its data in
/// `<data_dir>/<app id>` ([`AppOptions::data_dir`]), so apps of different
/// ids stay apart.
///
/// The window's title is [`PanelOptions::title`]: the `app`'s
/// [`AppOptions::title`] names the [main window](crate::run_app), which a
/// panel does not have.
///
/// The `app`'s calls and posts reach the host on the engine thread
/// ([`AppOptions::host`]): a host that shows, hides or closes a panel from
/// one of them hands it back to the main thread first (a main-queue
/// dispatch), as windows belong to it.
pub struct Panel {
    window: Retained<PanelWindow>,
    /// What the window's delegate and the panel share.
    state: Rc<PanelState>,
}

/// What a panel's window, its delegate and the [`Panel`] share: the app it
/// shows, the app in front before it showed, and whether it closed.
///
/// The window keeps it in its ivars (`objc2` 0.6), so the window (its own
/// delegate) acts on the same state as the panel without owning it: the
/// panel owns the window, and a window that owned the panel would never go.
struct PanelState {
    /// The app the window shows, stopped when the window closes.
    app: Rc<AppView>,
    /// The app in front before the panel showed, to give the focus back.
    previous: RefCell<Option<Retained<NSRunningApplication>>>,
    /// [`PanelOptions::hides_on_focus_loss`]: the window can be its own
    /// delegate for the close button too, so it reads the option here.
    hides_on_focus_loss: bool,
    /// The window closed: `close`, the window's own close button and `Drop`
    /// end here, once.
    closed: Cell<bool>,
}

impl PanelState {
    /// Closes the window and stops its app, once: [`Panel::close`] and
    /// dropping the panel both end here (the window's own close button stops
    /// the app too, in [`window_will_close`](Self::window_will_close)).
    fn close_window(&self, window: &PanelWindow) {
        if self.closed.replace(true) {
            return;
        }
        // Take the focus back to the app that had it before the panel
        // showed, as `hide` does.
        self.return_focus();
        window.close();
        self.app.stop();
    }

    /// The user closed the window (`windowWillClose:`): stop the app and
    /// forget the app in front, without activating it (closing the window is
    /// not a request to switch back to it). AppKit owns the window from here
    /// on: nothing of the panel touches it again.
    fn window_will_close(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.previous.borrow_mut().take();
        self.app.stop();
    }

    /// Takes the window off screen and gives the focus back to the app that
    /// had it before the panel showed.
    fn hide(&self, window: &PanelWindow) {
        if self.closed.get() {
            return;
        }
        window.orderOut(None);
        self.return_focus();
    }

    /// Gives the focus back to the app that had it before the panel showed
    /// (when it is still running).
    fn return_focus(&self) {
        if let Some(previous) = self.previous.borrow_mut().take() {
            // The app that had the focus before the panel showed takes it
            // back; when the app itself had it, this keeps it there.
            let _ = previous.activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }
}

impl Panel {
    /// Opens a panel window showing `source` with `options`, without
    /// ordering it in front: call [`show`](Self::show) to show it.
    ///
    /// # Errors
    ///
    /// [`PanelError::WrongThread`] off the main thread, and
    /// [`PanelError::Invalid`] for a size that is not positive.
    pub fn open(
        options: PanelOptions,
        app: AppOptions,
        source: AppSource,
    ) -> Result<Self, PanelError> {
        let mtm = MainThreadMarker::new().ok_or(PanelError::WrongThread)?;
        let visible = NSScreen::mainScreen(mtm).map(|screen| screen.visibleFrame());
        let (width, height) = panel_size(options.size, visible)?;
        let style = if options.borderless {
            NSWindowStyleMask::Borderless
        } else {
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
        };
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
        // The app first: the panel's state, shared with the window, stops it
        // when the window closes.
        let appearance = app.appearance;
        let view = AppView::new(mtm, app);
        let state = Rc::new(PanelState {
            app: view.clone(),
            previous: RefCell::new(None),
            hides_on_focus_loss: options.hides_on_focus_loss,
            closed: Cell::new(false),
        });
        // SAFETY: NSPanel's designated initializer; the alloc is the panel's
        // class, the style is a panel's, and its ivars are set.
        let window: Retained<PanelWindow> = unsafe {
            msg_send![
                super(PanelWindow::alloc(mtm).set_ivars(state.clone())),
                initWithContentRect: frame,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        };
        // SAFETY: the panel keeps the window in `self.window` until it closes
        // it (or is dropped), so AppKit must not release it on close.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setHasShadow(true);
        window.setLevel(level_of(options.level));
        window.setTitle(&NSString::from_str(&options.title));
        crate::system::open_window(&window, appearance);
        window.setContentView(Some(&view.view()));
        window.center();
        if let Some(position) = options.position {
            // The same conversion `WindowFrame`'s `x` and `y` take: the
            // window's top-left corner, from the primary screen's.
            crate::chrome::set_frame(
                &window,
                &WindowFrame {
                    x: Some(position.x),
                    y: Some(position.y),
                    width: None,
                    height: None,
                    animated: Some(false),
                },
            );
        }
        // AppKit does not retain a window's delegate: the panel keeps the
        // window (its own delegate) alive for as long as it may be called.
        // The delegate is always the window, for the close button's
        // `windowWillClose:` as much as for `windowDidResignKey:`.
        window.setDelegate(Some(ProtocolObject::from_ref(&*window)));
        match source {
            AppSource::Rust(root) => view.load(App::Rust(root)),
            AppSource::Bundle(bundle) => view.load(App::Js(bundle)),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => crate::dev::start(&view, url),
        }
        window.makeFirstResponder(Some(view.surface().root.ns()));
        Ok(Self { window, state })
    }

    /// Shows the window in front of the windows at its level, and makes it
    /// the key window (typed into) without activating the app.
    pub fn show(&self) {
        if self.state.closed.get() {
            return;
        }
        // Remember the app in front, so `hide` gives the focus back to it.
        *self.state.previous.borrow_mut() = NSWorkspace::sharedWorkspace().frontmostApplication();
        self.window.makeKeyAndOrderFront(None);
    }

    /// Hides the window, giving the focus back to the app that had it before
    /// the panel showed (when it is still running).
    pub fn hide(&self) {
        self.state.hide(&self.window);
    }

    /// Whether the window is on screen.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        !self.state.closed.get() && self.window.isVisible()
    }

    /// Moves and sizes the window, as [`WindowFrame`] does for the main
    /// window: a field left out keeps its value.
    pub fn set_frame(&self, frame: WindowFrame) {
        if !self.state.closed.get() {
            crate::chrome::set_frame(&self.window, &frame);
        }
    }

    /// Changes how the window floats (see [`WindowLevel`]).
    pub fn set_level(&self, level: WindowLevel) {
        if !self.state.closed.get() {
            self.window.setLevel(level_of(level));
        }
    }

    /// Sets the window title (shown when the panel has a title bar).
    pub fn set_title(&self, title: &str) {
        if !self.state.closed.get() {
            self.window.setTitle(&NSString::from_str(title));
        }
    }

    /// Closes the window and stops its app, giving the focus back to the app
    /// that had it before the panel showed (as [`hide`](Self::hide) does).
    pub fn close(self) {
        self.state.close_window(&self.window);
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        // A window left open would keep the app (and AppKit's loop) alive.
        self.state.close_window(&self.window);
    }
}

/// The panel's content size: 70% of `visible` (the main screen's visible
/// frame) when not given.
fn panel_size(
    size: Option<Size<Points>>,
    visible: Option<NSRect>,
) -> Result<(f64, f64), PanelError> {
    match size {
        Some(size) if size.width.0 > 0.0 && size.height.0 > 0.0 => {
            Ok((f64::from(size.width.0), f64::from(size.height.0)))
        }
        Some(_) => Err(PanelError::Invalid("a panel's size must be positive")),
        None => {
            let visible = visible
                .unwrap_or_else(|| NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1280.0, 800.0)));
            Ok((visible.size.width * 0.7, visible.size.height * 0.7))
        }
    }
}

/// The window level of a [`WindowLevel`]: a panel's
/// ([`PanelOptions::level`]) and the [main window](crate::run_app)'s
/// (`Window.setLevel`) both take it.
pub(crate) fn level_of(level: WindowLevel) -> NSWindowLevel {
    match level {
        WindowLevel::Normal => NSNormalWindowLevel,
        WindowLevel::Floating => NSFloatingWindowLevel,
        // Just above the Dock (the Dock's level is not a window level of
        // AppKit's: the window server's key gives it, and one more is above
        // it).
        WindowLevel::AboveDock => {
            NSWindowLevel::try_from(CGWindowLevelForKey(CGWindowLevelKey::DockWindowLevelKey) + 1)
                .unwrap_or(NSFloatingWindowLevel)
        }
        // The desktop widget's level, under every window, as the main
        // window's `Window.setLevel('desktopIcon')` takes it (`chrome.rs`).
        WindowLevel::DesktopIcon => kCGDesktopIconWindowLevel as isize,
        // A level of a newer nsh-core than this backend: it falls back to
        // the floating band, as `WindowLevel`'s documentation says.
        _ => NSFloatingWindowLevel,
    }
}

define_class!(
    /// A panel window whose borderless style still takes the keyboard:
    /// AppKit's `NSPanel` takes it only with a title bar.
    #[unsafe(super(NSPanel, NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Rc<PanelState>]
    #[name = "NshPanelWindow"]
    struct PanelWindow;

    unsafe impl NSObjectProtocol for PanelWindow {}

    impl PanelWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }

        // A panel is never the app's main window: the main window keeps its
        // title bar and its menu bar.
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool {
            false
        }
    }

    unsafe impl NSWindowDelegate for PanelWindow {
        // The window lost the keyboard: the user moved on to another window
        // or app. A panel that hides on focus loss goes off screen, without
        // touching the app in front (the user chose it); every panel forgets
        // the app that had the focus before it showed, so a later hide or
        // drop does not activate it.
        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            let state = self.ivars();
            if state.closed.get() {
                return;
            }
            state.previous.borrow_mut().take();
            if state.hides_on_focus_loss {
                self.orderOut(None);
            }
        }

        // The user closed the window (a decorated panel's close button):
        // stop its app, and forget the app in front without activating it.
        // AppKit owns the window from here on: the panel does not touch it.
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            self.ivars().window_will_close();
        }
    }
);

#[cfg(test)]
mod tests {
    use nsh_core::geometry::{Points, Size};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    use super::{PanelError, WindowLevel, level_of, panel_size};

    #[test]
    fn levels_stack_from_the_desktop_to_above_the_dock() {
        let normal = level_of(WindowLevel::Normal);
        let floating = level_of(WindowLevel::Floating);
        let above_dock = level_of(WindowLevel::AboveDock);
        let desktop_icon = level_of(WindowLevel::DesktopIcon);
        assert!(normal < floating);
        assert!(
            floating < above_dock,
            "AboveDock is over the other floating windows, below menus"
        );
        assert!(
            desktop_icon < normal,
            "the desktop icon level is under every window, as the main window's is"
        );
    }

    #[test]
    fn a_panel_takes_its_size_or_seventy_percent_of_the_screen() {
        let visible = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1000.0, 500.0));
        let size = Size::new(Points(420.0), Points(300.0));
        assert_eq!(panel_size(Some(size), Some(visible)), Ok((420.0, 300.0)));
        let (width, height) = panel_size(None, Some(visible)).expect("a default size");
        assert!((width - 700.0).abs() < 1e-9 && (height - 350.0).abs() < 1e-9);
        // No screen to measure (no display): 70% of a default one.
        let (width, height) = panel_size(None, None).expect("a default size");
        assert!((width - 1280.0 * 0.7).abs() < 1e-9 && (height - 800.0 * 0.7).abs() < 1e-9);
        for size in [
            Size::new(Points(0.0), Points(300.0)),
            Size::new(Points(420.0), Points(0.0)),
            Size::new(Points(-1.0), Points(-1.0)),
        ] {
            assert!(
                matches!(
                    panel_size(Some(size), Some(visible)),
                    Err(PanelError::Invalid(_))
                ),
                "{size:?} is not a size"
            );
        }
    }
}
