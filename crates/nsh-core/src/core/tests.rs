use std::cell::Cell;

use nsh_animation::{AnimationOp, AnimationProp};

use super::*;
use crate::{Color, EventKind, StyleValue};

/// 8pt per char, 16pt per line, wraps at `max_width`. Counts calls.
#[derive(Default)]
struct FakeEngine {
    measures: Cell<u32>,
    layouts: Cell<u32>,
}

impl TextEngine for FakeEngine {
    /// Characters and attachment offsets.
    type Prepared = (usize, Vec<u32>);

    fn prepare(&self, text: &AttributedText) -> (usize, Vec<u32>) {
        (
            text.text.chars().count(),
            text.attachments.iter().map(|a| a.index).collect(),
        )
    }

    fn measure(&self, &(chars, _): &(usize, Vec<u32>), max_width: Option<f32>) -> Size {
        self.measures.set(self.measures.get() + 1);
        let w = chars as f32 * 8.0;
        match max_width {
            Some(max) if w > max && max > 0.0 => Size::new(max, (w / max).ceil() * 16.0),
            _ => Size::new(w, 16.0),
        }
    }

    fn layout(&self, (_, attachments): &(usize, Vec<u32>), _: Size) -> TextLayout {
        self.layouts.set(self.layouts.get() + 1);
        // One line; the third attachment on is cut off.
        let points: Vec<Option<(f32, f32)>> = attachments
            .iter()
            .enumerate()
            .map(|(i, &at)| (i < 2).then_some((at as f32 * 8.0, 2.0)))
            .collect();
        TextLayout(Arc::new(points))
    }

    fn attachments(&self, layout: &TextLayout) -> Vec<Option<(f32, f32)>> {
        layout
            .downcast::<Vec<Option<(f32, f32)>>>()
            .cloned()
            .unwrap_or_default()
    }
}

fn core() -> Core<FakeEngine> {
    Core::new(FakeEngine::default(), Size::new(320.0, 480.0))
}

fn style(props: &[(&str, StyleValue<'_>)]) -> Box<Style> {
    let mut s = Style::default();
    for &(k, v) in props {
        s.set(k, v).unwrap();
    }
    Box::new(s)
}

fn style_op(id: NodeId, props: &[(&str, StyleValue<'_>)]) -> Op {
    let mut style = Box::<Style>::default();
    let mut patch = StylePatch::default();
    for &(k, v) in props {
        patch.set(&mut style, k, v).unwrap();
    }
    Op::SetStyle { id, style, patch }
}

fn apply(core: &mut Core<FakeEngine>, ops: Vec<Op>) {
    for op in ops {
        core.apply(op).unwrap();
    }
}

fn frame_of(batch: &MountBatch, id: NodeId) -> Option<Rect> {
    batch.items.iter().rev().find_map(|i| match i {
        MountItem::Frame { id: i, frame } if *i == id => Some(*frame),
        _ => None,
    })
}

#[test]
fn mounts_subtree_built_detached() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Create {
                id: 3,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 4,
                text: "hello".into(),
            },
            Op::Insert {
                parent: 3,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            style_op(2, &[("padding", StyleValue::Number(10.0))]),
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let creates: Vec<_> = batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Create { id, kind } => Some((*id, *kind)),
            _ => None,
        })
        .collect();
    // Raw text never becomes a native view.
    assert_eq!(creates, vec![(2, ViewKind::View), (3, ViewKind::Text)]);
    let text = batch.items.iter().find_map(|i| match i {
        MountItem::Text { id: 3, text, .. } => Some(text.clone()),
        _ => None,
    });
    assert_eq!(text.unwrap().text, "hello");
    assert_eq!(
        frame_of(&batch, 2),
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 36.0
        })
    );
    assert_eq!(
        frame_of(&batch, 3),
        Some(Rect {
            x: 10.0,
            y: 10.0,
            width: 300.0,
            height: 16.0
        })
    );
    assert!(c.commit().is_none(), "second commit is a no-op");
}

#[test]
fn text_update_emits_only_text_and_frames() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 3,
                text: "a".into(),
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit().unwrap();
    c.apply(Op::SetText {
        id: 3,
        text: "abc".into(),
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(matches!(&batch.items[0], MountItem::Text { id: 2, text, .. } if text.text == "abc"));
    // Text blocks stretch to the column width, so only the content changes.
    assert_eq!(batch.items.len(), 1);
}

#[test]
fn flex_text_in_a_row_shrinks_and_wraps() {
    // Yoga has no automatic minimum size: `flex: 1` text takes the space
    // left in the row and wraps, however long its natural width.
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            style_op(2, &[("flexDirection", StyleValue::Str("row"))]),
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            style_op(
                3,
                &[
                    ("width", StyleValue::Number(120.0)),
                    ("height", StyleValue::Number(10.0)),
                ],
            ),
            Op::Create {
                id: 4,
                tag: Tag::Text,
            },
            style_op(4, &[("flex", StyleValue::Number(1.0))]),
            Op::CreateText {
                id: 5,
                text: "x".repeat(50),
            },
            Op::Insert {
                parent: 4,
                child: 5,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    // 400pt of text in 200pt: two lines.
    assert_eq!(
        frame_of(&batch, 4),
        Some(Rect {
            x: 120.0,
            y: 0.0,
            width: 200.0,
            height: 32.0
        })
    );
}

#[test]
fn nested_spans_merge_with_styles() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 3,
                text: "Count: ".into(),
            },
            Op::Create {
                id: 4,
                tag: Tag::Text,
            },
            style_op(4, &[("fontWeight", StyleValue::Str("bold"))]),
            Op::CreateText {
                id: 5,
                text: "1".into(),
            },
            Op::Insert {
                parent: 4,
                child: 5,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let text = batch
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.text, "Count: 1");
    assert_eq!(text.spans.len(), 2);
    assert_eq!(text.spans[1].style.font_weight, 700);
    // The span has no native view.
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Create { id: 4, .. }))
    );
}

#[test]
fn placeholder_text_outside_text_is_ignored() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            Op::CreateText {
                id: 3,
                text: String::new(),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 3,
                before: Some(2),
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 4,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let inserts: Vec<_> = batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Insert { child, before, .. } => Some((*child, *before)),
            _ => None,
        })
        .collect();
    assert_eq!(inserts, vec![(2, None), (4, None)]);
}

#[test]
fn remove_reinsert_and_release() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 3,
                before: None,
            },
        ],
    );
    c.commit().unwrap();

    // Move 3 before 2: a single insert, no remove.
    c.apply(Op::Insert {
        parent: ROOT_ID,
        child: 3,
        before: Some(2),
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(matches!(
        batch.items[0],
        MountItem::Insert {
            child: 3,
            before: Some(2),
            ..
        }
    ));
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Remove { .. }))
    );

    c.apply(Op::Remove {
        parent: ROOT_ID,
        child: 2,
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(matches!(batch.items[0], MountItem::Remove { id: 2 }));

    // Reinserting a detached, already-created view does not recreate it.
    c.apply(Op::Insert {
        parent: ROOT_ID,
        child: 2,
        before: None,
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Create { .. }))
    );

    c.apply(Op::Release { id: 2 }).unwrap();
    let batch = c.commit().unwrap();
    assert!(
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Delete { id: 2 }))
    );
    assert_eq!(c.node_count(), 2);
    assert_eq!(
        c.apply(Op::SetText {
            id: 2,
            text: String::new()
        }),
        Err(ApplyError::UnknownNode(2))
    );
}

#[test]
fn events_and_errors() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            Op::Listen {
                id: 2,
                event: EventKind::Press,
                enabled: true,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(
        |i| matches!(i, MountItem::Events { id: 2, events } if events.contains(EventKind::Press))
    ));
    assert_eq!(
        c.apply(Op::Create {
            id: 2,
            tag: Tag::View
        }),
        Err(ApplyError::DuplicateNode(2))
    );
    assert_eq!(
        c.apply(Op::Insert {
            parent: 2,
            child: ROOT_ID,
            before: None
        }),
        Err(ApplyError::Cycle(1))
    );
}

#[test]
fn color_change_skips_layout_and_identical_texts_share_measurements() {
    let mut c = core();
    let mut ops = Vec::new();
    for i in 0..3u32 {
        let (block, raw) = (10 + i * 2, 11 + i * 2);
        ops.extend([
            Op::Create {
                id: block,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: raw,
                text: "same".into(),
            },
            Op::Insert {
                parent: block,
                child: raw,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: block,
                before: None,
            },
        ]);
    }
    apply(&mut c, ops);
    let batch = c.commit().unwrap();
    // Three identical texts: measured once per distinct constraint.
    let first_measures = c.text_engine.measures.get();
    assert!(first_measures <= 2, "measured {first_measures} times");
    assert_eq!(c.text_engine.layouts.get(), 3);
    let content = batch.items.iter().find_map(|i| match i {
        MountItem::Text {
            id: 10, content, ..
        } => Some(*content),
        _ => None,
    });
    assert_eq!(
        content,
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 16.0
        })
    );

    c.apply(style_op(10, &[("color", StyleValue::Str("red"))]))
        .unwrap();
    let batch = c.commit().unwrap();
    assert_eq!(
        batch.items.len(),
        1,
        "only the new text layout: {:?}",
        batch.items
    );
    assert!(matches!(&batch.items[0], MountItem::Text { id: 10, .. }));
    assert_eq!(
        c.text_engine.measures.get(),
        first_measures,
        "no re-measure"
    );
    assert_eq!(c.text_engine.layouts.get(), 4);
}

#[test]
fn image_uses_natural_size_and_aspect_ratio() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Image,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Source(Some(ImageSource {
                    uri: "asset:/logo.png".into(),
                    size: Some((200.0, 100.0)),
                })),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            Op::Create {
                id: 3,
                tag: Tag::Image,
            },
            style_op(
                3,
                &[
                    ("width", StyleValue::Number(50.0)),
                    ("alignSelf", StyleValue::Str("flex-start")),
                ],
            ),
            Op::SetProp {
                id: 3,
                prop: Prop::Source(Some(ImageSource {
                    uri: "asset:/logo.png".into(),
                    size: Some((200.0, 100.0)),
                })),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 3,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::Create {
            id: 2,
            kind: ViewKind::Image
        }
    )));
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::Image {
            id: 2,
            source: Some(_),
            ..
        }
    )));
    // Stretched to the column width, height from the aspect ratio.
    assert_eq!(
        frame_of(&batch, 2).map(|f| (f.width, f.height)),
        Some((320.0, 160.0))
    );
    assert_eq!(
        frame_of(&batch, 3).map(|f| (f.width, f.height)),
        Some((50.0, 25.0))
    );
}

#[test]
fn native_components_get_their_name_props_and_size() {
    let mut c = core();
    let prop = |name: &str, value: Option<crate::JsonValue>| Op::SetProp {
        id: 2,
        prop: Prop::ComponentProp(name.into(), value),
    };
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Native,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Component("com.example.map".into()),
            },
            prop("zoom", Some(serde_json::json!(3))),
            prop("center", Some(serde_json::json!({ "lat": 1.5, "lng": 2 }))),
            style_op(2, &[("alignSelf", StyleValue::Str("flex-start"))]),
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let at = |pred: &dyn Fn(&MountItem) -> bool| batch.items.iter().position(pred);
    let create = at(&|i| {
        matches!(
            i,
            MountItem::Create {
                id: 2,
                kind: ViewKind::Native
            }
        )
    })
    .unwrap();
    let component =
        at(&|i| matches!(i, MountItem::Component { id: 2, name } if &**name == "com.example.map"))
            .unwrap();
    let props = batch
        .items
        .iter()
        .position(|i| matches!(i, MountItem::ComponentProps { id: 2, .. }))
        .unwrap();
    assert!(create < component && component < props);
    let MountItem::ComponentProps { props: all, .. } = &batch.items[props] else {
        unreachable!()
    };
    assert_eq!(all.len(), 2);
    assert!(all.contains(&(
        "center".into(),
        Some(serde_json::json!({ "lat": 1.5, "lng": 2 }))
    )));
    // No size of its own yet.
    assert_eq!(
        frame_of(&batch, 2).map(|f| (f.width, f.height)),
        Some((0.0, 0.0))
    );

    // Only what changed; removals as `None`.
    apply(
        &mut c,
        vec![
            prop("zoom", Some(serde_json::json!(4))),
            prop("center", None),
        ],
    );
    let batch = c.commit().unwrap();
    let changed: Vec<_> = batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::ComponentProps { id: 2, props } => Some(props.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        changed,
        [vec![
            ("zoom".into(), Some(serde_json::json!(4))),
            ("center".into(), None)
        ]]
    );

    // The size it reports lays it out.
    c.set_component_size(2, Size::new(120.0, 40.0));
    let batch = c.commit().unwrap();
    assert_eq!(
        frame_of(&batch, 2).map(|f| (f.width, f.height)),
        Some((120.0, 40.0))
    );
}

#[test]
fn scroll_reports_content_size() {
    let mut c = core();
    let mut ops = vec![
        Op::Create {
            id: 2,
            tag: Tag::Scroll,
        },
        style_op(2, &[("height", StyleValue::Number(200.0))]),
        Op::Insert {
            parent: ROOT_ID,
            child: 2,
            before: None,
        },
    ];
    for i in 0..5u32 {
        ops.push(Op::Create {
            id: 10 + i,
            tag: Tag::View,
        });
        ops.push(style_op(10 + i, &[("height", StyleValue::Number(100.0))]));
        ops.push(Op::Insert {
            parent: 2,
            child: 10 + i,
            before: None,
        });
    }
    apply(&mut c, ops);
    let batch = c.commit().unwrap();
    let content = batch.items.iter().find_map(|i| match i {
        MountItem::ScrollContent { id: 2, size } => Some(*size),
        _ => None,
    });
    assert_eq!(content, Some(Size::new(320.0, 500.0)));
    // Children are not shrunk to the scroll view's height.
    assert_eq!(
        frame_of(&batch, 14).map(|f| (f.y, f.height)),
        Some((400.0, 100.0))
    );
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::ScrollConfig {
            id: 2,
            horizontal: false,
            ..
        }
    )));
}

/// A scroll view's content size set twice in one batch (a row relaid in
/// two passes) reaches the platform once, with the last size: a transient
/// smaller size would clamp the offset before the content grows back.
#[test]
fn scroll_content_is_sent_once_per_batch() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Scroll,
            },
            style_op(2, &[("height", StyleValue::Number(200.0))]),
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit();
    let key = c.key(2).unwrap();
    c.set_scroll_content(key, Size::new(320.0, 277.0));
    c.set_scroll_content(key, Size::new(320.0, 909.0));
    let sizes: Vec<_> = c
        .mount
        .iter()
        .filter_map(|i| match i {
            MountItem::ScrollContent { id: 2, size } => Some(size.height),
            _ => None,
        })
        .collect();
    assert_eq!(sizes, [909.0]);
    // The next batch sends its own.
    c.mount.clear();
    c.batches += 1;
    c.set_scroll_content(key, Size::new(320.0, 300.0));
    assert!(matches!(
        c.mount[..],
        [MountItem::ScrollContent { id: 2, .. }]
    ));
}

/// Plays JS: creates a cell per new slot, renders a fixed-height
/// row for each bind and confirms the index.
fn render_binds(
    c: &mut Core<FakeEngine>,
    next_id: &mut u32,
    cells: &mut Vec<u32>,
    row_height: f32,
) -> usize {
    let mut binds = Vec::new();
    c.take_list_binds(&mut binds);
    let mut count = 0;
    for bind in binds {
        for (slot, index) in bind.binds {
            count += 1;
            while cells.len() <= slot as usize {
                let id = *next_id;
                *next_id += 1;
                apply(
                    c,
                    vec![
                        Op::Create { id, tag: Tag::Cell },
                        Op::SetProp {
                            id,
                            prop: Prop::Slot(CellSlot::Item(cells.len() as u32)),
                        },
                        Op::Insert {
                            parent: bind.list,
                            child: id,
                            before: None,
                        },
                    ],
                );
                cells.push(id);
            }
            // New content for the item, then the confirmation, as the
            // runtime does.
            let cell = cells[slot as usize];
            c.apply(style_op(
                cell,
                &[("height", StyleValue::Number(row_height))],
            ))
            .unwrap();
            c.apply(Op::SetProp {
                id: cell,
                prop: Prop::CellIndex(index),
            })
            .unwrap();
        }
    }
    count
}

#[test]
fn list_renders_window_and_recycles_cells() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::List,
            },
            style_op(2, &[("height", StyleValue::Number(400.0))]),
            Op::SetProp {
                id: 2,
                prop: Prop::EstimatedItemSize(50.0),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Count(1000),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit();
    let (mut next_id, mut cells) = (100, Vec::new());
    // Estimated 50 pt rows: viewport + one screen of overscan = 17 rows.
    assert_eq!(render_binds(&mut c, &mut next_id, &mut cells, 100.0), 17);
    let batch = c.commit().unwrap();
    // Rows are really 100 pt: positioned by measured sizes.
    assert_eq!(
        frame_of(&batch, cells[3]),
        Some(Rect {
            x: 0.0,
            y: 300.0,
            width: 320.0,
            height: 100.0
        })
    );
    let content = batch.items.iter().rev().find_map(|i| match i {
        MountItem::ScrollContent { id: 2, size } => Some(size.height),
        _ => None,
    });
    assert_eq!(content, Some(17.0 * 100.0 + 983.0 * 50.0));

    // Scroll far down into unmeasured (50 pt estimated) rows: the window
    // needs 25 rows; all 17 existing cells are reused and 8 are added.
    c.set_scroll_offset(2, 0.0, 10_000.0, u32::MAX);
    c.commit();
    assert_eq!(render_binds(&mut c, &mut next_id, &mut cells, 50.0), 25);
    assert_eq!(cells.len(), 25);
    c.commit();
    // Scrolling on reuses cells only.
    c.set_scroll_offset(2, 0.0, 20_000.0, u32::MAX);
    c.commit();
    assert_eq!(render_binds(&mut c, &mut next_id, &mut cells, 50.0), 25);
    assert_eq!(cells.len(), 25);
    c.commit();
    assert_eq!(c.node_count(), 2 + cells.len());
}

#[test]
fn list_keeps_visible_content_anchored() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::List,
            },
            style_op(2, &[("height", StyleValue::Number(400.0))]),
            Op::SetProp {
                id: 2,
                prop: Prop::EstimatedItemSize(50.0),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Count(100),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit();
    // Jump into the middle before anything above was measured.
    c.set_scroll_offset(2, 0.0, 2000.0, u32::MAX);
    c.commit();
    let (mut next_id, mut cells) = (100, Vec::new());
    render_binds(&mut c, &mut next_id, &mut cells, 50.0);
    c.commit();
    // Scroll back up so rows above the old position get rendered at 80 pt.
    c.set_scroll_offset(2, 0.0, 1200.0, u32::MAX);
    c.commit();
    render_binds(&mut c, &mut next_id, &mut cells, 80.0);
    let batch = c.commit().unwrap();
    let shift: f32 = batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::ScrollBy { id: 2, dy, .. } => Some(*dy),
            _ => None,
        })
        .sum();
    assert!(
        shift > 0.0,
        "rows above the viewport grew; content must be shifted down"
    );
}

/// A list of 100 rows estimated at 50 pt in a 400 pt viewport.
fn estimated_list(c: &mut Core<FakeEngine>) {
    apply(
        c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::List,
            },
            style_op(2, &[("height", StyleValue::Number(400.0))]),
            Op::SetProp {
                id: 2,
                prop: Prop::EstimatedItemSize(50.0),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Count(100),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
}

/// Where list 2 is after `batch`: from `start`, its `ScrollTo` commands
/// and `ScrollBy` items in order.
fn scrolled(batch: &MountBatch, start: f32) -> f32 {
    batch.items.iter().fold(start, |at, item| match item {
        MountItem::Command {
            id: 2,
            command: Command::ScrollTo { y, .. },
        } => *y,
        MountItem::ScrollBy { id: 2, dy, .. } => at + dy,
        _ => at,
    })
}

#[test]
fn scroll_commands_follow_their_target_as_rows_are_measured() {
    let mut c = core();
    estimated_list(&mut c);
    c.commit();
    let (mut next_id, mut cells) = (100, Vec::new());
    render_binds(&mut c, &mut next_id, &mut cells, 50.0);
    c.commit();
    // The rows are really 80 pt: the end of the estimated content is too
    // early, and the list follows the end once the last rows measure.
    c.apply(Op::Command {
        id: 2,
        command: Command::ScrollToEnd { animated: false },
    })
    .unwrap();
    let mut at = scrolled(&c.commit().unwrap(), 0.0);
    for _ in 0..4 {
        render_binds(&mut c, &mut next_id, &mut cells, 80.0);
        if let Some(batch) = c.commit() {
            at = scrolled(&batch, at);
        }
    }
    let key = c.key(2).unwrap();
    let content = c.nodes[key as usize]
        .scroll
        .as_ref()
        .unwrap()
        .content
        .unwrap();
    assert_eq!(at, content.height - 400.0);

    // Item 50 in the middle of the viewport.
    c.apply(Op::Command {
        id: 2,
        command: Command::ScrollToIndex {
            index: 50,
            animated: false,
            view_position: 0.5,
            view_offset: 0.0,
        },
    })
    .unwrap();
    at = scrolled(&c.commit().unwrap(), at);
    for _ in 0..4 {
        render_binds(&mut c, &mut next_id, &mut cells, 80.0);
        if let Some(batch) = c.commit() {
            at = scrolled(&batch, at);
        }
    }
    let list = c.nodes[key as usize].list.as_ref().unwrap();
    assert_eq!(list.item_at(50) + 40.0 - at, 200.0);
    assert!(!list.pinned(), "settled");

    // Scrolled by the user, the list stays where it is put.
    c.set_scroll_offset(2, 0.0, at - 600.0, u32::MAX);
    c.commit();
    assert!(render_binds(&mut c, &mut next_id, &mut cells, 120.0) > 0);
    let batch = c.commit().unwrap();
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Command { id: 2, .. })),
        "no scroll command after the user scrolled"
    );
}

