//! The `js` lines engines log (the app's `console` and its errors),
//! captured: the logger is the process's, shared by every test, so each
//! test looks for words only it logs, or for the lines of its own engine's
//! thread ([`printed_by`]).

use std::{
    sync::Mutex,
    thread::ThreadId,
    time::{Duration, Instant},
};

/// What was logged: the thread that logged it, whether it is an error, the
/// line.
static LINES: Mutex<Vec<(ThreadId, bool, String)>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        // `console.debug` is `Debug`: every line an app prints.
        metadata.target() == "js" && metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            LINES.lock().unwrap().push((
                std::thread::current().id(),
                record.level() == log::Level::Error,
                record.args().to_string(),
            ));
        }
    }

    fn flush(&self) {}
}

static CAPTURE: Capture = Capture;

/// Captures from here on (installed once).
pub(crate) fn capture() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        if log::set_logger(&CAPTURE).is_ok() {
            log::set_max_level(log::LevelFilter::Debug);
        }
    });
}

/// The errors logged so far that contain `word`.
pub(crate) fn containing(word: &str) -> Vec<String> {
    LINES
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, error, l)| *error && l.contains(word))
        .map(|(_, _, l)| l.clone())
        .collect()
}

/// The lines of every level `thread` logged so far.
pub(crate) fn printed_by(thread: ThreadId) -> Vec<String> {
    LINES
        .lock()
        .unwrap()
        .iter()
        .filter(|(t, _, _)| *t == thread)
        .map(|(_, _, l)| l.clone())
        .collect()
}

/// The errors that contain `word`, once there is one (or after 5 s).
pub(crate) fn wait_for(word: &str) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let lines = containing(word);
        if !lines.is_empty() || Instant::now() > deadline {
            return lines;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
