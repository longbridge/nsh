//! The windows of an app [`crate::run_app`] runs: one UI thread per window
//! ([`AppWindows`], the app's [`WindowHost`]).
//!
//! Each window is an [`AppView`] with an engine of its own on a thread of
//! its own: its message loop, input, views, Direct3D / Direct2D device,
//! swap chain and pacing are that thread's, so a window that stalls (its
//! engine or its drawing) keeps no other window from taking input and
//! drawing. Other threads reach a window only by posting: `WM_CLOSE` to its
//! `HWND`, closures to its dispatcher ([`UiThread::invoke`]); windows have
//! no owner or parent, whose input queues Windows would attach. Closing the
//! main window closes the others and ends the app.

//! The windows of an app ([`nsh_core::window::WindowHost`]): one UI thread
//! per window.
//!
//! A window owns a surface, which owns thread-locals (the graphics devices,
//! the window procedure's data), so a window and its message loop live on a
//! thread of their own, as [`crate::run_app`]'s main window does. What
//! crosses threads is small and `Send`: the app's source, the `Send` parts of
//! its options, the shared store, and the window table below. Engines call
//! the host from their own threads and never wait for a window's thread:
//! `open` spawns the thread and returns its id, `focus` posts a closure to the
//! window's [`UiThread`], `close` posts `WM_CLOSE` (the one call Win32 allows
//! on another thread's window).
//!
//! Native components ([`crate::AppOptions::components`], `Rc`s of one thread)
//! are not carried: a window the host opens starts with none registered, apart
//! from the ones [`crate::set_components`] registers for every view.

use std::{
    collections::HashMap,
    panic::AssertUnwindSafe,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, Weak},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use nsh_core::{
    platform::Appearance,
    shared::{SharedStore, SharedValue},
    window::{WindowContent, WindowHost, WindowId, WindowInfo, WindowOptions, Windows, window_key},
};
use nsh_engine::{App, Bundle, EngineContext, Limits, Root};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::Ole::{OleInitialize, OleUninitialize},
    UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE},
};

use crate::{
    app::{AppOptions, AppSource, AppView, install_shortcuts},
    image::AssetBase,
    window::UiThread,
};

/// How long the main window's end waits for the other windows to close.
const CLOSE_DEADLINE: Duration = Duration::from_secs(2);

/// What a window of [`WindowContent::App`] runs again: the app's source.
enum Source {
    Rust(Root),
    Bundle(Box<Bundle>),
    #[cfg(feature = "dev")]
    DevServer(String),
}

/// The `Send` parts of the app's [`AppOptions`], which every window starts
/// from.
struct Seed {
    trace: bool,
    fps: bool,
    asset_base: Option<AssetBase>,
    app_id: String,
    title: String,
    size: (i32, i32),
    appearance: Appearance,
    limits: Limits,
    data_dir: Option<PathBuf>,
    cache: nsh_engine::CacheOptions,
    storage: Option<Arc<dyn nsh_engine::StorageBackend>>,
}

/// One open window.
struct Entry {
    name: String,
    /// Its `HWND` once made (0 before), for `WM_CLOSE`.
    hwnd: isize,
    /// Its thread's dispatcher once the window exists.
    ui: Option<UiThread>,
    /// Its thread, until the main window's end joins it.
    join: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct Table {
    /// The id the next window gets (the main window is 0).
    next: u64,
    /// The app is ending: no window opens or starts any more.
    closing: bool,
    windows: HashMap<u64, Entry>,
}

/// The window host of an app run by [`crate::run_app`].
pub(crate) struct AppWindows {
    this: Weak<AppWindows>,
    source: Source,
    seed: Seed,
    store: SharedStore,
    table: Mutex<Table>,
}

impl AppWindows {
    /// The host of an app run from `source` with `options`, whose windows
    /// share `store`.
    pub(crate) fn new(
        source: &AppSource,
        options: &AppOptions,
        store: SharedStore,
    ) -> Arc<AppWindows> {
        let source = match source {
            AppSource::Rust(root) => Source::Rust(root.clone()),
            AppSource::Bundle(bundle) => Source::Bundle(Box::new(bundle.clone())),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => Source::DevServer(url.clone()),
        };
        let seed = Seed {
            trace: options.trace,
            fps: options.fps,
            asset_base: options.asset_base.clone(),
            app_id: options.app_id.clone(),
            title: options.title.clone(),
            size: options.size,
            appearance: options.appearance,
            limits: options.limits,
            data_dir: options.data_dir.clone(),
            cache: options.cache.clone(),
            storage: options.storage.clone(),
        };
        Arc::new_cyclic(|this| AppWindows {
            this: this.clone(),
            source,
            seed,
            store,
            table: Mutex::new(Table {
                next: 1,
                ..Table::default()
            }),
        })
    }

