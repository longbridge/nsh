//! Sounds on Linux (`nsh`'s `Sound`; `audio.rs` drives them): decoded once
//! into memory by GStreamer, played from there by one mixing pipeline per
//! engine.
//!
//! - **Decoding** ([`decode`]) runs a pipeline of its own off the main
//!   thread: the encoded bytes through `decodebin` (whatever GStreamer's
//!   plug-ins read) into 32-bit float, stereo, 48 kHz samples, stopped at
//!   the length the core allows. The samples are the sound: nothing is read
//!   or decoded again at a play.
//! - **Playing** ([`Mixer`]): one live pipeline, a silent source into an
//!   `audiomixer` into the default output (`autoaudiosink`), made at the
//!   first play and kept. Each play is a voice: an `appsrc` holding the
//!   samples (shared, not copied, at the recorded rate; resampled once at
//!   another rate, the pitch with it, as on the other platforms), linked to
//!   a mixer pad whose `volume` is the play's, its first sample placed at
//!   the pipeline's running time when it is added. So a play starts within
//!   the output's latency and plays overlap. A voice is removed once its
//!   samples played (looked at when the next play or stop comes); past
//!   [`MAX_VOICES`] the oldest stops.
//! - An output that cannot start, or that fails later (an error on the
//!   bus), makes sounds silence: the mixer is made again at the next play.
//!
//! The samples' format is fixed ([`RATE`], two channels): what desktop
//! outputs mix at, so the output's own conversion is usually none.

use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use gstreamer::{self as gst, prelude::*};
use gstreamer_app as gst_app;
use gstreamer_audio as gst_audio;

/// The samples' rate.
pub(crate) const RATE: u32 = 48_000;
/// The samples' channels.
const CHANNELS: usize = 2;
/// Bytes of one frame: a 32-bit float per channel.
const FRAME_BYTES: usize = CHANNELS * 4;
/// The silent source's buffers, so the mixer's own latency is one of them.
const SILENCE_SAMPLES: i32 = 480;
/// How long a sound may take to decode.
const DECODE_TIMEOUT: Duration = Duration::from_secs(10);
/// Voices at once (`nsh_core::audio::MAX_VOICES`).
const MAX_VOICES: usize = 32;

/// A decoded sound: interleaved 32-bit float frames, little-endian, at
/// [`RATE`]. Cloning shares the samples.
#[derive(Clone)]
pub(crate) struct Pcm(Arc<[u8]>);

impl Pcm {
    /// Its size in bytes, as the core counts decoded audio.
    pub(crate) fn bytes(&self) -> u64 {
        self.0.len() as u64
    }

    fn frames(&self) -> usize {
        self.0.len() / FRAME_BYTES
    }
}

/// What [`decode`] made of a sound.
pub(crate) struct Decoded {
    pub(crate) pcm: Pcm,
    pub(crate) duration: Duration,
    /// Longer than the length asked for, and cut there.
    pub(crate) truncated: bool,
}

/// Initializes GStreamer once (GTK's media streams may have already).
fn init() -> Result<(), String> {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    INIT.get_or_init(|| gst::init().map_err(|e| format!("GStreamer did not start: {e}")))
        .clone()
}

/// The samples' caps.
fn caps() -> gst::Caps {
    gst_audio::AudioCapsBuilder::new_interleaved()
        .format(gst_audio::AudioFormat::F32le)
        .rate(RATE as i32)
        .channels(CHANNELS as i32)
        .build()
}

fn element(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .build()
        .map_err(|_| format!("the GStreamer element `{factory}` is not installed"))
}

/// The duration of `frames` at [`RATE`].
fn duration_of(frames: usize) -> Duration {
    Duration::from_secs_f64(frames as f64 / f64::from(RATE))
}

