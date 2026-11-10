//! Native controls (videos, web views) and the native components the
//! embedding app registers (`nsh/native`'s `nativeComponent`).
//!
//! Each control is an AppKit view hosted in its node's view
//! ([`NshView::set_native`]) filling it: the node's view keeps decoration,
//! frames, input and accessibility, as for any view, and [`resized`] keeps
//! the control at its size.
//!
//! - **Videos**: an `AVPlayer` shown in an `AVPlayerLayer` (the layer of a
//!   view that lets the pointer through, so presses and gestures reach the
//!   node as on any view), or in AVKit's `AVPlayerView` with the system's
//!   playback controls when `controls` is set (that view takes the mouse).
//!   AVFoundation and AVKit are loaded on first use (`dlopen`), as on iOS,
//!   so apps without videos do not pay for them at launch.
//! - **Web views**: a `WKWebView` (WebKit, also loaded on first use) that
//!   takes the mouse, wheel and keyboard itself. Pages talk to the app
//!   through `window.nsh.postMessage` (a script message
//!   handler); top-level navigations stay within the app's `webview`
//!   capability. WebKit on macOS has no scroll view to turn off: with
//!   `scrollable={false}` the root's `hitTest:` keeps wheel and
//!   trackpad scrolls from the web view ([`passes_wheel`]), so they move
//!   the app's scroll views around it, as on iOS (the page itself still
//!   scrolls with its keys and its own scripts).
//!   A web view with an `onDownload` listener saves what the page downloads
//!   (an attachment, a type WebKit cannot show, a `download` link) with
//!   WebKit's `WKDownload` into the app's data directory,
//!   `downloads/<id>/<name>`, and reports it as it starts, finishes or
//!   fails; without a listener those navigations stay as they were. In
//!   `Manual` download behavior nothing is written until the app answers
//!   `DownloadPending` with an `AcceptDownload` destination (or cancels,
//!   or the decision expires): WebKit holds its destination decision until
//!   then. An accepted destination is never touched before its transfer
//!   finishes: WebKit writes a unique staging sibling, and finishing
//!   publishes it (create-new, or an atomic replace only with
//!   `overwrite`); cancel, failure and teardown remove only the staging
//!   file.
//!   `Command::WebViewRequest` adds screenshots
//!   (`takeSnapshotWithConfiguration:`, PNG), the profile's cookies
//!   (`WKHTTPCookieStore`), `inspectable` (macOS 13.3+: WebKit has no
//!   public way to open its inspector, so `OpenDevtools` fails), and
//!   download acceptance/cancellation. `content_rules` are
//!   `WKContentRuleList`s: WebKit compiles each list and the user content
//!   controller applies the whole set before the page it guards may load,
//!   so the blocked requests never leave the page and the hiding rules are
//!   the engine's. While a requested nonempty set is compiling or its
//!   compile failed, `load`/`reload`/history commands wait in order and
//!   page or user navigation is refused with `onError`, so a new policy
//!   cannot be bypassed; screenshots, cookies and inspection keep
//!   working.
//!   `EventKind::WebViewProgress` reports WebKit's own
//!   `estimatedProgress` (KVO) while a page loads.
//!   Dropping the host cancels pending decisions and active downloads,
//!   releasing WebKit's destination decisions and removing only the output
//!   the platform was writing (a download's directory, or a manual
//!   download's staging file). The app owns completed files' retention.
//! - **Registered components**: [`AppView::register_component`] names a
//!   factory; the core creates a [`ViewKind::Native`] container, names the
//!   component ([`MountItem::Component`]) and sends its props as JSON as
//!   they change. Components emit events and report their own size through
//!   their [`ComponentContext`]. Registries belong to one `AppView` (and
//!   the views made from the same [`AppOptions`]): no globals.
//!
//! [`AppView::register_component`]: crate::AppView::register_component
//! [`AppOptions`]: crate::AppOptions
//! [`MountItem::Component`]: nsh_core::MountItem::Component

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    ffi::{CStr, c_void},
    path::{Path, PathBuf},
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use block2::RcBlock;
use nsh_core::access::WebMedia;
use nsh_core::{
    Bounds, Command, EventKind, EventPayload, JsonValue, NativeProps, NodeId, PayloadValue, Points,
    ResizeMode, Size, VideoProps, ViewKind, WebInput, WebKey, WebProps, WebSource,
    geometry::Point,
    webview::{CookieSameSite, WebCookie, WebDownloadBehavior, WebViewOperation, WebViewRequest},
};
use nsh_engine::{Access, App, EngineControl, Policy};
use objc2::{
    DefinedClass, Encode, Encoding, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool, ProtocolObject},
    sel,
};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWindow};
use objc2_core_foundation::CGSize;
use objc2_foundation::{
    NSArray, NSData, NSHTTPCookie, NSNotification, NSNotificationCenter, NSObject,
    NSObjectProtocol, NSOperationQueue, NSPoint, NSRect, NSString, NSURL,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    image::AssetBase,
    surface::Surface,
    view::{NshView, ViewObj},
};

/// A native component made by a registered factory.
///
/// ```no_run
/// use nsh_platform_macos::{
///     NativeComponent,
///     objc2::{MainThreadMarker, rc::Retained},
///     objc2_app_kit::{NSTextField, NSView},
///     objc2_foundation::NSString,
/// };
///
/// /// A label the app shows with a `text` prop.
/// struct Badge(Retained<NSTextField>);
///
/// impl Badge {
///     fn new(mtm: MainThreadMarker) -> Self {
///         Badge(NSTextField::labelWithString(&NSString::new(), mtm))
///     }
/// }
///
/// impl NativeComponent for Badge {
///     fn view(&self) -> Retained<NSView> {
///         Retained::into_super(Retained::into_super(self.0.clone()))
///     }
///
///     fn set_prop(&mut self, name: &str, value: Option<&serde_json::Value>) {
///         if name == "text" {
///             let text = value.and_then(|v| v.as_str()).unwrap_or("");
///             self.0.setStringValue(&NSString::from_str(text));
///         }
///     }
/// }
/// ```
pub trait NativeComponent {
    /// Its view; it fills the container nsh lays out.
    fn view(&self) -> Retained<NSView>;

    /// Prop `name` changed (`None`: removed). Every prop the app set comes
    /// right after the component is made.
    fn set_prop(&mut self, name: &str, value: Option<&JsonValue>);

    /// JS called `ref.command(name, args)`.
    fn command(&mut self, name: &str, args: &JsonValue) {
        let _ = (name, args);
    }

    /// Prop `name`, bound to an animated value, moved to `value` (on the
    /// frames it changes, without JS). By default, [`Self::set_prop`] with
    /// a JSON number.
    fn set_animated_number(&mut self, name: &str, value: f64) {
        let json = serde_json::Number::from_f64(value).map_or(JsonValue::Null, JsonValue::Number);
        self.set_prop(name, Some(&json));
    }

    /// Prop `name`, bound to an animated color, moved to `argb`
    /// (`0xAARRGGBB`). By default, [`Self::set_prop`] with a JSON number.
    fn set_animated_color(&mut self, name: &str, argb: u32) {
        self.set_prop(name, Some(&JsonValue::from(argb)));
    }
}

/// What a component can tell its app. Cheap to clone; usable from any
/// thread.
#[derive(Clone)]
pub struct ComponentContext {
    id: NodeId,
    control: EngineControl,
}

impl std::fmt::Debug for ComponentContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentContext")
            .field("id", &self.id)
            .finish()
    }
}

impl ComponentContext {
    /// Emits event `name` with `data`: the handler `on<Name>` gets it.
    pub fn emit(&self, name: &str, data: JsonValue) {
        self.control.dispatch_event(
            self.id,
            EventKind::NativeEvent,
            EventPayload::Native {
                name: name.into(),
                data,
            },
        );
    }

    /// The component's own size (points): used where its style leaves a
    /// dimension open.
    pub fn set_size(&self, width: f32, height: f32) {
        self.control
            .set_component_size(self.id, Size::new(Points(width), Points(height)));
    }
}

// Components may emit from any thread (their own callbacks, workers).
const _: () = {
    const fn send_sync<T: Send + Sync>() {}
    send_sync::<ComponentContext>();
};

/// Makes a component for an element, given its context.
pub type ComponentFactory = Rc<dyn Fn(ComponentContext) -> Box<dyn NativeComponent>>;

/// The components registered with a view (shared by its clones, so
/// registrations apply to elements created from then on).
#[derive(Clone, Default)]
pub struct Components {
    factories: Rc<RefCell<FxHashMap<Arc<str>, ComponentFactory>>>,
    /// Names already reported as unknown.
    missing: Rc<RefCell<FxHashSet<Arc<str>>>>,
}

impl std::fmt::Debug for Components {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set()
            .entries(self.factories.borrow().keys())
            .finish()
    }
}

impl Components {
    /// Registers `factory` under `name` (replacing an earlier one). Names
    /// under `nsh/` are NSH's own: registering one logs a warning and
    /// changes nothing.
    pub fn register(&self, name: &str, factory: ComponentFactory) {
        if nsh_core::platform::is_reserved_component(name) {
            log::warn!("native component `{name}` is not registered: names under `nsh/` are NSH's");
            return;
        }
        self.factories.borrow_mut().insert(name.into(), factory);
        self.missing.borrow_mut().remove(name);
    }

    /// The factory registered as `name`; `None` (logged once per name) if
    /// there is none.
    fn factory(&self, name: &Arc<str>) -> Option<ComponentFactory> {
        let factory = self.factories.borrow().get(name).cloned();
        if factory.is_none() && self.missing.borrow_mut().insert(name.clone()) {
            log::warn!(
                target: "js",
                "no native component is registered as `{name}`: it renders an empty view"
            );
        }
        factory
    }
}

/// The native controls of one surface.
#[derive(Default)]
pub(crate) struct NativeState {
    /// The components the app can use.
    components: RefCell<Components>,
    /// Where `asset:/` video sources are.
    asset_base: RefCell<Option<AssetBase>>,
    /// What the running app may show in web views (its `webview`
    /// capability); `None` before an app starts.
    access: RefCell<Option<Arc<Access>>>,
    /// Where the running app's web views save downloads (its data
    /// directory's `downloads`); `None` before an app starts.
    downloads: RefCell<Option<PathBuf>>,
    /// The controls, by node.
    hosts: RefCell<FxHashMap<NodeId, Host>>,
}

impl NativeState {
    /// Uses `components` (an [`crate::AppOptions`]'s registry).
    pub(crate) fn set_components(&self, components: Components) {
        *self.components.borrow_mut() = components;
    }

    /// The registry, to register in.
    pub(crate) fn components(&self) -> Components {
        self.components.borrow().clone()
    }

    /// Where `asset:/` sources resolve from now on.
    pub(crate) fn set_asset_base(&self, base: Option<AssetBase>) {
        *self.asset_base.borrow_mut() = base;
    }

    /// `app` is about to start, keeping its data in `data_dir`'s directory
    /// of it: its web views are limited to what its policy grants and save
    /// automatic downloads there (a manual download writes where its app
    /// accepts).
    pub(crate) fn set_app(&self, app: &App, data_dir: Option<&Path>) {
        *self.downloads.borrow_mut() = data_dir.map(|root| {
            root.join(nsh_engine::app_dir_name(app.app_id()))
                .join("downloads")
        });
        let access = match app {
            App::Js(bundle) => bundle.policy.access(),
            // Native code reaches what the platform allows.
            App::Rust(_) => Policy::default().access(),
        };
        *self.access.borrow_mut() = Some(Arc::new(access));
    }

    /// What the running app may reach (all before an app starts): what
    /// its image loads check their redirects against.
    pub(crate) fn access(&self) -> Arc<Access> {
        self.access.borrow().clone().unwrap_or_default()
    }

    /// Drops every control (the app stopped): media stops playing.
    pub(crate) fn clear(&self) {
        let hosts = std::mem::take(&mut *self.hosts.borrow_mut());
        drop(hosts);
    }
}

/// What a native view holds.
enum Host {
    Video(VideoHost),
    Web(Box<WebHost>),
    /// A registered component; `None` until named, or when unknown.
    Component(Option<Box<dyn NativeComponent>>),
}

impl Host {
    /// Its node's view and the hosted view, if that one shows what
    /// another process draws, which `renderInContext:` leaves out: a
    /// `WKWebView` (WebKit's web content process) or a video (an
    /// `AVPlayerLayer`, drawn by the media server).
    fn drawn_elsewhere(&self) -> Option<(NshView, Retained<NSView>)> {
        match self {
            Host::Web(host) => {
                let web = host.web.as_ref()?;
                let holder = host.holder.load().and_then(|v| NshView::from_ns(&v))?;
                // SAFETY: a WKWebView is an NSView.
                Some((holder, Retained::from(unsafe { as_view(web) })))
            }
            Host::Video(host) => Some((host.view.clone(), host.shown.clone()?)),
            Host::Component(_) => None,
        }
    }

    /// Whether `view` is the view it hosts in its node's view.
    fn hosts(&self, view: &NSView) -> bool {
        match self {
            Host::Web(host) => host.is(view),
            Host::Video(host) => host.shown.as_deref().is_some_and(|v| std::ptr::eq(v, view)),
            Host::Component(_) => false,
        }
    }
}

/// Sets up a web view, video or component container.
pub(crate) fn create(surface: &Rc<Surface>, view: &NshView) {
    let host = match view.kind() {
        ViewKind::Video => Host::Video(VideoHost::new(surface, view)),
        ViewKind::WebView => {
            let access = surface.native.access.borrow().clone();
            let downloads = surface.native.downloads.borrow().clone();
            Host::Web(Box::new(WebHost::new(surface, view, access, downloads)))
        }
        _ => Host::Component(None),
    };
    surface.native.hosts.borrow_mut().insert(view.id(), host);
}

/// A view was deleted: its control stops (a removal animation may keep
/// the view on screen a little longer).
pub(crate) fn deleted(surface: &Rc<Surface>, view: &NshView) {
    let host = surface.native.hosts.borrow_mut().remove(&view.id());
    drop(host);
}

/// The frame of `view` changed: its controls fill it again. Called by
/// [`NshView::place`] before it places its hosted views.
pub(crate) fn resized(view: &NshView) {
    let mut s = view.state_mut();
    if !matches!(
        s.kind,
        // An input's one hosted view is its pad (`crate::text_input`).
        ViewKind::Video | ViewKind::WebView | ViewKind::Native | ViewKind::Input
    ) {
        return;
    }
    let rect = bounds(s.frame.size.width.0, s.frame.size.height.0);
    for native in &mut s.natives {
        native.rect = rect;
    }
}

/// `NativeProps`.
pub(crate) fn props(surface: &Rc<Surface>, view: &NshView, props: &NativeProps) {
    let mut hosts = surface.native.hosts.borrow_mut();
    match (hosts.get_mut(&view.id()), props) {
        (Some(Host::Video(video)), NativeProps::Video(p)) => {
            let base = surface.native.asset_base.borrow();
            video.set(p, base.as_ref());
        }
        (Some(Host::Web(web)), NativeProps::WebView(p)) => web.set(p),
        _ => log::debug!("native: props for a view of another kind"),
    }
}

/// `Component`.
pub(crate) fn component(surface: &Rc<Surface>, view: &NshView, name: &Arc<str>) {
    let factory = surface.native.components.borrow().factory(name);
    let Some(factory) = factory else { return };
    let mut control = None;
    surface.control(|c| control = Some(c.clone()));
    let Some(control) = control else { return };
    let made = factory(ComponentContext {
        id: view.id(),
        control,
    });
    fill(view, &made.view());
    surface
        .native
        .hosts
        .borrow_mut()
        .insert(view.id(), Host::Component(Some(made)));
}

/// `ComponentProps`.
pub(crate) fn component_props(
    surface: &Rc<Surface>,
    view: &NshView,
    props: &[(Arc<str>, Option<JsonValue>)],
) {
    // Taken out while the app's code runs, so it may reach the surface.
    let Some(mut component) = take_component(surface, view.id()) else {
        return;
    };
    for (name, value) in props {
        component.set_prop(name, value.as_ref());
    }
    put_component(surface, view.id(), component);
}

/// A command for a web view, video or component.
pub(crate) fn command(surface: &Rc<Surface>, view: &NshView, command: &Command) {
    // `load`, `reload` and history moves cannot take a page out while a
    // requested content policy is not installed: they wait, in order, for
    // the rules to be in (or cleared). The borrow is let go before any
    // ObjC call.
    let held = match surface.native.hosts.borrow_mut().get_mut(&view.id()) {
        Some(Host::Web(host)) => host.hold_navigation(command),
        _ => false,
    };
    if held {
        return;
    }
    match command {
        Command::Seek(seconds) => {
            if let Some(Host::Video(video)) = surface.native.hosts.borrow().get(&view.id()) {
                video.seek(*seconds);
            }
        }
        Command::Native { name, args } => {
            if let Some(mut component) = take_component(surface, view.id()) {
                component.command(name, args);
                put_component(surface, view.id(), component);
            }
        }
        Command::SendInput(input) => {
            let web = match surface.native.hosts.borrow().get(&view.id()) {
                Some(Host::Web(web)) => web.web.clone(),
                _ => None,
            };
            if let Some(web) = web {
                web_input(&web, input);
            }
        }
        Command::Evaluate { .. } | Command::CancelEvaluate { .. } => {
            let found = match surface.native.hosts.borrow().get(&view.id()) {
                Some(Host::Web(web)) => Some((
                    web.handler.ivars().evaluations.clone(),
                    web.props.as_ref().is_none_or(|p| p.javascript_enabled),
                )),
                _ => None,
            };
            let Some((evaluations, scripts)) = found else {
                return;
            };
            match command {
                Command::Evaluate { request, script } => {
                    evaluations.start(*request, script, scripts)
                }
                Command::CancelEvaluate { request } => evaluations.cancel(*request),
                _ => {}
            }
        }
        Command::WebViewRequest(request) => {
            // The target is taken before anything may reenter the hosts
            // (accepting a download resumes WebKit, which calls the
            // delegate back).
            let target = match surface.native.hosts.borrow().get(&view.id()) {
                Some(Host::Web(host)) => Some((host.web.clone(), host.handler.clone())),
                _ => None,
            };
            match target {
                Some((Some(web), handler)) => {
                    web_request(surface, view.id(), &web, &handler, request);
                }
                Some((None, _)) => reply(
                    surface,
                    view.id(),
                    request.id,
                    Err("the web view is not available: WebKit could not be loaded".into()),
                ),
                None => reply(
                    surface,
                    view.id(),
                    request.id,
                    Err("this view is not a web view".into()),
                ),
            }
        }
        _ => {
            // WebKit may call the handler back synchronously (`stopLoading`
            // fails the navigation): the hosts are not borrowed meanwhile.
            let found = match surface.native.hosts.borrow().get(&view.id()) {
                Some(Host::Web(web)) => web
                    .web
                    .clone()
                    .map(|w| (w, web.handler.ivars().evaluations.clone())),
                _ => None,
            };
            if let Some((web, evaluations)) = found {
                if matches!(
                    command,
                    Command::Reload | Command::GoBack | Command::GoForward | Command::Load(_)
                ) {
                    evaluations.navigated();
                }
                web_command(&web, command);
            }
        }
    }
}

/// Whether `native` is a web view whose page does not scroll
/// (`scrollable={false}`): wheel and trackpad scrolls over it go to
/// the root, which scrolls the app's scroll views around it.
pub(crate) fn passes_wheel(surface: &Surface, native: &NSView) -> bool {
    let Ok(hosts) = surface.native.hosts.try_borrow() else {
        return false;
    };
    hosts.values().any(|host| match host {
        Host::Web(web) => !web.scrolls && web.is(native),
        _ => false,
    })
}

/// Whether the keyboard focus is in one of the surface's web views (the
/// page has it).
pub(crate) fn web_focused(surface: &Surface) -> bool {
    let Some(responder) = crate::input::event_root(surface)
        .ns()
        .window()
        .and_then(|w| w.firstResponder())
    else {
        return false;
    };
    let Some(focused) = responder.downcast_ref::<NSView>() else {
        return false;
    };
    let Ok(hosts) = surface.native.hosts.try_borrow() else {
        return false;
    };
    hosts.values().any(|host| match host {
        Host::Web(host) => host.web.as_ref().is_some_and(|web| {
            // SAFETY: a WKWebView is an NSView.
            focused.isDescendantOf(unsafe { as_view(web) })
        }),
        _ => false,
    })
}

