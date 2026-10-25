//! `nsh/base/plot`: the parts charts are built from, as `gpui-base`'s
//! `plot` module, with no look of their own. A [`Plot`] holds the rows of
//! data, the scales of its two axes, the pointer over it and its motion;
//! its marks ([`Line`], [`Area`], [`Grid`]) draw what it holds, each one
//! canvas filling the plot, natively: the rows are read once when the app
//! sets them, scaled, recorded and drawn in Rust, so no JS runs per point,
//! and the pointer, the hover's glide and fade and the appear run in Rust
//! and on the platform's animator (the app hears of a new index, nothing
//! more). Marks paint with SVG's paint properties, CSS's on any element
//! (`fill-*`, `stroke-*`, `strokeWidth`, `strokeDasharray`),
//! `currentColor` where the app sets none; the hover's cross line, dots
//! and tooltip are parts the app styles and fills.
//!
//! A module of its own (`nsh/base/plot`, a submodule of the `nsh/base`
//! set) because `Line`, `Area` and `Grid` only make sense inside a plot.

mod area;
mod grid;
mod line;
mod scale;
mod series;

use std::sync::Arc;

use nsh_core::{
    AccessibilityValue, PatchValue, StyleDecl, StyleKey,
    ui::{Registry, RowAccessor, Rows, Spring, Style, Timing, prelude::*},
};

pub use self::{
    area::{Area, AreaElement, AreaProps, area},
    grid::{Grid, GridElement, GridProps, grid},
    line::{Curve, Line, LineElement, LineProps, line},
    scale::Scale,
};
use self::{
    scale::{Linear, extent, point_tick},
    series::Mark,
};

/// The JS module apps import the plot parts as.
pub const SPECIFIER: &str = "nsh/base/plot";

/// The components of `nsh/base/plot`, served with `nsh/base`
/// ([`Registry::submodule`]).
pub fn registry() -> Registry {
    let mut registry = Registry::ui_set(SPECIFIER);
    registry
        .register::<Plot>()
        .register::<Line>()
        .register::<Area>()
        .register::<Grid>();
    registry
}

/// The timing of a plot's motion (`gpui-base`'s `PlotMotion`): how its
/// marks appear, how the hover fades in and out, and the spring the hover
/// follows the pointer with. Each is the app's; none moves at once, as
/// `gpui-base`'s zero durations do, and so does every one while the user
/// asks for reduced motion.
#[derive(Debug, Clone, Copy, Default, PartialEq, JsObject)]
pub struct PlotMotion {
    /// The spring the cross line, the dots and the tooltip follow the row
    /// under the pointer with (`with_pointer`); none: they move to it at
    /// once.
    pub pointer: Option<Spring>,
    /// How the hover fades in when the pointer comes over the plot
    /// (`with_enter`).
    pub enter: Option<Timing>,
    /// How the hover fades out after the pointer leaves (`with_exit`).
    pub exit: Option<Timing>,
    /// How the marks draw in, from the leading edge (`with_appear`): when
    /// the plot is first shown with an `appearGeneration`, and again each
    /// time that changes.
    pub appear: Option<Timing>,
}

/// What screen readers hear as the plot's value (TS `{ min?, max?, now?,
/// text? }`, as a view's `accessibilityValue`): the app's words for the
/// row under the pointer, so a screen reader says the value, not only the
/// label.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct PlotAccessibilityValue {
    /// The smallest value of a range.
    pub min: Option<f64>,
    /// The largest value of a range.
    pub max: Option<f64>,
    /// The value now.
    pub now: Option<f64>,
    /// The value in words (`"Thursday, 2,480 visitors"`).
    pub text: Option<SharedString>,
}

