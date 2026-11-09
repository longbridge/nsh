//! Live streams the embedding app registers by name, which `<Video
//! srcObject>` plays (`HostStream.fromHost(name)`, `VideoProps::stream`):
//! the host pushes frames (its own `CVPixelBuffer`s, shown without a copy
//! in any format `AVSampleBufferDisplayLayer` takes, BGRA or biplanar YUV;
//! or BGRA bytes copied into one) from any thread, and each video showing
//! the stream draws them in an `AVSampleBufferDisplayLayer` of its own, on
//! the main thread. PCM audio pushed to the same handle plays in each video
//! through an `AVAudioEngine` of its own (`stream_audio.rs`), with the
//! video's `paused`, `muted` and `volume`. Neither passes through JS.
//!
//! The registry is the core's ([`nsh_core::stream`]: one entry per name
//! whether the host registered it or a video asked for it first, the
//! newest frame delivered with one dispatch to the main queue, `paused`
//! keeping the last frame, the end blanking the layers, a frame that does
//! not fit its size logged once); this module is the Apple side: the
//! frame type, the dispatch, and the layer each video's sink draws in.

use std::{
    ffi::c_void,
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use dispatch2::DispatchQueue;
use objc2::{Encode, Encoding, encode::RefEncode, msg_send, rc::Retained, runtime::AnyObject};

use crate::stream_audio::StreamAudio;

/// CoreVideo's `CVPixelBufferRef`: a frame the embedding app hands a
/// stream, retained while NSH holds it.
pub type CVPixelBufferRef = *mut c_void;

/// CoreMedia's `CMTime`; a zeroed one is `kCMTimeInvalid`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

// SAFETY: the layout and encoding of CoreMedia's `CMTime` (`{?=qiIq}`).
unsafe impl Encode for CMTime {
    const ENCODING: Encoding = Encoding::Struct(
        "?",
        &[i64::ENCODING, i32::ENCODING, u32::ENCODING, i64::ENCODING],
    );
}

/// CoreMedia's `CMSampleTimingInfo`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CMSampleTimingInfo {
    duration: CMTime,
    presentation: CMTime,
    decode: CMTime,
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFDictionarySetValue(dict: *const c_void, key: *const c_void, value: *const c_void);
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVPixelBufferGetWidth(buffer: *const c_void) -> usize;
    fn CVPixelBufferGetHeight(buffer: *const c_void) -> usize;
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

/// `kCVPixelFormatType_32BGRA`.
const BGRA: u32 = 0x4247_5241;

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    static kCMSampleAttachmentKey_DisplayImmediately: *const c_void;
    fn CMVideoFormatDescriptionCreateForImageBuffer(
        allocator: *const c_void,
        image_buffer: *const c_void,
        out: *mut *const c_void,
    ) -> i32;
    fn CMSampleBufferCreateReadyWithImageBuffer(
        allocator: *const c_void,
        image_buffer: *const c_void,
        format: *const c_void,
        timing: *const CMSampleTimingInfo,
        out: *mut *const c_void,
    ) -> i32;
    fn CMSampleBufferGetSampleAttachmentsArray(
        buffer: *const c_void,
        create: bool,
    ) -> *const c_void;
}

/// A frame: a retained `CVPixelBuffer`, and the sample buffer of it once
/// a layer asked for one (made once, shared by every video showing it).
struct Frame {
    buffer: NonNull<c_void>,
    sample: std::sync::OnceLock<Option<Sample>>,
}

// SAFETY: a CVPixelBuffer is a CoreFoundation object, which any thread may
// hold and release.
unsafe impl Send for Frame {}

impl Frame {
    /// Retains `buffer`; `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// `buffer` is a `CVPixelBufferRef` or null.
    unsafe fn retain(buffer: CVPixelBufferRef) -> Option<Self> {
        let buffer = NonNull::new(buffer)?;
        // SAFETY: a live CF object, as the caller guarantees.
        unsafe { CFRetain(buffer.as_ptr()) };
        Some(Frame::new(buffer))
    }

