//! Tailwind utilities (without variants) to style declarations, and the
//! properties several utilities build together (transforms, box shadows
//! and rings, gradients, filters, transitions).

use std::sync::Arc;

use super::{ClassProps, Theme, deps};
use crate::{
    css,
    effects::{
        BoxShadow, ColorStop, Filter, Gradient, GradientKind, GradientSize, KeyframeAnimation,
        Keyframes, Transition, TransitionProps,
    },
    geometry::{DefiniteLength, Points, REM_SIZE, Size, deg, percentage, point, px},
    ops::ResizeMode,
    props::PointerEvents,
    style::{
        Color, EllipsizeMode, FontVariant, PatchValue, Style, StyleDecl, StyleKey, TextShadow,
    },
    transform::TransformKind,
};

/// A length that may be a fraction of the view's own size (translations).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Len {
    Points(f32),
    Fraction(f32),
}

/// One contribution to a property several utilities build together.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Part {
    TranslateX(Len),
    TranslateY(Len),
    Rotate(f32),
    RotateX(f32),
    RotateY(f32),
    ScaleX(f32),
    ScaleY(f32),
    SkewX(f32),
    SkewY(f32),
    Perspective(f32),
    TransformNone,
    Shadow(Arc<[BoxShadow]>),
    /// A color for the shadow layers; `true` when it has its own alpha
    /// (`shadow-black/50`), else the layers keep theirs.
    ShadowColor(Color, bool),
    InsetShadow(Arc<[BoxShadow]>),
    InsetShadowColor(Color, bool),
    Ring(f32),
    RingInset,
    RingColor(Color),
    RingOffset(f32),
    RingOffsetColor(Color),
    InsetRing(f32),
    InsetRingColor(Color),
    Gradient(GradientKind),
    GradientNone,
    From(Color),
    Via(Color),
    To(Color),
    FromPos(f32),
    ViaPos(f32),
    ToPos(f32),
    /// A filter in its CSS slot; `None` clears the slot.
    Filter {
        backdrop: bool,
        slot: u8,
        filter: Option<Filter>,
    },
    FilterNone {
        backdrop: bool,
    },
    FontVariant {
        set: u8,
        clear: u8,
    },
    TransitionProps(u8),
    Duration(f32),
    Ease([f32; 4]),
    Delay(f32),
    TransitionNone,
    Animate(Option<Keyframes>),
    /// A keyframe animation with its own timing.
    Animation(KeyframeAnimation),
    TextShadow(Arc<[TextShadow]>),
    TextShadowColor(Color, bool),
}

/// A node property set by a class.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PropDecl {
    Lines(u32),
    Ellipsize(EllipsizeMode),
    NoWrap(bool),
    PointerEvents(PointerEvents),
    Resize(ResizeMode),
    Selectable(bool),
    Caret(Color),
}

/// What one utility declares.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Decl {
    Set(StyleKey, PatchValue),
    /// A value the utility implies for another property, applied before
    /// every other rule so any utility of that property wins wherever it is
    /// in the string (`flex` is a row as in CSS, `flex-col flex` a column).
    Default(StyleKey, PatchValue),
    Style(StyleDecl),
    Part(Part),
    Prop(PropDecl),
    /// A length relative to the window (`w-screen`): `(key, fraction of
    /// width, fraction of height)`.
    Screen(StyleKey, f32, f32),
    /// One value in light appearance, another in dark (a theme color of a
    /// Rust component): one declaration, so a later plain one still wins.
    Adaptive(StyleKey, [PatchValue; 2]),
    /// [`Decl::Adaptive`] for a part (a shadow's, ring's or gradient
    /// stop's theme color).
    AdaptivePart(Box<[Part; 2]>),
}

impl Decl {
    pub(crate) fn apply(
        &self,
        dark: bool,
        style: &mut Style,
        props: &mut ClassProps,
        parts: &mut Parts,
    ) {
        match self {
            Decl::Set(key, value) => {
                let _ = style.set_key(*key, value.as_value());
            }
            // Applied by `ClassList::apply` before the rules.
            Decl::Default(..) => {}
            Decl::Style(decl) => {
                let _ = decl.apply(style);
            }
            Decl::Part(part) => parts.push(part.clone()),
            Decl::Prop(p) => match p {
                PropDecl::Lines(n) => props.number_of_lines = Some(*n),
                PropDecl::Ellipsize(m) => props.ellipsize = Some(*m),
                PropDecl::NoWrap(b) => props.no_wrap = Some(*b),
                PropDecl::PointerEvents(p) => props.pointer_events = Some(*p),
                PropDecl::Resize(m) => props.resize_mode = Some(*m),
                PropDecl::Selectable(b) => props.selectable = Some(*b),
                PropDecl::Caret(c) => props.caret_color = Some(*c),
            },
            Decl::Screen(key, fw, fh) => {
                parts.screen.push((*key, *fw, *fh));
                parts.touched |= touched::SCREEN;
            }
            Decl::Adaptive(key, values) => {
                let _ = style.set_key(*key, values[usize::from(dark)].as_value());
            }
            Decl::AdaptivePart(pair) => parts.push(pair[usize::from(dark)].clone()),
        }
    }

    /// The text color this declares, for `placeholder:` rules.
    /// Sets the background color.
    pub(crate) fn sets_background_color(&self) -> bool {
        matches!(
            self,
            Decl::Set(StyleKey::BackgroundColor, _)
                | Decl::Adaptive(StyleKey::BackgroundColor, _)
                | Decl::Style(StyleDecl::Set(StyleKey::BackgroundColor, _))
        )
    }

    /// Sets (or removes) a gradient background, or a part of one.
    pub(crate) fn sets_background_image(&self) -> bool {
        let gradient = |p: &Part| {
            matches!(
                p,
                Part::Gradient(_)
                    | Part::GradientNone
                    | Part::From(_)
                    | Part::Via(_)
                    | Part::To(_)
                    | Part::FromPos(_)
                    | Part::ViaPos(_)
                    | Part::ToPos(_)
            )
        };
        match self {
            Decl::Style(StyleDecl::Set(StyleKey::BackgroundImage, _)) => true,
            Decl::Part(p) => gradient(p),
            Decl::AdaptivePart(p) => gradient(&p[0]),
            _ => false,
        }
    }

    pub(crate) fn text_color(&self) -> Option<Color> {
        match self {
            Decl::Set(StyleKey::Color, PatchValue::Str(s)) => Color::parse(s),
            // `placeholder:` rules are Tailwind's: never adaptive.
            _ => None,
        }
    }

    pub(crate) fn deps(&self) -> u16 {
        match self {
            Decl::Screen(_, w, h) => {
                (if *w != 0.0 { deps::WIDTH } else { 0 })
                    | (if *h != 0.0 { deps::HEIGHT } else { 0 })
            }
            Decl::Adaptive(..) | Decl::AdaptivePart(_) => deps::DARK,
            _ => 0,
        }
    }
}

/// Groups of properties [`Parts::finish`] writes.
pub(crate) mod touched {
    pub(crate) const SCREEN: u16 = 1;
    pub(crate) const TRANSFORM: u16 = 2;
    pub(crate) const SHADOWS: u16 = 4;
    pub(crate) const GRADIENT: u16 = 8;
    pub(crate) const FILTERS: u16 = 16;
    pub(crate) const BACKDROP: u16 = 32;
    pub(crate) const FONT_VARIANT: u16 = 64;
    pub(crate) const TRANSITION: u16 = 128;
    pub(crate) const ANIMATE: u16 = 256;
    pub(crate) const TEXT_SHADOW: u16 = 512;
    pub(crate) const ALL: u16 = u16::MAX;
}

/// Accumulated [`Part`]s of the matching rules, later ones winning.
#[derive(Debug, Default)]
pub(crate) struct Parts {
    /// [`touched`] groups the parts pushed since this was last cleared.
    pub(crate) touched: u16,
    translate: [Option<Len>; 2],
    rotate: [Option<f32>; 3],
    scale: [Option<f32>; 2],
    skew: [Option<f32>; 2],
    perspective: Option<f32>,
    transform_set: bool,
    shadow: Option<Arc<[BoxShadow]>>,
    shadow_color: Option<(Color, bool)>,
    inset_shadow: Option<Arc<[BoxShadow]>>,
    inset_shadow_color: Option<(Color, bool)>,
    ring: Option<f32>,
    ring_inset: bool,
    ring_color: Option<Color>,
    ring_offset: Option<f32>,
    ring_offset_color: Option<Color>,
    inset_ring: Option<f32>,
    inset_ring_color: Option<Color>,
    shadows_set: bool,
    gradient: Option<Option<GradientKind>>,
    stops: [Option<Color>; 3],
    stop_positions: [Option<f32>; 3],
    filters: [Option<Filter>; 9],
    filters_set: bool,
    backdrop: [Option<Filter>; 9],
    backdrop_set: bool,
    font_variant: Option<u8>,
    transition_props: Option<u8>,
    duration: Option<f32>,
    ease: Option<[f32; 4]>,
    delay: Option<f32>,
    transition_none: bool,
    animate: Option<Option<KeyframeAnimation>>,
    text_shadow: Option<Arc<[TextShadow]>>,
    text_shadow_color: Option<(Color, bool)>,
    /// `w-screen` and friends, resolved against the window.
    pub(crate) screen: Vec<(StyleKey, f32, f32)>,
}

