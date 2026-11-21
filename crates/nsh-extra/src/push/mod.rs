//! `Push` (feature `push`): remote notifications. The app registers the
//! device ([`register`]) and gets the token a push service addresses it
//! with; the notifications that arrive while it is in front, the
//! background deliveries that wake it and the presses of notifications come
//! as events, in JS through `Push.on` and in Rust through [`on_event`]. The
//! module is generic: it works with any push service, and a push service's
//! SDK binding builds on its Rust API (the token as bytes, the launch
//! options as the system passed them).
//!
//! Showing notifications needs the user's permission, which the app asks
//! with `Notifications.requestPermission` (`nsh`'s); background deliveries
//! need none. A notification the app removes by its id goes with
//! `Notifications.close`; [`close_thread`] removes those grouped in a
//! thread.
//!
//! On iOS (`ios.rs`) the module observes a standalone app's application
//! delegate (`nsh_platform_ios::on_application_event`) and is the handler
//! of the notifications NSH did not show
//! (`nsh_platform_apple::notifications::set_foreign_handler`), both set up
//! in the crate's `#[native_module]` entry, before the app launches, so the
//! press that launches the app is not lost. Elsewhere (Android for now, the
//! desktops, the web, headless runs, Mac Catalyst, an app that embeds NSH
//! in its own UIKit app and keeps its own delegate) [`available`] is false
//! and the functions that ask the system fail with [`UNAVAILABLE`].
//!
//! The packaged app needs the `remote-notification` background mode, which
//! `nsh` adds from this crate's `[package.metadata.nsh.features.push]`.
//! The `aps-environment` entitlement comes from the provisioning profile
//! the app is signed with, which grants it with the environment the profile
//! is for.

#[cfg(target_os = "ios")]
mod ios;

use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::{Rc, Weak},
    sync::{Arc, Mutex, MutexGuard, RwLock},
    time::Duration,
};

use nsh_core::{
    JsonValue,
    element::{Answer, CallbackRet, JsHost, JsIn, JsOwned, Local, PropError, TsTypes, codes},
    prelude::*,
    time::Instant,
};
use tokio::sync::{mpsc, oneshot};

#[cfg(target_os = "ios")]
use self::ios as platform;
#[cfg(target_os = "ios")]
pub use self::ios::launch_options;

/// The code of a request where there are no remote notifications.
pub const UNAVAILABLE: &str = codes::UNAVAILABLE;

/// How long a background delivery waits for a JS listener when none
/// listens yet (the app was launched for it and its JS is starting): after
/// that the system hears it handled with nothing new.
pub const BACKGROUND_WAIT: Duration = Duration::from_secs(5);

/// How long the JS listeners of a background delivery have to settle: the
/// system gives the app about 30 seconds.
pub const BACKGROUND_LIMIT: Duration = Duration::from_secs(25);

/// How many presses wait for a JS listener; older ones are dropped.
const MAX_PENDING_PRESSES: usize = 16;

/// The token a push service addresses this device with.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceToken(Vec<u8>);

impl DeviceToken {
    /// The token of `bytes`, as the system gave them.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Its bytes, for an SDK that takes them as they are.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Its bytes in lowercase hexadecimal, as push services take it.
    pub fn to_hex(&self) -> String {
        use fmt::Write;
        self.0.iter().fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
    }
}

impl fmt::Display for DeviceToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// A remote notification, or one that other native code of the app
/// scheduled: any notification `nsh`'s `Notifications` did not show.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct RemoteNotification {
    /// Its request's identifier, which `Notifications.close` removes it
    /// by; `""` for a background delivery, which has none.
    pub id: String,
    /// The thread it is grouped in, `""` for none.
    pub thread_id: String,
    /// What the push service sent (on iOS the `aps` dictionary included)
    /// as JSON.
    pub payload: JsonValue,
}

impl RemoteNotification {
    /// A notification with the request identifier `id`, in `thread_id`.
    pub fn new(id: impl Into<String>, thread_id: impl Into<String>, payload: JsonValue) -> Self {
        Self {
            id: id.into(),
            thread_id: thread_id.into(),
            payload,
        }
    }

    /// A background delivery of `payload`: no request, its thread the one
    /// the payload names (`aps.thread-id`).
    pub fn background(payload: JsonValue) -> Self {
        let thread_id = payload
            .get("aps")
            .and_then(|aps| aps.get("thread-id"))
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_owned();
        Self {
            id: String::new(),
            thread_id,
            payload,
        }
    }
}

