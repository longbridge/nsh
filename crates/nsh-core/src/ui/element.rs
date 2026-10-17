//! The elements a component renders ([`view`], [`text`], [`image`], other
//! components, slots and children) and the contexts of renders and
//! handlers.

use std::{
    any::{Any, TypeId},
    cell::RefCell,
    future::Future,
    marker::PhantomData,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use rustc_hash::FxHashMap;

use super::{
    Component, ComponentSpec, Event, JsValue, PropError, SharedString, Theme,
    component::AnyInstance,
    env::{self, Environment},
    events::{self, GestureEvent, PanEvent},
    status_bar::{StatusBarEntry, StatusBars},
    style::{StyleRefinement, Styled, Variant},
    task::{self, Action, AsyncCx, DialogAnswer, Executor, Task, WeakComponent},
};
use crate::{
    AccessibilityProp, AccessibilityRole, AccessibilityState, Command, Dialog, EdgeInsets,
    EventKind, EventPayload, ImageSource, JsonValue, LayoutAnimation, ListContentStyle, NativeProp,
    NodeId, PointerEvents, Prop, Rect, Size, StatusBarSetting, Tag, TouchEvent, style::Color,
    time::Instant,
};

/// A handler of `E` on an element, made by [`RenderCx::listener`]: it runs
/// with the component that rendered the element.
pub struct Listener<E: ?Sized>(pub(crate) Rc<ErasedListener<E>>);

/// A listener with the component's type erased (it downcasts the state).
/// It gets the component's state and props.
pub(crate) type ErasedListener<E> = dyn Fn(&mut dyn Any, &dyn Any, &E, &mut ErasedCx);

impl<E: ?Sized + 'static> Listener<E> {
    /// A listener that does nothing (the element still listens).
    pub(crate) fn noop() -> Self {
        Self(Rc::new(|_, _, _, _| {}))
    }
}

impl<E: 'static> Listener<E> {
    /// This listener for events of another type, turned into `E` by `f`:
    /// a list row hands the component its index with
    /// `remove.map(move |_: &()| index)`.
    pub fn map<F: ?Sized + 'static>(self, f: impl Fn(&F) -> E + 'static) -> Listener<F> {
        let inner = self.0;
        Listener(Rc::new(move |state, props, event: &F, cx| {
            inner(state, props, &f(event), cx);
        }))
    }
}

impl<E: ?Sized> Clone for Listener<E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// An event a handler emitted, with its type erased.
pub(crate) trait AnyEvent {
    fn name(&self) -> &'static str;
    fn data(&self) -> JsonValue;
    fn as_any(&self) -> &dyn Any;
}

impl<E: Event> AnyEvent for E {
    fn name(&self) -> &'static str {
        Event::name(self)
    }

    fn data(&self) -> JsonValue {
        Event::data(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// State shared by the components of a surface, by type
/// ([`Context::update_global`]).
pub(crate) type Globals = Rc<RefCell<FxHashMap<TypeId, Box<dyn Any>>>>;

/// What setup, listeners, timers and tasks did, for the core.
pub(crate) struct ErasedCx {
    pub(crate) emits: Vec<Box<dyn AnyEvent>>,
    pub(crate) notify: bool,
    pub(crate) environment: Environment,
    /// The component's host and the core's executor, for its tasks.
    pub(crate) host: NodeId,
    pub(crate) executor: Rc<Executor>,
    pub(crate) globals: Globals,
    pub(crate) actions: Vec<Action>,
    /// The globals `update_global` changed (their readers render again).
    pub(crate) changed_globals: Vec<TypeId>,
}

impl ErasedCx {
    pub(crate) fn new(
        environment: Environment,
        host: NodeId,
        executor: Rc<Executor>,
        globals: Globals,
    ) -> Self {
        Self {
            emits: Vec::new(),
            notify: false,
            environment,
            host,
            executor,
            globals,
            actions: Vec::new(),
            changed_globals: Vec::new(),
        }
    }
}

/// A render's context: the theme, the environment (reading it subscribes:
/// the component renders again when what it read changes, as TS
/// accessors do), and listeners for the elements.
pub struct RenderCx<'a, C> {
    scope: &'a RenderScope<'a>,
    pub(crate) reads: RefCell<RenderReads>,
    _component: PhantomData<fn(&mut C)>,
}

/// What a render can read, from the core.
pub(crate) struct RenderScope<'a> {
    pub(crate) theme: &'a Theme,
    pub(crate) environment: &'a Environment,
    pub(crate) insets: EdgeInsets,
    pub(crate) globals: &'a Globals,
    /// The nearest provided value of a type above the component.
    pub(crate) context: &'a dyn Fn(TypeId) -> Option<Rc<dyn Any>>,
    /// The width of a line of text in a canvas font (the text engine's).
    pub(crate) measure: &'a dyn Fn(&crate::canvas::CanvasFont, &str) -> f32,
}

/// A value a render provides to the components below it.
#[derive(Clone)]
pub(crate) struct Provided {
    pub(crate) type_id: TypeId,
    pub(crate) value: Rc<dyn Any>,
    pub(crate) eq: fn(&dyn Any, &dyn Any) -> bool,
}

impl Provided {
    /// Whether `other` provides the same value.
    pub(crate) fn same(&self, other: &Provided) -> bool {
        self.type_id == other.type_id && (self.eq)(&*self.value, &*other.value)
    }
}

/// What a render read (it renders again when one changes) and provided.
#[derive(Clone, Default)]
pub(crate) struct RenderReads {
    /// Parts of the environment ([`env`] bits).
    pub(crate) env: u16,
    pub(crate) globals: Vec<TypeId>,
    pub(crate) contexts: Vec<TypeId>,
    pub(crate) provided: Vec<Provided>,
}

impl<'a, C: Component> RenderCx<'a, C> {
    pub(crate) fn new(scope: &'a RenderScope<'a>) -> Self {
        Self {
            scope,
            reads: RefCell::new(RenderReads::default()),
            _component: PhantomData,
        }
    }

