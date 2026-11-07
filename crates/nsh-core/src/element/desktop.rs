//! Desktop windows and menus for Rust components, using the platform's
//! native chrome (TS: `nsh/desktop`'s `Window`, `currentWindow()` and
//! `Menu`).
//!
//! Every window of the app renders from its one engine. [`window`] declares
//! a window of the app's own in the tree, controlled as a dialog is: open
//! while [`Window::open`] holds, its content in the same reactive graph as
//! the rest of the app. [`current_window`] is the window the calling code
//! renders in (the main window outside any [`window`]): a [`WindowHandle`]
//! whose [`WindowHandle::set`] changes its attributes and whose requests
//! ([`WindowHandle::set_mode`], [`WindowHandle::minimize`], ...) the
//! platform carries out, reporting what happened in
//! [`WindowHandle::state`]. [`quit`] and [`restart`] are `Application`'s.
//!
//! A custom title bar hides the system's (`TitleBarOptions` with
//! [`TitleBarStyle::Hidden`](crate::desktop::TitleBarStyle::Hidden)),
//! marks its background with [`Interactive::window_drag`], and reserves the
//! left and right edges reported by [`title_bar`]. The application menu
//! ([`set_application_menu`]) and context menus ([`popup_menu`]) are
//! [`MenuItem`]s whose [`MenuItem::on_select`] runs when the user chooses
//! the item, by a click or its shortcut.
//!
//! An app that opens its window as the user left it saves
//! [`WindowState::placement`] when it changes and gives it back to
//! [`WindowHandle::set_placement`], which fits it on the displays there
//! are now; a [`window`] with [`Window::restore_bounds`] has the core do
//! both.
//!
//! ```ignore
//! let main = current_window();
//! if let Some(saved) = Storage::get_item("window")
//!     .and_then(|json| serde_json::from_str::<WindowPlacement>(&json).ok())
//! {
//!     main.set_placement(saved);
//! }
//! effect(move || {
//!     if let Some(state) = main.state() {
//!         let json = serde_json::to_string(&state.placement).unwrap();
//!         let _ = Storage::set_item("window", json);
//!     }
//! });
//! ```
//!
//! On platforms without desktop windows these have no effect.
//!
//! [`Interactive::window_drag`]: super::Interactive::window_drag

use std::{cell::RefCell, fmt, rc::Rc};

use super::{
    AnyElement, Element, ElementRef, ImageSrc, IntoElement, IntoElements, IntoReactive, Owner,
    ParentElement, Signal, cleanup, menus::MenuHandler, task::Answer,
};
use crate::{
    Command, EventKind, EventPayload,
    desktop::{
        self, CloseReason, DesktopCommand, Display, DisplayId, MenuAction, MenuBarMode, MenuRole,
        Shortcut, WindowAttributes, WindowCloseCause, WindowEvent, WindowFrame, WindowKind,
        WindowMode, WindowOptions, WindowPlacement, WindowPosition, WindowRequest, WindowState,
    },
    geometry::{Bounds, Edges, Point, Points, Size},
    keymap::When,
    platform::Os,
};

/// The current window's ([`current_window`]) custom title bar: its height
/// (`top`) and the widths its native window controls occupy (`left` and
/// `right`), in points (TS: `titleBar()`). Reading subscribes to changes,
/// including entering and leaving full screen. These are zero with a
/// system title bar and on platforms without desktop windows.
pub fn title_bar() -> Edges<Points> {
    current_window().title_bar()
}

/// When the menu bar shows on Windows and Linux where the window draws the
/// application menu as a bar above the app (the GPUI backend):
/// [`MenuBarMode::Shown`] (the default) always; [`MenuBarMode::AutoHide`]
/// only after Alt is pressed and released alone, until Alt alone hides it
/// again. F10 and Alt with a menu's initial open a menu either way, and the
/// bar shows while it is open. macOS's menu bar is the system's (TS:
/// `Menu.setMenuBar`).
pub fn set_menu_bar(mode: MenuBarMode) {
    super::desktop_command(DesktopCommand::SetMenuBar(mode));
}

/// The current window's state (TS: `windowState()`):
/// [`WindowHandle::state`] of [`current_window`].
pub fn window_state() -> Option<WindowState> {
    current_window().state()
}

