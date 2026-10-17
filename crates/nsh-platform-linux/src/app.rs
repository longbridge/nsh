//! `AppView` (one surface as a GTK widget, for embedding) and the
//! standalone app ([`run_app`]: one `AppView` filling a window).

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, RwLock},
};

use gtk4::{self as gtk, glib, prelude::*};
use nsh_core::platform::Appearance;
use nsh_engine::{App, Bundle, ColorScheme, EngineConfig, Environment, Idiom, Limits, Os, Root};

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
    /// The application id (`com.example.app`); web views keep their
    /// website data (cookies, storage) under it, in
    /// `$XDG_DATA_HOME/<app id>/webkit`.
    pub app_id: String,
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
            app_id: "com.longbridge.nsh.host".to_owned(),
            title: "nsh".to_owned(),
            size: (390, 844),
            script: None,
            components: Default::default(),
            appearance: Appearance::System,
            limits: Limits::default(),
        }
    }
}

/// One nsh surface as a GTK widget: an engine running one app.
///
/// ```no_run
/// use std::{cell::RefCell, rc::Rc};
///
/// use gtk::prelude::*;
/// use nsh_platform_linux::{AppOptions, AppView, AssetBase, Bundle, gtk4 as gtk};
///
/// let app = gtk::Application::builder()
///     .application_id("com.example.host")
///     .build();
/// app.connect_activate(|app| {
///     let window = gtk::ApplicationWindow::builder().application(app).build();
///     let view = AppView::new(AppOptions {
///         asset_base: Some(AssetBase::Directory("app/assets".into())),
///         ..AppOptions::default()
///     });
///     let code = std::fs::read_to_string("app/bundle.js").expect("bundle");
///     view.load(Bundle::source(code).into());
///     window.set_child(Some(&view.widget()));
///     // The window keeps the view; closing it stops the app.
///     let keep = RefCell::new(Some(Rc::clone(&view)));
///     window.connect_destroy(move |_| {
///         if let Some(view) = keep.take() {
///             view.stop();
///         }
///     });
///     window.present();
/// });
/// app.run();
/// ```
pub struct AppView {
    surface: Rc<Surface>,
    /// Hosts the surface and, in development, the status line and error
    /// overlay above it.
    #[cfg_attr(not(feature = "dev"), allow(dead_code))]
    overlay: gtk::Overlay,
    /// The widget put in a window: the menu bar's slot above the overlay
    /// ([`crate::chrome`]).
    outer: gtk::Box,
    options: RefCell<AppOptions>,
    #[cfg(feature = "dev")]
    pub(crate) dev: RefCell<Option<crate::dev::DevSession>>,
}

impl AppView {
    /// A view with no app yet.
    pub fn new(options: AppOptions) -> Rc<AppView> {
        let surface = Surface::new(initial_environment());
        surface.appearance.set(options.appearance);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&crate::surface::widget(&surface)));
        overlay.set_vexpand(true);
        let slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer.append(&slot);
        outer.append(&overlay);
        crate::chrome::attach(&surface, &overlay, &slot);
        surface.native.set_components(options.components.clone());
        surface.native.set_app_id(&options.app_id);
        let view = Rc::new(AppView {
            surface,
            overlay,
            outer,
            options: RefCell::new(options),
            #[cfg(feature = "dev")]
            dev: RefCell::new(None),
        });
        crate::system::watch_environment(&view.surface);
        view
    }

    /// The widget to put in a window.
    pub fn widget(&self) -> gtk::Widget {
        self.outer.clone().upcast()
    }

    /// Runs `app`, replacing what ran.
    pub fn load(&self, app: App) {
        let access = match &app {
            App::Js(bundle) => bundle.policy.access(),
            // Native code reaches what the platform allows.
            App::Rust(_) => nsh_engine::Policy::default().access(),
        };
        self.surface.native.set_access(Arc::new(access));
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

    /// Stops the app.
    pub fn stop(&self) {
        self.surface.stop();
    }

    pub(crate) fn surface(&self) -> &Rc<Surface> {
        &self.surface
    }

    /// The overlay the surface is the child of (development overlays go
    /// above it).
    #[cfg(feature = "dev")]
    pub(crate) fn overlay(&self) -> &gtk::Overlay {
        &self.overlay
    }
}

fn initial_environment() -> Environment {
    Environment {
        window: (0.0, 0.0),
        color_scheme: ColorScheme::Light,
        os: Os::Linux,
        idiom: Idiom::Desktop,
        ..Environment::default()
    }
}

/// Runs `source` in a window until it is closed.
pub fn run_app(source: AppSource, options: AppOptions) -> glib::ExitCode {
    // X11's WM_CLASS comes from the program name: the app's id, as the
    // `StartupWMClass` of the `.desktop` file `nsh package` writes (Wayland's
    // app id is the application id).
    glib::set_prgname(Some(&options.app_id));
    let app = gtk::Application::builder()
        .application_id(&options.app_id)
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let source = RefCell::new(Some(source));
    app.connect_activate(move |app| {
        let Some(source) = source.borrow_mut().take() else {
            return;
        };
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title(&options.title)
            .default_width(options.size.0)
            .default_height(options.size.1)
            .build();
        let view = AppView::new(options.clone());
        window.set_child(Some(&view.widget()));
        match source {
            AppSource::Rust(root) => view.load(App::Rust(root)),
            AppSource::Bundle(bundle) => view.load(App::Js(bundle)),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => crate::dev::start(&view, url),
        }
        install_shortcuts(app, &window);
        if let Some(script) = &options.script {
            crate::script::run(&window, view.surface().clone(), script);
        }
        // The window keeps the view (and its engine) alive.
        unsafe_keep(&window, view);
        window.present();
    });
    // GTK must not parse the host's own arguments.
    app.run_with_args::<&str>(&[])
}

/// GNOME's window shortcuts: Ctrl+W closes the window, Ctrl+Q quits, F11
/// toggles full screen. Accelerators also match the keys of the layout's
/// Latin group, so they work under Cyrillic or Greek layouts too.
fn install_shortcuts(app: &gtk::Application, window: &gtk::ApplicationWindow) {
    let quit = gtk::gio::SimpleAction::new("quit", None);
    let weak = app.downgrade();
    quit.connect_activate(move |_, _| {
        // Closing the windows stops their apps; the last one ends the run.
        if let Some(app) = weak.upgrade() {
            for window in app.windows() {
                window.close();
            }
        }
    });
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
    // GtkWindow's own action. Not an application accelerator: those run
    // before everything, and an app may bind Ctrl+W itself (`nsh/keymap`,
    // e.g. to close a tab); this one comes last.
    let close = gtk::ShortcutController::new();
    close.set_propagation_phase(gtk::PropagationPhase::Bubble);
    close.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("<Control>w"),
        Some(gtk::NamedAction::new("window.close")),
    ));
    window.add_controller(close);
    let fullscreen = gtk::gio::SimpleAction::new("fullscreen", None);
    let weak = window.downgrade();
    fullscreen.connect_activate(move |_, _| {
        if let Some(window) = weak.upgrade() {
            window.set_fullscreened(!window.is_fullscreen());
        }
    });
    window.add_action(&fullscreen);
    app.set_accels_for_action("win.fullscreen", &["F11"]);
}

/// Ties `view` to `window`'s lifetime.
fn unsafe_keep(window: &gtk::ApplicationWindow, view: Rc<AppView>) {
    let cell = RefCell::new(Some(view));
    window.connect_destroy(move |_| {
        if let Some(view) = cell.borrow_mut().take() {
            view.stop();
        }
    });
}