/// How a notification that arrives while the app is in front shows
/// ([`set_foreground_presentation`]). By default it shows as it does while
/// the app is behind, as the notifications `Notifications` shows do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ForegroundPresentation {
    /// It shows as a banner.
    pub banner: bool,
    /// It stays in the notification list.
    pub list: bool,
    /// Its sound plays.
    pub sound: bool,
    /// Its badge applies to the app's icon.
    pub badge: bool,
}

impl ForegroundPresentation {
    /// Shown in every way: what the app gets until it sets another.
    pub const SHOWN: Self = Self {
        banner: true,
        list: true,
        sound: true,
        badge: true,
    };

    /// Shows it as a banner ([`Self::banner`]).
    pub fn banner(mut self, banner: bool) -> Self {
        self.banner = banner;
        self
    }

    /// Keeps it in the notification list ([`Self::list`]).
    pub fn list(mut self, list: bool) -> Self {
        self.list = list;
        self
    }

    /// Plays its sound ([`Self::sound`]).
    pub fn sound(mut self, sound: bool) -> Self {
        self.sound = sound;
        self
    }

    /// Applies its badge ([`Self::badge`]).
    pub fn badge(mut self, badge: bool) -> Self {
        self.badge = badge;
        self
    }
}

impl Default for ForegroundPresentation {
    fn default() -> Self {
        Self::SHOWN
    }
}

/// How the work a background delivery started turned out, for the system
/// to schedule the app's later background launches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
#[non_exhaustive]
pub enum BackgroundResult {
    /// New data was downloaded.
    NewData,
    /// There was nothing new.
    #[default]
    NoData,
    /// The work failed.
    Failed,
}

impl BackgroundResult {
    /// The result of two pieces of work: new data over a failure, a
    /// failure over nothing new.
    pub fn merge(self, other: Self) -> Self {
        let rank = |result: Self| match result {
            Self::NoData => 0,
            Self::Failed => 1,
            Self::NewData => 2,
        };
        if rank(other) > rank(self) {
            other
        } else {
            self
        }
    }
}

/// The completion of a background delivery ([`PushEvent::Background`]).
///
/// Clone it to keep it across asynchronous work; it can move to any
/// thread. The system hears once, when the last clone is dropped, with the
/// most significant result reported ([`BackgroundCompletion::set_result`]),
/// so code that does no work need not touch it.
#[derive(Clone)]
pub struct BackgroundCompletion(Arc<Completion>);

/// Tells the system how a background delivery's work turned out.
type Finish = Box<dyn FnOnce(BackgroundResult) + Send>;

struct Completion {
    result: Mutex<BackgroundResult>,
    finish: Mutex<Option<Finish>>,
}

impl BackgroundCompletion {
    /// A completion that calls `finish` with the result once the last
    /// clone is dropped: the platform's, or a test's.
    pub fn new(finish: impl FnOnce(BackgroundResult) + Send + 'static) -> Self {
        Self(Arc::new(Completion {
            result: Mutex::new(BackgroundResult::default()),
            finish: Mutex::new(Some(Box::new(finish))),
        }))
    }

    /// Reports how this piece of work turned out. Results combine: new data
    /// wins over a failure, a failure over nothing new
    /// ([`BackgroundResult::NoData`], the result when nobody reports one).
    pub fn set_result(&self, result: BackgroundResult) {
        let mut current = self.0.result.lock().unwrap_or_else(|e| e.into_inner());
        *current = current.merge(result);
    }
}

impl fmt::Debug for BackgroundCompletion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let result = *self.0.result.lock().unwrap_or_else(|e| e.into_inner());
        f.debug_struct("BackgroundCompletion")
            .field("result", &result)
            .finish()
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        let result = *self.result.get_mut().unwrap_or_else(|e| e.into_inner());
        if let Some(finish) = self
            .finish
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            finish(result);
        }
    }
}

/// Why a request of the module failed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PushError {
    /// A stable code the app can branch on ([`UNAVAILABLE`]); `None` for
    /// other failures, such as the system's refusal to register.
    pub code: Option<String>,
    /// What went wrong, in words.
    pub message: String,
}

impl PushError {
    /// A failure with `message` and no code.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
        }
    }

    /// There are no remote notifications here ([`UNAVAILABLE`]).
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: Some(UNAVAILABLE.into()),
            message: message.into(),
        }
    }

    /// How JS gets it: an `Error` with its `code`.
    fn to_js(&self) -> JsError {
        match &self.code {
            Some(code) => JsError::Error(self.message.clone()).with_code(code),
            None => JsError::Error(self.message.clone()),
        }
    }
}

impl fmt::Display for PushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PushError {}

