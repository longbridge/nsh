//! What the gallery's headless tests share: an engine running the
//! gallery, what it shows, presses.

#![allow(dead_code, unreachable_pub)]

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, AttributedText, Command, EventKind, EventMask, EventPayload, GestureValues,
    MountBatch, MountItem, ROOT_ID, Size, TouchEvent, TouchPhase, TouchPoint, canvas::Drawing,
    ui::Root,
};
use nsh_engine::{App, Engine, EngineConfig, EngineControl, Environment, MountSink};

/// Takes the engine's batches, and gives it display frames as a platform
/// does (60 Hz while it asks for them), so what waits for a frame (a
/// stream drawn per frame, `request_animation_frame`) runs as it does on
/// screen.
pub struct Sink {
    batches: crossbeam_channel::Sender<MountBatch>,
    /// Set once the engine runs.
    control: Arc<OnceLock<EngineControl>>,
}

impl Sink {
    pub fn new(batches: crossbeam_channel::Sender<MountBatch>) -> Self {
        Self {
            batches,
            control: Arc::default(),
        }
    }

    /// Starts `config`'s app with this sink driving its frames.
    pub fn start(config: EngineConfig, batches: crossbeam_channel::Sender<MountBatch>) -> Engine {
        let sink = Sink::new(batches);
        let control = sink.control.clone();
        let engine = Engine::start(config, || ApproxTextEngine, sink).unwrap();
        let _ = control.set(engine.control());
        engine
    }
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

/// What is on screen: texts by node and screens' titles.
#[derive(Default)]
pub struct Screen {
    pub texts: Vec<(u32, String)>,
    /// The text blocks as built: spans, links, attachments.
    pub blocks: Vec<(u32, Arc<AttributedText>)>,
    pub titles: Vec<(u32, String)>,
    pub parents: std::collections::HashMap<u32, u32>,
    /// The native events each view listens to.
    pub events: std::collections::HashMap<u32, EventMask>,
    /// Each canvas's drawing.
    pub drawings: std::collections::HashMap<u32, Arc<Drawing>>,
    /// The native values gestures drive, by view.
    pub gestures: std::collections::HashMap<u32, GestureValues>,
    /// The commands sent, in order.
    pub commands: Vec<(u32, Command)>,
    /// Scroll views' and lists' settings, as last sent.
    pub scrolls: std::collections::HashMap<u32, MountItem>,
    pub behaviors: std::collections::HashMap<u32, nsh_core::ScrollBehavior>,
    /// Lists with sticky cells.
    pub sticky: std::collections::HashSet<u32>,
    /// Accessibility labels, by node.
    pub labels: std::collections::HashMap<u32, String>,
}

impl Screen {
    pub fn apply(&mut self, batch: &MountBatch) {
        for item in &batch.items {
            match item {
                MountItem::Text { id, text, .. } => {
                    self.texts.retain(|t| t.0 != *id);
                    self.texts.push((*id, text.text.clone()));
                    self.blocks.retain(|t| t.0 != *id);
                    self.blocks.push((*id, text.clone()));
                }
                MountItem::ScreenConfig { id, config } => {
                    self.titles.retain(|t| t.0 != *id);
                    if let Some(title) = &config.title {
                        self.titles.push((*id, title.to_string()));
                    }
                }
                MountItem::Insert { parent, child, .. } => {
                    self.parents.insert(*child, *parent);
                }
                MountItem::Events { id, events } => {
                    self.events.insert(*id, *events);
                }
                MountItem::Canvas { id, drawing } => {
                    self.drawings.insert(*id, drawing.clone());
                }
                MountItem::Gestures { id, values } => {
                    self.gestures.insert(*id, *values);
                }
                MountItem::Command { id, command } => {
                    self.commands.push((*id, command.clone()));
                }
                MountItem::ScrollConfig { id, .. } => {
                    self.scrolls.insert(*id, item.clone());
                }
                MountItem::ScrollBehavior { id, behavior } => {
                    self.behaviors.insert(*id, (**behavior).clone());
                }
                MountItem::StickyCells { id, cells } if !cells.is_empty() => {
                    self.sticky.insert(*id);
                }
                MountItem::ViewProps { id, props } => match &props.accessibility.label {
                    Some(label) => {
                        self.labels.insert(*id, label.to_string());
                    }
                    None => {
                        self.labels.remove(id);
                    }
                },
                MountItem::Delete { id } => {
                    self.labels.remove(id);
                    self.texts.retain(|t| t.0 != *id);
                    self.blocks.retain(|t| t.0 != *id);
                    self.titles.retain(|t| t.0 != *id);
                }
                _ => {}
            }
        }
    }

    /// The views listening to touches.
    pub fn touch_listeners(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .events
            .iter()
            .filter(|(_, e)| e.contains(EventKind::Touch))
            .map(|(id, _)| *id)
            .collect();
        ids.sort();
        ids
    }

    /// Whether a view with accessibility label `label` exists.
    pub fn labelled(&self, label: &str) -> bool {
        self.labels.values().any(|l| l == label)
    }

    /// The pressable node around text `label` (the text or a parent).
    pub fn find(&self, label: &str) -> Option<u32> {
        self.texts.iter().find(|t| t.1 == label).map(|t| t.0)
    }

