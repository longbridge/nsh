//! Applies each engine commit on the ArkUI thread. Deleted boxes stay alive
//! until a native frame and a subsequent event-loop turn have completed.

use std::{collections::VecDeque, rc::Rc, sync::Arc};

use nsh_core::{Command, MountBatch, MountItem, NodeId, ViewKind};
use nsh_engine::EngineControl;
use ohos_arkui_binding::component::attribute::ArkUICommonAttribute;

use crate::{
    render::{
        geometry::{Rect, Size},
        node::{Error, Node, Result},
    },
    text::typography::NativeText,
};

struct MountTarget {
    parent: std::rc::Weak<std::cell::RefCell<ohos_arkui_binding::common::node::ArkUINode>>,
    attached: bool,
}
impl MountTarget {
    fn mount(&mut self, root: crate::render::node::NativeNode) -> Result<()> {
        if let Some(parent) = self.parent.upgrade() {
            parent.borrow_mut().insert_child(root, 0)?;
            self.attached = true;
        }
        Ok(())
    }
    fn unmount(&mut self) -> Result<()> {
        if self.attached {
            if let Some(parent) = self.parent.upgrade() {
                parent.borrow_mut().remove_child(0)?;
            }
            self.attached = false;
        }
        Ok(())
    }
}

pub(crate) struct UiContext {
    pub(crate) shaders: super::shader::Shaders,
    /// The video whose native Stack is temporarily the root's last child.
    pub(crate) fullscreen_video: std::cell::RefCell<std::rc::Weak<Node>>,
    pub(crate) scroll_reports: std::cell::RefCell<Vec<std::rc::Weak<Node>>>,
    pub(crate) flushing_batch: std::cell::Cell<bool>,
    pub(crate) texture_posting: std::cell::Cell<bool>,
    pub(crate) scene: std::cell::RefCell<Option<Rc<crate::render::composition::scene::Scene>>>,
    pub(crate) compositor:
        std::cell::RefCell<Option<Rc<crate::render::composition::worker::Worker>>>,
    pub(crate) composite_nodes: std::cell::RefCell<Vec<std::rc::Weak<Node>>>,
    pub(crate) selected_text: std::cell::RefCell<Option<std::rc::Weak<Node>>>,
    pub(crate) accessibility_dirty: std::cell::RefCell<Vec<std::rc::Weak<Node>>>,
    pub(crate) accessibility_focus: std::cell::RefCell<Vec<std::rc::Weak<Node>>>,
    pub(crate) accessibility_live_regions: std::cell::Cell<usize>,
    pub(crate) accessibility_wrappers: std::cell::Cell<usize>,
    /// How many nodes are leaving (exit transitions, popped screens): while
    /// none is, no mount item needs the walk up its ancestors that looks
    /// for one ([`Node::set_leaving`]).
    pub(crate) leaving_nodes: std::cell::Cell<usize>,
    pub(crate) arkui: std::cell::RefCell<crate::interop::sdk::Batch>,
    pub(crate) arkui_views: std::cell::RefCell<Vec<crate::interop::attachment::NativeAttachment>>,
    pub(crate) host: Arc<crate::app::session::Services>,
    pub(crate) dispatcher: std::sync::Weak<crate::app::runtime::Dispatcher>,
    pub(crate) env: napi_ohos::Env,
    pub(crate) scale: f32,
    pub(crate) control: EngineControl,
    pub(crate) animator: Rc<crate::render::animation::Animator>,
    pub(crate) images: Arc<crate::media::image::Loader>,
    pub(crate) access: Arc<nsh_engine::Access>,
    pub(crate) assets: Arc<dyn nsh_engine::Assets>,
    pub(crate) components: crate::interop::component::Components,
    pub(crate) touches: crate::input::touch::State,
    pub(crate) canvas_bytes: Rc<std::cell::Cell<usize>>,
    pub(crate) platform: std::cell::Cell<nsh_engine::Environment>,
    pub(crate) appearance: std::cell::Cell<nsh_core::platform::Appearance>,
}

pub(crate) struct Mounter {
    context: Rc<UiContext>,
    mount: MountTarget,

    slots: Vec<Option<Rc<Node>>>,
    // Sorted like slots: back priority must not depend on recycled IDs' age.
    stacks: Vec<NodeId>,
    removed: Vec<Rc<Node>>,
    retired: VecDeque<(u64, Vec<Rc<Node>>)>,
    epoch: u64,
    /// Batches applied, from 1: what the mount log counts by.
    batches: u64,
    modals: Vec<std::rc::Weak<crate::navigation::modal::Modal>>,
    pending_delete: Vec<NodeId>,
    dialogs: crate::services::dialogs::Dialogs,
}

impl std::ops::Deref for Mounter {
    type Target = UiContext;
    fn deref(&self) -> &UiContext {
        &self.context
    }
}

impl Mounter {
    pub(crate) fn scene_ready(&self) -> Result<()> {
        if let Some(scene) = self.scene.borrow().as_ref() {
            scene.ready()?;
        }
        Ok(())
    }
    pub(crate) fn composite_ready(&self, id: NodeId, generation: u32) -> Result<()> {
        if let Some(Some(node)) = self.slots.get(id as usize) {
            node.composite_ready(generation)?;
        }
        Ok(())
    }
    pub(crate) fn text_selection(
        &self,
        id: NodeId,
        command: crate::text::selection::controller::Command,
    ) -> Result<()> {
        let selected = self
            .selected_text
            .borrow()
            .as_ref()
            .and_then(std::rc::Weak::upgrade);
        if let Some(node) = selected.filter(|node| node.id == id) {
            node.selection_command(command)?;
        }
        Ok(())
    }
    #[cfg(feature = "api-23")]
    pub(crate) fn accessibility_event(
        &self,
        id: NodeId,
        event: ohos_accessibility_binding::EventType,
    ) -> Result<()> {
        // A callback queued before removal cannot reach a retired node.
        if let Some(Some(node)) = self.slots.get(id as usize) {
            node.accessibility_provider.send_event(event)?;
        }
        Ok(())
    }