define_class!(
    /// Holds a web view or video lent to the image of a screen or modal
    /// that leaves ([`lend`]), clipped where its ancestors clipped it.
    /// The pointer passes through (the image takes no events).
    #[unsafe(super(NSView, objc2_app_kit::NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NshLentClip"]
    struct LentClip;

    unsafe impl NSObjectProtocol for LentClip {}

    impl LentClip {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            None
        }
    }
);

/// A live web view or video lent to the image of a screen or modal that
/// leaves: `renderInContext:` cannot draw what other processes draw
/// (WebKit's, the media server), so the image shows the view itself
/// where it was. Dropping it (the image is gone) gives the view back to
/// its node's view if that still hosts it (the app kept the node), else
/// lets it go.
pub(crate) struct Lent {
    surface: Weak<Surface>,
    holder: NshView,
    shown: Retained<NSView>,
    clip: Retained<LentClip>,
}

impl Drop for Lent {
    fn drop(&mut self) {
        let id = self.holder.id();
        let back = self.surface.upgrade().is_some_and(|surface| {
            surface
                .native
                .hosts
                .try_borrow()
                .is_ok_and(|hosts| hosts.get(&id).is_some_and(|host| host.hosts(&self.shown)))
        });
        crate::surface::quiet(|| {
            if back {
                fill(&self.holder, &self.shown);
            } else {
                self.shown.removeFromSuperview();
            }
            self.clip.removeFromSuperview();
        });
    }
}

/// Moves the web views and videos inside `top` (a screen or modal that
/// leaves as an image of its layers) onto `image` (a view at `top`'s
/// place and size, showing that image), each where it showed and clipped
/// as it was (to a rectangle, rounded only by the corners of the one view
/// that clips it), so they keep showing their page or frame while the
/// image leaves. The keyboard focus does not go with them.
pub(crate) fn lend(surface: &Rc<Surface>, top: &NshView, image: &NSView) -> Vec<Lent> {
    let found: Vec<(NshView, Retained<NSView>)> = {
        let Ok(hosts) = surface.native.hosts.try_borrow() else {
            return Vec::new();
        };
        hosts
            .values()
            .filter_map(Host::drawn_elsewhere)
            .filter(|(_, shown)| shown.isDescendantOf(top.ns()))
            .collect()
    };
    let mut lent = Vec::new();
    for (holder, shown) in found {
        // `place_in` converts with AppKit's geometry, which the bounds place.
        crate::scroll::commit_ancestors(&holder);
        let Some(place) = place_in(&shown, top.ns()) else {
            continue;
        };
        if let Some(window) = shown.window() {
            let focused = window.firstResponder().is_some_and(|r| {
                r.downcast_ref::<NSView>()
                    .is_some_and(|v| v.isDescendantOf(&shown))
            });
            if focused {
                window.makeFirstResponder(Some(crate::input::root_in(surface, &window).ns()));
            }
        }
        // SAFETY: NSView's designated initializer.
        let clip: Retained<LentClip> =
            unsafe { msg_send![LentClip::alloc(surface.mtm), initWithFrame: place.clip] };
        crate::surface::quiet(|| {
            clip.setWantsLayer(true);
            if let Some(layer) = clip.layer() {
                layer.setMasksToBounds(true);
                if let Some(corners) = &place.corners {
                    layer.setCornerRadius(corners.cornerRadius());
                    layer.setMaskedCorners(corners.maskedCorners());
                    layer.setCornerCurve(&corners.cornerCurve());
                }
            }
            image.addSubview(&clip);
            holder
                .state_mut()
                .natives
                .retain(|n| !std::ptr::eq(&*n.view, &*shown));
            clip.addSubview(&shown);
            let (f, c) = (place.frame, place.clip);
            shown.setFrame(NSRect::new(
                NSPoint::new(f.origin.x - c.origin.x, f.origin.y - c.origin.y),
                f.size,
            ));
        });
        lent.push(Lent {
            surface: Rc::downgrade(surface),
            holder,
            shown,
            clip,
        });
    }
    lent
}

/// Where a lent view goes, in its leaving view's coordinates.
struct Place {
    /// The lent view's frame.
    frame: NSRect,
    /// What its ancestors let show of it.
    clip: NSRect,
    /// The layer whose rounded corners clipped it, when that layer alone
    /// made the clip.
    corners: Option<Retained<objc2_quartz_core::CALayer>>,
}

/// Where `shown` shows inside `top`; `None` when it does not (hidden, or
/// clipped away).
fn place_in(shown: &NSView, top: &NSView) -> Option<Place> {
    let frame = shown.convertRect_toView(shown.bounds(), Some(top));
    let mut clip = top.bounds();
    let mut innermost: Option<(NSRect, Retained<objc2_quartz_core::CALayer>)> = None;
    let mut at: Retained<NSView> = Retained::from(shown);
    while !std::ptr::eq(&*at, top) {
        if at.isHidden() || at.layer().is_some_and(|l| l.isHidden()) {
            return None;
        }
        at = crate::view::superview(&at)?;
        let Some(layer) = at.layer() else { continue };
        if layer.masksToBounds() || layer.mask().is_some() {
            let r = at.convertRect_toView(at.bounds(), Some(top));
            clip = intersect(clip, r)?;
            if innermost.is_none() {
                innermost = Some((r, layer));
            }
        }
    }
    let clip = intersect(clip, frame)?;
    // Corners belong to the clip only when its layer's bounds are the
    // whole clip; a mask (differing radii) is not copied.
    let corners = innermost
        .filter(|(r, layer)| layer.mask().is_none() && same_rect(*r, clip))
        .map(|(_, layer)| layer);
    Some(Place {
        frame,
        clip,
        corners,
    })
}

/// The intersection of two rectangles; `None` when it is empty.
fn intersect(a: NSRect, b: NSRect) -> Option<NSRect> {
    let x0 = a.origin.x.max(b.origin.x);
    let y0 = a.origin.y.max(b.origin.y);
    let x1 = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let y1 = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    (x1 > x0 && y1 > y0).then(|| NSRect::new(NSPoint::new(x0, y0), CGSize::new(x1 - x0, y1 - y0)))
}

fn same_rect(a: NSRect, b: NSRect) -> bool {
    (a.origin.x - b.origin.x).abs() < 0.5
        && (a.origin.y - b.origin.y).abs() < 0.5
        && (a.size.width - b.size.width).abs() < 0.5
        && (a.size.height - b.size.height).abs() < 0.5
}

/// Whether view `id` holds a made component (its animated props wait until
/// it does).
pub(crate) fn has_component(surface: &Surface, id: NodeId) -> bool {
    matches!(
        surface.native.hosts.borrow().get(&id),
        Some(Host::Component(Some(_)))
    )
}

/// Bound props that moved (`Animator::write_components`): handed to their
/// components.
pub(crate) fn component_animated(
    surface: &Surface,
    moved: Vec<(NodeId, Arc<str>, nsh_core::animation::AnimatedPropValue)>,
) {
    use nsh_core::animation::AnimatedPropValue;
    for (id, name, value) in moved {
        // Taken out while the app's code runs, so it may reach the surface.
        let Some(mut component) = take_component(surface, id) else {
            continue;
        };
        match value {
            AnimatedPropValue::Number(n) => component.set_animated_number(&name, n),
            AnimatedPropValue::Color(argb) => component.set_animated_color(&name, argb),
            _ => {}
        }
        put_component(surface, id, component);
    }
}

fn take_component(surface: &Surface, id: NodeId) -> Option<Box<dyn NativeComponent>> {
    match surface.native.hosts.borrow_mut().get_mut(&id) {
        Some(Host::Component(component)) => component.take(),
        _ => None,
    }
}

fn put_component(surface: &Surface, id: NodeId, component: Box<dyn NativeComponent>) {
    // The view may have been deleted meanwhile: then the component drops.
    if let Some(Host::Component(slot)) = surface.native.hosts.borrow_mut().get_mut(&id) {
        *slot = Some(component);
    }
}

/// Hosts `native` in `view`, filling it ([`resized`] keeps it so).
fn fill(view: &NshView, native: &NSView) {
    let frame = view.state().frame;
    view.set_native(native, bounds(frame.size.width.0, frame.size.height.0));
}

fn bounds(width: f32, height: f32) -> Bounds<Points> {
    Bounds {
        origin: Point {
            x: Points(0.0),
            y: Points(0.0),
        },
        size: Size {
            width: Points(width),
            height: Points(height),
        },
    }
}

fn load(framework: &CStr) -> bool {
    // SAFETY: loads a system framework; the handle is never closed.
    let loaded = !unsafe { libc::dlopen(framework.as_ptr(), libc::RTLD_LAZY) }.is_null();
    if !loaded {
        log::error!("cannot load {}", framework.to_string_lossy());
    }
    loaded
}

fn av_foundation() -> bool {
    static LOADED: OnceLock<bool> = OnceLock::new();
    *LOADED.get_or_init(|| load(c"/System/Library/Frameworks/AVFoundation.framework/AVFoundation"))
}

fn av_kit() -> bool {
    static LOADED: OnceLock<bool> = OnceLock::new();
    *LOADED.get_or_init(|| load(c"/System/Library/Frameworks/AVKit.framework/AVKit"))
}

fn webkit() -> bool {
    static LOADED: OnceLock<bool> = OnceLock::new();
    *LOADED.get_or_init(|| load(c"/System/Library/Frameworks/WebKit.framework/WebKit"))
}

fn class(name: &CStr) -> Option<&'static AnyClass> {
    AnyClass::get(name)
}

/// `object` as the AppKit view it is.
///
/// # Safety
///
/// `object` must be an `NSView`.
unsafe fn as_view(object: &AnyObject) -> &NSView {
    // SAFETY: the caller's guarantee.
    unsafe { &*(object as *const AnyObject as *const NSView) }
}

/// Sends a control's events to JS, when JS listens to them.
#[derive(Clone)]
struct Emitter {
    surface: Weak<Surface>,
    view: objc2::rc::Weak<ViewObj>,
    id: NodeId,
}

impl Emitter {
    fn new(surface: &Rc<Surface>, view: &NshView) -> Self {
        Emitter {
            surface: Rc::downgrade(surface),
            view: objc2::rc::Weak::new(&**view),
            id: view.id(),
        }
    }

    /// Whether the view has a listener for `kind`.
    fn listens(&self, kind: EventKind) -> bool {
        self.view
            .load()
            .and_then(|v| NshView::from_ns(&v))
            .is_some_and(|v| v.state().events.contains(kind))
    }

    fn emit(&self, kind: EventKind, payload: impl FnOnce() -> EventPayload) {
        if !self.listens(kind) {
            return;
        }
        if let Some(surface) = self.surface.upgrade() {
            surface.event(self.id, kind, payload());
        }
    }
}

fn error(message: String) -> EventPayload {
    EventPayload::Map(vec![("message", PayloadValue::Str(message))])
}

/// Where a video's source is.
#[derive(Debug, PartialEq)]
enum Location {
    /// A URL AVFoundation opens (`https://`, `file://`, ...).
    Uri(String),
    /// A file path.
    Path(std::path::PathBuf),
}

/// Resolves `source` (`asset:/` URIs against `base`).
fn locate(source: &str, base: Option<&AssetBase>) -> Option<Location> {
    if let Some(path) = source.strip_prefix("asset:/") {
        let path = path.trim_start_matches('/');
        return match base {
            Some(dirs @ (AssetBase::Directory(_) | AssetBase::Directories(_))) => {
                dirs.file(path).map(Location::Path)
            }
            Some(AssetBase::Http(url)) => Some(Location::Uri(format!(
                "{}/{path}",
                url.trim_end_matches('/')
            ))),
            None => None,
        };
    }
    if source.contains("://") {
        Some(Location::Uri(source.to_owned()))
    } else if source.is_empty() {
        None
    } else {
        Some(Location::Path(source.into()))
    }
}

/// CoreMedia's `CMTime`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

// SAFETY: the layout and encoding of CoreMedia's `CMTime` (`{?=qiIq}`).
unsafe impl Encode for CMTime {
    const ENCODING: Encoding = Encoding::Struct(
        "?",
        &[i64::ENCODING, i32::ENCODING, u32::ENCODING, i64::ENCODING],
    );
}

impl CMTime {
    fn from_seconds(seconds: f64) -> Self {
        Self {
            value: (seconds * 600.0).round() as i64,
            timescale: 600,
            flags: 1,
            epoch: 0,
        }
    }

    /// Seconds, or NaN when invalid or indefinite (live streams).
    fn seconds(&self) -> f64 {
        if self.flags & 1 == 0 || self.flags & 0x10 != 0 || self.timescale == 0 {
            f64::NAN
        } else {
            self.value as f64 / self.timescale as f64
        }
    }
}

define_class!(
    /// Shows a video's `AVPlayerLayer` (its layer) and lets the pointer
    /// through to the node's view.
    #[unsafe(super(NSView, objc2_app_kit::NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NshVideoLayerView"]
    struct VideoLayerView;

    unsafe impl NSObjectProtocol for VideoLayerView {}

    impl VideoLayerView {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            None
        }
    }
);

/// What a video reported so far.
#[derive(Default)]
struct Status {
    loaded: Cell<bool>,
    failed: Cell<bool>,
    buffering: Cell<bool>,
}

/// A video: its player and the view showing it.
struct VideoHost {
    emitter: Emitter,
    view: NshView,
    player: Option<Retained<AnyObject>>,
    /// The view showing the player: a [`VideoLayerView`] or an
    /// `AVPlayerView`.
    shown: Option<Retained<NSView>>,
    /// The `AVPlayerLayer` (without controls).
    layer: Option<Retained<AnyObject>>,
    time_observer: Option<Retained<AnyObject>>,
    end_observer: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    status: Rc<Status>,
    /// The props applied (`None` before the first).
    props: Option<VideoProps>,
    repeat: Rc<Cell<bool>>,
    /// The item played to its end (and does not loop).
    ended: Rc<Cell<bool>>,
}

impl VideoHost {
    fn new(surface: &Rc<Surface>, view: &NshView) -> Self {
        VideoHost {
            emitter: Emitter::new(surface, view),
            view: view.clone(),
            player: None,
            shown: None,
            layer: None,
            time_observer: None,
            end_observer: None,
            status: Rc::default(),
            props: None,
            repeat: Rc::default(),
            ended: Rc::default(),
        }
    }

    fn set(&mut self, p: &VideoProps, base: Option<&AssetBase>) {
        if !av_foundation() {
            return;
        }
        let old = self.props.take();
        let first = old.is_none();
        let old = old.unwrap_or_default();
        let reopened = first || p.source != old.source;
        if reopened {
            self.open(p.source.as_deref(), base);
        }
        if reopened || p.controls != old.controls {
            self.show(p.controls);
        }
        if reopened || p.progress_interval_ms != old.progress_interval_ms {
            self.observe(p.progress_interval_ms);
        }
        self.repeat.set(p.repeat);
        if let Some(player) = &self.player {
            // Played from the end, a video starts over: a seek the app
            // makes in the same turn arrives after these props.
            if !p.paused && old.paused && self.ended.replace(false) {
                seek_exactly(player, 0.0);
            }
            // SAFETY: AVPlayer's documented API.
            unsafe {
                let _: () = msg_send![player, setMuted: Bool::new(p.muted)];
                let _: () = msg_send![player, setVolume: p.volume.clamp(0.0, 1.0)];
                if p.paused {
                    let _: () = msg_send![player, pause];
                } else {
                    let _: () = msg_send![player, setRate: p.rate];
                }
            }
        }
        let gravity = NSString::from_str(match p.resize_mode {
            ResizeMode::Cover => "AVLayerVideoGravityResizeAspectFill",
            ResizeMode::Stretch => "AVLayerVideoGravityResize",
            _ => "AVLayerVideoGravityResizeAspect",
        });
        // SAFETY: AVPlayerLayer's / AVPlayerView's `videoGravity` setters.
        unsafe {
            if let Some(layer) = &self.layer {
                let _: () = msg_send![layer, setVideoGravity: &*gravity];
            } else if let Some(shown) = &self.shown {
                let _: () = msg_send![shown, setVideoGravity: &*gravity];
            }
        }
        self.props = Some(p.clone());
    }

    /// Opens `source` (or nothing).
    fn open(&mut self, source: Option<&str>, base: Option<&AssetBase>) {
        self.close();
        self.status = Rc::default();
        self.ended.set(false);
        let Some(source) = source else { return };
        let url = match locate(source, base) {
            Some(Location::Uri(uri)) => NSURL::URLWithString(&NSString::from_str(&uri)),
            Some(Location::Path(path)) => Some(NSURL::fileURLWithPath(&NSString::from_str(
                &path.to_string_lossy(),
            ))),
            None => None,
        };
        let (Some(url), Some(cls)) = (url, class(c"AVPlayer")) else {
            self.status.failed.set(true);
            let message = format!("{source}: cannot resolve the video's location");
            self.emitter.emit(EventKind::Error, || error(message));
            return;
        };
        // SAFETY: AVPlayer's documented API.
        let player: Retained<AnyObject> = unsafe { msg_send![cls, playerWithURL: &*url] };
        self.emitter
            .emit(EventKind::LoadStart, || EventPayload::None);
        // The end of the item: report it, and loop if asked.
        // SAFETY: AVPlayer's documented API.
        let item: Option<Retained<AnyObject>> = unsafe { msg_send![&player, currentItem] };
        let (emitter, repeat, ended) = (
            self.emitter.clone(),
            self.repeat.clone(),
            self.ended.clone(),
        );
        let weak_player = objc2::rc::Weak::from_retained(&player);
        let block = RcBlock::new(move |_: NonNull<NSNotification>| {
            emitter.emit(EventKind::End, || EventPayload::None);
            ended.set(!repeat.get());
            if repeat.get()
                && let Some(player) = weak_player.load()
            {
                // SAFETY: AVPlayer's documented API.
                unsafe {
                    let _: () = msg_send![&player, seekToTime: CMTime::from_seconds(0.0)];
                    let _: () = msg_send![&player, play];
                }
            }
        });
        // SAFETY: a documented notification, delivered on the main queue;
        // the observer is removed in `close`.
        self.end_observer = Some(unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(&NSString::from_str(
                    "AVPlayerItemDidPlayToEndTimeNotification",
                )),
                item.as_deref(),
                Some(&NSOperationQueue::mainQueue()),
                &block,
            )
        });
        poll_load(&player, self.emitter.clone(), &self.status);
        self.player = Some(player);
    }

    /// Stops the player and stops observing it.
    fn close(&mut self) {
        if let (Some(player), Some(token)) = (&self.player, self.time_observer.take()) {
            // SAFETY: removes the observer this host added.
            let _: () = unsafe { msg_send![player, removeTimeObserver: &*token] };
        }
        if let Some(observer) = self.end_observer.take() {
            // SAFETY: an observer token of the default center.
            unsafe { NSNotificationCenter::defaultCenter().removeObserver(observer.as_ref()) };
        }
        if let Some(player) = self.player.take() {
            // SAFETY: AVPlayer's documented API.
            let _: () = unsafe { msg_send![&player, pause] };
        }
    }

    /// Shows the player in a layer, or with the system's controls.
    fn show(&mut self, controls: bool) {
        if let Some(old) = self.shown.take() {
            self.view.remove_native(&old);
        }
        self.layer = None;
        let Some(player) = &self.player else { return };
        let mtm = MainThreadMarker::new().expect("main thread");
        // SAFETY: AVFoundation / AVKit's documented API, sent dynamically;
        // `AVPlayerView` is an `NSView`.
        let shown = unsafe {
            if controls
                && av_kit()
                && let Some(cls) = class(c"AVPlayerView")
            {
                let alloc: Allocated<AnyObject> = msg_send![cls, alloc];
                let view: Retained<AnyObject> = msg_send![alloc, initWithFrame: NSRect::ZERO];
                let _: () = msg_send![&view, setPlayer: &**player];
                // AVPlayerViewControlsStyleInline.
                let _: () = msg_send![&view, setControlsStyle: 1isize];
                Retained::from(as_view(&view))
            } else if let Some(cls) = class(c"AVPlayerLayer") {
                let layer: Retained<AnyObject> = msg_send![cls, playerLayerWithPlayer: &**player];
                let view: Retained<VideoLayerView> =
                    msg_send![VideoLayerView::alloc(mtm), initWithFrame: NSRect::ZERO];
                // A layer-hosting view: the player layer is its layer, so
                // it follows the view's size.
                let ca: &objc2_quartz_core::CALayer = &*(Retained::as_ptr(&layer) as *const _);
                view.setLayer(Some(ca));
                view.setWantsLayer(true);
                self.layer = Some(layer);
                Retained::into_super(view)
            } else {
                return;
            }
        };
        crate::surface::quiet(|| fill(&self.view, &shown));
        self.shown = Some(shown);
    }

    /// Reports `onProgress` every `interval_ms` while time passes, and the
    /// load, buffering and failure as they happen.
    fn observe(&mut self, interval_ms: f32) {
        let Some(player) = &self.player else { return };
        if let Some(token) = self.time_observer.take() {
            // SAFETY: removes the observer this host added.
            let _: () = unsafe { msg_send![player, removeTimeObserver: &*token] };
        }
        let (emitter, status) = (self.emitter.clone(), self.status.clone());
        let weak = objc2::rc::Weak::from_retained(player);
        let block = RcBlock::new(move |time: CMTime| {
            let Some(player) = weak.load() else { return };
            report(&player, &emitter, &status);
            let duration = item_duration(&player);
            emitter.emit(EventKind::Progress, || {
                EventPayload::Map(vec![
                    ("currentTime", PayloadValue::Number(time.seconds().max(0.0))),
                    ("duration", PayloadValue::Number(duration)),
                ])
            });
        });
        let interval = CMTime::from_seconds(interval_ms.max(16.0) as f64 / 1000.0);
        // SAFETY: AVPlayer's documented API; a nil queue is the main queue.
        self.time_observer = Some(unsafe {
            msg_send![
                player,
                addPeriodicTimeObserverForInterval: interval,
                queue: std::ptr::null::<AnyObject>(),
                usingBlock: &*block
            ]
        });
    }

    fn seek(&self, seconds: f64) {
        let Some(player) = &self.player else { return };
        seek_exactly(player, seconds);
        // The player stopped itself at the end; a video the app has not
        // paused plays on from where it was sought.
        if self.ended.replace(false)
            && let Some(props) = &self.props
            && !props.paused
        {
            // SAFETY: AVPlayer's documented API.
            let _: () = unsafe { msg_send![player, setRate: props.rate] };
        }
    }
}

