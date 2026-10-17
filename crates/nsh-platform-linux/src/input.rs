//! Pointer input of a surface: hit testing, touches for the JS responder
//! system, presses, hover / pressed states, cursors, native pan / pinch /
//! rotation values, and routing wheel, touchpad and touchscreen scrolling
//! to the scroll views.
//!
//! One `EventControllerLegacy` on the root, in the capture phase, sees
//! every pointer event before the widgets under it. It never stops button,
//! motion or touch events, so hosted native widgets (text fields) still get
//! their clicks, focus and selection drags; presses, pans and scroll drags
//! simply do not start on them. Wheel and touchpad scrolls it routes are
//! stopped, unless they land on a native widget that scrolls itself.
//!
//! Hit testing walks the views itself rather than using `gtk_widget_pick`:
//! GTK's picking knows nothing of `overflow: hidden` (the views clip in
//! `snapshot`), hidden views or `box-only`. Points go into children through
//! their allocation transforms (frame, scroll offset, shift, transform).
//! Outside a view's bounds, only the area its descendants cover
//! (`MountItem::Overflow`: a dropdown under its field) is looked into, as
//! on iOS and Android. GTK still delivers events by its own picking, so
//! presses, hovers and wheels GTK would send to a web view's page where
//! nsh does not show it (clipped off, hidden) are kept from the page
//! (`shields_page`), after nsh handled them.
//!
//! Presses follow React Native and the other backends: the innermost view
//! listening to press events gets them (or the pressable text span under
//! the pointer); `activeOpacity` fades the view natively; inside scroll
//! views a touch reports `onPressIn` only after ~130 ms, as it may start a
//! scroll. A move beyond 6 pt starts the innermost pan or scroll view that
//! takes that direction (only touches drag scroll views: the mouse scrolls
//! with the wheel, the touchpad, the keyboard and the indicator, and its
//! drags go to pans, responders and text selection); beyond 10 pt the
//! press is over.
//!
//! A view with a `panActivation` around the first pointer gates it
//! ([`PanActivation::decide`] on every move): the scroll views around the
//! view wait until its pan fails, and never take the drag once it
//! activates; the activation goes to JS with the move's touches
//! (`TouchEvent::activated`), where the view becomes the responder.
//!
//! A scroll stays on the scroll view it began in (a touchpad gesture until
//! it ends, mouse wheel notches while they come within 200 ms), as in
//! browsers: inner views passing under the pointer do not take it over, and
//! its view reaching an edge does not hand the rest to an outer one.
//!
//! Real and simulated input ([`simulate`]) share the same handlers, which
//! take root coordinates and millisecond timestamps.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use gtk4::{self as gtk, gdk, glib, glib::translate::ToGlibPtr, graphene, prelude::*};
use nsh_core::{
    Command, Cursor, EventKind, EventMask, EventPayload, GestureValues, NodeId, PanActivation,
    PayloadValue, PointerEvents, ROOT_ID, TouchEvent, TouchPhase, TouchPoint, ViewKind,
    tailwind::state::{HOVERED, PRESSED},
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{surface::Surface, view::NshView};

/// Distance a pointer moves before a pan or scroll drag starts.
const PAN_SLOP: f32 = 6.0;
/// Distance a pointer moves before its press is cancelled.
const PRESS_SLOP: f32 = 10.0;
/// How far outside the pressed view (and its hit slop) a release still
/// counts as a press.
const PRESS_RETENTION: f32 = 8.0;
/// Mouse wheel notches this close together (ms) are one scroll: they stay
/// on the scroll view the first one moved.
const WHEEL_LATCH: f64 = 200.0;
/// Scale one Ctrl + wheel notch pinches by.
const ZOOM_STEP: f32 = 1.1;
/// A Ctrl + wheel pinch ends once the notches stop for this long (ms).
const ZOOM_IDLE: u64 = 300;
/// Delay before `onPressIn` for touches inside scroll views.
const SCROLL_PRESS_DELAY: u64 = 130;
/// Default `delayLongPress`.
const LONG_PRESS: f32 = 500.0;
/// Velocities are measured over this many milliseconds.
const VELOCITY_WINDOW: f64 = 100.0;
/// Duration of the `activeOpacity` fade when pressed.
const FADE_IN_MS: f64 = 100.0;
/// Duration of the fade back.
const FADE_OUT_MS: f64 = 250.0;

/// Which device a pointer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    /// The mouse (or touchpad clicks).
    Mouse,
    /// A touchscreen finger, by its event sequence.
    Touch(usize),
    /// A scripted finger ([`simulate`]).
    Simulated,
}

/// Pointer state of one surface.
#[derive(Default)]
pub(crate) struct Input {
    /// The pointers that are down.
    pointers: RefCell<Vec<Pointer>>,
    /// Two fingers pinching / rotating on the touchscreen.
    two: RefCell<Option<TwoFinger>>,
    /// A touchpad pinch.
    pinch: RefCell<Option<Transform>>,
    /// A pinch by Ctrl + mouse wheel, with the generation of its end timer.
    zoom: RefCell<Option<(Transform, u64)>>,
    /// A touchpad scroll gesture.
    wheel: RefCell<Option<Wheel>>,
    /// The scroll view mouse wheel notches go to while they keep coming,
    /// whether they scroll horizontally, and when the last one came (ms).
    latch: RefCell<Option<(NshView, bool, f64)>>,
    /// Views that track `HOVERED` or listen to hover events (hover is only
    /// tested while there are).
    hover_tracked: RefCell<FxHashSet<NodeId>>,
    /// Views that are hovered now.
    hovered: RefCell<Vec<NshView>>,
    /// Native gesture values by view.
    gestures: RefCell<FxHashMap<NodeId, (glib::WeakRef<NshView>, GestureValues)>>,
    /// Running `activeOpacity` fades.
    fades: RefCell<FxHashMap<NodeId, Fade>>,
    /// The JS responder blocks native scrolling and pans.
    js_block: Cell<bool>,
    /// Where the pointer last was (root coordinates), for events GTK
    /// gives no position (wheels, touchpad gestures).
    last: Cell<Option<(f32, f32)>>,
    /// Next touch identifier.
    next_id: Cell<u32>,
    /// Next press / fade generation (invalidates callbacks of earlier ones).
    next_gen: Cell<u64>,
}

/// What a drag on a view may start.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Candidate {
    /// A pan with these values (both when it only has listeners).
    Pan { x: bool, y: bool },
    /// A scroll view.
    Scroll { horizontal: bool },
}

impl Candidate {
    /// Whether a drag by `(dx, dy)` starts it: single-axis pans and scroll
    /// views take mostly-along-their-axis movements, so a horizontal pan
    /// coexists with a vertical scroll view around it.
    fn accepts(self, dx: f32, dy: f32) -> bool {
        let horizontal = dx.abs() > dy.abs();
        match self {
            Candidate::Pan { x, y } => (x && y) || (x && horizontal) || (y && !horizontal),
            Candidate::Scroll { horizontal: h } => h == horizontal,
        }
    }
}

/// What a pointer is doing.
#[derive(Debug, Clone, PartialEq)]
enum Mode {
    /// Not past the pan slop yet.
    Idle,
    /// Past it, with nothing to drag.
    Free,
    /// Dragging a pan view.
    Pan(NshView),
    /// Scrolling a scroll view, from where the drag began.
    Scroll(NshView, (f32, f32)),
    /// Dragging a scroll view's indicator as a scroll bar's thumb (the
    /// mouse), from where the drag began.
    Thumb(NshView, (f32, f32)),
    /// One of two fingers pinching / rotating.
    TwoFinger,
}

/// A press in progress.
struct Press {
    /// The view whose bounds the press must end in (the text view for a
    /// span).
    view: NshView,
    /// The node that gets the events.
    node: NodeId,
    events: EventMask,
    /// `activeOpacity` (none for spans).
    feedback: Option<f32>,
    /// Milliseconds before a long press.
    long_delay: f32,
    /// `onPressIn` was reported.
    pressed: bool,
    /// `onLongPress` was reported: the release is no press.
    long: bool,
    generation: u64,
}

/// A pointer that is down.
struct Pointer {
    key: Key,
    /// Touch identifier for JS.
    id: u32,
    /// Behaves like a finger (scroll drags, delayed press-in).
    touch: bool,
    x: f32,
    y: f32,
    start: (f32, f32),
    time: f64,
    velocity: VelocityTracker,
    /// The innermost view under it when it went down.
    target: NshView,
    /// The views around it that listen to touches, target outwards.
    path: Vec<NodeId>,
    /// Its touches are reported to JS.
    reporting: bool,
    press: Option<Press>,
    /// What a drag may start, innermost first.
    candidates: Vec<(NshView, Candidate)>,
    /// The undecided `panActivation` around it.
    gate: Option<Gate>,
    /// The view a second finger would pinch / rotate.
    pinch: Option<NshView>,
    /// Views that track `PRESSED` and are pressed by it.
    pressed: Vec<NshView>,
    mode: Mode,
    /// It moved (a JS responder granted now took it from a native press).
    moved: bool,
}

