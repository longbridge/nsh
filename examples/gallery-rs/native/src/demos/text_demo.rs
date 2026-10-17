//! Text: spans and links, inline views and images, type (custom fonts,
//! weights, decorations, transforms, shadows), the system text size and
//! truncation, each on a card (as the TSX gallery's `demos/TextDemo.tsx`).

use nsh_core::ui::{EllipsizeMode, prelude::*};

use crate::{
    assets::{LOGO, PACIFICO},
    ui::{
        Card, Finish, Finished, IconName, MONO, PressProps, appear, caption, colors, press_with,
        radius, rgb_a, tints,
    },
};

const LOREM: &str = "Native text, typeset once on the engine thread with CoreText and drawn on the main thread without any layout work.";

/// Body text.
fn body(content: impl Into<SharedString>) -> Text {
    text(content.into())
        .text_size(px(15.0))
        .line_height(px(22.0))
        .text_color(colors::TEXT)
}

/// A link span: the accent, underlined at half strength.
fn link(content: &'static str) -> Text {
    text(content)
        .text_color(colors::ACCENT)
        .underline()
        .text_decoration_color(colors::ACCENT.opacity(0.5))
}

/// A labeled specimen: a quiet label (and the prop it shows, in code)
/// above the text.
fn specimen(
    label: &'static str,
    code: Option<&'static str>,
    first: bool,
    content: impl IntoElement,
) -> View {
    view()
        .gap_2()
        .when(!first, |v| {
            v.pt_3().border_t_1().border_t_color(colors::LINE)
        })
        .child(
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
                .children(code.map(|code| {
                    text(code)
                        .number_of_lines(1)
                        .text_size(px(12.0))
                        .text_color(colors::FAINT)
                        .font_family(MONO)
                })),
        )
        .child(content)
}

/// A sentence with links in it, one inside a bold span; `tapped` is what
/// was pressed last.
fn links(tapped: Signal<&'static str>) -> Card {
    let tap = move |what: &'static str| move |_: &()| tapped.set(what);
    Card::new()
        .icon(IconName::Text)
        .title("Spans and links")
        .subtitle("Nested spans with presses of their own")
        .child(
            body("By continuing you agree to the ")
                .text_size(px(16.0))
                .line_height(px(25.0))
                .on_press(tap("the sentence (not a link)"))
                .child(
                    link("Terms of service")
                        .on_press(tap("Terms of service"))
                        .on_long_press(tap("Terms (long press)")),
                )
                .child(" and the ")
                .child(link("Privacy policy").on_press(tap("Privacy policy")))
                .child(", which mention ")
                .child(
                    text("nested ")
                        .font_weight(FontWeight::BOLD)
                        .child(link("links").on_press(tap("a link inside bold text")))
                        .child(" too"),
                )
                .child("."),
        )
        .child(caption("Pressed", Some(move || tapped.get())))
}

/// A sentence with an image, a badge and a button in it counting
/// `likes`.
fn inline(likes: Signal<u32>) -> Card {
    let like = move |_: &()| likes.update(|n| *n += 1);
    Card::new()
        .icon(IconName::Image)
        .title("Inline views and images")
        .subtitle("They wrap with the line")
        .child(
            body("Made with ")
                .text_size(px(16.0))
                .line_height(px(28.0))
                .child(image(LOGO).size(px(18.0)).rounded(px(5.0)))
                .child(" nsh. Status ")
                .child(
                    view()
                        .bg(tints::UP)
                        .rounded(px(9.0))
                        .px_2()
                        .h(px(18.0))
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(view().size(px(6.0)).rounded(px(3.0)).bg(colors::UP))
                        .child(
                            text("LIVE")
                                .text_color(colors::UP)
                                .text_size(px(11.0))
                                .font_weight(FontWeight::BOLD)
                                .letter_spacing(0.5),
                        ),
                )
                .child(" and a button inside the sentence ")
                .child(
                    press_with(
                        PressProps {
                            scale: 0.9.into(),
                            label: Some("Like".into()).into(),
                            hit_slop: Some(10.0),
                            ..PressProps::default()
                        },
                        like,
                    )
                    .child(
                        view()
                            .bg(colors::RAISED)
                            .finish(Finish::Raised)
                            .rounded(px(11.0))
                            .px(px(9.0))
                            .h(px(22.0))
                            .justify_center()
                            .border_1()
                            .border_color(colors::BORDER)
                            .child(
                                text(move || format!("♥ {}", likes.get()))
                                    .text_color(colors::ACCENT)
                                    .text_size(px(13.0))
                                    .tabular_nums(),
                            ),
                    ),
                )
                .child(" that takes presses of its own."),
        )
}

