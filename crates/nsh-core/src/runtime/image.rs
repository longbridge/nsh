//! `nsh/image`: image information and loading ahead, the functions of
//! `Image` (`nsh-js` makes it the image element's component).

use crate::{
    JsonValue,
    ui::{ImageSize, JsValue, PropError, TsTypes, module},
};

/// Image information and loading ahead.
pub(crate) struct Image;

/// An image's URI: a string or `{ uri }`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageUri(String);

impl JsValue for ImageUri {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string | { uri: string }".into()
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        match &value {
            JsonValue::String(uri) => Ok(Self(uri.clone())),
            _ => value
                .get("uri")
                .and_then(JsonValue::as_str)
                .map(|uri| Self(uri.to_owned()))
                .ok_or_else(|| PropError::expected("a URI or { uri }", &value)),
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::String(self.0.clone())
    }
}

/// Image information and loading ahead.
#[module]
impl Image {
    /// The image's size in pixels; its bytes stay loaded for the views that
    /// show it next.
    pub(crate) async fn get_size(source: ImageUri) -> Result<ImageSize, String> {
        crate::ui::prelude::image_size(source.0).await
    }

    /// Loads the image ahead of showing it: true once its bytes are in
    /// memory.
    pub(crate) async fn prefetch(source: ImageUri) -> bool {
        crate::ui::prelude::prefetch_image(source.0).await
    }
}
