//! The terminal's look: app colors drawn in the terminal's own theme.
//!
//! An app's RGBA color becomes one of the terminal's 16-color meanings, so
//! the user's terminal theme decides the actual colors ([`ink`]). A color is
//! read against its [`Ground`]: the opaque fill the app drew it on (what the
//! app designed it against), else the page of the app's appearance (black
//! when dark, white when light). Its own alpha is first blended over that
//! ground; view opacity never changes a color.
//!
//! - **Saturated** colors (chroma at least [`SATURATED_CHROMA`]) take the
//!   accent of their hue band, at normal intensity: red, yellow (orange and
//!   amber), green (lime and emerald), cyan (teal), blue (violet) or magenta
//!   (purple and pink).
//! - **Near-neutral** colors are the default foreground, or dim when their
//!   lightness is closer to the ground's than [`DEFAULT_CONTRAST`]
//!   (secondary text, placeholders, separators); never a gray of the
//!   palette, which themes set without regard to contrast. Black (the
//!   core's default text color) and colors within [`NEAR_DEFAULT`] of the
//!   page's foreground are always the default foreground, so unstyled text
//!   reads on dark and light terminals alike.
//! - A fully transparent color hides what it draws.
//!
//! Hovered and pressed controls show a band ([`Bands`]): the terminal's
//! background mixed a little toward its foreground, at the terminal's
//! depth; with only the 16 standard colors there is none.
//!
//! Pictures (canvas drawings) keep their colors ([`paint`]): converted at
//! the terminal's [`ColorDepth`], as they are in true color, to the nearest
//! entry of the xterm 256-color cube and gray ramp, or to the nearest of the
//! 16 standard colors.

use nsh_core::platform::ColorScheme;
use ratatui_core::style::{Color, Modifier, Style};

use crate::ColorDepth;

/// The chroma (largest minus smallest channel, of 255) from which a color
/// reads as an accent rather than a shade of gray.
const SATURATED_CHROMA: f32 = 0.2 * 255.0;
/// The CIE L* distance from the ground from which a neutral color is the
/// default foreground rather than dim.
const DEFAULT_CONTRAST: f32 = 75.0;
/// The CIE L* distance from black, or from the page's foreground, within
/// which a neutral color is the default foreground whatever its ground.
const NEAR_DEFAULT: f32 = 4.0;
/// How far a hovered control's band mixes the terminal's background toward
/// its foreground.
const HOVER_SHARE: f32 = 0.10;
/// How far a pressed control's band mixes it.
const PRESS_SHARE: f32 = 0.18;
/// Hue bands (degrees, upper bounds) and the accent each one reads as.
const HUE_BANDS: [(f32, Color); 6] = [
    (15.0, Color::Red),
    (75.0, Color::Yellow),
    (165.0, Color::Green),
    (200.0, Color::Cyan),
    (265.0, Color::Blue),
    (335.0, Color::Magenta),
];

/// An RGB triple.
pub(crate) type Rgb = [u8; 3];

/// What a color is read against: the opaque fill the app drew it on, else
/// the page of the app's appearance; and the depth pictures are drawn at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Ground {
    pub(crate) rgb: Rgb,
    /// The page's foreground: white when dark, black when light.
    pub(crate) foreground: Rgb,
    pub(crate) depth: ColorDepth,
    /// The terminal's surfaces, when it has them ([`Ramp`]); `None` is the
    /// bandless look.
    pub(crate) ramp: Option<Ramp>,
    /// The step of the ramp under this point ([`LADDER`]'s index).
    pub(crate) level: usize,
    /// The terminal background drawn under this point: `Reset` on the
    /// page, a step or an accent tint on a filled surface, a swatch's own
    /// color.
    pub(crate) surface: Color,
    /// Inside a swatch ([`swatch`]): text is black or white, whichever
    /// reads on its color.
    pub(crate) swatch: bool,
}

impl Ground {
    /// The page of `scheme`.
    pub(crate) fn page(scheme: ColorScheme, depth: ColorDepth) -> Self {
        let (rgb, foreground) = match scheme {
            ColorScheme::Dark => ([0; 3], [255; 3]),
            ColorScheme::Light => ([255; 3], [0; 3]),
        };
        Self {
            rgb,
            foreground,
            depth,
            ramp: None,
            level: 0,
            surface: Color::Reset,
            swatch: false,
        }
    }

    /// This ground with the terminal's surfaces `ramp`.
    pub(crate) fn with_ramp(self, ramp: Option<Ramp>) -> Self {
        Self { ramp, ..self }
    }

    /// The ground inside a view filled with `fill`: the fill when it is
    /// opaque, else this one.
    pub(crate) fn under(self, fill: nsh_core::Color) -> Self {
        let [red, green, blue, alpha] = fill.0.to_be_bytes();
        if alpha == 255 {
            Self {
                rgb: [red, green, blue],
                ..self
            }
        } else {
            self
        }
    }
}

