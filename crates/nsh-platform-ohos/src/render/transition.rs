//! UI-thread layout/style transitions and keyframes, delegating to
//! `nsh_core::transition` and driven by the surface's native frame clock.
pub(crate) use nsh_core::transition::Extra;
use nsh_core::{Bounds, LayoutAnimation, LayoutTransition, Points, ViewStyle, transition};

use crate::render::geometry::Rect;

#[derive(Default)]
pub(crate) struct Live {
    inner: transition::Live,
    target_style: Option<Box<ViewStyle>>,
}

pub(crate) struct Step {
    pub(crate) frame: Option<Rect>,
    pub(crate) opacity: f32,
    pub(crate) extra: Extra,
    pub(crate) active: bool,
    pub(crate) removed: bool,
}

impl Live {
    pub(crate) fn finish(&mut self, style: &mut ViewStyle) -> Step {
        let frame = self.inner.frame_target().map(Rect::from);
        if let Some(target) = self.target_style.take() {
            *style = *target;
        }
        let removed = self.inner.dying();
        self.inner = transition::Live::default();
        Step {
            frame,
            opacity: 1.0,
            extra: Extra::NONE,
            active: false,
            removed,
        }
    }

    pub(crate) fn frame_target(&self) -> Option<Rect> {
        self.inner.frame_target().map(Rect::from)
    }

    pub(crate) fn frame(
        &mut self,
        from: Rect,
        to: Rect,
        animation: Option<&LayoutAnimation>,
    ) -> bool {
        if let Some(t) = animation
            .and_then(|a| a.update)
            .filter(|_| from != Rect::default() && from != to)
        {
            self.inner
                .move_frame(Bounds::<Points>::from(from), Bounds::<Points>::from(to), t);
            true
        } else {
            self.inner.stop_frame();
            false
        }
    }

    pub(crate) fn style(
        &mut self,
        out: &mut ViewStyle,
        old: ViewStyle,
        reduce: bool,
        initialized: bool,
    ) {
        if reduce {
            let _ = self.finish(&mut ViewStyle::default());
            return;
        }
        let mut boxed = Box::new(out.clone());
        let prev = if initialized {
            old
        } else {
            ViewStyle::default()
        };
        if let Some(restyle) = transition::restyle(&prev, &mut boxed, false) {
            self.target_style = Some(Box::new(out.clone()));
            *out = *boxed;
            self.inner.restyle(restyle);
        } else {
            self.target_style = None;
            self.inner.stop_style();
        }
    }

    pub(crate) fn appear(&mut self, transition: LayoutTransition) -> (f32, Extra) {
        self.inner.appear(transition);
        (
            self.inner.opacity(),
            self.inner.extra().unwrap_or(Extra::NONE),
        )
    }

    pub(crate) fn disappear(&mut self, transition: LayoutTransition) {
        self.inner.disappear(transition);
    }

    pub(crate) fn cancel_removal(&mut self) {
        self.inner.revive();
    }

    pub(crate) fn step(&mut self, now: f64, current_frame: Rect, style: &mut ViewStyle) -> Step {
        let prev_bounds = Bounds::<Points>::from(current_frame);
        let mut bounds = prev_bounds;
        let had_frame = self.inner.frame_target().is_some();
        let mut boxed_style = Box::new(style.clone());
        let s = self.inner.step(now, &mut bounds, &mut boxed_style);
        if s.restyled {
            *style = *boxed_style;
            if self.inner.idle() {
                self.target_style = None;
            }
        }
        let frame_done = had_frame && self.inner.frame_target().is_none();
        Step {
            frame: (had_frame && (bounds != prev_bounds || frame_done)).then(|| Rect::from(bounds)),
            opacity: self.inner.opacity(),
            extra: self.inner.extra().unwrap_or(Extra::NONE),
            active: !s.finished && !self.inner.idle(),
            removed: s.removed,
        }
    }
}
