//! GPUI Kit's theme as nsh's: the host's theme for its embedded apps.
//!
//! nsh's theme (`nsh/base`'s) has gpui-kit's color names and JSON format, so the
//! conversion is short: both modes come from the host's theme configs
//! (serialized as gpui-kit theme JSON, where the colors a theme leaves out
//! are null and nsh keeps its own), then the current mode's colors are
//! taken as GPUI Kit resolved them (with its fallbacks), so the embedded
//! components match the host's widgets exactly.

use gpui_kit::{
    Hsla,
    component::{Theme, ThemeMode},
};
use nsh_platform_gpui::{AdaptiveColor, Appearance};

/// A GPUI color as NSH's: the same `Hsla`, channel for channel.
fn color(c: Hsla) -> nsh_platform_gpui::Hsla {
    nsh_platform_gpui::Hsla {
        h: c.h,
        s: c.s,
        l: c.l,
        a: c.a,
    }
}

/// The host's light or dark mode, for [`nsh_platform_gpui::AppView::set_appearance`].
pub fn appearance(mode: ThemeMode) -> Appearance {
    if mode.is_dark() {
        Appearance::Dark
    } else {
        Appearance::Light
    }
}

/// `theme` (GPUI Kit's active theme) as nsh's theme, for
/// [`nsh_platform_gpui::AppView::set_theme`].
pub fn nsh_theme(theme: &Theme) -> nsh_platform_gpui::Theme {
    let configs = serde_json::json!({ "themes": [&*theme.light_theme, &*theme.dark_theme] });
    let mut nsh = match nsh_platform_gpui::Theme::from_json(&configs.to_string()) {
        Ok(nsh) => nsh,
        Err(err) => {
            log::warn!("the host's theme configs: {err}");
            nsh_platform_gpui::Theme::default()
        }
    };
    let dark = theme.mode.is_dark();
    let mut set = |key: &str, c: Hsla| {
        let mut slot = nsh.color(key).unwrap_or(AdaptiveColor::TRANSPARENT);
        if dark {
            slot.dark = color(c);
        } else {
            slot.light = color(c);
        }
        nsh.set_color(key, slot);
    };
    // GPUI Kit's colors by the keys of its theme files, which nsh's theme
    // reads.
    macro_rules! resolved {
        ($($field:ident = $key:literal),* $(,)?) => {
            $( set($key, theme.colors.$field); )*
        };
    }
    resolved!(
        accent = "accent.background",
        accent_foreground = "accent.foreground",
        background = "background",
        foreground = "foreground",
        border = "border",
        caret = "caret",
        danger = "danger.background",
        danger_active = "danger.active.background",
        danger_foreground = "danger.foreground",
        group_box = "group_box.background",
        group_box_foreground = "group_box.foreground",
        info = "info.background",
        info_active = "info.active.background",
        info_foreground = "info.foreground",
        input = "input.border",
        link = "link",
        link_active = "link.active",
        list = "list.background",
        list_active = "list.active.background",
        list_active_border = "list.active.border",
        list_even = "list.even.background",
        list_head = "list.head.background",
        muted = "muted.background",
        muted_foreground = "muted.foreground",
        overlay = "overlay",
        popover = "popover.background",
        popover_foreground = "popover.foreground",
        primary = "primary.background",
        primary_active = "primary.active.background",
        primary_foreground = "primary.foreground",
        progress_bar = "progress.bar.background",
        ring = "ring",
        secondary = "secondary.background",
        secondary_active = "secondary.active.background",
        secondary_foreground = "secondary.foreground",
        selection = "selection.background",
        skeleton = "skeleton.background",
        slider_bar = "slider.background",
        slider_thumb = "slider.thumb.background",
        success = "success.background",
        success_active = "success.active.background",
        success_foreground = "success.foreground",
        switch = "switch.background",
        switch_thumb = "switch.thumb.background",
        tab = "tab.background",
        tab_active = "tab.active.background",
        tab_active_foreground = "tab.active.foreground",
        tab_bar = "tab_bar.background",
        tab_bar_segmented = "tab_bar.segmented.background",
        tab_foreground = "tab.foreground",
        warning = "warning.background",
        warning_active = "warning.active.background",
        warning_foreground = "warning.foreground",
    );
    nsh.set_number("radius", f32::from(theme.radius));
    nsh.set_number("radius.lg", f32::from(theme.radius_lg));
    nsh.set_number("font.size", f32::from(theme.font_size));
    nsh
}

#[cfg(test)]
mod tests {
    use nsh_platform_gpui::Color;

    use super::*;

    #[test]
    fn colors_convert_to_rgba() {
        assert_eq!(
            Color::from(color(gpui_kit::rgb(0x336699).into())),
            Color(0x336699ff)
        );
        assert_eq!(
            Color::from(color(gpui_kit::transparent_black())),
            Color(0x0000_0000)
        );
    }

    #[test]
    fn the_current_modes_resolved_colors_win() {
        let mut theme = Theme {
            mode: ThemeMode::Dark,
            radius: gpui_kit::px(6.0),
            ..Default::default()
        };
        theme.colors.primary = gpui_kit::rgb(0x7c3aed).into();
        let nsh = nsh_theme(&theme);
        assert_eq!(Color::from(nsh.primary().dark), Color(0x7c3aedff));
        // The other mode keeps nsh's (the configs set nothing here).
        assert_eq!(
            nsh.primary().light,
            nsh_platform_gpui::Theme::default().primary().light
        );
        assert_eq!(nsh.radius(), nsh_core::Points(6.0));
    }
}
