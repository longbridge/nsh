//! Audio on Linux (`Command::Audio`, `nsh`'s `Sound` and `AudioPlayer`),
//! through GTK's media support (`gtk::MediaFile`, which GTK backs with
//! GStreamer), as the video does: no other media crate is linked.
//!
//! - **Sounds** keep their encoded bytes in a temporary file written at
//!   [`AudioCommand::Load`]. Loading prepares a `MediaFile` on it once (so
//!   GStreamer proves the bytes decode and tells the duration), then drops
//!   it; every [`AudioCommand::Play`] is a new `MediaFile` of that local
//!   file, a voice of its own, so plays overlap, open nothing from the
//!   network and start as fast as GStreamer prepares a local file. The
//!   decoded size reported is an estimate (duration at 48 kHz, stereo,
//!   `f32`). A sound longer than the `max_duration` asked for is reported
//!   `truncated`, and its voices are stopped at that length of playback:
//!   timed from the moment the stream prepared and started, so the time
//!   GStreamer takes to prepare never shortens a sound.
//! - **Players** are one `MediaFile` each, made at the first preload or
//!   resume, which streams its source (a file, an `http(s)` URL).
//! - GTK streams have no playback rate: `rate` is ignored (warned once).
//!   They report no buffering either: a player waiting for its load
//!   reports `Buffer` from the resume to the load.
//! - Everything runs on the GTK main loop. An output or a decoder that is
//!   missing makes a sound silence and a player `Failed`, never a panic.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    rc::{Rc, Weak},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant},
};

use gtk4::{self as gtk, gio, glib, prelude::*};
use nsh_core::{
    EventKind, EventPayload, ROOT_ID,
    audio::{AudioCommand, AudioError, AudioEvent, MAX_VOICES, PROGRESS_INTERVAL, PlayerSettings},
};
use nsh_engine::EngineControl;

use crate::image::AssetBase;

/// How long a sound or a player has to prepare before it is called failed.
const PREPARE_TIMEOUT: Duration = Duration::from_secs(10);

/// One engine's sounds and players.
#[derive(Default)]
pub(crate) struct Speaker {
    /// Shared with the sounds still preparing, which land in it.
    inner: Rc<RefCell<Inner>>,
}

#[derive(Default)]
struct Inner {
    sounds: HashMap<u32, Sound>,
    voices: Vec<Voice>,
    players: HashMap<u32, Rc<Player>>,
}

/// A sound: its file, and where its voices stop when it was truncated.
struct Sound {
    path: PathBuf,
    /// How long a voice plays at most: `Some` only for a sound longer than
    /// the core allows (the others end by themselves).
    cut: Option<Duration>,
}

/// A sound playing.
struct Voice {
    sound: u32,
    stream: gtk::MediaFile,
    started: Instant,
    /// Set when the stream ended.
    done: Rc<Cell<bool>>,
}

impl Voice {
    fn end(self) {
        self.stream.set_playing(false);
        self.stream.clear();
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        let mut inner = self.inner.borrow_mut();
        for player in inner.players.values() {
            player.close();
        }
        for voice in inner.voices.drain(..) {
            voice.end();
        }
        for sound in inner.sounds.values() {
            let _ = std::fs::remove_file(&sound.path);
        }
    }
}

/// Applies `command` for the engine `control` reports to, `asset:/` URIs
/// resolving against `base`. On the main thread.
pub(crate) fn command(
    speaker: &Speaker,
    control: &EngineControl,
    command: &AudioCommand,
    base: Option<&AssetBase>,
) {
    let id = command.id();
    match command {
        AudioCommand::Load {
            bytes,
            hint,
            max_duration,
            ..
        } => load(speaker, control, id, bytes, hint, *max_duration),
        AudioCommand::LoadUrl { .. } => report(
            control,
            id,
            failed(
                AudioError::NotSupported,
                "sounds are loaded from their bytes here",
            ),
        ),
        AudioCommand::Play { volume, rate, .. } => speaker.play(id, *volume, *rate),
        AudioCommand::Stop { .. } => speaker.stop(id),
        AudioCommand::Unload { .. } => {
            speaker.stop(id);
            if let Some(sound) = speaker.inner.borrow_mut().sounds.remove(&id) {
                let _ = std::fs::remove_file(&sound.path);
            }
        }
        AudioCommand::Open { src, settings, .. } => {
            let player = Player::new(id, src, *settings, base, control.clone());
            speaker.inner.borrow_mut().players.insert(id, player);
        }
        AudioCommand::Close { .. } => {
            let player = speaker.inner.borrow_mut().players.remove(&id);
            if let Some(player) = player {
                player.close();
            }
        }
        other => {
            let player = speaker.inner.borrow().players.get(&id).cloned();
            if let Some(player) = player {
                player.apply(other);
            }
        }
    }
}

