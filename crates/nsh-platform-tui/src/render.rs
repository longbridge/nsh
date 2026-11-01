use std::{collections::HashMap, sync::Arc};

use nsh_core::{
    AccessibilityRole, Bounds, Checked, NodeId, Point, PointerEvents, Points, ROOT_ID, ViewKind,
    platform::ColorScheme,
};
use ratatui_core::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};
use ratatui_widgets::{block::Block, borders::Borders, paragraph::Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::{
    CellSize,
    animation::Projection,
    geometry::{CellBounds, TextRun},
    mount::Mounter,
    palette::{Ground, ink},
    text::CellLayout,
};

const COMPACT_HEIGHT_CELLS: f32 = 1.5;
const HAIRLINE_CELL_FRACTION: f32 = 0.5;
/// The alpha from which a translucent fill clears what is under it, as an
/// opaque one does (50%); lighter fills (scrims, tints) draw nothing.
const CLEARING_ALPHA: u32 = 128;
/// The Cells between a button's label and each of its brackets.
const BRACKET_GAP: i32 = 1;

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
    text_points: Option<Bounds<Points>>,
    pub(crate) scroll_origin: Point<Points>,
}

#[derive(Clone, Copy)]
struct Clip {
    visible: Rect,
    borders: CellBounds,
    fit: bool,
    /// The Cells of the view whose children are drawn.
    parent: CellBounds,
}

/// A checkbox's, radio's or switch's state as terminal glyphs, and the view
/// the app drew it with (its indicator), whose place the glyph takes.
struct Toggle {
    glyph: &'static str,
    /// The glyph where the full one does not fit.
    short: &'static str,
    indicator: Option<NodeId>,
}

/// What a button shows between its brackets.
enum ButtonLabel {
    /// Its own text, drawn on one row.
    Text,
    /// The accessibility label of a button whose content is only icons.
    Accessible(Arc<str>),
}

/// How a view's fill shows in the terminal, which draws no fill colors.
#[derive(Clone, Copy)]
enum Fill {
    /// A sub-Cell rule: line glyphs in the fill's palette color.
    Line(&'static str, Style),
    /// A leaf whose fill is its content (a progress indicator, a thumb):
    /// reversed Cells in the fill's palette color.
    Solid(Style),
    /// An opaque fill clears what is under it to the terminal's background.
    Clear,
}

struct Painter<'a> {
    text_runs: HashMap<NodeId, Vec<TextRun>>,
    tree: &'a mut Mounter,
    buffer: &'a mut Buffer,
    hits: &'a mut Vec<Hit>,
    geometry: &'a mut Vec<Geometry>,
    area: Rect,
    cells: CellSize,
    scheme: ColorScheme,
    focused: Option<NodeId>,
    /// The opaque fill of the nearest view drawn so far that has one: what
    /// the app drew the current view on.
    background: Option<nsh_core::Color>,
    /// The boxes of the buttons being drawn, which draw no border or fill.
    button_boxes: Vec<NodeId>,
    /// The indicators of the toggles being drawn, which a glyph replaces.
    indicators: Vec<NodeId>,
    /// Whether the view being drawn is inside an indicator.
    in_indicator: bool,
    /// The Views whose only content the terminal leaves out, by node id.
    emptied: Vec<bool>,
    /// Whether the view being drawn is inside one of them.
    in_emptied: bool,
    /// Whether the view being drawn is inside a selected or checked view,
    /// whose bar marks the selection.
    in_selection: bool,
    /// The solid leaves drawn so far, and which Cells still show one, by
    /// buffer index: a selection's bar takes the place of those under it.
    solids: Vec<Rect>,
    solid_cells: Vec<bool>,
    /// The hairline rules drawn once their parent's subtree is, as a rule
    /// keeps off rows (or columns) that hold text.
    rules: Vec<Rule>,
    /// The labels of icon-only buttons drawn once their parent's subtree is.
    labels: Vec<Label>,
    /// The visible Cells of the view whose children are being drawn.
    parent_box: Rect,
}

/// An icon-only button's accessibility label waiting for its parent's
/// subtree, as it may take only blank Cells beside the button's box.
struct Label {
    text: Arc<str>,
    /// The button's visible Cells.
    cells: Rect,
    /// The row of its icons' center.
    row: u16,
    /// Where it may reach: its parent's visible Cells.
    within: Rect,
    focused: bool,
}

/// A hairline rule waiting for its parent's subtree.
struct Rule {
    cells: Rect,
    line: &'static str,
    style: Style,
    /// Where it may move: its parent's visible Cells.
    within: Rect,
}

/// Below this opacity, multiplied through its ancestors, a view draws
/// nothing; above it opacity is ignored, as the palette has no translucency.
const INVISIBLE_OPACITY: f32 = 0.03;

/// Draws the tree in the terminal's look (`palette`): colors from the
/// terminal's palette in `scheme`, no fill colors, and the `focused` view,
/// selected views and checked views as a soft bar on their text rows.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    tree: &mut Mounter,
    buffer: &mut Buffer,
    area: Rect,
    cells: CellSize,
    scheme: ColorScheme,
    focused: Option<NodeId>,
    hits: &mut Vec<Hit>,
    geometry: &mut Vec<Geometry>,
) {
    hits.clear();
    geometry.clear();
    let clip = Clip {
        visible: area.intersection(buffer.area),
        borders: CellBounds::UNBOUNDED,
        fit: false,
        parent: CellBounds::UNBOUNDED,
    };
    let mut emptied = vec![false; tree.views.len()];
    mark_emptied(tree, ROOT_ID, &mut emptied);
    for &modal in &tree.modals {
        mark_emptied(tree, modal, &mut emptied);
    }
    let solid_cells = vec![false; buffer.area.area() as usize];
    let mut painter = Painter {
        text_runs: HashMap::new(),
        tree,
        buffer,
        hits,
        geometry,
        area,
        cells,
        scheme,
        focused,
        background: None,
        button_boxes: Vec::new(),
        indicators: Vec::new(),
        in_indicator: false,
        emptied,
        in_emptied: false,
        in_selection: false,
        solids: Vec::new(),
        solid_cells,
        rules: Vec::new(),
        labels: Vec::new(),
        parent_box: area,
    };
    painter.view(ROOT_ID, Projection::IDENTITY, clip, 1.0, true, None);
    let modals = painter.tree.modals.clone();
    for modal in modals {
        if crate::navigation::modal_visible(painter.tree, modal) {
            painter.view(modal, Projection::IDENTITY, clip, 1.0, true, None);
        }
    }
}

