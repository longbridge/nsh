//! A stream's audio in one video ([`StreamAudio`], the core's
//! [`AudioOutput`]): the samples the host pushes go, on its thread, into an
//! XAudio2 source voice of 32-bit float (made at the first samples, made
//! again when the channels or the rate change). There is no callback: a
//! buffer stays alive until the voice's state says it was played
//! ([`Queue::retire`]), and while more than [`MAX_QUEUED_MS`] is queued,
//! new samples are dropped, so a host pushing faster than the device plays
//! never builds up a delay. Paused or muted, the samples are dropped and
//! what is queued stops; the volume is the voice's (0 to 1). The UI thread
//! never waits for the pushing thread: it sets atomics and applies them
//! only when the voice's lock is free, engines and voices are made and
//! destroyed outside that lock, and a detached video's voice is destroyed
//! off the UI thread.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, MutexGuard, TryLockError,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

use nsh_core::stream::AudioOutput;
use windows::Win32::Media::Audio::{
    WAVEFORMATEX,
    XAudio2::{
        IXAudio2SourceVoice, IXAudio2VoiceCallback, XAUDIO2_BUFFER, XAUDIO2_MAX_QUEUED_BUFFERS,
        XAUDIO2_VOICE_NOSAMPLESPLAYED, XAUDIO2_VOICE_STATE,
    },
};

use crate::audio::{Output, make_output};

/// How much audio may wait for the device (milliseconds): past it, new
/// samples are dropped rather than played late.
pub(crate) const MAX_QUEUED_MS: usize = 200;

/// The buffers kept for reuse once played.
const SPARE: usize = 4;

/// The channels a voice plays: XAudio2 takes plain stereo safely, so of
/// more channels the front pair plays (as `crate::audio`'s sounds do).
pub(crate) fn voice_channels(channels: u16) -> u16 {
    channels.min(2)
}

/// Whether samples are heard: not paused, not muted, not detached.
pub(crate) fn audible(paused: bool, muted: bool, closed: bool) -> bool {
    !paused && !muted && !closed
}

/// The buffers a voice was given, oldest first, until it played them.
/// Plain memory: what the XAudio2 side reads is decided by the caller.
#[derive(Default)]
pub(crate) struct Queue {
    buffers: VecDeque<Vec<f32>>,
    /// Interleaved samples per frame of the buffers.
    channels: usize,
    /// The frames of the buffers in `buffers`.
    frames: usize,
    /// Played buffers, reused for the next samples.
    spare: Vec<Vec<f32>>,
}

impl Queue {
    /// A queue of buffers of `channels` samples a frame.
    pub(crate) fn new(channels: u16) -> Queue {
        Queue {
            channels: usize::from(channels.max(1)),
            ..Queue::default()
        }
    }

    /// The voice still holds the last `queued` buffers: the older ones
    /// were played and may be reused.
    pub(crate) fn retire(&mut self, queued: usize) {
        while self.buffers.len() > queued {
            let Some(buffer) = self.buffers.pop_front() else {
                break;
            };
            self.frames -= buffer.len() / self.channels;
            if self.spare.len() < SPARE {
                self.spare.push(buffer);
            }
        }
    }

    /// The frames waiting.
    #[cfg(test)]
    pub(crate) fn frames(&self) -> usize {
        self.frames
    }

    /// The buffers waiting.
    #[cfg(test)]
    pub(crate) fn buffers(&self) -> usize {
        self.buffers.len()
    }

    /// Whether `frames` more at `rate` frames a second may wait: no more
    /// than [`MAX_QUEUED_MS`] in all (a first buffer is always taken, however
    /// long) and no more buffers than XAudio2 queues.
    pub(crate) fn admits(&self, frames: usize, rate: u32) -> bool {
        if self.buffers.len() >= XAUDIO2_MAX_QUEUED_BUFFERS as usize {
            return false;
        }
        self.buffers.is_empty() || (self.frames + frames) * 1000 <= MAX_QUEUED_MS * rate as usize
    }