/// A view's `panActivation` deciding on a pointer's drag.
struct Gate {
    /// The gated view.
    id: NodeId,
    activation: PanActivation,
    /// Candidates from this index on are around the gated view: they wait
    /// for the pan to fail.
    outer: usize,
}

impl Gate {
    /// Decides on a drag by `(dx, dy)` from where the pointer went down:
    /// the gated view when its pan activates (the candidates around it are
    /// dropped), and whether the gate is still undecided.
    fn decide(
        &self,
        candidates: &mut Vec<(NshView, Candidate)>,
        dx: f32,
        dy: f32,
    ) -> (Option<NodeId>, bool) {
        match self.activation.decide(dx, dy) {
            Some(true) => {
                candidates.truncate(self.outer);
                (Some(self.id), false)
            }
            Some(false) => (None, false),
            None => (None, true),
        }
    }
}

/// A pinch / rotation in progress.
#[derive(Clone)]
struct Transform {
    view: NshView,
    pinch: bool,
    rotation: bool,
    scale: f32,
    degrees: f32,
}

/// Two touchscreen fingers transforming a view.
struct TwoFinger {
    keys: [Key; 2],
    distance: f32,
    angle: f32,
    transform: Transform,
}

/// A touchpad scroll gesture.
struct Wheel {
    view: NshView,
    /// Accumulated deltas, for the velocity.
    total: (f32, f32),
    velocity: VelocityTracker,
    /// Began with a hold (fingers resting) and has not scrolled yet.
    held: bool,
}

/// An `activeOpacity` fade.
struct Fade {
    /// The view's fade before the press.
    base: f32,
    tick: Option<gtk::TickCallbackId>,
    generation: u64,
}

/// Estimates a pointer's velocity from its recent positions.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct VelocityTracker {
    samples: [(f64, f32, f32); VelocityTracker::LEN],
    count: usize,
    head: usize,
}

impl VelocityTracker {
    const LEN: usize = 16;

    /// The pointer is at `(x, y)` at `time` (ms).
    pub(crate) fn add(&mut self, time: f64, x: f32, y: f32) {
        self.samples[self.head] = (time, x, y);
        self.head = (self.head + 1) % Self::LEN;
        self.count = (self.count + 1).min(Self::LEN);
    }

    /// Points per second over the last [`VELOCITY_WINDOW`] ms (zero after
    /// a pause).
    pub(crate) fn velocity(&self) -> (f32, f32) {
        if self.count < 2 {
            return (0.0, 0.0);
        }
        let at = |i: usize| self.samples[(self.head + Self::LEN - 1 - i) % Self::LEN];
        let newest = at(0);
        let mut oldest = newest;
        for i in 1..self.count {
            let s = at(i);
            if newest.0 - s.0 > VELOCITY_WINDOW {
                break;
            }
            oldest = s;
        }
        let dt = newest.0 - oldest.0;
        if dt < 1.0 {
            return (0.0, 0.0);
        }
        let k = (1000.0 / dt) as f32;
        ((newest.1 - oldest.1) * k, (newest.2 - oldest.2) * k)
    }
}

// --- Installation and the views' hooks ----------------------------------------------

/// Installs the event controllers on the surface's root view.
pub(crate) fn install(surface: &Rc<Surface>) {
    let root = &surface.root;
    let legacy = gtk::EventControllerLegacy::new();
    legacy.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = Rc::downgrade(surface);
    legacy.connect_event(move |_, event| match weak.upgrade() {
        Some(surface) => handle(&surface, event),
        None => glib::Propagation::Proceed,
    });
    root.add_controller(legacy);

    // Crossing events do not reach legacy controllers.
    let motion = gtk::EventControllerMotion::new();
    let weak = Rc::downgrade(surface);
    motion.connect_leave(move |_| {
        if let Some(surface) = weak.upgrade() {
            set_hovered(&surface, Vec::new());
        }
    });
    root.add_controller(motion);
}

/// The app stopped: forgets its pointers, hovered views and gestures.
pub(crate) fn reset(surface: &Surface) {
    let input = &surface.input;
    input.pointers.borrow_mut().clear();
    input.two.borrow_mut().take();
    input.pinch.borrow_mut().take();
    input.zoom.borrow_mut().take();
    input.wheel.borrow_mut().take();
    input.latch.borrow_mut().take();
    input.hover_tracked.borrow_mut().clear();
    input.hovered.borrow_mut().clear();
    input.gestures.borrow_mut().clear();
    for (_, fade) in input.fades.borrow_mut().drain() {
        if let Some(tick) = fade.tick {
            tick.remove();
        }
    }
    input.js_block.set(false);
}

/// `TrackStates`.
pub(crate) fn track_states(surface: &Rc<Surface>, view: &NshView, states: u8) {
    {
        let mut s = view.state_mut();
        s.tracked = states;
        // The core keeps the states still tracked and forgets the rest
        // (`Core::update_tracking`): so do we, or a change it was not told
        // about (the pointer leaving a view whose hover was tracked
        // throughout) would never reach it.
        s.states &= states;
    }
    update_hover_tracking(surface, view);
}

/// Whether hovering `view` matters: for its `hover:` styles (the
/// `HOVERED` state) or its `onHoverIn` / `onHoverOut`.
fn hoverable(view: &NshView) -> bool {
    let s = view.state();
    s.tracked & HOVERED != 0
        || s.events.contains(EventKind::HoverIn)
        || s.events.contains(EventKind::HoverOut)
}

fn update_hover_tracking(surface: &Surface, view: &NshView) {
    let id = view.state().id;
    let mut tracked = surface.input.hover_tracked.borrow_mut();
    if hoverable(view) {
        tracked.insert(id);
    } else {
        tracked.remove(&id);
        // Not hovered as far as anyone knows: picked again if hovering it
        // matters again under the pointer.
        surface.input.hovered.borrow_mut().retain(|v| v != view);
    }
}

/// Sets or clears interaction state `bit` of `view` and reports the
/// states when they changed and the view tracks the bit.
pub(crate) fn set_state(surface: &Surface, view: &NshView, bit: u8, on: bool) {
    let (id, next) = {
        let mut s = view.state_mut();
        if s.tracked & bit == 0 {
            return;
        }
        let next = if on { s.states | bit } else { s.states & !bit };
        if next == s.states {
            return;
        }
        s.states = next;
        (s.id, next)
    };
    surface.control(|c| c.set_states(id, next));
}

/// `Gestures`.
pub(crate) fn gestures(surface: &Rc<Surface>, view: &NshView, values: GestureValues) {
    let id = view.state().id;
    let mut gestures = surface.input.gestures.borrow_mut();
    if values.is_empty() {
        gestures.remove(&id);
    } else {
        gestures.insert(id, (view.downgrade(), values));
    }
}

/// The view's events changed (hover handlers have it hovered, from the
/// next mouse move).
pub(crate) fn events_changed(surface: &Rc<Surface>, view: &NshView) {
    update_hover_tracking(surface, view);
    update_cursor(view);
}

/// The view's props changed.
pub(crate) fn props_changed(_surface: &Rc<Surface>, _view: &NshView) {
    // `pointerEvents`, `hitSlop`, `activeOpacity` and `delayLongPress` are
    // read from the state when a pointer goes down.
}

/// The view's style changed (cursor).
pub(crate) fn style_changed(view: &NshView) {
    update_cursor(view);
}

/// `SetJsResponder` / `ClearJsResponder`. Runs while the mounter applies a
/// batch: it must not write animations (that borrows the mounter).
pub(crate) fn command(surface: &Rc<Surface>, _id: NodeId, command: &Command) {
    match command {
        Command::SetJsResponder { block } => {
            surface.input.js_block.set(*block);
            let mut pointers = std::mem::take(&mut *surface.input.pointers.borrow_mut());
            for p in &mut pointers {
                // Granted on a move: the JS gesture takes over from the native
                // press (a grant when the finger lands leaves inner presses be,
                // as they would have won the negotiation in React Native).
                if p.moved {
                    cancel_press(surface, p);
                    release_pressed(surface, p);
                }
                if *block && let Mode::Pan(view) = &p.mode {
                    let view = view.clone();
                    pan_end(surface, &view, p, false);
                    p.mode = Mode::Free;
                }
            }
            let mut current = surface.input.pointers.borrow_mut();
            pointers.append(&mut current);
            *current = pointers;
        }
        Command::ClearJsResponder => surface.input.js_block.set(false),
        _ => {}
    }
}

/// The CSS name of the pointer over a view, `None` to inherit its parent's.
fn cursor_name(cursor: Cursor, pressable: bool) -> Option<&'static str> {
    Some(match cursor {
        Cursor::Auto if pressable => "pointer",
        Cursor::Auto => return None,
        Cursor::Default => "default",
        Cursor::Pointer => "pointer",
        Cursor::Text => "text",
        Cursor::Wait => "wait",
        Cursor::Progress => "progress",
        Cursor::Help => "help",
        Cursor::Crosshair => "crosshair",
        Cursor::Move => "move",
        Cursor::Grab => "grab",
        Cursor::Grabbing => "grabbing",
        Cursor::NotAllowed => "not-allowed",
        Cursor::ZoomIn => "zoom-in",
        Cursor::ZoomOut => "zoom-out",
        Cursor::ColResize => "col-resize",
        Cursor::RowResize => "row-resize",
        Cursor::None => "none",
    })
}

