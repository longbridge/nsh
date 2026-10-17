//! Navigation: screens with header options, a deep stack, a screen that
//! asks before it is left, modals of every presentation and the Android
//! back button (as the TSX gallery's `demos/Navigation.tsx`). Each pushed
//! screen shows where its safe area is.

use nsh_core::{
    Dialog, DialogButton, DialogButtonStyle, ModalPresentation,
    ui::{AnyElement, Screen, prelude::*},
};
use nsh_ui_mobile::{SafeAreaView, SafeAreaViewProps};

use crate::{
    nav,
    ui::{
        Badge, Button, ButtonVariant, Card, Figure, Finish, Finished, IconName, IconTile, Inset,
        MONO, PressProps, Section, Stat, ToggleEvent, Tone, appear, colors, header, icon, motion,
        press_with, radius, tints, toggle,
    },
};

/// An edge's inset.
type Edge = fn(EdgeInsets) -> f32;

/// The safe-area insets where it renders, one figure per edge.
#[component]
fn Insets(cx: Scope<NoEvent>) -> impl IntoElement {
    let edges: [(&str, Edge); 4] = [
        ("Top", |i| i.top),
        ("Bottom", |i| i.bottom),
        ("Left", |i| i.left),
        ("Right", |i| i.right),
    ];
    view().flex_row().children(edges.map(|(edge, inset)| {
        Stat::new(edge, move || format!("{} pt", inset(cx.insets()).round())).width(0.25)
    }))
}

fn insets() -> AnyElement {
    Insets::element(()).into_element()
}

/// Muted body text.
fn body(content: impl Into<SharedString>) -> Text {
    text(content.into())
        .text_size(px(15.0))
        .line_height(px(21.0))
        .text_color(colors::MUTED)
}

/// A screen's big title over a badge naming its option.
fn title_block(badge: impl Into<SharedString>, title: impl Into<SharedString>, size: f32) -> View {
    view()
        .gap_2()
        .child(
            view()
                .flex_row()
                .child(Badge::new(badge.into()).tone(Tone::Accent)),
        )
        .child(
            text(title.into())
                .text_size(px(size))
                .font_weight(FontWeight::BOLD)
                .letter_spacing(-0.5)
                .text_color(colors::TEXT),
        )
}

/// The body of a pushed screen: what it shows, then where its safe area
/// is, then `content`.
fn page(intro: &str, content: impl IntoIterator<Item = AnyElement>) -> AnyElement {
    scroll()
        .flex_1()
        .bg(colors::BACKGROUND)
        .child(
            view()
                .p_4()
                .pb(px(40.0))
                .gap_4()
                .child(appear(0).child(body(intro.to_owned()).px_1()))
                .child(
                    appear(1).child(
                        Section::new("Safe area")
                            .note("useSafeAreaInsets()")
                            .child(insets()),
                    ),
                )
                .child(appear(2).child(view().gap_4().children(content))),
        )
        .into_element()
}

/// The stars of the custom header (its header item and content both show
/// them).
#[derive(Default, Clone)]
struct Likes(u32);

/// The draft of the unsaved-changes screen (its `prevent_back` depends on
/// it).
#[derive(Default, Clone)]
struct Draft(String);

fn large_title() -> Screen {
    let rows = (1..=20).map(|i| {
        view()
            .h(px(48.0))
            .flex_row()
            .items_center()
            .px(px(10.0))
            .when(i > 1, |v| v.border_t_1().border_t_color(colors::LINE))
            .child(
                text(format!("Row {i}"))
                    .flex_1()
                    .text_size(px(15.0))
                    .text_color(colors::TEXT),
            )
            .child(
                text(format!("{} pt", i * 48))
                    .text_size(px(13.0))
                    .text_color(colors::FAINT)
                    .tabular_nums(),
            )
    });
    header(screen().title("Large title"))
        .header_large_title(true)
        .child(page(
            "iOS: the title shrinks into the bar as the content scrolls.",
            [Card::new()
                .flush()
                .child(Inset::new().padding(0.0).gap(0.0).children(rows))
                .into_element()],
        ))
}

