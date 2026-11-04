//! The windows of an app [`crate::run_app`] runs ([`AppWindows`], the app's
//! [`WindowHost`]).
//!
//! Each window is an [`AppView`] with an engine of its own (and so a thread
//! of its own for JS and layout); AppKit keeps every window, view and event
//! on the main thread, so the windows share it: one window's engine that
//! stalls keeps no other window from taking input and drawing, one window's
//! main-thread work (its batches, its drawing) takes the main thread's time
//! of all of them.
//!
//! Engines call the host from their own threads and never wait for the main
//! thread: `open` takes the window's id and returns it, and the window is
//! made in a block on the main queue; `focus` and `close` are blocks there
//! too. What a block needs of the app (its source and options, the open
//! windows) lives on the main thread ([`MAIN`]); the host itself keeps only
//! what is `Send`: the store and the table of ids and names. Closing the
//! main window closes the others and ends the app.

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard, Weak},
};

use nsh_core::{
    JsonValue,
    shared::{SharedStore, SharedValue},
    window::{WindowContent, WindowHost, WindowId, WindowInfo, WindowOptions, Windows, window_key},
};
use nsh_engine::{App, Bundle, EngineContext, Root};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject,
};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSScreen, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::app::{AppOptions, AppSource, AppView};

/// What a window of [`WindowContent::App`] runs again: the app's source.
enum Source {
    Rust(Root),
    Bundle(Box<Bundle>),
    #[cfg(feature = "dev")]
    DevServer(String),
}

/// One window the host opened, on the main thread.
struct Open {
    window: Retained<NSWindow>,
    view: Rc<AppView>,
    /// The window's delegate (AppKit holds it weakly).
    _delegate: Retained<WindowDelegate>,
}

/// The app's part on the main thread: what new windows start from and the
/// windows the host opened.
struct Main {
    host: Arc<AppWindows>,
    source: Source,
    options: AppOptions,
    /// The main window, which the others cascade from.
    main_window: Retained<NSWindow>,
    open: HashMap<u64, Open>,
}

