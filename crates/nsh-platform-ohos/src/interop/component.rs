//! Host-owned native widgets. Factories run on the ArkTS thread before their
//! first property batch; their FrameNode owner stays retained until detachment.
//! Instance handles survive source replacement without reusing node identifiers.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use napi_derive_ohos::napi;
use napi_ohos::{
    Env,
    bindgen_prelude::{FnArgs, Function, FunctionRef, Object, ObjectRef},
};
use nsh_core::{JsonValue, NodeId};
use nsh_engine::EngineControl;
use ohos_arkui_binding::{component::built_in_component::Stack, types::advanced::LengthMetricUnit};
use rustc_hash::FxHashMap;

use crate::{
    interop::attachment::NativeAttachment,
    render::node::{NativeNode, Result},
};

/// One changed property of a host native component. A component receives
/// one array per batch; values cross Node-API directly without JSON strings.
#[napi(object)]
pub struct NshNativeProperty {
    /// Property name.
    pub name: String,
    /// Property value; `removed` distinguishes a removed property from null.
    #[napi(ts_type = "string | number | boolean | object | null")]
    pub value: JsonValue,
    /// The property was removed.
    pub removed: bool,
}
type Factory = FunctionRef<u32, ObjectRef>;
#[derive(Clone)]
pub(crate) struct Context {
    pub(crate) node: NodeId,
    pub(crate) control: EngineControl,
    active: Rc<Cell<bool>>,
}
#[derive(Default)]
pub(crate) struct Registry {
    factories: FxHashMap<Arc<str>, Rc<Factory>>,
    contexts: FxHashMap<u32, Context>,
    next: u32,
}
#[derive(Clone, Default)]
pub(crate) struct Components(Rc<RefCell<Registry>>);
impl Components {
    pub(crate) fn register(&self, name: String, factory: Factory) {
        self.0
            .borrow_mut()
            .factories
            .insert(name.into(), Rc::new(factory));
    }
    pub(crate) fn unregister(&self, name: &str) {
        self.0.borrow_mut().factories.remove(name);
    }
    pub(crate) fn clear(&self) {
        self.0.borrow_mut().factories.clear();
    }
    pub(crate) fn context(&self, handle: u32) -> napi_ohos::Result<Context> {
        self.0
            .borrow()
            .contexts
            .get(&handle)
            .filter(|context| context.active.get())
            .cloned()
            .ok_or_else(|| napi_ohos::Error::from_reason("native component is disposed"))
    }
    pub(crate) fn make(
        &self,
        env: Env,
        name: &str,
        node: NodeId,
        control: EngineControl,
    ) -> Result<Option<Instance>> {
        let (factory, handle, context) = {
            let mut registry = self.0.borrow_mut();
            let Some(factory) = registry.factories.get(name).cloned() else {
                log::warn!("OHOS native component is not registered: {name}");
                return Ok(None);
            };
            registry.next = registry.next.checked_add(1).ok_or_else(|| {
                napi_ohos::Error::from_reason("native component handles exhausted")
            })?;
            let handle = registry.next;
            let context = Context {
                node,
                control,
                active: Rc::new(Cell::new(true)),
            };
            registry.contexts.insert(handle, context.clone());
            (factory, handle, context)
        };
        // Release the registry borrow before calling the host: it can register
        // another factory, emit events or report intrinsic size synchronously.
        let registration = Registration {
            components: self.clone(),
            handle,
            context,
        };
        let value = factory.borrow_back(&env)?.call(handle)?;
        let retained = RetainedObject {
            value: Some(value),
            env,
            disposed: Cell::new(false),
        };
        let object = retained.object()?;
        let node: Object<'_> = object.get("node")?.ok_or_else(|| {
            napi_ohos::Error::from_reason(
                "native component factory must return an object with node",
            )
        })?;
        // Ability retains the callback-scoped FrameNode until its native
        // borrowing edge is retired. The business object remains independently
        // retained for props, commands and the host's disposal callback.
        let native = Stack::new()?.into_node();
        native
            .borrow()
            .set_length_metric_unit(LengthMetricUnit::Vp)?;
        let target = NativeAttachment::new(&native)?;
        NativeAttachment::attach(&env, target.id(), &node)?;
        Ok(Some(Instance {
            target,
            native,
            retained: Some(retained),
            registration,
            disposed: Cell::new(false),
        }))
    }
}
struct Registration {
    components: Components,
    handle: u32,
    context: Context,
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.context.active.set(false);
        self.components.0.borrow_mut().contexts.remove(&self.handle);
    }
}
struct RetainedObject {
    value: Option<ObjectRef>,
    env: Env,
    disposed: Cell<bool>,
}
impl RetainedObject {
    fn object(&self) -> napi_ohos::Result<Object<'_>> {
        self.value
            .as_ref()
            .ok_or_else(|| napi_ohos::Error::from_reason("native component is disposed"))?
            .get_value(&self.env)
    }
    fn dispose(&self) -> napi_ohos::Result<()> {
        if self.disposed.replace(true) {
            return Ok(());
        }
        self.env.run_in_scope(|| {
            let object = self.object()?;
            let dispose: Option<Function<'_, (), ()>> = object.get("dispose")?;
            if let Some(dispose) = dispose {
                dispose.apply(object, ())?;
            }
            Ok(())
        })
    }
}
impl Drop for RetainedObject {
    fn drop(&mut self) {
        if let Err(error) = self.dispose() {
            log::error!("OHOS component disposal: {error}");
        }
        if let Some(value) = self.value.take()
            && let Err(error) = value.unref(&self.env)
        {
            log::error!("OHOS component release: {error}");
        }
    }
}
pub(crate) struct Instance {
    target: NativeAttachment,
    pub(crate) native: NativeNode,
    retained: Option<RetainedObject>,
    registration: Registration,
    disposed: Cell<bool>,
}
impl Instance {
    fn object(&self) -> napi_ohos::Result<Object<'_>> {
        self.retained
            .as_ref()
            .filter(|_| !self.disposed.get())
            .ok_or_else(|| napi_ohos::Error::from_reason("native component is disposed"))?
            .object()
    }
    pub(crate) fn deactivate(&self) {
        self.registration.context.active.set(false);
    }
    pub(crate) fn props(&self, props: Vec<(Arc<str>, Option<JsonValue>)>) -> Result<()> {
        let object = self.object()?;
        let update: Option<Function<'_, Vec<NshNativeProperty>, ()>> = object.get("updateProps")?;
        if let Some(update) = update {
            update.apply(
                object,
                props
                    .into_iter()
                    .map(|(name, value)| NshNativeProperty {
                        name: name.to_string(),
                        removed: value.is_none(),
                        value: value.unwrap_or(JsonValue::Null),
                    })
                    .collect(),
            )?;
        }
        Ok(())
    }
    pub(crate) fn command(&self, name: &str, args: &JsonValue) -> Result<()> {
        let object = self.object()?;
        let command: Option<Function<'_, FnArgs<(String, JsonValue)>, ()>> =
            object.get("command")?;
        if let Some(command) = command {
            command.apply(object, (name.to_owned(), args.clone()).into())?;
        }
        Ok(())
    }
    pub(crate) fn dispose(&self) -> Result<()> {
        self.deactivate();
        if self.disposed.get() {
            return Ok(());
        }
        self.target.invalidate()?;
        if self.native.borrow().is_owned() {
            self.native.borrow_mut().dispose()?;
        }
        // Detachment is committed before calling the host, which may throw.
        self.disposed.set(true);
        if let Some(retained) = &self.retained {
            retained.dispose()?;
        }
        Ok(())
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        if let Err(error) = self.dispose() {
            if !self.disposed.get() {
                // The C container still borrows the ArkTS node. Keep both owners
                // alive permanently when the final native detach retry fails.
                std::mem::forget((self.retained.take(), self.native.clone()));
                log::error!(
                    "OHOS native component detachment failed; retaining the native container \
                     and ArkTS owner until process exit: {error}"
                );
            } else {
                log::error!("OHOS native component disposal: {error}");
            }
        }
    }
}
