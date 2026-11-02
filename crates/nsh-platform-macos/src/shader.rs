//! Shader views: the layer [`MountItem::Shader`](nsh_core::MountItem)
//! mounts on a view, rendered by `nsh_engine::shader` and shown as the contents of
//! a sublayer under the view's children, clipped to its rounded box as a
//! background is ([`crate::decor`] shapes it with the view).
//!
//! The surface opens the GPU device the first time a shader is mounted
//! ([`Shaders`]). The shader views with a frame to render or collect
//! ([`ShaderView::wants_frame`]: their uniforms or size changed, or they
//! read `nsh.time`) render on one frame callback of the surface's display
//! link, at their window's backing scale; a frame that comes back becomes a
//! `CGImage`, the layer's contents, stretched over the view's box. A view
//! out of sight (a list row scrolled away, a screen under another) stops
//! rendering and starts again once it shows ([`shown`], from
//! [`crate::mount::Mounter::revisit`]).
//!
//! Effects and transitions ([`ShaderRole`]) draw their children through
//! the shader: the children move into a container (as for clipping,
//! [`NshView::ensure_container`]) whose layer an empty mask hides while the
//! shader's frames show (AppKit hit-tests views, not what shows, so they
//! keep their clicks; [`crate::decor::hide_children`], which the
//! decoration's own clipping leaves in place), and the frames' layer goes
//! above the container, `overflow` points beyond the box. Where there is
//! no GPU device the children show as they are. A screen effect or
//! transition does the same with the surface's root, whose children stay
//! hidden while any of them hides them; its layer leaves the root when it
//! is deleted ([`deleted`]) or the app stops ([`clear`]). The pictures a
//! shader reads are the
//! container rendered with `renderInContext:` (without its mask) and, for
//! `sample_backdrop`, the root with the view left out, taken again on the
//! frame after the core says they changed ([`input_changed`]) or the UI
//! thread changed a view in them ([`ui_changed`]). A transition pictures
//! the children as they were when it starts ([`start_transition`]).
//! `renderInContext:` draws no video or web page, nor other shaders'
//! frames under an effect taken before they arrive.
//!
//! A page's or a dialog's `transitionShader` runs over the stack or the
//! surface as a [`NavOverlay`]: pictures of the lower and the upper side,
//! from and to. Where there is no GPU device, a transition starts no run
//! and a page or a dialog comes and goes with its own `animation`.

use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
    time::Instant,
};