/// How a color reads on its ground, before the terminal's look decides
/// how that is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tone {
    /// Fully transparent: what it draws is hidden.
    Hidden,
    /// A saturated color: the accent of its hue.
    Accent(Color),
    /// The default foreground.
    Default,
    /// Secondary: closer to its ground than [`DEFAULT_CONTRAST`].
    Muted,
    /// Barely there: closer to its ground than [`FAINT_CONTRAST`].
    Faint,
}

/// The CIE L* distance from the ground below which a neutral color is
/// faint rather than muted (with surfaces; both are dim without).
const FAINT_CONTRAST: f32 = 35.0;

/// How `color` reads on `ground`.
pub(crate) fn tone(color: nsh_core::Color, ground: Ground) -> Tone {
    let [red, green, blue, alpha] = color.0.to_be_bytes();
    if alpha == 0 {
        return Tone::Hidden;
    }
    // Whether it is an accent is the color's own: a translucent black or
    // white over a hued fill (a caption on a lime header) takes the fill's
    // hue but is ink, not an accent of that hue on its own color.
    let own_chroma = f32::from(red.max(green).max(blue)) - f32::from(red.min(green).min(blue));
    let [red, green, blue] = over_f32([red, green, blue], alpha, ground.rgb);
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    if max - min >= SATURATED_CHROMA && own_chroma >= SATURATED_CHROMA {
        let accent = HUE_BANDS
            .iter()
            .find(|(bound, _)| hue(red, green, blue, max, min) < *bound)
            .map_or(Color::Red, |(_, accent)| *accent);
        return Tone::Accent(accent);
    }
    let own = lightness(red, green, blue);
    let [fg_red, fg_green, fg_blue] = ground.foreground.map(f32::from);
    // The terminal's own foreground, where its surfaces say it.
    let [term_red, term_green, term_blue] = ground
        .ramp
        .map_or(ground.foreground, |ramp| ramp.foreground)
        .map(f32::from);
    let terminal = lightness(term_red, term_green, term_blue);
    if own <= NEAR_DEFAULT
        || (own - lightness(fg_red, fg_green, fg_blue)).abs() <= NEAR_DEFAULT
        || (own - terminal).abs() <= NEAR_DEFAULT
    {
        return Tone::Default;
    }
    let [under_red, under_green, under_blue] = ground.rgb.map(f32::from);
    let under = lightness(under_red, under_green, under_blue);
    let contrast = (own - under).abs();
    // Text that stands off its ground as far as the terminal's foreground
    // does is the foreground: the app's body text on a raised button, not
    // a secondary caption.
    let foreground = (terminal - under).abs();
    if contrast < FAINT_CONTRAST {
        Tone::Faint
    } else if contrast < DEFAULT_CONTRAST.min(foreground).max(FAINT_CONTRAST) {
        Tone::Muted
    } else {
        Tone::Default
    }
}

/// The terminal style `color` draws text or lines with on `ground`: a
/// foreground meaning and the dim (or hidden) attribute, which replace the
/// Cell's. With surfaces, secondary colors are fades of the terminal's
/// foreground ([`Ramp::muted`], [`Ramp::faint`]) instead of dim.
pub(crate) fn ink(color: nsh_core::Color, ground: Ground) -> Style {
    let plain = Style::new().remove_modifier(Modifier::HIDDEN);
    if ground.swatch && !color.is_transparent() {
        return plain.fg(swatch_text(ground)).remove_modifier(Modifier::DIM);
    }
    let tone = match tone(color, ground) {
        // The terminal's fades of its foreground are made for its page: on
        // a hued band (a lime header) a secondary caption reads only at
        // full strength.
        Tone::Muted
            if hued(nsh_core::Color(u32::from_be_bytes([
                ground.rgb[0],
                ground.rgb[1],
                ground.rgb[2],
                255,
            ]))) =>
        {
            Tone::Default
        }
        tone => tone,
    };
    match (tone, ground.ramp) {
        (Tone::Hidden, _) => Style::new()
            .fg(Color::Reset)
            .add_modifier(Modifier::HIDDEN)
            .remove_modifier(Modifier::DIM),
        (Tone::Accent(accent), _) => plain.fg(accent).remove_modifier(Modifier::DIM),
        (Tone::Default, _) => plain.fg(Color::Reset).remove_modifier(Modifier::DIM),
        // A fade as light as the band under it (a muted size on a
        // selected row's fill) is read at the next strength up.
        (Tone::Muted | Tone::Faint, Some(ramp)) => {
            let reads = |share: f32| {
                let [r, g, b] = ramp.mix(share).map(f32::from);
                let [ur, ug, ub] = ground.rgb.map(f32::from);
                (lightness(r, g, b) - lightness(ur, ug, ub)).abs() >= FADE_READS
            };
            let fg = if tone == Tone::Faint && reads(FAINT_SHARE) {
                ramp.faint()
            } else if reads(MUTED_SHARE) {
                ramp.muted()
            } else {
                Color::Reset
            };
            plain.fg(fg).remove_modifier(Modifier::DIM)
        }
        (Tone::Muted | Tone::Faint, None) => plain.fg(Color::Reset).add_modifier(Modifier::DIM),
    }
}

/// How far (in L*) a fade of the foreground must be from its ground's
/// lightness to read.
const FADE_READS: f32 = 15.0;

