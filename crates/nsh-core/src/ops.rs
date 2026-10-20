use std::sync::Arc;

use crate::{
    animation::AnimationOp,
    geometry::{Bounds, DevicePixels, Percentage, Point, Points, Size},
    js::host::HostProps,
    props::{AccessibilityProp, EdgeInsets, LayoutAnimation, PointerEvents},
    style::{Color, EllipsizeMode, Style, StylePatch},
    ui::PropEnum,
};

/// Identifier of a node, assigned by the JS renderer.
///
/// Ids are small, dense and increasing, so the core indexes them directly.
pub type NodeId = u32;

/// Id of the surface root view. It exists before any JS runs.
pub const ROOT_ID: NodeId = 1;

/// Element tag of a host node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    /// A container view (`<view>`).
    View,
    /// A text block, or a span when nested in another text (`<text>`).
    Text,
    /// An image (`<image>`).
    Image,
    /// A scrolling container (`<scroll>`).
    Scroll,
    /// A virtualized list (`<list>`, created by the runtime's `List`).
    List,
    /// A recycled list cell (created by the runtime's `List`).
    Cell,
    /// A text field, or a text area when `multiline` (`<input>`).
    Input,
    /// A native navigation stack (`<stack>`); its children are screens.
    Stack,
    /// One screen of a stack (`<screen>`), laid out at the stack's size.
    Screen,
    /// A modal presented over everything (`<modal>`), laid out at the size
    /// the platform presents it at.
    Modal,
    /// A view placed in a screen's native header (`<headeritem slot>`),
    /// sized to its content.
    HeaderItem,
    /// A bar over the keyboard of the inputs naming its `nativeID`
    /// (`<inputaccessory>`, iOS), as wide as the window and as tall as its
    /// content.
    InputAccessory,
    /// A 2D drawing (`<canvas>`, filled by [`Op::Draw`]).
    Canvas,
    /// A web page (`<webview>`).
    WebView,
    /// A video player (`<video>`).
    Video,
    /// A component the embedding app registered (`<native component>`,
    /// created by `nsh/native`'s `nativeComponent`).
    Native,
}

impl Tag {
    /// Parses a host element's name (what `nsh:rt`'s `element` takes).
    #[inline]
    pub fn from_name(name: &str) -> Option<Tag> {
        Some(match name {
            "view" => Tag::View,
            "text" => Tag::Text,
            "image" => Tag::Image,
            "scroll" => Tag::Scroll,
            "list" => Tag::List,
            "cell" => Tag::Cell,
            "input" => Tag::Input,
            "stack" => Tag::Stack,
            "screen" => Tag::Screen,
            "modal" => Tag::Modal,
            "headeritem" => Tag::HeaderItem,
            "inputaccessory" => Tag::InputAccessory,
            "canvas" => Tag::Canvas,
            "webview" => Tag::WebView,
            "video" => Tag::Video,
            "native" => Tag::Native,
            _ => return None,
        })
    }
}

macro_rules! events {
    ($($variant:ident = $prop:literal $(as $js:literal)?: $doc:literal,)*) => {
        /// An event a node can listen to.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum EventKind {
            $(#[doc = $doc] $variant,)*
        }

        impl EventKind {
            /// Every kind, in discriminant order (`ALL[k as usize] == k`).
            pub const ALL: &'static [EventKind] = &[$(EventKind::$variant,)*];

            /// Parses an event's protocol name such as `onPress` (what
            /// platforms send and log; [`EventKind::prop_name`]).
            #[inline]
            pub fn from_prop(name: &str) -> Option<EventKind> {
                match name {
                    $($prop => Some(EventKind::$variant),)*
                    _ => None,
                }
            }

            /// The protocol name of this event (platforms send and log it;
            /// apps name handlers with [`EventKind::handler_name`]).
            #[inline]
            pub const fn prop_name(self) -> &'static str {
                match self {
                    $(EventKind::$variant => $prop,)*
                }
            }

            /// The handler prop apps set for this event (`onChange` for
            /// [`EventKind::ChangeText`]); empty for the events only the
            /// runtime listens to (a dialog's dismissal, the root's back
            /// button), which no prop name sets.
            #[inline]
            pub const fn handler_name(self) -> &'static str {
                match self {
                    $(EventKind::$variant => events!(@js $prop $($js)?),)*
                }
            }

            /// Parses a handler prop name such as `onPress`
            /// ([`EventKind::handler_name`]).
            #[inline]
            pub fn from_handler(name: &str) -> Option<EventKind> {
                if name.is_empty() {
                    return None;
                }
                #[allow(unreachable_patterns)]
                match name {
                    $(events!(@js $prop $($js)?) => Some(EventKind::$variant),)*
                    _ => None,
                }
            }
        }
    };
    (@js $prop:literal) => { $prop };
    (@js $prop:literal $js:literal) => { $js };
}

// The order is part of the platform protocols (`encode.rs`, `Native.java`):
// append new kinds at the end.
events! {
    Press = "onPress": "A tap / click.",
    LongPress = "onLongPress": "A press held for about half a second.",
    PanStart = "onPanStart": "A drag started (payload: [`EventPayload::Pan`]).",
    PanEnd = "onPanEnd": "A drag ended (payload: [`EventPayload::Pan`] with velocity).",
    Refresh = "onRefresh": "Pull-to-refresh was triggered on a scroll or list.",
    EndReached = "onEndReached": "A list was scrolled close to its end.",
    ScrollEnd = "onScrollEnd": "Scrolling came to rest (payload: [`EventPayload::Scroll`]).",
    Load = "onLoad": "An image loaded (payload: [`EventPayload::Image`]).",
    Error = "onError": "An image failed to load (payload: [`EventPayload::Error`]).",
    ChangeText = "onChangeText" as "onChange": "The user edited an input (payload: [`EventPayload::Text`]).",
    SubmitEditing = "onSubmitEditing" as "onSubmit": "The return key was pressed in an input (payload: [`EventPayload::Text`]).",
    Focus = "onFocus": "An input became focused.",
    Blur = "onBlur": "An input lost focus.",
    PressDown = "onPressIn" as "onPressDown": "A press went down on a pressable view (a finger, a pen or a mouse button).",
    PressUp = "onPressOut" as "onPressUp": "The press ended: released, moved away or cancelled.",
    Layout = "onLayout": "The frame changed (payload: [`EventPayload::Layout`], relative to the parent).",
    PinchStart = "onPinchStart": "A pinch started (payload: `scale`, `velocity`).",
    PinchEnd = "onPinchEnd": "A pinch ended (payload: `scale`, `velocity`).",
    RotationStart = "onRotationStart": "A two-finger rotation started (payload: `rotation`, `velocity`).",
    RotationEnd = "onRotationEnd": "A two-finger rotation ended (payload: `rotation`, `velocity`).",
    AccessibilityAction = "onAccessibilityAction": "An assistive technology performed an action (payload: `actionName`).",
    Dismiss = "onDismiss" as "": "A screen was popped or a modal dismissed by the user or the system (`NavPage`'s `onDismiss`, `Dialog`'s `onClose`).",
    RequestClose = "onRequestClose" as "": "The user tried to close a modal natively (Escape or the back button, a sheet swiped down; payload: `reason`: `\"cancel\"` or `\"swipe\"`, `\"cancel\"` when none); `Dialog`'s `onCancel` and `onOpenChange` (a press on its backdrop part is handled in JS).",
    Show = "onShow" as "": "A modal finished appearing (`Dialog`'s `onOpen`).",
    Insets = "onInsets": "The insets of a screen or modal changed (payload: `top`, `right`, `bottom`, `left`).",
    LoadStart = "onLoadStart": "A web view, video or image started loading.",
    Message = "onMessage": "A web page posted a message (payload: `data`).",
    NavigationStateChange = "onNavigationStateChange": "A web view navigated (payload: `url`, `title`, `loading`, `canGoBack`, `canGoForward`).",
    Progress = "onProgress": "Video playback progressed (payload: `currentTime`, `duration`).",
    End = "onEnd": "A video played to its end.",
    Buffer = "onBuffer": "A video started or stopped buffering (payload: `isBuffering`).",
    ViewableItemsChanged = "onViewableItemsChanged": "The range of visible list items changed (payload: `first`, `last`).",
    BackPress = "onBackPress" as "": "The Android back button was pressed and nothing native handled it (root only).",
    Scroll = "onScroll": "A scroll view moved, at most once per frame (payload: `x`, `y`, `contentWidth`, `contentHeight`, `width`, `height`).",
    ScrollBeginDrag = "onScrollBeginDrag": "The user started dragging a scroll view (payload: [`EventPayload::Scroll`]).",
    ScrollEndDrag = "onScrollEndDrag": "The user let go of a scroll view (payload: [`EventPayload::Scroll`]).",
    MomentumScrollBegin = "onMomentumScrollBegin": "A scroll view started gliding after a fling (payload: [`EventPayload::Scroll`]).",
    MomentumScrollEnd = "onMomentumScrollEnd": "A scroll view stopped gliding (payload: [`EventPayload::Scroll`]).",
    Dialog = "onDialog": "An alert or action sheet closed (root only; payload: `id`, `index`, -1 when dismissed).",
    Touch = "onTouch": "Touches on the view or inside it, for the JS touch and responder handlers (payload: [`EventPayload::Touch`]).",
    SelectionChange = "onSelectionChange": "The selection (or caret) of an input moved (payload: `start`, `end` in UTF-16 units).",
    KeyPress = "onKeyPress": "A key was pressed in an input (payload: `key`: the character, `Backspace` or `Enter`).",
    LoadEnd = "onLoadEnd": "An image finished loading, or failed.",
    ImageInfo = "onImageInfo": "An `Image.getSize` / `Image.prefetch` request finished (root only; payload: `id`, `width` and `height` in pixels, or `error`).",
    TextLayout = "onTextLayout": "A text block was laid out (payload: [`EventPayload::TextLayout`]).",
    ContentSizeChange = "onContentSizeChange": "The size of an input's text changed (payload: `width`, `height`; raised by the core).",
    BackRequest = "onBackRequest" as "onBackPress": "The user pressed back on a screen (the header's back button, Android back, the desktops' back keys and mouse button, the browser's back), whether or not `popOnBack` let it pop.",
    NativeEvent = "onNativeEvent": "A registered native component emitted an event (payload: [`EventPayload::Native`]).",
    Clipboard = "onClipboard": "A clipboard read finished (root only; payload: `id`, `has`: whether it holds what was asked; for text `text`, unless only `has` was asked; for an image `uri` (a `data:` URI), `width` and `height`, unless only `has` was asked; for files `files`, their `file:` URLs one per line).",
    KeyCommand = "onKeyCommand": "A key matched a binding of `nsh/keymap` (root only; payload: the binding's id, [`crate::keymap::KeyBinding::id`]).",
    Menu = "onMenu": "A menu item was chosen: the application menu's or a context menu's (root only; desktops; payload: `id`), or a context menu closed (`closed`: true, after its item).",
    HoverIn = "onHoverIn": "The mouse came over the view or a view inside it (never on touches).",
    HoverOut = "onHoverOut": "The mouse left the view and the views inside it, or the window.",
    FileDragEnter = "onFileDragEnter": "A local file drag entered an eligible desktop drop destination.",
    FileDragLeave = "onFileDragLeave": "A local file drag left its desktop drop destination.",
    FileDrop = "onFileDrop": "Local files were dropped (payload: [`EventPayload::FileDrop`]).",
    HoverMove = "onHoverMove": "The mouse moved over a hovered view, with no button down (payload: [`EventPayload::Hover`]; at most one per frame).",
    ContextMenu = "onContextMenu": "The secondary button was pressed over the view (payload: [`EventPayload::ContextMenu`]): the nearest listener around the view under the pointer hears it, unless a text input or selectable text is nearer.",
}

