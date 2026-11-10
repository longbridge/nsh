//! Actual-Wasm ownership fixtures, excluded from published artifacts.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use nsh_core::{
    JsonValue,
    element::{Plugin, PluginDependency},
    prelude::*,
    runtime::ThemePlugin,
};

thread_local! {
    static KEPT: RefCell<Vec<Callback<(), i32>>> = const { RefCell::new(Vec::new()) };
    static LISTENING: RefCell<Vec<(u32, Callback<(), i32>)>> = const { RefCell::new(Vec::new()) };
    static PLUGIN_STOPS: Cell<u32> = const { Cell::new(0) };
}

#[derive(Default)]
struct WorkspaceTheme {
    theme: RefCell<Option<Rc<ThemePlugin>>>,
    calls: Cell<u32>,
}

impl Plugin for WorkspaceTheme {
    const ID: &'static str = "test.workspace-theme";
    const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new("nsh.theme", 1)];

    fn start(&self, system: &System) -> Result<(), String> {
        self.theme.replace(Some(
            system
                .plugin::<ThemePlugin>()
                .map_err(|e| e.message().to_owned())?,
        ));
        Ok(())
    }

    fn stop(&self) {
        self.theme.take();
        PLUGIN_STOPS.with(|stops| stops.set(stops.get() + 1));
    }
}

#[module]
pub(crate) struct PluginProbe;

#[module(flat)]
impl PluginProbe {
    pub(crate) fn apply_workspace(
        theme: JsonValue,
        system: System,
        cx: CallCx<'_>,
    ) -> Result<(), JsError> {
        let workspace = system.plugin::<WorkspaceTheme>()?;
        let provider = workspace
            .theme
            .borrow()
            .clone()
            .ok_or_else(|| JsError::Error("the workspace theme was stopped".into()))?;
        provider.set(Some(&theme), cx)?;
        workspace.calls.set(workspace.calls.get() + 1);
        Ok(())
    }

    pub(crate) fn workspace_calls(system: System) -> Result<u32, JsError> {
        Ok(system.plugin::<WorkspaceTheme>()?.calls.get())
    }
}

#[class]
pub(crate) struct CallbackBox {
    callback: Callback<(), i32>,
}

#[class]
impl CallbackBox {
    pub(crate) fn new(callback: Callback<(), i32>) -> Self {
        Self { callback }
    }
    pub(crate) fn call(&self) -> i32 {
        self.callback.call(()).unwrap_or(-1)
    }
    pub(crate) fn escape(&self) {
        KEPT.with(|kept| kept.borrow_mut().push(self.callback.clone()));
    }
    pub(crate) fn call_kept() -> i32 {
        let callback = KEPT.with(|kept| kept.borrow().last().cloned());
        callback
            .and_then(|callback| callback.call(()).ok())
            .unwrap_or(-1)
    }
    pub(crate) fn clear_kept() {
        let callbacks = KEPT.with(|kept| kept.take());
        drop(callbacks);
    }
    pub(crate) fn listen(callback: Callback<(), i32>) -> Stop {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        LISTENING.with(|l| l.borrow_mut().push((id, callback)));
        Stop::new(move || {
            let gone = LISTENING.with(|l| {
                let mut l = l.borrow_mut();
                let i = l.iter().position(|(i, _)| *i == id);
                i.map(|i| l.remove(i))
            });
            drop(gone);
        })
    }
    pub(crate) fn listening() -> u32 {
        LISTENING.with(|l| l.borrow().len() as u32)
    }
}

pub(super) fn registry() -> nsh_core::element::Registry {
    let mut registry = nsh_core::element::Registry::ui_set("test/browser");
    registry
        .class::<CallbackBox>()
        .plugin::<WorkspaceTheme>()
        .module::<PluginProbe>();
    registry
}

pub(super) fn initialize(runtime: &super::Runtime) {
    runtime.registries.borrow_mut().extend(
        nsh_core::runtime::registries()
            .iter()
            .cloned()
            .chain([registry()]),
    );
}