/// The displays (TS: `displays()`): their bounds and the parts of them
/// windows use, in screen coordinates. Reading subscribes: plugging in a
/// display, unplugging one or arranging them again changes them. Empty
/// off desktops.
pub fn displays() -> Vec<Display> {
    super::ambient::displays()
}

/// The display with `id`, of [`displays`] (reading subscribes).
pub fn display(id: DisplayId) -> Option<Display> {
    displays().into_iter().find(|d| d.id == id)
}

// --- Windows ---------------------------------------------------------------------------

/// Where a [`WindowHandle`]'s commands go.
#[derive(Clone, Copy)]
enum Target {
    /// The main window: the root.
    Main,
    /// A window of the app's own: the dialog root carrying the ref.
    Own(ElementRef),
}

/// A window (TS: `WindowRef`): the main window ([`main_window`]) or one of
/// the app's own ([`window`]), as [`current_window`] gives it. Setting its
/// attributes and its requests are commands that go out with the commit;
/// the platform may decline or adjust a request, and [`Self::state`]
/// reports what happened. Commands to a closed window do nothing.
#[derive(Clone)]
pub struct WindowHandle {
    id: Rc<str>,
    target: Target,
    /// The state of a window of the app's own (the main window's is the
    /// environment's).
    state: Option<Signal<Option<WindowState>>>,
    /// The hidden title bar's area of a window of the app's own (the main
    /// window's is the environment's).
    title_bar: Option<Signal<Edges<Points>>>,
    /// Closes a window of the app's own as its element does
    /// ([`WindowHandle::close`]).
    close: Option<Rc<dyn Fn(WindowCloseCause)>>,
    /// The close presses of a window of the app's own: how many, and the
    /// last one's reason (the main window's are the environment's).
    close_press: Option<Signal<(u64, CloseReason)>>,
}

impl fmt::Debug for WindowHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowHandle")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for WindowHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// The main window.
pub fn main_window() -> WindowHandle {
    WindowHandle {
        id: Rc::from("main"),
        target: Target::Main,
        state: None,
        title_bar: None,
        close: None,
        close_press: None,
    }
}

/// The window the calling code renders in (TS: `currentWindow()`): the
/// nearest [`window`] around it, else the main window. Never the focused
/// window. Call it while rendering and keep the handle: listeners and
/// tasks find no window around them.
pub fn current_window() -> WindowHandle {
    super::use_context::<WindowHandle>().unwrap_or_else(main_window)
}

impl WindowHandle {
    /// Its id: the app's (`"main"` for the main window).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// It is the main window.
    pub fn is_main(&self) -> bool {
        matches!(self.target, Target::Main)
    }

    fn command(&self, command: DesktopCommand) {
        match self.target {
            Target::Main => super::ambient::desktop_command(command),
            Target::Own(element) => element.send(Command::Desktop(Box::new(command))),
        }
    }

    /// What the platform reports of it (reading subscribes): its mode,
    /// bounds, display, whether it is focused, minimized or visible.
    /// `None` off desktops, while it is not open, and once it closed.
    pub fn state(&self) -> Option<WindowState> {
        match self.state {
            None => super::ambient::window_state(),
            Some(state) => state.get(),
        }
    }

    /// Changes the attributes `attributes` sets (TS: `set(partial)`); the
    /// others keep their values. One command per window and commit carries
    /// what changed. An attribute the app's capabilities do not grant is
    /// left as it is, with a warning.
    pub fn set(&self, attributes: WindowAttributes) {
        if attributes.is_empty() {
            return;
        }
        self.command(DesktopCommand::SetAttributes(Box::new(attributes)));
    }

    /// Shows it, restoring it when minimized.
    pub fn show(&self) {
        self.command(DesktopCommand::Request(WindowRequest::Show));
    }

    /// Hides it.
    pub fn hide(&self) {
        self.command(DesktopCommand::Request(WindowRequest::Hide));
    }

    /// Shows it, brings it to the front of its level and focuses it.
    pub fn focus(&self) {
        self.command(DesktopCommand::Request(WindowRequest::Focus));
    }

    /// Closes it, whatever `close_on_press` says. A [`window`] reports it
    /// to [`Window::on_open_change`] with [`WindowCloseCause::Close`], so
    /// the app's `open` state follows. The main window closes as when the
    /// user closes it: the app ends with its last window (an embedding host
    /// decides).
    pub fn close(&self) {
        match &self.close {
            Some(close) => close(WindowCloseCause::Close),
            None => self.command(DesktopCommand::Request(WindowRequest::Close)),
        }
    }