/// The props of [`Plot`].
#[derive(Debug, Clone, PartialEq, Props)]
#[props(generic)]
pub struct PlotProps {
    /// The rows of data, one object per point (`gpui-base`'s `data`): the
    /// marks read their values by field name or through the app's
    /// functions, once, when the app sets the rows. A row without a value
    /// is skipped, as `gpui-base` skips it.
    pub data: Prop<Rows>,
    /// What places a row along the x axis on a `linear` x scale
    /// (`gpui-base`'s `x`): a field name, or a function of the row and its
    /// index; without one, a row's index. A `point` scale places the rows
    /// in order.
    pub x: Prop<Option<RowAccessor>>,
    /// What the marks draw along the y axis unless they name their own
    /// (`gpui-base`'s `y`, `Area`'s `y1`): a field name or a function.
    pub y: Prop<Option<RowAccessor>>,
    /// How the x axis places rows: `{ type: "point" }` (the default, the
    /// rows in order, evenly from the left edge to the right one) or `{
    /// type: "linear", domain? }`.
    pub x_scale: Prop<Option<Scale>>,
    /// How the y axis places values: `{ type: "linear", domain? }` (the
    /// default; without a domain, the extent of what the marks draw), the
    /// domain's smallest value at the bottom edge and its largest at the
    /// top one.
    pub y_scale: Prop<Option<Scale>>,
    /// The plot follows the pointer (`gpui-base`'s `Plot::interactive`;
    /// default true): it reports the row under it and shows the hover's
    /// parts. False draws the marks only (a sparkline in a list).
    #[prop(default = true)]
    pub interactive: Prop<bool>,
    /// The timing of its motion: the appear, the hover's fade and the
    /// pointer's spring; none by default, so everything shows at once.
    pub motion: Prop<Option<PlotMotion>>,
    /// Opts in to the appear (`gpui-base`'s `Plot::appear_generation`):
    /// the marks draw in over `motion.appear` when the plot is first shown,
    /// and again whenever this changes (another symbol, another period).
    pub appear_generation: Prop<Option<f64>>,
    /// What screen readers say of the plot: what it shows and what stands
    /// out (it is one element to them, its marks are not).
    pub accessibility_label: Prop<Option<SharedString>>,
    /// What screen readers hear as its value: the app's words for the row
    /// under the pointer.
    pub accessibility_value: Prop<Option<PlotAccessibilityValue>>,
    /// The Tailwind classes of the cross line (`gpui-base`'s
    /// `TooltipState::cross_line`): a view as tall as the plot, centered on
    /// the row under the pointer (its width and color the app's), shown
    /// while the pointer is over the plot.
    pub cross_line_class: Prop<Option<SharedString>>,
    /// The style of the cross line, over `crossLineClass`.
    #[prop(default)]
    pub cross_line_style: Prop<Style>,
    /// The Tailwind classes of the hover's dots (`TooltipState::dots`): one
    /// view per `Line` and `Area`, centered on its point at the row under
    /// the pointer (its size, color and border the app's).
    pub dot_class: Prop<Option<SharedString>>,
    /// The style of the hover's dots, over `dotClass`.
    #[prop(default)]
    pub dot_style: Prop<Style>,
    /// The app's tooltip (`gpui-base`'s `Plot::tooltip`): placed at the
    /// top of the plot, centered on the cross line, and moved and faded
    /// with it; its content is the app's, for the row `onHoverChange`
    /// reported.
    pub tooltip_content: Prop<Option<Slot>>,
    /// The marks, drawn in order over each other (each fills the plot),
    /// and whatever the app lays over them.
    pub children: Prop<Children>,
}

/// What [`Plot`] reports.
#[derive(Debug, Clone, PartialEq, Event)]
pub enum PlotEvent {
    /// The pointer came over another row (`gpui-base`'s
    /// `TooltipState::index`): the index of the row nearest to it along
    /// the x axis, `null` once it leaves the plot.
    #[args(index)]
    HoverChange(Option<u32>),
}

/// Where values go along a plot's x axis.
#[derive(Debug, Clone, Copy, PartialEq)]
enum XMap {
    /// Row indices, evenly over `range` (fractions between rows).
    Point { len: usize, range: [f32; 2] },
    /// Values in proportion.
    Linear(Linear),
}

