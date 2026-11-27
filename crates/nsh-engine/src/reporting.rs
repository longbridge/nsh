//! Sends the app's errors to an error tracker that speaks the Sentry
//! envelope protocol (Sentry, Beacon): uncaught exceptions, bundles that
//! fail to load, stopped VMs and unhandled promise rejections
//! ([`EngineConfig::error_reporting`](crate::EngineConfig::error_reporting)).
//!
//! The stacks are the ones the engine already mapped to the app's sources
//! on the device, so the tracker needs no source maps for them. The release
//! is the signed package's `id@version+build`, so errors group by package
//! build. Requests go through the platform's HTTP stack
//! ([`Network`](crate::Network)), the one `fetch` uses (its TLS, proxies),
//! from a thread of their own: the engine thread never waits for them.

// The web has no threads to send from: the page reports its own errors.
#![cfg_attr(target_arch = "wasm32", allow(dead_code, unused_imports))]

use std::{
    collections::hash_map::RandomState,
    fmt::Write as _,
    hash::{BuildHasher, Hasher},
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use nsh_core::platform::{FetchEvent, HttpRequest};
use serde_json::{Map, Value, json};

use crate::{AppError, AppErrorKind, Network};

/// Where and how an engine reports its app's errors
/// ([`EngineConfig::error_reporting`](crate::EngineConfig::error_reporting)).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ErrorReporting {
    /// The project's DSN, as the tracker gives it:
    /// `https://<public key>@<host>[/<path>]/<project id>`.
    pub dsn: String,
    /// The environment the events are filed under (`production`, `sit`).
    /// `None`: the tracker's default.
    pub environment: Option<String>,
    /// The share of errors sent, from 0 to 1. [`ErrorReporting::new`]: 1.
    pub sample_rate: f32,
    /// Tags added to every event (the embedding app's version, channel).
    pub tags: Vec<(String, String)>,
    /// Unhandled promise rejections are sent too. [`ErrorReporting::new`]:
    /// on. They are reported only, never shown or given to
    /// [`MountSink::report`](crate::MountSink::report).
    pub rejections: bool,
    /// The most events one engine sends; the rest are dropped (an error
    /// thrown on every frame). [`ErrorReporting::new`]: 100.
    pub max_events: u32,
}

impl ErrorReporting {
    /// Reports every error to `dsn`.
    pub fn new(dsn: impl Into<String>) -> ErrorReporting {
        ErrorReporting {
            dsn: dsn.into(),
            environment: None,
            sample_rate: 1.0,
            tags: Vec::new(),
            rejections: true,
            max_events: 100,
        }
    }

    /// Files the events under `environment`.
    pub fn environment(mut self, environment: impl Into<String>) -> ErrorReporting {
        self.environment = Some(environment.into());
        self
    }

    /// Sends this share of the errors (0 to 1).
    pub fn sample_rate(mut self, rate: f32) -> ErrorReporting {
        self.sample_rate = rate;
        self
    }

    /// Adds a tag to every event.
    pub fn tag(mut self, name: impl Into<String>, value: impl Into<String>) -> ErrorReporting {
        self.tags.push((name.into(), value.into()));
        self
    }

    /// Sends unhandled promise rejections too (or not).
    pub fn rejections(mut self, on: bool) -> ErrorReporting {
        self.rejections = on;
        self
    }

    /// Sends at most `max` events per engine.
    pub fn max_events(mut self, max: u32) -> ErrorReporting {
        self.max_events = max;
        self
    }
}

/// A DSN's parts: where envelopes go and the key they carry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Dsn {
    /// `<scheme>://<host>[/<path>]/api/<project>/envelope/`.
    endpoint: String,
    key: String,
}

impl Dsn {
    fn parse(dsn: &str) -> Option<Dsn> {
        let (scheme, rest) = dsn.trim().split_once("://")?;
        if scheme != "https" && scheme != "http" {
            return None;
        }
        let (key, rest) = rest.split_once('@')?;
        // `key:secret` in old DSNs: the secret is not sent.
        let key = key.split(':').next().unwrap_or_default();
        let (host, path) = rest.split_once('/')?;
        let path = path.trim_end_matches('/');
        let (prefix, project) = match path.rsplit_once('/') {
            Some((prefix, project)) => (format!("/{prefix}"), project),
            None => (String::new(), path),
        };
        let valid = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        if !valid(key) || host.is_empty() || !valid(project) {
            return None;
        }
        Some(Dsn {
            endpoint: format!("{scheme}://{host}{prefix}/api/{project}/envelope/"),
            key: key.to_owned(),
        })
    }
}

