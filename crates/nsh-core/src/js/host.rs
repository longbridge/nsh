//! The props of host elements as JS sets them: how each one is decoded
//! from the JS value and what its TypeScript type is, from one definition.
//! `#[derive(HostProps)]` on [`crate::Prop`] (and the enums it nests) maps
//! JS names to variants, decodes their payloads with [`HostValue`] and
//! writes the [`HostPropSpec`]s the element types are generated from. The
//! decoders read JS values through [`HostInput`], which `nsh-js` implements
//! for QuickJS values: no JSON in between.

use std::sync::Arc;

pub use nsh_macros::HostProps;

use crate::{EdgeInsets, EventKind, geometry::Points, style::Color, ui::TsTypes};

/// A JS value as host props read it.
pub trait HostInput: Sized {
    /// `null` or `undefined`.
    fn is_nullish(&self) -> bool;
    /// An object (arrays and functions are objects too).
    fn is_object(&self) -> bool;
    /// A boolean.
    fn as_bool(&self) -> Option<bool>;
    /// A number.
    fn as_number(&self) -> Option<f64>;
    /// A string.
    fn as_string(&self) -> Option<String>;
    /// Field `key` of an object (`None`: not an object, or no such field).
    fn field(&self, key: &str) -> Option<Self>;
    /// The items of an array (empty for anything else).
    fn items(&self) -> Vec<Self> {
        match self.as_list() {
            Some(list) => (0..list.len()).filter_map(|i| list.item(i)).collect(),
            None => Vec::new(),
        }
    }
    /// An array, to read by index (checked once, not per item).
    fn as_list(&self) -> Option<Self::List>;
    /// What [`HostInput::as_list`] gives.
    type List: HostList<Item = Self>;
    /// The fields of an object (empty for anything else).
    fn entries(&self) -> Vec<(String, Self)>;
    /// Truthy, as JS sees it.
    fn truthy(&self) -> bool;
    /// The native animation node this value is (`AnimatedNode.__av`).
    fn animated_node(&self) -> Option<crate::animation::NodeId> {
        self.field("__av")
            .and_then(|v| v.as_number())
            .map(|n| n as crate::animation::NodeId)
    }
}

/// A JS array, read by index.
pub trait HostList {
    /// Its items.
    type Item;
    /// Its length.
    fn len(&self) -> usize;
    /// Whether it is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Item `index`.
    fn item(&self, index: usize) -> Option<Self::Item>;
}

/// Where decoding reports what it could not take.
#[derive(Debug, Default)]
pub struct HostCx {
    /// Warnings for the app's console (`unknown role`).
    pub warnings: Vec<String>,
}

impl HostCx {
    /// Reports `message`.
    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}

/// A prop's value: how it comes out of a JS value and its TypeScript type.
/// `decode` gives `None` for a value it does not take (`undefined`, another
/// type): the prop then gets its default.
pub trait HostValue: Sized {
    /// The value in `input`.
    fn decode<I: HostInput>(input: &I, cx: &mut HostCx) -> Option<Self>;
    /// Its TypeScript type, declaring the named types it uses.
    fn ts_type(types: &mut TsTypes) -> String;
}

/// Where a host prop applies, for the generated element types.
pub mod on {
    /// Every element.
    pub const ALL: u32 = 1;
    /// `<text>`.
    pub const TEXT: u32 = 1 << 1;
    /// `<image>`.
    pub const IMAGE: u32 = 1 << 2;
    /// Scroll views and lists (their scrolling behavior).
    pub const SCROLLING: u32 = 1 << 3;
    /// `<scroll>` only.
    pub const SCROLL: u32 = 1 << 4;
    /// Lists (`List`).
    pub const LIST: u32 = 1 << 5;
    /// `<input>`.
    pub const INPUT: u32 = 1 << 6;
    /// Screens of a stack.
    pub const SCREEN: u32 = 1 << 7;
    /// `<modal>`.
    pub const MODAL: u32 = 1 << 8;
    /// `<webview>`.
    pub const WEBVIEW: u32 = 1 << 9;
    /// `<video>`.
    pub const VIDEO: u32 = 1 << 10;
    /// Input accessories (iOS).
    pub const INPUT_ACCESSORY: u32 = 1 << 11;
    /// Set by the runtime itself, not part of the elements' types.
    pub const INTERNAL: u32 = 1 << 31;
}

