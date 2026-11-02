//! Shader views: the layer [`MountItem::Shader`](nsh_core::MountItem)
//! mounts on a view, rendered by `nsh_engine::shader` on the UI thread next to the
//! animator and shown by `Box.java` as a bitmap under the view's children,
//! clipped to its rounded box as a background is.
//!
//! The layer stays here, by view id: `encode.rs` only tells Java which views
//! show a shader (`SHADER`), and Java reports their size in pixels
//! (`Native.shaderSize`). The GPU device is opened the first time a shader
//! is mounted on the `NshView` and kept across app loads. The animated slots
//! are written from the animation graph whenever its outputs are read
//! ([`crate::animation::UiAnimation::outputs`]), which also keeps the
//! `Choreographer` ticking while a shader view has a frame to render or
//! collect ([`ShaderView::wants_frame`]); each tick renders them, and a frame
//! that comes back crosses to Java once (`NshView.shaderFrame`), which copies
//! it into the view's bitmap.
//!
//! Effects, transitions and shaders reading what is behind them take
//! pictures Java makes (`Pictures.java`: the children, the backdrop, the
//! window, rendered by a `HardwareRenderer` into an `ImageReader`) and hands
//! over with `Native.shaderInput`; a picture equal to the last one of its
//! input is dropped, so taking one on every window frame renders only what
//! changed. Each frame says whether it replaces the view's children (an
//! effect always, a transition while it runs): Java then draws the frame
//! where the children were. A page's or a dialog's `transitionShader`
//! ([`MountItem::NavShader`](nsh_core::MountItem)) runs under the id of the
//! stack or the dialog it plays in (`Native.navTransition`), over its lower
//! side (`from`) and its upper side (`to`): its own clock moves
//! `nsh.progress` from 0 to 1 as a page or a dialog comes, from 1 to 0 as it
//! goes. Java hears of it when a frame comes back, which replaces what it
//! covers, and once when its clock ends, with a frame that does not: until
//! the first frame comes back, what it covers shows as it is.
//!
//! Without a device (`Renderer::new` is `None`) shader views show their
//! background and children: an effect gets no frames, a transition starts
//! no run, and a page or a dialog comes and goes with its own animation.

