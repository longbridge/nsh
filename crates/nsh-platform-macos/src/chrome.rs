//! The window's chrome (`nsh/desktop`): its title, the hidden title bar,
//! drag regions, the application menu and context menus.
//!
//! - **Hidden title bar** ([`TitleBarStyle::Hidden`]): the window gets a
//!   full-size content view under a transparent, untitled title bar, so the
//!   app draws up to the window's top edge while the traffic lights stay.
//!   The environment's `title_bar` reports the bar's height (the window's
//!   frame above its content layout rectangle) and the traffic lights'
//!   width; it is zero in full screen, where AppKit shows the bar over the
//!   app only when the pointer reaches the top of the screen.
//! - **Drag regions**: a left press on a view marked `windowDrag`
//!   ([`crate::input::window_drag`]) moves the window with
//!   `performWindowDragWithEvent:` (AppKit's own drag, with snapping and
//!   Mission Control), and a double click does what the user chose in
//!   System Settings (`AppleActionOnDoubleClick`: zoom, minimize or
//!   nothing). The press never reaches the app. This works with the
//!   system's title bar too.
//! - **Menus** are `NSMenu`s: the application menu is the app's main menu
//!   (its first submenu is the app menu, which AppKit titles with the app's
//!   name), context menus pop up in the root view. Items with a role use
//!   AppKit's standard actions with no target, so the responder chain
//!   carries them out (the focused field's `copy:`, the application's
//!   `terminate:`); an item with a role is never reported to the app.
//!   Other items target an [`MenuTarget`] of the surface, which reports
//!   their id as [`EventKind::Menu`]. AppKit enables the items as it
//!   validates the menu, as it does its own: a role is enabled when the
//!   responder chain can carry it out (`Undo` with something to undo,
//!   `Paste` in a field), and gets AppKit's title for the state (`Exit
//!   Full Screen`). A disabled item's action is one nothing answers. The
//!   main menu the app replaced is kept and comes back with
//!   `setApplicationMenu(null)`.
//! - Context menus track the mouse in a loop of their own, so they are
//!   shown from the main queue once the batch is applied, never while it
//!   is.

use std::{
    cell::{Cell, RefCell},
    ptr::NonNull,
    rc::Rc,
    sync::Arc,
};

