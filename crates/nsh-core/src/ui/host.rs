//! The host elements beyond views, texts and images: inputs, scroll views,
//! navigation stacks and their screens, modals, canvases, web views, videos
//! and the embedder's native components, with their props and typed events. Their props are the
//! core's [`Prop`]s, kept per element and diffed by kind: a prop an
//! element's binding (or a region's element made again) no longer sets
//! goes back to its default, as when JS clears it.

use std::sync::Arc;

use super::{
    Accessible, AnyElement, Interactive, IntoReactive,
    element::{ElNode, Element, IntoElement, Listener, ParentElement},
    events::{self, NativeEvent, Selection, VideoInfo, VideoProgress, WebNavigation},
};
use crate::{
    AutoCapitalize, AutoFill, Color, EventKind, HeaderSlot, JsonValue, KeyboardAppearance,
    KeyboardType, ModalAnimation, ModalPresentation, ModalProp, NativeProp, Prop, ResizeMode,
    ReturnKeyType, ScreenAnimation, ScreenProp, ScrollProp, Size, SubmitBehavior, Tag, WebSource,
    canvas::{
        CanvasFont, CanvasGradient, CanvasImage, CanvasOp, CanvasTextAlign, CanvasTextBaseline,
        Composite, Drawing, FillRule, LineCap, LineJoin, Num, Paint, Path, PathOp,
    },
    js::host::HostProps,
};

/// Whether `a` and `b` set the same property (their values aside).
pub(crate) fn same_kind(a: &Prop, b: &Prop) -> bool {
    match (a, b) {
        (Prop::ComponentProp(a, _), Prop::ComponentProp(b, _)) => a == b,
        _ => HostProps::same_kind(a, b),
    }
}

/// `prop`'s property at its default (what clearing it from JS sets), for
/// the props elements set; `None` for those set once.
pub(crate) fn reset(prop: &Prop) -> Option<Prop> {
    match prop {
        Prop::ComponentProp(name, _) => Some(Prop::ComponentProp(name.clone(), None)),
        _ => HostProps::reset(prop),
    }
}

/// Sets `prop` on `node`, replacing the value of the same property.
pub(crate) fn set(node: &mut ElNode, prop: Prop) {
    set_in(&mut node.attrs.props, prop);
}

crate::static_reactive!(Color: Color);
crate::static_reactive!(AutoCapitalize: AutoCapitalize);
crate::static_reactive!(AutoFill: AutoFill);
crate::static_reactive!(KeyboardAppearance: KeyboardAppearance);
crate::static_reactive!(KeyboardType: KeyboardType);
crate::static_reactive!(ModalAnimation: ModalAnimation);
crate::static_reactive!(ModalPresentation: ModalPresentation);
crate::static_reactive!(ResizeMode: ResizeMode);
crate::static_reactive!(ReturnKeyType: ReturnKeyType);
crate::static_reactive!(SubmitBehavior: SubmitBehavior);

/// Sets `prop` among `props`, replacing one of its kind.
fn set_in(props: &mut Vec<Prop>, prop: Prop) {
    match props.iter_mut().find(|p| same_kind(p, &prop)) {
        Some(p) => *p = prop,
        None => props.push(prop),
    }
}

/// Sets the prop `prop` makes of `value`: now when it is static, else as a
/// bound part.
pub(crate) fn bind_prop<T: 'static>(
    node: &mut ElNode,
    value: impl IntoReactive<T>,
    prop: impl Fn(T) -> Prop + 'static,
) {
    node.bind_attr(value, move |attrs, v| set_in(&mut attrs.props, prop(v)));
}

/// Adds a typed listener of `kind`.
fn listen<T: ?Sized + ToOwned + 'static>(
    node: &mut ElNode,
    kind: EventKind,
    listener: Listener<T>,
    decode: events::Decode<T>,
) {
    node.handlers.push((kind, events::typed(listener, decode)));
}

macro_rules! host_element {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone)]
        pub struct $name(ElNode);

        impl Element for $name {
            fn node(&mut self) -> &mut ElNode {
                &mut self.0
            }
        }

        impl IntoElement for $name {
            fn into_element(self) -> AnyElement {
                AnyElement(self.0)
            }
        }

        impl Accessible for $name {}
        impl Interactive for $name {}

        // Not every element has props of its own and events.
        #[allow(dead_code)]
        impl $name {
            fn set(mut self, prop: Prop) -> Self {
                set(&mut self.0, prop);
                self
            }

            fn bind_prop<T: 'static>(
                mut self,
                value: impl IntoReactive<T>,
                prop: impl Fn(T) -> Prop + 'static,
            ) -> Self {
                bind_prop(&mut self.0, value, prop);
                self
            }

            fn listen<T: ?Sized + ToOwned + 'static>(
                mut self,
                kind: EventKind,
                listener: Listener<T>,
                decode: events::Decode<T>,
            ) -> Self {
                listen(&mut self.0, kind, listener, decode);
                self
            }
        }
    };
}

host_element! {
    /// A text field (`<input>`), made by [`input`]: one line, or several
    /// with [`Input::multiline`]. Its text style comes from its styles.
    Input
}

host_element! {
    /// A scroll view (`<scroll>`), made by [`scroll`]: its children are its
    /// content.
    Scroll
}

host_element! {
    /// A native navigation stack (`<stack>`), made by [`stack`]:
    /// UINavigationController on iOS (swipe back, native header), a stack
    /// with a toolbar and predictive back on Android. Its children are its
    /// [`Screen`]s, bottom to top.
    Stack
}

host_element! {
    /// One screen of a [`Stack`] (`<screen>`), made by [`screen`]: its
    /// header options and its content (its children).
    Screen
}

host_element! {
    /// A modal (`<modal>`), made by [`modal`]: presented while it is in the
    /// view, over everything else; its children are its content.
    Modal
}

host_element! {
    /// A bar over the keyboard (`<inputaccessory>`, TS: `InputAccessoryView`
    /// of `nsh/ios`), made by [`input_accessory_view`]: its children are its
    /// content.
    InputAccessoryView
}

host_element! {
    /// A view drawn with a [`Drawing`] (`<canvas>`), made by [`canvas`].
    CanvasView
}

host_element! {
    /// A web page (`<webview>`), made by [`web_view`].
    WebView
}

host_element! {
    /// A video player (`<video>`), made by [`video`].
    Video
}

host_element! {
    /// A native component the embedding app registered (`<native>`), made
    /// by [`native`].
    NativeView
}

impl ParentElement for Scroll {}
impl ParentElement for Stack {}
impl ParentElement for Screen {}
impl ParentElement for Modal {}

/// Android has no input accessories: there the bar keeps no content.
impl ParentElement for InputAccessoryView {
    fn child(mut self, child: impl IntoElement) -> Self {
        if !cfg!(target_os = "android") {
            self.0.children.push(child.into_element().0);
        }
        self
    }

    fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        if !cfg!(target_os = "android") {
            self.0
                .children
                .extend(children.into_iter().map(|c| c.into_element().0));
        }
        self
    }
}

/// A text field.
pub fn input() -> Input {
    Input(ElNode::new(Tag::Input))
}

/// A vertical scroll view ([`Scroll::horizontal`] for a horizontal one).
pub fn scroll() -> Scroll {
    Scroll(ElNode::new(Tag::Scroll))
}

/// A native navigation stack of [`screen`]s: put a screen in to push it,
/// leave it out to pop it. Give each screen a [`Element::key`] (its route)
/// so the screens below keep their state.
pub fn stack() -> Stack {
    Stack(ElNode::new(Tag::Stack))
}

/// A screen of a [`stack`]. What it shows reads its insets below the
/// header ([`super::Scope::insets`]).
pub fn screen() -> Screen {
    let mut node = ElNode::new(Tag::Screen);
    // Its insets come with this event.
    node.handlers.push((EventKind::Insets, Listener::noop()));
    Screen(node)
}

/// A modal presenting its children. Put it in the view to present it (in
/// a region: [`super::If`]), leave it out to dismiss it.
pub fn modal() -> Modal {
    let mut node = ElNode::new(Tag::Modal);
    // Its insets (`Scope::insets` inside it) come with this event.
    node.handlers.push((EventKind::Insets, Listener::noop()));
    Modal(node)
}

/// A bar shown over the keyboard of the inputs whose
/// [`Input::input_accessory`] is `native_id` (iOS's `inputAccessoryView`):
/// as wide as the window and as tall as its content, moving with the
/// keyboard natively. It takes no room where the view places it; on
/// Android, which has no input accessories, it is an empty hidden view.
pub fn input_accessory_view(native_id: &str) -> InputAccessoryView {
    let tag = if cfg!(target_os = "android") {
        Tag::View
    } else {
        Tag::InputAccessory
    };
    let mut node = ElNode::new(tag);
    if cfg!(target_os = "android") {
        node.style.keyword(crate::StyleKey::Display, "none");
    } else {
        set(&mut node, Prop::NativeId(Some(native_id.into())));
    }
    InputAccessoryView(node)
}

/// A canvas showing what `painter` recorded (native values in it move
/// natively). A closure draws in a binding: `canvas(move || draw(theme()))`
/// draws again when what it read changes (a size kept from `on_layout`,
/// the appearance).
pub fn canvas(painter: impl IntoReactive<Painter>) -> CanvasView {
    let mut node = ElNode::new(Tag::Canvas);
    node.bind_attr(painter, |attrs, painter: Painter| {
        attrs.drawing = Some(Arc::new(painter.drawing));
        attrs.bound = painter.nodes;
    });
    CanvasView(node)
}

crate::static_reactive!(Painter: Painter, Drawing);

/// A web view loading `url`.
pub fn web_view(url: &str) -> WebView {
    WebView(ElNode::new(Tag::WebView))
        .set(Prop::Native(NativeProp::Source(WebSource::Uri(url.into()))))
}

/// A web view showing the page `html`, its relative links resolved against
/// `base_url` (TS: `source={{ html, baseUrl }}`).
pub fn web_view_html(html: impl IntoReactive<String>, base_url: Option<&str>) -> WebView {
    WebView(ElNode::new(Tag::WebView)).html(html, base_url)
}

/// A video playing the file or stream at `url`.
pub fn video(url: &str) -> Video {
    Video(ElNode::new(Tag::Video)).set(Prop::Native(NativeProp::Source(WebSource::Uri(url.into()))))
}

/// The embedder's native component registered as `name`.
pub fn native(name: &str) -> NativeView {
    NativeView(ElNode::new(Tag::Native)).set(Prop::Component(name.into()))
}

impl Input {
    /// The text, when the component controls it (update it from
    /// [`Self::on_change_text`]).
    pub fn value(self, text: impl IntoReactive<String>) -> Self {
        self.bind_prop(text, |text: String| Prop::Value(text))
    }

    /// The first text, when the input keeps its own.
    pub fn default_value(self, text: impl IntoReactive<String>) -> Self {
        self.bind_prop(text, |text: String| Prop::DefaultValue(text))
    }

    /// Shown while it is empty.
    pub fn placeholder(self, text: impl IntoReactive<String>) -> Self {
        self.bind_prop(text, |text: String| Prop::Placeholder(text))
    }

    /// The placeholder's color (`cx.color(theme.muted_foreground)`).
    pub fn placeholder_color(self, color: impl IntoReactive<Color>) -> Self {
        self.bind_prop(color, |color: Color| Prop::PlaceholderColor(Some(color)))
    }

    /// The caret's and the selection's color.
    pub fn selection_color(self, color: impl IntoReactive<Color>) -> Self {
        self.bind_prop(color, |color: Color| Prop::SelectionColor(Some(color)))
    }

    /// Hides what is typed (passwords).
    pub fn secure(self, secure: impl IntoReactive<bool>) -> Self {
        self.bind_prop(secure, |secure: bool| Prop::Secure(secure))
    }

    /// The software keyboard's layout.
    pub fn keyboard_type(self, kind: impl IntoReactive<KeyboardType>) -> Self {
        self.bind_prop(kind, |kind: KeyboardType| Prop::KeyboardType(kind))
    }

    /// The software keyboard's light or dark look (iOS; TS:
    /// `ios={{ keyboardAppearance }}`).
    pub fn keyboard_appearance(self, appearance: impl IntoReactive<KeyboardAppearance>) -> Self {
        self.bind_prop(appearance, |appearance: KeyboardAppearance| {
            Prop::KeyboardAppearance(appearance)
        })
    }

    /// The return key's label.
    pub fn return_key_type(self, kind: impl IntoReactive<ReturnKeyType>) -> Self {
        self.bind_prop(kind, |kind: ReturnKeyType| Prop::ReturnKeyType(kind))
    }

    /// Automatic capitals.
    pub fn auto_capitalize(self, mode: impl IntoReactive<AutoCapitalize>) -> Self {
        self.bind_prop(mode, |mode: AutoCapitalize| Prop::AutoCapitalize(mode))
    }

    /// Spelling correction (default on).
    pub fn auto_correct(self, on: impl IntoReactive<bool>) -> Self {
        self.bind_prop(on, |on: bool| Prop::AutoCorrect(on))
    }

    /// What it holds, for autofill and suggestions.
    pub fn auto_fill(self, kind: impl IntoReactive<AutoFill>) -> Self {
        self.bind_prop(kind, |kind: AutoFill| Prop::AutoFill(kind))
    }

    /// Several lines, growing with the text (return adds a line).
    pub fn multiline(self, multiline: impl IntoReactive<bool>) -> Self {
        self.bind_prop(multiline, |multiline: bool| Prop::Multiline(multiline))
    }

    /// At most `length` characters.
    pub fn max_length(self, length: impl IntoReactive<u32>) -> Self {
        self.bind_prop(length, |length: u32| Prop::MaxLength(Some(length)))
    }

