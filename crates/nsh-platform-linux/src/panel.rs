//! Panel windows: a second window of the app, beside [`run_app`]'s.
//!
//! A panel is a window of its own, as a floating palette or a popover is:
//! it shows an [`AppSource`] of its own (a Rust root, a bundle or a dev
//! server), so it can run a different app than the main window, with an
//! engine of its own ([`AppView`]). It has no title bar by default and can
//! be transparent. A panel lives exactly as long as its [`Panel`] does:
//! dropping it closes the window and stops its app.
//!
//! [`Panel::open`] runs on the thread that runs the GTK main loop, as
//! [`run_app`] does: windows of one main loop share it, so the host opens a
//! panel and keeps running. Rust components run on the engine thread, so a
//! panel is opened by the host's main-thread code (as an app's runner does),
//! not from a component's event handler.
//!
//! GTK 4 has no panel concept, and several options have no equivalent:
//!
//! - [`PanelOptions::level`]: GTK 4 removed the API to keep a window above
//!   others ([`PanelLevel::Floating`] and [`PanelLevel::AboveDock`] were
//!   `gtk_window_set_keep_above` in GTK 3), and the compositor stacks
//!   windows: the level is recorded but has no effect.
//! - [`PanelOptions::nonactivating`]: GTK 4 has no "take the keyboard
//!   without activating the app"; the panel is a normal window.
//! - [`PanelOptions::hides_on_deactivate`]: GTK 4 cannot observe the app's
//!   deactivation (there is no per-application activation), so the panel
//!   hides when it loses the focus instead: the closest signal it has.
//!   [`PanelOptions::hides_on_resign_key`] asks for the same signal, and the
//!   two hide the window the same way.
//! - [`PanelOptions::position`]: GTK 4 has no window positioning (Wayland
//!   forbids it and X11's is not exposed), so it is ignored and the
//!   compositor places the window.
//! - [`PanelOptions::movable_by_background`]: a GTK window moves by its
//!   title bar when it is decorated; there is no client-side equivalent.
//!
//! [`Panel::set_frame`] sizes the window; GTK 4 has no window positioning
//! (Wayland forbids it and X11's is not exposed), so `x` and `y` are
//! ignored. [`Panel::hide`] hides the window: the compositor picks the
//! window to focus next, as there is no API to hand the focus back to the
//! window that was in front.

use std::{cell::Cell, rc::Rc};

use gtk4::{self as gtk, prelude::*};
pub use nsh_core::desktop::{PanelError, PanelLevel, PanelOptions};
use nsh_core::{
    desktop::WindowFrame,
    geometry::{Points, Size},
};
use nsh_engine::App;

use crate::{AppOptions, AppSource, AppView};

/// A panel window (see the [module](self)'s documentation): its own window
/// and app, closed and stopped when it is dropped. Drop it on the thread
/// that runs the GTK main loop, where it was opened.
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
/// one of them hands it back to the main thread first, as GTK's windows
/// belong to it.
pub struct Panel {
    window: gtk::Window,
    app: Rc<AppView>,
    /// Whether [`Panel::close`] already closed the window, so [`Drop`] does
    /// not close it again.
    closed: Cell<bool>,
}

impl Panel {
    /// Opens a panel window showing `source` with `options`, without
    /// showing it: call [`show`](Self::show) to show it.
    ///
    /// # Errors
    ///
    /// [`PanelError::Invalid`] when the panel's size is not positive.
    pub fn open(
        options: PanelOptions,
        app: AppOptions,
        source: AppSource,
    ) -> Result<Self, PanelError> {
        let (width, height) = panel_size(options.size)?;
        // A panel may open before `run_app` (which runs the main loop from
        // its `gtk::Application`): GTK must be initialized for its widgets.
        gtk::init().map_err(|error| {
            log::error!("GTK could not be initialized: {error}");
            PanelError::Invalid("GTK could not be initialized")
        })?;
        let window = gtk::Window::builder()
            .title(options.title.as_str())
            .default_width(width)
            .default_height(height)
            .decorated(!options.borderless)
            .build();
        if options.transparent {
            // A transparent GTK window is one whose background the CSS
            // paints as transparent: GTK 4 draws windows with alpha when
            // the compositor supports it.
            window.add_css_class("nsh-panel-transparent");
            if let Some(display) = gtk::gdk::Display::default() {
                let provider = gtk::CssProvider::new();
                provider.load_from_string(
                    "window.nsh-panel-transparent { background-color: transparent; }",
                );
                gtk::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        }
        if options.hides_on_deactivate || options.hides_on_resign_key {
            // GTK 4 has no app deactivation: the window losing the focus is
            // the closest it has (and the signal a nonactivating panel
            // asks for too).
            window.connect_is_active_notify(|window| {
                if !window.is_active() {
                    window.set_visible(false);
                }
            });
        }
        let view = AppView::new(AppOptions {
            size: (width, height),
            title: options.title.clone(),
            ..app
        });
        window.set_child(Some(&view.widget()));
        match source {
            AppSource::Rust(root) => view.load(App::Rust(root)),
            AppSource::Bundle(bundle) => view.load(App::Js(bundle)),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => crate::dev::start(&view, url),
        }
        Ok(Self {
            window,
            app: view,
            closed: Cell::new(false),
        })
    }

    /// Shows the window and gives it the focus.
    pub fn show(&self) {
        self.window.present();
    }

    /// Hides the window; the compositor picks the window to focus next.
    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    /// Whether the window is on screen.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    /// Sizes the window: a field left out keeps its value. GTK 4 has no
    /// window positioning, so `x` and `y` are ignored.
    pub fn set_frame(&self, frame: WindowFrame) {
        if let Some(width) = frame.width {
            self.window.set_default_width(width.0.round() as i32);
        }
        if let Some(height) = frame.height {
            self.window.set_default_height(height.0.round() as i32);
        }
    }

    /// Does nothing: GTK 4 has no way to keep a window above others (see
    /// the [module](self)'s documentation).
    pub fn set_level(&self, _level: PanelLevel) {}

    /// Sets the window's title.
    pub fn set_title(&self, title: &str) {
        self.window.set_title(Some(title));
    }

    /// Closes the window, stops its app and gives the window's memory back:
    /// [`Drop`] then has nothing left to do.
    pub fn close(self) {
        self.closed.set(true);
        self.app.stop();
        self.window.close();
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        if !self.closed.get() {
            self.app.stop();
            self.window.close();
        }
    }
}

/// The window's size in points: the options', or 70% of the first monitor's
/// geometry, as the macOS backend's default.
fn panel_size(size: Option<Size<Points>>) -> Result<(i32, i32), PanelError> {
    if let Some(size) = size {
        let (width, height) = (size.width.0, size.height.0);
        if width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite() {
            return Ok((width.round() as i32, height.round() as i32));
        }
        return Err(PanelError::Invalid("a panel's size must be positive"));
    }
    let (width, height) = first_monitor_size().unwrap_or((1280, 800));
    Ok((
        (width as f32 * 0.7).round() as i32,
        (height as f32 * 0.7).round() as i32,
    ))
}

/// The size of the display's first monitor in points, `None` before the
/// display is opened (or without one).
fn first_monitor_size() -> Option<(i32, i32)> {
    use gtk::gio::prelude::ListModelExt;
    let display = gtk::gdk::Display::default()?;
    let monitor = display
        .monitors()
        .item(0)?
        .downcast::<gtk::gdk::Monitor>()
        .ok()?;
    let geometry = monitor.geometry();
    Some((geometry.width(), geometry.height()))
}
