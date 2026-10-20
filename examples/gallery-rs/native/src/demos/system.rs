//! System: the environment, the status bar, alerts and action sheets, a
//! toast, display frames and timers, the network and logs (as the TSX
//! gallery's `demos/System.tsx`).

use std::time::Duration;

use nsh_core::{
    keymap::When,
    nsh_animation::{Curve, EaseMode},
    time::Instant,
    ui::{AnyElement, Dialog, DialogButton, DialogButtonStyle, prelude::*},
};
use nsh_ui_mobile::{BarStyle, StatusBar, StatusBarProps};

use crate::ui::{
    Badge, Button, ButtonVariant, Card, DemoPage, Figure, Finish, Finished, IconName, IconTile,
    Inset, MONO, Meter, PressProps, Row, Segment, SegmentedEvent, Stat, ToggleEvent, Tone, appear,
    caption, colors, icon, inset_shadow, legend, press_with, rgb_a, segmented, tints, toggle,
};

/// The status bar's styles, in the segments' order.
const BAR_STYLES: [(BarStyle, &str, &str); 3] = [
    (BarStyle::LightContent, "light-content", "Light"),
    (BarStyle::DarkContent, "dark-content", "Dark"),
    (BarStyle::Default, "default", "Default"),
];

/// The posts the network card asks for.
const POSTS: &str = "https://jsonplaceholder.typicode.com/posts?_limit=3";

/// A dialog button.
fn choice(label: &str, style: DialogButtonStyle) -> DialogButton {
    DialogButton {
        text: label.into(),
        style,
    }
}

/// `labels[index]`, or `"dismissed"`.
fn chosen(labels: &[&str], index: i32) -> String {
    usize::try_from(index)
        .ok()
        .and_then(|i| labels.get(i))
        .map_or("dismissed", |l| l)
        .to_owned()
}

/// The action sheet's options.
const SHEET_OPTIONS: [&str; 4] = ["Cancel", "Copy link", "Save", "Delete"];

/// A text and the tone it shows in.
#[derive(Debug, Clone, PartialEq)]
struct Toned {
    text: String,
    tone: Tone,
}

