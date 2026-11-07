//! Point-sized text decorations over the native paragraph's UTF-16 range geometry.

use nsh_core::{DecorationStyle, ResolvedTextStyle};
use ohos_drawing_binding::{Canvas, Path, PathEffect, Pen, TextStyle, Typography};

pub(crate) struct Decoration {
    start: u32,
    end: u32,
    style: ResolvedTextStyle,
    native: TextStyle,
}

impl Decoration {
    pub(crate) fn vertical_bounds(&self, paragraph: &Typography, scale: f32) -> Option<(f32, f32)> {
        let style_metrics = paragraph.font_metrics(&self.native);
        let mut bounds: Option<(f32, f32)> = None;
        for i in 0..paragraph.line_count().min(i32::MAX as usize) {
            let Some(line) = paragraph.line_metrics(i as i32) else {
                continue;
            };
            if (self.start as usize).max(line.start_index)
                >= (self.end as usize).min(line.end_index)
            {
                continue;
            }
            let metrics = style_metrics.unwrap_or(line.first_char_metrics);
            let baseline = (line.y + line.ascender) as f32;
            for (enabled, top, default_thickness) in [
                (
                    self.style.decoration.underline,
                    baseline
                        + self
                            .style
                            .underline_offset
                            .map_or(metrics.underline_position, |o| o.0),
                    metrics.underline_thickness,
                ),
                (
                    self.style.decoration.line_through,
                    baseline + metrics.strikeout_position,
                    metrics.strikeout_thickness,
                ),
            ] {
                if !enabled {
                    continue;
                }
                let Some(thickness) = self.thickness(default_thickness, scale) else {
                    continue;
                };
                let top = (top * scale).round() / scale;
                let extent = match self.style.decoration_style {
                    DecorationStyle::Double => (top - thickness, top + thickness * 2.0),
                    DecorationStyle::Wavy => {
                        let amplitude = (thickness * 1.5).max(2.0 / scale);
                        (top - amplitude, top + thickness + amplitude)
                    }
                    _ => (top, top + thickness),
                };
                bounds = Some(
                    bounds.map_or(extent, |(min, max)| (min.min(extent.0), max.max(extent.1))),
                );
            }
        }
        bounds
    }
    pub(crate) fn new(start: u32, end: u32, style: ResolvedTextStyle, native: TextStyle) -> Self {
        Self {
            start,
            end,
            style,
            native,
        }
    }
    pub(crate) fn draw(&self, canvas: &Canvas, paragraph: &Typography, scale: f32, width: f32) {
        let style_metrics = paragraph.font_metrics(&self.native);
        for i in 0..paragraph.line_count().min(i32::MAX as usize) {
            let Some(line) = paragraph.line_metrics(i as i32) else {
                continue;
            };
            let metrics = style_metrics.unwrap_or(line.first_char_metrics);
            let start = (self.start as usize).max(line.start_index);
            let end = (self.end as usize).min(line.end_index);
            if start >= end {
                continue;
            }
            let baseline = (line.y + line.ascender) as f32;
            for rect in paragraph.range_rectangles(start, end) {
                let range = (rect[0].max(0.0), rect[2].min(width));
                if self.style.decoration.underline {
                    let y = baseline
                        + self
                            .style
                            .underline_offset
                            .map_or(metrics.underline_position, |o| o.0);
                    self.line(canvas, range, y, metrics.underline_thickness, scale);
                }
                if self.style.decoration.line_through {
                    self.line(
                        canvas,
                        range,
                        baseline + metrics.strikeout_position,
                        metrics.strikeout_thickness,
                        scale,
                    );
                }
            }
        }
    }
    fn line(
        &self,
        canvas: &Canvas,
        range: (f32, f32),
        top: f32,
        default_thickness: f32,
        scale: f32,
    ) {
        let px = 1.0 / scale;
        let (left, right) = range;
        if right <= left {
            return;
        }
        let Some(thickness) = self.thickness(default_thickness, scale) else {
            return;
        };
        let mid = (top / px).round() * px + thickness * 0.5;
        let mut pen = Pen::new();
        pen.set_anti_alias(true);
        pen.set_width(thickness);
        pen.set_color(crate::text::typography::argb(
            self.style.decoration_color.unwrap_or(self.style.color),
        ));
        let dash = match self.style.decoration_style {
            DecorationStyle::Dashed => PathEffect::dash(&[thickness * 3.0, thickness * 2.0], 0.0),
            DecorationStyle::Dotted => PathEffect::dash(&[thickness, thickness], 0.0),
            _ => None,
        };
        pen.set_path_effect(dash.as_ref());
        canvas.attach_pen(&pen);
        let mut path = Path::new();
        if self.style.decoration_style == DecorationStyle::Double {
            for y in [mid - thickness, mid + thickness] {
                path.move_to(left, y);
                path.line_to(right, y);
            }
        } else if self.style.decoration_style == DecorationStyle::Wavy {
            let amplitude = (thickness * 1.5).max(px * 2.0);
            let mut x = left;
            let mut up = true;
            path.move_to(x, mid);
            while x < right {
                let next = (x + amplitude * 2.0).min(right);
                let peak = mid + if up { -amplitude } else { amplitude };
                // A quadratic wave represented as an equivalent cubic.
                let center = (x + next) * 0.5;
                path.bezier_curve_to(
                    x + (center - x) * 2.0 / 3.0,
                    mid + (peak - mid) * 2.0 / 3.0,
                    next + (center - next) * 2.0 / 3.0,
                    mid + (peak - mid) * 2.0 / 3.0,
                    next,
                    mid,
                );
                x = next;
                up = !up;
            }
        } else {
            path.move_to(left, mid);
            path.line_to(right, mid);
        }
        canvas.draw_path(&path);
        canvas.detach_pen();
    }
    fn thickness(&self, default: f32, scale: f32) -> Option<f32> {
        if self.style.decoration_thickness.is_some_and(|v| v.0 <= 0.0) {
            return None;
        }
        Some(
            (self.style.decoration_thickness.map_or(default, |v| v.0) * scale)
                .round()
                .max(1.0)
                / scale,
        )
    }
}