#[test]
fn animated_scroll_commands_retarget_while_rows_are_measured() {
    let mut c = core();
    estimated_list(&mut c);
    c.commit();
    let (mut next_id, mut cells) = (100, Vec::new());
    render_binds(&mut c, &mut next_id, &mut cells, 80.0);
    c.commit();
    c.apply(Op::Command {
        id: 2,
        command: Command::ScrollToEnd { animated: true },
    })
    .unwrap();
    let first = scrolled(&c.commit().unwrap(), 0.0);
    let mut target = first;
    // The platform starts the animation; the window follows it to the
    // end, whose rows are measured: the animation is retargeted.
    c.set_scroll_offset(2, 0.0, 1000.0, 1);
    c.commit();
    c.set_scroll_offset(2, 0.0, first - 10.0, 1);
    c.commit();
    let mut issued = 1;
    for _ in 0..4 {
        render_binds(&mut c, &mut next_id, &mut cells, 80.0);
        if let Some(batch) = c.commit() {
            for item in &batch.items {
                if let MountItem::Command {
                    id: 2,
                    command: Command::ScrollTo { y, animated, .. },
                } = item
                {
                    assert!(animated);
                    target = *y;
                    issued += 1;
                }
            }
        }
    }
    assert!(issued > 1 && target > first, "retargeted");
    let key = c.key(2).unwrap();
    let content = c.nodes[key as usize]
        .scroll
        .as_ref()
        .unwrap()
        .content
        .unwrap();
    assert_eq!(target, content.height - 400.0);
}

#[test]
fn animated_background_colors_replace_gradients() {
    let mut c = core();
    let mut s = Style::default();
    s.set_background_images(vec![crate::effects::Gradient {
        kind: crate::effects::GradientKind::Linear { angle: 180.0 },
        stops: Vec::new(),
        repeating: false,
    }]);
    s.animated.push((AnimationProp::BackgroundColor, 7));
    apply(
        &mut c,
        vec![
            Op::Animation(AnimationOp::CreateValue {
                node: 7,
                value: 1.0,
            }),
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 2,
                style: Box::new(s),
                patch: StylePatch::default(),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(view_style_of(&batch, 2).is_none_or(|s| s.effects.is_none()));
}

#[test]
fn animation_ops_and_bindings_are_forwarded_in_order() {
    let mut c = core();
    let mut s = Style::default();
    s.animated.push((AnimationProp::Opacity, 7));
    apply(
        &mut c,
        vec![
            Op::Animation(AnimationOp::CreateValue {
                node: 7,
                value: 1.0,
            }),
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 2,
                style: Box::new(s),
                patch: StylePatch::default(),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let animation = batch.items.iter().position(|i| {
        matches!(
            i,
            MountItem::Animation(AnimationOp::CreateValue { node: 7, .. })
        )
    });
    let bind = batch
        .items
        .iter()
        .position(|i| matches!(i, MountItem::AnimationBindings { id: 2, .. }));
    assert!(
        animation.unwrap() < bind.unwrap(),
        "values exist before views bind them"
    );
}

#[test]
fn leaf_styles_are_resolved() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            // Text with padding: the frame wraps the measured text.
            Op::Create {
                id: 2,
                tag: Tag::Text,
            },
            style_op(2, &[("padding", StyleValue::Number(10.0))]),
            Op::CreateText {
                id: 3,
                text: "abc".into(),
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            // Empty leaf view: min size and aspect ratio still apply.
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            style_op(
                4,
                &[
                    ("width", StyleValue::Number(100.0)),
                    ("aspectRatio", StyleValue::Number(2.0)),
                    ("alignSelf", StyleValue::Str("flex-start")),
                ],
            ),
            Op::Insert {
                parent: ROOT_ID,
                child: 4,
                before: None,
            },
            Op::Create {
                id: 5,
                tag: Tag::View,
            },
            style_op(5, &[("minHeight", StyleValue::Number(80.0))]),
            Op::Insert {
                parent: ROOT_ID,
                child: 5,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 2).map(|f| f.height), Some(16.0 + 20.0));
    let content = batch.items.iter().find_map(|i| match i {
        MountItem::Text { id: 2, content, .. } => Some(*content),
        _ => None,
    });
    assert_eq!(
        content,
        Some(Rect {
            x: 10.0,
            y: 10.0,
            width: 300.0,
            height: 16.0
        })
    );
    assert_eq!(
        frame_of(&batch, 4).map(|f| (f.width, f.height)),
        Some((100.0, 50.0))
    );
    assert_eq!(frame_of(&batch, 5).map(|f| f.height), Some(80.0));
}

/// `cargo test --release -p nsh-core bench_row_relayout -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_row_relayout() {
    let mut c = core();
    // A gallery-like row: [image] [title / author] in a list cell.
    let row = |s: &[(&str, StyleValue<'_>)]| style(s);
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 2,
                style: row(&[
                    ("flexDirection", StyleValue::Str("row")),
                    ("alignItems", StyleValue::Str("center")),
                    ("gap", StyleValue::Number(12.0)),
                    ("padding", StyleValue::Number(8.0)),
                ]),
                patch: StylePatch::default(),
            },
            Op::Create {
                id: 3,
                tag: Tag::Image,
            },
            Op::SetStyle {
                id: 3,
                style: row(&[
                    ("width", StyleValue::Number(48.0)),
                    ("height", StyleValue::Number(48.0)),
                ]),
                patch: StylePatch::default(),
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 4,
                style: row(&[("flex", StyleValue::Number(1.0))]),
                patch: StylePatch::default(),
            },
            Op::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            Op::Create {
                id: 5,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 6,
                text: "Photo #1".into(),
            },
            Op::Insert {
                parent: 5,
                child: 6,
                before: None,
            },
            Op::Insert {
                parent: 4,
                child: 5,
                before: None,
            },
            Op::Create {
                id: 7,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 8,
                text: "by Ada".into(),
            },
            Op::Insert {
                parent: 7,
                child: 8,
                before: None,
            },
            Op::Insert {
                parent: 4,
                child: 7,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit();
    let n = 2000;
    let start = std::time::Instant::now();
    for i in 0..n {
        c.apply(Op::SetText {
            id: 6,
            text: format!("Photo #{i}"),
        })
        .unwrap();
        c.apply(Op::SetText {
            id: 8,
            text: format!("by {}", ["Ada", "Ken"][i % 2]),
        })
        .unwrap();
        c.commit();
    }
    eprintln!("row relayout: {:?} per commit", start.elapsed() / n as u32);
}

/// `cargo test --release -p nsh-core bench_cell_relayout -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_cell_relayout() {
    let mut c = Core::new(crate::ApproxTextEngine, Size::new(390.0, 800.0));
    let s = |p: &[(&str, StyleValue<'_>)]| style(p);
    let mut ops = vec![
        Op::Create {
            id: 2,
            tag: Tag::List,
        },
        Op::SetStyle {
            id: 2,
            style: s(&[("flex", StyleValue::Number(1.0))]),
            patch: StylePatch::default(),
        },
        Op::SetProp {
            id: 2,
            prop: Prop::EstimatedItemSize(64.0),
        },
        Op::SetProp {
            id: 2,
            prop: Prop::Count(1000),
        },
        Op::Insert {
            parent: ROOT_ID,
            child: 2,
            before: None,
        },
    ];
    for op in std::mem::take(&mut ops) {
        c.apply(op).unwrap();
    }
    c.commit();
    let mut binds = Vec::new();
    c.take_list_binds(&mut binds);
    // One cell with a gallery row.
    for op in [
        Op::Create {
            id: 10,
            tag: Tag::Cell,
        },
        Op::SetProp {
            id: 10,
            prop: Prop::Slot(CellSlot::Item(0)),
        },
        Op::Insert {
            parent: 2,
            child: 10,
            before: None,
        },
        Op::Create {
            id: 11,
            tag: Tag::View,
        },
        Op::SetStyle {
            id: 11,
            style: s(&[
                ("flexDirection", StyleValue::Str("row")),
                ("alignItems", StyleValue::Str("center")),
                ("gap", StyleValue::Number(12.0)),
                ("paddingHorizontal", StyleValue::Number(16.0)),
                ("paddingVertical", StyleValue::Number(8.0)),
            ]),
            patch: StylePatch::default(),
        },
        Op::Insert {
            parent: 10,
            child: 11,
            before: None,
        },
        Op::Create {
            id: 12,
            tag: Tag::Image,
        },
        Op::SetStyle {
            id: 12,
            style: s(&[
                ("width", StyleValue::Number(48.0)),
                ("height", StyleValue::Number(48.0)),
            ]),
            patch: StylePatch::default(),
        },
        Op::Insert {
            parent: 11,
            child: 12,
            before: None,
        },
        Op::Create {
            id: 13,
            tag: Tag::View,
        },
        Op::SetStyle {
            id: 13,
            style: s(&[
                ("flex", StyleValue::Number(1.0)),
                ("gap", StyleValue::Number(2.0)),
            ]),
            patch: StylePatch::default(),
        },
        Op::Insert {
            parent: 11,
            child: 13,
            before: None,
        },
        Op::Create {
            id: 14,
            tag: Tag::Text,
        },
        Op::CreateText {
            id: 15,
            text: "Photo #1".into(),
        },
        Op::Insert {
            parent: 14,
            child: 15,
            before: None,
        },
        Op::Insert {
            parent: 13,
            child: 14,
            before: None,
        },
        Op::Create {
            id: 16,
            tag: Tag::Text,
        },
        Op::CreateText {
            id: 17,
            text: "by Ada".into(),
        },
        Op::Insert {
            parent: 16,
            child: 17,
            before: None,
        },
        Op::Insert {
            parent: 13,
            child: 16,
            before: None,
        },
        Op::SetProp {
            id: 10,
            prop: Prop::CellIndex(0),
        },
    ] {
        c.apply(op).unwrap();
    }
    c.commit();
    for idle in [0u64, 10] {
        let n = if idle == 0 { 2000 } else { 100 };
        let mut total = std::time::Duration::ZERO;
        for i in 0..n {
            std::thread::sleep(std::time::Duration::from_millis(idle));
            let start = std::time::Instant::now();
            c.apply(Op::SetText {
                id: 15,
                text: format!("Photo #{i}"),
            })
            .unwrap();
            c.apply(Op::SetText {
                id: 17,
                text: format!("by {}", ["Ada", "Ken"][i % 2]),
            })
            .unwrap();
            c.commit();
            total += start.elapsed();
        }
        eprintln!(
            "cell relayout ({idle} ms idle between frames): {:?} per commit",
            total / n as u32
        );
    }
}

fn input_core() -> Core<FakeEngine> {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Input,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Placeholder("Name".into()),
            },
            style_op(2, &[("padding", StyleValue::Number(6.0))]),
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c
}

#[test]
fn input_measures_its_placeholder_and_reports_its_content_box() {
    let mut c = input_core();
    let batch = c.commit().unwrap();
    // One line of text (16pt) plus padding, stretched across the root.
    assert_eq!(frame_of(&batch, 2).map(|f| f.height), Some(28.0));
    assert!(batch.items.iter().any(|i| matches!(i,
        MountItem::Input { id: 2, config } if config.placeholder == "Name" && !config.multiline)));
    assert!(batch.items.iter().any(|i| matches!(i,
        MountItem::InputContent { id: 2, content } if *content == Rect { x: 6.0, y: 6.0, width: 308.0, height: 16.0 })));
    // No text yet: nothing to send.
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::InputValue { .. }))
    );
}

#[test]
fn inputs_resolve_what_return_does() {
    use crate::{SubmitBehavior as S, mount::InputConfig};
    let config = |multiline, blur_on_submit, submit_behavior| InputConfig {
        multiline,
        blur_on_submit,
        submit_behavior,
        ..InputConfig::default()
    };
    // React Native's defaults: one line submits and unfocuses, several
    // lines take line breaks; `blurOnSubmit={false}` keeps the focus.
    assert_eq!(config(false, true, None).submit(), S::BlurAndSubmit);
    assert_eq!(config(false, false, None).submit(), S::Submit);
    assert_eq!(config(true, true, None).submit(), S::Newline);
    // `submitBehavior` wins; one line never takes line breaks.
    assert_eq!(config(true, true, Some(S::Submit)).submit(), S::Submit);
    assert_eq!(
        config(false, false, Some(S::BlurAndSubmit)).submit(),
        S::BlurAndSubmit
    );
    assert_eq!(
        config(false, true, Some(S::Newline)).submit(),
        S::BlurAndSubmit
    );
}

#[test]
fn inputs_take_their_submit_behavior() {
    let mut c = input_core();
    c.commit().unwrap();
    apply(
        &mut c,
        vec![
            Op::SetProp {
                id: 2,
                prop: Prop::Multiline(true),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::SubmitBehavior(Some(crate::SubmitBehavior::Submit)),
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(|i| matches!(i,
        MountItem::Input { id: 2, config } if config.submit() == crate::SubmitBehavior::Submit)));
}

#[test]
fn input_values_from_js_and_native_edits() {
    let mut c = input_core();
    c.commit();
    // The user types; JS echoes the same text: nothing goes back.
    c.input_changed(2, "ab", 2);
    c.apply(Op::SetProp {
        id: 2,
        prop: Prop::Value("ab".into()),
    })
    .unwrap();
    assert!(c.commit().is_none());
    // JS transforms the text: it is sent with the edit count it is based on.
    c.apply(Op::SetProp {
        id: 2,
        prop: Prop::Value("AB".into()),
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(|i| matches!(i,
        MountItem::InputValue { id: 2, text, count: 2 } if text == "AB")));
    // `defaultValue` no longer applies once a value was set.
    c.apply(Op::SetProp {
        id: 2,
        prop: Prop::DefaultValue("x".into()),
    })
    .unwrap();
    assert!(c.commit().is_none());
}

#[test]
fn multiline_inputs_grow_with_their_text() {
    let mut c = input_core();
    c.apply(Op::SetProp {
        id: 2,
        prop: Prop::Multiline(true),
    })
    .unwrap();
    c.commit();
    // 50 characters at 8pt in 308pt: two lines.
    c.input_changed(2, &"x".repeat(50), 1);
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 2).map(|f| f.height), Some(44.0));
    // A trailing line break starts an empty line that still takes space.
    c.input_changed(2, "x\n", 2);
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 2).map(|f| f.height), Some(28.0));
}

#[test]
fn commands_reach_created_views_only() {
    let mut c = input_core();
    c.apply(Op::Command {
        id: 2,
        command: crate::Command::Focus,
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(matches!(
        batch.items.last(),
        Some(MountItem::Command {
            id: 2,
            command: crate::Command::Focus
        })
    ));
    c.apply(Op::Create {
        id: 3,
        tag: Tag::Input,
    })
    .unwrap();
    c.apply(Op::Command {
        id: 3,
        command: crate::Command::Focus,
    })
    .unwrap();
    assert!(c.commit().is_none());
}

fn insert_items(batch: &MountBatch) -> Vec<(NodeId, NodeId, Option<NodeId>)> {
    batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Insert {
                parent,
                child,
                before,
            } => Some((*parent, *child, *before)),
            _ => None,
        })
        .collect()
}

#[test]
fn z_index_orders_native_views() {
    let mut c = core();
    let z = |v: f32| style(&[("zIndex", StyleValue::Number(v))]);
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 2,
                style: z(5.0),
                patch: StylePatch::default(),
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 3,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    // 3 (z 0) goes below 2 (z 5) although it comes later in the tree.
    assert_eq!(insert_items(&batch), vec![(1, 2, None), (1, 3, Some(2))]);
    // Raising 3 above 2 moves it to the top.
    apply(
        &mut c,
        vec![Op::SetStyle {
            id: 3,
            style: z(9.0),
            patch: StylePatch::default(),
        }],
    );
    let batch = c.commit().unwrap();
    assert_eq!(insert_items(&batch), vec![(1, 3, None)]);
    // A new child with the default z index goes below both.
    apply(
        &mut c,
        vec![Op::Insert {
            parent: 1,
            child: 4,
            before: None,
        }],
    );
    let batch = c.commit().unwrap();
    assert_eq!(insert_items(&batch), vec![(1, 4, Some(2))]);
}

#[test]
fn layout_events_carry_frames_and_late_listeners_get_one() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            style_op(2, &[("height", StyleValue::Number(40.0))]),
            Op::Listen {
                id: 2,
                event: EventKind::Layout,
                enabled: true,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit().unwrap();
    let mut events = Vec::new();
    c.take_events(&mut events);
    assert_eq!(
        events,
        vec![(
            2,
            EventKind::Layout,
            EventPayload::Layout {
                x: 0.0,
                y: 0.0,
                width: 320.0,
                height: 40.0
            }
        )]
    );
    // Unchanged frames report nothing; a listener added later learns the
    // current frame.
    apply(
        &mut c,
        vec![
            Op::Listen {
                id: 2,
                event: EventKind::Layout,
                enabled: false,
            },
            Op::Listen {
                id: 2,
                event: EventKind::Layout,
                enabled: true,
            },
        ],
    );
    c.commit();
    events.clear();
    c.take_events(&mut events);
    assert_eq!(events.len(), 1);
}

#[test]
fn font_scale_scales_text_that_allows_it() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 3,
                text: "abc".into(),
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
            Op::Create {
                id: 4,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 5,
                text: "abc".into(),
            },
            Op::SetProp {
                id: 4,
                prop: Prop::AllowFontScaling(false),
            },
            Op::Insert {
                parent: 4,
                child: 5,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 4,
                before: None,
            },
        ],
    );
    c.commit().unwrap();
    c.set_font_scale(2.0);
    let batch = c.commit().unwrap();
    let size = |id| {
        batch.items.iter().find_map(|i| match i {
            MountItem::Text { id: i, text, .. } if *i == id => Some(text.spans[0].style.font_size),
            _ => None,
        })
    };
    assert_eq!(size(2), Some(28.0));
    assert_eq!(size(4), None, "text without font scaling is unchanged");
}

#[test]
fn layout_animation_leads_the_batch() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
            Op::Command {
                id: 1,
                command: Command::LayoutAnimation(crate::LayoutAnimation {
                    create: None,
                    update: None,
                    delete: None,
                }),
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(
        matches!(batch.items[0], MountItem::LayoutAnimation(_)),
        "{:?}",
        batch.items
    );
}

#[test]
fn view_props_and_gestures_reach_the_platform() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::HitSlop(crate::EdgeInsets::all(10.0)),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::PinchValue(Some(7)),
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(
        batch.items.iter().any(
            |i| matches!(i, MountItem::ViewProps { id: 2, props } if props.hit_slop.top == 10.0)
        )
    );
    assert!(
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Gestures { id: 2, values } if values.pinch == Some(7)))
    );
}

/// A label that follows app state goes alone, not with all the view's
/// props; any other change (or the first props) sends them all.
#[test]
fn a_changing_label_is_sent_alone() {
    use crate::props::AccessibilityProp;
    let label = |v: &str| Op::SetProp {
        id: 2,
        prop: Prop::Accessibility(AccessibilityProp::Label(Some(v.into()))),
    };
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            label("0 items"),
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(|i| matches!(i, MountItem::ViewProps { id: 2, props } if props.accessibility.label.as_deref() == Some("0 items"))));
    apply(&mut c, vec![label("1 item")]);
    let batch = c.commit().unwrap();
    assert_eq!(batch.items.len(), 1, "{:?}", batch.items);
    assert!(
        matches!(&batch.items[0], MountItem::AccessibilityLabel { id: 2, label: Some(l) } if &**l == "1 item")
    );
    // With another prop in the same commit, everything goes once.
    apply(
        &mut c,
        vec![
            label("2 items"),
            Op::SetProp {
                id: 2,
                prop: Prop::TestId(Some("counter".into())),
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert_eq!(batch.items.len(), 1, "{:?}", batch.items);
    assert!(
        matches!(&batch.items[0], MountItem::ViewProps { id: 2, props } if props.accessibility.label.as_deref() == Some("2 items"))
    );
}

#[test]
fn screens_fill_their_stack_below_the_header() {
    let mut c = core();
    c.set_safe_area(crate::EdgeInsets {
        top: 20.0,
        ..Default::default()
    });
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Stack,
            },
            style_op(2, &[("flex", StyleValue::Number(1.0))]),
            Op::Create {
                id: 3,
                tag: Tag::Screen,
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            style_op(4, &[("flex", StyleValue::Number(1.0))]),
            Op::Insert {
                parent: 3,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    // The screen fills the stack; its content starts below the status bar
    // and the 44pt bar.
    assert_eq!(
        frame_of(&batch, 3),
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 480.0
        })
    );
    assert_eq!(
        frame_of(&batch, 4),
        Some(Rect {
            x: 0.0,
            y: 64.0,
            width: 320.0,
            height: 416.0
        })
    );
    assert!(batch.items.iter().any(
        |i| matches!(i, MountItem::ScreenConfig { id: 3, config } if config.header_height == 64.0)
    ));
    // The header the platform measured replaces the estimate (Catalyst
    // can show its bar in the window's title bar).
    c.set_screen_header(3, Some(52.0));
    let batch = c.commit().unwrap();
    assert_eq!(
        frame_of(&batch, 4),
        Some(Rect {
            x: 0.0,
            y: 52.0,
            width: 320.0,
            height: 428.0
        })
    );
    assert!(batch.items.iter().any(
        |i| matches!(i, MountItem::ScreenConfig { id: 3, config } if config.header_height == 52.0)
    ));
    // Without a header, the content starts at the top.
    apply(
        &mut c,
        vec![Op::SetProp {
            id: 3,
            prop: Prop::Screen(crate::ScreenProp::HeaderShown(false)),
        }],
    );
    let batch = c.commit().unwrap();
    assert_eq!(
        frame_of(&batch, 4),
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 480.0
        })
    );
}

