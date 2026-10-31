//! Panel windows: a second window of the app, beside [`run_app`]'s.
//!
//! A panel is a window of its own, as a floating palette or a popover is:
//! it shows an [`AppSource`] of its own (a Rust root, a bundle or a dev
//! server), so it can run a different app than the main window, with an
//! engine of its own ([`AppView`]). It has no title bar by default, can
//! float above other apps' windows ([`PanelLevel`]) and can take the
//! keyboard without activating the app ([`PanelOptions::nonactivating`]).
//! A panel lives exactly as long as its [`Panel`] does: dropping it closes
//! the window and stops its app.
//!
//! A panel runs on a thread of its own that pumps its messages, so
//! [`Panel::open`] may be called from any thread, the host's main thread
//! included even after [`run_app`] moved the main window to a thread of its
//! own and blocked in `join`. The panel's methods hand their work to that
//! thread and wait for it, and [`Drop`] stops the thread and joins it: its
//! thread-locals (the graphics devices, font faces and the other device
//! resources cached per thread) are released at a plain thread exit, not
//! by the main thread's destructors inside `ExitProcess` (see
//! [`run_app`]'s note on thread-locals). Rust components run on the engine
//! thread, so the host's main thread never runs one, and neither does the
//! panel's UI thread.
//!
//! Win32 has no panel concept, so the window is made to look like one:
//! a borderless window is a `WS_POPUP` window without a caption, `show`
//! shows it without activating the app (and `hide` hands the focus back to
//! the window that was in front), and [`PanelLevel::Floating`] and
//! [`PanelLevel::AboveDock`] are both `HWND_TOPMOST` (Windows has no
//! desktop-icon level to float above instead). [`PanelOptions::position`]
//! places the window's outer frame in screen points at its own scale, and
//! [`PanelOptions::hides_on_resign_key`] hides it from the window procedure,
//! as it loses the keyboard (`WM_KILLFOCUS`): a nonactivating panel never
//! gets an activation message.
//!
//! [`PanelOptions::transparent`], [`PanelOptions::hides_on_deactivate`] and
//! [`PanelOptions::movable_by_background`] have no Win32 equivalent here
//! and do nothing: transparency would need a layered window the Direct2D
//! swap chain does not draw through, hiding on deactivation would need the
//! app's activation, which a panel does not change, and moving by
//! background would need its hit tests.

use std::{
    ffi::c_void,
    rc::Rc,
    sync::mpsc,
    thread::{self, JoinHandle},
};

pub use nsh_core::desktop::{PanelError, PanelLevel, PanelOptions};
use nsh_core::{
    desktop::WindowFrame,
    geometry::{Points, Size},
};
use nsh_engine::App;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT, WPARAM},
        System::{
            Ole::{OleInitialize, OleUninitialize},
            Threading::GetCurrentThreadId,
        },
        UI::{
            HiDpi::{
                AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                GetDpiForSystem, SetProcessDpiAwarenessContext,
            },
            WindowsAndMessaging::{
                DestroyWindow, DispatchMessageW, GWL_EXSTYLE, GWL_STYLE, GetForegroundWindow,
                GetMessageW, GetWindowLongPtrW, GetWindowRect, HWND_NOTOPMOST, HWND_TOPMOST,
                IsWindow, IsWindowVisible, MSG, PostThreadMessageW, SPI_GETWORKAREA, SW_HIDE,
                SW_SHOWNA, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetForegroundWindow, SetWindowLongPtrW,
                SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW, TranslateMessage,
                WM_APP, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CAPTION, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
                WS_THICKFRAME,
            },
        },
    },
    core::HSTRING,
};

use crate::{AppOptions, AppSource, AppView};

/// A command reached the panel's thread (see [`Panel::wake`]).
const WM_NSH_PANEL: u32 = WM_APP + 5;