impl Parts {
    fn push(&mut self, part: Part) {
        use Part::*;
        self.touched |= match &part {
            TranslateX(_) | TranslateY(_) | Rotate(_) | RotateX(_) | RotateY(_) | ScaleX(_)
            | ScaleY(_) | SkewX(_) | SkewY(_) | Perspective(_) | TransformNone => {
                touched::TRANSFORM
            }
            Shadow(_) | ShadowColor(..) | InsetShadow(_) | InsetShadowColor(..) | Ring(_)
            | RingInset | RingColor(_) | RingOffset(_) | RingOffsetColor(_) | InsetRing(_)
            | InsetRingColor(_) => touched::SHADOWS,
            Gradient(_) | GradientNone | From(_) | Via(_) | To(_) | FromPos(_) | ViaPos(_)
            | ToPos(_) => touched::GRADIENT,
            Filter {
                backdrop: false, ..
            }
            | FilterNone { backdrop: false } => touched::FILTERS,
            Filter { backdrop: true, .. } | FilterNone { backdrop: true } => touched::BACKDROP,
            FontVariant { .. } => touched::FONT_VARIANT,
            TransitionProps(_) | Duration(_) | Ease(_) | Delay(_) | TransitionNone => {
                touched::TRANSITION
            }
            Animate(_) | Animation(_) => touched::ANIMATE,
            TextShadow(_) | TextShadowColor(..) => touched::TEXT_SHADOW,
        };
        match part {
            TranslateX(l) => (self.translate[0], self.transform_set) = (Some(l), true),
            TranslateY(l) => (self.translate[1], self.transform_set) = (Some(l), true),
            Rotate(d) => (self.rotate[2], self.transform_set) = (Some(d), true),
            RotateX(d) => (self.rotate[0], self.transform_set) = (Some(d), true),
            RotateY(d) => (self.rotate[1], self.transform_set) = (Some(d), true),
            ScaleX(s) => (self.scale[0], self.transform_set) = (Some(s), true),
            ScaleY(s) => (self.scale[1], self.transform_set) = (Some(s), true),
            SkewX(d) => (self.skew[0], self.transform_set) = (Some(d), true),
            SkewY(d) => (self.skew[1], self.transform_set) = (Some(d), true),
            Perspective(p) => (self.perspective, self.transform_set) = (Some(p), true),
            TransformNone => {
                self.translate = [None; 2];
                self.rotate = [None; 3];
                self.scale = [None; 2];
                self.skew = [None; 2];
                self.perspective = None;
                self.transform_set = true;
            }
            Shadow(s) => (self.shadow, self.shadows_set) = (Some(s), true),
            ShadowColor(c, a) => (self.shadow_color, self.shadows_set) = (Some((c, a)), true),
            InsetShadow(s) => (self.inset_shadow, self.shadows_set) = (Some(s), true),
            InsetShadowColor(c, a) => {
                (self.inset_shadow_color, self.shadows_set) = (Some((c, a)), true)
            }
            Ring(w) => (self.ring, self.shadows_set) = (Some(w), true),
            RingInset => (self.ring_inset, self.shadows_set) = (true, true),
            RingColor(c) => (self.ring_color, self.shadows_set) = (Some(c), true),
            RingOffset(w) => (self.ring_offset, self.shadows_set) = (Some(w), true),
            RingOffsetColor(c) => (self.ring_offset_color, self.shadows_set) = (Some(c), true),
            InsetRing(w) => (self.inset_ring, self.shadows_set) = (Some(w), true),
            InsetRingColor(c) => (self.inset_ring_color, self.shadows_set) = (Some(c), true),
            Gradient(k) => self.gradient = Some(Some(k)),
            GradientNone => self.gradient = Some(None),
            From(c) => self.stops[0] = Some(c),
            Via(c) => self.stops[1] = Some(c),
            To(c) => self.stops[2] = Some(c),
            FromPos(p) => self.stop_positions[0] = Some(p),
            ViaPos(p) => self.stop_positions[1] = Some(p),
            ToPos(p) => self.stop_positions[2] = Some(p),
            Filter {
                backdrop: false,
                slot,
                filter,
            } => {
                self.filters[slot as usize] = filter;
                self.filters_set = true;
            }
            Filter {
                backdrop: true,
                slot,
                filter,
            } => {
                self.backdrop[slot as usize] = filter;
                self.backdrop_set = true;
            }
            FilterNone { backdrop: false } => {
                self.filters = [None; 9];
                self.filters_set = true;
            }
            FilterNone { backdrop: true } => {
                self.backdrop = [None; 9];
                self.backdrop_set = true;
            }
            FontVariant { set, clear } => {
                let bits = self.font_variant.unwrap_or(0);
                self.font_variant = Some(bits & !clear | set);
            }
            TransitionProps(p) => {
                self.transition_props = Some(p);
                self.transition_none = false;
            }
            Duration(ms) => self.duration = Some(ms),
            Ease(e) => self.ease = Some(e),
            Delay(ms) => self.delay = Some(ms),
            TransitionNone => {
                self.transition_none = true;
                self.transition_props = None;
            }
            Animate(k) => self.animate = Some(k.map(preset)),
            Animation(a) => self.animate = Some(Some(a)),
            TextShadow(layers) => self.text_shadow = Some(layers),
            TextShadowColor(c, a) => self.text_shadow_color = Some((c, a)),
        }
    }

    /// Writes the built properties of the [`touched`] groups in `only`
    /// into `style`; `window` resolves window-relative lengths.
    pub(crate) fn finish(
        &self,
        style: &mut Style,
        props: &mut ClassProps,
        theme: &Theme,
        window: Size<Points>,
        only: u16,
    ) {
        let has = |group: u16| only & group != 0;
        for &(key, fw, fh) in self.screen.iter().filter(|_| has(touched::SCREEN)) {
            let _ = style.set_key(
                key,
                crate::style::StyleValue::Number(fw * window.width.0 + fh * window.height.0),
            );
        }
        if self.transform_set && has(touched::TRANSFORM) {
            let _ = StyleDecl::ClearTransform.apply(style);
            // CSS: `translate`, `rotate`, `scale`, then `transform`
            // (rotate-x / -y, skews); a list applies right to left.
            let mut push = |kind: TransformKind, v: f32| {
                let _ = style.view.transform.push(kind, v);
            };
            if let Some(p) = self.perspective {
                push(TransformKind::Perspective, p);
            }
            for (i, t) in self.translate.iter().enumerate() {
                match (i, t) {
                    (0, Some(Len::Points(v))) => push(TransformKind::TranslateX, *v),
                    (0, Some(Len::Fraction(f))) => push(TransformKind::TranslateXFraction, *f),
                    (1, Some(Len::Points(v))) => push(TransformKind::TranslateY, *v),
                    (1, Some(Len::Fraction(f))) => push(TransformKind::TranslateYFraction, *f),
                    _ => {}
                }
            }
            if let Some(r) = self.rotate[2] {
                push(TransformKind::Rotate, r);
            }
            match self.scale {
                [Some(x), Some(y)] if x == y => push(TransformKind::Scale, x),
                [x, y] => {
                    if let Some(x) = x {
                        push(TransformKind::ScaleX, x);
                    }
                    if let Some(y) = y {
                        push(TransformKind::ScaleY, y);
                    }
                }
            }
            if let Some(r) = self.rotate[0] {
                push(TransformKind::RotateX, r);
            }
            if let Some(r) = self.rotate[1] {
                push(TransformKind::RotateY, r);
            }
            if let Some(s) = self.skew[0] {
                push(TransformKind::SkewX, s);
            }
            if let Some(s) = self.skew[1] {
                push(TransformKind::SkewY, s);
            }
        }
        if self.shadows_set && has(touched::SHADOWS) {
            let current = style.text.color.unwrap_or(Color::BLACK);
            let recolor = |layers: &[BoxShadow], color: Option<(Color, bool)>| -> Vec<BoxShadow> {
                layers
                    .iter()
                    .map(|l| match color {
                        // The layer keeps its alpha unless the color sets one.
                        Some((c, false)) => BoxShadow {
                            color: c.with_alpha((l.color.0 & 0xff) as f32 / 255.0),
                            ..*l
                        },
                        Some((c, true)) => BoxShadow { color: c, ..*l },
                        None => *l,
                    })
                    .collect()
            };
            // CSS order, top layer first: inset shadow, inset ring, ring
            // offset, ring, shadow.
            let mut list = Vec::new();
            if let Some(s) = &self.inset_shadow {
                list.extend(recolor(s, self.inset_shadow_color));
            }
            if let Some(w) = self.inset_ring {
                list.push(BoxShadow {
                    spread: px(w),
                    color: self.inset_ring_color.unwrap_or(current),
                    inset: true,
                    ..BoxShadow::default()
                });
            }
            let offset = self.ring_offset.unwrap_or(0.0);
            if let Some(w) = self.ring {
                if offset > 0.0 {
                    list.push(BoxShadow {
                        spread: px(offset),
                        color: self.ring_offset_color.unwrap_or(Color(0xffff_ffff)),
                        inset: self.ring_inset,
                        ..BoxShadow::default()
                    });
                }
                list.push(BoxShadow {
                    spread: px(w + offset),
                    color: self.ring_color.unwrap_or(current),
                    inset: self.ring_inset,
                    ..BoxShadow::default()
                });
            }
            if let Some(s) = &self.shadow {
                list.extend(recolor(s, self.shadow_color));
            }
            style.set_box_shadows(list);
        }
        if let Some(kind) = self.gradient.filter(|_| has(touched::GRADIENT)) {
            match kind {
                None => style.set_background_images(Vec::new()),
                Some(kind) => {
                    let [from, via, to] = self.stops;
                    let [fp, vp, tp] = self.stop_positions;
                    let mut stops = vec![ColorStop {
                        color: from.unwrap_or(Color::TRANSPARENT),
                        position: Some(percentage(fp.unwrap_or(0.0)).into()),
                    }];
                    if let Some(via) = via {
                        stops.push(ColorStop {
                            color: via,
                            position: Some(percentage(vp.unwrap_or(0.5)).into()),
                        });
                    }
                    stops.push(ColorStop {
                        color: to.unwrap_or(Color::TRANSPARENT),
                        position: Some(percentage(tp.unwrap_or(1.0)).into()),
                    });
                    style.set_background_images(vec![Gradient {
                        kind,
                        stops,
                        repeating: false,
                    }]);
                }
            }
        }
        if self.filters_set && has(touched::FILTERS) {
            style.set_filters(self.filters.iter().flatten().copied().collect(), false);
        }
        if self.backdrop_set && has(touched::BACKDROP) {
            style.set_filters(self.backdrop.iter().flatten().copied().collect(), true);
        }
        if let Some(bits) = self.font_variant.filter(|_| has(touched::FONT_VARIANT)) {
            style.text.font_variant = Some(FontVariant(bits));
        }
        if !has(touched::TRANSITION) {
        } else if self.transition_none {
            style.view.transition = None;
        } else if self.transition_props.is_some() || self.duration.is_some() || self.ease.is_some()
        {
            let (duration, ease) = theme.transition;
            style.view.transition = Some(Transition {
                // CSS's initial `transition-property` is `all`.
                properties: TransitionProps(self.transition_props.unwrap_or(TransitionProps::ALL)),
                duration: self.duration.unwrap_or(if self.transition_props.is_some() {
                    duration
                } else {
                    0.0
                }),
                delay: self.delay.unwrap_or(0.0),
                easing: self.ease.unwrap_or(ease),
            });
        }
        if let Some(k) = self.animate.filter(|_| has(touched::ANIMATE)) {
            style.view.animation = k;
        }
        if let Some(layers) = self
            .text_shadow
            .as_ref()
            .filter(|_| has(touched::TEXT_SHADOW))
        {
            // One text shadow natively: the widest layer stands for all.
            let mut shadow = layers
                .iter()
                .copied()
                .max_by(|a, b| a.radius.0.total_cmp(&b.radius.0));
            if let (Some(s), Some((c, own))) = (shadow.as_mut(), self.text_shadow_color) {
                s.color = if own {
                    c
                } else {
                    c.with_alpha((s.color.0 & 0xff) as f32 / 255.0)
                };
            }
            match shadow {
                Some(s) => {
                    style.text.shadow_color = Some(s.color);
                    style.text.shadow_offset = Some(point(s.offset_x, s.offset_y));
                    style.text.shadow_radius = Some(s.radius);
                }
                None => {
                    style.text.shadow_color = None;
                    style.text.shadow_offset = None;
                    style.text.shadow_radius = None;
                }
            }
            props.text_shadows = Some(layers.to_vec());
        }
    }
}

