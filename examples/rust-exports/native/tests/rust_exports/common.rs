//! What the tests share: `App.tsx` compiled against the crate's set and
//! run on a headless engine, what it shows, presses.

use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{
    ApproxTextEngine, Command, EventKind, EventMask, EventPayload, MountBatch, MountItem, Size,
    ui::Registry,
};
use nsh_engine::{App, Bundle, Engine, EngineConfig, EngineControl, Environment, MountSink};

/// The sets the app's hosts serve: the mobile UI, then the crate's
/// (`nsh.json` `"rust"`), as `nsh` passes them.
fn ui_sets() -> Vec<Registry> {
    vec![
        nsh_ui_mobile::registry(),
        rust_exports_native::__nsh_native_module(),
    ]
}

/// `App.tsx` as bytecode, compiled once for every test.
fn bytecode() -> Vec<u8> {
    static BYTECODE: OnceLock<Vec<u8>> = OnceLock::new();
    BYTECODE
        .get_or_init(|| {
            let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../App.tsx");
            let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
                entry,
                minify: false,
                split: false,
                ui_sets: ui_sets(),
            })
            .expect("App.tsx bundles");
            nsh_js::compile_bytecode_with(&bundle.code, false, &ui_sets())
                .expect("App.tsx compiles")
        })
        .clone()
}

/// Takes the engine's batches, and gives it display frames as a platform
/// does while it asks for them.
struct Sink {
    batches: Sender<MountBatch>,
    control: Arc<OnceLock<EngineControl>>,
}

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.batches.send(batch);
    }

    fn request_frame(&self) {
        let control = self.control.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_micros(16_667));
            if let Some(control) = control.get() {
                control.frame();
            }
        });
    }
}

/// The app on a phone-sized headless engine, and what it shows.
pub(crate) struct Running {
    engine: Engine,
    batches: Receiver<MountBatch>,
    /// The texts on screen, by node.
    texts: Vec<(u32, String)>,
    /// Each node's parent.
    parents: std::collections::HashMap<u32, u32>,
    /// The native events each view listens to.
    events: std::collections::HashMap<u32, EventMask>,
    /// The commands sent, in order.
    pub(crate) commands: Vec<Command>,
}

impl Running {
    /// Starts the app and waits until it shows `ready`.
    pub(crate) fn start(ready: &str) -> Running {
        let (tx, batches) = crossbeam_channel::unbounded();
        let control = Arc::new(OnceLock::new());
        let config = EngineConfig {
            // SAFETY: compiled by this runtime just now.
            app: App::Js(unsafe { Bundle::bytecode(bytecode()) }),
            viewport: Size::new(390.0, 844.0),
            trace: false,
            network: None,
            assets: None,
            environment: Environment::default(),
            header_heights: (44.0, 96.0),
            limits: nsh_engine::Limits::default(),
            ui_sets: ui_sets(),
            runtime: None,
            data_dir: None,
            storage: None,
        };
        let sink = Sink {
            batches: tx,
            control: control.clone(),
        };
        let engine = Engine::start(config, || ApproxTextEngine, sink).expect("the engine starts");
        let _ = control.set(engine.control());
        let mut app = Running {
            engine,
            batches,
            texts: Vec::new(),
            parents: Default::default(),
            events: Default::default(),
            commands: Vec::new(),
        };
        app.wait_for(ready);
        app
    }

    fn apply(&mut self, batch: &MountBatch) {
        for item in &batch.items {
            match item {
                MountItem::Text { id, text, .. } => {
                    self.texts.retain(|t| t.0 != *id);
                    self.texts.push((*id, text.text.clone()));
                }
                MountItem::Insert { parent, child, .. } => {
                    self.parents.insert(*child, *parent);
                }
                MountItem::Events { id, events } => {
                    self.events.insert(*id, *events);
                }
                MountItem::Command { command, .. } => self.commands.push(command.clone()),
                MountItem::Delete { id } => self.texts.retain(|t| t.0 != *id),
                _ => {}
            }
        }
    }

    /// Whether `text` is on screen.
    pub(crate) fn shows(&self, text: &str) -> bool {
        self.texts.iter().any(|t| t.1 == text)
    }

    /// Applies batches until `text` is on screen (5 s at most).
    pub(crate) fn wait_for(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.shows(text) {
            match self.batches.recv_deadline(deadline) {
                Ok(batch) => self.apply(&batch),
                Err(_) => panic!("{text:?} never shows: {:?}", self.texts),
            }
        }
    }

    /// Presses what the text `label` is in (its nearest ancestor listening
    /// to presses: a button, the gauge), as a tap does, and waits until
    /// `then` shows.
    pub(crate) fn press(&mut self, label: &str, then: &str) {
        let text = self
            .texts
            .iter()
            .find(|t| t.1 == label)
            .unwrap_or_else(|| panic!("no {label:?}: {:?}", self.texts))
            .0;
        let mut at = text;
        while !self
            .events
            .get(&at)
            .is_some_and(|e| e.contains(EventKind::Press))
        {
            at = *self
                .parents
                .get(&at)
                .unwrap_or_else(|| panic!("nothing around {label:?} hears presses"));
        }
        self.engine
            .control()
            .dispatch_event(at, EventKind::Press, EventPayload::None);
        self.wait_for(then);
    }
}
