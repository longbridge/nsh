//! Dev-server wire protocol (WebSocket at [`WS_PATH`]).
//!
//! Control messages are JSON text frames. A bundle is sent as a
//! [`ServerMessage::Bundle`] text frame immediately followed by one binary
//! frame with the payload and, if announced, one binary frame with the
//! source map (JSON). A [`ServerMessage::Patch`] (a hot update) is framed the
//! same way and precedes the bundle of the same version.
//!
//! `nsh test --target` drives a host's app through the same connection: the
//! server's hello asks for a test session, and each [`ServerMessage::Step`]
//! (a [`TestStep`] with an id) gets one [`ClientMessage::StepDone`] with
//! that id, from the host's test agent (`nsh_testing::agent`).

use serde::{Deserialize, Serialize};

/// Protocol version; bump on incompatible changes.
pub const PROTOCOL_VERSION: u32 = 2;

/// WebSocket endpoint path.
pub const WS_PATH: &str = "/ws";

/// Default dev-server port.
pub const DEFAULT_PORT: u16 = 8790;

/// Messages from a dev host to the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message after connecting.
    Hello {
        /// [`PROTOCOL_VERSION`] of the client.
        protocol: u32,
        /// Bytecode format the host's engine can load; the server falls back
        /// to JS source when it differs from its own. Empty: the host runs
        /// JS source only (the web host).
        bytecode_version: String,
        /// Human-readable device name, shown in logs.
        device: String,
        /// What kind of host it is, for the command that installs another
        /// (`None`: a host that does not say).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        platform: Option<HostPlatform>,
        /// The UI sets the host serves, by specifier (an
        /// app's own `gallery/native`): bundles importing others are not
        /// sent to it ([`ServerMessage::CannotRun`]). `None`: a host that
        /// does not say, which gets every bundle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ui_sets: Option<Vec<String>>,
        /// The host takes test steps ([`ServerMessage::Step`]).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        tests: bool,
    },
    /// The outcome of the [`ServerMessage::Step`] with this id.
    StepDone {
        /// The step's id.
        id: u64,
        /// What it gave, or why it failed.
        outcome: StepOutcome,
    },
    /// A log line from the host (JS console or native).
    Log {
        /// Severity.
        level: LogLevel,
        /// Source, e.g. `js` or a Rust module path.
        target: String,
        /// The message.
        message: String,
    },
}

/// What kind of host a dev host is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HostPlatform {
    /// The Mac Catalyst host.
    MacCatalyst,
    /// A host in an iOS simulator.
    IosSimulator,
    /// A host on an iOS device.
    Ios,
    /// A host on an Android device or emulator.
    Android,
    /// The Linux (GTK) host.
    Linux,
    /// The macOS (AppKit) host.
    MacOs,
    /// The Windows host.
    Windows,
    /// The web host: a browser page the dev server serves.
    Web,
    /// The GPUI host (macOS, Windows, Linux).
    Gpui,
    /// A platform this version does not know.
    #[serde(other)]
    Other,
}

/// Messages from the server to dev hosts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Reply to [`ClientMessage::Hello`].
    Hello {
        /// [`PROTOCOL_VERSION`] of the server.
        protocol: u32,
        /// This run of the server: build numbers start at 1 in each run, so
        /// a host still running a build of an earlier run tells them apart
        /// by it.
        #[serde(default)]
        session: u64,
        /// A test session: the host keeps the model of its app that steps
        /// need from the first bundle on ([`ServerMessage::Step`]).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        test: bool,
    },
    /// A step of a test, which the host answers with a
    /// [`ClientMessage::StepDone`] of the same id.
    Step {
        /// Its id.
        id: u64,
        /// What to do.
        step: TestStep,
    },
    /// A rebuild started.
    Building,
    /// The host cannot run the app: the latest bundle imports UI sets it
    /// does not serve (it was built for another app). No bundle is sent to
    /// it until one needs only what it has; the host shows `message`.
    CannotRun {
        /// The UI sets the app imports that the host does not serve.
        missing: Vec<String>,
        /// What to tell the developer: what is missing, and how to install
        /// the app's host.
        message: String,
    },
    /// The build failed; the host should show the error.
    BuildFailed {
        /// Compiler output.
        message: String,
    },
    /// A hot update from bundle `base` to bundle `version`: the next binary
    /// frame is a script (UTF-8 JS) that replaces the changed modules in
    /// place. Hosts running `base` apply it; the bundle of `version`
    /// follows either way.
    Patch {
        /// Build number after the update.
        version: u64,
        /// Build number the update applies to.
        base: u64,
        /// A second binary frame with the source map follows.
        #[serde(default)]
        source_map: bool,
    },
    /// A new bundle; the next binary frame is its payload.
    Bundle {
        /// Monotonic build number.
        version: u64,
        /// Payload format.
        format: BundleFormat,
        /// A second binary frame with the source map follows.
        #[serde(default)]
        source_map: bool,
        /// What the app declares it may reach (JSON), so development runs
        /// with the same capabilities as the release.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capabilities: Option<String>,
        /// The app's id (`nsh.json` `"id"`), which names where it keeps its
        /// data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app_id: Option<String>,
        /// The VM limits to run the app with, as an embedding app would
        /// set them (`nsh run --limits`, in `nsh_core::js::HostLimits`'
        /// syntax: `memory=128MB,call=500ms`; empty: the embedding
        /// defaults). `None`: none, as an app on its own has.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limits: Option<String>,
    },
}