    /// Its hidden title bar's area (reading subscribes): the bar's height
    /// (`top`) and the widths its window controls take (`left`, `right`),
    /// as [`title_bar`] gives the main window's.
    pub fn title_bar(&self) -> Edges<Points> {
        match self.title_bar {
            Some(area) => area.get(),
            None => super::title_bar(),
        }
    }

    /// Minimizes it.
    pub fn minimize(&self) {
        self.command(DesktopCommand::Request(WindowRequest::Minimize));
    }

    /// Brings it back from being minimized, in the mode it had.
    pub fn restore(&self) {
        self.command(DesktopCommand::Request(WindowRequest::Restore));
    }

    /// Puts it in `mode`; doing it again changes nothing.
    pub fn set_mode(&self, mode: WindowMode) {
        self.command(DesktopCommand::Request(WindowRequest::SetMode(mode)));
    }

    /// Moves and sizes it in points, in screen coordinates; fields left out
    /// keep their values. Widths and heights take positive finite numbers,
    /// `x` and `y` finite ones: other values keep the field's value.
    pub fn set_bounds(&self, frame: WindowFrame) {
        self.command(DesktopCommand::Request(WindowRequest::SetBounds(
            WindowFrame {
                x: frame.x.filter(|v| v.is_finite()),
                y: frame.y.filter(|v| v.is_finite()),
                width: frame.width.filter(|v| v.is_finite() && v.0 > 0.0),
                height: frame.height.filter(|v| v.is_finite() && v.0 > 0.0),
                animated: frame.animated,
            },
        )));
    }

    /// Restores a placement saved earlier ([`WindowState::placement`]):
    /// restored to its bounds, then maximized or in full screen when its
    /// mode says so. The bounds are first fitted on the displays there are
    /// now: unchanged when at least 100 × 20 points of the window's top 40
    /// points are on a display's visible bounds, where the user can take
    /// it by its title bar; otherwise centered on the primary display's
    /// visible bounds, no larger than them.
    pub fn set_placement(&self, placement: WindowPlacement) {
        self.command(DesktopCommand::Request(WindowRequest::SetPlacement(
            placement,
        )));
    }

    /// Centers it on `display` (its own with `None`).
    pub fn center(&self, display: Option<DisplayId>) {
        self.command(DesktopCommand::Request(WindowRequest::Center { display }));
    }

    /// Asks for the user's attention: the Dock's icon bounces, the taskbar
    /// button flashes; `critical` until the user activates the app.
    pub fn request_attention(&self, critical: bool) {
        self.command(DesktopCommand::Request(WindowRequest::RequestAttention {
            critical,
        }));
    }

    /// Runs `f` with what changed whenever `changed` finds a change between
    /// two states, under the current owner.
    fn on_change<T: 'static>(
        &self,
        changed: impl Fn(Option<&WindowState>, &WindowState) -> Option<T> + 'static,
        f: impl Fn(T) + 'static,
    ) {
        let this = self.clone();
        let last = super::stored(None::<WindowState>);
        super::effect(move || {
            let Some(next) = this.state() else { return };
            let previous = last.replace(Some(next));
            if let Some(change) = changed(previous.as_ref(), &next) {
                super::untracked(|| f(change));
            }
        });
    }

    /// Runs `f` with the reason each time the user asks to close it (its
    /// close button, a shortcut, a menu, the taskbar, the system), under
    /// the current owner (TS: `on("closePress", …)`). With
    /// `close_on_press: false` it stays open: save or ask, then
    /// [`Self::close`].
    pub fn on_close_press(&self, f: impl Fn(CloseReason) + 'static) {
        let signal = self
            .close_press
            .unwrap_or_else(super::ambient::main_close_press_signal);
        let seen = super::stored(signal.with_untracked(|(count, _)| *count));
        super::effect(move || {
            let (count, reason) = signal.get();
            if seen.replace(count) != count {
                super::untracked(|| f(reason));
            }
        });
    }

    /// Runs `f` with `focused` when it gains or loses the focus.
    pub fn on_focus_change(&self, f: impl Fn(bool) + 'static) {
        self.on_change(
            |old, new| (old.is_some_and(|o| o.focused != new.focused)).then_some(new.focused),
            f,
        );
    }

    /// Runs `f` with its mode and whether it is minimized when either
    /// changes.
    pub fn on_mode_change(&self, f: impl Fn(WindowMode, bool) + 'static) {
        self.on_change(
            |old, new| {
                old.is_some_and(|o| o.mode != new.mode || o.minimized != new.minimized)
                    .then_some((new.mode, new.minimized))
            },
            move |(mode, minimized)| f(mode, minimized),
        );
    }

    /// Runs `f` with its bounds once a move or a resize settles.
    pub fn on_bounds_change(&self, f: impl Fn(Bounds<Points>) + 'static) {
        self.on_change(
            |old, new| (old.is_some_and(|o| o.bounds != new.bounds)).then_some(new.bounds),
            f,
        );
    }

    /// Runs `f` with the display it moved to.
    pub fn on_display_change(&self, f: impl Fn(DisplayId) + 'static) {
        self.on_change(
            |old, new| {
                (old.is_some_and(|o| o.display != new.display))
                    .then_some(new.display)
                    .flatten()
            },
            f,
        );
    }
}

