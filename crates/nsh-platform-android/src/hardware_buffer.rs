//! The pixels of a live stream's `HardwareBuffer` frame on the CPU, for
//! `Mounter.Stream.pushHardwareBuffer` before API 29: no `Bitmap` wraps a
//! buffer there (`Bitmap.wrapHardwareBuffer` is API 29), so the frame is
//! read through the NDK's `AHardwareBuffer_lock` (API 26, NSH's minimum)
//! and copied, as a BGRA frame is.

use std::{
    ffi::c_void,
    ptr::{null, null_mut},
};

use jni::{
    JNIEnv,
    objects::{JClass, JIntArray, JObject},
    sys::{JNI_FALSE, JNI_TRUE, jboolean, jint},
};

/// The NDK's opaque `AHardwareBuffer`.
#[repr(C)]
struct AHardwareBuffer {
    _private: [u8; 0],
}

/// The NDK's `AHardwareBuffer_Desc`.
#[repr(C)]
#[derive(Default)]
struct AHardwareBufferDesc {
    width: u32,
    height: u32,
    layers: u32,
    format: u32,
    usage: u64,
    /// In pixels.
    stride: u32,
    rfu0: u32,
    rfu1: u64,
}

/// `AHARDWAREBUFFER_FORMAT_R8G8B8A8_UNORM` and `_R8G8B8X8_UNORM`.
const FORMAT_RGBA_8888: u32 = 1;
const FORMAT_RGBX_8888: u32 = 2;
/// The CPU read bits of a usage (`AHARDWAREBUFFER_USAGE_CPU_READ_MASK`).
const USAGE_CPU_READ_MASK: u64 = 0xF;

#[link(name = "android")]
unsafe extern "C" {
    fn AHardwareBuffer_fromHardwareBuffer(
        env: *mut jni::sys::JNIEnv,
        buffer: jni::sys::jobject,
    ) -> *mut AHardwareBuffer;
}

#[link(name = "nativewindow")]
unsafe extern "C" {
    fn AHardwareBuffer_describe(buffer: *const AHardwareBuffer, desc: *mut AHardwareBufferDesc);
    fn AHardwareBuffer_lock(
        buffer: *mut AHardwareBuffer,
        usage: u64,
        fence: i32,
        rect: *const c_void,
        address: *mut *mut c_void,
    ) -> i32;
    fn AHardwareBuffer_unlock(buffer: *mut AHardwareBuffer, fence: *mut i32) -> i32;
}

/// `Native.hardwareBufferPixels(buffer, pixels)`: copies the pixels of
/// `buffer` into `pixels` as `0xAARRGGBB` ints (what `Bitmap.setPixels`
/// takes), `width × height` of them; false, logged, when the buffer cannot
/// be read. Any thread.
#[unsafe(no_mangle)]
extern "system" fn Java_com_longbridge_nsh_Native_hardwareBufferPixels<'local>(
    env: JNIEnv<'local>,
    _: JClass<'local>,
    buffer: JObject<'local>,
    pixels: JIntArray<'local>,
) -> jboolean {
    match copy(&env, &buffer, &pixels) {
        Ok(()) => JNI_TRUE,
        Err(e) => {
            log::warn!("a live stream's hardware buffer cannot be read: {e}");
            JNI_FALSE
        }
    }
}

fn copy(env: &JNIEnv<'_>, buffer: &JObject<'_>, pixels: &JIntArray<'_>) -> Result<(), String> {
    if buffer.is_null() {
        return Err("it is null".into());
    }
    // SAFETY: `buffer` is a live `android.hardware.HardwareBuffer` local
    // reference for the length of this call; the pointer acquires no
    // reference and is not kept past the call.
    let raw = unsafe { AHardwareBuffer_fromHardwareBuffer(env.get_raw(), buffer.as_raw()) };
    if raw.is_null() {
        return Err("it has no native buffer (closed?)".into());
    }
    let mut desc = AHardwareBufferDesc::default();
    // SAFETY: `raw` is a live buffer and `desc` a writable descriptor.
    unsafe { AHardwareBuffer_describe(raw, &mut desc) };
    let opaque = match desc.format {
        FORMAT_RGBA_8888 => false,
        FORMAT_RGBX_8888 => true,
        other => return Err(format!("format {other} is not RGBA_8888 or RGBX_8888")),
    };
    let read = desc.usage & USAGE_CPU_READ_MASK;
    if read == 0 {
        return Err("its usage has no CPU read".into());
    }
    let (width, height, stride) = (
        desc.width as usize,
        desc.height as usize,
        desc.stride as usize,
    );
    if width == 0 || height == 0 || stride < width {
        return Err(format!(
            "{width}×{height} with a stride of {stride} has no pixels"
        ));
    }
    let length = env.get_array_length(pixels).map_err(|e| e.to_string())? as usize;
    if length < width * height {
        return Err(format!("{length} ints are fewer than {width}×{height}"));
    }
    let mut address = null_mut();
    // SAFETY: `raw` is live; the lock asks for the CPU read usage the buffer
    // was made with, no fence (-1) and the whole buffer (null rect).
    let locked = unsafe { AHardwareBuffer_lock(raw, read, -1, null(), &mut address) };
    if locked != 0 || address.is_null() {
        return Err(format!("the lock failed ({locked})"));
    }
    let mut out: Vec<jint> = Vec::with_capacity(width * height);
    {
        // SAFETY: a locked buffer maps its rows of `stride` 4-byte pixels at
        // `address` until it is unlocked below; the last row ends at its
        // `width`th pixel.
        let bytes = unsafe {
            std::slice::from_raw_parts(address as *const u8, ((height - 1) * stride + width) * 4)
        };
        for y in 0..height {
            let row = &bytes[y * stride * 4..(y * stride + width) * 4];
            out.extend(row.chunks_exact(4).map(|p| argb(p, opaque)));
        }
    }
    // SAFETY: locked above; a null fence waits for the unlock to finish.
    unsafe { AHardwareBuffer_unlock(raw, null_mut()) };
    env.set_int_array_region(pixels, 0, &out)
        .map_err(|e| e.to_string())
}

/// One RGBA pixel (bytes R, G, B, A) as `0xAARRGGBB`; RGBX's fourth byte
/// is undefined, so `opaque` pixels take 0xFF.
fn argb(p: &[u8], opaque: bool) -> jint {
    let a = if opaque { 0xFF } else { u32::from(p[3]) };
    ((a << 24) | (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2])) as jint
}
