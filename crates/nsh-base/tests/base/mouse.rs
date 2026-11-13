//! The mouse over a component: what a view gives its element
//! (`.on_mouse_enter(..)`, ...) hears it on the component's root, as on a
//! view, after the component's own listener; a component given none
//! listens to nothing.

use std::sync::{Arc, Mutex};

use nsh_base::{button, link, toggle};
use nsh_core::{
    EventKind, MountBatch, MountItem, NodeId, Size, element::Root, platform::Os, prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

/// Every batch's items.
type Seen = Arc<Mutex<Vec<MountItem>>>;

fn start<C: Component<Props = ()>>() -> (Headless, Seen) {
    let mut config = EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        ..EngineConfig::new(Root::of::<C>())
    };
    // A desktop: the mouse hovers.
    config.environment.os = Os::MacOs;
    let mut app = Headless::start(config).unwrap();
    let seen = Seen::default();
    let log = seen.clone();
    app.observe(move |_: &str, batch: &MountBatch| {
        log.lock().unwrap().extend(batch.items.iter().cloned());
    });
    app.first_render().unwrap();
    (app, seen)
}

/// Whether the platforms were last told to report `kind` on view `id`.
fn listens(seen: &Seen, id: NodeId, kind: EventKind) -> bool {
    seen.lock()
        .unwrap()
        .iter()
        .filter_map(|item| match item {
            MountItem::Events { id: of, events } if *of == id => Some(events.contains(kind)),
            _ => None,
        })
        .next_back()
        .unwrap_or(false)
}

/// A component whose root hears the mouse itself.
#[component]
fn Tile() -> impl IntoElement {
    let own = signal(0u32);
    view()
        .on_mouse_enter(move |_: &MouseEvent| own.update(|n| *n += 1))
        .child(text(move || format!("tile own:{}", own.get())))
}

/// A button, a toggle and a link reporting the mouse, a tile reporting it
/// beside its own listener, and a button given none; the text logs what
/// the app heard, in order.
#[component]
fn Hovered() -> impl IntoElement {
    let log = signal(Vec::<String>::new());
    let say = move |s: String| log.update(|l| l.push(s));
    view()
        .child(
            button()
                .on_mouse_enter(move |e: &MouseEvent| {
                    say(format!(
                        "button enter {},{}",
                        e.position.y.0, e.page_position.y.0
                    ))
                })
                .on_mouse_move(move |e: &MouseEvent| say(format!("button move {}", e.position.y.0)))
                .on_mouse_leave(move |_: &MouseEvent| say("button leave".into()))
                .child(text("Button")),
        )
        .child(
            toggle()
                .on_mouse_enter(move |_: &MouseEvent| say("toggle enter".into()))
                .on_mouse_leave(move |_: &MouseEvent| say("toggle leave".into()))
                .child(text("Toggle")),
        )
        .child(
            link()
                .href("/help")
                .on_mouse_enter(move |_: &MouseEvent| say("link enter".into()))
                .child(text("Link")),
        )
        .child(tile().on_mouse_enter(move |_: &MouseEvent| say("tile enter".into())))
        .child(button().child(text("Quiet")))
        .child(text(move || log.with(|l| format!("log:{}", l.join(";")))))
}

fn target(app: &Headless, selector: &str) -> NodeId {
    app.screen()
        .press_target(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no pressable {selector}"))
}

#[test]
fn components_report_the_mouse_their_view_listens_to() {
    let (mut app, _) = start::<Hovered>();
    app.hover("text:Button").unwrap();
    app.wait_for("text:log:button enter 8.5,8.5;button move 8.5")
        .unwrap();
    app.hover("text:Toggle").unwrap();
    app.wait_for("text:log:button enter 8.5,8.5;button move 8.5;button leave;toggle enter")
        .unwrap();
    app.hover("text:Link").unwrap();
    app.wait_for(
        "text:log:button enter 8.5,8.5;button move 8.5;button leave;toggle enter;toggle leave;link enter",
    )
    .unwrap();
    // The component's own listener, then the app's.
    app.hover("text:tile own:0").unwrap();
    app.wait_for("text:tile own:1").unwrap();
    app.wait_for(
        "text:log:button enter 8.5,8.5;button move 8.5;button leave;toggle enter;toggle leave;link enter;tile enter",
    )
    .unwrap();
}

#[test]
fn a_component_given_no_mouse_listener_listens_to_none() {
    let (app, seen) = start::<Hovered>();
    let quiet = target(&app, "text:Quiet");
    let listened = target(&app, "text:Button");
    for kind in [
        EventKind::MouseEnter,
        EventKind::MouseMove,
        EventKind::MouseLeave,
    ] {
        assert!(!listens(&seen, quiet, kind), "{kind:?}");
        assert!(listens(&seen, listened, kind), "{kind:?}");
    }
}
