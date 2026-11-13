//! `Command::RenderCanvas` (`OffscreenCanvas`'s `convertToBlob`): a drawing
//! replayed as a canvas view's is ([`record`]), into pixmaps of its own on
//! a thread of its own, and encoded as PNG. The images it names load
//! first, on the main thread, through the canvas' loader ([`load_image`]);
//! the image goes to the engine as one root [`EventKind::CanvasImage`],
//! from the main thread's queue ([`crate::rt::invoke`]).
//!
//! The replay is the on-screen one with the view's transform the scale
//! alone (y down, `scale` pixels per point, animated values at 0); its
//! pixel layers are composited bottom to top. Text waits for the system's
//! fonts to be found; glyphs drawn as GPUI glyphs on screen (those without
//! an outline: emoji) are not in the image.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use gpui::WindowTextSystem;
use nsh_core::{
    EventKind, EventPayload, ROOT_ID,
    canvas::{CanvasOp, Drawing, render::ImageFormat},
};
use rustc_hash::FxHashMap;
use tiny_skia as sk;

use super::{IRect, Key, Layer, load_image, record, wait_for_fonts_here};
use crate::surface::Surface;

type Images = FxHashMap<Arc<str>, Option<Arc<sk::Pixmap>>>;

/// The longest a render waits for the system's fonts.
const FONTS_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Renders `drawing` into a `width` × `height` pixel image at `scale`
/// pixels per point and answers request `id`.
pub(crate) fn render(
    surface: &Rc<Surface>,
    id: u32,
    drawing: Arc<Drawing>,
    (width, height): (u32, u32),
    scale: f32,
    format: ImageFormat,
) {
    let key = surface.key();
    // Without the app's (a surface not drawn yet), text draws no glyphs.
    let system =
        surface.text_system.borrow().clone().unwrap_or_else(|| {
            Arc::new(gpui::TextSystem::new(Arc::new(gpui::NoopTextSystem::new())))
        });
    let mut uris: Vec<Arc<str>> = drawing.images().map(|i| i.uri.clone()).collect();
    uris.sort_unstable();
    uris.dedup();
    let job = move |images: Images| {
        let job = Job {
            drawing,
            images,
            system,
            width,
            height,
            scale,
            format,
        };
        let answer = move |result: Result<Vec<u8>, String>| {
            crate::rt::invoke(move || {
                if let Some(surface) = Surface::by_key(key) {
                    surface.event(
                        ROOT_ID,
                        EventKind::CanvasImage,
                        EventPayload::CanvasImage { id, result },
                    );
                }
            });
        };
        let spawned = std::thread::Builder::new()
            .name("nsh-canvas-offscreen".into())
            .spawn(move || answer(job.encode()));
        if let Err(e) = spawned {
            answer(Err(format!(
                "cannot start a thread to render the canvas: {e}"
            )));
        }
    };
    if uris.is_empty() {
        job(Images::default());
        return;
    }
    let waiting = Rc::new(RefCell::new((uris.len(), Images::default(), Some(job))));
    for uri in uris {
        let waiting = waiting.clone();
        let key = uri.clone();
        load_image(surface, &uri, move |result| {
            let mut w = waiting.borrow_mut();
            let image = result
                .inspect_err(|e| log::warn!("offscreen canvas: {e}"))
                .ok()
                .map(Arc::new);
            w.1.insert(key, image);
            w.0 -= 1;
            if w.0 == 0
                && let Some(job) = w.2.take()
            {
                let images = std::mem::take(&mut w.1);
                drop(w);
                job(images);
            }
        });
    }
}

/// What a background thread paints and encodes.
struct Job {
    drawing: Arc<Drawing>,
    images: Images,
    /// The app's text system (fonts are resolved and text shaped in it).
    system: Arc<gpui::TextSystem>,
    width: u32,
    height: u32,
    scale: f32,
    format: ImageFormat,
}

impl Job {
    fn encode(self) -> Result<Vec<u8>, String> {
        let format = self.format;
        let pixmap = self.paint().ok_or("the canvas could not be drawn")?;
        encode(&pixmap, format)
    }