/// The posts of a JSON array of `{ id, title }`.
fn parse_posts(body: &str) -> Result<Vec<(u64, String)>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let posts = value.as_array().ok_or("not an array")?;
    Ok(posts
        .iter()
        .map(|post| {
            (
                post["id"].as_u64().unwrap_or_default(),
                post["title"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect())
}

/// A thin rule between the parts of a card.
fn rule() -> AnyElement {
    view().h(px(1.0)).bg(colors::LINE).into_element()
}

/// A small label above a figure.
fn label(content: &'static str) -> Text {
    text(content).text_size(px(12.0)).text_color(colors::MUTED)
}

/// The window drawn to scale, with the safe area insets and the keyboard
/// shaded (bound: it follows the window, the insets and the keyboard).
fn safe_area_diagram() -> AnyElement {
    let width = 76.0;
    let band = move |pt: f32| {
        px((pt * width / window().width.max(px(1.0)).0)
            .round()
            .max(0.0))
    };
    view()
        .w(px(width))
        .h(move || px((window().height * width / window().width.max(px(1.0))).round()))
        .rounded(px(14.0))
        .border_curve(BorderCurve::Continuous)
        .border(1.5)
        .border_color(colors::BORDER)
        .bg(colors::SURFACE)
        .finish_fill(Finish::Card)
        .adaptive_shadow(
            [inset_shadow(1.0, 0.0, rgb(0xffffff))],
            [inset_shadow(1.0, 0.0, rgb_a(0xffffff, 0.06))],
        )
        .overflow_hidden()
        .child(view().h(move || band(safe_area().top)).bg(tints::INFO))
        .child(
            view()
                .flex_1()
                .m(px(6.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(colors::LINE)
                .border_dashed(),
        )
        .child(view().h(move || band(keyboard_height())).bg(tints::WARN))
        .child(view().h(move || band(safe_area().bottom)).bg(tints::INFO))
        .into_element()
}

/// The environment: every value is read in a binding.
fn environment() -> AnyElement {
    type Edge = fn(EdgeInsets) -> f32;
    let insets: [(&str, Edge); 4] = [
        ("Top", |i| i.top),
        ("Right", |i| i.right),
        ("Bottom", |i| i.bottom),
        ("Left", |i| i.left),
    ];
    let insets = insets.map(|(label, value)| {
        Stat::new(label, move || value(safe_area()).round().to_string()).width(0.25)
    });
    let keyboard_stat =
        || Stat::new("Keyboard", || format!("{} pt", keyboard_height().round())).width(0.5);
    Card::new()
        .icon(IconName::System)
        .title("Environment")
        .subtitle("Updates when the window, insets or keyboard change")
        .right(Badge::new("Live").tone(Tone::Up))
        .child(
            view()
                .flex_row()
                .gap_4()
                .items_center()
                .child(
                    view()
                        .flex_1()
                        .gap_3()
                        .child(
                            view().gap(px(2.0)).child(label("Window")).child(
                                Figure::new(|| {
                                    let window = window();
                                    format!(
                                        "{} × {}",
                                        window.width.0.round(),
                                        window.height.0.round()
                                    )
                                })
                                .unit("pt"),
                            ),
                        )
                        .child(
                            view()
                                .flex_row()
                                .child(
                                    If::new(
                                        || keyboard_height() > 0.0,
                                        move || keyboard_stat().tone(Tone::Warn),
                                    )
                                    .otherwise(keyboard_stat),
                                )
                                .child(
                                    Stat::new("Font scale", || format!("{:.2}×", font_scale()))
                                        .width(0.5),
                                ),
                        ),
                )
                .child(safe_area_diagram()),
        )
        .child(rule())
        .child(
            view()
                .gap_1()
                .child(caption("Safe area insets", Some("pt")))
                .child(view().flex_row().children(insets)),
        )
        .child(rule())
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_4()
                .child(legend(colors::INFO, "Safe area", None::<&str>))
                .child(legend(colors::WARN, "Keyboard", None::<&str>))
                .child(view().flex_1())
                .child(
                    Badge::new(|| {
                        if dark() {
                            "Dark scheme"
                        } else {
                            "Light scheme"
                        }
                    })
                    .tone(|| if dark() { Tone::Info } else { Tone::Warn })
                    .outline(),
                ),
        )
        .into_element()
}

/// The status bar: its style (following the appearance until one is
/// picked) and whether it is hidden.
#[component]
fn StatusBarCard() -> impl IntoElement {
    // The style the user picked.
    let picked = signal(None::<usize>);
    let hidden = signal(false);
    // Until one is picked, the style follows the appearance (light text
    // on the dark page).
    let index = computed(move || picked.get().unwrap_or(if dark() { 0 } else { 1 }));
    Card::new()
        .icon(IconName::Mobile)
        .title("Status bar")
        .subtitle("StatusBar: style and visibility, animated")
        .child(StatusBar::element(StatusBarProps {
            bar_style: Prop::bind(move || Some(BAR_STYLES[index.get()].0)),
            hidden: Prop::bind(move || Some(hidden.get())),
            animated: Some(true).into(),
            android: None.into(),
            web: None.into(),
        }))
        .child(
            segmented(
                BAR_STYLES.map(|(_, _, label)| Segment::new(label)),
                Prop::bind(move || index.get() as u32),
            )
            .on_event(move |event: &SegmentedEvent| {
                let SegmentedEvent::Change(i) = *event;
                picked.set(Some(i as usize));
            }),
        )
        .child(
            Row::new(false)
                .child(
                    view()
                        .flex_1()
                        .gap(px(2.0))
                        .child(text("Hidden").text_size(px(15.0)).text_color(colors::TEXT))
                        .child(
                            text(move || format!("barStyle: \"{}\"", BAR_STYLES[index.get()].1))
                                .text_size(px(12.0))
                                .text_color(colors::MUTED)
                                .font_family(MONO),
                        ),
                )
                .child(toggle(hidden, "Hide the status bar").on_event(
                    move |event: &ToggleEvent| {
                        let ToggleEvent::Change(on) = *event;
                        hidden.set(on);
                    },
                )),
        )
}

/// A tappable row with an icon tile, a title, a detail and a chevron.
fn action_row(
    first: bool,
    name: IconName,
    tint: AdaptiveColor,
    title: &'static str,
    detail: &'static str,
    on_press: impl Into<Listener<()>>,
) -> AnyElement {
    press_with(
        PressProps {
            scale: 0.98.into(),
            label: Some(title.into()).into(),
            ..PressProps::default()
        },
        on_press,
    )
    .child(
        Row::new(first)
            .child(IconTile::new(name).tint(tint).size(34.0))
            .child(
                view()
                    .flex_1()
                    .gap(px(1.0))
                    .py_2()
                    .child(text(title).text_size(px(15.0)).text_color(colors::TEXT))
                    .child(text(detail).text_size(px(12.0)).text_color(colors::MUTED)),
            )
            .child(icon(IconName::Chevron, 14.0, colors::FAINT)),
    )
    .into_element()
}

/// System dialogs and a toast; the last answer pops in.
#[component]
fn Dialogs() -> impl IntoElement {
    // What the last dialog answered.
    let answer = signal(None::<Toned>);
    // Pops the answer in each time it changes.
    let pop = animated_value(1.0);
    let owner = Owner::current();
    // Shows `dialog`, then what `answered` makes of the index chosen (-1:
    // dismissed), popping it in.
    let ask = move |dialog: Dialog, answered: fn(i32) -> Toned| {
        let chosen = nsh_core::ui::prelude::dialog(dialog);
        owner.run(|| {
            spawn(async move {
                let index = chosen.await;
                answer.set(Some(answered(index)));
                pop.set(0.85);
                pop.spring(
                    1.0,
                    Spring {
                        stiffness: 380.0,
                        damping: 18.0,
                        ..Spring::default()
                    },
                );
            })
        });
    };
    let alert = move |_: &()| {
        let dialog = Dialog {
            id: 0,
            title: Some("Saved".into()),
            message: Some("Your changes were saved.".into()),
            buttons: vec![choice("OK", DialogButtonStyle::Default)],
            cancelable: false,
            sheet: false,
        };
        ask(dialog, |_| Toned {
            text: "OK".to_owned(),
            tone: Tone::Up,
        });
    };
    let confirm = move |_: &()| {
        let dialog = Dialog {
            id: 0,
            title: Some("Delete the photo?".into()),
            message: Some("This cannot be undone.".into()),
            buttons: vec![
                choice("Cancel", DialogButtonStyle::Cancel),
                choice("Delete", DialogButtonStyle::Destructive),
            ],
            cancelable: true,
            sheet: false,
        };
        ask(dialog, |i| {
            let text = chosen(&["Cancel", "Delete"], i);
            Toned {
                tone: if text == "Delete" {
                    Tone::Danger
                } else {
                    Tone::Neutral
                },
                text,
            }
        });
    };
    let three = move |_: &()| {
        let dialog = Dialog {
            id: 0,
            title: Some("Update available".into()),
            message: Some("Install it now?".into()),
            buttons: vec![
                choice("Later", DialogButtonStyle::Default),
                choice("Never", DialogButtonStyle::Cancel),
                choice("Install", DialogButtonStyle::Default),
            ],
            cancelable: false,
            sheet: false,
        };
        ask(dialog, |i| {
            let text = chosen(&["Later", "Never", "Install"], i);
            Toned {
                tone: if text == "Install" {
                    Tone::Info
                } else {
                    Tone::Neutral
                },
                text,
            }
        });
    };
    let sheet = move |_: &()| {
        let dialog = Dialog {
            id: 0,
            title: Some("Share the photo".into()),
            message: None,
            buttons: SHEET_OPTIONS
                .iter()
                .enumerate()
                .map(|(i, option)| {
                    choice(
                        option,
                        match i {
                            0 => DialogButtonStyle::Cancel,
                            3 => DialogButtonStyle::Destructive,
                            _ => DialogButtonStyle::Default,
                        },
                    )
                })
                .collect(),
            cancelable: true,
            sheet: true,
        };
        // Dismissing the sheet is its cancel action (as `ActionSheet.show`
        // answers).
        ask(dialog, |i| {
            let text = chosen(&SHEET_OPTIONS, i.max(0));
            Toned {
                tone: if text == "Delete" {
                    Tone::Danger
                } else {
                    Tone::Accent
                },
                text,
            }
        });
    };
    let shown = If::new(
        move || answer.with(Option::is_some),
        move || {
            view().bind_scale(&pop).child(
                Badge::new(move || {
                    answer.with(|a| a.as_ref().map(|a| a.text.clone()).unwrap_or_default())
                })
                .tone(move || answer.with(|a| a.as_ref().map_or(Tone::Neutral, |a| a.tone))),
            )
        },
    )
    .otherwise(|| {
        text("None yet")
            .text_size(px(13.0))
            .text_color(colors::FAINT)
    });
    Card::new()
        .icon(IconName::Bell)
        .title("Alerts and sheets")
        .subtitle("Alert · ActionSheet · Toast")
        .flush()
        .child(
            Inset::new()
                .gap(0.0)
                .padding(12.0)
                .child(action_row(
                    true,
                    IconName::Check,
                    colors::UP,
                    "Alert",
                    "One button",
                    alert,
                ))
                .child(action_row(
                    false,
                    IconName::Bell,
                    colors::DANGER,
                    "Confirm",
                    "Cancel and destructive buttons",
                    confirm,
                ))
                .child(action_row(
                    false,
                    IconName::Star,
                    colors::INFO,
                    "Three buttons",
                    "Resolves to the index pressed",
                    three,
                ))
                .child(action_row(
                    false,
                    IconName::Lists,
                    colors::ACCENT,
                    "Action sheet",
                    "Title, cancel and destructive options",
                    sheet,
                ))
                .child(action_row(
                    false,
                    IconName::Spark,
                    colors::WARN,
                    "Toast",
                    "Android only",
                    |_: &()| toast("Hello from a toast", ToastDuration::Long),
                )),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .px(px(11.0))
                .pt_3()
                .pb(px(11.0))
                .min_h(px(44.0))
                .child(
                    text("Answer")
                        .flex_1()
                        .text_size(px(13.0))
                        .text_color(colors::MUTED),
                )
                .child(shown),
        )
}

/// Display frames counted while asked, and an interval's ticks.
#[component]
fn FramesAndTimers() -> impl IntoElement {
    // Seconds since the page opened, counted by an interval.
    let ticks = signal(0_u32);
    // Flashes the beat dot once per tick (the fade runs natively).
    let beat = animated_value(0.0);
    set_interval(Duration::from_secs(1), move || {
        ticks.update(|t| *t += 1);
        beat.set(1.0);
        beat.timing(0.0, 700.0, Easing::Curve(Curve::Quad, EaseMode::Out));
    });
    // Counts the display frames while it runs ("Count frames").
    let frames = stored(None::<Spawned<()>>);
    let running = signal(false);
    // Frames since `since`, and when they started.
    let counted = stored((0_u32, None::<Instant>));
    // Frames per second, over the last second.
    let fps = signal(0_u32);
    let owner = Owner::current();
    // A frame began at `at`: once a second, the rate.
    let frame = move |at: Instant| {
        let rate = counted.with(|(count, since)| {
            *count += 1;
            let start = *since.get_or_insert(at);
            let elapsed = at.saturating_duration_since(start).as_secs_f64();
            (elapsed >= 1.0).then(|| {
                let rate = (f64::from(*count) / elapsed).round() as u32;
                *count = 0;
                *since = Some(at);
                rate
            })
        });
        if let Some(rate) = rate {
            fps.set(rate);
        }
    };
    // Starts or stops counting frames: a task awaits each display frame
    // (TS: `requestAnimationFrame` asking again from its callback).
    let toggle_frames = move |_: &()| {
        if let Some(task) = frames.replace(None) {
            task.cancel();
            running.set(false);
            return;
        }
        counted.replace((0, Some(Instant::now())));
        let task = owner.run(|| {
            spawn(async move {
                loop {
                    frame(next_frame().await);
                }
            })
        });
        frames.replace(Some(task));
        running.set(true);
    };
    Card::new()
        .icon(IconName::Animation)
        .title("Frames and timers")
        .subtitle("requestAnimationFrame · setInterval")
        .child(
            view()
                .flex_row()
                .gap_3()
                .child(
                    view()
                        .flex_1()
                        .gap(px(2.0))
                        .child(label("Frame rate"))
                        .child(
                            Figure::new(move || {
                                if running.get() {
                                    fps.get().to_string()
                                } else {
                                    "–".to_owned()
                                }
                            })
                            .unit("fps")
                            .note(move || if running.get() { "live" } else { "stopped" })
                            .tone(move || {
                                if running.get() {
                                    Tone::Up
                                } else {
                                    Tone::Neutral
                                }
                            }),
                        ),
                )
                .child(view().w(px(1.0)).bg(colors::LINE))
                .child(
                    view()
                        .flex_1()
                        .gap(px(2.0))
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap(px(6.0))
                                .child(label("Interval ticks"))
                                .child(
                                    view()
                                        .size(px(7.0))
                                        .rounded(px(3.5))
                                        .bg(colors::ACCENT)
                                        .bind_opacity(&beat.interpolate(&[0.0, 1.0], &[0.25, 1.0]))
                                        .bind_scale(&beat.interpolate(&[0.0, 1.0], &[1.0, 1.6])),
                                ),
                        )
                        .child(
                            Figure::new(move || ticks.get().to_string())
                                .note("every 1 s")
                                .tone(Tone::Accent),
                        ),
                ),
        )
        .child(
            view()
                .gap_2()
                .child(caption(
                    "Share of 120 fps",
                    Some(move || {
                        if running.get() {
                            format!("{}%", (f64::from(fps.get()) / 120.0 * 100.0).round())
                        } else {
                            "–".to_owned()
                        }
                    }),
                ))
                .child(
                    Meter::new(move || {
                        if running.get() {
                            fps.get() as f32 / 120.0
                        } else {
                            0.0
                        }
                    })
                    .tone(Tone::Up)
                    .marker(0.5),
                ),
        )
        .child(
            Button::new(
                move || {
                    if running.get() {
                        "Stop counting frames"
                    } else {
                        "Count frames"
                    }
                },
                toggle_frames,
            )
            .icon(move || {
                if running.get() {
                    IconName::Pause
                } else {
                    IconName::Play
                }
            }),
        )
}

/// Three posts loaded over HTTP, and a log line.
#[component]
fn Network() -> impl IntoElement {
    // The posts loaded: id and title.
    let posts = signal(Vec::<(u64, String)>::new());
    // How the last request went.
    let status = signal(None::<Toned>);
    // A request is in progress.
    let loading = signal(false);
    let owner = Owner::current();
    // Loads three posts (TS: `fetch`) with the engine's network stack: the
    // platform's, or the browser's on the web.
    let load = move |_: &()| {
        batch(|| {
            status.set(Some(Toned {
                text: "loading…".to_owned(),
                tone: Tone::Neutral,
            }));
            loading.set(true);
        });
        owner.run(|| {
            spawn(async move {
                let started = Instant::now();
                let result = fetch(POSTS)
                    .await
                    .map(|response| (response.status(), response.text()));
                let ms = started.elapsed().as_millis();
                batch(|| {
                    loading.set(false);
                    status.set(Some(
                        match result.and_then(|(code, body)| Ok((code, parse_posts(&body)?))) {
                            Ok((code, loaded)) => {
                                posts.set(loaded);
                                Toned {
                                    text: format!("HTTP {code} · {ms} ms"),
                                    tone: if (200..300).contains(&code) {
                                        Tone::Up
                                    } else {
                                        Tone::Danger
                                    },
                                }
                            }
                            Err(error) => Toned {
                                text: error,
                                tone: Tone::Danger,
                            },
                        },
                    ));
                });
            })
        });
    };
    let list = If::new(
        move || posts.with(Vec::is_empty),
        || {
            caption(
                "Three posts from jsonplaceholder.typicode.com",
                None::<&str>,
            )
        },
    )
    .otherwise(move || {
        view().child(Each::new(
            move || {
                posts.with(|p| {
                    p.iter()
                        .cloned()
                        .enumerate()
                        .collect::<Vec<(usize, (u64, String))>>()
                })
            },
            |(_, (id, _))| *id,
            |post, _| {
                let i = post.with_untracked(|(i, _)| *i);
                appear(i as u32).child(
                    Row::new(i == 0)
                        .child(
                            text(move || post.with(|(_, (id, _))| format!("#{id}")))
                                .w(px(28.0))
                                .text_size(px(13.0))
                                .text_color(colors::ACCENT)
                                .tabular_nums(),
                        )
                        .child(
                            text(move || post.with(|(_, (_, title))| title.clone()))
                                .number_of_lines(1)
                                .flex_1()
                                .text_size(px(15.0))
                                .text_color(colors::TEXT),
                        ),
                )
            },
        ))
    });
    let status_row = dynamic(move || {
        status.get().map(|status| {
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    view()
                        .size(px(7.0))
                        .rounded(px(3.5))
                        .bg(status.tone.color()),
                )
                .child(
                    text(status.text)
                        .number_of_lines(2)
                        .flex_1()
                        .text_size(px(13.0))
                        .text_color(status.tone.color())
                        .tabular_nums(),
                )
        })
    });
    Card::new()
        .icon(IconName::Search)
        .title("Network and logs")
        .subtitle("fetch · log::info!")
        .child(list)
        .child(status_row)
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(
                    Button::new(
                        move || {
                            if loading.get() {
                                "Loading…"
                            } else {
                                "fetch()"
                            }
                        },
                        load,
                    )
                    .icon(IconName::ArrowDown)
                    .disabled(loading)
                    .flex_1(),
                )
                .child(
                    Button::new("log::info!", |_: &()| {
                        log::info!(target: "gallery", "hello from the gallery");
                    })
                    .variant(ButtonVariant::Secondary)
                    .flex_1(),
                ),
        )
        .child(
            text("log::info! shows in the terminal of nsh run.")
                .text_size(px(12.0))
                .text_color(colors::FAINT),
        )
}

/// App-level shortcuts: bound while the demo shows (the component's own).
#[component]
fn Shortcuts() -> impl IntoElement {
    let count = signal(0_u32);
    let on = signal(false);
    let last = signal(SharedString::default());
    on_key("mod+k", move |k| {
        count.update(|c| *c += 1);
        last.set(k.to_owned().into());
    });
    on_key("mod+shift+l", move |k| {
        on.update(|on| *on = !*on);
        last.set(k.to_owned().into());
    });
    on_key_with(
        "mod+backspace",
        KeyOptions {
            when: When::NotEditing,
            ..KeyOptions::default()
        },
        move |k| {
            count.set(0);
            last.set(k.to_owned().into());
        },
    );
    let keys = [
        (key_label("mod+k"), "Counts"),
        (key_label("mod+shift+l"), "Switches the light"),
        (key_label("mod+backspace"), "Resets (not while typing)"),
    ];
    Card::new()
        .icon(IconName::Bell)
        .title("Keyboard shortcuts")
        .subtitle("on_key · desktop")
        .child(
            view().children(keys.into_iter().enumerate().map(|(i, (key, what))| {
                Row::new(i == 0)
                    .child(
                        text(key)
                            .w(px(120.0))
                            .text_size(px(14.0))
                            .text_color(colors::ACCENT)
                            .font_family(MONO),
                    )
                    .child(
                        text(what)
                            .flex_1()
                            .text_size(px(15.0))
                            .text_color(colors::TEXT),
                    )
            })),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    Badge::new(move || if on.get() { "Light on" } else { "Light off" })
                        .tone(move || if on.get() { Tone::Up } else { Tone::Neutral }),
                )
                .child(view().flex_1().child(caption(
                    move || format!("Counted {}", count.get()),
                    Some(move || {
                        let last = last.get();
                        if last.is_empty() { "–".into() } else { last }
                    }),
                ))),
        )
}

/// The demo's page.
#[component]
pub(crate) fn System() -> impl IntoElement {
    DemoPage::new()
        .card(environment)
        .card(|| StatusBarCard::element(()))
        .card(|| Dialogs::element(()))
        .card(|| FramesAndTimers::element(()))
        .card(|| Shortcuts::element(()))
        .card(|| Network::element(()))
}