// --- Values -----------------------------------------------------------------

/// `[...]` contents with `_` as spaces (`\_` stays an underscore).
fn arbitrary(v: &str) -> Option<String> {
    let inner = v.strip_prefix('[')?.strip_suffix(']')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'_') => {
                out.push('_');
                chars.next();
            }
            '_' => out.push(' '),
            c => out.push(c),
        }
    }
    (!out.is_empty()).then_some(out)
}

/// An arbitrary value without a type hint (`[length:2px]` -> `2px`).
fn arbitrary_hinted<'a>(v: &'a str, hint: &str) -> Option<&'a str> {
    v.strip_prefix(hint).and_then(|v| v.strip_prefix(':'))
}

fn num(v: &str) -> Option<f32> {
    css::number(v).filter(|n| *n >= 0.0)
}

/// A spacing step count (`4`, `1.5`; quarter steps as in v4).
fn steps(v: &str) -> Option<f32> {
    let n = num(v)?;
    ((n * 4.0).fract() == 0.0).then_some(n)
}

/// `a/b` as a fraction.
fn fraction(v: &str) -> Option<f32> {
    let (a, b) = v.split_once('/')?;
    let (a, b) = (num(a)?, num(b)?);
    (b > 0.0).then(|| a / b)
}

/// A length value of a spacing-like utility: steps, `px`, fractions,
/// `full`, `auto` (when allowed), or arbitrary lengths / percentages.
fn length_value(theme: &Theme, v: &str, neg: bool, auto: bool) -> Option<PatchValue> {
    let sign = if neg { -1.0 } else { 1.0 };
    if let Some(n) = steps(v) {
        return Some(PatchValue::Number(sign * n * theme.spacing));
    }
    Some(match v {
        "px" => PatchValue::Number(sign),
        "0" => PatchValue::Number(0.0),
        "full" => PatchValue::Str(if neg { "-100%" } else { "100%" }.into()),
        "auto" if auto && !neg => PatchValue::Str("auto".into()),
        v if v.contains('/') && !v.starts_with('[') => {
            let f = fraction(v)?;
            PatchValue::Str(format!("{}%", sign * f * 100.0).into())
        }
        v => {
            let a = arbitrary(v)?;
            let a = arbitrary_hinted(&a, "length").unwrap_or(&a).to_string();
            match css::length_or_percent(&a)? {
                DefiniteLength::Absolute(l) => PatchValue::Number(sign * l.to_points(REM_SIZE).0),
                DefiniteLength::Fraction(f) => {
                    PatchValue::Str(format!("{}%", sign * f.0 * 100.0).into())
                }
            }
        }
    })
}

/// A length in points (no percentages).
fn points(theme: &Theme, v: &str) -> Option<f32> {
    match length_value(theme, v, false, false)? {
        PatchValue::Number(n) => Some(n),
        _ => None,
    }
}

/// A color with an optional opacity modifier (`red-500/50`), and whether
/// the modifier (or an arbitrary color) set its alpha.
fn color_value(theme: &Theme, v: &str) -> Option<(Color, bool)> {
    // `[rgb(0_0_0)]/50` or `red-500/50`: the modifier follows the base.
    let (base, modifier) = if v.starts_with('[') {
        let end = v.find(']')?;
        (&v[..=end], v[end + 1..].strip_prefix('/'))
    } else {
        match v.rsplit_once('/') {
            Some((b, m)) => (b, Some(m)),
            None => (v, None),
        }
    };
    let (color, own_alpha) = if base.starts_with('[') {
        let a = arbitrary(base)?;
        let a = arbitrary_hinted(&a, "color").unwrap_or(&a).to_string();
        let c = css::color(&a)?;
        (c, (c.0 & 0xff) != 0xff)
    } else {
        (theme.color(base)?, false)
    };
    match modifier {
        None => Some((color, own_alpha)),
        Some(m) => {
            let alpha = match arbitrary(m) {
                Some(a) => css::amount(&a)?,
                None => num(m)? / 100.0,
            };
            Some((color.with_alpha(alpha), true))
        }
    }
}

fn color(theme: &Theme, v: &str) -> Option<Color> {
    color_value(theme, v).map(|(c, _)| c)
}

fn color_str(c: Color) -> PatchValue {
    PatchValue::Str(format!("#{:08x}", c.0).into())
}

fn set(key: StyleKey, v: PatchValue) -> Decl {
    Decl::Set(key, v)
}

fn setn(key: StyleKey, n: f32) -> Decl {
    Decl::Set(key, PatchValue::Number(n))
}

fn sets(key: StyleKey, s: &str) -> Decl {
    Decl::Set(key, PatchValue::Str(s.into()))
}

fn one(d: Decl) -> Option<Vec<Decl>> {
    Some(vec![d])
}

fn part(p: Part) -> Option<Vec<Decl>> {
    Some(vec![Decl::Part(p)])
}

/// An angle in degrees: a number (`45`) or arbitrary (`[0.5turn]`).
fn degrees(v: &str) -> Option<f32> {
    match arbitrary(v) {
        Some(a) => css::angle(&a).or_else(|| css::number(&a)),
        None => num(v),
    }
}

/// Milliseconds: `300` or `[0.3s]`.
fn millis(v: &str) -> Option<f32> {
    match arbitrary(v) {
        Some(a) => {
            let a = a.trim();
            if let Some(n) = a.strip_suffix("ms") {
                num(n)
            } else if let Some(n) = a.strip_suffix('s') {
                num(n).map(|s| s * 1000.0)
            } else {
                num(a)
            }
        }
        None => num(v),
    }
}

/// A factor from a percentage step (`scale-150` = 1.5) or arbitrary.
fn percent_factor(v: &str) -> Option<f32> {
    match arbitrary(v) {
        Some(a) => css::amount(&a),
        None => num(v).map(|n| n / 100.0),
    }
}

// --- Utilities --------------------------------------------------------------

/// Parses one utility.
pub(crate) fn parse(theme: &Theme, utility: &str) -> Option<Vec<Decl>> {
    if let Some(d) = keyword(utility) {
        return Some(d);
    }
    let (neg, u) = match utility.strip_prefix('-') {
        Some(u) => (true, u),
        None => (false, utility),
    };
    // The longest matching prefix decides (`inset-x-` before `inset-`).
    for &(prefix, handler) in PREFIXES {
        if let Some(value) = u.strip_prefix(prefix)
            && let Some(value) = value.strip_prefix('-')
            && !value.is_empty()
            && let Some(d) = handler(theme, value, neg)
        {
            return Some(d);
        }
    }
    // Bare forms that also take a value (`shadow`, `ring`, `border`, ...).
    bare(theme, u, neg)
}

type Handler = fn(&Theme, &str, bool) -> Option<Vec<Decl>>;

use StyleKey as K;

