//! Native-plane inversion and surface-relative window geometry.

use nsh_core::{Bounds, Point, Points, Size as CoreSize};

/// Logical rectangle in vp (points).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl From<Bounds<Points>> for Rect {
    fn from(b: Bounds<Points>) -> Self {
        Self {
            x: b.origin.x.0,
            y: b.origin.y.0,
            width: b.size.width.0,
            height: b.size.height.0,
        }
    }
}

impl From<Rect> for Bounds<Points> {
    fn from(r: Rect) -> Self {
        Bounds::new(
            Point::new(Points(r.x), Points(r.y)),
            CoreSize::new(Points(r.width), Points(r.height)),
        )
    }
}

/// Logical size in vp (points).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Size {
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl Size {
    pub(crate) const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

impl From<CoreSize<Points>> for Size {
    fn from(s: CoreSize<Points>) -> Self {
        Self {
            width: s.width.0,
            height: s.height.0,
        }
    }
}

impl From<Size> for CoreSize<Points> {
    fn from(s: Size) -> Self {
        Self::new(Points(s.width), Points(s.height))
    }
}

/// One window-space rectangle in physical pixels.
#[derive(Clone, Copy, Default)]
pub(crate) struct WindowRect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl WindowRect {
    pub(crate) fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (x < right && y < bottom).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

/// Top/right/bottom/left exclusions from the visible visual avoid-area types.
/// Areas overlap (for example a cutout inside a status bar), so reserve their
/// deepest extent from each surface edge, rather than adding their sizes.
pub(crate) fn visual_insets(
    viewport: WindowRect,
    areas: impl IntoIterator<Item = [WindowRect; 4]>,
) -> [f64; 4] {
    let mut insets = [0.0_f64; 4];
    for area in areas {
        for (edge, rect) in area.into_iter().enumerate() {
            if let Some(overlap) = viewport.intersection(rect) {
                let depth = match edge {
                    0 => overlap.y + overlap.height - viewport.y,
                    1 => viewport.x + viewport.width - overlap.x,
                    2 => viewport.y + viewport.height - overlap.y,
                    _ => overlap.x + overlap.width - viewport.x,
                };
                insets[edge] = insets[edge].max(depth);
            }
        }
    }
    insets
}

/// Maps a surface point onto the local z=0 plane of a row-vector transform.
/// Inverting a 4D point with screen z=0 loses depth after perspective division.
pub(crate) fn local_point(matrix: &[f64; 16], point: (f32, f32)) -> Option<(f32, f32)> {
    let m = matrix;
    let (u, v) = (point.0 as f64, point.1 as f64);
    let (a, b, c) = (m[0] - u * m[3], m[4] - u * m[7], u * m[15] - m[12]);
    let (d, e, f) = (m[1] - v * m[3], m[5] - v * m[7], v * m[15] - m[13]);
    let determinant = a * e - b * d;
    if !determinant.is_finite()
        || determinant.abs() <= 1e-12 * (a * e).abs().max((b * d).abs()).max(1.0)
    {
        return None;
    }
    let (x, y) = ((c * e - b * f) / determinant, (a * f - c * d) / determinant);
    let w = x * m[3] + y * m[7] + m[15];
    (x.is_finite()
        && y.is_finite()
        && x.abs() <= f32::MAX as f64
        && y.abs() <= f32::MAX as f64
        && w.is_finite()
        && w > 0.0)
        .then_some((x as f32, y as f32))
}

#[cfg(test)]
mod tests {
    use super::{WindowRect, local_point, visual_insets};

    fn rect(x: f64, y: f64, width: f64, height: f64) -> WindowRect {
        WindowRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn visual_safe_areas_merge_status_cutout_and_navigation_without_double_padding() {
        let empty = WindowRect::default();
        let status = [rect(0.0, 0.0, 1080.0, 72.0), empty, empty, empty];
        let cutout = [rect(420.0, 12.0, 240.0, 78.0), empty, empty, empty];
        let navigation = [empty, empty, rect(400.0, 2352.0, 280.0, 16.0), empty];
        let pixels = visual_insets(rect(0.0, 0.0, 1080.0, 2400.0), [status, cutout, navigation]);
        assert_eq!(pixels, [90.0, 0.0, 48.0, 0.0]);
        // Pixel geometry is converted once using the surface's actual density.
        assert_eq!(pixels.map(|edge| edge / 3.0), [30.0, 0.0, 16.0, 0.0]);
    }

    #[test]
    fn embedded_or_already_inset_surfaces_consume_only_their_own_overlap() {
        let empty = WindowRect::default();
        let visual = [
            rect(420.0, 0.0, 240.0, 90.0),
            empty,
            rect(400.0, 2352.0, 280.0, 16.0),
            empty,
        ];
        assert_eq!(
            visual_insets(rect(0.0, 120.0, 1080.0, 2200.0), [visual]),
            [0.0; 4]
        );
        assert_eq!(
            visual_insets(rect(0.0, 40.0, 1080.0, 2340.0), [visual]),
            [50.0, 0.0, 28.0, 0.0]
        );
        assert_eq!(
            visual_insets(rect(0.0, 0.0, 300.0, 2400.0), [visual]),
            [0.0; 4]
        );
    }

    #[test]
    fn landscape_visual_areas_preserve_both_side_exclusions() {
        let empty = WindowRect::default();
        let system = [empty, empty, empty, rect(0.0, 0.0, 24.0, 1080.0)];
        let cutout = [empty, empty, empty, rect(0.0, 400.0, 60.0, 280.0)];
        let navigation = [empty, rect(2352.0, 400.0, 16.0, 280.0), empty, empty];
        assert_eq!(
            visual_insets(
                rect(24.0, 0.0, 2376.0, 1080.0),
                [system, cutout, navigation]
            ),
            [0.0, 48.0, 0.0, 36.0]
        );
    }

    #[test]
    fn visual_insets_do_not_accumulate_keyboard_height_or_empty_rectangles() {
        let empty = WindowRect::default();
        let viewport = rect(0.0, 0.0, 1080.0, 2400.0);
        let navigation = [empty, empty, rect(400.0, 2352.0, 280.0, 16.0), empty];
        assert_eq!(visual_insets(viewport, [navigation]), [0.0, 0.0, 48.0, 0.0]);
        assert_eq!(
            viewport
                .intersection(rect(0.0, 1600.0, 1080.0, 800.0))
                .unwrap()
                .height,
            800.0
        );
        assert_eq!(visual_insets(viewport, [[empty; 4]]), [0.0; 4]);
        assert!(
            viewport
                .intersection(rect(1080.0, 0.0, 40.0, 2400.0))
                .is_none()
        );
    }

    #[test]
    fn rotated_scaled_and_translated_plane() {
        let m = [
            0.0, 2.0, 0.0, 0.0, -3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 120.0, 40.0, 0.0, 1.0,
        ];
        assert_eq!(local_point(&m, (60.0, 60.0)), Some((10.0, 20.0)));
    }

    #[test]
    fn projected_plane_round_trips_without_assuming_screen_depth() {
        // A rotated plane with perspective and an offset origin.
        let m = [
            0.8, 0.0, -0.6, 0.003, 0.0, 1.0, 0.0, 0.0, 0.6, 0.0, 0.8, -0.004, 33.0, 17.0, 8.0, 0.96,
        ];
        let (x, y) = (41.0, 29.0);
        let w = x * m[3] + y * m[7] + m[15];
        let p = (
            ((x * m[0] + y * m[4] + m[12]) / w) as f32,
            ((x * m[1] + y * m[5] + m[13]) / w) as f32,
        );
        let actual = local_point(&m, p).unwrap();
        assert!((actual.0 - x as f32).abs() < 0.0001);
        assert!((actual.1 - y as f32).abs() < 0.0001);
    }

    #[test]
    fn singular_nonfinite_and_behind_camera_are_not_hit() {
        assert_eq!(local_point(&[0.0; 16], (1.0, 2.0)), None);
        assert_eq!(local_point(&[f64::NAN; 16], (1.0, 2.0)), None);
        let mut m = [0.0; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[15] = -1.0;
        assert_eq!(local_point(&m, (1.0, 2.0)), None);
    }
}