use nsh_core::{
    animation::AnimationValue,
    canvas::Damage,
    shader::{ShaderInput, ShaderLayer, ShaderRole},
};
use nsh_engine::shader::{Frame, PixelFormat, Renderer, ShaderView};
use objc2::rc::Retained;
use objc2_core_foundation::{CFRetained, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGImage;
use objc2_quartz_core::{CALayer, kCAGravityResize};

use crate::{surface::Surface, view::NshView};

/// A surface's shader views: the GPU device, and the views rendering.
#[derive(Default)]
pub(crate) struct Shaders {
    /// Opened the first time a shader is mounted (`None` inside: there is
    /// no device, and shader views show nothing of their own).
    renderer: OnceCell<Option<Renderer>>,
    /// The views with a frame to render or collect.
    views: RefCell<Vec<NshView>>,
    /// The views whose shaders read pictures (effects, transitions,
    /// screens, backdrops): [`ui_changed`] looks among them.
    pictured: RefCell<Vec<NshView>>,
    /// Its frame callback runs.
    running: Cell<bool>,
}

impl Shaders {
    /// The GPU device, opened the first time one asks.
    pub(crate) fn renderer(&self) -> Option<&Renderer> {
        self.renderer
            .get_or_init(|| Renderer::new(PixelFormat::Rgba))
            .as_ref()
    }
}

/// What a shader view shows.
pub(crate) struct ShaderState {
    surface: Weak<Surface>,
    view: ShaderView,
    /// Shows the frames: a sublayer of the view's layer, under its
    /// children.
    layer: Retained<CALayer>,
    /// It is in [`Shaders::views`].
    ticking: bool,
    /// Out of sight with a frame to render ([`shown`]).
    hidden: bool,
    /// What the layer draws ([`ShaderRole::Fill`]: under the children, as
    /// upstream's; the others over them).
    role: ShaderRole,
    /// The children's (the root's, for a screen's) picture is due.
    stale: bool,
    /// The backdrop's picture is due.
    backdrop_stale: bool,
    /// The children's container is masked: the frames show instead
    /// ([`ShaderState::hides`]).
    masked: bool,
}

impl ShaderState {
    fn new(surface: Weak<Surface>) -> Self {
        let layer = CALayer::new();
        // Clipped to its rounded corners ([`crate::decor`] sets them).
        layer.setMasksToBounds(true);
        // SAFETY: an immutable Core Animation constant.
        layer.setContentsGravity(unsafe { kCAGravityResize });
        // Above the background, its gradients and inset shadows, below the
        // borders behind the children and the children.
        layer.setZPosition(crate::decor::BEHIND + 0.75);
        ShaderState {
            surface,
            view: ShaderView::default(),
            layer,
            ticking: false,
            hidden: false,
            role: ShaderRole::Fill,
            stale: false,
            backdrop_stale: false,
            masked: false,
        }
    }

    /// The layer the frames show in.
    pub(crate) fn layer(&self) -> &CALayer {
        &self.layer
    }

    /// Whether it draws under the children, shaped as a background
    /// ([`crate::decor`] places it); the others place themselves.
    pub(crate) fn fills(&self) -> bool {
        self.role == ShaderRole::Fill
    }

    /// Whether the children it draws are hidden now: an effect's always, a
    /// transition's while it runs; never where there is no GPU device (the
    /// view shows its background and children as they are).
    fn hides(&mut self, shaders: &Shaders, now: Instant) -> bool {
        self.view.replaces_children(now) && shaders.renderer().is_some()
    }
}

/// Above every child (and a canvas's or text's content), as the frames of
/// an effect show instead of them.
const OVER: CGFloat = 1.0e6;

thread_local! {
    /// The keys of the surfaces with views in [`Shaders::pictured`] (no
    /// views: those are the surfaces').
    static PICTURING: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

fn is_screen(role: ShaderRole) -> bool {
    matches!(role, ShaderRole::Screen | ShaderRole::ScreenTransition)
}

/// The view whose children `view`'s shader draws: itself, or the surface's
/// root for a screen effect or transition.
fn host(surface: &Surface, view: &NshView, role: ShaderRole) -> NshView {
    if is_screen(role) {
        surface.root.clone()
    } else {
        view.clone()
    }
}

/// Hides `host`'s children while a shader drawing them hides them
/// ([`ShaderState::masked`]): its own, or any of the surface's screen
/// effects and transitions, which share the root's.
fn sync_mask(surface: &Surface, host: &NshView) {
    let masked = surface.shaders.pictured.borrow().iter().any(|v| {
        v.state()
            .shader
            .as_ref()
            .is_some_and(|s| s.masked && self::host(surface, v, s.role) == *host)
    });
    crate::decor::hide_children(host, masked);
}

/// Whether the children `view`'s shader draws are hidden follows its role
/// and the device ([`ShaderState::hides`]), and so does their mask.
fn refresh_mask(surface: &Surface, view: &NshView, now: Instant) {
    let role = {
        let mut state = view.state_mut();
        let Some(shader) = state.shader.as_mut() else {
            return;
        };
        let masked = shader.hides(&surface.shaders, now);
        if masked == shader.masked {
            return;
        }
        shader.masked = masked;
        shader.role
    };
    sync_mask(surface, &host(surface, view, role));
}

/// Keeps `view` among the views whose shaders read pictures, or not.
fn track(surface: &Surface, view: &NshView, pictures: bool) {
    let mut pictured = surface.shaders.pictured.borrow_mut();
    pictured.retain(|v| v != view);
    if pictures {
        pictured.push(view.clone());
    }
    let key = surface.key();
    PICTURING.with(|p| {
        let mut p = p.borrow_mut();
        p.retain(|&k| k != key);
        if !pictured.is_empty() {
            p.push(key);
        }
    });
}

/// Mounts `layer` on `view` (`None`: it shows nothing of its own).
pub(crate) fn set(surface: &Rc<Surface>, view: &NshView, layer: Option<Arc<ShaderLayer>>) {
    {
        let mut state = view.state_mut();
        if layer.is_none() && state.shader.is_none() {
            return;
        }
        if layer.is_some() {
            surface.shaders.renderer();
        }
        let shader = state
            .shader
            .get_or_insert_with(|| ShaderState::new(Rc::downgrade(surface)));
        if layer.is_none() {
            // SAFETY: clearing the contents.
            unsafe { shader.layer.setContents(None) };
        }
        let role = layer.as_ref().map_or(shader.role, |l| l.role);
        let pictures = layer
            .as_ref()
            .is_some_and(|l| !l.shader.inputs().is_empty() || l.role != ShaderRole::Fill);
        shader.backdrop_stale = layer
            .as_ref()
            .is_some_and(|l| l.shader.inputs().contains(ShaderInput::Backdrop));
        shader.stale = pictures;
        shader.role = role;
        shader.view.set_layer(layer.clone());
        track(surface, view, pictures);
    }
    let (role, on) = {
        let state = view.state();
        let shader = state.shader.as_ref().expect("a shader");
        (shader.role, shader.view.layer().is_some())
    };
    if role != ShaderRole::Fill {
        let host = host(surface, view, role);
        let state = view.state();
        let shader = state.shader.as_ref().expect("a shader");
        if on {
            if shader.layer.superlayer().is_none()
                && let Some(parent) = host.ns().layer()
            {
                shader.layer.setZPosition(OVER);
                parent.addSublayer(&shader.layer);
            }
        } else {
            shader.layer.removeFromSuperlayer();
        }
        drop(state);
        if on {
            // Its pictures are of the children alone.
            host.ensure_container();
        }
        // An effect always shows its frames, a transition while it runs (a
        // new layer mid-run too); without a device, the children.
        refresh_mask(surface, view, Instant::now());
    }
    // The decoration shapes a fill's layer and adds it to the view's.
    view.restyle();
    if tick(view) {
        start(surface);
    }
}

/// Hides the children the frames show instead of, in `window`, for an
/// in-process picture of it ([`crate::synth::shot`]: `cacheDisplayInRect:`
/// draws masked layers whole); the views to show again after.
#[cfg(feature = "dev")]
pub(crate) fn hide_masked(
    window: &objc2_app_kit::NSWindow,
) -> Vec<Retained<objc2_app_kit::NSView>> {
    let keys = PICTURING.with(|p| p.borrow().clone());
    let views: Vec<NshView> = keys
        .into_iter()
        .filter_map(Surface::with_key)
        .flat_map(|s| s.shaders.pictured.borrow().clone())
        .collect();
    let mut hidden = Vec::new();
    for view in views {
        let Some(surface) = view
            .state()
            .shader
            .as_ref()
            .filter(|s| s.masked)
            .and_then(|s| {
                let role = s.role;
                s.surface.upgrade().map(|surface| (surface, role))
            })
        else {
            continue;
        };
        let host = host(&surface.0, &view, surface.1);
        if let Some(c) = host.container_view()
            && c.window().as_deref() == Some(window)
            && !c.isHidden()
        {
            c.setHidden(true);
            hidden.push(c);
        }
    }
    hidden
}

/// The core says what `view`'s shader reads changed
/// ([`nsh_core::MountItem::ShaderInput`]): its pictures are taken again on
/// the next display frame.
pub(crate) fn input_changed(surface: &Rc<Surface>, view: &NshView) {
    let marked = view.state_mut().shader.as_mut().is_some_and(|s| {
        let inputs = s
            .view
            .layer()
            .map(|l| l.shader.inputs())
            .unwrap_or_default();
        s.stale = s.role != ShaderRole::Fill;
        s.backdrop_stale = inputs.contains(ShaderInput::Backdrop);
        s.stale || s.backdrop_stale
    });
    if marked && tick_due(view) {
        start(surface);
    }
}

/// The UI thread changed `view` (an animated value, a scroll, an image
/// that loaded): the effects around it, and every backdrop, take their
/// pictures again.
pub(crate) fn ui_changed(view: &NshView) {
    let keys = PICTURING.with(|p| p.borrow().clone());
    for key in keys {
        if let Some(surface) = Surface::with_key(key) {
            ui_changed_in(&surface, view);
        }
    }
}

fn ui_changed_in(surface: &Rc<Surface>, view: &NshView) {
    let views = surface.shaders.pictured.borrow().clone();
    if views.is_empty() {
        return;
    }
    let mut marked = false;
    for v in views {
        if &v == view {
            continue;
        }
        let Some(mut state) = v.try_state_mut() else {
            continue;
        };
        let Some(shader) = state.shader.as_mut() else {
            continue;
        };
        let inside = shader.role != ShaderRole::Fill
            && (is_screen(shader.role) || view.ns().isDescendantOf(v.ns()));
        let backdrop = shader
            .view
            .layer()
            .is_some_and(|l| l.shader.inputs().contains(ShaderInput::Backdrop));
        if inside {
            shader.stale = true;
        }
        if backdrop {
            shader.backdrop_stale = true;
        }
        if inside || backdrop {
            marked = true;
            drop(state);
            tick_due(&v);
        }
    }
    // While the animator writes, [`wake`] starts them once it is done.
    if marked && surface.anim.try_borrow_mut().is_ok() {
        start(surface);
    }
}

/// A transition starts ([`nsh_core::MountItem::ShaderTransition`], before
/// the rest of its batch): the children as they show now are its `from`.
pub(crate) fn start_transition(
    surface: &Rc<Surface>,
    view: &NshView,
    duration: std::time::Duration,
) {
    let Some(renderer) = surface.shaders.renderer() else {
        return;
    };
    let (role, overflow) = {
        let state = view.state();
        let Some(shader) = state.shader.as_ref() else {
            return;
        };
        (shader.role, shader.view.overflow())
    };
    let host = host(surface, view, role);
    let scale = host.ns().window().map_or(2.0, |w| w.backingScaleFactor());
    let size = host.state().frame.size;
    // From what shows: the running transition's frame, or the children.
    let from = {
        let state = view.state();
        let shader = state.shader.as_ref().expect("a shader");
        (shader.masked && shader.view.frame().width > 0)
            .then(|| shader.view.frame().tight().to_vec())
            .map(|rgba| {
                (
                    rgba,
                    shader.view.frame().width as usize,
                    shader.view.frame().height as usize,
                )
            })
    };
    let from = from.or_else(|| children_picture(&host, size, overflow.0, scale, is_screen(role)));
    let now = Instant::now();
    {
        let mut state = view.state_mut();
        let shader = state.shader.as_mut().expect("a shader");
        if let Some((rgba, w, h)) = &from {
            shader.view.set_input(
                renderer,
                ShaderInput::From,
                Some((rgba, *w as u32, *h as u32, *w as u32 * 4)),
            );
        }
        shader.view.start_transition(duration, now);
        shader.stale = true;
    }
    refresh_mask(surface, view, now);
    if tick_due(view) {
        start(surface);
    }
}

/// `host`'s children (its container, without the mask that hides them),
/// the box grown by `overflow` points, at `scale`: premultiplied RGBA and
/// its size in pixels. A screen's (`whole`) is all the root shows (the
/// dialogs and what navigation hosts are beside the container) but the
/// frames over it.
fn children_picture(
    host: &NshView,
    size: nsh_core::Size<nsh_core::Points>,
    overflow: f32,
    scale: CGFloat,
    whole: bool,
) -> Option<(Vec<u8>, usize, usize)> {
    let container = host.container_view().and_then(|c| c.layer());
    let layer = match (&container, whole) {
        (Some(c), false) => c.clone(),
        _ => host.ns().layer()?,
    };
    let size = picture_size(size, overflow, scale)?;
    let origin = (-overflow as f64, -overflow as f64);
    let rgba = render_with(&layer, origin, size, |context| {
        let mask = container.as_ref().and_then(|c| c.mask());
        if let Some(c) = &container {
            // SAFETY: clearing (and below restoring) the container's own mask.
            unsafe { c.setMask(None) };
        }
        // SAFETY: a getter; the sublayers are retained while hidden.
        let over: Vec<Retained<CALayer>> = unsafe { layer.sublayers() }
            .map(|l| {
                l.iter()
                    .filter(|l| l.zPosition() >= OVER && !l.isHidden())
                    .collect()
            })
            .unwrap_or_default();
        over.iter().for_each(|l| l.setHidden(true));
        layer.renderInContext(context);
        over.iter().for_each(|l| l.setHidden(false));
        if let Some(c) = &container {
            unsafe { c.setMask(mask.as_deref()) };
        }
    });
    Some((rgba, size.0, size.1))
}

fn picture_size(
    size: nsh_core::Size<nsh_core::Points>,
    overflow: f32,
    scale: CGFloat,
) -> Option<(usize, usize, CGFloat)> {
    let (w, h) = (
        size.width.0 as f64 + 2.0 * overflow as f64,
        size.height.0 as f64 + 2.0 * overflow as f64,
    );
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    // At most as large as the renderer draws.
    let scale = scale.min(nsh_engine::shader::MAX_SIDE as f64 / w.max(h));
    Some((
        (w * scale).round().max(1.0) as usize,
        (h * scale).round().max(1.0) as usize,
        scale,
    ))
}

/// Draws the layers of `layer`'s tree that are due to draw (text and
/// canvases, which draw through a delegate): `renderInContext:` takes
/// their contents as they are.
fn display_tree(layer: &CALayer) {
    layer.displayIfNeeded();
    // SAFETY: a getter; the sublayers are retained while they are drawn.
    if let Some(sublayers) = unsafe { layer.sublayers() } {
        for sublayer in sublayers.iter() {
            display_tree(&sublayer);
        }
    }
}

/// `layer` (its views draw y down) rendered into premultiplied RGBA of
/// `pw` × `ph` pixels at `scale`, top row first: its point `origin` at the
/// top-left. Its layers that are due to draw draw first; then `draw` renders
/// into the context (`renderInContext:`), hiding what it leaves out (after
/// the drawing, which sets the views' layers again).
fn render_with(
    layer: &CALayer,
    origin: (f64, f64),
    (pw, ph, scale): (usize, usize, CGFloat),
    draw: impl FnOnce(&objc2_core_graphics::CGContext),
) -> Vec<u8> {
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo,
        kCGColorSpaceSRGB,
    };
    display_tree(layer);
    let mut out = vec![0u8; pw * ph * 4];
    // SAFETY: `out` holds `pw * 4 * ph` bytes and outlives the context; the
    // layer is rendered on the main thread, which owns it.
    unsafe {
        let Some(space) = CGColorSpace::with_name(Some(kCGColorSpaceSRGB)) else {
            return out;
        };
        let Some(context) = CGBitmapContextCreate(
            out.as_mut_ptr().cast(),
            pw,
            ph,
            8,
            pw * 4,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
        ) else {
            return out;
        };
        CGContext::translate_ctm(Some(&context), 0.0, ph as CGFloat);
        CGContext::scale_ctm(Some(&context), scale, -scale);
        CGContext::translate_ctm(Some(&context), -origin.0, -origin.1);
        crate::surface::quiet(|| draw(&context));
    }
    out
}

/// Takes the pictures `view`'s shader is due ([`ShaderState::stale`]).
fn take_pictures(surface: &Surface, view: &NshView, renderer: &Renderer, now: Instant) {
    let (role, stale, backdrop, overflow) = {
        let mut state = view.state_mut();
        let Some(shader) = state.shader.as_mut() else {
            return;
        };
        // An idle transition draws nothing: its children show.
        let running = match shader.role {
            ShaderRole::Transition | ShaderRole::ScreenTransition => {
                shader.view.replaces_children(now)
            }
            _ => true,
        };
        let stale = shader.stale && running && shader.role != ShaderRole::Fill;
        let backdrop = shader.backdrop_stale;
        // An idle transition's are taken again when it starts.
        shader.stale = false;
        shader.backdrop_stale = false;
        (shader.role, stale, backdrop, shader.view.overflow().0)
    };
    if !stale && !backdrop {
        return;
    }
    let host = host(surface, view, role);
    let Some(window) = host.ns().window() else {
        return;
    };
    let scale = window.backingScaleFactor();
    let size = host.state().frame.size;
    let mut pictures = Vec::new();
    if stale && let Some(p) = children_picture(&host, size, overflow, scale, is_screen(role)) {
        let input = match role {
            ShaderRole::Transition | ShaderRole::ScreenTransition => ShaderInput::To,
            _ => ShaderInput::Content,
        };
        pictures.push((input, p));
    }
    if backdrop
        && let (Some(root), Some(layer)) = (surface.root.ns().layer(), view.ns().layer())
        && let Some(picture) = picture_size(size, overflow, scale)
    {
        // What is behind: the surface without the view (and what it holds),
        // where the view's layer is.
        let at = layer.convertRect_toLayer(layer.bounds(), Some(&root));
        let origin = (at.origin.x - overflow as f64, at.origin.y - overflow as f64);
        let rgba = render_with(&root, origin, picture, |context| {
            let hidden = layer.isHidden();
            layer.setHidden(true);
            root.renderInContext(context);
            layer.setHidden(hidden);
        });
        pictures.push((ShaderInput::Backdrop, (rgba, picture.0, picture.1)));
    }
    let mut state = view.state_mut();
    let Some(shader) = state.shader.as_mut() else {
        return;
    };
    for (input, (rgba, w, h)) in pictures {
        shader.view.set_input(
            renderer,
            input,
            Some((&rgba, w as u32, h as u32, w as u32 * 4)),
        );
    }
}

/// The values of the layer's animated slots. The animator is borrowed
/// while it writes them: rendering starts once it is done ([`wake`]).
pub(crate) fn set_args(view: &NshView, values: &[(u16, AnimationValue)]) {
    {
        let mut state = view.state_mut();
        let Some(shader) = state.shader.as_mut() else {
            return;
        };
        for &(slot, value) in values {
            shader.view.set_slot(slot, value);
        }
    }
    tick(view);
}

/// It was deleted: no more pictures are taken for it. A screen effect's
/// or transition's frames, on the root rather than in the view, leave with
/// it, and the root's children show again unless another screen shader
/// hides them.
pub(crate) fn deleted(surface: &Surface, view: &NshView) {
    let screen = {
        let mut state = view.state_mut();
        let Some(shader) = state.shader.as_mut() else {
            return;
        };
        let screen = is_screen(shader.role);
        if screen {
            shader.masked = false;
            crate::surface::quiet(|| shader.layer.removeFromSuperlayer());
        }
        screen
    };
    track(surface, view, false);
    if screen {
        sync_mask(surface, &surface.root);
    }
}

/// Its size changed: a frame of the new size is rendered.
pub(crate) fn resized(surface: &Rc<Surface>, view: &NshView) {
    if view.state().shader.is_some() && tick(view) {
        start(surface);
    }
}

/// It can be seen again (after [`crate::mount::Mounter::draw_when_shown`]
/// put it off): renders what it missed.
pub(crate) fn shown(view: &NshView) {
    let surface = {
        let mut state = view.state_mut();
        let Some(shader) = state.shader.as_mut() else {
            return;
        };
        if !shader.hidden {
            return;
        }
        shader.hidden = false;
        shader.surface.upgrade()
    };
    if tick(view)
        && let Some(surface) = surface
    {
        start(&surface);
    }
}

/// Starts rendering the views whose animated slots were written
/// ([`set_args`]), once the animator is no longer borrowed.
pub(crate) fn wake(surface: &Surface) {
    let shaders = &surface.shaders;
    if shaders.running.get() || shaders.views.borrow().is_empty() {
        return;
    }
    if let Some(surface) = Surface::with_key(surface.key()) {
        start(&surface);
    }
}

/// The app stopped: its views render no more, and the screen effects'
/// and transitions' frames leave the root (which the next app keeps),
/// showing its children again.
pub(crate) fn clear(surface: &Surface) {
    let pictured = std::mem::take(&mut *surface.shaders.pictured.borrow_mut());
    for view in &pictured {
        if let Some(shader) = view.state_mut().shader.as_mut()
            && is_screen(shader.role)
        {
            shader.masked = false;
            crate::surface::quiet(|| shader.layer.removeFromSuperlayer());
        }
    }
    crate::decor::hide_children(&surface.root, false);
    let key = surface.key();
    PICTURING.with(|p| p.borrow_mut().retain(|&k| k != key));
    surface.shaders.views.borrow_mut().clear();
    surface.shaders.running.set(false);
}

/// Adds `view` to the views rendering if it has a frame to render; whether
/// it was added (the caller then [`start`]s the frame callback).
fn tick(view: &NshView) -> bool {
    tick_if(view, false)
}

/// As [`tick`], also when only pictures are due.
fn tick_due(view: &NshView) -> bool {
    tick_if(view, true)
}

fn tick_if(view: &NshView, pictures_due: bool) -> bool {
    let surface = {
        let mut state = view.state_mut();
        let Some(shader) = state.shader.as_mut() else {
            return false;
        };
        let due =
            shader.view.wants_frame() || (pictures_due && (shader.stale || shader.backdrop_stale));
        if shader.ticking || shader.hidden || !due {
            return false;
        }
        let Some(surface) = shader.surface.upgrade() else {
            return false;
        };
        if surface.shaders.renderer().is_none() {
            return false;
        }
        shader.ticking = true;
        surface
    };
    surface.shaders.views.borrow_mut().push(view.clone());
    true
}

/// Runs the frame callback that renders the views, if it does not run.
fn start(surface: &Rc<Surface>) {
    let shaders = &surface.shaders;
    if shaders.running.get() || shaders.views.borrow().is_empty() {
        return;
    }
    shaders.running.set(true);
    surface.add_frame_callback(|surface, _| {
        let more = frame(surface);
        if !more {
            surface.shaders.running.set(false);
        }
        more
    });
}

/// One display frame: each view renders and collects; whether a view has
/// more to render.
fn frame(surface: &Rc<Surface>) -> bool {
    let shaders = &surface.shaders;
    let Some(renderer) = shaders.renderer() else {
        return false;
    };
    let Ok(mounter) = surface.mounter.try_borrow() else {
        return true;
    };
    let views = std::mem::take(&mut *shaders.views.borrow_mut());
    let now = Instant::now();
    let mut kept = Vec::with_capacity(views.len());
    for view in views {
        let deleted = mounter.views.get(&view.id()) != Some(&view);
        let screen = view
            .state()
            .shader
            .as_ref()
            .is_some_and(|s| is_screen(s.role));
        let hidden = !deleted && !screen && !view.shown();
        if !deleted && !hidden {
            take_pictures(surface, &view, renderer, now);
        }
        let more = !deleted && !hidden && render_view(surface, &view, renderer, now);
        if more {
            kept.push(view);
            continue;
        }
        if let Some(shader) = view.state_mut().shader.as_mut() {
            shader.ticking = false;
            shader.hidden = hidden;
        }
        if hidden {
            // Rendered again once it shows ([`shown`]).
            mounter.draw_when_shown(&view, Damage::All);
        }
    }
    drop(mounter);
    let mut views = shaders.views.borrow_mut();
    kept.append(&mut views);
    *views = kept;
    !views.is_empty()
}

/// Renders and collects `view`'s frame, showing one that came back;
/// whether it has more to render.
fn render_view(surface: &Surface, view: &NshView, renderer: &Renderer, now: Instant) -> bool {
    let role = view
        .state()
        .shader
        .as_ref()
        .map_or(ShaderRole::Fill, |s| s.role);
    let host = host(surface, view, role);
    let scale = host.ns().window().map_or(2.0, |w| w.backingScaleFactor()) as f32;
    let size = host.state().frame.size;
    let mut state = view.state_mut();
    let Some(shader) = state.shader.as_mut() else {
        return false;
    };
    shader.view.set_size(size, scale);
    if role != ShaderRole::Fill {
        // Over the box, `overflow` points beyond it.
        let o = shader.view.overflow().0 as f64;
        crate::surface::quiet(|| {
            shader.layer.setFrame(CGRect::new(
                CGPoint::new(-o, -o),
                CGSize::new(
                    size.width.0 as f64 + 2.0 * o,
                    size.height.0 as f64 + 2.0 * o,
                ),
            ));
        });
    }
    if shader.view.render(renderer, now) {
        let image = frame_image(shader.view.frame());
        let contents = image.as_deref().map(crate::decor::image_object);
        // SAFETY: a CGImage is a valid `contents` object (none clears it).
        crate::surface::quiet(|| unsafe { shader.layer.setContents(contents.as_deref()) });
    }
    // A transition that ended: its children show as they are (unless a
    // screen effect still hides the root's).
    if matches!(role, ShaderRole::Transition | ShaderRole::ScreenTransition)
        && shader.masked
        && !shader.view.replaces_children(now)
    {
        shader.masked = false;
        // SAFETY: clearing the contents.
        crate::surface::quiet(|| unsafe { shader.layer.setContents(None) });
        drop(state);
        sync_mask(surface, &host);
        return false;
    }
    shader.view.wants_frame() || shader.stale || shader.backdrop_stale
}

/// A frame's premultiplied sRGB RGBA pixels as a CGImage (`None` when it is
/// empty).
fn frame_image(frame: &Frame) -> Option<CFRetained<CGImage>> {
    rgba_image(frame.width as usize, frame.height as usize, frame.rows())
}

/// `height` rows of `width` premultiplied sRGB RGBA pixels as a CGImage
/// (`None` when it is empty).
fn rgba_image<'a>(
    width: usize,
    height: usize,
    rows: impl Iterator<Item = &'a [u8]>,
) -> Option<CFRetained<CGImage>> {
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGBitmapContextCreateImage, CGBitmapContextGetBytesPerRow,
        CGBitmapContextGetData, CGColorSpace, CGImageAlphaInfo, CGImageByteOrderInfo,
        kCGColorSpaceSRGB,
    };
    if width == 0 || height == 0 {
        return None;
    }
    // SAFETY: a constant of CoreGraphics.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: CoreGraphics allocates the pixels (null data) of an RGBA
    // context of these dimensions; each row is copied within its
    // `bytes_per_row` bytes of the `bytes_per_row * height` while `context`
    // lives.
    unsafe {
        let context = CGBitmapContextCreate(
            std::ptr::null_mut(),
            width,
            height,
            8,
            width * 4,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
        )?;
        let data = CGBitmapContextGetData(Some(&context)).cast::<u8>();
        if data.is_null() {
            return None;
        }
        let stride = CGBitmapContextGetBytesPerRow(Some(&context));
        let pixels = std::slice::from_raw_parts_mut(data, stride * height);
        for (row, src) in rows.enumerate() {
            let len = src.len().min(stride);
            pixels[row * stride..row * stride + len].copy_from_slice(&src[..len]);
        }
        CGBitmapContextCreateImage(Some(&context))
    }
}

