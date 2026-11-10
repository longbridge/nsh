//! Live streams without a toolkit: the registry the native backends share
//! for `<Video srcObject={HostStream.fromHost(name)}>` (`VideoProps::stream`).
//! The embedding app registers a stream by name and pushes frames from any
//! thread ([`StreamHandle`]); each video naming the stream attaches a sink
//! ([`Streams::attach`]) that draws them on the main thread. The frames
//! never pass through JS.
//!
//! What every backend does the same way lives here: a name is one entry
//! whether the host registered it or a video asked for it first (a video
//! attached before the registration shows the stream once it comes); a name
//! nothing registered is logged once, as an unknown native component is;
//! the newest frame wins (a frame pushed before the previous one was drawn
//! replaces it, so a slow main thread drops frames rather than queueing
//! them), delivered with one hop to the main thread per batch; `paused`
//! sinks keep their last frame, but one with no frame yet still takes the
//! first that comes, as a paused `<video>` shows its first frame; the end of
//! a registration blanks the videos until the name is registered again
//! (registering a name again replaces the earlier registration: its handle
//! pushes nothing, and dropping or ending it leaves the new one playing); a
//! frame whose bytes do not fit its size is logged once and shows nothing;
//! and while no video shows the stream a push costs nothing: the bytes are
//! not copied and the main thread is not woken.
//!
//! A backend gives the frame type `F` it draws (a `CVPixelBuffer`, a
//! `Vec<u8>`, a `glib::Bytes`, a GPUI `RenderImage`), made once per push on
//! the pushing thread, and the [`Hop`] to its main thread. The sinks are
//! the backend's closures and need not be `Send`: they are only ever touched
//! on the thread that made the registry (attach, detach, pause, deliver),
//! which [`MainThreadCell`] holds them to.
//!
//! **GPU frames.** A frame need not pass through bytes: a backend that takes
//! the host's native GPU buffers (an `IOSurface`-backed `CVPixelBuffer`, a
//! D3D11 texture, a dmabuf) wraps one in its `F` and pushes it with
//! [`StreamHandle::push`], without a copy. The registry never reads a
//! frame: its pixel size is `F`'s, which the backend's sink reads for the
//! video's `onLoad` (`StreamEvent::Frame::first`) as it does for frames
//! made by [`StreamHandle::push_bgra`]. Newest-wins, `paused`, the end and
//! [`StreamHandle::wanted`] behave the same for both; a backend checks
//! `wanted()` before wrapping a buffer that costs something to wrap, and
//! refuses (logs, does not push) a buffer with no pixels, which
//! `push_bgra` does for bytes.
//!
//! **Audio.** A registration also takes PCM audio
//! ([`StreamHandle::push_pcm`]: interleaved `f32` samples in -1 to 1). Audio
//! is not newest-wins and takes no hop: the samples go from the pushing
//! thread straight to every [`AudioOutput`] attached to the name
//! ([`Streams::attach_audio`], one per video, before or after the
//! registration), which queues them for the device. While no output is
//! attached a push costs nothing (no copy, no lock). Each output applies
//! its video's `paused`, `muted` and `volume` itself, set from the main
//! thread: paused or muted, it drops the samples; otherwise it scales them
//! by the volume. A push of the wrong format (channels outside 1 to 8, a
//! rate outside 8 to 192 kHz, a sample count that is no whole number of
//! frames) is logged once per stream and dropped. The end of a
//! registration reaches the outputs ([`AudioOutput::end`]).
//!
//! **Reserved names.** Names starting with `nsh/` are NSH's own (the
//! development hosts' test pattern, `nsh/test-pattern`):
//! [`Streams::register`] refuses them (logged, the handle plays nothing);
//! NSH's hosts register them with [`Streams::register_reserved`].

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    fmt,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread::ThreadId,
};

/// What a stream tells the videos showing it, on the main thread.
pub enum StreamEvent<'a, F> {
    /// A new frame to show.
    Frame {
        /// The frame.
        frame: &'a F,
        /// The sink's first frame since it attached or the stream last
        /// ended: the video's `onLoad`, with the frame's size.
        first: bool,
    },
    /// The registration ended: show nothing until another frame comes.
    Ended,
}

/// Hears a stream's events: a video's closure, called on the main thread.
pub type Sink<F> = Box<dyn Fn(StreamEvent<'_, F>)>;

/// One hop to the backend's main thread: runs `job` there, soon and in
/// order with other hops. `false` when it could not be queued (the thread
/// is gone, or not started): the job is dropped and the registry tries
/// again with the next push.
pub trait Hop: Send + Sync + 'static {
    /// Runs `job` on the main thread.
    fn invoke(&self, job: Box<dyn FnOnce() + Send>) -> bool;
}

impl<T: Fn(Box<dyn FnOnce() + Send>) -> bool + Send + Sync + 'static> Hop for T {
    fn invoke(&self, job: Box<dyn FnOnce() + Send>) -> bool {
        self(job)
    }
}