    fn table(&self) -> MutexGuard<'_, Table> {
        self.table.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Records the main window (id 0) and publishes it in the store.
    pub(crate) fn register_main(&self, view: &AppView, title: &str) {
        self.table().windows.insert(
            0,
            Entry {
                name: String::new(),
                hwnd: view.hwnd().0 as isize,
                ui: Some(UiThread::current()),
                join: None,
            },
        );
        self.publish(WindowId(0), "", title);
    }

    fn publish(&self, id: WindowId, name: &str, title: &str) {
        self.store.set(
            window_key(id),
            SharedValue::map([
                ("name", SharedValue::from(name)),
                ("title", SharedValue::from(title)),
            ]),
        );
    }

    /// The main window ended: asks every other window to close and waits for
    /// their threads, at most [`CLOSE_DEADLINE`] in all (the process exits
    /// anyway).
    pub(crate) fn shutdown(&self) {
        let handles: Vec<(u64, JoinHandle<()>)> = {
            let mut table = self.table();
            table.closing = true;
            let mut handles = Vec::new();
            for (id, entry) in table.windows.iter_mut().filter(|(id, _)| **id != 0) {
                post_close(entry.hwnd);
                if let Some(join) = entry.join.take() {
                    handles.push((*id, join));
                }
            }
            handles
        };
        let deadline = Instant::now() + CLOSE_DEADLINE;
        for (id, join) in handles {
            while !join.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if join.is_finished() {
                let _ = join.join();
            } else {
                log::warn!("window {id} did not close in time; not waiting for it");
            }
        }
    }

    /// The body of a window's thread (`nsh-window-<id>`).
    fn window_main(self: Arc<Self>, id: u64, options: WindowOptions, content: WindowContent) {
        // SAFETY: COM for the clipboard and drag and drop, once on this thread.
        // SAFETY: this thread's OLE apartment (clipboard, drag and drop,
        // WIC, WebView2), released below once its window is gone.
        let _ = unsafe { OleInitialize(None) };
        let run =
            std::panic::catch_unwind(AssertUnwindSafe(|| self.run_window(id, &options, content)));
        if run.is_err() {
            log::error!("window {id} panicked; closing it");
        }
        self.store.delete(window_key(WindowId(id)));
        self.table().windows.remove(&id);
        // SAFETY: pairs the `OleInitialize` above on this thread.
        // SAFETY: pairs the `OleInitialize` above; nothing on this thread
        // uses OLE any more.
        unsafe { OleUninitialize() };
    }

