//! Scroll views the embedding app watches ([`crate::AppView::watch_scroll`]):
//! native UI of the host that moves with the app's content in every frame (a
//! sheet the end of the content pulls up) follows a `<Scroll>` by its
//! `testID`, on the main thread, in the frame of the scroll. `onScroll`
//! through JS would be a frame late.
//!
//! The mounter keys a scroll view by the `testID` of its props (the newest of
//! an id wins) and unkeys it when it is deleted, loses the id or its app
//! stops; unkeying tells the watcher and kills the handle. The event target
//! calls the handle's listeners from `scrollViewDidScroll`, after the core's
//! report, without borrowing the mounter (a listener may scroll, which calls
//! back in); the mounter calls them when a content height changes without a
//! scroll. Nothing runs for a scroll view no host watches.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use nsh_core::NodeId;
use objc2::rc::{Retained, Weak};
use objc2_core_foundation::{CGFloat, CGPoint};
use objc2_ui_kit::{UIScrollView, UIView};
use rustc_hash::FxHashMap;

use crate::target::EventTarget;

/// Hears a scroll view the host watches come and go, on the main thread.
pub trait ScrollWatcher {
    /// The scroll view is there: at once when it already is, and again with
    /// a new handle when the app replaces the element or another app loads.
    fn attached(&self, handle: ScrollHandle);

    /// The scroll view is gone (deleted, its `testID` changed or its app
    /// stopped): its handle is dead.
    fn detached(&self);
}

/// The watchers of a view's apps, by `testID`, and the scroll views keyed
/// now (shared by the view's options and its surface's mounter).
#[derive(Clone, Default)]
pub struct ScrollWatchers(Rc<RefCell<Registry>>);

impl std::fmt::Debug for ScrollWatchers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set()
            .entries(self.0.borrow().watchers.keys())
            .finish()
    }
}

#[derive(Default)]
struct Registry {
    watchers: FxHashMap<Arc<str>, Rc<dyn ScrollWatcher>>,
    keyed: FxHashMap<Arc<str>, ScrollHandle>,
}

impl ScrollWatchers {
    /// Watches the scroll view with `test_id` (`None`: stops watching).
    pub(crate) fn watch(&self, test_id: &str, watcher: Option<Rc<dyn ScrollWatcher>>) {
        let attach = {
            let mut registry = self.0.borrow_mut();
            match watcher {
                None => {
                    registry.watchers.remove(test_id);
                    None
                }
                Some(watcher) => {
                    registry.watchers.insert(test_id.into(), watcher.clone());
                    registry.keyed.get(test_id).cloned().map(|h| (watcher, h))
                }
            }
        };
        // Outside the borrow: the watcher may watch again.
        if let Some((watcher, handle)) = attach {
            watcher.attached(handle);
        }
    }

    /// `handle`'s scroll view now has `test_id`: an older one of the id
    /// makes way.
    fn key(&self, test_id: Arc<str>, handle: &ScrollHandle) {
        let (previous, watcher) = {
            let mut registry = self.0.borrow_mut();
            let previous = registry.keyed.insert(test_id.clone(), handle.clone());
            (previous, registry.watchers.get(&test_id).cloned())
        };
        if let Some(previous) = previous {
            previous.kill();
            if let Some(w) = &watcher {
                w.detached();
            }
        }
        if let Some(w) = watcher {
            w.attached(handle.clone());
        }
    }

    /// `handle`'s scroll view is gone or lost `test_id`.
    fn unkey(&self, test_id: &str, handle: &ScrollHandle) {
        handle.kill();
        let watcher = {
            let mut registry = self.0.borrow_mut();
            if !registry.keyed.get(test_id).is_some_and(|h| h.same(handle)) {
                return;
            }
            registry.keyed.remove(test_id);
            registry.watchers.get(test_id).cloned()
        };
        if let Some(w) = watcher {
            w.detached();
        }
    }
}

/// The keyed scroll views of a surface: their `testID` and handle, by node id.
type Keyed = FxHashMap<NodeId, (Arc<str>, ScrollHandle)>;

/// The scroll views of one surface the host may watch, by node id (its
/// mounter keys them, its event target reports their scrolls).
#[derive(Clone, Default)]
pub(crate) struct SurfaceScrolls(Rc<RefCell<Keyed>>);