/// Where a stream's audio goes: a backend's output for one video (an audio
/// unit, a WASAPI client, a GStreamer source), attached with
/// [`Streams::attach_audio`].
///
/// Called on the pushing thread, never the main thread: `write` must not
/// block (it copies the samples into the output's own queue). The output
/// applies its video's `paused`, `muted` and `volume` itself, which the
/// backend sets from the main thread without waiting on the pushing
/// thread (opening the audio device happens outside anything the main
/// thread locks). Every backend keeps the same rules, so an app hears the
/// same on each platform:
/// - at most about 200 ms is queued: past it the newest samples are
///   dropped (an empty queue always takes a push);
/// - paused or muted, the samples are dropped and what is queued stops at
///   once;
/// - the volume (0 to 1) applies at once, to what is queued too;
/// - a failed output (no device, a refused format) logs once and stays
///   silent until [`AudioOutput::end`], after which it tries again.
pub trait AudioOutput: Send + Sync {
    /// Plays `samples`: interleaved `f32` in -1 to 1, `channels` per frame
    /// (1 to 8; `samples.len()` a multiple of it), `sample_rate` frames a
    /// second (8 000 to 192 000), as the host pushed them (checked).
    fn write(&self, samples: &[f32], channels: u16, sample_rate: u32);

    /// The registration ended: what is queued may play out, nothing more
    /// comes until the name is registered again.
    fn end(&self) {}
}

/// The outputs attached to a stream, each with its attachment's id.
type OutputList = Arc<[(u64, Arc<dyn AudioOutput>)]>;

/// The audio outputs of one stream, shared with their attachments. A
/// snapshot, swapped whole on attach and detach, so a push holds the lock
/// only to clone an `Arc`.
struct AudioOutputs {
    list: Mutex<OutputList>,
    /// How many outputs are attached: read without the lock, so a push to
    /// a stream nobody hears costs nothing.
    count: AtomicUsize,
}

impl Default for AudioOutputs {
    fn default() -> Self {
        AudioOutputs {
            list: Mutex::new(Arc::from(Vec::new())),
            count: AtomicUsize::new(0),
        }
    }
}

impl AudioOutputs {
    fn snapshot(&self) -> OutputList {
        self.list.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update(&self, f: impl FnOnce(&mut Vec<(u64, Arc<dyn AudioOutput>)>)) {
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = list.to_vec();
        f(&mut next);
        self.count.store(next.len(), Ordering::Release);
        *list = next.into();
    }
}

/// A video's audio output attached to a stream ([`Streams::attach_audio`]);
/// dropping it detaches the output (on any thread). It has no `Default`:
/// it is a handle to one attachment, made only by `attach_audio`.
#[non_exhaustive]
pub struct AudioAttachment {
    name: Arc<str>,
    key: u64,
    outputs: Arc<AudioOutputs>,
}

impl fmt::Debug for AudioAttachment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioAttachment")
            .field("name", &self.name)
            .finish()
    }
}

impl AudioAttachment {
    /// The stream's name.
    pub fn name(&self) -> &Arc<str> {
        &self.name
    }
}

impl Drop for AudioAttachment {
    fn drop(&mut self) {
        let key = self.key;
        self.outputs.update(|list| list.retain(|(k, _)| *k != key));
    }
}

/// Why `len` samples of `channels` channels at `sample_rate` frames a
/// second cannot be played, if they cannot ([`StreamHandle::push_pcm`]).
pub fn check_pcm(len: usize, channels: u16, sample_rate: u32) -> Result<(), &'static str> {
    if !(1..=8).contains(&channels) {
        return Err("the channels are not 1 to 8");
    }
    if !(8_000..=192_000).contains(&sample_rate) {
        return Err("the sample rate is not 8000 to 192000");
    }
    if !len.is_multiple_of(usize::from(channels)) {
        return Err("the samples are not a whole number of frames");
    }
    Ok(())
}

/// A value only the thread that made it touches (the main thread's sinks),
/// kept in a structure other threads share. Debug builds check the thread.
struct MainThreadCell<T> {
    thread: ThreadId,
    value: RefCell<T>,
}

// SAFETY: every access goes through `get`, which asserts the thread in
// debug builds; the registry's contract is that sinks are attached,
// detached, paused and told on the one thread that made the registry.
unsafe impl<T> Send for MainThreadCell<T> {}
// SAFETY: as above.
unsafe impl<T> Sync for MainThreadCell<T> {}

impl<T> MainThreadCell<T> {
    fn new(value: T) -> Self {
        MainThreadCell {
            thread: std::thread::current().id(),
            value: RefCell::new(value),
        }
    }

    fn get(&self) -> &RefCell<T> {
        debug_assert_eq!(
            std::thread::current().id(),
            self.thread,
            "a stream's sinks are the main thread's"
        );
        &self.value
    }
}

/// A video showing a stream: its sink and what it hears. The flags are
/// cells, so a sink may pause its own attachment from inside.
struct Attachment<F> {
    key: u64,
    /// `paused`: frames are dropped, the last one stays.
    paused: Cell<bool>,
    /// A frame was delivered since the registration started or last ended.
    loaded: Cell<bool>,
    sink: Sink<F>,
}

/// One stream by name, shared by its registration and the videos showing
/// it.
struct Shared<F> {
    name: Arc<str>,
    hop: Arc<dyn Hop>,
    /// The frame waiting to be delivered.
    pending: Mutex<Option<F>>,
    /// A delivery is queued on the main thread for `pending`.
    scheduled: AtomicBool,
    /// The live registration's number (frames may come), 0 for none: a
    /// handle acts only while it holds this number, so one replaced by a
    /// later registration neither pushes nor ends it.
    registration: AtomicU64,
    /// A wrong push was reported (once per stream).
    warned: AtomicBool,
    /// A wrong audio push was reported (once per stream).
    audio_warned: AtomicBool,
    /// The outputs hearing the stream's audio, any thread.
    audio: Arc<AudioOutputs>,
    /// The registration ended but the hop refused the job: the sinks hear
    /// the end with the next delivery that reaches the main thread.
    ended: AtomicBool,
    /// How many videos show the stream (readable off the main thread).
    attached: AtomicUsize,
    /// The videos showing the stream, main thread only.
    sinks: MainThreadCell<Vec<Rc<Attachment<F>>>>,
}