    /// Editable (default yes).
    pub fn editable(self, editable: impl IntoReactive<bool>) -> Self {
        self.bind_prop(editable, |editable: bool| Prop::Editable(editable))
    }

    /// The return key unfocuses it (default yes, for one line).
    pub fn blur_on_submit(self, blur: impl IntoReactive<bool>) -> Self {
        self.bind_prop(blur, |blur: bool| Prop::BlurOnSubmit(blur))
    }

    /// What return does (`submitBehavior`; default: submit and unfocus for
    /// one line, a line break for several).
    pub fn submit_behavior(self, behavior: impl IntoReactive<SubmitBehavior>) -> Self {
        self.bind_prop(behavior, |behavior: SubmitBehavior| {
            Prop::SubmitBehavior(Some(behavior))
        })
    }

    /// Focused when it appears.
    pub fn auto_focus(self, focus: impl IntoReactive<bool>) -> Self {
        self.bind_prop(focus, |focus: bool| Prop::AutoFocus(focus))
    }

    /// No caret.
    pub fn caret_hidden(self, hidden: impl IntoReactive<bool>) -> Self {
        self.bind_prop(hidden, |hidden: bool| Prop::CaretHidden(hidden))
    }

    /// Shows the input accessory with this `nativeID` over the keyboard
    /// ([`input_accessory_view`]; iOS).
    pub fn input_accessory(self, native_id: impl IntoReactive<String>) -> Self {
        self.bind_prop(native_id, |id: String| {
            Prop::InputAccessoryViewId(Some(id.into()))
        })
    }

    /// Selects `(start, end)` (UTF-16 units; a caret when equal) when the
    /// input is made and whenever the value changes (TS: the controlled
    /// `selection`); the user moves it on from there
    /// ([`Self::on_selection_change`]).
    pub fn selection(self, range: impl IntoReactive<(u32, u32)>) -> Self {
        self.bind_prop(range, |(start, end): (u32, u32)| {
            Prop::Selection(start, end)
        })
    }

    /// The text changed (the new text).
    pub fn on_change_text(self, listener: impl Into<Listener<str>>) -> Self {
        self.listen(EventKind::ChangeText, listener.into(), events::text)
    }

    /// The return key was pressed (the text).
    pub fn on_submit_editing(self, listener: impl Into<Listener<str>>) -> Self {
        self.listen(EventKind::SubmitEditing, listener.into(), events::text)
    }

    /// It became focused.
    pub fn on_focus(self, listener: impl Into<Listener<()>>) -> Self {
        self.listen(EventKind::Focus, listener.into(), events::unit)
    }

    /// It lost focus.
    pub fn on_blur(self, listener: impl Into<Listener<()>>) -> Self {
        self.listen(EventKind::Blur, listener.into(), events::unit)
    }

    /// The selection or caret moved.
    pub fn on_selection_change(self, listener: impl Into<Listener<Selection>>) -> Self {
        self.listen(
            EventKind::SelectionChange,
            listener.into(),
            events::selection,
        )
    }

    /// A key was pressed: the character, `Backspace` or `Enter`.
    pub fn on_key_press(self, listener: impl Into<Listener<str>>) -> Self {
        self.listen(EventKind::KeyPress, listener.into(), events::key)
    }

    /// The text's size changed (multiline inputs growing).
    pub fn on_content_size_change(self, listener: impl Into<Listener<Size>>) -> Self {
        self.listen(EventKind::ContentSizeChange, listener.into(), events::size)
    }
}

impl Scroll {
    /// Scrolls horizontally.
    pub fn horizontal(self) -> Self {
        self.set(Prop::Horizontal(true))
    }

    /// Whether the scroll indicator shows (default yes).
    pub fn shows_indicator(self, shows: impl IntoReactive<bool>) -> Self {
        self.bind_prop(shows, |shows: bool| Prop::ShowsIndicator(shows))
    }

    /// Pinch zoom between `min` and `max`.
    pub fn zoom_range(self, min: f32, max: f32) -> Self {
        self.set(Prop::Scroll(ScrollProp::MinZoom(min)))
            .set(Prop::Scroll(ScrollProp::MaxZoom(max)))
    }

    /// The zoom.
    pub fn zoom(self, scale: impl IntoReactive<f32>) -> Self {
        self.bind_prop(scale, |scale: f32| Prop::Scroll(ScrollProp::Zoom(scale)))
    }

    /// Pinches may go past the zoom limits and spring back (default yes;
    /// TS: `bouncesZoom`).
    pub fn bounces_zoom(self, bounces: impl IntoReactive<bool>) -> Self {
        self.bind_prop(bounces, |bounces: bool| {
            Prop::Scroll(ScrollProp::BouncesZoom(bounces))
        })
    }

    /// The user can pinch to zoom (default yes; TS: `pinchGestureEnabled`).
    pub fn pinch_gesture_enabled(self, enabled: impl IntoReactive<bool>) -> Self {
        self.bind_prop(enabled, |enabled: bool| {
            Prop::Scroll(ScrollProp::Pinch(enabled))
        })
    }

    /// Content smaller than the view sits in its center (zoomed images).
    pub fn center_content(self, center: impl IntoReactive<bool>) -> Self {
        self.bind_prop(center, |center: bool| {
            Prop::Scroll(ScrollProp::CenterContent(center))
        })
    }
}

impl super::Scrollable for Scroll {}

impl Screen {
    fn header_item(mut self, slot: HeaderSlot, key: &'static str, item: impl IntoElement) -> Self {
        let mut node = ElNode::new(Tag::HeaderItem);
        node.key = Some(key.into());
        set(&mut node, Prop::HeaderSlot(slot));
        node.children.push(item.into_element().0);
        self.0.children.push(node);
        self
    }

    /// The header's title.
    pub fn title(self, title: impl IntoReactive<super::SharedString>) -> Self {
        self.bind_prop(title, |t| {
            Prop::Screen(ScreenProp::Title(Some(Arc::from(t.as_str()))))
        })
    }

    /// Whether the header shows (default true).
    pub fn header_shown(self, shown: impl IntoReactive<bool>) -> Self {
        self.bind_prop(shown, |v| Prop::Screen(ScreenProp::HeaderShown(v)))
    }

    /// A large title (iOS); the content scrolls under the header.
    pub fn header_large_title(self, large: impl IntoReactive<bool>) -> Self {
        self.bind_prop(large, |v| Prop::Screen(ScreenProp::HeaderLargeTitle(v)))
    }

    /// The content extends under the header.
    pub fn header_transparent(self, transparent: impl IntoReactive<bool>) -> Self {
        self.bind_prop(transparent, |v| {
            Prop::Screen(ScreenProp::HeaderTransparent(v))
        })
    }

    /// The back button's and bar buttons' color.
    pub fn header_tint_color(self, color: impl IntoReactive<Color>) -> Self {
        self.bind_prop(color, |c| {
            Prop::Screen(ScreenProp::HeaderTintColor(Some(c)))
        })
    }

