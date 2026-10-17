//! Navigation stacks, screens with their headers, modals.
//!
//! A stack's screens are its child views; only the top one shows (and the
//! one below it while a transition runs): the others are
//! [`ViewState::concealed`](crate::view::ViewState::concealed), so they are
//! neither drawn nor hit. Changes of the app's screens are reconciled at
//! the end of each batch, once every item is applied: a new top screen
//! slides in from the right over the previous one (which moves left a bit
//! and dims, as WinUI's and GNOME's navigation views do); a top screen the
//! app removes slides out as it last showed.
//!
//! GPUI has no offscreen pictures to record that last image into (the
//! Windows backend draws it into a Direct2D command list), so a departing
//! screen or modal keeps its views instead: once the batch starts tearing
//! one down ([`removing`]), the mounter's detaching of its descendants is
//! deferred ([`defer_detach`]). At the end of the batch the screen or modal
//! either stayed (then the deferred views are detached after all,
//! [`take_deferred`]) or was removed: then its whole subtree, still
//! intact, goes into a layer view that ignores the pointer and is drawn
//! moving out until the transition ends, when the layer is dropped with
//! everything in it. Views inside a departing layer are never detached
//! one by one (the app may delete them while it leaves). Native surfaces
//! (web views, videos) are destroyed when the app deletes their views, so
//! they are missing from a departing screen.
//!
//! The header is drawn natively: a layer view the screen holds above its
//! content hosts a [`HeaderBar`] (the bar, its hairline, the title and the
//! back button, with glyphs GPUI shapes and paints). The app's header items stay
//! `NshView`s (they have events of their own): children of the screen,
//! placed in the bar with their `shift`, above the bar. The header goes
//! over the screen's content, which the core laid out below
//! `header_height` (or under the bar for transparent headers and large
//! titles).
//!
//! Going back (the back button, Escape, Alt+Left, Command+[ on macOS, the
//! Browser Back key, the mouse's back button; Escape in a focused text
//! field only leaves it)
//! pops natively, then sends `onDismiss`, which removes the route in JS;
//! until the app removes the screen it is `popped` and ignored. With
//! `preventBack` the screen gets `onBackRequest` instead: the answer comes
//! from props mounted beforehand, never from JS. The newest modal comes
//! first: a stack inside it pops, else it gets `onRequestClose` (when
//! `dismissible` it also closes natively, and gets `onDismiss` right
//! after).
//!
//! Modals are children of the surface's root, kept above the app's views,
//! at the size of the surface. They slide up or fade in, then get
//! `onShow`; when the app removes one it leaves as it last showed (its
//! views kept, as a departing screen's), then gets `onDismiss`. Page and form sheets are cards centered
//! in the window (rounded, with a shadow and a width of their own) over a
//! scrim that dims the window and takes its input: clicking it asks the
//! sheet to close, as going back does.
//!
//! Headers have a fixed height on a desktop, reported to the core
//! (`set_screen_header`) so screens lay their content out below the
//! header actually shown rather than the core's estimate.
//!
//! Transitions run on the surface's frame clock ([`Surface::on_frame`])
//! and only move views (`shift`, `fade`): nothing is laid out again while
//! they run.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gpui::{
    Font, FontFeatures, FontId, FontStyle, FontWeight, GlyphId, LineLayout, PathBuilder, TextRun,
    WindowTextSystem, px,
};
use nsh_core::{
    Color, EdgeInsets, EventKind, EventPayload, ModalAnimation, ModalConfig, ModalPresentation,
    NodeId, PayloadValue, ROOT_ID, Rect, ScreenAnimation, ScreenConfig, Shadow, Size, ViewKind,
    ViewStyle,
};
use rustc_hash::FxHashMap;

use crate::{
    geom::rect,
    input::{KeyEvent, Modifiers, PointerKind},
    render::DrawCx,
    surface::Surface,
    view::{Hosted, ItemPointer, NshView},
};

/// Height of a screen's header bar, normal and with a large title (a
/// large title gets a second row).
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
/// How dark the screen under a pushed one gets (black's opacity).
const DIM: f32 = 0.12;
/// The title's font size, normal and large (points).
const TITLE_SIZE: (f32, f32) = (15.0, 26.0);
/// The back button's padding (horizontal, vertical), chevron box and the
/// space between the chevron and the label.
const BUTTON_PAD: (f32, f32) = (8.0, 4.0);
const CHEVRON: (f32, f32) = (10.0, 18.0);
const CHEVRON_GAP: f32 = 4.0;
/// Corner radius of the back button's hover and press feedback.
const BUTTON_RADIUS: f32 = 6.0;
/// Widest a page sheet and a form sheet get (a form sheet is also at most
/// [`FORM_HEIGHT`] tall), as iPad sheets.
const SHEET_WIDTH: (f32, f32) = (720.0, 540.0);
const FORM_HEIGHT: f32 = 620.0;
/// Space a sheet leaves to the window's edges (less in small windows).
const SHEET_MARGIN: f32 = 32.0;
/// Corner radius of a sheet.
const SHEET_RADIUS: f32 = 12.0;
/// The node id of the views navigation makes itself (backgrounds, headers,
/// scrims, images of leaving screens): never one of the app's (JS ids are
/// even and small, the core's odd and small) and never in the mounter.
const LAYER_ID: NodeId = NodeId::MAX;

const VK_ESCAPE: u16 = 0x1B;
const VK_LEFT: u16 = 0x25;
const VK_BROWSER_BACK: u16 = 0xA6;
/// `[` (Command+[ goes back on macOS).
const VK_OEM_4: u16 = 0xDB;

/// Colors of the bars and backgrounds, light or dark.
#[derive(Clone, Copy)]
struct Theme {
    window: Color,
    bar: Color,
    line: Color,
    fg: Color,
    /// Dims what is behind a sheet.
    scrim: Color,
    /// A sheet's background and hairline edge.
    sheet: Color,
    sheet_edge: Color,
    /// The back button's feedback under the pointer and pressed.
    hover: Color,
    pressed: Color,
}

fn theme(dark: bool) -> Theme {
    if dark {
        Theme {
            window: Color::rgba(0, 0, 0, 255),
            bar: Color::rgba(32, 32, 32, 255),
            line: Color::rgba(255, 255, 255, 24),
            fg: Color::rgba(255, 255, 255, 255),
            scrim: Color::rgba(0, 0, 0, 128),
            sheet: Color::rgba(32, 32, 32, 255),
            sheet_edge: Color::rgba(255, 255, 255, 20),
            hover: Color::rgba(255, 255, 255, 15),
            pressed: Color::rgba(255, 255, 255, 10),
        }
    } else {
        Theme {
            window: Color::rgba(255, 255, 255, 255),
            bar: Color::rgba(249, 249, 249, 255),
            line: Color::rgba(0, 0, 0, 24),
            fg: Color::rgba(0, 0, 0, 228),
            scrim: Color::rgba(0, 0, 0, 82),
            sheet: Color::rgba(255, 255, 255, 255),
            sheet_edge: Color::rgba(0, 0, 0, 20),
            hover: Color::rgba(0, 0, 0, 10),
            pressed: Color::rgba(0, 0, 0, 6),
        }
    }
}

/// `color` with its alpha scaled by `f`.
fn with_alpha(color: Color, f: f32) -> Color {
    let (r, g, b, a) = color.to_f64();
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::rgba(c(r), c(g), c(b), c(a * f64::from(f)))
}

fn contains(r: Rect, x: f32, y: f32) -> bool {
    x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
}

/// A view navigation makes itself (see [`LAYER_ID`]).
fn layer_view() -> NshView {
    NshView::new(LAYER_ID, ViewKind::View)
}

fn set_frame(view: &NshView, frame: Rect) {
    let changed = std::mem::replace(&mut view.state_mut().frame, frame) != frame;
    if changed {
        view.moved();
    }
}

