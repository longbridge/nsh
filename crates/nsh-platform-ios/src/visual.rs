//! Effects beyond classic view styles, as Core Animation layers of a view:
//! layered and inset box shadows, gradient backgrounds, outlines, a
//! brightness veil and backdrop blur.
//!
//! CSS draws a box shadow only outside the box, so outer shadow layers are
//! masked to everything but the box: they can sit above the view's own
//! background (sublayers always draw over a layer's contents) without
//! covering it, and work with transparent backgrounds.

use nsh_core::{
    BorderStyle, BoxShadow, Color, CornerRadii, Filter, Gradient, GradientKind, GradientSize,
    ViewStyle,
};
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained};
use objc2_core_foundation::{CFRetained, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGMutablePath;
use objc2_foundation::{NSArray, NSNumber};
use objc2_quartz_core::{
    CAGradientLayer, CALayer, CAShapeLayer, kCAFillRuleEvenOdd, kCAGradientLayerAxial,
    kCAGradientLayerConic, kCAGradientLayerRadial, kCALineCapButt, kCALineCapRound,
};
use objc2_ui_kit::{
    UIBlurEffect, UIBlurEffectStyle, UIView, UIViewAnimating, UIViewAnimationCurve,
    UIViewPropertyAnimator, UIVisualEffectView,
};

use crate::decor::{cg_color, inset, rounded};

/// Shadows sit below the background and the children.
const SHADOW_Z: CGFloat = -2048.0;
/// Gradients sit above the background color, below borders.
const GRADIENT_Z: CGFloat = -1024.0 + 0.25;
/// Inset shadows sit above backgrounds, below borders.
const INSET_Z: CGFloat = -1024.0 + 0.5;
/// Outlines and the brightness veil are above everything.
const OUTLINE_Z: CGFloat = 2048.0;
const VEIL_Z: CGFloat = 2049.0;

/// The effect layers of one view.
#[derive(Default)]
pub(crate) struct Visual {
    shadows: Vec<Retained<CALayer>>,
    /// The holder clipped to the padding box, and one shape per layer.
    insets: Option<(Retained<CALayer>, Vec<Retained<CAShapeLayer>>)>,
    gradients: Vec<Retained<CAGradientLayer>>,
    outline: Option<Retained<CAShapeLayer>>,
    veil: Option<Retained<CALayer>>,
    backdrop: Option<Backdrop>,
}

struct Backdrop {
    view: Retained<UIVisualEffectView>,
    /// Holds the blur at a fraction of the material's strength.
    animator: Retained<UIViewPropertyAnimator>,
    radius: f32,
    dark: bool,
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        // A paused animator must be stopped before it is released.
        self.animator.stopAnimation(true);
        self.view.removeFromSuperview();
    }
}

/// Whether `style` has effects that draw outside the box (they need an
/// unclipped layer, so clipping moves to a content view).
pub(crate) fn draws_outside(style: &ViewStyle) -> bool {
    style.outline.is_visible()
        || style
            .effects
            .as_ref()
            .is_some_and(|e| e.box_shadows.iter().any(|s| !s.inset && s.is_visible()))
}

/// The view's drop shadow filter, if any (it follows the content's shape).
pub(crate) fn drop_shadow(style: &ViewStyle) -> Option<(f32, f32, f32, Color)> {
    style
        .effects
        .as_ref()?
        .filters
        .iter()
        .find_map(|f| match *f {
            Filter::DropShadow {
                offset_x,
                offset_y,
                blur,
                color,
            } => Some((offset_x.0, offset_y.0, blur.0, color)),
            _ => None,
        })
}

/// The opacity factor of the view's filters (`opacity()`).
pub(crate) fn filter_opacity(style: &ViewStyle) -> f32 {
    style.effects.as_ref().map_or(1.0, |e| {
        e.filters
            .iter()
            .map(|f| match *f {
                Filter::Opacity(o) => o,
                _ => 1.0,
            })
            .product()
    })
}