    /// The header's background.
    pub fn header_background_color(self, color: impl IntoReactive<Color>) -> Self {
        self.bind_prop(color, |c| {
            Prop::Screen(ScreenProp::HeaderBackgroundColor(Some(c)))
        })
    }

    /// The title's color.
    pub fn header_title_color(self, color: impl IntoReactive<Color>) -> Self {
        self.bind_prop(color, |c| {
            Prop::Screen(ScreenProp::HeaderTitleColor(Some(c)))
        })
    }

    /// The label of the back button leading back to this screen (iOS).
    pub fn header_back_title(self, title: impl IntoReactive<super::SharedString>) -> Self {
        self.bind_prop(title, |t| {
            Prop::Screen(ScreenProp::HeaderBackTitle(Some(Arc::from(t.as_str()))))
        })
    }

    /// Whether the back button shows (default true).
    pub fn header_back_visible(self, visible: impl IntoReactive<bool>) -> Self {
        self.bind_prop(visible, |v| Prop::Screen(ScreenProp::HeaderBackVisible(v)))
    }

    /// The interactive back gesture (iOS swipe back, Android predictive
    /// back); the back buttons pop either way (default true).
    pub fn gesture_enabled(self, enabled: impl IntoReactive<bool>) -> Self {
        self.bind_prop(enabled, |v| Prop::Screen(ScreenProp::GestureEnabled(v)))
    }

    /// The user cannot leave the screen: no swipe back, and the back
    /// buttons report [`Self::on_back_request`] instead of popping. Decided
    /// natively when the system asks, so set it before the user acts.
    pub fn prevent_back(self, prevent: impl IntoReactive<bool>) -> Self {
        self.bind_prop(prevent, |v| Prop::Screen(ScreenProp::PreventBack(v)))
    }

    /// How the screen enters and leaves.
    pub fn animation(self, animation: impl IntoReactive<ScreenAnimation>) -> Self {
        self.bind_prop(animation, |a| Prop::Screen(ScreenProp::Animation(a)))
    }

    /// An element at the header's start (instead of the back button).
    pub fn header_left(self, item: impl IntoElement) -> Self {
        self.header_item(HeaderSlot::Left, "header-left", item)
    }

    /// An element at the header's end.
    pub fn header_right(self, item: impl IntoElement) -> Self {
        self.header_item(HeaderSlot::Right, "header-right", item)
    }

    /// An element replacing the title.
    pub fn header_title(self, item: impl IntoElement) -> Self {
        self.header_item(HeaderSlot::Title, "header-title", item)
    }

    /// The user popped the screen natively (back button, swipe): leave it
    /// out of the view (its region).
    pub fn on_dismiss(self, listener: impl Into<Listener<()>>) -> Self {
        self.listen(EventKind::Dismiss, listener.into(), events::unit)
    }

    /// The user tried to leave a screen that [`Self::prevent_back`]s.
    pub fn on_back_request(self, listener: impl Into<Listener<()>>) -> Self {
        self.listen(EventKind::BackRequest, listener.into(), events::unit)
    }
}

impl Modal {
    /// How it comes in (default: slides up).
    pub fn animation(self, animation: impl IntoReactive<ModalAnimation>) -> Self {
        self.bind_prop(animation, |animation: ModalAnimation| {
            Prop::Modal(ModalProp::Animation(animation))
        })
    }

    /// Full screen, or a sheet (iOS).
    pub fn presentation(self, presentation: impl IntoReactive<ModalPresentation>) -> Self {
        self.bind_prop(presentation, |presentation: ModalPresentation| {
            Prop::Modal(ModalProp::Presentation(presentation))
        })
    }

    /// What is behind shows through (its content draws the dimming).
    pub fn transparent(self, transparent: impl IntoReactive<bool>) -> Self {
        self.bind_prop(transparent, |transparent: bool| {
            Prop::Modal(ModalProp::Transparent(transparent))
        })
    }

    /// Under the status bar (Android).
    pub fn status_bar_translucent(self, translucent: impl IntoReactive<bool>) -> Self {
        self.bind_prop(translucent, |translucent: bool| {
            Prop::Modal(ModalProp::StatusBarTranslucent(translucent))
        })
    }

    /// The orientations it allows while it is presented (iOS; default:
    /// those of what presents it).
    pub fn supported_orientations(
        self,
        orientations: impl IntoIterator<Item = Orientation>,
    ) -> Self {
        let bits = orientations.into_iter().fold(0, |bits, o| bits | o.bits());
        self.set(Prop::Modal(ModalProp::Orientations(bits)))
    }

    /// Swipes down and the back button dismiss it, reporting
    /// [`Self::on_dismiss`] (default: they ask with
    /// [`Self::on_request_close`]).
    pub fn dismissible(self, dismissible: impl IntoReactive<bool>) -> Self {
        self.bind_prop(dismissible, |dismissible: bool| {
            Prop::Modal(ModalProp::Dismissible(dismissible))
        })
    }

    /// The user asked to close it (back button, swipe down): leave it out
    /// of the view (its region) to close it.
    pub fn on_request_close(self, listener: impl Into<Listener<()>>) -> Self {
        self.listen(EventKind::RequestClose, listener.into(), events::unit)
    }

    /// It finished appearing.
    pub fn on_show(self, listener: impl Into<Listener<()>>) -> Self {
        self.listen(EventKind::Show, listener.into(), events::unit)
    }

    /// The user or the system dismissed it.
    pub fn on_dismiss(self, listener: impl Into<Listener<()>>) -> Self {
        self.listen(EventKind::Dismiss, listener.into(), events::unit)
    }
}

/// An orientation of the screen ([`Modal::supported_orientations`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// Upright.
    Portrait,
    /// Upside down.
    PortraitUpsideDown,
    /// Either landscape.
    Landscape,
    /// Landscape, the top on the left.
    LandscapeLeft,
    /// Landscape, the top on the right.
    LandscapeRight,
}

impl Orientation {
    fn bits(self) -> u8 {
        use crate::ModalConfig as M;
        match self {
            Orientation::Portrait => M::PORTRAIT,
            Orientation::PortraitUpsideDown => M::PORTRAIT_UPSIDE_DOWN,
            Orientation::Landscape => M::LANDSCAPE_LEFT | M::LANDSCAPE_RIGHT,
            Orientation::LandscapeLeft => M::LANDSCAPE_LEFT,
            Orientation::LandscapeRight => M::LANDSCAPE_RIGHT,
        }
    }
}

impl WebView {
    /// Loads `html` (relative links against `base_url`) instead of a URL;
    /// a bound page loads again when it changes.
    pub fn html(self, html: impl IntoReactive<String>, base_url: Option<&str>) -> Self {
        let base_url: Option<Arc<str>> = base_url.map(Into::into);
        self.bind_prop(html, move |html: String| {
            Prop::Native(NativeProp::Source(WebSource::Html {
                html: html.into(),
                base_url: base_url.clone(),
            }))
        })
    }

