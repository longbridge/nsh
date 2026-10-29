//! Host-side checks for OHOS geometry, motion and native ownership protocols.

extern crate self as ohos_arkui_binding;
use native_ownership::{common, component, event, render, types};
mod native_ownership;

#[path = "../../src/accessibility/value.rs"]
mod accessibility_value;
#[path = "../../src/accessibility/action_descriptions.rs"]
mod action_descriptions;
#[path = "../../src/render/composition/blend.rs"]
mod composite_blend;
#[path = "../../src/render/composition/geometry.rs"]
#[allow(dead_code)]
mod composite_geometry;
#[path = "../../src/app/features.rs"]
#[allow(unreachable_pub)]
mod features;
#[path = "../../src/render/geometry.rs"]
mod geometry;
#[path = "../../src/media/request.rs"]
mod image_request;
#[path = "../../src/navigation/motion.rs"]
mod navigation_motion;
#[path = "../../src/scroll/motion.rs"]
mod scroll_motion;
mod selection;
#[cfg(unix)]
#[path = "../../src/media/video/source.rs"]
mod video_source;

#[path = "../../src/app/dev/presentation.rs"]
mod dev_presentation;

#[path = "../../src/input/editor_state.rs"]
#[allow(dead_code)]
mod editor_state;

#[path = "../../src/services/executor.rs"]
mod executor;

#[path = "../../src/media/video/controls.rs"]
mod video_controls;

#[path = "../../src/render/shadow_geometry.rs"]
mod shadow_geometry;
