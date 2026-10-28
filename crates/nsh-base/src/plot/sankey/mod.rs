//! `Sankey`: flows between the plot's rows, drawn as ribbons between
//! nodes (`gpui-base`'s `shape/sankey.rs` generator, d3-sankey's layout,
//! painted as `gpui-component`'s `SankeyChart` paints it).

mod layout;

use std::sync::Arc as Shared;

use nsh_core::{
    SvgPaint,
    element::{JsHost, JsIn, JsOwned, JsValue, PropError, Rows, Style, TsTypes},
    prelude::*,
};

use self::layout::{Error, Generator, Graph, Link, NodeLayout, Ribbon};
use super::{
    PlotContext, STROKE_ROOM,
    class::{RowClass, row_classes},
    layer, mark_canvas, mark_canvas_with,
};

/// Which column a [`Sankey`]'s node goes in (`gpui-base`'s `SankeyAlign`,
/// d3-sankey's `nodeAlign`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum SankeyAlign {
    /// Its depth: the longest path from a node with no links in
    /// (d3's `sankeyLeft`).
    Left,
    /// As far right as its height lets it: the longest path to a node with
    /// no links out, counted from the last column (d3's `sankeyRight`).
    Right,
    /// Its depth, or for a node with only links out, the column before the
    /// nearest of its targets (d3's `sankeyCenter`).
    Center,
    /// Its depth, and a node with no links out in the last column (d3's
    /// `sankeyJustify`, its default).
    #[default]
    Justify,
}

/// How a [`Sankey`]'s values map to its nodes' heights and its ribbons'
/// widths (`gpui-base`'s `SankeyValueScale`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub enum SankeyValueScale {
    /// In proportion to the value, as d3-sankey's.
    #[default]
    Linear,
    /// In proportion to its square root: a wide range of values compressed,
    /// so a dominant flow does not dwarf the small ones. Each node is still
    /// exactly covered by its ribbons on both sides.
    Sqrt,
}

/// The TypeScript type of a link, declared once.
fn link_type(types: &mut TsTypes) -> String {
    types.declare("SankeyLink", |_| {
        "/** A flow of a `Sankey`: from the plot's row `source` to its row `target` (their indices in its `data`), `value` its size. */\nexport type SankeyLink = { source: number; target: number; value: number };\n".to_owned()
    })
}

/// The links of a [`Sankey`] (TS `SankeyLink[]`, `{ source: number;
/// target: number; value: number }`, `gpui-base`'s `SankeyLink`): from the
/// plot's row `source` to its row `target`, by their indices in its `data`
/// (d3-sankey's default `nodeId`), `value` the flow. Read natively as
/// [`Rows`] when the app sets them, the JS array kept for the app's
/// functions of a link (`linkClass`).
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct SankeyLinks(pub Rows);

impl SankeyLinks {
    /// Links of `(source, target, value)`, made in Rust (`gpui-base`'s
    /// `SankeyLink::new(source, target, value)`).
    pub fn new(links: impl IntoIterator<Item = (usize, usize, f64)>) -> Self {
        let (mut sources, mut targets, mut values) = (Vec::new(), Vec::new(), Vec::new());
        for (source, target, value) in links {
            sources.push(source as f64);
            targets.push(target as f64);
            values.push(value);
        }
        let len = sources.len();
        Self(Rows::from_columns(
            len,
            [
                ("source".into(), sources),
                ("target".into(), targets),
                ("value".into(), values),
            ],
        ))
    }

    /// The links as the layout takes them; `Err(i)`: link `i`'s source or
    /// target is not an index. A value that is not a number, or below zero,
    /// carries nothing.
    fn links(&self) -> Result<Vec<Link>, usize> {
        let rows = &self.0;
        let at = |key: &str, i: usize| rows.column(key).map_or(f64::NAN, |c| c[i]);
        let index = |n: f64| {
            (n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n <= u32::MAX as f64)
                .then_some(n as usize)
        };
        (0..rows.len())
            .map(|i| {
                let value = at("value", i);
                Ok(Link {
                    source: index(at("source", i)).ok_or(i)?,
                    target: index(at("target", i)).ok_or(i)?,
                    value: match value.is_finite() {
                        true => value.max(0.0),
                        false => 0.0,
                    },
                })
            })
            .collect()
    }
}

