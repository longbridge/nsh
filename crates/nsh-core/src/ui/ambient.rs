//! What components that run once read of the surface and ask of the
//! platform, as free functions (TS: `nsh`'s accessors and modules).
//!
//! The environment, the theme, globals and screens' insets are signals the
//! core writes: reading one in a binding subscribes it, so a change reaches
//! only what read it. Services (dialogs, the clipboard, toasts) are actions
//! for the core; tasks and timers belong to the current owner and end with
//! it.

use std::{
    any::{Any, TypeId},
    future::Future,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use rustc_hash::FxHashMap;

use super::{
    AdaptiveColor, Environment, Memo, Owner, Signal, SignalEquals, Theme, cleanup, computed,
    reactive::with_data,
    signal_with,
    status_bar::{SharedStatusBars, StatusBarEntry, StatusBars},
    task::{self, Action, Answer, DialogAnswer, Executor, ImageSize},
};
use crate::{
    Command, Dialog, EdgeInsets, LayoutAnimation, NodeId, Size, StatusBarSetting,
    canvas::CanvasFont,
    platform::{Appearance, Idiom, Locale, Os},
    style::Color,
    time::Instant,
};

/// How text is measured on one line, in a canvas font.
pub(crate) type MeasureFn = Rc<dyn Fn(&CanvasFont, &str) -> f32>;

/// What the core gives components that run once.
#[derive(Default)]
pub(crate) struct Ambient {
    executor: Option<Rc<Executor>>,
    /// The status bar's layers.
    status_bars: Option<SharedStatusBars>,
    env: Option<EnvSignals>,
    theme: Option<Signal<Arc<Theme>>>,
    globals: FxHashMap<TypeId, Box<dyn Any>>,
    /// The insets of the screens or modals components are on, by host.
    insets: FxHashMap<NodeId, Signal<EdgeInsets>>,
    measure: Option<MeasureFn>,
}

impl Ambient {
    /// The hosts whose insets components read.
    pub(crate) fn inset_hosts(&self) -> Vec<NodeId> {
        self.insets.keys().copied().collect()
    }
}

/// The environment as a signal, and a memo per part (a change notifies only
/// the readers of the parts that changed).
struct EnvSignals {
    env: Signal<Environment>,
    window: Memo<Size>,
    safe_area: Memo<EdgeInsets>,
    keyboard_height: Memo<f32>,
    keyboard_duration: Memo<f32>,
    dark: Memo<bool>,
    font_scale: Memo<f32>,
    reduce_motion: Memo<bool>,
    screen_reader: Memo<bool>,
    rtl: Memo<bool>,
    pixel_ratio: Memo<f32>,
    locale: Memo<Locale>,
}

impl EnvSignals {
    fn new(env: Environment) -> Self {
        Owner::detached().run(|| {
            let signal = super::signal(env);
            macro_rules! part {
                ($field:ident) => {
                    computed(move || signal.with(|e| e.$field))
                };
            }
            Self {
                env: signal,
                window: part!(window),
                safe_area: part!(safe_area),
                keyboard_height: part!(keyboard_height),
                keyboard_duration: part!(keyboard_duration),
                dark: part!(dark),
                font_scale: part!(font_scale),
                reduce_motion: part!(reduce_motion),
                screen_reader: part!(screen_reader),
                rtl: part!(rtl),
                pixel_ratio: part!(pixel_ratio),
                locale: part!(locale),
            }
        })
    }
}

// --- What the core sets ------------------------------------------------------

impl super::Runtime {
    /// The executor of tasks and actions, and the status bar's layers.
    pub(crate) fn set_services(&self, executor: Rc<Executor>, status_bars: SharedStatusBars) {
        let _entered = self.enter();
        with_data(|d| {
            d.ambient.executor = Some(executor);
            d.ambient.status_bars = Some(status_bars);
        });
    }

    /// How text is measured (the core's text engine).
    pub(crate) fn set_measure(&self, measure: MeasureFn) {
        let _entered = self.enter();
        with_data(|d| d.ambient.measure = Some(measure));
    }

    /// The environment now: what read a part that changed runs again.
    pub(crate) fn set_environment(&self, env: Environment) {
        let _entered = self.enter();
        let signal = with_data(|d| d.ambient.env.as_ref().map(|e| e.env));
        match signal {
            Some(signal) => signal.set(env),
            None => {
                let signals = EnvSignals::new(env);
                with_data(|d| d.ambient.env = Some(signals));
            }
        }
    }

    /// The theme now: what read it runs again.
    pub(crate) fn set_theme(&self, theme: Arc<Theme>) {
        let _entered = self.enter();
        let signal = with_data(|d| d.ambient.theme);
        match signal {
            Some(signal) => signal.set(theme),
            None => {
                let signal = Owner::detached().run(|| signal_with(theme, SignalEquals::Never));
                with_data(|d| d.ambient.theme = Some(signal));
            }
        }
    }

    /// The hosts whose insets components read.
    pub(crate) fn inset_hosts(&self) -> Vec<NodeId> {
        let _entered = self.enter();
        with_data(|d| d.ambient.inset_hosts())
    }

    /// The insets of the screen or modal `host` is on.
    pub(crate) fn set_insets(&self, host: NodeId, insets: EdgeInsets) {
        let _entered = self.enter();
        if let Some(signal) = with_data(|d| d.ambient.insets.get(&host).copied()) {
            signal.set(insets);
        }
    }

    /// `host` goes: its insets are no longer read.
    pub(crate) fn forget_host(&self, host: NodeId) {
        let _entered = self.enter();
        with_data(|d| d.ambient.insets.remove(&host));
    }
}

fn env<T: Clone + 'static>(part: impl FnOnce(&EnvSignals) -> Memo<T>) -> T {
    let memo = with_data(|d| d.ambient.env.as_ref().map(part));
    memo.expect("the core sets the environment before components run")
        .get()
}

