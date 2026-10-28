use nsh_core::{Bounds, NodeId, Point, PointerEvents, Points, ROOT_ID, ViewKind};
use ratatui_core::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};
use ratatui_widgets::{block::Block, borders::Borders};
use unicode_width::UnicodeWidthStr;

use crate::{
    CellSize, animation::Projection, geometry::CellBounds, mount::Mounter, text::CellLayout,
};

const COMPACT_HEIGHT_CELLS: f32 = 1.5;
const MIN_BORDERED_EXTENT: i32 = 3;

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

struct Outline {
    page: Color,
    stroke: CellBounds,
    content: CellBounds,
    clip: Rect,
    color: nsh_core::Color,
    opacity: f32,
}

struct Painter<'a> {
    outlines: Vec<Outline>,
    tree: &'a mut Mounter,
    buffer: &'a mut Buffer,
    hits: &'a mut Vec<Hit>,
    geometry: &'a mut Vec<Geometry>,
    area: Rect,
    cells: CellSize,
    page_backgrounds: &'a [nsh_core::Color],
    /// The color of the page under the views being drawn, which the Cells
    /// drawn with the terminal's background stand for when colors blend.
    page: Color,
}

/// Below this opacity a view draws nothing: blending changes no channel by
/// more than 8 of 255, while a border or glyph would still replace the
/// glyph under it.
const INVISIBLE_OPACITY: f32 = 0.03;

pub(crate) fn draw(
    tree: &mut Mounter,
    buffer: &mut Buffer,
    area: Rect,
    cells: CellSize,
    hits: &mut Vec<Hit>,
    geometry: &mut Vec<Geometry>,
) {
    hits.clear();
    geometry.clear();
    tree.collect_page_backgrounds();
    let clip = Clip {
        visible: area.intersection(buffer.area),
        borders: CellBounds::UNBOUNDED,
        fit: false,
    };
    let page_backgrounds = std::mem::take(&mut tree.page_backgrounds);
    let mut painter = Painter {
        outlines: Vec::new(),
        tree,
        buffer,
        hits,
        geometry,
        area,
        cells,
        page_backgrounds: &page_backgrounds,
        page: page_backgrounds
            .first()
            .map_or(Color::Rgb(0, 0, 0), |&page| rgb(page)),
    };
    painter.view(ROOT_ID, Projection::IDENTITY, clip, 1.0, true, None);
    painter.outlines();
    let modals = painter.tree.modals.clone();
    for modal in modals {
        if crate::navigation::modal_visible(painter.tree, modal) {
            painter.view(modal, Projection::IDENTITY, clip, 1.0, true, None);
            painter.outlines();
        }
    }
    tree.page_backgrounds = page_backgrounds;
}

