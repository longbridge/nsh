//! What the platform receives for a view, written in the words apps use:
//! the `prop`, `style` and `listens` steps of the fixtures' scripts
//! (compat/apps.rs) compare against it.
//!
//! The mount items carry the core's own types, which may change freely;
//! the scripts are frozen. So every key here is named as apps name it (the
//! JS prop or style key) and every value is written in a text form of the
//! JS vocabulary, one explicit line per key, never a Rust `Debug`. This
//! file's output is part of what the fixtures hold: change a key's form
//! only by adding a new key.
//!
//! Value forms:
//!
//! - numbers: decimal, at most 3 decimals, no trailing zeros (`8`, `0.5`,
//!   `-3`, `1.333`);
//! - colors: `#rrggbbaa`, lower case (`#ff0000ff`);
//! - booleans: `true` / `false`;
//! - strings and enumerations: as JS writes them (`button`, `box-none`);
//! - objects: their members as `name=value`, separated by spaces, in the
//!   order of their TS declaration; required members always, optional ones
//!   when set (not at their default); `{}` when none is;
//! - lists: their items separated by `, `;
//! - CSS-like values (`transform`, `boxShadow`, `filter`,
//!   `backgroundImage`, `transformOrigin`, timing functions): CSS's
//!   syntax with the numbers above, lengths without units, angles in
//!   `deg`, fractions in `%`.
//!
//! A key the view does not have (an optional prop not set, a style key of
//! another kind of view) is missing; a step naming a key that is missing
//! fails, `noprop` / `nostyle` pass when the key is missing or at the value
//! a view of its kind has with nothing set.

use std::{collections::BTreeMap, sync::Arc};

use nsh_core::{
    AccessibilityRole, AttributedText, BorderStyle, Checked, Color, DecorationStyle, EdgeInsets,
    EventKind, EventMask, Filter, Gradient, GradientKind, GradientSize, ImageOptions, ImageSource,
    InputConfig, KeyboardDismissMode, KeyboardTaps, Keyframes, ModalConfig, MountItem, NativeProps,
    ResizeMode, ResolvedTextStyle, ScreenConfig, ScrollBehavior, TextAlign, TextTransform,
    TransformKind, VideoProps, ViewKind, ViewProps, ViewStyle, WebProps, WebSource,
    geometry::{DefiniteLength, Percentage, Points, REM_SIZE},
    ui::PropEnum,
};

/// The platforms whose props are written `<platform>.<name>` in scripts as
/// in `api/uncovered.txt`; the platform receives them as `<name>`.
const PLATFORMS: &[&str] = &["ios.", "android.", "desktop.", "web."];

/// The last mount item of each kind a view got: what the platform shows.
#[derive(Default, Clone)]
pub(crate) struct Mounted {
    kind: Option<ViewKind>,
    style: Option<Box<ViewStyle>>,
    props: Option<Box<ViewProps>>,
    events: EventMask,
    text: Option<Arc<AttributedText>>,
    image: Option<(Option<ImageSource>, ResizeMode, ImageOptions)>,
    scroll: Option<(bool, bool, KeyboardDismissMode)>,
    behavior: Option<Box<ScrollBehavior>>,
    refreshing: bool,
    input: Option<Box<InputConfig>>,
    value: Option<String>,
    screen: Option<Box<ScreenConfig>>,
    modal: Option<ModalConfig>,
    native: Option<NativeProps>,
    /// The interaction states the platform reports
    /// ([`MountItem::TrackStates`]; bits of `nsh_core::tailwind::state`).
    tracked: u8,
    /// The keys [`Later`] reads.
    later: Later,
}

impl Mounted {
    /// Takes in `item`, one addressed to this view.
    pub(crate) fn apply(&mut self, item: &MountItem) {
        match item {
            MountItem::Create { kind, .. } => {
                *self = Mounted {
                    kind: Some(*kind),
                    ..Mounted::default()
                }
            }
            MountItem::ViewStyle { style, .. } => self.style = Some(style.clone()),
            MountItem::ViewProps { props, .. } => self.props = Some(props.clone()),
            MountItem::AccessibilityLabel { label, .. } => {
                self.props.get_or_insert_default().accessibility.label = label.clone();
            }
            MountItem::Events { events, .. } => self.events = *events,
            MountItem::Text { text, .. } => self.text = Some(text.clone()),
            MountItem::Image {
                source,
                resize_mode,
                options,
                ..
            } => self.image = Some((source.clone(), *resize_mode, options.clone())),
            MountItem::ScrollConfig {
                horizontal,
                shows_indicator,
                keyboard_dismiss,
                ..
            } => self.scroll = Some((*horizontal, *shows_indicator, *keyboard_dismiss)),
            MountItem::ScrollBehavior { behavior, .. } => self.behavior = Some(behavior.clone()),
            MountItem::Refreshing { refreshing, .. } => self.refreshing = *refreshing,
            MountItem::Input { config, .. } => self.input = Some(config.clone()),
            MountItem::InputValue { text, .. } => self.value = Some(text.clone()),
            MountItem::ScreenConfig { config, .. } => self.screen = Some(config.clone()),
            MountItem::ModalConfig { config, .. } => self.modal = Some(*config),
            MountItem::NativeProps { props, .. } => self.native = Some(props.clone()),
            MountItem::TrackStates { states, .. } => self.tracked = *states,
            _ => {}
        }
        self.later.apply(item);
    }

