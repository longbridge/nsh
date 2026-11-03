//! A surface's selection uses the core range model and the exact glyphs painted on Cells.

use nsh_core::{
    Bounds, NodeId, Point, Points,
    selection::{SelectionModel, SelectionTexts},
};
use ratatui_core::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Modifier,
};

use crate::{
    CellSize, MouseButton, MouseInput, MouseKind,
    geometry::CellBounds,
    mount::Mounter,
    render::{Geometry, Hit},
    text::CellLayout,
};

/// What a selection reads of the last frame drawn: the surface's `area` and
/// Cell size, the views that take the pointer (`hits`), every view's Cells
/// (`geometry`) and the tree.
#[derive(Clone, Copy)]
pub(crate) struct Scene<'a> {
    pub(crate) area: Rect,
    pub(crate) cells: CellSize,
    pub(crate) hits: &'a [Hit],
    pub(crate) geometry: &'a [Geometry],
    pub(crate) tree: &'a Mounter,
}

#[derive(Default)]
pub(crate) struct Selection {
    root: Option<NodeId>,
    model: SelectionModel,
    dragging: bool,
    taking: bool,
    start: Point<Points>,
}

struct Texts<'a> {
    tree: &'a Mounter,
    geometry: Vec<&'a Geometry>,
}

impl<'a> Texts<'a> {
    fn new(tree: &'a Mounter, geometry: &'a [Geometry], root: NodeId) -> Self {
        let geometry = geometry
            .iter()
            .filter(|g| {
                crate::keyboard::contains(tree, root, g.id)
                    && tree.get(g.id).is_some_and(|view| {
                        view.text.is_some()
                            && view
                                .text_content
                                .as_ref()
                                .is_some_and(|text| text.selectable != Some(false))
                    })
            })
            .collect();
        Self { tree, geometry }
    }

    fn layout(&self, i: usize) -> &CellLayout {
        self.tree
            .get(self.geometry[i].id)
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .0
            .downcast::<CellLayout>()
            .unwrap()
    }
}

impl SelectionTexts for Texts<'_> {
    fn len(&self) -> usize {
        self.geometry.len()
    }
    fn key(&self, i: usize) -> u64 {
        u64::from(self.geometry[i].id)
    }
    fn content(&self, i: usize) -> &str {
        &self
            .tree
            .get(self.geometry[i].id)
            .unwrap()
            .text_content
            .as_ref()
            .unwrap()
            .text
    }
    fn frame(&self, i: usize) -> Bounds<Points> {
        self.geometry[i].text_frame.expect("text geometry")
    }
    fn offset_at(&self, i: usize, point: Point<Points>) -> u32 {
        let point = point - self.frame(i).origin;
        let glyphs = &self.layout(i).glyphs;
        let row = glyphs
            .iter()
            .min_by(|(a, _), (b, _)| {
                let distance = |bounds: &Bounds<Points>| {
                    (point.y - bounds.bottom())
                        .max(Points::ZERO)
                        .max(bounds.top() - point.y)
                        .0
                };
                distance(a).total_cmp(&distance(b))
            })
            .map(|(bounds, _)| bounds.top());
        let mut end = 0;
        for (bounds, range) in glyphs
            .iter()
            .filter(|(bounds, _)| Some(bounds.top()) == row)
        {
            if range.is_empty() {
                continue;
            }
            if point.x <= bounds.left() + bounds.size.width / 2.0 {
                return range.start;
            }
            end = range.end;
        }
        end
    }
}

