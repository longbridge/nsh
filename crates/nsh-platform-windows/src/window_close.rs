//! Main-window close policy and notifications, acknowledged asynchronously
//! by the engine before default destruction, with a bounded native fallback.

use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

use nsh_core::{EventKind, EventPayload, ROOT_ID};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{KillTimer, SetTimer},
};

use crate::{surface::Surface, window};

/// The native close policy and an in-flight notification.
pub(crate) struct State {
    close_on_press: Cell<bool>,
    pending: Cell<Option<Pending>>,
}

#[derive(Clone, Copy)]
struct Pending {
    serial: usize,
    close: bool,
}

// Notification gets a turn when the engine is responsive; default closing
// remains a native operation when it is busy or no longer receiving messages.
const CLOSE_GRACE_MS: u32 = 250;

impl Default for State {
    fn default() -> Self {
        Self {
            close_on_press: Cell::new(true),
            pending: Cell::new(None),
        }
    }
}

impl State {
    /// A replacement engine starts with the default policy. An old
    /// acknowledgment cannot close its window.
    pub(crate) fn reset(&self, hwnd: HWND) {
        if let Some(pending) = self.pending.take() {
            pending.cancel_timer(hwnd);
        }
        self.close_on_press.set(true);
    }

    pub(crate) fn set_close_on_press(&self, close: bool) {
        self.close_on_press.set(close);
    }
}

impl Pending {
    fn cancel_timer(self, hwnd: HWND) {
        if self.close {
            // SAFETY: cancels this request's timer on its UI thread, if armed.
            let _ = unsafe { KillTimer(Some(hwnd), self.serial) };
        }
    }
}

/// Samples policy before notifying JS. The flush acknowledgment orders
/// destruction after notification, without awaiting an async handler.
pub(crate) fn close_press(surface: &Rc<Surface>) {
    let s = &surface.window_close;
    if s.pending.get().is_some() {
        return;
    }
    let close = s.close_on_press.get();
    // Unique across surfaces, including if Windows reuses a destroyed HWND.
    // Low timer ids belong to the surface's other native tasks.
    static NEXT_CLOSE: AtomicUsize = AtomicUsize::new(usize::MAX / 2 + 1);
    let serial = NEXT_CLOSE.fetch_add(1, Ordering::Relaxed);
    s.pending.set(Some(Pending { serial, close }));
    let hwnd = surface.hwnd().0 as isize;
    let sent = Cell::new(false);
    surface.control(|control| {
        sent.set(true);
        control.dispatch_event(ROOT_ID, EventKind::WindowClosePress, EventPayload::None);
        control.flush(move |_| {
            window::close_press_handled(hwnd, serial);
        });
    });
    if !sent.get() {
        handled(surface, serial);
    } else if close {
        // SAFETY: the timer belongs to this live window and runs on its UI thread.
        // A unique id also rejects queued timers from a stopped/replaced engine.
        let timer = unsafe { SetTimer(Some(surface.hwnd()), serial, CLOSE_GRACE_MS, None) };
        if timer == 0 {
            // Failure to arm the fallback must not strand the default close.
            handled(surface, serial);
        }
    }
}

/// Handles the engine's acknowledgment on the owning UI thread.
pub(crate) fn handled(surface: &Surface, serial: usize) {
    if let Some(pending) = surface.window_close.pending.get()
        && pending.serial == serial
    {
        surface.window_close.pending.set(None);
        pending.cancel_timer(surface.hwnd());
        if pending.close {
            window::close(surface.hwnd());
        }
    }
}

