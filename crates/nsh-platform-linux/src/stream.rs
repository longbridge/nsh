//! Live streams the embedding app registers by name, which `<Video
//! srcObject>` plays (`HostStream.fromHost(name)`, `VideoProps::stream`):
//! the host pushes frames and audio from any thread, and each video
//! showing the stream draws the frames in its `gtk::Picture`, on the main
//! thread, and plays the audio through a GStreamer pipeline of its own
//! ([`crate::stream_audio`]). Neither passes through JS.
//!
//! A frame is BGRA bytes ([`StreamHandle::push_bgra`]), drawn as a
//! `gdk::MemoryTexture` whose bytes are a `glib::Bytes` the videos'
//! textures share instead of copying again, or a dmabuf
//! ([`StreamHandle::push_dmabuf`]), drawn as one `gdk::DmabufTexture` the
//! videos share, without a copy: GTK hands the buffer to the GPU (or the
//! compositor) as it is. A dmabuf's texture is built on the main thread at
//! its first draw; the host's release runs once nothing uses the buffer
//! any more: GTK freed the texture, a newer frame replaced it before it
//! was drawn, no video wanted it, or GTK refused its format.
//!
//! The rules every backend shares (one entry per name whether the host or
//! a video came first, the newest frame wins, paused sinks keep their last
//! frame, a name nothing registered is logged once, a frame that does not
//! fit its size shows nothing, audio straight from the pushing thread) are
//! [`nsh_core::stream`]'s; this module gives it the frame GTK draws, a
//! [`Frame`], and the hop to GLib's default main context, where the sinks
//! (a video's picture and emitter) are told.

use std::{
    os::fd::RawFd,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use gtk4::{gdk, glib, prelude::*};
use nsh_core::stream::{AudioAttachment, AudioOutput};

/// A frame: `width` × `height` pixels, as bytes or a dmabuf.
pub(crate) struct Frame {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pixels: Pixels,
}

enum Pixels {
    /// BGRA (8 bits each, opaque or premultiplied), `stride` bytes per
    /// row: `stride * height` bytes, a `glib::Bytes` so the videos'
    /// textures share them instead of copying them again.
    Memory { stride: usize, bytes: glib::Bytes },
    /// A dmabuf the host owns until its release runs.
    Dmabuf(Dmabuf),
}

impl Frame {
    /// A frame of `bytes`, copied: the core's `make` ([`StreamHandle::push_bgra`]),
    /// called once the size is checked (`stride * height` bytes are there).
    fn make(width: usize, height: usize, stride: usize, bytes: &[u8]) -> Option<Self> {
        Some(Frame {
            width,
            height,
            pixels: Pixels::Memory {
                stride,
                bytes: glib::Bytes::from(&bytes[..stride * height]),
            },
        })
    }

    /// The frame as a texture (main thread); `None` for a dmabuf GTK
    /// cannot show (logged once per format and stream).
    pub(crate) fn texture(&self) -> Option<gdk::Texture> {
        match &self.pixels {
            Pixels::Memory { stride, bytes } => Some(
                gdk::MemoryTexture::new(
                    self.width as i32,
                    self.height as i32,
                    gdk::MemoryFormat::B8g8r8a8Premultiplied,
                    bytes,
                    *stride,
                )
                .upcast(),
            ),
            Pixels::Dmabuf(dmabuf) => dmabuf.texture(),
        }
    }
}

/// A plane of a [`DmabufFrame`]: its file descriptor and where its pixels
/// are in it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmabufPlane {
    /// The dmabuf's file descriptor (planes may share one). The host keeps
    /// it open until the frame's release runs. `-1` (the default) is no
    /// descriptor: a frame with one shows nothing.
    pub fd: RawFd,
    /// Where the plane starts in `fd`, in bytes.
    pub offset: u32,
    /// The bytes from one row of the plane to the next.
    pub stride: u32,
}

impl Default for DmabufPlane {
    fn default() -> Self {
        DmabufPlane {
            fd: -1,
            offset: 0,
            stride: 0,
        }
    }
}

/// What the host runs once a frame's buffer is free again
/// ([`DmabufFrame::release`]), on any thread.
pub type DmabufRelease = Box<dyn FnOnce() + Send>;

