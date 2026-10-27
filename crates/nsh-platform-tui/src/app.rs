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

/// Called from the engine thread to schedule a render on the embedding thread.
pub type WakeCallback = Arc<dyn Fn() + Send + Sync>;

/// Terminal information supplied by the host. Further capabilities are added with their renderers.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TerminalCapabilities {
    /// System appearance; dark when the host has no background information.
    pub background: nsh_core::platform::ColorScheme,
}

impl Default for TerminalCapabilities {
    fn default() -> Self {
        Self {
            background: nsh_core::platform::ColorScheme::Dark,
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
        }
    }
}

type ErrorCallback = Box<dyn FnMut(&AppError)>;
type TextErrorCallback = Box<dyn FnMut(&str)>;

/// One passive NSH surface. Only rendering applies pending engine commits and invokes callbacks.
#[non_exhaustive]
pub struct AppView {
    options: AppOptions,
    surface: Surface,
    tree: Mounter,
    area: Rect,
    focused: bool,
    pointer: Pointer,
    hits: Vec<Hit>,
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
            idiom: Idiom::Desktop,
            color_scheme: options.appearance.resolve(options.capabilities.background),
            reduce_motion: true,
            ..Default::default()
        };
        let surface = Surface::default();
        surface.wake.set(options.wake.clone());
        Self {
            options,
            surface,
            tree: Mounter::default(),
            area: Rect::default(),
            focused: true,
            pointer: Pointer::default(),
            hits: Vec::new(),
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
        config.storage = self.options.storage.clone();
        config.clipboard = self.options.clipboard.clone();
        config.host = self.options.host.clone();
        config.trace = self.options.trace;
        let cells = self.options.cells;
        let engine = Engine::start(
            config,
            move || CellTextEngine::new(cells),
            Sink {
                sender,
                wake: self.surface.wake.clone(),
            },
        )?;
        self.surface.engine = Some(engine);
        self.surface.inbox = inbox;
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
            self.pointer
                .cancel(self.surface.engine.as_ref().map(Engine::control).as_ref());
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

    /// Replaces the scheduler notification. It must only enqueue work, never render on the engine thread.
    pub fn set_wake_callback(&mut self, callback: Option<WakeCallback>) {
        self.surface.wake.set(callback);
    }

    /// Earliest requested render, or no deadline while idle. Pending commits are due now.
    pub fn next_frame_deadline(&self) -> Option<Instant> {
        self.surface.deadline()
    }

    /// Whether app code requested exit. Standalone loops close; embedders decide their own response.
    pub fn exit_requested(&self) -> bool {
        self.tree.exit
    }

    /// Routes input against the last completed render. Unused keys and outside-area events are ignored.
    pub fn handle(&mut self, input: Input) -> Handled {
        match input {
            Input::Focus(focused) => {
                self.set_focused(focused);
                Handled::Yes
            }
            Input::Mouse(mouse) => {
                let Some(engine) = &self.surface.engine else {
                    return Handled::Ignored;
                };
                self.pointer.handle(
                    mouse,
                    self.area,
                    self.options.cells,
                    &self.hits,
                    &self.tree,
                    &engine.control(),
                )
            }
            Input::Key { .. } | Input::Paste(_) => Handled::Ignored,
        }
    }

    /// Applies complete pending batches, resizes to `area`, and draws into the frame.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.draw(area, frame.buffer_mut());
    }

    fn size(&self) -> Size<Points> {
        Size::new(
            self.options.cells.width() * f32::from(self.area.width),
            self.options.cells.height() * f32::from(self.area.height),
        )
    }

    fn draw(&mut self, area: Rect, buffer: &mut Buffer) {
        self.surface.begin_frame();
        if self.area != area {
            self.pointer
                .cancel(self.surface.engine.as_ref().map(Engine::control).as_ref());
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
                Message::Batch(batch) => self.tree.apply(batch),
                Message::Error(error) => errors.push(error),
                Message::Frame => self.surface.request_frame(),
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
        render::draw(
            &mut self.tree,
            buffer,
            area,
            self.options.cells,
            &mut self.hits,
        );
    }

    /// Stops the engine, drops queued commits, and clears the surface's app-owned state.
    pub fn stop(&mut self) {
        self.pointer
            .cancel(self.surface.engine.as_ref().map(Engine::control).as_ref());
        self.surface.stop();
        self.tree.clear();
        self.hits.clear();
    }
}

impl Widget for &mut AppView {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        self.draw(area, buffer);
    }
}
