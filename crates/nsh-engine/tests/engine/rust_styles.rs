//! A Rust app's typed styles and texts end to end: effects reach the
//! platform as the same view styles classes make, a group's press restyles
//! what is inside it natively, and a text's spans are one paragraph whose
//! links take presses.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, AttributedText, EventKind, EventPayload, MountBatch, MountItem, Size,
    ViewStyle,
    tailwind::state,
    ui::{
        Root,
        palette::{SKY, VIOLET, ZINC},
        prelude::*,
    },
};
use nsh_engine::{App, Engine, EngineConfig, Environment};

use crate::ChannelSink;

#[component]
fn Card() -> impl IntoElement {
    let taps = signal(0);
    view()
        .flex_1()
        .child(
            view()
                .group()
                .p_4()
                .rounded_tl(px(20.0))
                .border_t(px(4.0))
                .border_dashed()
                .bg_linear_to_br()
                .gradient_from(SKY.shade(400))
                .gradient_to(VIOLET.shade(600))
                .shadow_lg()
                .shadow_color(SKY.shade(500).opacity(0.5))
                .ring(px(2.0))
                .ring_color(SKY.shade(500))
                .perspective(px(400.0))
                .rotate_y(deg(20.0))
                .animate_pulse()
                .child(
                    text("In the group")
                        .text_color(ZINC.shade(500))
                        .group_active(|s| s.text_color(VIOLET.shade(600))),
                ),
        )
        .child(
            text(move || format!("Tapped {} times: ", taps.get()))
                .child(
                    text("tap")
                        .underline()
                        .line_through()
                        .on_press(move |_: &()| taps.update(|t| *t += 1)),
                )
                .child(" or ")
                .child(text("not").font_weight(FontWeight::BOLD)),
        )
}

fn start() -> (Engine, Receiver<MountBatch>) {
    start_root(Root::new::<Card>(|| ()))
}

fn start_root(root: Root) -> (Engine, Receiver<MountBatch>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: App::Rust(root),
        viewport: Size::new(px(390.0), px(844.0)),
        trace: false,
        network: None,
        websocket: None,
        filesystem: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (nsh_core::Points(44.0), nsh_core::Points(96.0)),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        data_dir: None,
        storage: None,
        clock: Default::default(),
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// What the batches within a moment send: view styles, text blocks and
/// the views whose states the platform must report.
#[derive(Default)]
struct Seen {
    styles: Vec<(u32, ViewStyle)>,
    texts: Vec<(u32, std::sync::Arc<AttributedText>)>,
    tracked: Vec<u32>,
}

fn seen(rx: &Receiver<MountBatch>) -> Seen {
    let mut seen = Seen::default();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(500)) {
        for item in &batch.items {
            match item {
                MountItem::ViewStyle { id, style } => seen.styles.push((*id, (**style).clone())),
                MountItem::Text { id, text, .. } => seen.texts.push((*id, text.clone())),
                MountItem::TrackStates { id, states } if states & state::PRESSED != 0 => {
                    seen.tracked.push(*id)
                }
                _ => {}
            }
        }
    }
    seen
}

#[test]
fn a_rust_apps_typed_styles_and_spans_reach_the_platform() {
    let (engine, rx) = start();
    let first = seen(&rx);
    let (group, card) = first
        .styles
        .iter()
        .find(|(_, s)| {
            s.effects
                .as_ref()
                .is_some_and(|e| !e.background_images.is_empty())
        })
        .expect("the card's gradient");
    let effects = card.effects.as_ref().unwrap();
    // The shadow's two layers and the ring.
    assert_eq!(effects.box_shadows.len(), 3);
    assert_eq!(card.borders.widths.top.0, 4.0);
    assert_eq!(card.borders.style, nsh_core::BorderStyle::Dashed);
    assert_eq!(card.corner_radius.top_left, Some(nsh_core::Points(20.0)));
    assert_eq!(card.transform.ops().len(), 2, "perspective and rotateY");
    assert_eq!(
        card.animation.map(|a| a.keyframes),
        Some(nsh_core::Keyframes::Pulse)
    );
    assert!(first.tracked.contains(group), "a group reports its presses");

    let sentence = first
        .texts
        .iter()
        .find(|(_, t)| t.text.starts_with("Tapped"))
        .map(|(_, t)| t.clone())
        .expect("the sentence");
    assert_eq!(sentence.text, "Tapped 0 times: tap or not");
    let link = sentence.links[0];
    let tap = sentence
        .spans
        .iter()
        .find(|s| s.start == link.start)
        .unwrap();
    assert!(tap.style.decoration.underline && tap.style.decoration.line_through);

    // The group pressed: the text inside it turns violet, with no render.
    engine.control().set_states(*group, state::PRESSED);
    let pressed = seen(&rx);
    let inside = pressed
        .texts
        .iter()
        .find(|(_, t)| t.text == "In the group")
        .expect("restyled");
    assert_eq!(inside.1.spans[0].style.color, VIOLET.shade(600).into());

    // The link takes the press, not the sentence.
    engine
        .control()
        .dispatch_event(link.node, EventKind::Press, EventPayload::None);
    let tapped = seen(&rx);
    assert!(
        tapped
            .texts
            .iter()
            .any(|(_, t)| t.text == "Tapped 1 times: tap or not")
    );
}

/// `nsh/base`'s `Button` styled per state by a Rust app, as a TSX app's
/// `active:` / `selected:` classes style it.
#[component]
fn Buttons() -> impl IntoElement {
    let button = |selected: bool| {
        nsh_base::Button::element(nsh_base::ButtonProps {
            selected: selected.into(),
            ..nsh_base::ButtonProps::default()
        })
        .p_2()
        .bg(rgb(0x0000ff))
        .selected(|s| s.bg(rgb(0x00ff00)))
        .active(|s| s.bg(rgb(0xff0000)))
        .child(text(if selected { "On" } else { "Off" }))
    };
    view().flex_1().child(button(false)).child(button(true))
}

#[test]
fn a_rust_apps_button_takes_its_state_styles() {
    let (engine, rx) = start_root(Root::new::<Buttons>(|| ()));
    let first = seen(&rx);
    let background = |seen: &Seen, id: u32| {
        seen.styles
            .iter()
            .rev()
            .find(|(i, _)| *i == id)
            .map(|(_, s)| s.background_color)
    };
    let (blue, green, red) = (
        nsh_core::Color(0x0000ffff),
        nsh_core::Color(0x00ff00ff),
        nsh_core::Color(0xff0000ff),
    );
    // Both buttons report their presses; the selected one is green.
    assert_eq!(first.tracked.len(), 2);
    let plain = *first
        .tracked
        .iter()
        .find(|&&id| background(&first, id) == Some(blue))
        .expect("the plain button");
    assert!(
        first
            .tracked
            .iter()
            .any(|&id| background(&first, id) == Some(green)),
        "selected:"
    );
    // Pressed: red, natively.
    engine.control().set_states(plain, state::PRESSED);
    assert_eq!(background(&seen(&rx), plain), Some(red));
    engine.control().set_states(plain, 0);
    assert_eq!(background(&seen(&rx), plain), Some(blue));
}
