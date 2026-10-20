//! The theme of Rust components: semantic colors with a light and a dark
//! value, corner radii and the base font size. A theme file names the
//! colors by role and state (`primary.background`,
//! `primary.active.background`, ...); keys no component uses leave the
//! components as they are.
//!
//! The app's theme is `nsh.json`'s `"theme"`: a theme file's path or its
//! content, `{ "themes": [{ "mode": "light", "colors": { … } }, { "mode":
//! "dark", … }] }`, over the default (`default-theme.json`). Colors are CSS
//! colors or Tailwind palette names (`neutral-900`).
//!
//! Every color and number of a theme is also a token the app's Tailwind
//! classes name (`bg-primary`, `rounded-(--radius)`; `tailwind/theme.rs`
//! `Tokens`), including the keys no component uses (`sidebar.background`,
//! `chart.up`, `radius.card`): those are kept by key ([`Theme::color`],
//! [`Theme::number`]).

use std::sync::LazyLock;

use rustc_hash::FxHashMap;

use super::style::AdaptiveColor;
use crate::{
    JsonValue,
    color::{Hsla, opaque_grey},
    geometry::Points,
    style::Color,
    tailwind,
};

macro_rules! theme_colors {
    ($( $(#[doc = $doc:literal])* $field:ident = $key:literal $(| $alias:literal)* ),* $(,)?) => {
        /// The theme's semantic colors.
        #[derive(Debug, Clone, Copy, PartialEq, Default)]
        pub struct ThemeColors {
            $( $(#[doc = $doc])* pub $field: AdaptiveColor, )*
        }

        impl ThemeColors {
            /// The color of JSON key `key`.
            fn by_key(&mut self, key: &str) -> Option<&mut AdaptiveColor> {
                Some(match key {
                    $( $key $(| $alias)* => &mut self.$field, )*
                    _ => return None,
                })
            }

            /// The color of JSON key `key`.
            fn get(&self, key: &str) -> Option<AdaptiveColor> {
                Some(match key {
                    $( $key $(| $alias)* => self.$field, )*
                    _ => return None,
                })
            }

            /// Each color under each of its keys.
            fn each(&self, mut f: impl FnMut(&str, AdaptiveColor)) {
                $( $( f($alias, self.$field); )* f($key, self.$field); )*
            }
        }
    };
}

theme_colors! {
    /// The app's background.
    background = "background",
    /// Text and icons on [`Self::background`].
    foreground = "foreground",
    /// Borders and dividers.
    border = "border",
    /// Destructive actions.
    danger = "danger.background",
    /// [`Self::danger`] while pressed.
    danger_active = "danger.active.background",
    /// Text on [`Self::danger`].
    danger_foreground = "danger.foreground",
    /// Muted surfaces.
    muted = "muted.background",
    /// Secondary text: hints, captions, placeholders.
    muted_foreground = "muted.foreground",
    /// Dims what is under a sheet or a dialog.
    overlay = "overlay",
    /// The main action.
    primary = "primary.background",
    /// [`Self::primary`] while pressed.
    primary_active = "primary.active.background",
    /// Text on [`Self::primary`].
    primary_foreground = "primary.foreground",
    /// Focus rings.
    ring = "ring",
}

/// The theme components are drawn with ([`super::ambient::theme`]). Derefs
/// to its colors: `theme.primary`.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// The semantic colors.
    pub colors: ThemeColors,
    /// The corner radius of controls (buttons, inputs).
    pub radius: Points,
    /// The corner radius of large surfaces (cards, sheets, dialogs).
    pub radius_lg: Points,
    /// The base font size.
    pub font_size: Points,
    /// Colors and numbers under the keys the fields above do not have.
    custom: Custom,
}

/// A theme's colors and numbers under keys no component uses.
#[derive(Debug, Clone, PartialEq, Default)]
struct Custom {
    colors: FxHashMap<Box<str>, AdaptiveColor>,
    numbers: FxHashMap<Box<str>, f32>,
}

impl std::ops::Deref for Theme {
    type Target = ThemeColors;

    fn deref(&self) -> &ThemeColors {
        &self.colors
    }
}

/// A theme that does not parse.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeError(pub String);

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ThemeError {}

static DEFAULT: LazyLock<Theme> = LazyLock::new(|| {
    let mut theme = Theme {
        colors: ThemeColors::default(),
        radius: Points(0.0),
        radius_lg: Points(0.0),
        font_size: Points(0.0),
        custom: Custom::default(),
    };
    theme
        .apply(include_str!("default-theme.json"))
        .expect("the default theme parses");
    theme
});

impl Default for Theme {
    fn default() -> Self {
        DEFAULT.clone()
    }
}

/// A CSS color or a Tailwind palette name.
fn color(value: &JsonValue) -> Option<Hsla> {
    let s = value.as_str()?;
    Color::parse(s)
        .or_else(|| PALETTE.colors.get(s).copied())
        .map(Hsla::from)
}

type Field = fn(&mut ThemeColors) -> &mut AdaptiveColor;

/// Base colors and their pressed variants.
static PRESSED: &[(&str, &str, Field)] = &[
    ("primary.background", "primary.active.background", |c| {
        &mut c.primary_active
    }),
    ("danger.background", "danger.active.background", |c| {
        &mut c.danger_active
    }),
];

/// The pressed shade of `c`: 12% black over it in light appearance, 12%
/// white in dark (its alpha kept).
pub(crate) fn pressed(c: Hsla, dark: bool) -> Hsla {
    c.blend(opaque_grey(if dark { 1.0 } else { 0.0 }, 0.12))
}

static PALETTE: LazyLock<tailwind::Theme> = LazyLock::new(tailwind::Theme::default);

impl Theme {
    /// The default theme with `json` (a theme file's content) over it.
    pub fn from_json(json: &str) -> Result<Self, ThemeError> {
        let mut theme = Self::default();
        theme.apply(json)?;
        Ok(theme)
    }

    /// The color of theme key `key` (`primary.background`,
    /// `sidebar.background`, any key a theme sets).
    pub fn color(&self, key: &str) -> Option<AdaptiveColor> {
        self.colors
            .get(key)
            .or_else(|| self.custom.colors.get(key).copied())
    }

    /// Sets the color of theme key `key`: a field's when it has one, else
    /// a custom token apps name in classes.
    pub fn set_color(&mut self, key: &str, color: AdaptiveColor) {
        match self.colors.by_key(key) {
            Some(field) => *field = color,
            None => {
                self.custom.colors.insert(key.into(), color);
            }
        }
    }

    /// The number of theme key `key` (`radius`, `radius.lg`, `font.size`,
    /// any other number a theme sets).
    pub fn number(&self, key: &str) -> Option<f32> {
        match key {
            "radius" => Some(self.radius.0),
            "radius.lg" => Some(self.radius_lg.0),
            "font.size" => Some(self.font_size.0),
            key => self.custom.numbers.get(key).copied(),
        }
    }

    /// Sets the number of theme key `key`.
    pub fn set_number(&mut self, key: &str, value: f32) {
        match key {
            "radius" => self.radius = Points(value),
            "radius.lg" => self.radius_lg = Points(value),
            "font.size" => self.font_size = Points(value),
            key => {
                self.custom.numbers.insert(key.into(), value);
            }
        }
    }

    /// Each color by key (a field's under each of its keys).
    pub(crate) fn each_color(&self, mut f: impl FnMut(&str, AdaptiveColor)) {
        self.colors.each(&mut f);
        for (key, c) in &self.custom.colors {
            f(key, *c);
        }
    }

    /// Each number by key.
    pub(crate) fn each_number(&self, mut f: impl FnMut(&str, f32)) {
        f("radius", self.radius.0);
        f("radius.lg", self.radius_lg.0);
        f("font.size", self.font_size.0);
        for (key, n) in &self.custom.numbers {
            f(key, *n);
        }
    }

    /// Takes the custom colors and numbers `fallback` has and `self` does
    /// not: a host's theme keeps the app's own tokens it does not know.
    pub(crate) fn inherit(&mut self, fallback: &Theme) {
        for (key, c) in &fallback.custom.colors {
            self.custom.colors.entry(key.clone()).or_insert(*c);
        }
        for (key, n) in &fallback.custom.numbers {
            self.custom.numbers.entry(key.clone()).or_insert(*n);
        }
    }

    /// Applies the themes of `json` over `self`: each entry's `mode` picks
    /// the colors it sets; `radius`, `radius.lg`, `font.size` and other
    /// numbers come from any entry.
    fn apply(&mut self, json: &str) -> Result<(), ThemeError> {
        let value: JsonValue =
            serde_json::from_str(json).map_err(|e| ThemeError(format!("not JSON: {e}")))?;
        let entries = match value.get("themes") {
            Some(JsonValue::Array(entries)) => entries.as_slice(),
            _ => return Err(ThemeError("expected `{ \"themes\": [...] }`".into())),
        };
        for entry in entries {
            let dark = match entry.get("mode").and_then(JsonValue::as_str) {
                Some("dark") => true,
                Some("light") | None => false,
                Some(m) => return Err(ThemeError(format!("unknown mode `{m}`"))),
            };
            let number = |key: &str| entry.get(key).and_then(JsonValue::as_f64).map(|n| n as f32);
            if let Some(r) = number("radius") {
                self.radius = Points(r);
            }
            if let Some(r) = number("radius.lg") {
                self.radius_lg = Points(r);
            }
            if let Some(s) = number("font.size") {
                self.font_size = Points(s);
            }
            // Other numbers (`radius.card`, `mono_font.size`) are
            // tokens for classes.
            if let JsonValue::Object(fields) = entry {
                for (key, value) in fields {
                    if let Some(n) = value.as_f64()
                        && !matches!(key.as_str(), "radius" | "radius.lg" | "font.size")
                    {
                        self.custom.numbers.insert(key.as_str().into(), n as f32);
                    }
                }
            }
            let Some(JsonValue::Object(colors)) = entry.get("colors") else {
                continue;
            };
            // A pressed color the theme leaves out (or null) follows its
            // base color.
            for (base, active, field) in PRESSED {
                if colors.get(*active).is_none_or(JsonValue::is_null)
                    && let Some(c) = colors.get(*base).and_then(color)
                {
                    let field = field(&mut self.colors);
                    if dark {
                        field.dark = pressed(c, true);
                    } else {
                        field.light = pressed(c, false);
                    }
                }
            }
            for (key, value) in colors {
                // A theme file may write the colors it leaves unset as null.
                if value.is_null() {
                    continue;
                }
                // Keys of desktop components (sidebars, title bars, ...)
                // and the app's own have no field here: tokens for
                // classes, set in both modes until a mode sets its own.
                // Values NSH does not read (gradients) are left
                // out.
                let Some(field) = self.colors.by_key(key) else {
                    if let Some(c) = color(value) {
                        let custom = self
                            .custom
                            .colors
                            .entry(key.as_str().into())
                            .or_insert(AdaptiveColor::from(c));
                        if dark {
                            custom.dark = c;
                        } else {
                            custom.light = c;
                        }
                    }
                    continue;
                };
                let c = color(value)
                    .ok_or_else(|| ThemeError(format!("`{key}`: not a color: {value}")))?;
                if dark {
                    field.dark = c;
                } else {
                    field.light = c;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{color::rgb, geometry::px};

    fn hsla(hex: u32) -> Hsla {
        rgb(hex).into()
    }

    /// The pressed shades the theme derived before it computed them with
    /// `Hsla::blend`: each channel moved 12% of the way to black or white.
    #[test]
    fn pressed_shades_pack_as_before() {
        let before = |c: Color, level: u8| {
            let channel = |shift: u32| {
                let v = ((c.0 >> shift) & 0xff) as f32;
                ((v + (f32::from(level) - v) * 0.12).round() as u32) << shift
            };
            Color(channel(24) | channel(16) | channel(8) | c.0 & 0xff)
        };
        let mut colors: Vec<Color> = PALETTE.colors.values().copied().collect();
        Theme::default()
            .each_color(|_, c| colors.extend([Color::from(c.light), Color::from(c.dark)]));
        colors.extend((0..=0xffu32).map(|v| Color(v << 24 | v << 16 | v << 8 | 0xff)));
        for c in colors {
            assert_eq!(Color::from(pressed(c.into(), false)), before(c, 0), "{c:?}");
            assert_eq!(
                Color::from(pressed(c.into(), true)),
                before(c, 0xff),
                "{c:?}"
            );
        }
    }

    #[test]
    fn the_default_theme_has_every_color_in_both_modes() {
        let theme = Theme::default();
        assert_eq!(theme.primary.light, hsla(0x171717));
        assert_eq!(theme.primary.dark, hsla(0xfafafa));
        assert_eq!(
            (theme.radius, theme.radius_lg, theme.font_size),
            (px(12.0), px(16.0), px(16.0))
        );
    }

    #[test]
    fn a_theme_overrides_the_default_and_derives_pressed_colors() {
        let theme = Theme::from_json(
            r##"{ "themes": [
                { "mode": "light", "radius": 8, "colors": {
                    "primary.background": "#7c3aed", "border": "red-500", "sidebar.background": "#000" } },
                { "mode": "dark", "colors": { "primary.background": "#a78bfa", "primary.active.background": "#ffffff" } }
            ] }"##,
        )
        .unwrap();
        assert_eq!(theme.primary.light, hsla(0x7c3aed));
        // Left out: darker in light appearance; set: as given.
        assert_eq!(theme.primary_active.light, pressed(hsla(0x7c3aed), false));
        assert_eq!(theme.primary_active.dark, hsla(0xffffff));
        // Palette names; keys only desktop components use are ignored.
        assert_eq!(theme.border.light, PALETTE.colors["red-500"].into());
        assert_eq!(theme.radius, px(8.0));
        assert_eq!(theme.ring, Theme::default().ring);
        assert!(Theme::from_json(r#"{ "themes": [{ "colors": { "ring": 3 } }] }"#).is_err());
    }

    #[test]
    fn keys_no_component_uses_are_kept_as_tokens() {
        let theme = Theme::from_json(
            r##"{ "themes": [
                { "mode": "dark", "radius.card": 10, "colors": {
                    "sidebar.background": "#111111", "chart.up": "green-500",
                    "title_bar.background": "linear-gradient(red, blue)" } },
                { "mode": "light", "colors": { "sidebar.background": "#eeeeee" } }
            ] }"##,
        )
        .unwrap();
        let sidebar = theme.color("sidebar.background").unwrap();
        assert_eq!(
            (sidebar.light, sidebar.dark),
            (hsla(0xeeeeee), hsla(0x111111))
        );
        // Set in one mode only: both.
        let up = theme.color("chart.up").unwrap();
        assert_eq!(up.light, up.dark);
        // Values NSH does not read are left out, as before.
        assert_eq!(theme.color("title_bar.background"), None);
        assert_eq!(theme.color("primary.background"), Some(theme.primary));
        assert_eq!(theme.number("radius.card"), Some(10.0));
        assert_eq!(theme.number("radius"), Some(theme.radius.0));
        let mut host = Theme::default();
        host.set_color("sidebar.background", hsla(0x222222).into());
        host.inherit(&theme);
        assert_eq!(
            host.color("sidebar.background").unwrap().dark,
            hsla(0x222222)
        );
        assert_eq!(host.color("chart.up"), Some(up));
        assert_eq!(host.number("radius.card"), Some(10.0));
    }

    #[test]
    fn colors_a_theme_leaves_unset_keep_their_value() {
        // A theme file may write unset colors (and numbers) as null.
        let theme = Theme::from_json(
            r##"{ "themes": [{ "mode": "dark", "radius": null, "colors": {
                "primary.background": null, "primary.active.background": null,
                "danger.background": "#ff0000", "danger.active.background": null,
                "ring": "#ff0000", "sidebar.background": null } }] }"##,
        )
        .unwrap();
        let default = Theme::default();
        assert_eq!(theme.primary, default.primary);
        assert_eq!(theme.primary_active, default.primary_active);
        assert_eq!(theme.radius, default.radius);
        assert_eq!(theme.ring.dark, hsla(0xff0000));
        assert_eq!(theme.ring.light, default.ring.light);
        // A null pressed color follows its base color.
        assert_eq!(theme.danger_active.dark, pressed(hsla(0xff0000), true));
    }

    /// Text on the status colors, at rest and pressed, reads in both
    /// appearances: a contrast of at least 4.5:1 (WCAG AA for text the
    /// size of a button's label).
    #[test]
    fn text_on_status_colors_stands_out() {
        let theme = Theme::default();
        // WCAG's relative luminance.
        let contrast = |a: Hsla, b: Hsla| a.contrast_ratio(b);
        for (background, text) in [
            "danger.background",
            "danger.active.background",
            "info.background",
            "info.active.background",
            "success.background",
            "success.active.background",
            "warning.background",
            "warning.active.background",
        ]
        .map(|key| {
            let status = key.split('.').next().unwrap();
            let color = |key: &str| theme.color(key).unwrap();
            (color(key), color(&format!("{status}.foreground")))
        }) {
            for (b, t) in [(background.light, text.light), (background.dark, text.dark)] {
                assert!(contrast(b, t) >= 4.5, "{b:?} / {t:?}: {}", contrast(b, t));
            }
        }
    }
}