/// How far a fill's lightness is from its ground's for the fill to stand
/// out (a primary button on its page).
const STANDS_OUT: f32 = 40.0;

/// The color a control whose opaque `fill` stands out from `ground` (a
/// primary button, its fill not drawn) shows its text in: the fill's
/// accent, else the default foreground; `None` when the fill does not
/// stand out.
pub(crate) fn stands_out(fill: nsh_core::Color, ground: Ground) -> Option<Color> {
    let [red, green, blue, alpha] = fill.0.to_be_bytes();
    if alpha != 255 {
        return None;
    }
    let [red, green, blue] = [red, green, blue].map(f32::from);
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    if max - min >= SATURATED_CHROMA {
        return ink(fill, ground).fg;
    }
    let [under_red, under_green, under_blue] = ground.rgb.map(f32::from);
    let contrast =
        (lightness(red, green, blue) - lightness(under_red, under_green, under_blue)).abs();
    (contrast >= STANDS_OUT).then_some(Color::Reset)
}

/// The backgrounds of hovered and pressed controls; none at 16 colors.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Bands {
    pub(crate) hover: Option<Color>,
    pub(crate) press: Option<Color>,
}

impl Bands {
    /// The bands on a terminal whose own colors are `background` and
    /// `foreground` (from OSC 11 and 10), else black and white (white and
    /// black in the light scheme).
    pub(crate) fn new(
        scheme: ColorScheme,
        depth: ColorDepth,
        background: Option<Rgb>,
        foreground: Option<Rgb>,
    ) -> Self {
        if !matches!(depth, ColorDepth::TrueColor | ColorDepth::Ansi256) {
            return Self::default();
        }
        let page = Ground::page(scheme, depth);
        let background = background.unwrap_or(page.rgb);
        let foreground = foreground.unwrap_or(page.foreground);
        let band = |share: f32| {
            let rgb = std::array::from_fn(|i| {
                let (b, f) = (f32::from(background[i]), f32::from(foreground[i]));
                (b + (f - b) * share).round() as u8
            });
            Some(convert(rgb, depth))
        };
        Self {
            hover: band(HOVER_SHARE),
            press: band(PRESS_SHARE),
        }
    }
}

/// The chroma (largest minus smallest channel) from which a view's own
/// fill is a tint whose state bands keep its hue ([`Ramp::band`]).
const TINTED: u8 = 12;

/// The steps of a terminal's surfaces: how far each one mixes the
/// terminal's background toward its foreground. 0 is the terminal's own
/// background (never painted), 1 sunken, 2 raised, 3 hover, 4 select;
/// those above take fills nested deeper.
pub(crate) const LADDER: [f32; 7] = [0.0, 0.03, 0.07, 0.11, 0.16, 0.21, 0.26];
/// A sunken surface's step (code blocks, wells).
pub(crate) const SUNKEN: usize = 1;
/// A raised surface's step (panels, cards, popovers).
pub(crate) const RAISED: usize = 2;
/// A hovered control's step.
pub(crate) const HOVER: usize = 3;
/// A selected or focused item's step.
pub(crate) const SELECT: usize = 4;
/// How far box lines mix the background toward the foreground.
const BORDER_SHARE: f32 = 0.22;
/// A focused field's or container's line.
const FOCUS_LINE_SHARE: f32 = 0.40;
/// A popover's or menu's line: clearly brighter than a card's.
const POPOVER_LINE_SHARE: f32 = 0.45;
/// Barely there text (disabled, placeholders).
const FAINT_SHARE: f32 = 0.35;
/// Secondary text.
const MUTED_SHARE: f32 = 0.50;
/// How much of an accent a saturated fill's band takes.
const TINT_SHARE: f32 = 0.20;
/// How much of an accent an opaque fill's band takes, when it is no control's.
const SOLID_TINT_SHARE: f32 = 0.50;
/// How far a filled view's lightness must be from its ground's to count as
/// a surface of its own.
const FILLED_CONTRAST: f32 = 1.5;
/// Below this alpha a fill is no fill.
const FILLED_ALPHA: u8 = 8;

/// A terminal's surfaces, after grok-build's look: depth from background
/// steps derived from the terminal's own background and foreground (OSC 11
/// and 10), quantized at its depth. Only on terminals with 256 colors or
/// more whose background is known; elsewhere the look is bandless.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Ramp {
    background: Rgb,
    foreground: Rgb,
    depth: ColorDepth,
}

impl Ramp {
    /// The surfaces of a terminal at `depth` whose colors are `background`
    /// and `foreground` (white or black, against the background, when it
    /// did not say).
    pub(crate) fn new(
        depth: ColorDepth,
        background: Option<Rgb>,
        foreground: Option<Rgb>,
    ) -> Option<Self> {
        if !matches!(depth, ColorDepth::TrueColor | ColorDepth::Ansi256) {
            return None;
        }
        let background = background?;
        let foreground = foreground.unwrap_or_else(|| {
            let [r, g, b] = background.map(f32::from);
            if lightness(r, g, b) > 50.0 {
                [0; 3]
            } else {
                [255; 3]
            }
        });
        Some(Self {
            background,
            foreground,
            depth,
        })
    }