/// A panel window (see the [module](self)'s documentation): its own window
/// and app, closed and stopped when it is dropped. A panel runs on a thread
/// of its own with its own message loop; its methods may be called from any
/// thread and hand their work to that thread (waiting until it is done).
///
/// The `app`'s [`AppOptions`] are the app's own, not the window's: its
/// `data_dir`, `limits` and `components` come from the host, as they do for
/// the [main window](run_app)'s app, and every app keeps its data in
/// `<data_dir>/<app id>` ([`AppOptions::data_dir`]), so apps of different
/// ids stay apart.
///
/// The window's title is [`PanelOptions::title`]: the `app`'s
/// [`AppOptions::title`] names the [main window](run_app), which a panel
/// does not have.
///
/// The `app`'s calls and posts reach the host on the engine thread
/// ([`AppOptions::host`]): a host that shows, hides or closes a panel from
/// one of them may do so directly, as the panel's methods are safe from any
/// thread.
pub struct Panel {
    /// The channel to the thread running the window and its message loop.
    commands: mpsc::Sender<Command>,
    /// That thread, while it runs.
    thread: Option<JoinHandle<()>>,
    /// That thread's id, for waking it when a command arrives.
    ui: u32,
}

impl Panel {
    /// Opens a panel window showing `source` with `options`, without
    /// showing it: call [`show`](Self::show) to show it.
    ///
    /// # Errors
    ///
    /// [`PanelError::Invalid`] when the panel's size is not positive, or
    /// when Win32 does not create the window (it is logged).
    pub fn open(
        options: PanelOptions,
        app: AppOptions,
        source: AppSource,
    ) -> Result<Self, PanelError> {
        let (commands, received) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let job = Job {
            options,
            app,
            source,
            commands: received,
            ready,
        };
        let thread = thread::Builder::new()
            .name("nsh-panel".into())
            // Deep view trees recurse in drawing and hit testing.
            .stack_size(8 << 20)
            .spawn(move || run(job))
            .map_err(|error| {
                log::error!("the panel's thread could not be started: {error}");
                PanelError::Invalid("the panel's thread could not be started")
            })?;
        let ui = match started.recv() {
            Ok(Ok(ui)) => ui,
            Ok(Err(error)) => {
                let _ = thread.join();
                return Err(error);
            }
            // The thread ended before it could say whether it started.
            Err(_) => {
                let _ = thread.join();
                return Err(PanelError::Invalid(
                    "the panel's window could not be created",
                ));
            }
        };
        Ok(Self {
            commands,
            thread: Some(thread),
            ui,
        })
    }

    /// Shows the window above the other windows of its level, without
    /// taking the focus or activating the app: a click raises it and gives
    /// it the keyboard, as a panel's click does.
    pub fn show(&self) {
        self.ask(Command::Show);
    }

    /// Hides the window and gives the focus back to the window that was in
    /// front before it showed.
    pub fn hide(&self) {
        self.ask(Command::Hide);
    }

    /// Whether the window is on screen.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        let (reply, done) = mpsc::channel();
        match self.commands.send(Command::IsVisible(reply)) {
            Ok(()) => {
                self.wake();
                done.recv().unwrap_or(false)
            }
            // The thread ended: the window is gone too.
            Err(_) => false,
        }
    }

    /// Moves and sizes the window: a field left out keeps its value. The
    /// points are pixels at the window's scale; Win32 works on the outer
    /// frame (non-client area included).
    pub fn set_frame(&self, frame: WindowFrame) {
        self.ask(|reply| Command::SetFrame(frame, reply));
    }

    /// Sets the level the window floats at: `HWND_TOPMOST` for
    /// [`PanelLevel::Floating`] and [`PanelLevel::AboveDock`] (Windows has
    /// no desktop-icon level, so both are topmost), `HWND_NOTOPMOST` for
    /// [`PanelLevel::Normal`].
    pub fn set_level(&self, level: PanelLevel) {
        self.ask(|reply| Command::SetLevel(level, reply));
    }

    /// Sets the window's title.
    pub fn set_title(&self, title: &str) {
        let title = title.to_owned();
        self.ask(|reply| Command::SetTitle(title, reply));
    }

    /// Closes the window, stops its app and gives the focus back: [`Drop`]
    /// then has nothing left to do.
    pub fn close(self) {
        let mut panel = self;
        panel.shut();
    }

    /// Sends a command that takes no answer and waits for it to be done.
    fn ask(&self, command: impl FnOnce(mpsc::Sender<()>) -> Command) {
        let (reply, done) = mpsc::channel();
        if self.commands.send(command(reply)).is_ok() {
            self.wake();
            let _ = done.recv();
        }
    }

    /// Wakes the panel's thread to take its commands.
    fn wake(&self) {
        // SAFETY: a plain message to a live thread; the thread ignores a
        // failed post (it ends after its window closes).
        unsafe {
            let _ = PostThreadMessageW(self.ui, WM_NSH_PANEL, WPARAM(0), LPARAM(0));
        }
    }

    /// Closes the panel's window and stops its thread.
    fn shut(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        let (reply, done) = mpsc::channel();
        if self.commands.send(Command::Close(reply)).is_ok() {
            self.wake();
            let _ = done.recv();
        }
        let _ = thread.join();
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        self.shut();
    }
}