use block2::RcBlock;
use dispatch2::DispatchQueue;
use nsh_core::{
    EventKind, EventPayload, PayloadValue, ROOT_ID,
    desktop::{
        DesktopCommand, Key, MenuAction, MenuItem, MenuRole, Shortcut, TitleBarStyle, WindowFrame,
        WindowLevel,
    },
    platform::Insets,
};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Retained, Weak},
    runtime::{ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::{
    NSApplication, NSButton, NSEvent, NSEventModifierFlags, NSFloatingWindowLevel, NSMenu,
    NSMenuItem, NSNormalWindowLevel, NSRunningApplication, NSViewFrameDidChangeNotification,
    NSWindow, NSWindowButton, NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_core_graphics::kCGDesktopIconWindowLevel;
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSUserDefaults, ns_string,
};

use crate::surface::Surface;

/// The title bar's height when AppKit cannot tell (macOS 11 and later).
const TITLE_BAR_HEIGHT: f32 = 28.0;
/// The traffic lights' width when AppKit cannot tell.
const TRAFFIC_LIGHTS_WIDTH: f32 = 78.0;

thread_local! {
    /// The app's name for the app menu's roles ("Quit Gallery"), set by
    /// [`crate::run_app`].
    static APP_NAME: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The main menu before an app set its own: `setApplicationMenu(null)`
    /// brings it back.
    static ORIGINAL_MENU: RefCell<Option<Option<Retained<NSMenu>>>> = const { RefCell::new(None) };
}

/// Names the app for the app menu's roles.
pub(crate) fn set_app_name(name: &str) {
    APP_NAME.with(|n| *n.borrow_mut() = Some(name.to_owned()));
}

fn app_name() -> String {
    APP_NAME
        .with(|n| n.borrow().clone())
        .or_else(|| {
            NSRunningApplication::currentApplication()
                .localizedName()
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

define_class!(
    /// The target of a surface's menu items: reports the chosen item's tag
    /// (its id) to the surface with its key.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = u64]
    #[name = "NshMenuTarget"]
    pub(crate) struct MenuTarget;

    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        #[unsafe(method(menuItemChosen:))]
        fn chosen(&self, item: &NSMenuItem) {
            let Some(surface) = Surface::with_key(*self.ivars()) else {
                return;
            };
            let id = item.tag();
            if id > 0 {
                surface.event(
                    ROOT_ID,
                    EventKind::Menu,
                    EventPayload::Map(vec![("id", PayloadValue::Number(id as f64))]),
                );
            }
        }
    }
);

/// A surface's chrome.
#[derive(Default)]
pub(crate) struct Chrome {
    style: Cell<TitleBarStyle>,
    /// The app's title bar's height (`Window.setTitleBar`'s).
    height: Cell<Option<f32>>,
    /// The target of its menu items (made with the first menu).
    target: RefCell<Option<Retained<MenuTarget>>>,
    /// The main menu it set (`setApplicationMenu`).
    menu: RefCell<Option<Retained<NSMenu>>>,
    /// Watches the window controls' frames: AppKit lays the title bar out
    /// again whenever it likes (the window becoming key, its title, ...),
    /// which puts them back at the top.
    controls: RefCell<Option<ControlsWatch>>,
    /// The traffic lights are being moved (their frame changes are ours).
    centering: Cell<bool>,
    /// A frame changed while they were: they are centered again.
    recenter: Cell<bool>,
    /// They are centered again once AppKit's layout pass is over.
    deferred: Cell<bool>,
    /// The title bar's own height, before an app's height replaced it:
    /// without one, the controls go back there.
    natural: Cell<Option<f32>>,
}

impl Drop for Chrome {
    fn drop(&mut self) {
        // An app view that goes (an embedding app closed it) takes its
        // menu with it, and the one it replaced comes back.
        let (Some(bar), Some(mtm)) = (self.menu.take(), MainThreadMarker::new()) else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        if app.mainMenu().is_some_and(|m| m == bar) {
            restore_menu(&app);
        }
    }
}

/// The frame observers of a window's controls.
struct ControlsWatch {
    window: Weak<NSWindow>,
    observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl Drop for ControlsWatch {
    fn drop(&mut self) {
        let center = NSNotificationCenter::defaultCenter();
        for observer in &self.observers {
            // SAFETY: removes an observer this surface added.
            unsafe { center.removeObserver(observer.as_ref()) };
        }
    }
}

fn window(surface: &Surface) -> Option<Retained<NSWindow>> {
    surface.root.ns().window()
}

fn full_screen(window: &NSWindow) -> bool {
    window.styleMask().contains(NSWindowStyleMask::FullScreen)
}

/// Tells the app the hidden title bar's area (after a style change, a
/// resize, full screen).
pub(crate) fn update_area(surface: &Surface) {
    let area = match window(surface) {
        Some(window)
            if surface.chrome.style.get() == TitleBarStyle::Hidden && !full_screen(&window) =>
        {
            let frame = window.frame();
            let layout = window.contentLayoutRect();
            let height = match surface.chrome.height.get() {
                Some(height) => {
                    center_traffic_lights(surface, &window, height);
                    watch_controls(surface, &window);
                    height
                }
                None => {
                    restore_traffic_lights(surface, &window);
                    (frame.size.height - layout.size.height) as f32
                }
            };
            let left = window
                .standardWindowButton(NSWindowButton::ZoomButton)
                .map(|zoom| {
                    let f = zoom.frame();
                    (f.origin.x + f.size.width) as f32 + 8.0
                })
                .filter(|w| *w > 8.0)
                .unwrap_or(TRAFFIC_LIGHTS_WIDTH);
            Insets {
                top: if height > 0.0 {
                    height
                } else {
                    TITLE_BAR_HEIGHT
                },
                left,
                right: 0.0,
                bottom: 0.0,
            }
        }
        _ => Insets::default(),
    };
    let mut env = surface.environment.get();
    if env.title_bar != area {
        env.title_bar = area;
        surface.set_environment(env);
    }
}

/// Centers the traffic lights in an app's title bar `height` points tall:
/// the title bar's container takes that height at the window's top and the
/// buttons are centered in it. AppKit lays them out again on every resize
/// and full screen change, and whenever else it likes: this runs after
/// each ([`update_area`], [`watch_controls`]).
fn center_traffic_lights(surface: &Surface, window: &NSWindow, height: f32) {
    let chrome = &surface.chrome;
    if chrome.centering.replace(true) {
        // Moving one view moves others (the container's autoresizing):
        // the pass runs again.
        chrome.recenter.set(true);
        return;
    }
    // A few passes settle it; more would be AppKit fighting back.
    for _ in 0..4 {
        chrome.recenter.set(false);
        center_traffic_lights_once(surface, window, height);
        if !chrome.recenter.get() {
            break;
        }
    }
    chrome.centering.set(false);
}

fn center_traffic_lights_once(surface: &Surface, window: &NSWindow, height: f32) {
    let buttons = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ]
    .map(|b| window.standardWindowButton(b));
    // The buttons are in the title bar view, which fills its container.
    let container = buttons[0]
        .as_deref()
        .and_then(|close| crate::view::superview(close))
        .and_then(|bar| crate::view::superview(&bar));
    if let Some(container) = container {
        let natural = &surface.chrome.natural;
        if natural.get().is_none() {
            natural.set(Some(container.frame().size.height as f32));
        }
        let height = f64::from(height);
        let mut frame = container.frame();
        frame.size.height = height;
        frame.origin.y = window.frame().size.height - height;
        if container.frame() != frame {
            container.setFrame(frame);
        }
        for button in buttons.iter().flatten() {
            let f = button.frame();
            // The title bar view is not flipped: y goes up.
            let y = ((height - f.size.height) / 2.0).round();
            if f.origin.y != y {
                button.setFrameOrigin(NSPoint::new(f.origin.x, y));
            }
        }
    }
}

/// Puts the traffic lights back where AppKit had them, at the title bar's
/// own height (the app's height is gone).
fn restore_traffic_lights(surface: &Surface, window: &NSWindow) {
    if let Some(natural) = surface.chrome.natural.get() {
        center_traffic_lights(surface, window, natural);
        surface.chrome.natural.set(None);
    }
}

/// Centers the traffic lights again (they moved).
fn recenter(surface: &Surface) {
    let (Some(height), TitleBarStyle::Hidden) =
        (surface.chrome.height.get(), surface.chrome.style.get())
    else {
        return;
    };
    if let Some(window) = window(surface)
        && !full_screen(&window)
    {
        center_traffic_lights(surface, &window, height);
    }
}

/// Centers the traffic lights again whenever AppKit moves them or their
/// container (once per window).
fn watch_controls(surface: &Surface, window: &NSWindow) {
    let mut controls = surface.chrome.controls.borrow_mut();
    if controls
        .as_ref()
        .and_then(|c| c.window.load())
        .is_some_and(|w| std::ptr::eq(&*w, window))
    {
        return;
    }
    let buttons: Vec<Retained<NSButton>> = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ]
    .into_iter()
    .filter_map(|b| window.standardWindowButton(b))
    .collect();
    let container = buttons
        .first()
        .and_then(|close| crate::view::superview(close))
        .and_then(|bar| crate::view::superview(&bar));
    let key = surface.key();
    let block = RcBlock::new(move |_: NonNull<NSNotification>| {
        let Some(surface) = Surface::with_key(key) else {
            return;
        };
        recenter(&surface);
        // AppKit may move the others after this one, in the same layout
        // pass: once it is over, they are centered again.
        if !surface.chrome.deferred.replace(true) {
            DispatchQueue::main().exec_async(move || {
                if let Some(surface) = Surface::with_key(key) {
                    surface.chrome.deferred.set(false);
                    recenter(&surface);
                }
            });
        }
    });
    let center = NSNotificationCenter::defaultCenter();
    let views = buttons
        .iter()
        .map(|b| Retained::into_super(Retained::into_super(b.clone())))
        .chain(container);
    let observers = views
        .map(|view| {
            view.setPostsFrameChangedNotifications(true);
            // SAFETY: a documented notification of a view of this window;
            // with no queue the block runs where it is posted (the main
            // thread, laying the title bar out); the observer is removed
            // when the watch drops.
            unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(NSViewFrameDidChangeNotification),
                    Some(&view),
                    None,
                    &block,
                )
            }
        })
        .collect();
    *controls = Some(ControlsWatch {
        window: Weak::from(window),
        observers,
    });
}

