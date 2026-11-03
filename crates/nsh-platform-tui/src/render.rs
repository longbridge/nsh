//! The painter: the retained view tree drawn in the terminal's own look, as
//! a well-made terminal app draws, never the GUI copied Cell by Cell.
//!
//! Frames are in points; a view's Cells are its frame divided by the
//! [`CellSize`] (the core lays out on the grid in the terminal idiom, so
//! frames arrive in whole Cells and every border owns its Cells). Views
//! paint in tree order (the core orders siblings by `zIndex`), modals after
//! the root, system dialogs ([`crate::dialog`]) over everything.
//!
//! - **Colors** are the terminal's theme ([`crate::palette`]): the body
//!   keeps the terminal's default foreground and background
//!   (`Color::Reset`); app colors become the default foreground, DIM or a
//!   named accent. Fills, radii, shadows, gradients, outlines, opacity
//!   tints and filters are the GUI's and draw nothing; an opaque fill only
//!   decides what the colors drawn on it are read against. A gradient
//!   over no fill of its own is a fill of its first stop.
//! - **Borders** are single-line box drawing in the dim default color (an
//!   accent when the app colored them), rounded `╭╮╰╯` where a corner has a
//!   radius, on the sides the layout gave a Cell: the core sends a border
//!   width only for those (a card's), never on a control, a box of text or
//!   a small box.
//! - **Text** keeps its weight (bold from 600), slant, underline and
//!   strike-through; a run whose background is an accent is a reversed
//!   band of that accent (a highlight).
//! - **State** is drawn from what views are, never from how the app drew
//!   them: a selected or current item (`selected`, `current`, a checked
//!   menu item, an on toggle button) is a reversed band over its Cells; a
//!   focused control is reversed over its text (a text field: its text
//!   underlined, the terminal cursor its caret; a large container: its
//!   border bold, or its text bold and underlined); a hovered pressable
//!   view is a faint band ([`Bands`]), a pressed one a stronger band
//!   (reversed text at 16 colors); a disabled view is dim and never bold.
//!   A control whose own fill stands out from what it is on (a primary
//!   button) has bold text in the fill's accent.
//! - **Overlays**: what is behind a modal dims; the boxes of a modal or an
//!   anchored overlay (a border or an opaque fill) are cleared over what is
//!   behind them, and an overlay's outermost box without a border of its
//!   own gets a rounded dim one, its title the overlay's accessibility
//!   label.
//! - **Scroll bars** ([`crate::scrollbar`]): a dim track `│` with a thumb
//!   `┃` in the default color over the inner right (bottom) edge, only when
//!   the content overflows.
//! - **Unsupported** content is left out: images, video, web views,
//!   native components without a terminal rendering and icon-sized
//!   canvases take no Cells in the layout (the mounter logs each content
//!   kind once). A control whose content is all left out (an icon button)
//!   draws its accessibility label as its label.
//! - **Clipping**: a text draws inside its own view's box; a view that
//!   clips, scrolls or has a border keeps its children inside its content
//!   box (the border excluded), anchored overlays stand inside the
//!   surface; a one-line text cut at its right edge ends in `…`.
//!
//! Drawing the same tree twice changes no Cell.

use nsh_core::{
    AccessibilityRole, BorderStyle, Bounds, Checked, CornerRadii, NodeId, Point, PointerEvents,
    Points, ROOT_ID, TransformKind, ViewKind, tailwind::state,
};
use ratatui_core::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier},
};

use crate::{
    CellSize, dialog,
    geometry::CellBounds,
    mount::Mounter,
    palette::{self, Bands, Ground, HOVER, Ramp, SELECT, Surface, Tone, ink, stands_out},
    scrollbar::ScrollBar,
    text::CellLayout,
    view::NshView,
};

/// Below this opacity, multiplied through its ancestors, a view draws
/// nothing; above it opacity is ignored (the terminal has no translucency).
const INVISIBLE_OPACITY: f32 = 0.03;
/// From this many rows inside its border a focused view is a container,
/// shown by its border (or bold, underlined text) instead of a reversed
/// label.
const CONTAINER_ROWS: u16 = 3;

pub(crate) struct Hit {
    pub(crate) node: NodeId,
    pub(crate) area: Rect,
    pub(crate) target: Option<NodeId>,
    pub(crate) origin: Point<Points>,
    pub(crate) node_origin: Point<Points>,
}

#[derive(Clone, Copy)]
pub(crate) struct Geometry {
    pub(crate) id: NodeId,
    pub(crate) bounds: CellBounds,
    pub(crate) visible: Rect,
    pub(crate) frame: Bounds<Points>,
    pub(crate) text_frame: Option<Bounds<Points>>,
    pub(crate) scroll_origin: Point<Points>,
}

/// How a frame is drawn: the size of a Cell, the app's appearance at the
/// terminal's depth (`page`) and the bands that show hover and press.
pub(crate) struct Look {
    pub(crate) cells: CellSize,
    pub(crate) page: Ground,
    pub(crate) bands: Bands,
}

/// Draws the tree into `area` of `buffer` in `look` (colors read against its
/// `page`, hover and press shown with its `bands`), and records what input
/// needs:
/// the views that take the pointer (`hits`, topmost last; none while a
/// system dialog is shown), every view's Cells (`geometry`) and the scroll
/// bars (`tree.scroll_bars`).
pub(crate) fn draw(
    tree: &mut Mounter,
    buffer: &mut Buffer,
    area: Rect,
    look: Look,
    hits: &mut Vec<Hit>,
    geometry: &mut Vec<Geometry>,
) {
    let Look { cells, page, bands } = look;
    hits.clear();
    geometry.clear();
    let mut bars = std::mem::take(&mut tree.scroll_bars);
    bars.clear();
    let clip = area.intersection(buffer.area);
    let mut text_cells = std::mem::take(&mut tree.text_cells);
    text_cells.clear();
    text_cells.resize(buffer.content.len(), false);
    let mut painter = Painter {
        buffer,
        tree,
        hits,
        geometry,
        bars,
        area,
        cells,
        bands,
        screen: clip,
        text_cells,
        focus: Focus::None,
        popup: false,
        pane: None,
        page: page.rgb,
    };
    painter.pane = focus_pane(painter.tree);
    let root = Layer::default();
    painter.view(ROOT_ID, Point::default(), clip, 1.0, page, root, true, None);
    let mut index = 0;
    while let Some(&modal) = painter.tree.modals.get(index) {
        index += 1;
        if crate::navigation::modal_visible(painter.tree, modal) {
            dialog::backdrop(painter.buffer, clip, page.ramp);
            // The page behind the backdrop holds no text the modal's own
            // can cut short: no `…` lands on the modal's frame where its
            // text starts beside a page's words.
            painter.text_cells.fill(false);
            let layer = Layer {
                overlay: true,
                modal: true,
                boxed: false,
                title: None,
                scrolled: false,
                disabled: false,
                pane: false,
                right_line: None,
            };
            painter.view(modal, Point::default(), clip, 1.0, page, layer, true, None);
        }
    }
    painter.tree.scroll_bars = painter.bars;
    painter.tree.text_cells = std::mem::take(&mut painter.text_cells);
    if painter.tree.dialogs.is_shown() {
        painter.hits.clear();
        painter.tree.dialogs.draw(painter.buffer, clip, page.ramp);
    }
}

/// Where a view is painted: inside an overlay (a modal or an anchored
/// popup), and whether its box is already framed.
#[derive(Clone, Default)]
struct Layer {
    /// In a modal or an anchored overlay: boxes clear what is behind them.
    overlay: bool,
    /// In a modal (else an anchored overlay, when `overlay`): its box is
    /// the terminal's background, an anchored one's a raised surface.
    modal: bool,
    /// An outer box of the overlay is drawn: inner ones get no frame.
    boxed: bool,
    /// The overlay's accessibility label, its frame's title.
    title: Option<std::sync::Arc<str>>,
    /// The clip comes from a scroll view's viewport: text it cuts scrolls
    /// into view, so it ends in no `…`.
    scrolled: bool,
    /// Inside a disabled view: drawn faint (dim) whatever the view's own
    /// frame, as its content may stand outside it.
    disabled: bool,
    /// Inside the innermost box with a line around the focused view (a
    /// pane): its selected rows are where the keys go, an accent band.
    pane: bool,
    /// The parent's right line, when it has one: the column right of its
    /// content (`inner.right`) and the parent's visible Cells. A scroll
    /// view that reaches it draws its bar on that line, as lazygit does.
    right_line: Option<(i32, Rect)>,
}

