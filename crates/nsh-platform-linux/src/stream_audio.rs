//! A live stream's audio in one video (`<Video srcObject>`): the
//! [`AudioOutput`] each video showing a stream attaches
//! ([`crate::stream::Streams::attach_audio`]), playing the samples the host
//! pushes ([`crate::StreamHandle::push_pcm`]) through a GStreamer pipeline
//! of its own, `appsrc ! audioconvert ! audioresample ! volume !
//! autoaudiosink` (the first installed of [`sound::OUTPUTS`]).
//!
//! - The pipeline is made at the first samples, on the pushing thread,
//!   which pushes every buffer itself (no hop): `appsrc` never blocks, and
//!   at most [`MAX_QUEUED_MS`] wait in it; newer samples are dropped while
//!   it is full (the device's own buffer comes on top).
//! - The caps are F32LE at the samples' channels and rate, set again when
//!   either changes (the pipeline renegotiates with the next buffer).
//! - `paused` and `muted` drop the samples and mute the `volume` element,
//!   so what is queued stops at once (it drains silently). `volume` is
//!   the element's. The main thread sets them as the video's props change.
//! - The end of the registration sends end-of-stream: what is queued
//!   plays out, and the next samples (a new registration) start a new
//!   pipeline. A pipeline that fails is logged once and stays silent until
//!   the stream ends.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use gstreamer::{self as gst, prelude::*};
use gstreamer_app as gst_app;
use gstreamer_audio as gst_audio;
use nsh_core::stream::AudioOutput;

use crate::sound;

/// How much audio waits in a video's queue (milliseconds) before newer
/// samples are dropped.
const MAX_QUEUED_MS: u64 = 200;

/// The bytes `ms` milliseconds of `channels` channels of `f32` samples at
/// `rate` frames a second take.
fn queued_bytes(ms: u64, channels: u16, rate: u32) -> u64 {
    u64::from(rate) * u64::from(channels) * 4 * ms / 1000
}

/// Whether `len` more bytes may join the `level` queued: a queue that is
/// empty takes any push, so a push longer than the limit still plays.
fn fits(level: u64, len: u64, channels: u16, rate: u32) -> bool {
    level == 0 || level + len <= queued_bytes(MAX_QUEUED_MS, channels, rate)
}

/// `samples` as F32LE bytes, the pipeline's format.
fn le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// The caps of `channels` channels of F32LE at `rate`.
fn caps(channels: u16, rate: u32) -> gst::Caps {
    let builder = gst_audio::AudioCapsBuilder::new_interleaved()
        .format(gst_audio::AudioFormat::F32le)
        .rate(rate as i32)
        .channels(i32::from(channels));
    // More than two channels need positions: GStreamer's usual ones.
    if channels > 2 {
        builder.fallback_channel_mask().build()
    } else {
        builder.build()
    }
}

/// A video's audio of a stream.
pub(crate) struct StreamAudio {
    name: Arc<str>,
    /// The output element; `None`: the first installed of [`sound::OUTPUTS`].
    sink: Option<&'static str>,
    paused: AtomicBool,
    muted: AtomicBool,
    /// The volume (0 to 1), an `f32`'s bits.
    volume: AtomicU32,
    state: Mutex<State>,
    /// The playing pipeline's `volume` element, apart from `state`, so the
    /// main thread never waits for a pipeline being made.
    volume_element: Mutex<Option<gst::Element>>,
    /// A failure was logged.
    warned: AtomicBool,
}

enum State {
    /// No samples yet, or since the end.
    Idle,
    Playing(Pipeline),
    /// The registration ended: the queued samples play out.
    Ended(Pipeline),
    /// It failed (logged): silent until the end.
    Broken,
}

impl StreamAudio {
    /// The audio of stream `name` in a video, playing into the system's
    /// output.
    pub(crate) fn new(name: &str) -> Arc<Self> {
        Self::with_sink(name, None)
    }

    /// The same playing into `sink` (tests use `fakesink`).
    fn with_sink(name: &str, sink: Option<&'static str>) -> Arc<Self> {
        Arc::new(StreamAudio {
            name: name.into(),
            sink,
            paused: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            volume: AtomicU32::new(1f32.to_bits()),
            state: Mutex::new(State::Idle),
            volume_element: Mutex::new(None),
            warned: AtomicBool::new(false),
        })
    }

