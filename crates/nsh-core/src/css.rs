//! CSS value syntax shared by style strings and Tailwind classes: colors
//! (hex, `rgb()`, `hsl()`, `oklch()`, `oklab()`, named, and `color-mix()`
//! of any of them in `srgb`, `srgb-linear`, `xyz`, `oklab`, `oklch` or
//! `hsl`, as CSS Color 5 mixes), lengths, angles,
//! and the lists of `box-shadow`, `filter` and gradients. Lengths are
//! points: `px` and bare numbers are points, `rem` is 16 points.

use crate::style::Color;

/// Points per `rem` in style strings.
pub(crate) const REM: f32 = 16.0;

/// Splits `s` at `sep` outside parentheses and brackets.
pub(crate) fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim());
    parts
}

/// The whitespace-separated words of `s`, keeping functions (`rgb(0 0 0)`)
/// whole.
pub(crate) fn words(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start: Option<usize> = None;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => {
                depth += 1;
                start.get_or_insert(i);
            }
            ')' | ']' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => {
                if let Some(st) = start.take() {
                    out.push(&s[st..i]);
                }
            }
            _ => {
                start.get_or_insert(i);
            }
        }
    }
    if let Some(st) = start {
        out.push(&s[st..]);
    }
    out
}

/// A number, with an optional `%` (as a fraction when `percent` is set).
pub(crate) fn number(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok().filter(|n| n.is_finite())
}

/// A length in points: `4`, `4px`, `0.5rem`, `1pt`.
pub(crate) fn length(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("px").or_else(|| s.strip_suffix("pt")) {
        return number(n);
    }
    if let Some(n) = s.strip_suffix("rem") {
        return number(n).map(|n| n * REM);
    }
    number(s)
}

/// A length (in points) or a percentage (a fraction).
pub(crate) fn length_or_percent(s: &str) -> Option<DefiniteLength> {
    match s.trim().strip_suffix('%') {
        Some(p) => number(p).map(|p| percentage(p / 100.0).into()),
        None => length(s).map(|l| px(l).into()),
    }
}

/// An angle in degrees: `45deg`, `0.5turn`, `1rad`, `100grad`, `0`.
pub(crate) fn angle(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("deg") {
        return number(n);
    }
    if let Some(n) = s.strip_suffix("grad") {
        return number(n).map(|n| n * 0.9);
    }
    if let Some(n) = s.strip_suffix("rad") {
        return number(n).map(f32::to_degrees);
    }
    if let Some(n) = s.strip_suffix("turn") {
        return number(n).map(|n| n * 360.0);
    }
    number(s).filter(|&n| n == 0.0)
}

/// A number or a percentage, as a factor: `1.2`, `120%`.
pub(crate) fn amount(s: &str) -> Option<f32> {
    match s.trim().strip_suffix('%') {
        Some(p) => number(p).map(|p| p / 100.0),
        None => number(s),
    }
}

// --- Colors ---------------------------------------------------------------

/// Parses a CSS color.
pub(crate) fn color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex.as_bytes());
    }
    if let Some(open) = s.find('(') {
        let name = s[..open].trim().to_ascii_lowercase();
        let args = s[open + 1..].strip_suffix(')')?;
        return match name.as_str() {
            "rgb" | "rgba" => rgb(args),
            "hsl" | "hsla" => hsl(args),
            "oklch" => oklab(args, true),
            "oklab" => oklab(args, false),
            "color-mix" => color_mix(args),
            _ => None,
        };
    }
    let lower;
    let name = if s.bytes().any(|b| b.is_ascii_uppercase()) {
        lower = s.to_ascii_lowercase();
        lower.as_str()
    } else {
        s
    };
    if name == "transparent" {
        return Some(Color::TRANSPARENT);
    }
    crate::css_named::NAMED
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| Color(crate::css_named::NAMED[i].1))
}

#[inline]
fn hex_digit(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as u32),
        b'a'..=b'f' => Some((b - b'a' + 10) as u32),
        b'A'..=b'F' => Some((b - b'A' + 10) as u32),
        _ => None,
    }
}

fn parse_hex(hex: &[u8]) -> Option<Color> {
    let mut v = 0u32;
    for &b in hex {
        v = v << 4 | hex_digit(b)?;
    }
    Some(match hex.len() {
        3 | 4 => {
            // Expand each nibble: 0xabc -> 0xaabbcc.
            let (rgb, a) = if hex.len() == 3 {
                (v, 0xf)
            } else {
                (v >> 4, v & 0xf)
            };
            let r = (rgb >> 8) & 0xf;
            let g = (rgb >> 4) & 0xf;
            let b = rgb & 0xf;
            Color((r * 0x11) << 24 | (g * 0x11) << 16 | (b * 0x11) << 8 | (a * 0x11))
        }
        6 => Color(v << 8 | 0xff),
        8 => Color(v),
        _ => return None,
    })
}

/// The channels of a color function, either legacy (`1, 2, 3, 0.5`) or
/// modern (`1 2 3 / 50%`), and the alpha (1 when absent).
fn channels(args: &str) -> Option<([&str; 3], f32)> {
    let (main, alpha) = match args.split_once('/') {
        Some((m, a)) => (m, Some(a.trim())),
        None => (args, None),
    };
    let parts: Vec<&str> = if main.contains(',') {
        main.split(',').map(str::trim).collect()
    } else {
        main.split_whitespace().collect()
    };
    let (c, alpha) = match (parts.as_slice(), alpha) {
        ([a, b, c], alpha) => ([*a, *b, *c], alpha),
        ([a, b, c, d], None) => ([*a, *b, *c], Some(*d)),
        _ => return None,
    };
    let alpha = match alpha {
        Some(a) => amount(a)?.clamp(0.0, 1.0),
        None => 1.0,
    };
    Some((c, alpha))
}

