//! Shader layers: each shader view has a native surface first among its
//! children (an XComponent, `surface.rs`), and a thread of the surface's own
//! (`nsh-shader`) renders the view's frames straight into its window with
//! wgpu (`gpu.rs`). RenderService composites the window as it does a
//! video's; nothing is read back or drawn again on the UI thread.
//!
//! wgpu has only its GLES backend on OHOS, where rendering waits for the GPU
//! on the calling thread, so the UI thread only sends the worker what changed
//! ([`Command`]: layers, animated slots, sizes, whether a view is in sight,
//! its window). The worker renders on the surface's own VSync (the rate the
//! app's window asked for) while a view in sight has a frame due, and blocks
//! for a command otherwise. A view that goes keeps its XComponent until the
//! worker has dropped its surface of the window, which `Message::Shader`
//! reports ([`Shared::retired`]).

mod gpu;
mod surface;

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use nsh_core::{NodeId, animation::AnimationValue, shader::ShaderLayer};
use ohos_arkui_binding::{
    component::attribute::ArkUICommonAttribute, types::advanced::NodeDirtyFlag,
};
use ohos_native_window_binding::NativeWindow;
use ohos_vsync_binding::Vsync;
use rustc_hash::FxHashMap;

use super::{
    geometry::Size,
    node::{NativeNode, Node, Result, callback},
};
use crate::app::runtime::{Dispatcher, Message};

/// What the UI thread tells the worker, in order.
enum Command {
    Layer(NodeId, Arc<ShaderLayer>),
    Slot(NodeId, u16, AnimationValue),
    /// Its size in points and its scale (pixels per point).
    Size(NodeId, Size, f32),
    /// Whether it may show: out of sight it renders nothing.
    Shown(NodeId, bool),
    /// The native window of its XComponent (`None`: not one yet).
    Window(NodeId, Option<NativeWindow>),
    /// The window at this address went.
    WindowGone(NodeId, usize),
    /// It went: the worker drops it, then reports it ([`Shared::retired`]).
    Remove(NodeId),
    /// Look again at what is due: the window's VSync came back (the app
    /// returned to the foreground).
    Wake,
}

/// What the worker hands the UI thread.
#[derive(Default)]
struct Shared {
    /// Views whose surfaces the worker has dropped, in the order removed.
    retired: Mutex<Vec<NodeId>>,
    /// `Message::Shader` is on its way: posted once until it is handled.
    posted: AtomicBool,
}

/// The surface's shader thread. Dropping it ends the thread once it has
/// handled what was sent; the GPU device is dropped there, never on the UI
/// thread.
struct Worker {
    send: mpsc::Sender<Command>,
    shared: Arc<Shared>,
}

impl Worker {
    fn spawn(dispatcher: std::sync::Weak<Dispatcher>, vsync: Arc<Vsync>) -> Option<Worker> {
        let (send, receive) = mpsc::channel();
        let shared = Arc::new(Shared::default());
        let worker_shared = shared.clone();
        match std::thread::Builder::new()
            .name("nsh-shader".into())
            .spawn(move || run(&receive, &worker_shared, &dispatcher, &vsync))
        {
            Ok(_) => Some(Worker { send, shared }),
            Err(error) => {
                log::warn!("shaders: no shader thread ({error}); shader views draw nothing");
                None
            }
        }
    }

    fn send(&self, command: Command) {
        let _ = self.send.send(command);
    }
}

