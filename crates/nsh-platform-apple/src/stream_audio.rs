//! The audio of a live stream in one video: the core's [`AudioOutput`]
//! (`nsh_core::stream`, "Audio") played through an `AVAudioEngine` of its
//! own with one `AVAudioPlayerNode`, made at the first samples the video
//! hears, so a video whose stream has no audio costs nothing.
//!
//! The samples come on the host's pushing thread and are scheduled there:
//! each push becomes an `AVAudioPCMBuffer` (AVFoundation's standard format,
//! deinterleaved `f32`; more than two channels are folded into stereo). The
//! video's `paused`, `muted` and `volume` are set from the main thread
//! ([`StreamAudio::set_paused`], [`StreamAudio::set_sound`]) and apply at
//! once: paused or muted, what is queued stops (the node is stopped, the
//! backlog reset) and new samples are dropped; the volume is the player
//! node's, never baked into the scheduled samples. Setting them never
//! waits for the device: while the pushing thread holds it (opening the
//! engine takes a while), the main thread leaves its request in flags the
//! pushing thread applies before it lets go. At most
//! [`MAX_QUEUED_MS`] wait for the device: past that the newest samples are
//! dropped, so a host pushing ahead of real time, or a device that stalls,
//! never builds up delay. The engine restarts after the system stopped it
//! (a device change), and stops at the stream's end and when the video
//! detaches. A failure (the engine does not start) is logged once and
//! silences the output until the stream's end, after which a new
//! registration tries again.

use std::{
    ptr,
    sync::{
        Arc, Mutex, MutexGuard, TryLockError,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, fence},
    },
};

use block2::RcBlock;
use nsh_core::stream::AudioOutput;
use objc2::{
    msg_send,
    rc::{Allocated, Retained, autoreleasepool},
    runtime::{AnyClass, AnyObject, Bool},
};

/// The most audio queued ahead of the device, in milliseconds: past it the
/// newest samples are dropped.
pub(crate) const MAX_QUEUED_MS: u64 = 200;

/// A video's `paused`, `muted` and `volume`, set on the main thread and
/// read on the pushing thread.
#[derive(Debug)]
pub(crate) struct Gate {
    paused: AtomicBool,
    muted: AtomicBool,
    /// The volume's bits (`f32`, 0 to 1).
    volume: AtomicU32,
}

impl Default for Gate {
    fn default() -> Self {
        Gate {
            paused: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            volume: AtomicU32::new(1f32.to_bits()),
        }
    }
}

impl Gate {
    /// Whether samples play now (neither paused nor muted).
    pub(crate) fn audible(&self) -> bool {
        !self.paused.load(Ordering::Acquire) && !self.muted.load(Ordering::Acquire)
    }

    /// The volume, 0 to 1.
    pub(crate) fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Acquire))
    }

    /// Sets `paused`; `true` when that silenced the output (what is queued
    /// is to stop).
    pub(crate) fn set_paused(&self, paused: bool) -> bool {
        let was = self.audible();
        self.paused.store(paused, Ordering::Release);
        was && !self.audible()
    }

    /// Sets `muted` and `volume` (clamped to 0 to 1, NaN is 0); `true` when
    /// that silenced the output.
    pub(crate) fn set_sound(&self, muted: bool, volume: f32) -> bool {
        let volume = if volume.is_nan() {
            0.0
        } else {
            volume.clamp(0.0, 1.0)
        };
        self.volume.store(volume.to_bits(), Ordering::Release);
        let was = self.audible();
        self.muted.store(muted, Ordering::Release);
        was && !self.audible()
    }
}

/// How many frames are scheduled on the player node and not played yet.
/// A flush starts a new generation: the completion handlers of buffers
/// scheduled before it no longer count.
#[derive(Debug, Default)]
pub(crate) struct Backlog {
    frames: AtomicU64,
    generation: AtomicU64,
}