fn transparent() -> Screen {
    screen()
        .title("Transparent")
        .header_transparent(true)
        .header_tint_color(|| color(colors::TEXT))
        .header_title_color(|| color(colors::TEXT))
        .child(
            view()
                .flex_1()
                .bg(AdaptiveColor::new(rgb(0xeef3e2), rgb(0x11170c)))
                .overflow_hidden()
                .p_4()
                .pb(px(56.0))
                .justify_end()
                .gap_4()
                .child(
                    view()
                        .absolute()
                        .top(px(-90.0))
                        .right(px(-70.0))
                        .size(px(320.0))
                        .rounded(px(160.0))
                        .bg(tints::ACCENT),
                )
                .child(
                    view()
                        .absolute()
                        .top(px(150.0))
                        .left(px(-110.0))
                        .size(px(260.0))
                        .rounded(px(130.0))
                        .bg(tints::INFO),
                )
                .child(
                    appear(0).child(
                        title_block("headerTransparent", "Content under the header", 30.0)
                            .gap_3()
                            .child(body(
                                "The circles start at the top of the screen; the bar and its back \
                                 button float over them.",
                            )),
                    ),
                )
                .child(appear(1).child(Inset::new().child(insets()))),
        )
}

fn custom_header() -> Screen {
    let likes = global::<Likes>();
    let count = move || likes.with(|l| l.0);
    let on_accent = || color(colors::ON_ACCENT);
    screen()
        .title("Custom")
        .header_tint_color(on_accent)
        .header_background_color(|| color(colors::ACCENT_FILL))
        .header_title_color(on_accent)
        .header_back_visible(false)
        .header_left(
            text("Close")
                .text_color(colors::ON_ACCENT)
                .text_size(px(17.0))
                .font_weight(FontWeight::MEDIUM)
                .hit_slop(EdgeInsets::all(10.0))
                .on_press(|_: &()| nav::pop()),
        )
        .header_title(
            view()
                .flex_row()
                .items_center()
                .gap(px(7.0))
                .child(view().size(px(10.0)).rounded(px(5.0)).bg(colors::ON_ACCENT))
                .child(
                    text("Custom title")
                        .text_color(colors::ON_ACCENT)
                        .text_size(px(17.0))
                        .font_weight(FontWeight::BOLD),
                ),
        )
        .header_right(
            view()
                .flex_row()
                .items_center()
                .gap(px(5.0))
                .hit_slop(EdgeInsets::all(10.0))
                .on_press(move |_: &()| likes.update(|likes| likes.0 += 1))
                .child(icon(IconName::Star, 18.0, colors::ON_ACCENT))
                .child(
                    text(move || count().to_string())
                        .text_color(colors::ON_ACCENT)
                        .text_size(px(17.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .tabular_nums(),
                ),
        )
        .child(page(
            "The header's items are views: the star on the right counts presses and re-renders \
             the header.",
            [Section::new("Header")
                .note("headerRight")
                .child(
                    Figure::new(move || count().to_string())
                        .unit(move || if count() == 1 { "star" } else { "stars" }),
                )
                .into_element()],
        ))
}

fn locked() -> Screen {
    header(screen().title("No swipe back"))
        .gesture_enabled(false)
        .child(page(
            "The back gesture is off on this screen: no swipe on iOS, no predictive animation on \
             Android. The back buttons still work.",
            [Section::new("Leaving")
                .note("gestureEnabled={false}")
                .child(
                    view()
                        .flex_row()
                        .child(
                            Stat::new("Swipe / predictive", "Off")
                                .tone(Tone::Danger)
                                .width(0.5),
                        )
                        .child(Stat::new("Back buttons", "On").tone(Tone::Up).width(0.5)),
                )
                .into_element()],
        ))
}

/// The draft's card (the screen's `prevent_back` follows the draft).
#[component]
fn DraftCard() -> impl IntoElement {
    let draft = global::<Draft>();
    let focused = signal(false);
    let length = computed(move || draft.with(|d| d.0.chars().count()));
    let guarded = computed(move || length.get() > 0);
    Card::new()
        .icon(IconName::Form)
        .title("Draft")
        .subtitle("preventBack + onBackRequest")
        .right(
            Badge::new(move || if guarded.get() { "Guarded" } else { "Empty" }).tone(move || {
                if guarded.get() {
                    Tone::Warn
                } else {
                    Tone::Neutral
                }
            }),
        )
        .child(
            input()
                .value(move || draft.with(|d| d.0.clone()))
                .placeholder("A draft")
                .placeholder_color(|| color(colors::FAINT))
                .selection_color(|| color(colors::ACCENT))
                .h(px(48.0))
                .px(px(14.0))
                .text_size(px(15.0))
                .text_color(colors::TEXT)
                .bg(colors::INSET)
                .border_1()
                .border_color(move || {
                    if focused.get() {
                        colors::ACCENT
                    } else {
                        colors::BORDER
                    }
                })
                .rounded(px(radius::CONTROL))
                .finish(Finish::Inset)
                .on_focus(move |_: &()| focused.set(true))
                .on_blur(move |_: &()| focused.set(false))
                .on_change_text(move |text: &str| {
                    let text = text.to_owned();
                    draft.update(|draft| draft.0 = text);
                }),
        )
        .child(
            text(move || {
                if guarded.get() {
                    format!("{} characters would be lost: back asks first", length.get())
                } else {
                    "Nothing to lose: back pops".to_owned()
                }
            })
            .text_size(px(13.0))
            .text_color(colors::MUTED),
        )
}

fn unsaved() -> Screen {
    let draft = global::<Draft>();
    header(screen().title("Unsaved changes"))
        .prevent_back(move || draft.with(|d| !d.0.is_empty()))
        .on_back_request(move |_: &()| {
            let answer = dialog(Dialog {
                id: 0,
                title: Some("Discard your changes?".into()),
                message: None,
                buttons: vec![
                    DialogButton {
                        text: "Keep editing".into(),
                        style: DialogButtonStyle::Cancel,
                    },
                    DialogButton {
                        text: "Discard".into(),
                        style: DialogButtonStyle::Destructive,
                    },
                ],
                cancelable: true,
                sheet: false,
            });
            spawn(async move {
                if answer.await == 1 {
                    // TS's draft is the screen's own and goes with it.
                    draft.update(|draft| draft.0.clear());
                    nav::pop();
                }
            });
        })
        .child(page(
            "Type something, then go back (header button, swipe, Android back): the screen asks \
             first.",
            [DraftCard::element(()).into_element()],
        ))
}

fn faded() -> Screen {
    header(screen().title("Fade"))
        .animation(ScreenAnimation::Fade)
        .child(page("This screen faded in, and fades out.", []))
}

fn instant() -> Screen {
    header(screen().title("No animation"))
        .animation(ScreenAnimation::None)
        .child(page(
            "Shown without a transition, and removed without one.",
            [],
        ))
}

fn headerless() -> Screen {
    screen().header_shown(false).child(
        SafeAreaView::element(SafeAreaViewProps::default())
            .flex_1()
            .bg(colors::BACKGROUND)
            .child(
                view()
                    .flex_1()
                    .p_4()
                    .gap_4()
                    .child(
                        appear(0).child(
                            title_block("headerShown={false}", "No header", 30.0)
                                .px_1()
                                .pt_3()
                                .child(body(
                                    "The screen owns the whole surface; a SafeAreaView keeps it \
                                     clear of the status bar and the home indicator.",
                                )),
                        ),
                    )
                    .child(
                        appear(1).child(
                            Section::new("Safe area")
                                .note("SafeAreaView")
                                .child(insets()),
                        ),
                    )
                    .child(view().flex_1())
                    .child(Button::new("Back", |_: &()| nav::pop())),
            ),
    )
}

/// Level `depth` of a deep stack.
fn deep(depth: usize) -> impl Fn() -> Screen {
    move || {
        let deeper = move |_: &()| nav::push(format!("deep-{}", depth + 1), deep(depth + 1));
        let levels = (1..=depth).map(|level| {
            let current = level == depth;
            view()
                .size(px(32.0))
                .rounded(px(8.0))
                .items_center()
                .justify_center()
                .map(|v| {
                    if current {
                        v.bg(colors::ACCENT_FILL).finish(Finish::Accent)
                    } else {
                        v.border_1()
                            .border_color(colors::BORDER)
                            .bg(colors::RAISED)
                            .finish(Finish::Raised)
                    }
                })
                .child(
                    text(level.to_string())
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .tabular_nums()
                        .text_color(if current {
                            colors::ON_ACCENT
                        } else {
                            colors::MUTED
                        }),
                )
        });
        header(screen().title(format!("Level {depth}"))).child(page(
            "Every push is a native screen; popToTop() unwinds them all at once.",
            [Card::new()
                .icon(IconName::Nav)
                .title(format!("Level {depth}"))
                .subtitle(format!("{depth} deep in the stack"))
                .child(view().flex_row().flex_wrap().gap(px(6.0)).children(levels))
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .child(Button::new("Deeper", deeper).icon(IconName::Plus).flex_1())
                        .child(
                            Button::new("To the index", |_: &()| nav::pop_to_top())
                                .variant(ButtonVariant::Secondary)
                                .flex_1(),
                        ),
                )
                .into_element()],
        ))
    }
}

/// A row that pushes a screen: an icon, a title, the API it shows.
fn link_row(
    icon_name: IconName,
    title: &'static str,
    note: &'static str,
    first: bool,
    on_press: impl Into<Listener<()>>,
) -> impl IntoElement {
    press_with(
        PressProps {
            scale: 0.98.into(),
            label: Some(title.into()).into(),
            ..PressProps::default()
        },
        on_press,
    )
    .child(
        view()
            .min_h(px(60.0))
            .flex_row()
            .items_center()
            .gap_3()
            .px(px(10.0))
            .py_2()
            .when(!first, |v| v.border_t_1().border_t_color(colors::LINE))
            .child(IconTile::new(icon_name).size(34.0))
            .child(
                view()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        text(title)
                            .text_size(px(15.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors::TEXT),
                    )
                    .child(
                        text(note)
                            .number_of_lines(1)
                            .text_size(px(12.0))
                            .text_color(colors::FAINT)
                            .font_family(MONO),
                    ),
            )
            .child(icon(IconName::Chevron, 14.0, colors::FAINT)),
    )
}

/// Which modal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presented {
    FullScreen,
    PageSheet,
    FormSheet,
    Fade,
    Dialog,
    Dismissible,
}