/// A frame in GPU memory ([`StreamHandle::push_dmabuf`]): a dmabuf of
/// `width` × `height` pixels in a DRM format (`fourcc`, `modifier`, as
/// `drm_fourcc.h` defines them), one [`DmabufPlane`] per plane. Shown as a
/// `gdk::DmabufTexture`, without a copy.
#[non_exhaustive]
pub struct DmabufFrame {
    /// The width, in pixels.
    pub width: u32,
    /// The height, in pixels.
    pub height: u32,
    /// The pixel format, a DRM fourcc (`DRM_FORMAT_ARGB8888`, `DRM_FORMAT_NV12`, ...).
    pub fourcc: u32,
    /// The DRM format modifier (`DRM_FORMAT_MOD_LINEAR`, 0, by default).
    pub modifier: u64,
    /// The planes, 1 to 4, as the format has them.
    pub planes: Vec<DmabufPlane>,
    /// Whether the color is premultiplied by its alpha (the default, as GTK's).
    pub premultiplied: bool,
    /// Runs once nothing reads the buffer any more (GTK freed its
    /// texture, a newer frame replaced it before it was drawn, no video
    /// wanted it, or it could not be shown), on any thread, exactly once:
    /// the host keeps the planes' descriptors open and their pixels
    /// unchanged until then. `None` when the host needs no word.
    pub release: Option<DmabufRelease>,
}

impl Default for DmabufFrame {
    fn default() -> Self {
        DmabufFrame {
            width: 0,
            height: 0,
            fourcc: 0,
            modifier: 0,
            planes: Vec::new(),
            premultiplied: true,
            release: None,
        }
    }
}

impl std::fmt::Debug for DmabufFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DmabufFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("fourcc", &format_args!("{:#010x}", self.fourcc))
            .field("modifier", &format_args!("{:#x}", self.modifier))
            .field("planes", &self.planes)
            .field("premultiplied", &self.premultiplied)
            .field("release", &self.release.is_some())
            .finish()
    }
}

/// The most planes a DRM format has.
const MAX_PLANES: usize = 4;

/// Why `frame` cannot be shown before GTK sees it, if it cannot.
fn check_dmabuf(frame: &DmabufFrame) -> Result<(), &'static str> {
    if frame.width == 0 || frame.height == 0 {
        return Err("the size is empty");
    }
    if frame.width > i32::MAX as u32 || frame.height > i32::MAX as u32 {
        return Err("the size is too large");
    }
    if frame.planes.is_empty() || frame.planes.len() > MAX_PLANES {
        return Err("it has no planes, or more than 4");
    }
    if frame.planes.iter().any(|plane| plane.fd < 0) {
        return Err("a plane has no file descriptor");
    }
    if frame.planes.iter().any(|plane| plane.stride == 0) {
        return Err("a plane's stride is 0");
    }
    Ok(())
}

/// A frame's release, run once by whoever is first: the texture's destroy
/// notify, the frame dropped without a texture, or a refused build.
#[derive(Clone)]
struct ReleaseOnce(Arc<Mutex<Option<DmabufRelease>>>);

impl ReleaseOnce {
    fn new(release: Option<DmabufRelease>) -> Self {
        ReleaseOnce(Arc::new(Mutex::new(release)))
    }

    fn run(&self) {
        let release = self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(release) = release {
            release();
        }
    }
}

/// What a registration knows about its dmabufs: the formats GTK refused
/// (their frames are released at once, not built again) and whether a
/// wrong frame was reported.
struct DmabufState {
    name: Arc<str>,
    warned: AtomicBool,
    refused: Mutex<Vec<(u32, u64)>>,
}

impl DmabufState {
    fn is_refused(&self, fourcc: u32, modifier: u64) -> bool {
        self.refused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&(fourcc, modifier))
    }

    /// Remembers a refused format; `true` the first time.
    fn refuse(&self, fourcc: u32, modifier: u64) -> bool {
        let mut refused = self.refused.lock().unwrap_or_else(|e| e.into_inner());
        if refused.contains(&(fourcc, modifier)) {
            return false;
        }
        refused.push((fourcc, modifier));
        true
    }
}

/// A dmabuf frame on its way to the videos: its texture is built once, on
/// the main thread, at the first draw.
struct Dmabuf {
    frame: DmabufFrame,
    release: ReleaseOnce,
    texture: OnceLock<Option<gdk::Texture>>,
    state: Arc<DmabufState>,
}

impl Dmabuf {
    fn texture(&self) -> Option<gdk::Texture> {
        self.texture.get_or_init(|| self.build()).clone()
    }