/// What the module reports to Rust code ([`on_event`]), on the main
/// thread.
#[derive(Debug)]
#[non_exhaustive]
pub enum PushEvent<'a> {
    /// The system registered the device. Every registration reports its
    /// token, the same one again included.
    Token(&'a DeviceToken),
    /// The system could not register the device.
    RegistrationFailed(&'a PushError),
    /// A notification arrived while the app is in front; it shows as
    /// [`foreground_presentation`] says.
    Notification(&'a RemoteNotification),
    /// A delivery woke the app or reached it in front (on iOS one with
    /// `content-available`): work that keeps the completion reports when
    /// it is done.
    Background {
        /// What was delivered.
        notification: &'a RemoteNotification,
        /// Tells the system when the work is done.
        completion: &'a BackgroundCompletion,
    },
    /// The user pressed a notification, which may have launched the app.
    Press(&'a RemoteNotification),
}

type Observer = Arc<dyn Fn(&PushEvent<'_>) + Send + Sync>;

/// The Rust observers ([`on_event`]).
static OBSERVERS: RwLock<Vec<Observer>> = RwLock::new(Vec::new());

/// Adds an observer of the module's events ([`PushEvent`]), for native
/// code such as a push service's SDK binding: it hands the token to the
/// SDK and the notifications to its handlers. The events come on the main
/// thread, every one, whether JS listens or not. Add it from a
/// `#[native_module]` entry, before the app launches, to hear the press
/// that launches it.
pub fn on_event(observer: impl Fn(&PushEvent<'_>) + Send + Sync + 'static) {
    OBSERVERS
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .push(Arc::new(observer));
}

/// Reports `event` to the Rust observers.
fn emit(event: &PushEvent<'_>) {
    // A copy of the list, so that an observer may add another.
    let observers = OBSERVERS.read().unwrap_or_else(|e| e.into_inner()).clone();
    for observer in observers {
        observer(event);
    }
}

/// Whether remote notifications work here: in a standalone iOS app
/// (`nsh_platform_ios::run_app`), whose application delegate the module
/// observes. In a task.
pub async fn available() -> bool {
    hub().launched
}

/// Registers the device for remote notifications and resolves with its
/// token, which the app hands to its push service; the system answers
/// once it can reach its servers. Registering does not ask the user:
/// showing notifications needs the permission `Notifications` asks for,
/// background deliveries do not. Register at every launch, as the token
/// can change ([`PushEvent::Token`] and `Push.on("token")` report a new
/// one). In a task.
pub async fn register() -> Result<DeviceToken, PushError> {
    if !hub().launched {
        return Err(PushError::unavailable(
            "remote notifications need a standalone iOS app",
        ));
    }
    let (answer, token) = oneshot::channel();
    hub().registering.push(answer);
    platform::register().await?;
    match token.await {
        Ok(token) => token,
        Err(_) => Err(PushError::failed("the registration went away unanswered")),
    }
}

/// The token of this device's last registration in this run, if any.
pub fn device_token() -> Option<DeviceToken> {
    hub().token.clone()
}

/// How a notification that arrives while the app is in front shows.
pub fn foreground_presentation() -> ForegroundPresentation {
    hub().presentation
}

/// Shows the notifications that arrive while the app is in front as
/// `presentation` says, from now on: the system asks at once, so the
/// decision is made before they come (turn the banner off while the
/// conversation they would announce is open).
pub fn set_foreground_presentation(presentation: ForegroundPresentation) {
    hub().presentation = presentation;
}

/// Removes the delivered notifications grouped in `thread_id`, resolving
/// with how many there were. In a task.
pub async fn close_thread(thread_id: String) -> Result<usize, PushError> {
    platform::close_thread(thread_id).await
}

/// The identifiers of the notifications of `delivered` (`(id, thread)`
/// pairs) grouped in `thread_id`.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub(crate) fn in_thread<'a>(
    delivered: impl IntoIterator<Item = (&'a str, &'a str)>,
    thread_id: &str,
) -> Vec<String> {
    delivered
        .into_iter()
        .filter(|(_, thread)| *thread == thread_id)
        .map(|(id, _)| id.to_owned())
        .collect()
}

/// Where NSH has no remote notifications: Android for now, the desktops,
/// the web, headless runs.
#[cfg(not(target_os = "ios"))]
mod platform {
    use super::PushError;

    pub(super) fn install() {}

    pub(super) async fn register() -> Result<(), PushError> {
        Err(PushError::unavailable(
            "there are no remote notifications here",
        ))
    }

    pub(super) async fn close_thread(_: String) -> Result<usize, PushError> {
        Err(PushError::unavailable(
            "there are no remote notifications here",
        ))
    }
}

/// Sets the module up with the platform: called by the crate's
/// `#[native_module]` entry, on the main thread before the app launches.
pub(crate) fn install() {
    platform::install();
}

// --- What the platform reports ----------------------------------------------

/// The app launched with the module observing its application delegate.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn launched() {
    hub().launched = true;
}

/// The system registered the device with `token`.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn registered(token: DeviceToken) {
    let answers = hub().registered(&token);
    for answer in answers {
        let _ = answer.send(Ok(token.clone()));
    }
    emit(&PushEvent::Token(&token));
}

/// The system could not register the device.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn registration_failed(message: &str) {
    let error = PushError::failed(format!("the device was not registered: {message}"));
    log::warn!("push: {error}");
    let answers = std::mem::take(&mut hub().registering);
    for answer in answers {
        let _ = answer.send(Err(error.clone()));
    }
    emit(&PushEvent::RegistrationFailed(&error));
}

/// `notification` arrived while the app is in front: how it shows.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn arrived(notification: RemoteNotification) -> ForegroundPresentation {
    emit(&PushEvent::Notification(&notification));
    let mut hub = hub();
    let presentation = hub.presentation;
    let dropped = hub.dispatch(Delivery::Notification(notification), now());
    drop(hub);
    drop(dropped);
    presentation
}