/// Utilities without a value.
fn keyword(u: &str) -> Option<Vec<Decl>> {
    let d = match u {
        // Display and position.
        // A CSS flex container is a row; views without the class keep
        // React Native's column.
        "flex" | "inline-flex" => {
            return Some(vec![
                sets(K::Display, "flex"),
                Decl::Default(K::FlexDirection, PatchValue::Str("row".into())),
            ]);
        }
        "block" | "flow-root" => sets(K::Display, "block"),
        // No inline formatting context: inline boxes lay out as views.
        "inline-block" | "inline" => sets(K::Display, "flex"),
        "grid" | "inline-grid" => sets(K::Display, "grid"),
        "hidden" => sets(K::Display, "none"),
        "static" | "relative" | "sticky" => sets(K::Position, "relative"),
        "absolute" | "fixed" => sets(K::Position, "absolute"),
        "visible" => sets(K::Visibility, "visible"),
        "invisible" | "collapse" => sets(K::Visibility, "hidden"),
        "overflow-hidden" | "overflow-clip" | "overflow-x-hidden" | "overflow-y-hidden" => {
            sets(K::Overflow, "hidden")
        }
        "overflow-visible" => sets(K::Overflow, "visible"),
        // Flexbox.
        "flex-row" => sets(K::FlexDirection, "row"),
        "flex-row-reverse" => sets(K::FlexDirection, "row-reverse"),
        "flex-col" => sets(K::FlexDirection, "column"),
        "flex-col-reverse" => sets(K::FlexDirection, "column-reverse"),
        "flex-wrap" => sets(K::FlexWrap, "wrap"),
        "flex-wrap-reverse" => sets(K::FlexWrap, "wrap-reverse"),
        "flex-nowrap" => sets(K::FlexWrap, "nowrap"),
        "flex-1" => {
            return Some(vec![
                setn(K::FlexGrow, 1.0),
                setn(K::FlexShrink, 1.0),
                setn(K::FlexBasis, 0.0),
            ]);
        }
        "flex-auto" => {
            return Some(vec![
                setn(K::FlexGrow, 1.0),
                setn(K::FlexShrink, 1.0),
                sets(K::FlexBasis, "auto"),
            ]);
        }
        "flex-initial" => {
            return Some(vec![
                setn(K::FlexGrow, 0.0),
                setn(K::FlexShrink, 1.0),
                sets(K::FlexBasis, "auto"),
            ]);
        }
        "flex-none" => {
            return Some(vec![
                setn(K::FlexGrow, 0.0),
                setn(K::FlexShrink, 0.0),
                sets(K::FlexBasis, "auto"),
            ]);
        }
        "grow" => setn(K::FlexGrow, 1.0),
        "shrink" => setn(K::FlexShrink, 1.0),
        // Alignment: each class sets what Tailwind CSS's does (its generated
        // CSS is the reference), and the value means what it means in a
        // browser: `start` / `end` are the writing direction's, `flex-start`
        // / `flex-end` the flex direction's, `normal` CSS's `normal`.
        "justify-start" => sets(K::JustifyContent, "flex-start"),
        "justify-normal" => sets(K::JustifyContent, "normal"),
        "justify-end" => sets(K::JustifyContent, "flex-end"),
        "justify-center" => sets(K::JustifyContent, "center"),
        "justify-between" => sets(K::JustifyContent, "space-between"),
        "justify-around" => sets(K::JustifyContent, "space-around"),
        "justify-evenly" => sets(K::JustifyContent, "space-evenly"),
        "justify-stretch" => sets(K::JustifyContent, "stretch"),
        "justify-items-start" => sets(K::JustifyItems, "start"),
        "justify-items-end" => sets(K::JustifyItems, "end"),
        "justify-items-center" => sets(K::JustifyItems, "center"),
        "justify-items-stretch" => sets(K::JustifyItems, "stretch"),
        "justify-items-normal" => sets(K::JustifyItems, "normal"),
        "justify-self-auto" => sets(K::JustifySelf, "auto"),
        "justify-self-start" => sets(K::JustifySelf, "flex-start"),
        "justify-self-end" => sets(K::JustifySelf, "flex-end"),
        "justify-self-center" => sets(K::JustifySelf, "center"),
        "justify-self-stretch" => sets(K::JustifySelf, "stretch"),
        // CSS's `normal` behaves as `stretch` in flex and grid containers
        // alike; the style's `normal` is also what an unset `alignContent`
        // is, which stays React Native's `flex-start` outside grids
        // (`resolve_normal_alignment`), so the class sets `stretch`.
        "content-normal" => sets(K::AlignContent, "stretch"),
        "content-start" => sets(K::AlignContent, "flex-start"),
        "content-end" => sets(K::AlignContent, "flex-end"),
        "content-center" => sets(K::AlignContent, "center"),
        "content-between" => sets(K::AlignContent, "space-between"),
        "content-around" => sets(K::AlignContent, "space-around"),
        "content-evenly" => sets(K::AlignContent, "space-evenly"),
        "content-stretch" => sets(K::AlignContent, "stretch"),
        "items-start" => sets(K::AlignItems, "flex-start"),
        "items-end" => sets(K::AlignItems, "flex-end"),
        "items-center" => sets(K::AlignItems, "center"),
        "items-baseline" => sets(K::AlignItems, "baseline"),
        "items-stretch" => sets(K::AlignItems, "stretch"),
        "self-auto" => sets(K::AlignSelf, "auto"),
        "self-start" => sets(K::AlignSelf, "flex-start"),
        "self-end" => sets(K::AlignSelf, "flex-end"),
        "self-center" => sets(K::AlignSelf, "center"),
        "self-stretch" => sets(K::AlignSelf, "stretch"),
        "self-baseline" => sets(K::AlignSelf, "baseline"),
        "place-content-center" => {
            return Some(vec![
                sets(K::AlignContent, "center"),
                sets(K::JustifyContent, "center"),
            ]);
        }
        "place-content-start" => {
            return Some(vec![
                sets(K::AlignContent, "start"),
                sets(K::JustifyContent, "start"),
            ]);
        }
        "place-content-end" => {
            return Some(vec![
                sets(K::AlignContent, "end"),
                sets(K::JustifyContent, "end"),
            ]);
        }
        "place-content-between" => {
            return Some(vec![
                sets(K::AlignContent, "space-between"),
                sets(K::JustifyContent, "space-between"),
            ]);
        }
        "place-content-stretch" => {
            return Some(vec![
                sets(K::AlignContent, "stretch"),
                sets(K::JustifyContent, "stretch"),
            ]);
        }
        "place-items-center" => {
            return Some(vec![
                sets(K::AlignItems, "center"),
                sets(K::JustifyItems, "center"),
            ]);
        }
        "place-items-start" => {
            return Some(vec![
                sets(K::AlignItems, "start"),
                sets(K::JustifyItems, "start"),
            ]);
        }
        "place-items-end" => {
            return Some(vec![
                sets(K::AlignItems, "end"),
                sets(K::JustifyItems, "end"),
            ]);
        }
        "place-items-stretch" => {
            return Some(vec![
                sets(K::AlignItems, "stretch"),
                sets(K::JustifyItems, "stretch"),
            ]);
        }
        "place-self-auto" => {
            return Some(vec![
                sets(K::AlignSelf, "auto"),
                sets(K::JustifySelf, "auto"),
            ]);
        }
        "place-self-center" => {
            return Some(vec![
                sets(K::AlignSelf, "center"),
                sets(K::JustifySelf, "center"),
            ]);
        }
        "place-self-start" => {
            return Some(vec![
                sets(K::AlignSelf, "start"),
                sets(K::JustifySelf, "start"),
            ]);
        }
        "place-self-end" => {
            return Some(vec![sets(K::AlignSelf, "end"), sets(K::JustifySelf, "end")]);
        }
        "place-self-stretch" => {
            return Some(vec![
                sets(K::AlignSelf, "stretch"),
                sets(K::JustifySelf, "stretch"),
            ]);
        }
        "grid-flow-row" => sets(K::GridAutoFlow, "row"),
        "grid-flow-col" => sets(K::GridAutoFlow, "column"),
        "grid-flow-dense" | "grid-flow-row-dense" => sets(K::GridAutoFlow, "row dense"),
        "grid-flow-col-dense" => sets(K::GridAutoFlow, "column dense"),
        "col-auto" => sets(K::GridColumn, "auto"),
        "row-auto" => sets(K::GridRow, "auto"),
        "col-span-full" => sets(K::GridColumn, "1 / -1"),
        "row-span-full" => sets(K::GridRow, "1 / -1"),
        "grid-cols-none" => sets(K::GridTemplateColumns, "none"),
        "grid-rows-none" => sets(K::GridTemplateRows, "none"),
        "auto-cols-auto" => sets(K::GridAutoColumns, "auto"),
        "auto-cols-min" => sets(K::GridAutoColumns, "min-content"),
        "auto-cols-max" => sets(K::GridAutoColumns, "max-content"),
        "auto-cols-fr" => sets(K::GridAutoColumns, "minmax(0, 1fr)"),
        "auto-rows-auto" => sets(K::GridAutoRows, "auto"),
        "auto-rows-min" => sets(K::GridAutoRows, "min-content"),
        "auto-rows-max" => sets(K::GridAutoRows, "max-content"),
        "auto-rows-fr" => sets(K::GridAutoRows, "minmax(0, 1fr)"),
        "aspect-auto" => Decl::Set(K::AspectRatio, PatchValue::Null),
        // Typography.
        "italic" => sets(K::FontStyle, "italic"),
        "not-italic" => sets(K::FontStyle, "normal"),
        "underline" => sets(K::TextDecorationLine, "underline"),
        "line-through" => sets(K::TextDecorationLine, "line-through"),
        "no-underline" => sets(K::TextDecorationLine, "none"),
        "decoration-solid" => sets(K::TextDecorationStyle, "solid"),
        "decoration-double" => sets(K::TextDecorationStyle, "double"),
        "decoration-dotted" => sets(K::TextDecorationStyle, "dotted"),
        "decoration-dashed" => sets(K::TextDecorationStyle, "dashed"),
        "decoration-wavy" => sets(K::TextDecorationStyle, "wavy"),
        "decoration-auto" | "decoration-from-font" => sets(K::TextDecorationThickness, "auto"),
        "underline-offset-auto" => sets(K::TextUnderlineOffset, "auto"),
        "uppercase" => sets(K::TextTransform, "uppercase"),
        "lowercase" => sets(K::TextTransform, "lowercase"),
        "capitalize" => sets(K::TextTransform, "capitalize"),
        "normal-case" => sets(K::TextTransform, "none"),
        "text-left" => sets(K::TextAlign, "left"),
        "text-center" => sets(K::TextAlign, "center"),
        "text-right" => sets(K::TextAlign, "right"),
        "text-justify" => sets(K::TextAlign, "justify"),
        "text-start" => sets(K::TextAlign, "auto"),
        "text-end" => sets(K::TextAlign, "right"),
        "text-ellipsis" => Decl::Prop(PropDecl::Ellipsize(EllipsizeMode::Tail)),
        "text-clip" => Decl::Prop(PropDecl::Ellipsize(EllipsizeMode::Clip)),
        "truncate" => {
            return Some(vec![
                Decl::Prop(PropDecl::Lines(1)),
                Decl::Prop(PropDecl::Ellipsize(EllipsizeMode::Tail)),
                Decl::Prop(PropDecl::NoWrap(true)),
                sets(K::Overflow, "hidden"),
            ]);
        }
        "whitespace-nowrap" | "text-nowrap" => Decl::Prop(PropDecl::NoWrap(true)),
        "whitespace-normal" | "text-wrap" | "text-balance" | "text-pretty" => {
            Decl::Prop(PropDecl::NoWrap(false))
        }
        "line-clamp-none" => Decl::Prop(PropDecl::Lines(0)),
        "normal-nums" => Decl::Part(Part::FontVariant {
            set: 0,
            clear: 0xff,
        }),
        "small-caps" => Decl::Part(Part::FontVariant {
            set: FontVariant::SMALL_CAPS,
            clear: 0,
        }),
        "oldstyle-nums" => Decl::Part(Part::FontVariant {
            set: FontVariant::OLDSTYLE_NUMS,
            clear: FontVariant::LINING_NUMS,
        }),
        "lining-nums" => Decl::Part(Part::FontVariant {
            set: FontVariant::LINING_NUMS,
            clear: FontVariant::OLDSTYLE_NUMS,
        }),
        "tabular-nums" => Decl::Part(Part::FontVariant {
            set: FontVariant::TABULAR_NUMS,
            clear: FontVariant::PROPORTIONAL_NUMS,
        }),
        "proportional-nums" => Decl::Part(Part::FontVariant {
            set: FontVariant::PROPORTIONAL_NUMS,
            clear: FontVariant::TABULAR_NUMS,
        }),
        // Backgrounds.
        "bg-none" => Decl::Part(Part::GradientNone),
        "bg-linear-to-t" | "bg-gradient-to-t" => {
            Decl::Part(Part::Gradient(GradientKind::Linear { angle: deg(0.0) }))
        }
        "bg-linear-to-r" | "bg-gradient-to-r" => {
            Decl::Part(Part::Gradient(GradientKind::Linear { angle: deg(90.0) }))
        }
        "bg-linear-to-b" | "bg-gradient-to-b" => {
            Decl::Part(Part::Gradient(GradientKind::Linear { angle: deg(180.0) }))
        }
        "bg-linear-to-l" | "bg-gradient-to-l" => {
            Decl::Part(Part::Gradient(GradientKind::Linear { angle: deg(270.0) }))
        }
        "bg-linear-to-tr" | "bg-gradient-to-tr" => {
            Decl::Part(Part::Gradient(GradientKind::LinearCorner {
                right: true,
                bottom: false,
            }))
        }
        "bg-linear-to-br" | "bg-gradient-to-br" => {
            Decl::Part(Part::Gradient(GradientKind::LinearCorner {
                right: true,
                bottom: true,
            }))
        }
        "bg-linear-to-bl" | "bg-gradient-to-bl" => {
            Decl::Part(Part::Gradient(GradientKind::LinearCorner {
                right: false,
                bottom: true,
            }))
        }
        "bg-linear-to-tl" | "bg-gradient-to-tl" => {
            Decl::Part(Part::Gradient(GradientKind::LinearCorner {
                right: false,
                bottom: false,
            }))
        }
        "bg-radial" => Decl::Part(Part::Gradient(GradientKind::Radial {
            circle: false,
            size: GradientSize::FarthestCorner,
            x: percentage(0.5),
            y: percentage(0.5),
        })),
        "bg-conic" => Decl::Part(Part::Gradient(GradientKind::Conic {
            from: deg(0.0),
            x: percentage(0.5),
            y: percentage(0.5),
        })),
        // Borders and outlines.
        "border-solid" | "border-double" => sets(K::BorderStyle, "solid"),
        "border-dashed" => sets(K::BorderStyle, "dashed"),
        "border-dotted" => sets(K::BorderStyle, "dotted"),
        "border-none" | "border-hidden" => setn(K::BorderWidth, 0.0),
        "outline-solid" | "outline-double" => sets(K::OutlineStyle, "solid"),
        "outline-dashed" => sets(K::OutlineStyle, "dashed"),
        "outline-dotted" => sets(K::OutlineStyle, "dotted"),
        "outline-none" | "outline-hidden" => sets(K::OutlineStyle, "none"),
        "ring-inset" => Decl::Part(Part::RingInset),
        // Effects.
        "shadow-none" => Decl::Part(Part::Shadow(Arc::from([]))),
        "inset-shadow-none" => Decl::Part(Part::InsetShadow(Arc::from([]))),
        "text-shadow-none" => Decl::Part(Part::TextShadow(Arc::from([]))),
        "filter-none" => Decl::Part(Part::FilterNone { backdrop: false }),
        "backdrop-filter-none" => Decl::Part(Part::FilterNone { backdrop: true }),
        "grayscale" => filter(false, 3, Filter::Grayscale(1.0)),
        "invert" => filter(false, 5, Filter::Invert(1.0)),
        "sepia" => filter(false, 7, Filter::Sepia(1.0)),
        "backdrop-grayscale" => filter(true, 3, Filter::Grayscale(1.0)),
        "backdrop-invert" => filter(true, 5, Filter::Invert(1.0)),
        "backdrop-sepia" => filter(true, 8, Filter::Sepia(1.0)),
        // Transforms.
        "transform-none" => Decl::Part(Part::TransformNone),
        "backface-hidden" => sets(K::BackfaceVisibility, "hidden"),
        "backface-visible" => sets(K::BackfaceVisibility, "visible"),
        "transform" | "transform-gpu" | "transform-cpu" | "transform-3d" | "transform-flat" => {
            return Some(Vec::new());
        }
        // Transitions and animations.
        "transition" => Decl::Part(Part::TransitionProps(
            TransitionProps::COLORS
                | TransitionProps::OPACITY
                | TransitionProps::SHADOW
                | TransitionProps::TRANSFORM,
        )),
        "transition-all" => Decl::Part(Part::TransitionProps(TransitionProps::ALL)),
        "transition-colors" => Decl::Part(Part::TransitionProps(TransitionProps::COLORS)),
        "transition-opacity" => Decl::Part(Part::TransitionProps(TransitionProps::OPACITY)),
        "transition-shadow" => Decl::Part(Part::TransitionProps(TransitionProps::SHADOW)),
        "transition-transform" => Decl::Part(Part::TransitionProps(TransitionProps::TRANSFORM)),
        "transition-none" => Decl::Part(Part::TransitionNone),
        "ease-initial" => Decl::Part(Part::Ease(Transition::EASE)),
        "animate-spin" => Decl::Part(Part::Animate(Some(Keyframes::Spin))),
        "animate-ping" => Decl::Part(Part::Animate(Some(Keyframes::Ping))),
        "animate-pulse" => Decl::Part(Part::Animate(Some(Keyframes::Pulse))),
        "animate-bounce" => Decl::Part(Part::Animate(Some(Keyframes::Bounce))),
        "animate-none" => Decl::Part(Part::Animate(None)),
        // Interactivity.
        "pointer-events-none" => Decl::Prop(PropDecl::PointerEvents(PointerEvents::None)),
        "pointer-events-auto" => Decl::Prop(PropDecl::PointerEvents(PointerEvents::Auto)),
        "select-none" => Decl::Prop(PropDecl::Selectable(false)),
        "select-text" | "select-all" => Decl::Prop(PropDecl::Selectable(true)),
        "select-auto" => return Some(Vec::new()),
        // Images.
        "object-contain" => Decl::Prop(PropDecl::Resize(ResizeMode::Contain)),
        "object-cover" => Decl::Prop(PropDecl::Resize(ResizeMode::Cover)),
        "object-fill" => Decl::Prop(PropDecl::Resize(ResizeMode::Stretch)),
        "object-none" | "object-scale-down" => Decl::Prop(PropDecl::Resize(ResizeMode::Center)),
        _ => return None,
    };
    Some(vec![d])
}