fn rgb(args: &str) -> Option<Color> {
    let (c, a) = channels(args)?;
    let channel = |s: &str| -> Option<f32> {
        match s.strip_suffix('%') {
            Some(p) => number(p).map(|p| p / 100.0),
            None => number(s).map(|n| n / 255.0),
        }
    };
    Some(from_unit(channel(c[0])?, channel(c[1])?, channel(c[2])?, a))
}

fn hsl(args: &str) -> Option<Color> {
    let (c, a) = channels(args)?;
    let h = angle(c[0]).or_else(|| number(c[0]))?.rem_euclid(360.0);
    let pct = |s: &str| -> Option<f32> {
        number(s.strip_suffix('%').unwrap_or(s)).map(|v| (v / 100.0).clamp(0.0, 1.0))
    };
    let (s, l) = (pct(c[1])?, pct(c[2])?);
    let (r, g, b) = crate::color::hsl_to_rgb(h, s, l);
    Some(from_unit(r, g, b, a))
}

/// `oklch(L C H)` (`polar`) or `oklab(L a b)`, to sRGB with clamped
/// channels.
fn oklab(args: &str, polar: bool) -> Option<Color> {
    let (c, alpha) = channels(args)?;
    let l = match c[0].strip_suffix('%') {
        Some(p) => number(p)? / 100.0,
        None => number(c[0])?,
    };
    let (a, b) = if polar {
        let chroma = match c[1].strip_suffix('%') {
            Some(p) => number(p)? / 100.0 * 0.4,
            None => number(c[1])?,
        };
        let h = angle(c[2])
            .or_else(|| number(c[2]))
            .unwrap_or(0.0)
            .to_radians();
        (chroma * h.cos(), chroma * h.sin())
    } else {
        let axis = |s: &str| -> Option<f32> {
            match s.strip_suffix('%') {
                Some(p) => number(p).map(|p| p / 100.0 * 0.4),
                None => number(s),
            }
        };
        (axis(c[1])?, axis(c[2])?)
    };
    Some(oklab_to_color(l, a, b, alpha))
}

/// OKLab to sRGB (Björn Ottosson's matrices).
pub(crate) fn oklab_to_color(l: f32, a: f32, b: f32, alpha: f32) -> Color {
    let (r, g, b) = crate::color::oklab_to_rgb(l, a, b);
    from_unit(r, g, b, alpha)
}

fn from_unit(r: f32, g: f32, b: f32, a: f32) -> Color {
    use crate::color::byte;
    Color::rgba(byte(r), byte(g), byte(b), byte(a))
}

/// The color space `color-mix()` interpolates in.
#[derive(Clone, Copy, PartialEq)]
enum MixSpace {
    Srgb,
    /// Also `xyz`, `xyz-d65` and `xyz-d50`: they are linear transforms of
    /// linear sRGB, and a linear transform commutes with the (premultiplied)
    /// interpolation, so mixing in them gives the same color.
    SrgbLinear,
    Oklab,
    Oklch,
    Hsl,
}

