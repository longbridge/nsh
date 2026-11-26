//! `nsh`'s `openAuthSession` and `closeAuthSession`: web sign-in sessions
//! ([`crate::auth_session`]), as Rust's
//! [`open_auth_session`](crate::element::open_auth_session) and
//! [`close_auth_session`](crate::element::close_auth_session) run them.
//! Needs the `openUrl` capability for the sign-in page when the app
//! declares capabilities (else it rejects with `ERR_ACCESS_DENIED`).

use crate::{
    auth_session::{AuthSessionError, AuthSessionOutcome, AuthSessionRequest},
    element::{JsError, JsObject, Later, codes, module},
};

/// Options of `openAuthSession`.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[prop(exact)]
pub(crate) struct AuthSessionOptions {
    /// The scheme of the callback URL, without `:` (`"myapp"` for
    /// `myapp://signed-in?code=...`). Outside iOS it must be one the app
    /// claims (`nsh.json` `schemes`).
    callback_scheme: String,
    /// Asks the browser not to share cookies or other data with the user's
    /// browsing (iOS's `prefersEphemeralWebBrowserSession`). Off by default.
    ephemeral: Option<bool>,
}

/// Web sign-in sessions.
#[module]
pub(crate) struct AuthSession;

#[module(flat)]
impl AuthSession {
    /// Opens the sign-in page `url` (an `https:` OAuth authorization URL)
    /// and waits for it to redirect to a URL of `options.callbackScheme`:
    /// iOS shows it in an `ASWebAuthenticationSession` sheet, Android in a
    /// Custom Tab, macOS, Windows and Linux in the system browser. Resolves
    /// with the callback URL, or `null` when the user closed the page,
    /// came back without signing in, or a later session or
    /// `closeAuthSession` ended it. One session runs at a time. Rejects
    /// with a `TypeError` for a URL or scheme it never opens,
    /// `ERR_ACCESS_DENIED` when the `openUrl` capability does not grant the
    /// page's URL, and `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` where nothing
    /// brings the callback back (the web, a scheme the app does not claim
    /// outside iOS).
    pub(crate) fn open_auth_session(
        url: String,
        options: AuthSessionOptions,
    ) -> Result<Later<Option<String>>, JsError> {
        let request = AuthSessionRequest::new(url, options.callback_scheme)
            .ephemeral(options.ephemeral.unwrap_or(false));
        request.check().map_err(JsError::Type)?;
        Ok(Later::new(async move {
            match crate::prelude::open_auth_session(request).await {
                Ok(AuthSessionOutcome::Completed(url)) => Ok(Some(url)),
                Ok(_) => Ok(None),
                Err(error) => Err(js_error(error)),
            }
        }))
    }

    /// Closes the app's sign-in session (`openAuthSession`) while it still
    /// waits, as when the user signed in some other way: it resolves with
    /// `null`, and iOS and Android take the page off the screen. A page in
    /// the system browser elsewhere stays open. With none waiting, nothing
    /// happens. Resolves once it is closed.
    pub(crate) async fn close_auth_session() -> Result<(), JsError> {
        crate::prelude::close_auth_session();
        Ok(())
    }
}

/// A failed session as JS gets it: an `Error` with the failure's `code`.
fn js_error(error: AuthSessionError) -> JsError {
    match error.code.as_deref() {
        Some(codes::ACCESS_DENIED) => JsError::access_denied(error.message, "openUrl", ""),
        Some(code) => JsError::Error(error.message.clone()).with_code(code),
        None => JsError::Error(error.message),
    }
}