    /// Runs after each page loads.
    pub fn injected_java_script(self, script: impl IntoReactive<String>) -> Self {
        self.bind_prop(script, |script: String| {
            Prop::Native(NativeProp::InjectedJavaScript(Some(script.into())))
        })
    }

    /// Runs before each page's own scripts.
    pub fn injected_java_script_before_content(self, script: impl IntoReactive<String>) -> Self {
        self.bind_prop(script, |script: String| {
            Prop::Native(NativeProp::InjectedJavaScriptBefore(Some(script.into())))
        })
    }

    /// The user agent it sends.
    pub fn user_agent(self, agent: impl IntoReactive<String>) -> Self {
        self.bind_prop(agent, |agent: String| {
            Prop::Native(NativeProp::UserAgent(Some(agent.into())))
        })
    }

    /// Scrolls (default yes).
    pub fn scroll_enabled(self, enabled: impl IntoReactive<bool>) -> Self {
        self.bind_prop(enabled, |enabled: bool| {
            Prop::Native(NativeProp::ScrollEnabled(enabled))
        })
    }

    /// Runs the pages' JavaScript (default yes).
    pub fn java_script_enabled(self, enabled: impl IntoReactive<bool>) -> Self {
        self.bind_prop(enabled, |enabled: bool| {
            Prop::Native(NativeProp::JavaScriptEnabled(enabled))
        })
    }

    /// Swipes go back and forward (iOS).
    pub fn back_forward_gestures(self, enabled: impl IntoReactive<bool>) -> Self {
        self.bind_prop(enabled, |enabled: bool| {
            Prop::Native(NativeProp::BackForwardGestures(enabled))
        })
    }

    /// A page started loading.
    pub fn on_load_start(self, listener: impl Into<Listener<WebNavigation>>) -> Self {
        self.listen(EventKind::LoadStart, listener.into(), events::navigation)
    }

    /// A page loaded.
    pub fn on_load(self, listener: impl Into<Listener<WebNavigation>>) -> Self {
        self.listen(EventKind::Load, listener.into(), events::navigation)
    }

    /// A page failed to load (the message).
    pub fn on_error(self, listener: impl Into<Listener<str>>) -> Self {
        self.listen(EventKind::Error, listener.into(), events::message)
    }

    /// The page called `window.ReactNativeWebView.postMessage(data)`.
    pub fn on_message(self, listener: impl Into<Listener<str>>) -> Self {
        self.listen(EventKind::Message, listener.into(), events::data)
    }

    /// It navigated.
    pub fn on_navigation_state_change(self, listener: impl Into<Listener<WebNavigation>>) -> Self {
        self.listen(
            EventKind::NavigationStateChange,
            listener.into(),
            events::navigation,
        )
    }
}

impl Video {
    /// Paused.
    pub fn paused(self, paused: impl IntoReactive<bool>) -> Self {
        self.bind_prop(paused, |paused: bool| {
            Prop::Native(NativeProp::Paused(paused))
        })
    }

    /// Without sound.
    pub fn muted(self, muted: impl IntoReactive<bool>) -> Self {
        self.bind_prop(muted, |muted: bool| Prop::Native(NativeProp::Muted(muted)))
    }

    /// 0..1.
    pub fn volume(self, volume: impl IntoReactive<f32>) -> Self {
        self.bind_prop(volume, |volume: f32| {
            Prop::Native(NativeProp::Volume(volume))
        })
    }

    /// Playback speed.
    pub fn rate(self, rate: impl IntoReactive<f32>) -> Self {
        self.bind_prop(rate, |rate: f32| Prop::Native(NativeProp::Rate(rate)))
    }

    /// Starts over at the end.
    pub fn repeat(self, repeat: impl IntoReactive<bool>) -> Self {
        self.bind_prop(repeat, |repeat: bool| {
            Prop::Native(NativeProp::Repeat(repeat))
        })
    }

    /// How the picture fits (default: contain).
    pub fn resize_mode(self, mode: impl IntoReactive<ResizeMode>) -> Self {
        self.bind_prop(mode, |mode: ResizeMode| {
            Prop::Native(NativeProp::VideoResizeMode(mode))
        })
    }

    /// The platform's playback controls.
    pub fn controls(self, controls: impl IntoReactive<bool>) -> Self {
        self.bind_prop(controls, |controls: bool| {
            Prop::Native(NativeProp::Controls(controls))
        })
    }

    /// Milliseconds between [`Self::on_progress`] events (default 250).
    pub fn progress_interval(self, ms: impl IntoReactive<f32>) -> Self {
        self.bind_prop(ms, |ms: f32| Prop::Native(NativeProp::ProgressInterval(ms)))
    }

    /// It started loading.
    pub fn on_load_start(self, listener: impl Into<Listener<()>>) -> Self {
        self.listen(EventKind::LoadStart, listener.into(), events::unit)
    }

    /// It loaded: length and size.
    pub fn on_load(self, listener: impl Into<Listener<VideoInfo>>) -> Self {
        self.listen(EventKind::Load, listener.into(), events::video)
    }

    /// It played on.
    pub fn on_progress(self, listener: impl Into<Listener<VideoProgress>>) -> Self {
        self.listen(EventKind::Progress, listener.into(), events::progress)
    }

    /// It played to its end.
    pub fn on_end(self, listener: impl Into<Listener<()>>) -> Self {
        self.listen(EventKind::End, listener.into(), events::unit)
    }

    /// It started (`true`) or stopped buffering.
    pub fn on_buffer(self, listener: impl Into<Listener<bool>>) -> Self {
        self.listen(EventKind::Buffer, listener.into(), events::buffering)
    }

    /// It failed (the message).
    pub fn on_error(self, listener: impl Into<Listener<str>>) -> Self {
        self.listen(EventKind::Error, listener.into(), events::message)
    }
}

impl NativeView {
    /// Its prop `name` (any JSON value), as JS's `nativeComponent` passes it.
    pub fn prop(self, name: &str, value: JsonValue) -> Self {
        self.set(Prop::ComponentProp(name.into(), Some(value)))
    }

    /// It emitted an event.
    pub fn on_native_event(self, listener: impl Into<Listener<NativeEvent>>) -> Self {
        self.listen(EventKind::NativeEvent, listener.into(), events::native)
    }
}

/// A number in a drawing: a constant, or a native value the canvas reads
/// while it draws (it draws again natively as the value moves).
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasArg(pub(crate) CanvasArgKind);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CanvasArgKind {
    Const(f32),
    Node(super::NativeNode),
}

impl From<f32> for CanvasArg {
    fn from(n: f32) -> Self {
        Self(CanvasArgKind::Const(n))
    }
}

impl<A: super::Animated> From<&A> for CanvasArg {
    fn from(a: &A) -> Self {
        Self(CanvasArgKind::Node(a.native_node()))
    }
}

