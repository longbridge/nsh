//! Close observation for an owned GPUI window, after the embedding host's policy.

use std::cell::Cell;

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
    let key = surface.key();
    let sent = Cell::new(false);
    surface.control(|control| {
        sent.set(true);
        control.dispatch_event(ROOT_ID, EventKind::WindowClosePress, EventPayload::None);
        control.flush(move |_| {
            crate::rt::invoke(move || complete(key, serial));
        });
    });
    if !sent.get() {
        crate::rt::on_app(move |_| complete(key, serial));
    } else if close {
        crate::rt::after_ms(250, move || complete(key, serial));
    }
}

fn complete(key: u64, serial: u64) {
    let Some(surface) = Surface::by_key(key) else {
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
        crate::chrome::finish_close_press(&surface);
    }
}