/// Seeks `player` to `seconds`, to the frame.
fn seek_exactly(player: &AnyObject, seconds: f64) {
    let zero = CMTime::from_seconds(0.0);
    // SAFETY: AVPlayer's documented API.
    let _: () = unsafe {
        msg_send![
            player,
            seekToTime: CMTime::from_seconds(seconds.max(0.0)),
            toleranceBefore: zero,
            toleranceAfter: zero
        ]
    };
}

impl Drop for VideoHost {
    fn drop(&mut self) {
        // The view keeps showing the last frame while a removal animation
        // may still show it.
        self.close();
    }
}

/// Checks the player every 100ms until it loaded or failed (for at most 30
/// seconds): a paused player reports no time to the periodic observer.
/// Timed on the main queue rather than the display link, which stops while
/// the window is hidden.
fn poll_load(player: &Retained<AnyObject>, emitter: Emitter, status: &Rc<Status>) {
    poll(
        objc2::rc::Weak::from_retained(player),
        emitter,
        Rc::downgrade(status),
        0,
    );
}

fn poll(player: objc2::rc::Weak<AnyObject>, emitter: Emitter, status: Weak<Status>, attempt: u32) {
    let (Some(p), Some(s)) = (player.load(), status.upgrade()) else {
        return;
    };
    report(&p, &emitter, &s);
    if s.loaded.get() || s.failed.get() || attempt >= 300 {
        return;
    }
    let mtm = MainThreadMarker::new().expect("main thread");
    let state = dispatch2::MainThreadBound::new((player, emitter, status), mtm);
    let when = dispatch2::DispatchTime::NOW.time(100_000_000);
    let _ = dispatch2::DispatchQueue::main().after(when, move || {
        let mtm = MainThreadMarker::new().expect("main queue");
        let (player, emitter, status) = state.into_inner(mtm);
        poll(player, emitter, status, attempt + 1);
    });
}

fn item_duration(player: &AnyObject) -> f64 {
    // SAFETY: AVPlayer / AVPlayerItem getters.
    unsafe {
        let item: Option<Retained<AnyObject>> = msg_send![player, currentItem];
        item.map_or(f64::NAN, |item| {
            let duration: CMTime = msg_send![&item, duration];
            duration.seconds()
        })
    }
}

/// Reports a finished load (once), a failure (once) and buffering changes.
fn report(player: &AnyObject, emitter: &Emitter, status: &Status) {
    // SAFETY: AVPlayer / AVPlayerItem getters.
    unsafe {
        let item: Option<Retained<AnyObject>> = msg_send![player, currentItem];
        let Some(item) = item else { return };
        // AVPlayerItemStatus: 1 ready to play, 2 failed.
        let state: isize = msg_send![&item, status];
        if state == 1 && !status.loaded.replace(true) {
            let size: CGSize = msg_send![&item, presentationSize];
            emitter.emit(EventKind::Load, || {
                EventPayload::Map(vec![
                    ("duration", PayloadValue::Number(item_duration(player))),
                    ("width", PayloadValue::Number(size.width)),
                    ("height", PayloadValue::Number(size.height)),
                ])
            });
        }
        if state == 2 && !status.failed.replace(true) {
            let err: Option<Retained<AnyObject>> = msg_send![&item, error];
            let message = err
                .map(|e| {
                    let d: Retained<NSString> = msg_send![&e, localizedDescription];
                    d.to_string()
                })
                .unwrap_or_else(|| "the video failed to load".to_owned());
            log::warn!(target: "js", "video: {message}");
            emitter.emit(EventKind::Error, || error(message));
        }
        // AVPlayerTimeControlStatusWaitingToPlayAtSpecifiedRate.
        let control_status: isize = msg_send![player, timeControlStatus];
        let waiting = control_status == 1 && !status.failed.get();
        if status.buffering.replace(waiting) != waiting {
            emitter.emit(EventKind::Buffer, || {
                EventPayload::Map(vec![("isBuffering", PayloadValue::Bool(waiting))])
            });
        }
    }
}

/// Lets pages talk to the app: `window.nsh.postMessage(data)`.
const BRIDGE: &str = "window.nsh = window.nsh || { postMessage: function (data) { window.webkit.messageHandlers.nsh.postMessage(String(data)); } };";

/// WebKit's destination decision for one download: a `Block_copy` of the
/// completion handler, owned until the app accepts (a destination URL) or
/// cancels (null).
struct Decide {
    block: *mut block2::Block<dyn Fn(*mut NSURL) + 'static>,
}

impl Decide {
    /// Copies `completion`: WebKit keeps a completion handler callable
    /// after its delegate method returns, but the Rust borrow does not
    /// outlive the call.
    fn new(completion: &block2::Block<dyn Fn(*mut NSURL)>) -> Self {
        let copy = completion.copy();
        // The pointer's metadata is the block's; only the trait object's
        // (phantom) lifetime differs. The copy outlives the borrow because
        // it is a `Block_copy` of a completion handler, which WebKit may
        // call after the delegate method returns.
        let block = block2::RcBlock::into_raw(copy).cast::<()>()
            as *mut block2::Block<dyn Fn(*mut NSURL) + 'static>;
        Decide { block }
    }

    /// Answers with `url` (`None`: cancel).
    fn call(self, url: Option<&NSURL>) {
        let block = self.block;
        std::mem::forget(self);
        let url = url.map_or(std::ptr::null_mut(), |url| {
            url as *const NSURL as *mut NSURL
        });
        // SAFETY: `block` is the +1 this `Decide` owns, and it is called
        // once (`call` consumes it).
        if let Some(block) = unsafe { block2::RcBlock::from_raw(block) } {
            block.call((url,));
        }
    }
}

impl Drop for Decide {
    fn drop(&mut self) {
        if !self.block.is_null() {
            // SAFETY: owns the only reference to the copy made in `new`.
            unsafe {
                drop(block2::RcBlock::from_raw(self.block));
            }
        }
    }
}

/// Ivars of [`WebHandler`].
struct WebIvars {
    emitter: Emitter,
    /// What the app may show (its `webview` capability); `None`: nothing
    /// is known about the app, so nothing is refused.
    access: Option<Arc<Access>>,
    /// Where automatic downloads are saved; `None`: no app directory, so
    /// nothing is downloaded automatically.
    downloads: Option<PathBuf>,
    /// How downloads are treated: `Some(true)` manual (the app chooses
    /// every destination), `Some(false)` automatic; `None` for a behavior
    /// this backend does not know (nothing is downloaded then).
    behavior: Cell<Option<bool>>,
    /// Whether the requested content rules are installed; shared with the
    /// host (the navigation gate reads it in delegate callbacks without
    /// borrowing the hosts).
    rules_ready: Rc<Cell<bool>>,
    /// The web view's lifetime: false once its host is dropped, so an
    /// asynchronous completion (snapshot, cookie store, cancel) cannot
    /// deliver a reply to a restarted engine's node with the same id.
    alive: Arc<AtomicBool>,
    /// The downloads under way, by their `WKDownload`.
    active: RefCell<FxHashMap<usize, Download>>,
    /// The manual downloads waiting for the app's destination decision, by
    /// the id reported in `DownloadPending`.
    pending: RefCell<FxHashMap<String, PendingDownload>>,
    navigation: Cell<Option<usize>>,
    /// Response policies that converted a navigation into a download.
    downloading: RefCell<FxHashSet<usize>>,
    /// The folder path (`/a/b/`) of the `file://` page the app last loaded
    /// here: the only files this web view may open (its pages can link to
    /// each other, never to a file outside it).
    file: RefCell<Option<String>>,
    /// `evaluate()`'s requests in this web view.
    evaluations: Rc<nsh_platform_apple::web_evaluation::Evaluations>,
}

/// How long a manual download's destination decision waits for the app
/// before it is released (the download cancelled, the failure reported): a
/// decision the app never answers must not hold the page.
const DOWNLOAD_DECISION_TIMEOUT: Duration = Duration::from_secs(60);

/// A download under way: its `WKDownload` (kept alive until it ends) and
/// what `onDownload` reports of it.
struct Download {
    download: Retained<AnyObject>,
    id: String,
    url: String,
    filename: String,
    /// Where it writes: `downloads/<id>/<name>` (automatic), or the
    /// destination the app chose (manual). A manual download's bytes go to
    /// `stage` first and are published here when it finishes.
    path: PathBuf,
    /// The app chose `path`: its cleanup removes the staging file, not a
    /// directory of the platform's own.
    manual: bool,
    /// Where WebKit writes a manual download: a unique sibling of `path`,
    /// so the destination is never touched before it finishes.
    stage: Option<PathBuf>,
    /// `overwrite`: publishing replaces a file that appeared at `path`;
    /// without it, publishing fails rather than replace one.
    overwrite: bool,
}

/// A manual download waiting for the app to choose where it goes.
struct PendingDownload {
    /// The `WKDownload`, whose key finds its [`Download`] entry.
    download: Retained<AnyObject>,
    key: usize,
    /// WebKit's destination decision: called with the file URL to accept,
    /// or null to cancel. Nothing is written until it is called.
    decide: Decide,
}

/// App-driven navigations held while a requested content policy is not
/// installed (compiling, or its compile failed): they are replayed, in the
/// order the app asked, once the policy is in or cleared. Nothing is
/// dispatched meanwhile, so a `reload`, history move or `load` cannot take
/// a page out before the rules that were asked for apply.
#[derive(Default)]
struct PendingNavigation(VecDeque<Command>);

impl PendingNavigation {
    /// Holds `command` when it navigates and `ready` is false; answers
    /// whether it was held (other commands run at once).
    fn hold(&mut self, ready: bool, command: &Command) -> bool {
        if ready || !command_navigates(command) {
            return false;
        }
        self.0.push_back(command.clone());
        true
    }

    /// The held commands, in order, leaving the gate empty.
    fn take(&mut self) -> Vec<Command> {
        self.0.drain(..).collect()
    }
}

/// Whether `command` navigates the page (it must wait for a requested
/// content policy to install; everything else, including screenshots,
/// cookies and inspection, does not).
fn command_navigates(command: &Command) -> bool {
    matches!(
        command,
        Command::Load(_) | Command::Reload | Command::GoBack | Command::GoForward
    )
}

/// Whether a navigation to `url` must be refused while a requested content
/// policy is not installed. Empty and `about:blank` documents are not
/// network loads, so WebKit's own ones are left alone; anything else would
/// load unfiltered.
fn policy_refuses(ready: bool, url: &str) -> bool {
    !ready && !url.is_empty() && url != "about:blank"
}