impl Visual {
    /// Shapes the effect layers of `view` (its own `layer`, or the content
    /// view's when it clips its children) after `style`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply(
        &mut self,
        view: &UIView,
        style: &ViewStyle,
        local: CGRect,
        radii: CornerRadii,
        text: bool,
        mtm: MainThreadMarker,
    ) {
        let layer = view.layer();
        let effects = style.effects.as_deref();
        let shadows = effects.map_or(&[][..], |e| &e.box_shadows[..]);
        let outer: Vec<&BoxShadow> = shadows
            .iter()
            .filter(|s| !s.inset && s.is_visible())
            .collect();
        let inner: Vec<&BoxShadow> = shadows
            .iter()
            .filter(|s| s.inset && s.is_visible())
            .collect();
        self.set_shadows(&layer, &outer, local, radii);
        // Text draws in the layer's own contents, below every sublayer:
        // effects inside the box would cover it.
        let inner: &[&BoxShadow] = if text { &[] } else { &inner };
        let border = style.borders.uniform().map_or(0.0, |(w, _)| w);
        self.set_insets(&layer, inner, local, radii, border);
        let gradients = if text {
            &[][..]
        } else {
            effects.map_or(&[][..], |e| &e.background_images[..])
        };
        self.set_gradients(&layer, gradients, local, radii);
        self.set_outline(&layer, style, local, radii);
        let brightness = effects.map_or(1.0, |e| {
            e.filters
                .iter()
                .map(|f| match *f {
                    Filter::Brightness(b) => b,
                    _ => 1.0,
                })
                .product::<f32>()
        });
        self.set_veil(&layer, brightness, local, radii);
        let blur = effects.map_or(0.0, |e| {
            e.backdrop_filters
                .iter()
                .map(|f| match *f {
                    Filter::Blur(r) => r.0,
                    _ => 0.0,
                })
                .sum::<f32>()
        });
        // The material's own tint follows the view's background (CSS only
        // blurs; the background color tints): light unless it is dark.
        let (r, g, b, a) = style.background_color.to_f64();
        let dark = a > 0.0 && 0.2126 * r + 0.7152 * g + 0.0722 * b < 0.5;
        self.set_backdrop(view, blur, dark, local, radii, mtm);
    }

    fn set_shadows(
        &mut self,
        layer: &CALayer,
        shadows: &[&BoxShadow],
        local: CGRect,
        radii: CornerRadii,
    ) {
        resize(&mut self.shadows, shadows.len(), layer, || {
            let l = CALayer::new();
            l.setZPosition(SHADOW_Z);
            l
        });
        let box_path = rounded(local, radii);
        // CSS order is top first; later sublayers draw on top.
        for (l, s) in self.shadows.iter().zip(shadows.iter().rev()) {
            let spread = s.spread.0 as CGFloat;
            let shape = grow(local, spread);
            l.setFrame(local);
            l.setShadowPath(Some(&rounded(shape, grown(radii, s.spread.0))));
            l.setShadowColor(cg_color(Color(s.color.0 | 0xff)).as_deref());
            l.setShadowOpacity((s.color.0 & 0xff) as f32 / 255.0);
            l.setShadowOffset(CGSize::new(
                s.offset_x.0 as CGFloat,
                s.offset_y.0 as CGFloat,
            ));
            // CSS blur is twice the Gaussian's deviation; Core Animation's
            // radius is close to the deviation.
            l.setShadowRadius(s.blur.0 as CGFloat / 2.0);
            // Everything but the box.
            let reach = (s.offset_x.0.abs().max(s.offset_y.0.abs())
                + s.spread.0.abs()
                + s.blur.0 * 2.0
                + 4.0) as CGFloat;
            let mask = mask_layer(l);
            let around = grow(local, reach);
            let path = CGMutablePath::new();
            // SAFETY: valid mutable paths, a null transform.
            unsafe {
                CGMutablePath::add_rect(Some(&path), std::ptr::null(), around);
                CGMutablePath::add_path(Some(&path), std::ptr::null(), Some(&box_path));
            }
            mask.setFrame(local);
            mask.setPath(Some(&path));
        }
    }

    fn set_insets(
        &mut self,
        layer: &CALayer,
        shadows: &[&BoxShadow],
        local: CGRect,
        radii: CornerRadii,
        border: f32,
    ) {
        if shadows.is_empty() {
            if let Some((h, _)) = self.insets.take() {
                h.removeFromSuperlayer();
            }
            return;
        }
        let (holder, shapes) = self.insets.get_or_insert_with(|| {
            let h = CALayer::new();
            h.setZPosition(INSET_Z);
            layer.addSublayer(&h);
            (h, Vec::new())
        });
        // Inside the padding box.
        let padding = inset(local, border as CGFloat);
        let inner_radii = radii.inset(border);
        holder.setFrame(local);
        let mask = mask_layer(holder);
        mask.setFrame(local);
        mask.setPath(Some(&rounded(padding, inner_radii)));
        resize(shapes, shadows.len(), holder, CAShapeLayer::new);
        for (l, s) in shapes.iter().zip(shadows.iter().rev()) {
            let reach = (s.offset_x.0.abs().max(s.offset_y.0.abs())
                + s.spread.0.abs()
                + s.blur.0 * 2.0
                + 4.0) as CGFloat;
            // The frame around the shifted, shrunk inner shape casts its
            // shadow inwards (non-zero winding: the hole runs backwards).
            let hole = offset(
                grow(padding, -(s.spread.0 as CGFloat)),
                s.offset_x.0,
                s.offset_y.0,
            );
            let path = CGMutablePath::new();
            // SAFETY: valid mutable paths, a null transform.
            unsafe {
                CGMutablePath::add_rect(Some(&path), std::ptr::null(), grow(local, reach));
            }
            add_reversed(&path, hole, grown(inner_radii, -s.spread.0));
            l.setFrame(local);
            l.setShadowPath(Some(&path));
            l.setShadowColor(cg_color(Color(s.color.0 | 0xff)).as_deref());
            l.setShadowOpacity((s.color.0 & 0xff) as f32 / 255.0);
            l.setShadowOffset(CGSize::ZERO);
            l.setShadowRadius(s.blur.0 as CGFloat / 2.0);
        }
    }

    fn set_gradients(
        &mut self,
        layer: &CALayer,
        gradients: &[Gradient],
        local: CGRect,
        radii: CornerRadii,
    ) {
        resize(&mut self.gradients, gradients.len(), layer, || {
            let g = CAGradientLayer::new();
            g.setZPosition(GRADIENT_Z);
            g
        });
        let shared = radii.shared();
        // CSS order is top first.
        for (g, gradient) in self.gradients.iter().zip(gradients.iter().rev()) {
            g.setFrame(local);
            match shared {
                Some(r) => {
                    crate::decor::set_corners(
                        g,
                        r as CGFloat,
                        crate::decor::corner_mask(&radii),
                        radii.continuous,
                    );
                    g.setMasksToBounds(true);
                    // SAFETY: clearing the mask.
                    unsafe { g.setMask(None) };
                }
                None => {
                    g.setCornerRadius(0.0);
                    g.setMasksToBounds(false);
                    let mask = mask_layer(g);
                    mask.setFrame(local);
                    mask.setPath(Some(&rounded(local, radii)));
                }
            }
            shape_gradient(g, gradient, local.size);
        }
    }

    fn set_outline(
        &mut self,
        layer: &CALayer,
        style: &ViewStyle,
        local: CGRect,
        radii: CornerRadii,
    ) {
        let o = style.outline;
        if !o.is_visible() {
            if let Some(l) = self.outline.take() {
                l.removeFromSuperlayer();
            }
            return;
        }
        let l = self.outline.get_or_insert_with(|| {
            let l = CAShapeLayer::new();
            l.setZPosition(OUTLINE_Z);
            l.setFillColor(None);
            layer.addSublayer(&l);
            l
        });
        // Centered on a line `offset` outside the border box; the corners
        // follow the border radius outwards.
        let d = (o.offset + o.width / 2.0).0;
        let radii = if radii.is_zero() {
            radii
        } else {
            grown(radii, d)
        };
        l.setFrame(local);
        l.setPath(Some(&rounded(grow(local, d as CGFloat), radii)));
        l.setLineWidth(o.width.0 as CGFloat);
        l.setStrokeColor(cg_color(o.color).as_deref());
        let w = o.width.0 as f64;
        let pattern: Option<Retained<NSArray<NSNumber>>> = match o.style {
            BorderStyle::Solid => None,
            BorderStyle::Dashed => Some(NSArray::from_retained_slice(&[
                NSNumber::new_f64(w * 3.0),
                NSNumber::new_f64(w * 3.0),
            ])),
            BorderStyle::Dotted => Some(NSArray::from_retained_slice(&[
                NSNumber::new_f64(0.0),
                NSNumber::new_f64(w * 2.0),
            ])),
        };
        l.setLineDashPattern(pattern.as_deref());
        // SAFETY: immutable Core Animation constants.
        l.setLineCap(unsafe {
            if o.style == BorderStyle::Dotted {
                kCALineCapRound
            } else {
                kCALineCapButt
            }
        });
    }

    /// `brightness()` below 1: a black veil over the view (exact for opaque
    /// content). Brighter values are not drawn on iOS.
    fn set_veil(&mut self, layer: &CALayer, brightness: f32, local: CGRect, radii: CornerRadii) {
        if brightness >= 1.0 {
            if let Some(v) = self.veil.take() {
                v.removeFromSuperlayer();
            }
            return;
        }
        let v = self.veil.get_or_insert_with(|| {
            let v = CALayer::new();
            v.setZPosition(VEIL_Z);
            v.setBackgroundColor(cg_color(Color::BLACK).as_deref());
            layer.addSublayer(&v);
            v
        });
        v.setFrame(local);
        v.setOpacity((1.0 - brightness.max(0.0)).min(1.0));
        let mask = mask_layer(v);
        mask.setFrame(local);
        mask.setPath(Some(&rounded(local, radii)));
    }

    /// `backdrop-filter: blur()`: a system blur behind the content, its
    /// strength set by holding a property animator at a fraction.
    fn set_backdrop(
        &mut self,
        view: &UIView,
        radius: f32,
        dark: bool,
        local: CGRect,
        radii: CornerRadii,
        mtm: MainThreadMarker,
    ) {
        if radius <= 0.0 {
            self.backdrop = None;
            return;
        }
        if self
            .backdrop
            .as_ref()
            .is_none_or(|b| b.radius != radius || b.dark != dark)
        {
            self.backdrop = None;
            let effect_view =
                UIVisualEffectView::initWithEffect(UIVisualEffectView::alloc(mtm), None);
            effect_view.setUserInteractionEnabled(false);
            let target = effect_view.clone();
            let material = if dark {
                UIBlurEffectStyle::SystemUltraThinMaterialDark
            } else {
                UIBlurEffectStyle::SystemUltraThinMaterialLight
            };
            let block = block2::RcBlock::new(move || {
                let blur = UIBlurEffect::effectWithStyle(material, mtm);
                target.setEffect(Some(&blur));
            });
            let animator = UIViewPropertyAnimator::initWithDuration_curve_animations(
                UIViewPropertyAnimator::alloc(mtm),
                1.0,
                UIViewAnimationCurve::Linear,
                Some(&block),
            );
            animator.setPausesOnCompletion(true);
            // The material blurs by about 30 points at full strength.
            animator.setFractionComplete((radius / 30.0).clamp(0.05, 1.0) as CGFloat);
            view.insertSubview_atIndex(&effect_view, 0);
            self.backdrop = Some(Backdrop {
                view: effect_view,
                animator,
                radius,
                dark,
            });
        }
        if let Some(b) = &self.backdrop {
            b.view.setFrame(local);
            let layer = b.view.layer();
            match radii.shared() {
                Some(r) => {
                    crate::decor::set_corners(
                        &layer,
                        r as CGFloat,
                        crate::decor::corner_mask(&radii),
                        radii.continuous,
                    );
                    b.view.setClipsToBounds(true);
                }
                None => {
                    b.view.setClipsToBounds(false);
                    let mask = CAShapeLayer::new();
                    mask.setPath(Some(&rounded(local, radii)));
                    // SAFETY: a fresh mask layer owned by the effect view.
                    unsafe { layer.setMask(Some(&mask)) };
                }
            }
        }
    }
}

