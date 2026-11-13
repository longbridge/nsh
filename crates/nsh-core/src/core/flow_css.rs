//! A node's layout as CSS (`Core::layout_css`): its display, flex or grid
//! container properties, its own place in its parent's (grow, shrink,
//! basis, alignment, span) and its box (sizes, margins, padding, borders,
//! insets), with React Native's defaults written out where they differ
//! from CSS's (`flex-direction: column`, `flex-shrink: 0`, `min-width: 0`).
//! A page exported as static HTML (the web's prerendering tools) lays its
//! views out with it in the browser's own layout instead of at the frames
//! the core computed for one viewport, so it fits any width the page is
//! opened at, as server-rendered HTML does.
//!
//! What taffy lays out and this does not write: a `calc()` length and
//! `stretch`, which have no form here (the property is left out, so the
//! browser uses its initial value), and `grid-template-areas`, which the
//! core has no style for.

use std::fmt::Write;

use taffy::{
    AlignContent, AlignItems, BoxSizing, CompactLength, Dimension, Direction, Display,
    FlexDirection, FlexWrap, GridAutoFlow, GridTemplateComponent, LengthPercentage,
    LengthPercentageAuto, Position, RepetitionCount, Style,
};

use super::Core;
use crate::{NodeId, text::TextEngine};

impl<M: TextEngine> Core<M> {
    /// The layout of view `id` as CSS declarations (`display: flex;
    /// flex-direction: column; ...`): what a static copy of the page lays
    /// the view out with in a browser at any width. `None` for an id that
    /// is no view (raw text, spans, unknown ids).
    pub fn layout_css(&self, id: NodeId) -> Option<String> {
        let key = self.key(id).ok()?;
        let layout = self.nodes[key as usize].layout?;
        Some(css(self.layout.style(layout)))
    }
}

/// A length, share or sizing keyword as CSS. `None` for what has no form
/// here (a `calc()`, `stretch`): the property is left out, and the browser
/// lays the view out with the property's initial value.
fn len(raw: CompactLength) -> Option<String> {
    Some(match raw.tag() {
        CompactLength::LENGTH_TAG => format!("{}px", raw.value()),
        CompactLength::PERCENT_TAG => format!("{}%", raw.value() * 100.0),
        CompactLength::AUTO_TAG => "auto".into(),
        CompactLength::FR_TAG => format!("{}fr", raw.value()),
        CompactLength::MIN_CONTENT_TAG => "min-content".into(),
        CompactLength::MAX_CONTENT_TAG => "max-content".into(),
        CompactLength::FIT_CONTENT_KEYWORD_TAG => "fit-content".into(),
        CompactLength::FIT_CONTENT_PX_TAG => format!("fit-content({}px)", raw.value()),
        CompactLength::FIT_CONTENT_PERCENT_TAG => {
            format!("fit-content({}%)", raw.value() * 100.0)
        }
        CompactLength::CONTENT_TAG => "content".into(),
        _ => return None,
    })
}

fn dim(v: Dimension) -> Option<String> {
    len(v.into_raw())
}

fn lpa(v: LengthPercentageAuto) -> Option<String> {
    len(v.into_raw())
}

fn lp(v: LengthPercentage) -> Option<String> {
    len(v.into_raw())
}

fn align_items(v: AlignItems) -> &'static str {
    match v {
        AlignItems::FLEX_START => "flex-start",
        AlignItems::FLEX_END => "flex-end",
        AlignItems::START => "start",
        AlignItems::END => "end",
        AlignItems::SELF_START => "self-start",
        AlignItems::SELF_END => "self-end",
        AlignItems::CENTER => "center",
        AlignItems::BASELINE => "baseline",
        AlignItems::STRETCH => "stretch",
        _ => "normal",
    }
}

fn align_content(v: AlignContent) -> &'static str {
    match v {
        AlignContent::FLEX_START => "flex-start",
        AlignContent::FLEX_END => "flex-end",
        AlignContent::START => "start",
        AlignContent::END => "end",
        AlignContent::CENTER => "center",
        AlignContent::STRETCH => "stretch",
        AlignContent::SPACE_BETWEEN => "space-between",
        AlignContent::SPACE_AROUND => "space-around",
        AlignContent::SPACE_EVENLY => "space-evenly",
        _ => "normal",
    }
}

