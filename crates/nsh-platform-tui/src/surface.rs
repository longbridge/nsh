use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{MountBatch, Points, Size};
use nsh_engine::{AppError, Engine, MountSink};

use crate::WakeCallback;

const FRAME_INTERVAL: Duration = Duration::from_millis(16);

pub(crate) enum Message {
    Batch(MountBatch),
    Error(AppError),
    Frame,
    /// The engine laid the app out at this size ([`MountSink::resized`]).
    Resized(Size<Points>),
    /// The engine caught up with the keyboard's focus change of this number.
    Focused(u64),
    #[cfg(feature = "dev")]
    Reload,
}

#[derive(Default)]
pub(crate) struct Wake {
    pending: AtomicBool,
    callback: RwLock<Option<WakeCallback>>,
}

impl Wake {
    pub(crate) fn set(&self, callback: Option<WakeCallback>) {
        *self.callback.write().unwrap_or_else(|e| e.into_inner()) = callback;
        if self.pending.load(Ordering::Acquire) {
            self.call();
        }
    }

    fn call(&self) {
        let callback = self
            .callback
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(callback) = callback {
            callback();
        }
    }

    pub(crate) fn notify(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            self.call();
        }
    }
}

pub(crate) struct Sink {
    pub(crate) sender: Sender<Message>,
    pub(crate) wake: Arc<Wake>,
}

impl Sink {
    fn send(&self, message: Message) {
        if self.sender.send(message).is_ok() {
            self.wake.notify();
        }
    }
}

impl MountSink for Sink {
    fn needs_tree_order(&self) -> bool {
        true
    }

    fn needs_projected_states(&self) -> bool {
        true
    }

    fn submit(&self, batch: MountBatch) {
        self.send(Message::Batch(batch));
    }
    fn report(&self, error: &AppError) {
        self.send(Message::Error(error.clone()));
    }
    #[cfg(feature = "dev")]
    fn hot_update_failed(&self) {
        self.send(Message::Reload);
    }
    fn request_frame(&self) {
        self.send(Message::Frame);
    }
    fn resized(&self, size: Size<Points>) {
        self.send(Message::Resized(size));
    }
}

pub(crate) struct Surface {
    pub(crate) engine: Option<Engine>,
    pub(crate) inbox: Receiver<Message>,
    /// Messages a key's wait for the engine received before the draw.
    pub(crate) held: std::collections::VecDeque<Message>,
    pub(crate) wake: Arc<Wake>,
    /// The viewport the engine's layout is for: the one it started with,
    /// then the last it reported laid out ([`Message::Resized`]).
    pub(crate) laid_out: Size<Points>,
    pub(crate) frame: Option<Instant>,
    pub(crate) animation_frame: Option<Instant>,
    pub(crate) animation_start: Instant,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            engine: None,
            inbox: crossbeam_channel::never(),
            held: Default::default(),
            wake: Arc::default(),
            laid_out: Size::default(),
            frame: None,
            animation_frame: None,
            animation_start: Instant::now(),
        }
    }
}

impl Surface {
    pub(crate) fn deadline(&self) -> Option<Instant> {
        if self.wake.pending.load(Ordering::Acquire)
            || !self.inbox.is_empty()
            || !self.held.is_empty()
        {
            Some(Instant::now())
        } else {
            self.frame.into_iter().chain(self.animation_frame).min()
        }
    }

    /// Whether the engine has laid the app out at a size the surface had.
    /// An app loaded before the surface's first draw starts on an empty
    /// viewport: until the layout at the drawn area arrives, its views are
    /// all at the origin, sized for no room, and showing them would put
    /// them where the app never asked.
    pub(crate) fn sized(&self) -> bool {
        self.laid_out.width > Points(0.0) && self.laid_out.height > Points(0.0)
    }

    pub(crate) fn begin_frame(&mut self) {
        self.wake.pending.store(false, Ordering::Release);
        if self
            .frame
            .is_some_and(|deadline| deadline <= Instant::now())
        {
            self.frame = None;
            if let Some(engine) = &self.engine {
                engine.control().frame();
            }
        }
    }

    pub(crate) fn request_frame(&mut self) {
        self.frame
            .get_or_insert_with(|| Instant::now() + FRAME_INTERVAL);
    }

    pub(crate) fn animate(&mut self, tree: &mut crate::mount::Mounter) {
        let now = Instant::now();
        tree.animator
            .tick(now.duration_since(self.animation_start).as_secs_f64());
        let views = &mut tree.views;
        tree.animator.write(Some, |id, values, args| {
            if let Some(view) = views.get_mut(id as usize).and_then(Option::as_mut) {
                view.animated = values;
                if let Some(canvas) = &mut view.canvas {
                    for &(slot, value) in args {
                        canvas
                            .args
                            .resize(canvas.args.len().max(usize::from(slot) + 1), None);
                        canvas.args[usize::from(slot)] = Some(value);
                    }
                }
            }
        });
        // A delay keeps the graph running even when this tick changed no values.
        self.animation_frame = tree.animator.animating().then_some(now + FRAME_INTERVAL);
        if !tree.animator.finished.is_empty()
            && let Some(engine) = &self.engine
        {
            engine
                .control()
                .animations_finished(std::mem::take(&mut tree.animator.finished));
        }
    }

    pub(crate) fn stop(&mut self) {
        self.engine = None;
        self.inbox = crossbeam_channel::never();
        self.held.clear();
        self.laid_out = Size::default();
        self.frame = None;
        self.animation_frame = None;
        self.animation_start = Instant::now();
        self.wake.notify();
    }
}
