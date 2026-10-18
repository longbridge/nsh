//! Visual effects beyond React Native's classic view styles: layered and
//! inset box shadows, gradient backgrounds, filters, blend modes, outlines,
//! cursors, and the transitions / keyframe animations of class styles.

use std::sync::Arc;

use crate::style::{BorderStyle, Color};

/// One layer of `boxShadow` (CSS `box-shadow`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxShadow {
    /// Horizontal offset in points.
    pub offset_x: f32,
    /// Vertical offset in points.
    pub offset_y: f32,
    /// Blur radius in points (CSS: twice the Gaussian's standard deviation).
    pub blur: f32,
    /// How far the shadow's shape grows (negative: shrinks).
    pub spread: f32,
    /// Color.
    pub color: Color,
    /// Drawn inside the box, over the background.
    pub inset: bool,
}

impl BoxShadow {
    /// True if the layer draws anything.
    pub fn is_visible(&self) -> bool {
        !self.color.is_transparent()
    }
}

/// How far a radial gradient reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientSize {
    /// To the nearest side.
    ClosestSide,
    /// To the nearest corner.
    ClosestCorner,
    /// To the farthest side.
    FarthestSide,
    /// To the farthest corner (CSS default).
    FarthestCorner,
}

/// The geometry of a gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientKind {
    /// Along a line at `angle` degrees (CSS: 0 points up, 90 right).
    Linear {
        /// Direction in degrees.
        angle: f32,
    },
    /// Toward a corner (`to top right`): the angle follows the box's
    /// shape (see [`GradientKind::linear_angle`]).
    LinearCorner {
        /// The right corners (else the left ones).
        right: bool,
        /// The bottom corners (else the top ones).
        bottom: bool,
    },
    /// Out from a center.
    Radial {
        /// A circle (else an ellipse matching the box).
        circle: bool,
        /// Ending shape size.
        size: GradientSize,
        /// Center, as fractions of the width.
        x: f32,
        /// Center, as fractions of the height.
        y: f32,
    },
    /// Around a center, starting at `from` degrees.
    Conic {
        /// Start angle in degrees (0 is up, clockwise).
        from: f32,
        /// Center, as fractions of the width.
        x: f32,
        /// Center, as fractions of the height.
        y: f32,
    },
}

impl GradientKind {
    /// The CSS angle (degrees) of a linear gradient over a `w` × `h` box.
    pub fn linear_angle(&self, w: f32, h: f32) -> Option<f32> {
        match *self {
            GradientKind::Linear { angle } => Some(angle),
            GradientKind::LinearCorner { right, bottom } => {
                // Perpendicular to the diagonal between the other two
                // corners (CSS Images 3).
                let a = (h.max(f32::EPSILON) / w.max(f32::EPSILON))
                    .atan()
                    .to_degrees();
                Some(match (right, bottom) {
                    (true, false) => 90.0 - a,
                    (true, true) => 90.0 + a,
                    (false, true) => 270.0 - a,
                    (false, false) => 270.0 + a,
                })
            }
            _ => None,
        }
    }
}

/// A color at a point of a gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorStop {
    /// Color.
    pub color: Color,
    /// Position along the gradient as a fraction; `None` is spread evenly
    /// between its neighbors.
    pub position: Option<f32>,
}

/// A gradient image (`backgroundImage`).
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// Geometry.
    pub kind: GradientKind,
    /// At least two stops.
    pub stops: Vec<ColorStop>,
    /// Repeats past the last stop.
    pub repeating: bool,
}

impl Gradient {
    /// Stop positions resolved as CSS does: missing ones spread evenly,
    /// each at least the one before it.
    pub fn resolved_stops(&self) -> Vec<(Color, f32)> {
        let n = self.stops.len();
        let mut pos: Vec<Option<f32>> = self.stops.iter().map(|s| s.position).collect();
        if pos[0].is_none() {
            pos[0] = Some(0.0);
        }
        if pos[n - 1].is_none() {
            pos[n - 1] = Some(1.0);
        }
        let mut max = 0.0f32;
        for p in pos.iter_mut().flatten() {
            max = max.max(*p);
            *p = max;
        }
        let mut i = 0;
        while i < n {
            if pos[i].is_some() {
                i += 1;
                continue;
            }
            let start = i - 1;
            let mut end = i;
            while pos[end].is_none() {
                end += 1;
            }
            let (a, b) = (pos[start].unwrap_or(0.0), pos[end].unwrap_or(1.0));
            for (k, p) in pos.iter_mut().enumerate().take(end).skip(i) {
                *p = Some(a + (b - a) * (k - start) as f32 / (end - start) as f32);
            }
            i = end;
        }
        self.stops
            .iter()
            .zip(pos)
            .map(|(s, p)| (s.color, p.unwrap_or(0.0)))
            .collect()
    }
}