impl<F: Send + 'static> Shared<F> {
    fn new(name: Arc<str>, hop: Arc<dyn Hop>) -> Arc<Self> {
        Arc::new(Shared {
            name,
            hop,
            pending: Mutex::new(None),
            scheduled: AtomicBool::new(false),
            registration: AtomicU64::new(0),
            warned: AtomicBool::new(false),
            audio_warned: AtomicBool::new(false),
            audio: Arc::default(),
            ended: AtomicBool::new(false),
            attached: AtomicUsize::new(0),
            sinks: MainThreadCell::new(Vec::new()),
        })
    }

    /// Whether frames are wanted: a registration is live and a video shows
    /// the stream.
    fn wanted(&self) -> bool {
        self.registered() && self.attached.load(Ordering::Acquire) > 0
    }

    /// Whether a registration is live.
    fn registered(&self) -> bool {
        self.registration.load(Ordering::Acquire) != 0
    }

    /// Whether `registration` is the live one.
    fn is_live(&self, registration: u64) -> bool {
        self.registration.load(Ordering::Acquire) == registration
    }

    /// Keeps `frame` of `registration` for the next delivery, in place of
    /// one not delivered yet, and queues the delivery.
    fn push(self: &Arc<Self>, registration: u64, frame: F) {
        if !self.is_live(registration) || self.attached.load(Ordering::Acquire) == 0 {
            return;
        }
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        // Checked again under the lock `end` clears `pending` under: a
        // registration that ended after the check above has cleared it, or
        // clears it after this, so no frame shows after the end.
        if !self.is_live(registration) {
            return;
        }
        *pending = Some(frame);
        drop(pending);
        self.schedule();
    }

    /// One delivery on the main thread, unless one is on its way.
    fn schedule(self: &Arc<Self>) {
        if self.scheduled.swap(true, Ordering::AcqRel) {
            return;
        }
        let shared = self.clone();
        if !self.hop.invoke(Box::new(move || shared.deliver())) {
            self.scheduled.store(false, Ordering::Release);
        }
    }

    /// On the main thread: an end the hop refused earlier first, then the
    /// pending frame to every sink that is not paused (and to a paused one
    /// with no frame yet).
    fn deliver(&self) {
        self.scheduled.store(false, Ordering::Release);
        if self.ended.swap(false, Ordering::AcqRel) {
            self.tell_ended();
        }
        let frame = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let Some(frame) = frame else { return };
        // Cloned out, so a sink that attaches or detaches while told does
        // not meet the borrow.
        let sinks = self.sinks.get().borrow().clone();
        for attachment in sinks {
            if !attachment.paused.get() || !attachment.loaded.get() {
                let first = !attachment.loaded.replace(true);
                (attachment.sink)(StreamEvent::Frame {
                    frame: &frame,
                    first,
                });
            }
        }
    }

    /// On the main thread: the sinks hear the end.
    fn tell_ended(&self) {
        let sinks = self.sinks.get().borrow().clone();
        for attachment in sinks {
            attachment.loaded.set(false);
            (attachment.sink)(StreamEvent::Ended);
        }
    }

    /// The registration ended: the videos show nothing until another. When
    /// the hop cannot take the job (the main thread is not up), the end
    /// waits for the next delivery that reaches it.
    fn end(self: &Arc<Self>, registration: u64) {
        // Only the live registration ends: a handle replaced by a later
        // registration leaves it playing.
        if self
            .registration
            .compare_exchange(registration, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if self.audio.count.load(Ordering::Acquire) > 0 {
            for (_, output) in self.audio.snapshot().iter() {
                output.end();
            }
        }
        let shared = self.clone();
        if !self.hop.invoke(Box::new(move || shared.tell_ended())) {
            self.ended.store(true, Ordering::Release);
        }
    }
}

/// A stream the embedding app registered ([`Streams::register`]): where
/// it pushes the frames, from any thread. Dropping it ends the stream.
#[non_exhaustive]
pub struct StreamHandle<F: Send + 'static> {
    shared: Arc<Shared<F>>,
    /// Its registration's number ([`Shared::registration`]); never 0.
    registration: u64,
}

impl<F: Send + 'static> std::fmt::Debug for StreamHandle<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamHandle")
            .field("name", &self.shared.name)
            .finish()
    }
}

