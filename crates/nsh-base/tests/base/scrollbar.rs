//! `Scrollbar`: a bar of the app's own over a scroll view it names. Its
//! thumb follows the target's offset and content; a drag of it, or a
//! press on its track, scrolls the target; it shows by its mode (while
//! the target scrolls, while the pointer is over it, always), takes no
//! presses while hidden, and never shows while the content fits.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use nsh_base::{ScrollbarMode, scrollbar};
use nsh_core::{MountItem, NodeId, Size, element::Root, geometry::point, platform::Os, prelude::*};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

#[derive(Clone, PartialEq, Props)]
struct HarnessProps {
    mode: ScrollbarMode,
    /// How tall the content is (the viewport is 200).
    content: f32,
}

/// A scroll view 300 by 200 of `content` points with its own indicator
/// off, and a bar 10 wide over its right edge, labelled `Bar` (its track
/// 200 long: the thumb is 40 long over 1000 points and travels 160); the
/// offset the scroll view last reported.
#[component]
fn Harness(props: HarnessProps) -> impl IntoElement {
    let content = element_ref();
    let offset = signal(0.0_f32);
    view()
        .child(
            view()
                .relative()
                .w(px(300.0))
                .h(px(200.0))
                .child(
                    scroll()
                        .element_ref(&content)
                        .test_id("content")
                        .size_full()
                        .scrollbar(false)
                        .on_scroll(move |e: &ScrollEvent| offset.set(e.offset.y.0))
                        .child(view().w_full().h(px(props.content))),
                )
                .child(
                    scrollbar()
                        .scroll(content)
                        .mode(props.mode)
                        .hide_delay(500.0)
                        .accessibility_label("Bar")
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .w(px(10.0))
                        .thumb_style(|s| s.w_full()),
                ),
        )
        .child(text(move || format!("Offset {}", offset.get().round())))
}

/// What the batches showed of the bar.
#[derive(Default)]
struct Shown {
    /// The bar's node.
    bar: Option<NodeId>,
    /// Each view's opacity.
    opacity: HashMap<NodeId, f32>,
}

#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Shown>>);

impl Seen {
    /// The bar is shown: not at opacity 0.
    fn shown(&self) -> bool {
        let seen = self.0.lock().unwrap();
        let bar = seen.bar.expect("the bar is mounted");
        seen.opacity.get(&bar).copied().unwrap_or(1.0) > 0.0
    }
}

/// The harness on a headless macOS surface (a mouse to hover with), on a
/// manual clock; what its batches show of the bar.
fn start(mode: ScrollbarMode, content: f32) -> (Headless, Seen) {
    // A manual clock: the bar hides only when the test moves it.
    let mut config = EngineConfig {
        viewport: Size::new(px(400.0), px(844.0)),
        clock: nsh_engine::Clock::manual(std::time::SystemTime::now()),
        ..EngineConfig::new(Root::new::<Harness>(move || HarnessProps { mode, content }))
    };
    config.environment.os = Os::MacOs;
    let mut app = Headless::start(config).expect("the engine starts");
    let seen = Seen::default();
    let record = seen.clone();
    app.observe(move |_, batch| {
        let mut seen = record.0.lock().unwrap();
        for item in &batch.items {
            match item {
                MountItem::ViewProps { id, props }
                    if props.accessibility.label.as_deref() == Some("Bar") =>
                {
                    seen.bar = Some(*id);
                }
                MountItem::ViewStyle { id, style } => {
                    seen.opacity.insert(*id, style.opacity);
                }
                _ => {}
            }
        }
    });
    app.wait_for("text:Offset 0").unwrap();
    (app, seen)
}

/// Presses the middle of the bar and drags across its axis, which
/// scrolls nothing more: where the press went shows.
fn press_middle(app: &mut Headless) {
    app.drag("label:Bar", point(px(40.0), px(0.0))).unwrap();
}

#[test]
fn a_press_on_the_track_centers_the_thumb_there_and_a_drag_goes_on() {
    let (mut app, seen) = start(ScrollbarMode::Always, 1000.0);
    assert!(seen.shown());
    // From the middle of the track (the thumb is at 0 to 40): the thumb's
    // middle there puts its start at 80, half its travel (400), and 40
    // points down are 40 of 160 more: 200 of 800.
    app.drag("label:Bar", point(px(0.0), px(40.0))).unwrap();
    app.wait_for("text:Offset 600").unwrap();
    // Past the end: held at it.
    app.drag("label:Bar", point(px(0.0), px(400.0))).unwrap();
    app.wait_for("text:Offset 800").unwrap();
}

#[test]
fn the_thumb_follows_the_target_scrolled_elsewhere() {
    let (mut app, _) = start(ScrollbarMode::Always, 1000.0);
    app.scroll("id:content", px(400.0)).unwrap();
    app.wait_for("text:Offset 400").unwrap();
    // The thumb is at 80 to 120 now: a drag from the middle takes it, and
    // 16 points move the content 80.
    app.drag("label:Bar", point(px(0.0), px(16.0))).unwrap();
    app.wait_for("text:Offset 480").unwrap();
}

#[test]
fn while_scrolling_it_shows_and_hides_after_the_delay() {
    let (mut app, seen) = start(ScrollbarMode::Scrolling, 1000.0);
    assert!(!seen.shown());
    app.scroll("id:content", px(100.0)).unwrap();
    app.wait_for("text:Offset 100").unwrap();
    assert!(seen.shown());
    press_middle(&mut app);
    app.wait_for("text:Offset 400").unwrap();
    // A drag keeps it until the delay after it ends.
    app.fast_forward(Duration::from_millis(400)).unwrap();
    assert!(seen.shown());
    app.fast_forward(Duration::from_millis(200)).unwrap();
    assert!(!seen.shown());
}

#[test]
fn on_hover_it_shows_while_the_pointer_is_over_the_target() {
    let (mut app, seen) = start(ScrollbarMode::Hover, 1000.0);
    assert!(!seen.shown());
    app.hover("id:content").unwrap();
    assert!(seen.shown());
    press_middle(&mut app);
    app.wait_for("text:Offset 400").unwrap();
}

#[test]
fn it_never_shows_while_the_content_fits() {
    let (_, seen) = start(ScrollbarMode::Always, 150.0);
    assert!(!seen.shown());
}