/// A background delivery of `notification`, which `completion` reports.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn background(notification: RemoteNotification, completion: BackgroundCompletion) {
    emit(&PushEvent::Background {
        notification: &notification,
        completion: &completion,
    });
    let dropped = hub().dispatch(Delivery::Background(notification, completion), now());
    // Outside the hub's lock: the completion may tell the system now.
    drop(dropped);
}

/// The user pressed `notification`.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn pressed(notification: RemoteNotification) {
    emit(&PushEvent::Press(&notification));
    let dropped = hub().dispatch(Delivery::Press(notification), now());
    drop(dropped);
}

/// Completes the background deliveries that waited for a JS listener
/// longer than [`BACKGROUND_WAIT`].
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn expire_pending() {
    let expired = hub().expire(now());
    if !expired.is_empty() {
        log::info!(
            "push: {} background deliveries had no listener",
            expired.len()
        );
    }
    drop(expired);
}

fn now() -> Instant {
    nsh_core::time::now()
}

// --- The hub: what the platform reported, on its way to JS ------------------

/// The events JS listens to, as bits of a subscriber's interests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Token = 1,
    Notification = 2,
    Background = 4,
    Press = 8,
}

/// An event on its way to a VM.
#[derive(Debug)]
enum Delivery {
    Token(DeviceToken),
    Notification(RemoteNotification),
    Background(RemoteNotification, BackgroundCompletion),
    Press(RemoteNotification),
}

impl Delivery {
    fn kind(&self) -> Kind {
        match self {
            Self::Token(_) => Kind::Token,
            Self::Notification(_) => Kind::Notification,
            Self::Background(..) => Kind::Background,
            Self::Press(_) => Kind::Press,
        }
    }

    /// Whether it waits for a listener when none listens: a press (the one
    /// that launched the app comes before its JS runs) and a background
    /// delivery (the app was launched for it), not a token (`token()` reads
    /// it) or a notification in front (it showed).
    fn waits(&self) -> bool {
        matches!(self, Self::Background(..) | Self::Press(_))
    }

    /// A copy for one more VM: the completion of a background delivery is
    /// shared, so the system hears once every VM is done.
    fn copy(&self) -> Self {
        match self {
            Self::Token(token) => Self::Token(token.clone()),
            Self::Notification(n) => Self::Notification(n.clone()),
            Self::Background(n, completion) => Self::Background(n.clone(), completion.clone()),
            Self::Press(n) => Self::Press(n.clone()),
        }
    }
}

/// A VM listening to the module's events (`Push.on`).
struct Subscriber {
    id: u64,
    /// The [`Kind`]s it has listeners of.
    kinds: u8,
    sender: mpsc::UnboundedSender<Delivery>,
}

/// An event no VM listened to yet, and when it came.
#[derive(Debug)]
struct Pending {
    delivery: Delivery,
    at: Instant,
}

/// The module's state, process-wide: the platform reports on the main
/// thread and every VM's `Push` reads it. Nothing dropped under its lock
/// may take it again (a [`BackgroundCompletion`] tells the system without
/// it): what can be is handed back to drop outside.
struct Hub {
    /// The module observes a standalone app's delegate ([`available`]).
    launched: bool,
    /// The last token of this run.
    token: Option<DeviceToken>,
    /// The registrations waiting for the system's answer.
    registering: Vec<oneshot::Sender<Result<DeviceToken, PushError>>>,
    presentation: ForegroundPresentation,
    subscribers: Vec<Subscriber>,
    next_subscriber: u64,
    /// Presses and background deliveries no VM listened to yet, in order.
    pending: Vec<Pending>,
}