fn has_press(events: EventMask) -> bool {
    [
        EventKind::Press,
        EventKind::LongPress,
        EventKind::PressIn,
        EventKind::PressOut,
    ]
    .into_iter()
    .any(|k| events.contains(k))
}

fn update_cursor(view: &NshView) {
    let name = {
        let s = view.state();
        // Selectable text shows the I-beam (a press target its pointer).
        cursor_name(s.style.cursor, has_press(s.events))
            .or_else(|| crate::text::selectable(&s).then_some("text"))
    };
    let current = view.cursor().and_then(|c| c.name());
    if current.as_deref() != name {
        view.set_cursor_from_name(name);
    }
}

// --- GDK events ---------------------------------------------------------------------

fn handle(surface: &Rc<Surface>, event: &gdk::Event) -> glib::Propagation {
    // Decided before nsh's own handling moves anything.
    let shielded = shields_page(surface, event);
    let propagation = dispatch(surface, event);
    if shielded {
        glib::Propagation::Stop
    } else {
        propagation
    }
}

/// Whether `event` would reach a web view's page where nsh does not show
/// it: GTK picks widgets by their allocations and knows nothing of
/// `overflow: hidden`, `hidden` or `pointerEvents`, so without this a
/// page clipped by a parent would take clicks, hovers and wheels outside
/// what is drawn of it. nsh's own handling of the event goes on (the view
/// actually under the pointer gets it); only the page is kept from it.
fn shields_page(surface: &Surface, event: &gdk::Event) -> bool {
    use gdk::EventType as T;
    if !surface.native.has_pages() {
        return false;
    }
    let buttons = gdk::ModifierType::BUTTON1_MASK
        | gdk::ModifierType::BUTTON2_MASK
        | gdk::ModifierType::BUTTON3_MASK;
    let point = match event.event_type() {
        T::ButtonPress | T::TouchBegin => root_point(&surface.root, event),
        // Drags that started in the page stay with it.
        T::MotionNotify if !event.modifier_state().intersects(buttons) => {
            root_point(&surface.root, event)
        }
        T::Scroll => root_point(&surface.root, event).or_else(|| surface.input.last.get()),
        _ => None,
    };
    let Some((x, y)) = point else {
        return false;
    };
    let Some(page) = surface
        .root
        .pick(x as f64, y as f64, gtk::PickFlags::DEFAULT)
        .and_then(|target| crate::native::page_of(&target))
    else {
        return false;
    };
    let shown = pick(&surface.root, x, y)
        .and_then(|hit| hit.native)
        .is_some_and(|native| native == page || native.is_ancestor(&page));
    !shown
}

fn dispatch(surface: &Rc<Surface>, event: &gdk::Event) -> glib::Propagation {
    use gdk::EventType as T;
    let ty = event.event_type();
    match ty {
        T::ButtonPress
        | T::ButtonRelease
        | T::MotionNotify
        | T::TouchBegin
        | T::TouchUpdate
        | T::TouchEnd
        | T::TouchCancel
        | T::Scroll
        | T::TouchpadPinch
        | T::TouchpadHold => {}
        T::GrabBroken => {
            cancel_all(surface);
            return glib::Propagation::Proceed;
        }
        _ => return glib::Propagation::Proceed,
    }
    // Scroll and touchpad gesture events carry no position in GTK 4: they
    // happen where the pointer is.
    let point = match root_point(&surface.root, event) {
        Some(p) => {
            surface.input.last.set(Some(p));
            Some(p)
        }
        None if matches!(ty, T::Scroll | T::TouchpadPinch | T::TouchpadHold) => {
            surface.input.last.get()
        }
        None => None,
    };
    let Some((x, y)) = point else {
        return glib::Propagation::Proceed;
    };
    let time = event.time() as f64;
    let button = || event.downcast_ref::<gdk::ButtonEvent>().map(|b| b.button());
    match ty {
        // Touchscreens report touches; pointer events emulated from them
        // would press twice.
        T::ButtonPress | T::ButtonRelease | T::MotionNotify if event.is_pointer_emulated() => {}
        // On a view that drags the window: the window's, not the app's.
        T::ButtonPress if crate::chrome::press(surface, event, x, y) => {
            return glib::Propagation::Stop;
        }
        T::ButtonPress if button() == Some(gdk::BUTTON_PRIMARY) => {
            down(surface, Key::Mouse, x, y, time)
        }
        T::ButtonRelease if button() == Some(gdk::BUTTON_PRIMARY) => {
            up(surface, Key::Mouse, x, y, time, false)
        }
        T::MotionNotify => motion(surface, Key::Mouse, x, y, time),
        T::TouchBegin => down(surface, touch_key(event), x, y, time),
        T::TouchUpdate => motion(surface, touch_key(event), x, y, time),
        T::TouchEnd => up(surface, touch_key(event), x, y, time, false),
        T::TouchCancel => up(surface, touch_key(event), x, y, time, true),
        T::Scroll => {
            if let Some(scroll) = event.downcast_ref::<gdk::ScrollEvent>() {
                return scroll_event(surface, scroll, x, y, time);
            }
        }
        T::TouchpadPinch | T::TouchpadHold => {
            if let Some(pad) = event.downcast_ref::<gdk::TouchpadEvent>() {
                let handled = if ty == T::TouchpadPinch {
                    touchpad_pinch(surface, pad, x, y)
                } else {
                    touchpad_hold(surface, pad.gesture_phase(), x, y)
                };
                if handled {
                    return glib::Propagation::Stop;
                }
            }
        }
        _ => {}
    }
    glib::Propagation::Proceed
}

/// The event's position in the root's coordinates (`None` for events of
/// other surfaces, such as popovers of hosted widgets).
fn root_point(root: &NshView, event: &gdk::Event) -> Option<(f32, f32)> {
    let (x, y) = event.position()?;
    let native = root.native()?;
    if event.surface() != native.surface() {
        return None;
    }
    let (sx, sy) = native.surface_transform();
    let p = native.compute_point(
        root,
        &graphene::Point::new((x - sx) as f32, (y - sy) as f32),
    )?;
    Some((p.x(), p.y()))
}

fn touch_key(event: &gdk::Event) -> Key {
    let sequence = event.event_sequence();
    let ptr: *const gdk::ffi::GdkEventSequence = sequence.to_glib_none().0;
    Key::Touch(ptr as usize)
}

/// How a scroll event scrolls.
#[derive(Debug, Clone, Copy, PartialEq)]
enum WheelInput {
    /// A mouse wheel step: notches (`true`, scaled to the view) or
    /// points.
    Step { notches: bool },
    /// A touchpad gesture moved by points.
    Touchpad,
    /// The touchpad gesture ended.
    Stop,
}

fn scroll_event(
    surface: &Rc<Surface>,
    event: &gdk::ScrollEvent,
    x: f32,
    y: f32,
    time: f64,
) -> glib::Propagation {
    let notches = WheelInput::Step { notches: true };
    let (kind, mut dx, mut dy) = match event.direction() {
        gdk::ScrollDirection::Up => (notches, 0.0, -1.0),
        gdk::ScrollDirection::Down => (notches, 0.0, 1.0),
        gdk::ScrollDirection::Left => (notches, -1.0, 0.0),
        gdk::ScrollDirection::Right => (notches, 1.0, 0.0),
        _ => {
            let (dx, dy) = event.deltas();
            let kind = if event.is_stop() {
                WheelInput::Stop
            } else if event.unit() == gdk::ScrollUnit::Wheel {
                notches
            } else {
                WheelInput::Touchpad
            };
            (kind, dx as f32, dy as f32)
        }
    };
    let modifiers = event.modifier_state();
    // Mice have no pinch: Ctrl + wheel pinches, as it zooms on desktops.
    if let WheelInput::Step { .. } = kind
        && modifiers.contains(gdk::ModifierType::CONTROL_MASK)
        && dy != 0.0
        && wheel_zoom(surface, x, y, dy)
    {
        return glib::Propagation::Stop;
    }
    if kind == notches && dx == 0.0 && modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
        (dx, dy) = (dy, dx);
    }
    if wheel_input(surface, x, y, dx, dy, time, kind) {
        glib::Propagation::Stop
    } else {
        glib::Propagation::Proceed
    }
}

// --- Simulation ---------------------------------------------------------------------

/// A simulated pointer action ([`crate::script`]), in root coordinates.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Simulated {
    /// Press at `(x, y)`, with the mouse when `true`, else a finger.
    Down(f32, f32, bool),
    /// Move to `(x, y)` while pressed.
    Move(f32, f32, bool),
    /// Release at `(x, y)`.
    Up(f32, f32, bool),
    /// A wheel / touchpad scroll at `(x, y)` by `(dx, dy)` points.
    Wheel(f32, f32, f32, f32),
    /// The pointer moved to `(x, y)` without a button (hover).
    Hover(f32, f32),
}