/// Custom fonts, weights, decorations, transforms and shadows.
fn fonts() -> Card {
    Card::new()
        .icon(IconName::Styles)
        .title("Type")
        .subtitle("Custom fonts, weights, styles, spacing")
        .child(specimen(
            "Custom font",
            Some("Pacifico-Regular.ttf"),
            true,
            text("Custom fonts")
                .font_family(PACIFICO.family)
                .text_size(px(34.0))
                .text_color(colors::ACCENT)
                .line_height(px(52.0)),
        ))
        .child(specimen(
            "Weight, style, spacing",
            Some("fontWeight, fontStyle"),
            false,
            text("")
                .text_size(px(17.0))
                .text_color(colors::TEXT)
                .child(text("Light ").font_weight(FontWeight::LIGHT))
                .child(text("Bold ").font_weight(FontWeight::BOLD))
                .child(text("Italic ").italic())
                .child(text("Both ").font_weight(FontWeight::BOLD).italic())
                .child(text("Spaced").letter_spacing(4.0).text_color(colors::MUTED)),
        ))
        .child(specimen(
            "Decorations",
            Some("textDecorationLine"),
            false,
            text("")
                .text_size(px(17.0))
                .text_color(colors::TEXT)
                .child(text("Underline").underline())
                .child(" ")
                .child(
                    text("Strikethrough")
                        .line_through()
                        .text_decoration_color(colors::DANGER)
                        .text_color(colors::MUTED),
                )
                .child(" ")
                .child(
                    text("Both")
                        .underline()
                        .line_through()
                        .text_color(colors::INFO),
                ),
        ))
        .child(specimen(
            "Transforms",
            Some("textTransform"),
            false,
            text("uppercase ")
                .text_size(px(17.0))
                .text_color(colors::TEXT)
                .uppercase()
                .child(
                    text("capitalized words here")
                        .capitalize()
                        .text_color(colors::MUTED),
                ),
        ))
        .child(specimen(
            "Shadows",
            Some("textShadow*"),
            false,
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    text("Glow")
                        .text_size(px(26.0))
                        .font_weight(FontWeight::EXTRA_BOLD)
                        .text_color(colors::ACCENT)
                        .text_shadow_color(AdaptiveColor::new(
                            colors::ACCENT.light.with_alpha(0.3),
                            colors::ACCENT.dark.with_alpha(0.6),
                        ))
                        .text_shadow_offset(0.0, 0.0)
                        .text_shadow_radius(10.0),
                )
                .child(
                    view()
                        .h(px(48.0))
                        .px(px(14.0))
                        .justify_center()
                        .rounded(px(radius::CONTROL))
                        .border_curve(BorderCurve::Continuous)
                        .bg(colors::RAISED)
                        .finish(Finish::Raised)
                        .border_1()
                        .border_color(colors::BORDER)
                        .child(
                            text("Drop shadow")
                                .text_size(px(26.0))
                                .font_weight(FontWeight::EXTRA_BOLD)
                                .text_color(colors::INFO)
                                .text_shadow_color(AdaptiveColor::new(
                                    rgb_a(0x14140a, 0.22),
                                    rgb_a(0x000000, 0.9),
                                ))
                                .text_shadow_offset(2.0, 3.0)
                                .text_shadow_radius(3.0),
                        ),
                ),
        ))
}

/// The system text size and its limits.
fn scaling() -> Card {
    let sized = |content: Text| content.text_size(px(17.0));
    Card::new()
        .icon(IconName::Search)
        .title("System text size")
        .subtitle("fontScale and its limits")
        .right(
            text(move || format!("{:.2}×", font_scale()))
                .text_size(px(15.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors::ACCENT)
                .tabular_nums(),
        )
        .child(specimen(
            "Default",
            None,
            true,
            sized(body("Scales with the system setting")),
        ))
        .child(specimen(
            "Fixed",
            Some("allowFontScaling={false}"),
            false,
            sized(body("Fixed size")).allow_font_scaling(false),
        ))
        .child(specimen(
            "Capped",
            Some("maxFontSizeMultiplier={1.2}"),
            false,
            sized(body("At most 1.2×")).max_font_size_multiplier(1.2),
        ))
}

/// Lines and ellipsis modes.
fn truncation() -> Card {
    let lorem = |mode: EllipsizeMode| body(LOREM).number_of_lines(1).ellipsize_mode(mode);
    Card::new()
        .icon(IconName::Lists)
        .title("Truncation")
        .subtitle("numberOfLines and ellipsizeMode")
        .child(specimen(
            "Two lines, tail",
            Some("numberOfLines={2}"),
            true,
            body(LOREM).number_of_lines(2),
        ))
        .child(specimen(
            "One line, head",
            Some("ellipsizeMode=\"head\""),
            false,
            lorem(EllipsizeMode::Head),
        ))
        .child(specimen(
            "One line, middle",
            Some("ellipsizeMode=\"middle\""),
            false,
            lorem(EllipsizeMode::Middle),
        ))
        .child(specimen(
            "One line, clip",
            Some("ellipsizeMode=\"clip\""),
            false,
            lorem(EllipsizeMode::Clip),
        ))
        .child(specimen(
            "Centered, three lines",
            Some("textAlign=\"center\""),
            false,
            body(format!("{LOREM} {LOREM}"))
                .number_of_lines(3)
                .text_center()
                .text_color(colors::MUTED),
        ))
}

/// The demo's page.
#[component]
pub(crate) fn TextDemo() -> impl IntoElement {
    // What was pressed in the sentence, and the count of the button inside
    // a sentence.
    let tapped = signal("nothing yet");
    let likes = signal(3_u32);
    let cards = [
        links(tapped),
        inline(likes),
        fonts(),
        scaling(),
        truncation(),
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
