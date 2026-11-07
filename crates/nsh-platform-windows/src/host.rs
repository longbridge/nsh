//! Host windows: a second window of the app, beside [`run_app`](crate::run_app)'s.
//!
//! A host window is a window of the app's own, as a floating palette or a
//! popover is: it shows an [`AppSource`](crate::AppSource) of its own (a
//! Rust root, a bundle or a dev server), so it can run a different app
//! than the main window, with an engine of its own ([`AppView`]). Its
//! [`WindowOptions`] pick the geometry — the `kind` picks the look (a
//! `Panel` window has no taskbar button and does not activate the app by
//! default, as a utility window) and the `attributes` carry the title,
//! the frame and the level — while the [`AppOptions`](crate::AppOptions)
//! stay the app's own (its id, its assets, its host channel). A host
//! window lives exactly as long as its [`HostWindow`] does: dropping it
//! closes the window and stops its app — as its close button does, when
//! the frame has one and `close_on_press` allows it.
//!
//! A host window belongs to the thread that runs the app's message loop:
//! the `nsh-ui` thread [`run_app`](crate::run_app) spawns, where
//! [`run_app_with`](crate::run_app_with)'s setup hook opens host windows
//! through its [`AppContext`](crate::AppContext). It has no thread and no
//! message loop of its own: its methods call Win32 directly on the app's
//! thread, as the main window's do, so they must run there.
//! [`HostWindow::open`] elsewhere fails with
//! [`HostWindowError::WrongThread`]. Rust components run on the engine
//! thread, so a host window is opened by the host's code on the app's
//! thread (the setup hook), not from a component's event handler;
//! [`AppContext::run`](crate::AppContext::run) hops a closure there.
//!
//! Win32 maps what it can: the kind's frame comes through
//! [`crate::shell`]'s styles (a borderless window is a `WS_POPUP` window
//! without a caption), `show` shows the window without activating the
//! app when the attributes say not to (and `hide` hands the focus back
//! to the window that was in front), and [`WindowLevel::Floating`] and
//! [`WindowLevel::AboveDock`] are both `HWND_TOPMOST` (Windows has no
//! separate band above the taskbar; topmost is the one over the normal
//! windows, and a level from a newer core is topmost too).
//! [`WindowLevel::DesktopIcon`] is `HWND_BOTTOM`, the closest Win32 has,
//! as for the main window. `close_on_blur` hides the window from the
//! window procedure as the keyboard (`WM_KILLFOCUS`) leaves it for a
//! window outside it; a window hidden that way forgets the window that
//! was in front, so a later `hide` or drop does not give the focus to a
//! window the user did not choose.
//!
//! The attributes v1 honors: `title`, `frame` (`None` is borderless),
//! `resizable`, `minimizable`, `closable`, `focusable`, `activate`,
//! `level`, `close_on_press`, `close_on_blur` and `taskbar`. The rest
//! (`transparent`, the backdrop, the workspaces…) waits for a window
//! that opens with it; [`HostWindow::set_attributes`] on an open window
//! applies the `title`, the `level` and `close_on_blur`, and silently
//! ignores the rest.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub use nsh_core::desktop::HostWindowError;
use nsh_core::{
    desktop::{WindowAttributes, WindowKind, WindowLevel, WindowOptions, WindowPosition},
    geometry::{Points, Size},
};
use nsh_engine::App;
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        UI::{
            HiDpi::GetDpiForSystem,
            WindowsAndMessaging::{
                DestroyWindow, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetClientRect,
                GetForegroundWindow, GetWindow, GetWindowLongPtrW, HWND_BOTTOM, HWND_NOTOPMOST,
                HWND_TOPMOST, IsWindow, IsWindowVisible, SPI_GETWORKAREA, SW_HIDE, SW_SHOW,
                SW_SHOWNA, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetForegroundWindow, SetWindowLongPtrW,
                SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW,
            },
        },
    },
    core::HSTRING,
};