    fn run_window(self: &Arc<Self>, id: u64, options: &WindowOptions, content: WindowContent) {
        let seed = &self.seed;
        let title = options.title.clone().unwrap_or_else(|| seed.title.clone());
        let size = options.size.map_or(seed.size, |s| {
            (s.width.0.round() as i32, s.height.0.round() as i32)
        });
        let host: Arc<dyn WindowHost> = self.clone();
        let info = WindowInfo::new(
            WindowId(id),
            options.name.clone(),
            options.params.clone(),
            false,
        );
        let app_options = AppOptions {
            trace: seed.trace,
            fps: seed.fps,
            asset_base: seed.asset_base.clone(),
            app_id: seed.app_id.clone(),
            title: title.clone(),
            size,
            appearance: seed.appearance,
            limits: seed.limits,
            data_dir: seed.data_dir.clone(),
            cache: seed.cache.clone(),
            storage: seed.storage.clone(),
            context: EngineContext::default()
                .with(self.store.clone())
                .with(Windows::new(host, info)),
            ..AppOptions::default()
        };
        let view = match AppView::new(None, app_options) {
            Ok(view) => view,
            Err(err) => {
                log::error!("cannot open window {id}: {err}");
                return;
            }
        };
        let hwnd = view.hwnd();
        match options.position {
            Some(p) => crate::window::move_to(hwnd, p.x.0, p.y.0, false),
            // Cascaded from where the system puts a window, in 32 px steps.
            None => {
                let step = 32.0 * ((id.max(1) - 1) % 8 + 1) as f32;
                crate::window::move_to(hwnd, step, step, true);
            }
        }
        crate::window::set_resizable(hwnd, options.resizable);
        crate::window::set_min_size(hwnd, options.min_size.map(|s| (s.width.0, s.height.0)));
        {
            let mut table = self.table();
            // The app ended while this window was starting.
            if table.closing {
                return;
            }
            if let Some(entry) = table.windows.get_mut(&id) {
                entry.hwnd = hwnd.0 as isize;
                entry.ui = Some(UiThread::current());
            }
        }
        match content {
            WindowContent::Root(root) => view.load(App::Rust(root)),
            _ => match &self.source {
                Source::Rust(root) => view.load(App::Rust(root.clone())),
                Source::Bundle(bundle) => view.load(App::Js((**bundle).clone())),
                #[cfg(feature = "dev")]
                Source::DevServer(url) => crate::dev::start(&view, url.clone()),
            },
        }
        crate::system::environment_applied(view.surface());
        install_shortcuts(&view);
        self.publish(WindowId(id), &options.name, &title);
        if options.focus {
            view.show();
            crate::window::activate(hwnd);
        } else {
            crate::window::show_no_activate(hwnd);
        }
        crate::window::run_loop();
        self.store.delete(window_key(WindowId(id)));
        view.stop();
        drop(view);
    }
}

/// Asks the window `hwnd` (0: none yet) to close. Posting is allowed from
/// any thread.
fn post_close(hwnd: isize) {
    if hwnd != 0 {
        // SAFETY: posting to a window handle is thread-safe; a window that is
        // gone makes the call fail harmlessly.
        // SAFETY: posting needs no access to the window; a window that is
        // gone fails the post, which is ignored.
        let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    }
}

impl WindowHost for AppWindows {
    fn open(&self, options: WindowOptions, content: WindowContent) -> Result<WindowId, String> {
        let host = self.this.upgrade().ok_or("the app has ended")?;
        let id = {
            let mut table = self.table();
            if table.closing {
                return Err("the app is closing".to_owned());
            }
            let existing = options
                .single
                .then(|| {
                    table
                        .windows
                        .iter()
                        .find(|(id, entry)| **id != 0 && entry.name == options.name)
                        .map(|(id, _)| *id)
                })
                .flatten();
            if let Some(id) = existing {
                drop(table);
                self.focus(WindowId(id));
                return Ok(WindowId(id));
            }
            let id = table.next;
            table.next += 1;
            table.windows.insert(
                id,
                Entry {
                    name: options.name.clone(),
                    hwnd: 0,
                    ui: None,
                    join: None,
                },
            );
            id
        };
        let spawned = std::thread::Builder::new()
            .name(format!("nsh-window-{id}"))
            .stack_size(8 << 20)
            .spawn(move || host.window_main(id, options, content));
        match spawned {
            Ok(join) => {
                // The window may have ended already and removed its entry.
                if let Some(entry) = self.table().windows.get_mut(&id) {
                    entry.join = Some(join);
                }
                Ok(WindowId(id))
            }
            Err(err) => {
                self.table().windows.remove(&id);
                Err(format!("cannot start the window's thread: {err}"))
            }
        }
    }

    fn focus(&self, id: WindowId) {
        let target = self
            .table()
            .windows
            .get(&id.0)
            .and_then(|entry| entry.ui.map(|ui| (ui, entry.hwnd)));
        if let Some((ui, hwnd)) = target {
            // The window's own thread restores and activates it.
            ui.invoke(move || crate::window::activate(HWND(hwnd as *mut _)));
        }
    }

    fn close(&self, id: WindowId) {
        let hwnd = self.table().windows.get(&id.0).map(|entry| entry.hwnd);
        post_close(hwnd.unwrap_or(0));
    }
}
