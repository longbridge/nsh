//! Live streams the embedding app registers by name, which `<Video
//! srcObject>` plays (`HostStream.fromHost(name)`, `VideoProps::stream`):
//! the host pushes frames and PCM audio from any thread, and each video
//! showing the stream paints the frames as its picture, on the main thread,
//! and plays the audio. Neither passes through JS.
//!
//! The registry is the core's ([`nsh_core::stream`]: one entry per name
//! whether the host or a video came first, the newest frame wins, paused
//! sinks keep their last frame, a wrong push is logged once, a push nobody
//! shows costs nothing, names under `nsh/` are NSH's own). This module
//! gives it GPUI's frame: a frame becomes what GPUI paints on the pushing
//! thread, once, however many videos show it; the videos hear it through
//! the sinks they attached ([`Streams::attach`]), on the main thread, one
//! hop ([`crate::rt::invoke`]) per batch of frames. A frame pushed before
//! the app runs (a host registering at start) waits for the next push
//! rather than wedging the stream: the hop reports that it queued nothing
//! and the core schedules again with the next push.
//!
//! What GPUI paints is a [`RenderImage`] (BGRA bytes it uploads to its
//! sprite atlas), or on macOS a `CVPixelBuffer` it samples on the GPU
//! (`Window::paint_surface`). [`StreamHandle::push_bgra`] copies the bytes
//! into an image; `StreamHandle::push_pixel_buffer` (macOS) hands a
//! full-range NV12 buffer (`420f`) to GPUI without a copy and copies the
//! other formats it takes into an image. GPUI's Windows and Linux renderers
//! take no external textures, so frames there come as bytes.
//!
//! Audio is the core's too ([`nsh_core::stream::AudioOutput`]): each video
//! showing a stream attaches an output (`native/video/audio.rs`, feature
//! `video`) that [`StreamHandle::push_pcm`] reaches on the pushing thread.

#[cfg(target_os = "macos")]
mod apple;

use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::RenderImage;
use nsh_core::stream::{AudioAttachment, AudioOutput};

/// A frame of a stream: its size (pixels) and what GPUI paints.
#[derive(Clone)]
pub(crate) struct Frame {
    /// Width in pixels.
    pub(crate) width: u32,
    /// Height in pixels.
    pub(crate) height: u32,
    /// The picture, shared by every video painting it.
    pub(crate) content: FrameContent,
}

/// What a frame paints with.
#[derive(Clone)]
pub(crate) enum FrameContent {
    /// BGRA pixels in GPUI's sprite atlas.
    Image(Arc<RenderImage>),
    /// A full-range NV12 `CVPixelBuffer`, sampled by GPUI's surface shader
    /// without a copy.
    #[cfg(target_os = "macos")]
    Surface(apple::PixelBuffer),
}

impl FrameContent {
    /// The image, when the frame is one (it lives in the sprite atlas).
    pub(crate) fn image(&self) -> Option<&Arc<RenderImage>> {
        match self {
            FrameContent::Image(image) => Some(image),
            #[cfg(target_os = "macos")]
            FrameContent::Surface(_) => None,
        }
    }
}

/// What a stream tells the videos showing it.
pub(crate) type StreamEvent<'a> = nsh_core::stream::StreamEvent<'a, Frame>;