fn report(control: &EngineControl, id: u32, event: AudioEvent) {
    control.dispatch_event(ROOT_ID, EventKind::Audio, EventPayload::Audio { id, event });
}

fn failed(error: AudioError, message: &str) -> AudioEvent {
    AudioEvent::Failed {
        error,
        message: message.to_owned(),
    }
}

/// GStreamer does not do rates through GTK: said once.
fn warn_rate(rate: f32) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if (rate - 1.0).abs() > f32::EPSILON && !WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(target: "js", "audio `rate` is not supported on Linux: it plays at 1×");
    }
}

/// The file extension `hint` (a file name, a URL or a `data:` URI) names,
/// which GStreamer's typefinding can use.
fn extension(hint: &str) -> &'static str {
    let hint = hint.to_ascii_lowercase();
    let mime = hint
        .strip_prefix("data:")
        .map(|rest| rest.split([';', ',']).next().unwrap_or_default());
    let name = hint.split(['?', '#']).next().unwrap_or_default();
    let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
    match (mime, ext) {
        (Some("audio/mpeg" | "audio/mp3"), _) | (None, "mp3") => "mp3",
        (Some("audio/aac" | "audio/mp4" | "audio/x-m4a" | "audio/m4a"), _)
        | (None, "m4a" | "aac" | "mp4") => "m4a",
        (Some("audio/ogg" | "audio/opus"), _) | (None, "ogg" | "opus" | "oga") => "ogg",
        (Some("audio/flac"), _) | (None, "flac") => "flac",
        _ => "wav",
    }
}

/// A stream's microseconds as a duration; `None` when unknown (0).
fn micros(us: i64) -> Option<Duration> {
    (us > 0).then(|| Duration::from_micros(us as u64))
}

/// Writes `bytes` to a temporary file, prepares a stream on it to learn
/// that it decodes and how long it is, and reports `Loaded` or `Failed`.
fn load(
    speaker: &Speaker,
    control: &EngineControl,
    id: u32,
    bytes: &[u8],
    hint: &str,
    max: Duration,
) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "nsh-sound-{}-{n}.{}",
        std::process::id(),
        extension(hint)
    ));
    if let Err(e) = std::fs::write(&path, bytes) {
        report(
            control,
            id,
            failed(AudioError::Failed, &format!("cannot write the sound: {e}")),
        );
        return;
    }
    let stream = gtk::MediaFile::for_file(&gio::File::for_path(&path));
    let probe = Rc::new(Probe {
        stream: RefCell::new(Some(stream.clone())),
        handlers: RefCell::default(),
        timeout: RefCell::default(),
        path,
        max,
        control: control.clone(),
        id,
        // Where the sound goes once it decoded.
        speaker: Rc::downgrade(&speaker.inner),
    });
    PROBES.with(|p| p.borrow_mut().push(probe.clone()));
    let weak = Rc::downgrade(&probe);
    let mut handlers = Vec::new();
    {
        let weak = weak.clone();
        handlers.push(stream.connect_prepared_notify(move |_| {
            if let Some(probe) = weak.upgrade() {
                probe.check();
            }
        }));
    }
    {
        let weak = weak.clone();
        handlers.push(stream.connect_error_notify(move |stream| {
            let Some(probe) = weak.upgrade() else { return };
            if let Some(err) = stream.error() {
                probe.finish(Err(media_error(err.message())));
            }
        }));
    }
    *probe.handlers.borrow_mut() = handlers;
    *probe.timeout.borrow_mut() = Some(glib::timeout_add_local(PREPARE_TIMEOUT, move || {
        if let Some(probe) = weak.upgrade() {
            // This source ends by returning `Break`: it must not be
            // removed again.
            let _ = probe.timeout.borrow_mut().take();
            probe.finish(Err("the sound did not prepare in time".into()));
        }
        glib::ControlFlow::Break
    }));
    probe.check();
}