    pub(crate) fn environment(&self, environment: nsh_engine::Environment) -> Result<()> {
        self.platform.set(environment);
        for node in self.slots.iter().flatten() {
            if let Some(screen) = node.screen.borrow().as_ref() {
                screen.update_environment()?;
            }
            if node.modal.borrow().is_some() {
                node.native
                    .borrow()
                    .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedMeasure)?;
            }
        }
        self.system_appearance()
    }
    fn system_appearance(&self) -> Result<()> {
        let dark = self
            .appearance
            .get()
            .resolve(self.platform.get().color_scheme)
            == nsh_engine::ColorScheme::Dark;
        let config = self.dispatcher.upgrade().and_then(|route| {
            let status = *route.status_bar.lock().unwrap();
            status
                .filter(|(_, previous)| *previous != dark)
                .map(|(config, _)| config)
        });
        if let Some(config) = config {
            self.host
                .status_bar(self.dispatcher.clone(), config, dark)?;
        }
        Ok(())
    }
    pub(crate) fn native_context(&self, ready: bool) -> Result<()> {
        if !ready {
            let mut pending = self.arkui.borrow_mut();
            pending.editors.clear();
            pending.commands.clear();
            pending.views.clear();
            pending.webs.clear();
            pending.web_commands.clear();
            // Retirements keep their targets until SDK cleanup or the next ready batch.
        }
        for node in self.slots.iter().flatten() {
            if !ready {
                if let Some(wrapper) = node.wrapper.borrow().as_ref() {
                    wrapper.invalidate_context();
                }
                if let Some(scroll) = node.scroll.borrow().as_ref()
                    && let Some(plane) = scroll.overlay.borrow().as_ref()
                {
                    plane.invalidate_context();
                }
            }
            if ready {
                node.update_wrapper()?;
            }
            if let Some(input) = node.input.borrow().as_ref() {
                input.context(ready);
            }
            if let Some(web) = node.web.borrow().as_ref() {
                web.context(ready)?;
            }
            if let Some(video) = node.video.borrow().as_ref() {
                video.context(ready)?;
            }
        }
        if ready {
            self.flush_arkui()?;
        }
        Ok(())
    }
    pub(crate) fn cancel_input(&self) -> Result<()> {
        if let Ok(root) = self.node(nsh_core::ROOT_ID) {
            self.touches.borrow_mut().cancel(root)?;
        }
        for node in self.slots.iter().flatten() {
            if let Some(input) = node.input.borrow().as_ref() {
                input.suspend()?;
            }
            if let Some(scroll) = node.scroll.borrow().as_ref() {
                scroll.block(false)?;
            }
        }
        Ok(())
    }
    pub(crate) fn scroll_to(&self, id: NodeId, _x: f32, _y: f32, _animated: bool) -> Result<()> {
        if let Ok(node) = self.node(id)
            && let Some(scroll) = node.scroll.borrow().as_ref()
        {
            scroll.flush_layout()?;
        }
        Ok(())
    }
    pub(crate) fn wants_back(&self) -> bool {
        self.fullscreen_video.borrow().strong_count() > 0
            || self.dialogs.visible()
            || self
                .modals
                .iter()
                .filter_map(std::rc::Weak::upgrade)
                .any(|modal| modal.is_visible())
            || self.top_screen().is_some()
            || self
                .slots
                .get(1)
                .and_then(Option::as_ref)
                .is_some_and(|root| {
                    root.interaction
                        .mask()
                        .contains(nsh_core::EventKind::BackPress)
                })
    }
    pub(crate) fn back(&mut self) -> Result<()> {
        let fullscreen = self.fullscreen_video.borrow().upgrade();
        if let Some(owner) = fullscreen
            && let Some(video) = owner.video.borrow().as_ref()
        {
            return video.set_fullscreen(false);
        }
        if self.dialogs.visible() {
            return self.dialogs.request_close();
        }
        if let Some(modal) = self
            .modals
            .iter()
            .rev()
            .filter_map(std::rc::Weak::upgrade)
            .find(|modal| modal.is_visible())
        {
            return modal.request_close();
        }
        if self
            .node(nsh_core::ROOT_ID)?
            .interaction
            .mask()
            .contains(nsh_core::EventKind::BackPress)
        {
            self.control.dispatch_event(
                nsh_core::ROOT_ID,
                nsh_core::EventKind::BackPress,
                nsh_core::EventPayload::None,
            );
            return Ok(());
        }
        self.back_fallback()
    }
    pub(crate) fn dismiss_keyboard(&self) -> Result<()> {
        self.node(nsh_core::ROOT_ID)?.dismiss_keyboard()
    }
    fn back_fallback(&self) -> Result<()> {
        if let Some(screen) = self.top_screen() {
            screen.request_back()?;
        }
        Ok(())
    }
    fn top_screen(&self) -> Option<Rc<crate::navigation::stack::Screen>> {
        self.stacks
            .iter()
            .rev()
            .filter_map(|id| self.slots[*id as usize].as_ref())
            .filter(|n| n.visible_in_tree())
            .find_map(|stack| {
                let screens: Vec<_> = stack
                    .children
                    .borrow()
                    .iter()
                    .filter_map(std::rc::Weak::upgrade)
                    .filter_map(|n| n.screen.borrow().clone())
                    .filter(|s| !s.popped.get())
                    .collect();
                let top = screens.last()?;
                (screens.len() > 1 || top.config.borrow().prevent_back).then(|| top.clone())
            })
    }
    pub(crate) fn reconcile_navigation(&mut self) -> Result<()> {
        crate::navigation::stack::reconcile(
            self.stacks
                .iter()
                .filter_map(|id| self.slots[*id as usize].as_ref()),
        )?;
        for modal in self.modals.iter().filter_map(std::rc::Weak::upgrade) {
            modal.present_if_pending()?;
        }
        Ok(())
    }
    pub(crate) fn dialog_result(&mut self, id: u32, index: i32) -> Result<()> {
        self.dialogs.finish(id, index)
    }
    pub(crate) fn window_environment(
        &self,
        visual: [crate::app::session::AvoidArea; 3],
        keyboard: crate::app::session::AvoidArea,
    ) -> Result<crate::app::environment::NshEnvironment> {
        let root = self.node(1)?;
        let origin = root.native.borrow().layout_position_in_window()?;
        let frame = root.frame.get();
        let scale = self.scale as f64;
        let viewport = super::geometry::WindowRect {
            x: origin.x as f64,
            y: origin.y as f64,
            width: frame.width as f64 * scale,
            height: frame.height as f64 * scale,
        };
        let rect = |rect: crate::app::session::AvoidRect| super::geometry::WindowRect {
            x: rect.left as f64,
            y: rect.top as f64,
            width: rect.width as f64,
            height: rect.height as f64,
        };
        let [top, right, bottom, left] = super::geometry::visual_insets(
            viewport,
            visual.into_iter().filter(|area| area.visible).map(|area| {
                [
                    rect(area.top_rect),
                    rect(area.right_rect),
                    rect(area.bottom_rect),
                    rect(area.left_rect),
                ]
            }),
        );
        Ok(crate::app::environment::NshEnvironment {
            safe_top: Some(top / scale),
            safe_right: Some(right / scale),
            safe_bottom: Some(bottom / scale),
            safe_left: Some(left / scale),
            keyboard_height: Some(if keyboard.visible {
                viewport
                    .intersection(rect(keyboard.bottom_rect))
                    .map_or(0.0, |area| area.height / scale)
            } else {
                0.0
            }),
            ..crate::app::environment::NshEnvironment::default()
        })
    }
    pub(crate) fn new(
        parent: crate::render::node::NativeNode,
        size: Size,
        context: Rc<UiContext>,
    ) -> Result<Self> {
        let mut mount = MountTarget {
            parent: Rc::downgrade(&parent),
            attached: false,
        };
        let root = Node::new(1, ViewKind::View, context.clone())?;
        root.set_frame(Rect {
            x: 0.0,
            y: 0.0,
            width: size.width,
            height: size.height,
        })?;
        mount.mount(root.native.clone())?;
        *root.mount_parent.borrow_mut() = mount.parent.clone();
        root.set_events(nsh_core::EventMask::default())?;
        *context.scene.borrow_mut() = Some(Rc::new(crate::render::composition::scene::Scene::new(
            &root,
        )));
        Ok(Self {
            dialogs: crate::services::dialogs::Dialogs::new(context.clone()),
            context,
            mount,
            slots: vec![None, Some(root)],
            stacks: Vec::new(),
            removed: Vec::new(),
            retired: VecDeque::new(),
            epoch: 0,
            batches: 0,
            modals: Vec::new(),
            pending_delete: Vec::new(),
        })
    }
    fn node(&self, id: NodeId) -> Result<&Rc<Node>> {
        self.slots
            .get(id as usize)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidNode(id))
    }

    fn detach(&mut self, id: NodeId) -> Result<()> {
        if let Some(parent) = self.node(id)?.parent.get() {
            let parent = self.node(parent)?;
            let index = parent
                .children
                .borrow()
                .iter()
                .rposition(|n| n.upgrade().is_some_and(|n| n.id == id))
                .ok_or(Error::InvalidNode(id))?;
            if let Some(modal) = self.node(id)?.modal.borrow().as_ref() {
                modal.close(true)?;
            } else {
                let container = parent.child_container();
                container
                    .borrow_mut()
                    .remove_child(parent.child_index(index))?;
            }
            parent.children.borrow_mut().remove(index);
            if parent.children.borrow().is_empty() && !parent.clips_children() {
                parent.update_layout_bounds()?;
            }
            parent.accessibility_state.changed(parent);
            self.node(id)?.parent.set(None);
            *self.node(id)?.parent_node.borrow_mut() = std::rc::Weak::new();
            *self.node(id)?.mount_parent.borrow_mut() = std::rc::Weak::new();
            self.node(id)?.invalidate_scene();
        }
        Ok(())
    }

    fn closing_modal(&self, mut id: NodeId) -> bool {
        // Without a live modal no ancestor can be one that closes.
        if !self.modals.iter().any(|modal| modal.strong_count() > 0) {
            return false;
        }
        loop {
            let Ok(node) = self.node(id) else {
                return false;
            };
            if node
                .modal
                .borrow()
                .as_ref()
                .is_some_and(|modal| modal.closing())
            {
                return true;
            }
            let Some(parent) = node.parent.get() else {
                return false;
            };
            id = parent;
        }
    }
    fn leaving(&self, mut id: NodeId) -> bool {
        if self.leaving_nodes.get() == 0 {
            return false;
        }
        loop {
            let Ok(node) = self.node(id) else {
                return false;
            };
            if node.is_leaving() {
                return true;
            }
            let Some(parent) = node.parent.get() else {
                return false;
            };
            id = parent;
        }
    }
    /// Whether `id` or an ancestor is leaving or a closing modal: its
    /// content keeps what it showed until it is gone. One walk up for both,
    /// none while nothing leaves and no modal is open.
    fn frozen(&self, mut id: NodeId) -> bool {
        let leaving = self.leaving_nodes.get() > 0;
        let modals = self.modals.iter().any(|modal| modal.strong_count() > 0);
        if !leaving && !modals {
            return false;
        }
        loop {
            let Ok(node) = self.node(id) else {
                return false;
            };
            if (leaving && node.is_leaving())
                || (modals
                    && node
                        .modal
                        .borrow()
                        .as_ref()
                        .is_some_and(|modal| modal.closing()))
            {
                return true;
            }
            let Some(parent) = node.parent.get() else {
                return false;
            };
            id = parent;
        }
    }
    fn delete(&mut self, id: NodeId) -> Result<()> {
        if let Some(modal) = self.node(id)?.modal.borrow().as_ref() {
            modal.close(true)?;
        }
        let leaving = self.leaving(id);
        if leaving || self.closing_modal(id) {
            if !self.node(id)?.pending_delete.replace(true) {
                if !leaving {
                    self.node(id)?.deactivate();
                }
                self.pending_delete.push(id);
            }
            return Ok(());
        }
        self.delete_now(id)
    }
    fn orphan_subtree(&mut self, id: NodeId) -> Result<()> {
        self.detach(id)?;
        // Children can be deleted later in the batch (or when QuickJS collects a
        // reference cycle). Sever every native edge and release native attachments
        // across the orphaned subtree before any endpoint is eligible for disposal.
        loop {
            let child = self
                .node(id)?
                .children
                .borrow()
                .last()
                .and_then(|n| n.upgrade());
            let Some(child) = child else { break };
            self.orphan_subtree(child.id)?;
        }
        if let Ok(node) = self.node(id) {
            self.animator.remove(id);
            node.deactivate();
            *node.text_drawing.borrow_mut() = None;
            *node.raster.borrow_mut() = None;
        }
        Ok(())
    }
    fn delete_now(&mut self, id: NodeId) -> Result<()> {
        self.orphan_subtree(id)?;
        self.shaders.remove(id);
        if let Some(node) = self.slots[id as usize].take() {
            if node.kind == ViewKind::Stack
                && let Ok(index) = self.stacks.binary_search(&id)
            {
                self.stacks.remove(index);
            }
            self.removed.push(node);
        }
        Ok(())
    }

    pub(crate) fn apply(&mut self, batch: MountBatch) -> Result<()> {
        struct BatchGuard(Rc<UiContext>);
        impl Drop for BatchGuard {
            fn drop(&mut self) {
                self.0.flushing_batch.set(false);
            }
        }
        self.flushing_batch.set(true);
        let _batch_guard = BatchGuard(self.context.clone());
        if let Some(scene) = self.scene.borrow().as_ref() {
            scene.invalidate();
        }
        let layout = batch.items.iter().find_map(|item| match item {
            MountItem::LayoutAnimation(animation) => Some(*animation),
            _ => None,
        });
        // Start a screen's exit before its descendants' delete operations. An
        // enclosing tree deletion retires immediately instead of animating orphans.
        if batch.items.iter().any(|item| matches!(item, MountItem::Delete { id } if self.node(*id).is_ok_and(|node| node.kind == ViewKind::Screen))) {
            let deleted: rustc_hash::FxHashSet<_> = batch.items.iter().filter_map(|item| {
                if let MountItem::Delete { id } = item { Some(*id) } else { None }
            }).collect();
            for item in &batch.items {
                let MountItem::Delete { id } = item else { continue; };
                let Ok(node) = self.node(*id) else { continue; };
                let Some(screen) = node.screen.borrow().as_ref().cloned() else { continue; };
                let mut ancestor = node.parent_node.borrow().upgrade();
                let mut enclosing_delete = false;
                while let Some(parent) = ancestor {
                    if deleted.contains(&parent.id) { enclosing_delete = true; break; }
                    ancestor = parent.parent_node.borrow().upgrade();
                }
                let animated = !enclosing_delete && screen.previous().is_some_and(|previous| !deleted.contains(&previous.id));
                screen.prepare_removal(animated)?;
                if animated { screen.remove()?; }
            }
        }
        for item in &batch.items {
            if let MountItem::Delete { id } = item
                && let Ok(node) = self.node(*id)
                && let Some(modal) = node.modal.borrow().as_ref()
            {
                modal.close(true)?;
            }
        }
        if layout
            .as_ref()
            .is_some_and(|animation| animation.delete.is_some())
        {
            let removed: rustc_hash::FxHashSet<_> = batch
                .items
                .iter()
                .filter_map(|item| match item {
                    MountItem::Remove { id } | MountItem::Delete { id } => Some(*id),
                    _ => None,
                })
                .collect();
            for id in &removed {
                let Ok(node) = self.node(*id) else { continue };
                let mut parent = node.parent_node.borrow().upgrade();
                let mut enclosed = false;
                while let Some(ancestor) = parent {
                    if removed.contains(&ancestor.id) {
                        enclosed = true;
                        break;
                    }
                    parent = ancestor.parent_node.borrow().upgrade();
                }
                if !enclosed {
                    node.removing(layout.as_ref())?;
                }
            }
        }
        self.batches += 1;
        let items = batch.items.len();
        for (at, item) in batch.items.into_iter().enumerate() {
            let result = (|| -> Result<()> {
                match item {
                    MountItem::Component { id, name } => {
                        if let Some(component) =
                            self.components
                                .make(self.env, &name, id, self.control.clone())?
                        {
                            let node = self.node(id)?;
                            node.component_animated.borrow_mut().clear();
                            node.attach_component(component)?;
                            self.animator.mark_dirty();
                        }
                    }
                    MountItem::ComponentPropSlots { id, names } => {
                        let node = self.node(id)?;
                        *node.component_slots.borrow_mut() = names;
                        node.component_animated.borrow_mut().clear();
                        self.animator.mark_dirty();
                    }
                    MountItem::Canvas { id, drawing } => {
                        if let Some(canvas) = self.node(id)?.canvas.borrow().as_ref() {
                            canvas.set(drawing)?;
                        }
                    }
                    MountItem::ScreenConfig { id, config } => {
                        if !self.frozen(id)
                            && let Some(screen) = self.node(id)?.screen.borrow().as_ref()
                        {
                            screen.configure(*config)?;
                        }
                    }
                    MountItem::NativeProps { id, props } => match props {
                        nsh_core::NativeProps::WebView(props) => {
                            if let Some(web) = self.node(id)?.web.borrow().as_ref() {
                                web.configure(*props)?;
                            }
                        }
                        nsh_core::NativeProps::Video(props) => {
                            if let Some(video) = self.node(id)?.video.borrow().as_ref() {
                                video.configure(props)?;
                            }
                        }
                    },
                    MountItem::ComponentProps { id, props } => {
                        if let Some(component) = self.node(id)?.component.borrow().as_ref() {
                            component.props(props)?;
                        }
                    }
                    MountItem::Create { id, kind } => {
                        self.slots
                            .resize_with(self.slots.len().max(id as usize + 1), || None);
                        if self.slots[id as usize].is_some() {
                            return Err(Error::InvalidNode(id));
                        }
                        self.slots[id as usize] = Some(Node::new(id, kind, self.context.clone())?);
                        if kind == ViewKind::Stack {
                            let index = self.stacks.partition_point(|stack| *stack < id);
                            self.stacks.insert(index, id);
                        }
                        let modal = self.node(id)?.modal.borrow().as_ref().map(Rc::downgrade);
                        if let Some(modal) = modal {
                            self.modals.push(modal);
                        }
                    }
                    MountItem::Insert {
                        parent,
                        child,
                        before,
                    } => {
                        self.detach(child)?;
                        let parent = self.node(parent)?;
                        parent.enable_layout();
                        let node = self.slots[child as usize].as_ref().unwrap();
                        // A sibling not in the parent yet (a host retagged in this
                        // batch, inserted by its own later item) places the child
                        // last, as GTK's mounter does, rather than fail the batch.
                        let index = {
                            let children = parent.children.borrow();
                            before
                                .and_then(|before| {
                                    children
                                        .iter()
                                        .position(|n| n.upgrade().is_some_and(|n| n.id == before))
                                })
                                .unwrap_or(children.len())
                        };
                        if let Some(modal) = node.modal.borrow().as_ref() {
                            modal.schedule_present();
                        } else {
                            let container = parent.child_container();
                            container
                                .borrow_mut()
                                .insert_child(node.mounted(), parent.child_index(index))?;
                            // ArkUI insert/remove request measurement themselves.
                            // NeedMeasure adds SELF_AND_PARENT and would escape
                            // the engine-owned parent's measure boundary.
                            *node.mount_parent.borrow_mut() = Rc::downgrade(&container);
                        }
                        let was_empty = parent.children.borrow().is_empty();
                        parent
                            .children
                            .borrow_mut()
                            .insert(index, Rc::downgrade(node));
                        if was_empty && !parent.clips_children() {
                            parent.update_layout_bounds()?;
                        }
                        parent.accessibility_state.changed(parent);
                        node.parent.set(Some(parent.id));
                        *node.parent_node.borrow_mut() = Rc::downgrade(parent);
                        node.accessibility_state.changed(node);
                        node.interaction
                            .pointer(node, parent.interaction.blocks_children())?;
                        if node.kind == ViewKind::HeaderItem
                            && let Some(screen) = parent.screen.borrow().as_ref()
                        {
                            screen.update(screen.has_back())?;
                        }
                        node.inserted(layout.as_ref())?;
                    }
                    MountItem::Remove { id } => {
                        if !self.frozen(id) && !self.node(id)?.removing(layout.as_ref())? {
                            self.detach(id)?;
                        }
                    }
                    MountItem::Delete { id } => {
                        if !self.frozen(id) {
                            self.node(id)?.removing(layout.as_ref())?;
                        }
                        self.delete(id)?;
                    }
                    MountItem::Frame { id, frame } => {
                        if !self.frozen(id) {
                            self.node(id)?
                                .animated_frame(frame.into(), layout.as_ref())?;
                        }
                    }
                    MountItem::LayoutAnimation(_) => {}
                    MountItem::ViewStyle { id, style } => {
                        if !self.frozen(id) {
                            self.node(id)?.style(&style)?;
                        }
                    }
                    MountItem::Text {
                        id,
                        layout,
                        content,
                        ..
                    } => {
                        if !self.frozen(id) {
                            let node = self.node(id)?;
                            let raster = layout
                                .0
                                .downcast_ref::<NativeText>()
                                .ok_or(Error::Unsupported("foreign text layout"))?;
                            node.accessibility_state.text(node, &raster.text);
                            let links_changed =
                                node.text(raster, content.into(), layout.clone())?;
                            if links_changed {
                                node.set_events(node.interaction.mask())?;
                            }
                            node.interaction.text(node, &raster.text)?;
                        }
                    }
                    MountItem::Gestures { id, values } => self
                        .node(id)?
                        .interaction
                        .gestures(self.node(id)?, values)?,
                    MountItem::Animation(op) => self.animator.apply(op),
                    MountItem::AnimationBindings { id, bindings } => {
                        self.animator.bind(self.node(id)?, bindings)?
                    }
                    MountItem::Events { id, events } => self.node(id)?.set_events(events)?,
                    MountItem::Keymap(map) => {
                        if let Some(dispatcher) = self.dispatcher.upgrade() {
                            dispatcher.keyboard.lock().unwrap().map = map;
                        }
                    }
                    MountItem::ViewProps { id, props } => {
                        self.node(id)?.interaction.props(self.node(id)?, *props)?
                    }
                    MountItem::AccessibilityLabel { id, label } => {
                        self.node(id)?.interaction.label(self.node(id)?, label)?
                    }
                    // A host retagged in this batch (a component whose root is an
                    // input) can have its tracking ahead of its new `Create`, which
                    // sends it again: skip a view not made yet, as UIKit's and GTK's
                    // mounters do, rather than fail the batch.
                    MountItem::TrackStates { id, states } => {
                        if let Ok(node) = self.node(id) {
                            node.interaction.track(node, states)?;
                        }
                    }
                    MountItem::ModalConfig { id, config } => {
                        if !self.frozen(id)
                            && let Some(modal) = self.node(id)?.modal.borrow().as_ref()
                        {
                            modal.configure(config)?;
                        }
                    }
                    MountItem::Image {
                        id,
                        source,
                        resize_mode,
                        options,
                    } => {
                        if let Some(image) = self.node(id)?.image.borrow().as_ref() {
                            image.configure(source, resize_mode, options)?;
                        }
                    }
                    MountItem::Input { id, config } => {
                        if let Some(input) = self.node(id)?.input.borrow().as_ref() {
                            input.configure(*config)?;
                        }
                    }
                    MountItem::InputValue { id, text, count } => {
                        if let Some(input) = self.node(id)?.input.borrow().as_ref() {
                            input.value(text, count)?;
                        }
                    }
                    MountItem::InputContent { id, content } => {
                        if let Some(input) = self.node(id)?.input.borrow().as_ref() {
                            input.content(content.into())?;
                        }
                    }
                    MountItem::Shader { id, layer } => self.shaders.set(self.node(id)?, layer)?,
                    MountItem::ScrollConfig {
                        id,
                        horizontal,
                        scrollbar_visible,
                        replaced,
                        scroll_value,
                        pull_value,
                        keyboard_dismiss,
                        mirrored,
                    } => {
                        if let Some(scroll) = self.node(id)?.scroll.borrow().as_ref() {
                            scroll.config(
                                horizontal,
                                scrollbar_visible && !replaced,
                                scroll_value,
                                pull_value,
                                keyboard_dismiss,
                                mirrored,
                            )?;
                        }
                    }
                    MountItem::ScrollContent { id, size } => {
                        if let Some(scroll) = self.node(id)?.scroll.borrow().as_ref() {
                            scroll.size(size.into())?;
                        }
                    }
                    MountItem::StickyCells { id, cells } => {
                        // A cell without a view yet is not pinned, as on UIKit and GTK.
                        let cells = cells
                            .into_iter()
                            .filter_map(|cell| {
                                let node = self.node(cell.cell).ok()?;
                                Some((cell, Rc::downgrade(node)))
                            })
                            .collect::<Vec<_>>();
                        if let Some(scroll) = self.node(id)?.scroll.borrow().as_ref() {
                            scroll.sticky(cells)?;
                        }
                    }
                    MountItem::Refreshing { id, refreshing } => {
                        if let Some(scroll) = self.node(id)?.scroll.borrow().as_ref() {
                            scroll.refreshing(refreshing)?;
                        }
                    }
                    MountItem::ScrollBehavior { id, behavior } => {
                        if let Some(scroll) = self.node(id)?.scroll.borrow().as_ref() {
                            scroll.behavior(&behavior)?;
                        }
                    }
                    MountItem::ScrollBy { id, dx, dy } => {
                        if let Some(scroll) = self.node(id)?.scroll.borrow().as_ref() {
                            scroll.by(dx.0, dy.0)?;
                        }
                    }
                    MountItem::Command { id, command } => match command {
                        Command::StatusBar(config) => self.host.status_bar(
                            self.dispatcher.clone(),
                            config,
                            self.appearance
                                .get()
                                .resolve(self.platform.get().color_scheme)
                                == nsh_engine::ColorScheme::Dark,
                        )?,
                        Command::Desktop(_) => {}
                        Command::Announce(text) => self.host.accessibility_event(
                            self.dispatcher.clone(),
                            Some(text),
                            None,
                        )?,
                        Command::AccessibilityFocus => {
                            self.accessibility_focus
                                .borrow_mut()
                                .push(Rc::downgrade(self.node(id)?));
                        }
                        Command::Dialog(request) => {
                            let id = request.id;
                            if let Err(error) = self.dialogs.show(*request) {
                                self.control.dispatch_event(
                                    nsh_core::ROOT_ID,
                                    nsh_core::EventKind::Dialog,
                                    nsh_core::EventPayload::Map(vec![
                                        ("id", nsh_core::PayloadValue::Number(id as f64)),
                                        ("index", nsh_core::PayloadValue::Number(-1.0)),
                                    ]),
                                );
                                log::error!("OHOS dialog: {error}");
                            }
                        }
                        Command::Back => self.back_fallback()?,
                        Command::ExitApp => {
                            log::warn!("OHOS ExitApp: the embedding host owns Ability lifetime")
                        }
                        Command::ImageInfo { id, uri } => {
                            self.images
                                .load(crate::media::image::Target::Info(id), uri, None);
                        }
                        Command::OpenUrl { id, url } => {
                            if self
                                .host
                                .open_url(self.dispatcher.clone(), id, url)
                                .is_err()
                            {
                                self.control.dispatch_event(
                                    nsh_core::ROOT_ID,
                                    nsh_core::EventKind::OpenUrl,
                                    nsh_core::EventPayload::Map(vec![
                                        ("id", nsh_core::PayloadValue::Number(id as f64)),
                                        ("opened", nsh_core::PayloadValue::Bool(false)),
                                    ]),
                                );
                            }
                        }
                        Command::SetClipboardImage(_) => {}
                        Command::ReadClipboardImage { id, .. } => {
                            self.control.dispatch_event(
                                nsh_core::ROOT_ID,
                                nsh_core::EventKind::Clipboard,
                                nsh_core::platform::clipboard::image_answer(id, false, None),
                            );
                        }
                        Command::ReadClipboardFiles { id } => {
                            self.control.dispatch_event(
                                nsh_core::ROOT_ID,
                                nsh_core::EventKind::Clipboard,
                                nsh_core::platform::clipboard::files_answer(id, &[]),
                            );
                        }
                        Command::PickImages { id, request } => {
                            if let Err(error) =
                                self.host.pick_images(self.dispatcher.clone(), id, request)
                            {
                                log::warn!("OHOS pickImages failed: {error}");
                            }
                        }
                        Command::Native { name, args } => {
                            if let Some(component) = self.node(id)?.component.borrow().as_ref() {
                                component.command(&name, &args)?;
                            }
                        }
                        Command::SetClipboard(text) => self.host.enqueue_clipboard(
                            self.dispatcher.clone(),
                            Some(text),
                            None,
                        )?,
                        Command::ReadClipboard { id, has_only } => self.host.enqueue_clipboard(
                            self.dispatcher.clone(),
                            None,
                            Some((id, has_only)),
                        )?,
                        Command::Blur if id == nsh_core::ROOT_ID => {
                            for node in self.slots.iter().flatten() {
                                if let Some(input) = node.input.borrow().as_ref() {
                                    input.focus(false)?;
                                }
                            }
                        }
                        Command::Focus | Command::Blur => {
                            if let Some(web) = self.node(id)?.web.borrow().as_ref() {
                                web.command(command)?;
                            } else if let Some(input) = self.node(id)?.input.borrow().as_ref() {
                                input.focus(matches!(command, Command::Focus))?;
                            }
                        }
                        Command::SetSelection { start, end } => {
                            if let Some(input) = self.node(id)?.input.borrow().as_ref() {
                                input.selection(start, end)?;
                            }
                        }
                        Command::SetJsResponder { block } => {
                            let mut node = Some(self.node(id)?.clone());
                            while let Some(current) = node {
                                if let Some(scroll) = current.scroll.borrow().as_ref() {
                                    scroll.block(block)?;
                                }
                                node = current.parent_node.borrow().upgrade();
                            }
                        }
                        Command::ClearJsResponder => {
                            for node in self.slots.iter().flatten() {
                                if let Some(scroll) = node.scroll.borrow().as_ref() {
                                    scroll.block(false)?;
                                }
                            }
                        }
                        Command::ZoomTo {
                            scale,
                            center,
                            animated,
                        } => {
                            if let Some(scroll) = self.node(id)?.scroll.borrow().as_ref() {
                                scroll.zoom_to(scale, center.map(|p| (p.x.0, p.y.0)), animated)?;
                            }
                        }
                        Command::ScrollTo { x, y, animated } => {
                            if let Some(scroll) = self.node(id)?.scroll.borrow().as_ref() {
                                scroll.command_to(x.0, y.0, animated)?;
                            }
                        }
                        command @ Command::Seek(_) => {
                            if let Some(video) = self.node(id)?.video.borrow().as_ref() {
                                video.command(command)?;
                            }
                        }
                        command @ (Command::Reload
                        | Command::GoBack
                        | Command::GoForward
                        | Command::StopLoading
                        | Command::InjectJavaScript(_)
                        | Command::PostMessage(_)) => {
                            if let Some(web) = self.node(id)?.web.borrow().as_ref() {
                                web.command(command)?;
                            }
                        }
                        _ => log::warn!("OHOS native command is not implemented"),
                    },
                    // Appearance and direction also reach native controls and SDK subtrees.
                    MountItem::Appearance { appearance } => {
                        self.appearance.set(appearance);
                        self.system_appearance()?;
                    }
                    MountItem::LayoutDirection { rtl } => {
                        use ohos_arkui_binding::types::{
                            attribute::ArkUINodeAttributeType as A, direction::Direction,
                        };
                        self.node(nsh_core::ROOT_ID)?
                            .native
                            .borrow()
                            .set_i32_attribute(
                                A::Direction,
                                if rtl { Direction::Rtl } else { Direction::Ltr }.into(),
                            )?;
                    }
                    MountItem::Overflow { id, extent } => {
                        let node = self.node(id)?;
                        node.overflow.set(extent.map(Rect::from));
                        node.apply_clip()?;
                        node.interaction.hit_region(node)?;
                        node.invalidate_scene();
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                match error {
                    Error::Unsupported(_) => log::warn!(
                        target: "nsh::mount",
                        "skipped mount item: batch={} item={at}/{items}: {error}",
                        self.batches
                    ),
                    // A node the mounter does not have (made later in the batch, or
                    // never): skipped, as UIKit's and GTK's mounters skip it, rather
                    // than dropping the batch and the surface with it.
                    Error::InvalidNode(node) => log::warn!(
                        target: "nsh::mount",
                        "skipped mount item: batch={} item={at}/{items}: invalid OHOS mount node {node}",
                        self.batches
                    ),
                    _ => {
                        log::error!(
                            target: "nsh::mount",
                            "failed mount item: batch={} item={at}/{items}: {error}",
                            self.batches
                        );
                        return Err(error);
                    }
                }
            }
        }
        self.reconcile_navigation()?;
        self.flush_arkui()
    }

    pub(crate) fn image_ready(&self, ready: crate::media::image::Ready) -> Result<()> {
        match ready.target {
            crate::media::image::Target::Canvas {
                id,
                generation,
                index,
            } => {
                if let Ok(node) = self.node(id)
                    && let Some(canvas) = node.canvas.borrow().as_ref()
                {
                    canvas.ready(generation, index, ready.data)?;
                }
            }
            crate::media::image::Target::Info(id) => {
                crate::media::image::info(&self.control, id, ready.data)
            }
            crate::media::image::Target::View {
                id,
                generation,
                placeholder,
            } => {
                if let Ok(node) = self.node(id)
                    && let Some(image) = node.image.borrow().as_ref()
                {
                    image.ready(generation, placeholder, ready.data)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn image_progress(&self, progress: crate::media::image::Progress) {
        if let crate::media::image::Target::View {
            id,
            generation,
            placeholder: false,
        } = progress.target
            && let Ok(node) = self.node(id)
            && let Some(image) = node.image.borrow().as_ref()
        {
            image.progress(generation, progress.loaded, progress.total);
        }
    }
    pub(crate) fn flush_scroll_reports(&self) -> Result<()> {
        let pending = std::mem::take(&mut *self.scroll_reports.borrow_mut());
        for node in pending.into_iter().filter_map(|node| node.upgrade()) {
            if let Some(scroll) = node.scroll.borrow().as_ref() {
                scroll.flush_report()?;
            }
        }
        // A shader view scrolled into or out of sight starts or stops.
        self.shaders.reveal()
    }
    pub(crate) fn frame(&mut self) -> Result<bool> {
        let mut animating = self.animator.tick()?;
        animating |= self.shaders.frame()?;
        for id in self.animator.take_removals() {
            if self.node(id).is_ok() {
                self.node(id)?.finish_removal()?;
                self.detach(id)?;
            }
        }
        self.modals.retain(|modal| modal.strong_count() > 0);
        for modal in self.modals.iter().filter_map(std::rc::Weak::upgrade) {
            animating |= modal.frame()?;
        }
        let pending = std::mem::take(&mut self.pending_delete);
        for id in pending {
            if self.frozen(id) {
                self.pending_delete.push(id);
            } else {
                self.delete_now(id)?;
            }
        }
        self.flush_arkui()?;
        Ok(animating)
    }

    pub(crate) fn resize(&self, size: Size) -> Result<()> {
        self.node(1)?.set_frame(Rect {
            x: 0.0,
            y: 0.0,
            width: size.width,
            height: size.height,
        })?;
        if let Some(owner) = self.fullscreen_video.borrow().upgrade()
            && let Some(video) = owner.video.borrow().as_ref()
        {
            video.fullscreen_frame(size)?;
        }
        self.control.resize(size.into());
        Ok(())
    }

    pub(crate) fn close(&mut self) -> Result<()> {
        self.dialogs.clear();
        self.mount.unmount()?;
        for modal in self.modals.iter().filter_map(std::rc::Weak::upgrade) {
            modal.close(false)?;
        }
        self.pending_delete.clear();
        for id in (0..self.slots.len()).rev() {
            if self.slots[id].is_some() {
                self.delete_now(id as NodeId)?;
            }
        }
        self.flush_arkui()
    }

    pub(crate) fn freeze_retired(&mut self) -> Option<u64> {
        if self.removed.is_empty() {
            return None;
        }
        self.epoch += 1;
        self.retired
            .push_back((self.epoch, std::mem::take(&mut self.removed)));
        Some(self.epoch)
    }

    pub(crate) fn reclaim(&mut self, epoch: u64) {
        while self.retired.front().is_some_and(|(id, _)| *id <= epoch) {
            self.retired.pop_front();
        }
    }
}

impl Drop for Mounter {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            log::error!("OHOS detach failed: {error}");
        }
    }
}
