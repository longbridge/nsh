//! The app's own windows besides its main one (`Window.open`): each shows
//! a dialog root (a `modal` node) of the same engine in a `gtk::Window` of
//! its own instead of over the main window. The engine, the JS runtime and
//! the core stay one, so the windows share the app's state; this module
//! only hosts the root view, reports its size, routes its pointer input
//! and carries out the desktop commands addressed to it.

use std::rc::Rc;

use gtk4::{self as gtk, glib, prelude::*};
use nsh_core::{
    Bounds, EdgeInsets, EventKind, EventPayload, NodeId, Points, Size,
    desktop::{DesktopCommand, WindowFrameStyle, WindowOptions},
};

use crate::{surface::Surface, view::NshView};

/// A window of the app's own.
pub(crate) struct AppWindow {
    window: gtk::Window,
    /// The size last reported to the core.
    reported: Option<Size<Points>>,
}

/// Whether dialog root `id` is shown in a window of its own.
pub(crate) fn is_window(surface: &Surface, id: NodeId) -> bool {
    surface.windows.borrow().contains_key(&id)
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
    let Some(window) = surface.windows.borrow().get(&id).map(|w| w.window.clone()) else {
        return log::debug!("window: no window for {id}");
    };
    match command {
        DesktopCommand::SetTitle(title) => window.set_title(Some(title)),
        DesktopCommand::Minimize => window.minimize(),
        DesktopCommand::ToggleMaximize => {
            if window.is_maximized() {
                window.unmaximize();
            } else {
                window.maximize();
            }
        }
        DesktopCommand::ToggleFullScreen => window.set_fullscreened(!window.is_fullscreen()),
        DesktopCommand::SetFrame(frame) => {
            let (width, height) = window.default_size();
            window.set_default_size(
                frame.width.map_or(width, |v| v.0.round() as i32),
                frame.height.map_or(height, |v| v.0.round() as i32),
            );
        }
        DesktopCommand::OrderFront => window.present(),
        DesktopCommand::Hide => window.set_visible(false),
        // The app closes it by removing its root (`handle.close()`).
        DesktopCommand::Close => window.close(),
        // GTK 4 has no window levels; menus are the main window's.
        _ => {}
    }
}

/// Shows dialog root `id` in a new window.
fn open(surface: &Rc<Surface>, view: &NshView, options: &WindowOptions) {
    let id = view.id();
    if is_window(surface, id) {
        return;
    }
    // Presented over the main window by its insertion, earlier in the
    // batch: it leaves the surface's modals.
    crate::navigation::release(surface, id);
    if view.parent().is_some() {
        view.unparent();
    }
    let window = gtk::Window::builder()
        .title(options.title.as_deref().unwrap_or_default())
        .decorated(options.frame != WindowFrameStyle::None)
        .build();
    let main = surface.root.root().and_downcast::<gtk::Window>();
    if let Some(app) = main.as_ref().and_then(|w| w.application()) {
        window.set_application(Some(&app));
    }
    if options.title.is_none()
        && let Some(title) = main.as_ref().and_then(|w| w.title())
    {
        window.set_title(Some(&title));
    }
    let (width, height) = options.size.map_or((480, 360), |s| {
        (s.width.0.round() as i32, s.height.0.round() as i32)
    });
    window.set_default_size(width.max(1), height.max(1));
    view.set_hexpand(true);
    view.set_vexpand(true);
    window.set_child(Some(view));
    let weak = Rc::downgrade(surface);
    let resized = view.downgrade();
    view.on_resize(move |width, height| {
        if let (Some(surface), Some(view)) = (weak.upgrade(), resized.upgrade()) {
            resize(&surface, &view, width, height);
        }
    });
    crate::input::install_on(surface, view, Some(view));
    // Closed by the user: the app hears `Dismiss` and removes the root
    // (`handle.close()`), which destroys the window; it hides meanwhile.
    let weak = Rc::downgrade(surface);
    window.connect_close_request(move |window| {
        if let Some(surface) = weak.upgrade()
            && is_window(&surface, id)
        {
            window.set_visible(false);
            surface.event(id, EventKind::Dismiss, EventPayload::None);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    // The main window takes its windows with it.
    if let Some(main) = &main {
        let weak = window.downgrade();
        main.connect_destroy(move |_| {
            if let Some(window) = weak.upgrade() {
                window.destroy();
            }
        });
    }
    surface.windows.borrow_mut().insert(
        id,
        AppWindow {
            window: window.clone(),
            reported: None,
        },
    );
    window.present();
}

/// The window's root view was given `width` × `height`: the core lays the
/// root out at that size.
fn resize(surface: &Surface, view: &NshView, width: f32, height: f32) {
    let id = view.id();
    let size = Size::new(Points(width), Points(height));
    {
        let mut windows = surface.windows.borrow_mut();
        let Some(w) = windows.get_mut(&id) else {
            return;
        };
        if w.reported == Some(size) {
            return;
        }
        w.reported = Some(size);
    }
    let frame = Bounds::new(nsh_core::geometry::point(Points(0.0), Points(0.0)), size);
    if std::mem::replace(&mut view.state_mut().frame, frame) != frame {
        view.queue_allocate();
    }
    surface.control(|c| c.set_root_size(id, size, EdgeInsets::default()));
}

/// Dialog root `view` was removed: its window goes. Whether it had one.
pub(crate) fn removed(surface: &Surface, view: &NshView) -> bool {
    let Some(w) = surface.windows.borrow_mut().remove(&view.id()) else {
        return false;
    };
    w.window.set_child(None::<&gtk::Widget>);
    w.window.destroy();
    true
}