fn executor() -> Rc<Executor> {
    with_data(|d| d.ambient.executor.clone()).expect("the core sets the executor")
}

fn act(action: Action) {
    executor().act(action);
}

// --- The environment ---------------------------------------------------------

/// The surface's size in points (TS: `windowSize()`).
pub fn window() -> Size {
    env(|e| e.window)
}

/// The edges system UI covers: notch, home indicator (TS: `safeArea()`).
pub fn safe_area() -> EdgeInsets {
    env(|e| e.safe_area)
}

/// The software keyboard's height over the surface, 0 when hidden (TS:
/// `keyboard().height`).
pub fn keyboard_height() -> f32 {
    env(|e| e.keyboard_height)
}

/// How long the keyboard's show / hide animation takes, in seconds.
pub fn keyboard_duration() -> f32 {
    env(|e| e.keyboard_duration)
}

/// Dark appearance (TS: `colorScheme() === "dark"`). Theme colors follow
/// it without this; read it for anything else.
pub fn dark() -> bool {
    env(|e| e.dark)
}

/// `color` for the appearance now (props that take a [`Color`], such as an
/// input's placeholder color; styles take theme colors as they are).
pub fn color(color: impl Into<AdaptiveColor>) -> Color {
    let color = color.into();
    if dark() { color.dark } else { color.light }
}

/// The user's text size relative to the default (TS: `fontScale()`).
pub fn font_scale() -> f32 {
    env(|e| e.font_scale)
}

/// The user asked for less motion (TS: `reduceMotion()`).
pub fn reduce_motion() -> bool {
    env(|e| e.reduce_motion)
}

/// A screen reader runs (TS: `screenReaderEnabled()`).
pub fn screen_reader() -> bool {
    env(|e| e.screen_reader)
}

/// The layout runs right to left (TS: `I18nManager.isRTL`).
pub fn rtl() -> bool {
    env(|e| e.rtl)
}

/// Device pixels per point (TS: `PixelRatio.get()`).
pub fn pixel_ratio() -> f32 {
    env(|e| e.pixel_ratio)
}

/// `points` rounded to whole device pixels (TS:
/// `PixelRatio.roundToNearestPixel`).
pub fn round_to_nearest_pixel(points: f32) -> f32 {
    let ratio = pixel_ratio().max(f32::MIN_POSITIVE);
    (points * ratio).round() / ratio
}

