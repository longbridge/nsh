use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nsh_dev::protocol::{FetchReply, TestStep};
use nsh_devtools::{
    PATH,
    client::{Client, ClientError},
    console::Collector,
    protocol::{ERR_INPUT_LOCKED, ErrorBody},
    server::{Endpoint, EndpointConfig, Handler, SessionSetup},
};
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(2);

/// Answers every step with `{"ok": true}` and remembers what it was given.
#[derive(Default)]
struct Fake {
    log: Mutex<Vec<String>>,
    steps: Mutex<Vec<TestStep>>,
}

impl Handler for Fake {
    fn setup(&self, _: SessionSetup) -> Result<(), ErrorBody> {
        self.log.lock().unwrap().push("setup".into());
        Ok(())
    }
    fn start(&self) {
        self.log.lock().unwrap().push("start".into());
    }
    fn step(&self, step: TestStep, done: Box<dyn FnOnce(Result<Value, String>) + Send>) {
        self.steps.lock().unwrap().push(step);
        done(Ok(json!({ "ok": true })));
    }
    fn fetch_reply(&self, _: u64, _: FetchReply) {}
}

struct Rig {
    endpoint: Endpoint,
    handler: Arc<Fake>,
    console: Arc<Collector>,
    url: String,
}

fn rig(config: EndpointConfig) -> Rig {
    let handler = Arc::new(Fake::default());
    let console = Arc::new(Collector::default());
    let endpoint = Endpoint::start(config, handler.clone(), console.clone()).unwrap();
    let url = format!("ws://{}{PATH}", endpoint.addr());
    Rig {
        endpoint,
        handler,
        console,
        url,
    }
}

fn config() -> EndpointConfig {
    let mut config = EndpointConfig::default();
    config.token = "secret".into();
    config
}

fn connect(rig: &Rig) -> Client {
    Client::connect(&rig.url, "secret", T).unwrap()
}

fn press(client: &Client) -> Result<Value, ErrorBody> {
    client.call("Input.press", json!({ "selector": "id:go" }))
}

#[test]
fn wrong_token_is_refused() {
    let rig = rig(config());
    assert!(matches!(
        Client::connect(&rig.url, "nope", T),
        Err(ClientError::Unauthorized)
    ));
}

#[test]
fn hello_lists_methods_and_version() {
    let rig = rig(config());
    let hello = connect(&rig).call("Session.hello", json!({})).unwrap();
    assert_eq!(hello["version"], 1);
    let methods = hello["methods"].as_array().unwrap();
    assert!(methods.iter().any(|m| m == "Input.press"));
}

#[test]
fn a_method_reaches_the_handler_as_its_step() {
    let rig = rig(config());
    let result = press(&connect(&rig)).unwrap();
    assert_eq!(result, json!({ "ok": true }));
    assert_eq!(
        *rig.handler.steps.lock().unwrap(),
        [TestStep::Press {
            selector: "id:go".into()
        }]
    );
}

#[test]
fn second_actor_is_locked_out_until_release() {
    let rig = rig(config());
    let (a, b) = (connect(&rig), connect(&rig));
    press(&a).unwrap();
    assert_eq!(press(&b).unwrap_err().code, ERR_INPUT_LOCKED);
    b.call("Tree.count", json!({ "selector": "id:go" }))
        .unwrap();
    a.call("Session.release", json!({})).unwrap();
    press(&b).unwrap();
}