/// Feeds a simulated pointer action through the same path as real input.
/// A simulated finger's drags scroll, the mouse's do not.
pub(crate) fn simulate(surface: &Rc<Surface>, action: Simulated) {
    let time = glib::monotonic_time() as f64 / 1000.0;
    let key = |mouse| if mouse { Key::Mouse } else { Key::Simulated };
    match action {
        Simulated::Down(x, y, mouse) => down(surface, key(mouse), x, y, time),
        Simulated::Move(x, y, mouse) => motion(surface, key(mouse), x, y, time),
        Simulated::Up(x, y, mouse) => {
            up(surface, key(mouse), x, y, time, false);
            // GTK never sees simulated input: a release on a hosted widget
            // (a text field) focuses it, as a real click would.
            if let Some(native) = pick(&surface.root, x, y).and_then(|hit| hit.native) {
                focus_native(&native);
            }
        }
        Simulated::Wheel(x, y, dx, dy) => {
            let step = WheelInput::Step { notches: false };
            wheel_input(surface, x, y, dx, dy, time, step);
        }
        Simulated::Hover(x, y) => hover(surface, x, y),
    }
}

/// Focuses `widget` or the first focusable widget inside it.
fn focus_native(widget: &gtk::Widget) {
    if widget.grab_focus() {
        return;
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        if c.is_focusable() && c.grab_focus() {
            return;
        }
        focus_native(&c);
        child = c.next_sibling();
    }
}

// --- Hit testing --------------------------------------------------------------------

/// Where the pointer last was (root coordinates).
pub(crate) fn last_pointer(surface: &Surface) -> Option<(f32, f32)> {
    surface.input.last.get()
}

/// Pressing `(x, y)` (root coordinates) drags the window: the nearest view
/// that sets `windowDrag` says so, unless a text field, a hosted widget or
/// a view with press or touch handlers is nearer.
pub(crate) fn window_drag(surface: &Surface, x: f32, y: f32) -> bool {
    let Some(hit) = pick(&surface.root, x, y) else {
        return false;
    };
    if hit.native.is_some() {
        return false;
    }
    for view in chain(&hit.view) {
        let s = view.state();
        if let Some(drag) = s.props.window_drag {
            return drag;
        }
        if s.kind == ViewKind::Input || has_press(s.events) || s.events.contains(EventKind::Touch) {
            return false;
        }
    }
    false
}

/// What is under a point.
struct Hit {
    /// The innermost view.
    view: NshView,
    /// The hosted native widget the point is on.
    native: Option<gtk::Widget>,
}

/// The innermost view at `(x, y)` (root coordinates).
fn pick(root: &NshView, x: f32, y: f32) -> Option<Hit> {
    pick_in(root, graphene::Point::new(x, y))
}

fn pick_in(view: &NshView, p: graphene::Point) -> Option<Hit> {
    if !view.is_visible() || !view.can_target() {
        return None;
    }
    let (pe, in_slop, children) = {
        let s = view.state();
        if s.style.hidden || s.props.pointer_events == PointerEvents::None {
            return None;
        }
        let clip = s.style.overflow_hidden
            || matches!(
                s.kind,
                ViewKind::Scroll | ViewKind::Image | ViewKind::Stack | ViewKind::Input
            );
        let (w, h) = (s.frame.width, s.frame.height);
        let (x, y) = (p.x(), p.y());
        let inside = x >= 0.0 && y >= 0.0 && x < w && y < h;
        let slop = s.props.hit_slop;
        let in_slop =
            x >= -slop.left && y >= -slop.top && x < w + slop.right && y < h + slop.bottom;
        let children = if clip {
            inside
        } else if laid_out(s.kind, s.id) {
            in_slop
                || s.overflow.is_some_and(|e| {
                    x >= e.x && y >= e.y && x < e.x + e.width && y < e.y + e.height
                })
        } else {
            // The platform places these views' children (screens, header
            // items) and the core knows no area for them.
            true
        };
        (s.props.pointer_events, in_slop, children)
    };
    let own = || (pe != PointerEvents::BoxNone && in_slop).then(|| hit(view, None));
    if !children {
        return own();
    }
    if pe != PointerEvents::BoxOnly {
        let mut child = view.last_child();
        while let Some(widget) = child {
            child = widget.prev_sibling();
            if !widget.is_visible() || !widget.can_target() {
                continue;
            }
            let Some(cp) = view.compute_point(&widget, &p) else {
                continue;
            };
            match widget.downcast_ref::<NshView>() {
                Some(child) => {
                    if let Some(hit) = pick_in(child, cp) {
                        return Some(hit);
                    }
                }
                // A hosted native widget.
                None => {
                    if let Some(native) =
                        widget.pick(cp.x() as f64, cp.y() as f64, gtk::PickFlags::DEFAULT)
                    {
                        return Some(hit(view, Some(native)));
                    }
                }
            }
        }
    }
    own()
}

/// Whether the core lays out the children of a view of `kind`, so that
/// they are outside its bounds only in its overflow area.
fn laid_out(kind: ViewKind, id: NodeId) -> bool {
    id != ROOT_ID && matches!(kind, ViewKind::View | ViewKind::Text)
}

fn hit(view: &NshView, native: Option<gtk::Widget>) -> Hit {
    Hit {
        view: view.clone(),
        native,
    }
}

/// `view` and its ancestors, innermost first.
fn chain(view: &NshView) -> Vec<NshView> {
    let mut out = vec![view.clone()];
    let mut current = view.parent_view();
    while let Some(v) = current {
        current = v.parent_view();
        out.push(v);
    }
    out
}

/// A point in root coordinates in `view`'s.
fn local(surface: &Surface, view: &NshView, x: f32, y: f32) -> (f32, f32) {
    surface
        .root
        .compute_point(view, &graphene::Point::new(x, y))
        .map_or((x, y), |p| (p.x(), p.y()))
}

fn gesture_values(surface: &Surface, view: &NshView, id: NodeId) -> GestureValues {
    surface
        .input
        .gestures
        .borrow()
        .get(&id)
        .filter(|(weak, _)| weak.upgrade().as_ref() == Some(view))
        .map(|(_, values)| *values)
        .unwrap_or_default()
}

/// Whether a view pinches / rotates natively or listens to it.
fn transforms(values: &GestureValues, events: EventMask) -> (bool, bool) {
    (
        values.pinch.is_some()
            || events.contains(EventKind::PinchStart)
            || events.contains(EventKind::PinchEnd),
        values.rotation.is_some()
            || events.contains(EventKind::RotationStart)
            || events.contains(EventKind::RotationEnd),
    )
}

// --- Pointers -----------------------------------------------------------------------

fn take(surface: &Surface, key: Key) -> Option<Pointer> {
    let mut pointers = surface.input.pointers.borrow_mut();
    let i = pointers.iter().position(|p| p.key == key)?;
    Some(pointers.swap_remove(i))
}

fn put(surface: &Surface, p: Pointer) {
    surface.input.pointers.borrow_mut().push(p);
}