/// The user's locale (TS: what `Intl` formats for by default).
pub fn locale() -> Locale {
    env(|e| e.locale)
}

/// The operating system (TS: `Platform.OS`); it does not change.
pub fn os() -> Os {
    with_data(|d| d.ambient.env.as_ref().map(|e| e.env))
        .expect("the core sets the environment")
        .with_untracked(|e| e.os)
}

/// The OS version: major and minor on iOS, the API level and 0 on Android
/// (TS: `Platform.Version`).
pub fn os_version() -> (u16, u16) {
    with_data(|d| d.ambient.env.as_ref().map(|e| e.env))
        .expect("the core sets the environment")
        .with_untracked(|e| e.os_version)
}

/// The kind of device (TS: `Platform.isPad`, `isTV`, `isMacCatalyst`).
pub fn idiom() -> Idiom {
    with_data(|d| d.ambient.env.as_ref().map(|e| e.env))
        .expect("the core sets the environment")
        .with_untracked(|e| e.idiom)
}

/// The app's theme (reading it subscribes: a new theme reaches what read
/// it).
pub fn theme() -> Arc<Theme> {
    with_data(|d| d.ambient.theme)
        .expect("the core sets the theme")
        .get()
}

/// The width of `text` on one line in `font` (the CSS `font` shorthand; an
/// invalid one measures in `10px sans-serif`), by the platform's text engine
/// (TS: a canvas's `measureText(text).width`).
pub fn measure_text(font: &str, text: &str) -> f32 {
    let font = CanvasFont::parse(font).unwrap_or_default();
    let measure = with_data(|d| d.ambient.measure.clone()).expect("the core sets the text engine");
    measure(&font, text)
}

/// The surface's `G`: one signal per type, made with `Default` the first
/// time (TS: a signal at module level). Every write notifies.
pub fn global<G: Default + 'static>() -> Signal<G> {
    let id = TypeId::of::<G>();
    if let Some(signal) = with_data(|d| {
        d.ambient
            .globals
            .get(&id)
            .and_then(|g| g.downcast_ref::<Signal<G>>().copied())
    }) {
        return signal;
    }
    let signal = Owner::detached().run(|| signal_with(G::default(), SignalEquals::Never));
    with_data(|d| d.ambient.globals.insert(id, Box::new(signal)));
    signal
}

/// The insets of the screen or modal the component on `host` is on, below
/// its header, else the safe area (TS: `useSafeAreaInsets()`).
pub(crate) fn insets_of(host: NodeId) -> EdgeInsets {
    let existing = with_data(|d| d.ambient.insets.get(&host).copied());
    let signal = existing.unwrap_or_else(|| {
        // The core fills it in before the commit ends.
        let signal = Owner::detached().run(|| super::signal(safe_area()));
        with_data(|d| d.ambient.insets.insert(host, signal));
        signal
    });
    signal.get()
}

// --- Services ----------------------------------------------------------------

/// Shows `dialog` (an alert, or with `sheet` an action sheet); await the
/// answer in a task.
pub fn dialog(dialog: Dialog) -> DialogAnswer {
    let (answer, slot) = DialogAnswer::new();
    act(Action::Dialog(dialog, slot));
    answer
}

/// Unfocuses the focused input, hiding the software keyboard.
pub fn dismiss_keyboard() {
    act(Action::Command(crate::ROOT_ID, Command::Blur));
}

/// Animates the layout changes of the next commit (React Native's
/// `LayoutAnimation.configureNext`).
pub fn animate_layout(animation: LayoutAnimation) {
    act(Action::Command(
        crate::ROOT_ID,
        Command::LayoutAnimation(animation),
    ));
}

/// The screen reader reads `message` (TS:
/// `AccessibilityInfo.announceForAccessibility`).
pub fn announce_for_accessibility(message: impl Into<String>) {
    act(super::services::announce(message.into()));
}

