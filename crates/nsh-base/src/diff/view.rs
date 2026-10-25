//! The diff view's view: a vertical scroll view over a box of every row's
//! height, which holds the sides (two side by side, or one) and over them
//! the separators and the app's hunk actions. A side is a gutter of line
//! numbers beside a horizontal scroll view of the text. Only the rows in
//! view (and a few around them) are made.

use nsh_core::ui::{Style, prelude::*};

use super::{
    DiffExpand, DiffIndicators, DiffLayout, DiffSeparator, DiffSide, DiffViewPart, DiffViewProps,
    controller::{Dv, RowCell, Span, Tint},
    layout::Row,
};
use crate::theme;

/// Added lines' green.
const ADDED: AdaptiveColor = AdaptiveColor::rgb(0x16a34a, 0x4ade80);
/// Deleted lines' red.
const DELETED: AdaptiveColor = AdaptiveColor::rgb(0xdc2626, 0xf87171);
/// The width of a bar indicator.
const BAR: f32 = 3.0;
/// The room around a gutter's numbers and before a line's text.
const PAD: f32 = 8.0;

/// `el` with the classes and styles of `list`, in order (later parts win).
fn parts<E: Element>(el: E, props: DiffViewProps, list: Vec<DiffViewPart>) -> E {
    let styles = list.clone();
    el.app_class(move || {
        props.classes.with(|c| {
            let joined: Vec<&str> = list
                .iter()
                .filter_map(|p| c.get(*p).map(|s| s.as_str()))
                .collect();
            (!joined.is_empty()).then(|| SharedString::from(joined.join(" ")))
        })
    })
    .inline_style(move || {
        props.styles.with(|s| {
            Style::from_decls(
                styles
                    .iter()
                    .filter_map(|p| s.get(*p))
                    .flat_map(|s| s.decls().iter().cloned()),
            )
        })
    })
}

fn part<E: Element>(el: E, props: DiffViewProps, part: DiffViewPart) -> E {
    parts(el, props, vec![part])
}

/// Which column a side shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Old,
    New,
    /// Unified: both numbers, either line.
    Both,
}

/// The diff view.
pub(crate) fn render(dv: Dv) -> impl IntoElement {
    let props = dv.props;
    let lh = line_height(&dv);
    let (scroll_dv, layout_dv) = (dv.clone(), dv.clone());
    let font = {
        let dv = dv.clone();
        move || {
            dv.rev.get();
            dv.st.borrow().font.clone()
        }
    };
    let (font_a, font_b, font_c) = (font.clone(), font.clone(), font);
    let height = {
        let dv = dv.clone();
        let lh = lh.clone();
        move || {
            dv.rev.get();
            lh() * dv.st.borrow().rows.len() as f32
        }
    };
    let rows = visible_rows(dv.clone());
    let split_dv = dv.clone();
    let unified_dv = dv.clone();
    view()
        .relative()
        .overflow_hidden()
        .text_color(move || theme().foreground)
        .font_family(move || font_a().family)
        .text_size(move || Length::from(font_b().size))
        .line_height(move || Length::from(font_c().line_height))
        .child(
            scroll()
                .element_ref(&dv.vscroll)
                .absolute()
                .inset_0()
                .on_scroll(move |e: &ScrollEvent| {
                    scroll_dv.scroll_y.set(e.offset.y);
                    scroll_dv.viewport.set(e.size);
                    // Lines scrolled into view far past the parse are guessed.
                    scroll_dv.schedule_highlight();
                })
                .on_layout(move |b: &Bounds<Points>| layout_dv.viewport.set(b.size))
                .child(
                    view()
                        .relative()
                        .h(move || Length::from(height()))
                        .child(
                            when(
                                move || props.layout.get() == DiffLayout::Split,
                                move || {
                                    view()
                                        .absolute()
                                        .inset_0()
                                        .flex_row()
                                        .child(
                                            side(split_dv.clone(), Column::Old, rows)
                                                .border_r_1()
                                                .border_color(move || theme().border),
                                        )
                                        .child(side(split_dv.clone(), Column::New, rows))
                                },
                            )
                            .otherwise(move || {
                                view().absolute().inset_0().flex_row().child(side(
                                    unified_dv.clone(),
                                    Column::Both,
                                    rows,
                                ))
                            }),
                        )
                        .child(separators(dv.clone(), rows))
                        .child(hunk_actions(dv.clone(), rows)),
                ),
        )
}

fn line_height(dv: &Dv) -> impl Fn() -> Points + Clone + 'static {
    let dv = dv.clone();
    move || {
        dv.rev.get();
        dv.st.borrow().font.line_height
    }
}