fn down(surface: &Rc<Surface>, key: Key, x: f32, y: f32, time: f64) {
    // A release we never saw (the window lost the grab).
    if let Some(stale) = take(surface, key) {
        finish(surface, stale, true);
    }
    let Some(hit) = pick(&surface.root, x, y) else {
        return;
    };
    let input = &surface.input;
    if input.pointers.borrow().is_empty() {
        input.js_block.set(false);
    }
    let touch = key != Key::Mouse;
    let native = hit.native.is_some();
    // Mouse drags over selectable text select it (`text.rs`) rather than
    // pan.
    let selects = !touch && !native && crate::text::selectable(&hit.view.state());
    let views = chain(&hit.view);
    // The mouse drags an indicator as a scroll bar: its thumb follows the
    // pointer, where a drag on the content moves the content with it.
    let thumb = (!touch && !native)
        .then(|| {
            views.iter().find(|v| {
                let (lx, ly) = local(surface, v, x, y);
                crate::scroll::grabs_thumb(v, lx, ly)
            })
        })
        .flatten();
    if let Some(view) = thumb {
        let id = input.next_id.get();
        input.next_id.set(id.wrapping_add(1));
        let mut p = Pointer {
            key,
            id,
            touch,
            x,
            y,
            start: (x, y),
            time,
            velocity: VelocityTracker::default(),
            target: view.clone(),
            path: Vec::new(),
            reporting: false,
            press: None,
            candidates: Vec::new(),
            gate: None,
            pinch: None,
            pressed: Vec::new(),
            mode: Mode::Thumb(view.clone(), (x, y)),
            moved: false,
        };
        p.velocity.add(time, x, y);
        crate::scroll::thumb_begin(surface, view);
        put(surface, p);
        return;
    }
    let mut path = Vec::new();
    let mut candidates = Vec::new();
    let mut pinch = None;
    let mut pressed = Vec::new();
    let mut in_scroll = false;
    let mut gate = None;
    // The gate follows the first pointer only, as on the other platforms.
    let first = input.pointers.borrow().is_empty();
    for v in &views {
        let s = v.state();
        if s.events.contains(EventKind::Touch) {
            path.push(s.id);
        }
        if s.tracked & PRESSED != 0 {
            pressed.push(v.clone());
        }
        let values = gesture_values(surface, v, s.id);
        if !native
            && !selects
            && (values.pan_x.is_some()
                || values.pan_y.is_some()
                || s.events.contains(EventKind::PanStart)
                || s.events.contains(EventKind::PanEnd))
        {
            let (x, y) = match (values.pan_x.is_some(), values.pan_y.is_some()) {
                (false, false) => (true, true),
                axes => axes,
            };
            candidates.push((v.clone(), Candidate::Pan { x, y }));
        }
        if let Some(scroll) = &s.scroll {
            in_scroll = true;
            // Dragging the content is a touchscreen's way to scroll; on the
            // desktop the mouse's drags are the app's.
            if touch && !native && !selects {
                candidates.push((
                    v.clone(),
                    Candidate::Scroll {
                        horizontal: scroll.horizontal,
                    },
                ));
            }
        }
        if pinch.is_none() && transforms(&values, s.events) != (false, false) {
            pinch = Some(v.clone());
        }
        if gate.is_none()
            && first
            && !native
            && let Some(activation) = s.props.pan_activation
        {
            gate = Some(Gate {
                id: s.id,
                activation,
                outer: candidates.len(),
            });
        }
    }
    let press = if native {
        None
    } else {
        find_press(surface, &hit.view, &views, x, y)
    };
    let id = input.next_id.get();
    input.next_id.set(id.wrapping_add(1));
    let mut p = Pointer {
        key,
        id,
        touch,
        x,
        y,
        start: (x, y),
        time,
        velocity: VelocityTracker::default(),
        target: hit.view,
        reporting: !path.is_empty(),
        path,
        press,
        candidates,
        gate,
        pinch,
        pressed,
        mode: Mode::Idle,
        moved: false,
    };
    p.velocity.add(time, x, y);
    if p.reporting {
        report(surface, TouchPhase::Start, &p, None);
    }
    for v in &p.pressed {
        set_state(surface, v, PRESSED, true);
    }
    if touch && start_two_finger(surface, &mut p) {
        put(surface, p);
        return;
    }
    if let Some(press) = &mut p.press {
        let generation = press.generation;
        if touch && in_scroll {
            after(
                surface,
                key,
                generation,
                SCROLL_PRESS_DELAY,
                |surface, p| {
                    if let Some(press) = &mut p.press {
                        press_in(surface, press);
                    }
                },
            );
        } else {
            press_in(surface, press);
        }
        if press.events.contains(EventKind::LongPress) {
            let delay = press.long_delay.max(0.0) as u64;
            after(surface, key, generation, delay, |surface, p| {
                let Some(press) = &mut p.press else { return };
                press_in(surface, press);
                press.long = true;
                press_event(surface, press.node, EventKind::LongPress);
            });
        }
    }
    put(surface, p);
}

fn motion(surface: &Rc<Surface>, key: Key, x: f32, y: f32, time: f64) {
    if key == Key::Mouse {
        hover(surface, x, y);
    }
    let Some(mut p) = take(surface, key) else {
        return;
    };
    if p.x == x && p.y == y {
        put(surface, p);
        return;
    }
    p.x = x;
    p.y = y;
    p.time = time;
    p.velocity.add(time, x, y);
    let moved = std::mem::replace(&mut p.moved, true);
    let (tx, ty) = (x - p.start.0, y - p.start.1);
    let mut activated = None;
    if let Some(gate) = p.gate.take() {
        let (id, undecided) = gate.decide(&mut p.candidates, tx, ty);
        if let Some(id) = id {
            log::debug!(target: "nsh::input", "panActivation of {id} activated");
        }
        activated = id;
        if undecided {
            p.gate = Some(gate);
        }
        // The view's JS responder takes the drag over: nothing native
        // starts meanwhile.
        if activated.is_some() && p.mode == Mode::Idle {
            p.mode = Mode::Free;
        }
    }
    if p.reporting {
        report(surface, TouchPhase::Move, &p, activated);
    }
    if p.mode == Mode::TwoFinger {
        put(surface, p);
        two_finger_moved(surface);
        return;
    }
    let distance = tx.hypot(ty);
    // JS hears of the first move only now: give its responder a chance to
    // take the gesture (and block native drags) before one starts.
    let js_pending = p.reporting && !moved;
    if p.mode == Mode::Idle && distance > PAN_SLOP && !js_pending {
        // While the gate decides, the candidates around it wait (the
        // pointer stays idle and they are asked again on the next move).
        let waiting = p.gate.as_ref().map(|g| g.outer);
        if waiting.is_none() {
            p.mode = Mode::Free;
        }
        let started = if surface.input.js_block.get() {
            None
        } else {
            let open = waiting.unwrap_or(p.candidates.len());
            p.candidates[..open]
                .iter()
                .find(|(_, c)| c.accepts(tx, ty))
                .cloned()
        };
        match started {
            Some((view, Candidate::Pan { .. })) => {
                p.gate = None;
                pan_start(surface, &view, &p);
                p.mode = Mode::Pan(view);
            }
            Some((view, Candidate::Scroll { .. })) => {
                // The scroll view takes the touches over from JS.
                if p.reporting {
                    report(surface, TouchPhase::Cancel, &p, None);
                    p.reporting = false;
                }
                p.gate = None;
                crate::scroll::drag_begin(surface, &view);
                p.mode = Mode::Scroll(view, (x, y));
            }
            None => {}
        }
    }
    if matches!(p.mode, Mode::Pan(_) | Mode::Scroll(..)) || distance > PRESS_SLOP {
        cancel_press(surface, &mut p);
        release_pressed(surface, &mut p);
    }
    match &p.mode {
        Mode::Pan(view) => {
            let view = view.clone();
            pan_move(surface, &view, tx, ty);
        }
        Mode::Scroll(view, origin) => {
            crate::scroll::drag_move(surface, view, x - origin.0, y - origin.1);
        }
        Mode::Thumb(view, origin) => {
            crate::scroll::thumb_move(surface, view, x - origin.0, y - origin.1);
        }
        _ => {}
    }
    put(surface, p);
}

fn up(surface: &Rc<Surface>, key: Key, x: f32, y: f32, time: f64, cancelled: bool) {
    let Some(mut p) = take(surface, key) else {
        return;
    };
    p.x = x;
    p.y = y;
    p.time = time;
    p.velocity.add(time, x, y);
    finish(surface, p, cancelled);
}

/// The pointer lifted (or was cancelled): ends what it did.
fn finish(surface: &Rc<Surface>, mut p: Pointer, cancelled: bool) {
    if p.reporting {
        let phase = if cancelled {
            TouchPhase::Cancel
        } else {
            TouchPhase::End
        };
        report(surface, phase, &p, None);
    }
    release_pressed(surface, &mut p);
    let mode = std::mem::replace(&mut p.mode, Mode::Free);
    match mode {
        Mode::TwoFinger => end_two_finger(surface),
        Mode::Pan(view) => {
            pan_move(surface, &view, p.x - p.start.0, p.y - p.start.1);
            pan_end(surface, &view, &p, true);
        }
        Mode::Scroll(view, _) => {
            let (vx, vy) = p.velocity.velocity();
            crate::scroll::drag_end(surface, &view, vx, vy);
        }
        Mode::Thumb(view, _) => crate::scroll::thumb_end(surface, &view),
        Mode::Idle | Mode::Free => {
            if let Some(mut press) = p.press.take() {
                if cancelled {
                    press_out(surface, &mut press, false);
                } else {
                    // A quick tap inside a scroll view: the press in now.
                    press_in(surface, &mut press);
                    let inside = released_inside(surface, &press.view, p.x, p.y);
                    press_out(surface, &mut press, inside);
                }
            }
        }
    }
}

/// Cancels every pointer (the grab broke).
fn cancel_all(surface: &Rc<Surface>) {
    let pointers = std::mem::take(&mut *surface.input.pointers.borrow_mut());
    for p in pointers {
        // Each with the others gone: JS hears one cancel per finger.
        finish(surface, p, true);
    }
}

/// Sends the touches of the reporting pointers, `changed` being the one
/// that `phase` is about (it is not in the list while it is handled), and
/// the view whose pan activation fired with it.
fn report(surface: &Surface, phase: TouchPhase, changed: &Pointer, activated: Option<NodeId>) {
    let points = {
        let pointers = surface.input.pointers.borrow();
        pointers
            .iter()
            .chain(std::iter::once(changed))
            .filter(|p| p.reporting)
            .map(|p| {
                let is_changed = std::ptr::eq(p, changed);
                let (lx, ly) = local(surface, &p.target, p.x, p.y);
                TouchPoint {
                    id: p.id,
                    target: p.target.id(),
                    // JS keeps a finger's path from its start.
                    path: if phase == TouchPhase::Start && is_changed {
                        p.path.clone()
                    } else {
                        Vec::new()
                    },
                    page_x: p.x,
                    page_y: p.y,
                    location_x: lx,
                    location_y: ly,
                    changed: is_changed,
                }
            })
            .collect()
    };
    let event = TouchEvent {
        phase,
        timestamp: changed.time,
        points,
        activated,
    };
    surface.event(
        ROOT_ID,
        EventKind::Touch,
        EventPayload::Touch(Box::new(event)),
    );
}