/// A grid track with its minimum and its maximum, as CSS writes the pair:
/// one size when they are the same (`100px`, `auto`, `min-content`), the
/// maximum alone where CSS's short form has an automatic minimum (`1fr`,
/// `fit-content(100px)`), else `minmax(min, max)` (`minmax(0px, 1fr)`,
/// Tailwind's `grid-cols-*`; `minmax(200px, 1fr)`).
fn track(t: &taffy::TrackSizingFunction) -> String {
    let min = len(t.min_sizing_function().into_raw());
    let max = len(t.max_sizing_function().into_raw());
    match (min, max) {
        (Some(min), Some(max)) if min == max => min,
        (Some(min), Some(max))
            if min == "auto" && (max.ends_with("fr") || max.starts_with("fit-content")) =>
        {
            max
        }
        (Some(min), Some(max)) => format!("minmax({min},{max})"),
        (_, max) => max.unwrap_or_else(|| "auto".into()),
    }
}

/// Implicit tracks (`grid-auto-rows`).
fn auto_tracks(tracks: &[taffy::TrackSizingFunction]) -> Option<String> {
    (!tracks.is_empty()).then(|| tracks.iter().map(track).collect::<Vec<_>>().join(" "))
}

/// The names of a line (`[a b]`), when it has some.
fn line_names(out: &mut Vec<String>, names: Option<&Vec<String>>) {
    if let Some(names) = names.filter(|names| !names.is_empty()) {
        out.push(format!("[{}]", names.join(" ")));
    }
}

/// A template's tracks with the names of its lines: `names` has one set
/// per line between the components (a `repeat()` is one component, with
/// the names of its own lines), or none when no line is named.
fn tracks(components: &[GridTemplateComponent<String>], names: &[Vec<String>]) -> Option<String> {
    let mut out = Vec::new();
    for (i, component) in components.iter().enumerate() {
        line_names(&mut out, names.get(i));
        match component {
            GridTemplateComponent::Single(t) => out.push(track(t)),
            GridTemplateComponent::Repeat(r) => {
                let mut inner = Vec::new();
                for (j, t) in r.tracks.iter().enumerate() {
                    line_names(&mut inner, r.line_names.get(j));
                    inner.push(track(t));
                }
                line_names(&mut inner, r.line_names.get(r.tracks.len()));
                let count = match r.count {
                    RepetitionCount::Count(n) => n.to_string(),
                    RepetitionCount::AutoFill => "auto-fill".into(),
                    RepetitionCount::AutoFit => "auto-fit".into(),
                };
                out.push(format!("repeat({count},{})", inner.join(" ")));
            }
        }
    }
    line_names(&mut out, names.get(components.len()));
    (!out.is_empty()).then(|| out.join(" "))
}

fn placement(line: &taffy::Line<taffy::GridPlacement<String>>) -> Option<String> {
    use taffy::GridPlacement as P;
    let side = |p: &P<String>| match p {
        P::Auto => "auto".to_string(),
        P::Line(n) => n.as_i16().to_string(),
        P::Span(n) => format!("span {n}"),
        // The `n`th line of the name (`2 col`); 0 is the name alone.
        P::NamedLine(name, 0) => name.clone(),
        P::NamedLine(name, n) => format!("{n} {name}"),
        P::NamedSpan(name, n) => format!("span {n} {name}"),
    };
    (!matches!((&line.start, &line.end), (P::Auto, P::Auto)))
        .then(|| format!("{} / {}", side(&line.start), side(&line.end)))
}