struct Painter<'a> {
    buffer: &'a mut Buffer,
    tree: &'a mut Mounter,
    hits: &'a mut Vec<Hit>,
    geometry: &'a mut Vec<Geometry>,
    bars: Vec<ScrollBar>,
    area: Rect,
    cells: CellSize,
    bands: Bands,
    /// The surface's Cells: what anchored overlays are clipped to.
    screen: Rect,
    /// Which Cells of the buffer hold text drawn this frame.
    text_cells: Vec<bool>,
    /// Where the focus is in the subtree being marked.
    focus: Focus,
    /// A popup (an anchored overlay's box) is drawn: only boxes inside an
    /// overlay show the focus in the accent.
    popup: bool,
    /// The innermost box with a line around the focused view (`Layer::pane`).
    pane: Option<NodeId>,
    /// The page's color: until a view fills the whole screen with the app's
    /// own page color, the terminal's background stands for it.
    page: palette::Rgb,
}

/// A view's borders: which sides draw and each one's style.
struct Edges {
    /// The sides that have a width: they own a line of Cells (top, right,
    /// bottom, left).
    widths: [bool; 4],
    /// The sides that draw.
    sides: [bool; 4],
    colors: [(Color, Modifier); 4],
    style: BorderStyle,
}

impl Edges {
    fn any(&self) -> bool {
        self.sides.iter().any(|&side| side)
    }
}

