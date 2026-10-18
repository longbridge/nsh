//! Components written in Rust: the UI sets' components (`nsh-ui-mobile`),
//! run by the core on the engine thread.
//!
//! A [`Component`] is a function that runs once ([`Component::view`]) and
//! makes a tree of elements ([`view`], [`text`], [`image`], other
//! components) styled with typed methods ([`Styled`]: `.px_4()`,
//! `.rounded_xl()`, `.bg(theme().primary)`) that lower into the same rules
//! as Tailwind classes. The core expands the tree into nodes of its own and
//! lays it out in the same pass as everything else, so a component sizes
//! like any view, with no round trip. What changes is bound: a text, prop
//! or style given a signal, a memo, a [`Prop`] or a closure is a render
//! effect whose runs patch that part alone, and a region ([`dynamic`],
//! [`If`], [`Each`], [`catch_error`]) makes its own elements again. State
//! is signals the view makes (under the component's owner: they go with
//! it), changed by listeners, timers and tasks ([`ambient::spawn`]); the
//! component's listeners hear its elements' events first and decide what
//! reaches JS ([`Scope::emit`]). The environment and the theme are signals
//! too ([`ambient::window`], [`ambient::theme`]): reading one in a binding
//! subscribes it.
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

pub mod ambient;
pub(crate) mod animated;
mod asset;
mod back;
pub(crate) mod call;
mod callback;
pub(crate) mod class;
mod component;
pub(crate) mod element;
mod element_ref;
pub(crate) mod env;
mod events;
pub(crate) mod flow;
pub(crate) mod host;
mod js;
#[cfg(test)]
pub(crate) mod json_host;
pub(crate) mod keys;
pub(crate) mod module;
mod once;
pub mod palette;
mod prop;
pub mod reactive;
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
    Event, JsObject, PropEnum, Props, asset, assets, class, component, methods, module,
    native_module,
};

pub use self::{
    animated::{
        Animated, Animation, AnimationEnd, AnimationOperand, Interpolation, NativeNode,
        NativeValue, Spring, Timing, sample_decay, sample_decay_within, sample_spring,
    },
    asset::{AssetFile, AssetKind, FileAsset, FontAsset, ImageAsset, ImageSrc},
    call::{
        AnyValue, EventToJs, JsEvent, JsResult, MethodCall, OpCall, ParkedPost, Posts, RustCall,
        RustOp, TaskStart, ToJs,
    },
    callback::{Callback, CallbackArgs, CallbackHandle, CallbackRet, JsCall, Trace, Tracer},
    class::{
        Arg, AsyncCall, Class, ClassDef, ClassInstance, ClassRefs, ClassSpec, Instance, Member,
        MemberCall, MemberKind, MemberSpec, Ret, Thrown,
    },
    component::{
        Api, Component, ComponentError, ComponentSpec, Event, EventSpec, Method, MethodSpec,
        Methods, NoEvent, Param, PropDecode, PropKind, PropSpec, Props, Registry, Root,
        declarations,
    },
    element::{
        Accessible, AnyElement, Attrs, BoundListConfig, Child, Children, ComponentElement, ElNode,
        Element, FluentBuilder, Image, Interactive, IntoElement, Items, ListConfig, ListElement,
        Listener, ParentElement, Position, Slot, SlotElement, Style, Text, View, image, text, view,
    },
    element_ref::{ComponentRef, ElementRef},
    env::Environment,
    events::{
        GestureEvent, ImageProgress, NativeEvent, PanEvent, ScrollEvent, ScrollOffset, Selection,
        VideoInfo, VideoProgress, WebNavigation,
    },
    flow::{Each, If, IntoElements, Reset, catch_error, dynamic},
    host::{
        CanvasArg, CanvasPaint, CanvasView, Input, InputAccessoryView, Modal, NativeView,
        Orientation, Painter, Screen, Scroll, Stack, Video, WebView, canvas, input,
        input_accessory_view, modal, native, screen, scroll, stack, video, web_view, web_view_html,
    },
    js::{JsHost, JsIn, JsKind, JsOwned, JsRaw},
    module::{
        Function, FunctionCall, FunctionKind, FunctionSpec, Module, ModuleCx, ModuleFuture,
        ModuleParam, ModuleSpec, SyncCall,
    },
    once::Scope,
    prop::Prop,
    reactive::{
        IntoReactive, Memo, Owner, Reactive, ReadSignal, Runtime, Signal, SignalEquals, Stored,
        batch, cleanup, computed, create_root, effect, provide_context, signal, signal_with,
        stored, untracked, use_context,
    },
    responder::{ClaimRequest, PanGesture, ResponderEvent, TouchClaim},
    scrollable::Scrollable,
    services::{Fetch, HttpResponse, Prefetch, ToastDuration},
    status_bar::StatusBarEntry,
    style::{
        AdaptiveColor, BorderCurve, FontWeight, Length, StyleRefinement, Styled, WritingDirection,
        px, relative, rgb, rgba,
    },
    task::{Answer, DialogAnswer, ImageSize, Task},
    theme::{Theme, ThemeColors, ThemeError},
    value::{JsValue, NodeRef, PropEnum, PropError, SharedString, TsType, TsTypes, ts_of},
    virtual_list::{ListLayout, ViewableItems, VirtualList, virtual_list},
};
pub(crate) use self::{
    back::BackListener,
    component::{AnyInstance, caught},
    element::{ElKind, PlaceFn},
    task::{Action, AnswerSlot, ClipboardRead, Executor},
};
pub use crate::{
    AccessibilityRole, AccessibilityState, AutoCapitalize, AutoFill, BlendMode, BoxShadow, Cursor,
    Dialog, DialogButton, DialogButtonStyle, EdgeInsets, EllipsizeMode, EventKind, EventPayload,
    KeyboardAppearance, KeyboardDismissMode, KeyboardTaps, KeyboardType, KeyframeAnimation,
    Keyframes, LayoutAnimation, LayoutCurve, LayoutProperty, LayoutTransition, Measurement,
    ModalAnimation, ModalPresentation, PanActivation, PointerEvents, ResizeMode, ReturnKeyType,
    ScreenAnimation, SnapAlignment, StatusBarConfig, StatusBarSetting, StatusBarStyle,
    SubmitBehavior, TouchEvent, TransformKind, TransformOrigin, Viewability,
    platform::{Appearance, HttpBody, HttpHead, HttpRequest, Idiom, Locale, Os},
    style::{Color, OriginLen},
};

