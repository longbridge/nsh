//! Mount batches as the JSON the DOM mounter (`web/nsh.js`, `apply`)
//! reads: one array per batch, one array per item, `[op, id, ...]`. Visual
//! styles are turned into CSS here, so JS only assigns them.
//!
//! Native animation items stay in Rust ([`crate::animation::UiAnimation`]); the DOM
//! gets their results each frame.

use std::fmt::Write as _;

use nsh_core::{
    BorderStyle, Color, Command, EventKind, Filter, Gradient, GradientKind, GradientSize,
    ImageSource, InputConfig, Keyframes, LayoutCurve, LayoutTransition, MountItem, NativeProps,
    OriginLen, ResizeMode, ScrollBehavior, Transform, TransformKind, TransitionProps, ViewKind,
    ViewProps, ViewStyle, WebSource, ui::JsValue,
};

use crate::text::{WebLayout, css_font};

/// JSON output.
pub(crate) struct Out {
    pub(crate) s: String,
    first: bool,
}

impl Out {
    pub(crate) fn new() -> Self {
        Out {
            s: String::from("["),
            first: true,
        }
    }

    /// Starts item `op`.
    fn item(&mut self, op: &str) -> &mut Self {
        if !self.first {
            self.s.push(',');
        }
        self.first = false;
        self.s.push_str("[\"");
        self.s.push_str(op);
        self.s.push('"');
        self
    }

    fn end(&mut self) {
        self.s.push(']');
    }

    fn num(&mut self, n: impl Into<f64>) -> &mut Self {
        let n = n.into();
        self.s.push(',');
        if n.is_finite() {
            // Points need no more than 1/1000 precision.
            let r = (n * 1000.0).round() / 1000.0;
            let _ = write!(self.s, "{r}");
        } else {
            self.s.push_str("null");
        }
        self
    }

    fn str(&mut self, v: &str) -> &mut Self {
        self.s.push(',');
        push_str(&mut self.s, v);
        self
    }

    fn opt_str(&mut self, v: Option<&str>) -> &mut Self {
        match v {
            Some(v) => self.str(v),
            None => self.raw("null"),
        }
    }

    fn bool(&mut self, v: bool) -> &mut Self {
        self.raw(if v { "true" } else { "false" })
    }

    fn raw(&mut self, json: &str) -> &mut Self {
        self.s.push(',');
        self.s.push_str(json);
        self
    }

    pub(crate) fn finish(mut self) -> String {
        self.s.push(']');
        self.s
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.first
    }

    /// An animation output: `["a", id, prop, value]`.
    pub(crate) fn animation(&mut self, id: u32, prop: &str, value: &str) {
        self.item("a").num(id).str(prop).str(value).end();
    }
}