/// Runs `f` on pointer `key` after `ms` unless its press `generation` ended.
fn after(
    surface: &Rc<Surface>,
    key: Key,
    generation: u64,
    ms: u64,
    f: impl FnOnce(&Rc<Surface>, &mut Pointer) + 'static,
) {
    let weak = Rc::downgrade(surface);
    glib::timeout_add_local_once(Duration::from_millis(ms), move || {
        let Some(surface) = weak.upgrade() else {
            return;
        };
        let Some(mut p) = take(&surface, key) else {
            return;
        };
        if p.press
            .as_ref()
            .is_some_and(|press| press.generation == generation)
        {
            f(&surface, &mut p);
        }
        put(&surface, p);
    });
}

fn release_pressed(surface: &Surface, p: &mut Pointer) {
    for v in p.pressed.drain(..) {
        set_state(surface, &v, PRESSED, false);
    }
}

// --- Presses ------------------------------------------------------------------------

/// The press a pointer down at `(x, y)` on `target` starts: the pressable
/// text span under it, else the innermost enabled view listening to
/// presses.
fn find_press(
    surface: &Surface,
    target: &NshView,
    views: &[NshView],
    x: f32,
    y: f32,
) -> Option<Press> {
    let generation = surface.input.next_gen.get();
    surface.input.next_gen.set(generation + 1);
    {
        let s = target.state();
        if let Some(text) = &s.text
            && !text.text.links.is_empty()
        {
            let (lx, ly) = local(surface, target, x, y);
            let link = crate::text::char_at(&s, lx, ly).and_then(|i| text.text.link_at(i));
            if let Some(link) = link
                && has_press(link.events)
            {
                return Some(Press {
                    view: target.clone(),
                    node: link.node,
                    events: link.events,
                    feedback: None,
                    long_delay: s.props.long_press_delay.unwrap_or(LONG_PRESS),
                    pressed: false,
                    long: false,
                    generation,
                });
            }
        }
    }
    views.iter().find_map(|v| {
        let s = v.state();
        (has_press(s.events) && !s.props.accessibility.state.disabled).then(|| Press {
            view: v.clone(),
            node: s.id,
            events: s.events,
            feedback: s.props.active_opacity.filter(|o| *o < 1.0),
            long_delay: s.props.long_press_delay.unwrap_or(LONG_PRESS),
            pressed: false,
            long: false,
            generation,
        })
    })
}

fn press_in(surface: &Rc<Surface>, press: &mut Press) {
    if press.pressed {
        return;
    }
    press.pressed = true;
    if let Some(opacity) = press.feedback {
        fade(surface, &press.view, Some(opacity));
    }
    if press.events.contains(EventKind::PressIn) {
        press_event(surface, press.node, EventKind::PressIn);
    }
}

/// Ends a press: `onPressOut`, then `onPress` if `fire` (released inside)
/// and it was no long press.
fn press_out(surface: &Rc<Surface>, press: &mut Press, fire: bool) {
    if !press.pressed {
        return;
    }
    press.pressed = false;
    if press.feedback.is_some() {
        fade(surface, &press.view, None);
    }
    if press.events.contains(EventKind::PressOut) {
        press_event(surface, press.node, EventKind::PressOut);
    }
    if fire && !press.long && press.events.contains(EventKind::Press) {
        press_event(surface, press.node, EventKind::Press);
    }
}

fn press_event(surface: &Surface, node: NodeId, kind: EventKind) {
    log::debug!(target: "nsh::input", "{} on {node}", kind.prop_name());
    surface.event(node, kind, EventPayload::None);
}

fn cancel_press(surface: &Rc<Surface>, p: &mut Pointer) {
    if let Some(mut press) = p.press.take() {
        press_out(surface, &mut press, false);
    }
}

/// Whether a release at `(x, y)` (root coordinates) is on `view`, its hit
/// slop and a small tolerance around them.
fn released_inside(surface: &Surface, view: &NshView, x: f32, y: f32) -> bool {
    let (lx, ly) = local(surface, view, x, y);
    let s = view.state();
    let slop = s.props.hit_slop;
    let r = PRESS_RETENTION;
    lx >= -slop.left - r
        && ly >= -slop.top - r
        && lx <= s.frame.width + slop.right + r
        && ly <= s.frame.height + slop.bottom + r
}

/// Fades `view` to `opacity` times its fade before the press, or back
/// (`None`), over a few frames.
fn fade(surface: &Rc<Surface>, view: &NshView, opacity: Option<f32>) {
    let (id, from) = {
        let s = view.state();
        (s.id, s.fade)
    };
    let generation = surface.input.next_gen.get();
    surface.input.next_gen.set(generation + 1);
    let mut fades = surface.input.fades.borrow_mut();
    let entry = fades.entry(id).or_insert(Fade {
        base: from,
        tick: None,
        generation,
    });
    entry.generation = generation;
    if let Some(tick) = entry.tick.take() {
        tick.remove();
    }
    let to = opacity.map_or(entry.base, |o| entry.base * o);
    let duration = if opacity.is_some() {
        FADE_IN_MS
    } else {
        FADE_OUT_MS
    };
    let releasing = opacity.is_none();
    let weak = Rc::downgrade(surface);
    let start = Cell::new(None::<f64>);
    entry.tick = Some(view.add_tick_callback(move |view, clock| {
        let now = clock.frame_time() as f64 / 1000.0;
        let t0 = start.get().unwrap_or(now);
        start.set(Some(t0));
        let k = ((now - t0) / duration).min(1.0) as f32;
        let eased = 1.0 - (1.0 - k) * (1.0 - k);
        view.state_mut().fade = from + (to - from) * eased;
        view.changed();
        if k < 1.0 {
            return glib::ControlFlow::Continue;
        }
        if let Some(surface) = weak.upgrade() {
            let mut fades = surface.input.fades.borrow_mut();
            if fades.get(&id).is_some_and(|f| f.generation == generation) {
                if releasing {
                    fades.remove(&id);
                } else if let Some(f) = fades.get_mut(&id) {
                    f.tick = None;
                }
            }
        }
        glib::ControlFlow::Break
    }));
}

// --- Pans ---------------------------------------------------------------------------

fn pan_payload(surface: &Surface, id: NodeId, t: (f32, f32), v: (f32, f32)) -> EventPayload {
    let (value_x, value_y) = surface.anim.borrow().pan_values(id);
    EventPayload::Pan {
        translation_x: t.0,
        translation_y: t.1,
        velocity_x: v.0,
        velocity_y: v.1,
        value_x,
        value_y,
    }
}

fn pan_start(surface: &Rc<Surface>, view: &NshView, p: &Pointer) {
    let (id, events) = {
        let s = view.state();
        (s.id, s.events)
    };
    surface.anim.borrow_mut().pan_began(id);
    if events.contains(EventKind::PanStart) {
        let payload = pan_payload(surface, id, (0.0, 0.0), p.velocity.velocity());
        surface.event(id, EventKind::PanStart, payload);
    }
}

fn pan_move(surface: &Rc<Surface>, view: &NshView, tx: f32, ty: f32) {
    let id = view.state().id;
    surface.anim.borrow_mut().pan_moved(id, tx, ty);
    surface.write_animations();
    surface.ensure_ticking();
}

/// Reports the end of a pan (`write`: the values may be written now; not
/// while a batch is applied).
fn pan_end(surface: &Rc<Surface>, view: &NshView, p: &Pointer, write: bool) {
    let (id, events) = {
        let s = view.state();
        (s.id, s.events)
    };
    if write {
        surface.write_animations();
    }
    if events.contains(EventKind::PanEnd) {
        let t = (p.x - p.start.0, p.y - p.start.1);
        let payload = pan_payload(surface, id, t, p.velocity.velocity());
        surface.event(id, EventKind::PanEnd, payload);
    }
}

// --- Pinch and rotation -------------------------------------------------------------

fn transform_payload(key: &'static str, amount: f32) -> EventPayload {
    EventPayload::Map(vec![
        (key, PayloadValue::Number(amount as f64)),
        ("velocity", PayloadValue::Number(0.0)),
    ])
}

fn transform_view(surface: &Surface, view: &NshView) -> Transform {
    let (id, events) = {
        let s = view.state();
        (s.id, s.events)
    };
    let (pinch, rotation) = transforms(&gesture_values(surface, view, id), events);
    Transform {
        view: view.clone(),
        pinch,
        rotation,
        scale: 1.0,
        degrees: 0.0,
    }
}

fn transform_begin(surface: &Surface, t: &Transform) {
    let (id, events) = {
        let s = t.view.state();
        (s.id, s.events)
    };
    {
        let mut anim = surface.anim.borrow_mut();
        if t.pinch {
            anim.transform_began(id, false);
        }
        if t.rotation {
            anim.transform_began(id, true);
        }
    }
    if t.pinch && events.contains(EventKind::PinchStart) {
        surface.event(id, EventKind::PinchStart, transform_payload("scale", 1.0));
    }
    if t.rotation && events.contains(EventKind::RotationStart) {
        let payload = transform_payload("rotation", 0.0);
        surface.event(id, EventKind::RotationStart, payload);
    }
}