/// What an input holds, for the system's autofill and suggestions
/// (`autoComplete`, `textContentType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoFill {
    /// The platform decides.
    #[default]
    Default,
    /// No autofill.
    Off,
    /// Account name.
    Username,
    /// Existing password.
    Password,
    /// A password being chosen.
    NewPassword,
    /// A code from a text message or e-mail.
    OneTimeCode,
    /// E-mail address.
    Email,
    /// Phone number.
    Tel,
    /// Full name.
    Name,
    /// First name.
    GivenName,
    /// Last name.
    FamilyName,
    /// Middle name.
    MiddleName,
    /// Nickname.
    Nickname,
    /// Company.
    Organization,
    /// Street address, all lines.
    StreetAddress,
    /// First address line.
    AddressLine1,
    /// Second address line.
    AddressLine2,
    /// City.
    City,
    /// State or region.
    Region,
    /// Postal code.
    PostalCode,
    /// Country.
    Country,
    /// Credit card number.
    CreditCardNumber,
    /// Credit card expiry date.
    CreditCardExpiration,
    /// Credit card security code.
    CreditCardSecurityCode,
    /// Name on the credit card.
    CreditCardName,
    /// Web address.
    Url,
    /// Birth date.
    Birthdate,
}

impl AutoFill {
    /// Parses React Native's `autoComplete` values and iOS's
    /// `textContentType` names.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "off" | "none" => Self::Off,
            "username" | "username-new" => Self::Username,
            "password" | "current-password" => Self::Password,
            "new-password" | "newPassword" | "password-new" => Self::NewPassword,
            "one-time-code" | "oneTimeCode" | "sms-otp" => Self::OneTimeCode,
            "email" | "emailAddress" => Self::Email,
            "tel" | "telephoneNumber" | "tel-national" | "tel-device" | "tel-country-code" => {
                Self::Tel
            }
            "name" | "name-full" => Self::Name,
            "given-name" | "givenName" | "name-given" => Self::GivenName,
            "family-name" | "familyName" | "name-family" => Self::FamilyName,
            "additional-name" | "middleName" | "name-middle" => Self::MiddleName,
            "nickname" => Self::Nickname,
            "organization" | "organizationName" => Self::Organization,
            "street-address" | "fullStreetAddress" | "postal-address" => Self::StreetAddress,
            "address-line1" | "streetAddressLine1" => Self::AddressLine1,
            "address-line2" | "streetAddressLine2" => Self::AddressLine2,
            "address-level2" | "addressCity" | "postal-address-locality" => Self::City,
            "address-level1" | "addressState" | "postal-address-region" => Self::Region,
            "postal-code" | "postalCode" => Self::PostalCode,
            "country" | "countryName" | "postal-address-country" => Self::Country,
            "cc-number" | "creditCardNumber" => Self::CreditCardNumber,
            "cc-exp" | "creditCardExpiration" | "cc-exp-month" | "cc-exp-year" => {
                Self::CreditCardExpiration
            }
            "cc-csc" | "creditCardSecurityCode" => Self::CreditCardSecurityCode,
            "cc-name" | "creditCardName" => Self::CreditCardName,
            "url" | "URL" => Self::Url,
            "birthdate-full" | "birthdate" | "birthdate-day" | "birthdate-month"
            | "birthdate-year" => Self::Birthdate,
            _ => return None,
        })
    }
}

/// The phase of a [`TouchEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    /// Fingers went down (the changed points).
    Start,
    /// Fingers moved.
    Move,
    /// Fingers lifted (the changed points).
    End,
    /// The system or a native gesture took the touches over.
    Cancel,
}

/// One finger of a [`TouchEvent`], in points.
#[derive(Debug, Clone, PartialEq)]
pub struct TouchPoint {
    /// Identifies the finger while it is down.
    pub id: u32,
    /// The view under the finger when it went down.
    pub target: NodeId,
    /// The views around `target` (it included) that listen to touches,
    /// innermost first.
    pub path: Vec<NodeId>,
    /// Position in the surface.
    pub page: Point<Points>,
    /// Position in `target`.
    pub location: Point<Points>,
    /// This finger is one the event is about (it went down, moved or
    /// lifted).
    pub changed: bool,
}

/// Where the mouse is over a hovered view ([`EventKind::HoverMove`]), in
/// points.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HoverEvent {
    /// Position in the view, its transforms undone.
    pub location: Point<Points>,
    /// Position in the root view the view is in (the surface, or its
    /// modal), as touches' `page`.
    pub page: Point<Points>,
    /// The modifier keys held.
    pub modifiers: crate::keymap::Modifiers,
}

/// Where a context menu was asked for ([`EventKind::ContextMenu`]), in
/// points.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ContextMenuEvent {
    /// Position in the view, its transforms undone.
    pub location: Point<Points>,
    /// Position in the root view the view is in (the surface, or its
    /// modal): where `Menu.popup` opens a menu.
    pub page: Point<Points>,
    /// The modifier keys held.
    pub modifiers: crate::keymap::Modifiers,
}

/// Touches for the JS responder system.
#[derive(Debug, Clone, PartialEq)]
pub struct TouchEvent {
    /// What happened.
    pub phase: TouchPhase,
    /// Milliseconds since an arbitrary origin.
    pub timestamp: f64,
    /// The fingers down during the event (with those that lifted).
    pub points: Vec<TouchPoint>,
    /// A view around the fingers whose `panActivation` the platform
    /// activated with this event: it becomes the responder.
    pub activated: Option<NodeId>,
}