    fn read(&self, what: u16) -> &'a Environment {
        self.reads.borrow_mut().env |= what;
        self.scope.environment
    }

    /// The surface's `G` (made with `Default` the first time; TS: a
    /// signal at module level). Reading subscribes: the component
    /// renders again when a listener, task or module changes `G` with
    /// `update_global`.
    pub fn global<G: Default + 'static>(&self) -> std::cell::Ref<'a, G> {
        let id = TypeId::of::<G>();
        {
            let mut reads = self.reads.borrow_mut();
            if !reads.globals.contains(&id) {
                reads.globals.push(id);
            }
        }
        let globals = self.scope.globals;
        if !globals.borrow().contains_key(&id) {
            globals.borrow_mut().insert(id, Box::new(G::default()));
        }
        std::cell::Ref::map(globals.borrow(), |g| {
            g[&id].downcast_ref::<G>().expect("keyed by its type")
        })
    }

    /// Provides `value` to the components rendered below this one (TS:
    /// `<Ctx.Provider value>`): their [`Self::context`] finds it. A new value
    /// renders the components that read it again.
    pub fn provide<T: PartialEq + 'static>(&self, value: T) {
        fn eq<T: PartialEq + 'static>(a: &dyn Any, b: &dyn Any) -> bool {
            matches!((a.downcast_ref::<T>(), b.downcast_ref::<T>()), (Some(a), Some(b)) if a == b)
        }
        let provided = Provided {
            type_id: TypeId::of::<T>(),
            value: Rc::new(value),
            eq: eq::<T>,
        };
        let mut reads = self.reads.borrow_mut();
        reads.provided.retain(|p| p.type_id != provided.type_id);
        reads.provided.push(provided);
    }

    /// The `T` the nearest component above this one provides (TS:
    /// calling the context, `Ctx()`), through the app's elements too; `None` without one.
    /// Reading subscribes.
    pub fn context<T: 'static>(&self) -> Option<Rc<T>> {
        let id = TypeId::of::<T>();
        {
            let mut reads = self.reads.borrow_mut();
            if !reads.contexts.contains(&id) {
                reads.contexts.push(id);
            }
        }
        (self.scope.context)(id).and_then(|v| v.downcast::<T>().ok())
    }

    /// The app's theme.
    pub fn theme(&self) -> &'a Theme {
        self.scope.theme
    }

    /// The surface's size in points (TS: `windowSize()`).
    pub fn window(&self) -> Size {
        self.read(env::WINDOW).window
    }

    /// The edges system UI covers: notch, home indicator (TS:
    /// `safeArea()`).
    pub fn safe_area(&self) -> EdgeInsets {
        self.read(env::SAFE_AREA).safe_area
    }

    /// The edges system UI or a header covers where the component renders:
    /// its screen's (below the header) or modal's, else the safe area
    /// (TS: `useSafeAreaInsets()`).
    pub fn insets(&self) -> EdgeInsets {
        self.reads.borrow_mut().env |= env::INSETS;
        self.scope.insets
    }

    /// The software keyboard's height over the surface, 0 when hidden
    /// (TS: `keyboard().height`).
    pub fn keyboard_height(&self) -> f32 {
        self.read(env::KEYBOARD).keyboard_height
    }

    /// Dark appearance (TS: `colorScheme() === "dark"`). Theme colors
    /// follow it without this; read it for anything else.
    pub fn dark(&self) -> bool {
        self.read(env::APPEARANCE).dark
    }

    /// `color` for the appearance now (props that take a [`Color`], such as
    /// an input's placeholder color; styles take theme colors as they are).
    pub fn color(&self, color: impl Into<super::AdaptiveColor>) -> Color {
        let color = color.into();
        if self.dark() { color.dark } else { color.light }
    }

    /// The user's text size relative to the default (TS: `fontScale()`).
    pub fn font_scale(&self) -> f32 {
        self.read(env::FONT_SCALE).font_scale
    }

    /// The user asked for less motion (TS: `reduceMotion()`).
    pub fn reduce_motion(&self) -> bool {
        self.read(env::MOTION).reduce_motion
    }

    /// A screen reader runs (TS: `screenReaderEnabled()`).
    pub fn screen_reader(&self) -> bool {
        self.read(env::SCREEN_READER).screen_reader
    }

    /// The layout runs right to left (TS: `I18nManager.isRTL`).
    pub fn rtl(&self) -> bool {
        self.read(env::RTL).rtl
    }

    /// The width of `text` on one line in `font` (the CSS `font` shorthand,
    /// as [`super::Painter::font`] takes it; an invalid one measures in the
    /// canvas default, `10px sans-serif`), measured by the platform's text
    /// engine (TS: a canvas's `measureText(text).width`).
    pub fn measure_text(&self, font: &str, text: &str) -> f32 {
        let font = crate::canvas::CanvasFont::parse(font).unwrap_or_default();
        (self.scope.measure)(&font, text)
    }

    /// Device pixels per point (TS: `PixelRatio.get()`); one device pixel
    /// is `1.0 / cx.pixel_ratio()` points.
    pub fn pixel_ratio(&self) -> f32 {
        self.read(env::PIXEL_RATIO).pixel_ratio
    }

    /// `points` rounded to whole device pixels (TS:
    /// `PixelRatio.roundToNearestPixel`).
    pub fn round_to_nearest_pixel(&self, points: f32) -> f32 {
        let ratio = self.pixel_ratio().max(f32::MIN_POSITIVE);
        (points * ratio).round() / ratio
    }

    /// The user's locale (TS: what `Intl` and `toLocaleString` format
    /// for by default): format numbers and dates for it with a Rust crate.
    pub fn locale(&self) -> crate::platform::Locale {
        self.read(env::LOCALE).locale
    }

    /// The operating system (TS: `Platform.OS`).
    pub fn os(&self) -> crate::platform::Os {
        self.scope.environment.os
    }

    /// The OS version: major and minor on iOS, the API level and 0 on
    /// Android (TS: `Platform.Version`).
    pub fn os_version(&self) -> (u16, u16) {
        self.scope.environment.os_version
    }

    /// The kind of device (TS: `Platform.isPad`, `isTV`,
    /// `isMacCatalyst`).
    pub fn idiom(&self) -> crate::platform::Idiom {
        self.scope.environment.idiom
    }

    /// A handler for an element of this render: `f` gets the component (its
    /// state), the event and a [`Context`] to change state, emit events and
    /// start timers.
    pub fn listener<E: ?Sized + 'static>(
        &self,
        f: impl Fn(&mut C, &E, &mut Context<'_, C>) + 'static,
    ) -> Listener<E> {
        Listener(Rc::new(move |this, props, event, cx| {
            if let (Some(this), Some(props)) =
                (this.downcast_mut::<C>(), props.downcast_ref::<C::Props>())
            {
                f(this, event, &mut Context::new(cx, props));
            }
        }))
    }
}

/// The context of everything that changes a component: [`Component::setup`],
/// [`Component::props_changed`], listeners, timers and tasks' updates.
pub struct Context<'a, C: Component> {
    inner: &'a mut ErasedCx,
    props: &'a C::Props,
}

impl<'a, C: Component> Context<'a, C> {
    pub(crate) fn new(inner: &'a mut ErasedCx, props: &'a C::Props) -> Self {
        Self { inner, props }
    }

    /// The component's props now (TS: `props` in the component's body).
    pub fn props(&self) -> &C::Props {
        self.props
    }

    /// Sends `event` to the app (its `on<Event>` handler), or to the
    /// component that rendered this one.
    pub fn emit(&mut self, event: C::Event) {
        self.inner.emits.push(Box::new(event));
    }

    /// The component's state changed: render it again.
    pub fn notify(&mut self) {
        self.inner.notify = true;
    }

    /// The environment now (not a subscription: read it in the render for
    /// that).
    pub fn environment(&self) -> &Environment {
        &self.inner.environment
    }