define_class!(
    /// Navigation delegate and script message handler of one web view.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = WebIvars]
    #[name = "NshWebHandler"]
    struct WebHandler;

    unsafe impl NSObjectProtocol for WebHandler {}

    impl WebHandler {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive_message(&self, _controller: &AnyObject, message: &AnyObject) {
            // SAFETY: `body` of a WKScriptMessage.
            let body: Option<Retained<AnyObject>> = unsafe { msg_send![message, body] };
            let data = body.map(|b| describe(&b)).unwrap_or_default();
            self.emit(EventKind::Message, || {
                EventPayload::Map(vec![("data", PayloadValue::Str(data))])
            });
        }

        /// Top-level navigations stay within the app's `webview` capability.
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide_policy(
            &self,
            _web: &AnyObject,
            action: &AnyObject,
            decide: &block2::Block<dyn Fn(isize)>,
        ) {
            // SAFETY: WebKit's documented API (WKNavigationAction).
            let (main, url) = unsafe {
                let frame: Option<Retained<AnyObject>> = msg_send![action, targetFrame];
                // No target frame: a new window, which counts as top level.
                let main = frame.is_none_or(|f| msg_send![&*f, isMainFrame]);
                let request: Retained<AnyObject> = msg_send![action, request];
                let url: Option<Retained<NSURL>> = msg_send![&request, URL];
                let url = url
                    .and_then(|u| u.absoluteString())
                    .map(|s| s.to_string());
                (main, url.unwrap_or_default())
            };
            let ivars = self.ivars();
            // App-driven navigation is held until its requested policy is
            // installed; links, scripts, forms and history follow it too.
            if policy_refuses(ivars.rules_ready.get(), &url) {
                let message = format!(
                    "web view content rules are not applied yet: navigation to {url} is refused"
                );
                log::warn!(target: "js", "{message}");
                self.emit(EventKind::Error, || error(message));
                decide.call((0,));
                return;
            }
            let allowed = navigation_allowed(
                ivars.access.as_deref(),
                ivars.file.borrow().as_deref(),
                main,
                &url,
            );
            if !allowed {
                log::warn!(target: "js", "web view navigation to {url} is not granted");
                let message = format!("navigation to {url} is not granted");
                self.emit(EventKind::Error, || {
                    EventPayload::Map(vec![
                        ("message", PayloadValue::Str(message)),
                        ("code", PayloadValue::Str(nsh_core::element::codes::ACCESS_DENIED.into())),
                        ("url", PayloadValue::Str(url)),
                    ])
                });
            }
            // A `download` link becomes a download when the app listens.
            // SAFETY: WKNavigationAction's `shouldPerformDownload` (macOS
            // 11.3), asked only where it exists.
            let download = allowed
                && self.wants_download()
                && unsafe {
                    let known: Bool = msg_send![
                        action,
                        respondsToSelector: objc2::sel!(shouldPerformDownload)
                    ];
                    known.as_bool() && {
                        let should: Bool = msg_send![action, shouldPerformDownload];
                        should.as_bool()
                    }
                };
            // WKNavigationActionPolicyCancel = 0, Allow = 1, Download = 2.
            decide.call((if download { 2 } else { allowed as isize },));
        }

        /// The page's `getUserMedia` gets what the app's `webview.media`
        /// capability grants; the system still asks the user.
        #[unsafe(method(webView:requestMediaCapturePermissionForOrigin:initiatedByFrame:type:decisionHandler:))]
        fn media_capture(
            &self,
            _web: &AnyObject,
            _origin: &AnyObject,
            _frame: &AnyObject,
            kind: isize,
            decide: &block2::Block<dyn Fn(isize)>,
        ) {
            // WKMediaCaptureTypeCamera = 0, Microphone = 1, CameraAndMicrophone = 2.
            let wanted: &[WebMedia] = match kind {
                0 => &[WebMedia::Camera],
                1 => &[WebMedia::Microphone],
                _ => &[WebMedia::Camera, WebMedia::Microphone],
            };
            let access = self.ivars().access.as_ref();
            let granted = wanted
                .iter()
                .all(|m| access.is_some_and(|a| a.allows_web_media(*m)));
            if !granted {
                log::warn!(target: "js", "web view media capture is not granted: add it to capabilities.webview.media");
            }
            // WKPermissionDecisionGrant = 1, Deny = 2.
            decide.call((if granted { 1 } else { 2 },));
        }

        /// What the page cannot show, or the server sends as an attachment,
        /// is downloaded when the app listens for downloads.
        #[unsafe(method(webView:decidePolicyForNavigationResponse:decisionHandler:))]
        fn decide_response(
            &self,
            _web: &AnyObject,
            response: &AnyObject,
            decide: &block2::Block<dyn Fn(isize)>,
        ) {
            let download = self.wants_download() && is_download(response);
            if download && let Some(navigation) = self.ivars().navigation.get() {
                self.ivars().downloading.borrow_mut().insert(navigation);
            }
            // SAFETY: WKNavigationResponse's documented MIME support flag.
            let shows: Bool = unsafe { msg_send![response, canShowMIMEType] };
            // WKNavigationResponsePolicyCancel = 0, Allow = 1, Download = 2.
            decide.call((if download { 2 } else { shows.as_bool() as isize },));
        }

        #[unsafe(method(webView:navigationAction:didBecomeDownload:))]
        fn action_download(&self, _web: &AnyObject, _action: &AnyObject, download: &AnyObject) {
            self.adopt(download);
        }

        #[unsafe(method(webView:navigationResponse:didBecomeDownload:))]
        fn response_download(
            &self,
            _web: &AnyObject,
            _response: &AnyObject,
            download: &AnyObject,
        ) {
            self.adopt(download);
        }

        /// An automatic download gets a directory of its own, so two files
        /// of the same name never meet. A manual one waits here for the
        /// app: nothing is written until it accepts a destination (or the
        /// decision expires).
        #[unsafe(method(download:decideDestinationUsingResponse:suggestedFilename:completionHandler:))]
        fn download_destination(
            &self,
            download: &AnyObject,
            _response: &AnyObject,
            suggested: &NSString,
            done: &block2::Block<dyn Fn(*mut NSURL)>,
        ) {
            let key = download_key(download);
            let mut active = self.ivars().active.borrow_mut();
            let Some(entry) = active.get_mut(&key) else {
                drop(active);
                done.call((std::ptr::null_mut(),));
                return;
            };
            // The server's name replaces the one taken from the URL when it was adopted.
            entry.filename = file_name(&suggested.to_string());
            // The choice is the one made when the download was adopted: a
            // `download_behavior` change between the two callbacks must not
            // turn an automatic download's directory into a manual one's
            // chosen file (or the other way, whose cleanup would remove a
            // directory the app owns).
            let manual = entry.manual;
            if manual {
                let (id, url, filename) =
                    (entry.id.clone(), entry.url.clone(), entry.filename.clone());
                // Without a listener the decision would go unanswered: cancel.
                if !self.ivars().emitter.listens(EventKind::DownloadPending) {
                    log::warn!(
                        target: "js",
                        "web view download `{id}` has no `onDownloadPending` listener: cancelled"
                    );
                    active.remove(&key);
                    drop(active);
                    done.call((std::ptr::null_mut(),));
                    return;
                }
                // SAFETY: WebKit passes a live WKDownload; it is kept with
                // the decision until the app answers.
                let object =
                    unsafe { Retained::retain(download as *const AnyObject as *mut AnyObject) }
                        .expect("a download");
                let decide = Decide::new(done);
                drop(active);
                self.ivars().pending.borrow_mut().insert(
                    id.clone(),
                    PendingDownload {
                        download: object,
                        key,
                        decide,
                    },
                );
                self.emit(EventKind::DownloadPending, || {
                    EventPayload::Json(serde_json::json!({
                        "id": id,
                        "url": url,
                        "filename": filename,
                    }))
                });
                self.schedule_decision_timeout(id);
                return;
            }
            let dir = entry.path.parent().map(Path::to_path_buf).unwrap_or_default();
            entry.path = dir.join(&entry.filename);
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log::warn!(target: "js", "web view download into {}: {e}", dir.display());
                // WebKit then cancels it, which reaches no entry: the app hears of it here.
                let entry = active.remove(&key).expect("tracked above");
                drop(active);
                self.emit(EventKind::Download, || failed_event(&entry, e.to_string()));
                done.call((std::ptr::null_mut(),));
                return;
            }
            let destination = NSURL::fileURLWithPath(&NSString::from_str(&entry.path.to_string_lossy()));
            let started = download_event("started", entry, None);
            drop(active);
            self.emit(EventKind::Download, || started);
            done.call((
                Retained::as_ptr(&destination) as *mut NSURL,
            ));
        }

        #[unsafe(method(downloadDidFinish:))]
        fn download_finished(&self, download: &AnyObject) {
            let entry = self.ivars().active.borrow_mut().remove(&download_key(download));
            self.forget_pending(download);
            let Some(entry) = entry else {
                return;
            };
            match &entry.stage {
                // A manual download: publish the staging file at the
                // destination now. An existing file is replaced only when
                // the app said `overwrite`; otherwise publishing fails and
                // leaves it untouched.
                Some(stage) => {
                    let bytes = std::fs::metadata(stage).map_or(0, |m| m.len());
                    match publish_download(stage, &entry.path, entry.overwrite) {
                        Ok(()) => self.emit(EventKind::Download, || {
                            download_event("finished", &entry, Some(bytes))
                        }),
                        Err(error) => {
                            let _ = std::fs::remove_file(stage);
                            log::warn!(
                                target: "js",
                                "web view download into {}: {error}",
                                entry.path.display()
                            );
                            let message = format!(
                                "cannot publish `{}`: {error}",
                                entry.path.display()
                            );
                            self.emit(EventKind::Download, || failed_event(&entry, message));
                        }
                    }
                }
                None => {
                    let bytes = std::fs::metadata(&entry.path).map_or(0, |m| m.len());
                    self.emit(EventKind::Download, || {
                        download_event("finished", &entry, Some(bytes))
                    });
                }
            }
        }

        #[unsafe(method(download:didFailWithError:resumeData:))]
        fn download_failed(&self, download: &AnyObject, error: &AnyObject, _resume: Option<&AnyObject>) {
            let entry = self.ivars().active.borrow_mut().remove(&download_key(download));
            self.forget_pending(download);
            let Some(entry) = entry else {
                return;
            };
            // A download can fail before it has a destination (no response
            // yet): nothing was written then. Only the platform's own
            // output is removed: an automatic download's directory, or a
            // manual download's staging file (never the app's destination).
            remove_output(output_of(&entry));
            // SAFETY: `localizedDescription` of an NSError.
            let message: Retained<NSString> = unsafe { msg_send![error, localizedDescription] };
            self.emit(EventKind::Download, || failed_event(&entry, message.to_string()));
        }

        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn did_start(&self, web: &AnyObject, navigation: Option<&AnyObject>) {
            self.ivars().evaluations.navigated();
            self.ivars().navigation.set(navigation.map(download_key));
            self.emit(EventKind::LoadStart, || navigation_state(web, true));
            self.emit(EventKind::NavigationStateChange, || {
                navigation_state(web, true)
            });
            // Until WebKit estimates the progress there is none.
            self.emit(EventKind::WebViewProgress, || {
                EventPayload::Json(serde_json::json!({"progress": JsonValue::Null}))
            });
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn process_terminated(&self, _web: &AnyObject) {
            self.ivars().evaluations.process_terminated();
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, web: &AnyObject, navigation: Option<&AnyObject>) {
            if let Some(navigation) = navigation {
                self.ivars().downloading.borrow_mut().remove(&download_key(navigation));
            }
            self.emit(EventKind::Load, || navigation_state(web, false));
            self.emit(EventKind::NavigationStateChange, || {
                navigation_state(web, false)
            });
            self.emit(EventKind::WebViewProgress, || {
                EventPayload::Json(serde_json::json!({"progress": 1.0}))
            });
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail(&self, _web: &AnyObject, navigation: Option<&AnyObject>, error: &AnyObject) {
            self.failed(navigation, error);
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_provisional(
            &self,
            _web: &AnyObject,
            navigation: Option<&AnyObject>,
            error: &AnyObject,
        ) {
            self.failed(navigation, error);
        }

        /// `estimatedProgress` of the observed WKWebView (KVO): the value
        /// WebKit measured, as `WebViewProgress`.
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe_value(
            &self,
            key_path: &NSString,
            object: &AnyObject,
            _change: &AnyObject,
            _context: *mut c_void,
        ) {
            if key_path.to_string() != "estimatedProgress" {
                return;
            }
            // SAFETY: KVO calls this with the WKWebView the observer was
            // added to, and `estimatedProgress` is its documented getter.
            let progress: f64 = unsafe { msg_send![object, estimatedProgress] };
            self.emit(EventKind::WebViewProgress, || {
                EventPayload::Json(serde_json::json!({"progress": progress}))
            });
        }

    }
);

/// A manual download the app accepted: WebKit's destination decision and
/// the staging file it writes (published at the app's destination when it
/// finishes).
struct Accepted {
    decide: Decide,
    /// The [`Download`] entry the decision belongs to.
    key: usize,
    destination: Retained<NSURL>,
}

/// A cancel the caller may carry out once it has answered the request:
/// pending decisions are released, started transfers stopped and cleaned.
enum Cancelled {
    /// The app never accepted a destination: releasing the decision
    /// cancels the download and writes nothing.
    Pending(Decide),
    /// A transfer: cancelling stops it, removes the output it wrote, and
    /// reports the `Download` event.
    Active {
        download: Retained<AnyObject>,
        output: Output,
        event: EventPayload,
    },
}

/// The output a download was writing, so a cancel or failure removes only
/// what the platform owns.
#[derive(Debug, PartialEq)]
enum Output {
    /// An automatic download's directory of its own.
    Directory(PathBuf),
    /// The file at a destination the app chose.
    File(PathBuf),
    /// Nothing written yet.
    None,
}

/// The output a download owns: the unique staging file a download was
/// accepted into (never the destination the app chose, which cleanup must
/// not touch), or an automatic download's own directory.
///
/// A stored staging path wins over the behavior flag: if the two ever
/// disagree (a rename of the semantics, a mode change across callbacks),
/// cleanup must not infer an owned directory from the destination the app
/// chose.
fn output_for(manual: bool, stage: Option<&Path>, path: &Path) -> Output {
    if let Some(stage) = stage.filter(|stage| !stage.as_os_str().is_empty()) {
        return Output::File(stage.to_path_buf());
    }
    if manual {
        // Nothing has been written yet: nothing to remove.
        return Output::None;
    }
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => Output::Directory(dir.to_path_buf()),
        _ => Output::None,
    }
}

/// The output `entry` writes.
fn output_of(entry: &Download) -> Output {
    output_for(entry.manual, entry.stage.as_deref(), &entry.path)
}

/// The unique staging file a manual download writes next to `path`
/// (same directory, so publishing is a same-filesystem link or rename).
///
/// The first candidate can, by construction, be the destination the app
/// chose (it names a file, not a directory): a staging path equal to the
/// destination is skipped for a second fixed sibling. An existing
/// candidate is a refused staging slot, never overwritten.
fn download_stage_path(path: &Path, id: &str) -> Result<PathBuf, String> {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    for stage in [
        parent.join(format!(".nsh-download-{id}.part")),
        parent.join(format!(".nsh-download-{id}.part.tmp")),
    ] {
        if stage
            .to_string_lossy()
            .eq_ignore_ascii_case(&path.to_string_lossy())
        {
            continue;
        }
        match std::fs::symlink_metadata(&stage) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(stage),
            Ok(_) => {
                return Err(format!(
                    "`{}` is in the way of the download's staging file",
                    stage.display()
                ));
            }
            Err(error) => {
                return Err(format!(
                    "cannot stage the download in `{}`: {error}",
                    parent.display()
                ));
            }
        }
    }
    Err(format!(
        "cannot stage the download in `{}`",
        parent.display()
    ))
}

/// Publishes a finished manual download's staging file at `path`:
/// `overwrite` replaces a file there atomically; otherwise a create-new
/// link publishes only when nothing is there (a file that appeared
/// meanwhile is left untouched and the publish fails).
fn publish_download(stage: &Path, path: &Path, overwrite: bool) -> std::io::Result<()> {
    if overwrite {
        // A rename replaces the directory entry atomically.
        std::fs::rename(stage, path)
    } else {
        // A hard link creates the destination only if it does not exist.
        std::fs::hard_link(stage, path)?;
        // The published file has the destination's name; the staging name
        // is ours to remove.
        let _ = std::fs::remove_file(stage);
        Ok(())
    }
}

/// Removes what `output` holds (a download's own directory, or the file
/// it was writing).
fn remove_output(output: Output) {
    match output {
        Output::Directory(dir) => {
            let _ = std::fs::remove_dir_all(dir);
        }
        Output::File(file) => {
            let _ = std::fs::remove_file(file);
        }
        Output::None => {}
    }
}

/// Checks the app's destination for a manual download. An existing file is
/// refused without `overwrite`; with it the file is left untouched until
/// the finished download replaces it atomically. Answers the destination.
fn download_destination_path(path: &Path, overwrite: bool) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!(
            "`{}` is not an absolute destination path",
            path.display()
        ));
    }
    if path.file_name().is_none_or(|name| name.is_empty()) {
        return Err(format!("`{}` names no file", path.display()));
    }
    let Some(parent) = path.parent() else {
        return Err(format!("`{}` has no directory", path.display()));
    };
    if !parent.is_dir() {
        return Err(format!("`{}` is not in a directory", path.display()));
    }
    match std::fs::symlink_metadata(path) {
        Ok(info) => {
            if info.is_dir() {
                return Err(format!("`{}` is a directory", path.display()));
            }
            if !overwrite {
                return Err(format!(
                    "`{}` exists; accept the download with overwrite to replace it",
                    path.display()
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!("cannot use `{}`: {error}", path.display()));
        }
    }
    Ok(path.to_path_buf())
}

impl WebHandler {
    fn new(
        mtm: MainThreadMarker,
        emitter: Emitter,
        access: Option<Arc<Access>>,
        downloads: Option<PathBuf>,
        evaluations: Rc<nsh_platform_apple::web_evaluation::Evaluations>,
        rules_ready: Rc<Cell<bool>>,
        alive: Arc<AtomicBool>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(WebIvars {
            emitter,
            access,
            downloads,
            behavior: Cell::new(Some(false)),
            rules_ready,
            alive,
            active: RefCell::default(),
            pending: RefCell::default(),
            navigation: Cell::default(),
            downloading: RefCell::default(),
            file: RefCell::new(None),
            evaluations,
        });
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn emit(&self, kind: EventKind, payload: impl FnOnce() -> EventPayload) {
        self.ivars().emitter.emit(kind, payload);
    }

    /// `download_behavior` changed: from now on downloads either go to the
    /// app's data directory (automatic) or wait for the app's destination.
    fn set_behavior(&self, behavior: WebDownloadBehavior) {
        self.ivars().behavior.set(match behavior {
            WebDownloadBehavior::Automatic => Some(false),
            WebDownloadBehavior::Manual => Some(true),
            // Nothing may be written for a behavior this backend does not
            // know.
            _ => None,
        });
    }

    /// Whether this web view turns downloads into files: automatic mode
    /// with a data directory and an `onDownload` listener (the upstream
    /// opt-in), or manual mode with an `onDownloadPending` listener (the
    /// app chooses every destination). Without a listener the navigations
    /// stay as they were, and an unknown behavior downloads nothing.
    fn wants_download(&self) -> bool {
        match self.ivars().behavior.get() {
            Some(true) => self.ivars().emitter.listens(EventKind::DownloadPending),
            Some(false) => {
                self.ivars().downloads.is_some()
                    && self.ivars().emitter.listens(EventKind::Download)
            }
            None => false,
        }
    }

    /// Becomes the delegate of `download` and tracks it from now on, so a
    /// download that fails before WebKit asks for its destination is
    /// still reported. Its name comes from the URL until the server's is
    /// known.
    fn adopt(&self, download: &AnyObject) {
        // SAFETY: WKDownload's `delegate` (weak); this handler outlives it
        // as the web view's delegate, and keeps it in `active` until it ends.
        let _: () = unsafe { msg_send![download, setDelegate: self] };
        let Some(manual) = self.ivars().behavior.get() else {
            return;
        };
        let root = self.ivars().downloads.clone();
        if !manual && root.is_none() {
            return;
        }
        // SAFETY: NSUUID's and WKDownload's documented API.
        let (id, url) = unsafe {
            let uuid: Retained<AnyObject> = msg_send![class(c"NSUUID").expect("Foundation"), UUID];
            let id: Retained<NSString> = msg_send![&uuid, UUIDString];
            let request: Option<Retained<AnyObject>> = msg_send![download, originalRequest];
            let url: Option<Retained<NSURL>> = request.and_then(|r| msg_send![&r, URL]);
            let url = url.and_then(|u| u.absoluteString()).map(|s| s.to_string());
            (id.to_string().to_lowercase(), url.unwrap_or_default())
        };
        let filename = file_name(url_file_name(&url));
        // A manual download has no destination until the app picks one.
        let path = match root {
            Some(root) if !manual => root.join(&id).join(&filename),
            _ => PathBuf::new(),
        };
        // SAFETY: `download` is a live WKDownload this handler delegates.
        let object = unsafe { Retained::retain(download as *const AnyObject as *mut AnyObject) }
            .expect("a download");
        let entry = Download {
            download: object,
            id,
            url,
            filename,
            path,
            manual,
            stage: None,
            overwrite: false,
        };
        self.ivars()
            .active
            .borrow_mut()
            .insert(download_key(download), entry);
    }

    /// Accepts download `id` into `path`: WebKit writes to a unique staging
    /// file beside it, and the finished download publishes it (replacing an
    /// existing file only with `overwrite`). The destination is never
    /// touched before then. The pending decision stays while the
    /// destination is refused, so the app may retry it. The caller answers
    /// the request before [`Self::finish_accept`] starts the download.
    fn accept_download(&self, id: &str, path: &Path, overwrite: bool) -> Result<Accepted, String> {
        if !self.ivars().pending.borrow().contains_key(id) {
            return Err(format!("no download with id `{id}`"));
        }
        let path = download_destination_path(path, overwrite)?;
        // WebKit writes to the staging file, never the destination: the
        // finished download publishes it there. A refused staging slot
        // keeps the pending decision (the app may retry elsewhere).
        let stage = download_stage_path(&path, id)?;
        let Some(pending) = self.ivars().pending.borrow_mut().remove(id) else {
            return Err(format!("no download with id `{id}`"));
        };
        if let Some(entry) = self.ivars().active.borrow_mut().get_mut(&pending.key) {
            entry.path = path;
            entry.stage = Some(stage.clone());
            entry.overwrite = overwrite;
        }
        let destination = NSURL::fileURLWithPath(&NSString::from_str(&stage.to_string_lossy()));
        Ok(Accepted {
            decide: pending.decide,
            key: pending.key,
            destination,
        })
    }

    /// Starts an accepted download: WebKit writes to its staging file (the
    /// destination is published when it finishes), and the `Download`
    /// lifecycle reports it as automatic downloads are.
    fn finish_accept(&self, accepted: Accepted) {
        let started = self
            .ivars()
            .active
            .borrow()
            .get(&accepted.key)
            .map(|entry| download_event("started", entry, None));
        if let Some(started) = started {
            self.emit(EventKind::Download, || started);
        }
        let Accepted {
            decide,
            destination,
            ..
        } = accepted;
        decide.call(Some(&destination));
    }

    /// Cancels download `id`: a pending decision is released; an accepted
    /// transfer is stopped (the caller settles once writing has stopped,
    /// dropping only its staging file, and reports its failure).
    fn cancel_download(&self, id: &str) -> Result<Cancelled, String> {
        if let Some(pending) = self.ivars().pending.borrow_mut().remove(id) {
            self.ivars().active.borrow_mut().remove(&pending.key);
            return Ok(Cancelled::Pending(pending.decide));
        }
        let entry = {
            let mut active = self.ivars().active.borrow_mut();
            let key = active
                .iter()
                .find_map(|(key, entry)| (entry.id == id).then_some(*key));
            key.and_then(|key| active.remove(&key))
        };
        let Some(entry) = entry else {
            return Err(format!("no download with id `{id}`"));
        };
        let output = output_of(&entry);
        let event = failed_event(&entry, "cancelled".to_owned());
        Ok(Cancelled::Active {
            download: entry.download,
            output,
            event,
        })
    }

    /// Stops an accepted transfer, and only once WebKit reports that
    /// writing stopped removes the staging file it wrote and settles the
    /// request (a fire-and-forget cancel is not enough). A completed
    /// download was already taken out of `active`, so its published file
    /// is never touched.
    fn cancel_active(
        &self,
        download: Retained<AnyObject>,
        output: Output,
        event: EventPayload,
        surface: &Rc<Surface>,
        node: NodeId,
        request: u64,
    ) {
        let weak = Rc::downgrade(surface);
        let alive = self.ivars().alive.clone();
        // The block may be called again (it is an `Fn`): the output and
        // the event are taken once.
        let state = Rc::new(RefCell::new(Some((output, event))));
        let done = RcBlock::new(move |_resume: *const AnyObject| {
            let Some((output, event)) = state.borrow_mut().take() else {
                return;
            };
            // Writing has stopped: only the staging file is ours, and it is
            // removed even if the view is going.
            remove_output(output);
            while_alive(&alive, || {
                if let Some(surface) = weak.upgrade() {
                    reply(&surface, node, request, Ok(JsonValue::Null));
                    surface.event(node, EventKind::Download, event);
                }
            });
        });
        // SAFETY: the retained WKDownload is still live; cancel copies the
        // block and calls it after writing has stopped (on the main queue).
        unsafe {
            let _: () = msg_send![&download, cancel: &*done];
        }
    }

    /// Forgets the decision of a download WebKit finished or failed
    /// before the app answered it, releasing it (which cancels it).
    fn forget_pending(&self, download: &AnyObject) {
        let mut gone = Vec::new();
        {
            let mut pending = self.ivars().pending.borrow_mut();
            let mut keep = FxHashMap::default();
            for (id, decision) in pending.drain() {
                let same = std::ptr::eq(
                    Retained::as_ptr(&decision.download).cast::<u8>(),
                    (download as *const AnyObject).cast::<u8>(),
                );
                if same {
                    gone.push(decision.decide);
                } else {
                    keep.insert(id, decision);
                }
            }
            *pending = keep;
        }
        for decide in gone {
            decide.call(None);
        }
    }

    /// The app did not answer a manual download's destination in
    /// [`DOWNLOAD_DECISION_TIMEOUT`]: release it (cancelled) and report it
    /// as the failed download it is (nothing was written).
    fn expire_pending(&self, id: &str) {
        let Some(pending) = self.ivars().pending.borrow_mut().remove(id) else {
            return;
        };
        let entry = self.ivars().active.borrow_mut().remove(&pending.key);
        pending.decide.call(None);
        let message = "no download destination was accepted in time".to_owned();
        log::warn!(target: "js", "web view download `{id}`: {message}");
        if let Some(entry) = entry {
            let failed = failed_event(&entry, message);
            self.emit(EventKind::Download, || failed);
        }
    }

    /// Releases the decision of download `id` after
    /// [`DOWNLOAD_DECISION_TIMEOUT`].
    fn schedule_decision_timeout(&self, id: String) {
        let mtm = MainThreadMarker::new().expect("main thread");
        let target = dispatch2::MainThreadBound::new(
            (self.ivars().emitter.clone(), id, self.ivars().alive.clone()),
            mtm,
        );
        let when = dispatch2::DispatchTime::NOW.time(DOWNLOAD_DECISION_TIMEOUT.as_nanos() as i64);
        let _ = dispatch2::DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue");
            let (emitter, id, alive) = target.into_inner(mtm);
            if !alive.load(Ordering::Acquire) {
                return;
            }
            let Some(surface) = emitter.surface.upgrade() else {
                return;
            };
            let handler = match surface.native.hosts.borrow().get(&emitter.id) {
                Some(Host::Web(host)) => host.handler.clone(),
                _ => return,
            };
            handler.expire_pending(&id);
        });
    }

    fn failed(&self, navigation: Option<&AnyObject>, err: &AnyObject) {
        let downloading = navigation.is_some_and(|navigation| {
            self.ivars()
                .downloading
                .borrow_mut()
                .remove(&download_key(navigation))
        });
        // SAFETY: NSError's documented domain and code. WebKit reports 102
        // for the navigation we deliberately converted into a download.
        let (domain, code): (Retained<NSString>, isize) =
            unsafe { (msg_send![err, domain], msg_send![err, code]) };
        if downloading && domain.to_string() == "WebKitErrorDomain" && code == 102 {
            return;
        }
        // SAFETY: `localizedDescription` of an NSError.
        let message: Retained<NSString> = unsafe { msg_send![err, localizedDescription] };
        self.emit(EventKind::Error, || error(message.to_string()));
    }

    /// The web view is going: cancel pending decisions and active
    /// transfers, removing only the output the platform was writing.
    fn cancel_downloads(&self) {
        // Release the borrows before WebKit can call back into the delegate.
        let pending = std::mem::take(&mut *self.ivars().pending.borrow_mut());
        for (_, decision) in pending {
            decision.decide.call(None);
        }
        let active = std::mem::take(&mut *self.ivars().active.borrow_mut());
        for entry in active.into_values() {
            let output = output_of(&entry);
            let state = Rc::new(RefCell::new(Some(output)));
            let done = RcBlock::new(move |_resume: *const AnyObject| {
                if let Some(output) = state.borrow_mut().take() {
                    remove_output(output);
                }
            });
            // SAFETY: each retained WKDownload is still live; cancel copies
            // the block and calls it after writing has stopped.
            unsafe {
                let _: () = msg_send![&entry.download, setDelegate: std::ptr::null::<AnyObject>()];
                let _: () = msg_send![&entry.download, cancel: &*done];
            }
        }
    }
}