#[test]
fn modals_lay_out_at_the_reported_size() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Modal,
            },
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            style_op(3, &[("flex", StyleValue::Number(1.0))]),
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    // The viewport until the platform reports the presented size.
    assert_eq!(frame_of(&batch, 3).map(|f| f.height), Some(480.0));
    c.set_root_size(2, Size::new(320.0, 400.0), crate::EdgeInsets::default());
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 3).map(|f| f.height), Some(400.0));
}

#[test]
fn right_to_left_layouts() {
    let mut c = core();
    let n = |v: f32| StyleValue::Number(v);
    let s = |v: &'static str| StyleValue::Str(v);
    let mut ops = vec![
        Op::Create {
            id: 2,
            tag: Tag::View,
        },
        style_op(
            2,
            &[
                ("flexDirection", s("row")),
                ("width", n(300.0)),
                ("height", n(50.0)),
            ],
        ),
    ];
    // Two boxes in a row: the first with `marginStart`, the second with
    // `marginLeft` (swapped right to left, as in React Native).
    for (id, key) in [(3, "marginStart"), (4, "marginLeft")] {
        ops.push(Op::Create { id, tag: Tag::View });
        ops.push(Op::SetStyle {
            id,
            style: style(&[("width", n(50.0)), ("height", n(50.0)), (key, n(10.0))]),
            patch: StylePatch::default(),
        });
        ops.push(Op::Insert {
            parent: 2,
            child: id,
            before: None,
        });
    }
    ops.push(Op::Insert {
        parent: ROOT_ID,
        child: 2,
        before: None,
    });
    apply(&mut c, ops);
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 3).map(|f| f.x), Some(10.0));
    assert_eq!(frame_of(&batch, 4).map(|f| f.x), Some(70.0));

    // The surface turns right to left: the row runs from the right.
    apply(
        &mut c,
        vec![Op::SetProp {
            id: ROOT_ID,
            prop: Prop::LayoutDirection(true),
        }],
    );
    let batch = c.commit().unwrap();
    assert!(
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::LayoutDirection { rtl: true }))
    );
    assert_eq!(frame_of(&batch, 3).map(|f| f.x), Some(300.0 - 10.0 - 50.0));
    assert_eq!(
        frame_of(&batch, 4).map(|f| f.x),
        Some(300.0 - 60.0 - 10.0 - 50.0)
    );

    // A subtree built detached and inserted takes the direction too; an
    // explicit `direction` overrides it.
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 5,
                tag: Tag::View,
            },
            style_op(
                5,
                &[
                    ("flexDirection", s("row")),
                    ("direction", s("ltr")),
                    ("height", n(20.0)),
                ],
            ),
            Op::Create {
                id: 6,
                tag: Tag::View,
            },
            style_op(
                6,
                &[
                    ("width", n(20.0)),
                    ("start", n(5.0)),
                    ("position", s("absolute")),
                ],
            ),
            Op::Insert {
                parent: 5,
                child: 6,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 5,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 6).map(|f| f.x), Some(5.0));

    // Back to left to right.
    apply(
        &mut c,
        vec![Op::SetProp {
            id: ROOT_ID,
            prop: Prop::LayoutDirection(false),
        }],
    );
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 3).map(|f| f.x), Some(10.0));
    assert_eq!(frame_of(&batch, 4).map(|f| f.x), Some(70.0));
}

#[test]
fn right_to_left_horizontal_scrolls() {
    let mut c = core();
    let n = |v: f32| StyleValue::Number(v);
    let mut ops = vec![
        Op::SetProp {
            id: ROOT_ID,
            prop: Prop::LayoutDirection(true),
        },
        Op::Create {
            id: 2,
            tag: Tag::Scroll,
        },
        Op::SetProp {
            id: 2,
            prop: Prop::Horizontal(true),
        },
        style_op(
            2,
            &[
                ("width", n(150.0)),
                ("height", n(50.0)),
                ("paddingStart", n(10.0)),
            ],
        ),
    ];
    for id in [3, 4] {
        ops.push(Op::Create { id, tag: Tag::View });
        ops.push(Op::SetStyle {
            id,
            style: style(&[("width", n(100.0)), ("height", n(50.0))]),
            patch: StylePatch::default(),
        });
        ops.push(Op::Insert {
            parent: 2,
            child: id,
            before: None,
        });
    }
    ops.push(Op::Insert {
        parent: ROOT_ID,
        child: 2,
        before: None,
    });
    apply(&mut c, ops);
    let batch = c.commit().unwrap();
    // Content: 10 (start padding) + 200; the first child at the right end.
    assert_eq!(frame_of(&batch, 3).map(|f| f.x), Some(210.0 - 10.0 - 100.0));
    assert_eq!(frame_of(&batch, 4).map(|f| f.x), Some(0.0));
    // It starts scrolled to its right end.
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::Command {
            id: 2,
            command: Command::ScrollTo { x, .. }
        } if *x == 60.0
    )));
}

fn class_op(id: NodeId, classes: &str) -> Op {
    Op::SetProp {
        id,
        prop: Prop::Class(classes.into()),
    }
}

fn view_style_of(batch: &MountBatch, id: NodeId) -> Option<ViewStyle> {
    batch.items.iter().rev().find_map(|i| match i {
        MountItem::ViewStyle { id: i, style } if *i == id => Some((**style).clone()),
        _ => None,
    })
}

fn view_with_classes(c: &mut Core<FakeEngine>, id: NodeId, classes: &str) {
    apply(
        c,
        vec![
            Op::Create { id, tag: Tag::View },
            class_op(id, classes),
            Op::Insert {
                parent: ROOT_ID,
                child: id,
                before: None,
            },
        ],
    );
}

#[test]
fn classes_style_views_and_inline_styles_win() {
    let mut c = core();
    view_with_classes(&mut c, 2, "w-20 h-10 p-2 bg-blue-500 rounded-lg");
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 2).unwrap().width, 80.0);
    let v = view_style_of(&batch, 2).unwrap();
    assert_eq!(v.background_color, Color(0x2b7f_ffff));
    assert_eq!(v.border_radius, 8.0);
    // The inline style applies over the classes, and survives a class change.
    c.apply(style_op(
        2,
        &[("backgroundColor", StyleValue::Str("#ff0000"))],
    ))
    .unwrap();
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 2).unwrap().background_color,
        Color(0xff00_00ff)
    );
    c.apply(class_op(2, "w-24 h-10 bg-green-500")).unwrap();
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 2).unwrap().width, 96.0);
    let v = view_style_of(&batch, 2).unwrap();
    assert_eq!(v.background_color, Color(0xff00_00ff));
    assert_eq!(v.border_radius, 0.0);
    // Same classes again: nothing to do.
    c.apply(class_op(2, "w-24 h-10 bg-green-500")).unwrap();
    assert!(c.commit().is_none());
}

#[test]
fn important_classes_win_over_inline_styles() {
    let mut c = core();
    view_with_classes(
        &mut c,
        2,
        "!bg-blue-500 translate-x-2 !scale-150 opacity-50",
    );
    c.apply(style_op(
        2,
        &[
            ("backgroundColor", StyleValue::Str("#ff0000")),
            ("opacity", StyleValue::Number(0.8)),
        ],
    ))
    .unwrap();
    let v = view_style_of(&c.commit().unwrap(), 2).unwrap();
    assert_eq!(v.background_color, Color(0x2b7f_ffff));
    // Not important: the inline style wins.
    assert_eq!(v.opacity, 0.8);
    // The important scale still composes with the plain translation.
    let mut expected = crate::Transform::default();
    let _ = expected.push(crate::TransformKind::TranslateX, 8.0);
    let _ = expected.push(crate::TransformKind::Scale, 1.5);
    assert_eq!(v.transform, expected);
}

#[test]
fn the_apps_appearance_wins_over_the_systems() {
    use crate::platform::Appearance;
    let mut c = core();
    view_with_classes(&mut c, 2, "bg-white dark:bg-black");
    let background = |batch: &MountBatch| view_style_of(batch, 2).map(|s| s.background_color);
    assert_eq!(background(&c.commit().unwrap()), Some(Color(0xffff_ffff)));
    let appearance = |c: &mut Core<FakeEngine>, a| {
        apply(
            c,
            vec![Op::SetProp {
                id: ROOT_ID,
                prop: Prop::Appearance(a),
            }],
        )
    };
    appearance(&mut c, Appearance::Dark);
    let batch = c.commit().unwrap();
    assert_eq!(background(&batch), Some(Color(0x0000_00ff)));
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::Appearance {
            appearance: Appearance::Dark
        }
    )));
    // The system turning light changes nothing; following it again does.
    c.set_class_environment(crate::tailwind::ClassEnvironment::default());
    assert!(c.commit().is_none_or(|b| background(&b).is_none()));
    c.set_class_environment(crate::tailwind::ClassEnvironment {
        dark: true,
        ..Default::default()
    });
    appearance(&mut c, Appearance::Light);
    assert_eq!(background(&c.commit().unwrap()), Some(Color(0xffff_ffff)));
    appearance(&mut c, Appearance::System);
    assert_eq!(background(&c.commit().unwrap()), Some(Color(0x0000_00ff)));
}

#[test]
fn dark_mode_and_breakpoints_restyle_natively() {
    let mut c = core();
    view_with_classes(&mut c, 2, "bg-white dark:bg-black p-2 md:p-8");
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 2).unwrap().background_color,
        Color(0xffff_ffff)
    );
    c.set_class_environment(crate::tailwind::ClassEnvironment {
        dark: true,
        ..Default::default()
    });
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 2).unwrap().background_color,
        Color(0x0000_00ff)
    );
    // A wide window crosses `md`: the padding changes, the child moves.
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            style_op(3, &[("height", StyleValue::Number(5.0))]),
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 3).unwrap().x, 8.0);
    c.set_viewport(Size::new(1024.0, 768.0));
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 3).unwrap().x, 32.0);
}

#[test]
fn press_and_group_states() {
    let mut c = core();
    view_with_classes(&mut c, 2, "group bg-white active:bg-zinc-200");
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            class_op(3, "h-4 opacity-100 group-active:opacity-50"),
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    // The platform is asked to report the group's presses.
    assert!(batch.items.iter().any(|i| matches!(i, MountItem::TrackStates { id: 2, states } if states & crate::tailwind::state::PRESSED != 0)));
    c.set_states(2, crate::tailwind::state::PRESSED);
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 2).unwrap().background_color,
        Color(0xe4e4_e7ff)
    );
    assert_eq!(view_style_of(&batch, 3).unwrap().opacity, 0.5);
    c.set_states(2, 0);
    let batch = c.commit().unwrap();
    assert_eq!(view_style_of(&batch, 3).unwrap().opacity, 1.0);
}

#[test]
fn untracked_states_are_forgotten() {
    use crate::tailwind::state::{HOVERED, PRESSED};
    // A sidebar row: `hover:` while not selected, plain once selected.
    let mut c = core();
    view_with_classes(&mut c, 2, "bg-white hover:bg-zinc-200");
    c.commit().unwrap();
    c.set_states(2, HOVERED | PRESSED);
    c.commit().unwrap();
    // Selected: nothing reads the states, so the platform stops reporting
    // them and the pointer leaves unreported.
    apply(&mut c, vec![class_op(2, "bg-zinc-500")]);
    let batch = c.commit().unwrap();
    assert!(
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::TrackStates { id: 2, states: 0 }))
    );
    // Not selected again: no stale hover.
    apply(&mut c, vec![class_op(2, "bg-white hover:bg-zinc-200")]);
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 2).unwrap().background_color,
        Color(0xffff_ffff)
    );
}

#[test]
fn text_inherits_from_views_and_classes_set_props() {
    let mut c = core();
    view_with_classes(&mut c, 2, "text-white text-lg text-center");
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 3,
                tag: Tag::Text,
            },
            class_op(3, "line-clamp-2 font-bold"),
            Op::CreateText {
                id: 4,
                text: "hello".into(),
            },
            Op::Insert {
                parent: 3,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let text = batch
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Text { id: 3, text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.spans[0].style.color, Color(0xffff_ffff));
    assert_eq!(text.spans[0].style.font_size, 18.0);
    assert_eq!(text.spans[0].style.font_weight, 700);
    assert_eq!(text.align, TextAlign::Center);
    assert_eq!(text.max_lines, 2);
    // A container's text style change reaches the text inside.
    c.apply(class_op(2, "text-black text-lg")).unwrap();
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(|i| matches!(i, MountItem::Text { id: 3, text, .. } if text.spans[0].style.color == Color(0x0000_00ff))));
}

#[test]
fn sibling_positions() {
    let mut c = core();
    view_with_classes(&mut c, 2, "flex-row");
    for id in 3..6 {
        apply(
            &mut c,
            vec![
                Op::Create { id, tag: Tag::View },
                class_op(id, "w-2 h-2 bg-red-500 first:bg-green-500 last:bg-blue-500"),
                Op::Insert {
                    parent: 2,
                    child: id,
                    before: None,
                },
            ],
        );
    }
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 3).unwrap().background_color,
        Color(0x00c9_50ff)
    );
    assert_eq!(
        view_style_of(&batch, 4).unwrap().background_color,
        Color(0xfb2c_36ff)
    );
    assert_eq!(
        view_style_of(&batch, 5).unwrap().background_color,
        Color(0x2b7f_ffff)
    );
    // Removing the last child makes the middle one last.
    c.apply(Op::Remove {
        parent: 2,
        child: 5,
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 4).unwrap().background_color,
        Color(0x2b7f_ffff)
    );
}

#[test]
fn text_inherits_a_rows_positional_classes() {
    let mut c = core();
    view_with_classes(&mut c, 2, "");
    let add_row = |c: &mut Core<FakeEngine>, row: NodeId| {
        apply(
            c,
            vec![
                Op::Create {
                    id: row,
                    tag: Tag::View,
                },
                class_op(row, "text-black last:text-white"),
                Op::Create {
                    id: row + 1,
                    tag: Tag::Text,
                },
                Op::CreateText {
                    id: row + 2,
                    text: "label".into(),
                },
                Op::Insert {
                    parent: row + 1,
                    child: row + 2,
                    before: None,
                },
                Op::Insert {
                    parent: row,
                    child: row + 1,
                    before: None,
                },
                Op::Insert {
                    parent: 2,
                    child: row,
                    before: None,
                },
            ],
        );
    };
    let color = |batch: &MountBatch, id: NodeId| {
        batch.items.iter().find_map(|i| match i {
            MountItem::Text { id: t, text, .. } if *t == id => Some(text.spans[0].style.color),
            _ => None,
        })
    };
    add_row(&mut c, 10);
    add_row(&mut c, 20);
    let batch = c.commit().unwrap();
    assert_eq!(color(&batch, 11), Some(Color(0x0000_00ff)));
    assert_eq!(color(&batch, 21), Some(Color(0xffff_ffff)));
    // A row after it: the label of the row that was last turns back.
    add_row(&mut c, 30);
    let batch = c.commit().unwrap();
    assert_eq!(color(&batch, 21), Some(Color(0x0000_00ff)));
    assert_eq!(color(&batch, 31), Some(Color(0xffff_ffff)));
}

#[test]
fn fractional_translations_follow_the_size() {
    let mut c = core();
    view_with_classes(&mut c, 2, "w-20 h-10 -translate-x-1/2 translate-y-full");
    let batch = c.commit().unwrap();
    let v = view_style_of(&batch, 2).unwrap();
    let ops = v.transform.ops();
    assert_eq!(
        (ops[0].kind, ops[0].value),
        (crate::TransformKind::TranslateX, -40.0)
    );
    assert_eq!(
        (ops[1].kind, ops[1].value),
        (crate::TransformKind::TranslateY, 40.0)
    );
    c.apply(class_op(2, "w-40 h-10 -translate-x-1/2 translate-y-full"))
        .unwrap();
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 2).unwrap().transform.ops()[0].value,
        -80.0
    );
}

#[test]
fn disabled_views_take_no_presses() {
    let mut c = core();
    view_with_classes(&mut c, 2, "disabled:opacity-40");
    let press = |enabled| Op::Listen {
        id: 2,
        event: EventKind::Press,
        enabled,
    };
    let disabled = |d| Op::SetProp {
        id: 2,
        prop: Prop::Native(NativeProp::Disabled(d)),
    };
    let events_of = |batch: &MountBatch| {
        batch.items.iter().rev().find_map(|i| match i {
            MountItem::Events { id: 2, events } => Some(*events),
            _ => None,
        })
    };
    apply(&mut c, vec![press(true), disabled(true)]);
    let batch = c.commit().unwrap();
    assert!(!events_of(&batch).unwrap().contains(EventKind::Press));
    assert_eq!(view_style_of(&batch, 2).unwrap().opacity, 0.4);
    apply(&mut c, vec![disabled(false)]);
    let batch = c.commit().unwrap();
    assert!(events_of(&batch).unwrap().contains(EventKind::Press));
    assert_eq!(view_style_of(&batch, 2).unwrap().opacity, 1.0);
}

#[test]
fn underlay_shows_while_pressed() {
    let mut c = core();
    view_with_classes(&mut c, 2, "bg-white");
    view_with_classes(&mut c, 3, "opacity-50");
    apply(
        &mut c,
        vec![
            Op::Remove {
                parent: ROOT_ID,
                child: 3,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::UnderlayColor(Color::parse("#dddddd")),
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::TrackStates { id: 2, states } if *states == crate::tailwind::state::PRESSED
    )));
    c.set_states(2, crate::tailwind::state::PRESSED);
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 2).unwrap().background_color,
        Color(0xdddd_ddff)
    );
    assert_eq!(view_style_of(&batch, 3).unwrap().opacity, 0.5 * 0.85);
    c.set_states(2, 0);
    let batch = c.commit().unwrap();
    assert_eq!(
        view_style_of(&batch, 2).unwrap().background_color,
        Color(0xffff_ffff)
    );
    assert_eq!(view_style_of(&batch, 3).unwrap().opacity, 0.5);
}

#[test]
fn underlay_replaces_gradients_while_pressed() {
    let mut c = core();
    view_with_classes(&mut c, 2, "bg-linear-to-b from-white to-black");
    apply(
        &mut c,
        vec![Op::SetProp {
            id: 2,
            prop: Prop::UnderlayColor(Color::parse("#dddddd")),
        }],
    );
    let batch = c.commit().unwrap();
    assert!(view_style_of(&batch, 2).unwrap().effects.is_some());
    c.set_states(2, crate::tailwind::state::PRESSED);
    let batch = c.commit().unwrap();
    let style = view_style_of(&batch, 2).unwrap();
    assert_eq!(style.background_color, Color(0xdddd_ddff));
    assert!(style.effects.is_none());
    c.set_states(2, 0);
    let batch = c.commit().unwrap();
    assert!(view_style_of(&batch, 2).unwrap().effects.is_some());
}

#[test]
fn input_accessories_are_found_by_native_id() {
    let mut c = core();
    let accessory = |batch: &MountBatch| {
        batch.items.iter().rev().find_map(|i| match i {
            MountItem::Input { id: 2, config } => Some(config.accessory),
            _ => None,
        })
    };
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Input,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::InputAccessoryViewId(Some("bar".into())),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert_eq!(accessory(&batch), Some(None));
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 3,
                tag: Tag::InputAccessory,
            },
            Op::SetProp {
                id: 3,
                prop: Prop::NativeId(Some("bar".into())),
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 4,
                style: style(&[("height", StyleValue::Number(44.0))]),
                patch: Default::default(),
            },
            Op::Insert {
                parent: 3,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 3,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert_eq!(accessory(&batch), Some(Some(3)));
    // As wide as the window, as tall as its content.
    let frame = frame_of(&batch, 3).unwrap();
    assert_eq!((frame.width, frame.height), (320.0, 44.0));
    apply(
        &mut c,
        vec![Op::Remove {
            parent: ROOT_ID,
            child: 3,
        }],
    );
    let batch = c.commit().unwrap();
    assert_eq!(accessory(&batch), Some(None));
}

#[test]
fn inputs_report_their_content_size() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Input,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Multiline(true),
            },
            Op::SetStyle {
                id: 2,
                style: style(&[("width", StyleValue::Number(100.0))]),
                patch: Default::default(),
            },
            Op::Listen {
                id: 2,
                event: EventKind::ContentSizeChange,
                enabled: true,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let sizes = |c: &mut Core<FakeEngine>| {
        let mut events = Vec::new();
        c.take_events(&mut events);
        events
            .into_iter()
            .filter(|(id, kind, _)| *id == 2 && *kind == EventKind::ContentSizeChange)
            .count()
    };
    c.commit().unwrap();
    assert_eq!(sizes(&mut c), 1);
    // Typing a long text wraps it: taller.
    c.input_changed(2, &"x".repeat(80), 1);
    c.commit();
    assert_eq!(sizes(&mut c), 1);
    // The same text again: no event.
    c.input_changed(2, &"x".repeat(80), 2);
    c.commit();
    assert_eq!(sizes(&mut c), 0);
}