fn transform_move(surface: &Rc<Surface>, t: &Transform) {
    let id = t.view.state().id;
    {
        let mut anim = surface.anim.borrow_mut();
        if t.pinch {
            anim.transform_moved(id, false, t.scale);
        }
        if t.rotation {
            anim.transform_moved(id, true, t.degrees);
        }
    }
    surface.write_animations();
    surface.ensure_ticking();
}

fn transform_end(surface: &Surface, t: &Transform) {
    let (id, events) = {
        let s = t.view.state();
        (s.id, s.events)
    };
    if t.pinch && events.contains(EventKind::PinchEnd) {
        surface.event(id, EventKind::PinchEnd, transform_payload("scale", t.scale));
    }
    if t.rotation && events.contains(EventKind::RotationEnd) {
        let payload = transform_payload("rotation", t.degrees);
        surface.event(id, EventKind::RotationEnd, payload);
    }
}

/// A second finger went down (`p`, not in the list yet): pinches /
/// rotates the view either finger would, ending what the first one did.
fn start_two_finger(surface: &Rc<Surface>, p: &mut Pointer) -> bool {
    let other = {
        let pointers = surface.input.pointers.borrow();
        let mut touches = pointers.iter().filter(|o| o.touch);
        match (touches.next(), touches.next()) {
            (Some(o), None) => o.key,
            _ => return false,
        }
    };
    let Some(mut o) = take(surface, other) else {
        return false;
    };
    let Some(view) = p.pinch.clone().or_else(|| o.pinch.clone()) else {
        put(surface, o);
        return false;
    };
    for q in [&mut *p, &mut o] {
        cancel_press(surface, q);
        release_pressed(surface, q);
    }
    match std::mem::replace(&mut o.mode, Mode::TwoFinger) {
        Mode::Pan(pan) => pan_end(surface, &pan, &o, true),
        Mode::Scroll(scroll, _) => crate::scroll::drag_end(surface, &scroll, 0.0, 0.0),
        _ => {}
    }
    p.mode = Mode::TwoFinger;
    let transform = transform_view(surface, &view);
    transform_begin(surface, &transform);
    *surface.input.two.borrow_mut() = Some(TwoFinger {
        keys: [o.key, p.key],
        distance: (p.x - o.x).hypot(p.y - o.y).max(1.0),
        angle: (p.y - o.y).atan2(p.x - o.x),
        transform,
    });
    put(surface, o);
    true
}

fn two_finger_moved(surface: &Rc<Surface>) {
    let transform = {
        let mut two = surface.input.two.borrow_mut();
        let Some(t) = two.as_mut() else { return };
        let positions = {
            let pointers = surface.input.pointers.borrow();
            let at = |key: Key| pointers.iter().find(|p| p.key == key).map(|p| (p.x, p.y));
            at(t.keys[0]).zip(at(t.keys[1]))
        };
        let Some((a, b)) = positions else { return };
        t.transform.scale = (b.0 - a.0).hypot(b.1 - a.1) / t.distance;
        t.transform.degrees = ((b.1 - a.1).atan2(b.0 - a.0) - t.angle).to_degrees();
        t.transform.clone()
    };
    transform_move(surface, &transform);
}

/// A finger of the pair lifted: the gesture ends, the other finger does
/// nothing more until it lifts.
fn end_two_finger(surface: &Rc<Surface>) {
    let Some(t) = surface.input.two.borrow_mut().take() else {
        return;
    };
    for p in surface.input.pointers.borrow_mut().iter_mut() {
        if t.keys.contains(&p.key) {
            p.mode = Mode::Free;
        }
    }
    transform_end(surface, &t.transform);
}

fn touchpad_pinch(surface: &Rc<Surface>, event: &gdk::TouchpadEvent, x: f32, y: f32) -> bool {
    match event.gesture_phase() {
        gdk::TouchpadGesturePhase::Begin => {
            let Some(hit) = pick(&surface.root, x, y) else {
                return false;
            };
            let view = chain(&hit.view).into_iter().find(|v| {
                let s = v.state();
                transforms(&gesture_values(surface, v, s.id), s.events) != (false, false)
            });
            let Some(view) = view else { return false };
            let t = transform_view(surface, &view);
            transform_begin(surface, &t);
            *surface.input.pinch.borrow_mut() = Some(t);
            true
        }
        gdk::TouchpadGesturePhase::Update => {
            let t = {
                let mut pinch = surface.input.pinch.borrow_mut();
                let Some(t) = pinch.as_mut() else {
                    return false;
                };
                t.scale = event.pinch_scale() as f32;
                t.degrees += event.pinch_angle_delta().to_degrees() as f32;
                t.clone()
            };
            transform_move(surface, &t);
            true
        }
        _ => {
            let Some(t) = surface.input.pinch.borrow_mut().take() else {
                return false;
            };
            transform_end(surface, &t);
            true
        }
    }
}

/// A Ctrl + wheel step of `dy` notches (negative: zooms in) at `(x, y)`:
/// pinches the view under it that pinches, ending once the steps stop.
/// Returns whether a view took it.
fn wheel_zoom(surface: &Rc<Surface>, x: f32, y: f32, dy: f32) -> bool {
    let input = &surface.input;
    let generation = input.next_gen.get();
    input.next_gen.set(generation + 1);
    let current = input.zoom.borrow_mut().take();
    let mut t = match current {
        Some((t, _)) => t,
        None => {
            let Some(hit) = pick(&surface.root, x, y) else {
                return false;
            };
            let view = chain(&hit.view).into_iter().find(|v| {
                let s = v.state();
                transforms(&gesture_values(surface, v, s.id), s.events).0
            });
            let Some(view) = view else { return false };
            let mut t = transform_view(surface, &view);
            t.rotation = false;
            transform_begin(surface, &t);
            t
        }
    };
    t.scale *= ZOOM_STEP.powf(-dy);
    transform_move(surface, &t);
    *input.zoom.borrow_mut() = Some((t, generation));
    let weak = Rc::downgrade(surface);
    glib::timeout_add_local_once(Duration::from_millis(ZOOM_IDLE), move || {
        let Some(surface) = weak.upgrade() else {
            return;
        };
        let ended = {
            let mut zoom = surface.input.zoom.borrow_mut();
            match zoom.as_ref() {
                Some((_, g)) if *g == generation => zoom.take(),
                _ => None,
            }
        };
        if let Some((t, _)) = ended {
            transform_end(&surface, &t);
        }
    });
    true
}

// --- Hover --------------------------------------------------------------------------

/// The mouse moved to `(x, y)`: updates `HOVERED` and raises `onHoverIn` /
/// `onHoverOut` along the views under it.
fn hover(surface: &Rc<Surface>, x: f32, y: f32) {
    let input = &surface.input;
    let views = pick(&surface.root, x, y)
        .map(|hit| chain(&hit.view))
        .unwrap_or_default();
    // The scroll views under the mouse show their indicators.
    for v in views.iter().filter(|v| v.kind() == ViewKind::Scroll) {
        crate::scroll::hovered(surface, v);
    }
    if input.hover_tracked.borrow().is_empty() && input.hovered.borrow().is_empty() {
        return;
    }
    let views = views.into_iter().filter(hoverable).collect();
    set_hovered(surface, views);
}

/// The views under the mouse are `views` (innermost first): those it left
/// hear so from the inside out, then those it came over from the outside
/// in, as with the DOM's `pointerleave` / `pointerenter`.
fn set_hovered(surface: &Surface, views: Vec<NshView>) {
    let old = std::mem::replace(&mut *surface.input.hovered.borrow_mut(), views.clone());
    for v in old.iter().filter(|v| !views.contains(v)) {
        set_state(surface, v, HOVERED, false);
        hover_event(surface, v, EventKind::HoverOut);
    }
    for v in views.iter().rev().filter(|v| !old.contains(v)) {
        set_state(surface, v, HOVERED, true);
        hover_event(surface, v, EventKind::HoverIn);
    }
}

/// `onHoverIn` / `onHoverOut` of `view`, if it listens.
fn hover_event(surface: &Surface, view: &NshView, kind: EventKind) {
    let (id, listens) = {
        let s = view.state();
        (s.id, s.events.contains(kind))
    };
    if listens {
        surface.event(id, kind, EventPayload::None);
    }
}

// --- Wheel and touchpad scrolling ---------------------------------------------------

/// Whether hosted widget `widget` (or one around it inside its view)
/// scrolls by itself.
fn scrolls_itself(widget: &gtk::Widget) -> bool {
    let mut current = Some(widget.clone());
    while let Some(w) = current {
        if w.is::<NshView>() {
            return false;
        }
        let ty = w.type_();
        if ty.name().contains("WebView") {
            // A page with `scrollEnabled: false` leaves wheels to the
            // scroll views around it.
            return !w.has_css_class(crate::native::UNSCROLLABLE);
        }
        if ty.is_a(gtk::Scrollable::static_type()) || ty.is_a(gtk::ScrolledWindow::static_type()) {
            return true;
        }
        current = w.parent();
    }
    false
}

