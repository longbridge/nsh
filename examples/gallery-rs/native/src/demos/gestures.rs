//! Gestures: a photo dragged, pinched and rotated natively, a ball dragged
//! as the touch responder, raw touches with every finger drawn, press
//! events lighting up as they arrive, hit slop, pointer events and
//! accessibility labels (as the TSX gallery's `demos/Gestures.tsx`).

use std::rc::Rc;

use nsh_core::{
    Rect, Ripple, TouchPhase, TouchPoint, UNDERLAY_OPACITY,
    nsh_animation::{Curve, EaseMode},
    ui::{Animation, BoxShadow, TouchEvent, prelude::*},
};

use crate::ui::{
    Badge, Button, ButtonVariant, Card, Finish, Finished, IconName, IconTile, MONO, PressProps,
    Row, Stat, Tone, appear, caption, colors, icon, inset_shadow, press_with, radius, rgb_a, tints,
};

/// A dot matrix behind a stage, drawn for its size (again when it or the
/// appearance changes).
#[component]
fn Dots() -> impl IntoElement {
    let size = signal((0.0_f32, 0.0_f32));
    view()
        .flex_1()
        .pointer_events(PointerEvents::None)
        .child(
            canvas(move || {
                let (width, height) = size.get();
                let mut p = Painter::new().fill_color(color(colors::LINE));
                let mut x = 8.0;
                while x < width {
                    let mut y = 8.0;
                    while y < height {
                        p = p.fill_rect(x, y, 1.5, 1.5);
                        y += 14.0;
                    }
                    x += 14.0;
                }
                p
            })
            .flex_1(),
        )
        .on_layout(move |frame: &Rect| size.set((frame.width, frame.height)))
}

/// [`Dots`] filling its stage.
fn dots() -> impl IntoElement {
    Dots::element(()).absolute().inset_0()
}

/// A stage: the darker surface a demo plays on.
fn stage() -> View {
    view()
        .rounded(px(radius::CONTROL))
        .border_curve(BorderCurve::Continuous)
        .overflow_hidden()
        .bg(colors::BACKGROUND)
        .border_1()
        .border_color(colors::LINE)
        .finish(Finish::Track)
}

/// The highlight and shadow of a small colored knob.
fn knob<E: Finished>(element: E) -> E {
    let (light, _) = Finish::Knob.shadows();
    element.adaptive_shadow(
        light.iter().copied(),
        [
            inset_shadow(1.0, 0.0, rgb_a(0xffffff, 0.35)),
            BoxShadow {
                offset_x: 0.0,
                offset_y: 3.0,
                blur: 10.0,
                spread: 0.0,
                color: rgb_a(0x000000, 0.45),
                inset: false,
            },
        ],
    )
}

/// A plain label with the API it uses, in code type.
fn api(label: &'static str, api: &'static str) -> View {
    view()
        .flex_row()
        .items_baseline()
        .gap_2()
        .child(
            text(label)
                .flex_1()
                .text_size(px(13.0))
                .text_color(colors::MUTED),
        )
        .child(
            text(api)
                .text_size(px(12.0))
                .font_family(MONO)
                .text_color(colors::FAINT),
        )
}

/// The fingers still down during `touch` (an end or cancel lists the lifted
/// ones too), and the one it is about.
fn fingers(touch: &TouchEvent) -> (Vec<&TouchPoint>, Option<&TouchPoint>) {
    let lifted = matches!(touch.phase, TouchPhase::End | TouchPhase::Cancel);
    let down = touch
        .points
        .iter()
        .filter(|p| !lifted || !p.changed)
        .collect();
    (down, touch.points.iter().find(|p| p.changed))
}

