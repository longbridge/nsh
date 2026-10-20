//! Drawing a surface's view tree with Direct2D.
//!
//! [`frame`] draws the whole tree of a surface into its window's swap
//! chain and presents it; [`snapshot`] draws it into an offscreen bitmap
//! and reads the pixels back (`-> Rect {script` screenshots). Both go through
//! [`draw_view`], which draws one view and its subtree in this order:
//!
//! 1. shadows and background ([`crate::decor::draw_background`]),
//! 2. the content, clipped to the bounds: text, image, canvas,
//! 3. the children (clipped to the rounded bounds when the view clips),
//! 4. the hosted items ([`crate::view::Native`]) and the scroll indicator,
//! 5. the border ([`crate::decor::draw_border`]),
//!
//! all inside an opacity layer when the view is translucent. A view whose
//! transform is projective (perspective, `rotateX` with a perspective) is
//! drawn into an offscreen bitmap first and mapped with
//! `DrawBitmap`'s perspective transform; every other transform is a 3x2
//! world transform.
//!
//! Coordinates are points (Direct2D DIPs: the context's DPI is the
//! window's scale times 96).
//!
//! A window with web views draws the same single pass, split at each web
//! view ([`crate::compose`]): reaching one records where it goes
//! ([`DrawCx::compose`]), and the next thing drawn first moves the context
//! to the next layer above it ([`DrawCx::realize`]: the clips and opacity
//! layers pushed so far are popped, the target changes, and they are
//! pushed again). Without a composition, `realize` is one test of an
//! `Option`: the frame is the one swap chain, as before.

use std::rc::Rc;

use nsh_core::{
    Bounds, Color, Points,
    geometry::{Point, Size},
};
use windows::{
    Win32::{
        Foundation::{D2DERR_RECREATE_TARGET, RECT},
        Graphics::{
            Direct2D::{
                Common::{
                    D2D_POINT_2U, D2D_RECT_F, D2D_RECT_U, D2D_SIZE_F, D2D_SIZE_U,
                    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
                    D2D1_FIGURE_END_CLOSED, D2D1_PIXEL_FORMAT,
                },
                D2D1_ANTIALIAS_MODE_ALIASED, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT,
                D2D1_ARC_SIZE_SMALL, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_CPU_READ,
                D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
                D2D1_INTERPOLATION_MODE_LINEAR, D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1,
                D2D1_MAP_OPTIONS_READ, D2D1_ROUNDED_RECT, D2D1_SWEEP_DIRECTION_CLOCKWISE,
                ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Geometry, ID2D1Image, ID2D1SolidColorBrush,
            },
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
        },
    },
    core::Interface,
};
use windows_numerics::Matrix3x2;

use crate::{
    compose::{Composition, Placement},
    geom::Mat4,
    gfx::{self, Gfx},
    surface::Surface,
    view::{Hosted, NshView},
};

/// A color as Direct2D's (straight alpha; Direct2D premultiplies).
pub(crate) fn color_f(c: Color) -> D2D1_COLOR_F {
    let (r, g, b, a) = c.to_f64();
    D2D1_COLOR_F {
        r: r as f32,
        g: g as f32,
        b: b as f32,
        a: a as f32,
    }
}

/// A rectangle as Direct2D's.
pub(crate) fn rect_f(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

/// An unbounded rectangle (layer content bounds).
pub(crate) const INFINITE: D2D_RECT_F = D2D_RECT_F {
    left: -1.0e7,
    top: -1.0e7,
    right: 1.0e7,
    bottom: 1.0e7,
};

/// What was pushed on the context, to pop it the same way (and push it
/// again: [`DrawCx::flatten`]).
enum Pushed {
    /// A layer: its opacity and mask, with the transform it was pushed
    /// with.
    Layer {
        transform: Matrix3x2,
        opacity: f32,
        mask: Option<ID2D1Geometry>,
    },
    /// An axis-aligned clip, with the transform it was pushed with.
    Clip {
        transform: Matrix3x2,
        rect: D2D_RECT_F,
    },
}

/// The state of one drawing pass.
pub(crate) struct DrawCx<'a> {
    /// The devices.
    pub(crate) gfx: &'a Gfx,
    /// The context drawn with (the same as `gfx.dc`).
    pub(crate) dc: &'a ID2D1DeviceContext,
    /// The world transform of the view being drawn (its coordinates to
    /// the target's DIPs).
    pub(crate) transform: Mat4,
    /// Pixels per point.
    pub(crate) scale: f32,
    /// The surface draws in its dark appearance.
    pub(crate) dark: bool,
    /// The frame's time (seconds, the animation clock).
    pub(crate) now: f64,
    brush: ID2D1SolidColorBrush,
    pushed: Vec<Pushed>,
    /// Hosted items drawn in this pass, with their window rectangles.
    pub(crate) drawn: Vec<(Rc<dyn Hosted>, RECT)>,
    /// True when drawing offscreen (a snapshot): child windows are not
    /// moved.
    pub(crate) offscreen: bool,
    /// True when the context's target holds what lies behind the view
    /// being drawn (the window's pass, a snapshot), so blend modes and
    /// backdrop filters can read it ([`crate::decor`]); false in passes
    /// drawn offscreen for a filtered or 3D-transformed view.
    pub(crate) backdrop: bool,
    /// The opacity of what is drawn now (the views' around it, multiplied).
    pub(crate) alpha: f32,
    /// The composed items (web views) reached in this pass, in paint
    /// order, with where they go.
    pub(crate) composed: Vec<Composed>,
    /// The window's pass of a surface with a composition: what is drawn
    /// after a composed item goes into the next layer.
    pub(crate) split: Option<Split<'a>>,
    /// The clip of the passes around this one (DIPs of the window), for
    /// a pass drawn offscreen in window coordinates.
    clip_base: Option<D2D_RECT_F>,
}