impl XMap {
    fn tick(&self, value: f64) -> Option<f32> {
        match *self {
            XMap::Point { len, range } => match len {
                0 => None,
                _ if !value.is_finite() => None,
                1 => point_tick(0, 1, range),
                _ => Some(range[0] + value as f32 * (range[1] - range[0]) / (len - 1) as f32),
            },
            XMap::Linear(map) => map.tick(value),
        }
    }

    /// Where the rows `values` go (a row's index without values).
    fn ticks(&self, len: usize, values: Option<&[f64]>) -> Arc<[f32]> {
        let at = |v: f64| self.tick(v).unwrap_or(f32::NAN);
        match (self, values) {
            (XMap::Linear(_), Some(values)) => values.iter().map(|&v| at(v)).collect(),
            _ => (0..len).map(|i| at(i as f64)).collect(),
        }
    }
}

/// The plot's state, for its marks.
#[derive(Clone, Copy)]
struct PlotContext {
    rows: Prop<Rows>,
    y: Prop<Option<RowAccessor>>,
    /// Where values go along the x axis.
    x_map: Memo<XMap>,
    /// Where each of the plot's rows goes along the x axis (NaN: nowhere).
    xs: Memo<Arc<[f32]>>,
    /// Where values go along the y axis.
    y_map: Memo<Linear>,
    /// The plot's size.
    size: Signal<Size<Points>>,
    /// Its marks that count in the y domain and show a hover dot.
    marks: Signal<Vec<Mark>>,
    /// The next mark's key.
    next: Stored<u64>,
    /// How far the marks have drawn in, 0 to 1, while they appear (the
    /// appear's own value, on the platform's animator).
    reveal: AnimatedValue,
    /// The appear runs: the marks clip to `reveal`.
    appearing: Signal<bool>,
}

impl PlotContext {
    /// The plot has been laid out with an area to draw on: before that, a
    /// mark records nothing, so the first frame sends each drawing once,
    /// at the plot's size.
    fn is_sized(&self) -> bool {
        let size = self.size.get();
        size.width.0 > 0.0 && size.height.0 > 0.0
    }

    /// The plot a mark is in (`None`: a mark outside any).
    fn of(mark: &str) -> Option<Self> {
        let plot = use_context::<PlotContext>();
        if plot.is_none() {
            log::warn!("`{mark}` draws only inside a `Plot`");
        }
        plot
    }

    /// Adds `mark` while the current owner lives.
    fn add(&self, make: impl FnOnce(u64) -> Mark) {
        let key = self.next.get();
        self.next.replace(key + 1);
        let marks = self.marks;
        marks.update(|all| all.push(make(key)));
        cleanup(move || {
            if let Some(mut all) = marks.try_get() {
                all.retain(|m| m.key != key);
                marks.try_set(all);
            }
        });
    }

    /// Records the clip of the appear while it runs (before what a mark
    /// draws): the marks show from the leading edge to `reveal` of the
    /// plot's width.
    fn clip(&self, p: Painter, bleed: f32) -> Painter {
        if !self.appearing.get() {
            return p;
        }
        let size = self.size.get();
        let edge = self.reveal.mul(size.width.0);
        // What the mark draws past the plot's edges (`bleed`) is revealed
        // with the rest, from the leading edge.
        p.begin_path()
            .move_to(-bleed, -bleed)
            .line_to(&edge, -bleed)
            .line_to(&edge, size.height.0 + bleed)
            .line_to(-bleed, size.height.0 + bleed)
            .close_path()
            .clip()
    }
}