impl Backlog {
    /// Counts `frames` at `sample_rate` as queued and returns the
    /// generation to report them played in, unless that would queue more
    /// than [`MAX_QUEUED_MS`]: `None`, and the caller drops them (the
    /// newest). An empty queue always takes them, so one push longer than
    /// the limit still plays.
    pub(crate) fn admit(&self, frames: u64, sample_rate: u32) -> Option<u64> {
        let limit = u64::from(sample_rate) * MAX_QUEUED_MS / 1000;
        let generation = self.generation.load(Ordering::Acquire);
        self.frames
            .try_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                (queued == 0 || queued.saturating_add(frames) <= limit)
                    .then(|| queued.saturating_add(frames))
            })
            .ok()
            .map(|_| generation)
    }

    /// `frames` admitted in `generation` played (or were flushed).
    pub(crate) fn played(&self, generation: u64, frames: u64) {
        if self.generation.load(Ordering::Acquire) != generation {
            return;
        }
        let _ = self
            .frames
            .try_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                Some(queued.saturating_sub(frames))
            });
    }

    /// Everything queued was flushed.
    pub(crate) fn reset(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.frames.store(0, Ordering::Release);
    }
}

/// The channels a push of `channels` plays as: mono stays mono, more than
/// two fold into stereo.
pub(crate) fn output_channels(channels: u16) -> usize {
    usize::from(channels.min(2))
}

/// Writes `samples` (interleaved, `channels` per frame) into `out`, one
/// slice per output channel ([`output_channels`]). With more than two
/// channels, the even ones (0, 2, ...) are averaged into the left and the
/// odd ones into the right.
pub(crate) fn mix(samples: &[f32], channels: usize, out: &mut [&mut [f32]]) {
    if channels <= 2 {
        for (c, out) in out.iter_mut().enumerate() {
            for (sample, frame) in out.iter_mut().zip(samples.chunks_exact(channels)) {
                *sample = frame[c];
            }
        }
        return;
    }
    let counts = [channels.div_ceil(2), channels / 2];
    for (c, out) in out.iter_mut().enumerate() {
        let scale = 1.0 / counts[c] as f32;
        for (sample, frame) in out.iter_mut().zip(samples.chunks_exact(channels)) {
            *sample = frame.iter().skip(c).step_by(2).sum::<f32>() * scale;
        }
    }
}

/// The engine and player node of one output, connected for one format.
struct Device {
    engine: Retained<AnyObject>,
    node: Retained<AnyObject>,
    format: Retained<AnyObject>,
    /// The output channels and sample rate `format` is.
    shape: (usize, u32),
}

// SAFETY: AVAudioEngine, AVAudioPlayerNode and AVAudioFormat may be used
// from any thread, one at a time; a `Device` lives in its output's mutex.
unsafe impl Send for Device {}

impl Device {
    /// An engine with a node connected for `shape` at `volume`; `None` when
    /// AVFoundation refuses.
    fn new(shape: (usize, u32), volume: f32) -> Option<Device> {
        if !crate::audio::av_foundation() {
            return None;
        }
        let engine_cls = AnyClass::get(c"AVAudioEngine")?;
        let node_cls = AnyClass::get(c"AVAudioPlayerNode")?;
        let format = format(shape)?;
        // SAFETY: AVAudioEngine's and AVAudioPlayerNode's documented API.
        unsafe {
            let alloc: Allocated<AnyObject> = msg_send![engine_cls, alloc];
            let engine: Retained<AnyObject> = msg_send![alloc, init];
            let alloc: Allocated<AnyObject> = msg_send![node_cls, alloc];
            let node: Retained<AnyObject> = msg_send![alloc, init];
            let _: () = msg_send![&engine, attachNode: &*node];
            let mixer: Retained<AnyObject> = msg_send![&engine, mainMixerNode];
            let _: () = msg_send![&engine, connect: &*node, to: &*mixer, format: &*format];
            let _: () = msg_send![&node, setVolume: volume];
            Some(Device {
                engine,
                node,
                format,
                shape,
            })
        }
    }

    /// Makes the device play `shape`: reconnected for another format,
    /// restarted after the system stopped it (a device change), the node
    /// playing. A reconnection or restart flushes what was queued (its
    /// completion handlers run). Why not, when the engine does not start.
    fn ready(&mut self, shape: (usize, u32)) -> Result<(), String> {
        // SAFETY: AVAudioEngine's and AVAudioPlayerNode's documented API;
        // `error` is an out parameter.
        unsafe {
            if shape != self.shape {
                let Some(format) = format(shape) else {
                    return Err(format!(
                        "AVAudioFormat refuses {} channels at {} Hz",
                        shape.0, shape.1
                    ));
                };
                let _: () = msg_send![&self.node, stop];
                let mixer: Retained<AnyObject> = msg_send![&self.engine, mainMixerNode];
                let _: () =
                    msg_send![&self.engine, connect: &*self.node, to: &*mixer, format: &*format];
                self.format = format;
                self.shape = shape;
            }
            let running: Bool = msg_send![&self.engine, isRunning];
            if !running.as_bool() {
                let _: () = msg_send![&self.node, stop];
                let mut error: *mut AnyObject = ptr::null_mut();
                let started: Bool = msg_send![&self.engine, startAndReturnError: &mut error];
                if !started.as_bool() {
                    return Err(crate::audio::describe(
                        error,
                        "the audio engine did not start",
                    ));
                }
            }
            let playing: Bool = msg_send![&self.node, isPlaying];
            if !playing.as_bool() {
                let _: () = msg_send![&self.node, play];
            }
        }
        Ok(())
    }