/// A command to the panel's thread.
enum Command {
    /// Show the window; the answer comes once it is shown.
    Show(mpsc::Sender<()>),
    /// Hide the window; the answer comes once it is hidden.
    Hide(mpsc::Sender<()>),
    /// Move or size the window.
    SetFrame(WindowFrame, mpsc::Sender<()>),
    /// Change the level the window floats at.
    SetLevel(PanelLevel, mpsc::Sender<()>),
    /// Set the window's title.
    SetTitle(String, mpsc::Sender<()>),
    /// Answer whether the window is on screen.
    IsVisible(mpsc::Sender<bool>),
    /// Close the window (and end the panel's thread).
    Close(mpsc::Sender<()>),
}

/// What the panel's thread starts with: the app, and the way back to the
/// caller.
struct Job {
    options: PanelOptions,
    app: AppOptions,
    source: AppSource,
    commands: mpsc::Receiver<Command>,
    ready: mpsc::Sender<Result<u32, PanelError>>,
}

// SAFETY: the options hold `Rc`s (the native components), so they are not
// `Send`. They move to the panel's thread whole while the caller waits in
// [`Panel::open`] and never touches them; the caller holds no clone it
// could use before that thread ends.
unsafe impl Send for Job {}

/// The panel's thread: its window, its app and its message loop, until the
/// window closes.
fn run(job: Job) {
    let Job {
        options,
        app,
        source,
        commands,
        ready,
    } = job;
    // SAFETY: process setup on this thread, before any window: per-monitor
    // DPI and an OLE apartment for the app's clipboard, drag and drop and
    // WIC use.
    let ole = unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        OleInitialize(None).is_ok()
    };
    let mut worker = match start(options, app, source) {
        Ok(worker) => worker,
        Err(error) => {
            let _ = ready.send(Err(error));
            if ole {
                // SAFETY: pairs the `OleInitialize` above.
                unsafe { OleUninitialize() };
            }
            return;
        }
    };
    // SAFETY: a plain query.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
    let mut msg = MSG::default();
    loop {
        // SAFETY: the standard message loop over this thread's messages.
        let more = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if !more.as_bool() {
            break;
        }
        if msg.hwnd.is_invalid() && msg.message == WM_NSH_PANEL {
            for command in commands.try_iter() {
                worker.handle(command);
            }
            continue;
        }
        // SAFETY: the message goes to the window (of this thread) that it
        // was sent to.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    worker.stop();
    drop(worker);
    if ole {
        // SAFETY: pairs the `OleInitialize` above; nothing on this thread
        // uses OLE afterwards.
        unsafe { OleUninitialize() };
    }
}

/// Makes the panel's window and loads the app into it: the window stays on
/// this thread, whose message loop the caller of [`run`] then runs.
fn start(options: PanelOptions, app: AppOptions, source: AppSource) -> Result<Worker, PanelError> {
    let size = panel_size(options.size)?;
    let title = options.title.clone();
    let view = AppView::new(None, AppOptions { size, title, ..app }).map_err(|error| {
        log::error!("the panel's window could not be created: {error}");
        PanelError::Invalid("the panel's window could not be created")
    })?;
    apply(&view, &options, size);
    if let Some((x, y)) = options.position {
        place(&view, x, y);
    }
    match source {
        AppSource::Rust(root) => view.load(App::Rust(root)),
        AppSource::Bundle(bundle) => view.load(App::Js(bundle)),
        #[cfg(feature = "dev")]
        AppSource::DevServer(url) => crate::dev::start(&view, url),
    }
    crate::system::environment_applied(view.surface());
    Ok(Worker {
        view,
        level: options.level,
        previous: None,
        stopped: false,
    })
}

/// The window and the state its commands work on, on the panel's thread.
struct Worker {
    view: Rc<AppView>,
    /// The level the window has, to re-assert it when it shows again.
    level: PanelLevel,
    /// The window in front before the panel showed, to give it the focus
    /// back.
    previous: Option<HWND>,
    /// Whether the app is stopped, so it stops once.
    stopped: bool,
}

