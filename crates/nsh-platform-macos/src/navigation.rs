//! Navigation stacks, screens with their headers, modals.
//!
//! A stack's screens are its child views; only the top one shows (and the
//! one below it while a transition runs): the others are hidden
//! (`NSView.hidden`), so they are neither drawn nor picked. Changes of the
//! app's screens are reconciled at the end of each batch, once every item
//! is applied: a new top screen slides in from the right over the previous
//! one (which moves left a bit); a top screen the app removes slides out as
//! the image it last showed (rendered from its layers when it is detached,
//! before the same batch tears its content down). The rendering leaves
//! out what other processes draw, web pages and videos: their live views
//! go with the image ([`crate::native::lend`]), so they do not go blank
//! as the screen (or a modal, which leaves the same way) slides out.
//!
//! The header is drawn by plain AppKit views the screen hosts
//! ([`NshView::set_native`]): a bar (with its hairline and the title) and a
//! borderless `NSButton` going back. The app's header items stay
//! `NshView`s (they have events of their own): children of the screen,
//! placed in the bar with their `shift` and kept above it. The header goes
//! over the screen's content, which the core laid out below
//! `header_height` (or under the bar for transparent headers and large
//! titles). Each screen also gets an opaque background under its content.
//!
//! Going back (the back button, Escape, Option+Left, Command+[, the
//! mouse's back button; Escape in a focused text field or web view only
//! leaves it) pops natively, then sends `onDismiss`, which removes the route in JS;
//! until the app removes the screen it is `popped` and ignored. With
//! `preventBack` the screen gets `onBackRequest` instead. The newest modal
//! comes first: a stack inside it pops, else it gets `onRequestClose`
//! (when `dismissible` it also closes natively, and gets `onDismiss` right
//! after).
//!
//! Modals are children of the surface's root, kept above the app's views,
//! at the size of the surface. They slide up or fade in, then get
//! `onShow`; when the app removes one it leaves as an image of its last
//! frame, then gets `onDismiss`. Page and form sheets are cards centered
//! in the window (rounded, with a shadow and a width of their own) over a
//! scrim that dims the window and takes its clicks: clicking it asks the
//! sheet to close, as going back does.
//!
//! Headers have a fixed height on a desktop, reported to the core
//! (`set_screen_header`) so screens lay their content out below the
//! header actually shown rather than the core's estimate.
//!
//! Transitions run on the display link ([`Surface::add_frame_callback`])
//! and only move views (`shift`, `fade`): nothing is laid out again while
//! they run.

use std::{cell::Cell, rc::Rc, sync::Arc};

use nsh_core::{
    Bounds, Color, EdgeInsets, EventKind, EventPayload, ModalAnimation, ModalConfig,
    ModalPresentation, NodeId, PayloadValue, Points, ROOT_ID, ScreenAnimation, ScreenConfig,
    Shadow, Size, ViewKind, ViewStyle,
    geometry::{Point, px},
};
use nsh_engine::ColorScheme;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::AnyObject,
};
use objc2_app_kit::{
    NSButton, NSCellImagePosition, NSColor, NSControl, NSEvent, NSEventModifierFlags, NSFont,
    NSFontWeightSemibold, NSImage, NSLineBreakMode, NSResponder, NSText, NSTextAlignment,
    NSTextField, NSView, NSWindowOrderingMode,
};
use objc2_core_foundation::{CFRetained, CGFloat};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage,
    CGImageAlphaInfo, CGImageByteOrderInfo,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSObject, NSObjectProtocol, NSPoint,
    NSRect, NSString,
};
use rustc_hash::FxHashMap;

use crate::{
    decor::cg_color,
    surface::Surface,
    view::{Container, NshView, ViewObj, ns_rect},
};

/// Height of a screen's header bar, normal and with a large title (a large
/// title gets a second row).
pub(crate) const HEADER_HEIGHTS: (f32, f32) = (47.0, 94.0);

/// The row of the bar holding the buttons and a normal title.
const BAR: f32 = HEADER_HEIGHTS.0;
/// Space between the bar's edges and its buttons.
const EDGE: f32 = 6.0;
/// Space between the bar's items.
const GAP: f32 = 8.0;
/// Push and pop (ms).
const SLIDE_MS: f64 = 300.0;
/// Cross-fades (and slides when the user asked for less motion).
const FADE_MS: f64 = 200.0;
/// Modals sliding up or down.
const MODAL_MS: f64 = 300.0;
/// How far the screen under a pushed one moves left, of the width.
const PARALLAX: f32 = 0.3;
/// The title's font size, normal and large (points).
const TITLE_SIZE: (f64, f64) = (15.0, 26.0);
/// Widest a page sheet and a form sheet get (a form sheet is also at most
/// [`FORM_HEIGHT`] tall), as iPad sheets.
const SHEET_WIDTH: (f32, f32) = (720.0, 540.0);
const FORM_HEIGHT: f32 = 620.0;
/// Space a sheet leaves to the window's edges (less in small windows).
const SHEET_MARGIN: f32 = 32.0;
/// Corner radius of a sheet.
const SHEET_RADIUS: f32 = 12.0;
/// How far below the window a sheet starts and ends, so that its shadow
/// is not seen at the bottom edge.
const SHADOW_ROOM: f32 = 48.0;

/// Colors of the bars and backgrounds, light or dark (AppKit's).
#[derive(Clone, Copy)]
struct Theme {
    window: Color,
    bar: Color,
    line: Color,
    fg: Color,
    /// Dims what is behind a sheet.
    scrim: Color,
    /// A sheet's background and hairline edge (the view's own style,
    /// under its rounded clip).
    sheet: Color,
    sheet_edge: Color,
}

fn theme(dark: bool) -> Theme {
    if dark {
        Theme {
            window: Color::rgba(30, 30, 30, 255),
            bar: Color::rgba(44, 44, 46, 255),
            line: Color::rgba(0, 0, 0, 128),
            fg: Color::rgba(255, 255, 255, 230),
            scrim: Color::rgba(0, 0, 0, 128),
            sheet: Color::rgba(36, 36, 38, 255),
            sheet_edge: Color::rgba(255, 255, 255, 26),
        }
    } else {
        Theme {
            window: Color::rgba(255, 255, 255, 255),
            bar: Color::rgba(246, 246, 246, 255),
            line: Color::rgba(0, 0, 0, 30),
            fg: Color::rgba(0, 0, 0, 217),
            scrim: Color::rgba(0, 0, 0, 82),
            sheet: Color::rgba(255, 255, 255, 255),
            sheet_edge: Color::rgba(0, 0, 0, 26),
        }
    }
}

fn ns_color(c: Color) -> Retained<NSColor> {
    let (r, g, b, a) = c.to_f64();
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a)
}

define_class!(
    /// A plain drawing navigation hosts (a header bar, its hairline, the
    /// last image of a screen or modal that is leaving): its layer's
    /// background or contents. The pointer passes through it.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NshNavFill"]
    struct Fill;

    unsafe impl NSObjectProtocol for Fill {}

    impl Fill {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(wantsUpdateLayer))]
        fn wants_update_layer(&self) -> bool {
            true
        }

        #[unsafe(method(updateLayer))]
        fn update_layer(&self) {}

        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            None
        }
    }
);

impl Fill {
    fn new(mtm: MainThreadMarker) -> Retained<Fill> {
        // SAFETY: NSView's designated initializer.
        let fill: Retained<Fill> =
            unsafe { msg_send![Fill::alloc(mtm), initWithFrame: NSRect::ZERO] };
        fill.setWantsLayer(true);
        fill
    }

    fn set_color(&self, color: Option<Color>) {
        if let Some(layer) = self.layer() {
            layer.setBackgroundColor(color.and_then(cg_color).as_deref());
        }
    }

