//! `Command::RenderCanvas` (`OffscreenCanvas`'s `convertToBlob`): a drawing
//! replayed as a canvas view's is ([`record`] into a command list), drawn
//! into a Direct2D bitmap of its own and encoded as PNG with WIC.
//!
//! The images the drawing names load first through the shared loader
//! ([`crate::image::load_bitmap`]). The replay and the drawing run on the
//! UI thread: recording uses the thread's devices ([`gfx::get`], a
//! single-threaded factory), its pool of recording contexts and the
//! images' bitmaps, which are all kept per UI thread, so the image is the
//! very replay a canvas view plays on screen. The pixels are read back
//! there; turning them into straight alpha and encoding them, the part
//! that grows with the image, runs on a thread of its own, and the image
//! goes back to the engine on the UI thread as one root
//! [`EventKind::CanvasImage`].

use std::{cell::RefCell, rc::Rc, sync::Arc};

use nsh_core::{
    Color, EventKind, EventPayload, ROOT_ID,
    canvas::{Drawing, render::ImageFormat},
};
use rustc_hash::FxHashMap;
use windows::{
    Win32::{
        Foundation::{D2DERR_RECREATE_TARGET, E_FAIL, E_OUTOFMEMORY},
        Graphics::{
            Direct2D::{
                Common::{
                    D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COMPOSITE_MODE_SOURCE_OVER,
                    D2D1_PIXEL_FORMAT,
                },
                D2D1_BITMAP_OPTIONS, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_CPU_READ,
                D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
                D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_INTERPOLATION_MODE_LINEAR,
                D2D1_MAP_OPTIONS_READ, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
            },
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Imaging::{
                CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
                IWICImagingFactory, WICBitmapEncoderNoCache,
            },
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, STREAM_SEEK_END, STREAM_SEEK_SET, StructuredStorage::IPropertyBag2,
        },
        UI::Shell::SHCreateMemStream,
    },
    core::Error,
};

use super::{TextCache, record};
use crate::{gfx, image::Frame, render::color_f, surface::Surface};

