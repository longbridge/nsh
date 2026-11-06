//! Windows implementation of the shared desktop window-state API.
//! Monitor-local transforms keep screen offsets separate from DPI scaling.
//! The engine owns move settling and fitting saved bounds onto current displays.

mod screens;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use nsh_core::{
    desktop::{Display, WindowBounds, WindowState},
    geometry::Points,
};
use screens::Screens;
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow},
    UI::WindowsAndMessaging::*,
};

use crate::{surface::Surface, window};

#[derive(Default)]
pub(crate) struct Watch {
    screens: RefCell<Screens>,
    windowed: Cell<Option<RECT>>,
    pub(crate) applying: Cell<bool>,
    pending_show: Cell<Option<SHOW_WINDOW_CMD>>,
}

fn controlled(hwnd: HWND) -> bool {
    // SAFETY: querying the surface's window on its UI thread.
    !hwnd.is_invalid() && unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32 & WS_CHILD.0 == 0
}

pub(crate) fn refresh(surface: &Surface) {
    let screens = Screens::read();
    let displays = screens.displays();
    let displays_changed = displays != surface.window_state.screens.borrow().displays();
    *surface.window_state.screens.borrow_mut() = screens;
    if displays_changed {
        surface.control(|c| c.set_displays(displays));
    }
    changed(surface);
}

/// Native movement and visibility changes also keep borderless full screen
/// covering its current monitor after resolution or monitor changes.
pub(crate) fn changed(surface: &Surface) {
    fit_full_screen(surface);
    report(surface);
}

fn fit_full_screen(surface: &Surface) {
    let hwnd = surface.hwnd();
    if surface.window_state.applying.get() || !window::is_full_screen(hwnd) {
        return;
    }
    // SAFETY: queries and repositioning of the surface's own window on its UI thread.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            return;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let screen = {
            surface
                .window_state
                .screens
                .borrow()
                .0
                .iter()
                .find(|s| s.monitor == monitor)
                .copied()
        };
        let Some(screen) = screen else {
            return;
        };
        let mut actual = RECT::default();
        if GetWindowRect(hwnd, &mut actual).is_err() || actual == screen.native {
            return;
        }
        let rect = screen.native;
        surface.window_state.applying.set(true);
        let _ = SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        surface.window_state.applying.set(false);
    }
}

pub(crate) fn displays(surface: &Surface) -> Vec<Display> {
    surface.window_state.screens.borrow().displays()
}

pub(crate) fn report(surface: &Surface) {
    let hwnd = surface.hwnd();
    if !controlled(hwnd) || surface.window_state.applying.get() {
        return;
    }
    if surface.window_state.screens.borrow().0.is_empty() {
        *surface.window_state.screens.borrow_mut() = Screens::read();
    }
    let mut rect = RECT::default();
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    // SAFETY: queries on this live HWND, with local output structures.
    unsafe {
        if GetWindowRect(hwnd, &mut rect).is_err()
            || GetWindowPlacement(hwnd, &mut placement).is_err()
        {
            return;
        }
        let screens = surface.window_state.screens.borrow();
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let Some(screen) = screens
            .0
            .iter()
            .copied()
            .find(|s| s.monitor == monitor)
            .or_else(|| screens.native_screen(rect))
        else {
            return;
        };
        let full = window::is_full_screen(hwnd);
        let minimized = IsIconic(hwnd).as_bool();
        let visible = IsWindowVisible(hwnd).as_bool();
        // A host can show the HWND directly, overriding the deferred startup mode.
        // Once shown, the native mode is authoritative.
        if visible {
            surface.window_state.pending_show.take();
        }
        let maximized = if let Some(show) = surface.window_state.pending_show.get() {
            show == SW_SHOWMAXIMIZED
        } else {
            IsZoomed(hwnd).as_bool()
                || (minimized && placement.flags.contains(WPF_RESTORETOMAXIMIZED))
        };
        let normal = if full {
            surface.window_state.windowed.get().unwrap_or(rect)
        } else if !minimized && !maximized {
            rect
        } else {
            let mut normal = placement.rcNormalPosition;
            if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0 {
                let dx = screen.work.left - screen.native.left;
                let dy = screen.work.top - screen.native.top;
                normal.left += dx;
                normal.right += dx;
                normal.top += dy;
                normal.bottom += dy;
            }
            normal
        };
        if !full {
            surface.window_state.windowed.set(Some(normal));
        }
        let restore = screens
            .native_screen(normal)
            .unwrap_or(screen)
            .bounds(normal);
        let bounds = if full {
            WindowBounds::Fullscreen(restore)
        } else if maximized {
            WindowBounds::Maximized(restore)
        } else {
            WindowBounds::Windowed(restore)
        };
        let mut state = WindowState::new(screen.bounds(rect), bounds);
        state.minimized = minimized;
        state.hidden = !minimized && !visible;
        state.display = Some(screen.display);
        let mut env = surface.environment.get();
        env.window_state = Some(state);
        drop(screens);
        surface.set_environment(env);
    }
}