/// A page's or a dialog's coming or going drawn through its
/// `transitionShader` ([`nsh_core::NavShader`]): a layer over `host` (the
/// stack, or the root for a dialog) showing the shader's frames, which
/// read the host without its upper side (what comes, or what leaves) as
/// `from` and with it as `to`; going away runs backwards. Both are pictured
/// on the first display frame (a view navigation just showed has no layer
/// in the tree before AppKit displays the window, and out of it its layers
/// draw y up), the side that comes again on each (it is laid out and drawn
/// over the next ones). Navigation keeps the views at rest under it and
/// ends it with its motion.
pub(crate) struct NavOverlay {
    view: RefCell<ShaderView>,
    layer: Retained<CALayer>,
    host: NshView,
    upper: Retained<objc2_app_kit::NSView>,
    /// What the upper side shows, when it is a picture navigation hosts
    /// (a page or a dialog that left: its view's layer is out of the tree).
    image: Option<CFRetained<CGImage>>,
    backwards: bool,
    duration: std::time::Duration,
    picture: (usize, usize, CGFloat),
    /// The first display frame pictured both sides and started it.
    started: Cell<bool>,
}

impl NavOverlay {
    /// Starts the shader over `host`, whose upper side is `upper` (a view
    /// in it, showing `image` when it is a picture); `None`: there is no
    /// GPU device, or nothing to show.
    pub(crate) fn new(
        surface: &Surface,
        host: &NshView,
        upper: &objc2_app_kit::NSView,
        image: Option<CFRetained<CGImage>>,
        shader: &nsh_core::NavShader,
        backwards: bool,
    ) -> Option<NavOverlay> {
        surface.shaders.renderer()?;
        let layer = host.ns().layer()?;
        let scale = host.ns().window().map_or(2.0, |w| w.backingScaleFactor());
        let size = host.state().frame.size;
        let picture = picture_size(size, 0.0, scale)?;
        let mut view = ShaderView::default();
        view.set_layer(Some(shader.layer.clone()));
        view.set_size(size, picture.2 as f32);
        let overlay = NavOverlay {
            view: RefCell::new(view),
            layer: CALayer::new(),
            host: host.clone(),
            upper: Retained::from(upper),
            image,
            backwards,
            duration: shader.duration,
            picture,
            started: Cell::new(false),
        };
        // SAFETY: an immutable Core Animation constant.
        overlay
            .layer
            .setContentsGravity(unsafe { kCAGravityResize });
        overlay.layer.setZPosition(OVER);
        // Until its first frame: what showed. Coming, the lower side (the
        // host as it is, the upper side left out); going, the views at rest
        // show it (the upper side on top).
        let first = (!backwards).then(|| overlay.side(false));
        let (pw, ph, _) = picture;
        let image = first.and_then(|rgba| rgba_image(pw, ph, rgba.chunks(pw * 4)));
        let contents = image.as_deref().map(crate::decor::image_object);
        crate::surface::quiet(|| {
            overlay.layer.setFrame(CGRect::new(
                CGPoint::new(0.0, 0.0),
                CGSize::new(size.width.0 as f64, size.height.0 as f64),
            ));
            // SAFETY: a CGImage is a valid `contents` object.
            unsafe { overlay.layer.setContents(contents.as_deref()) };
            layer.addSublayer(&overlay.layer);
        });
        Some(overlay)
    }