impl From<Rows> for SankeyLinks {
    fn from(rows: Rows) -> Self {
        Self(rows)
    }
}

impl JsValue for SankeyLinks {
    fn ts_type(types: &mut TsTypes) -> String {
        format!("{}[]", link_type(types))
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        Rows::from_js(value).map(Self)
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        self.0.to_js(host)
    }
}

/// The classes of a [`Sankey`]'s links' parts (TS `string | ((link:
/// SankeyLink, index: number) => string)`): a [`RowClass`] whose rows are
/// the links, so the app's function gets the link object it put in `links`
/// and its index there.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SankeyLinkClass(pub RowClass);

impl Default for SankeyLinkClass {
    fn default() -> Self {
        Self(RowClass::All(SharedString::default()))
    }
}

impl From<RowClass> for SankeyLinkClass {
    fn from(class: RowClass) -> Self {
        Self(class)
    }
}

impl From<&'static str> for SankeyLinkClass {
    fn from(class: &'static str) -> Self {
        Self(class.into())
    }
}

impl From<SharedString> for SankeyLinkClass {
    fn from(class: SharedString) -> Self {
        Self(class.into())
    }
}

impl JsValue for SankeyLinkClass {
    fn ts_type(types: &mut TsTypes) -> String {
        format!(
            "string | ((link: {}, index: number) => string)",
            link_type(types)
        )
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        RowClass::read(value).map(Self)
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        self.0.write(host)
    }
}

/// The props of [`Sankey`].
#[derive(Debug, Clone, PartialEq, Props)]
#[props(generic)]
pub struct SankeyProps {
    /// The flows (`gpui-base`'s `SankeyLink`s, d3-sankey's `links`): `{
    /// source, target, value }`, the indices of two of the plot's rows and
    /// the flow between them. A link whose value is not a number or is
    /// below zero carries nothing.
    pub links: Prop<SankeyLinks>,
    /// The width of each node, in points (`gpui-base`'s
    /// `Sankey::node_width`, d3's `nodeWidth`); 24 by default.
    #[prop(default = Points(24.0))]
    pub node_width: Prop<Points>,
    /// The gap between the nodes of a column, in points
    /// (`Sankey::node_padding`, d3's `nodePadding`); 8 by default, less in
    /// a column too crowded for it.
    #[prop(default = Points(8.0))]
    pub node_padding: Prop<Points>,
    /// Which column each node goes in (`Sankey::node_align`, d3's
    /// `nodeAlign`): `"justify"` (the default), `"left"`, `"right"` or
    /// `"center"`.
    #[prop(default)]
    pub node_align: Prop<SankeyAlign>,
    /// How many times the layout relaxes the nodes toward their flows
    /// (`Sankey::iterations`, d3's `iterations`); 6 by default.
    #[prop(default = 6)]
    pub iterations: Prop<u32>,
    /// How values map to heights and widths (`Sankey::value_scale`):
    /// `"linear"` (the default) or `"sqrt"`.
    #[prop(default)]
    pub value_scale: Prop<SankeyValueScale>,
    /// The thinnest a ribbon draws, in points (`gpui-component`'s
    /// `SankeyChart::min_link_width`), so a small flow stays visible; 1 by
    /// default.
    #[prop(default = Points(1.0))]
    pub min_link_width: Prop<Points>,
    /// The radius of each node's corners, in points (`gpui-component`'s
    /// `SankeyChart::node_corner_radius`); 0 by default, square.
    #[prop(default)]
    pub node_corner_radius: Prop<Points>,
    /// The Tailwind classes of each node, the `node` part: the same on
    /// every node, or a function of the node's row and its index that gives
    /// each its own (`gpui-component`'s `SankeyChart::node_color`): its
    /// `fill-*`, `stroke-*`.
    pub node_class: Prop<Option<RowClass>>,
    /// The style of every node, over its classes (`fill`, `stroke`,
    /// `strokeWidth`).
    #[prop(default)]
    pub node_style: Prop<Style>,
    /// The Tailwind classes of each ribbon, the `link` part: the same on
    /// every link (`"fill-foreground/10"`, `gpui-component`'s
    /// `link_opacity`), or a function of the link and its index.
    pub link_class: Prop<Option<SankeyLinkClass>>,
    /// The style of every ribbon, over its classes.
    #[prop(default)]
    pub link_style: Prop<Style>,
    /// The Tailwind classes of each ribbon's background, the
    /// `linkBackground` part: a box from its source node's right edge to
    /// its target's left edge, its background (`bg-*`, a gradient:
    /// `bg-linear-to-r from-… to-…`, `gpui-component`'s source-to-target
    /// `linear_gradient`) shown only inside the ribbon, under its `fill`; a
    /// function of the link and its index gives each its own (its source's
    /// color to its target's). CSS's `fill` takes a color, so a gradient is
    /// a background here, as `Area`'s. With a background, a ribbon's `fill`
    /// is `none` where none is set.
    pub link_background_class: Prop<Option<SankeyLinkClass>>,
    /// The style of every ribbon's background, over its classes.
    #[prop(default)]
    pub link_background_style: Prop<Style>,
}

