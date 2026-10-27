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

use crate::{CellSize, geometry::CellBounds, mount::Mounter, text::CellLayout};

pub(crate) struct Hit {
    pub(crate) area: Rect,
    pub(crate) target: Option<NodeId>,
    pub(crate) origin: Point<Points>,
}

struct Painter<'a> {
    tree: &'a mut Mounter,
    buffer: &'a mut Buffer,
    hits: &'a mut Vec<Hit>,
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
) {
    hits.clear();
    let clip = area.intersection(buffer.area);
    let page_backgrounds = std::mem::take(&mut tree.page_backgrounds);
    Painter {
        tree,
        buffer,
        hits,
        area,
        cells,
        page_backgrounds: &page_backgrounds,
    }
    .view(ROOT_ID, Point::default(), clip, 1.0, true, None);
    tree.page_backgrounds = page_backgrounds;
}

impl Painter<'_> {
    fn view(
        &mut self,
        id: NodeId,
        parent: Point<Points>,
        clip: Rect,
        opacity: f32,
        pointer: bool,
        press: Option<(NodeId, Point<Points>)>,
    ) {
        let Some(view) = self.tree.get_mut(id) else {
            return;
        };
        let opacity = opacity * view.style.opacity;
        if view.style.hidden || opacity <= 0.0 {
            return;
        }
        let bounds = CellBounds::new(
            Bounds::new(parent + view.frame.origin, view.frame.size),
            self.cells,
            self.area,
        );
        let origin = bounds.origin(self.cells, self.area);
        let visible = bounds.clipped(clip);
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
                area: visible,
                target: press.map(|p| p.0),
                origin: press.map_or(origin, |p| p.1),
            });
        }
        let children_pointer = pointer && view.props.pointer_events != PointerEvents::BoxOnly;
        let children_clip = if view.style.overflow_hidden {
            visible
        } else {
            clip
        };
        if !visible.is_empty() {
            let fill = view.style.background_color;
            if !fill.is_transparent() {
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
            let mut borders = Borders::NONE;
            if widths.top > Points::ZERO
                && !view.style.borders.colors[0].is_transparent()
                && bounds.top == i32::from(visible.y)
            {
                borders |= Borders::TOP;
            }
            if widths.bottom > Points::ZERO
                && !view.style.borders.colors[2].is_transparent()
                && bounds.bottom == i32::from(visible.bottom())
            {
                borders |= Borders::BOTTOM;
            }
            if widths.left > Points::ZERO
                && !view.style.borders.colors[3].is_transparent()
                && bounds.left == i32::from(visible.x)
            {
                borders |= Borders::LEFT;
            }
            if widths.right > Points::ZERO
                && !view.style.borders.colors[1].is_transparent()
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
            for (side, color) in view.style.borders.colors.iter().enumerate() {
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
                let text_bounds = CellBounds::new(
                    Bounds::new(origin + content.origin, content.size),
                    self.cells,
                    self.area,
                );
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
            }
            if let Some(component) = &mut view.component {
                component.render(visible, self.buffer);
            } else if !matches!(view.kind, ViewKind::View | ViewKind::Text) {
                let title = view
                    .component_name
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", view.kind));
                Block::bordered().title(title).render(visible, self.buffer);
            }
        }
        let children = view.children.len();
        for index in 0..children {
            let child = self.tree.get(id).unwrap().children[index];
            self.view(
                child,
                origin,
                children_clip,
                opacity,
                children_pointer,
                press,
            );
        }
    }
}

fn blend(color: nsh_core::Color, opacity: f32, background: Color) -> Color {
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
