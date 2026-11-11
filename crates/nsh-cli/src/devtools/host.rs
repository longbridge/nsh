//! The host a test drives over DevTools: its steps are the endpoint's
//! methods, its network the test's, its trace and log lines the events the
//! endpoint sends.

use std::{
    collections::HashMap,
    path::PathBuf,
    process::Child,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nsh_dev::{
    DevServer,
    protocol::{FetchReply, TestStep, WireRequest},
};
use nsh_devtools::{client::Client, console::ConsoleEntry, methods::method_of};
use nsh_testing::{Host, HostOptions};
use serde_json::{Value, json};

use super::{artifacts::FileRun, launch::Launched, timeline::Timeline};

/// The functions that cancel the requests in flight, by id.
type Cancels = HashMap<u64, Box<dyn FnOnce() + Send>>;

/// A host running one app for a test, driven over its DevTools endpoint.
pub(crate) struct DevtoolsHost {
    /// Serves the app's bundle and assets; the steps do not go through it.
    /// None for a packaged app (`nsh test --app`), which has them itself.
    _server: Option<DevServer>,
    /// The host's job (Windows): it closes after the host is gone, or with
    /// this process.
    _job: super::launch::Job,
    child: Child,
    client: Arc<Client>,
    /// The endpoint's URL and token, which another client connects with.
    url: String,
    token: String,
    /// The host's last output lines, shown when it dies mid-test.
    output: crate::ui::Tail,
    /// The app's data directory, fresh for it and removed after it.
    data: PathBuf,
    /// The test file's artifacts, and this host's place among its hosts.
    file: Option<(Arc<FileRun>, u64)>,
}

impl DevtoolsHost {
    /// Sets the app up as the test's `options` say, starts it and begins
    /// serving its events.
    pub(crate) fn start(
        server: Option<DevServer>,
        launched: Launched,
        data: PathBuf,
        options: &HostOptions,
        file: Option<Arc<FileRun>>,
    ) -> Result<DevtoolsHost, String> {
        let Launched {
            child,
            job,
            client,
            output,
            url,
            token,
        } = launched;
        let events = client.events();
        let client = Arc::new(client);
        let file = file.map(|file| {
            let id = file.add(client.clone(), url.clone(), token.clone());
            (file, id)
        });
        // Its drop quits the app and cleans up, also when the setup fails.
        let host = DevtoolsHost {
            _server: server,
            _job: job,
            child,
            client: client.clone(),
            url,
            token,
            output,
            data,
            file: file.clone(),
        };
        let setup = json!({
            "storage": options.storage,
            "trace": options.trace.is_some(),
            "environment": serde_json::to_value(&options.environment)
                .map_err(|e| e.to_string())?,
        });
        client
            .call("Session.setup", setup)
            .map_err(|e| format!("Session.setup: {}", e.message))?;
        let network = options.network.clone();
        let trace = options.trace.clone();
        let timeline = file.map(|(file, _)| file);
        std::thread::Builder::new()
            .name("nsh-devtools-events".into())
            .spawn(move || {
                // The requests in flight; a final reply removes its entry.
                let cancels: Arc<Mutex<Cancels>> = Arc::default();
                let done = cancels.clone();
                while let Ok(event) = events.recv() {
                    match event.method.as_str() {
                        "Network.request" => {
                            let id = event.params["id"].as_u64();
                            let request: Option<WireRequest> =
                                serde_json::from_value(event.params["request"].clone()).ok();
                            let (Some(id), Some(request)) = (id, request) else {
                                continue;
                            };
                            let Some(network) = &network else {
                                // No stubbed network: the host makes the
                                // request itself, as the dev server's test
                                // session lets it.
                                let _ = client.call(
                                    "Network.respond",
                                    json!({ "id": id, "reply": { "reply": "pass" } }),
                                );
                                continue;
                            };
                            let client = client.clone();
                            let done = done.clone();
                            let cancel = network.fetch(
                                request,
                                Box::new(move |reply: FetchReply| {
                                    if matches!(
                                        reply,
                                        FetchReply::Pass
                                            | FetchReply::End
                                            | FetchReply::Failed { .. }
                                    ) {
                                        done.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                                    }
                                    let Ok(reply) = serde_json::to_value(reply) else {
                                        return;
                                    };
                                    let _ = client.call(
                                        "Network.respond",
                                        json!({ "id": id, "reply": reply }),
                                    );
                                }),
                            );
                            cancels
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .insert(id, cancel);
                        }
                        "Network.cancel" => {
                            let cancel = event.params["id"].as_u64().and_then(|id| {
                                cancels
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .remove(&id)
                            });
                            if let Some(cancel) = cancel {
                                cancel();
                            }
                        }
                        "Trace.batch" => {
                            if let Some(trace) = &trace {
                                let items = event.params["items"]
                                    .as_array()
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|i| i.as_str().map(str::to_owned))
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                trace(items);
                            }
                        }
                        "Console.entry" => {
                            let entry: Option<ConsoleEntry> =
                                serde_json::from_value(event.params.clone()).ok();
                            if let (Some(entry), Some(file)) = (entry, &timeline)
                                && let Some(timeline) = &file.timeline
                            {
                                timeline.console(&entry);
                            }
                        }
                        "Console.error" => {
                            if let Some(message) = event.params["message"].as_str() {
                                crate::logger::forward(log::Level::Error, "js", message);
                            }
                        }
                        _ => {}
                    }
                }
            })
            .map_err(|e| format!("cannot start the events thread: {e}"))?;
        if host.timeline().is_some() {
            // The console lines of the apps go to the timeline, those of
            // the app's start too. Artifacts are a convenience: a failure
            // to subscribe only warns.
            if let Err(e) = host.client.call("Console.subscribe", Value::Null) {
                crate::ui::warn(format!("no console lines in the timeline: {}", e.message));
            }
        }
        host.client
            .call("Session.start", Value::Null)
            .map_err(|e| format!("Session.start: {}", e.message))?;
        Ok(host)
    }
}

