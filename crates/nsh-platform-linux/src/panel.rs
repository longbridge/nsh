//! Panel windows: a second window of the app, beside the main window's.
//!
//! A panel is a window of its own, as a floating palette or a popover is:
//! it shows an [`AppSource`] of its own (a Rust root, a bundle or a dev
//! server), so it can run a different app than the main window, with an
//! engine of its own ([`AppView`]). It has no title bar by default. A panel
//! lives exactly as long as its [`Panel`] does: dropping it closes the
//! window and stops its app.
//!
//! A panel is opened on the thread that runs the app's GTK main loop, from
//! [`run_app_with`](crate::run_app_with)'s setup hook: [`Panel::open`]
//! returns [`PanelError::WrongThread`] on any other thread, and [`Panel`] is
//! not `Send`. Rust components run on the engine thread, so a panel is
//! opened by the host's main-loop code (as an app's runner does), not from a
//! component's event handler.
//!
//! GTK 4 has no panel concept, and several options have no equivalent:
//!
//! - [`PanelOptions::transparent`]: the window's own background is
//!   transparent. It is best effort (the compositor must composite the
//!   windows), and the app's root still paints its own background: one that
//!   would show what is behind the window gives its root a transparent
//!   background too.
//! - [`PanelOptions::level`]: GTK 4 removed the API to keep a window above
//!   others (GTK 3's `gtk_window_set_keep_above`), and the compositor stacks
//!   windows: the level has no effect on Linux, and [`Panel::set_level`]
//!   does nothing.
//! - [`PanelOptions::nonactivating`]: GTK 4 has no "take the keyboard
//!   without activating the app"; the panel is a normal window.
//! - [`PanelOptions::hides_on_deactivate`]: GTK 4 cannot observe the app's
//!   deactivation (there is no per-application activation), so the option
//!   does nothing: a panel that hides when the focus leaves it asks for
//!   [`PanelOptions::hides_on_resign_key`].
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

use std::{cell::Cell, rc::Rc, sync::OnceLock};

use gtk4::{self as gtk, prelude::*};
pub use nsh_core::desktop::{PanelError, PanelOptions, WindowLevel};
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
    /// [`PanelError::WrongThread`] when this is not the thread that runs the
    /// app's GTK main loop ([`run_app_with`](crate::run_app_with)), where
    /// GTK is up; [`PanelError::Invalid`] when the panel's size is not
    /// positive.
    pub fn open(
        options: PanelOptions,
        app: AppOptions,
        source: AppSource,
    ) -> Result<Self, PanelError> {
        // GTK's widgets belong to the main loop, and the app's GTK is
        // already initialized there: another thread cannot make a window.
        if app_thread() != Some(std::thread::current().id()) {
            return Err(PanelError::WrongThread);
        }
        let (width, height) = panel_size(options.size)?;
        let window = gtk::Window::builder()
            .title(options.title.as_str())
            .default_width(width)
            .default_height(height)
            .decorated(!options.borderless)
            .build();
        if options.transparent {
            // GTK paints the window's own background from this class; the
            // app's root paints its own, inside the window.
            window.add_css_class("nsh-panel-transparent");
            register_transparent_css();
        }
        if options.hides_on_resign_key {
            // GTK 4's active window is the one with the keyboard: losing it
            // is the signal a panel asks for here. App deactivation has no
            // GTK 4 signal (`hides_on_deactivate` does nothing on Linux).
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

    /// Does nothing: GTK 4 has no way to keep a window above others, so
    /// Linux does not ask the window manager for a layer either (see the
    /// [module](self)'s documentation).
    pub fn set_level(&self, _level: WindowLevel) {}

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

/// The thread that runs the GTK main loop ([`run_app`](crate::run_app)), if
/// it runs: the one a panel opens on, where GTK is up.
fn app_thread() -> Option<std::thread::ThreadId> {
    crate::app::APP_THREAD.get().copied()
}

/// Registers the stylesheet that makes a `nsh-panel-transparent` window's
/// own background transparent, once: a display keeps every provider added
/// to it, so one provider per panel would pile up and restyle every window
/// again.
fn register_transparent_css() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    let _ = REGISTERED.get_or_init(|| {
        // GTK 4 draws windows with alpha when the compositor supports it.
        let provider = gtk::CssProvider::new();
        provider
            .load_from_string("window.nsh-panel-transparent { background-color: transparent; }");
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
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

#[cfg(test)]
mod tests {
    use nsh_core::geometry::{Points, Size};

    use super::{PanelError, app_thread, panel_size};

    /// Nothing ran the main loop: the test thread is not the app's, which
    /// `Panel::open` refuses.
    #[test]
    fn a_panel_opens_only_on_the_thread_that_runs_the_app() {
        assert_eq!(app_thread(), None);
    }

    #[test]
    fn a_panels_size_must_be_positive() {
        let size = |width, height| Some(Size::new(Points(width), Points(height)));
        assert_eq!(panel_size(size(420.0, 300.0)), Ok((420, 300)));
        assert_eq!(
            panel_size(size(0.0, 300.0)),
            Err(PanelError::Invalid("a panel's size must be positive"))
        );
        assert_eq!(
            panel_size(size(f32::NAN, 300.0)),
            Err(PanelError::Invalid("a panel's size must be positive"))
        );
    }
}
