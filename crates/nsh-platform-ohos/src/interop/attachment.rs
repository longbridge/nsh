//! UI-thread ArkUI `NodeContent` roots and `BuilderNode` `FrameNode` attachments.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt,
    rc::Rc,
    sync::atomic::{AtomicU32, Ordering},
};

use napi_ohos::{
    Env, Error, JsValue, Result,
    bindgen_prelude::{FromNapiValue, Function, Object, ObjectRef},
};
use ohos_arkui_binding::{
    ArkUIHandle, RootNode,
    api::node_content::NodeContent,
    common::{
        error::ArkUIResult,
        node::{ArkUINode, ArkUINodeRaw},
    },
    component::attribute::ArkUICommonAttribute,
};

fn allocate_id(counter: &AtomicU32) -> Option<u32> {
    let mut id = counter.load(Ordering::Relaxed);
    loop {
        let next = id.checked_add(1)?;
        match counter.compare_exchange_weak(id, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Some(id),
            Err(current) => id = current,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum AdmissionError {
    Retired,
    Busy,
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Retired => "is retired",
            Self::Busy => "already has an operation in progress",
        })
    }
}

#[derive(Default)]
struct AttachmentState {
    retired: Cell<bool>,
    active: Cell<bool>,
}

impl AttachmentState {
    fn begin(&self) -> std::result::Result<Active<'_>, AdmissionError> {
        if self.retired.get() {
            return Err(AdmissionError::Retired);
        }
        self.enter()
    }

    fn begin_retire(&self) -> std::result::Result<Active<'_>, AdmissionError> {
        let active = self.enter()?;
        self.retired.set(true);
        Ok(active)
    }

    fn enter(&self) -> std::result::Result<Active<'_>, AdmissionError> {
        if self.active.replace(true) {
            Err(AdmissionError::Busy)
        } else {
            Ok(Active(&self.active))
        }
    }
}

#[must_use]
struct Active<'a>(&'a Cell<bool>);

impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Owns the platform `NodeContent` object, native root and window listener on the creating UI thread.
pub(crate) struct ContentRoot {
    owner: Option<ContentOwner>,
}

struct ContentOwner {
    root: RootNode,
    events: NodeContent,
    value: Option<ObjectRef>,
    env: Env,
}

impl ContentRoot {
    pub(crate) fn new(env: Env) -> Result<Self> {
        let module = env.load("@ohos.arkui.node")?;
        let constructor: Function<'_, (), Object<'_>> = module.get("NodeContent")?;
        let value = constructor.new_instance(())?;
        // SAFETY: the system constructor returned a live NodeContent in this callback.
        let object = unsafe { Object::from_napi_value(env.raw(), value.raw()) }?;
        // SAFETY: the live object belongs to this callback; it is retained below
        // before any native root or event callback can use the handle.
        let handle = unsafe { ArkUIHandle::from_napi_value(env.raw(), object.raw()) }?;
        let value = object.create_ref()?;
        Ok(Self {
            owner: Some(ContentOwner {
                root: RootNode::new(handle),
                events: NodeContent::from_handle(handle),
                value: Some(value),
                env,
            }),
        })
    }

    pub(crate) fn object<'env>(&self, env: &'env Env) -> Result<Object<'env>> {
        let owner = self.owner.as_ref().expect("live ContentRoot");
        if owner.env.raw() != env.raw() {
            return Err(Error::from_reason(
                "ContentRoot belongs to another N-API environment",
            ));
        }
        owner
            .value
            .as_ref()
            .expect("retained NodeContent")
            .get_value(env)
    }

    pub(crate) fn mount(&mut self, node: Rc<RefCell<ArkUINode>>) -> ArkUIResult<()> {
        self.owner
            .as_mut()
            .expect("live ContentRoot")
            .root
            .mount_node(node)
    }

    pub(crate) fn unmount(&mut self) -> ArkUIResult<()> {
        self.owner
            .as_mut()
            .expect("live ContentRoot")
            .root
            .unmount()
    }

    pub(crate) fn on_window_change(
        &mut self,
        callback: impl Fn(bool) + 'static,
    ) -> ArkUIResult<()> {
        self.owner
            .as_mut()
            .expect("live ContentRoot")
            .events
            .register_callback(move |event| {
                callback(
                    event.event_type()
                        == ohos_arkui_binding::types::advanced::NodeContentEventType::OnAttachToWindow,
                );
            })
    }

    pub(crate) fn clear_window_callback(&mut self) -> ArkUIResult<()> {
        self.owner
            .as_mut()
            .expect("live ContentRoot")
            .events
            .clear_callback()
    }
}