    /// Runs `f` on the surface's `G` (made with `Default` the first time):
    /// state components share, like a signal at module level.
    pub fn update_global<G: Default + 'static, R>(
        &mut self,
        f: impl FnOnce(&mut G, &mut Self) -> R,
    ) -> R {
        let id = TypeId::of::<G>();
        // Out of the map while `f` runs: it may use other globals.
        let taken = self.inner.globals.borrow_mut().remove(&id);
        let mut global = taken
            .and_then(|g| g.downcast::<G>().ok())
            .unwrap_or_default();
        let result = f(&mut global, self);
        self.inner.globals.borrow_mut().insert(id, global);
        if !self.inner.changed_globals.contains(&id) {
            self.inner.changed_globals.push(id);
        }
        result
    }

    /// Shows `dialog` (an alert, or with `sheet` an action sheet); await the
    /// answer in a task.
    pub fn dialog(&mut self, dialog: Dialog) -> DialogAnswer {
        let (answer, slot) = DialogAnswer::new();
        self.inner.actions.push(Action::Dialog(dialog, slot));
        answer
    }

    /// Unfocuses the focused input, hiding the software keyboard.
    pub fn dismiss_keyboard(&mut self) {
        self.inner
            .actions
            .push(Action::Command(crate::ROOT_ID, Command::Blur));
    }

    /// Adds `setting` over the status bar's other settings (and what
    /// functions set) until [`Self::remove_status_bar`]: the most recently
    /// added wins, field by field.
    pub fn add_status_bar(&mut self, setting: StatusBarSetting) -> StatusBarEntry {
        StatusBars::update(&self.inner.globals, |bars| bars.add(setting))
    }

    /// Changes a setting [`Self::add_status_bar`] added.
    pub fn update_status_bar(&mut self, entry: StatusBarEntry, setting: StatusBarSetting) {
        StatusBars::update(&self.inner.globals, |bars| bars.set(entry, setting));
    }

    /// Removes a setting [`Self::add_status_bar`] added.
    pub fn remove_status_bar(&mut self, entry: StatusBarEntry) {
        StatusBars::update(&self.inner.globals, |bars| bars.remove(entry));
    }

    /// Animates the layout changes of the next commit (React Native's
    /// `LayoutAnimation.configureNext`).
    pub fn animate_layout(&mut self, animation: LayoutAnimation) {
        self.inner.actions.push(Action::Command(
            crate::ROOT_ID,
            Command::LayoutAnimation(animation),
        ));
    }

    /// The screen reader reads `message` (TS:
    /// `AccessibilityInfo.announceForAccessibility`).
    pub fn announce_for_accessibility(&mut self, message: impl Into<String>) {
        self.inner
            .actions
            .push(super::services::announce(message.into()));
    }

    /// Puts `text` on the system clipboard (TS: `Clipboard.setString`).
    pub fn set_clipboard(&mut self, text: impl Into<String>) {
        self.inner
            .actions
            .push(super::services::set_clipboard(text.into()));
    }

    /// The text on the system clipboard, `""` when it holds none (TS:
    /// `Clipboard.getString`), to await in a task. On iOS, reading it may
    /// show the system's paste notice.
    pub fn clipboard_string(&mut self) -> task::Answer<String> {
        let (answer, action) = super::services::clipboard_string();
        self.inner.actions.push(action);
        answer
    }

    /// Whether the system clipboard holds text, without reading it (TS:
    /// `Clipboard.hasString`), to await in a task.
    pub fn clipboard_has_string(&mut self) -> task::Answer<bool> {
        let (answer, action) = super::services::clipboard_has_string();
        self.inner.actions.push(action);
        answer
    }

    /// Shows `message` at the bottom of the screen (Android's toasts, TS:
    /// `Toast.show`); nothing happens elsewhere.
    pub fn toast(&mut self, message: impl Into<String>, duration: super::ToastDuration) {
        self.inner
            .actions
            .push(super::services::toast(message.into(), duration));
    }

    /// The size in pixels of the image at `uri` (TS: `Image.getSize`), to
    /// await in a task; its bytes stay loaded for the views that show it
    /// next.
    pub fn image_size(
        &mut self,
        uri: impl Into<String>,
    ) -> task::Answer<Result<task::ImageSize, String>> {
        let (answer, action) = super::services::image_size(uri.into());
        self.inner.actions.push(action);
        answer
    }

    /// Loads the image at `uri` ahead of showing it (TS: `Image.prefetch`):
    /// await it in a task for whether it loaded.
    pub fn prefetch_image(&mut self, uri: impl Into<String>) -> super::Prefetch {
        let (answer, action) = super::services::prefetch(uri.into());
        self.inner.actions.push(action);
        answer
    }

    /// Shows the surface in `appearance` whatever the system's (TS:
    /// `setColorScheme`): [`crate::ui::RenderCx::dark`] (the theme's
    /// mode), `dark:` classes and the platform's controls follow it;
    /// [`crate::platform::Appearance::System`] follows the system again.
    pub fn set_color_scheme(&mut self, appearance: crate::platform::Appearance) {
        self.inner.actions.push(Action::Appearance(appearance));
    }

    /// Lays the surface out right to left whatever the user's language
    /// (TS: `I18nManager.forceRTL`), at once: the components that read
    /// [`RenderCx::rtl`] render again. `false` follows the language again.
    pub fn force_rtl(&mut self, force: bool) {
        self.inner
            .actions
            .push(Action::Direction(super::services::Direction::Force(force)));
    }

    /// Whether a right-to-left language lays the surface out right to left
    /// (TS: `I18nManager.allowRTL`; true by default).
    pub fn allow_rtl(&mut self, allow: bool) {
        self.inner
            .actions
            .push(Action::Direction(super::services::Direction::Allow(allow)));
    }

    /// A native animation value starting at `value` (TS:
    /// `createNativeValue`): keep it in a field, bind it to styles, drags,
    /// scroll offsets or drawings, and animate it from listeners and tasks.
    pub fn native_value(&self, value: f32) -> super::NativeValue {
        super::NativeValue::new(&self.inner.executor, value)
    }

    /// A handle for an element of the render (TS: `ref`): keep it in a
    /// field, put it on the element with [`Element::element_ref`], call its
    /// commands from listeners and tasks.
    pub fn element_ref(&self) -> super::ElementRef {
        super::ElementRef::new(&self.inner.executor)
    }

    /// A handle for a component of the render (TS: a `ref` to a component,
    /// `stepper.value()`): keep it in a field, put it on the component with
    /// [`ComponentElement::component_ref`], call its methods with
    /// [`super::ComponentRef::update`] from listeners and tasks.
    pub fn component_ref<T: Component>(&self) -> super::ComponentRef<T> {
        super::ComponentRef::new(&self.inner.executor)
    }

    /// Starts a task: `f(this, cx)` builds a future run on the engine
    /// thread, which changes the component with `this.update(..)` and waits
    /// with `cx.timer(..)` or for any other future. Dropping the [`Task`]
    /// cancels it; the component's tasks end with it.
    ///
    /// ```ignore
    /// self.loading = Some(cx.spawn(|this, cx| async move {
    ///     cx.timer(Duration::from_millis(300)).await;
    ///     this.update(|this, cx| { this.ready = true; cx.notify(); }).await;
    /// }));
    /// ```
    pub fn spawn<R: 'static, Fut: Future<Output = R> + 'static>(
        &mut self,
        f: impl FnOnce(WeakComponent<C>, AsyncCx) -> Fut,
    ) -> Task<R> {
        task::spawn(&self.inner.executor, self.inner.host, f)
    }

    /// This component as a handle others keep (TS: handing another part
    /// of the app an object with methods, a store's `start()` / `stop()`):
    /// a component elsewhere, on another screen say, changes it with
    /// [`WeakComponent::update`], applied before the next render, `None`
    /// once it is gone. Its tasks get the same handle.
    pub fn weak(&self) -> WeakComponent<C> {
        WeakComponent::new(self.inner.host, Rc::downgrade(&self.inner.executor))
    }

    /// Listens to the Android back button (TS:
    /// `BackHandler.addEventListener("hardwareBackPress", f)`): when no modal
    /// is shown, `f` runs before the stack pops, newest listener first, and
    /// returns true when it handled the press; when no listener does, the
    /// innermost stack pops, else the app exits. Dropping the subscription
    /// removes the listener; while one exists, a back gesture does not
    /// preview the pop.
    pub fn on_back_press(
        &mut self,
        f: impl Fn(&mut C, &mut Context<'_, C>) -> bool + 'static,
    ) -> super::BackSubscription {
        let listener: super::back::BackListener = Rc::new(move |this, props, cx| {
            match (this.downcast_mut::<C>(), props.downcast_ref::<C::Props>()) {
                (Some(this), Some(props)) => f(this, &mut Context::new(cx, props)),
                _ => false,
            }
        });
        let back = &self.inner.executor.back;
        let id = back.borrow_mut().add(self.inner.host, listener);
        super::BackSubscription::new(back, id)
    }

    /// Runs `f` once after `delay` (TS: `setTimeout`; dropping the task
    /// is `clearTimeout`). A task: a timer, then an update.
    pub fn set_timeout(
        &mut self,
        delay: Duration,
        f: impl FnOnce(&mut C, &mut Context<'_, C>) + 'static,
    ) -> Task<()> {
        self.spawn(move |this, cx| async move {
            cx.timer(delay).await;
            this.update(f).await;
        })
    }

    /// Runs `f` when the next display frame starts, with its time (TS:
    /// `requestAnimationFrame`; dropping the task is
    /// `cancelAnimationFrame`). Ask again from `f` for the frame after; a
    /// task awaiting [`AsyncCx::next_frame`] in a loop runs every frame.
    pub fn request_animation_frame(
        &mut self,
        f: impl FnOnce(&mut C, Instant, &mut Context<'_, C>) + 'static,
    ) -> Task<()> {
        self.spawn(move |this, cx| async move {
            let at = cx.next_frame().await;
            this.update(move |this, cx| f(this, at, cx)).await;
        })
    }

    /// Runs `f` every `period` (TS: `setInterval`; dropping the task is
    /// `clearInterval`). Late wakes do not pile up runs.
    pub fn set_interval(
        &mut self,
        period: Duration,
        f: impl Fn(&mut C, &mut Context<'_, C>) + 'static,
    ) -> Task<()> {
        let f = Rc::new(f);
        self.spawn(move |this, cx| async move {
            let mut next = Instant::now() + period;
            loop {
                cx.timer_until(next).await;
                next = (next + period).max(Instant::now());
                let f = f.clone();
                if this.update(move |this, cx| f(this, cx)).await.is_none() {
                    break;
                }
            }
        })
    }
}

/// The props of an element the core diffs field by field (opaque).
#[doc(hidden)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attrs {
    pub(crate) disabled: bool,
    pub(crate) accessible: Option<bool>,
    pub(crate) label: Option<SharedString>,
    pub(crate) hint: Option<SharedString>,
    pub(crate) role: AccessibilityRole,
    pub(crate) state: AccessibilityState,
    pub(crate) pointer_events: PointerEvents,
    pub(crate) active_opacity: Option<f32>,
    pub(crate) number_of_lines: u32,
    pub(crate) src: Option<super::ImageSrc>,
    pub(crate) inset_padding: Option<EdgeInsets>,
    pub(crate) inset_margin: Option<EdgeInsets>,
    /// An inline style from JS (a `StyleProp` prop), over the styles.
    pub(crate) inline: Option<JsonValue>,
    /// The props of host elements (inputs, scroll views, ...), one per
    /// property ([`super::host`]).
    pub(crate) props: Vec<Prop>,
    /// A canvas's drawing.
    pub(crate) drawing: Option<Arc<crate::canvas::Drawing>>,
    /// The handle the render put on the element (not a prop).
    pub(crate) element_ref: Option<super::ElementRef>,
    /// Style properties bound to native animation nodes (kept alive while
    /// bound).
    pub(crate) animated: Vec<(crate::StyleDecl, super::NativeNode)>,
    /// Native nodes host props read (drags, scroll offsets), kept alive.
    pub(crate) bound: Vec<super::NativeNode>,
}

impl Attrs {
    /// The props of the fields that differ from `old`, in a fixed order.
    pub(crate) fn changes(&self, old: &Attrs, out: &mut Vec<Prop>) {
        use AccessibilityProp as A;
        if self.disabled != old.disabled {
            out.push(Prop::Native(NativeProp::Disabled(self.disabled)));
        }
        if self.accessible != old.accessible {
            out.push(Prop::Accessibility(A::Accessible(self.accessible)));
        }
        if self.label != old.label {
            out.push(Prop::Accessibility(A::Label(
                self.label.as_ref().map(SharedString::to_arc),
            )));
        }
        if self.hint != old.hint {
            out.push(Prop::Accessibility(A::Hint(
                self.hint.as_ref().map(SharedString::to_arc),
            )));
        }
        if self.role != old.role {
            out.push(Prop::Accessibility(A::Role(self.role)));
        }
        if self.state != old.state {
            out.push(Prop::Accessibility(A::State(self.state)));
        }
        if self.pointer_events != old.pointer_events {
            out.push(Prop::PointerEvents(self.pointer_events));
        }
        if self.active_opacity != old.active_opacity {
            out.push(Prop::ActiveOpacity(self.active_opacity));
        }
        if self.number_of_lines != old.number_of_lines {
            out.push(Prop::NumberOfLines(self.number_of_lines));
        }
        if self.src != old.src {
            out.push(Prop::Source(self.src.as_ref().map(|src| ImageSource {
                uri: src.uri.to_arc(),
                size: src.size,
            })));
        }
        if self.inset_padding != old.inset_padding {
            out.push(Prop::InsetPadding(self.inset_padding.unwrap_or_default()));
        }
        if self.inset_margin != old.inset_margin {
            out.push(Prop::InsetMargin(self.inset_margin.unwrap_or_default()));
        }
        // Host props: the ones no longer set go back to their defaults,
        // then the new and changed ones.
        for prop in &old.props {
            if !self.props.iter().any(|p| super::host::same_kind(p, prop))
                && let Some(reset) = super::host::reset(prop)
            {
                out.push(reset);
            }
        }
        for prop in &self.props {
            if !old.props.contains(prop) {
                out.push(prop.clone());
            }
        }
    }
}

/// A nested component's props, with their type erased.
pub(crate) trait AnyProps {
    fn as_any(&self) -> &dyn Any;
    fn clone_box(&self) -> Box<dyn AnyProps>;
}

impl<P: Clone + 'static> AnyProps for P {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn AnyProps> {
        Box::new(self.clone())
    }
}

