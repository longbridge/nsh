//! Intrinsic extent is independent of the viewport, and geometry work
//! stops after layout settles and when the view is removed.

use std::sync::Mutex;

use nsh_base::rich_editor::view::{RichEditorClasses, RichEditorGeometry};
use nsh_core::element::{EngineContext, engine_context};

use super::*;

struct Probe {
    editor: EditorSession,
    events: Mutex<Vec<RichEditorGeometry>>,
    caret: Mutex<Option<Bounds<Points>>>,
    tail: Mutex<Option<Bounds<Points>>>,
    local: Mutex<(Option<Bounds<Points>>, Option<Bounds<Points>>)>,
}

#[component]
fn GeometryProbe() -> impl IntoElement {
    let probe = engine_context::<Arc<Probe>>().unwrap().as_ref().clone();
    let shown = signal(true);
    let tall = signal(false);
    let big = signal(false);
    let reports = signal(0_u64);
    let moved = signal(false);
    let close = probe.editor.clone();
    let placement = probe.editor.clone();
    view()
        .size_full()
        .child(
            view()
                .test_id("tall")
                .on_press(move || tall.set(true))
                .child(text("Tall viewport")),
        )
        .child(
            view()
                .test_id("large-font")
                .on_press(move || big.set(true))
                .child(text("Large font")),
        )
        .child(
            view()
                .test_id("close")
                .on_press(move || {
                    // Queue a document update and remove its view in the same
                    // turn: pending geometry notifications belong to that view.
                    let state = close.state();
                    let mut tr = state.tr();
                    tr.insert_text("changed", Some(1), Some(1));
                    close.dispatch(tr).unwrap();
                    shown.set(false);
                })
                .child(text("Close")),
        )
        .child(
            view()
                .test_id("move-parent")
                .on_press(move || {
                    moved.set(true);
                    placement.refresh_geometry();
                })
                .child(text("Move enclosing layout")),
        )
        .child(
            view()
                .flex_shrink_0()
                .h(move || px(if moved.get() { 75.0 } else { 0.0 })),
        )
        .child(when(
            move || shown.get(),
            move || {
                let events = probe.clone();
                view()
                    .test_id("geometry-editor")
                    .flex_shrink_0()
                    .h(move || px(if tall.get() { 100_000.0 } else { 120.0 }))
                    .child(
                        rich_editor()
                            .editor(probe.editor.clone())
                            .size_full()
                            .classes(move || {
                                let mut classes = RichEditorClasses::default();
                                classes.content = Some("p-4".into());
                                classes.nodes = Some(
                                    [(
                                        "paragraph".into(),
                                        if big.get() {
                                            "text-[24px]"
                                        } else {
                                            "text-[16px]"
                                        }
                                        .into(),
                                    )]
                                    .into(),
                                );
                                classes
                            })
                            .on_geometry_change(move |event: &RichEditorGeometry| {
                                *events.caret.lock().unwrap() = events.editor.page_coords_at_pos(1);
                                let state = events.editor.state();
                                *events.local.lock().unwrap() = (
                                    events.editor.coords_at_pos(1),
                                    events
                                        .editor
                                        .coords_at_pos(state.doc().content().size() - 1),
                                );
                                let last = state.doc().last_child().unwrap();
                                *events.tail.lock().unwrap() = events.editor.page_node_rect(
                                    state.doc().content().size() - last.node_size(),
                                );
                                events.events.lock().unwrap().push(*event);
                                // A UI witness wakes Headless::wait_until even
                                // when the event itself changes no native frame.
                                reports.update(|count| *count += 1);
                            }),
                    )
            },
        ))
        .child(text(move || format!("Geometry reports {}", reports.get())))
}

#[test]
fn placement_refresh_updates_page_coordinates_without_changing_local_coordinates() {
    let (mut app, probe) = start_probe();
    let before = probe.caret.lock().unwrap().unwrap();
    let local = *probe.local.lock().unwrap();
    let extent = *probe.events.lock().unwrap().last().unwrap();
    let revision = probe.editor.state().revision();
    app.press("id:move-parent").unwrap();
    app.wait_until(
        "page geometry follows enclosing layout",
        Duration::from_secs(5),
        |_| {
            probe
                .caret
                .lock()
                .unwrap()
                .is_some_and(|caret| (caret.origin.y.0 - before.origin.y.0 - 75.0).abs() < 1.0)
        },
    )
    .unwrap();
    assert_eq!(*probe.local.lock().unwrap(), local);
    assert_eq!(
        probe
            .events
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .content_size
            .height,
        extent.content_size.height
    );
    assert!(probe.events.lock().unwrap().last().unwrap().generation > extent.generation);
    assert_eq!(
        probe.editor.state().revision(),
        revision,
        "placement refresh is not an editor transaction"
    );
}

