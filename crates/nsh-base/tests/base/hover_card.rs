//! `HoverCard` on a Mac: it opens once the pointer rested on the trigger
//! for its open delay (or the keyboard focus came to it), stays open while
//! the pointer moves into the card, closes its close delay after the
//! pointer left both, and at once on Escape; a controlled card shows what
//! the app says; a hover sends the platform nothing until the card opens.
//! On a phone a press opens and closes it. On a manual clock: time moves
//! only as the test says.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use nsh_base::{HoverCardEvent, hover_card};
use nsh_core::{
    Size,
    element::Root,
    platform::{Idiom, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

#[derive(Clone, PartialEq, Props)]
struct ScreenProps {
    /// The app owns `open`, changed only by pressing "Toggle".
    controlled: bool,
}

/// A mention whose card previews a profile, a place away from it, and
/// what `onOpenChange` reported on screen.
#[component]
fn HoverScreen(props: ScreenProps) -> impl IntoElement {
    let changes = signal(Vec::<String>::new());
    let shown = signal(false);
    let card = hover_card()
        .offset(px(4.0))
        .on_open_change(move |e: &HoverCardEvent| {
            if let HoverCardEvent::OpenChange(open, reason) = e {
                let change = format!("{open}:{reason:?}");
                changes.update(|c| c.push(change));
            }
        })
        .trigger(
            view()
                .test_id("trigger")
                .focusable(true)
                .w(px(100.0))
                .h(px(30.0))
                .child(text("@ada")),
        )
        .child(
            view()
                .test_id("card")
                .w(px(200.0))
                .h(px(80.0))
                .child(text("Ada Lovelace")),
        );
    let card = if props.controlled {
        card.open(move || Some(shown.get()))
    } else {
        card
    };
    view()
        .pt(px(100.0))
        .pl(px(40.0))
        .child(card)
        .child(view().test_id("away").h(px(200.0)).child(text("Away")))
        .child(
            view()
                .test_id("toggle")
                .w(px(100.0))
                .h(px(30.0))
                .on_press(move || shown.set(!shown.get_untracked()))
                .child(text("Toggle")),
        )
        .child(text(move || {
            format!("Changes: {}", changes.get().join(" "))
        }))
}

/// The screen on a Mac (or an iPhone) on a manual clock, counting the
/// mount items the platform is sent.
fn start(controlled: bool, desktop: bool) -> (Headless, Arc<AtomicUsize>) {
    let mut config = EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        clock: nsh_engine::Clock::manual(std::time::SystemTime::now()),
        ..EngineConfig::new(Root::new::<HoverScreen>(move || ScreenProps { controlled }))
    };
    if desktop {
        config.environment.os = Os::MacOs;
        config.environment.idiom = Idiom::Desktop;
    } else {
        config.environment.os = Os::Ios;
        config.environment.idiom = Idiom::Phone;
    }
    let mut app = Headless::start(config).expect("the engine starts");
    let items = Arc::new(AtomicUsize::new(0));
    let seen = items.clone();
    app.observe(move |_, batch| {
        seen.fetch_add(batch.items.len(), Ordering::Relaxed);
    });
    app.first_render().unwrap();
    (app, items)
}

/// Hovers the trigger and waits out the open delay (600 ms).
fn open_by_hover(app: &mut Headless) {
    app.hover("id:trigger").unwrap();
    app.run_for(Duration::from_millis(650)).unwrap();
    assert!(app.shows("Ada Lovelace"), "open after its delay");
}

#[test]
fn a_hover_card_opens_once_the_pointer_rested_for_its_delay() {
    let (mut app, _) = start(false, true);
    app.hover("id:trigger").unwrap();
    app.run_for(Duration::from_millis(500)).unwrap();
    assert!(!app.shows("Ada Lovelace"), "not before 600 ms");
    app.run_for(Duration::from_millis(150)).unwrap();
    assert!(app.shows("Ada Lovelace"));
    app.wait_for("Changes: true:Hover").unwrap();
}

#[test]
fn leaving_the_trigger_before_the_delay_never_opens_it() {
    let (mut app, _) = start(false, true);
    app.hover("id:trigger").unwrap();
    app.run_for(Duration::from_millis(300)).unwrap();
    app.hover("id:away").unwrap();
    app.run_for(Duration::from_millis(1000)).unwrap();
    assert!(!app.shows("Ada Lovelace"));
    assert!(app.shows("Changes:"), "nothing asked");
}

#[test]
fn a_hover_card_stays_open_while_the_pointer_moves_into_it() {
    let (mut app, _) = start(false, true);
    open_by_hover(&mut app);
    // Off the trigger, onto the card: within the close delay.
    app.hover("id:card").unwrap();
    app.run_for(Duration::from_millis(1000)).unwrap();
    assert!(app.shows("Ada Lovelace"), "kept while the pointer is on it");
    // And back to the trigger.
    app.hover("id:trigger").unwrap();
    app.run_for(Duration::from_millis(1000)).unwrap();
    assert!(app.shows("Ada Lovelace"));
    app.wait_for("Changes: true:Hover").unwrap();
}

#[test]
fn a_hover_card_closes_its_delay_after_the_pointer_left_both() {
    let (mut app, _) = start(false, true);
    open_by_hover(&mut app);
    app.hover("id:card").unwrap();
    app.hover("id:away").unwrap();
    app.run_for(Duration::from_millis(250)).unwrap();
    assert!(app.shows("Ada Lovelace"), "not before 300 ms");
    app.run_for(Duration::from_millis(100)).unwrap();
    assert!(!app.shows("Ada Lovelace"));
    app.wait_for("Changes: true:Hover false:Hover").unwrap();
}

#[test]
fn escape_closes_a_hover_card_at_once() {
    let (mut app, _) = start(false, true);
    open_by_hover(&mut app);
    app.press_key("escape").unwrap();
    assert!(!app.shows("Ada Lovelace"));
    app.wait_for("Changes: true:Hover false:Cancel").unwrap();
    // The pointer still on the trigger opens it only once it comes again.
    app.run_for(Duration::from_millis(1000)).unwrap();
    assert!(!app.shows("Ada Lovelace"));
}

#[test]
fn the_keyboard_focus_opens_a_hover_card_and_leaving_closes_it() {
    let (mut app, _) = start(false, true);
    app.press_key("tab").unwrap();
    app.run_for(Duration::from_millis(650)).unwrap();
    assert!(app.shows("Ada Lovelace"), "open after its delay");
    app.wait_for("Changes: true:Focus").unwrap();
    app.press_key("tab").unwrap();
    app.run_for(Duration::from_millis(350)).unwrap();
    assert!(!app.shows("Ada Lovelace"));
    app.wait_for("Changes: true:Focus false:Focus").unwrap();
}

#[test]
fn a_blur_of_the_trigger_leaves_the_card_open_while_the_pointer_is_on_it() {
    let (mut app, _) = start(false, true);
    app.press_key("tab").unwrap();
    app.run_for(Duration::from_millis(650)).unwrap();
    assert!(app.shows("Ada Lovelace"));
    app.hover("id:card").unwrap();
    // The focus leaves the trigger for the next stop.
    app.press_key("tab").unwrap();
    app.run_for(Duration::from_millis(1000)).unwrap();
    assert!(app.shows("Ada Lovelace"), "the pointer keeps it");
    app.wait_for("Changes: true:Focus").unwrap();
    // Off the card: now it closes.
    app.hover("id:away").unwrap();
    app.run_for(Duration::from_millis(350)).unwrap();
    assert!(!app.shows("Ada Lovelace"));
    app.wait_for("Changes: true:Focus false:Hover").unwrap();
}

#[test]
fn a_controlled_hover_card_shows_what_the_app_says() {
    let (mut app, _) = start(true, true);
    app.hover("id:trigger").unwrap();
    app.run_for(Duration::from_millis(650)).unwrap();
    app.wait_for("Changes: true:Hover").unwrap();
    assert!(!app.shows("Ada Lovelace"), "asked, the app keeps it closed");
    app.press("id:toggle").unwrap();
    app.wait_for("Ada Lovelace").unwrap();
    app.press("id:toggle").unwrap();
    app.wait_for_gone("Ada Lovelace", Duration::from_secs(2))
        .unwrap();
}

#[test]
fn a_hover_sends_nothing_until_the_card_opens() {
    let (mut app, items) = start(false, true);
    let before = items.load(Ordering::Relaxed);
    app.hover("id:trigger").unwrap();
    app.run_for(Duration::from_millis(500)).unwrap();
    app.hover("id:away").unwrap();
    app.run_for(Duration::from_millis(500)).unwrap();
    assert_eq!(
        items.load(Ordering::Relaxed),
        before,
        "resting and leaving before the delay send nothing"
    );
    app.hover("id:trigger").unwrap();
    app.run_for(Duration::from_millis(650)).unwrap();
    let opened = items.load(Ordering::Relaxed);
    assert!(app.shows("Ada Lovelace"));
    // The card, its content and the report on screen.
    assert!(
        opened - before <= 40,
        "{} items to open the card",
        opened - before
    );
    // Moving between the trigger and the card sends nothing.
    app.hover("id:card").unwrap();
    app.hover("id:trigger").unwrap();
    app.run_for(Duration::from_millis(1000)).unwrap();
    assert_eq!(items.load(Ordering::Relaxed), opened);
}

#[test]
fn on_a_phone_a_press_opens_and_closes_a_hover_card() {
    let (mut app, _) = start(false, false);
    app.press("id:trigger").unwrap();
    app.wait_for("Ada Lovelace").unwrap();
    app.press("id:trigger").unwrap();
    app.wait_for_gone("Ada Lovelace", Duration::from_secs(2))
        .unwrap();
    app.press("id:trigger").unwrap();
    app.wait_for("Ada Lovelace").unwrap();
    app.press("id:toggle").unwrap();
    app.wait_for_gone("Ada Lovelace", Duration::from_secs(2))
        .unwrap();
    app.wait_for(
        "Changes: true:TriggerPress false:TriggerPress true:TriggerPress false:PressOutside",
    )
    .unwrap();
}