impl Painter<'_> {
    fn view(
        &mut self,
        id: NodeId,
        parent: Projection,
        mut clip: Clip,
        opacity: f32,
        pointer: bool,
        press: Option<(NodeId, Point<Points>)>,
    ) {
        let text_stack = text_stack(self.tree, id);
        let text_parent = horizontal_text_parent(self.tree, id, parent);
        let sibling_content =
            border_has_sibling_content(self.tree, id, parent, self.cells, self.area);
        let reaches = if clip.fit {
            reserved_reach(self.tree, id, parent, self.cells, self.area, clip)
        } else {
            Reach::default()
        };
        let blocked = blocked_border_rows(self.tree, id, parent, self.cells, self.area);
        let (button, boxed) = button_label(
            self.tree,
            id,
            parent,
            self.cells,
            self.area,
            self.background,
        )
        .unzip();
        let toggle = toggle(self.tree, id, self.cells);
        let Some(view) = self.tree.get_mut(id) else {
            return;
        };
        let opacity = opacity
            * view
                .animated
                .opacity
                .unwrap_or(view.style.opacity)
                .clamp(0.0, 1.0);
        if view.concealed || view.style.hidden || opacity <= 0.0 {
            return;
        }
        self.button_boxes.extend(boxed);
        self.indicators
            .extend(toggle.as_ref().and_then(|toggle| toggle.indicator));
        let outer_in_indicator = self.in_indicator;
        self.in_indicator |= self.indicators.contains(&id);
        let outer_in_emptied = self.in_emptied;
        self.in_emptied |= self.emptied.get(id as usize).copied().unwrap_or(false);
        let unboxed = self.button_boxes.contains(&id);
        let mut projection = parent.view(view);
        let local_frame = Bounds::new(Point::default(), view.frame.size);
        let line = hairline_fill(view, projection.bounds(local_frame).size, self.cells);
        let compact =
            line.is_none() && (view.kind == ViewKind::Canvas || !view.style.radii().is_zero());
        let project = |frame: Bounds<Points>| {
            let mut bounds = CellBounds::new(frame, self.cells, self.area);
            // Compact canvases and rounded controls share their one-line sibling's center row.
            if compact
                && frame.size.height > Points::ZERO
                && frame.size.height < self.cells.height() * COMPACT_HEIGHT_CELLS
            {
                bounds.top = i32::from(self.area.y)
                    + (frame.center().y / self.cells.height()).floor() as i32;
                bounds.bottom = bounds.top.saturating_add(1);
            }
            bounds
        };
        if let Some(anchor) = view.props.overlay_anchor {
            let anchor_bounds = anchor
                .node
                .and_then(|node| self.geometry.iter().find(|geometry| geometry.id == node))
                .map(|geometry| geometry.bounds)
                .unwrap_or_else(|| {
                    CellBounds::new(parent.bounds(anchor.bounds), self.cells, self.area)
                });
            let popup = project(projection.bounds(local_frame));
            use nsh_core::overlay::Placement;
            let (columns, rows) = match anchor.placement {
                Placement::Bottom => (0, (anchor_bounds.bottom - popup.top).max(0)),
                Placement::Top => (0, (anchor_bounds.top - popup.bottom).min(0)),
                Placement::Right => ((anchor_bounds.right - popup.left).max(0), 0),
                Placement::Left => ((anchor_bounds.left - popup.right).min(0), 0),
            };
            projection = projection.snapped(Point::new(
                self.cells.width() * columns as f32,
                self.cells.height() * rows as f32,
            ));
        }
        let text_run = text_parent.and_then(|parent| {
            view.text.as_ref().and_then(|(layout, content)| {
                let layout = layout.downcast::<CellLayout>()?;
                if layout.rows.len() != 1 {
                    return None;
                }
                let row = &layout.rows[0];
                let width = row.glyphs.iter().map(|glyph| glyph.width).sum::<usize>() as i32;
                if width == 0 {
                    return None;
                }
                let indent = row.indent as i32;
                let points = projection.bounds(Bounds::new(
                    content.origin + Point::new(self.cells.width() * indent as f32, Points::ZERO),
                    nsh_core::Size::new(self.cells.width() * width as f32, self.cells.height()),
                ));
                let mut cells = CellBounds::new(points, self.cells, self.area);
                cells.right = cells.left.saturating_add(width);
                cells.bottom = cells.top.saturating_add(1);
                let run = TextRun { points, cells }
                    .separated(self.text_runs.get(&parent).into_iter().flatten());
                let delta = run.cells.left.saturating_sub(cells.left);
                let frame = project(projection.bounds(local_frame));
                let narrowed = run.cells.right - run.cells.left < width;
                if delta > 0 || narrowed {
                    let edge = run
                        .cells
                        .left
                        .saturating_sub(cells.left.saturating_sub(frame.left));
                    clip.borders.left = clip.borders.left.max(edge).min(clip.borders.right);
                }
                if delta < 0 || narrowed {
                    let edge = run
                        .cells
                        .right
                        .saturating_add(frame.right.saturating_sub(cells.right));
                    clip.borders.right = clip.borders.right.min(edge).max(clip.borders.left);
                }
                projection =
                    projection.snapped(Point::new(self.cells.width() * delta as f32, Points::ZERO));
                Some((parent, run, indent, width))
            })
        });
        let frame = projection.bounds(local_frame);
        let bounds = project(frame);
        let content_projection = projection;
        let raw_bounds = bounds;
        let mut bounds = if clip.fit {
            // A border reserves only its own side: on the other sides the
            // box is not pushed past its parent's edge, it narrows instead.
            let limits = CellBounds {
                left: clip.borders.left.max(bounds.left.min(clip.parent.left)),
                top: clip.borders.top.max(bounds.top.min(clip.parent.top)),
                right: clip.borders.right.min(bounds.right.max(clip.parent.right)),
                bottom: clip
                    .borders
                    .bottom
                    .min(bounds.bottom.max(clip.parent.bottom)),
            };
            let mut fitted = bounds.fitted(limits);
            // Content clear of the reserved columns or rows keeps its own.
            let kept = bounds.constrained(limits);
            if !reaches.columns {
                fitted.left = kept.left;
                fitted.right = kept.right;
            }
            if !reaches.rows {
                fitted.top = kept.top;
                fitted.bottom = kept.bottom;
            }
            fitted
        } else {
            bounds.constrained(clip.borders)
        };
        let shift = if reaches.columns {
            bounds.left - raw_bounds.left
        } else {
            0
        };
        let mut contract = reaches.rows;
        if clip.fit
            && view.style.borders.widths.top > Points::ZERO
            && !bounds.clipped(clip.visible).is_empty()
        {
            // Reserving an ancestor's border can move the previous sibling's
            // text down. Keep that row when the next inset rounds onto it.
            let visible = bounds.clipped(clip.visible);
            let mut top = bounds.top;
            for geometry in self.geometry.iter() {
                if let Some(text) = geometry.text_frame {
                    let text = CellBounds::new(text, self.cells, self.area)
                        .clipped(geometry.visible.intersection(visible));
                    if (text.y..text.bottom()).any(|y| {
                        (text.x..text.right()).any(|x| self.buffer[(x, y)].symbol() != " ")
                    }) {
                        top = top.max(i32::from(text.bottom()));
                    }
                }
            }
            contract |= top.min(bounds.bottom) != bounds.top;
            bounds.top = top.min(bounds.bottom);
        }
        if clip.fit {
            projection =
                projection.snapped(Point::new(self.cells.width() * shift as f32, Points::ZERO));
        }
        if clip.fit && contract {
            let old_height = raw_bounds.bottom - raw_bounds.top;
            let new_height = bounds.bottom - bounds.top;
            let scale = if old_height > 0 {
                f64::from(new_height) / f64::from(old_height)
            } else {
                1.0
            };
            let old_top = raw_bounds.origin(self.cells, self.area).y;
            let new_top = bounds.origin(self.cells, self.area).y;
            projection.matrix.0[1] *= scale;
            projection.matrix.0[5] *= scale;
            projection.matrix.0[13] =
                (projection.matrix.0[13] - f64::from(old_top.0)) * scale + f64::from(new_top.0);
        }
        if let Some((stack, rows)) = text_stack {
            let stack = projection.bounds(stack);
            let height = self.cells.height() * rows as f32;
            if stack.size.height > Points::ZERO && stack.size.height < height {
                let top = i32::from(self.area.y)
                    + (stack.center().y / self.cells.height() - rows as f32 / 2.0 + 0.5).floor()
                        as i32;
                let target = CellBounds {
                    top,
                    bottom: top.saturating_add(rows),
                    ..bounds
                };
                if bounds.bottom - bounds.top < rows {
                    bounds.top = target.top;
                    bounds.bottom = target.bottom;
                    bounds = bounds.fitted(clip.borders);
                }
                let target = target.fitted(bounds);
                if target.bottom - target.top == rows {
                    let scale = f64::from(height / stack.size.height);
                    projection.matrix.0[1] *= scale;
                    projection.matrix.0[5] *= scale;
                    projection.matrix.0[13] =
                        (projection.matrix.0[13] - f64::from(stack.origin.y.0)) * scale
                            + f64::from(target.origin(self.cells, self.area).y.0);
                }
            }
        }
        let origin = bounds.origin(self.cells, self.area);
        let visible = bounds.clipped(clip.visible);
        let pointer = pointer && view.props.pointer_events != PointerEvents::None;
        let press = if pointer
            && view.props.pointer_events != PointerEvents::BoxNone
            && !view.props.accessibility.state.disabled
            && nsh_core::input::has_press(view.events)
        {
            Some((id, origin))
        } else {
            press
        };
        if pointer && view.props.pointer_events != PointerEvents::BoxNone && !visible.is_empty() {
            self.hits.push(Hit {
                node: id,
                area: visible,
                target: press.map(|p| p.0),
                origin: press.map_or(origin, |p| p.1),
                node_origin: origin,
            });
        }
        let children_pointer = pointer && view.props.pointer_events != PointerEvents::BoxOnly;
        let widths = &view.style.borders.widths;
        let colors = view
            .animated
            .border_color
            .map_or(view.style.borders.colors, |color| [color; 4]);
        // A button reads `[ label ]`: no border or fill around it.
        let sides = std::array::from_fn::<_, 4, _>(|i| {
            [widths.top, widths.right, widths.bottom, widths.left][i] > Points::ZERO
                && !colors[i].is_transparent()
                && !unboxed
                && !(i == 0 && blocked.top)
                && !(i == 2 && blocked.bottom)
        });
        let content = i32::from(
            view.kind != ViewKind::View
                || !view.children.is_empty()
                || view.component.is_some()
                || sibling_content,
        );
        // A border is drawn whole or not at all: each drawn side owns a Cell,
        // and content keeps a row and a column between them.
        let room = bounds.bottom - bounds.top
            >= i32::from(sides[0]) + i32::from(sides[2]) + content
            && bounds.right - bounds.left >= i32::from(sides[1]) + i32::from(sides[3]) + content;
        let sides = sides.map(|side| side && room);
        let inner = CellBounds {
            left: bounds.left + i32::from(sides[3]),
            top: bounds.top + i32::from(sides[0]),
            right: (bounds.right - i32::from(sides[1])).max(bounds.left + i32::from(sides[3])),
            bottom: (bounds.bottom - i32::from(sides[2])).max(bounds.top + i32::from(sides[0])),
        };
        let text_bounds = view.text.as_ref().map(|(_, content)| {
            CellBounds::new(content_projection.bounds(*content), self.cells, self.area)
                .fitted(inner)
        });
        if let Some(((parent, mut run, indent, width), text)) = text_run.zip(text_bounds) {
            run.cells = CellBounds {
                left: text.left.saturating_add(indent).min(text.right),
                right: text
                    .left
                    .saturating_add(indent)
                    .saturating_add(width)
                    .min(text.right),
                top: text.top,
                bottom: text.top.saturating_add(1).min(text.bottom),
            };
            self.text_runs.entry(parent).or_default().push(run);
        }
        self.geometry.push(Geometry {
            id,
            bounds,
            visible,
            frame: Bounds::new(origin, frame.size),
            text_points: view
                .text
                .as_ref()
                .map(|(_, content)| content_projection.bounds(*content)),
            text_frame: text_bounds.map(|text| {
                Bounds::new(
                    text.origin(self.cells, self.area),
                    nsh_core::Size::new(
                        self.cells.width() * (text.right - text.left) as f32,
                        self.cells.height() * (text.bottom - text.top) as f32,
                    ),
                )
            }),
            scroll_origin: view
                .scroll
                .as_ref()
                .map_or_else(Point::default, |scroll| scroll.origin()),
        });
        let own_geometry = self.geometry.len() - 1;
        let children_clip = Clip {
            fit: sides.iter().any(|&side| side) && view.kind != ViewKind::Scroll,
            parent: bounds,
            visible: if view.style.overflow_hidden
                || matches!(
                    view.kind,
                    ViewKind::Scroll | ViewKind::Stack | ViewKind::Screen | ViewKind::Modal
                ) {
                inner.clipped(clip.visible)
            } else {
                clip.visible
            },
            borders: CellBounds {
                left: if sides[3] {
                    inner.left
                } else {
                    clip.borders.left
                },
                top: if sides[0] {
                    inner.top
                } else {
                    clip.borders.top
                },
                right: if sides[1] {
                    inner.right
                } else {
                    clip.borders.right
                },
                bottom: if sides[2] {
                    inner.bottom
                } else {
                    clip.borders.bottom
                },
            },
        };
        let radii = view.style.radii().fit(frame.size);
        let clipped_corners: Vec<_> = if view.style.overflow_hidden && !radii.is_zero() {
            (visible.y..visible.bottom())
                .flat_map(|y| (visible.x..visible.right()).map(move |x| (x, y)))
                .filter(|&(x, y)| !bounds.contains_rounded(x, y, frame.size, radii))
                .map(|position| (position, self.buffer[position].clone()))
                .collect()
        } else {
            Vec::new()
        };
        let decorative = is_decorative(view);
        let shown = !visible.is_empty()
            && opacity >= INVISIBLE_OPACITY
            && !decorative
            && !self.in_indicator;
        let visible_drawn = shown && !self.in_emptied;
        let outer_background = self.background;
        let own_fill = view
            .animated
            .background
            .unwrap_or(view.style.background_color);
        if own_fill.0 & 255 == 255 {
            self.background = Some(own_fill);
        }
        // Fills and borders sit on what is under the view; its content on its
        // own fill.
        let outer_ground = Ground::new(self.scheme, outer_background);
        let ground = Ground::new(self.scheme, self.background);
        if visible_drawn {
            let fill = if unboxed {
                nsh_core::Color::TRANSPARENT
            } else {
                view.animated
                    .background
                    .unwrap_or(view.style.background_color)
            };
            let rounded = !radii.is_zero();
            // A view that holds text never draws its fill as content.
            let leaf = view.kind == ViewKind::View
                && view.children.is_empty()
                && view.component.is_none()
                && view.text.is_none();
            let fill = if fill.is_transparent() {
                None
            } else if let Some(line) = line {
                Some(Fill::Line(line, ink(fill, outer_ground)))
            } else if fill.0 & 255 < CLEARING_ALPHA {
                None
            } else if leaf {
                Some(Fill::Solid(ink(fill, outer_ground)))
            } else {
                Some(Fill::Clear)
            };
            // The bar marks a selection: its accent bars and leaves would
            // mark it twice.
            let fill = fill.filter(|fill| {
                !self.in_selection
                    || match fill {
                        Fill::Line(_, style) => style.fg == Some(Color::Reset),
                        Fill::Solid(_) => false,
                        Fill::Clear => true,
                    }
            });
            let protected = if fill.is_some()
                && (frame.size.height < self.cells.height()
                    || frame.size.width < self.cells.width())
            {
                disjoint_text_cells(
                    self.geometry,
                    self.buffer,
                    frame,
                    visible,
                    self.cells,
                    self.area,
                )
            } else {
                Vec::new()
            };
            let covered = |x, y| {
                (!rounded || bounds.contains_rounded(x, y, frame.size, radii))
                    && (protected.is_empty()
                        || !protected[usize::from(y - visible.y) * usize::from(visible.width)
                            + usize::from(x - visible.x)])
            };
            if let Some(Fill::Line(line, style)) = fill {
                self.rules.push(Rule {
                    cells: visible,
                    line,
                    style,
                    within: self.parent_box,
                });
            } else if let Some(fill) = fill {
                fill_cells(self.buffer, visible, fill, covered);
                let solid = matches!(fill, Fill::Solid(_));
                for y in visible.y..visible.bottom() {
                    for x in visible.x..visible.right() {
                        if covered(x, y) {
                            self.solid_cells[self.buffer.index_of(x, y)] = solid;
                        }
                    }
                }
                if solid {
                    self.solids.push(visible);
                }
            }
            let mut borders = Borders::NONE;
            if sides[0] && bounds.top == i32::from(visible.y) {
                borders |= Borders::TOP;
            }
            if sides[2] && bounds.bottom == i32::from(visible.bottom()) {
                borders |= Borders::BOTTOM;
            }
            if sides[3] && bounds.left == i32::from(visible.x) {
                borders |= Borders::LEFT;
            }
            if sides[1] && bounds.right == i32::from(visible.right()) {
                borders |= Borders::RIGHT;
            }
            Block::default()
                .borders(borders)
                .border_set(ratatui_core::symbols::border::PLAIN)
                .render(visible, self.buffer);
            // Each edge keeps its own palette color.
            for (side, color) in colors.iter().enumerate() {
                let edge = match side {
                    0 if borders.contains(Borders::TOP) => {
                        Rect::new(visible.x, visible.y, visible.width, 1)
                    }
                    1 if borders.contains(Borders::RIGHT) => {
                        Rect::new(visible.right() - 1, visible.y, 1, visible.height)
                    }
                    2 if borders.contains(Borders::BOTTOM) => {
                        Rect::new(visible.x, visible.bottom() - 1, visible.width, 1)
                    }
                    3 if borders.contains(Borders::LEFT) => {
                        Rect::new(visible.x, visible.y, 1, visible.height)
                    }
                    _ => continue,
                };
                let style = ink(*color, outer_ground);
                for y in edge.y..edge.bottom() {
                    for x in edge.x..edge.right() {
                        self.buffer[(x, y)].set_style(style);
                    }
                }
            }
            if let Some((layout, _)) = &view.text
                && let Some(layout) = layout.downcast::<CellLayout>()
            {
                let text_bounds = text_bounds.expect("text geometry");
                let text_area = text_bounds.clipped(visible);
                let scroll = (
                    (i32::from(text_area.y) - text_bounds.top).clamp(0, u16::MAX as i32) as u16,
                    (i32::from(text_area.x) - text_bounds.left).clamp(0, u16::MAX as i32) as u16,
                );
                let paragraph = layout.paragraph(ground);
                if scroll == (0, 0) {
                    paragraph.as_ref().render(text_area, self.buffer);
                } else {
                    Paragraph::clone(&paragraph)
                        .scroll(scroll)
                        .render(text_area, self.buffer);
                }
                if let Some(color) = view.animated.color {
                    let style = ink(color, ground);
                    for y in text_area.y..text_area.bottom() {
                        for x in text_area.x..text_area.right() {
                            self.buffer[(x, y)].set_style(style);
                        }
                    }
                }
            }
            if let (Some(field), Some(config)) = (&mut view.field, &view.input) {
                let content = CellBounds::new(
                    content_projection.bounds(field.content),
                    self.cells,
                    self.area,
                )
                .fitted(inner);
                field.draw(
                    config,
                    content,
                    inner.clipped(clip.visible),
                    self.buffer,
                    view.animated.color,
                    ground,
                );
            }
            if let Some(canvas) = &view.canvas {
                crate::canvas::draw(
                    canvas,
                    self.buffer,
                    bounds,
                    inner.clipped(clip.visible),
                    self.cells,
                    projection.snapped(Point::new(-origin.x, -origin.y)),
                    ground,
                );
            }
            if let Some(component) = &mut view.component {
                component.render(inner.clipped(clip.visible), self.buffer);
            } else if !matches!(
                view.kind,
                ViewKind::View
                    | ViewKind::Text
                    | ViewKind::Input
                    | ViewKind::Canvas
                    | ViewKind::Scroll
                    | ViewKind::Stack
                    | ViewKind::Screen
                    | ViewKind::HeaderItem
                    | ViewKind::Modal
            ) {
                // An image's placeholder carries its accessibility label.
                let title = match (view.kind, &view.props.accessibility.label) {
                    (ViewKind::Image, Some(label)) => label.to_string(),
                    _ => view
                        .component_name
                        .clone()
                        .unwrap_or_else(|| format!("{:?}", view.kind)),
                };
                Block::bordered().title(title).render(visible, self.buffer);
            }
        }
        let outer_in_selection = self.in_selection;
        self.in_selection |= visible_drawn && shows_state(view);
        let scroll_origin = view
            .scroll
            .as_ref()
            .map_or_else(Point::default, |scroll| scroll.origin());
        let child_projection =
            projection.translated(Point::new(-scroll_origin.x, -scroll_origin.y));
        let pitch = even_pitch(self.tree, id, child_projection, self.cells);
        let outer_parent_box = std::mem::replace(&mut self.parent_box, visible);
        let rules = self.rules.len();
        let labels = self.labels.len();
        for (index, delta) in pitch.into_iter().enumerate() {
            let child = self.tree.get(id).unwrap().children[index];
            if self
                .tree
                .get(child)
                .is_some_and(|v| v.kind == ViewKind::Modal)
            {
                continue;
            }
            self.view(
                child,
                child_projection.snapped(Point::new(Points::ZERO, delta)),
                children_clip,
                opacity,
                children_pointer,
                press,
            );
        }
        self.parent_box = outer_parent_box;
        self.in_selection = outer_in_selection;
        for label in self.labels.split_off(labels) {
            self.label(label);
        }
        for rule in self.rules.split_off(rules) {
            self.rule(rule);
        }
        for (position, cell) in clipped_corners {
            self.buffer[position] = cell;
        }
        self.background = outer_background;
        if boxed.is_some() {
            self.button_boxes.pop();
        }
        self.in_indicator = outer_in_indicator;
        self.in_emptied = outer_in_emptied;
        if let Some(toggle) = &toggle
            && toggle.indicator.is_some()
        {
            self.indicators.pop();
        }
        let focused = self.focused == Some(id);
        let mut labeled = false;
        match button {
            Some(ButtonLabel::Text) if shown => self.brackets(visible, own_geometry),
            Some(ButtonLabel::Accessible(text)) if shown => {
                let center = i32::from(self.area.y)
                    + (frame.center().y / self.cells.height()).floor() as i32;
                self.labels.push(Label {
                    text,
                    cells: visible,
                    row: center.clamp(i32::from(visible.y), i32::from(visible.bottom()) - 1) as u16,
                    within: self.parent_box.intersection(clip.visible),
                    focused,
                });
                labeled = true;
            }
            _ => {}
        }
        if visible_drawn && let Some(toggle) = toggle {
            self.toggle_glyph(visible, toggle, own_geometry);
        }
        let Some(view) = self.tree.get(id) else {
            return;
        };
        let shows_state = shows_state(view);
        // An emptied box shows no selection, but focus stays visible on it;
        // an icon-only button's label takes its bar.
        let focused = focused && view.input.is_none() && !labeled;
        if shows_state && visible_drawn {
            self.unmark(visible);
        }
        if (shows_state && visible_drawn) || (focused && shown) {
            self.bar(visible, own_geometry);
        }
    }
}