/// The modals: what they are, their note, their icon.
const MODALS: [(Presented, &str, &str, &str, IconName); 6] = [
    (
        Presented::FullScreen,
        "fullScreen",
        "Full screen",
        "Slides up",
        IconName::Mobile,
    ),
    (
        Presented::PageSheet,
        "pageSheet",
        "Page sheet",
        "iOS sheet",
        IconName::Components,
    ),
    (
        Presented::FormSheet,
        "formSheet",
        "Form sheet",
        "iOS, centered",
        IconName::Form,
    ),
    (Presented::Fade, "fade", "Fade", "Fades in", IconName::Spark),
    (
        Presented::Dialog,
        "dialog",
        "Dialog",
        "See-through",
        IconName::Bell,
    ),
    (
        Presented::Dismissible,
        "dismissible",
        "Dismissible",
        "Swipe to close",
        IconName::ArrowDown,
    ),
];

/// A modal's content: what it is, its safe area, a close button.
fn modal_body(
    title: &str,
    note: &str,
    text_body: &str,
    close: impl Into<Listener<()>>,
) -> impl IntoElement {
    SafeAreaView::element(SafeAreaViewProps::default())
        .flex_1()
        .bg(colors::BACKGROUND)
        .child(
            view()
                .flex_1()
                .p_4()
                .gap_4()
                .child(
                    title_block(note.to_owned(), title.to_owned(), 28.0)
                        .px_1()
                        .pt_2()
                        .child(body(text_body.to_owned())),
                )
                .child(
                    Section::new("Safe area")
                        .note("the modal's")
                        .child(insets()),
                )
                .child(view().flex_1())
                .child(Button::new("Close", close)),
        )
}