fn filter(backdrop: bool, slot: u8, f: Filter) -> Decl {
    Decl::Part(Part::Filter {
        backdrop,
        slot,
        filter: Some(f),
    })
}

/// Prefixed utilities, longest prefixes first where they overlap.
static PREFIXES: &[(&str, Handler)] = &[
    // Spacing.
    ("px", |t, v, n| {
        one(set(K::PaddingHorizontal, length_value(t, v, n, false)?))
    }),
    ("py", |t, v, n| {
        one(set(K::PaddingVertical, length_value(t, v, n, false)?))
    }),
    ("pt", |t, v, n| {
        one(set(K::PaddingTop, length_value(t, v, n, false)?))
    }),
    ("pr", |t, v, n| {
        one(set(K::PaddingRight, length_value(t, v, n, false)?))
    }),
    ("pb", |t, v, n| {
        one(set(K::PaddingBottom, length_value(t, v, n, false)?))
    }),
    ("pl", |t, v, n| {
        one(set(K::PaddingLeft, length_value(t, v, n, false)?))
    }),
    ("ps", |t, v, n| {
        one(set(K::PaddingStart, length_value(t, v, n, false)?))
    }),
    ("pe", |t, v, n| {
        one(set(K::PaddingEnd, length_value(t, v, n, false)?))
    }),
    ("p", |t, v, n| {
        one(set(K::Padding, length_value(t, v, n, false)?))
    }),
    ("mx", |t, v, n| {
        one(set(K::MarginHorizontal, length_value(t, v, n, true)?))
    }),
    ("my", |t, v, n| {
        one(set(K::MarginVertical, length_value(t, v, n, true)?))
    }),
    ("mt", |t, v, n| {
        one(set(K::MarginTop, length_value(t, v, n, true)?))
    }),
    ("mr", |t, v, n| {
        one(set(K::MarginRight, length_value(t, v, n, true)?))
    }),
    ("mb", |t, v, n| {
        one(set(K::MarginBottom, length_value(t, v, n, true)?))
    }),
    ("ml", |t, v, n| {
        one(set(K::MarginLeft, length_value(t, v, n, true)?))
    }),
    ("ms", |t, v, n| {
        one(set(K::MarginStart, length_value(t, v, n, true)?))
    }),
    ("me", |t, v, n| {
        one(set(K::MarginEnd, length_value(t, v, n, true)?))
    }),
    ("m", |t, v, n| {
        one(set(K::Margin, length_value(t, v, n, true)?))
    }),
    ("gap-x", |t, v, _| {
        one(set(K::ColumnGap, length_value(t, v, false, false)?))
    }),
    ("gap-y", |t, v, _| {
        one(set(K::RowGap, length_value(t, v, false, false)?))
    }),
    ("gap", |t, v, _| {
        one(set(K::Gap, length_value(t, v, false, false)?))
    }),
    // `space-*` puts margins between children; in flex layouts a gap is
    // the same thing.
    ("space-x", |t, v, _| {
        one(set(K::ColumnGap, length_value(t, v, false, false)?))
    }),
    ("space-y", |t, v, _| {
        one(set(K::RowGap, length_value(t, v, false, false)?))
    }),
    // Insets.
    ("inset-x", |t, v, n| {
        let l = length_value(t, v, n, true)?;
        Some(vec![set(K::Left, l.clone()), set(K::Right, l)])
    }),
    ("inset-y", |t, v, n| {
        let l = length_value(t, v, n, true)?;
        Some(vec![set(K::Top, l.clone()), set(K::Bottom, l)])
    }),
    ("inset", |t, v, n| {
        let l = length_value(t, v, n, true)?;
        Some(vec![
            set(K::Top, l.clone()),
            set(K::Right, l.clone()),
            set(K::Bottom, l.clone()),
            set(K::Left, l),
        ])
    }),
    ("top", |t, v, n| {
        one(set(K::Top, length_value(t, v, n, true)?))
    }),
    ("right", |t, v, n| {
        one(set(K::Right, length_value(t, v, n, true)?))
    }),
    ("bottom", |t, v, n| {
        one(set(K::Bottom, length_value(t, v, n, true)?))
    }),
    ("left", |t, v, n| {
        one(set(K::Left, length_value(t, v, n, true)?))
    }),
    ("start", |t, v, n| {
        one(set(K::Start, length_value(t, v, n, true)?))
    }),
    ("end", |t, v, n| {
        one(set(K::End, length_value(t, v, n, true)?))
    }),
    ("z", |_, v, n| {
        let z = match v {
            "auto" => 0.0,
            v => num(v).or_else(|| arbitrary(v).and_then(|a| css::number(&a)))?,
        };
        one(setn(K::ZIndex, if n { -z } else { z }))
    }),
    // Sizing.
    ("min-w", |t, v, _| size(t, v, K::MinWidth, true)),
    ("max-w", |t, v, _| size(t, v, K::MaxWidth, true)),
    ("min-h", |t, v, _| size(t, v, K::MinHeight, false)),
    ("max-h", |t, v, _| size(t, v, K::MaxHeight, false)),
    ("size", |t, v, _| {
        let mut w = size(t, v, K::Width, true)?;
        w.extend(size(t, v, K::Height, false)?);
        Some(w)
    }),
    ("w", |t, v, _| size(t, v, K::Width, true)),
    ("h", |t, v, _| size(t, v, K::Height, false)),
    ("basis", |t, v, _| {
        one(set(K::FlexBasis, length_value(t, v, false, true)?))
    }),
    ("aspect", |t, v, _| {
        let r = match t.aspect.get(v) {
            Some(r) => *r,
            None => match arbitrary(v) {
                Some(a) => fraction(&a.replace(' ', "")).or_else(|| num(&a))?,
                None => fraction(v)?,
            },
        };
        one(setn(K::AspectRatio, r))
    }),
    // Flex and grid.
    ("flex", |t, v, _| {
        if let Some(n) = num(v) {
            return Some(vec![
                setn(K::FlexGrow, n),
                setn(K::FlexShrink, 1.0),
                setn(K::FlexBasis, 0.0),
            ]);
        }
        // `flex-[2]`, `flex-[2_1_0%]`: the CSS shorthand.
        let a = arbitrary(v)?;
        let w = css::words(&a);
        match w.as_slice() {
            [g] => Some(vec![
                setn(K::FlexGrow, num(g)?),
                setn(K::FlexShrink, 1.0),
                setn(K::FlexBasis, 0.0),
            ]),
            [g, s, b] => {
                let basis = match *b {
                    "auto" => PatchValue::Str("auto".into()),
                    b => length_value(t, &format!("[{b}]"), false, true)?,
                };
                Some(vec![
                    setn(K::FlexGrow, num(g)?),
                    setn(K::FlexShrink, num(s)?),
                    set(K::FlexBasis, basis),
                ])
            }
            _ => None,
        }
    }),
    ("grow", |_, v, _| {
        one(setn(
            K::FlexGrow,
            num(v).or_else(|| arbitrary(v).and_then(|a| num(&a)))?,
        ))
    }),
    ("shrink", |_, v, _| {
        one(setn(
            K::FlexShrink,
            num(v).or_else(|| arbitrary(v).and_then(|a| num(&a)))?,
        ))
    }),
    // Tailwind CSS has no negative grid templates, spans or auto tracks.
    ("grid-cols", |_, v, n| {
        one(set(K::GridTemplateColumns, grid_template(v, n)?))
    }),
    ("grid-rows", |_, v, n| {
        one(set(K::GridTemplateRows, grid_template(v, n)?))
    }),
    ("col-span", |_, v, n| {
        let c = count(v, n)?;
        one(sets(K::GridColumn, &format!("span {c} / span {c}")))
    }),
    ("row-span", |_, v, n| {
        let c = count(v, n)?;
        one(sets(K::GridRow, &format!("span {c} / span {c}")))
    }),
    ("col-start", |_, v, n| {
        one(sets(K::GridColumnStart, &line_index(v, n)?))
    }),
    ("col-end", |_, v, n| {
        one(sets(K::GridColumnEnd, &line_index(v, n)?))
    }),
    ("row-start", |_, v, n| {
        one(sets(K::GridRowStart, &line_index(v, n)?))
    }),
    ("row-end", |_, v, n| {
        one(sets(K::GridRowEnd, &line_index(v, n)?))
    }),
    ("col", |_, v, n| {
        let line = match arbitrary(v) {
            // Left to the style, which logs an invalid one and sets
            // nothing, as a browser drops an invalid declaration.
            Some(a) if !n => unhinted(&a).to_string(),
            // `-col-[2]`: Tailwind CSS's `calc(2 * -1)`; anything else
            // negated is a declaration a browser drops.
            Some(_) => {
                return Some(
                    line_index(v, n).map_or_else(Vec::new, |l| vec![sets(K::GridColumn, &l)]),
                );
            }
            None => line_index(v, n)?,
        };
        one(sets(K::GridColumn, &line))
    }),
    ("row", |_, v, n| {
        let line = match arbitrary(v) {
            // As `col`.
            Some(a) if !n => unhinted(&a).to_string(),
            Some(_) => {
                return Some(
                    line_index(v, n).map_or_else(Vec::new, |l| vec![sets(K::GridRow, &l)]),
                );
            }
            None => line_index(v, n)?,
        };
        one(sets(K::GridRow, &line))
    }),
    ("auto-cols", |_, v, n| auto_tracks(K::GridAutoColumns, v, n)),
    ("auto-rows", |_, v, n| auto_tracks(K::GridAutoRows, v, n)),
    ("order", |_, _, _| Some(Vec::new())),
    // Typography.
    ("font", |t, v, _| {
        if let Some(w) = t.weights.get(v) {
            return one(setn(K::FontWeight, *w as f32));
        }
        if let Some(f) = t.fonts.get(v) {
            return one(match f {
                Some(name) => sets(K::FontFamily, name),
                None => Decl::Set(K::FontFamily, PatchValue::Null),
            });
        }
        let a = arbitrary(v)?;
        match num(&a) {
            Some(w) => one(setn(K::FontWeight, w)),
            None => one(sets(K::FontFamily, a.trim_matches(['"', '\'']))),
        }
    }),
    ("text-shadow", |t, v, _| {
        if let Some(layers) = t.text_shadows.get(v) {
            return part(Part::TextShadow(Arc::from(layers.as_slice())));
        }
        let (c, own) = color_value(t, v)?;
        part(Part::TextShadowColor(c, own))
    }),
    ("text", text),
    ("tracking", |t, v, n| {
        let em = match t.tracking.get(v) {
            Some(em) => *em,
            None => {
                let a = arbitrary(v)?;
                return one(sets(K::LetterSpacing, &a));
            }
        };
        one(sets(
            K::LetterSpacing,
            &format!("{}em", if n { -em } else { em }),
        ))
    }),
    ("leading", |t, v, _| {
        if let Some(m) = t.leading.get(v) {
            return one(sets(K::LineHeight, &m.to_string()));
        }
        match length_value(t, v, false, false)? {
            PatchValue::Number(n) => one(setn(K::LineHeight, n)),
            _ => None,
        }
    }),
    ("line-clamp", |_, v, _| {
        one(Decl::Prop(PropDecl::Lines(num(v)? as u32)))
    }),
    ("decoration", |t, v, _| {
        if let Some(n) = num(v) {
            return one(setn(K::TextDecorationThickness, n));
        }
        if let Some(c) = color(t, v) {
            return one(set(K::TextDecorationColor, color_str(c)));
        }
        one(setn(K::TextDecorationThickness, points(t, v)?))
    }),
    ("underline-offset", |t, v, n| {
        let p = num(v).or_else(|| points(t, v))?;
        one(setn(K::TextUnderlineOffset, if n { -p } else { p }))
    }),
    ("caret", |t, v, _| {
        one(Decl::Prop(PropDecl::Caret(color(t, v)?)))
    }),
    // Backgrounds.
    ("bg-linear", |_, v, n| {
        let a = degrees(v)?;
        part(Part::Gradient(GradientKind::Linear {
            angle: deg(if n { -a } else { a }),
        }))
    }),
    ("bg-conic", |_, v, n| {
        let a = degrees(v)?;
        part(Part::Gradient(GradientKind::Conic {
            from: deg(if n { -a } else { a }),
            x: percentage(0.5),
            y: percentage(0.5),
        }))
    }),
    ("bg", |t, v, _| {
        if let Some(a) = arbitrary(v)
            && a.contains("gradient(")
        {
            if css::gradient(&a).is_none() {
                // Reported here: the class is also unknown, but that does
                // not say the gradient is what was rejected.
                log::warn!("invalid gradient in class `bg-{v}`: {a}");
                return None;
            }
            return one(Decl::Style(StyleDecl::Set(
                K::BackgroundImage,
                PatchValue::Str(a.into()),
            )));
        }
        one(set(K::BackgroundColor, color_str(color(t, v)?)))
    }),
    ("from", |t, v, _| match stop_position(v) {
        Some(p) => part(Part::FromPos(p)),
        None => part(Part::From(color(t, v)?)),
    }),
    ("via", |t, v, _| match stop_position(v) {
        Some(p) => part(Part::ViaPos(p)),
        None => part(Part::Via(color(t, v)?)),
    }),
    ("to", |t, v, _| match stop_position(v) {
        Some(p) => part(Part::ToPos(p)),
        None => part(Part::To(color(t, v)?)),
    }),
    // Borders.
    ("rounded-tl", |t, v, _| {
        radius(t, v, &[K::BorderTopLeftRadius])
    }),
    ("rounded-tr", |t, v, _| {
        radius(t, v, &[K::BorderTopRightRadius])
    }),
    ("rounded-br", |t, v, _| {
        radius(t, v, &[K::BorderBottomRightRadius])
    }),
    ("rounded-bl", |t, v, _| {
        radius(t, v, &[K::BorderBottomLeftRadius])
    }),
    ("rounded-ss", |t, v, _| {
        radius(t, v, &[K::BorderTopStartRadius])
    }),
    ("rounded-se", |t, v, _| {
        radius(t, v, &[K::BorderTopEndRadius])
    }),
    ("rounded-ee", |t, v, _| {
        radius(t, v, &[K::BorderBottomEndRadius])
    }),
    ("rounded-es", |t, v, _| {
        radius(t, v, &[K::BorderBottomStartRadius])
    }),
    ("rounded-t", |t, v, _| {
        radius(t, v, &[K::BorderTopLeftRadius, K::BorderTopRightRadius])
    }),
    ("rounded-r", |t, v, _| {
        radius(t, v, &[K::BorderTopRightRadius, K::BorderBottomRightRadius])
    }),
    ("rounded-b", |t, v, _| {
        radius(
            t,
            v,
            &[K::BorderBottomRightRadius, K::BorderBottomLeftRadius],
        )
    }),
    ("rounded-l", |t, v, _| {
        radius(t, v, &[K::BorderTopLeftRadius, K::BorderBottomLeftRadius])
    }),
    ("rounded-s", |t, v, _| {
        radius(t, v, &[K::BorderTopStartRadius, K::BorderBottomStartRadius])
    }),
    ("rounded-e", |t, v, _| {
        radius(t, v, &[K::BorderTopEndRadius, K::BorderBottomEndRadius])
    }),
    ("rounded", |t, v, _| radius(t, v, &[K::BorderRadius])),
    ("border-x", |t, v, _| {
        border(
            t,
            v,
            &[K::BorderLeftWidth, K::BorderRightWidth],
            &[K::BorderLeftColor, K::BorderRightColor],
        )
    }),
    ("border-y", |t, v, _| {
        border(
            t,
            v,
            &[K::BorderTopWidth, K::BorderBottomWidth],
            &[K::BorderTopColor, K::BorderBottomColor],
        )
    }),
    ("border-t", |t, v, _| {
        border(t, v, &[K::BorderTopWidth], &[K::BorderTopColor])
    }),
    ("border-r", |t, v, _| {
        border(t, v, &[K::BorderRightWidth], &[K::BorderRightColor])
    }),
    ("border-b", |t, v, _| {
        border(t, v, &[K::BorderBottomWidth], &[K::BorderBottomColor])
    }),
    ("border-l", |t, v, _| {
        border(t, v, &[K::BorderLeftWidth], &[K::BorderLeftColor])
    }),
    ("border-s", |t, v, _| {
        border(t, v, &[K::BorderStartWidth], &[K::BorderStartColor])
    }),
    ("border-e", |t, v, _| {
        border(t, v, &[K::BorderEndWidth], &[K::BorderEndColor])
    }),
    ("border", |t, v, _| {
        border(t, v, &[K::BorderWidth], &[K::BorderColor])
    }),
    ("outline-offset", |t, v, n| {
        let p = num(v).or_else(|| points(t, v))?;
        one(setn(K::OutlineOffset, if n { -p } else { p }))
    }),
    ("outline", |t, v, _| {
        if let Some(n) = num(v) {
            return Some(vec![
                setn(K::OutlineWidth, n),
                sets(K::OutlineStyle, "solid"),
            ]);
        }
        if let Some(c) = color(t, v) {
            return one(set(K::OutlineColor, color_str(c)));
        }
        Some(vec![
            setn(K::OutlineWidth, points(t, v)?),
            sets(K::OutlineStyle, "solid"),
        ])
    }),
    ("inset-ring", |t, v, _| {
        if let Some(n) = num(v).or_else(|| arbitrary(v).and_then(|a| css::length(&a))) {
            return part(Part::InsetRing(n));
        }
        part(Part::InsetRingColor(color(t, v)?))
    }),
    ("ring-offset", |t, v, _| {
        if let Some(n) = num(v).or_else(|| arbitrary(v).and_then(|a| css::length(&a))) {
            return part(Part::RingOffset(n));
        }
        part(Part::RingOffsetColor(color(t, v)?))
    }),
    ("ring", |t, v, _| {
        if let Some(n) = num(v).or_else(|| arbitrary(v).and_then(|a| css::length(&a))) {
            return part(Part::Ring(n));
        }
        part(Part::RingColor(color(t, v)?))
    }),
    // Effects.
    ("inset-shadow", |t, v, _| {
        if let Some(s) = t.inset_shadows.get(v) {
            return part(Part::InsetShadow(Arc::from(s.as_slice())));
        }
        if let Some(a) = arbitrary(v)
            && let Some(s) = css::box_shadows(&a)
        {
            return part(Part::InsetShadow(Arc::from(
                s.into_iter()
                    .map(|s| BoxShadow { inset: true, ..s })
                    .collect::<Vec<_>>(),
            )));
        }
        let (c, own) = color_value(t, v)?;
        part(Part::InsetShadowColor(c, own))
    }),
    ("shadow", |t, v, _| {
        if let Some(s) = t.shadows.get(v) {
            return part(Part::Shadow(Arc::from(s.as_slice())));
        }
        if let Some(a) = arbitrary(v)
            && let Some(s) = css::box_shadows(&a)
        {
            return part(Part::Shadow(Arc::from(s)));
        }
        let (c, own) = color_value(t, v)?;
        part(Part::ShadowColor(c, own))
    }),
    ("opacity", |_, v, _| {
        one(setn(K::Opacity, percent_factor(v)?.clamp(0.0, 1.0)))
    }),
    ("mix-blend", |_, v, _| {
        crate::effects::BlendMode::parse(v)?;
        one(sets(K::MixBlendMode, v))
    }),
    ("backdrop-blur", |t, v, _| blur(t, v, true)),
    ("backdrop-brightness", |_, v, _| {
        one(filter(true, 1, Filter::Brightness(percent_factor(v)?)))
    }),
    ("backdrop-contrast", |_, v, _| {
        one(filter(true, 2, Filter::Contrast(percent_factor(v)?)))
    }),
    ("backdrop-grayscale", |_, v, _| {
        one(filter(true, 3, Filter::Grayscale(percent_factor(v)?)))
    }),
    ("backdrop-hue-rotate", |_, v, n| {
        one(filter(
            true,
            4,
            Filter::HueRotate(deg(if n { -degrees(v)? } else { degrees(v)? })),
        ))
    }),
    ("backdrop-invert", |_, v, _| {
        one(filter(true, 5, Filter::Invert(percent_factor(v)?)))
    }),
    ("backdrop-opacity", |_, v, _| {
        one(filter(true, 6, Filter::Opacity(percent_factor(v)?)))
    }),
    ("backdrop-saturate", |_, v, _| {
        one(filter(true, 7, Filter::Saturate(percent_factor(v)?)))
    }),
    ("backdrop-sepia", |_, v, _| {
        one(filter(true, 8, Filter::Sepia(percent_factor(v)?)))
    }),
    ("blur", |t, v, _| blur(t, v, false)),
    ("brightness", |_, v, _| {
        one(filter(false, 1, Filter::Brightness(percent_factor(v)?)))
    }),
    ("contrast", |_, v, _| {
        one(filter(false, 2, Filter::Contrast(percent_factor(v)?)))
    }),
    ("grayscale", |_, v, _| {
        one(filter(false, 3, Filter::Grayscale(percent_factor(v)?)))
    }),
    ("hue-rotate", |_, v, n| {
        one(filter(
            false,
            4,
            Filter::HueRotate(deg(if n { -degrees(v)? } else { degrees(v)? })),
        ))
    }),
    ("invert", |_, v, _| {
        one(filter(false, 5, Filter::Invert(percent_factor(v)?)))
    }),
    ("saturate", |_, v, _| {
        one(filter(false, 6, Filter::Saturate(percent_factor(v)?)))
    }),
    ("sepia", |_, v, _| {
        one(filter(false, 7, Filter::Sepia(percent_factor(v)?)))
    }),
    ("drop-shadow", |t, v, _| {
        let layers = match t.drop_shadows.get(v) {
            Some(l) => l.clone(),
            None => css::box_shadows(&arbitrary(v)?)?,
        };
        // One drop shadow natively: the last (widest) layer.
        let l = layers.last().copied().unwrap_or_default();
        one(filter(
            false,
            8,
            Filter::DropShadow {
                offset_x: l.offset_x,
                offset_y: l.offset_y,
                blur: l.blur,
                color: l.color,
            },
        ))
    }),
    // Transforms.
    ("translate-x", |t, v, n| {
        part(Part::TranslateX(translate(t, v, n)?))
    }),
    ("translate-y", |t, v, n| {
        part(Part::TranslateY(translate(t, v, n)?))
    }),
    ("translate", |t, v, n| {
        let l = translate(t, v, n)?;
        Some(vec![
            Decl::Part(Part::TranslateX(l)),
            Decl::Part(Part::TranslateY(l)),
        ])
    }),
    ("rotate-x", |_, v, n| {
        part(Part::RotateX(signed(degrees(v)?, n)))
    }),
    ("rotate-y", |_, v, n| {
        part(Part::RotateY(signed(degrees(v)?, n)))
    }),
    ("rotate-z", |_, v, n| {
        part(Part::Rotate(signed(degrees(v)?, n)))
    }),
    ("rotate", |_, v, n| {
        part(Part::Rotate(signed(degrees(v)?, n)))
    }),
    ("scale-x", |_, v, n| {
        part(Part::ScaleX(signed(percent_factor(v)?, n)))
    }),
    ("scale-y", |_, v, n| {
        part(Part::ScaleY(signed(percent_factor(v)?, n)))
    }),
    ("scale", |_, v, n| {
        let s = signed(percent_factor(v)?, n);
        Some(vec![
            Decl::Part(Part::ScaleX(s)),
            Decl::Part(Part::ScaleY(s)),
        ])
    }),
    ("skew-x", |_, v, n| {
        part(Part::SkewX(signed(degrees(v)?, n)))
    }),
    ("skew-y", |_, v, n| {
        part(Part::SkewY(signed(degrees(v)?, n)))
    }),
    ("skew", |_, v, n| {
        let s = signed(degrees(v)?, n);
        Some(vec![Decl::Part(Part::SkewX(s)), Decl::Part(Part::SkewY(s))])
    }),
    ("perspective", |t, v, _| {
        let p = match t.perspective.get(v) {
            Some(p) => *p,
            None => css::length(&arbitrary(v)?)?,
        };
        part(Part::Perspective(p))
    }),
    ("origin", |_, v, _| {
        let o = match v {
            "center" => "center",
            "top" => "top",
            "top-right" => "top right",
            "right" => "right",
            "bottom-right" => "bottom right",
            "bottom" => "bottom",
            "bottom-left" => "bottom left",
            "left" => "left",
            "top-left" => "top left",
            v => return one(sets(K::TransformOrigin, &arbitrary(v)?)),
        };
        one(sets(K::TransformOrigin, o))
    }),
    // Transitions.
    ("duration", |_, v, _| part(Part::Duration(millis(v)?))),
    ("delay", |_, v, _| part(Part::Delay(millis(v)?))),
    ("ease", |t, v, _| {
        let e = match t.ease.get(v) {
            Some(e) => *e,
            None => {
                let a = arbitrary(v)?;
                let args = a.strip_prefix("cubic-bezier(")?.strip_suffix(')')?;
                let n: Vec<f32> = args.split(',').filter_map(css::number).collect();
                let [a, b, c, d] = n.as_slice() else {
                    return None;
                };
                [*a, *b, *c, *d]
            }
        };
        part(Part::Ease(e))
    }),
    ("cursor", |_, v, _| {
        let name = arbitrary(v).unwrap_or_else(|| v.to_string());
        crate::effects::Cursor::parse(&name)?;
        one(sets(K::Cursor, &name))
    }),
    // Images.
    ("tint", |t, v, _| {
        one(set(K::TintColor, color_str(color(t, v)?)))
    }),
];

