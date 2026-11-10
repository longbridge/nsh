//! `Button`: `hitSlop` widens where presses count past its bounds, as a
//! view's does: the platforms get it on the button's pressable view, it
//! follows a binding, and a button without one gets none. The looks the
//! app declares for buttons apply by the `variant` and `size` a button
//! picks, before its use site's styles, and cost nothing undeclared.

use std::sync::{Arc, Mutex};

use nsh_base::button;
use nsh_core::{
    EdgeInsets, MountBatch, MountItem, NodeId, Size, ViewStyle,
    element::{ComponentStyle, Root, Tokens, set_component_style, set_tokens, tokens},
    platform::{ColorScheme, Idiom, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

/// Every batch's items.
type Seen = Arc<Mutex<Vec<MountItem>>>;

fn start<C: Component<Props = ()>>() -> (Headless, Seen) {
    let config = EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        ..EngineConfig::new(Root::of::<C>())
    };
    let mut app = Headless::start(config).unwrap();
    let seen = Seen::default();
    let log = seen.clone();
    app.observe(move |_: &str, batch: &MountBatch| {
        log.lock().unwrap().extend(batch.items.iter().cloned());
    });
    app.first_render().unwrap();
    (app, seen)
}

/// The hit slop the platforms last got for view `id` (none: zero).
fn hit_slop(seen: &Seen, id: NodeId) -> EdgeInsets {
    seen.lock()
        .unwrap()
        .iter()
        .filter_map(|item| match item {
            MountItem::ViewProps { id: of, props } if *of == id => Some(props.hit_slop),
            _ => None,
        })
        .next_back()
        .unwrap_or_default()
}

/// A small close button with a slop on every side, a button whose slop the
/// app widens, and a button with none.
#[component]
fn Buttons() -> impl IntoElement {
    let presses = signal(0u32);
    let wide = signal(false);
    view()
        .child(
            button()
                .hit_slop(EdgeInsets::all(px(12.0)))
                .accessibility_label("Close")
                .on_press(move || presses.update(|n| *n += 1))
                .child(text("×")),
        )
        .child(
            button()
                .hit_slop(Prop::bind(move || match wide.get() {
                    true => EdgeInsets {
                        top: px(4.0),
                        left: px(2.0),
                        ..EdgeInsets::default()
                    },
                    false => EdgeInsets::default(),
                }))
                .child(text("Grows")),
        )
        .child(
            button()
                .on_press(move || wide.set(true))
                .child(text("Widen")),
        )
        .child(text(move || format!("presses:{}", presses.get())))
}

fn target(app: &Headless, selector: &str) -> NodeId {
    app.screen()
        .press_target(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no pressable {selector}"))
}

#[test]
fn hit_slop_reaches_the_pressable_view() {
    let (mut app, seen) = start::<Buttons>();
    let close = target(&app, "label:Close");
    assert_eq!(hit_slop(&seen, close), EdgeInsets::all(px(12.0)));
    // The slop changes nothing of what a press does.
    app.press("label:Close").unwrap();
    app.wait_for("text:presses:1").unwrap();
}

#[test]
fn hit_slop_follows_a_binding_and_is_none_by_default() {
    let (mut app, seen) = start::<Buttons>();
    let grows = target(&app, "text:Grows");
    let widen = target(&app, "text:Widen");
    assert_eq!(hit_slop(&seen, grows), EdgeInsets::default());
    app.press("text:Widen").unwrap();
    let expected = EdgeInsets {
        top: px(4.0),
        left: px(2.0),
        ..EdgeInsets::default()
    };
    assert_eq!(hit_slop(&seen, grows), expected);
    // A button without `hitSlop` never gets one.
    assert_eq!(hit_slop(&seen, widen), EdgeInsets::default());
}

// The looks an app declares for buttons (`set_component_style("button",
// ..)`), picked by `variant` and `size`.

const PRIMARY: u32 = 0x4f46_e5ff;
const OUTLINE: u32 = 0x00ff_00ff;
const USE_SITE: u32 = 0xff00_00ff;

/// The style items each batch sent, in order.
type Batches = Arc<Mutex<Vec<Vec<MountItem>>>>;

/// A desktop app (hover reaches views) of root `C`, recording each batch.
fn start_desktop<C: Component<Props = ()>>() -> (Headless, Batches) {
    let mut config = EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        ..EngineConfig::new(Root::of::<C>())
    };
    config.environment.os = Os::MacOs;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).unwrap();
    let batches = Batches::default();
    let log = batches.clone();
    app.observe(move |_: &str, batch: &MountBatch| {
        log.lock().unwrap().push(batch.items.clone());
    });
    app.first_render().unwrap();
    (app, batches)
}