use std::{
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

use jni::{
    objects::{JObject, JValue},
    signature::{Primitive, ReturnType},
};
use nsh_core::{
    NavShader, NodeId,
    animation::AnimationValue,
    geometry::{Percentage, Points, size},
    shader::{ShaderInput, ShaderLayer},
};
use nsh_engine::shader::{Frame, PixelFormat, Renderer, ShaderView};
use rustc_hash::FxHashMap;

use crate::java::{java, with_env};

/// The shader views of one `NshView` and the GPU device they render with.
#[derive(Default)]
pub(crate) struct Shaders {
    /// Opened the first time a shader is mounted (`Some(None)`: there is
    /// no device, and shader views show nothing of their own).
    renderer: Option<Option<Renderer>>,
    views: FxHashMap<u32, ShaderView>,
    /// What Java was last told of each view: whether frames replace the
    /// children, and the hashes of its pictures by input.
    told: FxHashMap<u32, (bool, [u64; 4])>,
    /// The pages' and dialogs' `transitionShader`s, by page or dialog id.
    nav: FxHashMap<u32, NavShader>,
    /// The running page and dialog transitions (by stack or dialog id),
    /// whose views are dropped once they end.
    navs: FxHashMap<u32, NavRun>,
}

/// A page or a dialog coming or going: when it started, for how long, and
/// the slot its progress moves.
struct NavRun {
    start: Instant,
    duration: Duration,
    /// It goes: `nsh.progress` from 1 to 0.
    reverse: bool,
    slot: u16,
}

impl NavRun {
    /// How far it ran at `now`, from 0 to 1.
    fn at(&self, now: Instant) -> f32 {
        (now.saturating_duration_since(self.start).as_secs_f32()
            / self.duration.as_secs_f32().max(f32::MIN_POSITIVE))
        .min(1.0)
    }
}

impl Shaders {
    /// Forgets every view (a new app runs), keeping the device.
    pub(crate) fn clear(&mut self) {
        self.views.clear();
        self.told.clear();
        self.nav.clear();
        self.navs.clear();
    }

    fn renderer(&mut self) -> Option<&Renderer> {
        // Android's ARGB_8888 is R, G, B, A in memory, premultiplied.
        self.renderer
            .get_or_insert_with(|| Renderer::new(PixelFormat::Rgba))
            .as_ref()
    }

    /// Page or dialog `id`'s `transitionShader` (`None`: its animation).
    pub(crate) fn set_nav(&mut self, id: u32, shader: Option<NavShader>) {
        match shader {
            Some(s) => {
                self.nav.insert(id, s);
            }
            None => {
                self.nav.remove(&id);
            }
        }
    }

    /// Picture `input` of view `id`: `height` rows of `width` premultiplied
    /// RGBA pixels, `stride` bytes apart; dropped when it is the last one.
    pub(crate) fn input(
        &mut self,
        id: u32,
        input: ShaderInput,
        picture: Option<(&[u8], u32, u32, u32)>,
    ) {
        let hash = picture.map_or(0, |(pixels, w, h, stride)| {
            let mut hasher = rustc_hash::FxHasher::default();
            (w, h, stride).hash(&mut hasher);
            pixels.hash(&mut hasher);
            hasher.finish() | 1
        });
        if !self.views.contains_key(&id) || self.renderer().is_none() {
            return;
        }
        let told = &mut self.told.entry(id).or_default().1[input.index()];
        if *told == hash {
            return;
        }
        *told = hash;
        if let (Some(Some(renderer)), Some(view)) = (&self.renderer, self.views.get_mut(&id)) {
            view.set_input(renderer, input, picture);
        }
    }

    /// View `id`'s transition starts (its `from` picture came first);
    /// whether it runs. Without a device none does: its children show as
    /// they are.
    pub(crate) fn start_transition(&mut self, id: u32, duration: Duration) -> bool {
        if self.renderer().is_none() {
            return false;
        }
        let Some(view) = self.views.get_mut(&id).filter(|v| v.layer().is_some()) else {
            return false;
        };
        view.start_transition(duration, Instant::now());
        true
    }

    /// A page or a dialog comes or goes (`reverse`) through `page`'s
    /// `transitionShader` over the view `key` (the stack or the dialog) of
    /// `width` × `height` pixels at `density`: `from` is its lower side (the
    /// page under it, the window without the dialog), `to` its upper side
    /// (each `(pixels, stride)` at that size). Whether it runs: not without
    /// a device, so the page or the dialog comes with its own animation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn nav_transition(
        &mut self,
        key: u32,
        page: u32,
        size_px: (i32, i32),
        density: f32,
        from: (&[u8], u32),
        to: (&[u8], u32),
        reverse: bool,
    ) -> bool {
        let Some(nav) = self.nav.get(&page).cloned() else {
            return false;
        };
        // The pictures cover the host's box; progress moves as an animated
        // value (one more slot).
        let layer = (*nav.layer)
            .clone()
            .with_overflow(Points::ZERO)
            .with_progress(Percentage::ZERO, Some(NodeId::MAX));
        self.renderer();
        let (Some(slot), Some(Some(renderer))) = (layer.progress_slot(), &self.renderer) else {
            return false;
        };
        let (w, h) = (size_px.0.max(0) as u32, size_px.1.max(0) as u32);
        let mut view = ShaderView::default();
        view.set_layer(Some(Arc::new(layer)));
        let d = density.max(f32::MIN_POSITIVE);
        view.set_size(size(Points(w as f32 / d), Points(h as f32 / d)), d);
        view.set_slot(
            slot,
            AnimationValue::Number(if reverse { 1.0 } else { 0.0 }),
        );
        view.set_input(renderer, ShaderInput::From, Some((from.0, w, h, from.1)));
        view.set_input(renderer, ShaderInput::To, Some((to.0, w, h, to.1)));
        self.views.insert(key, view);
        self.navs.insert(
            key,
            NavRun {
                start: Instant::now(),
                duration: nav.duration,
                reverse,
                slot,
            },
        );
        true
    }

    /// Mounts `layer` on view `id` (`None`: it shows nothing of its own).
    pub(crate) fn set(&mut self, id: u32, layer: Option<Arc<ShaderLayer>>) {
        if layer.is_none() && !self.views.contains_key(&id) {
            return;
        }
        if layer.is_some() {
            self.renderer();
        }
        if layer.is_none() {
            self.told.remove(&id);
        }
        self.views.entry(id).or_default().set_layer(layer);
    }

    /// View `id` is `width` × `height` pixels at `density` pixels per point
    /// (Java reports it before the layer arrives, so this may come first).
    pub(crate) fn resized(&mut self, id: u32, width: i32, height: i32, density: f32) {
        let density = density.max(f32::MIN_POSITIVE);
        self.views.entry(id).or_default().set_size(
            size(
                Points(width.max(0) as f32 / density),
                Points(height.max(0) as f32 / density),
            ),
            density,
        );
    }

    /// View `id` is gone.
    pub(crate) fn delete(&mut self, id: u32) {
        self.views.remove(&id);
        self.told.remove(&id);
        self.nav.remove(&id);
        self.navs.remove(&id);
    }

    /// Whether view `id` shows (or showed) a shader: its animated values are
    /// written here, never sent to Java.
    pub(crate) fn contains(&self, id: u32) -> bool {
        self.views.contains_key(&id)
    }

    /// The views, to write their animated slots.
    pub(crate) fn views_mut(&mut self) -> impl Iterator<Item = (u32, &mut ShaderView)> {
        self.views.iter_mut().map(|(&id, view)| (id, view))
    }

    /// Whether a view has a frame to render or collect.
    pub(crate) fn wants_frame(&self) -> bool {
        matches!(self.renderer, Some(Some(_)))
            && (!self.navs.is_empty() || self.views.values().any(ShaderView::wants_frame))
    }

    /// One `Choreographer` frame: renders the views that have a frame due,
    /// hands those that came back to `nsh_view` (the `NshView`); whether
    /// one wants another frame.
    pub(crate) fn render(&mut self, nsh_view: &JObject<'_>) -> bool {
        let Some(Some(renderer)) = &self.renderer else {
            return false;
        };
        let now = Instant::now();
        let mut more = false;
        let mut ended = Vec::new();
        for (&id, view) in &mut self.views {
            if let Some(run) = self.navs.get(&id) {
                // A page or a dialog: frames while its clock runs, and one
                // that replaces nothing when it ends (Java settles it). Until
                // the first frame comes back, what it covers shows as it is,
                // so an empty frame is never taken for its end.
                let t = run.at(now);
                if t >= 1.0 {
                    deliver(nsh_view, id, &Frame::default(), false, 0.0);
                    ended.push(id);
                    continue;
                }
                let progress = if run.reverse { 1.0 - t } else { t };
                view.set_slot(run.slot, AnimationValue::Number(progress));
                if view.wants_frame() && view.render(renderer, now) && view.frame().width > 0 {
                    deliver(nsh_view, id, view.frame(), true, view.overflow().0);
                }
                more = true;
                continue;
            }
            // Until its pictures came, the children show as they are.
            let replaces = view.replaces_children(now) && pictured(view);
            let told = self.told.entry(id).or_default();
            let flipped = told.0 != replaces;
            told.0 = replaces;
            let fresh = view.wants_frame() && view.render(renderer, now);
            if fresh || flipped {
                let frame = view.frame();
                deliver(
                    nsh_view,
                    id,
                    frame,
                    replaces && frame.width > 0,
                    view.overflow().0,
                );
            }
            more |= view.wants_frame();
        }
        for id in ended {
            self.delete(id);
        }
        more
    }
}