    /// The background mixed `share` of the way toward the foreground.
    pub(crate) fn mix(&self, share: f32) -> Rgb {
        mix(self.background, self.foreground, share)
    }

    fn color(&self, share: f32) -> Color {
        convert(self.mix(share), self.depth)
    }

    /// The color of step `level` ([`LADDER`]); the terminal's own
    /// background (`Reset`) at 0.
    pub(crate) fn level(&self, level: usize) -> Color {
        if level == 0 {
            Color::Reset
        } else {
            self.color(LADDER[level.min(LADDER.len() - 1)])
        }
    }

    /// The band of `target` over a view whose own surface at `level` is
    /// `own`: the ramp's step ([`Ramp::above`]), or, on a tinted fill of
    /// its own (a primary button's lime, a danger button's red), the tint
    /// lightened by as much as that step lightens the ramp, so the
    /// control keeps its color hovered, pressed or focused.
    pub(crate) fn band(&self, own: Color, level: usize, target: usize) -> Color {
        let step = Self::above(level, target);
        let band = self.level(step);
        let Color::Rgb(r, g, b) = own else {
            return band;
        };
        if r.max(g).max(b) - r.min(g).min(b) < TINTED {
            return band;
        }
        let (to, from) = (
            self.mix(LADDER[step]),
            self.mix(LADDER[level.min(LADDER.len() - 1)]),
        );
        let lifted: Rgb = std::array::from_fn(|i| {
            let channel = i16::from([r, g, b][i]) + i16::from(to[i]) - i16::from(from[i]);
            channel.clamp(0, 255) as u8
        });
        convert(lifted, self.depth)
    }

    /// The step a band of `target` takes over a surface at `level`: the
    /// target, or one above the surface where that is not lighter.
    pub(crate) fn above(level: usize, target: usize) -> usize {
        target.max(level + 1).min(LADDER.len() - 1)
    }

    /// Box lines.
    pub(crate) fn border(&self) -> Color {
        self.color(BORDER_SHARE)
    }

    /// A focused field's or container's line.
    pub(crate) fn focus_line(&self) -> Color {
        self.color(FOCUS_LINE_SHARE)
    }

    /// A popover's or menu's line.
    pub(crate) fn popover_line(&self) -> Color {
        self.color(POPOVER_LINE_SHARE)
    }

    /// The terminal's own background as a color: text on an accent band.
    pub(crate) fn base(&self) -> Color {
        convert(self.background, self.depth)
    }

    /// Secondary text.
    pub(crate) fn muted(&self) -> Color {
        self.color(MUTED_SHARE)
    }

    /// Faint text: disabled, placeholders.
    pub(crate) fn faint(&self) -> Color {
        self.color(FAINT_SHARE)
    }

    /// A saturated fill's band over step `level`: the accent `rgb` mixed
    /// [`TINT_SHARE`] into that step.
    pub(crate) fn tint(&self, rgb: Rgb, level: usize) -> Color {
        let under = self.mix(LADDER[level.min(LADDER.len() - 1)]);
        convert(mix(under, rgb, TINT_SHARE), self.depth)
    }

    /// The band of a saturated fill of `alpha` that is no control's (a
    /// swatch, a tinted box): the more opaque the fill, the more of its
    /// accent the band takes, from [`TINT_SHARE`] up to a quarter's alpha
    /// to [`SOLID_TINT_SHARE`] when opaque, so washes of one hue at
    /// several strengths stay apart.
    pub(crate) fn tint_alpha(&self, rgb: Rgb, level: usize, alpha: u8) -> Color {
        let alpha = f32::from(alpha) / 255.0;
        let share =
            TINT_SHARE + (SOLID_TINT_SHARE - TINT_SHARE) * ((alpha - 0.25) / 0.75).clamp(0.0, 1.0);
        let under = self.mix(LADDER[level.min(LADDER.len() - 1)]);
        convert(mix(under, rgb, share), self.depth)
    }

    /// `color` faded half way into the terminal's background: what is
    /// behind a modal. `foreground` says whether it is a text color (the
    /// default one fades to [`Self::muted`]) or a background (the default
    /// one stays).
    pub(crate) fn fade(&self, color: Color, foreground: bool) -> Color {
        let rgb = match color {
            Color::Reset if foreground => return self.muted(),
            Color::Reset => return Color::Reset,
            Color::Rgb(r, g, b) => [r, g, b],
            Color::Indexed(index) if index >= 16 => indexed_rgb(index),
            _ if foreground => return self.muted(),
            _ => return color,
        };
        convert(mix(rgb, self.background, 0.5), self.depth)
    }
}

/// What an app's fill is in a terminal with surfaces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Surface {
    /// No fill of its own: its parent's surface shows.
    None,
    /// Lighter than its ground: one step up.
    Raised,
    /// Darker than its ground: one step down.
    Sunken,
    /// A saturated fill (a primary button, a badge): its color tinted into
    /// the step under it.
    Accent(Rgb),
}