/// `style` as CSS declarations.
fn css(style: &Style) -> String {
    let mut s = String::new();
    let mut put = |name: &str, value: Option<String>| {
        if let Some(v) = value {
            let _ = write!(s, "{name}:{v};");
        }
    };
    put(
        "display",
        Some(
            match style.display {
                Display::Flex => "flex",
                Display::Grid => "grid",
                Display::Block => "block",
                Display::FlowRoot => "flow-root",
                Display::None => "none",
            }
            .into(),
        ),
    );
    put(
        "position",
        Some(
            match style.position {
                Position::Absolute => "absolute",
                Position::Relative => "relative",
                Position::Static => "static",
                Position::Fixed => "fixed",
                Position::Sticky => "sticky",
            }
            .into(),
        ),
    );
    // Insets move a relative view from its place as they place an absolute
    // one; `auto`, the initial value, is left out. A static view has none.
    if style.position != Position::Static {
        let inset = |v| lpa(v).filter(|v| v != "auto");
        put("top", inset(style.inset.top));
        put("right", inset(style.inset.right));
        put("bottom", inset(style.inset.bottom));
        put("left", inset(style.inset.left));
    }
    // The page's rules size every box by its border (`box-sizing:
    // border-box`), the core's default.
    if style.box_sizing == BoxSizing::ContentBox {
        put("box-sizing", Some("content-box".into()));
    }
    if style.direction == Direction::Rtl {
        put("direction", Some("rtl".into()));
    }
    // As a container.
    put(
        "flex-direction",
        Some(
            match style.flex_direction {
                FlexDirection::Row => "row",
                FlexDirection::Column => "column",
                FlexDirection::RowReverse => "row-reverse",
                FlexDirection::ColumnReverse => "column-reverse",
            }
            .into(),
        ),
    );
    put(
        "flex-wrap",
        Some(
            match style.flex_wrap {
                FlexWrap::NoWrap => "nowrap",
                FlexWrap::Wrap => "wrap",
                FlexWrap::WrapReverse => "wrap-reverse",
            }
            .into(),
        ),
    );
    put(
        "align-items",
        Some(align_items(style.align_items))
            .filter(|v| *v != "normal")
            .map(Into::into),
    );
    put(
        "justify-items",
        Some(align_items(style.justify_items))
            .filter(|v| *v != "normal")
            .map(Into::into),
    );
    put(
        "align-content",
        Some(align_content(style.align_content))
            .filter(|v| *v != "normal")
            .map(Into::into),
    );
    put(
        "justify-content",
        Some(align_content(style.justify_content))
            .filter(|v| *v != "normal")
            .map(Into::into),
    );
    put("row-gap", lp(style.gap.height));
    put("column-gap", lp(style.gap.width));
    if style.display == Display::Grid {
        put(
            "grid-template-columns",
            tracks(
                &style.grid_template_columns,
                &style.grid_template_column_names,
            ),
        );
        put(
            "grid-template-rows",
            tracks(&style.grid_template_rows, &style.grid_template_row_names),
        );
        put("grid-auto-columns", auto_tracks(&style.grid_auto_columns));
        put("grid-auto-rows", auto_tracks(&style.grid_auto_rows));
        put(
            "grid-auto-flow",
            match style.grid_auto_flow {
                GridAutoFlow::Row => None,
                GridAutoFlow::Column => Some("column".into()),
                GridAutoFlow::RowDense => Some("row dense".into()),
                GridAutoFlow::ColumnDense => Some("column dense".into()),
            },
        );
        // No `grid-template-areas`: the core has no style for them, so a
        // name in `grid-column` / `grid-row` is always a line's.
    }
    // As an item.
    put("flex-grow", Some(style.flex_grow.to_string()));
    put("flex-shrink", Some(style.flex_shrink.to_string()));
    put("flex-basis", dim(style.flex_basis));
    put(
        "align-self",
        style.align_self.map(|v| align_items(v).into()),
    );
    put(
        "justify-self",
        style.justify_self.map(|v| align_items(v).into()),
    );
    put("grid-column", placement(&style.grid_column));
    put("grid-row", placement(&style.grid_row));
    // The box: borders reserve their room (the runtime draws them over it).
    put("width", dim(style.size.width));
    put("height", dim(style.size.height));
    put("min-width", lpa(style.min_size.width));
    put("min-height", lpa(style.min_size.height));
    put(
        "max-width",
        lpa(style.max_size.width).filter(|v| v != "auto"),
    );
    put(
        "max-height",
        lpa(style.max_size.height).filter(|v| v != "auto"),
    );
    put("aspect-ratio", style.aspect_ratio.map(|r| r.to_string()));
    let sides = |r: &taffy::Rect<Option<String>>| {
        format!(
            "{} {} {} {}",
            r.top.as_deref().unwrap_or("0"),
            r.right.as_deref().unwrap_or("0"),
            r.bottom.as_deref().unwrap_or("0"),
            r.left.as_deref().unwrap_or("0")
        )
    };
    put("margin", Some(sides(&style.margin.map(lpa))));
    put("padding", Some(sides(&style.padding.map(lp))));
    let border = style.border.map(lp);
    if [&border.top, &border.right, &border.bottom, &border.left]
        .into_iter()
        .any(|b| b.as_deref().is_some_and(|b| b != "0px"))
    {
        put("border-style", Some("solid".into()));
        put("border-color", Some("transparent".into()));
        put("border-width", Some(sides(&border)));
    }
    s
}
