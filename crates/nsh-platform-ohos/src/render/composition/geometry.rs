//! Finite capture bounds in surface units, including projected native overflow.
use nsh_core::{Filter, Matrix, ViewStyle};

pub(crate) fn blur_sigma(radius: f32, scale: f32) -> f32 {
    radius.max(0.0) * scale * 0.5
}

pub(crate) fn requires_scene(style: &ViewStyle) -> bool {
    style.blend_mode == nsh_core::BlendMode::PlusDarker
        || style.effects.as_ref().is_some_and(|effects| {
            effects.backdrop_filters.len() > 1
                || effects
                    .backdrop_filters
                    .iter()
                    .any(|filter| !matches!(filter, Filter::Blur(_)))
        })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds(pub(crate) [f32; 4]);
impl Bounds {
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self([0.0, 0.0, width, height])
    }
    pub(crate) fn union(&mut self, other: Self) {
        self.0 = [
            self.0[0].min(other.0[0]),
            self.0[1].min(other.0[1]),
            self.0[2].max(other.0[2]),
            self.0[3].max(other.0[3]),
        ];
    }
    fn extend(&mut self, dx: f32, dy: f32, pad: f32) {
        let [a, b, c, d] = self.0;
        self.union(Self([
            a + dx - pad,
            b + dy - pad,
            c + dx + pad,
            d + dy + pad,
        ]));
    }
    pub(crate) fn own(mut self, style: &ViewStyle) -> Self {
        let border = self;
        if style.outline.is_visible() {
            self.extend(
                0.0,
                0.0,
                (style.outline.width.0 + style.outline.offset.0).max(0.0),
            );
        }
        if let Some(effects) = style.effects.as_ref() {
            for shadow in effects
                .box_shadows
                .iter()
                .filter(|s| !s.inset && s.is_visible())
            {
                let mut bounds = border;
                bounds.extend(
                    shadow.offset_x.0,
                    shadow.offset_y.0,
                    shadow.spread.0 + shadow.blur.0.max(0.0) * 1.5,
                );
                self.union(bounds);
            }
        }
        self
    }
    pub(crate) fn filters(mut self, filters: &[Filter]) -> Self {
        for filter in filters {
            match *filter {
                Filter::Blur(radius) => self.extend(0.0, 0.0, blur_sigma(radius.0, 1.0) * 3.0),
                Filter::DropShadow {
                    offset_x,
                    offset_y,
                    blur,
                    ..
                } => self.extend(offset_x.0, offset_y.0, blur_sigma(blur.0, 1.0) * 3.0),
                _ => {}
            }
        }
        self
    }
    pub(crate) fn transformed(self, matrix: Matrix) -> Option<Self> {
        let m = matrix.0;
        let [a, b, c, d] = self.0;
        let mut result = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for (x, y) in [(a, b), (c, b), (c, d), (a, d)] {
            let (x, y) = (x as f64, y as f64);
            let w = x * m[3] + y * m[7] + m[15];
            if !w.is_finite() || w <= 1e-6 {
                return None;
            }
            let (x, y) = (
                (x * m[0] + y * m[4] + m[12]) / w,
                (x * m[1] + y * m[5] + m[13]) / w,
            );
            if !x.is_finite() || !y.is_finite() || x.abs() > 1e7 || y.abs() > 1e7 {
                return None;
            }
            result[0] = result[0].min(x as f32);
            result[1] = result[1].min(y as f32);
            result[2] = result[2].max(x as f32);
            result[3] = result[3].max(y as f32);
        }
        Some(Self(result))
    }
    pub(crate) fn insets(self, width: f32, height: f32, scale: f32) -> [u32; 4] {
        [
            (-self.0[0]).max(0.0),
            (-self.0[1]).max(0.0),
            (self.0[2] - width).max(0.0),
            (self.0[3] - height).max(0.0),
        ]
        .map(|v| (v * scale).ceil() as u32)
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::{Color, Points, TransformKind};

    use super::*;
    #[test]
    fn scene_requirements_follow_the_current_style() {
        let mut style = ViewStyle {
            blend_mode: nsh_core::BlendMode::PlusDarker,
            ..ViewStyle::default()
        };
        assert!(requires_scene(&style));
        style.blend_mode = nsh_core::BlendMode::Normal;
        assert!(!requires_scene(&style));
        let mut effects = nsh_core::Effects {
            backdrop_filters: vec![Filter::Blur(Points(4.0)), Filter::Blur(Points(8.0))],
            ..Default::default()
        };
        style.effects = Some(std::sync::Arc::new(effects.clone()));
        assert!(requires_scene(&style));
        effects.backdrop_filters.pop();
        style.effects = Some(std::sync::Arc::new(effects));
        assert!(!requires_scene(&style));
    }
    #[test]
    fn css_blur_radius_becomes_half_a_physical_sigma() {
        assert_eq!(blur_sigma(8.0, 3.0), 12.0);
        assert_eq!(blur_sigma(-8.0, 3.0), 0.0);
        assert_eq!(
            Bounds::new(10.0, 10.0)
                .filters(&[Filter::Blur(Points(8.0))])
                .0,
            [-12.0, -12.0, 22.0, 22.0]
        );
    }
    #[test]
    fn nested_shadow_overflow_and_negative_translation() {
        let child = Bounds::new(40.0, 20.0).filters(&[Filter::DropShadow {
            offset_x: Points(8.0),
            offset_y: Points(4.0),
            blur: Points(2.0),
            color: Color::BLACK,
        }]);
        let child = child
            .transformed(Matrix::of(TransformKind::TranslateX, -15.0))
            .unwrap();
        let mut parent = Bounds::new(30.0, 10.0);
        parent.union(child);
        assert_eq!(parent.insets(30.0, 10.0, 2.0), [30, 0, 12, 34]);
    }
    #[test]
    fn perspective_crossing_camera_has_no_finite_capture() {
        let mut matrix = Matrix::IDENTITY;
        matrix.0[3] = -0.1;
        assert!(Bounds::new(20.0, 10.0).transformed(matrix).is_none());
    }
}