#[test]
fn text_shrinks_to_fit_its_lines() {
    let mut c = Core::new(crate::text::ApproxTextEngine, Size::new(320.0, 480.0));
    let ops = vec![
        Op::Create {
            id: 2,
            tag: Tag::Text,
        },
        Op::SetStyle {
            id: 2,
            style: style(&[
                ("width", StyleValue::Number(60.0)),
                ("fontSize", StyleValue::Number(20.0)),
            ]),
            patch: Default::default(),
        },
        Op::SetProp {
            id: 2,
            prop: Prop::NumberOfLines(1),
        },
        Op::SetProp {
            id: 2,
            prop: Prop::AdjustsFontSizeToFit(true),
        },
        Op::CreateText {
            id: 3,
            text: "Hello world".into(),
        },
        Op::Insert {
            parent: 2,
            child: 3,
            before: None,
        },
        Op::Insert {
            parent: ROOT_ID,
            child: 2,
            before: None,
        },
    ];
    for op in ops {
        c.apply(op).unwrap();
    }
    let batch = c.commit().unwrap();
    let size = batch
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Text { id: 2, text, .. } => Some(text.spans[0].style.font_size),
            _ => None,
        })
        .unwrap();
    // 11 characters at 0.55 em in 60pt: at most ~9.9pt.
    assert!(size < 10.0 && size > 9.0, "{size}");
}

#[test]
fn views_inside_text_are_attachments() {
    let mut c = core();
    let raw = |id, text: &str| Op::CreateText {
        id,
        text: text.into(),
    };
    let insert = |parent, child| Op::Insert {
        parent,
        child,
        before: None,
    };
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Text,
            },
            raw(3, "ab"),
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            style_op(
                4,
                &[
                    ("width", StyleValue::Number(20.0)),
                    ("height", StyleValue::Number(10.0)),
                    ("padding", StyleValue::Number(2.0)),
                ],
            ),
            Op::Create {
                id: 5,
                tag: Tag::View,
            },
            style_op(5, &[("height", StyleValue::Number(3.0))]),
            insert(4, 5),
            // A span holding an image (inside a span still attaches to the
            // block).
            Op::Create {
                id: 6,
                tag: Tag::Text,
            },
            raw(7, "cd"),
            Op::Create {
                id: 8,
                tag: Tag::Image,
            },
            style_op(
                8,
                &[
                    ("width", StyleValue::Number(12.0)),
                    ("height", StyleValue::Number(12.0)),
                ],
            ),
            insert(6, 7),
            insert(6, 8),
            insert(2, 3),
            insert(2, 4),
            insert(2, 6),
            style_op(2, &[("padding", StyleValue::Number(5.0))]),
            insert(ROOT_ID, 2),
        ],
    );
    let batch = c.commit().unwrap();
    let text = batch
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Text { id: 2, text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.text, "ab\u{FFFC}cd\u{FFFC}");
    assert_eq!(
        text.attachments,
        vec![
            crate::TextAttachment {
                index: 2,
                width: 20.0,
                height: 10.0
            },
            crate::TextAttachment {
                index: 5,
                width: 12.0,
                height: 12.0
            },
        ]
    );
    // Their views go inside the text view, on their placeholders.
    let inserts: Vec<_> = batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Insert {
                parent: 2, child, ..
            } => Some(*child),
            _ => None,
        })
        .collect();
    assert_eq!(inserts, vec![4, 8]);
    assert_eq!(
        frame_of(&batch, 4),
        Some(Rect {
            x: 5.0 + 16.0,
            y: 5.0 + 2.0,
            width: 20.0,
            height: 10.0
        })
    );
    assert_eq!(frame_of(&batch, 8).map(|f| f.x), Some(5.0 + 40.0));
    // The attachment's own subtree is laid out inside it.
    assert_eq!(
        frame_of(&batch, 5),
        Some(Rect {
            x: 2.0,
            y: 2.0,
            width: 16.0,
            height: 3.0
        })
    );

    // Growing an attachment builds the text again.
    apply(
        &mut c,
        vec![style_op(
            4,
            &[
                ("width", StyleValue::Number(30.0)),
                ("height", StyleValue::Number(10.0)),
            ],
        )],
    );
    let batch = c.commit().unwrap();
    let text = batch
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Text { id: 2, text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.attachments[0].width, 30.0);
    assert_eq!(frame_of(&batch, 4).map(|f| f.width), Some(30.0));

    // Taken out of the text: its view leaves the text view.
    apply(
        &mut c,
        vec![Op::Remove {
            parent: 2,
            child: 4,
        }],
    );
    let batch = c.commit().unwrap();
    assert!(
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Remove { id: 4 }))
    );
    let text = batch
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Text { id: 2, text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.text, "abcd\u{FFFC}");
    assert_eq!(frame_of(&batch, 8).map(|f| f.x), Some(5.0 + 32.0));
}

#[test]
fn node_quota_and_dense_ids() {
    let mut c = core();
    c.set_max_nodes(3);
    // The root is one node.
    for id in 2..4 {
        c.apply(Op::Create { id, tag: Tag::View }).unwrap();
    }
    assert!(matches!(
        c.apply(Op::Create {
            id: 4,
            tag: Tag::View
        }),
        Err(ApplyError::TooManyNodes(3))
    ));
    let mut c = core();
    c.set_max_nodes(1000);
    // An id that would make the id table enormous.
    let err = c
        .apply(Op::Create {
            id: 4_000_000_000,
            tag: Tag::View,
        })
        .unwrap_err();
    assert!(matches!(err, ApplyError::IdOutOfRange(_)) && err.is_fatal());
}

#[test]
fn refused_urls_fail_and_send_nothing() {
    let mut c = core();
    c.set_access(Arc::new(crate::access::Access {
        declared: crate::access::Capabilities::from_json(
            &serde_json::json!({ "network": { "hosts": ["cdn.example.com"] } }),
        )
        .unwrap(),
        granted: None,
    }));
    let image = |id, uri: &str| {
        vec![
            Op::Create {
                id,
                tag: Tag::Image,
            },
            Op::Listen {
                id,
                event: EventKind::Error,
                enabled: true,
            },
            Op::SetProp {
                id,
                prop: Prop::Source(Some(ImageSource {
                    uri: uri.into(),
                    size: None,
                })),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: id,
                before: None,
            },
        ]
    };
    let mut ops = image(2, "https://cdn.example.com/a.png");
    ops.extend(image(3, "https://tracker.io/pixel.gif"));
    ops.push(Op::Command {
        id: ROOT_ID,
        command: Command::StatusBar(Default::default()),
    });
    apply(&mut c, ops);
    let batch = c.commit().unwrap();
    let sources: Vec<_> = batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Image { id, source, .. } => {
                Some((*id, source.as_ref().map(|s| s.uri.to_string())))
            }
            _ => None,
        })
        .collect();
    // The refused image loads nothing.
    assert_eq!(
        sources,
        [
            (2, Some("https://cdn.example.com/a.png".to_owned())),
            (3, None)
        ]
    );
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Command { .. }))
    );
    let mut events = Vec::new();
    c.take_events(&mut events);
    assert!(matches!(
        &events[..],
        [(3, EventKind::Error, EventPayload::Error { message })] if message.contains("`tracker.io`")
    ));
}

mod components {
    use serde_json::json;

    use super::*;
    use crate::{
        Prop,
        ui::{Children, Event, Props, Registry, Slot, prelude::*},
    };

    /// Counts taps; shows `label` and the count; holds its JSX children.
    #[derive(Clone, PartialEq, Props)]
    struct TallyProps {
        #[prop(default = "Taps".into())]
        label: crate::ui::Prop<SharedString>,
        children: Children,
    }

    #[derive(Event)]
    enum TallyEvent {
        Tap { count: u32 },
    }

    #[component]
    fn Tally(props: TallyProps, cx: Scope<TallyEvent>) -> impl IntoElement {
        let taps = signal(0);
        view()
            .p_2()
            .on_press(move |_: &()| {
                taps.update(|t| *t += 1);
                cx.emit(TallyEvent::Tap {
                    count: taps.get_untracked(),
                });
            })
            .child(text(move || {
                format!("{} {}", props.label.get(), taps.get())
            }))
            .child(view().gap_1().child(props.children))
    }