/// Drag, pinch and rotate one photo at the same time, all on the UI thread.
#[component]
fn Transformable() -> impl IntoElement {
    let values = [0.0, 0.0, 1.0, 0.0].map(native_value);
    let [x, y, scale, angle] = values.clone();
    // The last drag's distance, scale and angle.
    let drag = signal(0.0_f32);
    let pinched = signal(1.0_f32);
    let turned = signal(0.0_f32);
    let reset = move |_: &()| {
        let [x, y, scale, angle] = &values;
        let _ = x.spring(0.0, Spring::default());
        let _ = y.spring(0.0, Spring::default());
        let _ = scale.spring(1.0, Spring::default());
        let _ = angle.spring(0.0, Spring::default());
        batch(|| {
            drag.set(0.0);
            pinched.set(1.0);
            turned.set(0.0);
        });
    };
    Card::new()
        .icon(IconName::Gestures)
        .title("Pan, pinch and rotation")
        .subtitle("Three recognizers on one view, driven natively")
        .child(
            stage()
                .h(px(260.0))
                .items_center()
                .justify_center()
                .child(dots())
                .child(
                    view()
                        .pointer_events(PointerEvents::None)
                        .absolute()
                        .w(px(200.0))
                        .h(px(150.0))
                        .rounded(px(18.0))
                        .border_1()
                        .border_color(colors::FAINT)
                        .border_dashed(),
                )
                .child(
                    view()
                        .w(px(200.0))
                        .h(px(150.0))
                        .rounded(px(18.0))
                        .border_curve(BorderCurve::Continuous)
                        .overflow_hidden()
                        .border_1()
                        .border_color(colors::BORDER)
                        .bg(colors::RAISED)
                        .pan_values(Some(&x), Some(&y))
                        .pinch_value(&scale)
                        .rotation_value(&angle)
                        .on_pan_end(move |e: &PanEvent| {
                            drag.set(e.translation_x.hypot(e.translation_y).round());
                        })
                        .on_pinch_end(move |e: &GestureEvent| pinched.set(e.value))
                        .on_rotation_end(move |e: &GestureEvent| turned.set(e.value))
                        .bind_translate_x(&x)
                        .bind_translate_y(&y)
                        .bind_scale(&scale)
                        .bind_rotate(&angle)
                        .child(image("https://picsum.photos/seed/nsh-card/400/300").flex_1()),
                ),
        )
        .child(
            view()
                .flex_row()
                .child(Stat::new("Drag", move || format!("{} pt", drag.get())).width(0.33))
                .child(
                    Stat::new("Scale", move || format!("{:.2}×", pinched.get()))
                        .tone(Tone::Accent)
                        .width(0.33),
                )
                .child(
                    Stat::new("Angle", move || format!("{}°", turned.get().round()))
                        .tone(Tone::Info)
                        .width(0.34),
                ),
        )
        .child(
            text("Pinch and rotate with two fingers (Option-drag in the simulator).")
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(colors::MUTED),
        )
        .child(Button::new("Reset", reset).variant(ButtonVariant::Secondary))
}

/// A spring of stiffness `k` and damping `c`.
const fn spring(k: f32, c: f32) -> Spring {
    Spring {
        stiffness: k,
        damping: c,
        mass: 1.0,
        velocity: None,
    }
}

/// A ball dragged as the touch responder (positions go through Rust, a
/// binding per move). It claims the touches, so the page does not scroll
/// while it is dragged.
#[component]
fn Ball() -> impl IntoElement {
    // Where the ball is, from where it started; where it was when the drag
    // began; what happened last, and its tone.
    let pos = signal((0.0_f32, 0.0_f32));
    let start = stored((0.0_f32, 0.0_f32));
    let state = signal((SharedString::from("Idle"), Tone::Neutral));
    // 1 while held.
    let grab = native_value(0.0);
    // The ball's colors are the appearance's: it is made again when that
    // changes.
    let ball = dynamic(move || {
        let (grant, release, terminate) = (grab.clone(), grab.clone(), grab.clone());
        view()
            .absolute()
            .left(relative(0.5))
            .top(px(70.0))
            .ml(px(-30.0))
            .size(px(60.0))
            .rounded(px(30.0))
            .items_center()
            .justify_center()
            .bind_background(&grab.interpolate_colors(
                &[0.0, 1.0],
                &[color(colors::INFO), color(colors::ACCENT_FILL)],
            ))
            .map(knob)
            .style_with(move |s| {
                let (x, y) = pos.get();
                s.translate_x(x).translate_y(y)
            })
            .bind_scale(&grab.interpolate(&[0.0, 1.0], &[1.0, 1.15]))
            .claim_touches(TouchClaim::on_start())
            .on_responder_grant(move |_: &ResponderEvent| {
                start.replace(pos.get_untracked());
                let _ = grant.spring(1.0, spring(420.0, 24.0));
                state.set(("Dragging".into(), Tone::Accent));
            })
            .on_responder_move(move |e: &ResponderEvent| {
                let g = e.gesture;
                let (x, y) = start.with(|s| *s);
                pos.set((x + g.dx, y + g.dy));
            })
            .on_responder_release(move |e: &ResponderEvent| {
                let g = e.gesture;
                let _ = release.spring(0.0, spring(300.0, 20.0));
                state.set((format!("{:.2}, {:.2} pt/ms", g.vx, g.vy).into(), Tone::Info));
            })
            .on_responder_terminate(move |_: &ResponderEvent| {
                let _ = terminate.spring(0.0, Spring::default());
                state.set(("Taken over".into(), Tone::Warn));
            })
            .child(
                view()
                    .size(px(18.0))
                    .rounded(px(9.0))
                    .bg(rgb_a(0x13160c, 0.35)),
            )
    });
    Card::new()
        .icon(IconName::Scroll)
        .title("PanResponder")
        .subtitle("Positions go through Rust; the page does not scroll")
        .right(Badge::new(move || state.with(|s| s.0.clone())).tone(move || state.with(|s| s.1)))
        .child(stage().h(px(200.0)).child(dots()).child(ball))
        .child(caption(
            "Position",
            Some(move || {
                let (x, y) = pos.get();
                format!("{}, {}", x.round(), y.round())
            }),
        ))
        .child(
            Button::new("Center", move |_: &()| pos.set((0.0, 0.0)))
                .variant(ButtonVariant::Secondary),
        )
}

