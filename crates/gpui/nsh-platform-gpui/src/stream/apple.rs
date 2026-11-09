//! Frames from `CVPixelBuffer`s (macOS): full-range NV12 goes to GPUI as
//! it is, painted by its surface shader (`Window::paint_surface`), which
//! samples the buffer's planes as Metal textures, no copy; GPUI's shader
//! takes only that format (its renderer fails a frame with another), so
//! video-range NV12 and BGRA buffers are copied into an image instead.

use objc2_core_foundation::{CFRetained, Type};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBaseAddressOfPlane,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetHeight,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, kCVReturnSuccess,
};

use super::{Frame, FrameContent};

/// A retained `CVPixelBuffer` a frame paints, shared by the threads that
/// push and paint it.
#[derive(Clone)]
pub(crate) struct PixelBuffer(CFRetained<CVPixelBuffer>);

// SAFETY: CoreVideo's retain and release are thread safe, and a frame only
// reads the buffer (its size; GPUI's renderer its planes): the host hands
// it over finished, as `AVCaptureVideoDataOutput` and VideoToolbox hand
// buffers across threads.
unsafe impl Send for PixelBuffer {}
// SAFETY: as above; nothing mutates the buffer through a shared reference.
unsafe impl Sync for PixelBuffer {}

impl PixelBuffer {
    /// The buffer, for `Window::paint_surface`.
    pub(crate) fn retained(&self) -> CFRetained<CVPixelBuffer> {
        self.0.clone()
    }
}

/// The frame `buffer` shows, or why it shows nothing.
pub(super) fn frame_from_pixel_buffer(buffer: &CVPixelBuffer) -> Result<Frame, &'static str> {
    let (width, height) = (
        CVPixelBufferGetWidth(buffer),
        CVPixelBufferGetHeight(buffer),
    );
    if width == 0 || height == 0 {
        return Err("it has no pixels");
    }
    let too_large = "it is too large";
    let (w, h) = (
        u32::try_from(width).map_err(|_| too_large)?,
        u32::try_from(height).map_err(|_| too_large)?,
    );
    #[allow(non_upper_case_globals)]
    match CVPixelBufferGetPixelFormatType(buffer) {
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange => {
            if CVPixelBufferGetPlaneCount(buffer) != 2 {
                return Err("its NV12 data is not two planes");
            }
            Ok(Frame {
                width: w,
                height: h,
                content: FrameContent::Surface(PixelBuffer(buffer.retain())),
            })
        }
        kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange => {
            let lock = Lock::new(buffer)?;
            let (y, y_stride) = lock.plane(0).ok_or("its luma plane cannot be read")?;
            let (uv, uv_stride) = lock.plane(1).ok_or("its chroma plane cannot be read")?;
            let pixels = nv12_to_bgra(width, height, y, y_stride, uv, uv_stride, false)
                .ok_or("its planes are smaller than its size")?;
            super::image_frame(w, h, pixels).ok_or(too_large)
        }
        kCVPixelFormatType_32BGRA => {
            let lock = Lock::new(buffer)?;
            let (bytes, stride) = lock.bytes().ok_or("its pixels cannot be read")?;
            super::frame_from_bgra(width, height, stride, bytes)
                .ok_or("its pixels are fewer than its size")
        }
        _ => Err("its pixel format is not 420f, 420v or 32BGRA"),
    }
}

/// The buffer's base address locked for reading, unlocked on drop.
struct Lock<'a>(&'a CVPixelBuffer);

impl<'a> Lock<'a> {
    fn new(buffer: &'a CVPixelBuffer) -> Result<Self, &'static str> {
        // SAFETY: a valid buffer, locked read-only; `Drop` unlocks it with
        // the same flags.
        let status =
            unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) };
        if status == kCVReturnSuccess {
            Ok(Lock(buffer))
        } else {
            Err("it cannot be locked for reading")
        }
    }

    /// The bytes of a non-planar buffer and its bytes per row.
    fn bytes(&self) -> Option<(&[u8], usize)> {
        let base = CVPixelBufferGetBaseAddress(self.0);
        let stride = CVPixelBufferGetBytesPerRow(self.0);
        let len = stride.checked_mul(CVPixelBufferGetHeight(self.0))?;
        if base.is_null() {
            return None;
        }
        // SAFETY: while locked, the base address holds `bytes per row ×
        // height` readable bytes (CoreVideo's layout of a non-planar
        // buffer), alive as long as the lock borrows the buffer.
        Some((
            unsafe { std::slice::from_raw_parts(base.cast::<u8>(), len) },
            stride,
        ))
    }

    /// The bytes of plane `index` and its bytes per row.
    fn plane(&self, index: usize) -> Option<(&[u8], usize)> {
        let base = CVPixelBufferGetBaseAddressOfPlane(self.0, index);
        let stride = CVPixelBufferGetBytesPerRowOfPlane(self.0, index);
        let len = stride.checked_mul(CVPixelBufferGetHeightOfPlane(self.0, index))?;
        if base.is_null() {
            return None;
        }
        // SAFETY: while locked, a plane's base address holds its `bytes per
        // row × height` readable bytes, alive as long as the lock borrows
        // the buffer.
        Some((
            unsafe { std::slice::from_raw_parts(base.cast::<u8>(), len) },
            stride,
        ))
    }
}

