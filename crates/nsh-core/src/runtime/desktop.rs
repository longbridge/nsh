//! `nsh/desktop`'s app functions (macOS, Windows, Linux; nothing happens
//! elsewhere). Windows and menus, which call the app's functions and live
//! in its reactive graph, are `nsh-js`'s.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use crate::{
    Command, ROOT_ID,
    desktop::DesktopCommand,
    element::{
        CallCx, Callback, JsObject, Local, ModuleCx, PropEnum, Stop, System, module, task::Action,
    },
    single_instance::SecondInstance,
};

fn desktop(cx: &mut ModuleCx, command: DesktopCommand) {
    cx.act(Action::Command(
        ROOT_ID,
        Command::Desktop(Box::new(command)),
    ));
}

/// The application as a whole (desktops; its functions do nothing elsewhere).
#[module]
pub(crate) struct Application;

#[module]
impl Application {
    /// Quits the app, as the application menu's `quit` role and the
    /// platform's quit key do: its windows close and the app stops as when
    /// its last window closes, then its run ends, also while closing the
    /// window alone would leave it running (a window kept hidden, a tray
    /// icon). A view embedded in another app leaves the app's lifecycle to
    /// that app.
    pub(crate) fn quit(cx: &mut ModuleCx) {
        desktop(cx, DesktopCommand::Quit);
    }

    /// Restarts the app: it quits as `quit` does, and once the app has
    /// exited it starts again with the arguments it was started with. For a
    /// setting that takes effect at the next start (the app's language, say):
    /// ask the user first, then call it. Standalone apps on macOS, Windows
    /// and Linux; a view embedded in another app leaves the app's lifecycle
    /// to that app.
    pub(crate) fn restart(cx: &mut ModuleCx) {
        desktop(cx, DesktopCommand::Restart);
    }

    /// Listens to `event`. `secondInstance`: with nsh.json
    /// `"singleInstance": true`, the app was launched again on Windows or
    /// Linux; that launch exited without opening a window and handed this
    /// one its `args` (the program's path not included) and `cwd`, the
    /// directory relative paths among them are relative to. The main window
    /// is already brought forward (shown when hidden); what else the launch
    /// means is the app's. Anyone can start the app with any arguments:
    /// treat them as untrusted input. Launches while nothing listened go to
    /// the next listener (at most 16, the oldest dropped). macOS runs one
    /// instance of an app on its own and reports nothing; files and links
    /// still come through `UrlScheme`. Returns a function that stops
    /// listening.
    pub(crate) fn on(
        event: ApplicationEvent,
        #[args(launch)] listener: Callback<ApplicationSecondInstance>,
        sys: System,
        cx: CallCx<'_>,
    ) -> Stop {
        let ApplicationEvent::SecondInstance = event;
        let launches = sys.locals.get::<Launches>();
        let id = launches.next.get();
        launches.next.set(id.wrapping_add(1));
        for launch in launches.unheard.take() {
            listener.post(launch.into());
        }
        launches.listeners.borrow_mut().push((id, listener));
        if !launches.started.replace(true) {
            cx.start(deliver(Rc::downgrade(&launches)));
        }
        let launches = Rc::downgrade(&launches);
        Stop::new(move || {
            if let Some(launches) = launches.upgrade() {
                launches.listeners.borrow_mut().retain(|(i, _)| *i != id);
            }
        })
    }
}

/// `Application.on`'s events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub(crate) enum ApplicationEvent {
    /// The app was launched again (nsh.json `singleInstance`).
    #[default]
    SecondInstance,
}

/// A later launch of the app (the `secondInstance` event).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub(crate) struct ApplicationSecondInstance {
    /// Its command line's arguments, the program's path not included.
    args: Vec<String>,
    /// The directory it was started in; empty when unknown.
    cwd: String,
}

impl From<SecondInstance> for ApplicationSecondInstance {
    fn from(launch: SecondInstance) -> Self {
        Self {
            args: launch.args,
            cwd: launch.cwd,
        }
    }
}

/// The VM's `secondInstance` listeners.
#[derive(Default)]
struct Launches {
    listeners: RefCell<Vec<(u32, Callback<ApplicationSecondInstance>)>>,
    next: Cell<u32>,
    /// The task waiting for the core's launches started; it lasts as the
    /// VM does.
    started: Cell<bool>,
    /// Launches that came with no listener, for the next one.
    unheard: RefCell<Vec<SecondInstance>>,
}

impl Local for Launches {
    fn close(&self) {
        self.listeners.borrow_mut().clear();
    }
}

/// Posts the core's launches to the listeners, while the VM lasts.
async fn deliver(launches: Weak<Launches>) {
    loop {
        let launch = crate::prelude::next_second_instance().await;
        let Some(launches) = launches.upgrade() else {
            return;
        };
        let listeners = launches.listeners.borrow();
        if listeners.is_empty() {
            let mut unheard = launches.unheard.borrow_mut();
            if unheard.len() == crate::single_instance::MAX_PENDING {
                unheard.remove(0);
            }
            unheard.push(launch);
            continue;
        }
        for (_, listener) in &*listeners {
            listener.post(launch.clone().into());
        }
    }
}
