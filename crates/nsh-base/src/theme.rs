//! The theme of NSH apps' UI, as `gpui-base`'s: semantic tokens (the app's
//! background and text, borders, the main and destructive actions, muted
//! surfaces, the overlay under dialogs, focus rings) with a light and a
//! dark value each, corner radii and the base font size.
//!
//! A theme file is gpui-kit's format: `{ "themes": [{ "mode": "light",
//! "radius": 8, "colors": { "primary.background": "#7c3aed", … } }, {
//! "mode": "dark", … }] }`, colors as CSS colors or Tailwind palette names
//! (`neutral-900`). An app's (`nsh.json` `"theme"`) goes over the default
//! ([`DEFAULT_THEME`]); keys no token here names (`sidebar.background`,
//! `chart.up`, `radius.card`) are kept as they are. The core keeps the
//! result as design tokens ([`Tokens`]), which Tailwind classes name
//! (`bg-primary`, `rounded-(--radius)`), and reads `nsh.json`'s theme with
//! [`tokens_of`], which `nsh/base`'s registry brings. What draws with them
//! is the app's: no component of `nsh/base` reads them, but for the
//! default reading typography of `TextView`, whose colors are these tokens.

use std::sync::{Arc, LazyLock};

use nsh_core::{
    Color, JsonValue,
    color::{Hsla, opaque_grey},
    geometry::{Points, px},
    tailwind,
    ui::{AdaptiveColor, Tokens},
};

/// The default theme file: gpui-kit's default theme with mobile radii.
pub const DEFAULT_THEME: &str = include_str!("default-theme.json");

static DEFAULT: LazyLock<Theme> = LazyLock::new(|| {
    let mut tokens = Tokens::default();
    apply(&mut tokens, DEFAULT_THEME).expect("the default theme parses");
    Theme(Arc::new(tokens))
});

static PALETTE: LazyLock<tailwind::Theme> = LazyLock::new(tailwind::Theme::default);

/// A theme: design tokens with the semantic ones named. Cheap to clone.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme(Arc<Tokens>);

impl Default for Theme {
    /// The default theme ([`DEFAULT_THEME`]).
    fn default() -> Self {
        DEFAULT.clone()
    }
}

/// A theme file that does not parse.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeError(pub String);

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ThemeError {}

/// The theme the app is drawn with (reading it subscribes: a new theme
/// reaches what read it).
pub fn theme() -> Theme {
    Theme(nsh_core::ui::ambient::tokens())
}

/// The tokens of theme file `json` over the default's (`None`: the
/// default's): how the core reads `nsh.json`'s `"theme"`
/// ([`nsh_core::ui::Registry::tokens_from`]).
pub fn tokens_of(json: Option<&str>) -> Result<Tokens, String> {
    match json {
        Some(json) => Theme::from_json(json)
            .map(Theme::into_tokens)
            .map_err(|e| e.0),
        None => Ok(Theme::default().into_tokens()),
    }
}

