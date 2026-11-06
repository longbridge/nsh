//! The app's own windows besides its main one (`Window.open`): each shows
//! a dialog root (a `modal` node) of the same engine in an `NSWindow` of its
//! own instead of over the main window. The engine, the JS runtime and the
//! core stay one, so the windows share the app's state; this module only
//! hosts the root view, reports its size, routes its input and carries out
//! the desktop commands addressed to it.
//!
//! - The root view leaves the main window's modals
//!   ([`crate::navigation::release`]) and becomes the window's content
//!   view.
//! - Its events go to the surface with the view as the root they are
//!   picked in ([`crate::view::NshView::set_window_of`],
//!   [`crate::input::enter`]).
//! - Its size is reported to the core as the root's
//!   (`EngineControl::set_root_size`), which lays the root out at it.
//! - Closing it hides it and tells the app (`Dismiss`), which removes the
//!   root; the window goes with it ([`removed`]), and with the main window
//!   ([`close_all`], when the surface stops).

use std::{cell::Cell, rc::Rc};

use nsh_core::{
    Bounds, EdgeInsets, EventKind, EventPayload, NodeId, Points, Size,
    desktop::{DesktopCommand, WindowFrameStyle, WindowLevel, WindowOptions},
    geometry::point,
};
use objc2::{
    DefinedClass, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{Bool, ProtocolObject},
};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSScreen, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use crate::{surface::Surface, view::NshView};

/// A window of the app's own.
pub(crate) struct AppWindow {
    window: Retained<NSWindow>,
    /// The window's delegate (AppKit holds it weakly).
    _delegate: Retained<WindowDelegate>,
    /// The dialog root it shows.
    view: NshView,
    /// The size last reported to the core.
    reported: Cell<Option<Size<Points>>>,
}

/// Whether dialog root `id` is shown in a window of its own.
pub(crate) fn is_window(surface: &Surface, id: NodeId) -> bool {
    surface
        .windows
        .try_borrow()
        .is_ok_and(|windows| windows.contains_key(&id))
}

/// The window showing dialog root `id`.
fn window_of(surface: &Surface, id: NodeId) -> Option<Retained<NSWindow>> {
    surface.windows.borrow().get(&id).map(|w| w.window.clone())
}

/// A desktop command addressed to dialog root `id`.
pub(crate) fn command(
    surface: &Rc<Surface>,
    id: NodeId,
    view: Option<&NshView>,
    command: &DesktopCommand,
) {
    if let DesktopCommand::OpenWindow(options) = command {
        let Some(view) = view else {
            return log::warn!("window: no view {id}");
        };
        return open(surface, view, options);
    }
    let Some(window) = window_of(surface, id) else {
        return log::debug!("window: no window for {id}");
    };
    match command {
        DesktopCommand::SetTitle(title) => window.setTitle(&NSString::from_str(title)),
        DesktopCommand::Minimize => window.miniaturize(None),
        DesktopCommand::ToggleMaximize => window.zoom(None),
        DesktopCommand::ToggleFullScreen => window.toggleFullScreen(None),
        DesktopCommand::SetFrame(frame) => crate::chrome::set_frame(&window, frame),
        DesktopCommand::SetLevel(level) => crate::chrome::set_level(&window, *level),
        DesktopCommand::OrderFront => {
            window.deminiaturize(None);
            // activate() first: ordering a window front in an inactive app
            // leaves it behind the front app's windows.
            NSApplication::sharedApplication(surface.mtm).activate();
            window.makeKeyAndOrderFront(None);
        }
        DesktopCommand::Hide => window.orderOut(None),
        // The app closes it by removing its root (`handle.close()`); a
        // close asked for here is the user's (`Dismiss`).
        DesktopCommand::Close => window.performClose(None),
        // Menus and the title bar's style are the main window's.
        _ => {}
    }
}