    /// The view an item this module reads is addressed to.
    pub(crate) fn target(item: &MountItem) -> Option<nsh_core::NodeId> {
        Some(match item {
            MountItem::Create { id, .. }
            | MountItem::ViewStyle { id, .. }
            | MountItem::ViewProps { id, .. }
            | MountItem::AccessibilityLabel { id, .. }
            | MountItem::Events { id, .. }
            | MountItem::Text { id, .. }
            | MountItem::Image { id, .. }
            | MountItem::ScrollConfig { id, .. }
            | MountItem::ScrollBehavior { id, .. }
            | MountItem::Refreshing { id, .. }
            | MountItem::Input { id, .. }
            | MountItem::InputValue { id, .. }
            | MountItem::ScreenConfig { id, .. }
            | MountItem::ModalConfig { id, .. }
            | MountItem::NativeProps { id, .. }
            | MountItem::TrackStates { id, .. }
            | MountItem::Gestures { id, .. }
            | MountItem::StickyCells { id, .. }
            | MountItem::ScrollBy { id, .. }
            | MountItem::Command { id, .. } => *id,
            _ => return None,
        })
    }

    /// A view of the same kind that got nothing but its creation.
    pub(crate) fn blank(&self) -> Mounted {
        Mounted {
            kind: self.kind,
            ..Mounted::default()
        }
    }

    /// Whether the platform reports `event` (an `on…` prop) of the view.
    pub(crate) fn listens(&self, event: EventKind) -> bool {
        self.events.contains(event)
    }

    /// Whether the platform reports all the interaction states `states`
    /// (bits of `nsh_core::tailwind::state`) of the view.
    pub(crate) fn tracks(&self, states: u8) -> bool {
        self.tracked & states == states
    }