    /// The host without the upper side (`with_upper`: with it on top, drawn
    /// on its own where its view is), the overlay left out.
    fn side(&self, with_upper: bool) -> Vec<u8> {
        let (Some(host), Some(upper)) = (self.host.ns().layer(), self.upper.layer()) else {
            return Vec::new();
        };
        display_tree(&upper);
        let at = self.upper.convertPoint_toView(
            objc2_foundation::NSPoint::new(0.0, 0.0),
            Some(self.host.ns()),
        );
        render_with(&host, (0.0, 0.0), self.picture, |context| {
            use objc2_core_graphics::CGContext;
            let hidden = (upper.isHidden(), self.layer.isHidden());
            upper.setHidden(true);
            self.layer.setHidden(true);
            host.renderInContext(context);
            upper.setHidden(hidden.0);
            self.layer.setHidden(hidden.1);
            if with_upper {
                CGContext::save_g_state(Some(context));
                CGContext::translate_ctm(Some(context), at.x, at.y);
                match &self.image {
                    // Images draw y up.
                    Some(image) => {
                        let size = self.upper.frame().size;
                        CGContext::translate_ctm(Some(context), 0.0, size.height);
                        CGContext::scale_ctm(Some(context), 1.0, -1.0);
                        let rect = CGRect::new(CGPoint::new(0.0, 0.0), size);
                        CGContext::draw_image(Some(context), rect, Some(image));
                    }
                    None => upper.renderInContext(context),
                }
                CGContext::restore_g_state(Some(context));
            }
        })
    }

