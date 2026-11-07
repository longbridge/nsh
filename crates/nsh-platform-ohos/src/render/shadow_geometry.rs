//! Shadow routing and conservative physical paint bounds, independent of ArkUI.
use nsh_core::{BoxShadow, ViewStyle};

/// The native shadow caster is the border box, without CSS spread or custom corners.
pub(crate) fn native_box_shadow(style: &ViewStyle) -> Option<BoxShadow> {
    if style.continuous_corners || style.background_color.0 & 0xff != 0xff {
        return None;
    }
    let mut outer = style
        .effects
        .as_ref()?
        .box_shadows
        .iter()
        .filter(|s| !s.inset && s.is_visible());
    let shadow = *outer.next()?;
    // Tiny CSS blurs cannot be represented by ArkUI's radius-to-sigma conversion.
    (outer.next().is_none() && shadow.spread.0 == 0.0 && shadow.blur.0 >= 2.0).then_some(shadow)
}

/// ArkUI/Rosen converts a Skia radius to sigma with radius / sqrt(3) + 0.5.
pub(crate) fn native_radius(sigma: f32) -> f32 {
    if sigma <= 0.0 {
        0.0
    } else {
        ((sigma - 0.5) * 3.0_f32.sqrt()).max(0.0)
    }
}

/// Physical pixels, rounded outwards so all blurred ink belongs to this node.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PaintBounds {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}
impl PaintBounds {
    pub(crate) fn shadows(style: &ViewStyle, width: f32, height: f32, scale: f32) -> Self {
        if width <= 0.0 || height <= 0.0 {
            return Self::default();
        }
        // Match the snapped border box used by Decoration::shadows.
        let sx = (width * scale).round().max(1.0) / width;
        let sy = (height * scale).round().max(1.0) / height;
        let mut rect = [
            0.0_f32,
            0.0,
            (width * scale).round(),
            (height * scale).round(),
        ];
        let mut visible = false;
        for s in style
            .effects
            .iter()
            .flat_map(|e| &e.box_shadows)
            .filter(|s| !s.inset && s.is_visible())
        {
            visible = true;
            // Three sigma plus an antialias pixel; signed spread shrinks the caster.
            let reach = s.spread.0 + s.blur.0.max(0.0) * 1.5;
            rect[0] = rect[0].min((s.offset_x.0 - reach) * sx - 1.0);
            rect[1] = rect[1].min((s.offset_y.0 - reach) * sy - 1.0);
            rect[2] = rect[2].max((width + s.offset_x.0 + reach) * sx + 1.0);
            rect[3] = rect[3].max((height + s.offset_y.0 + reach) * sy + 1.0);
        }
        if !visible {
            return Self::default();
        }
        let x = rect[0].floor() as i32;
        let y = rect[1].floor() as i32;
        Self {
            x,
            y,
            width: (rect[2].ceil() as i32).saturating_sub(x),
            height: (rect[3].ceil() as i32).saturating_sub(y),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nsh_core::{Color, Effects, Points};

    use super::*;

    fn style(layers: Vec<BoxShadow>) -> ViewStyle {
        ViewStyle {
            background_color: Color(0xffffffff),
            effects: Some(Arc::new(Effects {
                box_shadows: layers,
                ..Default::default()
            })),
            ..Default::default()
        }
    }
    fn shadow() -> BoxShadow {
        BoxShadow {
            color: Color::BLACK,
            blur: Points(8.0),
            ..Default::default()
        }
    }
    #[test]
    fn native_route_preserves_css_features() {
        let mut s = style(vec![shadow()]);
        assert!(native_box_shadow(&s).is_some());
        Arc::make_mut(s.effects.as_mut().unwrap())
            .box_shadows
            .push(BoxShadow {
                inset: true,
                ..shadow()
            });
        assert!(native_box_shadow(&s).is_some());
        s.continuous_corners = true;
        assert!(native_box_shadow(&s).is_none());
        s.continuous_corners = false;
        s.background_color = Color::TRANSPARENT;
        assert!(native_box_shadow(&s).is_none());
        for layers in [
            vec![shadow(), shadow()],
            vec![BoxShadow {
                spread: Points(-1.0),
                ..shadow()
            }],
            vec![BoxShadow {
                blur: Points(0.0),
                ..shadow()
            }],
        ] {
            assert!(native_box_shadow(&style(layers)).is_none());
        }
    }
    #[test]
    fn bounds_cover_offsets_spread_blur_and_density() {
        let s = style(vec![BoxShadow {
            offset_x: Points(-9.0),
            offset_y: Points(7.0),
            spread: Points(2.0),
            ..shadow()
        }]);
        assert_eq!(
            PaintBounds::shadows(&s, 56.0, 56.0, 3.25),
            PaintBounds {
                x: -76,
                y: -24,
                width: 276,
                height: 276
            }
        );
        assert_eq!(
            PaintBounds::shadows(&s, 0.0, 56.0, 3.25),
            PaintBounds::default()
        );
        let inset = style(vec![BoxShadow {
            inset: true,
            ..shadow()
        }]);
        assert_eq!(
            PaintBounds::shadows(&inset, 56.0, 56.0, 3.25),
            PaintBounds::default()
        );
    }
    #[test]
    fn native_blur_matches_css_sigma_at_each_density() {
        for scale in [1.0, 2.0, 3.25] {
            let sigma = 8.0 * 0.5 * scale;
            assert!((native_radius(sigma) / 3.0_f32.sqrt() + 0.5 - sigma).abs() < 0.00001);
        }
        assert_eq!(native_radius(0.0), 0.0);
    }
}