#[test]
fn lock_goes_with_a_dropped_connection() {
    let rig = rig(config());
    let a = connect(&rig);
    press(&a).unwrap();
    a.close();
    let b = connect(&rig);
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match press(&b) {
            Ok(_) => break,
            Err(e) => {
                assert_eq!(e.code, ERR_INPUT_LOCKED);
                assert!(Instant::now() < deadline, "the lock stayed");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[test]
fn wait_times_out_without_start() {
    let mut cfg = config();
    cfg.wait = true;
    cfg.wait_timeout = Duration::from_millis(200);
    let rig = rig(cfg);
    let began = Instant::now();
    assert!(rig.endpoint.wait_for_start().is_err());
    assert!(began.elapsed() >= Duration::from_millis(200));
}

#[test]
fn wait_returns_after_setup_and_start() {
    let mut cfg = config();
    cfg.wait = true;
    cfg.wait_timeout = Duration::from_secs(2);
    let rig = rig(cfg);
    let client = connect(&rig);
    // Steps are refused until the session started.
    assert_eq!(
        press(&client).unwrap_err().code,
        nsh_devtools::protocol::ERR_NOT_STARTED
    );
    client
        .call("Session.setup", json!({ "trace": true }))
        .unwrap();
    client.call("Session.start", json!({})).unwrap();
    client.call("Session.start", json!({})).unwrap();
    rig.endpoint.wait_for_start().unwrap();
    assert_eq!(*rig.handler.log.lock().unwrap(), ["setup", "start"]);
}

/// A handler whose setup waits for a signal.
struct Slow {
    entered: Mutex<std::sync::mpsc::Sender<()>>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl Handler for Slow {
    fn setup(&self, _: SessionSetup) -> Result<(), ErrorBody> {
        let _ = self.entered.lock().unwrap().send(());
        let _ = self
            .release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5));
        Ok(())
    }
    fn start(&self) {}
    fn step(&self, _: TestStep, done: Box<dyn FnOnce(Result<Value, String>) + Send>) {
        done(Ok(json!({ "ok": true })));
    }
    fn fetch_reply(&self, _: u64, _: FetchReply) {}
}

#[test]
fn not_ready_until_setup_returns() {
    let (entered_tx, entered) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let handler = Arc::new(Slow {
        entered: Mutex::new(entered_tx),
        release: Mutex::new(release_rx),
    });
    let mut cfg = config();
    cfg.wait = true;
    cfg.wait_timeout = Duration::from_millis(150);
    let endpoint = Endpoint::start(cfg, handler, Arc::new(Collector::default())).unwrap();
    let url = format!("ws://{}{PATH}", endpoint.addr());
    let starter = Client::connect(&url, "secret", T).unwrap();
    let other = Client::connect(&url, "secret", T).unwrap();
    let start = std::thread::spawn(move || starter.call("Session.start", json!({})));
    entered.recv_timeout(T).unwrap();
    assert!(endpoint.wait_for_start().is_err());
    assert_eq!(
        press(&other).unwrap_err().code,
        nsh_devtools::protocol::ERR_NOT_STARTED
    );
    release.send(()).unwrap();
    start.join().unwrap().unwrap();
    endpoint.wait_for_start().unwrap();
    press(&other).unwrap();
}

#[test]
fn missing_and_truncated_tokens_are_refused() {
    let rig = rig(config());
    assert!(matches!(
        Client::connect(&rig.url, "", T),
        Err(ClientError::Unauthorized)
    ));
    assert!(matches!(
        Client::connect(&rig.url, "secre", T),
        Err(ClientError::Unauthorized)
    ));
    // No token parameter at all.
    let bare = tungstenite::connect(rig.url.as_str());
    assert!(matches!(
        bare,
        Err(tungstenite::Error::Http(r)) if r.status() == 401
    ));
}

#[test]
fn start_without_setup_runs_an_empty_setup() {
    #[derive(Default)]
    struct Rec(Mutex<Vec<String>>);
    impl Handler for Rec {
        fn setup(&self, setup: SessionSetup) -> Result<(), ErrorBody> {
            self.0
                .lock()
                .unwrap()
                .push(format!("setup {}", setup == SessionSetup::default()));
            Ok(())
        }
        fn start(&self) {
            self.0.lock().unwrap().push("start".into());
        }
        fn step(&self, _: TestStep, _: Box<dyn FnOnce(Result<Value, String>) + Send>) {}
        fn fetch_reply(&self, _: u64, _: FetchReply) {}
    }
    let handler = Arc::new(Rec::default());
    let endpoint =
        Endpoint::start(config(), handler.clone(), Arc::new(Collector::default())).unwrap();
    let url = format!("ws://{}{PATH}", endpoint.addr());
    Client::connect(&url, "secret", T)
        .unwrap()
        .call("Session.start", json!({}))
        .unwrap();
    assert_eq!(*handler.0.lock().unwrap(), ["setup true", "start"]);
}

#[test]
fn port_file_holds_the_address() {
    let dir = std::env::temp_dir().join(format!("nsh-devtools-port-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("port");
    let mut cfg = config();
    cfg.port_file = Some(file.clone());
    let rig = rig(cfg);
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text, format!("{}\n", rig.url));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn console_query_and_subscribe() {
    let rig = rig(config());
    let emit = |message: &str| {
        rig.console.record(
            &log::Record::builder()
                .level(log::Level::Warn)
                .target("js")
                .args(format_args!("{message}"))
                .build(),
        );
    };
    emit("before");
    let client = connect(&rig);
    let events = client.events();
    let got = client
        .call("Console.query", json!({ "level": "warn" }))
        .unwrap();
    assert_eq!(got["entries"][0]["message"], "before");
    client.call("Console.subscribe", json!({})).unwrap();
    emit("after");
    let event = events.recv_timeout(T).unwrap();
    assert_eq!(event.method, "Console.entry");
    assert_eq!(event.params["message"], "after");
}
