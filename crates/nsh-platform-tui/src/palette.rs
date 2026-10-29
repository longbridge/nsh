//! The terminal's look: app colors drawn with the terminal's own palette.
//!
//! An app's RGBA color becomes one of the 16-color meanings, so the user's
//! terminal theme decides the actual colors. The color's own alpha is first
//! blended over the page of the app's appearance (black when dark, white when
//! light); view opacity never changes a color.
//!
//! - **Saturated** colors (chroma at least [`SATURATED_CHROMA`]) take the
//!   accent of their hue band, at normal intensity: red, yellow (orange and
//!   amber), green (lime and emerald), cyan (teal), blue (violet) or magenta
//!   (purple and pink).
//! - **Near-neutral** colors are the default foreground, or dim when their
//!   lightness is closer to the page's than [`DEFAULT_CONTRAST`] (secondary
//!   text, placeholders, separators).
//! - A fully transparent color hides what it draws.

use nsh_core::platform::ColorScheme;
use ratatui_core::style::{Color, Modifier, Style};

/// The chroma (largest minus smallest channel, of 255) from which a color
/// reads as an accent rather than a shade of gray.
const SATURATED_CHROMA: f32 = 0.2 * 255.0;
/// The CIE L* distance from the page from which a neutral color is the
/// default foreground rather than dim.
const DEFAULT_CONTRAST: f32 = 75.0;
/// Hue bands (degrees, upper bounds) and the accent each one reads as.
const HUE_BANDS: [(f32, Color); 6] = [
    (15.0, Color::Red),
    (75.0, Color::Yellow),
    (165.0, Color::Green),
    (200.0, Color::Cyan),
    (265.0, Color::Blue),
    (335.0, Color::Magenta),
];

/// The terminal style `color` draws with in `scheme`: a foreground from the
/// palette and the dim (or hidden) attribute, which replace the Cell's.
pub(crate) fn ink(color: nsh_core::Color, scheme: ColorScheme) -> Style {
    let [red, green, blue, alpha] = color.0.to_be_bytes();
    if alpha == 0 {
        return Style::new()
            .fg(Color::Reset)
            .add_modifier(Modifier::HIDDEN)
            .remove_modifier(Modifier::DIM);
    }
    let page = match scheme {
        ColorScheme::Dark => 0.0,
        ColorScheme::Light => 255.0,
    };
    let alpha = f32::from(alpha) / 255.0;
    let [red, green, blue] =
        [red, green, blue].map(|channel| f32::from(channel) * alpha + page * (1.0 - alpha));
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let plain = Style::new().remove_modifier(Modifier::HIDDEN);
    if max - min >= SATURATED_CHROMA {
        let accent = HUE_BANDS
            .iter()
            .find(|(bound, _)| hue(red, green, blue, max, min) < *bound)
            .map_or(Color::Red, |(_, accent)| *accent);
        return plain.fg(accent).remove_modifier(Modifier::DIM);
    }
    let contrast = (lightness(red, green, blue) - lightness(page, page, page)).abs();
    if contrast < DEFAULT_CONTRAST {
        plain.fg(Color::Reset).add_modifier(Modifier::DIM)
    } else {
        plain.fg(Color::Reset).remove_modifier(Modifier::DIM)
    }
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

#[cfg(test)]
mod tests {
    use nsh_core::{Color as Rgba, platform::ColorScheme};
    use ratatui_core::style::{Color, Modifier};

    use super::ink;

    fn reads(color: u32, scheme: ColorScheme) -> (Color, bool) {
        let style = ink(Rgba(color), scheme);
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
        assert_eq!(reads(0xd8d8d1ff, Light), (Color::Reset, true));
        assert_eq!(reads(0x64748bff, Light), (Color::Reset, true));
    }

    #[test]
    fn saturated_colors_take_the_accent_of_their_hue_band() {
        let accent = |color| reads(color, ColorScheme::Dark).0;
        assert_eq!(accent(0xef4444ff), Color::Red);
        assert_eq!(accent(0xf43f5eff), Color::Red);
        assert_eq!(accent(0xf97316ff), Color::Yellow);
        assert_eq!(accent(0xf59e0bff), Color::Yellow);
        assert_eq!(accent(0xc8f26dff), Color::Green);
        assert_eq!(accent(0x45d49aff), Color::Green);
        assert_eq!(accent(0x14b8a6ff), Color::Cyan);
        assert_eq!(accent(0x7ea8ffff), Color::Blue);
        assert_eq!(accent(0x8b5cf6ff), Color::Blue);
        assert_eq!(accent(0xa855f7ff), Color::Magenta);
        assert_eq!(accent(0xec4899ff), Color::Magenta);
        assert!(!reads(0x3d63d1ff, ColorScheme::Light).1);
    }

    #[test]
    fn translucent_colors_blend_over_the_page_and_transparent_ones_hide() {
        assert_eq!(reads(0xef44441a, ColorScheme::Dark), (Color::Reset, true));
        let hidden = ink(Rgba(0xef444400), ColorScheme::Dark);
        assert!(hidden.add_modifier.contains(Modifier::HIDDEN));
    }
}
