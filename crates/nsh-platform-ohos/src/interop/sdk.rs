//! NSH-owned SDK fragments that have no equivalent C attribute or constructor.
//! Rust owns policy and native attachment targets retain SDK owners on the UI thread.

use std::{cell::RefCell, rc::Weak};

use napi_derive_ohos::napi;
use napi_ohos::{Env, Error, Result, bindgen_prelude::Object};
use rustc_hash::FxHashMap;

use crate::{
    interop::attachment::NativeAttachment,
    render::node::{NativeNode, Node},
};

#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct EditorSettings {
    pub target_id: u32,
    pub multiline: bool,
    pub secure: bool,
    pub text: String,
    pub count: u32,
    pub width: f64,
    pub height: f64,
    pub placeholder: String,
    pub font_size: f64,
    pub font_weight: u32,
    pub font_family: Option<String>,
    pub italic: bool,
    pub color: u32,
    pub placeholder_color: u32,
    pub caret_color: u32,
    pub selection_color: u32,
    pub editable: bool,
    pub scrollbar_visible: bool,
    pub hit_test: u32,
    pub align: u32,
    pub rtl: bool,
    pub capitalize: u32,
    pub keyboard: String,
    pub enter_key: String,
    pub blur_on_submit: bool,
    pub max_length: Option<u32>,
    pub auto_fill: bool,
    pub content_type: Option<String>,
    pub auto_focus: bool,
    pub accessibility: Option<EditorAccessibility>,
}

#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct EditorAccessibility {
    pub label: String,
    pub hint: String,
    pub hidden: bool,
    pub accessible: Option<bool>,
    pub disabled: bool,
}

/// Focus and selection operations ordered after this batch's state updates.
#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct EditorCommand {
    pub target_id: u32,
    pub kind: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Default)]
#[napi(object)]
pub(crate) struct EditorEvent {
    pub kind: String,
    pub text: String,
    pub count: u32,
    pub start: u32,
    pub end: u32,
    pub width: f64,
    pub height: f64,
}