/// The platform a prop is for: everywhere, or inside `ios={{…}}` /
/// `android={{…}}` / `desktop={{…}}` / `web={{…}}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Every platform.
    All,
    /// `ios={{…}}`.
    Ios,
    /// `android={{…}}`.
    Android,
    /// `desktop={{…}}` (macOS, Windows, Linux, desktop browsers).
    Desktop,
    /// `web={{…}}`.
    Web,
}

/// One host prop, for the generated TypeScript.
#[derive(Debug, Clone)]
pub struct HostPropSpec {
    /// Its JS name.
    pub name: &'static str,
    /// Its TypeScript type.
    pub ts: String,
    /// Documentation.
    pub doc: &'static str,
    /// The elements it applies to ([`on`] bits).
    pub on: u32,
    /// The platform it is for.
    pub platform: Platform,
}

/// An enum of host props (`#[derive(HostProps)]`).
pub trait HostProps: Sized {
    /// The prop JS name `name` sets with `input`: `None` for a name that is
    /// not one of these props, `Some(None)` when nothing is to be set.
    fn decode<I: HostInput>(name: &str, input: &I, cx: &mut HostCx) -> Option<Option<Self>>;
    /// This prop at its default (what clearing it from JS sets); `None` for
    /// the props the runtime sets once.
    fn reset(&self) -> Option<Self>;
    /// Whether `self` and `other` set the same property.
    fn same_kind(&self, other: &Self) -> bool;
    /// The props, for the generated TypeScript.
    fn specs(types: &mut TsTypes, out: &mut Vec<HostPropSpec>);
}

/// Numbers held to a range after decoding (`#[prop(min = …, max = …)]`).
pub trait Clamped {
    /// `self` within `min..=max` (either bound left out).
    fn clamped(self, min: Option<f64>, max: Option<f64>) -> Self;
}

impl Clamped for f32 {
    fn clamped(self, min: Option<f64>, max: Option<f64>) -> Self {
        let v = min.map_or(self, |m| self.max(m as f32));
        max.map_or(v, |m| v.min(m as f32))
    }
}

impl Clamped for Points {
    fn clamped(self, min: Option<f64>, max: Option<f64>) -> Self {
        Points(self.0.clamped(min, max))
    }
}

impl Clamped for u32 {
    fn clamped(self, min: Option<f64>, max: Option<f64>) -> Self {
        let v = min.map_or(self, |m| self.max(m as u32));
        max.map_or(v, |m| v.min(m as u32))
    }
}

impl<T: Clamped> Clamped for Option<T> {
    fn clamped(self, min: Option<f64>, max: Option<f64>) -> Self {
        self.map(|v| v.clamped(min, max))
    }
}

impl HostValue for bool {
    fn decode<I: HostInput>(input: &I, _: &mut HostCx) -> Option<Self> {
        input.as_bool()
    }

    fn ts_type(_: &mut TsTypes) -> String {
        "boolean".into()
    }
}

impl HostValue for f32 {
    fn decode<I: HostInput>(input: &I, _: &mut HostCx) -> Option<Self> {
        input.as_number().map(|n| n as f32)
    }

    fn ts_type(_: &mut TsTypes) -> String {
        "number".into()
    }
}

/// A number of points.
impl HostValue for Points {
    fn decode<I: HostInput>(input: &I, cx: &mut HostCx) -> Option<Self> {
        f32::decode(input, cx).map(Points)
    }

    fn ts_type(_: &mut TsTypes) -> String {
        "number".into()
    }
}

impl HostValue for u32 {
    /// Negative numbers are 0, fractions are cut.
    fn decode<I: HostInput>(input: &I, _: &mut HostCx) -> Option<Self> {
        input.as_number().map(|n| n.max(0.0) as u32)
    }

    fn ts_type(_: &mut TsTypes) -> String {
        "number".into()
    }
}

