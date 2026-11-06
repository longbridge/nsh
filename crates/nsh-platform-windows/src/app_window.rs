//! The app's own windows besides its main one (`Window.open`): each shows
//! a dialog root (a `modal` node) of the same engine in a top-level window
//! of its own instead of over the main window. The engine, the JS runtime
//! and the core stay one, so the windows share the app's state.
//!
//! - The root view leaves the main window's tree ([`open`]): it is the root
//!   of a tree of its own, whose changes ask its window to draw
//!   ([`NshView::set_invalidate`]), and which [`crate::render::window_frame`]
//!   draws whole into the window's own swap chain.
//! - Its size is reported to the core as the root's
//!   (`EngineControl::set_root_size`), which lays the root out at it.
//! - Its input is the surface's: while one of its messages is handled, the
//!   surface's input picks views in its root ([`enter`],
//!   [`Surface::input_root`]).
//! - Closing it hides it and tells the app (`Dismiss`), which removes the
//!   root; the window goes with it ([`removed`]), and with the main window.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use nsh_core::{
    Bounds, EdgeInsets, EventKind, EventPayload, NodeId, Points, Size,
    desktop::{DesktopCommand, WindowFrame, WindowFrameStyle, WindowLevel, WindowOptions},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT},
    UI::WindowsAndMessaging::*,
};

use crate::{surface::Surface, view::NshView};

/// A window of the app's own.
pub(crate) struct AppWindow {
    /// The dialog root it shows.
    id: NodeId,
    hwnd: HWND,
    view: NshView,
    swap: RefCell<Option<crate::gfx::SwapChain>>,
    /// The scale of the swap chain's target.
    target_scale: Cell<f32>,
    /// Its client area in pixels, and its scale.
    pixels: Cell<(u32, u32)>,
    scale: Cell<f32>,
    /// A `WM_PAINT` is asked for.
    invalid: Cell<bool>,
    /// The size last reported to the core.
    reported: Cell<Option<Size<Points>>>,
}

/// The window showing dialog root `id`.
fn get(surface: &Surface, id: NodeId) -> Option<Rc<AppWindow>> {
    surface
        .windows
        .borrow()
        .iter()
        .find(|w| w.id == id)
        .cloned()
}