impl SurfaceScrolls {
    /// Scroll view `id`'s props were applied, with `test_id`.
    pub(crate) fn props(
        &self,
        watchers: &ScrollWatchers,
        id: NodeId,
        scroll: &Retained<UIScrollView>,
        target: &Retained<EventTarget>,
        test_id: Option<Arc<str>>,
    ) {
        let current = self.0.borrow().get(&id).map(|(k, _)| k.clone());
        if current == test_id {
            return;
        }
        self.remove(watchers, id);
        let Some(test_id) = test_id else { return };
        let handle = ScrollHandle::new(scroll, target);
        self.0
            .borrow_mut()
            .insert(id, (test_id.clone(), handle.clone()));
        watchers.key(test_id, &handle);
    }

    /// Scroll view `id` is deleted, or lost its id.
    pub(crate) fn remove(&self, watchers: &ScrollWatchers, id: NodeId) {
        let removed = self.0.borrow_mut().remove(&id);
        if let Some((test_id, handle)) = removed {
            watchers.unkey(&test_id, &handle);
        }
    }

    /// The app stopped: every keyed scroll view goes.
    pub(crate) fn clear(&self, watchers: &ScrollWatchers) {
        let all: Vec<_> = self.0.borrow_mut().drain().collect();
        for (_, (test_id, handle)) in all {
            watchers.unkey(&test_id, &handle);
        }
    }

    fn handle(&self, id: NodeId) -> Option<ScrollHandle> {
        self.0.borrow().get(&id).map(|(_, h)| h.clone())
    }

    /// Scroll view `id` scrolled (`scrollViewDidScroll`).
    pub(crate) fn scrolled(&self, id: NodeId) {
        if let Some(handle) = self.handle(id) {
            handle.scrolled();
        }
    }

    /// Scroll view `id`'s content got a new size.
    pub(crate) fn resized(&self, id: NodeId) {
        if let Some(handle) = self.handle(id) {
            handle.resized();
        }
    }
}

/// A listener of a [`ScrollHandle`].
enum Listener {
    Scroll(Rc<dyn Fn(CGFloat, CGFloat)>),
    ContentSize(Rc<dyn Fn(CGFloat)>),
}

struct HandleState {
    scroll: Weak<UIScrollView>,
    target: Weak<EventTarget>,
    alive: Cell<bool>,
    /// The offset and content height last reported.
    offset: Cell<CGFloat>,
    height: Cell<CGFloat>,
    listeners: RefCell<Vec<(u64, Listener)>>,
    next: Cell<u64>,
}

/// A `<Scroll>` of the app ([`crate::AppView::watch_scroll`]), for native UI
/// of the host that follows or drives the app's vertical scrolling in the
/// frame of the scroll, without JS. Lengths are points, offsets from the top
/// of the content (the scroll view's top inset is not counted). Main
/// thread.
///
/// Once the scroll view is deleted, loses its `testID` or its app stops,
/// [`ScrollHandle::is_alive`] is false, reads give 0 and the rest does
/// nothing.
#[derive(Clone)]
pub struct ScrollHandle(Rc<HandleState>);

impl std::fmt::Debug for ScrollHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollHandle")
            .field("alive", &self.is_alive())
            .finish()
    }
}

impl ScrollHandle {
    fn new(scroll: &Retained<UIScrollView>, target: &Retained<EventTarget>) -> Self {
        let handle = ScrollHandle(Rc::new(HandleState {
            scroll: Weak::from_retained(scroll),
            target: Weak::from_retained(target),
            alive: Cell::new(true),
            offset: Cell::new(0.0),
            height: Cell::new(0.0),
            listeners: RefCell::default(),
            next: Cell::new(1),
        }));
        handle.0.offset.set(handle.content_offset_y());
        handle.0.height.set(handle.content_height());
        handle
    }

    fn same(&self, other: &ScrollHandle) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    fn kill(&self) {
        self.0.alive.set(false);
        self.0.listeners.borrow_mut().clear();
    }

    fn scroll(&self) -> Option<Retained<UIScrollView>> {
        if !self.0.alive.get() {
            return None;
        }
        self.0.scroll.load()
    }