static HUB: Mutex<Hub> = Mutex::new(Hub::new());

fn hub() -> MutexGuard<'static, Hub> {
    HUB.lock().unwrap_or_else(|e| e.into_inner())
}

impl Hub {
    const fn new() -> Self {
        Self {
            launched: false,
            token: None,
            registering: Vec::new(),
            presentation: ForegroundPresentation::SHOWN,
            subscribers: Vec::new(),
            next_subscriber: 0,
            pending: Vec::new(),
        }
    }

    /// Records the token of a registration: the registrations waiting for
    /// it, to answer; the VMs hear it when it differs from the last one.
    fn registered(
        &mut self,
        token: &DeviceToken,
    ) -> Vec<oneshot::Sender<Result<DeviceToken, PushError>>> {
        if self.token.as_ref() != Some(token) {
            self.token = Some(token.clone());
            drop(self.dispatch(Delivery::Token(token.clone()), now()));
        }
        std::mem::take(&mut self.registering)
    }

    /// Hands `delivery` to the VMs listening to its kind, or keeps it for
    /// the first that will when it waits ([`Delivery::waits`]). What it
    /// returns is dropped outside the lock.
    #[must_use]
    fn dispatch(&mut self, delivery: Delivery, at: Instant) -> Vec<Delivery> {
        let kind = delivery.kind() as u8;
        let mut dropped = Vec::new();
        let mut taken = false;
        self.subscribers.retain(|subscriber| {
            if subscriber.kinds & kind == 0 {
                return true;
            }
            match subscriber.sender.send(delivery.copy()) {
                Ok(()) => {
                    taken = true;
                    true
                }
                // Its VM went.
                Err(mpsc::error::SendError(copy)) => {
                    dropped.push(copy);
                    false
                }
            }
        });
        if !taken && delivery.waits() {
            if matches!(delivery, Delivery::Press(_)) {
                let presses = self
                    .pending
                    .iter()
                    .filter(|p| matches!(p.delivery, Delivery::Press(_)))
                    .count();
                if presses == MAX_PENDING_PRESSES
                    && let Some(oldest) = self
                        .pending
                        .iter()
                        .position(|p| matches!(p.delivery, Delivery::Press(_)))
                {
                    dropped.push(self.pending.remove(oldest).delivery);
                }
            }
            self.pending.push(Pending { delivery, at });
        } else {
            dropped.push(delivery);
        }
        dropped
    }

    /// A new VM's subscription, which listens to nothing yet.
    fn subscribe(&mut self) -> (u64, mpsc::UnboundedReceiver<Delivery>) {
        let id = self.next_subscriber;
        self.next_subscriber += 1;
        let (sender, receiver) = mpsc::unbounded_channel();
        self.subscribers.push(Subscriber {
            id,
            kinds: 0,
            sender,
        });
        (id, receiver)
    }

    /// Subscriber `id` listens to `kind` now: it takes what waited for it.
    fn listen(&mut self, id: u64, kind: Kind) {
        let Some(subscriber) = self.subscribers.iter_mut().find(|s| s.id == id) else {
            return;
        };
        subscriber.kinds |= kind as u8;
        let (waited, rest) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| p.delivery.kind() == kind);
        self.pending = rest;
        for Pending { delivery, .. } in waited {
            // A VM that went drops it later, outside the lock.
            let _ = subscriber.sender.send(delivery);
        }
    }

    /// Subscriber `id` no longer listens to `kind`.
    fn unlisten(&mut self, id: u64, kind: Kind) {
        if let Some(subscriber) = self.subscribers.iter_mut().find(|s| s.id == id) {
            subscriber.kinds &= !(kind as u8);
        }
    }

    /// Subscriber `id`'s VM went.
    fn unsubscribe(&mut self, id: u64) {
        self.subscribers.retain(|s| s.id != id);
    }

    /// Takes the background deliveries that waited for a listener longer
    /// than [`BACKGROUND_WAIT`] at `now`, to drop outside the lock.
    #[must_use]
    fn expire(&mut self, now: Instant) -> Vec<Delivery> {
        let (expired, rest) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| {
                matches!(p.delivery, Delivery::Background(..))
                    && now.saturating_duration_since(p.at) >= BACKGROUND_WAIT
            });
        self.pending = rest;
        expired.into_iter().map(|p: Pending| p.delivery).collect()
    }
}

// --- JS ---------------------------------------------------------------------

