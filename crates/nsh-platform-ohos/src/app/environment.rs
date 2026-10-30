//! Host observations of environment settings, forwarded to the shared core.

use napi_derive_ohos::napi;
use nsh_engine::{ColorScheme, Environment, Idiom, Locale};

/// Environment changes observed by the embedding host. Omitted fields keep
/// their current values. Insets and keyboard height are relative to the surface in vp.
#[napi(object)]
#[derive(Clone, Default, PartialEq)]
pub struct NshEnvironment {
    /// Safe inset at the top, in vp.
    pub safe_top: Option<f64>,
    /// Safe inset at the right, in vp.
    pub safe_right: Option<f64>,
    /// Safe inset at the bottom, in vp.
    pub safe_bottom: Option<f64>,
    /// Safe inset at the left, in vp.
    pub safe_left: Option<f64>,
    /// Keyboard overlap in vp; zero when hidden.
    pub keyboard_height: Option<f64>,
    /// Keyboard animation duration in seconds.
    pub keyboard_duration: Option<f64>,
    /// Preferred text scale (0.25 through 8).
    pub font_scale: Option<f64>,
    /// System appearance: `light` or `dark`.
    pub color_scheme: Option<String>,
    /// BCP 47 locale tag.
    pub locale: Option<String>,
    /// System layout direction.
    pub rtl: Option<bool>,
    /// Reduced motion preference.
    pub reduce_motion: Option<bool>,
    /// Screen reader state.
    pub screen_reader: Option<bool>,
    /// Device category: `phone`, `pad`, `desktop` or `tv`.
    pub idiom: Option<String>,
    /// OHOS API level.
    pub api_level: Option<u32>,
}

impl NshEnvironment {
    pub(crate) fn merge(&mut self, changes: Self) {
        macro_rules! merge { ($($field:ident),* $(,)?) => { $(
            if changes.$field.is_some() { self.$field = changes.$field; }
        )* }; }
        merge!(
            safe_top,
            safe_right,
            safe_bottom,
            safe_left,
            keyboard_height,
            keyboard_duration,
            font_scale,
            color_scheme,
            locale,
            rtl,
            reduce_motion,
            screen_reader,
            idiom,
            api_level
        );
    }
    pub(crate) fn snapshot(env: Environment) -> Self {
        Self {
            safe_top: Some(env.safe_area.top.0 as f64),
            safe_right: Some(env.safe_area.right.0 as f64),
            safe_bottom: Some(env.safe_area.bottom.0 as f64),
            safe_left: Some(env.safe_area.left.0 as f64),
            keyboard_height: Some(env.keyboard_height.0 as f64),
            keyboard_duration: Some(env.keyboard_duration as f64),
            font_scale: Some(env.font_scale as f64),
            color_scheme: Some(
                if env.color_scheme == ColorScheme::Dark {
                    "dark"
                } else {
                    "light"
                }
                .into(),
            ),
            locale: Some(env.locale.as_str().to_owned()),
            rtl: Some(env.rtl),
            reduce_motion: Some(env.reduce_motion),
            screen_reader: Some(env.screen_reader),
            idiom: Some(
                match env.idiom {
                    Idiom::Phone => "phone",
                    Idiom::Pad => "pad",
                    Idiom::Desktop => "desktop",
                    Idiom::Tv => "tv",
                    _ => "phone",
                }
                .into(),
            ),
            api_level: Some(env.os_version.0 as u32),
        }
    }
    pub(crate) fn apply(self, mut env: Environment) -> napi_ohos::Result<Environment> {
        fn number(value: Option<f64>, out: &mut f32, min: f64, max: f64) -> napi_ohos::Result<()> {
            if let Some(value) = value {
                if !value.is_finite() || !(min..=max).contains(&value) {
                    return Err(napi_ohos::Error::from_reason(
                        "environment dimension out of range",
                    ));
                }
                *out = value as f32;
            }
            Ok(())
        }
        number(self.safe_top, &mut env.safe_area.top.0, 0.0, 16384.0)?;
        number(self.safe_right, &mut env.safe_area.right.0, 0.0, 16384.0)?;
        number(self.safe_bottom, &mut env.safe_area.bottom.0, 0.0, 16384.0)?;
        number(self.safe_left, &mut env.safe_area.left.0, 0.0, 16384.0)?;
        number(
            self.keyboard_height,
            &mut env.keyboard_height.0,
            0.0,
            16384.0,
        )?;
        number(
            self.keyboard_duration,
            &mut env.keyboard_duration,
            0.0,
            10.0,
        )?;
        number(self.font_scale, &mut env.font_scale, 0.25, 8.0)?;
        if let Some(value) = self.color_scheme {
            env.color_scheme = match value.as_str() {
                "light" => ColorScheme::Light,
                "dark" => ColorScheme::Dark,
                _ => {
                    return Err(napi_ohos::Error::from_reason(
                        "colorScheme must be light or dark",
                    ));
                }
            };
        }
        if let Some(value) = self.locale {
            env.locale = Locale::parse(&value)
                .ok_or_else(|| napi_ohos::Error::from_reason("invalid locale tag"))?;
        }
        if let Some(value) = self.rtl {
            env.rtl = value;
        }
        if let Some(value) = self.reduce_motion {
            env.reduce_motion = value;
        }
        if let Some(value) = self.screen_reader {
            env.screen_reader = value;
        }
        if let Some(value) = self.idiom {
            env.idiom = match value.as_str() {
                "phone" => Idiom::Phone,
                "pad" => Idiom::Pad,
                "desktop" => Idiom::Desktop,
                "tv" => Idiom::Tv,
                _ => return Err(napi_ohos::Error::from_reason("invalid idiom")),
            };
        }
        if let Some(value) = self.api_level {
            env.os_version = (
                u16::try_from(value)
                    .map_err(|_| napi_ohos::Error::from_reason("invalid API level"))?,
                0,
            );
        }
        Ok(env)
    }
}
