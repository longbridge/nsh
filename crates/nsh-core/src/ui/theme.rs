//! The theme of Rust components: semantic colors with a light and a dark
//! value, corner radii and the base font size. Field names and the JSON
//! format follow gpui-kit's themes (`primary.background`,
//! `primary.active.background`, ...), so a gpui-kit theme file styles the
//! mobile components too; keys only desktop components use are ignored.
//!
//! The app's theme is `nsh.json`'s `"theme"`: a theme file's path or its
//! content, `{ "themes": [{ "mode": "light", "colors": { … } }, { "mode":
//! "dark", … }] }`, over the default (`default-theme.json`). Colors are CSS
//! colors or Tailwind palette names (`neutral-900`).

use std::sync::LazyLock;

use super::style::AdaptiveColor;
use crate::{JsonValue, style::Color, tailwind};

macro_rules! theme_colors {
    ($( $(#[doc = $doc:literal])* $field:ident = $key:literal $(| $alias:literal)* ),* $(,)?) => {
        /// The theme's semantic colors (gpui-kit's names).
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
        }
    };
}

theme_colors! {
    /// Accents: the background of a selected or highlighted item.
    accent = "accent.background",
    /// Text on [`Self::accent`].
    accent_foreground = "accent.foreground",
    /// The app's background.
    background = "background",
    /// Text and icons on [`Self::background`].
    foreground = "foreground",
    /// Borders and dividers.
    border = "border",
    /// The text cursor.
    caret = "caret",
    /// Destructive actions.
    danger = "danger.background",
    /// [`Self::danger`] while pressed.
    danger_active = "danger.active.background",
    /// Text on [`Self::danger`].
    danger_foreground = "danger.foreground",
    /// A contained surface: cards, grouped content.
    group_box = "group_box.background",
    /// Text on [`Self::group_box`].
    group_box_foreground = "group_box.foreground",
    /// Informational messages.
    info = "info.background",
    /// [`Self::info`] while pressed.
    info_active = "info.active.background",
    /// Text on [`Self::info`].
    info_foreground = "info.foreground",
    /// The border of text inputs.
    input = "input.border",
    /// Links.
    link = "link" | "link.foreground",
    /// Links while pressed.
    link_active = "link.active" | "link.active.foreground",
    /// The background of lists.
    list = "list.background",
    /// The selected row.
    list_active = "list.active.background",
    /// The selected row's border.
    list_active_border = "list.active.border",
    /// Every other row.
    list_even = "list.even.background",
    /// List headers.
    list_head = "list.head.background",
    /// Muted surfaces.
    muted = "muted.background",
    /// Secondary text: hints, captions, placeholders.
    muted_foreground = "muted.foreground",
    /// Dims what is under a sheet or a dialog.
    overlay = "overlay",
    /// Floating surfaces: menus, popovers, sheets.
    popover = "popover.background",
    /// Text on [`Self::popover`].
    popover_foreground = "popover.foreground",
    /// The main action.
    primary = "primary.background",
    /// [`Self::primary`] while pressed.
    primary_active = "primary.active.background",
    /// Text on [`Self::primary`].
    primary_foreground = "primary.foreground",
    /// The filled part of progress bars.
    progress_bar = "progress.bar.background" | "progress_bar.background",
    /// Focus rings.
    ring = "ring",
    /// Secondary actions.
    secondary = "secondary.background",
    /// [`Self::secondary`] while pressed.
    secondary_active = "secondary.active.background",
    /// Text on [`Self::secondary`].
    secondary_foreground = "secondary.foreground",
    /// Selected text.
    selection = "selection.background",
    /// Loading placeholders.
    skeleton = "skeleton.background",
    /// A slider's filled track.
    slider_bar = "slider.background" | "slider.bar.background",
    /// A slider's thumb.
    slider_thumb = "slider.thumb.background",
    /// Success.
    success = "success.background",
    /// [`Self::success`] while pressed.
    success_active = "success.active.background",
    /// Text on [`Self::success`].
    success_foreground = "success.foreground",
    /// A switch's track while off.
    switch = "switch.background",
    /// A switch's thumb.
    switch_thumb = "switch.thumb.background",
    /// A tab.
    tab = "tab.background",
    /// The selected tab.
    tab_active = "tab.active.background",
    /// The selected tab's text.
    tab_active_foreground = "tab.active.foreground",
    /// The bar holding tabs.
    tab_bar = "tab_bar.background",
    /// A segmented tab bar.
    tab_bar_segmented = "tab_bar.segmented.background",
    /// A tab's text.
    tab_foreground = "tab.foreground",
    /// Warnings.
    warning = "warning.background",
    /// [`Self::warning`] while pressed.
    warning_active = "warning.active.background",
    /// Text on [`Self::warning`].
    warning_foreground = "warning.foreground",
}

/// The theme components render with ([`super::RenderCx::theme`]). Derefs
/// to its colors: `theme.primary`.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// The semantic colors.
    pub colors: ThemeColors,
    /// The corner radius of controls (buttons, inputs), in points.
    pub radius: f32,
    /// The corner radius of large surfaces (cards, sheets, dialogs).
    pub radius_lg: f32,
    /// The base font size, in points.
    pub font_size: f32,
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
        radius: 0.0,
        radius_lg: 0.0,
        font_size: 0.0,
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
fn color(value: &JsonValue) -> Option<Color> {
    let s = value.as_str()?;
    Color::parse(s).or_else(|| PALETTE.colors.get(s).copied())
}

type Field = fn(&mut ThemeColors) -> &mut AdaptiveColor;

/// Base colors and their pressed variants.
static PRESSED: &[(&str, &str, Field)] = &[
    ("primary.background", "primary.active.background", |c| {
        &mut c.primary_active
    }),
    ("secondary.background", "secondary.active.background", |c| {
        &mut c.secondary_active
    }),
    ("danger.background", "danger.active.background", |c| {
        &mut c.danger_active
    }),
    ("info.background", "info.active.background", |c| {
        &mut c.info_active
    }),
    ("success.background", "success.active.background", |c| {
        &mut c.success_active
    }),
    ("warning.background", "warning.active.background", |c| {
        &mut c.warning_active
    }),
    ("link", "link.active", |c| &mut c.link_active),
];

/// `c` moved `amount` of the way to gray level `level` (alpha kept): a
/// pressed shade, darker in light appearance, lighter in dark.
fn mix(c: Color, level: u8, amount: f32) -> Color {
    let channel = |shift: u32| {
        let v = ((c.0 >> shift) & 0xff) as f32;
        ((v + (f32::from(level) - v) * amount).round() as u32) << shift
    };
    Color(channel(24) | channel(16) | channel(8) | c.0 & 0xff)
}

static PALETTE: LazyLock<tailwind::Theme> = LazyLock::new(tailwind::Theme::default);

impl Theme {
    /// The default theme with `json` (a gpui-kit theme file's content)
    /// over it.
    pub fn from_json(json: &str) -> Result<Self, ThemeError> {
        let mut theme = Self::default();
        theme.apply(json)?;
        Ok(theme)
    }

    /// Applies the themes of `json` over `self`: each entry's `mode` picks
    /// the colors it sets; `radius`, `radius.lg` and `font.size` come from
    /// any entry.
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
                self.radius = r;
            }
            if let Some(r) = number("radius.lg") {
                self.radius_lg = r;
            }
            if let Some(s) = number("font.size") {
                self.font_size = s;
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
                        field.dark = mix(c, 0xff, 0.12);
                    } else {
                        field.light = mix(c, 0x00, 0.12);
                    }
                }
            }
            for (key, value) in colors {
                // Keys of desktop components (sidebars, title bars, ...)
                // have no field here.
                let Some(field) = self.colors.by_key(key) else {
                    continue;
                };
                // gpui-kit writes the colors a theme leaves unset as null.
                if value.is_null() {
                    continue;
                }
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
    use crate::ui::rgb;

    #[test]
    fn the_default_theme_has_every_color_in_both_modes() {
        let theme = Theme::default();
        assert_eq!(theme.primary.light, rgb(0x171717));
        assert_eq!(theme.primary.dark, rgb(0xfafafa));
        assert_eq!(
            (theme.radius, theme.radius_lg, theme.font_size),
            (12.0, 16.0, 16.0)
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
        assert_eq!(theme.primary.light, rgb(0x7c3aed));
        // Left out: darker in light appearance; set: as given.
        assert_eq!(theme.primary_active.light, mix(rgb(0x7c3aed), 0, 0.12));
        assert_eq!(theme.primary_active.dark, rgb(0xffffff));
        // Palette names; keys only desktop components use are ignored.
        assert_eq!(theme.border.light, PALETTE.colors["red-500"]);
        assert_eq!(theme.radius, 8.0);
        assert_eq!(theme.secondary, Theme::default().secondary);
        assert!(Theme::from_json(r#"{ "themes": [{ "colors": { "ring": 3 } }] }"#).is_err());
    }

    #[test]
    fn colors_a_gpui_kit_theme_leaves_unset_keep_their_value() {
        // gpui-kit serializes unset colors (and numbers) as null.
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
        assert_eq!(theme.ring.dark, rgb(0xff0000));
        assert_eq!(theme.ring.light, default.ring.light);
        // A null pressed color follows its base color.
        assert_eq!(theme.danger_active.dark, mix(rgb(0xff0000), 0xff, 0.12));
    }
}