/// A composed item reached by a pass.
pub(crate) struct Composed {
    /// The item.
    pub(crate) item: Rc<dyn Hosted>,
    /// Where it goes.
    pub(crate) at: Placement,
}

/// The layers a window's pass draws into ([`DrawCx::realize`]).
pub(crate) struct Split<'a> {
    composition: &'a Composition,
    /// The window's size in pixels.
    size: (u32, u32),
    /// For each layer started, how many composed items are below it.
    pub(crate) layers: Vec<usize>,
    /// A composed item was reached: the next drawing starts a layer.
    pending: bool,
    /// Ending the draw into a target failed.
    pub(crate) error: Option<windows::core::Error>,
}

impl<'a> DrawCx<'a> {
    /// A pass with `dc` (its target set, between `BeginDraw` and
    /// `EndDraw`).
    pub(crate) fn new(
        gfx: &'a Gfx,
        scale: f32,
        dark: bool,
        now: f64,
    ) -> windows::core::Result<Self> {
        Self::with_dc(gfx, &gfx.dc, scale, dark, now)
    }

    fn with_dc(
        gfx: &'a Gfx,
        dc: &'a ID2D1DeviceContext,
        scale: f32,
        dark: bool,
        now: f64,
    ) -> windows::core::Result<Self> {
        let black = color_f(Color::BLACK);
        // SAFETY: a COM call on the live context.
        let brush = unsafe { dc.CreateSolidColorBrush(&black, None)? };
        Ok(DrawCx {
            gfx,
            dc,
            transform: Mat4::new(),
            scale,
            dark,
            now,
            brush,
            pushed: Vec::new(),
            drawn: Vec::new(),
            offscreen: false,
            backdrop: true,
            alpha: 1.0,
            composed: Vec::new(),
            split: None,
            clip_base: None,
        })
    }