impl Worker {
    /// Runs a command from the host.
    fn handle(&mut self, command: Command) {
        match command {
            Command::Show(reply) => {
                self.show();
                let _ = reply.send(());
            }
            Command::Hide(reply) => {
                self.hide();
                let _ = reply.send(());
            }
            Command::SetFrame(frame, reply) => {
                self.set_frame(frame);
                let _ = reply.send(());
            }
            Command::SetLevel(level, reply) => {
                self.set_level(level);
                let _ = reply.send(());
            }
            Command::SetTitle(title, reply) => {
                self.set_title(&title);
                let _ = reply.send(());
            }
            Command::IsVisible(reply) => {
                let _ = reply.send(self.is_visible());
            }
            Command::Close(reply) => {
                self.close();
                let _ = reply.send(());
            }
        }
    }

    /// Shows the window above the other windows of its level, without
    /// taking the focus or activating the app.
    fn show(&mut self) {
        let hwnd = self.view.hwnd();
        if hwnd.is_invalid() {
            return;
        }
        // Remember what was in front, so `hide` gives it the focus back.
        // SAFETY: a plain query.
        let foreground = unsafe { GetForegroundWindow() };
        if !foreground.is_invalid() && foreground != hwnd {
            self.previous = Some(foreground);
        }
        // SAFETY: plain calls on the panel's live window; `SW_SHOWNA`
        // shows it without activating it, and the level is re-asserted
        // without moving or resizing it.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
            let _ = SetWindowPos(
                hwnd,
                Some(order_of(self.level)),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }

    /// Hides the window and gives the focus back.
    fn hide(&mut self) {
        let hwnd = self.view.hwnd();
        // SAFETY: a plain query on a window that may have closed.
        if !hwnd.is_invalid() && unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            // SAFETY: a plain call on a live window.
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
        if let Some(previous) = self.previous.take()
            && !previous.is_invalid()
            // SAFETY: a plain query on a window that may have closed.
            && unsafe { IsWindow(Some(previous)) }.as_bool()
        {
            // SAFETY: a plain call on the window that was in front.
            let _ = unsafe { SetForegroundWindow(previous) };
        }
    }

    /// Whether the window is on screen.
    fn is_visible(&self) -> bool {
        let hwnd = self.view.hwnd();
        if hwnd.is_invalid() {
            return false;
        }
        // SAFETY: a plain query.
        unsafe { IsWindowVisible(hwnd) }.as_bool()
    }

    /// Moves and sizes the window: a field left out keeps its value.
    fn set_frame(&self, frame: WindowFrame) {
        let hwnd = self.view.hwnd();
        if hwnd.is_invalid() {
            return;
        }
        let mut rect = RECT::default();
        // SAFETY: a plain query with a local out value.
        if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
            return;
        }
        let scale = self.view.surface().scale();
        let x = frame.x.map_or(rect.left, |v| (v.0 * scale).round() as i32);
        let y = frame.y.map_or(rect.top, |v| (v.0 * scale).round() as i32);
        let width = frame
            .width
            .map_or(rect.right - rect.left, |v| (v.0 * scale).round() as i32);
        let height = frame
            .height
            .map_or(rect.bottom - rect.top, |v| (v.0 * scale).round() as i32);
        // SAFETY: a plain call on a live window; `SWP_NOZORDER` keeps the
        // level the panel has.
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
    }

    /// Sets the level the window floats at.
    fn set_level(&mut self, level: PanelLevel) {
        self.level = level;
        let hwnd = self.view.hwnd();
        if hwnd.is_invalid() {
            return;
        }
        // SAFETY: plain call on a live window; no other field changes, so
        // its position and size stay.
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(order_of(level)),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            )
        };
    }

    /// Sets the window's title.
    fn set_title(&self, title: &str) {
        let hwnd = self.view.hwnd();
        if hwnd.is_invalid() {
            return;
        }
        // SAFETY: a plain call on a live window with a temporary string.
        let _ = unsafe { SetWindowTextW(hwnd, &HSTRING::from(title)) };
    }

    /// Closes the window (which ends the message loop) and stops the app.
    fn close(&mut self) {
        self.hide();
        self.stop();
        let hwnd = self.view.hwnd();
        if !hwnd.is_invalid() {
            // SAFETY: the panel's own window, destroyed once (the checks
            // around it skip a window that is gone).
            let _ = unsafe { DestroyWindow(hwnd) };
        }
    }

    /// Stops the app, once.
    fn stop(&mut self) {
        if !self.stopped {
            self.stopped = true;
            self.view.stop();
        }
    }
}

