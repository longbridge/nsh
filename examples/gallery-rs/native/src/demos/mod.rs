//! The demos and the gallery's index.

mod animations;
mod canvas;
mod chat;
mod components;
mod form;
mod gestures;
mod images;
mod lists;
mod media;
mod mobile_ui;
mod navigation;
mod photos;
mod physics;
mod scrolling;
mod stocks;
mod styles;
mod system;
mod text_demo;
mod typed_styles;

use nsh_core::ui::{AnyElement, Screen, prelude::*};

use crate::{
    nav,
    ui::{IconName, IconTile, MONO, PressProps, appear, colors, header, icon, press_with, radius},
};

/// A demo of the index: its name, what it shows, its icon and its
/// screen's content.
struct Demo {
    name: &'static str,
    detail: &'static str,
    icon: IconName,
    content: fn() -> AnyElement,
}

/// A group of the index.
struct Group {
    title: &'static str,
    demos: &'static [Demo],
}

/// The demos, by what they show.
const GROUPS: &[Group] = &[
    Group {
        title: "Apps",
        demos: &[
            Demo {
                name: "Stocks",
                detail: "A live watchlist and a stock: candles, depth, the tape, a tick every 50 ms",
                icon: IconName::Stocks,
                content: || stocks::Stocks::element(()).into_element(),
            },
            Demo {
                name: "AI Chat",
                detail: "Streaming replies, tools, reasoning, attachments, errors",
                icon: IconName::Chat,
                content: || chat::Chat::element(()).into_element(),
            },
            Demo {
                name: "Photos",
                detail: "A long recycled list: images, pull to refresh, paging",
                icon: IconName::Photos,
                content: || photos::Photos::element(()).into_element(),
            },
        ],
    },
    Group {
        title: "Layout and lists",
        demos: &[
            Demo {
                name: "Lists",
                detail: "Columns, sticky sections, a chat, horizontal lists",
                icon: IconName::Lists,
                content: || lists::Lists::element(()).into_element(),
            },
            Demo {
                name: "Scrolling",
                detail: "Paging, snapping, zoom, scroll events, prepending, keyboard taps",
                icon: IconName::Scroll,
                content: || scrolling::Scrolling::element(()).into_element(),
            },
            Demo {
                name: "Navigation",
                detail: "Header options, modals, the back button, safe areas",
                icon: IconName::Nav,
                content: || navigation::Navigation::element(()).into_element(),
            },
            Demo {
                name: "Form",
                detail: "Inputs, keyboard types, the keyboard over the content",
                icon: IconName::Form,
                content: || form::Form::element(()).into_element(),
            },
        ],
    },
    Group {
        title: "Touch and motion",
        demos: &[
            Demo {
                name: "Gestures",
                detail: "Pinch, rotate, press states, hit slop, pointer events",
                icon: IconName::Gestures,
                content: || gestures::Gestures::element(()).into_element(),
            },
            Demo {
                name: "Physics",
                detail: "Drags and springs on the UI thread",
                icon: IconName::Physics,
                content: || physics::Physics::element(()).into_element(),
            },
            Demo {
                name: "Animations",
                detail: "Easing, sequence / parallel / stagger / loop, springs, interpolation",
                icon: IconName::Animation,
                content: || animations::Animations::element(()).into_element(),
            },
        ],
    },
    Group {
        title: "Drawing and type",
        demos: &[
            Demo {
                name: "Canvas",
                detail: "2D context: paths, gradients, text, images, native animation",
                icon: IconName::Canvas,
                content: || canvas::CanvasDemo::element(()).into_element(),
            },
            Demo {
                name: "Styles",
                detail: "Corners, borders, shadows, clipping, 3D transforms",
                icon: IconName::Styles,
                content: || styles::Styles::element(()).into_element(),
            },
            Demo {
                name: "Typed styles",
                detail: "Tailwind's utilities as typed methods: variants, grid, shadows, motion",
                icon: IconName::Tailwind,
                content: || typed_styles::TypedStyles::element(()).into_element(),
            },
            Demo {
                name: "Text",
                detail: "Fonts, spans, inline views and images, decorations, truncation",
                icon: IconName::Text,
                content: || text_demo::TextDemo::element(()).into_element(),
            },
            Demo {
                name: "Images",
                detail: "Resize modes, tint, GIF / WebP, placeholders, blur, fade",
                icon: IconName::Image,
                content: || images::Images::element(()).into_element(),
            },
        ],
    },
    Group {
        title: "Platform",
        demos: &[
            Demo {
                name: "Mobile UI",
                detail: "nsh/ui/mobile in Rust: a ListView and a Group of the app's rows",
                icon: IconName::Mobile,
                content: || mobile_ui::MobileUi::element(()).into_element(),
            },
            Demo {
                name: "Components",
                detail: "Press feedback, sheet, layout animation, input accessory, modal options",
                icon: IconName::Components,
                content: || components::Components::element(()).into_element(),
            },
            Demo {
                name: "Web and video",
                detail: "WKWebView / WebView, AVPlayer / MediaPlayer",
                icon: IconName::Media,
                content: || media::Media::element(()).into_element(),
            },
            Demo {
                name: "System",
                detail: "Status bar, alerts, action sheets, environment, timers, HTTP",
                icon: IconName::System,
                content: || system::System::element(()).into_element(),
            },
        ],
    },
];

