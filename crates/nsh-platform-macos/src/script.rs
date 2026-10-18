//! Steps run once the window shows (`--script`, [`crate::AppOptions::script`]):
//! screenshots and simulated input, to check a change without touching
//! the screen.
//!
//! Steps are separated by `;` or new lines:
//!
//! | Step | Does |
//! |---|---|
//! | `wait <ms>` | Waits. |
//! | `shot <file.png>` | Renders the window's content to a PNG (in process, whatever covers the window). |
//! | `tap [mods] <x> <y> [count]` | Presses and releases the left button at a point (points, from the content's top left); `count` clicks in a row (2: a double click); `mods` as for `key`, without the key (`tap shift 10 20`: a shift-click). |
//! | `down [mods] <x> <y>` / `up <x> <y>` | Presses the left button and keeps it down (pressed states, long presses) / releases it. |
//! | `rtap <x> <y>` | Presses and releases the right button (a secondary click). |
//! | `drag <x0> <y0> <x1> <y1> [ms]` | Moves the pointer to the start, presses, moves in steps a frame apart over `ms` (default 300), releases. |
//! | `wheel <x> <y> <dx> <dy> [ms]` | A trackpad scroll gesture by `(dx, dy)` points over `ms` (default 100): phase began, changed events a frame apart, ended; no momentum. Positive moves the content up / left. |
//! | `fling <x> <y> <dx> <dy> [ms]` | A trackpad flick as a ProMotion Mac sends it: phase began, changed events 8 ms apart carrying `(dx, dy)` over `ms` (default 150), ended, then the system's momentum (began, changed events 8 ms apart decaying from the flick's speed, ended). |
//! | `notch <x> <y> <dx> <dy>` | A mouse wheel: one event of `(dx, dy)` lines, without phases. Positive moves the content up / left. |
//! | `hover <x> <y>` | Moves the pointer. |
//! | `type <text>` | Types into the focused field. |
//! | `mark <text>` | Sets the focused field's marked text, as an input method composing it does (a `key Return` then commits it). |
//! | `key <name>` | Presses a key: `Return`, `Escape`, `Tab`, `BackSpace`, `Up`, `Down`, `Left`, `Right`, `Page_Up`, `Page_Down`, `Home`, `End`, `Delete`, `F1` to `F12`, `space`, or one character (with its US layout key code); with modifiers before it, as in `cmd+a`, `shift+Tab` (`cmd`, `shift`, `alt`, `ctrl`). Keys go through the application (menus, key monitors such as `nsh/keymap`'s) while the window is key, else through the window (keys with `cmd` always through the application). |
//! | `dark` / `light` | Switches the app's window to that appearance (as the system's would). |
//! | `resize <w> <h>` | Resizes the window's content. |
//! | `zoom` | Zooms the window, or unzooms it (the green button, Window > Zoom): AppKit's animated resize. |
//! | `fullscreen` | Enters full screen, or leaves it (Window > Enter Full Screen). |
//! | `quit` | Closes the window. |
//!
//! Mouse button and move events go through the application's event queue
//! (`postEvent:atStart:`), wheels through the window and keys through
//! the application (`sendEvent:`, see `key`), so they take the path real
//! ones take: AppKit's hit testing, the root view, [`crate::input`]. A
//! wheel is the event the root's `hitTest:` sees while the window handles
//! it, as `NSApp.currentEvent` would be ([`crate::view::scripted`]).
//! Presses make the app active first: WebKit takes left presses only in
//! the active app. `hover` makes the window accept mouse moves (sent moves
//! reach the first responder only then; real ones come through the root's
//! tracking area).

use std::{rc::Rc, time::Duration};

use dispatch2::{DispatchQueue, DispatchTime};
use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSEvent, NSEventModifierFlags, NSEventType, NSWindow,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSPoint, NSSize, NSString};

use crate::surface::Surface;