    /// A new BGRA buffer of `width` × `height` pixels holding `bytes`
    /// (`stride` bytes a row), or `None` when CoreVideo refuses the size.
    fn from_bgra(width: usize, height: usize, stride: usize, bytes: &[u8]) -> Option<Self> {
        let mut buffer: *mut c_void = std::ptr::null_mut();
        // SAFETY: CoreVideo's documented API; the buffer is locked while
        // its rows are written, and comes retained (+1), which `Frame`
        // releases.
        unsafe {
            if CVPixelBufferCreate(
                std::ptr::null(),
                width,
                height,
                BGRA,
                std::ptr::null(),
                &mut buffer,
            ) != 0
            {
                return None;
            }
            let buffer = NonNull::new(buffer)?;
            CVPixelBufferLockBaseAddress(buffer.as_ptr(), 0);
            let base = CVPixelBufferGetBaseAddress(buffer.as_ptr());
            let own = CVPixelBufferGetBytesPerRow(buffer.as_ptr());
            for y in 0..height {
                std::ptr::copy_nonoverlapping(
                    bytes[y * stride..].as_ptr(),
                    base.add(y * own),
                    width * 4,
                );
            }
            CVPixelBufferUnlockBaseAddress(buffer.as_ptr(), 0);
            Some(Frame::new(buffer))
        }
    }

    /// A frame of a retained `buffer`.
    fn new(buffer: NonNull<c_void>) -> Self {
        Frame {
            buffer,
            sample: std::sync::OnceLock::new(),
        }
    }

    fn size(&self) -> (usize, usize) {
        // SAFETY: a retained CVPixelBuffer.
        unsafe {
            (
                CVPixelBufferGetWidth(self.buffer.as_ptr()),
                CVPixelBufferGetHeight(self.buffer.as_ptr()),
            )
        }
    }

    /// The sample buffer of the frame to display at once, made once for
    /// every layer showing it; `None` when CoreMedia refuses the buffer
    /// (not a video pixel format).
    fn sample(&self) -> Option<&Sample> {
        self.sample.get_or_init(|| self.make_sample()).as_ref()
    }

    fn make_sample(&self) -> Option<Sample> {
        let mut format: *const c_void = std::ptr::null();
        let mut sample: *const c_void = std::ptr::null();
        // SAFETY: CoreMedia's documented API on a retained CVPixelBuffer;
        // what it creates is released by `Sample`'s drop.
        unsafe {
            if CMVideoFormatDescriptionCreateForImageBuffer(
                std::ptr::null(),
                self.buffer.as_ptr(),
                &mut format,
            ) != 0
                || format.is_null()
            {
                return None;
            }
            let timing = CMSampleTimingInfo::default();
            let made = CMSampleBufferCreateReadyWithImageBuffer(
                std::ptr::null(),
                self.buffer.as_ptr(),
                format,
                &timing,
                &mut sample,
            );
            CFRelease(format);
            if made != 0 || sample.is_null() {
                return None;
            }
            // Shown as it comes: a live stream has no timeline.
            let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, true);
            if !attachments.is_null() && CFArrayGetCount(attachments) > 0 {
                CFDictionarySetValue(
                    CFArrayGetValueAtIndex(attachments, 0),
                    kCMSampleAttachmentKey_DisplayImmediately,
                    kCFBooleanTrue,
                );
            }
        }
        Some(Sample(sample))
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: retained in `retain` or created in `from_bgra`.
        unsafe { CFRelease(self.buffer.as_ptr()) };
    }
}

/// CoreMedia's `opaqueCMSampleBuffer`: what a `CMSampleBufferRef` points
/// to, with the encoding `enqueueSampleBuffer:` declares (a pointer to it,
/// `^{opaqueCMSampleBuffer=}`), which objc2 checks in debug builds.
#[repr(C)]
struct OpaqueCMSampleBuffer {
    _private: [u8; 0],
}

// SAFETY: the pointer's Objective-C encoding, as CoreMedia declares the
// struct (opaque, no fields).
unsafe impl RefEncode for OpaqueCMSampleBuffer {
    const ENCODING_REF: Encoding =
        Encoding::Pointer(&Encoding::Struct("opaqueCMSampleBuffer", &[]));
}

/// A `CMSampleBuffer` made for one frame.
struct Sample(*const c_void);

// SAFETY: a CMSampleBuffer is a CoreFoundation object, which any thread may
// hold and release; it lives in its `Frame`, which crosses threads.
unsafe impl Send for Sample {}
// SAFETY: as above; it is only read through `as_ref`.
unsafe impl Sync for Sample {}

impl Sample {
    /// The buffer as `enqueueSampleBuffer:` takes it.
    fn as_ref(&self) -> *const OpaqueCMSampleBuffer {
        self.0.cast()
    }
}

impl Drop for Sample {
    fn drop(&mut self) {
        // SAFETY: created in `Frame::make_sample`.
        unsafe { CFRelease(self.0) };
    }
}

