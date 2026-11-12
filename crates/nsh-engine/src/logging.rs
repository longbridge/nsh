//! The logger of the desktop release runners (`nsh-macos-app`,
//! `nsh-windows-app`, `nsh-linux-app`): one filter, one rolling file in the
//! app's data directory, and the platform's own place for each line.
//!
//! - What is logged: `RUST_LOG` when it is set, in `env_logger`'s syntax
//!   (`info`, `my_app=debug,js=info`, `warn,nsh_engine::updates=info`); else
//!   the runners' rule, warnings, errors and the app's `js` lines
//!   (`console.*`), and every line from info up when run by `nsh run`.
//! - Where: a rolling file, `logs/app.log` in the app's data directory
//!   ([`log_dir`]), whose path apps read (`nsh_core::element::log_file`, TS
//!   `process.logFile`), and the platform's output the runner hands
//!   [`LogOptions::output`] (the unified log, the debugger's output,
//!   stderr).
//! - The file: one line per record, `<UTC time> <LEVEL> <target>: <message>`.
//!   Past [`MAX_FILE_BYTES`] it becomes `app.1.log` (the one before that
//!   `app.2.log`, ...) and a new `app.log` starts; [`KEPT_FILES`] older files
//!   are kept. A file that cannot be made or written is left alone, tried
//!   again after [`RETRY_AFTER`]: the app and the platform's output carry
//!   on without it.