/// A set of [`EventKind`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventMask(u64);

impl std::ops::BitOr for EventMask {
    type Output = EventMask;

    #[inline]
    fn bitor(self, other: EventMask) -> EventMask {
        EventMask(self.0 | other.0)
    }
}

impl EventMask {
    /// Returns true if `event` is in the set.
    #[inline]
    pub const fn contains(self, event: EventKind) -> bool {
        self.0 & (1 << event as u8) != 0
    }

    /// The raw bit set, indexed by `EventKind as u8`.
    #[inline]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Adds or removes `event`.
    #[inline]
    pub fn set(&mut self, event: EventKind, enabled: bool) {
        if enabled {
            self.0 |= 1 << event as u8;
        } else {
            self.0 &= !(1 << event as u8);
        }
    }

    /// Returns true if no event is set.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A value in an event payload.
#[derive(Debug, Clone, PartialEq)]
pub enum PayloadValue {
    /// A number.
    Number(f64),
    /// A boolean.
    Bool(bool),
    /// A string.
    Str(String),
    /// `null`.
    Null,
}

/// Data delivered with an event.
#[derive(Debug, Clone, PartialEq)]
pub enum EventPayload {
    /// No data.
    None,
    /// A single value handed to the handler as is (`onValueChange(value)`).
    Value(PayloadValue),
    /// An object with named fields.
    Map(Vec<(&'static str, PayloadValue)>),
    /// A frame relative to the parent (`onLayout`).
    Layout(Bounds<Points>),
    /// A drag: translation since the start and velocity (points / second).
    Pan {
        /// The translation.
        translation: Point<Points>,
        /// Horizontal velocity.
        velocity_x: f32,
        /// Vertical velocity.
        velocity_y: f32,
        /// The `pan.x` value now (`None` without one).
        value_x: Option<Points>,
        /// The `pan.y` value now.
        value_y: Option<Points>,
    },
    /// A scroll position: the content offset.
    Scroll(Point<Points>),
    /// A loaded image's source size in pixels (before downsampling).
    Image(Size<DevicePixels>),
    /// An error message.
    Error {
        /// The message.
        message: String,
    },
    /// The text of an input after an edit.
    Text {
        /// The whole text.
        text: String,
        /// Number of edits the input has reported so far. A value set by JS
        /// based on an older edit is stale and must not overwrite newer
        /// typing.
        count: u32,
    },
    /// Touches (`onTouch`).
    Touch(Box<TouchEvent>),
    /// The mouse over a hovered view (`onHoverMove`).
    Hover(HoverEvent),
    /// A context menu asked for over a view (`onContextMenu`).
    ContextMenu(ContextMenuEvent),
    /// Local file URLs dropped by the user, in the platform's order.
    FileDrop(Vec<String>),
    /// The lines of a text block.
    TextLayout(Vec<crate::text::TextLine>),
    /// An event of a registered native component: its name and data (any
    /// JSON value), handed to JS as `{ name, data }`.
    Native {
        /// The event (`regionChange`).
        name: Arc<str>,
        /// Its data.
        data: crate::JsonValue,
    },
}

impl TouchEvent {
    /// Folds a later move into this pending one: the later positions,
    /// changed if either moved them, and either's activated pan.
    pub fn merge_move(&mut self, later: TouchEvent) {
        let mut later = later;
        for p in &mut later.points {
            if let Some(old) = self.points.iter().find(|o| o.id == p.id) {
                p.changed |= old.changed;
            }
        }
        later.activated = later.activated.or(self.activated);
        *self = later;
    }
}

/// How an image fits its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum ResizeMode {
    /// Fill the box, cropping (default).
    #[default]
    Cover,
    /// Fit inside the box, letterboxing.
    Contain,
    /// Stretch to the box.
    Stretch,
    /// Center at natural size.
    Center,
}

/// An image source.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSource {
    /// `https://…`, `asset:/path` (bundled file) or a file path.
    pub uri: Arc<str>,
    /// Natural size in points, when known up front (bundled assets): used
    /// as the intrinsic layout size.
    pub size: Option<Size<Points>>,
}

/// Keyboard layout of an input (`keyboardType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum KeyboardType {
    /// Full keyboard.
    #[default]
    Default,
    /// Digits only (`number-pad`): no sign, no decimal separator.
    #[prop(rename = "number-pad")]
    Number,
    /// Digits and a decimal separator (`decimal-pad`).
    #[prop(rename = "decimal-pad")]
    Decimal,
    /// Numbers (`numeric`, React Native's): iOS's decimal pad, Android's
    /// number keyboard with a sign and a decimal separator.
    Numeric,
    /// Digits, a sign and punctuation (`numbers-and-punctuation`): iOS's
    /// numbers and punctuation keyboard, Android's signed decimal number
    /// keyboard (the closest it has).
    #[prop(rename = "numbers-and-punctuation")]
    NumbersAndPunctuation,
    /// E-mail addresses (`email-address`).
    #[prop(rename = "email-address")]
    Email,
    /// Phone numbers (`phone-pad`).
    #[prop(rename = "phone-pad")]
    Phone,
    /// URLs (`url`).
    Url,
}

/// The keyboard's look (`keyboardAppearance`, iOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum KeyboardAppearance {
    /// The system's: follows the app's appearance (default).
    #[default]
    Default,
    /// Light.
    Light,
    /// Dark.
    Dark,
}

/// Label of the return key (`returnKeyType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum ReturnKeyType {
    /// Return / new line.
    #[default]
    Default,
    /// Done.
    Done,
    /// Go.
    Go,
    /// Next.
    Next,
    /// Search.
    Search,
    /// Send.
    Send,
}

/// What return does in an input (`submitBehavior`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum SubmitBehavior {
    /// Reports `onSubmit` and keeps the focus.
    Submit,
    /// Reports `onSubmit` and unfocuses the input.
    #[default]
    BlurAndSubmit,
    /// Inserts a line break (multiline inputs; a single line submits).
    Newline,
}

/// Automatic capitalization (`autoCapitalize`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum AutoCapitalize {
    /// Never.
    None,
    /// First letter of each sentence (default).
    #[default]
    Sentences,
    /// First letter of each word.
    Words,
    /// Every letter.
    Characters,
}

/// When a scroll view dismisses the keyboard (`keyboardDismissMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum KeyboardDismissMode {
    /// Never (default).
    #[default]
    None,
    /// As soon as a drag starts.
    #[prop(rename = "on-drag")]
    OnDrag,
    /// Follows the finger while dragging down (iOS).
    Interactive,
}

/// Which point of a page a snapping scroll view aligns to its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum SnapAlignment {
    /// The snap point at the viewport's start.
    #[default]
    Start,
    /// At its center.
    Center,
    /// At its end.
    End,
}

/// Whether a tap in a scroll view dismisses the keyboard
/// (`keyboardShouldPersistTaps`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardTaps {
    /// The tap dismisses the keyboard and reaches nothing else (default).
    #[default]
    Never,
    /// Taps never dismiss the keyboard.
    Always,
    /// Taps that nothing handles dismiss the keyboard.
    Handled,
}

/// How an `<image>` is shown, besides its source.
#[derive(Debug, Clone, PartialEq, HostProps)]
pub enum ImageProp {
    /// `defaultSource`: shown until `src` has loaded (best a local asset).
    #[prop(js = "defaultSource", on = IMAGE)]
    DefaultSource(Option<ImageSource>),
    /// `blurRadius`: blurs the image by this radius in points (decoded once, off
    /// the main thread).
    #[prop(js = "blurRadius", on = IMAGE)]
    BlurRadius(Points),
    /// `fadeDuration`: how long a loaded image fades in, in milliseconds
    /// (default: the platform's, 0 on iOS and 300 on Android, like React
    /// Native).
    #[prop(js = "fadeDuration", min = 0, on = IMAGE)]
    FadeDuration(Option<f32>),
}