thread_local! {
    /// Sounds preparing: kept alive until they finish.
    static PROBES: RefCell<Vec<Rc<Probe>>> = const { RefCell::new(Vec::new()) };
}

/// A sound being prepared.
struct Probe {
    stream: RefCell<Option<gtk::MediaFile>>,
    handlers: RefCell<Vec<glib::SignalHandlerId>>,
    timeout: RefCell<Option<glib::SourceId>>,
    path: PathBuf,
    max: Duration,
    control: EngineControl,
    id: u32,
    /// The speaker the sound goes to, if it still lives.
    speaker: Weak<RefCell<Inner>>,
}

impl Probe {
    /// Finishes when the stream is prepared with audio.
    fn check(&self) {
        let Some(stream) = self.stream.borrow().clone() else {
            return;
        };
        if !stream.is_prepared() {
            return;
        }
        if !stream.has_audio() {
            self.finish(Err("the sound has no audio GStreamer can decode".into()));
            return;
        }
        match micros(stream.duration()) {
            Some(duration) => self.finish(Ok(duration)),
            None => self.finish(Err("the sound is empty".into())),
        }
    }

    /// Reports the outcome, once.
    fn finish(&self, result: Result<Duration, String>) {
        let Some(stream) = self.stream.borrow_mut().take() else {
            return;
        };
        for handler in self.handlers.borrow_mut().drain(..) {
            stream.disconnect(handler);
        }
        if let Some(timeout) = self.timeout.borrow_mut().take() {
            timeout.remove();
        }
        stream.set_playing(false);
        stream.clear();
        let id = self.id;
        match result {
            Ok(duration) => {
                let truncated = duration > self.max;
                let length = duration.min(self.max);
                match self.speaker.upgrade() {
                    Some(inner) => {
                        inner.borrow_mut().sounds.insert(
                            id,
                            Sound {
                                path: self.path.clone(),
                                cut: truncated.then_some(length),
                            },
                        );
                    }
                    // The engine went away while it prepared.
                    None => {
                        let _ = std::fs::remove_file(&self.path);
                        return;
                    }
                }
                report(
                    &self.control,
                    id,
                    AudioEvent::Loaded {
                        duration: length,
                        // 48 kHz, stereo, f32: what GStreamer's sinks mix.
                        bytes: (length.as_secs_f64() * 48_000.0 * 2.0 * 4.0) as u64,
                        truncated,
                    },
                );
            }
            Err(message) => {
                let _ = std::fs::remove_file(&self.path);
                report(&self.control, id, failed(AudioError::Decode, &message));
            }
        }
        let this: *const Probe = self;
        PROBES.with(|p| {
            p.borrow_mut()
                .retain(|probe| !std::ptr::eq(Rc::as_ptr(probe), this))
        });
    }
}

/// Stops `stream` once it played `cut`, counted from where it is now.
fn cut_at(stream: &gtk::MediaFile, cut: Duration) {
    let played = Duration::from_micros(stream.timestamp().max(0) as u64);
    let weak = stream.downgrade();
    glib::timeout_add_local(
        cut.saturating_sub(played).max(Duration::from_millis(1)),
        move || {
            if let Some(stream) = weak.upgrade() {
                stream.set_playing(false);
            }
            glib::ControlFlow::Break
        },
    );
}