/// The rows in view.
fn visible_rows(dv: Dv) -> Memo<Vec<usize>> {
    computed(move || {
        dv.rev.get();
        dv.visible_rows().collect()
    })
}

/// A side: its gutter beside its text.
fn side(dv: Dv, column: Column, rows: Memo<Vec<usize>>) -> View {
    let props = dv.props;
    part(
        view().flex_1().flex_row().min_w(px(0.0)).overflow_hidden(),
        props,
        DiffViewPart::Side,
    )
    .child(gutter(dv.clone(), column, rows))
    .child(content(dv, column, rows))
}

/// What a row shows in `column`: its cell, `None` for a separator.
fn row_cell(dv: &Dv, row: usize, column: Column) -> Option<(RowCell, Option<usize>)> {
    let inline = dv.props.inline.get();
    let (old, new) = dv.st.borrow_mut().cells(row, inline)?;
    Some(match column {
        Column::Old => (old, None),
        Column::New => (new, None),
        Column::Both => {
            let old_number = old.number;
            if new.kind == Tint::Filler {
                (old, old_number)
            } else {
                (new, old_number)
            }
        }
    })
}

/// A cell's row, kept up to date.
fn cell_memo(dv: &Dv, row: usize, column: Column) -> Memo<Option<(RowCell, Option<usize>)>> {
    let dv = dv.clone();
    computed(move || {
        dv.rev.get();
        dv.hl_rev.get();
        row_cell(&dv, row, column)
    })
}

/// The gutter of a side: numbers and indicators.
fn gutter(dv: Dv, column: Column, rows: Memo<Vec<usize>>) -> impl IntoElement {
    let props = dv.props;
    let number_width = {
        let dv = dv.clone();
        move || {
            dv.rev.get();
            dv.st.borrow().number_width()
        }
    };
    let sign_width = {
        let dv = dv.clone();
        move || {
            dv.rev.get();
            if props.indicators.get() == DiffIndicators::Classic {
                dv.st.borrow().column * 2.0
            } else {
                px(0.0)
            }
        }
    };
    let width = {
        let nw = number_width.clone();
        let sw = sign_width.clone();
        move || {
            let numbers = if props.line_numbers.get() {
                let n = if column == Column::Both { 2.0 } else { 1.0 };
                nw() * n + px(PAD * (n + 1.0))
            } else {
                px(0.0)
            };
            numbers + sw() + px(BAR)
        }
    };
    let lh = line_height(&dv);
    part(
        view()
            .flex_none()
            .relative()
            .w(move || Length::from(width()))
            .bg(move || theme().background),
        props,
        DiffViewPart::Gutter,
    )
    .child(each(
        rows,
        |row: &usize| *row,
        move |row, _| {
            let row = row.get_untracked();
            gutter_row(
                dv.clone(),
                row,
                column,
                lh.clone(),
                number_width.clone(),
                sign_width.clone(),
            )
        },
    ))
}

