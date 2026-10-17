//! `AppView` (one surface as an AppKit view, for embedding) and the
//! standalone app ([`run_app`]: one `AppView` filling a window).

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, RwLock},
};

use nsh_core::platform::Appearance;
use nsh_engine::{App, Bundle, ColorScheme, EngineConfig, Environment, Idiom, Limits, Os, Root};
use objc2::{
    MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject, sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSAutoresizingMaskOptions,
    NSBackingStoreType, NSEventModifierFlags, NSMenu, NSMenuItem, NSView, NSWindow,
    NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::{
    image::{AssetBase, AssetReader},
    surface::Surface,
};

static UI_SETS: RwLock<Option<Vec<nsh_core::ui::Registry>>> = RwLock::new(None);

/// Sets the UI sets every engine serves (default: `nsh/ui/mobile`); an app
/// with Rust components of its own calls this before [`run_app`].
pub fn set_ui_sets(sets: Vec<nsh_core::ui::Registry>) {
    *UI_SETS.write().unwrap_or_else(|e| e.into_inner()) = Some(sets);
}

/// The UI sets engines serve.
pub(crate) fn ui_sets() -> Vec<nsh_core::ui::Registry> {
    UI_SETS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| vec![nsh_ui_mobile::registry()])
}

/// What [`run_app`] runs.
#[derive(Debug)]
pub enum AppSource {
    /// A Rust app: its root component.
    Rust(Root),
    /// A JS bundle shipped with (or passed to) the app.
    Bundle(Bundle),
    /// A dev server (`ws://host:port/ws`); bundles are pushed on every
    /// rebuild.
    #[cfg(feature = "dev")]
    DevServer(String),
}

/// Options of an [`AppView`] and [`run_app`].
#[derive(Debug, Clone)]
pub struct AppOptions {
    /// Log per-commit timings.
    pub trace: bool,
    /// Where bundled assets are (for [`AppSource::Bundle`]); dev servers
    /// serve them themselves.
    pub asset_base: Option<AssetBase>,
    /// The window title.
    pub title: String,
    /// The initial window size in points.
    pub size: (i32, i32),
    /// Steps run once the window shows (`wait 500; shot a.png; tap 10 20;
    /// quit`): for checking a backend change without touching the screen.
    /// See [`crate::script`].
    pub script: Option<String>,
    /// The native components apps can use (`nsh/native`); shared by the
    /// views made with these options. See [`AppView::register_component`].
    pub components: crate::native::Components,
    /// The app's appearance until it sets one (`nsh.json` `"appearance"`,
    /// which the bundle sets again when it starts): known before the
    /// first frame, the window opens in it rather than flashing the
    /// system's.
    pub appearance: Appearance,
    /// Shows the performance monitor over the app (frame rate, frame and
    /// commit costs, CPU, GPU, memory): see [`AppView::set_fps_monitor`].
    pub fps: bool,
    /// The VM limits of the apps it runs: [`Limits::default`] (an app
    /// embedded in a host app) unless set; the development host and the
    /// release runner run apps on their own, with
    /// [`Limits::standalone_desktop`].
    pub limits: Limits,
}

impl Default for AppOptions {
    fn default() -> Self {
        AppOptions {
            trace: false,
            asset_base: None,
            title: "nsh".to_owned(),
            size: (1200, 800),
            script: None,
            components: Default::default(),
            appearance: Appearance::System,
            fps: false,
            limits: Limits::default(),
        }
    }
}

/// One nsh surface as an AppKit view: an engine running one app.
///
/// ```no_run
/// use nsh_platform_macos::{
///     AppOptions, AppView, AssetBase, Bundle, objc2::MainThreadMarker, objc2_app_kit::NSWindow,
/// };
///
/// fn show(window: &NSWindow) -> std::rc::Rc<AppView> {
///     let mtm = MainThreadMarker::new().expect("main thread");
///     let view = AppView::new(
///         mtm,
///         AppOptions {
///             asset_base: Some(AssetBase::Directory("app/assets".into())),
///             ..AppOptions::default()
///         },
///     );
///     let code = std::fs::read_to_string("app/bundle.js").expect("bundle");
///     view.load(Bundle::source(code).into());
///     window.setContentView(Some(&view.view()));
///     // Keep the view (its engine) while the window shows it; `stop` it
///     // when the window closes.
///     view
/// }
/// ```
pub struct AppView {
    surface: Rc<Surface>,
    /// Hosts the surface and, in development, the status line and error
    /// overlay above it.
    container: Retained<NSView>,
    options: RefCell<AppOptions>,
    /// The performance monitor's HUD, while it shows.
    fps: RefCell<Option<Rc<crate::fps::Hud>>>,
    #[cfg(feature = "dev")]
    pub(crate) dev: RefCell<Option<crate::dev::DevSession>>,
}