fn start_probe() -> (Headless, Arc<Probe>) {
    let content: Vec<Value> = (0..80).map(|i| json!({"type":"paragraph","content":[{"type":"text","text":format!("Paragraph {i}: {}", "wrapping text ".repeat(8))}]})).collect();
    let probe = Arc::new(Probe {
        editor: session(json!({"type":"doc","content":content})),
        events: Mutex::new(Vec::new()),
        caret: Mutex::new(None),
        tail: Mutex::new(None),
        local: Mutex::new((None, None)),
    });
    let mut app = Headless::start(EngineConfig {
        viewport: Size::new(px(600.0), px(500.0)),
        context: EngineContext::default().with(probe.clone()),
        ..EngineConfig::new(Root::of::<GeometryProbe>())
    })
    .unwrap();
    app.wait_until("full offscreen extent and block geometry", Duration::from_secs(5), |_| probe.events.lock().unwrap().last().is_some_and(|event| event.content_size.height.0 > 1_000.0) && probe.caret.lock().unwrap().is_some() && probe.tail.lock().unwrap().is_some()).unwrap_or_else(|error| {
        let layout = app.screen().layout_nodes().into_iter().filter(|node| node.test_id.is_some() || node.scroll.is_some() || node.text.as_ref().is_some_and(|text| text.text.text.starts_with("Paragraph 0:") || text.text.text.starts_with("Paragraph 79:"))).map(|node| (node.id, node.test_id, node.bounds, node.scroll)).collect::<Vec<_>>();
        panic!("{error}; geometry_events={:?}; caret={:?}; tail={:?}; local_first_and_last_caret={:?}; selected_layout={layout:?}", probe.events.lock().unwrap(), probe.caret.lock().unwrap(), probe.tail.lock().unwrap(), probe.local.lock().unwrap());
    });
    (app, probe)
}

#[test]
fn geometry_exposes_offscreen_content_and_shrinks_an_overestimated_viewport() {
    let (mut app, probe) = start_probe();
    let height = probe
        .events
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .content_size
        .height
        .0;
    assert!(
        height > 120.0,
        "the complete document is measured, not the visible part"
    );
    // The wrapper's origin is included in coordinate queries even when
    // Part::Content has padding.
    let page = probe.caret.lock().unwrap().unwrap();
    let tail = probe.tail.lock().unwrap().unwrap();
    assert!(
        tail.origin.y.0 - page.origin.y.0 > 120.0,
        "the final block lies beyond the initial viewport"
    );
    assert!(
        tail.origin.y.0 - page.origin.y.0 + tail.size.height.0 <= height + 1.0,
        "the complete final block fits the intrinsic extent"
    );
    let first = app
        .screen()
        .layout_nodes()
        .into_iter()
        .find(|node| {
            node.text
                .as_ref()
                .is_some_and(|text| text.text.text.starts_with("Paragraph 0:"))
        })
        .unwrap();
    assert!((page.origin.x.0 - first.bounds.origin.x.0).abs() < 1.0);
    assert!((page.origin.y.0 - first.bounds.origin.y.0).abs() < 1.0);
    app.press("id:tall").unwrap();
    app.wait_until(
        "overestimated viewport laid out",
        Duration::from_secs(5),
        |screen| {
            screen.layout_nodes().into_iter().any(|node| {
                node.test_id.as_deref() == Some("geometry-editor")
                    && node.bounds.size.height.0 > 90_000.0
            })
        },
    )
    .unwrap();
    let measured = probe
        .events
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .content_size
        .height
        .0;
    assert!(
        (measured - height).abs() < 1.0,
        "intrinsic {measured}, previous {height}; viewport minimum must not become document height"
    );
}

#[test]
fn geometry_updates_for_width_font_and_scroll_then_stops_at_idle_and_unmount() {
    let (mut app, probe) = start_probe();
    let initial = *probe.events.lock().unwrap().last().unwrap();
    let selected = (
        probe.editor.state().selection().anchor(),
        probe.editor.state().selection().head(),
    );
    app.set_viewport(Size::new(px(300.0), px(500.0))).unwrap();
    app.wait_until(
        "wrapped extent at new width",
        Duration::from_secs(5),
        |_| {
            probe.events.lock().unwrap().last().is_some_and(|event| {
                event.content_size.width < initial.content_size.width
                    && event.content_size.height > initial.content_size.height
            })
        },
    )
    .unwrap();
    let narrow = *probe.events.lock().unwrap().last().unwrap();
    app.press("id:large-font").unwrap();
    app.wait_until("font-only extent change", Duration::from_secs(5), |_| {
        probe
            .events
            .lock()
            .unwrap()
            .last()
            .is_some_and(|event| event.content_size.height > narrow.content_size.height)
    })
    .unwrap();
    let larger = *probe.events.lock().unwrap().last().unwrap();
    app.set_environment("system text scale", |environment| {
        environment.font_scale = 1.5
    })
    .unwrap();
    app.wait_until(
        "system font scale extent change",
        Duration::from_secs(5),
        |_| {
            probe
                .events
                .lock()
                .unwrap()
                .last()
                .is_some_and(|event| event.content_size.height > larger.content_size.height)
        },
    )
    .unwrap();
    assert_eq!(
        probe
            .events
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .document_revision,
        initial.document_revision,
        "typography need not edit the document"
    );
    app.scroll("id:geometry-editor", px(200.0)).unwrap();
    app.wait_until(
        "internal scroll notification",
        Duration::from_secs(5),
        |_| {
            probe
                .events
                .lock()
                .unwrap()
                .last()
                .is_some_and(|event| event.scroll_offset.0 > 0.0)
        },
    )
    .unwrap();
    app.wait(Duration::from_millis(100)).unwrap();
    let settled = probe.events.lock().unwrap().len();
    app.wait(Duration::from_secs(1)).unwrap();
    assert_eq!(
        probe.events.lock().unwrap().len(),
        settled,
        "no periodic geometry work at idle"
    );
    assert_eq!(
        (
            probe.editor.state().selection().anchor(),
            probe.editor.state().selection().head()
        ),
        selected,
        "layout and scroll retain the text selection"
    );
    app.press("id:close").unwrap();
    let removed = probe.events.lock().unwrap().len();
    app.wait(Duration::from_secs(1)).unwrap();
    assert_eq!(
        probe.events.lock().unwrap().len(),
        removed,
        "unmount cancels pending geometry work"
    );
}