macro_rules! semantic {
    ($( $(#[doc = $doc:literal])* $name:ident = $key:literal ),* $(,)?) => {
        impl Theme {
            $(
                $(#[doc = $doc])*
                #[doc = concat!("\n\nKey `", $key, "`.")]
                pub fn $name(&self) -> AdaptiveColor {
                    self.semantic($key)
                }
            )*
        }
    };
}

semantic! {
    /// The app's background.
    background = "background",
    /// Text and icons on [`Theme::background`].
    foreground = "foreground",
    /// Borders and dividers.
    border = "border",
    /// Destructive actions.
    danger = "danger.background",
    /// [`Theme::danger`] while pressed.
    danger_active = "danger.active.background",
    /// Text on [`Theme::danger`].
    danger_foreground = "danger.foreground",
    /// Muted surfaces.
    muted = "muted.background",
    /// Secondary text: hints, captions, placeholders.
    muted_foreground = "muted.foreground",
    /// Dims what is under a sheet or a dialog.
    overlay = "overlay",
    /// The main action.
    primary = "primary.background",
    /// [`Theme::primary`] while pressed.
    primary_active = "primary.active.background",
    /// Text on [`Theme::primary`].
    primary_foreground = "primary.foreground",
    /// Focus rings.
    ring = "ring",
    /// Links in text.
    link = "link",
    /// Information: notes, hints.
    info = "info.background",
    /// Success: done, tips.
    success = "success.background",
    /// Warnings.
    warning = "warning.background",
    /// The background of selected text.
    selection = "selection.background",
    /// The header rows of tables and lists.
    list_head = "list.head.background",
}

impl Theme {
    /// The default theme with theme file `json` over it.
    pub fn from_json(json: &str) -> Result<Self, ThemeError> {
        let mut tokens = (*DEFAULT.0).clone();
        apply(&mut tokens, json)?;
        Ok(Self(Arc::new(tokens)))
    }

    /// The corner radius of controls (buttons, inputs). Key `radius`.
    pub fn radius(&self) -> Points {
        self.size("radius")
    }

    /// The corner radius of large surfaces (cards, sheets, dialogs). Key
    /// `radius.lg`.
    pub fn radius_lg(&self) -> Points {
        self.size("radius.lg")
    }

    /// The base font size. Key `font.size`.
    pub fn font_size(&self) -> Points {
        self.size("font.size")
    }

    /// The color of key `key` (any key the theme sets).
    pub fn color(&self, key: &str) -> Option<AdaptiveColor> {
        self.0.color(key)
    }

    /// The number of key `key` (any key the theme sets).
    pub fn number(&self, key: &str) -> Option<f32> {
        self.0.number(key)
    }

    /// Sets the color of key `key`.
    pub fn set_color(&mut self, key: &str, color: AdaptiveColor) {
        Arc::make_mut(&mut self.0).set_color(key, color);
    }

    /// Sets the number of key `key`.
    pub fn set_number(&mut self, key: &str, value: f32) {
        Arc::make_mut(&mut self.0).set_number(key, value);
    }

    /// Its design tokens.
    pub fn tokens(&self) -> &Tokens {
        &self.0
    }

    /// Its design tokens, for the core ([`nsh_core::ui::Root::tokens`],
    /// an engine's host theme).
    pub fn into_tokens(self) -> Tokens {
        Arc::unwrap_or_clone(self.0)
    }

    /// A semantic color: the theme's, else the default's (tokens an
    /// embedding app gives may leave it out).
    fn semantic(&self, key: &str) -> AdaptiveColor {
        self.color(key)
            .or_else(|| DEFAULT.color(key))
            .unwrap_or(AdaptiveColor::TRANSPARENT)
    }

    fn size(&self, key: &str) -> Points {
        px(self
            .number(key)
            .or_else(|| DEFAULT.number(key))
            .unwrap_or(0.0))
    }
}

impl From<Theme> for Tokens {
    fn from(theme: Theme) -> Self {
        theme.into_tokens()
    }
}

/// A CSS color or a Tailwind palette name.
fn color(value: &str) -> Option<Hsla> {
    Color::parse(value)
        .or_else(|| PALETTE.color(value))
        .map(Hsla::from)
}

/// The pressed shade of `c`: 12% black over it in light appearance, 12%
/// white in dark (its alpha kept).
fn pressed(c: Hsla, dark: bool) -> Hsla {
    c.blend(opaque_grey(if dark { 1.0 } else { 0.0 }, 0.12))
}

/// Applies the themes of `json` over `tokens`: each entry's `mode` picks
/// the appearance of the colors it sets (a key `tokens` lacks takes the
/// color in both until a mode sets its own); numbers come from any entry.
/// A pressed color (`X.active.background`) an entry leaves out (or null)
/// where it sets the color it is pressed on (`X.background`) follows that
/// color, when the theme under it has one.
fn apply(tokens: &mut Tokens, json: &str) -> Result<(), ThemeError> {
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
        if let JsonValue::Object(fields) = entry {
            for (key, value) in fields {
                if let Some(n) = value.as_f64() {
                    tokens.set_number(key, n as f32);
                }
            }
        }
        let Some(JsonValue::Object(colors)) = entry.get("colors") else {
            continue;
        };
        let set = |tokens: &mut Tokens, key: &str, c: Hsla| {
            let mut slot = tokens.color(key).unwrap_or(AdaptiveColor::from(c));
            if dark {
                slot.dark = c;
            } else {
                slot.light = c;
            }
            tokens.set_color(key, slot);
        };
        for (key, value) in colors {
            let Some(prefix) = key.strip_suffix(".background") else {
                continue;
            };
            let active = format!("{prefix}.active.background");
            if colors.get(&active).is_none_or(JsonValue::is_null)
                && tokens.color(&active).is_some()
                && let Some(c) = value.as_str().and_then(color)
            {
                set(tokens, &active, pressed(c, dark));
            }
        }
        for (key, value) in colors {
            // A theme file may write the colors it leaves unset as null.
            let value = match value {
                JsonValue::Null => continue,
                JsonValue::String(s) => s,
                v => return Err(ThemeError(format!("`{key}`: not a color: {v}"))),
            };
            // Values NSH does not read (gradients) are left out.
            if let Some(c) = color(value) {
                set(tokens, key, c);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use nsh_core::color::rgb;

    use super::*;

    fn hsla(hex: u32) -> Hsla {
        rgb(hex).into()
    }

    #[test]
    fn the_default_theme_has_every_token_in_both_modes() {
        let theme = Theme::default();
        assert_eq!(theme.primary().light, hsla(0x171717));
        assert_eq!(theme.primary().dark, hsla(0xfafafa));
        assert_eq!(
            (theme.radius(), theme.radius_lg(), theme.font_size()),
            (px(12.0), px(16.0), px(16.0))
        );
        // Tokens without them read the defaults.
        assert_eq!(Theme(Arc::default()).ring(), theme.ring());
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
        let mut colors: Vec<Color> = Vec::new();
        for (_, c) in Theme::default().tokens().colors() {
            colors.extend([Color::from(c.light), Color::from(c.dark)]);
        }
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
    fn a_theme_overrides_the_default_and_derives_pressed_colors() {
        let theme = Theme::from_json(
            r##"{ "themes": [
                { "mode": "light", "radius": 8, "colors": {
                    "primary.background": "#7c3aed", "border": "red-500", "sidebar.background": "#000" } },
                { "mode": "dark", "colors": { "primary.background": "#a78bfa", "primary.active.background": "#ffffff" } }
            ] }"##,
        )
        .unwrap();
        assert_eq!(theme.primary().light, hsla(0x7c3aed));
        // Left out: its base color's pressed shade; set: as given.
        assert_eq!(theme.primary_active().light, pressed(hsla(0x7c3aed), false));
        assert_eq!(theme.primary_active().dark, hsla(0xffffff));
        assert_eq!(
            theme.border().light,
            Hsla::from(PALETTE.color("red-500").unwrap())
        );
        // A mode the theme leaves out stays the default's.
        assert_eq!(theme.border().dark, Theme::default().border().dark);
        assert_eq!(theme.radius(), px(8.0));
        assert!(theme.color("sidebar.background").is_some());
        assert!(Theme::from_json(r#"{ "themes": [{ "colors": { "ring": 3 } }] }"#).is_err());
    }

    #[test]
    fn keys_no_token_names_are_kept() {
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
        // Values NSH does not read are left out.
        assert_eq!(theme.color("title_bar.background"), None);
        assert_eq!(theme.number("radius.card"), Some(10.0));
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
        assert_eq!(theme.primary(), default.primary());
        assert_eq!(theme.primary_active(), default.primary_active());
        assert_eq!(theme.radius(), default.radius());
        assert_eq!(theme.ring().dark, hsla(0xff0000));
        assert_eq!(theme.ring().light, default.ring().light);
        // A null pressed color follows its base color.
        assert_eq!(theme.danger_active().dark, pressed(hsla(0xff0000), true));
    }

    /// Text on the status colors, at rest and pressed, reads in both
    /// appearances: a contrast of at least 4.5:1 (WCAG AA for text the
    /// size of a button's label).
    #[test]
    fn text_on_status_colors_stands_out() {
        let theme = Theme::default();
        for status in ["danger", "info", "success", "warning"] {
            let text = theme.color(&format!("{status}.foreground")).unwrap();
            for key in [
                format!("{status}.background"),
                format!("{status}.active.background"),
            ] {
                let background = theme.color(&key).unwrap();
                for (b, t) in [(background.light, text.light), (background.dark, text.dark)] {
                    let contrast = b.contrast_ratio(t);
                    assert!(contrast >= 4.5, "{key}: {b:?} / {t:?}: {contrast}");
                }
            }
        }
    }
}