    /// A pass like this one drawing with `dc` (another context of the same
    /// device, its target set and drawing begun) at `scale` pixels per
    /// point: offscreen drawing for filters and 3D transforms
    /// ([`crate::decor`]).
    pub(crate) fn nested<'b>(
        &self,
        dc: &'b ID2D1DeviceContext,
        scale: f32,
    ) -> windows::core::Result<DrawCx<'b>>
    where
        'a: 'b,
    {
        let mut cx = DrawCx::with_dc(self.gfx, dc, scale, self.dark, self.now)?;
        cx.offscreen = self.offscreen;
        cx.backdrop = false;
        cx.alpha = self.alpha;
        cx.clip_base = Some(self.clip_bounds());
        Ok(cx)
    }

    /// What the clips pushed so far leave visible, in DIPs of the target
    /// (and of the passes around it).
    fn clip_bounds(&self) -> D2D_RECT_F {
        let mut r = self.clip_base.unwrap_or(INFINITE);
        for p in &self.pushed {
            let b = match p {
                Pushed::Clip { transform, rect } => crate::compose::map_bounds(transform, rect),
                Pushed::Layer {
                    transform,
                    mask: Some(mask),
                    ..
                } => {
                    // SAFETY: a COM call on a live geometry.
                    match unsafe { mask.GetBounds(Some(transform)) } {
                        Ok(b) => b,
                        Err(_) => continue,
                    }
                }
                Pushed::Layer { .. } => continue,
            };
            r = crate::compose::intersect(r, b);
        }
        r
    }

    /// A composed item (`item`, `size` points with `radii`, at `m`) is
    /// drawn here: records where it goes, and what follows goes into the
    /// next layer.
    fn compose(&mut self, item: &Rc<dyn Hosted>, m: Mat4, size: (f32, f32), radii: [f32; 4]) {
        let s = self.scale;
        let t = m.to_3x2();
        // The page's pixels are the view's points times the scale: the
        // linear part stays, the translation is in pixels.
        let mut matrix = Matrix3x2 {
            M31: t.M31 * s,
            M32: t.M32 * s,
            ..t
        };
        if t.M11 == 1.0 && t.M22 == 1.0 && t.M12 == 0.0 && t.M21 == 0.0 {
            // Only moved: on whole pixels, so that the page stays sharp.
            matrix.M31 = matrix.M31.round();
            matrix.M32 = matrix.M32.round();
        }
        let c = self.clip_bounds();
        let clip = D2D_RECT_F {
            left: (c.left * s).max(0.0),
            top: (c.top * s).max(0.0),
            right: (c.right * s).min(1.0e7),
            bottom: (c.bottom * s).min(1.0e7),
        };
        let size_px = ((size.0 * s).round() as i32, (size.1 * s).round() as i32);
        let bounds = crate::compose::map_bounds(
            &matrix,
            &rect_f(0.0, 0.0, size_px.0 as f32, size_px.1 as f32),
        );
        if size_px.0 <= 0
            || size_px.1 <= 0
            || crate::compose::is_empty(&crate::compose::intersect(bounds, clip))
        {
            return;
        }
        self.composed.push(Composed {
            item: item.clone(),
            at: Placement {
                matrix,
                size: size_px,
                clip,
                radii: radii.map(|r| r * s),
                alpha: self.alpha,
            },
        });
        if let Some(split) = &mut self.split {
            split.pending = true;
        }
    }

    /// Something is about to be drawn: after a composed item, the pass
    /// goes on in the next layer (made as needed), with its clips and
    /// layers pushed again there.
    pub(crate) fn realize(&mut self) {
        let Some(split) = &mut self.split else { return };
        if !split.pending {
            return;
        }
        split.pending = false;
        let index = split.layers.len();
        let target = split
            .composition
            .layer_target(self.gfx, index, split.size, self.scale);
        let target = match target {
            Ok(t) => t,
            Err(err) => {
                // Drawn on below the web view instead.
                log::warn!("compose: no layer above the web view: {err}");
                return;
            }
        };
        split.layers.push(self.composed.len());
        let ended = self.flatten(|dc| {
            // SAFETY: COM calls on the live context between its pushes:
            // the draw into the previous target ends, one into the layer
            // begins.
            unsafe {
                let ended = dc.EndDraw(None, None);
                dc.SetTarget(&target);
                dc.BeginDraw();
                dc.SetTransform(&Matrix3x2::identity());
                dc.Clear(Some(&color_f(Color::TRANSPARENT)));
                ended
            }
        });
        if let (Err(err), Some(split)) = (ended, &mut self.split) {
            split.error.get_or_insert(err);
        }
    }

    /// Takes the composed items a nested pass reached (in window
    /// coordinates): what follows them goes above them.
    pub(crate) fn adopt_composed(&mut self, mut composed: Vec<Composed>) {
        if composed.is_empty() {
            return;
        }
        self.composed.append(&mut composed);
        if let Some(split) = &mut self.split {
            split.pending = true;
        }
    }

    /// The pass's solid brush, set to `color`.
    pub(crate) fn solid(&self, color: Color) -> &ID2D1SolidColorBrush {
        // SAFETY: a COM call on a live brush.
        unsafe { self.brush.SetColor(&color_f(color)) };
        &self.brush
    }

    /// Sets the world transform (`m` must be affine: see
    /// [`Mat4::is_affine`]).
    pub(crate) fn set_transform(&mut self, m: Mat4) {
        self.transform = m;
        let t = m.to_3x2();
        // SAFETY: a COM call on the live context.
        unsafe { self.dc.SetTransform(&t) };
    }

    /// Draws what follows at `opacity`.
    pub(crate) fn push_opacity(&mut self, opacity: f32) {
        self.push_layer(opacity, None);
    }

    /// Clips what follows to `geometry` (in the current coordinates).
    pub(crate) fn push_clip_geometry(&mut self, geometry: &ID2D1Geometry) {
        self.push_layer(1.0, Some(geometry.clone()));
    }

    /// Pushes a layer of `opacity` masked by `mask` (current coordinates).
    fn push_layer(&mut self, opacity: f32, mask: Option<ID2D1Geometry>) {
        self.push_layer_raw(opacity, mask.as_ref());
        self.pushed.push(Pushed::Layer {
            transform: self.dc_transform(),
            opacity,
            mask,
        });
    }

    fn push_layer_raw(&self, opacity: f32, mask: Option<&ID2D1Geometry>) {
        let mut params = D2D1_LAYER_PARAMETERS1 {
            contentBounds: INFINITE,
            geometricMask: std::mem::ManuallyDrop::new(mask.cloned()),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity,
            opacityBrush: std::mem::ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS1_NONE,
        };
        // SAFETY: valid parameters; popped by `pop`.
        unsafe { self.dc.PushLayer(&params, None) };
        // The parameters own a reference to the mask: release it.
        // SAFETY: the field is not used afterwards.
        unsafe { std::mem::ManuallyDrop::drop(&mut params.geometricMask) };
    }

    /// The context's current transform.
    fn dc_transform(&self) -> Matrix3x2 {
        let mut t = Matrix3x2::identity();
        // SAFETY: a COM call on the live context writing to a local.
        unsafe { self.dc.GetTransform(&mut t) };
        t
    }

    /// Pops every layer and clip pushed so far, runs `between` (what was
    /// drawn into the layers is in the target then: to read what lies
    /// behind a view, [`crate::decor`]) and pushes them again as they
    /// were. Exact for clips; a translucent layer splits in two (what is
    /// drawn before and after no longer composites as one group).
    pub(crate) fn flatten<R>(&mut self, between: impl FnOnce(&ID2D1DeviceContext) -> R) -> R {
        for p in self.pushed.iter().rev() {
            match p {
                // SAFETY: matches the push.
                Pushed::Layer { .. } => unsafe { self.dc.PopLayer() },
                // SAFETY: as above.
                Pushed::Clip { .. } => unsafe { self.dc.PopAxisAlignedClip() },
            }
        }
        let out = between(self.dc);
        let current = self.dc_transform();
        for p in &self.pushed {
            match p {
                Pushed::Layer {
                    transform,
                    opacity,
                    mask,
                } => {
                    // SAFETY: a COM call on the live context.
                    unsafe { self.dc.SetTransform(transform) };
                    self.push_layer_raw(*opacity, mask.as_ref());
                }
                Pushed::Clip { transform, rect } => {
                    // SAFETY: COM calls on the live context; popped by
                    // `pop`.
                    unsafe {
                        self.dc.SetTransform(transform);
                        self.dc
                            .PushAxisAlignedClip(rect, D2D1_ANTIALIAS_MODE_ALIASED);
                    }
                }
            }
        }
        // SAFETY: a COM call on the live context.
        unsafe { self.dc.SetTransform(&current) };
        out
    }

    /// Clips what follows to the rectangle (current coordinates). Exact
    /// when the transform keeps rectangles axis-aligned; otherwise a
    /// geometric layer.
    pub(crate) fn push_clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let t = self.transform.to_3x2();
        if t.M12 == 0.0 && t.M21 == 0.0 {
            // Snapped to pixels when the edges fall on them, as UIKit's
            // `masksToBounds` of a pixel-aligned layer.
            let rect = rect_f(x, y, w, h);
            // SAFETY: popped by `pop`.
            unsafe {
                self.dc
                    .PushAxisAlignedClip(&rect, D2D1_ANTIALIAS_MODE_ALIASED)
            };
            self.pushed.push(Pushed::Clip {
                transform: self.dc_transform(),
                rect,
            });
        } else if let Some(g) = self.rounded_geometry(crate::geom::rect(x, y, w, h), [0.0; 4]) {
            self.push_clip_geometry(&g);
        } else {
            self.push_opacity(1.0);
        }
    }

    /// Clips what follows to the rounded rectangle `rect` with `radii`
    /// (clockwise from the top left).
    pub(crate) fn push_clip_rounded(&mut self, rect: Bounds<Points>, radii: [f32; 4]) {
        if radii.iter().all(|r| *r <= 0.0) {
            return self.push_clip_rect(
                rect.origin.x.0,
                rect.origin.y.0,
                rect.size.width.0,
                rect.size.height.0,
            );
        }
        match self.rounded_geometry(rect, radii) {
            Some(g) => self.push_clip_geometry(&g),
            None => self.push_clip_rect(
                rect.origin.x.0,
                rect.origin.y.0,
                rect.size.width.0,
                rect.size.height.0,
            ),
        }
    }

    /// Pops the last push.
    pub(crate) fn pop(&mut self) {
        match self.pushed.pop() {
            // SAFETY: matches the push.
            Some(Pushed::Layer { .. }) => unsafe { self.dc.PopLayer() },
            // SAFETY: as above.
            Some(Pushed::Clip { .. }) => unsafe { self.dc.PopAxisAlignedClip() },
            None => log::error!("render: pop without push"),
        }
    }

    /// The rounded rectangle `rect` with corner radii clockwise from the
    /// top left, as a geometry.
    pub(crate) fn rounded_geometry(
        &self,
        rect: Bounds<Points>,
        radii: [f32; 4],
    ) -> Option<ID2D1Geometry> {
        rounded_geometry(self.gfx, rect, radii)
    }
}

