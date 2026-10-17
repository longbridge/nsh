//! What components read from the environment: the surface, system UI, the
//! user's settings ([`super::ambient::window`] and the like).

use crate::{
    EdgeInsets, Size,
    platform::{Idiom, Locale, Os},
};

/// The environment of a surface, as the platform reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// The surface's size in points.
    pub window: Size,
    /// The edges system UI covers.
    pub safe_area: EdgeInsets,
    /// The software keyboard's height, 0 when hidden.
    pub keyboard_height: f32,
    /// How long the keyboard's show / hide animation takes, in seconds.
    pub keyboard_duration: f32,
    /// Dark appearance.
    pub dark: bool,
    /// The user's text size relative to the default.
    pub font_scale: f32,
    /// Less motion asked for.
    pub reduce_motion: bool,
    /// A screen reader runs.
    pub screen_reader: bool,
    /// Right-to-left layout.
    pub rtl: bool,
    /// Device pixels per point.
    pub pixel_ratio: f32,
    /// The operating system.
    pub os: Os,
    /// The OS version: major and minor (iOS), Android's API level and 0.
    pub os_version: (u16, u16),
    /// The kind of device.
    pub idiom: Idiom,
    /// The user's locale, for formatting numbers and dates.
    pub locale: Locale,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            window: Size::new(0.0, 0.0),
            safe_area: EdgeInsets::default(),
            keyboard_height: 0.0,
            keyboard_duration: 0.0,
            dark: false,
            font_scale: 1.0,
            reduce_motion: false,
            screen_reader: false,
            rtl: false,
            pixel_ratio: 1.0,
            os: Os::default(),
            os_version: (0, 0),
            idiom: Idiom::Phone,
            locale: Locale::EN_US,
        }
    }
}