// --- Commands ---------------------------------------------------------------------------

/// A desktop command (on the root).
pub(crate) fn command(surface: &Rc<Surface>, command: &DesktopCommand) {
    match command {
        DesktopCommand::SetTitle(title) => {
            if let Some(window) = window(surface) {
                window.setTitle(&NSString::from_str(title));
            }
        }
        DesktopCommand::SetTitleBar { style, height } => {
            surface.chrome.height.set(*height);
            surface.chrome.style.set(*style);
            apply_title_bar(surface);
        }
        DesktopCommand::Minimize => {
            if let Some(window) = window(surface) {
                window.miniaturize(None);
            }
        }
        DesktopCommand::ToggleMaximize => {
            if let Some(window) = window(surface) {
                window.zoom(None);
            }
        }
        DesktopCommand::ToggleFullScreen => {
            if let Some(window) = window(surface) {
                window.toggleFullScreen(None);
            }
        }
        DesktopCommand::SetFrame(frame) => {
            if let Some(window) = window(surface) {
                set_frame(&window, frame);
            }
        }
        DesktopCommand::SetLevel(level) => {
            if let Some(window) = window(surface) {
                window.setLevel(match level {
                    WindowLevel::Normal => NSNormalWindowLevel,
                    WindowLevel::Floating => NSFloatingWindowLevel,
                    WindowLevel::DesktopIcon => kCGDesktopIconWindowLevel as isize,
                });
            }
        }
        DesktopCommand::Close => {
            if let Some(window) = window(surface) {
                window.performClose(None);
            }
        }
        DesktopCommand::SetMenu(items) => set_menu(surface, items.as_deref()),
        DesktopCommand::PopupMenu { items, at } => {
            // The menu tracks the mouse in a loop of its own: not while the
            // batch is applied.
            let key = surface.key();
            let items = items.clone();
            let at = *at;
            DispatchQueue::main().exec_async(move || {
                if let Some(surface) = Surface::with_key(key) {
                    popup(&surface, &items, at);
                }
            });
        }
    }
}

