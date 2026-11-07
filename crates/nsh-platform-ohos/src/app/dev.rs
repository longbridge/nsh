//! A development connection belongs to a retained view, independently of its
//! engine generations. The shared protocol handles reconnects and version negotiation.
use std::{
    io::Read,
    sync::{Arc, LazyLock, Mutex, Weak},
};

use napi_ohos::{Error, Result};
use nsh_dev::{
    DevClient, HostInfo,
    protocol::{BundleFormat, HostPlatform},
};
use nsh_engine::{Bundle, Policy};

use crate::app::{
    runtime::App,
    surface::{Message, Queue},
};

mod presentation;

pub(crate) struct Session {
    _client: Arc<DevClient>,
    pub(crate) latest: Option<((u64, u64), Bundle)>,
    pub(crate) running: Option<(u64, u64)>,
    pub(crate) origin: String,
    token: u64,
    queue: Weak<Queue>,
}
impl Session {
    pub(crate) fn connect(address: String, queue: Weak<Queue>, token: u64) -> Result<Self> {
        let url = url::Url::parse(&address).map_err(error)?;
        if !matches!(url.scheme(), "ws" | "wss")
            || url.host_str().is_none()
            || url.cannot_be_a_base()
        {
            return Err(error("development server must be a ws:// or wss:// URL"));
        }
        let mut origin = url.clone();
        origin
            .set_scheme(if url.scheme() == "wss" {
                "https"
            } else {
                "http"
            })
            .map_err(|_| error("invalid server URL"))?;
        origin.set_path("/");
        origin.set_query(None);
        origin.set_fragment(None);
        let callback_queue = queue.clone();
        let client = Arc::new(
            DevClient::start(
                address,
                HostInfo {
                    device: "OHOS Native Shell".into(),
                    platform: HostPlatform::Ohos,
                    bytecode_version: nsh_engine::BYTECODE_VERSION.into(),
                    ui_sets: vec!["nsh/base".into()],
                    tests: false,
                },
                move |event| {
                    if let Some(queue) = callback_queue.upgrade() {
                        queue.call(
                            Message::Dev { token, event },
                            napi_ohos::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
                        );
                    }
                },
            )
            .map_err(error)?,
        );
        CLIENTS.lock().unwrap().push(Arc::downgrade(&client));
        Ok(Self {
            _client: client,
            latest: None,
            running: None,
            origin: origin.to_string(),
            queue,
            token,
        })
    }
    pub(crate) fn bundle(
        format: BundleFormat,
        bytes: Vec<u8>,
        map: Option<String>,
        capabilities: Option<String>,
        app_id: Option<String>,
    ) -> Result<Bundle> {
        let mut bundle = match format {
            // SAFETY: DevClient negotiated this engine's bytecode version with
            // the server the host explicitly selected for this development view.
            BundleFormat::Bytecode => unsafe { Bundle::bytecode(bytes) },
            BundleFormat::Source => Bundle::source(String::from_utf8(bytes).map_err(error)?),
        }
        .with_policy(Policy::development())
        .with_app_id(app_id.as_deref());
        bundle.source_map = map.map(Into::into);
        bundle
            .with_capabilities_json(capabilities.as_deref())
            .map_err(error)
    }
    pub(crate) fn app(&self, mut bundle: Bundle) -> App {
        bundle.base_url = Some(self.origin.clone().into());
        let mut app = App::new(
            bundle,
            Arc::new(Assets {
                origin: self.origin.clone(),
            }),
        );
        app.reload = Some((self.token, self.queue.clone()));
        app
    }
}
struct Assets {
    origin: String,
}
impl nsh_engine::Assets for Assets {
    fn read(&self, uri: &str) -> std::result::Result<Vec<u8>, String> {
        let name = uri.strip_prefix("asset:/").ok_or("expected asset:/ URI")?;
        crate::app::session::resource_name(name).map_err(|e| e.to_string())?;
        let base =
            url::Url::parse(&format!("{}assets/", self.origin)).map_err(|e| e.to_string())?;
        let mut target = base.clone();
        target
            .path_segments_mut()
            .map_err(|_| "invalid asset URL")?
            .pop_if_empty()
            .extend(name.split('/'));
        let mut response = ureq::get(target.as_str())
            .call()
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take((64 << 20) + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 64 << 20 {
            return Err("asset exceeds 64 MiB".into());
        }
        Ok(bytes)
    }
}
type Clients = Vec<Weak<DevClient>>;
static CLIENTS: LazyLock<Mutex<Clients>> = LazyLock::new(Mutex::default);
struct Logger;
impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.target().starts_with("nsh") || m.target() == "js" || m.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        let clients = {
            let mut clients = CLIENTS.lock().unwrap();
            clients.retain(|c| c.strong_count() > 0);
            clients.iter().filter_map(Weak::upgrade).collect::<Vec<_>>()
        };
        for client in clients {
            client.log(record.level().into(), record.target(), &message);
        }
        let line = format!("{}: {message}", record.target());
        match record.level() {
            log::Level::Error => ohos_hilog_binding::error(line),
            log::Level::Warn => ohos_hilog_binding::warn(line),
            log::Level::Info => ohos_hilog_binding::info(line),
            _ => ohos_hilog_binding::debug(line),
        }
    }
    fn flush(&self) {}
}
pub(crate) fn init_logging() {
    ohos_hilog_binding::set_global_options(ohos_hilog_binding::LogOptions {
        domain: 0,
        tag: "nsh",
    });
    static LOGGER: Logger = Logger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}
