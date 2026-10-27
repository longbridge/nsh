use std::{
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use nsh_platform_macos::{
    AppOptions, AppView, Bundle,
    objc2::{MainThreadMarker, MainThreadOnly, rc::Retained},
    objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSApplication, NSBackingStoreType, NSColor, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow,
        NSWindowStyleMask,
    },
    objc2_foundation::{NSDate, NSPoint, NSRect, NSRunLoop, NSSize},
};

pub(super) fn run() {
    let mtm = MainThreadMarker::new().expect("AppKit integration tests need the main thread");
    let _app = NSApplication::sharedApplication(mtm);
    opaque_keeps_the_window(mtm);
    blurred_puts_a_backdrop_behind_the_content(mtm);
    blurred_leaves_the_window_alone(mtm);
    backdrop_stays_below_new_content(mtm);
    backdrop_follows_the_appearance(mtm);
    navigation_follows_the_background_appearance(mtm);
    navigation_is_clear_when_blur_starts_with_it(mtm);
    next_app_starts_opaque(mtm);
    backdrop_follows_the_view_to_another_window(mtm);
    std::fs::remove_dir_all(data_dir()).ok();
    println!("9 window background appearance checks passed");
}

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.nsh/tests/macos-window-background")
        .join(std::process::id().to_string())
}

fn window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    // SAFETY: NSWindow's designated initializer runs on the main thread.
    unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(420.0, 300.0)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable,
            NSBackingStoreType::Buffered,
            false,
        )
    }
}

/// An `AppView` an embedding app put in a window's content view.
struct Embedded(Rc<AppView>);

impl Embedded {
    fn new(window: &NSWindow, mtm: MainThreadMarker) -> Self {
        let options = AppOptions {
            size: (420, 300),
            data_dir: Some(data_dir()),
            ..AppOptions::default()
        };
        let view = AppView::new(mtm, options);
        window.contentView().unwrap().addSubview(&view.view());
        Self(view)
    }

    /// Loads an app that runs `source`, and waits until its first batch is
    /// applied.
    fn run(&self, window: &NSWindow, source: &str) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let marker = format!("background-check-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.0.load(
            Bundle::source(format!(
                "import {{ Window }} from 'nsh/desktop';\n{source}\nWindow.setTitle('{marker}');"
            ))
            .into(),
        );
        // The title command follows the tested commands in the same batch.
        until(|| window.title().to_string() == marker);
    }

    fn root(&self) -> Retained<NSView> {
        self.0.view().subviews().firstObject().unwrap()
    }
}

impl Drop for Embedded {
    fn drop(&mut self) {
        self.0.view().removeFromSuperview();
        self.0.stop();
    }
}

fn until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < deadline, "AppKit state did not settle");
        NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.005));
    }
}

/// The backdrops in the window's content view.
fn backdrops(window: &NSWindow) -> Vec<Retained<NSVisualEffectView>> {
    window
        .contentView()
        .unwrap()
        .subviews()
        .iter()
        .filter(|view| view.class().name().to_bytes() == b"NshWindowBackdrop")
        .filter_map(|view| view.downcast::<NSVisualEffectView>().ok())
        .collect()
}

fn dark(view: &NSView) -> bool {
    view.effectiveAppearance()
        .name()
        .to_string()
        .contains("Dark")
}

fn opaque_keeps_the_window(mtm: MainThreadMarker) {
    let host = window(mtm);
    let app = Embedded::new(&host, mtm);
    app.run(&host, "Window.setBackgroundAppearance('opaque');");
    assert!(backdrops(&host).is_empty());
}

