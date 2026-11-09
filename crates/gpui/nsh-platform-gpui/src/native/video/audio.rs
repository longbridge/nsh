//! A live stream's sound in a video (feature `video`): each video showing a
//! stream attaches an [`Output`] to it (`Live` in `video.rs`), which the
//! host's `StreamHandle::push_pcm` reaches on the pushing thread.
//!
//! - The output is a GStreamer pipeline of its own, `appsrc ! audioconvert
//!   ! audioresample ! volume ! autoaudiosink`, made with the first samples
//!   (no device is opened for a stream that sends no sound), outside the
//!   output's lock, and stopped when the video stops showing the stream
//!   (the attachment and the output go).
//! - `appsrc` never blocks; at most [`MAX_QUEUED_MS`] wait in it: newer
//!   samples are dropped while it is full.
//! - The caps are F32LE at the samples' channels and rate (GStreamer's
//!   usual positions above two channels), set again when either changes.
//! - `paused` and `muted` drop the samples and mute the `volume` element,
//!   so what is queued stops at once; `volume` (0 to 1) is the element's.
//!   The main thread sets them without waiting for a pipeline being made.
//! - The end of the registration sends end-of-stream: what is queued plays
//!   out, and the next samples (a new registration) start a new pipeline.
//!   A pipeline that fails (an error on its bus, or one that cannot start)
//!   is logged once and stays silent until the end, after which a new
//!   registration tries again.

use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_audio as gst_audio;
use nsh_core::stream::AudioOutput;

/// How much audio waits in a video's queue (milliseconds) before newer
/// samples are dropped.
pub(super) const MAX_QUEUED_MS: u64 = 200;

/// The bytes `ms` milliseconds of `channels` channels of `f32` samples at
/// `rate` frames a second take.
fn queued_bytes(ms: u64, channels: u16, rate: u32) -> u64 {
    u64::from(rate) * u64::from(channels) * 4 * ms / 1000
}

/// Whether `len` more bytes may join the `level` queued: an empty queue
/// takes any push, so a push longer than the limit still plays.
fn fits(level: u64, len: u64, channels: u16, rate: u32) -> bool {
    level == 0 || level + len <= queued_bytes(MAX_QUEUED_MS, channels, rate)
}

/// A video's `paused`, `muted` and `volume`, set on the main thread and
/// read on the pushing thread.
pub(super) struct Gate {
    paused: AtomicBool,
    muted: AtomicBool,
    /// The volume's bits (`f32`), 0 to 1.
    volume: AtomicU32,
}