    /// Stops what the node has queued (its completion handlers run).
    fn flush(&self) {
        // SAFETY: AVAudioPlayerNode's documented API.
        let _: () = unsafe { msg_send![&self.node, stop] };
    }

    /// The node's volume, 0 to 1: applies to what is queued too.
    fn set_volume(&self, volume: f32) {
        // SAFETY: AVAudioPlayerNode's documented API (`AVAudioMixing`).
        let _: () = unsafe { msg_send![&self.node, setVolume: volume] };
    }

    /// Schedules `samples` (`frames` frames of `channels`); `done` runs
    /// once they played or were flushed. `false` when no buffer could be
    /// made (`done` does not run).
    fn schedule(
        &self,
        samples: &[f32],
        channels: u16,
        frames: u32,
        done: impl Fn() + 'static,
    ) -> bool {
        let Some(buffer_cls) = AnyClass::get(c"AVAudioPCMBuffer") else {
            return false;
        };
        // SAFETY: AVAudioPCMBuffer's and AVAudioPlayerNode's documented
        // API: a buffer of the node's format holds `frames` frames, one
        // `f32` array per channel of `shape.0`, written before it is
        // scheduled and not touched after.
        unsafe {
            let alloc: Allocated<AnyObject> = msg_send![buffer_cls, alloc];
            let buffer: Option<Retained<AnyObject>> =
                msg_send![alloc, initWithPCMFormat: &*self.format, frameCapacity: frames];
            let Some(buffer) = buffer else { return false };
            let _: () = msg_send![&buffer, setFrameLength: frames];
            let data: *const *mut f32 = msg_send![&buffer, floatChannelData];
            if data.is_null() {
                return false;
            }
            let mut out: Vec<&mut [f32]> = (0..self.shape.0)
                .map(|c| std::slice::from_raw_parts_mut(*data.add(c), frames as usize))
                .collect();
            mix(samples, usize::from(channels), &mut out);
            let done = RcBlock::new(done);
            let _: () = msg_send![&self.node, scheduleBuffer: &*buffer, completionHandler: &*done];
        }
        true
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: AVAudioPlayerNode's and AVAudioEngine's documented API.
        unsafe {
            let _: () = msg_send![&self.node, stop];
            let _: () = msg_send![&self.engine, stop];
        }
    }
}

/// AVFoundation's standard format (deinterleaved `f32`) of `shape`.
fn format((channels, rate): (usize, u32)) -> Option<Retained<AnyObject>> {
    let cls = AnyClass::get(c"AVAudioFormat")?;
    // SAFETY: AVAudioFormat's documented initializer; nil for a format it
    // does not take.
    unsafe {
        let alloc: Allocated<AnyObject> = msg_send![cls, alloc];
        msg_send![alloc, initStandardFormatWithSampleRate: f64::from(rate), channels: channels as u32]
    }
}

/// One video's output for its stream's audio (attached with
/// `Streams::attach_audio` by [`crate::stream::Streams::attach`]).
#[derive(Default)]
pub(crate) struct StreamAudio {
    gate: Gate,
    backlog: Arc<Backlog>,
    /// Made at the first samples heard, gone at the end.
    device: Mutex<Option<Device>>,
    /// Playing failed (logged): silent until the end.
    failed: AtomicBool,
    /// The main thread asked to stop what is queued, not applied yet.
    halt: AtomicBool,
    /// The main thread changed the volume, not applied to the node yet.
    stale_volume: AtomicBool,
}

