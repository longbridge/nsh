use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::MountBatch;
use nsh_engine::{AppError, Engine, MountSink};

use crate::WakeCallback;

pub(crate) enum Message {
    Batch(MountBatch),
    Error(AppError),
    Frame,
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
}

pub(crate) struct Surface {
    pub(crate) engine: Option<Engine>,
    pub(crate) inbox: Receiver<Message>,
    pub(crate) wake: Arc<Wake>,
    pub(crate) frame: Option<Instant>,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            engine: None,
            inbox: crossbeam_channel::never(),
            wake: Arc::default(),
            frame: None,
        }
    }
}

impl Surface {
    pub(crate) fn deadline(&self) -> Option<Instant> {
        if self.wake.pending.load(Ordering::Acquire) || !self.inbox.is_empty() {
            Some(Instant::now())
        } else {
            self.frame
        }
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
            .get_or_insert_with(|| Instant::now() + Duration::from_millis(16));
    }

    pub(crate) fn stop(&mut self) {
        self.engine = None;
        self.inbox = crossbeam_channel::never();
        self.frame = None;
        self.wake.notify();
    }
}