fn push_str(out: &mut String, v: &str) {
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn json_str(v: &str) -> String {
    let mut s = String::new();
    push_str(&mut s, v);
    s
}

/// A CSS color.
pub(crate) fn css_color(c: Color) -> String {
    let v = c.0;
    let (r, g, b, a) = (v >> 24, (v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff);
    if a == 255 {
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    } else {
        format!("rgba({r},{g},{b},{:.3})", a as f32 / 255.0)
    }
}

fn with_alpha(c: Color, opacity: f32) -> Color {
    let a = ((c.0 & 0xff) as f32 * opacity).round().clamp(0.0, 255.0) as u32;
    Color((c.0 & !0xff) | a)
}

/// `FooBar` (a `Debug` name) as `foo-bar`.
fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn enum_name(v: &impl JsValue) -> String {
    match v.to_json() {
        nsh_core::JsonValue::String(s) => s,
        other => other.to_string(),
    }
}

/// The CSS transform functions of a transform list (`value(i)` overrides
/// entry `i`).
pub(crate) fn css_transform(t: &Transform, value: impl Fn(usize) -> Option<f32>) -> String {
    let mut out = String::new();
    for (i, op) in t.ops().iter().enumerate() {
        let v = value(i).unwrap_or(op.value);
        let _ = match op.kind {
            TransformKind::TranslateX => write!(out, "translateX({v}px) "),
            TransformKind::TranslateY => write!(out, "translateY({v}px) "),
            TransformKind::TranslateXFraction => write!(out, "translateX({}%) ", v * 100.0),
            TransformKind::TranslateYFraction => write!(out, "translateY({}%) ", v * 100.0),
            TransformKind::Scale => write!(out, "scale({v}) "),
            TransformKind::ScaleX => write!(out, "scaleX({v}) "),
            TransformKind::ScaleY => write!(out, "scaleY({v}) "),
            TransformKind::Rotate => write!(out, "rotate({v}deg) "),
            TransformKind::RotateX => write!(out, "rotateX({v}deg) "),
            TransformKind::RotateY => write!(out, "rotateY({v}deg) "),
            TransformKind::SkewX => write!(out, "skewX({v}deg) "),
            TransformKind::SkewY => write!(out, "skewY({v}deg) "),
            TransformKind::Perspective => write!(out, "perspective({v}px) "),
        };
    }
    out
}

fn origin(o: OriginLen) -> String {
    match o {
        OriginLen::Px(v) => format!("{v}px"),
        OriginLen::Fraction(f) => format!("{}%", f * 100.0),
    }
}

fn gradient(g: &Gradient) -> String {
    let stops: Vec<String> = g
        .resolved_stops()
        .iter()
        .map(|(c, p)| format!("{} {}%", css_color(*c), p * 100.0))
        .collect();
    let stops = stops.join(", ");
    let repeating = if g.repeating { "repeating-" } else { "" };
    match g.kind {
        GradientKind::Linear { angle } => {
            format!("{repeating}linear-gradient({angle}deg, {stops})")
        }
        GradientKind::LinearCorner { right, bottom } => format!(
            "{repeating}linear-gradient(to {} {}, {stops})",
            if bottom { "bottom" } else { "top" },
            if right { "right" } else { "left" }
        ),
        GradientKind::Radial { circle, size, x, y } => {
            let size = match size {
                GradientSize::ClosestSide => "closest-side",
                GradientSize::ClosestCorner => "closest-corner",
                GradientSize::FarthestSide => "farthest-side",
                GradientSize::FarthestCorner => "farthest-corner",
            };
            format!(
                "{repeating}radial-gradient({} {size} at {}% {}%, {stops})",
                if circle { "circle" } else { "ellipse" },
                x * 100.0,
                y * 100.0
            )
        }
        GradientKind::Conic { from, x, y } => format!(
            "{repeating}conic-gradient(from {from}deg at {}% {}%, {stops})",
            x * 100.0,
            y * 100.0
        ),
    }
}

fn filters(list: &[Filter]) -> String {
    let mut out = String::new();
    for f in list {
        let _ = match *f {
            Filter::Blur(v) => write!(out, "blur({v}px) "),
            Filter::Brightness(v) => write!(out, "brightness({v}) "),
            Filter::Contrast(v) => write!(out, "contrast({v}) "),
            Filter::Grayscale(v) => write!(out, "grayscale({v}) "),
            Filter::HueRotate(v) => write!(out, "hue-rotate({v}deg) "),
            Filter::Invert(v) => write!(out, "invert({v}) "),
            Filter::Opacity(v) => write!(out, "opacity({v}) "),
            Filter::Saturate(v) => write!(out, "saturate({v}) "),
            Filter::Sepia(v) => write!(out, "sepia({v}) "),
            Filter::DropShadow {
                offset_x,
                offset_y,
                blur,
                color,
            } => write!(
                out,
                "drop-shadow({offset_x}px {offset_y}px {blur}px {}) ",
                css_color(color)
            ),
        };
    }
    out
}

fn bezier(e: [f32; 4]) -> String {
    format!("cubic-bezier({},{},{},{})", e[0], e[1], e[2], e[3])
}

/// The CSS of a view's visual style, and of its border overlay (drawn
/// over the children, as UIKit draws layer borders; also keeps borders
/// out of the content box, which layout already placed).
pub(crate) fn view_css(s: &ViewStyle) -> (String, Option<String>) {
    let mut css = String::new();
    if !s.background_color.is_transparent() {
        let _ = write!(css, "background-color:{};", css_color(s.background_color));
    }
    let radii: [f32; 4] = std::array::from_fn(|i| s.corner_radius[i].unwrap_or(s.border_radius));
    let radius = if radii.iter().any(|&r| r > 0.0) {
        format!(
            "border-radius:{}px {}px {}px {}px;",
            radii[0], radii[1], radii[2], radii[3]
        )
    } else {
        String::new()
    };
    css.push_str(&radius);
    if s.opacity < 1.0 {
        let _ = write!(css, "opacity:{};", s.opacity);
    }
    if s.overflow_hidden {
        css.push_str("overflow:hidden;");
    }
    let mut shadows = Vec::new();
    if s.shadow.opacity > 0.0 && !s.shadow.color.is_transparent() {
        shadows.push(format!(
            "{}px {}px {}px {}",
            s.shadow.offset_x,
            s.shadow.offset_y,
            s.shadow.radius,
            css_color(with_alpha(s.shadow.color, s.shadow.opacity))
        ));
    }
    if let Some(effects) = &s.effects {
        for b in &effects.box_shadows {
            shadows.push(format!(
                "{}{}px {}px {}px {}px {}",
                if b.inset { "inset " } else { "" },
                b.offset_x,
                b.offset_y,
                b.blur,
                b.spread,
                css_color(b.color)
            ));
        }
        if !effects.background_images.is_empty() {
            let images: Vec<String> = effects.background_images.iter().map(gradient).collect();
            let _ = write!(css, "background-image:{};", images.join(","));
        }
        if !effects.filters.is_empty() {
            let _ = write!(css, "filter:{};", filters(&effects.filters));
        }
        if !effects.backdrop_filters.is_empty() {
            let f = filters(&effects.backdrop_filters);
            let _ = write!(css, "backdrop-filter:{f};-webkit-backdrop-filter:{f};");
        }
    }
    if !shadows.is_empty() {
        let _ = write!(css, "box-shadow:{};", shadows.join(","));
    }
    if !s.transform.is_identity() {
        let _ = write!(
            css,
            "transform:{};",
            css_transform(&s.transform, |_| None).trim_end()
        );
    }
    let o = s.transform_origin;
    if o.x != OriginLen::Fraction(0.5) || o.y != OriginLen::Fraction(0.5) {
        let _ = write!(css, "transform-origin:{} {};", origin(o.x), origin(o.y));
    }
    if s.backface_hidden {
        css.push_str("backface-visibility:hidden;");
    }
    let blend = kebab(&format!("{:?}", s.blend_mode));
    if blend != "normal" {
        let _ = write!(css, "mix-blend-mode:{blend};");
    }
    if s.outline.is_visible() {
        let _ = write!(
            css,
            "outline:{}px {} {};outline-offset:{}px;",
            s.outline.width,
            border_style(s.outline.style),
            css_color(s.outline.color),
            s.outline.offset
        );
    }
    if s.hidden {
        css.push_str("visibility:hidden;");
    }
    let cursor = kebab(&format!("{:?}", s.cursor));
    if cursor != "auto" {
        let _ = write!(css, "cursor:{cursor};");
    }
    if let Some(t) = &s.transition {
        let mut props = Vec::new();
        let p = t.properties;
        if p.has(TransitionProps::COLORS) {
            props.extend(["background-color", "border-color", "color"]);
        }
        if p.has(TransitionProps::OPACITY) {
            props.push("opacity");
        }
        if p.has(TransitionProps::TRANSFORM) {
            props.push("transform");
        }
        if p.has(TransitionProps::SHADOW) {
            props.push("box-shadow");
        }
        if p.has(TransitionProps::SHAPE) {
            props.push("border-radius");
        }
        let _ = write!(
            css,
            "transition-property:{};transition-duration:{}ms;transition-delay:{}ms;transition-timing-function:{};",
            props.join(","),
            t.duration,
            t.delay,
            bezier(t.easing)
        );
    }
    if let Some(a) = &s.animation {
        let name = match a.keyframes {
            Keyframes::Spin => "nsh-spin",
            Keyframes::Ping => "nsh-ping",
            Keyframes::Pulse => "nsh-pulse",
            Keyframes::Bounce => "nsh-bounce",
        };
        let iterations = a
            .iterations
            .map_or("infinite".to_owned(), |n| n.to_string());
        let _ = write!(
            css,
            "animation:{name} {}ms {} {}ms {iterations};",
            a.duration,
            bezier(a.easing),
            a.delay
        );
    }
    let b = &s.borders;
    let border = if b.widths.iter().any(|&w| w > 0.0) {
        let style = border_style(b.style);
        let sides = ["top", "right", "bottom", "left"];
        let mut out = String::new();
        for ((side, width), color) in sides.iter().zip(b.widths).zip(b.colors) {
            let _ = write!(
                out,
                "border-{side}-width:{width}px;border-{side}-style:{style};border-{side}-color:{};",
                css_color(color)
            );
        }
        out.push_str(&radius);
        Some(out)
    } else {
        None
    };
    (css, border)
}

fn border_style(s: BorderStyle) -> &'static str {
    match s {
        BorderStyle::Solid => "solid",
        BorderStyle::Dashed => "dashed",
        BorderStyle::Dotted => "dotted",
    }
}

fn kind_name(kind: ViewKind) -> &'static str {
    match kind {
        ViewKind::View => "view",
        ViewKind::Text => "text",
        ViewKind::Image => "image",
        ViewKind::Scroll => "scroll",
        ViewKind::Input => "input",
        ViewKind::Stack => "stack",
        ViewKind::Screen => "screen",
        ViewKind::Modal => "modal",
        ViewKind::HeaderItem => "header",
        ViewKind::InputAccessory => "accessory",
        ViewKind::Canvas => "canvas",
        ViewKind::WebView => "web",
        ViewKind::Native => "native",
        ViewKind::Video => "video",
    }
}

