//! `Scrollbar`: a scroll bar of the app's own look over a scroll view or a
//! list it names, placed by the app like any other view (`gpui-base`'s
//! `scrollbar.rs`, `Scrollbar::new(&scroll_handle)`).
//!
//! The bar watches its target (`watch_scroll`): the platforms report every
//! scroll view's offset and the core lays out its content, so the thumb's
//! length and place follow the content, the viewport and the offset, also
//! while a list grows or the window resizes. Dragging the thumb, or
//! pressing the track, scrolls the target (`ScrollTo` commands). The
//! target's own indicator is the app's to turn off (`scrollbar={false}`).

use std::time::Duration;

use nsh_core::{
    NodeId,
    element::{
        ElementRef, JsHost, JsIn, JsOwned, JsValue, NodeRef, PropError, ScrollEvent, Style, TsTypes,
    },
    geometry::{Along, Axis},
    prelude::*,
    time::{Instant, now},
};

/// What a [`Scrollbar`] moves: a scroll view or a list (TS: its `ref`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ScrollbarTarget {
    /// None yet: the bar shows nothing.
    #[default]
    None,
    /// The element a Rust view's ref is on, found when it is mounted.
    Ref(ElementRef),
    /// A node JS passes (a `ref`).
    Node(NodeRef),
}

impl ScrollbarTarget {
    /// Its node, while one is mounted.
    pub fn node(&self) -> Option<NodeId> {
        match self {
            Self::None => None,
            Self::Ref(r) => r.node(),
            Self::Node(n) => Some(n.0),
        }
    }
}

impl From<ElementRef> for ScrollbarTarget {
    fn from(r: ElementRef) -> Self {
        Self::Ref(r)
    }
}

impl From<&ElementRef> for ScrollbarTarget {
    fn from(r: &ElementRef) -> Self {
        Self::Ref(*r)
    }
}

impl From<NodeRef> for ScrollbarTarget {
    fn from(n: NodeRef) -> Self {
        Self::Node(n)
    }
}

impl JsValue for ScrollbarTarget {
    fn ts_type(types: &mut TsTypes) -> String {
        NodeRef::ts_type(types)
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        Ok(Option::<NodeRef>::from_js(value)?.map_or(Self::None, Self::Node))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self.node() {
            Some(node) => NodeRef(node).to_js(host),
            None => Option::<NodeRef>::None.to_js(host),
        }
    }
}

/// When a [`Scrollbar`] shows (`gpui-base`'s `ScrollbarMode`). A drag of
/// its thumb, and the pointer on its track, keep it shown in every mode;
/// it never shows while the content fits the viewport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum ScrollbarMode {
    /// While the target scrolls, until `hideDelay` after it stops.
    #[default]
    Scrolling,
    /// While the pointer is over the target (or the bar), and until
    /// `hideDelay` after it scrolls.
    Hover,
    /// Always.
    Always,
}

/// The props of [`Scrollbar`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct ScrollbarProps {
    /// The scroll view or list it follows and moves (TS: its `ref`; a
    /// Rust view's [`ElementRef`]). Read in every commit, so a ref set
    /// later in the same render is found once its element is mounted.
    #[prop(default)]
    pub scroll: Prop<ScrollbarTarget>,
    /// The axis of the target (default `"vertical"`): the bar's track
    /// runs along it. Read when the bar is made.
    #[prop(default = Axis::Vertical)]
    pub axis: Prop<Axis>,
    /// When it shows (default `"scrolling"`).
    #[prop(default)]
    pub mode: Prop<ScrollbarMode>,
    /// How long it stays after the target stops scrolling, the pointer
    /// leaves or a drag ends, in milliseconds (default `1000`).
    #[prop(default = 1000.0)]
    pub hide_delay: Prop<f64>,
    /// The shortest the thumb gets on a long content (default `24`), so it
    /// stays easy to grab.
    #[prop(default = px(24.0))]
    pub min_thumb_length: Prop<Points>,
    /// The Tailwind classes of the thumb, inside the track's padding and
    /// sized along the axis by the bar (`w-full rounded-full
    /// bg-foreground/30`); `hover:` under the pointer, `active:` while it
    /// is dragged.
    pub thumb_class: Prop<Option<SharedString>>,
    /// The style of the thumb, over `thumbClass`.
    #[prop(default)]
    pub thumb_style: Prop<Style>,
    /// What screen readers say of the bar.
    pub accessibility_label: Prop<Option<SharedString>>,
}

/// A thumb's drag, from the grant to the release.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    /// The target's offset along the axis when it started.
    from: f32,
}

/// What the bar's parts and listeners share.
#[derive(Clone, Copy)]
struct Ctl {
    scroll: Prop<ScrollbarTarget>,
    axis: Axis,
    min_thumb: Prop<Points>,
    hide_delay: Prop<f64>,
    /// What the target reported last.
    metrics: Signal<Option<ScrollEvent>>,
    /// The rail's length along the axis: the track's content box.
    rail: Signal<f32>,
    /// Where the rail starts in the track.
    rail_origin: Stored<Point<Points>>,
    /// The target scrolled a moment ago (until `hide_delay` after).
    active: Signal<bool>,
    /// The pointer is over the target.
    over_target: Signal<bool>,
    /// The pointer is over the track.
    over_track: Signal<bool>,
    drag: Signal<Option<Drag>>,
    /// When the bar hides, unless activity comes first: one timer runs at a
    /// time and waits again while activity has moved this on (scrolling
    /// reports every frame).
    deadline: Stored<Option<Instant>>,
}