/// Forms without a value: `shadow`, `ring`, `border`, `rounded`, ...
fn bare(theme: &Theme, u: &str, neg: bool) -> Option<Vec<Decl>> {
    if neg {
        return None;
    }
    Some(match u {
        "shadow" => return part(Part::Shadow(Arc::from(theme.shadows.get("")?.as_slice()))),
        "inset-shadow" => {
            return part(Part::InsetShadow(Arc::from(
                theme.inset_shadows.get("")?.as_slice(),
            )));
        }
        "drop-shadow" => {
            let l = theme.drop_shadows.get("")?.last().copied()?;
            vec![filter(
                false,
                8,
                Filter::DropShadow {
                    offset_x: l.offset_x,
                    offset_y: l.offset_y,
                    blur: l.blur,
                    color: l.color,
                },
            )]
        }
        "ring" => return part(Part::Ring(1.0)),
        "inset-ring" => return part(Part::InsetRing(1.0)),
        "border" => vec![setn(K::BorderWidth, 1.0)],
        "border-x" => vec![
            setn(K::BorderLeftWidth, 1.0),
            setn(K::BorderRightWidth, 1.0),
        ],
        "border-y" => vec![
            setn(K::BorderTopWidth, 1.0),
            setn(K::BorderBottomWidth, 1.0),
        ],
        "border-t" => vec![setn(K::BorderTopWidth, 1.0)],
        "border-r" => vec![setn(K::BorderRightWidth, 1.0)],
        "border-b" => vec![setn(K::BorderBottomWidth, 1.0)],
        "border-l" => vec![setn(K::BorderLeftWidth, 1.0)],
        "border-s" => vec![setn(K::BorderStartWidth, 1.0)],
        "border-e" => vec![setn(K::BorderEndWidth, 1.0)],
        "outline" => vec![setn(K::OutlineWidth, 1.0), sets(K::OutlineStyle, "solid")],
        "rounded" => return radius(theme, "sm", &[K::BorderRadius]),
        "rounded-t" | "rounded-r" | "rounded-b" | "rounded-l" | "rounded-tl" | "rounded-tr"
        | "rounded-br" | "rounded-bl" | "rounded-s" | "rounded-e" | "rounded-ss" | "rounded-se"
        | "rounded-ee" | "rounded-es" => {
            return parse(theme, &format!("{u}-sm"));
        }
        "blur" => return blur(theme, "", false),
        "backdrop-blur" => return blur(theme, "", true),
        "grow" => vec![setn(K::FlexGrow, 1.0)],
        "shrink" => vec![setn(K::FlexShrink, 1.0)],
        "transition" => return keyword("transition"),
        _ => return None,
    })
}