/// A stream the embedding app registered ([`Streams::register`]): where it
/// pushes the frames and audio. Any thread. Dropping it ends the stream.
pub struct StreamHandle {
    inner: nsh_core::stream::StreamHandle<Frame>,
    /// A pixel buffer without pixels was reported (once per stream).
    warned: AtomicBool,
}

impl std::fmt::Debug for StreamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamHandle")
            .field("name", &self.inner.name())
            .finish()
    }
}

impl StreamHandle {
    fn new(inner: nsh_core::stream::StreamHandle<Frame>) -> Self {
        StreamHandle {
            inner,
            warned: AtomicBool::new(false),
        }
    }

    /// The name it was registered as.
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// Shows `buffer`, the host's `CVPixelBuffer`, in the videos playing
    /// the stream without copying its pixels: any format
    /// `AVSampleBufferDisplayLayer` takes (`32BGRA`, the biplanar
    /// `420YpCbCr8BiPlanarVideoRange` / `FullRange` cameras and decoders
    /// hand out, ...), an `IOSurface`-backed one staying on the GPU. NSH
    /// retains it while it shows it; the caller keeps its own reference and
    /// must not write to it afterwards. A frame that comes before the
    /// previous one was drawn replaces it; the first after the video
    /// attached is its `onLoad`, with the buffer's pixel size. While no
    /// video shows the stream nothing is done (the buffer is not even
    /// retained). A buffer without pixels is logged once and shows nothing.
    ///
    /// # Safety
    ///
    /// `buffer` is a `CVPixelBufferRef` (null shows nothing).
    pub unsafe fn push_pixel_buffer(&self, buffer: CVPixelBufferRef) {
        if buffer.is_null() || !self.inner.wanted() {
            return;
        }
        // SAFETY: a CVPixelBufferRef, as the caller guarantees.
        let (width, height) = unsafe {
            (
                CVPixelBufferGetWidth(buffer),
                CVPixelBufferGetHeight(buffer),
            )
        };
        if width == 0 || height == 0 {
            if !self.warned.swap(true, Ordering::AcqRel) {
                log::warn!(
                    "stream `{}`: a pixel buffer of {width}×{height} pixels shows nothing",
                    self.inner.name()
                );
            }
            return;
        }
        // SAFETY: guaranteed by the caller.
        if let Some(frame) = unsafe { Frame::retain(buffer) } {
            self.inner.push(frame);
        }
    }

    /// Plays `samples` in the videos playing the stream: interleaved `f32`
    /// in -1 to 1, `channels` per frame (1 to 8; more than two are mixed
    /// into stereo), `sample_rate` frames a second (8 000 to 192 000). Push
    /// them as they come, in order, from one thread; each video plays them
    /// at its `volume` (applied at once), stops what is queued and drops
    /// new samples while `paused` or `muted`, and drops the newest when
    /// more than about 200 ms wait for the device, so a host pushing ahead
    /// of real time adds no delay. While no video shows the
    /// stream the call costs nothing; a wrong format is logged once and
    /// dropped.
    pub fn push_pcm(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        self.inner.push_pcm(samples, channels, sample_rate);
    }

    /// Shows a frame given as bytes: `width` × `height` pixels of BGRA (8
    /// bits each), `stride` bytes a row (at least `width * 4`), copied into
    /// a `CVPixelBuffer`, as the other platforms' handles take frames. A
    /// frame that comes before the previous one was drawn replaces it. A
    /// size the bytes do not cover, or an empty one, is logged once and
    /// shows nothing; while no video shows the stream the bytes are not
    /// copied.
    pub fn push_bgra(&self, width: usize, height: usize, stride: usize, bytes: &[u8]) {
        self.inner
            .push_bgra(width, height, stride, bytes, Frame::from_bgra);
    }

    /// Ends the stream: the videos playing it show nothing and stop its
    /// audio from now on, until the name is registered again.
    pub fn end(&self) {
        self.inner.end();
    }
}

/// A video's attachment to a stream, by name ([`Streams::attach`]): its
/// layer and its audio output. Dropping it detaches both and stops the
/// audio.
pub struct Attached {
    video: nsh_core::stream::Attached<Frame>,
    audio: Arc<StreamAudio>,
    _audio: nsh_core::stream::AudioAttachment,
}

impl Attached {
    /// The stream's name.
    pub fn name(&self) -> &Arc<str> {
        self.video.name()
    }

    /// Stops or resumes drawing frames in the video's layer (the last one
    /// stays; a layer with none yet still draws the first) and playing the
    /// stream's audio (pausing stops what is queued at once).
    pub fn set_paused(&self, paused: bool) {
        self.video.set_paused(paused);
        self.audio.set_paused(paused);
    }

