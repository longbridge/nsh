//! Native interaction state and view metadata. Input feedback is answered on
//! the UI thread from mounted props; notifications to the engine are asynchronous.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use nsh_core::{
    EventKind, EventMask, EventPayload, GestureValues, PayloadValue, PointerEvents, ViewProps,
    tailwind::state,
};
use ohos_arkui_binding::{
    arkui_input_binding::{UIInputAction, UIMouseEventAction, UIMouseEventButton},
    component::attribute::{ArkUICommonAttribute, ArkUIEvent, ArkUIGesture},
    gesture::{gesture_data::GestureData, inner_gesture::Gesture},
    types::{
        attribute::ArkUINodeAttributeType as A,
        event::NodeEventType as E,
        gesture_direction::GestureDirection,
        gesture_event::GestureEventAction,
        view::{AccessibilityMode, HitTestMode},
    },
};

use crate::render::node::{Node, Result, callback};

#[derive(Default)]
pub(crate) struct Interaction {
    props: RefCell<ViewProps>,
    events: Cell<EventMask>,
    tracked: Cell<u16>,
    states: Cell<u16>,
    inherited_block: Cell<bool>,
    click_registered: Cell<bool>,
    touch_registered: Cell<bool>,
    expanded_hit_region: Cell<bool>,
    hover_registered: Cell<bool>,
    mouse_registered: Cell<bool>,
    opacity: Cell<Option<f32>>,
    label: RefCell<Option<Arc<str>>>,
    applied_label: RefCell<Option<Arc<str>>>,
    long_press: RefCell<Option<(u32, Gesture)>>,
    long_fired: Cell<bool>,
    gestures: RefCell<Vec<Gesture>>,
    gesture_values: Cell<GestureValues>,
    gesture_start: Cell<[f32; 4]>,
}