/// What a drawing fills or strokes with (`fillStyle` / `strokeStyle`): a
/// color, a gradient ([`CanvasGradient::linear`], `radial`, `conic`), or a
/// native value interpolating colors
/// ([`super::NativeNode::interpolate_colors`]), read as it moves.
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasPaint(CanvasPaintKind);

#[derive(Debug, Clone, PartialEq)]
enum CanvasPaintKind {
    Color(Color),
    Gradient(Arc<CanvasGradient>),
    Node(super::NativeNode),
}

impl From<Color> for CanvasPaint {
    fn from(color: Color) -> Self {
        Self(CanvasPaintKind::Color(color))
    }
}

impl From<CanvasGradient> for CanvasPaint {
    fn from(gradient: CanvasGradient) -> Self {
        Self(CanvasPaintKind::Gradient(Arc::new(gradient)))
    }
}

impl From<Arc<CanvasGradient>> for CanvasPaint {
    fn from(gradient: Arc<CanvasGradient>) -> Self {
        Self(CanvasPaintKind::Gradient(gradient))
    }
}

impl<A: super::Animated> From<&A> for CanvasPaint {
    fn from(a: &A) -> Self {
        Self(CanvasPaintKind::Node(a.native_node()))
    }
}

/// Records a [`Drawing`] with the HTML Canvas 2D operations (Rust names):
/// `Painter::new().fill_color(red).fill_rect(0.0, 0.0, w, h).finish()`.
/// Coordinates are points from the canvas's top left. Numbers and paints
/// may be native values (the canvas draws again natively as they move);
/// operations without a method here are built by hand with
/// [`Painter::num`], [`Painter::paint`] and [`Painter::op`]. Text is
/// measured with [`super::ambient::measure_text`].
#[derive(Debug, Clone, Default)]
pub struct Painter {
    drawing: Drawing,
    /// The native values the drawing reads, kept alive with it.
    nodes: Vec<super::NativeNode>,
}

impl Painter {
    /// An empty drawing.
    pub fn new() -> Self {
        Self::default()
    }

    /// The drawing.
    pub fn finish(self) -> Drawing {
        self.drawing
    }

    /// `value` as a number of this drawing, for an operation built by hand
    /// ([`Self::op`]): a native value becomes a slot the canvas reads while
    /// it draws, kept alive with the drawing.
    pub fn num(&mut self, value: impl Into<CanvasArg>) -> Num {
        match value.into().0 {
            CanvasArgKind::Const(n) => Num::Const(n),
            CanvasArgKind::Node(node) => Num::Slot(self.slot(node)),
        }
    }

    /// `paint` as a paint of this drawing, for an operation built by hand
    /// ([`Self::op`]).
    pub fn paint(&mut self, paint: impl Into<CanvasPaint>) -> Paint {
        match paint.into().0 {
            CanvasPaintKind::Color(color) => Paint::Color(color),
            CanvasPaintKind::Gradient(gradient) => Paint::Gradient(gradient),
            CanvasPaintKind::Node(node) => Paint::Slot(self.slot(node)),
        }
    }

    fn slot(&mut self, node: super::NativeNode) -> u16 {
        let slot = self.drawing.slot(node.id());
        self.nodes.push(node);
        slot
    }

    /// Records `op` (one built by hand: its numbers and paints from
    /// [`Self::num`] and [`Self::paint`]).
    pub fn op(mut self, op: CanvasOp) -> Self {
        self.drawing.ops.push(op);
        self
    }

    fn path(self, op: PathOp) -> Self {
        self.op(CanvasOp::Path(op))
    }

    /// Whether point (`x`, `y`) (canvas points, as a touch's `location_x` /
    /// `location_y`) is inside the current path by `rule`, with the
    /// transform, as recorded so far (TS: `ctx.isPointInPath(x, y, rule)`).
    /// Answered from the geometry here, with no platform round trip; a
    /// number given as a native value counts as 0, as in TS. To find what
    /// is under a tap, build the canvas's painter in a function the press
    /// handler calls too.
    pub fn is_point_in_path(&self, x: f32, y: f32, rule: FillRule) -> bool {
        crate::canvas::point_in_path(&self.drawing.ops, None, rule, x, y, |_| 0.0)
    }

    /// Whether point (`x`, `y`) is on the stroke of the current path with
    /// the line width and cap recorded so far (TS: `ctx.isPointInStroke(x,
    /// y)`; round joins, no dashes). See [`Self::is_point_in_path`].
    pub fn is_point_in_stroke(&self, x: f32, y: f32) -> bool {
        crate::canvas::point_in_stroke(&self.drawing.ops, None, x, y, |_| 0.0)
    }

    /// Whether point (`x`, `y`) is inside `path` by `rule`, with the
    /// transform recorded so far (TS: `ctx.isPointInPath(path2d, x, y,
    /// rule)`): the shape [`Self::fill_path`] drew under a tap. See
    /// [`Self::is_point_in_path`].
    pub fn path_contains(&self, path: &Path, x: f32, y: f32, rule: FillRule) -> bool {
        crate::canvas::point_in_path(&self.drawing.ops, Some(path), rule, x, y, |_| 0.0)
    }

    /// Whether point (`x`, `y`) is on the stroke of `path` with the
    /// transform, line width and cap recorded so far (TS:
    /// `ctx.isPointInStroke(path2d, x, y)`). See [`Self::is_point_in_path`].
    pub fn stroke_contains(&self, path: &Path, x: f32, y: f32) -> bool {
        crate::canvas::point_in_stroke(&self.drawing.ops, Some(path), x, y, |_| 0.0)
    }

    /// Saves the state (styles, transform, clip).
    pub fn save(self) -> Self {
        self.op(CanvasOp::Save)
    }

    /// Restores the state saved last.
    pub fn restore(self) -> Self {
        self.op(CanvasOp::Restore)
    }

    /// Moves the origin.
    pub fn translate(mut self, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        let (x, y) = (self.num(x), self.num(y));
        self.op(CanvasOp::Translate(x, y))
    }

    /// Scales the axes.
    pub fn scale(mut self, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        let (x, y) = (self.num(x), self.num(y));
        self.op(CanvasOp::Scale(x, y))
    }

    /// Rotates the axes (radians, clockwise).
    pub fn rotate(mut self, angle: impl Into<CanvasArg>) -> Self {
        let a = self.num(angle);
        self.op(CanvasOp::Rotate(a))
    }

    /// Multiplies the transform by the matrix `[a c e; b d f]`
    /// (`transform`).
    pub fn transform(mut self, m: [impl Into<CanvasArg>; 6]) -> Self {
        let m = m.map(|n| self.num(n));
        self.op(CanvasOp::Transform(m))
    }

    /// Replaces the transform with the matrix `[a c e; b d f]`
    /// (`setTransform`).
    pub fn set_transform(mut self, m: [impl Into<CanvasArg>; 6]) -> Self {
        let m = m.map(|n| self.num(n));
        self.op(CanvasOp::SetTransform(m))
    }

