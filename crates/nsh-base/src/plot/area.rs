//! `Area`: a series drawn as the region between its line and a baseline.

use nsh_core::{
    SvgPaint,
    ui::{RowAccessor, Rows, prelude::*},
};

use super::{
    PlotContext, STROKE_ROOM, layer,
    line::{Curve, trace},
    mark_canvas, reach,
    series::Series,
};

/// The props of [`Area`].
#[derive(Debug, Clone, PartialEq, Props)]
#[props(generic)]
pub struct AreaProps {
    /// Rows of its own (`gpui-base`'s `data`); the plot's rows without.
    pub data: Prop<Option<Rows>>,
    /// What places its own rows along the plot's x axis (`gpui-base`'s
    /// `x`); a row's index without.
    pub x: Prop<Option<RowAccessor>>,
    /// What makes its top edge (`gpui-base`'s `y1`): a field name or a
    /// function of the row and its index; the plot's `y` without.
    pub y1: Prop<Option<RowAccessor>>,
    /// The value of its baseline on the y scale (`gpui-base`'s `y0`);
    /// without one, the bottom edge of the plot.
    pub y0: Prop<Option<f64>>,
    /// How its top edge connects its points: `"natural"` (the default, a
    /// smooth curve, as `gpui-base`'s), `"linear"` or `"stepAfter"`.
    #[prop(default)]
    pub curve: Prop<Curve>,
}

/// A series of a [`Plot`](super::Plot) drawn as the region between its
/// line and a baseline (`gpui-base`'s `Area`): its top edge goes through
/// one point per row with a value, placed by the plot's scales, the rows
/// without one skipped, as a [`Line`](super::Line)'s does, and it closes
/// down to `y0`. It fills with its `fill` (`fill-*` classes; inherited from
/// the plot as CSS inherits it), `currentColor` where none is set, and
/// strokes its top edge with its `stroke` (`stroke-*`, `strokeWidth`,
/// `strokeDasharray`), none where none is set. Its values and baseline
/// count in the y scale's domain when the plot has none; it has a hover dot
/// when the plot has a `dotClass`, and draws in with the plot's appear.
#[component]
pub fn Area(props: AreaProps, _cx: Scope<NoEvent>) -> impl IntoElement {
    let plot = PlotContext::of("Area");
    let (y0, curve) = (props.y0, props.curve);
    let series = plot.map(|plot| {
        let series = Series::new(plot, props.data, props.x, props.y1);
        series.join(plot, move || y0.get());
        series
    });
    let base = move |plot: PlotContext| match y0.get() {
        Some(v) => plot.y_map.get().tick(v).unwrap_or(0.0),
        None => plot.size.get().height.0,
    };
    layer().child(mark_canvas(
        plot,
        move || {
            let (Some(plot), Some(series)) = (plot, series) else {
                return STROKE_ROOM;
            };
            let height = plot.size.get().height.0;
            let floor = base(plot);
            let swing = series.points.with(|ps| reach(ps, curve.get(), height));
            STROKE_ROOM + swing.max(-floor).max(floor - height).ceil()
        },
        move |p, bleed| {
            let p = p.fill_style_from_view(SvgPaint::CurrentColor);
            let (Some(plot), Some(series)) = (plot, series) else {
                return p;
            };
            let points = series.points.get();
            let (Some(first), Some(last)) = (points.first(), points.last()) else {
                return p;
            };
            let curve = curve.get();
            let base = base(plot);
            let top = |p: Painter| trace(p.begin_path().move_to(first.x, first.y), &points, curve);
            let p = top(plot.clip(p, bleed))
                .line_to(last.x, px(base))
                .line_to(first.x, px(base))
                .close_path()
                .fill()
                .stroke_style_from_view(SvgPaint::None)
                .line_width_from_view()
                .line_dash_from_view();
            top(p).stroke()
        },
    ))
}