/// Why a [`window`] closed ([`Window::on_open_change`]).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OpenChange {
    /// Whether it is open now (always `false`: the app opens windows).
    pub open: bool,
    /// What closed it.
    pub reason: WindowCloseCause,
}

type OpenChangeFn = Rc<dyn Fn(&OpenChange)>;
type ClosePressFn = Rc<dyn Fn(CloseReason)>;
type BindFn = Rc<dyn Fn(&mut WindowAttributes)>;
type ContentFn = Rc<dyn Fn() -> Vec<AnyElement>>;
type HandleSlot = Rc<RefCell<Option<WindowHandle>>>;

/// A window of the app's own in the tree (TS: `<Window>`), made by
/// [`window`]: open while [`Self::open`] holds and the user has not closed
/// it, its content made each time it opens.
pub struct Window {
    options: WindowOptions,
    bound: Vec<BindFn>,
    reactive: Vec<super::Reactive<WindowAttributes>>,
    open: Option<super::Reactive<bool>>,
    content: Option<ContentFn>,
    on_open_change: Option<OpenChangeFn>,
    on_close_press: Option<ClosePressFn>,
    handle: Option<HandleSlot>,
}

/// A window of the app's own (TS: `<Window>`): a window the platform shows
/// its content in, rendered from the app's one engine, so it shares the
/// app's signals, context and theme. Give it an [`Window::id`] (one window
/// per id), its [`Window::kind`] and its content; it opens while
/// [`Window::open`] holds (always, without it) and closes when it leaves
/// the tree.
///
/// ```ignore
/// window()
///     .id("inspector")
///     .kind(WindowKind::Panel)
///     .title("Inspector")
///     .default_size(size(px(320.), px(560.)))
///     .open(inspector)
///     .on_open_change(move |e| inspector.set(e.open))
///     .content(inspector_panel)
/// ```
///
/// Off the desktops it shows nothing.
pub fn window() -> Window {
    Window {
        options: WindowOptions::default(),
        bound: Vec::new(),
        reactive: Vec::new(),
        open: None,
        content: None,
        on_open_change: None,
        on_close_press: None,
        handle: None,
    }
}

