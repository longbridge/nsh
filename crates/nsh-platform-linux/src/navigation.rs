//! Navigation stacks, screens with their headers, modals.
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
//! The header is drawn by plain GTK widgets the screen hosts
//! ([`NshView::set_native`]): a [`Layer`] for the bar, a label for the
//! title and a flat button going back. The app's header items stay
//! `NshView`s (they have events of their own): children of the screen,
//! placed in the bar with their `shift`. The header goes over the
//! screen's content, which the core laid out below `header_height`
//! (or under the bar for transparent headers and large titles).
//!
//! Going back (the back button, Escape, Alt+Left, the Back key, the
//! mouse's back button; Escape in a focused text field or page only
//! leaves it; the others go back in a page's history first while the page
//! has the focus, or the pointer for the mouse button)
//! pops natively, then sends `onDismiss`, which
//! removes the route in JS; until the app removes the screen it is
//! `popped` and ignored. With `preventBack` the screen gets
//! `onBackRequest` instead. The newest modal comes first: a stack inside it
//! pops, else it gets `onRequestClose` (when `dismissible` it also closes
//! natively, and gets `onDismiss` right after).
//!
//! Modals are children of the surface's root, kept above the app's views,
//! at the size of the surface. They slide up or fade in, then get
//! `onShow`; when the app removes one it leaves as an image of its last
//! frame, then gets `onDismiss`. Page and form sheets are cards centered
//! in the window (rounded, with a shadow and a width of their own) over a
//! scrim that dims the window and takes its input: clicking it asks the
//! sheet to close, as going back does.
//!
//! Headers have a fixed height on a desktop, reported to the core
//! (`set_screen_header`) so screens lay their content out below the
//! header actually shown rather than the core's estimate.
//!
//! Transitions run on the root's frame clock and only move views
//! (`shift`, `fade`): nothing is laid out again while they run.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
};

use gtk4::{self as gtk, gdk, glib, graphene, gsk, pango, prelude::*, subclass::prelude::*};
use nsh_core::{
    Bounds, Color, EdgeInsets, EventKind, EventPayload, ModalAnimation, ModalConfig,
    ModalPresentation, NodeId, PayloadValue, Point, Points, ROOT_ID, ScreenAnimation, ScreenConfig,
    Shadow, Size, ViewKind, ViewStyle,
};
use nsh_engine::ColorScheme;
use rustc_hash::FxHashMap;

use crate::{decor::rgba, surface::Surface, view::NshView};

/// Height of a screen's header bar, normal and with a large title (a
/// GNOME header bar; a large title gets a second row).
pub(crate) const HEADER_HEIGHTS: (f32, f32) = (47.0, 94.0);

/// The row of the bar holding the buttons and a normal title.
const BAR: f32 = HEADER_HEIGHTS.0;
/// Space between the bar's edges and its buttons.
const EDGE: f32 = 6.0;
/// Space between the bar's items.
const GAP: f32 = 8.0;
/// Push and pop.
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

/// Colors of the bars and backgrounds, light or dark (libadwaita's).
#[derive(Clone, Copy)]
struct Theme {
    window: gdk::RGBA,
    bar: gdk::RGBA,
    line: gdk::RGBA,
    fg: gdk::RGBA,
    /// Dims what is behind a sheet.
    scrim: gdk::RGBA,
    /// A sheet's background and hairline edge (`window`, as the core's
    /// color: it is the view's own background, under its rounded clip).
    sheet: Color,
    sheet_edge: Color,
}

