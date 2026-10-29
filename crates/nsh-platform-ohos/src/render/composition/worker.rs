//! One compositor worker per surface; exports and replies are generation scoped.
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use nsh_core::{Filter, NodeId};
use ohos_drawing_binding::{Bitmap, BitmapFormat, Canvas};

use super::readback::{TextureContext, TextureReader};
use crate::{
    app::runtime::{Dispatcher, Message},
    render::{
        composition::filter::Image,
        node::{Node, Result},
    },
};

const MAX_LAYERS: usize = 128;
const MAX_SURFACE_PIXELS: u64 = 67_108_864;

#[derive(Clone, PartialEq)]
struct Settings {
    width: u32,
    height: u32,
    filters: Vec<Filter>,
    scale: f32,
    insets: [u32; 4],
    scene: bool,
}
impl Settings {
    fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}
struct Output {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    x: f32,
    y: f32,
}
struct State {
    settings: Settings,
    output: Option<Output>,
    surface: Option<std::result::Result<u64, String>>,
    closed: bool,
}
struct Shared {
    state: Mutex<State>,
    queued: AtomicBool,
    posted: AtomicBool,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Key {
    pub(crate) id: NodeId,
    pub(crate) generation: u32,
    overlay: bool,
}
impl Key {
    fn slot(self) -> usize {
        self.id as usize * 2 + usize::from(self.overlay)
    }
}
pub(crate) struct ScenePixels {
    pub(crate) id: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}
#[derive(Default)]
pub(crate) struct SceneShared {
    pub(crate) output: Mutex<Option<Vec<ScenePixels>>>,
    pub(crate) posted: AtomicBool,
}
type ScenePlan = (
    Arc<crate::render::composition::draw::Plan>,
    Arc<SceneShared>,
);
enum Task {
    Create { key: Key, shared: Arc<Shared> },
    Wake(Key),
    Plan,
    Close,
}
pub(crate) struct Worker {
    send: mpsc::Sender<Task>,
    plan: Arc<Mutex<Option<ScenePlan>>>,
}
impl Worker {
    pub(crate) fn new(route: Weak<Dispatcher>) -> std::io::Result<Self> {
        let (send, receive) = mpsc::channel();
        let callback = send.clone();
        let plan = Arc::new(Mutex::new(None));
        let pending_plan = plan.clone();
        std::thread::Builder::new()
            .name("nsh-compositor".into())
            .spawn(move || {
                let mut entries: Vec<Option<Entry>> = Vec::new();
                let mut gpu: Option<TextureContext> = None;
                let mut scene: Option<ScenePlan> = None;
                while let Ok(task) = receive.recv() {
                    let mut repaint = false;
                    match task {
                        Task::Close => break,
                        Task::Plan => {
                            scene = pending_plan.lock().unwrap().take();
                            repaint = scene.is_some();
                        }
                        Task::Create { key, shared } => {
                            let result = (|| {
                                if shared.state.lock().unwrap().closed {
                                    return Err("composite retired".into());
                                }
                                if entries.iter().flatten().count() >= MAX_LAYERS {
                                    return Err("compositor layer budget exceeded".into());
                                }
                                let settings = shared.state.lock().unwrap().settings.clone();
                                check_pixels(&entries, key, settings.pixels())?;
                                let callback = callback.clone();
                                let weak = Arc::downgrade(&shared);
                                if gpu.is_none() {
                                    gpu = Some(
                                        TextureContext::new().map_err(|error| error.to_string())?,
                                    );
                                }
                                let mut reader =
                                    TextureReader::with_context(gpu.as_ref().unwrap(), move || {
                                        if let Some(shared) = weak.upgrade() {
                                            wake(&callback, key, &shared);
                                        }
                                    })
                                    .map_err(|error| error.to_string())?;
                                reader
                                    .resize(settings.width, settings.height)
                                    .map_err(|error| error.to_string())?;
                                let id = reader.id().map_err(|error| error.to_string())?;
                                entries.resize_with(entries.len().max(key.slot() + 1), || None);
                                entries[key.slot()] = Some(Entry {
                                    key,
                                    shared: shared.clone(),
                                    reader,
                                    settings,
                                    source: None,
                                });
                                Ok(id)
                            })();
                            shared.state.lock().unwrap().surface = Some(result);
                            if let Some(route) = route.upgrade() {
                                route.send(Message::Composite {
                                    id: key.id,
                                    generation: key.generation,
                                });
                            }
                        }
                        Task::Wake(key) => {
                            let Some(entry) = entries
                                .get(key.slot())
                                .and_then(Option::as_ref)
                                .filter(|entry| entry.key == key)
                            else {
                                continue;
                            };
                            entry.shared.queued.store(false, Ordering::Release);
                            if entry.shared.state.lock().unwrap().closed {
                                entries[key.slot()].take();
                                continue;
                            }
                            let settings = entry.shared.state.lock().unwrap().settings.clone();
                            let result =
                                check_pixels(&entries, key, settings.pixels()).and_then(|()| {
                                    entries[key.slot()].as_mut().unwrap().render(settings)
                                });
                            let Some(route) = route.upgrade() else {
                                break;
                            };
                            match result {
                                Ok(changed) => repaint = changed,
                                Err(error) => {
                                    route.send(Message::Error(format!("compositor: {error}")));
                                    continue;
                                }
                            }
                            let shared = &entries[key.slot()].as_ref().unwrap().shared;
                            let publish = {
                                let state = shared.state.lock().unwrap();
                                !state.settings.scene && state.output.is_some()
                            };
                            if publish && !shared.posted.swap(true, Ordering::AcqRel) {
                                route.send(Message::Composite {
                                    id: key.id,
                                    generation: key.generation,
                                });
                            }
                        }
                    }
                    if repaint && let Some((plan, shared)) = &scene {
                        let result = plan.render(|key| {
                            entries
                                .get(key.slot())
                                .and_then(Option::as_ref)
                                .filter(|entry| entry.key == key)
                                .and_then(|entry| entry.source.as_ref())
                        });
                        let Some(route) = route.upgrade() else {
                            break;
                        };
                        match result {
                            Ok(Some(frames)) => {
                                *shared.output.lock().unwrap() = Some(
                                    frames
                                        .into_iter()
                                        .map(|frame| ScenePixels {
                                            id: frame.id,
                                            width: frame.image.bitmap.width(),
                                            height: frame.image.bitmap.height(),
                                            pixels: frame.image.bitmap.pixels().to_vec(),
                                        })
                                        .collect(),
                                );
                                if !shared.posted.swap(true, Ordering::AcqRel) {
                                    route.send(Message::Scene);
                                }
                            }
                            Ok(None) => {}
                            Err(error) => {
                                route.send(Message::Error(format!("scene compositor: {error}")))
                            }
                        }
                    }
                }
            })?;
        Ok(Self { send, plan })
    }
    pub(crate) fn clear_plan(&self) {
        self.plan.lock().unwrap().take();
        let _ = self.send.send(Task::Plan);
    }
    pub(crate) fn plan(
        &self,
        plan: Arc<crate::render::composition::draw::Plan>,
        shared: Arc<SceneShared>,
    ) {
        let mut pending = self.plan.lock().unwrap();
        let wake = pending.is_none();
        *pending = Some((plan, shared));
        if wake {
            let _ = self.send.send(Task::Plan);
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.send.send(Task::Close);
    }
}
fn wake(send: &mpsc::Sender<Task>, key: Key, shared: &Shared) {
    if !shared.queued.swap(true, Ordering::AcqRel) {
        let _ = send.send(Task::Wake(key));
    }
}
fn check_pixels(
    entries: &[Option<Entry>],
    key: Key,
    pixels: u64,
) -> std::result::Result<(), String> {
    if pixels > 16_777_216
        || entries
            .iter()
            .flatten()
            .filter(|entry| entry.key.slot() != key.slot())
            .map(|entry| entry.settings.pixels())
            .sum::<u64>()
            + pixels
            > MAX_SURFACE_PIXELS
    {
        Err("compositor pixel budget exceeded".into())
    } else {
        Ok(())
    }
}
struct Entry {
    key: Key,
    shared: Arc<Shared>,
    reader: TextureReader,
    settings: Settings,
    source: Option<Image>,
}
impl Entry {
    fn render(&mut self, settings: Settings) -> std::result::Result<bool, String> {
        let mut changed = self.settings != settings;
        if (settings.width, settings.height) != (self.settings.width, self.settings.height) {
            self.reader
                .resize(settings.width, settings.height)
                .map_err(|error| error.to_string())?;
        }
        self.settings = settings;
        // Bounded drains let other layers progress during a producer animation.
        for _ in 0..4 {
            let Some(frame) = self.reader.read().map_err(|error| error.to_string())? else {
                break;
            };
            let x = -(self.settings.insets[0] as f32);
            let y = -(self.settings.insets[1] as f32);
            if self.source.as_ref().is_some_and(|source| {
                source.x == x
                    && source.y == y
                    && source.bitmap.width() == frame.width
                    && source.bitmap.height() == frame.height
                    && source.bitmap.pixels() == frame.pixels
            }) {
                continue;
            }
            let mut image = Image::new(frame);
            image.x = x;
            image.y = y;
            self.source = Some(image);
            changed = true;
        }
        if !changed {
            return Ok(false);
        }
        let Some(source) = &self.source else {
            return Ok(false);
        };
        if self.settings.scene {
            return Ok(true);
        }
        let image = source.filtered(&self.settings.filters, self.settings.scale)?;
        self.shared.state.lock().unwrap().output = Some(Output {
            width: image.bitmap.width(),
            height: image.bitmap.height(),
            pixels: image.bitmap.pixels().to_vec(),
            x: image.x,
            y: image.y,
        });
        Ok(true)
    }
}
pub(crate) struct Composite {
    surface: RefCell<Option<String>>,
    pub(crate) generation: u32,
    key: Key,
    shared: Arc<Shared>,
    worker: Rc<Worker>,
    bitmap: RefCell<Option<(Bitmap, f32, f32)>>,
}
impl Composite {
    pub(crate) fn key(&self) -> Key {
        self.key
    }
    pub(crate) fn new(owner: &Rc<Node>, generation: u32) -> Result<Self> {
        Self::create(owner, generation, false)
    }
    pub(crate) fn overlay(owner: &Rc<Node>, generation: u32) -> Result<Self> {
        Self::create(owner, generation, true)
    }
    fn create(owner: &Rc<Node>, generation: u32, overlay: bool) -> Result<Self> {
        let worker = {
            let mut slot = owner.compositor.borrow_mut();
            if slot.is_none() {
                *slot = Some(Rc::new(Worker::new(owner.dispatcher.clone()).map_err(
                    |_| napi_ohos::Error::from_reason("Could not start OHOS compositor"),
                )?));
            }
            slot.as_ref().unwrap().clone()
        };
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                settings: settings(owner, overlay)?,
                output: None,
                surface: None,
                closed: false,
            }),
            queued: AtomicBool::new(false),
            posted: AtomicBool::new(false),
        });
        let key = Key {
            id: owner.id,
            generation,
            overlay,
        };
        worker
            .send
            .send(Task::Create {
                key,
                shared: shared.clone(),
            })
            .map_err(|_| napi_ohos::Error::from_reason("OHOS compositor stopped"))?;
        // GPU/EGL initialization completes on the compositor lane; its ready
        // event installs the texture producer in a later UI turn.
        if !overlay {
            owner
                .composite_nodes
                .borrow_mut()
                .push(Rc::downgrade(owner));
        }
        Ok(Self {
            surface: RefCell::default(),
            generation,
            key,
            shared,
            worker,
            bitmap: RefCell::default(),
        })
    }
    pub(crate) fn update(&self, owner: &Node) -> Result<()> {
        let settings = settings(owner, self.key.overlay)?;
        let mut state = self.shared.state.lock().unwrap();
        if state.settings != settings {
            state.settings = settings;
            drop(state);
            wake(&self.worker.send, self.key, &self.shared);
            owner
                .visual_node()
                .borrow()
                .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedMeasure)?;
        }
        Ok(())
    }
    pub(crate) fn insets(&self) -> crate::interop::sdk::TextureInsets {
        let state = self.shared.state.lock().unwrap();
        let [left, top, right, bottom] = state
            .settings
            .insets
            .map(|v| v as f64 / state.settings.scale as f64);
        crate::interop::sdk::TextureInsets {
            left,
            top,
            right,
            bottom,
        }
    }
    pub(crate) fn origin(&self) -> (f32, f32) {
        let insets = self.insets();
        (-(insets.left as f32), -(insets.top as f32))
    }
    pub(crate) fn surface(&self) -> Option<String> {
        self.surface.borrow().clone()
    }
    pub(crate) fn ready(&self) {
        let surface = self.shared.state.lock().unwrap().surface.take();
        if let Some(result) = surface {
            match result {
                Ok(id) => *self.surface.borrow_mut() = Some(id.to_string()),
                Err(error) => log::warn!("OHOS compositor initialization: {error}"),
            }
        }
        self.shared.posted.store(false, Ordering::Release);
        if let Some(output) = self.shared.state.lock().unwrap().output.take() {
            let mut slot = self.bitmap.borrow_mut();
            if let Some((existing, x, y)) = slot.as_mut()
                && existing.width() == output.width
                && existing.height() == output.height
            {
                existing.pixels_mut().copy_from_slice(&output.pixels);
                *x = output.x;
                *y = output.y;
            } else {
                let mut bitmap = Bitmap::new(output.width, output.height, BitmapFormat::default());
                bitmap.pixels_mut().copy_from_slice(&output.pixels);
                *slot = Some((bitmap, output.x, output.y));
            }
        }
    }
    pub(crate) fn draw(&self, canvas: &Canvas) {
        if let Some((bitmap, x, y)) = self.bitmap.borrow().as_ref() {
            canvas.draw_bitmap(bitmap, *x, *y);
        }
    }
}
impl Drop for Composite {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().closed = true;
        wake(&self.worker.send, self.key, &self.shared);
    }
}
fn settings(owner: &Node, overlay: bool) -> Result<Settings> {
    let frame = owner.frame.get();
    let scene = owner.scene_active();
    let bounds = if overlay {
        crate::render::composition::geometry::Bounds::new(frame.width, frame.height)
    } else if scene {
        crate::render::composition::geometry::Bounds::new(frame.width, frame.height)
            .own(&owner.style.borrow())
    } else {
        owner.capture_bounds(0)?
    };
    let insets = bounds.insets(frame.width, frame.height, owner.scale);
    let width = (frame.width * owner.scale).ceil().max(1.0) as u32;
    let height = (frame.height * owner.scale).ceil().max(1.0) as u32;
    let width = width
        .checked_add(insets[0])
        .and_then(|v| v.checked_add(insets[2]));
    let height = height
        .checked_add(insets[1])
        .and_then(|v| v.checked_add(insets[3]));
    let (Some(width), Some(height)) = (width, height) else {
        return Err(crate::render::node::Error::Unsupported(
            "compositor capture size overflow",
        ));
    };
    if u64::from(width) * u64::from(height) > 16_777_216 {
        return Err(crate::render::node::Error::Unsupported(
            "compositor capture exceeds 16 megapixels",
        ));
    }
    Ok(Settings {
        width,
        height,
        insets,
        scene,
        filters: if overlay {
            Vec::new()
        } else {
            owner
                .style
                .borrow()
                .effects
                .as_ref()
                .map_or_else(Vec::new, |effects| effects.filters.clone())
        },
        scale: owner.scale,
    })
}