impl Gate {
    pub(super) fn new() -> Self {
        Gate {
            paused: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            volume: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    /// The video's props now.
    pub(super) fn set(&self, paused: bool, muted: bool, volume: f32) {
        self.paused.store(paused, Ordering::Release);
        self.muted.store(muted, Ordering::Release);
        let volume = if volume.is_nan() {
            0.0
        } else {
            volume.clamp(0.0, 1.0)
        };
        self.volume.store(volume.to_bits(), Ordering::Release);
    }

    /// Whether samples play: the video is neither paused nor muted.
    pub(super) fn open(&self) -> bool {
        !self.paused.load(Ordering::Acquire) && !self.muted.load(Ordering::Acquire)
    }

    /// The gain, 0 to 1.
    pub(super) fn volume(&self) -> f64 {
        f64::from(f32::from_bits(self.volume.load(Ordering::Acquire)))
    }

    /// Sets `element`'s volume and mute (paused or muted silence what is
    /// queued).
    fn apply(&self, element: &gst::Element) {
        element.set_property("volume", self.volume());
        element.set_property("mute", !self.open());
    }
}

/// `samples` as the bytes of GStreamer's `F32LE`.
pub(super) fn le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// The caps of `channels` channels of interleaved F32LE at `rate`.
fn caps(channels: u16, rate: u32) -> gst::Caps {
    let builder = gst_audio::AudioCapsBuilder::new_interleaved()
        .format(gst_audio::AudioFormat::F32le)
        .rate(i32::try_from(rate).unwrap_or(i32::MAX))
        .channels(i32::from(channels));
    // More than two channels need positions: GStreamer's usual ones.
    if channels > 2 {
        builder.fallback_channel_mask().build()
    } else {
        builder.build()
    }
}

/// A pipeline playing an output's samples.
struct Pipeline {
    pipeline: gst::Pipeline,
    src: gst_app::AppSrc,
    volume: gst::Element,
    /// The channels and rate of the caps set.
    format: (u16, u32),
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        // Stops the device now.
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

impl Pipeline {
    /// Makes and starts the pipeline for samples of `channels` and `rate`,
    /// its `volume` element set by `gate`.
    fn new(channels: u16, rate: u32, gate: &Gate) -> Result<Self, String> {
        if !super::player::gst_ready() {
            return Err("GStreamer did not start".to_owned());
        }
        let element = |factory: &str| {
            gst::ElementFactory::make(factory)
                .build()
                .map_err(|_| format!("GStreamer's `{factory}` is missing"))
        };
        let src = gst_app::AppSrc::builder()
            .caps(&caps(channels, rate))
            .format(gst::Format::Time)
            .block(false)
            .max_bytes(queued_bytes(MAX_QUEUED_MS, channels, rate))
            .build();
        let convert = element("audioconvert")?;
        let resample = element("audioresample")?;
        let volume = element("volume")?;
        gate.apply(&volume);
        let output = element("autoaudiosink")?;
        let pipeline = gst::Pipeline::new();
        let all = [
            src.upcast_ref::<gst::Element>(),
            &convert,
            &resample,
            &volume,
            &output,
        ];
        pipeline.add_many(all).map_err(|e| e.to_string())?;
        gst::Element::link_many(all).map_err(|e| e.to_string())?;
        // Made before it starts, so a start that fails sets it to `Null`.
        let made = Pipeline {
            pipeline,
            src,
            volume,
            format: (channels, rate),
        };
        made.pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| "the audio output did not start".to_owned())?;
        Ok(made)
    }

    /// Samples of `channels` and `rate` from now on.
    fn set_format(&mut self, channels: u16, rate: u32) {
        self.src.set_caps(Some(&caps(channels, rate)));
        self.src
            .set_max_bytes(queued_bytes(MAX_QUEUED_MS, channels, rate));
        self.format = (channels, rate);
    }

    /// The pipeline's failure, if it failed: its bus emptied, as nothing
    /// else reads it.
    fn failure(&self) -> Option<String> {
        let bus = self.pipeline.bus()?;
        let mut failure = None;
        while let Some(message) = bus.pop() {
            if let gst::MessageView::Error(error) = message.view() {
                failure = Some(error.error().to_string());
            }
        }
        failure
    }

    /// Queues `samples` unless the queue is full (newer samples are
    /// dropped then).
    fn push(&mut self, samples: &[f32], channels: u16, rate: u32) {
        if self.format != (channels, rate) {
            self.set_format(channels, rate);
        }
        let len = samples.len() as u64 * 4;
        if !fits(self.src.current_level_bytes(), len, channels, rate) {
            return;
        }
        // Refused only once the pipeline stops (a failure the next write
        // finds on the bus).
        let _ = self
            .src
            .push_buffer(gst::Buffer::from_mut_slice(le_bytes(samples)));
    }
}

/// What an output plays with, the pushing thread's.
enum State {
    /// No samples yet, or since the end.
    Idle,
    Playing(Pipeline),
    /// The registration ended: the queued samples play out.
    Ended(Pipeline),
    /// It failed (logged): silent until the end.
    Broken,
}

/// A video's output for a stream's audio.
pub(crate) struct Output {
    gate: Gate,
    state: Mutex<State>,
    /// The playing pipeline's `volume` element, apart from `state`, so the
    /// main thread never waits for a pipeline being made.
    volume_element: Mutex<Option<gst::Element>>,
    /// Bumped by every end: a pipeline made across one is dropped.
    epoch: AtomicU64,
    /// A failure was logged.
    warned: AtomicBool,
}

impl Output {
    pub(crate) fn new() -> Self {
        Output {
            gate: Gate::new(),
            state: Mutex::new(State::Idle),
            volume_element: Mutex::new(None),
            epoch: AtomicU64::new(0),
            warned: AtomicBool::new(false),
        }
    }

    /// The video's `paused`, `muted` and `volume` (main thread).
    pub(crate) fn set(&self, paused: bool, muted: bool, volume: f32) {
        self.gate.set(paused, muted, volume);
        if let Some(element) = &*self.volume_element() {
            self.gate.apply(element);
        }
    }

    fn volume_element(&self) -> MutexGuard<'_, Option<gst::Element>> {
        self.volume_element
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The output failed: logged once, silent until the end.
    fn broken(&self, state: &mut State, what: &str) {
        if !self.warned.swap(true, Ordering::AcqRel) {
            log::warn!(target: "js", "a live stream's audio does not play: {what}");
        }
        *state = State::Broken;
        *self.volume_element() = None;
    }
}

impl AudioOutput for Output {
    fn write(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        if !self.gate.open() {
            return;
        }
        let epoch = {
            let mut state = self.state();
            // A pipeline that ended goes (to `Null`): these samples start anew.
            if matches!(*state, State::Ended(_)) {
                *state = State::Idle;
            }
            let failure = match &*state {
                State::Playing(pipeline) => pipeline.failure(),
                _ => None,
            };
            if let Some(failure) = failure {
                self.broken(&mut state, &failure);
            }
            match &mut *state {
                State::Playing(pipeline) => {
                    pipeline.push(samples, channels, sample_rate);
                    return;
                }
                State::Broken | State::Ended(_) => return,
                State::Idle => self.epoch.load(Ordering::Acquire),
            }
        };
        // Made without the lock: starting opens the audio device.
        let made = Pipeline::new(channels, sample_rate, &self.gate);
        let mut state = self.state();
        if !matches!(*state, State::Idle) || self.epoch.load(Ordering::Acquire) != epoch {
            // Another write made one, or the registration ended meanwhile.
            return;
        }
        match made {
            Ok(mut pipeline) => {
                {
                    let mut element = self.volume_element();
                    // Props set while it was made.
                    self.gate.apply(&pipeline.volume);
                    *element = Some(pipeline.volume.clone());
                }
                pipeline.push(samples, channels, sample_rate);
                *state = State::Playing(pipeline);
            }
            Err(failure) => self.broken(&mut state, &failure),
        }
    }

    fn end(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let mut state = self.state();
        *state = match std::mem::replace(&mut *state, State::Idle) {
            State::Playing(pipeline) => {
                let _ = pipeline.src.end_of_stream();
                State::Ended(pipeline)
            }
            State::Ended(pipeline) => State::Ended(pipeline),
            // A failed output tries again with the next registration.
            State::Idle | State::Broken => {
                self.warned.store(false, Ordering::Release);
                State::Idle
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_or_muted_videos_drop_the_samples() {
        let gate = Gate::new();
        assert!(gate.open());
        assert_eq!(gate.volume(), 1.0);
        gate.set(true, false, 1.0);
        assert!(!gate.open());
        gate.set(false, true, 1.0);
        assert!(!gate.open());
        gate.set(false, false, 0.5);
        assert!(gate.open());
        assert_eq!(gate.volume(), 0.5);
    }

    #[test]
    fn the_volume_is_clamped_to_one() {
        let gate = Gate::new();
        gate.set(false, false, -1.0);
        assert_eq!(gate.volume(), 0.0);
        gate.set(false, false, 5.0);
        assert_eq!(gate.volume(), 1.0);
        gate.set(false, false, f32::NAN);
        assert_eq!(gate.volume(), 0.0);
    }

    #[test]
    fn a_full_queue_drops_the_newest_samples() {
        // 200 ms of stereo at 48 kHz: 9600 frames of 8 bytes.
        assert_eq!(queued_bytes(200, 2, 48_000), 76_800);
        assert_eq!(queued_bytes(200, 8, 192_000), 1_228_800);
        // An empty queue takes any push; a full one drops what would
        // overflow it.
        assert!(fits(0, 1_000_000, 2, 48_000));
        assert!(fits(70_000, 6_800, 2, 48_000));
        assert!(!fits(70_000, 6_801, 2, 48_000));
    }

    #[test]
    fn samples_become_little_endian_floats() {
        let bytes = le_bytes(&[1.0, -0.5]);
        assert_eq!(bytes.len(), 8);
        assert_eq!(&bytes[..4], &1f32.to_le_bytes());
        assert_eq!(&bytes[4..], &(-0.5f32).to_le_bytes());
    }

    #[test]
    fn more_than_two_channels_are_positioned() {
        gst::init().unwrap();
        let six = caps(6, 48_000);
        assert!(six.structure(0).unwrap().has_field("channel-mask"));
        let two = caps(2, 48_000);
        assert!(!two.structure(0).unwrap().has_field("channel-mask"));
    }

    #[test]
    fn a_paused_output_makes_no_pipeline_and_the_end_resets_a_failure() {
        let output = Output::new();
        output.set(true, false, 1.0);
        output.write(&[0.0; 4], 2, 48_000);
        assert!(matches!(*output.state(), State::Idle));
        *output.state() = State::Broken;
        output.warned.store(true, Ordering::Release);
        output.end();
        assert!(matches!(*output.state(), State::Idle));
        assert!(!output.warned.load(Ordering::Acquire));
    }
}
