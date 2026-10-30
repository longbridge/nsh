//! Constraints for native children of an engine-sized custom node.

use ohos_arkui_binding::{
    api::node_custom_event::{LayoutConstraintHandle, NodeCustomEvent},
    common::node::ArkUINode,
};

use super::node::{Error, Result};

/// One native constraint snapshot per measurement pass. A parent's minimum
/// size describes the parent; its children can be smaller. Native children
/// retain their own width/height rules within the available maximum size.
pub(crate) struct ChildMeasure<'event> {
    event: &'event NodeCustomEvent,
    constraint: Option<LayoutConstraintHandle<'event>>,
}

impl<'event> ChildMeasure<'event> {
    pub(crate) fn new(event: &'event NodeCustomEvent) -> Self {
        Self {
            event,
            constraint: None,
        }
    }

    pub(crate) fn child(&mut self, child: &ArkUINode) -> Result<()> {
        let constraint = match &mut self.constraint {
            Some(constraint) => constraint,
            None => {
                let mut constraint = self
                    .event
                    .layout_constraint_in_measure()
                    .ok_or(Error::Unsupported("native measurement without constraints"))?;
                constraint.loosen();
                self.constraint.insert(constraint)
            }
        };
        self.event
            .measure_child_with_constraint(child, constraint)?;
        Ok(())
    }
}