use crate::app::{AppOptions, AppSource, AppView};

/// A host window's own app in the app's second window.
///
/// Opened on the thread that runs the app's message loop, from
/// [`run_app_with`](crate::run_app_with)'s setup hook; dropped to close.
/// See the [module](self) docs for the Win32 mapping.
pub struct HostWindow {
    window: HWND,
    app: Rc<AppView>,
    attributes: RefCell<WindowAttributes>,
    level: Cell<WindowLevel>,
    activate: Cell<bool>,
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
        if !crate::app::in_app_thread() {
            return Err(HostWindowError::WrongThread);
        }
        let size = host_size(options.default_size)?;
        let attributes = WindowAttributes::resolve(options.kind, &options.attributes);
        let mut view_options = app;
        view_options.size = size;
        view_options.title = attributes
            .title
            .clone()
            .unwrap_or_else(|| String::from("nsh"));
        let app = AppView::new(None, view_options).map_err(|error| {
            log::error!("the host window could not be created: {error}");
            HostWindowError::Invalid("the host window's window could not be created")
        })?;
        let window = app.hwnd();
        Self::apply(window, options.kind, &attributes);
        Self::place(&app, &options);
        match source {
            AppSource::Rust(root) => app.load(App::Rust(root)),
            AppSource::Bundle(bundle) => app.load(App::Js(bundle)),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => crate::dev::start(&app, url),
        }
        crate::system::environment_applied(app.surface());
        let attrs = crate::window::shell(window).map(|shell| shell.attrs());
        crate::window::set_host(window, true);
        Ok(HostWindow {
            window,
            app,
            attributes: RefCell::new(attributes),
            level: Cell::new(attrs.map(|attrs| attrs.level).unwrap_or_default()),
            activate: Cell::new(attrs.map(|attrs| attrs.activate).unwrap_or(true)),
            closed: Cell::new(false),
        })
    }

    /// Shows the window, activating the app as its `attributes` say.
    pub fn show(&self) {
        if self.closed.get() || !self.live() {
            return;
        }
        if self.activate.get() {
            // SAFETY: a plain query of the window in front; the window is
            // given it back by `hide`.
            let front = unsafe { GetForegroundWindow() };
            if !front.is_invalid() && front != self.window {
                crate::window::set_previous(self.window, front);
            }
            // SAFETY: showing the window of this thread.
            let _ = unsafe { ShowWindow(self.window, SW_SHOW) };
        } else {
            // SAFETY: showing the window of this thread.
            let _ = unsafe { ShowWindow(self.window, SW_SHOWNA) };
        }
        // SAFETY: re-asserting the level, as `show` does for the main
        // window's own.
        let _ = unsafe {
            SetWindowPos(
                self.window,
                Some(order_of(self.level.get())),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            )
        };
    }

    /// Hides the window, handing the focus back to the window that was
    /// in front when it was shown.
    pub fn hide(&self) {
        self.dismiss();
    }

    /// Whether the window is on screen.
    pub fn is_visible(&self) -> bool {
        self.live() && {
            // SAFETY: a plain query.
            unsafe { IsWindowVisible(self.window) }.as_bool()
        }
    }

    /// Applies `attributes` on the open window.
    ///
    /// The `title`, the `level` and `close_on_blur` take effect at once;
    /// the rest waits for a window that opens with it, as the module
    /// docs say.
    pub fn set_attributes(&self, attributes: &WindowAttributes) {
        let changed = {
            let stored = self.attributes.borrow();
            stored.diff(attributes)
        };
        let Some(changed) = changed else {
            return;
        };
        if let Some(title) = changed.title.as_ref() {
            // SAFETY: the window of this thread.
            let _ = unsafe { SetWindowTextW(self.window, &HSTRING::from(title)) };
        }
        if changed.level.is_some() {
            let level = attributes.level.unwrap_or(self.level.get());
            self.level.set(level);
            if self.live() {
                // SAFETY: re-asserting the level.
                let _ = unsafe {
                    SetWindowPos(
                        self.window,
                        Some(order_of(level)),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                    )
                };
            }
        }
        if changed.close_on_blur.is_some() {
            crate::window::set_hides_on_focus_loss(
                self.window,
                attributes.close_on_blur.unwrap_or_default(),
            );
        }
        self.attributes.borrow_mut().merge(attributes);
    }

    /// Closes the window and stops its app. Calling it again, or any
    /// other method, does nothing.
    pub fn close(self) {
        self.shut();
    }

    /// Whether the window still exists: the app forgets it in
    /// `WM_DESTROY` (the user closed a window with a frame).
    fn live(&self) -> bool {
        !self.app.hwnd().is_invalid() && self.app.hwnd() == self.window
    }

    /// The one shutdown every path shares.
    fn shut(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.dismiss();
        self.app.stop();
        if self.live() {
            // SAFETY: the window of this thread, now empty.
            let _ = unsafe { DestroyWindow(self.window) };
        }
    }

    /// Hides the window and hands the focus back.
    fn dismiss(&self) {
        if !self.live() {
            return;
        }
        // SAFETY: hiding the window of this thread.
        let _ = unsafe { ShowWindow(self.window, SW_HIDE) };
        if let Some(previous) = crate::window::take_previous(self.window)
            && !previous.is_invalid()
            // SAFETY: a plain query.
            && unsafe { IsWindow(Some(previous)) }.as_bool()
        {
            // SAFETY: giving the focus to the window the user had.
            let _ = unsafe { SetForegroundWindow(previous) };
        }
    }

    /// Styles `window` as `kind` with `attributes`: what
    /// [`crate::shell`] knows, as the main window is styled.
    fn apply(window: HWND, kind: WindowKind, attributes: &WindowAttributes) {
        let Some(shell) = crate::window::shell(window) else {
            return;
        };
        let mut attrs = crate::shell::Attrs::of_kind(kind);
        attrs.merge(attributes);
        shell.set(attrs);
        // SAFETY: the styles of the window of this thread, as
        // [`crate::shell::restyle`] sets them.
        let _ = unsafe {
            let owned = GetWindow(window, GW_OWNER).is_ok_and(|o| !o.is_invalid());
            let next = crate::shell::styles(kind, &attrs, owned);
            let current = (
                GetWindowLongPtrW(window, GWL_STYLE) as u32,
                GetWindowLongPtrW(window, GWL_EXSTYLE) as u32,
            );
            let (style, ex_style) = crate::shell::restyled(current, next);
            SetWindowLongPtrW(window, GWL_STYLE, style as isize);
            SetWindowLongPtrW(window, GWL_EXSTYLE, ex_style as isize);
            SetWindowPos(
                window,
                Some(order_of(attrs.level)),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            )
        };
        crate::window::set_hides_on_focus_loss(window, attrs.close_on_blur);
    }

    /// Places the window as its `options` ask: at a point, centered on a
    /// display, or wherever Win32 put it.
    fn place(app: &AppView, options: &WindowOptions) {
        let window = app.hwnd();
        let display = options.display;
        match &options.default_position {
            Some(WindowPosition::At(at)) => {
                crate::window_state::open_at(window, content_size(app), Some(*at), None, display);
            }
            Some(WindowPosition::Center | WindowPosition::ParentCenter) => {
                crate::window_state::open_at(window, content_size(app), None, None, display);
            }
            None if display.is_some() => {
                crate::window_state::open_at(window, content_size(app), None, None, display);
            }
            None => {
                crate::window_state::size_content(window, content_size(app));
            }
        }
    }
}

