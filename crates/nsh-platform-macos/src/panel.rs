//! Panel windows: a second window of the app, beside [`run_app`]'s.
//!
//! A panel is a window of its own, as a floating palette or a popover is:
//! it shows an [`AppSource`] of its own (a Rust root, a bundle or a dev
//! server), so it can run a different app than the main window, with an
//! engine of its own ([`AppView`]). It has no title bar by default, can
//! float above other apps' windows ([`PanelLevel`]) and can take the
//! keyboard without activating the app ([`PanelOptions::nonactivating`]).
//! A panel lives exactly as long as its [`Panel`] does: dropping it closes
//! the window and stops its app.
//!
//! [`Panel::open`] runs on the main thread, as [`run_app`] does. Rust
//! components run on the engine thread, so a panel is opened by the host's
//! main-thread code (as an app's runner does), not from a component's event
//! handler.
//!
//! A [`PanelOptions::position`] places the window's top-left corner from
//! the primary screen's top-left one, as [`WindowFrame`]'s `x` and `y` do.
//! [`PanelOptions::hides_on_resign_key`] listens for the window losing the
//! keyboard (`windowDidResignKey:`), the signal a nonactivating panel has.

use std::{cell::RefCell, rc::Rc};

use nsh_core::desktop::WindowFrame;
pub use nsh_core::desktop::{PanelError, PanelLevel, PanelOptions};
use nsh_core::geometry::{Points, Size};
use objc2::{
    MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBackingStoreType, NSColor, NSFloatingWindowLevel,
    NSNormalWindowLevel, NSPanel, NSResponder, NSRunningApplication, NSScreen, NSWindow,
    NSWindowDelegate, NSWindowLevel, NSWindowStyleMask, NSWorkspace,
};
use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::{App, AppOptions, AppSource, AppView};

/// A panel window (see the [module](self)'s documentation): its own window
/// and app, closed and stopped when it is dropped. Drop it on the main
/// thread, where it was opened.
///
/// [`Panel::open`] must run on the main thread, as `run_app` does (Rust
/// components run on the engine thread). The JS side's `Window.openPanel`
/// needs a `DesktopCommand` and a hop back to the main thread, which is
/// future work: a panel is opened from Rust only for now.
///
/// The `app`'s [`AppOptions`] are the app's own, not the window's: its
/// `data_dir`, `limits` and `components` come from the host, as they do for
/// the [main window](run_app)'s app, and every app keeps its data in
/// `<data_dir>/<app id>` ([`AppOptions::data_dir`]), so apps of different
/// ids stay apart.
///
/// The window's title is [`PanelOptions::title`]: the `app`'s
/// [`AppOptions::title`] names the [main window](run_app), which a panel
/// does not have.
///
/// The `app`'s calls and posts reach the host on the engine thread
/// ([`AppOptions::host`]): a host that shows, hides or closes a panel from
/// one of them hands it back to the main thread first (a main-queue
/// dispatch), as windows belong to it.
pub struct Panel {
    window: Retained<PanelWindow>,
    app: Rc<AppView>,
    /// The app in front before the panel showed, to give it the focus back.
    previous: RefCell<Option<Retained<NSRunningApplication>>>,
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
        let (width, height) = panel_size(mtm, options.size)?;
        let mut style = if options.borderless {
            NSWindowStyleMask::Borderless
        } else {
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
        };
        if options.nonactivating {
            // Only an NSPanel takes this mask: the window does not activate
            // the app when it is clicked.
            style |= NSWindowStyleMask::NonactivatingPanel;
        }
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
        // SAFETY: NSPanel's designated initializer; the alloc is the panel's
        // class and the style is a panel's.
        let window: Retained<PanelWindow> = unsafe {
            msg_send![
                PanelWindow::alloc(mtm),
                initWithContentRect: frame,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        };
        // SAFETY: the panel keeps the window in `self.window` until it closes
        // it (or is dropped), so AppKit must not release it on close.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setOpaque(!options.transparent);
        let background = if options.transparent {
            NSColor::clearColor()
        } else {
            NSColor::windowBackgroundColor()
        };
        window.setBackgroundColor(Some(&background));
        window.setHasShadow(true);
        window.setMovableByWindowBackground(options.movable_by_background);
        window.setHidesOnDeactivate(options.hides_on_deactivate);
        window.setLevel(level_of(options.level));
        window.setTitle(&NSString::from_str(&options.title));
        crate::system::open_window(&window, app.appearance);
        let view = AppView::new(mtm, app);
        if options.transparent {
            // The window is see-through; its content surface must be too
            // (`system::follow_background` keeps a transparent background
            // unless the app paints an opaque one).
            view.set_transparent();
        }
        window.setContentView(Some(&view.view()));
        window.center();
        if let Some((x, y)) = options.position {
            // The same conversion `WindowFrame`'s `x` and `y` take: the
            // window's top-left corner, from the primary screen's.
            crate::chrome::set_frame(
                &window,
                &WindowFrame {
                    x: Some(x),
                    y: Some(y),
                    width: None,
                    height: None,
                    animated: Some(false),
                },
            );
        }
        if options.hides_on_resign_key {
            // AppKit does not retain a window's delegate: the panel keeps
            // the window (its own delegate) alive for as long as it may be
            // called.
            window.setDelegate(Some(ProtocolObject::from_ref(&*window)));
        }
        match source {
            AppSource::Rust(root) => view.load(App::Rust(root)),
            AppSource::Bundle(bundle) => view.load(App::Js(bundle)),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => crate::dev::start(&view, url),
        }
        window.makeFirstResponder(Some(view.surface().root.ns()));
        Ok(Self {
            window,
            app: view,
            previous: RefCell::new(None),
        })
    }