/// One function of `filter` / `backdropFilter`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    /// Gaussian blur radius in points.
    Blur(f32),
    /// 1 is unchanged.
    Brightness(f32),
    /// 1 is unchanged.
    Contrast(f32),
    /// 0..1.
    Grayscale(f32),
    /// Degrees.
    HueRotate(f32),
    /// 0..1.
    Invert(f32),
    /// 0..1.
    Opacity(f32),
    /// 1 is unchanged.
    Saturate(f32),
    /// 0..1.
    Sepia(f32),
    /// A shadow of the content's shape.
    DropShadow {
        /// Horizontal offset in points.
        offset_x: f32,
        /// Vertical offset in points.
        offset_y: f32,
        /// Blur radius in points.
        blur: f32,
        /// Color.
        color: Color,
    },
}

impl Filter {
    /// The 4×5 color matrix (row major, sRGB, offsets in 0..1) of a color
    /// filter; `None` for blur and drop shadows.
    pub fn color_matrix(&self) -> Option<[f32; 20]> {
        let id = |r: [f32; 3], g: [f32; 3], b: [f32; 3]| {
            [
                r[0], r[1], r[2], 0.0, 0.0, g[0], g[1], g[2], 0.0, 0.0, b[0], b[1], b[2], 0.0, 0.0,
                0.0, 0.0, 0.0, 1.0, 0.0,
            ]
        };
        Some(match *self {
            Filter::Brightness(v) => id([v, 0.0, 0.0], [0.0, v, 0.0], [0.0, 0.0, v]),
            Filter::Contrast(v) => {
                let t = (1.0 - v) / 2.0;
                [
                    v, 0.0, 0.0, 0.0, t, 0.0, v, 0.0, 0.0, t, 0.0, 0.0, v, 0.0, t, 0.0, 0.0, 0.0,
                    1.0, 0.0,
                ]
            }
            Filter::Grayscale(v) => {
                let a = 1.0 - v;
                id(
                    [
                        0.2126 + 0.7874 * a,
                        0.7152 - 0.7152 * a,
                        0.0722 - 0.0722 * a,
                    ],
                    [
                        0.2126 - 0.2126 * a,
                        0.7152 + 0.2848 * a,
                        0.0722 - 0.0722 * a,
                    ],
                    [
                        0.2126 - 0.2126 * a,
                        0.7152 - 0.7152 * a,
                        0.0722 + 0.9278 * a,
                    ],
                )
            }
            Filter::Sepia(v) => {
                let a = 1.0 - v;
                id(
                    [0.393 + 0.607 * a, 0.769 - 0.769 * a, 0.189 - 0.189 * a],
                    [0.349 - 0.349 * a, 0.686 + 0.314 * a, 0.168 - 0.168 * a],
                    [0.272 - 0.272 * a, 0.534 - 0.534 * a, 0.131 + 0.869 * a],
                )
            }
            Filter::Saturate(s) => id(
                [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
                [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
                [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
            ),
            Filter::HueRotate(deg) => {
                let (s, c) = deg.to_radians().sin_cos();
                id(
                    [
                        0.213 + c * 0.787 - s * 0.213,
                        0.715 - c * 0.715 - s * 0.715,
                        0.072 - c * 0.072 + s * 0.928,
                    ],
                    [
                        0.213 - c * 0.213 + s * 0.143,
                        0.715 + c * 0.285 + s * 0.140,
                        0.072 - c * 0.072 - s * 0.283,
                    ],
                    [
                        0.213 - c * 0.213 - s * 0.787,
                        0.715 - c * 0.715 + s * 0.715,
                        0.072 + c * 0.928 + s * 0.072,
                    ],
                )
            }
            Filter::Invert(v) => {
                let m = 1.0 - 2.0 * v;
                [
                    m, 0.0, 0.0, 0.0, v, 0.0, m, 0.0, 0.0, v, 0.0, 0.0, m, 0.0, v, 0.0, 0.0, 0.0,
                    1.0, 0.0,
                ]
            }
            Filter::Opacity(v) => [
                1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
                0.0, 0.0, v, 0.0,
            ],
            Filter::Blur(_) | Filter::DropShadow { .. } => return None,
        })
    }
}

/// The color matrices of `filters` composed in order (the first applies
/// first), or `None` when none changes colors.
pub fn compose_color_matrices(filters: &[Filter]) -> Option<[f32; 20]> {
    let mut out: Option<[f32; 20]> = None;
    for m in filters.iter().filter_map(Filter::color_matrix) {
        out = Some(match out {
            None => m,
            Some(prev) => {
                // `m · prev`, with the offset column treated as a 5th
                // homogeneous coordinate.
                let mut r = [0.0; 20];
                for row in 0..4 {
                    for col in 0..5 {
                        let mut v = if col == 4 { m[row * 5 + 4] } else { 0.0 };
                        for k in 0..4 {
                            v += m[row * 5 + k] * prev[k * 5 + col];
                        }
                        r[row * 5 + col] = v;
                    }
                }
                r
            }
        });
    }
    out
}

/// The total blur radius of `filters`.
pub fn blur_radius(filters: &[Filter]) -> f32 {
    filters
        .iter()
        .map(|f| match f {
            Filter::Blur(r) => *r,
            _ => 0.0,
        })
        .sum()
}

/// `mixBlendMode`: how the view composites with what is behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(missing_docs)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    PlusDarker,
    PlusLighter,
}

impl BlendMode {
    /// The CSS name ([`BlendMode::parse`]'s input).
    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "normal",
            BlendMode::Multiply => "multiply",
            BlendMode::Screen => "screen",
            BlendMode::Overlay => "overlay",
            BlendMode::Darken => "darken",
            BlendMode::Lighten => "lighten",
            BlendMode::ColorDodge => "color-dodge",
            BlendMode::ColorBurn => "color-burn",
            BlendMode::HardLight => "hard-light",
            BlendMode::SoftLight => "soft-light",
            BlendMode::Difference => "difference",
            BlendMode::Exclusion => "exclusion",
            BlendMode::Hue => "hue",
            BlendMode::Saturation => "saturation",
            BlendMode::Color => "color",
            BlendMode::Luminosity => "luminosity",
            BlendMode::PlusDarker => "plus-darker",
            BlendMode::PlusLighter => "plus-lighter",
        }
    }

