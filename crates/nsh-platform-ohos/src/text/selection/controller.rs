//! Surface-local text selection. UI callbacks consume owned geometry, never typography.
use std::{ops::Range, rc::Rc};

use ohos_arkui_binding::types::advanced::NodeDirtyFlag;
use ohos_drawing_binding::{Brush, Canvas, Point, Rect};

use crate::{
    interop::sdk::{Menu as ViewMenu, MenuAction as NativeViewAction, MenuItem as ViewAction},
    render::node::{Node, Result},
    text::{
        selection::layout::Layout,
        typography::{NativeText, argb},
    },
};

pub(crate) enum Command {
    Copy,
    SelectAll,
    Clear,
}
impl Node {
    fn selection_layout<T>(&self, f: impl FnOnce(&Layout) -> T) -> Option<T> {
        let raster = self.raster.borrow();
        let layout = raster
            .as_ref()?
            .0
            .downcast_ref::<NativeText>()?
            .selection
            .as_ref()?;
        Some(f(layout))
    }
    pub(crate) fn selectable(&self) -> bool {
        self.selection_layout(|l| l.len() > 0).unwrap_or(false)
    }
    fn selection_range(&self) -> Option<Range<usize>> {
        self.selection.range.get().map(|(start, end)| start..end)
    }
    pub(crate) fn clear_text_selection(&self) -> Result<()> {
        let old = self
            .selected_text
            .borrow_mut()
            .take()
            .and_then(|n| n.upgrade());
        if let Some(old) = old {
            old.selection_clear()?;
        }
        Ok(())
    }
    pub(crate) fn selection_clear(&self) -> Result<()> {
        let selected = self
            .selected_text
            .borrow()
            .as_ref()
            .is_some_and(|node| std::ptr::eq(node.as_ptr(), self));
        if selected {
            self.selected_text.borrow_mut().take();
        }
        if self.selection.range.take().is_some() {
            self.selection.invalidate_menu();
            *self.selection.geometry.borrow_mut() = Default::default();
            if let Some(route) = self.dispatcher.upgrade() {
                let mut keys = route.keyboard.lock().unwrap();
                if keys.selection == Some(self.id) {
                    keys.selection = None;
                }
            }
            self.selection.menu.set(false);
            self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
            self.update_wrapper()?;
        }
        Ok(())
    }
    pub(crate) fn selection_begin(self: &Rc<Self>, x: f32, y: f32) -> Result<bool> {
        if !self.visible_in_tree() || self.accessibility_state.spec.borrow().state.disabled {
            return Ok(false);
        }
        let content = self.content.get();
        let Some(Some(range)) = self.selection_layout(|l| l.word((x - content.x, y - content.y)))
        else {
            return Ok(false);
        };
        self.clear_text_selection()?;
        *self.selected_text.borrow_mut() = Some(Rc::downgrade(self));
        self.selection_set(range.clone())?;
        self.selection_menu_show(true)?;
        self.touches
            .borrow_mut()
            .selection_started(self, range, (x, y))?;
        Ok(true)
    }
    pub(crate) fn selection_set(&self, range: Range<usize>) -> Result<()> {
        if self.selection.range.get() == Some((range.start, range.end)) {
            return Ok(());
        }
        {
            let mut geometry = self.selection.geometry.borrow_mut();
            if self
                .selection_layout(|l| l.geometry(range.clone(), &mut geometry))
                .is_none()
            {
                return Ok(());
            }
        }
        self.selection.invalidate_menu();
        self.selection.range.set(Some((range.start, range.end)));
        if let Some(route) = self.dispatcher.upgrade() {
            route.keyboard.lock().unwrap().selection = Some(self.id);
        }
        self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
        Ok(())
    }
    pub(crate) fn selection_extend(&self, anchor: usize, point: (f32, f32)) -> Result<()> {
        let content = self.content.get();
        if let Some(Some(end)) =
            self.selection_layout(|l| l.boundary((point.0 - content.x, point.1 - content.y)))
        {
            // Keep an actual character selected while a handle crosses its anchor.
            if anchor != end {
                self.selection_set(anchor.min(end)..anchor.max(end))?;
            }
        }
        Ok(())
    }
    pub(crate) fn selection_handle(&self, point: (f32, f32)) -> Option<usize> {
        let range = self.selection_range()?;
        let handles = self.selection.geometry.borrow().handles?;
        let content = self.content.get();
        handles
            .into_iter()
            .enumerate()
            .filter_map(|(index, (x, top, bottom))| {
                let dx = point.0 - content.x - x;
                let dy = point.1 - content.y - (bottom + 5.0);
                (dx.abs() <= 22.0 && point.1 >= content.y + top - 12.0 && dy.abs() <= 28.0)
                    .then_some((
                        dx * dx + dy * dy,
                        if index == 0 { range.end } else { range.start },
                    ))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, anchor)| anchor)
    }
    pub(crate) fn selection_menu_show(&self, visible: bool) -> Result<()> {
        if visible && self.selection.range.get().is_some() {
            if !self.selection.open_menu() {
                return Ok(());
            }
        } else {
            self.selection.menu.set(false);
        }
        self.update_wrapper()
    }
    pub(crate) fn selection_menu(&self) -> Option<ViewMenu> {
        // A selectable block keeps its hidden menu; disabling selection removes its actions.
        if !self.selectable() {
            return None;
        }
        let content = self.content.get();
        let anchor = self
            .selection
            .geometry
            .borrow()
            .handles
            .map_or((0.0, 0.0), |h| (h[0].0, h[0].1));
        Some(ViewMenu {
            generation: self.selection.generation.get().max(1),
            visible: self.selection.menu.get()
                && self.selection.range.get().is_some()
                && !self.accessibility_state.spec.borrow().state.disabled
                && self.visible_in_tree(),
            x: (content.x + anchor.0).clamp(-16384.0, 16384.0) as f64,
            y: (content.y + anchor.1).clamp(-16384.0, 16384.0) as f64,
            items: {
                let mut items = vec![ViewAction {
                    kind: "action".into(),
                    name: "copy".into(),
                    label: "Copy".into(),
                }];
                let len = self
                    .raster
                    .borrow()
                    .as_ref()
                    .and_then(|layout| {
                        layout
                            .0
                            .downcast_ref::<crate::text::typography::NativeText>()
                    })
                    .and_then(|raster| raster.selection.as_ref())
                    .map_or(0, |layout| layout.len());
                if self.selection.range.get() != Some((0, len)) {
                    items.push(ViewAction {
                        kind: "action".into(),
                        name: "select-all".into(),
                        label: "Select All".into(),
                    });
                }
                items
            },
        })
    }
    pub(crate) fn selection_action(&self, event: &NativeViewAction) -> Result<bool> {
        if !self.visible_in_tree()
            || self.accessibility_state.spec.borrow().state.disabled
            || event.generation != Some(self.selection.generation.get())
        {
            return Ok(false);
        }
        if self.selection_range().is_none() {
            return Ok(false);
        }
        match event.kind.as_deref() {
            Some("menu-dismiss") => {
                self.selection.menu.set(false);
                self.update_wrapper()?;
            }
            Some("menu") if event.action == "copy" => {
                self.selection_command(Command::Copy)?;
            }
            Some("menu") if event.action == "select-all" => {
                self.selection_command(Command::SelectAll)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    pub(crate) fn selection_command(&self, command: Command) -> Result<()> {
        if !self.visible_in_tree() || self.accessibility_state.spec.borrow().state.disabled {
            return Ok(());
        }
        let Some(range) = self.selection_range() else {
            return Ok(());
        };
        match command {
            Command::Copy => {
                if let Some(text) = self.selection_layout(|l| l.copy(range)) {
                    self.host
                        .enqueue_clipboard(self.dispatcher.clone(), Some(text), None)?;
                    self.selection_clear()?;
                }
            }
            Command::SelectAll => {
                if let Some(len) = self.selection_layout(Layout::len) {
                    self.selection_set(0..len)?;
                    // The native menu dismisses after a choice. A subsequent tap reopens it.
                    self.selection_menu_show(false)?;
                }
            }
            Command::Clear => self.selection_clear()?,
        }
        Ok(())
    }
    pub(crate) fn draw_selection(&self, canvas: &Canvas, handles: bool) {
        if self.selection_range().is_none() {
            return;
        }
        let content = self.content.get();
        let color = self
            .raster
            .borrow()
            .as_ref()
            .and_then(|r| r.0.downcast_ref::<NativeText>())
            .and_then(|r| r.selection_color)
            .unwrap_or(nsh_core::Color(0x007aff55));
        let mut brush = Brush::new();
        brush.set_color(argb(if handles {
            nsh_core::Color(color.0 | 0xff)
        } else {
            color
        }));
        brush.set_anti_alias(true);
        canvas.attach_brush(&brush);
        let geometry = self.selection.geometry.borrow();
        if handles {
            if let Some(edges) = geometry.handles {
                for (x, top, bottom) in edges {
                    let x = (content.x + x) * self.scale;
                    let top = (content.y + top) * self.scale;
                    let bottom = (content.y + bottom) * self.scale;
                    canvas.draw_rect(&Rect::new(x - self.scale, top, x + self.scale, bottom));
                    canvas.draw_circle(&Point::new(x, bottom + 5.0 * self.scale), 5.0 * self.scale);
                }
            }
        } else {
            for r in &geometry.rectangles {
                canvas.draw_rect(&Rect::new(
                    (content.x + r[0]) * self.scale,
                    (content.y + r[1]) * self.scale,
                    (content.x + r[2]) * self.scale,
                    (content.y + r[3]) * self.scale,
                ));
            }
        }
        canvas.detach_brush();
    }
}