impl Drop for ContentRoot {
    fn drop(&mut self) {
        let Some(mut owner) = self.owner.take() else {
            return;
        };
        if let Err(error) = owner
            .events
            .clear_callback()
            .and_then(|()| owner.root.unmount())
        {
            log::error!("Failed to retire ContentRoot; retaining its native owners: {error}");
            std::mem::forget(owner);
            return;
        }
        if let Some(value) = owner.value.take()
            && let Err(error) = value.unref(&owner.env)
        {
            log::error!("Failed to release ContentRoot reference: {error}");
        }
    }
}

static NEXT_TARGET_ID: AtomicU32 = AtomicU32::new(1);

thread_local! {
    static TARGETS: RefCell<BTreeMap<u32, Rc<Target>>> = RefCell::default();
}

struct RetainedContent {
    env: Env,
    frame: Option<ObjectRef>,
    slot: Option<ObjectRef>,
}

impl RetainedContent {
    fn new(env: &Env, frame: &Object<'_>, slot: Option<&Object<'_>>) -> Result<Self> {
        let mut retained = Self {
            env: *env,
            frame: Some(frame.create_ref()?),
            slot: None,
        };
        retained.slot = slot.map(Object::create_ref).transpose()?;
        Ok(retained)
    }
}

impl Drop for RetainedContent {
    fn drop(&mut self) {
        for reference in [self.slot.take(), self.frame.take()].into_iter().flatten() {
            if let Err(error) = reference.unref(&self.env) {
                log::error!("Failed to release native attachment reference: {error}");
            }
        }
    }
}

struct Target {
    id: u32,
    state: AttachmentState,
    parent: RefCell<Option<Rc<RefCell<ArkUINode>>>>,
    child: RefCell<Option<Rc<RefCell<ArkUINode>>>>,
    inner: RefCell<Option<Rc<RefCell<ArkUINode>>>>,
    inner_receiver_active: Cell<bool>,
    slot: Cell<Option<ArkUIHandle>>,
    retained: RefCell<Option<RetainedContent>>,
}

impl Target {
    fn begin(&self) -> Result<Active<'_>> {
        self.state.begin().map_err(|error| {
            Error::from_reason(format!("Native attachment target {} {error}", self.id))
        })
    }

    fn detach_inner(&self) -> Result<()> {
        let inner = self.inner.borrow();
        if let (Some(handle), Some(inner)) = (self.slot.get(), inner.as_ref()) {
            if self.inner_receiver_active.get() {
                inner
                    .borrow()
                    .remove_event_receiver()
                    .map_err(|error| Error::from_reason(error.to_string()))?;
                self.inner_receiver_active.set(false);
            }
            NodeContent::from_handle(handle)
                .remove_node(&inner.borrow())
                .map_err(|error| Error::from_reason(error.to_string()))?;
            self.slot.set(None);
            ohos_arkui_binding::common::node_registry::unregister(&inner.borrow());
        }
        if let Some(child) = self.child.borrow().clone() {
            let parent = self.parent.borrow();
            let parent = parent
                .as_ref()
                .ok_or_else(|| Error::from_reason("Native attachment parent is retired"))?;
            let mut parent = parent.borrow_mut();
            let index = parent
                .children()
                .iter()
                .position(|node| Rc::ptr_eq(node, &child));
            let native_parent = child
                .borrow()
                .parent()
                .map_err(|error| Error::from_reason(error.to_string()))?;
            if let Some(native_parent) = native_parent {
                if native_parent != parent.raw_handle() {
                    return Err(Error::from_reason(
                        "Native attachment was reparented outside its target",
                    ));
                }
                if let Some(index) = index {
                    parent
                        .remove_child(index)
                        .map_err(|error| Error::from_reason(error.to_string()))?;
                } else {
                    child
                        .borrow()
                        .remove_from_parent()
                        .map_err(|error| Error::from_reason(error.to_string()))?;
                }
            } else if let Some(index) = index {
                parent.children_mut().remove(index);
            }
        }
        self.child.borrow_mut().take();
        self.retained.borrow_mut().take();
        Ok(())
    }

    fn retire(&self) -> Result<()> {
        // A failed detachment still closes admission, but preserves all native owners for retry.
        let _active = self.state.begin_retire().map_err(|error| {
            Error::from_reason(format!("Native attachment target {} {error}", self.id))
        })?;
        self.detach_inner()?;
        let dispose_inner = self
            .inner
            .borrow()
            .as_ref()
            .is_some_and(|inner| inner.borrow().is_owned() && Rc::strong_count(inner) == 1);
        if dispose_inner {
            let inner = self.inner.borrow();
            let inner = inner.as_ref().expect("live inner node");
            inner
                .borrow()
                .remove_from_parent()
                .map_err(|error| Error::from_reason(error.to_string()))?;
            inner
                .borrow_mut()
                .dispose()
                .map_err(|error| Error::from_reason(error.to_string()))?;
        }
        self.inner.borrow_mut().take();
        let dispose_parent = self
            .parent
            .borrow()
            .as_ref()
            .is_some_and(|parent| parent.borrow().is_owned() && Rc::strong_count(parent) == 1);
        if dispose_parent {
            let parent = self.parent.borrow();
            let parent = parent.as_ref().expect("live parent node");
            parent
                .borrow()
                .remove_from_parent()
                .map_err(|error| Error::from_reason(error.to_string()))?;
            parent
                .borrow_mut()
                .dispose()
                .map_err(|error| Error::from_reason(error.to_string()))?;
        }
        self.parent.borrow_mut().take();
        Ok(())
    }
}