impl Selection {
    /// A selection area holds the pointer's last selection.
    pub(crate) fn is_active(&self) -> bool {
        self.root.is_some()
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn handle(&mut self, mouse: MouseInput, scene: Scene<'_>, clicks: u32) -> bool {
        let Scene {
            area,
            cells,
            hits,
            geometry,
            tree,
        } = scene;
        let scope = crate::navigation::focus_scope(tree);
        if self
            .root
            .is_some_and(|root| !crate::keyboard::contains(tree, scope, root))
        {
            self.clear();
        }
        let position = Position::new(mouse.column, mouse.row);
        let point = Point::new(
            cells.width() * (f32::from(mouse.column) - f32::from(area.x) + 0.5),
            cells.height() * (f32::from(mouse.row) - f32::from(area.y) + 0.5),
        );
        if self.dragging
            && matches!(
                mouse.kind,
                MouseKind::Move | MouseKind::Up(MouseButton::Left)
            )
        {
            let delta = point - self.start;
            if !self.taking && delta.x.abs().max(delta.y.abs()) > nsh_core::input::PRESS_SLOP {
                self.taking = true;
            }
            if !self.taking {
                self.dragging = !matches!(mouse.kind, MouseKind::Up(_));
                return false;
            }
            if let Some(root) = self.root {
                let texts = Texts::new(tree, geometry, root);
                self.model.texts_changed(&texts);
                self.model.drag(&texts, point);
            }
            self.dragging = !matches!(mouse.kind, MouseKind::Up(_));
            return true;
        }
        if !matches!(mouse.kind, MouseKind::Down(MouseButton::Left)) {
            return false;
        }
        let hit = area
            .contains(position)
            .then(|| {
                hits.iter().rev().find(|hit| {
                    hit.area.contains(position) && crate::keyboard::contains(tree, scope, hit.node)
                })
            })
            .flatten();
        let mut root = None;
        if let Some(hit) = hit {
            let mut node = Some(hit.node);
            let mut excluded = false;
            while let Some(id) = node {
                let Some(view) = tree.get(id) else {
                    break;
                };
                if id == hit.node {
                    excluded = view
                        .text_content
                        .as_ref()
                        .is_some_and(|text| text.selectable == Some(false));
                    if view
                        .text_content
                        .as_ref()
                        .is_some_and(|text| text.is_selectable())
                    {
                        root = Some(id);
                    }
                }
                if view.props.selectable_view && !excluded {
                    root = Some(id);
                }
                node = if id == scope { None } else { view.parent };
            }
        }
        let Some(root) = root else {
            self.clear();
            return false;
        };
        if self.root != Some(root) {
            self.model.clear();
        }
        self.root = Some(root);
        let texts = Texts::new(tree, geometry, root);
        if texts.is_empty() {
            self.clear();
            return false;
        }
        self.model.texts_changed(&texts);
        self.model
            .press(&texts, point, clicks.max(1), mouse.modifiers.shift);
        self.dragging = true;
        self.start = point;
        self.taking = hit.is_none_or(|hit| hit.target.is_none());
        self.taking
    }

    pub(crate) fn key(
        &mut self,
        key: &str,
        modifiers: crate::Modifiers,
        tree: &Mounter,
        geometry: &[Geometry],
        clipboard: &dyn nsh_core::clipboard::ClipboardBackend,
    ) -> bool {
        let Some(root) = self.root else {
            return false;
        };
        if !crate::keyboard::contains(tree, crate::navigation::focus_scope(tree), root) {
            self.clear();
            return false;
        }
        let texts = Texts::new(tree, geometry, root);
        self.model.texts_changed(&texts);
        if key == "Escape" || (modifiers.ctrl && key == "g") {
            self.clear();
            return true;
        }
        if ((modifiers.ctrl || modifiers.cmd) && key.eq_ignore_ascii_case("c"))
            || (modifiers.ctrl && key == "Insert")
            || (modifiers.alt && key == "w")
        {
            if self.model.is_empty() {
                return false;
            }
            clipboard.set_text(Some(self.model.text(&texts)));
            return true;
        }
        if (modifiers.ctrl || modifiers.cmd) && key.eq_ignore_ascii_case("a") {
            self.model.select_all(&texts);
            return true;
        }
        // Any other key ends the selection and then acts as usual.
        self.clear();
        false
    }

    pub(crate) fn draw(&mut self, scene: Scene<'_>, buffer: &mut Buffer) {
        let Scene {
            area,
            cells,
            hits,
            geometry,
            tree,
        } = scene;
        let Some(root) = self.root else {
            return;
        };
        if !crate::keyboard::contains(tree, crate::navigation::focus_scope(tree), root) {
            self.clear();
            return;
        }
        let texts = Texts::new(tree, geometry, root);
        self.model.texts_changed(&texts);
        for (i, range) in self.model.ranges(&texts) {
            let origin = texts.frame(i).origin;
            let geometry = texts.geometry[i];
            for (bounds, glyph) in &texts.layout(i).glyphs {
                if glyph.start >= range.end || glyph.end <= range.start {
                    continue;
                }
                let rect = CellBounds::new(
                    Bounds::new(origin + bounds.origin, bounds.size),
                    cells,
                    area,
                )
                .clipped(geometry.visible.intersection(buffer.area));
                for y in rect.y..rect.bottom() {
                    for x in rect.x..rect.right() {
                        if hits
                            .iter()
                            .rev()
                            .find(|hit| hit.area.contains(Position::new(x, y)))
                            .is_some_and(|hit| hit.node == geometry.id)
                        {
                            buffer[(x, y)].set_style(
                                ratatui_core::style::Style::default()
                                    .add_modifier(Modifier::REVERSED),
                            );
                        }
                    }
                }
            }
        }
    }
}
