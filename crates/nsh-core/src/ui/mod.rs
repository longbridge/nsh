//! Components written in Rust: the UI sets' components (`nsh-ui-mobile`),
//! run by the core on the engine thread.
//!
//! A [`Component`] renders a tree of elements ([`view`], [`text`],
//! [`image`], other components) styled with typed methods ([`Styled`]:
//! `.px_4()`, `.rounded_xl()`, `.bg(theme.primary)`) that lower into the
//! same rules as Tailwind classes. The core expands the tree into nodes of
//! its own, diffs it against the previous render and lays it out in the same
//! pass as everything else, so a component sizes like any view, with no
//! round trip. Events on its elements go to the component first
//! ([`RenderCx::listener`]); it decides what reaches JS ([`Context::emit`])
//! and calls [`Context::notify`] to render again. State lives in the
//! component (`Self`), set up from the first props ([`Component::setup`]),
//! changed by listeners, timers and async tasks ([`Context::spawn`]); a render
//! subscribes to what it reads of the environment ([`RenderCx::window`]).
//!
//! Props are a struct (`#[derive(Props)]`), events an enum
//! (`#[derive(Event)]`); their JS face (`nsh-js`) and TypeScript
//! declarations ([`declarations`]) are made from them. In JS a component is one host node:
//! the root of its expansion. The app's `class` is appended to the root's
//! styles and its `style` stays the root's inline style. Colors come from
//! the [`Theme`] (gpui-kit's fields), with a light and a dark value each.
//!
//! Node ids: JS allocates even ids, the core odd ones for the nodes it
//! expands, so both stay dense. How the core runs components:
//! `docs/dev/components.md`.

pub(crate) mod animated;
mod asset;
mod back;
mod component;
mod element;
mod element_ref;
pub(crate) mod env;
mod events;
pub(crate) mod host;
mod module;
pub mod palette;
pub(crate) mod responder;
mod scrollable;
pub(crate) mod services;
pub(crate) mod status_bar;
mod style;
pub(crate) mod task;
mod theme;
mod value;
pub(crate) mod virtual_list;

/// Whether this build carries the text of declarations: the documentation
/// and TypeScript of the specs, which only make `.nsh/types`. The
/// WebAssembly runtime, which pages download and which writes no
/// declarations, goes without ([`decl!`](crate::decl)); the specs keep
/// their shape (names, kinds, whether a method returns).
pub const DECLARATIONS: bool = !cfg!(target_arch = "wasm32");