    /// Shows the window in front of the windows at its level, and makes it
    /// the key window (typed into) without activating the app.
    pub fn show(&self) {
        // Remember the app in front, so `hide` gives the focus back to it.
        *self.previous.borrow_mut() = NSWorkspace::sharedWorkspace().frontmostApplication();
        self.window.makeKeyAndOrderFront(None);
    }

    /// Hides the window, giving the focus back to the app that had it before
    /// the panel showed (when it is still running).
    pub fn hide(&self) {
        self.dismiss();
    }

    /// Takes the window off screen and gives the focus back to the app that
    /// had it before the panel showed. `hide`, `close` and dropping the
    /// panel all end here, so closing a nonactivating panel leaves the front
    /// app where hiding it does.
    fn dismiss(&self) {
        self.window.orderOut(None);
        if let Some(previous) = self.previous.borrow_mut().take() {
            // The app that had the focus before the panel showed takes it
            // back; when the app itself had it, this keeps it there.
            let _ = previous.activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }

    /// Whether the window is on screen.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.window.isVisible()
    }

    /// Moves and sizes the window, as [`WindowFrame`] does for the main
    /// window: a field left out keeps its value.
    pub fn set_frame(&self, frame: WindowFrame) {
        crate::chrome::set_frame(&self.window, &frame);
    }

    /// Changes how the window floats (see [`PanelLevel`]).
    pub fn set_level(&self, level: PanelLevel) {
        self.window.setLevel(level_of(level));
    }

    /// Sets the window title (shown when the panel has a title bar).
    pub fn set_title(&self, title: &str) {
        self.window.setTitle(&NSString::from_str(title));
    }

    /// Closes the window and stops its app, giving the focus back to the app
    /// that had it before the panel showed (as [`hide`](Self::hide) does).
    pub fn close(self) {
        self.dismiss();
        self.window.close();
        self.app.stop();
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        self.dismiss();
        self.app.stop();
    }
}

/// The panel's content size: 70% of the main screen when not given.
fn panel_size(mtm: MainThreadMarker, size: Option<Size<Points>>) -> Result<(f64, f64), PanelError> {
    match size {
        Some(size) if size.width.0 > 0.0 && size.height.0 > 0.0 => {
            Ok((f64::from(size.width.0), f64::from(size.height.0)))
        }
        Some(_) => Err(PanelError::Invalid("a panel's size must be positive")),
        None => {
            let visible = NSScreen::mainScreen(mtm)
                .map(|screen| screen.visibleFrame())
                .unwrap_or_else(|| NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1280.0, 800.0)));
            Ok((visible.size.width * 0.7, visible.size.height * 0.7))
        }
    }
}

/// The window level of a [`PanelLevel`].
fn level_of(level: PanelLevel) -> NSWindowLevel {
    match level {
        PanelLevel::Normal => NSNormalWindowLevel,
        PanelLevel::Floating => NSFloatingWindowLevel,
        // Just above the Dock (the Dock's level is not a window level of
        // AppKit's: the window server's key gives it, and one more is above
        // it).
        PanelLevel::AboveDock => {
            NSWindowLevel::try_from(CGWindowLevelForKey(CGWindowLevelKey::DockWindowLevelKey) + 1)
                .unwrap_or(NSFloatingWindowLevel)
        }
    }
}

define_class!(
    /// A panel window whose borderless (and nonactivating) style still takes
    /// the keyboard: AppKit's `NSPanel` takes it only with a title bar.
    #[unsafe(super(NSPanel, NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
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
        // A panel that hides on resign loses the keyboard to another window:
        // take it off screen, without touching the app in front (the user
        // chose it).
        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            self.orderOut(None);
        }
    }
);

#[cfg(test)]
mod tests {
    use super::{PanelLevel, level_of};

    #[test]
    fn levels_stack_normal_floating_above_dock() {
        let normal = level_of(PanelLevel::Normal);
        let floating = level_of(PanelLevel::Floating);
        let above_dock = level_of(PanelLevel::AboveDock);
        assert!(normal < floating);
        assert!(
            floating < above_dock,
            "AboveDock is over the other floating windows, below menus"
        );
    }
}