/// How long a report may take before it is given up.
const SEND_TIMEOUT: Duration = Duration::from_secs(30);
/// Reports waiting to be sent; more are dropped.
const QUEUE: usize = 32;

/// An engine's reporter: builds the events and queues them for its sending
/// thread.
pub(crate) struct Reporter {
    config: ErrorReporting,
    queue: crossbeam_channel::Sender<Vec<u8>>,
    sent: AtomicU32,
    seed: RandomState,
    counter: AtomicU64,
}

impl Reporter {
    /// `None` without a usable DSN (logged), or where there are no threads
    /// to send from (the web, whose page reports its own errors).
    pub(crate) fn new(config: &ErrorReporting, network: Arc<dyn Network>) -> Option<Arc<Reporter>> {
        let Some(dsn) = Dsn::parse(&config.dsn) else {
            log::warn!("error reporting is off: invalid DSN");
            return None;
        };
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (dsn, network);
            None
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (queue, rx) = crossbeam_channel::bounded::<Vec<u8>>(QUEUE);
            let endpoint = dsn.endpoint.clone();
            let auth = format!(
                "Sentry sentry_version=7, sentry_client=nsh/{}, sentry_key={}",
                env!("CARGO_PKG_VERSION"),
                dsn.key
            );
            std::thread::Builder::new()
                .name("nsh-error-reporting".into())
                .spawn(move || {
                    for envelope in rx {
                        send(&*network, &endpoint, &auth, envelope);
                    }
                })
                .ok()?;
            Some(Arc::new(Reporter {
                config: config.clone(),
                queue,
                sent: AtomicU32::new(0),
                seed: RandomState::new(),
                counter: AtomicU64::new(0),
            }))
        }
    }

    /// Reports `error`, unless it is over the engine's budget or sampled
    /// out. Never blocks.
    pub(crate) fn report(&self, error: &AppError) {
        self.queue_event(error, false);
    }

    /// Reports an unhandled promise rejection (`text`: its message and
    /// stack, mapped), if [`ErrorReporting::rejections`].
    pub(crate) fn report_rejection(&self, error: &AppError) {
        if self.config.rejections {
            self.queue_event(error, true);
        }
    }

    fn queue_event(&self, error: &AppError, rejection: bool) {
        if !self.sampled() {
            return;
        }
        if self.sent.fetch_add(1, Ordering::Relaxed) >= self.config.max_events {
            return;
        }
        let id = self.event_id();
        let envelope = self.envelope(error, rejection, &id, unix_now());
        if self.queue.try_send(envelope).is_err() {
            log::warn!("error report {id} dropped: too many waiting");
        }
    }

    fn sampled(&self) -> bool {
        let rate = self.config.sample_rate;
        if rate >= 1.0 {
            return true;
        }
        if rate <= 0.0 || rate.is_nan() {
            return false;
        }
        (self.random() >> 11) as f64 / (1u64 << 53) as f64 <= rate as f64
    }

    fn random(&self) -> u64 {
        let mut hasher = self.seed.build_hasher();
        hasher.write_u64(self.counter.fetch_add(1, Ordering::Relaxed));
        hasher.write_u128(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        hasher.finish()
    }

    /// 32 hex digits, random.
    fn event_id(&self) -> String {
        format!("{:016x}{:016x}", self.random(), self.random())
    }

    /// The envelope of one event.
    fn envelope(&self, error: &AppError, rejection: bool, id: &str, now: f64) -> Vec<u8> {
        let event = event(&self.config, error, rejection, id, now);
        let payload = serde_json::to_vec(&event).unwrap_or_default();
        let header = json!({
            "event_id": id,
            "dsn": self.config.dsn.trim(),
            "sent_at": rfc3339(now),
            "sdk": sdk(),
        });
        let mut out = serde_json::to_vec(&header).unwrap_or_default();
        out.push(b'\n');
        out.extend_from_slice(
            &serde_json::to_vec(&json!({"type": "event", "length": payload.len()}))
                .unwrap_or_default(),
        );
        out.push(b'\n');
        out.extend_from_slice(&payload);
        out.push(b'\n');
        out
    }
}