fn target(id: u32) -> Result<Rc<Target>> {
    TARGETS
        .with(|targets| targets.borrow().get(&id).cloned())
        .ok_or_else(|| Error::from_reason(format!("Unknown native attachment target {id}")))
}

/// The single UI-thread owner of an externally constructed native subtree.
pub(crate) struct NativeAttachment(Rc<Target>);

impl NativeAttachment {
    pub(crate) fn new(parent: &Rc<RefCell<ArkUINode>>) -> Result<Self> {
        Self::create(parent, None)
    }

    pub(crate) fn for_subtree(
        parent: &Rc<RefCell<ArkUINode>>,
        inner: &Rc<RefCell<ArkUINode>>,
    ) -> Result<Self> {
        if Rc::ptr_eq(parent, inner) {
            return Err(Error::from_reason(
                "A subtree attachment needs distinct owned nodes",
            ));
        }
        Self::create(parent, Some(inner.clone()))
    }

    fn create(
        parent: &Rc<RefCell<ArkUINode>>,
        inner: Option<Rc<RefCell<ArkUINode>>>,
    ) -> Result<Self> {
        if !parent.borrow().is_owned() || parent.borrow().raw_handle().is_null() {
            return Err(Error::from_reason(
                "native retention requires an owned node",
            ));
        }
        ArkUINode::bind_rc(parent);
        if let Some(inner) = inner.as_ref() {
            if !inner.borrow().is_owned() || inner.borrow().raw_handle().is_null() {
                return Err(Error::from_reason(
                    "native retention requires an owned node",
                ));
            }
            ArkUINode::bind_rc(inner);
        }
        let id = allocate_id(&NEXT_TARGET_ID)
            .ok_or_else(|| Error::from_reason("Native attachment IDs exhausted"))?;
        let target = Rc::new(Target {
            id,
            state: AttachmentState::default(),
            parent: RefCell::new(Some(parent.clone())),
            child: RefCell::default(),
            inner: RefCell::new(inner),
            inner_receiver_active: Cell::new(false),
            slot: Cell::new(None),
            retained: RefCell::default(),
        });
        TARGETS.with(|targets| targets.borrow_mut().insert(id, target.clone()));
        Ok(Self(target))
    }

    pub(crate) fn id(&self) -> u32 {
        self.0.id
    }

    /// Runs a platform callback under the same guard as attachment and retirement.
    pub(crate) fn dispatch<R>(id: u32, callback: impl FnOnce() -> R) -> Result<R> {
        let target = target(id)?;
        let _active = target.begin()?;
        if target.child.borrow().is_none() {
            return Err(Error::from_reason(format!(
                "Native attachment target {id} is not attached"
            )));
        }
        Ok(callback())
    }

    pub(crate) fn with_child<R>(
        &self,
        callback: impl FnOnce(&ArkUINode) -> R,
    ) -> Result<Option<R>> {
        let _active = self.0.begin()?;
        let child = self.0.child.borrow().clone();
        Ok(child.map(|child| callback(&child.borrow())))
    }