fn image_uri(s: &Option<ImageSource>) -> Option<&str> {
    s.as_ref().map(|s| &*s.uri)
}

fn resize_mode(m: ResizeMode) -> &'static str {
    match m {
        ResizeMode::Cover => "cover",
        ResizeMode::Contain => "contain",
        ResizeMode::Stretch => "fill",
        ResizeMode::Center => "none",
    }
}

fn text_css(style: &nsh_core::ResolvedTextStyle, shadow: Option<&nsh_core::TextShadow>) -> String {
    let mut css = format!("font:{};color:{};", css_font(style), css_color(style.color));
    if style.letter_spacing != 0.0 {
        let _ = write!(css, "letter-spacing:{}px;", style.letter_spacing);
    }
    let d = style.decoration;
    if !d.is_none() {
        let lines = match (d.underline, d.line_through) {
            (true, true) => "underline line-through",
            (true, false) => "underline",
            _ => "line-through",
        };
        let _ = write!(
            css,
            "text-decoration:{lines} {};",
            kebab(&format!("{:?}", style.decoration_style))
        );
        if let Some(c) = style.decoration_color {
            let _ = write!(css, "text-decoration-color:{};", css_color(c));
        }
        if let Some(t) = style.decoration_thickness {
            let _ = write!(css, "text-decoration-thickness:{t}px;");
        }
        if let Some(o) = style.underline_offset {
            let _ = write!(css, "text-underline-offset:{o}px;");
        }
    }
    let v = style.font_variant.0;
    let mut numeric = Vec::new();
    if v & nsh_core::FontVariant::OLDSTYLE_NUMS != 0 {
        numeric.push("oldstyle-nums");
    }
    if v & nsh_core::FontVariant::LINING_NUMS != 0 {
        numeric.push("lining-nums");
    }
    if v & nsh_core::FontVariant::TABULAR_NUMS != 0 {
        numeric.push("tabular-nums");
    }
    if v & nsh_core::FontVariant::PROPORTIONAL_NUMS != 0 {
        numeric.push("proportional-nums");
    }
    if !numeric.is_empty() {
        let _ = write!(css, "font-variant-numeric:{};", numeric.join(" "));
    }
    if v & nsh_core::FontVariant::SMALL_CAPS != 0 {
        css.push_str("font-variant-caps:small-caps;");
    }
    if let Some(s) = shadow
        && !s.color.is_transparent()
    {
        let _ = write!(
            css,
            "text-shadow:{}px {}px {}px {};",
            s.offset_x,
            s.offset_y,
            s.radius,
            css_color(s.color)
        );
    }
    css
}