/// Remote notifications.
#[module]
pub(crate) struct Push;

#[module]
impl Push {
    /// Resolves with whether remote notifications work here: `true` in a
    /// standalone iOS app, `false` on Android (for now), the desktops, the
    /// web, Mac Catalyst and in an app that embeds NSH in its own iOS app,
    /// where `register` and `closeThread` reject with
    /// `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`.
    pub(crate) async fn available() -> bool {
        crate::push::available().await
    }

    /// Registers the device for remote notifications and resolves with its
    /// token in hexadecimal, which the app hands to its push service. The
    /// system answers once it can reach its servers. It does not ask the
    /// user: showing notifications needs `Notifications.requestPermission`,
    /// background deliveries do not. Register at every launch: the token
    /// can change, and `on("token")` reports a new one. Rejects with the
    /// system's reason when it refuses (an app signed without the
    /// `aps-environment` entitlement), and with
    /// `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` where `available()` is false.
    pub(crate) async fn register() -> Result<String, JsError> {
        match crate::push::register().await {
            Ok(token) => Ok(token.to_hex()),
            Err(error) => Err(error.to_js()),
        }
    }

    /// The token of the device's last registration in this run, in
    /// hexadecimal; `null` before one.
    pub(crate) fn token() -> Option<String> {
        crate::push::device_token().map(|token| token.to_hex())
    }

    /// How a notification that arrives while the app is in front shows.
    pub(crate) fn foreground_presentation() -> PushPresentation {
        crate::push::foreground_presentation().into()
    }

    /// Shows the notifications that arrive while the app is in front as
    /// `presentation` says, from now on; what it leaves out shows (all of
    /// it, until the app sets another). The system asks as they arrive,
    /// without waiting for JS: set it before (turn `banner` off while the
    /// conversation they would announce is open). They are reported to
    /// `on("notification")` either way.
    pub(crate) fn set_foreground_presentation(presentation: PushPresentationOptions) {
        crate::push::set_foreground_presentation(presentation.presentation());
    }

    /// Removes the delivered notifications grouped in the thread
    /// `threadId` (the `thread-id` the push service sent), resolving with
    /// how many there were. `Notifications.close(id)` removes one by its
    /// `id`. Rejects with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` where
    /// `available()` is false.
    pub(crate) async fn close_thread(thread_id: String) -> Result<f64, JsError> {
        match crate::push::close_thread(thread_id).await {
            Ok(count) => Ok(count as f64),
            Err(error) => Err(error.to_js()),
        }
    }

    /// Listens to `event`. `token`: a registration gave a token that
    /// differs from the last one of this run (the first included).
    /// `notification`: a notification arrived while the app is in front;
    /// it shows as `setForegroundPresentation` says. `press`: the user
    /// pressed a notification (the app comes to the front); presses that
    /// came while nothing listened, the one that launched the app
    /// included, go to the next listener (at most 16). `background`: a
    /// delivery woke the app or reached it in front (on iOS one with
    /// `content-available`); the system waits for the listeners, so return
    /// a promise for the work and what it gave (`"newData"`, `"noData"`
    /// when nothing is returned, `"failed"`, as a thrown error is), within
    /// 25 seconds. One that came while nothing listened waits 5 seconds
    /// for a listener. These are the notifications `Notifications` did not
    /// show: remote ones and those other native code scheduled. A payload
    /// is untrusted input. Returns a function that stops listening.
    #[overload(fn(event: PushTokenEvent, #[args(token)] listener: Callback<PushToken>) -> Stop)]
    #[overload(fn(event: PushNotificationEvent, #[args(notification)] listener: Callback<PushNotification>) -> Stop)]
    #[overload(fn(event: PushPressEvent, #[args(press)] listener: Callback<PushNotification>) -> Stop)]
    #[overload(fn(event: PushBackgroundEvent, #[args(background)] listener: Callback<PushBackground, BackgroundReturn>) -> Stop)]
    pub(crate) fn on(
        event: PushEventName,
        #[args(payload)] listener: Callback<Payload, BackgroundReturn>,
        sys: System,
        cx: CallCx<'_>,
    ) -> Stop {
        let kind = event.kind();
        let listeners = sys.locals.get::<Listeners>();
        let id = listeners.next.get();
        listeners.next.set(id.wrapping_add(1));
        let first = listeners.of(kind).borrow().is_empty();
        listeners.of(kind).borrow_mut().push((id, listener));
        let subscriber = listeners.subscriber(&cx);
        if first {
            hub().listen(subscriber, kind);
        }
        let listeners = Rc::downgrade(&listeners);
        Stop::new(move || {
            let Some(listeners) = listeners.upgrade() else {
                return;
            };
            let mut list = listeners.of(kind).borrow_mut();
            list.retain(|(i, _)| *i != id);
            if list.is_empty()
                && let Some(subscriber) = listeners.subscriber.get()
            {
                hub().unlisten(subscriber, kind);
            }
        })
    }
}