impl StreamAudio {
    fn device(&self) -> MutexGuard<'_, Option<Device>> {
        self.device.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The device if no other thread holds it: the main thread never waits.
    fn try_device(&self) -> Option<MutexGuard<'_, Option<Device>>> {
        match self.device.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(e)) => Some(e.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    /// Main thread: pausing stops what is queued at once.
    pub(crate) fn set_paused(&self, paused: bool) {
        if self.gate.set_paused(paused) {
            self.halt.store(true, Ordering::Release);
            self.apply_soon();
        }
    }

    /// Main thread: muting stops what is queued at once; the volume applies
    /// to it at once.
    pub(crate) fn set_sound(&self, muted: bool, volume: f32) {
        if self.gate.set_sound(muted, volume) {
            self.halt.store(true, Ordering::Release);
        }
        self.stale_volume.store(true, Ordering::Release);
        self.apply_soon();
    }

    /// Whether the main thread left a request to apply.
    fn requested(&self) -> bool {
        self.halt.load(Ordering::Acquire) || self.stale_volume.load(Ordering::Acquire)
    }

    /// Applies the main thread's requests now if the device is free, else
    /// the pushing thread holding it does before it lets go.
    fn apply_soon(&self) {
        // Pairs with the fence in `write`: either this sees the lock free or
        // the pushing thread sees the request after letting go of it.
        fence(Ordering::SeqCst);
        if let Some(mut device) = self.try_device() {
            self.apply(&mut device);
        }
    }

    /// Applies the main thread's requests (short calls only), with the
    /// device held: the volume, then stopping what is queued.
    fn apply(&self, device: &mut Option<Device>) {
        if self.stale_volume.swap(false, Ordering::AcqRel)
            && let Some(device) = device.as_ref()
        {
            device.set_volume(self.gate.volume());
        }
        if self.halt.swap(false, Ordering::AcqRel) {
            if let Some(device) = device.as_ref() {
                device.flush();
            }
            self.backlog.reset();
        }
    }

    /// Logs `why` once and silences the output until the end.
    fn fail(&self, why: &str) {
        if !self.failed.swap(true, Ordering::AcqRel) {
            log::warn!("a live stream's audio cannot play: {why}");
        }
    }
}

impl AudioOutput for StreamAudio {
    fn write(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        self.play(samples, channels, sample_rate);
        // A request the main thread left while this thread held the device
        // (after the last `apply` under it): applied now, waiting for the
        // device here rather than there, checked again after each letting
        // go, so none is left until the next push.
        loop {
            fence(Ordering::SeqCst);
            if !self.requested() {
                break;
            }
            self.apply(&mut self.device());
        }
    }

    fn end(&self) {
        let device = self.device().take();
        autoreleasepool(|_| drop(device));
        self.backlog.reset();
        self.halt.store(false, Ordering::Release);
        self.stale_volume.store(false, Ordering::Release);
        // A new registration tries again.
        self.failed.store(false, Ordering::Release);
    }
}