    /// The view's props, by JS name.
    pub(crate) fn props(&self) -> Out {
        let mut out = Out::new();
        let p = self.props.as_deref().cloned().unwrap_or_default();
        let a = &p.accessibility;
        out.put("pointerEvents", p.pointer_events.js_name().into());
        out.put("hitSlop", insets(&p.hit_slop));
        if let Some(v) = p.long_press_delay {
            out.put("delayLongPress", num(v));
        }
        if let Some(r) = p.ripple {
            let mut o = Object::default();
            o.opt("color", r.color.map(color));
            o.flag("borderless", r.borderless);
            o.opt("radius", r.radius.map(num));
            o.flag("foreground", r.foreground);
            out.put("ripple", o.done());
        }
        if let Some(v) = a.accessible {
            out.put("accessible", v.to_string());
        }
        if let Some(v) = &a.label {
            out.put("accessibilityLabel", v.to_string());
        }
        if let Some(v) = &a.hint {
            out.put("accessibilityHint", v.to_string());
        }
        out.put("role", role(a.role).into());
        let s = a.state;
        let mut o = Object::default();
        o.flag("disabled", s.disabled);
        o.flag("selected", s.selected);
        o.opt(
            "checked",
            s.checked.map(|c| {
                match c {
                    Checked::False => "false",
                    Checked::True => "true",
                    Checked::Mixed => "mixed",
                }
                .to_owned()
            }),
        );
        o.flag("busy", s.busy);
        o.opt("expanded", s.expanded.map(|e| e.to_string()));
        out.put("accessibilityState", o.done());
        if let Some(v) = &a.value {
            let mut o = Object::default();
            o.opt("min", v.min.map(num));
            o.opt("max", v.max.map(num));
            o.opt("now", v.now.map(num));
            o.opt("text", v.text.as_deref().map(str::to_owned));
            out.put("accessibilityValue", o.done());
        }
        out.put("accessibilityElementsHidden", a.hidden.to_string());
        out.put("accessibilityLiveRegion", a.live_region.js_name().into());
        out.put(
            "accessibilityActions",
            list(a.actions.iter().map(|d| match &d.label {
                Some(label) => format!("{}:{label}", d.name),
                None => d.name.to_string(),
            })),
        );
        if let Some(v) = &p.test_id {
            out.put("testID", v.to_string());
        }
        if let Some(pan) = p.pan_activation {
            let mut o = Object::default();
            o.opt("activeOffsetX", pan.active_offset_x.map(num));
            o.opt("activeOffsetY", pan.active_offset_y.map(num));
            o.opt("failOffsetX", pan.fail_offset_x.map(num));
            o.opt("failOffsetY", pan.fail_offset_y.map(num));
            out.put("panActivation", o.done());
        }
        if let Some(v) = p.window_drag {
            out.put("windowDrag", v.to_string());
        }
        if let Some(v) = p.focusable {
            out.put("focusable", v.to_string());
        }
        if p.selection_area {
            out.put("selectionArea", "true".into());
        }
        let style = self.style.as_deref().cloned().unwrap_or_default();
        out.put("elevation", num(style.elevation));

        match self.kind {
            Some(ViewKind::Text) => {
                let text = self.text.as_deref().cloned().unwrap_or_default();
                out.put("numberOfLines", text.max_lines.to_string());
                out.put("ellipsizeMode", text.ellipsize.js_name().into());
                if let Some(v) = text.selectable {
                    out.put("selectable", v.to_string());
                }
            }
            Some(ViewKind::Image) => {
                let (source, resize, options) = self.image.clone().unwrap_or_default();
                if let Some(s) = source {
                    out.put("src", s.uri.to_string());
                }
                out.put("resizeMode", resize.js_name().into());
                if let Some(s) = options.placeholder {
                    out.put("defaultSource", s.uri.to_string());
                }
                out.put("blurRadius", num(options.blur_radius));
                if let Some(v) = options.fade_ms {
                    out.put("fadeDuration", num(v));
                }
            }
            Some(ViewKind::Scroll) => {
                let (horizontal, indicator, dismiss) =
                    self.scroll
                        .unwrap_or((false, true, KeyboardDismissMode::default()));
                out.put("horizontal", horizontal.to_string());
                out.put("showsIndicator", indicator.to_string());
                out.put("keyboardDismissMode", dismiss.js_name().into());
                let b = self.behavior.as_deref().cloned().unwrap_or_default();
                out.put("pagingEnabled", b.paging.to_string());
                out.put("snapToInterval", num(b.snap_interval));
                out.put(
                    "snapToOffsets",
                    list(b.snap_offsets.iter().map(|o| num(*o))),
                );
                out.put("snapToAlignment", b.snap_alignment.js_name().into());
                out.put("decelerationRate", num(b.deceleration));
                out.put("scrollEnabled", b.enabled.to_string());
                out.put("bounces", b.bounces.to_string());
                out.put("contentInset", insets(&b.content_inset));
                out.put(
                    "keyboardShouldPersistTaps",
                    match b.keyboard_taps {
                        KeyboardTaps::Never => "never",
                        KeyboardTaps::Always => "always",
                        KeyboardTaps::Handled => "handled",
                    }
                    .into(),
                );
                out.put("disableIntervalMomentum", b.one_step.to_string());
                out.put("minimumZoomScale", num(b.min_zoom));
                out.put("maximumZoomScale", num(b.max_zoom));
                out.put("bouncesZoom", b.bounces_zoom.to_string());
                out.put("pinchGestureEnabled", b.pinch.to_string());
                out.put("centerContent", b.center_content.to_string());
                out.put("refreshing", self.refreshing.to_string());
            }
            Some(ViewKind::Input) => {
                let c = self.input.as_deref().cloned().unwrap_or_default();
                if let Some(v) = &self.value {
                    out.put("value", v.clone());
                }
                out.put("placeholder", c.placeholder.clone());
                out.put("masked", c.secure.to_string());
                out.put("keyboardType", c.keyboard.js_name().into());
                out.put("keyboardAppearance", c.keyboard_appearance.js_name().into());
                out.put("returnKeyType", c.return_key.js_name().into());
                out.put("autoCapitalize", c.auto_capitalize.js_name().into());
                out.put("autoCorrect", c.auto_correct.to_string());
                out.put("multiline", c.multiline.to_string());
                if let Some(v) = c.max_length {
                    out.put("maxLength", v.to_string());
                }
                out.put("readonly", (!c.editable).to_string());
                if let Some(v) = c.submit_behavior {
                    out.put("submitBehavior", v.js_name().into());
                }
                out.put("autoFocus", c.auto_focus.to_string());
                out.put("autoComplete", auto_fill(c.auto_fill).into());
                out.put("caretHidden", c.caret_hidden.to_string());
                if let Some(v) = &c.accessory_id {
                    out.put("inputAccessoryViewID", v.to_string());
                }
                // Whether a mounted `InputAccessoryView` has the `nativeID`
                // it names.
                if c.accessory.is_some() {
                    out.put("inputAccessoryView", "true".into());
                }
            }
            Some(ViewKind::Screen) => {
                let c = self.screen.as_deref().cloned().unwrap_or_default();
                if let Some(v) = &c.title {
                    out.put("title", v.to_string());
                }
                out.put("headerShown", c.header_shown.to_string());
                out.put("headerLargeTitle", c.large_title.to_string());
                out.put("headerTransparent", c.header_transparent.to_string());
                let mut o = Object::default();
                o.opt("backgroundColor", c.background_color.map(color));
                o.opt("color", c.title_color.map(color));
                o.opt("tintColor", c.tint_color.map(color));
                out.put("headerStyle", o.done());
                if let Some(v) = &c.back_title {
                    out.put("headerBackTitle", v.to_string());
                }
                out.put("headerBackVisible", c.back_visible.to_string());
                out.put("popOnSwipe", c.gesture_enabled.to_string());
                out.put("popOnBack", (!c.prevent_back).to_string());
                out.put("animation", c.animation.js_name().into());
            }
            Some(ViewKind::Modal) => {
                let c = self.modal.unwrap_or_default();
                out.put("animation", c.animation.js_name().into());
                out.put("presentation", c.presentation.js_name().into());
                out.put("statusBarTranslucent", c.status_bar_translucent.to_string());
                out.put(
                    "supportedOrientations",
                    list(
                        [
                            (ModalConfig::PORTRAIT, "portrait"),
                            (ModalConfig::PORTRAIT_UPSIDE_DOWN, "portrait-upside-down"),
                            (ModalConfig::LANDSCAPE_LEFT, "landscape-left"),
                            (ModalConfig::LANDSCAPE_RIGHT, "landscape-right"),
                        ]
                        .into_iter()
                        .filter(|(bit, _)| c.orientations & bit != 0)
                        .map(|(_, name)| name.to_owned()),
                    ),
                );
                out.put("closeOnEscape", c.close_on_escape.to_string());
                out.put(
                    "closeOnBackdropPress",
                    c.close_on_backdrop_press.to_string(),
                );
                out.put("closeOnSwipe", c.close_on_swipe.to_string());
            }
            Some(ViewKind::WebView) => {
                let w = match &self.native {
                    Some(NativeProps::WebView(w)) => (**w).clone(),
                    _ => WebProps::default(),
                };
                if let Some(v) = web_source(&w.source) {
                    out.put("source", v);
                }
                if let Some(v) = &w.injected_js {
                    out.put("injectedJavaScript", v.to_string());
                }
                if let Some(v) = &w.injected_js_before {
                    out.put("injectedJavaScriptBeforeContentLoaded", v.to_string());
                }
                if let Some(v) = &w.user_agent {
                    out.put("userAgent", v.to_string());
                }
                out.put("scrollEnabled", w.scroll_enabled.to_string());
                out.put("javaScriptEnabled", w.javascript_enabled.to_string());
                out.put(
                    "allowsBackForwardNavigationGestures",
                    w.back_forward_gestures.to_string(),
                );
            }
            Some(ViewKind::Video) => {
                let v = match &self.native {
                    Some(NativeProps::Video(v)) => (**v).clone(),
                    _ => VideoProps::default(),
                };
                if let Some(s) = &v.source {
                    out.put("source", s.to_string());
                }
                out.put("paused", v.paused.to_string());
                out.put("muted", v.muted.to_string());
                out.put("volume", num(v.volume));
                out.put("rate", num(v.rate));
                out.put("repeat", v.repeat.to_string());
                out.put("resizeMode", v.resize_mode.js_name().into());
                out.put("controls", v.controls.to_string());
                out.put("progressUpdateInterval", num(v.progress_interval_ms));
            }
            _ => {}
        }
        self.later.props(self.kind, &mut out);
        out
    }