impl AppView {
    /// A view with no app yet; it fills its superview.
    pub fn new(mtm: MainThreadMarker, options: AppOptions) -> Rc<AppView> {
        let surface = Surface::new(mtm, initial_environment());
        surface.appearance.set(options.appearance);
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(options.size.0 as f64, options.size.1 as f64),
        );
        let container = NSView::initWithFrame(NSView::alloc(mtm), frame);
        container.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        container.setWantsLayer(true);
        let root = surface.root.ns();
        root.setFrame(frame);
        root.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        container.addSubview(root);
        surface.native.set_components(options.components.clone());
        let view = Rc::new(AppView {
            surface,
            container,
            options: RefCell::new(options),
            fps: RefCell::new(None),
            #[cfg(feature = "dev")]
            dev: RefCell::new(None),
        });
        crate::system::watch_environment(&view.surface);
        if view.options.borrow().fps {
            view.set_fps_monitor(true);
        }
        view
    }

    /// The view to put in a window.
    pub fn view(&self) -> Retained<NSView> {
        self.container.clone()
    }

    /// Runs `app`, replacing what ran.
    pub fn load(&self, app: App) {
        let options = self.options.borrow();
        let config = EngineConfig {
            app,
            viewport: nsh_core::Size::new(0.0, 0.0),
            trace: options.trace,
            network: Some(crate::net::network()),
            assets: Some(Arc::new(AssetReader(options.asset_base.clone()))),
            environment: self.surface.environment.get(),
            header_heights: crate::navigation::HEADER_HEIGHTS,
            limits: options.limits,
            ui_sets: ui_sets(),
            runtime: None,
        };
        crate::image::set_asset_base(&self.surface, options.asset_base.clone());
        self.surface
            .native
            .set_asset_base(options.asset_base.clone());
        self.surface.native.set_app(&config.app);
        self.surface.start(config);
    }

    /// Where `asset:/` URIs resolve from now on.
    pub fn set_asset_base(&self, base: Option<AssetBase>) {
        self.options.borrow_mut().asset_base = base;
    }

    /// Registers native component `name` (`nativeComponent(name, ...)` in
    /// JS): `factory` makes one for every element. Applies to elements
    /// created from now on.
    pub fn register_component(&self, name: &str, factory: crate::native::ComponentFactory) {
        self.surface.native.components().register(name, factory);
    }

    /// Shows (or hides) the performance monitor: a panel at the top right
    /// with the frame rate, what frames cost the main thread, what commits
    /// cost the engine thread (JS, layout), and the process' CPU, GPU and
    /// memory. Measuring starts with it and stops when it hides; it
    /// survives reloads.
    pub fn set_fps_monitor(&self, show: bool) {
        let mut hud = self.fps.borrow_mut();
        match (show, hud.is_some()) {
            (true, false) => {
                let log = self.options.borrow().trace;
                *hud = Some(crate::fps::Hud::show(self, log));
            }
            (false, true) => *hud = None,
            _ => {}
        }
    }

    /// What the performance monitor measured, while it shows
    /// ([`AppView::set_fps_monitor`]): [`nsh_engine::perf::PerfMonitor::snapshot`]
    /// reads it.
    pub fn perf_monitor(&self) -> Option<Arc<nsh_engine::perf::PerfMonitor>> {
        self.fps.borrow().as_ref().map(|h| h.monitor().clone())
    }

    /// Stops the app.
    pub fn stop(&self) {
        self.surface.stop();
    }

    pub(crate) fn surface(&self) -> &Rc<Surface> {
        &self.surface
    }

    /// The view the surface is in (overlays go above it).
    pub(crate) fn container(&self) -> &NSView {
        &self.container
    }
}

fn initial_environment() -> Environment {
    Environment {
        window: (0.0, 0.0),
        color_scheme: ColorScheme::Light,
        os: Os::MacOs,
        idiom: Idiom::Desktop,
        ..Environment::default()
    }
}

/// The standalone app's window and view.
struct Running {
    window: Retained<NSWindow>,
    view: Rc<AppView>,
}

thread_local! {
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
}

define_class!(
    /// Closes the app with its window.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NshAppDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_terminate_after_last_window_closed(&self, _app: &NSApplication) -> bool {
            true
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _notification: &NSNotification) {
            stop_running();
        }
    }

    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            stop_running();
            // AppKit ends the app after its last window closes only if the
            // window was on screen: not a minimized one. The app ends with
            // its window either way (once the close is done).
            dispatch2::DispatchQueue::main().exec_async(|| {
                if let Some(mtm) = MainThreadMarker::new() {
                    NSApplication::sharedApplication(mtm).terminate(None);
                }
            });
        }
    }
);

/// Stops the standalone app's engine (its window closes).
fn stop_running() {
    if let Some(running) = RUNNING.with(|r| r.borrow_mut().take()) {
        running.view.stop();
        drop(running.window);
    }
}