impl Ctl {
    fn node(self) -> Option<NodeId> {
        self.scroll.get_untracked().node()
    }

    /// The viewport, the content and the offset along the axis (tracked).
    fn along(self) -> Option<(f32, f32, f32)> {
        let m = self.metrics.get()?;
        Some((
            m.size.along(self.axis).0,
            m.content_size.along(self.axis).0,
            m.offset.along(self.axis).0,
        ))
    }

    /// How far the target scrolls along the axis (tracked).
    fn range(self) -> f32 {
        self.along()
            .map_or(0.0, |(view, content, _)| (content - view).max(0.0))
    }

    /// The content overflows the viewport (tracked).
    fn scrolls(self) -> bool {
        self.range() > 0.5
    }

    /// The thumb's length (tracked).
    fn thumb(self) -> f32 {
        let rail = self.rail.get();
        let Some((view, content, _)) = self.along() else {
            return rail;
        };
        let min = self.min_thumb.get().0.min(rail);
        (rail * view / content.max(1.0)).clamp(min, rail)
    }

    /// Where the thumb starts along the rail (tracked): the offset, held
    /// to the content, so overscrolls leave it at an end.
    fn position(self) -> f32 {
        let range = self.range();
        let Some((_, _, offset)) = self.along() else {
            return 0.0;
        };
        if range <= 0.0 {
            return 0.0;
        }
        (self.rail.get() - self.thumb()).max(0.0) * (offset / range).clamp(0.0, 1.0)
    }

    /// The target's offset that puts the thumb at `position` on the rail.
    fn offset_at(self, position: f32) -> f32 {
        untracked(|| {
            let travel = (self.rail.get() - self.thumb()).max(0.0);
            if travel <= 0.0 {
                return 0.0;
            }
            (position / travel).clamp(0.0, 1.0) * self.range()
        })
    }

    /// Scrolls the target to `offset` along the axis.
    fn scroll_to(self, offset: f32) {
        let Some(node) = self.node() else { return };
        let Some(now) = self.metrics.get_untracked() else {
            return;
        };
        let mut to = now.offset;
        match self.axis {
            Axis::Horizontal => to.x = px(offset),
            Axis::Vertical => to.y = px(offset),
        }
        NodeRef(node).scroll_to(to, false);
    }

    /// Something kept the bar shown: it hides `hide_delay` after the last.
    fn touch(self) {
        if !self.active.get_untracked() {
            self.active.set(true);
        }
        let delay = Duration::from_secs_f64(self.hide_delay.get_untracked().max(0.0) / 1000.0);
        if self.deadline.replace(Some(now() + delay)).is_none() {
            self.hide_after(delay);
        }
    }

    /// Hides the bar at the deadline, waiting again while it moved on.
    fn hide_after(self, delay: Duration) {
        set_timeout(delay, move || {
            let Some(deadline) = self.deadline.get() else {
                return;
            };
            let left = deadline.saturating_duration_since(now());
            if left.is_zero() {
                self.deadline.replace(None);
                self.active.set(false);
            } else {
                self.hide_after(left);
            }
        });
    }

    /// The bar shows (tracked).
    fn shown(self, mode: ScrollbarMode) -> bool {
        if !self.scrolls() {
            return false;
        }
        let held = self.drag.get().is_some() || self.over_track.get();
        held || match mode {
            ScrollbarMode::Always => true,
            ScrollbarMode::Scrolling => self.active.get(),
            ScrollbarMode::Hover => self.over_target.get() || self.active.get(),
        }
    }

    /// A press on the track at `at` (in the track): the thumb's middle
    /// jumps there, and a drag goes on from there.
    fn press_track(self, at: Point<Points>) {
        let along = at.along(self.axis).0 - self.rail_origin.get().along(self.axis).0;
        let thumb = untracked(|| self.thumb());
        let offset = self.offset_at(along - thumb / 2.0);
        self.drag.set(Some(Drag { from: offset }));
        self.scroll_to(offset);
    }

    /// A press on the thumb: a drag moves it by the pointer's offset.
    fn press_thumb(self) {
        let from = untracked(|| self.along()).map_or(0.0, |(_, _, o)| o);
        self.drag.set(Some(Drag {
            from: from.clamp(0.0, untracked(|| self.range())),
        }));
    }

    /// The drag moved `delta` points along the axis.
    fn drag_by(self, delta: f32) {
        let Some(drag) = self.drag.get_untracked() else {
            return;
        };
        let (travel, range) =
            untracked(|| ((self.rail.get() - self.thumb()).max(0.0), self.range()));
        if travel <= 0.0 {
            return;
        }
        let offset = (drag.from + delta * range / travel).clamp(0.0, range);
        self.scroll_to(offset);
    }