/// Whether a WKNavigationResponse is a file to save: the main frame's, of
/// a type WebKit cannot show or sent as an attachment.
fn is_download(response: &AnyObject) -> bool {
    // SAFETY: WKNavigationResponse's and NSHTTPURLResponse's documented API.
    unsafe {
        let main: Bool = msg_send![response, isForMainFrame];
        if !main.as_bool() {
            return false;
        }
        let shows: Bool = msg_send![response, canShowMIMEType];
        if !shows.as_bool() {
            return true;
        }
        let inner: Retained<AnyObject> = msg_send![response, response];
        let Some(http) = class(c"NSHTTPURLResponse") else {
            return false;
        };
        let is_http: Bool = msg_send![&inner, isKindOfClass: http];
        if !is_http.as_bool() {
            return false;
        }
        let disposition: Option<Retained<NSString>> = msg_send![
            &inner,
            valueForHTTPHeaderField: &*NSString::from_str("Content-Disposition")
        ];
        disposition.is_some_and(|d| is_attachment(&d.to_string()))
    }
}

/// Whether a `Content-Disposition` value says attachment.
fn is_attachment(disposition: &str) -> bool {
    disposition
        .split(';')
        .next()
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("attachment"))
}

/// A file name for a server's suggested one: its last component, without
/// separators or a leading dot, at most 200 bytes; `download` when nothing
/// is left.
fn file_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name: String = last
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(c, ':' | '\u{061c}' | '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{206f}')
        })
        .collect::<String>()
        .trim()
        .trim_start_matches('.')
        .to_owned();
    if name.len() > 200 {
        // Preserve a normal extension while shortening the stem on UTF-8 boundaries.
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| format!(".{ext}"))
            .filter(|ext| ext.len() + name.chars().next().map_or(0, char::len_utf8) <= 200);
        if let Some(extension) = &extension {
            name.truncate(name.len() - extension.len());
        }
        let limit = 200 - extension.as_ref().map_or(0, String::len);
        while name.len() > limit {
            name.pop();
        }
        if let Some(extension) = extension {
            name.push_str(&extension);
        }
    }
    if name.is_empty() {
        "download".to_owned()
    } else {
        name
    }
}

/// `onDownload`'s payload for a download that failed, with why.
fn failed_event(entry: &Download, message: String) -> EventPayload {
    let mut event = download_event("failed", entry, None);
    if let EventPayload::Map(fields) = &mut event {
        fields.push(("message", PayloadValue::Str(message)));
    }
    event
}

/// The last path segment of `url`, without its query or fragment.
fn url_file_name(url: &str) -> &str {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    url[..end].rsplit('/').next().unwrap_or_default()
}

fn download_key(download: &AnyObject) -> usize {
    download as *const AnyObject as usize
}

/// `onDownload`'s payload: `state`, `id`, `url`, `filename`, `path` and,
/// once finished, `bytes`.
fn download_event(state: &str, entry: &Download, bytes: Option<u64>) -> EventPayload {
    let mut fields = vec![
        ("state", PayloadValue::Str(state.to_owned())),
        ("id", PayloadValue::Str(entry.id.clone())),
        ("url", PayloadValue::Str(entry.url.clone())),
        ("filename", PayloadValue::Str(entry.filename.clone())),
        (
            "path",
            PayloadValue::Str(entry.path.to_string_lossy().into_owned()),
        ),
    ];
    if let Some(bytes) = bytes {
        fields.push(("bytes", PayloadValue::Number(bytes as f64)));
    }
    EventPayload::Map(fields)
}

/// `{ url, title, loading, canGoBack, canGoForward }` of a WKWebView.
fn navigation_state(web: &AnyObject, loading: bool) -> EventPayload {
    // SAFETY: WKWebView getters.
    let (url, title, back, forward) = unsafe {
        let url: Option<Retained<NSURL>> = msg_send![web, URL];
        let title: Option<Retained<NSString>> = msg_send![web, title];
        let back: Bool = msg_send![web, canGoBack];
        let forward: Bool = msg_send![web, canGoForward];
        (url, title, back.as_bool(), forward.as_bool())
    };
    let url = url
        .and_then(|u| u.absoluteString())
        .map(|s| s.to_string())
        .unwrap_or_default();
    EventPayload::Map(vec![
        ("url", PayloadValue::Str(url)),
        (
            "title",
            PayloadValue::Str(title.map(|t| t.to_string()).unwrap_or_default()),
        ),
        ("loading", PayloadValue::Bool(loading)),
        ("canGoBack", PayloadValue::Bool(back)),
        ("canGoForward", PayloadValue::Bool(forward)),
    ])
}

fn describe(object: &AnyObject) -> String {
    // SAFETY: every NSObject has a description.
    let d: Retained<NSString> = unsafe { msg_send![object, description] };
    d.to_string()
}

/// Whether a web view may navigate to `url`: a frame inside the page, a
/// file in the folder of the `file://` page the app itself loaded there
/// (`folder`, which the `webview` grant, made of web hosts, can't name),
/// and what the grant allows. Never a file outside that folder.
fn navigation_allowed(
    access: Option<&Access>,
    folder: Option<&str>,
    main: bool,
    url: &str,
) -> bool {
    if !main || url.is_empty() {
        return true;
    }
    if is_file_url(url) {
        return folder.is_some_and(|dir| {
            local_file_path(url).is_some_and(|path| path.starts_with(dir) && !has_dot_segment(path))
        });
    }
    access.is_none_or(|a| a.allows_webview(url))
}

fn is_file_url(url: &str) -> bool {
    url.get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case("file:"))
}

/// The path of a local `file://` URL, without query or fragment: `None`
/// for any other URL and for a file on another host. `file:///a` and
/// `file://localhost/a` are the same file, `/a`.
fn local_file_path(url: &str) -> Option<&str> {
    const SCHEME: &str = "file://";
    if !url
        .get(..SCHEME.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(SCHEME))
    {
        return None;
    }
    let rest = &url[SCHEME.len()..];
    let slash = rest.find('/')?;
    let host = &rest[..slash];
    if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
        return None;
    }
    let path = &rest[slash..];
    Some(path.split(['?', '#']).next().unwrap_or(path))
}

/// Whether `path` has a `.` or `..` segment, written out or
/// percent-encoded (`%2e`, `..%2f`), which could name a folder above the
/// one it seems to.
fn has_dot_segment(path: &str) -> bool {
    percent_decoded(path)
        .split(|&b| b == b'/')
        .any(|segment| segment == b"." || segment == b"..")
}

/// The folder path (`/a/b/`) a `file://` page may read: the file's own and
/// what is under it, nothing above. `None` for any other URL, which loads
/// as a request, for a file on another host, for a file at the root, whose
/// folder is the whole disk (`file://localhost/a.pdf` included), and for a
/// path with a `.` or `..` segment.
fn file_read_scope(uri: &str) -> Option<&str> {
    let path = local_file_path(uri)?;
    if has_dot_segment(path) {
        return None;
    }
    let end = path.rfind('/')? + 1;
    (end > 1).then(|| &path[..end])
}

/// `s` with each `%XX` turned into its byte; a `%` not followed by two hex
/// digits stays as it is.
fn percent_decoded(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// A JS string literal for `s`.
fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A web view: the `WKWebView` (when WebKit loads) filling its node's view,
/// made with its first props (`incognito` picks its website data store).
struct WebHost {
    /// The node's view, which hosts the web view.
    holder: objc2::rc::Weak<ViewObj>,
    web: Option<Retained<AnyObject>>,
    handler: Retained<WebHandler>,
    /// Serves the URL schemes of the first props while the web view is
    /// here.
    schemes: Option<nsh_platform_apple::web_scheme::SchemeServer>,
    /// The props applied (`None` before the first).
    props: Option<WebProps>,
    /// The page scrolls with the wheel and the trackpad
    /// (`scrollable`).
    scrolls: bool,
    /// The surface and node the view belongs to (content rules are
    /// compiled asynchronously and added back here).
    surface: Weak<Surface>,
    node: NodeId,
    /// Counts the content rule changes: a compilation finishing for rules
    /// that were replaced meanwhile is dropped.
    rules_gen: u64,
    /// Whether the wanted content rule lists are installed: a source that
    /// must load waits until they are (`pending_source`), and app-driven
    /// navigation waits in `pending_nav`. Shared with the handler, whose
    /// navigation delegate refuses page navigation meanwhile.
    rules_ready: Rc<Cell<bool>>,
    /// The source that waits for the content rules.
    pending_source: Option<WebSource>,
    /// App-driven navigations (`load`, `reload`, back, forward) held until
    /// the wanted rules are installed or cleared.
    pending_nav: PendingNavigation,
    /// False once this host is dropped: asynchronous completions stop
    /// delivering replies then (cleanup still runs).
    alive: Arc<AtomicBool>,
    /// The content rule lists installed in the user content controller:
    /// `removeContentRuleList:` takes the compiled list, not its
    /// identifier, so the objects are kept to remove them.
    rules_applied: Vec<(Arc<str>, Retained<AnyObject>)>,
}

impl WebHost {
    fn new(
        surface: &Rc<Surface>,
        view: &NshView,
        access: Option<Arc<Access>>,
        downloads: Option<PathBuf>,
    ) -> Self {
        // Replies are internal events: they reach the engine whether or not
        // the app listens.
        let weak = Rc::downgrade(surface);
        let evaluations = nsh_platform_apple::web_evaluation::Evaluations::new(
            view.id(),
            Box::new(move |id, kind, payload| {
                if let Some(surface) = weak.upgrade() {
                    surface.event(id, kind, payload);
                }
            }),
        );
        let rules_ready = Rc::new(Cell::new(true));
        let alive = Arc::new(AtomicBool::new(true));
        let handler = WebHandler::new(
            surface.mtm,
            Emitter::new(surface, view),
            access,
            downloads,
            evaluations,
            rules_ready.clone(),
            alive.clone(),
        );
        WebHost {
            holder: objc2::rc::Weak::new(&**view),
            web: None,
            handler,
            schemes: None,
            props: None,
            scrolls: true,
            surface: Rc::downgrade(surface),
            node: view.id(),
            rules_gen: 0,
            rules_ready,
            pending_source: None,
            pending_nav: PendingNavigation::default(),
            alive,
            rules_applied: Vec::new(),
        }
    }

    /// Makes the `WKWebView`, keeping its website data in memory only
    /// (a store of its own) when `incognito`, serving the app's URL
    /// schemes.
    fn make(&mut self, p: &WebProps) {
        let Some(view) = self.holder.load().and_then(|v| NshView::from_ns(&v)) else {
            return;
        };
        let handler = &self.handler;
        let web = webkit().then(|| {
            // SAFETY: WebKit's documented API, sent dynamically; the handler
            // is unregistered when the host is dropped; a WKWebView is an
            // NSView.
            unsafe {
                let config: Retained<AnyObject> = msg_send![class(c"WKWebViewConfiguration")?, new];
                if p.incognito {
                    let store: Retained<AnyObject> =
                        msg_send![class(c"WKWebsiteDataStore")?, nonPersistentDataStore];
                    let _: () = msg_send![&config, setWebsiteDataStore: &*store];
                }
                self.schemes = nsh_platform_apple::web_scheme::serve(
                    handler.mtm(),
                    &config,
                    &p.url_schemes,
                );
                let content: Retained<AnyObject> = msg_send![&config, userContentController];
                let _: () = msg_send![
                    &content,
                    addScriptMessageHandler: &**handler,
                    name: &*NSString::from_str("nsh")
                ];
                let alloc: Allocated<AnyObject> = msg_send![class(c"WKWebView")?, alloc];
                let web: Retained<AnyObject> = msg_send![
                    alloc,
                    initWithFrame: NSRect::ZERO,
                    configuration: &*config
                ];
                let _: () = msg_send![&web, setNavigationDelegate: &**handler];
                let _: () = msg_send![&web, setUIDelegate: &**handler];
                // Inspectable is off unless the app asks for it
                // (`SetInspectable`); setting it is macOS 13.3+.
                let inspectable: Bool =
                    msg_send![&web, respondsToSelector: sel!(setInspectable:)];
                if inspectable.as_bool() {
                    let _: () = msg_send![&web, setInspectable: Bool::new(false)];
                }
                // `estimatedProgress` is KVO-compliant: report what WebKit
                // measured as it loads.
                let _: () = msg_send![
                    &web,
                    addObserver: &**handler,
                    forKeyPath: &*NSString::from_str("estimatedProgress"),
                    options: 1usize,
                    context: std::ptr::null_mut::<c_void>()
                ];
                // Transparent by default (WebKit paints white otherwise): a
                // page without a background shows the view behind it, the
                // way `backgroundColor` on the node expects.
                let no: Retained<AnyObject> = msg_send![class(c"NSNumber")?, numberWithBool: false];
                let _: () = msg_send![&web, setValue: &*no, forKey: &*NSString::from_str("drawsBackground")];
                crate::surface::quiet(|| fill(&view, as_view(&web)));
                Some(web)
            }
        });
        self.web = web.flatten();
        self.handler.ivars().evaluations.set_web(self.web.as_ref());
    }

    /// Whether `view` is its `WKWebView`.
    fn is(&self, view: &NSView) -> bool {
        self.web.as_ref().is_some_and(|web| {
            std::ptr::eq(
                Retained::as_ptr(web).cast::<u8>(),
                (view as *const NSView).cast::<u8>(),
            )
        })
    }

    fn set(&mut self, p: &WebProps) {
        self.scrolls = p.scrollable;
        self.handler.set_behavior(p.download_behavior);
        if self.props.is_none() {
            // The core keeps `incognito` and the URL schemes from changing
            // once it is made.
            self.make(p);
        }
        let Some(web_view) = self.web.clone() else {
            return;
        };
        if !p.javascript_enabled {
            self.handler.ivars().evaluations.scripts_disabled();
        }
        let old = self.props.take();
        let first = old.is_none();
        let old = old.unwrap_or_default();
        // The rules installed keep applying while the whole set compiles;
        // a page must not go out before it is in. Clearing blocking takes
        // effect at once.
        if first || p.content_rules != old.content_rules {
            self.rules_gen = self.rules_gen.wrapping_add(1);
            if p.content_rules.is_empty() {
                self.remove_applied(&web_view);
                self.rules_ready.set(true);
            } else {
                self.rules_ready.set(false);
                let rules = p
                    .content_rules
                    .iter()
                    .map(|rule| (rule.id.clone(), rule.rules.clone()))
                    .collect();
                self.schedule_rules(self.rules_gen, rules);
            }
        }
        let web: &AnyObject = &web_view;
        let scripts_changed = first
            || p.injected_js != old.injected_js
            || p.injected_js_before != old.injected_js_before;
        // SAFETY: WebKit's documented API.
        unsafe {
            let config: Retained<AnyObject> = msg_send![web, configuration];
            if scripts_changed {
                let content: Retained<AnyObject> = msg_send![&config, userContentController];
                let _: () = msg_send![&content, removeAllUserScripts];
                let add = |source: &str, at_end: bool| {
                    let Some(cls) = class(c"WKUserScript") else {
                        return;
                    };
                    let alloc: Allocated<AnyObject> = msg_send![cls, alloc];
                    // WKUserScriptInjectionTime: 0 at document start, 1 at
                    // its end.
                    let script: Retained<AnyObject> = msg_send![
                        alloc,
                        initWithSource: &*NSString::from_str(source),
                        injectionTime: at_end as isize,
                        forMainFrameOnly: Bool::YES
                    ];
                    let _: () = msg_send![&content, addUserScript: &*script];
                };
                add(BRIDGE, false);
                if let Some(js) = &p.injected_js_before {
                    add(js, false);
                }
                if let Some(js) = &p.injected_js {
                    add(js, true);
                }
            }
            let ua = p.user_agent.as_deref().map(NSString::from_str);
            let _: () = msg_send![web, setCustomUserAgent: ua.as_deref()];
            let _: () = msg_send![
                web,
                setAllowsBackForwardNavigationGestures: Bool::new(p.back_forward_gestures)
            ];
            let prefs: Retained<AnyObject> = msg_send![&config, defaultWebpagePreferences];
            let _: () =
                msg_send![&prefs, setAllowsContentJavaScript: Bool::new(p.javascript_enabled)];
        }
        // Scripts apply to pages loaded from now on: load again, once the
        // wanted content rules are the ones applied (the page's first
        // requests must not go out unblocked).
        let wants_load = first || p.source != old.source || scripts_changed;
        self.props = Some(p.clone());
        if wants_load && !self.rules_ready.get() {
            // The wanted rules are compiling: the page waits (its first
            // request must not go out unblocked).
            self.pending_source = Some(p.source.clone());
        } else if self.rules_ready.get() {
            // Ready (the props were applied before any rules, cleared
            // blocking, or this is the first props): the page that waited
            // may load, then the navigations the app asked for meanwhile,
            // in order. A failed set leaves them not ready, so its page
            // and those navigations stay held.
            if wants_load {
                self.pending_source = Some(p.source.clone());
            }
            self.replay_navigation();
        }
    }

    /// Holds `command` when it navigates and the wanted rules are not
    /// installed; answers whether it was held.
    fn hold_navigation(&mut self, command: &Command) -> bool {
        self.pending_nav.hold(self.rules_ready.get(), command)
    }

    /// The rules are in (or cleared): loads the page that waited, then
    /// replays the app's held navigations in the order it asked.
    fn replay_navigation(&mut self) {
        let source = self.pending_source.take();
        let queued = self.pending_nav.take();
        if let Some(web) = self.web.clone() {
            apply_navigation(&web, &self.handler, source, queued);
        }
    }

    /// Removes every content rule list the controller took (by object:
    /// `removeContentRuleList:` takes the list, not its identifier).
    fn remove_applied(&mut self, web: &AnyObject) {
        if self.rules_applied.is_empty() {
            return;
        }
        // SAFETY: WebKit's documented API.
        unsafe {
            let config: Retained<AnyObject> = msg_send![web, configuration];
            let content: Retained<AnyObject> = msg_send![&config, userContentController];
            for (_, list) in self.rules_applied.drain(..) {
                let _: () = msg_send![&content, removeContentRuleList: &*list];
            }
        }
    }

    /// Schedules compiling the complete wanted `rules` (identifier, JSON)
    /// for this view. The lists installed keep applying while they
    /// compile; [`finish_rules`] replaces them all at once when every one
    /// compiled, and only then does the source that waited load.
    fn schedule_rules(&self, generation: u64, rules: Vec<(Arc<str>, Arc<str>)>) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        // WebKit's compiler runs off the app's thread: start it after this
        // call (the hosts are borrowed while props are applied).
        let work =
            dispatch2::MainThreadBound::new((self.surface.clone(), self.node, generation), mtm);
        let alive = self.alive.clone();
        dispatch2::DispatchQueue::main().exec_async(move || {
            let mtm = MainThreadMarker::new().expect("main queue");
            let (surface, node, generation) = work.get(mtm);
            let Some(surface) = surface.upgrade() else {
                return;
            };
            compile_content_rules(&surface, *node, *generation, rules, alive);
        });
    }
}

