//! Fonts the app registers while it runs (`registerFont`): one it
//! downloads or makes, where an imported font is registered before the
//! first frame.

use crate::element::{JsError, JsHost, JsIn, JsOwned, JsValue, Later, PropError, TsTypes, module};

/// Fonts registered while the app runs.
#[module]
pub(crate) struct Fonts;

/// Where a font comes from: its data, or the URL of a file of it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FontSource {
    /// TrueType, OpenType, WOFF or WOFF2 data.
    Data(Vec<u8>),
    /// An `asset:/` file of the app's, or a file on the network.
    Url(String),
}

impl JsValue for FontSource {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string | Uint8Array".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if let Some(url) = value.as_string() {
            return Ok(Self::Url(url));
        }
        value
            .as_bytes()
            .map(Self::Data)
            .ok_or_else(|| value.expected("a URL or a Uint8Array"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self {
            Self::Data(data) => data.to_js(host),
            Self::Url(url) => url.to_js(host),
        }
    }
}

#[module(flat)]
impl Fonts {
    /// Registers a font while the app runs and resolves to its family name,
    /// for `fontFamily`: a font the app downloaded or made, as TrueType,
    /// OpenType, WOFF or WOFF2 data, or the URL of one (a file of the app's,
    /// `asset:/...`, or one on the network, read as `fetch` reads it: the
    /// `network` capability applies). Text that names the family is laid out
    /// with it from then on, text shown before again. Rejects with an
    /// `Error` when the URL does not load or the data is no font.
    pub(crate) fn register_font(source: FontSource) -> Result<Later<String>, JsError> {
        Ok(Later::new(async move {
            let data = match source {
                FontSource::Data(data) => data,
                FontSource::Url(url) => {
                    let response = crate::prelude::fetch(url.as_str())
                        .await
                        .map_err(|e| JsError::Error(format!("registerFont: {url}: {e}")))?;
                    if !response.ok() {
                        return Err(JsError::Error(format!(
                            "registerFont: {url}: status {}",
                            response.status()
                        )));
                    }
                    response.body
                }
            };
            crate::prelude::register_font(data)
                .await
                .map_err(|e| JsError::Error(format!("registerFont: {e}")))
        }))
    }
}