    fn set_image(&self, image: &CGImage) {
        if let Some(layer) = self.layer() {
            // SAFETY: a CGImage is a Core Foundation object, and valid
            // layer contents.
            unsafe {
                let contents = &*(image as *const CGImage).cast::<AnyObject>();
                layer.setContents(Some(contents));
            }
        }
    }

    fn set_opacity(&self, opacity: f32) {
        if let Some(layer) = self.layer() {
            layer.setOpacity(opacity);
        }
    }
}

/// Ivars of [`Scrim`].
struct ScrimIvars {
    surface: u64,
    modal: NodeId,
    pressed: Cell<bool>,
}

define_class!(
    /// Dims the window behind a sheet and takes the pointer: a click asks
    /// the sheet to close.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ScrimIvars]
    #[name = "NshNavScrim"]
    struct Scrim;

    unsafe impl NSObjectProtocol for Scrim {}

    impl Scrim {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(wantsUpdateLayer))]
        fn wants_update_layer(&self) -> bool {
            true
        }

        #[unsafe(method(updateLayer))]
        fn update_layer(&self) {}

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            self.ivars().pressed.set(true);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {}

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            if !self.ivars().pressed.replace(false) {
                return;
            }
            let p = self.convertPoint_fromView(event.locationInWindow(), None);
            let b = self.bounds();
            let inside = p.x >= 0.0 && p.y >= 0.0 && p.x <= b.size.width && p.y <= b.size.height;
            if !inside {
                return;
            }
            let modal = self.ivars().modal;
            if let Some(surface) = Surface::with_key(self.ivars().surface)
                && let Ok(mut nav) = surface.nav.try_borrow_mut()
            {
                crate::surface::quiet(|| nav.request_close(&surface, modal));
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {}

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, _event: &NSEvent) {
            // What is behind the sheet does not scroll.
        }
    }
);

impl Scrim {
    fn new(mtm: MainThreadMarker, surface: u64, modal: NodeId) -> Retained<Scrim> {
        let scrim = Scrim::alloc(mtm).set_ivars(ScrimIvars {
            surface,
            modal,
            pressed: Cell::new(false),
        });
        // SAFETY: NSView's designated initializer.
        let scrim: Retained<Scrim> =
            unsafe { msg_send![super(scrim), initWithFrame: NSRect::ZERO] };
        scrim.setWantsLayer(true);
        scrim
    }

    fn set_color(&self, color: Color) {
        if let Some(layer) = self.layer() {
            layer.setBackgroundColor(cg_color(color).as_deref());
        }
    }

    fn set_opacity(&self, opacity: f32) {
        if let Some(layer) = self.layer() {
            layer.setOpacity(opacity);
        }
    }
}

/// Ivars of [`BackButton`].
struct BackIvars {
    surface: u64,
    screen: NodeId,
    pressed: Cell<bool>,
}

define_class!(
    /// A header's back button. It goes back on the mouse's release without
    /// `NSButton`'s modal tracking loop (which would hold the main thread
    /// while the mouse is down), or when accessibility presses it.
    #[unsafe(super(NSButton, NSControl, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = BackIvars]
    #[name = "NshNavBackButton"]
    struct BackButton;

    unsafe impl NSObjectProtocol for BackButton {}

    impl BackButton {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            self.ivars().pressed.set(true);
            self.highlight(true);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let inside = self.contains(event);
            self.highlight(inside);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.highlight(false);
            if self.ivars().pressed.replace(false) && self.contains(event) {
                self.go_back();
            }
        }

        #[unsafe(method(performClick:))]
        fn perform_click(&self, _sender: Option<&AnyObject>) {
            self.go_back();
        }
    }
);

impl BackButton {
    fn new(mtm: MainThreadMarker, surface: u64, screen: NodeId) -> Retained<BackButton> {
        let button = BackButton::alloc(mtm).set_ivars(BackIvars {
            surface,
            screen,
            pressed: Cell::new(false),
        });
        // SAFETY: NSView's designated initializer.
        unsafe { msg_send![super(button), initWithFrame: NSRect::ZERO] }
    }

    fn contains(&self, event: &NSEvent) -> bool {
        let p = self.convertPoint_fromView(event.locationInWindow(), None);
        let b = self.bounds();
        p.x >= 0.0 && p.y >= 0.0 && p.x <= b.size.width && p.y <= b.size.height
    }

    fn go_back(&self) {
        if let Some(surface) = Surface::with_key(self.ivars().surface) {
            crate::surface::quiet(|| go_back_from(&surface, self.ivars().screen));
        }
    }
}

/// An opaque background placed first in `view` (under its content). A
/// [`Container`] (an "own" view for [`NshView::insert_before`]), so the
/// app's views keep going above it, below the hosted AppKit views.
fn new_backdrop(view: &NshView) -> Retained<Container> {
    // SAFETY: NSView's designated initializer.
    let c: Retained<Container> =
        unsafe { msg_send![Container::alloc(view.mtm()), initWithFrame: NSRect::ZERO] };
    c.setWantsLayer(true);
    view.ns()
        .addSubview_positioned_relativeTo(&c, NSWindowOrderingMode::Below, None);
    c
}

fn set_layer_color(view: &NSView, color: Option<Color>) {
    if let Some(layer) = view.layer() {
        layer.setBackgroundColor(color.and_then(cg_color).as_deref());
    }
}

/// A screen's header views.
struct Header {
    /// The bar's fill (hosted by the screen), holding the hairline and the
    /// title.
    bar: Retained<Fill>,
    line: Retained<Fill>,
    title: Retained<NSTextField>,
    back: Retained<BackButton>,
    /// What it was laid out for (width, header height, the items' frames):
    /// unchanged, nothing moves.
    laid_out: Option<(f32, f32, [Bounds<Points>; 3])>,
}

/// One screen.
struct ScreenNav {
    view: NshView,
    config: ScreenConfig,
    /// Its opaque background, under the app's content.
    backdrop: Retained<Container>,
    header: Option<Header>,
    /// The header items the app inserted.
    items: Vec<NshView>,
    /// The back button shows (not the bottom screen, `back_visible`).
    can_go_back: bool,
    /// The back button's label (the previous screen's `back_title`).
    back_title: Option<Arc<str>>,
    /// The size and insets last reported.
    reported: Option<(Size<Points>, EdgeInsets)>,
    /// The header height last reported to the core (`None` inside: none
    /// measured, the core estimates it); `None`: never reported.
    header_reported: Option<Option<f32>>,
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
    /// The top screen's view: its image is kept when the app removes it.
    top: Option<NshView>,
    /// The image the top screen showed when the app took it away.
    image: Option<(NodeId, Shot)>,
}

/// One modal (kept while its node lives: the app may show it again).
#[derive(Default)]
struct ModalNav {
    config: ModalConfig,
    /// Its view while it is presented.
    view: Option<NshView>,
    /// The user closed it (`dismissible`) and it was reported: the app's
    /// removal only forgets it.
    closed: bool,
    backdrop: Option<Retained<Container>>,
    /// Dims the window behind a sheet; clicking it asks to close.
    scrim: Option<Retained<Scrim>>,
    reported: Option<(Size<Points>, EdgeInsets)>,
    /// The input or pressable view that had the keyboard focus when it was
    /// presented (a popover's trigger, a field; held weakly, as the app
    /// may remove it meanwhile): it gets the focus back when the modal
    /// closes, as on the other desktops.
    focus_before: Option<objc2::rc::Weak<ViewObj>>,
}

