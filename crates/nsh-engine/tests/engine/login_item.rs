//! `Application.loginItem()` and `setLoginItem` of `nsh/desktop` from JS,
//! with a platform's login item: its statuses reach the app, the calls run
//! in the order the app makes them, a failure of the system rejects with its
//! code, and only a declaration grants the capability.

use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use nsh_core::{
    ApproxTextEngine, MountItem, Size,
    desktop::{LoginItemBackend, LoginItemStatus},
    geometry::px,
};
use nsh_engine::{Bundle, Capabilities, Engine, EngineConfig};

use crate::ChannelSink;

const APP: &str = r#"
import { mount, signal, Text } from "nsh";
import { Application } from "nsh/desktop";
function App() {
  const said = signal("");
  const say = (s) => said.set(`${said()} ${s}`);
  const status = (item) => say(item.status);
  const code = (e) => say(e.code ?? e.name);
  (async () => {
    // Made together, they run in the order the app made them.
    const calls = [
      Application.setLoginItem({ enabled: true }),
      Application.loginItem(),
      Application.setLoginItem({ enabled: false }),
    ];
    for (const call of calls) status(await call);
    // The system's failures, each with its code.
    await Application.setLoginItem({ enabled: true }).then(status, code);
    await Application.setLoginItem({ enabled: true }).then(status, code);
    await Application.loginItem().then(status, code);
    await Application.setLoginItem({ enabled: false }).then(status, code);
    // Options it does not take reject before the system is asked.
    await Application.setLoginItem({}).then(status, code);
    await Application.setLoginItem({ enabled: true, hidden: true }).then(status, code);
    say("done");
  })();
  return <Text>{said()}</Text>;
}
mount(App);
"#;

const UNDECLARED: &str = r#"
import { mount, signal, Text } from "nsh";
import { Application } from "nsh/desktop";
function App() {
  const said = signal("");
  const say = (e) => said.set(`${said()} ${e.code}/${e.permission}`);
  (async () => {
    await Application.loginItem().catch(say);
    await Application.setLoginItem({ enabled: true }).catch(say);
  })();
  return <Text>{said()}</Text>;
}
mount(App);
"#;

/// A platform's login item that answers from a script and records the
/// calls: a change (`Some`) or a read.
struct Scripted {
    answers: Mutex<VecDeque<io::Result<LoginItemStatus>>>,
    calls: Mutex<Vec<Option<bool>>>,
}

impl Scripted {
    fn new(answers: impl IntoIterator<Item = io::Result<LoginItemStatus>>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into_iter().collect()),
            calls: Mutex::default(),
        })
    }

    fn answer(&self, call: Option<bool>) -> io::Result<LoginItemStatus> {
        self.calls.lock().unwrap().push(call);
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(io::Error::other("no answer left")))
    }
}

impl LoginItemBackend for Scripted {
    fn status(&self) -> io::Result<LoginItemStatus> {
        self.answer(None)
    }

    fn set(&self, enabled: bool) -> io::Result<LoginItemStatus> {
        self.answer(Some(enabled))
    }
}

fn bundle(app: &str, name: &str) -> Bundle {
    let dir = crate::temporary_root().join(format!("nsh-login-item-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, app).unwrap();
    let ui_sets = [nsh_base::registry()];
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: ui_sets.iter().map(Into::into).collect(),
        ..Default::default()
    })
    .unwrap();
    // The bundle is in memory now.
    let _ = std::fs::remove_dir_all(&dir);
    let bytecode = nsh_js::compile_bytecode_with(
        &bundle.code,
        false,
        &ui_sets.iter().map(Into::into).collect::<Vec<_>>(),
    )
    .unwrap();
    // SAFETY: compiled just now by this runtime.
    unsafe { Bundle::bytecode(bytecode) }
}

/// Runs `bundle` with the platform's login item `backend` until its text
/// contains `end`, and gives the text.
fn shown(bundle: Bundle, backend: Arc<Scripted>, end: &str) -> String {
    let config = EngineConfig {
        viewport: Size::new(px(400.0), px(600.0)),
        ui_sets: vec![nsh_base::registry()],
        login_item: Some(backend),
        ..EngineConfig::new(bundle)
    };
    let (tx, rx) = crossbeam_channel::unbounded();
    let _engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let mut text = String::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let Ok(batch) = rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        for item in batch.items {
            if let MountItem::Text { text: t, .. } = item {
                text = t.text.to_string();
            }
        }
        if text.contains(end) {
            return text;
        }
    }
    panic!("no text with {end}; last: {text}");
}

#[test]
fn a_declared_app_reads_and_changes_its_login_item() {
    let backend = Scripted::new([
        Ok(LoginItemStatus::Enabled),
        Ok(LoginItemStatus::RequiresApproval),
        Ok(LoginItemStatus::Disabled),
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "not permitted",
        )),
        Err(io::Error::new(io::ErrorKind::InvalidFilename, "too long")),
        Err(io::Error::new(io::ErrorKind::InvalidInput, "no such id")),
        Err(io::Error::other("failed")),
    ]);
    let mut app = bundle(APP, "declared");
    app.policy.declared =
        Some(Capabilities::from_json(&serde_json::json!({ "loginItem": true })).unwrap());
    let text = shown(app, backend.clone(), "done");
    assert_eq!(
        text.trim(),
        "enabled requiresApproval disabled EACCES ENAMETOOLONG EINVAL EIO TypeError TypeError done"
    );
    assert_eq!(
        *backend.calls.lock().unwrap(),
        [
            Some(true),
            None,
            Some(false),
            Some(true),
            Some(true),
            None,
            Some(false)
        ]
    );
}

#[test]
fn an_app_without_the_declaration_never_asks_the_system() {
    let backend = Scripted::new([Ok(LoginItemStatus::Enabled)]);
    // No `"capabilities"`: everything else is granted, the login item is not.
    let text = shown(
        bundle(UNDECLARED, "undeclared"),
        backend.clone(),
        "ERR_ACCESS_DENIED/loginItem ERR_ACCESS_DENIED/loginItem",
    );
    assert_eq!(
        text.trim(),
        "ERR_ACCESS_DENIED/loginItem ERR_ACCESS_DENIED/loginItem"
    );
    assert!(backend.calls.lock().unwrap().is_empty());
}