impl Window {
    /// The app's id for it: one window per id, and the key of its kept
    /// placement ([`Self::restore_bounds`]).
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.options.id = id.into();
        self
    }

    /// What it is for: its attributes' defaults.
    pub fn kind(mut self, kind: WindowKind) -> Self {
        self.options.kind = kind;
        self
    }

    /// The id of the window that owns it (`"main"`: the main window).
    pub fn parent(mut self, id: impl Into<String>) -> Self {
        self.options.parent = Some(id.into());
        self
    }

    /// Its parent takes no input while it is open (dialogs).
    pub fn modal(mut self, modal: bool) -> Self {
        self.options.modal = modal;
        self
    }

    /// Its content size when it opens.
    pub fn default_size(mut self, size: Size<Points>) -> Self {
        self.options.default_size = Some(size);
        self
    }

    /// Where it opens.
    pub fn default_position(mut self, position: WindowPosition) -> Self {
        self.options.default_position = Some(position);
        self
    }

    /// The display it opens on.
    pub fn display(mut self, display: DisplayId) -> Self {
        self.options.display = Some(display);
        self
    }

    /// The mode it opens in.
    pub fn default_mode(mut self, mode: WindowMode) -> Self {
        self.options.default_mode = Some(mode);
        self
    }

    /// Opens where the user left it: the core keeps its placement under its
    /// id and fits it on the displays there are when it opens again.
    pub fn restore_bounds(mut self, restore: bool) -> Self {
        self.options.restore_bounds = restore;
        self
    }

    /// Its title (a value, a signal or a closure).
    pub fn title(mut self, title: impl IntoReactive<String>) -> Self {
        match title.into_reactive() {
            super::Reactive::Static(title) => self.options.attributes.title = Some(title),
            reactive => {
                let reactive = Rc::new(reactive);
                self.bound.push(Rc::new(move |a: &mut WindowAttributes| {
                    a.title = Some(reactive.get());
                }));
            }
        }
        self
    }

    /// Any of its attributes (a value, a signal or a closure): the fields
    /// set, over its kind's defaults; given again, the later fields win.
    pub fn attributes(mut self, attributes: impl IntoReactive<WindowAttributes>) -> Self {
        match attributes.into_reactive() {
            super::Reactive::Static(a) => self.options.attributes.merge(&a),
            reactive => self.reactive.push(reactive),
        }
        self
    }

    /// Whether it is open (a value, a signal or a closure).
    pub fn open(mut self, open: impl IntoReactive<bool>) -> Self {
        self.open = Some(open.into_reactive());
        self
    }

    /// Runs `f` when the window closed while `open` held (the user's close
    /// press with `close_on_press`, Escape, losing the focus, its parent
    /// closing, [`WindowHandle::close`]): the app sets its `open` state
    /// from it.
    pub fn on_open_change(mut self, f: impl Fn(&OpenChange) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Runs `f` when the user asks to close it (its close button, the
    /// close key), whether or not `close_on_press` lets it close.
    pub fn on_close_press(mut self, f: impl Fn(CloseReason) + 'static) -> Self {
        self.on_close_press = Some(Rc::new(f));
        self
    }

    /// What it shows, made each time it opens; [`current_window`] in it is
    /// this window.
    pub fn content<E: IntoElements>(mut self, content: impl Fn() -> E + 'static) -> Self {
        self.content = Some(Rc::new(move || content().into_elements()));
        self
    }

    /// Fills `slot` with its handle while it is open (`None` after).
    pub fn handle(mut self, slot: Rc<RefCell<Option<WindowHandle>>>) -> Self {
        self.handle = Some(slot);
        self
    }
}

impl IntoElement for Window {
    fn into_element(self) -> AnyElement {
        if !super::os().is_desktop() {
            return super::when(false, Vec::<AnyElement>::new).into_element();
        }
        let Window {
            options,
            bound,
            reactive,
            open,
            content,
            on_open_change,
            on_close_press,
            handle,
        } = self;
        let open = open.unwrap_or(super::Reactive::Static(true));
        // Closed by the user while `open` stayed true: hidden until the app
        // follows (then `open` opens it again).
        let closed = super::signal(false);
        let open = Rc::new(open);
        let is_open = {
            let open = open.clone();
            move || {
                let open = open.get();
                if !open {
                    super::untracked(|| closed.set(false));
                }
                open && !closed.get()
            }
        };
        let options = Rc::new(options);
        let bound = Rc::new(bound);
        let reactive = Rc::new(reactive);
        super::when(is_open, move || {
            let element = super::element_ref();
            let state = super::signal(None::<WindowState>);
            let title_bar = super::signal(Edges::<Points>::default());
            let close_press = super::signal((0u64, CloseReason::default()));
            let close: Rc<dyn Fn(WindowCloseCause)> = {
                let on_open_change = on_open_change.clone();
                Rc::new(move |cause: WindowCloseCause| {
                    closed.set(true);
                    if let Some(f) = &on_open_change {
                        f(&OpenChange {
                            open: false,
                            reason: cause,
                        });
                    }
                })
            };
            let this = WindowHandle {
                id: Rc::from(options.id.as_str()),
                target: Target::Own(element),
                state: Some(state),
                title_bar: Some(title_bar),
                close: Some(close.clone()),
                close_press: Some(close_press),
            };
            if let Some(slot) = &handle {
                *slot.borrow_mut() = Some(this.clone());
                let slot = slot.clone();
                cleanup(move || *slot.borrow_mut() = None);
            }
            super::provide_context(this);
            let mut modal = super::modal().element_ref(&element);
            {
                let node = modal.node();
                let dynamic = !bound.is_empty() || !reactive.is_empty();
                let options = options.clone();
                let bound = bound.clone();
                let reactive = reactive.clone();
                let resolve = move || {
                    let mut o = (*options).clone();
                    for b in bound.iter() {
                        b(&mut o.attributes);
                    }
                    for r in reactive.iter() {
                        o.attributes.merge(&r.get());
                    }
                    o
                };
                let apply = |attrs: &mut super::Attrs, o: WindowOptions| {
                    attrs.window = Some(Box::new(o));
                };
                if !dynamic {
                    node.bind_attr(super::Reactive::Static(resolve()), apply);
                } else {
                    node.bind_attr(super::Reactive::Bound(Rc::new(resolve)), apply);
                }
                let on_close_press = on_close_press.clone();
                let close_on_dismiss = close.clone();
                node.handlers.push((
                    EventKind::Window,
                    super::Listener::from(move |payload: &EventPayload| {
                        let EventPayload::Window(event) = payload else {
                            return;
                        };
                        match event.as_ref() {
                            WindowEvent::State(next) => state.set(Some(**next)),
                            WindowEvent::TitleBar(area) => title_bar.set(*area),
                            WindowEvent::ClosePress { reason } => {
                                if let Some(f) = &on_close_press {
                                    f(*reason);
                                }
                                close_press.update(|(count, last)| {
                                    *count += 1;
                                    *last = *reason;
                                });
                            }
                            WindowEvent::Closed { cause } => close(*cause),
                        }
                    }),
                ));
                // The platforms that close windows themselves report it so.
                node.handlers.push((
                    EventKind::Dismiss,
                    super::Listener::from(move |_: &EventPayload| {
                        close_on_dismiss(WindowCloseCause::ClosePress)
                    }),
                ));
            }
            if let Some(content) = &content {
                modal = modal.children(content());
            }
            modal
        })
        .into_element()
    }
}

/// Quits the app, as the application menu's Quit item
/// ([`MenuRole::Quit`]) and the platform's quit key do (TS: `Application.quit`):
/// its windows close and the engine stops as when the last window closes
/// (an embedding app's close handling runs as it does for the Quit item),
/// then the app's run ends, also when closing the window alone would leave
/// it running (a window kept hidden for the Dock, a tray icon). As with the
/// Quit item, the app cannot cancel it; a GPUI host that owns the window is
/// asked first and quits when it is ready. For a menu item or a button of
/// the app's own that quits, and for restarting after an update. A view
/// embedded in another app leaves the app's lifecycle to that app. On
/// platforms without desktop windows this has no effect.
pub fn quit() {
    super::desktop_command(DesktopCommand::Quit);
}

/// Restarts the app (TS: `Application.restart`): it quits as [`quit`]
/// does, and once this process has exited the platform starts the same
/// program again with the arguments it was started with. For a setting
/// that takes effect at the next start (the app's language, say): the app
/// asks the user first, then calls this. Standalone apps on macOS, Windows
/// and Linux; a view embedded in another app leaves the app's lifecycle to
/// that app, and on other platforms this has no effect.
pub fn restart() {
    super::desktop_command(DesktopCommand::Restart);
}

/// An item of a menu (TS: `nsh/desktop`'s `MenuItem`): an item that runs
/// its [`on_select`](Self::on_select) or carries out its
/// [`role`](Self::role), a separator, or a submenu.
///
/// ```ignore
/// MenuItem::submenu("File", [
///     MenuItem::new("New").shortcut("mod+n").on_select(new_document),
///     MenuItem::separator(),
///     MenuItem::role(MenuRole::Quit),
/// ])
/// ```
#[derive(Clone)]
pub struct MenuItem {
    kind: Kind,
    label: String,
    role: Option<MenuRole>,
    /// The keystroke as written, checked.
    shortcut: Option<Rc<str>>,
    when: When,
    enabled: bool,
    checked: Option<bool>,
    icon: Option<ImageSrc>,
    on_select: Option<MenuHandler>,
}

#[derive(Clone)]
enum Kind {
    Action,
    Separator,
    Submenu(Vec<MenuItem>),
}

impl MenuItem {
    fn of(kind: Kind, label: String) -> Self {
        MenuItem {
            kind,
            label,
            role: None,
            shortcut: None,
            when: When::Always,
            enabled: true,
            checked: None,
            icon: None,
            on_select: None,
        }
    }

    /// An item showing `label`.
    pub fn new(label: impl Into<String>) -> Self {
        Self::of(Kind::Action, label.into())
    }

    /// A standard item the platform carries out, with the platform's label
    /// and shortcut unless it is given its own.
    pub fn role(role: MenuRole) -> Self {
        MenuItem {
            role: Some(role),
            ..Self::of(Kind::Action, String::new())
        }
    }

    /// A line between groups of items.
    pub fn separator() -> Self {
        Self::of(Kind::Separator, String::new())
    }

    /// An item opening a menu of `items` (the top-level menus of the
    /// application menu: `File`, `Edit`, ...).
    pub fn submenu(label: impl Into<String>, items: impl IntoIterator<Item = MenuItem>) -> Self {
        Self::of(Kind::Submenu(items.into_iter().collect()), label.into())
    }

    /// What it shows, in place of its role's label.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Its keyboard shortcut, as `nsh/keymap` writes keystrokes: `"mod+n"`,
    /// `"mod+shift+z"`, `"alt+f4"`, `"f11"` (`mod` is Command on macOS and
    /// Control elsewhere; [`Shortcut::parse`]).
    ///
    /// # Panics
    ///
    /// On a keystroke that is not one, as TS's menus throw.
    #[track_caller]
    pub fn shortcut(mut self, keystroke: &str) -> Self {
        if let Err(e) = Shortcut::parse(keystroke, false) {
            panic!("MenuItem::shortcut: {e}");
        }
        self.shortcut = Some(keystroke.into());
        self
    }

    /// When its shortcut chooses it while a text field has the keyboard
    /// focus ([`When::Always`] by default), on the GPUI backend.
    pub fn shortcut_when(mut self, when: When) -> Self {
        self.when = when;
        self
    }

    /// It cannot be chosen (or opened): shown dimmed.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.enabled = !disabled;
        self
    }

    /// A check mark: makes it a checkable item, checked or not.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// The image beside its label, or beside a submenu's (the top-level
    /// menus of a menu bar show their titles alone): a URL or an
    /// [`asset!`](crate::asset)'s image, as [`image`](crate::element::image)
    /// takes, fitted in the menu's icon size for the screen. An SVG takes
    /// the color of the menu's text; a bitmap keeps its colors. It loads
    /// off the main thread and shows once it has (one that fails to load
    /// leaves the item without an icon, with a warning). GTK's menus show
    /// no icons beside labels, and a terminal shows no images.
    pub fn icon(mut self, src: impl Into<ImageSrc>) -> Self {
        self.icon = Some(src.into());
        self
    }

    /// Runs `f` when it is chosen, by a click or its shortcut, under the
    /// owner that showed the menu.
    pub fn on_select(mut self, f: impl Fn() + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
}

impl fmt::Debug for MenuItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::Separator => f.write_str("Separator"),
            Kind::Submenu(items) => f
                .debug_struct("Submenu")
                .field("label", &self.label)
                .field("items", items)
                .finish_non_exhaustive(),
            Kind::Action => f
                .debug_struct("MenuItem")
                .field("label", &self.label)
                .field("role", &self.role)
                .field("shortcut", &self.shortcut)
                .finish_non_exhaustive(),
        }
    }
}

