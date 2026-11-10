//! `nsh/desktop`'s app functions (macOS, Windows, Linux; elsewhere `quit`
//! and `restart` do nothing and the login item is unavailable). Windows and
//! menus, which call the app's functions and live in its reactive graph,
//! are `nsh-js`'s.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use crate::{
    Command, ROOT_ID,
    desktop::{DesktopCommand, LoginItem, LoginItemError, SetLoginItemOptions},
    element::{
        CallCx, Callback, JsError, JsHost, JsIn, JsObject, JsOwned, JsValue, Local, ModuleCx,
        PropEnum, PropError, Ret, Stop, System, TsTypes, codes, io_code, module, task::Action,
    },
    single_instance::SecondInstance,
};

fn desktop(cx: &mut ModuleCx, command: DesktopCommand) {
    cx.act(Action::Command(
        ROOT_ID,
        Command::Desktop(Box::new(command)),
    ));
}

/// Whether the app may use its login item: why not when it may not.
fn login_item_granted(sys: &System) -> Result<(), JsError> {
    if sys.access.allows_login_item() {
        return Ok(());
    }
    Err(login_item_error(LoginItemError::AccessDenied))
}

/// What a login item call rejects with.
fn login_item_error(error: LoginItemError) -> JsError {
    match error {
        LoginItemError::AccessDenied => JsError::access_denied(error.to_string(), "loginItem", ""),
        LoginItemError::Unavailable => {
            JsError::Error(error.to_string()).with_code(codes::UNAVAILABLE)
        }
        // Every failure has a code to branch on: the error's own, else EIO.
        LoginItemError::System(io) => {
            let code = io_code(&io).unwrap_or("EIO");
            JsError::Error(LoginItemError::System(io).to_string()).with_code(code)
        }
    }
}

/// The application as a whole (desktops; elsewhere `quit` and `restart` do
/// nothing, and the login item is `"unavailable"`).
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

    /// The app's login item: whether the system opens the app when the
    /// user logs in, as the system has it now (nothing follows the system's
    /// settings on its own: ask again when the app shows it). Its `status`:
    /// `"enabled"`, `"disabled"`, `"requiresApproval"` (registered, and
    /// waiting for the user in the system's settings, where only they turn
    /// it on) or `"unavailable"` (this run has no login item: a view
    /// embedded in another app, `nsh run`, the platforms without desktop
    /// windows). Standalone apps on macOS, Windows and Linux. Rejects with
    /// `ERR_ACCESS_DENIED` when the `loginItem` capability is not granted,
    /// and with the system's error (Node's code, `EIO` without one).
    pub(crate) async fn login_item(sys: System) -> Result<LoginItem, JsError> {
        login_item_granted(&sys)?;
        crate::element::desktop::login_item()
            .await
            .map_err(login_item_error)
    }

    /// Registers the app to open when the user logs in (`enabled: true`) or
    /// removes its registration, and resolves with the login item after it:
    /// registering what is registered and removing what is not there
    /// succeed. Call it when the user turns the app's own setting on or off,
    /// never on the app's own (at every start, say): what the user chose in
    /// the system's settings stands. The changes run in the order the app
    /// makes them.
    /// Rejects with `ERR_ACCESS_DENIED` when the `loginItem` capability is
    /// not granted, with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` where the
    /// status is `"unavailable"`, and with the system's error (Node's code,
    /// `EIO` without one).
    pub(crate) async fn set_login_item(
        options: SetLoginItemOptions,
        sys: System,
    ) -> Result<LoginItem, JsError> {
        login_item_granted(&sys)?;
        crate::element::desktop::set_login_item(options)
            .await
            .map_err(login_item_error)
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
    /// still come through `UrlScheme`. `quit`: the app is quitting (its Quit
    /// item, `quit()`, its last window closing, the system ending it, a host
    /// app removing its view), for its cleanup, as GPUI's `on_app_quit`: it
    /// cannot keep the app running (an app that asks first quits from a Quit
    /// item of its own). A promise the listener returns is awaited, 200 ms
    /// at most for all of them, while the platform waits too. Returns a
    /// function that stops listening.
    #[overload(fn(event: ApplicationSecondInstanceEvent, #[args(launch)] listener: Callback<ApplicationSecondInstance>) -> Stop)]
    #[overload(fn(event: ApplicationQuitEvent, listener: Callback<()>) -> Stop)]
    pub(crate) fn on(
        event: ApplicationEvent,
        #[args(payload)] listener: Callback<Payload>,
        sys: System,
        cx: CallCx<'_>,
    ) -> Stop {
        match event {
            ApplicationEvent::SecondInstance => second_instance(listener, &sys, cx),
            ApplicationEvent::Quit => quit(listener, &sys, cx),
        }
    }
}