impl Speaker {
    fn play(&self, id: u32, volume: f32, rate: f32) {
        warn_rate(rate);
        let mut inner = self.inner.borrow_mut();
        let Some((path, cut)) = inner.sounds.get(&id).map(|s| (s.path.clone(), s.cut)) else {
            return;
        };
        // Voices that ended free their streams (and their places).
        let (done, live): (Vec<_>, Vec<_>) = std::mem::take(&mut inner.voices)
            .into_iter()
            .partition(|v| v.done.get());
        inner.voices = live;
        for voice in done {
            voice.end();
        }
        if inner.voices.len() >= MAX_VOICES
            && let Some(oldest) = (0..inner.voices.len()).min_by_key(|&i| inner.voices[i].started)
        {
            inner.voices.remove(oldest).end();
        }
        let stream = gtk::MediaFile::for_file(&gio::File::for_path(path));
        stream.set_volume(f64::from(volume.clamp(0.0, 1.0)));
        let done = Rc::new(Cell::new(false));
        let flag = done.clone();
        let _ = stream.connect_ended_notify(move |stream| {
            if stream.is_ended() {
                flag.set(true);
            }
        });
        let flag = done.clone();
        let _ = stream.connect_error_notify(move |_| flag.set(true));
        if let Some(cut) = cut {
            // Timed from the start of playback, which waits for the stream
            // to prepare.
            let armed = Cell::new(false);
            let _ = stream.connect_prepared_notify(move |stream| {
                if stream.is_prepared() && !armed.replace(true) {
                    cut_at(stream, cut);
                }
            });
        }
        stream.set_playing(true);
        inner.voices.push(Voice {
            sound: id,
            stream,
            started: Instant::now(),
            done,
        });
    }

    fn stop(&self, id: u32) {
        let mut inner = self.inner.borrow_mut();
        let (stopped, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut inner.voices)
            .into_iter()
            .partition(|v| v.sound == id);
        inner.voices = rest;
        for voice in stopped {
            voice.end();
        }
    }
}

/// A player and what it reported.
struct Player {
    id: u32,
    file: Option<gio::File>,
    /// Whether the source is an `http(s)` URL (its failures are the
    /// network's).
    network: bool,
    control: EngineControl,
    settings: Cell<PlayerSettings>,
    /// Made at the first preload or play.
    stream: RefCell<Option<gtk::MediaFile>>,
    handlers: RefCell<Vec<glib::SignalHandlerId>>,
    progress: RefCell<Option<glib::SourceId>>,
    loaded: Cell<bool>,
    failed: Cell<bool>,
    buffering: Cell<bool>,
    /// A play waits for the load.
    wants_play: Cell<bool>,
    /// A seek waits for the load: GTK seeks only a prepared stream.
    seek_to: Cell<Option<Duration>>,
    playing: Cell<bool>,
}

impl Player {
    fn new(
        id: u32,
        src: &str,
        settings: PlayerSettings,
        base: Option<&AssetBase>,
        control: EngineControl,
    ) -> Rc<Player> {
        let file = locate(src, base);
        let player = Rc::new(Player {
            id,
            network: src.starts_with("http://")
                || src.starts_with("https://")
                || (src.starts_with("asset:/") && matches!(base, Some(AssetBase::Http(_)))),
            file,
            control,
            settings: Cell::new(settings),
            stream: RefCell::default(),
            handlers: RefCell::default(),
            progress: RefCell::default(),
            loaded: Cell::new(false),
            failed: Cell::new(false),
            buffering: Cell::new(false),
            wants_play: Cell::new(false),
            seek_to: Cell::new(None),
            playing: Cell::new(false),
        });
        if player.file.is_none() {
            player.fail(
                AudioError::NotSupported,
                &format!("{src}: cannot resolve the source"),
            );
        }
        player
    }

    fn report(&self, event: AudioEvent) {
        report(&self.control, self.id, event);
    }

    fn fail(&self, error: AudioError, message: &str) {
        if self.failed.replace(true) {
            return;
        }
        self.playing.set(false);
        self.wants_play.set(false);
        self.set_buffering(false);
        self.report(failed(error, message));
    }

    fn set_buffering(&self, buffering: bool) {
        if self.buffering.replace(buffering) != buffering {
            self.report(AudioEvent::Buffer { buffering });
        }
    }

    fn current(&self) -> Option<gtk::MediaFile> {
        self.stream.borrow().clone()
    }

    fn duration(&self) -> Option<Duration> {
        self.current().and_then(|s| micros(s.duration()))
    }