/// Makes the platform's items of `items`, keeping their handlers (run
/// under `owner`) in `handlers` with the ids it gives them.
fn decode(
    items: Vec<MenuItem>,
    mac: bool,
    owner: &Owner,
    handlers: &mut Vec<(u32, MenuHandler)>,
) -> Vec<desktop::MenuItem> {
    let menus = super::view_menus();
    items
        .into_iter()
        .map(|item| match item.kind {
            Kind::Separator => desktop::MenuItem::Separator,
            Kind::Submenu(items) => desktop::MenuItem::Submenu {
                label: item.label,
                enabled: item.enabled,
                items: decode(items, mac, owner, handlers),
                icon: item.icon.as_ref().and_then(ImageSrc::menu_icon),
            },
            Kind::Action => {
                let id = match item.on_select {
                    Some(f) => {
                        let id = menus.borrow_mut().next_id();
                        let owner = *owner;
                        handlers.push((id, Rc::new(move || owner.run(&*f))));
                        id
                    }
                    None => 0,
                };
                let shortcut = match item.shortcut {
                    // Checked when it was given.
                    Some(text) => Shortcut::parse(&text, mac).ok(),
                    None => item.role.and_then(|r| r.shortcut(mac)),
                };
                desktop::MenuItem::Action(MenuAction {
                    id,
                    label: item.label,
                    shortcut,
                    enabled: item.enabled,
                    checked: item.checked,
                    role: item.role,
                    when: item.when,
                    icon: item.icon.as_ref().and_then(ImageSrc::menu_icon),
                })
            }
        })
        .collect()
}