    /// Attaches and strongly retains the FrameNode until retirement succeeds.
    pub(crate) fn attach(env: &Env, id: u32, content: &Object<'_>) -> Result<()> {
        Self::attach_content(env, id, content, None)
    }

    /// Retains both the wrapper FrameNode and its NodeContent slot.
    pub(crate) fn attach_subtree(
        env: &Env,
        id: u32,
        content: &Object<'_>,
        slot: &Object<'_>,
    ) -> Result<()> {
        Self::attach_content(env, id, content, Some(slot))
    }

    fn attach_content(
        env: &Env,
        id: u32,
        content: &Object<'_>,
        slot: Option<&Object<'_>>,
    ) -> Result<()> {
        let target = target(id)?;
        let _active = target.begin()?;
        if target.child.borrow().is_some() {
            return Err(Error::from_reason("Native attachment is already occupied"));
        }
        let parent = target.parent.borrow();
        let parent = parent
            .as_ref()
            .ok_or_else(|| Error::from_reason("Native attachment parent is retired"))?;
        let inner = target.inner.borrow();
        if slot.is_some() != inner.is_some() {
            return Err(Error::from_reason(
                "Native attachment subtree does not match its registered target",
            ));
        }
        let retained = RetainedContent::new(env, content, slot)?;
        // SAFETY: conversion is scoped to this active N-API callback; `retained`
        // keeps the FrameNode alive until after the native edge is detached.
        let raw = unsafe { ArkUINodeRaw::from_napi_value(env.raw(), content.raw()) }?;
        // SAFETY: the target retains the FrameNode and never disposes its borrowed native node.
        let child = Rc::new(RefCell::new(
            unsafe { raw.borrowed_node() }
                .map_err(|error| Error::from_reason(error.to_string()))?,
        ));
        let handle = slot
            .map(|slot| {
                // SAFETY: the retained NodeContent belongs to this callback's N-API environment.
                unsafe { ArkUIHandle::from_napi_value(env.raw(), slot.raw()) }
            })
            .transpose()?;
        if let Some(inner) = inner.as_ref() {
            inner
                .borrow()
                .remove_event_receiver()
                .map_err(|error| Error::from_reason(error.to_string()))?;
            if let Err(error) = inner.borrow().remove_from_parent() {
                if let Err(restore_error) = inner.borrow().add_event_receiver() {
                    log::error!("Failed to restore native subtree event receiver: {restore_error}");
                }
                return Err(Error::from_reason(error.to_string()));
            }
            parent
                .borrow_mut()
                .children_mut()
                .retain(|child| !Rc::ptr_eq(child, inner));
        }
        parent
            .borrow_mut()
            .add_existing_node(child.clone())
            .map_err(|error| Error::from_reason(error.to_string()))?;
        *target.child.borrow_mut() = Some(child.clone());
        *target.retained.borrow_mut() = Some(retained);
        if let (Some(handle), Some(inner)) = (handle, inner.as_ref()) {
            if let Err(error) = NodeContent::from_handle(handle).add_node(&inner.borrow()) {
                target.detach_inner()?;
                return Err(Error::from_reason(error.to_string()));
            }
            target.slot.set(Some(handle));
            if let Err(error) =
                ohos_arkui_binding::common::node_registry::register(&inner.borrow(), inner.clone())
                    .and_then(|()| inner.borrow().add_event_receiver())
            {
                target.detach_inner()?;
                return Err(Error::from_reason(error.to_string()));
            }
            target.inner_receiver_active.set(true);
            let _ = inner
                .borrow()
                .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedMeasure);
            let _ = inner
                .borrow()
                .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedLayout);
            let _ = inner
                .borrow()
                .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedRender);
        }
        let _ = child
            .borrow()
            .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedMeasure);
        let _ = child
            .borrow()
            .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedLayout);
        let _ = parent
            .borrow()
            .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedMeasure);
        let _ = parent
            .borrow()
            .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedLayout);
        Ok(())
    }

    /// Permanently retires the target. Failure preserves its references for a UI-thread retry.
    pub(crate) fn retire(id: u32) -> Result<()> {
        target(id)?.retire()
    }

    pub(crate) fn invalidate(&self) -> Result<()> {
        self.0.retire()
    }
}

impl Drop for NativeAttachment {
    fn drop(&mut self) {
        if let Err(error) = self.invalidate() {
            log::error!("Failed to retire native attachment {}: {error}", self.id());
        } else {
            TARGETS.with(|targets| targets.borrow_mut().remove(&self.id()));
        }
    }
}
