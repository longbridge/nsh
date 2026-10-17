//! Platform-independent core of nsh.
//!
//! The core receives [`Op`]s from the JS renderer, maintains the shadow tree,
//! converts styles to [`taffy`] layout, merges inline text into
//! [`AttributedText`], runs layout and finally emits a [`MountBatch`] of
//! [`MountItem`]s that a platform backend applies to native views.
//!
//! Everything here is single-threaded and allocation-conscious: the core runs
//! on the engine thread, between the JS host and the platform backend.

pub mod access;
pub mod canvas;
mod core;
mod css;
mod css_named;
pub mod desktop;
mod effects;
mod fenwick;
mod geometry;
mod grid;
pub mod js;
pub mod keymap;
mod mount;
mod ops;
pub mod platform;
mod props;
pub mod reactive;
pub mod runtime;
pub mod shape;
mod style;
pub mod tailwind;
mod text;
pub mod time;
mod transform;
pub mod ui;

// The derive macros name `::nsh_core`, also in this crate's own tests.
extern crate self as nsh_core;

pub use nsh_animation;
/// The values registered native components take and emit (props, events,
/// command arguments): JSON.
pub use serde_json::Value as JsonValue;

pub use crate::{
    core::{ApplyError, Core, ListBind, Measurement},
    effects::{
        BlendMode, BoxShadow, ColorStop, Cursor, Effects, Filter, Gradient, GradientKind,
        GradientSize, KeyframeAnimation, Keyframes, Outline, Transition, TransitionProps,
        blur_radius, compose_color_matrices,
    },
    geometry::{Rect, Size},
    mount::{
        ImageOptions, InputConfig, ModalConfig, MountBatch, MountItem, NativeProps, ScreenConfig,
        ScrollBehavior, StickyCell, VideoProps, ViewKind, WebProps,
    },
    ops::{
        AutoCapitalize, AutoFill, CellSlot, Command, Dialog, DialogButton, DialogButtonStyle,
        EventKind, EventMask, EventPayload, HeaderSlot, ImageProp, ImageSource, KeyboardAppearance,
        KeyboardDismissMode, KeyboardTaps, KeyboardType, ListContentStyle, ModalAnimation,
        ModalPresentation, ModalProp, NativeProp, NodeId, Op, PayloadValue, Prop, ROOT_ID,
        ResizeMode, ReturnKeyType, ScreenAnimation, ScreenProp, ScrollProp, SnapAlignment,
        StatusBarConfig, StatusBarSetting, StatusBarStyle, SubmitBehavior, Tag, TouchEvent,
        TouchPhase, TouchPoint, UNDERLAY_OPACITY, Viewability, WebSource,
    },
    props::{
        Accessibility, AccessibilityActionDef, AccessibilityProp, AccessibilityRole,
        AccessibilityState, AccessibilityValue, Checked, EdgeInsets, GestureValues,
        LayoutAnimation, LayoutCurve, LayoutProperty, LayoutTransition, LiveRegion, PanActivation,
        PointerEvents, Ripple, ViewProps,
    },
    style::{
        BorderStyle, Borders, Color, CornerRadii, DecorationStyle, EllipsizeMode, FontFamily,
        FontVariant, OriginLen, PatchValue, Shadow, Style, StyleDecl, StyleError, StyleKey,
        StylePatch, StyleValue, TextAlign, TextDecoration, TextShadow, TextStyle, TextTransform,
        TransformOrigin, ViewStyle,
    },
    text::{
        ATTACHMENT_CHAR, ApproxTextEngine, AttributedText, ResolvedTextStyle, TextAttachment,
        TextEngine, TextLayout, TextLine, TextLink, TextSpan,
    },
    transform::{Decomposed, MAX_TRANSFORMS, Matrix, Transform, TransformKind, TransformOp},
};