/// A stream the embedding app registered ([`Streams::register`],
/// [`crate::AppView::register_stream`]): where it pushes the frames and the
/// audio. Any thread. Dropping it ends the stream.
pub struct StreamHandle {
    inner: nsh_core::stream::StreamHandle<Frame>,
    /// A buffer this backend cannot paint was reported (once per handle).
    #[cfg(target_os = "macos")]
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
            #[cfg(target_os = "macos")]
            warned: AtomicBool::new(false),
        }
    }

    /// The name it was registered as.
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// Whether a frame pushed now would show: a video shows the stream and
    /// the registration is live. A host that makes its frames at a cost
    /// may skip them while this is `false`.
    pub fn wanted(&self) -> bool {
        self.inner.wanted()
    }

    /// Shows a frame in the videos playing the stream: `width` × `height`
    /// pixels of BGRA (8 bits each, opaque or premultiplied), `stride`
    /// bytes per row (at least `width * 4`). Copies the bytes. A frame
    /// that comes before the previous one was drawn replaces it. A frame
    /// that does not fit its size (or has none) is a wrong use: it shows
    /// nothing, logged once per stream.
    pub fn push_bgra(&self, width: usize, height: usize, stride: usize, bytes: &[u8]) {
        self.inner
            .push_bgra(width, height, stride, bytes, frame_from_bgra);
    }

    /// Shows `buffer` in the videos playing the stream, as
    /// [`Self::push_bgra`] shows bytes (the newest wins, `paused` keeps the
    /// last frame, the first one is the video's `onLoad` with its size). A
    /// full-range NV12 buffer
    /// (`kCVPixelFormatType_420YpCbCr8BiPlanarFullRange`, best
    /// `IOSurface`-backed) is retained and sampled by GPUI on the GPU,
    /// without a copy; video-range NV12 (`420v`) and `32BGRA` buffers are
    /// copied into an image, once. Other formats, and a buffer with no
    /// pixels, show nothing (logged once per handle). Nothing is read or
    /// retained while no video shows the stream.
    #[cfg(target_os = "macos")]
    pub fn push_pixel_buffer(&self, buffer: &objc2_core_video::CVPixelBuffer) {
        if !self.inner.wanted() {
            return;
        }
        match apple::frame_from_pixel_buffer(buffer) {
            Ok(frame) => self.inner.push(frame),
            Err(what) => {
                if !self.warned.swap(true, Ordering::AcqRel) {
                    log::warn!(
                        "stream `{}`: a pixel buffer shows nothing: {what}",
                        self.name()
                    );
                }
            }
        }
    }

    /// Plays `samples` in the videos playing the stream: interleaved `f32`
    /// in -1 to 1, `channels` per frame (1 to 8), `sample_rate` frames a
    /// second (8 000 to 192 000), in order, nothing replaced by newer
    /// samples. A video that is paused or muted drops them and stops what
    /// it queued at once; the others play them at their `volume` (0 to 1),
    /// each queueing at most about 200 ms (newer samples are dropped while
    /// it is full). Audio plays with feature `video`
    /// (GStreamer's `autoaudiosink`); without it, and while no video shows
    /// the stream, the call costs nothing. Samples of the wrong format are
    /// logged once per stream and dropped.
    pub fn push_pcm(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        self.inner.push_pcm(samples, channels, sample_rate);
    }

    /// Whether audio pushed now would be heard: the registration is live
    /// and a video plays the stream's audio. A host that decodes or
    /// resamples its audio at a cost may skip it while this is `false`.
    pub fn wants_audio(&self) -> bool {
        self.inner.wants_audio()
    }

    /// Ends the stream: the videos playing it show nothing from now on and
    /// their audio stops once what is queued played, until the name is
    /// registered again.
    pub fn end(&self) {
        self.inner.end();
    }
}

/// A frame of `width` × `height` BGRA pixels, rows packed (`pixels` holds
/// exactly `width * height * 4` bytes); `None` when that does not fit.
pub(crate) fn image_frame(width: u32, height: u32, pixels: Vec<u8>) -> Option<Frame> {
    let image = image::RgbaImage::from_raw(width, height, pixels)?;
    Some(Frame {
        width,
        height,
        content: FrameContent::Image(Arc::new(RenderImage::new(vec![image::Frame::new(image)]))),
    })
}

/// The GPUI image of `width` × `height` BGRA pixels in rows of `stride`
/// bytes (checked by the core already), rows packed; `None` when the size
/// does not fit a GPUI image.
fn frame_from_bgra(width: usize, height: usize, stride: usize, bytes: &[u8]) -> Option<Frame> {
    let row = width.checked_mul(4)?;
    if width == 0 || height == 0 || stride < row || bytes.len() < stride.checked_mul(height)? {
        return None;
    }
    let (w, h) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    let mut pixels = Vec::with_capacity(row.checked_mul(height)?);
    for y in 0..height {
        pixels.extend_from_slice(&bytes[y * stride..y * stride + row]);
    }
    image_frame(w, h, pixels)
}

/// A video's attachment to a stream, by name ([`Streams::attach`]);
/// dropping it detaches the video's sink. Main thread.
pub(crate) struct Attached(nsh_core::stream::Attached<Frame>);

impl Attached {
    /// The stream's name.
    pub(crate) fn name(&self) -> &Arc<str> {
        self.0.name()
    }

    /// Stops or resumes delivering frames to the video's sink (the last
    /// one stays; a video without one yet still takes the first).
    pub(crate) fn set_paused(&self, paused: bool) {
        self.0.set_paused(paused);
    }
}

/// The streams of one view's registry ([`crate::Components`]), by name
/// (shared with its videos, so a registration applies to videos showing
/// the name from then on, and to those already showing it). Main thread.
#[derive(Clone)]
pub(crate) struct Streams(nsh_core::stream::Streams<Frame>);