use std::{
    fs::{File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock, PoisonError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// The size past which the log file rolls over to `app.1.log`.
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// How many rolled-over files are kept beside `app.log` (`app.1.log` up to
/// `app.<KEPT_FILES>.log`).
pub const KEPT_FILES: usize = 3;

/// How long the logger waits before it tries again to open a log file it
/// could not open or write.
pub const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The log file's name (and the stem of the rolled-over ones).
const FILE_STEM: &str = "app";

/// The environment variable of the filter.
const FILTER_VARIABLE: &str = "RUST_LOG";

/// The platform's own place for each line the filter lets through.
pub type LogOutput = Box<dyn Fn(&Record<'_>) + Send + Sync>;

/// What [`install`] sets up.
#[non_exhaustive]
#[derive(Default)]
pub struct LogOptions {
    /// Run by `nsh run`: every line from info up when `RUST_LOG` is not set.
    pub development: bool,
    /// The directory of the log files ([`log_dir`]); `None` writes none.
    pub directory: Option<PathBuf>,
    /// The platform's own place for each line the filter lets through.
    pub output: Option<LogOutput>,
}

/// The directory of the log files of app `app_id`: `logs` in its data
/// directory (`<data_dir>/<app id>/logs`). `None` without a data directory.
pub fn log_dir(app_id: &str) -> Option<PathBuf> {
    crate::data_dir().map(|dir| dir.join(crate::app_dir_name(Some(app_id))).join("logs"))
}

/// Makes the runners' logger the process's logger, filtered by `RUST_LOG`
/// when it is set, and records its file for `nsh_core::element::log_file`.
/// Returns whether it did: `false` when the process has a logger already.
pub fn install(options: LogOptions) -> bool {
    static LOGGER: OnceLock<Logger> = OnceLock::new();
    let filter = std::env::var(FILTER_VARIABLE).ok();
    let mut installed = false;
    let logger = LOGGER.get_or_init(|| {
        installed = true;
        Logger::new(options, filter.as_deref())
    });
    if !installed || log::set_logger(logger).is_err() {
        return false;
    }
    log::set_max_level(logger.filter.max_level());
    if let Some(file) = &logger.file {
        nsh_core::log_file::set(file.lock().unwrap_or_else(PoisonError::into_inner).path());
    }
    if let Some(error) = &logger.filter_error {
        log::warn!("{FILTER_VARIABLE} is not a filter ({error}); the default levels apply");
    }
    true
}

pub(crate) struct Logger {
    filter: Filter,
    /// Why `RUST_LOG` was not used, to say once the logger is in place.
    filter_error: Option<String>,
    file: Option<Mutex<RollingFile>>,
    output: Option<LogOutput>,
}

impl Logger {
    /// The logger of `options`, filtered by `filter` (`RUST_LOG`'s value).
    pub(crate) fn new(options: LogOptions, filter: Option<&str>) -> Logger {
        let (filter, filter_error) = Filter::new(filter, options.development);
        Logger {
            filter,
            filter_error,
            file: options
                .directory
                .map(|dir| Mutex::new(RollingFile::new(dir))),
            output: options.output,
        }
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.filter.enabled(metadata)
    }

    fn log(&self, record: &Record<'_>) {
        if !self.filter.matches(record) {
            return;
        }
        if let Some(file) = &self.file {
            let line = format!(
                "{} {:<5} {}: {}\n",
                Timestamp(SystemTime::now()),
                record.level(),
                record.target(),
                record.args()
            );
            file.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .write(line.as_bytes(), Instant::now());
        }
        if let Some(output) = &self.output {
            output(record);
        }
    }

    fn flush(&self) {}
}

/// Which records are logged.
enum Filter {
    /// Warnings, errors and `js` lines from info up; every line from info
    /// up in development.
    Default { development: bool },
    /// `RUST_LOG`'s.
    Env(env_filter::Filter),
}

impl Filter {
    /// `RUST_LOG`'s filter when `value` is one, else the default (and why
    /// `value` is not one).
    fn new(value: Option<&str>, development: bool) -> (Filter, Option<String>) {
        let default = Filter::Default { development };
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            return (default, None);
        };
        match env_filter::Builder::new().try_parse(value) {
            Ok(builder) => (Filter::Env(builder.build()), None),
            Err(error) => (default, Some(error.to_string())),
        }
    }

    fn max_level(&self) -> LevelFilter {
        match self {
            Filter::Default { .. } => LevelFilter::Info,
            Filter::Env(filter) => filter.filter(),
        }
    }

    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        match self {
            Filter::Default { development } => {
                metadata.level() <= Level::Warn
                    || (metadata.level() <= Level::Info
                        && (*development || metadata.target() == "js"))
            }
            Filter::Env(filter) => filter.enabled(metadata),
        }
    }

    fn matches(&self, record: &Record<'_>) -> bool {
        match self {
            Filter::Default { .. } => self.enabled(record.metadata()),
            Filter::Env(filter) => filter.matches(record),
        }
    }
}

/// `app.log` in a directory, rolled over past [`MAX_FILE_BYTES`].
struct RollingFile {
    dir: PathBuf,
    /// The open file and its size, opened on the first line.
    open: Option<(File, u64)>,
    /// When to try again after a failure.
    retry_at: Option<Instant>,
}

impl RollingFile {
    fn new(dir: PathBuf) -> RollingFile {
        RollingFile {
            dir,
            open: None,
            retry_at: None,
        }
    }

    /// The current file, `app.log`.
    fn path(&self) -> PathBuf {
        self.dir.join(format!("{FILE_STEM}.log"))
    }

    /// The `n`th older file, `app.<n>.log`.
    fn older(&self, n: usize) -> PathBuf {
        self.dir.join(format!("{FILE_STEM}.{n}.log"))
    }

    /// Appends `line` at `now`, rolling the file over first when it would
    /// pass [`MAX_FILE_BYTES`]. A failure closes the file until
    /// [`RETRY_AFTER`] has passed.
    fn write(&mut self, line: &[u8], now: Instant) {
        if self.retry_at.is_some_and(|at| now < at) {
            return;
        }
        self.retry_at = None;
        let written = self.try_write(line);
        if written.is_err() {
            self.open = None;
            self.retry_at = Some(now + RETRY_AFTER);
        }
    }

    fn try_write(&mut self, line: &[u8]) -> std::io::Result<()> {
        if self.open.is_none() {
            self.open = Some(self.open_file()?);
        }
        let size = self.open.as_ref().map_or(0, |(_, size)| *size);
        if size > 0 && size + line.len() as u64 > MAX_FILE_BYTES {
            self.open = None;
            self.roll_over()?;
            self.open = Some(self.open_file()?);
        }
        let (file, size) = self.open.as_mut().expect("opened above");
        file.write_all(line)?;
        *size += line.len() as u64;
        Ok(())
    }

    /// `app.log` for appending, made with its directory when missing, and
    /// its size.
    fn open_file(&self) -> std::io::Result<(File, u64)> {
        std::fs::create_dir_all(&self.dir)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path())?;
        let size = file.metadata()?.len();
        Ok((file, size))
    }

    /// Moves each file one place older, dropping the oldest: `app.log`
    /// becomes `app.1.log`, `app.1.log` `app.2.log`, ...
    fn roll_over(&self) -> std::io::Result<()> {
        // Windows' rename does not replace a file.
        remove_if_present(&self.older(KEPT_FILES))?;
        for n in (1..KEPT_FILES).rev() {
            rename_if_present(&self.older(n), &self.older(n + 1))?;
        }
        if KEPT_FILES == 0 {
            return remove_if_present(&self.path());
        }
        rename_if_present(&self.path(), &self.older(1))
    }
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn rename_if_present(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// A time as RFC 3339 in UTC to the millisecond: `2026-11-12T08:30:05.123Z`.
struct Timestamp(SystemTime);

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let since = self.0.duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = since.as_secs();
        let (year, month, day) = civil_from_days((secs / 86_400) as i64);
        let of_day = secs % 86_400;
        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
            of_day / 3600,
            of_day / 60 % 60,
            of_day % 60,
            since.subsec_millis()
        )
    }
}

