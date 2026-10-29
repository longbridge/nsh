//! Cached background paints and outlines; geometry comes from the shared core.
use std::{cell::RefCell, sync::Arc};

use nsh_core::{
    BorderStyle, Gradient, GradientKind, GradientSize, Points, Size as CoreSize, ViewStyle,
};
use ohos_arkui_binding::{
    component::attribute::ArkUICommonAttribute,
    types::{attribute::ArkUINodeAttributeType as A, view::BlendMode as B},
};
use ohos_drawing_binding::{
    Brush, Canvas, ClipOperation, CornerPosition, Filter, MaskFilter, Matrix, Path, PathDirection,
    PathEffect, PathFillType, Pen, Rect, RoundRect, ShaderEffect, TileMode,
};

use crate::{
    render::{
        geometry::Size,
        node::{Node, Result},
    },
    text::typography::argb,
};
#[derive(Default)]
pub(crate) struct Decoration {
    backgrounds: RefCell<Option<Backgrounds>>,
    native: RefCell<Option<NativeEffects>>,
    shadows: RefCell<[Option<Shadows>; 2]>,
}
struct Shadows {
    effects: Option<Arc<nsh_core::Effects>>,
    size: Size,
    radii: [f32; 4],
    borders: [f32; 4],
    continuous: bool,
    border: Path,
    padding: Path,
    paints: Vec<ShadowPaint>,
}
struct ShadowPaint {
    inset: bool,
    path: Path,
    hole: Option<Path>,
    brush: Brush,
}
struct Backgrounds {
    effects: Option<Arc<nsh_core::Effects>>,
    size: Size,
    radii: [f32; 4],
    continuous: bool,
    path: Path,
    paints: Vec<Brush>,
}
#[derive(Clone, PartialEq)]
struct NativeEffects {
    blur: f32,
    backdrop_blur: f32,
    blend: nsh_core::BlendMode,
}
impl Default for NativeEffects {
    fn default() -> Self {
        Self {
            blur: 0.0,
            backdrop_blur: 0.0,
            blend: nsh_core::BlendMode::Normal,
        }
    }
}
impl Decoration {
    pub(crate) fn invalidate_native(&self) {
        self.native.borrow_mut().take();
    }
    pub(crate) fn clear_paint_cache(&self) {
        self.backgrounds.borrow_mut().take();
        *self.shadows.borrow_mut() = [None, None];
    }
    pub(crate) fn native(&self, owner: &Node, style: &ViewStyle) -> Result<()> {
        use nsh_core::{BlendMode as M, Filter as F};
        for g in style
            .effects
            .as_ref()
            .into_iter()
            .flat_map(|e| &e.background_images)
        {
            if matches!(g.kind, GradientKind::Conic { .. }) && g.repeating && !g.stops.is_empty() {
                let frame = owner.frame.get();
                let stops =
                    g.resolved_stops(CoreSize::new(Points(frame.width), Points(frame.height)));
                let span = stops.last().unwrap().1.0 - stops[0].1.0;
                if span > 1e-5 && (1.0 / span + 3.0) * stops.len() as f32 > 4096.0 {
                    log::warn!("OHOS repeating conic gradient: limiting repeated stops to 4096");
                }
            }
        }
        let mut effect = NativeEffects {
            blend: if owner.scene_active() {
                nsh_core::BlendMode::Normal
            } else {
                style.blend_mode
            },
            ..NativeEffects::default()
        };
        for filter in style
            .effects
            .as_ref()
            .filter(|_| !owner.scene_active())
            .into_iter()
            .flat_map(|e| &e.backdrop_filters)
        {
            if let F::Blur(radius) = filter {
                effect.backdrop_blur += radius.0;
            }
        }
        if !owner.scene_active()
            && let Some(effects) = style.effects.as_ref()
            && effects
                .filters
                .iter()
                .all(|filter| matches!(filter, F::Blur(_)))
        {
            for filter in &effects.filters {
                if let F::Blur(radius) = filter {
                    effect.blur += radius.0;
                }
            }
        }
        let mut previous = self.native.borrow_mut();
        let default = NativeEffects::default();
        let old = previous.as_ref().unwrap_or(&default);
        let native = owner.native.borrow();
        if old.blur != effect.blur {
            owner
                .visual_node()
                .borrow()
                .set_f32_attribute(A::Blur, effect.blur * owner.scale)?;
        }
        if old.backdrop_blur != effect.backdrop_blur {
            native.set_f32_attribute(A::BackdropBlur, effect.backdrop_blur * owner.scale)?;
        }
        if old.blend != effect.blend {
            let mode = match effect.blend {
                M::Normal => B::None,
                M::Multiply => B::Multiply,
                M::Screen => B::Screen,
                M::Overlay => B::Overlay,
                M::Darken => B::Darken,
                M::Lighten => B::Lighten,
                M::ColorDodge => B::ColorDodge,
                M::ColorBurn => B::ColorBurn,
                M::HardLight => B::HardLight,
                M::SoftLight => B::SoftLight,
                M::Difference => B::Difference,
                M::Exclusion => B::Exclusion,
                M::Hue => B::Hue,
                M::Saturation => B::Saturation,
                M::Color => B::Color,
                M::Luminosity => B::Luminosity,
                M::PlusLighter => B::Plus,
                // Scene composition owns this mode. Hidden/detached triggers
                // can keep the style while their native exports retire.
                M::PlusDarker => B::None,
            };
            owner
                .visual_node()
                .borrow()
                .set_attribute(A::BlendMode, vec![i32::from(mode), 1].into())?;
        }
        *previous = Some(effect);
        Ok(())
    }
    pub(crate) fn background(&self, canvas: &Canvas, style: &ViewStyle, size: Size, scale: f32) {
        let Some(effects) = style.effects.as_ref() else {
            self.backgrounds.borrow_mut().take();
            *self.shadows.borrow_mut() = [None, None];
            return;
        };
        if effects.background_images.is_empty() && effects.box_shadows.is_empty() {
            self.backgrounds.borrow_mut().take();
            *self.shadows.borrow_mut() = [None, None];
            return;
        }
        let images = effects.background_images.as_slice();
        if images.is_empty() {
            self.backgrounds.borrow_mut().take();
            self.shadows(canvas, style, size, scale, true);
            return;
        }
        let radii = style
            .corner_radius
            .map(|radius| radius.unwrap_or(style.border_radius).0)
            .to_array();
        let mut backgrounds = self.backgrounds.borrow_mut();
        if backgrounds.as_ref().is_none_or(|c| {
            c.size != size
                || c.radii != radii
                || c.continuous != style.continuous_corners
                || match (&c.effects, &style.effects) {
                    (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                    (None, None) => false,
                    _ => true,
                }
        }) {
            *backgrounds = Some(Backgrounds {
                effects: style.effects.clone(),
                size,
                radii,
                continuous: style.continuous_corners,
                path: outline(style, size, 0.0),
                paints: images.iter().rev().map(|g| gradient(g, size)).collect(),
            });
        }
        let cache = backgrounds.as_ref().unwrap();
        let (sx, sy) = pixel_scale(size, scale);
        canvas.save();
        canvas.scale(sx, sy);
        for brush in &cache.paints {
            canvas.attach_brush(brush);
            canvas.draw_path(&cache.path);
            canvas.detach_brush();
        }
        canvas.restore();
        self.shadows(canvas, style, size, scale, true);
    }
    pub(crate) fn shadows(
        &self,
        canvas: &Canvas,
        style: &ViewStyle,
        size: Size,
        scale: f32,
        inset: bool,
    ) {
        let layers = style
            .effects
            .as_ref()
            .map_or(&[][..], |effects| effects.box_shadows.as_slice());
        let visible_layer = |layer: &nsh_core::BoxShadow| {
            layer.inset == inset && layer.is_visible() && (!inset || layer.blur.0 > 0.0)
        };
        if !layers.iter().any(visible_layer) {
            self.shadows.borrow_mut()[usize::from(inset)] = None;
            return;
        }
        let radii = style
            .corner_radius
            .map(|radius| radius.unwrap_or(style.border_radius).0)
            .to_array();
        let borders = style.borders.widths.to_array().map(|v| v.0);
        let mut caches = self.shadows.borrow_mut();
        let cache = &mut caches[usize::from(inset)];
        if cache.as_ref().is_none_or(|cache| {
            cache.size != size
                || cache.radii != radii
                || cache.borders != borders
                || cache.continuous != style.continuous_corners
                || !match (&cache.effects, &style.effects) {
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                }
        }) {
            let padding = padding_outline(style, size, 0.0);
            let paints = layers
                .iter()
                .rev()
                .filter(|layer| visible_layer(layer))
                .map(|layer| {
                    let mut brush = Brush::new();
                    brush.set_anti_alias(true);
                    brush.set_color(argb(layer.color));
                    if layer.blur.0 > 0.0 {
                        let mask = MaskFilter::blur(layer.blur.0 * 0.5, true);
                        let mut filter = Filter::new();
                        filter.set_mask_filter(mask.as_ref());
                        brush.set_filter(Some(&filter));
                    }
                    let (path, hole) = if layer.inset {
                        let mut hole = padding_outline(style, size, layer.spread.0);
                        hole.transform(&Matrix::from_affine(
                            1.0,
                            0.0,
                            0.0,
                            1.0,
                            layer.offset_x.0,
                            layer.offset_y.0,
                        ));
                        if layer.blur.0 <= 0.0 {
                            (Path::new(), Some(hole))
                        } else {
                            let reach = layer.offset_x.0.abs()
                                + layer.offset_y.0.abs()
                                + layer.spread.0.abs()
                                + layer.blur.0.max(0.0) * 2.0
                                + 4.0;
                            let mut outer = Path::new();
                            outer.set_fill_type(PathFillType::EvenOdd);
                            outer.add_rect(
                                -reach,
                                -reach,
                                size.width + reach,
                                size.height + reach,
                                PathDirection::Cw,
                            );
                            outer.add_path(&hole, None);
                            (outer, None)
                        }
                    } else {
                        let mut path = outline(style, size, layer.spread.0);
                        path.transform(&Matrix::from_affine(
                            1.0,
                            0.0,
                            0.0,
                            1.0,
                            layer.offset_x.0,
                            layer.offset_y.0,
                        ));
                        (path, None)
                    };
                    ShadowPaint {
                        inset: layer.inset,
                        path,
                        hole,
                        brush,
                    }
                })
                .collect();
            *cache = Some(Shadows {
                effects: style.effects.clone(),
                size,
                radii,
                borders,
                continuous: style.continuous_corners,
                border: outline(style, size, 0.0),
                padding,
                paints,
            });
        }
        let cache = cache.as_ref().unwrap();
        let (sx, sy) = pixel_scale(size, scale);
        canvas.save();
        canvas.scale(sx, sy);
        canvas.clip_path(
            if inset { &cache.padding } else { &cache.border },
            if inset {
                ClipOperation::Intersect
            } else {
                ClipOperation::Difference
            },
            true,
        );
        for paint in cache.paints.iter().filter(|paint| paint.inset == inset) {
            if let Some(hole) = &paint.hole {
                canvas.save();
                canvas.clip_path(hole, ClipOperation::Difference, true);
                canvas.attach_brush(&paint.brush);
                canvas.draw_path(&cache.padding);
                canvas.detach_brush();
                canvas.restore();
            } else {
                canvas.attach_brush(&paint.brush);
                canvas.draw_path(&paint.path);
                canvas.detach_brush();
            }
        }
        canvas.restore();
    }
    pub(crate) fn outline(&self, canvas: &Canvas, style: &ViewStyle, size: Size, scale: f32) {
        let o = style.outline;
        if o.width.0 <= 0.0 || o.color.is_transparent() {
            return;
        }
        let width = o.width.0;
        let mut pen = Pen::new();
        pen.set_anti_alias(true);
        pen.set_color(argb(o.color));
        pen.set_width(width);
        let dash = match o.style {
            BorderStyle::Solid => None,
            BorderStyle::Dashed => PathEffect::dash(&[width * 3.0, width * 3.0], 0.0),
            BorderStyle::Dotted => PathEffect::dash(&[width, width], 0.0),
        };
        pen.set_path_effect(dash.as_ref());
        let (sx, sy) = pixel_scale(size, scale);
        canvas.save();
        canvas.scale(sx, sy);
        canvas.attach_pen(&pen);
        canvas.draw_path(&outline(style, size, o.offset.0 + width * 0.5));
        canvas.detach_pen();
        canvas.restore();
    }
}
fn pixel_scale(size: Size, scale: f32) -> (f32, f32) {
    let sx = if size.width > 0.0 {
        (size.width * scale).round().max(1.0) / size.width
    } else {
        scale
    };
    let sy = if size.height > 0.0 {
        (size.height * scale).round().max(1.0) / size.height
    } else {
        scale
    };
    (sx, sy)
}
fn padding_outline(style: &ViewStyle, size: Size, spread: f32) -> Path {
    let [top, right, bottom, left] = style
        .borders
        .widths
        .to_array()
        .map(|width| width.0 + spread);
    let radii = nsh_core::canvas::fit_radii(
        size.into(),
        style
            .corner_radius
            .map(|radius| radius.unwrap_or(style.border_radius)),
    )
    .to_array()
    .map(|r| r.0);
    let corners = [(left, top), (right, top), (right, bottom), (left, bottom)];
    let mut path = Path::new();
    if size.width <= left + right || size.height <= top + bottom {
        return path;
    }
    let rect = Rect::new(left, top, size.width - right, size.height - bottom);
    let mut round_rect = RoundRect::new(&rect, 0.0, 0.0);
    let positions = [
        CornerPosition::TopLeft,
        CornerPosition::TopRight,
        CornerPosition::BottomRight,
        CornerPosition::BottomLeft,
    ];
    for (i, pos) in positions.into_iter().enumerate() {
        round_rect.set_corner(
            pos,
            (radii[i] - corners[i].0).max(0.0),
            (radii[i] - corners[i].1).max(0.0),
        );
    }
    path.add_round_rect(&round_rect, PathDirection::Cw);
    path
}
pub(crate) fn outline(style: &ViewStyle, size: Size, spread: f32) -> Path {
    let mut path = Path::new();
    let w = (size.width + spread * 2.0).max(0.0);
    let h = (size.height + spread * 2.0).max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return path;
    }
    let radii = nsh_core::canvas::fit_radii(
        CoreSize::new(Points(w), Points(h)),
        style
            .corner_radius
            .map(|r| Points((r.unwrap_or(style.border_radius).0 + spread).max(0.0))),
    )
    .to_array()
    .map(|r| r.0);
    let rect = Rect::new(-spread, -spread, -spread + w, -spread + h);
    let mut round_rect = RoundRect::new(&rect, 0.0, 0.0);
    let positions = [
        CornerPosition::TopLeft,
        CornerPosition::TopRight,
        CornerPosition::BottomRight,
        CornerPosition::BottomLeft,
    ];
    for (i, pos) in positions.into_iter().enumerate() {
        round_rect.set_corner(pos, radii[i], radii[i]);
    }
    path.add_round_rect(&round_rect, PathDirection::Cw);
    path
}
pub(crate) fn drawing_blend(mode: nsh_core::BlendMode) -> ohos_drawing_binding::BlendMode {
    use nsh_core::BlendMode as M;
    use ohos_drawing_binding::BlendMode as B;
    match mode {
        M::Normal | M::PlusDarker => B::SrcOver,
        M::Multiply => B::Multiply,
        M::Screen => B::Screen,
        M::Overlay => B::Overlay,
        M::Darken => B::Darken,
        M::Lighten => B::Lighten,
        M::ColorDodge => B::ColorDodge,
        M::ColorBurn => B::ColorBurn,
        M::HardLight => B::HardLight,
        M::SoftLight => B::SoftLight,
        M::Difference => B::Difference,
        M::Exclusion => B::Exclusion,
        M::Hue => B::Hue,
        M::Saturation => B::Saturation,
        M::Color => B::Color,
        M::Luminosity => B::Luminosity,
        M::PlusLighter => B::Plus,
    }
}
fn gradient(g: &Gradient, size: Size) -> Brush {
    let mut brush = Brush::new();
    brush.set_anti_alias(true);
    if g.stops.is_empty() {
        return brush;
    }
    let stops = g.resolved_stops(size.into());
    let first = stops[0].1.0;
    let last = stops.last().unwrap().1.0;
    let span = (last - first).max(1e-6);
    brush.set_color(argb(stops.last().unwrap().0));
    let colors: Vec<_> = stops.iter().map(|(c, _)| argb(*c)).collect();
    let positions: Vec<_> = stops.iter().map(|(_, p)| (p.0 - first) / span).collect();
    let tile = if g.repeating {
        TileMode::Repeat
    } else {
        TileMode::Clamp
    };
    let (w, h) = (size.width, size.height);
    let shader = match g.kind {
        GradientKind::Radial { circle, size, x, y } => {
            let (cx, cy) = (x.0 * w, y.0 * h);
            let (rx, ry) = radii(circle, size, cx, cy, w, h);
            let matrix = Matrix::from_affine(rx.max(0.01), 0.0, 0.0, ry.max(0.01), cx, cy);
            ShaderEffect::two_point_conical_gradient(
                (0.0, 0.0),
                first.max(0.0),
                (0.0, 0.0),
                last.max(0.0001),
                &colors,
                &positions,
                tile,
                Some(&matrix),
            )
        }
        GradientKind::Conic { from, x, y } => {
            let mut stops: Vec<_> = stops.into_iter().map(|(c, p)| (p.0, c)).collect();
            if g.repeating && span > 1e-5 {
                let original = stops.clone();
                stops.clear();
                let start = (-last / span).floor() as i32;
                let end = ((1.0 - first) / span).ceil() as i32;
                // A maliciously tiny period must not allocate unbounded stops.
                if (end - start) as usize * original.len() <= 4096 {
                    for k in start..=end {
                        stops.extend(original.iter().map(|(p, c)| (*p + k as f32 * span, *c)));
                    }
                } else {
                    stops = vec![(0.0, original[0].1), (1.0, original.last().unwrap().1)];
                }
            }
            let begin = crate::render::canvas::sample(&stops, 0.0);
            let end = crate::render::canvas::sample(&stops, 1.0);
            stops.retain(|(p, _)| *p > 0.0 && *p < 1.0);
            stops.insert(0, (0.0, begin));
            stops.push((1.0, end));
            let (colors, positions) =
                crate::render::canvas::sweep_stops(&stops, from.0 - std::f32::consts::FRAC_PI_2);
            ShaderEffect::conic_gradient((x.0 * w, y.0 * h), &colors, &positions, TileMode::Clamp)
        }
        kind => {
            let angle = kind
                .linear_angle(size.into())
                .map_or(std::f32::consts::PI, |r| r.0);
            let (dx, dy) = (angle.sin(), -angle.cos());
            let length = (w * dx).abs() + (h * dy).abs();
            let point = |t: f32| {
                (
                    w * 0.5 + dx * length * (t - 0.5),
                    h * 0.5 + dy * length * (t - 0.5),
                )
            };
            ShaderEffect::linear_gradient(
                point(first),
                point(last.max(first + 1e-6)),
                &colors,
                &positions,
                tile,
            )
        }
    };
    if shader.is_some() {
        brush.set_color(0xffffffff);
        brush.set_shader_effect(shader.as_ref());
    }
    brush
}
fn radii(circle: bool, size: GradientSize, cx: f32, cy: f32, w: f32, h: f32) -> (f32, f32) {
    let near = (cx.abs().min((w - cx).abs()), cy.abs().min((h - cy).abs()));
    let far = (cx.abs().max((w - cx).abs()), cy.abs().max((h - cy).abs()));
    if circle {
        let r = match size {
            GradientSize::ClosestSide => near.0.min(near.1),
            GradientSize::FarthestSide => far.0.max(far.1),
            GradientSize::ClosestCorner => near.0.hypot(near.1),
            GradientSize::FarthestCorner => far.0.hypot(far.1),
        };
        (r, r)
    } else {
        match size {
            GradientSize::ClosestSide => near,
            GradientSize::FarthestSide => far,
            GradientSize::ClosestCorner => (
                near.0 * std::f32::consts::SQRT_2,
                near.1 * std::f32::consts::SQRT_2,
            ),
            GradientSize::FarthestCorner => (
                far.0 * std::f32::consts::SQRT_2,
                far.1 * std::f32::consts::SQRT_2,
            ),
        }
    }
}
