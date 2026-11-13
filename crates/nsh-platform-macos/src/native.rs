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
//!   fails; without a listener those navigations stay as they were. A web
//!   view with an `onDownloadPending` listener instead defers every
//!   download and reports it (`DownloadPending`) for the app to accept a
//!   destination or cancel: nothing is written until it accepts, and an
//!   accepted download writes an owned staging file beside the destination
//!   and publishes it only when it finishes (never replacing an existing
//!   file unless the app asked to overwrite it). A decision the app does
//!   not answer in time is cancelled. Dropping the host cancels pending
//!   decisions and active downloads and removes only what the platform was
//!   writing (a download's own directory, or a manual download's staging
//!   file). The app owns completed files' retention.
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
    ffi::CStr,
    path::{Path, PathBuf},
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use block2::RcBlock;
use nsh_core::{
    Bounds, Command, EventKind, EventPayload, JsonValue, NativeProps, NodeId, PayloadValue, Points,
    ResizeMode, Size, VideoProps, ViewKind, WebInput, WebKey, WebProps, WebSource,
    access::WebMedia,
    geometry::Point,
    webview::{WebViewOperation, WebViewRequest},
};
use nsh_engine::{Access, App, EngineControl, Policy};
use objc2::{
    DefinedClass, Encode, Encoding, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool, ProtocolObject},
};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWindow};
use objc2_core_foundation::CGSize;
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSOperationQueue, NSPoint,
    NSRect, NSString, NSURL,
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
    /// downloads there.
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
    Web(WebHost),
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
            Host::Web(WebHost::new(surface, view, access, downloads))
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
        Host::Web(WebHost { web: Some(web), .. }) => {
            // SAFETY: a WKWebView is an NSView.
            focused.isDescendantOf(unsafe { as_view(web) })
        }
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
    /// Where downloads are saved; `None`: no app directory, so nothing is
    /// downloaded.
    downloads: Option<PathBuf>,
    /// The downloads under way, by their `WKDownload`.
    active: RefCell<FxHashMap<usize, Download>>,
    /// The manual downloads waiting for the app's destination decision, by
    /// the id reported in `DownloadPending`.
    pending: RefCell<FxHashMap<String, PendingDownload>>,
    /// The view's lifetime: false once its host is dropped, so a late
    /// cancel completion cannot deliver a reply to a restarted engine's
    /// node with the same request id.
    alive: Arc<AtomicBool>,
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
    /// The app chose `path`: cleanup removes only `stage`, never a
    /// directory of the platform's own.
    manual: bool,
    /// Where WebKit writes a manual download: a unique sibling of `path`,
    /// so the destination is never touched before it finishes.
    stage: Option<PathBuf>,
    /// `overwrite`: publishing replaces a file that appeared at `path`;
    /// without it, publishing fails rather than replace one.
    overwrite: bool,
}

/// How long a manual download's destination decision waits for the app
/// before it is released (the download cancelled, the failure reported): a
/// decision the app never answers must not hold the page.
const DOWNLOAD_DECISION_TIMEOUT: Duration = Duration::from_secs(60);

