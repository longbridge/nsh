//! `Carousel`: the slide shown is the app's `index` (else its own from
//! `defaultIndex`); previous / next stop at the ends unless it loops; a
//! pagination item moves to its slide and is selected while it shows; a
//! swipe reports the slide it rests on, between `onDragStart` and
//! `onDragEnd`; the keys move while it has the focus, which `onFocus` /
//! `onBlur` report for the carousel as a whole; reduced motion scrolls at
//! once; several slides per view rest with the last at the end of the
//! range; a change sends a handful of items. Looping is seamless: runways
//! of a period, snap points over three periods, each slide's translation
//! made once, a wrap scrolling one step across the seam and a silent
//! re-center after it. On macOS (Tab stopping at views that take presses)
//! and on a manual clock.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nsh_base::{
    carousel, carousel_content, carousel_item, carousel_next, carousel_pagination,
    carousel_pagination_item, carousel_previous,
};
use nsh_core::{
    Command, MountItem, NodeId, Size, animation::AnimationOp, element::Root, platform::Os,
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

/// How many slides the harness has.
const SLIDES: u32 = 5;

/// How wide the content is.
const WIDTH: f32 = 300.0;

#[derive(Clone, PartialEq, Props)]
struct HarnessProps {
    /// The app passes `index` (else the carousel keeps its own).
    controlled: bool,
    looping: bool,
    /// Slides per view: 1 (or 0) or 3.
    per_view: u32,
    /// The slide shown at first.
    start: u32,
    /// The pagination has an indicator.
    indicator: bool,
}

/// A button, a carousel of five slides in a 300 by 100 content (each
/// `300 / per_view` wide), previous and next, a pagination item per slide;
/// buttons that set the app's index to 3 (`Jump`) and to 0 (`First`) and
/// one that pads the content's start by 20 (`Pad`). The app's index as
/// `index:…`, the number of changes as `changes:…`, the drag and focus
/// events as `events:…`.
#[component]
fn Harness(props: HarnessProps) -> impl IntoElement {
    let index = signal(props.start);
    let changes = signal(0u32);
    let events = signal(Vec::<&'static str>::new());
    let log = move |e: &'static str| events.update(|v| v.push(e));
    let padded = signal(false);
    let width = WIDTH / props.per_view.max(1) as f32;
    let carousel = carousel()
        .default_index(props.start)
        .looping(props.looping)
        .accessibility_label("Photos")
        .on_change(move |i: &u32| {
            changes.update(|n| *n += 1);
            index.set(*i);
        })
        .on_drag_start(move || log("dragStart"))
        .on_drag_end(move || log("dragEnd"))
        .on_focus(move || log("focus"))
        .on_blur(move || log("blur"));
    let carousel = if props.controlled {
        carousel.index(move || Some(index.get()))
    } else {
        carousel
    };
    let pagination = carousel_pagination()
        .accessibility_label("Slides")
        .flex_row()
        .when(props.indicator, |p| {
            p.indicator_style(|s| s.top(px(0.0)).bottom(px(0.0)))
        })
        .child(
            (0..SLIDES)
                .map(|i| {
                    carousel_pagination_item()
                        .index(i)
                        .accessibility_label(format!("Go to {}", i + 1))
                        .w(px(10.0))
                        .h(px(10.0))
                })
                .collect::<Vec<_>>(),
        );
    view()
        .child(view().on_press(|| {}).child(text("Before")))
        .child(
            carousel
                .child(
                    carousel_content()
                        .w(px(WIDTH))
                        .h(px(100.0))
                        .style_with(move |s| if padded.get() { s.pl(px(20.0)) } else { s })
                        .child(
                            (0..SLIDES)
                                .map(|i| {
                                    carousel_item()
                                        .accessibility_label(format!("{} of {SLIDES}", i + 1))
                                        .w(px(width))
                                        .h(px(100.0))
                                        .child(text(format!("Slide {}", i + 1)))
                                })
                                .collect::<Vec<_>>(),
                        ),
                )
                .child(
                    carousel_previous()
                        .accessibility_label("Previous")
                        .child(text("<")),
                )
                .child(carousel_next().accessibility_label("Next").child(text(">")))
                .child(pagination),
        )
        .child(view().on_press(move || index.set(3)).child(text("Jump")))
        .child(view().on_press(move || index.set(0)).child(text("First")))
        .child(view().on_press(move || padded.set(true)).child(text("Pad")))
        .child(text(move || format!("index:{}", index.get())))
        .child(text(move || format!("changes:{}", changes.get())))
        .child(text(move || format!("events:{}", events.get().join(","))))
}

/// The items of each step's batches.
type Seen = Arc<Mutex<Vec<MountItem>>>;

struct App {
    app: Headless,
    seen: Seen,
}

impl std::ops::Deref for App {
    type Target = Headless;
    fn deref(&self) -> &Headless {
        &self.app
    }
}

impl std::ops::DerefMut for App {
    fn deref_mut(&mut self) -> &mut Headless {
        &mut self.app
    }
}

impl App {
    /// The items that came since the last call.
    fn take(&self) -> Vec<MountItem> {
        std::mem::take(&mut *self.seen.lock().unwrap())
    }

    /// Presses the button screen readers call `label`; the items that came.
    fn press_button(&mut self, label: &str) -> Vec<MountItem> {
        self.take();
        self.app.press(&format!("role:button[{label}]")).unwrap();
        self.take()
    }

    /// Whether the button screen readers call `label` takes presses.
    fn enabled(&self, label: &str) -> bool {
        self.app
            .screen()
            .enabled(&format!("role:button[{label}]"))
            .unwrap()
    }

    /// Runs the clock past the re-center after a wrap; the items that came.
    fn after_wrap(&mut self) -> Vec<MountItem> {
        self.take();
        self.app.run_for(Duration::from_millis(700)).unwrap();
        self.take()
    }
}

/// The content's scrolls in `items`: where to, and whether animated.
fn scrolls(items: &[MountItem]) -> Vec<(f32, bool)> {
    items
        .iter()
        .filter_map(|i| match i {
            MountItem::Command {
                command: Command::ScrollTo { x, animated, .. },
                ..
            } => Some((x.0, *animated)),
            _ => None,
        })
        .collect()
}

/// The animation graph's nodes `items` make.
fn made_nodes(items: &[MountItem]) -> usize {
    items
        .iter()
        .filter(
            |i| matches!(i, MountItem::Animation(op) if format!("{op:?}").starts_with("Create")),
        )
        .count()
}

/// The values `items` set in the animation graph.
fn sets(items: &[MountItem]) -> usize {
    items
        .iter()
        .filter(|i| matches!(i, MountItem::Animation(AnimationOp::Set { .. })))
        .count()
}

/// The content's snap offsets, as last sent.
fn snap_offsets(items: &[MountItem]) -> Vec<f32> {
    items
        .iter()
        .rev()
        .find_map(|i| match i {
            MountItem::ScrollBehavior { behavior, .. } => {
                Some(behavior.snap_offsets.iter().map(|o| o.0).collect())
            }
            _ => None,
        })
        .expect("the content's behavior")
}

fn start(props: HarnessProps, reduce_motion: bool) -> App {
    let mut config = EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        clock: nsh_engine::Clock::manual(std::time::SystemTime::now()),
        ..EngineConfig::new(Root::new::<Harness>(move || props.clone()))
    };
    config.environment.os = Os::MacOs;
    config.environment.reduce_motion = reduce_motion;
    let mut app = Headless::start(config).expect("the engine starts");
    let seen: Seen = Arc::default();
    let into = seen.clone();
    app.observe(move |_, batch| into.lock().unwrap().extend(batch.items.iter().cloned()));
    app.first_render().unwrap();
    app.wait_for("Slide 1").unwrap();
    App { app, seen }
}

fn looping() -> HarnessProps {
    HarnessProps {
        looping: true,
        ..HarnessProps::default()
    }
}

#[test]
fn previous_and_next_move_and_stop_at_the_ends() {
    let mut app = start(HarnessProps::default(), false);
    assert!(
        !app.enabled("Previous"),
        "the first slide has none before it"
    );
    let items = app.press_button("Next");
    app.wait_for("index:1").unwrap();
    assert_eq!(scrolls(&items), [(300.0, true)], "scrolled, animated");
    assert!(app.enabled("Previous"));
    for i in 2..SLIDES {
        app.press_button("Next");
        app.wait_for(&format!("index:{i}")).unwrap();
    }
    assert!(!app.enabled("Next"), "the last slide has none after it");
    let items = app.press_button("Previous");
    app.wait_for("index:3").unwrap();
    assert_eq!(scrolls(&items), [(900.0, true)]);
    app.wait_for("changes:5").unwrap();
}

#[test]
fn a_pagination_item_moves_to_its_slide_and_is_selected_while_it_shows() {
    let mut app = start(HarnessProps::default(), false);
    assert!(app.screen().selected("role:button[Go to 1]").unwrap());
    let items = app.press_button("Go to 3");
    app.wait_for("index:2").unwrap();
    assert_eq!(scrolls(&items), [(600.0, true)]);
    assert!(app.screen().selected("role:button[Go to 3]").unwrap());
    assert!(!app.screen().selected("role:button[Go to 1]").unwrap());
    // Its own slide's item reports nothing.
    app.press_button("Go to 3");
    app.wait(Duration::from_millis(50)).unwrap();
    app.wait_for("changes:1").unwrap();
}

#[test]
fn the_content_snaps_to_the_slides_one_per_fling() {
    let app = start(HarnessProps::default(), false);
    let items = app.take();
    assert_eq!(snap_offsets(&items), [0.0, 300.0, 600.0, 900.0, 1200.0]);
    let one_step = items.iter().rev().find_map(|i| match i {
        MountItem::ScrollBehavior { behavior, .. } => Some(behavior.one_step),
        _ => None,
    });
    assert_eq!(one_step, Some(true));
}

#[test]
fn a_swipe_reports_the_slide_it_rests_on_between_drag_start_and_end() {
    let mut app = start(HarnessProps::default(), false);
    app.take();
    // The finger lets go two thirds of the way: the content lands on the
    // second slide, and the app hears of it once it rests.
    app.scroll("text:Slide 1", px(200.0)).unwrap();
    app.wait_for("index:1").unwrap();
    app.wait_for("changes:1").unwrap();
    app.wait_for("events:dragStart,dragEnd").unwrap();
    let items = app.take();
    assert_eq!(scrolls(&items), [(300.0, true)], "landed on the slide");
    assert!(app.screen().selected("role:button[Go to 2]").unwrap());
    // Back past the middle of the first: the first again.
    app.scroll("text:Slide 2", px(-200.0)).unwrap();
    app.wait_for("index:0").unwrap();
    app.wait_for("events:dragStart,dragEnd,dragStart,dragEnd")
        .unwrap();
}

#[test]
fn the_app_index_scrolls_to_its_slide_without_a_change() {
    let mut app = start(
        HarnessProps {
            controlled: true,
            ..HarnessProps::default()
        },
        false,
    );
    app.take();
    app.press("text:Jump").unwrap();
    app.wait_for("index:3").unwrap();
    assert_eq!(scrolls(&app.take()), [(900.0, true)]);
    assert!(app.screen().selected("role:button[Go to 4]").unwrap());
    app.wait_for("changes:0").unwrap();
    // A control asks the app, which follows: one scroll.
    let items = app.press_button("Next");
    app.wait_for("index:4").unwrap();
    assert_eq!(scrolls(&items), [(1200.0, true)]);
}

#[test]
fn default_index_shows_its_slide_at_once() {
    let app = start(
        HarnessProps {
            start: 2,
            ..HarnessProps::default()
        },
        false,
    );
    assert_eq!(scrolls(&app.take()), [(600.0, false)]);
    assert!(app.screen().selected("role:button[Go to 3]").unwrap());
}

#[test]
fn the_keys_move_while_the_carousel_has_the_focus() {
    let mut app = start(HarnessProps::default(), false);
    // On "Before": the arrows are not the carousel's.
    app.press_key("tab").unwrap();
    app.press_key("right").unwrap();
    app.wait(Duration::from_millis(50)).unwrap();
    app.wait_for("index:0").unwrap();
    // On the carousel.
    app.press_key("tab").unwrap();
    app.press_key("right").unwrap();
    app.wait_for("index:1").unwrap();
    app.press_key("end").unwrap();
    app.wait_for("index:4").unwrap();
    // The last slide: Right moves nothing.
    app.press_key("right").unwrap();
    app.press_key("left").unwrap();
    app.wait_for("index:3").unwrap();
    app.press_key("home").unwrap();
    app.wait_for("index:0").unwrap();
    app.wait_for("changes:4").unwrap();
}

#[test]
fn focus_and_blur_report_the_carousel_as_a_whole() {
    let mut app = start(HarnessProps::default(), false);
    app.press_key("tab").unwrap();
    app.run_for(Duration::from_millis(50)).unwrap();
    app.wait_for("events:").unwrap();
    // Into the carousel.
    app.press_key("tab").unwrap();
    app.run_for(Duration::from_millis(50)).unwrap();
    app.wait_for("events:focus").unwrap();
    // From the carousel to its next control: still in it, nothing.
    app.press_key("tab").unwrap();
    app.run_for(Duration::from_millis(50)).unwrap();
    app.wait_for("events:focus").unwrap();
    // Back out before it.
    app.press_key("shift+tab").unwrap();
    app.press_key("shift+tab").unwrap();
    app.run_for(Duration::from_millis(50)).unwrap();
    app.wait_for("events:focus,blur").unwrap();
}

#[test]
fn reduced_motion_scrolls_at_once() {
    let mut app = start(HarnessProps::default(), true);
    let items = app.press_button("Next");
    app.wait_for("index:1").unwrap();
    assert_eq!(scrolls(&items), [(300.0, false)]);
}

#[test]
fn several_slides_per_view_rest_with_the_last_at_the_end() {
    let mut app = start(
        HarnessProps {
            per_view: 3,
            ..HarnessProps::default()
        },
        false,
    );
    // Five slides 100 wide in 300: the range ends at 200, where the last
    // three rest.
    app.press_button("Next");
    app.wait_for("index:1").unwrap();
    let items = app.press_button("Next");
    app.wait_for("index:2").unwrap();
    assert_eq!(scrolls(&items), [(200.0, true)]);
    assert!(!app.enabled("Next"), "the third rests where the last does");
    // The fifth's item shows the same place; previous goes to the second.
    let items = app.press_button("Go to 5");
    app.wait_for("index:4").unwrap();
    assert_eq!(scrolls(&items), []);
    app.press_button("Previous");
    app.wait_for("index:1").unwrap();
}

#[test]
fn a_change_updates_the_two_pagination_items_and_scrolls_once() {
    let mut app = start(HarnessProps::default(), false);
    let items = app.press_button("Go to 2");
    app.wait_for("index:1").unwrap();
    assert_eq!(scrolls(&items), [(300.0, true)]);
    // The two pagination items, the previous button, the scroll and the
    // harness's two texts; no slide.
    assert!(
        items.len() <= 10,
        "a change sends a handful of items, not one per slide: {items:#?}"
    );
    assert!(
        !items
            .iter()
            .any(|i| matches!(i, MountItem::Create { .. } | MountItem::Delete { .. })),
        "nothing is made again"
    );
}

#[test]
fn the_pagination_indicator_follows_the_slide_shown() {
    let mut app = start(
        HarnessProps {
            indicator: true,
            ..HarnessProps::default()
        },
        false,
    );
    let frames = |items: &[MountItem]| -> Vec<(NodeId, f32)> {
        items
            .iter()
            .filter_map(|i| match i {
                MountItem::Frame { id, frame } => Some((*id, frame.origin.x.0)),
                _ => None,
            })
            .collect()
    };
    app.take();
    let items = app.press_button("Go to 4");
    app.wait_for("index:3").unwrap();
    // The indicator moves to the fourth item, 10 wide each: x 30.
    assert!(
        frames(&items).iter().any(|(_, x)| (*x - 30.0).abs() < 0.01),
        "the indicator moved to the fourth item: {items:#?}"
    );
}

#[test]
fn looping_rests_in_the_middle_period_between_runways() {
    let app = start(looping(), false);
    let items = app.take();
    // Five slides 300 wide: a period of 1500, a runway of 1500 on each
    // side; the first slide rests at 1500, and the snap points cover three
    // periods.
    assert_eq!(scrolls(&items).last(), Some(&(1500.0, false)));
    let snaps = snap_offsets(&items);
    assert_eq!(snaps.len(), 15);
    assert_eq!(snaps.first(), Some(&0.0));
    assert_eq!(snaps.last(), Some(&4200.0));
    // Each slide is there once, for screen readers too.
    for i in 1..=SLIDES {
        assert_eq!(
            app.screen()
                .count(&format!("label:{i} of {SLIDES}"))
                .unwrap(),
            1
        );
    }
    // Each slide's translation: a shift and three derived nodes, with the
    // offset and the period.
    assert!(made_nodes(&items) >= 2 + 4 * SLIDES as usize);
}

#[test]
fn next_on_the_last_slide_goes_across_the_seam_and_recenters_silently() {
    let mut app = start(
        HarnessProps {
            looping: true,
            start: 4,
            ..HarnessProps::default()
        },
        false,
    );
    assert!(app.enabled("Next"));
    let items = app.press_button("Next");
    app.wait_for("index:0").unwrap();
    // One step forward, into the next period: no rewind across the slides.
    assert_eq!(scrolls(&items), [(3000.0, true)]);
    app.wait_for("changes:1").unwrap();
    // Once it rests: exactly one scroll back into the middle period, not
    // animated, and no change.
    let items = app.after_wrap();
    assert_eq!(scrolls(&items), [(1500.0, false)]);
    app.wait_for("changes:1").unwrap();
    assert!(app.screen().selected("role:button[Go to 1]").unwrap());
}

#[test]
fn previous_on_the_first_slide_goes_back_across_the_seam() {
    let mut app = start(looping(), false);
    assert!(app.enabled("Previous"));
    let items = app.press_button("Previous");
    app.wait_for("index:4").unwrap();
    assert_eq!(scrolls(&items), [(1200.0, true)]);
    assert_eq!(scrolls(&app.after_wrap()), [(2700.0, false)]);
    app.wait_for("changes:1").unwrap();
}

#[test]
fn the_app_going_from_the_last_slide_to_the_first_wraps_forward() {
    let mut app = start(
        HarnessProps {
            looping: true,
            controlled: true,
            start: 4,
            ..HarnessProps::default()
        },
        false,
    );
    app.take();
    app.press("text:First").unwrap();
    app.wait_for("index:0").unwrap();
    assert_eq!(scrolls(&app.take()), [(3000.0, true)]);
    assert_eq!(scrolls(&app.after_wrap()), [(1500.0, false)]);
    app.wait_for("changes:0").unwrap();
}

#[test]
fn a_swipe_crosses_the_seam() {
    let mut app = start(looping(), false);
    app.take();
    // From the first slide back past the seam: the last one, in the period
    // before, then silently back in the middle.
    app.scroll("text:Slide 1", px(-200.0)).unwrap();
    app.wait_for("index:4").unwrap();
    app.wait_for("changes:1").unwrap();
    let items = app.take();
    assert_eq!(
        scrolls(&items),
        [(1200.0, true)],
        "landed on the last slide"
    );
    assert_eq!(scrolls(&app.after_wrap()), [(2700.0, false)]);
    app.wait_for("changes:1").unwrap();
    app.wait_for("events:dragStart,dragEnd").unwrap();
}

#[test]
fn a_relayout_makes_no_animation_nodes() {
    let mut app = start(looping(), false);
    app.take();
    // A padding of 20 before the runway moves every slide and the runway by
    // 20: the period and the snap points stay, so the first slide still
    // rests at 1500 (20 into the viewport, as without looping). The layout
    // reports each slide's frame in turn; the track is made once all of
    // them are in, so no scroll is sent, not even to an offset between.
    app.press("text:Pad").unwrap();
    app.wait(Duration::from_millis(50)).unwrap();
    let items = app.take();
    assert_eq!(
        made_nodes(&items),
        0,
        "the translations are reused: {items:#?}"
    );
    assert!(sets(&items) > 0, "the slides' shifts are set");
    assert_eq!(
        scrolls(&items),
        [],
        "the slide stays where it rests: {items:#?}"
    );
}

#[test]
fn without_looping_there_are_no_runways_or_translations() {
    let app = start(HarnessProps::default(), false);
    let items = app.take();
    assert_eq!(made_nodes(&items), 0);
    assert_eq!(snap_offsets(&items).len(), SLIDES as usize);
}