impl Painter<'_> {
    /// Puts the soft bar of a selected or focused view on the rows of
    /// `visible` that hold the text of the views from `texts` on in
    /// `geometry`, else on its first row: across its width, the palette's
    /// color 0 in the dark appearance or color 7 in the light one, in bold.
    fn bar(&mut self, visible: Rect, texts: usize) {
        let mut rows: Vec<u16> = Vec::new();
        for geometry in &self.geometry[texts..] {
            let Some(text) = geometry.text_frame else {
                continue;
            };
            let text = CellBounds::new(text, self.cells, self.area)
                .clipped(geometry.visible.intersection(visible));
            for y in text.y..text.bottom() {
                if !rows.contains(&y)
                    && (text.x..text.right()).any(|x| self.buffer[(x, y)].symbol() != " ")
                {
                    rows.push(y);
                }
            }
        }
        if rows.is_empty() {
            rows.push(visible.y);
        }
        for y in rows {
            self.bar_row(y, visible.x..visible.right());
        }
    }

    /// Takes back the solid leaves whose Cells overlap a selected view's
    /// `visible` Cells (a sliding indicator under a selected tab): its bar
    /// marks the selection.
    fn unmark(&mut self, visible: Rect) {
        let (under, others): (Vec<_>, Vec<_>) = std::mem::take(&mut self.solids)
            .into_iter()
            .partition(|solid| solid.intersects(visible));
        self.solids = others;
        for solid in under {
            for y in solid.y..solid.bottom() {
                for x in solid.x..solid.right() {
                    let index = self.buffer.index_of(x, y);
                    if self.solid_cells[index] {
                        self.solid_cells[index] = false;
                        self.buffer[(x, y)].modifier.remove(Modifier::REVERSED);
                    }
                }
            }
        }
    }

    fn bar_row(&mut self, y: u16, columns: std::ops::Range<u16>) {
        let color = match self.scheme {
            ColorScheme::Dark => Color::Black,
            ColorScheme::Light => Color::Gray,
        };
        for x in columns {
            let cell = &mut self.buffer[(x, y)];
            cell.bg = color;
            cell.modifier.insert(Modifier::BOLD);
        }
    }

    /// Draws an icon-only button's accessibility label as `[ label ]`, else
    /// `[label]`, on its icons' row: centered in its box, or over the box and
    /// blank Cells beside it, a blank Cell away from other glyphs, closest to
    /// centered. A label that does not fit whole is not drawn. Focus puts the
    /// bar under the box and the label.
    fn label(&mut self, label: Label) {
        let Label {
            text,
            cells,
            row,
            within,
            focused,
        } = label;
        let (left, right) = (i32::from(cells.x), i32::from(cells.right()));
        let blank = |buffer: &Buffer, x: i32| {
            u16::try_from(x).is_ok_and(|x| {
                within.contains(ratatui_core::layout::Position::new(x, row))
                    && buffer[(x, row)].symbol() == " "
            })
        };
        let apart = |buffer: &Buffer, x: i32| {
            u16::try_from(x).is_err()
                || !within.contains(ratatui_core::layout::Position::new(x as u16, row))
                || buffer[(x as u16, row)].symbol() == " "
        };
        let placed = [format!("[ {text} ]"), format!("[{text}]")]
            .into_iter()
            .find_map(|shown| {
                let width = shown.width() as i32;
                if width <= right - left {
                    return Some((left + (right - left - width) / 2, shown));
                }
                let centered = left - (width - (right - left)) / 2;
                let mut starts: Vec<i32> = (right - width..=left).collect();
                starts.sort_by_key(|start| (start - centered).abs());
                starts
                    .into_iter()
                    .find(|&start| {
                        let end = start + width;
                        (start..left)
                            .chain(right..end)
                            .all(|x| blank(self.buffer, x))
                            && (start == left || apart(self.buffer, start - 1))
                            && (end == right || apart(self.buffer, end))
                    })
                    .map(|start| (start, shown))
            });
        let mut columns = cells.x..cells.right();
        if let Some((start, shown)) = placed {
            let start = start as u16;
            self.buffer.set_string(start, row, &shown, Style::reset());
            columns = start.min(cells.x)..(start + shown.width() as u16).max(cells.right());
        }
        if focused {
            self.bar_row(row, columns);
        }
    }

    /// Draws a hairline rule on its own row (or column) when that holds no
    /// text, else on the nearest one inside its parent that holds none, else
    /// not at all.
    fn rule(&mut self, rule: Rule) {
        let Rule {
            cells,
            line,
            style,
            within,
        } = rule;
        let horizontal = line == "─";
        let free = |buffer: &Buffer, at: u16| {
            let cells: Box<dyn Iterator<Item = (u16, u16)>> = if horizontal {
                Box::new((cells.x..cells.right()).map(move |x| (x, at)))
            } else {
                Box::new((cells.y..cells.bottom()).map(move |y| (at, y)))
            };
            cells
                .into_iter()
                .all(|position| matches!(buffer[position].symbol(), " " | "─" | "│"))
        };
        let (first, end, low, high) = if horizontal {
            (cells.y, cells.bottom(), within.y, within.bottom())
        } else {
            (cells.x, cells.right(), within.x, within.right())
        };
        for at in first..end {
            let target = (0..=high.saturating_sub(low)).find_map(|distance| {
                [at.checked_sub(distance), at.checked_add(distance)]
                    .into_iter()
                    .flatten()
                    .find(|&near| (low..high).contains(&near) && free(self.buffer, near))
            });
            let Some(target) = target else {
                continue;
            };
            let positions: Vec<_> = if horizontal {
                (cells.x..cells.right()).map(|x| (x, target)).collect()
            } else {
                (cells.y..cells.bottom()).map(|y| (target, y)).collect()
            };
            for position in positions {
                let cell = &mut self.buffer[position];
                cell.reset();
                cell.set_symbol(line).set_style(style);
                self.solid_cells[self.buffer.index_of(position.0, position.1)] = false;
            }
        }
    }

    /// Draws a toggle's glyph on its label's first row (else its indicator's
    /// middle row). Before the label it ends one Cell from the label's first
    /// glyph, reaching left into the control's blank Cells, else its short
    /// form does, else it sits flush with the label. An indicator after the
    /// label (a trailing switch) takes the glyph from its left column,
    /// extending right only into blank Cells, else the short form.
    fn toggle_glyph(&mut self, visible: Rect, toggle: Toggle, texts: usize) {
        let label = self.text_extent(visible, texts);
        let indicator = toggle.indicator.and_then(|indicator| {
            let bounds = self.geometry[texts..]
                .iter()
                .find(|geometry| geometry.id == indicator)?
                .bounds;
            Some((indicator, bounds))
        });
        let style = match (indicator, label) {
            (Some((indicator, _)), _) => self
                .tree
                .get(indicator)
                .map_or_else(Style::reset, |view| self.indicator_ink(view)),
            (None, Some((y, left, _))) => self.buffer[(left, y)].style(),
            (None, None) => return,
        };
        let blank = |buffer: &Buffer, x: u16, y: u16| {
            visible.contains(ratatui_core::layout::Position::new(x, y))
                && buffer[(x, y)].symbol() == " "
        };
        let glyphs = [toggle.glyph, toggle.short];
        match (indicator, label) {
            (indicator, Some((y, left, _)))
                if indicator.is_none_or(|(_, bounds)| bounds.left < i32::from(left)) =>
            {
                let placed = [
                    (glyphs[0], 1),
                    (glyphs[1], 1),
                    (glyphs[0], 0),
                    (glyphs[1], 0),
                ]
                .into_iter()
                .find_map(|(glyph, gap)| {
                    let start = left.checked_sub(glyph.width() as u16 + gap)?;
                    (start..left)
                        .all(|x| blank(self.buffer, x, y))
                        .then_some((start, glyph))
                });
                if let Some((start, glyph)) = placed {
                    self.buffer.set_string(start, y, glyph, style);
                }
            }
            (Some((_, bounds)), label) => {
                let row = label.map_or(
                    bounds.top + (bounds.bottom - bounds.top - 1).max(0) / 2,
                    |(y, ..)| i32::from(y),
                );
                let (Ok(x), Ok(y)) = (u16::try_from(bounds.left), u16::try_from(row)) else {
                    return;
                };
                if !visible.contains(ratatui_core::layout::Position::new(x, y)) {
                    return;
                }
                let room = (x..visible.right())
                    .take_while(|&column| {
                        i32::from(column) < bounds.right || blank(self.buffer, column, y)
                    })
                    .count() as u16;
                if let Some(glyph) = glyphs
                    .into_iter()
                    .find(|glyph| glyph.width() as u16 <= room)
                {
                    self.buffer.set_string(x, y, glyph, style);
                }
            }
            (None, _) => {}
        }
    }

    /// The palette ink of an indicator: its opaque fill's, else its border's.
    fn indicator_ink(&self, view: &crate::view::NshView) -> Style {
        let ground = Ground::new(self.scheme, self.background);
        let fill = view
            .animated
            .background
            .unwrap_or(view.style.background_color);
        if fill.0 & 255 == 255 {
            return ink(fill, ground);
        }
        let colors = view
            .animated
            .border_color
            .map_or(view.style.borders.colors, |color| [color; 4]);
        colors
            .into_iter()
            .find(|color| !color.is_transparent())
            .map_or_else(Style::reset, |color| ink(color, ground))
    }

    /// The row, first column and end column of the first row of text the text
    /// views from `texts` on in `geometry` draw inside `visible`.
    fn text_extent(&self, visible: Rect, texts: usize) -> Option<(u16, u16, u16)> {
        let mut label: Option<(u16, u16, u16)> = None;
        for geometry in &self.geometry[texts..] {
            let Some(text) = geometry.text_frame else {
                continue;
            };
            let text = CellBounds::new(text, self.cells, self.area)
                .clipped(geometry.visible.intersection(visible));
            for y in text.y..text.bottom() {
                for x in text.x..text.right() {
                    let symbol = self.buffer[(x, y)].symbol();
                    if symbol == " " || label.is_some_and(|(row, ..)| row != y) {
                        continue;
                    }
                    let end = x + symbol.width().max(1) as u16;
                    label = Some(label.map_or((y, x, end), |(row, left, right)| {
                        (row, left.min(x), right.max(end))
                    }));
                }
            }
        }
        label
    }

    /// Draws a button's brackets in its own Cells around its one row of text
    /// (the text views from `texts` on in `geometry`), one Cell away on both
    /// sides when the box has room, else next to it.
    fn brackets(&mut self, visible: Rect, texts: usize) {
        let Some((y, left, right)) = self.text_extent(visible, texts) else {
            return;
        };
        let free = |x: u16| {
            (visible.x..visible.right()).contains(&x) && self.buffer[(x, y)].symbol() == " "
        };
        // The same gap on both sides, the widest the box has room for,
        // and both brackets or neither: a label flush with its box
        // stays bare.
        let Some(gap) = (0..=BRACKET_GAP as u16).rev().find(|&gap| {
            left > gap && (left - gap - 1..left).all(free) && (right..=right + gap).all(free)
        }) else {
            return;
        };
        let style = self.buffer[(left, y)].style();
        self.buffer[(left - gap - 1, y)]
            .set_symbol("[")
            .set_style(style);
        self.buffer[(right + gap, y)]
            .set_symbol("]")
            .set_style(style);
    }
}