/// The rounded rectangle `rect` with corner radii clockwise from the top
/// left (each clamped to half the box), as a geometry.
pub(crate) fn rounded_geometry(
    gfx: &Gfx,
    rect: Bounds<Points>,
    radii: [f32; 4],
) -> Option<ID2D1Geometry> {
    let Bounds {
        origin: Point {
            x: Points(x),
            y: Points(y),
        },
        size: Size {
            width: Points(w),
            height: Points(h),
        },
    } = rect;
    let max = (w.min(h) / 2.0).max(0.0);
    let r = radii.map(|r| r.clamp(0.0, max));
    if r[0] == r[1] && r[1] == r[2] && r[2] == r[3] {
        let rr = D2D1_ROUNDED_RECT {
            rect: rect_f(x, y, w, h),
            radiusX: r[0],
            radiusY: r[0],
        };
        // SAFETY: COM calls on the live factory.
        return unsafe { gfx.factory.CreateRoundedRectangleGeometry(&rr) }
            .ok()
            .and_then(|g| g.cast().ok());
    }
    // SAFETY: COM calls on live objects; the sink is closed before use.
    unsafe {
        let path = gfx.factory.CreatePathGeometry().ok()?;
        let sink = path.Open().ok()?;
        let p = |px: f32, py: f32| windows_numerics::Vector2 { X: px, Y: py };
        let arc = |to: windows_numerics::Vector2, radius: f32| D2D1_ARC_SEGMENT {
            point: to,
            size: D2D_SIZE_F {
                width: radius,
                height: radius,
            },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        };
        sink.BeginFigure(p(x + r[0], y), D2D1_FIGURE_BEGIN_FILLED);
        sink.AddLine(p(x + w - r[1], y));
        if r[1] > 0.0 {
            sink.AddArc(&arc(p(x + w, y + r[1]), r[1]));
        }
        sink.AddLine(p(x + w, y + h - r[2]));
        if r[2] > 0.0 {
            sink.AddArc(&arc(p(x + w - r[2], y + h), r[2]));
        }
        sink.AddLine(p(x + r[3], y + h));
        if r[3] > 0.0 {
            sink.AddArc(&arc(p(x, y + h - r[3]), r[3]));
        }
        sink.AddLine(p(x, y + r[0]));
        if r[0] > 0.0 {
            sink.AddArc(&arc(p(x + r[0], y), r[0]));
        }
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close().ok()?;
        path.cast().ok()
    }
}

