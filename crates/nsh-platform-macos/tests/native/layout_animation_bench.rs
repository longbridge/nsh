//! What a layout animation costs the main thread on every frame when a
//! sidebar opens and closes beside a page of rows: the page's ~400 views
//! change size (the rows, their text) or only move (the badges on their
//! right edge), as Noop's sidebar does.
//!
//! The window is ordered in without activating the app and ignores the
//! mouse. It is opaque: AppKit drives a see-through window's display link
//! at a few frames a second, which leaves one frame per animation.
//!
//! ```sh
//! NSH_LAYOUT_ANIMATION_BENCH=20 cargo test --release -p nsh-platform-macos \
//!     --test native -- --ignored
//! ```
//!
//! prints the frames each animation took and the main thread's cost per
//! frame, from the performance monitor.

use std::{
    rc::Rc,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

use nsh_core::{LayoutAnimation, LayoutCurve, LayoutTransition, prelude::*};
use nsh_platform_macos::{
    self as platform,
    objc2::{MainThreadMarker, MainThreadOnly, Message, rc::Retained},
    objc2_app_kit::{
        NSAccessibility, NSApplication, NSBackingStoreType, NSView, NSWindow, NSWindowStyleMask,
    },
    objc2_foundation::{NSDate, NSPoint, NSRect, NSRunLoop, NSSize, NSString},
};

/// 0: the sidebar is closed, 1: open.
static OPEN: AtomicU32 = AtomicU32::new(0);

const SIZE: (f64, f64) = (1200.0, 800.0);
const SIDEBAR: f32 = 260.0;
const ROWS: usize = 60;
const DURATION_MS: f32 = 200.0;

#[component]
fn Scene() -> impl IntoElement {
    let open = signal(false);
    set_interval(Duration::from_millis(2), move || {
        let want = OPEN.load(Ordering::SeqCst) == 1;
        if open.get_untracked() != want {
            animate_layout(LayoutAnimation {
                update: Some(LayoutTransition::new(
                    LayoutCurve::EaseInEaseOut,
                    DURATION_MS,
                )),
                ..Default::default()
            });
            open.set(want);
        }
    });
    view()
        .size_full()
        .flex_row()
        .bg(rgb(0xffffff))
        .child(
            view()
                .test_id("sidebar")
                .flex_none()
                .overflow_hidden()
                .w(move || Length::from(px(if open.get() { SIDEBAR } else { 0.0 })))
                .bg(rgb(0xf4f4f5))
                .children((0..20).map(|i| {
                    view()
                        .h(px(28.0))
                        .px(px(12.0))
                        .justify_center()
                        .child(text(format!("Session {i}")).text_color(rgb(0x18181b)))
                })),
        )
        .child(
            view()
                .flex_1()
                .overflow_hidden()
                .children((0..ROWS).map(|i| {
                    view()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .h(px(40.0))
                        .px(px(16.0))
                        .child(view().size(px(24.0)).rounded(px(12.0)).bg(rgb(0xdbeafe)))
                        .child(
                            view()
                                .flex_1()
                                .child(text(format!("Message {i}")).text_color(rgb(0x18181b)))
                                .child(
                                    text("A line of text that fills the row")
                                        .text_color(rgb(0x71717a)),
                                ),
                        )
                        .child(
                            view()
                                .px(px(6.0))
                                .rounded(px(4.0))
                                .bg(rgb(0xe4e4e7))
                                .child(text(format!("#{i}"))),
                        )
                })),
        )
}

fn find(view: &NSView, id: &str) -> Option<Retained<NSView>> {
    if view
        .accessibilityIdentifier()
        .is_some_and(|v| v.to_string() == id)
    {
        return Some(view.retain());
    }
    view.subviews().iter().find_map(|v| find(&v, id))
}

fn pump(seconds: f64) {
    NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(seconds));
}

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < end, "{what} never happened");
        pump(0.005);
    }
}

/// Opens or closes the sidebar and waits until the animation settled: the
/// number of frames its width changed in.
fn animate(sidebar: &NSView, open: bool) -> usize {
    let end = if open { SIDEBAR as f64 } else { 0.0 };
    let start = Instant::now();
    OPEN.store(open as u32, Ordering::SeqCst);
    let mut frames = 0;
    let mut last = sidebar.frame().size.width;
    let deadline = start + Duration::from_millis(DURATION_MS as u64 + 1500);
    loop {
        pump(0.001);
        let width = sidebar.frame().size.width;
        if width != last {
            last = width;
            frames += 1;
        }
        if width == end && start.elapsed() > Duration::from_millis(DURATION_MS as u64 + 50) {
            break;
        }
        assert!(Instant::now() < deadline, "the animation never settled");
    }
    frames
}

pub(super) fn run() {
    let Some(runs) = std::env::var("NSH_LAYOUT_ANIMATION_BENCH")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    else {
        return;
    };
    let mtm = MainThreadMarker::new().expect("the bench runs on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.finishLaunching();
    // SAFETY: The window is created on the process main thread.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(SIZE.0, SIZE.1)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: This scope retains the window until after it closes.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str("NSH layout animation bench"));
    window.setIgnoresMouseEvents(true);
    let surface: Rc<platform::AppView> = platform::AppView::new(
        mtm,
        platform::AppOptions {
            size: (SIZE.0 as i32, SIZE.1 as i32),
            fps: true,
            ..Default::default()
        },
    );
    let container = surface.view();
    window.setContentView(Some(&container));
    OPEN.store(0, Ordering::SeqCst);
    surface.load(platform::App::Rust(platform::Root::of::<Scene>()));
    window.orderFrontRegardless();
    surface.set_fps_mode(FpsMode::Hidden);
    until("the page mounting", || find(&container, "sidebar").is_some());
    let sidebar = find(&container, "sidebar").expect("the sidebar");
    pump(0.3);

    let monitor = surface.perf_monitor().expect("the performance monitor");
    let mut counts = Vec::new();
    let mut costs = Vec::new();
    for run in 0..runs {
        let frames = animate(&sidebar, run % 2 == 0);
        counts.push(frames);
        // The monitor keeps the last 120 frames: read them after each run.
        let snapshot = monitor.snapshot();
        let n = frames.min(snapshot.frames.len());
        costs.extend(
            snapshot.frames[snapshot.frames.len() - n..]
                .iter()
                .map(|d| d.as_secs_f64() * 1000.0),
        );
        pump(0.15);
    }
    let refresh = monitor.snapshot().refresh_rate;
    costs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| costs[((costs.len() as f64 * q).ceil() as usize).clamp(1, costs.len()) - 1];
    let mean = costs.iter().sum::<f64>() / costs.len() as f64;
    let mean_frames = counts.iter().sum::<usize>() as f64 / counts.len() as f64;
    println!("layout animation bench: {runs} runs of {DURATION_MS} ms, refresh {refresh:?} Hz");
    println!(
        "  frames per run: mean {mean_frames:.1} (min {}, max {})",
        counts.iter().min().unwrap(),
        counts.iter().max().unwrap(),
    );
    println!(
        "  main-thread cost per frame: mean {mean:.2} ms, p50 {:.2} ms, p95 {:.2} ms, max {:.2} ms",
        at(0.5),
        at(0.95),
        costs[costs.len() - 1]
    );

    surface.stop();
    window.close();
}
