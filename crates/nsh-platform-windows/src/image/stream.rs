//! Bounded, off-thread playback of animated images too large to cache decoded.

use std::{
    io::Cursor,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use image::AnimationDecoder;
use nsh_core::{
    NodeId,
    image::{Key, Lease},
};

use super::{Frame, MAX_FRAMES, Pixels, UiThread, frame_delay, images, scaled, shrunk};

// shortcut: a stream needing over 128 MiB falls back to a still; use tiled decoding to lift this.
const MAX_STREAM_BYTES: usize = 128 << 20;
// shortcut: more streams wait for capacity; share decoders if many must play together.
const MAX_STREAMS: usize = 4;
static USAGE: Mutex<(usize, usize)> = Mutex::new((0, 0));

pub(super) struct Source {
    pub bytes: Arc<[u8]>,
    pub format: image::ImageFormat,
    pub longest: u32,
    pub natural: (u32, u32),
}

#[derive(Clone)]
pub(super) struct Decoded {
    pub frame: Arc<Frame>,
    pub delay: u32,
    pub index: usize,
    pub played: u32,
}

pub(super) enum Reply {
    Frame(Decoded, bool),
    Finished,
    Error(String),
}

#[derive(Clone, Copy)]
pub(super) enum Command {
    Seek,
    Next,
}

pub(super) struct Worker {
    sender: mpsc::Sender<Command>,
    cancelled: Arc<AtomicBool>,
}

impl Worker {
    pub(super) fn send(&self, command: Command) -> Result<(), mpsc::SendError<Command>> {
        self.sender.send(command)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum StartError {
    Busy,
    TooLarge,
    Failed(String),
}

struct Reservation {
    bytes: usize,
    key: Option<Key>,
    _lease: Lease,
}

pub(super) fn working_bytes(natural: (u32, u32), longest: u32) -> Option<usize> {
    let (width, height) = natural;
    let (target_width, target_height) = scaled(natural, longest);
    let full = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(12));
    let target = (target_width as usize)
        .checked_mul(target_height as usize)
        .and_then(|n| n.checked_mul(16));
    let bytes = full.and_then(|n| target.and_then(|t| n.checked_add(t)))?;
    (bytes <= MAX_STREAM_BYTES).then_some(bytes)
}

impl Reservation {
    fn new(source: &Source, key: Option<Key>, lease: Lease) -> Result<Self, StartError> {
        let bytes = working_bytes(source.natural, source.longest).ok_or(StartError::TooLarge)?;
        let mut usage = USAGE.lock().unwrap_or_else(|e| e.into_inner());
        if usage.1 >= MAX_STREAMS || bytes > MAX_STREAM_BYTES.saturating_sub(usage.0) {
            return Err(StartError::Busy);
        }
        usage.0 += bytes;
        usage.1 += 1;
        drop(usage);
        if let Some(key) = &key {
            images().memory().add_cost(key, bytes as isize);
        }
        Ok(Self {
            bytes,
            key,
            _lease: lease,
        })
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(key) = &self.key {
            images().memory().add_cost(key, -(self.bytes as isize));
        }
        let mut usage = USAGE.lock().unwrap_or_else(|e| e.into_inner());
        usage.0 -= self.bytes;
        usage.1 -= 1;
    }
}

pub(super) struct Retained {
    bytes: usize,
    key: Option<Key>,
    _lease: Lease,
}

impl Retained {
    pub(super) fn new(key: Option<Key>, lease: Lease, bytes: usize) -> Self {
        if let Some(key) = &key {
            images().memory().add_cost(key, bytes as isize);
        }
        Self {
            bytes,
            key,
            _lease: lease,
        }
    }
}

impl Drop for Retained {
    fn drop(&mut self) {
        if let Some(key) = &self.key {
            images().memory().add_cost(key, -(self.bytes as isize));
        }
    }
}

pub(super) struct Decoder {
    source: Arc<Source>,
    frames: image::Frames<'static>,
    current: Option<Decoded>,
    played: u32,
    plays: u32,
}

impl Decoder {
    pub(super) fn new(source: Arc<Source>, played: u32, plays: u32) -> Result<Self, String> {
        let frames = Self::open(&source)?;
        Ok(Self {
            source,
            frames,
            current: None,
            played,
            plays,
        })
    }

    fn open(source: &Source) -> Result<image::Frames<'static>, String> {
        let cursor = Cursor::new(source.bytes.clone());
        match source.format {
            image::ImageFormat::Gif => Ok(image::codecs::gif::GifDecoder::new(cursor)
                .map_err(|e| e.to_string())?
                .into_frames()),
            image::ImageFormat::WebP => Ok(image::codecs::webp::WebPDecoder::new(cursor)
                .map_err(|e| e.to_string())?
                .into_frames()),
            _ => Err("unsupported animation format".into()),
        }
    }

    fn take(&mut self, index: usize) -> Result<Option<Decoded>, String> {
        let Some(frame) = self.frames.next() else {
            return Ok(None);
        };
        let frame = frame.map_err(|e| e.to_string())?;
        let delay = frame_delay(frame.delay());
        let pixels = Pixels::from_rgba(shrunk(frame.into_buffer(), self.source.longest));
        Ok(Some(Decoded {
            frame: Frame::new(pixels),
            delay,
            index,
            played: self.played,
        }))
    }

    #[cfg(test)]
    pub(super) fn seek(&mut self, index: usize) -> Result<Decoded, String> {
        self.seek_until(index, || false)
    }

    fn seek_until(
        &mut self,
        index: usize,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Decoded, String> {
        if index >= MAX_FRAMES {
            return Err("animation frame is out of range".into());
        }
        self.frames = Self::open(&self.source)?;
        for _ in 0..index {
            if cancelled() {
                return Err("animation decoding cancelled".into());
            }
            self.frames
                .next()
                .ok_or("animation ended while seeking")?
                .map_err(|e| e.to_string())?;
        }
        if cancelled() {
            return Err("animation decoding cancelled".into());
        }
        let current = self.take(index)?.ok_or("animation ended while seeking")?;
        self.current = Some(current.clone());
        Ok(current)
    }

    pub(super) fn next(&mut self) -> Result<Option<Decoded>, String> {
        let index = self.current.as_ref().map_or(0, |frame| frame.index + 1);
        let mut next = if index < MAX_FRAMES {
            self.take(index)?
        } else {
            None
        };
        if next.is_none() {
            self.played = self.played.saturating_add(1);
            if self.plays > 0 && self.played >= self.plays {
                return Ok(None);
            }
            self.frames = Self::open(&self.source)?;
            next = Some(self.take(0)?.ok_or("animation has no frames")?);
        }
        self.current = next.clone();
        Ok(next)
    }
}

pub(super) fn start(
    source: Arc<Source>,
    key: Option<Key>,
    lease: Lease,
    owner: (u64, NodeId),
    id: u64,
    position: usize,
    played: u32,
    plays: u32,
) -> Result<Worker, StartError> {
    let reservation = Reservation::new(&source, key, lease)?;
    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let ui = UiThread::current();
    thread::Builder::new()
        .name("nsh-image-animation".into())
        .spawn(move || {
            let _reservation = reservation;
            let initial = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Decoder::new(source, played, plays).and_then(|mut decoder| {
                    decoder
                        .seek_until(position, || worker_cancelled.load(Ordering::Acquire))
                        .map(|_| decoder)
                })
            }));
            if worker_cancelled.load(Ordering::Acquire) {
                return;
            }
            let mut decoder = match initial {
                Ok(Ok(decoder)) => decoder,
                Ok(Err(error)) => {
                    ui.invoke(move || super::stream_arrived(owner, id, Reply::Error(error)));
                    return;
                }
                Err(_) => {
                    ui.invoke(move || {
                        super::stream_arrived(
                            owner,
                            id,
                            Reply::Error("animation decoder panicked".into()),
                        )
                    });
                    return;
                }
            };
            while let Ok(command) = receiver.recv() {
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                let reply = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match command {
                        Command::Seek => Ok(decoder.current.clone()),
                        Command::Next => decoder.next(),
                    }
                })) {
                    Ok(Ok(Some(frame))) => Reply::Frame(frame, matches!(command, Command::Seek)),
                    Ok(Ok(None)) => Reply::Finished,
                    Ok(Err(error)) => Reply::Error(error),
                    Err(_) => Reply::Error("animation decoder panicked".into()),
                };
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                ui.invoke(move || super::stream_arrived(owner, id, reply));
            }
        })
        .map_err(|e| StartError::Failed(e.to_string()))?;
    Ok(Worker { sender, cancelled })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_release_slots_and_refuse_oversized_streams() {
        let mut source = Source {
            bytes: Arc::from([]),
            format: image::ImageFormat::Gif,
            longest: 1,
            natural: (1, 1),
        };
        let mut held = Vec::new();
        for _ in 0..MAX_STREAMS {
            held.push(Reservation::new(&source, None, Lease::none()).unwrap());
        }
        assert!(matches!(
            Reservation::new(&source, None, Lease::none()),
            Err(StartError::Busy)
        ));
        let animation = Arc::new(super::super::Animation {
            frames: vec![Frame::new(Pixels {
                width: 1,
                height: 1,
                data: vec![0; 4],
            })],
            delays: vec![100],
            plays: 1,
            stream: Some(Arc::new(Source {
                bytes: Arc::from([]),
                format: image::ImageFormat::Gif,
                longest: 1,
                natural: (1, 1),
            })),
        });
        let mut playback = super::super::Playback::new(animation, Lease::none());
        playback.request_stream((0, 0), false);
        assert!(!playback.stream.as_ref().unwrap().failed);
        assert!(playback.timeline.due.is_some());
        playback.request_stream((0, 0), true);
        assert!(playback.stream.as_ref().unwrap().needs_seek);
        playback.timeline.played = 1;
        if let Some(timer) = playback.timer.take() {
            super::super::cancel(timer);
        }
        assert!(playback.paused());
        drop(playback);
        drop(held.pop());
        assert!(Reservation::new(&source, None, Lease::none()).is_ok());
        drop(held);
        source.natural = (8192, 8192);
        source.longest = 8192;
        assert!(matches!(
            Reservation::new(&source, None, Lease::none()),
            Err(StartError::TooLarge)
        ));
    }

    #[test]
    fn seeking_stops_when_playback_is_cancelled() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/gallery/bounce.gif");
        let bytes = std::fs::read(path).unwrap();
        let source = Arc::new(Source {
            bytes: Arc::from(bytes),
            format: image::ImageFormat::Gif,
            longest: 32,
            natural: (32, 32),
        });
        let mut decoder = Decoder::new(source, 0, 0).unwrap();
        let mut checks = 0;
        let result = decoder.seek_until(10, || {
            checks += 1;
            checks == 3
        });
        assert!(matches!(result, Err(error) if error == "animation decoding cancelled"));
    }
}