/// Draws `view` and its subtree; `parent` is the world transform of the
/// parent's coordinates and `origin` where the view goes in them.
pub(crate) fn draw_view(cx: &mut DrawCx<'_>, view: &NshView, parent: Mat4, origin: (f32, f32)) {
    let state = view.state();
    if state.style.hidden || state.concealed {
        return;
    }
    let opacity = state.opacity();
    if opacity <= 0.0 {
        return;
    }
    let mut local = Mat4::new().translate(origin.0, origin.1);
    if let Some(t) = state.transform() {
        local = Mat4::mul(&t, &local);
    }
    let world = Mat4::mul(&local, &parent);
    if crate::decor::backface_away(&state, &world) {
        return;
    }
    // Drawn above a web view reached before it, if any.
    cx.realize();
    if !world.is_affine() {
        drop(state);
        return draw_projected(cx, view, world, opacity);
    }
    let layered = crate::decor::layered(&state);
    drop(state);
    cx.set_transform(world);
    if opacity < 1.0 {
        cx.push_opacity(opacity);
    }
    let alpha = cx.alpha;
    cx.alpha *= opacity;
    if layered {
        // Filters, a shadow of the content or a blend mode: the view is
        // drawn offscreen and composited as one image.
        crate::decor::draw_layered(cx, view, world);
    } else {
        draw_content(cx, view, world);
    }
    cx.alpha = alpha;
    if opacity < 1.0 {
        cx.pop();
    }
}

