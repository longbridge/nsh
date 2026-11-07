//! Ordered image filters over an exported native subtree, on the compositor worker.
use nsh_core::Filter as Effect;
use ohos_drawing_binding::{
    Bitmap, BitmapFormat, Brush, Canvas, ColorFilter, Filter, ImageFilter, TileMode,
};

pub(crate) struct Image {
    pub(crate) bitmap: Bitmap,
    pub(crate) x: f32,
    pub(crate) y: f32,
}
impl Image {
    pub(crate) fn new(frame: super::readback::RgbaFrame) -> Self {
        let mut bitmap = Bitmap::new(frame.width, frame.height, BitmapFormat::default());
        bitmap.pixels_mut().copy_from_slice(&frame.pixels);
        Self {
            bitmap,
            x: 0.0,
            y: 0.0,
        }
    }
    pub(crate) fn filtered(&self, effects: &[Effect], scale: f32) -> Result<Self, &'static str> {
        self.filter_chain(effects, scale, TileMode::Decal)
    }
    pub(crate) fn backdrop(&self, effects: &[Effect], scale: f32) -> Result<Self, &'static str> {
        self.filter_chain(effects, scale, TileMode::Mirror)
    }
    fn filter_chain(
        &self,
        effects: &[Effect],
        scale: f32,
        tile: TileMode,
    ) -> Result<Self, &'static str> {
        let mut image = Self::copy(&self.bitmap, self.x, self.y);
        for effect in effects {
            image = image.apply(*effect, scale, tile)?;
        }
        Ok(image)
    }
    pub(crate) fn copy(bitmap: &Bitmap, x: f32, y: f32) -> Self {
        let mut copy = Bitmap::new(bitmap.width(), bitmap.height(), BitmapFormat::default());
        copy.pixels_mut().copy_from_slice(bitmap.pixels());
        Self { bitmap: copy, x, y }
    }
    fn apply(self, effect: Effect, scale: f32, tile: TileMode) -> Result<Self, &'static str> {
        let (mut left, mut top, mut right, mut bottom) = (
            self.x,
            self.y,
            self.x + self.bitmap.width() as f32,
            self.y + self.bitmap.height() as f32,
        );
        let (dx, dy, sigma, shadow) = match effect {
            Effect::DropShadow {
                offset_x,
                offset_y,
                blur,
                color,
            } => (
                offset_x.0 * scale,
                offset_y.0 * scale,
                super::geometry::blur_sigma(blur.0, scale),
                Some(color),
            ),
            Effect::Blur(radius) => (0.0, 0.0, super::geometry::blur_sigma(radius.0, scale), None),
            _ => (0.0, 0.0, 0.0, None),
        };
        let pad = (sigma * 3.0).ceil();
        if shadow.is_some() {
            left = left.min(left + dx - pad).floor();
            top = top.min(top + dy - pad).floor();
            right = right.max(right + dx + pad).ceil();
            bottom = bottom.max(bottom + dy + pad).ceil();
        } else if sigma > 0.0 {
            left -= pad;
            top -= pad;
            right += pad;
            bottom += pad;
        }
        let width = (right - left).ceil().max(1.0) as u32;
        let height = (bottom - top).ceil().max(1.0) as u32;
        if ![left, top, right, bottom].iter().all(|v| v.is_finite())
            || u64::from(width) * u64::from(height) > 16_777_216
        {
            return Err("composite filter exceeds 16 megapixels");
        }
        let canvas = Canvas::with_bitmap(Bitmap::new(width, height, BitmapFormat::default()));
        canvas.clear(0);
        let mut filter = Filter::new();
        let blur = ImageFilter::blur(sigma, sigma, tile);
        filter.set_image_filter(blur.as_ref());
        let color = if let Some(color) = shadow {
            ColorFilter::blend(
                crate::text::typography::argb(color),
                ohos_drawing_binding::BlendMode::SrcIn,
            )
        } else {
            effect.color_matrix().and_then(ColorFilter::matrix)
        };
        filter.set_color_filter(color.as_ref());
        let mut brush = Brush::new();
        brush.set_filter(Some(&filter));
        canvas.attach_brush(&brush);
        canvas.draw_bitmap(&self.bitmap, self.x - left + dx, self.y - top + dy);
        canvas.detach_brush();
        if shadow.is_some() {
            canvas.draw_bitmap(&self.bitmap, self.x - left, self.y - top);
        }
        let bitmap = canvas.bitmap().ok_or("composite raster unavailable")?;
        Ok(Self::copy(bitmap, left, top))
    }
}