/// A nested component of a render.
pub(crate) struct ComponentEl {
    pub(crate) name: &'static str,
    /// Makes an instance: a component rendered by Rust need not be
    /// registered (the registry is what JS can render by name).
    pub(crate) make: fn() -> Box<dyn AnyInstance>,
    /// Its spec (made once per type and kept by the core).
    pub(crate) spec: fn() -> ComponentSpec,
    pub(crate) props: Box<dyn AnyProps>,
    /// The rendering component's handler of its events.
    pub(crate) on_event: Option<Listener<dyn Any>>,
}

impl Clone for ComponentEl {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            make: self.make,
            spec: self.spec,
            props: self.props.clone_box(),
            on_event: self.on_event.clone(),
        }
    }
}

/// Places one piece of the app's content: a JSX child, or a list row.
pub(crate) type PlaceFn = Rc<dyn Fn(Child, Position) -> AnyElement>;

/// Where a JSX child or a list row is among its siblings (what a TS row
/// knows from its index and the data's length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Its index.
    pub index: usize,
    /// How many there are.
    pub count: usize,
}

impl Position {
    /// The first one.
    pub fn is_first(&self) -> bool {
        self.index == 0
    }

    /// The last one.
    pub fn is_last(&self) -> bool {
        self.index + 1 == self.count
    }
}

/// How a list is laid out (each set field is sent to the list); set with
/// [`super::ListLayout`]'s methods.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ListConfig {
    pub(crate) estimated: Option<f32>,
    pub(crate) columns: Option<u32>,
    pub(crate) horizontal: Option<bool>,
    pub(crate) inverted: Option<bool>,
    pub(crate) shows_indicator: Option<bool>,
    pub(crate) content: Option<ListContentStyle>,
    pub(crate) follow_end: Option<f32>,
}

impl ListConfig {
    /// The props of the fields that differ from `old`.
    pub(crate) fn changes(&self, old: &ListConfig, out: &mut Vec<Prop>) {
        if self.estimated != old.estimated {
            out.push(Prop::EstimatedItemSize(self.estimated.unwrap_or(50.0)));
        }
        if self.columns != old.columns {
            out.push(Prop::Columns(self.columns.unwrap_or(1)));
        }
        if self.horizontal != old.horizontal {
            out.push(Prop::Horizontal(self.horizontal.unwrap_or(false)));
        }
        if self.inverted != old.inverted {
            out.push(Prop::Inverted(self.inverted.unwrap_or(false)));
        }
        if self.shows_indicator != old.shows_indicator {
            out.push(Prop::ShowsIndicator(self.shows_indicator.unwrap_or(true)));
        }
        if self.content != old.content {
            out.push(Prop::ListContent(self.content.unwrap_or_default()));
        }
        if self.follow_end != old.follow_end {
            out.push(Prop::FollowEnd(self.follow_end));
        }
    }
}

/// A list slot as a render places it.
#[derive(Clone)]
pub(crate) struct ListEl {
    pub(crate) name: &'static str,
    pub(crate) config: ListConfig,
    pub(crate) row: PlaceFn,
}

/// What an element is beyond its tag.
#[derive(Clone)]
pub(crate) enum ElKind {
    Plain,
    /// The glue's node of slot `name`.
    Slot(&'static str),
    /// Where the JSX children go (a list row's content, in a row).
    Children,
    /// One JSX child (in [`ParentElement::children_each`]).
    JsChild(NodeId),
    /// Every JSX child, each placed by the function.
    EachChild(PlaceFn),
    /// The glue's list of an [`Items`] prop.
    List(Box<ListEl>),
    /// A list of the component's own rows ([`super::virtual_list`]).
    VirtualList(Box<super::virtual_list::VirtualListEl>),
    /// A piece of a text's content among its spans (a raw text node, as
    /// JS's strings in a `<text>`).
    Raw,
    Component(ComponentEl),
}

/// An element as the core expands it (opaque).
#[doc(hidden)]
#[derive(Clone)]
pub struct ElNode {
    pub(crate) tag: Tag,
    pub(crate) key: Option<SharedString>,
    /// A text's content.
    pub(crate) text: Option<SharedString>,
    pub(crate) attrs: Attrs,
    pub(crate) style: StyleRefinement,
    pub(crate) handlers: Vec<(EventKind, Listener<EventPayload>)>,
    /// Its part in the touch responder's negotiation.
    pub(crate) responder: Option<Rc<super::responder::ResponderEl>>,
    pub(crate) children: Vec<ElNode>,
    pub(crate) kind: ElKind,
}

impl ElNode {
    /// What a component that is not registered renders.
    pub(crate) fn empty_view() -> Self {
        Self::new(Tag::View)
    }