/// Whether `view` has the pictures of what it replaces (its children's or
/// a transition's sides) that its shader reads.
fn pictured(view: &ShaderView) -> bool {
    view.layer().is_some_and(|layer| {
        layer
            .shader
            .inputs()
            .iter()
            .filter(|&i| i != ShaderInput::Backdrop)
            .all(|i| view.has_input(i))
    })
}

/// `NshView.shaderFrame(id, pixels, width, height, replaces, overflow)`:
/// Java copies the pixels into the view's bitmap (null pixels: an empty
/// frame); `replaces` says whether it shows instead of the children,
/// `overflow` how far beyond the box it reaches, in points.
fn deliver(nsh_view: &JObject<'_>, id: u32, frame: &Frame, replaces: bool, overflow: f32) {
    let pixels = frame.tight();
    let (width, height) = (frame.width as i32, frame.height as i32);
    with_env(|env| {
        let buffer = if width > 0 && height > 0 && !pixels.is_empty() {
            // SAFETY: `pixels` is a live, non-empty allocation of this
            // length that outlives the call below; Java only reads it, and
            // copies it out before `shaderFrame` returns without keeping
            // the buffer.
            JObject::from(unsafe {
                env.new_direct_byte_buffer(pixels.as_ptr().cast_mut(), pixels.len())
            }?)
        } else {
            JObject::null()
        };
        // SAFETY: matches the signature checked in `java::init`.
        unsafe {
            env.call_method_unchecked(
                nsh_view,
                java().view_shader_frame,
                ReturnType::Primitive(Primitive::Void),
                &[
                    JValue::Int(id as i32).as_jni(),
                    JValue::Object(&buffer).as_jni(),
                    JValue::Int(width).as_jni(),
                    JValue::Int(height).as_jni(),
                    JValue::Bool(replaces.into()).as_jni(),
                    JValue::Float(overflow).as_jni(),
                ],
            )
        }
        .map(|_| ())
    });
}