/// A behavior property of a scroll view or list.
#[derive(Debug, Clone, PartialEq, HostProps)]
pub enum ScrollProp {
    /// `pagingEnabled`: stops at multiples of the viewport.
    #[prop(js = "pagingEnabled", on = SCROLLING)]
    Paging(bool),
    /// `snapToInterval`: stops at multiples of this length (0: off).
    #[prop(js = "snapToInterval", on = SCROLLING)]
    SnapInterval(Points),
    /// `snapToOffsets`: stops at these offsets.
    #[prop(js = "snapToOffsets", on = SCROLLING)]
    SnapOffsets(Vec<Points>),
    /// `snapToAlignment`: which point of an item aligns with the viewport with
    /// `snapToInterval`.
    #[prop(js = "snapToAlignment", on = SCROLLING)]
    SnapAlignment(SnapAlignment),
    /// `decelerationRate`: how fast a fling slows down: `"normal"`, `"fast"`, or
    /// the velocity kept per millisecond (0.998 / 0.99; 0 is the platform's
    /// default).
    #[prop(js = "decelerationRate", with = crate::js::host_values::deceleration, reset = ScrollProp::Deceleration(0.0), ts = "\"normal\" | \"fast\" | number", on = SCROLLING)]
    Deceleration(f32),
    /// `bounces`: overscroll past the edges (default true).
    #[prop(js = "bounces", default = true, on = SCROLLING)]
    Bounces(bool),
    /// `contentInset`: room around the content.
    #[prop(js = "contentInset", on = SCROLLING)]
    ContentInset(EdgeInsets),
    /// `contentOffset`: where it starts.
    #[prop(js = "contentOffset", with = crate::js::host_values::content_offset, once, ts = "{ x?: number; y?: number }", on = SCROLLING)]
    ContentOffset(Point<Points>),
    /// `keyboardShouldPersistTaps`: whether a tap dismisses the keyboard: `"never"`
    /// (default: the tap only dismisses it), `"always"` (never dismisses),
    /// `"handled"` (unless something handles the tap).
    #[prop(js = "keyboardShouldPersistTaps", with = crate::js::host_values::keyboard_taps, reset = ScrollProp::KeyboardTaps(KeyboardTaps::Never), ts = "\"never\" | \"always\" | \"handled\" | boolean", on = SCROLLING)]
    KeyboardTaps(KeyboardTaps),
    /// `scrollEventThrottle`: minimum milliseconds between `onScroll` events (at
    /// most one per frame anyway).
    #[prop(js = "scrollEventThrottle", on = SCROLLING)]
    EventThrottle(f32),
    /// `disableIntervalMomentum`: a fling moves one snap point at most.
    #[prop(js = "disableIntervalMomentum", on = SCROLLING)]
    DisableIntervalMomentum(bool),
    /// `minimumZoomScale`: the smallest zoom (default 1).
    #[prop(js = "minimumZoomScale", default = 1.0, on = SCROLL)]
    MinZoom(f32),
    /// `maximumZoomScale`: pinching zooms the content when it is above
    /// `minimumZoomScale` (default 1 both); zoomed in, it scrolls both ways.
    #[prop(js = "maximumZoomScale", default = 1.0, on = SCROLL)]
    MaxZoom(f32),
    /// `zoomScale`: zooms to this scale when it changes (the user's zoom is kept
    /// otherwise).
    #[prop(js = "zoomScale", default = 1.0, on = SCROLL)]
    Zoom(f32),
    /// `bouncesZoom`: pinching may go past the limits and springs back (default
    /// true).
    #[prop(js = "bouncesZoom", default = true, on = SCROLL)]
    BouncesZoom(bool),
    /// `pinchGestureEnabled`: the user can pinch to zoom (default true).
    #[prop(js = "pinchGestureEnabled", default = true, on = SCROLL)]
    Pinch(bool),
    /// `centerContent`: content smaller than the viewport (zoomed out) is centered.
    #[prop(js = "centerContent", on = SCROLL)]
    CenterContent(bool),
}

/// What a list cell holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellSlot {
    /// A recycled item cell in this slot.
    Item(u32),
    /// `header`.
    Header,
    /// `footer`.
    Footer,
    /// `empty` (shown while there are no items).
    Empty,
}

/// A list's `contentStyle` (or `contentClass`): padding around the content and gaps
/// between rows and columns.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ListContentStyle {
    /// Padding around the content.
    pub padding: EdgeInsets,
    /// Gap between rows (vertical lists) / columns (horizontal lists).
    pub row_gap: Points,
    /// Gap between columns (`numColumns`) of vertical lists.
    pub column_gap: Points,
}

/// When list items count as viewable (React Native's `viewabilityConfig`).
/// Without thresholds, any visible pixel does.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Viewability {
    /// Percent of the viewport an item must cover.
    pub view_area_percent: Option<f32>,
    /// Percent of an item that must be visible.
    pub item_visible_percent: Option<f32>,
}

/// Where a header item goes (`<headeritem slot>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum HeaderSlot {
    /// Leading, after the back button.
    #[default]
    Left,
    /// Trailing.
    Right,
    /// Replaces the title.
    Title,
    /// Replaces the whole bar (`header`), where the platform draws it.
    Header,
}

/// How a screen enters and leaves (`animation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum ScreenAnimation {
    /// The platform's push / pop transition (`"slide"` too: a
    /// dialog's default, which shares the prop).
    #[default]
    #[prop(alias = "slide")]
    Default,
    /// Cross-fade.
    Fade,
    /// No animation.
    None,
}

/// A screen option (`<NavPage title headerShown ...>`).
#[derive(Debug, Clone, PartialEq, HostProps)]
pub enum ScreenProp {
    /// `title`: the header's title (iOS and Android: the system header;
    /// ignored on the desktops and the web, which show the app's `header`).
    #[prop(js = "title", on = SCREEN)]
    Title(Option<Arc<str>>),
    /// `headerShown`: the system header shows (default true; iOS and
    /// Android).
    #[prop(js = "headerShown", default = true, on = SCREEN)]
    HeaderShown(bool),
    /// `headerLargeTitle` (iOS: the system header; ignored elsewhere): a
    /// large title; the content scrolls under the header.
    #[prop(js = "headerLargeTitle", on = SCREEN)]
    HeaderLargeTitle(bool),
    /// `headerTransparent`: the content extends under the system header
    /// (iOS and Android).
    #[prop(js = "headerTransparent", on = SCREEN)]
    HeaderTransparent(bool),
    /// `headerBackTitle` (iOS: the system header; ignored elsewhere): the
    /// label of the back button leading back to this page.
    #[prop(js = "headerBackTitle", on = SCREEN)]
    HeaderBackTitle(Option<Arc<str>>),
    /// `headerBackVisible`: the system header's back button shows (default
    /// true; iOS and Android).
    #[prop(js = "headerBackVisible", default = true, on = SCREEN)]
    HeaderBackVisible(bool),
    /// `popOnSwipe`: the interactive back gesture pops (iOS swipe back,
    /// Android predictive back following the finger); the back buttons pop
    /// either way (default true).
    #[prop(js = "popOnSwipe", default = true, on = SCREEN)]
    PopOnSwipe(bool),
    /// `popOnBack`: the user leaves the page with the back buttons (header,
    /// Android back, the desktops' back keys and mouse button, the
    /// browser's back) and the swipe back (default true). False keeps it,
    /// e.g. to confirm discarding changes in `onBackPress` (called on every
    /// back press) and then `pop()`. Decided natively when the system asks,
    /// so set it before the user acts.
    #[prop(js = "popOnBack", default = true, on = SCREEN)]
    PopOnBack(bool),
    /// `animation`: how the page enters and leaves.
    #[prop(js = "animation", on = SCREEN)]
    Animation(ScreenAnimation),
    /// `statusBar`: what the status bar shows while the page or dialog is
    /// up (iOS and Android; ignored on the desktops and the web), as a view
    /// controller's `preferredStatusBarStyle` decides it on iOS. The most
    /// recently shown page or dialog wins, field by field, over the pages
    /// below it and over the app's default (`nsh.json` `statusBar`). Pages
    /// and dialogs share it.
    #[prop(js = "statusBar", on = SCREEN | MODAL)]
    StatusBar(Option<StatusBarSetting>),
}

/// How a modal appears (`animation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum ModalAnimation {
    /// Slides up from the bottom (default).
    #[default]
    Slide,
    /// Fades in.
    Fade,
    /// Appears at once.
    None,
}

/// How a modal covers the screen (`presentation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum ModalPresentation {
    /// Covers the whole screen (default).
    #[default]
    FullScreen,
    /// A sheet that leaves the presenting content visible behind it.
    PageSheet,
    /// A smaller centered sheet on large screens.
    FormSheet,
}

