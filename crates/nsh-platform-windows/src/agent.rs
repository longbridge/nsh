//! The platform's part of `nsh test --target windows`
//! (`nsh_testing::agent`): where a node's view is in the window, and the
//! input a user's hands give there, through the handlers the window's
//! messages go through ([`crate::synth`]). The agent calls from a thread
//! of its own; each call does its work on the main thread
//! ([`crate::window::invoke`]) and returns once it is done.

use std::{rc::Rc, sync::mpsc, time::Duration};

use nsh_core::{
    NodeId,
    geometry::{Bounds, Point, Points, Size, point},
};
use nsh_testing::agent::{Key, Native};

use crate::{
    input::{Modifiers, PointerKind, WheelEvent},
    surface::Surface,
    synth,
};

/// How long a call waits for the main thread at most.
const MAIN: Duration = Duration::from_secs(10);
/// A press's hold, as a quick click's.
const CLICK: Duration = Duration::from_millis(60);
/// A precision touchpad's events, a display frame apart.
const FRAME: Duration = Duration::from_millis(16);
/// How far one wheel event moves at most, in points.
const SCROLL_STEP: f32 = 20.0;
/// The virtual keys the agent presses.
const VK_BACK: u16 = 0x08;
const VK_RETURN: u16 = 0x0D;
const VK_A: u16 = 0x41;

/// The surface the agent acts on, by key (its calls must be `Send`).
pub(crate) struct WindowsNative {
    key: u64,
}

impl WindowsNative {
    pub(crate) fn new(key: u64) -> Self {
        WindowsNative { key }
    }

    /// `f` with the surface on the main thread, waited for; `None` once
    /// the surface is gone.
    fn on_main<R: Send + 'static>(
        &self,
        f: impl FnOnce(&Rc<Surface>) -> R + Send + 'static,
    ) -> Option<R> {
        let (tx, rx) = mpsc::channel();
        let key = self.key;
        crate::window::invoke(move || {
            let _ = tx.send(Surface::by_key(key).map(|surface| f(&surface)));
        });
        rx.recv_timeout(MAIN).ok().flatten()
    }

    fn click(&self, at: Point<Points>, hold: Duration, button: u8) {
        let (x, y) = (at.x.0, at.y.0);
        // A real pointer arrives before it presses (hover state).
        self.on_main(move |s| {
            synth::pointer(s, PointerKind::Move, x, y, 0);
            synth::pointer(s, PointerKind::Down, x, y, button);
        });
        std::thread::sleep(hold);
        self.on_main(move |s| synth::pointer(s, PointerKind::Up, x, y, button));
    }
}

impl Native for WindowsNative {
    fn bounds(&self, id: NodeId) -> Option<Bounds<Points>> {
        self.on_main(move |surface| {
            let view = surface.mounter.borrow().views.get(id)?;
            let to_root = view.to_root()?;
            let size = view.state().frame.size;
            let corners = [
                (0.0, 0.0),
                (size.width.0, 0.0),
                (0.0, size.height.0),
                (size.width.0, size.height.0),
            ];
            let mapped: Vec<(f32, f32)> = corners
                .iter()
                .map(|&(x, y)| to_root.map(x, y))
                .collect::<Option<_>>()?;
            let (x0, y0) = mapped
                .iter()
                .fold((f32::MAX, f32::MAX), |(a, b), &(x, y)| (a.min(x), b.min(y)));
            let (x1, y1) = mapped
                .iter()
                .fold((f32::MIN, f32::MIN), |(a, b), &(x, y)| (a.max(x), b.max(y)));
            Some(Bounds::new(
                point(Points(x0), Points(y0)),
                Size::new(Points(x1 - x0), Points(y1 - y0)),
            ))
        })
        .flatten()
    }

    fn viewport(&self) -> Size<Points> {
        let (width, height) = self.on_main(|s| s.size()).unwrap_or((0.0, 0.0));
        Size::new(Points(width), Points(height))
    }

    fn press(&self, at: Point<Points>) {
        self.click(at, CLICK, 0);
    }

    fn long_press(&self, at: Point<Points>, hold: Duration) {
        self.click(at, hold, 0);
    }

    fn scroll(&self, at: Point<Points>, delta: Point<Points>) {
        let (x, y) = (at.x.0, at.y.0);
        let (dx, dy) = (delta.x.0, delta.y.0);
        // A precision touchpad's events, a frame apart, adding up to it.
        let n = (dx.abs().max(dy.abs()) / SCROLL_STEP).ceil().max(1.0) as u32;
        for _ in 0..n {
            self.on_main(move |s| {
                crate::input::wheel(
                    s,
                    WheelEvent {
                        x,
                        y,
                        dx: dx / n as f32,
                        dy: dy / n as f32,
                        precise: true,
                        modifiers: Modifiers::default(),
                        time: s.clock(),
                    },
                );
            });
            std::thread::sleep(FRAME);
        }
    }

    fn type_text(&self, text: &str) {
        // Key by key, as a keyboard's characters come.
        for ch in text.chars() {
            self.on_main(move |s| synth::type_text(s, &ch.to_string()));
        }
    }

    fn key(&self, key: Key) {
        let (vk, modifiers) = match key {
            Key::Return => (VK_RETURN, Modifiers::default()),
            Key::Backspace => (VK_BACK, Modifiers::default()),
            Key::SelectAll => (
                VK_A,
                Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
            ),
        };
        self.on_main(move |s| synth::press(s, vk, modifiers));
    }

    fn input_text(&self, id: NodeId) -> Option<String> {
        self.on_main(move |surface| {
            let view = surface.mounter.borrow().views.get(id)?;
            crate::text_input::current_text(&view)
        })
        .flatten()
    }

    fn back(&self) -> Result<(), String> {
        // The mouse's back button goes back from wherever the focus is
        // ([`crate::input`]), as in browsers.
        let middle = self.viewport();
        let at = point(Points(middle.width.0 / 2.0), Points(middle.height.0 / 2.0));
        self.click(at, CLICK, 3);
        Ok(())
    }

    fn answer_dialog(&self, button: Option<&str>) -> Result<(), String> {
        let button = button.map(str::to_owned);
        let (x, y) = self
            .on_main(move |s| crate::system::dialog_point(s, button.as_deref()))
            .unwrap_or_else(|| Err("the window is gone".into()))?;
        self.click(point(Points(x), Points(y)), CLICK, 0);
        Ok(())
    }

    fn animating(&self) -> bool {
        self.on_main(|s| s.animating()).unwrap_or(false)
    }

    fn screenshot(&self) -> Result<Vec<u8>, String> {
        self.on_main(synth::screenshot)
            .unwrap_or_else(|| Err("the window is gone".into()))
    }

    fn resize(&self, width: Points, height: Points) -> Result<(), String> {
        self.on_main(move |s| synth::resize(s, width.0, height.0))
            .ok_or_else(|| "the window is gone".to_owned())
    }

    fn set_dark(&self, dark: bool) -> Result<(), String> {
        self.on_main(move |s| {
            crate::system::simulate_system_scheme(dark);
            crate::system::settings_changed(s);
        })
        .ok_or_else(|| "the window is gone".to_owned())
    }

    fn quit(&self) {
        self.on_main(|s| {
            // SAFETY: a plain call on the surface's window.
            let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(s.hwnd()) };
        });
    }
}
