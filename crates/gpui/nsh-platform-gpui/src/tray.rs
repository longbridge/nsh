//! A surface's system tray, drawn by its host rather than by GPUI.

use std::{cell::RefCell, rc::Rc};

use nsh_core::{
    EventKind, EventPayload, PayloadValue, ROOT_ID,
    desktop::Tray,
    geometry::{Bounds, Points},
};

use crate::surface::Surface;

/// A tray menu selection or click on the icon, reported on the GPUI thread.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct TrayActivation {
    /// The selected menu item's ID or the tray configuration's click ID.
    pub id: u32,
    /// The tray icon's bounds in screen points, when known for an icon click.
    pub bounds: Option<Bounds<Points>>,
}

impl TrayActivation {
    /// An activation with no known bounds (including menu selections).
    pub fn new(id: u32) -> Self {
        Self { id, bounds: None }
    }
}

/// Presents the system tray for one [`crate::AppView`]. All calls are on
/// the GPUI thread. The host owns the native item, its menu, and their input.
pub trait TrayHost {
    /// Shows `tray`. Call `activate` on the GPUI thread for the configured
    /// icon click or a selected menu item. Return a function that removes
    /// this item when the tray changes or the surface stops.
    fn show(&self, tray: &Tray, activate: Rc<dyn Fn(TrayActivation)>) -> Box<dyn FnOnce()>;
}

#[derive(Default)]
pub(crate) struct TrayState {
    host: RefCell<Option<Rc<dyn TrayHost>>>,
    token: RefCell<Option<Rc<()>>>,
    remove: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl TrayState {
    pub(crate) fn set_host(&self, host: Option<Rc<dyn TrayHost>>) {
        self.host.replace(host);
    }

    pub(crate) fn clear(&self) {
        self.token.take();
        if let Some(remove) = self.remove.take() {
            remove();
        }
    }

    pub(crate) fn set(&self, surface: &Rc<Surface>, tray: Option<&Tray>) {
        let Some(host) = self.host.borrow().clone() else {
            return;
        };
        let token = Rc::new(());
        self.token.replace(Some(token.clone()));
        let token = Rc::downgrade(&token);
        let surface = Rc::downgrade(surface);
        let tray = tray.cloned();
        crate::rt::on_app(move |_| {
            let Some(surface) = surface.upgrade().filter(|_| token.strong_count() != 0) else {
                return;
            };
            // The current token already revoked the old callback, even
            // if the host reports an activation while removing the item.
            if let Some(remove) = surface.tray.remove.take() {
                remove();
            }
            let Some(tray) = tray.filter(|_| token.strong_count() != 0) else {
                return;
            };
            let target = Rc::downgrade(&surface);
            let current = token.clone();
            let activate = Rc::new(move |activation: TrayActivation| {
                let target = target.clone();
                let current = current.clone();
                crate::rt::on_app(move |_| {
                    if let Some(surface) = target.upgrade().filter(|_| current.strong_count() != 0) {
                        let mut payload = vec![("id", PayloadValue::Number(f64::from(activation.id)))];
                        if let Some(bounds) = activation.bounds {
                            payload.push(("bounds", PayloadValue::Bounds(bounds)));
                        }
                        surface.event(ROOT_ID, EventKind::Menu, EventPayload::Map(payload));
                    }
                });
            });
            let remove = host.show(&tray, activate);
            if token.strong_count() != 0 {
                surface.tray.remove.replace(Some(remove));
            } else {
                remove();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use nsh_core::desktop::DesktopCommand;

    struct Host {
        shown: Cell<usize>,
        removed: Rc<Cell<usize>>,
    }

    impl TrayHost for Host {
        fn show(&self, _: &Tray, _: Rc<dyn Fn(TrayActivation)>) -> Box<dyn FnOnce()> {
            self.shown.set(self.shown.get() + 1);
            let removed = self.removed.clone();
            Box::new(move || removed.set(removed.get() + 1))
        }
    }

    #[gpui::test]
    fn an_embedded_surface_replaces_and_clears_its_host_tray(cx: &mut gpui::TestAppContext) {
        let _app = cx.update(crate::rt::init_test_app);
        let surface = Surface::new(Default::default());
        let host = Rc::new(Host {
            shown: Cell::new(0),
            removed: Rc::new(Cell::new(0)),
        });
        surface.tray.set_host(Some(host.clone()));
        let set = |tray| crate::chrome::command(&surface, &DesktopCommand::SetTray(tray));

        set(Some(Tray::default()));
        cx.run_until_parked();
        assert_eq!((host.shown.get(), host.removed.get()), (1, 0));

        set(Some(Tray::default()));
        cx.run_until_parked();
        assert_eq!((host.shown.get(), host.removed.get()), (2, 1));

        set(None);
        cx.run_until_parked();
        assert_eq!((host.shown.get(), host.removed.get()), (2, 2));

        set(Some(Tray::default()));
        cx.run_until_parked();
        surface.stop();
        assert_eq!((host.shown.get(), host.removed.get()), (3, 3));
    }
}