    /// Parses a CSS blend mode name.
    pub fn parse(s: &str) -> Option<BlendMode> {
        Some(match s {
            "normal" => BlendMode::Normal,
            "multiply" => BlendMode::Multiply,
            "screen" => BlendMode::Screen,
            "overlay" => BlendMode::Overlay,
            "darken" => BlendMode::Darken,
            "lighten" => BlendMode::Lighten,
            "color-dodge" => BlendMode::ColorDodge,
            "color-burn" => BlendMode::ColorBurn,
            "hard-light" => BlendMode::HardLight,
            "soft-light" => BlendMode::SoftLight,
            "difference" => BlendMode::Difference,
            "exclusion" => BlendMode::Exclusion,
            "hue" => BlendMode::Hue,
            "saturation" => BlendMode::Saturation,
            "color" => BlendMode::Color,
            "luminosity" => BlendMode::Luminosity,
            "plus-darker" => BlendMode::PlusDarker,
            "plus-lighter" => BlendMode::PlusLighter,
            _ => return None,
        })
    }
}

/// `outline*`: a line around the border box, outside the layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    /// Width in points (0: none).
    pub width: f32,
    /// Color.
    pub color: Color,
    /// Line style.
    pub style: BorderStyle,
    /// Gap between the border box and the outline (negative: inside).
    pub offset: f32,
}

impl Outline {
    /// No outline.
    pub const NONE: Outline = Outline {
        width: 0.0,
        color: Color::BLACK,
        style: BorderStyle::Solid,
        offset: 0.0,
    };

