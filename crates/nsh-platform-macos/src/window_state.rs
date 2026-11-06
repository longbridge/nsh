//! The window's bounds, mode and display (`windowState()`, the
//! environment's `window_state`), the displays (`displays()`) and
//! `Window.setBounds`, on AppKit.
//!
//! The surface observes the window its root is in: a move, a resize,
//! (de)miniaturizing, entering or leaving full screen, a change of screen
//! and being ordered onto the screen or off it read the state again ([`changed`]); the engine holds a move back
//! until the window stops. The displays are `NSScreen.screens`, sent to
//! the engine before the app starts and again on
//! `NSApplicationDidChangeScreenParametersNotification`
//! ([`crate::system`]). Cocoa's screen coordinates go up from the primary
//! screen's bottom; NSH's go down from its top ([`to_bounds`]).
//!
//! AppKit keeps a zoomed window's frame from before the zoom to itself, so
//! the surface keeps the last frame it saw windowed: a maximized or full
//! screen window's `windowBounds`.

use std::{
    cell::{Cell, RefCell},
    ptr::NonNull,
    rc::Rc,
};

use block2::RcBlock;
use nsh_core::{
    desktop::{Display, DisplayId, WindowBounds, WindowState},
    geometry::{Bounds, Points, point, px, size},
};
use objc2::{
    MainThreadMarker,
    rc::{Retained, Weak},
    runtime::ProtocolObject,
};
use objc2_app_kit::{
    NSApplication, NSScreen, NSWindow, NSWindowDidChangeOcclusionStateNotification,
    NSWindowDidChangeScreenNotification, NSWindowDidDeminiaturizeNotification,
    NSWindowDidEndLiveResizeNotification, NSWindowDidEnterFullScreenNotification,
    NSWindowDidExitFullScreenNotification, NSWindowDidMiniaturizeNotification,
    NSWindowDidMoveNotification, NSWindowDidResizeNotification,
};
use objc2_core_graphics::{
    CGDisplayModelNumber, CGDisplaySerialNumber, CGDisplayUnitNumber, CGDisplayVendorNumber,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize,
    ns_string,
};

use crate::surface::Surface;

/// What a surface keeps of its window.
#[derive(Default)]
pub(crate) struct Watch {
    /// The window observed.
    window: RefCell<Option<Weak<NSWindow>>>,
    /// Its notifications' observers (tokens of the default center).
    observers: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>>,
    /// Its frame when last seen windowed (Cocoa coordinates).
    windowed: Cell<Option<NSRect>>,
    /// Bounds `Window.setBounds` gave while the window was in full screen:
    /// applied once it has left it.
    after_full_screen: Cell<Option<WindowBounds>>,
}

impl Watch {
    fn stop(&self) {
        let center = NSNotificationCenter::defaultCenter();
        for observer in self.observers.borrow_mut().drain(..) {
            // SAFETY: an observer token of the default center.
            unsafe { center.removeObserver(observer.as_ref()) };
        }
        self.window.replace(None);
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The root moved to a window (or out of one): observes that window.
pub(crate) fn watch(surface: &Rc<Surface>) {
    let window = surface.root.ns().window();
    let watched = surface.window.window.borrow().as_ref().and_then(Weak::load);
    if window.as_deref().map(|w| w as *const NSWindow) == watched.as_deref().map(|w| w as *const _)
    {
        return;
    }
    surface.window.stop();
    surface.window.windowed.set(None);
    let Some(window) = window else {
        return;
    };
    let key = surface.key();
    let changed_block = RcBlock::new(move |_: NonNull<NSNotification>| {
        if let Some(surface) = Surface::with_key(key) {
            changed(&surface);
        }
    });
    let left_full_screen = RcBlock::new(move |_: NonNull<NSNotification>| {
        if let Some(surface) = Surface::with_key(key) {
            if let Some(bounds) = surface.window.after_full_screen.take()
                && let Some(window) = surface.root.ns().window()
            {
                set_bounds(&surface, &window, bounds);
            }
            changed(&surface);
        }
    });
    let center = NSNotificationCenter::defaultCenter();
    // SAFETY: immutable AppKit constants.
    let names = unsafe {
        [
            NSWindowDidMoveNotification,
            NSWindowDidResizeNotification,
            NSWindowDidEndLiveResizeNotification,
            NSWindowDidMiniaturizeNotification,
            NSWindowDidDeminiaturizeNotification,
            NSWindowDidEnterFullScreenNotification,
            NSWindowDidChangeScreenNotification,
            // Ordered onto the screen or off it.
            NSWindowDidChangeOcclusionStateNotification,
        ]
    };
    let mut observers = Vec::with_capacity(names.len() + 1);
    for name in names {
        // SAFETY: AppKit posts these on the main thread, where the block
        // runs (no queue: at once, so the state is current after a
        // command that moved the window); the observer is removed in
        // `stop`.
        observers.push(unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(name),
                Some(&window),
                None,
                &changed_block,
            )
        });
    }
    // SAFETY: as above.
    observers.push(unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWindowDidExitFullScreenNotification),
            Some(&window),
            None,
            &left_full_screen,
        )
    });
    surface.window.observers.replace(observers);
    surface
        .window
        .window
        .replace(Some(Weak::from_retained(&window)));
}

/// The window changed: the environment gets its state.
pub(crate) fn changed(surface: &Surface) {
    let state = read(surface);
    let mut env = surface.environment.get();
    if env.window_state != state {
        env.window_state = state;
        surface.set_environment(env);
    }
}