    /// The stream, made (and its load watched) the first time.
    fn stream(self: &Rc<Self>) -> Option<gtk::MediaFile> {
        if let Some(stream) = self.current() {
            return Some(stream);
        }
        if self.failed.get() {
            return None;
        }
        let stream = gtk::MediaFile::for_file(self.file.as_ref()?);
        let settings = self.settings.get();
        stream.set_volume(f64::from(settings.volume.clamp(0.0, 1.0)));
        stream.set_loop(settings.looped);
        warn_rate(settings.rate);
        let mut handlers = Vec::new();
        let weak = Rc::downgrade(self);
        handlers.push(stream.connect_prepared_notify({
            let weak = weak.clone();
            move |_| {
                if let Some(this) = weak.upgrade() {
                    this.prepared();
                }
            }
        }));
        handlers.push(stream.connect_ended_notify({
            let weak = weak.clone();
            move |stream| {
                if stream.is_ended()
                    && let Some(this) = weak.upgrade()
                {
                    this.ended();
                }
            }
        }));
        handlers.push(stream.connect_error_notify({
            let weak = weak.clone();
            move |stream| {
                let Some(err) = stream.error() else { return };
                if let Some(this) = weak.upgrade() {
                    let kind = if this.network {
                        AudioError::Network
                    } else {
                        AudioError::Decode
                    };
                    stream.set_playing(false);
                    this.fail(kind, &media_error(err.message()));
                }
            }
        }));
        *self.handlers.borrow_mut() = handlers;
        *self.stream.borrow_mut() = Some(stream.clone());
        // A paused stream reports no progress, a stream that never
        // prepares no error: a timer watches both.
        let started = Instant::now();
        let weak = Rc::downgrade(self);
        *self.progress.borrow_mut() = Some(glib::timeout_add_local(PROGRESS_INTERVAL, move || {
            let Some(this) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !this.loaded.get() && !this.failed.get() && started.elapsed() > PREPARE_TIMEOUT * 6 {
                this.fail(
                    AudioError::Network,
                    "the audio did not load within a minute",
                );
            }
            if this.playing.get()
                && this.loaded.get()
                && let Some(stream) = this.current()
            {
                this.report(AudioEvent::Progress {
                    position: Duration::from_micros(stream.timestamp().max(0) as u64),
                    duration: micros(stream.duration()),
                });
            }
            glib::ControlFlow::Continue
        }));
        Some(stream)
    }

    /// The stream prepared: the load, and a play that waited for it.
    fn prepared(&self) {
        let Some(stream) = self.current() else { return };
        if self.loaded.get() || self.failed.get() || !stream.is_prepared() {
            return;
        }
        // GStreamer prepares a stream with no streams when it has no
        // plug-in for them; the error follows.
        if !stream.has_audio() {
            return;
        }
        self.loaded.set(true);
        self.set_buffering(false);
        self.report(AudioEvent::Load {
            duration: micros(stream.duration()),
        });
        if let Some(position) = self.seek_to.take() {
            self.seek(&stream, position);
        }
        if self.wants_play.replace(false) {
            self.start(&stream);
        }
    }

    /// Moves a prepared stream to `position` (at most its end) and reports
    /// where it is: there, or where it stays when it cannot seek (a live
    /// stream).
    fn seek(&self, stream: &gtk::MediaFile, position: Duration) {
        let duration = micros(stream.duration());
        let position = duration.map_or(position, |d| position.min(d));
        let position = if stream.is_seekable() {
            stream.seek(position.as_micros().min(i64::MAX as u128) as i64);
            position
        } else {
            Duration::from_micros(stream.timestamp().max(0) as u64)
        };
        self.report(AudioEvent::Progress { position, duration });
    }

    fn start(&self, stream: &gtk::MediaFile) {
        stream.set_playing(true);
        self.playing.set(true);
        self.report(AudioEvent::Playing);
    }

    /// A stream that does not loop reached its end.
    fn ended(&self) {
        if self.settings.get().looped || !self.playing.replace(false) {
            return;
        }
        if let Some(duration) = self.duration() {
            self.report(AudioEvent::Progress {
                position: duration,
                duration: Some(duration),
            });
        }
        self.report(AudioEvent::Ended);
    }