/// What a button shows between brackets: a pressable view (role button, or no
/// role and press handlers) that the app drew as a box (a border, or an opaque
/// fill unlike the `background` it sits on), whose text sits on one row, or
/// whose content is only icons and which has an accessibility label. List
/// items without a box, selected or checked views (shown with the bar) and other
/// content (fields, labeled canvases, images, native components) keep their
/// look.
fn button_label(
    tree: &Mounter,
    id: NodeId,
    projection: Projection,
    cells: CellSize,
    area: Rect,
    background: Option<nsh_core::Color>,
) -> Option<(ButtonLabel, NodeId)> {
    let view = tree.get(id)?;
    let pressable = match view.props.accessibility.role {
        AccessibilityRole::Button | AccessibilityRole::ImageButton => true,
        AccessibilityRole::None => nsh_core::input::has_press(view.events),
        _ => false,
    };
    let state = &view.props.accessibility.state;
    if !pressable || state.selected || state.checked == Some(Checked::True) {
        return None;
    }
    // The box is the pressable's own, or that of the plain View it wraps all
    // its content in (a press handler around a styled View).
    let mut boxed = id;
    loop {
        let view = tree.get(boxed)?;
        if has_box(view, background) {
            break;
        }
        let mut shown = view.children.iter().filter(|&&child| {
            tree.get(child)
                .is_some_and(|child| !child.concealed && !child.style.hidden)
        });
        match (shown.next(), shown.next()) {
            (Some(&child), None) if tree.get(child)?.kind == ViewKind::View => boxed = child,
            _ => return None,
        }
    }
    let mut rows = Vec::new();
    if !text_rows(tree, id, projection, cells, area, &mut rows) {
        return None;
    }
    let label = match rows.len() {
        0 => ButtonLabel::Accessible(view.props.accessibility.label.clone()?),
        1 => ButtonLabel::Text,
        _ => return None,
    };
    Some((label, boxed))
}