    fn texts(batch: &MountBatch) -> Vec<String> {
        batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Text { text, .. } => Some(text.text.clone()),
                _ => None,
            })
            .collect()
    }

    fn core_with_tally() -> Core<FakeEngine> {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Tally>();
        c.set_components(registry);
        c
    }

    /// A Rust app's root: mounted with no op from JS and no registration.
    #[test]
    fn a_rust_root_mounts_renders_and_handles_events() {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Tally>(|| TallyProps {
            label: "Root".into(),
            children: Children::default(),
        }));
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), ["Root 0"]);
        let host = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Insert {
                    parent: ROOT_ID,
                    child,
                    ..
                } => Some(*child),
                _ => None,
            })
            .expect("the root's host is in the root view");
        assert_eq!(host % 2, 1, "the core's own id");
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        assert_eq!(texts(&c.commit().unwrap()), ["Root 1"]);
    }

    #[test]
    fn components_render_handle_events_and_place_children() {
        let mut c = core_with_tally();
        let prop = |name: &str, value| Op::SetProp {
            id: 2,
            prop: Prop::ComponentProp(name.into(), Some(value)),
        };
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Tally>().into()),
                },
                prop("label", json!("Hits")),
                Op::Create {
                    id: 4,
                    tag: Tag::View,
                },
                Op::Insert {
                    parent: 2,
                    child: 4,
                    before: None,
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
            ],
        );
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), ["Hits 0"]);
        // Expanded nodes have odd ids; the JS child sits where the render
        // put `Children`, not in the host.
        let holder = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Insert {
                    parent, child: 4, ..
                } => Some(*parent),
                _ => None,
            })
            .expect("JS child placed");
        assert!(holder % 2 == 1 && holder != 2);
        assert!(
            batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Create { id: 2, .. }))
        );

        // A press on the host goes to the component, which emits and
        // updates its text.
        assert!(c.component_event(2, EventKind::Press, &EventPayload::None));
        let mut events = Vec::new();
        c.take_events(&mut events);
        assert_eq!(
            events,
            [(
                2,
                EventKind::NativeEvent,
                EventPayload::Native {
                    name: "tap".into(),
                    data: json!({ "count": 1 })
                }
            )]
        );
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), ["Hits 1"]);

        // A prop change reaches what reads it; only what changed is sent.
        // Clearing a prop restores its default.
        apply(
            &mut c,
            vec![Op::SetProp {
                id: 2,
                prop: Prop::ComponentProp("label".into(), None),
            }],
        );
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), ["Taps 1"]);
        assert!(
            !batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Create { .. }))
        );
        // A value of the wrong type is refused (and logged), nothing changes.
        apply(&mut c, vec![prop("label", json!(3))]);
        assert!(c.commit().is_none());

        // Unknown events on the host (no handler) are JS's.
        assert!(!c.component_event(2, EventKind::Layout, &EventPayload::None));

        // Releasing the host frees what it rendered; its JS child stays.
        apply(
            &mut c,
            vec![
                Op::Remove {
                    parent: ROOT_ID,
                    child: 2,
                },
                Op::Release { id: 2 },
            ],
        );
        let batch = c.commit().unwrap();
        let deleted: Vec<NodeId> = batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Delete { id } => Some(*id),
                _ => None,
            })
            .collect();
        assert!(deleted.contains(&2) && deleted.contains(&holder));
        assert!(!deleted.contains(&4));
        assert!(c.key(4).is_ok());
    }

    /// A header slot above its children.
    #[derive(Clone, Copy, PartialEq, Props)]
    struct CardProps {
        header: crate::ui::Prop<Option<Slot>>,
        children: Children,
    }

    #[component]
    fn Card(props: CardProps) -> impl IntoElement {
        let filled = move || props.header.with(Option::is_some);
        view().child(props.header.place(|h| h.h(px(44.0)))).child(
            view()
                .mt_0()
                .style_when(filled, |v| v.mt_2())
                .child(props.children),
        )
    }

    #[test]
    fn slots_are_the_glues_nodes_placed_by_the_render() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Card>();
        c.set_components(registry);
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Card>().into()),
                },
                // The glue's node for `header`, then its JSX.
                Op::Create {
                    id: 4,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 4,
                    prop: Prop::ComponentSlot("header".into()),
                },
                Op::Insert {
                    parent: 2,
                    child: 4,
                    before: None,
                },
                Op::Create {
                    id: 6,
                    tag: Tag::View,
                },
                Op::Insert {
                    parent: 4,
                    child: 6,
                    before: None,
                },
                // Children go before the slot's node (the anchor).
                Op::Create {
                    id: 8,
                    tag: Tag::View,
                },
                style_op(8, &[("height", StyleValue::Number(10.0))]),
                Op::Insert {
                    parent: 2,
                    child: 8,
                    before: Some(4),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
            ],
        );
        let batch = c.commit().unwrap();
        // The slot's node is the header element (in the host, 44pt tall).
        assert!(batch.items.iter().any(|i| matches!(
            i,
            MountItem::Insert {
                parent: 2,
                child: 4,
                ..
            }
        )));
        assert_eq!(frame_of(&batch, 4).map(|f| f.height), Some(44.0));
        // The children sit below it, after `mt_2` (the slot is filled).
        let holder = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Insert {
                    parent, child: 8, ..
                } => Some(*parent),
                _ => None,
            })
            .unwrap();
        assert_eq!(frame_of(&batch, holder).map(|f| f.y), Some(52.0));

        // Emptying the slot: no header, `mt_0`.
        apply(
            &mut c,
            vec![Op::Remove {
                parent: 4,
                child: 6,
            }],
        );
        let batch = c.commit().unwrap();
        assert!(
            batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Remove { id: 4 }))
        );
        assert_eq!(frame_of(&batch, holder).map(|f| f.y), Some(0.0));
    }

    /// Renders a Tally and counts its taps.
    #[derive(Clone, PartialEq, Props)]
    struct OuterProps {}

    #[derive(Event)]
    enum OuterEvent {
        InnerTap(u32),
    }

    #[component]
    fn Outer(_props: OuterProps, cx: Scope<OuterEvent>) -> impl IntoElement {
        let inner_taps = signal(0);
        view()
            .child(text(move || format!("outer {}", inner_taps.get())))
            .child(
                Tally::element(TallyProps {
                    label: "In".into(),
                    ..TallyProps::initial()
                })
                .on_event(move |event: &TallyEvent| {
                    let TallyEvent::Tap { .. } = event;
                    inner_taps.update(|t| *t += 1);
                    cx.emit(OuterEvent::InnerTap(inner_taps.get_untracked()));
                }),
            )
    }

    #[test]
    fn nested_components_render_and_report_to_their_parent() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Tally>().register::<Outer>();
        c.set_components(registry);
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Outer>().into()),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
            ],
        );
        let batch = c.commit().unwrap();
        let mut shown = texts(&batch);
        shown.sort();
        assert_eq!(shown, ["In 0", "outer 0"]);
        // The nested Tally's host: the node listening to presses.
        let inner = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
                _ => None,
            })
            .unwrap();
        assert!(c.component_event(inner, EventKind::Press, &EventPayload::None));
        let mut events = Vec::new();
        c.take_events(&mut events);
        // JS hears the outer component's event, not the inner one.
        assert_eq!(
            events,
            [(
                2,
                EventKind::NativeEvent,
                EventPayload::Native {
                    name: "innerTap".into(),
                    data: json!(1)
                }
            )]
        );
        let mut shown = texts(&c.commit().unwrap());
        shown.sort();
        assert_eq!(shown, ["In 1", "outer 1"]);
    }

    #[test]
    fn the_apps_class_goes_last_and_unknown_components_render_nothing() {
        let mut c = core_with_tally();
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Tally>().into()),
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Class("p-8".into()),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
                Op::Create {
                    id: 6,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 6,
                    prop: Prop::Component("test.missing".into()),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 6,
                    before: None,
                },
            ],
        );
        let batch = c.commit().unwrap();
        // `p-8` (32pt) wins over the component's `p_2`: the text sits 32pt in.
        let text_id = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Text { id, .. } => Some(*id),
                _ => None,
            })
            .unwrap();
        let frame = frame_of(&batch, text_id).unwrap();
        assert_eq!((frame.x, frame.y), (32.0, 32.0));
        // The unknown one is an empty view.
        assert!(
            batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Create { id: 6, .. }))
        );
    }

    /// A theme-colored swatch that darkens while pressed.
    #[derive(Clone, PartialEq, Props)]
    struct SwatchProps {}

    #[component]
    fn Swatch(_props: SwatchProps) -> impl IntoElement {
        view()
            .size_10()
            .bg(move || theme().primary)
            .active(|s| s.bg(rgb(0xff0000)))
    }

    #[test]
    fn theme_colors_and_press_styles_switch_natively() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Swatch>();
        c.set_components(registry);
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Swatch>().into()),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
            ],
        );
        let background = |batch: &MountBatch| {
            batch.items.iter().find_map(|i| match i {
                MountItem::ViewStyle { id: 2, style } => Some(style.background_color),
                _ => None,
            })
        };
        let theme = crate::ui::Theme::default();
        let batch = c.commit().unwrap();
        assert_eq!(background(&batch), Some(theme.primary.light));
        assert_eq!(frame_of(&batch, 2).map(|f| f.width), Some(40.0));
        // The platform reports presses on it (`active`).
        assert!(batch.items.iter().any(|i| matches!(
            i,
            MountItem::TrackStates { id: 2, states } if states & crate::tailwind::state::PRESSED != 0
        )));
        // Dark appearance: restyled natively.
        c.set_class_environment(crate::tailwind::ClassEnvironment {
            dark: true,
            ..Default::default()
        });
        let batch = c.commit().unwrap();
        assert_eq!(background(&batch), Some(theme.primary.dark));
        c.set_states(2, crate::tailwind::state::PRESSED);
        let batch = c.commit().unwrap();
        assert_eq!(background(&batch), Some(Color(0xff0000ff)));
    }

    #[test]
    fn the_host_theme_wins_over_the_apps_until_cleared() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Swatch>();
        c.set_components(registry);
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Swatch>().into()),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
            ],
        );
        let background = |batch: &MountBatch| {
            batch.items.iter().find_map(|i| match i {
                MountItem::ViewStyle { id: 2, style } => Some(style.background_color),
                _ => None,
            })
        };
        let themed = |hex: &str| {
            crate::ui::Theme::from_json(&format!(
                r#"{{ "themes": [{{ "colors": {{ "primary.background": "{hex}" }} }}] }}"#
            ))
            .unwrap()
        };
        c.commit().unwrap();
        c.set_host_theme(Some(themed("#00ff00")));
        assert_eq!(
            c.commit().as_ref().and_then(background),
            Some(rgb(0x00ff00))
        );
        // The app's own theme (`uiTheme`) waits behind the host's.
        apply(
            &mut c,
            vec![Op::SetProp {
                id: ROOT_ID,
                prop: Prop::UiTheme(
                    r##"{ "themes": [{ "colors": { "primary.background": "#0000ff" } }] }"##.into(),
                ),
            }],
        );
        assert_eq!(c.commit().as_ref().and_then(background), None);
        c.set_host_theme(None);
        assert_eq!(
            c.commit().as_ref().and_then(background),
            Some(rgb(0x0000ff))
        );
        // Setting the same theme again renders nothing.
        c.set_theme(themed("#0000ff"));
        assert_eq!(c.commit().as_ref().and_then(background), None);
    }

    /// Wraps each JS child in a padded row.
    #[derive(Clone, PartialEq, Props)]
    struct RowsProps {
        children: Children,
    }

    #[component]
    fn Rows(props: RowsProps) -> impl IntoElement {
        view().children_each(props.children, |child, at| {
            view()
                .p_1()
                .style_when(move || !at.is_first(), |v| v.mt_2())
                .style_when(move || at.is_last(), |v| v.pb_2())
                .child(child)
        })
    }

    #[test]
    fn children_each_wraps_every_js_child_and_follows_changes() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Rows>();
        c.set_components(registry);
        let child = |id: NodeId| {
            vec![
                Op::Create { id, tag: Tag::View },
                style_op(id, &[("height", StyleValue::Number(10.0))]),
            ]
        };
        let mut ops = vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Component(std::any::type_name::<Rows>().into()),
            },
        ];
        ops.extend(child(4));
        ops.extend(child(6));
        ops.extend([
            Op::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 6,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ]);
        apply(&mut c, ops);
        let batch = c.commit().unwrap();
        let wrapper_of = |batch: &MountBatch, id: NodeId| {
            batch.items.iter().find_map(|i| match i {
                MountItem::Insert { parent, child, .. } if *child == id => Some(*parent),
                _ => None,
            })
        };
        let (w4, w6) = (
            wrapper_of(&batch, 4).unwrap(),
            wrapper_of(&batch, 6).unwrap(),
        );
        assert!(w4 % 2 == 1 && w6 % 2 == 1 && w4 != w6);
        // 4pt padding, 10pt children, 8pt between the rows.
        assert_eq!(
            frame_of(&batch, w4).map(|f| (f.y, f.height)),
            Some((0.0, 18.0))
        );
        // The last has 8pt below.
        assert_eq!(
            frame_of(&batch, w6).map(|f| (f.y, f.height)),
            Some((26.0, 22.0))
        );

        // A child inserted first gets its own wrapper; the others keep theirs
        // and move down (the second row now has the margin).
        let mut ops = child(8);
        ops.push(Op::Insert {
            parent: 2,
            child: 8,
            before: Some(4),
        });
        apply(&mut c, ops);
        let batch = c.commit().unwrap();
        let w8 = wrapper_of(&batch, 8).unwrap();
        assert!(![w4, w6].contains(&w8));
        assert_eq!(frame_of(&batch, w8).map(|f| f.y), Some(0.0));
        assert_eq!(frame_of(&batch, w4).map(|f| f.y), Some(26.0));
        assert!(wrapper_of(&batch, 4).is_none() && wrapper_of(&batch, 6).is_none());

        // A child removed takes its wrapper away; the child itself stays JS's.
        apply(
            &mut c,
            vec![Op::Remove {
                parent: 2,
                child: 4,
            }],
        );
        let batch = c.commit().unwrap();
        assert!(
            batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Delete { id } if *id == w4))
        );
        assert!(c.key(4).is_ok());
        assert_eq!(frame_of(&batch, w6).map(|f| f.y), Some(26.0));
    }

    /// Ticks every 20ms from `start` until 3, then reports it; shows
    /// whether the window is wide.
    #[derive(Clone, Copy, PartialEq, Props)]
    struct ClockProps {
        #[prop(default)]
        start: crate::ui::Prop<u32>,
    }

    #[derive(Event)]
    enum ClockEvent {
        Done,
    }

    #[component]
    fn Clock(props: ClockProps, cx: Scope<ClockEvent>) -> impl IntoElement {
        let ticks = signal(props.start.get_untracked());
        // A new start starts again from it.
        effect(move || ticks.set(props.start.get()));
        let timer = stored(None::<Spawned<()>>);
        timer.replace(Some(set_interval(
            std::time::Duration::from_millis(20),
            move || {
                ticks.update(|t| *t += 1);
                if ticks.get_untracked() == 3 {
                    cx.emit(ClockEvent::Done);
                    // Cancelling the task stops it (`clearInterval`).
                    if let Some(timer) = timer.replace(None) {
                        timer.cancel();
                    }
                }
            },
        )));
        view().child(text(move || {
            let width = if window().width > 500.0 {
                "wide"
            } else {
                "narrow"
            };
            format!("{} {width}", ticks.get())
        }))
    }

    /// Fetches a value on the runtime's threads, then shows it.
    #[derive(Clone, PartialEq, Props)]
    struct LoaderProps {}

    #[component]
    fn Loader(_props: LoaderProps) -> impl IntoElement {
        let value = signal(None::<u64>);
        spawn(async move {
            // tokio's ecosystem: a `Send` future on the runtime, a timer.
            let loaded = background(async { (1..=10u64).product::<u64>() })
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            value.set(Some(loaded));
        });
        view().child(text(move || match value.get() {
            Some(v) => format!("loaded {v}"),
            None => "loading".to_owned(),
        }))
    }

    /// Runs the tasks and commits until `done` holds of what was shown.
    fn run_until(c: &mut Core<FakeEngine>, done: impl Fn(&[String]) -> bool) -> Vec<String> {
        let mut shown = Vec::new();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < end {
            c.run_component_tasks();
            if let Some(batch) = c.commit() {
                shown.extend(texts(&batch));
                if done(&shown) {
                    return shown;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("not done: {shown:?}");
    }

    #[test]
    fn state_tasks_props_and_the_environment() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_time()
            .build()
            .unwrap();
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Clock>().register::<Loader>();
        c.set_components(registry);
        c.set_task_runtime(runtime.handle().clone());
        let mount = |c: &mut Core<FakeEngine>, id: NodeId, name: &str| {
            apply(
                c,
                vec![
                    Op::Create { id, tag: Tag::View },
                    Op::SetProp {
                        id,
                        prop: Prop::Component(name.into()),
                    },
                    Op::Insert {
                        parent: ROOT_ID,
                        child: id,
                        before: None,
                    },
                ],
            )
        };
        mount(&mut c, 2, std::any::type_name::<Clock>());
        assert_eq!(texts(&c.commit().unwrap()), ["0 narrow"]);

        // The interval (a task: a timer, then an update) ticks to 3,
        // reports it and stops.
        let shown = run_until(&mut c, |s| s.iter().any(|t| t == "3 narrow"));
        assert!(shown.starts_with(&["1 narrow".to_owned()]));
        let mut events = Vec::new();
        c.take_events(&mut events);
        assert!(
            matches!(&events[..], [(2, EventKind::NativeEvent, EventPayload::Native { name, .. })] if &**name == "done")
        );
        std::thread::sleep(std::time::Duration::from_millis(60));
        c.run_component_tasks();
        assert!(c.commit().is_none(), "stopped");

        // A prop change reaches what reads it.
        apply(
            &mut c,
            vec![Op::SetProp {
                id: 2,
                prop: Prop::ComponentProp("start".into(), Some(json!(10))),
            }],
        );
        assert_eq!(texts(&c.commit().unwrap()), ["10 narrow"]);

        // What a binding read of the environment updates it; the rest does
        // not.
        let env = crate::ui::Environment {
            window: Size::new(390.0, 844.0),
            ..Default::default()
        };
        c.set_environment(crate::ui::Environment {
            keyboard_height: 300.0,
            ..env
        });
        assert!(c.commit().is_none());
        c.set_viewport(Size::new(800.0, 600.0));
        assert!(texts(&c.commit().unwrap()).contains(&"10 wide".to_owned()));

        // A task with background work and a tokio timer.
        mount(&mut c, 4, std::any::type_name::<Loader>());
        assert!(texts(&c.commit().unwrap()).contains(&"loading".to_owned()));
        run_until(&mut c, |s| s.iter().any(|t| t == "loaded 3628800"));

        // A released component's tasks end with it.
        mount(&mut c, 6, std::any::type_name::<Clock>());
        c.commit();
        assert_eq!(c.components.executor.task_count(), 1);
        apply(
            &mut c,
            vec![
                Op::Remove {
                    parent: ROOT_ID,
                    child: 6,
                },
                Op::Release { id: 6 },
            ],
        );
        c.commit();
        assert_eq!(c.components.executor.task_count(), 0);
    }

    /// A form: an input it controls, a scroll view, a modal while the
    /// name is long enough, a canvas drawing its length.
    #[component]
    fn Form(_props: FormProps) -> impl IntoElement {
        let name = signal(String::new());
        let focused = signal(false);
        let field = element_ref();
        let back = field.clone();
        view()
            .child(
                scroll().bounces(false).child(
                    input()
                        .element_ref(&field)
                        .value(move || name.get())
                        .placeholder(move || {
                            if name.with(String::is_empty) {
                                "Name"
                            } else {
                                ""
                            }
                        })
                        .on_change_text(move |text: &str| name.set(text.to_owned()))
                        .on_focus(move |_: &()| focused.set(true)),
                ),
            )
            .child(canvas(move || {
                let len = name.with(String::len) as f32;
                Painter::new().fill_rect(0.0, 0.0, len, 4.0)
            }))
            .child(If::new(
                move || name.with(String::len) > 2,
                move || {
                    modal()
                        .transparent(true)
                        .child(text(move || format!("Hi {}", name.get())))
                },
            ))
            .child(text(move || if focused.get() { "focused" } else { "idle" }))
            // Back to the field: focus it, the caret at the end.
            .child(view().on_press(move |_: &()| {
                back.focus();
                back.set_selection(3, 3);
            }))
    }

    #[derive(Clone, PartialEq, Props)]
    struct FormProps {}

    #[test]
    fn host_elements_take_typed_props_and_events() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Form>();
        c.set_components(registry);
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Form>().into()),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
            ],
        );
        let batch = c.commit().unwrap();
        let created = |batch: &MountBatch, wanted: ViewKind| {
            batch.items.iter().find_map(|i| match i {
                MountItem::Create { id, kind } if *kind == wanted => Some(*id),
                _ => None,
            })
        };
        let input = created(&batch, ViewKind::Input).expect("an input");
        let button = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
                _ => None,
            })
            .expect("the button listens to presses");
        assert!(created(&batch, ViewKind::Scroll).is_some());
        assert!(created(&batch, ViewKind::Modal).is_none(), "no modal yet");
        let placeholder = |batch: &MountBatch| {
            batch.items.iter().find_map(|i| match i {
                MountItem::Input { config, .. } => Some(config.placeholder.clone()),
                _ => None,
            })
        };
        assert_eq!(placeholder(&batch).as_deref(), Some("Name"));
        assert!(
            batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Canvas { .. }))
        );
        assert_eq!(texts(&batch), ["idle"]);

        // Typing: the component gets the text, the placeholder goes back to
        // its default, the modal appears, the canvas draws again; the
        // input's value is the platform's already, so it is not sent back.
        c.input_changed(input, "Ada", 1);
        assert!(c.component_event(
            input,
            EventKind::ChangeText,
            &EventPayload::Text {
                text: "Ada".into(),
                count: 1
            }
        ));
        assert!(c.component_event(input, EventKind::Focus, &EventPayload::None));
        let batch = c.commit().unwrap();
        assert_eq!(placeholder(&batch).as_deref(), Some(""));
        assert!(created(&batch, ViewKind::Modal).is_some());
        assert!(
            batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Canvas { .. }))
        );
        assert!(
            !batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::InputValue { .. }))
        );
        assert_eq!(texts(&batch), ["Hi Ada", "focused"]);

        // The ref's commands reach the input it is on, in the next batch.
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        let commands: Vec<_> = batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Command { id, command } => Some((*id, command.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            commands,
            [
                (input, Command::Focus),
                (input, Command::SetSelection { start: 3, end: 3 })
            ]
        );
    }

    /// Measures its box when pressed (and a ref no element carries).
    #[component]
    fn Ruler() -> impl IntoElement {
        let target = element_ref();
        let unused = element_ref();
        let shown = signal(String::new());
        let measured = target.clone();
        view()
            .p_4()
            .child(
                view()
                    .pl(px(10.0))
                    .child(view().element_ref(&target).w(px(30.0)).h(px(20.0))),
            )
            .child(text(move || shown.get()))
            .child(view().w(px(5.0)).h(px(5.0)).on_press(move |_: &()| {
                let (target, unused) = (measured.measure(), unused.measure());
                spawn(async move {
                    let (m, none) = (target.await, unused.await);
                    let m = m.expect("laid out");
                    shown.set(format!(
                        "{},{} {}x{} at {},{} {}",
                        m.frame.x,
                        m.frame.y,
                        m.frame.width,
                        m.frame.height,
                        m.page_x,
                        m.page_y,
                        none.is_none()
                    ));
                });
            }))
    }

    #[test]
    fn element_refs_measure_with_the_commits_layout() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Ruler>();
        c.set_components(registry);
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Ruler>().into()),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
            ],
        );
        let batch = c.commit().unwrap();
        let button = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
                _ => None,
            })
            .expect("the button listens to presses");
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        // The frame in its parent (past the parent's padding), the corner
        // in the surface (past the component's padding too); the ref no
        // element carries measures nothing.
        let shown = run_until(&mut c, |s| s.iter().any(|t| t.contains(" at ")));
        assert_eq!(shown.last().unwrap(), "10,0 30x20 at 26,16 true");
    }

    #[derive(Clone, PartialEq, Debug)]
    struct Accent(u32);

    #[derive(Default, Clone)]
    struct Clicks(u32);

    #[derive(Clone, PartialEq, Props)]
    struct WithChildren {
        children: Children,
    }

    /// Provides an accent to what is below it, through the app's JSX.
    #[component]
    fn Theming(props: WithChildren) -> impl IntoElement {
        let accent = signal(Accent(0));
        provide_context(accent);
        view()
            .on_press(move |_: &()| accent.update(|a| a.0 += 1))
            .child(props.children)
    }

    /// Shows the accent above it and the surface's clicks; a press counts
    /// one.
    #[component]
    fn AccentSwatch(_props: FormProps) -> impl IntoElement {
        let accent = use_context::<Signal<Accent>>();
        let clicks = global::<Clicks>();
        view()
            .on_press(move |_: &()| clicks.update(|c| c.0 += 1))
            .child(text(move || {
                let accent = accent.map_or(0, |a| a.with(|a| a.0));
                format!("accent {accent} clicks {}", clicks.with(|c| c.0))
            }))
    }

    #[test]
    fn context_flows_down_and_globals_reach_their_readers() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Theming>().register::<AccentSwatch>();
        c.set_components(registry);
        // <Theming><view><Swatch/></view></Theming> and a second Swatch
        // outside it.
        let host = |id, name: &str| {
            vec![
                Op::Create { id, tag: Tag::View },
                Op::SetProp {
                    id,
                    prop: Prop::Component(name.into()),
                },
            ]
        };
        let mut ops = host(2, std::any::type_name::<Theming>());
        ops.push(Op::Create {
            id: 4,
            tag: Tag::View,
        });
        ops.extend(host(6, std::any::type_name::<AccentSwatch>()));
        ops.extend(host(8, std::any::type_name::<AccentSwatch>()));
        for (parent, child) in [(4, 6), (2, 4), (ROOT_ID, 2), (ROOT_ID, 8)] {
            ops.push(Op::Insert {
                parent,
                child,
                before: None,
            });
        }
        apply(&mut c, ops);
        let batch = c.commit().unwrap();
        let mut shown = texts(&batch);
        shown.sort();
        assert_eq!(shown, ["accent 0 clicks 0", "accent 0 clicks 0"]);

        // A new value: only the swatch below the provider changes.
        assert!(c.component_event(2, EventKind::Press, &EventPayload::None));
        assert_eq!(texts(&c.commit().unwrap()), ["accent 1 clicks 0"]);

        // A global changed by one swatch changes both.
        assert!(c.component_event(8, EventKind::Press, &EventPayload::None));
        let mut shown = texts(&c.commit().unwrap());
        shown.sort();
        assert_eq!(shown, ["accent 0 clicks 1", "accent 1 clicks 1"]);
    }

    /// A sheet that follows drags natively and springs back when let go.
    #[component]
    fn Sheet(_props: FormProps) -> impl IntoElement {
        let offset = native_value(0.0);
        let state = signal("resting");
        let springing = offset.clone();
        view()
            .bind_translate_y(&offset)
            .bind_opacity(&offset.interpolate(&[0.0, 300.0], &[1.0, 0.2]))
            .pan_values(None, Some(&offset))
            .on_pan_end(move |e: &PanEvent| {
                assert_eq!(e.value_y, Some(120.0));
                let end = springing.spring(0.0, Spring::default());
                state.set("springing");
                spawn(async move {
                    let finished = end.await;
                    state.set(if finished { "back" } else { "stopped" });
                });
            })
            .child(text(move || state.get()))
    }

    #[test]
    fn native_values_bind_animate_and_end() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Sheet>();
        c.set_components(registry);
        c.set_task_runtime(runtime.handle().clone());
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Sheet>().into()),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
            ],
        );
        let batch = c.commit().unwrap();
        let animations: Vec<_> = batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Animation(op) => Some(op.clone()),
                _ => None,
            })
            .collect();
        // The value and its interpolation, in the core's id range.
        let value = match animations[0] {
            nsh_animation::AnimationOp::CreateValue { node, value: 0.0 } => node,
            ref op => panic!("{op:?}"),
        };
        assert_eq!(value % 2, 1, "the core's ids are odd");
        assert!(
            matches!(animations[1], nsh_animation::AnimationOp::CreateInterpolate { input, .. } if input == value)
        );
        assert_eq!(texts(&batch), ["resting"]);

        // Let go: a spring starts; its end resolves the task.
        let payload = EventPayload::Pan {
            translation_x: 0.0,
            translation_y: 120.0,
            velocity_x: 0.0,
            velocity_y: 300.0,
            value_x: None,
            value_y: Some(120.0),
        };
        assert!(c.component_event(2, EventKind::PanEnd, &payload));
        let batch = c.commit().unwrap();
        let animation = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Animation(nsh_animation::AnimationOp::Start {
                    node, animation, ..
                }) if *node == value => Some(*animation),
                _ => None,
            })
            .expect("a spring");
        assert_eq!(texts(&batch), ["springing"]);
        let mut finished = vec![
            nsh_animation::Finished::new(animation, true),
            // JS's (even ids) stay for JS.
            nsh_animation::Finished::new(8, true),
        ];
        c.animations_finished(&mut finished);
        assert_eq!(finished.len(), 1);
        let _guard = runtime.enter();
        assert_eq!(
            run_until(&mut c, |s| s.contains(&"back".to_owned())),
            ["back"]
        );

        // Released with the component: the value and what derives from it.
        apply(
            &mut c,
            vec![
                Op::Remove {
                    parent: ROOT_ID,
                    child: 2,
                },
                Op::Release { id: 2 },
            ],
        );
        let batch = [c.commit(), c.commit()];
        let released = batch
            .iter()
            .flatten()
            .flat_map(|b| &b.items)
            .filter(|i| {
                matches!(
                    i,
                    MountItem::Animation(nsh_animation::AnimationOp::Release { .. })
                )
            })
            .count();
        assert_eq!(released, 2, "{batch:?}");
    }

    /// Its own rows indexing its data; a press clears the data. Its errors
    /// show as a text.
    #[component]
    fn Clearable() -> impl IntoElement {
        let rows = signal((0..3).map(|i| format!("row {i}")).collect::<Vec<_>>());
        view()
            .flex_1()
            .child(
                view()
                    .h(px(20.0))
                    .on_press(move |_: &()| rows.set(Vec::new())),
            )
            .child(catch_error(
                move || {
                    virtual_list(
                        move || rows.with(Vec::len),
                        move |at| {
                            view()
                                .h(px(40.0))
                                .child(text(move || rows.with(|r| r[at.index()].clone())))
                        },
                    )
                    .estimated_item_size(40.0)
                    .flex_1()
                },
                |e, _| text(format!("failed: {}", e.message)),
            ))
    }

    #[test]
    fn virtual_lists_drop_rows_that_are_gone() {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Clearable>(|| ()));
        let batch = c.commit().unwrap();
        assert!(
            texts(&batch).contains(&"row 2".to_owned()),
            "{:?}",
            texts(&batch)
        );
        let button = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
                _ => None,
            })
            .expect("the button");
        // The cells of rows that are gone render nothing until the list
        // binds them again, never a row past the end.
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        let mut shown = Vec::new();
        for _ in 0..3 {
            if let Some(batch) = c.commit() {
                shown.extend(texts(&batch));
            }
        }
        assert!(shown.iter().all(|t| !t.starts_with("failed")), "{shown:?}");
    }

    /// A chat: its own rows in a list that follows its end; a press makes
    /// the last row taller (a reply streaming in).
    #[component]
    fn Chat() -> impl IntoElement {
        let tall = signal(false);
        view()
            .flex_1()
            .child(
                view()
                    .h(px(20.0))
                    .on_press(move |_: &()| tall.update(|t| *t = !*t)),
            )
            .child(
                virtual_list(20, move |at| {
                    view().h(move || {
                        px(if tall.get() && at.is_last() {
                            120.0
                        } else {
                            40.0
                        })
                    })
                })
                .estimated_item_size(40.0)
                .follow_end(10.0)
                .flex_1(),
            )
    }

    #[test]
    fn virtual_lists_follow_their_end() {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Chat>(|| ()));
        let batch = c.commit().unwrap();
        let created = |kind: ViewKind| {
            batch.items.iter().find_map(|i| match i {
                MountItem::Create { id, kind: k } if *k == kind => Some(*id),
                _ => None,
            })
        };
        let list = created(ViewKind::Scroll).expect("a list");
        let button = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
                _ => None,
            })
            .expect("the button");
        let scrolled = |batch: &MountBatch| -> f32 {
            batch
                .items
                .iter()
                .filter_map(|i| match i {
                    MountItem::ScrollBy { id, dy, .. } if *id == list => Some(*dy),
                    _ => None,
                })
                .sum()
        };
        // At its end (20 rows of 40 in 460 points), the last row grows by
        // 80: the list scrolls with it.
        c.set_scroll_offset(list, 0.0, 800.0 - 460.0, 0);
        assert!(c.commit().is_none(), "every row is bound already");
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        assert_eq!(scrolled(&c.commit().unwrap()), 80.0);
        // Scrolled up, what is visible stays as the last row shrinks and
        // grows again.
        c.set_scroll_offset(list, 0.0, 100.0, 1);
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        assert_eq!(scrolled(&c.commit().unwrap()), 0.0);
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        assert_eq!(scrolled(&c.commit().unwrap()), 0.0);
    }

    thread_local! {
        /// How many times `Opened` was made on this thread.
        static SETUPS: Cell<u32> = const { Cell::new(0) };
    }

    /// A drawer whose content (lazy children) shows while it is open.
    #[derive(Clone, Copy, PartialEq, Props)]
    struct DrawerProps {
        #[prop(default)]
        open: crate::ui::Prop<bool>,
        #[prop(lazy)]
        children: Children,
    }

    #[component]
    fn Drawer(props: DrawerProps) -> impl IntoElement {
        view().child(If::new(props.open, move || {
            view().p_2().child(props.children)
        }))
    }

    /// Content with state of its own: the how-manieth it was made.
    #[component]
    fn Opened() -> impl IntoElement {
        let n = SETUPS.with(|s| {
            s.set(s.get() + 1);
            s.get()
        });
        view().child(text(format!("opened {n}")))
    }

    /// Renders a drawer with content, opened and closed by a press.
    #[component]
    fn Shelf() -> impl IntoElement {
        let open = signal(false);
        view()
            .child(
                view()
                    .h(px(10.0))
                    .on_press(move |_: &()| open.update(|o| *o = !*o)),
            )
            .child(
                Drawer::element(DrawerProps {
                    open: open.into(),
                    children: Children::default(),
                })
                .child(Opened::element(())),
            )
    }

    #[test]
    fn lazy_children_a_component_passes_are_made_while_placed() {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Shelf>(|| ()));
        let batch = c.commit().unwrap();
        let button = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
                _ => None,
            })
            .expect("the button");
        let instances = c.components.instances.len();
        assert_eq!(SETUPS.with(Cell::get), 0, "closed: not made");
        let mut events = Vec::new();
        c.take_events(&mut events);
        assert!(events.is_empty(), "nothing for JS: {events:?}");

        // Opened: made, and placed in the same commit.
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        assert_eq!(texts(&c.commit().unwrap()), ["opened 1"]);
        assert_eq!(c.components.instances.len(), instances + 1);
        // Closed: released.
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        c.commit();
        assert_eq!(c.components.instances.len(), instances);
        // Opened again: made afresh.
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        assert_eq!(texts(&c.commit().unwrap()), ["opened 2"]);
        c.take_events(&mut events);
        assert!(events.is_empty(), "nothing for JS: {events:?}");
    }

    /// Pastes the clipboard's text when pressed, if it holds some.
    #[component]
    fn Paster() -> impl IntoElement {
        let pasted = signal(String::new());
        view()
            .child(text(move || format!("pasted {}", pasted.get())))
            .child(view().h(px(10.0)).on_press(move |_: &()| {
                let (has, text) = (clipboard_has_string(), clipboard_string());
                spawn(async move {
                    let value = if has.await { text.await } else { "-".into() };
                    pasted.set(value);
                });
            }))
    }

    #[test]
    fn components_read_the_clipboard() {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Paster>(|| ()));
        let batch = c.commit().unwrap();
        let button = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
                _ => None,
            })
            .expect("the button");
        assert!(c.component_event(button, EventKind::Press, &EventPayload::None));
        let reads: Vec<_> = c
            .commit()
            .unwrap()
            .items
            .into_iter()
            .filter_map(|i| match i {
                MountItem::Command {
                    id: ROOT_ID,
                    command: Command::ReadClipboard { id, has_only },
                } => Some((id, has_only)),
                _ => None,
            })
            .collect();
        assert_eq!(reads, [(1, true), (2, false)]);
        // The platform answers both on the root.
        for (id, has_only) in reads {
            let mut payload = vec![
                ("id", crate::PayloadValue::Number(f64::from(id))),
                ("has", crate::PayloadValue::Bool(true)),
            ];
            if !has_only {
                payload.push(("text", crate::PayloadValue::Str("hello".into())));
            }
            assert!(c.component_event(ROOT_ID, EventKind::Clipboard, &EventPayload::Map(payload)));
        }
        let shown = run_until(&mut c, |s| s.iter().any(|t| t.starts_with("pasted h")));
        assert_eq!(shown.last().unwrap(), "pasted hello");
    }

    /// Shows a fallback while a component below it failed; a press tries
    /// again.
    #[component]
    fn Boundary(props: WithChildren) -> impl IntoElement {
        let retry = stored(None::<crate::ui::Reset>);
        view()
            .on_press(move |_: &()| {
                if let Some(reset) = retry.replace(None) {
                    reset.reset();
                }
            })
            .child(catch_error(
                move || props.children,
                move |e, reset| {
                    retry.replace(Some(reset));
                    text(format!("oops: {} ({})", e.component, e.message))
                },
            ))
    }

    /// Panics when pressed, and in a binding once `explode` is set.
    #[derive(Clone, Copy, PartialEq, Props)]
    struct FaultyProps {
        #[prop(default)]
        explode: crate::ui::Prop<bool>,
    }

    #[component]
    fn Faulty(props: FaultyProps) -> impl IntoElement {
        view()
            .on_press(|_: &()| panic!("pressed"))
            .child(text(move || {
                assert!(!props.explode.get(), "exploded");
                "fine"
            }))
    }

    #[test]
    fn errors_of_components_go_to_the_nearest_boundary() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Boundary>().register::<Faulty>();
        c.set_components(registry);
        let host = |id, name: &str| {
            vec![
                Op::Create { id, tag: Tag::View },
                Op::SetProp {
                    id,
                    prop: Prop::Component(name.into()),
                },
            ]
        };
        let mut ops = host(2, std::any::type_name::<Boundary>());
        ops.push(Op::Create {
            id: 4,
            tag: Tag::View,
        });
        ops.extend(host(6, std::any::type_name::<Faulty>()));
        for (parent, child) in [(4, 6), (2, 4), (ROOT_ID, 2)] {
            ops.push(Op::Insert {
                parent,
                child,
                before: None,
            });
        }
        apply(&mut c, ops);
        assert_eq!(texts(&c.commit().unwrap()), ["fine"]);

        // A listener panics: the boundary shows its fallback.
        assert!(c.component_event(6, EventKind::Press, &EventPayload::None));
        assert_eq!(
            texts(&c.commit().unwrap()),
            [format!(
                "oops: {} (pressed)",
                std::any::type_name::<Faulty>()
            )]
        );
        // Again: the content is back.
        assert!(c.component_event(2, EventKind::Press, &EventPayload::None));
        c.commit();

        // A binding panics: the same.
        apply(
            &mut c,
            vec![Op::SetProp {
                id: 6,
                prop: Prop::ComponentProp("explode".into(), Some(serde_json::json!(true))),
            }],
        );
        assert_eq!(
            texts(&c.commit().unwrap()),
            [format!(
                "oops: {} (exploded)",
                std::any::type_name::<Faulty>()
            )]
        );
    }

    /// A sentence with a link, a bold span and a badge inside it.
    #[component]
    fn Sentence() -> impl IntoElement {
        let taps = signal(0);
        let long = signal(false);
        view().child(
            text(move || format!("Tapped {} ", taps.get()))
                .text_size(px(16.0))
                .child(text("the link").underline().on_press(move |_: &()| {
                    taps.update(|t| *t += 1);
                    long.set(true);
                }))
                .child(", ")
                .child(text("bold").font_weight(FontWeight::BOLD))
                .child(If::new(long, || " and more"))
                .child(view().size(px(10.0)).bg(rgb(0xff0000))),
        )
    }

    #[test]
    fn rust_texts_nest_spans_links_and_inline_views() {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Sentence>(|| ()));
        let batch = c.commit().unwrap();
        let text = |batch: &MountBatch| {
            batch.items.iter().find_map(|i| match i {
                MountItem::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
        };
        let t = text(&batch).expect("one paragraph");
        assert_eq!(t.text, "Tapped 0 the link, bold\u{fffc}");
        assert_eq!(t.attachments.len(), 1, "the view sits in the text");
        assert!(t.spans.iter().any(|s| s.style.font_weight == 700));
        let link = t.links[0];
        assert_eq!((link.start, link.end), (9, 17));
        // Only the block and the inline view have native views.
        let views = batch
            .items
            .iter()
            .filter(|i| matches!(i, MountItem::Create { .. }))
            .count();
        assert_eq!(views, 3, "the root's host, the text and the view");
        // The link takes the press; the content changes in place.
        assert!(c.component_event(link.node, EventKind::Press, &EventPayload::None));
        let t = text(&c.commit().unwrap()).expect("built again");
        // A piece added between a span and the view goes in its place.
        assert_eq!(t.text, "Tapped 1 the link, bold and more\u{fffc}");
    }
}