/// Puts `text` on the system clipboard (TS: `Clipboard.setString`).
pub fn set_clipboard(text: impl Into<String>) {
    act(super::services::set_clipboard(text.into()));
}

/// The text on the system clipboard, `""` when it holds none (TS:
/// `Clipboard.getString`), to await in a task.
pub fn clipboard_string() -> Answer<String> {
    let (answer, action) = super::services::clipboard_string();
    act(action);
    answer
}

/// Whether the system clipboard holds text (TS: `Clipboard.hasString`).
pub fn clipboard_has_string() -> Answer<bool> {
    let (answer, action) = super::services::clipboard_has_string();
    act(action);
    answer
}

/// Shows `message` at the bottom of the screen (Android's toasts, TS:
/// `Toast.show`); nothing happens elsewhere.
pub fn toast(message: impl Into<String>, duration: super::ToastDuration) {
    act(super::services::toast(message.into(), duration));
}

/// The size in pixels of the image at `uri` (TS: `Image.getSize`).
pub fn image_size(uri: impl Into<String>) -> Answer<Result<ImageSize, String>> {
    let (answer, action) = super::services::image_size(uri.into());
    act(action);
    answer
}

/// Loads the image at `uri` ahead of showing it (TS: `Image.prefetch`).
pub fn prefetch_image(uri: impl Into<String>) -> super::Prefetch {
    let (answer, action) = super::services::prefetch(uri.into());
    act(action);
    answer
}

/// Shows the surface in `appearance` whatever the system's (TS:
/// `setColorScheme`).
pub fn set_color_scheme(appearance: Appearance) {
    act(Action::Appearance(appearance));
}

/// Lays the surface out right to left whatever the user's language (TS:
/// `I18nManager.forceRTL`).
pub fn force_rtl(force: bool) {
    act(Action::Direction(super::services::Direction::Force(force)));
}

/// Whether a right-to-left language lays the surface out right to left
/// (TS: `I18nManager.allowRTL`).
pub fn allow_rtl(allow: bool) {
    act(Action::Direction(super::services::Direction::Allow(allow)));
}

/// Adds `setting` over the status bar's other settings while the current
/// owner lives (the most recently added wins, field by field): the entry,
/// to change it.
pub fn add_status_bar(setting: StatusBarSetting) -> StatusBarEntry {
    let globals =
        with_data(|d| d.ambient.status_bars.clone()).expect("the core sets the status bars");
    let entry = StatusBars::update(&globals, |bars| bars.add(setting));
    cleanup(move || StatusBars::update(&globals, |bars| bars.remove(entry)));
    entry
}

/// Changes a setting [`add_status_bar`] added.
pub fn update_status_bar(entry: StatusBarEntry, setting: StatusBarSetting) {
    let globals =
        with_data(|d| d.ambient.status_bars.clone()).expect("the core sets the status bars");
    StatusBars::update(&globals, |bars| bars.set(entry, setting));
}

// --- Tasks and timers --------------------------------------------------------

/// A running task ([`spawn`]): it ends with the owner it was made under, or
/// when [`Spawned::cancel`] is called. Awaiting it gives its output.
pub struct Spawned<T>(task::Task<T>);

impl<T> Spawned<T> {
    /// Ends the task now.
    pub fn cancel(self) {
        self.0.handle().cancel();
    }
}

impl<T> Future for Spawned<T> {
    type Output = T;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<T> {
        std::pin::Pin::new(&mut self.0).poll(cx)
    }
}

/// Runs `future` on the engine thread (TS: an `async` function): it changes
/// what it wants by writing signals, and ends with the current owner.
pub fn spawn<T: 'static>(future: impl Future<Output = T> + 'static) -> Spawned<T> {
    let executor = executor();
    let task = task::spawn_future(&executor, future);
    let handle = task.handle();
    cleanup(move || handle.cancel());
    Spawned(task)
}

/// Runs `f` once after `delay` (TS: `setTimeout`; cancelling the task is
/// `clearTimeout`), while the current owner lives.
pub fn set_timeout(delay: Duration, f: impl FnOnce() + 'static) -> Spawned<()> {
    let owner = Owner::current();
    spawn(async move {
        timer(delay).await;
        owner.run(f);
    })
}