/// The proleptic Gregorian date of `days` since 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// A fresh directory under the system's temporary one.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nsh-logging-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn metadata(level: Level, target: &str) -> Metadata<'_> {
        Metadata::builder().level(level).target(target).build()
    }

    fn log(logger: &Logger, level: Level, target: &str, message: &str) {
        logger.log(
            &Record::builder()
                .level(level)
                .target(target)
                .args(format_args!("{message}"))
                .build(),
        );
    }

    #[test]
    fn without_rust_log_the_runners_levels_hold() {
        let (filter, error) = Filter::new(None, false);
        assert!(error.is_none());
        assert_eq!(filter.max_level(), LevelFilter::Info);
        assert!(filter.enabled(&metadata(Level::Warn, "my_app")));
        assert!(filter.enabled(&metadata(Level::Info, "js")));
        assert!(!filter.enabled(&metadata(Level::Debug, "js")));
        assert!(!filter.enabled(&metadata(Level::Info, "my_app")));
        let (development, _) = Filter::new(Some("  "), true);
        assert!(development.enabled(&metadata(Level::Info, "my_app")));
        assert!(!development.enabled(&metadata(Level::Debug, "my_app")));
    }

    #[test]
    fn rust_log_sets_the_levels() {
        let (filter, error) =
            Filter::new(Some("warn,my_app=debug,nsh_engine::updates=info"), false);
        assert!(error.is_none());
        assert_eq!(filter.max_level(), LevelFilter::Debug);
        assert!(filter.enabled(&metadata(Level::Debug, "my_app::login")));
        assert!(!filter.enabled(&metadata(Level::Trace, "my_app")));
        assert!(filter.enabled(&metadata(Level::Info, "nsh_engine::updates")));
        assert!(!filter.enabled(&metadata(Level::Info, "js")));
        assert!(filter.enabled(&metadata(Level::Warn, "js")));
        // A bad filter keeps the defaults, and says why.
        let (filter, error) = Filter::new(Some("my_app=loud"), false);
        assert!(error.is_some());
        assert!(filter.enabled(&metadata(Level::Info, "js")));
    }

    #[test]
    fn lines_go_to_the_file_and_the_output() {
        let dir = temp_dir("lines");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let output = {
            let seen = seen.clone();
            Box::new(move |record: &Record<'_>| {
                seen.lock()
                    .unwrap()
                    .push(format!("{} {}", record.level(), record.args()));
            })
        };
        let logger = Logger::new(
            LogOptions {
                directory: Some(dir.clone()),
                output: Some(output),
                ..LogOptions::default()
            },
            Some("info"),
        );
        log(&logger, Level::Info, "my_app", "signed in");
        log(&logger, Level::Debug, "my_app", "token refreshed");
        log(&logger, Level::Error, "js", "boom");
        let text = std::fs::read_to_string(dir.join("app.log")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].ends_with(" INFO  my_app: signed in"), "{text}");
        assert!(lines[1].ends_with(" ERROR js: boom"), "{text}");
        assert_eq!(&lines[0][4..5], "-");
        assert_eq!(&lines[0][23..25], "Z ");
        assert_eq!(*seen.lock().unwrap(), ["INFO signed in", "ERROR boom"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_file_rolls_over_and_keeps_the_newest() {
        let dir = temp_dir("roll");
        let mut file = RollingFile::new(dir.clone());
        let line = vec![b'x'; (MAX_FILE_BYTES / 2) as usize];
        let now = Instant::now();
        // Two lines fill a file; each third starts the next one.
        for n in 0..(2 * (KEPT_FILES + 2)) {
            let mut line = line.clone();
            line[0] = b'0' + n as u8;
            file.write(&line, now);
        }
        for n in 1..=KEPT_FILES {
            let older = std::fs::metadata(dir.join(format!("app.{n}.log"))).unwrap();
            assert_eq!(older.len(), line.len() as u64 * 2);
        }
        assert!(!dir.join(format!("app.{}.log", KEPT_FILES + 1)).exists());
        let newest = std::fs::read(dir.join("app.log")).unwrap();
        assert_eq!(newest.len(), line.len() * 2);
        assert_eq!(newest[0], b'0' + (2 * (KEPT_FILES + 1)) as u8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cannot_be_written_is_tried_again_later() {
        let dir = temp_dir("blocked");
        // A file where the directory should be.
        std::fs::write(&dir, b"").unwrap();
        let mut file = RollingFile::new(dir.join("logs"));
        let now = Instant::now();
        file.write(b"lost\n", now);
        assert_eq!(file.retry_at, Some(now + RETRY_AFTER));
        std::fs::remove_file(&dir).unwrap();
        file.write(b"still waiting\n", now + RETRY_AFTER / 2);
        assert!(!dir.join("logs/app.log").exists());
        file.write(b"back\n", now + RETRY_AFTER);
        assert_eq!(
            std::fs::read_to_string(dir.join("logs/app.log")).unwrap(),
            "back\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_log_directory_is_in_the_apps_data_directory() {
        let Some(data) = crate::data_dir() else {
            return;
        };
        assert_eq!(
            log_dir("com.example.notes"),
            Some(data.join("com.example.notes/logs"))
        );
        assert_eq!(log_dir("../x"), Some(data.join("default/logs")));
    }

    #[test]
    fn timestamps_are_utc() {
        let time = UNIX_EPOCH + Duration::from_millis(1_794_472_205_123);
        assert_eq!(Timestamp(time).to_string(), "2026-11-12T08:30:05.123Z");
        assert_eq!(
            Timestamp(UNIX_EPOCH).to_string(),
            "1970-01-01T00:00:00.000Z"
        );
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }
}