fn gutter_row(
    dv: Dv,
    row: usize,
    column: Column,
    lh: impl Fn() -> Points + Clone + 'static,
    number_width: impl Fn() -> Points + Clone + 'static,
    sign_width: impl Fn() -> Points + Clone + 'static,
) -> impl IntoElement {
    let props = dv.props;
    let cell = cell_memo(&dv, row, column);
    let lh2 = lh.clone();
    // A stable view per row: `each` keeps it while the row stays in view,
    // whatever the row shows.
    let place = view()
        .absolute()
        .left_0()
        .right_0()
        .top(move || Length::from(lh() * row as f32))
        .h(move || Length::from(lh2()));
    place.child(dynamic(move || {
        let (cell, old_number) = cell.get()?;
        let tint = cell.kind;
        let mut el = view()
            .absolute()
            .inset_0()
            .flex_row()
            .items_center()
            .bg(move || line_tint(props, tint).unwrap_or(AdaptiveColor::TRANSPARENT));
        if props.line_numbers.get() {
            let numbers: Vec<(Option<usize>, DiffSide)> = match column {
                Column::Old => vec![(cell.number, DiffSide::Old)],
                Column::New => vec![(cell.number, DiffSide::New)],
                Column::Both => {
                    let (old, new) = match tint {
                        Tint::Deleted => (cell.number, None),
                        Tint::Added => (None, cell.number),
                        _ => (old_number, cell.number),
                    };
                    vec![(old, DiffSide::Old), (new, DiffSide::New)]
                }
            };
            for (number, side_of) in numbers {
                let dv = dv.clone();
                let label = number.map(|n| n.to_string()).unwrap_or_default();
                let mut list = vec![DiffViewPart::LineNumber];
                match tint {
                    Tint::Added => list.push(DiffViewPart::LineNumberAdded),
                    Tint::Deleted => list.push(DiffViewPart::LineNumberDeleted),
                    _ => {}
                }
                let line = number.map(|n| n.saturating_sub(1));
                let number = parts(
                    text(label)
                        .w(Length::from(number_width()))
                        .ml(px(PAD))
                        .text_right()
                        .text_color(move || theme().muted_foreground),
                    props,
                    list,
                )
                .on_press(move || {
                    if let Some(line) = line {
                        dv.line_number_press(side_of, line);
                    }
                });
                el = el.child(number);
            }
        }
        if props.indicators.get() == DiffIndicators::Classic {
            let (sign, which) = match tint {
                Tint::Added => ("+", Some(DiffViewPart::IndicatorAdded)),
                Tint::Deleted => ("−", Some(DiffViewPart::IndicatorDeleted)),
                _ => ("", None),
            };
            let sign = text(sign)
                .ml(px(PAD))
                .w(Length::from(sign_width()))
                .text_color(move || match tint {
                    Tint::Added => ADDED,
                    Tint::Deleted => DELETED,
                    _ => theme().muted_foreground,
                });
            el = el.child(match which {
                Some(p) => part(sign, props, p),
                None => sign,
            });
        }
        if props.indicators.get() == DiffIndicators::Bars {
            let which = match tint {
                Tint::Added => Some((DiffViewPart::IndicatorAdded, ADDED)),
                Tint::Deleted => Some((DiffViewPart::IndicatorDeleted, DELETED)),
                _ => None,
            };
            if let Some((p, color)) = which {
                el = el.child(part(
                    view()
                        .absolute()
                        .right_0()
                        .top_0()
                        .bottom_0()
                        .w(px(BAR))
                        .bg(color),
                    props,
                    p,
                ));
            }
        }
        Some(el)
    }))
}

/// The tint of a row's background, `None` for none.
fn line_tint(props: DiffViewProps, tint: Tint) -> Option<AdaptiveColor> {
    match tint {
        Tint::Context => None,
        Tint::Filler => Some(theme().muted.opacity(0.5)),
        Tint::Added if props.background.get() => Some(ADDED.opacity(0.12)),
        Tint::Deleted if props.background.get() => Some(DELETED.opacity(0.12)),
        _ => None,
    }
}

/// The text of a side, in a horizontal scroll view.
fn content(dv: Dv, column: Column, rows: Memo<Vec<usize>>) -> impl IntoElement {
    let props = dv.props;
    let width = {
        let dv = dv.clone();
        move || {
            dv.rev.get();
            let st = dv.st.borrow();
            let side = match column {
                Column::Old => DiffSide::Old,
                Column::New => DiffSide::New,
                Column::Both => {
                    return st
                        .text_width(DiffSide::Old)
                        .max(st.text_width(DiffSide::New));
                }
            };
            st.text_width(side)
        }
    };
    let height = {
        let dv = dv.clone();
        let lh = line_height(&dv);
        move || {
            dv.rev.get();
            lh() * dv.st.borrow().rows.len() as f32
        }
    };
    let lh = line_height(&dv);
    scroll().horizontal().flex_1().child(
        part(
            view()
                .relative()
                .min_w(relative(1.0))
                .w(move || Length::from(width() + px(PAD * 2.0)))
                .h(move || Length::from(height())),
            props,
            DiffViewPart::Content,
        )
        .child(each(
            rows,
            |row: &usize| *row,
            move |row, _| line_row(dv.clone(), row.get_untracked(), column, lh.clone()),
        )),
    )
}

fn line_row(
    dv: Dv,
    row: usize,
    column: Column,
    lh: impl Fn() -> Points + Clone + 'static,
) -> impl IntoElement {
    let props = dv.props;
    let cell = cell_memo(&dv, row, column);
    let lh2 = lh.clone();
    let place = view()
        .absolute()
        .left_0()
        .right_0()
        .top(move || Length::from(lh() * row as f32))
        .h(move || Length::from(lh2()));
    place.child(dynamic(move || {
        let (cell, _) = cell.get()?;
        let tint = cell.kind;
        let mut list = vec![DiffViewPart::Line];
        match tint {
            Tint::Added => list.push(DiffViewPart::LineAdded),
            Tint::Deleted => list.push(DiffViewPart::LineDeleted),
            Tint::Filler => list.push(DiffViewPart::Filler),
            Tint::Context => {}
        }
        let spans = cell
            .spans
            .into_iter()
            .map(|span| span_view(props, span, tint))
            .collect::<Vec<_>>();
        Some(
            parts(
                view()
                    .absolute()
                    .inset_0()
                    .bg(move || line_tint(props, tint).unwrap_or(AdaptiveColor::TRANSPARENT)),
                props,
                list,
            )
            .child(text("").absolute().top_0().left(px(PAD)).children(spans)),
        )
    }))
}