fn error(e: impl std::fmt::Display) -> Error {
    Error::from_reason(e.to_string())
}

pub(crate) struct Overlay {
    parent: std::rc::Weak<std::cell::RefCell<ohos_arkui_binding::common::node::ArkUINode>>,
    node: crate::render::node::NativeNode,
    title: crate::render::node::NativeNode,
    actions: crate::render::node::NativeNode,
    copy: crate::render::node::NativeNode,
    dismiss: crate::render::node::NativeNode,
    status: crate::render::node::NativeNode,
    scroll: crate::render::node::NativeNode,
    text: crate::render::node::NativeNode,
    host: Arc<crate::app::session::Services>,
    queue: Weak<Queue>,
    presentation: std::cell::RefCell<presentation::Presentation>,
}
impl Overlay {
    pub(crate) fn new(
        parent: &crate::render::node::NativeNode,
        host: Arc<crate::app::session::Services>,
        queue: Weak<Queue>,
    ) -> Result<std::rc::Rc<Self>> {
        use ohos_arkui_binding::{
            component::{
                attribute::{ArkUICommonAttribute, ArkUIEvent},
                built_in_component::{Button, Column, Row, Scroll, Text},
            },
            types::{
                advanced::LengthMetricUnit, attribute::ArkUINodeAttributeType as A,
                view::HitTestMode,
            },
        };
        let node = Column::new().map_err(error)?.into_node();
        let build = || -> crate::render::node::Result<std::rc::Rc<Self>> {
            let title = Text::new()?.into_node();
            node.borrow_mut().add_child(title.clone())?;
            let actions = Row::new()?.into_node();
            node.borrow_mut().add_child(actions.clone())?;
            let copy = Button::new()?.into_node();
            actions.borrow_mut().add_child(copy.clone())?;
            let dismiss = Button::new()?.into_node();
            actions.borrow_mut().add_child(dismiss.clone())?;
            let status = Text::new()?.into_node();
            node.borrow_mut().add_child(status.clone())?;
            let scroll = Scroll::new()?.into_node();
            node.borrow_mut().add_child(scroll.clone())?;
            let text = Text::new()?.into_node();
            scroll.borrow_mut().add_child(text.clone())?;
            for (view, id) in [
                (&node, "nsh-dev-overlay"),
                (&title, "nsh-dev-overlay-title"),
                (&actions, "nsh-dev-overlay-actions"),
                (&copy, "nsh-dev-overlay-copy"),
                (&dismiss, "nsh-dev-overlay-dismiss"),
                (&status, "nsh-dev-overlay-status"),
                (&scroll, "nsh-dev-overlay-scroll"),
                (&text, "nsh-dev-overlay-message"),
            ] {
                let view = view.borrow();
                view.set_length_metric_unit(LengthMetricUnit::Vp)?;
                view.id(id)?;
            }
            node.borrow().background_color(0xff611111)?;
            node.borrow().set_clip(true)?;
            node.borrow().z_index(i32::MAX)?;
            // Default tests descendants while blocking siblings behind the overlay.
            node.borrow()
                .set_i32_attribute(A::HitTestBehavior, HitTestMode::Default.into())?;
            node.borrow_mut().on_touch_event(|event| {
                crate::render::node::callback(|| {
                    if let Some(input) = event.input_event() {
                        input
                            .pointer_set_stop_propagation(true)
                            .map_err(|e| error(e.to_string()))?;
                    }
                    Ok(())
                });
            });
            title
                .borrow()
                .set_string_attribute(A::TextContent, "Development error")?;
            for view in [&title, &status, &text] {
                view.borrow().set_u32_attribute(A::FontColor, 0xffffffff)?;
                view.borrow().set_f32_attribute(A::FontSize, 14.0)?;
                view.borrow()
                    .set_attribute(A::Padding, vec![4.0_f32, 8.0, 4.0, 8.0].into())?;
            }
            title.borrow().set_f32_attribute(A::FontSize, 16.0)?;
            status.borrow().set_f32_attribute(A::FontSize, 12.0)?;
            status.borrow().set_i32_attribute(A::TextMaxLines, 1)?;
            text.borrow()
                .set_string_attribute(A::FontFamily, "monospace")?;
            for (button, label) in [(&copy, "Copy"), (&dismiss, "Dismiss")] {
                button
                    .borrow()
                    .set_string_attribute(A::ButtonLabel, label)?;
                button
                    .borrow()
                    .set_u32_attribute(A::FontColor, 0xffffffff)?;
                button.borrow().set_f32_attribute(A::FontSize, 14.0)?;
                button.borrow().background_color(0xff8b3333)?;
            }
            scroll.borrow().set_clip(true)?;
            scroll
                .borrow()
                .set_i32_attribute(A::ScrollScrollDirection, 0)?;
            let overlay = std::rc::Rc::new(Self {
                parent: std::rc::Rc::downgrade(parent),
                node: node.clone(),
                title,
                actions,
                copy,
                dismiss,
                status,
                scroll,
                text,
                host,
                queue,
                presentation: std::cell::RefCell::default(),
            });
            let weak = std::rc::Rc::downgrade(&overlay);
            overlay.copy.borrow_mut().on_click(move || {
                if let Some(overlay) = weak.upgrade() {
                    overlay.copy();
                }
            });
            let weak = std::rc::Rc::downgrade(&overlay);
            overlay.dismiss.borrow_mut().on_click(move || {
                if let Some(overlay) = weak.upgrade() {
                    overlay.hide();
                }
            });
            overlay.hide();
            // Engine roots always mount at index zero; this remains the last child.
            parent.borrow_mut().add_child(node.clone())?;
            Ok(overlay)
        };
        match build() {
            Ok(overlay) => Ok(overlay),
            Err(failure) => {
                crate::render::node::callback(|| node.borrow_mut().dispose().map_err(Into::into));
                Err(error(failure))
            }
        }
    }
    pub(crate) fn resize(&self, size: crate::render::geometry::Size) -> Result<()> {
        self.layout(size).map_err(error)
    }
    fn layout(&self, size: crate::render::geometry::Size) -> crate::render::node::Result<()> {
        use ohos_arkui_binding::component::attribute::ArkUICommonAttribute;
        let width = size.width.max(0.0);
        let height = size.height.max(0.0);
        let title = 28.0_f32.min(height * 0.2);
        let actions = 44.0_f32.min(height * 0.32);
        let status = 24.0_f32.min(height * 0.18);
        for (node, w, h) in [
            (&self.node, width, height),
            (&self.title, width, title),
            (&self.actions, width, actions),
            (&self.copy, width * 0.5, actions),
            (&self.dismiss, width * 0.5, actions),
            (&self.status, width, status),
            (
                &self.scroll,
                width,
                (height - title - actions - status).max(0.0),
            ),
        ] {
            node.borrow().width(w)?;
            node.borrow().height(h)?;
        }
        // An unconstrained text height lets native Scroll measure every wrapped line.
        self.text.borrow().width(width)?;
        Ok(())
    }
    pub(crate) fn show(&self, message: &str) {
        use ohos_arkui_binding::{
            component::attribute::ArkUICommonAttribute,
            types::attribute::ArkUINodeAttributeType as A,
        };
        if !self.presentation.borrow_mut().show(message) {
            return;
        }
        crate::render::node::callback(|| {
            self.text
                .borrow()
                .set_string_attribute(A::TextContent, message.replace('\0', "\u{fffd}"))?;
            self.status
                .borrow()
                .set_string_attribute(A::TextContent, "")?;
            self.copy.borrow().enabled(true)?;
            self.node.borrow().set_i32_attribute(A::Visibility, 0)?;
            self.scroll
                .borrow()
                .set_attribute(A::ScrollOffset, vec![0.0_f32, 0.0].into())?;
            Ok(())
        });
    }
    pub(crate) fn hide(&self) {
        use ohos_arkui_binding::{
            component::attribute::ArkUICommonAttribute,
            types::attribute::ArkUINodeAttributeType as A,
        };
        self.presentation.borrow_mut().hide();
        crate::render::node::callback(|| {
            if self.node.borrow().is_owned() {
                self.node.borrow().set_i32_attribute(A::Visibility, 1)?;
            }
            Ok(())
        });
    }
    fn copy(&self) {
        let Some((revision, text)) = self.presentation.borrow_mut().begin_copy() else {
            return;
        };
        self.feedback("Copying…", false);
        if let Err(error) = self
            .host
            .copy_development_error(self.queue.clone(), revision, text)
        {
            self.copied(revision, Err(error.to_string()));
        }
    }
    pub(crate) fn copied(&self, revision: u64, result: std::result::Result<(), String>) {
        let result = self.presentation.borrow_mut().finish_copy(revision, result);
        match result {
            Some(Ok(())) => self.feedback("Copied", true),
            Some(Err(error)) => self.feedback(&format!("Copy failed: {error}"), true),
            None => {}
        }
    }
    fn feedback(&self, message: &str, enabled: bool) {
        use ohos_arkui_binding::{
            component::attribute::ArkUICommonAttribute,
            types::attribute::ArkUINodeAttributeType as A,
        };
        // Clipboard failures are presentation feedback, not new surface errors.
        crate::render::node::callback(|| {
            self.status
                .borrow()
                .set_string_attribute(A::TextContent, message.replace('\0', "\u{fffd}"))?;
            self.copy.borrow().enabled(enabled)?;
            Ok(())
        });
    }
    pub(crate) fn close(&self) -> Result<()> {
        use ohos_arkui_binding::component::attribute::ArkUICommonAttribute;
        if self.presentation.borrow().closed() {
            return Ok(());
        }
        self.hide();
        let close = || -> crate::render::node::Result<()> {
            if let Some(parent) = self.parent.upgrade() {
                let index = parent
                    .borrow()
                    .children()
                    .iter()
                    .position(|child| std::rc::Rc::ptr_eq(child, &self.node));
                if let Some(index) = index {
                    parent.borrow_mut().remove_child(index)?;
                }
            }
            // Overlay fields retain every child separately from its tree edge.
            // Detach first so disposal sees only the corresponding field owner;
            // keeping the fields also makes partial native failures retryable.
            for (parent, child) in [
                (&self.scroll, &self.text),
                (&self.actions, &self.copy),
                (&self.actions, &self.dismiss),
                (&self.node, &self.title),
                (&self.node, &self.actions),
                (&self.node, &self.status),
                (&self.node, &self.scroll),
            ] {
                if child.borrow().is_owned() {
                    child.borrow().remove_from_parent()?;
                    parent
                        .borrow_mut()
                        .children_mut()
                        .retain(|node| !std::rc::Rc::ptr_eq(node, child));
                }
            }
            for node in [
                &self.text,
                &self.scroll,
                &self.copy,
                &self.dismiss,
                &self.actions,
                &self.title,
                &self.status,
                &self.node,
            ] {
                if node.borrow().is_owned() {
                    node.borrow_mut().dispose()?;
                }
            }
            Ok(())
        };
        close().map_err(error)?;
        self.presentation.borrow_mut().close();
        Ok(())
    }
}