fn theme(dark: bool) -> Theme {
    if dark {
        Theme {
            window: gdk::RGBA::new(0.141, 0.141, 0.141, 1.0),
            bar: gdk::RGBA::new(0.188, 0.188, 0.188, 1.0),
            line: gdk::RGBA::new(0.0, 0.0, 0.0, 0.36),
            fg: gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
            scrim: gdk::RGBA::new(0.0, 0.0, 0.0, 0.5),
            sheet: Color::rgba(36, 36, 36, 255),
            sheet_edge: Color::rgba(255, 255, 255, 20),
        }
    } else {
        Theme {
            window: gdk::RGBA::new(0.98, 0.98, 0.98, 1.0),
            bar: gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
            line: gdk::RGBA::new(0.0, 0.0, 0.0, 0.12),
            fg: gdk::RGBA::new(0.0, 0.0, 0.024, 0.8),
            scrim: gdk::RGBA::new(0.0, 0.0, 0.0, 0.32),
            sheet: Color::rgba(250, 250, 250, 255),
            sheet_edge: Color::rgba(0, 0, 0, 20),
        }
    }
}

const CSS: &str = "
button.nsh-header-button { background: none; border: none; box-shadow: none; outline: none;
  min-height: 0; min-width: 0; padding: 4px 8px; border-radius: 8px; }
button.nsh-header-button:hover { background: rgba(0, 0, 0, 0.07); }
button.nsh-header-button:active { background: rgba(0, 0, 0, 0.14); }
button.nsh-header-button.nsh-dark:hover { background: rgba(255, 255, 255, 0.1); }
button.nsh-header-button.nsh-dark:active { background: rgba(255, 255, 255, 0.18); }
";

mod imp {
    use super::*;

    /// See [`super::Layer`].
    #[derive(Default)]
    pub(crate) struct Layer {
        pub(crate) fill: Cell<Option<gdk::RGBA>>,
        pub(crate) line: Cell<Option<gdk::RGBA>>,
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
            if let Some(c) = self.line.get() {
                snapshot.append_color(&c, &graphene::Rect::new(0.0, h - 1.0, w, 1.0));
            }
        }
    }
}

glib::wrapper! {
    /// A plain drawing a view hosts: a fill (a screen's background, a
    /// header bar) with a hairline at the bottom, or the last image of a
    /// screen or modal that is leaving.
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

    fn set_colors(&self, fill: Option<gdk::RGBA>, line: Option<gdk::RGBA>) {
        let imp = self.imp();
        if imp.fill.replace(fill) != fill || imp.line.replace(line) != line {
            self.queue_draw();
        }
    }

    fn set_node(&self, node: Option<gsk::RenderNode>) {
        *self.imp().node.borrow_mut() = node;
        self.queue_draw();
    }
}

/// A screen's header widgets.
struct Header {
    bar: Layer,
    title: gtk::Label,
    back: gtk::Button,
    back_label: gtk::Label,
    chevron: Rc<Cell<gdk::RGBA>>,
    chevron_area: gtk::DrawingArea,
    /// What it was laid out for (width, header height, the items' frames):
    /// unchanged, nothing moves.
    laid_out: Option<(f32, f32, [Bounds<Points>; 3])>,
}

