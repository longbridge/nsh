//! Compiled enhancements intersected with the real system's capabilities.

#[cfg(target_env = "ohos")]
use std::sync::atomic::{AtomicI32, Ordering};

#[cfg(target_env = "ohos")]
static RUNTIME_API_VERSION: AtomicI32 = AtomicI32::new(0);

#[cfg(target_env = "ohos")]
pub(crate) fn set_runtime_api_version(version: i32) {
    RUNTIME_API_VERSION.store(version.max(0), Ordering::Relaxed);
}

/// Native Shell's minimum supported OHOS API level.
pub(crate) const MIN_API: u32 = if cfg!(feature = "api-26") {
    26
} else if cfg!(feature = "api-23") {
    23
} else {
    20
};

/// Capabilities available to the embedding host on this runtime.
#[cfg_attr(target_env = "ohos", napi_derive_ohos::napi(object))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NshCapabilities {
    /// Actual system API level, independent of surface environment overrides.
    pub api_level: u32,
    /// Native Shell's minimum supported system API level.
    pub minimum_api_level: u32,
    /// API-23 support was included in this library.
    pub compiled_api23: bool,
    /// API-26 support was included in this library.
    pub compiled_api26: bool,
    /// API-23 enhancements may be used on this runtime.
    pub api23: bool,
    /// API-26 enhancements may be used on this runtime.
    pub api26: bool,
}

impl NshCapabilities {
    pub(crate) fn for_api(api_level: u32) -> Self {
        Self::with_compiled(
            api_level,
            cfg!(feature = "api-23"),
            cfg!(feature = "api-26"),
        )
    }

    fn with_compiled(api_level: u32, api23: bool, api26: bool) -> Self {
        Self {
            api_level,
            minimum_api_level: MIN_API,
            compiled_api23: api23,
            compiled_api26: api26,
            api23: api23 && api_level >= 23,
            api26: api26 && api_level >= 26,
        }
    }
}

/// Returns compiled enhancements intersected with the running system API level.
#[cfg(target_env = "ohos")]
#[napi_derive_ohos::napi]
pub fn get_capabilities() -> NshCapabilities {
    let mut version = RUNTIME_API_VERSION.load(Ordering::Relaxed);
    if version <= 0 {
        version = ohos_device_binding::sdk_api_version() as i32;
        if version > 0 {
            RUNTIME_API_VERSION.store(version, Ordering::Relaxed);
        }
    }
    NshCapabilities::for_api(version.max(0) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enhancements_require_both_build_and_runtime_support() {
        assert_eq!(NshCapabilities::for_api(20).minimum_api_level, MIN_API);
        for api in [20, 22, 23, 25, 26, 27] {
            for (compiled23, compiled26) in [(false, false), (true, false), (true, true)] {
                let c = NshCapabilities::with_compiled(api, compiled23, compiled26);
                assert_eq!(c.api23, compiled23 && api >= 23);
                assert_eq!(c.api26, compiled26 && api >= 26);
                assert_eq!(c.minimum_api_level, MIN_API);
            }
        }
    }
}
