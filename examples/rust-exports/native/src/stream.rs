//! `rust-exports.stream`, a live stream of the crate: a test pattern drawn
//! 30 times a second on a thread of its own and pushed as `CVPixelBuffer`s,
//! as a camera or a call would push its frames. `lib.rs` registers it with
//! `#[native_components]` on macOS and iOS, so the hosts nsh builds for the
//! app show it in `App.tsx`'s `<Video srcObject>`; `Ledger.stream()` tells
//! the app whether this host registered it.

use std::sync::atomic::{AtomicBool, Ordering};

/// The name `App.tsx` names the stream with (`HostStream.fromHost`).
const NAME: &str = "rust-exports.stream";

/// Whether the host this crate runs in registered the stream.
static REGISTERED: AtomicBool = AtomicBool::new(false);

/// The stream's name where this host registered it, `None` elsewhere (the
/// headless runner, a platform without streams).
pub(crate) fn registered() -> Option<String> {
    REGISTERED.load(Ordering::Acquire).then(|| NAME.to_owned())
}

#[cfg(target_os = "macos")]
pub(crate) fn register(components: &nsh_platform_macos::Components) {
    apple::start(components.register_stream(NAME));
}

#[cfg(target_os = "ios")]
pub(crate) fn register(components: &nsh_platform_ios::Components) {
    apple::start(components.register_stream(NAME));
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple {
    use std::{ffi::c_void, sync::atomic::Ordering, thread, time::Duration};

    #[cfg(target_os = "ios")]
    use nsh_platform_ios::StreamHandle;
    #[cfg(target_os = "macos")]
    use nsh_platform_macos::StreamHandle;

    /// The frames' size (pixels).
    const WIDTH: usize = 320;
    const HEIGHT: usize = 180;
    /// `kCVPixelFormatType_32BGRA`.
    const BGRA: u32 = 0x4247_5241;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        fn CVPixelBufferCreate(
            allocator: *const c_void,
            width: usize,
            height: usize,
            format: u32,
            attributes: *const c_void,
            out: *mut *mut c_void,
        ) -> i32;
        fn CVPixelBufferLockBaseAddress(buffer: *mut c_void, flags: u64) -> i32;
        fn CVPixelBufferUnlockBaseAddress(buffer: *mut c_void, flags: u64) -> i32;
        fn CVPixelBufferGetBaseAddress(buffer: *mut c_void) -> *mut u8;
        fn CVPixelBufferGetBytesPerRow(buffer: *mut c_void) -> usize;
    }

    /// Starts the thread pushing frames to `stream` for as long as the
    /// process runs.
    pub(super) fn start(stream: StreamHandle) {
        super::REGISTERED.store(true, Ordering::Release);
        thread::spawn(move || {
            let mut tick: u32 = 0;
            loop {
                if let Some(frame) = frame(tick) {
                    // SAFETY: a CVPixelBuffer this thread made and still holds.
                    unsafe {
                        stream.push_pixel_buffer(frame);
                        CFRelease(frame);
                    }
                }
                tick = tick.wrapping_add(1);
                thread::sleep(Duration::from_millis(33));
            }
        });
    }

    /// A frame of the pattern at `tick`: a color sweeping through the hues
    /// behind a bar crossing left to right, so a still and a stuck stream
    /// look different.
    fn frame(tick: u32) -> Option<*mut c_void> {
        let mut buffer: *mut c_void = std::ptr::null_mut();
        // SAFETY: CoreVideo's documented API; the buffer is locked while
        // its bytes are written and released by the caller.
        unsafe {
            if CVPixelBufferCreate(
                std::ptr::null(),
                WIDTH,
                HEIGHT,
                BGRA,
                std::ptr::null(),
                &mut buffer,
            ) != 0
                || buffer.is_null()
            {
                return None;
            }
            CVPixelBufferLockBaseAddress(buffer, 0);
            let base = CVPixelBufferGetBaseAddress(buffer);
            let stride = CVPixelBufferGetBytesPerRow(buffer);
            let (r, g, b) = hue((tick % 360) as f32);
            let bar = (tick as usize * 4) % WIDTH;
            for y in 0..HEIGHT {
                let row = std::slice::from_raw_parts_mut(base.add(y * stride), WIDTH * 4);
                for x in 0..WIDTH {
                    let on_bar = x.abs_diff(bar) < 8;
                    let px = &mut row[x * 4..x * 4 + 4];
                    px.copy_from_slice(&if on_bar {
                        [255, 255, 255, 255]
                    } else {
                        [b, g, r, 255]
                    });
                }
            }
            CVPixelBufferUnlockBaseAddress(buffer, 0);
        }
        Some(buffer)
    }

    /// A saturated color at `degrees` of hue, as (red, green, blue).
    fn hue(degrees: f32) -> (u8, u8, u8) {
        let h = degrees / 60.0;
        let x = (1.0 - (h % 2.0 - 1.0).abs()) * 255.0;
        let (r, g, b) = match h as u32 {
            0 => (255.0, x, 0.0),
            1 => (x, 255.0, 0.0),
            2 => (0.0, 255.0, x),
            3 => (0.0, x, 255.0),
            4 => (x, 0.0, 255.0),
            _ => (255.0, 0.0, x),
        };
        (r as u8, g as u8, b as u8)
    }
}