/// Whether the app runs on a desktop OS, where menus show (as TS's).
fn desktop_os() -> bool {
    super::os().is_desktop()
}

/// Sets the application menu, a submenu per top-level item (`File`,
/// `Edit`, ...), or with `None` restores the platform's default (TS:
/// `Menu.setApplicationMenu`). On macOS it is the menu bar, whose first
/// menu is the app menu (titled with the app's name); on Windows and Linux
/// the window's menu bar, not shown while the title bar is hidden (its
/// shortcuts still work: a title bar of the app's own shows its menus with
/// [`popup_menu`]). Call it again to change it; in an [`effect`] it follows
/// the signals it reads. The items' handlers run while the current owner
/// lives: the menu stays after it, inert, until it is set again.
///
/// [`effect`]: super::effect
pub fn set_application_menu(items: impl Into<Option<Vec<MenuItem>>>) {
    if !desktop_os() {
        return;
    }
    let menus = super::view_menus();
    let Some(items) = items.into() else {
        let mut m = menus.borrow_mut();
        m.app_generation += 1;
        let old = std::mem::take(&mut m.app);
        drop(m);
        drop(old);
        super::desktop_command(DesktopCommand::SetMenu(None));
        return;
    };
    let owner = Owner::current();
    let mut handlers = Vec::new();
    let items = decode(items, super::os() == Os::MacOs, &owner, &mut handlers);
    let generation = {
        let mut m = menus.borrow_mut();
        m.app_generation += 1;
        let old = std::mem::replace(&mut m.app, handlers);
        let generation = m.app_generation;
        drop(m);
        drop(old);
        generation
    };
    let weak = Rc::downgrade(&menus);
    cleanup(move || {
        if let Some(menus) = weak.upgrade() {
            let mut m = menus.borrow_mut();
            if m.app_generation == generation {
                let old = std::mem::take(&mut m.app);
                drop(m);
                drop(old);
            }
        }
    });
    super::desktop_command(DesktopCommand::SetMenu(Some(items.into())));
}

