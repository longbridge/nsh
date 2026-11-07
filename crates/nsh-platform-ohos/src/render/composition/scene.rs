//! Surface-local composition policy. ArkUI retains every native input/scroll node.
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::{Arc, atomic::Ordering},
};

use nsh_core::{Matrix, ViewKind};
use ohos_arkui_binding::types::advanced::NodeDirtyFlag;
use ohos_drawing_binding::{Bitmap, BitmapFormat, Canvas};

use crate::render::{
    composition::{
        draw::{PaintNode, Plan, Root},
        geometry::{Bounds, requires_scene},
        worker::SceneShared,
    },
    geometry::Size,
    node::{Error, Node, Result},
};

pub(crate) struct Scene {
    root: Weak<Node>,
    active: Cell<bool>,
    dirty: Cell<bool>,
    preparing: Cell<bool>,
    shared: Arc<SceneShared>,
    bitmaps: RefCell<Vec<Option<Bitmap>>>,
    roots: RefCell<Vec<Weak<Node>>>,
    requirements: RefCell<Vec<Weak<Node>>>,
}
impl Scene {
    pub(crate) fn new(root: &Rc<Node>) -> Self {
        Self {
            root: Rc::downgrade(root),
            active: Cell::new(false),
            dirty: Cell::new(false),
            preparing: Cell::new(false),
            shared: Arc::default(),
            bitmaps: RefCell::default(),
            roots: RefCell::default(),
            requirements: RefCell::default(),
        }
    }
    pub(crate) fn active(&self) -> bool {
        self.active.get()
    }
    pub(crate) fn require(&self, node: &Rc<Node>) {
        let required = requires_scene(&node.style.borrow());
        let mut nodes = self.requirements.borrow_mut();
        nodes.retain(|weak| weak.upgrade().is_some_and(|owner| owner.id != node.id));
        if required {
            nodes.push(Rc::downgrade(node));
        }
        drop(nodes);
        self.invalidate();
    }
    pub(crate) fn invalidate(&self) {
        if (self.active.get() || !self.requirements.borrow().is_empty())
            && !self.preparing.get()
            && !self.dirty.replace(true)
            && let Some(root) = self.root.upgrade()
            && let Some(route) = root.dispatcher.upgrade()
        {
            route.send(crate::app::runtime::Message::Arkui);
        }
    }
    pub(crate) fn prepare(&self) -> Result<()> {
        if !self.dirty.replace(false) {
            return Ok(());
        }
        self.preparing.set(true);
        let result = self.reconcile();
        self.preparing.set(false);
        result
    }
    fn reconcile(&self) -> Result<()> {
        let Some(root) = self.root.upgrade() else {
            return Ok(());
        };
        let mut requirements = self.requirements.borrow_mut();
        requirements.retain(|node| node.strong_count() > 0);
        // Most surfaces have no scene effects. Check only current trigger nodes;
        // ordinary mount/animation turns do not scan the whole native tree.
        let active = requirements.iter().filter_map(Weak::upgrade).any(|node| {
            if !node.paint_visible() {
                return false;
            }
            let mut ancestor = Some(node);
            while let Some(node) = ancestor {
                if Rc::ptr_eq(&node, &root) {
                    return true;
                }
                ancestor = node.parent_node.borrow().upgrade();
            }
            false
        });
        drop(requirements);
        let previous = self.active.replace(active);
        if active {
            return self.build();
        }
        if previous {
            if let Some(worker) = root.compositor.borrow().as_ref() {
                worker.clear_plan();
            }
            self.bitmaps.borrow_mut().clear();
            self.roots.borrow_mut().clear();
            self.shared.output.lock().unwrap().take();
            fn restore(node: &Rc<Node>) -> Result<()> {
                node.scene_prepared.set(false);
                if let Some(scroll) = node.scroll.borrow().as_ref() {
                    scroll.restore_scene(node)?;
                }
                node.ensure_wrapper()?;
                node.restore_style()?;
                node.native
                    .borrow()
                    .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
                node.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
                node.mounted()
                    .borrow()
                    .mark_dirty(NodeDirtyFlag::NeedRender)?;
                for child in node.children.borrow().iter().filter_map(Weak::upgrade) {
                    restore(&child)?;
                }
                Ok(())
            }
            restore(&root)?;
        }
        Ok(())
    }
    fn build(&self) -> Result<()> {
        let Some(root) = self.root.upgrade() else {
            return Ok(());
        };
        fn prepare(node: &Rc<Node>, roots: &mut Vec<Rc<Node>>, depth: usize) -> Result<()> {
            if depth > 256 {
                return Err(Error::Unsupported("scene hierarchy exceeds 256 levels"));
            }
            if !node.paint_visible() {
                return Ok(());
            }
            node.ensure_wrapper()?;
            if let Some(scroll) = node.scroll.borrow().as_ref() {
                scroll.prepare_scene(node)?;
            }
            if !node.scene_prepared.replace(true) {
                node.restore_style()?;
            }
            if node.kind == ViewKind::Modal {
                roots.push(node.clone());
            }
            for child in node.children.borrow().iter().filter_map(Weak::upgrade) {
                prepare(&child, roots, depth + 1)?;
            }
            Ok(())
        }
        let mut roots = vec![root.clone()];
        prepare(&root, &mut roots, 0)?;
        let mut plan = Plan {
            scale: root.scale,
            roots: Vec::new(),
        };
        for root in &roots {
            if !root.paint_visible() {
                continue;
            }
            let frame = root.frame.get();
            let node = snapshot(root, None, 0)?;
            plan.roots.push(Root {
                id: root.id,
                width: (frame.width * root.scale).ceil().max(1.0) as u32,
                height: (frame.height * root.scale).ceil().max(1.0) as u32,
                node,
            });
        }
        *self.roots.borrow_mut() = roots.iter().map(Rc::downgrade).collect();
        if let Some(worker) = root.compositor.borrow().as_ref() {
            worker.plan(Arc::new(plan), self.shared.clone());
        }
        Ok(())
    }
    pub(crate) fn ready(&self) -> Result<()> {
        self.shared.posted.store(false, Ordering::Release);
        let frames = self.shared.output.lock().unwrap().take();
        if !self.active.get() {
            return Ok(());
        }
        if let Some(frames) = frames {
            let mut bitmaps = self.bitmaps.borrow_mut();
            let max_id = frames
                .iter()
                .map(|frame| frame.id as usize + 1)
                .max()
                .unwrap_or(0);
            if bitmaps.len() < max_id {
                bitmaps.resize_with(max_id, || None);
            }
            let mut active_ids = vec![false; bitmaps.len()];
            for frame in frames {
                let idx = frame.id as usize;
                active_ids[idx] = true;
                if let Some(existing) = bitmaps[idx].as_mut()
                    && existing.width() == frame.width
                    && existing.height() == frame.height
                {
                    existing.pixels_mut().copy_from_slice(&frame.pixels);
                } else {
                    let mut bitmap =
                        Bitmap::new(frame.width, frame.height, BitmapFormat::default());
                    bitmap.pixels_mut().copy_from_slice(&frame.pixels);
                    bitmaps[idx] = Some(bitmap);
                }
            }
            for (idx, active) in active_ids.into_iter().enumerate() {
                if !active {
                    bitmaps[idx] = None;
                }
            }
            for root in self.roots.borrow().iter().filter_map(Weak::upgrade) {
                root.mounted()
                    .borrow()
                    .mark_dirty(NodeDirtyFlag::NeedRender)?;
            }
        }
        Ok(())
    }
    pub(crate) fn draw(&self, id: u32, canvas: &Canvas) {
        if let Some(Some(bitmap)) = self.bitmaps.borrow().get(id as usize) {
            canvas.draw_bitmap(bitmap, 0.0, 0.0);
        }
    }
}
fn snapshot(owner: &Rc<Node>, parent: Option<&Node>, depth: usize) -> Result<PaintNode> {
    if depth > 256 {
        return Err(Error::Unsupported("scene hierarchy exceeds 256 levels"));
    }
    let frame = owner.frame.get();
    let size = Size::new(frame.width, frame.height);
    let style = owner.style.borrow().clone();
    let source = owner.wrapper.borrow().as_ref().and_then(|wrapper| {
        wrapper
            .composite
            .borrow()
            .as_ref()
            .map(|composite| composite.key())
    });
    let mut children: Vec<_> = owner
        .children
        .borrow()
        .iter()
        .filter_map(Weak::upgrade)
        .filter(|child| child.kind != ViewKind::Modal && child.paint_visible())
        .collect();
    children.sort_by_key(|child| child.input_priority());
    let mut bounds = Bounds::new(size.width, size.height).own(&style);
    let children = children
        .iter()
        .map(|child| snapshot(child, Some(owner), depth + 1))
        .collect::<Result<Vec<_>>>()?;
    if !owner.clips_children() {
        for child in &children {
            let effects = child
                .style
                .effects
                .as_ref()
                .map_or(&[][..], |effects| effects.filters.as_slice());
            if let Some(child) = child.bounds.filters(effects).transformed(child.transform) {
                bounds.union(child);
            } else {
                return Err(Error::Unsupported("unbounded scene projection"));
            }
        }
    }
    if let Some(effects) = &style.effects {
        bounds.union(Bounds::new(size.width, size.height).filters(&effects.backdrop_filters));
    }
    let transform = parent.map_or(Ok(Matrix::IDENTITY), |parent| owner.input_to_parent(parent))?;
    Ok(PaintNode {
        source,
        overlay: owner.scroll.borrow().as_ref().and_then(|scroll| {
            scroll
                .overlay
                .borrow()
                .as_ref()
                .and_then(|plane| plane.key())
        }),
        transform,
        size,
        opacity: owner.scene_opacity(),
        clip: owner.clips_children(),
        bounds,
        style: Arc::new(style),
        children,
    })
}