/// Rust components' touches and responder (`touch.rs`).
mod touch {
    use super::*;
    use crate::{
        TouchEvent, TouchPhase, TouchPoint,
        ui::{ClaimRequest, ResponderEvent, TouchClaim, prelude::*},
    };

    #[derive(Clone, PartialEq, Props)]
    struct PadProps {
        #[prop(default)]
        keep: bool,
    }

    /// An outer view taking horizontal swipes from an inner one that claims
    /// touches on start.
    #[component]
    fn Pad(props: PadProps) -> impl IntoElement {
        let log = signal(Vec::<String>::new());
        let note = move |what: &'static str, f: fn(&ResponderEvent) -> String| {
            move |e: &ResponderEvent| log.update(|l| l.push(format!("{what}{}", f(e))))
        };
        let none = |_: &ResponderEvent| String::new();
        view()
            .on_touch(move |t: &TouchEvent| log.update(|l| l.push(format!("touch {:?}", t.phase))))
            .claim_touches(TouchClaim::on_move().capture().ask(|r: &ClaimRequest| {
                if r.event.gesture.dx.abs() > 50.0 {
                    r.claim();
                }
            }))
            .on_responder_grant(note("outer grant", none))
            .on_responder_reject(note("outer reject", none))
            .child(
                view()
                    .size(px(100.0))
                    .claim_touches(TouchClaim::on_start())
                    .keep_touches(props.keep)
                    .on_responder_grant(note("grant at ", |e| {
                        format!("{},{}", e.location_x, e.location_y)
                    }))
                    .on_responder_move(note("move ", |e| {
                        format!("{},{}", e.gesture.dx, e.gesture.dy)
                    }))
                    .on_responder_release(note("release ", |e| format!("{:.2}", e.gesture.vx)))
                    .on_responder_terminate(note("terminate", none)),
            )
            .child(text(move || log.with(|l| l.join("|"))))
    }

    fn touch(phase: TouchPhase, path: &[NodeId], x: f32, t: f64) -> EventPayload {
        EventPayload::Touch(Box::new(TouchEvent {
            phase,
            timestamp: t,
            activated: None,
            points: vec![TouchPoint {
                id: 7,
                target: path[0],
                path: if phase == TouchPhase::Start {
                    path.to_vec()
                } else {
                    Vec::new()
                },
                page_x: x,
                page_y: 100.0,
                location_x: x - 10.0,
                location_y: 5.0,
                changed: true,
            }],
        }))
    }

    fn log(batch: &MountBatch) -> Option<String> {
        batch.items.iter().rev().find_map(|i| match i {
            MountItem::Text { text, .. } => Some(text.text.clone()),
            _ => None,
        })
    }

    fn commands(batch: &MountBatch) -> Vec<(NodeId, Command)> {
        batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Command { id, command } => Some((*id, command.clone())),
                _ => None,
            })
            .collect()
    }

    /// The pad mounted: the core, the outer node and the inner one (both
    /// listen to touches).
    fn pad(keep: bool) -> (Core<FakeEngine>, NodeId, NodeId) {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Pad>(move || PadProps { keep }));
        let batch = c.commit().unwrap();
        let listening: Vec<NodeId> = batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Events { id, events } if events.contains(EventKind::Touch) => Some(*id),
                _ => None,
            })
            .collect();
        let [outer, inner] = listening[..] else {
            panic!("two listening views: {listening:?}");
        };
        (c, outer, inner)
    }

    #[test]
    fn an_element_claims_the_touches_on_start_and_blocks_scrolling() {
        let (mut c, outer, inner) = pad(false);
        let path = [inner, outer];
        // The root's touches: the components hear theirs, JS still gets
        // the event for its own nodes.
        assert!(!c.component_event(
            ROOT_ID,
            EventKind::Touch,
            &touch(TouchPhase::Start, &path, 20.0, 0.0)
        ));
        let batch = c.commit().unwrap();
        assert_eq!(log(&batch).unwrap(), "touch Start|grant at 10,5");
        assert_eq!(
            commands(&batch),
            [(inner, Command::SetJsResponder { block: true })]
        );
        c.component_event(
            ROOT_ID,
            EventKind::Touch,
            &touch(TouchPhase::Move, &path, 40.0, 16.0),
        );
        let batch = c.commit().unwrap();
        assert_eq!(
            log(&batch).unwrap(),
            "touch Start|grant at 10,5|touch Move|move 20,0"
        );
        c.component_event(
            ROOT_ID,
            EventKind::Touch,
            &touch(TouchPhase::End, &path, 40.0, 32.0),
        );
        let batch = c.commit().unwrap();
        assert!(log(&batch).unwrap().ends_with("|touch End|release 0.88"));
        assert_eq!(commands(&batch), [(ROOT_ID, Command::ClearJsResponder)]);
    }

    #[test]
    fn a_capturing_claim_takes_the_touches_over_unless_they_are_kept() {
        let (mut c, outer, inner) = pad(false);
        let path = [inner, outer];
        let send = |c: &mut Core<FakeEngine>, phase, x, t| {
            c.component_event(ROOT_ID, EventKind::Touch, &touch(phase, &path, x, t));
        };
        send(&mut c, TouchPhase::Start, 0.0, 0.0);
        send(&mut c, TouchPhase::Move, 30.0, 16.0);
        // Far enough sideways: the outer view asks and claims.
        send(&mut c, TouchPhase::Move, 80.0, 32.0);
        let batch = c.commit().unwrap();
        let text = log(&batch).unwrap();
        assert!(
            text.ends_with("move 30,0|touch Move|terminate|outer grant"),
            "{text}"
        );
        assert_eq!(
            commands(&batch).last(),
            Some(&(outer, Command::SetJsResponder { block: true }))
        );

        // An inner responder that keeps the touches rejects the claim.
        let (mut c, outer, inner) = pad(true);
        let path = [inner, outer];
        let send = |c: &mut Core<FakeEngine>, phase, x, t| {
            c.component_event(ROOT_ID, EventKind::Touch, &touch(phase, &path, x, t));
        };
        send(&mut c, TouchPhase::Start, 0.0, 0.0);
        send(&mut c, TouchPhase::Move, 80.0, 16.0);
        let text = log(&c.commit().unwrap()).unwrap();
        assert!(text.ends_with("|outer reject|move 80,0"), "{text}");
        // A cancel (a native gesture took over) terminates it.
        send(&mut c, TouchPhase::Cancel, 80.0, 32.0);
        let batch = c.commit().unwrap();
        assert!(log(&batch).unwrap().ends_with("|terminate"));
        assert_eq!(commands(&batch), [(ROOT_ID, Command::ClearJsResponder)]);
    }

    #[test]
    fn js_and_component_responders_share_the_native_block() {
        let (mut c, outer, inner) = pad(false);
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
                Op::Command {
                    id: 2,
                    command: Command::SetJsResponder { block: true },
                },
            ],
        );
        assert_eq!(
            commands(&c.commit().unwrap()),
            [(2, Command::SetJsResponder { block: true })]
        );
        // A component's responder blocks around itself while it has the
        // touches; when it lets go, JS's block is back.
        c.component_event(
            ROOT_ID,
            EventKind::Touch,
            &touch(TouchPhase::Start, &[inner, outer], 0.0, 0.0),
        );
        assert_eq!(
            commands(&c.commit().unwrap()),
            [(inner, Command::SetJsResponder { block: true })]
        );
        c.component_event(
            ROOT_ID,
            EventKind::Touch,
            &touch(TouchPhase::End, &[inner, outer], 0.0, 16.0),
        );
        assert_eq!(
            commands(&c.commit().unwrap()),
            [(2, Command::SetJsResponder { block: true })]
        );
        apply(
            &mut c,
            vec![Op::Command {
                id: ROOT_ID,
                command: Command::ClearJsResponder,
            }],
        );
        assert_eq!(
            commands(&c.commit().unwrap()),
            [(ROOT_ID, Command::ClearJsResponder)]
        );
    }

    /// A view scrubbed sideways: it waits for the platform's pan (its
    /// start claim is not asked meanwhile).
    #[component]
    fn Scrub() -> impl IntoElement {
        let log = signal(Vec::<String>::new());
        view()
            .child(
                view()
                    .size(px(100.0))
                    .claim_touches(TouchClaim::on_start())
                    .claim_touches(TouchClaim::on_pan(crate::PanActivation::horizontal(
                        10.0, 10.0,
                    )))
                    .on_responder_grant(move |_: &ResponderEvent| {
                        log.update(|l| l.push("grant".into()));
                    }),
            )
            .child(text(move || log.with(|l| l.join("|"))))
    }

    #[test]
    fn a_pan_claim_waits_for_the_platform_to_activate_it() {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Scrub>(|| ()));
        let batch = c.commit().unwrap();
        let (id, activation) = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::ViewProps { id, props } => props.pan_activation.map(|a| (*id, a)),
                _ => None,
            })
            .expect("the activation is mounted");
        assert_eq!(activation, crate::PanActivation::horizontal(10.0, 10.0));
        c.component_event(
            ROOT_ID,
            EventKind::Touch,
            &touch(TouchPhase::Start, &[id], 0.0, 0.0),
        );
        c.component_event(
            ROOT_ID,
            EventKind::Touch,
            &touch(TouchPhase::Move, &[id], 5.0, 16.0),
        );
        // Nothing to mount: no claim before the activation.
        assert!(c.commit().is_none_or(|b| commands(&b).is_empty()));
        let mut activated = touch(TouchPhase::Move, &[id], 14.0, 32.0);
        if let EventPayload::Touch(t) = &mut activated {
            t.activated = Some(id);
        }
        c.component_event(ROOT_ID, EventKind::Touch, &activated);
        let batch = c.commit().unwrap();
        assert_eq!(log(&batch), Some("grant".into()), "{batch:?}");
        assert_eq!(
            commands(&batch),
            [(id, Command::SetJsResponder { block: true })]
        );
    }

    #[test]
    fn a_pan_activation_decides_by_axis() {
        let pan = crate::PanActivation::horizontal(10.0, 8.0);
        assert_eq!(pan.decide(4.0, 3.0), None);
        assert_eq!(pan.decide(-11.0, 3.0), Some(true));
        assert_eq!(pan.decide(4.0, -9.0), Some(false));
        // Both passed by one move: the fail offset wins.
        assert_eq!(pan.decide(20.0, 9.0), Some(false));
        let any = crate::PanActivation::default();
        assert_eq!(any.decide(6.0, 6.0), None);
        assert_eq!(any.decide(8.0, 8.0), Some(true));
    }

    /// A card scaled and turned by two-finger gestures.
    #[component]
    fn Card() -> impl IntoElement {
        let (scale, angle) = (native_value(1.0), native_value(0.0));
        view()
            .pinch_value(&scale)
            .rotation_value(&angle)
            .bind_scale(&scale)
            .bind_rotate(&angle)
    }

    #[test]
    fn pinches_and_rotations_drive_native_values() {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Card>(|| ()));
        let batch = c.commit().unwrap();
        let values = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Gestures { values, .. } => Some(*values),
                _ => None,
            })
            .expect("gesture values");
        let created: Vec<_> = batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Animation(AnimationOp::CreateValue { node, .. }) => Some(*node),
                _ => None,
            })
            .collect();
        assert_eq!(values.pinch, Some(created[0]));
        assert_eq!(values.rotation, Some(created[1]));
    }
}

/// Rust components' drawings (`ui::Painter`).
mod painter {
    use super::*;
    use crate::{
        canvas::{CanvasGradient, CanvasOp, FillRule, GradientShape, Num, Paint, svg_path},
        ui::prelude::*,
    };

    #[test]
    fn painters_hit_test_what_they_drew() {
        // A slice drawn moved by 10, a line along the top, a rectangle as
        // the current path.
        let slice = svg_path("M0 0 L40 0 L0 40 Z");
        let p = Painter::new()
            .save()
            .translate(10.0, 10.0)
            .fill_path(&slice, FillRule::NonZero)
            .restore()
            .line_width(4.0)
            .stroke_svg("M0 2 L100 2")
            .begin_path()
            .rect(60.0, 60.0, 20.0, 20.0);
        // Paths given are tested with the transform at the end: drawn
        // translated, the slice is hit where it shows only with the same
        // transform recorded.
        assert!(p.path_contains(&slice, 5.0, 5.0, FillRule::NonZero));
        assert!(!p.path_contains(&slice, 45.0, 45.0, FillRule::NonZero));
        let moved = Painter::new().translate(10.0, 10.0);
        assert!(moved.path_contains(&slice, 15.0, 15.0, FillRule::NonZero));
        assert!(!moved.path_contains(&slice, 5.0, 5.0, FillRule::NonZero));
        // The current path, and strokes with the line width.
        assert!(p.is_point_in_path(70.0, 70.0, FillRule::EvenOdd));
        assert!(!p.is_point_in_path(50.0, 70.0, FillRule::NonZero));
        assert!(p.is_point_in_stroke(60.0, 70.0));
        assert!(!p.is_point_in_stroke(70.0, 70.0));
        let top = svg_path("M0 2 L100 2");
        assert!(p.stroke_contains(&top, 50.0, 3.5));
        assert!(!p.stroke_contains(&top, 50.0, 5.0));
    }

    /// A label placed by its measured width, gradients, a stroked SVG path
    /// and an operation built by hand reading a native value.
    #[component]
    fn Label() -> impl IntoElement {
        let offset = native_value(3.0);
        let width = crate::ui::prelude::measure_text("600 20px system-ui", "100%");
        let mut p = Painter::new()
            .stroke_style(
                CanvasGradient::conic(0.0, 50.0, 50.0)
                    .add_color_stop(1.0, rgb(0x6366f1))
                    .add_color_stop(0.0, rgb(0x22d3ee)),
            )
            .stroke_path(&svg_path("M20 6L9 17l-5-5"))
            .fill_style(CanvasGradient::radial(0.0, 0.0, 4.0, 0.0, 0.0, 40.0))
            .fill_path(&svg_path("M0 0h10v10z"), FillRule::EvenOdd)
            .fill_text("100%", 50.0 - width, 20.0);
        let x = p.num(&offset);
        p = p.op(CanvasOp::LineDashOffset(x));
        view().child(canvas(p))
    }

    #[test]
    fn rust_drawings_measure_text_and_use_every_paint() {
        let mut c = core();
        c.mount_root(&crate::ui::Root::new::<Label>(|| ()));
        let batch = c.commit().unwrap();
        let drawing = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Canvas { drawing, .. } => Some(drawing.clone()),
                _ => None,
            })
            .expect("a drawing");
        let ops = &drawing.ops;
        // Stops sorted by offset, whatever order they came in.
        let CanvasOp::StrokeStyle(Paint::Gradient(ring)) = &ops[0] else {
            panic!("{:?}", ops[0])
        };
        assert!(matches!(
            ring.shape,
            GradientShape::Conic([0.0, 50.0, 50.0])
        ));
        assert_eq!(ring.stops[0], (0.0, rgb(0x22d3ee)));
        assert!(matches!(&ops[1], CanvasOp::Stroke(Some(path)) if path.len() == 3));
        assert!(matches!(
            &ops[2],
            CanvasOp::FillStyle(Paint::Gradient(g)) if matches!(g.shape, GradientShape::Radial(_))
        ));
        assert!(matches!(
            &ops[3],
            CanvasOp::Fill(Some(_), FillRule::EvenOdd)
        ));
        // 8pt per character in the fake engine.
        assert!(matches!(
            &ops[4],
            CanvasOp::FillText {
                x: Num::Const(18.0),
                ..
            }
        ));
        // The native value is a slot of the drawing.
        assert_eq!(ops[5], CanvasOp::LineDashOffset(Num::Slot(0)));
        assert_eq!(drawing.slots.len(), 1);
    }
}