impl Drop for HostWindow {
    fn drop(&mut self) {
        self.shut();
    }
}

/// The user pressed a host window's close button (`WM_CLOSE`): it closes
/// as its `close_on_press` says. A host window's app is its own — there
/// is no event to give it — so closing stops it at once.
pub(crate) fn close_press(hwnd: HWND) {
    let Some(shell) = crate::window::shell(hwnd) else {
        return;
    };
    if !shell.attrs().close_on_press {
        // The veto the app would ask for itself.
        return;
    }
    // The window's own `HostWindow` (if the host kept it) learns the
    // window is gone from `WM_DESTROY` forgetting it; a dropped one has
    // nothing left to do.
    // SAFETY: the window of this thread, closed by its user.
    let _ = unsafe { DestroyWindow(hwnd) };
}

/// A host window's content size, in points at its own scale.
fn content_size(app: &AppView) -> Size<Points> {
    let scale = app.surface().scale();
    // SAFETY: a plain query.
    let mut frame = RECT::default();
    if !app.hwnd().is_invalid() && unsafe { GetClientRect(app.hwnd(), &mut frame) }.is_ok() {
        return Size::new(
            Points((frame.right - frame.left) as f32 / scale),
            Points((frame.bottom - frame.top) as f32 / scale),
        );
    }
    Size::new(Points(480.0), Points(360.0))
}

