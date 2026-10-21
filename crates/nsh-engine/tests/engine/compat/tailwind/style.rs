//! What a resolved class sets, as apps write it: NSH's style keys
//! (`paddingTop`, `backgroundColor`, `transform`) and the prop names of
//! what classes set besides styles (`numberOfLines`), with CSS's values:
//! points as plain numbers (`16`), `50%`, `#fb2c36`, `blur(8)`,
//! `150ms`. Never Rust's `{:?}` of the types behind them, so the record
//! changes when what apps get changes, not when the core's types do.
//!
//! Each field of `Style` and `ClassProps` that differs from its default is
//! written; `tailwind_record_is_faithful` checks that no field is missed.

use nsh_core::{
    BlendMode, BoxShadow, Color, Cursor, Filter, Gradient, GradientKind, GradientSize, Style,
    TextShadow, TransformKind,
    geometry::{AbsoluteLength, DefiniteLength, Percentage, Points, Radians},
    tailwind::ClassProps,
};
use taffy::{
    CompactLength, GridPlacement, GridTemplateComponent, LengthPercentage, LengthPercentageAuto,
    TrackSizingFunction,
};

/// What `style` and `props` set that `default` does not, as
/// `key: value`, in a fixed order.
pub(super) fn decls(style: &Style, props: &ClassProps, default: &Style) -> Vec<String> {
    let mut out = Decls::default();
    layout(&mut out, &style.layout, &default.layout);
    view(&mut out, style, default);
    text(&mut out, &style.text, &default.text);
    class_props(&mut out, props);
    // A border's width is both layout's and the view's: once.
    let mut seen = std::collections::HashSet::new();
    out.0.retain(|d| seen.insert(d.clone()));
    out.0
}

#[derive(Default)]
struct Decls(Vec<String>);

impl Decls {
    fn push(&mut self, key: &str, value: impl Into<String>) {
        self.0.push(format!("{key}: {}", value.into()));
    }

    /// `key` when `value` differs from `default`.
    fn diff<T: PartialEq>(
        &mut self,
        key: &str,
        value: &T,
        default: &T,
        show: impl Fn(&T) -> String,
    ) {
        if value != default {
            self.push(key, show(value));
        }
    }

    /// Four sides (top, right, bottom, left): `keys.all` when they are
    /// equal, else the horizontal and vertical pairs, else each side.
    fn sides(&mut self, keys: &Sides, values: [String; 4], default: [String; 4]) {
        let set = |i: usize| values[i] != default[i];
        if !(0..4).any(set) {
            return;
        }
        if let Some(all) = keys.all
            && values.iter().all(|v| *v == values[0])
        {
            return self.push(all, values[0].clone());
        }
        let mut done = [false; 4];
        if let Some(v) = keys.vertical
            && values[0] == values[2]
            && set(0)
            && set(2)
        {
            self.push(v, values[0].clone());
            done[0] = true;
            done[2] = true;
        }
        if let Some(h) = keys.horizontal
            && values[1] == values[3]
            && set(1)
            && set(3)
        {
            self.push(h, values[1].clone());
            done[1] = true;
            done[3] = true;
        }
        for i in 0..4 {
            if set(i) && !done[i] {
                self.push(keys.each[i], values[i].clone());
            }
        }
    }
}