/// Keeps `layers` at `n`, adding new ones from `make` to `parent`.
fn resize<L: AsRef<CALayer>>(
    layers: &mut Vec<Retained<L>>,
    n: usize,
    parent: &CALayer,
    make: impl Fn() -> Retained<L>,
) {
    while layers.len() > n {
        if let Some(l) = layers.pop() {
            l.as_ref().removeFromSuperlayer();
        }
    }
    while layers.len() < n {
        let l = make();
        parent.addSublayer(l.as_ref());
        layers.push(l);
    }
}

/// The even-odd shape mask of `layer` (created on first use).
fn mask_layer(layer: &CALayer) -> Retained<CAShapeLayer> {
    // SAFETY: masks of effect layers are always shape layers owned here.
    if let Some(m) = layer.mask()
        && let Ok(m) = m.downcast::<CAShapeLayer>()
    {
        return m;
    }
    let m = CAShapeLayer::new();
    // SAFETY: an immutable Core Animation constant.
    m.setFillRule(unsafe { kCAFillRuleEvenOdd });
    // SAFETY: a fresh mask layer, owned by `layer`.
    unsafe { layer.setMask(Some(&m)) };
    m
}

fn grow(rect: CGRect, d: CGFloat) -> CGRect {
    CGRect::new(
        CGPoint::new(rect.origin.x - d, rect.origin.y - d),
        CGSize::new(
            (rect.size.width + 2.0 * d).max(0.0),
            (rect.size.height + 2.0 * d).max(0.0),
        ),
    )
}

