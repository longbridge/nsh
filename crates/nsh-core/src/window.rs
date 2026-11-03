//! The windows of an app: opening one, and knowing which one a surface is.
//!
//! A window is a surface of its own, with an engine of its own, as each
//! `AppView` is: opening one starts a new instance of the app (its entry, or
//! a Rust [`Root`]) in a new top-level window. What the windows share is the
//! app's [`SharedStore`](crate::shared::SharedStore).
//!
//! Who makes the surfaces makes the windows: a platform's app runner, or an
//! embedding app that shows surfaces its own way, implements [`WindowHost`]
//! and gives each engine a [`Windows`] in its context (the host, and which
//! window the engine's surface is). An engine without one cannot open
//! windows ([`open`] says so). The host is called on the engine's thread and
//! must not wait for a window's thread (it starts one and returns);
//! focusing and closing post to the window's thread.
//!
//! The host keeps the open windows in the shared store under
//! [`WINDOWS_PREFIX`] (`nsh/windows/<id>`: `{ name, title }`), which an app
//! follows to know when one closes (`shared::prefix(WINDOWS_PREFIX)`).

use std::sync::Arc;

use crate::{
    JsonValue,
    element::{Root, engine_context},
    geometry::{Point, Points, Size},
};

/// Where the window host keeps the open windows in the shared store: one
/// key per window, `nsh/windows/<id>`, whose value is `{ name, title }`.
pub const WINDOWS_PREFIX: &str = "nsh/windows/";

/// A window of the app, unique for the app's run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct WindowId(pub u64);

/// The key of window `id` in the shared store ([`WINDOWS_PREFIX`]).
pub fn window_key(id: WindowId) -> String {
    format!("{WINDOWS_PREFIX}{}", id.0)
}

/// How a window opens ([`open`]).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct WindowOptions {
    /// The app's name for the window: its entry reads it
    /// ([`current`]) to choose what to show, and [`single`](Self::single)
    /// finds the window by it.
    pub name: String,
    /// What the window's instance of the app is given ([`WindowInfo::params`]).
    pub params: JsonValue,
    /// The title; the app's name without one.
    pub title: Option<String>,
    /// The size of its content; the platform's default without one.
    pub size: Option<Size<Points>>,
    /// Where its top-left corner is on the screen; cascaded from the
    /// window that opens it without one.
    pub position: Option<Point<Points>>,
    /// The smallest size of its content the user can make it.
    pub min_size: Option<Size<Points>>,
    /// Whether the user can resize it.
    pub resizable: bool,
    /// Whether it becomes the active window when it opens.
    pub focus: bool,
    /// When a window of the same name is open: that one is focused and
    /// returned instead of opening another.
    pub single: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            name: String::new(),
            params: JsonValue::Null,
            title: None,
            size: None,
            position: None,
            min_size: None,
            resizable: true,
            focus: true,
            single: false,
        }
    }
}

/// What a window shows.
#[derive(Clone, Default)]
#[non_exhaustive]
pub enum WindowContent {
    /// The app itself, started again in the window: its entry chooses what
    /// to show by [`current`] (a TSX app's bundle, a Rust app's root).
    #[default]
    App,
    /// A Rust root.
    Root(Root),
}

impl std::fmt::Debug for WindowContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WindowContent::App => f.write_str("App"),
            WindowContent::Root(_) => f.write_str("Root"),
        }
    }
}

/// Which window a surface is.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct WindowInfo {
    /// Its id.
    pub id: WindowId,
    /// The name it was opened with ([`WindowOptions::name`]; empty for the
    /// main window).
    pub name: String,
    /// What it was opened with ([`WindowOptions::params`]).
    pub params: JsonValue,
    /// Whether it is the app's main window: the app ends when it closes.
    pub main: bool,
}

impl WindowInfo {
    /// Window `id` named `name`, opened with `params`; `main` when it is the
    /// app's main window.
    pub fn new(id: WindowId, name: impl Into<String>, params: JsonValue, main: bool) -> Self {
        Self {
            id,
            name: name.into(),
            params,
            main,
        }
    }
}

