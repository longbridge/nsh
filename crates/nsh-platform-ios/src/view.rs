//! [`AppView`]: a view that runs a nsh app, for standalone apps and for
//! embedding in existing ones.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use nsh_engine::{App, Bundle, Root};
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained};
use objc2_core_foundation::CGRect;
use objc2_ui_kit::{UIView, UIViewAutoresizing};

use crate::surface::{Surface, SurfaceOptions};

/// A UIKit view running a nsh app.
///
/// Add [`AppView::view`] to any hierarchy, then [`AppView::run`] a Rust
/// app's root component or [`AppView::load`] a JS bundle (or, in
/// development builds, [`AppView::connect_dev_server`]). Loading again
/// replaces the running app. Dropping the `AppView` stops the app.
pub struct AppView {
    inner: Rc<Inner>,
}

pub(crate) struct Inner {
    /// Hosts the surface and, in development, overlays above it.
    pub(crate) container: Retained<UIView>,
    state: RefCell<State>,
}

/// Receives fatal app errors.
type ErrorHandler = Rc<dyn Fn(&str)>;

struct State {
    surface: Option<Surface>,
    options: SurfaceOptions,
    on_error: Option<ErrorHandler>,
    #[cfg(feature = "dev")]
    dev: Option<crate::dev::DevSession>,
}

impl AppView {
    /// Creates an empty view; it resizes with its superview.
    pub fn new(mtm: MainThreadMarker, frame: CGRect, options: SurfaceOptions) -> AppView {
        let container = UIView::initWithFrame(UIView::alloc(mtm), frame);
        container.setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
        AppView {
            inner: Rc::new(Inner {
                container,
                state: RefCell::new(State {
                    surface: None,
                    options,
                    on_error: None,
                    #[cfg(feature = "dev")]
                    dev: None,
                }),
            }),
        }
    }

    /// The view to add to a hierarchy.
    pub fn view(&self) -> &UIView {
        &self.inner.container
    }

    /// Runs `bundle`, replacing the running app (its state is reset).
    pub fn load(&self, bundle: Bundle) {
        Inner::load(&self.inner, App::Js(bundle));
    }

    /// Runs a Rust app: its root component (`Root::of::<App>()`), replacing
    /// the running app.
    pub fn run(&self, root: Root) {
        Inner::load(&self.inner, App::Rust(root));
    }

    /// Sets where `asset:/…` files (images, fonts) are read from. A change
    /// applies from the next [`AppView::load`].
    pub fn set_asset_base(&self, base: Option<crate::AssetBase>) {
        let mut state = self.inner.state.borrow_mut();
        let changed = match (&state.options.asset_base, &base) {
            (None, None) => false,
            (Some(crate::AssetBase::Http(a)), Some(crate::AssetBase::Http(b))) => a != b,
            (Some(crate::AssetBase::Directory(a)), Some(crate::AssetBase::Directory(b))) => a != b,
            _ => true,
        };
        if changed {
            state.options.asset_base = base;
            // The next load starts a surface with the new location.
            if let Some(old) = state.surface.take() {
                old.view().removeFromSuperview();
            }
        }
    }

    /// What the apps this view loads are granted at most (`None`: what each
    /// app declares). Applies from the next [`AppView::load`].
    pub fn set_granted_capabilities(&self, granted: Option<nsh_engine::Capabilities>) {
        self.reconfigure(|options| options.granted = granted);
    }

    /// Registers native component `name` (`nativeComponent(name, ...)` in
    /// JS): `factory` makes one for every element. Applies to elements
    /// created from now on.
    pub fn register_component(&self, name: &str, factory: crate::ComponentFactory) {
        self.inner
            .state
            .borrow()
            .options
            .components
            .register(name, factory);
    }

    /// The VM limits of the apps this view loads. Applies from the next
    /// [`AppView::load`].
    pub fn set_limits(&self, limits: nsh_engine::Limits) {
        self.reconfigure(|options| options.limits = limits);
    }

    /// The directory the storage (`nsh/storage`) of the apps this view loads
    /// is kept in (`None`: in memory). A change applies from the next
    /// [`AppView::load`].
    pub fn set_storage_directory(&self, dir: Option<std::path::PathBuf>) {
        if self.inner.state.borrow().options.storage != dir {
            self.reconfigure(|options| options.storage = dir);
        }
    }

    fn reconfigure(&self, change: impl FnOnce(&mut SurfaceOptions)) {
        let mut state = self.inner.state.borrow_mut();
        change(&mut state.options);
        // The next load starts a surface with the new options.
        if let Some(old) = state.surface.take() {
            old.view().removeFromSuperview();
        }
    }

    /// Receives fatal app errors (bundle failed to load, uncaught JS
    /// exceptions) on the main thread. They are always logged; development
    /// builds also show them over the app.
    pub fn set_error_handler(&self, handler: impl Fn(&str) + 'static) {
        self.inner.state.borrow_mut().on_error = Some(Rc::new(handler));
    }

    /// Loads bundles from a `nsh dev` server (`ws://host:port/ws`) and
    /// reloads on every rebuild; build and runtime errors show over the app.
    #[cfg(feature = "dev")]
    pub fn connect_dev_server(&self, url: String) {
        // `ws://host:port/ws` -> assets at `http://host:port/assets/`.
        if let Some(rest) = url.strip_prefix("ws://")
            && let Some((host, _)) = rest.split_once('/')
        {
            self.inner.state.borrow_mut().options.asset_base =
                Some(crate::AssetBase::Http(format!("http://{host}/assets/")));
        }
        let session = crate::dev::DevSession::start(Rc::downgrade(&self.inner), url);
        self.inner.state.borrow_mut().dev = session;
    }
}

impl Inner {
    pub(crate) fn load(this: &Rc<Inner>, app: App) {
        let mut state = this.state.borrow_mut();
        if let Some(surface) = &state.surface {
            return surface.reload(app);
        }
        let mtm = this.container.mtm();
        let options = state.options.clone();
        let surface = match Surface::new(mtm, app, this.container.bounds(), options) {
            Ok(surface) => surface,
            Err(err) => return log::error!("failed to start the engine: {err}"),
        };
        surface.view().setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
        let weak: Weak<Inner> = Rc::downgrade(this);
        surface.set_error_handler(move |message| {
            if let Some(inner) = weak.upgrade() {
                inner.report_error(message);
            }
        });
        #[cfg(feature = "dev")]
        {
            let weak: Weak<Inner> = Rc::downgrade(this);
            surface.set_hot_failed_handler(move || {
                if let Some(inner) = weak.upgrade() {
                    crate::dev::DevSession::reload(&inner);
                }
            });
        }
        // Below development overlays.
        this.container.insertSubview_atIndex(surface.view(), 0);
        state.surface = Some(surface);
    }

    /// Applies a development update in place; false when no app runs.
    #[cfg(feature = "dev")]
    pub(crate) fn hot_update(&self, update: nsh_engine::HotUpdate) -> bool {
        self.state
            .borrow()
            .surface
            .as_ref()
            .is_some_and(|s| s.hot_update(update))
    }

    #[cfg(feature = "dev")]
    pub(crate) fn dev(&self) -> std::cell::Ref<'_, Option<crate::dev::DevSession>> {
        std::cell::Ref::map(self.state.borrow(), |s| &s.dev)
    }

    fn report_error(&self, message: &str) {
        let handler = self.state.borrow().on_error.clone();
        if let Some(handler) = handler {
            handler(message);
        }
        #[cfg(feature = "dev")]
        if let Some(dev) = &self.state.borrow().dev {
            dev.show_error(&self.container, "Uncaught error", message);
        }
    }
}