    fn apply(self: &Rc<Self>, command: &AudioCommand) {
        match command {
            AudioCommand::Preload { .. } => {
                let _ = self.stream();
            }
            AudioCommand::Resume { .. } => {
                if self.failed.get() && self.file.is_some() {
                    // Tried again from the start.
                    self.close();
                    self.failed.set(false);
                    self.loaded.set(false);
                }
                let Some(stream) = self.stream() else { return };
                if self.loaded.get() {
                    if stream.is_ended() && stream.is_seekable() {
                        stream.seek(0);
                    }
                    self.start(&stream);
                } else {
                    self.wants_play.set(true);
                    self.set_buffering(true);
                }
            }
            AudioCommand::Pause { .. } => {
                self.wants_play.set(false);
                if let Some(stream) = self.current() {
                    stream.set_playing(false);
                }
                if self.playing.replace(false) {
                    self.report(AudioEvent::Paused);
                }
            }
            AudioCommand::Seek { position, .. } => {
                let Some(stream) = self.stream() else { return };
                if self.loaded.get() {
                    self.seek(&stream, *position);
                } else {
                    // `prepared` applies it (the last one asked).
                    self.seek_to.set(Some(*position));
                }
            }
            AudioCommand::Set { settings, .. } => {
                self.settings.set(*settings);
                warn_rate(settings.rate);
                if let Some(stream) = self.current() {
                    stream.set_volume(f64::from(settings.volume.clamp(0.0, 1.0)));
                    stream.set_loop(settings.looped);
                }
            }
            _ => {}
        }
    }

    /// Stops the player and frees its stream.
    fn close(&self) {
        if let Some(id) = self.progress.borrow_mut().take() {
            id.remove();
        }
        let handlers = std::mem::take(&mut *self.handlers.borrow_mut());
        let stream = self.stream.borrow_mut().take();
        if let Some(stream) = stream {
            for handler in handlers {
                stream.disconnect(handler);
            }
            stream.set_playing(false);
            stream.clear();
        }
        self.playing.set(false);
        self.wants_play.set(false);
        self.seek_to.set(None);
    }
}

/// Where `src` is (`asset:/` URIs against `base`).
fn locate(src: &str, base: Option<&AssetBase>) -> Option<gio::File> {
    if let Some(path) = src.strip_prefix("asset:/") {
        let path = path.trim_start_matches('/');
        return match base? {
            AssetBase::Http(url) => Some(gio::File::for_uri(&format!(
                "{}/{path}",
                url.trim_end_matches('/')
            ))),
            dirs @ (AssetBase::Directory(_) | AssetBase::Directories(_)) => {
                dirs.file(path).map(gio::File::for_path)
            }
        };
    }
    if src.contains("://") {
        Some(gio::File::for_uri(src))
    } else if src.is_empty() {
        None
    } else {
        Some(gio::File::for_path(src))
    }
}

/// What GStreamer says, for apps: its first line without the element path.
fn media_error(message: &str) -> String {
    let line = message.lines().next().unwrap_or(message).trim();
    match line.strip_prefix("Error from element ") {
        Some(rest) => rest.split_once(": ").map_or(rest, |(_, m)| m),
        None => line,
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_come_from_names_and_media_types() {
        assert_eq!(extension("sounds/click.MP3"), "mp3");
        assert_eq!(extension("https://a.com/x.m4a?v=2"), "m4a");
        assert_eq!(extension("data:audio/mpeg;base64,AA"), "mp3");
        assert_eq!(extension("data:audio/ogg,%00"), "ogg");
        assert_eq!(extension("click"), "wav");
    }

    #[test]
    fn microseconds_are_durations_when_known() {
        assert_eq!(micros(0), None);
        assert_eq!(micros(-1), None);
        assert_eq!(micros(1_500_000), Some(Duration::from_millis(1500)));
    }

    #[test]
    fn media_errors_lose_the_element_path() {
        assert_eq!(
            media_error("Error from element /GstPlayBin3:p: Could not open.\nmore"),
            "Could not open."
        );
        assert_eq!(media_error("plain"), "plain");
    }
}
