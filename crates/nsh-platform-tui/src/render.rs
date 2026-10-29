use std::{collections::HashMap, sync::Arc};

use nsh_core::{
    AccessibilityRole, Bounds, Checked, NodeId, Point, PointerEvents, Points, ROOT_ID, ViewKind,
    platform::ColorScheme,
};
use ratatui_core::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};
use ratatui_widgets::{block::Block, borders::Borders, paragraph::Paragraph};
use unicode_segmentation::UnicodeSegmentation;
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
/// Image placeholders smaller than this, in Cells, are decorative and not drawn.
const MIN_IMAGE_CELLS: (i32, i32) = (3, 2);
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
    /// The Cells a canvas drew, by buffer index: a reversed view around them
    /// leaves them as drawn.
    canvas_cells: Vec<bool>,
}

/// Below this opacity, multiplied through its ancestors, a view draws
/// nothing; above it opacity is ignored, as the palette has no translucency.
const INVISIBLE_OPACITY: f32 = 0.03;

/// Draws the tree in the terminal's look (`palette`): colors from the
/// terminal's palette in `scheme`, no fill colors, and the `focused` view,
/// selected views and checked views reversed over their Cells.
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
    };
    let canvas_cells = vec![false; buffer.area.area() as usize];
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
        canvas_cells,
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
        let button = button_label(
            self.tree,
            id,
            parent,
            self.cells,
            self.area,
            self.background,
        );
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
            bounds.fitted(clip.borders)
        } else {
            bounds.constrained(clip.borders)
        };
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
            bounds.top = top.min(bounds.bottom);
        }
        if clip.fit {
            projection = projection.snapped(Point::new(
                self.cells.width() * (bounds.left - raw_bounds.left) as f32,
                Points::ZERO,
            ));
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
                && button.is_none()
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
        let role = view.props.accessibility.role;
        let decorative = match view.kind {
            ViewKind::Canvas => {
                role == AccessibilityRole::Image || view.props.accessibility.label.is_none()
            }
            ViewKind::Image => {
                bounds.right - bounds.left < MIN_IMAGE_CELLS.0
                    || bounds.bottom - bounds.top < MIN_IMAGE_CELLS.1
            }
            _ => false,
        };
        let visible_drawn = !visible.is_empty() && opacity >= INVISIBLE_OPACITY && !decorative;
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
            let fill = if button.is_some() {
                nsh_core::Color::TRANSPARENT
            } else {
                view.animated
                    .background
                    .unwrap_or(view.style.background_color)
            };
            let rounded = !radii.is_zero();
            let leaf =
                view.kind == ViewKind::View && view.children.is_empty() && view.component.is_none();
            let fill = if fill.is_transparent() {
                None
            } else if let Some(line) = line {
                Some(Fill::Line(line, ink(fill, outer_ground)))
            } else if fill.0 & 255 < 255 {
                None
            } else if leaf {
                Some(Fill::Solid(ink(fill, outer_ground)))
            } else {
                Some(Fill::Clear)
            };
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
            if let Some(fill) = fill {
                fill_cells(self.buffer, visible, fill, &covered);
                for y in visible.y..visible.bottom() {
                    for x in visible.x..visible.right() {
                        if covered(x, y) {
                            self.canvas_cells[self.buffer.index_of(x, y)] = false;
                        }
                    }
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
                let buffer_area = self.buffer.area;
                let canvas_cells = &mut self.canvas_cells;
                crate::canvas::draw(
                    canvas,
                    self.buffer,
                    bounds,
                    inner.clipped(clip.visible),
                    self.cells,
                    projection.snapped(Point::new(-origin.x, -origin.y)),
                    ground,
                    |x, y| {
                        let index = usize::from(y - buffer_area.y) * usize::from(buffer_area.width)
                            + usize::from(x - buffer_area.x);
                        canvas_cells[index] = true;
                    },
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
                let title = view
                    .component_name
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", view.kind));
                Block::bordered().title(title).render(visible, self.buffer);
            }
        }
        let scroll_origin = view
            .scroll
            .as_ref()
            .map_or_else(Point::default, |scroll| scroll.origin());
        let child_projection =
            projection.translated(Point::new(-scroll_origin.x, -scroll_origin.y));
        let children = view.children.len();
        for index in 0..children {
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
                child_projection,
                children_clip,
                opacity,
                children_pointer,
                press,
            );
        }
        for (position, cell) in clipped_corners {
            self.buffer[position] = cell;
        }
        self.background = outer_background;
        if visible_drawn && let Some(button) = button {
            self.brackets(visible, button, own_geometry);
        }
        let Some(view) = self.tree.get(id) else {
            return;
        };
        let state = &view.props.accessibility.state;
        let reversed = state.selected
            || state.checked == Some(Checked::True)
            || (self.focused == Some(id) && view.input.is_none());
        if reversed && visible_drawn {
            for y in visible.y..visible.bottom() {
                for x in visible.x..visible.right() {
                    if !self.canvas_cells[self.buffer.index_of(x, y)] {
                        self.buffer[(x, y)].modifier.insert(Modifier::REVERSED);
                    }
                }
            }
        }
    }
}

