use nsh_core::{Bounds, NodeId, Point, PointerEvents, Points, ROOT_ID, ViewKind};
use ratatui_core::{
    buffer::{Buffer, Cell},
    layout::{Position, Rect},
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
    outlines: Outlines,
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
        outlines: Outlines::new(clip),
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
            if !fill.is_transparent() {
                self.outlines.clear(visible);
                let page_background = self.page_backgrounds.contains(&fill);
                if page_background || (fill.0 & 255 == 255 && opacity >= 1.0) {
                    let background = if page_background {
                        Color::Reset
                    } else {
                        blend(fill, opacity, Color::Reset)
                    };
                    Block::default()
                        .style(Style::reset().bg(background))
                        .render(visible, self.buffer);
                    for y in visible.y..visible.bottom() {
                        for x in visible.x..visible.right() {
                            self.buffer[(x, y)].set_symbol(" ");
                        }
                    }
                } else {
                    for y in visible.y..visible.bottom() {
                        for x in visible.x..visible.right() {
                            let background = blend(fill, opacity, self.buffer[(x, y)].bg);
                            self.buffer[(x, y)].set_symbol(" ");
                            Block::default()
                                .style(Style::reset().bg(background))
                                .render(Rect::new(x, y, 1, 1), self.buffer);
                        }
                    }
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
            let outline = view.style.outline;
            if outline.is_visible() {
                let outset = outline.offset + outline.width;
                let outline_bounds = CellBounds::new(
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
                self.outlines
                    .draw(outline_bounds, clip, outline.color, opacity, self.buffer);
            }
            if let Some((layout, content)) = &view.text
                && let Some(layout) = layout.downcast::<CellLayout>()
            {
                let text_bounds =
                    CellBounds::new(projection.bounds(*content), self.cells, self.area);
                let text_area = text_bounds.clipped(visible);
                self.outlines.protect(text_area, self.buffer);
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
                self.outlines.protect(content.clipped(visible), self.buffer);
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
                self.outlines.protect(visible, self.buffer);
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
                self.outlines.protect(visible, self.buffer);
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
                self.outlines.protect(visible, self.buffer);
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

struct Outlines {
    area: Rect,
    content: Vec<bool>,
    strokes: Vec<(Position, Cell, Cell)>,
}

impl Outlines {
    fn new(area: Rect) -> Self {
        Self {
            area,
            content: vec![false; usize::from(area.width) * usize::from(area.height)],
            strokes: Vec::new(),
        }
    }

    fn set_content(&mut self, area: Rect, content: bool) {
        for y in area.y..area.bottom() {
            let start = usize::from(y - self.area.y) * usize::from(self.area.width)
                + usize::from(area.x - self.area.x);
            self.content[start..start + usize::from(area.width)].fill(content);
        }
    }

    fn clear(&mut self, area: Rect) {
        self.set_content(area, false);
        self.strokes
            .retain(|(position, _, _)| !area.contains(*position));
    }

    fn protect(&mut self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        // Paragraphs and canvases leave some Cells untouched. Restore the background
        // under an outline there, without undoing anything painted since the stroke.
        self.strokes.retain(|(position, before, painted)| {
            if !area.contains(*position) {
                return true;
            }
            if buffer[*position] == *painted {
                buffer[*position] = before.clone();
            }
            false
        });
        self.set_content(area, true);
    }

    fn draw(
        &mut self,
        bounds: CellBounds,
        clip: Rect,
        color: nsh_core::Color,
        opacity: f32,
        buffer: &mut Buffer,
    ) {
        let area = bounds.clipped(clip);
        let symbols = ratatui_core::symbols::border::PLAIN;
        for y in area.y..area.bottom() {
            let top = i32::from(y) == bounds.top;
            let bottom = i32::from(y) + 1 == bounds.bottom;
            for x in area.x..area.right() {
                let left = i32::from(x) == bounds.left;
                let right = i32::from(x) + 1 == bounds.right;
                let symbol = match (top, bottom, left, right) {
                    (_, true, _, true) => symbols.bottom_right,
                    (_, true, true, _) => symbols.bottom_left,
                    (true, _, _, true) => symbols.top_right,
                    (true, _, true, _) => symbols.top_left,
                    (_, true, _, _) => symbols.horizontal_bottom,
                    (true, _, _, _) => symbols.horizontal_top,
                    (_, _, true, _) => symbols.vertical_left,
                    (_, _, _, true) => symbols.vertical_right,
                    _ => continue,
                };
                let position = Position::new(x, y);
                let index = usize::from(y - self.area.y) * usize::from(self.area.width)
                    + usize::from(x - self.area.x);
                if self.content[index] {
                    continue;
                }
                let cell = &mut buffer[position];
                if cell.symbol() != " " {
                    continue;
                }
                let before = cell.clone();
                let foreground = blend(color, opacity, cell.bg);
                cell.set_symbol(symbol).set_fg(foreground);
                self.strokes.push((position, before, cell.clone()));
            }
        }
    }
}
