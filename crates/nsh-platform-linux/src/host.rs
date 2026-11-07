//! A host's second window on Linux: a GTK window with an app of its own.
//!
//! A host window runs its own [`AppSource`] on its own engine, never
//! sharing state with the main app; its geometry comes from
//! [`WindowOptions`] (the `kind` picks the look — a `Panel` is a utility
//! window hidden from the task bar — while [`AppOptions`] stays the
//! app's own metadata), and dropping it closes its window and stops its
//! app. A close button the user presses does the same: the window stops
//! its app once, and every method on it after that does nothing.
//!
//! It belongs to the thread that runs the app's GTK loop, where
//! [`run_app_with`](crate::run_app_with) runs its setup hook (GTK takes
//! the thread that first initializes it as its main thread for good);
//! [`HostWindow::open`] on another thread fails with
//! [`HostWindowError::WrongThread`]. The host's own code runs on its
//! own threads, so the hook's [`AppContext`](crate::AppContext) also
//! offers [`run`](crate::AppContext::run) to bring work back to the
//! app's thread later.
//!
//! GTK 4 limits what this backend honors: the `level` and the `position`
//! are no-ops (the stacking and the placing are the window manager's,
//! and the docks and panels of GNOME and KDE keep their own), and
//! `ParentCenter` opens centered as `Center` would. There is no
//! `skip-taskbar` in GTK 4 either: a `Panel` or `Popup` window stays out
//! of the task bar only where the window manager reads its own hints
//! (the utility type hints GTK 3 had are gone). The attributes v1
//! honors: `title`, `frame` (`None` is borderless), `resizable`,
//! `closable`, `close_on_press`, `close_on_blur` and the `kind`'s own
//! `taskbar`; [`HostWindow::set_attributes`] on an open window applies
//! the `title`, the `frame`, `resizable` and `closable`, silently
//! ignoring the rest.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::OnceLock,
    thread,
};

use gtk4::{self as gtk, glib, prelude::*};
use nsh_core::{
    desktop::{HostWindowError, WindowAttributes, WindowFrameStyle, WindowOptions},
    geometry::{Points, Size},
};

use crate::app::{AppOptions, AppSource, AppView};

/// The thread that runs the app's GTK loop, recorded as
/// [`run_app_with`](crate::run_app_with) starts: a host window belongs
/// to it.
pub(crate) static APP_THREAD: OnceLock<thread::ThreadId> = OnceLock::new();

/// Whether this thread runs the app's GTK loop.
pub(crate) fn in_app_thread() -> bool {
    APP_THREAD.get() == Some(&thread::current().id())
}

/// A host window: [`WindowOptions`] for the window, [`AppOptions`] for
/// the app it runs, and [`AppSource`] for the app itself.
///
/// Opened on the thread that runs the app's GTK loop, from
/// [`run_app_with`](crate::run_app_with)'s setup hook; dropped to close.
/// See the [module](self) docs for what GTK honors.
pub struct HostWindow {
    window: gtk::Window,
    state: Rc<HostState>,
    attributes: RefCell<WindowAttributes>,
}

/// What a host window keeps: its app, and whether it is over.
struct HostState {
    app: RefCell<Option<Rc<AppView>>>,
    hides_on_focus_loss: Cell<bool>,
    closed: Cell<bool>,
}

impl HostWindow {
    /// Opens a host window of `options` running `source` under `app`.
    ///
    /// The thread that calls this must be the app's own (the one
    /// [`run_app_with`](crate::run_app_with) runs its hook on), or the
    /// window is refused with [`HostWindowError::WrongThread`].
    pub fn open(
        options: WindowOptions,
        app: AppOptions,
        source: AppSource,
    ) -> Result<HostWindow, HostWindowError> {
        if !in_app_thread() {
            return Err(HostWindowError::WrongThread);
        }
        let attributes = WindowAttributes::resolve(options.kind, &options.attributes);
        let size = host_size(options.default_size);
        let mut view_options = app;
        view_options.size = size;
        let view = AppView::new(view_options);
        let window = gtk::Window::new();
        // The app's application, so the window lives and quits with it.
        if let Some(app) = gtk::Application::default() {
            window.set_application(Some(&app));
        }
        window.set_title(Some(attributes.title.as_deref().unwrap_or("nsh")));
        window.set_default_size(size.0.max(1), size.1.max(1));
        window.set_decorated(attributes.frame != Some(WindowFrameStyle::None));
        if let Some(resizable) = attributes.resizable {
            window.set_resizable(resizable);
        }
        if let Some(closable) = attributes.closable {
            window.set_deletable(closable);
        }
        window.set_child(Some(&view.widget()));
        let state = Rc::new(HostState {
            app: RefCell::new(Some(view.clone())),
            hides_on_focus_loss: Cell::new(attributes.close_on_blur.unwrap_or(false)),
            closed: Cell::new(false),
        });
        let closer = state.clone();
        window.connect_close_request(move |_| {
            // The user pressed the close button (a `closable` window has
            // one): the app stops once, and the window closes as asked.
            closer.stop();
            glib::Propagation::Proceed
        });
        // Losing the active window's place hides it, as `close_on_blur`
        // asks — the one blur GTK reports.
        let watcher = state.clone();
        let weak = window.downgrade();
        window.connect_is_active_notify(move |window| {
            if !window.is_active()
                && watcher.hides_on_focus_loss.get()
                && !watcher.closed.get()
                && let Some(window) = weak.upgrade()
            {
                window.set_visible(false);
            }
        });
        // However the window goes, its app stops with it, once.
        let keeper = state.clone();
        window.connect_destroy(move |_| keeper.stop());
        match source {
            AppSource::Rust(root) => view.load(nsh_engine::App::Rust(root)),
            AppSource::Bundle(bundle) => view.load(nsh_engine::App::Js(bundle)),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => crate::dev::start(&view, url),
        }
        crate::system::environment_applied(view.surface());
        Ok(HostWindow {
            window,
            state,
            attributes: RefCell::new(attributes),
        })
    }