    /// The view's style, by JS key. For text views, the text keys are those
    /// of the run at UTF-16 offset `at` (the first run by default).
    pub(crate) fn styles(&self, at: Option<u32>) -> Out {
        let mut out = Out::new();
        let s = self.style.as_deref().cloned().unwrap_or_default();
        out.put("backgroundColor", color(s.background_color));
        out.put("borderRadius", num(s.border_radius));
        for (i, key) in [
            "borderTopLeftRadius",
            "borderTopRightRadius",
            "borderBottomRightRadius",
            "borderBottomLeftRadius",
        ]
        .into_iter()
        .enumerate()
        {
            out.put(
                key,
                num(s.corner_radius.to_array()[i].unwrap_or(s.border_radius)),
            );
        }
        out.put(
            "borderCurve",
            if s.continuous_corners {
                "continuous"
            } else {
                "circular"
            }
            .into(),
        );
        let b = s.borders;
        let widths = b.widths.to_array();
        for (i, side) in ["Top", "Right", "Bottom", "Left"].into_iter().enumerate() {
            let (width, colour) = match side {
                "Top" => ("borderTopWidth", "borderTopColor"),
                "Right" => ("borderRightWidth", "borderRightColor"),
                "Bottom" => ("borderBottomWidth", "borderBottomColor"),
                _ => ("borderLeftWidth", "borderLeftColor"),
            };
            out.put(width, num(widths[i]));
            out.put(colour, color(b.colors[i]));
        }
        if widths.iter().all(|w| *w == widths[0]) {
            out.put("borderWidth", num(widths[0]));
        }
        if b.colors.iter().all(|c| *c == b.colors[0]) {
            out.put("borderColor", color(b.colors[0]));
        }
        out.put("borderStyle", border_style(b.style).into());
        out.put("opacity", num(s.opacity));
        out.put(
            "overflow",
            if s.overflow_hidden {
                "hidden"
            } else {
                "visible"
            }
            .into(),
        );
        out.put("shadowColor", color(s.shadow.color));
        out.put("shadowOffset", size(s.shadow.offset_x, s.shadow.offset_y));
        out.put("shadowOpacity", num(s.shadow.opacity));
        out.put("shadowRadius", num(s.shadow.radius));
        out.put("transform", transform(&s));
        if let Some(c) = s.tint_color {
            out.put("tintColor", color(c));
        }
        let origin = |o: DefiniteLength| match o {
            DefiniteLength::Absolute(v) => num(v.to_points(REM_SIZE)),
            DefiniteLength::Fraction(f) => format!("{}%", num(f.0 * 100.0)),
        };
        out.put(
            "transformOrigin",
            format!(
                "{} {} {}",
                origin(s.transform_origin.x),
                origin(s.transform_origin.y),
                num(s.transform_origin.z)
            ),
        );
        out.put(
            "backfaceVisibility",
            if s.backface_hidden {
                "hidden"
            } else {
                "visible"
            }
            .into(),
        );
        let effects = s.effects.as_deref().cloned().unwrap_or_default();
        out.put(
            "boxShadow",
            or_none(list(effects.box_shadows.iter().map(|b| {
                format!(
                    "{}{} {} {} {} {}",
                    if b.inset { "inset " } else { "" },
                    num(b.offset_x),
                    num(b.offset_y),
                    num(b.blur),
                    num(b.spread),
                    color(b.color)
                )
            }))),
        );
        out.put(
            "backgroundImage",
            or_none(list(effects.background_images.iter().map(gradient))),
        );
        out.put("filter", filters(&effects.filters));
        out.put("backdropFilter", filters(&effects.backdrop_filters));
        out.put("mixBlendMode", s.blend_mode.name().into());
        out.put("outlineWidth", num(s.outline.width));
        out.put("outlineColor", color(s.outline.color));
        out.put("outlineStyle", border_style(s.outline.style).into());
        out.put("outlineOffset", num(s.outline.offset));
        out.put(
            "visibility",
            if s.hidden { "hidden" } else { "visible" }.into(),
        );
        out.put("cursor", s.cursor.name().into());
        if let Some(t) = s.transition {
            let p = t.properties;
            use nsh_core::TransitionProps as T;
            out.put(
                "transitionProperty",
                if p.0 == T::ALL {
                    "all".into()
                } else if p.0 == 0 {
                    "none".into()
                } else {
                    list(
                        [
                            (T::COLORS, "colors"),
                            (T::OPACITY, "opacity"),
                            (T::TRANSFORM, "transform"),
                            (T::SHADOW, "shadow"),
                            (T::SHAPE, "shape"),
                        ]
                        .into_iter()
                        .filter(|(bit, _)| p.has(*bit))
                        .map(|(_, name)| name.to_owned()),
                    )
                },
            );
            out.put("transitionDuration", num(t.duration));
            out.put("transitionDelay", num(t.delay));
            out.put("transitionTimingFunction", bezier(t.easing));
        }
        if let Some(a) = s.animation {
            out.put(
                "animationName",
                match a.keyframes {
                    Keyframes::Spin => "spin",
                    Keyframes::Ping => "ping",
                    Keyframes::Pulse => "pulse",
                    Keyframes::Bounce => "bounce",
                }
                .into(),
            );
            out.put("animationDuration", num(a.duration));
            out.put("animationDelay", num(a.delay));
            out.put("animationTimingFunction", bezier(a.easing));
            out.put(
                "animationIterationCount",
                a.iterations
                    .map_or("infinite".to_owned(), |n| n.to_string()),
            );
        }

        let (text, align) = match self.kind {
            Some(ViewKind::Text) => {
                let text = self.text.as_deref().cloned().unwrap_or_default();
                let run = match at {
                    Some(at) => text
                        .spans
                        .iter()
                        .find(|s| s.start <= at && at < s.end)
                        .map(|s| s.style),
                    None => text.spans.first().map(|s| s.style),
                };
                if let Some(shadow) = text.shadow {
                    out.put("textShadowColor", color(shadow.color));
                    out.put("textShadowOffset", size(shadow.offset_x, shadow.offset_y));
                    out.put("textShadowRadius", num(shadow.radius));
                }
                out.put(
                    "writingDirection",
                    match text.writing_direction {
                        None => "auto",
                        Some(true) => "rtl",
                        Some(false) => "ltr",
                    }
                    .into(),
                );
                if let Some(v) = text.selection_color {
                    out.put("selectionColor", color(v));
                }
                (run.unwrap_or_default(), text.align)
            }
            Some(ViewKind::Input) => {
                let c = self.input.as_deref().cloned().unwrap_or_default();
                if let Some(v) = c.placeholder_color {
                    out.put("placeholderColor", color(v));
                }
                if let Some(v) = c.selection_color {
                    out.put("selectionColor", color(v));
                }
                (c.style, c.align)
            }
            _ => return out,
        };
        text_style(&text, &mut out);
        out.put(
            "textAlign",
            match align {
                TextAlign::Auto => "auto",
                TextAlign::Left => "left",
                TextAlign::Center => "center",
                TextAlign::Right => "right",
                TextAlign::Justify => "justify",
            }
            .into(),
        );
        out
    }
}