/// Posts one envelope and waits for the answer (or [`SEND_TIMEOUT`]).
#[cfg(not(target_arch = "wasm32"))]
fn send(network: &dyn Network, endpoint: &str, auth: &str, envelope: Vec<u8>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let request = HttpRequest::new("POST", endpoint)
        .header("Content-Type", "application/x-sentry-envelope")
        .header("X-Sentry-Auth", auth)
        .body(envelope);
    let task = network.fetch(
        request,
        Box::new(move |event| {
            let _ = tx.send(event);
        }),
    );
    let deadline = std::time::Instant::now() + SEND_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(FetchEvent::Head(head)) if !(200..300).contains(&head.status) => {
                log::warn!("error report refused: HTTP {}", head.status);
            }
            Ok(FetchEvent::End) => break,
            Ok(FetchEvent::Failed(message)) => {
                log::warn!("error report failed: {message}");
                break;
            }
            Ok(_) => {}
            Err(_) => {
                log::warn!("error report timed out");
                task.cancel();
                return;
            }
        }
    }
    drop(task);
}

fn sdk() -> Value {
    json!({"name": "nsh.engine", "version": env!("CARGO_PKG_VERSION")})
}

/// The Sentry event of `error`.
fn event(config: &ErrorReporting, error: &AppError, rejection: bool, id: &str, now: f64) -> Value {
    let (kind, level) = match error.kind {
        _ if rejection => ("unhandledrejection", "error"),
        AppErrorKind::Load => ("load", "fatal"),
        AppErrorKind::Uncaught => ("uncaught", "error"),
        AppErrorKind::Stopped => ("stopped", "fatal"),
        AppErrorKind::HotUpdate => ("hot-update", "warning"),
        AppErrorKind::Asset => ("asset", "error"),
    };
    let (ty, value) = exception_type(&error.message);
    let frames = error.stack.as_deref().map(frames).unwrap_or_default();
    let mut exception = json!({
        "type": ty,
        "value": value,
        "mechanism": {"type": format!("nsh.{kind}"), "handled": false},
    });
    if !frames.is_empty() {
        exception["stacktrace"] = json!({"frames": frames});
    }

    let mut tags = Map::new();
    tags.insert("nsh.kind".into(), kind.into());
    tags.insert("nsh.version".into(), env!("CARGO_PKG_VERSION").into());
    tags.insert("os.name".into(), std::env::consts::OS.into());
    let mut event = json!({
        "event_id": id,
        "timestamp": now,
        "platform": "javascript",
        "level": level,
        "logger": "nsh",
        "sdk": sdk(),
        "exception": {"values": [exception]},
        "contexts": {
            "os": {"name": std::env::consts::OS},
            "runtime": {"name": "nsh", "version": env!("CARGO_PKG_VERSION")},
        },
    });
    if let Some(package) = &error.package {
        event["release"] = format!("{}@{}+{}", package.id, package.version, package.build).into();
        event["dist"] = package.build.to_string().into();
        tags.insert("nsh.package".into(), package.id.clone().into());
        tags.insert("nsh.sha256".into(), package.sha256.clone().into());
        event["contexts"]["app"] = json!({
            "app_identifier": package.id,
            "app_name": package.name,
            "app_version": package.version,
            "app_build": package.build.to_string(),
        });
    }
    if let Some(environment) = &config.environment {
        event["environment"] = environment.clone().into();
    }
    for (name, value) in &config.tags {
        tags.insert(name.clone(), value.clone().into());
    }
    event["tags"] = Value::Object(tags);
    event
}

/// `TypeError: x is undefined` → (`TypeError`, `x is undefined`); a message
/// without a type is an `Error`.
fn exception_type(message: &str) -> (String, String) {
    let first = message.lines().next().unwrap_or_default();
    if let Some((ty, rest)) = first.split_once(": ")
        && !ty.is_empty()
        && ty
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
    {
        let rest_of = &message[first.len()..];
        return (ty.to_owned(), format!("{rest}{rest_of}"));
    }
    ("Error".to_owned(), message.to_owned())
}

/// The frames of a QuickJS stack (`    at name (file:line:column)`, newest
/// first), oldest first as Sentry orders them.
fn frames(stack: &str) -> Vec<Value> {
    let mut frames: Vec<Value> = stack
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix("at ")?;
            let (function, location) =
                match line.strip_suffix(')').and_then(|l| l.rsplit_once(" (")) {
                    Some((function, location)) => (Some(function), location),
                    None => (None, line),
                };
            let mut frame = Map::new();
            let mut parts = location.rsplitn(3, ':');
            let (column, row, file) = (parts.next(), parts.next(), parts.next());
            match (
                file,
                row.and_then(|r| r.parse::<u64>().ok()),
                column.and_then(|c| c.parse::<u64>().ok()),
            ) {
                (Some(file), Some(row), Some(column)) => {
                    frame.insert("filename".into(), file.into());
                    frame.insert("lineno".into(), row.into());
                    frame.insert("colno".into(), column.into());
                }
                _ => {
                    frame.insert("filename".into(), location.into());
                }
            }
            if let Some(function) = function.filter(|f| !f.is_empty()) {
                frame.insert("function".into(), function.into());
            }
            frame.insert("in_app".into(), (!location.starts_with("native")).into());
            Some(Value::Object(frame))
        })
        .collect();
    frames.reverse();
    frames
}

fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// `2026-11-27T09:30:00.123Z`.
fn rfc3339(unix: f64) -> String {
    let secs = unix.max(0.0) as i64;
    let millis = ((unix.max(0.0) - secs as f64) * 1000.0) as u32;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let mut out = String::new();
    let _ = write!(
        out,
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackageInfo;

    #[test]
    fn dsns_give_the_envelope_endpoint_and_key() {
        let dsn = Dsn::parse("https://abc123@beacon.example.com/7").unwrap();
        assert_eq!(dsn.endpoint, "https://beacon.example.com/api/7/envelope/");
        assert_eq!(dsn.key, "abc123");
        let dsn = Dsn::parse("http://k:secret@127.0.0.1:8787/tracker/12/").unwrap();
        assert_eq!(
            dsn.endpoint,
            "http://127.0.0.1:8787/tracker/api/12/envelope/"
        );
        assert_eq!(dsn.key, "k");
        assert!(Dsn::parse("ftp://k@h/1").is_none());
        assert!(Dsn::parse("https://h/1").is_none());
        assert!(Dsn::parse("https://k@h/").is_none());
    }

    #[test]
    fn stacks_become_frames_oldest_first() {
        let frames =
            frames("    at fail (App.tsx:5:9)\n    at <anonymous> (App.tsx:7:1)\n    at native\n");
        assert_eq!(
            Value::Array(frames),
            json!([
                {"filename": "native", "in_app": false},
                {"filename": "App.tsx", "lineno": 7, "colno": 1, "function": "<anonymous>", "in_app": true},
                {"filename": "App.tsx", "lineno": 5, "colno": 9, "function": "fail", "in_app": true},
            ])
        );
    }

    #[test]
    fn messages_give_the_exception_type() {
        assert_eq!(
            exception_type("TypeError: cannot read property 'x' of undefined"),
            (
                "TypeError".into(),
                "cannot read property 'x' of undefined".into()
            )
        );
        assert_eq!(
            exception_type("boom: it broke"),
            ("boom".into(), "it broke".into())
        );
        assert_eq!(
            exception_type("not an error type: x"),
            ("Error".into(), "not an error type: x".into())
        );
        assert_eq!(exception_type("42"), ("Error".into(), "42".into()));
    }

    #[test]
    fn events_carry_the_package_release_and_tags() {
        let config = ErrorReporting::new("https://k@h/1")
            .environment("sit")
            .tag("app.version", "4.55.0");
        let error = AppError::new(
            AppErrorKind::Uncaught,
            "Error: boom\n    at fail (App.tsx:5:9)",
        )
        .with_package(Some(Arc::new(PackageInfo {
            id: "com.example.app".into(),
            name: "Example".into(),
            version: "1.2.0".into(),
            build: 7,
            sha256: "ab".into(),
        })));
        let event = event(
            &config,
            &error,
            false,
            "0123456789abcdef0123456789abcdef",
            1.0,
        );
        assert_eq!(event["release"], "com.example.app@1.2.0+7");
        assert_eq!(event["environment"], "sit");
        assert_eq!(event["level"], "error");
        assert_eq!(event["tags"]["nsh.kind"], "uncaught");
        assert_eq!(event["tags"]["nsh.package"], "com.example.app");
        assert_eq!(event["tags"]["app.version"], "4.55.0");
        let exception = &event["exception"]["values"][0];
        assert_eq!(exception["type"], "Error");
        assert_eq!(exception["value"], "boom");
        assert_eq!(exception["stacktrace"]["frames"][0]["lineno"], 5);
        assert_eq!(exception["mechanism"]["handled"], false);

        let rejection = event_of_rejection(&config);
        assert_eq!(rejection["tags"]["nsh.kind"], "unhandledrejection");
        assert!(rejection.get("release").is_none());
    }

    fn event_of_rejection(config: &ErrorReporting) -> Value {
        let error = AppError::new(AppErrorKind::Uncaught, "Error: offline");
        event(
            config,
            &error,
            true,
            "0123456789abcdef0123456789abcdef",
            1.0,
        )
    }

    #[test]
    fn timestamps_are_rfc3339() {
        assert_eq!(rfc3339(0.0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(1_795_771_800.5), "2026-11-27T09:30:00.500Z");
    }
}