    /// Back to no transform (`resetTransform`).
    pub fn reset_transform(self) -> Self {
        self.set_transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    }

    /// Starts a new path.
    pub fn begin_path(self) -> Self {
        self.op(CanvasOp::BeginPath)
    }

    /// Starts a subpath at (`x`, `y`).
    pub fn move_to(mut self, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        let (x, y) = (self.num(x), self.num(y));
        self.path(PathOp::MoveTo(x, y))
    }

    /// A straight line to (`x`, `y`).
    pub fn line_to(mut self, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        let (x, y) = (self.num(x), self.num(y));
        self.path(PathOp::LineTo(x, y))
    }

    /// A quadratic Bézier curve to `to` (`quadraticCurveTo`).
    pub fn quadratic_curve_to(self, control: (f32, f32), to: (f32, f32)) -> Self {
        let c = Num::Const;
        self.path(PathOp::QuadTo([
            c(control.0),
            c(control.1),
            c(to.0),
            c(to.1),
        ]))
    }

    /// A cubic Bézier curve to (`x`, `y`).
    pub fn bezier_curve_to(self, c1: (f32, f32), c2: (f32, f32), to: (f32, f32)) -> Self {
        let c = Num::Const;
        self.path(PathOp::CubicTo([
            c(c1.0),
            c(c1.1),
            c(c2.0),
            c(c2.1),
            c(to.0),
            c(to.1),
        ]))
    }

    /// A circular arc around (`x`, `y`); angles in radians, clockwise from
    /// the x axis.
    pub fn arc(
        self,
        x: impl Into<CanvasArg>,
        y: impl Into<CanvasArg>,
        radius: impl Into<CanvasArg>,
        start: impl Into<CanvasArg>,
        end: impl Into<CanvasArg>,
    ) -> Self {
        self.arc_in(x, y, radius, start, end, false)
    }

    /// [`Self::arc`] drawn counterclockwise.
    pub fn arc_counterclockwise(
        self,
        x: impl Into<CanvasArg>,
        y: impl Into<CanvasArg>,
        radius: impl Into<CanvasArg>,
        start: impl Into<CanvasArg>,
        end: impl Into<CanvasArg>,
    ) -> Self {
        self.arc_in(x, y, radius, start, end, true)
    }

    fn arc_in(
        mut self,
        x: impl Into<CanvasArg>,
        y: impl Into<CanvasArg>,
        radius: impl Into<CanvasArg>,
        start: impl Into<CanvasArg>,
        end: impl Into<CanvasArg>,
        ccw: bool,
    ) -> Self {
        let (x, y, radius, start, end) = (
            self.num(x),
            self.num(y),
            self.num(radius),
            self.num(start),
            self.num(end),
        );
        self.path(PathOp::Arc {
            x,
            y,
            radius,
            start,
            end,
            ccw,
        })
    }

    /// An arc of `radius` tangent to the lines to (`x1`, `y1`) and on to
    /// (`x2`, `y2`) (`arcTo`): a rounded corner.
    pub fn arc_to(self, x1: f32, y1: f32, x2: f32, y2: f32, radius: f32) -> Self {
        let c = Num::Const;
        self.path(PathOp::ArcTo([c(x1), c(y1), c(x2), c(y2), c(radius)]))
    }

    /// An elliptical arc around (`x`, `y`) with radii `radius` turned by
    /// `rotation`; angles in radians, clockwise (`counterclockwise` draws
    /// the other way).
    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(
        self,
        x: f32,
        y: f32,
        radius: (f32, f32),
        rotation: f32,
        start: f32,
        end: f32,
        counterclockwise: bool,
    ) -> Self {
        let c = Num::Const;
        self.path(PathOp::Ellipse {
            geometry: [c(x), c(y), c(radius.0), c(radius.1), c(rotation)],
            start: c(start),
            end: c(end),
            ccw: counterclockwise,
        })
    }

    /// A rectangle subpath.
    pub fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Self {
        let c = Num::Const;
        self.path(PathOp::Rect([c(x), c(y), c(w), c(h)]))
    }

    /// A rectangle subpath with round corners (one radius for all).
    pub fn round_rect(self, x: f32, y: f32, w: f32, h: f32, radius: f32) -> Self {
        self.round_rect_corners(x, y, w, h, [radius; 4])
    }

    /// A rectangle subpath with round corners: top left, top right, bottom
    /// right, bottom left.
    pub fn round_rect_corners(self, x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) -> Self {
        let c = Num::Const;
        self.path(PathOp::RoundRect([c(x), c(y), c(w), c(h)], radii.map(c)))
    }

    /// A straight line back to the subpath's start.
    pub fn close_path(self) -> Self {
        self.path(PathOp::Close)
    }

    /// Fills the current path.
    pub fn fill(self) -> Self {
        self.fill_with(FillRule::NonZero)
    }

    /// Fills the current path by `rule`.
    pub fn fill_with(self, rule: FillRule) -> Self {
        self.op(CanvasOp::Fill(None, rule))
    }

    /// Fills `path` (TS: `fill(path2d)`; parse SVG data once with
    /// [`crate::canvas::svg_path`]).
    pub fn fill_path(self, path: &Path, rule: FillRule) -> Self {
        self.op(CanvasOp::Fill(Some(path.clone()), rule))
    }

    /// Fills SVG path data (parsed now: [`Self::fill_path`] reuses a
    /// parsed path).
    pub fn fill_svg(self, data: &str) -> Self {
        self.fill_path(&crate::canvas::svg_path(data), FillRule::NonZero)
    }

    /// Strokes the current path.
    pub fn stroke(self) -> Self {
        self.op(CanvasOp::Stroke(None))
    }

    /// Strokes `path` (TS: `stroke(path2d)`).
    pub fn stroke_path(self, path: &Path) -> Self {
        self.op(CanvasOp::Stroke(Some(path.clone())))
    }

    /// Strokes SVG path data (parsed now).
    pub fn stroke_svg(self, data: &str) -> Self {
        self.stroke_path(&crate::canvas::svg_path(data))
    }

    /// Clips to the current path.
    pub fn clip(self) -> Self {
        self.clip_with(FillRule::NonZero)
    }

    /// Clips to the current path by `rule`.
    pub fn clip_with(self, rule: FillRule) -> Self {
        self.op(CanvasOp::Clip(None, rule))
    }

    /// Clips to `path` (TS: `clip(path2d)`).
    pub fn clip_path(self, path: &Path, rule: FillRule) -> Self {
        self.op(CanvasOp::Clip(Some(path.clone()), rule))
    }

    /// Fills a rectangle.
    pub fn fill_rect(
        mut self,
        x: f32,
        y: f32,
        w: impl Into<CanvasArg>,
        h: impl Into<CanvasArg>,
    ) -> Self {
        let (w, h) = (self.num(w), self.num(h));
        self.op(CanvasOp::FillRect([Num::Const(x), Num::Const(y), w, h]))
    }