/// The keys of a property with four sides.
struct Sides {
    all: Option<&'static str>,
    vertical: Option<&'static str>,
    horizontal: Option<&'static str>,
    each: [&'static str; 4],
}

const PADDING: Sides = Sides {
    all: Some("padding"),
    vertical: Some("paddingVertical"),
    horizontal: Some("paddingHorizontal"),
    each: ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"],
};
const MARGIN: Sides = Sides {
    all: Some("margin"),
    vertical: Some("marginVertical"),
    horizontal: Some("marginHorizontal"),
    each: ["marginTop", "marginRight", "marginBottom", "marginLeft"],
};
const INSET: Sides = Sides {
    all: None,
    vertical: None,
    horizontal: None,
    each: ["top", "right", "bottom", "left"],
};
const BORDER_WIDTH: Sides = Sides {
    all: Some("borderWidth"),
    vertical: None,
    horizontal: None,
    each: [
        "borderTopWidth",
        "borderRightWidth",
        "borderBottomWidth",
        "borderLeftWidth",
    ],
};
const BORDER_COLOR: Sides = Sides {
    all: Some("borderColor"),
    vertical: None,
    horizontal: None,
    each: [
        "borderTopColor",
        "borderRightColor",
        "borderBottomColor",
        "borderLeftColor",
    ],
};
const CORNERS: Sides = Sides {
    all: None,
    vertical: None,
    horizontal: None,
    each: [
        "borderTopLeftRadius",
        "borderTopRightRadius",
        "borderBottomRightRadius",
        "borderBottomLeftRadius",
    ],
};
const LOGICAL_CORNERS: Sides = Sides {
    all: None,
    vertical: None,
    horizontal: None,
    each: [
        "borderTopStartRadius",
        "borderTopEndRadius",
        "borderBottomEndRadius",
        "borderBottomStartRadius",
    ],
};

/// A number as CSS writes it (`16`, `0.5`, `-4`): points as numbers,
/// angles in degrees.
pub(super) fn num(v: impl Num) -> String {
    format!("{}", v.number())
}

/// What [`num`] writes.
pub(super) trait Num {
    fn number(&self) -> f32;
}

impl Num for f32 {
    fn number(&self) -> f32 {
        *self
    }
}

impl Num for &f32 {
    fn number(&self) -> f32 {
        **self
    }
}

impl Num for Points {
    fn number(&self) -> f32 {
        self.0
    }
}

impl Num for &Points {
    fn number(&self) -> f32 {
        self.0
    }
}

impl Num for Radians {
    /// Degrees, without the float noise of going through radians.
    fn number(&self) -> f32 {
        (self.to_degrees() * 1e4).round() / 1e4
    }
}

/// A fraction as a percentage (`50%`).
fn percent(p: Percentage) -> String {
    format!("{}%", num(p.0 * 100.0))
}

/// A length a transform origin or a gradient stop is at.
fn definite(l: &DefiniteLength) -> String {
    match *l {
        DefiniteLength::Absolute(AbsoluteLength::Points(p)) => num(p),
        DefiniteLength::Absolute(AbsoluteLength::Rems(r)) => format!("{}rem", num(r.0)),
        DefiniteLength::Fraction(f) => percent(f),
    }
}

/// `#rrggbb`, or `#rrggbbaa` when it is not opaque.
pub(super) fn color(c: &Color) -> String {
    if c.0 & 0xff == 0xff {
        format!("#{:06x}", c.0 >> 8)
    } else {
        format!("#{:08x}", c.0)
    }
}

/// A keyword from an enum's variant: `RowReverse` → `row-reverse`.
fn keyword(value: &impl std::fmt::Debug) -> String {
    let name = format!("{value:?}");
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn length(c: CompactLength) -> String {
    let v = c.value();
    match c.tag() {
        CompactLength::LENGTH_TAG => num(v),
        CompactLength::PERCENT_TAG => format!("{}%", num(v * 100.0)),
        CompactLength::AUTO_TAG => "auto".into(),
        CompactLength::FR_TAG => format!("{}fr", num(v)),
        CompactLength::CONTENT_TAG => "content".into(),
        CompactLength::MIN_CONTENT_TAG => "min-content".into(),
        CompactLength::MAX_CONTENT_TAG => "max-content".into(),
        CompactLength::FIT_CONTENT_PX_TAG => format!("fit-content({})", num(v)),
        CompactLength::FIT_CONTENT_PERCENT_TAG => format!("fit-content({}%)", num(v * 100.0)),
        CompactLength::FIT_CONTENT_KEYWORD_TAG => "fit-content".into(),
        CompactLength::STRETCH_TAG => "stretch".into(),
        tag => panic!("a length with tag {tag:#x}"),
    }
}

fn lp(v: &LengthPercentage) -> String {
    length(v.into_raw())
}

fn lpa(v: &LengthPercentageAuto) -> String {
    length(v.into_raw())
}

fn track(t: &TrackSizingFunction) -> String {
    let (min, max) = (length(t.min.into_raw()), length(t.max.into_raw()));
    if min == max {
        min
    } else {
        format!("minmax({min}, {max})")
    }
}

fn tracks(list: &[TrackSizingFunction]) -> String {
    list.iter().map(track).collect::<Vec<_>>().join(" ")
}

fn template(list: &[GridTemplateComponent<String>]) -> String {
    let parts: Vec<String> = list
        .iter()
        .map(|c| match c {
            GridTemplateComponent::Single(t) => track(t),
            GridTemplateComponent::Repeat(r) => {
                let count = match r.count {
                    taffy::RepetitionCount::AutoFill => "auto-fill".to_owned(),
                    taffy::RepetitionCount::AutoFit => "auto-fit".to_owned(),
                    taffy::RepetitionCount::Count(n) => n.to_string(),
                };
                format!("repeat({count}, {})", tracks(&r.tracks))
            }
        })
        .collect();
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(" ")
    }
}

fn placement(p: &GridPlacement<String>) -> String {
    match p {
        GridPlacement::Auto => "auto".into(),
        GridPlacement::Line(l) => l.as_i16().to_string(),
        GridPlacement::Span(n) => format!("span {n}"),
        GridPlacement::NamedLine(name, n) => format!("{name} {n}"),
        GridPlacement::NamedSpan(name, n) => format!("span {name} {n}"),
    }
}

fn layout(out: &mut Decls, s: &taffy::Style, d: &taffy::Style) {
    out.diff("display", &s.display, &d.display, keyword);
    out.diff("boxSizing", &s.box_sizing, &d.box_sizing, keyword);
    out.diff("overflowX", &s.overflow.x, &d.overflow.x, keyword);
    out.diff("overflowY", &s.overflow.y, &d.overflow.y, keyword);
    out.diff(
        "scrollbarWidth",
        &s.scrollbar_width,
        &d.scrollbar_width,
        |v| num(*v),
    );
    out.diff("position", &s.position, &d.position, keyword);
    let rect = |r: &taffy::Rect<LengthPercentageAuto>| {
        [lpa(&r.top), lpa(&r.right), lpa(&r.bottom), lpa(&r.left)]
    };
    let rect_lp =
        |r: &taffy::Rect<LengthPercentage>| [lp(&r.top), lp(&r.right), lp(&r.bottom), lp(&r.left)];
    out.sides(&INSET, rect(&s.inset), rect(&d.inset));
    out.diff("width", &s.size.width, &d.size.width, |v| {
        length(v.into_raw())
    });
    out.diff("height", &s.size.height, &d.size.height, |v| {
        length(v.into_raw())
    });
    out.diff("minWidth", &s.min_size.width, &d.min_size.width, lpa);
    out.diff("minHeight", &s.min_size.height, &d.min_size.height, lpa);
    out.diff("maxWidth", &s.max_size.width, &d.max_size.width, lpa);
    out.diff("maxHeight", &s.max_size.height, &d.max_size.height, lpa);
    out.diff("aspectRatio", &s.aspect_ratio, &d.aspect_ratio, |v| {
        v.map_or("auto".into(), num)
    });
    out.sides(&MARGIN, rect(&s.margin), rect(&d.margin));
    out.sides(&PADDING, rect_lp(&s.padding), rect_lp(&d.padding));
    out.sides(&BORDER_WIDTH, rect_lp(&s.border), rect_lp(&d.border));
    let align = |keyword_: String, safety: &taffy::AlignmentSafety| match safety {
        taffy::AlignmentSafety::Default => keyword_,
        safety => format!("{} {keyword_}", keyword(safety)),
    };
    out.diff("alignItems", &s.align_items, &d.align_items, |v| {
        align(keyword(&v.keyword), &v.safety)
    });
    out.diff("alignSelf", &s.align_self, &d.align_self, |v| {
        v.map_or("auto".into(), |v| align(keyword(&v.keyword), &v.safety))
    });
    out.diff("justifyItems", &s.justify_items, &d.justify_items, |v| {
        align(keyword(&v.keyword), &v.safety)
    });
    out.diff("justifySelf", &s.justify_self, &d.justify_self, |v| {
        v.map_or("auto".into(), |v| align(keyword(&v.keyword), &v.safety))
    });
    out.diff("alignContent", &s.align_content, &d.align_content, |v| {
        align(keyword(&v.keyword), &v.safety)
    });
    out.diff(
        "justifyContent",
        &s.justify_content,
        &d.justify_content,
        |v| align(keyword(&v.keyword), &v.safety),
    );
    if s.gap.width == s.gap.height && s.gap != d.gap {
        out.push("gap", lp(&s.gap.width));
    } else {
        out.diff("rowGap", &s.gap.height, &d.gap.height, lp);
        out.diff("columnGap", &s.gap.width, &d.gap.width, lp);
    }
    out.diff("layoutTextAlign", &s.text_align, &d.text_align, keyword);
    out.diff(
        "flexDirection",
        &s.flex_direction,
        &d.flex_direction,
        keyword,
    );
    out.diff("flexWrap", &s.flex_wrap, &d.flex_wrap, keyword);
    out.diff("flexBasis", &s.flex_basis, &d.flex_basis, |v| {
        length(v.into_raw())
    });
    out.diff("flexGrow", &s.flex_grow, &d.flex_grow, |v| num(*v));
    out.diff("flexShrink", &s.flex_shrink, &d.flex_shrink, |v| num(*v));
    out.diff(
        "gridTemplateRows",
        &s.grid_template_rows,
        &d.grid_template_rows,
        |v| template(v),
    );
    out.diff(
        "gridTemplateColumns",
        &s.grid_template_columns,
        &d.grid_template_columns,
        |v| template(v),
    );
    out.diff("gridAutoRows", &s.grid_auto_rows, &d.grid_auto_rows, |v| {
        tracks(v)
    });
    out.diff(
        "gridAutoColumns",
        &s.grid_auto_columns,
        &d.grid_auto_columns,
        |v| tracks(v),
    );
    out.diff(
        "gridAutoFlow",
        &s.grid_auto_flow,
        &d.grid_auto_flow,
        keyword,
    );
    out.diff(
        "gridRowStart",
        &s.grid_row.start,
        &d.grid_row.start,
        placement,
    );
    out.diff("gridRowEnd", &s.grid_row.end, &d.grid_row.end, placement);
    out.diff(
        "gridColumnStart",
        &s.grid_column.start,
        &d.grid_column.start,
        placement,
    );
    out.diff(
        "gridColumnEnd",
        &s.grid_column.end,
        &d.grid_column.end,
        placement,
    );
}

fn filters(list: &[Filter]) -> String {
    list.iter()
        .map(|f| match *f {
            Filter::Blur(v) => format!("blur({})", num(v)),
            Filter::Brightness(v) => format!("brightness({})", num(v)),
            Filter::Contrast(v) => format!("contrast({})", num(v)),
            Filter::Grayscale(v) => format!("grayscale({})", num(v)),
            Filter::HueRotate(v) => format!("hue-rotate({}deg)", num(v)),
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
                color(&c)
            ),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn box_shadows(list: &[BoxShadow]) -> String {
    list.iter()
        .map(|s| {
            format!(
                "{}{} {} {} {} {}",
                if s.inset { "inset " } else { "" },
                num(s.offset_x),
                num(s.offset_y),
                num(s.blur),
                num(s.spread),
                color(&s.color)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn gradients(list: &[Gradient]) -> String {
    list.iter()
        .map(|g| {
            let at = |x: Percentage, y: Percentage| format!("at {} {}", percent(x), percent(y));
            let (name, head) = match g.kind {
                GradientKind::Linear { angle } => ("linear", format!("{}deg", num(angle))),
                GradientKind::LinearCorner { right, bottom } => (
                    "linear",
                    format!(
                        "to {} {}",
                        if bottom { "bottom" } else { "top" },
                        if right { "right" } else { "left" }
                    ),
                ),
                GradientKind::Radial { circle, size, x, y } => {
                    let size = match size {
                        GradientSize::ClosestSide => "closest-side",
                        GradientSize::ClosestCorner => "closest-corner",
                        GradientSize::FarthestSide => "farthest-side",
                        GradientSize::FarthestCorner => "farthest-corner",
                    };
                    let shape = if circle { "circle" } else { "ellipse" };
                    ("radial", format!("{shape} {size} {}", at(x, y)))
                }
                GradientKind::Conic { from, x, y } => {
                    ("conic", format!("from {}deg {}", num(from), at(x, y)))
                }
            };
            let stops: Vec<String> = g
                .stops
                .iter()
                .map(|s| match s.position {
                    Some(p) => format!("{} {}", color(&s.color), definite(&p)),
                    None => color(&s.color),
                })
                .collect();
            format!(
                "{}{name}-gradient({head}, {})",
                if g.repeating { "repeating-" } else { "" },
                stops.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn curve(c: &[f32; 4]) -> String {
    format!(
        "cubic-bezier({}, {}, {}, {})",
        num(c[0]),
        num(c[1]),
        num(c[2]),
        num(c[3])
    )
}

fn text_shadows(list: &[TextShadow]) -> String {
    if list.is_empty() {
        return "none".into();
    }
    list.iter()
        .map(|s| {
            format!(
                "{} {} {} {}",
                num(s.offset_x),
                num(s.offset_y),
                num(s.radius),
                color(&s.color)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn view(out: &mut Decls, style: &Style, default: &Style) {
    let (v, d) = (&style.view, &default.view);
    out.diff(
        "backgroundColor",
        &v.background_color,
        &d.background_color,
        color,
    );
    out.diff("borderRadius", &v.border_radius, &d.border_radius, |v| {
        num(*v)
    });
    let corners = |c: &nsh_core::geometry::Corners<Option<Points>>| {
        [c.top_left, c.top_right, c.bottom_right, c.bottom_left].map(|r| r.map_or("-".into(), num))
    };
    out.sides(
        &CORNERS,
        corners(&v.corner_radius),
        corners(&d.corner_radius),
    );
    out.diff(
        "borderCurve",
        &v.continuous_corners,
        &d.continuous_corners,
        |c| if *c { "continuous" } else { "circular" }.into(),
    );
    let widths =
        |b: &nsh_core::geometry::Edges<Points>| [b.top, b.right, b.bottom, b.left].map(num);
    out.sides(
        &BORDER_WIDTH,
        widths(&v.borders.widths),
        widths(&d.borders.widths),
    );
    let colors = |b: &[Color; 4]| b.map(|c| color(&c));
    out.sides(
        &BORDER_COLOR,
        colors(&v.borders.colors),
        colors(&d.borders.colors),
    );
    out.diff("borderStyle", &v.borders.style, &d.borders.style, keyword);
    out.diff("opacity", &v.opacity, &d.opacity, |v| num(*v));
    out.diff("overflow", &v.overflow_hidden, &d.overflow_hidden, |h| {
        if *h { "hidden" } else { "visible" }.into()
    });
    out.diff("shadowColor", &v.shadow.color, &d.shadow.color, color);
    if (v.shadow.offset_x, v.shadow.offset_y) != (d.shadow.offset_x, d.shadow.offset_y) {
        out.push(
            "shadowOffset",
            format!("{} {}", num(v.shadow.offset_x), num(v.shadow.offset_y)),
        );
    }
    out.diff("shadowOpacity", &v.shadow.opacity, &d.shadow.opacity, |v| {
        num(*v)
    });
    out.diff("shadowRadius", &v.shadow.radius, &d.shadow.radius, |v| {
        num(*v)
    });
    out.diff("transform", &v.transform, &d.transform, |t| {
        let ops: Vec<String> = t
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
    });
    out.diff("tintColor", &v.tint_color, &d.tint_color, |c| {
        c.as_ref().map_or("none".into(), color)
    });
    out.diff("elevation", &v.elevation, &d.elevation, |v| num(*v));
    out.diff(
        "transformOrigin",
        &v.transform_origin,
        &d.transform_origin,
        |o| format!("{} {} {}", definite(&o.x), definite(&o.y), num(o.z)),
    );
    out.diff(
        "backfaceVisibility",
        &v.backface_hidden,
        &d.backface_hidden,
        |h| if *h { "hidden" } else { "visible" }.into(),
    );
    let effects = v.effects.as_deref().cloned().unwrap_or_default();
    let none = d.effects.as_deref().cloned().unwrap_or_default();
    out.diff("boxShadow", &effects.box_shadows, &none.box_shadows, |l| {
        box_shadows(l)
    });
    out.diff(
        "backgroundImage",
        &effects.background_images,
        &none.background_images,
        |l| gradients(l),
    );
    out.diff("filter", &effects.filters, &none.filters, |l| filters(l));
    out.diff(
        "backdropFilter",
        &effects.backdrop_filters,
        &none.backdrop_filters,
        |l| filters(l),
    );
    out.diff(
        "mixBlendMode",
        &v.blend_mode,
        &d.blend_mode,
        |m: &BlendMode| keyword(m),
    );
    out.diff("outlineWidth", &v.outline.width, &d.outline.width, |v| {
        num(*v)
    });
    out.diff("outlineColor", &v.outline.color, &d.outline.color, color);
    out.diff("outlineStyle", &v.outline.style, &d.outline.style, keyword);
    out.diff("outlineOffset", &v.outline.offset, &d.outline.offset, |v| {
        num(*v)
    });
    out.diff("visibility", &v.hidden, &d.hidden, |h| {
        if *h { "hidden" } else { "visible" }.into()
    });
    out.diff("cursor", &v.cursor, &d.cursor, |c: &Cursor| keyword(c));
    if v.transition != d.transition {
        match &v.transition {
            None => out.push("transitionProperty", "none"),
            Some(t) => {
                let names = [
                    (nsh_core::TransitionProps::COLORS, "colors"),
                    (nsh_core::TransitionProps::OPACITY, "opacity"),
                    (nsh_core::TransitionProps::TRANSFORM, "transform"),
                    (nsh_core::TransitionProps::SHADOW, "shadow"),
                    (nsh_core::TransitionProps::SHAPE, "shape"),
                ];
                let props: Vec<&str> = names
                    .iter()
                    .filter(|(bit, _)| t.properties.has(*bit))
                    .map(|(_, n)| *n)
                    .collect();
                out.push(
                    "transitionProperty",
                    if props.is_empty() {
                        "none".into()
                    } else {
                        props.join(" ")
                    },
                );
                out.push("transitionDuration", format!("{}ms", num(t.duration)));
                out.push("transitionDelay", format!("{}ms", num(t.delay)));
                out.push("transitionTimingFunction", curve(&t.easing));
            }
        }
    }
    if v.animation != d.animation {
        match &v.animation {
            None => out.push("animationName", "none"),
            Some(a) => {
                out.push("animationName", keyword(&a.keyframes));
                out.push("animationDuration", format!("{}ms", num(a.duration)));
                out.push("animationDelay", format!("{}ms", num(a.delay)));
                out.push("animationTimingFunction", curve(&a.easing));
                out.push(
                    "animationIterationCount",
                    a.iterations.map_or("infinite".into(), |n| n.to_string()),
                );
            }
        }
    }
    out.diff("zIndex", &style.z_index, &default.z_index, |v| {
        v.to_string()
    });
    out.diff(
        "verticalAlign",
        &style.vertical_align,
        &default.vertical_align,
        |v| {
            match v {
                nsh_core::VerticalAlign::Baseline => "baseline",
                nsh_core::VerticalAlign::TextTop => "text-top",
                nsh_core::VerticalAlign::TextBottom => "text-bottom",
                nsh_core::VerticalAlign::Middle => "middle",
            }
            .into()
        },
    );
    out.diff("direction", &style.direction, &default.direction, |v| {
        match v {
            None => "inherit",
            Some(true) => "rtl",
            Some(false) => "ltr",
        }
        .into()
    });
    let (l, ld) = (&style.logical, &default.logical);
    let opt = |v: Option<String>| v.unwrap_or_else(|| "-".into());
    for (i, key) in ["marginStart", "marginEnd"].into_iter().enumerate() {
        out.diff(key, &l.margin[i], &ld.margin[i], |v| {
            opt(v.as_ref().map(lpa))
        });
    }
    for (i, key) in ["paddingStart", "paddingEnd"].into_iter().enumerate() {
        out.diff(key, &l.padding[i], &ld.padding[i], |v| {
            opt(v.as_ref().map(lp))
        });
    }
    for (i, key) in ["start", "end"].into_iter().enumerate() {
        out.diff(key, &l.inset[i], &ld.inset[i], |v| opt(v.as_ref().map(lpa)));
    }
    for (i, key) in ["borderStartWidth", "borderEndWidth"]
        .into_iter()
        .enumerate()
    {
        out.diff(key, &l.border_width[i], &ld.border_width[i], |v| {
            opt(v.map(num))
        });
    }
    for (i, key) in ["borderStartColor", "borderEndColor"]
        .into_iter()
        .enumerate()
    {
        out.diff(key, &l.border_color[i], &ld.border_color[i], |v| {
            opt(v.as_ref().map(color))
        });
    }
    let corners = |c: &[Option<Points>; 4]| c.map(|r| r.map_or("-".into(), num));
    out.sides(&LOGICAL_CORNERS, corners(&l.radius), corners(&ld.radius));
    if style.animated != default.animated {
        out.push("animated", style.animated.len().to_string());
    }
}

fn text(out: &mut Decls, t: &nsh_core::TextStyle, d: &nsh_core::TextStyle) {
    fn some<T>(v: &Option<T>, f: impl Fn(&T) -> String) -> String {
        v.as_ref().map_or("inherit".into(), f)
    }
    out.diff("color", &t.color, &d.color, |v| some(v, color));
    out.diff("fontSize", &t.font_size, &d.font_size, |v| {
        some(v, |v| num(*v))
    });
    out.diff("fontWeight", &t.font_weight, &d.font_weight, |v| {
        some(v, |v| v.to_string())
    });
    out.diff("lineHeight", &t.line_height, &d.line_height, |v| {
        some(v, |v| num(*v))
    });
    // A multiple of the font size, as CSS's percentages are.
    out.diff(
        "lineHeight",
        &t.line_height_multiple,
        &d.line_height_multiple,
        |v| some(v, |v| format!("{}%", num(v * 100.0))),
    );
    out.diff("textAlign", &t.text_align, &d.text_align, |v| {
        some(v, keyword)
    });
    out.diff("fontFamily", &t.font_family, &d.font_family, |v| {
        some(v, |f| format!("{:?}", &*f.name()))
    });
    out.diff("fontStyle", &t.italic, &d.italic, |v| {
        some(v, |i| if *i { "italic" } else { "normal" }.into())
    });
    out.diff("letterSpacing", &t.letter_spacing, &d.letter_spacing, |v| {
        some(v, |v| num(*v))
    });
    out.diff(
        "letterSpacing",
        &t.letter_spacing_em,
        &d.letter_spacing_em,
        |v| some(v, |v| format!("{}em", num(*v))),
    );
    out.diff("textDecorationLine", &t.decoration, &d.decoration, |v| {
        some(v, |d| {
            match (d.underline, d.line_through) {
                (false, false) => "none",
                (true, false) => "underline",
                (false, true) => "line-through",
                (true, true) => "underline line-through",
            }
            .into()
        })
    });
    out.diff(
        "textDecorationStyle",
        &t.decoration_style,
        &d.decoration_style,
        |v| some(v, keyword),
    );
    out.diff(
        "textDecorationThickness",
        &t.decoration_thickness,
        &d.decoration_thickness,
        |v| some(v, |v| num(*v)),
    );
    out.diff(
        "textUnderlineOffset",
        &t.underline_offset,
        &d.underline_offset,
        |v| some(v, |v| num(*v)),
    );
    out.diff(
        "textDecorationColor",
        &t.decoration_color,
        &d.decoration_color,
        |v| some(v, color),
    );
    out.diff("textTransform", &t.transform, &d.transform, |v| {
        some(v, keyword)
    });
    out.diff("textShadowColor", &t.shadow_color, &d.shadow_color, |v| {
        some(v, color)
    });
    out.diff(
        "textShadowOffset",
        &t.shadow_offset,
        &d.shadow_offset,
        |v| some(v, |p| format!("{} {}", num(p.x), num(p.y))),
    );
    out.diff(
        "textShadowRadius",
        &t.shadow_radius,
        &d.shadow_radius,
        |v| some(v, |v| num(*v)),
    );
    out.diff("fontVariant", &t.font_variant, &d.font_variant, |v| {
        some(v, |f| {
            let names = [
                (nsh_core::FontVariant::SMALL_CAPS, "small-caps"),
                (nsh_core::FontVariant::OLDSTYLE_NUMS, "oldstyle-nums"),
                (nsh_core::FontVariant::LINING_NUMS, "lining-nums"),
                (nsh_core::FontVariant::TABULAR_NUMS, "tabular-nums"),
                (
                    nsh_core::FontVariant::PROPORTIONAL_NUMS,
                    "proportional-nums",
                ),
            ];
            let set: Vec<&str> = names
                .iter()
                .filter(|(bit, _)| f.0 & bit != 0)
                .map(|(_, n)| *n)
                .collect();
            if set.is_empty() {
                "normal".into()
            } else {
                set.join(" ")
            }
        })
    });
    out.diff(
        "writingDirection",
        &t.writing_direction,
        &d.writing_direction,
        |v| some(v, |r| if *r { "rtl" } else { "ltr" }.into()),
    );
    out.diff(
        "textAlignVertical",
        &t.align_vertical,
        &d.align_vertical,
        |v| {
            some(v, |v| match v.0 {
                0.0 => "top".into(),
                0.5 => "center".into(),
                1.0 => "bottom".into(),
                _ => percent(*v),
            })
        },
    );
    out.diff(
        "placeholderColor",
        &t.placeholder_color,
        &d.placeholder_color,
        |v| some(v, color),
    );
    out.diff(
        "selectionColor",
        &t.selection_color,
        &d.selection_color,
        |v| some(v, color),
    );
}

fn class_props(out: &mut Decls, p: &ClassProps) {
    let d = ClassProps::default();
    out.diff(
        "numberOfLines",
        &p.number_of_lines,
        &d.number_of_lines,
        |v| v.map_or("-".into(), |n| n.to_string()),
    );
    out.diff("ellipsizeMode", &p.ellipsize, &d.ellipsize, |v| {
        v.as_ref().map_or("-".into(), keyword)
    });
    out.diff("whiteSpace", &p.no_wrap, &d.no_wrap, |v| {
        match v {
            Some(true) => "nowrap",
            Some(false) => "normal",
            None => "-",
        }
        .into()
    });
    out.diff("pointerEvents", &p.pointer_events, &d.pointer_events, |v| {
        v.as_ref().map_or("-".into(), keyword)
    });
    out.diff("resizeMode", &p.resize_mode, &d.resize_mode, |v| {
        v.as_ref().map_or("-".into(), keyword)
    });
    out.diff("selectable", &p.selectable, &d.selectable, |v| {
        v.map_or("-".into(), |b| b.to_string())
    });
    out.diff(
        "placeholderTextColor",
        &p.placeholder_color,
        &d.placeholder_color,
        |v| v.as_ref().map_or("-".into(), color),
    );
    out.diff(
        "selectionColor",
        &p.selection_color,
        &d.selection_color,
        |v| v.as_ref().map_or("-".into(), color),
    );
    out.diff("caretColor", &p.caret_color, &d.caret_color, |v| {
        v.as_ref().map_or("-".into(), color)
    });
    out.diff("textShadow", &p.text_shadows, &d.text_shadows, |v| {
        v.as_deref().map_or("-".into(), text_shadows)
    });
}
