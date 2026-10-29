//! Optional provider metadata for semantic leaves, without hiding native widget trees.
use std::{
    cell::{Cell, RefCell},
    ffi::CStr,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use nsh_core::{
    Accessibility, AccessibilityRole as R, Checked, EventKind, EventPayload, PayloadValue, ViewKind,
};
use ohos_accessibility_binding::{
    AccessibilityError as E, AccessibleRect, ActionArguments, ActionType as A, ElementInfo,
    ElementInfoList, EventInfo, EventType, FocusMoveDirection, FocusType, ProviderCallbacks,
    RangeInfo, SearchMode,
};
use ohos_arkui_binding::{
    component::{accessibility::NodeAccessibility, attribute::ArkUICommonAttribute},
    types::{attribute::ArkUINodeAttributeType as Attribute, view::AccessibilityMode},
};

use super::action_descriptions::ActionDescriptions;
use crate::render::node::{Error, Node, Result};

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
#[derive(Default)]
struct Snapshot {
    spec: Accessibility,
    label: String,
    bounds: AccessibleRect,
    visible: bool,
    focused: bool,
    actions: Vec<(A, &'static CStr)>,
}
#[derive(Default)]
pub(crate) struct State {
    instance: Cell<u64>,
    suppressed: Cell<bool>,
    registration: RefCell<Option<NodeAccessibility>>,
    snapshot: Arc<Mutex<Snapshot>>,
}
impl State {
    pub(crate) fn hides_native_children(&self) -> bool {
        self.registration.borrow().is_some()
    }
    pub(crate) fn clear(&self) {
        self.registration.borrow_mut().take();
    }
    pub(crate) fn configure(&self, owner: &Rc<Node>) -> Result<()> {
        let existing = self.registration.borrow().is_some();
        let was_providing = self.suppressed.get();
        let spec = owner.accessibility_state.spec.borrow();
        let semantic_leaf = owner.children.borrow().is_empty() || spec.accessible == Some(true);
        let eligible = crate::get_capabilities().api23
            && !owner.wrapper_owns_semantics()
            && semantic_leaf
            && !spec.hidden
            && spec.accessible != Some(false)
            && !matches!(
                owner.kind,
                ViewKind::Scroll | ViewKind::WebView | ViewKind::Video | ViewKind::Native
            )
            && owner.kind != ViewKind::Input
            && (crate::accessibility::value::range_value(&spec).is_some()
                || spec
                    .actions
                    .iter()
                    .any(|action| matches!(action.name.as_ref(), "increment" | "decrement")));
        if !eligible {
            self.clear();
            if self.suppressed.get() && !owner.wrapper_owns_semantics() {
                // Restore native metadata when the virtual leaf retires.
                crate::accessibility::node::configure(owner, &Accessibility::default(), &spec)?;
                owner.interaction.label(owner, spec.label.clone())?;
                let mode = if spec.hidden {
                    AccessibilityMode::DisabledForDescendants
                } else {
                    match spec.accessible {
                        Some(true) => AccessibilityMode::Enabled,
                        Some(false) => AccessibilityMode::Disabled,
                        None => AccessibilityMode::Auto,
                    }
                };
                let native = owner.native.borrow();
                native.set_i32_attribute(Attribute::AccessibilityMode, mode.into())?;
                native.set_bool_attribute(
                    Attribute::AccessibilityGroup,
                    spec.accessible == Some(true),
                )?;
                self.suppressed.set(false);
            }
            if was_providing {
                for child in owner
                    .children
                    .borrow()
                    .iter()
                    .filter_map(std::rc::Weak::upgrade)
                {
                    child.accessibility_state.changed(&child);
                }
            }
            return Ok(());
        }
        self.update_snapshot(
            &spec,
            owner.accessibility_state.label(owner),
            owner.visible_in_tree(),
        )?;
        drop(spec);
        self.update_bounds(owner)?;
        if self.registration.borrow().is_none() {
            if self.instance.get() == 0 {
                self.instance.set(
                    NEXT_INSTANCE
                        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                        .map_err(|_| Error::Unsupported("exhausted accessibility generations"))?,
                );
            }
            let registration = NodeAccessibility::new(
                &owner.native,
                &format!("nsh-access-{}", self.instance.get()),
                Callbacks {
                    snapshot: self.snapshot.clone(),
                    control: owner.control.clone(),
                    node: owner.id,
                    dispatcher: owner.dispatcher.clone(),
                },
            )?;
            *self.registration.borrow_mut() = Some(registration);
        }
        // The virtual root is structural. Grouping the host would merge that
        // root with its leaf and prevent a reader from traversing the leaf.
        owner
            .native
            .borrow()
            .set_bool_attribute(Attribute::AccessibilityGroup, false)?;
        // Keep the host as the native traversal anchor; its semantic text and
        // role belong only to the virtual leaf below.
        owner.native.borrow().set_i32_attribute(
            Attribute::AccessibilityMode,
            AccessibilityMode::Enabled.into(),
        )?;
        {
            let native = owner.native.borrow();
            native.reset_attribute(Attribute::AccessibilityRole)?;
            native.reset_attribute(Attribute::AccessibilityValue)?;
            native.set_string_attribute(Attribute::AccessibilityText, "")?;
            native.set_string_attribute(Attribute::AccessibilityDescription, "")?;
        }
        self.suppressed.set(true);
        if !was_providing {
            for child in owner
                .children
                .borrow()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
            {
                child.accessibility_state.changed(&child);
            }
        }
        if existing {
            self.send_event(EventType::PageContentUpdate)?;
        }
        Ok(())
    }
    pub(crate) fn send_event(&self, event_type: EventType) -> Result<()> {
        let registration = self.registration.borrow();
        let Some(registration) = registration.as_ref() else {
            return Ok(());
        };
        let mut element = ElementInfo::new()?;
        self.snapshot.lock().unwrap().fill(&mut element)?;
        let mut event = EventInfo::new()?;
        event
            .set_event_type(event_type)?
            .set_element_info(&element)?;
        registration.send_event(&event);
        Ok(())
    }
    pub(crate) fn bounds(&self, owner: &Node) -> Result<()> {
        if self.registration.borrow().is_none() {
            return Ok(());
        }
        self.update_bounds(owner)
    }
    fn update_snapshot(&self, spec: &Accessibility, label: String, visible: bool) -> Result<()> {
        let actions = Snapshot::actions(spec)?;
        let mut snapshot = self.snapshot.lock().unwrap();
        snapshot.spec = spec.clone();
        snapshot.label = label;
        snapshot.visible = visible;
        snapshot.actions = actions;
        Ok(())
    }
    fn update_bounds(&self, owner: &Node) -> Result<()> {
        let frame = owner.frame.get();
        // ArkUI applies the host's screen offset when embedding this tree.
        self.snapshot.lock().unwrap().bounds = AccessibleRect {
            left: 0,
            top: 0,
            right: (frame.width * owner.scale).round() as i32,
            bottom: (frame.height * owner.scale).round() as i32,
        };
        Ok(())
    }
}
struct Callbacks {
    snapshot: Arc<Mutex<Snapshot>>,
    control: nsh_engine::EngineControl,
    node: nsh_core::NodeId,
    dispatcher: std::sync::Weak<crate::app::runtime::Dispatcher>,
}
impl Snapshot {
    fn root(&self, element: &mut ElementInfo<'_>) -> ohos_accessibility_binding::Result<()> {
        element
            .set_element_id(0)?
            .set_parent_id(-2_100_000)?
            .set_component_type("root")?
            .set_enabled(true)?
            .set_focusable(false)?
            .set_accessibility_group(false)?
            .set_accessibility_level("no")?
            .set_screen_rect(self.bounds)?
            .set_visible(self.visible)?
            .set_child_node_ids(&[1])?;
        Ok(())
    }
    fn actions(spec: &Accessibility) -> Result<Vec<(A, &'static CStr)>> {
        let mut actions = vec![
            (A::GainAccessibilityFocus, c"accessibility focus"),
            (A::ClearAccessibilityFocus, c"clear accessibility focus"),
        ];
        let mut dynamic = Vec::new();
        for action in &spec.actions {
            let (kind, name) = match action.name.as_ref() {
                "activate" => (A::Click, c"activate"),
                "longpress" => (A::LongClick, c"longpress"),
                "increment" => (A::ScrollForward, c"increment"),
                "decrement" => (A::ScrollBackward, c"decrement"),
                "copy" => (A::Copy, c"copy"),
                "cut" => (A::Cut, c"cut"),
                "paste" => (A::Paste, c"paste"),
                _ => continue,
            };
            if let Some(label) = action.label.as_deref() {
                dynamic.push((actions.len(), label));
            }
            actions.push((kind, name));
        }
        if !dynamic.is_empty() {
            let labels: Vec<_> = dynamic.iter().map(|(_, label)| *label).collect();
            match ActionDescriptions::intern(&labels) {
                Ok(labels) => {
                    for ((index, _), label) in dynamic.into_iter().zip(labels) {
                        actions[index].1 = label;
                    }
                }
                Err(error) => {
                    log::warn!("OHOS accessibility action labels: {error:?}; using action names")
                }
            }
        }
        Ok(actions)
    }
    fn fill(&self, element: &mut ElementInfo<'_>) -> ohos_accessibility_binding::Result<()> {
        let component = match self.spec.role {
            R::Checkbox => "checkbox",
            R::Switch => "switch",
            R::Radio => "radio",
            R::Adjustable | R::Scrollbar => "slider",
            R::Button | R::ImageButton | R::Option => "button",
            R::List | R::Listbox => "list",
            _ => "text",
        };
        element
            .set_element_id(1)?
            .set_parent_id(0)?
            .set_component_type(component)?
            .set_accessibility_text(&self.label.replace('\0', "\u{fffd}"))?
            .set_accessibility_description(
                &self
                    .spec
                    .hint
                    .as_deref()
                    .unwrap_or("")
                    .replace('\0', "\u{fffd}"),
            )?
            .set_contents(
                &crate::accessibility::value::spoken_value(&self.spec)
                    .unwrap_or_default()
                    .replace('\0', "\u{fffd}"),
            )?
            .set_screen_rect(self.bounds)?
            .set_checkable(self.spec.state.checked.is_some())?
            .set_checked(self.spec.state.checked == Some(Checked::True))?
            .set_selected(self.spec.state.selected)?
            .set_enabled(!self.spec.state.disabled)?
            .set_visible(self.visible)?
            .set_focusable(true)?
            .set_accessibility_group(false)?
            .set_scrollable(
                self.spec
                    .actions
                    .iter()
                    .any(|action| matches!(action.name.as_ref(), "increment" | "decrement")),
            )?
            .set_accessibility_level("yes")?
            .set_accessibility_focused(self.focused)?
            .set_operation_actions(&self.actions)?;
        if let Some(value) = &self.spec.value
            && let (Some(min), Some(max), Some(now)) = (value.min, value.max, value.now)
            && min.is_finite()
            && max.is_finite()
            && now.is_finite()
            && max >= min
        {
            element.set_range_info(RangeInfo {
                min: min as f64,
                max: max as f64,
                current: now.clamp(min, max) as f64,
            })?;
        }
        Ok(())
    }
}
impl ProviderCallbacks for Callbacks {
    fn find_node_infos_by_id(
        &self,
        id: i64,
        mode: SearchMode,
        _request: i32,
        elements: &mut ElementInfoList<'_>,
    ) -> ohos_accessibility_binding::Result<()> {
        if !matches!(id, -1..=1) {
            return Err(E::BadParameter);
        }
        let state = self.snapshot.lock().map_err(|_| E::LockPoisoned)?;
        if id == 1 {
            state.fill(&mut elements.add()?)?;
            if mode.contains(SearchMode::PREDECESSORS) {
                state.root(&mut elements.add()?)?;
            }
        } else {
            state.root(&mut elements.add()?)?;
            if mode.contains(SearchMode::CHILDREN) || mode.contains(SearchMode::RECURSIVE_CHILDREN)
            {
                state.fill(&mut elements.add()?)?;
            }
        }
        Ok(())
    }
    fn find_node_infos_by_text(
        &self,
        _id: i64,
        text: &CStr,
        _request: i32,
        elements: &mut ElementInfoList<'_>,
    ) -> ohos_accessibility_binding::Result<()> {
        let state = self.snapshot.lock().map_err(|_| E::LockPoisoned)?;
        if state
            .label
            .contains(text.to_str().map_err(|_| E::BadParameter)?)
        {
            state.fill(&mut elements.add()?)?;
        }
        Ok(())
    }
    fn find_focused_node(
        &self,
        id: i64,
        kind: FocusType,
        _request: i32,
        element: &mut ElementInfo<'_>,
    ) -> ohos_accessibility_binding::Result<()> {
        if !matches!(id, -1..=1) {
            return Err(E::BadParameter);
        }
        let state = self.snapshot.lock().map_err(|_| E::LockPoisoned)?;
        let focused = match kind {
            FocusType::Accessibility => state.focused,
            FocusType::Input => false,
            _ => false,
        };
        if !focused {
            return Err(E::Failed);
        }
        state.fill(element)
    }
    fn find_next_focus_node(
        &self,
        id: i64,
        direction: FocusMoveDirection,
        _request: i32,
        element: &mut ElementInfo<'_>,
    ) -> ohos_accessibility_binding::Result<()> {
        // This provider has one semantic leaf. A query from that leaf must
        // leave the provider rather than cycle back to itself.
        if !matches!(id, -1 | 0) || direction == FocusMoveDirection::Invalid {
            return Err(E::Failed);
        }
        self.snapshot
            .lock()
            .map_err(|_| E::LockPoisoned)?
            .fill(element)
    }
    fn execute_action(
        &self,
        id: i64,
        action: A,
        _args: &ActionArguments<'_>,
        _request: i32,
    ) -> ohos_accessibility_binding::Result<()> {
        if id != 1 {
            return Err(E::BadParameter);
        }
        let mut state = self.snapshot.lock().map_err(|_| E::LockPoisoned)?;
        if state.spec.state.disabled || !state.visible {
            return Err(E::Failed);
        }
        match action {
            A::GainAccessibilityFocus => {
                state.focused = true;
                drop(state);
                self.event(EventType::AccessibilityFocused);
                return Ok(());
            }
            A::ClearAccessibilityFocus => {
                state.focused = false;
                drop(state);
                self.event(EventType::AccessibilityFocusCleared);
                return Ok(());
            }
            _ => {}
        }
        let name = match action {
            A::Click => "activate",
            A::LongClick => "longpress",
            A::ScrollForward => "increment",
            A::ScrollBackward => "decrement",
            A::Copy => "copy",
            A::Cut => "cut",
            A::Paste => "paste",
            _ => return Err(E::Unsupported),
        };
        if !state.spec.actions.iter().any(|a| a.name.as_ref() == name) {
            return Err(E::Unsupported);
        }
        drop(state);
        self.control.dispatch_event(
            self.node,
            EventKind::AccessibilityAction,
            EventPayload::Map(vec![("actionName", PayloadValue::Str(name.into()))]),
        );
        Ok(())
    }
    fn clear_focused_node(&self) -> ohos_accessibility_binding::Result<()> {
        self.snapshot.lock().map_err(|_| E::LockPoisoned)?.focused = false;
        Ok(())
    }
    fn cursor_position(&self, id: i64, _request: i32) -> ohos_accessibility_binding::Result<i32> {
        if id != 1 {
            return Err(E::BadParameter);
        }
        Ok(0)
    }
}
impl Callbacks {
    fn event(&self, event: EventType) {
        if let Some(route) = self.dispatcher.upgrade() {
            route.send(crate::app::runtime::Message::AccessibilityEvent(
                self.node, event,
            ));
        }
    }
}
