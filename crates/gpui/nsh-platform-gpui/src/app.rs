//! `AppView` (one surface as a GPUI view, for embedding in any GPUI or
//! GPUI Kit app) and the standalone app ([`run_app`]: one `AppView`
//! filling a window).

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, RwLock},
};

use gpui::{
    App as GpuiApp, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Window,
};
use nsh_core::{platform::Appearance, ui::ThemeError};
use nsh_engine::{
    App, Bundle, ColorScheme, EngineConfig, Environment, Idiom, Limits, Os, Root, Theme,
};

use crate::{
    image::{AssetBase, AssetReader},
    render::SurfaceElement,
    surface::Surface,
};

gpui::actions!(
    nsh,
    [
        /// Tab in a surface: its next text field, or the window's next tab
        /// stop.
        FocusNext,
        /// Shift + Tab in a surface: its previous text field, or the
        /// window's previous tab stop.
        FocusPrev
    ]
);

/// The key context of [`AppView`]: its Tab bindings are deeper than an
/// embedding app's (GPUI Kit's `Root` binds Tab to move the window's
/// focus), so the surface's text fields come first.
const KEY_CONTEXT: &str = "NshAppView";

/// Marks the app whose key bindings are set ([`bind_keys`]).
struct KeysBound;

impl gpui::Global for KeysBound {}

/// Binds Tab and Shift + Tab in surfaces, once per app.
fn bind_keys(cx: &mut GpuiApp) {
    if cx.has_global::<KeysBound>() {
        return;
    }
    cx.set_global(KeysBound);
    cx.bind_keys([
        gpui::KeyBinding::new("tab", FocusNext, Some(KEY_CONTEXT)),
        gpui::KeyBinding::new("shift-tab", FocusPrev, Some(KEY_CONTEXT)),
    ]);
}

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
    /// The application id (`com.example.app`).
    pub app_id: String,
    /// The window title.
    pub title: String,
    /// The initial window size in points.
    pub size: (i32, i32),
    /// Steps run once the window shows (`wait 500; shot a.png; tap 10 20;
    /// quit`): for checking a backend change without touching the screen.
    pub script: Option<String>,
    /// The native components apps can use (`nsh/native`); shared by the
    /// views made with these options. See [`AppView::register_component`].
    pub components: crate::native::Components,
    /// The app's appearance until it sets one (`nsh.json` `"appearance"`).
    pub appearance: Appearance,
    /// Log frame statistics once a second (`nsh::perf fps …`): see
    /// [`AppView::set_fps_monitor`].
    pub fps: bool,
}

impl Default for AppOptions {
    fn default() -> Self {
        AppOptions {
            trace: false,
            asset_base: None,
            app_id: "com.longbridge.nsh.host".to_owned(),
            title: "nsh".to_owned(),
            size: (1200, 800),
            script: None,
            components: Default::default(),
            appearance: Appearance::System,
            fps: false,
        }
    }
}

/// One nsh surface as a GPUI view: an engine running one app, drawn by
/// GPUI wherever the embedding app puts the view (it fills the space its
/// parent element gives it, and follows it when it is resized).
///
/// Embedded in a GPUI or GPUI Kit app:
///
/// - Several views can run at once, in one window or several: each has
///   its own engine thread. Dropping the view (its last `Entity` handle)
///   stops its engine; so does [`AppView::stop`].
/// - The view is [`Focusable`] and a tab stop: Tab moves the window's
///   focus into it (and through its text fields), and on out of it after
///   the last one. A press on it focuses it.
/// - [`AppView::set_theme`] / [`AppView::set_theme_json`] give the app the
///   embedding app's theme (gpui-kit's theme format) and
///   [`AppView::set_appearance`] its light or dark mode.
/// - What the app tells the embedding app goes through the native
///   components the latter registers ([`AppView::register_component`]):
///   their events and commands. [`AppView::on_error`] hears fatal errors.
/// - The view draws only inside its bounds: the host's overlays (GPUI
///   Kit's dialogs, popovers, notifications) draw above it.
/// - Open the host window with `inactive_frame_interval: None` in its
///   `WindowOptions`: GPUI draws unfocused windows at 30 fps by default,
///   which halves the surface's animations whenever the window does not
///   have the keyboard (nsh's own windows set it).
///
/// Most methods take `&self` and can be called through `view.read(cx)`:
/// they hand work to the engine thread, whose next frame redraws the view.
/// [`AppView::set_theme`], [`AppView::set_theme_json`] and
/// [`AppView::set_appearance`] also change what the main thread draws (the
/// surface's background, its light or dark environment) without waiting
/// for the engine, so they take `cx` to redraw at once: call them through
/// `view.update(cx, |view, cx| …)`.
///
/// ```no_run
/// use gpui::{AppContext, Application, WindowOptions};
/// use nsh_platform_gpui::{App, AppOptions, AppView, Bundle};
///
/// gpui_platform::application().run(|cx| {
///     cx.open_window(WindowOptions::default(), |window, cx| {
///         cx.new(|cx| {
///             let view = AppView::new(AppOptions::default(), window, cx);
///             let code = std::fs::read_to_string("app/bundle.js").expect("bundle");
///             view.load(App::Js(Bundle::source(code)));
///             view
///         })
///     })
///     .unwrap();
/// });
/// ```
pub struct AppView {
    surface: Rc<Surface>,
    focus: FocusHandle,
    options: RefCell<AppOptions>,
    /// Tab into the view from the host's widgets ([`AppView::tab_entered`]).
    _keystrokes: gpui::Subscription,
    /// The view itself handled the Tab being dispatched ([`AppView::tab`]).
    tabbed: std::cell::Cell<bool>,
    #[cfg(feature = "dev")]
    pub(crate) dev: RefCell<Option<crate::dev::DevSession>>,
}