/// One step of a script.
#[derive(Debug, Clone, PartialEq)]
enum Step {
    Wait(u64),
    Shot(String),
    Tap(f64, f64, u32, NSEventModifierFlags),
    Down(f64, f64, NSEventModifierFlags),
    Up(f64, f64),
    RightTap(f64, f64),
    Drag(f64, f64, f64, f64, u64),
    Wheel(f64, f64, f64, f64, u64),
    Fling(f64, f64, f64, f64, u64),
    Notch(f64, f64, f64, f64),
    Hover(f64, f64),
    /// A drag's move (internal: [`Step::Drag`] queues them).
    DragTo(f64, f64),
    /// A drag's release (internal).
    Release(f64, f64),
    /// One event of a wheel gesture (internal: [`Step::Wheel`] queues
    /// them): a point delta, a `CGScrollPhase`, a `CGMomentumScrollPhase`
    /// and the delay before the next step.
    WheelEvent(f64, f64, i32, i32, i64, i64, u64),
    Type(String),
    /// Marked text: an input method's composition in progress.
    Mark(String),
    Key(String),
    Appearance(bool),
    Resize(f64, f64),
    Zoom,
    FullScreen,
    Quit,
}

/// Parses `script` (unknown steps are logged and skipped).
fn parse(script: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    for raw in script.split([';', '\n']) {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let (name, rest) = raw.split_once(' ').unwrap_or((raw, ""));
        let mut rest = rest.trim();
        // Pointer steps may start with modifiers (`tap shift 10 20`).
        let mut mods = NSEventModifierFlags::empty();
        if matches!(name, "tap" | "down")
            && let Some((first, after)) = rest.split_once(' ')
            && let Some(flags) = modifiers(first)
        {
            mods = flags;
            rest = after.trim();
        }
        let nums: Vec<f64> = rest
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        let step = match (name, nums.as_slice()) {
            ("wait", [ms]) => Some(Step::Wait(*ms as u64)),
            ("shot", _) if !rest.is_empty() => Some(Step::Shot(rest.to_owned())),
            ("tap", [x, y]) => Some(Step::Tap(*x, *y, 1, mods)),
            ("tap", [x, y, n]) if *n >= 1.0 => Some(Step::Tap(*x, *y, *n as u32, mods)),
            ("down", [x, y]) => Some(Step::Down(*x, *y, mods)),
            ("up", [x, y]) => Some(Step::Up(*x, *y)),
            ("rtap", [x, y]) => Some(Step::RightTap(*x, *y)),
            ("drag", [x0, y0, x1, y1]) => Some(Step::Drag(*x0, *y0, *x1, *y1, 300)),
            ("drag", [x0, y0, x1, y1, ms]) => Some(Step::Drag(*x0, *y0, *x1, *y1, *ms as u64)),
            ("wheel", [x, y, dx, dy]) => Some(Step::Wheel(*x, *y, *dx, *dy, 100)),
            ("wheel", [x, y, dx, dy, ms]) => Some(Step::Wheel(*x, *y, *dx, *dy, *ms as u64)),
            ("fling", [x, y, dx, dy]) => Some(Step::Fling(*x, *y, *dx, *dy, 150)),
            ("fling", [x, y, dx, dy, ms]) => Some(Step::Fling(*x, *y, *dx, *dy, *ms as u64)),
            ("notch", [x, y, dx, dy]) => Some(Step::Notch(*x, *y, *dx, *dy)),
            ("hover", [x, y]) => Some(Step::Hover(*x, *y)),
            ("type", _) => Some(Step::Type(rest.to_owned())),
            ("mark", _) => Some(Step::Mark(rest.to_owned())),
            ("key", _) if !rest.is_empty() => Some(Step::Key(rest.to_owned())),
            ("dark", []) => Some(Step::Appearance(true)),
            ("light", []) => Some(Step::Appearance(false)),
            ("resize", [w, h]) => Some(Step::Resize(*w, *h)),
            ("zoom", []) => Some(Step::Zoom),
            ("fullscreen", []) => Some(Step::FullScreen),
            ("quit", []) => Some(Step::Quit),
            _ => None,
        };
        match step {
            Some(step) => steps.push(step),
            None => log::warn!("script: cannot read step `{raw}`"),
        }
    }
    steps
}

/// Runs `script`'s steps in `window`, one after the other, starting after
/// the window's first frames.
pub(crate) fn run(window: &NSWindow, surface: Rc<Surface>, script: &str) {
    let steps = parse(script);
    let window = Retained::from(window);
    let key = surface.key();
    drop(surface);
    // The window number identifies it for `screencapture`; the rest needs
    // the window itself, on the main thread.
    let state = Rc::new(Runner {
        window,
        _surface: key,
        steps: std::cell::RefCell::new(steps.into_iter().collect()),
        activations: std::cell::Cell::new(0),
    });
    RUNNER.with(|r| *r.borrow_mut() = Some(state));
    later(300, next);
}