/// A plot (`gpui-base`'s `Plot`): the rows of data a chart shows, the
/// scales of its x and y axes, its size as their range, the pointer over
/// it and its motion. It draws nothing itself: its marks, the children
/// (`Line`, `Area`, `Grid`), each fill it and draw in order over each
/// other. It reports the row nearest to the pointer (`onHoverChange`) and
/// shows the hover's parts the app styles (`crossLineClass`, `dotClass`,
/// `tooltipContent`), moved and faded natively with `motion`. It is one
/// element for screen readers, with the image role, the app's
/// `accessibilityLabel` and `accessibilityValue`. The app's `class` and
/// `style` size and style it; the marks' paint inherits from it as CSS
/// inherits `fill` and `stroke`.
#[component]
pub fn Plot(props: PlotProps, cx: Scope<PlotEvent>) -> impl IntoElement {
    let size = signal(Size::<Points>::default());
    let marks = signal(Vec::<Mark>::new());
    let (data, x, x_scale, y_scale) = (props.data, props.x, props.x_scale, props.y_scale);
    let motion = props.motion;
    // A row's value along the x axis: its `x` on a linear scale, else its
    // index.
    let x_values = series::values(move || data.get(), move || x.get());
    let x_map = computed(move || {
        let range = [0.0, size.get().width.0];
        let len = data.with(|rows| rows.len());
        match x_scale.get().unwrap_or(Scale::Point) {
            Scale::Point => XMap::Point { len, range },
            Scale::Linear { domain } => {
                let domain = domain.or_else(|| match x_values.get() {
                    Some(values) => extent(values.iter().copied()),
                    None => (len > 0).then(|| [0.0, (len - 1) as f64]),
                });
                XMap::Linear(Linear::new(domain.unwrap_or([0.0, 0.0]), range))
            }
        }
    });
    let xs = computed(move || {
        let len = data.with(|rows| rows.len());
        x_map.get().ticks(len, x_values.get().as_deref())
    });
    let y_map = computed(move || {
        let range = [size.get().height.0, 0.0];
        let domain = match y_scale.get() {
            Some(Scale::Linear {
                domain: Some(domain),
            }) => Some(domain),
            _ => marks.with(|all| {
                extent(
                    all.iter()
                        .filter_map(|m| m.extent.try_get().flatten())
                        .flatten(),
                )
            }),
        };
        Linear::new(domain.unwrap_or([0.0, 0.0]), range)
    });

    // The appear: from the first frame with a size, and again for each new
    // generation, over `motion.appear`; at once without one or with reduced
    // motion.
    let reveal = animated_value(1.0);
    let appearing = signal(false);
    let shown = computed(move || size.get().width.0 > 0.0);
    let generation = props.appear_generation;
    effect(move || {
        let Some(_) = generation.get() else { return };
        if !shown.get() {
            return;
        }
        let timing = untracked(|| motion.get().and_then(|m| m.appear));
        match timing {
            Some(timing) if timing.duration_ms > 0.0 && !reduce_motion() => {
                reveal.set(0.0);
                appearing.set(true);
                spawn(async move {
                    // Stopped by a newer generation: that one ends it.
                    if reveal.timing_with(1.0, timing).await {
                        appearing.set(false);
                    }
                });
            }
            _ => {
                reveal.set(1.0);
                appearing.set(false);
            }
        }
    });

    provide_context(PlotContext {
        rows: data,
        y: props.y,
        x_map,
        xs,
        y_map,
        size,
        marks,
        next: stored(0),
        reveal,
        appearing,
    });

    // The hover: where its parts are and how far they have faded in.
    let cross_x = animated_value(0.0);
    let fade = animated_value(0.0);
    let hovered = stored(None::<u32>);
    let follow = move |index: Option<u32>| {
        let was = hovered.get();
        if was == index {
            return;
        }
        hovered.replace(index);
        cx.emit(PlotEvent::HoverChange(index));
        let m = motion.get_untracked().unwrap_or_default();
        let still = reduce_motion();
        let Some(index) = index else {
            match m.exit.filter(|_| !still) {
                Some(exit) => drop(fade.timing_with(0.0, exit)),
                None => fade.set(0.0),
            }
            return;
        };
        let Some(x) = xs.with_untracked(|xs| xs.get(index as usize).copied()) else {
            return;
        };
        // Entering: the parts take the row where they are, then fade in;
        // over the plot, they glide to the next one on the pointer spring
        // (`PlotHover::glide`).
        let entering = was.is_none();
        let to = |value: AnimatedValue, target: f32| match m.pointer.filter(|_| !entering && !still)
        {
            Some(spring) => drop(value.spring(target, spring)),
            None => value.set(target),
        };
        to(cross_x, x);
        marks.with_untracked(|all| {
            for mark in all {
                if let Some(p) = mark.near(x) {
                    to(mark.dot_x, p.x.0);
                    to(mark.dot_y, p.y.0);
                }
            }
        });
        if entering {
            match m.enter.filter(|_| !still) {
                Some(enter) => drop(fade.timing_with(1.0, enter)),
                None => fade.set(1.0),
            }
        }
    };
    let interactive = props.interactive;
    let (cross_class, cross_style) = (props.cross_line_class, props.cross_line_style);
    let (dot_class, dot_style) = (props.dot_class, props.dot_style);
    let has_cross = move || cross_class.get().is_some() || cross_style.get() != Style::default();
    let has_dots = move || dot_class.get().is_some() || dot_style.get() != Style::default();
    let tooltip = props.tooltip_content;
    let a11y_value = props.accessibility_value;

    view()
        .accessible(true)
        .role(AccessibilityRole::Image)
        .optional_label(props.accessibility_label)
        .accessibility_value(move || {
            a11y_value.get().map(|v| AccessibilityValue {
                min: v.min.map(|n| n as f32),
                max: v.max.map(|n| n as f32),
                now: v.now.map(|n| n as f32),
                text: v.text.map(|t| std::sync::Arc::from(&*t)),
            })
        })
        .on_layout(move |b: &Bounds<Points>| size.set(b.size))
        .on_hover_move(move |e: &HoverEvent| {
            if !interactive.get_untracked() {
                return;
            }
            let at = e.position.x.0;
            let nearest = xs.with_untracked(|xs| {
                xs.iter()
                    .enumerate()
                    .filter(|(_, x)| x.is_finite())
                    .min_by(|(_, a), (_, b)| (*a - at).abs().total_cmp(&(*b - at).abs()))
                    .map(|(i, _)| i as u32)
            });
            follow(nearest);
        })
        .on_hover_out(move || follow(None))
        .child(props.children)
        // The hover's parts, over the marks; they take no pointer, so the
        // plot keeps the hover.
        .child(when(
            move || interactive.get() && has_cross(),
            move || {
                anchor(&cross_x, None, &fade)
                    .top(px(0.0))
                    .bottom(px(0.0))
                    .child(
                        view()
                            .h_full()
                            .app_class(cross_class)
                            .inline_style(cross_style),
                    )
            },
        ))
        .child(when(
            move || interactive.get() && has_dots(),
            move || {
                each(
                    move || marks.get(),
                    |m| m.key,
                    move |mark, _| {
                        let m = mark.get_untracked();
                        anchor(&m.dot_x, Some(&m.dot_y), &fade).child(
                            view()
                                .flex_none()
                                .app_class(dot_class)
                                .inline_style(dot_style),
                        )
                    },
                )
            },
        ))
        .child(when(
            move || interactive.get() && tooltip.get().is_some(),
            move || {
                // A row whose item does not shrink: the content keeps its
                // own width (the anchor has none) and centers on the line,
                // overflowing it on both sides.
                anchor(&cross_x, None, &fade)
                    .flex_row()
                    .top(px(0.0))
                    .child(view().flex_none().child(tooltip.place(|t| t)))
            },
        ))
}