/// Whether the terminal leaves `view` out as decoration: icons (canvases
/// with role `image` or no accessibility label) and images without an
/// accessibility label.
fn is_decorative(view: &crate::view::NshView) -> bool {
    let label = &view.props.accessibility.label;
    match view.kind {
        ViewKind::Canvas => {
            view.props.accessibility.role == AccessibilityRole::Image || label.is_none()
        }
        ViewKind::Image => label.is_none(),
        _ => false,
    }
}

/// What a subtree holds once the terminal leaves out its decorations.
#[derive(Clone, Copy, Default)]
struct Holds {
    /// Something drawn as content: text, a field, a labeled canvas or image,
    /// a native component, a placeholder, a toggle's glyph.
    content: bool,
    /// Something left out: an icon or a decorative image.
    dropped: bool,
}

/// Marks in `emptied` the Views under `id` whose only content was left out
/// (an icon tile, a logo's frame): they draw no border, fill or bar.
fn mark_emptied(tree: &Mounter, id: NodeId, emptied: &mut [bool]) -> Holds {
    let Some(view) = tree.get(id) else {
        return Holds::default();
    };
    if view.concealed || view.style.hidden {
        return Holds::default();
    }
    if is_decorative(view) {
        return Holds {
            content: false,
            dropped: true,
        };
    }
    let mut holds = Holds {
        content: !matches!(
            view.kind,
            ViewKind::View
                | ViewKind::Text
                | ViewKind::Scroll
                | ViewKind::Stack
                | ViewKind::Screen
                | ViewKind::HeaderItem
                | ViewKind::Modal
        ) || view.component.is_some()
            || view.input.is_some()
            || is_toggle(view.props.accessibility.role)
            || view.text.as_ref().is_some_and(|(layout, _)| {
                layout.downcast::<CellLayout>().is_some_and(|layout| {
                    layout.rows.iter().any(|row| {
                        row.glyphs
                            .iter()
                            .any(|glyph| !glyph.text.chars().all(char::is_whitespace))
                    })
                })
            }),
        dropped: false,
    };
    for &child in &view.children {
        if tree
            .get(child)
            .is_some_and(|child| child.kind == ViewKind::Modal)
        {
            continue;
        }
        let child = mark_emptied(tree, child, emptied);
        holds.content |= child.content;
        holds.dropped |= child.dropped;
    }
    if view.kind == ViewKind::View && holds.dropped && !holds.content {
        emptied[id as usize] = true;
    }
    holds
}