/// The key a script's `name` reads: a platform's props are named with
/// their platform (`ios.keyboardAppearance`) or without, and a few keys
/// have a second JS name.
pub(crate) fn key(name: &str) -> &str {
    let name = PLATFORMS
        .iter()
        .find_map(|p| name.strip_prefix(p))
        .unwrap_or(name);
    match name {
        "textContentType" => "autoComplete",
        "experimental_backgroundImage" => "backgroundImage",
        other => other,
    }
}

fn text_style(t: &ResolvedTextStyle, out: &mut Out) {
    out.put("color", color(t.color));
    out.put("fontSize", num(t.font_size));
    out.put("fontWeight", t.font_weight.to_string());
    out.put("lineHeight", num(t.line_height));
    if let Some(family) = t.font_family {
        out.put("fontFamily", family.name().to_string());
    }
    out.put(
        "fontStyle",
        if t.italic { "italic" } else { "normal" }.into(),
    );
    out.put("letterSpacing", num(t.letter_spacing));
    let d = t.decoration;
    out.put(
        "textDecorationLine",
        match (d.underline, d.line_through) {
            (false, false) => "none",
            (true, false) => "underline",
            (false, true) => "line-through",
            (true, true) => "underline line-through",
        }
        .into(),
    );
    if let Some(c) = t.decoration_color {
        out.put("textDecorationColor", color(c));
    }
    out.put(
        "textDecorationStyle",
        match t.decoration_style {
            DecorationStyle::Solid => "solid",
            DecorationStyle::Double => "double",
            DecorationStyle::Dotted => "dotted",
            DecorationStyle::Dashed => "dashed",
            DecorationStyle::Wavy => "wavy",
        }
        .into(),
    );
    if let Some(v) = t.decoration_thickness {
        out.put("textDecorationThickness", num(v));
    }
    if let Some(v) = t.underline_offset {
        out.put("textUnderlineOffset", num(v));
    }
    out.put(
        "textTransform",
        match t.transform {
            TextTransform::None => "none",
            TextTransform::Uppercase => "uppercase",
            TextTransform::Lowercase => "lowercase",
            TextTransform::Capitalize => "capitalize",
        }
        .into(),
    );
    let v = t.font_variant;
    use nsh_core::FontVariant as F;
    out.put(
        "fontVariant",
        list(
            [
                (F::SMALL_CAPS, "small-caps"),
                (F::OLDSTYLE_NUMS, "oldstyle-nums"),
                (F::LINING_NUMS, "lining-nums"),
                (F::TABULAR_NUMS, "tabular-nums"),
                (F::PROPORTIONAL_NUMS, "proportional-nums"),
            ]
            .into_iter()
            .filter(|(bit, _)| v.has(*bit))
            .map(|(_, name)| name.to_owned()),
        ),
    );
}

/// Values by key.
pub(crate) type Out = BTreeMap<&'static str, String>;