    /// One display frame: pictures the sides (the first time both, then the
    /// one that comes), renders and shows a frame that came back.
    pub(crate) fn tick(&self, surface: &Surface) {
        let Some(renderer) = surface.shaders.renderer() else {
            return;
        };
        let first = !self.started.replace(true);
        let mut sides = Vec::with_capacity(2);
        if first || self.backwards {
            sides.push((ShaderInput::From, self.side(false)));
        }
        if first || !self.backwards {
            sides.push((ShaderInput::To, self.side(true)));
        }
        let (pw, ph, _) = self.picture;
        let mut view = self.view.borrow_mut();
        for (input, rgba) in &sides {
            let picture = (&rgba[..], pw as u32, ph as u32, pw as u32 * 4);
            view.set_input(renderer, *input, Some(picture));
        }
        if first {
            let now = Instant::now();
            if self.backwards {
                view.start_transition_backwards(self.duration, now);
            } else {
                view.start_transition(self.duration, now);
            }
        }
        if view.render(renderer, Instant::now()) {
            let image = frame_image(view.frame());
            let contents = image.as_deref().map(crate::decor::image_object);
            // SAFETY: a CGImage is a valid `contents` object.
            crate::surface::quiet(|| unsafe { self.layer.setContents(contents.as_deref()) });
        }
    }
}

