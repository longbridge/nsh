//! The console collector: the app's log lines, the newest kept.

use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        mpsc::{Receiver, Sender, channel},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

/// How many entries a collector keeps by default.
pub const DEFAULT_CAPACITY: usize = 10_000;

/// One log line.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ConsoleEntry {
    /// Milliseconds since the Unix epoch.
    pub time_ms: u64,
    /// `ERROR`, `WARN`, `INFO`, `DEBUG` or `TRACE`.
    pub level: String,
    /// The log target (`js`, a Rust module path).
    pub target: String,
    /// The logging thread's name.
    pub thread: String,
    /// The message.
    pub message: String,
}

/// Which entries to read. Every field set narrows the answer. On the wire
/// `level` is a name (`"warn"`, any case) and the other fields keep their names.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ConsoleQuery {
    /// Entries at or after this time.
    pub since_ms: Option<u64>,
    /// Entries of this level or more severe.
    #[serde(deserialize_with = "level_from_name")]
    pub level: Option<log::Level>,
    /// Entries of exactly this target.
    pub target: Option<String>,
    /// Entries whose message contains this text.
    pub text: Option<String>,
    /// At most this many, the newest.
    pub limit: Option<usize>,
}

fn level_from_name<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<log::Level>, D::Error> {
    let name = Option::<String>::deserialize(deserializer)?;
    name.map(|n| n.parse().map_err(serde::de::Error::custom))
        .transpose()
}

/// Keeps the newest log lines and hands new ones to subscribers.
pub struct Collector {
    capacity: usize,
    entries: Mutex<VecDeque<ConsoleEntry>>,
    subscribers: Mutex<Vec<Sender<ConsoleEntry>>>,
}

impl Default for Collector {
    fn default() -> Self {
        Collector::new(DEFAULT_CAPACITY)
    }
}

impl Collector {
    /// A collector keeping `capacity` entries.
    pub fn new(capacity: usize) -> Collector {
        Collector {
            capacity,
            entries: Mutex::new(VecDeque::new()),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Keeps a log record and sends it to the subscribers.
    pub fn record(&self, record: &log::Record<'_>) {
        let entry = ConsoleEntry {
            time_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            level: record.level().as_str().to_string(),
            target: record.target().to_string(),
            thread: std::thread::current().name().unwrap_or("").to_string(),
            message: record.args().to_string(),
        };
        {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if self.capacity > 0 {
                while entries.len() >= self.capacity {
                    entries.pop_front();
                }
                entries.push_back(entry.clone());
            }
        }
        // A subscriber that dropped its receiver is forgotten.
        self.subscribers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|tx| tx.send(entry.clone()).is_ok());
    }

    /// The entries matching `q`, oldest first.
    pub fn query(&self, q: &ConsoleQuery) -> Vec<ConsoleEntry> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<ConsoleEntry> = entries
            .iter()
            .filter(|e| q.since_ms.is_none_or(|t| e.time_ms >= t))
            .filter(|e| {
                q.level.is_none_or(|max| {
                    e.level
                        .parse::<log::Level>()
                        .is_ok_and(|level| level <= max)
                })
            })
            .filter(|e| q.target.as_ref().is_none_or(|t| &e.target == t))
            .filter(|e| {
                q.text
                    .as_ref()
                    .is_none_or(|t| e.message.contains(t.as_str()))
            })
            .cloned()
            .collect();
        if let Some(limit) = q.limit
            && out.len() > limit
        {
            out.drain(..out.len() - limit);
        }
        out
    }

    /// A receiver of the entries recorded from now on.
    pub fn subscribe(&self) -> Receiver<ConsoleEntry> {
        let (tx, rx) = channel();
        self.subscribers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tx);
        rx
    }
}