    /// Copies `samples` (`channels` a frame) into a buffer at the back,
    /// keeping the queue's channels of each frame (the first ones), and
    /// returns it.
    pub(crate) fn push(&mut self, samples: &[f32], channels: u16) -> &[f32] {
        let from = usize::from(channels.max(1));
        let mut buffer = self.spare.pop().unwrap_or_default();
        buffer.clear();
        if from == self.channels {
            buffer.extend_from_slice(samples);
        } else {
            let keep = self.channels.min(from);
            for frame in samples.chunks_exact(from) {
                buffer.extend_from_slice(&frame[..keep]);
            }
        }
        self.frames += buffer.len() / self.channels;
        self.buffers.push_back(buffer);
        self.buffers.back().map_or(&[], Vec::as_slice)
    }

    /// Takes back the buffer [`Self::push`] added (XAudio2 refused it).
    pub(crate) fn unpush(&mut self) {
        if let Some(buffer) = self.buffers.pop_back() {
            self.frames -= buffer.len() / self.channels;
        }
    }
}

/// The XAudio2 engine of an output, shared with its voice so the voice is
/// always destroyed before the engine's mastering voice.
struct Engine(Output);

// SAFETY: XAudio2's engine and voices are free-threaded (their methods may
// be called from any thread, except from XAudio2's callbacks, of which
// there are none here).
unsafe impl Send for Engine {}
// SAFETY: as above.
unsafe impl Sync for Engine {}

/// A float source voice of one format and the buffers it reads.
struct Voice {
    source: IXAudio2SourceVoice,
    channels: u16,
    rate: u32,
    started: bool,
    /// The volume last set on it.
    volume: f32,
    /// Dropped after the voice (`Drop`), so XAudio2 never reads freed
    /// memory.
    queue: Queue,
    /// Dropped after the voice: its engine.
    _engine: Arc<Engine>,
}

// SAFETY: as `Engine`'s; a voice is only reached through its output's
// mutex.
unsafe impl Send for Voice {}

impl Drop for Voice {
    fn drop(&mut self) {
        // SAFETY: the voice is destroyed once, here (DestroyVoice waits for
        // the audio thread to let go of it); `queue` and the engine drop
        // afterwards.
        unsafe { self.source.DestroyVoice() };
    }
}