/// A row (2) holding a field (3) whose absolutely placed menu (4) hangs
/// below it, inside a card (5): row and card get the menu's area, the
/// field does not reach out of the card, a clipping card stops it.
#[test]
fn overflowing_children_mount_their_area() {
    let mut c = core();
    let create = |id| Op::Create { id, tag: Tag::View };
    let insert = |parent, child| Op::Insert {
        parent,
        child,
        before: None,
    };
    apply(
        &mut c,
        vec![
            create(5),
            create(2),
            create(4),
            style_op(5, &[("padding", StyleValue::Number(10.0))]),
            style_op(2, &[("height", StyleValue::Number(40.0))]),
            style_op(
                4,
                &[
                    ("position", StyleValue::Str("absolute")),
                    ("top", StyleValue::Number(40.0)),
                    ("left", StyleValue::Number(0.0)),
                    ("width", StyleValue::Number(100.0)),
                    ("height", StyleValue::Number(120.0)),
                ],
            ),
            insert(2, 4),
            insert(5, 2),
            insert(ROOT_ID, 5),
        ],
    );
    let batch = c.commit().unwrap();
    let overflow = |batch: &MountBatch, id: NodeId| {
        batch.items.iter().rev().find_map(|i| match i {
            MountItem::Overflow { id: i, extent } if *i == id => Some(*extent),
            _ => None,
        })
    };
    // The row is 300 wide (the card's padding), 40 tall: the menu reaches
    // 120 below it.
    assert_eq!(
        overflow(&batch, 2),
        Some(Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 160.0,
        }))
    );
    // In the card (60 tall: 40 + padding) at (10, 10): down to 170.
    assert_eq!(
        overflow(&batch, 5),
        Some(Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 170.0,
        }))
    );
    // Leaves have nothing to report, and nothing is sent again unchanged.
    assert_eq!(overflow(&batch, 4), None);
    c.apply(style_op(2, &[("height", StyleValue::Number(40.0))]))
        .unwrap();
    if let Some(batch) = c.commit() {
        assert_eq!(overflow(&batch, 2), None);
    }
    // A clipping card has no area (what is outside is not drawn), and the
    // menu inside the row now fits: the row's area goes away.
    c.apply(style_op(
        5,
        &[
            ("padding", StyleValue::Number(10.0)),
            ("overflow", StyleValue::Str("hidden")),
        ],
    ))
    .unwrap();
    c.apply(style_op(2, &[("height", StyleValue::Number(200.0))]))
        .unwrap();
    let batch = c.commit().unwrap();
    assert_eq!(overflow(&batch, 5), Some(None));
    assert_eq!(overflow(&batch, 2), Some(None));
}

/// A view that loses its only, absolutely placed child lays out again:
/// taffy keeps the children it hoisted out of flow while the view has no
/// children to lay out, so the freed child must not stay listed there.
#[test]
fn a_view_that_loses_its_absolute_child_lays_out() {
    let mut c = core();
    for (id, absolute) in [(2, false), (4, true), (6, false)] {
        c.apply(Op::Create { id, tag: Tag::View }).unwrap();
        let mut props = vec![("height", StyleValue::Number(20.0))];
        if absolute {
            props.push(("position", StyleValue::Str("absolute")));
        }
        c.apply(style_op(id, &props)).unwrap();
    }
    for (parent, child) in [(2, 4), (ROOT_ID, 2), (ROOT_ID, 6)] {
        c.apply(Op::Insert {
            parent,
            child,
            before: None,
        })
        .unwrap();
    }
    c.commit().unwrap();
    c.apply(Op::Release { id: 4 }).unwrap();
    // A new node takes the freed one's slot.
    c.apply(Op::Create {
        id: 8,
        tag: Tag::View,
    })
    .unwrap();
    c.apply(Op::Insert {
        parent: 6,
        child: 8,
        before: None,
    })
    .unwrap();
    c.apply(style_op(2, &[("height", StyleValue::Number(30.0))]))
        .unwrap();
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::Frame { id: 6, frame, .. } if frame.y == 30.0
    )));
}

/// `measure` gives the frame in the parent and the position in the root
/// view, minus the offsets of the scroll views around.
#[test]
fn measures_in_root_coordinates() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Scroll,
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::Create {
                id: 6,
                tag: Tag::View,
            },
            style_op(
                2,
                &[
                    ("marginTop", StyleValue::Number(50.0)),
                    ("height", StyleValue::Number(300.0)),
                ],
            ),
            style_op(
                4,
                &[
                    ("padding", StyleValue::Number(20.0)),
                    ("height", StyleValue::Number(1000.0)),
                ],
            ),
            style_op(
                6,
                &[
                    ("marginTop", StyleValue::Number(500.0)),
                    ("height", StyleValue::Number(30.0)),
                ],
            ),
            Op::Insert {
                parent: 4,
                child: 6,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit().unwrap();
    let m = c.measure(6).unwrap();
    assert_eq!(
        m.frame,
        Rect {
            x: 20.0,
            y: 520.0,
            width: 280.0,
            height: 30.0,
        }
    );
    assert_eq!((m.page_x, m.page_y), (20.0, 570.0));
    c.set_scroll_offset(2, 0.0, 400.0, u32::MAX);
    let m = c.measure(6).unwrap();
    assert_eq!((m.page_x, m.page_y), (20.0, 170.0));
    // Detached or unknown: nothing to measure.
    c.apply(Op::Remove {
        parent: 4,
        child: 6,
    })
    .unwrap();
    c.commit();
    assert_eq!(c.measure(6), None);
    assert_eq!(c.measure(1000), None);
}

/// `desktop={{ windowDrag }}` reaches the platform in the view's props, and
/// clearing it sends them again.
#[test]
fn window_drag_is_a_view_prop() {
    let mut c = core();
    let drag = |value| Op::SetProp {
        id: 2,
        prop: Prop::WindowDrag(value),
    };
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            drag(Some(true)),
            Op::Insert {
                parent: crate::ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(
        |i| matches!(i, MountItem::ViewProps { id: 2, props } if props.window_drag == Some(true))
    ));
    apply(&mut c, vec![drag(None)]);
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(
        |i| matches!(i, MountItem::ViewProps { id: 2, props } if props.window_drag.is_none())
    ));
}

/// Components that run once: their views mounted with bindings.
mod views {
    use std::{cell::Cell, rc::Rc};

    use serde_json::json;

    use super::*;
    use crate::{
        Prop,
        ui::{Component, Each, If, Props, Registry, Root, Scope, prelude::*},
    };