/// Draws what `view` draws and its subtree at `world` (set on the context):
/// the backdrop filter, shadows and background, the content clipped to the
/// bounds, the children, the hosted items, the scroll indicator, the
/// border and outline.
pub(crate) fn draw_content(cx: &mut DrawCx<'_>, view: &NshView, world: Mat4) {
    let state = view.state();
    crate::decor::draw_backdrop(cx, &state);
    crate::decor::draw_background(cx, &state);
    let clip = state.clips();
    let (w, h) = (state.frame.size.width.0, state.frame.size.height.0);
    if clip {
        cx.push_clip_rounded(crate::geom::rect(0.0, 0.0, w, h), state.radii());
    } else {
        // The view's own content stays inside it, as a UIKit view's drawing
        // stays in its backing store.
        cx.push_clip_rect(0.0, 0.0, w, h);
    }
    crate::text::draw(cx, &state);
    crate::image::draw(cx, &state);
    crate::canvas::draw(cx, &state);
    if !clip {
        cx.pop();
    }
    let natives = state.natives.clone();
    drop(state);
    view.with_children(|children| {
        for child in children {
            let origin = {
                let s = child.state();
                view.state().child_origin(s.frame, s.shift)
            };
            draw_view(cx, child, world, origin);
        }
    });
    cx.set_transform(world);
    for native in &natives {
        let r = native.rect;
        let m = Mat4::new()
            .translate(r.origin.x.0, r.origin.y.0)
            .then(&world);
        if native.item.composed() {
            let radii = {
                let s = view.state();
                if s.clips() { s.radii() } else { [0.0; 4] }
            };
            cx.compose(&native.item, m, (r.size.width.0, r.size.height.0), radii);
            continue;
        }
        cx.realize();
        cx.set_transform(m);
        native.item.draw(cx, (r.size.width.0, r.size.height.0));
        if let Some((x, y, bw, bh)) = m.bounds(0.0, 0.0, r.size.width.0, r.size.height.0) {
            let s = cx.scale;
            let rect = RECT {
                left: (x * s).round() as i32,
                top: (y * s).round() as i32,
                right: ((x + bw) * s).round() as i32,
                bottom: ((y + bh) * s).round() as i32,
            };
            cx.drawn.push((native.item.clone(), rect));
        }
    }
    let state = view.state();
    let above = crate::scroll::indicates(&state)
        || state.borders().is_visible()
        || state.style.outline.is_visible();
    if above {
        // A scroll indicator, a border over a web view.
        cx.realize();
    }
    cx.set_transform(world);
    crate::scroll::draw_indicator(cx, &state);
    if clip {
        cx.pop();
    }
    crate::decor::draw_border(cx, &state);
}

/// Most pixels along a side of a 3D-transformed view's bitmap.
const MAX_PROJECTED: f32 = 8192.0;

/// Draws `view` (with a projective `world` transform) into an offscreen
/// bitmap, then maps the bitmap with Direct2D's perspective transform.
fn draw_projected(cx: &mut DrawCx<'_>, view: &NshView, world: Mat4, opacity: f32) {
    let (w, h, overflow, outset) = {
        let s = view.state();
        (
            s.frame.size.width.0,
            s.frame.size.height.0,
            s.overflow,
            crate::decor::outset(&s),
        )
    };
    // What the subtree covers, in the view's coordinates, with the view's
    // shadows and outline.
    let area = overflow.unwrap_or(crate::geom::rect(0.0, 0.0, w, h));
    let area = crate::geom::rect(
        area.origin.x.0 - outset,
        area.origin.y.0 - outset,
        area.size.width.0 + 2.0 * outset,
        area.size.height.0 + 2.0 * outset,
    );
    if area.size.width.0 <= 0.0 || area.size.height.0 <= 0.0 {
        return;
    }
    // Drawn at the window's scale, less for a huge subtree (the bitmap's
    // DIPs stay the view's points).
    let scale = cx
        .scale
        .min(MAX_PROJECTED / area.size.width.0)
        .min(MAX_PROJECTED / area.size.height.0);
    let px = (
        (area.size.width.0 * scale).ceil().max(1.0) as u32,
        (area.size.height.0 * scale).ceil().max(1.0) as u32,
    );
    let props = gfx::target_properties(scale, true);
    // SAFETY: a COM call on the live context with valid properties.
    let bitmap = unsafe {
        cx.dc.CreateBitmap(
            D2D_SIZE_U {
                width: px.0,
                height: px.1,
            },
            None,
            0,
            &D2D1_BITMAP_PROPERTIES1 {
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
                ..props
            },
        )
    };
    let bitmap = match bitmap {
        Ok(b) => b,
        Err(err) => {
            log::debug!("render: projected view not drawn: {err}");
            return;
        }
    };
    let Ok(image) = bitmap.cast::<ID2D1Image>() else {
        return;
    };
    // The subtree as if untransformed, at the bitmap's origin, drawn by
    // another context: layers pushed on this one stay as they are.
    let base = Mat4::new().translate(-area.origin.x.0, -area.origin.y.0);
    let mut flat = view.clone();
    let composed = cx.composed.len();
    let drawn = crate::decor::layer::draw_into(cx, &image, scale, true, |inner| {
        draw_flat(inner, &mut flat, base);
    });
    // Web views in it were placed in the bitmap's coordinates, not the
    // window's: they are not shown (a visual takes no perspective).
    cx.composed.truncate(composed);
    if !drawn {
        return;
    }
    // The bitmap's DIPs map to the view's coordinates (offset by the
    // area's origin), then through the view's world transform.
    let m = Mat4::new()
        .translate(area.origin.x.0, area.origin.y.0)
        .then(&world);
    let dest = rect_f(0.0, 0.0, area.size.width.0, area.size.height.0);
    let saved = cx.transform;
    // SAFETY: COM calls on the live context and bitmap.
    unsafe {
        cx.dc.SetTransform(&Matrix3x2::identity());
        cx.dc.DrawBitmap(
            &bitmap,
            Some(&dest),
            opacity,
            D2D1_INTERPOLATION_MODE_LINEAR,
            None,
            Some(&m.to_d2d()),
        );
    }
    cx.set_transform(saved);
}