impl Voice {
    fn new(engine: &Arc<Engine>, channels: u16, rate: u32, volume: f32) -> Option<Voice> {
        let channels = voice_channels(channels);
        let block = channels * 4;
        let wave = WAVEFORMATEX {
            wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
            nChannels: channels,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * u32::from(block),
            nBlockAlign: block,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        let mut source: Option<IXAudio2SourceVoice> = None;
        // SAFETY: `wave` and the out pointer are valid for the call; no
        // callback, send list or effect chain.
        let made = unsafe {
            engine.0.xaudio.CreateSourceVoice(
                &mut source,
                &wave,
                0,
                1.0,
                None::<&IXAudio2VoiceCallback>,
                None,
                None,
            )
        };
        if let Err(err) = made {
            log::warn!(
                "stream audio: cannot make a voice of {channels} channels at {rate} Hz: {err}"
            );
            return None;
        }
        let source = source?;
        // SAFETY: a call on the live voice.
        let _ = unsafe { source.SetVolume(volume, 0) };
        Some(Voice {
            source,
            channels,
            rate,
            started: false,
            volume,
            queue: Queue::new(channels),
            _engine: engine.clone(),
        })
    }

    /// Forgets the buffers the voice played.
    fn retire(&mut self) {
        let mut state = XAUDIO2_VOICE_STATE::default();
        // SAFETY: a call on a live voice with an out pointer to a local.
        unsafe {
            self.source
                .GetState(&mut state, XAUDIO2_VOICE_NOSAMPLESPLAYED)
        };
        self.queue.retire(state.BuffersQueued as usize);
    }

    /// Stops it and drops what it queued (the buffers stay until its state
    /// says they are gone).
    fn halt(&mut self) {
        if self.started {
            // SAFETY: calls on a live voice.
            unsafe {
                let _ = self.source.Stop(0, 0);
                let _ = self.source.FlushSourceBuffers();
            }
            self.started = false;
        }
    }
}

/// The XAudio2 side of an output: the engine and the voice, made at the
/// first samples. Only ever held for short, non-blocking calls; making and
/// destroying engines and voices happens outside its lock.
#[derive(Default)]
struct Player {
    voice: Option<Voice>,
    engine: Option<Arc<Engine>>,
    /// The format no voice could be made for: not tried again until the
    /// format changes or the registration ends.
    failed: Option<(u16, u32)>,
}

impl Player {
    fn is_empty(&self) -> bool {
        self.voice.is_none() && self.engine.is_none()
    }
}

/// A volume as a voice takes it: 0 to 1 (not a number is silence).
pub(crate) fn clamp_volume(volume: f32) -> f32 {
    if volume.is_nan() {
        0.0
    } else {
        volume.clamp(0.0, 1.0)
    }
}

/// Drops a detached player's voice and engine off the calling thread (the
/// UI thread): destroying them waits for XAudio2's audio thread.
fn retire_player(player: Player) {
    if player.is_empty() {
        return;
    }
    if let Err(err) = std::thread::Builder::new()
        .name("nsh-stream-audio".into())
        .spawn(move || drop(player))
    {
        log::debug!("stream audio: voice destroyed in place: {err}");
    }
}

/// A video's output for a stream's audio, attached with
/// `Streams::attach_audio`.
///
/// The UI thread never waits on it: it sets the video's settings in
/// atomics and applies them to the voice only when the lock is free
/// (`try_lock`); otherwise the pushing thread applies them as it lets go.
pub(crate) struct StreamAudio {
    paused: AtomicBool,
    muted: AtomicBool,
    /// The volume's `f32` bits (0 to 1).
    volume: AtomicU32,
    /// Detached: nothing plays any more.
    closed: AtomicBool,
    /// What is queued is to stop (paused, muted, ended).
    halt: AtomicBool,
    /// The registration ended: a format that failed is tried again.
    reset: AtomicBool,
    player: Mutex<Player>,
    warned: AtomicBool,
}

impl StreamAudio {
    /// An output with the video's settings.
    pub(crate) fn new(paused: bool, muted: bool, volume: f32) -> StreamAudio {
        StreamAudio {
            paused: AtomicBool::new(paused),
            muted: AtomicBool::new(muted),
            volume: AtomicU32::new(clamp_volume(volume).to_bits()),
            closed: AtomicBool::new(false),
            halt: AtomicBool::new(false),
            reset: AtomicBool::new(false),
            player: Mutex::new(Player::default()),
            warned: AtomicBool::new(false),
        }
    }

