//! Web and video: WKWebView / WebView, AVPlayer / MediaPlayer (as the TSX
//! gallery's `demos/Media.tsx`): an inline page in the gallery's colors
//! with messages both ways, a web site with its navigation state, back and
//! forward, and a video with controls of the demo's own or the system's.

use std::time::{SystemTime, UNIX_EPOCH};

use nsh_core::ui::prelude::*;

use crate::ui::{
    Badge, Button, ButtonVariant, Card, Finish, Finished, IconName, Meter, PressProps, RoundButton,
    Segment, SegmentedEvent, Tone, appear, caption, colors, icon, press_with, radius, segmented,
};

/// The web site of the second web view.
const SITE: &str = "https://nsh.dev";

/// The video.
const FLOWER: &str = "https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4";

/// `color` as CSS's `#rrggbb`.
fn css(color: Color) -> String {
    format!("#{:06x}", color.0 >> 8)
}

/// The inline page in the gallery's colors for the appearance shown (the
/// page reloads when it changes).
fn page(dark: bool) -> String {
    let pick = |c: AdaptiveColor| css(if dark { c.dark } else { c.light });
    let (inset, text, muted, border, on_accent) = (
        pick(colors::INSET),
        pick(colors::TEXT),
        pick(colors::MUTED),
        pick(colors::BORDER),
        pick(colors::ON_ACCENT),
    );
    // The accent finish's gradient and highlight.
    let (gradient, highlight) = if dark {
        ("linear-gradient(180deg, #dbf893 0%, #b3dd4e 100%)", 0.45)
    } else {
        ("linear-gradient(180deg, #d8f88f 0%, #bde65a 100%)", 0.6)
    };
    format!(
        r#"<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1">
<style>
  html {{ -webkit-text-size-adjust: 100%; }}
  body {{ margin: 0; padding: 16px; background: {inset}; color: {text}; font-family: -apple-system, system-ui, sans-serif; font-size: 15px; line-height: 1.4; }}
  h3 {{ margin: 0 0 4px; font-size: 17px; font-weight: 600; }}
  p {{ margin: 0 0 12px; color: {muted}; font-size: 15px; }}
  button {{ height: 44px; padding: 0 18px; border: 0; border-radius: 10px; background: {gradient}; color: {on_accent}; font-family: inherit; font-size: 15px; font-weight: 600; box-shadow: inset 0 1px 0 rgba(255,255,255,{highlight}); }}
  #got {{ margin-top: 12px; padding: 10px 12px; border-radius: 10px; border: 1px solid {border}; color: {muted}; font-size: 15px; }}
</style></head>
<body>
<h3>An inline page</h3>
<p>HTML from a string. It sends messages to the app and receives them.</p>
<button onclick="window.ReactNativeWebView.postMessage('hello from the page')">Post to the app</button>
<div id="got">No message from the app yet.</div>
<script>window.addEventListener('message', (e) => {{ document.getElementById('got').textContent = 'From the app: ' + e.data; }});</script>
</body></html>"#
    )
}

/// `75.4` s → `"1:15"`.
fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A web view's or the video's frame.
fn frame<S: Styled>(element: S) -> S {
    element
        .rounded(px(radius::CONTROL))
        .overflow_hidden()
        .border_1()
        .border_color(colors::BORDER)
}

/// The demo's page: an inline page, a web site and a video, with what they
/// tell the app.
#[derive(Debug, Default)]
pub(crate) struct Media {
    /// The inline page.
    web: Option<ElementRef>,
    /// The web site.
    site: Option<ElementRef>,
    /// The video.
    video: Option<ElementRef>,
    /// What the inline page posted last.
    message: String,
    /// What the app posted last (`hi #N`), 0 before.
    sent: u32,
    /// The web site's last navigation state.
    state: Option<WebNavigation>,
    /// The video shows the platform's controls.
    system_controls: bool,
    /// The video plays (it starts paused).
    playing: bool,
    /// The video's position and length, in seconds.
    time: f32,
    duration: f32,
    buffering: bool,
}

