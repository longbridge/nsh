//! Scripted runs of the standalone app (`AppOptions::script`): steps
//! separated by `;` or new lines, run in order once the window shows.
//!
//! - `wait <ms>`: waits.
//! - `shot <file.png>`: renders the window's content to a PNG (offscreen,
//!   at scale 1), whatever covers the window on screen.
//! - `tap <x> <y>`: presses and releases a finger at a point (points, root
//!   coordinates).
//! - `drag <x0> <y0> <x1> <y1> [ms]`: presses the mouse, moves in steps
//!   over `ms` (default 300), releases; `touch` the same with a finger
//!   (whose drags scroll).
//! - `wheel <x> <y> <dx> <dy>`: a wheel / touchpad scroll.
//! - `hover <x> <y>`: moves the pointer.
//! - `type <text>`: types into the focused field.
//! - `mark [text]`: an input method's composition (its preedit text) in
//!   the focused field, as GTK reports it; `mark` alone ends it. A
//!   commit is the text typed before the end: `type 腾; mark`.
//! - `selectall`: selects the focused field's text.
//! - `key <name>`: presses a key (GDK names: `Return`, `BackSpace`, ...).
//! - `dark` / `light`: switches the system's appearance (an app that
//!   forces one keeps it).
//! - `resize <w> <h>`: resizes the window (a tiling window manager keeps
//!   its own size).
//! - `quit`: closes the window.

use std::{rc::Rc, time::Duration};

use gtk4::{self as gtk, gdk, glib, prelude::*};

use crate::{input::Simulated, surface::Surface};

/// Runs `script` against `window`'s surface.
pub(crate) fn run(window: &gtk::ApplicationWindow, surface: Rc<Surface>, script: &str) {
    let steps: Vec<String> = script
        .split([';', '\n'])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    let window = window.clone();
    // The first frame needs the first commit: start after a short delay.
    glib::timeout_add_local_once(Duration::from_millis(50), move || {
        next(window, surface, Rc::new(steps), 0)
    });
}

fn next(window: gtk::ApplicationWindow, surface: Rc<Surface>, steps: Rc<Vec<String>>, i: usize) {
    let Some(step) = steps.get(i) else { return };
    let mut words = step.split_whitespace();
    let cmd = words.next().unwrap_or_default();
    let args: Vec<&str> = words.collect();
    let num = |k: usize| {
        args.get(k)
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0)
    };
    let mut delay = 16u64;
    match cmd {
        "wait" => delay = num(0) as u64,
        "shot" => {
            let path = args.first().copied().unwrap_or("shot.png").to_owned();
            // The step after starts once the shot is taken.
            after_paint(&window.clone(), move |w| {
                shot(w, &path);
                next(window, surface, steps, i + 1);
            });
            return;
        }
        "tap" => {
            crate::input::simulate(&surface, Simulated::Down(num(0), num(1), false));
            let surface = surface.clone();
            let (x, y) = (num(0), num(1));
            glib::timeout_add_local_once(Duration::from_millis(60), move || {
                crate::input::simulate(&surface, Simulated::Up(x, y, false))
            });
            delay = 120;
        }
        "drag" | "touch" => {
            let mouse = cmd == "drag";
            let (x0, y0, x1, y1) = (num(0), num(1), num(2), num(3));
            let ms = if args.len() > 4 { num(4) } else { 300.0 };
            let n = (ms / 16.0).max(1.0) as u32;
            crate::input::simulate(&surface, Simulated::Down(x0, y0, mouse));
            for k in 1..=n {
                let t = k as f32 / n as f32;
                let surface = surface.clone();
                glib::timeout_add_local_once(Duration::from_millis(k as u64 * 16), move || {
                    let p = Simulated::Move(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, mouse);
                    crate::input::simulate(&surface, p);
                    if k == n {
                        crate::input::simulate(&surface, Simulated::Up(x1, y1, mouse));
                    }
                });
            }
            delay = n as u64 * 16 + 60;
        }
        "wheel" => {
            crate::input::simulate(&surface, Simulated::Wheel(num(0), num(1), num(2), num(3)))
        }
        "hover" => crate::input::simulate(&surface, Simulated::Hover(num(0), num(1))),
        "type" => {
            let text = step.split_once(' ').map(|x| x.1).unwrap_or_default();
            type_text(&window, text);
        }
        "mark" => {
            let text = step.split_once(' ').map(|x| x.1).unwrap_or_default();
            mark(&window, text);
        }
        "selectall" => select_all(&window),
        "key" => press_key(&window, &surface, args.first().copied().unwrap_or_default()),
        "dark" | "light" => {
            crate::system::simulate_system_scheme(cmd == "dark");
            delay = 100;
        }
        "resize" => window.set_default_size(num(0) as i32, num(1) as i32),
        "quit" => {
            window.close();
            return;
        }
        other => log::warn!("script: unknown step {other}"),
    }
    glib::timeout_add_local_once(Duration::from_millis(delay), move || {
        next(window, surface, steps, i + 1)
    });
}