impl Painter<'_> {
    /// Draws view `id`, whose parent's content starts at `origin` (points
    /// from the area's top left), inside the `clip` Cells.
    #[allow(clippy::too_many_arguments)]
    fn view(
        &mut self,
        id: NodeId,
        origin: Point<Points>,
        clip: Rect,
        opacity: f32,
        ground: Ground,
        layer: Layer,
        pointer: bool,
        press: Option<(NodeId, Point<Points>)>,
    ) {
        let cells = self.cells;
        let area = self.area;
        let icon = self
            .tree
            .get(id)
            .and_then(|view| icon_label(self.tree, view))
            .map(str::to_owned);
        // Only an overlay's box takes a title, so only there is it asked
        // whether its label is already its text.
        let label_shown = self.tree.get(id).is_some_and(|view| {
            let boxes = view.props.overlay_anchor.is_some()
                || view.kind == ViewKind::Modal
                || (layer.overlay && !layer.boxed);
            boxes
                && view
                    .props
                    .accessibility
                    .label
                    .as_deref()
                    .is_some_and(|label| shown(self.tree, id, label))
        });
        let Some(view) = self
            .tree
            .views
            .get_mut(id as usize)
            .and_then(Option::as_mut)
        else {
            return;
        };
        let opacity = opacity
            * view
                .animated
                .opacity
                .unwrap_or(view.style.opacity)
                .clamp(0.0, 1.0);
        if view.concealed || view.style.hidden || opacity < INVISIBLE_OPACITY {
            return;
        }
        let at = origin + view.frame.origin + translation(view);
        let frame = Bounds::new(at, view.frame.size);
        // A box with nothing in it shows its scale (a progress fill's
        // `scaleX`): its Cells are the scaled box's, rounded to whole ones.
        // A box with children keeps its frame, so its content stays put.
        let bounds = if view.children.is_empty() {
            CellBounds::new(scaled(view, frame), cells, area)
        } else {
            CellBounds::new(frame, cells, area)
        };
        let visible = bounds.clipped(clip);
        let cell_origin = bounds.origin(cells, area);

        let pointer = pointer && view.props.pointer_events != PointerEvents::None;
        let press = if pointer
            && view.props.pointer_events != PointerEvents::BoxNone
            && !view.props.accessibility.state.disabled
            && nsh_core::input::has_press(view.events)
        {
            Some((id, cell_origin))
        } else {
            press
        };
        if pointer && view.props.pointer_events != PointerEvents::BoxNone && !visible.is_empty() {
            self.hits.push(Hit {
                node: id,
                area: visible,
                target: press.map(|p| p.0),
                origin: press.map_or(cell_origin, |p| p.1),
                node_origin: cell_origin,
            });
        }
        let children_pointer = pointer && view.props.pointer_events != PointerEvents::BoxOnly;

        // What a terminal cannot show (an image, a video, a web view)
        // keeps its box blank: its fill would read as a band of content.
        let blank = matches!(
            view.kind,
            ViewKind::Image | ViewKind::Video | ViewKind::WebView
        );
        let fill = if blank {
            nsh_core::Color::TRANSPARENT
        } else {
            let color = view
                .animated
                .background
                .unwrap_or(view.style.background_color);
            // A terminal draws one color per Cell: a gradient over no
            // fill of its own fills with its first stop.
            match view.style.effects.as_deref() {
                Some(effects) if color.0 & 255 == 0 => effects
                    .background_images
                    .first()
                    .and_then(|gradient| gradient.stops.first())
                    .map_or(color, |stop| stop.color),
                _ => color,
            }
        };
        let control = control(view);
        let emphasis = if control {
            stands_out(fill, ground)
        } else {
            None
        };
        let parent = ground;
        let mut ground = ground.under(fill);
        // On a terminal with surfaces a fill is a step of its ramp: raised
        // or sunken from the parent's, or an accent tinted into it.
        // The app's page color (an opaque fill over the whole screen with
        // nothing filled above it) is the terminal's background: never a
        // band, and the ground every step below it is taken from.
        let page = parent.rgb == self.page
            && parent.level == 0
            && !layer.overlay
            && fill.0 & 255 == 255
            && visible == self.screen;
        let filled = match parent.ramp {
            Some(_) if !page => palette::surface(fill, parent),
            _ => Surface::None,
        };
        if let Some(ramp) = parent.ramp {
            ground.level = palette::step(filled, parent.level);
            ground.surface = match filled {
                Surface::None => parent.surface,
                Surface::Accent(rgb) if !control => {
                    ramp.tint_alpha(rgb, parent.level, fill.0 as u8)
                }
                Surface::Accent(rgb) => ramp.tint(rgb, parent.level),
                Surface::Raised | Surface::Sunken => ramp.level(ground.level),
            };
        }
        // A view in the image role is a picture, which keeps its colors:
        // its fill is its own color at the terminal's depth, not a step of
        // the ramp (a color picker's swatches and track Cells).
        let swatch = (view.props.accessibility.role == AccessibilityRole::Image)
            .then(|| palette::swatch(fill, parent))
            .flatten();
        if filled != Surface::None {
            ground.swatch = false;
        }
        if let Some((rgb, color)) = swatch {
            ground.rgb = rgb;
            ground.level = parent.level;
            ground.surface = color;
            ground.swatch = true;
        }
        // The layout decides where a border is: the core sends a width
        // only for the sides it gave a Cell (a card's), none on controls,
        // boxes of text or small boxes. A filled surface's band is its
        // edge: its line shows only where it stands out far more.
        let mut edges = edges(view, parent);
        // The same rule as the terminal layout's: the first side with a
        // width gives the border's color.
        let first_border = (0..4)
            .find(|&side| edges.widths[side])
            .map(|side| view.style.borders.colors[side]);
        if filled != Surface::None
            && let Some(color) = first_border
            && palette::gap(color, parent) < 2.0 * palette::gap(fill, parent)
            && !palette::hued(color)
        {
            edges.sides = [false; 4];
        }
        // A one-row text field without a line or a fill of its own is a
        // raised band, as terminal search fields are.
        if let Some(ramp) = parent.ramp
            && view.input.is_some()
            && filled == Surface::None
            && !edges.any()
        {
            ground.level = Ramp::above(parent.level, palette::RAISED);
            ground.surface = ramp.level(ground.level);
        }
        let [top, right, bottom, left] = edges.widths.map(i32::from);
        let inner = CellBounds {
            left: bounds.left + left,
            top: bounds.top + top,
            right: (bounds.right - right).max(bounds.left + left),
            bottom: (bounds.bottom - bottom).max(bounds.top + top),
        };
        let text_bounds = view.text.as_ref().map(|(_, content)| {
            CellBounds::new(Bounds::new(at + content.origin, content.size), cells, area)
                .fitted(inner)
        });
        let scroll_origin = view
            .scroll
            .as_ref()
            .map_or_else(Point::default, |scroll| scroll.origin());
        self.geometry.push(Geometry {
            id,
            bounds,
            visible,
            frame: Bounds::new(cell_origin, frame.size),
            text_frame: text_bounds.map(|text| text.points(cells, area)),
            scroll_origin,
        });
        let inner_visible = inner.clipped(clip);
        // A view that clips, scrolls or stands framed holds its children
        // inside its content box; otherwise a child placed outside it (a
        // badge, an absolute label below a row) shows, as in CSS. Every
        // view's own text stays inside its own frame either way.
        let holds = view.style.overflow_hidden
            || edges.any()
            || filled != Surface::None
            || matches!(
                view.kind,
                ViewKind::Scroll | ViewKind::Stack | ViewKind::Screen | ViewKind::Modal
            );
        let children_clip = if holds { inner_visible } else { clip };
        let radii = view.style.radii().fit(frame.size);

        // An overlay's boxes stand on their own over what is behind them.
        let mut layer = layer;
        if view.props.overlay_anchor.is_some() || view.kind == ViewKind::Modal {
            layer.overlay = true;
            layer.modal = view.kind == ViewKind::Modal;
            layer.boxed = false;
            // A label the overlay already shows as its text (a toast's,
            // made of its lines) is no title: the frame would repeat it.
            layer.title = view
                .props
                .accessibility
                .label
                .clone()
                .filter(|_| !label_shown);
        }
        let surface = edges.any() || fill.0 & 255 == 255 || filled != Surface::None;
        let mut framed = None;
        let mut frame_look = None;
        // The overlay's box: whether it is a popup's (a drop shadow, a
        // brighter line) and where its title is (an accent chip while the
        // focus is inside).
        let mut popup_box = false;
        let mut title_at = None;
        if layer.overlay && surface && !visible.is_empty() {
            dialog::clear(self.buffer, visible);
            // A word behind it that the box cuts at its sides (or the
            // frame drawn a Cell around it) is left out whole: no half of
            // it is glued to the frame.
            let area = self.buffer.area;
            let (left, right) = (i32::from(visible.x), i32::from(visible.right()));
            let starts = [(left - 1, -1), (left - 2, -1), (right, 1), (right + 1, 1)];
            let rows =
                visible.y.saturating_sub(1)..visible.bottom().saturating_add(1).min(area.bottom());
            for y in rows {
                for (start, step) in starts {
                    let mut column = start;
                    while column >= i32::from(area.x) && column < i32::from(area.right()) {
                        let at = self.buffer.index_of(column as u16, y);
                        let word = self.text_cells[at]
                            && self.buffer.content[at]
                                .symbol()
                                .chars()
                                .any(|c| !c.is_whitespace());
                        if !word {
                            break;
                        }
                        self.buffer.content[at].set_symbol(" ");
                        self.text_cells[at] = false;
                        column += step;
                    }
                }
            }
            // What was behind holds no text any more, under the box or its
            // frame a Cell around it: text drawn next to it is not cut
            // short by it (no `…` on the frame).
            let ring = Rect::new(
                visible.x.saturating_sub(1),
                visible.y.saturating_sub(1),
                visible.width + 2,
                visible.height + 2,
            )
            .intersection(area);
            for y in ring.y..ring.bottom() {
                for x in ring.x..ring.right() {
                    let at = self.buffer.index_of(x, y);
                    self.text_cells[at] = false;
                }
            }
            if !layer.boxed {
                layer.boxed = true;
                // A modal's box is the terminal's background, an anchored
                // popup's a raised surface; each framed in the border color.
                if let Some(ramp) = parent.ramp {
                    ground.level = if layer.modal {
                        0
                    } else {
                        Ramp::above(0, palette::RAISED)
                    };
                    ground.surface = ramp.level(ground.level);
                    let line = if layer.modal {
                        ramp.border()
                    } else {
                        ramp.popover_line()
                    };
                    frame_look = Some((line, ground.surface));
                    popup_box = !layer.modal;
                    if popup_box {
                        self.popup = true;
                    }
                }
                let title = view
                    .props
                    .accessibility
                    .label
                    .clone()
                    .filter(|_| !label_shown)
                    .or_else(|| layer.title.take())
                    .unwrap_or_default();
                if edges.any() {
                    framed = Some(title);
                } else {
                    title_at = frame_around(self.buffer, visible, clip, &title, frame_look);
                }
            }
        }

        if !visible.is_empty() {
            // An opaque fill covers what was drawn under it before: a card
            // over a canvas's dots shows the card, not the dots through it.
            if fill.0 & 255 == 255 && !page {
                clear_symbols(self.buffer, visible);
                for y in visible.y..visible.bottom() {
                    for x in visible.x..visible.right() {
                        let at = self.buffer.index_of(x, y);
                        self.text_cells[at] = false;
                    }
                }
            }
            if parent.ramp.is_some() && ground.surface != parent.surface {
                self::fill(self.buffer, visible, ground.surface);
            }
            if let Some(centre) = gradient(self.buffer, view, bounds, visible, cells, parent) {
                ground.rgb = centre;
                ground.surface = palette::convert(centre, parent.depth);
                ground.swatch = true;
            }
            if let Some((line, _)) = frame_look {
                edges.colors = [(line, Modifier::empty()); 4];
            }
            lines(self.buffer, bounds, visible, &edges, radii);
            // Where the terminal has surfaces the title is set off by the
            // line on both sides: `╭─ Title ─`.
            let offset = 1 + u16::from(frame_look.is_some());
            if let Some(title) = framed.filter(|title| !title.is_empty())
                && edges.sides[0]
                && visible.y == u16::try_from(bounds.top).unwrap_or(u16::MAX)
                && visible.width > 4 + 2 * offset
            {
                let drawn = dialog::put(
                    self.buffer,
                    visible.x + offset,
                    visible.y,
                    &format!(" {title} "),
                    visible.width - 2 * offset,
                    Color::Reset,
                    Modifier::BOLD,
                );
                title_at = Some(Rect::new(visible.x + offset, visible.y, drawn, 1));
            }
            if let Some((layout, _)) = &view.text
                && let Some(layout) = layout.downcast::<CellLayout>()
                && let Some(text_bounds) = text_bounds
            {
                let place = TextPlace {
                    layout,
                    bounds: text_bounds,
                    visible,
                    scrolled: layer.scrolled,
                };
                text(self.buffer, &mut self.text_cells, place, view, ground);
            }
            if let (Some(field), Some(config)) = (&mut view.field, &view.input) {
                let content = CellBounds::new(
                    Bounds::new(at + field.content.origin, field.content.size),
                    cells,
                    area,
                )
                .fitted(inner);
                field.draw(
                    config,
                    content,
                    inner_visible,
                    self.buffer,
                    view.animated.color,
                    ground,
                );
            }
            if let Some(canvas) = &view.canvas
                && !inner_visible.is_empty()
            {
                crate::canvas::draw(
                    canvas,
                    self.buffer,
                    &self.text_cells,
                    bounds,
                    inner_visible,
                    cells,
                    ground,
                );
            }
            if let Some(component) = &mut view.component {
                component.render(inner_visible, self.buffer);
            }
            if control && let Some(label) = &icon {
                let (x, y) = (inner_visible.x, inner_visible.y);
                let room = (inner.right - inner.left).max(0);
                // An icon-only popup trigger (a split button's chevron, a
                // menu's) shows which way its popup opens, set apart from
                // a label beside it, and an icon-only disclosure whether
                // it is open; its name stays its accessible name.
                let state = view.props.accessibility.state;
                let label = match (state.expanded, state.has_popup) {
                    (Some(true), true) => "▴",
                    (Some(false), true) | (Some(true), false) => "▾",
                    (Some(false), false) => "▸",
                    (None, _) => label.as_str(),
                };
                if i32::from(x) == inner.left && i32::from(y) == inner.top {
                    let drawn = dialog::put(
                        self.buffer,
                        x,
                        y,
                        label,
                        inner_visible.width,
                        Color::Reset,
                        Modifier::empty(),
                    );
                    // A row short of room shrinks an icon button below its
                    // label: the label is cut, its last Cell `…`.
                    if drawn > 0 && unicode_width::UnicodeWidthStr::width(label) > room as usize {
                        self.buffer[(x + drawn - 1, y)].set_symbol("…");
                    }
                    let width = usize::from(self.buffer.area.width);
                    let start = usize::from(y) * width + usize::from(x);
                    for cell in &mut self.text_cells[start..start + usize::from(drawn)] {
                        *cell = true;
                    }
                }
            }
        }

        let sticky: Vec<(NodeId, Point<Points>)> = view
            .scroll
            .as_ref()
            .filter(|scroll| !scroll.sticky.is_empty())
            .map(|scroll| scroll.sticky_shifts().collect())
            .unwrap_or_default();
        let content_origin = at - scroll_origin;
        let outer_focus = std::mem::take(&mut self.focus);
        let own_disabled =
            view.props.accessibility.state.disabled || view.states & state::DISABLED != 0;
        let mut child_layer = layer.clone();
        child_layer.disabled |= own_disabled;
        if view.kind == ViewKind::Scroll {
            child_layer.scrolled = true;
        } else if holds {
            child_layer.scrolled = false;
        }
        if self.pane == Some(id) {
            child_layer.pane = true;
        }
        child_layer.right_line = edges.sides[1].then_some((inner.right, visible));
        let mut index = 0;
        while let Some(child) = self
            .tree
            .get(id)
            .and_then(|view| view.children.get(index).copied())
        {
            index += 1;
            if self
                .tree
                .get(child)
                .is_some_and(|view| view.kind == ViewKind::Modal)
            {
                continue;
            }
            // An anchored overlay stands over everything, wherever its
            // owner is.
            let child_clip = if self
                .tree
                .get(child)
                .is_some_and(|view| view.props.overlay_anchor.is_some())
            {
                self.screen
            } else {
                children_clip
            };
            let shift = sticky
                .iter()
                .find(|(cell, _)| *cell == child)
                .map_or_else(Point::default, |(_, shift)| *shift);
            self.view(
                child,
                content_origin + shift,
                child_clip,
                opacity,
                ground,
                child_layer.clone(),
                children_pointer,
                press,
            );
        }

        let Some(view) = self.tree.get(id) else {
            return;
        };
        // A scroll view filling its parent's content up to a right line
        // draws its vertical bar on that line, taking no column of its own.
        let on_line = view
            .scroll
            .as_ref()
            .filter(|scroll| !scroll.horizontal && !edges.sides[1])
            .and(layer.right_line)
            .filter(|&(right, _)| right == bounds.right);
        let (bar_inner, bar_clip) = match on_line {
            Some((right, parent)) => {
                let widened = Rect {
                    width: inner_visible.width + u16::from(!inner_visible.is_empty()),
                    ..inner_visible
                };
                (
                    CellBounds {
                        right: right + 1,
                        ..inner
                    },
                    widened.intersection(parent),
                )
            }
            None => (inner, inner_visible),
        };
        if let Some(scroll) = &view.scroll
            && let Some(bar) =
                crate::scrollbar::bar(id, scroll, view.frame.size, bar_inner, bar_clip)
            && (on_line.is_some() || !covers_content(self.buffer, bar.track))
        {
            scroll_bar(self.buffer, bar, ground.ramp);
            self.bars.push(bar);
        }
        let popup_item = layer.overlay && !layer.modal;
        // Content of a disabled view drawn outside its frame is disabled too.
        if layer.disabled && !visible.is_empty() {
            match ground.ramp {
                Some(ramp) => drawn(self.buffer, visible, |cell| {
                    cell.fg = ramp.faint();
                    cell.modifier -= Modifier::BOLD;
                }),
                None => {
                    for y in visible.y..visible.bottom() {
                        for x in visible.x..visible.right() {
                            let cell = &mut self.buffer[(x, y)];
                            if cell.symbol() != " " {
                                cell.modifier = (cell.modifier | Modifier::DIM) - Modifier::BOLD;
                            }
                        }
                    }
                }
            }
        }
        // A disclosure (it expands in place, no popup) says which way it
        // goes before its label, as the terminal's own disclosures do.
        if let Some(open) = disclosure(view)
            && !inner_visible.is_empty()
        {
            let focused = view.states & state::FOCUSED != 0;
            let look = match (focused, ground.ramp) {
                (true, _) => (ACCENT, Modifier::empty()),
                (false, Some(ramp)) => (ramp.muted(), Modifier::empty()),
                (false, None) => (Color::Reset, Modifier::DIM),
            };
            mark_disclosure(self.buffer, inner_visible, open, look);
        }
        if !visible.is_empty() {
            let marks = Marks {
                bands: self.bands,
                emphasis,
            };
            match ground.ramp {
                Some(ramp) => mark_surface(
                    self.buffer,
                    view,
                    bounds,
                    visible,
                    inner_visible,
                    &edges,
                    emphasis,
                    (ramp, ground.level, ground.surface),
                    popup_item,
                    layer.pane,
                ),
                None => mark_state(
                    self.buffer,
                    view,
                    bounds,
                    visible,
                    inner_visible,
                    &edges,
                    marks,
                ),
            }
        }
        // The innermost box with a line around the focus shows it in the
        // accent (only a popup's while one is open); a focused overlay's
        // title is an accent chip.
        if let Some(ramp) = ground.ramp {
            if self.focus == Focus::Within
                && edges.any()
                && (!self.popup || layer.overlay)
                && !visible.is_empty()
            {
                recolor_border(self.buffer, bounds, visible, ACCENT);
                self.focus = Focus::Shown;
            }
            let chip = self.focus != Focus::None && title_at.is_some();
            if frame_look.is_some() && self.focus == Focus::Within {
                // An overlay's own frame holds the focus: nothing outside
                // it shows it.
                self.focus = Focus::Shown;
            }
            if chip && let Some(title) = title_at {
                for x in title.x..title.right() {
                    let cell = &mut self.buffer[(x, title.y)];
                    cell.bg = ACCENT;
                    cell.fg = ramp.base();
                }
            }
            if popup_box {
                shadow(
                    self.buffer,
                    visible,
                    frame_look.is_some() && !edges.any(),
                    self.screen,
                    ramp,
                );
            }
        }
        if self.focus == Focus::None && view.states & state::FOCUSED != 0 {
            self.focus = Focus::Within;
        }
        self.focus = self.focus.max(outer_focus);
    }
}