/// Moves and sizes the window: a field left out keeps its value
/// (`setFrame`'s screen coordinates are the bottom left corner's).
fn set_frame(window: &NSWindow, frame: &WindowFrame) {
    let current = window.frame();
    let point = NSPoint::new(
        frame.x.map_or(current.origin.x, f64::from),
        frame.y.map_or(current.origin.y, f64::from),
    );
    let size = NSSize::new(
        frame.width.map_or(current.size.width, f64::from),
        frame.height.map_or(current.size.height, f64::from),
    );
    window.setFrame_display_animate(
        NSRect::new(point, size),
        true,
        frame.animated.unwrap_or(false),
    );
}

/// The window's style for the title bar's.
pub(crate) fn apply_title_bar(surface: &Surface) {
    let Some(window) = window(surface) else {
        return;
    };
    let hidden = surface.chrome.style.get() == TitleBarStyle::Hidden;
    if !hidden && !full_screen(&window) {
        restore_traffic_lights(surface, &window);
    }
    let mut mask = window.styleMask();
    if hidden {
        mask |= NSWindowStyleMask::FullSizeContentView;
    } else {
        mask &= !NSWindowStyleMask::FullSizeContentView;
    }
    window.setStyleMask(mask);
    window.setTitlebarAppearsTransparent(hidden);
    window.setTitleVisibility(if hidden {
        NSWindowTitleVisibility::Hidden
    } else {
        NSWindowTitleVisibility::Visible
    });
    update_area(surface);
    // AppKit can restore the old content size after setStyleMask returns.
    // Reconcile it after that pass, using the current window and style so
    // another title bar change in the same batch takes precedence.
    let key = surface.key();
    DispatchQueue::main().exec_async(move || {
        if let Some(surface) = Surface::with_key(key)
            && let Some(window) = surface.root.ns().window()
            && let Some(content) = window.contentView()
        {
            let size = window.contentRectForFrameRect(window.frame()).size;
            if content.frame().size != size {
                content.setFrameSize(size);
            }
        }
    });
}

