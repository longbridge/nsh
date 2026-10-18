//! `nsh/image`: image information and loading ahead, the functions of
//! `Image` (`nsh-js` makes it the image element's component).

use crate::ui::{ImageSize, JsHost, JsIn, JsKind, JsOwned, JsValue, PropError, TsTypes, module};

/// Image information and loading ahead.
pub(crate) struct Image;

/// An image's URI: a string or `{ uri }`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageUri(String);

impl JsValue for ImageUri {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string | { uri: string }".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if let Some(uri) = value.as_string() {
            return Ok(Self(uri));
        }
        if value.kind() == JsKind::Object
            && let Some(uri) = value.field("uri")?.get().as_string()
        {
            return Ok(Self(uri));
        }
        Err(value.expected("a URI or { uri }"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Ok(JsOwned::new(host, host.new_string(&self.0)?))
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