    pub(crate) fn new(tag: Tag) -> Self {
        Self {
            tag,
            key: None,
            text: None,
            attrs: Attrs::default(),
            style: StyleRefinement::default(),
            handlers: Vec::new(),
            responder: None,
            children: Vec::new(),
            kind: ElKind::Plain,
        }
    }
}

/// Any element, its type erased.
pub struct AnyElement(pub(crate) ElNode);

/// What can be a child: elements, and strings (as texts).
pub trait IntoElement {
    /// As an element.
    fn into_element(self) -> AnyElement;
}

impl IntoElement for AnyElement {
    fn into_element(self) -> AnyElement {
        self
    }
}

impl IntoElement for SharedString {
    fn into_element(self) -> AnyElement {
        text(self).into_element()
    }
}

impl IntoElement for &'static str {
    fn into_element(self) -> AnyElement {
        text(self).into_element()
    }
}

impl IntoElement for String {
    fn into_element(self) -> AnyElement {
        text(self).into_element()
    }
}

/// Conditional building, as in GPUI.
pub trait FluentBuilder: Sized {
    /// `then(self)` when `condition` holds.
    fn when(self, condition: bool, then: impl FnOnce(Self) -> Self) -> Self {
        if condition { then(self) } else { self }
    }

    /// `then(self, value)` when `option` has one.
    fn when_some<T>(self, option: Option<T>, then: impl FnOnce(Self, T) -> Self) -> Self {
        match option {
            Some(v) => then(self, v),
            None => self,
        }
    }

    /// `f(self)`.
    fn map(self, f: impl FnOnce(Self) -> Self) -> Self {
        f(self)
    }
}

impl<T: Element> FluentBuilder for T {}

/// An element: what every element can do. Its other abilities are the
/// traits it implements ([`Styled`], [`Accessible`], [`Interactive`],
/// [`ParentElement`]).
pub trait Element: IntoElement + Sized {
    #[doc(hidden)]
    fn node(&mut self) -> &mut ElNode;

    /// Identifies the element among its siblings across renders (where
    /// their order changes).
    fn key(mut self, key: impl Into<SharedString>) -> Self {
        self.node().key = Some(key.into());
        self
    }

    /// Binds `opacity` to a native value (it moves natively).
    fn bind_opacity(mut self, value: &impl super::Animated) -> Self {
        let node = value.native_node();
        let decl = crate::StyleDecl::Animated(nsh_animation::AnimationProp::Opacity, node.id());
        self.node().attrs.animated.push((decl, node));
        self
    }

    /// Binds the background color to a node of colors
    /// ([`super::NativeNode::interpolate_colors`]).
    fn bind_background(mut self, value: &impl super::Animated) -> Self {
        let node = value.native_node();
        let decl =
            crate::StyleDecl::Animated(nsh_animation::AnimationProp::BackgroundColor, node.id());
        self.node().attrs.animated.push((decl, node));
        self
    }

    /// Binds the color of every border side to a node of colors.
    fn bind_border_color(self, value: &impl super::Animated) -> Self {
        self.bind(nsh_animation::AnimationProp::BorderColor, value)
    }

    /// Binds a text's color (the whole text) to a node of colors.
    fn bind_color(self, value: &impl super::Animated) -> Self {
        self.bind(nsh_animation::AnimationProp::Color, value)
    }

    /// Binds the shadow's color to a node of colors (TS: `shadowColor`).
    fn bind_shadow_color(self, value: &impl super::Animated) -> Self {
        self.bind(nsh_animation::AnimationProp::ShadowColor, value)
    }

    /// Binds the shadow's opacity, 0..1 (TS: `shadowOpacity`).
    fn bind_shadow_opacity(self, value: &impl super::Animated) -> Self {
        self.bind(nsh_animation::AnimationProp::ShadowOpacity, value)
    }

    /// Binds the shadow's blur radius, in points (TS: `shadowRadius`).
    fn bind_shadow_radius(self, value: &impl super::Animated) -> Self {
        self.bind(nsh_animation::AnimationProp::ShadowRadius, value)
    }

    /// Binds `prop` (anything but a transform step or a canvas argument)
    /// to a native value.
    fn bind(mut self, prop: nsh_animation::AnimationProp, value: &impl super::Animated) -> Self {
        let node = value.native_node();
        let decl = crate::StyleDecl::Animated(prop, node.id());
        self.node().attrs.animated.push((decl, node));
        self
    }

    /// Appends a transform step driven by a native value, after the styles'
    /// transforms.
    fn bind_transform(mut self, kind: crate::TransformKind, value: &impl super::Animated) -> Self {
        let node = value.native_node();
        let decl = crate::StyleDecl::Transform(kind, kind.identity(), Some(node.id()));
        self.node().attrs.animated.push((decl, node));
        self
    }

    /// Moves it right by a native value (points).
    fn bind_translate_x(self, value: &impl super::Animated) -> Self {
        self.bind_transform(crate::TransformKind::TranslateX, value)
    }

    /// Moves it down by a native value (points).
    fn bind_translate_y(self, value: &impl super::Animated) -> Self {
        self.bind_transform(crate::TransformKind::TranslateY, value)
    }

    /// Scales it by a native value.
    fn bind_scale(self, value: &impl super::Animated) -> Self {
        self.bind_transform(crate::TransformKind::Scale, value)
    }

    /// Rotates it by a native value (degrees, clockwise).
    fn bind_rotate(self, value: &impl super::Animated) -> Self {
        self.bind_transform(crate::TransformKind::Rotate, value)
    }

    /// Puts `handle` on this element: its commands (`focus`, `scroll_to`)
    /// go here.
    fn element_ref(mut self, handle: &super::ElementRef) -> Self {
        self.node().attrs.element_ref = Some(handle.clone());
        self
    }

    /// Padding added to the element's own (from styles): insets of system
    /// UI, a keyboard.
    fn inset_padding(mut self, insets: EdgeInsets) -> Self {
        self.node().attrs.inset_padding = Some(insets);
        self
    }

    /// Margin added to the element's own.
    fn inset_margin(mut self, insets: EdgeInsets) -> Self {
        self.node().attrs.inset_margin = Some(insets);
        self
    }

    /// An inline style the app passed (a prop of type `Style`, React
    /// Native's names), over the element's styles.
    fn inline_style(mut self, style: &Style) -> Self {
        self.node().attrs.inline = style.0.clone();
        self
    }
}

/// An inline style from JS: a prop typed `StyleProp<ViewStyle>` in
/// TypeScript (an object, an array of them, or nothing), applied with
/// [`Element::inline_style`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style(pub(crate) Option<JsonValue>);

impl JsValue for Style {
    fn ts_type(_types: &mut super::TsTypes) -> String {
        "StyleProp<ViewStyle>".into()
    }

    fn from_json(value: JsonValue) -> Result<Self, PropError> {
        Ok(Self((!value.is_null()).then_some(value)))
    }

    fn to_json(&self) -> JsonValue {
        self.0.clone().unwrap_or(JsonValue::Null)
    }
}

impl<T: Element> Styled for T {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.node().style
    }
}

/// Elements with children.
pub trait ParentElement: Element {
    /// Adds a child.
    fn child(mut self, child: impl IntoElement) -> Self {
        self.node().children.push(child.into_element().0);
        self
    }