fn second_instance(listener: Callback<Payload>, sys: &System, cx: CallCx<'_>) -> Stop {
    let launches = sys.locals.get::<Launches>();
    let id = launches.next.get();
    launches.next.set(id.wrapping_add(1));
    for launch in launches.unheard.take() {
        listener.post(Payload::SecondInstance(launch.into()));
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

fn quit(listener: Callback<Payload>, sys: &System, cx: CallCx<'_>) -> Stop {
    let quits = sys.locals.get::<Quits>();
    let id = quits.next.get();
    quits.next.set(id.wrapping_add(1));
    quits.listeners.borrow_mut().push((id, listener));
    if !quits.started.replace(true) {
        cx.start(hold(Rc::downgrade(&quits)));
    }
    let quits = Rc::downgrade(&quits);
    Stop::new(move || {
        if let Some(quits) = quits.upgrade() {
            quits.listeners.borrow_mut().retain(|(i, _)| *i != id);
        }
    })
}

/// `Application.on`'s events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub(crate) enum ApplicationEvent {
    /// The app was launched again (nsh.json `singleInstance`).
    #[default]
    SecondInstance,
    /// The app is quitting.
    Quit,
}

/// The event of a later launch (`on`'s first overload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub(crate) enum ApplicationSecondInstanceEvent {
    SecondInstance,
}

/// The event of quitting (`on`'s second overload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub(crate) enum ApplicationQuitEvent {
    Quit,
}

/// What a listener gets.
#[derive(Debug, Clone)]
pub(crate) enum Payload {
    SecondInstance(ApplicationSecondInstance),
    Quit,
}

impl JsValue for Payload {
    fn ts_type(types: &mut TsTypes) -> String {
        <ApplicationSecondInstance as JsValue>::ts_type(types)
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        Err(value.expected("no value (a payload goes to JS only)"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self {
            Payload::SecondInstance(launch) => launch.to_js(host),
            Payload::Quit => Ret::into_js((), host),
        }
    }
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
    listeners: RefCell<Vec<(u32, Callback<Payload>)>>,
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
            listener.post(Payload::SecondInstance(launch.clone().into()));
        }
    }
}

/// The VM's `quit` listeners.
#[derive(Default)]
struct Quits {
    listeners: RefCell<Vec<(u32, Callback<Payload>)>>,
    next: Cell<u32>,
    /// The task holding the core's quit listener started; it lasts as the
    /// VM does.
    started: Cell<bool>,
}

impl Local for Quits {
    fn close(&self) {
        self.listeners.borrow_mut().clear();
    }
}

/// Holds the core's quit listener for the VM's: as the app quits, it calls
/// each of them and waits for what they return.
async fn hold(quits: Weak<Quits>) {
    let _subscription = crate::element::desktop::add_quit_listener(Rc::new(move || {
        let calls: Vec<_> = quits
            .upgrade()
            .map(|quits| {
                let listeners = quits.listeners.borrow();
                listeners
                    .iter()
                    .map(|(_, listener)| listener.call_async(Payload::Quit))
                    .collect()
            })
            .unwrap_or_default();
        Box::pin(async move {
            for call in calls {
                if let Err(message) = call.await {
                    log::error!(target: "js", "a quit listener failed: {message}");
                }
            }
        })
    }));
    std::future::pending::<()>().await;
}