/// The state of the surface's window, `None` without one.
pub(crate) fn read(surface: &Surface) -> Option<WindowState> {
    let window = surface.root.ns().window()?;
    let primary = primary_height(surface.mtm);
    let frame = window.frame();
    let full = crate::chrome::full_screen(&window);
    let zoomed = !full && window.isZoomed();
    let minimized = window.isMiniaturized();
    // A minimized window is not visible either: it is `minimized` alone.
    let hidden = !minimized
        && (NSApplication::sharedApplication(surface.mtm).isHidden() || !window.isVisible());
    if !full && !zoomed && !minimized {
        surface.window.windowed.set(Some(frame));
    }
    let bounds = to_bounds(frame, primary);
    let windowed = to_bounds(surface.window.windowed.get().unwrap_or(frame), primary);
    let window_bounds = if full {
        WindowBounds::Fullscreen(windowed)
    } else if zoomed {
        WindowBounds::Maximized(windowed)
    } else {
        WindowBounds::Windowed(bounds)
    };
    let mut state = WindowState::new(bounds, window_bounds);
    state.minimized = minimized;
    state.hidden = hidden;
    state.display = window.screen().map(|s| display(&s, primary));
    Some(state)
}

/// The displays, the primary one first.
pub(crate) fn displays(mtm: MainThreadMarker) -> Vec<Display> {
    let primary = primary_height(mtm);
    NSScreen::screens(mtm)
        .iter()
        .map(|s| display(&s, primary))
        .collect()
}

/// `Window.setBounds`: the window restored to `bounds`' windowed bounds,
/// then zoomed or in full screen as its mode says. Leaving full screen
/// takes AppKit's animation: the bounds apply once it is done.
pub(crate) fn set_bounds(surface: &Surface, window: &NSWindow, bounds: WindowBounds) {
    let primary = primary_height(surface.mtm);
    let frame = from_bounds(bounds.get_bounds(), primary);
    let full = crate::chrome::full_screen(window);
    match bounds {
        WindowBounds::Fullscreen(_) if full => {
            // Where it returns to.
            surface.window.windowed.set(Some(frame));
        }
        WindowBounds::Fullscreen(_) => {
            window.setFrame_display(frame, true);
            surface.window.windowed.set(Some(frame));
            window.toggleFullScreen(None);
        }
        _ if full => {
            surface.window.after_full_screen.set(Some(bounds));
            window.toggleFullScreen(None);
        }
        WindowBounds::Windowed(_) => window.setFrame_display(frame, true),
        WindowBounds::Maximized(_) => {
            window.setFrame_display(frame, true);
            surface.window.windowed.set(Some(frame));
            if !window.isZoomed() {
                window.zoom(None);
            }
        }
    }
    changed(surface);
}

/// The height of the primary screen, whose bottom left corner Cocoa's
/// screen coordinates start from.
fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .firstObject()
        .map_or(0.0, |s| s.frame().size.height)
}

/// `screen` as a display.
fn display(screen: &NSScreen, primary_height: f64) -> Display {
    let frame = screen.frame();
    let id = screen_number(screen).map_or(DisplayId::default(), stable_id);
    Display::new(
        id,
        to_bounds(frame, primary_height),
        to_bounds(screen.visibleFrame(), primary_height),
        frame.origin.x == 0.0 && frame.origin.y == 0.0,
    )
}

/// The screen's display id (`CGDirectDisplayID`).
fn screen_number(screen: &NSScreen) -> Option<u32> {
    let number = screen
        .deviceDescription()
        .objectForKey(ns_string!("NSScreenNumber"))?;
    let number = number.downcast::<NSNumber>().ok()?;
    Some(number.unsignedIntValue())
}

/// An id of display `number` that stays from one launch to the next: its
/// vendor, model and serial number (the unit number for a display that has
/// no serial number, telling identical ones apart), where the display id
/// itself may change.
fn stable_id(number: u32) -> DisplayId {
    let vendor = CGDisplayVendorNumber(number);
    let model = CGDisplayModelNumber(number);
    let serial = CGDisplaySerialNumber(number);
    let name = if serial != 0 {
        format!("{vendor:x}:{model:x}:{serial:x}")
    } else {
        format!("{vendor:x}:{model:x}:unit{}", CGDisplayUnitNumber(number))
    };
    DisplayId::from_name(name.as_bytes())
}

/// A Cocoa screen rectangle (from the primary screen's bottom, `y` up) in
/// NSH's coordinates (from its top, `y` down).
fn to_bounds(rect: NSRect, primary_height: f64) -> Bounds<Points> {
    let top = primary_height - (rect.origin.y + rect.size.height);
    Bounds::new(
        point(px(rect.origin.x as f32), px(top as f32)),
        size(px(rect.size.width as f32), px(rect.size.height as f32)),
    )
}

/// [`to_bounds`]' inverse.
fn from_bounds(bounds: Bounds<Points>, primary_height: f64) -> NSRect {
    let height = f64::from(bounds.size.height.0);
    NSRect::new(
        NSPoint::new(
            f64::from(bounds.origin.x.0),
            primary_height - f64::from(bounds.origin.y.0) - height,
        ),
        NSSize::new(f64::from(bounds.size.width.0), height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_rectangles_flip_to_the_primary_screens_top() {
        // A window 600 tall whose top is 100 below a 900 tall primary
        // screen's top.
        let rect = NSRect::new(NSPoint::new(40.0, 200.0), NSSize::new(800.0, 600.0));
        let bounds = to_bounds(rect, 900.0);
        assert_eq!(
            bounds,
            Bounds::new(point(px(40.0), px(100.0)), size(px(800.0), px(600.0)))
        );
        assert_eq!(from_bounds(bounds, 900.0), rect);
        // A display above the primary one has negative `y`s.
        let above = NSRect::new(NSPoint::new(0.0, 900.0), NSSize::new(1920.0, 1080.0));
        assert_eq!(to_bounds(above, 900.0).origin.y, px(-1080.0));
    }
}