/// How `color-mix()` interpolates a hue (CSS Color 4 "hue-interpolation").
#[derive(Clone, Copy)]
enum HueMethod {
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

/// A chroma (OKLCh) or saturation (HSL) below which the hue is powerless:
/// colors arrive packed to 8 bits, so a grey's chroma is float noise, not 0.
const ACHROMATIC: f32 = 1e-4;

/// `color-mix(in <space> [<hue-method> hue]?, <color> <pct>?, <color>
/// <pct>?)`, as CSS Color 5 defines it: percentages normalized (both
/// omitted 50 / 50, one omitted the rest of 100%, a sum other than 100%
/// scaled to it and a sum below 100% kept as an alpha factor), channels
/// interpolated with premultiplied alpha, a powerless hue taking the other
/// color's.
fn color_mix(args: &str) -> Option<Color> {
    let [method, first, second] = split_top(args, ',')[..] else {
        return None;
    };
    let (space, hue) = mix_method(method)?;
    let (c1, p1) = mix_color(first)?;
    let (c2, p2) = mix_color(second)?;
    let (p1, p2) = match (p1, p2) {
        (None, None) => (0.5, 0.5),
        (Some(p1), None) => (p1, 1.0 - p1),
        (None, Some(p2)) => (1.0 - p2, p2),
        (Some(p1), Some(p2)) => (p1, p2),
    };
    let sum = p1 + p2;
    if sum <= 0.0 {
        return None;
    }
    let w1 = p1 / sum;
    let (mut v1, a1, missing1) = to_mix_space(c1, space);
    let (mut v2, a2, missing2) = to_mix_space(c2, space);
    let alpha = a1 * w1 + a2 * (1.0 - w1);
    let hue_index = match space {
        MixSpace::Oklch => Some(2),
        MixSpace::Hsl => Some(0),
        _ => None,
    };
    let mut out = [0.0; 3];
    for i in 0..3 {
        out[i] = if hue_index == Some(i) {
            // A missing hue takes the other color's, so mixing with a grey
            // keeps the hue instead of sweeping from red (hue 0).
            match (missing1, missing2) {
                (true, false) => v1[i] = v2[i],
                (false, true) => v2[i] = v1[i],
                _ => {}
            }
            let (h1, h2) = fix_hues(v1[i], v2[i], hue.unwrap_or(HueMethod::Shorter));
            (h1 * w1 + h2 * (1.0 - w1)).rem_euclid(360.0)
        } else if alpha > 0.0 {
            (v1[i] * a1 * w1 + v2[i] * a2 * (1.0 - w1)) / alpha
        } else {
            v1[i] * w1 + v2[i] * (1.0 - w1)
        };
    }
    // A sum below 100% leaves the rest transparent.
    let alpha = alpha * sum.min(1.0);
    Some(from_mix_space(out, alpha, space))
}

/// `in <space> [<hue-method> hue]?`; a hue method only for polar spaces.
fn mix_method(s: &str) -> Option<(MixSpace, Option<HueMethod>)> {
    let lower = s.to_ascii_lowercase();
    let w: Vec<&str> = lower.split_whitespace().collect();
    let (space, rest) = match w.as_slice() {
        ["in", space, rest @ ..] => (*space, rest),
        _ => return None,
    };
    let space = match space {
        "srgb" => MixSpace::Srgb,
        "srgb-linear" | "xyz" | "xyz-d65" | "xyz-d50" => MixSpace::SrgbLinear,
        "oklab" => MixSpace::Oklab,
        "oklch" => MixSpace::Oklch,
        "hsl" => MixSpace::Hsl,
        _ => return None,
    };
    let hue = match rest {
        [] => None,
        [method, "hue"] if matches!(space, MixSpace::Oklch | MixSpace::Hsl) => {
            Some(match *method {
                "shorter" => HueMethod::Shorter,
                "longer" => HueMethod::Longer,
                "increasing" => HueMethod::Increasing,
                "decreasing" => HueMethod::Decreasing,
                _ => return None,
            })
        }
        _ => return None,
    };
    Some((space, hue))
}

/// `<color> <percentage>?` in either order; the percentage as a fraction,
/// only within `0%..=100%`.
fn mix_color(s: &str) -> Option<(Color, Option<f32>)> {
    let percent = |w: &str| -> Option<f32> {
        let p = number(w.strip_suffix('%')?)?;
        (0.0..=100.0).contains(&p).then_some(p / 100.0)
    };
    match words(s).as_slice() {
        [c] => Some((color(c)?, None)),
        [a, b] if a.ends_with('%') => Some((color(b)?, Some(percent(a)?))),
        [a, b] => Some((color(a)?, Some(percent(b)?))),
        _ => None,
    }
}

/// A color's coordinates in `space` (the hue in degrees), its alpha, and
/// whether its hue is powerless (missing, in CSS's terms).
fn to_mix_space(c: Color, space: MixSpace) -> ([f32; 3], f32, bool) {
    use crate::color::{Rgba, rgb_to_oklab, srgb_to_linear};
    let rgba = Rgba::from(c);
    let v = match space {
        MixSpace::Srgb => [rgba.r, rgba.g, rgba.b],
        MixSpace::SrgbLinear => [
            srgb_to_linear(rgba.r),
            srgb_to_linear(rgba.g),
            srgb_to_linear(rgba.b),
        ],
        MixSpace::Oklab | MixSpace::Oklch => {
            let (l, a, b) = rgb_to_oklab(rgba);
            if space == MixSpace::Oklab {
                [l, a, b]
            } else {
                let chroma = a.hypot(b);
                let hue = b.atan2(a).to_degrees().rem_euclid(360.0);
                return ([l, chroma, hue], rgba.a, chroma < ACHROMATIC);
            }
        }
        MixSpace::Hsl => {
            let hsla = rgba.to_hsla();
            return (
                [hsla.h * 360.0, hsla.s, hsla.l],
                rgba.a,
                hsla.s < ACHROMATIC,
            );
        }
    };
    (v, rgba.a, false)
}

/// The mixed coordinates back to a packed sRGB color, channels clamped.
fn from_mix_space(v: [f32; 3], alpha: f32, space: MixSpace) -> Color {
    use crate::color::linear_to_srgb;
    match space {
        MixSpace::Srgb => from_unit(v[0], v[1], v[2], alpha),
        MixSpace::SrgbLinear => from_unit(
            linear_to_srgb(v[0]),
            linear_to_srgb(v[1]),
            linear_to_srgb(v[2]),
            alpha,
        ),
        MixSpace::Oklab => oklab_to_color(v[0], v[1], v[2], alpha),
        MixSpace::Oklch => {
            let h = v[2].to_radians();
            oklab_to_color(v[0], v[1] * h.cos(), v[1] * h.sin(), alpha)
        }
        MixSpace::Hsl => {
            let (r, g, b) = crate::color::hsl_to_rgb(v[0], v[1], v[2]);
            from_unit(r, g, b, alpha)
        }
    }
}

/// Two hues (degrees in `0..360`) adjusted so a straight interpolation
/// between them goes the way `method` asks (CSS Color 4 "hue-interpolation").
fn fix_hues(mut h1: f32, mut h2: f32, method: HueMethod) -> (f32, f32) {
    let d = h2 - h1;
    match method {
        HueMethod::Shorter if d > 180.0 => h1 += 360.0,
        HueMethod::Shorter if d < -180.0 => h2 += 360.0,
        HueMethod::Longer if 0.0 < d && d < 180.0 => h1 += 360.0,
        HueMethod::Longer if -180.0 < d && d <= 0.0 => h2 += 360.0,
        HueMethod::Increasing if h2 < h1 => h2 += 360.0,
        HueMethod::Decreasing if h1 < h2 => h1 += 360.0,
        _ => {}
    }
    (h1, h2)
}

impl Color {
    /// The color with its alpha multiplied by `factor`.
    pub fn with_alpha(self, factor: f32) -> Color {
        let a = ((self.0 & 0xff) as f32 * factor.clamp(0.0, 1.0)).round() as u32;
        Color(self.0 & 0xffff_ff00 | a)
    }
}

// --- Shadows, gradients, filters --------------------------------------------

use crate::{
    effects::{BoxShadow, ColorStop, Filter, Gradient, GradientKind, GradientSize},
    geometry::{DefiniteLength, Percentage, deg, percentage, px},
    style::ObjectPosition,
};

/// A `box-shadow` list (`none` is empty).
pub(crate) fn box_shadows(s: &str) -> Option<Vec<BoxShadow>> {
    let s = s.trim();
    if s.is_empty() || s == "none" {
        return Some(Vec::new());
    }
    split_top(s, ',').into_iter().map(box_shadow).collect()
}

/// One shadow: `[inset] <x> <y> [<blur> [<spread>]] [<color>]`, words in
/// any order around the lengths.
pub(crate) fn box_shadow(s: &str) -> Option<BoxShadow> {
    let mut shadow = BoxShadow {
        color: Color(0x0000_00ff),
        ..BoxShadow::default()
    };
    let mut lengths = Vec::with_capacity(4);
    let mut color_seen = false;
    for word in words(s) {
        if word == "inset" {
            shadow.inset = true;
        } else if let Some(l) = length(word) {
            lengths.push(l);
        } else if !color_seen && let Some(c) = color(word) {
            shadow.color = c;
            color_seen = true;
        } else {
            return None;
        }
    }
    match lengths.as_slice() {
        [x, y] => (shadow.offset_x, shadow.offset_y) = (px(*x), px(*y)),
        [x, y, blur] => {
            (shadow.offset_x, shadow.offset_y, shadow.blur) = (px(*x), px(*y), px(blur.max(0.0)))
        }
        [x, y, blur, spread] => {
            (shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread) =
                (px(*x), px(*y), px(blur.max(0.0)), px(*spread))
        }
        _ => return None,
    }
    Some(shadow)
}

/// A `background-image` list of gradients (`none` is empty).
pub(crate) fn gradients(s: &str) -> Option<Vec<Gradient>> {
    let s = s.trim();
    if s.is_empty() || s == "none" {
        return Some(Vec::new());
    }
    split_top(s, ',').into_iter().map(gradient).collect()
}

/// `linear-gradient(...)`, `radial-gradient(...)`, `conic-gradient(...)`
/// (and their `repeating-` forms).
pub(crate) fn gradient(s: &str) -> Option<Gradient> {
    let s = s.trim();
    let open = s.find('(')?;
    let name = s[..open].trim();
    let args = s[open + 1..].strip_suffix(')')?;
    let (repeating, base) = match name.strip_prefix("repeating-") {
        Some(b) => (true, b),
        None => (false, name),
    };
    let parts = split_top(args, ',');
    let mut parts = parts.as_slice();
    let mut kind = match base {
        "linear-gradient" => GradientKind::Linear { angle: deg(180.0) },
        "radial-gradient" => GradientKind::Radial {
            circle: false,
            size: GradientSize::FarthestCorner,
            x: percentage(0.5),
            y: percentage(0.5),
        },
        "conic-gradient" => GradientKind::Conic {
            from: deg(0.0),
            x: percentage(0.5),
            y: percentage(0.5),
        },
        _ => return None,
    };
    // The first argument configures the gradient when it is not a stop.
    if let Some(first) = parts.first()
        && configure(&mut kind, first)
    {
        parts = &parts[1..];
    }
    let conic = matches!(kind, GradientKind::Conic { .. });
    let mut stops = Vec::with_capacity(parts.len());
    for part in parts {
        let w = words(part);
        let (c, positions) = w.split_first()?;
        let color = color(c)?;
        // A stop takes up to two positions (`red 0 10px`: a band of one
        // color).
        if positions.len() > 2 {
            return None;
        }
        if positions.is_empty() {
            stops.push(ColorStop {
                color,
                position: None,
            });
        }
        for p in positions {
            stops.push(ColorStop {
                color,
                position: Some(stop_position(p, conic)?),
            });
        }
    }
    if stops.is_empty() {
        return None;
    }
    if stops.len() == 1 {
        let only = stops[0];
        stops.push(only);
    }
    Some(Gradient {
        kind,
        stops,
        repeating,
    })
}

/// A stop's position: a percentage, or a length along a linear or radial
/// gradient's line (`10px`, `1rem`; resolved against the box when it is
/// drawn), or an angle around a conic one (`90deg`, `0.25turn`, as a
/// fraction of a turn).
fn stop_position(s: &str, conic: bool) -> Option<DefiniteLength> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        return number(p).map(|p| percentage(p / 100.0).into());
    }
    if conic {
        angle(s).map(|a| percentage(a / 360.0).into())
    } else {
        // A zero length is the line's start whatever its length, as
        // before lengths were taken.
        length(s).map(|l| {
            if l == 0.0 {
                percentage(0.0).into()
            } else {
                px(l).into()
            }
        })
    }
}