/// A modal option (`<Dialog …>`).
#[derive(Debug, Clone, Copy, PartialEq, HostProps)]
pub enum ModalProp {
    /// `animation`: how the dialog appears. Pages and dialogs share the
    /// name, so JS's value arrives as [`ScreenProp::Animation`], which the
    /// core takes for this on a dialog.
    Animation(ModalAnimation),
    /// `presentation`: `"pageSheet"` / `"formSheet"` keep what is behind
    /// visible (iOS; ignored on Android, the desktops and the web, where a
    /// dialog is a transparent layer over the window and the app's popup
    /// and backdrop are all it shows).
    #[prop(js = "presentation", on = MODAL)]
    Presentation(ModalPresentation),
    /// `statusBarTranslucent` (Android): the content reaches under the status bar
    /// (default: it starts below).
    #[prop(android = "statusBarTranslucent", on = MODAL)]
    StatusBarTranslucent(bool),
    /// `supportedOrientations` (iOS): the orientations it allows (default: the
    /// presenter's).
    ///
    /// [`crate::ModalConfig::PORTRAIT`] bits.
    #[prop(ios = "supportedOrientations", with = crate::js::host_values::orientations, reset = ModalProp::Orientations(0), ts = "(\"portrait\" | \"portrait-upside-down\" | \"landscape\" | \"landscape-left\" | \"landscape-right\")[]", on = MODAL)]
    Orientations(u8),
    /// `closeOnEscape`: Escape and the Android back button close it natively
    /// (default true); either way they call `onCancel`.
    #[prop(js = "closeOnEscape", default = true, on = MODAL)]
    CloseOnEscape(bool),
    /// `closeOnBackdropPress`: a press on the backdrop part
    /// (`backdropClass` / `backdropStyle`, rendered by the runtime) closes
    /// it (default true); either way it calls `onCancel`.
    #[prop(js = "closeOnBackdropPress", default = true, on = MODAL)]
    CloseOnBackdropPress(bool),
    /// `closeOnSwipe`: swiping a sheet down (iOS, phone browsers) closes it
    /// natively (default true); either way it calls `onCancel`.
    #[prop(js = "closeOnSwipe", default = true, on = MODAL)]
    CloseOnSwipe(bool),
}

/// An imperative call on a native view (`ref.focus()` and the like).
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Focus an input (show the keyboard).
    Focus,
    /// Unfocus an input; on the root view, unfocus whatever is focused.
    Blur,
    /// Scroll a scroll view to an offset.
    ScrollTo {
        /// Horizontal offset.
        x: Points,
        /// Vertical offset.
        y: Points,
        /// Animate the change.
        animated: bool,
    },
    /// Animate the layout changes of the next commit (on the root).
    LayoutAnimation(LayoutAnimation),
    /// Leave the app (Android back handling; on the root).
    ExitApp,
    /// What the back button does when none of the app's listeners took
    /// it: the innermost stack pops (or asks, with `popOnBack` false); with
    /// none, the app leaves on Android (on the root).
    Back,
    /// Reload a web view.
    Reload,
    /// Go back in a web view's history.
    GoBack,
    /// Go forward in a web view's history.
    GoForward,
    /// Stop loading a web view.
    StopLoading,
    /// Run JavaScript in a web view's page.
    InjectJavaScript(String),
    /// Send a message to a web view's page (`window.onmessage` data).
    PostMessage(String),
    /// Seek a video to a time in seconds.
    Seek(f64),
    /// Scroll a list to an item (resolved by the core into `ScrollTo`).
    ScrollToIndex {
        /// The item.
        index: u32,
        /// Animate the change.
        animated: bool,
        /// Where the item ends up: 0 at the start of the viewport, 1 at the
        /// end, 0.5 in the middle.
        view_position: Percentage,
        /// Extra distance from that position.
        view_offset: Points,
    },
    /// Zoom a zoomable scroll view to `scale`, with the content point
    /// (`x`, `y`) (points, unzoomed) in the middle of the viewport, or
    /// around the viewport's middle.
    ZoomTo {
        /// The zoom scale (clamped to the view's limits).
        scale: f32,
        /// The content point to center.
        center: Option<Point<Points>>,
        /// Animate the change.
        animated: bool,
    },
    /// Scroll a list to the end of its content.
    ScrollToEnd {
        /// Animate the change.
        animated: bool,
    },
    /// Set the status bar (on the root): what the platform shows.
    StatusBar(StatusBarConfig),
    /// Change what functions set for the status bar (on the root): the
    /// core merges it under the settings components render and sends
    /// [`Command::StatusBar`] when the result changes.
    SetStatusBar(StatusBarSetting),
    /// Show an alert or action sheet (on the root); the answer comes back
    /// as [`EventKind::Dialog`].
    Dialog(Box<Dialog>),
    /// A JS responder took the touches on this view: `block` keeps native
    /// scroll views around it from taking them over.
    SetJsResponder {
        /// Block native scrolling.
        block: bool,
    },
    /// The JS responder let go (on the root).
    ClearJsResponder,
    /// Select a range of an input (UTF-16 units; equal for a caret).
    SetSelection {
        /// Start.
        start: u32,
        /// End.
        end: u32,
    },
    /// Show a short message (Android toast; on the root).
    Toast {
        /// The message.
        text: String,
        /// Show it longer.
        long: bool,
    },
    /// Have the screen reader read a message (on the root).
    Announce(String),
    /// Load the image at `uri` (keeping its bytes for views that show it
    /// next) and report its pixel size as [`EventKind::ImageInfo`] `id`
    /// (on the root).
    ImageInfo {
        /// The request.
        id: u32,
        /// URL or `asset:/` URI.
        uri: String,
    },
    /// Move the screen reader's focus to this view.
    AccessibilityFocus,
    /// Measure the view with this commit's layout (`ref.measure()`):
    /// answered by the core in its events for promise `call`
    /// ([`crate::ui::JsEvent::Reply`]: a [`crate::Measurement`], or null
    /// when the view is not laid out); never mounted.
    Measure {
        /// The call to answer.
        call: u32,
    },
    /// Put `text` on the system clipboard (on the root; `Clipboard.setString`).
    SetClipboard(String),
    /// Read the system clipboard and report it as [`EventKind::Clipboard`]
    /// `id` (on the root): `has` and, unless `has_only`, `text`. Asking
    /// only whether it holds text does not read it (no iOS paste notice).
    ReadClipboard {
        /// The request.
        id: u32,
        /// Only whether it holds text (`Clipboard.hasString`).
        has_only: bool,
    },
    /// Put the image of a `data:` URI on the system clipboard, replacing
    /// what it holds (on the root; `Clipboard.setImage`).
    SetClipboardImage(String),
    /// Read the image on the system clipboard and report it as
    /// [`EventKind::Clipboard`] `id` (on the root): `has` and, unless
    /// `has_only`, the image as a `data:` URI (`uri`) and its size in
    /// pixels (`width`, `height`).
    ReadClipboardImage {
        /// The request.
        id: u32,
        /// Only whether it holds an image (`Clipboard.hasImage`).
        has_only: bool,
    },
    /// Read the local files on the system clipboard (files copied in a
    /// file manager) and report them as [`EventKind::Clipboard`] `id` (on
    /// the root): `has` and `files`, their `file:` URLs one per line.
    ReadClipboardFiles {
        /// The request.
        id: u32,
    },
    /// A desktop command (on the root; `nsh/desktop`): the window, its
    /// title bar, menus. Mobile platforms ignore it.
    Desktop(Box<crate::desktop::DesktopCommand>),
    /// A call on a registered native component (`ref.command(name, args)`).
    Native {
        /// The command.
        name: Arc<str>,
        /// Its arguments (any JSON value).
        args: crate::JsonValue,
    },
}

/// The status bar's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusBarStyle {
    /// The platform's default for the appearance.
    #[default]
    Default,
    /// Light content (for dark backgrounds).
    Light,
    /// Dark content (for light backgrounds).
    Dark,
}

impl StatusBarStyle {
    /// The style of a TS `StatusBarForeground` (`"auto"`, `"light"`,
    /// `"dark"`).
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "auto" => Some(Self::Default),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

/// What the status bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatusBarConfig {
    /// Content color.
    pub style: StatusBarStyle,
    /// Hidden.
    pub hidden: bool,
    /// Animate the change.
    pub animated: bool,
}

/// A status bar setting: what it sets; the fields it leaves out keep the
/// settings below it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatusBarSetting {
    /// Content color.
    pub style: Option<StatusBarStyle>,
    /// Hidden.
    pub hidden: Option<bool>,
    /// Animate the change.
    pub animated: Option<bool>,
}

impl StatusBarSetting {
    /// Sets the content's color.
    pub const fn foreground(mut self, style: StatusBarStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Hides or shows the bar.
    pub const fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = Some(hidden);
        self
    }

    /// Animates the change to this setting.
    pub const fn animated(mut self, animated: bool) -> Self {
        self.animated = Some(animated);
        self
    }

