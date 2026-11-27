//! A tray item with an `onClick` reports a click to the app, and its menu
//! stays reachable by VoiceOver: its button offers `AXShowMenu` (VO-Shift-M)
//! beside `AXPress`, and the menu it shows is the tray's.

use std::{
    sync::atomic::{AtomicU8, Ordering},
    time::{Duration, Instant},
};

use nsh_core::{
    element::desktop::{MenuItem, set_tray, tray},
    prelude::*,
};
use nsh_platform_macos as platform;
use platform::{
    objc2::{MainThreadMarker, MainThreadOnly, Message, rc::Retained},
    objc2_app_kit::{
        NSAccessibility, NSApplication, NSBackingStoreType, NSEvent, NSEventModifierFlags,
        NSEventType, NSObjectNSAccessibility, NSStatusBarButton, NSView, NSWindow,
        NSWindowStyleMask,
    },
    objc2_foundation::{NSDate, NSPoint, NSRect, NSRunLoop, NSSize},
};

/// The tray the app sets: none, one with a menu and an `onClick`, or one
/// with a menu alone.
static MODE: AtomicU8 = AtomicU8::new(0);

#[component]
fn App() -> impl IntoElement {
    let shown = std::cell::Cell::new(0);
    set_interval(Duration::from_millis(5), move || {
        let mode = MODE.load(Ordering::SeqCst);
        if mode == shown.replace(mode) {
            return;
        }
        let menu = || [MenuItem::new("Quit")];
        set_tray(match mode {
            1 => Some(tray().tooltip("Clicks").menu(menu()).on_click(|_| {})),
            2 => Some(tray().tooltip("Menu").menu(menu())),
            _ => None,
        });
    });
    view().flex_1()
}

/// The visible tray button whose label is `label`, found through the status
/// bar windows (AppKit has no public list of them).
fn button(mtm: MainThreadMarker, label: &str) -> Option<Retained<NSStatusBarButton>> {
    fn find(view: &NSView) -> Option<Retained<NSStatusBarButton>> {
        if let Ok(button) = view.retain().downcast::<NSStatusBarButton>() {
            return Some(button);
        }
        view.subviews().iter().find_map(|child| find(&child))
    }
    NSApplication::sharedApplication(mtm)
        .windows()
        .iter()
        .filter(|window| window.isVisible())
        .filter_map(|window| window.contentView().and_then(|view| find(&view)))
        .find(|button| {
            button
                .accessibilityLabel()
                .is_some_and(|l| l.to_string() == label)
        })
}

/// The accessibility actions of `button`'s element, its cell, as
/// accessibility clients list them.
#[allow(deprecated)]
fn actions(button: &NSStatusBarButton) -> Vec<String> {
    let cell = button.cell().expect("the button's cell");
    cell.accessibilityActionNames()
        .iter()
        .map(|a| a.to_string())
        .collect()
}

/// Runs the main run loop until `done` holds, for at most 5 s.
fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "{what} never happened");
        NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.01));
    }
}

/// Runs after `window_press::run`, which launched the application.
// `AppOptions` is non-exhaustive: its fields are set after `default()`.
#[allow(clippy::field_reassign_with_default)]
pub(super) fn run() {
    let mtm = MainThreadMarker::new().unwrap();
    // SAFETY: The window is created on the process main thread.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(700.0, 300.0), NSSize::new(300.0, 200.0)),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: This scope retains the window until after it closes.
    unsafe {
        window.setReleasedWhenClosed(false);
    }
    let mut options = platform::AppOptions::default();
    options.size = (300, 200);
    let app = platform::AppView::new(mtm, options);
    window.setContentView(Some(&app.view()));
    app.load(platform::App::Rust(platform::Root::of::<App>()));
    window.makeKeyAndOrderFront(None);

    MODE.store(1, Ordering::SeqCst);
    until("the clickable tray", || button(mtm, "Clicks").is_some());
    let clicks = button(mtm, "Clicks").unwrap();
    let names = actions(&clicks);
    assert!(
        names.iter().any(|a| a == "AXPress") && names.iter().any(|a| a == "AXShowMenu"),
        "a tray with an onClick and a menu offers AXPress and AXShowMenu, got {names:?}"
    );
    // `AXShowMenu` opens the button's context menu for a right mouse
    // down AppKit makes up: it is the tray's menu.
    let down = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        NSEventType::RightMouseDown,
        NSPoint::new(2.0, 2.0),
        NSEventModifierFlags::empty(),
        0.0,
        clicks.window().unwrap().windowNumber(),
        None,
        0,
        1,
        1.0,
    )
    .unwrap();
    let menu = clicks.menuForEvent(&down).expect("the tray's menu");
    assert_eq!(menu.numberOfItems(), 1);
    assert_eq!(
        menu.itemAtIndex(0).unwrap().title().to_string(),
        "Quit",
        "AXShowMenu shows the tray's menu"
    );

    // Without an `onClick`, the item opens its menu on any click itself.
    MODE.store(2, Ordering::SeqCst);
    until("the menu tray", || button(mtm, "Menu").is_some());
    let only_menu = button(mtm, "Menu").unwrap();
    assert!(
        only_menu.menu().is_none(),
        "a tray without an onClick has its menu on the item, not on the button"
    );

    MODE.store(0, Ordering::SeqCst);
    until("the tray's removal", || button(mtm, "Menu").is_none());
    println!("tray accessibility: a clickable tray's button offers AXShowMenu with its menu");
    app.stop();
    window.close();
}
