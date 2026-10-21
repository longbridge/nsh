//! The platform's part of `nsh test --target linux` (`nsh_testing::agent`):
//! where a node's view is in the window, and the input a user's hands give
//! there, through the handlers real input goes through ([`crate::synth`]).
//! The agent calls from a thread of its own; each call does its work on
//! the GTK main thread and returns once it is done. Calls go at the
//! default priority, as input does: an idle source (`MainContext::invoke`)
//! waits for a busy main loop (software rendering redrawing) to be idle.

use std::{rc::Rc, sync::mpsc, time::Duration};

use gtk4::{self as gtk, gdk, glib, prelude::*};
use nsh_core::{
    NodeId,
    geometry::{Bounds, Point, Points, Size, point},
};
use nsh_testing::agent::{Key, Native};

use crate::{input::Simulated, surface::Surface, synth};

/// How long a call waits for the main thread at most.
const MAIN: Duration = Duration::from_secs(10);
/// A press's hold, as a quick click's.
const CLICK: Duration = Duration::from_millis(60);
/// A touchpad's scroll events, a display frame apart.
const FRAME: Duration = Duration::from_millis(16);
/// How far one scroll event moves at most, in points.
const SCROLL_STEP: f32 = 20.0;

/// The surface the agent acts on, by key (its calls must be `Send`).
pub(crate) struct LinuxNative {
    key: u64,
}

impl LinuxNative {
    pub(crate) fn new(key: u64) -> Self {
        LinuxNative { key }
    }

    /// `f` with the surface on the main thread, waited for; `None` once
    /// the surface is gone.
    fn on_main<R: Send + 'static>(
        &self,
        f: impl FnOnce(&Rc<Surface>) -> R + Send + 'static,
    ) -> Option<R> {
        let (tx, rx) = mpsc::channel();
        let key = self.key;
        let asked = std::time::Instant::now();
        glib::MainContext::default().invoke_with_priority(glib::Priority::DEFAULT, move || {
            let ran = asked.elapsed();
            let started = std::time::Instant::now();
            let _ = tx.send(Surface::by_key(key).map(|surface| f(&surface)));
            if ran.as_millis() > 300 || started.elapsed().as_millis() > 300 {
                log::info!("DEBUG main call waited {ran:?}, ran {:?}", started.elapsed());
            }
        });
        let r = rx.recv_timeout(MAIN);
        if r.is_err() {
            log::info!("DEBUG main call timed out after {:?}", asked.elapsed());
        }
        r.ok().flatten()
    }

    /// `f` with the surface's window on the main thread.
    fn with_window<R: Send + 'static>(
        &self,
        f: impl FnOnce(&Rc<Surface>, &gtk::Window) -> R + Send + 'static,
    ) -> Option<R> {
        self.on_main(move |surface| synth::window(surface).map(|w| f(surface, &w)))
            .flatten()
    }

    fn click(&self, at: Point<Points>, hold: Duration) {
        let (x, y) = (at.x.0, at.y.0);
        // A real pointer arrives before it presses (hover state).
        self.on_main(move |s| {
            crate::input::simulate(s, Simulated::Hover(x, y));
            crate::input::simulate(s, Simulated::Down(x, y));
        });
        std::thread::sleep(hold);
        self.on_main(move |s| crate::input::simulate(s, Simulated::Up(x, y)));
    }
}

impl Native for LinuxNative {
    fn bounds(&self, id: NodeId) -> Option<Bounds<Points>> {
        self.on_main(move |surface| {
            let view = surface.mounter.borrow().views.get(&id)?.clone();
            if !view.is_drawable() {
                return None;
            }
            let r = view.compute_bounds(&surface.root)?;
            Some(Bounds::new(
                point(Points(r.x()), Points(r.y())),
                Size::new(Points(r.width()), Points(r.height())),
            ))
        })
        .flatten()
    }

