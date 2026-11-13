//! The CSS of NSH's styles: a view's visual style (backgrounds, borders,
//! radii, shadows, filters, transforms, transitions) and a text run's
//! (fonts, colors, decorations). The web backend draws views with it and
//! static pages (`nsh render`) are written with it, so both draw a style
//! the same way.

use std::fmt::Write as _;

use nsh_core::{
    BorderStyle, Color, Filter, FontVariant, Gradient, GradientKind, GradientSize, Keyframes,
    Points, ResizeMode, ResolvedTextStyle, Transform, TransformKind, TransitionProps, ViewStyle,
    geometry::{DefiniteLength, REM_SIZE, Radians, Size, relative},
};

/// A CSS color.
pub fn css_color(c: Color) -> String {
    let v = c.0;
    let (r, g, b, a) = (v >> 24, (v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff);
    if a == 255 {
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    } else {
        format!("rgba({r},{g},{b},{:.3})", a as f32 / 255.0)
    }
}

/// The CSS `object-fit` of an image's or a video's resize mode.
pub fn object_fit(mode: ResizeMode) -> &'static str {
    match mode {
        ResizeMode::Cover => "cover",
        ResizeMode::Contain => "contain",
        ResizeMode::Stretch => "fill",
        // Centered at its natural size.
        ResizeMode::Center => "none",
        // The same, scaled down to fit when larger.
        ResizeMode::ScaleDown => "scale-down",
    }
}

/// `FooBar` (a `Debug` name) as `foo-bar`.
pub fn kebab(name: &str) -> String {
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

/// The CSS transform functions of a transform list (`value(i)` overrides
/// entry `i`).
pub fn css_transform(t: &Transform, value: impl Fn(usize) -> Option<f32>) -> String {
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

fn origin(o: DefiniteLength) -> String {
    match o {
        DefiniteLength::Absolute(v) => format!("{}px", v.to_points(REM_SIZE).0),
        DefiniteLength::Fraction(f) => format!("{}%", f.0 * 100.0),
    }
}

/// The CSS of `g`. Without lengths, the stops the core resolves, so the
/// browser places them as every platform does (a negative stop at 0);
/// with lengths, the stops as written (lengths in pixels), which only the
/// box resolves.
pub(crate) fn gradient(g: &Gradient) -> String {
    let stops: Vec<String> = if g.has_lengths() {
        g.stops
            .iter()
            .map(|s| match s.position {
                Some(DefiniteLength::Fraction(p)) => {
                    format!("{} {}%", css_color(s.color), p.0 * 100.0)
                }
                Some(DefiniteLength::Absolute(a)) => {
                    format!("{} {}px", css_color(s.color), a.to_points(REM_SIZE).0)
                }
                None => css_color(s.color),
            })
            .collect()
    } else {
        g.resolved_stops(Size::default())
            .iter()
            .map(|(c, p)| format!("{} {}%", css_color(*c), p.0 * 100.0))
            .collect()
    };
    let stops = stops.join(", ");
    let repeating = if g.repeating { "repeating-" } else { "" };
    match g.kind {
        GradientKind::Linear { angle } => {
            format!("{repeating}linear-gradient({}deg, {stops})", degrees(angle))
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
                x.0 * 100.0,
                y.0 * 100.0
            )
        }
        GradientKind::Conic { from, x, y } => format!(
            "{repeating}conic-gradient(from {}deg at {}% {}%, {stops})",
            degrees(from),
            x.0 * 100.0,
            y.0 * 100.0
        ),
    }
}

/// An angle in CSS degrees, rounded to 1e-4 so that whole degrees are
/// written as the style wrote them (radians to degrees in `f32` can be an
/// ulp off).
pub(crate) fn degrees(a: Radians) -> f32 {
    (a.to_degrees() * 1e4).round() / 1e4
}

/// The CSS functions of a filter list (`filter`, `backdrop-filter`).
pub(crate) fn filters(list: &[Filter]) -> String {
    let mut out = String::new();
    for f in list {
        let _ = match *f {
            Filter::Blur(v) => write!(out, "blur({}px) ", v.0),
            Filter::Brightness(v) => write!(out, "brightness({v}) "),
            Filter::Contrast(v) => write!(out, "contrast({v}) "),
            Filter::Grayscale(v) => write!(out, "grayscale({v}) "),
            Filter::HueRotate(v) => write!(out, "hue-rotate({}deg) ", degrees(v)),
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
                "drop-shadow({}px {}px {}px {}) ",
                offset_x.0,
                offset_y.0,
                blur.0,
                css_color(color)
            ),
        };
    }
    out
}