/// Whether `view` shows its selected or checked state with the bar; a toggle
/// shows it with its glyph, and only focus puts a bar on it.
fn shows_state(view: &crate::view::NshView) -> bool {
    let state = &view.props.accessibility.state;
    !is_toggle(view.props.accessibility.role)
        && (state.selected || state.checked == Some(Checked::True))
}

fn is_toggle(role: AccessibilityRole) -> bool {
    matches!(
        role,
        AccessibilityRole::Checkbox | AccessibilityRole::Radio | AccessibilityRole::Switch
    )
}

/// A checkbox's, radio's or switch's glyphs for its checked state, and its
/// indicator: the first view in it, in tree order, that the app drew as a box
/// (a border or an opaque fill, not a hairline rule) holding no text.
fn toggle(tree: &Mounter, id: NodeId, cells: CellSize) -> Option<Toggle> {
    let view = tree.get(id)?;
    let checked = view.props.accessibility.state.checked;
    let (glyph, short) = match (view.props.accessibility.role, checked) {
        (AccessibilityRole::Checkbox, Some(Checked::True)) => ("[x]", "x"),
        (AccessibilityRole::Checkbox, Some(Checked::Mixed)) => ("[-]", "-"),
        (AccessibilityRole::Checkbox, _) => ("[ ]", " "),
        (AccessibilityRole::Radio, Some(Checked::True)) => ("(•)", "•"),
        (AccessibilityRole::Radio, _) => ("( )", " "),
        (AccessibilityRole::Switch, Some(Checked::True)) => ("[on ]", "on"),
        (AccessibilityRole::Switch, _) => ("[off]", "off"),
        _ => return None,
    };
    Some(Toggle {
        glyph,
        short,
        indicator: indicator(tree, id, cells),
    })
}

fn indicator(tree: &Mounter, id: NodeId, cells: CellSize) -> Option<NodeId> {
    let view = tree.get(id)?;
    view.children.iter().find_map(|&child| {
        let shown = tree.get(child)?;
        if shown.concealed || shown.style.hidden {
            return None;
        }
        let rule = shown.frame.size.width < cells.width() * HAIRLINE_CELL_FRACTION
            || shown.frame.size.height < cells.height() * HAIRLINE_CELL_FRACTION;
        if has_box(shown, None) && !rule && !holds_text(tree, child) {
            Some(child)
        } else {
            indicator(tree, child, cells)
        }
    })
}

fn holds_text(tree: &Mounter, id: NodeId) -> bool {
    tree.get(id).is_some_and(|view| {
        view.input.is_some()
            || view.text.as_ref().is_some_and(|(layout, _)| {
                layout.downcast::<CellLayout>().is_some_and(|layout| {
                    layout.rows.iter().any(|row| {
                        row.glyphs
                            .iter()
                            .any(|glyph| !glyph.text.chars().all(char::is_whitespace))
                    })
                })
            })
            || view.children.iter().any(|&child| holds_text(tree, child))
    })
}