impl Drop for Lock<'_> {
    fn drop(&mut self) {
        // SAFETY: locked read-only in `Lock::new`.
        unsafe { CVPixelBufferUnlockBaseAddress(self.0, CVPixelBufferLockFlags::ReadOnly) };
    }
}

/// `width` × `height` pixels of NV12 (a luma plane, and a plane of
/// interleaved Cb Cr at half the size in each direction) as opaque BGRA,
/// rows packed, with BT.601's matrix as GPUI's surface shader uses it;
/// `full_range` luma 0 to 255, else 16 to 235 (chroma 16 to 240). `None`
/// when a plane is smaller than the size.
fn nv12_to_bgra(
    width: usize,
    height: usize,
    y: &[u8],
    y_stride: usize,
    uv: &[u8],
    uv_stride: usize,
    full_range: bool,
) -> Option<Vec<u8>> {
    let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
    if width == 0 || height == 0 || y_stride < width || uv_stride < cw * 2 {
        return None;
    }
    let y_len = y_stride.checked_mul(height - 1)?.checked_add(width)?;
    let uv_len = uv_stride.checked_mul(ch - 1)?.checked_add(cw * 2)?;
    if y.len() < y_len || uv.len() < uv_len {
        return None;
    }
    let (luma, chroma, offset) = if full_range {
        (1.0, 1.0, 0.0)
    } else {
        (255.0 / 219.0, 255.0 / 224.0, 16.0)
    };
    let mut out = Vec::with_capacity(width.checked_mul(height)?.checked_mul(4)?);
    for row in 0..height {
        let y_row = &y[row * y_stride..];
        let uv_row = &uv[(row / 2) * uv_stride..];
        for x in 0..width {
            let l = (f32::from(y_row[x]) - offset) * luma;
            let cb = (f32::from(uv_row[x / 2 * 2]) - 128.0) * chroma;
            let cr = (f32::from(uv_row[x / 2 * 2 + 1]) - 128.0) * chroma;
            let to_byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
            out.extend_from_slice(&[
                to_byte(l + 1.772 * cb),
                to_byte(l - 0.3441 * cb - 0.7141 * cr),
                to_byte(l + 1.402 * cr),
                255,
            ]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use std::ptr::{self, NonNull};

    use objc2_core_video::{CVPixelBufferCreate, kCVPixelFormatType_32ARGB};

    use super::*;

    /// A new `width` × `height` buffer of `format` (its pixels unset).
    fn buffer(width: usize, height: usize, format: u32) -> CFRetained<CVPixelBuffer> {
        let mut out = ptr::null_mut();
        // SAFETY: no attributes, and a writable out pointer.
        let status = unsafe {
            CVPixelBufferCreate(None, width, height, format, None, NonNull::from(&mut out))
        };
        assert_eq!(status, kCVReturnSuccess);
        // SAFETY: a successful create hands over a +1 reference.
        unsafe { CFRetained::from_raw(NonNull::new(out).expect("a buffer")) }
    }

    #[test]
    fn full_range_nv12_is_painted_as_it_is_and_others_are_copied() {
        let surface = frame_from_pixel_buffer(&buffer(
            4,
            2,
            kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        ))
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((surface.width, surface.height), (4, 2));
        assert!(matches!(surface.content, FrameContent::Surface(_)));
        for format in [
            kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            kCVPixelFormatType_32BGRA,
        ] {
            let copied =
                frame_from_pixel_buffer(&buffer(4, 2, format)).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!((copied.width, copied.height), (4, 2));
            let image = copied.content.image().expect("an image");
            assert_eq!(image.as_bytes(0).map(<[u8]>::len), Some(4 * 2 * 4));
        }
        assert!(frame_from_pixel_buffer(&buffer(4, 2, kCVPixelFormatType_32ARGB)).is_err());
    }

    #[test]
    fn converts_nv12_to_bgra() {
        // 2×2 pixels, padded rows: white and black luma over neutral
        // chroma stay gray; video range maps 16 to 0 and 235 to 255.
        let y = [235, 16, 0, 0, 126, 126, 0, 0];
        let uv = [128, 128, 0, 0];
        let video = nv12_to_bgra(2, 2, &y, 4, &uv, 4, false).expect("pixels");
        assert_eq!(&video[0..4], &[255, 255, 255, 255]);
        assert_eq!(&video[4..8], &[0, 0, 0, 255]);
        let full = nv12_to_bgra(2, 2, &y, 4, &uv, 4, true).expect("pixels");
        assert_eq!(&full[0..4], &[235, 235, 235, 255]);
        assert_eq!(full.len(), 16);
        // Strong red chroma: red up, blue down.
        let red = nv12_to_bgra(1, 1, &[128], 1, &[128, 255], 2, true).expect("pixels");
        assert!(red[2] == 255 && red[0] == 128);
    }

    #[test]
    fn refuses_planes_smaller_than_the_size() {
        assert!(nv12_to_bgra(2, 2, &[0; 3], 2, &[128; 2], 2, true).is_none());
        assert!(nv12_to_bgra(2, 2, &[0; 4], 2, &[128; 1], 2, true).is_none());
        assert!(nv12_to_bgra(2, 2, &[0; 4], 1, &[128; 2], 2, true).is_none());
        assert!(nv12_to_bgra(0, 2, &[0; 4], 2, &[128; 2], 2, true).is_none());
        assert!(nv12_to_bgra(3, 3, &[0; 9], 3, &[128; 8], 4, true).is_some());
    }
}