/// The innermost scroll view at `(x, y)` that can move by `(dx, dy)`
/// (nested views fall through to outer ones at their edges; `None` too
/// when a hosted widget there scrolls itself). Scroll views along the
/// wheel's axis take it: a vertical wheel over a carousel scrolls the page
/// around it, as in browsers; horizontal views take vertical wheels only
/// where nothing around scrolls vertically.
fn scroll_target(surface: &Surface, x: f32, y: f32, dx: f32, dy: f32) -> Option<NshView> {
    let hit = pick(&surface.root, x, y)?;
    if hit.native.as_ref().is_some_and(scrolls_itself) {
        return None;
    }
    let views = chain(&hit.view);
    let horizontal = dx.abs() > dy.abs();
    let along = |v: &NshView| {
        v.state()
            .scroll
            .as_ref()
            .is_some_and(|s| s.horizontal == horizontal)
    };
    let any_along = views.iter().any(along);
    views.into_iter().find(|v| {
        v.kind() == ViewKind::Scroll
            && (!any_along || along(v))
            && crate::scroll::can_scroll(v, dx, dy)
    })
}

/// Points one wheel notch scrolls a view `extent` points long, as
/// GtkScrolledWindow does.
fn notch(extent: f32) -> f32 {
    extent.max(1.0).powf(2.0 / 3.0)
}

/// A scroll event at `(x, y)`; returns whether a scroll view took it.
fn wheel_input(
    surface: &Rc<Surface>,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    time: f64,
    kind: WheelInput,
) -> bool {
    let input = &surface.input;
    match kind {
        WheelInput::Step { notches } => {
            if dx == 0.0 && dy == 0.0 {
                return false;
            }
            // One scroll stays on its view, as in browsers: an inner view
            // passing under the pointer does not take it over, and the view
            // reaching its edge does not hand the rest to an outer one.
            let horizontal = dx.abs() > dy.abs();
            let latched = match &*input.latch.borrow() {
                Some((view, h, last))
                    if *h == horizontal && time - last < WHEEL_LATCH && view.is_mapped() =>
                {
                    Some(view.clone())
                }
                _ => None,
            };
            let Some(view) = latched.or_else(|| scroll_target(surface, x, y, dx, dy)) else {
                input.latch.borrow_mut().take();
                return false;
            };
            *input.latch.borrow_mut() = Some((view.clone(), horizontal, time));
            let (dx, dy) = if notches {
                let s = view.state();
                (dx * notch(s.frame.width), dy * notch(s.frame.height))
            } else {
                (dx, dy)
            };
            // Without a begin, the scroll module takes it as a mouse wheel:
            // it settles (and snaps) by itself once the notches stop.
            crate::scroll::wheel(surface, &view, dx, dy);
            true
        }
        WheelInput::Touchpad => {
            if dx == 0.0 && dy == 0.0 {
                return input.wheel.borrow().is_some();
            }
            // After a hold the scroll goes where it points: the held view
            // keeps it only if it scrolls that way.
            let held = match &*input.wheel.borrow() {
                Some(w) if w.held => Some(w.view.clone()),
                _ => None,
            };
            if let Some(held) = held {
                let target = scroll_target(surface, x, y, dx, dy);
                if target.as_ref() == Some(&held) {
                    *input.wheel.borrow_mut() = Some(new_wheel(held, time));
                } else {
                    input.wheel.borrow_mut().take();
                    crate::scroll::wheel_end(surface, &held, 0.0, 0.0);
                }
            }
            if input.wheel.borrow().is_none() {
                let Some(view) = scroll_target(surface, x, y, dx, dy) else {
                    return false;
                };
                *input.wheel.borrow_mut() = Some(new_wheel(view.clone(), time));
                crate::scroll::wheel_begin(surface, &view);
            }
            let view = {
                let mut wheel = input.wheel.borrow_mut();
                let Some(w) = wheel.as_mut() else {
                    return false;
                };
                w.total.0 += dx;
                w.total.1 += dy;
                w.velocity.add(time, w.total.0, w.total.1);
                w.view.clone()
            };
            crate::scroll::wheel(surface, &view, dx, dy);
            true
        }
        WheelInput::Stop => {
            let Some(mut w) = input.wheel.borrow_mut().take() else {
                return false;
            };
            w.velocity.add(time, w.total.0, w.total.1);
            let (vx, vy) = w.velocity.velocity();
            crate::scroll::wheel_end(surface, &w.view, vx, vy);
            true
        }
    }
}

fn new_wheel(view: NshView, time: f64) -> Wheel {
    let mut velocity = VelocityTracker::default();
    velocity.add(time, 0.0, 0.0);
    Wheel {
        view,
        total: (0.0, 0.0),
        velocity,
        held: false,
    }
}

/// Fingers resting on the touchpad stop a glide; lifting them without
/// scrolling ends that.
fn touchpad_hold(surface: &Rc<Surface>, phase: gdk::TouchpadGesturePhase, x: f32, y: f32) -> bool {
    let input = &surface.input;
    if phase == gdk::TouchpadGesturePhase::Begin {
        if input.wheel.borrow().is_some() {
            return false;
        }
        let Some(hit) = pick(&surface.root, x, y) else {
            return false;
        };
        let scroll = chain(&hit.view)
            .into_iter()
            .find(|v| v.kind() == ViewKind::Scroll);
        let Some(view) = scroll else {
            return false;
        };
        let mut wheel = new_wheel(view.clone(), 0.0);
        wheel.held = true;
        *input.wheel.borrow_mut() = Some(wheel);
        crate::scroll::wheel_begin(surface, &view);
        return true;
    }
    let held = input.wheel.borrow().as_ref().is_some_and(|w| w.held);
    if !held {
        return false;
    }
    if let Some(w) = input.wheel.borrow_mut().take() {
        crate::scroll::wheel_end(surface, &w.view, 0.0, 0.0);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_over_recent_samples() {
        let mut v = VelocityTracker::default();
        for i in 0..10 {
            v.add(i as f64 * 10.0, i as f32 * 5.0, 0.0);
        }
        let (vx, vy) = v.velocity();
        assert!((vx - 500.0).abs() < 1.0, "{vx}");
        assert_eq!(vy, 0.0);
    }

    #[test]
    fn velocity_is_zero_after_a_pause() {
        let mut v = VelocityTracker::default();
        v.add(0.0, 0.0, 0.0);
        v.add(10.0, 10.0, 10.0);
        v.add(300.0, 10.0, 10.0);
        assert_eq!(v.velocity(), (0.0, 0.0));
    }

    #[test]
    fn velocity_needs_two_samples() {
        let mut v = VelocityTracker::default();
        assert_eq!(v.velocity(), (0.0, 0.0));
        v.add(5.0, 3.0, 3.0);
        assert_eq!(v.velocity(), (0.0, 0.0));
    }

    #[test]
    fn velocity_wraps_around() {
        let mut v = VelocityTracker::default();
        for i in 0..100 {
            v.add(i as f64 * 8.0, 0.0, -(i as f32) * 4.0);
        }
        let (_, vy) = v.velocity();
        assert!((vy + 500.0).abs() < 1.0, "{vy}");
    }

    #[test]
    fn candidates_take_their_axis() {
        let horizontal = Candidate::Pan { x: true, y: false };
        assert!(horizontal.accepts(8.0, 2.0));
        assert!(!horizontal.accepts(2.0, 8.0));
        let both = Candidate::Pan { x: true, y: true };
        assert!(both.accepts(2.0, 8.0) && both.accepts(8.0, 2.0));
        let vertical = Candidate::Scroll { horizontal: false };
        assert!(vertical.accepts(1.0, -7.0));
        assert!(!vertical.accepts(-7.0, 1.0));
    }

    #[test]
    fn cursors() {
        assert_eq!(cursor_name(Cursor::Auto, false), None);
        assert_eq!(cursor_name(Cursor::Auto, true), Some("pointer"));
        assert_eq!(cursor_name(Cursor::Default, true), Some("default"));
        assert_eq!(cursor_name(Cursor::NotAllowed, false), Some("not-allowed"));
    }

    #[test]
    fn wheel_notches_scale_with_the_view() {
        assert!((notch(1000.0) - 100.0).abs() < 0.01);
        assert_eq!(notch(0.0), 1.0);
    }

    #[test]
    fn gates_decide_on_their_offsets() {
        let gate = Gate {
            id: 7,
            activation: PanActivation::horizontal(6.0, 8.0),
            outer: 0,
        };
        let mut candidates = Vec::new();
        assert_eq!(gate.decide(&mut candidates, 3.0, 3.0), (None, true));
        assert_eq!(gate.decide(&mut candidates, 7.0, 1.0), (Some(7), false));
        assert_eq!(gate.decide(&mut candidates, 2.0, -9.0), (None, false));
        // A fail offset passed by the same move wins.
        assert_eq!(gate.decide(&mut candidates, 9.0, 9.0), (None, false));
    }

    #[test]
    fn only_laid_out_views_bound_their_children() {
        assert!(laid_out(ViewKind::View, 8));
        assert!(laid_out(ViewKind::Text, 8));
        assert!(!laid_out(ViewKind::View, ROOT_ID));
        assert!(!laid_out(ViewKind::Screen, 8));
        assert!(!laid_out(ViewKind::Modal, 8));
    }

    #[test]
    fn press_events() {
        let mut events = EventMask::default();
        assert!(!has_press(events));
        events.set(EventKind::LongPress, true);
        assert!(has_press(events));
    }
}
