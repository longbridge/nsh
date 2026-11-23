//! `nsh`'s `Location`: the device's location, and whether the app may read
//! it ([`crate::location`]).
//!
//! `available()` is whether the platform reports one, or the embedding app
//! gave the core its own provider. `authorization()` is what the app may read
//! now, as the system says.

use crate::{
    element::{JsError, Later, module},
    location::Authorization,
};

/// The device's location.
#[module]
pub(crate) struct Location;

#[module]
impl Location {
    /// Resolves with whether the device's location can be read here: the
    /// platform reports one, or the embedding app gave its own provider;
    /// `false` on the desktops NSH has no provider for, the terminal and the
    /// GPUI backend.
    pub(crate) fn available() -> Later<bool> {
        Later::new(async { Ok::<_, JsError>(crate::prelude::location_available().await) })
    }

    /// Resolves with what the app may read now, as the system says:
    /// `"notDetermined"`, `"whenInUse"`, `"always"`, `"denied"` or
    /// `"restricted"`. Asking the user is `requestAuthorization`, and every
    /// call that reads a position asks for when-in-use itself when the user
    /// has not been asked.
    pub(crate) fn authorization() -> Later<String> {
        Later::new(async {
            let authorization: Authorization = crate::prelude::location_authorization().await;
            Ok::<_, JsError>(authorization.as_str().to_owned())
        })
    }
}
