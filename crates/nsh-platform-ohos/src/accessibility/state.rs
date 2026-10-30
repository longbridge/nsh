//! Coalesced live-region updates and the desired accessibility snapshot.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use nsh_core::{Accessibility, LiveRegion};

use crate::render::node::{Node, Result};

pub(crate) struct State {
    pub(crate) spec: RefCell<Accessibility>,
    text: RefCell<Arc<str>>,
    dirty: Cell<bool>,
    spec_dirty: Cell<bool>,
    has_live_region: Cell<bool>,
    announced: RefCell<Option<String>>,
    #[cfg(feature = "api-23")]
    hidden_by_provider: Cell<bool>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            spec: RefCell::default(),
            text: RefCell::new(Arc::from("")),
            dirty: Cell::new(false),
            spec_dirty: Cell::new(false),
            has_live_region: Cell::new(false),
            announced: RefCell::new(None),
            #[cfg(feature = "api-23")]
            hidden_by_provider: Cell::new(false),
        }
    }
}
impl State {
    pub(crate) fn deactivate(&self, owner: &Node) {
        if self.has_live_region.replace(false) {
            owner
                .accessibility_live_regions
                .set(owner.accessibility_live_regions.get().saturating_sub(1));
        }
    }
    pub(crate) fn configure(&self, owner: &Rc<Node>, spec: &Accessibility) {
        if *self.spec.borrow() != *spec {
            let live = spec.live_region != LiveRegion::None;
            if self.has_live_region.replace(live) != live {
                let count = owner.accessibility_live_regions.get();
                owner.accessibility_live_regions.set(if live {
                    count + 1
                } else {
                    count.saturating_sub(1)
                });
            }
            *self.spec.borrow_mut() = spec.clone();
            self.changed(owner);
        }
    }
    pub(crate) fn text(&self, owner: &Rc<Node>, text: &Arc<str>) {
        if **self.text.borrow() != **text {
            *self.text.borrow_mut() = text.clone();
            self.text_changed(owner);
        }
    }
    fn text_changed(&self, owner: &Rc<Node>) {
        let needs_self_flush = owner.wrapper.borrow().is_some()
            || owner.input.borrow().is_some()
            || self.spec.borrow().live_region != LiveRegion::None
            || {
                #[cfg(feature = "api-23")]
                {
                    owner.accessibility_provider.hides_native_children()
                }
                #[cfg(not(feature = "api-23"))]
                {
                    false
                }
            };
        if needs_self_flush && !self.dirty.replace(true) {
            owner
                .accessibility_dirty
                .borrow_mut()
                .push(Rc::downgrade(owner));
        }
        let needs_ancestors =
            owner.accessibility_live_regions.get() > 0 || owner.accessibility_wrappers.get() > 0;
        if needs_ancestors {
            self.mark_ancestors(owner);
        }
    }
    pub(crate) fn changed(&self, owner: &Rc<Node>) {
        self.spec_dirty.set(true);
        if !self.dirty.replace(true) {
            owner
                .accessibility_dirty
                .borrow_mut()
                .push(Rc::downgrade(owner));
        }
        if owner.accessibility_live_regions.get() > 0 || owner.accessibility_wrappers.get() > 0 {
            self.mark_ancestors(owner);
        }
    }
    fn mark_ancestors(&self, owner: &Rc<Node>) {
        let mut parent = owner.parent_node.borrow().upgrade();
        while let Some(node) = parent {
            let state = &node.accessibility_state;
            if (state.spec.borrow().live_region != LiveRegion::None
                || node.wrapper.borrow().is_some())
                && !state.dirty.replace(true)
            {
                owner
                    .accessibility_dirty
                    .borrow_mut()
                    .push(Rc::downgrade(&node));
            }
            parent = node.parent_node.borrow().upgrade();
        }
    }
    pub(crate) fn label(&self, owner: &Node) -> String {
        if let Some(label) = &self.spec.borrow().label {
            return label.to_string();
        }
        if !self.text.borrow().is_empty() {
            return self.text.borrow().to_string();
        }
        owner
            .children
            .borrow()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .filter(|child| !child.accessibility_state.spec.borrow().hidden)
            .map(|child| child.accessibility_state.label(&child))
            .filter(|label| !label.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub(crate) fn flush(&self, owner: &Rc<Node>) -> Result<()> {
        self.dirty.set(false);
        let spec_dirty = self.spec_dirty.replace(false);
        if spec_dirty || owner.wrapper.borrow().is_some() {
            owner.update_wrapper()?;
        }
        if let Some(input) = owner.input.borrow().as_ref() {
            input.update();
            if !owner.wrapper_owns_semantics() {
                use ohos_arkui_binding::{
                    component::attribute::ArkUICommonAttribute,
                    types::{attribute::ArkUINodeAttributeType as A, view::AccessibilityMode},
                };
                // The editor owns selection, edit actions and its spoken value.
                // A grouped attachment container would consume directional focus.
                let native = owner.native.borrow();
                native.set_bool_attribute(A::AccessibilityGroup, false)?;
                native.set_i32_attribute(
                    A::AccessibilityMode,
                    if self.spec.borrow().hidden {
                        AccessibilityMode::DisabledForDescendants
                    } else {
                        AccessibilityMode::Disabled
                    }
                    .into(),
                )?;
            }
        }
        #[cfg(feature = "api-23")]
        if spec_dirty || owner.accessibility_provider.hides_native_children() {
            owner.accessibility_provider.configure(owner)?;
        }
        #[cfg(feature = "api-23")]
        if spec_dirty {
            self.provider_visibility(owner)?;
        }
        let spec = self.spec.borrow();
        if spec.live_region == LiveRegion::None || spec.hidden || !owner.visible_in_tree() {
            self.announced.borrow_mut().take();
            return Ok(());
        }
        let mut text = self.label(owner);
        if let Some(value) = crate::accessibility::value::spoken_value(&spec) {
            if !text.is_empty() {
                text.push_str(", ");
            }
            text.push_str(&value);
        }
        let previous = self.announced.borrow_mut().replace(text.clone());
        if previous.as_ref().is_some_and(|old| *old != text) && !text.is_empty() {
            owner.host.accessibility_live(
                owner.dispatcher.clone(),
                text,
                spec.live_region == LiveRegion::Assertive,
            )?;
        }
        Ok(())
    }
    #[cfg(feature = "api-23")]
    fn provider_visibility(&self, owner: &Rc<Node>) -> Result<()> {
        use ohos_arkui_binding::{
            component::attribute::ArkUICommonAttribute,
            types::{attribute::ArkUINodeAttributeType as A, view::AccessibilityMode as M},
        };
        let mut parent = owner.parent_node.borrow().upgrade();
        let mut hidden = false;
        while let Some(node) = parent {
            if node.accessibility_provider.hides_native_children() {
                hidden = true;
                break;
            }
            parent = node.parent_node.borrow().upgrade();
        }
        if hidden {
            self.hidden_by_provider.set(true);
            owner
                .mounted()
                .borrow()
                .set_i32_attribute(A::AccessibilityMode, M::DisabledForDescendants.into())?;
        } else if self.hidden_by_provider.replace(false) {
            if owner.wrapper.borrow().is_some() {
                owner
                    .mounted()
                    .borrow()
                    .reset_attribute(A::AccessibilityMode)?;
            } else {
                let spec = self.spec.borrow();
                let mode = if spec.hidden {
                    M::DisabledForDescendants
                } else if owner.input.borrow().is_some() {
                    M::Disabled
                } else if owner.accessibility_provider.hides_native_children() {
                    M::Enabled
                } else {
                    match spec.accessible {
                        Some(true) => M::Enabled,
                        Some(false) => M::Disabled,
                        None => M::Auto,
                    }
                };
                owner
                    .native
                    .borrow()
                    .set_i32_attribute(A::AccessibilityMode, mode.into())?;
            }
        }
        Ok(())
    }
}