    /// The texture; `None` (released, logged once per format) when GTK
    /// refuses the buffer.
    fn build(&self) -> Option<gdk::Texture> {
        let frame = &self.frame;
        let Some(display) = gdk::Display::default() else {
            self.release.run();
            return None;
        };
        let mut builder = gdk::DmabufTextureBuilder::new()
            .set_display(&display)
            .set_width(frame.width)
            .set_height(frame.height)
            .set_fourcc(frame.fourcc)
            .set_modifier(frame.modifier)
            .set_premultiplied(frame.premultiplied)
            .set_n_planes(frame.planes.len() as u32);
        for (i, plane) in frame.planes.iter().enumerate() {
            let i = i as u32;
            // SAFETY: the host keeps the descriptor open until the frame's
            // release runs, which only the texture's destroy notify, the
            // frame dropped without a texture or a refused build call.
            builder = unsafe { builder.set_fd(i, plane.fd) }
                .set_offset(i, plane.offset)
                .set_stride(i, plane.stride);
        }
        let release = self.release.clone();
        // SAFETY: as above: the descriptors stay open until `release` runs.
        match unsafe { builder.build_with_release_func(move || release.run()) } {
            Ok(texture) => Some(texture),
            Err(err) => {
                // GTK may not call the destroy notify of a texture it did
                // not make: the release runs here (once, whoever is first).
                self.release.run();
                if self.state.refuse(frame.fourcc, frame.modifier) {
                    log::warn!(
                        "stream `{}`: a dmabuf of format {:#010x} (modifier {:#x}) shows nothing: {err}",
                        self.state.name,
                        frame.fourcc,
                        frame.modifier
                    );
                }
                None
            }
        }
    }
}

impl Drop for Dmabuf {
    fn drop(&mut self) {
        // With a texture, its destroy notify releases the buffer once GTK
        // is done with it; without one, nothing reads the buffer now.
        if !matches!(self.texture.get(), Some(Some(_))) {
            self.release.run();
        }
    }
}

/// What a stream tells the videos showing it.
pub(crate) type StreamEvent<'a> = nsh_core::stream::StreamEvent<'a, Frame>;

/// A stream the embedding app registered ([`AppView::register_stream`]):
/// where it pushes the frames and the audio. Any thread. Dropping it ends
/// the stream.
///
/// [`AppView::register_stream`]: crate::AppView::register_stream
pub struct StreamHandle {
    inner: nsh_core::stream::StreamHandle<Frame>,
    dmabuf: Arc<DmabufState>,
}

impl std::fmt::Debug for StreamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.inner, f)
    }
}

impl StreamHandle {
    fn new(inner: nsh_core::stream::StreamHandle<Frame>) -> Self {
        let dmabuf = Arc::new(DmabufState {
            name: inner.name().into(),
            warned: AtomicBool::new(false),
            refused: Mutex::new(Vec::new()),
        });
        StreamHandle { inner, dmabuf }
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
    /// bytes per row (at least `width * 4`). Copies the bytes. A frame that
    /// comes before the previous one was drawn replaces it. A frame of a
    /// size no frame can have (zero, a stride shorter than its rows, fewer
    /// bytes than its rows) shows nothing, logged once per stream.
    pub fn push_bgra(&self, width: usize, height: usize, stride: usize, bytes: &[u8]) {
        self.inner
            .push_bgra(width, height, stride, bytes, Frame::make);
    }

    /// Shows a dmabuf in the videos playing the stream, without a copy:
    /// GTK draws it as a `gdk::DmabufTexture` the videos share. The rules
    /// are [`Self::push_bgra`]'s: a frame that comes before the previous
    /// one was drawn replaces it, `paused` videos keep their last frame,
    /// the first frame's size is the videos' `onLoad`. The frame's
    /// [`DmabufFrame::release`] runs once nothing reads the buffer any
    /// more; at once when no video wants the frame ([`Self::wanted`]). A
    /// frame no frame can be (no size, no planes or more than 4, a plane
    /// without a descriptor or a stride) and one in a format GTK cannot
    /// show (`gdk::DmabufTextureBuilder` refuses it; later frames of that
    /// format are released without trying again) show nothing, logged once
    /// per stream.
    pub fn push_dmabuf(&self, mut frame: DmabufFrame) {
        let release = ReleaseOnce::new(frame.release.take());
        if let Err(what) = check_dmabuf(&frame) {
            if !self.dmabuf.warned.swap(true, Ordering::AcqRel) {
                log::warn!(
                    "stream `{}`: a dmabuf of {}×{} pixels with {} planes shows nothing: {what}",
                    self.dmabuf.name,
                    frame.width,
                    frame.height,
                    frame.planes.len()
                );
            }
            release.run();
            return;
        }
        if !self.inner.wanted() || self.dmabuf.is_refused(frame.fourcc, frame.modifier) {
            release.run();
            return;
        }
        let (width, height) = (frame.width as usize, frame.height as usize);
        // Dropped unseen (replaced, no longer wanted, ended), it releases
        // the buffer.
        self.inner.push(Frame {
            width,
            height,
            pixels: Pixels::Dmabuf(Dmabuf {
                frame,
                release,
                texture: OnceLock::new(),
                state: self.dmabuf.clone(),
            }),
        });
    }

    /// Plays `samples` in the videos playing the stream: interleaved `f32`
    /// in -1 to 1, `channels` per frame (1 to 8), `sample_rate` frames a
    /// second (8 000 to 192 000). They go from this thread straight to each
    /// video's output, which queues at most about 200 ms of them (newer
    /// samples are dropped while it is full); while no video plays the
    /// stream the call costs nothing. A change of channels or rate is
    /// followed. Samples of the wrong format are logged once per stream
    /// and dropped.
    pub fn push_pcm(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        self.inner.push_pcm(samples, channels, sample_rate);
    }

    /// Whether audio pushed now would be heard: the registration is live
    /// and a video plays the stream. A host that decodes or resamples its
    /// audio at a cost may skip it while this is `false`.
    pub fn wants_audio(&self) -> bool {
        self.inner.wants_audio()
    }

    /// Ends the stream: the videos playing it show nothing from now on,
    /// until the name is registered again; their queued audio plays out.
    pub fn end(&self) {
        self.inner.end();
    }
}

/// A video's attachment to a stream, by name ([`Streams::attach`]);
/// dropping it detaches the video's sink. Main thread.
pub(crate) struct Attached(nsh_core::stream::Attached<Frame>);

impl Attached {
    /// The stream's name.
    pub(crate) fn name(&self) -> &Arc<str> {
        self.0.name()
    }

