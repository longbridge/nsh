//! Standalone-window close observation and its native, sampled policy.
//! Requests are completed on AppKit's main queue, with a bounded grace period.

use std::cell::Cell;

use dispatch2::{DispatchQueue, DispatchTime};
use nsh_core::{EventKind, EventPayload, ROOT_ID};

use crate::surface::Surface;

pub(crate) struct State {
    pub(crate) close_on_press: Cell<bool>,
    next: Cell<u64>,
    pending: Cell<Option<(u64, bool)>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            close_on_press: Cell::new(true),
            next: Cell::new(0),
            pending: Cell::new(None),
        }
    }
}

impl State {
    pub(crate) fn reset(&self) {
        self.close_on_press.set(true);
        self.pending.set(None);
        // Keep next monotonic: a queued reply may belong to the old engine.
    }
}

pub(crate) fn press(surface: &Surface) {
    let state = &surface.window_close;
    if state.pending.get().is_some() {
        return;
    }
    let serial = state.next.get() + 1;
    state.next.set(serial);
    let close = state.close_on_press.get();
    state.pending.set(Some((serial, close)));
    // Save handlers read the actual last frame, including a move still settling.
    crate::window_state::changed(surface);
    let key = surface.key();
    let sent = Cell::new(false);
    surface.control(|control| {
        sent.set(true);
        control.dispatch_event(ROOT_ID, EventKind::WindowClosePress, EventPayload::None);
        control.flush(move |_| {
            DispatchQueue::main().exec_async(move || complete(key, serial));
        });
    });
    if !sent.get() {
        // Leave windowShouldClose: before running a close's delegate callbacks.
        DispatchQueue::main().exec_async(move || complete(key, serial));
    } else if close {
        let when = DispatchTime::NOW.time(250_000_000);
        if DispatchQueue::main()
            .after(when, move || complete(key, serial))
            .is_err()
        {
            DispatchQueue::main().exec_async(move || complete(key, serial));
        }
    }
}

fn complete(key: u64, serial: u64) {
    let Some(surface) = Surface::with_key(key) else {
        return;
    };
    let Some((pending, close)) = surface.window_close.pending.get() else {
        return;
    };
    if pending != serial {
        return;
    }
    surface.window_close.pending.set(None);
    if close {
        crate::app::close_main_window(key);
    }
}