/// Decodes `bytes` into memory, at most `max` of them. Blocks: off the main
/// thread.
pub(crate) fn decode(bytes: Vec<u8>, max: Duration) -> Result<Decoded, String> {
    init()?;
    let pipeline = gst::Pipeline::new();
    let src = gst_app::AppSrc::builder()
        .format(gst::Format::Bytes)
        .build();
    let decoder = element("decodebin")?;
    let convert = element("audioconvert")?;
    let resample = element("audioresample")?;
    let sink = gst_app::AppSink::builder()
        .caps(&caps())
        .sync(false)
        .build();
    pipeline
        .add_many([
            src.upcast_ref::<gst::Element>(),
            &decoder,
            &convert,
            &resample,
            sink.upcast_ref(),
        ])
        .map_err(|e| e.to_string())?;
    src.link(&decoder).map_err(|e| e.to_string())?;
    gst::Element::link_many([&convert, &resample, sink.upcast_ref()]).map_err(|e| e.to_string())?;
    // The decoder's pads come once it knows the stream: the first audio one
    // is the sound (a file with video too gives its audio).
    let convert_sink = convert
        .static_pad("sink")
        .ok_or("audioconvert has no sink")?;
    decoder.connect_pad_added(move |_, pad| {
        let audio = pad
            .current_caps()
            .and_then(|caps| caps.structure(0).map(|s| s.name().starts_with("audio/")))
            .unwrap_or(false);
        if audio && !convert_sink.is_linked() {
            let _ = pad.link(&convert_sink);
        }
    });
    let result = pull(&pipeline, &src, &sink, bytes, max);
    let _ = pipeline.set_state(gst::State::Null);
    let (bytes, truncated) = result?;
    if bytes.is_empty() {
        return Err("the sound has no audio GStreamer can decode".into());
    }
    let pcm = Pcm(bytes.into());
    Ok(Decoded {
        duration: duration_of(pcm.frames()),
        pcm,
        truncated,
    })
}

/// Runs the decoding `pipeline` and collects its samples: the bytes and
/// whether they were cut at `max`.
fn pull(
    pipeline: &gst::Pipeline,
    src: &gst_app::AppSrc,
    sink: &gst_app::AppSink,
    bytes: Vec<u8>,
    max: Duration,
) -> Result<(Vec<u8>, bool), String> {
    let limit = (max.as_secs_f64() * f64::from(RATE)).ceil() as usize * FRAME_BYTES;
    src.push_buffer(gst::Buffer::from_slice(bytes))
        .map_err(|e| format!("the sound cannot be read: {e}"))?;
    let _ = src.end_of_stream();
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|_| "the sound cannot be decoded".to_owned())?;
    let bus = pipeline.bus().ok_or("the pipeline has no bus")?;
    let deadline = Instant::now() + DECODE_TIMEOUT;
    let mut out = Vec::new();
    loop {
        if let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_mseconds(50)) {
            if let Some(buffer) = sample.buffer() {
                let map = buffer.map_readable().map_err(|e| e.to_string())?;
                out.extend_from_slice(map.as_slice());
            }
            if out.len() > limit {
                out.truncate(limit);
                return Ok((out, true));
            }
            continue;
        }
        if sink.is_eos() {
            return Ok((out, false));
        }
        if let Some(message) = bus.pop_filtered(&[gst::MessageType::Error])
            && let gst::MessageView::Error(error) = message.view()
        {
            return Err(error.error().to_string());
        }
        if Instant::now() > deadline {
            return Err("the sound did not decode in time".into());
        }
    }
}

/// `pcm` played at `rate`: resampled (linearly) so that it lasts
/// `1 / rate` as long, the pitch going with it.
fn at_rate(pcm: &Pcm, rate: f32) -> Pcm {
    if (rate - 1.0).abs() < f32::EPSILON || pcm.frames() < 2 {
        return pcm.clone();
    }
    let sample = |frame: usize, channel: usize| {
        let at = frame * FRAME_BYTES + channel * 4;
        f32::from_le_bytes(pcm.0[at..at + 4].try_into().expect("four bytes"))
    };
    let last = pcm.frames() - 1;
    let frames = ((pcm.frames() as f64) / f64::from(rate)).floor() as usize;
    let mut out = Vec::with_capacity(frames * FRAME_BYTES);
    for i in 0..frames {
        let at = i as f64 * f64::from(rate);
        let from = (at.floor() as usize).min(last);
        let to = (from + 1).min(last);
        let t = (at - from as f64) as f32;
        for channel in 0..CHANNELS {
            let (a, b) = (sample(from, channel), sample(to, channel));
            out.extend_from_slice(&(a + (b - a) * t).to_le_bytes());
        }
    }
    Pcm(out.into())
}

/// A play: its source and the mixer pad it plays through.
struct Voice {
    sound: u32,
    src: gst_app::AppSrc,
    pad: gst::Pad,
    started: Instant,
    /// When its samples will have played.
    until: Instant,
}

/// The pipeline every sound of an engine plays through.
pub(crate) struct Mixer {
    pipeline: gst::Pipeline,
    mixer: gst::Element,
    voices: Vec<Voice>,
    /// Set by the bus when the output failed.
    broken: Rc<Cell<bool>>,
    _bus: Option<gst::bus::BusWatchGuard>,
}