    /// Adds children (an `Option` adds one or none).
    fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.node()
            .children
            .extend(children.into_iter().map(|c| c.into_element().0));
        self
    }

    /// Places slot `slot` here when the app passed it, as `place` styles it
    /// (the closure gets the slot's own node).
    fn slot(mut self, slot: Option<Slot>, place: impl FnOnce(SlotElement) -> SlotElement) -> Self {
        if let Some(slot) = slot {
            let mut node = ElNode::new(Tag::View);
            node.kind = ElKind::Slot(slot.name);
            self.node().children.push(place(SlotElement(node)).0);
        }
        self
    }

    /// Places the app's JSX children here one by one: `place(child, at)`
    /// wraps each (runs again when the children change).
    fn children_each<R: IntoElement>(
        mut self,
        children: Children,
        place: impl Fn(Child, Position) -> R + 'static,
    ) -> Self {
        let _ = children;
        let mut node = ElNode::new(Tag::View);
        node.kind = ElKind::EachChild(Rc::new(move |child, at| place(child, at).into_element()));
        self.node().children.push(node);
        self
    }

    /// Places the app's virtualized list of `items` here: `config` lays
    /// the list out (size, columns, padding), `row(content, at)` wraps
    /// each row the app renders (runs again when a cell shows another row
    /// or the number of rows changes). Only rows near the viewport exist;
    /// the core recycles them.
    fn list<R: IntoElement>(
        mut self,
        items: Items,
        config: impl FnOnce(ListElement) -> ListElement,
        row: impl Fn(Child, Position) -> R + 'static,
    ) -> Self {
        let mut node = ElNode::new(Tag::List);
        node.kind = ElKind::List(Box::new(ListEl {
            name: items.name,
            config: ListConfig::default(),
            row: Rc::new(move |content, at| row(content, at).into_element()),
        }));
        self.node().children.push(config(ListElement(node)).0);
        self
    }
}

/// Elements with an accessibility description.
pub trait Accessible: Element {
    /// Whether assistive technologies see it as one element.
    fn accessible(mut self, accessible: bool) -> Self {
        self.node().attrs.accessible = Some(accessible);
        self
    }

    /// What screen readers say.
    fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.node().attrs.label = Some(label.into());
        self
    }

    /// What happens when activated, for screen readers.
    fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.node().attrs.hint = Some(hint.into());
        self
    }

    /// What it is (a button, a header, ...).
    fn role(mut self, role: AccessibilityRole) -> Self {
        self.node().attrs.role = role;
        self
    }

    /// Its state for screen readers (`disabled` follows
    /// [`Interactive::disabled`]).
    fn accessibility_state(mut self, state: AccessibilityState) -> Self {
        let attrs = &mut self.node().attrs;
        attrs.state = AccessibilityState {
            disabled: attrs.state.disabled,
            ..state
        };
        self
    }
}

/// Elements that take touches.
pub trait Interactive: Element {
    /// Handles presses.
    fn on_press(self, listener: Listener<()>) -> Self {
        self.on(EventKind::Press, events::typed(listener, events::unit))
    }

    /// Handles presses held for about half a second.
    fn on_long_press(self, listener: Listener<()>) -> Self {
        self.on(EventKind::LongPress, events::typed(listener, events::unit))
    }

    /// A finger went down on the element.
    fn on_press_in(self, listener: Listener<()>) -> Self {
        self.on(EventKind::PressIn, events::typed(listener, events::unit))
    }

    /// The press ended, inside or outside the element.
    fn on_press_out(self, listener: Listener<()>) -> Self {
        self.on(EventKind::PressOut, events::typed(listener, events::unit))
    }

    /// Its frame changed (relative to its parent).
    fn on_layout(self, listener: Listener<Rect>) -> Self {
        self.on(EventKind::Layout, events::typed(listener, events::layout))
    }

    /// Touches on it or inside it, every phase (custom gestures): the
    /// elements around the finger hear them from it outwards. To have them
    /// alone, claim them ([`Interactive::claim_touches`]).
    fn on_touch(self, listener: Listener<TouchEvent>) -> Self {
        self.on(EventKind::Touch, events::typed(listener, events::touch))
    }

    /// Claims the touches of a gesture as `claim` says (TS:
    /// `onStartShouldSetResponder`, `PanResponder`): as the responder it
    /// hears them alone (`on_responder_*`), and native scroll views around
    /// it stop taking them over. Several claims may be given.
    fn claim_touches(mut self, claim: super::TouchClaim) -> Self {
        if let Some(pan) = claim.pan {
            super::host::set(self.node(), Prop::PanActivation(Some(pan)));
        }
        super::responder::of(self.node()).claims.push(claim);
        self
    }

    /// As the responder, refuses to hand the touches over to an element
    /// around it that claims them (TS: `onResponderTerminationRequest`
    /// returning false); a native scroll view that started dragging first
    /// still takes them.
    fn keep_touches(mut self, keep: bool) -> Self {
        super::responder::of(self.node()).keep = keep;
        self
    }

    /// It became the responder (TS: `onResponderGrant`).
    fn on_responder_grant(mut self, listener: Listener<super::ResponderEvent>) -> Self {
        super::responder::of(self.node()).grant = Some(listener);
        self
    }

    /// It claimed the touches, but the responder kept them (TS:
    /// `onResponderReject`).
    fn on_responder_reject(mut self, listener: Listener<super::ResponderEvent>) -> Self {
        super::responder::of(self.node()).reject = Some(listener);
        self
    }

    /// As the responder, a finger went down (the first one too, after the
    /// grant; TS: `onResponderStart`).
    fn on_responder_start(mut self, listener: Listener<super::ResponderEvent>) -> Self {
        super::responder::of(self.node()).start = Some(listener);
        self
    }

    /// As the responder, the fingers moved (TS: `onResponderMove`).
    fn on_responder_move(mut self, listener: Listener<super::ResponderEvent>) -> Self {
        super::responder::of(self.node()).moved = Some(listener);
        self
    }

    /// As the responder, a finger lifted (TS: `onResponderEnd`).
    fn on_responder_end(mut self, listener: Listener<super::ResponderEvent>) -> Self {
        super::responder::of(self.node()).end = Some(listener);
        self
    }

    /// The last finger lifted: it is no longer the responder (TS:
    /// `onResponderRelease`).
    fn on_responder_release(mut self, listener: Listener<super::ResponderEvent>) -> Self {
        super::responder::of(self.node()).release = Some(listener);
        self
    }

    /// Another element or a native gesture took the touches over, or the
    /// system cancelled them (TS: `onResponderTerminate`).
    fn on_responder_terminate(mut self, listener: Listener<super::ResponderEvent>) -> Self {
        super::responder::of(self.node()).terminate = Some(listener);
        self
    }

    /// Drags move these values natively (by the drag's translation from
    /// where they were), with no render per frame; bind them to its
    /// transform to follow the finger. `on_pan_end` reports where they
    /// ended, to spring them on.
    fn pan_values(
        mut self,
        x: Option<&super::NativeValue>,
        y: Option<&super::NativeValue>,
    ) -> Self {
        let x = x.map(|v| v.node().clone());
        let y = y.map(|v| v.node().clone());
        let prop = Prop::PanValues(x.as_ref().map(|n| n.id()), y.as_ref().map(|n| n.id()));
        super::host::set(self.node(), prop);
        let bound = &mut self.node().attrs.bound;
        bound.extend(x);
        bound.extend(y);
        self
    }

    /// Two-finger pinches scale `scale` natively (multiplying the value it
    /// has when the pinch starts; TS: `pinch={{ scale }}`); bind it to its
    /// transform ([`Element::bind_scale`]). `on_pinch_start` /
    /// `on_pinch_end` report the pinches.
    fn pinch_value(mut self, scale: &super::NativeValue) -> Self {
        let node = scale.node().clone();
        super::host::set(self.node(), Prop::PinchValue(Some(node.id())));
        self.node().attrs.bound.push(node);
        self
    }

    /// Two-finger rotations turn `angle` natively, in degrees (added to
    /// the value it has when the rotation starts; TS: `rotation={{ angle
    /// }}`); bind it to its transform ([`Element::bind_rotate`]).
    fn rotation_value(mut self, angle: &super::NativeValue) -> Self {
        let node = angle.node().clone();
        super::host::set(self.node(), Prop::RotationValue(Some(node.id())));
        self.node().attrs.bound.push(node);
        self
    }

    /// A drag started on it (the platform recognizes it).
    fn on_pan_start(self, listener: Listener<PanEvent>) -> Self {
        self.on(EventKind::PanStart, events::typed(listener, events::pan))
    }

    /// The drag ended, with its velocity.
    fn on_pan_end(self, listener: Listener<PanEvent>) -> Self {
        self.on(EventKind::PanEnd, events::typed(listener, events::pan))
    }