impl Default for Streams {
    fn default() -> Self {
        Streams(nsh_core::stream::Streams::new(
            |job: Box<dyn FnOnce() + Send>| crate::rt::invoke(job),
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
    /// handle takes its frames and audio. The videos whose `srcObject`
    /// names it, now or later, play it. Names under `nsh/` are NSH's own:
    /// refused (logged), the handle plays nothing.
    pub(crate) fn register(&self, name: &str) -> StreamHandle {
        StreamHandle::new(self.0.register(name))
    }

    /// [`Self::register`] for NSH's own `nsh/` names (the development
    /// host's test pattern).
    pub(crate) fn register_reserved(&self, name: &str) -> StreamHandle {
        StreamHandle::new(self.0.register_reserved(name))
    }

    /// Attaches `sink`, a video's, to stream `name`: it hears the frames
    /// and the end, on the main thread. A name nothing registered is logged
    /// once; the sink hears frames once something is.
    pub(crate) fn attach(&self, name: &str, sink: Box<dyn Fn(StreamEvent<'_>)>) -> Attached {
        Attached(self.0.attach(name, sink))
    }

    /// Attaches `output`, a video's, to stream `name`'s audio (heard on the
    /// pushing thread); dropping the attachment detaches it.
    #[cfg_attr(not(any(test, feature = "video")), allow(dead_code))]
    pub(crate) fn attach_audio(&self, name: &str, output: Arc<dyn AudioOutput>) -> AudioAttachment {
        self.0.attach_audio(name, output)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn solid(width: usize, height: usize, stride: usize, bgra: [u8; 4]) -> Vec<u8> {
        let mut bytes = vec![0u8; stride * height];
        for y in 0..height {
            for px in bytes[y * stride..y * stride + width * 4]
                .as_chunks_mut::<4>()
                .0
            {
                px.copy_from_slice(&bgra);
            }
        }
        bytes
    }

    #[test]
    fn a_frame_drops_its_row_padding() {
        // The rows are padded (a stride wider than the row), which the
        // frame drops: the image is width × height, every pixel the color.
        let frame = frame_from_bgra(2, 2, 12, &solid(2, 2, 12, [4, 5, 6, 255])).expect("a frame");
        assert_eq!((frame.width, frame.height), (2, 2));
        let image = frame.content.image().expect("an image");
        let bytes = image.as_bytes(0).expect("one frame");
        assert_eq!(bytes.len(), 16);
        for px in bytes.as_chunks::<4>().0 {
            assert_eq!(px, &[4, 5, 6, 255]);
        }
    }

    #[test]
    fn a_frame_refuses_a_wrong_size() {
        // Too few bytes, a stride narrower than a row, no size.
        assert!(frame_from_bgra(2, 2, 8, &[0; 12]).is_none());
        assert!(frame_from_bgra(2, 2, 4, &[0; 32]).is_none());
        assert!(frame_from_bgra(0, 2, 8, &[0; 32]).is_none());
        assert!(frame_from_bgra(2, 2, 8, &[0; 16]).is_some());
        assert!(image_frame(2, 2, vec![0; 15]).is_none());
    }

    #[test]
    fn one_entry_per_name() {
        let streams = Streams::default();
        let _a = streams.attach("x", Box::new(|_| {}));
        let h = streams.register("x");
        assert_eq!(h.name(), "x");
        assert_eq!(format!("{h:?}"), r#"StreamHandle { name: "x" }"#);
        assert_eq!(format!("{streams:?}"), r#"{"x"}"#);
    }

    #[test]
    fn nsh_names_are_registered_only_as_reserved() {
        let streams = Streams::default();
        let _a = streams.attach("nsh/test-pattern", Box::new(|_| {}));
        // The public path refuses NSH's names: nothing would show.
        assert!(!streams.register("nsh/test-pattern").wanted());
        // NSH's hosts register them.
        assert!(streams.register_reserved("nsh/test-pattern").wanted());
    }

    /// Records the samples it hears.
    #[derive(Default)]
    struct Speaker(Mutex<Vec<f32>>);

    impl AudioOutput for Speaker {
        fn write(&self, samples: &[f32], _channels: u16, _sample_rate: u32) {
            self.0.lock().unwrap().extend_from_slice(samples);
        }
    }

    #[test]
    fn audio_reaches_the_attached_outputs_until_detached() {
        let streams = Streams::default();
        let handle = streams.register("call");
        assert!(!handle.wants_audio());
        let speaker = Arc::new(Speaker::default());
        let attached = streams.attach_audio("call", speaker.clone());
        assert!(handle.wants_audio());
        handle.push_pcm(&[0.5, -0.5], 2, 48_000);
        drop(attached);
        assert!(!handle.wants_audio());
        handle.push_pcm(&[0.25, 0.25], 2, 48_000);
        assert_eq!(*speaker.0.lock().unwrap(), [0.5, -0.5]);
    }
}
