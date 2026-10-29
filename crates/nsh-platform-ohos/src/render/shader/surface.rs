//! A shader view's XComponent: a native surface first among the view's
//! children (under the app's), the size of its box and clipped to its
//! corners, whose native window the shader thread draws into.
//!
//! It is kept (in the tree, its callbacks unregistered) until the shader
//! thread has dropped its surface of the window, as a video's is until its
//! player lets it go.

use std::{cell::Cell, ptr::NonNull, sync::mpsc};

use nsh_core::NodeId;
use ohos_arkui_binding::{
    common::{attribute::ArkUINodeAttributeNumber as Number, node_registry},
    component::{attribute::ArkUICommonAttribute, built_in_component::XComponent},
    types::{
        advanced::LengthMetricUnit,
        attribute::ArkUINodeAttributeType as A,
        view::{AccessibilityMode, HitTestMode},
    },
};
use ohos_native_window_binding::NativeWindow;
use ohos_xcomponent_binding::NativeXComponent;

use super::Command;
use crate::render::node::{NativeNode, Result};

/// A box: its size in points and its corner radii (top-left first).
type Placement = ((f32, f32), [f32; 4]);

pub(super) struct Surface {
    pub(super) native: NativeNode,
    xcomponent: NativeXComponent,
    /// The box last given it.
    applied: Cell<Option<Placement>>,
}

impl Surface {
    pub(super) fn new(id: NodeId, send: mpsc::Sender<Command>) -> Result<Surface> {
        let native = XComponent::new()?.into_node();
        {
            let node = native.borrow();
            node.set_length_metric_unit(LengthMetricUnit::Vp)?;
            node.set_string_attribute(A::XComponentId, format!("nsh-shader-{id}"))?;
            // A picture: presses and screen readers go to the view it is in.
            node.set_i32_attribute(A::HitTestBehavior, HitTestMode::None.into())?;
            node.set_i32_attribute(A::AccessibilityMode, AccessibilityMode::Disabled.into())?;
            node.set_bool_attribute(A::Clip, true)?;
        }
        let xcomponent = native
            .borrow()
            .native_xcomponent()
            .ok_or_else(|| napi_ohos::Error::from_reason("shader XComponent is unavailable"))?;
        let created = send.clone();
        xcomponent.on_surface_created(move |_, window| {
            let raw = NonNull::new(window.0)
                .ok_or_else(|| napi_ohos::Error::from_reason("shader window is null"))?;
            // SAFETY: the SDK lends a live window for this callback; the
            // XComponent that produces it is kept until the shader thread has
            // dropped its surface of it (`Shaders::collect`).
            let window = unsafe { NativeWindow::retain_from_ptr(raw) }
                .map_err(|error| napi_ohos::Error::from_reason(error.to_string()))?;
            let _ = created.send(Command::Window(id, Some(window)));
            Ok(())
        });
        xcomponent.on_surface_destroyed(move |_, window| {
            let _ = send.send(Command::WindowGone(id, window.0 as usize));
            Ok(())
        });
        xcomponent.register_callback()?;
        Ok(Surface {
            native,
            xcomponent,
            applied: Cell::new(None),
        })
    }

    /// Gives it the view's size in points and its corner radii.
    pub(super) fn place(&self, size: (f32, f32), radii: [f32; 4]) -> Result<bool> {
        if self.applied.get() == Some((size, radii)) {
            return Ok(false);
        }
        self.applied.set(Some((size, radii)));
        let node = self.native.borrow();
        node.width(size.0)?;
        node.height(size.1)?;
        node.set_number_slice_attribute(A::BorderRadius, &radii.map(Number::Float))?;
        Ok(true)
    }

    /// Takes it out of `parent` and releases it: once the shader thread no
    /// longer draws into its window.
    pub(super) fn dispose(&self, parent: Option<&NativeNode>) -> Result<()> {
        self.xcomponent.unregister_callbacks();
        if !self.native.borrow().is_owned() {
            return Ok(());
        }
        node_registry::unregister(&self.native.borrow());
        let index = parent.and_then(|parent| {
            parent
                .borrow()
                .children()
                .iter()
                .position(|child| std::rc::Rc::ptr_eq(child, &self.native))
        });
        match (parent, index) {
            (Some(parent), Some(index)) => {
                if self.native.borrow().parent()?.is_some() {
                    parent.borrow_mut().remove_child(index)?;
                } else {
                    parent.borrow_mut().children_mut().remove(index);
                }
            }
            _ => self.native.borrow().remove_from_parent()?,
        }
        self.native.borrow_mut().dispose()?;
        Ok(())
    }
}