/// The worker's loop: the commands sent, then a frame of every view due,
/// once per frame of the surface's VSync while one is; it blocks for a
/// command while none is.
fn run(
    receive: &mpsc::Receiver<Command>,
    shared: &Shared,
    dispatcher: &std::sync::Weak<Dispatcher>,
    vsync: &Vsync,
) {
    let (tick, ticks) = mpsc::sync_channel::<()>(1);
    let mut gpu: Option<Option<gpu::Gpu>> = None;
    let mut views: FxHashMap<NodeId, gpu::View> = FxHashMap::default();
    let mut due = false;
    loop {
        let first = if due {
            match receive.try_recv() {
                Ok(command) => Some(command),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        } else {
            match receive.recv() {
                Ok(command) => Some(command),
                Err(_) => return,
            }
        };
        let mut retired = Vec::new();
        for command in first.into_iter().chain(receive.try_iter()) {
            match command {
                Command::Layer(id, layer) => {
                    gpu.get_or_insert_with(gpu::Gpu::new);
                    views.entry(id).or_default().set_layer(layer);
                }
                Command::Slot(id, slot, value) => {
                    if let Some(view) = views.get_mut(&id) {
                        view.set_slot(slot, value);
                    }
                }
                Command::Size(id, size, scale) => {
                    if let Some(view) = views.get_mut(&id) {
                        view.set_size(size.into(), scale);
                    }
                }
                Command::Shown(id, shown) => {
                    if let Some(view) = views.get_mut(&id) {
                        view.shown = shown;
                    }
                }
                Command::Window(id, window) => {
                    if let (Some(view), Some(Some(gpu))) = (views.get_mut(&id), &gpu) {
                        view.set_window(gpu, window);
                    }
                }
                Command::WindowGone(id, raw) => {
                    // A window made since keeps drawing.
                    if let (Some(view), Some(Some(gpu))) = (views.get_mut(&id), &gpu)
                        && view.window() == Some(raw)
                    {
                        view.set_window(gpu, None);
                    }
                }
                Command::Remove(id) => {
                    views.remove(&id);
                    retired.push(id);
                }
                Command::Wake => {}
            }
        }
        if !retired.is_empty() {
            shared.retired.lock().unwrap().extend(retired);
            if !shared.posted.swap(true, Ordering::AcqRel)
                && let Some(dispatcher) = dispatcher.upgrade()
            {
                dispatcher.send(Message::Shader);
            }
        }
        let Some(Some(gpu)) = &mut gpu else {
            due = false;
            continue;
        };
        let now = Instant::now();
        due = false;
        for view in views.values_mut() {
            if view.due() {
                view.render(gpu, now);
                due |= view.due();
            }
        }
        if due {
            // The next frame of the app's window; a VSync that cannot be asked
            // (the window went) leaves the loop to the next command.
            let tick = tick.clone();
            let status = vsync.request_frame_once(move |_| {
                let _ = tick.try_send(());
            });
            if status != 0 || ticks.recv_timeout(Duration::from_millis(100)).is_err() {
                due = false;
            }
        }
    }
}

/// The shader views of a surface, on the UI thread.
pub(crate) struct Shaders {
    /// The surface's VSync, which paces the worker.
    vsync: Arc<Vsync>,
    /// Made with the first shader (`None` inside when no thread starts).
    worker: RefCell<Option<Option<Worker>>>,
    views: RefCell<FxHashMap<NodeId, State>>,
    /// Views that went, oldest first, until the worker has let go of their
    /// windows (an id can come back meanwhile: they are told apart by
    /// order).
    retiring: RefCell<Vec<(NodeId, Retiring)>>,
}

struct State {
    owner: Weak<Node>,
    surface: surface::Surface,
    /// What the worker was last told: the size and scale, whether it shows.
    size: Option<(Size, f32)>,
    shown: Option<bool>,
}

/// A view's surface kept after it went, with the view's native node it is
/// in (kept too: the view leaves disposing it to this when it is held).
struct Retiring {
    surface: surface::Surface,
    parent: NativeNode,
}

impl Shaders {
    pub(crate) fn new(vsync: Arc<Vsync>) -> Shaders {
        Shaders {
            vsync,
            worker: RefCell::default(),
            views: RefCell::default(),
            retiring: RefCell::default(),
        }
    }

    pub(crate) fn set(&self, owner: &Rc<Node>, layer: Option<Arc<ShaderLayer>>) -> Result<()> {
        let Some(layer) = layer else {
            self.remove(owner.id);
            return Ok(());
        };
        let mut worker = self.worker.borrow_mut();
        let Some(worker) = worker
            .get_or_insert_with(|| Worker::spawn(owner.dispatcher.clone(), self.vsync.clone()))
            .as_ref()
        else {
            return Ok(());
        };
        worker.send(Command::Layer(owner.id, layer));
        if !self.views.borrow().contains_key(&owner.id) {
            let surface = surface::Surface::new(owner.id, worker.send.clone())?;
            // Under the app's children, after the other platform-owned ones.
            let index = owner.child_index(0);
            owner
                .native
                .borrow_mut()
                .insert_child(surface.native.clone(), index)?;
            self.views.borrow_mut().insert(
                owner.id,
                State {
                    owner: Rc::downgrade(owner),
                    surface,
                    size: None,
                    shown: None,
                },
            );
            owner.enable_layout();
        }
        owner.animator.request_frame();
        Ok(())
    }

    pub(crate) fn contains(&self, id: NodeId) -> bool {
        self.views.borrow().contains_key(&id)
    }

    /// The native node of `id`'s surface, which its view lays out.
    pub(crate) fn native(&self, id: NodeId) -> Option<NativeNode> {
        self.views
            .borrow()
            .get(&id)
            .map(|state| state.surface.native.clone())
    }

    pub(crate) fn remove(&self, id: NodeId) {
        let Some(state) = self.views.borrow_mut().remove(&id) else {
            return;
        };
        let parent = state.owner.upgrade().map(|owner| owner.native.clone());
        let worker = self.worker.borrow();
        match (worker.as_ref().and_then(Option::as_ref), parent) {
            (Some(worker), Some(parent)) => {
                worker.send(Command::Remove(id));
                self.retiring.borrow_mut().push((
                    id,
                    Retiring {
                        surface: state.surface,
                        parent,
                    },
                ));
            }
            // The worker never drew into it.
            (_, parent) => callback(|| state.surface.dispose(parent.as_ref())),
        }
    }

    pub(crate) fn slot(&self, id: NodeId, slot: u16, value: AnimationValue) {
        if self.views.borrow().contains_key(&id)
            && let Some(Some(worker)) = self.worker.borrow().as_ref()
        {
            worker.send(Command::Slot(id, slot, value));
        }
    }

    /// A display frame: tells the worker what changed of its views (in
    /// sight, size) and gives each surface its view's box. The worker keeps
    /// its own pace, so this never asks for another frame.
    pub(crate) fn frame(&self) -> Result<bool> {
        let worker = self.worker.borrow();
        let Some(Some(worker)) = worker.as_ref() else {
            return Ok(false);
        };
        let mut views = self.views.borrow_mut();
        for (&id, state) in views.iter_mut() {
            let Some(owner) = state.owner.upgrade() else {
                continue;
            };
            let shown = in_sight(&owner);
            if state.shown != Some(shown) {
                state.shown = Some(shown);
                worker.send(Command::Shown(id, shown));
            }
            let frame = owner.frame.get();
            let size = (Size::new(frame.width, frame.height), owner.scale);
            if state.size != Some(size) {
                state.size = Some(size);
                worker.send(Command::Size(id, size.0, size.1));
            }
            if state
                .surface
                .place((frame.width, frame.height), owner.border_radii())?
            {
                owner
                    .native
                    .borrow()
                    .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
            }
        }
        Ok(false)
    }

    /// The app came back to the foreground: a VSync that stopped while it was
    /// away left the worker waiting for a command.
    pub(crate) fn wake(&self) {
        if let Some(Some(worker)) = self.worker.borrow().as_ref() {
            worker.send(Command::Wake);
        }
    }

    /// Scroll views moved: tells the worker which views came into sight or
    /// left it.
    pub(crate) fn reveal(&self) -> Result<()> {
        self.frame().map(|_| ())
    }

    /// `Message::Shader`: releases the surfaces of views the worker has let
    /// go of.
    pub(crate) fn collect(&self) -> Result<()> {
        let worker = self.worker.borrow();
        let Some(Some(worker)) = worker.as_ref() else {
            return Ok(());
        };
        worker.shared.posted.store(false, Ordering::Release);
        let retired = std::mem::take(&mut *worker.shared.retired.lock().unwrap());
        let mut retiring = self.retiring.borrow_mut();
        for id in retired {
            let Some(at) = retiring.iter().position(|(r, _)| *r == id) else {
                continue;
            };
            let (_, Retiring { surface, parent }) = retiring.remove(at);
            callback(|| surface.dispose(Some(&parent)));
            // The view went meanwhile and left its native node to this.
            if parent.borrow().is_owned() && Rc::strong_count(&parent) == 1 {
                callback(|| {
                    parent.borrow().remove_from_parent()?;
                    parent.borrow_mut().dispose()?;
                    Ok(())
                });
            }
        }
        Ok(())
    }
}

/// Whether `node` can be seen: in the tree and shown, and its box (as
/// ArkUI places it, translations and scroll offsets included) within the
/// surface's root and within every scroll view around it, which clip their
/// content (a row scrolled away, a card below the fold).
fn in_sight(node: &Rc<Node>) -> bool {
    if !node.visible_in_tree() {
        return false;
    }
    let rect = |n: &Node| -> Option<(i32, i32, i32, i32)> {
        let native = n.mounted();
        let native = native.borrow();
        let origin = native.position_with_translate_in_window().ok()?;
        let size = native.layout_size().ok()?;
        Some((
            origin.x,
            origin.y,
            origin.x + size.width,
            origin.y + size.height,
        ))
    };
    let Some(own) = rect(node) else {
        return true;
    };
    if own.2 <= own.0 || own.3 <= own.1 {
        return false;
    }
    let mut at = node.parent_node.borrow().upgrade();
    while let Some(ancestor) = at {
        let clips = ancestor.id == nsh_core::ROOT_ID || ancestor.scroll.borrow().is_some();
        if clips
            && let Some(r) = rect(&ancestor)
            && (own.0 >= r.2 || own.2 <= r.0 || own.1 >= r.3 || own.3 <= r.1)
        {
            return false;
        }
        at = ancestor.parent_node.borrow().upgrade();
    }
    true
}