impl HostValue for String {
    /// Anything but a string is the empty string's default.
    fn decode<I: HostInput>(input: &I, _: &mut HostCx) -> Option<Self> {
        input.as_string()
    }

    fn ts_type(_: &mut TsTypes) -> String {
        "string".into()
    }
}

impl HostValue for Arc<str> {
    /// The empty string is no value.
    fn decode<I: HostInput>(input: &I, _: &mut HostCx) -> Option<Self> {
        input.as_string().filter(|s| !s.is_empty()).map(Into::into)
    }

    fn ts_type(_: &mut TsTypes) -> String {
        "string".into()
    }
}

impl HostValue for Box<str> {
    /// The empty string is no value.
    fn decode<I: HostInput>(input: &I, _: &mut HostCx) -> Option<Self> {
        input.as_string().filter(|s| !s.is_empty()).map(Into::into)
    }

    fn ts_type(_: &mut TsTypes) -> String {
        "string".into()
    }
}

impl HostValue for Color {
    /// A CSS color string; anything else, or an invalid color, is none.
    fn decode<I: HostInput>(input: &I, _: &mut HostCx) -> Option<Self> {
        input.as_string().and_then(|s| Color::parse(&s))
    }

    fn ts_type(_: &mut TsTypes) -> String {
        "ColorValue".into()
    }
}

/// `None` when there is no value: the prop is cleared.
impl<T: HostValue> HostValue for Option<T> {
    fn decode<I: HostInput>(input: &I, cx: &mut HostCx) -> Option<Self> {
        Some(T::decode(input, cx))
    }

    fn ts_type(types: &mut TsTypes) -> String {
        T::ts_type(types)
    }
}

/// The items `T` takes, the others skipped.
impl<T: HostValue> HostValue for Vec<T> {
    fn decode<I: HostInput>(input: &I, cx: &mut HostCx) -> Option<Self> {
        Some(
            input
                .items()
                .iter()
                .filter_map(|i| T::decode(i, cx))
                .collect(),
        )
    }

    fn ts_type(types: &mut TsTypes) -> String {
        format!("{}[]", T::ts_type(types))
    }
}

/// `{ top, right, bottom, left }` (each 0 when left out).
impl HostValue for EdgeInsets {
    fn decode<I: HostInput>(input: &I, _: &mut HostCx) -> Option<Self> {
        let side = |key| Points(input.field(key).and_then(|v| v.as_number()).unwrap_or(0.0) as f32);
        Some(EdgeInsets {
            top: side("top"),
            right: side("right"),
            bottom: side("bottom"),
            left: side("left"),
        })
    }

    fn ts_type(_: &mut TsTypes) -> String {
        "{ top?: number; right?: number; bottom?: number; left?: number }".into()
    }
}

/// Field `key` of `input` as `T`.
pub fn field<T: HostValue, I: HostInput>(input: &I, key: &str, cx: &mut HostCx) -> Option<T> {
    input.field(key).and_then(|v| T::decode(&v, cx))
}