/// Runs `f` every `period` (TS: `setInterval`), while the current owner
/// lives. Late wakes do not pile up runs.
pub fn set_interval(period: Duration, f: impl Fn() + 'static) -> Spawned<()> {
    let owner = Owner::current();
    spawn(async move {
        let mut next = Instant::now() + period;
        loop {
            timer_until(next).await;
            next = (next + period).max(Instant::now());
            owner.run(&f);
        }
    })
}

/// Runs `f` when the next display frame starts, with its time (TS:
/// `requestAnimationFrame`).
pub fn request_animation_frame(f: impl FnOnce(Instant) + 'static) -> Spawned<()> {
    let owner = Owner::current();
    spawn(async move {
        let at = next_frame().await;
        owner.run(move || f(at));
    })
}

/// Waits `delay` (in a task; TS: what `setTimeout` waits for).
pub fn timer(delay: Duration) -> impl Future<Output = ()> {
    #[cfg(not(target_arch = "wasm32"))]
    let sleep = tokio::time::sleep(delay);
    #[cfg(target_arch = "wasm32")]
    let sleep = crate::time::sleep(delay);
    sleep
}

/// Waits until `at` (in a task).
pub fn timer_until(at: Instant) -> impl Future<Output = ()> {
    #[cfg(not(target_arch = "wasm32"))]
    let sleep = tokio::time::sleep_until(at.into());
    #[cfg(target_arch = "wasm32")]
    let sleep = crate::time::sleep_until(at);
    sleep
}

/// The next display frame's start (in a task; TS: what
/// `requestAnimationFrame` waits for). Awaited in a loop, it runs once per
/// frame.
pub fn next_frame() -> Answer<Instant> {
    let (answer, action) = super::services::frame();
    act(action);
    answer
}

/// Runs blocking `f` on the runtime's blocking threads (in a task: await
/// its result; `tokio::task::spawn_blocking`). Not in WebAssembly.
#[cfg(not(target_arch = "wasm32"))]
pub fn blocking<R: Send + 'static>(
    f: impl FnOnce() -> R + Send + 'static,
) -> tokio::task::JoinHandle<R> {
    tokio::task::spawn_blocking(f)
}

/// Runs `future` on the runtime's threads (in a task: await its result;
/// `tokio::spawn`): `Send` work that should not hold the engine thread.
/// Not in WebAssembly, which has no threads.
#[cfg(not(target_arch = "wasm32"))]
pub fn background<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future)
}

// --- Handles -----------------------------------------------------------------

/// A native animation value starting at `value` (TS: `createNativeValue`):
/// bind it to styles, drags, scroll offsets or drawings, animate it from
/// handlers and tasks.
pub fn native_value(value: f32) -> super::NativeValue {
    super::NativeValue::new(&executor(), value)
}

/// A handle for an element (TS: `ref`): put it on the element with
/// [`super::Element::element_ref`], call its commands from handlers and
/// tasks.
pub fn element_ref() -> super::ElementRef {
    super::ElementRef::new(&executor())
}

/// A handle for a component this view renders (TS: a `ref` to a
/// component): put it on the component's element with
/// [`super::ComponentElement::component_ref`]; [`super::ComponentRef::api`]
/// is the API its view registered.
pub fn component_ref<C: super::Component>() -> super::ComponentRef<C> {
    super::ComponentRef::new(&executor())
}

/// Listens to the Android back button while the current owner lives (TS:
/// `BackHandler.addEventListener("hardwareBackPress", f)`): when no modal is
/// shown, `f` runs before the stack pops, newest listener first, and returns
/// true when it handled the press. While one listens, a back gesture does
/// not preview the pop.
pub fn on_back_press(f: impl Fn() -> bool + 'static) {
    let owner = Owner::current();
    let listener: super::back::BackListener = Rc::new(move || owner.run(&f));
    let back = &executor().back;
    let id = back.borrow_mut().add(listener);
    let subscription = super::back::BackSubscription::new(back, id);
    cleanup(move || drop(subscription));
}