// --- Drag regions -----------------------------------------------------------------------

/// A left press at `(x, y)` (root coordinates): on a view marked
/// `windowDrag`, it moves (or, double-clicked, zooms) the window; true
/// when it did, and the press is not the app's.
pub(crate) fn mouse_down(surface: &Surface, event: &NSEvent, x: f32, y: f32) -> bool {
    if !crate::input::window_drag(surface, x, y) {
        return false;
    }
    let Some(window) = window(surface) else {
        return false;
    };
    if event.clickCount() == 2 {
        let action = NSUserDefaults::standardUserDefaults()
            .stringForKey(ns_string!("AppleActionOnDoubleClick"))
            .map(|s| s.to_string());
        match action.as_deref() {
            Some("Minimize") => window.miniaturize(None),
            Some("None") => {}
            _ => window.zoom(None),
        }
    } else {
        window.performWindowDragWithEvent(event);
    }
    true
}

// --- Menus ------------------------------------------------------------------------------

/// The target of `surface`'s menu items.
fn target(surface: &Surface) -> Retained<MenuTarget> {
    let mut target = surface.chrome.target.borrow_mut();
    target
        .get_or_insert_with(|| {
            let t = MenuTarget::alloc(surface.mtm).set_ivars(surface.key());
            // SAFETY: NSObject's initializer.
            unsafe { msg_send![super(t), init] }
        })
        .clone()
}

/// The action of disabled items: nothing answers it, so AppKit disables
/// them as it validates the menu.
fn disabled() -> Sel {
    sel!(nshMenuItemDisabled:)
}

/// The standard action of a role (sent down the responder chain).
fn role_action(role: MenuRole) -> Sel {
    match role {
        MenuRole::About => sel!(orderFrontStandardAboutPanel:),
        MenuRole::Hide => sel!(hide:),
        MenuRole::HideOthers => sel!(hideOtherApplications:),
        MenuRole::ShowAll => sel!(unhideAllApplications:),
        MenuRole::Quit => sel!(terminate:),
        MenuRole::Undo => sel!(undo:),
        MenuRole::Redo => sel!(redo:),
        MenuRole::Cut => sel!(cut:),
        MenuRole::Copy => sel!(copy:),
        MenuRole::Paste => sel!(paste:),
        MenuRole::SelectAll => sel!(selectAll:),
        MenuRole::Minimize => sel!(performMiniaturize:),
        MenuRole::Zoom => sel!(performZoom:),
        MenuRole::ToggleFullScreen => sel!(toggleFullScreen:),
        MenuRole::Close => sel!(performClose:),
    }
}

/// An item's title: its label, else its role's (with the app's name where
/// macOS writes it).
fn title(action: &MenuAction) -> String {
    if !action.label.is_empty() {
        return action.label.clone();
    }
    match action.role {
        Some(role @ (MenuRole::About | MenuRole::Hide | MenuRole::Quit)) => {
            format!("{} {}", role.label(), app_name())
        }
        Some(MenuRole::ToggleFullScreen) => "Enter Full Screen".to_owned(),
        Some(role) => role.label().to_owned(),
        None => String::new(),
    }
}