fn set_background(view: &NshView, color: Color) {
    let changed = std::mem::replace(&mut view.state_mut().style.background_color, color) != color;
    if changed {
        view.changed();
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

/// True when `ancestor` is `view` or one of its ancestors.
fn is_ancestor(ancestor: &NshView, view: &NshView) -> bool {
    let mut v = Some(view.clone());
    while let Some(current) = v {
        if &current == ancestor {
            return true;
        }
        v = current.parent_view();
    }
    false
}

/// True when `view` is drawn: in a surface, and neither it nor an
/// ancestor is concealed or hidden.
fn is_shown(view: &NshView) -> bool {
    let mut v = view.clone();
    loop {
        {
            let s = v.state();
            if s.concealed || s.style.hidden {
                return false;
            }
        }
        match v.parent_view() {
            Some(p) => v = p,
            None => return v.attached(),
        }
    }
}

/// Takes the keyboard focus away from a field inside `view` before it is
/// concealed or removed: typing must not go to a field nobody sees.
fn drop_focus(surface: &Surface, view: &NshView) {
    if let Some(field) = crate::text_input::focused(surface)
        && is_ancestor(view, &field)
    {
        crate::text_input::blur(&field);
    }
}

/// Conceals `view` (or shows it again).
fn set_concealed(surface: &Surface, view: &NshView, concealed: bool) {
    if view.state().concealed == concealed {
        return;
    }
    if concealed {
        drop_focus(surface, view);
    }
    view.state_mut().concealed = concealed;
    view.changed();
}

/// Moves `tail` to the end of `parent`'s children (drawn last), unless it
/// is there already.
fn keep_last(parent: &NshView, tail: &[NshView]) {
    let in_place =
        parent.with_children(|c| c.len() >= tail.len() && c[c.len() - tail.len()..] == *tail);
    if !in_place {
        for v in tail {
            v.insert_before(parent, None);
        }
    }
}

/// The header item of node `id`.
fn find_item(items: &[NshView], id: Option<NodeId>) -> Option<&NshView> {
    let id = id?;
    items.iter().find(|v| v.id() == id)
}

fn ease(t: f64) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    (1.0 - u * u * u) as f32
}

/// How a screen comes and goes: the duration and whether it fades;
/// `None`: at once.
fn screen_motion(animation: ScreenAnimation, reduce_motion: bool) -> Option<(f64, bool)> {
    match animation {
        ScreenAnimation::None => None,
        ScreenAnimation::Fade => Some((FADE_MS, true)),
        ScreenAnimation::Default if reduce_motion => Some((FADE_MS, true)),
        ScreenAnimation::Default => Some((SLIDE_MS, false)),
    }
}

/// How a modal comes and goes: the duration and whether it fades;
/// `None`: at once.
fn modal_motion(animation: ModalAnimation, reduce_motion: bool) -> Option<(f64, bool)> {
    match animation {
        ModalAnimation::None => None,
        ModalAnimation::Fade => Some((FADE_MS, true)),
        ModalAnimation::Slide if reduce_motion => Some((FADE_MS, true)),
        ModalAnimation::Slide => Some((MODAL_MS, false)),
    }
}

fn reduce_motion(surface: &Surface) -> bool {
    surface.environment.get().reduce_motion
}

/// A modal with `config` is a sheet: a page or form sheet that is not
/// transparent (a transparent one covers the window, as on iOS).
fn is_sheet(config: &ModalConfig) -> bool {
    !config.transparent && config.presentation != ModalPresentation::FullScreen
}

/// Where a modal with `config` sits in a `width` × `height` window when it
/// is a sheet: a card centered over the dimmed window, as wide as the
/// window allows up to its kind's width (a desktop window is no phone: a
/// sheet sliding up over all of it would look like a new page). `None`:
/// it covers the window.
fn sheet_frame(config: &ModalConfig, width: f32, height: f32) -> Option<Rect> {
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
    last: &mut Option<(Size, EdgeInsets)>,
    size: Size,
    insets: EdgeInsets,
) {
    if size.width <= 0.0 || *last == Some((size, insets)) {
        return;
    }
    let insets_changed = last.is_none_or(|(_, i)| i != insets);
    *last = Some((size, insets));
    surface.control(|c| c.set_root_size(id, size, insets));
    if insets_changed {
        surface.event(id, EventKind::Insets, insets_payload(insets));
    }
}

/// Escape, Alt+Left and the Browser Back key go back (no other modifier
/// held), and Command+[ on macOS.
fn is_back_key(vk: u16, m: Modifiers) -> bool {
    let others = m.shift || m.ctrl || m.meta;
    match vk {
        VK_ESCAPE | VK_BROWSER_BACK => !others && !m.alt,
        VK_LEFT => m.alt && !others,
        VK_OEM_4 => cfg!(target_os = "macos") && m.meta && !m.shift && !m.ctrl && !m.alt,
        _ => false,
    }
}

/// A layer that holds the subtree of a screen or modal while it leaves
/// (see the module doc): `size` covers the stack or the window, the
/// subtree sits in it at its own frame, and nothing in it takes the
/// pointer.
fn departing_layer(view: &NshView, size: (f32, f32)) -> NshView {
    let layer = layer_view();
    set_frame(&layer, rect(0.0, 0.0, size.0, size.1));
    layer.state_mut().props.pointer_events = nsh_core::PointerEvents::None;
    view.insert_before(&layer, None);
    layer
}

/// A plain fill: the scrim behind a sheet (which takes the pointer and
/// asks the sheet to close when clicked) or the dimming of the screen
/// under a pushed one.
struct Fill {
    color: Cell<Color>,
    /// The modal a click asks to close (with its surface's key).
    closes: Option<(u64, NodeId)>,
    pressed: Cell<bool>,
}

impl Fill {
    fn set_color(&self, color: Color, owner: &NshView) {
        if self.color.replace(color) != color {
            owner.changed();
        }
    }
}

impl Hosted for Fill {
    fn draw(&self, cx: &mut DrawCx<'_>, size: (f32, f32)) {
        let color = self.color.get();
        if color.is_transparent() {
            return;
        }
        cx.fill_rect(rect(0.0, 0.0, size.0, size.1), [0.0; 4], color);
    }

    fn hit(&self, x: f32, y: f32, size: (f32, f32)) -> bool {
        self.closes.is_some() && x >= 0.0 && y >= 0.0 && x < size.0 && y < size.1
    }

    fn pointer(&self, event: &ItemPointer) -> bool {
        let Some((key, modal)) = self.closes else {
            return false;
        };
        match event.kind {
            PointerKind::Down => self.pressed.set(true),
            PointerKind::Up if self.pressed.replace(false) => {
                // After the input handling that got here.
                crate::rt::invoke(move || {
                    if let Some(surface) = Surface::by_key(key)
                        && let Ok(mut nav) = surface.nav.try_borrow_mut()
                    {
                        nav.request_close(&surface, modal);
                    }
                });
            }
            PointerKind::Cancel | PointerKind::Leave => self.pressed.set(false),
            _ => {}
        }
        true
    }
}

/// A single line of the header's text, shaped by GPUI.
struct TextLine {
    layout: Arc<LineLayout>,
    /// "…", which ends what does not fit.
    ellipsis: Arc<LineLayout>,
    /// Its natural size.
    width: f32,
    height: f32,
    /// The widest it may be drawn ([`TextLine::fit`]).
    max: Cell<f32>,
}

/// A glyph of a [`TextLine`], positioned on its baseline.
#[derive(Clone, Copy)]
struct PlacedGlyph {
    x: f32,
    font: FontId,
    id: GlyphId,
    emoji: bool,
}

impl TextLine {
    fn new(surface: &Surface, text: &str, size: f32, semibold: bool) -> Option<TextLine> {
        let system = surface.text_system.borrow().clone()?;
        // A shaper of its own: lines shaped outside a window's frame stay
        // in its cache, which goes with it.
        let shaper = WindowTextSystem::new(system);
        let font = Font {
            family: crate::text::ui_family().to_string().into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: if semibold {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            },
            style: FontStyle::Normal,
        };
        let shape = |text: &str| {
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            shaper.layout_line(text, px(size), &[run], None)
        };
        let layout = shape(text);
        let ellipsis = shape("\u{2026}");
        let width = f32::from(layout.width).ceil();
        let height = f32::from(layout.ascent + layout.descent).ceil();
        Some(TextLine {
            layout,
            ellipsis,
            width,
            height,
            max: Cell::new(f32::INFINITY),
        })
    }

    /// Lets it take at most `width` (an ellipsis ends what does not fit).
    fn fit(&self, width: f32) {
        self.max.set(width.max(0.0));
    }

    /// The glyphs drawn: all of them when they fit, else as many as fit
    /// with the ellipsis after them.
    fn glyphs(&self) -> Vec<PlacedGlyph> {
        let flat = |layout: &LineLayout, dx: f32| -> Vec<PlacedGlyph> {
            layout
                .runs
                .iter()
                .flat_map(|run| {
                    run.glyphs.iter().map(move |g| PlacedGlyph {
                        x: f32::from(g.position.x) + dx,
                        font: run.font_id,
                        id: g.id,
                        emoji: g.is_emoji,
                    })
                })
                .collect()
        };
        let mut glyphs = flat(&self.layout, 0.0);
        let max = self.max.get();
        if self.width <= max + 0.5 {
            return glyphs;
        }
        let room = max - f32::from(self.ellipsis.width);
        // A glyph ends where the next one starts.
        let ends: Vec<f32> = glyphs
            .iter()
            .skip(1)
            .map(|g| g.x)
            .chain([f32::from(self.layout.width)])
            .collect();
        let keep = ends.iter().take_while(|&&end| end <= room).count();
        let at = if keep == 0 { 0.0 } else { ends[keep - 1] };
        glyphs.truncate(keep);
        if room >= 0.0 {
            glyphs.extend(flat(&self.ellipsis, at));
        }
        glyphs
    }

    fn draw(&self, cx: &mut DrawCx<'_>, x: f32, y: f32, color: Color) {
        if cx.culled() {
            return;
        }
        let k = cx.transform_scale();
        let size = f32::from(self.layout.font_size) * k;
        let baseline = y + f32::from(self.layout.ascent);
        let c = cx.color(color);
        let glyphs: Vec<_> = self
            .glyphs()
            .into_iter()
            .map(|g| (cx.map(x + g.x, baseline), g))
            .collect();
        cx.paint(|window, _| {
            for (p, g) in glyphs {
                let result = if g.emoji {
                    window.paint_emoji(p, g.font, g.id, px(size))
                } else {
                    window.paint_glyph(p, g.font, g.id, px(size), c)
                };
                if let Err(err) = result {
                    log::debug!("navigation: cannot paint a glyph: {err}");
                }
            }
        });
    }
}

/// What a header bar draws (set by [`ScreenNav::style`] and
/// [`ScreenNav::layout`]).
#[derive(Default)]
struct BarState {
    /// No bar, no hairline: the content shows through.
    transparent: bool,
    /// The app's colors (`None`: the theme's, resolved when drawn, so a
    /// change of appearance shows on the next frame).
    background: Option<Color>,
    title_color: Option<Color>,
    tint: Option<Color>,
    title: Option<TextLine>,
    /// Where the title goes; `None`: not shown (a title view replaces it).
    title_at: Option<(f32, f32)>,
    /// The back button's frame, when it shows.
    back: Option<Rect>,
    back_label: Option<TextLine>,
    /// The appearance the screen was styled in.
    dark: bool,
    /// Drawn in another appearance, it asked the screens to follow.
    asked: bool,
    hovered: bool,
    pressed: bool,
}

/// Strokes the back button's chevron through `points` (current
/// coordinates), 2 points wide with round ends and a round tip.
fn draw_chevron(cx: &mut DrawCx<'_>, points: [(f32, f32); 3], color: Color) {
    if cx.culled() {
        return;
    }
    let k = cx.transform_scale();
    let mut path = PathBuilder::stroke(px(2.0 * k));
    path.move_to(cx.map(points[0].0, points[0].1));
    for p in &points[1..] {
        path.line_to(cx.map(p.0, p.1));
    }
    let Ok(path) = path.build() else { return };
    let c = cx.color(color);
    cx.paint(|window, _| window.paint_path(path, c));
    // GPUI's strokes end square: dots make the ends and the tip round.
    for (x, y) in points {
        cx.fill_rect(rect(x - 1.0, y - 1.0, 2.0, 2.0), [1.0; 4], color);
    }
}

/// A screen's header: the bar, its hairline, the title and the back
/// button.
struct HeaderBar {
    surface: u64,
    screen: NodeId,
    state: RefCell<BarState>,
}

impl HeaderBar {
    fn invalidate(&self) {
        if let Some(surface) = Surface::by_key(self.surface) {
            surface.invalidate();
        }
    }
}

impl Hosted for HeaderBar {
    fn draw(&self, cx: &mut DrawCx<'_>, size: (f32, f32)) {
        let mut s = self.state.borrow_mut();
        let t = theme(cx.dark);
        if s.dark != cx.dark && !s.asked {
            s.asked = true;
            // The appearance changed without a batch (a system setting):
            // the backgrounds and sheets follow, after this frame.
            let key = self.surface;
            crate::rt::invoke(move || {
                if let Some(surface) = Surface::by_key(key)
                    && let Ok(mut nav) = surface.nav.try_borrow_mut()
                {
                    nav.follow_appearance(&surface);
                }
            });
        }
        let (w, h) = size;
        if !s.transparent {
            let fill = s.background.unwrap_or(t.bar);
            cx.fill_rect(rect(0.0, 0.0, w, h), [0.0; 4], fill);
            cx.fill_rect(rect(0.0, h - 1.0, w, 1.0), [0.0; 4], t.line);
        }
        let tint = s.tint.unwrap_or(t.fg);
        if let Some(back) = s.back {
            let feedback = if s.pressed {
                Some(t.pressed)
            } else if s.hovered {
                Some(t.hover)
            } else {
                None
            };
            if let Some(color) = feedback {
                cx.fill_rect(back, [BUTTON_RADIUS; 4], color);
            }
            // The chevron: a "<" in its box, at the left of the button.
            let x = back.x + BUTTON_PAD.0;
            let y = back.y + (back.height - CHEVRON.1) / 2.0;
            let (cw, ch) = CHEVRON;
            draw_chevron(
                cx,
                [
                    (x + cw - 2.0, y + ch * 0.5 - 6.0),
                    (x + cw - 8.0, y + ch * 0.5),
                    (x + cw - 2.0, y + ch * 0.5 + 6.0),
                ],
                tint,
            );
            if let Some(label) = &s.back_label {
                let lx = x + cw + CHEVRON_GAP;
                let ly = back.y + (back.height - label.height) / 2.0;
                label.draw(cx, lx, ly, tint);
            }
        }
        if let (Some(title), Some((x, y))) = (&s.title, s.title_at) {
            title.draw(cx, x, y, s.title_color.unwrap_or(t.fg));
        }
    }

    fn hit(&self, x: f32, y: f32, size: (f32, f32)) -> bool {
        let s = self.state.borrow();
        if s.back.is_some_and(|b| contains(b, x, y)) {
            return true;
        }
        // An opaque bar takes the pointer; a transparent one lets the
        // content under it have it.
        !s.transparent && x >= 0.0 && y >= 0.0 && x < size.0 && y < size.1
    }

    fn pointer(&self, event: &ItemPointer) -> bool {
        let mut s = self.state.borrow_mut();
        let inside = s.back.is_some_and(|b| contains(b, event.x, event.y));
        let before = (s.hovered, s.pressed);
        let mut click = false;
        match event.kind {
            PointerKind::Down => {
                s.pressed = inside;
                s.hovered = inside;
            }
            PointerKind::Move => s.hovered = inside,
            PointerKind::Up => {
                click = s.pressed && inside;
                s.pressed = false;
                s.hovered = inside;
            }
            PointerKind::Cancel | PointerKind::Leave => {
                s.pressed = false;
                s.hovered = false;
            }
        }
        let changed = before != (s.hovered, s.pressed);
        drop(s);
        if changed {
            self.invalidate();
        }
        if click {
            let (key, screen) = (self.surface, self.screen);
            // After the input handling that got here: going back changes
            // the views it may still look at.
            crate::rt::invoke(move || {
                if let Some(surface) = Surface::by_key(key) {
                    go_back_from(&surface, screen);
                }
            });
        }
        true
    }
}

/// A screen's header: a layer view above the screen's content hosting the
/// bar.
struct Header {
    view: NshView,
    bar: Rc<HeaderBar>,
    item: Rc<dyn Hosted>,
    /// What it was laid out for (width, header height, the items' frames):
    /// unchanged, nothing moves.
    laid_out: Option<(f32, f32, [Rect; 3])>,
}

impl Header {
    fn new(surface: u64, screen: NodeId) -> Self {
        let bar = Rc::new(HeaderBar {
            surface,
            screen,
            state: RefCell::default(),
        });
        let item: Rc<dyn Hosted> = bar.clone();
        Header {
            view: layer_view(),
            bar,
            item,
            laid_out: None,
        }
    }
}

/// One screen.
struct ScreenNav {
    view: NshView,
    config: ScreenConfig,
    /// Its opaque background, under the app's content (its first child).
    backdrop: NshView,
    header: Option<Header>,
    /// The header items the app inserted.
    items: Vec<NshView>,
    /// The back button shows (not the bottom screen, `back_visible`).
    can_go_back: bool,
    /// The back button's label (the previous screen's `back_title`).
    back_title: Option<Arc<str>>,
    /// The size and insets last reported.
    reported: Option<(Size, EdgeInsets)>,
    /// The header height last reported to the core (`None` inside: none
    /// measured, the core estimates it); `None`: never reported.
    header_reported: Option<Option<f32>>,
}

impl ScreenNav {
    fn new(view: &NshView) -> Self {
        let backdrop = layer_view();
        let first = view.with_children(|c| c.first().cloned());
        backdrop.insert_before(view, first.as_ref());
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
        let size = Size::new(f.width, f.height);
        report(surface, id, &mut self.reported, size, insets);
    }

    /// Applies the config's colors and texts to the header (builds or
    /// removes it) and the appearance to the background.
    fn style(&mut self, surface: &Surface, dark: bool) {
        let t = theme(dark);
        set_background(&self.backdrop, t.window);
        if !self.header_shown() {
            if let Some(header) = self.header.take() {
                header.view.unparent();
            }
            for item in &self.items {
                set_concealed(surface, item, true);
            }
            self.layout(surface);
            return;
        }
        let id = self.view.id();
        let key = surface.key();
        let header = self.header.get_or_insert_with(|| Header::new(key, id));
        if header.view.parent_view().is_none() {
            header.view.insert_before(&self.view, None);
            header.view.set_native(&header.item, Rect::default());
        }
        let c = &self.config;
        {
            let mut s = header.bar.state.borrow_mut();
            s.transparent = c.header_transparent;
            s.background = c.background_color;
            s.title_color = c.title_color;
            s.tint = c.tint_color;
            s.dark = dark;
            s.asked = false;
            let size = if c.large_title {
                TITLE_SIZE.1
            } else {
                TITLE_SIZE.0
            };
            s.title = c
                .title
                .as_deref()
                .filter(|t| !t.is_empty())
                .and_then(|t| TextLine::new(surface, t, size, true));
            s.back_label = self
                .back_title
                .as_deref()
                .filter(|t| !t.is_empty())
                .and_then(|t| TextLine::new(surface, t, TITLE_SIZE.0, false));
        }
        header.laid_out = None;
        header.view.changed();
        self.layout(surface);
    }

    /// Places the background, the header and its items (when something
    /// changed).
    fn layout(&mut self, surface: &Surface) {
        let (w, h) = {
            let f = self.view.state().frame;
            (f.width, f.height)
        };
        set_frame(&self.backdrop, rect(0.0, 0.0, w, h));
        let c = &self.config;
        let frames = [c.left, c.right, c.title_view]
            .map(|id| find_item(&self.items, id).map_or(Rect::default(), |v| v.state().frame));
        let Some(header) = &mut self.header else {
            return;
        };
        let key = (w, c.header_height, frames);
        if header.laid_out == Some(key) {
            return;
        }
        header.laid_out = Some(key);
        let height = c.header_height;
        let rows = if c.large_title { 2.0 } else { 1.0 };
        // The status area (none on desktops) is above the bar.
        let top = (height - BAR * rows).max(0.0);
        set_frame(&header.view, rect(0.0, 0.0, w, height));
        header
            .view
            .set_native(&header.item, rect(0.0, 0.0, w, height));
        let centered = |h: f32| top + ((BAR - h) / 2.0).max(0.0);
        let place = |v: &NshView, x: f32| {
            let f = v.state().frame;
            set_concealed(surface, v, false);
            set_shift(v, (x - f.x, centered(f.height) - f.y));
        };
        let mut s = header.bar.state.borrow_mut();
        let mut left = EDGE;
        if self.can_go_back {
            let (lw, lh) = s
                .back_label
                .as_ref()
                .map_or((0.0, 0.0), |l| (CHEVRON_GAP + l.width, l.height));
            let bw = BUTTON_PAD.0 * 2.0 + CHEVRON.0 + lw;
            let bh = lh.max(CHEVRON.1) + BUTTON_PAD.1 * 2.0;
            s.back = Some(rect(EDGE, centered(bh), bw, bh));
            left += bw;
        } else {
            s.back = None;
            s.hovered = false;
            s.pressed = false;
        }
        if let Some(v) = find_item(&self.items, c.left) {
            let x = left + GAP;
            left = x + v.state().frame.width;
            place(v, x);
        }
        let mut right = w;
        if let Some(v) = find_item(&self.items, c.right) {
            right = w - EDGE - GAP - v.state().frame.width;
            place(v, right);
        }
        // Items no slot names stay hidden.
        for item in &self.items {
            let id = Some(item.id());
            if id != c.left && id != c.right && id != c.title_view {
                set_concealed(surface, item, true);
            }
        }
        if let Some(v) = find_item(&self.items, c.title_view) {
            let fw = v.state().frame.width;
            place(v, ((w - fw) / 2.0).max(left));
            s.title_at = None;
        } else if let Some(title) = &s.title {
            let th = title.height;
            if c.large_title {
                title.fit((w - 32.0).max(0.0));
                s.title_at = Some((16.0, top + BAR + (BAR - th) / 2.0));
            } else {
                // Centered in the bar, as long as it fits between the items.
                let side = left.max(w - right) + GAP;
                let tw = title.width.min((w - 2.0 * side).max(0.0));
                title.fit(tw);
                s.title_at = Some(((w - tw) / 2.0, centered(th)));
            }
        } else {
            s.title_at = None;
        }
        drop(s);
        header.view.changed();
    }

    /// Keeps the header and its items over the app's content (which the
    /// app may insert after them), and the background under it.
    fn raise_header(&self) {
        let first = self.view.with_children(|c| c.first().cloned());
        if first.as_ref() != Some(&self.backdrop) {
            self.backdrop.insert_before(&self.view, first.as_ref());
        }
        let Some(header) = &self.header else { return };
        let mut tail = Vec::with_capacity(1 + self.items.len());
        tail.push(header.view.clone());
        tail.extend(self.items.iter().cloned());
        keep_last(&self.view, &tail);
    }
}

/// One stack.
struct StackNav {
    view: NshView,
    /// Its screens as shown, bottom to top (without popped ones).
    shown: Vec<NodeId>,
    /// Screens the user popped, until the app removes them.
    popped: Vec<NodeId>,
}

/// A sheet's scrim: a layer view under the sheet, covering the window.
struct Scrim {
    view: NshView,
    fill: Rc<Fill>,
    item: Rc<dyn Hosted>,
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
    /// The opaque background of a full-screen modal (its first child).
    backdrop: Option<NshView>,
    /// Dims the window behind a sheet; clicking it asks to close.
    scrim: Option<Scrim>,
    reported: Option<(Size, EdgeInsets)>,
    /// The field that had the keyboard focus when it was presented: it
    /// gets it back when the modal closes, as a desktop dialog gives it
    /// back.
    restore: Option<NshView>,
}

/// The screen under the moving one darkening (a fill it hosts while the
/// transition runs).
struct Dim {
    view: NshView,
    fill: Rc<Fill>,
    item: Rc<dyn Hosted>,
}

impl Dim {
    fn new(view: &NshView) -> Dim {
        let fill = Rc::new(Fill {
            color: Cell::new(Color::TRANSPARENT),
            closes: None,
            pressed: Cell::new(false),
        });
        let item: Rc<dyn Hosted> = fill.clone();
        let f = view.state().frame;
        view.set_native(&item, rect(0.0, 0.0, f.width, f.height));
        Dim {
            view: view.clone(),
            fill,
            item,
        }
    }

    /// Dims at `amount` (0 to 1).
    fn set(&self, amount: f32) {
        self.fill
            .set_color(with_alpha(Color::BLACK, DIM * amount), &self.view);
    }

    fn remove(&self) {
        self.view.remove_native(&self.item);
    }
}

/// What leaves the screen during a pop.
enum Leaving {
    /// The screen itself (popped by the user).
    View(NshView),
    /// A layer view (on top of the stack) holding a screen the app
    /// removed, with its views as they last showed.
    Departed(NshView),
}

enum Motion {
    Push {
        stack: NshView,
        entering: NshView,
        below: Option<NshView>,
        dim: Option<Dim>,
        fade: bool,
    },
    Pop {
        stack: NshView,
        leaving: Leaving,
        below: Option<NshView>,
        dim: Option<Dim>,
        fade: bool,
        /// The screen the user popped: concealed and reported at the end.
        popped: Option<NodeId>,
    },
    ModalIn {
        view: NshView,
        scrim: Option<NshView>,
        fade: bool,
    },
    ModalOut {
        /// The modal to report `onDismiss` for at the end (`None`: it was
        /// reported when the user closed it).
        dismiss: Option<NodeId>,
        /// A layer view on top of the root, covering it, holding the
        /// modal with its views as they last showed.
        layer: NshView,
        /// The modal (in the layer, unless the app presented it again).
        view: NshView,
        /// The modal's top edge (it slides down from there).
        top: f32,
        /// A full-screen modal's background, taken out at the end.
        backdrop: Option<NshView>,
        scrim: Option<NshView>,
        fade: bool,
    },
}

struct Transition {
    /// Time of its first frame (seconds, the surface's clock).
    start: Option<f64>,
    /// Milliseconds.
    duration: f64,
    motion: Motion,
}

impl Transition {
    fn stack(&self) -> Option<&NshView> {
        match &self.motion {
            Motion::Push { stack, .. } | Motion::Pop { stack, .. } => Some(stack),
            _ => None,
        }
    }

    /// Its progress at `now` (0 to 1 and beyond, not eased).
    fn progress(&self, now: f64) -> f64 {
        self.start
            .map_or(0.0, |s| (now - s) * 1000.0 / self.duration.max(1.0))
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
    /// Top screens and presented modals this batch started tearing down:
    /// their descendants stay attached until the end of the batch, so they
    /// can leave as they showed (see the module doc).
    keeping: Vec<NshView>,
    /// Views whose detaching was deferred: (view, its parent then, the
    /// kept screen or modal it is in).
    deferred: Vec<(NshView, NshView, NshView)>,
    /// The appearance the headers and backgrounds have.
    dark: Option<bool>,
}

impl Nav {
    /// Forgets the app's views (it stopped; the surface drops its frame
    /// callbacks too).
    pub(crate) fn reset(&mut self) {
        *self = Nav::default();
    }

    fn screen(&mut self, view: &NshView) -> &mut ScreenNav {
        self.screens
            .entry(view.id())
            .or_insert_with(|| ScreenNav::new(view))
    }

    /// Takes kept screen or modal `id` (see [`Nav::keeping`]).
    fn take_kept(&mut self, id: NodeId) -> Option<NshView> {
        let i = self.keeping.iter().position(|v| v.id() == id)?;
        Some(self.keeping.swap_remove(i))
    }

    /// True when `view` is in the layer of a screen or modal leaving.
    fn in_departing(&self, view: &NshView) -> bool {
        self.transitions.iter().any(|t| match &t.motion {
            Motion::Pop {
                leaving: Leaving::Departed(layer),
                ..
            }
            | Motion::ModalOut { layer, .. } => is_ancestor(layer, view),
            _ => false,
        })
    }

    /// Whether the mounter's detaching of `view` waits (see
    /// [`defer_detach`]).
    fn defers(&mut self, view: &NshView) -> bool {
        let Some(parent) = view.parent_view() else {
            return false;
        };
        if self.in_departing(&parent) {
            return true;
        }
        let kept = self
            .keeping
            .iter()
            .find(|k| *k != view && is_ancestor(k, &parent))
            .cloned();
        match kept {
            Some(kept) => {
                self.deferred.push((view.clone(), parent, kept));
                true
            }
            None => false,
        }
    }

    /// The deferred views to detach after all: those whose screen or modal
    /// stayed (still shown where it was, not leaving) and that are still
    /// where they were. The screens and modals that stayed are no longer
    /// kept.
    fn stayed(&mut self) -> Vec<NshView> {
        let gone = |nav: &Nav, k: &NshView| !k.attached() || nav.in_departing(k);
        let stayed: Vec<NshView> = self
            .keeping
            .iter()
            .filter(|k| !gone(self, k))
            .cloned()
            .collect();
        self.keeping.retain(|k| !stayed.contains(k));
        let mut detach = Vec::new();
        self.deferred.retain(|(view, parent, kept)| {
            if !stayed.contains(kept) {
                return true;
            }
            if view.parent_view().as_ref() == Some(parent) {
                detach.push(view.clone());
            }
            false
        });
        detach
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
    fn apply(surface: &Surface, motion: &Motion, e: f32) {
        match motion {
            Motion::Push {
                stack,
                entering,
                below,
                dim,
                fade,
            } => {
                let w = stack.state().frame.width;
                if *fade {
                    set_fade(entering, e);
                } else {
                    set_shift(entering, (w * (1.0 - e), 0.0));
                    if let Some(below) = below {
                        set_shift(below, (-PARALLAX * w * e, 0.0));
                    }
                }
                if let Some(dim) = dim {
                    dim.set(e);
                }
            }
            Motion::Pop {
                stack,
                leaving,
                below,
                dim,
                fade,
                ..
            } => {
                let w = stack.state().frame.width;
                let v = match leaving {
                    Leaving::View(v) | Leaving::Departed(v) => v,
                };
                if *fade {
                    set_fade(v, 1.0 - e);
                } else {
                    set_shift(v, (w * e, 0.0));
                    if let Some(below) = below {
                        set_shift(below, (-PARALLAX * w * (1.0 - e), 0.0));
                    }
                }
                if let Some(dim) = dim {
                    dim.set(1.0 - e);
                }
            }
            Motion::ModalIn { view, scrim, fade } => {
                if *fade {
                    set_fade(view, e);
                } else {
                    // From below the window's bottom edge.
                    let h = surface.root.state().frame.height;
                    let y = view.state().frame.y;
                    set_shift(view, (0.0, (h - y) * (1.0 - e)));
                }
                if let Some(scrim) = scrim {
                    set_fade(scrim, e);
                }
            }
            Motion::ModalOut {
                layer,
                top,
                scrim,
                fade,
                ..
            } => {
                if *fade {
                    set_fade(layer, 1.0 - e);
                } else {
                    let h = surface.root.state().frame.height;
                    set_shift(layer, (0.0, (h - top) * e));
                }
                if let Some(scrim) = scrim {
                    set_fade(scrim, 1.0 - e);
                }
            }
        }
    }

    fn end(&mut self, surface: &Surface, t: Transition) {
        Self::apply(surface, &t.motion, 1.0);
        match t.motion {
            Motion::Push { stack, dim, .. } => {
                if let Some(dim) = dim {
                    dim.remove();
                }
                self.settle(surface, stack.id());
            }
            Motion::Pop {
                stack,
                leaving,
                dim,
                popped,
                ..
            } => {
                if let Some(dim) = dim {
                    dim.remove();
                }
                match leaving {
                    Leaving::View(v) => {
                        set_concealed(surface, &v, true);
                        set_shift(&v, (0.0, 0.0));
                        set_fade(&v, 1.0);
                    }
                    Leaving::Departed(layer) => layer.unparent(),
                }
                self.settle(surface, stack.id());
                if let Some(id) = popped {
                    surface.event(id, EventKind::Dismiss, EventPayload::None);
                }
            }
            Motion::ModalIn { view, scrim, .. } => {
                set_shift(&view, (0.0, 0.0));
                set_fade(&view, 1.0);
                if let Some(scrim) = scrim {
                    set_fade(&scrim, 1.0);
                }
                surface.event(view.id(), EventKind::Show, EventPayload::None);
            }
            Motion::ModalOut {
                dismiss,
                layer,
                backdrop,
                scrim,
                ..
            } => {
                // Presented again, the modal gets a new one.
                if let Some(b) = backdrop {
                    b.unparent();
                }
                layer.unparent();
                if let Some(scrim) = scrim {
                    scrim.unparent();
                }
                if let Some(id) = dismiss {
                    surface.event(id, EventKind::Dismiss, EventPayload::None);
                }
            }
        }
    }

    /// Shows only the top screen of `stack`, where it belongs.
    fn settle(&mut self, surface: &Surface, stack: NodeId) {
        let Some(st) = self.stacks.get(&stack) else {
            return;
        };
        let top = st.shown.last().copied();
        for view in st.view.child_views() {
            if view.kind() != ViewKind::Screen {
                continue;
            }
            set_concealed(surface, &view, Some(view.id()) != top);
            set_shift(&view, (0.0, 0.0));
            set_fade(&view, 1.0);
        }
    }

    fn start(&mut self, surface: &Rc<Surface>, duration: f64, motion: Motion) {
        Self::apply(surface, &motion, 0.0);
        self.transitions.push(Transition {
            start: None,
            duration,
            motion,
        });
        if self.ticking {
            return;
        }
        self.ticking = true;
        surface.on_frame(|surface, now| {
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
                Self::apply(surface, &self.transitions[i].motion, ease(p));
                i += 1;
            }
        }
    }

    /// Shows the app's screens of `stack` after a batch changed them.
    fn reconcile(&mut self, surface: &Rc<Surface>, stack_id: NodeId) {
        self.finish(surface, |t| t.stack().is_some_and(|s| s.id() == stack_id));
        let dark = crate::system::dark(surface);
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
        // The top screen the app removed, with its views as they showed.
        let departed = match old {
            Some(old) if Some(old) != new && find(old).is_none() => self.take_kept(old),
            _ => None,
        };
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
            // Every time (it only reports changes): the stack may have been
            // sized before its screens were known to it.
            s.report(surface, &stack);
        }
        if children.is_empty() && stack.parent_view().is_none() {
            // The stack is gone.
            self.stacks.remove(&stack_id);
            return;
        }
        self.settle(surface, stack_id);
        let (Some(old), Some(new)) = (old, new) else {
            return;
        };
        if old == new || frame.width <= 0.0 {
            return;
        }
        let pushing = !prev.contains(&new);
        let animation = if pushing {
            self.screens.get(&new).map(|s| s.config.animation)
        } else {
            leaving_animation
        };
        let Some((duration, fade)) =
            screen_motion(animation.unwrap_or_default(), reduce_motion(surface))
        else {
            return;
        };
        let below = if pushing { find(old) } else { find(new) };
        if let Some(b) = &below {
            set_concealed(surface, b, false);
        }
        let dim = below.as_ref().filter(|_| !fade).map(Dim::new);
        if pushing {
            let Some(entering) = find(new) else { return };
            let motion = Motion::Push {
                stack,
                entering,
                below,
                dim,
                fade,
            };
            self.start(surface, duration, motion);
        } else {
            let leaving = match find(old) {
                Some(v) => {
                    set_concealed(surface, &v, false);
                    Leaving::View(v)
                }
                None => {
                    let Some(view) = departed else {
                        if let Some(dim) = dim {
                            dim.remove();
                        }
                        self.settle(surface, stack_id);
                        return;
                    };
                    let layer = departing_layer(&view, (frame.width, frame.height));
                    layer.insert_before(&stack, None);
                    Leaving::Departed(layer)
                }
            };
            let motion = Motion::Pop {
                stack,
                leaving,
                below,
                dim,
                fade,
                popped: None,
            };
            self.start(surface, duration, motion);
        }
    }

    /// The user goes back in `stack`: tells the screen if it prevents it,
    /// else pops it natively and tells the app once it is gone.
    fn back(&mut self, surface: &Rc<Surface>, stack_id: NodeId) {
        self.finish(surface, |t| t.stack().is_some_and(|s| s.id() == stack_id));
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
        let animation = self.screens.get(&top).map(|s| s.config.animation);
        match (
            screen_motion(animation.unwrap_or_default(), reduce_motion(surface)),
            find(top),
        ) {
            (Some((duration, fade)), Some(leaving)) => {
                if let Some(b) = &below {
                    set_concealed(surface, b, false);
                }
                let dim = below.as_ref().filter(|_| !fade).map(Dim::new);
                let motion = Motion::Pop {
                    stack,
                    leaving: Leaving::View(leaving),
                    below,
                    dim,
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
            within.is_none_or(|w| is_ancestor(w, &st.view))
                && (st.shown.len() > 1
                    || st.shown.last().is_some_and(|top| {
                        self.screens.get(top).is_some_and(|s| s.config.prevent_back)
                    }))
                && is_shown(&st.view)
        };
        let candidates: Vec<&StackNav> = self.stacks.values().filter(|s| wants(s)).collect();
        candidates
            .iter()
            .find(|c| {
                !candidates
                    .iter()
                    .any(|o| o.view != c.view && is_ancestor(&c.view, &o.view))
            })
            .map(|c| c.view.id())
    }

    /// Takes modal `id` away natively (it leaves as it last showed, its
    /// views kept), then reports `onDismiss` unless `reported` already.
    fn close_modal(&mut self, surface: &Rc<Surface>, id: NodeId, reported: bool) {
        self.finish(
            surface,
            |t| matches!(&t.motion, Motion::ModalIn { view, .. } if view.id() == id),
        );
        self.order.retain(|&m| m != id);
        // Its descendants were kept for this; once it is in its departing
        // layer, they stay with it.
        self.take_kept(id);
        let Some(m) = self.modals.get_mut(&id) else {
            return;
        };
        let Some(view) = m.view.take() else { return };
        let restore = m.restore.take();
        let motion = modal_motion(m.config.animation, reduce_motion(surface));
        let frame = view.state().frame;
        let leaves = motion.filter(|_| frame.width > 0.0 && is_shown(&view));
        // A full-screen modal's background leaves with it, then goes.
        let backdrop = m.backdrop.take();
        let scrim = m.scrim.take().map(|s| s.view);
        drop_focus(surface, &view);
        view.unparent();
        set_shift(&view, (0.0, 0.0));
        set_fade(&view, 1.0);
        if let Some(field) = restore
            && field.attached()
            && crate::text_input::focused(surface).is_none()
            && is_ancestor(&self.focus_scope(surface), &field)
        {
            // Not while another modal still covers it.
            crate::text_input::focus(&field);
        }
        match leaves {
            Some((duration, fade)) => {
                let f = surface.root.state().frame;
                let layer = departing_layer(&view, (f.width, f.height));
                layer.insert_before(&surface.root, None);
                let motion = Motion::ModalOut {
                    dismiss: (!reported).then_some(id),
                    view,
                    layer,
                    top: frame.y,
                    backdrop,
                    scrim,
                    fade,
                };
                self.start(surface, duration, motion);
            }
            None => {
                if let Some(b) = backdrop {
                    b.unparent();
                }
                if let Some(scrim) = scrim {
                    scrim.unparent();
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
        let scrim = m.scrim.as_ref().map(|s| s.view.clone());
        match modal_motion(m.config.animation, reduce_motion(surface)) {
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
        self.go_back_to(
            surface,
            surface.root.state().events.contains(EventKind::BackPress),
        )
    }

    /// Goes back to what [`Nav::back_target`] names.
    fn go_back_to(&mut self, surface: &Rc<Surface>, back_press: bool) -> bool {
        match self.back_target(back_press) {
            Some(Back::Stack(stack)) => self.back(surface, stack),
            Some(Back::Modal(id)) => self.request_close(surface, id),
            Some(Back::App) => surface.event(ROOT_ID, EventKind::BackPress, EventPayload::None),
            None => return false,
        }
        true
    }

    /// Where the keyboard focus may go: the newest modal, else the whole
    /// surface.
    fn focus_scope(&self, surface: &Surface) -> NshView {
        self.order
            .last()
            .and_then(|id| self.modals.get(id))
            .and_then(|m| m.view.clone())
            .unwrap_or_else(|| surface.root.clone())
    }

    /// What going back acts on (`back_press`: the app listens to
    /// `onBackPress`, and hears it before the stacks).
    fn back_target(&self, back_press: bool) -> Option<Back> {
        if let Some(&id) = self.order.last() {
            let view = self.modals.get(&id).and_then(|m| m.view.clone());
            return Some(match self.back_stack(view.as_ref()) {
                Some(stack) => Back::Stack(stack),
                None => Back::Modal(id),
            });
        }
        if back_press {
            return Some(Back::App);
        }
        self.back_stack(None).map(Back::Stack)
    }

    /// Keeps the presented modals (and those leaving) over the app's
    /// views, each sheet right above its scrim.
    fn raise_modals(&self, surface: &Surface) {
        let mut tail: Vec<NshView> = Vec::new();
        for m in self.order.iter().filter_map(|id| self.modals.get(id)) {
            if let Some(scrim) = &m.scrim {
                tail.push(scrim.view.clone());
            }
            if let Some(view) = &m.view {
                tail.push(view.clone());
            }
        }
        for t in &self.transitions {
            if let Motion::ModalOut { layer, scrim, .. } = &t.motion {
                if let Some(scrim) = scrim {
                    tail.push(scrim.clone());
                }
                tail.push(layer.clone());
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
        let sheet = sheet_frame(&m.config, f.width, f.height);
        let insets = if sheet.is_some() {
            // A card inside the window: no system UI over it.
            EdgeInsets::default()
        } else {
            let safe = surface.environment.get().safe_area;
            EdgeInsets {
                top: safe.top,
                right: safe.right,
                bottom: safe.bottom,
                left: safe.left,
            }
        };
        let frame = sheet.unwrap_or(rect(0.0, 0.0, f.width, f.height));
        set_frame(view, frame);
        if let Some(b) = &m.backdrop {
            set_frame(b, rect(0.0, 0.0, frame.width, frame.height));
        }
        if let Some(scrim) = &m.scrim {
            set_frame(&scrim.view, rect(0.0, 0.0, f.width, f.height));
            scrim
                .view
                .set_native(&scrim.item, rect(0.0, 0.0, f.width, f.height));
        }
        let size = Size::new(frame.width, frame.height);
        report(surface, id, &mut m.reported, size, insets);
    }

    /// Gives presented modal `id` the scrim of a sheet, or takes it away.
    fn sheet_scrim(&mut self, surface: &Surface, id: NodeId) {
        let Some(m) = self.modals.get_mut(&id) else {
            return;
        };
        let Some(view) = &m.view else {
            return;
        };
        if !is_sheet(&m.config) {
            if let Some(scrim) = m.scrim.take() {
                scrim.view.unparent();
            }
            return;
        }
        if m.scrim.is_some() {
            return;
        }
        let fill = Rc::new(Fill {
            color: Cell::new(theme(crate::system::dark(surface)).scrim),
            closes: Some((surface.key(), id)),
            pressed: Cell::new(false),
        });
        let item: Rc<dyn Hosted> = fill.clone();
        let scrim = layer_view();
        let f = surface.root.state().frame;
        set_frame(&scrim, rect(0.0, 0.0, f.width, f.height));
        scrim.set_native(&item, rect(0.0, 0.0, f.width, f.height));
        // Right under the sheet.
        scrim.insert_before(&surface.root, Some(view));
        m.scrim = Some(Scrim {
            view: scrim,
            fill,
            item,
        });
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
            scrim.fill.set_color(t.scrim, &scrim.view);
        }
        if m.config.transparent || sheet {
            if let Some(b) = m.backdrop.take() {
                b.unparent();
            }
            return;
        }
        let backdrop = m.backdrop.get_or_insert_with(|| {
            let layer = layer_view();
            // First, under the app's content.
            let first = view.with_children(|c| c.first().cloned());
            layer.insert_before(view, first.as_ref());
            layer
        });
        set_background(backdrop, t.window);
        let f = view.state().frame;
        set_frame(backdrop, rect(0.0, 0.0, f.width, f.height));
    }

    /// Restyles the backgrounds, headers and sheets when the appearance
    /// changed since they were styled.
    fn follow_appearance(&mut self, surface: &Surface) {
        let dark = crate::system::dark(surface);
        if self.dark.replace(dark).is_none_or(|d| d == dark) {
            return;
        }
        for s in self.screens.values_mut() {
            s.style(surface, dark);
        }
        let ids: Vec<NodeId> = self.modals.keys().copied().collect();
        for id in ids {
            self.style_modal(dark, id);
        }
    }

    /// Keeps the views of `view` (a top screen or a presented modal)
    /// until the end of the batch, unless it would leave without a
    /// transition anyway.
    fn keep(&mut self, view: &NshView, animated: bool) {
        if animated && !self.keeping.contains(view) {
            self.keeping.push(view.clone());
        }
    }
}

/// What going back acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Back {
    /// This stack pops (or its top screen hears `onBackRequest`).
    Stack(NodeId),
    /// The newest modal is asked to close.
    Modal(NodeId),
    /// The app's `onBackPress`.
    App,
}

/// `Command::Back`: goes back as without the app's listeners, once the
/// batch being applied is in.
pub(crate) fn default_back(surface: &Rc<Surface>) {
    let key = surface.key();
    crate::rt::invoke(move || {
        if let Some(surface) = Surface::by_key(key)
            && let Ok(mut nav) = surface.nav.try_borrow_mut()
        {
            nav.go_back_to(&surface, false);
        }
    });
}

/// Goes back (the newest modal, the innermost stack or the app's
/// `onBackPress`): whether something took it.
pub(crate) fn go_back(surface: &Rc<Surface>) -> bool {
    match surface.nav.try_borrow_mut() {
        Ok(mut nav) => nav.go_back(surface),
        Err(_) => false,
    }
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

/// A key nothing in the window used: Escape, Alt+Left and the Browser Back
/// key go back. Whether it was taken. Escape in a focused text field only
/// leaves the field (a desktop convention: it never navigates away from
/// what is being typed); Alt+Left and the Back key still go back. A key
/// held down goes back once.
pub(crate) fn key_pressed(surface: &Rc<Surface>, event: &KeyEvent) -> bool {
    if !event.down || event.repeat || !is_back_key(event.vk, event.modifiers) {
        return false;
    }
    if event.vk == VK_ESCAPE
        && let Some(field) = crate::text_input::focused(surface)
    {
        crate::text_input::blur(&field);
        return true;
    }
    go_back(surface)
}

/// A view is about to be removed or deleted: when it is (or is inside) the
/// top screen of a stack or a presented modal, that screen's or modal's
/// views are kept until the end of the batch ([`defer_detach`]), to leave
/// as they showed if the app removes it (later in the batch, in whatever
/// order the app releases its views).
pub(crate) fn removing(surface: &Rc<Surface>, view: &NshView) {
    let Ok(mut nav) = surface.nav.try_borrow_mut() else {
        return;
    };
    if nav.stacks.is_empty() && nav.order.is_empty() {
        return;
    }
    let reduce = reduce_motion(surface);
    let mut child = view.clone();
    loop {
        let parent = child.parent_view();
        let id = child.id();
        match child.kind() {
            ViewKind::Screen => {
                let on_top = parent.as_ref().is_some_and(|p| {
                    nav.stacks
                        .get(&p.id())
                        .is_some_and(|st| st.shown.last() == Some(&id))
                });
                if on_top {
                    let animated = nav
                        .screens
                        .get(&id)
                        .is_none_or(|s| screen_motion(s.config.animation, reduce).is_some());
                    nav.keep(&child, animated);
                }
            }
            ViewKind::Modal => {
                let presented = nav
                    .modals
                    .get(&id)
                    .and_then(|m| m.view.as_ref().map(|v| (v == &child, m.config.animation)));
                if let Some((true, animation)) = presented {
                    let animated = modal_motion(animation, reduce).is_some();
                    nav.keep(&child, animated);
                }
            }
            _ => {}
        }
        match parent {
            Some(p) => child = p,
            None => break,
        }
    }
}

/// The mounter detaches `view` (removed or deleted): true when that waits,
/// because `view` is inside a top screen or modal the batch started
/// tearing down (it may leave as it showed; [`take_deferred`] hands back
/// the views to detach after all) or inside one leaving (it goes with
/// it).
pub(crate) fn defer_detach(surface: &Surface, view: &NshView) -> bool {
    match surface.nav.try_borrow_mut() {
        Ok(mut nav) => nav.defers(view),
        Err(_) => false,
    }
}

/// The batch is applied, before [`end_batch`]: the views whose detaching
/// waited and that the mounter detaches now (their screen or modal
/// stayed).
pub(crate) fn take_deferred(surface: &Surface) -> Vec<NshView> {
    match surface.nav.try_borrow_mut() {
        Ok(mut nav) => nav.stayed(),
        Err(_) => Vec::new(),
    }
}

/// A screen was pushed (`added`) or popped in `stack`.
pub(crate) fn stack_changed(surface: &Rc<Surface>, stack: &NshView, added: Option<&NshView>) {
    let mut nav = surface.nav.borrow_mut();
    let id = stack.id();
    nav.stacks.entry(id).or_insert_with(|| StackNav {
        view: stack.clone(),
        shown: Vec::new(),
        popped: Vec::new(),
    });
    if let Some(screen) = added.filter(|v| v.kind() == ViewKind::Screen) {
        nav.screen(screen);
        // Until the end of the batch decides which screen shows: a screen
        // pushed under the top one must not flash.
        if nav.stacks[&id]
            .shown
            .last()
            .is_some_and(|&top| top != screen.id())
        {
            set_concealed(surface, screen, true);
        }
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
    let dark = crate::system::dark(surface);
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
    nav.style_modal(crate::system::dark(surface), id);
    // Its presentation decides its frame and whether a scrim dims the
    // window.
    nav.sheet_scrim(surface, id);
    nav.size_modal(surface, id);
}

/// A modal was inserted: present it.
pub(crate) fn present(surface: &Rc<Surface>, view: &NshView) {
    let mut nav = surface.nav.borrow_mut();
    let id = view.id();
    if nav.modals.get(&id).is_some_and(|m| m.view.is_some()) {
        return;
    }
    // Presented again while it leaves: it stops leaving.
    nav.finish(
        surface,
        |t| matches!(&t.motion, Motion::ModalOut { view: v, .. } if v == view),
    );
    let m = nav.modals.entry(id).or_default();
    // The field being typed in under the modal leaves it (until the modal
    // closes); one of the modal's own keeps it.
    let field = crate::text_input::focused(surface).filter(|f| !is_ancestor(view, f));
    if let Some(field) = &field {
        crate::text_input::blur(field);
    }
    m.restore = field;
    m.view = Some(view.clone());
    m.closed = false;
    m.reported = None;
    view.insert_before(&surface.root, None);
    nav.order.push(id);
    nav.style_modal(crate::system::dark(surface), id);
    nav.sheet_scrim(surface, id);
    nav.size_modal(surface, id);
    // Animated in at the end of the batch, with the options it brings
    // (until then it does not show).
    if nav.modals[&id].config.animation != ModalAnimation::None {
        set_fade(view, 0.0);
        if let Some(scrim) = &nav.modals[&id].scrim {
            set_fade(&scrim.view, 0.0);
        }
    }
    nav.presenting.push(id);
}

/// Where the keyboard focus may go (Tab): the newest modal, else the whole
/// surface.
pub(crate) fn focus_scope(surface: &Surface) -> NshView {
    surface.nav.borrow().focus_scope(surface)
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
            drop_focus(surface, view);
            view.unparent();
        }
    }
}

/// A header item was inserted into `parent` (a screen).
pub(crate) fn header_item(surface: &Rc<Surface>, parent: &NshView, child: &NshView) {
    let mut nav = surface.nav.borrow_mut();
    if child.parent_view().as_ref() != Some(parent) {
        child.insert_before(parent, None);
    }
    let s = nav.screen(parent);
    if !s.items.contains(child) {
        s.items.push(child.clone());
    }
    // Placed (and shown) by the header's layout at the end of the batch.
    set_concealed(surface, child, true);
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
    view.unparent();
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
    nav.follow_appearance(surface);
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
    // Screens and modals that left took their views along.
    nav.keeping.clear();
    nav.deferred.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eases_out() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert_eq!(ease(2.0), 1.0);
        assert_eq!(ease(-1.0), 0.0);
        assert!(ease(0.5) > 0.5);
        // Monotonic.
        let mut last = 0.0;
        for i in 1..=20 {
            let e = ease(i as f64 / 20.0);
            assert!(e >= last);
            last = e;
        }
    }

    #[test]
    fn transitions_progress_from_their_first_frame() {
        let t = Transition {
            start: None,
            duration: SLIDE_MS,
            motion: Motion::ModalIn {
                view: layer_view(),
                scrim: None,
                fade: true,
            },
        };
        // Not started: at the beginning.
        assert_eq!(t.progress(5.0), 0.0);
        let t = Transition {
            start: Some(2.0),
            ..t
        };
        assert_eq!(t.progress(2.0), 0.0);
        assert!((t.progress(2.15) - 0.5).abs() < 1e-9);
        assert!(t.progress(2.3) >= 1.0 - 1e-9);
    }

    #[test]
    fn motions_follow_the_animation_and_reduce_motion() {
        assert_eq!(screen_motion(ScreenAnimation::None, false), None);
        assert_eq!(
            screen_motion(ScreenAnimation::Default, false),
            Some((SLIDE_MS, false))
        );
        assert_eq!(
            screen_motion(ScreenAnimation::Default, true),
            Some((FADE_MS, true))
        );
        assert_eq!(
            screen_motion(ScreenAnimation::Fade, false),
            Some((FADE_MS, true))
        );
        assert_eq!(modal_motion(ModalAnimation::None, true), None);
        assert_eq!(
            modal_motion(ModalAnimation::Slide, false),
            Some((MODAL_MS, false))
        );
        assert_eq!(
            modal_motion(ModalAnimation::Slide, true),
            Some((FADE_MS, true))
        );
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

    #[test]
    fn back_keys() {
        let none = Modifiers::default();
        let alt = Modifiers { alt: true, ..none };
        let ctrl = Modifiers { ctrl: true, ..none };
        assert!(is_back_key(VK_ESCAPE, none));
        assert!(is_back_key(VK_BROWSER_BACK, none));
        assert!(is_back_key(VK_LEFT, alt));
        assert!(!is_back_key(VK_LEFT, none));
        assert!(!is_back_key(VK_ESCAPE, ctrl));
        assert!(!is_back_key(VK_LEFT, Modifiers { ctrl: true, ..alt }));
        assert!(!is_back_key(0x41, none));
    }

    /// A root that counts as attached to a surface, and views under it.
    fn root() -> NshView {
        let root = NshView::new(ROOT_ID, ViewKind::View);
        root.set_invalidate(Rc::new(|| {}));
        root
    }

    fn child(parent: &NshView, id: NodeId, kind: ViewKind) -> NshView {
        let v = NshView::new(id, kind);
        v.insert_before(parent, None);
        v
    }

    /// A stack under `parent` with screens `ids` (the last on top).
    fn stack(nav: &mut Nav, parent: &NshView, id: NodeId, ids: &[NodeId]) -> NshView {
        let stack = child(parent, id, ViewKind::Stack);
        for &s in ids {
            let screen = child(&stack, s, ViewKind::Screen);
            nav.screen(&screen);
            if s != *ids.last().unwrap() {
                screen.state_mut().concealed = true;
            }
        }
        nav.stacks.insert(
            id,
            StackNav {
                view: stack.clone(),
                shown: ids.to_vec(),
                popped: Vec::new(),
            },
        );
        stack
    }

    #[test]
    fn going_back_pops_the_innermost_stack() {
        let root = root();
        let mut nav = Nav::default();
        // Nothing to pop: the app's onBackPress, if it listens.
        assert_eq!(nav.back_target(false), None);
        assert_eq!(nav.back_target(true), Some(Back::App));
        let outer = stack(&mut nav, &root, 2, &[4, 6]);
        assert_eq!(nav.back_target(false), Some(Back::Stack(2)));
        // The app's listeners hear it before the stack pops.
        assert_eq!(nav.back_target(true), Some(Back::App));
        // A stack inside the top screen, with a screen to pop: it goes
        // first.
        let top = outer.child_views().last().cloned().unwrap();
        stack(&mut nav, &top, 8, &[10, 12]);
        assert_eq!(nav.back_target(false), Some(Back::Stack(8)));
        // A stack in a concealed screen does not count.
        let below = outer.child_views()[0].clone();
        stack(&mut nav, &below, 14, &[16, 18]);
        assert_eq!(nav.back_target(false), Some(Back::Stack(8)));
        // A single screen that prevents going back still hears it.
        let lone = stack(&mut nav, &root, 20, &[22]);
        nav.stacks.remove(&2);
        nav.stacks.remove(&8);
        nav.screens.get_mut(&22).unwrap().config.prevent_back = true;
        assert_eq!(nav.back_target(false), Some(Back::Stack(20)));
        drop(lone);
    }

    #[test]
    fn going_back_asks_the_newest_modal_first() {
        let root = root();
        let mut nav = Nav::default();
        stack(&mut nav, &root, 2, &[4, 6]);
        let modal = child(&root, 30, ViewKind::Modal);
        nav.modals.insert(
            30,
            ModalNav {
                view: Some(modal.clone()),
                ..ModalNav::default()
            },
        );
        nav.order.push(30);
        // The stack outside the modal does not pop.
        assert_eq!(nav.back_target(true), Some(Back::Modal(30)));
        // A stack inside it does.
        stack(&mut nav, &modal, 32, &[34, 36]);
        assert_eq!(nav.back_target(true), Some(Back::Stack(32)));
        // Popped down to one screen: the modal is asked to close.
        nav.stacks.get_mut(&32).unwrap().shown.pop();
        assert_eq!(nav.back_target(true), Some(Back::Modal(30)));
    }

    #[test]
    fn header_views_stay_above_the_content() {
        let screen = NshView::new(4, ViewKind::Screen);
        let mut s = ScreenNav::new(&screen);
        s.header = Some(Header::new(0, 4));
        let content = child(&screen, 6, ViewKind::View);
        s.raise_header();
        let header = s.header.as_ref().unwrap().view.clone();
        assert_eq!(
            screen.child_views(),
            vec![s.backdrop.clone(), content.clone(), header.clone()]
        );
        // Content inserted later goes under the header again.
        let more = child(&screen, 8, ViewKind::View);
        s.raise_header();
        assert_eq!(
            screen.child_views(),
            vec![s.backdrop.clone(), content, more, header]
        );
    }

    /// A line of `n` glyphs 10 points wide each.
    fn line(n: usize) -> Arc<LineLayout> {
        let glyphs = (0..n)
            .map(|i| gpui::ShapedGlyph {
                id: GlyphId(i as u32),
                position: gpui::point(px(i as f32 * 10.0), px(0.0)),
                index: i,
                is_emoji: false,
            })
            .collect();
        Arc::new(LineLayout {
            font_size: px(15.0),
            width: px(n as f32 * 10.0),
            ascent: px(12.0),
            descent: px(4.0),
            runs: vec![gpui::ShapedRun {
                font_id: FontId(0),
                glyphs,
            }],
            len: n,
        })
    }

    #[test]
    fn titles_that_do_not_fit_end_with_an_ellipsis() {
        let text = TextLine {
            layout: line(10),
            ellipsis: line(1),
            width: 100.0,
            height: 16.0,
            max: Cell::new(f32::INFINITY),
        };
        assert_eq!(text.glyphs().len(), 10);
        text.fit(100.0);
        assert_eq!(text.glyphs().len(), 10);
        // 55 points: four glyphs (40) and the ellipsis (10).
        text.fit(55.0);
        let g = text.glyphs();
        assert_eq!(g.len(), 5);
        assert_eq!(g[4].x, 40.0);
        assert_eq!(g[4].id, GlyphId(0));
        // No room even for the ellipsis: nothing.
        text.fit(5.0);
        assert!(text.glyphs().is_empty());
    }

    #[test]
    fn a_departing_screen_keeps_its_views() {
        let root = root();
        let mut nav = Nav::default();
        let stack = stack(&mut nav, &root, 2, &[4, 6]);
        let top = stack.child_views()[1].clone();
        let content = child(&top, 8, ViewKind::View);
        let label = child(&content, 10, ViewKind::Text);
        // Nothing kept: views detach at once.
        assert!(!nav.defers(&label));
        // The batch tears the top screen down: its views wait.
        nav.keep(&top, true);
        assert!(nav.defers(&label));
        assert!(nav.defers(&content));
        // The screen itself leaves its stack.
        assert!(!nav.defers(&top));
        top.unparent();
        // It is gone: nothing is detached, the subtree is intact.
        assert!(nav.stayed().is_empty());
        assert_eq!(label.parent_view(), Some(content.clone()));
        assert_eq!(nav.take_kept(6), Some(top));
    }

    #[test]
    fn views_of_a_screen_that_stayed_detach_at_the_end() {
        let root = root();
        let mut nav = Nav::default();
        let stack = stack(&mut nav, &root, 2, &[4]);
        let top = stack.child_views()[0].clone();
        let a = child(&top, 8, ViewKind::View);
        let b = child(&top, 10, ViewKind::View);
        nav.keep(&top, true);
        assert!(nav.defers(&a));
        assert!(nav.defers(&b));
        // `b` moved elsewhere in the batch: it stays where it went.
        b.insert_before(&root, None);
        assert_eq!(nav.stayed(), vec![a]);
        assert!(nav.keeping.is_empty());
        assert!(nav.deferred.is_empty());
        // A screen that leaves without a transition keeps nothing.
        nav.keep(&top, false);
        assert!(nav.keeping.is_empty());
    }
}