fn offset(rect: CGRect, x: f32, y: f32) -> CGRect {
    CGRect::new(
        CGPoint::new(rect.origin.x + x as CGFloat, rect.origin.y + y as CGFloat),
        rect.size,
    )
}

/// Radii grown by `d` (shrunk when negative), corners without a radius
/// staying square.
fn grown(radii: CornerRadii, d: f32) -> CornerRadii {
    let g = |r: f32| if r > 0.0 { (r + d).max(0.0) } else { 0.0 };
    CornerRadii {
        top_left: g(radii.top_left),
        top_right: g(radii.top_right),
        bottom_right: g(radii.bottom_right),
        bottom_left: g(radii.bottom_left),
        ..radii
    }
}

/// Adds a rounded rectangle to `path` counterclockwise (a hole under the
/// non-zero rule of shadow paths).
fn add_reversed(path: &CFRetained<CGMutablePath>, rect: CGRect, radii: CornerRadii) {
    crate::decor::add_rounded(path, rect, radii, true);
}

/// Configures `g` to draw `gradient` over a `size` box.
fn shape_gradient(g: &CAGradientLayer, gradient: &Gradient, size: CGSize) {
    let (w, h) = (size.width.max(1.0), size.height.max(1.0));
    let stops = gradient.resolved_stops(nsh_core::Size::new(
        nsh_core::Points(w as f32),
        nsh_core::Points(h as f32),
    ));
    // A transparent stop takes its neighbor's hue (as CSS's premultiplied
    // mixing does), so fades to transparent do not darken.
    let colors: Vec<Retained<objc2::runtime::AnyObject>> = (0..stops.len())
        .map(|i| {
            let c = stops[i].0;
            let c = if c.is_transparent() {
                let neighbor = stops[i + 1..]
                    .iter()
                    .chain(stops[..i].iter().rev())
                    .map(|(n, _)| *n)
                    .find(|n| !n.is_transparent());
                neighbor.map_or(c, |n| Color(n.0 & 0xffff_ff00))
            } else {
                c
            };
            let color = crate::text::cg_color(c);
            // SAFETY: a CGColor, which `colors` takes; CF objects are
            // Objective-C objects.
            unsafe {
                Retained::from_raw(CFRetained::into_raw(color).as_ptr().cast()).expect("color")
            }
        })
        .collect();
    let locations: Vec<Retained<NSNumber>> = stops
        .iter()
        .map(|(_, p)| NSNumber::new_f64(p.0 as f64))
        .collect();
    // SAFETY: an array of CGColors, as `colors` documents.
    unsafe { g.setColors(Some(&NSArray::from_retained_slice(&colors))) };
    g.setLocations(Some(&NSArray::from_retained_slice(&locations)));
    match gradient.kind {
        GradientKind::Radial {
            circle,
            size: extent,
            x,
            y,
        } => {
            // SAFETY: an immutable Core Animation constant.
            g.setType(unsafe { kCAGradientLayerRadial });
            let (cx, cy) = (x.0 as CGFloat * w, y.0 as CGFloat * h);
            let (dx, dy) = match extent {
                GradientSize::ClosestSide => (cx.min(w - cx), cy.min(h - cy)),
                GradientSize::FarthestSide => (cx.max(w - cx), cy.max(h - cy)),
                GradientSize::ClosestCorner => {
                    let (a, b) = (cx.min(w - cx), cy.min(h - cy));
                    (a * std::f64::consts::SQRT_2, b * std::f64::consts::SQRT_2)
                }
                GradientSize::FarthestCorner => {
                    let (a, b) = (cx.max(w - cx), cy.max(h - cy));
                    (a * std::f64::consts::SQRT_2, b * std::f64::consts::SQRT_2)
                }
            };
            let (rx, ry) = if circle {
                let r = match extent {
                    GradientSize::ClosestSide => dx.min(dy),
                    GradientSize::FarthestSide => dx.max(dy),
                    _ => {
                        let (a, b) = (dx / std::f64::consts::SQRT_2, dy / std::f64::consts::SQRT_2);
                        (a * a + b * b).sqrt()
                    }
                };
                (r, r)
            } else {
                (dx, dy)
            };
            g.setStartPoint(CGPoint::new(cx / w, cy / h));
            g.setEndPoint(CGPoint::new((cx + rx) / w, (cy + ry) / h));
        }
        GradientKind::Conic { from, x, y } => {
            // SAFETY: an immutable Core Animation constant.
            g.setType(unsafe { kCAGradientLayerConic });
            let (x, y) = (x.0 as CGFloat, y.0 as CGFloat);
            g.setStartPoint(CGPoint::new(x, y));
            // Core Animation starts at the end point's direction, clockwise
            // from the right; CSS starts up.
            let a = from.0 as f64 - std::f64::consts::FRAC_PI_2;
            g.setEndPoint(CGPoint::new(x + a.cos() / 2.0, y + a.sin() / 2.0));
        }
        kind => {
            // SAFETY: an immutable Core Animation constant.
            g.setType(unsafe { kCAGradientLayerAxial });
            let box_size =
                nsh_core::Size::new(nsh_core::Points(w as f32), nsh_core::Points(h as f32));
            let angle = kind
                .linear_angle(box_size)
                .unwrap_or(nsh_core::geometry::deg(180.0));
            // CSS: the gradient line through the center at `angle` (0 up,
            // clockwise), long enough that corners get the end colors.
            let (s, c) = (angle.0 as f64).sin_cos();
            let len = (w * s).abs() + (h * c).abs();
            let (dx, dy) = (s * len / 2.0, -c * len / 2.0);
            let (cx, cy) = (w / 2.0, h / 2.0);
            g.setStartPoint(CGPoint::new((cx - dx) / w, (cy - dy) / h));
            g.setEndPoint(CGPoint::new((cx + dx) / w, (cy + dy) / h));
        }
    }
}
