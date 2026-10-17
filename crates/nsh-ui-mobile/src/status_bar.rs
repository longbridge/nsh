//! `StatusBar`: what the status bar shows, rendered or set.

use nsh_core::{
    platform::Os,
    ui::{StatusBarEntry, StatusBarSetting, StatusBarStyle, prelude::*},
};

/// A rendered status bar setting ([`StatusBarProps`]).
#[derive(Debug, Default)]
pub struct StatusBar {
    /// Its place among the surface's settings.
    entry: Option<StatusBarEntry>,
}

/// What the status bar shows while this renders: the most recently
/// mounted `StatusBar` wins, prop by prop, over what the functions set. In
/// an app embedded in a native one, the host app's view controller
/// decides on iOS.
#[derive(Debug, Clone, PartialEq, Props)]
pub struct StatusBarProps {
    /// The content's color.
    pub bar_style: Option<BarStyle>,
    /// Hidden.
    pub hidden: Option<bool>,
    /// Animate changes of `barStyle` and `hidden`.
    pub animated: Option<bool>,
    /// What only Android has.
    pub android: Option<StatusBarAndroid>,
    /// What only the web has.
    pub web: Option<StatusBarWeb>,
}

/// The props of a [`StatusBar`] only Android has.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct StatusBarAndroid {
    /// The bar's color (before edge-to-edge, API 35).
    pub background_color: Option<Color>,
}

/// The props of a [`StatusBar`] only the web has.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct StatusBarWeb {
    /// The color the browser gives its own bars around the page (the
    /// page's `<meta name="theme-color">`).
    pub theme_color: Option<Color>,
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

fn setting(props: &StatusBarProps, os: Os) -> StatusBarSetting {
    let background = if os == Os::Web {
        props.web.as_ref().and_then(|w| w.theme_color)
    } else {
        props.android.as_ref().and_then(|a| a.background_color)
    };
    StatusBarSetting {
        style: props.bar_style.map(Into::into),
        hidden: props.hidden,
        animated: props.animated,
        background,
    }
}

impl Component for StatusBar {
    type Props = StatusBarProps;
    type Event = NoEvent;

    fn setup(&mut self, props: &StatusBarProps, cx: &mut Context<'_, Self>) {
        let setting = setting(props, cx.environment().os);
        self.entry = Some(cx.add_status_bar(setting));
    }

    fn props_changed(
        &mut self,
        _old: &StatusBarProps,
        new: &StatusBarProps,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(entry) = self.entry {
            let setting = setting(new, cx.environment().os);
            cx.update_status_bar(entry, setting);
        }
    }

    fn cleanup(&mut self, _props: &StatusBarProps, cx: &mut Context<'_, Self>) {
        if let Some(entry) = self.entry.take() {
            cx.remove_status_bar(entry);
        }
    }

    fn render(&self, _props: &StatusBarProps, _cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        // Nothing to lay out.
        view().hidden()
    }
}

/// The status bar, set imperatively (under the rendered `StatusBar`s).
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