    /// The drawing replayed into a transparent pixmap, y down, `scale`
    /// pixels per point (premultiplied).
    fn paint(self) -> Option<sk::Pixmap> {
        let text =
            |op: &CanvasOp| matches!(op, CanvasOp::FillText { .. } | CanvasOp::StrokeText { .. });
        if self.drawing.ops.iter().any(text) {
            crate::text::init_thread();
            wait_for_fonts_here(FONTS_LIMIT);
        }
        let s = self.scale;
        let key = Key {
            linear: [s, 0.0, 0.0, s],
            scale: s,
            size: (self.width as f32 / s, self.height as f32 / s),
        };
        let region = IRect {
            x0: 0,
            y0: 0,
            x1: i32::try_from(self.width).ok()?,
            y1: i32::try_from(self.height).ok()?,
        };
        let shaping = WindowTextSystem::new(self.system.clone());
        let recorded = record(
            &self.drawing,
            &[],
            &self.images,
            (&shaping, &self.system),
            key,
            region,
            (0.0, 0.0),
        );
        let mut out: Option<sk::Pixmap> = None;
        for layer in recorded.layers {
            let Layer::Pixels(pixmap, _) = layer else {
                continue;
            };
            match out.as_mut() {
                None => out = Some(pixmap),
                Some(below) => below.draw_pixmap(
                    0,
                    0,
                    pixmap.as_ref(),
                    &sk::PixmapPaint::default(),
                    sk::Transform::identity(),
                    None,
                ),
            }
        }
        // Nothing drawn (no operations, or only glyphs): transparent.
        out.or_else(|| sk::Pixmap::new(self.width, self.height))
    }
}

/// `pixmap` (premultiplied) encoded as `format`.
fn encode(pixmap: &sk::Pixmap, format: ImageFormat) -> Result<Vec<u8>, String> {
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let image = image::RgbaImage::from_raw(pixmap.width(), pixmap.height(), rgba)
        .ok_or("the image could not be encoded")?;
    let format = match format {
        ImageFormat::Png => image::ImageFormat::Png,
    };
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, format)
        .map_err(|e| format!("the image could not be encoded: {e}"))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use nsh_core::{
        Color,
        canvas::{Num, Paint},
    };

    use super::*;

    fn job(ops: Vec<CanvasOp>, (width, height): (u32, u32), scale: f32) -> Job {
        Job {
            drawing: Arc::new(Drawing {
                ops,
                ..Drawing::default()
            }),
            images: Images::default(),
            system: Arc::new(gpui::TextSystem::new(Arc::new(gpui::NoopTextSystem::new()))),
            width,
            height,
            scale,
            format: ImageFormat::Png,
        }
    }

    /// The pixel at (x, y), y down, as premultiplied `[r, g, b, a]`.
    fn pixel(pixmap: &sk::Pixmap, x: u32, y: u32) -> [u8; 4] {
        let p = pixmap.pixel(x, y).unwrap();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    fn red_rect() -> Vec<CanvasOp> {
        vec![
            CanvasOp::FillStyle(Paint::Color(Color(0xff00_00ff))),
            CanvasOp::FillRect([
                Num::Const(0.0),
                Num::Const(0.0),
                Num::Const(30.0),
                Num::Const(10.0),
            ]),
        ]
    }

    #[test]
    fn a_drawing_paints_y_down_at_its_scale() {
        let pixmap = job(red_rect(), (60, 40), 2.0).paint().unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (60, 40));
        assert_eq!(pixel(&pixmap, 5, 5), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 59, 19), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 5, 25)[3], 0);
    }

    #[test]
    fn animated_values_are_zero() {
        let ops = vec![
            CanvasOp::FillStyle(Paint::Color(Color(0x0000_ffff))),
            CanvasOp::FillRect([
                Num::Const(0.0),
                Num::Const(0.0),
                Num::Slot(0),
                Num::Const(10.0),
            ]),
        ];
        let pixmap = job(ops, (20, 20), 1.0).paint().unwrap();
        assert_eq!(pixel(&pixmap, 1, 1)[3], 0);
    }

    #[test]
    fn an_empty_drawing_is_transparent() {
        let pixmap = job(Vec::new(), (4, 3), 1.0).paint().unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (4, 3));
        assert!(pixmap.pixels().iter().all(|p| p.alpha() == 0));
    }

    #[test]
    fn an_image_is_encoded_as_png() {
        let png = job(Vec::new(), (3, 2), 1.0).encode().unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[16..24], &[0, 0, 0, 3, 0, 0, 0, 2]);
        // It decodes to the pixels drawn.
        let png = job(red_rect(), (60, 40), 2.0).encode().unwrap();
        let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!(image.dimensions(), (60, 40));
        assert_eq!(image.get_pixel(5, 5).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(5, 25).0[3], 0);
    }
}