    /// Stops or resumes telling the sink the frames (the last one stays; a
    /// sink with none yet still gets the first).
    pub(crate) fn set_paused(&self, paused: bool) {
        self.0.set_paused(paused);
    }
}

/// The streams of one surface, by name (shared with its videos, so a
/// registration applies to videos showing the name from then on, and to
/// those already showing it). Main thread.
#[derive(Clone)]
pub(crate) struct Streams(nsh_core::stream::Streams<Frame>);

impl Default for Streams {
    fn default() -> Self {
        Streams(nsh_core::stream::Streams::new(
            |job: Box<dyn FnOnce() + Send>| {
                glib::MainContext::default().invoke(job);
                true
            },
        ))
    }
}

impl std::fmt::Debug for Streams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl Streams {
    /// Registers stream `name` (replacing an earlier registration): the
    /// handle takes its frames and audio. The videos whose `srcObject`
    /// names it, now or later, play it. A name under `nsh/` is NSH's own:
    /// refused (logged), the handle plays nothing.
    pub(crate) fn register(&self, name: &str) -> StreamHandle {
        StreamHandle::new(self.0.register(name))
    }

    /// [`Self::register`] without the check of reserved names: NSH's own
    /// hosts register `nsh/` names with it (`nsh/test-pattern`).
    pub(crate) fn register_reserved(&self, name: &str) -> StreamHandle {
        StreamHandle::new(self.0.register_reserved(name))
    }