    /// The video's `paused` (main thread): samples are dropped and what is
    /// queued is silenced at once.
    pub(crate) fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
        self.apply_mute();
    }

    /// The video's `muted` (main thread): samples are dropped and what is
    /// queued is silenced at once.
    pub(crate) fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Release);
        self.apply_mute();
    }

    /// Whether the samples are dropped: paused or muted.
    fn silenced(&self) -> bool {
        self.paused.load(Ordering::Acquire) || self.muted.load(Ordering::Acquire)
    }

    /// Mutes the `volume` element while paused or muted, so what is queued
    /// drains silently instead of playing on.
    fn apply_mute(&self) {
        if let Some(element) = &*self
            .volume_element
            .lock()
            .unwrap_or_else(|e| e.into_inner())
        {
            element.set_property("mute", self.silenced());
        }
    }

    /// The video's `volume` (main thread), 0 to 1.
    pub(crate) fn set_volume(&self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.volume.store(volume.to_bits(), Ordering::Release);
        if let Some(element) = &*self
            .volume_element
            .lock()
            .unwrap_or_else(|e| e.into_inner())
        {
            element.set_property("volume", f64::from(volume));
        }
    }

    fn warn(&self, what: &str) {
        if !self.warned.swap(true, Ordering::AcqRel) {
            log::warn!("stream `{}`: its audio does not play: {what}", self.name);
        }
    }

    /// Whether a pipeline plays (tests).
    #[cfg(test)]
    fn is_playing(&self) -> bool {
        matches!(
            *self.state.lock().unwrap_or_else(|e| e.into_inner()),
            State::Playing(_)
        )
    }
}

