//! `Terminal`'s input methods: what one composes is drawn at the cursor,
//! the cursor after it, and the hidden input that takes the keyboard sits
//! on the cursor's cell, where the platform opens the candidates. A program
//! that hides the cursor still composes where it is.

use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use nsh_base::{TerminalEmulator, TerminalEmulatorFactory, VtEmulator, terminal};
use nsh_core::{
    EventKind, EventPayload, PayloadValue, Size, ViewKind,
    element::{Root, measure_text},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

/// A cell's width, as the harness measured it.
static CELL: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, PartialEq, Props)]
struct HarnessProps {
    /// What the program wrote before the terminal was laid out.
    output: String,
}

/// A terminal 400 wide and five rows of 18 high, whose emulator has
/// `output` already.
#[component]
fn Harness(props: HarnessProps) -> impl IntoElement {
    let output = props.output.clone();
    let cell = measure_text("13px monospace", "0000000000") / 10.0;
    CELL.store(cell.0.to_bits(), Ordering::Relaxed);
    provide_context(TerminalEmulatorFactory::new(move |size, scrollback| {
        let mut vt = VtEmulator::new(size, scrollback);
        vt.feed(output.as_bytes());
        Box::new(vt)
    }));
    view().w(px(400.0)).h(px(18.0 * 5.0)).child(
        terminal()
            .font_family("monospace")
            .accessibility_label("Shell")
            .flex_1(),
    )
}

fn start(output: &str) -> Headless {
    let output = output.to_owned();
    let config = EngineConfig {
        viewport: Size::new(px(400.0), px(400.0)),
        ..EngineConfig::new(Root::new::<Harness>(move || HarnessProps {
            output: output.clone(),
        }))
    };
    let mut app = Headless::start(config).expect("the engine starts");
    app.wait_for("text:$ abc").unwrap();
    app
}

/// An input method's composition changing in the hidden input, as the
/// platform reports it. The driver has no composition step and does not
/// expose the input's node id, so the event goes to every node; only the
/// input listens to it.
fn compose(app: &mut Headless, composing: bool, text: &str) {
    for id in 1..4096 {
        app.engine().control().dispatch_event(
            id,
            EventKind::CompositionChange,
            EventPayload::Map(vec![
                ("composing", PayloadValue::Bool(composing)),
                ("text", PayloadValue::Str(text.to_owned())),
            ]),
        );
    }
    app.wait(Duration::from_millis(50)).unwrap();
}

/// Where the hidden input is, in the root's points.
fn input_at(app: &Headless) -> Point<Points> {
    let input = app
        .screen()
        .layout_nodes()
        .into_iter()
        .find(|n| n.kind == ViewKind::Input)
        .expect("the hidden input");
    input.bounds.origin
}

fn cell() -> f32 {
    f32::from_bits(CELL.load(Ordering::Relaxed))
}

#[test]
fn a_composition_is_drawn_at_the_cursor_and_the_input_sits_on_its_cell() {
    let mut app = start("$ abc");
    // The cursor is after `$ abc`: column 5 of the first row.
    let at = input_at(&app);
    // Layout places views on whole points.
    assert!(
        (at.x.0 - 5.0 * cell()).abs() < 0.5,
        "{at:?}, a cell {}",
        cell()
    );
    assert_eq!(at.y.0, 0.0);
    compose(&mut app, true, "ni hao");
    app.wait_for("text:ni hao").unwrap();
    compose(&mut app, false, "");
    app.wait_for_gone("text:ni hao", Duration::from_secs(2))
        .unwrap();
}

#[test]
fn the_input_follows_the_cursor_and_a_hidden_cursor_still_has_a_place() {
    let mut app = start("$ abc\r\n> \x1b[?25l");
    let at = input_at(&app);
    assert!((at.x.0 - 2.0 * cell()).abs() < 0.5, "{at:?}");
    assert_eq!(at.y.0, 18.0, "the second row");
    compose(&mut app, true, "ka");
    app.wait_for("text:ka").unwrap();
}