/// The style view `id` was last sent.
fn style_of(batches: &Batches, id: NodeId) -> Option<ViewStyle> {
    batches
        .lock()
        .unwrap()
        .iter()
        .flatten()
        .filter_map(|item| match item {
            MountItem::ViewStyle { id: of, style } if *of == id => Some((**style).clone()),
            _ => None,
        })
        .next_back()
}

fn background(batches: &Batches, id: NodeId) -> u32 {
    style_of(batches, id).map_or(0, |s| s.background_color.0)
}

/// The looks of the tests: `primary` and `outline` variants, a `lg` size.
fn declare() {
    set_component_style(
        "button",
        ComponentStyle::new()
            .variant("primary", |s| s.bg(rgb(0x4f46e5)))
            .variant("outline", |s| s.border_2().bg(rgb(0x00ff00)))
            .size("lg", |s| s.rounded(px(8.0))),
    );
}

#[component]
fn Declared() -> impl IntoElement {
    declare();
    view().child(
        button()
            .variant("primary")
            .size("lg")
            .accessibility_label("Go")
            .child(text("Go")),
    )
}

#[test]
fn a_declared_variant_styles_the_button() {
    let (app, batches) = start_desktop::<Declared>();
    let go = target(&app, "label:Go");
    assert_eq!(background(&batches, go), PRIMARY);
    assert_eq!(style_of(&batches, go).unwrap().border_radius, px(8.0));
}

#[component]
fn Defaults() -> impl IntoElement {
    set_component_style(
        "button",
        ComponentStyle::new()
            .variant("primary", |s| s.bg(rgb(0x4f46e5)))
            .size("lg", |s| s.rounded(px(8.0)))
            .default_variant("primary")
            .default_size("lg"),
    );
    view().child(button().accessibility_label("Go").child(text("Go")))
}

#[test]
fn the_default_variant_and_size_apply_when_unset() {
    let (app, batches) = start_desktop::<Defaults>();
    let go = target(&app, "label:Go");
    assert_eq!(background(&batches, go), PRIMARY);
    assert_eq!(style_of(&batches, go).unwrap().border_radius, px(8.0));
}

#[component]
fn Switching() -> impl IntoElement {
    declare();
    let outline = signal(false);
    view()
        .child(
            button()
                .variant(move || -> Option<SharedString> {
                    Some(if outline.get() { "outline" } else { "primary" }.into())
                })
                .accessibility_label("Go")
                .child(text("Go")),
        )
        .child(
            button()
                .on_press(move || outline.set(true))
                .child(text("Outline")),
        )
}

#[test]
fn changing_the_variant_restyles_in_one_commit() {
    let (mut app, batches) = start_desktop::<Switching>();
    let go = target(&app, "label:Go");
    assert_eq!(background(&batches, go), PRIMARY);
    let from = batches.lock().unwrap().len();
    app.press("text:Outline").unwrap();
    app.wait(std::time::Duration::from_millis(50)).unwrap();
    // One batch restyles the button: primary's background gone, the
    // outline's border and background in its place.
    let styles: Vec<ViewStyle> = batches.lock().unwrap()[from..]
        .iter()
        .flatten()
        .filter_map(|item| match item {
            MountItem::ViewStyle { id, style } if *id == go => Some((**style).clone()),
            _ => None,
        })
        .collect();
    assert_eq!(styles.len(), 1, "one style for the button");
    assert_eq!(styles[0].background_color.0, OUTLINE);
    assert_eq!(styles[0].borders.widths.to_array().map(|w| w.0), [2.0; 4]);
}

#[component]
fn Hovered() -> impl IntoElement {
    set_component_style(
        "button",
        ComponentStyle::new().variant_classes("primary", "bg-white hover:bg-black"),
    );
    view().child(
        button()
            .variant("primary")
            .accessibility_label("Go")
            .w(px(80.0))
            .h(px(40.0))
            .child(text("Go")),
    )
}