impl<F: Send + 'static> StreamHandle<F> {
    /// The name it was registered as.
    pub fn name(&self) -> &str {
        &self.shared.name
    }

    /// Whether a frame pushed now would show: a video shows the stream and
    /// the registration is live. A host that makes its frames at a cost
    /// may skip them while this is `false`.
    pub fn wanted(&self) -> bool {
        self.shared.is_live(self.registration) && self.shared.wanted()
    }

    /// Shows `frame` in the videos playing the stream (the newest wins):
    /// a frame the backend made, such as a native GPU buffer wrapped
    /// without a copy. The registry does not read it; the backend's sink
    /// takes its pixel size from it for the video's `onLoad`. A frame that
    /// costs something to make is best made only while [`Self::wanted`].
    pub fn push(&self, frame: F) {
        self.shared.push(self.registration, frame);
    }

    /// Plays `samples` in the videos playing the stream: interleaved `f32`
    /// in -1 to 1, `channels` per frame (1 to 8), `sample_rate` frames a
    /// second (8 000 to 192 000). They go from this thread straight to
    /// every attached [`AudioOutput`], in order, with no hop and nothing
    /// dropped for newer ones; while none is attached (or the registration
    /// ended) the call costs nothing. Samples of the wrong format (or a
    /// count that is no whole number of frames) are logged once per stream
    /// and dropped.
    pub fn push_pcm(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        let shared = &self.shared;
        if let Err(what) = check_pcm(samples.len(), channels, sample_rate) {
            if !shared.audio_warned.swap(true, Ordering::AcqRel) {
                log::warn!(
                    "stream `{}`: {} samples of {channels} channels at {sample_rate} Hz play \
                     nothing: {what}",
                    shared.name,
                    samples.len()
                );
            }
            return;
        }
        if samples.is_empty()
            || shared.audio.count.load(Ordering::Acquire) == 0
            || !shared.is_live(self.registration)
        {
            return;
        }
        for (_, output) in shared.audio.snapshot().iter() {
            output.write(samples, channels, sample_rate);
        }
    }

    /// Whether audio pushed now would be heard: the registration is live
    /// and an output is attached. A host that decodes or resamples its
    /// audio at a cost may skip it while this is `false`.
    pub fn wants_audio(&self) -> bool {
        self.shared.is_live(self.registration)
            && self.shared.audio.count.load(Ordering::Acquire) > 0
    }

    /// Shows a frame given as bytes: `width` × `height` pixels of BGRA (8
    /// bits each, opaque or premultiplied), `stride` bytes per row (at
    /// least `width * 4`), which `make` turns into the backend's frame
    /// (copying them) once the size is checked and a video wants it. A
    /// frame that does not fit its size (or has none) is logged once per
    /// stream and shows nothing; so does one `make` refuses.
    pub fn push_bgra(
        &self,
        width: usize,
        height: usize,
        stride: usize,
        bytes: &[u8],
        make: impl FnOnce(usize, usize, usize, &[u8]) -> Option<F>,
    ) {
        if let Err(what) = check_bgra(width, height, stride, bytes.len()) {
            if !self.shared.warned.swap(true, Ordering::AcqRel) {
                log::warn!(
                    "stream `{}`: a frame of {width}×{height} pixels with {stride} bytes per row \
                     and {} bytes in all shows nothing: {what}",
                    self.shared.name,
                    bytes.len()
                );
            }
            return;
        }
        // No video shows the stream: the bytes are not even copied.
        if !self.wanted() {
            return;
        }
        let Some(frame) = make(width, height, stride, bytes) else {
            if !self.shared.warned.swap(true, Ordering::AcqRel) {
                log::warn!(
                    "stream `{}`: a frame of {width}×{height} pixels cannot be made: it shows nothing",
                    self.shared.name
                );
            }
            return;
        };
        self.shared.push(self.registration, frame);
    }

    /// Ends the stream: the videos playing it show nothing from now on,
    /// until the name is registered again. Nothing once a later
    /// registration of the name replaced this one.
    pub fn end(&self) {
        self.shared.end(self.registration);
    }
}

impl<F: Send + 'static> Drop for StreamHandle<F> {
    fn drop(&mut self) {
        self.shared.end(self.registration);
    }
}

/// Why a frame of `width` × `height` pixels, `stride` bytes per row and
/// `len` bytes in all cannot be shown, if it cannot.
pub fn check_bgra(
    width: usize,
    height: usize,
    stride: usize,
    len: usize,
) -> Result<(), &'static str> {
    if width == 0 || height == 0 {
        return Err("the size is empty");
    }
    if width.checked_mul(4).is_none_or(|row| stride < row) {
        return Err("the stride is shorter than a row of BGRA pixels");
    }
    if stride.checked_mul(height).is_none_or(|all| len < all) {
        return Err("the bytes are fewer than stride × height");
    }
    Ok(())
}

/// A video's attachment to a stream ([`Streams::attach`]); dropping it
/// detaches the video's sink. Main thread.
#[non_exhaustive]
pub struct Attached<F> {
    shared: Arc<Shared<F>>,
    attachment: Rc<Attachment<F>>,
}

impl<F> Attached<F> {
    /// The stream's name.
    pub fn name(&self) -> &Arc<str> {
        &self.shared.name
    }

    /// Stops or resumes delivering frames to the video's sink (the last
    /// one stays; a video without one yet still takes the first).
    pub fn set_paused(&self, paused: bool) {
        self.attachment.paused.set(paused);
    }
}

impl<F> Drop for Attached<F> {
    fn drop(&mut self) {
        let key = self.attachment.key;
        let sinks = self.shared.sinks.get();
        let mut sinks = sinks.borrow_mut();
        sinks.retain(|a| a.key != key);
        self.shared.attached.store(sinks.len(), Ordering::Release);
    }
}

static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

/// What the names of NSH's own streams start with: [`Streams::register`]
/// refuses them, [`Streams::register_reserved`] takes them.
pub const RESERVED_PREFIX: &str = "nsh/";