    /// The video's `muted` (stops what is queued at once and drops the
    /// audio) and `volume` (clamped to 0 to 1, applied at once), for the
    /// stream's audio. Main thread.
    pub fn set_sound(&self, muted: bool, volume: f32) {
        self.audio.set_sound(muted, volume);
    }
}

/// The streams of one surface, by name (shared with its videos, so a
/// registration applies to videos showing the name from then on, and to
/// those already showing it). Main thread.
#[derive(Clone)]
pub struct Streams(nsh_core::stream::Streams<Frame>);

impl Default for Streams {
    fn default() -> Self {
        // The hop: one dispatch to the main queue per batch of frames.
        Streams(nsh_core::stream::Streams::new(
            |job: Box<dyn FnOnce() + Send>| {
                DispatchQueue::main().exec_async(job);
                true
            },
        ))
    }
}

impl std::fmt::Debug for Streams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Streams {
    /// Registers stream `name` (replacing an earlier registration): the
    /// handle takes its frames. The videos whose `srcObject` names it,
    /// now or later, play it. Main thread.
    ///
    /// Names starting with `nsh/` are NSH's own
    /// ([`nsh_core::stream::RESERVED_PREFIX`]): they are refused (logged),
    /// and the handle plays nothing.
    pub fn register(&self, name: &str) -> StreamHandle {
        StreamHandle::new(self.0.register(name))
    }

    /// [`Self::register`] without the check of reserved names: NSH's
    /// development hosts register `nsh/test-pattern` with it.
    #[doc(hidden)]
    pub fn register_reserved(&self, name: &str) -> StreamHandle {
        StreamHandle::new(self.0.register_reserved(name))
    }

    /// Attaches `layer`, a video's `AVSampleBufferDisplayLayer`, to stream
    /// `name`: it draws the frames, and `on_size` hears the first frame's
    /// size (pixels) after each start of the stream. The video's audio
    /// output attaches with it (silent until the host pushes audio; set its
    /// `paused`, `muted` and `volume` on the attachment). A name nothing
    /// registered is logged once; the layer draws once something is.
    pub fn attach(
        &self,
        name: &str,
        layer: Retained<AnyObject>,
        on_size: Box<dyn Fn(usize, usize)>,
    ) -> Attached {
        let sink = Box::new(
            move |event: nsh_core::stream::StreamEvent<'_, Frame>| match event {
                nsh_core::stream::StreamEvent::Frame { frame, first } => {
                    let Some(sample) = frame.sample() else { return };
                    // SAFETY: AVSampleBufferDisplayLayer's documented API; the
                    // sample buffer is live until `sample` drops.
                    unsafe {
                        // 2: failed (a buffer it could not decode): flush and go on.
                        let status: isize = msg_send![&layer, status];
                        if status == 2 {
                            let _: () = msg_send![&layer, flush];
                        }
                        let _: () = msg_send![&layer, enqueueSampleBuffer: sample.as_ref()];
                    }
                    if first {
                        let (width, height) = frame.size();
                        on_size(width, height);
                    }
                }
                nsh_core::stream::StreamEvent::Ended => {
                    // SAFETY: AVSampleBufferDisplayLayer's documented API.
                    let _: () = unsafe { msg_send![&layer, flushAndRemoveImage] };
                }
            },
        );
        let video = self.0.attach(name, sink);
        let audio = Arc::new(StreamAudio::default());
        let attachment = self.0.attach_audio(name, audio.clone());
        Attached {
            video,
            audio,
            _audio: attachment,
        }
    }
}

/// A new `AVSampleBufferDisplayLayer` (AVFoundation loaded), or `None`
/// where the class is missing.
pub fn display_layer() -> Option<Retained<AnyObject>> {
    if !crate::audio::av_foundation() {
        return None;
    }
    let cls = objc2::runtime::AnyClass::get(c"AVSampleBufferDisplayLayer")?;
    // SAFETY: a CALayer subclass's documented constructor.
    Some(unsafe { msg_send![cls, new] })
}

/// `AVLayerVideoGravity*` for a video's `resize_mode`.
pub fn gravity(mode: nsh_core::ResizeMode) -> Retained<objc2_foundation::NSString> {
    objc2_foundation::NSString::from_str(match mode {
        nsh_core::ResizeMode::Cover => "AVLayerVideoGravityResizeAspectFill",
        nsh_core::ResizeMode::Stretch => "AVLayerVideoGravityResize",
        _ => "AVLayerVideoGravityResizeAspect",
    })
}