/// What stops a sankey from drawing, told once while it stays.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Problem {
    /// Link `i`'s source or target is not an index.
    NotAnIndex(usize),
    /// What the layout refused.
    Layout(Error),
}

/// Records `ribbon`'s outline as a new path.
fn trace(p: Painter, r: &Ribbon) -> Painter {
    let mx = px(r.mx());
    p.begin_path()
        .move_to(r.sx, r.s_top)
        .bezier_curve_to(
            point(mx, px(r.s_top)),
            point(mx, px(r.t_top)),
            point(px(r.tx), px(r.t_top)),
        )
        .line_to(r.tx, r.t_bottom)
        .bezier_curve_to(
            point(mx, px(r.t_bottom)),
            point(mx, px(r.s_bottom)),
            point(px(r.sx), px(r.s_bottom)),
        )
        .close_path()
}

/// A ribbon's box: from its source end to its target end, from its highest
/// edge to its lowest.
fn ribbon_box(r: &Ribbon) -> Bounds<Points> {
    let (top, bottom) = (r.s_top.min(r.t_top), r.s_bottom.max(r.t_bottom));
    Bounds::new(
        point(px(r.sx), px(top)),
        size(px(r.tx - r.sx), px(bottom - top)),
    )
}

/// A node's rectangle, at least a point tall (`gpui-component`'s
/// `SankeyChart`: a node with nothing through it stays visible).
fn node_rect(n: &NodeLayout) -> Bounds<Points> {
    Bounds::new(
        point(px(n.x0), px(n.y0)),
        size(px(n.x1 - n.x0), px((n.y1 - n.y0).max(1.0))),
    )
}

/// Paints with the view's `fill` (the given paint where none is set) and
/// `stroke` (none where none is set).
fn painted(p: Painter, fill: SvgPaint) -> Painter {
    p.fill_style_from_view(fill)
        .stroke_style_from_view(SvgPaint::None)
        .line_width_from_view()
        .line_dash_from_view()
}

