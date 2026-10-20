//! The `js` errors engines log, captured: the logger is the process's,
//! shared by every test, so each test looks for words only it logs.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == "js" && metadata.level() == log::Level::Error
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            LINES.lock().unwrap().push(record.args().to_string());
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
            log::set_max_level(log::LevelFilter::Error);
        }
    });
}

/// The lines logged so far that contain `word`.
pub(crate) fn containing(word: &str) -> Vec<String> {
    LINES
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains(word))
        .cloned()
        .collect()
}

/// The lines that contain `word`, once there is one (or after 5 s).
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