impl AppView {
    /// A view with no app yet (in `cx.new(|cx| AppView::new(...))`).
    pub fn new(options: AppOptions, window: &mut Window, cx: &mut Context<Self>) -> AppView {
        crate::rt::init(cx);
        // The focused field's caret follows the window's activation
        // (`text_input::window_focus`, checked as the view is drawn).
        cx.observe_window_activation(window, |_, _, cx| cx.notify())
            .detach();
        bind_keys(cx);
        let surface = Surface::new(initial_environment());
        surface.appearance.set(options.appearance);
        if options.fps {
            *surface.stats.borrow_mut() = Some(Default::default());
        }
        surface.entity.set(Some(cx.entity_id()));
        *surface.text_system.borrow_mut() = Some(cx.text_system().clone());
        surface.native.set_components(options.components.clone());
        crate::system::watch_environment(&surface, window, cx);
        // The open menu's keys come before anything focused sees them.
        let weak = Rc::downgrade(&surface);
        surface.shortcuts.borrow_mut().push(Box::new(move |event| {
            weak.upgrade()
                .is_some_and(|s| crate::chrome::window_key(&s, event))
        }));
        // The chrome follows the window: full screen, maximized, active.
        cx.observe_window_bounds(window, |_, _, cx| cx.notify())
            .detach();
        cx.observe_window_activation(window, |view: &mut AppView, window, cx| {
            if !window.is_window_active() {
                crate::keymap::forget_held(&view.surface);
            }
            cx.notify();
        })
        .detach();
        AppView {
            surface,
            focus: cx.focus_handle().tab_stop(true),
            options: RefCell::new(options),
            tabbed: std::cell::Cell::new(false),
            _keystrokes: cx.observe_keystrokes(Self::tab_entered),
            #[cfg(feature = "dev")]
            dev: RefCell::new(None),
        }
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
            limits: Limits::default(),
            ui_sets: ui_sets(),
            runtime: None,
        };
        crate::image::set_asset_base(&self.surface, options.asset_base.clone());
        self.surface
            .native
            .set_asset_base(options.asset_base.clone());
        self.surface.start(config);
    }

    /// Where `asset:/` URIs resolve: at once for the images, videos and
    /// web views the running app loads from now on, and for every app
    /// loaded later. Fonts the running app already registered stay; a
    /// running engine keeps reading the files it asks for itself (fonts)
    /// from the base it was loaded with, until the next [`AppView::load`].
    pub fn set_asset_base(&self, base: Option<AssetBase>) {
        crate::image::set_asset_base(&self.surface, base.clone());
        self.surface.native.set_asset_base(base.clone());
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

    /// Gives the app the embedding app's theme: the components of the UI
    /// sets (`nsh/ui/mobile`) render with it instead of the app's own
    /// (`nsh.json` `"theme"`), and the surface is drawn on its
    /// `background`. `None` gives the app its own theme back. It applies
    /// at once to what runs and to every app loaded later, from its first
    /// frame on.
    pub fn set_theme(&self, theme: Option<Theme>, cx: &mut GpuiApp) {
        *self.surface.host_theme.borrow_mut() = theme.clone();
        self.surface.control(|c| c.set_theme(theme));
        self.surface.invalidate();
        crate::rt::flush(cx);
    }

    /// [`AppView::set_theme`] with a theme in gpui-kit's JSON format
    /// (`{ "themes": [{ "mode": "light", "colors": { … } }, …] }`: a
    /// gpui-kit theme file, or `ThemeConfig`s serialized; null colors are
    /// left as they are), over nsh's default theme.
    pub fn set_theme_json(&self, json: &str, cx: &mut GpuiApp) -> Result<(), ThemeError> {
        let theme = Theme::from_json(json)?;
        self.set_theme(Some(theme), cx);
        Ok(())
    }

    /// The embedding app's appearance, which stands in for the system's:
    /// [`Appearance::Light`] or [`Appearance::Dark`] (the host's own light
    /// or dark mode), or [`Appearance::System`] to follow the window's
    /// again. An appearance the app sets itself (`nsh.json`
    /// `"appearance"`, `setColorScheme`) still wins, as it does over the
    /// system's.
    pub fn set_appearance(&self, appearance: Appearance, cx: &mut GpuiApp) {
        self.surface.host_dark.set(match appearance {
            Appearance::Light => Some(false),
            Appearance::Dark => Some(true),
            _ => None,
        });
        crate::system::host_appearance_changed(&self.surface);
        crate::rt::flush(cx);
    }

    /// Logs the surface's frame statistics once a second (`nsh::perf fps
    /// …`: frames drawn, their gaps, the main thread's tick and paint, what
    /// made GPUI draw), or stops.
    pub fn set_fps_monitor(&self, on: bool) {
        *self.surface.stats.borrow_mut() = on.then(Default::default);
    }

    /// Calls `f` with the message of every fatal error of the app (an
    /// uncaught exception at the start, a VM limit reached), which is also
    /// logged.
    pub fn on_error(&self, f: impl Fn(&str) + 'static) {
        self.surface.set_error_handler(Box::new(f));
    }

    pub(crate) fn surface(&self) -> &Rc<Surface> {
        &self.surface
    }

    /// Tab: the surface's next text field; past its last one, the window's
    /// next tab stop (the embedding app's widgets), or the first field again
    /// when there is none.
    fn focus_next(&mut self, _: &FocusNext, window: &mut Window, cx: &mut Context<Self>) {
        self.tab(false, window, cx);
    }

    /// Shift + Tab, the other way.
    fn focus_prev(&mut self, _: &FocusPrev, window: &mut Window, cx: &mut Context<Self>) {
        self.tab(true, window, cx);
    }

    /// After every keystroke: when Tab (handled by the host, GPUI Kit's
    /// `Root`) moved the window's focus onto the view, the focus goes on to
    /// its first text field (its last with Shift), not to the view itself.
    fn tab_entered(
        &mut self,
        event: &gpui::KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let k = &event.keystroke;
        let m = &k.modifiers;
        if k.key != "tab" || m.control || m.alt || m.platform || self.tabbed.take() {
            return;
        }
        // The host's Tab lands on the view, or (by GPUI's tab order, which
        // does not follow the surface's tree) on one of its components.
        let on_view =
            self.focus.is_focused(window) && crate::text_input::focused(&self.surface).is_none();
        if !on_view && !crate::native::focused(&self.surface, window) {
            return;
        }
        crate::input::tab_to_edge(&self.surface, m.shift);
        crate::native::apply_focus(&self.surface, &self.focus, window, cx);
        crate::rt::flush(cx);
    }

    fn tab(&self, shift: bool, window: &mut Window, cx: &mut Context<Self>) {
        let event = crate::input::KeyEvent {
            vk: crate::keys::VK_TAB,
            down: true,
            repeat: false,
            modifiers: crate::input::Modifiers {
                shift,
                ..Default::default()
            },
        };
        self.tabbed.set(true);
        if !crate::input::key(&self.surface, event) {
            // Past the last stop (the first with Shift): on to the window's
            // next tab stop outside the view, or around again when there is
            // none (a window of its own).
            if self.leave(shift, window, cx) || !crate::input::tab_to_edge(&self.surface, shift) {
                crate::text_input::blur_all(&self.surface);
            }
        }
        // A component Tab reached takes the window's focus; a field reached
        // from a component gives it back to the view.
        crate::native::apply_focus(&self.surface, &self.focus, window, cx);
        crate::rt::flush(cx);
    }
}