fn props_json(p: &ViewProps) -> String {
    let a = &p.accessibility;
    let mut s = String::from("{");
    let _ = write!(
        s,
        "\"pointerEvents\":{}",
        json_str(&enum_name(&p.pointer_events))
    );
    let h = p.hit_slop;
    let _ = write!(
        s,
        ",\"hitSlop\":[{},{},{},{}]",
        h.top, h.right, h.bottom, h.left
    );
    if let Some(o) = p.active_opacity {
        let _ = write!(s, ",\"activeOpacity\":{o}");
    }
    if let Some(d) = p.long_press_delay {
        let _ = write!(s, ",\"longPressDelay\":{d}");
    }
    if let Some(a) = p.pan_activation {
        let n = |v: Option<f32>| v.map_or("null".to_owned(), |v| v.to_string());
        let _ = write!(
            s,
            ",\"panActivation\":[{},{},{},{}]",
            n(a.active_offset_x),
            n(a.active_offset_y),
            n(a.fail_offset_x),
            n(a.fail_offset_y)
        );
    }
    if let Some(l) = &a.label {
        let _ = write!(s, ",\"label\":{}", json_str(l));
    }
    if let Some(h) = &a.hint {
        let _ = write!(s, ",\"hint\":{}", json_str(h));
    }
    let _ = write!(
        s,
        ",\"role\":{}",
        json_str(&kebab(&format!("{:?}", a.role)))
    );
    if a.state.disabled {
        s.push_str(",\"disabled\":true");
    }
    if a.state.selected {
        s.push_str(",\"selected\":true");
    }
    if a.hidden {
        s.push_str(",\"hidden\":true");
    }
    if let Some(t) = &p.test_id {
        let _ = write!(s, ",\"testID\":{}", json_str(t));
    }
    s.push('}');
    s
}