/// What leaves the screen during a pop.
enum Leaving {
    /// The screen itself (popped by the user).
    View(NshView),
    /// The image of a screen the app removed, hosted by the stack.
    Image(Shot),
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
        scrim: Option<Retained<Scrim>>,
        fade: bool,
    },
    ModalOut {
        /// The modal to report `onDismiss` for at the end (`None`: it was
        /// reported when the user closed it).
        dismiss: Option<NodeId>,
        image: Shot,
        /// Where the modal was (a sheet's card, or the whole window).
        frame: Bounds<Points>,
        scrim: Option<Retained<Scrim>>,
        fade: bool,
    },
}

struct Transition {
    /// Frame time of its first frame (seconds).
    start: Option<f64>,
    /// Milliseconds.
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

    /// Its progress at `now` (seconds; 0 to 1, not eased).
    fn progress(&self, now: f64) -> f64 {
        self.start
            .map_or(0.0, |s| (now - s) * 1000.0 / self.duration)
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
    /// The appearance the headers and backgrounds have.
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

/// Where a modal with `config` sits in a `width` × `height` window when it
/// is a sheet (`pageSheet` / `formSheet`, not transparent): a card centered
/// over the dimmed window, as wide as the window allows up to its kind's
/// width (a desktop window is no phone: a sheet sliding up over all of it
/// would look like a new page). `None`: it covers the window.
fn sheet_frame(config: &ModalConfig, width: f32, height: f32) -> Option<Bounds<Points>> {
    let (max_w, max_h) = match config.presentation {
        _ if !is_sheet(config) => return None,
        ModalPresentation::FullScreen => return None,
        ModalPresentation::PageSheet => (SHEET_WIDTH.0, f32::INFINITY),
        ModalPresentation::FormSheet => (SHEET_WIDTH.1, FORM_HEIGHT),
    };
    // Small windows keep a smaller margin, so the sheet stays usable.
    let margin = |extent: f32| SHEET_MARGIN.min((extent * 0.05).round()).max(8.0);
    let w = (width - 2.0 * margin(width)).min(max_w).max(0.0).round();
    let h = (height - 2.0 * margin(height)).min(max_h).max(0.0).round();
    Some(rect(
        ((width - w) / 2.0).round(),
        ((height - h) / 2.0).round(),
        w,
        h,
    ))
}

/// A modal with `config` is a sheet: a page or form sheet that is not
/// transparent (a transparent one covers the window, as on iOS).
fn is_sheet(config: &ModalConfig) -> bool {
    !config.transparent && config.presentation != ModalPresentation::FullScreen
}

/// The style a sheet's view gets: its background, rounded corners that
/// clip the content, a hairline edge and a soft shadow.
fn sheet_style(t: &Theme) -> ViewStyle {
    ViewStyle {
        background_color: t.sheet,
        border_radius: SHEET_RADIUS,
        overflow_hidden: true,
        borders: nsh_core::Borders {
            widths: [1.0; 4],
            colors: [t.sheet_edge; 4],
            ..nsh_core::Borders::NONE
        },
        shadow: Shadow {
            color: Color::BLACK,
            offset_x: 0.0,
            offset_y: 8.0,
            opacity: 0.35,
            radius: 24.0,
        },
        ..ViewStyle::default()
    }
}

/// The insets of a screen's content covered by its header: the core lays
/// the content out below a header, but under a transparent one or a large
/// title.
fn content_insets(c: &ScreenConfig) -> EdgeInsets {
    let under = c.header_transparent || c.large_title;
    let top = if c.header_shown && under {
        c.header_height.max(0.0)
    } else {
        0.0
    };
    EdgeInsets {
        top,
        ..EdgeInsets::default()
    }
}

/// The height of the header a screen shows, which its content starts
/// below (the core's estimate adds the safe area, which a desktop window
/// does not have, and stays a guess); `None` when the content is laid out
/// under it (large titles, transparent headers) or there is none.
fn measured_header(c: &ScreenConfig) -> Option<f32> {
    (c.header_shown && !c.large_title && !c.header_transparent).then_some(BAR)
}

fn insets_payload(i: EdgeInsets) -> EventPayload {
    let n = |v: f32| PayloadValue::Number(v as f64);
    EventPayload::Map(vec![
        ("top", n(i.top)),
        ("right", n(i.right)),
        ("bottom", n(i.bottom)),
        ("left", n(i.left)),
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
    if size.width <= px(0.0) || *last == Some((size, insets)) {
        return;
    }
    let insets_changed = last.is_none_or(|(_, i)| i != insets);
    *last = Some((size, insets));
    surface.control(|c| c.set_root_size(id, size, insets));
    if insets_changed {
        surface.event(id, EventKind::Insets, insets_payload(insets));
    }
}

/// Takes the keyboard focus away from inside `view` before it is hidden or
/// detached (a text field of a screen that is gone would keep typing),
/// giving it to the surface's root.
fn drop_focus(surface: &Surface, view: &NSView) {
    let Some(window) = view.window() else { return };
    let Some(responder) = window.firstResponder() else {
        return;
    };
    if responder
        .downcast_ref::<NSView>()
        .is_some_and(|r| r.isDescendantOf(view))
    {
        window.makeFirstResponder(Some(surface.root.ns()));
    }
}

/// The input or pressable view that has the keyboard focus in `surface`'s
/// window (not the root, which holds it when nothing else does). A field
/// edits in the window's shared field editor, a subview of the field only
/// while it edits, and a text view sits in a scroll view: the input is the
/// responder's nearest view of ours.
fn focused(surface: &Surface) -> Option<objc2::rc::Weak<ViewObj>> {
    let root = surface.root.ns();
    let responder = root.window()?.firstResponder()?;
    let mut at = responder.downcast::<NSView>().ok()?;
    if !at.isDescendantOf(root) {
        return None;
    }
    while &*at != root {
        if let Some(view) = NshView::from_ns(&at)
            && (view.kind() == ViewKind::Input || crate::input::is_key_control(&view))
        {
            let obj: &ViewObj = &view;
            return Some(objc2::rc::Weak::from(obj));
        }
        at = crate::view::superview(&at)?;
    }
    None
}

/// Gives the keyboard focus back to `view` (what had it before a modal
/// opened), if the focus fell to the root or the window meanwhile and
/// `view` can still take it inside `scope` (the newest modal still
/// presented, else the root). A field gets the caret at the end, as
/// `focus()` puts it (AppKit would select its whole text).
fn restore_focus(surface: &Surface, scope: &NSView, view: Option<objc2::rc::Weak<ViewObj>>) {
    let Some(view) = view
        .and_then(|w| w.load())
        .and_then(|v| NshView::from_ns(&v))
    else {
        return;
    };
    let Some(window) = surface.root.ns().window() else {
        return;
    };
    // Nothing else took it meanwhile: the root, or the window itself,
    // holds it.
    let at_root = window.firstResponder().is_none_or(|r| {
        r.downcast_ref::<NSView>()
            .is_none_or(|v| v == surface.root.ns())
    });
    if !at_root || view.ns().window().is_none() || !view.ns().isDescendantOf(scope) {
        return;
    }
    if view.kind() == ViewKind::Input {
        if view.can_target() && !view.ns().isHiddenOrHasHiddenAncestor() {
            crate::text_input::focus(&view);
        }
    } else if crate::input::is_key_control(&view) {
        window.makeFirstResponder(Some(view.ns()));
    }
}

/// Hides `view` (a screen not on top, a header item no slot names).
fn hide(surface: &Surface, view: &NshView) {
    if !view.ns().isHidden() {
        drop_focus(surface, view.ns());
        view.ns().setHidden(true);
    }
}

/// Shows `view` again (its layer's visibility follows its style).
fn show(view: &NshView) {
    if view.ns().isHidden() {
        view.ns().setHidden(false);
        view.place();
    }
}

fn set_shift(view: &NshView, shift: (f32, f32)) {
    let changed = std::mem::replace(&mut view.state_mut().shift, shift) != shift;
    if changed {
        view.place();
    }
}

fn set_fade(view: &NshView, fade: f32) {
    let changed = std::mem::replace(&mut view.state_mut().fade, fade) != fade;
    if changed {
        view.place();
    }
}

/// How far below the window `view` must go to be out of sight with its
/// shadow.
fn shadow_room(view: &NshView) -> f32 {
    if view.state().style.shadow.is_visible() {
        SHADOW_ROOM
    } else {
        0.0
    }
}

/// The header item of node `id`.
fn find_item(items: &[NshView], id: Option<NodeId>) -> Option<&NshView> {
    let id = id?;
    items.iter().find(|v| v.id() == id)
}

/// What `view` shows now, rendered from its layers (it may be detached:
/// layers keep what they drew).
fn image_of(surface: &Surface, view: &NshView) -> Option<CFRetained<CGImage>> {
    let layer = view.ns().layer()?;
    let f = view.state().frame;
    if f.size.width.0 <= 0.0 || f.size.height.0 <= 0.0 {
        return None;
    }
    let scale = surface
        .root
        .ns()
        .window()
        .map_or(2.0, |w| w.backingScaleFactor());
    let (pw, ph) = (
        (f.size.width.0 as f64 * scale).ceil() as usize,
        (f.size.height.0 as f64 * scale).ceil() as usize,
    );
    let space = CGColorSpace::new_device_rgb()?;
    // SAFETY: CoreGraphics allocates the pixels (null data) for an RGBA
    // context of these dimensions; the layer is rendered on the main
    // thread, which owns it.
    unsafe {
        let context = CGBitmapContextCreate(
            std::ptr::null_mut(),
            pw,
            ph,
            8,
            pw * 4,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
        )?;
        // Layers of flipped views draw y down.
        CGContext::translate_ctm(Some(&context), 0.0, ph as CGFloat);
        CGContext::scale_ctm(Some(&context), scale, -scale);
        // The view's own transform and opacity are where it is placed, not
        // what it shows.
        let opacity = layer.opacity();
        layer.setOpacity(1.0);
        layer.renderInContext(&context);
        layer.setOpacity(opacity);
        CGBitmapContextCreateImage(Some(&context))
    }
}

/// The last image of a screen or modal that leaves, hosted by navigation
/// while it leaves. `renderInContext:` draws no web page (WebKit's process
/// draws them), so the live web views it showed are lent to it
/// ([`crate::native::lend`]), and given back or let go with it.
struct Shot {
    fill: Retained<Fill>,
    _lent: Vec<crate::native::Lent>,
}

impl std::ops::Deref for Shot {
    type Target = Fill;

    fn deref(&self) -> &Fill {
        &self.fill
    }
}

/// What `view` shows now, as a view of its own to host while it leaves
/// (`view` may be detached; it is not changed but for its web views).
fn shot(surface: &Rc<Surface>, view: &NshView) -> Option<Shot> {
    let image = image_of(surface, view)?;
    let fill = Fill::new(surface.mtm);
    fill.set_image(&image);
    let lent = crate::native::lend(surface, view, &fill);
    Some(Shot { fill, _lent: lent })
}

/// Moves `tail` to the end of `parent`'s subviews (drawn last), unless
/// they are there already (moving a view out of place would cost its
/// focus).
fn keep_last(parent: &NSView, tail: &[Retained<NSView>]) {
    let subviews = parent.subviews();
    let n = subviews.len();
    let in_place = tail.len() <= n
        && tail
            .iter()
            .enumerate()
            .all(|(i, v)| *subviews.objectAtIndex(n - tail.len() + i) == **v);
    if !in_place {
        for v in tail {
            parent.addSubview_positioned_relativeTo(v, NSWindowOrderingMode::Above, None);
        }
    }
}

fn attributed(text: &str, font: &NSFont, color: &NSColor) -> Retained<NSAttributedString> {
    // SAFETY: the attribute keys are AppKit's constants; the values have
    // the types they require.
    unsafe {
        let keys: [&NSAttributedStringKey; 2] = [
            objc2_app_kit::NSFontAttributeName,
            objc2_app_kit::NSForegroundColorAttributeName,
        ];
        let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
        let attrs = NSDictionary::from_slices(&keys, &values);
        NSAttributedString::new_with_attributes(&NSString::from_str(text), &attrs)
    }
}

fn size_of(view: &NSView) -> (f32, f32) {
    let s = view.fittingSize();
    (s.width.ceil() as f32, s.height.ceil() as f32)
}

impl Header {
    fn new(surface: &Rc<Surface>, screen: NodeId) -> Self {
        let mtm = surface.mtm;
        let bar = Fill::new(mtm);
        let line = Fill::new(mtm);
        bar.addSubview(&line);
        let title = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        title.setDrawsBackground(false);
        bar.addSubview(&title);
        let back = BackButton::new(mtm, surface.key(), screen);
        back.setBordered(false);
        back.setRefusesFirstResponder(true);
        if let Some(chevron) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str("chevron.left"),
            Some(&NSString::from_str("Back")),
        ) {
            back.setImage(Some(&chevron));
        }
        back.setImagePosition(NSCellImagePosition::ImageLeading);
        Header {
            bar,
            line,
            title,
            back,
            laid_out: None,
        }
    }

    fn natives(&self) -> [Retained<NSView>; 2] {
        [
            Retained::into_super(Retained::into_super(Retained::into_super(
                self.back.clone(),
            ))),
            Retained::into_super(self.bar.clone()),
        ]
    }
}

impl ScreenNav {
    fn new(view: &NshView) -> Self {
        ScreenNav {
            view: view.clone(),
            config: ScreenConfig::default(),
            backdrop: new_backdrop(view),
            header: None,
            items: Vec::new(),
            can_go_back: false,
            back_title: None,
            reported: None,
            header_reported: None,
            backdrop_size: (-1.0, -1.0),
        }
    }

    fn header_shown(&self) -> bool {
        self.config.header_shown && self.config.header_height > 0.0
    }

    fn report(&mut self, surface: &Surface, stack: &NshView) {
        let header = measured_header(&self.config);
        if self.header_reported.replace(header) != Some(header) {
            let id = self.view.id();
            surface.control(|c| c.set_screen_header(id, header));
        }
        let f = stack.state().frame;
        let insets = content_insets(&self.config);
        let id = self.view.id();
        let size = f.size;
        report(surface, id, &mut self.reported, size, insets);
    }

    /// Applies the config's colors and texts to the header (builds or
    /// removes it) and the appearance to the background.
    fn style(&mut self, surface: &Rc<Surface>, dark: bool) {
        let t = theme(dark);
        set_layer_color(&self.backdrop, Some(t.window));
        if !self.header_shown() {
            if let Some(header) = self.header.take() {
                for view in header.natives() {
                    self.view.remove_native(&view);
                }
            }
            for item in &self.items {
                hide(surface, item);
            }
            self.layout(surface);
            return;
        }
        let id = self.view.id();
        let header = self.header.get_or_insert_with(|| Header::new(surface, id));
        let c = &self.config;
        if c.header_transparent {
            header.bar.set_color(None);
            header.line.set_color(None);
        } else {
            header
                .bar
                .set_color(Some(c.background_color.unwrap_or(t.bar)));
            header.line.set_color(Some(t.line));
        }
        let fg = ns_color(c.title_color.unwrap_or(t.fg));
        let size = if c.large_title {
            TITLE_SIZE.1
        } else {
            TITLE_SIZE.0
        };
        // SAFETY: an AppKit constant.
        let weight = unsafe { NSFontWeightSemibold };
        let font = NSFont::systemFontOfSize_weight(size as CGFloat, weight);
        let title = c.title.as_deref().unwrap_or("");
        header
            .title
            .setAttributedStringValue(&attributed(title, &font, &fg));
        let tint = ns_color(c.tint_color.unwrap_or(t.fg));
        header.back.setContentTintColor(Some(&tint));
        let back_title = self.back_title.as_deref().unwrap_or("");
        let font = NSFont::systemFontOfSize(TITLE_SIZE.0 as CGFloat);
        header
            .back
            .setAttributedTitle(&attributed(back_title, &font, &tint));
        header.back.setImagePosition(if back_title.is_empty() {
            NSCellImagePosition::ImageOnly
        } else {
            NSCellImagePosition::ImageLeading
        });
        header.laid_out = None;
        self.layout(surface);
    }

    /// Places the background, the header's views and items (when something
    /// changed).
    fn layout(&mut self, surface: &Surface) {
        let (w, h) = {
            let f = self.view.state().frame;
            (f.size.width.0, f.size.height.0)
        };
        if self.backdrop_size != (w, h) {
            self.backdrop_size = (w, h);
            self.backdrop.setFrame(ns_rect(0.0, 0.0, w, h));
        }
        let c = &self.config;
        let frames = [c.left, c.right, c.title_view]
            .map(|id| find_item(&self.items, id).map_or(Bounds::default(), |v| v.state().frame));
        let Some(header) = &mut self.header else {
            return;
        };
        let key = (w, c.header_height, frames);
        if header.laid_out == Some(key) {
            return;
        }
        header.laid_out = Some(key);
        let view = &self.view;
        let height = c.header_height;
        let rows = if c.large_title { 2.0 } else { 1.0 };
        // The status area (none on desktops) is above the bar.
        let top = (height - BAR * rows).max(0.0);
        view.set_native(&header.bar, rect(0.0, 0.0, w, height));
        header.line.setFrame(ns_rect(0.0, height - 1.0, w, 1.0));
        let centered = |h: f32| top + ((BAR - h) / 2.0).max(0.0);
        let place = |v: &NshView, x: f32| {
            let f = v.state().frame;
            show(v);
            set_shift(
                v,
                (x - f.origin.x.0, centered(f.size.height.0) - f.origin.y.0),
            );
        };
        let mut left = EDGE;
        header.back.setHidden(!self.can_go_back);
        if self.can_go_back {
            let (bw, bh) = size_of(&header.back);
            let (bw, bh) = (bw + 12.0, bh.max(24.0));
            view.set_native(&header.back, rect(EDGE, centered(bh), bw, bh));
            left += bw;
        } else {
            view.set_native(&header.back, Bounds::default());
        }
        if let Some(v) = find_item(&self.items, c.left) {
            let x = left + GAP;
            left = x + v.state().frame.size.width.0;
            place(v, x);
        }
        let mut right = w;
        if let Some(v) = find_item(&self.items, c.right) {
            right = w - EDGE - GAP - v.state().frame.size.width.0;
            place(v, right);
        }
        // Items no slot names stay hidden.
        for item in &self.items {
            let id = Some(item.id());
            if id != c.left && id != c.right && id != c.title_view {
                hide(surface, item);
            }
        }
        if let Some(v) = find_item(&self.items, c.title_view) {
            let fw = v.state().frame.size.width.0;
            place(v, ((w - fw) / 2.0).max(left));
            header.title.setHidden(true);
            return;
        }
        header.title.setHidden(false);
        // Labels truncate at exactly their fitting width.
        let (tw, th) = size_of(&header.title);
        let tw = tw + 4.0;
        if c.large_title {
            let r = rect(16.0, top + BAR + (BAR - th) / 2.0, (w - 32.0).max(0.0), th);
            header.title.setAlignment(NSTextAlignment::Left);
            header.title.setFrame(ns_rect(
                r.origin.x.0,
                r.origin.y.0,
                r.size.width.0,
                r.size.height.0,
            ));
        } else {
            // Centered in the bar, as long as it fits between the items.
            let side = left.max(w - right) + GAP;
            let tw = tw.min((w - 2.0 * side).max(0.0));
            header.title.setAlignment(NSTextAlignment::Center);
            let r = rect((w - tw) / 2.0, centered(th), tw, th);
            header.title.setFrame(ns_rect(
                r.origin.x.0,
                r.origin.y.0,
                r.size.width.0,
                r.size.height.0,
            ));
        }
    }

    /// Keeps the header over the app's content (which the app may insert
    /// after it) and the items over the header.
    fn raise_header(&self) {
        if self.header.is_none() || self.items.is_empty() {
            return;
        }
        let tail: Vec<Retained<NSView>> =
            self.items.iter().map(|v| Retained::from(v.ns())).collect();
        keep_last(self.view.ns(), &tail);
    }
}

impl Nav {
    /// Forgets the app's views (it stopped); a running frame callback
    /// stays.
    pub(crate) fn reset(&mut self) {
        // The views navigation hosts in the root (the mounter only takes
        // the app's views away).
        let mut hosted: Vec<Retained<NSView>> = Vec::new();
        for m in self.modals.values() {
            hosted.extend(m.scrim.iter().map(|s| Retained::into_super(s.clone())));
        }
        for t in &self.transitions {
            if let Motion::ModalOut { image, scrim, .. } = &t.motion {
                hosted.push(Retained::into_super(image.fill.clone()));
                hosted.extend(scrim.iter().map(|s| Retained::into_super(s.clone())));
            }
        }
        for view in hosted {
            match crate::view::superview(&view).and_then(|s| NshView::from_ns(&s)) {
                Some(root) => root.remove_native(&view),
                None => view.removeFromSuperview(),
            }
        }
        *self = Nav {
            ticking: self.ticking,
            ..Nav::default()
        };
    }

    fn screen(&mut self, view: &NshView) -> &mut ScreenNav {
        self.screens
            .entry(view.id())
            .or_insert_with(|| ScreenNav::new(view))
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
                    Leaving::Image(image) => {
                        let x = if *fade {
                            image.set_opacity(1.0 - e);
                            0.0
                        } else {
                            f.size.width.0 * e
                        };
                        stack.set_native(image, rect(x, 0.0, f.size.width.0, f.size.height.0));
                    }
                }
                if let (Some(below), false) = (below, fade) {
                    set_shift(below, (-PARALLAX * f.size.width.0 * (1.0 - e), 0.0));
                }
            }
            Motion::ModalIn { view, scrim, fade } => {
                if *fade {
                    set_fade(view, e);
                } else {
                    // From below the window's bottom edge (a sheet's
                    // shadow too).
                    let h = surface.root.state().frame.size.height.0 + shadow_room(view);
                    let y = view.state().frame.origin.y.0;
                    set_shift(view, (0.0, (h - y) * (1.0 - e)));
                }
                if let Some(scrim) = scrim {
                    scrim.set_opacity(e);
                }
            }
            Motion::ModalOut {
                image,
                frame,
                scrim,
                fade,
                ..
            } => {
                // Sheets leave with their shadow.
                let room = if scrim.is_some() { SHADOW_ROOM } else { 0.0 };
                let h = surface.root.state().frame.size.height.0 + room;
                let y = if *fade {
                    image.set_opacity(1.0 - e);
                    frame.origin.y.0
                } else {
                    frame.origin.y.0 + (h - frame.origin.y.0) * e
                };
                surface.root.set_native(
                    image,
                    rect(frame.origin.x.0, y, frame.size.width.0, frame.size.height.0),
                );
                if let Some(scrim) = scrim {
                    scrim.set_opacity(1.0 - e);
                }
            }
        }
    }

