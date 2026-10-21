//! Navigation stacks, screens, modals.
//!
//! A stack's screens are its child views; only the top one shows (and the
//! one below it while a transition runs): the others are not child
//! visible, so GTK neither draws nor picks them. Changes of the app's
//! screens are reconciled at the end of each batch, once every item is
//! applied: a new top screen slides in from the right over the previous
//! one (which moves left a bit, as GNOME's navigation view does); a top
//! screen the app removes slides out as the image it last showed (a
//! `gtk::WidgetPaintable` of the top screen keeps it when the view is
//! unmapped, as the same batch tears its content down).
//!
//! NSH draws no header: a screen's `header` (the app's view,
//! [`ScreenConfig::header_view`]) is a child of the screen whose frame the
//! core sets at its top (as wide as the screen, as tall as it lays out),
//! with the content laid out below it; it is kept above the content. The
//! other header items (the native header's slots on iOS and Android) stay
//! hidden. Under the app's content each screen has an opaque backdrop in
//! the window's color, so a screen sliding over another hides it.
//!
//! Going back (Escape, Alt+Left, the Back key, the mouse's back button;
//! Escape in a focused text field or page only leaves it; the others go
//! back in a page's history first while the page has the focus, or the
//! pointer for the mouse button) sends `onBackPress` (`BackRequest`) to the
//! top screen when the app listens, then pops natively unless its
//! `popOnBack` is false, then sends `onDismiss`, which removes the route in
//! JS; until the app removes the screen it is `popped` and ignored. The
//! answer comes from props mounted beforehand, never from JS. The newest
//! modal comes first: a stack inside it pops, else it gets
//! `onRequestClose` (reason `"cancel"`; with `closeOnEscape` it also closes
//! natively, and gets `onDismiss` right after).
//!
//! Modals are children of the surface's root, kept above the app's views,
//! at the size of the surface: transparent layers with no look of their own
//! (the runtime renders the app's popup and backdrop in them; the
//! presentation is ignored). They slide up or fade in, then get `onShow`;
//! when the app removes one it leaves as an image of its last frame, then
//! gets `onDismiss`.
//!
//! Transitions run on the root's frame clock and only move views
//! (`shift`, `fade`): nothing is laid out again while they run.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use gtk4::{self as gtk, gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};
use nsh_core::{
    Bounds, EdgeInsets, EventKind, EventPayload, ModalAnimation, ModalConfig, NodeId, PayloadValue,
    Point, Points, ROOT_ID, ScreenAnimation, ScreenConfig, Size, ViewKind,
};
use nsh_engine::ColorScheme;
use rustc_hash::FxHashMap;

use crate::{surface::Surface, view::NshView};

/// Push and pop.
const SLIDE_MS: f64 = 300.0;
/// Cross-fades (and slides when the user asked for less motion).
const FADE_MS: f64 = 200.0;
/// Modals sliding up or down.
const MODAL_MS: f64 = 300.0;
/// How far the screen under a pushed one moves left, of the width.
const PARALLAX: f32 = 0.3;

/// The window's color (libadwaita's), under a screen's content, so a screen
/// moving over another hides it.
fn window_color(dark: bool) -> gdk::RGBA {
    if dark {
        gdk::RGBA::new(0.141, 0.141, 0.141, 1.0)
    } else {
        gdk::RGBA::new(0.98, 0.98, 0.98, 1.0)
    }
}

mod imp {
    use super::*;