/// What `fill` is over `ground`: a fill of at least [`FILLED_ALPHA`] that
/// is saturated (its own color, however translucent) or whose lightness,
/// blended over the ground, differs from the ground's by
/// [`FILLED_CONTRAST`] (the same rule the terminal layout reserves a
/// surface's room by).
pub(crate) fn surface(fill: nsh_core::Color, ground: Ground) -> Surface {
    let [red, green, blue, alpha] = fill.0.to_be_bytes();
    if alpha < FILLED_ALPHA {
        return Surface::None;
    }
    let [r, g, b] = over_f32([red, green, blue], alpha, ground.rgb);
    // A saturated fill is its hue's accent however translucent: a 12 %
    // accent wash over a selected row reads as the accent, not as gray.
    let own = [red, green, blue].map(f32::from);
    if own[0].max(own[1]).max(own[2]) - own[0].min(own[1]).min(own[2]) >= SATURATED_CHROMA {
        let rgb = [r, g, b].map(|c| c.round() as u8);
        return if rgb == ground.rgb {
            Surface::None
        } else {
            Surface::Accent([red, green, blue])
        };
    }
    let [ur, ug, ub] = ground.rgb.map(f32::from);
    let difference = lightness(r, g, b) - lightness(ur, ug, ub);
    if difference >= FILLED_CONTRAST {
        Surface::Raised
    } else if difference <= -FILLED_CONTRAST {
        Surface::Sunken
    } else {
        Surface::None
    }
}

/// Whether `color` is hued (its channels 51 or more apart), the terminal
/// layout's rule: a hued border keeps its line on a filled view.
pub(crate) fn hued(color: nsh_core::Color) -> bool {
    let [r, g, b, _] = color.0.to_be_bytes();
    r.max(g).max(b) - r.min(g).min(b) >= 51
}

/// The step a surface takes over one at `level`.
pub(crate) fn step(surface: Surface, level: usize) -> usize {
    match surface {
        Surface::None | Surface::Accent(_) => level,
        Surface::Raised => Ramp::above(level, RAISED),
        Surface::Sunken if level >= RAISED => level - 1,
        Surface::Sunken if level == 0 => SUNKEN,
        Surface::Sunken => 0,
    }
}

fn mix(from: Rgb, to: Rgb, share: f32) -> Rgb {
    std::array::from_fn(|i| {
        let (a, b) = (f32::from(from[i]), f32::from(to[i]));
        (a + (b - a) * share).round() as u8
    })
}

/// The RGB of a 256-color palette entry above the 16 themable ones.
fn indexed_rgb(index: u8) -> Rgb {
    if index >= 232 {
        [8 + 10 * (index - 232); 3]
    } else {
        let i = index - 16;
        [
            CUBE[usize::from(i / 36)],
            CUBE[usize::from(i / 6 % 6)],
            CUBE[usize::from(i % 6)],
        ]
    }
}

/// The style a picture's `color` draws with on `ground`: its own color
/// blended over the ground, at the terminal's depth.
pub(crate) fn paint(color: nsh_core::Color, ground: Ground) -> Style {
    if color.is_transparent() {
        return Style::new().add_modifier(Modifier::HIDDEN);
    }
    let [red, green, blue, alpha] = color.0.to_be_bytes();
    let rgb = over_f32([red, green, blue], alpha, ground.rgb).map(|c| c.round() as u8);
    Style::new()
        .fg(convert(rgb, ground.depth))
        .remove_modifier(Modifier::HIDDEN | Modifier::DIM)
}

/// A swatch's fill (a color picker's swatch or track Cell): its own
/// color blended over `ground`, as an RGB triple and as the terminal color
/// closest to it at the ground's depth. `None` when it is transparent or
/// the terminal has no surfaces (the bandless look paints no fills).
pub(crate) fn swatch(color: nsh_core::Color, ground: Ground) -> Option<(Rgb, Color)> {
    if color.is_transparent() || ground.ramp.is_none() {
        return None;
    }
    let [red, green, blue, alpha] = color.0.to_be_bytes();
    let rgb = over_f32([red, green, blue], alpha, ground.rgb).map(|c| c.round() as u8);
    Some((rgb, convert(rgb, ground.depth)))
}

/// The color text takes on a swatch (`ground.swatch`), whatever the app
/// colored it: black on a light swatch, white on a dark one, at the
/// terminal's depth.
pub(crate) fn swatch_text(ground: Ground) -> Color {
    let [red, green, blue] = ground.rgb.map(f32::from);
    let rgb = if lightness(red, green, blue) >= 55.0 {
        [0; 3]
    } else {
        [255; 3]
    };
    convert(rgb, ground.depth)
}

fn over_f32(top: Rgb, alpha: u8, under: Rgb) -> [f32; 3] {
    let alpha = f32::from(alpha) / 255.0;
    std::array::from_fn(|i| f32::from(top[i]) * alpha + f32::from(under[i]) * (1.0 - alpha))
}

fn hue(red: f32, green: f32, blue: f32, max: f32, min: f32) -> f32 {
    let chroma = max - min;
    let sector = if max == red {
        ((green - blue) / chroma).rem_euclid(6.0)
    } else if max == green {
        (blue - red) / chroma + 2.0
    } else {
        (red - green) / chroma + 4.0
    };
    sector * 60.0
}