struct Runner {
    window: Retained<NSWindow>,
    /// The surface's key (kept for steps that will need it).
    _surface: u64,
    steps: std::cell::RefCell<std::collections::VecDeque<Step>>,
    /// Attempts to make the app active since it last was (presses wait
    /// for it).
    activations: std::cell::Cell<u32>,
}

thread_local! {
    static RUNNER: std::cell::RefCell<Option<Rc<Runner>>> = const { std::cell::RefCell::new(None) };
}

/// Calls `f` on the main thread after `ms` milliseconds.
fn later(ms: u64, f: fn()) {
    let when = DispatchTime::NOW.time(Duration::from_millis(ms).as_nanos() as i64);
    if DispatchQueue::main().after(when, f).is_err() {
        DispatchQueue::main().exec_async(f);
    }
}

/// Runs the next step, then schedules the one after it.
fn next() {
    let Some(runner) = RUNNER.with(|r| r.borrow().clone()) else {
        return;
    };
    let Some(step) = runner.steps.borrow_mut().pop_front() else {
        return;
    };
    let delay = run_step(&runner, step);
    if let Some(delay) = delay {
        later(delay, next);
    }
}

/// Runs `step`; returns the delay before the next one (`None`: no next).
fn run_step(runner: &Runner, step: Step) -> Option<u64> {
    let window = &*runner.window;
    // WebKit takes left presses only in the active app (in another, a
    // click into a page just activates it), and another app may have
    // become active since the script started: activate first, then press.
    if matches!(step, Step::Tap(..) | Step::Down(..) | Step::Drag(..)) {
        let app = objc2_app_kit::NSApplication::sharedApplication(MainThreadMarker::from(window));
        if app.isActive() {
            runner.activations.set(0);
        } else if runner.activations.get() < 20 {
            // Given up after a second (a locked screen): press anyway.
            runner.activations.set(runner.activations.get() + 1);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
            runner.steps.borrow_mut().push_front(step);
            return Some(50);
        }
    }
    match step {
        Step::Wait(ms) => return Some(ms),
        Step::Shot(path) => shot(window, &path),
        Step::Tap(x, y, count, mods) => {
            for clicks in 1..=count as isize {
                mouse_with(window, NSEventType::LeftMouseDown, x, y, clicks, mods);
                mouse_with(window, NSEventType::LeftMouseUp, x, y, clicks, mods);
            }
        }
        Step::Down(x, y, mods) => {
            // A real pointer arrives before it presses (hover state).
            mouse(window, NSEventType::MouseMoved, x, y, 0);
            mouse_with(window, NSEventType::LeftMouseDown, x, y, 1, mods);
        }
        Step::Up(x, y) => mouse(window, NSEventType::LeftMouseUp, x, y, 1),
        Step::RightTap(x, y) => {
            mouse(window, NSEventType::RightMouseDown, x, y, 1);
            mouse(window, NSEventType::RightMouseUp, x, y, 1);
        }
        Step::Drag(x0, y0, x1, y1, ms) => {
            // A real pointer arrives before it presses (hover state).
            mouse(window, NSEventType::MouseMoved, x0, y0, 0);
            mouse(window, NSEventType::LeftMouseDown, x0, y0, 1);
            // The moves run as steps of their own, a frame apart.
            let frames = (ms / 16).max(1);
            let mut queued: Vec<Step> = Vec::new();
            for i in 1..=frames {
                let t = i as f64 / frames as f64;
                queued.push(Step::DragTo(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t));
            }
            queued.push(Step::Release(x1, y1));
            let mut steps = runner.steps.borrow_mut();
            for s in queued.into_iter().rev() {
                steps.push_front(s);
            }
        }
        Step::DragTo(x, y) => mouse(window, NSEventType::LeftMouseDragged, x, y, 1),
        Step::Release(x, y) => mouse(window, NSEventType::LeftMouseUp, x, y, 1),
        Step::Hover(x, y) => {
            // Synthesized moves reach the first responder only so.
            window.setAcceptsMouseMovedEvents(true);
            mouse(window, NSEventType::MouseMoved, x, y, 0);
        }
        Step::Wheel(x, y, dx, dy, ms) => {
            // CGScrollPhase: began, then changed events a frame apart
            // carrying the distance, then ended (with no momentum after).
            let frames = (ms / 16).max(1) as i64;
            let mut queued = vec![Step::WheelEvent(x, y, 0, 0, 1, 0, 16)];
            gesture_events(&mut queued, x, y, dx, dy, frames, 16);
            queued.push(Step::WheelEvent(x, y, 0, 0, 4, 0, 16));
            let mut steps = runner.steps.borrow_mut();
            for s in queued.into_iter().rev() {
                steps.push_front(s);
            }
        }
        Step::Fling(x, y, dx, dy, ms) => {
            // 120 Hz, as trackpads report on ProMotion Macs; the momentum
            // starts at the flick's speed and decays as AppKit's does
            // (about 0.95 per 8 ms). The events are timed by a thread of
            // their own, as the system's come, whatever the main thread
            // does meanwhile.
            let frames = (ms / 8).max(1) as i64;
            let mut queued = vec![Step::WheelEvent(x, y, 0, 0, 1, 0, 8)];
            gesture_events(&mut queued, x, y, dx, dy, frames, 8);
            queued.push(Step::WheelEvent(x, y, 0, 0, 4, 0, 8));
            let (mut vx, mut vy) = (dx / frames as f64, dy / frames as f64);
            let (mut fx, mut fy) = (0.0, 0.0);
            let mut momentum = 1;
            while vx.abs() + vy.abs() > 0.5 {
                fx += vx;
                fy += vy;
                let (sx, sy) = (fx.round(), fy.round());
                (fx, fy) = (fx - sx, fy - sy);
                queued.push(Step::WheelEvent(x, y, sx as i32, sy as i32, 0, momentum, 8));
                momentum = 2;
                vx *= 0.95;
                vy *= 0.95;
            }
            queued.push(Step::WheelEvent(x, y, 0, 0, 0, 3, 8));
            std::thread::spawn(move || {
                let start = std::time::Instant::now();
                let period = Duration::from_micros(8333);
                for (i, step) in queued.into_iter().enumerate() {
                    let at = start + period * i as u32;
                    if let Some(wait) = at.checked_duration_since(std::time::Instant::now()) {
                        std::thread::sleep(wait);
                    }
                    let Step::WheelEvent(x, y, dx, dy, phase, momentum, _) = step else {
                        continue;
                    };
                    // Stamped when it happens, as the system stamps its
                    // events, whenever the main thread gets to it.
                    let at = timestamp();
                    DispatchQueue::main().exec_async(move || {
                        if let Some(runner) = RUNNER.with(|r| r.borrow().clone()) {
                            wheel_with(&runner.window, x, y, dx, dy, Some(phase), momentum, at);
                        }
                    });
                }
                DispatchQueue::main().exec_async(next);
            });
            return None;
        }
        Step::WheelEvent(x, y, dx, dy, phase, momentum, delay) => {
            wheel_with(window, x, y, dx, dy, Some(phase), momentum, timestamp());
            return Some(delay);
        }
        Step::Notch(x, y, dx, dy) => {
            wheel(window, x, y, dx.round() as i32, dy.round() as i32, None)
        }
        Step::Type(text) => type_text(window, &text),
        Step::Mark(text) => mark(window, &text),
        Step::Key(name) => key(window, &name),
        Step::Appearance(dark) => {
            // SAFETY: immutable AppKit constants.
            let name = if dark {
                unsafe { NSAppearanceNameDarkAqua }
            } else {
                unsafe { NSAppearanceNameAqua }
            };
            window.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
        }
        Step::Resize(w, h) => window.setContentSize(NSSize::new(w, h)),
        Step::Zoom => window.performZoom(None),
        Step::FullScreen => window.toggleFullScreen(None),
        Step::Quit => {
            window.close();
            return None;
        }
    }
    Some(16)
}