type Images = FxHashMap<Arc<str>, Option<Arc<Frame>>>;

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
    let ui = surface.ui_thread();
    let mut uris: Vec<Arc<str>> = drawing.images().map(|i| i.uri.clone()).collect();
    uris.sort_unstable();
    uris.dedup();
    let job = move |images: Images| {
        let job = Job {
            drawing,
            images,
            width,
            height,
            scale,
        };
        let answer = move |result: Result<Vec<u8>, String>| {
            ui.invoke(move || {
                if let Some(surface) = Surface::by_key(key) {
                    surface.event(
                        ROOT_ID,
                        EventKind::CanvasImage,
                        EventPayload::CanvasImage {
                            id,
                            result: result.map_err(Into::into),
                        },
                    );
                }
            });
        };
        match job.paint() {
            Ok(pixels) => encode_off_thread(pixels, (width, height), format, answer),
            Err(e) => answer(Err(e)),
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
        // A cached image is handed over before `load_bitmap` returns: the
        // loop holds no borrow of `waiting`.
        crate::image::load_bitmap(surface, &uri, move |result| {
            let mut w = waiting.borrow_mut();
            let image = result
                .inspect_err(|e| log::warn!("offscreen canvas: {e}"))
                .ok();
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

/// Encodes premultiplied BGRA `pixels` as `format` on a thread of its own
/// and hands the result to `answer` there.
fn encode_off_thread(
    pixels: Vec<u8>,
    (width, height): (u32, u32),
    format: ImageFormat,
    answer: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static,
) {
    let work = move || encode(pixels, (width, height), format);
    // The closure moves into the thread only when it starts: keep a way
    // to run it here if no thread can be made.
    let slot = Arc::new(std::sync::Mutex::new(Some((work, answer))));
    let theirs = slot.clone();
    let spawned = std::thread::Builder::new()
        .name("nsh-canvas-encode".into())
        .spawn(move || {
            let taken = theirs.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some((work, answer)) = taken {
                answer(work());
            }
        });
    if let Err(e) = spawned {
        log::debug!("offscreen canvas: encoding on the UI thread: {e}");
        let taken = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some((work, answer)) = taken {
            answer(work());
        }
    }
}

/// `pixels` (premultiplied BGRA, rows packed) encoded as `format`.
fn encode(
    mut pixels: Vec<u8>,
    (width, height): (u32, u32),
    format: ImageFormat,
) -> Result<Vec<u8>, String> {
    match format {
        ImageFormat::Png => {
            unpremultiply(&mut pixels);
            // SAFETY: initializes COM on this thread for WIC; balanced by
            // `CoUninitialize` below when it succeeded (S_OK or S_FALSE).
            let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            let png = encode_png(&pixels, width, height)
                .map_err(|e| format!("the image could not be encoded: {}", e.message()));
            if com {
                // SAFETY: every COM object `encode_png` made is released.
                unsafe { CoUninitialize() };
            }
            png
        }
        _ => Err(format!("{} images are not encoded here", format.mime())),
    }
}

/// Premultiplied BGRA to straight BGRA (what PNG holds), in place.
fn unpremultiply(pixels: &mut [u8]) {
    for p in pixels.chunks_exact_mut(4) {
        let a = u32::from(p[3]);
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut p[..3] {
            *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

/// Straight BGRA `pixels` encoded as PNG with WIC (COM initialized on the
/// calling thread).
fn encode_png(pixels: &[u8], width: u32, height: u32) -> windows::core::Result<Vec<u8>> {
    // SAFETY: COM calls on objects made here; `pixels` holds `height`
    // rows of `width * 4` bytes, and `out` is `size` bytes long.
    unsafe {
        let wic: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = SHCreateMemStream(None).ok_or_else(|| Error::from_hresult(E_OUTOFMEMORY))?;
        let encoder = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        encoder.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
        let frame = frame.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        frame.Initialize(None::<&IPropertyBag2>)?;
        frame.SetSize(width, height)?;
        let mut pixel_format = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut pixel_format)?;
        // PNG's encoder takes 32bpp BGRA as is.
        if pixel_format != GUID_WICPixelFormat32bppBGRA {
            return Err(Error::from_hresult(E_FAIL));
        }
        frame.WritePixels(height, width * 4, pixels)?;
        frame.Commit()?;
        encoder.Commit()?;
        let mut size = 0u64;
        stream.Seek(0, STREAM_SEEK_END, Some(&raw mut size))?;
        stream.Seek(0, STREAM_SEEK_SET, None)?;
        let len = u32::try_from(size).map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        let mut out = vec![0u8; len as usize];
        let mut read = 0u32;
        stream
            .Read(out.as_mut_ptr().cast(), len, Some(&raw mut read))
            .ok()?;
        out.truncate(read as usize);
        Ok(out)
    }
}

/// What the UI thread replays.
struct Job {
    drawing: Arc<Drawing>,
    images: Images,
    width: u32,
    height: u32,
    scale: f32,
}

impl Job {
    /// The drawing replayed into a transparent bitmap, y down, `scale`
    /// pixels per point, its animated values at 0: premultiplied BGRA,
    /// rows packed.
    fn paint(&self) -> Result<Vec<u8>, String> {
        let fail = |e: Error| format!("the canvas could not be drawn: {}", e.message());
        let gfx = gfx::get().map_err(fail)?;
        let mut text = TextCache::default();
        let list =
            record(&gfx, &self.drawing.ops, 0, &[], &self.images, &mut text).map_err(fail)?;
        let (w, h) = (self.width, self.height);
        let dpi = 96.0 * self.scale;
        let props = |options: D2D1_BITMAP_OPTIONS| D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi,
            dpiY: dpi,
            bitmapOptions: options,
            colorContext: std::mem::ManuallyDrop::new(None),
        };
        let size = D2D_SIZE_U {
            width: w,
            height: h,
        };
        // SAFETY: COM calls on live objects of this thread's device; the
        // context is our own, so no frame is being drawn with it, and the
        // mapped memory (`height` rows `pitch` apart) is copied before
        // `Unmap`.
        unsafe {
            // A context of its own: the shared one may hold a target.
            let dc = gfx
                .device
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)
                .map_err(fail)?;
            let max = dc.GetMaximumBitmapSize();
            if w > max || h > max {
                return Err(format!(
                    "a {w}×{h} pixel image is larger than this device draws ({max} pixels a side)"
                ));
            }
            let target = dc
                .CreateBitmap(size, None, 0, &props(D2D1_BITMAP_OPTIONS_TARGET))
                .map_err(fail)?;
            dc.SetTarget(&target);
            // The drawing is in points: one point is `scale` pixels.
            dc.SetDpi(dpi, dpi);
            dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            dc.BeginDraw();
            dc.Clear(Some(&color_f(Color::TRANSPARENT)));
            dc.DrawImage(
                &list,
                None,
                None,
                D2D1_INTERPOLATION_MODE_LINEAR,
                D2D1_COMPOSITE_MODE_SOURCE_OVER,
            );
            let drawn = dc.EndDraw(None, None);
            dc.SetTarget(None);
            if let Err(e) = drawn {
                if e.code() == D2DERR_RECREATE_TARGET {
                    gfx::lost();
                }
                return Err(fail(e));
            }
            let read = dc
                .CreateBitmap(
                    size,
                    None,
                    0,
                    &props(D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW),
                )
                .map_err(fail)?;
            read.CopyFromBitmap(None, &target, None).map_err(fail)?;
            let mapped = read.Map(D2D1_MAP_OPTIONS_READ).map_err(fail)?;
            let row = w as usize * 4;
            let mut data = Vec::with_capacity(row * h as usize);
            for y in 0..h as usize {
                let start = mapped.bits.add(y * mapped.pitch as usize);
                data.extend_from_slice(std::slice::from_raw_parts(start, row));
            }
            let _ = read.Unmap();
            Ok(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::canvas::{CanvasOp, Num, Paint};

    use super::*;

    fn job(ops: Vec<CanvasOp>, (width, height): (u32, u32), scale: f32) -> Job {
        Job {
            drawing: Arc::new(Drawing {
                ops,
                ..Drawing::default()
            }),
            images: Images::default(),
            width,
            height,
            scale,
        }
    }

    /// The pixel at (x, y), y down, as `[b, g, r, a]` (premultiplied).
    fn pixel(data: &[u8], width: u32, x: usize, y: usize) -> [u8; 4] {
        let at = (y * width as usize + x) * 4;
        [data[at], data[at + 1], data[at + 2], data[at + 3]]
    }

    #[test]
    fn a_drawing_paints_y_down_at_its_scale() {
        if gfx::get().is_err() {
            // No Direct3D device, not even WARP.
            return;
        }
        let red = Paint::Color(Color(0xff00_00ff));
        let ops = vec![
            CanvasOp::FillStyle(red),
            CanvasOp::FillRect([
                Num::Const(0.0),
                Num::Const(0.0),
                Num::Const(30.0),
                Num::Const(10.0),
            ]),
        ];
        let data = job(ops, (60, 40), 2.0).paint().unwrap();
        assert_eq!(data.len(), 60 * 40 * 4);
        assert_eq!(pixel(&data, 60, 5, 5), [0, 0, 255, 255]);
        assert_eq!(pixel(&data, 60, 59, 19), [0, 0, 255, 255]);
        assert_eq!(pixel(&data, 60, 5, 25)[3], 0);
    }

    #[test]
    fn animated_values_paint_at_zero() {
        if gfx::get().is_err() {
            return;
        }
        let red = Paint::Color(Color(0xff00_00ff));
        // Slot 0 is the rectangle's x: unset, it is 0.
        let ops = vec![
            CanvasOp::FillStyle(red),
            CanvasOp::FillRect([
                Num::Slot(0),
                Num::Const(0.0),
                Num::Const(10.0),
                Num::Const(10.0),
            ]),
        ];
        let data = job(ops, (20, 10), 1.0).paint().unwrap();
        assert_eq!(pixel(&data, 20, 2, 2), [0, 0, 255, 255]);
        assert_eq!(pixel(&data, 20, 15, 2)[3], 0);
    }

    #[test]
    fn an_image_is_encoded_as_png() {
        let png = encode(vec![0; 3 * 2 * 4], (3, 2), ImageFormat::Png).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        // IHDR: the width and height, big-endian.
        assert_eq!(&png[16..24], &[0, 0, 0, 3, 0, 0, 0, 2]);
    }

    #[test]
    fn a_drawing_is_encoded_as_png() {
        if gfx::get().is_err() {
            return;
        }
        let ops = vec![
            CanvasOp::FillStyle(Paint::Color(Color(0x00ff_00ff))),
            CanvasOp::FillRect([
                Num::Const(0.0),
                Num::Const(0.0),
                Num::Const(4.0),
                Num::Const(4.0),
            ]),
        ];
        let pixels = job(ops, (8, 6), 2.0).paint().unwrap();
        let png = encode(pixels, (8, 6), ImageFormat::Png).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[16..24], &[0, 0, 0, 8, 0, 0, 0, 6]);
    }

    #[test]
    fn premultiplied_pixels_become_straight() {
        // Half-transparent red, premultiplied; opaque and clear pixels
        // stay as they are.
        let mut pixels = vec![0, 0, 128, 128, 10, 20, 30, 255, 0, 0, 0, 0];
        unpremultiply(&mut pixels);
        assert_eq!(pixels, [0, 0, 255, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
    }
}
