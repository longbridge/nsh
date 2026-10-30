//! Canvas 2D commands recorded directly into ArkUI's native drawing context.
//! Clear/composite drawings use a native isolated layer, keeping their destination
//! separate from the host background and siblings.
use std::{
    cell::{Cell, RefCell},
    rc::Weak,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use nsh_core::{
    Color, FontVariant,
    animation::AnimationValue,
    canvas::{
        self, CanvasFont, CanvasOp as Op, CanvasTextAlign, CanvasTextBaseline, Composite, Drawing,
        FillRule, Num, Paint, PathOp,
    },
};
use ohos_arkui_binding::types::advanced::NodeDirtyFlag;
use ohos_drawing_binding::{
    Bitmap, BlendMode, Brush, Canvas, ClipOperation, FilterMode, FontCollection, FontSlant,
    FontStyle, FontWeight, FontWidth, Matrix, MipmapMode, Path, PathEffect, PathFillType, Pen,
    Rect, SamplingOptions, ShaderEffect, ShadowLayer, TextStyle, TileMode, TypographyBuilder,
    TypographyStyle,
};

use crate::{
    render::node::{Error, Node, Result},
    text::typography::argb,
};

const BUDGET: usize = 96 << 20;
struct Image {
    uri: Arc<str>,
    bitmap: Option<Bitmap>,
    job: Arc<AtomicBool>,
}
pub(crate) struct CanvasView {
    owner: Weak<Node>,
    drawing: RefCell<Arc<Drawing>>,
    slots: RefCell<Vec<AnimationValue>>,
    images: RefCell<Vec<Image>>,
    generation: Cell<u64>,
    scheduled: Cell<bool>,
    isolated: Cell<bool>,
    fonts: RefCell<FontCollection>,
    budget: std::rc::Rc<Cell<usize>>,
    image_bytes: Cell<usize>,
}
impl CanvasView {
    pub(crate) fn new(owner: Weak<Node>, budget: std::rc::Rc<Cell<usize>>) -> Self {
        Self {
            owner,
            drawing: RefCell::new(Arc::default()),
            slots: RefCell::default(),
            images: RefCell::default(),
            generation: Cell::new(0),
            scheduled: Cell::new(false),
            isolated: Cell::new(false),
            fonts: RefCell::new(FontCollection::shared()),
            budget,
            image_bytes: Cell::new(0),
        }
    }
    pub(crate) fn set(&self, drawing: Arc<Drawing>) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        let generation = self
            .generation
            .get()
            .checked_add(1)
            .ok_or(Error::Unsupported("exhausted canvas generation"))?;
        self.generation.set(generation);
        let mut images = self.images.borrow_mut();
        for image in images.iter() {
            image.job.store(true, Ordering::Relaxed);
        }
        let mut next = Vec::new();
        for image in drawing.images() {
            if next.iter().any(|i: &Image| i.uri == image.uri) {
                continue;
            }
            let bitmap = images
                .iter_mut()
                .find(|i| i.uri == image.uri)
                .and_then(|i| i.bitmap.take());
            let job = if bitmap.is_none() {
                owner.images.load(
                    crate::media::image::Target::Canvas {
                        id: owner.id,
                        generation,
                        index: next.len(),
                    },
                    image.uri.to_string(),
                    None,
                )
            } else {
                Arc::new(AtomicBool::new(false))
            };
            next.push(Image {
                uri: image.uri.clone(),
                bitmap,
                job,
            });
        }
        *images = next;
        let bytes = images
            .iter()
            .filter_map(|i| i.bitmap.as_ref())
            .map(|b| b.width() as usize * b.height() as usize * 4)
            .sum();
        self.budget
            .set(self.budget.get() - self.image_bytes.replace(bytes) + bytes);
        self.slots
            .borrow_mut()
            .resize(drawing.slots.len(), AnimationValue::Number(0.0));
        // Destination-dependent operations must only see this canvas's pixels.
        let isolated = drawing.ops.iter().any(|op| {
            matches!(op, Op::ClearRect(_))
                || matches!(op, Op::Composite(mode) if *mode != Composite::SourceOver)
        });
        self.isolated.set(isolated);
        *self.drawing.borrow_mut() = drawing;
        self.invalidate()
    }
    pub(crate) fn ready(
        &self,
        generation: u64,
        index: usize,
        data: std::result::Result<Arc<crate::media::image::Data>, String>,
    ) -> Result<()> {
        if generation != self.generation.get() {
            return Ok(());
        }
        let mut images = self.images.borrow_mut();
        let Some(image) = images.get_mut(index) else {
            return Ok(());
        };
        let decoded = data.and_then(|data| {
            if self
                .budget
                .get()
                .saturating_add(data.decoded_bytes().saturating_mul(2))
                > BUDGET
            {
                return Err("canvas image memory budget exhausted".into());
            }
            data.bitmap()
        });
        match decoded {
            Ok(bitmap) => {
                let bytes = bitmap.width() as usize * bitmap.height() as usize * 4;
                self.budget.set(self.budget.get() + bytes);
                self.image_bytes.set(self.image_bytes.get() + bytes);
                image.bitmap = Some(bitmap);
            }
            Err(error) => log::warn!("OHOS canvas image {}: {error}", image.uri),
        }
        drop(images);
        self.invalidate()
    }
    pub(crate) fn slot(&self, index: u16, value: AnimationValue) -> Result<()> {
        let mut slots = self.slots.borrow_mut();
        if let Some(slot) = slots.get_mut(index as usize)
            && *slot != value
        {
            *slot = value;
            drop(slots);
            self.invalidate()?;
        }
        Ok(())
    }
    pub(crate) fn invalidate(&self) -> Result<()> {
        if self.scheduled.get() {
            return Ok(());
        }
        if let Some(owner) = self.owner.upgrade() {
            owner
                .native
                .borrow()
                .mark_dirty(NodeDirtyFlag::NeedRender)?;
            self.scheduled.set(true);
        }
        Ok(())
    }
    pub(crate) fn draw(&self, target: &Canvas) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        self.scheduled.set(false);
        let frame = owner.frame.get();
        let bounds = Rect::new(
            0.0,
            0.0,
            frame.width * owner.scale,
            frame.height * owner.scale,
        );
        target.save();
        target.clip_rect(&bounds, ClipOperation::Intersect, false);
        if self.isolated.get() {
            target.save_layer(Some(&bounds), None);
        }
        self.replay(target, &owner);
        if self.isolated.get() {
            target.restore();
        }
        target.restore();
        Ok(())
    }
    fn replay(&self, canvas: &Canvas, owner: &Node) {
        let mut replay = Replay {
            canvas,
            matrix: RefCell::new(Matrix::new()),
            scale: owner.scale,
            slots: &self.slots.borrow(),
            images: &self.images.borrow(),
            fonts: &mut self.fonts.borrow_mut(),
            rtl: owner.platform.get().rtl,
            state: State::default(),
            saved: Vec::new(),
            path: Geometry::default(),
        };
        replay.run(&self.drawing.borrow());
    }
}
impl Drop for CanvasView {
    fn drop(&mut self) {
        for image in self.images.get_mut() {
            image.job.store(true, Ordering::Relaxed);
        }
        self.budget.set(self.budget.get() - self.image_bytes.get());
    }
}
#[derive(Clone, Copy, PartialEq)]
struct Affine([f32; 6]);
impl Default for Affine {
    fn default() -> Self {
        Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    }
}
impl Affine {
    fn point(self, x: f32, y: f32) -> [f32; 2] {
        let [a, b, c, d, e, f] = self.0;
        [a * x + c * y + e, b * x + d * y + f]
    }
    fn mul(self, other: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = other.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }
    fn inverse(self) -> Option<Self> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        (det.abs() > 1e-12).then(|| {
            Self([
                d / det,
                -b / det,
                -c / det,
                a / det,
                (c * f - d * e) / det,
                (b * e - a * f) / det,
            ])
        })
    }
    fn native(self) -> Matrix {
        let [a, b, c, d, e, f] = self.0;
        Matrix::from_affine(a, b, c, d, e, f)
    }
}
#[derive(Clone)]
struct State {
    transform: Affine,
    fill: Paint,
    stroke: Paint,
    width: f32,
    cap: canvas::LineCap,
    join: canvas::LineJoin,
    miter: f32,
    dash: Arc<[f32]>,
    dash_offset: f32,
    alpha: f32,
    composite: Composite,
    shadow: Color,
    blur: f32,
    offset: (f32, f32),
    font: CanvasFont,
    numeric: FontVariant,
    align: CanvasTextAlign,
    baseline: CanvasTextBaseline,
}
impl Default for State {
    fn default() -> Self {
        Self {
            transform: Affine::default(),
            fill: Paint::Color(Color(0x000000ff)),
            stroke: Paint::Color(Color(0x000000ff)),
            width: 1.0,
            cap: canvas::LineCap::Butt,
            join: canvas::LineJoin::Miter,
            miter: 10.0,
            dash: Arc::default(),
            dash_offset: 0.0,
            alpha: 1.0,
            composite: Composite::SourceOver,
            shadow: Color(0),
            blur: 0.0,
            offset: (0.0, 0.0),
            font: CanvasFont::default(),
            numeric: FontVariant::default(),
            align: CanvasTextAlign::Start,
            baseline: CanvasTextBaseline::Alphabetic,
        }
    }
}
struct Replay<'a> {
    canvas: &'a Canvas,
    matrix: RefCell<Matrix>,
    scale: f32,
    slots: &'a [AnimationValue],
    images: &'a [Image],
    fonts: &'a mut FontCollection,
    rtl: bool,
    state: State,
    saved: Vec<State>,
    path: Geometry,
}
impl Replay<'_> {
    fn num(&self, n: Num) -> f32 {
        n.get(|i| {
            self.slots
                .get(i as usize)
                .copied()
                .unwrap_or(AnimationValue::Number(0.0))
                .number()
        })
    }
    fn save_transform(&self) {
        // ArkUI records these commands for replay under the node's transform.
        // An absolute set_matrix would replace that transform at replay time.
        self.canvas.save();
        if self.state.transform != Affine::default() {
            self.canvas.concat(&self.transform());
        }
    }
    fn transform(&self) -> std::cell::RefMut<'_, Matrix> {
        let [a, b, c, d, e, f] = self.state.transform.0;
        let mut matrix = self.matrix.borrow_mut();
        matrix.set_affine(a, b, c, d, e, f);
        matrix
    }
    fn run(&mut self, drawing: &Drawing) {
        self.canvas.save();
        self.canvas.scale(self.scale, self.scale);
        for op in &drawing.ops {
            match op {
                Op::Save => {
                    self.saved.push(self.state.clone());
                    self.canvas.save();
                }
                Op::Restore => {
                    if let Some(state) = self.saved.pop() {
                        self.state = state;
                        self.canvas.restore();
                    }
                }
                Op::Translate(x, y) => {
                    self.state.transform = self.state.transform.mul(Affine([
                        1.0,
                        0.0,
                        0.0,
                        1.0,
                        self.num(*x),
                        self.num(*y),
                    ]))
                }
                Op::Scale(x, y) => {
                    self.state.transform = self.state.transform.mul(Affine([
                        self.num(*x),
                        0.0,
                        0.0,
                        self.num(*y),
                        0.0,
                        0.0,
                    ]))
                }
                Op::Rotate(a) => {
                    let (s, c) = self.num(*a).sin_cos();
                    self.state.transform =
                        self.state.transform.mul(Affine([c, s, -s, c, 0.0, 0.0]));
                }
                Op::Transform(v) => {
                    self.state.transform = self.state.transform.mul(Affine(v.map(|n| self.num(n))))
                }
                Op::SetTransform(v) => self.state.transform = Affine(v.map(|n| self.num(n))),
                Op::BeginPath => {
                    self.path.path.reset();
                    self.path.current = None;
                    self.path.start = None;
                }
                Op::Path(op) => self.path.append(op, self.state.transform, self.slots),
                Op::Fill(path, rule) => {
                    let path = self.paint_path(path.as_deref());
                    self.paint_path_native(path, *rule, false);
                }
                Op::Stroke(path) => {
                    let path = self.paint_path(path.as_deref());
                    self.paint_path_native(path, FillRule::NonZero, true);
                }
                Op::Clip(path, rule) => {
                    let mut path = self.paint_path(path.as_deref());
                    path.set_fill_type(fill_rule(*rule));
                    // Keep the clip in canvas space, so later transforms do not
                    // move it and save/restore retain the native clip stack.
                    path.transform(&self.transform());
                    self.canvas.clip_path(&path, ClipOperation::Intersect, true);
                }
                Op::FillRect(v) | Op::StrokeRect(v) | Op::ClearRect(v) => {
                    let [x, y, w, h] = v.map(|n| self.num(n));
                    let rect = Rect::new(x.min(x + w), y.min(y + h), x.max(x + w), y.max(y + h));
                    self.save_transform();
                    if matches!(op, Op::ClearRect(_)) {
                        let mut brush = Brush::new();
                        brush.set_blend_mode(BlendMode::Clear);
                        self.canvas.attach_brush(&brush);
                        self.canvas.draw_rect(&rect);
                        self.canvas.detach_brush();
                    } else if matches!(op, Op::StrokeRect(_)) {
                        let pen = self.pen();
                        self.canvas.attach_pen(&pen);
                        self.canvas.draw_rect(&rect);
                        self.canvas.detach_pen();
                    } else {
                        let brush = self.brush(&self.state.fill);
                        self.canvas.attach_brush(&brush);
                        self.canvas.draw_rect(&rect);
                        self.canvas.detach_brush();
                    }
                    self.canvas.restore();
                }
                Op::FillStyle(p) => self.state.fill = p.clone(),
                Op::StrokeStyle(p) => self.state.stroke = p.clone(),
                Op::LineWidth(n) => self.state.width = self.num(*n),
                Op::LineCap(c) => self.state.cap = *c,
                Op::LineJoin(j) => self.state.join = *j,
                Op::MiterLimit(v) => self.state.miter = *v,
                Op::LineDash(v) => self.state.dash = v.iter().map(|p| p.0).collect(),
                Op::LineDashOffset(v) => self.state.dash_offset = self.num(*v),
                Op::GlobalAlpha(v) => self.state.alpha = self.num(*v).clamp(0.0, 1.0),
                Op::Composite(c) => self.state.composite = *c,
                Op::ShadowColor(c) => self.state.shadow = *c,
                Op::ShadowBlur(v) => self.state.blur = v.0,
                Op::ShadowOffset(p) => self.state.offset = (p.x.0, p.y.0),
                Op::Font(f) => self.state.font = f.clone(),
                Op::FontVariantNumeric(v) => self.state.numeric = *v,
                Op::TextAlign(v) => self.state.align = *v,
                Op::TextBaseline(v) => self.state.baseline = *v,
                Op::FillText {
                    text,
                    x,
                    y,
                    max_width,
                }
                | Op::StrokeText {
                    text,
                    x,
                    y,
                    max_width,
                } => self.text(
                    text,
                    self.num(*x),
                    self.num(*y),
                    max_width.map(|w| w.0),
                    matches!(op, Op::StrokeText { .. }),
                ),
                Op::DrawImage {
                    image,
                    source,
                    dest,
                } => {
                    if let Some(bitmap) = self
                        .images
                        .iter()
                        .find(|i| i.uri == image.uri)
                        .and_then(|i| i.bitmap.as_ref())
                    {
                        let [x, y, w, h] = dest.map(|n| self.num(n));
                        let w = if w.is_nan() {
                            source.map_or(bitmap.width() as f32, |s| s.size.width.0)
                        } else {
                            w
                        };
                        let h = if h.is_nan() {
                            source.map_or(bitmap.height() as f32, |s| s.size.height.0)
                        } else {
                            h
                        };
                        self.save_transform();
                        let brush = self.brush(&Paint::Color(Color(0xffffffff)));
                        self.canvas.attach_brush(&brush);
                        let source = source.map(|s| {
                            Rect::new(
                                s.origin.x.0,
                                s.origin.y.0,
                                s.origin.x.0 + s.size.width.0,
                                s.origin.y.0 + s.size.height.0,
                            )
                        });
                        self.canvas.draw_bitmap_rect(
                            bitmap,
                            source.as_ref(),
                            &Rect::new(x, y, x + w, y + h),
                            &SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
                        );
                        self.canvas.detach_brush();
                        self.canvas.restore();
                    }
                }
            }
        }
        while self.saved.pop().is_some() {
            self.canvas.restore();
        }
        self.canvas.restore();
    }
    fn paint_path(&self, ops: Option<&[PathOp]>) -> Path {
        if let Some(ops) = ops {
            let mut path = Geometry::default();
            for op in ops {
                path.append(op, Affine::default(), self.slots);
            }
            path.path
        } else {
            let mut path = self.path.path.clone_path();
            if self.state.transform != Affine::default() {
                if let Some(inverse) = self.state.transform.inverse() {
                    path.transform(&inverse.native());
                } else {
                    path.reset();
                }
            }
            path
        }
    }
    fn paint_path_native(&self, mut path: Path, rule: FillRule, stroke: bool) {
        path.set_fill_type(fill_rule(rule));
        self.save_transform();
        if stroke {
            let pen = self.pen();
            self.canvas.attach_pen(&pen);
            self.canvas.draw_path(&path);
            self.canvas.detach_pen();
        } else {
            let brush = self.brush(&self.state.fill);
            self.canvas.attach_brush(&brush);
            self.canvas.draw_path(&path);
            self.canvas.detach_brush();
        }
        self.canvas.restore();
    }
    fn color(&self, paint: &Paint) -> u32 {
        match paint {
            Paint::Color(c) => argb(*c),
            Paint::Slot(i) => argb(crate::render::animation::color(
                self.slots
                    .get(*i as usize)
                    .copied()
                    .unwrap_or(AnimationValue::Number(0.0)),
            )),
            Paint::Gradient(g) => g.stops.last().map_or(0, |(_, c)| argb(*c)),
        }
    }
    fn shader(&self, paint: &Paint) -> Option<ShaderEffect> {
        let Paint::Gradient(g) = paint else {
            return None;
        };
        let colors: Vec<_> = g.stops.iter().map(|(_, c)| argb(*c)).collect();
        let positions: Vec<_> = g.stops.iter().map(|(p, _)| p.0).collect();
        match g.shape {
            canvas::GradientShape::Linear { start, end } => ShaderEffect::linear_gradient(
                (start.x.0, start.y.0),
                (end.x.0, end.y.0),
                &colors,
                &positions,
                TileMode::Clamp,
            ),
            canvas::GradientShape::Radial {
                start,
                start_radius,
                end,
                end_radius,
            } => ShaderEffect::two_point_conical_gradient(
                (start.x.0, start.y.0),
                start_radius.0,
                (end.x.0, end.y.0),
                end_radius.0,
                &colors,
                &positions,
                TileMode::Clamp,
                None,
            ),
            canvas::GradientShape::Conic {
                start_angle,
                center,
            } => {
                let stops: Vec<_> = g.stops.iter().map(|(p, c)| (p.0, *c)).collect();
                let (colors, positions) = sweep_stops(&stops, start_angle.0);
                ShaderEffect::conic_gradient(
                    (center.x.0, center.y.0),
                    &colors,
                    &positions,
                    TileMode::Clamp,
                )
            }
        }
    }
    fn shadow(&self) -> Option<ShadowLayer> {
        (self.state.shadow.0 & 255 != 0)
            .then(|| {
                ShadowLayer::new(
                    self.state.blur,
                    self.state.offset.0,
                    self.state.offset.1,
                    argb(self.state.shadow),
                )
            })
            .flatten()
    }
    fn brush(&self, paint: &Paint) -> Brush {
        let mut brush = Brush::new();
        brush.set_anti_alias(true);
        let color = self.color(paint);
        brush.set_color(color);
        let shader = self.shader(paint);
        brush.set_shader_effect(shader.as_ref());
        brush.set_alpha(
            ((if shader.is_some() { 255 } else { color >> 24 }) as f32 * self.state.alpha).round()
                as u8,
        );
        brush.set_blend_mode(blend(self.state.composite));
        brush.set_shadow_layer(self.shadow().as_ref());
        brush
    }
    fn pen(&self) -> Pen {
        let mut pen = Pen::new();
        pen.set_anti_alias(true);
        let color = self.color(&self.state.stroke);
        pen.set_color(color);
        let shader = self.shader(&self.state.stroke);
        pen.set_shader_effect(shader.as_ref());
        pen.set_alpha(
            ((if shader.is_some() { 255 } else { color >> 24 }) as f32 * self.state.alpha).round()
                as u8,
        );
        pen.set_width(self.state.width);
        pen.set_miter_limit(self.state.miter);
        pen.set_blend_mode(blend(self.state.composite));
        pen.set_cap(match self.state.cap {
            canvas::LineCap::Butt => ohos_drawing_binding::LineCap::FlatCap,
            canvas::LineCap::Round => ohos_drawing_binding::LineCap::RoundCap,
            canvas::LineCap::Square => ohos_drawing_binding::LineCap::SquareCap,
        });
        pen.set_join(match self.state.join {
            canvas::LineJoin::Miter => ohos_drawing_binding::LineJoin::MiterJoin,
            canvas::LineJoin::Round => ohos_drawing_binding::LineJoin::RoundJoin,
            canvas::LineJoin::Bevel => ohos_drawing_binding::LineJoin::BevelJoin,
        });
        let dash = PathEffect::dash(&self.state.dash, self.state.dash_offset);
        pen.set_path_effect(dash.as_ref());
        pen.set_shadow_layer(self.shadow().as_ref());
        pen
    }
    fn text(&mut self, text: &str, x: f32, y: f32, max_width: Option<f32>, stroke: bool) {
        let font = self.state.font.with_numeric(self.state.numeric);
        let mut style = TypographyStyle::new();
        style.set_max_lines(1);
        style.set_text_direction(if self.rtl {
            ohos_drawing_binding::TextDirection::Rtl
        } else {
            ohos_drawing_binding::TextDirection::Ltr
        });
        let mut text_style = TextStyle::new();
        text_style.set_font_size(font.size.0 as f64);
        text_style.set_font_style(FontStyle::new(
            FontWeight::from_css(font.weight),
            FontWidth::Normal,
            if font.italic {
                FontSlant::Italic
            } else {
                FontSlant::Normal
            },
        ));
        if let Some(family) = &font.family
            && !family.contains('\0')
        {
            text_style.set_font_families(&[family]);
        }
        for tag in font.variant.tags() {
            text_style.add_font_feature(tag, 1);
        }
        let brush = self.brush(&self.state.fill);
        let pen = self.pen();
        if stroke {
            text_style.set_foreground_pen(&pen);
        } else {
            text_style.set_foreground_brush(&brush);
        }
        let mut builder = TypographyBuilder::new(&mut style, self.fonts);
        builder.push_text_style(&mut text_style);
        builder.add_text(&text.replace(['\n', '\r'], " "));
        builder.pop_text_style();
        let mut paragraph = builder.build();
        paragraph.layout(1_000_000.0);
        let width = paragraph.longest_line() as f32;
        let fit = max_width.map_or(1.0, |v| (v / width.max(0.001)).clamp(0.0, 1.0));
        let right = match self.state.align {
            CanvasTextAlign::Right => true,
            CanvasTextAlign::Start => self.rtl,
            CanvasTextAlign::End => !self.rtl,
            _ => false,
        };
        let x = x - if right {
            width * fit
        } else if self.state.align == CanvasTextAlign::Center {
            width * fit * 0.5
        } else {
            0.0
        };
        let ascent = paragraph
            .line_metrics(0)
            .map_or(font.size.0 * 0.8, |m| m.ascender as f32);
        let dy = match self.state.baseline {
            CanvasTextBaseline::Alphabetic => ascent,
            CanvasTextBaseline::Top => 0.0,
            CanvasTextBaseline::Hanging => ascent * 0.2,
            CanvasTextBaseline::Middle => paragraph.height() as f32 * 0.5,
            CanvasTextBaseline::Ideographic | CanvasTextBaseline::Bottom => {
                paragraph.height() as f32
            }
        };
        self.save_transform();
        self.canvas.translate(x, y - dy);
        self.canvas.scale(fit, 1.0);
        paragraph.paint(self.canvas, 0.0, 0.0);
        self.canvas.restore();
    }
}
#[derive(Default)]
struct Geometry {
    path: Path,
    current: Option<[f32; 2]>,
    start: Option<[f32; 2]>,
}
impl Geometry {
    fn move_to(&mut self, p: [f32; 2]) {
        self.path.move_to(p[0], p[1]);
        self.current = Some(p);
        self.start = Some(p);
    }
    fn line_to(&mut self, p: [f32; 2]) {
        if self.current.is_none() {
            self.move_to(p)
        } else {
            self.path.line_to(p[0], p[1]);
            self.current = Some(p);
        }
    }
    fn cubic(&mut self, a: [f32; 2], b: [f32; 2], p: [f32; 2]) {
        if self.current.is_none() {
            self.move_to(a)
        }
        self.path
            .bezier_curve_to(a[0], a[1], b[0], b[1], p[0], p[1]);
        self.current = Some(p);
    }
    fn close(&mut self) {
        self.path.close();
        self.current = self.start;
    }
    fn rect(&mut self, [x, y, w, h]: [f32; 4], m: Affine) {
        self.move_to(m.point(x, y));
        self.line_to(m.point(x + w, y));
        self.line_to(m.point(x + w, y + h));
        self.line_to(m.point(x, y + h));
        self.close();
    }
    fn ellipse(&mut self, m: Affine, geometry: [f32; 5], start: f32, sweep: f32) {
        let [x, y, rx, ry, rotation] = geometry;
        if rx < 0.0 || ry < 0.0 {
            return;
        }
        let (s, c) = rotation.sin_cos();
        let ellipse = m.mul(Affine([c, s, -s, c, x, y]));
        let p = |t: f32| ellipse.point(rx * t.cos(), ry * t.sin());
        self.line_to(p(start));
        let count = (sweep.abs() / std::f32::consts::FRAC_PI_2).ceil() as usize;
        for i in 0..count {
            let a = start + sweep * i as f32 / count as f32;
            let b = start + sweep * (i + 1) as f32 / count as f32;
            let k = 4.0 / 3.0 * ((b - a) * 0.25).tan();
            self.cubic(
                ellipse.point(rx * (a.cos() - k * a.sin()), ry * (a.sin() + k * a.cos())),
                ellipse.point(rx * (b.cos() + k * b.sin()), ry * (b.sin() - k * b.cos())),
                p(b),
            );
        }
    }
    fn arc_to(&mut self, m: Affine, [x1, y1, x2, y2, r]: [f32; 5]) {
        if r < 0.0 {
            return;
        }
        let Some(current) = self.current else {
            self.move_to(m.point(x1, y1));
            return;
        };
        let Some(inv) = m.inverse() else { return };
        let p = inv.point(current[0], current[1]);
        let norm = |x: f32, y: f32| {
            let n = x.hypot(y);
            (n > 1e-6).then(|| [x / n, y / n])
        };
        let (Some(u), Some(v)) = (norm(p[0] - x1, p[1] - y1), norm(x2 - x1, y2 - y1)) else {
            self.line_to(m.point(x1, y1));
            return;
        };
        let cross = u[0] * v[1] - u[1] * v[0];
        if r == 0.0 || cross.abs() < 1e-6 {
            self.line_to(m.point(x1, y1));
            return;
        }
        let theta = (u[0] * v[0] + u[1] * v[1]).clamp(-1.0, 1.0).acos();
        let d = r / (theta / 2.0).tan();
        let t1 = [x1 + u[0] * d, y1 + u[1] * d];
        let t2 = [x1 + v[0] * d, y1 + v[1] * d];
        let Some(bis) = norm(u[0] + v[0], u[1] + v[1]) else {
            return;
        };
        let h = r / (theta / 2.0).sin();
        let c = [x1 + bis[0] * h, y1 + bis[1] * h];
        let a = (t1[1] - c[1]).atan2(t1[0] - c[0]);
        let b = (t2[1] - c[1]).atan2(t2[0] - c[0]);
        let mut sweep = (b - a).rem_euclid(std::f32::consts::TAU);
        if sweep > std::f32::consts::PI {
            sweep -= std::f32::consts::TAU
        }
        self.ellipse(m, [c[0], c[1], r, r, 0.0], a, sweep);
    }
    fn append(&mut self, op: &PathOp, m: Affine, slots: &[AnimationValue]) {
        let n = |n: Num| {
            n.get(|i| {
                slots
                    .get(i as usize)
                    .copied()
                    .unwrap_or(AnimationValue::Number(0.0))
                    .number()
            })
        };
        match op {
            PathOp::MoveTo(x, y) => self.move_to(m.point(n(*x), n(*y))),
            PathOp::LineTo(x, y) => self.line_to(m.point(n(*x), n(*y))),
            PathOp::QuadTo(v) => {
                let [x1, y1, x, y] = v.map(n);
                if self.current.is_none() {
                    self.move_to(m.point(x1, y1))
                }
                let a = m.point(x1, y1);
                let p = m.point(x, y);
                self.path.quadratic_curve_to(a[0], a[1], p[0], p[1]);
                self.current = Some(p);
            }
            PathOp::CubicTo(v) => {
                let [a, b, c, d, x, y] = v.map(n);
                self.cubic(m.point(a, b), m.point(c, d), m.point(x, y));
            }
            PathOp::Arc {
                x,
                y,
                radius,
                start,
                end,
                ccw,
            } => self.ellipse(
                m,
                [n(*x), n(*y), n(*radius), n(*radius), 0.0],
                n(*start),
                canvas::arc_sweep(
                    nsh_core::geometry::Radians(n(*start)),
                    nsh_core::geometry::Radians(n(*end)),
                    *ccw,
                )
                .0,
            ),
            PathOp::Ellipse {
                geometry,
                start,
                end,
                ccw,
            } => self.ellipse(
                m,
                geometry.map(n),
                n(*start),
                canvas::arc_sweep(
                    nsh_core::geometry::Radians(n(*start)),
                    nsh_core::geometry::Radians(n(*end)),
                    *ccw,
                )
                .0,
            ),
            PathOp::ArcTo(v) => self.arc_to(m, v.map(n)),
            PathOp::Rect(v) => self.rect(v.map(n), m),
            PathOp::RoundRect(v, r) => {
                let [x, y, w, h] = v.map(n);
                let mut r = r.map(n);
                if w < 0.0 {
                    r.swap(0, 1);
                    r.swap(2, 3);
                }
                if h < 0.0 {
                    r.swap(0, 3);
                    r.swap(1, 2);
                }
                let [x, y, w, h] = [x.min(x + w), y.min(y + h), w.abs(), h.abs()];
                let [tl, tr, br, bl] = canvas::fit_radii(
                    nsh_core::Size::new(nsh_core::Points(w), nsh_core::Points(h)),
                    nsh_core::Corners::from(r.map(nsh_core::Points)),
                )
                .to_array()
                .map(|p| p.0);
                self.move_to(m.point(x + tl, y));
                self.arc_to(m, [x + w, y, x + w, y + h, tr]);
                self.arc_to(m, [x + w, y + h, x, y + h, br]);
                self.arc_to(m, [x, y + h, x, y, bl]);
                self.arc_to(m, [x, y, x + w, y, tl]);
                self.close();
            }
            PathOp::Close => self.close(),
        }
    }
}
fn fill_rule(rule: FillRule) -> PathFillType {
    match rule {
        FillRule::NonZero => PathFillType::Winding,
        FillRule::EvenOdd => PathFillType::EvenOdd,
    }
}
fn blend(value: Composite) -> BlendMode {
    match value {
        Composite::SourceOver => BlendMode::SrcOver,
        Composite::SourceIn => BlendMode::SrcIn,
        Composite::SourceOut => BlendMode::SrcOut,
        Composite::SourceAtop => BlendMode::SrcAtop,
        Composite::DestinationOver => BlendMode::DstOver,
        Composite::DestinationIn => BlendMode::DstIn,
        Composite::DestinationOut => BlendMode::DstOut,
        Composite::DestinationAtop => BlendMode::DstAtop,
        Composite::Lighter => BlendMode::Plus,
        Composite::Copy => BlendMode::Src,
        Composite::Xor => BlendMode::Xor,
        Composite::Multiply => BlendMode::Multiply,
        Composite::Screen => BlendMode::Screen,
        Composite::Overlay => BlendMode::Overlay,
        Composite::Darken => BlendMode::Darken,
        Composite::Lighten => BlendMode::Lighten,
        Composite::ColorDodge => BlendMode::ColorDodge,
        Composite::ColorBurn => BlendMode::ColorBurn,
        Composite::HardLight => BlendMode::HardLight,
        Composite::SoftLight => BlendMode::SoftLight,
        Composite::Difference => BlendMode::Difference,
        Composite::Exclusion => BlendMode::Exclusion,
        Composite::Hue => BlendMode::Hue,
        Composite::Saturation => BlendMode::Saturation,
        Composite::Color => BlendMode::Color,
        Composite::Luminosity => BlendMode::Luminosity,
    }
}
pub(crate) fn sweep_stops(stops: &[(f32, Color)], angle: f32) -> (Vec<u32>, Vec<f32>) {
    if stops.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let shift = (angle / std::f32::consts::TAU).rem_euclid(1.0);
    if shift < 1e-6 {
        return (
            stops.iter().map(|(_, c)| argb(*c)).collect(),
            stops.iter().map(|(p, _)| *p).collect(),
        );
    }
    let at = (1.0 - shift).rem_euclid(1.0);
    let color = sample(stops, at);
    let mut rotated: Vec<_> = stops
        .iter()
        .map(|(p, c)| ((p + shift).rem_euclid(1.0), *c, *p))
        .collect();
    rotated.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.2.total_cmp(&a.2)));
    rotated.insert(0, (0.0, color, 0.0));
    rotated.push((1.0, color, 1.0));
    (
        rotated.iter().map(|(_, c, _)| argb(*c)).collect(),
        rotated.iter().map(|(p, _, _)| *p).collect(),
    )
}
pub(crate) fn sample(stops: &[(f32, Color)], t: f32) -> Color {
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for pair in stops.windows(2) {
        let [(a, ca), (b, cb)] = pair else {
            unreachable!()
        };
        if t <= *b {
            let f = if b > a { (t - a) / (b - a) } else { 1.0 };
            let mut out = 0;
            for shift in [0, 8, 16, 24] {
                let a = (ca.0 >> shift) & 255;
                let b = (cb.0 >> shift) & 255;
                out |= ((a as f32 + (b as f32 - a as f32) * f).round() as u32) << shift;
            }
            return Color(out);
        }
    }
    stops.last().unwrap().1
}