fn blurred_puts_a_backdrop_behind_the_content(mtm: MainThreadMarker) {
    let host = window(mtm);
    let app = Embedded::new(&host, mtm);
    app.run(&host, "Window.setBackgroundAppearance('blurred');");
    let found = backdrops(&host);
    assert_eq!(found.len(), 1);
    let backdrop = &found[0];
    assert_eq!(
        backdrop.blendingMode(),
        NSVisualEffectBlendingMode::BehindWindow
    );
    assert_eq!(backdrop.material(), NSVisualEffectMaterial::Sidebar);
    assert_eq!(
        backdrop.state(),
        NSVisualEffectState::FollowsWindowActiveState
    );
    let content = host.contentView().unwrap();
    assert_eq!(backdrop.frame(), content.bounds());
    // At the back of the content view, so presses where the app draws
    // nothing reach the content view as before.
    let back: &NSView = backdrop;
    assert!(
        content
            .subviews()
            .firstObject()
            .is_some_and(|v| *v == *back)
    );
    assert!(backdrop.hitTest(NSPoint::new(10.0, 10.0)).is_none());
    host.setContentSize(NSSize::new(540.0, 360.0));
    assert_eq!(backdrop.frame(), content.bounds());

    // Setting it again keeps the backdrop it has.
    app.run(
        &host,
        "Window.setBackgroundAppearance('blurred'); Window.setBackgroundAppearance('blurred');",
    );
    assert_eq!(backdrops(&host).len(), 1);

    app.run(
        &host,
        "Window.setBackgroundAppearance('blurred'); Window.setBackgroundAppearance('opaque');",
    );
    assert!(backdrops(&host).is_empty());
}

fn blurred_leaves_the_window_alone(mtm: MainThreadMarker) {
    let host = window(mtm);
    let background = NSColor::colorWithSRGBRed_green_blue_alpha(0.2, 0.4, 0.6, 1.0);
    host.setBackgroundColor(Some(&background));
    let opaque = host.isOpaque();
    let transparent = host.titlebarAppearsTransparent();
    let style = host.styleMask();
    let app = Embedded::new(&host, mtm);
    app.run(&host, "Window.setBackgroundAppearance('blurred');");
    assert_eq!(backdrops(&host).len(), 1);
    assert_eq!(host.isOpaque(), opaque);
    assert_eq!(host.backgroundColor(), background);
    assert_eq!(host.titlebarAppearsTransparent(), transparent);
    assert_eq!(host.styleMask(), style);
}

fn backdrop_stays_below_new_content(mtm: MainThreadMarker) {
    let host = window(mtm);
    let app = Embedded::new(&host, mtm);
    app.run(
        &host,
        "import { mount, View, Text } from 'nsh'; \
         Window.setBackgroundAppearance('blurred'); \
         mount(() => View({ children: Text({ children: 'New content' }) }));",
    );
    assert!(!app.root().subviews().is_empty());
    let content = host.contentView().unwrap();
    let siblings = content.subviews();
    let backdrop = backdrops(&host).remove(0);
    let position = |view: &NSView| siblings.iter().position(|v| *v == *view).unwrap();
    assert!(position(&backdrop) < position(&app.0.view()));
    // The surface's background is clear, so the backdrop shows through
    // where the app draws nothing.
    let layer = app.0.view().layer().unwrap();
    assert!(
        layer
            .backgroundColor()
            .is_none_or(|c| objc2_core_graphics::CGColor::alpha(Some(&c)) == 0.0)
    );
}

fn backdrop_follows_the_appearance(mtm: MainThreadMarker) {
    let host = window(mtm);
    // SAFETY: immutable AppKit appearance constants.
    let light = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }).unwrap();
    // SAFETY: immutable AppKit appearance constants.
    let dark_appearance =
        NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }).unwrap();
    host.setAppearance(Some(&light));
    let app = Embedded::new(&host, mtm);

    app.run(
        &host,
        "import { setColorScheme } from 'nsh'; \
         Window.setBackgroundAppearance('blurred'); \
         setColorScheme('dark');",
    );
    let backdrop = backdrops(&host).remove(0);
    assert!(dark(&backdrop));
    // The app's scheme is the root view's, never the host window's.
    assert_eq!(host.appearance(), Some(light.clone()));

    app.run(
        &host,
        "import { setColorScheme } from 'nsh'; \
         Window.setBackgroundAppearance('blurred'); \
         setColorScheme('light');",
    );
    let backdrop = backdrops(&host).remove(0);
    assert!(!dark(&backdrop));

    // Following the system's scheme, the backdrop takes the host's. The
    // scheme an app set stays with the view, and a new app starts in
    // "system" as JS sees it: switch away first so that "system" is sent.
    app.run(
        &host,
        "import { setColorScheme } from 'nsh'; \
         Window.setBackgroundAppearance('blurred'); \
         setColorScheme('dark'); setColorScheme('system');",
    );
    let backdrop = backdrops(&host).remove(0);
    host.setAppearance(Some(&dark_appearance));
    until(|| dark(&app.root()));
    assert!(dark(&backdrop));
}