/// Shows dialog root `view` in a new window.
fn open(surface: &Rc<Surface>, view: &NshView, options: &WindowOptions) {
    let id = view.id();
    if is_window(surface, id) {
        return;
    }
    // Presented over the main window by an insertion earlier in the batch:
    // it leaves the surface's modals.
    crate::navigation::release(surface, id);
    if view.has_parent() {
        view.unparent();
    }
    let mtm = surface.mtm;
    let size = options.size.map_or(NSSize::new(480.0, 360.0), |s| {
        NSSize::new(
            f64::from(s.width.0.max(1.0)),
            f64::from(s.height.0.max(1.0)),
        )
    });
    let style = match options.frame {
        WindowFrameStyle::None => NSWindowStyleMask::Borderless | NSWindowStyleMask::Resizable,
        _ => {
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
                | NSWindowStyleMask::Resizable
        }
    };
    // SAFETY: NSWindow's designated initializer.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), size),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is kept in `Surface::windows` until it goes.
    unsafe { window.setReleasedWhenClosed(false) };
    let main = surface.root.ns().window();
    let title = match &options.title {
        Some(title) => NSString::from_str(title),
        None => main.as_ref().map_or_else(NSString::new, |w| w.title()),
    };
    window.setTitle(&title);
    let delegate: Retained<WindowDelegate> = {
        let d = WindowDelegate::alloc(mtm).set_ivars((surface.key(), id));
        // SAFETY: NSObject's initializer.
        unsafe { msg_send![super(d), init] }
    };
    window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    crate::system::open_window(&window, surface.appearance.get());
    let top_left = match options.position {
        // NSH's origin is the top left, down from the primary screen's top;
        // Cocoa's goes up from its bottom.
        Some(p) => {
            let primary = NSScreen::screens(mtm)
                .firstObject()
                .map_or(0.0, |s| s.frame().size.height);
            NSPoint::new(f64::from(p.x.0), primary - f64::from(p.y.0))
        }
        // Cascaded from the main window's top left.
        None => {
            let step = 24.0 * (surface.windows.borrow().len() % 8 + 1) as f64;
            main.as_ref().map_or(NSPoint::new(step, step), |main| {
                let f = main.frame();
                NSPoint::new(f.origin.x + step, f.origin.y + f.size.height - step)
            })
        }
    };
    window.setFrameTopLeftPoint(top_left);
    if options.level != WindowLevel::Normal {
        crate::chrome::set_level(&window, options.level);
    }
    surface.windows.borrow_mut().insert(
        id,
        AppWindow {
            window: window.clone(),
            _delegate: delegate,
            view: view.clone(),
            reported: Cell::new(None),
        },
    );
    // The view's own place is the window's: the core only sizes it.
    view.state_mut().frame = Bounds::new(
        point(Points(0.0), Points(0.0)),
        Size::new(Points(size.width as f32), Points(size.height as f32)),
    );
    view.set_window_of(Rc::downgrade(surface));
    window.setContentView(Some(view.ns()));
    resized(surface, view);
    window.makeKeyAndOrderFront(None);
    window.makeFirstResponder(Some(view.ns()));
}

/// The root view of a window of the app's own was resized (it is the
/// window's content view): the core lays the root out at its size.
pub(crate) fn resized(surface: &Surface, view: &NshView) {
    let id = view.id();
    let ns = view.ns().frame().size;
    let size = Size::new(Points(ns.width as f32), Points(ns.height as f32));
    {
        let windows = surface.windows.borrow();
        let Some(w) = windows.get(&id) else {
            return;
        };
        if w.reported.replace(Some(size)) == Some(size) {
            return;
        }
    }
    view.state_mut().frame = Bounds::new(point(Points(0.0), Points(0.0)), size);
    surface.control(|c| c.set_root_size(id, size, EdgeInsets::default()));
}

/// Dialog root `view` was removed: its window goes. Whether it had one.
pub(crate) fn removed(surface: &Surface, view: &NshView) -> bool {
    let Some(w) = surface.windows.borrow_mut().remove(&view.id()) else {
        return false;
    };
    close(w);
    true
}

/// The surface stops (its main window closed, or its app ended): its
/// windows go with it.
pub(crate) fn close_all(surface: &Surface) {
    let windows: Vec<AppWindow> = surface
        .windows
        .borrow_mut()
        .drain()
        .map(|(_, w)| w)
        .collect();
    for w in windows {
        close(w);
    }
}

fn close(w: AppWindow) {
    w.view.set_window_of(std::rc::Weak::new());
    w.window.setDelegate(None);
    w.window.orderOut(None);
    w.window.setContentView(None);
    w.window.close();
}

define_class!(
    /// The delegate of a window of the app's own: the user's close asks the
    /// app (`Dismiss`), which removes the root.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = (u64, NodeId)]
    #[name = "NshOwnWindowDelegate"]
    struct WindowDelegate;

    unsafe impl NSObjectProtocol for WindowDelegate {}

    unsafe impl NSWindowDelegate for WindowDelegate {
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, window: &NSWindow) -> Bool {
            let (key, id) = *self.ivars();
            let Some(surface) = Surface::with_key(key) else {
                return Bool::YES;
            };
            if !is_window(&surface, id) {
                return Bool::YES;
            }
            // Hidden until the app removes the root (`handle.close()`),
            // which destroys it.
            window.orderOut(None);
            surface.event(id, EventKind::Dismiss, EventPayload::None);
            Bool::NO
        }
    }
);
