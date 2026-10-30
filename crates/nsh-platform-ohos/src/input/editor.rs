//! ArkTS-owned API-20 editors; native edit counts reject stale controlled values.
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use nsh_core::{
    AutoCapitalize, EventKind, EventPayload, InputConfig, KeyboardType, PayloadValue,
    ReturnKeyType, SubmitBehavior, TextAlign,
};
use ohos_arkui_binding::{
    api::node_custom_event::NodeCustomEvent, component::attribute::ArkUICommonAttribute,
    types::view::HitTestMode,
};

pub(crate) use crate::interop::sdk::EditorEvent;
use crate::{
    input::editor_state::{self, State},
    interop::{
        attachment::NativeAttachment,
        sdk::{EditorAccessibility, EditorCommand, EditorRoute, EditorSettings},
    },
    render::{
        geometry::Rect,
        node::{Error, Node, Result, callback},
    },
    text::typography::argb,
};

pub(crate) struct Input {
    owner: Weak<Node>,
    target: RefCell<Option<NativeAttachment>>,
    route: RefCell<Option<EditorRoute>>,
    weak: Weak<Self>,
    config: RefCell<InputConfig>,
    state: RefCell<State>,
    content: Cell<Rect>,
    closed: Cell<bool>,
    hit_test: Cell<u32>,
}
impl Input {
    pub(crate) fn new(owner: Weak<Node>) -> Result<Rc<Self>> {
        let node = owner.upgrade().ok_or(Error::InvalidNode(0))?;
        let target = NativeAttachment::new(&node.native)?;
        let input = Rc::new_cyclic(|weak| Self {
            owner,
            target: RefCell::new(Some(target)),
            route: RefCell::default(),
            weak: weak.clone(),
            config: RefCell::new(InputConfig::default()),
            state: RefCell::default(),
            content: Cell::new(Rect::default()),
            closed: Cell::new(false),
            hit_test: Cell::new(i32::from(HitTestMode::Default) as u32),
        });
        input.listen();
        Ok(input)
    }
    fn target_id(&self) -> Option<u32> {
        self.target.borrow().as_ref().map(NativeAttachment::id)
    }
    fn with_child<T>(
        &self,
        f: impl FnOnce(&ohos_arkui_binding::common::node::ArkUINode) -> T,
    ) -> Result<Option<T>> {
        if self.route.borrow().is_none()
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
    fn listen(&self) {
        *self.route.borrow_mut() = self
            .target_id()
            .map(|id| EditorRoute::new(id, self.weak.clone()));
    }
    pub(crate) fn native_event(&self, mut event: EditorEvent) -> Result<u32> {
        if self.closed.get() {
            return Ok(self.state.borrow().count);
        }
        if event.kind == "change" {
            let changed = self.state.borrow_mut().changed(event.text.clone());
            let Some(count) = changed else {
                return Ok(self.state.borrow().count);
            };
            event.count = count;
        }
        self.event(event)?;
        Ok(self.state.borrow().count)
    }
    fn event(&self, event: EditorEvent) -> Result<()> {
        let Some(owner) = self.owner.upgrade().filter(|_| !self.closed.get()) else {
            return Ok(());
        };
        match event.kind.as_str() {
            "change" => {
                {
                    let mut state = self.state.borrow_mut();
                    state.text = event.text.clone();
                    state.count = event.count;
                }
                owner.emit(
                    EventKind::ChangeText,
                    EventPayload::Text {
                        text: event.text,
                        count: event.count,
                    },
                );
            }
            "focus" => {
                if !self.state.borrow_mut().focus(true) {
                    return Ok(());
                }
                owner.clear_text_selection()?;
                owner.emit(EventKind::Focus, EventPayload::None);
            }
            "blur" => {
                if self.state.borrow_mut().focus(false) {
                    owner.emit(EventKind::Blur, EventPayload::None);
                }
            }
            "submit" => {
                if self.config.borrow().submit() != SubmitBehavior::Newline {
                    self.key(&owner, "Enter");
                }
                owner.emit(
                    EventKind::SubmitEditing,
                    EventPayload::Text {
                        text: self.state.borrow().text.clone(),
                        count: self.state.borrow().count,
                    },
                );
            }
            "selection" => owner.emit(
                EventKind::SelectionChange,
                EventPayload::Map(vec![
                    ("start", PayloadValue::Number(event.start as f64)),
                    ("end", PayloadValue::Number(event.end as f64)),
                ]),
            ),
            "size" => owner.emit(
                EventKind::ContentSizeChange,
                EventPayload::Map(vec![
                    ("width", PayloadValue::Number(event.width)),
                    ("height", PayloadValue::Number(event.height)),
                ]),
            ),
            "key" if event.text == "Backspace" || event.text.chars().count() == 1 => {
                self.key(&owner, &event.text)
            }
            _ => {}
        }
        Ok(())
    }
    fn key(&self, owner: &Node, key: &str) {
        owner.emit(
            EventKind::KeyPress,
            EventPayload::Map(vec![("key", PayloadValue::Str(key.into()))]),
        );
    }
    pub(crate) fn configure(self: &Rc<Self>, config: InputConfig) -> Result<()> {
        if config.multiline != self.config.borrow().multiline
            || config.secure != self.config.borrow().secure
        {
            self.retire()?;
            let Some(owner) = self.owner.upgrade() else {
                return Ok(());
            };
            let target = NativeAttachment::new(&owner.native)?;
            *self.target.borrow_mut() = Some(target);
            self.listen();
        }
        *self.config.borrow_mut() = config;
        self.update();
        if let Some(owner) = self.owner.upgrade() {
            owner.interaction.events(&owner, owner.interaction.mask())?;
        }
        Ok(())
    }
    pub(crate) fn update(&self) {
        let Some(owner) = self.owner.upgrade().filter(|_| !self.closed.get()) else {
            return;
        };
        if !owner.host.has_ui_context() {
            return;
        }
        let Some(target_id) = self.target_id() else {
            return;
        };
        let config = self.config.borrow();
        let accessibility = owner.accessibility_state.spec.borrow();
        let keyboard = if config.secure {
            if config.keyboard == KeyboardType::Number {
                "number-password"
            } else {
                "password"
            }
        } else {
            match config.keyboard {
                KeyboardType::Number => "number",
                KeyboardType::Decimal
                | KeyboardType::Numeric
                | KeyboardType::NumbersAndPunctuation => "decimal",
                KeyboardType::Phone => "phone",
                KeyboardType::Email => "email",
                _ => "normal",
            }
        };
        let enter = match config.return_key {
            ReturnKeyType::Go => "go",
            ReturnKeyType::Next => "next",
            ReturnKeyType::Search => "search",
            ReturnKeyType::Send => "send",
            _ if config.submit() == SubmitBehavior::Newline => "newline",
            _ => "done",
        };
        let hint = content_type(config.auto_fill);
        let selection = config
            .selection_color
            .unwrap_or(nsh_core::Color(0x3388ff55));
        owner.editor_update(EditorSettings {
            accessibility: Some(EditorAccessibility {
                label: accessibility.label.as_deref().unwrap_or("").to_owned(),
                hint: accessibility.hint.as_deref().unwrap_or("").to_owned(),
                hidden: accessibility.hidden,
                accessible: accessibility.accessible,
                disabled: accessibility.state.disabled,
            }),
            target_id,
            multiline: config.multiline && !config.secure,
            secure: config.secure,
            text: self.state.borrow().text.clone(),
            count: self.state.borrow().count,
            width: self.content.get().width as f64,
            height: self.content.get().height as f64,
            placeholder: config.placeholder.clone(),
            font_size: config.style.font_size.0 as f64,
            font_weight: config.style.font_weight as u32,
            font_family: config
                .style
                .font_family
                .map(|family| family.name().to_string()),
            italic: config.style.italic,
            color: argb(config.style.color),
            placeholder_color: argb(config.placeholder_color.unwrap_or(config.style.color)),
            caret_color: argb(if config.caret_hidden {
                nsh_core::Color::TRANSPARENT
            } else {
                config.selection_color.unwrap_or(config.style.color)
            }),
            selection_color: argb(selection),
            editable: config.editable,
            scrollbar_visible: config.scrollbar_visible && !config.replaced,
            hit_test: self.hit_test.get(),
            align: match config.align {
                TextAlign::Center => 1,
                TextAlign::Right => 2,
                _ => 0,
            },
            rtl: config.rtl,
            capitalize: match config.auto_capitalize {
                AutoCapitalize::None => 0,
                AutoCapitalize::Words => 1,
                AutoCapitalize::Sentences => 2,
                AutoCapitalize::Characters => 3,
            },
            keyboard: keyboard.to_owned(),
            enter_key: enter.to_owned(),
            blur_on_submit: config.submit() == SubmitBehavior::BlurAndSubmit,
            max_length: config.max_length,
            auto_fill: config.auto_fill != nsh_core::AutoFill::Off,
            content_type: hint.map(str::to_owned),
            auto_focus: config.auto_focus,
        });
    }
    pub(crate) fn value(&self, text: String, count: u32) -> Result<()> {
        if self.state.borrow_mut().controlled(text, count) {
            self.update();
        }
        Ok(())
    }
    pub(crate) fn content(&self, content: Rect) -> Result<()> {
        if self.content.replace(content) != content {
            self.update();
        }
        Ok(())
    }
    pub(crate) fn context(&self, ready: bool) {
        callback(|| {
            if ready {
                let Some(owner) = self.owner.upgrade() else {
                    return Ok(());
                };
                if self.route.borrow().is_none() {
                    let fresh = NativeAttachment::new(&owner.native)?;
                    if let Some(old) = self.target.replace(Some(fresh)) {
                        owner.editor_remove(old);
                    }
                    self.listen();
                }
                self.update();
            } else {
                self.route.borrow_mut().take();
                // SDK lifecycle cleanup retires its borrowed edge before releasing its BuilderNode.
                // Keep this target until that cleanup or a later replacement batch acknowledges it.
                if self.state.borrow_mut().focus(false)
                    && let Some(owner) = self.owner.upgrade()
                {
                    owner.emit(EventKind::Blur, EventPayload::None);
                }
            }
            Ok(())
        });
    }
    pub(crate) fn accessibility_id(&self) -> Result<Option<i32>> {
        use ohos_arkui_binding::types::attribute::ArkUINodeAttributeType as A;
        self.with_child(|node| node.i32_attribute(A::AccessibilityId, 0))?
            .transpose()
            .map_err(Into::into)
    }
    pub(crate) fn measure(&self, event: &NodeCustomEvent) -> Result<()> {
        self.with_child(|node| crate::render::layout::ChildMeasure::new(event).child(node))?
            .transpose()?;
        Ok(())
    }
    pub(crate) fn layout(&self, event: &NodeCustomEvent, scale: f32) -> Result<()> {
        let content = self.content.get();
        self.with_child(|node| {
            event.layout_child(
                node,
                (content.x * scale).round() as i32,
                (content.y * scale).round() as i32,
            )
        })?
        .transpose()?;
        Ok(())
    }
    fn command(&self, kind: &str, start: u32, end: u32) {
        if let Some(owner) = self.owner.upgrade().filter(|_| !self.closed.get())
            && let Some(target_id) = self.target_id()
        {
            owner.editor_command(EditorCommand {
                target_id,
                kind: kind.into(),
                start,
                end,
            });
        }
    }
    pub(crate) fn focus(&self, focused: bool) -> Result<()> {
        self.command(if focused { "focus" } else { "blur" }, 0, 0);
        Ok(())
    }
    pub(crate) fn selection(&self, start: u32, end: u32) -> Result<()> {
        let (start, end) = editor_state::selection(&self.state.borrow().text, start, end);
        self.command("selection", start as u32, end as u32);
        Ok(())
    }
    pub(crate) fn hit_test(&self, mode: HitTestMode) -> Result<()> {
        if self.hit_test.replace(i32::from(mode) as u32) != i32::from(mode) as u32 {
            self.update();
        }
        Ok(())
    }
    fn retire(&self) -> Result<()> {
        self.route.borrow_mut().take();
        if let Some(owner) = self.owner.upgrade() {
            // The pending batch retains the old target until SDK retirement is acknowledged.
            if let Some(old) = self.target.borrow_mut().take() {
                owner.editor_remove(old);
            }
            if self.state.borrow_mut().focus(false) {
                owner.emit(EventKind::Blur, EventPayload::None);
            }
        }
        Ok(())
    }
    pub(crate) fn close(&self) -> Result<()> {
        self.closed.set(true);
        self.retire()
    }
    pub(crate) fn suspend(&self) -> Result<()> {
        self.focus(false)
    }
}

fn content_type(fill: nsh_core::AutoFill) -> Option<&'static str> {
    use nsh_core::AutoFill as F;
    match fill {
        F::Username => Some("username"),
        F::Password => Some("password"),
        F::NewPassword => Some("new-password"),
        F::Email => Some("email"),
        F::Tel => Some("tel"),
        F::Name | F::CreditCardName => Some("name"),
        F::GivenName => Some("given-name"),
        F::FamilyName => Some("family-name"),
        F::Nickname => Some("nickname"),
        F::StreetAddress | F::AddressLine1 => Some("street-address"),
        F::AddressLine2 => Some("address-line2"),
        F::City => Some("city"),
        F::Region => Some("region"),
        F::Country => Some("country"),
        F::CreditCardNumber => Some("credit-card"),
        _ => None,
    }
}