fn navigation_follows_the_background_appearance(mtm: MainThreadMarker) {
    let host = window(mtm);
    let app = Embedded::new(&host, mtm);
    app.run(
        &host,
        "import { mount, host } from 'nsh'; \
         import { createNavStack, NavStack, NavPage } from 'nsh/base'; \
         host.on('background', appearance => { \
           Window.setBackgroundAppearance(appearance); Window.setTitle(appearance); \
         }); \
         mount(() => NavStack({ state: createNavStack(['root']), \
           style: { flex: 1 }, children: () => NavPage({}) }));",
    );
    let stack = app.root().subviews().firstObject().unwrap();
    let page = stack.subviews().firstObject().unwrap();
    let backdrop = page.subviews().firstObject().unwrap();
    assert_eq!(backdrop.frame().size, NSSize::new(420.0, 300.0));
    let alpha = || {
        backdrop
            .layer()
            .unwrap()
            .backgroundColor()
            .map_or(0.0, |color| {
                objc2_core_graphics::CGColor::alpha(Some(&color))
            })
    };
    assert_eq!(alpha(), 1.0);
    for (appearance, expected) in [("blurred", 0.0), ("opaque", 1.0), ("blurred", 0.0)] {
        app.0.host().send("background", appearance.into()).unwrap();
        until(|| host.title().to_string() == appearance);
        assert_eq!(alpha(), expected, "navigation background: {appearance}");
        assert_eq!(stack.subviews().firstObject().as_deref(), Some(&*page));
    }
}

/// The page backgrounds are made before the same batch's blur is applied.
fn navigation_is_clear_when_blur_starts_with_it(mtm: MainThreadMarker) {
    let host = window(mtm);
    let app = Embedded::new(&host, mtm);
    app.run(
        &host,
        "import { mount } from 'nsh'; \
         import { createNavStack, NavStack, NavPage } from 'nsh/base'; \
         Window.setBackgroundAppearance('blurred'); \
         mount(() => NavStack({ state: createNavStack(['root']), \
           style: { flex: 1 }, children: () => NavPage({}) }));",
    );
    let stack = app.root().subviews().firstObject().unwrap();
    let page = stack.subviews().firstObject().unwrap();
    let backdrop = page.subviews().firstObject().unwrap();
    assert_eq!(backdrop.frame().size, NSSize::new(420.0, 300.0));
    let color = backdrop.layer().unwrap().backgroundColor();
    assert_eq!(
        color.map_or(0.0, |c| objc2_core_graphics::CGColor::alpha(Some(&c))),
        0.0
    );
}

fn next_app_starts_opaque(mtm: MainThreadMarker) {
    let host = window(mtm);
    let app = Embedded::new(&host, mtm);
    app.run(&host, "Window.setBackgroundAppearance('blurred');");
    assert_eq!(backdrops(&host).len(), 1);
    // Another app in the same view never asked for one.
    app.run(
        &host,
        "import { mount, View } from 'nsh'; mount(() => View({}));",
    );
    assert!(backdrops(&host).is_empty());
    let layer = app.0.view().layer().unwrap();
    let color = layer.backgroundColor().expect("the surface's background");
    assert_eq!(objc2_core_graphics::CGColor::alpha(Some(&color)), 1.0);
}

fn backdrop_follows_the_view_to_another_window(mtm: MainThreadMarker) {
    let first = window(mtm);
    let app = Embedded::new(&first, mtm);
    app.run(&first, "Window.setBackgroundAppearance('blurred');");
    assert_eq!(backdrops(&first).len(), 1);

    let second = window(mtm);
    second.contentView().unwrap().addSubview(&app.0.view());
    assert!(backdrops(&first).is_empty());
    assert_eq!(backdrops(&second).len(), 1);

    app.0.view().removeFromSuperview();
    assert!(backdrops(&second).is_empty());
}