impl AudioOutput for StreamAudio {
    fn write(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        if self.silenced() {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // A pipeline that ended goes (to `Null`): these samples start anew.
        if matches!(*state, State::Ended(_)) {
            *state = State::Idle;
        }
        let failure = match &*state {
            State::Playing(pipeline) => pipeline.failure(),
            _ => None,
        };
        if let Some(failure) = failure {
            self.warn(&failure);
            *state = State::Broken;
            *self
                .volume_element
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
        }
        if matches!(*state, State::Idle) {
            let volume = f32::from_bits(self.volume.load(Ordering::Acquire));
            *state = match Pipeline::new(self.sink, channels, sample_rate, volume) {
                Ok(pipeline) => {
                    *self
                        .volume_element
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Some(pipeline.volume.clone());
                    // Paused or muted while it was made.
                    self.apply_mute();
                    State::Playing(pipeline)
                }
                Err(failure) => {
                    self.warn(&failure);
                    State::Broken
                }
            };
        }
        let State::Playing(pipeline) = &mut *state else {
            return;
        };
        if pipeline.format != (channels, sample_rate) {
            pipeline.set_format(channels, sample_rate);
        }
        let len = samples.len() as u64 * 4;
        if !fits(
            pipeline.src.current_level_bytes(),
            len,
            channels,
            sample_rate,
        ) {
            return;
        }
        // Refused only once the pipeline stops (a failure the next write
        // finds on the bus).
        let _ = pipeline
            .src
            .push_buffer(gst::Buffer::from_mut_slice(le_bytes(samples)));
    }

    fn end(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *state = match std::mem::replace(&mut *state, State::Idle) {
            State::Playing(pipeline) => {
                let _ = pipeline.src.end_of_stream();
                State::Ended(pipeline)
            }
            State::Ended(pipeline) => State::Ended(pipeline),
            // A failed output tries again with the next registration.
            State::Idle | State::Broken => State::Idle,
        };
    }
}

/// One video's pipeline: `appsrc ! audioconvert ! audioresample ! volume !
/// <output>`.
struct Pipeline {
    pipeline: gst::Pipeline,
    src: gst_app::AppSrc,
    volume: gst::Element,
    /// The channels and rate of the caps.
    format: (u16, u32),
}

impl Pipeline {
    /// Makes the pipeline for samples of `channels` and `rate` and starts
    /// it, at `volume`, into `sink` (or the first installed output).
    fn new(
        sink: Option<&'static str>,
        channels: u16,
        rate: u32,
        volume: f32,
    ) -> Result<Pipeline, String> {
        sound::init()?;
        let sink = match sink {
            Some(sink) => sink,
            None => sound::OUTPUTS
                .iter()
                .copied()
                .find(|factory| gst::ElementFactory::find(factory).is_some())
                .ok_or("no GStreamer audio output is installed")?,
        };
        let src = gst_app::AppSrc::builder()
            .caps(&caps(channels, rate))
            .format(gst::Format::Time)
            .block(false)
            .max_bytes(queued_bytes(MAX_QUEUED_MS, channels, rate))
            .build();
        let convert = sound::element("audioconvert")?;
        let resample = sound::element("audioresample")?;
        let volume_element = sound::element("volume")?;
        volume_element.set_property("volume", f64::from(volume));
        let output = sound::element(sink)?;
        let pipeline = gst::Pipeline::new();
        let all = [
            src.upcast_ref::<gst::Element>(),
            &convert,
            &resample,
            &volume_element,
            &output,
        ];
        pipeline.add_many(all).map_err(|e| e.to_string())?;
        gst::Element::link_many(all).map_err(|e| e.to_string())?;
        // Made before it starts, so a start that fails sets it to `Null`.
        let made = Pipeline {
            pipeline,
            src,
            volume: volume_element,
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
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_the_bytes_of_queued_audio() {
        // 200 ms of stereo at 48 kHz: 9600 frames of 8 bytes.
        assert_eq!(queued_bytes(200, 2, 48_000), 76_800);
        assert_eq!(queued_bytes(200, 1, 8_000), 6_400);
        assert_eq!(queued_bytes(200, 8, 192_000), 1_228_800);
        // An empty queue takes any push; a full one drops what would
        // overflow it.
        assert!(fits(0, 1_000_000, 2, 48_000));
        assert!(fits(70_000, 6_800, 2, 48_000));
        assert!(!fits(70_000, 6_801, 2, 48_000));
    }

    #[test]
    fn writes_samples_as_little_endian_floats() {
        let bytes = le_bytes(&[1.0, -0.5]);
        assert_eq!(bytes.len(), 8);
        assert_eq!(&bytes[..4], &1f32.to_le_bytes());
        assert_eq!(&bytes[4..], &(-0.5f32).to_le_bytes());
    }

    #[test]
    fn positions_more_than_two_channels() {
        sound::init().unwrap();
        let stereo = caps(2, 48_000);
        let s = stereo.structure(0).unwrap();
        assert_eq!(s.get::<i32>("channels").unwrap(), 2);
        assert_eq!(s.get::<i32>("rate").unwrap(), 48_000);
        assert!(!s.has_field("channel-mask"));
        assert!(
            caps(6, 44_100)
                .structure(0)
                .unwrap()
                .has_field("channel-mask")
        );
    }

    // Into `fakesink`: no device needed.
    #[test]
    fn plays_until_paused_muted_or_ended() {
        let audio = StreamAudio::with_sink("call", Some("fakesink"));
        // Paused or muted: no pipeline is even made.
        audio.set_paused(true);
        audio.write(&[0.0; 960], 2, 48_000);
        assert!(!audio.is_playing());
        audio.set_paused(false);
        audio.set_muted(true);
        audio.write(&[0.0; 960], 2, 48_000);
        assert!(!audio.is_playing());
        audio.set_muted(false);
        audio.set_volume(0.5);
        audio.write(&[0.25; 960], 2, 48_000);
        assert!(audio.is_playing());
        // The volume reaches the element; a new format the caps.
        let element = audio.volume_element.lock().unwrap().clone().unwrap();
        assert_eq!(element.property::<f64>("volume"), 0.5);
        audio.set_volume(2.0);
        assert_eq!(element.property::<f64>("volume"), 1.0);
        // Paused or muted, what is queued is silenced at once.
        assert!(!element.property::<bool>("mute"));
        audio.set_paused(true);
        assert!(element.property::<bool>("mute"));
        audio.set_muted(true);
        audio.set_paused(false);
        assert!(element.property::<bool>("mute"));
        audio.set_muted(false);
        assert!(!element.property::<bool>("mute"));
        audio.write(&[0.25; 160], 1, 16_000);
        {
            let state = audio.state.lock().unwrap();
            let State::Playing(pipeline) = &*state else {
                panic!("playing");
            };
            assert_eq!(pipeline.format, (1, 16_000));
            let caps = pipeline.src.caps().unwrap();
            assert_eq!(
                caps.structure(0).unwrap().get::<i32>("rate").unwrap(),
                16_000
            );
        }
        // Ended: the queue plays out; the next samples start anew.
        audio.end();
        assert!(!audio.is_playing());
        audio.write(&[0.25; 960], 2, 48_000);
        assert!(audio.is_playing());
        assert!(!audio.warned.load(Ordering::Acquire));
    }

    #[test]
    fn an_output_that_cannot_be_made_is_silent_until_the_end() {
        let audio = StreamAudio::with_sink("call", Some("no-such-sink"));
        audio.write(&[0.0; 960], 2, 48_000);
        assert!(!audio.is_playing());
        assert!(audio.warned.load(Ordering::Acquire));
        assert!(matches!(*audio.state.lock().unwrap(), State::Broken));
        audio.end();
        assert!(matches!(*audio.state.lock().unwrap(), State::Idle));
    }
}