/// A step of a test on a host's app, named as `nsh/test` names them: views
/// are found by selectors (`id:save`, `text:Add`, `role:button[Save]`), and
/// actions are native input at the view the selector names.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum TestStep {
    /// Waits for the app's first render, then until it is quiet.
    Ready {
        /// How long to wait for the first render, in milliseconds.
        timeout_ms: u64,
    },
    /// Presses the view (`app.press`).
    Press {
        /// The view.
        selector: String,
    },
    /// Holds a press on the view for its long press delay (`app.longPress`).
    LongPress {
        /// The view.
        selector: String,
    },
    /// Types `text` into the input (`app.type`).
    Type {
        /// The input.
        selector: String,
        /// What to type; `\n` is the return key in a single-line input.
        text: String,
    },
    /// The return key in the input (`app.submit`).
    Submit {
        /// The input.
        selector: String,
    },
    /// Empties the input (`app.clear`).
    Clear {
        /// The input.
        selector: String,
    },
    /// Scrolls the scroll view by `distance` points along its axis
    /// (`app.scroll`).
    Scroll {
        /// The scroll view.
        selector: String,
        /// Points; negative scrolls back.
        distance: f32,
    },
    /// Goes back as the user does (`app.back`).
    Back,
    /// Lets the app run (`app.wait`).
    Wait {
        /// Milliseconds.
        ms: u64,
    },
    /// Waits until a view matches, or none does (`app.waitFor`).
    WaitFor {
        /// The view.
        selector: String,
        /// Milliseconds at most.
        timeout_ms: u64,
        /// Until none matches.
        gone: bool,
    },
    /// Waits for the next batch, `timeout_ms` at most: what polling a
    /// condition waits for between looks (`app.waitUntil`).
    NextChange {
        /// Milliseconds at most.
        timeout_ms: u64,
    },
    /// The texts on screen, in order: a list of strings.
    Texts,
    /// How many views match: a number.
    Count {
        /// The selector.
        selector: String,
    },
    /// Whether a view matches: a boolean.
    Shows {
        /// The selector.
        selector: String,
    },
    /// Whether the view takes input: a boolean.
    Enabled {
        /// The selector.
        selector: String,
    },
    /// The text of the input: a string, or null when none matches.
    Value {
        /// The selector.
        selector: String,
    },
    /// The dialogs the app showed, in order (`app.dialogs`): a list of
    /// `nsh_core::Dialog`s.
    Dialogs,
    /// The oldest dialog not answered yet: an `nsh_core::Dialog`, or null.
    PendingDialog,
    /// Answers the oldest dialog not answered yet: presses its button
    /// `button`, or closes it without one.
    AnswerDialog {
        /// The button's text (`None`: close it without one).
        button: Option<String>,
    },
    /// A picture of the app's window: a PNG, base64-encoded (the test's
    /// side saves it: the host may run on another machine).
    Screenshot,
    /// Makes the window `width` × `height` points (`app.setViewport`).
    SetViewport {
        /// Points.
        width: f32,
        /// Points.
        height: f32,
    },
    /// Switches the system's appearance as the app sees it
    /// (`app.setColorScheme`).
    SetColorScheme {
        /// Dark rather than light.
        dark: bool,
    },
    /// Closes the app: the host exits.
    Quit,
}

/// How a [`TestStep`] ended.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum StepOutcome {
    /// It was taken; `value` is what a query gives (`null` for actions).
    Done {
        /// The value.
        #[serde(default)]
        value: serde_json::Value,
    },
    /// It could not be taken, or the app threw meanwhile.
    Failed {
        /// Why, with what is on screen.
        message: String,
    },
}

/// Format of a bundle payload.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BundleFormat {
    /// QuickJS bytecode.
    Bytecode,
    /// UTF-8 JS source.
    Source,
}

/// Log severity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Errors.
    Error,
    /// Warnings.
    Warn,
    /// Informational.
    Info,
    /// Debugging.
    Debug,
    /// Very verbose.
    Trace,
}

impl From<log::Level> for LogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => LogLevel::Error,
            log::Level::Warn => LogLevel::Warn,
            log::Level::Info => LogLevel::Info,
            log::Level::Debug => LogLevel::Debug,
            log::Level::Trace => LogLevel::Trace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_is_stable() {
        let msg = ServerMessage::Bundle {
            version: 3,
            format: BundleFormat::Bytecode,
            source_map: true,
            capabilities: None,
            app_id: None,
            limits: None,
        };
        // No declaration: the field is left out (older hosts read the same).
        assert_eq!(
            serde_json::to_string(&msg).unwrap(),
            r#"{"type":"bundle","version":3,"format":"bytecode","source_map":true}"#
        );
        let hello: ClientMessage = serde_json::from_str(
            r#"{"type":"hello","protocol":1,"bytecode_version":"x","device":"Mac"}"#,
        )
        .unwrap();
        // An older host says nothing of its platform or UI sets.
        assert!(matches!(
            hello,
            ClientMessage::Hello {
                protocol: 1,
                platform: None,
                ui_sets: None,
                ..
            }
        ));
        let hello: ClientMessage = serde_json::from_str(
            r#"{"type":"hello","protocol":2,"bytecode_version":"x","device":"Mac","platform":"vision","ui_sets":["app/native","app/charts"]}"#,
        )
        .unwrap();
        assert!(matches!(
            hello,
            ClientMessage::Hello {
                platform: Some(HostPlatform::Other),
                ui_sets: Some(sets),
                ..
            } if sets == ["app/native", "app/charts"]
        ));
    }
}
