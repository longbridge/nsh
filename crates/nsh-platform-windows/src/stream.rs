//! Live streams the embedding app registers by name, which `<Video
//! srcObject>` plays (`HostStream.fromHost(name)`, `VideoProps::stream`):
//! the host pushes frames and audio from any thread, and each video showing
//! the stream draws the frames on the UI thread ([`crate::native`]'s
//! `video`) and plays the audio. Neither passes through JS.
//!
//! The rules every backend shares (one entry per name whichever side comes
//! first, the newest frame wins, paused sinks, the end of a registration,
//! a frame that does not fit its size, a name nothing registered, audio
//! straight from the pushing thread, reserved names) live in
//! [`nsh_core::stream`]; this module is the Windows side of it. A frame
//! ([`Frame`]) is BGRA bytes copied into a `Vec`
//! ([`StreamHandle::push_bgra`]) or a Direct3D 11 texture
//! ([`StreamHandle::push_d3d11_texture`]): one of the renderer's device is
//! drawn as it is, one shared from another device is opened on the
//! renderer's and copied on the GPU, and only one that is neither is read
//! back through the CPU ([`texture`]). The sinks hear the frames on the UI
//! thread, reached with [`UiThread::invoke`]; the UI thread is captured when
//! the first video attaches (a registry may be made before its window's
//! thread runs its message loop), and so is the renderer's device, which
//! the pushing thread compares a texture's device with. Each video's audio
//! is an XAudio2 voice of its own ([`audio::StreamAudio`]).

mod audio;
mod texture;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) use audio::StreamAudio;
pub(crate) use texture::acquire;
use windows::Win32::Graphics::Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11Texture2D};

use crate::{gfx, window::UiThread};

/// One frame of a stream: `width` × `height` BGRA pixels (8 bits each,
/// premultiplied or opaque).
pub(crate) struct Frame {
    /// Pixels.
    pub(crate) width: usize,
    /// Pixels.
    pub(crate) height: usize,
    pub(crate) pixels: Pixels,
}

/// Where a frame's pixels are.
pub(crate) enum Pixels {
    /// `height` rows of `stride` bytes (at least `width * 4`).
    Bytes { stride: usize, bytes: Vec<u8> },
    /// A texture of the renderer's device that a shader can read: drawn as
    /// it is.
    Texture(ID3D11Texture2D),
    /// A texture of the renderer's device, copied on the GPU into the
    /// video's own; `keyed`, its keyed mutex's key 0 is held for the copy.
    Copy {
        texture: ID3D11Texture2D,
        keyed: bool,
    },
}

/// What a video attached to a stream hears, on the UI thread.
pub(crate) type StreamEvent<'a> = nsh_core::stream::StreamEvent<'a, Frame>;

/// The UI thread the sinks live on and the renderer's device there, shared
/// with the registry's handles.
#[derive(Clone, Default)]
struct UiHop {
    ui: Arc<Mutex<Option<UiThread>>>,
    renderer: Renderer,
}

impl UiHop {
    /// Makes the calling thread the sinks' thread, if none is yet.
    fn capture(&self) {
        self.ui
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(UiThread::current);
    }

    /// The registry's hop ([`nsh_core::stream::Hop`]): runs `job` on the
    /// UI thread; `false` while no video attached yet, or when the thread
    /// cannot take it (the registry tries again with the next push).
    fn post(&self, job: Box<dyn FnOnce() + Send>) -> bool {
        let ui = *self.ui.lock().unwrap_or_else(|e| e.into_inner());
        ui.is_some_and(|ui| ui.invoke(job))
    }
}

/// The Direct3D device the UI thread draws with, as last seen there (a
/// device loss replaces it): what a pushed texture's device is compared
/// with.
#[derive(Clone, Default)]
pub(crate) struct Renderer(Arc<Mutex<Option<ID3D11Device>>>);

impl Renderer {
    /// Notes the UI thread's device (UI thread).
    pub(crate) fn note(&self, device: &ID3D11Device) {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if slot.as_ref() != Some(device) {
            *slot = Some(device.clone());
        }
    }

