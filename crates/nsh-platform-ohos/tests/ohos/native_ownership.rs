//! Run the production Refresh controller with native effects doubled. The
//! double enforces the binding's Rc and child-before-parent disposal contract.
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use crate::geometry::Size;

#[path = "../../src/scroll/refresh.rs"]
#[allow(dead_code)]
mod refresh;

type NativeNode = Rc<RefCell<Native>>;
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Debug)]
pub(crate) struct ControllerError(&'static str);
impl From<&'static str> for ControllerError {
    fn from(error: &'static str) -> Self {
        Self(error)
    }
}
impl std::fmt::Display for ControllerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Default)]
pub(crate) struct Native {
    this: Weak<RefCell<Self>>,
    parent: Weak<RefCell<Self>>,
    children: Vec<NativeNode>,
    disposals: Rc<Cell<usize>>,
    fail_dispose: bool,
    fail_remove: bool,
}
impl Native {
    fn new() -> NativeNode {
        Rc::new_cyclic(|this| {
            RefCell::new(Self {
                this: this.clone(),
                ..Self::default()
            })
        })
    }
    fn children(&self) -> &[NativeNode] {
        &self.children
    }
    fn raw_handle(&self) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
    fn dispose(&mut self) -> Result<()> {
        if self.this.strong_count() > 1 {
            return Err("active native owners");
        }
        if self.parent.strong_count() != 0 {
            return Err("still attached");
        }
        if self.fail_dispose {
            return Err("native disposal failed");
        }
        while let Some(child) = self.children.last().cloned() {
            self.remove_child(self.children.len() - 1)?;
            child.borrow_mut().dispose()?;
        }
        self.disposals.set(self.disposals.get() + 1);
        Ok(())
    }
    fn add_child(&mut self, child: NativeNode) -> Result<()> {
        self.insert_child(child, self.children.len())
    }
    fn insert_child(&mut self, child: NativeNode, index: usize) -> Result<()> {
        if child.borrow().parent.strong_count() != 0 {
            return Err("child already attached");
        }
        child.borrow_mut().parent = self.this.clone();
        self.children.insert(index, child);
        Ok(())
    }
    fn remove_child(&mut self, index: usize) -> Result<()> {
        if self.fail_remove {
            return Err("native detachment failed");
        }
        self.children.remove(index).borrow_mut().parent = Weak::new();
        Ok(())
    }
}

// Only the binding API used by the production controller is doubled here.
pub(crate) mod render {
    pub(crate) use crate::geometry;
    pub(crate) mod node {
        pub(crate) type NativeNode = super::super::NativeNode;
        pub(crate) type Result<T> = std::result::Result<T, super::super::ControllerError>;
    }
}
pub(crate) mod common {
    pub(crate) mod attribute {
        #[allow(dead_code)]
        pub(crate) enum ArkUINodeAttributeItem {
            Object(*mut std::ffi::c_void),
        }
    }
}
pub(crate) mod types {
    pub(crate) mod advanced {
        pub(crate) enum LengthMetricUnit {
            Vp,
        }
    }
    pub(crate) mod attribute {
        // Match the binding's attribute names in the production controller.
        #[allow(clippy::enum_variant_names)]
        pub(crate) enum ArkUINodeAttributeType {
            RefreshContent,
            RefreshOffset,
            RefreshPullDownRatio,
            RefreshPullToRefresh,
            RefreshRefreshing,
        }
    }
    pub(crate) mod event {
        pub(crate) enum NodeEventType {
            RefreshOnRefresh,
            RefreshOnOffsetChange,
        }
    }
}
pub(crate) mod event {
    pub(crate) mod inner_event {
        pub(crate) struct Event;
    }
}
pub(crate) mod component {
    pub(crate) mod attribute {
        use super::super::{Native, Result};
        use crate::{
            common::attribute::ArkUINodeAttributeItem,
            event::inner_event::Event,
            types::{
                advanced::LengthMetricUnit, attribute::ArkUINodeAttributeType, event::NodeEventType,
            },
        };