/// How a notification in front shows (`foregroundPresentation()`).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub(crate) struct PushPresentation {
    /// It shows as a banner.
    banner: bool,
    /// It stays in the notification list.
    list: bool,
    /// Its sound plays.
    sound: bool,
    /// Its badge applies to the app's icon.
    badge: bool,
}

impl From<ForegroundPresentation> for PushPresentation {
    fn from(p: ForegroundPresentation) -> Self {
        Self {
            banner: p.banner,
            list: p.list,
            sound: p.sound,
            badge: p.badge,
        }
    }
}

/// How notifications in front show (`setForegroundPresentation`); what is
/// left out shows.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[prop(exact)]
pub(crate) struct PushPresentationOptions {
    /// It shows as a banner (true when not given).
    pub(crate) banner: Option<bool>,
    /// It stays in the notification list (true when not given).
    pub(crate) list: Option<bool>,
    /// Its sound plays (true when not given).
    pub(crate) sound: Option<bool>,
    /// Its badge applies to the app's icon (true when not given).
    pub(crate) badge: Option<bool>,
}

impl PushPresentationOptions {
    fn presentation(self) -> ForegroundPresentation {
        ForegroundPresentation::SHOWN
            .banner(self.banner.unwrap_or(true))
            .list(self.list.unwrap_or(true))
            .sound(self.sound.unwrap_or(true))
            .badge(self.badge.unwrap_or(true))
    }
}

/// `Push.on`'s events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub(crate) enum PushEventName {
    /// A registration gave a new token.
    #[default]
    Token,
    /// A notification arrived in front.
    Notification,
    /// The user pressed a notification.
    Press,
    /// A background delivery.
    Background,
}

impl PushEventName {
    fn kind(self) -> Kind {
        match self {
            Self::Token => Kind::Token,
            Self::Notification => Kind::Notification,
            Self::Press => Kind::Press,
            Self::Background => Kind::Background,
        }
    }
}

/// The event a token comes with (`on`'s first overload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub(crate) enum PushTokenEvent {
    Token,
}

/// The event a notification in front comes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub(crate) enum PushNotificationEvent {
    Notification,
}

/// The event a press comes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub(crate) enum PushPressEvent {
    Press,
}

/// The event a background delivery comes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub(crate) enum PushBackgroundEvent {
    Background,
}

/// A new token (the `token` event).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub(crate) struct PushToken {
    /// The token, in hexadecimal.
    token: String,
}

/// A notification that arrived in front or was pressed (the
/// `notification` and `press` events).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub(crate) struct PushNotification {
    /// Its request's identifier, which `Notifications.close` removes it
    /// by.
    id: String,
    /// The thread it is grouped in, `""` for none.
    thread_id: String,
    /// What the push service sent (on iOS the `aps` object included).
    payload: JsonValue,
}

impl From<RemoteNotification> for PushNotification {
    fn from(n: RemoteNotification) -> Self {
        Self {
            id: n.id,
            thread_id: n.thread_id,
            payload: n.payload,
        }
    }
}

/// A background delivery (the `background` event).
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub(crate) struct PushBackground {
    /// The thread its payload names, `""` for none.
    thread_id: String,
    /// What the push service sent (on iOS the `aps` object included).
    payload: JsonValue,
}

impl From<RemoteNotification> for PushBackground {
    fn from(n: RemoteNotification) -> Self {
        Self {
            thread_id: n.thread_id,
            payload: n.payload,
        }
    }
}

/// What a listener gets.
#[derive(Debug, Clone)]
pub(crate) enum Payload {
    Token(PushToken),
    Notification(PushNotification),
    Background(PushBackground),
}

