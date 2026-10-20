//! `nsh/keymap`: the table the platform gets, the handlers its key
//! commands run, unbinding, bindings undone with their component, and the
//! errors of invalid and reserved keystrokes.

use std::{sync::Arc, time::Duration};

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, EventKind, EventPayload, MountBatch, MountItem, PayloadValue, ROOT_ID, Size,
    keymap::{KeyMatch, Keymap, Keystroke},
    platform::Os,
};
use nsh_engine::{Bundle, Engine, EngineConfig, Environment};

use crate::ChannelSink;

fn start(app: &str) -> (Engine, Receiver<MountBatch>) {
    static APPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "nsh-keymap-{}-{}",
        std::process::id(),
        APPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, app).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: Vec::new(),
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &[]).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        // SAFETY: compiled just now by this runtime.
        app: nsh_engine::App::Js(unsafe { Bundle::bytecode(bytecode) }),
        viewport: Size::new(800.0, 600.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment {
            os: Os::MacOs,
            ..Environment::default()
        },
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        data_dir: None,
        storage: None,
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// What happened since the last call: the texts shown and the newest table.
#[derive(Default)]
struct Seen {
    texts: Vec<String>,
    keymap: Option<Arc<Keymap>>,
}

fn next(rx: &Receiver<MountBatch>, mut done: impl FnMut(&Seen) -> bool) -> Seen {
    let mut seen = Seen::default();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(500)) {
        for item in batch.items {
            match item {
                MountItem::Text { text, .. } => seen.texts.push(text.text.to_string()),
                MountItem::Keymap(keymap) => seen.keymap = Some(keymap),
                _ => {}
            }
        }
        if done(&seen) {
            break;
        }
    }
    seen
}

fn press(engine: &Engine, keymap: &Keymap, keystroke: &str, editing: bool) {
    let keystroke = Keystroke::parse(keystroke, Os::MacOs).unwrap();
    let KeyMatch::Fire(id) = keymap.find(keystroke, editing, false) else {
        panic!("{keystroke:?} is not bound");
    };
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::KeyCommand,
        EventPayload::Value(PayloadValue::Number(id as f64)),
    );
}

const APP: &str = r#"import { mount, signal, Text } from "nsh";
import { keymap } from "nsh/keymap";

const log = signal("");
const say = (s: string) => log.set(s);

const errors: string[] = [];
for (const k of ["cmd+kk", "mod+q"]) {
  try { keymap.bind(k, () => {}); } catch (e) { errors.push(String(e.message)); }
}
say(errors.join(" | "));

keymap.bind("mod+k", (k) => say(`k ${k}`));
keymap.bind({ "mod+s": (k) => say(`saved ${k}`), "escape": (k) => say(`esc ${k}`) }, { when: "always" });
keymap.bind("alt+down", () => {}, { repeat: true });
// Any case; the handler gets the keystroke as written.
keymap.bind("Mod+Shift+J", (k) => say(`written ${k}`));

function Panel() {
  keymap.bind("mod+p", () => say("panel"));
  return <Text>panel</Text>;
}

mount(() => (
  <>
    <Text>{`log ${log()}`}</Text>
    <Panel />
  </>
));
"#;

#[test]
fn key_commands_run_their_handlers() {
    let (engine, rx) = start(APP);
    let seen = next(&rx, |s| s.texts.iter().any(|t| t.starts_with("log ")));
    let errors = seen.texts.iter().find(|t| t.starts_with("log ")).unwrap();
    assert!(errors.contains("invalid keystroke \"cmd+kk\": unknown key \"kk\""));
    assert!(errors.contains("\"mod+q\" cannot be bound: it quits the app"));
    let keymap = seen.keymap.expect("a table");
    // mod+k, mod+s, escape (always), alt+down (repeats), Mod+Shift+J,
    // mod+p (the panel's).
    assert_eq!(keymap.bindings().len(), 6);
    let down = Keystroke::parse("alt+down", Os::MacOs).unwrap();
    assert!(keymap.find(down, false, true).consumed());
    assert!(!matches!(
        keymap.find(Keystroke::parse("cmd+k", Os::MacOs).unwrap(), false, true),
        KeyMatch::Fire(_)
    ));
    // Escape is bound `always`: it fires in text inputs too.
    let escape = Keystroke::parse("escape", Os::MacOs).unwrap();
    assert!(keymap.find(escape, true, false).consumed());

    press(&engine, &keymap, "cmd+k", false);
    assert!(
        next(&rx, |s| s.texts.contains(&"log k mod+k".into()))
            .texts
            .contains(&"log k mod+k".into())
    );
    press(&engine, &keymap, "cmd+s", true);
    assert!(
        next(&rx, |s| s.texts.contains(&"log saved mod+s".into()))
            .texts
            .contains(&"log saved mod+s".into())
    );
    press(&engine, &keymap, "cmd+shift+j", false);
    assert!(
        next(&rx, |s| s.texts.contains(&"log written Mod+Shift+J".into()))
            .texts
            .contains(&"log written Mod+Shift+J".into())
    );
}

const UNBIND: &str = r#"import { mount, signal, Text } from "nsh";
import { keymap } from "nsh/keymap";

const panel = signal(true);
const off = keymap.bind("mod+k", () => {});
function Panel() {
  keymap.bind("mod+p", () => {});
  return <Text>panel</Text>;
}
setTimeout(() => off(), 50);
setTimeout(() => off(), 60);
setTimeout(() => panel.set(false), 100);

mount(() => <If when={panel()}><Panel /></If>);
"#;

#[test]
fn bindings_go_when_unbound_and_with_their_component() {
    let (_engine, rx) = start(UNBIND);
    let first = next(&rx, |s| {
        s.keymap.as_ref().is_some_and(|k| k.bindings().len() == 2)
    });
    assert_eq!(first.keymap.unwrap().bindings().len(), 2);
    // `off()`: the component's binding is left.
    let after = next(&rx, |s| {
        s.keymap.as_ref().is_some_and(|k| k.bindings().len() == 1)
    });
    let left = after.keymap.unwrap();
    let p = Keystroke::parse("cmd+p", Os::MacOs).unwrap();
    assert_eq!(left.bindings()[0].keystroke, p);
    // The panel goes, and its binding with it.
    let gone = next(&rx, |s| s.keymap.as_ref().is_some_and(|k| k.is_empty()));
    assert!(gone.keymap.unwrap().is_empty());
}