/// Names the app `name` in the menu bar, which shows the bundle's name
/// as the app menu's title: an executable without a bundle (the
/// development host) would show its file's name. Before the app finishes
/// launching.
fn name_app(name: &str) {
    use objc2::ClassType;
    use objc2_foundation::{NSBundle, NSMutableDictionary};
    let Some(info) = NSBundle::mainBundle().infoDictionary() else {
        return;
    };
    let name = NSString::from_str(name);
    // A bundle's dictionary is mutable in practice; the dictionary of an
    // executable without one too.
    // SAFETY: `isKindOfClass:` takes any class; `setObject:forKey:` is
    // sent only to a mutable dictionary, with a string key and value.
    unsafe {
        let mutable: bool =
            msg_send![&*info, isKindOfClass: NSMutableDictionary::<NSString, NSString>::class()];
        if mutable {
            let _: () = msg_send![&*info, setObject: &*name, forKey: objc2_foundation::ns_string!("CFBundleName")];
        }
    }
}

/// Runs `source` in a window until it is closed. Must be called on the
/// main thread.
pub fn run_app(source: AppSource, options: AppOptions) -> std::process::ExitCode {
    let mtm = MainThreadMarker::new().expect("run_app must be called on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    name_app(&options.title);
    crate::chrome::set_app_name(&options.title);
    let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(mtm), init] };
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.setMainMenu(Some(&main_menu(mtm, &options.title)));

    let frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(options.size.0 as f64, options.size.1 as f64),
    );
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    // SAFETY: NSWindow's designated initializer.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is kept in `RUNNING` until it closes.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(&options.title));
    window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    crate::system::open_window(&window, options.appearance);
    let view = AppView::new(mtm, options.clone());
    window.setContentView(Some(&view.view()));
    window.center();
    match source {
        AppSource::Rust(root) => view.load(App::Rust(root)),
        AppSource::Bundle(bundle) => view.load(App::Js(bundle)),
        #[cfg(feature = "dev")]
        AppSource::DevServer(url) => crate::dev::start(&view, url),
    }
    window.makeKeyAndOrderFront(None);
    window.makeFirstResponder(Some(view.surface().root.ns()));
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    if let Some(script) = &options.script {
        crate::script::run(&window, view.surface().clone(), script);
    }
    RUNNING.with(|r| *r.borrow_mut() = Some(Running { window, view }));
    app.run();
    drop(delegate);
    std::process::ExitCode::SUCCESS
}

/// The app menu (About, Hide, Quit), Edit (the text commands fields
/// answer: undo, cut, copy, paste, select all) and Window (minimize, zoom,
/// full screen, close).
fn main_menu(mtm: MainThreadMarker, title: &str) -> Retained<NSMenu> {
    let bar = NSMenu::new(mtm);
    let item = |menu: &NSMenu, title: &str, action: Option<objc2::runtime::Sel>, key: &str| {
        // SAFETY: the actions are AppKit's standard ones, sent down the
        // responder chain.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                action,
                &NSString::from_str(key),
            )
        };
        menu.addItem(&item);
        item
    };
    let submenu = |name: &str| {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(name));
        let holder = item(&bar, name, None, "");
        holder.setSubmenu(Some(&menu));
        menu
    };
    let app_menu = submenu(title);
    item(
        &app_menu,
        &format!("About {title}"),
        Some(sel!(orderFrontStandardAboutPanel:)),
        "",
    );
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    item(&app_menu, &format!("Hide {title}"), Some(sel!(hide:)), "h");
    let others = item(
        &app_menu,
        "Hide Others",
        Some(sel!(hideOtherApplications:)),
        "h",
    );
    others
        .setKeyEquivalentModifierMask(NSEventModifierFlags::Command | NSEventModifierFlags::Option);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    item(
        &app_menu,
        &format!("Quit {title}"),
        Some(sel!(terminate:)),
        "q",
    );

    let edit = submenu("Edit");
    item(&edit, "Undo", Some(sel!(undo:)), "z");
    // An uppercase key equivalent is Shift with it.
    item(&edit, "Redo", Some(sel!(redo:)), "Z");
    edit.addItem(&NSMenuItem::separatorItem(mtm));
    item(&edit, "Cut", Some(sel!(cut:)), "x");
    item(&edit, "Copy", Some(sel!(copy:)), "c");
    item(&edit, "Paste", Some(sel!(paste:)), "v");
    item(&edit, "Select All", Some(sel!(selectAll:)), "a");

    let window = submenu("Window");
    item(&window, "Minimize", Some(sel!(performMiniaturize:)), "m");
    item(&window, "Zoom", Some(sel!(performZoom:)), "");
    let full = item(
        &window,
        "Enter Full Screen",
        Some(sel!(toggleFullScreen:)),
        "f",
    );
    full.setKeyEquivalentModifierMask(
        NSEventModifierFlags::Command | NSEventModifierFlags::Control,
    );
    window.addItem(&NSMenuItem::separatorItem(mtm));
    item(&window, "Close", Some(sel!(performClose:)), "w");
    bar
}