/// A cubic Bézier easing as CSS.
pub(crate) fn bezier(e: [f32; 4]) -> String {
    format!("cubic-bezier({},{},{},{})", e[0], e[1], e[2], e[3])
}

/// The CSS of a view's visual style, and of its border overlay (drawn
/// over the children, as UIKit draws layer borders; also keeps borders
/// out of the content box, which layout already placed).
pub fn view_css(s: &ViewStyle) -> (String, Option<String>) {
    let mut css = String::new();
    if !s.background_color.is_transparent() {
        let _ = write!(css, "background-color:{};", css_color(s.background_color));
    }
    let radii: [f32; 4] = s
        .corner_radius
        .to_array()
        .map(|r| r.unwrap_or(s.border_radius).0);
    let radius = if radii.iter().any(|&r| r > 0.0) {
        format!(
            "border-radius:{}px {}px {}px {}px;",
            radii[0], radii[1], radii[2], radii[3]
        )
    } else {
        String::new()
    };
    css.push_str(&radius);
    if s.animation.is_some() {
        // Keyframes multiply the view's opacity (`--nsh-ko`, `nsh.js`).
        let _ = write!(css, "opacity:calc({}*var(--nsh-ko));", s.opacity);
    } else if s.opacity < 1.0 {
        let _ = write!(css, "opacity:{};", s.opacity);
    }
    // `clip`, not `hidden`: a hidden box is still a scroll container, which
    // the browser scrolls to bring a focused field into view, moving
    // content away from where the core laid it out.
    if s.overflow_hidden {
        css.push_str("overflow:clip;");
    }
    let mut shadows = Vec::new();
    if let Some(effects) = &s.effects {
        for b in &effects.box_shadows {
            shadows.push(format!(
                "{}{}px {}px {}px {}px {}",
                if b.inset { "inset " } else { "" },
                b.offset_x.0,
                b.offset_y.0,
                b.blur.0,
                b.spread.0,
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
        if !effects.mask_images.is_empty() {
            let images: Vec<String> = effects.mask_images.iter().map(gradient).collect();
            let m = images.join(",");
            // `-webkit-mask-image` for WebKit, which still needs the prefix.
            let _ = write!(css, "mask-image:{m};-webkit-mask-image:{m};");
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
    if o.x != relative(0.5) || o.y != relative(0.5) {
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
            s.outline.width.0,
            border_style(s.outline.style),
            css_color(s.outline.color),
            s.outline.offset.0
        );
    }
    if s.hidden {
        css.push_str("visibility:hidden;");
    }
    let cursor = kebab(&format!("{:?}", s.cursor));
    if cursor != "auto" {
        let _ = write!(css, "cursor:{cursor};");
    }
    let mut border_transition = String::new();
    if let Some(t) = &s.transition {
        let mut props = Vec::new();
        let p = t.properties;
        if p.has(TransitionProps::COLORS) {
            // `color`: a text's, on its element (`op_t`).
            props.extend(["background-color", "outline-color", "color"]);
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
            props.extend(["border-radius", "outline-width"]);
        }
        let timing = format!(
            "transition-duration:{}ms;transition-delay:{}ms;transition-timing-function:{};",
            t.duration,
            t.delay,
            bezier(t.easing)
        );
        let _ = write!(css, "transition-property:{};{timing}", props.join(","));
        // The overlay draws the border.
        let mut border = Vec::new();
        if p.has(TransitionProps::COLORS) {
            border.push("border-color");
        }
        if p.has(TransitionProps::SHAPE) {
            border.extend(["border-width", "border-radius"]);
        }
        if !border.is_empty() {
            border_transition = format!("transition-property:{};{timing}", border.join(","));
        }
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
        // Added to the view's own transform, as the native backends
        // apply keyframes after the style's transform.
        let _ = write!(
            css,
            "animation:{name} {}ms {} {}ms {iterations};animation-composition:add;",
            a.duration,
            bezier(a.easing),
            a.delay
        );
    }
    let b = &s.borders;
    let border = if b.widths.any(|&w| w > Points::ZERO) {
        let style = border_style(b.style);
        let sides = ["top", "right", "bottom", "left"];
        let mut out = String::new();
        for ((side, width), color) in sides.iter().zip(b.widths.to_array()).zip(b.colors) {
            let width = width.0;
            let _ = write!(
                out,
                "border-{side}-width:{width}px;border-{side}-style:{style};border-{side}-color:{};",
                css_color(color)
            );
        }
        out.push_str(&radius);
        out.push_str(&border_transition);
        Some(out)
    } else {
        None
    };
    (css, border)
}

/// A border style as CSS.
pub(crate) fn border_style(s: BorderStyle) -> &'static str {
    match s {
        BorderStyle::Solid => "solid",
        BorderStyle::Dashed => "dashed",
        BorderStyle::Dotted => "dotted",
    }
}

/// The CSS of a run of `style`; `inherit_color`: without its color (and a
/// decoration color, which is the same), which the run takes from its text's
/// element, drawn in one color.
pub fn text_css(
    style: &nsh_core::ResolvedTextStyle,
    shadow: Option<&nsh_core::TextShadow>,
    inherit_color: bool,
) -> String {
    let mut css = format!("font:{};", css_font(style));
    // Hidden runs keep their place; the browser leaves them out of the
    // selection's copy and of the accessibility tree.
    if style.hidden {
        css.push_str("visibility:hidden;");
    }
    if !inherit_color {
        let _ = write!(css, "color:{};", css_color(style.color));
    }
    if style.letter_spacing.0 != 0.0 {
        let _ = write!(css, "letter-spacing:{}px;", style.letter_spacing.0);
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
        if let Some(c) = style.decoration_color.filter(|_| !inherit_color) {
            let _ = write!(css, "text-decoration-color:{};", css_color(c));
        }
        if let Some(t) = style.decoration_thickness.map(|t| t.0) {
            let _ = write!(css, "text-decoration-thickness:{t}px;");
        }
        if let Some(o) = style.underline_offset.map(|o| o.0) {
            let _ = write!(css, "text-underline-offset:{o}px;");
        }
    }
    if let Some(numeric) = font_variant_numeric(style.font_variant) {
        let _ = write!(css, "font-variant-numeric:{numeric};");
    }
    if style.font_variant.0 & nsh_core::FontVariant::SMALL_CAPS != 0 {
        css.push_str("font-variant-caps:small-caps;");
    }
    if let Some(s) = shadow
        && !s.color.is_transparent()
    {
        let _ = write!(
            css,
            "text-shadow:{}px {}px {}px {};",
            s.offset_x.0,
            s.offset_y.0,
            s.radius.0,
            css_color(s.color)
        );
    }
    css
}

/// The system font stack (what `fontFamily` unset means).
const SYSTEM_FONT: &str = "-apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, \
                                      \"Helvetica Neue\", Arial, \"PingFang SC\", \
                                      \"Noto Sans SC\", \"Microsoft YaHei\", sans-serif";

/// The CSS `font` shorthand of a style.
pub fn css_font(style: &ResolvedTextStyle) -> String {
    let family = match style.font_family.map(|f| family_list(&f.name())) {
        Some(list) if !list.is_empty() => format!("{list}, {SYSTEM_FONT}"),
        _ => SYSTEM_FONT.to_owned(),
    };
    format!(
        "{}{} {}px {family}",
        if style.italic { "italic " } else { "" },
        style.font_weight,
        style.font_size.0
    )
}

/// The CSS `font-variant-numeric` of a style's variant (`None`: normal).
/// Canvas text has no numeric variants, so text with one is measured in
/// the DOM (`ffi::measure_numeric_text`).
pub fn font_variant_numeric(variant: FontVariant) -> Option<String> {
    let names = [
        (FontVariant::OLDSTYLE_NUMS, "oldstyle-nums"),
        (FontVariant::LINING_NUMS, "lining-nums"),
        (FontVariant::TABULAR_NUMS, "tabular-nums"),
        (FontVariant::PROPORTIONAL_NUMS, "proportional-nums"),
    ];
    let mut css = String::new();
    for (bit, name) in names {
        if variant.0 & bit != 0 {
            if !css.is_empty() {
                css.push(' ');
            }
            css.push_str(name);
        }
    }
    (!css.is_empty()).then_some(css)
}

/// A `fontFamily` value as a CSS family list: one family or a
/// comma-separated stack. Generic families and the system font keywords
/// stay bare (quoted, `"monospace"` names a font called monospace, which
/// no system has, and the text falls back to the next family); other
/// names are quoted, and names the app quoted keep their quotes.
pub fn family_list(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for family in split_families(value) {
        if !out.is_empty() {
            out.push_str(", ");
        }
        if well_quoted(family) || is_keyword(family) {
            out.push_str(family);
        } else {
            // A name, or a quoted one whose quotes do not close it: quoted
            // here, escaped, without control characters (a CSS string
            // cannot hold a line break).
            let name = family.trim_matches(['"', '\'']);
            out.push('"');
            for c in name.chars().filter(|c| !c.is_control()) {
                if c == '"' || c == '\\' {
                    out.push('\\');
                }
                out.push(c);
            }
            out.push('"');
        }
    }
    out
}

/// A name the app quoted that its quotes close: the same quote at both
/// ends, and none of it, a backslash or a control character inside, so it
/// is one CSS string as written.
fn well_quoted(family: &str) -> bool {
    let Some(quote) = family.chars().next().filter(|c| *c == '"' || *c == '\'') else {
        return false;
    };
    family.len() >= 2
        && family.ends_with(quote)
        && !family[1..family.len() - 1]
            .chars()
            .any(|c| c == quote || c == '\\' || c.is_control())
}

/// The families of a list, trimmed, without empty ones; commas inside
/// quotes do not separate.
fn split_families(value: &str) -> impl Iterator<Item = &str> {
    let mut rest = value;
    std::iter::from_fn(move || {
        loop {
            if rest.is_empty() {
                return None;
            }
            let mut quote = None;
            let mut end = rest.len();
            for (i, c) in rest.char_indices() {
                match quote {
                    Some(q) if c == q => quote = None,
                    Some(_) => {}
                    None if c == '"' || c == '\'' => quote = Some(c),
                    None if c == ',' => {
                        end = i;
                        break;
                    }
                    None => {}
                }
            }
            let family = rest[..end].trim();
            rest = rest.get(end + 1..).unwrap_or("");
            if !family.is_empty() {
                return Some(family);
            }
        }
    })
}

/// A CSS generic family (CSS Fonts 4) or a system font keyword: written
/// without quotes. CSS matches them ASCII case-insensitively.
fn is_keyword(family: &str) -> bool {
    const KEYWORDS: [&str; 15] = [
        "serif",
        "sans-serif",
        "monospace",
        "cursive",
        "fantasy",
        "system-ui",
        "ui-serif",
        "ui-sans-serif",
        "ui-monospace",
        "ui-rounded",
        "math",
        "emoji",
        "fangsong",
        "-apple-system",
        // Chrome's name for the system font on macOS: a family only unquoted.
        "BlinkMacSystemFont",
    ];
    KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(family))
}
