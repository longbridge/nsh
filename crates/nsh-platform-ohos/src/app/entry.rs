//! OHOS host session binding. Each window owns its own `Services` session,
//! bound explicitly from its `UIContext` without global single-window state.

use std::{cell::RefCell, sync::Arc};

use napi_derive_ohos::napi;
use napi_ohos::{
    Env, Error, JsValue, Result, Status,
    bindgen_prelude::{FromNapiValue, Function, FunctionRef, Object, Unknown},
};
use ohos_resource_manager_binding::ResourceManager;

use crate::app::{
    environment::NshEnvironment,
    runtime::Message,
    session::{AvoidArea, HostCommand, HostInit, Services},
};

thread_local! {
    static SESSION_BINDER: RefCell<Option<FunctionRef<(), ()>>> = RefCell::default();
}

#[napi]
pub(crate) fn nsh_register_session_binder(
    sdk_api_version: i32,
    #[napi(ts_arg_type = "(uiContext: object) => number")] binder: Function<'_, (), ()>,
) -> Result<()> {
    crate::app::features::set_runtime_api_version(sdk_api_version);
    crate::media::webview::register_custom_scheme();
    let reference = binder.create_ref()?;
    SESSION_BINDER.with(|slot| {
        *slot.borrow_mut() = Some(reference);
    });
    Ok(())
}

pub(crate) fn bind_session(env: &Env, ui_context: &Object<'_>) -> Result<Arc<Services>> {
    let has_binder = SESSION_BINDER.with(|slot| slot.borrow().is_some());
    if !has_binder {
        let _ = env.load("@nsh/ohos");
    }
    let binder = SESSION_BINDER
        .with(|slot| {
            slot.borrow()
                .as_ref()
                .map(|reference| reference.borrow_back(env))
                .transpose()
        })?
        .ok_or_else(|| Error::from_reason("@nsh/ohos session binder is not registered"))?;
    crate::app::session::clear_pending();
    let arg = ui_context.raw();
    let mut raw_result = std::ptr::null_mut();
    let status = unsafe {
        napi_ohos::sys::napi_call_function(
            env.raw(),
            binder.raw(),
            binder.raw(),
            1,
            &arg,
            &mut raw_result,
        )
    };
    if status != napi_ohos::sys::Status::napi_ok {
        crate::app::session::clear_pending();
        return Err(Error::from_status(Status::from(status)));
    }
    let session_id = unsafe { u32::from_napi_value(env.raw(), raw_result) }.inspect_err(|_| {
        crate::app::session::clear_pending();
    })?;
    crate::app::session::take_pending_or_get(session_id)
}

#[napi]
pub(crate) fn nsh_find_session(window_name: String) -> u32 {
    crate::app::session::find_by_window_name(&window_name)
}

#[napi]
pub(crate) fn nsh_create_session(
    init: HostInit,
    window_name: String,
    platform_window_id: i32,
    #[napi(ts_arg_type = "object")] resource_manager: Unknown<'_>,
    #[napi(ts_arg_type = "(batch: Batch) => void")] apply_batch: Function<
        '_,
        crate::interop::sdk::Batch,
        (),
    >,
    #[napi(ts_arg_type = "(command: HostCommand) => void")] on_command: Function<
        '_,
        HostCommand,
        (),
    >,
) -> Result<u32> {
    if init.sdk_api_version < crate::app::features::MIN_API as i32 {
        return Err(Error::from_reason(format!(
            "Native Shell requires OHOS API {} or later",
            crate::app::features::MIN_API
        )));
    }
    crate::app::features::set_runtime_api_version(init.sdk_api_version);
    crate::media::webview::register_custom_scheme();
    let resource_manager = ResourceManager::from_napi_value(resource_manager)?;
    let applier = apply_batch.create_ref()?;
    let command_tsfn = on_command
        .build_threadsafe_function::<HostCommand>()
        .callee_handled::<false>()
        .build()?;
    Services::create(
        init,
        window_name,
        platform_window_id,
        resource_manager,
        applier,
        command_tsfn,
    )
}

#[napi]
pub(crate) fn nsh_close_session(session_id: u32) {
    crate::app::session::close_ui_session(session_id);
}

#[napi]
pub(crate) fn nsh_set_foreground(session_id: u32, foreground: bool) {
    if let Ok(host) = crate::app::session::get(session_id) {
        host.set_foreground(foreground);
    }
}

#[napi]
pub(crate) fn nsh_update_config(
    session_id: u32,
    color_mode: i32,
    font_size_scale: f64,
    language: String,
    device_type: Option<String>,
) -> Result<()> {
    if let Ok(host) = crate::app::session::get(session_id) {
        host.update(crate::app::session::environment_from_config(
            color_mode,
            font_size_scale,
            language,
            device_type.as_deref(),
        ))?;
    }
    Ok(())
}

#[napi]
pub(crate) fn nsh_update_accessibility(
    session_id: u32,
    screen_reader: Option<bool>,
    reduce_motion: Option<bool>,
) -> Result<()> {
    if let Ok(host) = crate::app::session::get(session_id) {
        host.update(NshEnvironment {
            screen_reader,
            reduce_motion: reduce_motion.filter(|_| crate::get_capabilities().api23),
            ..Default::default()
        })?;
    }
    Ok(())
}

#[napi]
pub(crate) fn nsh_update_window(
    session_id: u32,
    platform_window_id: i32,
    visual: Vec<AvoidArea>,
    keyboard: AvoidArea,
) -> Result<()> {
    if let Ok(host) = crate::app::session::get(session_id) {
        let visual = [
            visual.first().copied().unwrap_or_default(),
            visual.get(1).copied().unwrap_or_default(),
            visual.get(2).copied().unwrap_or_default(),
        ];
        host.update_window(platform_window_id, visual, keyboard)?;
    }
    Ok(())
}

#[napi]
pub(crate) fn nsh_open_url_result(session_id: u32, open_id: u32, opened: bool) {
    if let Ok(host) = crate::app::session::get(session_id) {
        host.complete_open_url(open_id, opened);
    }
}

#[napi]
pub(crate) fn nsh_pick_images_result(session_id: u32, pick_id: u32, paths: Vec<String>) {
    if let Ok(host) = crate::app::session::get(session_id) {
        host.complete_pick_images(pick_id, paths);
    } else {
        for path in &paths {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[napi]
pub(crate) fn nsh_report_host_error(session_id: u32, message: String) {
    if let Ok(host) = crate::app::session::get(session_id) {
        host.broadcast(|| Message::Error(message.clone()));
    }
}

#[napi]
pub(crate) fn on_back_press_intercept(
    env: Env,
    #[napi(ts_arg_type = "import('@ohos.arkui.UIContext').UIContext")] ui_context: Option<
        Object<'_>,
    >,
) -> bool {
    if let Some(ui_context) = ui_context {
        return bind_session(&env, &ui_context).is_ok_and(|host| host.handle_back());
    }
    crate::app::session::handle_back_any()
}