/// Draws `view` with its transform replaced by `base` (the inside of
/// [`draw_projected`]), at full opacity.
fn draw_flat(cx: &mut DrawCx<'_>, view: &mut NshView, base: Mat4) {
    let (saved_style_transform, saved_opacity, saved_anim) = {
        let mut s = view.state_mut();
        let t = std::mem::take(&mut s.style.transform);
        let o = std::mem::replace(&mut s.style.opacity, 1.0);
        // Only the animated opacity and transform: animated colors and
        // shadows still draw.
        let a = s.anim;
        s.anim.opacity = None;
        s.anim.transform = Default::default();
        (t, o, a)
    };
    let saved_live = view.state_mut().live.take();
    let saved_fade = std::mem::replace(&mut view.state_mut().fade, 1.0);
    draw_view(cx, view, base, (0.0, 0.0));
    let mut s = view.state_mut();
    s.style.transform = saved_style_transform;
    s.style.opacity = saved_opacity;
    s.anim = saved_anim;
    s.live = saved_live;
    s.fade = saved_fade;
}

/// Draws the surface's window: the whole tree, then presents it.
pub(crate) fn frame(surface: &Rc<Surface>) {
    let Ok(gfx) = gfx::get() else { return };
    let hwnd = surface.hwnd();
    if hwnd.is_invalid() {
        return;
    }
    let scale = surface.scale();
    let size = surface.pixel_size();
    if size.0 == 0 || size.1 == 0 {
        return;
    }
    let mut swap = surface.swap.borrow_mut();
    if swap.as_ref().is_some_and(|s| s.stale()) {
        *swap = None;
    }
    if swap.is_none() {
        match gfx::SwapChain::new(&gfx, hwnd, size) {
            Ok(s) => *swap = Some(s),
            Err(err) => {
                log::error!("cannot create the swap chain: {err}");
                return;
            }
        }
    }
    let Some(chain) = swap.as_mut() else { return };
    if let Err(err) = chain.resize(&gfx, size) {
        log::error!("cannot resize the swap chain: {err}");
        *swap = None;
        return;
    }
    if (surface.target_scale.get() - scale).abs() > f32::EPSILON {
        chain.drop_target();
        surface.target_scale.set(scale);
    }
    let target = match chain.target(&gfx, scale) {
        Ok(t) => t,
        Err(err) => {
            log::error!("cannot get the back buffer: {err}");
            return;
        }
    };
    // Only a window with web views has a composition: without one the
    // frame is the one swap chain, drawn and presented as always.
    let composition = surface.composition();
    let drawn = draw_into(surface, &gfx, &target, scale, false, composition.as_deref());
    match drawn {
        Ok(drawn) => {
            if let Some(c) = &composition
                && let Err(err) = c.present(drawn.layers.len())
            {
                log::warn!("present failed: {err}");
                drop(swap);
                device_lost(surface);
                return;
            }
            if let Err(err) = chain.present() {
                log::warn!("present failed: {err}");
                drop(swap);
                device_lost(surface);
                return;
            }
            drop(swap);
            surface.items_placed(drawn.items);
            surface.composed_placed(composition.as_deref(), drawn.composed, &drawn.layers);
        }
        Err(err) if err.code() == D2DERR_RECREATE_TARGET => {
            drop(swap);
            device_lost(surface);
        }
        Err(err) => log::error!("drawing failed: {err}"),
    }
}

fn device_lost(surface: &Rc<Surface>) {
    *surface.swap.borrow_mut() = None;
    if let Some(c) = surface.composition() {
        c.device_lost();
    }
    gfx::lost();
    surface.invalidate();
}

/// What a pass drew besides pixels.
struct Drawn {
    /// The hosted items drawn and their window rectangles.
    items: Vec<(Rc<dyn Hosted>, RECT)>,
    /// The composed items (web views) and where they go.
    composed: Vec<Composed>,
    /// The layers drawn: how many composed items are below each.
    layers: Vec<usize>,
}