/// Opens, focuses and closes the windows of an app (its maker's: the
/// module docs). Called on engines' threads: it never waits for a window's
/// thread.
pub trait WindowHost: Send + Sync {
    /// Opens a window shown by `content` as `options` say, and returns its
    /// id (or, with [`WindowOptions::single`], the open one's); why not
    /// when it cannot.
    fn open(&self, options: WindowOptions, content: WindowContent) -> Result<WindowId, String>;
    /// Makes window `id` the active window (nothing when it is closed).
    fn focus(&self, id: WindowId);
    /// Closes window `id` (nothing when it is closed). Closing the main
    /// window ends the app.
    fn close(&self, id: WindowId);
}

/// What an engine's context holds of its app's windows: their host, and the
/// window the engine's surface is.
#[derive(Clone)]
pub struct Windows {
    host: Arc<dyn WindowHost>,
    current: WindowInfo,
}

impl Windows {
    /// `host`'s windows, as seen from window `current`.
    pub fn new(host: Arc<dyn WindowHost>, current: WindowInfo) -> Self {
        Self { host, current }
    }

    /// The window host.
    pub fn host(&self) -> &Arc<dyn WindowHost> {
        &self.host
    }

    /// The window the engine's surface is.
    pub fn current(&self) -> &WindowInfo {
        &self.current
    }
}

impl std::fmt::Debug for Windows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Windows")
            .field("current", &self.current)
            .finish_non_exhaustive()
    }
}

/// Why windows cannot open here.
const NO_HOST: &str = "this surface cannot open windows: its host has no window host";

fn windows() -> Option<Arc<Windows>> {
    engine_context::<Windows>()
}

/// The window this engine's surface is, when its host has windows (on the
/// engine's thread).
pub fn current() -> Option<WindowInfo> {
    windows().map(|w| w.current.clone())
}

/// Opens a window shown by `content` (on the engine's thread); its id, or
/// why it did not open.
pub fn open(options: WindowOptions, content: WindowContent) -> Result<WindowId, String> {
    match windows() {
        Some(w) => w.host.open(options, content),
        None => Err(NO_HOST.to_owned()),
    }
}

/// Makes window `id` the active window (on the engine's thread).
pub fn focus(id: WindowId) {
    if let Some(w) = windows() {
        w.host.focus(id);
    }
}

/// Closes window `id` (on the engine's thread); closing the main window
/// ends the app.
pub fn close(id: WindowId) {
    if let Some(w) = windows() {
        w.host.close(id);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::element::EngineContext;

    #[derive(Default)]
    struct Fake {
        opened: Mutex<Vec<String>>,
        closed: Mutex<Vec<WindowId>>,
    }

    impl WindowHost for Fake {
        fn open(
            &self,
            options: WindowOptions,
            _content: WindowContent,
        ) -> Result<WindowId, String> {
            let mut opened = self.opened.lock().unwrap();
            opened.push(options.name);
            Ok(WindowId(opened.len() as u64))
        }

        fn focus(&self, _id: WindowId) {}

        fn close(&self, id: WindowId) {
            self.closed.lock().unwrap().push(id);
        }
    }

    #[test]
    fn an_engine_opens_windows_through_its_host() {
        assert!(open(WindowOptions::default(), WindowContent::App).is_err());
        assert!(current().is_none());

        let host = Arc::new(Fake::default());
        let main = WindowInfo::new(WindowId(0), "", JsonValue::Null, true);
        let context = EngineContext::default().with(Windows::new(host.clone(), main.clone()));
        let _entered = context.enter();
        assert_eq!(current(), Some(main));
        let options = WindowOptions {
            name: "settings".into(),
            ..Default::default()
        };
        assert_eq!(open(options, WindowContent::App), Ok(WindowId(1)));
        close(WindowId(1));
        assert_eq!(*host.opened.lock().unwrap(), ["settings"]);
        assert_eq!(*host.closed.lock().unwrap(), [WindowId(1)]);
    }
}