/// Reads a gradient's configuration argument into `kind`; false when the
/// argument is a color stop.
fn configure(kind: &mut GradientKind, arg: &str) -> bool {
    // An interpolation space (`in oklab`) changes how colors mix; sRGB is
    // what the platforms draw.
    let arg = match arg
        .find(" in ")
        .or_else(|| arg.starts_with("in ").then_some(0))
    {
        Some(0) => "",
        Some(i) => arg[..i].trim(),
        None => arg,
    };
    let w = words(arg);
    if w.is_empty() {
        return true;
    }
    match kind {
        GradientKind::Linear { .. } | GradientKind::LinearCorner { .. } => {
            if w[0] == "to" {
                let mut h = 0.0f32;
                let mut v = 0.0f32;
                for side in &w[1..] {
                    match *side {
                        "top" => v = -1.0,
                        "bottom" => v = 1.0,
                        "left" => h = -1.0,
                        "right" => h = 1.0,
                        _ => return false,
                    }
                }
                *kind = match (h as i8, v as i8) {
                    (0, -1) => GradientKind::Linear { angle: deg(0.0) },
                    (1, 0) => GradientKind::Linear { angle: deg(90.0) },
                    (0, 1) => GradientKind::Linear { angle: deg(180.0) },
                    (-1, 0) => GradientKind::Linear { angle: deg(270.0) },
                    (0, 0) => return false,
                    // A corner: the angle depends on the box's shape.
                    (h, v) => GradientKind::LinearCorner {
                        right: h > 0,
                        bottom: v > 0,
                    },
                };
                true
            } else if let Some(a) = angle(w[0]) {
                *kind = GradientKind::Linear { angle: deg(a) };
                true
            } else {
                false
            }
        }
        GradientKind::Radial { circle, size, x, y } => {
            let mut i = 0;
            let mut any = false;
            while i < w.len() {
                match w[i] {
                    "circle" => *circle = true,
                    "ellipse" => *circle = false,
                    "closest-side" => *size = GradientSize::ClosestSide,
                    "closest-corner" => *size = GradientSize::ClosestCorner,
                    "farthest-side" => *size = GradientSize::FarthestSide,
                    "farthest-corner" => *size = GradientSize::FarthestCorner,
                    "at" => {
                        let Some((px, py)) = position(&w[i + 1..]) else {
                            return false;
                        };
                        (*x, *y) = (Percentage(px), Percentage(py));
                        return true;
                    }
                    _ => return any,
                }
                any = true;
                i += 1;
            }
            any
        }
        GradientKind::Conic { from, x, y } => {
            let mut i = 0;
            let mut any = false;
            while i < w.len() {
                match w[i] {
                    "from" => {
                        let Some(a) = w.get(i + 1).and_then(|s| angle(s)) else {
                            return false;
                        };
                        *from = deg(a);
                        i += 2;
                    }
                    "at" => {
                        let Some((px, py)) = position(&w[i + 1..]) else {
                            return false;
                        };
                        (*x, *y) = (Percentage(px), Percentage(py));
                        return true;
                    }
                    _ => return any,
                }
                any = true;
            }
            any
        }
    }
}