/// Draws the tree into `target` (its DPI set for `scale`), and into the
/// layers of `composition` above its web views.
fn draw_into(
    surface: &Rc<Surface>,
    gfx: &Gfx,
    target: &ID2D1Bitmap1,
    scale: f32,
    offscreen: bool,
    composition: Option<&Composition>,
) -> windows::core::Result<Drawn> {
    let dc = &gfx.dc;
    // SAFETY: COM calls on the live context and target; `EndDraw` always
    // follows `BeginDraw`.
    unsafe {
        dc.SetTarget(target);
        dc.SetDpi(96.0 * scale, 96.0 * scale);
        dc.BeginDraw();
        dc.SetTransform(&Matrix3x2::identity());
        dc.Clear(Some(&color_f(crate::system::window_background(surface))));
    }
    let mut cx = match DrawCx::new(gfx, scale, crate::system::dark(surface), surface.now()) {
        Ok(cx) => cx,
        Err(err) => {
            // SAFETY: closes the `BeginDraw` above.
            let _ = unsafe { dc.EndDraw(None, None) };
            return Err(err);
        }
    };
    cx.offscreen = offscreen;
    cx.split = composition.map(|composition| Split {
        composition,
        size: surface.pixel_size(),
        layers: Vec::new(),
        pending: false,
        error: None,
    });
    let root = surface.root.clone();
    draw_view(&mut cx, &root, Mat4::new(), (0.0, 0.0));
    // The caption buttons, last: in the topmost layer, above web views.
    crate::chrome::draw(surface, dc, scale);
    let items = std::mem::take(&mut cx.drawn);
    let composed = std::mem::take(&mut cx.composed);
    let (layers, error) = match cx.split.take() {
        Some(s) => (s.layers, s.error),
        None => (Vec::new(), None),
    };
    drop(cx);
    // SAFETY: closes the last `BeginDraw` (above, or a layer's); the target
    // is unset so that the swap chains can be resized later.
    let result = unsafe { dc.EndDraw(None, None) };
    // SAFETY: as above.
    unsafe { dc.SetTarget(None) };
    if let Some(err) = error {
        return Err(err);
    }
    result.map(|()| Drawn {
        items,
        composed,
        layers,
    })
}

/// Pixels of a snapshot: BGRA, premultiplied, `width * 4` bytes per row.
pub(crate) struct Pixels {
    /// Width in pixels.
    pub(crate) width: u32,
    /// Height in pixels.
    pub(crate) height: u32,
    /// The rows.
    pub(crate) data: Vec<u8>,
}

/// Draws the surface offscreen at `scale` and reads the pixels back,
/// whatever covers the window.
pub(crate) fn snapshot(surface: &Rc<Surface>, scale: f32) -> windows::core::Result<Pixels> {
    let gfx = gfx::get()?;
    let (w, h) = surface.size();
    let px = (
        (w * scale).round().max(1.0) as u32,
        (h * scale).round().max(1.0) as u32,
    );
    let size = D2D_SIZE_U {
        width: px.0,
        height: px.1,
    };
    let format = D2D1_PIXEL_FORMAT {
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
    };
    // SAFETY: COM calls on the live context; the mapped memory is copied
    // before `Unmap`.
    unsafe {
        let target = gfx.dc.CreateBitmap(
            size,
            None,
            0,
            &D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: format,
                dpiX: 96.0 * scale,
                dpiY: 96.0 * scale,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
                colorContext: std::mem::ManuallyDrop::new(None),
            },
        )?;
        // Web views are not in it: their pages are composed by the system.
        let drawn = draw_into(surface, &gfx, &target, scale, true, None)?;
        drop(drawn);
        let read = gfx.dc.CreateBitmap(
            size,
            None,
            0,
            &D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: format,
                dpiX: 96.0 * scale,
                dpiY: 96.0 * scale,
                bitmapOptions: D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: std::mem::ManuallyDrop::new(None),
            },
        )?;
        read.CopyFromBitmap(
            Some(&D2D_POINT_2U { x: 0, y: 0 }),
            &target,
            Some(&D2D_RECT_U {
                left: 0,
                top: 0,
                right: px.0,
                bottom: px.1,
            }),
        )?;
        let mapped = read.Map(D2D1_MAP_OPTIONS_READ)?;
        let mut data = Vec::with_capacity((px.0 * px.1 * 4) as usize);
        for row in 0..px.1 {
            let start = mapped.bits.add((row * mapped.pitch) as usize);
            data.extend_from_slice(std::slice::from_raw_parts(start, (px.0 * 4) as usize));
        }
        read.Unmap()?;
        Ok(Pixels {
            width: px.0,
            height: px.1,
            data,
        })
    }
}