/// Makes the panel's window out of the top-level window
/// [`AppView::new`] created: its style, its level and the tool-window bit
/// that keeps it out of the taskbar's window list.
fn apply(view: &AppView, options: &PanelOptions, size: (i32, i32)) {
    let hwnd = view.hwnd();
    // SAFETY: plain calls on the window just made, before it shows.
    unsafe {
        let mut style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        if options.borderless {
            // A panel has no caption, no resize frame and no system menu,
            // as a borderless window has none.
            style &= !(WS_CAPTION.0
                | WS_THICKFRAME.0
                | WS_SYSMENU.0
                | WS_MINIMIZEBOX.0
                | WS_MAXIMIZEBOX.0);
            style |= WS_POPUP.0;
            SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        }
        let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        // The panel is a tool window, not an app window.
        ex_style |= WS_EX_TOOLWINDOW.0;
        if options.nonactivating {
            // Clicks do not activate the app; the panel takes the keyboard
            // only after the user aims at it.
            ex_style |= WS_EX_NOACTIVATE.0;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);
        // The window procedure hides the panel when it loses the keyboard,
        // when the options ask for it (no activation message reaches a
        // panel's window otherwise).
        crate::window::set_hides_on_resign_key(hwnd, options.hides_on_resign_key);
        // A window's size is its outer frame, a panel's is its content: a
        // borderless popup has the whole window as its client area, so it
        // is sized to the points the panel asked for, once the frame is
        // gone. A decorated panel was sized by `AppView::new` already.
        let (width, height) = if options.borderless {
            let dpi = crate::window::dpi(hwnd);
            let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: (size.0 as f32 * scale).round() as i32,
                bottom: (size.1 as f32 * scale).round() as i32,
            };
            let _ = AdjustWindowRectExForDpi(
                &mut rect,
                WINDOW_STYLE(style),
                false,
                WINDOW_EX_STYLE(ex_style),
                dpi,
            );
            (rect.right - rect.left, rect.bottom - rect.top)
        } else {
            (0, 0)
        };
        // The frame changed: ask Win32 to remake it, without moving or
        // activating the window.
        let mut flags = SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE;
        if !options.borderless {
            flags |= SWP_NOSIZE;
        }
        let _ = SetWindowPos(
            hwnd,
            Some(order_of(options.level)),
            0,
            0,
            width,
            height,
            flags,
        );
    }
}

/// Moves the window: `position` is the screen coordinates, in points at the
/// window's scale, of its top-left corner (Win32's outer frame).
fn place(view: &AppView, x: Points, y: Points) {
    let hwnd = view.hwnd();
    if hwnd.is_invalid() {
        return;
    }
    let scale = view.surface().scale();
    let (x, y) = ((x.0 * scale).round() as i32, (y.0 * scale).round() as i32);
    // SAFETY: a plain call on a live window; its size and level stay.
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSIZE,
        )
    };
}

/// The z-order a level is: the closest Win32 has to a panel's level.
fn order_of(level: PanelLevel) -> HWND {
    match level {
        PanelLevel::Normal => HWND_NOTOPMOST,
        PanelLevel::Floating | PanelLevel::AboveDock => HWND_TOPMOST,
    }
}

/// The window's size in points: the options', or 70% of the work area (the
/// screen without the taskbar), as the macOS backend's default.
fn panel_size(size: Option<Size<Points>>) -> Result<(i32, i32), PanelError> {
    if let Some(size) = size {
        let (width, height) = (size.width.0, size.height.0);
        if width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite() {
            return Ok((width.round() as i32, height.round() as i32));
        }
        return Err(PanelError::Invalid("a panel's size must be positive"));
    }
    let mut rect = RECT::default();
    // SAFETY: a plain query with a local out value.
    let area = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&raw mut rect as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if area.is_err() || rect.right <= rect.left || rect.bottom <= rect.top {
        return Ok((800, 600));
    }
    // SAFETY: a plain query.
    let dpi = unsafe { GetDpiForSystem() };
    let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
    let width = (rect.right - rect.left) as f32 / scale;
    let height = (rect.bottom - rect.top) as f32 / scale;
    Ok(((width * 0.7).round() as i32, (height * 0.7).round() as i32))
}