    /// The app's default (`nsh.json` `"statusBar"`): `{ "foreground":
    /// "auto" | "light" | "dark", "hidden": bool }`, both optional. The
    /// error says what is wrong, for `nsh.json: statusBar: …`.
    pub fn from_json(value: &crate::JsonValue) -> Result<Self, String> {
        let Some(object) = value.as_object() else {
            return Err(format!(
                "expected an object ({{ \"foreground\": \"light\", \"hidden\": false }}), got {value}"
            ));
        };
        let mut setting = Self::default();
        for (key, v) in object {
            match key.as_str() {
                "foreground" => {
                    let style = v.as_str().and_then(StatusBarStyle::from_name);
                    setting.style = Some(style.ok_or_else(|| {
                        format!("foreground: {v} is not \"auto\", \"light\" or \"dark\"")
                    })?);
                }
                "hidden" => {
                    setting.hidden = Some(
                        v.as_bool()
                            .ok_or_else(|| format!("hidden: {v} is not true or false"))?,
                    );
                }
                other => {
                    return Err(format!(
                        "unknown key {other:?} (\"foreground\" and \"hidden\" are known)"
                    ));
                }
            }
        }
        Ok(setting)
    }

    /// This setting over `below`.
    pub fn over(self, below: Self) -> Self {
        Self {
            style: self.style.or(below.style),
            hidden: self.hidden.or(below.hidden),
            animated: self.animated.or(below.animated),
        }
    }
}

/// How a dialog button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogButtonStyle {
    /// A regular button.
    #[default]
    Default,
    /// The cancel button.
    Cancel,
    /// A destructive action.
    Destructive,
}

/// A dialog button.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DialogButton {
    /// Label.
    pub text: String,
    /// Look.
    pub style: DialogButtonStyle,
}

/// An alert, or with `sheet` an action sheet.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Dialog {
    /// Echoed in the answer.
    pub id: u32,
    /// Title.
    pub title: Option<String>,
    /// Message.
    pub message: Option<String>,
    /// Buttons, or the sheet's options.
    pub buttons: Vec<DialogButton>,
    /// Tapping outside (or the back button) dismisses it.
    pub cancelable: bool,
    /// An action sheet rather than an alert.
    pub sheet: bool,
}