fn input_json(c: &InputConfig) -> String {
    let mut s = String::from("{");
    let _ = write!(
        s,
        "\"css\":{},\"placeholder\":{},\"secure\":{},\"keyboard\":{},\"returnKey\":{},\"multiline\":{},\"editable\":{},\"submit\":{},\"autoFocus\":{},\"autoCapitalize\":{},\"autoCorrect\":{}",
        json_str(&text_css(&c.style, None)),
        json_str(&c.placeholder),
        c.secure,
        json_str(&enum_name(&c.keyboard)),
        json_str(&enum_name(&c.return_key)),
        c.multiline,
        c.editable,
        json_str(&enum_name(&c.submit())),
        c.auto_focus,
        json_str(&enum_name(&c.auto_capitalize)),
        c.auto_correct,
    );
    let align = match c.align {
        nsh_core::TextAlign::Center => "center",
        nsh_core::TextAlign::Right => "right",
        _ => "left",
    };
    let _ = write!(s, ",\"align\":\"{align}\"");
    if let Some(c) = c.placeholder_color {
        let _ = write!(s, ",\"placeholderColor\":{}", json_str(&css_color(c)));
    }
    if let Some(c) = c.selection_color {
        let _ = write!(s, ",\"selectionColor\":{}", json_str(&css_color(c)));
    }
    if let Some(m) = c.max_length {
        let _ = write!(s, ",\"maxLength\":{m}");
    }
    let _ = write!(s, ",\"lineHeight\":{}", c.style.line_height);
    s.push('}');
    s
}

