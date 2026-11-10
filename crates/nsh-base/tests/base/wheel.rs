//! Raw wheel and pinch events (`on_wheel`, `on_magnify`): the innermost
//! view under the pointer that listens takes them, before any scroll view
//! around it, and nothing else scrolls with them. A desktop, on a Mac.

use nsh_core::{
    MagnifyPhase, Size, WheelPhase,
    element::Root,
    geometry::Percentage,
    platform::{Idiom, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

/// A scroll view 300 by 200 of 1000 points holding a `plain` view and a
/// `chart` that listens to the wheel and to pinches, with what it heard
/// and the scroll offset shown as text; an `outer` listener around the
/// scroll view listens to the wheel too.
#[component]
fn Page() -> impl IntoElement {
    let offset = signal(0.0_f32);
    let heard = signal(String::from("nothing"));
    view()
        .test_id("outer")
        .on_wheel(move |e: &WheelEvent| {
            heard.set(format!("outer {}", e.delta.y.0));
        })
        .child(
            scroll()
                .test_id("page")
                .w(px(300.0))
                .h(px(200.0))
                .on_scroll(move |e: &ScrollEvent| offset.set(e.offset.y.0))
                .child(
                    view()
                        .w_full()
                        .h(px(1000.0))
                        .child(view().test_id("plain").w_full().h(px(100.0)))
                        .child(
                            view()
                                .test_id("chart")
                                .w_full()
                                .h(px(100.0))
                                .on_wheel(move |e: &WheelEvent| {
                                    heard.set(format!(
                                        "chart {} {} {:?} {} {} at {} {}",
                                        e.delta.x.0,
                                        e.delta.y.0,
                                        e.phase,
                                        e.modifiers.ctrl,
                                        e.precise,
                                        e.position.x.0,
                                        e.position.y.0
                                    ));
                                })
                                .on_magnify(move |e: &MagnifyEvent| {
                                    heard.set(format!("pinch {} {:?}", e.delta.0, e.phase));
                                }),
                        ),
                ),
        )
        .child(text(move || format!("Offset {}", offset.get().round())))
        .child(text(move || format!("Heard {}", heard.get())))
}

fn start() -> Headless {
    let mut config = EngineConfig {
        viewport: Size::new(px(400.0), px(400.0)),
        ..EngineConfig::new(Root::of::<Page>())
    };
    config.environment.os = Os::MacOs;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).expect("the engine starts");
    app.wait_for("Offset 0").unwrap();
    app
}

#[test]
fn the_innermost_listener_takes_the_wheel_and_nothing_scrolls() {
    let mut app = start();
    app.wheel_at("id:chart", point(px(0.0), px(40.0)), WheelPhase::Changed)
        .unwrap();
    // The chart, not the listener around the scroll view, heard it with
    // where it was and that it came from a touchpad; the scroll view
    // stayed.
    app.wait_for("Heard chart 0 40 Changed false true at 150 50")
        .unwrap();
    assert!(app.shows("Offset 0"), "the scroll view stayed");
}

#[test]
fn a_listener_around_the_scroll_view_takes_wheels_over_views_with_none() {
    let mut app = start();
    // The plain view has no listener of its own: the nearest one around
    // it hears the wheel, before the scroll view between them.
    app.wheel_at("id:plain", point(px(0.0), px(120.0)), WheelPhase::Unknown)
        .unwrap();
    app.wait_for("Heard outer 120").unwrap();
    assert!(app.shows("Offset 0"));
}

#[test]
fn without_any_listener_the_wheel_scrolls() {
    // Only the scroll view: no `on_wheel` around it.
    #[component]
    fn Bare() -> impl IntoElement {
        let offset = signal(0.0_f32);
        view()
            .child(
                scroll()
                    .test_id("page")
                    .w(px(300.0))
                    .h(px(200.0))
                    .on_scroll(move |e: &ScrollEvent| offset.set(e.offset.y.0))
                    .child(view().test_id("row").w_full().h(px(1000.0))),
            )
            .child(text(move || format!("Offset {}", offset.get().round())))
    }
    let mut config = EngineConfig {
        viewport: Size::new(px(400.0), px(400.0)),
        ..EngineConfig::new(Root::of::<Bare>())
    };
    config.environment.os = Os::MacOs;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).expect("the engine starts");
    app.wait_for("Offset 0").unwrap();
    app.wheel_at("id:row", point(px(0.0), px(120.0)), WheelPhase::Unknown)
        .unwrap();
    app.wait_for("Offset 120").unwrap();
}

#[test]
fn ctrl_wheel_reaches_the_listener_with_the_modifier() {
    let mut app = start();
    app.hold_keys("ctrl", true).unwrap();
    app.wheel_at("id:chart", point(px(0.0), px(-24.0)), WheelPhase::Unknown)
        .unwrap();
    app.wait_for("Heard chart 0 -24 Unknown true false at 150 50")
        .unwrap();
    // Nothing pinched or scrolled.
    assert!(app.shows("Offset 0"));
}

#[test]
fn a_touchpad_pinch_reaches_an_on_magnify_listener() {
    let mut app = start();
    app.magnify_at("id:chart", Percentage(0.02), MagnifyPhase::Began)
        .unwrap();
    app.wait_for("Heard pinch 0.02 Began").unwrap();
    app.magnify_at("id:chart", Percentage(-0.5), MagnifyPhase::Ended)
        .unwrap();
    app.wait_for("Heard pinch -0.5 Ended").unwrap();
}

#[test]
fn a_phone_has_no_wheel() {
    let mut config = EngineConfig {
        viewport: Size::new(px(400.0), px(400.0)),
        ..EngineConfig::new(Root::of::<Page>())
    };
    config.environment.os = Os::Ios;
    config.environment.idiom = Idiom::Phone;
    let mut app = Headless::start(config).expect("the engine starts");
    assert!(
        app.wheel_at("id:chart", point(px(0.0), px(1.0)), WheelPhase::Unknown)
            .is_err()
    );
}