/// Shows a context menu of `items` at the pointer (TS: `Menu.popup`). The
/// chosen item's [`MenuItem::on_select`] runs under the current owner. The
/// answer comes once the menu closes, chosen or dismissed (at once off
/// desktops), so a button that opened it can look pressed until then; it
/// need not be awaited.
pub fn popup_menu(items: Vec<MenuItem>) -> Answer<()> {
    popup(items, None)
}

/// As [`popup_menu`], with the menu's top left corner at `at` (the
/// window's coordinates, in points: an
/// [`on_context_menu`](super::Interactive::on_context_menu)'s
/// `page_position`; TS: `Menu.popup(items, at)`).
pub fn popup_menu_at(items: Vec<MenuItem>, at: Point<Points>) -> Answer<()> {
    popup(items, Some(at))
}

fn popup(items: Vec<MenuItem>, at: Option<Point<Points>>) -> Answer<()> {
    let (answer, slot) = Answer::new();
    if !desktop_os() {
        slot.borrow_mut().deliver(());
        return answer;
    }
    let menus = super::view_menus();
    let owner = Owner::current();
    let mut handlers = Vec::new();
    let items = decode(items, super::os() == Os::MacOs, &owner, &mut handlers);
    let generation = {
        let mut m = menus.borrow_mut();
        // One context menu at a time: one shown before is closed by now.
        m.settle_popup();
        m.closed = Some(slot);
        m.popup_generation += 1;
        let old = std::mem::replace(&mut m.popup, handlers);
        let generation = m.popup_generation;
        drop(m);
        drop(old);
        generation
    };
    let weak = Rc::downgrade(&menus);
    cleanup(move || {
        if let Some(menus) = weak.upgrade() {
            let mut m = menus.borrow_mut();
            if m.popup_generation == generation {
                let old = std::mem::take(&mut m.popup);
                drop(m);
                drop(old);
            }
        }
    });
    super::desktop_command(DesktopCommand::PopupMenu {
        items: items.into(),
        at,
    });
    answer
}
