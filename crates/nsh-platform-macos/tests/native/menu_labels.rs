//! Menu labels changed while the app runs (`nsh/i18n`'s `set_locale`): the items of
//! the app's menu bar that have a role and no label of their own take them
//! at once.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use nsh_core::{
    desktop::MenuRole,
    element::desktop::{MenuItem, set_application_menu},
    prelude::*,
};
use nsh_platform_macos as platform;
use platform::{
    objc2::{MainThreadMarker, MainThreadOnly, sel},
    objc2_app_kit::{NSApplication, NSBackingStoreType, NSMenu, NSWindow, NSWindowStyleMask},
    objc2_foundation::{NSDate, NSPoint, NSRect, NSRunLoop, NSSize},
};

/// The test sets it for the app to label its menus in Japanese.
static JAPANESE: AtomicBool = AtomicBool::new(false);

/// An Edit menu of roles only, labelled in Japanese once asked.
#[component]
fn Menus() -> impl IntoElement {
    nsh_core::i18n::set_locale(Some(nsh_core::platform::Locale::parse("en").unwrap()));
    set_application_menu(vec![
        MenuItem::submenu("App", [MenuItem::role(MenuRole::Quit)]),
        MenuItem::submenu(
            "Edit",
            [
                MenuItem::role(MenuRole::Copy),
                MenuItem::role(MenuRole::Paste),
            ],
        ),
    ]);
    set_interval(Duration::from_millis(10), || {
        if JAPANESE.swap(false, Ordering::SeqCst) {
            nsh_core::i18n::set_locale(Some(nsh_core::platform::Locale::parse("ja").unwrap()));
        }
    });
    view().flex_1()
}

/// The title of the item of `menu`'s submenus whose action is `action`.
fn title_of(menu: &NSMenu, action: platform::objc2::runtime::Sel) -> Option<String> {
    menu.itemArray().iter().find_map(|item| {
        if item.action() == Some(action) {
            return Some(item.title().to_string());
        }
        item.submenu().and_then(|sub| title_of(&sub, action))
    })
}

fn titles(mtm: MainThreadMarker) -> (Option<String>, Option<String>) {
    let Some(menu) = NSApplication::sharedApplication(mtm).mainMenu() else {
        return (None, None);
    };
    (title_of(&menu, sel!(copy:)), title_of(&menu, sel!(paste:)))
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
    let surface = platform::AppView::new(mtm, options);
    window.setContentView(Some(&surface.view()));
    surface.load(platform::App::Rust(platform::Root::of::<Menus>()));
    window.makeKeyAndOrderFront(None);

    until("the app's menu", || {
        titles(mtm) == (Some("Copy".into()), Some("Paste".into()))
    });
    JAPANESE.store(true, Ordering::SeqCst);
    until("the Japanese label", || {
        titles(mtm) == (Some("コピー".into()), Some("ペースト".into()))
    });

    println!("menu labels: the app's menu takes new labels while it runs");
    surface.stop();
    window.close();
}
