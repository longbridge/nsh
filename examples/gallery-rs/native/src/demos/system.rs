//! System: the environment, the status bar, alerts and action sheets, a
//! toast, display frames and timers, the network and logs (as the TSX
//! gallery's `demos/System.tsx`).

use std::time::{Duration, Instant};

use nsh_core::{
    nsh_animation::{Curve, EaseMode},
    ui::{AnyElement, Dialog, DialogButton, DialogButtonStyle, prelude::*},
};
use nsh_ui_mobile::{BarStyle, StatusBar, StatusBarProps};

use crate::ui::{
    Badge, Button, ButtonVariant, Card, Figure, Finish, Finished, IconName, IconTile, Inset, MONO,
    Meter, PressProps, Row, Segment, SegmentedEvent, Stat, ToggleEvent, Tone, appear, caption,
    colors, icon, inset_shadow, legend, press_with, rgb_a, segmented, tints, toggle,
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

/// A text and the tone it shows in.
#[derive(Debug, Clone)]
struct Toned {
    text: String,
    tone: Tone,
}

/// The demo's page.
#[derive(Default)]
pub(crate) struct System {
    /// The status bar's style the user picked (until then, it follows the
    /// appearance).
    picked: Option<usize>,
    /// The status bar is hidden.
    bar_hidden: bool,
    /// What the last dialog answered.
    answer: Option<Toned>,
    /// Pops the answer in each time it changes.
    pop: Option<NativeValue>,
    /// Seconds since the page opened, counted by an interval.
    ticks: u32,
    /// The interval counting `ticks` (dropped with the page: cleared).
    timer: Option<Task<()>>,
    /// Flashes the beat dot once per tick (the fade runs natively).
    beat: Option<NativeValue>,
    /// Counts the display frames while it runs ("Count frames").
    frames: Option<Task<()>>,
    /// Frames since `since`.
    frame_count: u32,
    /// When the frames counted started.
    since: Option<Instant>,
    /// Frames per second, over the last second.
    fps: u32,
    /// The posts loaded: id and title.
    posts: Vec<(u64, String)>,
    /// How the last request went.
    status: Option<Toned>,
    /// The request in progress.
    request: Option<Task<()>>,
}

impl System {
    /// Shows `dialog`, then shows what `answer` makes of the index chosen
    /// (-1: dismissed).
    fn ask(
        cx: &mut Context<'_, Self>,
        dialog: Dialog,
        answer: impl FnOnce(i32) -> Toned + 'static,
    ) {
        let chosen = cx.dialog(dialog);
        cx.spawn(|this, _| async move {
            let index = chosen.await;
            this.update(move |this, cx| this.show(answer(index), cx))
                .await;
        })
        .detach();
    }

    /// Shows `answer`, popping it in.
    fn show(&mut self, answer: Toned, cx: &mut Context<'_, Self>) {
        self.answer = Some(answer);
        let pop = self.pop.as_ref().expect("set up");
        pop.set(0.85);
        pop.spring(
            1.0,
            Spring {
                stiffness: 380.0,
                damping: 18.0,
                ..Spring::default()
            },
        );
        cx.notify();
    }

    /// Starts or stops counting frames: a task awaits each display frame
    /// (TS: `requestAnimationFrame` asking again from its callback).
    fn toggle_frames(&mut self, cx: &mut Context<'_, Self>) {
        if self.frames.take().is_some() {
            cx.notify();
            return;
        }
        self.frame_count = 0;
        self.since = Some(Instant::now());
        self.frames = Some(cx.spawn(|this, cx| async move {
            loop {
                let at = cx.next_frame().await;
                if this
                    .update(move |this, cx| this.frame(at, cx))
                    .await
                    .is_none()
                {
                    break;
                }
            }
        }));
        cx.notify();
    }

    /// A frame began at `at`: once a second, the rate.
    fn frame(&mut self, at: Instant, cx: &mut Context<'_, Self>) {
        self.frame_count += 1;
        let since = *self.since.get_or_insert(at);
        let elapsed = at.saturating_duration_since(since).as_secs_f64();
        if elapsed >= 1.0 {
            self.fps = (f64::from(self.frame_count) / elapsed).round() as u32;
            self.frame_count = 0;
            self.since = Some(at);
            cx.notify();
        }
    }

    /// Loads three posts (TS: `fetch`, a web API of TS apps; a Rust
    /// component uses an HTTP crate on the runtime's blocking threads).
    fn load(&mut self, cx: &mut Context<'_, Self>) {
        self.status = Some(Toned {
            text: "loading…".to_owned(),
            tone: Tone::Neutral,
        });
        cx.notify();
        self.request = Some(cx.spawn(|this, cx| async move {
            let started = Instant::now();
            let response = cx.blocking(|| -> Result<(u16, String), String> {
                let mut response = ureq::get(POSTS).call().map_err(|e| e.to_string())?;
                let status = response.status().as_u16();
                let body = response
                    .body_mut()
                    .read_to_string()
                    .map_err(|e| e.to_string())?;
                Ok((status, body))
            });
            let result = match response.await {
                Ok(result) => result,
                Err(error) => Err(error.to_string()),
            };
            let ms = started.elapsed().as_millis();
            this.update(move |this, cx| {
                this.request = None;
                this.status = Some(
                    match result.and_then(|(status, body)| Ok((status, parse_posts(&body)?))) {
                        Ok((status, posts)) => {
                            this.posts = posts;
                            Toned {
                                text: format!("HTTP {status} · {ms} ms"),
                                tone: if (200..300).contains(&status) {
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
                );
                cx.notify();
            })
            .await;
        }));
    }
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

impl System {
    /// The window drawn to scale, with the safe area insets shaded.
    fn safe_area_diagram(cx: &mut RenderCx<'_, Self>) -> AnyElement {
        let width = 76.0;
        let window = cx.window();
        let safe = cx.safe_area();
        let keyboard = cx.keyboard_height();
        let scale = width / window.width.max(1.0);
        let band = |pt: f32| (pt * scale).round().max(0.0);
        view()
            .w(px(width))
            .h(px((window.height * scale).round()))
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
            .child(view().h(px(band(safe.top))).bg(tints::INFO))
            .child(
                view()
                    .flex_1()
                    .m(px(6.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(colors::LINE)
                    .border_dashed(),
            )
            .child(view().h(px(band(keyboard))).bg(tints::WARN))
            .child(view().h(px(band(safe.bottom))).bg(tints::INFO))
            .into_element()
    }

    fn environment(cx: &mut RenderCx<'_, Self>) -> AnyElement {
        let window = cx.window();
        let safe = cx.safe_area();
        let keyboard = cx.keyboard_height();
        let font_scale = cx.font_scale();
        let dark = cx.dark();
        let insets = [
            ("Top", safe.top),
            ("Right", safe.right),
            ("Bottom", safe.bottom),
            ("Left", safe.left),
        ]
        .map(|(label, value)| Stat::new(label, value.round().to_string()).width(0.25));
        let keyboard_stat = Stat::new("Keyboard", format!("{} pt", keyboard.round())).width(0.5);
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
                                    Figure::new(format!(
                                        "{} × {}",
                                        window.width.round(),
                                        window.height.round()
                                    ))
                                    .unit("pt"),
                                ),
                            )
                            .child(
                                view()
                                    .flex_row()
                                    .child(if keyboard > 0.0 {
                                        keyboard_stat.tone(Tone::Warn)
                                    } else {
                                        keyboard_stat
                                    })
                                    .child(
                                        Stat::new("Font scale", format!("{font_scale:.2}×"))
                                            .width(0.5),
                                    ),
                            ),
                    )
                    .child(Self::safe_area_diagram(cx)),
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
                        Badge::new(if dark { "Dark scheme" } else { "Light scheme" })
                            .tone(if dark { Tone::Info } else { Tone::Warn })
                            .outline(),
                    ),
            )
            .into_element()
    }

    fn status_bar(&self, cx: &mut RenderCx<'_, Self>) -> AnyElement {
        // Until one is picked, the style follows the appearance (light
        // text on the dark page).
        let index = self.picked.unwrap_or(if cx.dark() { 0 } else { 1 });
        let (style, name, _) = BAR_STYLES[index];
        Card::new()
            .icon(IconName::Mobile)
            .title("Status bar")
            .subtitle("StatusBar: style and visibility, animated")
            .child(StatusBar::element(StatusBarProps {
                bar_style: Some(style),
                hidden: Some(self.bar_hidden),
                animated: Some(true),
                android: None,
                web: None,
            }))
            .child(
                segmented(
                    BAR_STYLES.map(|(_, _, label)| Segment::new(label)),
                    index as u32,
                )
                .on_event(cx.listener(
                    |this: &mut Self, event: &SegmentedEvent, cx| {
                        let SegmentedEvent::Change(i) = *event;
                        this.picked = Some(i as usize);
                        cx.notify();
                    },
                )),
            )
            .child(
                Row::new(false)
                    .child(
                        view()
                            .flex_1()
                            .gap(px(2.0))
                            .child(text("Hidden").text_size(px(15.0)).text_color(colors::TEXT))
                            .child(
                                text(format!("barStyle: \"{name}\""))
                                    .text_size(px(12.0))
                                    .text_color(colors::MUTED)
                                    .font_family(MONO),
                            ),
                    )
                    .child(
                        toggle(self.bar_hidden, "Hide the status bar").on_event(cx.listener(
                            |this: &mut Self, event: &ToggleEvent, cx| {
                                let ToggleEvent::Change(hidden) = *event;
                                this.bar_hidden = hidden;
                                cx.notify();
                            },
                        )),
                    ),
            )
            .into_element()
    }

    /// A tappable row with an icon tile, a title, a detail and a chevron.
    fn action_row(
        first: bool,
        name: IconName,
        tint: AdaptiveColor,
        title: &'static str,
        detail: &'static str,
        on_press: Listener<()>,
    ) -> AnyElement {
        press_with(
            PressProps {
                scale: 0.98,
                label: Some(title.into()),
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

    fn dialogs(&self, cx: &mut RenderCx<'_, Self>) -> AnyElement {
        let alert = cx.listener(|_: &mut Self, _, cx| {
            let dialog = Dialog {
                id: 0,
                title: Some("Saved".into()),
                message: Some("Your changes were saved.".into()),
                buttons: vec![choice("OK", DialogButtonStyle::Default)],
                cancelable: false,
                sheet: false,
            };
            Self::ask(cx, dialog, |_| Toned {
                text: "OK".to_owned(),
                tone: Tone::Up,
            });
        });
        let confirm = cx.listener(|_: &mut Self, _, cx| {
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
            Self::ask(cx, dialog, |i| {
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
        });
        let three = cx.listener(|_: &mut Self, _, cx| {
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
            Self::ask(cx, dialog, |i| {
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
        });
        let sheet = cx.listener(|_: &mut Self, _, cx| {
            let options = ["Cancel", "Copy link", "Save", "Delete"];
            let dialog = Dialog {
                id: 0,
                title: Some("Share the photo".into()),
                message: None,
                buttons: options
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
            // Dismissing the sheet is its cancel action (as
            // `ActionSheet.show` answers).
            Self::ask(cx, dialog, move |i| {
                let text = chosen(&options, i.max(0));
                Toned {
                    tone: if text == "Delete" {
                        Tone::Danger
                    } else {
                        Tone::Accent
                    },
                    text,
                }
            });
        });
        let toast = cx.listener(|_: &mut Self, _, cx| {
            cx.toast("Hello from a toast", ToastDuration::Long);
        });
        let answer = match &self.answer {
            Some(answer) => view()
                .bind_scale(self.pop.as_ref().expect("set up"))
                .child(Badge::new(answer.text.clone()).tone(answer.tone))
                .into_element(),
            None => text("None yet")
                .text_size(px(13.0))
                .text_color(colors::FAINT)
                .into_element(),
        };
        Card::new()
            .icon(IconName::Bell)
            .title("Alerts and sheets")
            .subtitle("Alert · ActionSheet · Toast")
            .flush()
            .child(
                Inset::new()
                    .gap(0.0)
                    .padding(12.0)
                    .child(Self::action_row(
                        true,
                        IconName::Check,
                        colors::UP,
                        "Alert",
                        "One button",
                        alert,
                    ))
                    .child(Self::action_row(
                        false,
                        IconName::Bell,
                        colors::DANGER,
                        "Confirm",
                        "Cancel and destructive buttons",
                        confirm,
                    ))
                    .child(Self::action_row(
                        false,
                        IconName::Star,
                        colors::INFO,
                        "Three buttons",
                        "Resolves to the index pressed",
                        three,
                    ))
                    .child(Self::action_row(
                        false,
                        IconName::Lists,
                        colors::ACCENT,
                        "Action sheet",
                        "Title, cancel and destructive options",
                        sheet,
                    ))
                    .child(Self::action_row(
                        false,
                        IconName::Spark,
                        colors::WARN,
                        "Toast",
                        "Android only",
                        toast,
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
                    .child(answer),
            )
            .into_element()
    }

    fn frames_and_timers(&self, cx: &mut RenderCx<'_, Self>) -> AnyElement {
        let running = self.frames.is_some();
        let beat = self.beat.as_ref().expect("set up");
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
                                Figure::new(if running {
                                    self.fps.to_string()
                                } else {
                                    "–".to_owned()
                                })
                                .unit("fps")
                                .note(if running { "live" } else { "stopped" })
                                .tone(if running {
                                    Tone::Up
                                } else {
                                    Tone::Neutral
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
                                            .bind_opacity(
                                                &beat.interpolate(&[0.0, 1.0], &[0.25, 1.0]),
                                            )
                                            .bind_scale(
                                                &beat.interpolate(&[0.0, 1.0], &[1.0, 1.6]),
                                            ),
                                    ),
                            )
                            .child(
                                Figure::new(self.ticks.to_string())
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
                        Some(if running {
                            format!("{}%", (f64::from(self.fps) / 120.0 * 100.0).round())
                        } else {
                            "–".to_owned()
                        }),
                    ))
                    .child(
                        Meter::new(if running {
                            self.fps as f32 / 120.0
                        } else {
                            0.0
                        })
                        .tone(Tone::Up)
                        .marker(0.5),
                    ),
            )
            .child(
                Button::new(
                    if running {
                        "Stop counting frames"
                    } else {
                        "Count frames"
                    },
                    cx.listener(|this: &mut Self, _, cx| this.toggle_frames(cx)),
                )
                .icon(if running {
                    IconName::Pause
                } else {
                    IconName::Play
                }),
            )
            .into_element()
    }

    fn network(&self, cx: &mut RenderCx<'_, Self>) -> AnyElement {
        let loading = self.request.is_some();
        let posts = if self.posts.is_empty() {
            caption(
                "Three posts from jsonplaceholder.typicode.com",
                None::<&str>,
            )
            .into_element()
        } else {
            view()
                .children(self.posts.iter().enumerate().map(|(i, (id, title))| {
                    appear(i as u32).child(
                        Row::new(i == 0)
                            .child(
                                text(format!("#{id}"))
                                    .w(px(28.0))
                                    .text_size(px(13.0))
                                    .text_color(colors::ACCENT)
                                    .tabular_nums(),
                            )
                            .child(
                                text(title.clone())
                                    .number_of_lines(1)
                                    .flex_1()
                                    .text_size(px(15.0))
                                    .text_color(colors::TEXT),
                            ),
                    )
                }))
                .into_element()
        };
        let status = self.status.as_ref().map(|status| {
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
                    text(status.text.clone())
                        .number_of_lines(2)
                        .flex_1()
                        .text_size(px(13.0))
                        .text_color(status.tone.color())
                        .tabular_nums(),
                )
        });
        Card::new()
            .icon(IconName::Search)
            .title("Network and logs")
            .subtitle("ureq · log::info!")
            .child(posts)
            .children(status)
            .child(
                view()
                    .flex_row()
                    .gap_2()
                    .child(
                        Button::new(
                            if loading { "Loading…" } else { "ureq::get()" },
                            cx.listener(|this: &mut Self, _, cx| this.load(cx)),
                        )
                        .icon(IconName::ArrowDown)
                        .disabled(loading)
                        .flex_1(),
                    )
                    .child(
                        Button::new(
                            "log::info!",
                            cx.listener(|_: &mut Self, _, _| {
                                log::info!(target: "gallery", "hello from the gallery");
                            }),
                        )
                        .variant(ButtonVariant::Secondary)
                        .flex_1(),
                    ),
            )
            .child(
                text("log::info! shows in the terminal of nsh run.")
                    .text_size(px(12.0))
                    .text_color(colors::FAINT),
            )
            .into_element()
    }
}

impl Component for System {
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), cx: &mut Context<'_, Self>) {
        self.pop = Some(cx.native_value(1.0));
        self.beat = Some(cx.native_value(0.0));
        self.timer = Some(cx.set_interval(Duration::from_secs(1), |this, cx| {
            this.ticks += 1;
            let beat = this.beat.as_ref().expect("set up");
            beat.set(1.0);
            beat.timing(0.0, 700.0, Easing::Curve(Curve::Quad, EaseMode::Out));
            cx.notify();
        }));
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let sections = [
            Self::environment(cx),
            self.status_bar(cx),
            self.dialogs(cx),
            self.frames_and_timers(cx),
            self.network(cx),
        ];
        // A component's root is a view: the page scrolls inside it.
        view().flex_1().child(
            scroll().flex_1().bg(colors::BACKGROUND).child(
                view().p_4().pb(px(40.0)).gap_4().children(
                    sections
                        .into_iter()
                        .enumerate()
                        .map(|(i, section)| appear(i as u32).child(section)),
                ),
            ),
        )
    }
}