/// The innermost view with a border line around the focused view (or the
/// overlay holding it): the pane the keys go to.
/// Whether every word of `label` is in the text drawn under `id`: an
/// overlay's label that only gathers its lines for a screen reader.
fn shown(tree: &Mounter, id: NodeId, label: &str) -> bool {
    fn gather(tree: &Mounter, id: NodeId, into: &mut String, depth: usize) {
        let Some(view) = tree.get(id) else { return };
        if let Some(content) = &view.text_content {
            into.push_str(&content.text);
            into.push(' ');
        }
        if depth < 32 {
            for &child in &view.children {
                gather(tree, child, into, depth + 1);
            }
        }
    }
    let mut drawn = String::new();
    gather(tree, id, &mut drawn, 0);
    let words: Vec<&str> = drawn.split_whitespace().collect();
    let mut label_words = label
        .split(|c: char| c.is_whitespace())
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .peekable();
    label_words.peek().is_some()
        && label_words.all(|word| {
            words
                .iter()
                .any(|drawn| drawn.trim_matches(|c: char| !c.is_alphanumeric()) == word)
        })
}

/// Whether `c` is a braille pattern: a canvas's or a chart's dots.
fn is_braille(c: char) -> bool {
    ('\u{2800}'..='\u{28FF}').contains(&c)
}

fn focus_pane(tree: &Mounter) -> Option<NodeId> {
    let focused = tree.views.iter().position(|view| {
        view.as_ref()
            .is_some_and(|view| view.states & state::FOCUSED != 0)
    })?;
    let mut at = tree.get(focused as NodeId)?.parent;
    while let Some(id) = at {
        let view = tree.get(id)?;
        let lined = view
            .style
            .borders
            .widths
            .to_array()
            .iter()
            .any(|&w| w > Points::ZERO);
        if lined || view.props.overlay_anchor.is_some() || view.kind == ViewKind::Modal {
            return Some(id);
        }
        at = view.parent;
    }
    None
}

/// The accent the focus shows in: a box's line around it, an overlay's
/// title, a popup's highlighted item.
const ACCENT: Color = Color::Blue;

/// Where the focus is, as the views around it are marked (their children
/// first): nowhere in the subtree, inside it with no box showing it yet,
/// or shown by an inner box's line.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
enum Focus {
    #[default]
    None,
    Within,
    Shown,
}

/// A popup's drop shadow, one Cell to the right of and below its box
/// (`visible`, framed one Cell outside it when `around`): the glyphs there
/// stay, darkened, after Turbo Vision.
fn shadow(buffer: &mut Buffer, visible: Rect, around: bool, screen: Rect, ramp: Ramp) {
    let grow = u16::from(around);
    let left = visible.x.saturating_sub(grow);
    let top = visible.y.saturating_sub(grow);
    let right = visible.right() + grow;
    let bottom = visible.bottom() + grow;
    let screen = screen.intersection(buffer.area);
    let mut shade = |x: u16, y: u16| {
        if screen.contains(Position::new(x, y)) {
            let cell = &mut buffer[(x, y)];
            cell.bg = ramp.shade(cell.bg);
            cell.fg = ramp.fade(cell.fg, true);
            cell.modifier -= Modifier::BOLD | Modifier::REVERSED;
        }
    };
    for y in top + 1..=bottom {
        shade(right, y);
    }
    for x in left + 1..right {
        shade(x, bottom);
    }
}