    fn end(&mut self, surface: &Surface, t: Transition) {
        self.apply(surface, &t.motion, 1.0);
        match t.motion {
            Motion::Push { stack, .. } => self.settle(surface, stack.id()),
            Motion::Pop {
                stack,
                leaving,
                popped,
                ..
            } => {
                match leaving {
                    Leaving::View(v) => {
                        hide(surface, &v);
                        set_shift(&v, (0.0, 0.0));
                        set_fade(&v, 1.0);
                    }
                    Leaving::Image(image) => stack.remove_native(&image),
                }
                self.settle(surface, stack.id());
                if let Some(id) = popped {
                    surface.event(id, EventKind::Dismiss, EventPayload::None);
                }
            }
            Motion::ModalIn { view, .. } => {
                set_shift(&view, (0.0, 0.0));
                set_fade(&view, 1.0);
                surface.event(view.id(), EventKind::Show, EventPayload::None);
            }
            Motion::ModalOut {
                dismiss,
                image,
                scrim,
                ..
            } => {
                surface.root.remove_native(&image);
                if let Some(scrim) = scrim {
                    surface.root.remove_native(&scrim);
                }
                if let Some(id) = dismiss {
                    surface.event(id, EventKind::Dismiss, EventPayload::None);
                }
            }
        }
    }