/// Everything a component's module needs: `use nsh_core::ui::prelude::*;`.
pub mod prelude {
    // Components that run once: signals, bindings, control flow; what they
    // read of the surface and ask of the platform.
    #[cfg(not(target_arch = "wasm32"))]
    pub use super::ambient::{background, blocking};
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
        // Host elements and their settings.
        AutoCapitalize,
        BlendMode,
        BorderCurve,
        BoxShadow,
        Callback,
        CanvasView,
        Child,
        Children,
        ClaimRequest,
        Color,
        Component,
        ComponentElement,
        ComponentError,
        ComponentRef,
        Cursor,
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
        // The network.
        HttpRequest,
        HttpResponse,
        ImageAsset,
        Input,
        InputAccessoryView,
        Instance,
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
        Methods,
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
        WebNavigation,
        WebView,
        WritingDirection,
        asset,
        assets,
        canvas,
        class,
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
    pub use super::{
        Each, If, IntoElements, IntoReactive, Memo, Owner, Prop, Reactive, ReadSignal, Scope,
        Signal, SignalEquals, Stored,
        ambient::{
            Delay, KeyOptions, Spawned, add_status_bar, allow_rtl, animate_layout,
            announce_for_accessibility, clipboard_has_string, clipboard_string, color,
            component_ref, dark, dialog, dismiss_keyboard, element_ref, fetch, font_scale,
            force_rtl, global, idiom, image_size, is_desktop, key_label, keyboard_duration,
            keyboard_height, locale, measure_text, native_value, next_frame, on_back_press, on_key,
            on_key_with, os, os_version, pixel_ratio, prefetch_image, reduce_motion,
            request_animation_frame, round_to_nearest_pixel, rtl, safe_area, screen_reader,
            set_clipboard, set_color_scheme, set_interval, set_timeout, spawn, theme, timer,
            timer_until, toast, update_status_bar, window,
        },
        batch, catch_error, cleanup, component, computed, dynamic, effect, provide_context, signal,
        signal_with, stored, untracked, use_context,
    };
}

/// What the macros' code and nsh's generated workspace name.
#[doc(hidden)]
pub mod __private {
    pub use serde_json::Map as JsonMap;

    pub use super::class::__private::*;

    /// Implemented by nothing: what the generated workspace of an app
    /// calls when a crate of the app has no `#[native_module]`
    /// ([`no_native_module`]) fails the build with this message.
    #[diagnostic::on_unimplemented(
        message = "a crate of the app (nsh.json \"rust\") has no `#[native_module(\"…\")]` at its root",
        label = "the crate's UI set",
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
