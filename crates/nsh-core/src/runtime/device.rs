//! `nsh/device`'s requests to the system: opening a URL outside the app
//! ([`crate::open_url`]), which needs the `openUrl` capability when the app
//! declares capabilities (else it rejects with `ERR_ACCESS_DENIED`), and
//! opening the app's own page in the system's settings
//! ([`crate::app_settings`]), which needs none.

use crate::element::{JsError, JsObject, Later, System, module};

/// What `openUrl` did.
#[derive(Debug, Clone, Copy, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub(crate) struct OpenUrlResult {
    /// Whether an app opened the URL (`false` when none handles it).
    pub(crate) opened: bool,
}

/// What `openAppSettings` did.
#[derive(Debug, Clone, Copy, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub(crate) struct OpenAppSettingsResult {
    /// Whether the system opened the app's page in its settings.
    pub(crate) opened: bool,
}

/// What the app asks of the system it runs on.
#[module]
pub(crate) struct Device;

#[module(flat)]
impl Device {
    /// Opens `url` outside the app: a web page in the browser, a `mailto:`
    /// link in the mail app, a `tel:` link in the phone app, any scheme
    /// another app registered. The embedding app may take it first and
    /// route it its own way. Resolves with `{ opened }`: whether it opened
    /// (`false` when no app handles it). Rejects with a `TypeError` for a
    /// URL it never opens (not absolute, `javascript:`, `data:`, `file:`,
    /// ...), and with `ERR_ACCESS_DENIED` when the `openUrl` capability
    /// does not grant its scheme.
    pub(crate) fn open_url(url: String, sys: System) -> Result<Later<OpenUrlResult>, JsError> {
        crate::open_url::check_url(&url).map_err(JsError::Type)?;
        if !sys.access.allows_open_url(&url) {
            return Err(crate::open_url::not_granted(&url));
        }
        Ok(Later::new(async move {
            Ok::<_, JsError>(OpenUrlResult {
                opened: crate::prelude::open_url(url).await,
            })
        }))
    }

    /// Opens the app's own page in the system's settings, where the user
    /// turns back on a permission they denied (the camera, photos,
    /// notifications): iOS's Settings app at the app, Android's App info.
    /// Resolves with `{ opened }`: whether the system opened it. Rejects
    /// with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` where the platform has no
    /// such page (the desktops, the web, a terminal;
    /// `appSettingsAvailable()`). Needs no capability.
    pub(crate) async fn open_app_settings() -> Result<OpenAppSettingsResult, JsError> {
        match crate::prelude::open_app_settings().await {
            Ok(opened) => Ok(OpenAppSettingsResult { opened }),
            Err(error) => Err(error.js()),
        }
    }

    /// Resolves with whether `openAppSettings()` opens the app's page in
    /// the system's settings here: `true` on iOS and Android, `false`
    /// elsewhere, where it rejects with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`.
    /// Ask it before showing an "Open Settings" button.
    pub(crate) async fn app_settings_available() -> bool {
        crate::prelude::app_settings_available().await
    }
}