    /// True if the outline draws anything.
    pub fn is_visible(&self) -> bool {
        self.width > 0.0 && !self.color.is_transparent()
    }
}

impl Default for Outline {
    fn default() -> Self {
        Self::NONE
    }
}

/// The pointer's look over a view (`cursor`; iPad, Mac Catalyst and
/// Android with a mouse).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(missing_docs)]
pub enum Cursor {
    #[default]
    Auto,
    Default,
    Pointer,
    Text,
    Wait,
    Progress,
    Help,
    Crosshair,
    Move,
    Grab,
    Grabbing,
    NotAllowed,
    ZoomIn,
    ZoomOut,
    ColResize,
    RowResize,
    None,
}

impl Cursor {
    /// Its CSS name ([`Cursor::parse`]'s input).
    pub fn name(self) -> &'static str {
        match self {
            Cursor::Auto => "auto",
            Cursor::Default => "default",
            Cursor::Pointer => "pointer",
            Cursor::Text => "text",
            Cursor::Wait => "wait",
            Cursor::Progress => "progress",
            Cursor::Help => "help",
            Cursor::Crosshair => "crosshair",
            Cursor::Move => "move",
            Cursor::Grab => "grab",
            Cursor::Grabbing => "grabbing",
            Cursor::NotAllowed => "not-allowed",
            Cursor::ZoomIn => "zoom-in",
            Cursor::ZoomOut => "zoom-out",
            Cursor::ColResize => "col-resize",
            Cursor::RowResize => "row-resize",
            Cursor::None => "none",
        }
    }

    /// Parses a CSS cursor name (unknown resize directions map to their
    /// axis).
    pub fn parse(s: &str) -> Option<Cursor> {
        Some(match s {
            "auto" => Cursor::Auto,
            "default" | "context-menu" | "alias" | "copy" | "cell" => Cursor::Default,
            "pointer" => Cursor::Pointer,
            "text" | "vertical-text" => Cursor::Text,
            "wait" => Cursor::Wait,
            "progress" => Cursor::Progress,
            "help" => Cursor::Help,
            "crosshair" => Cursor::Crosshair,
            "move" | "all-scroll" => Cursor::Move,
            "grab" => Cursor::Grab,
            "grabbing" => Cursor::Grabbing,
            "not-allowed" | "no-drop" => Cursor::NotAllowed,
            "zoom-in" => Cursor::ZoomIn,
            "zoom-out" => Cursor::ZoomOut,
            "col-resize" | "e-resize" | "w-resize" | "ew-resize" => Cursor::ColResize,
            "row-resize" | "n-resize" | "s-resize" | "ns-resize" => Cursor::RowResize,
            "ne-resize" | "nw-resize" | "se-resize" | "sw-resize" | "nesw-resize"
            | "nwse-resize" => Cursor::Move,
            "none" => Cursor::None,
            _ => return None,
        })
    }
}

/// Which property groups a [`Transition`] animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransitionProps(pub u8);

impl TransitionProps {
    /// Background, border, outline, shadow and text colors.
    pub const COLORS: u8 = 1;
    /// `opacity`.
    pub const OPACITY: u8 = 2;
    /// `transform`.
    pub const TRANSFORM: u8 = 4;
    /// Box shadows.
    pub const SHADOW: u8 = 8;
    /// Corner radii and border widths.
    pub const SHAPE: u8 = 16;
    /// Everything that can transition.
    pub const ALL: u8 = 0x1f;

    /// True if `group` is included.
    #[inline]
    pub fn has(self, group: u8) -> bool {
        self.0 & group != 0
    }
}

/// `transition*`: style changes animate natively instead of jumping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    /// Properties that animate.
    pub properties: TransitionProps,
    /// Milliseconds.
    pub duration: f32,
    /// Milliseconds before it starts.
    pub delay: f32,
    /// Cubic Bézier `x1, y1, x2, y2`.
    pub easing: [f32; 4],
}

impl Transition {
    /// CSS's default curve (`ease`).
    pub const EASE: [f32; 4] = [0.25, 0.1, 0.25, 1.0];
}

/// A built-in keyframe animation (Tailwind's `animate-*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyframes {
    /// One turn clockwise.
    Spin,
    /// Scales up to 2× and fades out.
    Ping,
    /// Fades to half opacity and back.
    Pulse,
    /// Jumps up by a quarter of its height and lands.
    Bounce,
}

