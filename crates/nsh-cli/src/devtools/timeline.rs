//! The timeline of a test file's run over DevTools: every step the test took
//! and every console line the apps printed, in time order, one JSON object
//! per line (`timeline.jsonl` of `nsh test --artifacts`).

use std::{io::Write as _, path::Path, sync::Mutex, time::Instant};

use nsh_devtools::console::ConsoleEntry;
use serde_json::{Value, json};

/// What a test file's run did, in milliseconds since the file started.
pub(crate) struct Timeline {
    /// Milliseconds since the file started; a test fakes it.
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
    /// The lines with their times, in the order they were recorded.
    lines: Mutex<Vec<(u64, Value)>>,
}

impl Timeline {
    /// A timeline whose time starts now.
    pub(crate) fn start() -> Timeline {
        let started = Instant::now();
        Timeline::with_clock(move || started.elapsed().as_millis() as u64)
    }

    /// A timeline that reads the time from `clock`.
    pub(crate) fn with_clock(clock: impl Fn() -> u64 + Send + Sync + 'static) -> Timeline {
        Timeline {
            clock: Box::new(clock),
            lines: Mutex::new(Vec::new()),
        }
    }

    /// Milliseconds since the file started.
    pub(crate) fn now(&self) -> u64 {
        (self.clock)()
    }

    /// A step that began at `t_ms` and ended now: its method, params,
    /// duration and outcome.
    pub(crate) fn step(
        &self,
        t_ms: u64,
        method: &str,
        params: &Value,
        outcome: &Result<Value, String>,
    ) {
        let mut line = json!({
            "t_ms": t_ms,
            "method": method,
            "params": params,
            "ms": self.now().saturating_sub(t_ms),
            "ok": outcome.is_ok(),
        });
        if let Err(error) = outcome {
            line["error"] = Value::String(error.clone());
        }
        self.push(t_ms, line);
    }

    /// A console line an app printed, now.
    pub(crate) fn console(&self, entry: &ConsoleEntry) {
        let t_ms = self.now();
        let console = serde_json::to_value(entry).unwrap_or_default();
        self.push(t_ms, json!({ "t_ms": t_ms, "console": console }));
    }

    fn push(&self, t_ms: u64, line: Value) {
        self.lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((t_ms, line));
    }

    /// The lines in time order (those at the same time in the order they
    /// were recorded).
    pub(crate) fn lines(&self) -> Vec<Value> {
        let mut lines = self.lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
        lines.sort_by_key(|(t, _)| *t);
        lines.into_iter().map(|(_, line)| line).collect()
    }

    /// Writes the lines to `path`.
    pub(crate) fn write(&self, path: &Path) -> std::io::Result<()> {
        let mut text = String::new();
        for line in self.lines() {
            text.push_str(&line.to_string());
            text.push('\n');
        }
        let mut file = std::fs::File::create(path)?;
        file.write_all(text.as_bytes())
    }
}

#[cfg(test)]
mod devtools_artifacts {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    use super::*;

    #[test]
    fn timeline_records_steps_and_console_in_order() {
        let now = Arc::new(AtomicU64::new(0));
        let clock = now.clone();
        let timeline = Timeline::with_clock(move || clock.load(Ordering::Relaxed));

        // A first step, 0 to 10 ms.
        now.store(10, Ordering::Relaxed);
        timeline.step(
            0,
            "Input.press",
            &json!({ "selector": "Add" }),
            &Ok(Value::Null),
        );
        // A console line between the steps.
        now.store(15, Ordering::Relaxed);
        let mut entry = ConsoleEntry::default();
        entry.level = "WARN".into();
        entry.message = "slow".into();
        timeline.console(&entry);
        // A second step, 20 to 35 ms, that failed; recorded after the
        // console line, as it ended later.
        now.store(35, Ordering::Relaxed);
        timeline.step(
            20,
            "Tree.count",
            &json!({ "selector": "Gone" }),
            &Err("no such view".into()),
        );

        let lines = timeline.lines();
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[0],
            json!({
                "t_ms": 0,
                "method": "Input.press",
                "params": { "selector": "Add" },
                "ms": 10,
                "ok": true
            })
        );
        assert_eq!(lines[1]["t_ms"], 15);
        assert_eq!(lines[1]["console"]["message"], "slow");
        assert_eq!(lines[1]["console"]["level"], "WARN");
        assert_eq!(lines[2]["t_ms"], 20);
        assert_eq!(lines[2]["method"], "Tree.count");
        assert_eq!(lines[2]["ms"], 15);
        assert_eq!(lines[2]["ok"], false);
        assert_eq!(lines[2]["error"], "no such view");
    }
}