    /// See [`super::Layer`].
    #[derive(Default)]
    pub(crate) struct Layer {
        pub(crate) fill: Cell<Option<gdk::RGBA>>,
        pub(crate) node: RefCell<Option<gsk::RenderNode>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Layer {
        const NAME: &'static str = "NshNavLayer";
        type Type = super::Layer;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Layer {}

    impl WidgetImpl for Layer {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            if let Some(c) = self.fill.get() {
                snapshot.append_color(&c, &graphene::Rect::new(0.0, 0.0, w, h));
            }
            if let Some(node) = &*self.node.borrow() {
                snapshot.append_node(node);
            }
        }
    }
}

glib::wrapper! {
    /// A plain drawing a view hosts: a fill (a screen's background) or the
    /// last image of a screen or modal that is leaving.
    pub(crate) struct Layer(ObjectSubclass<imp::Layer>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Layer {
    fn new() -> Self {
        let layer: Layer = glib::Object::new();
        layer.set_can_target(false);
        layer.set_focusable(false);
        layer
    }

    fn set_fill(&self, fill: Option<gdk::RGBA>) {
        if self.imp().fill.replace(fill) != fill {
            self.queue_draw();
        }
    }

    fn set_node(&self, node: Option<gsk::RenderNode>) {
        *self.imp().node.borrow_mut() = node;
        self.queue_draw();
    }
}

/// One screen.
struct ScreenNav {
    view: NshView,
    config: ScreenConfig,
    /// Its opaque background, under the app's content.
    backdrop: Layer,
    /// The header items the app inserted (only the `header` one shows).
    items: Vec<NshView>,
    /// The size and insets last reported.
    reported: Option<(Size<Points>, EdgeInsets)>,
    /// The size the background was placed at.
    backdrop_size: (f32, f32),
}

/// One stack.
struct StackNav {
    view: NshView,
    /// Its screens as shown, bottom to top (without popped ones).
    shown: Vec<NodeId>,
    /// Screens the user popped, until the app removes them.
    popped: Vec<NodeId>,
    /// Watches the top screen: keeps its last image when it is unmapped.
    paintable: Option<(NodeId, gtk::WidgetPaintable)>,
}

/// One modal (kept while its node lives: the app may show it again).
#[derive(Default)]
struct ModalNav {
    config: ModalConfig,
    /// Its view while it is presented.
    view: Option<NshView>,
    /// The user closed it natively (`closeOnEscape`) and it was reported:
    /// the app's removal only forgets it.
    closed: bool,
    reported: Option<(Size<Points>, EdgeInsets)>,
    /// Watches it while it is presented: keeps its last image.
    paintable: Option<gtk::WidgetPaintable>,
    /// What had the keyboard focus when it was presented: it gets it back
    /// when the modal closes, as a desktop dialog gives it back.
    restore: Option<glib::WeakRef<gtk::Widget>>,
}

/// What leaves the screen during a pop.
enum Leaving {
    /// The screen itself (popped by the user).
    View(NshView),
    /// The image of a screen the app removed, hosted by the stack.
    Image(Layer),
}

enum Motion {
    Push {
        stack: NshView,
        entering: NshView,
        below: Option<NshView>,
        fade: bool,
    },
    Pop {
        stack: NshView,
        leaving: Leaving,
        below: Option<NshView>,
        fade: bool,
        /// The screen the user popped: hidden and reported at the end.
        popped: Option<NodeId>,
    },
    ModalIn {
        view: NshView,
        fade: bool,
    },
    ModalOut {
        /// The modal to report `onDismiss` for at the end (`None`: it was
        /// reported when the user closed it).
        dismiss: Option<NodeId>,
        layer: Layer,
        /// Where the modal was.
        frame: Bounds<Points>,
        fade: bool,
    },
}

struct Transition {
    /// Frame time of its first frame (µs).
    start: Option<i64>,
    duration: f64,
    motion: Motion,
}

impl Transition {
    fn stack(&self) -> Option<NodeId> {
        match &self.motion {
            Motion::Push { stack, .. } | Motion::Pop { stack, .. } => Some(stack.id()),
            _ => None,
        }
    }

    /// Its progress at `now` (0 to 1, not eased).
    fn progress(&self, now: i64) -> f64 {
        self.start
            .map_or(0.0, |s| (now - s) as f64 / 1000.0 / self.duration)
    }
}

/// The navigation state of a surface.
#[derive(Default)]
pub(crate) struct Nav {
    stacks: FxHashMap<NodeId, StackNav>,
    screens: FxHashMap<NodeId, ScreenNav>,
    modals: FxHashMap<NodeId, ModalNav>,
    /// Presented modals, oldest first.
    order: Vec<NodeId>,
    /// Stacks whose screens changed in this batch.
    dirty: Vec<NodeId>,
    /// Modals presented in this batch.
    presenting: Vec<NodeId>,
    transitions: Vec<Transition>,
    ticking: bool,
    keys_installed: bool,
    /// The appearance the screens' backgrounds have.
    dark: Option<bool>,
}

fn dark(surface: &Surface) -> bool {
    surface.environment.get().color_scheme == ColorScheme::Dark
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Points> {
    Bounds {
        origin: Point {
            x: Points(x),
            y: Points(y),
        },
        size: Size {
            width: Points(width),
            height: Points(height),
        },
    }
}

fn ease(t: f64) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    (1.0 - u * u * u) as f32
}

/// How a modal comes and goes: the duration and whether it fades;
/// `None`: at once.
fn modal_motion(surface: &Surface, animation: ModalAnimation) -> Option<(f64, bool)> {
    match animation {
        ModalAnimation::None => None,
        ModalAnimation::Fade => Some((FADE_MS, true)),
        ModalAnimation::Slide if surface.environment.get().reduce_motion => Some((FADE_MS, true)),
        ModalAnimation::Slide => Some((MODAL_MS, false)),
    }
}

/// The surface's safe-area insets.
fn safe_area(surface: &Surface) -> EdgeInsets {
    let safe = surface.environment.get().safe_area;
    EdgeInsets {
        top: safe.top,
        right: safe.right,
        bottom: safe.bottom,
        left: safe.left,
    }
}

fn insets_payload(i: EdgeInsets) -> EventPayload {
    let n = |v: f32| PayloadValue::Number(v as f64);
    EventPayload::Map(vec![
        ("top", n(i.top.0)),
        ("right", n(i.right.0)),
        ("bottom", n(i.bottom.0)),
        ("left", n(i.left.0)),
    ])
}

/// Reports root `id`'s size and insets when they changed.
fn report(
    surface: &Surface,
    id: NodeId,
    last: &mut Option<(Size<Points>, EdgeInsets)>,
    size: Size<Points>,
    insets: EdgeInsets,
) {
    if size.width <= Points::ZERO || *last == Some((size, insets)) {
        return;
    }
    let insets_changed = last.is_none_or(|(_, i)| i != insets);
    *last = Some((size, insets));
    surface.control(|c| c.set_root_size(id, size, insets));
    if insets_changed {
        surface.event(id, EventKind::Insets, insets_payload(insets));
    }
}

/// Takes the keyboard focus away from inside `widget` before it is hidden
/// or unparented: GTK would hand it to the parent, which (not focusable)
/// gives it to its first focusable descendant, the field of another
/// screen (selecting all its text).
fn drop_focus(widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    if let Some(root) = widget.root()
        && root
            .focus()
            .is_some_and(|f| &f == widget || f.is_ancestor(widget))
    {
        root.set_focus(None::<&gtk::Widget>);
    }
}

/// Hides `widget` (not child visible), without passing its focus on.
fn hide(widget: &impl IsA<gtk::Widget>) {
    if widget.as_ref().is_child_visible() {
        drop_focus(widget);
        widget.as_ref().set_child_visible(false);
    }
}

fn set_shift(view: &NshView, shift: (f32, f32)) {
    let changed = std::mem::replace(&mut view.state_mut().shift, shift) != shift;
    if changed {
        view.moved();
    }
}

fn set_fade(view: &NshView, fade: f32) {
    let changed = std::mem::replace(&mut view.state_mut().fade, fade) != fade;
    if changed {
        view.changed();
    }
}

/// What `paintable` shows now (its widget, or the image it kept of it).
fn image_of(paintable: &gtk::WidgetPaintable, width: f32, height: f32) -> Option<gsk::RenderNode> {
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width as f64, height as f64);
    snapshot.to_node()
}

/// Moves `tail` to the end of `parent`'s children (drawn last), unless it
/// is there already.
fn keep_last(parent: &impl IsA<gtk::Widget>, tail: &[gtk::Widget]) {
    let mut expected = parent.last_child();
    let in_place = tail.iter().rev().all(|w| {
        let ok = expected.as_ref() == Some(w);
        expected = expected.as_ref().and_then(|e| e.prev_sibling());
        ok
    });
    if !in_place {
        for w in tail {
            w.insert_before(parent, None::<&gtk::Widget>);
        }
    }
}

impl ScreenNav {
    fn new(view: &NshView) -> Self {
        let backdrop = Layer::new();
        // First, under the app's content.
        backdrop.insert_after(view, None::<&gtk::Widget>);
        ScreenNav {
            view: view.clone(),
            config: ScreenConfig::default(),
            backdrop,
            items: Vec::new(),
            reported: None,
            backdrop_size: (-1.0, -1.0),
        }
    }

    /// Reports the screen's size (its stack's) and insets: the window's
    /// safe area, but the top's when its `header` covers it.
    fn report(&mut self, surface: &Surface, stack: &NshView) {
        let f = stack.state().frame;
        let mut insets = safe_area(surface);
        if self.config.header_view.is_some() {
            insets.top = Points::ZERO;
        }
        let id = self.view.id();
        let size = f.size;
        report(surface, id, &mut self.reported, size, insets);
    }

    /// Gives the background the window's color in the appearance `dark`.
    fn style(&self, dark: bool) {
        self.backdrop.set_fill(Some(window_color(dark)));
    }

    /// Places the background, and shows the `header` item (where the core
    /// put it) and only that one.
    fn layout(&mut self) {
        let (w, h) = {
            let f = self.view.state().frame;
            (f.size.width.0, f.size.height.0)
        };
        if self.backdrop_size != (w, h) {
            self.backdrop_size = (w, h);
            self.view.set_native(&self.backdrop, rect(0.0, 0.0, w, h));
        }
        for item in &self.items {
            if Some(item.id()) == self.config.header_view {
                item.set_child_visible(true);
            } else {
                hide(item);
            }
        }
    }

    /// Keeps the `header` over the app's content (which the app may insert
    /// after it).
    fn raise_header(&self) {
        let header = self
            .items
            .iter()
            .find(|v| Some(v.id()) == self.config.header_view);
        if let Some(header) = header {
            keep_last(&self.view, &[header.clone().upcast()]);
        }
    }
}

impl Nav {
    /// Forgets the app's views (it stopped); the window's key handlers
    /// and a running tick callback stay.
    pub(crate) fn reset(&mut self) {
        *self = Nav {
            ticking: self.ticking,
            keys_installed: self.keys_installed,
            ..Nav::default()
        };
    }

    fn screen(&mut self, view: &NshView) -> &mut ScreenNav {
        self.screens
            .entry(view.id())
            .or_insert_with(|| ScreenNav::new(view))
    }

    /// Whether a transition runs.
    #[cfg(feature = "dev")]
    pub(crate) fn transitioning(&self) -> bool {
        !self.transitions.is_empty()
    }

    /// Ends the transitions `which` selects at once.
    fn finish(&mut self, surface: &Surface, which: impl Fn(&Transition) -> bool) {
        let mut i = 0;
        while i < self.transitions.len() {
            if which(&self.transitions[i]) {
                let t = self.transitions.remove(i);
                self.end(surface, t);
            } else {
                i += 1;
            }
        }
    }

    /// Puts the views of `motion` where they are at eased progress `e`.
    fn apply(&self, surface: &Surface, motion: &Motion, e: f32) {
        match motion {
            Motion::Push {
                stack,
                entering,
                below,
                fade,
            } => {
                let w = stack.state().frame.size.width.0;
                if *fade {
                    set_fade(entering, e);
                } else {
                    set_shift(entering, (w * (1.0 - e), 0.0));
                    if let Some(below) = below {
                        set_shift(below, (-PARALLAX * w * e, 0.0));
                    }
                }
            }
            Motion::Pop {
                stack,
                leaving,
                below,
                fade,
                ..
            } => {
                let f = stack.state().frame;
                match leaving {
                    Leaving::View(v) if *fade => set_fade(v, 1.0 - e),
                    Leaving::View(v) => set_shift(v, (f.size.width.0 * e, 0.0)),
                    Leaving::Image(layer) => {
                        let x = if *fade {
                            layer.set_opacity((1.0 - e) as f64);
                            0.0
                        } else {
                            f.size.width.0 * e
                        };
                        stack.set_native(layer, rect(x, 0.0, f.size.width.0, f.size.height.0));
                    }
                }
                if let (Some(below), false) = (below, fade) {
                    set_shift(below, (-PARALLAX * f.size.width.0 * (1.0 - e), 0.0));
                }
            }
            Motion::ModalIn { view, fade } => {
                if *fade {
                    set_fade(view, e);
                } else {
                    // From below the window's bottom edge.
                    let h = surface.root.state().frame.size.height.0;
                    let y = view.state().frame.origin.y.0;
                    set_shift(view, (0.0, (h - y) * (1.0 - e)));
                }
            }
            Motion::ModalOut {
                layer, frame, fade, ..
            } => {
                let h = surface.root.state().frame.size.height.0;
                let y = if *fade {
                    layer.set_opacity((1.0 - e) as f64);
                    frame.origin.y.0
                } else {
                    frame.origin.y.0 + (h - frame.origin.y.0) * e
                };
                surface.root.set_native(
                    layer,
                    rect(frame.origin.x.0, y, frame.size.width.0, frame.size.height.0),
                );
            }
        }
    }

    fn end(&mut self, surface: &Surface, t: Transition) {
        self.apply(surface, &t.motion, 1.0);
        match t.motion {
            Motion::Push { stack, .. } => self.settle(stack.id()),
            Motion::Pop {
                stack,
                leaving,
                popped,
                ..
            } => {
                match leaving {
                    Leaving::View(v) => {
                        hide(&v);
                        set_shift(&v, (0.0, 0.0));
                        set_fade(&v, 1.0);
                    }
                    Leaving::Image(layer) => stack.remove_native(&layer),
                }
                self.settle(stack.id());
                if let Some(id) = popped {
                    surface.event(id, EventKind::Dismiss, EventPayload::None);
                }
            }
            Motion::ModalIn { view, .. } => {
                set_shift(&view, (0.0, 0.0));
                set_fade(&view, 1.0);
                surface.event(view.id(), EventKind::Show, EventPayload::None);
            }
            Motion::ModalOut { dismiss, layer, .. } => {
                surface.root.remove_native(&layer);
                if let Some(id) = dismiss {
                    surface.event(id, EventKind::Dismiss, EventPayload::None);
                }
            }
        }
    }

    /// Shows only the top screen of `stack`, where it belongs.
    fn settle(&mut self, stack: NodeId) {
        let Some(st) = self.stacks.get(&stack) else {
            return;
        };
        let top = st.shown.last().copied();
        for view in st.view.child_views() {
            if view.kind() != ViewKind::Screen {
                continue;
            }
            if Some(view.id()) == top {
                view.set_child_visible(true);
            } else {
                hide(&view);
            }
            set_shift(&view, (0.0, 0.0));
            set_fade(&view, 1.0);
        }
    }

    fn start(&mut self, surface: &Rc<Surface>, duration: f64, motion: Motion) {
        self.apply(surface, &motion, 0.0);
        self.transitions.push(Transition {
            start: None,
            duration,
            motion,
        });
        if self.ticking {
            return;
        }
        self.ticking = true;
        let weak = Rc::downgrade(surface);
        surface.root.add_tick_callback(move |_, clock| {
            let Some(surface) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let Ok(mut nav) = surface.nav.try_borrow_mut() else {
                return glib::ControlFlow::Continue;
            };
            nav.tick(&surface, clock.frame_time());
            if nav.transitions.is_empty() {
                nav.ticking = false;
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    fn tick(&mut self, surface: &Surface, now: i64) {
        for t in &mut self.transitions {
            t.start.get_or_insert(now);
        }
        let mut i = 0;
        while i < self.transitions.len() {
            let p = self.transitions[i].progress(now);
            if p >= 1.0 {
                let t = self.transitions.remove(i);
                self.end(surface, t);
            } else {
                self.apply(surface, &self.transitions[i].motion, ease(p));
                i += 1;
            }
        }
    }

    /// The duration and whether it fades; `None`: no animation.
    fn screen_motion(surface: &Surface, animation: ScreenAnimation) -> Option<(f64, bool)> {
        match animation {
            ScreenAnimation::None => None,
            ScreenAnimation::Fade => Some((FADE_MS, true)),
            ScreenAnimation::Default if surface.environment.get().reduce_motion => {
                Some((FADE_MS, true))
            }
            ScreenAnimation::Default => Some((SLIDE_MS, false)),
        }
    }

    /// Shows the app's screens of `stack` after a batch changed them.
    fn reconcile(&mut self, surface: &Rc<Surface>, stack_id: NodeId) {
        self.finish(surface, |t| t.stack() == Some(stack_id));
        let dark = dark(surface);
        let Some(st) = self.stacks.get_mut(&stack_id) else {
            return;
        };
        let stack = st.view.clone();
        let children: Vec<NshView> = stack
            .child_views()
            .into_iter()
            .filter(|v| v.kind() == ViewKind::Screen)
            .collect();
        let find = |id: NodeId| children.iter().find(|c| c.id() == id).cloned();
        st.popped.retain(|&id| find(id).is_some());
        let desired: Vec<NodeId> = children
            .iter()
            .map(NshView::id)
            .filter(|id| !st.popped.contains(id))
            .collect();
        let prev = std::mem::replace(&mut st.shown, desired.clone());
        let (old, new) = (prev.last().copied(), desired.last().copied());
        let frame = stack.state().frame;
        // The image the top screen showed when the app removed it.
        let image = match (old, &st.paintable) {
            (Some(old), Some((watched, paintable)))
                if Some(old) != new && *watched == old && find(old).is_none() =>
            {
                image_of(paintable, frame.size.width.0, frame.size.height.0)
            }
            _ => None,
        };
        match (new.and_then(find), &mut st.paintable) {
            (Some(top), Some((watched, paintable))) => {
                if *watched != top.id() {
                    *watched = top.id();
                    paintable.set_widget(Some(&top));
                }
            }
            (Some(top), slot @ None) => {
                *slot = Some((top.id(), gtk::WidgetPaintable::new(Some(&top))));
            }
            (None, slot) => *slot = None,
        }
        let leaving_animation = old
            .and_then(|id| self.screens.get(&id))
            .map(|s| s.config.animation);
        for id in &prev {
            if find(*id).is_none() {
                self.screens.remove(id);
            }
        }
        for &id in &desired {
            let Some(view) = find(id) else { continue };
            let s = self.screen(&view);
            if !prev.contains(&id) {
                s.style(dark);
                s.report(surface, &stack);
            }
        }
        if children.is_empty() && stack.parent().is_none() {
            // The stack is gone.
            self.stacks.remove(&stack_id);
            return;
        }
        self.settle(stack_id);
        let (Some(old), Some(new)) = (old, new) else {
            return;
        };
        if old == new || frame.size.width.0 <= 0.0 {
            return;
        }
        let pushing = !prev.contains(&new);
        let animation = if pushing {
            self.screens.get(&new).map(|s| s.config.animation)
        } else {
            leaving_animation
        };
        let Some((duration, fade)) = Self::screen_motion(surface, animation.unwrap_or_default())
        else {
            return;
        };
        if pushing {
            let Some(entering) = find(new) else { return };
            let below = find(old);
            if let Some(b) = &below {
                b.set_child_visible(true);
            }
            let motion = Motion::Push {
                stack,
                entering,
                below,
                fade,
            };
            self.start(surface, duration, motion);
        } else {
            let leaving = match find(old) {
                Some(v) => {
                    v.set_child_visible(true);
                    Leaving::View(v)
                }
                None => {
                    let Some(node) = image else { return };
                    let layer = Layer::new();
                    layer.set_node(Some(node));
                    Leaving::Image(layer)
                }
            };
            let motion = Motion::Pop {
                stack,
                leaving,
                below: find(new),
                fade,
                popped: None,
            };
            self.start(surface, duration, motion);
        }
    }

    /// The user goes back in `stack`: tells the top screen when the app
    /// listens (`onBackPress`), then pops it natively unless it prevents it
    /// (`popOnBack` false), and tells the app once it is gone.
    fn back(&mut self, surface: &Rc<Surface>, stack_id: NodeId) {
        self.finish(surface, |t| t.stack() == Some(stack_id));
        let Some(st) = self.stacks.get_mut(&stack_id) else {
            return;
        };
        let Some(&top) = st.shown.last() else { return };
        if let Some(s) = self.screens.get(&top) {
            if s.view.state().events.contains(EventKind::BackRequest) {
                surface.event(top, EventKind::BackRequest, EventPayload::None);
            }
            if s.config.prevent_back {
                return;
            }
        }
        if st.shown.len() < 2 {
            return;
        }
        st.shown.pop();
        st.popped.push(top);
        let stack = st.view.clone();
        let children = stack.child_views();
        let find = |id: NodeId| children.iter().find(|c| c.id() == id).cloned();
        let below = st.shown.last().copied().and_then(find);
        if let (Some(below), Some((watched, paintable))) = (&below, &mut st.paintable) {
            *watched = below.id();
            paintable.set_widget(Some(below));
        }
        let animation = self.screens.get(&top).map(|s| s.config.animation);
        match (
            Self::screen_motion(surface, animation.unwrap_or_default()),
            find(top),
        ) {
            (Some((duration, fade)), Some(leaving)) => {
                if let Some(b) = &below {
                    b.set_child_visible(true);
                }
                let motion = Motion::Pop {
                    stack,
                    leaving: Leaving::View(leaving),
                    below,
                    fade,
                    popped: Some(top),
                };
                self.start(surface, duration, motion);
            }
            _ => {
                self.settle(stack_id);
                surface.event(top, EventKind::Dismiss, EventPayload::None);
            }
        }
    }

    /// The stack going back acts on: the innermost shown one that can pop
    /// or asks first, inside `within` (the newest modal) when given.
    fn back_stack(&self, within: Option<&NshView>) -> Option<NodeId> {
        let wants = |st: &StackNav| {
            st.view.is_mapped()
                && within.is_none_or(|w| st.view.is_ancestor(w))
                && (st.shown.len() > 1
                    || st.shown.last().is_some_and(|top| {
                        self.screens.get(top).is_some_and(|s| s.config.prevent_back)
                    }))
        };
        let candidates: Vec<&StackNav> = self.stacks.values().filter(|s| wants(s)).collect();
        candidates
            .iter()
            .find(|c| {
                !candidates
                    .iter()
                    .any(|o| o.view != c.view && o.view.is_ancestor(&c.view))
            })
            .map(|c| c.view.id())
    }

    /// Takes modal `id` away natively (it leaves as an image of its last
    /// frame), then reports `onDismiss` unless `reported` already.
    fn close_modal(&mut self, surface: &Rc<Surface>, id: NodeId, reported: bool) {
        self.finish(
            surface,
            |t| matches!(&t.motion, Motion::ModalIn { view, .. } if view.id() == id),
        );
        self.order.retain(|&m| m != id);
        let Some(m) = self.modals.get_mut(&id) else {
            return;
        };
        let Some(view) = m.view.take() else { return };
        let restore = m.restore.take().and_then(|w| w.upgrade());
        let frame = view.state().frame;
        let motion = modal_motion(surface, m.config.animation);
        // Unmapped, the paintable keeps the image the modal showed last.
        let paintable = m.paintable.take();
        let mapped = view.is_mapped();
        if view.parent().is_some() {
            drop_focus(&view);
            view.unparent();
        }
        let image = match (motion, paintable) {
            (Some(_), Some(paintable)) if mapped => {
                image_of(&paintable, frame.size.width.0, frame.size.height.0)
            }
            _ => None,
        };
        set_shift(&view, (0.0, 0.0));
        set_fade(&view, 1.0);
        self.scope_focus(surface);
        if let Some(widget) = restore
            && let Some(root) = surface.root.root()
            && root.focus().is_none()
            && widget.is_ancestor(&surface.root)
        {
            // Refused while another modal still covers it. A text field
            // would select all its text otherwise, and the next key typed
            // would replace it.
            match widget.downcast_ref::<gtk::Text>() {
                Some(text) => text.grab_focus_without_selecting(),
                None => widget.grab_focus(),
            };
        }
        match (motion, image) {
            (Some((duration, fade)), Some(node)) => {
                let layer = Layer::new();
                layer.set_node(Some(node));
                let motion = Motion::ModalOut {
                    dismiss: (!reported).then_some(id),
                    layer,
                    frame,
                    fade,
                };
                self.start(surface, duration, motion);
            }
            _ => {
                if !reported {
                    surface.event(id, EventKind::Dismiss, EventPayload::None);
                }
            }
        }
    }

    /// Animates presented modal `id` in, then reports `onShow`.
    fn show_modal(&mut self, surface: &Rc<Surface>, id: NodeId) {
        let Some(m) = self.modals.get(&id) else {
            return;
        };
        let Some(view) = m.view.clone() else { return };
        match modal_motion(surface, m.config.animation) {
            Some((duration, fade)) => {
                self.start(surface, duration, Motion::ModalIn { view, fade });
            }
            None => surface.event(id, EventKind::Show, EventPayload::None),
        }
    }

    /// The user asks modal `id` to close (going back): it hears
    /// `onRequestClose` (reason `"cancel"`). With `closeOnEscape` it also
    /// leaves natively and hears `onDismiss` right after, as on iOS (the
    /// runtime keeps the handler of a modal the app removed in between).
    fn request_close(&mut self, surface: &Rc<Surface>, id: NodeId) {
        let reason = EventPayload::Map(vec![("reason", PayloadValue::Str("cancel".into()))]);
        surface.event(id, EventKind::RequestClose, reason);
        if let Some(m) = self.modals.get_mut(&id)
            && m.config.close_on_escape
            && m.view.is_some()
        {
            m.closed = true;
            surface.event(id, EventKind::Dismiss, EventPayload::None);
            self.close_modal(surface, id, true);
        }
    }

    /// Going back: a stack inside the newest modal pops, else the modal
    /// asks to close; without a modal the app's `onBackPress` hears it
    /// first when it listens (it sends `Command::Back` when no listener
    /// takes it), else the innermost stack pops. Whether something took it.
    fn go_back(&mut self, surface: &Rc<Surface>) -> bool {
        if self.order.is_empty() && surface.root.state().events.contains(EventKind::BackPress) {
            surface.event(ROOT_ID, EventKind::BackPress, EventPayload::None);
            return true;
        }
        self.default_back(surface)
    }

    /// Going back as without the app's listeners (`Command::Back`): a
    /// stack inside the newest modal pops, else the modal asks to close,
    /// else the innermost stack pops. Whether something took it.
    fn default_back(&mut self, surface: &Rc<Surface>) -> bool {
        if let Some(&id) = self.order.last() {
            let view = self.modals.get(&id).and_then(|m| m.view.clone());
            match self.back_stack(view.as_ref()) {
                Some(stack) => self.back(surface, stack),
                None => self.request_close(surface, id),
            }
            return true;
        }
        if let Some(stack) = self.back_stack(None) {
            self.back(surface, stack);
            return true;
        }
        false
    }

    /// Keeps the keyboard focus inside the newest modal: Tab never reaches
    /// the views under it (`can-focus` off holds for a widget's descendants
    /// too). Dialogs and toasts are not views, and keep theirs.
    fn scope_focus(&self, surface: &Surface) {
        let top = self
            .order
            .last()
            .and_then(|id| self.modals.get(id))
            .and_then(|m| m.view.clone());
        let mut child = surface.root.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            if c.is::<NshView>() {
                c.set_can_focus(
                    top.as_ref()
                        .is_none_or(|t| t.upcast_ref::<gtk::Widget>() == &c),
                );
            }
        }
    }

    /// Keeps the presented modals (and those leaving) over the app's
    /// views.
    fn raise_modals(&self, surface: &Surface) {
        let mut tail: Vec<gtk::Widget> = Vec::new();
        for m in self.order.iter().filter_map(|id| self.modals.get(id)) {
            if let Some(view) = &m.view {
                tail.push(view.clone().upcast());
            }
        }
        for t in &self.transitions {
            if let Motion::ModalOut { layer, .. } = &t.motion {
                tail.push(layer.clone().upcast());
            }
        }
        keep_last(&surface.root, &tail);
    }

    /// Sizes modal `id` to the surface and reports it (with the surface's
    /// safe area).
    fn size_modal(&mut self, surface: &Surface, id: NodeId) {
        let Some(m) = self.modals.get_mut(&id) else {
            return;
        };
        let Some(view) = &m.view else { return };
        let f = surface.root.state().frame;
        let frame = rect(0.0, 0.0, f.size.width.0, f.size.height.0);
        if std::mem::replace(&mut view.state_mut().frame, frame) != frame {
            view.queue_allocate();
            view.moved();
        }
        let size = frame.size;
        report(surface, id, &mut m.reported, size, safe_area(surface));
    }

    /// Listens to the window's back keys and the mouse's back button.
    fn install_keys(&mut self, surface: &Rc<Surface>) {
        if self.keys_installed {
            return;
        }
        let Some(window) = surface.root.root() else {
            return;
        };
        self.keys_installed = true;
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(surface);
        keys.connect_key_pressed(move |_, key, _, modifiers| match weak.upgrade() {
            Some(surface) if key_pressed(&surface, key, modifiers) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        });
        window.add_controller(keys);
        let click = gtk::GestureClick::new();
        click.set_button(8);
        let weak: Weak<Surface> = Rc::downgrade(surface);
        click.connect_pressed(move |gesture, _, _, _| {
            if let Some(surface) = weak.upgrade()
                && go_back(&surface)
            {
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
        });
        window.add_controller(click);
    }
}

/// Goes back (the newest modal, the innermost stack or the app's
/// `onBackPress`): whether something took it.
pub(crate) fn go_back(surface: &Rc<Surface>) -> bool {
    match surface.nav.try_borrow_mut() {
        Ok(mut nav) => nav.go_back(surface),
        Err(_) => false,
    }
}

/// `Command::Back`: goes back as without the app's listeners, once the
/// batch being applied is in.
pub(crate) fn default_back(surface: &Rc<Surface>) {
    let surface = Rc::downgrade(surface);
    glib::idle_add_local_once(move || {
        if let Some(surface) = surface.upgrade()
            && let Ok(mut nav) = surface.nav.try_borrow_mut()
        {
            nav.default_back(&surface);
        }
    });
}

/// The window of `surface`, when a text field (a `gtk::TextView` or any
/// `gtk::Editable`) or a web view's page has its keyboard focus.
pub(crate) fn focused_field(surface: &Surface) -> Option<gtk::Root> {
    let root = surface.root.root()?;
    let focus = root.focus()?;
    (focus.is::<gtk::TextView>()
        || focus.is::<gtk::Editable>()
        || crate::native::page_of(&focus).is_some())
    .then_some(root)
}

/// A key nothing in the window used: Escape, Alt+Left and the Back key go
/// back. Whether it was taken. Escape in a focused text field only leaves
/// the field (a desktop convention: it never navigates away from what is
/// being typed), and so does Escape a focused page did not use (WebKit
/// hands those back to the window); Alt+Left and the Back key still go
/// back (in a page they go back in its history first, `native::web`).
pub(crate) fn key_pressed(
    surface: &Rc<Surface>,
    key: gdk::Key,
    modifiers: gdk::ModifierType,
) -> bool {
    let mods = modifiers
        & (gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::SUPER_MASK);
    let back = match key {
        gdk::Key::Escape | gdk::Key::Back => mods.is_empty(),
        gdk::Key::Left | gdk::Key::KP_Left => mods == gdk::ModifierType::ALT_MASK,
        _ => false,
    };
    if back
        && key == gdk::Key::Escape
        && let Some(root) = focused_field(surface)
    {
        root.set_focus(None::<&gtk::Widget>);
        return true;
    }
    back && go_back(surface)
}

/// A screen was pushed (`added`) or popped in `stack`.
pub(crate) fn stack_changed(surface: &Rc<Surface>, stack: &NshView, added: Option<&NshView>) {
    let mut nav = surface.nav.borrow_mut();
    let id = stack.id();
    nav.stacks.entry(id).or_insert_with(|| StackNav {
        view: stack.clone(),
        shown: Vec::new(),
        popped: Vec::new(),
        paintable: None,
    });
    if let Some(screen) = added.filter(|v| v.kind() == ViewKind::Screen) {
        nav.screen(screen);
    }
    if !nav.dirty.contains(&id) {
        nav.dirty.push(id);
    }
}

/// A stack's frame changed.
pub(crate) fn stack_resized(surface: &Rc<Surface>, stack: &NshView) {
    let mut nav = surface.nav.borrow_mut();
    let nav = &mut *nav;
    let Some(st) = nav.stacks.get(&stack.id()) else {
        return;
    };
    for id in &st.shown {
        if let Some(s) = nav.screens.get_mut(id) {
            s.report(surface, stack);
        }
    }
}

/// `ScreenConfig`.
pub(crate) fn screen_config(surface: &Rc<Surface>, view: &NshView, config: &ScreenConfig) {
    let dark = dark(surface);
    let mut nav = surface.nav.borrow_mut();
    let s = nav.screen(view);
    s.config = config.clone();
    s.style(dark);
    s.layout();
    if let Some(stack) = view.parent_view().filter(|p| p.kind() == ViewKind::Stack) {
        s.report(surface, &stack);
    }
}

/// `ModalConfig`.
pub(crate) fn modal_config(surface: &Rc<Surface>, view: &NshView, config: &ModalConfig) {
    let mut nav = surface.nav.borrow_mut();
    let id = view.id();
    nav.modals.entry(id).or_default().config = *config;
    nav.size_modal(surface, id);
}

/// A modal was inserted: present it.
pub(crate) fn present(surface: &Rc<Surface>, view: &NshView) {
    let mut nav = surface.nav.borrow_mut();
    let id = view.id();
    let m = nav.modals.entry(id).or_default();
    if m.view.is_some() {
        return;
    }
    // The field being typed in under the modal leaves it (until the modal
    // closes).
    let window = surface.root.root();
    let focus = window
        .as_ref()
        .and_then(|r| r.focus())
        .filter(|f| f.is_ancestor(&surface.root));
    if let (Some(window), Some(_)) = (&window, &focus) {
        window.set_focus(None::<&gtk::Widget>);
    }
    m.restore = focus.map(|f| f.downgrade());
    m.view = Some(view.clone());
    m.paintable = Some(gtk::WidgetPaintable::new(Some(view)));
    m.closed = false;
    m.reported = None;
    if view.parent().is_some() {
        view.unparent();
    }
    view.insert_before(&surface.root, None::<&gtk::Widget>);
    nav.order.push(id);
    nav.scope_focus(surface);
    nav.size_modal(surface, id);
    // Animated in at the end of the batch, with the options it brings.
    nav.presenting.push(id);
}

/// A modal was removed: dismiss it.
pub(crate) fn dismiss(surface: &Rc<Surface>, view: &NshView) {
    let mut nav = surface.nav.borrow_mut();
    let id = view.id();
    match nav.modals.get_mut(&id) {
        // The user closed it already, and it was reported.
        Some(m) if m.closed => m.closed = false,
        Some(_) => nav.close_modal(surface, id, false),
        None => {
            if view.parent().is_some() {
                drop_focus(view);
                view.unparent();
            }
        }
    }
}

/// A header item was inserted into `parent` (a screen).
pub(crate) fn header_item(surface: &Rc<Surface>, parent: &NshView, child: &NshView) {
    let mut nav = surface.nav.borrow_mut();
    if child.parent().as_ref() != Some(parent.upcast_ref()) {
        if child.parent().is_some() {
            child.unparent();
        }
        child.insert_before(parent, None::<&gtk::Widget>);
    }
    let s = nav.screen(parent);
    if !s.items.contains(child) {
        s.items.push(child.clone());
    }
    // Shown by the screen's layout at the end of the batch when it is the
    // `header` (the core places it).
    hide(child);
}

/// A header item was removed.
pub(crate) fn header_item_removed(surface: &Rc<Surface>, view: &NshView) {
    let mut nav = surface.nav.borrow_mut();
    if let Some(screen) = view.parent_view()
        && let Some(s) = nav.screens.get_mut(&screen.id())
    {
        s.items.retain(|v| v != view);
    }
    if view.parent().is_some() {
        view.unparent();
    }
    set_shift(view, (0.0, 0.0));
}

/// The surface was resized (modals follow).
pub(crate) fn surface_resized(surface: &Surface, _width: f32, _height: f32) {
    let Ok(mut nav) = surface.nav.try_borrow_mut() else {
        return;
    };
    for id in nav.order.clone() {
        nav.size_modal(surface, id);
    }
}

/// A batch was applied.
pub(crate) fn end_batch(surface: &Rc<Surface>) {
    let mut nav = surface.nav.borrow_mut();
    nav.install_keys(surface);
    let dark = dark(surface);
    if nav.dark.replace(dark).is_some_and(|d| d != dark) {
        for s in nav.screens.values() {
            s.style(dark);
        }
    }
    for stack in std::mem::take(&mut nav.dirty) {
        nav.reconcile(surface, stack);
    }
    for id in std::mem::take(&mut nav.presenting) {
        nav.show_modal(surface, id);
    }
    // Screen sizes change with the app's layout.
    for s in nav.screens.values_mut() {
        s.layout();
        s.raise_header();
    }
    if !nav.order.is_empty() || !nav.transitions.is_empty() {
        nav.raise_modals(surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eases_out() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert_eq!(ease(2.0), 1.0);
        assert!(ease(0.5) > 0.5);
    }
}