/// `animation*`: a keyframe animation running natively, forever by default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyframeAnimation {
    /// What runs.
    pub keyframes: Keyframes,
    /// Milliseconds per iteration.
    pub duration: f32,
    /// Milliseconds before it starts.
    pub delay: f32,
    /// Cubic Bézier `x1, y1, x2, y2` (Bounce uses its own per half).
    pub easing: [f32; 4],
    /// `None` repeats forever.
    pub iterations: Option<u32>,
}

impl KeyframeAnimation {
    /// `keyframes` as TS's `animationName` alone runs them: a second per
    /// iteration at a constant pace, forever.
    pub const fn new(keyframes: Keyframes) -> Self {
        Self {
            keyframes,
            duration: 1000.0,
            delay: 0.0,
            easing: [0.0, 0.0, 1.0, 1.0],
            iterations: None,
        }
    }

    /// `ms` milliseconds per iteration.
    pub const fn duration(mut self, ms: f32) -> Self {
        self.duration = ms;
        self
    }

    /// Starts after `ms` milliseconds.
    pub const fn delay(mut self, ms: f32) -> Self {
        self.delay = ms;
        self
    }

    /// Along the cubic Bézier curve `x1, y1, x2, y2`.
    pub const fn easing(mut self, curve: [f32; 4]) -> Self {
        self.easing = curve;
        self
    }

    /// `count` iterations, then it stops.
    pub const fn iterations(mut self, count: u32) -> Self {
        self.iterations = Some(count);
        self
    }
}

/// The list-valued effects of a view, shared between equal styles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effects {
    /// Box shadows, top layer first (CSS order).
    pub box_shadows: Vec<BoxShadow>,
    /// Gradient backgrounds, top layer first, over `backgroundColor`.
    pub background_images: Vec<Gradient>,
    /// Filters applied to the view and its content, in order.
    pub filters: Vec<Filter>,
    /// Filters applied to what is behind the view (blur only on iOS).
    pub backdrop_filters: Vec<Filter>,
}

impl Effects {
    /// True if nothing is set.
    pub fn is_empty(&self) -> bool {
        self.box_shadows.is_empty()
            && self.background_images.is_empty()
            && self.filters.is_empty()
            && self.backdrop_filters.is_empty()
    }
}

/// [`Effects`] behind a cheap shared pointer (`None` when empty).
pub(crate) fn shared(effects: Effects) -> Option<Arc<Effects>> {
    (!effects.is_empty()).then(|| Arc::new(effects))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_spread_evenly() {
        let c = Color::BLACK;
        let g = Gradient {
            kind: GradientKind::Linear { angle: 90.0 },
            stops: vec![
                ColorStop {
                    color: c,
                    position: None,
                },
                ColorStop {
                    color: c,
                    position: None,
                },
                ColorStop {
                    color: c,
                    position: Some(0.8),
                },
                ColorStop {
                    color: c,
                    position: Some(0.5),
                },
            ],
            repeating: false,
        };
        let p: Vec<f32> = g.resolved_stops().into_iter().map(|(_, p)| p).collect();
        assert_eq!(p, [0.0, 0.4, 0.8, 0.8]);
    }

    #[test]
    fn matrices_compose_in_order() {
        let m = compose_color_matrices(&[Filter::Invert(1.0), Filter::Brightness(0.5)]).unwrap();
        // White inverted is black; halved stays black. Black: 1 * 0.5.
        let apply = |c: f32| m[0] * c + m[4];
        assert!((apply(1.0) - 0.0).abs() < 1e-6);
        assert!((apply(0.0) - 0.5).abs() < 1e-6);
        assert!(compose_color_matrices(&[Filter::Blur(2.0)]).is_none());
    }

    #[test]
    fn corner_angles() {
        let k = GradientKind::LinearCorner {
            right: true,
            bottom: false,
        };
        assert!((k.linear_angle(100.0, 100.0).unwrap() - 45.0).abs() < 1e-4);
        let k = GradientKind::LinearCorner {
            right: true,
            bottom: true,
        };
        assert!((k.linear_angle(100.0, 100.0).unwrap() - 135.0).abs() < 1e-4);
    }
}