    fn texts(batch: &MountBatch) -> Vec<String> {
        batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Text { text, .. } => Some(text.text.clone()),
                _ => None,
            })
            .collect()
    }

    fn root_host(batch: &MountBatch) -> NodeId {
        batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Insert {
                    parent: ROOT_ID,
                    child,
                    ..
                } => Some(*child),
                _ => None,
            })
            .expect("the root's host is in the root view")
    }

    /// The node showing text `content`.
    fn text_node(batch: &MountBatch, content: &str) -> NodeId {
        batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Text { id, text, .. } if text.text == content => Some(*id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no text {content:?}"))
    }

    /// Counts presses: the text is a binding, nothing renders again.
    #[derive(Default)]
    struct Counter;

    #[derive(Clone, PartialEq, Props)]
    struct CounterProps {
        #[prop(default = "Taps".into())]
        label: crate::ui::Prop<SharedString>,
    }

    #[derive(Event)]
    enum CounterEvent {
        Tap(u32),
    }

    impl Component for Counter {
        type Props = CounterProps;
        type Event = CounterEvent;

        fn view(props: CounterProps, cx: Scope<CounterEvent>) -> impl IntoElement {
            let count = signal(0);
            view()
                .p_2()
                .on_press(move |_: &()| {
                    count.update(|n| *n += 1);
                    cx.emit(CounterEvent::Tap(count.get_untracked()));
                })
                .child(text(move || {
                    format!("{} {}", props.label.get(), count.get())
                }))
                .child(text("static"))
        }
    }

    #[test]
    fn a_view_runs_once_and_its_bindings_send_only_what_changed() {
        let mut c = core();
        c.mount_root(&Root::new::<Counter>(|| CounterProps {
            label: "Hits".into(),
        }));
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), ["Hits 0", "static"]);
        let host = root_host(&batch);
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), ["Hits 1"], "one text changed");
        assert!(
            !batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Create { .. } | MountItem::Delete { .. })),
            "no node made or freed: {:?}",
            batch.items
        );
    }

    #[test]
    fn a_prop_from_js_reaches_only_what_reads_it() {
        let mut c = core();
        let mut registry = Registry::new();
        registry.register::<Counter>();
        c.set_components(registry);
        apply(
            &mut c,
            vec![
                Op::Create {
                    id: 2,
                    tag: Tag::View,
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::Component(std::any::type_name::<Counter>().into()),
                },
                Op::SetProp {
                    id: 2,
                    prop: Prop::ComponentProp("label".into(), Some(json!("Hits"))),
                },
                Op::Insert {
                    parent: ROOT_ID,
                    child: 2,
                    before: None,
                },
            ],
        );
        assert_eq!(texts(&c.commit().unwrap()), ["Hits 0", "static"]);
        c.apply(Op::SetProp {
            id: 2,
            prop: Prop::ComponentProp("label".into(), Some(json!("Runs"))),
        })
        .unwrap();
        assert_eq!(texts(&c.commit().unwrap()), ["Runs 0"]);
        // Its events reach JS.
        assert!(c.component_event(2, EventKind::Press, &EventPayload::None));
        let mut events = Vec::new();
        c.commit();
        c.take_events(&mut events);
        assert!(events.iter().any(|(id, kind, payload)| *id == 2
            && *kind == EventKind::NativeEvent
            && matches!(payload, EventPayload::Native { name, data } if &**name == "tap" && *data == json!(1))));
    }

    /// Shows one branch or the other.
    #[derive(Default)]
    struct Toggle;

    #[derive(Clone, PartialEq, Props)]
    struct ToggleProps {}

    thread_local! {
        static CLEANED: Cell<u32> = const { Cell::new(0) };
    }

    impl Component for Toggle {
        type Props = ToggleProps;
        type Event = NoEvent;

        fn view(_: ToggleProps, _: Scope<NoEvent>) -> impl IntoElement {
            let on = signal(false);
            view()
                .on_press(move |_: &()| on.update(|v| *v = !*v))
                .child(text("before"))
                .child(
                    If::new(on, || {
                        cleanup(|| CLEANED.with(|c| c.set(c.get() + 1)));
                        text("on")
                    })
                    .otherwise(|| text("off")),
                )
                .child(text("after"))
        }
    }

    /// The texts under `parent`, in order (by the inserts of `batch` and
    /// those before).
    fn order(c: &Core<FakeEngine>, parent: NodeId) -> Vec<String> {
        let key = c.key(parent).unwrap();
        c.nodes[key as usize]
            .children
            .iter()
            .filter_map(|&k| {
                let node = &c.nodes[k as usize];
                node.children
                    .first()
                    .map(|&raw| c.nodes[raw as usize].text.clone())
            })
            .collect()
    }

    #[test]
    fn if_makes_the_branch_it_flips_to_in_place_and_drops_the_other() {
        let mut c = core();
        c.mount_root(&Root::new::<Toggle>(|| ToggleProps {}));
        let batch = c.commit().unwrap();
        let host = root_host(&batch);
        assert_eq!(order(&c, host), ["before", "off", "after"]);
        c.component_event(host, EventKind::Press, &EventPayload::None);
        c.commit();
        assert_eq!(order(&c, host), ["before", "on", "after"]);
        c.component_event(host, EventKind::Press, &EventPayload::None);
        c.commit();
        assert_eq!(order(&c, host), ["before", "off", "after"]);
        assert_eq!(CLEANED.with(Cell::get), 1, "the branch's cleanup ran");
    }

    /// A list of rows by key.
    #[derive(Default)]
    struct Rows;

    #[derive(Clone, PartialEq, Props)]
    struct RowsProps {}

    thread_local! {
        static MADE: Cell<u32> = const { Cell::new(0) };
        static ITEMS: Cell<Option<Signal<Vec<u32>>>> = const { Cell::new(None) };
    }

    impl Component for Rows {
        type Props = RowsProps;
        type Event = NoEvent;

        fn view(_: RowsProps, _: Scope<NoEvent>) -> impl IntoElement {
            let items = signal(vec![1, 2, 3]);
            ITEMS.with(|i| i.set(Some(items)));
            view().child(Each::new(
                items,
                |n| *n,
                |item, index| {
                    MADE.with(|m| m.set(m.get() + 1));
                    text(move || format!("{}@{}", item.get(), index.get()))
                },
            ))
        }
    }

    #[test]
    fn each_keeps_the_rows_that_stay_and_makes_only_new_ones() {
        let mut c = core();
        c.mount_root(&Root::new::<Rows>(|| RowsProps {}));
        let batch = c.commit().unwrap();
        // The rows are the root view's children: the host's.
        let list = root_host(&batch);
        assert_eq!(order(&c, list), ["1@0", "2@1", "3@2"]);
        let one = text_node(&batch, "1@0");
        let items = ITEMS.with(Cell::get).unwrap();
        {
            let _entered = c.components.runtime.enter();
            items.set(vec![3, 1, 4]);
        }
        let batch = c.commit().unwrap();
        assert_eq!(order(&c, list), ["3@0", "1@1", "4@2"]);
        assert_eq!(MADE.with(Cell::get), 4, "one new row");
        assert!(
            !batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Create { id, .. } if *id == one)),
            "row 1 kept its node"
        );
    }

    /// Made with `#[component]`: reads the appearance and a timer.
    #[component]
    fn Clock(_props: (), cx: Scope<NoEvent>) -> impl IntoElement {
        let ticks = signal(0);
        set_timeout(std::time::Duration::ZERO, move || ticks.set(1));
        let _ = cx.insets();
        view().child(text(move || {
            format!(
                "{} {} {}",
                if dark() { "dark" } else { "light" },
                ticks.get(),
                theme().radius
            )
        }))
    }

    #[test]
    fn a_function_component_reads_the_environment_and_its_timers_run() {
        let mut c = core();
        c.mount_root(&Root::new::<Clock>(|| ()));
        let batch = c.commit().unwrap();
        let shown = texts(&batch);
        assert!(shown[0].starts_with("light 0 "), "{shown:?}");
        c.set_environment(crate::ui::Environment {
            dark: true,
            ..Default::default()
        });
        let batch = c.commit().unwrap();
        assert!(
            texts(&batch)[0].starts_with("dark 0 "),
            "{:?}",
            texts(&batch)
        );
    }

    thread_local! {
        static INTROS: Cell<u32> = const { Cell::new(0) };
    }

    #[derive(Clone, PartialEq, Props)]
    struct IntroProps {
        label: SharedString,
    }

    /// Counts its views: what a component does as it is made (an entrance)
    /// happens each time.
    #[component]
    fn Intro(props: IntroProps) -> impl IntoElement {
        INTROS.with(|n| n.set(n.get() + 1));
        text(props.label)
    }

    /// Shows an `Intro` for the tab chosen; a press switches tabs.
    #[component]
    fn Tabs() -> impl IntoElement {
        let first = signal(true);
        view()
            .on_press(move |_: &()| first.update(|f| *f = !*f))
            .child(dynamic(move || {
                Intro::element(IntroProps {
                    label: if first.get() { "one" } else { "two" }.into(),
                })
            }))
    }

    #[test]
    fn a_component_a_region_makes_again_runs_again() {
        INTROS.with(|n| n.set(0));
        let mut c = core();
        c.mount_root(&Root::new::<Tabs>(|| ()));
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), ["one"]);
        let host = root_host(&batch);
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        assert_eq!(texts(&c.commit().unwrap()), ["two"]);
        assert_eq!(INTROS.with(Cell::get), 2);
    }

    /// Shows "late" 10 ms after it is made.
    #[component]
    fn Later() -> impl IntoElement {
        let shown = signal("soon");
        set_timeout(std::time::Duration::from_millis(10), move || {
            shown.set("late")
        });
        text(move || shown.get())
    }

    #[test]
    fn the_core_keeps_its_tasks_timers() {
        // No tokio runtime: the core fires its tasks' timers itself, and
        // tells the engine when to wake for the next.
        let mut c = core();
        c.mount_root(&Root::new::<Later>(|| ()));
        assert_eq!(texts(&c.commit().unwrap()), ["soon"]);
        c.run_component_tasks();
        let due = c.next_task_timer().expect("a timer waits");
        assert!(due > std::time::Instant::now());
        std::thread::sleep(due.saturating_duration_since(std::time::Instant::now()));
        c.run_component_tasks();
        assert_eq!(texts(&c.commit().unwrap()), ["late"]);
        assert_eq!(c.next_task_timer(), None);
    }

    /// A box bordered while a signal holds.
    #[component]
    fn Framed() -> impl IntoElement {
        let on = signal(false);
        view()
            .size_10()
            .on_press(move |_: &()| on.set(true))
            .style_when(on, |s| s.border_1().border_color(rgb(0xff0000)))
    }

    #[test]
    fn a_bound_style_layer_draws_its_border() {
        let mut c = core();
        c.mount_root(&Root::new::<Framed>(|| ()));
        let host = root_host(&c.commit().unwrap());
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        let style = batch.items.iter().find_map(|i| match i {
            MountItem::ViewStyle { style, .. } if style.borders.widths[0] > 0.0 => {
                Some(style.clone())
            }
            _ => None,
        });
        assert!(style.is_some(), "{:#?}", batch.items);
    }

    /// A track whose color moves between two theme colors natively.
    #[component]
    fn Track() -> impl IntoElement {
        let on = native_value(0.0);
        let off_color = AdaptiveColor {
            light: rgb(0xeeeeee),
            dark: rgb(0x222222),
        };
        view().size_10().bind_background(&on.interpolate_colors(
            &[0.0, 1.0],
            &[off_color, AdaptiveColor::from(rgb(0x00ff00))],
        ))
    }

    #[test]
    fn interpolated_theme_colors_follow_the_appearance() {
        let colors = |batch: &MountBatch| {
            batch.items.iter().find_map(|i| match i {
                MountItem::Animation(
                    nsh_animation::AnimationOp::CreateInterpolate { output, .. }
                    | nsh_animation::AnimationOp::SetOutput { output, .. },
                ) => Some(output.clone()),
                _ => None,
            })
        };
        let mut c = core();
        c.mount_root(&Root::new::<Track>(|| ()));
        assert_eq!(
            colors(&c.commit().unwrap()),
            Some(nsh_animation::Output::Colors(vec![0xeeeeeeff, 0x00ff00ff]))
        );
        c.set_environment(crate::ui::Environment {
            dark: true,
            ..Default::default()
        });
        // The same node, the dark colors: no view is made again.
        let batch = c.commit().unwrap();
        assert_eq!(
            colors(&batch),
            Some(nsh_animation::Output::Colors(vec![0x222222ff, 0x00ff00ff]))
        );
        assert!(!batch.items.iter().any(|i| matches!(
            i,
            MountItem::Animation(nsh_animation::AnimationOp::CreateInterpolate { .. })
        )));
    }

    /// A style bound to a signal.
    #[derive(Default)]
    struct Lamp;

    #[derive(Clone, PartialEq, Props)]
    struct LampProps {}

    impl Component for Lamp {
        type Props = LampProps;
        type Event = NoEvent;

        fn view(_: LampProps, _: Scope<NoEvent>) -> impl IntoElement {
            let on = signal(false);
            view()
                .on_press(move |_: &()| on.update(|v| *v = !*v))
                .w(px(10.0))
                .bg(move || {
                    if on.get() {
                        rgb(0xff0000)
                    } else {
                        rgb(0x0000ff)
                    }
                })
                .style_when(on, |s| s.opacity(0.5))
        }
    }

    #[test]
    fn bound_styles_change_alone() {
        let mut c = core();
        c.mount_root(&Root::new::<Lamp>(|| LampProps {}));
        let batch = c.commit().unwrap();
        let host = root_host(&batch);
        let style = view_style_of(&batch, host).unwrap();
        assert_eq!(style.background_color, crate::style::Color(0x0000_ffff));
        assert_eq!(style.opacity, 1.0);
        c.component_event(host, EventKind::Press, &EventPayload::None);
        let batch = c.commit().unwrap();
        let style = view_style_of(&batch, host).unwrap();
        assert_eq!(style.background_color, crate::style::Color(0xff00_00ff));
        assert_eq!(style.opacity, 0.5);
        let _ = Rc::new(());
    }

    /// Its own rows in a list, from a signal: a press on a row removes it,
    /// one on "add" appends one. A row's text says when it is the last.
    #[derive(Default)]
    struct Mailbox;

    #[derive(Clone, PartialEq, Props)]
    struct MailboxProps {
        #[prop(default)]
        rows: crate::ui::Prop<u32>,
    }

    impl Component for Mailbox {
        type Props = MailboxProps;
        type Event = NoEvent;

        fn view(props: MailboxProps, _: Scope<NoEvent>) -> impl IntoElement {
            let rows = signal(
                (0..props.rows.get_untracked())
                    .map(|i| format!("mail {i}"))
                    .collect::<Vec<_>>(),
            );
            view()
                .flex_1()
                .child(view().h(px(20.0)).label("add").on_press(move |_: &()| {
                    rows.update(|r| {
                        let n = r.len();
                        r.push(format!("new {n}"));
                    });
                }))
                .child(
                    virtual_list(
                        move || rows.with(Vec::len),
                        move |at| {
                            view()
                                .h(px(40.0))
                                .on_press(move |_: &()| {
                                    rows.update(|r| {
                                        r.remove(at.index());
                                    });
                                })
                                // Indexing: a row's bindings never read
                                // past the rows.
                                .child(text(move || {
                                    let row = rows.with(|r| r[at.index()].clone());
                                    let last = if at.is_last() { " (last)" } else { "" };
                                    format!("{row}{last}")
                                }))
                        },
                    )
                    .estimated_item_size(40.0)
                    .keys(move || rows.get())
                    .flex_1(),
                )
        }
    }

    fn mailbox(rows: u32) -> (Core<FakeEngine>, MountBatch) {
        let mut c = core();
        c.mount_root(&Root::new::<Mailbox>(move || MailboxProps {
            rows: rows.into(),
        }));
        let batch = c.commit().unwrap();
        (c, batch)
    }

    fn pressable(batch: &MountBatch) -> Vec<NodeId> {
        batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn creates(batch: &MountBatch) -> usize {
        batch
            .items
            .iter()
            .filter(|i| matches!(i, MountItem::Create { .. }))
            .count()
    }

    #[test]
    fn a_views_list_splices_and_its_cells_keep_their_rows() {
        let (mut c, batch) = mailbox(100);
        let shown = texts(&batch);
        assert!(shown.contains(&"mail 0".to_owned()), "{shown:?}");
        assert!(shown.len() < 100, "virtualized: {}", shown.len());
        // A press on row 0 removes it: the list splices, the other cells
        // keep their content (their rows did not change: nothing to send)
        // and the freed cell shows the row coming into view.
        let row0 = pressable(&batch)[1];
        assert!(c.component_event(row0, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), [format!("mail {}", shown.len())]);
        assert_eq!(creates(&batch), 0, "{:?}", batch.items);
    }

    #[test]
    fn a_views_list_grows_in_place() {
        let (mut c, batch) = mailbox(3);
        let mut shown = texts(&batch);
        shown.sort();
        assert_eq!(shown, ["mail 0", "mail 1", "mail 2 (last)"]);
        let add = pressable(&batch)[0];
        assert!(c.component_event(add, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        // The old last row's text changes; a cell comes for the new one.
        let mut shown = texts(&batch);
        shown.sort();
        assert_eq!(shown, ["mail 2", "new 3 (last)"]);
    }

    #[test]
    fn a_views_list_cells_show_other_rows_through_their_bindings() {
        let (mut c, batch) = mailbox(1000);
        let list = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Create {
                    id,
                    kind: crate::ViewKind::Scroll,
                } => Some(*id),
                _ => None,
            })
            .expect("a list");
        c.set_scroll_offset(list, 0.0, 10_000.0, u32::MAX);
        c.commit().unwrap();
        // Scrolling on: the cells are reused and their content stays; only
        // the texts change.
        c.set_scroll_offset(list, 0.0, 20_000.0, u32::MAX);
        let batch = c.commit().unwrap();
        assert_eq!(creates(&batch), 0, "{:?}", batch.items);
        let shown = texts(&batch);
        assert!(shown.contains(&"mail 500".to_owned()), "{shown:?}");
    }

    #[test]
    fn a_views_list_drops_the_rows_that_went_past_its_end() {
        let (mut c, batch) = mailbox(3);
        // The row of the last text.
        let text = text_node(&batch, "mail 2 (last)");
        let last = batch
            .items
            .iter()
            .find_map(|i| match i {
                MountItem::Insert { parent, child, .. } if *child == text => Some(*parent),
                _ => None,
            })
            .expect("the last row");
        assert!(c.component_event(last, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), ["mail 1 (last)"]);
        assert!(
            batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Delete { .. })),
            "the gone row's nodes: {:?}",
            batch.items
        );
    }

    /// A list whose top padding is a signal; a press on it flips it.
    #[derive(Default)]
    struct Padded;

    #[derive(Clone, PartialEq, Props)]
    struct PaddedProps {}

    impl Component for Padded {
        type Props = PaddedProps;
        type Event = NoEvent;

        fn view(_: PaddedProps, _: Scope<NoEvent>) -> impl IntoElement {
            let padded = signal(false);
            view()
                .flex_1()
                .on_press(move |_: &()| padded.update(|p| *p = !*p))
                .child(
                    virtual_list(3, |at| text(format!("row {}", at.index())).h(px(40.0)))
                        .content_padding_top(move || if padded.get() { 10.0_f32 } else { 0.0 })
                        .flex_1(),
                )
        }
    }

    #[test]
    fn a_views_list_layout_is_a_binding() {
        let mut c = core();
        c.mount_root(&Root::new::<Padded>(|| PaddedProps {}));
        let batch = c.commit().unwrap();
        let host = root_host(&batch);
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        // The rows move down; nothing is made again.
        assert_eq!(creates(&batch), 0, "{:?}", batch.items);
        let moved: Vec<f32> = batch
            .items
            .iter()
            .filter_map(|i| match i {
                MountItem::Frame { frame, .. } => Some(frame.y),
                _ => None,
            })
            .collect();
        assert!(moved.contains(&10.0), "{:?}", batch.items);
    }

    thread_local! {
        /// How many times `Fresh` was made on this thread.
        static FRESH: Cell<u32> = const { Cell::new(0) };
    }

    /// Content with a state of its own: the how-manieth it was made.
    #[derive(Default)]
    struct Fresh;

    #[derive(Clone, PartialEq, Props)]
    struct FreshProps {}

    impl Component for Fresh {
        type Props = FreshProps;
        type Event = NoEvent;

        fn view(_: FreshProps, _: Scope<NoEvent>) -> impl IntoElement {
            let n = FRESH.with(|f| {
                f.set(f.get() + 1);
                f.get()
            });
            view().child(text(format!("fresh {n}")))
        }
    }

    /// Places its (lazy) children while open.
    #[derive(Default)]
    struct Pocket;

    #[derive(Clone, PartialEq, Props)]
    struct PocketProps {
        #[prop(default)]
        open: crate::ui::Prop<bool>,
        #[prop(lazy)]
        children: Children,
    }

    impl Component for Pocket {
        type Props = PocketProps;
        type Event = NoEvent;

        fn view(props: PocketProps, _: Scope<NoEvent>) -> impl IntoElement {
            let children = props.children;
            view().child(If::new(props.open, move || view().p_2().child(children)))
        }
    }

    /// Places its children.
    #[derive(Default)]
    struct Holder;

    #[derive(Clone, PartialEq, Props)]
    struct HolderProps {
        children: Children,
    }

    impl Component for Holder {
        type Props = HolderProps;
        type Event = NoEvent;

        fn view(props: HolderProps, _: Scope<NoEvent>) -> impl IntoElement {
            view().child(props.children)
        }
    }

    /// A view passing content to nested components: a pocket's lazy
    /// children and a region in a holder's; a press opens and closes.
    #[derive(Default)]
    struct Wardrobe;

    #[derive(Clone, PartialEq, Props)]
    struct WardrobeProps {}

    impl Component for Wardrobe {
        type Props = WardrobeProps;
        type Event = NoEvent;

        fn view(_: WardrobeProps, _: Scope<NoEvent>) -> impl IntoElement {
            let open = signal(false);
            view()
                .on_press(move |_: &()| open.update(|o| *o = !*o))
                .child(
                    Pocket::element(PocketProps {
                        open: open.into(),
                        children: Children::default(),
                    })
                    .child(Fresh::element(FreshProps {})),
                )
                .child(
                    Holder::element(HolderProps {
                        children: Children::default(),
                    })
                    .child(text("before"))
                    .child(If::new(open, || text("open")))
                    .child(text("after")),
                )
        }
    }

    #[test]
    fn a_view_passes_lazy_children_and_regions_to_nested_components() {
        let mut c = core();
        c.mount_root(&Root::new::<Wardrobe>(|| WardrobeProps {}));
        let batch = c.commit().unwrap();
        let mut shown = texts(&batch);
        shown.sort();
        assert_eq!(shown, ["after", "before"]);
        assert_eq!(FRESH.with(Cell::get), 0, "closed: not made");
        let host = root_host(&batch);
        // Opened: the content is made, the region's text placed between.
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        let mut shown = texts(&batch);
        shown.sort();
        assert_eq!(shown, ["fresh 1", "open"]);
        // The region's text is the holder's middle child.
        let holder = c
            .components
            .instances
            .values()
            .find(|i| i.js_children.len() == 3)
            .expect("the holder has three children");
        assert_eq!(holder.js_children[1], text_node(&batch, "open"));
        // Closed, then opened again: made afresh.
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        c.commit();
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        let mut shown = texts(&c.commit().unwrap());
        shown.sort();
        assert_eq!(shown, ["fresh 2", "open"]);
        let mut events = Vec::new();
        c.take_events(&mut events);
        assert!(events.is_empty(), "nothing for JS: {events:?}");
    }

    /// A text as its root: a count, bumped by a press.
    #[derive(Default)]
    struct Tally;

    #[derive(Clone, PartialEq, Props)]
    struct TallyProps {}

    impl Component for Tally {
        type Props = TallyProps;
        type Event = NoEvent;

        fn view(_: TallyProps, _: Scope<NoEvent>) -> impl IntoElement {
            let n = signal(0);
            text(move || format!("tally {}", n.get())).on_press(move |_: &()| n.update(|n| *n += 1))
        }
    }

    /// A scroll view as its root.
    #[derive(Default)]
    struct Scroller;

    #[derive(Clone, PartialEq, Props)]
    struct ScrollerProps {}

    impl Component for Scroller {
        type Props = ScrollerProps;
        type Event = NoEvent;

        fn view(_: ScrollerProps, _: Scope<NoEvent>) -> impl IntoElement {
            scroll().child(text("inside"))
        }
    }

    #[test]
    fn a_views_root_can_be_any_element() {
        let mut c = core();
        c.mount_root(&Root::new::<Tally>(|| TallyProps {}));
        let batch = c.commit().unwrap();
        let host = root_host(&batch);
        assert!(
            batch.items.iter().any(|i| matches!(
                i,
                MountItem::Create { id, kind: crate::ViewKind::Text } if *id == host
            )),
            "the host is the text: {:?}",
            batch.items
        );
        assert_eq!(
            batch
                .items
                .iter()
                .filter(|i| matches!(i, MountItem::Create { id, .. } if *id == host))
                .count(),
            1,
            "made once"
        );
        assert_eq!(texts(&batch), ["tally 0"]);
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        assert_eq!(texts(&c.commit().unwrap()), ["tally 1"]);

        let mut c = core();
        c.mount_root(&Root::new::<Scroller>(|| ScrollerProps {}));
        let batch = c.commit().unwrap();
        let host = root_host(&batch);
        assert!(
            batch.items.iter().any(|i| matches!(
                i,
                MountItem::Create { id, kind: crate::ViewKind::Scroll } if *id == host
            )),
            "the host is the scroll view: {:?}",
            batch.items
        );
        assert_eq!(texts(&batch), ["inside"]);
    }

    /// Dims a nested component (its host) with a bound style; a press
    /// flips it.
    #[derive(Default)]
    struct Dimmer;

    #[derive(Clone, PartialEq, Props)]
    struct DimmerProps {}

    impl Component for Dimmer {
        type Props = DimmerProps;
        type Event = NoEvent;

        fn view(_: DimmerProps, _: Scope<NoEvent>) -> impl IntoElement {
            let dim = signal(false);
            view()
                .on_press(move |_: &()| dim.update(|d| *d = !*d))
                .child(
                    Holder::element(HolderProps {
                        children: Children::default(),
                    })
                    .w(px(10.0))
                    .opacity(move || if dim.get() { 0.5_f32 } else { 1.0 }),
                )
        }
    }

    #[test]
    fn a_views_bound_styles_on_a_nested_component_are_a_binding() {
        let mut c = core();
        c.mount_root(&Root::new::<Dimmer>(|| DimmerProps {}));
        let batch = c.commit().unwrap();
        let host = root_host(&batch);
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        let dimmed = batch.items.iter().find_map(|i| match i {
            MountItem::ViewStyle { id, style } if style.opacity == 0.5 => Some(*id),
            _ => None,
        });
        let nested = dimmed.expect("the nested host is dimmed");
        assert_ne!(nested, host);
        assert_eq!(creates(&batch), 0);
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        assert_eq!(view_style_of(&batch, nested).map(|s| s.opacity), Some(1.0));
    }

    /// Counts back presses while its view lives.
    #[derive(Default)]
    struct Catcher;

    #[derive(Clone, PartialEq, Props)]
    struct CatcherProps {}

    impl Component for Catcher {
        type Props = CatcherProps;
        type Event = NoEvent;

        fn view(_: CatcherProps, _: Scope<NoEvent>) -> impl IntoElement {
            let caught = signal(0);
            on_back_press(move || {
                caught.update(|n| *n += 1);
                true
            });
            view().child(text(move || format!("caught {}", caught.get())))
        }
    }

    #[test]
    fn a_views_back_press_listener_hears_the_back_button() {
        let mut c = core();
        c.mount_root(&Root::new::<Catcher>(|| CatcherProps {}));
        assert_eq!(texts(&c.commit().unwrap()), ["caught 0"]);
        assert!(c.component_event(ROOT_ID, EventKind::BackPress, &EventPayload::None));
        assert_eq!(texts(&c.commit().unwrap()), ["caught 1"]);
    }

    /// Its view panics as it is made.
    #[derive(Default)]
    struct Broken;

    #[derive(Clone, PartialEq, Props)]
    struct BrokenProps {}

    impl Component for Broken {
        type Props = BrokenProps;
        type Event = NoEvent;

        fn view(_: BrokenProps, _: Scope<NoEvent>) -> impl IntoElement {
            panic!("no view");
            #[allow(unreachable_code)]
            view()
        }
    }

    /// A counter whose text panics past 1, and a broken component, each
    /// under an error boundary.
    #[derive(Default)]
    struct Guarded;

    #[derive(Clone, PartialEq, Props)]
    struct GuardedProps {}

    impl Component for Guarded {
        type Props = GuardedProps;
        type Event = NoEvent;

        fn view(_: GuardedProps, _: Scope<NoEvent>) -> impl IntoElement {
            let n = signal(0);
            view()
                .on_press(move |_: &()| n.update(|n| *n += 1))
                .child(catch_error(
                    move || {
                        text(move || {
                            assert!(n.get() < 2, "too many");
                            format!("n {}", n.get())
                        })
                    },
                    |e, _| text(format!("failed: {}", e.message)),
                ))
                .child(catch_error(
                    || Broken::element(BrokenProps {}),
                    |e, _| text(format!("{} failed", e.component)),
                ))
        }
    }

    #[test]
    fn error_boundaries_show_their_fallback() {
        let mut c = core();
        c.mount_root(&Root::new::<Guarded>(|| GuardedProps {}));
        let batch = c.commit().unwrap();
        let shown = texts(&batch);
        assert!(shown.contains(&"n 0".to_owned()), "{shown:?}");
        assert!(
            shown.iter().any(|t| t.ends_with("Broken failed")),
            "{shown:?}"
        );
        let host = root_host(&batch);
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        assert_eq!(texts(&c.commit().unwrap()), ["n 1"]);
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        assert_eq!(texts(&c.commit().unwrap()), ["failed: too many"]);
    }

    /// A large component with one value changing often: 300 rows of
    /// static content and a counter.
    #[component]
    fn Ticker() -> impl IntoElement {
        let tick = signal(0_u32);
        view()
            .on_press(move |_: &()| tick.update(|t| *t += 1))
            .child(text(move || format!("tick {}", tick.get())))
            .children((0..300).map(|i| {
                view()
                    .flex_row()
                    .p_2()
                    .gap_2()
                    .child(view().size(px(20.0)).bg(rgb(0x3366ff)))
                    .child(text(format!("row {i}")).text_sm())
            }))
    }

    /// `cargo test --release -p nsh-core bench_view_tick -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_view_tick() {
        let mut c = core();
        c.mount_root(&Root::new::<Ticker>(|| ()));
        let host = root_host(&c.commit().unwrap());
        let n = 2000;
        let (mut event, mut commit) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        let mut items = 0;
        for _ in 0..n {
            let start = std::time::Instant::now();
            c.component_event(host, EventKind::Press, &EventPayload::None);
            let mid = std::time::Instant::now();
            items += c.commit().map_or(0, |b| b.items.len());
            event += mid - start;
            commit += mid.elapsed();
        }
        eprintln!(
            "view tick: {:?} per event, {:?} per commit, {} mount items",
            event / n,
            commit / n,
            items / n as usize
        );
    }

    #[derive(Clone, Copy, PartialEq, Props)]
    struct ShownProps {
        value: crate::ui::Prop<SharedString>,
    }

    /// Shows its prop.
    #[component]
    fn Shown(props: ShownProps) -> impl IntoElement {
        text(props.value)
    }

    /// Passes `Shown` a binding, then, from the second press, a value: its
    /// region makes the element again, a new `Shown`.
    #[component]
    fn Passer() -> impl IntoElement {
        let n = signal(0);
        let fixed = signal(false);
        view()
            .on_press(move |_: &()| {
                if n.get_untracked() == 1 {
                    fixed.set(true);
                }
                n.update(|n| *n += 1);
            })
            .child(dynamic(move || {
                let value: crate::ui::Prop<SharedString> = if fixed.get() {
                    "fixed".into()
                } else {
                    crate::ui::Prop::bind(move || format!("n {}", n.get()).into())
                };
                Shown::element(ShownProps { value })
            }))
    }

    #[test]
    fn a_prop_follows_a_binding_and_a_region_made_again_makes_a_new_component() {
        let mut c = core();
        c.mount_root(&Root::new::<Passer>(|| ()));
        let batch = c.commit().unwrap();
        assert_eq!(texts(&batch), ["n 0"]);
        let host = root_host(&batch);
        let press = |c: &mut Core<FakeEngine>| {
            assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
            c.commit().map(|b| texts(&b)).unwrap_or_default()
        };
        assert_eq!(press(&mut c), ["n 1"]);
        assert_eq!(press(&mut c), ["fixed"]);
        // The old `Shown` went with its binding.
        assert!(press(&mut c).is_empty());
    }

    /// Counts Mod+K while a branch is shown; a press hides the branch.
    #[component]
    fn Shortcuts() -> impl IntoElement {
        let count = signal(0);
        let shown = signal(true);
        view()
            .on_press(move |_: &()| shown.set(false))
            .child(text(move || format!("k {}", count.get())))
            .child(If::new(shown, move || {
                on_key("mod+k", move |_| count.update(|c| *c += 1));
                view()
            }))
    }

    #[test]
    fn a_views_shortcuts_join_jss_and_run_in_the_core() {
        use crate::keymap::{KeyBinding, Keymap, Keystroke, When};
        let tables = |batch: &MountBatch| -> Vec<Vec<u32>> {
            batch
                .items
                .iter()
                .filter_map(|i| match i {
                    MountItem::Keymap(k) => Some(k.bindings().iter().map(|b| b.id).collect()),
                    _ => None,
                })
                .collect()
        };
        let mut c = core();
        c.mount_root(&Root::new::<Shortcuts>(|| ()));
        let batch = c.commit().unwrap();
        let host = root_host(&batch);
        let table = tables(&batch);
        let [ours] = table.last().expect("a table").as_slice() else {
            panic!("{table:?}");
        };
        let ours = *ours;
        assert!(ours >= crate::ui::keys::FIRST_ID);
        // JS's table joins it: JS's first, then the views'.
        let js = KeyBinding::new(
            1,
            Keystroke::parse_with("mod+j", true).unwrap(),
            When::Auto,
            false,
        );
        apply(
            &mut c,
            vec![Op::SetProp {
                id: ROOT_ID,
                prop: Prop::Keymap(std::sync::Arc::new(Keymap::new(vec![js]))),
            }],
        );
        assert_eq!(tables(&c.commit().unwrap()), [vec![1, ours]]);
        // The view's runs here; JS's goes to JS.
        let key = |id: u32| EventPayload::Value(crate::PayloadValue::Number(id as f64));
        assert!(c.component_event(ROOT_ID, EventKind::KeyCommand, &key(ours)));
        assert_eq!(texts(&c.commit().unwrap()), ["k 1"]);
        assert!(!c.component_event(ROOT_ID, EventKind::KeyCommand, &key(1)));
        // The binding goes with its owner.
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        assert_eq!(tables(&c.commit().unwrap()), [vec![1]]);
    }

    /// A leaf in a region of its own.
    #[component]
    fn Leafy() -> impl IntoElement {
        view().child(dynamic(|| text("leaf")))
    }

    /// Shows a `Leafy` until pressed.
    #[component]
    fn Branch() -> impl IntoElement {
        let shown = signal(true);
        view()
            .on_press(move |_: &()| shown.set(false))
            .child(If::new(shown, || Leafy::element(())))
    }

    #[test]
    fn a_released_view_releases_what_its_regions_made() {
        let mut c = core();
        c.mount_root(&Root::new::<Branch>(|| ()));
        let batch = c.commit().unwrap();
        let leaf = text_node(&batch, "leaf");
        let host = root_host(&batch);
        assert!(c.component_event(host, EventKind::Press, &EventPayload::None));
        let batch = c.commit().unwrap();
        assert!(
            batch
                .items
                .iter()
                .any(|i| matches!(i, MountItem::Delete { id } if *id == leaf)),
            "{:?}",
            batch.items
        );
    }
}