/// One screen.
struct ScreenNav {
    view: NshView,
    config: ScreenConfig,
    /// Its opaque background, under the app's content.
    backdrop: Layer,
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
    /// Watches the top screen: keeps its last image when it is unmapped.
    paintable: Option<(NodeId, gtk::WidgetPaintable)>,
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
    backdrop: Option<Layer>,
    /// Dims the window behind a sheet; clicking it asks to close.
    scrim: Option<Layer>,
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
        scrim: Option<Layer>,
        fade: bool,
    },
    ModalOut {
        /// The modal to report `onDismiss` for at the end (`None`: it was
        /// reported when the user closed it).
        dismiss: Option<NodeId>,
        layer: Layer,
        /// Where the modal was (a sheet's card, or the whole window).
        frame: Bounds<Points>,
        scrim: Option<Layer>,
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
    if size.width.0 <= 0.0 || *last == Some((size, insets)) {
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

/// The header item of node `id`.
fn find_item(items: &[NshView], id: Option<NodeId>) -> Option<&NshView> {
    let id = id?;
    items.iter().find(|v| v.id() == id)
}

/// What `paintable` shows now (its widget, or the image it kept of it).
fn image_of(paintable: &gtk::WidgetPaintable, width: f32, height: f32) -> Option<gsk::RenderNode> {
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width as f64, height as f64);
    snapshot.to_node()
}

fn label_attrs(color: gdk::RGBA, size: f64, bold: bool) -> pango::AttrList {
    let list = pango::AttrList::new();
    let c = |v: f32| (v.clamp(0.0, 1.0) * 65535.0) as u16;
    list.insert(pango::AttrColor::new_foreground(
        c(color.red()),
        c(color.green()),
        c(color.blue()),
    ));
    list.insert(pango::AttrInt::new_foreground_alpha(c(color.alpha())));
    list.insert(pango::AttrSize::new_size_absolute(
        (size * pango::SCALE as f64) as i32,
    ));
    if bold {
        list.insert(pango::AttrInt::new_weight(pango::Weight::Bold));
    }
    list
}

/// The natural size of `widget`.
fn natural(widget: &impl IsA<gtk::Widget>) -> (f32, f32) {
    let (_, w, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
    let (_, h, _, _) = widget.measure(gtk::Orientation::Vertical, w);
    (w as f32, h as f32)
}

fn install_css() {
    thread_local!(static DONE: Cell<bool> = const { Cell::new(false) });
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
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

impl Header {
    fn new(surface: &Rc<Surface>, screen: NodeId) -> Self {
        install_css();
        let bar = Layer::new();
        let title = gtk::Label::new(None);
        title.set_ellipsize(pango::EllipsizeMode::End);
        title.set_single_line_mode(true);
        title.set_can_target(false);
        let chevron = Rc::new(Cell::new(gdk::RGBA::BLACK));
        let chevron_area = gtk::DrawingArea::new();
        chevron_area.set_content_width(10);
        chevron_area.set_content_height(18);
        let color = chevron.clone();
        chevron_area.set_draw_func(move |_, cr, w, h| {
            let c = color.get();
            cr.set_source_rgba(
                c.red() as f64,
                c.green() as f64,
                c.blue() as f64,
                c.alpha() as f64,
            );
            let (w, h) = (w as f64, h as f64);
            cr.set_line_width(2.0);
            cr.set_line_cap(gtk::cairo::LineCap::Round);
            cr.set_line_join(gtk::cairo::LineJoin::Round);
            cr.move_to(w - 2.0, h * 0.5 - 6.0);
            cr.line_to(w - 8.0, h * 0.5);
            cr.line_to(w - 2.0, h * 0.5 + 6.0);
            let _ = cr.stroke();
        });
        let back_label = gtk::Label::new(None);
        back_label.set_single_line_mode(true);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        content.append(&chevron_area);
        content.append(&back_label);
        let back = gtk::Button::new();
        back.set_child(Some(&content));
        back.add_css_class("nsh-header-button");
        back.set_focusable(false);
        back.set_focus_on_click(false);
        let weak = Rc::downgrade(surface);
        back.connect_clicked(move |_| {
            if let Some(surface) = weak.upgrade() {
                go_back_from(&surface, screen);
            }
        });
        Header {
            bar,
            title,
            back,
            back_label,
            chevron,
            chevron_area,
            laid_out: None,
        }
    }

    fn widgets(&self) -> [gtk::Widget; 3] {
        [
            self.bar.clone().upcast(),
            self.back.clone().upcast(),
            self.title.clone().upcast(),
        ]
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
        self.backdrop.set_colors(Some(t.window), None);
        if !self.header_shown() {
            if let Some(header) = self.header.take() {
                for widget in header.widgets() {
                    self.view.remove_native(&widget);
                }
            }
            for item in &self.items {
                hide(item);
            }
            self.layout();
            return;
        }
        let id = self.view.id();
        let header = self.header.get_or_insert_with(|| Header::new(surface, id));
        let c = &self.config;
        if c.header_transparent {
            header.bar.set_colors(None, None);
        } else {
            let fill = c.background_color.map(rgba).unwrap_or(t.bar);
            header.bar.set_colors(Some(fill), Some(t.line));
        }
        let fg = c.title_color.map(rgba).unwrap_or(t.fg);
        let size = if c.large_title {
            TITLE_SIZE.1
        } else {
            TITLE_SIZE.0
        };
        header
            .title
            .set_attributes(Some(&label_attrs(fg, size, true)));
        header.title.set_text(c.title.as_deref().unwrap_or(""));
        let tint = c.tint_color.map(rgba).unwrap_or(t.fg);
        header.chevron.set(tint);
        header.chevron_area.queue_draw();
        header
            .back_label
            .set_attributes(Some(&label_attrs(tint, TITLE_SIZE.0, false)));
        let back_title = self.back_title.as_deref().unwrap_or("");
        header.back_label.set_text(back_title);
        header.back_label.set_visible(!back_title.is_empty());
        if dark {
            header.back.add_css_class("nsh-dark");
        } else {
            header.back.remove_css_class("nsh-dark");
        }
        header.laid_out = None;
        self.layout();
    }

    /// Places the background, the header's widgets and items (when
    /// something changed).
    fn layout(&mut self) {
        let (w, h) = {
            let f = self.view.state().frame;
            (f.size.width.0, f.size.height.0)
        };
        if self.backdrop_size != (w, h) {
            self.backdrop_size = (w, h);
            self.view.set_native(&self.backdrop, rect(0.0, 0.0, w, h));
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
        let centered = |h: f32| top + ((BAR - h) / 2.0).max(0.0);
        let place = |v: &NshView, x: f32| {
            let f = v.state().frame;
            v.set_child_visible(true);
            set_shift(
                v,
                (x - f.origin.x.0, centered(f.size.height.0) - f.origin.y.0),
            );
        };
        let mut left = EDGE;
        header.back.set_visible(self.can_go_back);
        if self.can_go_back {
            let (bw, bh) = natural(&header.back);
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
                hide(item);
            }
        }
        if let Some(v) = find_item(&self.items, c.title_view) {
            let fw = v.state().frame.size.width.0;
            place(v, ((w - fw) / 2.0).max(left));
            header.title.set_visible(false);
            view.set_native(&header.title, Bounds::default());
            return;
        }
        header.title.set_visible(true);
        let (tw, th) = natural(&header.title);
        if c.large_title {
            let rect = rect(16.0, top + BAR + (BAR - th) / 2.0, (w - 32.0).max(0.0), th);
            header.title.set_xalign(0.0);
            view.set_native(&header.title, rect);
        } else {
            // Centered in the bar, as long as it fits between the items.
            let side = left.max(w - right) + GAP;
            let tw = tw.min((w - 2.0 * side).max(0.0));
            header.title.set_xalign(0.5);
            let rect = rect((w - tw) / 2.0, centered(th), tw, th);
            view.set_native(&header.title, rect);
        }
    }

    /// Keeps the header over the app's content (which the app may insert
    /// after it).
    fn raise_header(&self) {
        let Some(header) = &self.header else { return };
        let mut tail: Vec<gtk::Widget> = header.widgets().into();
        tail.extend(self.items.iter().map(|v| v.clone().upcast()));
        keep_last(&self.view, &tail);
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
            Motion::ModalIn { view, scrim, fade } => {
                if *fade {
                    set_fade(view, e);
                } else {
                    // From below the window's bottom edge.
                    let h = surface.root.state().frame.size.height.0;
                    let y = view.state().frame.origin.y.0;
                    set_shift(view, (0.0, (h - y) * (1.0 - e)));
                }
                if let Some(scrim) = scrim {
                    scrim.set_opacity(e as f64);
                }
            }
            Motion::ModalOut {
                layer,
                frame,
                scrim,
                fade,
                ..
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
                if let Some(scrim) = scrim {
                    scrim.set_opacity((1.0 - e) as f64);
                }
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
            Motion::ModalOut {
                dismiss,
                layer,
                scrim,
                ..
            } => {
                surface.root.remove_native(&layer);
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
        if let Some(b) = m.backdrop.take() {
            view.remove_native(&b);
        }
        let scrim = m.scrim.take();
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

    /// The user asks modal `id` to close (going back, its sheet's
    /// backdrop): it hears `onRequestClose`. A `dismissible` one also
    /// leaves natively and hears `onDismiss` right after, as on iOS (the
    /// runtime keeps the handler of a modal the app removed in between).
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
    /// views, each sheet right above its scrim.
    fn raise_modals(&self, surface: &Surface) {
        let mut tail: Vec<gtk::Widget> = Vec::new();
        for m in self.order.iter().filter_map(|id| self.modals.get(id)) {
            if let Some(scrim) = &m.scrim {
                tail.push(scrim.clone().upcast());
            }
            if let Some(view) = &m.view {
                tail.push(view.clone().upcast());
            }
        }
        for t in &self.transitions {
            if let Motion::ModalOut { layer, scrim, .. } = &t.motion {
                if let Some(scrim) = scrim {
                    tail.push(scrim.clone().upcast());
                }
                tail.push(layer.clone().upcast());
            }
        }
        keep_last(&surface.root, &tail);
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
            view.queue_allocate();
            view.moved();
        }
        if let Some(b) = &m.backdrop {
            view.set_native(b, rect(0.0, 0.0, frame.size.width.0, frame.size.height.0));
        }
        if let Some(scrim) = &m.scrim {
            surface.root.set_native(scrim, f);
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
        let sheet = is_sheet(&m.config);
        if !sheet {
            if let Some(scrim) = m.scrim.take() {
                surface.root.remove_native(&scrim);
            }
            return;
        }
        if m.scrim.is_some() {
            return;
        }
        let scrim = Layer::new();
        // Picked as a hosted widget: what is behind gets no input.
        scrim.set_can_target(true);
        scrim.set_colors(Some(theme(dark(surface)).scrim), None);
        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(surface);
        click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(surface) = weak.upgrade()
                && let Ok(mut nav) = surface.nav.try_borrow_mut()
            {
                nav.request_close(&surface, id);
            }
        });
        scrim.add_controller(click);
        let frame = surface.root.state().frame;
        surface.root.set_native(&scrim, frame);
        m.scrim = Some(scrim);
        // Right under the sheet.
        self.raise_modals(surface);
    }

    /// Gives modal `id` its opaque background, unless it is transparent: a
    /// sheet's is its own (a rounded card with a shadow), a full-screen
    /// modal's a layer under its content.
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
            *view.state_mut().style = style.unwrap_or_default();
            view.moved();
        }
        if let Some(scrim) = &m.scrim {
            scrim.set_colors(Some(t.scrim), None);
        }
        if m.config.transparent || sheet {
            if let Some(b) = m.backdrop.take() {
                view.remove_native(&b);
            }
            return;
        }
        let backdrop = m.backdrop.get_or_insert_with(|| {
            let layer = Layer::new();
            // First, under the app's content.
            layer.insert_after(view, None::<&gtk::Widget>);
            layer
        });
        backdrop.set_colors(Some(t.window), None);
        let f = view.state().frame;
        view.set_native(backdrop, rect(0.0, 0.0, f.size.width.0, f.size.height.0));
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

/// The window of `surface`, when a text field (a `gtk::TextView` or any
/// `gtk::Editable`) or a web view's page has its keyboard focus.
fn focused_field(surface: &Surface) -> Option<gtk::Root> {
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
    nav.style_modal(dark(surface), id);
    nav.sheet_scrim(surface, id);
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
    // Placed (and shown) by the header's layout at the end of the batch.
    hide(child);
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
        // A wide window: the sheet's own width, centered, the full height
        // but the margins.
        assert_eq!(
            sheet_frame(&page, 1200.0, 900.0),
            Some(rect(240.0, 32.0, 720.0, 836.0))
        );
        assert_eq!(
            sheet_frame(&form, 1200.0, 900.0),
            Some(rect(330.0, 140.0, 540.0, 620.0))
        );
        // A small window: smaller margins.
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