    fn audible(&self) -> bool {
        audible(
            self.paused.load(Ordering::Acquire),
            self.muted.load(Ordering::Acquire),
            self.closed.load(Ordering::Acquire),
        )
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Acquire))
    }

    fn lock(&self) -> MutexGuard<'_, Player> {
        self.player.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The lock if it is free: the UI thread never waits for it.
    fn try_lock(&self) -> Option<MutexGuard<'_, Player>> {
        match self.player.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(e)) => Some(e.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    /// Applies the settings and requests to the player (short calls only):
    /// the detached player to drop, outside the lock, once closed.
    fn apply(&self, player: &mut Player) -> Option<Player> {
        if self.reset.swap(false, Ordering::AcqRel) {
            player.failed = None;
        }
        if self.closed.load(Ordering::Acquire) {
            return Some(std::mem::take(player));
        }
        let halt = self.halt.swap(false, Ordering::AcqRel);
        if let Some(voice) = &mut player.voice {
            let volume = self.volume();
            if voice.volume != volume {
                // SAFETY: a call on a live voice.
                let _ = unsafe { voice.source.SetVolume(volume, 0) };
                voice.volume = volume;
            }
            if halt || !self.audible() {
                voice.halt();
            }
        }
        None
    }

    /// Applies the requests now if the lock is free, else the pushing
    /// thread does as it lets go.
    fn apply_soon(&self) {
        let Some(mut guard) = self.try_lock() else {
            return;
        };
        let detached = self.apply(&mut guard);
        drop(guard);
        if let Some(player) = detached {
            retire_player(player);
        }
    }

    /// The video's props changed (UI thread; never waits): paused or muted
    /// stops what is queued; the volume applies at once.
    pub(crate) fn set(&self, paused: bool, muted: bool, volume: f32) {
        self.paused.store(paused, Ordering::Release);
        self.muted.store(muted, Ordering::Release);
        self.volume
            .store(clamp_volume(volume).to_bits(), Ordering::Release);
        if paused || muted {
            self.halt.store(true, Ordering::Release);
        }
        self.apply_soon();
    }

    /// The video lets go of the stream (UI thread; never waits): nothing
    /// plays from now on; the voice and engine are destroyed off this
    /// thread.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.apply_soon();
    }

    /// Makes an engine and a voice of `channels` at `rate` outside the lock
    /// and swaps them in: the voice it replaced, to drop outside the lock.
    fn make_voice(&self, channels: u16, rate: u32) -> Option<Voice> {
        let format = (voice_channels(channels), rate);
        let engine = self.lock().engine.clone();
        let engine = match engine {
            Some(engine) => engine,
            None => match make_output() {
                Ok(output) => Arc::new(Engine(output)),
                Err(err) => {
                    self.lock().failed = Some(format);
                    if !self.warned.swap(true, Ordering::AcqRel) {
                        log::warn!("stream audio: the output did not start: {err}");
                    }
                    return None;
                }
            },
        };
        let voice = Voice::new(&engine, channels, rate, self.volume());
        let mut guard = self.lock();
        guard.engine = Some(engine);
        match voice {
            Some(voice) => {
                guard.failed = None;
                guard.voice.replace(voice)
            }
            None => {
                guard.failed = Some(format);
                None
            }
        }
    }
}

impl AudioOutput for StreamAudio {
    fn write(&self, samples: &[f32], channels: u16, sample_rate: u32) {
        if !self.audible() {
            return;
        }
        let format = (voice_channels(channels), sample_rate);
        // Dropped after the guard: what is destroyed waits for XAudio2.
        let mut retired: Option<Voice> = None;
        let mut detached: Option<Player>;
        {
            let mut guard = self.lock();
            detached = self.apply(&mut guard);
            if detached.is_some() || guard.failed == Some(format) {
                return;
            }
            let fits = guard
                .voice
                .as_ref()
                .is_some_and(|v| (v.channels, v.rate) == format);
            drop(guard);
            if !fits {
                retired = self.make_voice(channels, sample_rate);
            }
        }
        let mut guard = self.lock();
        detached = self.apply(&mut guard);
        if detached.is_some() || !self.audible() {
            return;
        }
        let Some(voice) = &mut guard.voice else {
            return;
        };
        if (voice.channels, voice.rate) != format {
            return;
        }
        voice.retire();
        let frames = samples.len() / usize::from(channels.max(1));
        // Past the limit, the newest samples are dropped.
        if !voice.queue.admits(frames, sample_rate) {
            return;
        }
        let buffer = voice.queue.push(samples, channels);
        let submit = XAUDIO2_BUFFER {
            AudioBytes: (buffer.len() * 4) as u32,
            pAudioData: buffer.as_ptr().cast(),
            ..Default::default()
        };
        // SAFETY: calls on a live voice. The buffer is the queue's, kept
        // until the voice's state says it was played or the voice is
        // destroyed (`Voice`'s drop order).
        let submitted = unsafe {
            voice
                .source
                .SubmitSourceBuffer(&submit, None)
                .and_then(|()| {
                    if voice.started {
                        Ok(())
                    } else {
                        voice.source.Start(0, 0)
                    }
                })
        };
        match submitted {
            Ok(()) => voice.started = true,
            Err(err) => {
                log::debug!("stream audio: samples not queued: {err}");
                voice.queue.unpush();
            }
        }
        // Settings the UI thread set while this held the lock.
        detached = self.apply(&mut guard);
        drop(guard);
        drop(detached);
        drop(retired);
    }