/// The label an icon button shows: its accessibility label, trimmed, when
/// all its content is left out (every child laid out empty or hidden), as
/// the layout then gives it a row as wide as that label.
fn icon_label<'a>(tree: &Mounter, view: &'a NshView) -> Option<&'a str> {
    if view.text.is_some() || view.field.is_some() || view.canvas.is_some() {
        return None;
    }
    let label = view.props.accessibility.label.as_deref()?.trim();
    let empty = !view.children.iter().any(|&child| draws(tree, child));
    (empty && !label.is_empty()).then_some(label)
}

/// Whether `id` or a view inside it draws anything of its own in the
/// terminal: a text, a field, a canvas or a native component (a wrapper of
/// an icon the terminal leaves out draws nothing).
fn draws(tree: &Mounter, id: NodeId) -> bool {
    tree.get(id).is_some_and(|view| {
        view.frame.size.width > Points::ZERO
            && view.frame.size.height > Points::ZERO
            && (view.text.is_some()
                || view.field.is_some()
                || view.canvas.is_some()
                || view.component.is_some()
                || view.children.iter().any(|&child| draws(tree, child)))
    })
}

/// The view's translation, which moves it by whole Cells; scales and
/// rotations are left out (glyphs stay on the grid).
fn translation(view: &NshView) -> Point<Points> {
    let mut offset = Point::default();
    for (index, step) in view.style.transform.ops().iter().enumerate() {
        let value = view
            .animated
            .transform
            .get(index)
            .copied()
            .flatten()
            .unwrap_or(step.value);
        match step.kind {
            TransformKind::TranslateX => offset.x += Points(value),
            TransformKind::TranslateY => offset.y += Points(value),
            _ => {}
        }
    }
    offset
}

/// `frame` scaled by the view's `scale` / `scaleX` / `scaleY` around its
/// transform origin; other steps of its transform are left out.
fn scaled(view: &NshView, frame: Bounds<Points>) -> Bounds<Points> {
    let (mut sx, mut sy) = (1.0f32, 1.0f32);
    for (index, step) in view.style.transform.ops().iter().enumerate() {
        let value = view
            .animated
            .transform
            .get(index)
            .copied()
            .flatten()
            .unwrap_or(step.value);
        match step.kind {
            TransformKind::Scale => {
                sx *= value;
                sy *= value;
            }
            TransformKind::ScaleX => sx *= value,
            TransformKind::ScaleY => sy *= value,
            _ => {}
        }
    }
    if (sx - 1.0).abs() < 1e-4 && (sy - 1.0).abs() < 1e-4 {
        return frame;
    }
    let (sx, sy) = (sx.max(0.0), sy.max(0.0));
    let size = frame.size;
    let (offset, _) = view.style.transform_origin.offset(size);
    // The origin, from the frame's top left.
    let ox = size.width / 2.0 + offset.x;
    let oy = size.height / 2.0 + offset.y;
    Bounds::new(
        frame.origin + Point::new(ox - ox * sx, oy - oy * sy),
        nsh_core::Size::new(size.width * sx, size.height * sy),
    )
}

/// The view's borders, read on `ground`: a neutral color is dim, an accent
/// stays the accent.
fn edges(view: &NshView, ground: Ground) -> Edges {
    let borders = &view.style.borders;
    let widths = borders.widths.to_array();
    let colors = view
        .animated
        .border_color
        .map_or(borders.colors, |color| [color; 4]);
    Edges {
        widths: std::array::from_fn(|side| widths[side] > Points::ZERO),
        sides: std::array::from_fn(|side| {
            widths[side] > Points::ZERO && !colors[side].is_transparent()
        }),
        colors: colors.map(|color| {
            // Lines are the ramp's border step; an accent stays its accent.
            if let Some(ramp) = ground.ramp {
                return match palette::tone(color, ground) {
                    Tone::Accent(accent) => (accent, Modifier::empty()),
                    _ => (ramp.border(), Modifier::empty()),
                };
            }
            let style = ink(color, ground);
            let fg = style.fg.unwrap_or(Color::Reset);
            let modifier = if fg == Color::Reset {
                Modifier::DIM
            } else {
                Modifier::empty()
            };
            (fg, modifier)
        }),
        style: borders.style,
    }
}

/// Draws the sides of `edges` on the edge Cells of `bounds` that are in
/// `visible`.
fn lines(
    buffer: &mut Buffer,
    bounds: CellBounds,
    visible: Rect,
    edges: &Edges,
    radii: CornerRadii,
) {
    if !edges.any() {
        return;
    }
    let [top, right, bottom, left] = edges.sides;
    let (horizontal, vertical) = match edges.style {
        BorderStyle::Solid => ("─", "│"),
        BorderStyle::Dashed => ("╌", "╎"),
        BorderStyle::Dotted => ("┈", "┊"),
    };
    let rounded = |radius: Points, round: &'static str, square: &'static str| {
        if radius > Points::ZERO { round } else { square }
    };
    for y in visible.y..visible.bottom() {
        let cy = i32::from(y);
        let on_top = top && cy == bounds.top;
        let on_bottom = bottom && cy == bounds.bottom - 1;
        for x in visible.x..visible.right() {
            let cx = i32::from(x);
            let on_left = left && cx == bounds.left;
            let on_right = right && cx == bounds.right - 1;
            let (symbol, side) = match (on_top, on_bottom, on_left, on_right) {
                (true, _, true, _) => (rounded(radii.top_left, "╭", "┌"), 0),
                (true, _, _, true) => (rounded(radii.top_right, "╮", "┐"), 0),
                (false, true, true, _) => (rounded(radii.bottom_left, "╰", "└"), 2),
                (false, true, _, true) => (rounded(radii.bottom_right, "╯", "┘"), 2),
                (true, _, _, _) => (horizontal, 0),
                (false, true, _, _) => (horizontal, 2),
                (false, false, true, _) => (vertical, 3),
                (false, false, false, true) => (vertical, 1),
                _ => continue,
            };
            let (fg, modifier) = edges.colors[side];
            let cell = &mut buffer[(x, y)];
            cell.set_symbol(symbol);
            cell.fg = fg;
            cell.modifier = modifier;
        }
    }
}

/// Where a text view's rows go: its `layout` at `bounds`, drawn inside
/// `visible`, and whether a scroll view moved it (`scrolled`).
struct TextPlace<'a> {
    layout: &'a CellLayout,
    bounds: CellBounds,
    visible: Rect,
    scrolled: bool,
}