/// CIE L* (0 to 100) of an sRGB color with channels of 0 to 255.
fn lightness(red: f32, green: f32, blue: f32) -> f32 {
    let linear = |channel: f32| {
        let channel = channel / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue);
    if luminance > 216.0 / 24389.0 {
        116.0 * luminance.cbrt() - 16.0
    } else {
        luminance * 24389.0 / 27.0
    }
}

/// The terminal color closest to `rgb` that `depth` can show.
pub(crate) fn convert(rgb: Rgb, depth: ColorDepth) -> Color {
    match depth {
        ColorDepth::Ansi256 => Color::Indexed(nearest_256(rgb)),
        ColorDepth::Ansi16 => nearest_16(rgb),
        _ => Color::Rgb(rgb[0], rgb[1], rgb[2]),
    }
}

/// The levels of the xterm 6 × 6 × 6 color cube.
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The nearest entry of the 256-color palette above the 16 themable ones:
/// the color cube (16–231) or the gray ramp (232–255).
fn nearest_256(rgb: Rgb) -> u8 {
    let level = |channel: u8| {
        CUBE.iter()
            .enumerate()
            .min_by_key(|(_, level)| (i32::from(**level) - i32::from(channel)).abs())
            .map_or(0, |(index, _)| index)
    };
    let [r, g, b] = rgb.map(level);
    let cube = [CUBE[r], CUBE[g], CUBE[b]];
    let average = (u32::from(rgb[0]) + u32::from(rgb[1]) + u32::from(rgb[2])) / 3;
    let step = ((average.saturating_sub(3)) / 10).min(23) as u8;
    let gray = 8 + 10 * step;
    if distance(rgb, [gray; 3]) < distance(rgb, cube) {
        232 + step
    } else {
        16 + 36 * r as u8 + 6 * g as u8 + b as u8
    }
}

/// xterm's default values of the 16 standard colors.
const STANDARD: [(Rgb, Color); 16] = [
    ([0, 0, 0], Color::Black),
    ([205, 0, 0], Color::Red),
    ([0, 205, 0], Color::Green),
    ([205, 205, 0], Color::Yellow),
    ([0, 0, 238], Color::Blue),
    ([205, 0, 205], Color::Magenta),
    ([0, 205, 205], Color::Cyan),
    ([229, 229, 229], Color::Gray),
    ([127, 127, 127], Color::DarkGray),
    ([255, 0, 0], Color::LightRed),
    ([0, 255, 0], Color::LightGreen),
    ([255, 255, 0], Color::LightYellow),
    ([92, 92, 255], Color::LightBlue),
    ([255, 0, 255], Color::LightMagenta),
    ([0, 255, 255], Color::LightCyan),
    ([255, 255, 255], Color::White),
];

fn nearest_16(rgb: Rgb) -> Color {
    STANDARD
        .iter()
        .min_by_key(|(standard, _)| distance(rgb, *standard))
        .map_or(Color::Reset, |(_, color)| *color)
}

/// The "redmean" weighted distance (squared): close to how far apart two
/// colors look, without leaving sRGB.
fn distance(a: Rgb, b: Rgb) -> u32 {
    let mean = (i32::from(a[0]) + i32::from(b[0])) / 2;
    let [dr, dg, db] = std::array::from_fn(|i| i32::from(a[i]) - i32::from(b[i]));
    (((512 + mean) * dr * dr) >> 8) as u32
        + (4 * dg * dg) as u32
        + (((767 - mean) * db * db) >> 8) as u32
}

#[cfg(test)]
mod tests {
    use nsh_core::{Color as Rgba, platform::ColorScheme};
    use ratatui_core::style::{Color, Modifier};

    use super::{
        Bands, Ground, RAISED, Ramp, SELECT, SUNKEN, Surface, Tone, convert, ink, paint, step,
        surface, tone,
    };
    use crate::ColorDepth;

    fn page(scheme: ColorScheme) -> Ground {
        Ground::page(scheme, ColorDepth::TrueColor)
    }

    fn reads(color: u32, scheme: ColorScheme) -> (Color, bool) {
        let style = ink(Rgba(color), page(scheme));
        (
            style.fg.unwrap(),
            style.add_modifier.contains(Modifier::DIM),
        )
    }

    #[test]
    fn neutral_colors_are_the_default_foreground_or_dim_by_contrast() {
        use ColorScheme::*;
        assert_eq!(reads(0xfafafaff, Dark), (Color::Reset, false));
        assert_eq!(reads(0x8d9198ff, Dark), (Color::Reset, true));
        assert_eq!(reads(0xffffff1a, Dark), (Color::Reset, true));
        assert_eq!(reads(0x0a0a0aff, Light), (Color::Reset, false));
        assert_eq!(reads(0x5f636aff, Light), (Color::Reset, true));
        assert_eq!(reads(0x64748bff, Light), (Color::Reset, true));
    }