/// Whether the app drew `view` as a box: a border, or an opaque fill unlike the
/// `background` it sits on.
fn has_box(view: &crate::view::NshView, background: Option<nsh_core::Color>) -> bool {
    let border = view
        .animated
        .border_color
        .map_or(view.style.borders.colors, |color| [color; 4]);
    let widths = &view.style.borders.widths;
    let bordered = [widths.top, widths.right, widths.bottom, widths.left]
        .iter()
        .zip(border)
        .any(|(&width, color)| width > Points::ZERO && !color.is_transparent());
    let fill = view
        .animated
        .background
        .unwrap_or(view.style.background_color);
    bordered || (fill.0 & 255 == 255 && Some(fill) != background)
}

/// Collects the Cell rows `id`'s subtree draws text glyphs on, up to two;
/// `false` when it holds content other than text and decorative icons.
fn text_rows(
    tree: &Mounter,
    id: NodeId,
    projection: Projection,
    cells: CellSize,
    area: Rect,
    rows: &mut Vec<i32>,
) -> bool {
    let Some(view) = tree.get(id) else {
        return true;
    };
    if view.concealed
        || view.style.hidden
        || view.kind == ViewKind::Modal
        || view.animated.opacity.unwrap_or(view.style.opacity) < INVISIBLE_OPACITY
    {
        return true;
    }
    let projection = projection.view(view);
    let content = match view.kind {
        ViewKind::View | ViewKind::Text => view.component.is_some(),
        _ => !is_decorative(view),
    };
    if content {
        return false;
    }
    if let Some((layout, text)) = &view.text
        && let Some(layout) = layout.downcast::<CellLayout>()
    {
        for (index, row) in layout.rows.iter().enumerate() {
            if row
                .glyphs
                .iter()
                .all(|glyph| glyph.text.chars().all(char::is_whitespace))
            {
                continue;
            }
            let bounds = projection.bounds(Bounds::new(
                text.origin + Point::new(Points::ZERO, cells.height() * index as f32),
                nsh_core::Size::new(text.size.width, cells.height()),
            ));
            let top = CellBounds::new(bounds, cells, area).top;
            if !rows.contains(&top) {
                rows.push(top);
            }
            if rows.len() > 1 {
                return true;
            }
        }
    }
    let offset = view
        .scroll
        .as_ref()
        .map_or_else(Point::default, |scroll| scroll.origin());
    let projection = projection.translated(Point::new(-offset.x, -offset.y));
    view.children
        .iter()
        .all(|&child| rows.len() > 1 || text_rows(tree, child, projection, cells, area, rows))
}

/// How far each child of `id` moves down so that consecutive siblings of
/// equal height stacked in a column, at an equal point pitch, keep one Cell
/// pitch and one Cell height: the first keeps its rounded row, and each
/// next one starts the pitch's rounded rows below it, on a Cell edge. Others
/// keep their absolute edges.
fn even_pitch(tree: &Mounter, id: NodeId, projection: Projection, cells: CellSize) -> Vec<Points> {
    const SAME: f32 = 0.01;
    let Some(view) = tree.get(id) else {
        return Vec::new();
    };
    let frames: Vec<_> = view
        .children
        .iter()
        .map(|&child| {
            tree.get(child)
                .filter(|child| {
                    !child.concealed && !child.style.hidden && child.kind != ViewKind::Modal
                })
                .map(|child| {
                    projection
                        .view(child)
                        .bounds(Bounds::new(Point::default(), child.frame.size))
                })
                .filter(|frame| frame.size.height > Points::ZERO)
        })
        .collect();
    let mut deltas = vec![Points::ZERO; frames.len()];
    let height = cells.height();
    let mut start = 0;
    while start < frames.len() {
        let Some(first) = frames[start] else {
            start += 1;
            continue;
        };
        let mut pitch = None;
        let mut end = start + 1;
        while let Some(Some(next)) = frames.get(end) {
            let previous = frames[end - 1].expect("a frame of the run");
            let step = next.top() - previous.top();
            if (next.size.height - first.size.height).abs().0 > SAME
                || (next.left() - first.left()).abs().0 > SAME
                || next.top() < previous.bottom() - Points(SAME)
                || pitch.is_some_and(|pitch: Points| (step - pitch).abs().0 > SAME)
            {
                break;
            }
            pitch = Some(step);
            end += 1;
        }
        if let Some(pitch) = pitch {
            let first_row = (first.top() / height + 0.5).floor();
            let pitch_rows = (pitch / height + 0.5).floor();
            for (offset, index) in (start..end).enumerate() {
                let top = frames[index].expect("a frame of the run").top();
                deltas[index] = height * (first_row + offset as f32 * pitch_rows) - top;
            }
        }
        start = end;
    }
    deltas
}

fn hairline_fill(
    view: &crate::view::NshView,
    size: nsh_core::Size<Points>,
    cells: CellSize,
) -> Option<&'static str> {
    if view.kind != ViewKind::View || !view.children.is_empty() || view.component.is_some() {
        return None;
    }
    let horizontal =
        size.height > Points::ZERO && size.height < cells.height() * HAIRLINE_CELL_FRACTION;
    let vertical = size.width > Points::ZERO && size.width < cells.width() * HAIRLINE_CELL_FRACTION;
    let fill = view
        .animated
        .background
        .unwrap_or(view.style.background_color);
    if fill.is_transparent() || !(horizontal || vertical) {
        return None;
    }
    Some(
        if vertical && (!horizontal || size.height / cells.height() >= size.width / cells.width()) {
            "│"
        } else {
            "─"
        },
    )
}

fn border_has_sibling_content(
    tree: &Mounter,
    id: NodeId,
    projection: Projection,
    cells: CellSize,
    area: Rect,
) -> bool {
    let Some(view) = tree.get(id) else {
        return false;
    };
    if view.kind != ViewKind::View
        || !view.children.is_empty()
        || (view.style.borders.widths.top <= Points::ZERO
            && view.style.borders.widths.bottom <= Points::ZERO)
    {
        return false;
    }
    let bounds = projection
        .view(view)
        .bounds(Bounds::new(Point::default(), view.frame.size));
    let projected = CellBounds::new(bounds, cells, area);
    if projected.bottom - projected.top != 2 {
        return false;
    }
    let Some(parent) = view.parent.and_then(|id| tree.get(id)) else {
        return false;
    };
    let Some(index) = parent.children.iter().position(|&child| child == id) else {
        return false;
    };
    let transparent = view
        .animated
        .background
        .unwrap_or(view.style.background_color)
        .is_transparent()
        && view
            .style
            .effects
            .as_ref()
            .is_none_or(|effects| effects.background_images.is_empty());
    parent
        .children
        .iter()
        .enumerate()
        .any(|(position, &child)| {
            child != id
                && (transparent || position > index)
                && subtree_content_intersects(tree, child, projection, cells, bounds)
        })
}

/// Whether a view's content reaches into the columns, and into the rows, of
/// its box that the borders it is fitted into reserve.
#[derive(Clone, Copy, Default)]
struct Reach {
    columns: bool,
    rows: bool,
}

/// Where `id`'s content lies against the Cells the borders it is fitted
/// into reserve: the box moves off them with its content only where its
/// content reaches into them, and otherwise only narrows.
fn reserved_reach(
    tree: &Mounter,
    id: NodeId,
    projection: Projection,
    cells: CellSize,
    area: Rect,
    clip: Clip,
) -> Reach {
    let Some(view) = tree.get(id) else {
        return Reach::default();
    };
    let raw = CellBounds::new(
        projection
            .view(view)
            .bounds(Bounds::new(Point::default(), view.frame.size)),
        cells,
        area,
    );
    let reaches = |strips: [CellBounds; 2]| {
        strips.into_iter().any(|strip| {
            strip.right > strip.left
                && strip.bottom > strip.top
                && subtree_content_intersects(
                    tree,
                    id,
                    projection,
                    cells,
                    strip.points(cells, area),
                )
        })
    };
    Reach {
        columns: reaches([
            CellBounds {
                right: clip.borders.left.min(raw.right),
                ..raw
            },
            CellBounds {
                left: clip.borders.right.max(raw.left),
                ..raw
            },
        ]),
        rows: reaches([
            CellBounds {
                bottom: clip.borders.top.min(raw.bottom),
                ..raw
            },
            CellBounds {
                top: clip.borders.bottom.max(raw.top),
                ..raw
            },
        ]),
    }
}