/// Draws a text view's rows, as laid out, inside the place's `visible`,
/// colors read on `ground`.
fn text(
    buffer: &mut Buffer,
    text_cells: &mut [bool],
    place: TextPlace<'_>,
    view: &NshView,
    ground: Ground,
) {
    let TextPlace {
        layout,
        bounds,
        visible,
        scrolled,
    } = place;
    let visible = bounds.clipped(visible);
    let single = layout.rows.len() == 1;
    // The end of the last row drawn: where a text cut below ends in `…`.
    let mut last_row = None;
    for (row_index, row) in layout.rows.iter().enumerate() {
        let y = bounds.top + row_index as i32;
        if y < i32::from(visible.y) {
            continue;
        }
        if y >= i32::from(visible.bottom()) {
            break;
        }
        let y = y as u16;
        // A row of text stands on its own over a drawing behind it (a
        // page's canvas backdrop): the braille dots across its box are
        // cleared with a Cell each side, as a canvas's own text clears
        // them, so a word never touches a dot.
        if !row.glyphs.is_empty() {
            let area = buffer.area;
            let from = visible.x.saturating_sub(1).max(area.x);
            let to = visible.right().saturating_add(1).min(area.right());
            for column in from..to {
                let cell = &mut buffer[(column, y)];
                if cell.symbol().chars().next().is_some_and(is_braille) {
                    cell.set_symbol(" ");
                }
            }
        }
        let mut x = bounds.left + row.indent as i32;
        let mut cut = None;
        let mut first = true;
        for glyph in &row.glyphs {
            let width = glyph.width as i32;
            let (start, end) = (x, x + width);
            x = end;
            if end > i32::from(visible.right()) && !glyph.text.trim().is_empty() {
                cut = Some(glyph);
            }
            if glyph.is_attachment()
                || width == 0
                || start < i32::from(visible.x)
                || end > i32::from(visible.right())
            {
                continue;
            }
            let (mut fg, mut modifier) = match view.animated.color.or(glyph.color()) {
                Some(color) => {
                    let style = ink(color, ground);
                    (style.fg.unwrap_or(Color::Reset), style.add_modifier)
                }
                None if ground.swatch => (palette::swatch_text(ground), Modifier::empty()),
                None => (Color::Reset, Modifier::empty()),
            };
            modifier |= glyph.modifiers();
            let mut bg = None;
            // A run on its own background is a band: a step of the ramp, an
            // accent tinted into it, or reverse video without surfaces.
            if let Some(background) = glyph.background()
                && let Some(ramp) = ground.ramp
            {
                bg = match palette::surface(background, ground) {
                    Surface::None => None,
                    Surface::Accent(rgb) => {
                        if let Tone::Accent(accent) = palette::tone(background, ground) {
                            fg = accent;
                        }
                        Some(ramp.tint(rgb, ground.level))
                    }
                    kind => Some(ramp.level(palette::step(kind, ground.level))),
                };
            } else if let Some(background) = glyph.background()
                && let Some(accent) = ink(background, ground).fg
                && accent != Color::Reset
                && !background.is_transparent()
            {
                fg = accent;
                modifier = (modifier | Modifier::REVERSED) - Modifier::DIM;
            }
            // Text drawn over an earlier text's Cells wins them whole; the
            // earlier one reads as cut short rather than mixed in. Only
            // words read cut: a run of glyphs (a slider's track under its
            // thumb) is only covered.
            if std::mem::take(&mut first) && !glyph.text.trim().is_empty() {
                let at = buffer.index_of(start as u16, y);
                if text_cells[at] && start > i32::from(buffer.area.x) {
                    let before = buffer.index_of(start as u16 - 1, y);
                    if text_cells[before]
                        && buffer.content[before]
                            .symbol()
                            .chars()
                            .any(char::is_alphanumeric)
                    {
                        buffer.content[before].set_symbol("…");
                    }
                }
            }
            for (offset, cx) in (start..end).enumerate() {
                text_cells[buffer.index_of(cx as u16, y)] = true;
                let cell = &mut buffer[(cx as u16, y)];
                cell.set_symbol(if offset == 0 { &glyph.text } else { " " });
                cell.fg = fg;
                cell.modifier = modifier;
                if let Some(bg) = bg {
                    cell.bg = bg;
                }
            }
        }
        let words = row
            .glyphs
            .iter()
            .any(|glyph| glyph.text.chars().any(char::is_alphanumeric));
        let drawn_end = x.min(i32::from(visible.right()));
        if words {
            let row_start = bounds.left + row.indent as i32;
            clear_dots(buffer, text_cells, row_start - 1, y);
            clear_dots(buffer, text_cells, drawn_end, y);
        }
        last_row = Some((y, drawn_end, row_index, words));
        // A line cut at the box's right edge ends in an ellipsis; one a
        // scroll view's viewport cuts is only cut (it scrolls into view),
        // and a run of glyphs only (a slider's track) is never words cut.
        if single
            && words
            && cut.is_some()
            && !visible.is_empty()
            && i32::from(visible.right()) <= bounds.right
            && (!scrolled || i32::from(visible.right()) == bounds.right)
        {
            let last = visible.right() - 1;
            buffer[(last, y)].set_symbol("…");
        }
    }
    // Rows past the bottom of a box that does not scroll: the last one
    // shown ends in `…`.
    if let Some((y, end, index, true)) = last_row
        && !scrolled
        && index + 1 < layout.rows.len()
        && !visible.is_empty()
    {
        let x = if end < i32::from(visible.right()) {
            end
        } else {
            i32::from(visible.right()) - 1
        };
        if x >= i32::from(visible.x) {
            buffer[(x as u16, y)].set_symbol("…");
            text_cells[buffer.index_of(x as u16, y)] = true;
        }
    }
}

/// Frames an overlay's box `rect` one Cell outside it, rounded and dim,
/// `title` on its top line, clearing the Cells under the frame; a side
/// past `clip` (the box at the screen's edge) is left out, never drawn
/// over the box.
///
/// With `look` (the terminal's surfaces: the line's color and the box's
/// background) the frame is drawn in that color on that background and
/// the title is set off by the line: `╭─ Title ─`.
fn frame_around(
    buffer: &mut Buffer,
    rect: Rect,
    clip: Rect,
    title: &str,
    look: Option<(Color, Color)>,
) -> Option<Rect> {
    let (left, top) = (i32::from(rect.x) - 1, i32::from(rect.y) - 1);
    let (right, bottom) = (i32::from(rect.right()), i32::from(rect.bottom()));
    let clip = clip.intersection(buffer.area);
    let inside = |x: i32, y: i32| {
        x >= i32::from(clip.x)
            && x < i32::from(clip.right())
            && y >= i32::from(clip.y)
            && y < i32::from(clip.bottom())
    };
    let mut put = |x: i32, y: i32, symbol: &str| {
        if inside(x, y) {
            // `inside` keeps both within the buffer's u16 area.
            let cell = &mut buffer[(x as u16, y as u16)];
            cell.reset();
            cell.set_symbol(symbol);
            match look {
                Some((line, background)) => {
                    cell.fg = line;
                    cell.bg = background;
                }
                None => cell.modifier = Modifier::DIM,
            }
        }
    };
    for x in left..=right {
        let (first, last) = (x == left, x == right);
        put(
            x,
            top,
            if first {
                "╭"
            } else if last {
                "╮"
            } else {
                "─"
            },
        );
        put(
            x,
            bottom,
            if first {
                "╰"
            } else if last {
                "╯"
            } else {
                "─"
            },
        );
    }
    for y in rect.y..rect.bottom() {
        put(left, i32::from(y), "│");
        put(right, i32::from(y), "│");
    }
    let x = rect.x + u16::from(look.is_some());
    if !title.is_empty() && rect.width > 6 && inside(i32::from(x), top) {
        let drawn = dialog::put(
            buffer,
            x,
            top as u16,
            &format!(" {title} "),
            rect.width.min(clip.right().saturating_sub(x)),
            Color::Reset,
            Modifier::BOLD,
        );
        if let Some((_, background)) = look {
            for at in x..x + drawn {
                buffer[(at, top as u16)].bg = background;
            }
        }
        return Some(Rect::new(x, top as u16, drawn, 1));
    }
    None
}

/// Whether content drew a glyph where a scroll bar's `track` would go: the
/// layout keeps no column (or row) for the bar, so it then draws none
/// rather than cover what scrolls.
fn covers_content(buffer: &Buffer, track: Rect) -> bool {
    let track = track.intersection(buffer.area);
    (track.y..track.bottom())
        .any(|y| (track.x..track.right()).any(|x| buffer[(x, y)].symbol() != " "))
}

/// A track with its thumb: dim and the default color, or the ramp's border
/// and muted steps where the terminal has surfaces.
fn scroll_bar(buffer: &mut Buffer, bar: ScrollBar, ramp: Option<Ramp>) {
    let (track, thumb) = if bar.vertical {
        ("│", "┃")
    } else {
        ("─", "━")
    };
    let rect = bar.track;
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let on_thumb = bar.thumb.contains(Position::new(x, y));
            let cell = &mut buffer[(x, y)];
            // The surface under the bar stays.
            let bg = cell.bg;
            cell.reset();
            if ramp.is_some() {
                cell.bg = bg;
            }
            match (on_thumb, ramp) {
                (true, ramp) => {
                    cell.set_symbol(thumb);
                    if let Some(ramp) = ramp {
                        cell.fg = ramp.muted();
                    }
                }
                (false, Some(ramp)) => {
                    cell.set_symbol(track);
                    cell.fg = ramp.border();
                }
                (false, None) => {
                    cell.set_symbol(track);
                    cell.modifier = Modifier::DIM;
                }
            }
        }
    }
}

/// Whether the view shows as a selected or current item: a band over its
/// Cells. Checkboxes, radios and switches show their state themselves.
fn banded(view: &NshView) -> bool {
    let accessibility = &view.props.accessibility;
    let state = &accessibility.state;
    match accessibility.role {
        AccessibilityRole::Checkbox | AccessibilityRole::Radio | AccessibilityRole::Switch => false,
        AccessibilityRole::MenuItemCheckbox | AccessibilityRole::MenuItemRadio => {
            state.selected || state.checked == Some(Checked::True)
        }
        _ => state.selected || state.current.is_some() || state.pressed == Some(Checked::True),
    }
}

/// Whether a press or hover on the view acts on it.
fn pressable(view: &NshView) -> bool {
    matches!(
        view.props.accessibility.role,
        AccessibilityRole::Button
            | AccessibilityRole::ImageButton
            | AccessibilityRole::Link
            | AccessibilityRole::Menuitem
            | AccessibilityRole::MenuItemCheckbox
            | AccessibilityRole::MenuItemRadio
            | AccessibilityRole::Option
            | AccessibilityRole::Tab
    ) || nsh_core::input::has_press(view.events)
}

