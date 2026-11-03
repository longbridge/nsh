#[cfg(feature = "dev")]
#[path = "agent.rs"]
mod agent;
#[cfg(feature = "dev")]
#[path = "dev.rs"]
mod dev;

use std::{io, path::PathBuf, sync::Arc, time::Instant};

use nsh_core::{Bounds, Points, ROOT_ID, Size};
use nsh_engine::{App, AppError, Engine, EngineConfig, Environment, HostChannel, Idiom, Limits};
use ratatui_core::{buffer::Buffer, layout::Rect, terminal::Frame, widgets::Widget};

use crate::{
    Appearance, CellSize, CellTextEngine, ComponentFactory, Handled, Input,
    input::Pointer,
    mount::Mounter,
    render::{self, Hit},
    surface::{Message, Sink, Surface},
};

/// The machine's operating system, reported as the desktop hosts report theirs:
/// files, processes and OS-specific behavior follow it.
pub(crate) const OS: nsh_core::platform::Os = if cfg!(target_os = "macos") {
    nsh_core::platform::Os::MacOs
} else if cfg!(target_os = "windows") {
    nsh_core::platform::Os::Windows
} else {
    nsh_core::platform::Os::Linux
};

/// Called from the engine thread to schedule a render on the embedding thread.
pub type WakeCallback = Arc<dyn Fn() + Send + Sync>;

/// Terminal information supplied by the host. Further capabilities are added with their renderers.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TerminalCapabilities {
    /// System appearance; dark when the host has no background information.
    pub background: nsh_core::platform::ColorScheme,
    /// The colors the terminal can show; app colors are drawn at this depth.
    pub colors: ColorDepth,
    /// The terminal's default background (OSC 11). At 256 colors or more
    /// it is the base of the terminal's surfaces: app fills, selections,
    /// hover and press drawn as steps of it mixed toward
    /// [`Self::foreground_color`], secondary text as fades. Without it
    /// the look is bandless: no fills, reverse video and dim text.
    pub background_color: Option<[u8; 3]>,
    /// The terminal's default foreground (OSC 10): what the surfaces' steps
    /// mix toward (white or black against the background when unknown).
    pub foreground_color: Option<[u8; 3]>,
    /// The terminal reports the Command (Super) modifier, through the kitty
    /// keyboard protocol. Without it, Ctrl stands in for Cmd in the app's
    /// shortcuts on macOS.
    pub command_key: bool,
}

/// The colors a terminal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ColorDepth {
    /// 24-bit RGB (`COLORTERM=truecolor`).
    #[default]
    TrueColor,
    /// The xterm 256-color palette.
    Ansi256,
    /// The 16 named colors.
    Ansi16,
}

impl Default for TerminalCapabilities {
    fn default() -> Self {
        Self {
            background: nsh_core::platform::ColorScheme::Dark,
            colors: ColorDepth::default(),
            background_color: None,
            foreground_color: None,
            command_key: false,
        }
    }
}

/// Configuration of one terminal surface. Embedding uses bounded engine limits.
#[non_exhaustive]
pub struct AppOptions {
    /// Grid scale, shared by layout, drawing and input.
    pub cells: CellSize,
    /// Terminal properties determined by the embedding host.
    pub capabilities: TerminalCapabilities,
    /// App appearance override.
    pub appearance: Appearance,
    /// Notification that the surface has pending work.
    pub wake: Option<WakeCallback>,
    /// VM and core resource limits.
    pub limits: Limits,
    /// Image and response cache policy.
    pub cache: nsh_engine::CacheOptions,
    /// Persistent app data directory.
    pub data_dir: Option<PathBuf>,
    /// Registries available to app code.
    pub ui_sets: Vec<nsh_core::element::Registry>,
    /// Embedding app's channel, retained across loads.
    pub host: HostChannel,
    /// Optional app storage implementation.
    pub storage: Option<Arc<dyn nsh_core::storage::StorageBackend>>,
    /// Optional asset reader supplied by the embedding app.
    pub assets: Option<Arc<dyn nsh_engine::Assets>>,
    /// Clipboard implementation supplied by the host.
    pub clipboard: Option<Arc<dyn nsh_core::clipboard::ClipboardBackend>>,
    /// Log engine timings through `nsh::perf`.
    pub trace: bool,
    /// Show the terminal frame monitor.
    pub fps: bool,
    /// Root linked into an app-specific development host.
    #[cfg(feature = "dev")]
    pub root: Option<nsh_engine::Root>,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            cells: CellSize::default(),
            capabilities: TerminalCapabilities::default(),
            appearance: Appearance::System,
            wake: None,
            limits: Limits::default(),
            cache: nsh_engine::CacheOptions::default(),
            data_dir: None,
            ui_sets: vec![nsh_base::registry()],
            host: HostChannel::default(),
            storage: None,
            assets: None,
            clipboard: None,
            trace: false,
            fps: false,
            #[cfg(feature = "dev")]
            root: None,
        }
    }
}

type ErrorCallback = Box<dyn FnMut(&AppError)>;
type TextErrorCallback = Box<dyn FnMut(&str)>;

