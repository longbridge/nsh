use nsh_core::Points;

/// The point dimensions of one terminal Cell (8 × 16 by default).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct CellSize {
    width: Points,
    height: Points,
}

impl Default for CellSize {
    fn default() -> Self {
        Self {
            width: Points(8.0),
            height: Points(16.0),
        }
    }
}

impl CellSize {
    /// Creates finite, positive Cell dimensions.
    pub fn new(width: Points, height: Points) -> Result<Self, String> {
        if !width.0.is_finite()
            || !height.0.is_finite()
            || width <= Points::ZERO
            || height <= Points::ZERO
        {
            return Err("Cell dimensions must be finite and positive".into());
        }
        Ok(Self { width, height })
    }

    /// Width in points.
    pub fn width(self) -> Points {
        self.width
    }

    /// Height in points.
    pub fn height(self) -> Points {
        self.height
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CellBounds {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) right: i32,
    pub(crate) bottom: i32,
}

impl CellBounds {
    pub(crate) fn new(
        bounds: nsh_core::Bounds<Points>,
        cells: CellSize,
        area: ratatui_core::layout::Rect,
    ) -> Self {
        Self {
            left: i32::from(area.x) + (bounds.origin.x / cells.width()).round() as i32,
            top: i32::from(area.y) + (bounds.origin.y / cells.height()).round() as i32,
            right: i32::from(area.x)
                + ((bounds.origin.x + bounds.size.width) / cells.width()).round() as i32,
            bottom: i32::from(area.y)
                + ((bounds.origin.y + bounds.size.height) / cells.height()).round() as i32,
        }
    }

    pub(crate) fn clipped(self, clip: ratatui_core::layout::Rect) -> ratatui_core::layout::Rect {
        let left = self.left.clamp(i32::from(clip.x), i32::from(clip.right())) as u16;
        let top = self.top.clamp(i32::from(clip.y), i32::from(clip.bottom())) as u16;
        let right = self.right.clamp(i32::from(left), i32::from(clip.right())) as u16;
        let bottom = self.bottom.clamp(i32::from(top), i32::from(clip.bottom())) as u16;
        ratatui_core::layout::Rect::new(left, top, right - left, bottom - top)
    }
}
