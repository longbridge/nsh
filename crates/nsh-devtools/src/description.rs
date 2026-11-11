//! The protocol's description, generated from the method table.

use serde_json::{Value, json};

use crate::{PROTOCOL_VERSION, methods::METHODS, protocol::ERROR_CODES};

/// The events the endpoint sends, with their payloads.
const EVENTS: &[(&str, &str)] = &[
    ("Session.ready", "The app finished its first render."),
    (
        "Console.entry",
        "A log line: time_ms, level, target, thread, message.",
    ),
    ("Network.request", "A request the app made."),
    (
        "Trace.batch",
        "A batch the app applied, as mount item lines.",
    ),
];

/// What `Protocol.describe` answers: the version, every method, event and error code.
pub fn description() -> Value {
    json!({
        "version": PROTOCOL_VERSION,
        "stability": "experimental",
        "methods": METHODS
            .iter()
            .map(|m| json!({
                "name": m.method,
                "kind": m.kind.name(),
                "capability": m.capability,
                "doc": m.doc,
            }))
            .collect::<Vec<_>>(),
        "events": EVENTS
            .iter()
            .map(|(name, doc)| json!({ "name": name, "doc": doc }))
            .collect::<Vec<_>>(),
        "errors": ERROR_CODES
            .iter()
            .map(|(code, doc)| json!({ "code": code, "doc": doc }))
            .collect::<Vec<_>>(),
    })
}
