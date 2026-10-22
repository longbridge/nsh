//! `Sheet`: it comes in when opened and goes once its slide-out ends, also
//! on a manual clock that only the test moves (its end is the motion's,
//! not a timer of the app's clock).

use nsh_base::sheet;
use nsh_core::{
    Size,
    ui::{Root, prelude::*},
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

/// A button that opens and closes a sheet holding `Sheet body`.
#[component]
fn Screen() -> impl IntoElement {
    let open = signal(false);
    view()
        .child(
            view()
                .test_id("toggle")
                .w(px(40.0))
                .h(px(40.0))
                .on_press(move || open.set(!open.get_untracked())),
        )
        .child(
            sheet()
                .open(move || Some(open.get()))
                .child(view().h(px(200.0)).child(text("Sheet body"))),
        )
}

#[test]
fn a_sheet_goes_once_its_motion_ends_on_a_manual_clock() {
    let mut app = Headless::start(EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        clock: nsh_engine::Clock::manual(std::time::SystemTime::now()),
        ..EngineConfig::new(Root::of::<Screen>())
    })
    .expect("the engine starts");
    app.first_render().unwrap();
    app.press("id:toggle").unwrap();
    app.wait_for("Sheet body").unwrap();
    app.press("id:toggle").unwrap();
    app.wait_for_gone("Sheet body", std::time::Duration::from_secs(5))
        .unwrap();
}
