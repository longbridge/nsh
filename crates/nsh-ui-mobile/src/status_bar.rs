//! `StatusBar`: what the status bar shows, rendered or set.

use nsh_core::{
    platform::Os,
    ui::{StatusBarSetting, StatusBarStyle, prelude::*},
};

/// The props of [`StatusBar`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct StatusBarProps {
    /// The content's color.
    pub bar_style: Prop<Option<BarStyle>>,
    /// Hidden.
    pub hidden: Prop<Option<bool>>,
    /// Animate changes of `barStyle` and `hidden`.
    pub animated: Prop<Option<bool>>,
    /// What only Android has.
    pub android: Prop<Option<StatusBarAndroid>>,
    /// What only the web has.
    pub web: Prop<Option<StatusBarWeb>>,
}

/// The props of a [`StatusBar`] only Android has.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct StatusBarAndroid {
    /// The bar's color (before edge-to-edge, API 35).
    pub background_color: Option<Hsla>,
}

/// The props of a [`StatusBar`] only the web has.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct StatusBarWeb {
    /// The color the browser gives its own bars around the page (the
    /// page's `<meta name="theme-color">`).
    pub theme_color: Option<Hsla>,
}

/// The color of the status bar's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub enum BarStyle {
    /// The platform's default for the appearance.
    #[prop(rename = "default")]
    Default,
    /// Light content, for dark backgrounds.
    #[prop(rename = "light-content")]
    LightContent,
    /// Dark content, for light backgrounds.
    #[prop(rename = "dark-content")]
    DarkContent,
}

impl From<BarStyle> for StatusBarStyle {
    fn from(style: BarStyle) -> Self {
        match style {
            BarStyle::Default => StatusBarStyle::Default,
            BarStyle::LightContent => StatusBarStyle::Light,
            BarStyle::DarkContent => StatusBarStyle::Dark,
        }
    }
}

/// How hiding or showing the status bar animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub enum BarAnimation {
    /// At once.
    None,
    /// Fades.
    Fade,
    /// Slides.
    Slide,
}

/// What `props` set (reading subscribes): the background is the
/// browser's theme color on the web, the bar's color on Android.
fn setting(props: &StatusBarProps) -> StatusBarSetting {
    let background = if os() == Os::Web {
        props.web.get().and_then(|w| w.theme_color)
    } else {
        props.android.get().and_then(|a| a.background_color)
    };
    StatusBarSetting {
        style: props.bar_style.get().map(Into::into),
        hidden: props.hidden.get(),
        animated: props.animated.get(),
        background: background.map(Into::into),
    }
}

/// What the status bar shows while this renders: the most recently
/// mounted `StatusBar` wins, prop by prop, over what the functions set. In
/// an app embedded in a native one, the host app's view controller
/// decides on iOS.
#[component]
pub fn StatusBar(props: StatusBarProps) -> impl IntoElement {
    let entry = add_status_bar(untracked(|| setting(&props)));
    effect(move || update_status_bar(entry, setting(&props)));
    // Nothing to lay out.
    view().hidden()
}

#[module]
impl StatusBar {
    /// Sets the color of the status bar's content.
    pub fn set_bar_style(bar_style: BarStyle, animated: Option<bool>, cx: &mut ModuleCx) {
        cx.set_status_bar(StatusBarSetting {
            style: Some(bar_style.into()),
            animated: Some(animated.unwrap_or(false)),
            ..Default::default()
        });
    }

    /// Hides or shows the status bar.
    pub fn set_hidden(hidden: bool, animation: Option<BarAnimation>, cx: &mut ModuleCx) {
        cx.set_status_bar(StatusBarSetting {
            hidden: Some(hidden),
            animated: Some(animation.is_some_and(|a| a != BarAnimation::None)),
            ..Default::default()
        });
    }
}