#[test]
fn a_declared_hover_style_follows_hover() {
    let (mut app, batches) = start_desktop::<Hovered>();
    let go = target(&app, "label:Go");
    assert_eq!(background(&batches, go), 0xffff_ffff);
    app.hover("label:Go").unwrap();
    assert_eq!(background(&batches, go), 0x0000_00ff);
}

/// Every warning logged in this test binary.
fn warnings() -> &'static Mutex<Vec<String>> {
    struct Capture;
    static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Warn
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                WARNINGS.lock().unwrap().push(record.args().to_string());
            }
        }

        fn flush(&self) {}
    }
    static CAPTURE: Capture = Capture;
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        if log::set_logger(&CAPTURE).is_ok() {
            log::set_max_level(log::LevelFilter::Warn);
        }
    });
    &WARNINGS
}

#[component]
fn Undeclared() -> impl IntoElement {
    declare();
    let again = signal(0u32);
    view()
        .child(
            button()
                .variant(move || -> Option<SharedString> {
                    again.get();
                    Some("nonexistent-variant".into())
                })
                .accessibility_label("Go")
                .child(text("Go")),
        )
        .child(
            button()
                .variant("nonexistent-variant")
                .accessibility_label("Other")
                .child(text("Other")),
        )
        .child(
            button()
                .on_press(move || again.update(|n| *n += 1))
                .child(text("Again")),
        )
}

#[test]
fn an_undeclared_variant_applies_nothing() {
    let warnings = warnings();
    let (mut app, batches) = start_desktop::<Undeclared>();
    let go = target(&app, "label:Go");
    assert_eq!(style_of(&batches, go), None);
    app.press("text:Again").unwrap();
    app.wait(std::time::Duration::from_millis(50)).unwrap();
    assert_eq!(style_of(&batches, go), None);
    let warned = warnings
        .lock()
        .unwrap()
        .iter()
        .filter(|w| w.contains("`nonexistent-variant`"))
        .count();
    assert_eq!(warned, 1, "one warning per component and name");
}

#[component]
fn UseSite() -> impl IntoElement {
    declare();
    view().child(
        button()
            .variant("primary")
            .bg(rgb(0xff0000))
            .accessibility_label("Go")
            .child(text("Go")),
    )
}

#[test]
fn the_use_site_style_goes_after_the_declared_one() {
    let (app, batches) = start_desktop::<UseSite>();
    let go = target(&app, "label:Go");
    assert_eq!(background(&batches, go), USE_SITE);
}

#[component]
fn Tokened() -> impl IntoElement {
    let mut theme = Tokens::default();
    theme.set_color("action", AdaptiveColor::new(rgb(0x112233), rgb(0x445566)));
    set_tokens(theme);
    set_component_style(
        "button",
        ComponentStyle::new().variant("primary", |s| {
            s.bg(move || tokens().color("action").unwrap_or_default())
        }),
    );
    view().child(
        button()
            .variant("primary")
            .accessibility_label("Go")
            .child(text("Go")),
    )
}

#[test]
fn declared_token_colors_follow_dark() {
    let (mut app, batches) = start_desktop::<Tokened>();
    // The app's tokens reach the look a turn after the first render.
    app.wait(std::time::Duration::from_millis(50)).unwrap();
    let go = target(&app, "label:Go");
    assert_eq!(background(&batches, go), 0x1122_33ff);
    app.set_environment("dark", |env| env.color_scheme = ColorScheme::Dark)
        .unwrap();
    assert_eq!(background(&batches, go), 0x4455_66ff);
}

#[component]
fn Plain() -> impl IntoElement {
    view().child(button().accessibility_label("Go").child(text("Go")))
}

#[component]
fn PickedUndeclared() -> impl IntoElement {
    view().child(
        button()
            .variant("primary")
            .size("lg")
            .accessibility_label("Go")
            .child(text("Go")),
    )
}

#[test]
fn no_declaration_adds_no_mount_items() {
    let (app, plain) = start_desktop::<Plain>();
    let go = target(&app, "label:Go");
    // A button has no look of its own: no style for its view.
    assert_eq!(style_of(&plain, go), None);
    // Picking a variant and size nothing declares changes nothing sent.
    let (_, picked) = start_desktop::<PickedUndeclared>();
    assert_eq!(
        format!("{:?}", plain.lock().unwrap()),
        format!("{:?}", picked.lock().unwrap())
    );
}