/// A registry's entries by name, shared with its clones.
type Entries<F> = Rc<RefCell<HashMap<Arc<str>, Arc<Shared<F>>>>>;

/// The streams of one registry (a backend's `Components`), by name: shared
/// with its videos, so a registration applies to videos showing the name
/// from then on, and to those already showing it. Main thread.
#[non_exhaustive]
pub struct Streams<F> {
    entries: Entries<F>,
    /// Names already reported as unregistered.
    missing: Rc<RefCell<HashSet<Arc<str>>>>,
    hop: Arc<dyn Hop>,
}

impl<F> Clone for Streams<F> {
    fn clone(&self) -> Self {
        Streams {
            entries: self.entries.clone(),
            missing: self.missing.clone(),
            hop: self.hop.clone(),
        }
    }
}

impl<F> std::fmt::Debug for Streams<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.entries.borrow().keys()).finish()
    }
}

impl<F: Send + 'static> Streams<F> {
    /// A registry whose deliveries reach the main thread through `hop`.
    pub fn new(hop: impl Hop) -> Self {
        Streams {
            entries: Rc::default(),
            missing: Rc::default(),
            hop: Arc::new(hop),
        }
    }

    /// The entry of `name`, made if none.
    fn entry(&self, name: &str) -> Arc<Shared<F>> {
        let mut entries = self.entries.borrow_mut();
        if let Some(shared) = entries.get(name) {
            return shared.clone();
        }
        let name: Arc<str> = name.into();
        let shared = Shared::new(name.clone(), self.hop.clone());
        entries.insert(name, shared.clone());
        shared
    }

    /// Registers stream `name` (replacing an earlier registration, whose
    /// handle then pushes nothing and ends nothing): the handle takes its
    /// frames and audio. The videos whose `srcObject`
    /// names it, now or later, play it. A name starting with `nsh/` is
    /// NSH's own ([`RESERVED_PREFIX`]): it is refused, logged, and the
    /// handle plays nothing (its pushes cost nothing).
    pub fn register(&self, name: &str) -> StreamHandle<F> {
        if name.starts_with(RESERVED_PREFIX) {
            log::warn!(
                "stream `{name}` is not registered: names starting with `{RESERVED_PREFIX}` are \
                 NSH's own"
            );
            // An entry of its own, outside the registry: never registered
            // (no number of a live registration is `u64::MAX`), no video
            // attaches to it.
            return StreamHandle {
                shared: Shared::new(name.into(), self.hop.clone()),
                registration: u64::MAX,
            };
        }
        self.register_reserved(name)
    }

    /// [`Self::register`] without the check of reserved names: NSH's own
    /// hosts register `nsh/` names with it (the development hosts' test
    /// pattern, `nsh/test-pattern`).
    #[doc(hidden)]
    pub fn register_reserved(&self, name: &str) -> StreamHandle<F> {
        let shared = self.entry(name);
        let registration = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
        shared.registration.store(registration, Ordering::Release);
        self.missing.borrow_mut().remove(name);
        StreamHandle {
            shared,
            registration,
        }
    }

    /// Attaches `output`, a video's, to stream `name`'s audio: it hears the
    /// samples the registration pushes ([`StreamHandle::push_pcm`]), on the
    /// pushing thread, and its end. Attached before the registration, it
    /// hears the stream once something registers it. Dropping the
    /// attachment detaches it.
    pub fn attach_audio(&self, name: &str, output: Arc<dyn AudioOutput>) -> AudioAttachment {
        let shared = self.entry(name);
        let key = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
        shared.audio.update(|list| list.push((key, output)));
        AudioAttachment {
            name: shared.name.clone(),
            key,
            outputs: shared.audio.clone(),
        }
    }

    /// Attaches `sink`, a video's, to stream `name`: it hears the frames
    /// and the end on this thread. A name nothing registered is logged
    /// once; the sink hears frames once something is. A frame pushed but
    /// not delivered yet shows in the video at once.
    pub fn attach(&self, name: &str, sink: Sink<F>) -> Attached<F> {
        let shared = self.entry(name);
        if !shared.registered() && self.missing.borrow_mut().insert(shared.name.clone()) {
            log::warn!(
                target: "js",
                "no stream is registered as `{name}`: the video shows nothing"
            );
        }
        let attachment = Rc::new(Attachment {
            key: NEXT_KEY.fetch_add(1, Ordering::Relaxed),
            paused: Cell::new(false),
            loaded: Cell::new(false),
            sink,
        });
        {
            let sinks = shared.sinks.get();
            let mut sinks = sinks.borrow_mut();
            sinks.push(attachment.clone());
            shared.attached.store(sinks.len(), Ordering::Release);
        }
        if shared
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
        {
            shared.schedule();
        }
        Attached { shared, attachment }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    /// A registry whose hop queues the jobs for the test to run (`drain`),
    /// as a main loop would.
    type Jobs = mpsc::Receiver<Box<dyn FnOnce() + Send>>;

    fn streams() -> (Streams<Vec<u8>>, Jobs) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let hop = move |job: Box<dyn FnOnce() + Send>| tx.lock().unwrap().send(job).is_ok();
        (Streams::new(hop), rx)
    }

    fn drain(rx: &Jobs) -> usize {
        let mut n = 0;
        while let Ok(job) = rx.try_recv() {
            job();
            n += 1;
        }
        n
    }

    /// A sink recording the first byte of each frame, 0 for the end.
    fn recorder() -> (Sink<Vec<u8>>, Rc<RefCell<Vec<u8>>>) {
        let heard = Rc::new(RefCell::new(Vec::new()));
        let log = heard.clone();
        let sink: Sink<Vec<u8>> = Box::new(move |event| {
            log.borrow_mut().push(match event {
                StreamEvent::Frame { frame, first } => frame[0] + if first { 100 } else { 0 },
                StreamEvent::Ended => 0,
            });
        });
        (sink, heard)
    }

    fn copy(_: usize, _: usize, _: usize, bytes: &[u8]) -> Option<Vec<u8>> {
        Some(bytes.to_vec())
    }

    #[test]
    fn a_name_is_one_entry_and_the_newest_frame_wins() {
        let (streams, rx) = streams();
        let (sink, heard) = recorder();
        // Attached first: the registration that follows reaches it.
        let attached = streams.attach("cam", sink);
        let handle = streams.register("cam");
        assert_eq!(handle.name(), "cam");
        assert_eq!(&**attached.name(), "cam");
        assert_eq!(streams.entries.borrow().len(), 1);
        assert!(handle.wanted());
        handle.push_bgra(1, 1, 4, &[1, 0, 0, 255], copy);
        handle.push_bgra(1, 1, 4, &[2, 0, 0, 255], copy);
        // One hop for the two frames; only the newest is seen.
        assert_eq!(drain(&rx), 1);
        assert_eq!(*heard.borrow(), [102]);
        drop(attached);
        assert!(!handle.wanted());
        handle.push_bgra(1, 1, 4, &[3, 0, 0, 255], copy);
        assert_eq!(drain(&rx), 0);
        assert_eq!(*heard.borrow(), [102]);
        assert_eq!(format!("{streams:?}"), r#"{"cam"}"#);
    }

    #[test]
    fn paused_sinks_keep_the_last_frame_and_the_end_reaches_all() {
        let (streams, rx) = streams();
        let handle = streams.register("cam");
        let (sink, heard) = recorder();
        let attached = streams.attach("cam", sink);
        // Paused before any frame: the first one still shows, the next is
        // dropped.
        attached.set_paused(true);
        handle.push_bgra(1, 1, 4, &[7, 0, 0, 255], copy);
        drain(&rx);
        handle.push_bgra(1, 1, 4, &[8, 0, 0, 255], copy);
        drain(&rx);
        assert_eq!(*heard.borrow(), [107]);
        attached.set_paused(false);
        handle.push_bgra(1, 1, 4, &[9, 0, 0, 255], copy);
        drain(&rx);
        assert_eq!(*heard.borrow(), [107, 9]);
        // Ended: a pending frame goes, pushes after it show nothing, the
        // sink is a sink with no frame again.
        attached.set_paused(true);
        handle.push_bgra(1, 1, 4, &[1, 0, 0, 255], copy);
        handle.end();
        drain(&rx);
        assert_eq!(*heard.borrow(), [107, 9, 0]);
        handle.push_bgra(1, 1, 4, &[1, 0, 0, 255], copy);
        assert_eq!(drain(&rx), 0);
        // Registered again: the same entry, frames flow, the first shows
        // in the paused sink.
        let again = streams.register("cam");
        again.push_bgra(1, 1, 4, &[2, 0, 0, 255], copy);
        drain(&rx);
        assert_eq!(*heard.borrow(), [107, 9, 0, 102]);
        assert_eq!(streams.entries.borrow().len(), 1);
    }

    #[test]
    fn a_wrong_push_shows_nothing_and_does_not_panic() {
        let (streams, rx) = streams();
        let handle = streams.register("cam");
        let (sink, heard) = recorder();
        let _attached = streams.attach("cam", sink);
        // Too few bytes, a stride narrower than a row, no size, overflow.
        handle.push_bgra(2, 2, 8, &[0; 12], copy);
        handle.push_bgra(2, 2, 4, &[0; 32], copy);
        handle.push_bgra(0, 2, 8, &[0; 32], copy);
        handle.push_bgra(usize::MAX, 1, usize::MAX, &[0; 32], copy);
        // A frame the backend cannot make.
        handle.push_bgra(2, 2, 8, &[0; 32], |_, _, _, _| None);
        assert_eq!(drain(&rx), 0);
        assert!(handle.shared.warned.load(Ordering::Acquire));
        // A right one after them still shows.
        handle.push_bgra(2, 2, 8, &[5; 32], copy);
        drain(&rx);
        assert_eq!(*heard.borrow(), [105]);
    }

    #[test]
    fn a_pending_frame_reaches_a_sink_attaching_before_the_hop() {
        let (streams, rx) = streams();
        let handle = streams.register("cam");
        let (first, heard_first) = recorder();
        let _a = streams.attach("cam", first);
        handle.push_bgra(1, 1, 4, &[4, 0, 0, 255], copy);
        // Attached while the delivery waits: it hears the frame too.
        let (second, heard_second) = recorder();
        let _b = streams.attach("cam", second);
        drain(&rx);
        assert_eq!(*heard_first.borrow(), [104]);
        assert_eq!(*heard_second.borrow(), [104]);
    }

    #[test]
    fn dropping_the_handle_ends_the_stream() {
        let (streams, rx) = streams();
        let (sink, heard) = recorder();
        let _attached = streams.attach("cam", sink);
        let handle = streams.register("cam");
        let shared = handle.shared.clone();
        drop(handle);
        drain(&rx);
        assert!(!shared.registered());
        assert_eq!(*heard.borrow(), [0]);
        let again = streams.register("cam");
        assert!(Arc::ptr_eq(&again.shared, &shared));
        assert!(again.wanted());
    }

    #[test]
    fn a_replaced_handle_neither_pushes_nor_ends_the_new_registration() {
        let (streams, rx) = streams();
        let (sink, heard) = recorder();
        let _attached = streams.attach("cam", sink);
        let speaker = Arc::new(Speaker::default());
        let _audio = streams.attach_audio("cam", speaker.clone());
        let old = streams.register("cam");
        // Registered again before the old handle goes, as
        // `self.cam = streams.register("cam")` does.
        let new = streams.register("cam");
        assert!(!old.wanted() && !old.wants_audio());
        old.push_bgra(1, 1, 4, &[1, 0, 0, 255], copy);
        old.push_pcm(&[0.5], 1, 16_000);
        assert_eq!(drain(&rx), 0);
        assert!(speaker.heard().is_empty());
        old.end();
        drop(old);
        assert_eq!(drain(&rx), 0);
        assert_eq!(speaker.ends.load(Ordering::Relaxed), 0);
        // The new registration plays on.
        assert!(new.wanted() && new.wants_audio());
        new.push_bgra(1, 1, 4, &[2, 0, 0, 255], copy);
        new.push_pcm(&[0.25], 1, 16_000);
        drain(&rx);
        assert_eq!(*heard.borrow(), [102]);
        assert_eq!(speaker.heard().len(), 1);
        // Its own end still ends it, once.
        drop(new);
        drain(&rx);
        assert_eq!(*heard.borrow(), [102, 0]);
        assert_eq!(speaker.ends.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_frame_pushed_as_the_registration_ends_does_not_show_after_the_end() {
        let (streams, rx) = streams();
        let (sink, heard) = recorder();
        let _attached = streams.attach("cam", sink);
        let handle = streams.register("cam");
        let shared = handle.shared.clone();
        let registration = handle.registration;
        // The end takes `pending`'s lock: held here while a push on another
        // thread passes its first check and waits for the lock.
        let mut pending = shared.pending.lock().unwrap();
        let pusher = {
            let shared = shared.clone();
            std::thread::spawn(move || shared.push(registration, vec![9, 0, 0, 255]))
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        // What `end` does under the lock: no registration, nothing pending.
        shared.registration.store(0, Ordering::Release);
        pending.take();
        drop(pending);
        pusher.join().unwrap();
        // The push checked again under the lock and kept nothing.
        assert!(shared.pending.lock().unwrap().is_none());
        assert_eq!(drain(&rx), 0);
        assert!(heard.borrow().is_empty());
    }

    #[test]
    fn a_hop_that_fails_lets_the_next_push_try_again() {
        let failing = Streams::<Vec<u8>>::new(|_: Box<dyn FnOnce() + Send>| false);
        let (sink, _heard) = recorder();
        let _attached = failing.attach("cam", sink);
        let handle = failing.register("cam");
        handle.push_bgra(1, 1, 4, &[1, 0, 0, 255], copy);
        assert!(!handle.shared.scheduled.load(Ordering::Acquire));
    }

    #[test]
    fn an_end_the_hop_refused_reaches_the_sinks_with_the_next_delivery() {
        // A hop that refuses until the "main thread" is up.
        let up = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (tx, up_hop) = (Mutex::new(tx), up.clone());
        let streams = Streams::<Vec<u8>>::new(move |job: Box<dyn FnOnce() + Send>| {
            up_hop.load(Ordering::Acquire) && tx.lock().unwrap().send(job).is_ok()
        });
        let (sink, heard) = recorder();
        let attached = streams.attach("cam", sink);
        attached.set_paused(true);
        let handle = streams.register("cam");
        drop(handle);
        assert!(streams.entry("cam").ended.load(Ordering::Acquire));
        up.store(true, Ordering::Release);
        // Registered again: the first delivery tells the end first, then
        // the frame, a first one again for the paused sink.
        let again = streams.register("cam");
        again.push_bgra(1, 1, 4, &[3, 0, 0, 255], copy);
        drain(&rx);
        assert_eq!(*heard.borrow(), [0, 103]);
    }

    #[test]
    fn nsh_names_are_registered_only_as_reserved() {
        let (streams, rx) = streams();
        let (sink, heard) = recorder();
        let _attached = streams.attach("nsh/test-pattern", sink);
        // Refused: the handle plays nothing and the name stays unregistered.
        let refused = streams.register("nsh/test-pattern");
        assert_eq!(refused.name(), "nsh/test-pattern");
        assert!(!refused.wanted());
        refused.push_bgra(1, 1, 4, &[1, 0, 0, 255], copy);
        drop(refused);
        drain(&rx);
        assert!(heard.borrow().iter().all(|&b| b == 0));
        assert!(!streams.entry("nsh/test-pattern").registered());
        // NSH's hosts register it as reserved.
        let pattern = streams.register_reserved("nsh/test-pattern");
        assert!(pattern.wanted());
        pattern.push_bgra(1, 1, 4, &[2, 0, 0, 255], copy);
        drain(&rx);
        assert_eq!(heard.borrow().last(), Some(&102));
        // Other names are the host's.
        assert!(streams.register("nshx/cam").shared.registered());
    }

    /// An audio output recording what it hears: (channels, rate, samples)
    /// per write, and its ends.
    #[derive(Default)]
    struct Speaker {
        writes: Mutex<Vec<(u16, u32, Vec<f32>)>>,
        ends: AtomicUsize,
    }

    impl AudioOutput for Speaker {
        fn write(&self, samples: &[f32], channels: u16, sample_rate: u32) {
            self.writes
                .lock()
                .unwrap()
                .push((channels, sample_rate, samples.to_vec()));
        }

        fn end(&self) {
            self.ends.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl Speaker {
        fn heard(&self) -> Vec<(u16, u32, Vec<f32>)> {
            self.writes.lock().unwrap().clone()
        }
    }

    #[test]
    fn audio_goes_straight_to_the_outputs_attached_before_or_after() {
        let (streams, rx) = streams();
        // Attached before the registration.
        let early = Arc::new(Speaker::default());
        let early_attached = streams.attach_audio("call", early.clone());
        assert_eq!(&**early_attached.name(), "call");
        let handle = streams.register("call");
        assert!(handle.wants_audio());
        handle.push_pcm(&[0.5, -0.5], 2, 48_000);
        let late = Arc::new(Speaker::default());
        let _late_attached = streams.attach_audio("call", late.clone());
        handle.push_pcm(&[0.25], 1, 16_000);
        // No hop: every push reached the outputs on this thread, in order,
        // none replacing another.
        assert_eq!(drain(&rx), 0);
        assert_eq!(
            early.heard(),
            [(2, 48_000, vec![0.5, -0.5]), (1, 16_000, vec![0.25])]
        );
        assert_eq!(late.heard(), [(1, 16_000, vec![0.25])]);
        // Detached on drop: the other output still hears.
        drop(early_attached);
        handle.push_pcm(&[0.75], 1, 16_000);
        assert_eq!(early.heard().len(), 2);
        assert_eq!(late.heard().len(), 2);
        // The end reaches the outputs; pushes after it are not heard.
        handle.end();
        assert_eq!(late.ends.load(Ordering::Relaxed), 1);
        assert_eq!(early.ends.load(Ordering::Relaxed), 0);
        assert!(!handle.wants_audio());
        handle.push_pcm(&[0.1], 1, 16_000);
        assert_eq!(late.heard().len(), 2);
    }

    #[test]
    fn audio_nobody_hears_costs_nothing() {
        let (streams, rx) = streams();
        let handle = streams.register("call");
        assert!(!handle.wants_audio());
        handle.push_pcm(&[0.5; 4], 2, 48_000);
        assert_eq!(drain(&rx), 0);
        assert_eq!(handle.shared.audio.count.load(Ordering::Acquire), 0);
        // Frames are unaffected by audio outputs: none attached, none wanted.
        let _audio = streams.attach_audio("call", Arc::new(Speaker::default()));
        assert!(handle.wants_audio() && !handle.wanted());
    }

    #[test]
    fn wrong_audio_is_dropped_and_logged_once() {
        let (streams, _rx) = streams();
        let handle = streams.register("call");
        let speaker = Arc::new(Speaker::default());
        let _attached = streams.attach_audio("call", speaker.clone());
        // No channels, too many, a rate too low and too high, a partial
        // frame.
        handle.push_pcm(&[0.0; 4], 0, 48_000);
        handle.push_pcm(&[0.0; 9], 9, 48_000);
        handle.push_pcm(&[0.0; 4], 2, 4_000);
        handle.push_pcm(&[0.0; 4], 2, 384_000);
        handle.push_pcm(&[0.0; 3], 2, 48_000);
        assert!(speaker.heard().is_empty());
        assert!(handle.shared.audio_warned.load(Ordering::Acquire));
        // Frames have a warning of their own.
        assert!(!handle.shared.warned.load(Ordering::Acquire));
        // A right push after them is heard.
        handle.push_pcm(&[0.0; 4], 2, 48_000);
        assert_eq!(speaker.heard().len(), 1);
    }

    #[test]
    fn checks_audio() {
        assert_eq!(check_pcm(4, 2, 48_000), Ok(()));
        assert_eq!(check_pcm(0, 1, 8_000), Ok(()));
        assert_eq!(check_pcm(8, 8, 192_000), Ok(()));
        assert!(check_pcm(4, 0, 48_000).is_err());
        assert!(check_pcm(9, 9, 48_000).is_err());
        assert!(check_pcm(4, 2, 7_999).is_err());
        assert!(check_pcm(4, 2, 192_001).is_err());
        assert!(check_pcm(3, 2, 48_000).is_err());
    }

    #[test]
    fn checks_frames() {
        assert_eq!(check_bgra(2, 2, 8, 16), Ok(()));
        assert_eq!(check_bgra(2, 2, 12, 30), Ok(()));
        assert!(check_bgra(0, 2, 8, 16).is_err());
        assert!(check_bgra(2, 0, 8, 16).is_err());
        assert!(check_bgra(2, 2, 7, 16).is_err());
        assert!(check_bgra(2, 2, 8, 15).is_err());
        assert!(check_bgra(usize::MAX, 1, usize::MAX, usize::MAX).is_err());
    }
}