/// A part's anchor: a point with no size at `x` (and `y`, else the top),
/// faded by `fade`, its content centered on it; it takes no pointer.
fn anchor(x: &AnimatedValue, y: Option<&AnimatedValue>, fade: &AnimatedValue) -> View {
    let v = view()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .w(px(0.0))
        .items_center()
        .justify_center()
        .pointer_events(PointerEvents::None)
        .bind_translate_x(x)
        .bind_opacity(fade);
    match y {
        Some(y) => v.h(px(0.0)).bind_translate_y(y),
        None => v,
    }
}

/// A mark's layer: it fills the plot.
fn layer() -> View {
    view().absolute().inset(px(0.0))
}

/// Room a mark's canvas keeps past the plot's edges for its strokes: a
/// stroke centred on an edge (a line through the domain's highest value,
/// a grid line at its end) is half outside the plot, and a canvas draws
/// nothing outside itself. Strokes up to twice this wide draw whole.
const STROKE_ROOM: f32 = 8.0;

/// How far what a mark traces through `points` reaches past the plot's
/// top and bottom edges (`height` apart): a smooth curve swings past its
/// highest and lowest points, never past its control points.
fn reach(points: &[Point<Points>], curve: Curve, height: f32) -> f32 {
    let mut over = 0.0_f32;
    let mut take = |y: f32| over = over.max(-y).max(y - height);
    for q in points {
        take(q.y.0);
    }
    if curve == Curve::Natural {
        let n = points.len();
        for i in 0..n.saturating_sub(1) {
            let p0 = points[i.saturating_sub(1)];
            let (p1, p2) = (points[i], points[i + 1]);
            let p3 = points[(i + 2).min(n - 1)];
            take(p1.y.0 + (p2.y.0 - p0.y.0) / 6.0);
            take(p2.y.0 - (p3.y.0 - p1.y.0) / 6.0);
        }
    }
    over
}