/// One passive NSH surface. Only rendering applies pending engine commits and invokes callbacks.
#[non_exhaustive]
pub struct AppView {
    options: AppOptions,
    perf: Option<crate::perf::Monitor>,
    output: crate::perf::SharedOutput,
    #[cfg(feature = "dev")]
    dev: Option<dev::DevSession>,
    /// The development server's `asset:/` files, unless the embedder supplies a reader.
    #[cfg(feature = "dev")]
    dev_assets: Option<Arc<dyn nsh_engine::Assets>>,
    #[cfg(feature = "dev")]
    diagnostics: dev::Diagnostics,
    #[cfg(feature = "dev")]
    test: Option<agent::TestSession>,
    surface: Surface,
    tree: Mounter,
    area: Rect,
    focused: bool,
    clipboard: Arc<dyn nsh_core::clipboard::ClipboardBackend>,
    kill: String,
    field_drag: Option<FieldDrag>,
    /// The next `draw_terminal` repaints every Cell (the terminal came back to focus).
    repaint: bool,
    pointer: Pointer,
    bar_drag: Option<crate::input::BarDrag>,
    cursor_style: crate::CursorStyle,
    on_cursor_style: Option<Box<dyn FnMut(crate::CursorStyle)>>,
    selection: crate::selection::Selection,
    hits: Vec<Hit>,
    geometry: Vec<crate::render::Geometry>,
    keyboard: crate::keyboard::Keyboard,
    minibuffer: crate::search::Minibuffer,
    environment: Environment,
    appearance_changed: bool,
    on_error: Option<TextErrorCallback>,
    on_app_error: Option<ErrorCallback>,
    on_appearance: Option<Box<dyn FnMut(Appearance)>>,
}

impl Default for AppView {
    fn default() -> Self {
        Self::new(AppOptions::default())
    }
}

impl AppView {
    /// Creates an empty surface; it takes over no terminal state or event loop.
    pub fn new(options: AppOptions) -> Self {
        let environment = Environment {
            os: OS,
            idiom: Idiom::Terminal,
            color_scheme: options.appearance.resolve(options.capabilities.background),
            reduce_motion: false,
            // The core lays out on the terminal's Cells: every frame here is
            // whole Cells and every border owns its own.
            cell: Some(nsh_core::Size::new(
                options.cells.width(),
                options.cells.height(),
            )),
            // Fills are surfaces (bands, padded, no line) where the painter
            // has a ramp for them: the same condition as `Ramp::new`.
            cell_surfaces: crate::palette::Ramp::new(
                options.capabilities.colors,
                options.capabilities.background_color,
                options.capabilities.foreground_color,
            )
            .is_some(),
            ..Default::default()
        };
        let clipboard = options
            .clipboard
            .clone()
            .unwrap_or_else(|| Arc::new(nsh_core::clipboard::MemoryClipboard::default()));
        let surface = Surface::default();
        surface.wake.set(options.wake.clone());
        let output = crate::perf::SharedOutput::default();
        output.enabled.set(options.fps);
        Self {
            perf: options.fps.then(crate::perf::Monitor::new),
            output,
            options,
            #[cfg(feature = "dev")]
            dev: None,
            #[cfg(feature = "dev")]
            dev_assets: None,
            #[cfg(feature = "dev")]
            diagnostics: dev::Diagnostics::default(),
            #[cfg(feature = "dev")]
            test: None,
            surface,
            tree: Mounter::default(),
            area: Rect::default(),
            focused: true,
            clipboard,
            kill: String::new(),
            field_drag: None,
            repaint: false,
            pointer: Pointer::default(),
            bar_drag: None,
            cursor_style: crate::CursorStyle::Default,
            on_cursor_style: None,
            selection: Default::default(),
            hits: Vec::new(),
            geometry: Vec::new(),
            keyboard: Default::default(),
            minibuffer: Default::default(),
            environment,
            appearance_changed: false,
            on_error: None,
            on_app_error: None,
            on_appearance: None,
        }
    }

    /// Replaces the app and starts its engine. Startup errors of app code arrive during render.
    pub fn load(&mut self, app: impl Into<App>) -> io::Result<()> {
        self.stop();
        self.load_app(app.into())
    }

    fn load_app(&mut self, app: App) -> io::Result<()> {
        self.stop_engine();
        let (sender, inbox) = crossbeam_channel::unbounded();
        let viewport = self.size();
        self.environment.window = viewport;
        let mut config = EngineConfig::new(app);
        config.viewport = viewport;
        config.environment = self.environment;
        config.header_heights = nsh_core::NO_NATIVE_HEADER;
        config.ui_sets = self.options.ui_sets.clone();
        config.network = Some(nsh_engine::net::network());
        config.limits = self.options.limits;
        config.cache = self.options.cache.clone();
        config.data_dir = self.options.data_dir.clone();
        config.assets = self.options.assets.clone();
        #[cfg(feature = "dev")]
        if config.assets.is_none() {
            config.assets = self.dev_assets.clone();
        }
        config.storage = self.options.storage.clone();
        config.clipboard = Some(self.clipboard.clone());
        config.host = self.options.host.clone();
        config.trace = self.options.trace;
        #[cfg(feature = "dev")]
        if let Some(test) = &self.test {
            test.model.set_up_engine(&mut config);
        }
        self.clipboard = config
            .clipboard
            .clone()
            .unwrap_or_else(|| Arc::new(nsh_core::clipboard::MemoryClipboard::default()));
        let cells = self.options.cells;
        let engine = Engine::start(
            config,
            move || CellTextEngine::new(cells),
            Sink {
                sender,
                wake: self.surface.wake.clone(),
            },
        )?;
        #[cfg(feature = "dev")]
        if let Some(test) = &self.test {
            test.model.engine_started(engine.control());
        }
        if let Some(perf) = &self.perf {
            engine.control().set_perf_monitor(Some(perf.engine.clone()));
        }
        self.surface.engine = Some(engine);
        self.surface.inbox = inbox;
        Ok(())
    }