/// Whether the view is a control: something the user presses, chooses or
/// types in, drawn as its label row.
fn control(view: &NshView) -> bool {
    view.input.is_some()
        || pressable(view)
        || matches!(
            view.props.accessibility.role,
            AccessibilityRole::Checkbox
                | AccessibilityRole::Radio
                | AccessibilityRole::Switch
                | AccessibilityRole::Adjustable
                | AccessibilityRole::Combobox
                | AccessibilityRole::Spinbutton
        )
}

/// What state marks draw with besides modifiers.
#[derive(Clone, Copy)]
struct Marks {
    /// The hover and press bands.
    bands: Bands,
    /// The color of a control whose fill stands out (a primary button):
    /// its accent, or the default foreground.
    emphasis: Option<Color>,
}

/// Draws the view's state over its drawn Cells (its subtree's included):
/// selection, focus, press, hover, then disabled.
fn mark_state(
    buffer: &mut Buffer,
    view: &NshView,
    bounds: CellBounds,
    visible: Rect,
    inner: Rect,
    edges: &Edges,
    marks: Marks,
) {
    let flags = view.states;
    let disabled = view.props.accessibility.state.disabled || flags & state::DISABLED != 0;
    let selected = banded(view);
    let field = view.input.is_some();
    let focused = flags & state::FOCUSED != 0
        && !disabled
        && view.props.accessibility.role != AccessibilityRole::Adjustable;
    let pressed = flags & state::PRESSED != 0 && !disabled;
    let hovered = flags & state::HOVERED != 0 && !disabled && pressable(view);
    // A disabled view's own content (its text, when the view is the text)
    // is dim; what it holds was dimmed as it was drawn (`Layer::disabled`).
    if disabled {
        for y in visible.y..visible.bottom() {
            for x in visible.x..visible.right() {
                let cell = &mut buffer[(x, y)];
                if cell.symbol() != " " {
                    cell.modifier =
                        (cell.modifier | Modifier::DIM) - Modifier::BOLD - Modifier::REVERSED;
                }
            }
        }
        return;
    }
    if let Some(accent) = marks.emphasis {
        for y in inner.y..inner.bottom() {
            for x in inner.x..inner.right() {
                let cell = &mut buffer[(x, y)];
                if cell.symbol() != " " {
                    if accent != Color::Reset {
                        cell.fg = accent;
                    }
                    cell.modifier = (cell.modifier | Modifier::BOLD) - Modifier::DIM;
                }
            }
        }
    }
    // A tab the app underlines keeps its underline alone, as with colors.
    let tab = view.props.accessibility.role == AccessibilityRole::Tab;
    if selected && !inner.is_empty() && !(tab && underlined(buffer, inner)) {
        for y in inner.y..inner.bottom() {
            for x in inner.x..inner.right() {
                buffer[(x, y)].modifier.insert(Modifier::REVERSED);
            }
        }
    }
    if focused && field {
        label(buffer, inner, Modifier::UNDERLINED);
    } else if focused {
        if selected {
            label(buffer, inner, Modifier::BOLD | Modifier::UNDERLINED);
        } else if inner.height >= CONTAINER_ROWS && edges.any() {
            emphasize_border(buffer, bounds, visible);
        } else if inner.height >= CONTAINER_ROWS {
            label(buffer, inner, Modifier::BOLD | Modifier::UNDERLINED);
        } else {
            label(buffer, inner, Modifier::REVERSED);
        }
    } else if pressed && !selected {
        match marks.bands.press {
            Some(band) => fill(buffer, visible, band),
            None => label(buffer, inner, Modifier::REVERSED),
        }
    } else if hovered
        && !selected
        && let Some(band) = marks.bands.hover
    {
        fill(buffer, visible, band);
    }
    if disabled {
        for y in visible.y..visible.bottom() {
            for x in visible.x..visible.right() {
                let cell = &mut buffer[(x, y)];
                if cell.symbol() != " " {
                    cell.modifier = (cell.modifier | Modifier::DIM) - Modifier::BOLD;
                }
            }
        }
    }
}

/// Draws the view's state on a terminal with surfaces (`ramp`, and the
/// view's own step and background): bands of the ramp and bold, after
/// grok-build's look. Selected and focused items take the select band,
/// hovered ones the hover band, pressed ones the select band; a focused
/// field brightens its line (or sinks); tabs are bold when chosen and on
/// the select band unless the app underlines them, muted otherwise;
/// disabled views are faint, with no band.
#[allow(clippy::too_many_arguments)]
fn mark_surface(
    buffer: &mut Buffer,
    view: &NshView,
    bounds: CellBounds,
    visible: Rect,
    inner: Rect,
    edges: &Edges,
    emphasis: Option<Color>,
    (ramp, level, own): (Ramp, usize, Color),
    popup: bool,
    pane: bool,
) {
    let flags = view.states;
    let disabled = view.props.accessibility.state.disabled || flags & state::DISABLED != 0;
    let selected = banded(view);
    let field = view.input.is_some();
    let tab = view.props.accessibility.role == AccessibilityRole::Tab;
    let focused = flags & state::FOCUSED != 0 && !disabled;
    let pressed = flags & state::PRESSED != 0 && !disabled;
    let hovered = flags & state::HOVERED != 0 && !disabled && pressable(view);
    let step = |target: usize| ramp.level(Ramp::above(level, target));
    if let Some(accent) = emphasis {
        drawn(buffer, inner, |cell| {
            if accent != Color::Reset {
                cell.fg = accent;
            }
            cell.modifier.insert(Modifier::BOLD);
        });
    }
    if disabled {
        drawn(buffer, visible, |cell| {
            cell.fg = ramp.faint();
            cell.modifier -= Modifier::BOLD;
        });
        return;
    }
    if field {
        if focused {
            if edges.any() {
                recolor_border(buffer, bounds, visible, ramp.focus_line());
            } else {
                band(buffer, visible, step(HOVER), own);
            }
        }
        return;
    }
    // The highlighted item of a menu, list or palette in a popup (the
    // keyboard's or the pointer's) is an accent band; its text, shortcuts
    // included, is in the terminal's background color.
    // A focusable view showing focus its own way (a resize handle's line,
    // a slider's track) takes no marks for focus.
    let adjustable = view.props.accessibility.role == AccessibilityRole::Adjustable;
    let focused = focused && !adjustable;
    // The same for the selected row of a list in the pane the keys go to
    // (lazygit's): elsewhere selection is the gray select band.
    let active = (popup && (focused || hovered || pressed) && pressable(view))
        || (pane && !popup && selected && !tab);
    if active {
        band(buffer, visible, ACCENT, own);
        drawn(buffer, visible, |cell| {
            cell.fg = ramp.base();
            cell.modifier -= Modifier::BOLD;
        });
        if selected {
            drawn(buffer, inner, |cell| cell.modifier.insert(Modifier::BOLD));
        }
        return;
    }
    // A trigger whose popup or menu is open stands raised.
    if !popup
        && pressable(view)
        && view.props.accessibility.state.has_popup
        && view.props.accessibility.state.expanded == Some(true)
    {
        band(buffer, visible, step(palette::RAISED), own);
    }
    if tab {
        // The selected tab of a segmented list stands on the select band;
        // one whose app marks it with an underline (an underlined tab
        // strip) keeps that mark alone.
        if selected && !underlined(buffer, inner) {
            band(buffer, visible, step(SELECT), own);
        }
        if selected || focused {
            drawn(buffer, inner, |cell| cell.modifier.insert(Modifier::BOLD));
        } else {
            drawn(buffer, inner, |cell| {
                if cell.fg == Color::Reset {
                    cell.fg = ramp.muted();
                }
            });
        }
        if focused || pressed {
            band(buffer, visible, step(SELECT), own);
        } else if hovered {
            band(buffer, visible, step(HOVER), own);
        }
        return;
    }
    let container = inner.height >= CONTAINER_ROWS;
    if selected {
        band(buffer, visible, step(SELECT), own);
        drawn(buffer, inner, |cell| cell.modifier.insert(Modifier::BOLD));
    }
    if focused && container {
        if edges.any() {
            recolor_border(buffer, bounds, visible, ramp.focus_line());
        } else if !selected {
            band(buffer, visible, step(HOVER), own);
        }
    } else if focused {
        band(buffer, visible, step(SELECT), own);
        drawn(buffer, inner, |cell| cell.modifier.insert(Modifier::BOLD));
    } else if pressed && !selected {
        band(buffer, visible, step(SELECT), own);
    } else if hovered && !selected {
        band(buffer, visible, step(HOVER), own);
    }
}