impl Mixer {
    /// Makes the pipeline, playing into the default output, its failures
    /// watched on the main loop. On the main thread.
    pub(crate) fn new() -> Result<Mixer, String> {
        let mut mixer = Self::with_sink("autoaudiosink")?;
        let flag = mixer.broken.clone();
        mixer._bus = mixer.pipeline.bus().and_then(|bus| {
            bus.add_watch_local(move |_, message| {
                if let gst::MessageView::Error(error) = message.view() {
                    log::warn!("the sound output failed: {}", error.error());
                    flag.set(true);
                }
                gst::glib::ControlFlow::Continue
            })
            .ok()
        });
        Ok(mixer)
    }

    /// Makes the pipeline, playing into an element of `sink` (tests use
    /// `fakesink`), unwatched.
    fn with_sink(sink: &str) -> Result<Mixer, String> {
        init()?;
        let pipeline = gst::Pipeline::new();
        // The silent source keeps the mixer running, live, between plays:
        // a voice's samples go at the running time they were added at.
        let silence = gst::ElementFactory::make("audiotestsrc")
            .property_from_str("wave", "silence")
            .property("is-live", true)
            .property("samplesperbuffer", SILENCE_SAMPLES)
            .build()
            .map_err(|_| "the GStreamer element `audiotestsrc` is not installed".to_owned())?;
        let filter = gst::ElementFactory::make("capsfilter")
            .property("caps", caps())
            .build()
            .map_err(|_| "the GStreamer element `capsfilter` is not installed".to_owned())?;
        let mixer = element("audiomixer")?;
        let convert = element("audioconvert")?;
        let resample = element("audioresample")?;
        let output = element(sink)?;
        let all = [&silence, &filter, &mixer, &convert, &resample, &output];
        pipeline.add_many(all).map_err(|e| e.to_string())?;
        gst::Element::link_many(all).map_err(|e| e.to_string())?;
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| "the sound output did not start".to_owned())?;
        Ok(Mixer {
            pipeline,
            mixer,
            voices: Vec::new(),
            broken: Rc::default(),
            _bus: None,
        })
    }

    /// Whether the output failed: the next play makes a new mixer.
    pub(crate) fn is_broken(&self) -> bool {
        self.broken.get()
    }

    /// Plays `pcm` (the samples of `sound`) at `volume` and `rate`, over
    /// what plays.
    pub(crate) fn play(
        &mut self,
        sound: u32,
        pcm: &Pcm,
        volume: f32,
        rate: f32,
    ) -> Result<(), String> {
        self.sweep();
        if self.voices.len() >= MAX_VOICES
            && let Some(oldest) = (0..self.voices.len()).min_by_key(|&i| self.voices[i].started)
        {
            let voice = self.voices.remove(oldest);
            self.remove(voice);
        }
        let pcm = at_rate(pcm, rate);
        let length = duration_of(pcm.frames());
        let src = gst_app::AppSrc::builder()
            .caps(&caps())
            .format(gst::Format::Time)
            .is_live(true)
            .build();
        self.pipeline.add(&src).map_err(|e| e.to_string())?;
        let pad = self
            .mixer
            .request_pad_simple("sink_%u")
            .ok_or("the mixer gave no pad")?;
        pad.set_property("volume", f64::from(volume.clamp(0.0, 1.0)));
        let src_pad = src.static_pad("src").ok_or("appsrc has no pad")?;
        // The samples start at 0 in their segment: offset to the running
        // time now, they play from now on (and are not late).
        let now = self
            .pipeline
            .current_running_time()
            .unwrap_or(gst::ClockTime::ZERO);
        src_pad.set_offset(now.nseconds() as i64);
        let voice = Voice {
            sound,
            src: src.clone(),
            pad: pad.clone(),
            started: Instant::now(),
            until: Instant::now() + length,
        };
        if let Err(e) = src_pad.link(&pad) {
            self.remove(voice);
            return Err(format!("{e:?}"));
        }
        let mut buffer = gst::Buffer::from_slice(pcm.0.clone());
        if let Some(buffer) = buffer.get_mut() {
            buffer.set_pts(gst::ClockTime::ZERO);
            buffer.set_duration(gst::ClockTime::from_nseconds(length.as_nanos() as u64));
        }
        let pushed = src
            .sync_state_with_parent()
            .map_err(|e| e.to_string())
            .and_then(|()| src.push_buffer(buffer).map_err(|e| e.to_string()))
            .and_then(|_| src.end_of_stream().map_err(|e| e.to_string()));
        if let Err(e) = pushed {
            self.remove(voice);
            return Err(e);
        }
        self.voices.push(voice);
        Ok(())
    }

    /// Stops the plays of `sound`.
    pub(crate) fn stop(&mut self, sound: u32) {
        let (stopped, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.voices)
            .into_iter()
            .partition(|v| v.sound == sound);
        self.voices = rest;
        for voice in stopped {
            self.remove(voice);
        }
    }

    /// The voices playing.
    #[cfg(test)]
    fn voices(&self) -> usize {
        self.voices.len()
    }

    /// Removes the voices that played to their end.
    fn sweep(&mut self) {
        let now = Instant::now();
        let (done, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.voices)
            .into_iter()
            .partition(|v| v.until <= now);
        self.voices = rest;
        for voice in done {
            self.remove(voice);
        }
    }

    fn remove(&self, voice: Voice) {
        let _ = voice.src.set_state(gst::State::Null);
        if let Some(src_pad) = voice.src.static_pad("src") {
            let _ = src_pad.unlink(&voice.pad);
        }
        self.mixer.release_request_pad(&voice.pad);
        let _ = self.pipeline.remove(&voice.src);
    }
}

