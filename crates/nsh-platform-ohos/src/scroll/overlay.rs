//! Export the native scroll chrome separately so it paints above child planes.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use ohos_arkui_binding::{
    component::{attribute::ArkUIEvent, built_in_component::Custom},
    types::advanced::{LengthMetricUnit, NodeDirtyFlag},
};

use crate::{
    interop::{attachment::NativeAttachment, sdk::ViewOptions},
    render::{
        composition::worker::{Composite, Key},
        node::{NativeNode, Node, Result, callback},
    },
};

pub(crate) struct Plane {
    pub(crate) outer: NativeNode,
    target: RefCell<Option<NativeAttachment>>,
    route: RefCell<Option<crate::interop::sdk::ViewRoute>>,
    inner: NativeNode,
    owner: std::rc::Weak<Node>,
    context_live: Cell<bool>,
    composite: RefCell<Option<Composite>>,
    settings: RefCell<Option<ViewOptions>>,
    closed: Cell<bool>,
}
impl Plane {
    fn with_child<T>(
        &self,
        f: impl FnOnce(&ohos_arkui_binding::common::node::ArkUINode) -> T,
    ) -> Result<Option<T>> {
        if self.closed.get()
            || !self.context_live.get()
            || !self
                .owner
                .upgrade()
                .is_some_and(|owner| owner.host.has_ui_context())
        {
            return Ok(None);
        }
        self.target
            .borrow()
            .as_ref()
            .map_or(Ok(None), |target| Ok(target.with_child(f)?))
    }
    pub(crate) fn new(owner: &Rc<Node>, widget: &NativeNode) -> Result<Rc<Self>> {
        let outer = Custom::new()?.into_node();
        outer
            .borrow()
            .set_length_metric_unit(LengthMetricUnit::Vp)?;
        let target = NativeAttachment::for_subtree(&outer, widget)?;
        crate::interop::texture_input::TextureInput::install(owner, &outer, widget, true);
        let composite = Composite::overlay(owner, target.id())?;
        let plane = Rc::new(Self {
            outer,
            route: RefCell::new(Some(crate::interop::sdk::ViewRoute::new(
                target.id(),
                Rc::downgrade(owner),
                widget,
            ))),
            target: RefCell::new(Some(target)),
            inner: widget.clone(),
            owner: Rc::downgrade(owner),
            context_live: Cell::new(true),
            composite: RefCell::new(Some(composite)),
            settings: RefCell::default(),
            closed: Cell::new(false),
        });
        let weak = Rc::downgrade(&plane);
        let node = Rc::downgrade(owner);
        plane.outer.borrow_mut().on_custom_measure(move |event| {
            callback(|| {
                if let (Some(plane), Some(owner)) = (weak.upgrade(), node.upgrade()) {
                    let frame = owner.frame.get();
                    event.set_measured_size(
                        (frame.width * owner.scale).round() as i32,
                        (frame.height * owner.scale).round() as i32,
                    )?;
                    if let Some(result) = plane.with_child(|child| {
                        crate::render::layout::ChildMeasure::new(event).child(child)
                    })? {
                        result?;
                    }
                }
                Ok(())
            })
        });
        let weak = Rc::downgrade(&plane);
        plane.outer.borrow_mut().on_custom_layout(move |event| {
            callback(|| {
                let position = event.position_in_layout();
                event.set_layout_position(position.x, position.y)?;
                if let Some(plane) = weak.upgrade()
                    && let Some(result) =
                        plane.with_child(|child| event.layout_child(child, 0, 0))?
                {
                    result?;
                }
                Ok(())
            })
        });
        Ok(plane)
    }
    pub(crate) fn key(&self) -> Option<Key> {
        self.composite.borrow().as_ref().map(Composite::key)
    }
    fn replace_target(&self, owner: &Node) -> Result<u32> {
        let target = NativeAttachment::for_subtree(&self.outer, &self.inner)?;
        let id = target.id();
        *self.route.borrow_mut() = Some(crate::interop::sdk::ViewRoute::new(
            id,
            self.owner.clone(),
            &self.inner,
        ));
        if let Some(old) = self.target.replace(Some(target)) {
            owner.view_remove(old);
        }
        Ok(id)
    }
    pub(crate) fn update(&self, owner: &Node) -> Result<()> {
        if self.closed.get() || !owner.host.has_ui_context() {
            return Ok(());
        }
        if !self.context_live.get() {
            let id = self.replace_target(owner)?;
            *self.composite.borrow_mut() = Some(Composite::overlay(
                &self
                    .owner
                    .upgrade()
                    .ok_or(crate::render::node::Error::InvalidNode(owner.id))?,
                id,
            )?);
            self.context_live.set(true);
        }
        let composite = self.composite.borrow();
        let Some(composite) = composite.as_ref() else {
            return Ok(());
        };
        composite.update(owner)?;
        let texture_surface = composite.surface();
        if self
            .settings
            .borrow()
            .as_ref()
            .is_some_and(|previous| previous.texture_surface != texture_surface)
        {
            self.replace_target(owner)?;
        }
        let Some(target_id) = self.target.borrow().as_ref().map(NativeAttachment::id) else {
            return Ok(());
        };
        let frame = owner.frame.get();
        let settings = ViewOptions {
            target_id,
            width: frame.width as f64,
            height: frame.height as f64,
            texture_surface,
            ..ViewOptions::default()
        };
        let mut previous = self.settings.borrow_mut();
        if previous.as_ref() != Some(&settings) {
            *previous = Some(settings.clone());
            owner.view_update(settings);
            self.outer.borrow().mark_dirty(NodeDirtyFlag::NeedMeasure)?;
        }
        Ok(())
    }
    pub(crate) fn ready(&self, owner: &Node, generation: u32) -> Result<()> {
        if let Some(composite) = self.composite.borrow().as_ref()
            && composite.generation == generation
        {
            composite.ready();
        }
        self.update(owner)
    }
    pub(crate) fn restore(&self, owner: &Node) -> Result<()> {
        if let Some(target) = self.target.borrow().as_ref() {
            target.invalidate()?;
        }
        self.close(owner)
    }
    pub(crate) fn invalidate_context(&self) {
        self.settings.borrow_mut().take();
        self.route.borrow_mut().take();
        self.context_live.set(false);
        // SDK cleanup owns the borrowed edge until its synchronous retirement acknowledgement.
    }
    pub(crate) fn close(&self, owner: &Node) -> Result<()> {
        self.closed.set(true);
        self.route.borrow_mut().take();
        if let Some(old) = self.target.borrow_mut().take() {
            owner.view_remove(old);
        }
        self.composite.borrow_mut().take();
        Ok(())
    }
}