impl Media {
    /// The web site's host, from its last URL.
    fn host(&self) -> String {
        let url = self.state.as_ref().map_or("", |s| s.url.as_str());
        let rest = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        let host = rest.split('/').next().unwrap_or("");
        if host.is_empty() {
            "…".to_owned()
        } else {
            host.to_owned()
        }
    }

    fn messaging(&self, cx: &mut RenderCx<'_, Self>) -> impl IntoElement + use<> {
        let web = self.web.clone().expect("set up");
        let post = web.clone();
        Card::new()
            .icon(IconName::Media)
            .title("Web view")
            .subtitle("Inline HTML, with messages both ways")
            .child(frame(
                web_view_html(&page(cx.dark()), None)
                    .element_ref(&web)
                    .h(px(210.0))
                    .bg(colors::INSET)
                    .on_message(cx.listener(|this: &mut Self, data: &str, cx| {
                        this.message = data.to_owned();
                        cx.notify();
                    })),
            ))
            .child(caption(
                "From the page",
                Some(if self.message.is_empty() {
                    "—".to_owned()
                } else {
                    self.message.clone()
                }),
            ))
            .child(caption(
                "To the page",
                Some(if self.sent > 0 {
                    format!("hi #{}", self.sent)
                } else {
                    "—".to_owned()
                }),
            ))
            .child(Button::new(
                "Post to the page",
                cx.listener(move |this: &mut Self, _, cx| {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default();
                    this.sent = (now.as_millis() % 1000) as u32;
                    post.post_message(format!("hi #{}", this.sent));
                    cx.notify();
                }),
            ))
    }

    fn site(&self, cx: &mut RenderCx<'_, Self>) -> impl IntoElement + use<> {
        let site = self.site.clone().expect("set up");
        let state = self.state.as_ref();
        let loading = state.is_some_and(|s| s.loading);
        let can_go_back = state.is_some_and(|s| s.can_go_back);
        let can_go_forward = state.is_some_and(|s| s.can_go_forward);
        let subtitle = state
            .map(|s| s.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Navigation state, back and forward".to_owned());
        let (back, forward, reload) = (site.clone(), site.clone(), site.clone());
        Card::new()
            .icon(IconName::Search)
            .title("A web site")
            .subtitle(subtitle)
            .right(
                Badge::new(if loading { "Loading" } else { "Loaded" }).tone(if loading {
                    Tone::Warn
                } else {
                    Tone::Up
                }),
            )
            .child(
                view()
                    .h(px(40.0))
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px(px(14.0))
                    .rounded(px(20.0))
                    .bg(colors::INSET)
                    .finish(Finish::Track)
                    .border_1()
                    .border_color(colors::BORDER)
                    .child(view().size(px(7.0)).rounded(px(3.5)).bg(if loading {
                        colors::WARN
                    } else {
                        colors::UP
                    }))
                    .child(
                        text(self.host())
                            .number_of_lines(1)
                            .ellipsize_mode(nsh_core::EllipsizeMode::Middle)
                            .flex_1()
                            .text_size(px(15.0))
                            .text_color(colors::MUTED),
                    ),
            )
            .child(frame(
                web_view(SITE)
                    .element_ref(&site)
                    .h(px(340.0))
                    .bg(colors::INSET)
                    .on_navigation_state_change(cx.listener(
                        |this: &mut Self, state: &WebNavigation, cx| {
                            this.state = Some(state.clone());
                            cx.notify();
                        },
                    )),
            ))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(nav_button(
                        true,
                        "Back",
                        can_go_back,
                        cx.listener(move |_: &mut Self, _, _| back.go_back()),
                    ))
                    .child(nav_button(
                        false,
                        "Forward",
                        can_go_forward,
                        cx.listener(move |_: &mut Self, _, _| forward.go_forward()),
                    ))
                    .child(view().flex_1())
                    .child(
                        Button::new(
                            "Reload",
                            cx.listener(move |_: &mut Self, _, _| reload.reload()),
                        )
                        .variant(ButtonVariant::Secondary),
                    ),
            )
    }

