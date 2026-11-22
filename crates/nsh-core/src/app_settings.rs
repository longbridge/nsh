//! Opening the app's own page in the system's settings (`openAppSettings`
//! of `nsh/device`, [`crate::element::open_app_settings`]): where the user
//! turns back on a permission they denied (the camera, photos,
//! notifications), which only the system's settings change.
//!
//! The core sends [`crate::Command::OpenAppSettings`] on the root to a
//! platform whose [`crate::platform::Environment::app_settings`] is set
//! (iOS, Android), which reports whether the system opened the page as
//! [`crate::EventKind::OpenAppSettings`]. Elsewhere the call fails at once
//! with [`AppSettingsError::Unavailable`]: the desktops have no page of an
//! app's own (macOS's privacy panes are per permission, not per app), and
//! the web and terminals have no system settings at all. It needs no
//! capability: like `quit`, it is about the app itself, and the user
//! decides there.

/// Why [`crate::element::open_app_settings`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppSettingsError {
    /// The platform has no settings page of the app's own (TS:
    /// `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`): only iOS and Android have
    /// one ([`crate::element::app_settings_available`]).
    Unavailable,
}

impl std::fmt::Display for AppSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => f.write_str(
                "openAppSettings: this platform has no settings page of the app's own (iOS and \
                 Android have one)",
            ),
        }
    }
}

impl std::error::Error for AppSettingsError {}

impl AppSettingsError {
    /// The error JS gets: an `Error` with the code to branch on.
    pub(crate) fn js(self) -> crate::element::JsError {
        match self {
            Self::Unavailable => crate::element::JsError::Error(self.to_string())
                .with_code(crate::element::codes::UNAVAILABLE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_is_the_platform_code() {
        let error = AppSettingsError::Unavailable.js();
        assert_eq!(error.code(), Some("ERR_FEATURE_UNAVAILABLE_ON_PLATFORM"));
        assert!(error.message().contains("openAppSettings"), "{error:?}");
    }
}