    /// Strokes a rectangle.
    pub fn stroke_rect(self, x: f32, y: f32, w: f32, h: f32) -> Self {
        let c = Num::Const;
        self.op(CanvasOp::StrokeRect([c(x), c(y), c(w), c(h)]))
    }

    /// Makes a rectangle transparent.
    pub fn clear_rect(self, x: f32, y: f32, w: f32, h: f32) -> Self {
        let c = Num::Const;
        self.op(CanvasOp::ClearRect([c(x), c(y), c(w), c(h)]))
    }

    /// Fills with `color`.
    pub fn fill_color(self, color: Color) -> Self {
        self.fill_style(color)
    }

    /// Strokes with `color`.
    pub fn stroke_color(self, color: Color) -> Self {
        self.stroke_style(color)
    }

    /// Fills with `paint`: a color, a gradient or a native value of colors
    /// (`fillStyle`).
    pub fn fill_style(mut self, paint: impl Into<CanvasPaint>) -> Self {
        let paint = self.paint(paint);
        self.op(CanvasOp::FillStyle(paint))
    }

    /// Strokes with `paint` (`strokeStyle`).
    pub fn stroke_style(mut self, paint: impl Into<CanvasPaint>) -> Self {
        let paint = self.paint(paint);
        self.op(CanvasOp::StrokeStyle(paint))
    }

    /// The stroke width.
    pub fn line_width(mut self, width: impl Into<CanvasArg>) -> Self {
        let w = self.num(width);
        self.op(CanvasOp::LineWidth(w))
    }

    /// How strokes end.
    pub fn line_cap(self, cap: LineCap) -> Self {
        self.op(CanvasOp::LineCap(cap))
    }

    /// How stroke segments join.
    pub fn line_join(self, join: LineJoin) -> Self {
        self.op(CanvasOp::LineJoin(join))
    }

    /// How far mitered joins reach, in line widths (`miterLimit`; at least
    /// 1).
    pub fn miter_limit(self, limit: f32) -> Self {
        self.op(CanvasOp::MiterLimit(limit.max(1.0)))
    }

    /// Dashes: lengths of dashes and gaps.
    pub fn line_dash(self, segments: &[f32]) -> Self {
        self.op(CanvasOp::LineDash(segments.into()))
    }

    /// Where the dashes start along the line (`lineDashOffset`; a native
    /// value marches them).
    pub fn line_dash_offset(mut self, offset: impl Into<CanvasArg>) -> Self {
        let o = self.num(offset);
        self.op(CanvasOp::LineDashOffset(o))
    }

    /// The opacity of what is drawn next.
    pub fn global_alpha(mut self, alpha: impl Into<CanvasArg>) -> Self {
        let a = self.num(alpha);
        self.op(CanvasOp::GlobalAlpha(a))
    }

    /// How what is drawn next combines with the canvas.
    pub fn composite(self, composite: Composite) -> Self {
        self.op(CanvasOp::Composite(composite))
    }

    /// Shadows of what is drawn next.
    pub fn shadow(self, color: Color, blur: f32, offset: (f32, f32)) -> Self {
        self.op(CanvasOp::ShadowColor(color))
            .op(CanvasOp::ShadowBlur(blur.max(0.0)))
            .op(CanvasOp::ShadowOffset(offset.0, offset.1))
    }

    /// The font (CSS `font` shorthand: `"600 14px Inter"`); an invalid one
    /// keeps the previous font, as in HTML.
    pub fn font(self, font: &str) -> Self {
        match CanvasFont::parse(font) {
            Some(font) => self.op(CanvasOp::Font(font)),
            None => self,
        }
    }

    /// The number forms of text (TS: `fontVariantNumeric`): with
    /// [`crate::style::FontVariant::TABULAR_NUMS`] figures have equal widths, so changing
    /// values do not jitter; `FontVariant::default()` keeps the font's.
    pub fn font_variant_numeric(self, numeric: crate::style::FontVariant) -> Self {
        self.op(CanvasOp::FontVariantNumeric(crate::style::FontVariant(
            numeric.0 & !crate::style::FontVariant::SMALL_CAPS,
        )))
    }

    /// Where text sits relative to its x.
    pub fn text_align(self, align: CanvasTextAlign) -> Self {
        self.op(CanvasOp::TextAlign(align))
    }

    /// Where text sits relative to its y.
    pub fn text_baseline(self, baseline: CanvasTextBaseline) -> Self {
        self.op(CanvasOp::TextBaseline(baseline))
    }

    /// Fills `text` at (`x`, `y`).
    pub fn fill_text(self, text: &str, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        self.text(text, x, y, None, false)
    }

    /// Fills `text` at (`x`, `y`), narrowed to fit `max_width`.
    pub fn fill_text_max_width(
        self,
        text: &str,
        x: impl Into<CanvasArg>,
        y: impl Into<CanvasArg>,
        max_width: f32,
    ) -> Self {
        self.text(text, x, y, Some(max_width), false)
    }

    /// Strokes the outline of `text` at (`x`, `y`).
    pub fn stroke_text(self, text: &str, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        self.text(text, x, y, None, true)
    }

    /// Strokes the outline of `text` at (`x`, `y`), narrowed to fit
    /// `max_width`.
    pub fn stroke_text_max_width(
        self,
        text: &str,
        x: impl Into<CanvasArg>,
        y: impl Into<CanvasArg>,
        max_width: f32,
    ) -> Self {
        self.text(text, x, y, Some(max_width), true)
    }

    fn text(
        mut self,
        text: &str,
        x: impl Into<CanvasArg>,
        y: impl Into<CanvasArg>,
        max_width: Option<f32>,
        stroke: bool,
    ) -> Self {
        let (text, x, y) = (text.into(), self.num(x), self.num(y));
        self.op(if stroke {
            CanvasOp::StrokeText {
                text,
                x,
                y,
                max_width,
            }
        } else {
            CanvasOp::FillText {
                text,
                x,
                y,
                max_width,
            }
        })
    }

    /// Draws the image at `uri` (a URL or `asset:/path`) into the rectangle.
    pub fn draw_image(self, uri: &str, x: f32, y: f32, w: f32, h: f32) -> Self {
        self.draw_image_part(uri, None, [x, y, w, h])
    }

    /// Draws the part `source` of the image at `uri` (`[x, y, w, h]` in
    /// image pixels; `None`: all of it) into `dest` (`[x, y, w, h]`).
    pub fn draw_image_part(self, uri: &str, source: Option<[f32; 4]>, dest: [f32; 4]) -> Self {
        self.op(CanvasOp::DrawImage {
            image: CanvasImage { uri: uri.into() },
            source,
            dest: dest.map(Num::Const),
        })
    }
}

impl From<Drawing> for Painter {
    fn from(drawing: Drawing) -> Self {
        Self {
            drawing,
            nodes: Vec::new(),
        }
    }
}

crate::static_reactive!(ScreenAnimation: ScreenAnimation);