    /// Attaches `sink`, a video's, to stream `name`: it hears the frames
    /// and the end. A name nothing registered is logged once; the sink
    /// hears frames once something is.
    pub(crate) fn attach(&self, name: &str, sink: Box<dyn Fn(StreamEvent<'_>)>) -> Attached {
        Attached(self.0.attach(name, sink))
    }

    /// Attaches `output`, a video's, to stream `name`'s audio, before or
    /// after its registration; dropping the attachment detaches it.
    pub(crate) fn attach_audio(&self, name: &str, output: Arc<dyn AudioOutput>) -> AudioAttachment {
        self.0.attach_audio(name, output)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    /// A registry whose hop never runs its jobs: no job reaches GLib's
    /// default main context, which the GTK tests' thread owns.
    fn unhopped() -> Streams {
        Streams(nsh_core::stream::Streams::new(
            |_: Box<dyn FnOnce() + Send>| false,
        ))
    }

    #[test]
    fn a_frame_keeps_its_size_and_rows_of_bytes() {
        // Three rows of a 12-byte stride, with 8 bytes of slack behind.
        let bytes = vec![7u8; 12 * 3 + 8];
        let frame = Frame::make(2, 3, 12, &bytes).unwrap();
        assert_eq!((frame.width, frame.height), (2, 3));
        let Pixels::Memory { stride, bytes } = &frame.pixels else {
            panic!("a frame of bytes");
        };
        assert_eq!(*stride, 12);
        assert_eq!(bytes.len(), 36);
        assert!(bytes.iter().all(|b| *b == 7));
    }

    // Registering and attaching need no main loop: a push would hop to
    // GLib's default main context, so none is made here.
    #[test]
    fn registers_and_attaches_by_name() {
        let streams = Streams::default();
        let attached = streams.attach("cam", Box::new(|_: StreamEvent<'_>| {}));
        let handle = streams.register("cam");
        assert_eq!(handle.name(), "cam");
        assert_eq!(&**attached.name(), "cam");
        assert_eq!(format!("{streams:?}"), r#"{"cam"}"#);
        assert_eq!(format!("{handle:?}"), r#"StreamHandle { name: "cam" }"#);
    }

    #[test]
    fn nsh_names_are_registered_only_as_reserved() {
        let streams = unhopped();
        let _attached = streams.attach("nsh/test-pattern", Box::new(|_: StreamEvent<'_>| {}));
        assert!(!streams.register("nsh/test-pattern").wanted());
        assert!(streams.register_reserved("nsh/test-pattern").wanted());
    }

    /// A frame of one linear XRGB8888 plane whose release counts in `count`.
    fn dmabuf(count: &Arc<AtomicUsize>) -> DmabufFrame {
        let count = count.clone();
        DmabufFrame {
            width: 4,
            height: 2,
            fourcc: u32::from_le_bytes(*b"XR24"),
            planes: vec![DmabufPlane {
                fd: 3,
                offset: 0,
                stride: 16,
            }],
            release: Some(Box::new(move || {
                count.fetch_add(1, Ordering::Relaxed);
            })),
            ..DmabufFrame::default()
        }
    }

    #[test]
    fn checks_dmabufs() {
        let count = Arc::new(AtomicUsize::new(0));
        assert_eq!(check_dmabuf(&dmabuf(&count)), Ok(()));
        let plane = DmabufPlane {
            fd: 3,
            offset: 0,
            stride: 16,
        };
        let wrong = [
            DmabufFrame {
                width: 0,
                ..dmabuf(&count)
            },
            DmabufFrame {
                height: u32::MAX,
                ..dmabuf(&count)
            },
            DmabufFrame {
                planes: Vec::new(),
                ..dmabuf(&count)
            },
            DmabufFrame {
                planes: vec![DmabufPlane::default()],
                ..dmabuf(&count)
            },
            DmabufFrame {
                planes: vec![DmabufPlane { stride: 0, ..plane }],
                ..dmabuf(&count)
            },
            DmabufFrame {
                planes: vec![plane; 5],
                ..dmabuf(&count)
            },
        ];
        assert!(wrong.iter().all(|frame| check_dmabuf(frame).is_err()));
        // A plane's defaults name no descriptor; a frame's are premultiplied.
        assert_eq!(DmabufPlane::default().fd, -1);
        assert!(DmabufFrame::default().premultiplied);
        // Checking never releases: the push does, once.
        assert_eq!(count.load(Ordering::Relaxed), 0);
    }

    // No video wants the frames, a frame is wrong, a format was refused, a
    // frame is dropped before it was drawn: each release runs once, at
    // once. Nothing here reaches the main context (no push is wanted).
    #[test]
    fn dmabufs_nobody_shows_are_released_once() {
        let streams = unhopped();
        let handle = streams.register("cam");
        let count = Arc::new(AtomicUsize::new(0));
        assert!(!handle.wanted());
        handle.push_dmabuf(dmabuf(&count));
        assert_eq!(count.load(Ordering::Relaxed), 1);
        handle.push_dmabuf(DmabufFrame {
            planes: Vec::new(),
            ..dmabuf(&count)
        });
        assert_eq!(count.load(Ordering::Relaxed), 2);
        assert!(handle.dmabuf.warned.load(Ordering::Acquire));
        // Wanted, but in a format GTK refused before: released, not pushed.
        let _attached = streams.attach("cam", Box::new(|_: StreamEvent<'_>| {}));
        assert!(handle.wanted());
        let refused = dmabuf(&count);
        assert!(handle.dmabuf.refuse(refused.fourcc, refused.modifier));
        assert!(!handle.dmabuf.refuse(refused.fourcc, refused.modifier));
        handle.push_dmabuf(refused);
        assert_eq!(count.load(Ordering::Relaxed), 3);
        let mut frame = dmabuf(&count);
        let release = ReleaseOnce::new(frame.release.take());
        drop(Dmabuf {
            frame,
            release: release.clone(),
            texture: OnceLock::new(),
            state: handle.dmabuf.clone(),
        });
        release.run();
        assert_eq!(count.load(Ordering::Relaxed), 4);
    }
}