trait Put {
    fn put(&mut self, key: &'static str, value: String);
}

impl Put for Out {
    fn put(&mut self, key: &'static str, value: String) {
        self.insert(key, value);
    }
}

/// An object's members, as [the module](self) writes objects.
#[derive(Default)]
struct Object(Vec<String>);

impl Object {
    fn opt(&mut self, name: &str, value: Option<String>) {
        if let Some(v) = value {
            self.0.push(format!("{name}={v}"));
        }
    }

    fn flag(&mut self, name: &str, value: bool) {
        if value {
            self.0.push(format!("{name}=true"));
        }
    }

    fn done(self) -> String {
        if self.0.is_empty() {
            "{}".into()
        } else {
            self.0.join(" ")
        }
    }
}

/// What `num` writes: a bare number, or a length in points.
pub(crate) trait Num {
    fn value(self) -> f32;
}

impl Num for f32 {
    fn value(self) -> f32 {
        self
    }
}

impl Num for Points {
    fn value(self) -> f32 {
        self.0
    }
}

/// A number with at most 3 decimals, without trailing zeros.
pub(crate) fn num(v: impl Num) -> String {
    let v = v.value();
    let r = (v as f64 * 1000.0).round() / 1000.0;
    // `-0` reads as `0`.
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

/// `#rrggbbaa`.
pub(crate) fn color(c: Color) -> String {
    format!("#{:08x}", c.0)
}

fn list(items: impl Iterator<Item = String>) -> String {
    items.collect::<Vec<_>>().join(", ")
}

fn or_none(s: String) -> String {
    if s.is_empty() { "none".into() } else { s }
}

/// `{ width, height }`: both members are required.
fn size(width: impl Num, height: impl Num) -> String {
    format!("width={} height={}", num(width), num(height))
}

/// `{ top?, right?, bottom?, left? }`.
fn insets(e: &EdgeInsets) -> String {
    let mut o = Object::default();
    for (name, v) in [
        ("top", e.top),
        ("right", e.right),
        ("bottom", e.bottom),
        ("left", e.left),
    ] {
        o.opt(name, (v.0 != 0.0).then(|| num(v)));
    }
    o.done()
}

fn bezier(e: [f32; 4]) -> String {
    format!(
        "cubic-bezier({}, {}, {}, {})",
        num(e[0]),
        num(e[1]),
        num(e[2]),
        num(e[3])
    )
}

fn border_style(s: BorderStyle) -> &'static str {
    match s {
        BorderStyle::Solid => "solid",
        BorderStyle::Dashed => "dashed",
        BorderStyle::Dotted => "dotted",
    }
}

fn role(r: AccessibilityRole) -> &'static str {
    r.js_name()
}

/// `transform` as CSS writes it, `none` when empty.
fn transform(s: &ViewStyle) -> String {
    let ops: Vec<String> = s
        .transform
        .ops()
        .iter()
        .map(|op| {
            let v = num(op.value);
            match op.kind {
                TransformKind::TranslateX => format!("translateX({v})"),
                TransformKind::TranslateY => format!("translateY({v})"),
                TransformKind::Scale => format!("scale({v})"),
                TransformKind::ScaleX => format!("scaleX({v})"),
                TransformKind::ScaleY => format!("scaleY({v})"),
                TransformKind::Rotate => format!("rotate({v}deg)"),
                TransformKind::RotateX => format!("rotateX({v}deg)"),
                TransformKind::RotateY => format!("rotateY({v}deg)"),
                TransformKind::SkewX => format!("skewX({v}deg)"),
                TransformKind::SkewY => format!("skewY({v}deg)"),
                TransformKind::Perspective => format!("perspective({v})"),
                TransformKind::TranslateXFraction => {
                    format!("translateX({}%)", num(op.value * 100.0))
                }
                TransformKind::TranslateYFraction => {
                    format!("translateY({}%)", num(op.value * 100.0))
                }
            }
        })
        .collect();
    if ops.is_empty() {
        "none".into()
    } else {
        ops.join(" ")
    }
}