    fn end(&self) {
        self.halt.store(true, Ordering::Release);
        self.reset.store(true, Ordering::Release);
        self.apply_soon();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn played_buffers_are_retired_oldest_first() {
        let mut queue = Queue::new(2);
        queue.push(&[0.0; 8], 2);
        queue.push(&[0.0; 4], 2);
        queue.push(&[0.0; 2], 2);
        assert_eq!((queue.buffers(), queue.frames()), (3, 7));
        // The voice holds the last two: the first was played.
        queue.retire(2);
        assert_eq!((queue.buffers(), queue.frames()), (2, 3));
        queue.retire(0);
        assert_eq!((queue.buffers(), queue.frames()), (0, 0));
    }

    #[test]
    fn samples_past_two_hundred_milliseconds_are_dropped() {
        let mut queue = Queue::new(1);
        // A first buffer is taken however long.
        assert!(queue.admits(48_000, 48_000));
        queue.push(&[0.0; 4_800], 1); // 100 ms
        assert!(queue.admits(4_800, 48_000)); // 200 ms in all
        queue.push(&[0.0; 4_800], 1);
        assert!(!queue.admits(1, 48_000));
        // Played: room again.
        queue.retire(1);
        assert!(queue.admits(4_800, 48_000));
    }

    #[test]
    fn no_more_buffers_than_xaudio2_queues() {
        let mut queue = Queue::new(1);
        for _ in 0..XAUDIO2_MAX_QUEUED_BUFFERS {
            queue.push(&[0.0], 1);
        }
        assert!(!queue.admits(1, 192_000));
    }

    #[test]
    fn more_than_two_channels_play_the_front_pair() {
        assert_eq!(voice_channels(1), 1);
        assert_eq!(voice_channels(6), 2);
        let mut queue = Queue::new(voice_channels(3));
        let buffer = queue.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3).to_vec();
        assert_eq!(buffer, [1.0, 2.0, 4.0, 5.0]);
        assert_eq!(queue.frames(), 2);
        queue.unpush();
        assert_eq!((queue.buffers(), queue.frames()), (0, 0));
    }

    #[test]
    fn paused_muted_or_detached_outputs_hear_nothing() {
        assert!(audible(false, false, false));
        assert!(!audible(true, false, false));
        assert!(!audible(false, true, false));
        assert!(!audible(false, false, true));
        // Without a voice (none made yet), settings and writes while
        // silent touch no device.
        let output = StreamAudio::new(true, false, 2.0);
        output.write(&[0.0; 4], 2, 48_000);
        output.set(false, true, 0.5);
        output.write(&[0.0; 4], 2, 48_000);
        output.end();
        output.close();
        output.set(false, false, 1.0);
        output.write(&[0.0; 4], 2, 48_000);
        assert!(output.player.lock().unwrap().voice.is_none());
    }

    #[test]
    fn the_ui_thread_never_waits_for_the_voice() {
        let output = StreamAudio::new(false, false, 1.0);
        let guard = output.player.lock().unwrap();
        // The pushing thread holds the lock: the settings are kept.
        output.set(true, false, 0.25);
        output.close();
        assert!(output.paused.load(Ordering::Acquire));
        assert_eq!(output.volume(), 0.25);
        assert!(output.halt.load(Ordering::Acquire));
        drop(guard);
        // Applied as the lock is let go: a closed output's player goes.
        let mut player = output.player.lock().unwrap();
        assert!(output.apply(&mut player).is_some());
    }

    #[test]
    fn the_end_of_a_registration_tries_a_failed_format_again() {
        let output = StreamAudio::new(false, false, 1.0);
        output.player.lock().unwrap().failed = Some((2, 48_000));
        output.end();
        assert_eq!(output.player.lock().unwrap().failed, None);
    }

    #[test]
    fn volumes_are_zero_to_one() {
        assert_eq!(clamp_volume(2.0), 1.0);
        assert_eq!(clamp_volume(-1.0), 0.0);
        assert_eq!(clamp_volume(0.5), 0.5);
        assert_eq!(clamp_volume(f32::NAN), 0.0);
    }
}