impl Interaction {
    pub(crate) fn test_id(&self) -> Option<String> {
        self.props.borrow().test_id.as_deref().map(str::to_owned)
    }
    pub(crate) fn hit_region(&self, owner: &Node) -> Result<()> {
        let frame = owner.frame.get();
        let has_area = frame.width > 0.0 && frame.height > 0.0;
        let mut slop = if has_area {
            self.props.borrow().hit_slop
        } else {
            nsh_core::EdgeInsets::default()
        };
        let eff_w = if frame.width > 0.0 {
            frame.width
        } else {
            1.0 / owner.scale
        };
        let eff_h = if frame.height > 0.0 {
            frame.height
        } else {
            1.0 / owner.scale
        };
        if !owner.clips_children()
            && let Some(extent) = owner.overflow.get()
        {
            slop.left = slop.left.max(nsh_core::Points(-extent.x));
            slop.top = slop.top.max(nsh_core::Points(-extent.y));
            slop.right = slop
                .right
                .max(nsh_core::Points(extent.x + extent.width - eff_w));
            slop.bottom = slop
                .bottom
                .max(nsh_core::Points(extent.y + extent.height - eff_h));
        }
        if slop == nsh_core::EdgeInsets::default() {
            if self.expanded_hit_region.replace(false) {
                owner.native.borrow().reset_attribute(A::ResponseRegion)?;
            }
        } else {
            self.expanded_hit_region.set(true);
            owner.native.borrow().set_attribute(
                A::ResponseRegion,
                [
                    -slop.left.0,
                    -slop.top.0,
                    (eff_w + slop.left.0 + slop.right.0) / eff_w * 100.0,
                    (eff_h + slop.top.0 + slop.bottom.0) / eff_h * 100.0,
                ]
                .into(),
            )?;
        }
        Ok(())
    }
    pub(crate) fn hit_slop(&self) -> nsh_core::EdgeInsets {
        self.props.borrow().hit_slop
    }
    pub(crate) fn pointer_events(&self) -> PointerEvents {
        self.props.borrow().pointer_events
    }
    pub(crate) fn pan_activation(&self) -> Option<nsh_core::PanActivation> {
        self.props.borrow().pan_activation
    }
    pub(crate) fn mask(&self) -> EventMask {
        self.events.get()
    }
    pub(crate) fn events(&self, owner: &Rc<Node>, events: EventMask) -> Result<()> {
        self.events.set(events);
        if let Some(scroll) = owner.scroll.borrow().as_ref() {
            scroll.refreshable(events.contains(EventKind::Refresh))?;
        }
        let blocked = self.inherited_block.get()
            || matches!(
                self.props.borrow().pointer_events,
                PointerEvents::None | PointerEvents::BoxNone
            );
        let click =
            !blocked && (events.contains(EventKind::Press) || owner.text_listens(EventKind::Press));
        if self.click_registered.replace(click) != click {
            if click {
                let weak = Rc::downgrade(owner);
                owner
                    .native
                    .borrow_mut()
                    .on_event(E::OnClick, move |event| {
                        if let Some(owner) = weak.upgrade()
                            && !owner.interaction.long_fired.replace(false)
                        {
                            owner.press(
                                EventKind::Press,
                                event.component_f32(0).unwrap_or_default() / owner.scale,
                                event.component_f32(1).unwrap_or_default() / owner.scale,
                            );
                        }
                    });
            } else {
                owner.native.borrow_mut().off_event(E::OnClick)?;
            }
        }
        let touch = !blocked
            && (owner.id == nsh_core::ROOT_ID
                || owner.kind == nsh_core::ViewKind::Modal
                || owner.selectable()
                || click
                || events.contains(EventKind::PressDown)
                || events.contains(EventKind::PressUp)
                || self.tracked.get() & state::PRESSED != 0);
        if self.touch_registered.replace(touch) != touch {
            if touch {
                let weak = Rc::downgrade(owner);
                owner.native.borrow_mut().on_touch_event(move |event| {
                    callback(|| {
                        if let Some(owner) = weak.upgrade()
                            && let Some(input) = event.input_event()
                        {
                            if owner.id == nsh_core::ROOT_ID
                                || owner.kind == nsh_core::ViewKind::Modal
                            {
                                owner.touches.borrow_mut().input(&owner, &input)?;
                            }
                            let x = input.pointer_x_by_index(0);
                            let y = input.pointer_y_by_index(0);
                            match input.action {
                                UIInputAction::Down => {
                                    owner.touches.borrow_mut().record_down(owner.scale, &input);
                                    owner.interaction.long_fired.set(false);
                                    if owner.press_feedback_allowed() {
                                        owner.interaction.state(&owner, state::PRESSED, true)?;
                                        owner.emit(EventKind::PressDown, owner.press_payload(x, y));
                                    }
                                }
                                UIInputAction::Up | UIInputAction::Cancel => {
                                    owner.interaction.state(&owner, state::PRESSED, false)?;
                                    if owner.press_feedback_allowed() {
                                        owner.emit(EventKind::PressUp, owner.press_payload(x, y));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(())
                    })
                });
            } else {
                owner.native.borrow_mut().off_event(E::TouchEvent)?;
            }
        }
        let hover = !blocked
            && (events.contains(EventKind::MouseEnter)
                || events.contains(EventKind::MouseLeave)
                || self.tracked.get() & state::HOVERED != 0);
        if self.hover_registered.replace(hover) != hover {
            if hover {
                let weak = Rc::downgrade(owner);
                owner
                    .native
                    .borrow_mut()
                    .on_event(E::OnHoverEvent, move |event| {
                        callback(|| {
                            if let Some(owner) = weak.upgrade()
                                && let Some(input) = event.input_event()
                            {
                                let hovered = input.is_hovered();
                                owner.interaction.state(&owner, state::HOVERED, hovered)?;
                                owner.emit(
                                    if hovered {
                                        EventKind::MouseEnter
                                    } else {
                                        EventKind::MouseLeave
                                    },
                                    owner.mouse_payload(input.pointer_x(), input.pointer_y(), None),
                                );
                            }
                            Ok(())
                        })
                    });
            } else {
                owner.native.borrow_mut().off_event(E::OnHoverEvent)?;
            }
        }
        let mouse = !blocked
            && (owner.id == nsh_core::ROOT_ID
                || owner.kind == nsh_core::ViewKind::Modal
                || events.contains(EventKind::MouseMove)
                || events.contains(EventKind::ContextMenu));
        if self.mouse_registered.replace(mouse) != mouse {
            if mouse {
                let weak = Rc::downgrade(owner);
                owner.native.borrow_mut().on_mouse(move |event| {
                    callback(|| {
                        if let Some(owner) = weak.upgrade()
                            && let Some(input) = event.input_event()
                        {
                            let x = input.pointer_x();
                            let y = input.pointer_y();
                            let mask = owner.interaction.mask();
                            match input.mouse_action() {
                                UIMouseEventAction::Move => {
                                    if mask.contains(EventKind::MouseMove) {
                                        owner.emit(
                                            EventKind::MouseMove,
                                            owner.mouse_payload(
                                                x,
                                                y,
                                                match input.mouse_button() {
                                                    UIMouseEventButton::Left => {
                                                        Some(nsh_core::MouseButton::Left)
                                                    }
                                                    UIMouseEventButton::Right => {
                                                        Some(nsh_core::MouseButton::Right)
                                                    }
                                                    UIMouseEventButton::Middle => {
                                                        Some(nsh_core::MouseButton::Middle)
                                                    }
                                                    UIMouseEventButton::Back => {
                                                        Some(nsh_core::MouseButton::Back)
                                                    }
                                                    UIMouseEventButton::Forward => {
                                                        Some(nsh_core::MouseButton::Forward)
                                                    }
                                                    _ => None,
                                                },
                                            ),
                                        );
                                    }
                                }
                                UIMouseEventAction::Press
                                    if input.mouse_button() == UIMouseEventButton::Right
                                        && mask.contains(EventKind::ContextMenu) =>
                                {
                                    owner.emit(
                                        EventKind::ContextMenu,
                                        owner.context_menu_payload(x, y),
                                    );
                                }
                                UIMouseEventAction::Press
                                    if input.mouse_button() == UIMouseEventButton::Back
                                        && (owner.id == nsh_core::ROOT_ID
                                            || owner.kind == nsh_core::ViewKind::Modal) =>
                                {
                                    let _ = owner.host.handle_back();
                                }
                                _ => {}
                            }
                        }
                        Ok(())
                    })
                });
            } else {
                owner.native.borrow_mut().off_event(E::OnMouse)?;
            }
        }
        let delay = self
            .props
            .borrow()
            .long_press_delay
            .unwrap_or(500.0)
            .clamp(1.0, i32::MAX as f32) as u32;
        let wants_long = !blocked
            && (owner.selectable()
                || events.contains(EventKind::LongPress)
                || owner.text_listens(EventKind::LongPress));
        if self.long_press.borrow().as_ref().map(|(delay, _)| *delay) != wants_long.then_some(delay)
        {
            self.clear_long(owner)?;
            if wants_long {
                let gesture =
                    Gesture::create_long_gesture(1, false, delay.min(i32::MAX as u32) as i32)?;
                let weak = Rc::downgrade(owner);
                gesture.on_gesture(GestureEventAction::Accept, move |event| {
                    if let Some(owner) = weak.upgrade() {
                        if owner.touches.borrow().selection_dragging(owner.id) {
                            return;
                        }
                        owner.interaction.long_fired.set(true);
                        let (x, y) = event.input.map_or((0.0, 0.0), |input| (input.x, input.y));
                        owner.press(EventKind::LongPress, x, y);
                        callback(|| {
                            owner.selection_begin(x, y)?;
                            Ok(())
                        });
                    }
                })?;
                owner
                    .native
                    .borrow()
                    .add_gesture_ref(&gesture, None, None)?;
                *self.long_press.borrow_mut() = Some((delay, gesture));
            }
        }
        Ok(())
    }
    pub(crate) fn props(&self, owner: &Rc<Node>, props: ViewProps) -> Result<()> {
        owner.wrapper_semantics_reset();
        {
            let native = owner.native.borrow();
            if let Some(id) = &props.test_id {
                native.id(id.as_ref())?;
            } else {
                native.reset_attribute(A::Id)?;
            }
            native.set_bool_attribute(
                A::AccessibilityGroup,
                props.accessibility.accessible.unwrap_or(false),
            )?;
            let mode = if props.accessibility.hidden {
                AccessibilityMode::DisabledForDescendants
            } else {
                match props.accessibility.accessible {
                    Some(true) => AccessibilityMode::Enabled,
                    Some(false) => AccessibilityMode::Disabled,
                    None => AccessibilityMode::Auto,
                }
            };
            native.set_i32_attribute(A::AccessibilityMode, mode.into())?;
            native.set_string_attribute(
                A::AccessibilityDescription,
                props.accessibility.hint.as_deref().unwrap_or_default(),
            )?;
        }
        crate::accessibility::node::configure(
            owner,
            &self.props.borrow().accessibility,
            &props.accessibility,
        )?;
        let disabled = props.accessibility.state.disabled;
        *self.props.borrow_mut() = props;
        owner.update_wrapper()?;
        self.hit_region(owner)?;
        self.state(owner, state::DISABLED, disabled)?;
        self.apply_label(owner)?;
        self.apply_opacity(owner)?;
        self.pointer(owner, self.inherited_block.get())
    }
    pub(crate) fn label(&self, owner: &Node, label: Option<Arc<str>>) -> Result<()> {
        self.props.borrow_mut().accessibility.label = label;
        self.apply_label(owner)
    }
    pub(crate) fn text(&self, owner: &Node, text: &Arc<str>) -> Result<()> {
        *self.label.borrow_mut() = (!text.is_empty()).then(|| text.clone());
        self.apply_label(owner)
    }
    fn apply_label(&self, owner: &Node) -> Result<()> {
        let effective = self
            .props
            .borrow()
            .accessibility
            .label
            .clone()
            .or_else(|| self.label.borrow().clone())
            .filter(|s| !s.is_empty());
        let unchanged = match (&*self.applied_label.borrow(), &effective) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) || **a == **b,
            _ => false,
        };
        if !unchanged {
            owner.native.borrow().set_string_attribute(
                A::AccessibilityText,
                effective.as_deref().unwrap_or_default(),
            )?;
            *self.applied_label.borrow_mut() = effective;
        }
        Ok(())
    }
    pub(crate) fn pointer(&self, owner: &Rc<Node>, inherited: bool) -> Result<()> {
        self.inherited_block.set(inherited);
        let pointer = self.props.borrow().pointer_events;
        let mode = if inherited {
            HitTestMode::None
        } else {
            match pointer {
                PointerEvents::Auto => HitTestMode::Default,
                PointerEvents::BoxOnly => HitTestMode::Block,
                PointerEvents::BoxNone | PointerEvents::None => HitTestMode::None,
            }
        };
        owner
            .native
            .borrow()
            .set_i32_attribute(A::HitTestBehavior, mode.into())?;
        let internal =
            if inherited || matches!(pointer, PointerEvents::None | PointerEvents::BoxOnly) {
                HitTestMode::None
            } else {
                HitTestMode::Default
            };
        if let Some(input) = owner.input.borrow().as_ref() {
            input.hit_test(internal)?;
        }
        if let Some(scroll) = owner.scroll.borrow().as_ref() {
            scroll
                .native
                .borrow()
                .set_i32_attribute(A::HitTestBehavior, internal.into())?;
        }
        self.events(owner, self.events.get())?;
        for child in owner
            .children
            .borrow()
            .iter()
            .filter_map(|child| child.upgrade())
        {
            child.interaction.pointer(
                &child,
                inherited || matches!(pointer, PointerEvents::None | PointerEvents::BoxOnly),
            )?;
        }
        Ok(())
    }
    pub(crate) fn blocks_children(&self) -> bool {
        self.inherited_block.get()
            || matches!(
                self.props.borrow().pointer_events,
                PointerEvents::None | PointerEvents::BoxOnly
            )
    }
    pub(crate) fn gestures(&self, owner: &Rc<Node>, values: GestureValues) -> Result<()> {
        if self.gesture_values.replace(values) == values {
            return Ok(());
        }
        for gesture in self.gestures.borrow_mut().drain(..) {
            owner.native.borrow().remove_gesture(&gesture)?;
            gesture.dispose()?;
        }
        for kind in 0..3 {
            let enabled = match kind {
                0 => values.pan_x.is_some() || values.pan_y.is_some(),
                1 => values.pinch.is_some(),
                _ => values.rotation.is_some(),
            };
            if !enabled {
                continue;
            }
            let gesture = match kind {
                0 => Gesture::create_pan_gesture(
                    1,
                    GestureDirection::All,
                    10.0 * owner.scale as f64,
                )?,
                1 => Gesture::create_pinch_gesture(2, 5.0 * owner.scale as f64)?,
                _ => Gesture::create_rotation_gesture(2, 1.0)?,
            };
            let weak = Rc::downgrade(owner);
            gesture.on_gesture(
                GestureEventAction::Accept
                    | GestureEventAction::Update
                    | GestureEventAction::End
                    | GestureEventAction::Cancel,
                move |event| {
                    callback(|| {
                        let Some(owner) = weak.upgrade() else {
                            return Ok(());
                        };
                        let values = owner.interaction.gesture_values.get();
                        let accept = event.event_action_type == GestureEventAction::Accept;
                        let ended = event
                            .event_action_type
                            .intersects(GestureEventAction::End | GestureEventAction::Cancel);
                        if accept {
                            owner.interaction.gesture_start.set([
                                values.pan_x.map_or(0.0, |id| owner.animator.value(id)),
                                values.pan_y.map_or(0.0, |id| owner.animator.value(id)),
                                values.pinch.map_or(1.0, |id| owner.animator.value(id)),
                                values.rotation.map_or(0.0, |id| owner.animator.value(id)),
                            ]);
                        }
                        let start = owner.interaction.gesture_start.get();
                        match event.event_action_data {
                            GestureData::Pan(pan) => {
                                if let Some(id) = values.pan_x {
                                    owner
                                        .animator
                                        .set(id, start[0] + pan.offset_x / owner.scale)?;
                                }
                                if let Some(id) = values.pan_y {
                                    owner
                                        .animator
                                        .set(id, start[1] + pan.offset_y / owner.scale)?;
                                }
                                if accept || ended {
                                    let translation = nsh_core::Point::new(
                                        nsh_core::Points(pan.offset_x / owner.scale),
                                        nsh_core::Points(pan.offset_y / owner.scale),
                                    );
                                    let velocity_x = pan.velocity_x / owner.scale;
                                    let velocity_y = pan.velocity_y / owner.scale;
                                    let value_x = values
                                        .pan_x
                                        .map(|id| nsh_core::Points(owner.animator.value(id)));
                                    let value_y = values
                                        .pan_y
                                        .map(|id| nsh_core::Points(owner.animator.value(id)));
                                    owner.emit(
                                        if accept {
                                            EventKind::PanStart
                                        } else {
                                            EventKind::PanEnd
                                        },
                                        if event.event_action_type == GestureEventAction::Cancel {
                                            EventPayload::PanCancelled {
                                                translation,
                                                velocity_x,
                                                velocity_y,
                                                value_x,
                                                value_y,
                                            }
                                        } else {
                                            EventPayload::Pan {
                                                translation,
                                                velocity_x,
                                                velocity_y,
                                                value_x,
                                                value_y,
                                            }
                                        },
                                    );
                                }
                            }
                            GestureData::Pinch(pinch) => {
                                if let Some(id) = values.pinch {
                                    owner.animator.set(id, start[2] * pinch.scale)?;
                                }
                                if accept || ended {
                                    owner.emit(
                                        if accept {
                                            EventKind::PinchStart
                                        } else {
                                            EventKind::PinchEnd
                                        },
                                        EventPayload::Map(vec![
                                            ("scale", PayloadValue::Number(pinch.scale as f64)),
                                            ("velocity", PayloadValue::Number(0.0)),
                                        ]),
                                    );
                                }
                            }
                            GestureData::Rotation(rotation) => {
                                if let Some(id) = values.rotation {
                                    owner.animator.set(id, start[3] + rotation.angle)?;
                                }
                                if accept || ended {
                                    owner.emit(
                                        if accept {
                                            EventKind::RotationStart
                                        } else {
                                            EventKind::RotationEnd
                                        },
                                        EventPayload::Map(vec![
                                            (
                                                "rotation",
                                                PayloadValue::Number(rotation.angle as f64),
                                            ),
                                            ("velocity", PayloadValue::Number(0.0)),
                                        ]),
                                    );
                                }
                            }
                            _ => {}
                        }
                        Ok(())
                    })
                },
            )?;
            owner
                .native
                .borrow()
                .add_gesture_ref(&gesture, None, None)?;
            self.gestures.borrow_mut().push(gesture);
        }
        Ok(())
    }
    pub(crate) fn dispose(&self, owner: &Node) -> Result<()> {
        self.clear_long(owner)?;
        for gesture in self.gestures.borrow_mut().drain(..) {
            owner.native.borrow().remove_gesture(&gesture)?;
            gesture.dispose()?;
        }
        Ok(())
    }
    pub(crate) fn clear_long(&self, owner: &Node) -> Result<()> {
        if let Some((_, gesture)) = self.long_press.borrow_mut().take() {
            owner.native.borrow().remove_gesture(&gesture)?;
            gesture.dispose()?;
        }
        Ok(())
    }
    pub(crate) fn track(&self, owner: &Rc<Node>, tracked: u16) -> Result<()> {
        self.tracked.set(tracked);
        owner
            .control
            .set_states(owner.id, self.states.get() & tracked);
        self.events(owner, self.events.get())
    }
    pub(crate) fn state(&self, owner: &Node, bit: u16, enabled: bool) -> Result<()> {
        let old = self.states.get();
        let next = if enabled { old | bit } else { old & !bit };
        self.states.set(next);
        if old != next && self.tracked.get() & bit != 0 {
            owner
                .control
                .set_states(owner.id, next & self.tracked.get());
        }
        if bit == state::PRESSED {
            self.apply_opacity(owner)?;
        }
        Ok(())
    }
    pub(crate) fn opacity(&self, owner: &Node, opacity: f32) -> Result<()> {
        if self.opacity.replace(Some(opacity)) != Some(opacity) {
            self.apply_opacity(owner)?;
        }
        Ok(())
    }
    pub(crate) fn invalidate_opacity(&self) {
        self.opacity.set(None);
    }
    fn apply_opacity(&self, owner: &Node) -> Result<()> {
        owner
            .visual_node()
            .borrow()
            .opacity(self.opacity.get().unwrap_or(1.0))?;
        Ok(())
    }
}