impl StreamAudio {
    /// `write`'s work: schedules the samples on the device, made or
    /// restarted first, and applies the main thread's requests before
    /// letting go of it.
    fn play(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        if !self.gate.audible() || self.failed.load(Ordering::Acquire) {
            return;
        }
        let Ok(frames) = u32::try_from(samples.len() / usize::from(channels.max(1))) else {
            return;
        };
        if frames == 0 {
            return;
        }
        let shape = (output_channels(channels), sample_rate);
        autoreleasepool(|_| {
            let mut device = self.device();
            // Checked again under the lock a flush takes: samples are never
            // scheduled after a pause or mute stopped the queue.
            if !self.gate.audible() {
                return self.apply(&mut device);
            }
            if device.is_none() {
                *device = Device::new(shape, self.gate.volume());
            }
            let Some(ready) = device.as_mut() else {
                return self.fail("AVAudioEngine is not available");
            };
            if let Err(why) = ready.ready(shape) {
                *device = None;
                return self.fail(&why);
            }
            let Some(generation) = self.backlog.admit(u64::from(frames), sample_rate) else {
                return;
            };
            let backlog = self.backlog.clone();
            let done = move || backlog.played(generation, u64::from(frames));
            if !ready.schedule(samples, channels, frames, done) {
                self.backlog.played(generation, u64::from(frames));
                *device = None;
                self.fail("AVAudioPCMBuffer refused the samples");
            }
            // A pause, mute or volume the main thread set while this held
            // the device, including one that came after the check above.
            self.apply(&mut device);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pausing_or_muting_silences_and_asks_for_a_flush_once() {
        let gate = Gate::default();
        assert!(gate.audible());
        assert_eq!(gate.volume(), 1.0);
        assert!(gate.set_paused(true));
        assert!(!gate.audible());
        // Already silent: nothing more to stop.
        assert!(!gate.set_paused(true));
        assert!(!gate.set_sound(true, 0.5));
        assert!(!gate.set_paused(false));
        assert!(!gate.audible());
        assert!(!gate.set_sound(false, 0.5));
        assert!(gate.audible());
        assert!(gate.set_sound(true, 0.5));
    }

    #[test]
    fn the_volume_is_clamped_and_does_not_silence() {
        let gate = Gate::default();
        assert!(!gate.set_sound(false, 0.0));
        assert!(gate.audible());
        assert_eq!(gate.volume(), 0.0);
        gate.set_sound(false, 3.0);
        assert_eq!(gate.volume(), 1.0);
        gate.set_sound(false, -1.0);
        assert_eq!(gate.volume(), 0.0);
        gate.set_sound(false, f32::NAN);
        assert_eq!(gate.volume(), 0.0);
        gate.set_sound(false, 0.25);
        assert_eq!(gate.volume(), 0.25);
    }

    #[test]
    fn the_main_thread_never_waits_for_the_device() {
        let audio = StreamAudio::default();
        // The pushing thread holds the device (opening the engine).
        let held = audio.device();
        // Returns at once, leaving its requests.
        audio.set_sound(true, 0.5);
        audio.set_paused(true);
        assert!(audio.requested());
        assert_eq!(audio.gate.volume(), 0.5);
        drop(held);
        // The pushing thread applies them as it lets go, even for samples
        // it drops.
        audio.write(&[0.0; 2], 2, 48_000);
        assert!(!audio.requested());
        // With the device free, a request applies at once.
        audio.set_sound(true, 0.25);
        assert!(!audio.requested());
    }

    #[test]
    fn at_most_200_ms_wait_for_the_device() {
        let backlog = Backlog::default();
        // 48 kHz: 9 600 frames are 200 ms.
        let generation = backlog.admit(4_800, 48_000).unwrap();
        assert!(backlog.admit(4_800, 48_000).is_some());
        // The newest are dropped.
        assert!(backlog.admit(1, 48_000).is_none());
        backlog.played(generation, 4_800);
        assert!(backlog.admit(480, 48_000).is_some());
        // Reported played more than counted: never below empty.
        backlog.played(generation, 1_000_000);
        // An empty queue takes a push longer than the limit.
        assert!(backlog.admit(48_000, 48_000).is_some());
        assert!(backlog.admit(1, 48_000).is_none());
    }

    #[test]
    fn a_flush_empties_the_backlog_and_ignores_older_completions() {
        let backlog = Backlog::default();
        let old = backlog.admit(9_600, 48_000).unwrap();
        backlog.reset();
        let new = backlog.admit(4_800, 48_000).unwrap();
        assert_ne!(old, new);
        // The flushed buffer's handler runs late: the new count stays.
        backlog.played(old, 9_600);
        assert!(backlog.admit(4_800, 48_000).is_some());
        assert!(backlog.admit(1, 48_000).is_none());
    }

    #[test]
    fn samples_are_deinterleaved_unscaled() {
        let (mut left, mut right) = ([0.0; 2], [0.0; 2]);
        mix(
            &[0.5, -0.5, 1.0, -1.0],
            2,
            &mut [&mut left[..], &mut right[..]],
        );
        assert_eq!(left, [0.5, 1.0]);
        assert_eq!(right, [-0.5, -1.0]);

        let mut mono = [0.0; 3];
        mix(&[0.1, 0.2, 0.3], 1, &mut [&mut mono[..]]);
        assert_eq!(mono, [0.1, 0.2, 0.3]);
    }

    #[test]
    fn more_than_two_channels_fold_into_stereo() {
        assert_eq!(output_channels(1), 1);
        assert_eq!(output_channels(2), 2);
        assert_eq!(output_channels(6), 2);
        let (mut left, mut right) = ([0.0; 1], [0.0; 1]);
        // Three channels: 0 and 2 left, 1 right.
        mix(&[0.2, 0.6, 0.4], 3, &mut [&mut left[..], &mut right[..]]);
        assert!((left[0] - 0.3).abs() < 1e-6);
        assert!((right[0] - 0.6).abs() < 1e-6);
    }
}