pub(crate) fn set_bounds(surface: &Rc<Surface>, bounds: WindowBounds) {
    let hwnd = surface.hwnd();
    if !controlled(hwnd) {
        return;
    }
    if surface
        .environment
        .get()
        .window_state
        .is_some_and(|s| s.window_bounds == bounds && !s.minimized)
    {
        return;
    }
    let (screen, rect) = {
        let screens = surface.window_state.screens.borrow();
        let Some(screen) = screens.logical_screen(bounds.get_bounds()) else {
            return;
        };
        (screen, screen.native_bounds(bounds.get_bounds()))
    };
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    // SAFETY: this surface owns the HWND; all rectangles are physical screen coordinates here.
    unsafe {
        if GetWindowPlacement(hwnd, &mut placement).is_err() {
            return;
        }
        let visible = IsWindowVisible(hwnd).as_bool();
        placement.flags = WINDOWPLACEMENT_FLAGS(0);
        placement.rcNormalPosition = rect;
        if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0 {
            let dx = screen.work.left - screen.native.left;
            let dy = screen.work.top - screen.native.top;
            placement.rcNormalPosition.left -= dx;
            placement.rcNormalPosition.right -= dx;
            placement.rcNormalPosition.top -= dy;
            placement.rcNormalPosition.bottom -= dy;
        }
        let target = if matches!(bounds, WindowBounds::Maximized(_)) {
            SW_SHOWMAXIMIZED
        } else {
            SW_SHOWNORMAL
        };
        placement.showCmd = if visible {
            target.0 as u32
        } else {
            SW_HIDE.0 as u32
        };
        surface
            .window_state
            .pending_show
            .set((!visible).then_some(target));
        surface.window_state.windowed.set(Some(rect));
        surface.window_state.applying.set(true);
        let leaving_full_screen =
            window::is_full_screen(hwnd) && !matches!(bounds, WindowBounds::Fullscreen(_));
        if window::is_full_screen(hwnd) {
            window::set_restore_placement(hwnd, placement);
            if !matches!(bounds, WindowBounds::Fullscreen(_)) {
                window::toggle_full_screen(hwnd);
            }
        }
        if matches!(bounds, WindowBounds::Fullscreen(_)) {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            if window::is_full_screen(hwnd) {
                // Its ordinary placement was updated above; keep this full-screen display.
            } else {
                window::full_screen_at(hwnd, placement, screen.native);
            }
        } else if !leaving_full_screen {
            let _ = SetWindowPlacement(hwnd, &placement);
        }
        surface.window_state.applying.set(false);
    }
    changed(surface);
    surface.invalidate();
}

pub(crate) fn show_command(surface: &Surface) -> SHOW_WINDOW_CMD {
    let pending = surface.window_state.pending_show.take();
    if window::is_full_screen(surface.hwnd()) {
        SW_SHOW
    } else {
        pending.unwrap_or(SW_SHOW)
    }
}