/// Declaration text: `$text` where the build carries it
/// ([`DECLARATIONS`]), `""` (a list: `&[]`) where it does not. In a
/// constant the text is then not in the binary at all.
#[macro_export]
macro_rules! decl {
    ([$($text:expr),* $(,)?]) => {
        if $crate::ui::DECLARATIONS { &[$($text),*] as &[&'static str] } else { &[] }
    };
    ($text:expr) => {
        if $crate::ui::DECLARATIONS { $text } else { "" }
    };
}

pub use nsh_animation::{
    Curve, EaseMode, Easing, Extrapolate, Output as AnimationOutput, sample_easing,
};
pub use nsh_macros::{
    Event, JsObject, PropEnum, Props, asset, assets, methods, module, native_module,
};

pub use self::{
    animated::{
        Animated, Animation, AnimationEnd, AnimationOperand, Interpolation, NativeNode,
        NativeValue, Spring, Timing, sample_decay, sample_decay_within, sample_spring,
    },
    asset::{AssetFile, AssetKind, FileAsset, FontAsset, ImageAsset, ImageSrc},
    back::BackSubscription,
    component::{
        Component, ComponentError, ComponentSpec, Event, EventSpec, Method, MethodSpec, NoEvent,
        Param, PropKind, PropSpec, Props, Registry, Root, declarations,
    },
    element::{
        Accessible, AnyElement, Attrs, Child, Children, ComponentElement, Context, ElNode, Element,
        FluentBuilder, Image, Interactive, IntoElement, Items, ListConfig, ListElement, Listener,
        ParentElement, Position, RenderCx, Slot, SlotElement, Style, Text, View, image, text, view,
    },
    element_ref::{ComponentRef, ElementRef},
    env::Environment,
    events::{
        GestureEvent, ImageProgress, NativeEvent, PanEvent, ScrollEvent, ScrollOffset, Selection,
        VideoInfo, VideoProgress, WebNavigation,
    },
    host::{
        CanvasArg, CanvasPaint, CanvasView, Input, InputAccessoryView, Modal, NativeView,
        Orientation, Painter, Screen, Scroll, Stack, Video, WebView, canvas, input,
        input_accessory_view, modal, native, screen, scroll, stack, video, web_view, web_view_html,
    },
    module::{
        Function, FunctionCall, FunctionSpec, Module, ModuleCx, ModuleFuture, ModuleParam,
        ModuleSpec,
    },
    responder::{ClaimRequest, PanGesture, ResponderEvent, TouchClaim},
    scrollable::Scrollable,
    services::{Prefetch, ToastDuration},
    status_bar::StatusBarEntry,
    style::{
        AdaptiveColor, BorderCurve, FontWeight, Length, StyleRefinement, Styled, WritingDirection,
        px, relative, rgb, rgba,
    },
    task::{Answer, AsyncCx, DialogAnswer, ImageSize, Task, Updated, WeakComponent},
    theme::{Theme, ThemeColors, ThemeError},
    value::{JsValue, NodeRef, PropError, SharedString, TsType, TsTypes, ts_of},
    virtual_list::{ListLayout, ViewableItems, VirtualList, virtual_list},
};
pub(crate) use self::{
    back::BackListener,
    component::{AnyInstance, caught},
    element::{ElKind, ErasedCx, Globals, PlaceFn, Provided, RenderReads, RenderScope},
    task::{Action, AnswerSlot, ClipboardRead, Executor},
};
pub use crate::{
    AccessibilityRole, AccessibilityState, AutoCapitalize, AutoFill, BlendMode, BoxShadow, Dialog,
    DialogButton, DialogButtonStyle, EdgeInsets, EllipsizeMode, EventKind, EventPayload,
    KeyboardAppearance, KeyboardDismissMode, KeyboardTaps, KeyboardType, KeyframeAnimation,
    Keyframes, LayoutAnimation, LayoutCurve, LayoutProperty, LayoutTransition, Measurement,
    ModalAnimation, ModalPresentation, PanActivation, PointerEvents, ResizeMode, ReturnKeyType,
    ScreenAnimation, SnapAlignment, StatusBarConfig, StatusBarSetting, StatusBarStyle,
    SubmitBehavior, TouchEvent, TransformKind, TransformOrigin, Viewability,
    platform::{Appearance, Idiom, Locale, Os},
    style::{Color, OriginLen},
};

/// Everything a component's module needs: `use nsh_core::ui::prelude::*;`.
pub mod prelude {
    pub use super::{
        AccessibilityRole,
        AccessibilityState,
        Accessible,
        AdaptiveColor,
        Animated,
        Animation,
        AnimationOutput,
        AnyElement,
        Appearance,
        AsyncCx,
        // Host elements and their settings.
        AutoCapitalize,
        BackSubscription,
        BlendMode,
        BorderCurve,
        BoxShadow,
        CanvasView,
        Child,
        Children,
        ClaimRequest,
        Color,
        Component,
        ComponentElement,
        ComponentError,
        ComponentRef,
        Context,
        Curve,
        DialogAnswer,
        EaseMode,
        // Native animation values.
        Easing,
        EdgeInsets,
        Element,
        ElementRef,
        Environment,
        Event,
        EventKind,
        EventPayload,
        Extrapolate,
        FluentBuilder,
        FontAsset,
        FontWeight,
        // Typed events.
        GestureEvent,
        ImageAsset,
        Input,
        InputAccessoryView,
        Interactive,
        Interpolation,
        IntoElement,
        Items,
        JsObject,
        JsValue,
        // Scrolling and lists.
        KeyboardAppearance,
        KeyboardDismissMode,
        KeyboardTaps,
        KeyboardType,
        KeyframeAnimation,
        Keyframes,
        Length,
        ListLayout,
        Listener,
        Measurement,
        Method,
        Modal,
        ModalAnimation,
        ModalPresentation,
        ModuleCx,
        NativeEvent,
        NativeNode,
        NativeValue,
        NativeView,
        NoEvent,
        Orientation,
        Painter,
        PanActivation,
        PanEvent,
        PanGesture,
        ParentElement,
        PointerEvents,
        Position,
        PropEnum,
        Props,
        Registry,
        RenderCx,
        ResizeMode,
        ResponderEvent,
        ReturnKeyType,
        Screen,
        ScreenAnimation,
        Scroll,
        ScrollEvent,
        ScrollOffset,
        Scrollable,
        Selection,
        SharedString,
        Slot,
        SnapAlignment,
        Spring,
        Stack,
        Styled,
        SubmitBehavior,
        Task,
        Text,
        Theme,
        Timing,
        ToastDuration,
        TouchClaim,
        Video,
        VideoInfo,
        VideoProgress,
        View,
        Viewability,
        ViewableItems,
        VirtualList,
        WeakComponent,
        WebNavigation,
        WebView,
        WritingDirection,
        asset,
        assets,
        canvas,
        image,
        input,
        input_accessory_view,
        methods,
        modal,
        module,
        native,
        native_module,
        px,
        relative,
        rgb,
        rgba,
        screen,
        scroll,
        stack,
        text,
        video,
        view,
        virtual_list,
        web_view,
        web_view_html,
    };
}

/// What the macros' code and nsh's generated workspace name.
#[doc(hidden)]
pub mod __private {
    pub use serde_json::Map as JsonMap;

    /// Implemented by nothing: what the generated workspace of an app
    /// calls when the app's crate has no `#[native_module]`
    /// ([`no_native_module`]) fails the build with this message.
    #[diagnostic::on_unimplemented(
        message = "the app's crate (nsh.json \"rust\") has no `#[native_module(\"…\")]` at its root",
        label = "the app's UI set",
        note = "add to the crate's lib.rs, with the specifier its TSX imports the set by: `#[native_module(\"my-app/native\")] fn native(registry: &mut Registry) {{ registry.register::<MyComponent>(); }}` (`use nsh_core::ui::prelude::*`)"
    )]
    pub trait NoNativeModule {}

    /// Implemented by nothing: `#[native_module]` in a module other than
    /// the crate's root fails with this message.
    #[diagnostic::on_unimplemented(
        message = "`#[native_module]` goes at the root of the crate (lib.rs)",
        label = "nsh's generated workspace calls the entry it makes there",
        note = "move the function with `#[native_module(\"…\")]` to the crate's lib.rs"
    )]
    pub trait NotAtRoot {}

    /// Stands in for the entry `#[native_module]` makes
    /// (`app::__nsh_native_module`) when the app's crate has none: a glob
    /// import of the crate in a block shadows it, so it is only called, and
    /// fails to compile, without the entry.
    pub fn no_native_module<R: NoNativeModule>() -> R {
        unreachable!("never compiles")
    }

    /// Stands in for `crate::__nsh_native_module` when `#[native_module]`
    /// is not at the crate's root (as [`no_native_module`]).
    pub fn native_module_not_at_root<R: NotAtRoot>() -> R {
        unreachable!("never compiles")
    }
}