/// The top and bottom borders of `id` that are left out because a child
/// whose content reaches into the border's row cannot move off it without
/// landing on a sibling's content: stacking two rows of text on one is
/// worse than a missing rule.
#[derive(Clone, Copy, Default)]
struct BlockedRows {
    top: bool,
    bottom: bool,
}

fn blocked_border_rows(
    tree: &Mounter,
    id: NodeId,
    projection: Projection,
    cells: CellSize,
    area: Rect,
) -> BlockedRows {
    let Some(view) = tree.get(id) else {
        return BlockedRows::default();
    };
    let widths = &view.style.borders.widths;
    if widths.top <= Points::ZERO && widths.bottom <= Points::ZERO {
        return BlockedRows::default();
    }
    let projection = projection.view(view);
    let raw = CellBounds::new(
        projection.bounds(Bounds::new(Point::default(), view.frame.size)),
        cells,
        area,
    );
    let offset = view
        .scroll
        .as_ref()
        .map_or_else(Point::default, |scroll| scroll.origin());
    let projection = projection.translated(Point::new(-offset.x, -offset.y));
    let children: Vec<(NodeId, CellBounds)> = view
        .children
        .iter()
        .filter_map(|&child| {
            let shown = tree.get(child)?;
            if shown.concealed || shown.style.hidden || shown.kind == ViewKind::Modal {
                return None;
            }
            let frame = projection
                .view(shown)
                .bounds(Bounds::new(Point::default(), shown.frame.size));
            Some((child, CellBounds::new(frame, cells, area)))
        })
        .collect();
    let row = |bounds: CellBounds, y: i32| CellBounds {
        top: y,
        bottom: y + 1,
        ..bounds
    };
    // A child reaching into the border's `reserved` row moves one row
    // toward `step`; the row it newly covers must hold no sibling's content.
    let blocked = |reserved: i32, step: i32| {
        children.iter().any(|&(child, bounds)| {
            if !(bounds.top..bounds.bottom).contains(&reserved)
                || !subtree_content_intersects(
                    tree,
                    child,
                    projection,
                    cells,
                    row(bounds, reserved).points(cells, area),
                )
            {
                return false;
            }
            let covered = if step > 0 {
                bounds.bottom
            } else {
                bounds.top - 1
            };
            if !(raw.top..raw.bottom).contains(&covered) {
                return false;
            }
            let covered = row(bounds, covered).points(cells, area);
            children.iter().any(|&(sibling, _)| {
                sibling != child
                    && subtree_content_intersects(tree, sibling, projection, cells, covered)
            })
        })
    };
    BlockedRows {
        top: widths.top > Points::ZERO && blocked(raw.top, 1),
        bottom: widths.bottom > Points::ZERO && blocked(raw.bottom - 1, -1),
    }
}

fn subtree_content_intersects(
    tree: &Mounter,
    id: NodeId,
    projection: Projection,
    cells: CellSize,
    bounds: Bounds<Points>,
) -> bool {
    let Some(view) = tree.get(id) else {
        return false;
    };
    if view.concealed
        || view.style.hidden
        || view.kind == ViewKind::Modal
        || view.animated.opacity.unwrap_or(view.style.opacity) < INVISIBLE_OPACITY
    {
        return false;
    }
    let projection = projection.view(view);
    if let Some((layout, content)) = &view.text
        && let Some(layout) = layout.downcast::<CellLayout>()
    {
        for (row_index, row) in layout.rows.iter().enumerate() {
            let mut column = row.indent;
            for glyph in &row.glyphs {
                let glyph_bounds = Bounds::new(
                    content.origin
                        + Point::new(
                            cells.width() * column as f32,
                            cells.height() * row_index as f32,
                        ),
                    nsh_core::Size::new(cells.width() * glyph.width as f32, cells.height()),
                );
                if !glyph.text.chars().all(char::is_whitespace)
                    && projection.bounds(glyph_bounds).intersects(&bounds)
                {
                    return true;
                }
                column += glyph.width;
            }
        }
    }
    if (view.input.is_some() || view.canvas.is_some() || view.component.is_some())
        && projection
            .bounds(Bounds::new(Point::default(), view.frame.size))
            .intersects(&bounds)
    {
        return true;
    }
    let offset = view
        .scroll
        .as_ref()
        .map_or_else(Point::default, |scroll| scroll.origin());
    let projection = projection.translated(Point::new(-offset.x, -offset.y));
    view.children
        .iter()
        .any(|&child| subtree_content_intersects(tree, child, projection, cells, bounds))
}

fn horizontal_text_parent(tree: &Mounter, id: NodeId, projection: Projection) -> Option<NodeId> {
    let view = tree.get(id)?;
    view.text.as_ref()?;
    let parent = view.parent?;
    let siblings = &tree.get(parent)?.children;
    if siblings.len() < 2 {
        return None;
    }
    let bounds = projection
        .view(view)
        .bounds(Bounds::new(Point::default(), view.frame.size));
    siblings
        .iter()
        .any(|&sibling| {
            sibling != id
                && tree.get(sibling).is_some_and(|sibling| {
                    if sibling.text.is_none() {
                        return false;
                    }
                    let other = projection
                        .view(sibling)
                        .bounds(Bounds::new(Point::default(), sibling.frame.size));
                    bounds.top() < other.bottom() && other.top() < bounds.bottom()
                })
        })
        .then_some(parent)
}

// A vertical stack of terminal text must keep one Cell per measured line,
// even when its enclosing box contracted to reserve an ancestor's border.
fn text_stack(tree: &Mounter, id: NodeId) -> Option<(Bounds<Points>, i32)> {
    let view = tree.get(id)?;
    if view.children.len() < 2 {
        return None;
    }
    let mut rows = 0;
    let mut top = None;
    let mut bottom = Points::ZERO;
    for child in &view.children {
        let child = tree.get(*child)?;
        let (layout, _) = child.text.as_ref()?;
        if child.style.hidden
            || child.concealed
            || child.frame.top() < bottom
            || child.frame.bottom() > view.frame.size.height
        {
            return None;
        }
        top.get_or_insert(child.frame.top());
        bottom = child.frame.bottom();
        rows += layout.downcast::<CellLayout>()?.rows.len() as i32;
    }
    let top = top?;
    Some((
        Bounds::new(
            Point::new(Points::ZERO, top),
            nsh_core::Size::new(view.frame.size.width, bottom - top),
        ),
        rows,
    ))
}

// Positive sub-Cell fills can quantize onto a glyph they do not cover in
// points. Preserve that glyph while retaining fills that really overlap it.
fn disjoint_text_cells(
    geometry: &[Geometry],
    buffer: &Buffer,
    frame: Bounds<Points>,
    visible: Rect,
    cells: CellSize,
    area: Rect,
) -> Vec<bool> {
    let mut protected = vec![false; usize::from(visible.width) * usize::from(visible.height)];
    for geometry in geometry {
        if let (Some(points), Some(text)) = (geometry.text_points, geometry.text_frame)
            && !points.intersects(&frame)
        {
            let text_cells =
                CellBounds::new(text, cells, area).clipped(geometry.visible.intersection(visible));
            for y in text_cells.y..text_cells.bottom() {
                for x in text_cells.x..text_cells.right() {
                    let symbol = buffer[(x, y)].symbol();
                    if symbol != " " {
                        for column in x..x
                            .saturating_add(symbol.width().max(1) as u16)
                            .min(text_cells.right())
                        {
                            protected[usize::from(y - visible.y) * usize::from(visible.width)
                                + usize::from(column - visible.x)] = true;
                        }
                    }
                }
            }
        }
    }
    protected
}

/// Draws `fill` on the `visible` Cells it covers; each replaces what the
/// Cell held.
fn fill_cells(buffer: &mut Buffer, visible: Rect, fill: Fill, covered: impl Fn(u16, u16) -> bool) {
    for y in visible.y..visible.bottom() {
        for x in visible.x..visible.right() {
            if !covered(x, y) {
                continue;
            }
            let cell = &mut buffer[(x, y)];
            cell.reset();
            match fill {
                Fill::Line(line, style) => {
                    cell.set_symbol(line).set_style(style);
                }
                Fill::Solid(style) => {
                    cell.set_style(style.add_modifier(Modifier::REVERSED));
                }
                Fill::Clear => {}
            }
        }
    }
}
