//! Optional refresh chrome around the identity-stable native scroll body.

use std::rc::Rc;

use ohos_arkui_binding::{
    common::attribute::ArkUINodeAttributeItem,
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::{Refresh as NativeRefresh, Stack},
    },
    event::inner_event::Event,
    types::{
        advanced::LengthMetricUnit, attribute::ArkUINodeAttributeType as A,
        event::NodeEventType as E,
    },
};

use crate::render::{
    geometry::Size,
    node::{NativeNode, Result},
};

#[derive(Default)]
pub(super) struct Refresh {
    node: Option<NativeNode>,
    indicator: Option<NativeNode>,
    listening: bool,
    offset_listening: bool,
}

pub(super) struct Config {
    pub(super) enabled: bool,
    pub(super) refreshing: bool,
    pub(super) viewport: Size,
    pub(super) own_indicator: Option<f32>,
    pub(super) threshold: f32,
}

impl Refresh {
    pub(super) fn is_mounted(&self) -> bool {
        self.node.is_some()
    }

    pub(super) fn update(
        &mut self,
        wrapper: &NativeNode,
        body: &NativeNode,
        config: Config,
        callback: impl Fn(&Event) + 'static,
        on_offset: impl Fn(&Event) + 'static,
    ) -> Result<()> {
        let wanted = config.enabled || config.refreshing;
        if !wanted {
            return self.close(wrapper, body);
        }
        if self.node.is_none() {
            let node = NativeRefresh::new()?.into_node();
            // Retain the owner before moving any native edge. Failed native
            // operations can then be retried without releasing a live subtree.
            self.node = Some(node);
        }
        let Some(node) = self.node.as_ref().cloned() else {
            return Ok(());
        };
        node.borrow().set_length_metric_unit(LengthMetricUnit::Vp)?;
        node.borrow().width(config.viewport.width)?;
        node.borrow().height(config.viewport.height)?;
        if let Some(height) = config.own_indicator {
            if self.indicator.is_none() {
                let indicator = Stack::new()?.into_node();
                indicator
                    .borrow()
                    .set_length_metric_unit(LengthMetricUnit::Vp)?;
                self.indicator = Some(indicator);
            }
            if let Some(indicator) = self.indicator.as_ref() {
                indicator.borrow().width(config.viewport.width)?;
                indicator.borrow().height(height)?;
                node.borrow().set_attribute(
                    A::RefreshContent,
                    ArkUINodeAttributeItem::Object(indicator.borrow().raw_handle().cast()),
                )?;
            }
        } else if let Some(indicator) = self.indicator.take() {
            node.borrow().reset_attribute(A::RefreshContent)?;
            indicator.borrow_mut().dispose()?;
        }
        node.borrow()
            .set_f32_attribute(A::RefreshOffset, config.threshold)?;
        // Keep the existing trigger resistance in small embedded views.
        node.borrow()
            .set_f32_attribute(A::RefreshPullDownRatio, 0.5)?;
        node.borrow()
            .set_bool_attribute(A::RefreshPullToRefresh, config.enabled)?;
        node.borrow()
            .set_bool_attribute(A::RefreshRefreshing, config.refreshing)?;
        if config.enabled && !self.listening {
            node.borrow_mut().on_event(E::RefreshOnRefresh, callback);
            self.listening = true;
        } else if !config.enabled && self.listening {
            node.borrow_mut().off_event(E::RefreshOnRefresh)?;
            self.listening = false;
        }
        if !self.offset_listening {
            node.borrow_mut()
                .on_event(E::RefreshOnOffsetChange, on_offset);
            self.offset_listening = true;
        }
        self.wrap(wrapper, body, &node)?;
        Ok(())
    }

    pub(super) fn close(&mut self, wrapper: &NativeNode, body: &NativeNode) -> Result<()> {
        let Some(node) = self.node.take() else {
            return Ok(());
        };
        let closed = (|| -> Result<()> {
            node.borrow()
                .set_bool_attribute(A::RefreshPullToRefresh, false)?;
            node.borrow()
                .set_bool_attribute(A::RefreshRefreshing, false)?;
            if self.listening {
                node.borrow_mut().off_event(E::RefreshOnRefresh)?;
                self.listening = false;
            }
            if self.offset_listening {
                node.borrow_mut().off_event(E::RefreshOnOffsetChange)?;
                self.offset_listening = false;
            }
            if let Some(indicator) = self.indicator.take() {
                node.borrow().reset_attribute(A::RefreshContent)?;
                indicator.borrow_mut().dispose()?;
            }
            Self::move_body(&node, wrapper, body)?;
            Self::detach(wrapper, &node)?;
            node.borrow_mut().dispose()?;
            Ok(())
        })();
        if let Err(error) = closed {
            self.node = Some(node);
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn viewport(&self, size: Size) -> Result<()> {
        if let Some(node) = self.node.as_ref() {
            node.borrow().width(size.width)?;
            node.borrow().height(size.height)?;
        }
        if let Some(indicator) = self.indicator.as_ref() {
            indicator.borrow().width(size.width)?;
        }
        Ok(())
    }

    fn parent(&self, wrapper: &NativeNode, body: &NativeNode) -> NativeNode {
        self.node
            .as_ref()
            .filter(|node| {
                !Rc::ptr_eq(node, body) && (contains(node, body) || contains(wrapper, node))
            })
            .unwrap_or(wrapper)
            .clone()
    }

    pub(super) fn wrap(
        &self,
        wrapper: &NativeNode,
        body: &NativeNode,
        outer: &NativeNode,
    ) -> Result<()> {
        // Refresh is a direct child of the stable Stack; the optional cross
        // scroll belongs to the active body parent, including during recovery.
        let parent = if self
            .node
            .as_ref()
            .is_some_and(|node| Rc::ptr_eq(node, outer))
        {
            wrapper.clone()
        } else {
            let mounted_parent = self.parent(wrapper, outer);
            if contains(&mounted_parent, outer) {
                mounted_parent
            } else {
                self.parent(wrapper, body)
            }
        };
        Self::move_body(&parent, outer, body)?;
        if !contains(&parent, outer) {
            let mounted = parent.borrow_mut().add_child(outer.clone());
            if let Err(error) = mounted {
                if let Err(rollback) = Self::move_body(outer, &parent, body) {
                    log::warn!("OHOS scroll reparent rollback failed: {rollback}");
                }
                return Err(error.into());
            }
        }
        Ok(())
    }

    fn detach(parent: &NativeNode, child: &NativeNode) -> Result<Option<usize>> {
        let index = parent
            .borrow()
            .children()
            .iter()
            .position(|node| Rc::ptr_eq(node, child));
        if let Some(index) = index {
            parent.borrow_mut().remove_child(index)?;
        }
        Ok(index)
    }

    fn move_body(from: &NativeNode, to: &NativeNode, body: &NativeNode) -> Result<()> {
        if contains(to, body) {
            return Ok(());
        }
        let index = Self::detach(from, body)?;
        if let Err(error) = to.borrow_mut().add_child(body.clone()) {
            if let Some(index) = index
                && let Err(rollback) = from.borrow_mut().insert_child(body.clone(), index)
            {
                log::warn!("OHOS scroll body rollback failed: {rollback}");
            }
            return Err(error.into());
        }
        Ok(())
    }
}

fn contains(parent: &NativeNode, child: &NativeNode) -> bool {
    parent
        .borrow()
        .children()
        .iter()
        .any(|node| Rc::ptr_eq(node, child))
}