/// A point of the content view (points from its top left) in window
/// coordinates (from its bottom left).
fn to_window(window: &NSWindow, x: f64, y: f64) -> NSPoint {
    let height = window.contentView().map_or(0.0, |v| v.frame().size.height);
    NSPoint::new(x as CGFloat, height - y as CGFloat)
}

fn mouse(window: &NSWindow, kind: NSEventType, x: f64, y: f64, clicks: isize) {
    mouse_with(window, kind, x, y, clicks, NSEventModifierFlags::empty());
}

fn mouse_with(
    window: &NSWindow,
    kind: NSEventType,
    x: f64,
    y: f64,
    clicks: isize,
    mods: NSEventModifierFlags,
) {
    let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        kind,
        to_window(window, x, y),
        mods,
        timestamp(),
        window.windowNumber(),
        None,
        0,
        clicks,
        if clicks > 0 { 1.0 } else { 0.0 },
    );
    if let Some(event) = event {
        // Through the event queue, as real clicks come: the event becomes
        // `NSApp.currentEvent` while it is handled (responders look at it).
        // The run loop handles it before the next step's timer.
        objc2_app_kit::NSApplication::sharedApplication(MainThreadMarker::from(window))
            .postEvent_atStart(&event, false);
    }
}

/// Seconds since the system started, as event timestamps are.
fn timestamp() -> f64 {
    objc2_foundation::NSProcessInfo::processInfo().systemUptime()
}