/// Runs `f` right after the window's next frame is painted (or after
/// half a second when no frame comes).
///
/// `GtkWidgetPaintable` draws the render nodes widgets kept from their
/// last frame, and a widget queueing a draw drops its node (and those of
/// its ancestors) until the next frame: a web view's page does so every
/// time it paints, so a shot taken between frames came out empty or
/// without the page.
fn after_paint(window: &gtk::ApplicationWindow, f: impl FnOnce(&gtk::ApplicationWindow) + 'static) {
    let Some(clock) = window.frame_clock() else {
        f(window);
        return;
    };
    let f = Rc::new(std::cell::RefCell::new(Some(f)));
    let handler = Rc::new(std::cell::Cell::new(None::<glib::SignalHandlerId>));
    let run = {
        let window = window.clone();
        let clock = clock.clone();
        let handler = handler.clone();
        move || {
            if let Some(id) = handler.take() {
                clock.disconnect(id);
            }
            if let Some(f) = f.borrow_mut().take() {
                f(&window);
            }
        }
    };
    let run = Rc::new(run);
    handler.set(Some(clock.connect_after_paint({
        let run = run.clone();
        move |_| run()
    })));
    glib::timeout_add_local_once(Duration::from_millis(500), move || run());
    if let Some(child) = window.child() {
        child.queue_draw();
    }
}

/// Renders the window's content offscreen and saves it as a PNG.
fn shot(window: &gtk::ApplicationWindow, path: &str) {
    let Some(child) = window.child() else { return };
    let (w, h) = (child.width(), child.height());
    let paintable = gtk::WidgetPaintable::new(Some(&child));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, w as f64, h as f64);
    // An empty surface renders no node: save a blank image all the same.
    let node = snapshot.to_node().unwrap_or_else(|| {
        gtk::gsk::ColorNode::new(
            &gdk::RGBA::TRANSPARENT,
            &gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32),
        )
        .upcast()
    });
    let Some(renderer) = window.renderer() else {
        log::warn!("script: the window has no renderer");
        return;
    };
    let bounds = gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32);
    let texture = renderer.render_texture(&node, Some(&bounds));
    match texture.save_to_png(path) {
        Ok(()) => log::info!("script: saved {path}"),
        Err(err) => log::warn!("script: cannot save {path}: {err}"),
    }
}

fn focused_text(window: &gtk::ApplicationWindow) -> Option<gtk::Widget> {
    let mut focus = gtk::prelude::GtkWindowExt::focus(window)?;
    loop {
        if focus.is::<gtk::Text>() || focus.is::<gtk::TextView>() {
            return Some(focus);
        }
        focus = focus.parent()?;
    }
}

fn select_all(window: &gtk::ApplicationWindow) {
    match focused_text(window) {
        Some(w) => {
            if let Some(t) = w.downcast_ref::<gtk::Text>() {
                t.select_region(0, -1);
            } else if let Some(v) = w.downcast_ref::<gtk::TextView>() {
                let b = v.buffer();
                b.select_range(&b.start_iter(), &b.end_iter());
            }
        }
        None => log::warn!("script: no focused text field to select in"),
    }
}

/// GTK's report of a preedit change, as its input method context makes it
/// (the editor's own context is private).
fn mark(window: &gtk::ApplicationWindow, text: &str) {
    match focused_text(window) {
        Some(w) => w.emit_by_name::<()>("preedit-changed", &[&text]),
        None => log::warn!("script: no focused text field to compose in"),
    }
}

fn type_text(window: &gtk::ApplicationWindow, text: &str) {
    match focused_text(window) {
        Some(w) => {
            if let Some(t) = w.downcast_ref::<gtk::Text>() {
                let mut pos = t.position();
                t.insert_text(text, &mut pos);
                t.set_position(pos);
            } else if let Some(v) = w.downcast_ref::<gtk::TextView>() {
                v.buffer().insert_at_cursor(text);
            }
        }
        None => log::warn!("script: no focused text field to type into"),
    }
}

fn press_key(window: &gtk::ApplicationWindow, surface: &Rc<Surface>, name: &str) {
    let Some(key) = gdk::Key::from_name(name) else {
        log::warn!("script: unknown key {name}");
        return;
    };
    match focused_text(window) {
        Some(w) => {
            if (key == gdk::Key::Return || key == gdk::Key::KP_Enter)
                && let Some(t) = w.downcast_ref::<gtk::Text>()
            {
                t.emit_by_name::<()>("activate", &[]);
                return;
            }
            if key == gdk::Key::BackSpace
                && let Some(t) = w.downcast_ref::<gtk::Text>()
            {
                t.emit_by_name::<()>("backspace", &[]);
                return;
            }
            w.activate_action("misc.insert-emoji", None).ok();
            log::warn!("script: key {name} is not simulated");
        }
        None => {
            if !crate::navigation::key_pressed(surface, key, gdk::ModifierType::empty()) {
                log::warn!("script: no focused widget for key {name}");
            }
        }
    }
}