    /// What a press on the text `text` goes to: its nearest ancestor
    /// listening to presses (a row, a button, the gallery's `Press` around
    /// them), else its parent.
    pub fn press_target(&self, text: u32) -> u32 {
        let parent = self.parents[&text];
        let mut at = parent;
        loop {
            if self
                .events
                .get(&at)
                .is_some_and(|e| e.contains(EventKind::Press))
            {
                return at;
            }
            match self.parents.get(&at) {
                Some(&up) => at = up,
                None => return parent,
            }
        }
    }

    /// The scroll view or list `id` is in (its nearest one).
    pub fn scroll_of(&self, mut id: u32) -> Option<u32> {
        loop {
            id = *self.parents.get(&id)?;
            if self.scrolls.contains_key(&id) || self.behaviors.contains_key(&id) {
                return Some(id);
            }
        }
    }

    /// The text block showing `text`.
    pub fn block(&self, text: &str) -> Option<&AttributedText> {
        self.blocks.iter().find(|b| b.1.text == text).map(|b| &*b.1)
    }

    /// The span node of the link `label` in a text block.
    pub fn link(&self, label: &str) -> Option<u32> {
        self.blocks.iter().find_map(|(_, block)| {
            let units: Vec<u16> = block.text.encode_utf16().collect();
            block.links.iter().find_map(|l| {
                let range = units.get(l.start as usize..l.end as usize)?;
                (String::from_utf16_lossy(range) == label).then_some(l.node)
            })
        })
    }
}

/// Applies batches until none comes for 300 ms, or for 2 s at most (a
/// live feed never goes quiet).
pub fn drain(rx: &Receiver<MountBatch>, screen: &mut Screen) {
    let end = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < end
        && let Ok(batch) = rx.recv_timeout(Duration::from_millis(300))
    {
        screen.apply(&batch);
    }
}

pub fn logger() {
    struct L;
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata<'_>) -> bool {
            m.level() <= log::Level::Warn
        }
        fn log(&self, r: &log::Record<'_>) {
            if self.enabled(r.metadata()) {
                eprintln!("{} {}: {}", r.level(), r.target(), r.args());
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: L = L;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Warn);
}

/// A headless engine running the gallery on a phone, and its batches.
pub fn start() -> (nsh_engine::Engine, Receiver<MountBatch>) {
    start_at(Size::new(390.0, 844.0))
}

/// A headless engine running the gallery in a window of `viewport`, and
/// its batches.
pub fn start_at(viewport: Size) -> (nsh_engine::Engine, Receiver<MountBatch>) {
    logger();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: App::Rust(Root::of::<gallery_native::Gallery>()),
        viewport,
        trace: false,
        network: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        storage: None,
    };
    let engine = Sink::start(config, tx);
    (engine, rx)
}

/// Presses what the text `label` is in (a row or a button: its nearest
/// ancestor listening to presses, else its parent), and waits for what it
/// changes.
pub fn press(engine: &Engine, rx: &Receiver<MountBatch>, screen: &mut Screen, label: &str) {
    let text = screen
        .find(label)
        .unwrap_or_else(|| panic!("no {label}: {:?}", screen.texts));
    let target = screen.press_target(text);
    engine
        .control()
        .dispatch_event(target, EventKind::Press, EventPayload::None);
    drain(rx, screen);
}

/// The gallery, with demo `name` opened from the index.
pub fn open(name: &str) -> (Engine, Receiver<MountBatch>, Screen) {
    let (engine, rx) = start();
    let mut screen = Screen::default();
    drain(&rx, &mut screen);
    press(&engine, &rx, &mut screen, name);
    assert!(
        screen.titles.iter().any(|t| t.1 == name),
        "{name} opens: {:?}",
        screen.titles
    );
    (engine, rx, screen)
}

/// Whether text `text` is on screen.
pub fn shows(screen: &Screen, text: &str) -> bool {
    screen.texts.iter().any(|t| t.1 == text)
}

/// Presses the text `label` itself (a pressable text: a link, a chip, a
/// tab), and waits for what it changes.
pub fn press_text(engine: &Engine, rx: &Receiver<MountBatch>, screen: &mut Screen, label: &str) {
    let text = screen
        .find(label)
        .unwrap_or_else(|| panic!("no {label}: {:?}", screen.texts));
    engine
        .control()
        .dispatch_event(text, EventKind::Press, EventPayload::None);
    drain(rx, screen);
}

/// Sends `kind` to the link `label` inside a text block (its span), and
/// waits for what it changes.
pub fn press_link(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut Screen,
    label: &str,
    kind: EventKind,
) {
    let link = screen
        .link(label)
        .unwrap_or_else(|| panic!("no link {label}: {:?}", screen.texts));
    engine
        .control()
        .dispatch_event(link, kind, EventPayload::None);
    drain(rx, screen);
}

/// One finger at `(x, y)` in the view it went down on (`path[0]`; the
/// surface position is 100 pt further right and down), reported on the
/// root as the platform does, with the listening views around it
/// (`path`, innermost first); waits for what it changes.
pub fn touch(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut Screen,
    phase: TouchPhase,
    path: &[u32],
    (x, y): (f32, f32),
    timestamp: f64,
) {
    let event = TouchEvent {
        phase,
        timestamp,
        activated: None,
        points: vec![TouchPoint {
            id: 1,
            target: path[0],
            path: if phase == TouchPhase::Start {
                path.to_vec()
            } else {
                Vec::new()
            },
            page_x: x + 100.0,
            page_y: y + 100.0,
            location_x: x,
            location_y: y,
            changed: true,
        }],
    };
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::Touch,
        EventPayload::Touch(Box::new(event)),
    );
    drain(rx, screen);
}