thread_local! {
    static MAIN: RefCell<Option<Main>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Table {
    /// The id the next window gets (the main window is 0).
    next: u64,
    /// The app is ending: no window opens any more.
    closing: bool,
    /// The open windows' names, the main window's (0) included.
    names: HashMap<u64, String>,
}

/// The window host of an app run by [`crate::run_app`].
pub(crate) struct AppWindows {
    this: Weak<AppWindows>,
    store: SharedStore,
    table: Mutex<Table>,
}

impl AppWindows {
    /// The host of an app whose windows share `store`.
    pub(crate) fn new(store: SharedStore) -> Arc<AppWindows> {
        Arc::new_cyclic(|this| AppWindows {
            this: this.clone(),
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

    /// Records the main window (id 0), what the others start from (the
    /// app's `source` and `options`), and publishes it in the store. On
    /// the main thread.
    pub(crate) fn register_main(
        self: &Arc<Self>,
        source: &AppSource,
        options: &AppOptions,
        window: &NSWindow,
    ) {
        let source = match source {
            AppSource::Rust(root) => Source::Rust(root.clone()),
            AppSource::Bundle(bundle) => Source::Bundle(Box::new(bundle.clone())),
            #[cfg(feature = "dev")]
            AppSource::DevServer(url) => Source::DevServer(url.clone()),
        };
        self.table().names.insert(0, String::new());
        MAIN.with(|main| {
            *main.borrow_mut() = Some(Main {
                host: self.clone(),
                source,
                options: options.clone(),
                main_window: window.retain(),
                open: HashMap::new(),
            })
        });
        self.publish(WindowId(0), "", &options.title);
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

    /// Forgets window `id` (it closed).
    fn forget(&self, id: u64) {
        self.store.delete(window_key(WindowId(id)));
        self.table().names.remove(&id);
    }
}

/// The main window ended: closes every other window and stops its engine.
/// On the main thread; nothing when no app runs.
pub(crate) fn shutdown() {
    let Some(main) = MAIN.with(|main| main.borrow_mut().take()) else {
        return;
    };
    main.host.table().closing = true;
    for (id, open) in main.open {
        // Closed without its delegate, which would look for the window in
        // `MAIN`, gone already.
        open.window.setDelegate(None);
        open.window.close();
        open.view.stop();
        main.host.forget(id);
    }
    main.host.forget(0);
}

/// Makes window `id` on the main thread.
fn make_window(mtm: MainThreadMarker, id: u64, options: WindowOptions, content: WindowContent) {
    let Some((host, base, main_frame)) = MAIN.with(|main| {
        main.borrow()
            .as_ref()
            .map(|m| (m.host.clone(), m.options.clone(), m.main_window.frame()))
    }) else {
        return;
    };
    if host.table().closing {
        return;
    }
    let title = options.title.clone().unwrap_or_else(|| base.title.clone());
    let size = options
        .size
        .map_or(NSSize::new(base.size.0 as f64, base.size.1 as f64), |s| {
            NSSize::new(f64::from(s.width.0), f64::from(s.height.0))
        });
    let window_host: Arc<dyn WindowHost> = host.clone();
    let info = WindowInfo::new(
        WindowId(id),
        options.name.clone(),
        options.params.clone(),
        false,
    );
    let app_options = AppOptions {
        title: title.clone(),
        size: (size.width.round() as i32, size.height.round() as i32),
        display: None,
        keep_size: false,
        keep_alive_on_close: false,
        // Each window's apps get a channel of their own, as on Windows.
        host: nsh_engine::HostChannel::default(),
        context: EngineContext::default()
            .with(host.store.clone())
            .with(Windows::new(window_host, info)),
        ..base
    };
    let mut style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    if options.resizable {
        style |= NSWindowStyleMask::Resizable;
    }
    // SAFETY: NSWindow's designated initializer.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), size),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is kept in `MAIN` until it closes.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(&title));
    let delegate: Retained<WindowDelegate> = {
        let d = WindowDelegate::alloc(mtm).set_ivars(id);
        // SAFETY: NSObject's initializer.
        unsafe { msg_send![super(d), init] }
    };
    window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    crate::system::open_window(&window, app_options.appearance);
    if let Some(min) = options.min_size {
        window.setContentMinSize(NSSize::new(f64::from(min.width.0), f64::from(min.height.0)));
    }
    let top_left = match options.position {
        // NSH's origin is the top left, down from the primary screen's top;
        // Cocoa's goes up from its bottom.
        Some(p) => {
            let primary = NSScreen::screens(mtm)
                .firstObject()
                .map_or(0.0, |s| s.frame().size.height);
            NSPoint::new(f64::from(p.x.0), primary - f64::from(p.y.0))
        }
        // Cascaded from the main window's top left, in 24 pt steps.
        None => {
            let step = 24.0 * ((id.max(1) - 1) % 8 + 1) as f64;
            NSPoint::new(
                main_frame.origin.x + step,
                main_frame.origin.y + main_frame.size.height - step,
            )
        }
    };
    window.setFrameTopLeftPoint(top_left);
    let view = AppView::new(mtm, app_options);
    window.setContentView(Some(&view.view()));
    let source_app = match content {
        WindowContent::Root(root) => Some(App::Rust(root)),
        _ => MAIN.with(|main| {
            main.borrow().as_ref().and_then(|m| match &m.source {
                Source::Rust(root) => Some(App::Rust(root.clone())),
                Source::Bundle(bundle) => Some(App::Js((**bundle).clone())),
                #[cfg(feature = "dev")]
                Source::DevServer(_) => None,
            })
        }),
    };
    match source_app {
        Some(app) => view.load(app),
        #[cfg(feature = "dev")]
        None => {
            let url = MAIN.with(|main| {
                main.borrow().as_ref().and_then(|m| match &m.source {
                    Source::DevServer(url) => Some(url.clone()),
                    _ => None,
                })
            });
            if let Some(url) = url {
                crate::dev::start(&view, url);
            }
        }
        #[cfg(not(feature = "dev"))]
        None => {}
    }
    host.publish(WindowId(id), &options.name, &title);
    if options.focus {
        window.makeKeyAndOrderFront(None);
        window.makeFirstResponder(Some(view.surface().root.ns()));
        NSApplication::sharedApplication(mtm).activate();
    } else {
        window.orderFront(None);
    }
    MAIN.with(|main| {
        if let Some(main) = main.borrow_mut().as_mut() {
            main.open.insert(
                id,
                Open {
                    window,
                    view,
                    _delegate: delegate,
                },
            );
        }
    });
}

/// Brings window `id` (0: the main window) to the front, on the main thread.
fn focus_window(mtm: MainThreadMarker, id: u64) {
    let window = MAIN.with(|main| {
        main.borrow().as_ref().and_then(|m| match id {
            0 => Some(m.main_window.clone()),
            _ => m.open.get(&id).map(|open| open.window.clone()),
        })
    });
    if let Some(window) = window {
        if window.isMiniaturized() {
            window.deminiaturize(None);
        }
        window.makeKeyAndOrderFront(None);
        NSApplication::sharedApplication(mtm).activate();
    }
}

define_class!(
    /// A window the host opened: forgets it when it closes.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = u64]
    #[name = "NshAppWindowDelegate"]
    struct WindowDelegate;

    unsafe impl NSObjectProtocol for WindowDelegate {}

    unsafe impl NSWindowDelegate for WindowDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            let id = *self.ivars();
            // Leave AppKit's close callback (and this delegate) before the
            // engine stops and the window and view go (pitfalls: "closing a
            // window calls its delegate synchronously").
            dispatch2::DispatchQueue::main().exec_async(move || {
                let closed = MAIN.with(|main| {
                    main.borrow_mut()
                        .as_mut()
                        .and_then(|m| m.open.remove(&id).map(|open| (m.host.clone(), open)))
                });
                if let Some((host, open)) = closed {
                    open.view.stop();
                    host.forget(id);
                }
            });
        }
    }
);

impl WindowHost for AppWindows {
    fn open(&self, options: WindowOptions, content: WindowContent) -> Result<WindowId, String> {
        if self.this.upgrade().is_none() {
            return Err("the app has ended".to_owned());
        }
        let id = {
            let mut table = self.table();
            if table.closing {
                return Err("the app is closing".to_owned());
            }
            let existing = options
                .single
                .then(|| {
                    table
                        .names
                        .iter()
                        .find(|(id, name)| **id != 0 && **name == options.name)
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
            table.names.insert(id, options.name.clone());
            id
        };
        dispatch2::DispatchQueue::main().exec_async(move || {
            if let Some(mtm) = MainThreadMarker::new() {
                make_window(mtm, id, options, content);
            }
        });
        Ok(WindowId(id))
    }

    fn focus(&self, id: WindowId) {
        dispatch2::DispatchQueue::main().exec_async(move || {
            if let Some(mtm) = MainThreadMarker::new() {
                focus_window(mtm, id.0);
            }
        });
    }

    fn close(&self, id: WindowId) {
        if id.0 == 0 {
            // The main window: the app ends.
            crate::app::quit_app();
            return;
        }
        dispatch2::DispatchQueue::main().exec_async(move || {
            let window = MAIN.with(|main| {
                main.borrow()
                    .as_ref()
                    .and_then(|m| m.open.get(&id.0).map(|open| open.window.clone()))
            });
            if let Some(window) = window {
                window.close();
            }
        });
    }
}

/// The main window's [`WindowInfo`].
pub(crate) fn main_info() -> WindowInfo {
    WindowInfo::new(WindowId(0), "", JsonValue::Null, true)
}