impl Painter<'_> {
    fn view(
        &mut self,
        id: NodeId,
        parent: Projection,
        clip: Clip,
        opacity: f32,
        pointer: bool,
        press: Option<(NodeId, Point<Points>)>,
    ) {
        let text_stack = text_stack(self.tree, id);
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
        let compact = view.kind == ViewKind::Canvas || !view.style.radii().is_zero();
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
        let mut projection = parent.view(view);
        let local_frame = Bounds::new(Point::default(), view.frame.size);
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
        let outline = view.style.outline;
        let outline = outline.is_visible().then(|| {
            let outset = outline.offset + outline.width;
            let stroke = CellBounds::new(
                projection.bounds(Bounds::new(
                    Point::new(-outset, -outset),
                    nsh_core::Size::new(
                        view.frame.size.width + outset * 2.0,
                        view.frame.size.height + outset * 2.0,
                    ),
                )),
                self.cells,
                self.area,
            );
            (stroke, outline.color)
        });
        let children_pointer = pointer && view.props.pointer_events != PointerEvents::BoxOnly;
        let widths = &view.style.borders.widths;
        let colors = view
            .animated
            .border_color
            .map_or(view.style.borders.colors, |color| [color; 4]);
        let sides = [widths.top, widths.right, widths.bottom, widths.left]
            .map(|width| width > Points::ZERO);
        let has_content =
            view.kind != ViewKind::View || !view.children.is_empty() || view.component.is_some();
        let sides = std::array::from_fn::<_, 4, _>(|i| {
            let extent = if i % 2 == 0 {
                bounds.bottom - bounds.top
            } else {
                bounds.right - bounds.left
            };
            let has_room =
                extent >= MIN_BORDERED_EXTENT || (i % 2 == 0 && extent == 2 && !has_content);
            sides[i] && !colors[i].is_transparent() && has_room
        });
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
        let outer_page = self.page;
        if !visible.is_empty() && opacity >= INVISIBLE_OPACITY {
            let fill = view
                .animated
                .background
                .unwrap_or(view.style.background_color);
            let rounded = !radii.is_zero();
            let has_fill = !fill.is_transparent()
                || view
                    .style
                    .effects
                    .as_ref()
                    .is_some_and(|effects| !effects.background_images.is_empty());
            let protected = if has_fill
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
            fill_cells(
                self.buffer,
                self.page_backgrounds,
                self.page,
                visible,
                fill,
                opacity,
                covered,
            );
            if self.page_backgrounds.contains(&fill) {
                self.page = rgb(fill);
            }
            // A Cell holds one background color: each gradient layer draws its
            // middle stop, as canvas gradients do, bottom layer first.
            let gradients = view
                .style
                .effects
                .as_ref()
                .map_or(&[][..], |effects| &effects.background_images[..]);
            for gradient in gradients.iter().rev() {
                if let Some(stop) = gradient.stops.get(gradient.stops.len() / 2) {
                    fill_cells(
                        self.buffer,
                        self.page_backgrounds,
                        self.page,
                        visible,
                        stop.color,
                        opacity,
                        covered,
                    );
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
                .border_set(ratatui_core::symbols::border::Set {
                    top_left: if radii.top_left > Points::ZERO {
                        "╭"
                    } else {
                        "┌"
                    },
                    top_right: if radii.top_right > Points::ZERO {
                        "╮"
                    } else {
                        "┐"
                    },
                    bottom_right: if radii.bottom_right > Points::ZERO {
                        "╯"
                    } else {
                        "┘"
                    },
                    bottom_left: if radii.bottom_left > Points::ZERO {
                        "╰"
                    } else {
                        "└"
                    },
                    ..ratatui_core::symbols::border::PLAIN
                })
                .render(visible, self.buffer);
            // Block chooses the corner symbols; each edge keeps its own wire color.
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
                for y in edge.y..edge.bottom() {
                    for x in edge.x..edge.right() {
                        let cell = &mut self.buffer[(x, y)];
                        cell.set_fg(blend(*color, opacity, cell.bg, self.page));
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
                if scroll == (0, 0) {
                    (&layout.paragraph).render(text_area, self.buffer);
                } else {
                    layout
                        .paragraph
                        .clone()
                        .scroll(scroll)
                        .render(text_area, self.buffer);
                }
                if view.animated.color.is_some() || opacity < 1.0 {
                    for y in text_area.y..text_area.bottom() {
                        for x in text_area.x..text_area.right() {
                            let cell = &mut self.buffer[(x, y)];
                            let color = view.animated.color.or_else(|| match cell.fg {
                                Color::Rgb(r, g, b) => Some(nsh_core::Color::rgba(r, g, b, 255)),
                                _ => None,
                            });
                            if let Some(color) = color {
                                cell.set_fg(blend(color, opacity, cell.bg, self.page));
                            }
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
                    opacity,
                    view.animated.color,
                    self.page,
                );
            }
            if let Some(canvas) = &view.canvas {
                crate::canvas::draw(
                    canvas,
                    self.buffer,
                    bounds,
                    inner.clipped(clip.visible),
                    self.cells,
                    opacity,
                    projection.snapped(Point::new(-origin.x, -origin.y)),
                    self.page,
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
        if let Some((stroke, color)) = outline
            && opacity >= INVISIBLE_OPACITY
        {
            self.outlines.push(Outline {
                stroke,
                content: bounds,
                clip: clip.borders.clipped(clip.visible),
                color,
                opacity,
                page: self.page,
            });
        }
        self.page = outer_page;
    }
}

impl Painter<'_> {
    fn outlines(&mut self) {
        for outline in std::mem::take(&mut self.outlines) {
            self.outline(
                outline.stroke,
                outline.content,
                outline.clip,
                outline.color,
                outline.opacity,
                outline.page,
            );
        }
    }

    /// Draws an outline after the view's content. A Cell cannot hold a stroke
    /// between two text rows, so an edge that would cross glyphs inside the
    /// view's box moves outward to the nearest free row or column, and the
    /// stroke never replaces a glyph that is already drawn.
    fn outline(
        &mut self,
        mut stroke: CellBounds,
        content: CellBounds,
        clip: Rect,
        color: nsh_core::Color,
        opacity: f32,
        page: Color,
    ) {
        let buffer = &*self.buffer;
        let glyph_at = |x: i32, y: i32| {
            let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                return false;
            };
            clip.contains(ratatui_core::layout::Position::new(x, y))
                && buffer[(x, y)].symbol() != " "
        };
        let inside_rows = content.top..content.bottom;
        let inside_columns = content.left..content.right;
        let columns = stroke.left.max(content.left)..stroke.right.min(content.right);
        let row_has_glyphs = |y: i32| columns.clone().any(|x| glyph_at(x, y));
        while inside_rows.contains(&stroke.top) && row_has_glyphs(stroke.top) {
            stroke.top -= 1;
        }
        while inside_rows.contains(&(stroke.bottom - 1)) && row_has_glyphs(stroke.bottom - 1) {
            stroke.bottom += 1;
        }
        let rows = stroke.top.max(content.top)..stroke.bottom.min(content.bottom);
        let column_has_glyphs = |x: i32| rows.clone().any(|y| glyph_at(x, y));
        while inside_columns.contains(&stroke.left) && column_has_glyphs(stroke.left) {
            stroke.left -= 1;
        }
        while inside_columns.contains(&(stroke.right - 1)) && column_has_glyphs(stroke.right - 1) {
            stroke.right += 1;
        }

        let area = stroke.clipped(clip);
        let closed = stroke.top == i32::from(area.y)
            && stroke.bottom == i32::from(area.bottom())
            && stroke.left == i32::from(area.x)
            && stroke.right == i32::from(area.right())
            && area.width >= 2
            && area.height >= 2
            && (area.x..area.right()).all(|x| {
                !glyph_at(i32::from(x), stroke.top) && !glyph_at(i32::from(x), stroke.bottom - 1)
            })
            && (area.y..area.bottom()).all(|y| {
                !glyph_at(stroke.left, i32::from(y)) && !glyph_at(stroke.right - 1, i32::from(y))
            });
        if !closed {
            let area = content.clipped(clip);
            for y in area.y..area.bottom() {
                for x in area.x..area.right() {
                    if i32::from(x) == content.left
                        || i32::from(x) + 1 == content.right
                        || i32::from(y) == content.top
                        || i32::from(y) + 1 == content.bottom
                    {
                        let cell = &mut self.buffer[(x, y)];
                        cell.set_fg(blend(color, opacity, cell.bg, page));
                    }
                }
            }
            return;
        }
        let top = stroke.top == i32::from(area.y);
        let bottom = stroke.bottom == i32::from(area.bottom());
        let left = stroke.left == i32::from(area.x);
        let right = stroke.right == i32::from(area.right());
        let set = ratatui_core::symbols::border::PLAIN;
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let on_top = top && y == area.y;
                let on_bottom = bottom && y + 1 == area.bottom();
                let on_left = left && x == area.x;
                let on_right = right && x + 1 == area.right();
                let symbol = match (on_top, on_bottom, on_left, on_right) {
                    (true, _, true, _) => set.top_left,
                    (true, _, _, true) => set.top_right,
                    (_, true, true, _) => set.bottom_left,
                    (_, true, _, true) => set.bottom_right,
                    (true, ..) => set.horizontal_top,
                    (_, true, ..) => set.horizontal_bottom,
                    (_, _, true, _) => set.vertical_left,
                    (_, _, _, true) => set.vertical_right,
                    _ => continue,
                };
                let cell = &mut self.buffer[(x, y)];
                if cell.symbol() == " " {
                    cell.set_symbol(symbol);
                    cell.set_fg(blend(color, opacity, cell.bg, page));
                }
            }
        }
    }
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

/// Fills the `visible` Cells with `fill`: page backgrounds with the terminal's
/// own, opaque fills replace what is there, translucent ones blend over it.
fn fill_cells(
    buffer: &mut Buffer,
    page_backgrounds: &[nsh_core::Color],
    page: Color,
    visible: Rect,
    fill: nsh_core::Color,
    opacity: f32,
    covered: impl Fn(u16, u16) -> bool,
) {
    if fill.is_transparent() {
        return;
    }
    let constant = if page_backgrounds.contains(&fill) {
        Some(Color::Reset)
    } else if fill.0 & 255 == 255 && opacity >= 1.0 {
        Some(rgb(fill))
    } else {
        None
    };
    for y in visible.y..visible.bottom() {
        for x in visible.x..visible.right() {
            if !covered(x, y) {
                continue;
            }
            let cell = &mut buffer[(x, y)];
            let background = constant.unwrap_or_else(|| blend(fill, opacity, cell.bg, page));
            cell.reset();
            cell.set_bg(background);
        }
    }
}

/// Blends `color` at `opacity` over a Cell's `background`; a Cell drawn with
/// the terminal's background shows the `page` color.
pub(crate) fn blend(color: nsh_core::Color, opacity: f32, background: Color, page: Color) -> Color {
    let packed = color.0;
    let alpha = (packed & 255) as f32 / 255.0 * opacity;
    let (r, g, b) = match (background, page) {
        (Color::Rgb(r, g, b), _) | (_, Color::Rgb(r, g, b)) => (r, g, b),
        _ => (0, 0, 0),
    };
    let channel = |shift: u32, base: u8| {
        (((packed >> shift) & 255) as f32 * alpha + f32::from(base) * (1.0 - alpha)).round() as u8
    };
    Color::Rgb(channel(24, r), channel(16, g), channel(8, b))
}

/// `color`'s red, green and blue as a Cell color.
fn rgb(color: nsh_core::Color) -> Color {
    let [r, g, b, _] = color.0.to_be_bytes();
    Color::Rgb(r, g, b)
}