impl Drop for NavOverlay {
    fn drop(&mut self) {
        crate::surface::quiet(|| self.layer.removeFromSuperlayer());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nsh_core::shader::Shader;

    use super::*;

    /// A shader view showing a layer of `role`, outside any surface.
    fn state(role: ShaderRole) -> ShaderState {
        let shader =
            Shader::compile("fn shade(uv: vec2f) -> vec4f { return vec4f(uv, 0.0, 1.0); }")
                .expect("a shader");
        let mut layer = ShaderLayer::new(shader, &[], &mut Vec::new());
        layer.role = role;
        let mut state = ShaderState::new(Weak::new());
        state.role = role;
        state.view.set_layer(Some(Arc::new(layer)));
        state
    }

    /// Shaders whose GPU device was looked for and not found.
    fn no_device() -> Shaders {
        Shaders {
            renderer: OnceCell::from(None),
            ..Shaders::default()
        }
    }

    #[test]
    fn without_a_device_nothing_hides_its_children() {
        let shaders = no_device();
        let now = Instant::now();
        for role in [ShaderRole::Effect, ShaderRole::Screen] {
            assert!(!state(role).hides(&shaders, now), "{role:?}");
        }
        for role in [ShaderRole::Transition, ShaderRole::ScreenTransition] {
            let mut s = state(role);
            s.view.start_transition(Duration::from_millis(300), now);
            assert!(!s.hides(&shaders, now), "{role:?}");
        }
    }

    #[test]
    fn an_effect_hides_its_children_and_a_transition_while_it_runs() {
        let shaders = Shaders::default();
        if shaders.renderer().is_none() {
            eprintln!("no GPU: skipped");
            return;
        }
        let now = Instant::now();
        for role in [ShaderRole::Effect, ShaderRole::Screen] {
            let mut s = state(role);
            assert!(s.hides(&shaders, now), "{role:?}");
            // It shows nothing of its own any more.
            s.view.set_layer(None);
            assert!(!s.hides(&shaders, now), "{role:?}");
        }
        let duration = Duration::from_millis(300);
        for role in [ShaderRole::Transition, ShaderRole::ScreenTransition] {
            let mut s = state(role);
            assert!(!s.hides(&shaders, now), "{role:?}: idle");
            s.view.start_transition(duration, now);
            assert!(s.hides(&shaders, now), "{role:?}");
            // A new layer while it runs (another uniform) keeps them hidden.
            let layer = s.view.layer().cloned();
            s.view.set_layer(layer);
            assert!(s.hides(&shaders, now), "{role:?}: a new layer");
            assert!(!s.hides(&shaders, now + duration), "{role:?}: ended");
        }
        assert!(!state(ShaderRole::Fill).hides(&shaders, now));
    }
}