    #[test]
    fn saturated_colors_take_the_accent_of_their_hue_band() {
        let accent = |color| reads(color, ColorScheme::Dark).0;
        assert_eq!(accent(0xef4444ff), Color::Red);
        assert_eq!(accent(0xf97316ff), Color::Yellow);
        assert_eq!(accent(0x45d49aff), Color::Green);
        assert_eq!(accent(0x14b8a6ff), Color::Cyan);
        assert_eq!(accent(0x8b5cf6ff), Color::Blue);
        assert_eq!(accent(0xec4899ff), Color::Magenta);
    }

    #[test]
    fn colors_are_read_against_the_fill_they_were_designed_on() {
        let lime = page(ColorScheme::Dark).under(Rgba(0xc8f26dff));
        let label = ink(Rgba(0x13160cff), lime);
        assert_eq!(label.fg, Some(Color::Reset));
        assert!(!label.add_modifier.contains(Modifier::DIM));
        // A translucent black over the lime takes its hue, but is ink, at
        // full strength on the band.
        let caption = ink(Rgba(0x13160c9e), lime);
        assert_eq!(caption.fg, Some(Color::Reset));
        assert!(!caption.add_modifier.contains(Modifier::DIM));
        assert!(
            ink(Rgba(0xa1a1aaff), lime)
                .add_modifier
                .contains(Modifier::DIM)
        );
        // A translucent fill is no ground.
        assert_eq!(
            page(ColorScheme::Dark).under(Rgba(0xc8f26d80)),
            page(ColorScheme::Dark)
        );
    }

    #[test]
    fn black_and_the_pages_foreground_are_always_the_default_foreground() {
        use ColorScheme::*;
        assert_eq!(reads(0x000000ff, Dark), (Color::Reset, false));
        assert_eq!(reads(0x09090bff, Dark), (Color::Reset, false));
        let lime = page(Dark).under(Rgba(0xc8f26dff));
        let white = ink(Rgba(0xfafafaff), lime);
        assert_eq!(white.fg, Some(Color::Reset));
        assert!(!white.add_modifier.contains(Modifier::DIM));
        assert_eq!(reads(0x27272aff, Dark), (Color::Reset, true));
    }

    #[test]
    fn hover_and_press_bands_mix_the_terminals_background_toward_its_foreground() {
        let bands = Bands::new(ColorScheme::Dark, ColorDepth::TrueColor, None, None);
        assert_eq!(bands.hover, Some(Color::Rgb(26, 26, 26)));
        assert_eq!(bands.press, Some(Color::Rgb(46, 46, 46)));
        let known = Bands::new(
            ColorScheme::Dark,
            ColorDepth::TrueColor,
            Some([0x10, 0x20, 0x30]),
            Some([0xf0, 0xf0, 0xf0]),
        );
        assert_eq!(known.hover, Some(Color::Rgb(0x26, 0x35, 0x43)));
        let light = Bands::new(ColorScheme::Light, ColorDepth::Ansi256, None, None);
        assert!(matches!(light.hover, Some(Color::Indexed(_))));
        assert_eq!(
            Bands::new(ColorScheme::Dark, ColorDepth::Ansi16, None, None),
            Bands::default()
        );
    }

    #[test]
    fn transparent_colors_hide() {
        let hidden = ink(Rgba(0xef444400), page(ColorScheme::Dark));
        assert!(hidden.add_modifier.contains(Modifier::HIDDEN));
    }

    #[test]
    fn pictures_keep_their_colors_at_the_terminals_depth() {
        let at = |depth| Ground::page(ColorScheme::Dark, depth);
        assert_eq!(
            paint(Rgba(0x123456ff), at(ColorDepth::TrueColor)).fg,
            Some(Color::Rgb(0x12, 0x34, 0x56))
        );
        assert_eq!(
            paint(Rgba(0xffffff80), at(ColorDepth::TrueColor)).fg,
            Some(Color::Rgb(128, 128, 128))
        );
        assert_eq!(
            convert([255, 0, 0], ColorDepth::Ansi256),
            Color::Indexed(196)
        );
        assert_eq!(
            convert([0x80, 0x80, 0x80], ColorDepth::Ansi256),
            Color::Indexed(244)
        );
        assert_eq!(
            convert([0x3b, 0x82, 0xf6], ColorDepth::Ansi256),
            Color::Indexed(69)
        );
        assert_eq!(
            convert([0xef, 0x44, 0x44], ColorDepth::Ansi16),
            Color::LightRed
        );
        assert_eq!(
            convert([0x22, 0xc5, 0x5e], ColorDepth::Ansi16),
            Color::Green
        );
    }

    fn ramp() -> Ramp {
        Ramp::new(ColorDepth::TrueColor, Some([24; 3]), Some([224; 3])).unwrap()
    }

    #[test]
    fn surfaces_need_a_known_background_and_256_colors() {
        assert_eq!(Ramp::new(ColorDepth::TrueColor, None, Some([224; 3])), None);
        assert_eq!(Ramp::new(ColorDepth::Ansi16, Some([24; 3]), None), None);
        let light = Ramp::new(ColorDepth::TrueColor, Some([250; 3]), None).unwrap();
        assert_eq!(
            light.mix(0.5),
            [125; 3],
            "a light terminal mixes toward black"
        );
        let indexed = Ramp::new(ColorDepth::Ansi256, Some([24; 3]), None).unwrap();
        assert!(matches!(indexed.level(SELECT), Color::Indexed(_)));
    }