impl DevtoolsHost {
    /// The connection to the host's endpoint.
    pub(crate) fn client(&self) -> Arc<Client> {
        self.client.clone()
    }

    /// The endpoint's URL.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// The token the endpoint was given.
    pub(crate) fn token(&self) -> &str {
        &self.token
    }

    /// The host process's id.
    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Whether the host process has ended, and if so whether it exited
    /// cleanly (an app that quit, as `close` asks).
    pub(crate) fn exit(&mut self) -> Option<bool> {
        match self.child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) => Some(status.success()),
            Err(_) => Some(false),
        }
    }

    /// The host's last output lines.
    pub(crate) fn output(&self) -> String {
        self.output.text()
    }

    /// The timeline the steps go to (`--artifacts`).
    fn timeline(&self) -> Option<&Timeline> {
        self.file
            .as_ref()
            .and_then(|(file, _)| file.timeline.as_ref())
    }
}

impl Host for DevtoolsHost {
    fn step(&self, step: TestStep, timeout: Duration) -> Result<Value, String> {
        let Some((method, params)) = method_of(&step) else {
            return Err(format!("the step {step:?} is not available over DevTools"));
        };
        let timeline = self.timeline();
        let began = timeline.map(|t| t.now());
        let params_kept = timeline.map(|_| params.clone());
        let outcome = self
            .client
            .call_within(method, params, timeout)
            .map_err(|e| {
                if e.code == nsh_devtools::client::ERR_CLOSED {
                    // The host died: say what it printed last.
                    format!(
                        "{}; the host is gone{}",
                        e.message,
                        crate::test_host::indented(&self.output.text())
                    )
                } else {
                    e.message
                }
            });
        if let (Some(timeline), Some(began), Some(params)) = (timeline, began, params_kept) {
            timeline.step(began, method, &params, &outcome);
        }
        outcome
    }
}

impl Drop for DevtoolsHost {
    fn drop(&mut self) {
        if let Some((file, id)) = &self.file {
            file.remove(*id);
        }
        // It quits after the test's `Quit` or on `Session.quit`; a host that
        // does not goes.
        let _ = self
            .client
            .call_within("Session.quit", Value::Null, Duration::from_secs(1));
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut exited = false;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                exited = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !exited {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.data);
    }
}