    /// Shows the window and brings it to the front.
    pub fn show(&self) {
        if self.state.closed.get() {
            return;
        }
        self.window.present();
    }

    /// Hides the window. The window manager picks what the user sees
    /// next; GTK has no focus to hand back, as macOS does.
    pub fn hide(&self) {
        if self.state.closed.get() {
            return;
        }
        self.window.set_visible(false);
    }

    /// Whether the window is on screen.
    pub fn is_visible(&self) -> bool {
        !self.state.closed.get() && self.window.is_visible()
    }

    /// Applies `attributes` on the open window.
    ///
    /// The `title`, the `frame`, `resizable`, `closable` and
    /// `close_on_blur` take effect at once; the rest waits for a window
    /// that opens with it, as the module docs say.
    pub fn set_attributes(&self, attributes: &WindowAttributes) {
        let changed = {
            let stored = self.attributes.borrow();
            stored.diff(attributes)
        };
        let Some(changed) = changed else {
            return;
        };
        if let Some(title) = changed.title.as_ref() {
            self.window.set_title(Some(title));
        }
        if let Some(frame) = changed.frame {
            self.window.set_decorated(frame != WindowFrameStyle::None);
        }
        if let Some(resizable) = changed.resizable {
            self.window.set_resizable(resizable);
        }
        if let Some(closable) = changed.closable {
            self.window.set_deletable(closable);
        }
        if let Some(close_on_blur) = changed.close_on_blur {
            self.state.hides_on_focus_loss.set(close_on_blur);
        }
        self.attributes.borrow_mut().merge(attributes);
    }

    /// Closes the window and stops its app. Calling it again, or any
    /// other method, does nothing.
    pub fn close(self) {
        self.shut();
    }

    /// The one shutdown every path shares.
    fn shut(&self) {
        if self.state.closed.replace(true) {
            return;
        }
        if let Some(view) = self.state.app.borrow_mut().take() {
            view.stop();
        }
        self.window.destroy();
    }
}

impl Drop for HostWindow {
    fn drop(&mut self) {
        self.shut();
    }
}

impl HostState {
    /// Stops the app once: the close button, the window going away and
    /// the host's own `close` all land here.
    fn stop(&self) {
        if !self.closed.replace(true) {
            if let Some(view) = self.app.borrow_mut().take() {
                view.stop();
            }
        }
    }
}

/// A host window's size in pixels: its own, or seventy percent of the
/// primary monitor's.
fn host_size(size: Option<Size<Points>>) -> (i32, i32) {
    size.map_or_else(default_size, |size| {
        (size.width.0.round() as i32, size.height.0.round() as i32)
    })
}

/// Seventy percent of the primary monitor, or a desktop's own guess
/// where GTK knows none.
fn default_size() -> (i32, i32) {
    let Some(display) = gtk::gdk::Display::default() else {
        return (896, 560);
    };
    let monitors = display.monitors();
    let Some(monitor) = (0..monitors.n_items())
        .filter_map(|i| monitors.item(i).and_downcast::<gtk::gdk::Monitor>())
        .next()
    else {
        return (896, 560);
    };
    let geometry = monitor.geometry();
    (
        (geometry.width() as f32 * 0.7) as i32,
        (geometry.height() as f32 * 0.7) as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_window_is_not_opened_off_the_app_thread() {
        assert!(!in_app_thread());
    }

    #[test]
    fn a_host_window_takes_its_size_in_pixels() {
        let size = Size::new(Points(420.4), Points(300.6));
        assert_eq!(host_size(Some(size)), (420, 301));
    }
}