/// A mark's canvas: the plot's area and `bleed()` points around it (half
/// its strokes and its curves' swing past the plot's edges, which a
/// canvas the size of the plot would cut off), its drawing in the plot's
/// coordinates. It takes no pointer: the plot follows it. Before the
/// plot's first layout it draws nothing.
fn mark_canvas(
    plot: Option<PlotContext>,
    bleed: impl Fn() -> f32 + Copy + 'static,
    draw: impl Fn(Painter, f32) -> Painter + 'static,
) -> CanvasView {
    canvas(move || {
        if plot.is_some_and(|plot| !plot.is_sized()) {
            return Painter::new();
        }
        let b = bleed();
        draw(Painter::new().translate(b, b), b)
    })
    .absolute()
    .pointer_events(PointerEvents::None)
    .inline_style(move || {
        let b = -bleed();
        Style::from_decls([
            StyleDecl::Set(StyleKey::Left, PatchValue::Number(b)),
            StyleDecl::Set(StyleKey::Top, PatchValue::Number(b)),
            StyleDecl::Set(StyleKey::Right, PatchValue::Number(b)),
            StyleDecl::Set(StyleKey::Bottom, PatchValue::Number(b)),
        ])
    })
}

#[cfg(test)]
mod tests {
    use nsh_core::geometry::{point, px};

    use super::{Curve, reach};

    /// What a mark draws past the plot's top and bottom edges: nothing for
    /// straight segments inside it; a smooth curve that rises to the top
    /// edge and levels off there swings above it, so its canvas reaches
    /// further.
    #[test]
    fn a_curve_through_an_edge_reaches_past_it() {
        let pts = |ys: &[f32]| {
            ys.iter()
                .enumerate()
                .map(|(i, y)| point(px(i as f32 * 10.0), px(*y)))
                .collect::<Vec<_>>()
        };
        let rise = pts(&[100.0, 50.0, 0.0, 0.0]);
        assert_eq!(reach(&rise, Curve::Linear, 100.0), 0.0);
        assert_eq!(reach(&rise, Curve::StepAfter, 100.0), 0.0);
        let swing = reach(&rise, Curve::Natural, 100.0);
        assert!((swing - 50.0 / 6.0).abs() < 1e-4, "{swing}");
        // Points outside the plot (a domain narrower than the data) count.
        assert_eq!(reach(&pts(&[-5.0, 50.0]), Curve::Linear, 100.0), 5.0);
        assert_eq!(reach(&pts(&[]), Curve::Natural, 100.0), 0.0);
    }
}
