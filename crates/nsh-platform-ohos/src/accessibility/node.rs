//! Native metadata on the same ArkUI nodes used for rendering and hit testing.
use std::rc::Rc;

use nsh_core::{
    Accessibility, AccessibilityRole as R, Checked, EventKind, EventPayload, PayloadValue,
};
use ohos_arkui_binding::{
    api::attribute_option::{AccessibilityState, AccessibilityValue},
    component::attribute::{ArkUICommonAttribute, ArkUIEvent},
    types::{attribute::ArkUINodeAttributeType as A, node_type::ArkUINodeType as K},
};

use crate::render::node::{Node, Result};

pub(crate) fn configure(owner: &Rc<Node>, old: &Accessibility, new: &Accessibility) -> Result<()> {
    owner.accessibility_state.configure(owner, new);
    let native = owner.native.borrow();
    if old.role != new.role {
        let role = match new.role {
            R::None => None,
            R::Button | R::ImageButton | R::KeyboardKey | R::Link | R::Menuitem | R::Option => {
                Some(K::Button)
            }
            R::Image => Some(K::Image),
            R::Search => Some(K::TextInput),
            R::Adjustable | R::Scrollbar => Some(K::Slider),
            R::Checkbox | R::Switch => Some(K::Checkbox),
            R::Radio => Some(K::Radio),
            R::Progressbar => Some(K::Progress),
            R::List | R::Listbox => Some(K::List),
            R::Grid => Some(K::Grid),
            _ => Some(K::Text),
        };
        if let Some(role) = role {
            native.set_u32_attribute(A::AccessibilityRole, role.into())?;
        } else {
            native.reset_attribute(A::AccessibilityRole)?;
        }
    }
    if old.state != new.state {
        let mut state = AccessibilityState::new()?;
        state.set_disabled(i32::from(new.state.disabled));
        state.set_selected(i32::from(new.state.selected));
        state.set_checked_state(i32::from(new.state.checked == Some(Checked::True)));
        let result = native.set_accessibility_state(&state);
        state.dispose();
        result?;
    }
    if old.value != new.value || old.state != new.state {
        let range = crate::accessibility::value::range_value(new);
        let spoken = crate::accessibility::value::spoken_value(new);
        if range.is_some() || spoken.is_some() {
            let mut value = AccessibilityValue::new()?;
            if let Some((min, max, current)) = range {
                value.set_min(min);
                value.set_max(max);
                value.set_current(current);
            }
            let result = (|| {
                if let Some(text) = spoken {
                    value.set_text(&text.replace('\0', "\u{fffd}"))?;
                }
                native.set_accessibility_value(&value)
            })();
            value.dispose();
            result?;
        } else {
            native.reset_attribute(A::AccessibilityValue)?;
        }
    }
    drop(native);
    if old.actions != new.actions {
        let actions = new.actions.iter().fold(0, |mask, a| {
            mask | match a.name.as_ref() {
                "activate" => 1,
                "longpress" => 2,
                "cut" => 4,
                "copy" => 8,
                "paste" => 16,
                _ => 0,
            }
        });
        owner
            .native
            .borrow()
            .set_u32_attribute(A::AccessibilityActions, actions)?;
        let weak = Rc::downgrade(owner);
        owner
            .native
            .borrow_mut()
            .on_accessibility_actions(move |action| {
                if let Some(owner) = weak.upgrade() {
                    let name = match action {
                        1 => "activate",
                        2 => "longpress",
                        4 => "cut",
                        8 => "copy",
                        16 => "paste",
                        _ => return,
                    };
                    owner.emit(
                        EventKind::AccessibilityAction,
                        EventPayload::Map(vec![("actionName", PayloadValue::Str(name.into()))]),
                    );
                }
            });
    }
    owner.ensure_wrapper()
}