/// A position (`center`, `top left`, `25% 75%`) as fractions of the box.
pub(crate) fn position(w: &[&str]) -> Option<(f32, f32)> {
    let (mut x, mut y) = (0.5, 0.5);
    let mut xs = Vec::new();
    for word in w {
        match *word {
            "left" => x = 0.0,
            "right" => x = 1.0,
            "top" => y = 0.0,
            "bottom" => y = 1.0,
            "center" => {}
            p => xs.push(amount(p)?),
        }
    }
    match xs.as_slice() {
        [] => {}
        [a] => x = *a,
        [a, b] => (x, y) = (*a, *b),
        _ => return None,
    }
    Some((x, y))
}

/// An `object-position` (CSS's `<position>`): one value (`top`, `25%`,
/// the other axis centered), two (`left bottom`, `bottom left`, `25% 10px`,
/// `right 30%`: horizontal first unless both are keywords) or four, edges
/// with offsets (`right 10px bottom 20%`, either axis first). Three values
/// are not a `<position>`, as in browsers.
pub(crate) fn object_position(s: &str) -> Option<ObjectPosition> {
    use crate::style::PositionAxis as A;
    #[derive(Clone, Copy, PartialEq)]
    enum Word {
        Left,
        Right,
        Top,
        Bottom,
        Center,
        Length(DefiniteLength),
    }
    let word = |w: &str| -> Option<Word> {
        Some(match w {
            "left" => Word::Left,
            "right" => Word::Right,
            "top" => Word::Top,
            "bottom" => Word::Bottom,
            "center" => Word::Center,
            w => Word::Length(length_or_percent(w)?),
        })
    };
    let horizontal = |w: Word| -> Option<A> {
        match w {
            Word::Left => Some(A::START),
            Word::Center => Some(A::CENTER),
            Word::Right => Some(A::END),
            Word::Length(l) => Some(A::start(l)),
            _ => None,
        }
    };
    let vertical = |w: Word| -> Option<A> {
        match w {
            Word::Top => Some(A::START),
            Word::Center => Some(A::CENTER),
            Word::Bottom => Some(A::END),
            Word::Length(l) => Some(A::start(l)),
            _ => None,
        }
    };
    let words: Vec<&str> = s.split_whitespace().collect();
    let (x, y) = match words.as_slice() {
        [a] => match word(a)? {
            w @ (Word::Top | Word::Bottom) => (A::CENTER, vertical(w)?),
            w => (horizontal(w)?, A::CENTER),
        },
        [a, b] => {
            let (a, b) = (word(a)?, word(b)?);
            // Two keywords go in either order (`top left`).
            let keyword = |w: Word| !matches!(w, Word::Length(_));
            let swapped = keyword(a)
                && keyword(b)
                && (matches!(a, Word::Top | Word::Bottom) || matches!(b, Word::Left | Word::Right));
            let (a, b) = if swapped { (b, a) } else { (a, b) };
            (horizontal(a)?, vertical(b)?)
        }
        [e1, o1, e2, o2] => {
            let mut x = None;
            let mut y = None;
            for (edge, o) in [(*e1, *o1), (*e2, *o2)] {
                let o = length_or_percent(o)?;
                let (slot, axis) = match edge {
                    "left" => (&mut x, A::start(o)),
                    "right" => (&mut x, A::end(o)),
                    "top" => (&mut y, A::start(o)),
                    "bottom" => (&mut y, A::end(o)),
                    _ => return None,
                };
                if slot.replace(axis).is_some() {
                    return None;
                }
            }
            (x?, y?)
        }
        _ => return None,
    };
    Some(ObjectPosition::new(x, y))
}

/// A `filter` list (`none` is empty).
pub(crate) fn filters(s: &str) -> Option<Vec<Filter>> {
    let s = s.trim();
    if s.is_empty() || s == "none" {
        return Some(Vec::new());
    }
    words(s).into_iter().map(filter).collect()
}

