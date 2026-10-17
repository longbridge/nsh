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
                .border_t(4.0)
                .border_dashed()
                .bg_linear_to_br()
                .gradient_from(SKY.shade(400))
                .gradient_to(VIOLET.shade(600))
                .shadow_lg()
                .shadow_color(SKY.shade(500).with_alpha(0.5))
                .ring(2.0)
                .ring_color(SKY.shade(500))
                .perspective(400.0)
                .rotate_y(20.0)
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
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: App::Rust(Root::new::<Card>(|| ())),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
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
    assert_eq!(card.borders.widths[0], 4.0);
    assert_eq!(card.borders.style, nsh_core::BorderStyle::Dashed);
    assert_eq!(card.corner_radius[0], Some(20.0));
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
    assert_eq!(inside.1.spans[0].style.color, VIOLET.shade(600));

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