/// A native deadline only closes a request whose sampled policy allowed it.
pub(crate) fn timed_out(surface: &Surface, serial: usize) -> bool {
    if surface
        .window_close
        .pending
        .get()
        .is_some_and(|p| p.serial == serial && p.close)
    {
        handled(surface, serial);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::desktop::DesktopCommand;
    use windows::Win32::UI::WindowsAndMessaging::*;

    use super::*;
    use crate::{AppOptions, AppView};

    /// Each child owns the backend's process-wide UI dispatcher.
    fn isolated(name: &str) -> bool {
        const CHILD: &str = "NSH_WINDOW_CLOSE_TEST";
        if std::env::var(CHILD).as_deref() == Ok(name) {
            return false;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("window_close::tests::{name}"),
                "--test-threads=1",
                "--nocapture",
            ])
            .env(CHILD, name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        true
    }

    struct UiApartment;

    impl UiApartment {
        fn new() -> Self {
            // SAFETY: the test's UI thread hosts AppView, as its SDK requires.
            unsafe { windows::Win32::System::Ole::OleInitialize(None) }.unwrap();
            Self
        }
    }

    impl Drop for UiApartment {
        fn drop(&mut self) {
            // SAFETY: AppView and its COM objects have already dropped.
            unsafe { windows::Win32::System::Ole::OleUninitialize() };
        }
    }

    #[test]
    fn quitting_closes_before_the_message_loop_ends_even_with_close_on_press_disabled() {
        if isolated(
            "quitting_closes_before_the_message_loop_ends_even_with_close_on_press_disabled",
        ) {
            return;
        }
        let _apartment = UiApartment::new();
        let view = AppView::new(None, AppOptions::default()).unwrap();
        view.surface().window_close.set_close_on_press(false);
        crate::chrome::command(view.surface(), &DesktopCommand::Quit);
        let mut message = MSG::default();
        let mut quit = false;
        // SAFETY: pumps this test's UI thread as the standalone runner does.
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT {
                    quit = true;
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            assert!(quit, "quitting must end the standalone message loop");
            assert!(
                !IsWindow(Some(view.hwnd())).as_bool(),
                "the window closes before WM_QUIT"
            );
        }
    }

    #[test]
    fn tray_notification_keeps_main_window_open() {
        if isolated("tray_notification_keeps_main_window_open") {
            return;
        }
        let _apartment = UiApartment::new();
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        let view = AppView::new(None, AppOptions::default()).unwrap();
        // SAFETY: a shell notification sent to this test's own window.
        unsafe {
            SendMessageW(
                view.hwnd(),
                crate::tray::WM_NSH_TRAY,
                Some(WPARAM(0)),
                Some(LPARAM(WM_MOUSEMOVE as isize)),
            );
        }
        assert!(
            unsafe { IsWindow(Some(view.hwnd())) }.as_bool(),
            "a tray notification must not close the window"
        );
        // SAFETY: this test owns the window.
        unsafe {
            DestroyWindow(view.hwnd()).unwrap();
        }
    }

    #[cfg(feature = "js")]
    mod closing {
        use std::{
            collections::BTreeMap,
            sync::{Arc, Mutex},
            time::{Duration, Instant},
        };

        use nsh_core::storage::StorageChange;
        use nsh_engine::{App, Bundle, StorageBackend};
        use windows::Win32::Foundation::{LPARAM, WPARAM};

        use super::*;

        #[derive(Default)]
        struct Saved(Mutex<BTreeMap<String, String>>);

        impl StorageBackend for Saved {
            fn load(&self) -> Result<BTreeMap<String, String>, String> {
                Ok(self.0.lock().unwrap().clone())
            }
            fn write(&self, change: StorageChange) {
                let mut entries = self.0.lock().unwrap();
                match change {
                    StorageChange::Set(k, v) => {
                        entries.insert(k, v);
                    }
                    StorageChange::Remove(k) => {
                        entries.remove(&k);
                    }
                    StorageChange::Clear => entries.clear(),
                }
            }
        }

        fn pump_until(mut done: impl FnMut() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !done() {
                assert!(Instant::now() < deadline, "window/engine did not settle");
                let mut message = MSG::default();
                // SAFETY: pumps messages of this test's own UI thread.
                unsafe {
                    while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                        if message.message != WM_QUIT {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn app(code: &str) -> (Rc<AppView>, Arc<Saved>) {
            let saved = Arc::new(Saved::default());
            let view = AppView::new(
                None,
                AppOptions {
                    storage: Some(saved.clone()),
                    size: (400, 300),
                    limits: nsh_engine::Limits::standalone(),
                    ..Default::default()
                },
            )
            .unwrap();
            view.load(App::Js(Bundle::source(format!(
                r#"
                import {{ Window }} from 'nsh/desktop';
                import {{ Storage }} from 'nsh';
                {code}
                await Storage.setItem('ready', 'yes');
            "#
            ))));
            pump_until(|| saved.0.lock().unwrap().contains_key("ready"));
            view.surface().drain();
            (view, saved)
        }

        fn busy_app(close: bool) -> Rc<AppView> {
            use std::sync::atomic::{AtomicBool, Ordering};

            let (view, saved) = app(&format!(
                r#"
                Window.setCloseOnPress({close});
                setTimeout(async () => {{
                    await Storage.setItem('busy', 'yes');
                    while (true) {{}}
                }}, 20);
            "#
            ));
            pump_until(|| saved.0.lock().unwrap().contains_key("busy"));
            // Give the promise's continuation a turn to enter the loop, then
            // verify that the engine cannot acknowledge even a plain flush.
            std::thread::sleep(Duration::from_millis(50));
            let acknowledged = Arc::new(AtomicBool::new(false));
            let reply = acknowledged.clone();
            view.surface().control(|c| {
                c.flush(move |_| {
                    reply.store(true, Ordering::SeqCst);
                })
            });
            std::thread::sleep(Duration::from_millis(50));
            assert!(!acknowledged.load(Ordering::SeqCst));
            view
        }

        #[test]
        fn close_listener_reads_the_latest_window_bounds_before_move_settling() {
            if isolated(
                "closing::close_listener_reads_the_latest_window_bounds_before_move_settling",
            ) {
                return;
            }
            use std::sync::atomic::{AtomicBool, Ordering};

            use nsh_core::{
                desktop::{WindowBounds, WindowState},
                geometry::{Bounds, point, size},
            };

            let _apartment = UiApartment::new();
            let (view, saved) = app(r#"
                import { windowState } from 'nsh/desktop';
                Window.setCloseOnPress(false);
                Window.on('closePress', async () => {
                    await Storage.setItem('saved-x', String(windowState().windowBounds.bounds.origin.x));
                });
            "#);
            let snapshot = |x| {
                let bounds = Bounds::new(
                    point(nsh_core::Points(x), nsh_core::Points(10.0)),
                    size(nsh_core::Points(400.0), nsh_core::Points(300.0)),
                );
                WindowState::new(bounds, WindowBounds::Windowed(bounds))
            };
            let mut env = view.surface().environment.get();
            env.window_state = Some(snapshot(20.0));
            view.surface().set_environment(env);
            let acknowledged = Arc::new(AtomicBool::new(false));
            let reply = acknowledged.clone();
            view.surface()
                .control(|c| c.flush(move |_| reply.store(true, Ordering::SeqCst)));
            pump_until(|| acknowledged.load(Ordering::SeqCst));
            env.window_state = Some(snapshot(40.0));
            view.surface().set_environment(env);
            // SAFETY: close follows the new move immediately on this test's HWND.
            unsafe {
                SendMessageW(view.hwnd(), WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
            }
            pump_until(|| saved.0.lock().unwrap().contains_key("saved-x"));
            assert_eq!(
                saved.0.lock().unwrap().get("saved-x").map(String::as_str),
                Some("40")
            );
            let hwnd = view.hwnd();
            window::close(hwnd);
            // SAFETY: waiting for this test's explicit native close.
            pump_until(|| !unsafe { IsWindow(Some(hwnd)) }.as_bool());
        }

        #[test]
        fn default_close_interrupts_busy_js_without_an_engine_acknowledgment() {
            if isolated(
                "closing::default_close_interrupts_busy_js_without_an_engine_acknowledgment",
            ) {
                return;
            }
            let _apartment = UiApartment::new();
            let view = busy_app(true);
            let hwnd = view.hwnd();
            // SAFETY: sends user close messages to the window owned by this test.
            unsafe {
                SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
                SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
            }
            let started = Instant::now();
            // SAFETY: querying this test's HWND until destruction.
            pump_until(|| !unsafe { IsWindow(Some(hwnd)) }.as_bool());
            assert!(started.elapsed() < Duration::from_secs(2));
        }

        #[test]
        fn busy_js_keeps_the_window_when_close_on_press_is_disabled() {
            if isolated("closing::busy_js_keeps_the_window_when_close_on_press_is_disabled") {
                return;
            }
            let _apartment = UiApartment::new();
            let view = busy_app(false);
            let hwnd = view.hwnd();
            // SAFETY: sends a user close to the test's live window.
            unsafe {
                SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
            }
            let serial = view.surface().window_close.pending.get().unwrap().serial;
            assert!(!timed_out(view.surface(), serial));
            let deadline = Instant::now() + Duration::from_millis(u64::from(CLOSE_GRACE_MS) * 2);
            pump_until(|| Instant::now() >= deadline);
            // SAFETY: querying this test's still-live window.
            assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
            window::close(hwnd);
            // SAFETY: the explicit native close interrupts the busy engine.
            pump_until(|| !unsafe { IsWindow(Some(hwnd)) }.as_bool());
        }

        #[test]
        fn stale_close_replies_and_timers_cannot_close_a_replacement_request() {
            if isolated(
                "closing::stale_close_replies_and_timers_cannot_close_a_replacement_request",
            ) {
                return;
            }
            let _apartment = UiApartment::new();
            let (view, _) = app("");
            let surface = view.surface();
            let hwnd = view.hwnd();
            close_press(surface);
            let old = surface.window_close.pending.get().unwrap().serial;
            surface.window_close.reset(hwnd);
            surface.window_close.set_close_on_press(false);
            close_press(surface);
            let current = surface.window_close.pending.get().unwrap().serial;
            assert_ne!(old, current);
            handled(surface, old);
            assert!(!timed_out(surface, old));
            assert!(!timed_out(surface, current));
            assert_eq!(surface.window_close.pending.get().unwrap().serial, current);
            surface.window_close.set_close_on_press(true);
            handled(surface, current);
            assert!(surface.window_close.pending.get().is_none());
            // SAFETY: the sampled false policy still keeps this test window.
            assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
            close_press(surface);
            let allowed = surface.window_close.pending.get().unwrap().serial;
            surface.window_close.set_close_on_press(false);
            handled(surface, allowed);
            // SAFETY: the sampled true policy closes this test's HWND.
            pump_until(|| !unsafe { IsWindow(Some(hwnd)) }.as_bool());
        }

        #[test]
        fn window_close_user_close_notifies_before_default_shutdown() {
            if isolated("closing::window_close_user_close_notifies_before_default_shutdown") {
                return;
            }
            let _apartment = UiApartment::new();
            let (view, saved) = app(r#"
                Window.on('closePress', async () => {
                    await Storage.setItem('close', 'notified');
                    await new Promise(() => {});
                    await Storage.setItem('after', 'unreachable');
                });
            "#);
            let hwnd = view.hwnd();
            // SAFETY: sends the same message as the system's close actions.
            unsafe {
                SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
            }
            pump_until(|| !unsafe { IsWindow(Some(hwnd)) }.as_bool());
            assert_eq!(
                saved.0.lock().unwrap().get("close").map(String::as_str),
                Some("notified")
            );
            assert!(!saved.0.lock().unwrap().contains_key("after"));
        }

        #[test]
        fn window_close_manual_close_saves_then_explicit_close_bypasses_notification() {
            if isolated(
                "closing::window_close_manual_close_saves_then_explicit_close_bypasses_notification",
            ) {
                return;
            }
            let _apartment = UiApartment::new();
            let (view, saved) = app(r#"
                Window.setCloseOnPress(false);
                let count = 0;
                Window.on('closePress', async () => {
                    await Storage.setItem('close', String(++count));
                });
            "#);
            let hwnd = view.hwnd();
            // SAFETY: a close action on this test's own main window.
            unsafe {
                SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
            }
            pump_until(|| saved.0.lock().unwrap().contains_key("close"));
            assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
            crate::chrome::command(view.surface(), &DesktopCommand::Close);
            pump_until(|| !unsafe { IsWindow(Some(hwnd)) }.as_bool());
            assert_eq!(
                saved.0.lock().unwrap().get("close").map(String::as_str),
                Some("1")
            );
        }

        #[test]
        fn close_press_waits_for_app_save_before_explicit_close() {
            if isolated("closing::close_press_waits_for_app_save_before_explicit_close") {
                return;
            }
            let _apartment = UiApartment::new();
            let (view, saved) = app(r#"
                Window.setCloseOnPress(false);
                Window.on('closePress', async () => {
                    await Storage.setItem('snapshot', 'saved');
                    Window.close();
                });
            "#);
            let hwnd = view.hwnd();
            // SAFETY: the same message as a close button on this test's window.
            unsafe {
                SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
            }
            pump_until(|| !unsafe { IsWindow(Some(hwnd)) }.as_bool());
            assert_eq!(
                saved.0.lock().unwrap().get("snapshot").map(String::as_str),
                Some("saved")
            );
        }
    }
}