/// Whether dialog root `id` is shown in a window of its own.
pub(crate) fn is_window(surface: &Surface, id: NodeId) -> bool {
    surface.windows.borrow().iter().any(|w| w.id == id)
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
    let Some(window) = get(surface, id) else {
        return log::debug!("window: no window for {id}");
    };
    let hwnd = window.hwnd;
    // SAFETY: plain calls on the window, alive while it is listed.
    unsafe {
        match command {
            DesktopCommand::SetTitle(title) => {
                let _ = SetWindowTextW(hwnd, &windows::core::HSTRING::from(title.as_str()));
            }
            DesktopCommand::Minimize => {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            DesktopCommand::ToggleMaximize => {
                let show = if IsZoomed(hwnd).as_bool() {
                    SW_RESTORE
                } else {
                    SW_MAXIMIZE
                };
                let _ = ShowWindow(hwnd, show);
            }
            DesktopCommand::SetFrame(frame) => set_frame(&window, frame),
            DesktopCommand::SetLevel(level) => set_level(hwnd, *level),
            DesktopCommand::OrderFront => {
                let show = if IsIconic(hwnd).as_bool() {
                    SW_RESTORE
                } else {
                    SW_SHOW
                };
                let _ = ShowWindow(hwnd, show);
                let _ = SetForegroundWindow(hwnd);
            }
            DesktopCommand::Hide => {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            // As the user closing it: the app hears `Dismiss` and removes
            // the root (`handle.close()` removes it itself).
            DesktopCommand::Close => {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            // Menus, the title bar's style and full screen are the main
            // window's.
            _ => {}
        }
    }
}

/// Shows dialog root `view` in a new window.
fn open(surface: &Rc<Surface>, view: &NshView, options: &WindowOptions) {
    let id = view.id();
    if is_window(surface, id) {
        return;
    }
    // Presented over the main window by its insertion, earlier in the
    // batch: it leaves the main window's modals and tree.
    crate::navigation::release(surface, view);
    let title = match &options.title {
        Some(title) => title.clone(),
        None => main_title(surface),
    };
    let size = options.size.map_or((480.0, 360.0), |s| {
        (s.width.0.max(1.0), s.height.0.max(1.0))
    });
    let position = options.position.map(|p| (p.x.0, p.y.0));
    let framed = options.frame != WindowFrameStyle::None;
    let hwnd = match crate::window::create_app_window(surface, id, &title, size, position, framed) {
        Ok(hwnd) => hwnd,
        Err(err) => return log::error!("window: cannot create a window: {err}"),
    };
    let window = Rc::new(AppWindow {
        id,
        hwnd,
        view: view.clone(),
        swap: RefCell::new(None),
        target_scale: Cell::new(0.0),
        pixels: Cell::new((0, 0)),
        scale: Cell::new(1.0),
        invalid: Cell::new(false),
        reported: Cell::new(None),
    });
    let weak = Rc::downgrade(&window);
    view.set_invalidate(Rc::new(move |_| {
        if let Some(window) = weak.upgrade() {
            invalidate(&window);
        }
    }));
    surface.windows.borrow_mut().push(window.clone());
    if options.level != WindowLevel::Normal {
        set_level(hwnd, options.level);
    }
    resized(surface, &window);
    // SAFETY: plain calls on the window just made.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// The main window's title (the app's name).
fn main_title(surface: &Surface) -> String {
    let mut text = [0u16; 256];
    // SAFETY: a plain call into a local buffer; 0 for no title.
    let n = unsafe { GetWindowTextW(surface.hwnd(), &mut text) };
    String::from_utf16_lossy(&text[..n.max(0) as usize])
}

/// Asks for `window` to be drawn again (once, however often asked before it
/// is).
fn invalidate(window: &AppWindow) {
    if !window.invalid.replace(true) {
        crate::window::invalidate(window.hwnd);
    }
}

/// Asks for every window of the app's own to be drawn again (the theme
/// changed, the device was lost, ...).
pub(crate) fn invalidate_all(surface: &Surface) {
    let Ok(windows) = surface.windows.try_borrow() else {
        return;
    };
    for window in windows.iter() {
        invalidate(window);
    }
}

/// The window's client area was resized or moved to another scale: the
/// root view takes its size, which the core lays the root out at.
fn resized(surface: &Surface, window: &AppWindow) {
    let mut rect = RECT::default();
    // SAFETY: plain call with a local out value.
    let _ = unsafe { GetClientRect(window.hwnd, &mut rect) };
    let pixels = (
        (rect.right - rect.left).max(0) as u32,
        (rect.bottom - rect.top).max(0) as u32,
    );
    let scale = crate::window::dpi(window.hwnd) as f32 / 96.0;
    window.pixels.set(pixels);
    window.scale.set(scale);
    // Minimized: the root keeps its size.
    if pixels.0 == 0 || pixels.1 == 0 {
        return;
    }
    let size = Size::new(
        Points(pixels.0 as f32 / scale),
        Points(pixels.1 as f32 / scale),
    );
    let frame = Bounds::new(Default::default(), size);
    if std::mem::replace(&mut window.view.state_mut().frame, frame) != frame {
        window.view.moved();
    }
    if window.reported.replace(Some(size)) != Some(size) {
        surface.control(|c| c.set_root_size(window.id, size, EdgeInsets::default()));
    }
    invalidate(window);
}

/// Draws `window` (`WM_PAINT`, and at once when resized).
fn paint(surface: &Rc<Surface>, window: &AppWindow) {
    window.invalid.set(false);
    crate::render::window_frame(
        surface,
        &window.view,
        window.hwnd,
        &mut window.swap.borrow_mut(),
        &window.target_scale,
        window.pixels.get(),
        window.scale.get(),
    );
}

/// The window's frame (`Window.setFrame` on the window's handle): its
/// outer bounds in points, what is not given kept.
fn set_frame(window: &AppWindow, frame: &WindowFrame) {
    let hwnd = window.hwnd;
    let mut rect = RECT::default();
    // SAFETY: a plain query with a local out value.
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return;
    }
    let scale = window.scale.get();
    let px = |v: Option<Points>, or: i32| v.map_or(or, |v| (v.0 * scale).round() as i32);
    let x = px(frame.x, rect.left);
    let y = px(frame.y, rect.top);
    let width = px(frame.width, rect.right - rect.left);
    let height = px(frame.height, rect.bottom - rect.top);
    // SAFETY: a plain call on a live window.
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

/// The window's level: floating windows stay above the others.
fn set_level(hwnd: HWND, level: WindowLevel) {
    let order = match level {
        WindowLevel::Normal => HWND_NOTOPMOST,
        WindowLevel::Floating => HWND_TOPMOST,
        WindowLevel::DesktopIcon => HWND_BOTTOM,
    };
    // SAFETY: a plain call on a live window.
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            Some(order),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

/// The messages of the window showing dialog root `id` that are not the
/// surface's input: `Some(result)` when handled here (`Some(None)`: the
/// system's default handling), `None` for the surface's input handling.
pub(crate) fn handle(
    surface: &Rc<Surface>,
    id: NodeId,
    hwnd: HWND,
    msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> Option<Option<LRESULT>> {
    let window = get(surface, id);
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // SAFETY: paired Begin / EndPaint on this window.
            unsafe { BeginPaint(hwnd, &mut ps) };
            if let Some(window) = &window {
                paint(surface, window);
            }
            // SAFETY: as above.
            let _ = unsafe { EndPaint(hwnd, &ps) };
            Some(Some(LRESULT(0)))
        }
        WM_ERASEBKGND => Some(Some(LRESULT(1))),
        WM_SIZE => {
            if let Some(window) = &window {
                resized(surface, window);
                // Drawn now, so that it never shows stretched or stale
                // pixels while it is resized.
                paint(surface, window);
            }
            Some(Some(LRESULT(0)))
        }
        WM_DPICHANGED => {
            // SAFETY: `lparam` is the suggested window rectangle.
            let r = unsafe { &*(lparam.0 as *const RECT) };
            // SAFETY: plain call on this window.
            let _ = unsafe {
                SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            if let Some(window) = &window {
                resized(surface, window);
            }
            Some(Some(LRESULT(0)))
        }
        // Closed by the user: it hides, and the app hears `Dismiss` and
        // removes the root (`handle.close()`), which destroys it.
        WM_CLOSE => {
            if window.is_some() {
                // SAFETY: a plain call on this window.
                let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
                surface.event(id, EventKind::Dismiss, EventPayload::None);
                return Some(Some(LRESULT(0)));
            }
            Some(None)
        }
        // The main window's: its title bar, menu bar, accessibility tree,
        // lifecycle and engine.
        WM_NCCALCSIZE | WM_NCHITTEST | WM_NCMOUSEMOVE | WM_NCMOUSELEAVE | WM_NCLBUTTONDOWN
        | WM_NCLBUTTONUP | WM_NCLBUTTONDBLCLK | WM_NCACTIVATE | WM_NCPAINT | 0x0091 | 0x0092
        | WM_COMMAND | WM_GETOBJECT | WM_WINDOWPOSCHANGED | WM_DESTROY => Some(None),
        m if m >= WM_APP => Some(None),
        _ => None,
    }
}

/// While the window showing dialog root `id` handles input, the surface's
/// input picks views in its root.
pub(crate) fn enter(surface: &Surface, id: NodeId) -> Option<InputRoot<'_>> {
    let view = get(surface, id)?.view.clone();
    let outer = surface.input_root.replace(Some(view));
    Some(InputRoot { surface, outer })
}

/// Restores the surface's input root when dropped ([`enter`]).
pub(crate) struct InputRoot<'a> {
    surface: &'a Surface,
    outer: Option<NshView>,
}

impl Drop for InputRoot<'_> {
    fn drop(&mut self) {
        *self.surface.input_root.borrow_mut() = self.outer.take();
    }
}

/// Dialog root `view` was removed: its window goes. Whether it had one.
pub(crate) fn removed(surface: &Surface, view: &NshView) -> bool {
    let window = {
        let mut windows = surface.windows.borrow_mut();
        let Some(i) = windows.iter().position(|w| w.id == view.id()) else {
            return false;
        };
        windows.remove(i)
    };
    destroy(&window);
    true
}

/// Closes every window of the app's own (the app stopped).
pub(crate) fn close_all(surface: &Surface) {
    let windows = std::mem::take(&mut *surface.windows.borrow_mut());
    for window in &windows {
        destroy(window);
    }
}

fn destroy(window: &AppWindow) {
    window.view.clear_invalidate();
    window.swap.borrow_mut().take();
    // SAFETY: a plain call on a window of this thread.
    let _ = unsafe { DestroyWindow(window.hwnd) };
}