/// The payloads of the host elements' events.
const EVENT_TYPES: &str = r#"/** A frame relative to the parent: the payload of `onLayout`. */
export interface LayoutEvent {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A drag: translation since the start and velocity (points / second). */
export interface PanEvent {
  translationX: number;
  translationY: number;
  velocityX: number;
  velocityY: number;
  /** The `pan.x` value now (with one). */
  valueX?: number;
  /** The `pan.y` value now (with one). */
  valueY?: number;
}

/** Where the mouse is over a hovered view: the payload of `onHoverMove`. */
export interface HoverEvent {
  /** Position in the view (points, its transforms undone). */
  locationX: number;
  locationY: number;
  /** Position in the root view the view is in (the surface or its modal), as touches' `pageX`. */
  pageX: number;
  pageY: number;
  altKey: boolean;
  ctrlKey: boolean;
  /** Command on Apple platforms, the Windows / Super key elsewhere. */
  metaKey: boolean;
  shiftKey: boolean;
}

/** Where a context menu was asked for: the payload of `onContextMenu`. */
export interface ContextMenuEvent {
  /** Position in the view (points, its transforms undone). */
  locationX: number;
  locationY: number;
  /** Position in the root view the view is in: where `Menu.popup` opens a menu. */
  pageX: number;
  pageY: number;
  altKey: boolean;
  ctrlKey: boolean;
  /** Command on Apple platforms, the Windows / Super key elsewhere. */
  metaKey: boolean;
  shiftKey: boolean;
}

/** Where a scroll view is: the payload of `onScroll`. */
export interface ScrollEvent {
  x: number;
  y: number;
  /** The content's size (zoomed). */
  contentWidth: number;
  contentHeight: number;
  width: number;
  height: number;
  /** The zoom scale (1 unless the scroll view zooms). */
  zoomScale: number;
}

/** A line of a laid-out text (in the text's content box). */
export interface TextLayoutLine {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
  ascender: number;
  descender: number;
  capHeight: number;
  xHeight: number;
}

/** Where a web view is: the payload of its navigation events. */
export interface WebViewNavigation {
  url: string;
  title: string;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
}
"#;

/// The events of the host elements, as their handler props: the event, the
/// elements whose props declare it ([`on`] bits), the handler's type and
/// its documentation. An event whose payload differs between elements has
/// a row per payload.
pub const EVENTS: &[(EventKind, u32, &str, &str)] = &[
    (EventKind::Press, on::ALL, "() => void", ""),
    (EventKind::LongPress, on::ALL, "() => void", ""),
    (EventKind::PressDown, on::ALL, "() => void", ""),
    (EventKind::PressUp, on::ALL, "() => void", ""),
    (
        EventKind::HoverIn,
        on::ALL,
        "() => void",
        "The mouse came over the view or a view inside it (never on touches).",
    ),
    (
        EventKind::HoverOut,
        on::ALL,
        "() => void",
        "The mouse left the view and the views inside it, or the window.",
    ),
    (
        EventKind::HoverMove,
        on::ALL,
        "(e: HoverEvent) => void",
        "The mouse moved over the view (or a view inside it) with no button down, at most once per frame.",
    ),
    (
        EventKind::ContextMenu,
        on::ALL,
        "(e: ContextMenuEvent) => void",
        "The secondary button (a right click, Control-click on Apple platforms) was pressed over the view: open a menu with `Menu.popup({ x: e.pageX, y: e.pageY })`. The nearest listener hears it; a text input or selectable text nearer shows its own menu instead.",
    ),
    (
        EventKind::FileDragEnter,
        on::ALL,
        "() => void",
        "A local file drag entered this desktop destination.",
    ),
    (
        EventKind::FileDragLeave,
        on::ALL,
        "() => void",
        "A local file drag left this desktop destination.",
    ),
    (
        EventKind::FileDrop,
        on::ALL,
        "(e: { uris: string[] }) => void",
        "Local file URLs dropped on this desktop destination.",
    ),
    (EventKind::Layout, on::ALL, "(e: LayoutEvent) => void", ""),
    (EventKind::PanStart, on::ALL, "(e: PanEvent) => void", ""),
    (
        EventKind::PanEnd,
        on::ALL,
        "(e: PanEvent) => void",
        "With the release velocity, e.g. to start a `decay`.",
    ),
    (
        EventKind::PinchStart,
        on::ALL,
        "(e: { scale: number; velocity: number }) => void",
        "",
    ),
    (
        EventKind::PinchEnd,
        on::ALL,
        "(e: { scale: number; velocity: number }) => void",
        "",
    ),
    (
        EventKind::RotationStart,
        on::ALL,
        "(e: { rotation: number; velocity: number }) => void",
        "",
    ),
    (
        EventKind::RotationEnd,
        on::ALL,
        "(e: { rotation: number; velocity: number }) => void",
        "",
    ),
    (
        EventKind::AccessibilityAction,
        on::ALL,
        "(e: { actionName: string }) => void",
        "",
    ),
    (
        EventKind::TextLayout,
        on::TEXT,
        "(e: { lines: TextLayoutLine[] }) => void",
        "The laid-out lines (in the text's content box), after each layout.",
    ),
    (
        EventKind::LoadStart,
        on::IMAGE | on::VIDEO,
        "() => void",
        "",
    ),
    (
        EventKind::LoadStart,
        on::WEBVIEW,
        "(e: WebViewNavigation) => void",
        "",
    ),
    (
        EventKind::Progress,
        on::IMAGE,
        "(e: { loaded: number; total: number }) => void",
        "Download progress in bytes (`total` 0 when unknown); at most every 100 ms.",
    ),
    (
        EventKind::Progress,
        on::VIDEO,
        "(e: { currentTime: number; duration: number }) => void",
        "",
    ),
    (
        EventKind::Load,
        on::IMAGE,
        "(e: { width: number; height: number }) => void",
        "The source image's size in pixels (not the decoded, downsampled one).",
    ),
    (
        EventKind::Load,
        on::WEBVIEW,
        "(e: WebViewNavigation) => void",
        "",
    ),
    (
        EventKind::Load,
        on::VIDEO,
        "(e: { duration: number; width: number; height: number }) => void",
        "",
    ),
    (
        EventKind::Error,
        on::IMAGE | on::WEBVIEW | on::VIDEO,
        "(e: { message: string }) => void",
        "",
    ),
    (
        EventKind::LoadEnd,
        on::IMAGE,
        "() => void",
        "After `onLoad` or `onError`.",
    ),
    (
        EventKind::Scroll,
        on::SCROLLING,
        "(e: ScrollEvent) => void",
        "",
    ),
    (
        EventKind::ScrollBeginDrag,
        on::SCROLLING,
        "(e: { x: number; y: number }) => void",
        "",
    ),
    (
        EventKind::ScrollEndDrag,
        on::SCROLLING,
        "(e: { x: number; y: number }) => void",
        "",
    ),
    (
        EventKind::MomentumScrollBegin,
        on::SCROLLING,
        "(e: { x: number; y: number }) => void",
        "",
    ),
    (
        EventKind::MomentumScrollEnd,
        on::SCROLLING,
        "(e: { x: number; y: number }) => void",
        "",
    ),
    (
        EventKind::ScrollEnd,
        on::SCROLL | on::LIST,
        "(e: { x: number; y: number }) => void",
        "",
    ),
    (EventKind::Refresh, on::SCROLL | on::LIST, "() => void", ""),
    (EventKind::EndReached, on::LIST, "() => void", ""),
    (
        EventKind::ChangeText,
        on::INPUT,
        "(text: string) => void",
        "",
    ),
    (
        EventKind::SubmitEditing,
        on::INPUT,
        "(e: { text: string }) => void",
        "",
    ),
    (EventKind::Focus, on::INPUT, "() => void", ""),
    (EventKind::Blur, on::INPUT, "() => void", ""),
    (
        EventKind::SelectionChange,
        on::INPUT,
        "(e: { start: number; end: number }) => void",
        "",
    ),
    (
        EventKind::KeyPress,
        on::INPUT,
        "(e: { key: string }) => void",
        "Before the text changes: `key` is the typed text, `Backspace` or `Enter`.",
    ),
    (
        EventKind::CompositionChange,
        on::INPUT,
        "(e: { composing: boolean; text: string }) => void",
        "An input method's composition started, changed or ended: `text` is what is being composed (pinyin before a character is chosen), empty once it ends. The committed text reaches `onChange` before the composition's end; while composing, the platform holds the `value` the app sets and shows it once the composition ends.",
    ),
    (
        EventKind::ContentSizeChange,
        on::INPUT,
        "(e: { width: number; height: number }) => void",
        "The size of the text (at the input's width when `multiline`; one empty line's while it is empty, whatever its placeholder): grow a multiline input with it.",
    ),
    (
        EventKind::Message,
        on::WEBVIEW,
        "(e: { data: string }) => void",
        "The page called `window.ReactNativeWebView.postMessage(data)`.",
    ),
    (
        EventKind::NavigationStateChange,
        on::WEBVIEW,
        "(e: WebViewNavigation) => void",
        "",
    ),
    (EventKind::End, on::VIDEO, "() => void", ""),
    (
        EventKind::Buffer,
        on::VIDEO,
        "(e: { isBuffering: boolean }) => void",
        "",
    ),
];

/// The events no element's props declare: the root's (`BackHandler`,
/// `nsh/keymap`, dialogs, `Image.getSize`, clipboard reads, menus), the runtime's own (touches, insets, native
/// components' events) and those whose payload a module makes
/// (`List`'s viewable items, with the app's items).
pub const INTERNAL_EVENTS: &[EventKind] = &[
    EventKind::Touch,
    EventKind::Insets,
    EventKind::NativeEvent,
    EventKind::Dialog,
    EventKind::BackPress,
    EventKind::ImageInfo,
    EventKind::Clipboard,
    EventKind::KeyCommand,
    EventKind::Menu,
    EventKind::ViewableItemsChanged,
    // `NavPage`'s `onDismiss` / `onBackPress` and `Dialog`'s `onClose` /
    // `onCancel` / `onOpenChange` / `onOpen`: the navigation's handlers
    // (`nsh-js` `navigation.rs`), declared with its components.
    EventKind::BackRequest,
    EventKind::Dismiss,
    EventKind::RequestClose,
    EventKind::Show,
];

/// The interfaces of the host props, by the elements their props apply
/// to: `HostProps` for every element, `TextHostProps` for `<text>`, ...
const GROUPS: &[(u32, &str, &str)] = &[
    (on::ALL, "", "every element"),
    (on::TEXT, "Text", "`<Text>`"),
    (on::IMAGE, "Image", "`<Image>`"),
    (
        on::SCROLLING,
        "Scrolling",
        "scroll views and lists (how they scroll)",
    ),
    (on::SCROLL, "Scroll", "`<Scroll>`"),
    (on::LIST, "List", "lists"),
    (on::INPUT, "Input", "`<Input>`"),
    (on::SCREEN, "NavPage", "the pages of a `NavStack`"),
    (on::MODAL, "Dialog", "dialogs"),
    (on::WEBVIEW, "WebView", "web views"),
    (on::VIDEO, "Video", "videos"),
    (on::INPUT_ACCESSORY, "InputAccessory", "input accessories"),
];

/// The declarations of the host elements' props: the named types they use
/// and one interface per element group and platform (`InputHostProps`,
/// `IOSInputHostProps`, ...), which the element types and the components
/// of `nsh/webview`, `nsh/video`, ... extend.
pub fn declarations() -> String {
    use std::fmt::Write as _;
    let mut types = TsTypes::default();
    let mut specs = Vec::new();
    <crate::Prop as HostProps>::specs(&mut types, &mut specs);
    let mut out = String::from(EVENT_TYPES);
    for declaration in types.declarations() {
        let _ = write!(out, "\n{declaration}\n");
    }
    for (bit, group, what) in GROUPS {
        for (platform, prefix, inside) in [
            (Platform::All, "", ""),
            (Platform::Ios, "IOS", " inside `ios={{…}}`"),
            (Platform::Android, "Android", " inside `android={{…}}`"),
            (Platform::Desktop, "Desktop", " inside `desktop={{…}}`"),
            (Platform::Web, "Web", " inside `web={{…}}`"),
        ] {
            let props: Vec<&HostPropSpec> = specs
                .iter()
                .filter(|s| s.on & bit != 0 && s.platform == platform)
                .collect();
            // Events are for every platform.
            let events: Vec<_> = EVENTS
                .iter()
                .filter(|e| e.1 & bit != 0 && platform == Platform::All)
                .collect();
            if props.is_empty() && events.is_empty() {
                continue;
            }
            let _ = write!(
                out,
                "\n/** The props of {what}{inside}. */\nexport interface {prefix}{group}HostProps {{\n"
            );
            for p in props {
                if !p.doc.is_empty() {
                    let _ = writeln!(out, "  /** {} */", p.doc);
                }
                let _ = writeln!(out, "  {}?: {};", p.name, p.ts);
            }
            for (kind, _, handler, doc) in events {
                if !doc.is_empty() {
                    let _ = writeln!(out, "  /** {doc} */");
                }
                let _ = writeln!(out, "  {}?: {handler};", kind.handler_name());
            }
            out.push_str("}\n");
        }
    }
    out
}