/// The demo's page: rows pushing the screens, its modals, and the Android
/// back button.
#[component]
pub(crate) fn Navigation() -> impl IntoElement {
    let sheet = signal(None::<Presented>);
    // The dialog's spring in, 0 to 1.
    let pop = native_value(0.0);
    // The back button is intercepted, and the presses it caught.
    let intercepting = signal(false);
    let presses = signal(0_u32);
    // A listener hears the button before the stack pops it; listening only
    // while intercepting keeps Android's back gesture previewing the pop.
    effect(move || {
        if intercepting.get() {
            on_back_press(move || {
                presses.update(|p| *p += 1);
                true
            });
        }
    });
    type Make = fn() -> Screen;
    let screens: [(&str, IconName, &str, &str, Make); 8] = [
        (
            "large-title",
            IconName::Text,
            "Large title",
            "ios.headerLargeTitle",
            large_title,
        ),
        (
            "transparent",
            IconName::Styles,
            "Transparent header",
            "headerTransparent",
            transparent,
        ),
        (
            "custom",
            IconName::Star,
            "Custom header items",
            "headerLeft / Title / Right",
            custom_header,
        ),
        (
            "headerless",
            IconName::Mobile,
            "No header",
            "headerShown={false}",
            headerless,
        ),
        (
            "faded",
            IconName::Spark,
            "Fade in",
            "animation=\"fade\"",
            faded,
        ),
        (
            "instant",
            IconName::Play,
            "No animation",
            "animation=\"none\"",
            instant,
        ),
        (
            "locked",
            IconName::Gestures,
            "No swipe back",
            "gestureEnabled={false}",
            locked,
        ),
        (
            "unsaved",
            IconName::Form,
            "Unsaved changes",
            "preventBack + onBackRequest",
            unsaved,
        ),
    ];
    let mut links: Vec<AnyElement> = screens
        .into_iter()
        .enumerate()
        .map(|(i, (key, icon_name, title, note, make))| {
            link_row(icon_name, title, note, i == 0, move |_: &()| {
                nav::push(key, make)
            })
            .into_element()
        })
        .collect();
    links.push(
        link_row(
            IconName::Lists,
            "A deep stack",
            "push / popToTop",
            false,
            |_: &()| nav::push(format!("deep-{}", nav::depth()), deep(1)),
        )
        .into_element(),
    );
    let tiles = MODALS.map(|(presented, _, title, note, icon_name)| {
        let pop = pop.clone();
        press_with(
            PressProps {
                label: Some(title.into()).into(),
                ..PressProps::default()
            },
            move |_: &()| {
                if presented == Presented::Dialog {
                    // The dialog springs up from a little smaller each
                    // time it opens.
                    pop.set(0.0);
                    pop.spring(1.0, motion::SETTLE);
                }
                sheet.set(Some(presented));
            },
        )
        .w(relative(0.485))
        .child(
            view()
                .min_h(px(100.0))
                .p_3()
                .gap_3()
                .rounded(px(radius::INSET))
                .border_curve(BorderCurve::Continuous)
                .bg(colors::INSET)
                .border_1()
                .border_color(colors::LINE)
                .finish(Finish::Inset)
                .child(IconTile::new(icon_name).size(32.0))
                .child(
                    view()
                        .gap(px(2.0))
                        .child(
                            text(title)
                                .text_size(px(15.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors::TEXT),
                        )
                        .child(text(note).text_size(px(13.0)).text_color(colors::FAINT)),
                ),
        )
    });
    let close = move |_: &()| sheet.set(None);
    let modal_view = dynamic(move || {
        sheet.get().map(|presented| match presented {
            Presented::FullScreen
            | Presented::PageSheet
            | Presented::FormSheet
            | Presented::Fade => {
                let presentation = match presented {
                    Presented::PageSheet => ModalPresentation::PageSheet,
                    Presented::FormSheet => ModalPresentation::FormSheet,
                    _ => ModalPresentation::FullScreen,
                };
                let (_, key, title, ..) = MODALS
                    .into_iter()
                    .find(|m| m.0 == presented)
                    .expect("a modal");
                modal()
                    .presentation(presentation)
                    .animation(if presented == Presented::Fade {
                        ModalAnimation::Fade
                    } else {
                        ModalAnimation::Slide
                    })
                    .on_request_close(close)
                    .child(modal_body(
                        title,
                        key,
                        "Close with the button, the back button (Android) or a swipe down \
                         (iOS sheets).",
                        close,
                    ))
                    .into_element()
            }
            Presented::Dismissible => modal()
                .dismissible(true)
                .presentation(ModalPresentation::PageSheet)
                .on_request_close(close)
                .on_dismiss(close)
                .child(modal_body(
                    "Dismissible",
                    "dismissible",
                    "Swipe it down (iOS) or press back (Android): it closes natively, then \
                     tells the app.",
                    close,
                ))
                .into_element(),
            Presented::Dialog => modal()
                .transparent(true)
                .animation(ModalAnimation::Fade)
                .on_request_close(close)
                .child(
                    view()
                        .flex_1()
                        .bg(rgba(0x0000_009e))
                        .items_center()
                        .justify_center()
                        .p_4()
                        .child(
                            view()
                                .self_stretch()
                                .bind_opacity(&pop)
                                .bind_scale(&pop.interpolate(&[0.0, 1.0], &[0.9, 1.0]))
                                .child(
                                    Card::new()
                                        .icon(IconName::Bell)
                                        .title("A dialog")
                                        .subtitle("transparent + fade")
                                        .child(body(
                                            "A transparent modal shows what is behind it; the \
                                             dialog springs in natively.",
                                        ))
                                        .child(
                                            view()
                                                .flex_row()
                                                .gap_2()
                                                .child(
                                                    Button::new("Cancel", close)
                                                        .variant(ButtonVariant::Secondary)
                                                        .flex_1(),
                                                )
                                                .child(Button::new("OK", close).flex_1()),
                                        ),
                                ),
                        ),
                )
                .into_element(),
        })
    });
    view().flex_1().child(
        scroll()
            .flex_1()
            .bg(colors::BACKGROUND)
            .child(
                view()
                    .p_4()
                    .pb(px(40.0))
                    .gap_4()
                    .child(
                        appear(0).child(
                            Card::new()
                                .icon(IconName::Nav)
                                .title("Native stack")
                                .subtitle("Headers, transitions, the safe area")
                                .right(Badge::new("UIKit / View"))
                                .child(insets()),
                        ),
                    )
                    .child(
                        appear(1).child(
                            Card::new()
                                .title("Screens")
                                .subtitle("Screen options, pushed with nav.push")
                                .flush()
                                .child(Inset::new().padding(0.0).gap(0.0).children(links)),
                        ),
                    )
                    .child(
                        appear(2).child(
                            Card::new()
                                .title("Modals")
                                .subtitle("Presentation styles and transitions")
                                .flush()
                                .child(
                                    view()
                                        .flex_row()
                                        .flex_wrap()
                                        .justify_between()
                                        .gap_y_2()
                                        .children(tiles),
                                ),
                        ),
                    )
                    .child(
                        appear(3).child(
                            Card::new()
                                .icon(IconName::System)
                                .title("Back button")
                                .subtitle("Android: BackHandler")
                                .right(toggle(intercepting, "Intercept the back button").on_event(
                                    move |e: &ToggleEvent| {
                                        let ToggleEvent::Change(on) = *e;
                                        intercepting.set(on);
                                    },
                                ))
                                .child(
                                    Figure::new(move || presses.get().to_string())
                                        .unit(move || {
                                            if presses.get() == 1 {
                                                "press"
                                            } else {
                                                "presses"
                                            }
                                        })
                                        .note(move || {
                                            if intercepting.get() {
                                                "caught"
                                            } else {
                                                "popped"
                                            }
                                        })
                                        .tone(move || {
                                            if intercepting.get() {
                                                Tone::Warn
                                            } else {
                                                Tone::Accent
                                            }
                                        }),
                                )
                                .child(
                                    text(move || {
                                        if intercepting.get() {
                                            "The handler returns true: back presses stay on \
                                             this screen."
                                        } else {
                                            "Off: the back button pops this screen."
                                        }
                                    })
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .text_color(colors::MUTED),
                                ),
                        ),
                    ),
            )
            .child(modal_view),
    )
}