    /// Connects to a development server, replacing the current app and connection.
    #[cfg(feature = "dev")]
    pub fn connect_dev_server(&mut self, url: impl Into<String>) -> io::Result<()> {
        self.stop();
        let url = url.into();
        self.dev_assets = dev::ServerAssets::for_server(&url)
            .map(|assets| Arc::new(assets) as Arc<dyn nsh_engine::Assets>);
        self.dev = Some(dev::DevSession::start(
            url,
            &self.options,
            self.surface.wake.clone(),
        )?);
        Ok(())
    }

    /// Selects the development log shown by `C-h e`; the file is read only while the panel is open.
    #[cfg(feature = "dev")]
    pub fn set_dev_log_path(&mut self, path: PathBuf) {
        self.diagnostics.log_path = Some(path);
    }

    /// Shows or hides the performance line. Hiding it drops all samples and timers.
    pub fn set_fps_monitor(&mut self, enabled: bool) {
        if enabled == self.perf.is_some() {
            return;
        }
        self.perf = enabled.then(crate::perf::Monitor::new);
        self.output.enabled.set(enabled);
        if let Some(engine) = &self.surface.engine {
            engine
                .control()
                .set_perf_monitor(self.perf.as_ref().map(|p| p.engine.clone()));
        }
        self.surface.wake.notify();
    }

    /// The shared engine monitor while the performance line is enabled.
    pub fn perf_monitor(&self) -> Option<Arc<nsh_engine::perf::PerfMonitor>> {
        self.perf.as_ref().map(|p| p.engine.clone())
    }

    /// Timings and output of the last completed `draw_terminal`, if monitoring is enabled.
    pub fn frame_metrics(&self) -> Option<crate::FrameMetrics> {
        self.perf.as_ref().and_then(|p| p.last)
    }

    /// Wraps the host's backend to count the actual Cell diff without scanning the buffer again.
    pub fn measure_backend<B>(&self, backend: B) -> crate::MeasuredBackend<B> {
        self.output.cells.set(Some(0));
        crate::MeasuredBackend {
            inner: backend,
            output: self.output.clone(),
        }
    }

    /// Wraps the terminal writer to count successful bytes, including escape sequences.
    pub fn measure_writer<W>(&self, writer: W) -> crate::MeasuredWriter<W> {
        self.output.bytes.set(Some(0));
        crate::MeasuredWriter {
            inner: writer,
            output: self.output.clone(),
        }
    }

    /// Draws a full-terminal surface and completes its optional performance sample after flushing.
    /// Embedders owning a larger layout may still use `render` or the Widget interface.
    pub fn draw_terminal<B: ratatui_core::backend::Backend>(
        &mut self,
        terminal: &mut ratatui_core::terminal::Terminal<B>,
    ) -> Result<(), B::Error> {
        if std::mem::take(&mut self.repaint) {
            terminal.clear()?;
        }
        if self.perf.is_none() {
            terminal.draw(|frame| self.render(frame, frame.area()))?;
            return Ok(());
        }
        let app_frame = self
            .surface
            .deadline()
            .is_some_and(|due| due <= Instant::now())
            || terminal.size()? != self.area.into();
        self.output.bytes.set(self.output.bytes.get().map(|_| 0));
        self.output.cells.set(self.output.cells.get().map(|_| 0));
        let mut flush = None;
        terminal.draw(|frame| {
            self.render(frame, frame.area());
            flush = Some(Instant::now());
        })?;
        let at = Instant::now();
        if let Some(perf) = &mut self.perf {
            let mut sample = perf.pending;
            sample.flush = at.duration_since(flush.unwrap());
            sample.bytes = self.output.bytes.get();
            sample.cells = self.output.cells.get();
            perf.finish(at, sample, app_frame);
        }
        Ok(())
    }

    /// The channel shared with the running app.
    pub fn host(&self) -> HostChannel {
        self.options.host.clone()
    }

    /// Registers an embedding app's component for subsequent mounts.
    pub fn register_component(
        &mut self,
        name: impl Into<String>,
        factory: ComponentFactory,
    ) -> Result<(), String> {
        let name = name.into();
        if nsh_core::platform::is_reserved_component(&name) {
            return Err("component names under nsh/ are reserved".into());
        }
        self.tree.components.insert(name, factory);
        Ok(())
    }

    /// Changes the appearance; its callback runs on the next render.
    pub fn set_appearance(&mut self, appearance: Appearance) {
        if self.options.appearance == appearance {
            return;
        }
        self.options.appearance = appearance;
        self.environment.color_scheme = appearance.resolve(self.options.capabilities.background);
        if let Some(engine) = &self.surface.engine {
            engine.control().set_environment(self.environment);
        }
        self.appearance_changed = true;
        self.surface.wake.notify();
    }