fn behavior_json(b: &ScrollBehavior) -> String {
    let offsets: Vec<String> = b.snap_offsets.iter().map(|o| o.to_string()).collect();
    format!(
        "{{\"paging\":{},\"snapInterval\":{},\"snapOffsets\":[{}],\"snapAlignment\":{},\"enabled\":{},\"bounces\":{},\"inset\":[{},{},{},{}],\"minZoom\":{},\"maxZoom\":{}}}",
        b.paging,
        b.snap_interval,
        offsets.join(","),
        json_str(&enum_name(&b.snap_alignment)),
        b.enabled,
        b.bounces,
        b.content_inset.top,
        b.content_inset.right,
        b.content_inset.bottom,
        b.content_inset.left,
        b.min_zoom,
        b.max_zoom
    )
}

fn transition_json(t: &Option<LayoutTransition>) -> String {
    match t {
        None => "null".to_owned(),
        Some(t) => {
            let easing = match t.curve {
                LayoutCurve::Linear => "linear".to_owned(),
                LayoutCurve::EaseIn => "ease-in".to_owned(),
                LayoutCurve::EaseOut => "ease-out".to_owned(),
                LayoutCurve::EaseInEaseOut => "ease-in-out".to_owned(),
                // An overshooting curve stands in for a spring.
                LayoutCurve::Spring { damping } => {
                    let over = (1.0 - damping.clamp(0.0, 1.0)) * 0.8 + 1.0;
                    format!("cubic-bezier(0.34,{over},0.64,1)")
                }
            };
            format!(
                "{{\"duration\":{},\"delay\":{},\"easing\":{},\"property\":{}}}",
                t.duration_ms,
                t.delay_ms,
                json_str(&easing),
                json_str(&kebab(&format!("{:?}", t.property)))
            )
        }
    }
}