impl Drop for Mixer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV file of a 440 Hz tone, 16-bit mono at 8 kHz, `ms` long.
    fn tone(ms: u32) -> Vec<u8> {
        let samples = 8 * ms;
        let data = samples * 2;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for i in 0..samples {
            let t = i as f32 / 8000.0;
            let v = ((t * 440.0 * std::f32::consts::TAU).sin() * 8000.0) as i16;
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav
    }

    fn near(a: Duration, b: Duration) -> bool {
        a.abs_diff(b) < Duration::from_millis(20)
    }

    #[test]
    fn decodes_a_sound_into_memory() {
        let decoded = decode(tone(500), Duration::from_secs(5)).unwrap();
        assert!(
            near(decoded.duration, Duration::from_millis(500)),
            "{:?}",
            decoded.duration
        );
        assert!(!decoded.truncated);
        assert_eq!(
            decoded.pcm.bytes() as usize,
            decoded.pcm.frames() * FRAME_BYTES
        );
    }

    #[test]
    fn cuts_a_sound_at_its_limit() {
        let decoded = decode(tone(1000), Duration::from_millis(250)).unwrap();
        assert!(decoded.truncated);
        assert!(
            near(decoded.duration, Duration::from_millis(250)),
            "{:?}",
            decoded.duration
        );
    }

    #[test]
    fn refuses_what_is_not_audio() {
        assert!(decode(b"not a sound at all".to_vec(), Duration::from_secs(5)).is_err());
    }

    #[test]
    fn a_rate_changes_the_length() {
        let pcm = decode(tone(400), Duration::from_secs(5)).unwrap().pcm;
        let fast = at_rate(&pcm, 2.0);
        let slow = at_rate(&pcm, 0.5);
        assert_eq!(fast.frames(), pcm.frames() / 2);
        assert_eq!(slow.frames(), pcm.frames() * 2);
        assert!(
            Arc::ptr_eq(&at_rate(&pcm, 1.0).0, &pcm.0),
            "rate 1 shares the samples"
        );
    }

    #[test]
    fn plays_overlap_and_end() {
        let pcm = decode(tone(100), Duration::from_secs(5)).unwrap().pcm;
        let mut mixer = Mixer::with_sink("fakesink").unwrap();
        mixer.play(1, &pcm, 1.0, 1.0).unwrap();
        mixer.play(1, &pcm, 0.5, 2.0).unwrap();
        mixer.play(2, &pcm, 1.0, 1.0).unwrap();
        assert_eq!(mixer.voices(), 3);
        mixer.stop(1);
        assert_eq!(mixer.voices(), 1);
        std::thread::sleep(Duration::from_millis(200));
        mixer.sweep();
        assert_eq!(mixer.voices(), 0);
        assert!(!mixer.is_broken());
    }

    #[test]
    fn past_the_voices_the_oldest_stops() {
        let pcm = decode(tone(1000), Duration::from_secs(5)).unwrap().pcm;
        let mut mixer = Mixer::with_sink("fakesink").unwrap();
        for _ in 0..MAX_VOICES + 3 {
            mixer.play(1, &pcm, 1.0, 1.0).unwrap();
        }
        assert_eq!(mixer.voices(), MAX_VOICES);
    }
}
