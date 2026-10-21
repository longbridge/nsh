//! The native animation graph.
//!
//! JS declares values, derived nodes (interpolations, math) and animations
//! once; the platform evaluates them every display frame on the UI thread,
//! so animations, gestures and scroll-linked effects never wait for JS.
//!
//! It is platform-free: the platform feeds it [`AnimationOp`]s, frame
//! timestamps, gesture and scroll values, and reads back evaluated values.

mod driver;
mod graph;
mod ops;

pub use self::{
    driver::{MAX_SAMPLES, sample_driver, sample_easing},
    graph::{AnimationValue, Finished, Graph},
    ops::{
        AnimationId, AnimationOp, AnimationProp, Composite, Curve, Driver, EaseMode, Easing,
        Extrapolate, MathOp, NodeId, Operand, Output,
    },
};