/// Appends `item` (all but the animation items, which the caller keeps).
pub(crate) fn encode(out: &mut Out, item: &MountItem) {
    match item {
        MountItem::Create { id, kind } => {
            out.item("c").num(*id).str(kind_name(*kind)).end();
        }
        MountItem::TrackStates { id, states } => {
            out.item("ts").num(*id).num(*states).end();
        }
        MountItem::Insert {
            parent,
            child,
            before,
        } => {
            out.item("i").num(*parent).num(*child);
            match before {
                Some(b) => out.num(*b),
                None => out.raw("null"),
            };
            out.end();
        }
        MountItem::Remove { id } => out.item("r").num(*id).end(),
        MountItem::Delete { id } => out.item("d").num(*id).end(),
        MountItem::Frame { id, frame } => {
            out.item("f")
                .num(*id)
                .num(frame.x)
                .num(frame.y)
                .num(frame.width)
                .num(frame.height)
                .end();
        }
        MountItem::ViewStyle { id, style } => {
            let (css, border) = view_css(style);
            out.item("s").num(*id).str(&css).opt_str(border.as_deref());
            match style.tint_color {
                Some(c) => out.str(&css_color(c)),
                None => out.raw("null"),
            };
            out.end();
        }
        MountItem::Text {
            id,
            text,
            layout,
            content,
        } => {
            out.item("t")
                .num(*id)
                .num(content.x)
                .num(content.y)
                .num(content.width)
                .num(content.height);
            let Some(layout) = layout.downcast::<WebLayout>() else {
                out.raw("[]").raw("[]").end();
                return;
            };
            out.s.push_str(",[");
            for (i, run) in layout.runs.iter().enumerate() {
                if i > 0 {
                    out.s.push(',');
                }
                let _ = write!(out.s, "[{},{},", run.x, run.y);
                push_str(&mut out.s, &run.text);
                let _ = write!(out.s, ",{}]", run.style);
            }
            out.s.push_str("],[");
            for (i, s) in layout.styles.iter().enumerate() {
                if i > 0 {
                    out.s.push(',');
                }
                // A line box as tall as the font: the baseline sits at the
                // run's top plus its ascent, where layout put it.
                let css = format!(
                    "{}line-height:{}px;",
                    text_css(&s.style, text.shadow.as_ref()),
                    s.ascent + s.descent
                );
                push_str(&mut out.s, &css);
            }
            out.s.push(']');
            out.bool(text.selectable)
                .opt_str(text.selection_color.map(css_color).as_deref())
                .end();
        }
        MountItem::Events { id, events } => {
            out.item("e").num(*id);
            out.s.push_str(",[");
            let mut first = true;
            for kind in EventKind::ALL {
                if events.contains(*kind) {
                    if !first {
                        out.s.push(',');
                    }
                    first = false;
                    push_str(&mut out.s, kind.prop_name());
                }
            }
            out.s.push(']');
            out.end();
        }
        MountItem::Image {
            id,
            source,
            resize_mode: mode,
            options,
        } => {
            out.item("img")
                .num(*id)
                .opt_str(image_uri(source))
                .str(resize_mode(*mode))
                .opt_str(image_uri(&options.placeholder))
                .num(options.blur_radius)
                .num(options.fade_ms.unwrap_or(0.0))
                .end();
        }
        MountItem::ScrollConfig {
            id,
            horizontal,
            shows_indicator,
            mirrored,
            ..
        } => {
            out.item("sc")
                .num(*id)
                .bool(*horizontal)
                .bool(*shows_indicator)
                .bool(*mirrored)
                .end();
        }
        MountItem::ScrollContent { id, size } => {
            out.item("scs")
                .num(*id)
                .num(size.width)
                .num(size.height)
                .end();
        }
        MountItem::ScrollBy { id, dx, dy } => {
            out.item("sb").num(*id).num(*dx).num(*dy).end();
        }
        MountItem::LayoutDirection { rtl } => out.item("dir").bool(*rtl).end(),
        MountItem::Appearance { appearance } => out.item("appearance").num(*appearance as u8).end(),
        MountItem::StickyCells { id, cells } => {
            out.item("sticky").num(*id);
            out.s.push_str(",[");
            for (i, c) in cells.iter().enumerate() {
                if i > 0 {
                    out.s.push(',');
                }
                let end = if c.end.is_finite() { c.end } else { 1e9 };
                let _ = write!(out.s, "[{},{},{}]", c.cell, c.start, end);
            }
            out.s.push(']');
            out.end();
        }
        MountItem::ScrollBehavior { id, behavior } => {
            out.item("sbh").num(*id).raw(&behavior_json(behavior)).end();
        }
        MountItem::Refreshing { id, refreshing } => {
            out.item("ref").num(*id).bool(*refreshing).end();
        }
        MountItem::Canvas { id, drawing } => {
            out.item("cv")
                .num(*id)
                .raw(&crate::canvas::encode(drawing))
                .end();
        }
        MountItem::Gestures { id, values } => {
            out.item("g")
                .num(*id)
                .bool(values.pan_x.is_some() || values.pan_y.is_some())
                .bool(values.pinch.is_some())
                .bool(values.rotation.is_some())
                .end();
        }
        MountItem::ViewProps { id, props } => {
            out.item("vp").num(*id).raw(&props_json(props)).end();
        }
        MountItem::AccessibilityLabel { id, label } => {
            out.item("al").num(*id).opt_str(label.as_deref()).end();
        }
        MountItem::LayoutAnimation(a) => {
            out.item("la")
                .raw(&transition_json(&a.create))
                .raw(&transition_json(&a.update))
                .raw(&transition_json(&a.delete))
                .end();
        }
        MountItem::Input { id, config } => {
            out.item("in").num(*id).raw(&input_json(config)).end();
        }
        MountItem::InputValue { id, text, count } => {
            out.item("iv").num(*id).str(text).num(*count).end();
        }
        MountItem::InputContent { id, content } => {
            out.item("ic")
                .num(*id)
                .num(content.x)
                .num(content.y)
                .num(content.width)
                .num(content.height)
                .end();
        }
        MountItem::ScreenConfig { id, config } => {
            out.item("scr")
                .num(*id)
                .opt_str(config.title.as_deref())
                .bool(config.header_shown)
                .num(config.header_height)
                .opt_str(config.background_color.map(css_color).as_deref())
                .opt_str(config.title_color.map(css_color).as_deref())
                .opt_str(config.tint_color.map(css_color).as_deref())
                .bool(config.back_visible)
                .bool(config.prevent_back);
            for slot in [config.left, config.right, config.title_view] {
                match slot {
                    Some(n) => out.num(n),
                    None => out.raw("null"),
                };
            }
            out.end();
        }
        MountItem::ModalConfig { id, config } => {
            out.item("mc")
                .num(*id)
                .str(&enum_name(&config.animation))
                .bool(config.transparent)
                .bool(config.dismissible)
                .end();
        }
        MountItem::NativeProps { id, props } => match props {
            NativeProps::WebView(w) => {
                let (url, html) = match &w.source {
                    WebSource::Uri(u) => (Some(&**u), None),
                    WebSource::Html { html, .. } => (None, Some(&**html)),
                    _ => (None, None),
                };
                out.item("web").num(*id).opt_str(url).opt_str(html).end();
            }
            NativeProps::Video(v) => {
                out.item("video")
                    .num(*id)
                    .opt_str(v.source.as_deref())
                    .bool(v.paused)
                    .bool(v.muted)
                    .num(v.volume)
                    .num(v.rate)
                    .bool(v.repeat)
                    .str(resize_mode(v.resize_mode))
                    .bool(v.controls)
                    .end();
            }
        },
        MountItem::Command { id, command } => encode_command(out, *id, command),
        MountItem::Component { id, name } => out.item("comp").num(*id).str(name).end(),
        // Browsers hit-test what overflows a box themselves.
        MountItem::ComponentProps { .. }
        | MountItem::Keymap(_)
        | MountItem::Overflow { .. }
        | MountItem::AnimationBindings { .. }
        | MountItem::Animation(_) => {}
    }
}