    /// Whether the scroll view is still the app's, with the watched
    /// `testID`.
    pub fn is_alive(&self) -> bool {
        self.scroll().is_some()
    }

    /// The vertical offset.
    pub fn content_offset_y(&self) -> CGFloat {
        self.scroll()
            .map_or(0.0, |s| s.contentOffset().y + s.adjustedContentInset().top)
    }

    /// The content's height (at least the viewport's).
    pub fn content_height(&self) -> CGFloat {
        self.scroll()
            .map_or(0.0, |s| s.contentSize().height.max(Self::viewport(&s)))
    }

    /// The visible height (inside the scroll view's insets).
    pub fn viewport_height(&self) -> CGFloat {
        self.scroll().map_or(0.0, |s| Self::viewport(&s))
    }

    fn viewport(s: &UIScrollView) -> CGFloat {
        let inset = s.adjustedContentInset();
        (s.bounds().size.height - inset.top - inset.bottom).max(0.0)
    }

    /// The view the app's content is laid out in: the positions of the views
    /// inside are relative to it.
    pub fn content_view(&self) -> Option<Retained<UIView>> {
        self.scroll().map(Retained::into_super)
    }

    /// Scrolls by `dy`, clamped to the content, as the user's scroll would
    /// (the app's `onScroll` hears it); `animated` uses UIKit's animation.
    pub fn scroll_by(&self, dy: CGFloat, animated: bool) {
        let Some(s) = self.scroll() else { return };
        let inset = s.adjustedContentInset();
        let offset = s.contentOffset();
        let max = (s.contentSize().height + inset.bottom - s.bounds().size.height).max(-inset.top);
        let y = (offset.y + dy).clamp(-inset.top, max);
        s.setContentOffset_animated(CGPoint::new(offset.x, y), animated);
    }

    /// Ends a fling or an animated scroll where it is; nothing at rest.
    pub fn stop_deceleration(&self) {
        let Some(s) = self.scroll() else { return };
        let gliding = s.isDecelerating();
        // Setting the current offset without animation ends UIKit's motion.
        s.setContentOffset_animated(s.contentOffset(), false);
        if gliding && let Some(target) = self.0.target.load() {
            target.glide_stopped(&s);
        }
    }

    /// Calls `f(offset_y, dy)` on every offset change (`dy` positive: the
    /// content moved up). Returns an id for [`ScrollHandle::remove_listener`].
    pub fn on_scroll(&self, f: impl Fn(CGFloat, CGFloat) + 'static) -> u64 {
        self.add(Listener::Scroll(Rc::new(f)))
    }

    /// Calls `f(content_height)` when the content gets taller or shorter
    /// without a scroll. Returns an id for [`ScrollHandle::remove_listener`].
    pub fn on_content_size_change(&self, f: impl Fn(CGFloat) + 'static) -> u64 {
        self.add(Listener::ContentSize(Rc::new(f)))
    }

    /// Stops calling listener `id`.
    pub fn remove_listener(&self, id: u64) {
        self.0.listeners.borrow_mut().retain(|(i, _)| *i != id);
    }

    fn add(&self, listener: Listener) -> u64 {
        if !self.is_alive() {
            return 0;
        }
        let id = self.0.next.get();
        self.0.next.set(id + 1);
        self.0.listeners.borrow_mut().push((id, listener));
        id
    }

    fn scrolled(&self) {
        let y = self.content_offset_y();
        let dy = y - self.0.offset.replace(y);
        if dy == 0.0 {
            return;
        }
        // Outside the borrow: a listener may scroll or remove itself.
        let listeners: Vec<_> = self
            .0
            .listeners
            .borrow()
            .iter()
            .filter_map(|(_, l)| match l {
                Listener::Scroll(f) => Some(f.clone()),
                Listener::ContentSize(_) => None,
            })
            .collect();
        for f in listeners {
            f(y, dy);
        }
    }

    fn resized(&self) {
        let height = self.content_height();
        if self.0.height.replace(height) == height {
            return;
        }
        let listeners: Vec<_> = self
            .0
            .listeners
            .borrow()
            .iter()
            .filter_map(|(_, l)| match l {
                Listener::ContentSize(f) => Some(f.clone()),
                Listener::Scroll(_) => None,
            })
            .collect();
        for f in listeners {
            f(height);
        }
    }
}