fn signed(v: f32, neg: bool) -> f32 {
    if neg { -v } else { v }
}

fn count(v: &str, neg: bool) -> Option<u32> {
    if neg {
        return None;
    }
    match arbitrary(v) {
        Some(a) => unhinted(&a).trim().parse().ok().filter(|&n| n > 0),
        None => v.parse().ok().filter(|&n| n > 0),
    }
}

fn line_index(v: &str, neg: bool) -> Option<String> {
    if v == "auto" {
        return (!neg).then(|| "auto".into());
    }
    let n: i32 = match arbitrary(v) {
        Some(a) => match unhinted(&a).trim().parse() {
            Ok(n) => n,
            // A named line (`col-start-[content-start]`).
            Err(_) if !neg => {
                let a = unhinted(&a);
                return crate::grid::placement(a).map(|_| a.to_string());
            }
            Err(_) => return None,
        },
        None => v.parse().ok()?,
    };
    Some((if neg { -n } else { n }).to_string())
}

fn grid_template(v: &str, neg: bool) -> Option<PatchValue> {
    if neg {
        return None;
    }
    if let Ok(n) = v.parse::<u16>() {
        return (n > 0).then(|| PatchValue::Str(format!("repeat({n}, minmax(0, 1fr))").into()));
    }
    let a = arbitrary(v)?;
    let a = track_list(&a)?;
    crate::grid::template(a)?;
    Some(PatchValue::Str(a.into()))
}