/// Queues the changed events of a gesture by `(dx, dy)` over `frames`
/// events `delay` ms apart: whole points (as the event carries them) that
/// add up to the distance.
fn gesture_events(
    queued: &mut Vec<Step>,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    frames: i64,
    delay: u64,
) {
    let (mut sent_x, mut sent_y) = (0i32, 0i32);
    for i in 1..=frames {
        let t = i as f64 / frames as f64;
        let (to_x, to_y) = ((dx * t).round() as i32, (dy * t).round() as i32);
        queued.push(Step::WheelEvent(
            x,
            y,
            to_x - sent_x,
            to_y - sent_y,
            2,
            0,
            delay,
        ));
        (sent_x, sent_y) = (to_x, to_y);
    }
}

/// A scroll of `(dx, dy)`: points of a trackpad gesture in `phase` (a
/// `CGScrollPhase`), or lines of a mouse wheel (`None`).
fn wheel(window: &NSWindow, x: f64, y: f64, dx: i32, dy: i32, phase: Option<i64>) {
    wheel_with(window, x, y, dx, dy, phase, 0, timestamp());
}

/// [`wheel`], in a `CGMomentumScrollPhase` (`0`: none), stamped `at`
/// (seconds since the system started).
#[allow(clippy::too_many_arguments)]
fn wheel_with(
    window: &NSWindow,
    x: f64,
    y: f64,
    dx: i32,
    dy: i32,
    phase: Option<i64>,
    momentum: i64,
    at: f64,
) {
    use objc2_core_graphics::{CGEvent, CGEventField, CGScrollEventUnit};
    // Content follows the fingers (or the wheel), so a positive `dy`
    // (content up) is a negative delta.
    let unit = match phase {
        Some(_) => CGScrollEventUnit::Pixel,
        None => CGScrollEventUnit::Line,
    };
    let Some(event) = CGEvent::new_scroll_wheel_event2(None, unit, 2, -dy, -dx, 0) else {
        return;
    };
    // A window-less event's `locationInWindow` is its screen point, which
    // `sendEvent:` takes as a window point: place it so that they match.
    let p = to_window(window, x, y);
    let main_height = objc2_app_kit::NSScreen::screens(MainThreadMarker::from(window))
        .firstObject()
        .map_or(0.0, |s| s.frame().size.height);
    CGEvent::set_location(
        Some(&event),
        objc2_core_foundation::CGPoint::new(p.x, main_height - p.y),
    );
    if let Some(phase) = phase {
        CGEvent::set_integer_value_field(
            Some(&event),
            CGEventField::ScrollWheelEventScrollPhase,
            phase,
        );
    }
    if momentum != 0 {
        CGEvent::set_integer_value_field(
            Some(&event),
            CGEventField::ScrollWheelEventMomentumPhase,
            momentum,
        );
    }
    // Nanoseconds, as `NSEvent.timestamp` reads it.
    CGEvent::set_timestamp(Some(&event), (at * 1e9) as u64);
    let Some(ns) = NSEvent::eventWithCGEvent(&event) else {
        return;
    };
    if momentum == 0 {
        crate::view::scripted(&ns, || window.sendEvent(&ns));
        return;
    }
    // The window drops synthesized momentum events (the system's go to
    // the view that took the gesture): hand them to the view under the
    // pointer, as AppKit would.
    let view = window.contentView().and_then(|content| {
        let local = content.convertPoint_fromView(ns.locationInWindow(), None);
        // SAFETY: a superview's point, as hitTest: takes.
        let outer = unsafe { content.superview() };
        let p = outer.map_or(local, |o| {
            o.convertPoint_fromView(ns.locationInWindow(), None)
        });
        content.hitTest(p)
    });
    if let Some(view) = view {
        crate::view::scripted(&ns, || view.scrollWheel(&ns));
    }
}