impl AppView {
    /// Moves the window's focus to the next tab stop (the previous with
    /// `shift`) outside the view, counted from the view's own: GPUI's tab
    /// order also holds the components' handles, inserted wherever the
    /// surface prepaints them, and they are skipped. False when there is
    /// none: the focus is left on the view.
    fn leave(&self, shift: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        window.focus(&self.focus, cx);
        // Each component's handle at most once, the view's, then out.
        let tries = crate::native::component_count(&self.surface) + 2;
        for _ in 0..tries {
            if shift {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            let inside =
                self.focus.is_focused(window) || crate::native::focused(&self.surface, window);
            if !inside {
                return true;
            }
            if self.focus.is_focused(window) {
                // Back at the view: no stop outside it.
                break;
            }
        }
        window.focus(&self.focus, cx);
        false
    }
}

impl Drop for AppView {
    fn drop(&mut self) {
        self.surface.stop();
    }
}

impl Focusable for AppView {
    fn focus_handle(&self, _cx: &GpuiApp) -> FocusHandle {
        self.focus.clone()
    }
}

/// The window's input handler while a field of the surface has the focus
/// (registered by the surface's element): text and input methods go to
/// that field ([`crate::text_input::ime`]).
impl gpui::EntityInputHandler for AppView {
    fn text_for_range(
        &mut self,
        range: std::ops::Range<usize>,
        adjusted: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        crate::text_input::ime::text_for_range(&self.surface, range, adjusted)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::UTF16Selection> {
        crate::text_input::ime::selected_text_range(&self.surface)
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        crate::text_input::ime::marked_text_range(&self.surface)
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        crate::text_input::ime::unmark_text(&self.surface);
        crate::rt::flush(cx);
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::text_input::ime::replace_text_in_range(&self.surface, range, text);
        crate::rt::flush(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        text: &str,
        selected: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::text_input::ime::replace_and_mark_text_in_range(
            &self.surface,
            range,
            text,
            selected,
        );
        crate::rt::flush(cx);
    }

    fn bounds_for_range(
        &mut self,
        range: std::ops::Range<usize>,
        _element_bounds: gpui::Bounds<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        crate::text_input::ime::bounds_for_range(&self.surface, range)
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        crate::text_input::ime::character_index_for_point(&self.surface, point)
    }

    fn set_selected_text_range(
        &mut self,
        range: std::ops::Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::text_input::ime::set_selected_text_range(&self.surface, range);
        crate::rt::flush(cx);
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        crate::text_input::ime::text_length_utf16(&self.surface)
    }

    fn text_input_configuration(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> gpui::TextInputConfiguration {
        crate::text_input::ime::configuration(&self.surface)
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surface = SurfaceElement {
            surface: self.surface.clone(),
            focus: self.focus.clone(),
            view: cx.entity().downgrade(),
        };
        // A dev session draws its status line and error overlay above.
        #[cfg(feature = "dev")]
        let surface = crate::dev::wrap(self, surface.into_any_element(), cx);
        // The window's chrome (title bar, menu bar) goes around the surface.
        let content = crate::chrome::render(&self.surface, surface, window, cx);
        // The div registers the view's tab stop, which only GPUI's own
        // elements can.
        gpui::div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::focus_next))
            .on_action(cx.listener(Self::focus_prev))
            .size_full()
            .child(content)
    }
}

fn initial_environment() -> Environment {
    Environment {
        window: (0.0, 0.0),
        color_scheme: ColorScheme::Light,
        os: if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        },
        idiom: Idiom::Desktop,
        ..Environment::default()
    }
}

/// Opens a window showing `source` in an [`AppView`] (in a running GPUI
/// app).
pub fn open_app_window(
    source: AppSource,
    options: AppOptions,
    cx: &mut GpuiApp,
) -> anyhow::Result<gpui::WindowHandle<AppView>> {
    let size = gpui::size(
        gpui::px(options.size.0 as f32),
        gpui::px(options.size.1 as f32),
    );
    let bounds = gpui::Bounds::centered(None, size, cx);
    let window_options = gpui::WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
        titlebar: Some(crate::chrome::titlebar_options(&options.title)),
        app_id: Some(options.app_id.clone()),
        // Linux: the window manager's decorations until the app hides the
        // title bar (`crate::chrome`).
        window_decorations: Some(gpui::WindowDecorations::Server),
        // GPUI draws unfocused windows at 30 fps by default: animations
        // run at the display's rate wherever the keyboard is, as in the
        // other backends.
        inactive_frame_interval: None,
        ..Default::default()
    };
    let script = options.script.clone();
    let handle = cx.open_window(window_options, move |window, cx| {
        let title = options.title.clone();
        let view: Entity<AppView> = cx.new(|cx| AppView::new(options, window, cx));
        crate::chrome::set_standalone(view.read(cx).surface(), &title);
        match source {
            AppSource::Rust(root) => view.read(cx).load(App::Rust(root)),
            AppSource::Bundle(bundle) => view.read(cx).load(App::Js(bundle)),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => crate::dev::start(&view, url, cx),
        }
        let focus = view.read(cx).focus.clone();
        crate::system::own_window(view.read(cx).surface());
        window.focus(&focus, cx);
        view
    })?;
    if let Some(script) = script {
        crate::script::run(handle, &script, cx);
    }
    Ok(handle)
}

/// Runs `source` in a window until it is closed. Fails when the window
/// cannot open or [`AppOptions::script`] failed.
pub fn run_app(source: AppSource, options: AppOptions) -> std::process::ExitCode {
    let ok = Rc::new(std::cell::Cell::new(true));
    let result = ok.clone();
    gpui_platform::application().run(move |cx| {
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        if let Err(err) = open_app_window(source, options, cx) {
            log::error!("cannot open a window: {err}");
            ok.set(false);
            cx.quit();
            return;
        }
        cx.activate(true);
    });
    if result.get() && !crate::script::failed() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
