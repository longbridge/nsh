//! The app's errors go to the error tracker of `EngineConfig::error_reporting`
//! as Sentry envelopes, over the platform's network.

use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use nsh_core::{
    ApproxTextEngine, MountBatch, Size,
    geometry::px,
    platform::{FetchEvent, HttpBody, HttpHead, HttpRequest},
};
use nsh_engine::{
    Bundle, Engine, EngineConfig, ErrorReporting, FetchEvents, FetchTask, MountSink, Network,
    PackageInfo,
};
use serde_json::Value;

struct Quiet;

impl MountSink for Quiet {
    fn submit(&self, _batch: MountBatch) {}
}

/// Answers every request with 200 and hands it to the test.
struct Tracker(Mutex<mpsc::Sender<HttpRequest>>);

struct Done;

impl FetchTask for Done {
    fn send(&mut self, _chunk: Vec<u8>) {}
    fn finish(&mut self) {}
    fn pause(&mut self, _paused: bool) {}
    fn cancel(self: Box<Self>) {}
}

impl Network for Tracker {
    fn fetch(&self, request: HttpRequest, mut events: FetchEvents) -> Box<dyn FetchTask> {
        let head = HttpHead {
            status: 200,
            url: request.url.clone(),
            ..HttpHead::default()
        };
        let _ = self.0.lock().unwrap().send(request);
        events(FetchEvent::Head(head));
        events(FetchEvent::End);
        Box::new(Done)
    }
}

/// The app throws in a timer on line 5 of `App.tsx` and leaves a rejected
/// promise unhandled on line 8.
fn app() -> Bundle {
    let dir = std::env::temp_dir().join(format!("nsh-reporting-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(
        &entry,
        "import { mount, View } from \"nsh\";\n\
         const message: string = \"boom\";\n\
         function fail(): void {\n\
         \x20 // line 4\n\
         \x20 throw new Error(message);\n\
         }\n\
         setTimeout(fail, 0);\n\
         Promise.reject(new Error(\"offline\"));\n\
         mount(() => <View />);\n",
    )
    .unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: vec![nsh_base::registry().into()],
        ..Default::default()
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = Bundle::source(bundle.code).with_source_map(bundle.source_map.expect("map"));
    app.package = Some(Arc::new(PackageInfo {
        id: "com.example.reporting".into(),
        name: "Reporting".into(),
        version: "1.2.0".into(),
        build: 7,
        sha256: "ab12".into(),
    }));
    app
}

/// The event of an envelope, checking its framing.
fn event(request: &HttpRequest) -> Value {
    let HttpBody::Bytes(body) = &request.body else {
        panic!("no body");
    };
    let text = std::str::from_utf8(body).unwrap();
    if std::env::var_os("NSH_REPORTING_PRINT").is_some() {
        println!("ENVELOPE {}", serde_json::to_string(text).unwrap());
    }
    let mut lines = text.splitn(3, '\n');
    let header: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    let item: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    let payload = lines.next().unwrap();
    assert_eq!(item["type"], "event");
    let length = item["length"].as_u64().unwrap() as usize;
    assert_eq!(&payload[length..], "\n");
    let event: Value = serde_json::from_str(&payload[..length]).unwrap();
    assert_eq!(header["event_id"], event["event_id"]);
    assert_eq!(
        header["dsn"],
        "https://pubkeypubkeypubkey01@beacon.example.com/3"
    );
    event
}

#[test]
fn errors_and_rejections_go_to_the_tracker() {
    let (tx, rx) = mpsc::channel();
    let mut config = EngineConfig::new(app());
    config.viewport = Size::new(px(100.0), px(100.0));
    config.network = Some(Arc::new(Tracker(Mutex::new(tx))));
    config.error_reporting = Some(
        ErrorReporting::new("https://pubkeypubkeypubkey01@beacon.example.com/3").environment("sit"),
    );
    let _engine = Engine::start(config, || ApproxTextEngine, Quiet).unwrap();

    let mut events = Vec::new();
    while events.len() < 2 {
        let request = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("two reports");
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://beacon.example.com/api/3/envelope/");
        let auth = request
            .headers
            .iter()
            .find(|(name, _)| name == "X-Sentry-Auth")
            .map(|(_, value)| value.as_str())
            .unwrap_or_default();
        assert!(auth.contains("sentry_key=pubkeypubkeypubkey01"), "{auth}");
        events.push(event(&request));
    }
    events.sort_by_key(|event| {
        event["tags"]["nsh.kind"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    });

    let (uncaught, rejection) = (&events[0], &events[1]);
    assert_eq!(uncaught["tags"]["nsh.kind"], "uncaught");
    assert_eq!(uncaught["release"], "com.example.reporting@1.2.0+7");
    assert_eq!(uncaught["environment"], "sit");
    assert_eq!(uncaught["tags"]["nsh.package"], "com.example.reporting");
    let exception = &uncaught["exception"]["values"][0];
    assert_eq!(exception["type"], "Error");
    assert_eq!(exception["value"], "boom");
    let frames = exception["stacktrace"]["frames"].as_array().unwrap();
    let top = frames.last().unwrap();
    assert_eq!(
        top["filename"], "App.tsx",
        "mapped to the sources: {frames:?}"
    );
    assert_eq!(top["lineno"], 5);
    assert_eq!(top["function"], "fail");

    assert_eq!(rejection["tags"]["nsh.kind"], "unhandledrejection");
    assert_eq!(rejection["exception"]["values"][0]["value"], "offline");
    assert_eq!(rejection["release"], "com.example.reporting@1.2.0+7");
}

#[test]
fn nothing_is_sent_without_reporting() {
    let (tx, rx) = mpsc::channel();
    let mut config = EngineConfig::new(app());
    config.network = Some(Arc::new(Tracker(Mutex::new(tx))));
    let _engine = Engine::start(config, || ApproxTextEngine, Quiet).unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(1500)).is_err());
}
