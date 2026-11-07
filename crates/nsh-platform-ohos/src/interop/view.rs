//! On-demand ArkTS wrappers retain visual and accessibility composition around C subtrees.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use nsh_core::{AccessibilityRole as R, Checked, EventKind};
use ohos_arkui_binding::{
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::Custom,
    },
    types::advanced::{LengthMetricUnit, NodeDirtyFlag},
};

use crate::{
    interop::{
        attachment::NativeAttachment,
        sdk::{MenuItem, ViewOptions, ViewRoute},
    },
    render::node::{NativeNode, Node, Result, callback},
};

pub(crate) struct Wrapper {
    closed: std::cell::Cell<bool>,
    pub(crate) outer: NativeNode,
    target: RefCell<Option<NativeAttachment>>,
    route: RefCell<Option<ViewRoute>>,
    context_live: Cell<bool>,
    owner: std::rc::Weak<Node>,
    settings: std::cell::RefCell<Option<ViewOptions>>,
    pub(crate) composite: std::cell::RefCell<Option<crate::render::composition::worker::Composite>>,
    /// The accessibility mode and grouping last given the wrapped node
    /// (`None`: to give again, after the node's own props reset them).
    semantics: Cell<Option<(i32, bool)>>,
    /// The frame size the outer node was last asked to measure at.
    measured: Cell<Option<(f32, f32)>>,
}
impl Wrapper {
    pub(crate) fn invalidate_context(&self) {
        self.settings.borrow_mut().take();
        self.route.borrow_mut().take();
        self.context_live.set(false);
        // Keep the old target through SDK lifecycle cleanup; replacement queues its final owner.
    }
    fn replace_target(&self, owner: &Node) -> Result<u32> {
        let target = NativeAttachment::for_subtree(&self.outer, &owner.native)?;
        let id = target.id();
        if let Some(old) = self.target.replace(Some(target)) {
            owner.view_remove(old);
        }
        // The native target is the unique, nonreused callback generation.
        let weak = self.owner.clone();
        *self.route.borrow_mut() = Some(ViewRoute::new(id, weak, &owner.native));
        Ok(id)
    }
    fn ensure_context(&self, owner: &Node) -> Result<()> {
        if !self.context_live.get() {
            let id = self.replace_target(owner)?;
            if self.composite.borrow_mut().take().is_some() {
                *self.composite.borrow_mut() =
                    Some(crate::render::composition::worker::Composite::new(
                        &self
                            .owner
                            .upgrade()
                            .ok_or(crate::render::node::Error::InvalidNode(owner.id))?,
                        id,
                    )?);
            }
            self.context_live.set(true);
        }
        Ok(())
    }
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
    fn abort(self) {
        let cleanup = || -> Result<()> {
            if let Some(target) = self.target.borrow().as_ref() {
                target.invalidate()?;
            }
            self.outer.borrow().remove_from_parent()?;
            self.outer.borrow_mut().dispose()?;
            Ok(())
        };
        if let Err(error) = cleanup() {
            log::error!("Failed to release an unmounted OHOS wrapper: {error}");
            // A failed native cleanup must retain the complete owner, not just its target ID.
            std::mem::forget(self);
        }
    }
    fn new(owner: &Rc<Node>) -> Result<Self> {
        let outer = Custom::new()?.into_node();
        let setup = || -> Result<NativeAttachment> {
            outer
                .borrow()
                .set_length_metric_unit(LengthMetricUnit::Vp)?;
            Ok(NativeAttachment::for_subtree(&outer, &owner.native)?)
        };
        let target = match setup() {
            Ok(target) => target,
            Err(error) => {
                if let Err(cleanup) = outer.borrow_mut().dispose() {
                    log::error!("Failed to release a new OHOS wrapper: {cleanup}");
                    std::mem::forget(outer.clone());
                }
                return Err(error);
            }
        };
        crate::interop::texture_input::TextureInput::install(owner, &outer, &owner.native, false);
        let route = ViewRoute::new(target.id(), Rc::downgrade(owner), &owner.native);
        let weak = Rc::downgrade(owner);
        outer.borrow_mut().on_custom_draw(move |event| {
            if let Some(owner) = weak.upgrade()
                && let Some(wrapper) = owner.wrapper.borrow().as_ref()
                && let Some(raw) = event
                    .draw_context_in_draw()
                    .and_then(|context| context.canvas())
            {
                // SAFETY: ArkUI owns this canvas until this draw callback returns.
                let canvas = unsafe { ohos_drawing_binding::Canvas::from_raw_borrowed(raw) };
                if owner.scene_active() {
                    if let Some(scene) = owner.scene.borrow().as_ref() {
                        scene.draw(owner.id, &canvas);
                    }
                } else if let Some(composite) = wrapper.composite.borrow().as_ref() {
                    composite.draw(&canvas);
                }
            }
        });
        let weak = Rc::downgrade(owner);
        outer.borrow_mut().on_custom_measure(move |event| {
            callback(|| {
                if let Some(owner) = weak.upgrade() {
                    if let Some(modal) = owner.modal.borrow().as_ref() {
                        let size = modal.measure(event)?;
                        let mut raw = owner.raw_frame.get();
                        raw.width = size.width;
                        raw.height = size.height;
                        owner.set_frame(raw)?;
                    }
                    let frame = owner.frame.get();
                    let mut width = (frame.width * owner.scale).round() as i32;
                    let mut height = (frame.height * owner.scale).round() as i32;
                    if !owner.clips_children() && !owner.children.borrow().is_empty() {
                        width = width.max(1);
                        height = height.max(1);
                    }
                    event.set_measured_size(width, height)?;
                    if let Some(wrapper) = owner.wrapper.borrow().as_ref()
                        && let Some(result) = wrapper.with_child(|node| {
                            crate::render::layout::ChildMeasure::new(event).child(node)
                        })?
                    {
                        result?;
                    }
                }
                Ok(())
            })
        });
        let weak = Rc::downgrade(owner);
        outer.borrow_mut().on_custom_layout(move |event| {
            callback(|| {
                let position = event.position_in_layout();
                event.set_layout_position(position.x, position.y)?;
                if let Some(owner) = weak.upgrade()
                    && let Some(wrapper) = owner.wrapper.borrow().as_ref()
                    && let Some(result) = wrapper.with_child(|node| {
                        let (x, y) = wrapper
                            .composite
                            .borrow()
                            .as_ref()
                            .map_or((0.0, 0.0), |composite| composite.origin());
                        event.layout_child(
                            node,
                            (x * owner.scale).round() as i32,
                            (y * owner.scale).round() as i32,
                        )
                    })?
                {
                    result?;
                }
                Ok(())
            })
        });
        Ok(Self {
            closed: std::cell::Cell::new(false),
            outer,
            target: RefCell::new(Some(target)),
            route: RefCell::new(Some(route)),
            context_live: Cell::new(true),
            owner: Rc::downgrade(owner),
            settings: std::cell::RefCell::default(),
            composite: std::cell::RefCell::default(),
            semantics: Cell::new(None),
            measured: Cell::new(None),
        })
    }
    /// Sends the ArkTS view its settings; whether they changed.
    fn update(&self, owner: &Node) -> Result<bool> {
        if self.closed.get() || !owner.host.has_ui_context() {
            return Ok(false);
        }
        self.ensure_context(owner)?;
        let texture_surface = self
            .composite
            .borrow()
            .as_ref()
            .and_then(|composite| composite.surface());
        if self
            .settings
            .borrow()
            .as_ref()
            .is_some_and(|previous| previous.texture_surface != texture_surface)
        {
            self.replace_target(owner)?;
        }
        let Some(target_id) = self.target.borrow().as_ref().map(NativeAttachment::id) else {
            return Ok(false);
        };
        let spec = owner.accessibility_state.spec.borrow();
        let frame = owner.frame.get();
        let settings = ViewOptions {
            texture_insets: self
                .composite
                .borrow()
                .as_ref()
                .map(|composite| composite.insets()),
            texture_surface,
            target_id,
            test_id: owner.interaction.test_id(),
            width: frame.width as f64,
            height: frame.height as f64,
            hue: 0.0,
            semantic: owns_semantics(&spec),
            clickable: owner.interaction.mask().contains(EventKind::Press)
                || spec
                    .actions
                    .iter()
                    .any(|action| action.name.as_ref() == "activate"),
            disabled: spec.state.disabled,
            accessible: spec.accessible,
            hidden: spec.hidden,
            label: owner.accessibility_state.label(owner),
            hint: spec.hint.as_deref().unwrap_or("").to_owned(),
            value: crate::accessibility::value::spoken_value(&spec).unwrap_or_default(),
            role: role(spec.role).to_owned(),
            checked: spec.state.checked.map(|value| value == Checked::True),
            selected: spec.state.selected,
            api23: crate::get_capabilities().api23,
            api26: crate::get_capabilities().api26,
            actions: if crate::get_capabilities().api26 {
                spec.actions
                    .iter()
                    .take(16)
                    .map(|action| MenuItem {
                        kind: "action".into(),
                        name: action.name.to_string(),
                        label: action.label.as_deref().unwrap_or(&action.name).to_owned(),
                    })
                    .collect()
            } else {
                Vec::new()
            },
            menu: owner.selection_menu(),
        };
        let mut previous = self.settings.borrow_mut();
        if previous.as_ref() == Some(&settings) {
            return Ok(false);
        }
        *previous = Some(settings.clone());
        owner.view_update(settings);
        Ok(true)
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
impl Node {
    pub(crate) fn wrapper_owns_semantics(&self) -> bool {
        self.wrapper.borrow().is_some() && owns_semantics(&self.accessibility_state.spec.borrow())
    }
    pub(crate) fn accessibility_id(&self) -> Result<i32> {
        use ohos_arkui_binding::types::attribute::ArkUINodeAttributeType as A;
        let spec = self.accessibility_state.spec.borrow();
        if owns_semantics(&spec)
            && let Some(wrapper) = self.wrapper.borrow().as_ref()
            && let Some(id) =
                wrapper.with_child(|node| node.i32_attribute(A::AccessibilityId, 0))?
        {
            return Ok(id?);
        }
        if let Some(input) = self.input.borrow().as_ref()
            && let Some(id) = input.accessibility_id()?
        {
            return Ok(id);
        }
        Ok(self.native.borrow().i32_attribute(A::AccessibilityId, 0)?)
    }
    pub(crate) fn mounted(&self) -> NativeNode {
        self.wrapper
            .borrow()
            .as_ref()
            .map_or_else(|| self.native.clone(), |wrapper| wrapper.outer.clone())
    }
    pub(crate) fn ensure_wrapper(self: &Rc<Self>) -> Result<()> {
        if self
            .wrapper
            .borrow()
            .as_ref()
            .is_some_and(|wrapper| wrapper.closed.get())
        {
            return Ok(());
        }
        let spec = self.accessibility_state.spec.borrow();
        if crate::get_capabilities().api26 && spec.actions.len() > 16 {
            log::warn!("OHOS accessibility: showing the first 16 custom actions");
        }
        let non_blur_filter = self.style.borrow().effects.as_ref().is_some_and(|effects| {
            effects
                .filters
                .iter()
                .any(|filter| !matches!(filter, nsh_core::Filter::Blur(_)))
        });
        let needed = self.kind == nsh_core::ViewKind::Modal
            || self.scene_active() && self.has_native_paint()
            || self.selectable()
            || owns_semantics(&spec)
            || non_blur_filter;
        drop(spec);
        if needed && self.wrapper.borrow().is_none() {
            self.enable_layout();
            self.native.borrow().reset_attribute(
                ohos_arkui_binding::types::attribute::ArkUINodeAttributeType::LayoutRect,
            )?;
            self.native.borrow().reset_attribute(
                ohos_arkui_binding::types::attribute::ArkUINodeAttributeType::Position,
            )?;
            let parent = self.mount_parent.borrow().upgrade();
            let index = parent
                .as_ref()
                .map(|parent| {
                    parent
                        .borrow()
                        .children()
                        .iter()
                        .position(|child| Rc::ptr_eq(child, &self.native))
                        .ok_or(crate::render::node::Error::InvalidNode(self.id))
                })
                .transpose()?;
            let wrapper = Wrapper::new(self)?;
            if let Some((parent, index)) = parent.zip(index) {
                let mut parent = parent.borrow_mut();
                if let Err(error) = parent.remove_child(index) {
                    wrapper.abort();
                    return Err(error.into());
                }
                if let Err(error) = parent.insert_child(wrapper.outer.clone(), index) {
                    let restored = parent.insert_child(self.native.clone(), index);
                    wrapper.abort();
                    restored?;
                    return Err(error.into());
                }
            }
            *self.wrapper.borrow_mut() = Some(wrapper);
            self.accessibility_wrappers
                .set(self.accessibility_wrappers.get() + 1);
            self.invalidate_mounted_geometry();
            self.update_layout_bounds()?;
            // The wrapper owns this node's semantics; native descendants keep their own entries.
            #[cfg(feature = "api-23")]
            self.accessibility_provider.clear();
        }
        let needs_texture = self.scene_active() && self.has_native_paint() || non_blur_filter;
        if !needs_texture
            && let Some(wrapper) = self.wrapper.borrow().as_ref()
            && wrapper.composite.borrow_mut().take().is_some()
        {
            wrapper.outer.borrow().reset_attribute(
                ohos_arkui_binding::types::attribute::ArkUINodeAttributeType::Transform,
            )?;
            wrapper.outer.borrow().opacity(1.0)?;
            wrapper.outer.borrow().set_i32_attribute(
                ohos_arkui_binding::types::attribute::ArkUINodeAttributeType::Visibility,
                0,
            )?;
        }
        if needs_texture
            && let Some(wrapper) = self.wrapper.borrow().as_ref()
            && wrapper.composite.borrow().is_none()
        {
            let composite = crate::render::composition::worker::Composite::new(
                self,
                wrapper
                    .target
                    .borrow()
                    .as_ref()
                    .ok_or(crate::render::node::Error::Unsupported(
                        "wrapper target retired",
                    ))?
                    .id(),
            )?;
            self.native.borrow().reset_attribute(
                ohos_arkui_binding::types::attribute::ArkUINodeAttributeType::Transform,
            )?;
            self.native.borrow().opacity(1.0)?;
            self.native.borrow().set_i32_attribute(
                ohos_arkui_binding::types::attribute::ArkUINodeAttributeType::Visibility,
                0,
            )?;
            *wrapper.composite.borrow_mut() = Some(composite);
        }
        if let Some(wrapper) = self.wrapper.borrow().as_ref() {
            let spec = self.accessibility_state.spec.borrow();
            let semantic = owns_semantics(&spec);
            use ohos_arkui_binding::types::view::AccessibilityMode as Mode;
            let mode = if semantic && spec.accessible == Some(true) {
                // The ArkTS group is the single semantic entry for this subtree.
                Mode::DisabledForDescendants
            } else if semantic {
                Mode::Disabled
            } else if spec.hidden {
                Mode::DisabledForDescendants
            } else {
                match spec.accessible {
                    Some(true) => Mode::Enabled,
                    Some(false) => Mode::Disabled,
                    None => Mode::Auto,
                }
            };
            let next = (i32::from(mode), !semantic && spec.accessible == Some(true));
            // Style and text updates come here too: give the wrapped node
            // its semantics only when they change or its own props reset them.
            if wrapper.semantics.replace(Some(next)) != Some(next) {
                use ohos_arkui_binding::types::attribute::ArkUINodeAttributeType as A;
                let native = self.native.borrow();
                // The wrapper carries the test ID.
                native.reset_attribute(A::Id)?;
                native.set_i32_attribute(A::AccessibilityMode, next.0)?;
                native.set_bool_attribute(A::AccessibilityGroup, next.1)?;
            }
        }
        self.update_wrapper()
    }
    pub(crate) fn update_wrapper(&self) -> Result<()> {
        if let Some(scroll) = self.scroll.borrow().as_ref()
            && let Some(plane) = scroll.overlay.borrow().as_ref()
        {
            plane.update(self)?;
        }
        if let Some(wrapper) = self
            .wrapper
            .borrow()
            .as_ref()
            .filter(|wrapper| !wrapper.closed.get())
        {
            let composite = if let Some(composite) = wrapper.composite.borrow().as_ref() {
                composite.update(self)?;
                true
            } else {
                false
            };
            let changed = wrapper.update(self)?;
            // The outer node measures at the frame's size (a modal at its
            // content's) and places the ArkTS view at the composite's origin:
            // measure again only when one of those, or what the view shows,
            // may have changed.
            let frame = self.frame.get();
            let size = Some((frame.width, frame.height));
            let modal = self.modal.borrow().is_some();
            if changed || composite || modal || wrapper.measured.replace(size) != size {
                wrapper
                    .outer
                    .borrow()
                    .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
            }
        }
        Ok(())
    }
    /// The wrapped node's own props rewrote its test ID and accessibility
    /// mode: the wrapper gives them again at the next update.
    pub(crate) fn wrapper_semantics_reset(&self) {
        if let Some(wrapper) = self.wrapper.borrow().as_ref() {
            wrapper.semantics.set(None);
        }
    }
    pub(crate) fn visual_node(&self) -> NativeNode {
        self.wrapper
            .borrow()
            .as_ref()
            .filter(|wrapper| wrapper.composite.borrow().is_some())
            .map_or_else(|| self.native.clone(), |wrapper| wrapper.outer.clone())
    }
    pub(crate) fn composite_ready(&self, generation: u32) -> Result<()> {
        if let Some(scroll) = self.scroll.borrow().as_ref()
            && let Some(plane) = scroll.overlay.borrow().as_ref()
        {
            plane.ready(self, generation)?;
        }
        if let Some(wrapper) = self.wrapper.borrow().as_ref()
            && let Some(composite) = wrapper
                .composite
                .borrow()
                .as_ref()
                .filter(|composite| composite.generation == generation)
        {
            composite.ready();
            wrapper
                .outer
                .borrow()
                .mark_dirty(NodeDirtyFlag::NeedRender)?;
        }
        self.update_wrapper()?;
        self.invalidate_scene();
        Ok(())
    }
}
fn role(role: R) -> &'static str {
    match role {
        R::None => "",
        R::Button | R::ImageButton | R::KeyboardKey => "button",
        R::Checkbox => "checkbox",
        R::Switch => "switch",
        R::Radio => "radio",
        R::Adjustable | R::Scrollbar => "slider",
        R::Image => "image",
        R::Link => "link",
        R::List => "list",
        R::Grid => "grid",
        R::Progressbar => "progress",
        R::Search => "search",
        R::Menuitem => "menuitem",
        _ => "text",
    }
}

fn owns_semantics(spec: &nsh_core::Accessibility) -> bool {
    spec.state.checked.is_some()
        || crate::get_capabilities().api26
            && spec.actions.iter().any(|action| {
                !matches!(
                    action.name.as_ref(),
                    "activate" | "longpress" | "copy" | "cut" | "paste" | "increment" | "decrement"
                )
            })
}