#[napi(object)]
pub(crate) struct EditorReceipt {
    pub accepted: bool,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct MenuItem {
    pub name: String,
    pub label: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct Menu {
    pub generation: u32,
    pub visible: bool,
    pub x: f64,
    pub y: f64,
    pub items: Vec<MenuItem>,
}

#[napi(object)]
pub(crate) struct MenuAction {
    pub target_id: u32,
    pub action: String,
    pub kind: Option<String>,
    pub generation: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
#[napi(object)]
pub(crate) struct TextureInsets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
#[napi(object)]
pub(crate) struct ViewOptions {
    pub target_id: u32,
    pub width: f64,
    pub height: f64,
    pub hue: f64,
    pub texture_surface: Option<String>,
    pub texture_insets: Option<TextureInsets>,
    pub test_id: Option<String>,
    pub semantic: bool,
    pub clickable: bool,
    pub disabled: bool,
    pub accessible: Option<bool>,
    pub hidden: bool,
    pub label: String,
    pub hint: String,
    pub value: String,
    pub role: String,
    pub checked: Option<bool>,
    pub selected: bool,
    pub api23: bool,
    pub api26: bool,
    pub actions: Vec<MenuItem>,
    pub menu: Option<Menu>,
}

#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct WebSettings {
    pub target_id: u32,
    pub url: Option<String>,
    pub html: Option<String>,
    pub html_base_url: Option<String>,
    pub load_generation: u32,
    pub java_script_enabled: bool,
    pub scroll_enabled: bool,
    pub user_agent: Option<String>,
    pub injected_javascript: Option<String>,
    pub document_start_script: String,
}

#[derive(Clone, Debug, PartialEq)]
#[napi(object)]
pub(crate) struct WebCommand {
    pub target_id: u32,
    pub kind: String,
    pub script: Option<String>,
}

#[napi(object)]
pub(crate) struct WebEvent {
    pub target_id: u32,
    pub phase: String,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub message: Option<String>,
    pub code: Option<i32>,
}

// Only weak routes live here. Nodes, native targets and engines retain their own lifetime.
struct ViewRegistration {
    owner: Weak<Node>,
    inner: std::rc::Weak<std::cell::RefCell<ohos_arkui_binding::common::node::ArkUINode>>,
}
thread_local! {
    static VIEWS: RefCell<FxHashMap<u32, ViewRegistration>> = RefCell::default();
    static EDITORS: RefCell<FxHashMap<u32, Weak<crate::input::editor::Input>>> = RefCell::default();
}

pub(crate) struct ViewRoute {
    id: u32,
    owner: Weak<Node>,
}
impl ViewRoute {
    pub(crate) fn new(id: u32, owner: Weak<Node>, inner: &NativeNode) -> Self {
        VIEWS.with(|views| {
            views.borrow_mut().insert(
                id,
                ViewRegistration {
                    owner: owner.clone(),
                    inner: std::rc::Rc::downgrade(inner),
                },
            )
        });
        Self { id, owner }
    }
}
impl Drop for ViewRoute {
    fn drop(&mut self) {
        VIEWS.with(|views| {
            let mut views = views.borrow_mut();
            if views
                .get(&self.id)
                .is_some_and(|route| route.owner.ptr_eq(&self.owner))
            {
                views.remove(&self.id);
            }
        });
    }
}

pub(crate) struct EditorRoute {
    id: u32,
}
impl EditorRoute {
    pub(crate) fn new(id: u32, input: Weak<crate::input::editor::Input>) -> Self {
        EDITORS.with(|routes| routes.borrow_mut().insert(id, input));
        Self { id }
    }
}
impl Drop for EditorRoute {
    fn drop(&mut self) {
        EDITORS.with(|routes| routes.borrow_mut().remove(&self.id));
    }
}

#[derive(Default, Clone)]
#[napi(object)]
pub(crate) struct Batch {
    pub editors: Vec<EditorSettings>,
    pub commands: Vec<EditorCommand>,
    pub remove_editors: Vec<u32>,
    pub views: Vec<ViewOptions>,
    pub remove_views: Vec<u32>,
    pub webs: Vec<WebSettings>,
    pub web_commands: Vec<WebCommand>,
    pub remove_webs: Vec<u32>,
}

impl Batch {
    pub(crate) fn is_empty(&self) -> bool {
        self.editors.is_empty()
            && self.commands.is_empty()
            && self.remove_editors.is_empty()
            && self.views.is_empty()
            && self.remove_views.is_empty()
            && self.webs.is_empty()
            && self.web_commands.is_empty()
            && self.remove_webs.is_empty()
    }
}

pub(crate) fn apply(
    context: &crate::render::mount::UiContext,
    batch: Batch,
) -> crate::render::node::Result<()> {
    context.host.apply_batch(&context.env, batch)?;
    Ok(())
}

#[napi]
pub(crate) fn nsh_attach_native_node(
    env: Env,
    target_id: u32,
    #[napi(ts_arg_type = "FrameNode")] content: Object<'_>,
) -> Result<()> {
    NativeAttachment::attach(&env, target_id, &content)
}

#[napi]
pub(crate) fn nsh_attach_native_subtree(
    env: Env,
    target_id: u32,
    #[napi(ts_arg_type = "FrameNode")] content: Object<'_>,
    #[napi(ts_arg_type = "NodeContent")] slot: Object<'_>,
) -> Result<()> {
    let (_owner, inner) = VIEWS
        .with(|views| {
            views
                .borrow()
                .get(&target_id)
                .and_then(|route| route.owner.upgrade().zip(route.inner.upgrade()))
        })
        .ok_or_else(|| Error::from_reason("Native subtree route is no longer live"))?;
    ohos_arkui_binding::common::node_registry::register(&inner.borrow(), inner.clone())
        .and_then(|()| inner.borrow().add_event_receiver())
        .map_err(|error| Error::from_reason(error.to_string()))?;
    NativeAttachment::attach_subtree(&env, target_id, &content, &slot)
}

#[napi]
pub(crate) fn nsh_retire_native_node(target_id: u32) -> Result<()> {
    NativeAttachment::retire(target_id)
}

#[napi]
pub(crate) fn nsh_editor_event(target_id: u32, event: EditorEvent) -> Result<EditorReceipt> {
    let count = NativeAttachment::dispatch(target_id, || {
        let input = EDITORS.with(|routes| routes.borrow().get(&target_id).and_then(Weak::upgrade));
        input
            .map(|input| {
                input
                    .native_event(event)
                    .map_err(|error| Error::from_reason(error.to_string()))
            })
            .transpose()
    })??;
    Ok(EditorReceipt {
        accepted: count.is_some(),
        count: count.unwrap_or(0),
    })
}

#[napi]
pub(crate) fn nsh_view_action(action: MenuAction) -> Result<bool> {
    NativeAttachment::dispatch(action.target_id, || {
        let owner = VIEWS.with(|views| {
            views
                .borrow()
                .get(&action.target_id)
                .and_then(|route| route.owner.upgrade())
        });
        owner.is_some_and(|owner| {
            let mut accepted = false;
            crate::render::node::callback(|| {
                if action.kind.is_some() {
                    accepted = owner.selection_action(&action)?;
                } else {
                    let spec = owner.accessibility_state.spec.borrow();
                    if !spec.hidden && !spec.state.disabled && owner.visible_in_tree() {
                        let registered = spec
                            .actions
                            .iter()
                            .any(|item| item.name.as_ref() == action.action);
                        drop(spec);
                        if registered {
                            owner.emit(
                                nsh_core::EventKind::AccessibilityAction,
                                nsh_core::EventPayload::Map(vec![(
                                    "actionName",
                                    nsh_core::PayloadValue::Str(action.action.clone()),
                                )]),
                            );
                            accepted = true;
                        } else if action.action == "activate"
                            && owner
                                .interaction
                                .mask()
                                .contains(nsh_core::EventKind::Press)
                        {
                            owner.emit(nsh_core::EventKind::Press, nsh_core::EventPayload::None);
                            accepted = true;
                        }
                    }
                }
                Ok(())
            });
            accepted
        })
    })
}

#[napi]
pub(crate) fn nsh_web_controller_attached(target_id: u32, native_tag: String) -> Result<bool> {
    crate::media::webview::on_controller_attached(target_id, &native_tag)
}

#[napi]
pub(crate) fn nsh_web_navigate(target_id: u32, url: String) -> Result<bool> {
    crate::media::webview::on_navigate(target_id, &url)
}

#[napi]
pub(crate) fn nsh_web_event(event: WebEvent) -> Result<bool> {
    crate::media::webview::on_event(event)
}