/// A manual download waiting for the app to choose where it goes.
struct PendingDownload {
    /// The `WKDownload`, whose key finds its [`Download`] entry.
    download: Retained<AnyObject>,
    key: usize,
    /// WebKit's destination decision: called with the file URL to accept,
    /// or null to cancel. Nothing is written until it is called.
    decide: Decide,
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
            // listener change between the two callbacks must not turn an
            // automatic download's directory into a manual one's chosen
            // file (or the other way, whose cleanup would remove a
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
                    nsh_core::webview::download_pending_payload(&id, &url, &filename)
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
            let destination =
                NSURL::fileURLWithPath(&NSString::from_str(&entry.path.to_string_lossy()));
            let started = download_event("started", entry, None);
            drop(active);
            self.emit(EventKind::Download, || started);
            done.call((Retained::as_ptr(&destination) as *mut NSURL,));
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
                            let message =
                                format!("cannot publish `{}`: {error}", entry.path.display());
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
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn process_terminated(&self, _web: &AnyObject) {
            self.ivars().evaluations.process_terminated();
            // The app keeps running; the page comes back on its next reload.
            self.emit(EventKind::Error, || {
                EventPayload::Map(vec![
                    (
                        "message",
                        PayloadValue::Str("The web content process terminated".into()),
                    ),
                    (
                        "code",
                        PayloadValue::Str(nsh_core::element::codes::WEBVIEW_TERMINATED.into()),
                    ),
                ])
            });
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
/// A stored staging path wins over the `manual` flag: if the two ever
/// disagree (a listener change across callbacks), cleanup must not infer an
/// owned directory from the destination the app chose.
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
        alive: Arc<AtomicBool>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(WebIvars {
            emitter,
            access,
            downloads,
            active: RefCell::default(),
            pending: RefCell::default(),
            alive,
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

    /// Whether this web view turns downloads into files: an
    /// `onDownloadPending` listener (the app chooses every destination), or
    /// an `onDownload` listener with a data directory (the upstream
    /// automatic behavior). Without a listener the navigations stay as
    /// they were.
    fn wants_download(&self) -> bool {
        self.ivars().emitter.listens(EventKind::DownloadPending)
            || (self.ivars().downloads.is_some()
                && self.ivars().emitter.listens(EventKind::Download))
    }

    /// Becomes the delegate of `download` and tracks it from now on, so a
    /// download that fails before WebKit asks for its destination is
    /// still reported. Its name comes from the URL until the server's is
    /// known.
    fn adopt(&self, download: &AnyObject) {
        // SAFETY: WKDownload's `delegate` (weak); this handler outlives it
        // as the web view's delegate, and keeps it in `active` until it ends.
        let _: () = unsafe { msg_send![download, setDelegate: self] };
        // The choice is made when the download is adopted: a listener
        // registered or dropped between the two callbacks must not turn an
        // automatic download's directory into a manual one's chosen file
        // (whose cleanup would remove a directory the app owns), or the
        // other way.
        let manual = self.ivars().emitter.listens(EventKind::DownloadPending);
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
            return Err(format!("no pending download with id `{id}`"));
        }
        let path = download_destination_path(path, overwrite)?;
        // WebKit writes to the staging file, never the destination: the
        // finished download publishes it there. A refused staging slot
        // keeps the pending decision (the app may retry elsewhere).
        let stage = download_stage_path(&path, id)?;
        let Some(pending) = self.ivars().pending.borrow_mut().remove(id) else {
            return Err(format!("no pending download with id `{id}`"));
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
            return Err(format!("no active download with id `{id}`"));
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
    /// False once this host is dropped: an asynchronous cancel completion
    /// stops delivering a reply then (cleanup still runs).
    alive: Arc<AtomicBool>,
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
        let alive = Arc::new(AtomicBool::new(true));
        let handler = WebHandler::new(
            surface.mtm,
            Emitter::new(surface, view),
            access,
            downloads,
            evaluations,
            alive.clone(),
        );
        WebHost {
            holder: objc2::rc::Weak::new(&**view),
            web: None,
            handler,
            schemes: None,
            props: None,
            scrolls: true,
            alive,
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
        if self.props.is_none() {
            // The core keeps `incognito` and the URL schemes from changing
            // once it is made.
            self.make(p);
        }
        let Some(web) = &self.web else { return };
        if !p.javascript_enabled {
            self.handler.ivars().evaluations.scripts_disabled();
        }
        let old = self.props.take();
        let first = old.is_none();
        let old = old.unwrap_or_default();
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
            // Scripts apply to pages loaded from now on: load again.
            if first || p.source != old.source || scripts_changed {
                // Only the file this load opens is the page's own: none for
                // any other source.
                *self.handler.ivars().file.borrow_mut() = None;
                if p.source != WebSource::None {
                    self.handler.ivars().evaluations.navigated();
                }
                match &p.source {
                    WebSource::Uri(uri) => {
                        let url = NSURL::URLWithString(&NSString::from_str(uri));
                        let dir = file_read_scope(uri);
                        let scope = dir.and_then(|dir| {
                            NSURL::URLWithString(&NSString::from_str(&format!("file://{dir}")))
                        });
                        if let (Some(url), Some(scope), Some(dir)) = (&url, scope, dir) {
                            *self.handler.ivars().file.borrow_mut() = Some(dir.to_owned());
                            // A request for a file URL shows nothing: WebKit
                            // reads a local file only where it is let.
                            let _: Option<Retained<AnyObject>> = msg_send![
                                web,
                                loadFileURL: &**url,
                                allowingReadAccessToURL: &*scope
                            ];
                        } else if let (Some(url), Some(cls)) = (url, class(c"NSURLRequest")) {
                            let request: Retained<AnyObject> =
                                msg_send![cls, requestWithURL: &*url];
                            let _: Option<Retained<AnyObject>> =
                                msg_send![web, loadRequest: &*request];
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
        self.props = Some(p.clone());
    }
}

impl Drop for WebHost {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        self.handler.cancel_downloads();
        self.handler.ivars().evaluations.dispose();
        let Some(web) = &self.web else { return };
        // SAFETY: breaks the cycle web view → content controller → handler
        // and stops the page.
        unsafe {
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

/// Handles a [`WebViewOperation`] on `web` and answers through
/// `EventKind::WebViewReply`.
fn web_request(
    surface: &Rc<Surface>,
    node: NodeId,
    web: &AnyObject,
    handler: &WebHandler,
    request: &WebViewRequest,
) {
    let _ = web;
    match &request.operation {
        WebViewOperation::AcceptDownload { id, destination } => {
            match handler.accept_download(id, &destination.path, destination.overwrite) {
                Ok(accepted) => {
                    // The request is settled before WebKit resumes: the
                    // app's answer resolves with what it asked for, and the
                    // `Download` lifecycle reports what follows.
                    reply(surface, node, request.id, Ok(JsonValue::Null));
                    handler.finish_accept(accepted);
                }
                Err(error) => reply(surface, node, request.id, Err(error)),
            }
        }
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
    fn download_scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsh-webview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        dir
    }

    #[test]
    fn validates_manual_download_destinations() {
        let dir = download_scratch("destinations");
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
        assert_eq!(std::fs::read(&target).unwrap(), b"old".to_vec());
        // Relative paths, missing directories and directories are refused.
        assert!(download_destination_path(Path::new("relative.pdf"), true).is_err());
        assert!(download_destination_path(&dir.join("missing/report.pdf"), true).is_err());
        assert!(download_destination_path(&dir, true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removes_only_the_owned_output_never_the_destination() {
        let dir = download_scratch("output");
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
        let dir = download_scratch("publish");
        let target = dir.join("report.pdf");
        std::fs::write(&target, b"old").expect("an existing destination");
        let stage = download_stage_path(&target, "abc-123").expect("a staging slot");
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
        remove_output(Output::File(stage.clone()));
        assert!(!stage.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"later".to_vec());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stages_never_at_the_destination_and_never_overwrite_a_stale_slot() {
        let dir = download_scratch("stage");
        let target = dir.join(".nsh-download-id.part");
        let stage = download_stage_path(&target, "id").expect("a second sibling");
        assert_ne!(stage, target);
        assert_eq!(
            stage.file_name().unwrap().to_string_lossy(),
            ".nsh-download-id.part.tmp"
        );

        let alias = dir.join(".NSH-DOWNLOAD-case.part");
        let stage = download_stage_path(&alias, "case").expect("a distinct sibling");
        std::fs::write(&stage, b"complete").unwrap();
        assert!(!alias.exists());
        publish_download(&stage, &alias, false).unwrap();
        assert_eq!(std::fs::read(&alias).unwrap(), b"complete");
        assert!(!stage.exists());

        let occupied = dir.join(".nsh-download-other.part");
        std::fs::write(&occupied, b"keep").expect("an occupied slot");
        let other = dir.join("other.pdf");
        assert!(download_stage_path(&other, "other").is_err());
        assert_eq!(std::fs::read(&occupied).unwrap(), b"keep".to_vec());

        let normal = dir.join("normal.pdf");
        let stage = download_stage_path(&normal, "normal").expect("a staging slot");
        assert_eq!(
            stage.file_name().unwrap().to_string_lossy(),
            ".nsh-download-normal.part"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn late_replies_stop_at_the_view_lifetime() {
        let alive = Arc::new(AtomicBool::new(true));
        let mut delivered = 0;
        while_alive(&alive, || delivered += 1);
        assert_eq!(delivered, 1);
        alive.store(false, Ordering::Release);
        while_alive(&alive, || delivered += 1);
        assert_eq!(delivered, 1);
    }
}