    fn get(&self) -> Option<ID3D11Device> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// A stream the embedding app registered ([`crate::AppView::register_stream`]):
/// where it pushes the frames and the audio. Any thread. Dropping it ends
/// the stream.
pub struct StreamHandle {
    inner: nsh_core::stream::StreamHandle<Frame>,
    renderer: Renderer,
    textures: texture::Textures,
    /// A texture that cannot be a frame was reported.
    texture_warned: AtomicBool,
    /// The copy through the CPU was reported.
    copy_warned: AtomicBool,
}

impl std::fmt::Debug for StreamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamHandle")
            .field("name", &self.inner.name())
            .finish()
    }
}

impl StreamHandle {
    fn new(inner: nsh_core::stream::StreamHandle<Frame>, renderer: Renderer) -> StreamHandle {
        StreamHandle {
            inner,
            renderer,
            textures: texture::Textures::default(),
            texture_warned: AtomicBool::new(false),
            copy_warned: AtomicBool::new(false),
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
    /// that does not add up (`bytes` shorter than `stride * height`, a
    /// stride under `width * 4`, an empty size) is logged once and shows
    /// nothing.
    pub fn push_bgra(&self, width: usize, height: usize, stride: usize, bytes: &[u8]) {
        self.inner.push_bgra(
            width,
            height,
            stride,
            bytes,
            |width, height, stride, bytes| {
                Some(Frame {
                    width,
                    height,
                    pixels: Pixels::Bytes {
                        stride,
                        bytes: bytes[..stride * height].to_vec(),
                    },
                })
            },
        );
    }

    /// Shows a Direct3D 11 texture in the videos playing the stream, as
    /// [`Self::push_bgra`] shows bytes (the newest wins, a paused video
    /// keeps its last frame, the first frame's size is `onLoad`):
    /// `DXGI_FORMAT_B8G8R8A8_UNORM`, not multisampled, its first mip level
    /// and array slice the picture (opaque or premultiplied).
    ///
    /// How it reaches the screen depends on its device:
    /// - Made on the renderer's device ([`crate::AppView::d3d11_device`])
    ///   with `D3D11_BIND_SHADER_RESOURCE`, one mip level and one slice, it
    ///   is drawn as it is, without a copy: the video holds it until a
    ///   later frame replaces it, so draw each frame into a texture the
    ///   video does not show (a ring of three is enough). On that device
    ///   without the bind flag, or with a keyed mutex (`D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX`:
    ///   the copy takes key 0, as below), it is copied on the GPU.
    /// - Made on another device with `D3D11_RESOURCE_MISC_SHARED` or
    ///   `D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX` (a legacy shared handle),
    ///   it is opened on the renderer's device and copied on the GPU when
    ///   the frame is drawn. With a keyed mutex the copy takes key 0 and
    ///   gives it back: release key 0 once the frame is drawn
    ///   (`ReleaseSync(0)`) and push it; a frame whose key is held at the
    ///   copy is skipped. Without one, flush the device's context before
    ///   the push.
    /// - Anything else (another device, not shared, or shared only by an
    ///   NT handle) is read back through the CPU on this thread with the
    ///   texture's device's immediate context (logged once): push from the
    ///   thread that uses that context, or turn on its multithread
    ///   protection.
    ///
    /// A texture of another format, multisampled or empty is logged once
    /// and shows nothing. While no video shows the stream ([`Self::wanted`])
    /// the call costs nothing.
    pub fn push_d3d11_texture(&self, texture: &ID3D11Texture2D) {
        if !self.inner.wanted() {
            return;
        }
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: a call on a live texture with an out pointer to a local.
        unsafe { texture.GetDesc(&mut desc) };
        if let Err(what) = texture::check(&desc) {
            if !self.texture_warned.swap(true, Ordering::AcqRel) {
                log::warn!(
                    "stream `{}`: a texture of {}×{} pixels shows nothing: {what}",
                    self.inner.name(),
                    desc.Width,
                    desc.Height
                );
            }
            return;
        }
        let renderer = self.renderer.get();
        // SAFETY: a call on a live texture.
        let device = unsafe { texture.GetDevice() }.ok();
        let same = matches!((&renderer, &device), (Some(r), Some(d)) if texture::same_object(r, d));
        let pixels = match texture::route(same, &desc) {
            texture::Route::Direct => Some(Pixels::Texture(texture.clone())),
            texture::Route::Copy => Some(Pixels::Copy {
                texture: texture.clone(),
                keyed: texture::keyed(&desc),
            }),
            texture::Route::Shared => renderer
                .as_ref()
                .and_then(|r| self.textures.open(r, texture).ok())
                .map(|opened| Pixels::Copy {
                    texture: opened,
                    keyed: texture::keyed(&desc),
                }),
            texture::Route::Cpu => None,
        };
        let pixels = match pixels {
            Some(pixels) => pixels,
            None => {
                if !self.copy_warned.swap(true, Ordering::AcqRel) {
                    log::warn!(
                        "stream `{}`: a texture neither of the renderer's device nor shared with \
                         it is copied through the CPU",
                        self.inner.name()
                    );
                }
                match self.textures.read(texture, &desc) {
                    Ok((stride, bytes)) => Pixels::Bytes { stride, bytes },
                    Err(err) => {
                        log::debug!("stream `{}`: texture not read: {err}", self.inner.name());
                        return;
                    }
                }
            }
        };
        self.inner.push(Frame {
            width: desc.Width as usize,
            height: desc.Height as usize,
            pixels,
        });
    }

    /// Plays audio in the videos playing the stream: interleaved `f32`
    /// samples in -1 to 1, `channels` per frame (1 to 8; of more than two,
    /// the front pair plays), `sample_rate` frames a second (8 000 to
    /// 192 000). Each video queues them for its own XAudio2 voice on this
    /// thread, in order; past about 200 ms queued, newer samples are
    /// dropped rather than played late. A paused or muted video drops
    /// them, `volume` scales them. Samples of the wrong format are logged
    /// once and dropped; while no video shows the stream the call costs
    /// nothing.
    pub fn push_pcm(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        self.inner.push_pcm(samples, channels, sample_rate);
    }

    /// Whether audio pushed now would be heard: the registration is live
    /// and a video plays the stream. A host that decodes or resamples its
    /// audio at a cost may skip it while this is `false`.
    pub fn wants_audio(&self) -> bool {
        self.inner.wants_audio()
    }

    /// Ends the stream: the videos playing it show nothing and their audio
    /// stops from now on, until the name is registered again.
    pub fn end(&self) {
        self.inner.end();
    }
}

/// A video's attachment to a stream, by name ([`Streams::attach`]);
/// dropping it detaches the video's sink.
pub(crate) struct Attached(nsh_core::stream::Attached<Frame>);

impl Attached {
    /// The stream's name.
    pub(crate) fn name(&self) -> &str {
        self.0.name()
    }

    /// Stops or resumes delivering frames to the video's sink (the last
    /// one it drew stays; with none drawn yet, the first to come is).
    pub(crate) fn set_paused(&self, paused: bool) {
        self.0.set_paused(paused);
    }
}

/// A video's audio output attached to a stream ([`Streams::attach_audio`]);
/// dropping it detaches the output and stops it at once.
pub(crate) struct AudioAttached {
    output: Arc<StreamAudio>,
    _attachment: nsh_core::stream::AudioAttachment,
}

impl AudioAttached {
    /// Applies the video's `paused`, `muted` and `volume`.
    pub(crate) fn set(&self, paused: bool, muted: bool, volume: f32) {
        self.output.set(paused, muted, volume);
    }
}

impl Drop for AudioAttached {
    fn drop(&mut self) {
        // A push may still hold the output for a moment: it plays nothing.
        self.output.close();
    }
}

/// The streams of one registry ([`crate::Components`]), by name (shared
/// with its videos, so a registration applies to videos showing the name
/// from then on, and to those already showing it). One thread's: the UI
/// thread of the surfaces made with the registry.
#[derive(Clone)]
pub(crate) struct Streams {
    inner: nsh_core::stream::Streams<Frame>,
    ui: UiHop,
}

impl Default for Streams {
    fn default() -> Self {
        let ui = UiHop::default();
        let hop = ui.clone();
        Streams {
            inner: nsh_core::stream::Streams::new(move |job: Box<dyn FnOnce() + Send>| {
                hop.post(job)
            }),
            ui,
        }
    }
}

impl std::fmt::Debug for Streams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.inner, f)
    }
}

