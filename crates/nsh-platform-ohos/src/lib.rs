//! OHOS native backend: independent ContentSlot views in a standard UIAbility host.
//! Each rendering surface owns one engine and ArkUI tree.
//!
//! The engine lays out text off the UI thread and transfers owned native drawing
//! commands with selection geometry. Native views
//! consume one mount batch per wake, with no per-node ArkTS calls.
#![cfg(target_env = "ohos")]

mod accessibility;
mod app;
mod input;
mod interop;
mod media;
mod navigation;
mod render;
mod scroll;
mod services;
mod text;

pub use app::{
    NshCapabilities, NshEnvironment, NshOptions, NshPackage, NshPackageInfo, NshSurface, NshTrust,
    get_capabilities, init_logging,
};
pub use interop::NshNativeProperty;