/// A non-style property of a host node.
#[derive(Debug, Clone, PartialEq, HostProps)]
pub enum Prop {
    /// `component`: which registered native component a `<native>`
    /// element is (set once, before it is mounted).
    #[prop(js = "component", internal)]
    Component(Arc<str>),
    /// `componentSlot`: this view holds the JSX a Rust component's host got
    /// for slot `name` (created by the generated glue).
    #[prop(js = "componentSlot", internal)]
    ComponentSlot(Arc<str>),
    /// A property of a registered native component (`$name` from JS):
    /// any JSON value, `None` to remove it.
    ComponentProp(Arc<str>, Option<crate::JsonValue>),
    /// A prop of a Rust component's host, decoded in the JS call into the
    /// prop's Rust type ([`crate::ui::PropSpec::decode`]); `None` resets it
    /// to its default.
    RustProp(&'static str, Option<crate::ui::AnyValue>),
    /// `src`: a URL, `asset:/path`, or an imported asset.
    #[prop(js = "src", on = IMAGE)]
    Source(Option<ImageSource>),
    /// `resizeMode`: how an image (or a video) fits its box.
    #[prop(js = "resizeMode", on = IMAGE | VIDEO)]
    ResizeMode(ResizeMode),
    /// `horizontal`: scrolls sideways.
    #[prop(js = "horizontal", on = SCROLL | LIST)]
    Horizontal(bool),
    /// Scrolling behavior of a scroll view or list.
    #[prop(nested)]
    Scroll(ScrollProp),
    /// `showsIndicator`: the scroll indicator shows (default true). On the
    /// desktop a horizontal view keeps it: it is the mouse's scroll bar.
    #[prop(js = "showsIndicator", default = true, on = SCROLL | LIST)]
    ShowsIndicator(bool),
    /// `refreshing`: the pull-to-refresh spinner shows.
    #[prop(js = "refreshing", on = SCROLL | LIST)]
    Refreshing(bool),
    /// Number of items of a list.
    #[prop(js = "count", internal)]
    Count(u32),
    /// `estimatedItemSize`: the size of items not measured yet, in points (default
    /// 50).
    #[prop(js = "estimatedItemSize", default = Points(50.0), on = LIST)]
    EstimatedItemSize(Points),
    /// The recycling slot (or the role) of a list cell, set once by the
    /// runtime.
    #[prop(js = "slot", with = crate::js::host_values::slot, internal)]
    Slot(CellSlot),
    /// `numColumns`: items per row (vertical lists).
    #[prop(js = "numColumns", default = 1, min = 1, on = LIST)]
    Columns(u32),
    /// The items of a list changed: `removed` at `at` became `inserted`
    /// (precedes the new `Count`).
    #[prop(js = "splice", with = crate::js::host_values::splice, internal)]
    Splice(u32, u32, u32),
    /// `inverted`: laid out from the end, item 0 last (chat style); starts scrolled
    /// to the end and keeps its distance from the end as content changes.
    #[prop(js = "inverted", on = LIST)]
    Inverted(bool),
    /// `maintainVisibleContentPosition`: with `autoscrollToBottomThreshold`, a list
    /// (not inverted) at the end of its content, or within that many points of it,
    /// stays at the end as the content grows or the viewport shrinks (a reply
    /// streaming into its last item); scrolled further up, it keeps the visible
    /// content where it is. Items inserted above the visible ones never move them.
    #[prop(js = "maintainVisibleContentPosition", with = crate::js::host_values::visible_content, reset = Prop::FollowEnd(None), ts = "{ autoscrollToBottomThreshold?: number }", on = LIST)]
    FollowEnd(Option<Points>),
    /// `stickyHeaderIndices`: items that stick to the start while scrolled past
    /// (one column, not inverted).
    #[prop(js = "stickyHeaderIndices", with = crate::js::host_values::sticky, reset = Prop::StickyIndices(Vec::new()), on = LIST)]
    StickyIndices(Vec<u32>),
    /// `contentStyle`: padding and gaps of the content.
    #[prop(js = "contentStyle", on = LIST)]
    ListContent(ListContentStyle),
    /// `contentClass`: padding and gaps of the content as classes (`p-*`,
    /// `gap-*`); `contentStyle` wins over them (set by the runtime).
    ListContentClass(Box<str>),
    /// `viewabilityConfig`: when items count as viewable.
    #[prop(js = "viewabilityConfig", on = LIST)]
    Viewability(Viewability),
    /// How close to the end `onEndReached` fires, in viewports.
    #[prop(js = "endReachedThreshold", default = 1.0, internal)]
    EndReachedThreshold(f32),
    /// The item index a list cell now renders (set by the runtime after
    /// each bind, in the same batch as the cell's new content).
    #[prop(js = "cellIndex", internal)]
    CellIndex(u32),
    /// `scrollValue`: an animated value fed with the scroll offset (main axis), for
    /// scroll-linked effects.
    #[prop(js = "scrollValue", with = crate::js::host_values::scroll_value, reset = Prop::ScrollValue(None), ts = "AnimatedValue", on = SCROLL | LIST)]
    ScrollValue(Option<crate::animation::NodeId>),
    /// `pan`: drags drive these animated values (points) on the UI thread.
    #[prop(js = "pan", with = crate::js::host_values::pan, reset = Prop::PanValues(None, None), ts = "{ x?: AnimatedValue; y?: AnimatedValue }")]
    PanValues(
        Option<crate::animation::NodeId>,
        Option<crate::animation::NodeId>,
    ),
    /// `numberOfLines`: the most lines shown; 0 (default) is unlimited.
    #[prop(js = "numberOfLines", on = TEXT)]
    NumberOfLines(u32),
    /// `ellipsizeMode`: where text that does not fit `numberOfLines` is cut.
    #[prop(js = "ellipsizeMode", on = TEXT)]
    EllipsizeMode(EllipsizeMode),
    /// `value`: the text, controlled by JS.
    #[prop(js = "value", on = INPUT)]
    Value(String),
    /// `defaultValue`: the initial text.
    #[prop(js = "defaultValue", once, on = INPUT)]
    DefaultValue(String),
    /// `placeholder`: shown while the input is empty.
    #[prop(js = "placeholder", on = INPUT)]
    Placeholder(String),
    /// The placeholder's color (Rust components; TS sets the
    /// `placeholderColor` style key or `placeholder:text-*`).
    PlaceholderColor(Option<Color>),
    /// `masked`: the text is hidden (passwords).
    #[prop(js = "masked", on = INPUT)]
    Secure(bool),
    /// `keyboardType`: the keyboard layout.
    #[prop(js = "keyboardType", on = INPUT)]
    KeyboardType(KeyboardType),
    /// `keyboardAppearance` (iOS): the keyboard's light or dark look;
    /// Android's keyboards follow the system's theme.
    #[prop(ios = "keyboardAppearance", on = INPUT)]
    KeyboardAppearance(KeyboardAppearance),
    /// `returnKeyType`: the label of the return key.
    #[prop(js = "returnKeyType", on = INPUT)]
    ReturnKeyType(ReturnKeyType),
    /// `autoCapitalize`: automatic capitalization (default `"sentences"`).
    #[prop(js = "autoCapitalize", on = INPUT)]
    AutoCapitalize(AutoCapitalize),
    /// `autoCorrect`: the system's corrections (default true).
    #[prop(js = "autoCorrect", default = true, on = INPUT)]
    AutoCorrect(bool),
    /// `multiline`: several lines; return inserts a line break.
    #[prop(js = "multiline", on = INPUT)]
    Multiline(bool),
    /// `maxLength`: the most characters the user can enter.
    #[prop(js = "maxLength", on = INPUT)]
    MaxLength(Option<u32>),
    /// `readonly`: the user can select and copy the text but not change it.
    #[prop(js = "readonly", on = INPUT)]
    Readonly(bool),
    /// `submitBehavior`: what return does: `"submit"` (reports
    /// `onSubmit`, keeps the focus), `"blurAndSubmit"` (also
    /// unfocuses), `"newline"` (inserts a line break; multiline inputs
    /// only). Default: `"blurAndSubmit"` for one line, `"newline"` for
    /// several. A multiline input that submits still takes line breaks
    /// from Shift+Return or Option / Alt+Return on desktops.
    #[prop(js = "submitBehavior", on = INPUT)]
    SubmitBehavior(Option<SubmitBehavior>),
    /// `autoFocus`: focused when it appears.
    #[prop(js = "autoFocus", on = INPUT)]
    AutoFocus(bool),
    /// `caretHidden`: no caret is drawn.
    #[prop(js = "caretHidden", on = INPUT)]
    CaretHidden(bool),
    /// `inputAccessoryViewID`: the `nativeID` of an `InputAccessoryView`
    /// (`nsh/ios`) shown over the keyboard while the input has focus.
    #[prop(ios = "inputAccessoryViewID", on = INPUT)]
    InputAccessoryViewId(Option<Box<str>>),
    /// `nativeID`: names the view for others (an input accessory's for
    /// `inputAccessoryViewID`).
    #[prop(js = "nativeID")]
    NativeId(Option<Box<str>>),
    /// An input's caret and selection, the highlight of a `selectable`
    /// text's selection (Rust components; TS sets the `selectionColor`
    /// style key or `selection:text-*`).
    SelectionColor(Option<Color>),
    /// `autoComplete` (iOS: also `textContentType`): what an input holds, for
    /// autofill (React Native's values, e.g. "username", "current-password",
    /// "new-password", "one-time-code", "email", "tel", "postal-code", "cc-number",
    /// "off").
    #[prop(js = "autoComplete", ios = "textContentType", on = INPUT)]
    AutoFill(AutoFill),
    /// `selectedRange`: the controlled selection (UTF-16 units; `end` defaults to
    /// `start`, a caret).
    #[prop(js = "selectedRange", with = crate::js::host_values::selection, once, ts = "{ start: number; end?: number }", on = INPUT)]
    Selection(u32, u32),
    /// `keyboardDismissMode`: whether dragging dismisses the keyboard.
    #[prop(js = "keyboardDismissMode", on = SCROLL | LIST)]
    KeyboardDismissMode(KeyboardDismissMode),
    /// `pointerEvents`: touch handling of the view and its children (React Native
    /// semantics).
    #[prop(js = "pointerEvents")]
    PointerEvents(PointerEvents),
    /// `hitSlop`: extends the touch area beyond the bounds.
    #[prop(js = "hitSlop", with = crate::js::host_values::hit_slop, reset = Prop::HitSlop(EdgeInsets::default()), ts = "number | { top?: number; right?: number; bottom?: number; left?: number }")]
    HitSlop(EdgeInsets),
    /// `panActivation`: the view's touch responder waits for a pan that the
    /// platform activates past an offset on one axis (and fails past one on
    /// the other), deciding on the main thread so the scroll views around
    /// it keep the touches until then.
    #[prop(js = "panActivation")]
    PanActivation(Option<crate::props::PanActivation>),
    /// `activeStyle`: the style while pressed (and not disabled), over the
    /// view's style; applied natively, no JS round trip.
    ActiveStyle(Option<Arc<StylePatch>>),
    /// `hoverStyle`: the style while a pointer is over the view (and it is
    /// not disabled).
    HoverStyle(Option<Arc<StylePatch>>),
    /// `focusStyle`: the style while the view has focus (and is not
    /// disabled).
    FocusStyle(Option<Arc<StylePatch>>),
    /// `disabledStyle`: the style while `disabled`, over every other state's.
    DisabledStyle(Option<Arc<StylePatch>>),
    /// `selectedStyle`: the style while `accessibilityState.selected` (a
    /// `Button`'s `selected`), and not disabled.
    SelectedStyle(Option<Arc<StylePatch>>),
    /// `checkedStyle`: the style while `accessibilityState.checked` (true,
    /// not mixed: a checkbox's, a switch's, a radio's), and not disabled.
    CheckedStyle(Option<Arc<StylePatch>>),
    /// `headerStyle` of a page: `backgroundColor`, `color` (the title's) and
    /// `tintColor` (the buttons') of its header.
    HeaderStyle(Option<Arc<StylePatch>>),
    /// `headerClass` of a page: the header's colors as classes (`bg-*`,
    /// `text-*`, `tint-*`); `headerStyle` wins over them.
    HeaderClass(Box<str>),
    /// The surface lays out right to left (on the root, from
    /// `I18nManager`).
    #[prop(js = "layoutDirectionRtl", internal)]
    LayoutDirection(bool),
    /// The app's appearance (on the root, from `nsh.json` and
    /// `setColorScheme`).
    #[prop(js = "appearance", internal)]
    Appearance(crate::platform::Appearance),
    /// The surface's keyboard shortcuts (on the root, from `nsh/keymap`):
    /// the whole table, mounted as [`crate::MountItem::Keymap`].
    Keymap(std::sync::Arc<crate::keymap::Keymap>),
    /// Padding added to the style's own (safe areas, the keyboard), in
    /// points on the physical edges; set by components
    /// (`Element::inset_padding`), never by JS.
    InsetPadding(EdgeInsets),
    /// Margin added to the style's own (`KeyboardAvoidingView`'s
    /// `height` behavior), in points on the physical edges; set by
    /// components.
    InsetMargin(EdgeInsets),
    /// `class`: Tailwind CSS classes (`"flex-row gap-2 p-4 bg-white
    /// dark:bg-zinc-900"`), resolved natively. Later classes win over earlier ones;
    /// `style` wins over both.
    #[prop(js = "class")]
    Class(Box<str>),
    /// `classList`: classes applied while their value is truthy (`{ "opacity-50":
    /// busy() }`).
    #[prop(js = "classList", with = crate::js::host_values::class_list, reset = Prop::ClassList(Vec::new()), ts = "Record<string, boolean | null | undefined>")]
    ClassList(Vec<Box<str>>),
    /// The app's Tailwind theme (JSON, on the root).
    #[prop(js = "tailwindTheme", with = crate::js::host_values::tailwind_theme, internal)]
    TailwindTheme(Box<str>),
    /// The theme of Rust components (`nsh.json` `theme`, JSON), on the
    /// root.
    #[prop(js = "uiTheme", with = crate::js::host_values::ui_theme, internal)]
    UiTheme(Box<str>),
    /// An image option.
    #[prop(nested)]
    Image(ImageProp),
    /// `delayLongPress`: milliseconds before a press becomes a long press (default
    /// 500).
    #[prop(js = "delayLongPress", min = 0)]
    LongPressDelay(Option<f32>),
    /// `ripple` (Android): the material ripple while pressed.
    #[prop(android = "ripple")]
    Ripple(Option<crate::props::Ripple>),
    /// `elevation` (Android): the material shadow (points), from the view's shape.
    #[prop(android = "elevation")]
    Elevation(Points),
    /// An accessibility property.
    #[prop(nested)]
    Accessibility(AccessibilityProp),
    /// `testID`: an identifier for UI tests.
    #[prop(js = "testID")]
    TestId(Option<Arc<str>>),
    /// `windowDrag` (desktops): pressing the view drags the window, as its
    /// title bar would (double-clicking maximizes it); `false` keeps a part
    /// of a draggable view clickable. The nearest view that sets it
    /// decides; text fields and views with press or touch handlers inside
    /// a draggable view stay clickable without it.
    #[prop(desktop = "windowDrag")]
    WindowDrag(Option<bool>),
    /// `focusable`: whether a view that takes presses is a stop of the
    /// keyboard focus (Tab, on the platforms that move it between
    /// pressables); `false` leaves it to the pointer (a backdrop that
    /// closes a popover). Default true.
    #[prop(js = "focusable")]
    Focusable(Option<bool>),
    /// `allowFontScaling`: follow the system text size (default true).
    #[prop(js = "allowFontScaling", default = true, on = TEXT | INPUT)]
    AllowFontScaling(bool),
    /// `maxFontSizeMultiplier`: the upper bound of the system scale (0 is none).
    #[prop(js = "maxFontSizeMultiplier", min = 0, on = TEXT | INPUT)]
    MaxFontSizeMultiplier(f32),
    /// `selectable`: a long press offers to copy the text; `false` also
    /// keeps it from being selected where text is by default (desktop
    /// browsers), unset leaves it to the platform.
    #[prop(js = "selectable", on = TEXT)]
    Selectable(Option<bool>),
    /// `adjustsFontSizeToFit`: shrink the font until the text fits its lines
    /// (`numberOfLines`) and box.
    #[prop(js = "adjustsFontSizeToFit", on = TEXT)]
    AdjustsFontSizeToFit(bool),
    /// `minimumFontScale`: how far `adjustsFontSizeToFit` shrinks at most (0..1).
    #[prop(js = "minimumFontScale", min = 0, max = 1, on = TEXT)]
    MinimumFontScale(f32),
    /// `pinch`: two-finger pinches drive `scale` natively.
    #[prop(js = "pinch", with = crate::js::host_values::pinch, reset = Prop::PinchValue(None), ts = "{ scale?: AnimatedValue }")]
    PinchValue(Option<crate::animation::NodeId>),
    /// `rotation`: two-finger rotations drive `angle` (degrees) natively.
    #[prop(js = "rotation", with = crate::js::host_values::rotation, reset = Prop::RotationValue(None), ts = "{ angle?: AnimatedValue }")]
    RotationValue(Option<crate::animation::NodeId>),
    /// A screen option.
    #[prop(nested)]
    Screen(ScreenProp),
    /// A modal option.
    #[prop(nested)]
    Modal(ModalProp),
    /// `<headeritem slot>`.
    #[prop(js = "headerSlot", internal)]
    HeaderSlot(HeaderSlot),
    /// A property of a native control (web view, video), or `disabled`.
    #[prop(nested)]
    Native(NativeProp),
}

/// Where a web view loads from.
#[derive(Debug, Clone, PartialEq)]
pub enum WebSource {
    /// Nothing.
    None,
    /// A URL.
    Uri(Arc<str>),
    /// HTML, with the URL relative links resolve against.
    Html {
        /// The page.
        html: Arc<str>,
        /// Base URL.
        base_url: Option<Arc<str>>,
    },
}

/// A property of a native control.
#[derive(Debug, Clone, PartialEq, HostProps)]
pub enum NativeProp {
    /// `disabled` (any element): ignores presses (no press feedback either),
    /// applies `disabled:` classes, disables controls, and screen readers
    /// say it is disabled (its accessibility state).
    #[prop(js = "disabled")]
    Disabled(bool),
    /// `source` of a web view or video: a URL, `{ uri }`, or (web views) `{ html,
    /// baseUrl }`.
    #[prop(js = "source", default = WebSource::None, on = WEBVIEW | VIDEO)]
    Source(WebSource),
    /// `injectedJavaScript` (web views): runs after each page loads.
    #[prop(js = "injectedJavaScript", on = WEBVIEW)]
    InjectedJavaScript(Option<Arc<str>>),
    /// `injectedJavaScriptBeforeContentLoaded` (web views): runs before each page's
    /// own scripts.
    #[prop(js = "injectedJavaScriptBeforeContentLoaded", on = WEBVIEW)]
    InjectedJavaScriptBefore(Option<Arc<str>>),
    /// `userAgent` (web views): the browser's user agent string.
    #[prop(js = "userAgent", on = WEBVIEW)]
    UserAgent(Option<Arc<str>>),
    /// `scrollEnabled`: the user can scroll it (default true).
    #[prop(js = "scrollEnabled", default = true, on = WEBVIEW | SCROLLING)]
    ScrollEnabled(bool),
    /// `javaScriptEnabled` (web views): pages run scripts (default true).
    #[prop(js = "javaScriptEnabled", default = true, on = WEBVIEW)]
    JavaScriptEnabled(bool),
    /// `allowsBackForwardNavigationGestures` (iOS web views; desktop ones on
    /// macOS, with two-finger trackpad swipes): swipe back and forward.
    #[prop(
        ios = "allowsBackForwardNavigationGestures",
        desktop = "allowsBackForwardNavigationGestures",
        on = WEBVIEW
    )]
    BackForwardGestures(bool),
    /// `paused` (videos): playback is paused.
    #[prop(js = "paused", on = VIDEO)]
    Paused(bool),
    /// `muted` (videos): no sound.
    #[prop(js = "muted", on = VIDEO)]
    Muted(bool),
    /// `volume` (videos): 0..1.
    #[prop(js = "volume", default = 1.0, on = VIDEO)]
    Volume(f32),
    /// `rate` (videos): the playback speed.
    #[prop(js = "rate", default = 1.0, on = VIDEO)]
    Rate(f32),
    /// `repeat` (videos): plays again from the start at the end.
    #[prop(js = "repeat", on = VIDEO)]
    Repeat(bool),
    /// Video `resizeMode`.
    #[prop(reset = NativeProp::VideoResizeMode(ResizeMode::Contain))]
    VideoResizeMode(ResizeMode),
    /// `controls` (videos): the platform's playback controls.
    #[prop(js = "controls", on = VIDEO)]
    Controls(bool),
    /// `progressUpdateInterval` (videos): milliseconds between `onProgress` events
    /// (250 by default).
    #[prop(js = "progressUpdateInterval", default = 250.0, on = VIDEO)]
    ProgressInterval(f32),
}