impl Drop for WebHost {
    fn drop(&mut self) {
        // From here no completion (snapshot, cookie, cancel) delivers a
        // reply; cleanup below still runs.
        self.alive.store(false, Ordering::Release);
        self.handler.cancel_downloads();
        self.handler.ivars().evaluations.dispose();
        let Some(web) = &self.web else { return };
        // SAFETY: breaks the cycle web view → content controller → handler
        // and stops the page.
        unsafe {
            let _: () = msg_send![
                web,
                removeObserver: &*self.handler,
                forKeyPath: &*NSString::from_str("estimatedProgress")
            ];
            let _: () = msg_send![web, stopLoading];
            let _: () = msg_send![web, setNavigationDelegate: std::ptr::null::<AnyObject>()];
            let _: () = msg_send![web, setUIDelegate: std::ptr::null::<AnyObject>()];
            let config: Retained<AnyObject> = msg_send![web, configuration];
            let content: Retained<AnyObject> = msg_send![&config, userContentController];
            let _: () = msg_send![
                &content,
                removeScriptMessageHandlerForName: &*NSString::from_str("nsh")
            ];
        }
    }
}

/// One view's rule set compiling: the lists compiled so far (retained
/// until the main queue installs them), how many are left, and why a list
/// failed.
struct PendingRules {
    target: dispatch2::MainThreadBound<(Weak<Surface>, NodeId)>,
    /// False once the view is dropped: its lists are discarded, never
    /// installed into a restarted engine's node.
    alive: Arc<AtomicBool>,
    generation: u64,
    remaining: Mutex<usize>,
    lists: Mutex<Vec<(Arc<str>, usize)>>,
    failed: Mutex<Option<String>>,
}

/// Drops the lists retained for the main queue (they were not installed).
fn drop_lists(lists: Vec<(Arc<str>, usize)>) {
    for (_, ptr) in lists {
        // SAFETY: a +1 retained WKContentRuleList from `compile_content_rules`.
        drop(unsafe { Retained::<AnyObject>::from_raw(ptr as *mut AnyObject) });
    }
}

/// Has WebKit compile the complete `rules` (identifier, JSON) for the web
/// view `node`. The lists installed keep applying while they compile;
/// when every one compiled, [`finish_rules`] replaces the whole set at
/// once and the source that waited loads. A list that fails fails the
/// set, leaving the installed lists alone.
fn compile_content_rules(
    surface: &Rc<Surface>,
    node: NodeId,
    generation: u64,
    rules: Vec<(Arc<str>, Arc<str>)>,
    alive: Arc<AtomicBool>,
) {
    if !alive.load(Ordering::Acquire) {
        return;
    }
    let current = matches!(
        surface.native.hosts.borrow().get(&node),
        Some(Host::Web(host)) if host.web.is_some() && host.rules_gen == generation
    );
    if !current || rules.is_empty() {
        return;
    }
    // SAFETY: WebKit's documented API (WKContentRuleListStore, macOS
    // 10.13+); a store is shared by the app's web views.
    unsafe {
        let mtm = MainThreadMarker::new().expect("main thread");
        let target = dispatch2::MainThreadBound::new((Rc::downgrade(surface), node), mtm);
        let store = class(c"WKContentRuleListStore").and_then(|store_class| {
            let store: Option<Retained<AnyObject>> = msg_send![store_class, defaultStore];
            store
        });
        let Some(store) = store else {
            // No store, no rules: report it and hold the page that waited.
            finish_rules(Arc::new(PendingRules {
                target,
                alive,
                generation,
                remaining: Mutex::new(0),
                lists: Mutex::new(Vec::new()),
                failed: Mutex::new(Some(
                    "this WebKit has no content rule list store".to_owned(),
                )),
            }));
            return;
        };
        let pending = Arc::new(PendingRules {
            target,
            alive,
            generation,
            remaining: Mutex::new(rules.len()),
            lists: Mutex::new(Vec::new()),
            failed: Mutex::new(None),
        });
        for (id, json) in rules {
            let pending = pending.clone();
            let log_id = id.clone();
            let list_id = id.clone();
            let done = RcBlock::new(move |compiled: *mut AnyObject, error: *mut AnyObject| {
                if compiled.is_null() || !error.is_null() {
                    let message = if error.is_null() {
                        format!("WebKit could not compile content rules {log_id}")
                    } else {
                        // SAFETY: WebKit passes an NSError on failure.
                        let described: Retained<NSString> = msg_send![error, localizedDescription];
                        format!("content rules {log_id}: {described}")
                    };
                    let mut failed = pending.failed.lock().unwrap_or_else(|e| e.into_inner());
                    if failed.is_none() {
                        *failed = Some(message);
                    }
                } else {
                    // SAFETY: a live list; retained for the main queue.
                    let retained: *mut AnyObject = msg_send![compiled, retain];
                    let mut lists = pending.lists.lock().unwrap_or_else(|e| e.into_inner());
                    lists.push((list_id.clone(), retained as usize));
                }
                let left = {
                    let mut remaining = pending.remaining.lock().unwrap_or_else(|e| e.into_inner());
                    *remaining = remaining.saturating_sub(1);
                    *remaining
                };
                if left == 0 {
                    finish_rules(pending.clone());
                }
            });
            let _: () = msg_send![
                &store,
                compileContentRuleListForIdentifier: &*NSString::from_str(&id),
                encodedContentRuleList: &*NSString::from_str(&json),
                completionHandler: &*done
            ];
        }
    }
}

/// Replaces the view's content rule lists with the complete set at once
/// (only when the generation still holds), then loads the source that
/// waited. A set that failed leaves the installed lists (and the page)
/// alone, keeps the source waiting and reports the error: only a complete
/// set, or blocking cleared, lets a page out.
fn finish_rules(pending: Arc<PendingRules>) {
    dispatch2::DispatchQueue::main().exec_async(move || {
        let mtm = MainThreadMarker::new().expect("main queue");
        let (surface, node) = pending.target.get(mtm);
        let lists = std::mem::take(&mut *pending.lists.lock().unwrap_or_else(|e| e.into_inner()));
        if !pending.alive.load(Ordering::Acquire) {
            // The view is gone: never install into whatever now holds its
            // node id.
            drop_lists(lists);
            return;
        }
        let failed = pending
            .failed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let Some(surface) = surface.upgrade() else {
            drop_lists(lists);
            return;
        };
        let mut hosts = surface.native.hosts.borrow_mut();
        let Some(Host::Web(host)) = hosts.get_mut(node) else {
            drop_lists(lists);
            return;
        };
        if host.rules_gen != pending.generation {
            // Other rules are wanted now: these are not installed.
            drop_lists(lists);
            return;
        }
        let mut failure = None;
        let mut resume = None;
        match failed {
            None => {
                let Some(web) = &host.web else {
                    drop_lists(lists);
                    return;
                };
                // SAFETY: WebKit's documented API, on the main thread.
                let content: Retained<AnyObject> = unsafe {
                    let config: Retained<AnyObject> = msg_send![&**web, configuration];
                    msg_send![&config, userContentController]
                };
                // SAFETY: WebKit's documented API, on the main thread.
                unsafe {
                    for (_, list) in host.rules_applied.drain(..) {
                        let _: () = msg_send![&content, removeContentRuleList: &*list];
                    }
                    for (id, ptr) in lists {
                        // SAFETY: the retain in `compile_content_rules`
                        // balances this.
                        let Some(list) = Retained::<AnyObject>::from_raw(ptr as *mut AnyObject)
                        else {
                            continue;
                        };
                        let _: () = msg_send![&content, addContentRuleList: &*list];
                        host.rules_applied.push((id, list));
                    }
                }
                // The complete set is in: the page that waited and the
                // navigations held meanwhile resume (after the hosts
                // borrow is let go).
                host.rules_ready.set(true);
                if let Some(web) = host.web.clone() {
                    let source = host.pending_source.take();
                    let queued = host.pending_nav.take();
                    resume = Some((web, host.handler.clone(), source, queued));
                }
            }
            Some(reason) => {
                drop_lists(lists);
                // The lists installed keep applying, and the waiting
                // source stays queued: a page must not go out before the
                // rules it asked for are in.
                failure = Some(reason);
            }
        }
        drop(hosts);
        if let Some((web, handler, source, queued)) = resume {
            apply_navigation(&web, &handler, source, queued);
        }
        if let Some(reason) = failure {
            surface.event(
                *node,
                EventKind::Error,
                error(format!("content rules: {reason}")),
            );
        }
    });
}

/// Loads `source` in `web` (the deferred page, or the first of the held
/// navigations).
fn load_source(web: &AnyObject, handler: &WebHandler, source: &WebSource) {
    // Only the file this load opens grants access to its containing folder.
    *handler.ivars().file.borrow_mut() = None;
    // SAFETY: WebKit's documented API.
    unsafe {
        match source {
            WebSource::Uri(uri) => {
                let url = NSURL::URLWithString(&NSString::from_str(uri));
                let dir = file_read_scope(uri);
                let scope = dir.and_then(|dir| {
                    NSURL::URLWithString(&NSString::from_str(&format!("file://{dir}")))
                });
                if let (Some(url), Some(scope), Some(dir)) = (&url, scope, dir) {
                    *handler.ivars().file.borrow_mut() = Some(dir.to_owned());
                    let _: Option<Retained<AnyObject>> = msg_send![
                        web,
                        loadFileURL: &**url,
                        allowingReadAccessToURL: &*scope
                    ];
                } else if let (Some(url), Some(cls)) = (url, class(c"NSURLRequest")) {
                    let request: Retained<AnyObject> = msg_send![cls, requestWithURL: &*url];
                    let _: Option<Retained<AnyObject>> = msg_send![web, loadRequest: &*request];
                }
            }
            WebSource::Html { html, base_url } => {
                let base = base_url
                    .as_deref()
                    .and_then(|b| NSURL::URLWithString(&NSString::from_str(b)));
                let _: Option<Retained<AnyObject>> = msg_send![
                    web,
                    loadHTMLString: &*NSString::from_str(html),
                    baseURL: base.as_deref()
                ];
            }
            WebSource::None => {}
        }
    }
}

/// Applies the page that waited for the rules, then the navigations the
/// app asked for meanwhile, in order. The caller has let the hosts borrow
/// go (WebKit may call back into the delegate from the loads).
fn apply_navigation(
    web: &AnyObject,
    handler: &WebHandler,
    source: Option<WebSource>,
    queued: Vec<Command>,
) {
    if let Some(source) = source {
        // A page comes: the last document's evaluations are aborted (a
        // `None` source only retires its file access).
        if source != WebSource::None {
            handler.ivars().evaluations.navigated();
        }
        load_source(web, handler, &source);
    }
    for command in &queued {
        // Navigations held for the rules navigate now, as the app asked:
        // `command` aborted the last document's evaluations for those it
        // ran at once, so the held ones do it here.
        if command_navigates(command) {
            handler.ivars().evaluations.navigated();
        }
        web_command(web, command);
    }
}

/// Why [`WebViewOperation::OpenDevtools`] cannot be done: WebKit exposes no
/// public way to open its Web Inspector from an app.
const OPEN_DEVTOOLS: &str = "WKWebView has no public API to open the Web Inspector; enable `inspectable` and open it from the web view's context menu (or the Develop menu)";

/// Handles a [`WebViewOperation`] on `web` and answers through
/// `EventKind::WebViewReply`.
fn web_request(
    surface: &Rc<Surface>,
    node: NodeId,
    web: &AnyObject,
    handler: &WebHandler,
    request: &WebViewRequest,
) {
    let alive = handler.ivars().alive.clone();
    match &request.operation {
        WebViewOperation::CapturePage => capture_page(surface, node, web, request.id, alive),
        WebViewOperation::GetCookies { uri } => {
            get_cookies(surface, node, web, request.id, uri.as_deref(), alive)
        }
        WebViewOperation::SetCookies { cookies } => {
            set_cookies(surface, node, web, request.id, cookies, alive)
        }
        WebViewOperation::OpenDevtools => {
            reply(surface, node, request.id, Err(OPEN_DEVTOOLS.into()))
        }
        WebViewOperation::SetInspectable { enabled } => {
            set_inspectable(surface, node, web, request.id, *enabled)
        }
        WebViewOperation::AcceptDownload {
            id,
            path,
            overwrite,
        } => match handler.accept_download(id, path, *overwrite) {
            Ok(accepted) => {
                // The request is settled before WebKit resumes: the app's
                // promise resolves with what it asked for, and the
                // `Download` lifecycle reports what follows.
                reply(surface, node, request.id, Ok(JsonValue::Null));
                handler.finish_accept(accepted);
            }
            Err(error) => reply(surface, node, request.id, Err(error)),
        },
        WebViewOperation::CancelDownload { id } => match handler.cancel_download(id) {
            // A pending decision writes nothing: releasing it stops the
            // download, then the request is settled.
            Ok(Cancelled::Pending(decide)) => {
                decide.call(None);
                reply(surface, node, request.id, Ok(JsonValue::Null));
            }
            // An accepted transfer: stop it, wait for WebKit to stop
            // writing, remove the staging file, then settle and report.
            Ok(Cancelled::Active {
                download,
                output,
                event,
            }) => handler.cancel_active(download, output, event, surface, node, request.id),
            Err(error) => reply(surface, node, request.id, Err(error)),
        },
        // A newer core's operation: answer it rather than leave it pending.
        _ => reply(
            surface,
            node,
            request.id,
            Err("this macOS backend does not know the web view operation".into()),
        ),
    }
}

/// The [`EventKind::WebViewReply`] envelope of request `request`:
/// `{id, ok: true, value}` or `{id, ok: false, error}`.
fn reply_value(request: u64, result: Result<JsonValue, String>) -> JsonValue {
    match result {
        Ok(value) => serde_json::json!({"id": request, "ok": true, "value": value}),
        Err(error) => serde_json::json!({"id": request, "ok": false, "error": error}),
    }
}

/// Answers request `request` of `node`: success carries `value` (null for
/// unit operations), failure `error`.
fn reply(surface: &Surface, node: NodeId, request: u64, result: Result<JsonValue, String>) {
    surface.event(
        node,
        EventKind::WebViewReply,
        EventPayload::Json(reply_value(request, result)),
    );
}

/// Runs a late reply only while its web view lives: a completion that
/// outlives its host must not reach a restarted engine's node with the
/// same id.
fn while_alive(alive: &Arc<AtomicBool>, reply: impl FnOnce()) {
    if alive.load(Ordering::Acquire) {
        reply();
    }
}

/// Snapshots the page to a base64 PNG (`WKWebView`'s
/// `takeSnapshotWithConfiguration:completionHandler:`).
fn capture_page(
    surface: &Rc<Surface>,
    node: NodeId,
    web: &AnyObject,
    request: u64,
    alive: Arc<AtomicBool>,
) {
    // SAFETY: WebKit's documented snapshot API (macOS 10.13+).
    let supported = unsafe {
        let responds: Bool = msg_send![
            web,
            respondsToSelector: sel!(takeSnapshotWithConfiguration:completionHandler:)
        ];
        responds.as_bool()
    };
    let Some(config_class) = class(c"WKSnapshotConfiguration") else {
        reply(
            surface,
            node,
            request,
            Err("this WebKit cannot snapshot web views".into()),
        );
        return;
    };
    if !supported {
        reply(
            surface,
            node,
            request,
            Err("this WebKit cannot snapshot web views".into()),
        );
        return;
    }
    // SAFETY: the class was found above.
    let configuration: Retained<AnyObject> = unsafe { msg_send![config_class, new] };
    let weak = Rc::downgrade(surface);
    let done = RcBlock::new(move |image: *mut AnyObject, error: *mut AnyObject| {
        let result = snapshot_png(image, error);
        while_alive(&alive, || {
            if let Some(surface) = weak.upgrade() {
                reply(&surface, node, request, result);
            }
        });
    });
    // SAFETY: WebKit's documented WKWebView API.
    let _: () = unsafe {
        msg_send![
            web,
            takeSnapshotWithConfiguration: &*configuration,
            completionHandler: &*done
        ]
    };
}

/// The base64 PNG of a snapshot (`image`, or why it failed).
fn snapshot_png(image: *mut AnyObject, error: *mut AnyObject) -> Result<JsonValue, String> {
    if !error.is_null() {
        // SAFETY: WebKit passes an NSError on failure.
        let message: Retained<NSString> = unsafe { msg_send![&*error, localizedDescription] };
        return Err(message.to_string());
    }
    // SAFETY: WebKit passes the snapshot's NSImage when there is no error.
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Err("the web view returned no snapshot".into());
    };
    // SAFETY: AppKit's documented NSImage / NSBitmapImageRep API.
    unsafe {
        let tiff: Option<Retained<NSData>> = msg_send![image, TIFFRepresentation];
        let Some(tiff) = tiff else {
            return Err("the snapshot has no image data".into());
        };
        let Some(rep_class) = class(c"NSBitmapImageRep") else {
            return Err("NSBitmapImageRep is unavailable".into());
        };
        let rep: Option<Retained<AnyObject>> = msg_send![rep_class, imageRepWithData: &*tiff];
        let Some(rep) = rep else {
            return Err("the snapshot cannot be decoded".into());
        };
        let Some(dictionary_class) = class(c"NSDictionary") else {
            return Err("NSDictionary is unavailable".into());
        };
        let properties: Retained<AnyObject> = msg_send![dictionary_class, dictionary];
        // NSBitmapImageFileTypePNG = 4.
        let png: Option<Retained<NSData>> =
            msg_send![&*rep, representationUsingType: 4usize, properties: &*properties];
        let Some(png) = png else {
            return Err("the snapshot cannot be encoded as PNG".into());
        };
        let base64: Retained<NSString> = msg_send![&*png, base64EncodedStringWithOptions: 0usize];
        Ok(JsonValue::String(base64.to_string()))
    }
}

/// The web view's cookie store (`WKHTTPCookieStore`, scoped to the view's
/// own `WKWebsiteDataStore`: the app's profile, or the incognito store of
/// this web view).
fn cookie_jar(web: &AnyObject) -> Option<Retained<AnyObject>> {
    // SAFETY: WebKit's documented WKWebView / WKWebsiteDataStore API.
    unsafe {
        let configuration: Option<Retained<AnyObject>> = msg_send![web, configuration];
        let store: Option<Retained<AnyObject>> = msg_send![&*configuration?, websiteDataStore];
        let jar: Option<Retained<AnyObject>> = msg_send![&*store?, httpCookieStore];
        jar
    }
}

/// The URL a cookie must apply to when the app asks for one URI's cookies.
struct CookieScope {
    host: String,
    path: String,
    secure: bool,
}

impl CookieScope {
    fn new(uri: &str) -> Result<Self, String> {
        let url = NSURL::URLWithString(&NSString::from_str(uri))
            .ok_or_else(|| format!("`{uri}` is not a URL"))?;
        let host = url
            .host()
            .map(|host| host.to_string())
            .ok_or_else(|| format!("`{uri}` has no host"))?;
        let path = url
            .path()
            .map(|path| path.to_string())
            .unwrap_or_else(|| "/".into());
        let secure = url
            .scheme()
            .is_some_and(|scheme| scheme.to_string().eq_ignore_ascii_case("https"));
        Ok(CookieScope { host, path, secure })
    }