fn type_text(window: &NSWindow, text: &str) {
    for c in text.chars() {
        let s = c.to_string();
        key_event(window, &s, 0, NSEventModifierFlags::empty());
    }
}

/// Sets `text` as the focused text view's marked text, as an input method
/// composing it would (empty: unmarks it).
fn mark(window: &NSWindow, text: &str) {
    use objc2_app_kit::{NSTextInputClient, NSTextView};
    use objc2_foundation::NSRange;
    let Some(responder) = window.firstResponder() else {
        return;
    };
    let Some(view) = responder.downcast_ref::<NSTextView>() else {
        return log::error!("script: mark needs a focused text field");
    };
    let s = NSString::from_str(text);
    let len = s.length();
    // SAFETY: an NSString, as the protocol takes; the ranges are within it
    // and NSNotFound replaces the current marked text or selection.
    unsafe {
        view.setMarkedText_selectedRange_replacementRange(
            &s,
            NSRange::new(len, 0),
            NSRange::new(objc2_foundation::NSNotFound as usize, 0),
        )
    };
}

/// One modifier's flag.
fn modifier(name: &str) -> Option<NSEventModifierFlags> {
    Some(match name {
        "cmd" => NSEventModifierFlags::Command,
        "shift" => NSEventModifierFlags::Shift,
        "alt" => NSEventModifierFlags::Option,
        "ctrl" => NSEventModifierFlags::Control,
        _ => return None,
    })
}

/// Modifiers alone, `+`-separated (`cmd+shift`).
fn modifiers(names: &str) -> Option<NSEventModifierFlags> {
    names
        .split('+')
        .try_fold(NSEventModifierFlags::empty(), |flags, m| {
            Some(flags | modifier(m)?)
        })
}

fn key(window: &NSWindow, name: &str) {
    let mut flags = NSEventModifierFlags::empty();
    let mut name = name;
    // `cmd++` is Command and `+`.
    while let Some((m, rest)) = name.split_once('+').filter(|(_, r)| !r.is_empty()) {
        let Some(flag) = modifier(m) else { break };
        flags |= flag;
        name = rest;
    }
    // (characters, virtual key code)
    let (chars, code): (String, u16) = match name {
        "Return" => ("\r".into(), 36),
        "Escape" => ("\u{1b}".into(), 53),
        // Shift+Tab types a backtab, which is what field editors read.
        "Tab" if flags.contains(NSEventModifierFlags::Shift) => ("\u{19}".into(), 48),
        "Tab" => ("\t".into(), 48),
        "BackSpace" => ("\u{7f}".into(), 51),
        "space" => (" ".into(), 49),
        "Up" => ("\u{f700}".into(), 126),
        "Down" => ("\u{f701}".into(), 125),
        "Left" => ("\u{f702}".into(), 123),
        "Right" => ("\u{f703}".into(), 124),
        "Page_Up" => ("\u{f72c}".into(), 116),
        "Page_Down" => ("\u{f72d}".into(), 121),
        "Home" => ("\u{f729}".into(), 115),
        "End" => ("\u{f72b}".into(), 119),
        "Delete" => ("\u{f728}".into(), 117),
        // F1 to F12 (`F5`).
        f if f.len() > 1
            && f.starts_with('F')
            && let Some(n) = f[1..].parse::<u16>().ok().filter(|n| (1..=12).contains(n)) =>
        {
            const CODES: [u16; 12] = [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111];
            let c = char::from_u32(0xf703 + u32::from(n)).unwrap_or('\0');
            (c.to_string(), CODES[usize::from(n - 1)])
        }
        // Shift types the shifted character, which menus' key
        // equivalents match (`cmd+shift+z` is Redo's `Z`). A character's
        // key is the US layout's, as a real event's would be.
        other => {
            let mut chars = other.chars();
            let code = match (chars.next(), chars.next()) {
                (Some(c), None) => crate::input::us_key_code(c).unwrap_or(0),
                _ => 0,
            };
            if flags.contains(NSEventModifierFlags::Shift) {
                (other.to_uppercase(), code)
            } else {
                (other.into(), code)
            }
        }
    };
    key_event(window, &chars, code, flags);
}

