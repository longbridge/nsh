//! `nsh test --target web`: the apps tests open run on the web's
//! development host, a page of a headless Chromium browser ([`cdp`]). Each
//! `open` builds the entry and serves it with the page in a dev server's
//! test session of its own; the page connects as a host that takes test
//! steps, and its runtime keeps the model of the app
//! (`nsh_testing::agent::Model`). The agent runs here: it asks the page
//! about the app ([`RemoteScene`]) and acts with the browser's trusted
//! input, which pages cannot tell from a user's (`WebNative`).

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use nsh_core::{
    NodeId,
    geometry::{Bounds, Point, Points, Size, point},
};
use nsh_dev::{
    DevServer,
    protocol::{SceneQuery, StepOutcome, TestStep},
};
use nsh_testing::{
    Host, HostOptions, StartHost,
    agent::{Agent, Key, Native, RemoteScene},
};
use serde_json::{Value, json};

use crate::cdp::{self, Browser, Page};

/// How long the page may take to load and connect.
const CONNECT: Duration = Duration::from_secs(60);
/// How long the page may take to answer a question about the app.
const ASK: Duration = Duration::from_secs(10);
/// A press's hold, as a quick click's.
const CLICK: Duration = Duration::from_millis(60);
/// A precise scroll's events, a display frame apart.
const FRAME: Duration = Duration::from_millis(16);
/// How far one wheel event moves at most, in CSS pixels.
const SCROLL_STEP: f32 = 20.0;

/// Starts the apps tests open in pages of `browser` (`--browser`), else of
/// the Chrome, Edge or Chromium found on this machine.
pub(crate) fn starter(browser: Option<&Path>) -> anyhow::Result<StartHost> {
    let exe = cdp::find(browser)?;
    // Built (in a checkout) or found once, before the first test.
    let runtime =
        crate::runtime::web(crate::runtime::WebKind::Dev, true)?.context("no web runtime")?;
    let host = Arc::new(crate::web::dev_host(&runtime, false)?);
    // One browser for the run, started with the first app.
    let running = Arc::new(Mutex::new(None::<Arc<Browser>>));
    Ok(Arc::new(move |entry: &Path, options: &HostOptions| {
        start(&exe, &running, &host, entry, options).map(|h| Box::new(h) as Box<dyn Host>)
    }))
}

/// An app on a page, and the agent that takes its test's steps.
struct WebHost {
    agent: Agent,
    page: Arc<Page>,
    _server: Arc<DevServer>,
}

impl Host for WebHost {
    fn step(&self, step: TestStep, _: Duration) -> Result<Value, String> {
        // The agent's own waits end each step, as a host's do.
        match self.agent.take(step) {
            StepOutcome::Done { value } => Ok(value),
            StepOutcome::Failed { message } => Err(message),
        }
    }
}

impl Drop for WebHost {
    fn drop(&mut self) {
        self.page.close();
    }
}