    /// A pinch started (its scale).
    fn on_pinch_start(self, listener: Listener<GestureEvent>) -> Self {
        self.on(
            EventKind::PinchStart,
            events::typed(listener, events::pinch),
        )
    }

    /// The pinch ended.
    fn on_pinch_end(self, listener: Listener<GestureEvent>) -> Self {
        self.on(EventKind::PinchEnd, events::typed(listener, events::pinch))
    }

    /// A two-finger rotation started (radians).
    fn on_rotation_start(self, listener: Listener<GestureEvent>) -> Self {
        self.on(
            EventKind::RotationStart,
            events::typed(listener, events::rotation),
        )
    }

    /// The rotation ended.
    fn on_rotation_end(self, listener: Listener<GestureEvent>) -> Self {
        self.on(
            EventKind::RotationEnd,
            events::typed(listener, events::rotation),
        )
    }

    /// An assistive technology performed an action (its name).
    fn on_accessibility_action(self, listener: Listener<str>) -> Self {
        self.on(
            EventKind::AccessibilityAction,
            events::typed(listener, events::action),
        )
    }

    /// Handles native event `kind` with its raw payload (the element
    /// listens to it).
    fn on(mut self, kind: EventKind, listener: Listener<EventPayload>) -> Self {
        self.node().handlers.push((kind, listener));
        self
    }

    /// Presses also count this far outside it.
    fn hit_slop(mut self, insets: EdgeInsets) -> Self {
        super::host::set(self.node(), Prop::HitSlop(insets));
        self
    }

    /// How long a press must last to be a long press, in milliseconds.
    fn long_press_delay(mut self, ms: f32) -> Self {
        super::host::set(self.node(), Prop::LongPressDelay(Some(ms)));
        self
    }

    /// Shows `color` as the background while pressed, fading the children
    /// to `opacity`.
    fn underlay(mut self, color: Color, opacity: f32) -> Self {
        super::host::set(self.node(), Prop::UnderlayColor(Some(color)));
        super::host::set(self.node(), Prop::UnderlayOpacity(opacity));
        self
    }

    /// The press ripple (Android).
    fn ripple(mut self, ripple: crate::props::Ripple) -> Self {
        super::host::set(self.node(), Prop::Ripple(Some(ripple)));
        self
    }

    /// The elevation shadow (Android).
    fn elevation(mut self, elevation: f32) -> Self {
        super::host::set(self.node(), Prop::Elevation(elevation));
        self
    }

    /// The id UI tests find it by.
    fn test_id(mut self, id: &str) -> Self {
        super::host::set(self.node(), Prop::TestId(Some(id.into())));
        self
    }

    /// The id inputs name an input accessory by (`nativeID`).
    fn native_id(mut self, id: &str) -> Self {
        super::host::set(self.node(), Prop::NativeId(Some(id.into())));
        self
    }

    /// Styles while pressed (native: no render).
    fn active(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        let s = f(StyleRefinement::default());
        self.style().extend_as(Variant::ACTIVE, s);
        self
    }

    /// Styles while a pointer hovers it (iPad, Mac, mice).
    fn hover(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        let s = f(StyleRefinement::default());
        self.style().extend_as(Variant::HOVER, s);
        self
    }

    /// Styles while focused.
    fn focus(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        let s = f(StyleRefinement::default());
        self.style().extend_as(Variant::FOCUS, s);
        self
    }

    /// Styles while [`Interactive::disabled`] (Tailwind's `disabled:`).
    fn disabled_style(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        let s = f(StyleRefinement::default());
        self.style().extend_as(Variant::DISABLED, s);
        self
    }

    /// No presses nor press feedback; screen readers say it is disabled.
    fn disabled(mut self, disabled: bool) -> Self {
        let attrs = &mut self.node().attrs;
        attrs.disabled = disabled;
        attrs.state.disabled = disabled;
        self
    }

    /// Which touches it and its children take.
    fn pointer_events(mut self, pointer_events: PointerEvents) -> Self {
        self.node().attrs.pointer_events = pointer_events;
        self
    }

    /// The opacity while pressed (a fade instead of `.active(…)` styles).
    fn active_opacity(mut self, opacity: f32) -> Self {
        self.node().attrs.active_opacity = Some(opacity);
        self
    }
}

macro_rules! element {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone)]
        pub struct $name(ElNode);

        impl Element for $name {
            fn node(&mut self) -> &mut ElNode {
                &mut self.0
            }
        }

        impl IntoElement for $name {
            fn into_element(self) -> AnyElement {
                AnyElement(self.0)
            }
        }

        impl Accessible for $name {}
    };
}

element! {
    /// A view (`<view>`), made by [`view`].
    View
}

element! {
    /// A text block (`<text>`), made by [`text`].
    Text
}

element! {
    /// An image (`<image>`), made by [`image`].
    Image
}

element! {
    /// A slot's node as a render places it ([`ParentElement::slot`]):
    /// style it like a view; its children are the app's.
    SlotElement
}

element! {
    /// A list as a render lays it out ([`ParentElement::list`]).
    ListElement
}

impl Interactive for View {}
impl ParentElement for View {}
impl Interactive for SlotElement {}
impl Interactive for Text {}
impl Interactive for Image {}

/// A text's children are its spans, as nested `<text>`s in JS: texts with
/// styles and presses of their own (a link inside a sentence), strings,
/// and views and images placed inline, wrapping with the words. The core
/// merges them into one paragraph.
impl ParentElement for Text {
    fn child(mut self, child: impl IntoElement) -> Self {
        self.0.children.push(span(child.into_element().0));
        self
    }

    fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.0
            .children
            .extend(children.into_iter().map(|c| span(c.into_element().0)));
        self
    }
}

/// A child of a text: a bare string (a text with nothing of its own) is a
/// piece of its content, anything else a span or an inline element.
fn span(mut el: ElNode) -> ElNode {
    if el.tag == Tag::Text
        && matches!(el.kind, ElKind::Plain)
        && el.key.is_none()
        && el.children.is_empty()
        && el.handlers.is_empty()
        && el.style.is_empty()
        && el.attrs == Attrs::default()
    {
        el.kind = ElKind::Raw;
    }
    el
}

impl Text {
    /// At most `lines` lines (0: no limit), the last one truncated.
    pub fn number_of_lines(mut self, lines: u32) -> Self {
        self.0.attrs.number_of_lines = lines;
        self
    }

    /// Where a truncated line is cut.
    pub fn ellipsize_mode(mut self, mode: crate::EllipsizeMode) -> Self {
        super::host::set(&mut self.0, Prop::EllipsizeMode(mode));
        self
    }

    /// The user can select and copy it.
    pub fn selectable(mut self, selectable: bool) -> Self {
        super::host::set(&mut self.0, Prop::Selectable(Some(selectable)));
        self
    }

    /// The highlight of its selection, when it is selectable.
    pub fn selection_color(mut self, color: Color) -> Self {
        super::host::set(&mut self.0, Prop::SelectionColor(Some(color)));
        self
    }

    /// Shrinks the font, down to `min_scale` of it, to fit its lines.
    pub fn shrink_to_fit(mut self, min_scale: f32) -> Self {
        super::host::set(&mut self.0, Prop::AdjustsFontSizeToFit(true));
        super::host::set(&mut self.0, Prop::MinimumFontScale(min_scale));
        self
    }

    /// Whether its fonts follow the system's text size (true by default;
    /// TS: `allowFontScaling`).
    pub fn allow_font_scaling(mut self, allow: bool) -> Self {
        super::host::set(&mut self.0, Prop::AllowFontScaling(allow));
        self
    }

    /// At most this multiple of its fonts' sizes with the system's text
    /// size (TS: `maxFontSizeMultiplier`; 0: no limit).
    pub fn max_font_size_multiplier(mut self, multiplier: f32) -> Self {
        super::host::set(&mut self.0, Prop::MaxFontSizeMultiplier(multiplier));
        self
    }
}

impl Image {
    /// How it fits its box (default: cover).
    pub fn resize_mode(mut self, mode: crate::ResizeMode) -> Self {
        super::host::set(&mut self.0, Prop::ResizeMode(mode));
        self
    }