/// `auto-cols-[…]` / `auto-rows-[…]`: an invalid value is the style's to
/// log, and sets nothing, as a browser drops the declaration.
fn auto_tracks(key: StyleKey, v: &str, neg: bool) -> Option<Vec<Decl>> {
    let a = arbitrary(v).filter(|_| !neg)?;
    Some(track_list(&a).map_or_else(Vec::new, |a| vec![sets(key, a)]))
}

/// An arbitrary track list without its type hint; `None` when the hint
/// makes it a bare number, which CSS has no track size for (unlike a
/// bare number without a hint, NSH's length in points).
fn track_list(a: &str) -> Option<&str> {
    match hint(a) {
        (Some("number" | "integer"), _) => None,
        (_, a) => Some(a),
    }
}

/// An arbitrary value's type hint and the value without it (`[number:2]`
/// -> `2`), as Tailwind CSS writes the value into the declaration
/// whatever the hint.
fn hint(a: &str) -> (Option<&str>, &str) {
    match a.split_once(':') {
        Some((h, v)) if !h.is_empty() && h.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') => {
            (Some(h), v)
        }
        _ => (None, a),
    }
}

fn unhinted(a: &str) -> &str {
    hint(a).1
}

fn stop_position(v: &str) -> Option<f32> {
    if let Some(p) = v.strip_suffix('%') {
        return num(p).map(|p| p / 100.0);
    }
    let a = arbitrary(v)?;
    a.strip_suffix('%').and_then(num).map(|p| p / 100.0)
}

fn size(theme: &Theme, v: &str, key: StyleKey, width: bool) -> Option<Vec<Decl>> {
    // Window-relative: `w-screen`, `h-dvh`, `min-h-svh`...
    let screen = match v {
        "screen" | "dvw" | "svw" | "lvw" if width => Some((1.0, 0.0)),
        "screen" | "dvh" | "svh" | "lvh" if !width => Some((0.0, 1.0)),
        _ => None,
    };
    if let Some((fw, fh)) = screen {
        return one(Decl::Screen(key, fw, fh));
    }
    match v {
        "min" | "max" | "fit" => {
            return one(match key {
                K::MinWidth | K::MinHeight => setn(key, 0.0),
                K::MaxWidth | K::MaxHeight => Decl::Set(key, PatchValue::Null),
                _ => sets(key, "auto"),
            });
        }
        "none" if matches!(key, K::MaxWidth | K::MaxHeight) => {
            return one(Decl::Set(key, PatchValue::Null));
        }
        _ => {}
    }
    if matches!(key, K::MaxWidth | K::Width | K::MinWidth)
        && let Some(w) = theme.containers.get(v)
    {
        return one(setn(key, *w));
    }
    let auto = matches!(key, K::Width | K::Height);
    one(set(key, length_value(theme, v, false, auto)?))
}

fn radius(theme: &Theme, v: &str, keys: &[StyleKey]) -> Option<Vec<Decl>> {
    let r = match v {
        "none" => 0.0,
        "full" => 9999.0,
        v => match theme.radius.get(v) {
            Some(r) => *r,
            None => css::length(&arbitrary(v)?)?,
        },
    };
    Some(keys.iter().map(|k| setn(*k, r)).collect())
}

fn border(theme: &Theme, v: &str, widths: &[StyleKey], colors: &[StyleKey]) -> Option<Vec<Decl>> {
    if let Some(n) = num(v) {
        return Some(widths.iter().map(|k| setn(*k, n)).collect());
    }
    if let Some(a) = arbitrary(v)
        && let Some(w) = arbitrary_hinted(&a, "length")
            .and_then(css::length)
            .or_else(|| css::length(&a))
    {
        return Some(widths.iter().map(|k| setn(*k, w)).collect());
    }
    let c = color(theme, v)?;
    Some(colors.iter().map(|k| set(*k, color_str(c))).collect())
}

fn blur(theme: &Theme, v: &str, backdrop: bool) -> Option<Vec<Decl>> {
    let r = match v {
        "none" => 0.0,
        v => match theme.blur.get(v) {
            Some(r) => *r,
            None => css::length(&arbitrary(v)?)?,
        },
    };
    one(filter(backdrop, 0, Filter::Blur(px(r))))
}

fn translate(theme: &Theme, v: &str, neg: bool) -> Option<Len> {
    let sign = if neg { -1.0 } else { 1.0 };
    if v == "full" {
        return Some(Len::Fraction(sign));
    }
    if v.contains('/') && !v.starts_with('[') {
        return Some(Len::Fraction(sign * fraction(v)?));
    }
    match length_value(theme, v, neg, false)? {
        PatchValue::Number(n) => Some(Len::Points(n)),
        PatchValue::Str(s) => {
            let p = css::number(s.strip_suffix('%')?)?;
            Some(Len::Fraction(p / 100.0))
        }
        PatchValue::Null => None,
    }
}

/// `text-*`: sizes (with an optional line height, `text-sm/6`), colors,
/// arbitrary sizes or colors.
fn text(theme: &Theme, v: &str, _neg: bool) -> Option<Vec<Decl>> {
    let (base, line) = match v.split_once('/') {
        Some((b, l)) if theme.text.contains_key(b) || b.starts_with('[') && !b.contains('#') => {
            (b, Some(l))
        }
        _ => (v, None),
    };
    let line_height = || -> Option<Decl> {
        let l = line?;
        Some(match theme.leading.get(l) {
            Some(m) => sets(K::LineHeight, &m.to_string()),
            None => match length_value(theme, l, false, false)? {
                PatchValue::Number(n) => setn(K::LineHeight, n),
                _ => return None,
            },
        })
    };
    if let Some(&(size, multiple)) = theme.text.get(base) {
        let mut out = vec![setn(K::FontSize, size)];
        match line {
            Some(_) => out.push(line_height()?),
            None => {
                if let Some(m) = multiple {
                    out.push(sets(K::LineHeight, &m.to_string()));
                }
            }
        }
        return Some(out);
    }
    if base.starts_with('[') {
        let a = arbitrary(base)?;
        if let Some(l) = arbitrary_hinted(&a, "length").and_then(css::length) {
            return one(setn(K::FontSize, l));
        }
        if let Some(c) = arbitrary_hinted(&a, "color").and_then(css::color) {
            return one(set(K::Color, color_str(c)));
        }
        if let Some(l) = css::length(&a) {
            let mut out = vec![setn(K::FontSize, l)];
            if line.is_some() {
                out.push(line_height()?);
            }
            return Some(out);
        }
    }
    one(set(K::Color, color_str(color(theme, v)?)))
}

/// Tailwind's timing of `animate-*`.
fn preset(keyframes: Keyframes) -> KeyframeAnimation {
    match keyframes {
        Keyframes::Spin => KeyframeAnimation {
            keyframes,
            duration: 1000.0,
            delay: 0.0,
            easing: [0.0, 0.0, 1.0, 1.0],
            iterations: None,
        },
        Keyframes::Ping => KeyframeAnimation {
            keyframes,
            duration: 1000.0,
            delay: 0.0,
            easing: [0.0, 0.0, 0.2, 1.0],
            iterations: None,
        },
        Keyframes::Pulse => KeyframeAnimation {
            keyframes,
            duration: 2000.0,
            delay: 0.0,
            easing: [0.4, 0.0, 0.6, 1.0],
            iterations: None,
        },
        Keyframes::Bounce => KeyframeAnimation {
            keyframes,
            duration: 1000.0,
            delay: 0.0,
            easing: [0.8, 0.0, 1.0, 1.0],
            iterations: None,
        },
    }
}
