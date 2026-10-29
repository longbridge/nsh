//! Pure selection algorithms share their production sibling layout.

#[path = "../../src/text/selection/layout.rs"]
mod layout;
#[path = "../../src/text/selection/scroll.rs"]
mod scroll;
#[path = "../../src/text/selection/state.rs"]
#[allow(dead_code)] // The drawing cache is consumed only by the OHOS UI callbacks.
mod state;