/// Whether `view` is a disclosure, and open: a pressable view whose
/// `expanded` is set that opens no popup (a collapsible's or an
/// accordion's trigger).
fn disclosure(view: &NshView) -> Option<bool> {
    let state = view.props.accessibility.state;
    state
        .expanded
        .filter(|_| !state.has_popup && pressable(view))
}

/// The glyphs a disclosure starts with: a trigger that draws one of its
/// own (`nsh-base`'s accordion and tree rows) gets no second.
const DISCLOSURE_GLYPHS: [&str; 6] = ["▸", "▾", "▴", "▶", "▼", "▲"];

/// Marks a disclosure: `▾` open, `▸` closed, in `look`, two Cells before
/// the first glyph of its first drawn row (one when only one is free), in
/// `rect`'s blank Cells; nothing when there is no room or the label
/// already starts with a disclosure.
fn mark_disclosure(buffer: &mut Buffer, rect: Rect, open: bool, (fg, modifier): (Color, Modifier)) {
    let Some((first, y)) = (rect.y..rect.bottom()).find_map(|y| {
        (rect.x..rect.right())
            .find(|&x| buffer[(x, y)].symbol() != " ")
            .map(|x| (x, y))
    }) else {
        return;
    };
    if DISCLOSURE_GLYPHS.contains(&buffer[(first, y)].symbol()) {
        return;
    }
    let free = first - rect.x;
    let x = match free {
        0 => return,
        1 => first - 1,
        _ => first - 2,
    };
    let cell = &mut buffer[(x, y)];
    cell.set_symbol(if open { "▾" } else { "▸" });
    cell.fg = fg;
    cell.modifier = (cell.modifier | modifier) - Modifier::BOLD - Modifier::REVERSED;
}

/// Clears a canvas's dot at Cell (`x`, `y`) beside a text over the canvas
/// (a chart's value on its bar): the label stands apart from the marks, as
/// a canvas's own text does.
fn clear_dots(buffer: &mut Buffer, text_cells: &[bool], x: i32, y: u16) {
    let area = buffer.area;
    if x < i32::from(area.x) || x >= i32::from(area.right()) {
        return;
    }
    let at = buffer.index_of(x as u16, y);
    let cell = &mut buffer.content[at];
    let braille = cell
        .symbol()
        .chars()
        .next()
        .is_some_and(|c| ('\u{2801}'..='\u{28ff}').contains(&c));
    if braille && !text_cells[at] {
        cell.set_symbol(" ");
    }
}

/// Whether a glyph of `rect` is underlined: the app marks the view itself.
fn underlined(buffer: &Buffer, rect: Rect) -> bool {
    (rect.y..rect.bottom()).any(|y| {
        (rect.x..rect.right()).any(|x| {
            let cell = &buffer[(x, y)];
            cell.symbol() != " " && cell.modifier.contains(Modifier::UNDERLINED)
        })
    })
}

/// Changes each Cell of `rect` that shows a glyph.
fn drawn(buffer: &mut Buffer, rect: Rect, mut change: impl FnMut(&mut ratatui_core::buffer::Cell)) {
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let cell = &mut buffer[(x, y)];
            if cell.symbol() != " " {
                change(cell);
                cell.modifier -= Modifier::DIM;
            }
        }
    }
}

/// A band of `color` behind the Cells of `rect` on the view's own
/// background `own`: what its children fill with their own steps stays.
fn band(buffer: &mut Buffer, rect: Rect, color: Color, own: Color) {
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let cell = &mut buffer[(x, y)];
            if cell.bg == own {
                cell.bg = color;
            }
        }
    }
}

/// A focused box's line in `color`.
fn recolor_border(buffer: &mut Buffer, bounds: CellBounds, visible: Rect, color: Color) {
    for y in visible.y..visible.bottom() {
        for x in visible.x..visible.right() {
            let (cx, cy) = (i32::from(x), i32::from(y));
            if cx == bounds.left
                || cx == bounds.right - 1
                || cy == bounds.top
                || cy == bounds.bottom - 1
            {
                let cell = &mut buffer[(x, y)];
                if cell.symbol() != " " {
                    cell.fg = color;
                    cell.modifier -= Modifier::DIM;
                }
            }
        }
    }
}

/// A band of `color` behind every Cell of `rect`, modifiers untouched.
/// Paints `view`'s gradients (`backgroundImage`) over its fill in its
/// `visible` Cells, each Cell the color at its center, at the terminal's
/// depth; returns the color at the box's center, what its content reads
/// against. Nothing where the terminal paints no fills (the bandless look)
/// or the view has no gradient.
fn gradient(
    buffer: &mut Buffer,
    view: &NshView,
    bounds: CellBounds,
    visible: Rect,
    cells: CellSize,
    parent: Ground,
) -> Option<palette::Rgb> {
    let effects = view.style.effects.as_deref()?;
    if effects.background_images.is_empty()
        || parent.ramp.is_none()
        || visible.is_empty()
        || matches!(view.kind, ViewKind::Image | ViewKind::Video | ViewKind::WebView)
        || !effects.background_images.iter().any(crate::gradient::shows)
    {
        return None;
    }
    let fill = view
        .animated
        .background
        .unwrap_or(view.style.background_color);
    // A gradient over an opaque fill of the view's own is its finish (a
    // button's top-lit sheen): the fill is the surface, a step of the ramp.
    if fill.0 & 255 == 255 {
        return None;
    }
    let (cw, ch) = (cells.width().0, cells.height().0);
    let size = nsh_core::Size::new(
        Points((bounds.right - bounds.left) as f32 * cw),
        Points((bounds.bottom - bounds.top) as f32 * ch),
    );
    // CSS lists the top layer first.
    let layers: Vec<crate::gradient::Sampler> = effects
        .background_images
        .iter()
        .rev()
        .filter_map(|g| crate::gradient::Sampler::new(g, size))
        .collect();
    if layers.is_empty() {
        return None;
    }
    let [red, green, blue, alpha] = fill.0.to_be_bytes();
    let alpha = f32::from(alpha) / 255.0;
    let under: palette::Rgb = std::array::from_fn(|i| {
        let top = f32::from([red, green, blue][i]);
        (top * alpha + f32::from(parent.rgb[i]) * (1.0 - alpha)).round() as u8
    });
    let color_at = |point: (f32, f32)| crate::gradient::over(under, layers.iter().map(|l| l.at(point)));
    for y in visible.y..visible.bottom() {
        let py = (i32::from(y) - bounds.top) as f32 * ch + ch / 2.0;
        for x in visible.x..visible.right() {
            let px = (i32::from(x) - bounds.left) as f32 * cw + cw / 2.0;
            buffer[(x, y)].bg = palette::convert(color_at((px, py)), parent.depth);
        }
    }
    Some(color_at((size.width.0 / 2.0, size.height.0 / 2.0)))
}

/// Blanks the symbols in `rect`, keeping each Cell's background.
fn clear_symbols(buffer: &mut Buffer, rect: Rect) {
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let cell = &mut buffer[(x, y)];
            if cell.symbol() != " " {
                let bg = cell.bg;
                cell.reset();
                cell.bg = bg;
            }
        }
    }
}

fn fill(buffer: &mut Buffer, rect: Rect, color: Color) {
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            buffer[(x, y)].bg = color;
        }
    }
}

/// Adds `modifier` to each row's text in `rect`: from its first drawn Cell
/// to its last.
fn label(buffer: &mut Buffer, rect: Rect, modifier: Modifier) {
    for y in rect.y..rect.bottom() {
        let drawn = |x: u16| buffer[(x, y)].symbol() != " ";
        let Some(first) = (rect.x..rect.right()).find(|&x| drawn(x)) else {
            continue;
        };
        let last = (rect.x..rect.right())
            .rev()
            .find(|&x| drawn(x))
            .unwrap_or(first);
        for x in first..=last {
            buffer[(x, y)].modifier.insert(modifier);
        }
    }
}

/// A focused container's border: bold, no longer dim.
fn emphasize_border(buffer: &mut Buffer, bounds: CellBounds, visible: Rect) {
    for y in visible.y..visible.bottom() {
        for x in visible.x..visible.right() {
            let (cx, cy) = (i32::from(x), i32::from(y));
            let edge = cx == bounds.left
                || cx == bounds.right - 1
                || cy == bounds.top
                || cy == bounds.bottom - 1;
            if edge {
                let cell = &mut buffer[(x, y)];
                cell.modifier = (cell.modifier | Modifier::BOLD) - Modifier::DIM;
            }
        }
    }
}