/// A shortcut as a key equivalent and its modifiers.
fn key_equivalent(shortcut: &Shortcut) -> (String, NSEventModifierFlags) {
    let function = |code: u32| char::from_u32(code).map(String::from).unwrap_or_default();
    let key = match shortcut.key {
        Key::Char(c) => c.to_string(),
        Key::Enter => "\r".to_owned(),
        Key::Escape => "\u{1b}".to_owned(),
        Key::Tab => "\t".to_owned(),
        Key::Backspace => "\u{8}".to_owned(),
        Key::Space => " ".to_owned(),
        // AppKit's function key characters (NSUpArrowFunctionKey, ...).
        Key::Up => function(0xF700),
        Key::Down => function(0xF701),
        Key::Left => function(0xF702),
        Key::Right => function(0xF703),
        Key::F(n) => function(0xF704 + u32::from(n) - 1),
        Key::Insert => function(0xF727),
        Key::Delete => function(0xF728),
        Key::Home => function(0xF729),
        Key::End => function(0xF72B),
        Key::PageUp => function(0xF72C),
        Key::PageDown => function(0xF72D),
    };
    let mut mask = NSEventModifierFlags::empty();
    for (on, flag) in [
        (shortcut.meta, NSEventModifierFlags::Command),
        (shortcut.ctrl, NSEventModifierFlags::Control),
        (shortcut.alt, NSEventModifierFlags::Option),
        (shortcut.shift, NSEventModifierFlags::Shift),
    ] {
        if on {
            mask |= flag;
        }
    }
    (key, mask)
}

/// A menu of `items`, titled `title`.
fn menu(surface: &Surface, title: &str, items: &[MenuItem]) -> Retained<NSMenu> {
    let mtm = surface.mtm;
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    let mut last_separator = true;
    for item in items {
        match item {
            MenuItem::Separator => {
                if !last_separator {
                    menu.addItem(&NSMenuItem::separatorItem(mtm));
                    last_separator = true;
                }
            }
            MenuItem::Action(action) => {
                let (key, mask) = action
                    .shortcut
                    .as_ref()
                    .map(key_equivalent)
                    .unwrap_or_else(|| (String::new(), NSEventModifierFlags::empty()));
                let selector = match action.role {
                    _ if !action.enabled => Some(disabled()),
                    Some(role) => Some(role_action(role)),
                    None if action.id != 0 => Some(sel!(menuItemChosen:)),
                    None => None,
                };
                // SAFETY: the actions are AppKit's standard ones (sent down
                // the responder chain) or the target's own.
                let ns = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(&self::title(action)),
                        selector,
                        &NSString::from_str(&key),
                    )
                };
                if action.shortcut.is_some() {
                    ns.setKeyEquivalentModifierMask(mask);
                }
                if selector == Some(sel!(menuItemChosen:)) {
                    let target = target(surface);
                    // SAFETY: the target answers `menuItemChosen:`; the
                    // surface keeps it while its menus exist (menu items do
                    // not retain their targets).
                    unsafe { ns.setTarget(Some(&target)) };
                    ns.setTag(action.id as isize);
                }
                if let Some(checked) = action.checked {
                    ns.setState(if checked { 1 } else { 0 });
                }
                menu.addItem(&ns);
                last_separator = false;
            }
            MenuItem::Submenu {
                label,
                enabled,
                items,
            } => {
                let ns = holder(surface, label, *enabled, items);
                menu.addItem(&ns);
                last_separator = false;
            }
        }
    }
    menu
}

/// An item holding a submenu of `items`.
fn holder(
    surface: &Surface,
    label: &str,
    enabled: bool,
    items: &[MenuItem],
) -> Retained<NSMenuItem> {
    let sub = menu(surface, label, items);
    // SAFETY: an item with no action (or one nothing answers, which
    // disables it), holding the submenu.
    let ns = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(surface.mtm),
            &NSString::from_str(label),
            (!enabled).then(disabled),
            ns_string!(""),
        )
    };
    ns.setSubmenu(Some(&sub));
    ns
}