fn filters(list: &[Filter]) -> String {
    if list.is_empty() {
        return "none".into();
    }
    list.iter()
        .map(|f| match *f {
            Filter::Blur(v) => format!("blur({})", num(v)),
            Filter::Brightness(v) => format!("brightness({})", num(v)),
            Filter::Contrast(v) => format!("contrast({})", num(v)),
            Filter::Grayscale(v) => format!("grayscale({})", num(v)),
            Filter::HueRotate(v) => format!("hue-rotate({}deg)", num(v.to_degrees())),
            Filter::Invert(v) => format!("invert({})", num(v)),
            Filter::Opacity(v) => format!("opacity({})", num(v)),
            Filter::Saturate(v) => format!("saturate({})", num(v)),
            Filter::Sepia(v) => format!("sepia({})", num(v)),
            Filter::DropShadow {
                offset_x,
                offset_y,
                blur,
                color: c,
            } => format!(
                "drop-shadow({} {} {} {})",
                num(offset_x),
                num(offset_y),
                num(blur),
                color(c)
            ),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn gradient(g: &Gradient) -> String {
    let at =
        |x: Percentage, y: Percentage| format!("at {}% {}%", num(x.0 * 100.0), num(y.0 * 100.0));
    let (name, head) = match g.kind {
        GradientKind::Linear { angle } => {
            ("linear-gradient", format!("{}deg", num(angle.to_degrees())))
        }
        GradientKind::LinearCorner { right, bottom } => (
            "linear-gradient",
            format!(
                "to {} {}",
                if bottom { "bottom" } else { "top" },
                if right { "right" } else { "left" }
            ),
        ),
        GradientKind::Radial { circle, size, x, y } => (
            "radial-gradient",
            format!(
                "{} {} {}",
                if circle { "circle" } else { "ellipse" },
                match size {
                    GradientSize::ClosestSide => "closest-side",
                    GradientSize::ClosestCorner => "closest-corner",
                    GradientSize::FarthestSide => "farthest-side",
                    GradientSize::FarthestCorner => "farthest-corner",
                },
                at(x, y)
            ),
        ),
        GradientKind::Conic { from, x, y } => (
            "conic-gradient",
            format!("from {}deg {}", num(from.to_degrees()), at(x, y)),
        ),
    };
    let stops = g.stops.iter().map(|s| match s.position {
        Some(DefiniteLength::Fraction(p)) => {
            format!("{} {}%", color(s.color), num(p.0 * 100.0))
        }
        Some(DefiniteLength::Absolute(l)) => {
            format!("{} {}", color(s.color), num(l.to_points(REM_SIZE)))
        }
        None => color(s.color),
    });
    format!(
        "{}{name}({head}, {})",
        if g.repeating { "repeating-" } else { "" },
        list(stops)
    )
}

fn web_source(s: &WebSource) -> Option<String> {
    match s {
        WebSource::None => None,
        WebSource::Uri(uri) => Some(uri.to_string()),
        WebSource::Html { html, base_url } => {
            let mut o = Object::default();
            o.opt("html", Some(html.to_string()));
            o.opt("baseUrl", base_url.as_deref().map(str::to_owned));
            Some(o.done())
        }
    }
}

/// `autoComplete`'s value, as React Native names it.
fn auto_fill(a: nsh_core::AutoFill) -> &'static str {
    use nsh_core::AutoFill as A;
    match a {
        A::Default => "default",
        A::Off => "off",
        A::Username => "username",
        A::Password => "password",
        A::NewPassword => "new-password",
        A::OneTimeCode => "one-time-code",
        A::Email => "email",
        A::Tel => "tel",
        A::Name => "name",
        A::GivenName => "given-name",
        A::FamilyName => "family-name",
        A::MiddleName => "additional-name",
        A::Nickname => "nickname",
        A::Organization => "organization",
        A::StreetAddress => "street-address",
        A::AddressLine1 => "address-line1",
        A::AddressLine2 => "address-line2",
        A::City => "address-level2",
        A::Region => "address-level1",
        A::PostalCode => "postal-code",
        A::Country => "country",
        A::CreditCardNumber => "cc-number",
        A::CreditCardExpiration => "cc-exp",
        A::CreditCardSecurityCode => "cc-csc",
        A::CreditCardName => "cc-name",
        A::Url => "url",
        A::Birthdate => "birthdate",
    }
}

/// The forms of values are what the scripts hold: they never change.
#[test]
fn values_are_written_as_apps_write_them() {
    assert_eq!(num(8.0), "8");
    assert_eq!(num(0.5), "0.5");
    assert_eq!(num(-3.0), "-3");
    assert_eq!(num(-0.0), "0");
    assert_eq!(num(1.0 / 3.0), "0.333");
    assert_eq!(num(0.1), "0.1");
    assert_eq!(color(Color::rgba(255, 0, 16, 128)), "#ff001080");
    let mut style = ViewStyle::default();
    style.transform.push(TransformKind::Rotate, 45.0);
    style.transform.push(TransformKind::Scale, 2.0);
    style.transform.push(TransformKind::TranslateXFraction, 0.5);
    assert_eq!(transform(&style), "rotate(45deg) scale(2) translateX(50%)");
    assert_eq!(
        insets(&EdgeInsets {
            top: Points(4.0),
            left: Points(2.0),
            ..Default::default()
        }),
        "top=4 left=2"
    );
    assert_eq!(insets(&EdgeInsets::default()), "{}");
    assert_eq!(key("ios.keyboardAppearance"), "keyboardAppearance");
    let view = Mounted {
        kind: Some(ViewKind::View),
        ..Default::default()
    };
    let styles = view.styles(None);
    assert_eq!(styles["backgroundColor"], "#00000000");
    assert_eq!(styles["opacity"], "1");
    assert_eq!(styles["shadowOffset"], "width=0 height=-3");
    assert_eq!(styles["transformOrigin"], "50% 50% 0");
    assert_eq!(view.props()["accessibilityState"], "{}");
}

/// Keys added after the ones above, read from what the platform is told
/// to do with a view rather than what it is set to:
///
/// - `contentOffset` (scroll views and lists): where the platform holds
///   the view scrolled, `{ x?, y? }`: the last `ScrollTo` it was told
///   (`contentOffset`, a list starting at its end, `scrollTo()`), moved
///   by the `ScrollBy`s since and set by the script's `scroll` steps (the
///   user); missing until one of them;
/// - `zoomScale` (scroll views): the zoom the platform was last told;
/// - `stickyHeaderIndices` (lists): the sticky cells as the platform pins
///   them, `start=<n> end=<n>` each (content points along the main axis,
///   `end` `Infinity` for the last), in the core's order;
/// - `scrollValue`, `pan` (`{ x?, y? }`), `pinch` (`{ scale? }`),
///   `rotation` (`{ angle? }`): the animated values the scroll offset
///   and the gestures drive, each written as the value it was created
///   with (so a script tells them apart);
/// - `selectedRange` (inputs): the selection the platform was last told
///   to make (`selectedRange`, a ref's `setSelection`), `start=<n> end=<n>`
///   in UTF-16 units; missing until one;
/// - the status bar (the root, the `statusbar` step): what the platform
///   was last told to show (`Command::StatusBar`: `nsh.json`'s default
///   under the pages' and dialogs' `statusBar`, field by field), as
///   [`Mounted::status_bar`] writes it; missing until it is told.
#[derive(Default, Clone)]
struct Later {
    offset: Option<(f32, f32)>,
    selection: Option<(u32, u32)>,
    zoom: Option<f32>,
    sticky: Option<Vec<(f32, f32)>>,
    /// `pan.x`, `pan.y`, `pinch.scale`, `rotation.angle`, by animated
    /// node.
    gestures: Option<[Option<nsh_core::animation::NodeId>; 4]>,
    scroll_value: Option<nsh_core::animation::NodeId>,
    status_bar: Option<nsh_core::StatusBarConfig>,
}

impl Later {
    fn apply(&mut self, item: &MountItem) {
        match item {
            MountItem::Command { command, .. } => match *command {
                nsh_core::Command::StatusBar(config) => self.status_bar = Some(config),
                nsh_core::Command::ScrollTo { x, y, .. } => self.offset = Some((x.0, y.0)),
                nsh_core::Command::ZoomTo { scale, .. } => self.zoom = Some(scale),
                nsh_core::Command::SetSelection { start, end } => {
                    self.selection = Some((start, end));
                }
                _ => {}
            },
            MountItem::ScrollBy { dx, dy, .. } => {
                let (x, y) = self.offset.unwrap_or_default();
                self.offset = Some((x + dx.0, y + dy.0));
            }
            MountItem::StickyCells { cells, .. } => {
                self.sticky = Some(cells.iter().map(|c| (c.start.0, c.end.0)).collect());
            }
            MountItem::Gestures { values, .. } => {
                self.gestures = Some([values.pan_x, values.pan_y, values.pinch, values.rotation]);
            }
            MountItem::ScrollConfig { scroll_value, .. } => self.scroll_value = *scroll_value,
            _ => {}
        }
    }

    fn props(&self, kind: Option<ViewKind>, out: &mut Out) {
        let animated = |node: Option<nsh_core::animation::NodeId>| {
            let node = node?;
            ANIMATED.with(|a| a.borrow().get(&node).map(|v| num(*v)))
        };
        if kind == Some(ViewKind::Scroll) {
            if let Some((x, y)) = self.offset {
                let mut o = Object::default();
                o.opt("x", (x != 0.0).then(|| num(x)));
                o.opt("y", (y != 0.0).then(|| num(y)));
                out.put("contentOffset", o.done());
            }
            if let Some(v) = self.zoom {
                out.put("zoomScale", num(v));
            }
            if let Some(cells) = &self.sticky {
                out.put(
                    "stickyHeaderIndices",
                    list(cells.iter().map(|(s, e)| {
                        let end = if e.is_finite() {
                            num(*e)
                        } else {
                            "Infinity".into()
                        };
                        format!("start={} end={end}", num(*s))
                    })),
                );
            }
            if let Some(v) = animated(self.scroll_value) {
                out.put("scrollValue", v);
            }
        }
        if let Some((start, end)) = self.selection {
            let mut o = Object::default();
            o.opt("start", Some(start.to_string()));
            o.opt("end", Some(end.to_string()));
            out.put("selectedRange", o.done());
        }
        if let Some([x, y, scale, angle]) = self.gestures {
            let mut o = Object::default();
            o.opt("x", animated(x));
            o.opt("y", animated(y));
            if x.is_some() || y.is_some() {
                out.put("pan", o.done());
            }
            if scale.is_some() {
                let mut o = Object::default();
                o.opt("scale", animated(scale));
                out.put("pinch", o.done());
            }
            if angle.is_some() {
                let mut o = Object::default();
                o.opt("angle", animated(angle));
                out.put("rotation", o.done());
            }
        }
    }
}

impl Mounted {
    /// Whether the view is a scroll view (a list's included).
    pub(crate) fn is_scroll(&self) -> bool {
        self.kind == Some(ViewKind::Scroll)
    }

    /// The status bar the platform was last told to show (on the root):
    /// `foreground` (`auto`, `light`, `dark`, as `StatusBarForeground`
    /// names them), `hidden` and `animated`, each always written; `None`
    /// until the platform is told.
    pub(crate) fn status_bar(&self) -> Option<Out> {
        let config = self.later.status_bar?;
        let mut out = Out::new();
        out.put(
            "foreground",
            match config.style {
                nsh_core::StatusBarStyle::Default => "auto",
                nsh_core::StatusBarStyle::Light => "light",
                nsh_core::StatusBarStyle::Dark => "dark",
            }
            .into(),
        );
        out.put("hidden", config.hidden.to_string());
        out.put("animated", config.animated.to_string());
        Some(out)
    }

    /// The user scrolled the view to `x`, `y` (a script's `scroll` step).
    pub(crate) fn scrolled(&mut self, x: f32, y: f32) {
        self.later.offset = Some((x, y));
    }
}

thread_local! {
    /// The value each animated node of the run on this thread was created
    /// with: the mount items create them before views bind them.
    static ANIMATED: std::cell::RefCell<std::collections::HashMap<nsh_core::animation::NodeId, f32>> =
        Default::default();
}

/// Takes in the animated values `item` creates; `None` starts a run.
pub(crate) fn animated(item: Option<&MountItem>) {
    ANIMATED.with(|a| match item {
        None => a.borrow_mut().clear(),
        Some(MountItem::Animation(nsh_core::animation::AnimationOp::CreateValue {
            node,
            value,
        })) => {
            a.borrow_mut().insert(*node, *value);
        }
        Some(_) => {}
    });
}
