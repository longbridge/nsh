//! The app plugin calls the same theme service as the official facade.

use std::sync::OnceLock;

use nsh_core::{Color, platform::ColorScheme};
use nsh_engine::Bundle;
use nsh_testing::Headless;

use super::common::{press, start_bundle, ui_sets};

fn start() -> Headless {
    static APP: OnceLock<Bundle> = OnceLock::new();
    let bundle = APP.get_or_init(|| {
        let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Workspace.tsx");
        nsh_testing::compile_tsx(entry, &ui_sets()).expect("Workspace.tsx builds")
    });
    start_bundle(bundle.clone(), "Workspace: built")
}

fn preview(app: &Headless) -> Color {
    app.screen().span_style("Workspace preview").unwrap().color
}

#[test]
fn the_user_plugin_publishes_tokens_and_classes_and_restores_the_built_theme() {
    let mut app = start();
    app.wait_for("Accent: #2563ebff").unwrap();
    assert_eq!(preview(&app), Color(0x2563ebff));

    press(&mut app, "Ocean", "Workspace: ocean");
    assert!(app.shows("Read at once: #0369a1ff"));
    assert_eq!(preview(&app), Color(0x0369a1ff));

    app.set_environment("dark", |env| env.color_scheme = ColorScheme::Dark)
        .unwrap();
    app.wait_for("Accent: #67e8f9ff").unwrap();
    assert_eq!(preview(&app), Color(0x67e8f9ff));

    press(&mut app, "Orchid", "Workspace: orchid");
    assert!(app.shows("Read at once: #d8b4feff"));
    assert_eq!(preview(&app), Color(0xd8b4feff));

    press(&mut app, "Built theme", "Workspace: built");
    assert!(app.shows("Read at once: #93c5fdff"));
    assert_eq!(preview(&app), Color(0x93c5fdff));
}

#[test]
fn workspace_selection_and_theme_state_belong_to_each_engine() {
    let mut ocean = start();
    let mut orchid = start();

    press(&mut ocean, "Ocean", "Workspace: ocean");
    assert!(orchid.shows("Workspace: built"));
    assert_eq!(preview(&orchid), Color(0x2563ebff));

    press(&mut orchid, "Orchid", "Workspace: orchid");
    assert!(ocean.shows("Workspace: ocean"));
    assert_eq!(preview(&ocean), Color(0x0369a1ff));
    assert_eq!(preview(&orchid), Color(0x7e22ceff));

    drop(ocean);
    press(&mut orchid, "Built theme", "Workspace: built");
    assert_eq!(preview(&orchid), Color(0x2563ebff));
}