    #[test]
    fn the_ramp_steps_from_the_terminals_background_toward_its_foreground() {
        let ramp = ramp();
        assert_eq!(ramp.level(0), Color::Reset, "the terminal's own background");
        assert_eq!(ramp.level(SUNKEN), Color::Rgb(30, 30, 30));
        assert_eq!(ramp.level(RAISED), Color::Rgb(38, 38, 38));
        assert_eq!(ramp.level(SELECT), Color::Rgb(56, 56, 56));
        assert_eq!(ramp.border(), Color::Rgb(68, 68, 68));
        assert_eq!(ramp.muted(), Color::Rgb(124, 124, 124));
        assert_eq!(ramp.faint(), Color::Rgb(94, 94, 94));
        assert_eq!(ramp.tint([59, 130, 246], 0), Color::Rgb(31, 45, 68));
        assert_eq!(
            ramp.tint_alpha([59, 130, 246], 0, 51),
            ramp.tint([59, 130, 246], 0)
        );
        let half = ramp.tint_alpha([59, 130, 246], 0, 128);
        let solid = ramp.tint_alpha([59, 130, 246], 0, 255);
        assert!(
            half != ramp.tint([59, 130, 246], 0) && half != solid,
            "washes stay apart"
        );
        assert_eq!(
            ramp.fade(Color::Rgb(224, 0, 0), true),
            Color::Rgb(124, 12, 12)
        );
        assert_eq!(ramp.fade(Color::Reset, false), Color::Reset);
    }

    #[test]
    fn fills_are_steps_over_their_ground() {
        let ground = page(ColorScheme::Dark).with_ramp(Some(ramp()));
        assert_eq!(surface(Rgba(0x27272aff), ground), Surface::Raised);
        assert_eq!(
            surface(Rgba(0x000000ff), ground),
            Surface::None,
            "the page's own color"
        );
        assert_eq!(
            surface(Rgba(0xffffff04), ground),
            Surface::None,
            "all but transparent"
        );
        assert_eq!(
            surface(Rgba(0x3b82f6ff), ground),
            Surface::Accent([59, 130, 246])
        );
        assert_eq!(
            surface(Rgba(0x3b82f61f), ground),
            Surface::Accent([59, 130, 246]),
            "a translucent wash of the accent"
        );
        assert_eq!(step(Surface::Raised, 0), RAISED);
        assert_eq!(
            step(Surface::Raised, RAISED),
            RAISED + 1,
            "a fill in a card is lighter still"
        );
        assert_eq!(step(Surface::Sunken, 0), SUNKEN);
        assert_eq!(step(Surface::Sunken, RAISED + 1), RAISED);
    }

    #[test]
    fn a_tinted_controls_state_bands_keep_its_hue() {
        let ramp = ramp();
        // A gray surface takes the ramp's step.
        assert_eq!(
            ramp.band(ramp.level(RAISED), RAISED, super::HOVER),
            ramp.level(super::HOVER)
        );
        // A lime one is lightened by the same amount, still lime.
        let Color::Rgb(r, g, b) = ramp.band(Color::Rgb(55, 63, 37), 0, super::HOVER) else {
            panic!("a color of the terminal's depth");
        };
        let lift = ramp.mix(super::LADDER[super::HOVER])[0] - ramp.mix(super::LADDER[0])[0];
        assert_eq!((r, g, b), (55 + lift, 63 + lift, 37 + lift));
    }

    #[test]
    fn body_text_on_a_raised_button_reads_as_the_terminals_foreground() {
        // A terminal whose foreground is a light gray, and a secondary
        // button's raised fill: the app's body text stands off it as far
        // as that foreground does, its muted caption does not.
        let ramp = Ramp::new(ColorDepth::TrueColor, Some([20; 3]), Some([225; 3])).unwrap();
        let mut ground = page(ColorScheme::Dark).with_ramp(Some(ramp));
        ground.rgb = [0x2d, 0x2e, 0x32];
        assert_eq!(tone(Rgba(0xeceef1ff), ground), Tone::Default);
        assert_eq!(tone(Rgba(0xe1e1e1ff), ground), Tone::Default);
        assert_eq!(tone(Rgba(0x9a9ea5ff), ground), Tone::Muted);
        assert_eq!(tone(Rgba(0x6b6f76ff), ground), Tone::Faint);
    }

    #[test]
    fn secondary_text_fades_on_surfaces_and_dims_without_them() {
        let ground = page(ColorScheme::Dark);
        assert_eq!(tone(Rgba(0x71717aff), ground), Tone::Muted);
        let faded = ink(Rgba(0x71717aff), ground.with_ramp(Some(ramp())));
        assert_eq!(faded.fg, Some(Color::Rgb(124, 124, 124)));
        assert!(!faded.add_modifier.contains(Modifier::DIM));
        assert!(
            ink(Rgba(0x71717aff), ground)
                .add_modifier
                .contains(Modifier::DIM)
        );
    }
}
