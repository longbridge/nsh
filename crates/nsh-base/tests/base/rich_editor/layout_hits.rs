//! Presses and shift-presses land on character edges when a mark is in a
//! larger font with inline padding: the editor hit tests with the text
//! engine's layout, whose padding placeholders are not document offsets.

use nsh_base::rich_editor::view::RichEditorClasses;

use super::*;

/// `ab` `cd` (inline code: a larger font, padded) `ef`: positions 1..7.
fn mixed_doc() -> Value {
    json!({"type": "doc", "content": [{"type": "paragraph", "content": [
        {"type": "text", "text": "ab"},
        {"type": "text", "text": "cd", "marks": [{"type": "code"}]},
        {"type": "text", "text": "ef"},
    ]}]})
}

#[derive(Clone, PartialEq, Props)]
struct MixedProps {
    session: Option<EditorSession>,
}

#[component]
fn Mixed(props: MixedProps) -> impl IntoElement {
    let session = props.session.clone().expect("a session");
    let read = {
        let session = session.clone();
        move || {
            let state = session.state();
            let s = state.selection();
            format!("Sel {}-{}", s.from(), s.to())
        }
    };
    let shown = signal(read());
    view()
        .child(
            view().test_id("editor").w(px(600.0)).h(px(400.0)).child(
                rich_editor()
                    .editor(session)
                    .classes({
                        let mut classes = RichEditorClasses::default();
                        classes.nodes = Some([("paragraph".into(), "text-[16px]".into())].into());
                        classes.marks = Some([("code".into(), "px-2 text-[24px]".into())].into());
                        classes
                    })
                    .size_full()
                    .on_selection_change(move |_: &_| shown.set(read())),
            ),
        )
        .child(text(move || shown.get()))
        .child(text("Ready"))
}

fn start_mixed() -> Headless {
    let session = session(mixed_doc());
    let mut config = EngineConfig {
        viewport: Size::new(px(800.0), px(900.0)),
        ..EngineConfig::new(nsh_testing::windowed_root_with::<Mixed>(move || {
            MixedProps {
                session: Some(session.clone()),
            }
        }))
    };
    config.environment.os = Os::Linux;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).expect("the engine starts");
    app.wait_for("text:Ready").unwrap();
    app
}

/// The paragraph's laid-out text (padding placeholders included) and the
/// headless engine's advance for it (every character alike, in the largest
/// font).
fn laid(app: &Headless) -> (String, Arc<nsh_core::AttributedText>, f32) {
    let paragraph = app
        .screen()
        .layout_nodes()
        .into_iter()
        .find_map(|node| {
            node.text
                .filter(|t| t.text.text.replace(nsh_core::ATTACHMENT_CHAR, "") == "abcdef")
        })
        .expect("the paragraph's text");
    let advance = char_advance(&paragraph.text);
    (paragraph.text.text.to_string(), paragraph.text, advance)
}

/// A press at `at`, apart from the last one (not a double click).
fn press(app: &mut Headless, at: Point<Points>) {
    app.wait(Duration::from_millis(600)).unwrap();
    app.press_at("text:abcdef", at).unwrap();
}

/// Just after the left edge of laid-out character `k`.
fn edge(advance: f32, k: usize) -> Point<Points> {
    Point::new(px(advance * k as f32 + 1.0), px(2.0))
}

#[test]
fn presses_in_a_padded_larger_mark_land_on_character_edges() {
    let mut app = start_mixed();
    let (text, attributed, advance) = laid(&app);
    let ph = nsh_core::ATTACHMENT_CHAR;
    // The core pads the code span with a placeholder on each side.
    assert_eq!(text, format!("ab{ph}cd{ph}ef"), "{attributed:?}");
    // Laid-out index → document position: each placeholder before it is
    // no offset of the text.
    for (k, pos) in [(0, 1), (1, 2), (3, 3), (4, 4), (6, 5), (7, 6), (8, 7)] {
        press(&mut app, edge(advance, k));
        see(&mut app, &format!("Sel {pos}-{pos}"));
    }
    // "d" (laid 4, after a placeholder) to "f" (laid 7, after both: position 6): a
    // shift-press extends to the character edge, not past the padding.
    press(&mut app, edge(advance, 4));
    see(&mut app, "Sel 4-4");
    app.hold_keys("shift", true).unwrap();
    press(&mut app, edge(advance, 7));
    app.hold_keys("shift", false).unwrap();
    see(&mut app, "Sel 4-6");
}