    /// Shown until it loads (a URL, an `asset:/` URI or an
    /// [`super::ImageAsset`]).
    pub fn placeholder(mut self, src: impl Into<super::ImageSrc>) -> Self {
        let src = src.into();
        super::host::set(
            &mut self.0,
            Prop::Image(crate::ImageProp::DefaultSource(Some(ImageSource {
                uri: src.uri.to_arc(),
                size: src.size,
            }))),
        );
        self
    }

    /// Draws its opaque pixels in `color` (a template image, an icon).
    pub fn tint_color(mut self, color: impl Into<super::AdaptiveColor>) -> Self {
        self.style()
            .color(&[crate::StyleKey::TintColor], color.into());
        self
    }

    /// Blurs it (points).
    pub fn blur_radius(mut self, radius: f32) -> Self {
        super::host::set(
            &mut self.0,
            Prop::Image(crate::ImageProp::BlurRadius(radius)),
        );
        self
    }

    /// Fades it in over `ms` milliseconds once loaded.
    pub fn fade_duration(mut self, ms: f32) -> Self {
        super::host::set(
            &mut self.0,
            Prop::Image(crate::ImageProp::FadeDuration(Some(ms))),
        );
        self
    }

    /// It loaded: its size in pixels.
    pub fn on_load(self, listener: Listener<Size>) -> Self {
        self.on(EventKind::Load, events::typed(listener, events::size))
    }

    /// It failed to load (the message).
    pub fn on_error(self, listener: Listener<str>) -> Self {
        self.on(EventKind::Error, events::typed(listener, events::message))
    }

    /// It started loading.
    pub fn on_load_start(self, listener: Listener<()>) -> Self {
        self.on(EventKind::LoadStart, events::typed(listener, events::unit))
    }

    /// It finished loading, or failed.
    pub fn on_load_end(self, listener: Listener<()>) -> Self {
        self.on(EventKind::LoadEnd, events::typed(listener, events::unit))
    }

    /// Its download went on: bytes so far and in all (at most every 100
    /// ms; none for images already in memory).
    pub fn on_progress(self, listener: Listener<events::ImageProgress>) -> Self {
        self.on(
            EventKind::Progress,
            events::typed(listener, events::image_progress),
        )
    }
}

impl super::Scrollable for ListElement {}

impl super::ListLayout for ListElement {
    fn config(&mut self) -> &mut ListConfig {
        match &mut self.0.kind {
            ElKind::List(list) => &mut list.config,
            _ => unreachable!("a ListElement is a list"),
        }
    }
}

/// A named slot (an `Option<Slot>` prop): `Some` while the app passes
/// content for it. Placed with [`ParentElement::slot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    name: &'static str,
}

impl Slot {
    pub(crate) fn new(name: &'static str) -> Self {
        Self { name }
    }

    /// Its name (its prop's, in JS).
    pub fn name(&self) -> &'static str {
        self.name
    }
}

/// The app's virtualized rows (an `Items` prop): in JS the component takes
/// `each` and a row renderer like `List`. Placed with
/// [`ParentElement::list`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Items {
    name: &'static str,
}

impl Items {
    #[doc(hidden)]
    pub const fn __new(name: &'static str) -> Self {
        Self { name }
    }
}

/// A piece of the app's content to place: one JSX child
/// ([`ParentElement::children_each`]) or a list row's content
/// ([`ParentElement::list`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Child(ChildRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildRef {
    Js(NodeId),
    Row,
}

impl Child {
    pub(crate) fn js(id: NodeId) -> Self {
        Self(ChildRef::Js(id))
    }

    pub(crate) fn row() -> Self {
        Self(ChildRef::Row)
    }
}

impl IntoElement for Child {
    fn into_element(self) -> AnyElement {
        let mut node = ElNode::new(Tag::View);
        node.kind = match self.0 {
            ChildRef::Js(id) => ElKind::JsChild(id),
            ChildRef::Row => ElKind::Children,
        };
        AnyElement(node)
    }
}

/// A view.
pub fn view() -> View {
    View(ElNode::new(Tag::View))
}

/// A text block showing `content`, then its children (spans:
/// [`ParentElement`] for [`Text`]).
pub fn text(content: impl Into<SharedString>) -> Text {
    let mut node = ElNode::new(Tag::Text);
    node.text = Some(content.into());
    Text(node)
}

/// An image loading `src`: a URL, a file path or `asset:/` URI, or an
/// [`super::ImageAsset`] (`asset!("logo@2x.png")`), which lays out at its
/// natural size before it loads.
pub fn image(src: impl Into<super::ImageSrc>) -> Image {
    let mut node = ElNode::new(Tag::Image);
    node.attrs.src = Some(src.into());
    Image(node)
}

/// The JSX children of a component (a `Children` prop): rendered where the
/// render puts it, among the render's own children.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Children {
    present: bool,
}

impl Children {
    pub(crate) fn new(present: bool) -> Self {
        Self { present }
    }

    /// The app passed none.
    pub fn is_empty(&self) -> bool {
        !self.present
    }
}

impl IntoElement for Children {
    fn into_element(self) -> AnyElement {
        let mut node = ElNode::new(Tag::View);
        node.kind = ElKind::Children;
        AnyElement(node)
    }
}

/// Another component in a render ([`Component::element`]). Its styles go
/// after the component's own, like an app's classes.
pub struct ComponentElement<C: Component> {
    node: ElNode,
    _component: PhantomData<fn() -> C>,
}

impl<C: Component> ComponentElement<C> {
    pub(crate) fn new(props: C::Props) -> Self {
        let mut node = ElNode::new(Tag::View);
        node.kind = ElKind::Component(ComponentEl {
            name: super::component::name_of::<C>(),
            make: super::component::make_instance::<C>,
            spec: ComponentSpec::of::<C>,
            props: Box::new(props),
            on_event: None,
        });
        Self {
            node,
            _component: PhantomData,
        }
    }

    /// Fills its slot `slot` (its prop's JS name: `header`) with `content`.
    pub fn fill(mut self, slot: &'static str, content: impl IntoElement) -> Self {
        let mut holder = ElNode::new(Tag::View);
        holder
            .attrs
            .props
            .push(crate::Prop::ComponentSlot(slot.into()));
        holder.key = Some(format!("slot:{slot}").into());
        holder.children.push(content.into_element().0);
        self.node.children.push(holder);
        self
    }

    /// Fills its `Items` prop with rows of this render (TS: `each` and the
    /// row renderer, `keyExtractor`, `ListHeaderComponent`, ...): the rows,
    /// their keys, header, footer, empty view, separators, sticky rows,
    /// viewability, the refresh and the end reached are `list`'s, rendered
    /// and handled by this component; the layout and the styles are the
    /// placing component's ([`ParentElement::list`]), so `list`'s own are
    /// left out.
    pub fn items(mut self, list: super::VirtualList) -> Self {
        match <C::Props as super::Props>::ITEMS {
            Some(name) => self.node.children.push(list.into_items(name)),
            None => {
                log::error!(target: "js", "{} has no `Items` prop to fill", super::component::name_of::<C>())
            }
        }
        self
    }

    /// Puts `handle` on this component: its updates go to it.
    pub fn component_ref(mut self, handle: &super::ComponentRef<C>) -> Self {
        self.node.attrs.element_ref = Some(handle.element().clone());
        self
    }

    /// Handles its events (they then do not reach the app).
    pub fn on_event(mut self, listener: Listener<C::Event>) -> Self {
        let inner = listener.0;
        if let ElKind::Component(c) = &mut self.node.kind {
            c.on_event = Some(Listener(Rc::new(
                move |this, props, event: &dyn Any, cx| {
                    if let Some(event) = event.downcast_ref::<C::Event>() {
                        inner(this, props, event, cx);
                    }
                },
            )));
        }
        self
    }
}

impl<C: Component> Element for ComponentElement<C> {
    fn node(&mut self) -> &mut ElNode {
        &mut self.node
    }
}

/// The content of a nested component: `.child(..)` / `.children(..)` are
/// its `Children`, [`ComponentElement::fill`] its slots, placed where its
/// render places the app's.
impl<C: Component> ParentElement for ComponentElement<C> {}

impl<C: Component> IntoElement for ComponentElement<C> {
    fn into_element(self) -> AnyElement {
        AnyElement(self.node)
    }
}