    fn viewport(&self) -> Size<Points> {
        let (width, height) = self
            .on_main(|s| (s.root.width() as f32, s.root.height() as f32))
            .unwrap_or((0.0, 0.0));
        Size::new(Points(width), Points(height))
    }

    fn press(&self, at: Point<Points>) {
        self.click(at, CLICK);
    }

    fn long_press(&self, at: Point<Points>, hold: Duration) {
        self.click(at, hold);
    }

    fn scroll(&self, at: Point<Points>, delta: Point<Points>) {
        let (x, y) = (at.x.0, at.y.0);
        let (dx, dy) = (delta.x.0, delta.y.0);
        // A touchpad's smooth events, a frame apart, adding up to it.
        let n = (dx.abs().max(dy.abs()) / SCROLL_STEP).ceil().max(1.0) as u32;
        for _ in 0..n {
            let (sx, sy) = (dx / n as f32, dy / n as f32);
            self.on_main(move |s| crate::input::simulate(s, Simulated::Wheel(x, y, sx, sy)));
            std::thread::sleep(FRAME);
        }
    }

    fn type_text(&self, text: &str) {
        // Key by key, as a keyboard's characters come.
        for ch in text.chars() {
            self.with_window(move |_, w| synth::type_text(w, &ch.to_string()));
        }
    }

    fn key(&self, key: Key) {
        self.with_window(move |_, w| match key {
            Key::Return => synth::enter(w),
            Key::Backspace => synth::backspace(w),
            Key::SelectAll => synth::select_all(w),
        });
    }

    fn input_text(&self, id: NodeId) -> Option<String> {
        self.on_main(move |surface| {
            let view = surface.mounter.borrow().views.get(&id)?.clone();
            crate::text_input::current_text(&view)
        })
        .flatten()
    }

    fn back(&self) -> Result<(), String> {
        // Escape, as a user goes back: in a focused field it only leaves
        // it, so then once more.
        self.on_main(|s| {
            if crate::navigation::focused_field(s).is_some() {
                crate::navigation::key_pressed(s, gdk::Key::Escape, gdk::ModifierType::empty());
            }
            crate::navigation::key_pressed(s, gdk::Key::Escape, gdk::ModifierType::empty());
        })
        .ok_or_else(|| "the window is gone".to_owned())
    }

    fn answer_dialog(&self, button: Option<&str>) -> Result<(), String> {
        let button = button.map(str::to_owned);
        self.on_main(move |s| crate::system::answer_dialog(s, button.as_deref()))
            .unwrap_or_else(|| Err("the window is gone".into()))
    }

    fn animating(&self) -> bool {
        self.on_main(|s| s.animating()).unwrap_or(false)
    }

    fn screenshot(&self) -> Result<Vec<u8>, String> {
        let (tx, rx) = mpsc::channel();
        let key = self.key;
        glib::MainContext::default().invoke_with_priority(glib::Priority::DEFAULT, move || {
            let Some(window) = Surface::by_key(key).and_then(|s| synth::window(&s)) else {
                let _ = tx.send(Err("the window is gone".to_owned()));
                return;
            };
            // Once what changed so far is painted.
            synth::after_paint(&window, move |w| {
                let _ = tx.send(synth::shot(w));
            });
        });
        rx.recv_timeout(MAIN)
            .map_err(|_| "the window drew no picture".to_owned())?
    }

    fn resize(&self, width: Points, height: Points) -> Result<(), String> {
        self.with_window(move |_, w| w.set_default_size(width.0 as i32, height.0 as i32))
            .ok_or_else(|| "the window is gone".to_owned())
    }

    fn set_dark(&self, dark: bool) -> Result<(), String> {
        self.on_main(move |_| crate::system::simulate_system_scheme(dark))
            .ok_or_else(|| "the window is gone".to_owned())
    }

    fn quit(&self) {
        self.with_window(|_, w| w.close());
    }
}