fn filter(s: &str) -> Option<Filter> {
    let open = s.find('(')?;
    let name = &s[..open];
    let arg = s[open + 1..].strip_suffix(')')?.trim();
    let factor = |default: f32| {
        if arg.is_empty() {
            Some(default)
        } else {
            amount(arg)
        }
    };
    Some(match name {
        "blur" => Filter::Blur(px(if arg.is_empty() { 0.0 } else { length(arg)? })),
        "brightness" => Filter::Brightness(factor(1.0)?),
        "contrast" => Filter::Contrast(factor(1.0)?),
        "grayscale" => Filter::Grayscale(factor(1.0)?.clamp(0.0, 1.0)),
        "hue-rotate" => Filter::HueRotate(deg(if arg.is_empty() { 0.0 } else { angle(arg)? })),
        "invert" => Filter::Invert(factor(1.0)?.clamp(0.0, 1.0)),
        "opacity" => Filter::Opacity(factor(1.0)?.clamp(0.0, 1.0)),
        "saturate" => Filter::Saturate(factor(1.0)?),
        "sepia" => Filter::Sepia(factor(1.0)?.clamp(0.0, 1.0)),
        "drop-shadow" => {
            let s = box_shadow(arg)?;
            Filter::DropShadow {
                offset_x: s.offset_x,
                offset_y: s.offset_y,
                blur: s.blur,
                color: s.color,
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_positions() {
        use crate::style::PositionAxis as A;
        let p = |s: &str| object_position(s).unwrap_or_else(|| panic!("{s}"));
        let l = |v: f32| DefiniteLength::from(px(v));
        let f = |v: f32| DefiniteLength::from(percentage(v));
        let at = ObjectPosition::new;
        assert_eq!(p("center"), ObjectPosition::CENTER);
        assert_eq!(p("top"), at(A::CENTER, A::START));
        assert_eq!(p("bottom"), at(A::CENTER, A::END));
        assert_eq!(p("left"), at(A::START, A::CENTER));
        assert_eq!(p("25%"), at(A::start(f(0.25)), A::CENTER));
        assert_eq!(p("left top"), at(A::START, A::START));
        assert_eq!(p("top left"), at(A::START, A::START));
        assert_eq!(p("bottom right"), at(A::END, A::END));
        assert_eq!(p("center top"), at(A::CENTER, A::START));
        assert_eq!(p("top center"), at(A::CENTER, A::START));
        assert_eq!(p("25% 75%"), at(A::start(f(0.25)), A::start(f(0.75))));
        assert_eq!(p("10px 1rem"), at(A::start(l(10.0)), A::start(l(16.0))));
        assert_eq!(p("right 30%"), at(A::END, A::start(f(0.3))));
        assert_eq!(
            p("right 10px bottom 20%"),
            at(A::end(l(10.0)), A::end(f(0.2)))
        );
        assert_eq!(
            p("bottom 20% right 10px"),
            at(A::end(l(10.0)), A::end(f(0.2)))
        );
        // Not `<position>`s, as in browsers.
        for bad in [
            "",
            "middle",
            "left right",
            "top bottom",
            "10px left",
            "top 10px",
            "left 10px top",
            "center 10px top 5px",
            "left 10px left 5px",
            "1 2 3 4 5",
        ] {
            assert!(object_position(bad).is_none(), "{bad}");
        }
        // Written back as CSS, it reads the same.
        for s in ["top left", "25% 75%", "right 10px bottom 20%", "center"] {
            let v = p(s);
            assert_eq!(p(&v.to_string()), v, "{s}");
        }
    }

    #[test]
    fn object_positions_place_content() {
        use crate::geometry::{point, size};
        let at = |s: &str| {
            // A 100x300 image covering a 100x100 box.
            object_position(s)
                .unwrap()
                .place(size(px(100.0), px(100.0)), size(px(100.0), px(300.0)))
        };
        assert_eq!(at("center"), point(px(0.0), px(-100.0)));
        assert_eq!(at("top"), point(px(0.0), px(0.0)));
        assert_eq!(at("bottom"), point(px(0.0), px(-200.0)));
        assert_eq!(at("left 10px top 25%"), point(px(10.0), px(-50.0)));
        assert_eq!(at("right 10px bottom 10px"), point(px(-10.0), px(-210.0)));
    }

    /// The packed values CSS colors had before the Rust API took `Hsla`
    /// (`crate::color`): TSX apps render as they did.
    #[test]
    fn css_colors_keep_their_packed_values() {
        for (css, packed) in [
            ("#fff", 0xffffffff),
            ("#0000", 0x00000000),
            ("#1a2b3c", 0x1a2b3cff),
            ("#1a2b3c80", 0x1a2b3c80),
            ("rgb(255, 0, 10)", 0xff000aff),
            ("rgb(0 0 0 / 0.1)", 0x0000001a),
            ("rgba(0,0,0,0.5)", 0x00000080),
            ("rgb(50% 25% 12.5%)", 0x804020ff),
            ("rgb(12.7 200.4 99.5)", 0x0dc864ff),
            ("hsl(0 100% 50%)", 0xff0000ff),
            ("hsl(120, 100%, 25%)", 0x008000ff),
            ("hsl(240 100% 50%)", 0x0000ffff),
            ("hsl(200 50% 50%)", 0x4095bfff),
            ("hsl(30 80% 60% / 0.4)", 0xeb994766),
            ("hsl(-30 80% 60%)", 0xeb4799ff),
            ("hsl(390 80% 60%)", 0xeb9947ff),
            ("hsl(0.5turn 33% 77%)", 0xb1d8d8ff),
            ("hsl(1rad 20% 30%)", 0x5c5a3dff),
            ("hsla(210, 40%, 96%, 0.85)", 0xf1f5f9d9),
            ("hsl(359.9 100% 50%)", 0xff0000ff),
            ("hsl(17 3% 97%)", 0xf8f7f7ff),
            ("hsl(222.2 84% 4.9%)", 0x020817ff),
            ("hsl(217.2 91.2% 59.8%)", 0x3b82f6ff),
            ("hsl(0 0% 50%)", 0x808080ff),
            ("hsl(60 100% 100%)", 0xffffffff),
            ("hsl(300 76% 72%)", 0xee81eeff),
            ("hsl(45deg 93% 47%)", 0xe7b008ff),
            ("hsl(160 84% 39% / 50%)", 0x10b77f80),
            ("oklch(62.3% 0.214 259.815)", 0x2b7fffff),
            ("oklab(1 0 0)", 0xffffffff),
            ("oklch(0.7 0.1 30 / 0.5)", 0xd5867980),
            ("RebeccaPurple", 0x663399ff),
            ("transparent", 0x00000000),
            ("teal", 0x008080ff),
        ] {
            assert_eq!(color(css), Some(Color(packed)), "{css}");
            // And through the Rust API's colors.
            let hsla: crate::Hsla = css.parse().unwrap();
            assert_eq!(Color::from(hsla), Color(packed), "{css}");
        }
    }

    #[test]
    fn colors() {
        assert_eq!(color("#fff"), Some(Color(0xffff_ffff)));
        assert_eq!(color("rgb(255, 0, 10)"), Some(Color(0xff00_0aff)));
        assert_eq!(color("rgb(0 0 0 / 0.1)"), Some(Color(0x0000_001a)));
        assert_eq!(color("rgba(0,0,0,0.5)"), Some(Color(0x0000_0080)));
        assert_eq!(color("hsl(0 100% 50%)"), Some(Color(0xff00_00ff)));
        assert_eq!(color("hsl(120, 100%, 25%)"), Some(Color(0x0080_00ff)));
        assert_eq!(color("RebeccaPurple"), Some(Color(0x6633_99ff)));
        assert_eq!(color("transparent"), Some(Color::TRANSPARENT));
        // Tailwind's blue-500.
        assert_eq!(
            color("oklch(62.3% 0.214 259.815)"),
            Some(Color(0x2b7f_ffff))
        );
        assert_eq!(color("oklab(1 0 0)"), Some(Color(0xffff_ffff)));
        assert_eq!(color("nope"), None);
    }

    /// Whether two packed colors differ by at most one step per channel:
    /// mixes of polar colors go through 8-bit inputs and `f32` math.
    fn near(a: Option<Color>, b: Option<Color>) -> bool {
        let (Some(Color(a)), Some(Color(b))) = (a, b) else {
            return false;
        };
        (0..4).all(|i| ((a >> (i * 8)) & 0xff).abs_diff((b >> (i * 8)) & 0xff) <= 1)
    }

    /// `color-mix()`, against what CSS Color 5 and browsers compute.
    #[test]
    fn color_mix() {
        for (css, packed) in [
            // Both percentages omitted: 50 / 50.
            ("color-mix(in srgb, red, blue)", 0x800080ff),
            ("COLOR-MIX(IN SRGB, red, blue)", 0x800080ff),
            // One omitted: the rest of 100%, before or after the color.
            ("color-mix(in srgb, red 25%, blue)", 0x4000bfff),
            ("color-mix(in srgb, 25% red, blue)", 0x4000bfff),
            ("color-mix(in srgb, red, blue 75%)", 0x4000bfff),
            ("color-mix(in srgb, red 100%, blue)", 0xff0000ff),
            // A sum below 100%: scaled, and the result's alpha times the sum.
            ("color-mix(in srgb, red 20%, blue 30%)", 0x66009980),
            // A sum above 100%: scaled, alpha kept.
            ("color-mix(in srgb, red 75%, blue 75%)", 0x800080ff),
            // Premultiplied alpha: transparent only lowers the alpha.
            ("color-mix(in srgb, #ff0000 30%, transparent)", 0xff00004d),
            ("color-mix(in oklab, transparent, #3b82f6)", 0x3b82f680),
            ("color-mix(in oklch, transparent, #3b82f6 40%)", 0x3b82f666),
            (
                "color-mix(in srgb, rgb(255 0 0 / 0.5), rgb(0 0 255))",
                0x5500aabf,
            ),
            ("color-mix(in srgb-linear, red, blue)", 0xbc00bcff),
            ("color-mix(in xyz, red, blue)", 0xbc00bcff),
            ("color-mix(in oklab, white, black)", 0x636363ff),
            // HSL hues: shorter and longer ways around.
            (
                "color-mix(in hsl, hsl(350 100% 50%), hsl(30 100% 50%))",
                0xff2b00ff,
            ),
            (
                "color-mix(in hsl longer hue, hsl(350 100% 50%), hsl(30 100% 50%))",
                0x00d5ffff,
            ),
            // Nested.
            (
                "color-mix(in srgb, color-mix(in srgb, red, blue), black)",
                0x400040ff,
            ),
        ] {
            assert!(
                near(color(css), Some(Color(packed))),
                "{css}: {:x?}",
                color(css)
            );
        }
        // OKLCh hue interpolation, against the colors it should land on.
        let a = "oklch(0.7 0.1 350)";
        let b = "oklch(0.7 0.1 30)";
        for (method, expected) in [
            ("", "oklch(0.7 0.1 10)"),
            (" shorter hue", "oklch(0.7 0.1 10)"),
            (" longer hue", "oklch(0.7 0.1 190)"),
            (" increasing hue", "oklch(0.7 0.1 10)"),
            (" decreasing hue", "oklch(0.7 0.1 190)"),
        ] {
            let css = format!("color-mix(in oklch{method}, {a}, {b})");
            assert!(
                near(color(&css), color(expected)),
                "{css}: {:x?}",
                color(&css)
            );
        }
        // A grey's hue is powerless: the mix keeps the other color's.
        assert!(near(
            color("color-mix(in oklch, white, oklch(0.7 0.1 30))"),
            color("oklch(0.85 0.05 30)")
        ));
        assert!(near(
            color("color-mix(in hsl, white, hsl(120 100% 50%))"),
            color("hsl(120 50% 75%)")
        ));
        for css in [
            "color-mix(in srgb, red 0%, blue 0%)",
            "color-mix(in srgb, red -10%, blue)",
            "color-mix(in srgb, red 120%, blue)",
            "color-mix(in srgb, red 10% 20%, blue)",
            "color-mix(in srgb, red 10, blue)",
            "color-mix(srgb, red, blue)",
            "color-mix(in nope, red, blue)",
            "color-mix(in srgb shorter hue, red, blue)",
            "color-mix(in oklch sideways hue, red, blue)",
            "color-mix(in oklch shorter, red, blue)",
            "color-mix(in srgb, red)",
            "color-mix(in srgb, red, blue, green)",
            "color-mix(in srgb, red, nope)",
        ] {
            assert_eq!(color(css), None, "{css}");
        }
    }

    #[test]
    fn shadows() {
        let s = box_shadows("0 1px 3px 0 rgb(0 0 0 / 0.1), inset 0 1px 2px -1px #f00").unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(
            (s[0].offset_y, s[0].blur, s[0].spread),
            (px(1.0), px(3.0), px(0.0))
        );
        assert_eq!(s[0].color, Color(0x0000_001a));
        assert!(s[1].inset);
        assert_eq!(s[1].spread, px(-1.0));
        assert_eq!(s[1].color, Color(0xff00_00ff));
        assert!(box_shadows("none").unwrap().is_empty());
        assert!(box_shadows("1px").is_none());
    }

    #[test]
    fn gradient_syntax() {
        let g = gradient("linear-gradient(to right, #f00, rgb(0 0 255) 80%)").unwrap();
        assert_eq!(g.kind, GradientKind::Linear { angle: deg(90.0) });
        assert_eq!(g.stops[1].position, Some(percentage(0.8).into()));
        let g = gradient("linear-gradient(45deg in oklab, red, blue)").unwrap();
        assert_eq!(g.kind, GradientKind::Linear { angle: deg(45.0) });
        let g = gradient("radial-gradient(circle at 25% 75%, red, blue)").unwrap();
        assert_eq!(
            g.kind,
            GradientKind::Radial {
                circle: true,
                size: GradientSize::FarthestCorner,
                x: percentage(0.25),
                y: percentage(0.75)
            }
        );
        let g = gradient("conic-gradient(from 90deg, red, blue, red)").unwrap();
        assert_eq!(g.stops.len(), 3);
        assert!(gradient("linear-gradient(red)").unwrap().stops.len() == 2);
    }

    #[test]
    fn gradient_stop_positions() {
        let at = |g: &Gradient| g.stops.iter().map(|s| s.position).collect::<Vec<_>>();
        let fraction = |f: f32| DefiniteLength::from(percentage(f));
        let points = |p: f32| DefiniteLength::from(px(p));
        let g = gradient("repeating-linear-gradient(90deg, red 0px, blue 10px)").unwrap();
        assert!(g.repeating);
        assert_eq!(at(&g), [Some(fraction(0.0)), Some(points(10.0))]);
        // Lengths in every unit, mixed with percentages; two positions
        // make a band.
        let g = gradient("linear-gradient(red 0 1rem, blue 50%, green 2pt)").unwrap();
        assert_eq!(
            at(&g),
            [
                Some(fraction(0.0)),
                Some(points(16.0)),
                Some(fraction(0.5)),
                Some(points(2.0))
            ]
        );
        let g = gradient("radial-gradient(circle, red 4px, blue 20px)").unwrap();
        assert_eq!(at(&g), [Some(points(4.0)), Some(points(20.0))]);
        // Conic stops are angles (as fractions of a turn) or percentages.
        let g = gradient("conic-gradient(red 0deg 90deg, blue 0.5turn, green 75%)").unwrap();
        assert_eq!(
            at(&g),
            [
                Some(fraction(0.0)),
                Some(fraction(0.25)),
                Some(fraction(0.5)),
                Some(fraction(0.75))
            ]
        );
        assert!(gradient("conic-gradient(red 10px, blue)").is_none());
        assert!(gradient("linear-gradient(red 90deg, blue)").is_none());
        assert!(gradient("linear-gradient(red 0 10px 20px, blue)").is_none());
    }

    #[test]
    fn filter_lists() {
        let f = filters("blur(4px) brightness(120%) hue-rotate(90deg) drop-shadow(0 1px 2px #000)")
            .unwrap();
        assert_eq!(f[0], Filter::Blur(px(4.0)));
        assert_eq!(f[1], Filter::Brightness(1.2));
        assert_eq!(f[2], Filter::HueRotate(deg(90.0)));
        assert!(matches!(
            f[3],
            Filter::DropShadow {
                blur: crate::geometry::Points(2.0),
                ..
            }
        ));
    }

    #[test]
    fn units() {
        assert_eq!(length("0.5rem"), Some(8.0));
        assert_eq!(angle("0.25turn"), Some(90.0));
        assert_eq!(words("a rgb(0 0 0) b"), ["a", "rgb(0 0 0)", "b"]);
        assert_eq!(split_top("a(1,2), b", ','), ["a(1,2)", "b"]);
    }
}