/// Legacy partial frame updates: sizes keep current-window units; supplied
/// screen positions use the same monitor-local origin transform as windowState.
pub(crate) fn frame_origin(
    surface: &Surface,
    rect: RECT,
    x: Option<Points>,
    y: Option<Points>,
) -> (i32, i32) {
    if x.is_none() && y.is_none() {
        return (rect.left, rect.top);
    }
    let screens = surface.window_state.screens.borrow();
    let Some(current) = screens.native_screen(rect) else {
        let scale = surface.scale();
        return (
            x.map_or(rect.left, |p| (p.0 * scale).round() as i32),
            y.map_or(rect.top, |p| (p.0 * scale).round() as i32),
        );
    };
    let mut bounds = current.bounds(rect);
    if let Some(x) = x {
        bounds.origin.x = x;
    }
    if let Some(y) = y {
        bounds.origin.y = y;
    }
    let target = screens
        .logical_screen(bounds)
        .unwrap_or(current)
        .native_bounds(bounds);
    (
        if x.is_some() { target.left } else { rect.left },
        if y.is_some() { target.top } else { rect.top },
    )
}

#[cfg(test)]
mod tests {
    use nsh_core::{
        desktop::DesktopCommand,
        geometry::{Bounds, point, size},
    };
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        System::Ole::{OleInitialize, OleUninitialize},
        UI::HiDpi::{
            DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            SetThreadDpiAwarenessContext,
        },
    };

    use super::*;
    use crate::{AppOptions, AppView};

    fn isolated(name: &str) -> bool {
        const CHILD: &str = "NSH_WINDOW_STATE_TEST";
        if std::env::var(CHILD).as_deref() == Ok(name) {
            return false;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("window_state::tests::{name}"),
                "--nocapture",
            ])
            .env(CHILD, name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        true
    }

    struct UiApartment(DPI_AWARENESS_CONTEXT);
    impl UiApartment {
        fn new() -> Self {
            // SAFETY: each test child owns this UI thread, its DPI context and OLE apartment.
            unsafe {
                OleInitialize(None).unwrap();
                Self(SetThreadDpiAwarenessContext(
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                ))
            }
        }
    }
    impl Drop for UiApartment {
        fn drop(&mut self) {
            // SAFETY: all test windows and COM objects have dropped on this thread.
            unsafe {
                let _ = SetThreadDpiAwarenessContext(self.0);
                OleUninitialize();
            }
        }
    }
    fn rect(hwnd: HWND) -> RECT {
        let mut rect = RECT::default();
        // SAFETY: queries a window owned by this test.
        unsafe {
            GetWindowRect(hwnd, &mut rect).unwrap();
        }
        rect
    }
    fn state(view: &AppView) -> WindowState {
        view.surface().environment.get().window_state.unwrap()
    }
    fn normal(view: &AppView) -> Bounds<Points> {
        state(view).window_bounds.get_bounds()
    }
    fn close(view: &AppView) {
        // SAFETY: this test owns the window and its UI thread.
        unsafe {
            DestroyWindow(view.hwnd()).unwrap();
        }
    }
    fn near(a: Bounds<Points>, b: Bounds<Points>) {
        for (a, b) in [
            (a.origin.x, b.origin.x),
            (a.origin.y, b.origin.y),
            (a.size.width, b.size.width),
            (a.size.height, b.size.height),
        ] {
            assert!((a.0 - b.0).abs() <= 1.0, "{a:?} != {b:?}");
        }
    }
    fn initial(view: &AppView) -> Bounds<Points> {
        let area = displays(view.surface())
            .into_iter()
            .find(|d| d.primary)
            .unwrap()
            .visible_bounds;
        Bounds::new(
            point(area.origin.x + Points(40.0), area.origin.y + Points(50.0)),
            size(Points(640.0), Points(480.0)),
        )
    }

    #[test]
    fn current_frame_follows_native_moves_and_window_states() {
        if isolated("current_frame_follows_native_moves_and_window_states") {
            return;
        }
        let _ui = UiApartment::new();
        let view = AppView::new(None, AppOptions::default()).unwrap();
        let normal = initial(&view);
        set_bounds(view.surface(), WindowBounds::Windowed(normal));
        view.show();
        near(self::normal(&view), normal);
        for target in [
            WindowBounds::Maximized(normal),
            WindowBounds::Fullscreen(normal),
            WindowBounds::Windowed(normal),
        ] {
            set_bounds(view.surface(), target);
            let s = state(&view);
            assert!(!s.minimized);
            assert!(!s.hidden);
            near(s.window_bounds.get_bounds(), normal);
            assert_eq!(
                std::mem::discriminant(&s.window_bounds),
                std::mem::discriminant(&target)
            );
            let monitor = unsafe { MonitorFromWindow(view.hwnd(), MONITOR_DEFAULTTONEAREST) };
            let screens = view.surface().window_state.screens.borrow();
            let screen = *screens.0.iter().find(|s| s.monitor == monitor).unwrap();
            near(s.bounds, screen.bounds(rect(view.hwnd())));
            if matches!(target, WindowBounds::Fullscreen(_)) {
                assert_eq!(rect(view.hwnd()), screen.native);
            }
        }
        close(&view);
        assert!(view.surface().environment.get().window_state.is_none());
    }

    #[test]
    fn saved_bounds_restore_modes_idempotently_and_unminimize() {
        if isolated("saved_bounds_restore_modes_idempotently_and_unminimize") {
            return;
        }
        let _ui = UiApartment::new();
        let view = AppView::new(None, AppOptions::default()).unwrap();
        view.show();
        let normal = initial(&view);
        let modes = [
            WindowBounds::Windowed(normal),
            WindowBounds::Maximized(normal),
            WindowBounds::Fullscreen(normal),
        ];
        for from in modes {
            for to in modes {
                set_bounds(view.surface(), from);
                crate::chrome::command(view.surface(), &DesktopCommand::Minimize);
                assert!(state(&view).minimized);
                set_bounds(view.surface(), to);
                assert!(!state(&view).minimized);
                near(self::normal(&view), normal);
                assert_eq!(
                    std::mem::discriminant(&state(&view).window_bounds),
                    std::mem::discriminant(&to)
                );
                let before = rect(view.hwnd());
                set_bounds(view.surface(), to);
                assert_eq!(rect(view.hwnd()), before);
            }
        }
        set_bounds(view.surface(), WindowBounds::Windowed(normal));
        let before = rect(view.hwnd());
        crate::chrome::command(
            view.surface(),
            &DesktopCommand::SetFrame(nsh_core::desktop::WindowFrame {
                x: Some(normal.origin.x + Points(20.0)),
                y: Some(normal.origin.y + Points(20.0)),
                ..Default::default()
            }),
        );
        let after = rect(view.hwnd());
        assert_eq!(before.right - before.left, after.right - after.left);
        assert_eq!(before.bottom - before.top, after.bottom - after.top);
        let child = AppView::new(Some(view.hwnd()), AppOptions::default()).unwrap();
        assert!(child.surface().environment.get().window_state.is_none());
        set_bounds(child.surface(), WindowBounds::Maximized(normal));
        assert!(child.surface().environment.get().window_state.is_none());
        close(&child);
        close(&view);
    }

    #[test]
    fn full_screen_state_restores_a_borderless_windows_style() {
        if isolated("full_screen_state_restores_a_borderless_windows_style") {
            return;
        }
        let _ui = UiApartment::new();
        let view = AppView::new(
            None,
            AppOptions {
                transparent: true,
                ..Default::default()
            },
        )
        .unwrap();
        // SAFETY: reading this test window's style.
        let original = unsafe { GetWindowLongPtrW(view.hwnd(), GWL_STYLE) };
        let bounds = normal(&view);
        set_bounds(view.surface(), WindowBounds::Fullscreen(bounds));
        view.show();
        set_bounds(view.surface(), WindowBounds::Windowed(bounds));
        let restored = unsafe { GetWindowLongPtrW(view.hwnd(), GWL_STYLE) };
        assert_eq!(
            restored & WS_OVERLAPPEDWINDOW.0 as isize,
            original & WS_OVERLAPPEDWINDOW.0 as isize
        );
        assert!(
            !state(&view).hidden,
            "exiting startup full screen keeps the shown window visible"
        );
        close(&view);
    }

    #[test]
    fn hidden_startup_maximization_and_dpi_notifications_keep_the_restore_frame() {
        if isolated("hidden_startup_maximization_and_dpi_notifications_keep_the_restore_frame") {
            return;
        }
        let _ui = UiApartment::new();
        let view = AppView::new(None, AppOptions::default()).unwrap();
        assert!(state(&view).hidden);
        let normal = initial(&view);
        set_bounds(view.surface(), WindowBounds::Maximized(normal));
        assert!(state(&view).hidden);
        assert!(matches!(
            state(&view).window_bounds,
            WindowBounds::Maximized(_)
        ));
        view.show();
        assert!(unsafe { IsZoomed(view.hwnd()) }.as_bool());
        near(self::normal(&view), normal);
        set_bounds(view.surface(), WindowBounds::Windowed(normal));
        let before = rect(view.hwnd());
        let suggested = RECT {
            left: before.left + 40,
            top: before.top + 40,
            right: before.right + 80,
            bottom: before.bottom + 80,
        };
        let dpi = window::dpi(view.hwnd()) as usize;
        let notify = || unsafe {
            SendMessageW(
                view.hwnd(),
                WM_DPICHANGED,
                Some(WPARAM(dpi | (dpi << 16))),
                Some(LPARAM(&suggested as *const RECT as isize)),
            )
        };
        view.surface().window_state.applying.set(true);
        let _ = notify();
        view.surface().window_state.applying.set(false);
        assert_eq!(rect(view.hwnd()), before);
        let _ = notify();
        assert_eq!(rect(view.hwnd()), suggested);
        // A native host may show the HWND with a different mode from the saved
        // startup request. Report that real mode instead of keeping a stale intent.
        unsafe {
            let _ = ShowWindow(view.hwnd(), SW_HIDE);
        }
        set_bounds(view.surface(), WindowBounds::Maximized(normal));
        unsafe {
            let _ = ShowWindow(view.hwnd(), SW_SHOWNORMAL);
        }
        assert!(matches!(
            state(&view).window_bounds,
            WindowBounds::Windowed(_)
        ));
        assert!(!state(&view).hidden);
        close(&view);
    }

    #[test]
    fn window_and_displays_are_available_before_the_first_rust_render() {
        if isolated("window_and_displays_are_available_before_the_first_rust_render") {
            return;
        }
        use std::sync::{Mutex, mpsc};

        use nsh_core::element::{IntoElement, Root, component, view};
        type Snapshot = (Option<WindowState>, Vec<Display>);
        static READ: Mutex<Option<mpsc::Sender<Snapshot>>> = Mutex::new(None);
        #[component]
        fn Probe() -> impl IntoElement {
            let state = nsh_core::element::desktop::window_state();
            let displays = nsh_core::element::desktop::displays();
            if let Some(tx) = READ.lock().unwrap().take() {
                let _ = tx.send((state, displays));
            }
            view()
        }
        let _ui = UiApartment::new();
        let view = AppView::new(None, AppOptions::default()).unwrap();
        let (tx, rx) = mpsc::channel();
        *READ.lock().unwrap() = Some(tx);
        view.load(nsh_engine::App::Rust(Root::new::<Probe>(Default::default)));
        let (snapshot, displays) = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        let snapshot = snapshot.expect("window state before mount");
        assert!(!displays.is_empty());
        assert!(displays[0].primary);
        assert!(
            displays
                .iter()
                .any(|d| Some(d.id) == snapshot.display.map(|d| d.id))
        );
        view.stop();
        close(&view);
    }
}