/// A demo's row of the index: a tap pushes its screen.
fn demo_row(demo: &'static Demo, first: bool) -> AnyElement {
    let open = move |_: &()| {
        let (name, content) = (demo.name, demo.content);
        nav::push(name, move || {
            header(screen().title(name))
                .child(view().flex_1().bg(colors::BACKGROUND).child(content()))
        });
    };
    press_with(
        PressProps {
            scale: 0.98.into(),
            label: Some(demo.name.into()).into(),
            ..PressProps::default()
        },
        open,
    )
    .child(
        view()
            .flex_row()
            .items_center()
            .gap(px(14.0))
            .p_3()
            .when(!first, |v| v.border_t_1().border_t_color(colors::LINE))
            .child(IconTile::new(demo.icon).size(38.0))
            .child(
                view()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        text(demo.name)
                            .text_size(px(16.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors::TEXT),
                    )
                    .child(
                        text(demo.detail)
                            .number_of_lines(2)
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(colors::MUTED),
                    ),
            )
            .child(icon(IconName::Chevron, 16.0, colors::FAINT)),
    )
    .into_element()
}

/// The index: the demos by group; a tap pushes one.
pub(crate) fn index() -> Screen {
    let groups: Vec<AnyElement> = GROUPS
        .iter()
        .enumerate()
        .map(|(g, group)| {
            let rows: Vec<AnyElement> = group
                .demos
                .iter()
                .enumerate()
                .map(|(i, demo)| demo_row(demo, i == 0))
                .collect();
            appear(g as u32 + 1)
                .child(
                    view()
                        .gap_2()
                        .child(
                            view()
                                .flex_row()
                                .items_baseline()
                                .px_1()
                                .child(
                                    text(group.title.to_uppercase())
                                        .flex_1()
                                        .text_size(px(13.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(colors::MUTED)
                                        .letter_spacing(0.4),
                                )
                                .child(
                                    text(group.demos.len().to_string())
                                        .text_size(px(12.0))
                                        .text_color(colors::FAINT)
                                        .font_family(MONO),
                                ),
                        )
                        .child(
                            view()
                                .bg(colors::SURFACE)
                                .rounded(px(radius::CARD))
                                .border_curve(BorderCurve::Continuous)
                                .border_1()
                                .border_color(colors::BORDER)
                                .overflow_hidden()
                                .children(rows),
                        ),
                )
                .into_element()
        })
        .collect();
    header(screen().key("index").title("Rust Gallery")).child(
        scroll()
            .flex_1()
            .bg(colors::BACKGROUND)
            .child(view().p_4().pb(px(40.0)).gap_5().children(groups)),
    )
}