/// Builds `entry`, serves it with the web's host page and opens that in the
/// browser (started first if it is not running).
fn start(
    exe: &Path,
    running: &Mutex<Option<Arc<Browser>>>,
    host: &(String, Vec<(String, Vec<u8>)>),
    entry: &Path,
    options: &HostOptions,
) -> Result<WebHost, String> {
    let server = Arc::new(crate::test_host::serve(entry)?);
    server.serve_web(host.0.clone(), host.1.clone());
    let browser = {
        let mut running = running.lock().unwrap_or_else(|e| e.into_inner());
        match &*running {
            Some(browser) if browser.alive() => browser.clone(),
            _ => {
                let browser = Arc::new(Browser::launch(exe)?);
                *running = Some(browser.clone());
                browser
            }
        }
    };
    let page = Arc::new(browser.page()?);
    // Focused as the window in front is, whichever page is.
    page.call(
        "Emulation.setFocusEmulationEnabled",
        json!({ "enabled": true }),
    )?;
    let size = options
        .size
        .unwrap_or(Size::new(Points(390.0), Points(844.0)));
    set_size(&page, size.width, size.height)?;
    page.call(
        "Page.navigate",
        json!({ "url": format!("http://{}/", server.local_addr()) }),
    )?;
    let deadline = Instant::now() + CONNECT;
    while !server.has_tester() {
        if Instant::now() >= deadline {
            return Err(format!(
                "the page did not connect in {} s{}",
                CONNECT.as_secs(),
                page.browser().output()
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let asked = server.clone();
    let scene = Arc::new(RemoteScene::new(move |step| asked.step(step, ASK)));
    let native = WebNative {
        page: page.clone(),
        server: server.clone(),
    };
    Ok(WebHost {
        agent: Agent::new(scene, Box::new(native)),
        page,
        _server: server,
    })
}

/// The viewport of `page`, `width` × `height` CSS pixels.
fn set_size(page: &Page, width: Points, height: Points) -> Result<(), String> {
    page.call(
        "Emulation.setDeviceMetricsOverride",
        json!({
            "width": width.0.round() as i64,
            "height": height.0.round() as i64,
            // The display's own.
            "deviceScaleFactor": 0,
            "mobile": false,
        }),
    )
    .map(drop)
}

/// The page's part of the agent: where views are (the page answers) and the
/// browser's input there. CSS pixels are points.
struct WebNative {
    page: Arc<Page>,
    server: Arc<DevServer>,
}

/// Modifier bits of the protocol's input events.
const CTRL: u32 = 2;
const META: u32 = 4;

impl WebNative {
    /// What the page answers of `query`.
    fn query<T: serde::de::DeserializeOwned>(&self, query: SceneQuery) -> Result<T, String> {
        let value = self.server.step(TestStep::Scene { query }, ASK)?;
        serde_json::from_value(value).map_err(|e| format!("the page answered oddly: {e}"))
    }

    fn call(&self, method: &str, params: Value) {
        if let Err(err) = self.page.call(method, params) {
            log::warn!("{err}");
        }
    }

    fn mouse(&self, kind: &str, at: Point<Points>, buttons: u32) {
        self.call(
            "Input.dispatchMouseEvent",
            json!({
                "type": kind,
                "x": at.x.0,
                "y": at.y.0,
                "button": if kind == "mouseMoved" { "none" } else { "left" },
                "buttons": buttons,
                "clickCount": 1,
            }),
        );
    }

    fn click(&self, at: Point<Points>, hold: Duration) {
        self.mouse("mouseMoved", at, 0);
        self.mouse("mousePressed", at, 1);
        std::thread::sleep(hold);
        self.mouse("mouseReleased", at, 0);
    }

    /// A key's press and release. `commands` are the editing commands the
    /// key runs on macOS, where the browser leaves them to the system's
    /// key bindings (none reach it headless), as Playwright sends them.
    fn press_key(&self, key: &str, code: &str, vk: u32, modifiers: u32, commands: &[&str]) {
        let commands: &[&str] = if cfg!(target_os = "macos") {
            commands
        } else {
            &[]
        };
        let event = |kind: &str| {
            json!({
                "type": kind,
                "key": key,
                "code": code,
                "windowsVirtualKeyCode": vk,
                "modifiers": modifiers,
            })
        };
        let mut down = event("rawKeyDown");
        if key == "Enter" {
            down["type"] = "keyDown".into();
            down["text"] = "\r".into();
            down["unmodifiedText"] = "\r".into();
        }
        if !commands.is_empty() {
            down["commands"] = json!(commands);
        }
        self.call("Input.dispatchKeyEvent", down);
        self.call("Input.dispatchKeyEvent", event("keyUp"));
    }
}

impl Native for WebNative {
    fn bounds(&self, id: NodeId) -> Option<Bounds<Points>> {
        let [x, y, width, height] = self
            .query::<Option<[f32; 4]>>(SceneQuery::Bounds { id })
            .ok()??;
        Some(Bounds::new(
            point(Points(x), Points(y)),
            Size::new(Points(width), Points(height)),
        ))
    }

    fn viewport(&self) -> Size<Points> {
        let [width, height] = self
            .query::<[f32; 2]>(SceneQuery::Viewport)
            .unwrap_or([0.0, 0.0]);
        Size::new(Points(width), Points(height))
    }

    fn press(&self, at: Point<Points>) {
        self.click(at, CLICK);
    }

    fn long_press(&self, at: Point<Points>, hold: Duration) {
        self.click(at, hold);
    }

    fn scroll(&self, at: Point<Points>, delta: Point<Points>) {
        self.mouse("mouseMoved", at, 0);
        let (dx, dy) = (delta.x.0, delta.y.0);
        // A precise touchpad's events, a frame apart, adding up to it.
        let n = (dx.abs().max(dy.abs()) / SCROLL_STEP).ceil().max(1.0) as u32;
        for _ in 0..n {
            self.call(
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mouseWheel",
                    "x": at.x.0,
                    "y": at.y.0,
                    "deltaX": dx / n as f32,
                    "deltaY": dy / n as f32,
                }),
            );
            std::thread::sleep(FRAME);
        }
    }

    fn type_text(&self, text: &str) {
        for ch in text.chars() {
            let ch = ch.to_string();
            self.call(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyDown", "key": ch, "text": ch, "unmodifiedText": ch }),
            );
            self.call(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyUp", "key": ch }),
            );
        }
    }

    fn key(&self, key: Key) {
        match key {
            Key::Return => self.press_key("Enter", "Enter", 13, 0, &[]),
            Key::Backspace => self.press_key("Backspace", "Backspace", 8, 0, &["deleteBackward"]),
            Key::SelectAll => {
                // The platform's command key: Command on macOS, else Ctrl.
                let (name, code, vk, bit) = if cfg!(target_os = "macos") {
                    ("Meta", "MetaLeft", 91, META)
                } else {
                    ("Control", "ControlLeft", 17, CTRL)
                };
                let modifier = |kind: &str| {
                    json!({
                        "type": kind,
                        "key": name,
                        "code": code,
                        "windowsVirtualKeyCode": vk,
                        "modifiers": if kind == "keyUp" { 0 } else { bit },
                    })
                };
                self.call("Input.dispatchKeyEvent", modifier("rawKeyDown"));
                self.press_key("a", "KeyA", 65, bit, &["selectAll"]);
                self.call("Input.dispatchKeyEvent", modifier("keyUp"));
            }
        }
    }

    fn input_text(&self, id: NodeId) -> Option<String> {
        self.query::<Option<String>>(SceneQuery::InputText { id })
            .ok()
            .flatten()
    }

    fn back(&self) -> Result<(), String> {
        // The browser's back button: the page's history, which the app's
        // navigation keeps.
        self.page
            .call(
                "Runtime.evaluate",
                json!({ "expression": "history.back()" }),
            )
            .map(drop)
    }

    fn answer_dialog(&self, button: Option<&str>) -> Result<(), String> {
        let [x, y] = self.query::<[f32; 2]>(SceneQuery::DialogPoint {
            button: button.map(str::to_owned),
        })?;
        self.click(point(Points(x), Points(y)), CLICK);
        Ok(())
    }

    fn animating(&self) -> bool {
        self.query::<bool>(SceneQuery::Animating).unwrap_or(false)
    }

    fn screenshot(&self) -> Result<Vec<u8>, String> {
        use base64::Engine as _;
        let shot = self
            .page
            .call("Page.captureScreenshot", json!({ "format": "png" }))?;
        let data = shot["data"]
            .as_str()
            .ok_or("the browser took no screenshot")?;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("the browser's screenshot: {e}"))
    }

    fn resize(&self, width: Points, height: Points) -> Result<(), String> {
        set_size(&self.page, width, height)
    }

    fn set_dark(&self, dark: bool) -> Result<(), String> {
        self.page
            .call(
                "Emulation.setEmulatedMedia",
                json!({
                    "features": [{
                        "name": "prefers-color-scheme",
                        "value": if dark { "dark" } else { "light" },
                    }],
                }),
            )
            .map(drop)
    }

    fn quit(&self) {
        self.page.close();
    }
}
