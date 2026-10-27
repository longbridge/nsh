//! Cell projection of view transforms. Boxes use the axis-aligned extent;
//! glyphs stay upright in the terminal font. Canvas paths keep their affine
//! transform on the finer braille grid.

use nsh_core::{Bounds, Matrix, Point, Points, Size, TransformKind};

use crate::view::NshView;

#[derive(Clone, Copy)]
pub(crate) struct Projection {
    pub(crate) matrix: Matrix,
}

impl Projection {
    pub(crate) const IDENTITY: Self = Self {
        matrix: Matrix::IDENTITY,
    };

    pub(crate) fn translated(mut self, offset: Point<Points>) -> Self {
        let at = self.point(offset);
        self.matrix.0[12] = f64::from(at.x.0);
        self.matrix.0[13] = f64::from(at.y.0);
        self
    }

    pub(crate) fn view(self, view: &NshView) -> Self {
        let parent = self.translated(view.frame.origin);
        if view.style.transform.ops().is_empty() {
            return parent;
        }
        let mut transform = Matrix::IDENTITY;
        for (index, step) in view.style.transform.ops().iter().enumerate() {
            if matches!(
                step.kind,
                TransformKind::RotateX | TransformKind::RotateY | TransformKind::Perspective
            ) {
                continue;
            }
            let value = view.animated.transform[index].unwrap_or(step.value);
            transform = Matrix::of(step.kind, f64::from(value)).mul(&transform);
        }
        let (offset, _) = view.style.transform_origin.offset(view.frame.size);
        let pivot = offset + Point::new(view.frame.size.width / 2.0, view.frame.size.height / 2.0);
        let transform = Self { matrix: transform }.translated(Point::new(-pivot.x, -pivot.y));
        let mut transform = transform.matrix;
        transform.0[12] += f64::from(pivot.x.0);
        transform.0[13] += f64::from(pivot.y.0);
        Self {
            matrix: transform.mul(&parent.matrix),
        }
    }

    pub(crate) fn point(self, point: Point<Points>) -> Point<Points> {
        let m = self.matrix.0;
        let (x, y) = (f64::from(point.x.0), f64::from(point.y.0));
        Point::new(
            Points((m[0] * x + m[4] * y + m[12]) as f32),
            Points((m[1] * x + m[5] * y + m[13]) as f32),
        )
    }

    pub(crate) fn bounds(self, bounds: Bounds<Points>) -> Bounds<Points> {
        let start = self.point(bounds.origin);
        let mut min = start;
        let mut max = start;
        for offset in [
            Point::new(bounds.size.width, Points::ZERO),
            Point::new(Points::ZERO, bounds.size.height),
            Point::new(bounds.size.width, bounds.size.height),
        ] {
            let point = self.point(bounds.origin + offset);
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
        }
        Bounds::new(min, Size::new(max.x - min.x, max.y - min.y))
    }

    pub(crate) fn snapped(mut self, delta: Point<Points>) -> Self {
        self.matrix.0[12] += f64::from(delta.x.0);
        self.matrix.0[13] += f64::from(delta.y.0);
        self
    }
}