    fn matches(&self, cookie: &WebCookie) -> bool {
        if cookie.secure && !self.secure {
            return false;
        }
        let domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
        let host = self.host.to_ascii_lowercase();
        let domain_matches = !domain.is_empty()
            && (host == domain
                || (cookie.domain.starts_with('.') && host.ends_with(&format!(".{domain}"))));
        let cookie_path = if cookie.path.is_empty() {
            "/"
        } else {
            cookie.path.as_str()
        };
        let path_matches = self.path == cookie_path
            || (self.path.starts_with(cookie_path)
                && (cookie_path.ends_with('/') || self.path[cookie_path.len()..].starts_with('/')));
        domain_matches && path_matches
    }
}

/// Reads the profile's cookies, optionally restricted to those applying to `uri`.
fn get_cookies(
    surface: &Rc<Surface>,
    node: NodeId,
    web: &AnyObject,
    request: u64,
    uri: Option<&str>,
    alive: Arc<AtomicBool>,
) {
    let Some(jar) = cookie_jar(web) else {
        reply(
            surface,
            node,
            request,
            Err("this web view has no cookie store".into()),
        );
        return;
    };
    let scope = match uri {
        Some(uri) => match CookieScope::new(uri) {
            Ok(scope) => Some(scope),
            Err(error) => {
                reply(surface, node, request, Err(error));
                return;
            }
        },
        None => None,
    };
    let weak = Rc::downgrade(surface);
    let done = RcBlock::new(move |cookies: *mut NSArray<NSHTTPCookie>| {
        // SAFETY: WebKit passes the array of the store's cookies.
        let cookies = unsafe { cookies.as_ref() }
            .map(|cookies| cookies.to_vec())
            .unwrap_or_default();
        let list: Vec<WebCookie> = cookies
            .iter()
            .map(|cookie| web_cookie(cookie))
            .filter(|cookie| scope.as_ref().is_none_or(|scope| scope.matches(cookie)))
            .collect();
        let result = serde_json::to_value(list).map_err(|error| error.to_string());
        while_alive(&alive, || {
            if let Some(surface) = weak.upgrade() {
                reply(&surface, node, request, result);
            }
        });
    });
    // SAFETY: WebKit's documented WKHTTPCookieStore API.
    let _: () = unsafe { msg_send![&jar, getAllCookies: &*done] };
}

/// A cookie as core's [`WebCookie`] (`expires` in Unix milliseconds).
fn web_cookie(cookie: &NSHTTPCookie) -> WebCookie {
    let mut out = WebCookie::default();
    out.name = cookie.name().to_string();
    out.value = cookie.value().to_string();
    out.domain = cookie.domain().to_string();
    out.path = cookie.path().to_string();
    out.expires = cookie
        .expiresDate()
        .map(|date| (date.timeIntervalSince1970() * 1000.0).round() as i64);
    out.http_only = cookie.isHTTPOnly();
    out.secure = cookie.isSecure();
    out.same_site = cookie
        .sameSitePolicy()
        .and_then(|policy| CookieSameSite::parse(&policy.to_string()));
    out
}

/// A `Set-Cookie` header field for `cookie`, or why it cannot be written.
///
/// The header is synthesized (rather than `NSHTTPCookie`'s property
/// dictionary) because that is the only public way to keep `HttpOnly` and
/// `SameSite=None`; every field is validated so a value or attribute
/// cannot inject a second cookie or attribute (no CR/LF, `;`, `,` or
/// invalid name).
fn set_cookie_header(cookie: &WebCookie) -> Result<String, String> {
    let name = &cookie.name;
    if name.is_empty() || !name.bytes().all(is_token_byte) {
        return Err(format!("cookie name `{name}` is not a valid cookie name"));
    }
    if cookie
        .value
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | ',' | ';' | '\\'))
    {
        return Err(format!(
            "cookie `{name}` has a value that cannot be sent in a `Set-Cookie` header"
        ));
    }
    let domain = &cookie.domain;
    let host = domain.strip_prefix('.').unwrap_or(domain.as_str());
    // A hostname (or an IP literal): nothing that could end the header
    // field or start a quoted string.
    if host.is_empty()
        || host.chars().any(|c| {
            !(c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | ':' | '[' | ']' | '%'))
        })
    {
        return Err(format!("cookie `{name}` has an invalid domain `{domain}`"));
    }
    if cookie.path.is_empty() || !cookie.path.starts_with('/') {
        return Err(format!(
            "cookie `{name}` has no absolute path (`{}`)",
            cookie.path
        ));
    }
    if cookie
        .path
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | ';' | ',' | '\\'))
    {
        return Err(format!(
            "cookie `{name}` has a path that cannot be sent in a `Set-Cookie` header"
        ));
    }
    if cookie.same_site == Some(CookieSameSite::None) && !cookie.secure {
        return Err(format!(
            "cookie `{name}` asks for SameSite=None without Secure, which browsers do not send"
        ));
    }

    let mut header = format!("{name}={}", cookie.value);
    // A leading dot is how Foundation spells a cookie for subdomains; without
    // one the cookie is host-only, which the URL it is parsed against
    // supplies.
    if domain.starts_with('.') {
        header.push_str("; Domain=");
        header.push_str(domain);
    }
    header.push_str("; Path=");
    header.push_str(&cookie.path);
    if let Some(millis) = cookie.expires {
        // The header has one-second resolution; the platform reports and
        // verifies milliseconds.
        let seconds = millis as f64 / 1000.0;
        let date = http_date(seconds).ok_or_else(|| {
            format!("cookie `{name}` has an expiry date outside the representable range")
        })?;
        header.push_str("; Expires=");
        header.push_str(&date);
    }
    if cookie.secure {
        header.push_str("; Secure");
    }
    if cookie.http_only {
        header.push_str("; HttpOnly");
    }
    if let Some(policy) = cookie.same_site {
        header.push_str("; SameSite=");
        header.push_str(policy.as_str());
    }
    // Every field was validated above; this is the last line of defence
    // against a header that ends early or carries a second one.
    if header.contains('\r') || header.contains('\n') {
        return Err(format!(
            "cookie `{name}` cannot be sent in a `Set-Cookie` header"
        ));
    }
    Ok(header)
}

/// `Sun, 06 Nov 1994 08:49:37 GMT` (RFC 1123) for `seconds` since the Unix
/// epoch; `None` outside years 1000–9999.
fn http_date(seconds: f64) -> Option<String> {
    if !seconds.is_finite() {
        return None;
    }
    let seconds = seconds.floor();
    // A generous bound (about ±6 million years) keeps the day arithmetic
    // below from overflowing; the four-digit year check rejects the rest.
    if !(-2.0e14..=2.0e14).contains(&seconds) {
        return None;
    }
    let seconds = seconds as i64;
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (rest / 3_600, rest % 3_600 / 60, rest % 60);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + (if month <= 2 { 1 } else { 0 });
    if !(1000..=9999).contains(&year) {
        return None;
    }
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let weekday = WEEKDAYS[days.rem_euclid(7) as usize];
    Some(format!(
        "{weekday}, {day:02} {} {year:04} {hour:02}:{minute:02}:{second:02} GMT",
        MONTHS[(month - 1) as usize]
    ))
}

/// RFC 6265's `token`: what a cookie name may contain.
fn is_token_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
    ) || byte.is_ascii_alphanumeric()
}

/// The `same_site` a parsed cookie reports (`None` without the attribute).
fn same_site_of(cookie: &NSHTTPCookie) -> Option<CookieSameSite> {
    cookie
        .sameSitePolicy()
        .and_then(|policy| CookieSameSite::parse(&policy.to_string()))
}

/// Checks that `parsed` — what Foundation made of the synthesized
/// `Set-Cookie` header — carries every attribute `cookie` asked for. Making a
/// cookie that loses one (a missing `HttpOnly`, a weaker `SameSite`, another
/// expiry) would hand the app a false assurance, so that is an error.
fn verify_cookie(parsed: &NSHTTPCookie, cookie: &WebCookie) -> Result<(), String> {
    let name = &cookie.name;
    let mismatch = |attribute: &str| -> Result<(), String> {
        Err(format!(
            "macOS does not keep the `{attribute}` attribute of cookie `{name}`"
        ))
    };
    if parsed.name().to_string() != name.as_str()
        || parsed.value().to_string() != cookie.value.as_str()
    {
        return mismatch("name or value");
    }
    if !parsed
        .domain()
        .to_string()
        .trim_start_matches('.')
        .eq_ignore_ascii_case(cookie.domain.trim_start_matches('.'))
    {
        return mismatch("domain");
    }
    if parsed.path().to_string() != cookie.path.as_str() {
        return mismatch("path");
    }
    // The header carries seconds, so a millisecond expiry is kept to the
    // second Foundation parsed it back with.
    let expected_expiry = cookie.expires.map(|millis| millis.div_euclid(1000));
    let parsed_expiry = parsed
        .expiresDate()
        .map(|date| date.timeIntervalSince1970().round() as i64);
    if parsed_expiry != expected_expiry {
        return mismatch("expires");
    }
    if parsed.isSecure() != cookie.secure {
        return mismatch("secure");
    }
    if parsed.isHTTPOnly() != cookie.http_only {
        return mismatch("http_only");
    }
    if same_site_of(parsed) != cookie.same_site {
        return mismatch("same_site");
    }
    Ok(())
}

/// An `NSHTTPCookie` for `cookie`, made by parsing a synthesized
/// `Set-Cookie` header (the only public way to keep `HttpOnly` and
/// `SameSite=None`), verified attribute for attribute.
fn http_cookie(cookie: &WebCookie) -> Result<Retained<NSHTTPCookie>, String> {
    let name = &cookie.name;
    let header = set_cookie_header(cookie)?;
    let scheme = if cookie.secure { "https" } else { "http" };
    let host = cookie
        .domain
        .strip_prefix('.')
        .unwrap_or(cookie.domain.as_str());
    // The explicit `Path` attribute sets the cookie's path; the URL only
    // supplies the origin (host-only cookies take its host).
    let url = NSURL::URLWithString(&NSString::from_str(&format!("{scheme}://{host}/")))
        .ok_or_else(|| format!("cookie `{name}` has an invalid domain `{}`", cookie.domain))?;
    let Some(cookie_class) = class(c"NSHTTPCookie") else {
        return Err("NSHTTPCookie is unavailable".into());
    };
    let Some(dictionary_class) = class(c"NSMutableDictionary") else {
        return Err("NSMutableDictionary is unavailable".into());
    };
    // SAFETY: Foundation's documented `cookiesWithResponseHeaderFields:`
    // parser; the header was validated to hold exactly one cookie.
    let parsed = unsafe {
        let fields: Retained<AnyObject> = msg_send![dictionary_class, dictionary];
        let key = NSString::from_str("Set-Cookie");
        let value = NSString::from_str(&header);
        let _: () = msg_send![&fields, setObject: &*value, forKey: &*key];
        let cookies: Option<Retained<NSArray<NSHTTPCookie>>> = msg_send![
            cookie_class,
            cookiesWithResponseHeaderFields: &*fields,
            forURL: &*url
        ];
        cookies
    };
    let Some(cookies) = parsed else {
        return Err(format!(
            "macOS cannot make cookie `{name}` from its `Set-Cookie` header"
        ));
    };
    let mut cookies = cookies.to_vec();
    if cookies.len() != 1 {
        return Err(format!(
            "cookie `{name}` made {} cookies instead of one",
            cookies.len()
        ));
    }
    let parsed = cookies.remove(0);
    verify_cookie(&parsed, cookie)?;
    Ok(parsed)
}

/// Writes `cookies` to the profile's cookie store (`setCookie:`), answering
/// once all are set.
fn set_cookies(
    surface: &Rc<Surface>,
    node: NodeId,
    web: &AnyObject,
    request: u64,
    cookies: &[WebCookie],
    alive: Arc<AtomicBool>,
) {
    let Some(jar) = cookie_jar(web) else {
        reply(
            surface,
            node,
            request,
            Err("this web view has no cookie store".into()),
        );
        return;
    };
    let mut made = Vec::with_capacity(cookies.len());
    for cookie in cookies {
        match http_cookie(cookie) {
            Ok(cookie) => made.push(cookie),
            Err(error) => {
                // Nothing is set: a partial import would leave the profile
                // with some attributes masked and others not.
                reply(surface, node, request, Err(error));
                return;
            }
        }
    }
    if made.is_empty() {
        reply(surface, node, request, Ok(JsonValue::Null));
        return;
    }
    let pending = Rc::new(Cell::new(made.len()));
    let weak = Rc::downgrade(surface);
    for cookie in made {
        let pending = pending.clone();
        let weak = weak.clone();
        let alive = alive.clone();
        let done = RcBlock::new(move || {
            let left = pending.get().saturating_sub(1);
            pending.set(left);
            if left == 0 {
                while_alive(&alive, || {
                    if let Some(surface) = weak.upgrade() {
                        reply(&surface, node, request, Ok(JsonValue::Null));
                    }
                });
            }
        });
        // SAFETY: WebKit's documented WKHTTPCookieStore API.
        let _: () = unsafe { msg_send![&jar, setCookie: &*cookie, completionHandler: &*done] };
    }
}

/// `WKWebView.inspectable` (macOS 13.3+): lets the app open the Web
/// Inspector from the view's context menu. WebKit exposes no public way to
/// open it for the app, so [`WebViewOperation::OpenDevtools`] fails.
fn set_inspectable(surface: &Surface, node: NodeId, web: &AnyObject, request: u64, enabled: bool) {
    // SAFETY: WKWebView's documented `setInspectable:` property.
    let supported = unsafe {
        let responds: Bool = msg_send![web, respondsToSelector: sel!(setInspectable:)];
        if responds.as_bool() {
            let _: () = msg_send![web, setInspectable: Bool::new(enabled)];
        }
        responds.as_bool()
    };
    if supported {
        reply(surface, node, request, Ok(JsonValue::Null));
    } else if enabled {
        reply(
            surface,
            node,
            request,
            Err("`inspectable` requires macOS 13.3 or later".into()),
        );
    } else {
        // Disabled is the only thing older WebKit can be.
        reply(surface, node, request, Ok(JsonValue::Null));
    }
}

/// `reload()`, `goBack()`, `goForward()`, `stopLoading()`,
/// `injectJavaScript()`, `postMessage()` and `load()`.
fn web_command(web: &AnyObject, command: &Command) {
    let script = match command {
        Command::InjectJavaScript(js) => Some(js.to_string()),
        Command::PostMessage(data) => Some(format!(
            "window.dispatchEvent(new MessageEvent('message', {{ data: {} }}));",
            js_string(data)
        )),
        _ => None,
    };
    // SAFETY: WebKit's documented API.
    unsafe {
        match (command, script) {
            (_, Some(js)) => {
                let _: () = msg_send![
                    web,
                    evaluateJavaScript: &*NSString::from_str(&js),
                    completionHandler: std::ptr::null::<AnyObject>()
                ];
            }
            (Command::Reload, _) => {
                let _: Option<Retained<AnyObject>> = msg_send![web, reload];
            }
            (Command::GoBack, _) => {
                let _: Option<Retained<AnyObject>> = msg_send![web, goBack];
            }
            (Command::GoForward, _) => {
                let _: Option<Retained<AnyObject>> = msg_send![web, goForward];
            }
            (Command::StopLoading, _) => {
                let _: () = msg_send![web, stopLoading];
            }
            (Command::Load(load), _) => web_load(web, load),
            _ => {}
        }
    }
}

/// `load()`: the page's request with the app's headers. Only this request
/// carries them; WebKit makes its subresources' and later navigations'
/// requests itself.
fn web_load(web: &AnyObject, load: &nsh_core::WebLoad) {
    let (Some(url), Some(cls)) = (
        NSURL::URLWithString(&NSString::from_str(&load.src)),
        class(c"NSMutableURLRequest"),
    ) else {
        return;
    };
    // SAFETY: Foundation's and WebKit's documented API.
    unsafe {
        let request: Retained<AnyObject> = msg_send![cls, requestWithURL: &*url];
        for (name, value) in &load.headers {
            let _: () = msg_send![
                &request,
                setValue: &*NSString::from_str(value),
                forHTTPHeaderField: &*NSString::from_str(name)
            ];
        }
        let _: Option<Retained<AnyObject>> = msg_send![web, loadRequest: &*request];
    }
}

/// `sendInput()`: events made as AppKit makes the user's and handed to the
/// web view, so the page sees them trusted, which injected scripts cannot do.
fn web_input(web: &AnyObject, input: &WebInput) {
    // SAFETY: AppKit's and WebKit's documented API, on the main thread.
    unsafe {
        let window: Option<Retained<NSWindow>> = msg_send![web, window];
        let Some(window) = window else { return };
        // Keys go to the first responder, which a click makes the web view
        // too: it takes the keyboard focus, unless it has it already.
        let first: Option<Retained<AnyObject>> = msg_send![&*window, firstResponder];
        if first.as_deref().is_none_or(|f| !std::ptr::eq(f, web)) {
            let _: Bool = msg_send![&*window, makeFirstResponder: web];
        }
        let time = objc2_foundation::NSProcessInfo::processInfo().systemUptime();
        match input {
            WebInput::Click { x, y } => {
                // CSS pixels are points scaled by the page's zoom and magnification.
                let zoom: f64 = msg_send![web, pageZoom];
                let magnification: f64 = msg_send![web, magnification];
                let scale = zoom * magnification;
                let point: NSPoint = msg_send![
                    web,
                    convertPoint: NSPoint::new(f64::from(x.0) * scale, f64::from(y.0) * scale),
                    toView: std::ptr::null::<AnyObject>()
                ];
                for (kind, pressure) in [
                    (NSEventType::LeftMouseDown, 1.0),
                    (NSEventType::LeftMouseUp, 0.0),
                ] {
                    let Some(event) = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                        kind,
                        point,
                        NSEventModifierFlags::empty(),
                        time,
                        window.windowNumber(),
                        None,
                        0,
                        1,
                        pressure,
                    ) else {
                        continue;
                    };
                    if kind == NSEventType::LeftMouseDown {
                        let _: () = msg_send![web, mouseDown: &*event];
                    } else {
                        let _: () = msg_send![web, mouseUp: &*event];
                    }
                }
            }
            WebInput::Text(text) => {
                // The US layout's key (and Shift) for each character, so the
                // page's `KeyboardEvent.code` names the key a user would press;
                // a line break is Return, and what no US key types (`é`, CJK,
                // emoji) no key at all (`code` "Unidentified").
                for c in text.chars() {
                    let c = if c == '\n' { '\r' } else { c };
                    let (code, shift) =
                        crate::input::us_typing_key(c).unwrap_or((crate::input::NO_KEY, false));
                    // What the key types without Shift (`a` for `A`), as AppKit's own.
                    let plain = crate::input::us_key(code).unwrap_or(c);
                    key(
                        web,
                        &window,
                        time,
                        &c.to_string(),
                        &plain.to_string(),
                        code,
                        shift,
                    );
                }
            }
            WebInput::Key(k) => {
                let (chars, code) = match k {
                    WebKey::Enter => ("\r", 36),
                    WebKey::Tab => ("\t", 48),
                    WebKey::Escape => ("\u{1b}", 53),
                    WebKey::Backspace => ("\u{7f}", 51),
                    _ => return,
                };
                key(web, &window, time, chars, chars, code, false);
            }
            _ => {}
        }
    }
}

