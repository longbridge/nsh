//! Validated host policy and resources, shared by every load of one surface.
use std::sync::Arc;

use napi_derive_ohos::napi;

use crate::{app::environment::NshEnvironment, services::assets::AssetDirectory};

/// Per-surface resources and VM limits selected by the embedding host.
#[napi(object)]
#[derive(Clone, Default, PartialEq)]
pub struct NshOptions {
    /// Directory containing the build's `assets/` contents.
    pub asset_directory: Option<String>,
    /// System observations available before the app starts.
    pub environment: Option<NshEnvironment>,
    /// Interpreter stack ceiling in bytes (64 KiB through 4 MiB).
    pub stack_bytes: Option<u32>,
    /// Maximum queued operation values.
    pub max_op_values: Option<u32>,
    /// Maximum operations recorded in one canvas drawing.
    pub max_canvas_ops: Option<u32>,
    /// Host capability grants, using the `nsh.json` capabilities JSON schema.
    pub capabilities: Option<String>,
    /// QuickJS heap limit in bytes (at least 1 MiB).
    pub memory_bytes: Option<u32>,
    /// Maximum number of shadow nodes (at least one).
    pub max_nodes: Option<u32>,
    /// Per-call JS time limit in milliseconds (at least one).
    pub call_budget_ms: Option<u32>,
    /// Initial evaluation time limit in milliseconds (at least one).
    pub load_budget_ms: Option<u32>,
}

pub(crate) struct Config {
    pub(crate) host: Arc<crate::app::session::Services>,
    pub(crate) assets: Arc<dyn nsh_engine::Assets>,
    pub(crate) policy: nsh_engine::Policy,
    pub(crate) limits: nsh_engine::Limits,
    pub(crate) environment: NshEnvironment,
    pub(crate) channel: nsh_engine::HostChannel,
    pub(crate) host_tokens: Option<nsh_engine::Tokens>,
}
impl Config {
    pub(crate) fn new(
        host: Arc<crate::app::session::Services>,
        options: NshOptions,
    ) -> napi_ohos::Result<Self> {
        let directory = options
            .asset_directory
            .as_deref()
            .map(AssetDirectory::new)
            .transpose()
            .map_err(napi_ohos::Error::from_reason)?;
        let assets: Arc<dyn nsh_engine::Assets> = Arc::new(
            crate::services::assets::HostAssets::new(host.clone(), directory),
        );
        let mut policy = nsh_engine::Policy::default();
        if let Some(grants) = &options.capabilities {
            policy.granted = Some(
                nsh_engine::Capabilities::from_json(
                    &serde_json::from_str(grants)
                        .map_err(|e| napi_ohos::Error::from_reason(e.to_string()))?,
                )
                .map_err(napi_ohos::Error::from_reason)?,
            );
        }
        let mut limits = nsh_engine::Limits::default();
        for (name, value) in [
            ("maxNodes", options.max_nodes),
            ("callBudgetMs", options.call_budget_ms),
            ("loadBudgetMs", options.load_budget_ms),
        ] {
            if value == Some(0) {
                return Err(napi_ohos::Error::from_reason(format!(
                    "{name} must be positive"
                )));
            }
        }
        if let Some(value) = options.memory_bytes {
            if value < 1 << 20 {
                return Err(napi_ohos::Error::from_reason(
                    "memoryBytes must be at least 1 MiB",
                ));
            }
            limits.memory_bytes = value as usize;
        }
        if let Some(value) = options.max_nodes {
            limits.max_nodes = value as usize;
        }
        if let Some(value) = options.call_budget_ms {
            limits.call_budget = std::time::Duration::from_millis(value as u64);
        }
        if let Some(value) = options.load_budget_ms {
            limits.load_budget = std::time::Duration::from_millis(value as u64);
        }
        if let Some(value) = options.stack_bytes {
            if !(64 << 10..=4 << 20).contains(&value) {
                return Err(napi_ohos::Error::from_reason(
                    "stackBytes must be 64 KiB through 4 MiB",
                ));
            }
            limits.stack_bytes = value as usize;
        }
        if let Some(value) = options.max_op_values {
            if value == 0 {
                return Err(napi_ohos::Error::from_reason(
                    "maxOpValues must be positive",
                ));
            }
            limits.max_op_values = value as usize;
        }
        if let Some(value) = options.max_canvas_ops {
            if value == 0 {
                return Err(napi_ohos::Error::from_reason(
                    "maxCanvasOps must be positive",
                ));
            }
            limits.max_canvas_ops = value as usize;
        }

        let environment = options.environment.unwrap_or_default();
        environment
            .clone()
            .apply(nsh_engine::Environment::default())?;
        Ok(Self {
            host,
            assets,
            policy,
            limits,
            environment,
            channel: nsh_engine::HostChannel::new(),
            host_tokens: None,
        })
    }
}