fn span_view(props: DiffViewProps, span: Span, tint: Tint) -> AnyElement {
    let mut el = text(span.text);
    if let Some(token) = span.token {
        *el.style() = token.style();
    }
    if span.changed {
        let (color, which) = match tint {
            Tint::Deleted => (DELETED, DiffViewPart::WordDeleted),
            _ => (ADDED, DiffViewPart::WordAdded),
        };
        el = part(el.bg(color.opacity(0.3)), props, which);
    }
    el.into_element()
}

/// The separators in view, across the view.
fn separators(dv: Dv, rows: Memo<Vec<usize>>) -> impl IntoElement {
    let lh = line_height(&dv);
    let list = {
        let dv = dv.clone();
        computed(move || {
            dv.rev.get();
            let st = dv.st.borrow();
            rows.get()
                .into_iter()
                .filter_map(|row| match st.rows.get(row)? {
                    Row::Separator {
                        gap,
                        lines,
                        first,
                        last,
                        header,
                    } => Some((
                        row,
                        DiffSeparator {
                            gap: *gap as u32,
                            lines: *lines as u32,
                            first: *first,
                            last: *last,
                            header: header.clone().map(SharedString::from),
                        },
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
    };
    each(
        list,
        |(row, sep): &(usize, DiffSeparator)| (*row, sep.clone()),
        move |item, _| {
            let (row, sep) = item.get_untracked();
            separator(dv.clone(), row, sep, lh.clone())
        },
    )
}

fn separator(
    dv: Dv,
    row: usize,
    sep: DiffSeparator,
    lh: impl Fn() -> Points + Clone + 'static,
) -> impl IntoElement {
    let props = dv.props;
    let lh2 = lh.clone();
    let place = move |el: View| {
        el.absolute()
            .left_0()
            .right_0()
            .top(move || Length::from(lh() * row as f32))
            .h(move || Length::from(lh2()))
    };
    if let Some(render) = props.render_separator.get_untracked() {
        return place(view())
            .child(render.render(format!("gap:{}", sep.gap), sep).flex_1())
            .into_element();
    }
    let label = match &sep.header {
        Some(header) => header.to_string(),
        None => format!("⋯ {}", sep.lines),
    };
    let gap = sep.gap as usize;
    let expandable = sep.header.is_none();
    part(
        place(view())
            .flex_row()
            .items_center()
            .px(px(PAD))
            .bg(move || theme().muted)
            .text_color(move || theme().muted_foreground)
            .child(text(label)),
        props,
        DiffViewPart::Separator,
    )
    .on_press(move || {
        if expandable {
            dv.expand(gap, DiffExpand::All, 0);
        }
    })
    .into_element()
}

/// The app's actions of the changes whose first row is in view.
fn hunk_actions(dv: Dv, rows: Memo<Vec<usize>>) -> impl IntoElement {
    let props = dv.props;
    let lh = line_height(&dv);
    let list = {
        let dv = dv.clone();
        computed(move || {
            dv.rev.get();
            if props.render_hunk_actions.get().is_none() {
                return Vec::new();
            }
            let st = dv.st.borrow();
            let hunks = st.hunks();
            rows.get()
                .into_iter()
                .filter_map(|row| {
                    let Row::Line { hunk: Some(h), .. } = st.rows.get(row)? else {
                        return None;
                    };
                    let starts = match row.checked_sub(1).and_then(|r| st.rows.get(r)) {
                        Some(Row::Line { hunk: Some(p), .. }) => p != h,
                        _ => true,
                    };
                    starts
                        .then(|| (row, hunks.get(*h).copied()))
                        .and_then(|(r, h)| Some((r, h?)))
                })
                .collect::<Vec<_>>()
        })
    };
    each(
        list,
        |(row, hunk)| (*row, *hunk),
        move |item, _| {
            let (row, hunk) = item.get_untracked();
            let lh = lh.clone();
            // A stable view per change, which `each` keeps while the change
            // stays in view.
            part(
                view()
                    .absolute()
                    .right(px(PAD))
                    .top(move || Length::from(lh() * row as f32)),
                props,
                DiffViewPart::HunkActions,
            )
            .child(dynamic(move || {
                let render = props.render_hunk_actions.get()?;
                Some(render.render(format!("hunk:{}", hunk.index), hunk))
            }))
        },
    )
}
