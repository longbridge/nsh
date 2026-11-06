//! The standalone GTK window's close-request signal and sampled native policy.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use gtk4::{self as gtk, glib, prelude::*};
use nsh_core::{EventKind, EventPayload, ROOT_ID};

use crate::surface::Surface;

pub(crate) struct State {
    pub(crate) close_on_press: Cell<bool>,
    next: Cell<u64>,
    pending: Cell<Option<(u64, bool)>>,
    window: RefCell<glib::WeakRef<gtk::Window>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            close_on_press: Cell::new(true),
            next: Cell::new(0),
            pending: Cell::new(None),
            window: RefCell::default(),
        }
    }
}

impl State {
    pub(crate) fn reset(&self) {
        self.close_on_press.set(true);
        self.pending.set(None);
    }
}

pub(crate) fn attach(surface: &Rc<Surface>, window: &gtk::Window) {
    *surface.window_close.window.borrow_mut() = window.downgrade();
    let weak = Rc::downgrade(surface);
    window.connect_close_request(move |_| {
        if let Some(surface) = weak.upgrade() {
            press(&surface);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
}

/// Embedded windows keep their host's close-request handler.
pub(crate) fn close_explicitly(surface: &Surface) -> bool {
    let window = surface.window_close.window.borrow().upgrade();
    let Some(window) = window else { return false };
    glib::idle_add_local_once(move || window.destroy());
    true
}

fn press(surface: &Surface) {
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
            glib::idle_add_once(move || complete(key, serial));
        });
    });
    if !sent.get() {
        glib::idle_add_local_once(move || complete(key, serial));
    } else if close {
        glib::timeout_add_local_once(Duration::from_millis(250), move || complete(key, serial));
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
        let window = surface.window_close.window.borrow().upgrade();
        if let Some(window) = window {
            // destroy() bypasses close-request, as an explicit Window.close does.
            window.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk_close_request_keeps_a_blocked_window_and_explicit_close_destroys_it() {
        crate::gtk_test::run(|| {
            let surface = crate::gtk_test::surface();
            let window = crate::gtk_test::show(&surface);
            attach(&surface, &window);
            surface.window_close.close_on_press.set(false);
            window.close();
            assert!(crate::gtk_test::wait_until(|| surface
                .window_close
                .pending
                .get()
                .is_none()));
            assert!(window.is_visible());
            crate::chrome::choose(&surface, 0, Some(nsh_core::desktop::MenuRole::Close));
            assert!(crate::gtk_test::wait_until(|| surface
                .window_close
                .pending
                .get()
                .is_some()));
            assert!(crate::gtk_test::wait_until(|| surface
                .window_close
                .pending
                .get()
                .is_none()));
            assert!(window.is_visible());
            assert!(close_explicitly(&surface));
            assert!(crate::gtk_test::wait_until(|| !window.is_visible()));
        });
    }

    #[test]
    fn gtk_reload_invalidates_queued_close_and_keeps_the_sampled_policy() {
        crate::gtk_test::run(|| {
            let surface = crate::gtk_test::surface();
            let window = crate::gtk_test::show(&surface);
            attach(&surface, &window);
            window.close();
            let old = surface.window_close.pending.get().unwrap().0;
            surface.stop();
            surface.window_close.close_on_press.set(false);
            window.close();
            let current = surface.window_close.pending.get().unwrap().0;
            assert_ne!(old, current);
            complete(surface.key(), old);
            assert_eq!(surface.window_close.pending.get(), Some((current, false)));
            surface.window_close.close_on_press.set(true);
            complete(surface.key(), current);
            assert!(window.is_visible());
            window.close();
            assert!(crate::gtk_test::wait_until(|| !window.is_visible()));
        });
    }
}
