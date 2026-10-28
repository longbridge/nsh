use nsh_core::{Bounds, NodeId, Point, PointerEvents, Points, ROOT_ID, ViewKind};
use ratatui_core::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use ratatui_widgets::{
    block::Block,
    borders::{BorderType, Borders},
};

use crate::{
    CellSize, animation::Projection, geometry::CellBounds, mount::Mounter, text::CellLayout,
};

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
    pub(crate) scroll_origin: Point<Points>,
}

struct Painter<'a> {
    tree: &'a mut Mounter,
    buffer: &'a mut Buffer,
    hits: &'a mut Vec<Hit>,
    geometry: &'a mut Vec<Geometry>,
    area: Rect,
    cells: CellSize,
    page_backgrounds: &'a [nsh_core::Color],
}

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
    let clip = area.intersection(buffer.area);
    let page_backgrounds = std::mem::take(&mut tree.page_backgrounds);
    let mut painter = Painter {
        tree,
        buffer,
        hits,
        geometry,
        area,
        cells,
        page_backgrounds: &page_backgrounds,
    };
    painter.view(ROOT_ID, Projection::IDENTITY, clip, 1.0, true, None);
    let modals = painter.tree.modals.clone();
    for modal in modals {
        if crate::navigation::modal_visible(painter.tree, modal) {
            painter.view(modal, Projection::IDENTITY, clip, 1.0, true, None);
        }
    }
    tree.page_backgrounds = page_backgrounds;
}

impl Painter<'_> {
    fn view(
        &mut self,
        id: NodeId,
        parent: Projection,
        clip: Rect,
        opacity: f32,
        pointer: bool,
        press: Option<(NodeId, Point<Points>)>,
    ) {
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
        let projection = parent.view(view);
        let frame = projection.bounds(Bounds::new(Point::default(), view.frame.size));
        let bounds = CellBounds::new(frame, self.cells, self.area);
        let origin = bounds.origin(self.cells, self.area);
        let projection = projection.snapped(origin - frame.origin);
        let visible = bounds.clipped(clip);
        self.geometry.push(Geometry {
            id,
            bounds,
            visible,
            frame: Bounds::new(origin, frame.size),
            scroll_origin: view
                .scroll
                .as_ref()
                .map_or_else(Point::default, |scroll| scroll.origin()),
        });
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
        let children_clip = if view.style.overflow_hidden
            || matches!(
                view.kind,
                ViewKind::Scroll | ViewKind::Stack | ViewKind::Screen | ViewKind::Modal
            ) {
            visible
        } else {
            clip
        };
        if !visible.is_empty() {
            let fill = view
                .animated
                .background
                .unwrap_or(view.style.background_color);
            fill_cells(self.buffer, self.page_backgrounds, visible, fill, opacity);
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
                        visible,
                        stop.color,
                        opacity,
                    );
                }
            }
            let widths = &view.style.borders.widths;
            let colors = view
                .animated
                .border_color
                .map_or(view.style.borders.colors, |color| [color; 4]);
            let mut borders = Borders::NONE;
            if widths.top > Points::ZERO
                && !colors[0].is_transparent()
                && bounds.top == i32::from(visible.y)
            {
                borders |= Borders::TOP;
            }
            if widths.bottom > Points::ZERO
                && !colors[2].is_transparent()
                && bounds.bottom == i32::from(visible.bottom())
            {
                borders |= Borders::BOTTOM;
            }
            if widths.left > Points::ZERO
                && !colors[3].is_transparent()
                && bounds.left == i32::from(visible.x)
            {
                borders |= Borders::LEFT;
            }
            if widths.right > Points::ZERO
                && !colors[1].is_transparent()
                && bounds.right == i32::from(visible.right())
            {
                borders |= Borders::RIGHT;
            }
            Block::default()
                .borders(borders)
                .border_type(if view.style.border_radius > Points::ZERO {
                    BorderType::Rounded
                } else {
                    BorderType::Plain
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
                        cell.set_fg(blend(*color, opacity, cell.bg));
                    }
                }
            }
            if let Some((layout, content)) = &view.text
                && let Some(layout) = layout.downcast::<CellLayout>()
            {
                let text_bounds =
                    CellBounds::new(projection.bounds(*content), self.cells, self.area);
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
                                cell.set_fg(blend(color, opacity, cell.bg));
                            }
                        }
                    }
                }
            }
            if let (Some(field), Some(config)) = (&mut view.field, &view.input) {
                let content =
                    CellBounds::new(projection.bounds(field.content), self.cells, self.area);
                field.draw(
                    config,
                    content,
                    visible,
                    self.buffer,
                    opacity,
                    view.animated.color,
                );
            }
            if let Some(canvas) = &view.canvas {
                crate::canvas::draw(
                    canvas,
                    self.buffer,
                    bounds,
                    visible,
                    self.cells,
                    opacity,
                    projection.snapped(Point::new(-origin.x, -origin.y)),
                );
            }
            if let Some(component) = &mut view.component {
                component.render(visible, self.buffer);
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
        if let Some((stroke, color)) = outline {
            self.outline(stroke, bounds, clip, color, opacity);
        }
    }
}

impl Painter<'_> {
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
        while inside_columns.contains(&(stroke.right - 1)) && column_has_glyphs(stroke.right - 1)
        {
            stroke.right += 1;
        }

        let area = stroke.clipped(clip);
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
                    cell.set_fg(blend(color, opacity, cell.bg));
                }
            }
        }
    }
}

/// Fills the `visible` Cells with `fill`: page backgrounds with the terminal's
/// own, opaque fills replace what is there, translucent ones blend over it.
fn fill_cells(
    buffer: &mut Buffer,
    page_backgrounds: &[nsh_core::Color],
    visible: Rect,
    fill: nsh_core::Color,
    opacity: f32,
) {
    if fill.is_transparent() {
        return;
    }
    let page_background = page_backgrounds.contains(&fill);
    if page_background || (fill.0 & 255 == 255 && opacity >= 1.0) {
        let background = if page_background {
            Color::Reset
        } else {
            blend(fill, opacity, Color::Reset)
        };
        Block::default()
            .style(Style::reset().bg(background))
            .render(visible, buffer);
        for y in visible.y..visible.bottom() {
            for x in visible.x..visible.right() {
                buffer[(x, y)].set_symbol(" ");
            }
        }
    } else {
        for y in visible.y..visible.bottom() {
            for x in visible.x..visible.right() {
                let background = blend(fill, opacity, buffer[(x, y)].bg);
                buffer[(x, y)].set_symbol(" ");
                Block::default()
                    .style(Style::reset().bg(background))
                    .render(Rect::new(x, y, 1, 1), buffer);
            }
        }
    }
}

pub(crate) fn blend(color: nsh_core::Color, opacity: f32, background: Color) -> Color {
    let packed = color.0;
    let alpha = (packed & 255) as f32 / 255.0 * opacity;
    let (r, g, b) = match background {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    };
    let channel = |shift: u32, base: u8| {
        (((packed >> shift) & 255) as f32 * alpha + f32::from(base) * (1.0 - alpha)).round() as u8
    };
    Color::Rgb(channel(24, r), channel(16, g), channel(8, b))
}