/// The application menu: the main menu, with a menu of the bar per
/// top-level submenu (its top-level actions go to the first one).
fn set_menu(surface: &Surface, items: Option<&[MenuItem]>) {
    let mtm = surface.mtm;
    let app = NSApplication::sharedApplication(mtm);
    ORIGINAL_MENU.with(|o| {
        let mut o = o.borrow_mut();
        if o.is_none() {
            *o = Some(app.mainMenu());
        }
    });
    let Some(items) = items else {
        surface.chrome.menu.borrow_mut().take();
        restore_menu(&app);
        return;
    };
    let bar = NSMenu::new(mtm);
    for item in items {
        // A top-level item that is not a submenu gets one of its own.
        let (label, enabled, sub_items) = match item {
            MenuItem::Submenu {
                label,
                enabled,
                items,
            } => (label.clone(), *enabled, items.clone()),
            MenuItem::Action(action) => (title(action), true, vec![item.clone()]),
            MenuItem::Separator => continue,
        };
        bar.addItem(&holder(surface, &label, enabled, &sub_items));
    }
    app.setMainMenu(Some(&bar));
    *surface.chrome.menu.borrow_mut() = Some(bar);
}

/// Brings back the main menu the app replaced.
fn restore_menu(app: &NSApplication) {
    let original = ORIGINAL_MENU
        .try_with(|o| o.borrow_mut().take())
        .ok()
        .flatten();
    if let Some(original) = original {
        app.setMainMenu(original.as_deref());
    }
}

/// Shows a context menu at `at` (root coordinates) or at the mouse.
fn popup(surface: &Rc<Surface>, items: &Arc<[MenuItem]>, at: Option<(f32, f32)>) {
    let root = surface.root.ns();
    let menu = menu(surface, "", items);
    let location = match at {
        // The root view is flipped: y goes down, as the app's.
        Some((x, y)) => NSPoint::new(x as f64, y as f64),
        None => {
            let Some(window) = root.window() else { return };
            let in_window = window.convertPointFromScreen(NSEvent::mouseLocation());
            root.convertPoint_fromView(in_window, None)
        }
    };
    // Returns once the menu closed (the chosen item's action sent).
    menu.popUpMenuPositioningItem_atLocation_inView(None, location, Some(root));
    // A menu opened during a press (`onPressIn`, a long press) took its
    // release: the press ends here, or the view would stay pressed.
    crate::input::cancel_pointer(surface);
    // The app's `Menu.popup` promise settles.
    surface.event(
        ROOT_ID,
        EventKind::Menu,
        EventPayload::Map(vec![
            ("id", PayloadValue::Number(0.0)),
            ("closed", PayloadValue::Bool(true)),
        ]),
    );
}

/// The root view moved to a window, or the window changed: a hidden title
/// bar applies to it (a window keeps its own style otherwise).
pub(crate) fn window_changed(surface: &Surface) {
    if surface.chrome.style.get() == TitleBarStyle::Hidden {
        apply_title_bar(surface);
    } else {
        update_area(surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_are_key_equivalents() {
        let equivalent = |text: &str| key_equivalent(&Shortcut::parse(text, true).unwrap());
        // Lowercase: an uppercase key equivalent would add Shift.
        assert_eq!(
            equivalent("mod+shift+n"),
            (
                "n".to_owned(),
                NSEventModifierFlags::Command | NSEventModifierFlags::Shift
            )
        );
        assert_eq!(
            equivalent("ctrl+alt+left"),
            (
                "\u{f702}".to_owned(),
                NSEventModifierFlags::Control | NSEventModifierFlags::Option
            )
        );
        assert_eq!(
            equivalent("f5"),
            ("\u{f708}".to_owned(), NSEventModifierFlags::empty())
        );
        assert_eq!(equivalent("mod+plus").0, "=");
        assert_eq!(equivalent("cmd+pagedown").0, "\u{f72d}");
    }
}