impl JsValue for Payload {
    fn ts_type(types: &mut TsTypes) -> String {
        format!(
            "{} | {} | {}",
            <PushToken as JsValue>::ts_type(types),
            <PushNotification as JsValue>::ts_type(types),
            <PushBackground as JsValue>::ts_type(types)
        )
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        Err(value.expected("no value (a payload goes to JS only)"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self {
            Self::Token(token) => token.to_js(host),
            Self::Notification(notification) => notification.to_js(host),
            Self::Background(background) => background.to_js(host),
        }
    }
}

/// What a `background` listener returns: how its work turned out, or a
/// promise of it; nothing is `"noData"`.
pub(crate) struct BackgroundReturn(Option<BackgroundResult>);

impl CallbackRet for BackgroundReturn {
    fn ts_type(types: &mut TsTypes) -> String {
        let result = <BackgroundResult as JsValue>::ts_type(types);
        format!("void | {result} | Promise<void | {result}>")
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if value.is_nullish() {
            return Ok(Self(None));
        }
        <BackgroundResult as JsValue>::from_js(value).map(|result| Self(Some(result)))
    }
}

type Listener = (u32, Callback<Payload, BackgroundReturn>);

/// The VM's `Push.on` listeners and its subscription to the hub.
#[derive(Default)]
struct Listeners {
    token: RefCell<Vec<Listener>>,
    notification: RefCell<Vec<Listener>>,
    background: RefCell<Vec<Listener>>,
    press: RefCell<Vec<Listener>>,
    next: Cell<u32>,
    subscriber: Cell<Option<u64>>,
}

impl Listeners {
    fn of(&self, kind: Kind) -> &RefCell<Vec<Listener>> {
        match kind {
            Kind::Token => &self.token,
            Kind::Notification => &self.notification,
            Kind::Background => &self.background,
            Kind::Press => &self.press,
        }
    }

    /// The VM's subscription to the hub, made with the task that delivers
    /// what it hands over the first time.
    fn subscriber(self: &Rc<Self>, cx: &CallCx<'_>) -> u64 {
        if let Some(id) = self.subscriber.get() {
            return id;
        }
        let (id, receiver) = hub().subscribe();
        self.subscriber.set(Some(id));
        cx.start(deliver(Rc::downgrade(self), receiver));
        id
    }
}

impl Local for Listeners {
    fn close(&self) {
        for kind in [
            Kind::Token,
            Kind::Notification,
            Kind::Background,
            Kind::Press,
        ] {
            self.of(kind).borrow_mut().clear();
        }
        if let Some(id) = self.subscriber.take() {
            hub().unsubscribe(id);
        }
    }
}

/// Posts what the hub hands over to the VM's listeners, while the VM
/// lasts.
async fn deliver(listeners: Weak<Listeners>, mut receiver: mpsc::UnboundedReceiver<Delivery>) {
    while let Some(delivery) = receiver.recv().await {
        let Some(listeners) = listeners.upgrade() else {
            return;
        };
        let list = listeners.of(delivery.kind()).borrow().clone();
        if list.is_empty() {
            // The last listener stopped while it was on its way: the next
            // one, here or in another VM, gets it if it waits.
            let dropped = hub().dispatch(delivery, now());
            drop(dropped);
            continue;
        }
        let listeners = list.into_iter().map(|(_, listener)| listener);
        match delivery {
            Delivery::Token(token) => {
                let token = PushToken {
                    token: token.to_hex(),
                };
                for listener in listeners {
                    listener.post(Payload::Token(token.clone()));
                }
            }
            Delivery::Notification(n) | Delivery::Press(n) => {
                let notification = PushNotification::from(n);
                for listener in listeners {
                    listener.post(Payload::Notification(notification.clone()));
                }
            }
            Delivery::Background(n, completion) => {
                let background = PushBackground::from(n);
                let answers: Vec<_> = listeners
                    .map(|listener| listener.call_async(Payload::Background(background.clone())))
                    .collect();
                drop(spawn(settle(answers, completion)));
            }
        }
    }
}

/// Reports the background work of the listeners' `answers` to
/// `completion` once they settle, or [`BACKGROUND_LIMIT`] has passed.
async fn settle(
    answers: Vec<Answer<Result<BackgroundReturn, String>>>,
    completion: BackgroundCompletion,
) {
    let deadline = now() + BACKGROUND_LIMIT;
    for answer in answers {
        let result = match within(answer, deadline).await {
            Some(Ok(BackgroundReturn(result))) => result.unwrap_or_default(),
            Some(Err(error)) => {
                log::warn!("push: a background listener failed: {error}");
                BackgroundResult::Failed
            }
            None => {
                log::warn!("push: a background listener did not settle in time");
                BackgroundResult::Failed
            }
        };
        completion.set_result(result);
    }
}

/// `future`'s output, or `None` when `deadline` comes first.
async fn within<T>(future: impl std::future::Future<Output = T>, deadline: Instant) -> Option<T> {
    let mut future = std::pin::pin!(future);
    let mut timeout = std::pin::pin!(timer_until(deadline));
    std::future::poll_fn(|cx| {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(cx) {
            return std::task::Poll::Ready(Some(output));
        }
        timeout.as_mut().poll(cx).map(|()| None)
    })
    .await
}

#[cfg(test)]
mod tests;