    /// Shows only the top screen of `stack`, where it belongs.
    fn settle(&mut self, surface: &Surface, stack: NodeId) {
        let Some(st) = self.stacks.get_mut(&stack) else {
            return;
        };
        let top = st.shown.last().copied();
        st.top = None;
        for view in st.view.child_views() {
            if view.kind() != ViewKind::Screen {
                continue;
            }
            set_shift(&view, (0.0, 0.0));
            set_fade(&view, 1.0);
            if Some(view.id()) == top {
                show(&view);
                st.top = Some(view);
            } else {
                hide(surface, &view);
            }
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
        surface.add_frame_callback(|surface, now| {
            let Ok(mut nav) = surface.nav.try_borrow_mut() else {
                return true;
            };
            nav.tick(surface, now);
            if nav.transitions.is_empty() {
                nav.ticking = false;
                false
            } else {
                true
            }
        });
    }

    fn tick(&mut self, surface: &Surface, now: f64) {
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
        let image = st
            .image
            .take()
            .filter(|(id, _)| Some(*id) == old && Some(*id) != new && find(*id).is_none())
            .map(|(_, image)| image);
        let leaving_animation = old
            .and_then(|id| self.screens.get(&id))
            .map(|s| s.config.animation);
        for id in &prev {
            if find(*id).is_none() {
                self.screens.remove(id);
            }
        }
        // Back buttons and their labels follow the order.
        let mut previous_back: Option<Arc<str>> = None;
        for (i, &id) in desired.iter().enumerate() {
            let Some(view) = find(id) else { continue };
            let s = self.screen(&view);
            let can_go_back = i > 0 && s.config.back_visible;
            let back_title = previous_back.take().filter(|_| i > 0);
            previous_back = s.config.back_title.clone();
            if s.can_go_back != can_go_back || s.back_title != back_title {
                s.can_go_back = can_go_back;
                s.back_title = back_title;
                s.style(surface, dark);
            }
            if !prev.contains(&id) {
                s.report(surface, &stack);
            }
        }
        if children.is_empty() && !stack.has_parent() {
            // The stack is gone.
            self.stacks.remove(&stack_id);
            return;
        }
        self.settle(surface, stack_id);
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
                show(b);
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
                    show(&v);
                    Leaving::View(v)
                }
                None => {
                    let Some(image) = image else { return };
                    Leaving::Image(image)
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

    /// The user goes back in `stack`: tells the screen if it prevents it,
    /// else pops it natively and tells the app once it is gone.
    fn back(&mut self, surface: &Rc<Surface>, stack_id: NodeId) {
        self.finish(surface, |t| t.stack() == Some(stack_id));
        let Some(st) = self.stacks.get_mut(&stack_id) else {
            return;
        };
        let Some(&top) = st.shown.last() else { return };
        if self
            .screens
            .get(&top)
            .is_some_and(|s| s.config.prevent_back)
        {
            surface.event(top, EventKind::BackRequest, EventPayload::None);
            return;
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
        st.top = below.clone();
        let animation = self.screens.get(&top).map(|s| s.config.animation);
        match (
            Self::screen_motion(surface, animation.unwrap_or_default()),
            find(top),
        ) {
            (Some((duration, fade)), Some(leaving)) => {
                if let Some(b) = &below {
                    show(b);
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
                self.settle(surface, stack_id);
                surface.event(top, EventKind::Dismiss, EventPayload::None);
            }
        }
    }

    /// The stack going back acts on: the innermost shown one that can pop
    /// or asks first, inside `within` (the newest modal) when given.
    fn back_stack(&self, within: Option<&NshView>) -> Option<NodeId> {
        let wants = |st: &StackNav| {
            let ns = st.view.ns();
            ns.window().is_some()
                && !ns.isHiddenOrHasHiddenAncestor()
                && within.is_none_or(|w| ns.isDescendantOf(w.ns()))
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
                    .any(|o| o.view != c.view && o.view.ns().isDescendantOf(c.view.ns()))
            })
            .map(|c| c.view.id())
    }

    /// Where the keyboard focus may go ([`focus_scope`]).
    fn scope(&self, surface: &Surface) -> Retained<NSView> {
        self.order
            .iter()
            .rev()
            .find_map(|id| self.modals.get(id)?.view.as_ref())
            .map_or_else(
                || Retained::from(surface.root.ns()),
                |v| Retained::from(v.ns()),
            )
    }

    /// Takes modal `id` away natively (it leaves as an image of its last
    /// frame), then reports `onDismiss` unless `reported` already.
    fn close_modal(&mut self, surface: &Rc<Surface>, id: NodeId, reported: bool) {
        self.finish(
            surface,
            |t| matches!(&t.motion, Motion::ModalIn { view, .. } if view.id() == id),
        );
        let newer: Vec<NodeId> = self
            .order
            .iter()
            .skip_while(|&&m| m != id)
            .skip(1)
            .copied()
            .collect();
        self.order.retain(|&m| m != id);
        let Some(m) = self.modals.get_mut(&id) else {
            return;
        };
        let Some(view) = m.view.take() else { return };
        let scrim = m.scrim.take();
        let focus_before = m.focus_before.take();
        let frame = view.state().frame;
        let motion = modal_motion(surface, m.config.animation);
        let shown = view.ns().window().is_some();
        let image = motion.filter(|_| shown).and_then(|_| shot(surface, &view));
        if view.has_parent() {
            drop_focus(surface, view.ns());
            view.unparent();
        }
        if let Some(b) = m.backdrop.take() {
            b.removeFromSuperview();
        }
        // A newer modal still covers what had the focus: one that remembers
        // a view of this modal (gone now) gives this one's back instead.
        for newer in newer {
            if let Some(n) = self.modals.get_mut(&newer)
                && n.focus_before
                    .as_ref()
                    .and_then(|w| w.load())
                    .is_some_and(|v| v.isDescendantOf(view.ns()))
            {
                n.focus_before = focus_before.clone();
            }
        }
        restore_focus(surface, &self.scope(surface), focus_before);
        set_shift(&view, (0.0, 0.0));
        set_fade(&view, 1.0);
        match (motion, image) {
            (Some((duration, fade)), Some(image)) => {
                surface.root.set_native(
                    &image,
                    rect(
                        frame.origin.x.0,
                        frame.origin.y.0,
                        frame.size.width.0,
                        frame.size.height.0,
                    ),
                );
                let motion = Motion::ModalOut {
                    dismiss: (!reported).then_some(id),
                    image,
                    frame,
                    scrim,
                    fade,
                };
                self.start(surface, duration, motion);
            }
            _ => {
                if let Some(scrim) = scrim {
                    surface.root.remove_native(&scrim);
                }
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
        let scrim = m.scrim.clone();
        match modal_motion(surface, m.config.animation) {
            Some((duration, fade)) => {
                self.start(surface, duration, Motion::ModalIn { view, scrim, fade });
            }
            None => surface.event(id, EventKind::Show, EventPayload::None),
        }
    }

    /// The user asks modal `id` to close (going back, its sheet's scrim):
    /// it hears `onRequestClose`. A `dismissible` one also leaves natively
    /// and hears `onDismiss` right after, as on iOS (the runtime keeps the
    /// handler of a modal the app removed in between).
    fn request_close(&mut self, surface: &Rc<Surface>, id: NodeId) {
        surface.event(id, EventKind::RequestClose, EventPayload::None);
        if let Some(m) = self.modals.get_mut(&id)
            && m.config.dismissible
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

    /// Keeps the presented modals (and those leaving) over the app's
    /// views, each sheet right above its scrim.
    fn raise_modals(&self, surface: &Surface) {
        let mut tail: Vec<Retained<NSView>> = Vec::new();
        for m in self.order.iter().filter_map(|id| self.modals.get(id)) {
            if let Some(scrim) = &m.scrim {
                tail.push(Retained::into_super(scrim.clone()));
            }
            if let Some(view) = &m.view {
                tail.push(Retained::from(view.ns()));
            }
        }
        for t in &self.transitions {
            if let Motion::ModalOut { image, scrim, .. } = &t.motion {
                if let Some(scrim) = scrim {
                    tail.push(Retained::into_super(scrim.clone()));
                }
                tail.push(Retained::into_super(image.fill.clone()));
            }
        }
        keep_last(surface.root.ns(), &tail);
    }

    /// Sizes modal `id` to the surface (a sheet to its card, over a scrim
    /// covering the window) and reports it.
    fn size_modal(&mut self, surface: &Surface, id: NodeId) {
        let Some(m) = self.modals.get_mut(&id) else {
            return;
        };
        let Some(view) = &m.view else { return };
        let f = surface.root.state().frame;
        let sheet = sheet_frame(&m.config, f.size.width.0, f.size.height.0);
        let insets = if sheet.is_some() {
            // A card inside the window: no system UI over it.
            EdgeInsets::default()
        } else {
            let safe = surface.environment.get().safe_area;
            EdgeInsets {
                top: safe.top.0,
                right: safe.right.0,
                bottom: safe.bottom.0,
                left: safe.left.0,
            }
        };
        let frame = sheet.unwrap_or(rect(0.0, 0.0, f.size.width.0, f.size.height.0));
        if std::mem::replace(&mut view.state_mut().frame, frame) != frame {
            view.moved();
        }
        if let Some(b) = &m.backdrop {
            b.setFrame(ns_rect(0.0, 0.0, frame.size.width.0, frame.size.height.0));
        }
        if let Some(scrim) = &m.scrim {
            surface
                .root
                .set_native(scrim, rect(0.0, 0.0, f.size.width.0, f.size.height.0));
        }
        let size = frame.size;
        report(surface, id, &mut m.reported, size, insets);
    }

    /// Gives presented modal `id` the scrim of a sheet, or takes it away.
    fn sheet_scrim(&mut self, surface: &Rc<Surface>, id: NodeId) {
        let Some(m) = self.modals.get_mut(&id) else {
            return;
        };
        if m.view.is_none() {
            return;
        }
        if !is_sheet(&m.config) {
            if let Some(scrim) = m.scrim.take() {
                surface.root.remove_native(&scrim);
            }
            return;
        }
        if m.scrim.is_some() {
            return;
        }
        let scrim = Scrim::new(surface.mtm, surface.key(), id);
        scrim.set_color(theme(dark(surface)).scrim);
        let f = surface.root.state().frame;
        surface
            .root
            .set_native(&scrim, rect(0.0, 0.0, f.size.width.0, f.size.height.0));
        m.scrim = Some(scrim);
        // Right under the sheet.
        self.raise_modals(surface);
    }

    /// Gives modal `id` its opaque background, unless it is transparent: a
    /// sheet's is its own (a rounded card with a shadow), a full-screen
    /// modal's a view under its content.
    fn style_modal(&mut self, dark: bool, id: NodeId) {
        let Some(m) = self.modals.get_mut(&id) else {
            return;
        };
        let Some(view) = &m.view else { return };
        let t = theme(dark);
        let sheet = is_sheet(&m.config);
        let style = sheet.then(|| sheet_style(&t));
        let restyle = {
            let current = &view.state().style;
            match &style {
                Some(style) => **current != *style,
                // Back from a sheet: the modal itself has no style.
                None => current.overflow_hidden && current.border_radius == SHEET_RADIUS,
            }
        };
        if restyle {
            let old = std::mem::replace(
                &mut view.state_mut().style,
                Box::new(style.unwrap_or_default()),
            );
            view.moved();
            crate::decor::style_changed(view, &old);
        }
        if let Some(scrim) = &m.scrim {
            scrim.set_color(t.scrim);
        }
        if m.config.transparent || sheet {
            if let Some(b) = m.backdrop.take() {
                b.removeFromSuperview();
            }
            return;
        }
        let backdrop = m.backdrop.get_or_insert_with(|| new_backdrop(view));
        set_layer_color(backdrop, Some(t.window));
        let f = view.state().frame;
        backdrop.setFrame(ns_rect(0.0, 0.0, f.size.width.0, f.size.height.0));
    }
}

/// Where the keyboard focus may go: the newest presented modal (what is
/// under it cannot be used), else the whole surface.
pub(crate) fn focus_scope(surface: &Surface) -> Retained<NSView> {
    match surface.nav.try_borrow() {
        Ok(nav) => nav.scope(surface),
        Err(_) => Retained::from(surface.root.ns()),
    }
}

/// Goes back (the newest modal, the innermost stack or the app's
/// `onBackPress`): whether something took it. The mouse's back button
/// calls it too.
pub(crate) fn go_back(surface: &Rc<Surface>) -> bool {
    match surface.nav.try_borrow_mut() {
        Ok(mut nav) => crate::surface::quiet(|| nav.go_back(surface)),
        Err(_) => false,
    }
}

/// `Command::Back`: goes back as without the app's listeners, once the
/// batch being applied is in.
pub(crate) fn default_back(surface: &Rc<Surface>) {
    let key = surface.key();
    dispatch2::DispatchQueue::main().exec_async(move || {
        if let Some(surface) = Surface::with_key(key)
            && let Ok(mut nav) = surface.nav.try_borrow_mut()
        {
            crate::surface::quiet(|| nav.default_back(&surface));
        }
    });
}

/// The back button of `screen`'s header.
fn go_back_from(surface: &Rc<Surface>, screen: NodeId) {
    let Ok(mut nav) = surface.nav.try_borrow_mut() else {
        return;
    };
    let stack = nav
        .stacks
        .values()
        .find(|s| s.shown.last() == Some(&screen))
        .map(|s| s.view.id());
    if let Some(stack) = stack {
        nav.back(surface, stack);
    }
}

/// True when an AppKit text field (its field editor) or text view has the
/// window's keyboard focus.
fn focused_field(surface: &Surface) -> bool {
    surface
        .root
        .ns()
        .window()
        .and_then(|w| w.firstResponder())
        .is_some_and(|r| r.downcast_ref::<NSText>().is_some())
}

/// A key nothing in the window used: Escape, Option+Left and Command+[ go
/// back. Whether it was taken. Escape in a focused text field or web view
/// only leaves it (a desktop convention: it never navigates away from what
/// is being typed); Option+Left and Command+[ still go back.
pub(crate) fn key_pressed(surface: &Rc<Surface>, event: &NSEvent) -> bool {
    const ESCAPE: u16 = 53;
    const LEFT: u16 = 123;
    let mods = event.modifierFlags()
        & (NSEventModifierFlags::Control
            | NSEventModifierFlags::Option
            | NSEventModifierFlags::Shift
            | NSEventModifierFlags::Command);
    let code = event.keyCode();
    let bracket = || crate::input::shortcut_key(event) == Some('[');
    let escape = code == ESCAPE && mods.is_empty();
    let back = escape
        || (code == LEFT && mods == NSEventModifierFlags::Option)
        || (mods == NSEventModifierFlags::Command && bracket());
    // A page's Escape reaches here only when the page let it go (WebKit
    // sends the keys a page does not use up the responder chain): it
    // hands the keyboard back to the app, as a field's does, rather than
    // closing the screen under a form being filled in. Option+Left and
    // Command+[ a page does not use still go back.
    if escape && (focused_field(surface) || crate::native::web_focused(surface)) {
        if let Some(window) = surface.root.ns().window() {
            window.makeFirstResponder(Some(surface.root.ns()));
        }
        return true;
    }
    back && go_back(surface)
}

/// A screen was pushed (`added`) or popped in `stack`.
pub(crate) fn stack_changed(surface: &Rc<Surface>, stack: &NshView, added: Option<&NshView>) {
    let mut nav = surface.nav.borrow_mut();
    let id = stack.id();
    let st = nav.stacks.entry(id).or_insert_with(|| StackNav {
        view: stack.clone(),
        shown: Vec::new(),
        popped: Vec::new(),
        top: None,
        image: None,
    });
    // The top screen just left: keep what it shows before the rest of the
    // batch takes its content apart.
    if added.is_none()
        && st.image.is_none()
        && let Some(top) = st.top.clone()
        && !top.has_parent()
    {
        st.image = shot(surface, &top).map(|image| (top.id(), image));
    }
    if let Some(screen) = added.filter(|v| v.kind() == ViewKind::Screen) {
        nav.screen(screen);
        // Hidden until the batch is reconciled: a screen pushed under the
        // top one must not flash.
        hide(surface, screen);
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
    let back_title_changed = s.config.back_title != config.back_title;
    s.config = config.clone();
    s.style(surface, dark);
    let Some(stack) = view.parent_view().filter(|p| p.kind() == ViewKind::Stack) else {
        return;
    };
    s.report(surface, &stack);
    // The screen's back title labels the next one's back button.
    if back_title_changed {
        let next = nav.stacks.get(&stack.id()).and_then(|st| {
            let i = st.shown.iter().position(|&s| s == view.id())?;
            st.shown.get(i + 1).copied()
        });
        if let Some(next) = next.and_then(|id| nav.screens.get_mut(&id)) {
            next.back_title = config.back_title.clone();
            next.style(surface, dark);
        }
    }
}

/// `ModalConfig`.
pub(crate) fn modal_config(surface: &Rc<Surface>, view: &NshView, config: &ModalConfig) {
    let mut nav = surface.nav.borrow_mut();
    let id = view.id();
    nav.modals.entry(id).or_default().config = *config;
    nav.style_modal(dark(surface), id);
    // Its presentation decides its frame and whether a scrim dims the
    // window.
    nav.sheet_scrim(surface, id);
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
    m.view = Some(view.clone());
    m.closed = false;
    m.reported = None;
    m.focus_before = focused(surface);
    if view.has_parent() {
        view.unparent();
    }
    view.insert_before(&surface.root, None);
    nav.order.push(id);
    nav.style_modal(dark(surface), id);
    nav.sheet_scrim(surface, id);
    nav.size_modal(surface, id);
    nav.raise_modals(surface);
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
            if view.has_parent() {
                drop_focus(surface, view.ns());
                view.unparent();
            }
        }
    }
}

/// A header item was inserted into `parent` (a screen).
pub(crate) fn header_item(surface: &Rc<Surface>, parent: &NshView, child: &NshView) {
    let mut nav = surface.nav.borrow_mut();
    if child.parent_view().as_ref() != Some(parent) {
        if child.has_parent() {
            child.unparent();
        }
        child.insert_before(parent, None);
    }
    let s = nav.screen(parent);
    if !s.items.contains(child) {
        s.items.push(child.clone());
    }
    // Placed (and shown) by the header's layout at the end of the batch.
    hide(surface, child);
    if let Some(h) = &mut s.header {
        h.laid_out = None;
    }
}

/// A header item was removed.
pub(crate) fn header_item_removed(surface: &Rc<Surface>, view: &NshView) {
    let mut nav = surface.nav.borrow_mut();
    if let Some(screen) = view.parent_view()
        && let Some(s) = nav.screens.get_mut(&screen.id())
    {
        s.items.retain(|v| v != view);
        if let Some(h) = &mut s.header {
            h.laid_out = None;
        }
    }
    if view.has_parent() {
        drop_focus(surface, view.ns());
        view.unparent();
    }
    show(view);
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
    let dark = dark(surface);
    if nav.dark.replace(dark).is_some_and(|d| d != dark) {
        for s in nav.screens.values_mut() {
            s.style(surface, dark);
        }
        let ids: Vec<NodeId> = nav.modals.keys().copied().collect();
        for id in ids {
            nav.style_modal(dark, id);
        }
    }
    for stack in std::mem::take(&mut nav.dirty) {
        nav.reconcile(surface, stack);
    }
    for id in std::mem::take(&mut nav.presenting) {
        nav.show_modal(surface, id);
    }
    // Screen widths and item sizes change with the app's layout.
    for s in nav.screens.values_mut() {
        s.layout(surface);
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

    #[test]
    fn sheets_are_centered_cards() {
        let config = |presentation, transparent| ModalConfig {
            presentation,
            transparent,
            ..ModalConfig::default()
        };
        let page = config(ModalPresentation::PageSheet, false);
        let form = config(ModalPresentation::FormSheet, false);
        assert_eq!(
            sheet_frame(&config(ModalPresentation::FullScreen, false), 1200.0, 900.0),
            None
        );
        // Transparent modals cover the window, as on iOS.
        assert_eq!(
            sheet_frame(&config(ModalPresentation::PageSheet, true), 1200.0, 900.0),
            None
        );
        assert_eq!(
            sheet_frame(&page, 1200.0, 900.0),
            Some(rect(240.0, 32.0, 720.0, 836.0))
        );
        assert_eq!(
            sheet_frame(&form, 1200.0, 900.0),
            Some(rect(330.0, 140.0, 540.0, 620.0))
        );
        assert_eq!(
            sheet_frame(&page, 400.0, 300.0),
            Some(rect(20.0, 15.0, 360.0, 270.0))
        );
    }

    #[test]
    fn headers_shown_above_the_content_are_measured() {
        let config = |transparent: bool, large_title: bool, header_shown: bool| ScreenConfig {
            header_transparent: transparent,
            large_title,
            header_shown,
            ..ScreenConfig::default()
        };
        assert_eq!(measured_header(&config(false, false, true)), Some(BAR));
        assert_eq!(measured_header(&config(true, false, true)), None);
        assert_eq!(measured_header(&config(false, true, true)), None);
        assert_eq!(measured_header(&config(false, false, false)), None);
    }

    #[test]
    fn content_under_the_header_is_inset() {
        let config = |transparent: bool, large_title: bool, header_shown: bool| ScreenConfig {
            header_transparent: transparent,
            large_title,
            header_shown,
            header_height: 47.0,
            ..ScreenConfig::default()
        };
        assert_eq!(content_insets(&config(false, false, true)).top, 0.0);
        assert_eq!(content_insets(&config(true, false, true)).top, 47.0);
        assert_eq!(content_insets(&config(false, true, true)).top, 47.0);
        assert_eq!(content_insets(&config(true, false, false)).top, 0.0);
    }
}