fn theme_source(host: Option<Tokens>) -> nsh_core::element::ThemeTokens {
    let mut core = nsh_core::Core::new(
        nsh_core::ApproxTextEngine,
        Size::new(Points(320.0), Points(480.0)),
    );
    core.set_components(nsh_base::registry_of([]));
    core.set_host_tokens(host);
    core.theme_tokens()
}

/// Start fixtures without creating a full browser engine, as the actual-Wasm bridge tests do.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn nsh_browser_test_plugins_start() {
    let runtime = super::REACTIVE.with(|slot| slot.borrow().as_ref().unwrap().clone());
    let source = theme_source(None);
    *runtime.bindings.tokens.borrow_mut() = source.of(r#"{"cssVars":{}}"#).map(std::sync::Arc::new);
    *runtime.bindings.theme.borrow_mut() = Some(source);
    let system = runtime.bindings.system.borrow().clone();
    let registries = runtime.registries.borrow().clone();
    system.start_plugins(&registries).unwrap();
}

/// Replace the fixture's theme source with one carrying a host override.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn nsh_browser_test_theme_override() {
    let runtime = super::REACTIVE.with(|slot| slot.borrow().as_ref().unwrap().clone());
    let mut host = Tokens::default();
    host.set_color("brand", AdaptiveColor::rgb(0x16a34a, 0x86efac));
    *runtime.bindings.theme.borrow_mut() = Some(theme_source(Some(host)));
}

/// Read the queued theme files without a stand-in implementation of the theme service.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn nsh_browser_test_theme_ops() -> u32 {
    use super::values::Value;
    let runtime = super::REACTIVE.with(|slot| slot.borrow().as_ref().unwrap().clone());
    let themes = runtime
        .bindings
        .ops
        .take()
        .into_iter()
        .filter_map(|op| match op {
            nsh_core::Op::SetProp {
                id: nsh_core::ROOT_ID,
                prop: nsh_core::Prop::UiTheme(json),
            } => Some(Value::string(&json).unwrap()),
            _ => None,
        });
    Value::array(themes).unwrap().into_raw()
}

/// The count remains readable after disposal, without retaining a JS callback or plugin instance.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn nsh_browser_test_plugin_stops() -> u32 {
    PLUGIN_STOPS.with(Cell::get)
}

/// Exercise the Rust-to-browser event conversion with the actual value bridge.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn nsh_browser_test_mouse_events() -> u32 {
    use nsh_core::{EventPayload, MouseButton, MouseEvent, keymap::Modifiers};

    use super::{bindings::Bindings, events, values::Value};
    let host = Bindings::new();
    let buttons = [
        None,
        Some(MouseButton::Left),
        Some(MouseButton::Right),
        Some(MouseButton::Middle),
        Some(MouseButton::Back),
        Some(MouseButton::Forward),
    ];
    let events = buttons.into_iter().map(|button| {
        events::payload(
            &host,
            &EventPayload::Mouse(MouseEvent::new(
                Point::new(Points(3.0), Points(4.0)),
                Point::new(Points(30.0), Points(40.0)),
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                button,
            )),
        )
        .unwrap()
    });
    Value::array(events).unwrap().into_raw()
}

/// Both file-drop variants keep the same public shape as the native JS host.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn nsh_browser_test_file_drop_events() -> u32 {
    use nsh_core::EventPayload;

    use super::{bindings::Bindings, events, values::Value};
    let host = Bindings::new();
    let uris = vec![
        "file:///tmp/hello.txt".to_owned(),
        "file:///tmp/world.txt".to_owned(),
    ];
    let payloads = [
        EventPayload::FileDrop(uris.clone()),
        EventPayload::FileDropAt {
            uris,
            page_position: Point::new(Points(30.0), Points(40.0)),
        },
    ];
    Value::array(payloads.iter().map(|p| events::payload(&host, p).unwrap()))
        .unwrap()
        .into_raw()
}

/// A video's `onLoad` keeps the `{ duration, size }` of the native JS host.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn nsh_browser_test_video_load_event() -> u32 {
    use nsh_core::PayloadValue;

    use super::events;
    events::video_load(&[
        ("duration", PayloadValue::Number(12.5)),
        ("width", PayloadValue::Number(1920.0)),
        ("height", PayloadValue::Number(1080.0)),
    ])
    .unwrap()
    .into_raw()
}