/// A key's down and up, with `chars` typed (`plain` without modifiers),
/// handed to the web view.
fn key(
    web: &AnyObject,
    window: &NSWindow,
    time: f64,
    chars: &str,
    plain: &str,
    code: u16,
    shift: bool,
) {
    let s = NSString::from_str(chars);
    let plain = NSString::from_str(plain);
    for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            kind,
            NSPoint::new(0.0, 0.0),
            if shift { NSEventModifierFlags::Shift } else { NSEventModifierFlags::empty() },
            time,
            window.windowNumber(),
            None,
            &s,
            &plain,
            false,
            code,
        );
        let Some(event) = event else { continue };
        // SAFETY: NSResponder's documented API, on the main thread.
        unsafe {
            if kind == NSEventType::KeyDown {
                let _: () = msg_send![web, keyDown: &*event];
            } else {
                let _: () = msg_send![web, keyUp: &*event];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_get_safe_file_names() {
        assert!(is_attachment("attachment; filename=\"report.pdf\""));
        assert!(is_attachment("  Attachment "));
        assert!(!is_attachment("inline; filename=attachment.pdf"));
        assert_eq!(file_name("report.pdf"), "report.pdf");
        assert_eq!(file_name("../../etc/passwd"), "passwd");
        assert_eq!(file_name("C:\\x\\y.csv"), "y.csv");
        assert_eq!(file_name(".hidden"), "hidden");
        assert_eq!(file_name("a:b\u{7}.txt"), "ab.txt");
        assert_eq!(file_name(""), "download");
        assert_eq!(file_name(".."), "download");
        assert_eq!(file_name(&"x".repeat(300)).len(), 200);
        assert_eq!(file_name("invoice\u{202e}fdp.exe"), "invoicefdp.exe");
        assert_eq!(
            file_name(
                "\u{061c}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{2066}\u{2067}\u{2068}\u{2069}report.pdf"
            ),
            "report.pdf"
        );
        let long = file_name(&format!("{}.pdf", "文".repeat(100)));
        assert!(long.len() <= 200);
        assert!(long.ends_with(".pdf"));
        let long_extension = file_name(&format!("文.{}", "x".repeat(198)));
        assert!(long_extension.len() <= 200);
        assert!(!long_extension.starts_with('.'));
        assert_eq!(file_name(&format!("{}.txt", "\u{7}".repeat(300))), "txt");
        assert_eq!(
            url_file_name("https://example.com/files/report.pdf?v=2#p1"),
            "report.pdf"
        );
        assert_eq!(file_name(url_file_name("https://example.com/")), "download");
    }

    #[test]
    fn locates_sources() {
        let dir = AssetBase::Directory("/app/assets".into());
        let http = AssetBase::Http("http://10.0.0.2:8790/assets/".into());
        assert_eq!(
            locate("asset:/clips/a.mp4", Some(&dir)),
            Some(Location::Path("/app/assets/clips/a.mp4".into()))
        );
        assert_eq!(
            locate("asset:/a.mp4", Some(&http)),
            Some(Location::Uri("http://10.0.0.2:8790/assets/a.mp4".into()))
        );
        assert_eq!(locate("asset:/a.mp4", None), None);
        assert_eq!(
            locate("https://example.com/a.mp4", None),
            Some(Location::Uri("https://example.com/a.mp4".into()))
        );
        assert_eq!(
            locate("/Users/me/a.mp4", None),
            Some(Location::Path("/Users/me/a.mp4".into()))
        );
        assert_eq!(locate("", None), None);
    }

    #[test]
    fn a_file_url_reads_its_own_folder_and_nothing_wider() {
        assert_eq!(
            file_read_scope("file:///tmp/opened/7f/Invoice%20May.pdf"),
            Some("/tmp/opened/7f/")
        );
        assert_eq!(
            file_read_scope("FILE:///tmp/x/a.pdf#page=2"),
            Some("/tmp/x/")
        );
        assert_eq!(
            file_read_scope("file://localhost/tmp/x/a.pdf"),
            Some("/tmp/x/")
        );
        assert_eq!(
            file_read_scope("file://LOCALHOST/tmp/x/a.pdf"),
            Some("/tmp/x/")
        );
        assert_eq!(
            file_read_scope("file:///tmp/x/a.pdf?/b/c"),
            Some("/tmp/x/"),
            "a slash after the path is no folder"
        );
        assert_eq!(
            file_read_scope("file:///tmp/x/..a/b..pdf"),
            Some("/tmp/x/..a/"),
            "dots in a name are a name"
        );
        assert_eq!(file_read_scope("file:///tmp/x/a%20.pdf"), Some("/tmp/x/"));
        for uri in [
            "file:///a.pdf",
            "file://localhost/a.pdf",
            "file://localhost/",
            "file://server/share/a.pdf",
            "file://localhost",
            "file:///tmp/x/../../a.pdf",
            "file:///tmp/x/./a.pdf",
            "file:///tmp/x/%2e%2e/%2E%2E/a.pdf",
            "file:///tmp/x/.%2e/a.pdf",
            "file:///tmp/x/..%2f..%2fetc/a.pdf",
            "file:///tmp/x/..%2F/a.pdf",
            "file://localhost/tmp/../a.pdf",
            "file:///tmp/../a.pdf#page=2",
            "http://127.0.0.1:8790/file/raw/m1/a.pdf",
            "https://example.com/a.pdf",
            "about:blank",
            "/tmp/b.pdf",
            "",
        ] {
            assert_eq!(file_read_scope(uri), None, "{uri}");
        }
    }

    #[test]
    fn a_web_view_opens_only_files_in_the_folder_it_was_given() {
        let dir = "/tmp/opened/7f/";
        let native = Policy::default().access();
        let go =
            |loaded: Option<&str>, url: &str| navigation_allowed(Some(&native), loaded, true, url);
        for url in [
            "file:///tmp/opened/7f/a.pdf",
            "file:///tmp/opened/7f/a.pdf#page=2",
            "FILE:///tmp/opened/7f/a.pdf",
            "file://localhost/tmp/opened/7f/a.pdf",
            "file:///tmp/opened/7f/b.html",
            "file:///tmp/opened/7f/sub/c.html?x=1",
        ] {
            assert!(go(Some(dir), url), "same folder: {url}");
        }
        for url in [
            "file:///tmp/opened/a.pdf",
            "file:///etc/passwd",
            "file://localhost/etc/passwd",
            "file://server/tmp/opened/7f/a.pdf",
            "file:///tmp/opened/7f/../x.pdf",
            "file:///tmp/opened/7f/%2e%2e/x.pdf",
            "file:///tmp/opened/7fx/a.pdf",
        ] {
            assert!(!go(Some(dir), url), "outside the folder: {url}");
        }
        assert!(
            !go(None, "file:///tmp/opened/7f/a.pdf"),
            "a web view that loaded no file (the mail reader)"
        );
        assert!(
            !navigation_allowed(None, None, true, "file:///tmp/a.pdf"),
            "nothing known of the app"
        );
        // Not a file: the grant decides, unrestricted or not.
        assert!(go(Some(dir), "https://example.com/a"));
        assert!(go(None, "about:blank"));
        let granted = nsh_core::access::Capabilities::from_json(
            &serde_json::json!({ "webview": { "hosts": ["example.com"] } }),
        )
        .unwrap();
        let app = Access {
            declared: granted,
            granted: None,
        };
        for url in [
            "data:text/html,<p>x</p>",
            "buwiz://chat/new",
            "javascript:alert(1)",
        ] {
            assert!(!go(Some(dir), url), "not a web page: {url}");
            assert!(
                !navigation_allowed(Some(&app), Some(dir), true, url),
                "{url}"
            );
        }
        assert!(!navigation_allowed(
            Some(&app),
            Some(dir),
            true,
            "https://evil.example/a"
        ));
        assert!(
            navigation_allowed(Some(&app), Some(dir), true, "file:///tmp/opened/7f/a.pdf"),
            "a JS app's own file page"
        );
        assert!(!navigation_allowed(
            Some(&app),
            None,
            true,
            "file:///tmp/opened/7f/a.pdf"
        ));
        assert!(navigation_allowed(
            Some(&app),
            None,
            true,
            "https://example.com/a"
        ));
        assert!(
            navigation_allowed(Some(&app), None, false, "file:///etc/passwd"),
            "a frame inside the page"
        );
    }

    #[test]
    fn quotes_js_strings() {
        assert_eq!(js_string("a\"b\\c\nd"), r#""a\"b\\c\nd""#);
        assert_eq!(js_string("\u{1}"), r#""\u0001""#);
    }

    #[test]
    fn registers_components() {
        let components = Components::default();
        let factory: ComponentFactory = Rc::new(|_| unreachable!("not made in this test"));
        // Clones share the registry.
        components.clone().register("com.example.map", factory);
        assert!(components.factory(&"com.example.map".into()).is_some());
        assert!(components.factory(&"com.example.none".into()).is_none());
        assert_eq!(format!("{components:?}"), r#"{"com.example.map"}"#);
    }

    /// A cookie as the core hands one to `SetCookies`: Unix milliseconds,
    /// the new `CookieSameSite` enum.
    fn cookie() -> WebCookie {
        let mut cookie = WebCookie::default();
        cookie.name = "session".into();
        cookie.value = "abc123".into();
        cookie.domain = ".example.com".into();
        cookie.path = "/".into();
        cookie.expires = Some(784_111_777_000);
        cookie.http_only = true;
        cookie.secure = true;
        cookie.same_site = Some(CookieSameSite::None);
        cookie
    }

    #[test]
    fn cookie_url_filter_keeps_host_only_domain_and_path_boundaries() {
        let mut value = cookie();
        value.domain = "example.com".into();
        value.path = "/account".into();
        let mut scope = CookieScope {
            host: "EXAMPLE.COM".into(),
            path: "/account/profile".into(),
            secure: true,
        };
        assert!(scope.matches(&value));
        scope.host = "sub.example.com".into();
        assert!(!scope.matches(&value));
        value.domain = ".example.com".into();
        assert!(scope.matches(&value));
        scope.host = "notexample.com".into();
        assert!(!scope.matches(&value));
        scope.host = "example.com".into();
        scope.path = "/accounts".into();
        assert!(!scope.matches(&value));
        scope.path = "/account".into();
        assert!(scope.matches(&value));
        scope.secure = false;
        assert!(!scope.matches(&value));
    }

    #[test]
    fn writes_cookie_attributes_into_the_header() {
        let header = set_cookie_header(&cookie()).expect("a valid cookie");
        assert!(header.starts_with("session=abc123"));
        assert!(header.contains("; Domain=.example.com"));
        assert!(header.contains("; Path=/"));
        assert!(header.contains("; Expires=Sun, 06 Nov 1994 08:49:37 GMT"));
        assert!(header.contains("; Secure"));
        assert!(header.contains("; HttpOnly"));
        assert!(header.contains("; SameSite=none"));
    }

    #[test]
    fn translates_cookie_expiry_from_milliseconds() {
        let mut cookie = cookie();
        cookie.expires = Some(784_111_777_999);
        let header = set_cookie_header(&cookie).expect("a valid cookie");
        // The header carries seconds; WebKit keeps the second Foundation
        // parses back.
        assert!(header.contains("; Expires=Sun, 06 Nov 1994 08:49:37 GMT"));
        cookie.expires = None;
        assert!(!set_cookie_header(&cookie).unwrap().contains("; Expires="));
        cookie.expires = Some(i64::MAX);
        assert!(set_cookie_header(&cookie).is_err());
    }

    #[test]
    fn rejects_cookie_injection() {
        let mut value = cookie();
        value.value = "a; HttpOnly".into();
        assert!(set_cookie_header(&value).is_err());

        let mut name = cookie();
        name.name = "a=b".into();
        assert!(set_cookie_header(&name).is_err());

        let mut path = cookie();
        path.path = "/a\r\nSet-Cookie: x=y".into();
        assert!(set_cookie_header(&path).is_err());

        let mut domain = cookie();
        domain.domain = "example.com; Path=/evil".into();
        assert!(set_cookie_header(&domain).is_err());

        let mut insecure_none = cookie();
        insecure_none.secure = false;
        assert!(set_cookie_header(&insecure_none).is_err());
    }

    #[test]
    fn formats_http_dates() {
        assert_eq!(
            http_date(784_111_777.0).as_deref(),
            Some("Sun, 06 Nov 1994 08:49:37 GMT")
        );
        assert_eq!(
            http_date(0.0).as_deref(),
            Some("Thu, 01 Jan 1970 00:00:00 GMT")
        );
        assert!(http_date(f64::NAN).is_none());
        assert!(http_date(1.0e15).is_none());
    }

    #[test]
    fn answers_requests_in_the_reply_envelope() {
        assert_eq!(
            reply_value(7, Ok(JsonValue::Null)),
            serde_json::json!({"id": 7, "ok": true, "value": null})
        );
        assert_eq!(
            reply_value(8, Ok(JsonValue::String("png".into()))),
            serde_json::json!({"id": 8, "ok": true, "value": "png"})
        );
        assert_eq!(
            reply_value(9, Err("no download with id `x`".into())),
            serde_json::json!({"id": 9, "ok": false, "error": "no download with id `x`"})
        );
    }

    #[test]
    fn validates_manual_download_destinations() {
        let dir =
            std::env::temp_dir().join(format!("nsh-webview-downloads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let target = dir.join("report.pdf");
        // A new file is accepted as is.
        assert_eq!(
            download_destination_path(&target, false),
            Ok(target.clone())
        );
        // An existing file is refused (and left alone) unless the app said
        // overwrite; then it is cleared for WebKit, not written through.
        std::fs::write(&target, b"old").expect("an existing file");
        assert!(download_destination_path(&target, false).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"old".to_vec());
        assert_eq!(download_destination_path(&target, true), Ok(target.clone()));
        // Overwriting never touches the destination before the finished
        // download publishes: it stays byte-identical until then.
        assert_eq!(std::fs::read(&target).unwrap(), b"old".to_vec());
        // Relative paths, missing directories and directories are refused.
        assert!(download_destination_path(Path::new("relative.pdf"), true).is_err());
        assert!(download_destination_path(&dir.join("missing/report.pdf"), true).is_err());
        assert!(download_destination_path(&dir, true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removes_only_the_owned_output_never_the_destination() {
        let dir = std::env::temp_dir().join(format!("nsh-webview-output-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // A manual download owns its staging file, not the destination it
        // publishes at (which may hold a completed file by then).
        let folder = dir.join("chosen");
        std::fs::create_dir_all(&folder).expect("a chosen directory");
        let target = folder.join("report.pdf");
        std::fs::write(&target, b"completed").expect("a completed file");
        let stage = folder.join(".nsh-download-id.part");
        std::fs::write(&stage, b"partial").expect("a partial staging file");
        let output = output_for(true, Some(&stage), &target);
        assert_eq!(output, Output::File(stage.clone()));
        remove_output(output);
        assert!(!stage.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"completed".to_vec());
        assert!(folder.is_dir());

        // An automatic download owns the directory of its own.
        let own = dir.join("0f0e-uuid");
        std::fs::create_dir_all(&own).expect("a download's directory");
        std::fs::write(own.join("report.pdf"), b"partial").expect("a partial file");
        let output = output_for(false, None, &own.join("report.pdf"));
        assert_eq!(output, Output::Directory(own.clone()));
        remove_output(output);
        assert!(!own.exists());

        // Nothing was written before a destination was chosen.
        assert_eq!(output_for(true, None, Path::new("")), Output::None);
        assert_eq!(output_for(false, None, Path::new("")), Output::None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn publishes_manual_downloads_only_when_finished() {
        let dir = std::env::temp_dir().join(format!("nsh-webview-publish-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let target = dir.join("report.pdf");
        std::fs::write(&target, b"old").expect("an existing destination");
        let stage = download_stage_path(&target, "abc-123").expect("a staging slot");
        // The staging file is a unique sibling, never the destination.
        assert_eq!(stage.parent(), Some(dir.as_path()));
        assert_ne!(stage, target);

        // Overwrite: the finished download replaces the destination
        // atomically, and the staging name is gone.
        std::fs::write(&stage, b"new").expect("a staging file");
        publish_download(&stage, &target, true).expect("a replace");
        assert_eq!(std::fs::read(&target).unwrap(), b"new".to_vec());
        assert!(!stage.exists());

        // Create-new: a file that appeared at the destination meanwhile
        // makes publishing fail, and it stays byte-identical.
        std::fs::write(&target, b"later").expect("the destination again");
        std::fs::write(&stage, b"ours").expect("a staging file");
        assert!(publish_download(&stage, &target, false).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"later".to_vec());
        // Cleanup removes only the staging file.
        remove_output(Output::File(stage.clone()));
        assert!(!stage.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"later".to_vec());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn late_replies_stop_at_the_view_lifetime() {
        // A snapshot, cookie or cancel completion that outlives its web
        // view must not deliver to a restarted engine's node with the same
        // request id.
        let alive = Arc::new(AtomicBool::new(true));
        let mut delivered = 0;
        while_alive(&alive, || delivered += 1);
        assert_eq!(delivered, 1);
        alive.store(false, Ordering::Release);
        while_alive(&alive, || delivered += 1);
        assert_eq!(delivered, 1);
    }

    #[test]
    fn stages_never_at_the_destination_and_never_overwrite_a_stale_slot() {
        let dir = std::env::temp_dir().join(format!("nsh-webview-stage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");

        // The app chooses exactly the first candidate's name: the second
        // fixed sibling is used, so publishing is never `path` to itself.
        let target = dir.join(".nsh-download-id.part");
        let stage = download_stage_path(&target, "id").expect("a second sibling");
        assert_ne!(stage, target);
        assert_eq!(
            stage.file_name().unwrap().to_string_lossy(),
            ".nsh-download-id.part.tmp"
        );

        // These ASCII aliases coincide on default case-insensitive volumes.
        let alias = dir.join(".NSH-DOWNLOAD-case.part");
        let stage = download_stage_path(&alias, "case").expect("a distinct sibling");
        std::fs::write(&stage, b"complete").unwrap();
        assert!(!alias.exists());
        publish_download(&stage, &alias, false).unwrap();
        assert_eq!(std::fs::read(&alias).unwrap(), b"complete");
        assert!(!stage.exists());

        // An occupied candidate is refused, never overwritten.
        let occupied = dir.join(".nsh-download-other.part");
        std::fs::write(&occupied, b"keep").expect("an occupied slot");
        let other = dir.join("other.pdf");
        assert!(download_stage_path(&other, "other").is_err());
        assert_eq!(std::fs::read(&occupied).unwrap(), b"keep".to_vec());

        // A normal destination takes the first candidate.
        let normal = dir.join("normal.pdf");
        let stage = download_stage_path(&normal, "normal").expect("a staging slot");
        assert_eq!(
            stage.file_name().unwrap().to_string_lossy(),
            ".nsh-download-normal.part"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleanup_from_a_stored_stage_never_removes_a_chosen_directory() {
        let dir = std::env::temp_dir().join(format!("nsh-webview-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let folder = dir.join("chosen");
        std::fs::create_dir_all(&folder).expect("the app's directory");
        let target = folder.join("report.pdf");
        std::fs::write(&target, b"done").expect("the app's file");
        let sibling = folder.join("other.pdf");
        std::fs::write(&sibling, b"sibling").expect("a sibling file");
        let stage = folder.join(".nsh-download-id.part");
        std::fs::write(&stage, b"partial").expect("a staging file");

        // `manual` is false here (a behavior change between adopt and the
        // destination decision): the stored staging file still wins, and
        // the destination's directory (the app's) is never removed.
        let output = output_for(false, Some(&stage), &target);
        assert_eq!(output, Output::File(stage.clone()));
        remove_output(output);
        assert!(!stage.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"done".to_vec());
        assert_eq!(std::fs::read(&sibling).unwrap(), b"sibling".to_vec());

        // A manual download with no stage owns nothing: never infer a
        // directory from the destination.
        assert_eq!(output_for(true, None, &target), Output::None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn holds_app_navigation_until_requested_rules_install() {
        let mut gate = PendingNavigation::default();
        let load =
            Command::Load(nsh_core::WebLoad::new("http://127.0.0.1/after", Vec::new()).unwrap());
        // While the requested policy compiles (or its compile failed) no
        // navigation command is dispatched: they queue in order.
        assert!(gate.hold(false, &Command::Reload));
        assert!(gate.hold(false, &load));
        assert!(gate.hold(false, &Command::GoBack));
        assert!(!gate.hold(
            false,
            &Command::InjectJavaScript("location.href='/'".into())
        ));
        assert!(!gate.hold(false, &Command::WebViewRequest(Default::default())));
        // Page navigation is refused meanwhile, except for local empty
        // documents WebKit makes itself.
        assert!(policy_refuses(false, "https://example.com/"));
        assert!(!policy_refuses(false, ""));
        assert!(!policy_refuses(false, "about:blank"));
        // Ready (installed, retried, or cleared): navigation runs at once
        // and the queued commands replay in the order the app asked.
        assert!(!gate.hold(true, &Command::Reload));
        let queued = gate.take();
        assert!(matches!(
            queued.as_slice(),
            [Command::Reload, Command::Load(_), Command::GoBack]
        ));
        assert!(!policy_refuses(true, "https://example.com/"));
    }
}