/// The touch pad's log: at most this many entries.
const TOUCH_LOG: usize = 4;

/// Raw touch events, every finger drawn where it is.
#[component]
fn TouchPad() -> impl IntoElement {
    // The latest first: the event's name and what it says.
    let log = signal(Vec::<(&'static str, String)>::new());
    // The fingers down, where they are in the pad.
    let fingers_down = signal(Vec::<(f32, f32)>::new());
    let down = computed(move || fingers_down.with(Vec::len));
    let on_touch = move |touch: &TouchEvent| {
        let (down, about) = fingers(touch);
        let Some(p) = about else { return };
        let name = match touch.phase {
            TouchPhase::Start => "start",
            TouchPhase::Move => "move",
            TouchPhase::End => "end",
            TouchPhase::Cancel => "cancel",
        };
        let entry = format!(
            "{}, {} · page {}, {} · {} down",
            p.location_x.round(),
            p.location_y.round(),
            p.page_x.round(),
            p.page_y.round(),
            down.len()
        );
        batch(|| {
            fingers_down.set(if touch.phase == TouchPhase::Cancel {
                Vec::new()
            } else {
                down.iter().map(|f| (f.location_x, f.location_y)).collect()
            });
            log.update(|log| {
                log.insert(0, (name, entry));
                log.truncate(TOUCH_LOG);
            });
        });
    };
    // A finger's ring, by its place among the fingers down: it follows
    // that finger.
    let ring = |finger: ReadSignal<(usize, (f32, f32))>, _| {
        view()
            .pointer_events(PointerEvents::None)
            .absolute()
            .left(move || px(finger.get().1.0 - 26.0))
            .top(move || px(finger.get().1.1 - 26.0))
            .size(px(52.0))
            .rounded(px(26.0))
            .border_2()
            .border_color(colors::ACCENT)
            .bg(tints::ACCENT)
    };
    // A log line, by its place (the latest first, in the accent): its
    // texts follow the entry at that place.
    let line = move |i: usize| {
        let entry = move |f: fn(&(&'static str, String)) -> SharedString| {
            move || log.with(|log| log.get(i).map(f).unwrap_or_default())
        };
        view()
            .flex_row()
            .items_baseline()
            .gap_2()
            .child(
                text(entry(|e| e.0.into()))
                    .w(px(52.0))
                    .text_size(px(12.0))
                    .font_family(MONO)
                    .text_color(if i == 0 {
                        colors::ACCENT
                    } else {
                        colors::FAINT
                    }),
            )
            .child(
                text(entry(|e| e.1.clone().into()))
                    .number_of_lines(1)
                    .flex_1()
                    .text_size(px(13.0))
                    .text_color(if i == 0 { colors::TEXT } else { colors::FAINT })
                    .tabular_nums(),
            )
    };
    Card::new()
        .icon(IconName::System)
        .title("Touch events")
        .subtitle("Start, move, end and cancel, for every finger")
        .right(
            Badge::new(move || format!("{} down", down.get())).tone(move || {
                if down.get() > 0 {
                    Tone::Accent
                } else {
                    Tone::Neutral
                }
            }),
        )
        .child(
            stage()
                .h(px(150.0))
                .items_center()
                .justify_center()
                .on_touch(on_touch)
                .child(dots())
                .child(If::new(
                    move || down.get() == 0,
                    || {
                        text("Touch here, try two fingers")
                            .text_size(px(14.0))
                            .text_color(colors::MUTED)
                    },
                ))
                .child(Each::new(
                    move || {
                        fingers_down.with(|f| f.iter().copied().enumerate().collect::<Vec<_>>())
                    },
                    |finger| finger.0,
                    ring,
                )),
        )
        .child(
            view()
                .gap_1()
                .min_h(px(TOUCH_LOG as f32 * 18.0))
                .child(If::new(
                    move || log.with(Vec::is_empty),
                    || {
                        text("No touches yet")
                            .text_size(px(13.0))
                            .text_color(colors::FAINT)
                    },
                ))
                .children(
                    (0..TOUCH_LOG)
                        .map(|i| If::new(move || log.with(|log| log.len() > i), move || line(i))),
                ),
        )
}

/// The press events, in the order of their chips.
const PRESS_EVENTS: [&str; 4] = ["pressIn", "press", "longPress", "pressOut"];

/// Each press event lights up its chip as it arrives.
#[component]
fn PressEvents() -> impl IntoElement {
    let flashes: Rc<[NativeValue]> = PRESS_EVENTS.iter().map(|_| native_value(0.0)).collect();
    let count = signal(0_u32);
    // The latest first.
    let log = signal(Vec::<&'static str>::new());
    // A listener noting `event`.
    let fire = {
        let flashes = flashes.clone();
        move |event: &'static str| {
            let flashes = flashes.clone();
            move |_: &()| {
                batch(|| {
                    count.update(|n| *n += 1);
                    log.update(|log| {
                        log.insert(0, event);
                        log.truncate(6);
                    });
                });
                if let Some(i) = PRESS_EVENTS.iter().position(|e| *e == event) {
                    let flash = &flashes[i];
                    flash.set(1.0);
                    let _ = flash.timing(0.0, 700.0, Easing::Curve(Curve::Quad, EaseMode::Out));
                }
            }
        }
    };
    // The chips' colors are the appearance's: made again when it changes.
    let chips = dynamic(move || {
        let (raised, fill) = (color(colors::RAISED), color(colors::ACCENT_FILL));
        PRESS_EVENTS
            .iter()
            .zip(flashes.iter())
            .map(|(name, flash)| {
                view()
                    .flex_1()
                    .h(px(30.0))
                    .rounded(px(15.0))
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(colors::BORDER)
                    .bind_background(&flash.interpolate_colors(&[0.0, 1.0], &[raised, fill]))
                    .finish_shadow(Finish::Raised)
                    .child(
                        text(*name)
                            .text_size(px(11.0))
                            .font_family(MONO)
                            .text_color(colors::TEXT),
                    )
            })
            .collect::<Vec<_>>()
    });
    // The underlay's color follows the appearance.
    let ripple = view()
        .on_press(fire("ripple"))
        .on_long_press(fire("fast long press"))
        .long_press_delay(250.0)
        .underlay(|| color(colors::BORDER), UNDERLAY_OPACITY)
        .ripple(Ripple {
            color: Some(rgb_a(0xc8f26d, 0.25)),
            ..Ripple::default()
        })
        .flex_1()
        .h(px(56.0))
        .rounded(px(radius::CONTROL))
        .border_curve(BorderCurve::Continuous)
        .bg(colors::RAISED)
        .finish(Finish::Raised)
        .border_1()
        .border_color(colors::BORDER)
        .items_center()
        .justify_center()
        .gap(px(2.0))
        .child(text("Ripple").text_size(px(14.0)).text_color(colors::TEXT))
        .child(
            text("Long press after 250 ms")
                .text_size(px(12.0))
                .text_color(colors::FAINT),
        );
    Card::new()
        .icon(IconName::Components)
        .title("Press events")
        .subtitle("In, press, long press and out; native feedback")
        .right(Badge::new(move || count.get().to_string()))
        .child(
            view()
                .on_press_in(fire("pressIn"))
                .on_press_out(fire("pressOut"))
                .on_press(fire("press"))
                .on_long_press(fire("longPress"))
                .active_opacity(0.55)
                .role(AccessibilityRole::Button)
                .h(px(64.0))
                .rounded(px(radius::CONTROL))
                .border_curve(BorderCurve::Continuous)
                .bg(colors::LIGHT)
                .finish(Finish::Light)
                .items_center()
                .justify_center()
                .child(
                    text("Tap, hold, or slide off")
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors::ON_LIGHT),
                ),
        )
        .child(view().flex_row().gap(px(6.0)).child(chips))
        .child(
            text(move || {
                log.with(|log| {
                    if log.is_empty() {
                        "No events yet".to_owned()
                    } else {
                        log.join(" · ")
                    }
                })
            })
            .number_of_lines(1)
            .text_size(px(12.0))
            .font_family(MONO)
            .text_color(colors::FAINT),
        )
        .child(
            view().flex_row().gap(px(10.0)).child(ripple).child(
                view()
                    .on_press(fire("borderless"))
                    .ripple(Ripple {
                        borderless: true,
                        radius: Some(40.0),
                        ..Ripple::default()
                    })
                    .active_opacity(0.4)
                    .role(AccessibilityRole::Button)
                    .label("Star")
                    .size(px(56.0))
                    .items_center()
                    .justify_center()
                    .child(icon(IconName::Star, 24.0, colors::WARN)),
            ),
        )
        .child(
            text("Pressable text, dimmed while pressed")
                .on_press(fire("text"))
                .active_opacity(0.3)
                .self_start()
                .text_size(px(15.0))
                .text_color(colors::ACCENT)
                .py_3(),
        )
}

/// A tiny dot with a ring showing the 30 pt around it that also takes taps.
#[component]
fn HitSlop() -> impl IntoElement {
    let taps = signal(0_u32);
    let ring = native_value(0.0);
    let sin = Easing::Curve(Curve::Sin, EaseMode::InOut);
    let breathe = Animation::sequence([
        Animation::timing(&ring, 1.0, Timing::new(1400.0, sin)),
        Animation::timing(&ring, 0.0, Timing::new(1400.0, sin)),
    ])
    .looped(None);
    let _ = breathe.start();
    cleanup(move || breathe.stop());
    Card::new()
        .icon(IconName::Search)
        .title("Hit slop")
        .subtitle("A 12 pt dot that takes taps 30 pt around it")
        .right(
            Badge::new(move || format!("{} taps", taps.get())).tone(move || {
                if taps.get() > 0 {
                    Tone::Accent
                } else {
                    Tone::Neutral
                }
            }),
        )
        .child(
            stage()
                .h(px(140.0))
                .items_center()
                .justify_center()
                .child(dots())
                .child(
                    view()
                        .pointer_events(PointerEvents::None)
                        .absolute()
                        .size(px(72.0))
                        .rounded(px(36.0))
                        .border_1()
                        .border_dashed()
                        .border_color(colors::DANGER)
                        .bg(tints::DANGER)
                        .bind_opacity(&ring.interpolate(&[0.0, 1.0], &[0.45, 1.0]))
                        .bind_scale(&ring.interpolate(&[0.0, 1.0], &[0.96, 1.04])),
                )
                .child(
                    press_with(
                        PressProps {
                            hit_slop: Some(30.0),
                            scale: 0.8.into(),
                            label: Some("Tiny dot".into()).into(),
                            ..PressProps::default()
                        },
                        move |_: &()| taps.update(|n| *n += 1),
                    )
                    .child(
                        view()
                            .size(px(12.0))
                            .rounded(px(6.0))
                            .bg(colors::DANGER)
                            .map(knob),
                    ),
                ),
        )
        .child(api("The dashed ring is the touch area", "hitSlop={30}"))
}

/// The items labelled for screen readers: label, hint, icon.
const LABELLED: [(&str, &str, IconName); 3] = [
    ("Play", "Starts the song", IconName::Play),
    ("Like", "Adds to favorites", IconName::Star),
    ("Notify", "Turns on alerts", IconName::Bell),
];

/// A hatched band across the middle of a `width` wide overlay, framed
/// with dashes.
fn hatch(width: f32) -> Painter {
    let height = 88.0;
    let mut p = Painter::new()
        .save()
        .begin_path()
        .rect(width * 0.18, 0.0, width * 0.64, height)
        .clip()
        .stroke_color(color(colors::WARN.opacity(0.28)))
        .line_width(1.0)
        .begin_path();
    let mut i = -height;
    while i < width {
        p = p.move_to(i, height).line_to(i + height, 0.0);
        i += 8.0;
    }
    p.stroke()
        .restore()
        .line_dash(&[4.0, 4.0])
        .stroke_color(color(colors::WARN))
        .begin_path()
        .round_rect(
            width * 0.18 + 0.5,
            0.5,
            width * 0.64 - 1.0,
            height - 1.0,
            10.0,
        )
        .stroke()
}

/// An overlay that lets taps through to the button under it.
fn pointer_events() -> impl IntoElement {
    // Taps through the overlay; the overlay's width, from its layout.
    let through = signal(0_u32);
    let overlay = signal(0.0_f32);
    Card::new()
        .icon(IconName::Nav)
        .title("Pointer events")
        .subtitle("An overlay that lets taps through")
        .right(Badge::new(move || through.get().to_string()).tone(Tone::Up))
        .child(
            view()
                .h(px(88.0))
                .on_layout(move |frame: &Rect| overlay.set(frame.width))
                .child(
                    press_with(
                        PressProps {
                            scale: 0.98.into(),
                            ..PressProps::default()
                        },
                        move |_: &()| through.update(|n| *n += 1),
                    )
                    .flex_1()
                    .child(
                        view()
                            .flex_1()
                            .rounded(px(radius::CONTROL))
                            .border_curve(BorderCurve::Continuous)
                            .bg(tints::UP)
                            .border_1()
                            .border_color(colors::UP)
                            .items_center()
                            .justify_center()
                            .child(
                                text("Button under an overlay")
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors::UP),
                            ),
                    ),
                )
                .child(
                    canvas(move || hatch(overlay.get()))
                        .pointer_events(PointerEvents::None)
                        .absolute()
                        .inset_0(),
                ),
        )
        .child(api(
            "Taps on the hatched band reach the button",
            "pointerEvents=\"none\"",
        ))
}

/// Elements labelled for VoiceOver / TalkBack.
fn accessibility() -> impl IntoElement {
    // The item pressed last.
    let last = signal(None::<&'static str>);
    Card::new()
        .icon(IconName::Text)
        .title("Accessibility")
        .subtitle("Roles, labels and hints for screen readers")
        .right(dynamic(move || {
            last.get().map(|last| Badge::new(last).tone(Tone::Info))
        }))
        .children(
            LABELLED
                .iter()
                .enumerate()
                .map(|(i, &(label, hint, name))| {
                    let pressed = computed(move || last.get() == Some(label));
                    view()
                        .accessible(true)
                        .role(AccessibilityRole::Button)
                        .label(label)
                        .hint(hint)
                        .on_press(move |_: &()| last.set(Some(label)))
                        .child(
                            Row::new(i == 0)
                                // The tile's tint follows the press.
                                .child(IconTile::new(name).size(34.0).tint(move || {
                                    if pressed.get() {
                                        colors::ACCENT
                                    } else {
                                        colors::TEXT
                                    }
                                }))
                                .child(
                                    view()
                                        .flex_1()
                                        .gap(px(1.0))
                                        .child(
                                            text(label)
                                                .text_size(px(15.0))
                                                .text_color(colors::TEXT),
                                        )
                                        .child(
                                            text(hint)
                                                .text_size(px(12.0))
                                                .text_color(colors::MUTED),
                                        ),
                                )
                                .child(
                                    text("button")
                                        .text_size(px(11.0))
                                        .font_family(MONO)
                                        .text_color(colors::FAINT),
                                ),
                        )
                }),
        )
}

/// The demo's page.
#[component]
pub(crate) fn Gestures() -> impl IntoElement {
    let cards: [AnyElement; 7] = [
        Transformable::element(()).into_element(),
        Ball::element(()).into_element(),
        TouchPad::element(()).into_element(),
        PressEvents::element(()).into_element(),
        HitSlop::element(()).into_element(),
        pointer_events().into_element(),
        accessibility().into_element(),
    ];
    view().flex_1().child(
        scroll().flex_1().bg(colors::BACKGROUND).child(
            view().p_4().pb(px(40.0)).gap_4().children(
                cards
                    .into_iter()
                    .enumerate()
                    .map(|(i, card)| appear(i as u32).child(card)),
            ),
        ),
    )
}
