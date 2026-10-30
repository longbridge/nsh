//! C XComponent playback; a media worker owns decoding, bounded input and release.
mod controls;
mod icon;
mod player;
mod source;
mod ui;

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
};

use nsh_core::{Command, VideoProps};
use ohos_arkui_binding::component::attribute::ArkUICommonAttribute;
use ohos_native_window_binding::NativeWindow;
pub(crate) use ui::{UiState, retired, update};

use crate::render::{
    geometry::Size,
    node::{NativeNode, Node, Result},
};

enum Operation {
    Configure(Box<VideoProps>, u64),
    Context(bool),
    Seek(f64),
    Toggle,
    Window(Option<NativeWindow>),
    WindowGone(usize),
    Wake,
    Close,
}
pub(crate) struct Video {
    ui: Rc<ui::Ui>,
    queue: mpsc::SyncSender<Operation>,
    active: Arc<AtomicBool>,
    source: RefCell<Option<Arc<str>>>,
    source_epoch: Arc<AtomicU64>,
    closed: Cell<bool>,
}
impl Video {
    pub(crate) fn native(&self) -> &NativeNode {
        &self.ui.native
    }
    /// Fullscreen is a UI-thread reparenting operation on the same player.
    pub(crate) fn set_fullscreen(&self, enabled: bool) -> Result<()> {
        self.ui.set_fullscreen(enabled)
    }
    /// Whether the surface root owns the player's native Stack.
    pub(crate) fn is_fullscreen(&self) -> bool {
        self.ui.is_fullscreen()
    }
    /// Root layout supplies the fullscreen size independently of the owner.
    pub(crate) fn fullscreen_frame(&self, size: Size) -> Result<()> {
        self.ui.fullscreen_frame(size)
    }
    pub(crate) fn new(owner: Weak<Node>) -> Result<Self> {
        let owner = owner.upgrade().expect("live Video owner");
        let (queue, receiver) = mpsc::sync_channel(128);
        let active = Arc::new(AtomicBool::new(true));
        let source_epoch = Arc::new(AtomicU64::new(0));
        let ui = ui::Ui::new(&owner, queue.clone())?;
        let worker = match player::Worker::new(
            &owner,
            ui.token,
            active.clone(),
            source_epoch.clone(),
            queue.clone(),
        ) {
            Ok(worker) => worker,
            Err(error) => {
                ui.abort();
                return Err(error);
            }
        };
        let attached = { owner.native.borrow_mut().add_child(ui.native.clone()) };
        if let Err(error) = attached {
            ui.abort();
            return Err(error.into());
        }
        if let Err(error) = std::thread::Builder::new()
            .name("nsh-video".into())
            .spawn(move || worker.run(receiver))
        {
            active.store(false, Ordering::Release);
            ui.abort();
            return Err(napi_ohos::Error::from_reason(error.to_string()).into());
        }
        Ok(Self {
            ui,
            queue,
            active,
            source: RefCell::default(),
            source_epoch,
            closed: Cell::new(false),
        })
    }
    pub(crate) fn configure(&self, props: Box<VideoProps>) -> Result<()> {
        if self.closed.get() {
            return Ok(());
        }
        if *self.source.borrow() != props.source {
            let epoch = self
                .source_epoch
                .load(Ordering::Acquire)
                .checked_add(1)
                .ok_or_else(|| {
                    napi_ohos::Error::from_reason("Video source generation exhausted")
                })?;
            *self.source.borrow_mut() = props.source.clone();
            self.source_epoch.store(epoch, Ordering::Release);
        }
        self.ui.configure(&props)?;
        self.send(Operation::Configure(
            props,
            self.source_epoch.load(Ordering::Acquire),
        ))
    }
    pub(crate) fn context(&self, ready: bool) -> Result<()> {
        if self.closed.get() {
            return Ok(());
        }
        if ready {
            self.send(Operation::Window(self.ui.current_window()?))?;
        }
        self.send(Operation::Context(ready))
    }
    pub(crate) fn command(&self, command: Command) -> Result<()> {
        if let Command::Seek(seconds) = command {
            self.send(Operation::Seek(seconds))
        } else {
            Ok(())
        }
    }
    fn send(&self, operation: Operation) -> Result<()> {
        if !self.closed.get() {
            self.queue.try_send(operation).map_err(|_| {
                napi_ohos::Error::from_reason("Video operation queue is full or closed")
            })?;
        }
        Ok(())
    }
    pub(crate) fn tick(&self, now: f64) -> Result<bool> {
        if self.closed.get() {
            return Ok(false);
        }
        self.ui.tick(now)
    }
    pub(crate) fn frame(&self, size: Size) -> Result<()> {
        if self.closed.get() {
            return Ok(());
        }
        self.ui.frame(size)
    }
    pub(crate) fn close(&self) -> Result<()> {
        if !self.closed.replace(true) {
            // Retain the complete UI owner before the worker may enqueue its final ack.
            self.ui.begin_retirement();
            self.active.store(false, Ordering::Release);
            let _ = self.queue.try_send(Operation::Close);
        }
        Ok(())
    }
}
impl Drop for Video {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            log::error!("OHOS Video close: {error}");
        }
    }
}