    fn player(&self, cx: &mut RenderCx<'_, Self>) -> impl IntoElement + use<> {
        let player = self.video.clone().expect("set up");
        let seek = player.clone();
        let paused = !self.playing;
        let (status, tone) = if self.buffering {
            ("Buffering", Tone::Warn)
        } else if paused {
            ("Paused", Tone::Neutral)
        } else {
            ("Playing", Tone::Accent)
        };
        Card::new()
            .icon(IconName::Play)
            .title("Video")
            .subtitle("AVPlayer / MediaPlayer")
            .right(Badge::new(status).tone(tone))
            .child(frame(
                video(FLOWER)
                    .element_ref(&player)
                    .paused(paused)
                    .muted(true)
                    .repeat(true)
                    .controls(self.system_controls)
                    .h(px(200.0))
                    .bg(rgb(0x000000))
                    .on_load(cx.listener(|this: &mut Self, e: &VideoInfo, cx| {
                        this.duration = e.duration;
                        cx.notify();
                    }))
                    .on_buffer(cx.listener(|this: &mut Self, buffering: &bool, cx| {
                        this.buffering = *buffering;
                        cx.notify();
                    }))
                    .on_progress(cx.listener(|this: &mut Self, e: &VideoProgress, cx| {
                        this.time = e.current_time;
                        this.duration = e.duration;
                        cx.notify();
                    })),
            ))
            .children((!self.system_controls).then(|| {
                view()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(RoundButton::new(
                        if paused {
                            IconName::Play
                        } else {
                            IconName::Pause
                        },
                        if paused { "Play" } else { "Pause" },
                        cx.listener(|this: &mut Self, _, cx| {
                            this.playing = !this.playing;
                            cx.notify();
                        }),
                    ))
                    .child(
                        view()
                            .flex_1()
                            .gap_2()
                            .child(Meter::new(if self.duration > 0.0 {
                                self.time / self.duration
                            } else {
                                0.0
                            }))
                            .child(
                                view()
                                    .flex_row()
                                    .child(
                                        text(clock(self.time))
                                            .flex_1()
                                            .text_size(px(13.0))
                                            .text_color(colors::MUTED)
                                            .tabular_nums(),
                                    )
                                    .child(
                                        text(clock(self.duration))
                                            .text_size(px(13.0))
                                            .text_color(colors::FAINT)
                                            .tabular_nums(),
                                    ),
                            ),
                    )
                    .child(
                        Button::new(
                            "Restart",
                            cx.listener(move |_: &mut Self, _, _| seek.seek(0.0)),
                        )
                        .variant(ButtonVariant::Secondary),
                    )
            }))
            .child(
                segmented(
                    [
                        Segment::new("Own controls"),
                        Segment::new("System controls"),
                    ],
                    u32::from(self.system_controls),
                )
                .on_event(cx.listener(
                    |this: &mut Self, e: &SegmentedEvent, cx| {
                        let SegmentedEvent::Change(i) = *e;
                        this.system_controls = i == 1;
                        cx.notify();
                    },
                )),
            )
    }
}

/// A round chevron button, 44 pt (`flip`: pointing back).
fn nav_button(
    flip: bool,
    label: &'static str,
    enabled: bool,
    on_press: Listener<()>,
) -> impl IntoElement {
    press_with(
        PressProps {
            disabled: !enabled,
            label: Some(label.into()),
            hit_slop: Some(4.0),
            ..PressProps::default()
        },
        on_press,
    )
    .child(
        view()
            .size(px(44.0))
            .rounded(px(22.0))
            .bg(colors::RAISED)
            .finish(Finish::Raised)
            .border_1()
            .border_color(colors::BORDER)
            .items_center()
            .justify_center()
            .when(flip, |v| v.rotate(180.0))
            .child(icon(IconName::Chevron, 18.0, colors::TEXT)),
    )
}

impl Component for Media {
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), cx: &mut Context<'_, Self>) {
        self.web = Some(cx.element_ref());
        self.site = Some(cx.element_ref());
        self.video = Some(cx.element_ref());
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let cards = [
            self.messaging(cx).into_element(),
            self.site(cx).into_element(),
            self.player(cx).into_element(),
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
}