fn encode_command(out: &mut Out, id: u32, command: &Command) {
    match command {
        Command::Focus => out.item("cmd").num(id).str("focus").end(),
        Command::Blur => out.item("cmd").num(id).str("blur").end(),
        Command::ScrollTo { x, y, animated } => {
            out.item("cmd")
                .num(id)
                .str("scrollTo")
                .num(*x)
                .num(*y)
                .bool(*animated)
                .end();
        }
        Command::ScrollToEnd { animated } => {
            out.item("cmd")
                .num(id)
                .str("scrollToEnd")
                .bool(*animated)
                .end();
        }
        Command::SetSelection { start, end } => {
            out.item("cmd")
                .num(id)
                .str("setSelection")
                .num(*start)
                .num(*end)
                .end();
        }
        Command::Dialog(d) => {
            out.item("cmd").num(id).str("dialog").num(d.id);
            out.opt_str(d.title.as_deref())
                .opt_str(d.message.as_deref());
            out.s.push_str(",[");
            for (i, b) in d.buttons.iter().enumerate() {
                if i > 0 {
                    out.s.push(',');
                }
                push_str(&mut out.s, &b.text);
            }
            out.s.push(']');
            out.end();
        }
        Command::SetJsResponder { block } => {
            out.item("cmd")
                .num(id)
                .str("setResponder")
                .bool(*block)
                .end();
        }
        Command::ClearJsResponder => out.item("cmd").num(id).str("clearResponder").end(),
        Command::Reload => out.item("cmd").num(id).str("reload").end(),
        Command::GoBack => out.item("cmd").num(id).str("goBack").end(),
        Command::GoForward => out.item("cmd").num(id).str("goForward").end(),
        Command::PostMessage(m) => out.item("cmd").num(id).str("postMessage").str(m).end(),
        Command::InjectJavaScript(s) => {
            out.item("cmd").num(id).str("injectJavaScript").str(s).end();
        }
        Command::Seek(t) => out.item("cmd").num(id).str("seek").num(*t).end(),
        Command::SetClipboard(text) => out.item("cmd").num(id).str("setClipboard").str(text).end(),
        Command::ReadClipboard {
            id: request,
            has_only,
        } => {
            out.item("cmd")
                .num(id)
                .str("readClipboard")
                .num(*request)
                .bool(*has_only)
                .end();
        }
        Command::ImageInfo { id: request, uri } => {
            out.item("cmd")
                .num(id)
                .str("imageInfo")
                .num(*request)
                .str(uri)
                .end();
        }
        other => log::debug!("web: command {other:?} is not supported"),
    }
}