    fn release(self) {
        if self.drag.get_untracked().is_some() {
            self.drag.set(None);
            self.touch();
        }
    }
}

/// A scroll bar of the app's own look for a scroll view or a list it
/// names (`scroll`), placed by the app like any other view: over the
/// target's edge, beside a fixed header, along two panes. It owns its
/// behavior (the thumb's length and place follow the target's content,
/// viewport and offset; dragging the thumb or pressing the track scrolls
/// the target; when it shows, by `mode`) and leaves its look to the app:
/// no colors, sizes, radii or motion of its own. The app's `class` and
/// `style` style the bar, its track: placed by the app (`absolute inset-y-0
/// right-0 w-2.5 p-0.5`), the area that takes presses (a press scrolls the
/// target to that point and a drag goes on from there), a `group` for the
/// thumb's `group-hover:`, and hidden while not shown (`opacity: 0`, taking
/// no presses: `transition-opacity` fades it). `thumbClass` / `thumbStyle`
/// style the thumb, inside the track's padding and sized along the axis by
/// the bar. Turn the target's own indicator off with `scrollbar={false}`.
#[component]
pub fn Scrollbar(props: ScrollbarProps, _cx: Scope<NoEvent>) -> impl IntoElement {
    let ctl = Ctl {
        scroll: props.scroll,
        axis: props.axis.get_untracked(),
        min_thumb: props.min_thumb_length,
        hide_delay: props.hide_delay,
        metrics: signal(None),
        rail: signal(0.0),
        rail_origin: stored(Point::default()),
        active: signal(false),
        over_target: signal(false),
        over_track: signal(false),
        drag: signal(None),
        deadline: stored(None),
    };
    let target = move || ctl.node();
    watch_scroll(target, move |e: &ScrollEvent| {
        let before = ctl.metrics.get_untracked();
        ctl.metrics.set(Some(*e));
        // A move (not the first report, nor a resize) shows the bar.
        if before.is_some_and(|b| b.offset != e.offset) {
            ctl.touch();
        }
    });
    watch_hover(target, move |inside| {
        ctl.over_target.set(inside);
        if !inside {
            ctl.touch();
        }
    });
    let mode = props.mode;
    let shown = computed(move || ctl.shown(mode.get()));
    let axis = ctl.axis;
    let thumb = view()
        .flex_shrink_0()
        .map(|v| match axis {
            Axis::Vertical => v
                .h(move || Length::from(px(ctl.thumb())))
                .translate_y(move || DefiniteLength::from(px(ctl.position()))),
            Axis::Horizontal => v
                .w(move || Length::from(px(ctl.thumb())))
                .translate_x(move || DefiniteLength::from(px(ctl.position()))),
        })
        .claim_touches(TouchClaim::on_start().ask(|request: &ClaimRequest| request.claim()))
        // A scroll view around it does not take the drag over.
        .keep_touches(true)
        .on_responder_grant(move |_: &ResponderEvent| ctl.press_thumb())
        .on_responder_move(move |e: &ResponderEvent| ctl.drag_by(e.gesture.delta.along(axis).0))
        .on_responder_release(move |_: &ResponderEvent| ctl.release())
        .on_responder_terminate(move |_: &ResponderEvent| ctl.release())
        .app_class(props.thumb_class)
        .inline_style(props.thumb_style);
    // The thumb's room: the track's content box, measured.
    let rail = view()
        .flex_1()
        .map(|v| match axis {
            Axis::Vertical => v.flex_col(),
            Axis::Horizontal => v.flex_row(),
        })
        .pointer_events(PointerEvents::BoxNone)
        .on_layout(move |frame: &Bounds<Points>| {
            ctl.rail_origin.replace(frame.origin);
            ctl.rail.set(frame.size.along(axis).0);
        })
        .child(thumb);
    view()
        .group()
        .map(|v| match axis {
            Axis::Vertical => v.flex_col(),
            Axis::Horizontal => v.flex_row(),
        })
        .optional_label(move || props.accessibility_label.get())
        .on_mouse_enter(move |_: &MouseEvent| ctl.over_track.set(true))
        .on_mouse_leave(move |_: &MouseEvent| {
            ctl.over_track.set(false);
            ctl.touch();
        })
        .claim_touches(TouchClaim::on_start().ask(|request: &ClaimRequest| request.claim()))
        .keep_touches(true)
        .on_responder_grant(move |e: &ResponderEvent| ctl.press_track(e.location))
        .on_responder_move(move |e: &ResponderEvent| ctl.drag_by(e.gesture.delta.along(axis).0))
        .on_responder_release(move |_: &ResponderEvent| ctl.release())
        .on_responder_terminate(move |_: &ResponderEvent| ctl.release())
        .style_with(move |s| if shown.get() { s } else { s.opacity(0.0) })
        .pointer_events(move || {
            if shown.get() {
                PointerEvents::Auto
            } else {
                PointerEvents::None
            }
        })
        .child(rail)
}