impl Painter<'_> {
    /// Draws a button's brackets in its own Cells: around its one row of text
    /// (the text views from `texts` on in `geometry`), one Cell away when the
    /// box has room for both, or around its accessibility label on the box's
    /// middle row, cut to the box.
    fn brackets(&mut self, visible: Rect, button: ButtonLabel, texts: usize) {
        match button {
            ButtonLabel::Text => {
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
                let Some((y, left, right)) = label else {
                    return;
                };
                let gap = BRACKET_GAP as u16;
                let open = left.saturating_sub(gap + 1).max(visible.x);
                let close = (right + gap).min(visible.right() - 1);
                let free = |x: u16| self.buffer[(x, y)].symbol() == " ";
                // Both brackets or neither: a label flush with its box stays bare.
                if open < left && close >= right && free(open) && free(close) {
                    let style = self.buffer[(left, y)].style();
                    self.buffer[(open, y)].set_symbol("[").set_style(style);
                    self.buffer[(close, y)].set_symbol("]").set_style(style);
                }
            }
            ButtonLabel::Accessible(label) => {
                let width = usize::from(visible.width);
                let mut text = format!("[ {label} ]");
                if text.width() > width {
                    let mut kept = String::new();
                    for grapheme in label.graphemes(true) {
                        if kept.width() + grapheme.width() + "[…]".width() > width {
                            break;
                        }
                        kept.push_str(grapheme);
                    }
                    text = format!("[{kept}…]");
                }
                if text.width() > width {
                    return;
                }
                let x = visible.x + ((width - text.width()) / 2) as u16;
                let y = visible.y + (visible.height - 1) / 2;
                self.buffer.set_string(x, y, text, Style::reset());
            }
        }
    }
}

/// What a button shows between brackets: a pressable view (role button, or no
/// role and press handlers) that the app drew as a box (a border, or an opaque
/// fill unlike the `background` it sits on), whose text sits on one row, or
/// whose content is only icons and which has an accessibility label. List
/// items without a box, selected or checked views (shown reversed) and other
/// content (fields, labeled canvases, images, native components) keep their
/// look.
fn button_label(
    tree: &Mounter,
    id: NodeId,
    projection: Projection,
    cells: CellSize,
    area: Rect,
    background: Option<nsh_core::Color>,
) -> Option<ButtonLabel> {
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
    if !bordered && (fill.0 & 255 < 255 || Some(fill) == background) {
        return None;
    }
    let mut rows = Vec::new();
    if !text_rows(tree, id, projection, cells, area, &mut rows) {
        return None;
    }
    match rows.len() {
        0 => view
            .props
            .accessibility
            .label
            .clone()
            .map(ButtonLabel::Accessible),
        1 => Some(ButtonLabel::Text),
        _ => None,
    }
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
    let frame = CellBounds::new(
        projection.bounds(Bounds::new(Point::default(), view.frame.size)),
        cells,
        area,
    );
    let content = match view.kind {
        ViewKind::View | ViewKind::Text => view.component.is_some(),
        ViewKind::Canvas => view.props.accessibility.label.is_some(),
        ViewKind::Image => {
            frame.right - frame.left >= MIN_IMAGE_CELLS.0
                && frame.bottom - frame.top >= MIN_IMAGE_CELLS.1
        }
        _ => true,
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
