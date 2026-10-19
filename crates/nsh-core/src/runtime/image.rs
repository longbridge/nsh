//! `nsh/image`: image information and loading ahead, the functions of
//! `Image` (`nsh-js` makes it the image element's component).

use crate::{
    ImageScales, ImageSource,
    geometry::DevicePixels,
    ui::{ImageSize, JsHost, JsIn, JsKind, JsOwned, JsValue, PropError, TsTypes, module},
};

/// Image information and loading ahead.
#[module]
pub(crate) struct Image;

/// An image's URI: a string or `{ uri }` (an imported image, with the
/// `scales` of its variants).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageUri {
    uri: String,
    scales: ImageScales,
}

impl ImageUri {
    /// The URI the views load at the surface's pixel ratio, and the scale
    /// of that variant (1 for the URI itself).
    fn pick(self) -> (String, f32) {
        let source = ImageSource {
            uri: self.uri.into(),
            size: None,
            scales: self.scales,
        };
        let ratio = crate::ui::prelude::pixel_ratio();
        let scale = source.variant(ratio).unwrap_or(1.0);
        (source.uri_for(ratio).to_string(), scale)
    }
}

impl JsValue for ImageUri {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string | { uri: string }".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if let Some(uri) = value.as_string() {
            return Ok(Self {
                uri,
                scales: ImageScales::NONE,
            });
        }
        if value.kind() == JsKind::Object
            && let Some(uri) = value.field("uri")?.get().as_string()
        {
            let list = value.field("scales")?;
            let list = list.get();
            let mut scales = ImageScales::NONE;
            let len = list.array_len()?.unwrap_or(0);
            for i in 0..len.min(crate::ops::SCALES_READ) {
                if let Some(n) = list.index(i)?.get().as_number() {
                    scales = scales.with(n as f32);
                }
            }
            return Ok(Self { uri, scales });
        }
        Err(value.expected("a URI or { uri }"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Ok(JsOwned::new(host, host.new_string(&self.uri)?))
    }
}

#[module]
impl Image {
    /// The image's size in pixels; its bytes stay loaded for the views that
    /// show it next. An imported image with variants loads the variant the
    /// views load and resolves with the imported file's size: the
    /// variant's pixels divided by its scale.
    pub(crate) async fn get_size(source: ImageUri) -> Result<ImageSize, String> {
        let (uri, scale) = source.pick();
        let size = crate::ui::prelude::image_size(uri).await?;
        let unscaled = |px: DevicePixels| DevicePixels((px.0 as f32 / scale).round() as i32);
        Ok(ImageSize {
            width: unscaled(size.width),
            height: unscaled(size.height),
        })
    }

    /// Loads the image ahead of showing it: true once its bytes are in
    /// memory. An imported image with variants loads the variant the views
    /// load at the screen's pixel ratio.
    pub(crate) async fn prefetch(source: ImageUri) -> bool {
        crate::ui::prelude::prefetch_image(source.pick().0).await
    }
}
