//! What AppKit's accessibility clients read of mounted views: a real
//! `AppView` in a window, its elements walked through `accessibilityChildren`
//! as VoiceOver and UI tests walk them. AppKit views live on the main thread,
//! so this test runs without libtest's harness, on the process's own.

#[cfg(target_os = "macos")]
fn main() {
    macos::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };

    use nsh_core::{element::Root, prelude::*};
    use nsh_platform_macos::{
        AppOptions, AppView,
        objc2::{MainThreadMarker, MainThreadOnly, Message, rc::Retained, runtime::AnyObject},
        objc2_app_kit::{
            NSAccessibility, NSApplication, NSBackingStoreType, NSView, NSWindow, NSWindowStyleMask,
        },
        objc2_foundation::{NSDate, NSPoint, NSRect, NSRunLoop, NSSize, NSString},
    };

    /// The test sets it to change the texts inside the elements.
    static RENAMED: AtomicBool = AtomicBool::new(false);

    /// An option and a tree item holding only a text, an option with a
    /// label of its own, and a button holding only a text.
    #[component]
    fn Screen() -> impl IntoElement {
        let renamed = signal(false);
        set_interval(Duration::from_millis(20), move || {
            if RENAMED.load(Ordering::SeqCst) && !renamed.get_untracked() {
                renamed.set(true);
            }
        });
        let name = move |before: &'static str, after: &'static str| {
            text(move || if renamed.get() { after } else { before })
        };
        view()
            .child(
                view()
                    .test_id("option")
                    .role(AccessibilityRole::Option)
                    .child(name("Orders", "Trades")),
            )
            .child(
                view()
                    .test_id("tree-item")
                    .role(AccessibilityRole::TreeItem)
                    .child(name("Hosts", "Processes")),
            )
            .child(
                view()
                    .test_id("labelled")
                    .role(AccessibilityRole::Option)
                    .label("Fixed")
                    .child(name("Shown", "Shown again")),
            )
            .child(
                view()
                    .test_id("button")
                    .on_press(|| {})
                    .child(name("Save", "Save all")),
            )
    }

    /// Runs the main run loop until `done` holds, for at most 5 s.
    fn until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done() {
            assert!(Instant::now() < deadline, "{what} never happened");
            let soon = NSDate::dateWithTimeIntervalSinceNow(0.02);
            NSRunLoop::currentRunLoop().runUntilDate(&soon);
        }
    }

    /// The element with accessibility identifier `id` under `view`, as
    /// clients find it: through `accessibilityChildren`.
    fn find(view: &NSView, id: &str) -> Option<Retained<NSView>> {
        if view
            .accessibilityIdentifier()
            .is_some_and(|i| i.to_string() == id)
        {
            return Some(view.retain());
        }
        let children = view.accessibilityChildren()?;
        children
            .iter()
            .filter_map(|c| c.downcast::<NSView>().ok())
            .find_map(|c| find(&c, id))
    }

    fn string(object: Option<Retained<AnyObject>>) -> Option<String> {
        object
            .and_then(|o| o.downcast::<NSString>().ok())
            .map(|s| s.to_string())
    }

    /// The label, value and accessibility children of element `id`.
    fn read(root: &NSView, id: &str) -> (Option<String>, Option<String>, usize) {
        let e = find(root, id).unwrap_or_else(|| panic!("no element {id}"));
        let label = e.accessibilityLabel().map(|s| s.to_string());
        let value = string(e.accessibilityValue());
        let children = e.accessibilityChildren().map_or(0, |c| c.count());
        (label, value, children)
    }

    pub(super) fn run() {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let _app = NSApplication::sharedApplication(mtm);
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0));
        // SAFETY: a plain titled window, released by the test's end.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                NSWindowStyleMask::Titled,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { window.setReleasedWhenClosed(false) };
        let app = AppView::new(
            mtm,
            AppOptions {
                size: (400, 300),
                ..AppOptions::default()
            },
        );
        window.setContentView(Some(&app.view()));
        window.orderFrontRegardless();
        app.load(Root::of::<Screen>().into());
        let root = app.view();

        until("the first mount", || find(&root, "button").is_some());

        // Static text reads its name as its value: the folded text goes
        // there, and no text child is left to say it again.
        until("the option's value", || {
            read(&root, "option") == (None, Some("Orders".into()), 0)
        });
        assert_eq!(read(&root, "tree-item"), (None, Some("Hosts".into()), 0));
        // A label of its own stays what it reads, as its value.
        assert_eq!(read(&root, "labelled"), (None, Some("Fixed".into()), 0));
        // A button reads its name as its label.
        assert_eq!(read(&root, "button"), (Some("Save".into()), None, 0));

        // The texts change: what clients read follows.
        RENAMED.store(true, Ordering::SeqCst);
        until("the option's new value", || {
            read(&root, "option").1.as_deref() == Some("Trades")
        });
        assert_eq!(
            read(&root, "tree-item"),
            (None, Some("Processes".into()), 0)
        );
        assert_eq!(read(&root, "labelled"), (None, Some("Fixed".into()), 0));
        assert_eq!(read(&root, "button"), (Some("Save all".into()), None, 0));

        app.stop();
        window.close();
        println!("accessibility: ok");
    }
}