/// The z-order a `level` puts a window at.
fn order_of(level: WindowLevel) -> HWND {
    match level {
        WindowLevel::Normal => HWND_NOTOPMOST,
        WindowLevel::Floating | WindowLevel::AboveDock => HWND_TOPMOST,
        WindowLevel::DesktopIcon => HWND_BOTTOM,
        // A level from a newer core: over the normal windows, as a
        // floating one.
        _ => HWND_TOPMOST,
    }
}

/// A host window's size in pixels at its own scale: its own, or 70% of
/// the work area.
fn host_size(size: Option<Size<Points>>) -> Result<(i32, i32), HostWindowError> {
    // SAFETY: a plain query.
    let dpi = unsafe { GetDpiForSystem() };
    let scale = f64::from(dpi) / 96.0;
    let Some(size) = size else {
        // 70% of the work area, at the system's scale.
        let mut area = RECT::default();
        // SAFETY: a plain query.
        let asked = unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut area as *mut _ as *mut _),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
        if asked.is_err() || area.right <= area.left || area.bottom <= area.top {
            return Ok((800, 600));
        }
        let width = (f64::from(area.right - area.left) * 0.7 * scale).round() as i32;
        let height = (f64::from(area.bottom - area.top) * 0.7 * scale).round() as i32;
        return Ok((width.max(1), height.max(1)));
    };
    let Size {
        width: Points(width),
        height: Points(height),
    } = size;
    if width <= 0.0 || height <= 0.0 || !width.is_finite() || !height.is_finite() {
        return Err(HostWindowError::Invalid(
            "a host window's size must be positive",
        ));
    }
    // `AppOptions::size` is pixels; a host window asks in points, at
    // the system's scale as the window's own is not known before it
    // exists.
    Ok((
        (f64::from(width) * scale).round() as i32,
        (f64::from(height) * scale).round() as i32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_topmost_over_the_normal_windows() {
        assert_eq!(order_of(WindowLevel::Normal), HWND_NOTOPMOST);
        assert_eq!(order_of(WindowLevel::Floating), HWND_TOPMOST);
        assert_eq!(order_of(WindowLevel::AboveDock), HWND_TOPMOST);
        assert_eq!(order_of(WindowLevel::DesktopIcon), HWND_BOTTOM);
    }

    #[test]
    fn sizes_must_be_positive() {
        let size = |w: f32, h: f32| Some(Size::new(Points(w), Points(h)));
        // The DPI scale is the system's own; only the sign matters here.
        assert!(host_size(size(420.0, 300.0)).is_ok());
        assert!(host_size(size(0.0, 300.0)).is_err());
        assert!(host_size(size(420.0, f32::NAN)).is_err());
    }
}