        pub(crate) trait ArkUICommonAttribute {
            fn set_length_metric_unit(&self, _: LengthMetricUnit) -> Result<()> {
                Ok(())
            }
            fn width(&self, _: f32) -> Result<()> {
                Ok(())
            }
            fn height(&self, _: f32) -> Result<()> {
                Ok(())
            }
            fn set_attribute(
                &self,
                _: ArkUINodeAttributeType,
                _: ArkUINodeAttributeItem,
            ) -> Result<()> {
                Ok(())
            }
            fn reset_attribute(&self, _: ArkUINodeAttributeType) -> Result<()> {
                Ok(())
            }
            fn set_f32_attribute(&self, _: ArkUINodeAttributeType, _: f32) -> Result<()> {
                Ok(())
            }
            fn set_bool_attribute(&self, _: ArkUINodeAttributeType, _: bool) -> Result<()> {
                Ok(())
            }
        }
        impl ArkUICommonAttribute for Native {}
        pub(crate) trait ArkUIEvent {
            fn on_event(&mut self, _: NodeEventType, _: impl Fn(&Event) + 'static) {}
            fn off_event(&mut self, _: NodeEventType) -> Result<()> {
                Ok(())
            }
        }
        impl ArkUIEvent for Native {}
    }
    pub(crate) mod built_in_component {
        use super::super::{Native, NativeNode, Result};
        pub(crate) struct Refresh;
        impl Refresh {
            #[allow(clippy::unnecessary_wraps)]
            pub(crate) fn new() -> Result<Self> {
                Ok(Self)
            }
            pub(crate) fn into_node(self) -> NativeNode {
                Native::new()
            }
        }
        pub(crate) struct Stack;
        impl Stack {
            #[allow(clippy::unnecessary_wraps)]
            pub(crate) fn new() -> Result<Self> {
                Ok(Self)
            }
            pub(crate) fn into_node(self) -> NativeNode {
                Native::new()
            }
        }
    }
}

struct Fixture {
    refresh: refresh::Refresh,
    wrapper: NativeNode,
    body: NativeNode,
}
impl Fixture {
    fn new() -> Self {
        let wrapper = Native::new();
        let body = Native::new();
        wrapper.borrow_mut().add_child(body.clone()).unwrap();
        let mut fixture = Self {
            refresh: refresh::Refresh::default(),
            wrapper,
            body,
        };
        fixture.enable();
        fixture
    }
    fn enable(&mut self) {
        self.refresh
            .update(
                &self.wrapper,
                &self.body,
                refresh::Config {
                    enabled: true,
                    refreshing: false,
                    viewport: Size::new(100.0, 80.0),
                    own_indicator: None,
                    threshold: 64.0,
                },
                |_| {},
                |_| {},
            )
            .unwrap();
        self.refresh.viewport(Size::new(100.0, 80.0)).unwrap();
    }
    fn node(&self) -> Weak<RefCell<Native>> {
        Rc::downgrade(&self.wrapper.borrow().children()[0])
    }
    fn close(&mut self) -> Result<()> {
        self.refresh
            .close(&self.wrapper, &self.body)
            .map_err(|error| error.0)
    }
    fn assert_body_preserved(&self) {
        assert_eq!(self.wrapper.borrow().children().len(), 1);
        assert!(Rc::ptr_eq(&self.wrapper.borrow().children()[0], &self.body));
        assert_eq!(self.body.borrow().disposals.get(), 0);
    }
}

#[test]
fn refresh_close_releases_the_field_owner_before_disposal() {
    let mut fixture = Fixture::new();
    let node = fixture.node();
    let disposals = node.upgrade().unwrap().borrow().disposals.clone();
    fixture.close().unwrap();
    fixture.assert_body_preserved();
    assert_eq!(disposals.get(), 1);
    assert!(node.upgrade().is_none());
    fixture.close().unwrap();
    assert_eq!(disposals.get(), 1);
}

#[test]
fn refresh_disposal_failure_restores_the_owner_for_retry() {
    let mut fixture = Fixture::new();
    let node = fixture.node();
    node.upgrade().unwrap().borrow_mut().fail_dispose = true;
    assert_eq!(fixture.close(), Err("native disposal failed"));
    fixture.assert_body_preserved();
    node.upgrade().unwrap().borrow_mut().fail_dispose = false;
    fixture.close().unwrap();
    assert!(node.upgrade().is_none());
}

#[test]
fn refresh_detach_failure_keeps_both_owners_until_retry() {
    let mut fixture = Fixture::new();
    let node = fixture.node();
    fixture.wrapper.borrow_mut().fail_remove = true;
    assert_eq!(fixture.close(), Err("native detachment failed"));
    assert_eq!(node.strong_count(), 2);
    fixture.wrapper.borrow_mut().fail_remove = false;
    fixture.close().unwrap();
    fixture.assert_body_preserved();
    assert!(node.upgrade().is_none());
}

#[test]
fn refresh_waits_for_an_external_native_owner() {
    let mut fixture = Fixture::new();
    let node = fixture.node();
    let external = node.upgrade().unwrap();
    assert_eq!(fixture.close(), Err("active native owners"));
    fixture.assert_body_preserved();
    drop(external);
    fixture.close().unwrap();
    assert!(node.upgrade().is_none());
}

#[test]
fn refresh_can_be_enabled_again_after_failed_disposal() {
    let mut fixture = Fixture::new();
    let node = fixture.node();
    node.upgrade().unwrap().borrow_mut().fail_dispose = true;
    assert!(fixture.close().is_err());
    fixture.enable();
    assert!(Weak::ptr_eq(&node, &fixture.node()));
    node.upgrade().unwrap().borrow_mut().fail_dispose = false;
    fixture.close().unwrap();
    fixture.assert_body_preserved();
    assert!(node.upgrade().is_none());
}