fn key_event(window: &NSWindow, chars: &str, code: u16, flags: NSEventModifierFlags) {
    let s = NSString::from_str(chars);
    for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            kind,
            NSPoint::new(0.0, 0.0),
            flags,
            timestamp(),
            window.windowNumber(),
            None,
            &s,
            &s,
            false,
            code,
        );
        let Some(event) = event else { continue };
        // Key equivalents (menus) and key monitors (`nsh/keymap`) are the
        // application's, which hands the key to its key window.
        if window.isKeyWindow() || flags.contains(NSEventModifierFlags::Command) {
            objc2_app_kit::NSApplication::sharedApplication(MainThreadMarker::from(window))
                .sendEvent(&event);
        } else {
            window.sendEvent(&event);
        }
    }
}

/// Renders the window's content into a PNG (in process: no Screen
/// Recording permission, and it works while the screen is locked).
fn shot(window: &NSWindow, path: &str) {
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRepPropertyKey};
    use objc2_foundation::NSDictionary;
    let Some(view) = window.contentView() else {
        return;
    };
    if let Some(layer) = view.layer() {
        crate::surface::quiet(|| order_by_z(&layer));
    }
    let bounds = view.bounds();
    let Some(rep) = view.bitmapImageRepForCachingDisplayInRect(bounds) else {
        return log::error!("script: cannot make a bitmap for {path}");
    };
    view.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
    let properties = NSDictionary::<NSBitmapImageRepPropertyKey, objc2::runtime::AnyObject>::new();
    // SAFETY: an empty property dictionary is valid for PNG.
    let data =
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties) };
    match data.map(|d| std::fs::write(path, d.to_vec())) {
        Some(Ok(())) => log::info!("script: saved {path}"),
        other => log::error!("script: cannot save {path}: {other:?}"),
    }
}

/// Orders the sublayers of `layer` and its sublayers by their `zPosition`
/// (keeping their order where it is equal), as Core Animation composites
/// them: `cacheDisplayInRect:` draws sublayers in their order and ignores
/// `zPosition`, so a decoration added before a layer it is above (a
/// border's stroke before a gradient) would look covered in the shot. The
/// window looks the same either way.
fn order_by_z(layer: &objc2_quartz_core::CALayer) {
    // SAFETY: the main thread reads the layer tree, which nothing else
    // changes meanwhile.
    let Some(sublayers) = (unsafe { layer.sublayers() }) else {
        return;
    };
    let layers: Vec<Retained<objc2_quartz_core::CALayer>> = sublayers.to_vec();
    let sorted = layers
        .windows(2)
        .all(|w| w[0].zPosition() <= w[1].zPosition());
    if !sorted {
        let mut by_z = layers.clone();
        by_z.sort_by(|a, b| a.zPosition().total_cmp(&b.zPosition()));
        for (i, l) in by_z.iter().enumerate() {
            layer.insertSublayer_atIndex(l, i as u32);
        }
    }
    for l in &layers {
        order_by_z(l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steps() {
        assert_eq!(
            parse(
                "wait 500; shot a.png\ntap 10 20; drag 1 2 3 4 100; key Return; quit; nope; \
                 tap 1 2 2; rtap 3 4; wheel 1 2 0 50; notch 1 2 0 -3; fling 1 2 0 300 200"
            ),
            vec![
                Step::Wait(500),
                Step::Shot("a.png".into()),
                Step::Tap(10.0, 20.0, 1, NSEventModifierFlags::empty()),
                Step::Drag(1.0, 2.0, 3.0, 4.0, 100),
                Step::Key("Return".into()),
                Step::Quit,
                Step::Tap(1.0, 2.0, 2, NSEventModifierFlags::empty()),
                Step::RightTap(3.0, 4.0),
                Step::Wheel(1.0, 2.0, 0.0, 50.0, 100),
                Step::Notch(1.0, 2.0, 0.0, -3.0),
                Step::Fling(1.0, 2.0, 0.0, 300.0, 200),
            ]
        );
    }

    #[test]
    fn parses_modifiers_and_holds() {
        assert_eq!(
            parse("tap shift 5 6; tap cmd+shift 1 2 2; down alt 3 4; up 3 4"),
            vec![
                Step::Tap(5.0, 6.0, 1, NSEventModifierFlags::Shift),
                Step::Tap(
                    1.0,
                    2.0,
                    2,
                    NSEventModifierFlags::Command | NSEventModifierFlags::Shift
                ),
                Step::Down(3.0, 4.0, NSEventModifierFlags::Option),
                Step::Up(3.0, 4.0),
            ]
        );
    }
}
