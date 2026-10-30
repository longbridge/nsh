//! Deliver one owned touch clone across a texture export without changing the original event.
use std::{cell::Cell, rc::Rc};

use ohos_arkui_binding::component::attribute::ArkUIEvent;

use crate::render::node::{NativeNode, Node, Result, callback};

struct Posting<'a>(&'a Cell<bool>);
impl Drop for Posting<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
pub(crate) struct TextureInput;
impl TextureInput {
    pub(crate) fn install(owner: &Rc<Node>, outer: &NativeNode, inner: &NativeNode, overlay: bool) {
        let owner = Rc::downgrade(owner);
        let inner = Rc::downgrade(inner);
        outer.borrow_mut().on_touch_event(move |event| {
            callback(|| -> Result<()> {
                let (Some(owner), Some(inner)) = (owner.upgrade(), inner.upgrade()) else {
                    return Ok(());
                };
                if !owner.visible_in_tree()
                    || owner.texture_posting.get()
                    || !overlay
                        && owner
                            .wrapper
                            .borrow()
                            .as_ref()
                            .is_none_or(|wrapper| wrapper.composite.borrow().is_none())
                {
                    return Ok(());
                }
                let Some(input) = event.input_event() else {
                    return Ok(());
                };
                if input.source_type
                    == ohos_arkui_binding::arkui_input_binding::UIInputSourceType::Mouse
                {
                    return Ok(());
                }
                owner.texture_posting.set(true);
                let _posting = Posting(&owner.texture_posting);
                let forwarded = input.create_cloned_event().map_err(input_error)?;
                // C event accessors use the receiver's vp metric. Clone setters accept px.
                for index in 0..input.pointer_count() {
                    forwarded
                        .set_local_position_by_index(
                            input.pointer_x_by_index(index) * owner.scale,
                            input.pointer_y_by_index(index) * owner.scale,
                            index as i32,
                        )
                        .map_err(input_error)?;
                }
                // Injected handlers may synchronously retire their native node.
                let target = inner.borrow().raw_handle();
                // SAFETY: this UI-thread native callback retains `inner` for the full post;
                // the cloned event is owned and the target is a live node of this surface.
                if unsafe { forwarded.post_to_node_if_hit(target) }.map_err(input_error)? {
                    input
                        .pointer_set_stop_propagation(true)
                        .map_err(input_error)?;
                }
                Ok(())
            })
        });
    }
}
fn input_error(error: impl std::fmt::Display) -> crate::render::node::Error {
    napi_ohos::Error::from_reason(format!("OHOS texture input: {error}")).into()
}