/// A mutation produced by the JS renderer.
#[derive(Debug)]
pub enum Op {
    /// What a canvas draws (replaces the previous drawing).
    Draw {
        /// The canvas.
        id: NodeId,
        /// The recorded operations.
        drawing: Box<crate::canvas::Drawing>,
    },
    /// Creates a detached element node.
    Create {
        /// New node id.
        id: NodeId,
        /// Element tag.
        tag: Tag,
    },
    /// Creates a detached raw text node.
    CreateText {
        /// New node id.
        id: NodeId,
        /// Text content.
        text: String,
    },
    /// Replaces the content of a raw text node.
    SetText {
        /// Text node id.
        id: NodeId,
        /// New content.
        text: String,
    },
    /// Replaces the whole style of a node.
    SetStyle {
        /// Node id.
        id: NodeId,
        /// The new style; boxed because layout styles are large.
        style: Box<Style>,
        /// The declarations `style` was built from, applied again over the
        /// node's class styles.
        patch: StylePatch,
    },
    /// Sets a non-style property.
    SetProp {
        /// Node id.
        id: NodeId,
        /// The property.
        prop: Prop,
    },
    /// Enables or disables an event listener on a node.
    Listen {
        /// Node id.
        id: NodeId,
        /// Event kind.
        event: EventKind,
        /// Whether a handler is attached.
        enabled: bool,
    },
    /// Inserts (or moves) `child` into `parent` before `before`, or appends.
    Insert {
        /// Parent node id.
        parent: NodeId,
        /// Child node id.
        child: NodeId,
        /// Sibling to insert before; `None` appends.
        before: Option<NodeId>,
    },
    /// Detaches `child` from `parent`. The child stays alive until released.
    Remove {
        /// Parent node id.
        parent: NodeId,
        /// Child node id.
        child: NodeId,
    },
    /// The JS node was garbage collected: free the node and its native view.
    Release {
        /// Node id.
        id: NodeId,
    },
    /// An animation graph change, forwarded to the platform in order.
    Animation(AnimationOp),
    /// An imperative call on a node's native view.
    Command {
        /// Node id.
        id: NodeId,
        /// The call.
        command: Command,
    },
    /// A call JS made into Rust that runs in order with the UI's ops: a
    /// module function with a context, the start of an `async` function or
    /// member, a method of a component's ref, or the place of a JS call
    /// posted during a synchronous call ([`crate::ui::RustCall`]). Never
    /// reaches a platform.
    Rust(crate::ui::RustOp),
}
