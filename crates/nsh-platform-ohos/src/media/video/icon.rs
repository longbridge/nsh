//! The system controls' glyphs, drawn into their nodes as
//! HarmonyOS's video controls draw it (measured from ArkTS `Video`'s on a
//! phone): a white ring and, inside it, a triangle while paused or two bars
//! while playing, and diagonal arrows for fullscreen.

use ohos_drawing_binding::{Brush, Canvas, LineCap, LineJoin, Path, Pen, Point};

/// The ring's diameter and stroke, shares of the node's shorter side (30 vp
/// and 2.5 vp in a 40 vp node, as the system's).
const RING: f32 = 0.75;
const STROKE: f32 = 0.0625;

pub(super) fn draw(canvas: &Canvas, (width, height): (f32, f32), playing: bool) {
    let side = width.min(height);
    let (cx, cy) = (width / 2.0, height / 2.0);
    let radius = side * RING / 2.0;
    let stroke = side * STROKE;

    let mut pen = Pen::new();
    pen.set_anti_alias(true);
    pen.set_color(0xffff_ffff);
    pen.set_width(stroke);
    canvas.attach_pen(&pen);
    canvas.draw_circle(&Point::new(cx, cy), radius - stroke / 2.0);
    canvas.detach_pen();

    // The glyph inside: about two fifths of the ring.
    let g = radius * 0.8;
    let mut path = Path::new();
    if playing {
        let (bar, gap) = (g * 0.22, g * 0.3);
        for left in [cx - gap / 2.0 - bar, cx + gap / 2.0] {
            path.move_to(left, cy - g / 2.0);
            path.line_to(left + bar, cy - g / 2.0);
            path.line_to(left + bar, cy + g / 2.0);
            path.line_to(left, cy + g / 2.0);
            path.close();
        }
    } else {
        // Optically centred: a triangle's mass sits left of its box's centre.
        let left = cx - g * 0.36;
        path.move_to(left, cy - g / 2.0);
        path.line_to(left + g * 0.86, cy);
        path.line_to(left, cy + g / 2.0);
        path.close();
    }
    let mut brush = Brush::new();
    brush.set_anti_alias(true);
    brush.set_color(0xffff_ffff);
    canvas.attach_brush(&brush);
    canvas.draw_path(&path);
    canvas.detach_brush();
}

/// Diagonal arrows occupy 22 vp of the system's 40 vp target.
pub(super) fn draw_fullscreen(canvas: &Canvas, (width, height): (f32, f32), fullscreen: bool) {
    let side = width.min(height);
    let (cx, cy) = (width / 2.0, height / 2.0);
    let outer = side * 0.275;
    let inner = side * 0.075;
    let head = side * 0.15;
    let mut path = Path::new();
    for direction in [-1.0, 1.0] {
        let (tip, tail, wing) = if fullscreen {
            (inner, outer, inner + head)
        } else {
            (outer, inner, outer - head)
        };
        path.move_to(cx + direction * tail, cy + direction * tail);
        path.line_to(cx + direction * tip, cy + direction * tip);
        path.move_to(cx + direction * wing, cy + direction * tip);
        path.line_to(cx + direction * tip, cy + direction * tip);
        path.line_to(cx + direction * tip, cy + direction * wing);
    }
    let mut pen = Pen::new();
    pen.set_anti_alias(true);
    pen.set_color(0xffff_ffff);
    pen.set_width(side * STROKE);
    pen.set_cap(LineCap::RoundCap);
    pen.set_join(LineJoin::RoundJoin);
    canvas.attach_pen(&pen);
    canvas.draw_path(&path);
    canvas.detach_pen();
}

/// `ms` as the system controls show it: `mm:ss`, `hh:mm:ss` from an hour.
pub(super) fn time(ms: i32) -> String {
    let s = ms.max(0) / 1000;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