    /// Gives or removes the surface's keyboard focus.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.minibuffer.end();
            self.field_drag = None;
            self.bar_drag = None;
            self.selection.clear();
            if let Some(engine) = &self.surface.engine {
                self.keyboard.focus(None, &mut self.tree, &engine.control());
                self.pointer.leave(&mut crate::input::PointerContext {
                    area: self.area,
                    cells: self.options.cells,
                    hits: &self.hits,
                    geometry: &self.geometry,
                    tree: &mut self.tree,
                    control: &engine.control(),
                });
            }
            self.surface.wake.notify();
            self.pointer.cancel(
                &mut self.tree,
                self.surface.engine.as_ref().map(Engine::control).as_ref(),
            );
        }
    }

    /// Receives app errors as text, during rendering on the embedder's thread.
    pub fn on_error(&mut self, callback: impl FnMut(&str) + 'static) {
        self.on_error = Some(Box::new(callback));
    }

    /// Receives structured app errors during rendering on the embedder's thread.
    pub fn on_app_error(&mut self, callback: impl FnMut(&AppError) + 'static) {
        self.on_app_error = Some(Box::new(callback));
    }

    /// Receives appearance changes during rendering on the embedder's thread.
    pub fn on_appearance_changed(&mut self, callback: impl FnMut(Appearance) + 'static) {
        self.on_appearance = Some(Box::new(callback));
    }

    /// The terminal cursor's shape the surface wants: a bar while a text
    /// field is edited, the user's own otherwise.
    pub fn cursor_style(&self) -> crate::CursorStyle {
        self.cursor_style
    }

    /// Receives cursor shape changes during rendering on the embedder's
    /// thread; hosts write the shape to the terminal (`CSI Ps SP q`).
    pub fn on_cursor_style_changed(&mut self, callback: impl FnMut(crate::CursorStyle) + 'static) {
        self.on_cursor_style = Some(Box::new(callback));
    }

    /// Replaces the scheduler notification. It must only enqueue work, never render on the engine thread.
    pub fn set_wake_callback(&mut self, callback: Option<WakeCallback>) {
        self.surface.wake.set(callback);
    }

    /// Earliest requested render, or no deadline while idle. Pending commits are due now.
    pub fn next_frame_deadline(&self) -> Option<Instant> {
        let mut deadline = self.surface.deadline();
        if let Some(refresh) = self.perf.as_ref().and_then(|p| p.refresh) {
            deadline = Some(deadline.map_or(refresh, |due| due.min(refresh)));
        }
        #[cfg(feature = "dev")]
        if self.diagnostics.open {
            return Some(deadline.map_or(self.diagnostics.refresh, |due| {
                due.min(self.diagnostics.refresh)
            }));
        }
        deadline
    }

    /// Whether app code requested exit. Standalone loops close; embedders decide their own response.
    pub fn exit_requested(&self) -> bool {
        self.tree.exit
    }

    /// Routes input against the last completed render. Unused keys and outside-area events are ignored.
    pub fn handle(&mut self, input: Input) -> Handled {
        // Ctrl+C is never the diagnostics panel's: it copies or quits.
        #[cfg(feature = "dev")]
        let interrupt = matches!(&input, Input::Key { key, modifiers, .. }
            if key == "c" && modifiers.ctrl && !modifiers.alt && !modifiers.shift && !modifiers.cmd);
        #[cfg(feature = "dev")]
        if (self.dev.is_some() || self.diagnostics.log_path.is_some())
            && !interrupt
            && self.focused
            && (self.diagnostics.open
                || (self.editing_field().is_none() && !self.minibuffer.is_open()))
            && self.diagnostics.handle(&input)
        {
            self.pointer.cancel(
                &mut self.tree,
                self.surface.engine.as_ref().map(Engine::control).as_ref(),
            );
            if std::mem::take(&mut self.diagnostics.toggle_perf) {
                self.set_fps_monitor(self.perf.is_none());
            }
            if self.diagnostics.reload {
                self.diagnostics.reload = false;
                if let Some(mut dev) = self.dev.take() {
                    dev.reload_latest(self);
                    self.dev = Some(dev);
                }
            }
            self.surface.wake.notify();
            return Handled::Yes;
        }
        if self.tree.dialogs.is_shown()
            && let Some(handled) = self.handle_dialog(&input)
        {
            return handled;
        }
        if self.minibuffer.is_open() {
            match &input {
                Input::Paste(text) => {
                    self.search(|minibuffer, scene| minibuffer.paste(text, scene));
                    return Handled::Yes;
                }
                Input::Mouse(_) => {
                    self.minibuffer.end();
                    self.surface.wake.notify();
                }
                _ => {}
            }
        }
        match input {
            Input::Focus(focused) => {
                // The terminal may have drawn over the surface meanwhile.
                self.repaint |= focused && !self.focused;
                self.set_focused(focused);
                Handled::Yes
            }
            Input::Mouse(mouse) => {
                if matches!(mouse.kind, crate::MouseKind::Down(_)) {
                    self.pointer.count_click(mouse);
                }
                if let Some(handled) = self.handle_scrollbar(mouse) {
                    return handled;
                }
                if self.handle_field_mouse(mouse) {
                    self.selection.clear();
                    self.surface.wake.notify();
                    return Handled::Yes;
                }
                let Some(engine) = &self.surface.engine else {
                    return Handled::Ignored;
                };
                let scene = crate::selection::Scene {
                    area: self.area,
                    cells: self.options.cells,
                    hits: &self.hits,
                    geometry: &self.geometry,
                    tree: &self.tree,
                };
                if self.selection.handle(mouse, scene, self.pointer.clicks()) {
                    self.pointer.cancel(&mut self.tree, Some(&engine.control()));
                    self.keyboard.focus(None, &mut self.tree, &engine.control());
                    self.surface.wake.notify();
                    return Handled::Yes;
                }
                if matches!(mouse.kind, crate::MouseKind::Down(crate::MouseButton::Left)) {
                    self.surface.wake.notify();
                }
                let handled = if matches!(
                    mouse.kind,
                    crate::MouseKind::Down(crate::MouseButton::Right)
                ) {
                    self.pointer.context_menu(
                        mouse,
                        &mut crate::input::PointerContext {
                            area: self.area,
                            cells: self.options.cells,
                            hits: &self.hits,
                            geometry: &self.geometry,
                            tree: &mut self.tree,
                            control: &engine.control(),
                        },
                    )
                } else if matches!(mouse.kind, crate::MouseKind::Wheel { .. }) {
                    let handled = self.keyboard.wheel(
                        mouse,
                        self.area,
                        self.options.cells,
                        &self.hits,
                        &mut self.tree,
                        &self.geometry,
                    );
                    if handled == Handled::Yes {
                        self.pointer.cancel(&mut self.tree, Some(&engine.control()));
                    }
                    handled
                } else {
                    self.pointer.handle(
                        mouse,
                        crate::input::PointerContext {
                            area: self.area,
                            cells: self.options.cells,
                            hits: &self.hits,
                            geometry: &self.geometry,
                            tree: &mut self.tree,
                            control: &engine.control(),
                        },
                    )
                };
                if handled == Handled::Yes {
                    self.surface.wake.notify();
                }
                handled
            }
            Input::Key {
                key,
                modifiers,
                release,
            } => {
                if release {
                    Handled::Ignored
                } else {
                    self.handle_key(&key, modifiers, false)
                }
            }
            Input::KeyRepeat { key, modifiers } => self.handle_key(&key, modifiers, true),
            Input::Paste(text) => {
                let Some(id) = self.editing_field() else {
                    return Handled::Ignored;
                };
                let Some(engine) = &self.surface.engine else {
                    return Handled::Ignored;
                };
                let view = self.tree.get_mut(id).unwrap();
                view.field.as_mut().unwrap().paste(
                    view.input.as_ref().unwrap(),
                    &text,
                    id,
                    &engine.control(),
                );
                self.surface.wake.notify();
                Handled::Yes
            }
        }
    }

    /// A shown system dialog is modal: it takes every key and press, and the
    /// app's views get no input until it is answered. Ctrl+C still quits, and
    /// focus reports still reach the surface (`None`).
    fn handle_dialog(&mut self, input: &Input) -> Option<Handled> {
        let outcome = match input {
            Input::Focus(_) => return None,
            Input::Key {
                key,
                modifiers,
                release,
            } => {
                if *release || !self.focused {
                    return Some(Handled::Ignored);
                }
                if let Some(stroke) = crate::key_scheme::keystroke(key, *modifiers)
                    && let Some(crate::key_scheme::Action::Quit) =
                        crate::key_scheme::terminal(stroke)
                {
                    self.tree.exit = true;
                    return Some(Handled::Yes);
                }
                self.tree.dialogs.key(key, *modifiers)
            }
            Input::KeyRepeat { key, modifiers } => self.tree.dialogs.key(key, *modifiers),
            Input::Mouse(mouse) => {
                let crate::MouseKind::Down(_) = mouse.kind else {
                    return Some(Handled::Yes);
                };
                // Views behind the dialog lose a press or hover they held.
                self.pointer.cancel(
                    &mut self.tree,
                    self.surface.engine.as_ref().map(Engine::control).as_ref(),
                );
                self.tree
                    .dialogs
                    .press(ratatui_core::layout::Position::new(mouse.column, mouse.row))
            }
            Input::Paste(_) => return Some(Handled::Yes),
        };
        if let Some(crate::dialog::Outcome::Answered(answer)) = outcome
            && let Some(engine) = &self.surface.engine
        {
            answer.dispatch(&engine.control());
        }
        self.surface.wake.notify();
        Some(Handled::Yes)
    }

    fn handle_key(&mut self, key: &str, modifiers: crate::Modifiers, repeat: bool) -> Handled {
        if !self.focused {
            return Handled::Ignored;
        }
        let Some(stroke) = crate::key_scheme::keystroke(key, modifiers) else {
            return Handled::Ignored;
        };
        let Some(engine) = &self.surface.engine else {
            // No app to ask: Ctrl+C still quits.
            if let Some(crate::key_scheme::Action::Quit) = crate::key_scheme::terminal(stroke) {
                self.tree.exit = true;
                return Handled::Yes;
            }
            return Handled::Ignored;
        };
        let control = engine.control();
        if self.minibuffer.is_open()
            && let Some(crate::search::Outcome::Consumed) =
                self.search(|minibuffer, scene| minibuffer.key(key, stroke, scene))
        {
            return Handled::Yes;
        }
        let command_key = self.options.capabilities.command_key;
        let selecting = self.selection.is_active();
        let handled = if self
            .keyboard
            .shortcut(&self.tree, stroke, repeat, command_key, &control)
            || self.handle_field_key(key, stroke, &control)
            || self.selection.key(
                key,
                modifiers,
                &self.tree,
                &self.geometry,
                self.clipboard.as_ref(),
            ) {
            Handled::Yes
        } else if let Some(crate::key_scheme::Action::Search(forward)) =
            crate::key_scheme::terminal(stroke)
        {
            self.search(|minibuffer, scene| minibuffer.start(forward, scene));
            Handled::Yes
        } else if let Some(action) = crate::key_scheme::terminal(stroke) {
            self.keyboard
                .action(action, &mut self.tree, &self.geometry, &control, modifiers)
        } else {
            Handled::Ignored
        };
        if handled == Handled::Yes || (selecting && !self.selection.is_active()) {
            self.surface.wake.notify();
        }
        handled
    }

    fn search<R>(
        &mut self,
        act: impl FnOnce(&mut crate::search::Minibuffer, &mut crate::search::Scene) -> R,
    ) -> Option<R> {
        let control = self.surface.engine.as_ref()?.control();
        let mut scene = crate::search::Scene {
            tree: &mut self.tree,
            geometry: &self.geometry,
            keyboard: &mut self.keyboard,
            control: &control,
            cells: self.options.cells,
            area: self.area,
        };
        let result = act(&mut self.minibuffer, &mut scene);
        self.surface.wake.notify();
        Some(result)
    }

    fn editing_field(&self) -> Option<nsh_core::NodeId> {
        if !self.focused {
            return None;
        }
        let id = self.keyboard.focused?;
        let view = self.tree.get(id)?;
        (view.input.as_ref().is_some_and(|config| config.editable)
            && !view.props.accessibility.state.disabled)
            .then_some(id)
    }

    fn handle_field_key(
        &mut self,
        key: &str,
        stroke: nsh_core::keymap::Keystroke,
        control: &nsh_engine::EngineControl,
    ) -> bool {
        let Some(id) = self.editing_field() else {
            return false;
        };
        let view = self.tree.get_mut(id).unwrap();
        let config = view.input.as_ref().unwrap();
        let field = view.field.as_mut().unwrap();
        // Ctrl+C copies a selection, as in the terminal's other programs;
        // without one it is the terminal's interrupt (the Key scheme's quit).
        let copy = stroke.key == nsh_core::keymap::Key::Char('c')
            && stroke.modifiers.ctrl
            && !stroke.modifiers.alt
            && !stroke.modifiers.shift
            && !stroke.modifiers.cmd;
        if copy && !field.model.has_selection() {
            return false;
        }
        let action = if copy {
            Some(crate::key_scheme::EditAction::Copy)
        } else {
            crate::key_scheme::editing(stroke, config.multiline)
        };
        if let Some(action) = action {
            if field.edit(
                action,
                stroke.modifiers.shift,
                crate::text_input::EditContext {
                    config,
                    kill: &mut self.kill,
                    clipboard: self.clipboard.as_ref(),
                    id,
                    control,
                },
            ) {
                self.keyboard.focus(None, &mut self.tree, control);
            }
            true
        } else if !stroke.modifiers.ctrl
            && !stroke.modifiers.alt
            && !stroke.modifiers.cmd
            && matches!(stroke.key, nsh_core::keymap::Key::Char(_))
        {
            field.insert(config, key, true, id, control);
            true
        } else {
            false
        }
    }

    /// Presses and drags on the scroll bars of the last render: a press on
    /// a track jumps there, the thumb follows the pointer while it is held.
    fn handle_scrollbar(&mut self, mouse: crate::MouseInput) -> Option<Handled> {
        use crate::{MouseButton, MouseKind};
        if let Some(drag) = &self.bar_drag {
            match mouse.kind {
                MouseKind::Move => {
                    if drag.moved(mouse, &mut self.tree) {
                        self.surface.wake.notify();
                    }
                    return Some(Handled::Yes);
                }
                MouseKind::Up(MouseButton::Left) => {
                    self.bar_drag = None;
                    return Some(Handled::Yes);
                }
                _ => self.bar_drag = None,
            }
        }
        let position = ratatui_core::layout::Position::new(mouse.column, mouse.row);
        if !self.area.contains(position) {
            return None;
        }
        if !matches!(mouse.kind, MouseKind::Down(MouseButton::Left)) {
            return None;
        }
        let bars = self.tree.scroll_bars.clone();
        let drag = crate::input::BarDrag::press(&bars, mouse, &mut self.tree)?;
        let control = self.surface.engine.as_ref().map(Engine::control);
        self.pointer.cancel(&mut self.tree, control.as_ref());
        self.selection.clear();
        self.minibuffer.end();
        self.bar_drag = Some(drag);
        self.surface.wake.notify();
        Some(Handled::Yes)
    }

    fn handle_field_mouse(&mut self, mouse: crate::MouseInput) -> bool {
        use ratatui_core::layout::Position;

        use crate::{MouseButton, MouseKind};
        let Some(engine) = &self.surface.engine else {
            return false;
        };
        let control = engine.control();
        let position = Position::new(mouse.column, mouse.row);
        let scope = crate::navigation::focus_scope(&self.tree);
        if let Some(FieldDrag {
            id,
            last,
            clicks,
            anchor,
        }) = self.field_drag
        {
            if self.keyboard.focused != Some(id)
                || !crate::keyboard::contains(&self.tree, scope, id)
            {
                self.field_drag = None;
            } else if matches!(
                mouse.kind,
                MouseKind::Move | MouseKind::Up(MouseButton::Left)
            ) {
                if position != last
                    && let Some(view) = self.tree.get_mut(id)
                {
                    let field = view.field.as_mut().unwrap();
                    field.drag(view.input.as_ref().unwrap(), position, clicks, anchor);
                    field.report(id, &control, false);
                }
                self.field_drag = if matches!(mouse.kind, MouseKind::Up(_)) {
                    None
                } else {
                    Some(FieldDrag {
                        id,
                        last: position,
                        clicks,
                        anchor,
                    })
                };
                return true;
            }
        }
        if !matches!(mouse.kind, MouseKind::Down(MouseButton::Left))
            || !self.area.contains(position)
        {
            return false;
        }
        let hit = self.hits.iter().rev().find(|hit| {
            hit.area.contains(position) && crate::keyboard::contains(&self.tree, scope, hit.node)
        });
        let id = hit.map(|hit| hit.node).filter(|&id| {
            self.tree.get(id).is_some_and(|view| {
                view.input.as_ref().is_some_and(|config| config.editable)
                    && !view.props.accessibility.state.disabled
            })
        });
        if let Some(id) = id {
            // The press still goes down as a touch, so open popovers hear a
            // press outside them, then the field takes the pointer.
            self.pointer.handle(
                mouse,
                crate::input::PointerContext {
                    area: self.area,
                    cells: self.options.cells,
                    hits: &self.hits,
                    geometry: &self.geometry,
                    tree: &mut self.tree,
                    control: &control,
                },
            );
            self.pointer.cancel(&mut self.tree, Some(&control));
            self.keyboard.focus(Some(id), &mut self.tree, &control);
            let clicks = self.pointer.clicks();
            let view = self.tree.get_mut(id).unwrap();
            let field = view.field.as_mut().unwrap();
            let anchor = field.click(
                view.input.as_ref().unwrap(),
                position,
                mouse.modifiers.shift,
                clicks,
            );
            field.report(id, &control, false);
            self.field_drag = Some(FieldDrag {
                id,
                last: position,
                clicks: if mouse.modifiers.shift { 1 } else { clicks },
                anchor,
            });
            true
        } else {
            if self.editing_field().is_some() {
                self.keyboard.focus(None, &mut self.tree, &control);
                self.surface.wake.notify();
            }
            false
        }
    }

    /// Applies complete pending batches, resizes to `area`, and draws into the frame.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.draw(area, frame.buffer_mut());
        #[cfg(feature = "dev")]
        if self.diagnostics.open {
            self.set_cursor_style(crate::CursorStyle::Default);
            return;
        }
        let mut style = crate::CursorStyle::Default;
        if self.focused {
            if let Some(cursor) = self.minibuffer.cursor() {
                frame.set_cursor_position(cursor);
            } else if let Some(cursor) = self.keyboard.cursor(&self.tree, &self.geometry) {
                frame.set_cursor_position(cursor);
                style = crate::CursorStyle::Bar;
            }
        }
        self.set_cursor_style(style);
    }

    fn set_cursor_style(&mut self, style: crate::CursorStyle) {
        if self.cursor_style != style {
            self.cursor_style = style;
            if let Some(callback) = &mut self.on_cursor_style {
                callback(style);
            }
        }
    }

    fn size(&self) -> Size<Points> {
        Size::new(
            self.options.cells.width() * f32::from(self.area.width),
            self.options.cells.height() * f32::from(self.area.height),
        )
    }

    fn draw(&mut self, area: Rect, buffer: &mut Buffer) {
        let started = self.perf.as_ref().map(|_| Instant::now());
        self.surface.begin_frame();
        #[cfg(feature = "dev")]
        if let Some(mut dev) = self.dev.take() {
            while let Ok(event) = dev.inbox.try_recv() {
                match event {
                    dev::Message::Event(event) => dev.event(self, *event),
                    dev::Message::Test(test) => self.test = Some(test),
                }
            }
            self.dev = Some(dev);
        }
        if self.area != area {
            self.pointer.cancel(
                &mut self.tree,
                self.surface.engine.as_ref().map(Engine::control).as_ref(),
            );
            self.area = area;
            self.environment.window = self.size();
            if let Some(engine) = &self.surface.engine {
                engine.control().resize(self.size());
                engine.control().set_environment(self.environment);
            }
        }
        let mut errors = Vec::new();
        while let Ok(message) = self.surface.inbox.try_recv() {
            match message {
                Message::Batch(batch) => {
                    #[cfg(feature = "dev")]
                    let observed = self.test.as_ref().map(|test| {
                        (
                            test.model.clone(),
                            nsh_core::MountBatch {
                                items: batch.items.clone(),
                            },
                        )
                    });
                    self.tree.apply(batch);
                    #[cfg(feature = "dev")]
                    if let Some((model, batch)) = observed {
                        model.apply(&batch);
                    }
                }
                Message::Error(error) => errors.push(error),
                Message::Frame => self.surface.request_frame(),
                #[cfg(feature = "dev")]
                Message::Reload => {
                    if let Some(mut dev) = self.dev.take() {
                        dev.reload(self);
                        self.dev = Some(dev);
                    }
                }
            }
        }
        if let Some(engine) = &self.surface.engine {
            let control = engine.control();
            for &id in &self.tree.scrolled {
                if let Some(view) = self.tree.get(id)
                    && let Some(scroll) = &view.scroll
                {
                    control.scroll(id, scroll.reported(view.frame.size), scroll.applied);
                }
            }
        }
        self.tree.scrolled.clear();
        if let Some(engine) = &self.surface.engine {
            for _ in 0..std::mem::take(&mut self.tree.back_requests) {
                crate::navigation::back(&mut self.tree, &engine.control(), false);
            }
        }
        if let Some(appearance) = self.tree.appearance.take() {
            self.set_appearance(appearance);
        }
        if self.appearance_changed {
            self.appearance_changed = false;
            if let Some(callback) = &mut self.on_appearance {
                callback(self.options.appearance);
            }
        }
        for error in errors {
            #[cfg(feature = "dev")]
            if let Some(test) = &self.test {
                test.model.error(error.to_string());
            }
            #[cfg(feature = "dev")]
            if self.dev.is_some() || self.diagnostics.log_path.is_some() {
                self.diagnostics.error("App error", &error.to_string());
            }
            if let Some(callback) = &mut self.on_error {
                callback(&error.to_string());
            }
            if let Some(callback) = &mut self.on_app_error {
                callback(&error);
            }
        }
        let size = self.size();
        if let Some(root) = self.tree.get_mut(ROOT_ID) {
            root.frame = Bounds::new(Default::default(), size);
        }
        if let Some(engine) = &self.surface.engine {
            crate::navigation::report_shown(&mut self.tree, &engine.control());
        }
        self.surface.animate(&mut self.tree);
        let drawing = started.map(|start| {
            let now = Instant::now();
            if let Some(perf) = &mut self.perf {
                perf.pending.apply = now.duration_since(start);
            }
            now
        });
        let capabilities = &self.options.capabilities;
        // The terminal's own colors give its surfaces (OSC 11 and 10, at
        // 256 colors or more); the bandless look elsewhere.
        let ramp = crate::palette::Ramp::new(
            capabilities.colors,
            capabilities.background_color,
            capabilities.foreground_color,
        );
        let page = crate::palette::Ground::page(self.environment.color_scheme, capabilities.colors)
            .with_ramp(ramp);
        let bands = crate::palette::Bands::new(
            self.environment.color_scheme,
            capabilities.colors,
            capabilities.background_color,
            capabilities.foreground_color,
        );
        let look = render::Look {
            cells: self.options.cells,
            page,
            bands,
        };
        render::draw(
            &mut self.tree,
            buffer,
            area,
            look,
            &mut self.hits,
            &mut self.geometry,
        );
        let scene = crate::selection::Scene {
            area,
            cells: self.options.cells,
            hits: &self.hits,
            geometry: &self.geometry,
            tree: &self.tree,
        };
        self.selection.draw(scene, buffer);
        if let Some(engine) = &self.surface.engine {
            self.pointer.rehover(&mut crate::input::PointerContext {
                area,
                cells: self.options.cells,
                hits: &self.hits,
                geometry: &self.geometry,
                tree: &mut self.tree,
                control: &engine.control(),
            });
        }
        if let Some(engine) = &self.surface.engine
            && self
                .keyboard
                .reconcile(&mut self.tree, &self.geometry, &engine.control())
        {
            self.surface.wake.notify();
        }
        if let Some(engine) = &self.surface.engine {
            let control = engine.control();
            for (id, focus) in std::mem::take(&mut self.tree.focus_requests) {
                // A target scrolled out of sight is focused and revealed,
                // as keyboard navigation reveals what it moves to.
                let target = focus
                    .then(|| {
                        self.keyboard
                            .candidates(&self.tree, &self.geometry)
                            .iter()
                            .find(|g| g.id == id)
                            .map(|g| g.frame)
                    })
                    .flatten();
                if let Some(frame) = target {
                    self.keyboard.focus(Some(id), &mut self.tree, &control);
                    crate::keyboard::reveal_frame(&mut self.tree, &self.geometry, id, frame);
                    self.surface.wake.notify();
                } else if !focus && self.keyboard.focused == Some(id) {
                    self.keyboard.focus(None, &mut self.tree, &control);
                    self.surface.wake.notify();
                }
            }
            if let Some(id) = self.keyboard.focused
                && let Some(field) = self.tree.get_mut(id).and_then(|view| view.field.as_mut())
            {
                field.report(id, &control, false);
            }
        }
        self.minibuffer
            .draw(&self.tree, &self.geometry, self.options.cells, area, buffer);
        #[cfg(feature = "dev")]
        self.diagnostics.draw(area, buffer);
        if let (Some(drawing), Some(perf)) = (drawing, &mut self.perf) {
            perf.pending.draw = drawing.elapsed();
            perf.draw(area, buffer);
        }
        #[cfg(feature = "dev")]
        {
            let requests: Vec<_> = self
                .test
                .as_ref()
                .map(|test| test.inbox.try_iter().collect())
                .unwrap_or_default();
            for request in requests {
                request(self, buffer);
            }
        }
    }

    /// Stops the engine, drops queued commits, and clears the surface's app-owned state.
    pub fn stop(&mut self) {
        #[cfg(feature = "dev")]
        {
            self.dev = None;
            self.test = None;
            let log_path = self.diagnostics.log_path.take();
            self.diagnostics = dev::Diagnostics::default();
            self.diagnostics.log_path = log_path;
        }
        self.stop_engine();
    }

    fn stop_engine(&mut self) {
        self.pointer.cancel(
            &mut self.tree,
            self.surface.engine.as_ref().map(Engine::control).as_ref(),
        );
        self.selection.clear();
        self.pointer = Pointer::default();
        self.surface.stop();
        self.tree.clear();
        self.hits.clear();
        self.geometry.clear();
        self.keyboard = Default::default();
        self.minibuffer = Default::default();
        self.kill.clear();
        self.field_drag = None;
    }
}

/// A primary press held in a text field: where it was last, its clicks and
/// the unit (character, word, line) it pressed, which its drag extends.
#[derive(Clone, Copy)]
struct FieldDrag {
    id: nsh_core::NodeId,
    last: ratatui_core::layout::Position,
    clicks: u32,
    anchor: (usize, usize),
}

impl Widget for &mut AppView {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        self.draw(area, buffer);
    }
}