/// Flows between a [`Plot`](super::Plot)'s rows (`gpui-base`'s `Sankey`
/// layout of `SankeyLink`s, drawn as `gpui-component`'s `SankeyChart`
/// draws it): the rows are its nodes and `links` the flows between them,
/// laid out by d3-sankey's algorithm in the plot's size: a node per row,
/// in columns by `nodeAlign`, as tall as the most that flows through it,
/// and a ribbon per link from its source's right edge to its target's left
/// edge, as wide at each end as its share of that node's side (at least
/// `minLinkWidth`). Each column is centered, and a run of equal single
/// nodes staggered so the ribbons between them curve, as `gpui-base`'s
/// layout does. Each node and each ribbon is a canvas of its own, the
/// `node` and `link` parts (and `linkBackground`, a gradient inside the
/// ribbon), so each paints with its own classes (`nodeClass`, a function
/// of its row; `linkClass`, of its link) over the sankey's, as CSS
/// inherits `fill` and `stroke`: `fill` (`currentColor` where none is set)
/// and `stroke` (none). The ribbons draw under the nodes, and both from
/// the leading edge with the plot's appear. The layout runs again only
/// when the links, the options or the plot's size change; links that make
/// a cycle or name a row the plot does not have log a warning and draw
/// nothing. Its values take no part in the plot's scales, and the pointer
/// over it finds no row.
#[component]
pub fn Sankey(props: SankeyProps, _cx: Scope<NoEvent>) -> impl IntoElement {
    let Some(plot) = PlotContext::of("Sankey") else {
        return layer();
    };
    let links = props.links;
    let (node_width, node_padding, node_align) =
        (props.node_width, props.node_padding, props.node_align);
    let (iterations, value_scale) = (props.iterations, props.value_scale);
    let (min_link_width, corner) = (props.min_link_width, props.node_corner_radius);
    // Its nodes are the plot's rows, which have no place along x: the
    // pointer over it finds none, rather than the row nearest along x.
    plot.add_hit(|_| None, |_| None);

    let nodes = computed(move || plot.rows.with(Rows::len));
    let input = computed(move || links.with(SankeyLinks::links));
    let warned = stored(None::<Problem>);
    let graph = computed(move || -> Option<Shared<Graph>> {
        if !plot.is_sized() {
            return None;
        }
        let size = plot.size.get();
        let generator = Generator {
            node_width: node_width.get().0.max(0.0),
            node_padding: node_padding.get().0.max(0.0),
            align: node_align.get(),
            iterations: iterations.get() as usize,
            value_scale: value_scale.get(),
            x0: 0.0,
            y0: 0.0,
            x1: size.width.0,
            y1: size.height.0,
        };
        let count = nodes.get();
        let laid = input.with(|input| match input {
            Ok(links) => generator.layout(count, links).map_err(Problem::Layout),
            Err(i) => Err(Problem::NotAnIndex(*i)),
        });
        match laid {
            Ok(graph) => {
                warned.replace(None);
                Some(Shared::new(graph))
            }
            Err(problem) => {
                if warned.get() != Some(problem) {
                    warned.replace(Some(problem));
                    match problem {
                        Problem::NotAnIndex(i) => log::warn!(
                            "`Sankey` draws nothing: link {i}'s `source` or `target` is not the index of a row"
                        ),
                        Problem::Layout(Error::MissingNode(row)) => log::warn!(
                            "`Sankey` draws nothing: a link names row {row}, and the plot has {count}"
                        ),
                        Problem::Layout(Error::CircularLink) => {
                            log::warn!("`Sankey` draws nothing: its links make a cycle")
                        }
                    }
                }
                None
            }
        }
    });
    // The links that carry something (`gpui-component` draws no ribbon
    // for the others) and the nodes, made again only when they change.
    let link_keys = computed(move || {
        graph.with(|g| match g {
            Some(g) => g
                .links
                .iter()
                .filter(|l| l.value > 0.0)
                .map(|l| l.index as u32)
                .collect(),
            None => Vec::new(),
        })
    });
    let node_count = computed(move || graph.with(|g| g.as_ref().map_or(0, |g| g.nodes.len())));
    let ribbon = move |i: usize| -> Option<Ribbon> {
        let min = min_link_width.get().0.max(0.0);
        graph.with(|g| {
            let g = g.as_ref()?;
            Some(Ribbon::of(g, g.links.get(i)?, min))
        })
    };
    // A ribbon `minLinkWidth` thick on the extent's edge reaches past it
    // by half that.
    let bleed = move || STROKE_ROOM + (min_link_width.get().0.max(0.0) / 2.0).ceil();

    let (node_style, link_style) = (props.node_style, props.link_style);
    let (node_class, link_class) = (props.node_class, props.link_class);
    let (background_class, background_style) =
        (props.link_background_class, props.link_background_style);
    let link_rows = move || links.with(|l| l.0.clone());
    let node_classes = row_classes(move || plot.rows.get(), move || node_class.get());
    let link_classes = row_classes(link_rows, move || link_class.get().map(|c| c.0));
    let background_classes = row_classes(link_rows, move || background_class.get().map(|c| c.0));
    let class_of = |classes: Signal<Option<Shared<[SharedString]>>>, i: usize| {
        move || classes.with(|c| c.as_ref().and_then(|c| c.get(i).cloned()))
    };
    let has_background = move || {
        background_class.with(Option::is_some) || background_style.with(|s| *s != Style::default())
    };

    layer()
        // The `linkBackground` part: a canvas per ribbon whose view takes
        // the app's background and shows it inside the ribbon only.
        .child(when(has_background, move || {
            each(
                move || link_keys.get(),
                |k| *k,
                move |key, _| {
                    let i = key.get_untracked() as usize;
                    // From its first drawing on, so its view never draws
                    // the background as a box.
                    let nothing =
                        |p: Painter| p.begin_path().fill_background_from_view(Bounds::default());
                    mark_canvas_with(
                        Some(plot),
                        bleed,
                        move || nothing(Painter::new()),
                        move |p, bleed| match ribbon(i) {
                            Some(r) => trace(plot.clip(p, bleed), &r)
                                .fill_background_from_view(ribbon_box(&r)),
                            None => nothing(p),
                        },
                    )
                    .app_class(class_of(background_classes, i))
                    .inline_style(background_style)
                },
            )
        }))
        // The `link` part: a ribbon per link, under the nodes.
        .child(each(
            move || link_keys.get(),
            |k| *k,
            move |key, _| {
                let i = key.get_untracked() as usize;
                mark_canvas(Some(plot), bleed, move |p, bleed| {
                    let initial = match has_background() {
                        true => SvgPaint::None,
                        false => SvgPaint::CurrentColor,
                    };
                    let p = painted(p, initial);
                    match ribbon(i) {
                        Some(r) => trace(plot.clip(p, bleed), &r).fill().stroke(),
                        None => p,
                    }
                })
                .app_class(class_of(link_classes, i))
                .inline_style(link_style)
            },
        ))
        // The `node` part: a rectangle per row.
        .child(each(
            move || (0..node_count.get() as u32).collect::<Vec<_>>(),
            |k| *k,
            move |key, _| {
                let i = key.get_untracked() as usize;
                mark_canvas(
                    Some(plot),
                    || STROKE_ROOM,
                    move |p, bleed| {
                        let p = painted(p, SvgPaint::CurrentColor);
                        let Some(rect) = graph.with(|g| g.as_ref()?.nodes.get(i).map(node_rect))
                        else {
                            return p;
                        };
                        let p = plot.clip(p, bleed).begin_path();
                        let radius = corner.get();
                        let p = match radius.0 > 0.0 {
                            true => p.round_rect(rect, radius),
                            false => p.rect(rect),
                        };
                        p.fill().stroke()
                    },
                )
                .app_class(class_of(node_classes, i))
                .inline_style(node_style)
            },
        ))
}

#[cfg(test)]
mod tests {
    use super::SankeyLinks;

    /// Links read as the layout takes them: indices as numbers, a value
    /// that is no number or below zero carrying nothing; a source or target
    /// that is no index names the link.
    #[test]
    fn links_are_read_by_index() {
        let links = SankeyLinks::new([(0, 1, 5.0), (1, 2, f64::NAN), (1, 3, -2.0)])
            .links()
            .unwrap();
        let read: Vec<_> = links
            .iter()
            .map(|l| (l.source, l.target, l.value))
            .collect();
        assert_eq!(read, [(0, 1, 5.0), (1, 2, 0.0), (1, 3, 0.0)]);
        let rows = |source: f64| {
            SankeyLinks(nsh_core::element::Rows::from_columns(
                2,
                [
                    ("source".into(), vec![0.0, source]),
                    ("target".into(), vec![1.0, 2.0]),
                    ("value".into(), vec![1.0, 1.0]),
                ],
            ))
        };
        assert_eq!(rows(1.5).links(), Err(1));
        assert_eq!(rows(-1.0).links(), Err(1));
        assert_eq!(rows(f64::NAN).links(), Err(1));
        assert!(rows(1.0).links().is_ok());
    }
}