impl Streams {
    /// Registers stream `name` (replacing an earlier registration): the
    /// handle takes its frames and audio. The videos whose `srcObject`
    /// names it, now or later, play it. A name under `nsh/` is NSH's own:
    /// refused (logged), the handle plays nothing.
    pub(crate) fn register(&self, name: &str) -> StreamHandle {
        StreamHandle::new(self.inner.register(name), self.ui.renderer.clone())
    }

    /// [`Self::register`] for NSH's own names (`nsh/test-pattern`).
    pub(crate) fn register_reserved(&self, name: &str) -> StreamHandle {
        StreamHandle::new(self.inner.register_reserved(name), self.ui.renderer.clone())
    }

    /// The renderer's device, which the UI thread notes.
    pub(crate) fn renderer(&self) -> &Renderer {
        &self.ui.renderer
    }

    /// Attaches `sink`, a video's, to stream `name`: it hears the frames
    /// and the end on this thread, the UI thread of the video's surface. A
    /// name nothing registered is logged once; the sink hears once
    /// something is.
    pub(crate) fn attach(&self, name: &str, sink: Box<dyn Fn(StreamEvent<'_>)>) -> Attached {
        self.ui.capture();
        if let Some(gfx) = gfx::current() {
            self.ui.renderer.note(&gfx.d3d);
        }
        Attached(self.inner.attach(name, sink))
    }

    /// Attaches a video's audio output to stream `name`, with the video's
    /// settings.
    pub(crate) fn attach_audio(
        &self,
        name: &str,
        paused: bool,
        muted: bool,
        volume: f32,
    ) -> AudioAttached {
        let output = Arc::new(StreamAudio::new(paused, muted, volume));
        let attachment = self.inner.attach_audio(name, output.clone());
        AudioAttached {
            output,
            _attachment: attachment,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;

    #[test]
    fn a_frame_is_the_bytes_of_stride_times_height() {
        let frames = Rc::new(RefCell::new(Vec::new()));
        let seen = frames.clone();
        let streams = nsh_core::stream::Streams::<Frame>::new(|job: Box<dyn FnOnce() + Send>| {
            job();
            true
        });
        let _attached = streams.attach(
            "cam",
            Box::new(move |event: StreamEvent<'_>| {
                if let StreamEvent::Frame { frame, .. } = event
                    && let Pixels::Bytes { stride, bytes } = &frame.pixels
                {
                    seen.borrow_mut()
                        .push((frame.width, frame.height, *stride, bytes.clone()));
                }
            }),
        );
        let handle = StreamHandle::new(streams.register("cam"), Renderer::default());
        // A padded stride, and more bytes than the frame: exactly stride ×
        // height of them are copied.
        let mut bytes = vec![0; 40];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = i as u8;
        }
        handle.push_bgra(2, 2, 12, &bytes);
        let frames = frames.borrow();
        assert_eq!(frames.len(), 1);
        let (width, height, stride, copied) = &frames[0];
        assert_eq!((*width, *height, *stride), (2, 2, 12));
        assert_eq!(copied.len(), 24);
        assert_eq!(&copied[..], &bytes[..24]);
    }

    #[test]
    fn a_registry_registers_and_attaches_without_a_message_loop() {
        let streams = Streams::default();
        let attached = streams.attach("cam", Box::new(|_| {}));
        assert_eq!(attached.name(), "cam");
        let handle = streams.register("cam");
        assert_eq!(handle.name(), "cam");
        assert!(handle.wanted());
        assert_eq!(format!("{streams:?}"), r#"{"cam"}"#);
        assert_eq!(format!("{handle:?}"), r#"StreamHandle { name: "cam" }"#);
        // Delivered through the UI thread's hop, which needs a message
        // loop: here it only must not panic.
        handle.push_bgra(1, 1, 4, &[1, 2, 3, 255]);
        attached.set_paused(true);
        handle.end();
        assert!(!handle.wanted());
    }

    #[test]
    fn reserved_names_take_the_reserved_registration() {
        let streams = Streams::default();
        let _attached = streams.attach("nsh/test-pattern", Box::new(|_| {}));
        // The embedding app's registration is refused: nothing wants it.
        let refused = streams.register("nsh/test-pattern");
        assert!(!refused.wanted());
        let reserved = streams.register_reserved("nsh/test-pattern");
        assert!(reserved.wanted());
    }

    #[test]
    fn audio_goes_to_the_videos_playing_the_stream() {
        let streams = Streams::default();
        let handle = streams.register("mic");
        assert!(!handle.wants_audio());
        // Paused: attached, so wanted, but it opens no device.
        let audio = streams.attach_audio("mic", true, false, 1.0);
        assert!(handle.wants_audio());
        handle.push_pcm(&[0.0; 4], 2, 48_000);
        audio.set(true, true, 0.5);
        drop(audio);
        assert!(!handle.wants_audio());
        handle.end();
    }
}
