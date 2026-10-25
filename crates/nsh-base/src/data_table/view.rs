//! The table's view, in layers:
//!
//! - the *hit layer*: a vertical scroll view of a horizontal one of an empty
//!   box of the content's size (scaled past a few million points: `axis.rs`).
//!   The platforms scroll it, and its box takes presses, hovers and the
//!   context menu;
//! - the *body* over it, receiving no pointer: the cells of the rows and
//!   columns in view (and a reach around them), placed where the table last
//!   scrolled to. A scroll view moves its content at once, and the cells a
//!   move needs come with the next commit, which moves the body with them,
//!   so no frame shows the content without its cells. Its three regions are
//!   the row headers, the fixed columns (which move with the rows only) and
//!   the scrolling columns (which move both ways);
//! - the *header* above it, interactive: group labels and titles, in the
//!   same three regions, the columns' resize handles and the drop line of a
//!   column being moved;
//! - on the desktops, a horizontal scroll bar over the body's bottom edge,
//!   which moves the table's horizontal scroll view (whose own bar lies at
//!   the end of a content as tall as all the rows).

use std::{cell::Cell, rc::Rc};

use nsh_core::ui::{EventKind, EventPayload, Style, prelude::*};

use super::{
    DataTableAlign, DataTableCell, DataTableHeaderCell, DataTablePart, DataTableProps,
    DataTableSortDirection,
    controller::{CELL_PAD, Dt, MOVE_SLOP, PressRegion, SORT_WIDTH},
    selection::Selection,
};
use crate::{HandleEdge, theme};

/// The thickness of the desktops' horizontal scroll bar's band.
const STRIP: f32 = 12.0;
/// The room the drop line takes.
const DROP_LINE: f32 = 2.0;
/// Rows and columns that scaled content places its rows and columns
/// relative to (`controller.rs`).
const ROW_CHUNK: usize = 1024;
const COLUMN_CHUNK: usize = 256;

/// `el` with the classes and styles of the parts `list` gives, in order
/// (later parts win), kept up to date.
fn parts<E: Element>(
    el: E,
    props: DataTableProps,
    list: impl Fn() -> Vec<DataTablePart> + Clone + 'static,
) -> E {
    let styles = list.clone();
    el.app_class(move || {
        props.classes.with(|c| {
            let list = list();
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
                styles()
                    .iter()
                    .filter_map(|p| s.get(*p))
                    .flat_map(|s| s.decls().iter().cloned()),
            )
        })
    })
}

fn part<E: Element>(el: E, props: DataTableProps, which: DataTablePart) -> E {
    parts(el, props, move || vec![which])
}

/// The table.
pub(crate) fn render(dt: Dt) -> impl IntoElement {
    let props = dt.props;
    let layout = dt.clone();
    let custom_cell = computed(move || props.render_cell.with(Option::is_some));
    let custom_head = computed(move || props.render_header_cell.with(Option::is_some));
    let (body_dt, header_dt, hit_dt, strip_dt, over_dt) =
        (dt.clone(), dt.clone(), dt.clone(), dt.clone(), dt.clone());
    let (body_dt2, header_dt2) = (dt.clone(), dt.clone());
    let family = move || {
        props
            .font_family
            .get()
            .or_else(|| theme().font_sans.clone())
    };
    part(
        view()
            .relative()
            .overflow_hidden()
            .rounded(move || theme().radius)
            .border_1()
            .border_color(move || theme().border)
            .bg(move || theme().background)
            .text_color(move || theme().foreground)
            .text_size(move || Length::from(props.font_size.get()))
            .map(move |v| match family() {
                Some(f) => v.font_family(f),
                None => v,
            }),
        props,
        DataTablePart::Root,
    )
    .child(
        // The frame is the padding box: the layers lie in it, and its size
        // is the viewport's.
        view()
            .absolute()
            .inset_0()
            .on_layout(move |b: &Bounds<Points>| layout.viewport.set(b.size))
            .child(hit_layer(hit_dt))
            .child(
                when(
                    move || custom_cell.get(),
                    move || body(body_dt.clone(), true),
                )
                .otherwise(move || body(body_dt2.clone(), false)),
            )
            .child(
                when(
                    move || custom_head.get(),
                    move || header(header_dt.clone(), true),
                )
                .otherwise(move || header(header_dt2.clone(), false)),
            )
            .child(strip(strip_dt))
            .child(overlays(over_dt)),
    )
}

// The hit layer.

/// The width of the content box, whole columns included.
fn content_width(dt: &Dt) -> f32 {
    let g = dt.geo();
    g.lead + g.fixed + g.x.extent() as f32
}

fn hit_layer(dt: Dt) -> impl IntoElement {
    let props = dt.props;
    let (scroll_dt, hscroll_dt, strip_dt) = (dt.clone(), dt.clone(), dt.clone());
    let (top_dt, height_dt, width_dt) = (dt.clone(), dt.clone(), dt.clone());
    let (press, hover, menu) = (dt.clone(), dt.clone(), dt.clone());
    let leave = dt.clone();
    let strip_x = Rc::new(Cell::new(0.0f32));
    let echo = strip_x.clone();
    scroll()
        .element_ref(&dt.vscroll)
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .top(move || Length::from(px(top_dt.geo().header)))
        .scrollbar(move || props.scrollbar.get())
        .on_scroll(move |e: &ScrollEvent| {
            scroll_dt.scroll_y.set(e.offset.y);
        })
        .child(
            view()
                .flex_row()
                .items_stretch()
                .h(move || Length::from(px(height_dt.geo().y.extent() as f32)))
                .child(
                    scroll()
                        .horizontal()
                        .element_ref(&dt.hscroll)
                        .flex_1()
                        .scrollbar(false)
                        .on_scroll(move |e: &ScrollEvent| {
                            hscroll_dt.scroll_x.set(e.offset.x);
                            // The desktops' bar follows (a move of under half
                            // a point is its own echo).
                            if (e.offset.x.0 - echo.get()).abs() > 0.5 {
                                echo.set(e.offset.x.0);
                                hscroll_dt.strip.scroll_to(
                                    Point {
                                        x: e.offset.x,
                                        y: Points::ZERO,
                                    },
                                    false,
                                );
                            }
                        })
                        .child(
                            view()
                                .relative()
                                .min_w(relative(1.0))
                                .w(move || Length::from(px(content_width(&width_dt))))
                                .h(move || Length::from(px(strip_dt.geo().y.extent() as f32)))
                                .on(EventKind::Press, move |p: &EventPayload| {
                                    if let EventPayload::Press(e) = p {
                                        press.press(e.position, e.clicks);
                                    }
                                })
                                .on_hover_move(move |e: &HoverEvent| hover.hover_at(e.position))
                                .on_hover_out(move || {
                                    if leave.hover.get_untracked().is_some() {
                                        leave.hover.set(None);
                                    }
                                })
                                .on_context_menu(move |e: &ContextMenuEvent| {
                                    menu.context_menu(e.position, e.page_position)
                                }),
                        ),
                ),
        )
        .map(move |v| {
            // Keep a handle on the echo for the bar below.
            let _ = &strip_x;
            v
        })
}

/// The desktops' horizontal scroll bar: a band over the body's bottom edge
/// whose own scroll view the bar belongs to, moving the table's.
fn strip(dt: Dt) -> impl IntoElement {
    let props = dt.props;
    let shown = {
        let dt = dt.clone();
        move || {
            is_desktop() && props.scrollbar.get() && {
                let g = dt.geo();
                g.x.extent() > g.x.viewport
            }
        }
    };
    let width_dt = dt.clone();
    when(shown, move || {
        let (scroll_dt, width_dt) = (dt.clone(), width_dt.clone());
        scroll()
            .horizontal()
            .element_ref(&dt.strip)
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(px(STRIP))
            .on_scroll(move |e: &ScrollEvent| {
                if (e.offset.x - scroll_dt.scroll_x.get_untracked()).0.abs() > 0.5 {
                    scroll_dt.hscroll.scroll_to(
                        Point {
                            x: e.offset.x,
                            y: Points::ZERO,
                        },
                        false,
                    );
                }
            })
            .child(
                view()
                    .h(px(1.0))
                    .w(move || Length::from(px(content_width(&width_dt)))),
            )
    })
}

// The body.

/// What the three regions of the body share: the windows of rows and
/// columns made, and where they are placed from.
#[derive(Clone, Copy)]
struct Windows {
    rows: Memo<Vec<usize>>,
    scroll_cols: Memo<Vec<usize>>,
    fixed_cols: Memo<Vec<usize>>,
    /// The first row of the chunk of rows that rows are placed from.
    row_origin: Memo<usize>,
    /// The display index of the first scrolling column of its chunk, whence
    /// the scrolling columns are placed.
    col_origin: Memo<usize>,
    /// How wide the made scrolling columns reach from `col_origin`.
    span: Memo<f32>,
}

fn windows(dt: &Dt) -> Windows {
    let d = dt.clone();
    let rows = computed(move || d.row_window());
    let d = dt.clone();
    let scroll_cols = computed(move || d.col_window());
    let d = dt.clone();
    let fixed_cols = computed(move || {
        d.layout_rev.get();
        let st = d.st.borrow();
        (0..st.columns.fixed_len())
            .map(|display| st.columns.def(display))
            .collect()
    });
    let d = dt.clone();
    let row_origin = computed(move || d.visible_rows().start / ROW_CHUNK * ROW_CHUNK);
    let d = dt.clone();
    let col_origin = computed(move || {
        let fixed = d.st.borrow().columns.fixed_len();
        (d.visible_cols().start / COLUMN_CHUNK * COLUMN_CHUNK).max(fixed)
    });
    let d = dt.clone();
    let span = computed(move || {
        d.layout_rev.get();
        let list = scroll_cols.get();
        let origin = col_origin.get();
        let st = d.st.borrow();
        let fixed_width = st.columns.fixed_width();
        let from = st.columns.start(origin.min(st.columns.len())) - fixed_width;
        let end = list
            .iter()
            .filter_map(|&def| st.columns.display_of(def))
            .map(|display| st.columns.start(display) + st.columns.display_width(display) as f64)
            .fold(0.0, f64::max);
        ((end - fixed_width) - from).max(0.0) as f32
    });
    Windows {
        rows,
        scroll_cols,
        fixed_cols,
        row_origin,
        col_origin,
        span,
    }
}

/// Which of the body's regions a row or a header is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    /// The row numbers.
    Lead,
    /// The fixed columns.
    Fixed,
    /// The scrolling columns.
    Scroll,
}

fn body(dt: Dt, custom: bool) -> impl IntoElement {
    let props = dt.props;
    let w = windows(&dt);
    let top_dt = dt.clone();
    let (lead_dt, fixed_dt, scroll_dt) = (dt.clone(), dt.clone(), dt.clone());
    view()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .top(move || Length::from(px(top_dt.geo().header)))
        .overflow_hidden()
        .pointer_events(PointerEvents::None)
        .child(region(scroll_dt, Region::Scroll, w, custom))
        .child(region(fixed_dt, Region::Fixed, w, custom))
        .child(region(lead_dt, Region::Lead, w, custom))
        .child(edge(dt.clone(), props))
}

/// The line at the fixed columns' trailing edge.
fn edge(dt: Dt, props: DataTableProps) -> impl IntoElement {
    let left_dt = dt.clone();
    let shown = {
        let dt = dt.clone();
        move || dt.st.borrow().columns.fixed_len() > 0 && dt.layout_rev.get() > 0
    };
    when(shown, move || {
        let dt = left_dt.clone();
        part(
            view()
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(1.0))
                .left(move || {
                    let g = dt.geo();
                    Length::from(px(g.lead + g.fixed - 1.0))
                })
                .bg(move || theme().border),
            props,
            DataTablePart::FixedEdge,
        )
    })
}

/// One region of the body: its box (clipped, holding the rows) and the
/// rows' content.
fn region(dt: Dt, which: Region, w: Windows, custom: bool) -> impl IntoElement {
    let (left_dt, width_dt, content_dt) = (dt.clone(), dt.clone(), dt.clone());
    let rows_dt = dt.clone();
    let shown = {
        let dt = dt.clone();
        move || match which {
            Region::Lead => dt.props.row_header.get(),
            Region::Fixed => dt.layout_rev.get() > 0 && dt.st.borrow().columns.fixed_len() > 0,
            Region::Scroll => true,
        }
    };
    when(shown, move || {
        let (left_dt, width_dt, content_dt, rows_dt) = (
            left_dt.clone(),
            width_dt.clone(),
            content_dt.clone(),
            rows_dt.clone(),
        );
        let x_dt = content_dt.clone();
        view()
            .absolute()
            .top_0()
            .bottom_0()
            .overflow_hidden()
            .left(move || {
                let g = left_dt.geo();
                Length::from(px(match which {
                    Region::Lead => 0.0,
                    Region::Fixed => g.lead,
                    Region::Scroll => g.lead + g.fixed,
                }))
            })
            .map(move |v| match which {
                Region::Lead => v.w(move || Length::from(px(width_dt.geo().lead))),
                Region::Fixed => v.w(move || Length::from(px(width_dt.geo().fixed))),
                Region::Scroll => v.right_0(),
            })
            .child(
                // The rows' origin: the first row of its chunk, scrolled
                // by the content offsets.
                view()
                    .absolute()
                    .top(move || {
                        let g = content_dt.geo();
                        let origin = w.row_origin.get() as f64 * g.row_height as f64;
                        Length::from(px((origin - g.cy) as f32))
                    })
                    .left(move || match which {
                        Region::Scroll => {
                            let g = x_dt.geo();
                            let origin = x_dt.scroll_start(w.col_origin.get());
                            Length::from(px((origin - g.cx) as f32))
                        }
                        _ => Length::from(px(0.0)),
                    })
                    .child(each(
                        w.rows,
                        |row: &usize| *row,
                        move |row, _| {
                            body_row(rows_dt.clone(), row.get_untracked(), which, w, custom)
                        },
                    )),
            )
    })
}

/// The `cell_state` of a row of the three regions.
fn row_parts(dt: &Dt, row: usize) -> impl Fn() -> Vec<DataTablePart> + Clone + 'static {
    let props = dt.props;
    let hovered = {
        let hover = dt.hover;
        computed(move || hover.get() == Some(row))
    };
    let selected = {
        let dt = dt.clone();
        computed(move || {
            dt.selection().row() == Some(row) && matches!(dt.selection(), Selection::Row(_))
        })
    };
    move || {
        let mut list = vec![DataTablePart::Row];
        if props.stripe.get() && row % 2 == 1 {
            list.push(DataTablePart::RowOdd);
        }
        if hovered.get() {
            list.push(DataTablePart::RowHover);
        }
        if selected.get() {
            list.push(DataTablePart::RowSelected);
        }
        list
    }
}

/// The default look of a row's background, by what it is.
fn row_background(dt: &Dt, row: usize) -> impl Fn() -> AdaptiveColor + Clone + 'static {
    let props = dt.props;
    let hover = dt.hover;
    let dt = dt.clone();
    move || {
        let selected = matches!(dt.selection(), Selection::Row(r) if r == row);
        if selected {
            theme().primary.opacity(0.12)
        } else if hover.get() == Some(row) {
            theme().muted.opacity(0.6)
        } else if props.stripe.get() && row % 2 == 1 {
            theme().muted.opacity(0.35)
        } else {
            AdaptiveColor::TRANSPARENT
        }
    }
}

fn body_row(dt: Dt, row: usize, which: Region, w: Windows, custom: bool) -> impl IntoElement {
    let props = dt.props;
    let (top_dt, height_dt, width_dt) = (dt.clone(), dt.clone(), dt.clone());
    let list = row_parts(&dt, row);
    let background = row_background(&dt, row);
    let press_dt = dt.clone();
    let el = parts(
        view()
            .absolute()
            .left_0()
            .on(EventKind::Press, move |p: &EventPayload| {
                // A press without a position (a script's) lands at the row's start.
                let (x, clicks) = match p {
                    EventPayload::Press(e) => (e.position.x, e.clicks),
                    _ => (Points(0.0), 1),
                };
                let region = match which {
                    Region::Lead => PressRegion::Lead,
                    Region::Fixed => PressRegion::Fixed,
                    Region::Scroll => PressRegion::Scroll {
                        origin: w.col_origin.get_untracked(),
                    },
                };
                press_dt.press_row(row, region, x, clicks);
            })
            .top(move || {
                let origin = w.row_origin.get() as i64;
                Length::from(px((row as i64 - origin) as f32 * top_dt.row_height()))
            })
            .h(move || Length::from(px(height_dt.row_height())))
            .border_b_1()
            .border_color(move || theme().border.opacity(0.6))
            .bg(background),
        props,
        list,
    );
    match which {
        Region::Lead => el
            .w(move || Length::from(px(width_dt.geo().lead)))
            .child(row_number(dt, row)),
        Region::Fixed => el
            .w(move || Length::from(px(width_dt.geo().fixed)))
            .child(each(
                w.fixed_cols,
                |def: &usize| *def,
                move |def, _| cell(dt.clone(), row, def.get_untracked(), which, w, custom),
            )),
        Region::Scroll => el.w(move || Length::from(px(w.span.get()))).child(each(
            w.scroll_cols,
            |def: &usize| *def,
            move |def, _| cell(dt.clone(), row, def.get_untracked(), which, w, custom),
        )),
    }
}

/// The number of a row, in the row header column.
fn row_number(dt: Dt, row: usize) -> impl IntoElement {
    let props = dt.props;
    part(
        view()
            .absolute()
            .inset_0()
            .flex_row()
            .items_center()
            .justify_center()
            .text_color(move || theme().muted_foreground),
        props,
        DataTablePart::RowHeader,
    )
    .child(text(format!("{}", row + 1)).number_of_lines(1))
}

/// What marks a cell besides its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct CellFlags {
    selected: bool,
    column_selected: bool,
    active: bool,
}

fn cell(
    dt: Dt,
    row: usize,
    def: usize,
    which: Region,
    w: Windows,
    custom: bool,
) -> impl IntoElement {
    let props = dt.props;
    let align = dt
        .st
        .borrow()
        .defs
        .get(def)
        .map_or(DataTableAlign::Start, |c| c.align);
    let flags = {
        let dt = dt.clone();
        computed(move || {
            dt.data_rev.get();
            let st = dt.st.borrow();
            CellFlags {
                selected: st.selection == Selection::Cell(row, def),
                column_selected: st.selection == Selection::Column(def),
                active: st.active == Some((Some(row), Some(def))),
            }
        })
    };
    let list = move || {
        let mut list = vec![DataTablePart::Cell];
        let f = flags.get();
        if f.column_selected {
            list.push(DataTablePart::CellColumnSelected);
        }
        if f.selected {
            list.push(DataTablePart::CellSelected);
        }
        if f.active {
            list.push(DataTablePart::CellActive);
        }
        list
    };
    let left_dt = dt.clone();
    let width_dt = dt.clone();
    let el = parts(
        view()
            .absolute()
            .top_0()
            .bottom_0()
            .left(move || {
                left_dt.layout_rev.get();
                let st = left_dt.st.borrow();
                let Some(display) = st.columns.display_of(def) else {
                    return Length::from(px(0.0));
                };
                let start = st.columns.start(display);
                let origin = match which {
                    Region::Scroll => st.columns.start(w.col_origin.get().min(st.columns.len())),
                    _ => 0.0,
                };
                Length::from(px((start - origin) as f32))
            })
            .w(move || {
                width_dt.layout_rev.get();
                Length::from(px(width_dt.st.borrow().columns.width(def)))
            })
            .flex_row()
            .items_center()
            .px(px(CELL_PAD))
            .overflow_hidden()
            .map(move |v| match align {
                DataTableAlign::Start => v.justify_start(),
                DataTableAlign::Center => v.justify_center(),
                DataTableAlign::End => v.justify_end(),
            })
            .style_when(
                move || flags.get().selected,
                |s| s.border_1().border_color(move || theme().primary),
            )
            .style_when(
                move || flags.get().column_selected,
                |s| s.bg(move || theme().primary.opacity(0.08)),
            )
            .style_when(
                move || flags.get().active,
                |s| {
                    s.border_1()
                        .border_color(move || theme().primary.opacity(0.5))
                },
            ),
        props,
        list,
    );
    if custom {
        let render_dt = dt.clone();
        el.child(dynamic(move || {
            let render = props.render_cell.get()?;
            let value = {
                render_dt.data_rev.get();
                let st = render_dt.st.borrow();
                DataTableCell {
                    row: row as u32,
                    column: def as u32,
                    key: st.defs.get(def)?.key.clone(),
                    text: SharedString::default(),
                    selected: matches!(st.selection, Selection::Row(r) if r == row)
                        || st.selection == Selection::Cell(row, def),
                }
            };
            let value = DataTableCell {
                text: render_dt.cell_text(row, def),
                ..value
            };
            Some(
                render
                    .render(format!("cell:{row}:{def}"), value)
                    .flex_1()
                    .min_w(px(0.0)),
            )
        }))
    } else {
        let text_dt = dt;
        el.child(
            text(move || text_dt.cell_text(row, def))
                .number_of_lines(1)
                .min_w(px(0.0)),
        )
    }
}

// The header.

/// A label over columns, placed from a display index.
#[derive(Debug, Clone, PartialEq)]
struct GroupCell {
    level: usize,
    label: SharedString,
    from: usize,
    to: usize,
}

/// The groups of every level that start in `range` of the display indexes
/// (or, in the fixed region, before the first scrolling column).
fn group_cells(dt: &Dt, scroll: bool, window: &[usize], origin: usize) -> Vec<GroupCell> {
    let (fixed, n) = {
        let st = dt.st.borrow();
        (st.columns.fixed_len(), st.columns.len())
    };
    let first = window
        .first()
        .and_then(|&d| dt.st.borrow().columns.display_of(d));
    let last = window
        .iter()
        .filter_map(|&d| dt.st.borrow().columns.display_of(d))
        .max();
    let mut cells = Vec::new();
    dt.props.groups.with(|levels| {
        for (level, groups) in levels.iter().enumerate() {
            let mut at = 0usize;
            for group in groups {
                let from = at;
                let to = (at + group.span.max(1) as usize).min(n);
                at = to;
                if from >= n {
                    break;
                }
                let keep = if scroll {
                    // Overlapping the made columns (or the chunk they are
                    // placed from).
                    let low = first.unwrap_or(origin).min(origin);
                    to > fixed.max(low) && last.is_some_and(|l| from <= l) && from >= low.min(from)
                } else {
                    from < fixed
                };
                if keep {
                    cells.push(GroupCell {
                        level,
                        label: group.label.clone(),
                        from,
                        to,
                    });
                }
            }
        }
    });
    cells
}

fn header(dt: Dt, custom: bool) -> impl IntoElement {
    let props = dt.props;
    let w = windows(&dt);
    let height_dt = dt.clone();
    let (corner_dt, fixed_dt, scroll_dt) = (dt.clone(), dt.clone(), dt.clone());
    part(
        view()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(move || Length::from(px(height_dt.geo().header)))
            .overflow_hidden()
            .border_b_1()
            .border_color(move || theme().border)
            .bg(move || theme().muted.opacity(0.5))
            .text_color(move || theme().muted_foreground),
        props,
        DataTablePart::Header,
    )
    .child(header_region(scroll_dt, Region::Scroll, w, custom))
    .child(header_region(fixed_dt, Region::Fixed, w, custom))
    .child(corner(corner_dt))
    .child(move_line(dt))
}

/// The corner above the row headers.
fn corner(dt: Dt) -> impl IntoElement {
    let props = dt.props;
    let width_dt = dt.clone();
    when(
        move || dt.props.row_header.get(),
        move || {
            let width_dt = width_dt.clone();
            part(
                view()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(move || Length::from(px(width_dt.geo().lead)))
                    .bg(move || theme().muted.opacity(0.5)),
                props,
                DataTablePart::RowHeaderCorner,
            )
        },
    )
}

fn header_region(dt: Dt, which: Region, w: Windows, custom: bool) -> impl IntoElement {
    let (left_dt, width_dt, x_dt, titles_dt, groups_dt) =
        (dt.clone(), dt.clone(), dt.clone(), dt.clone(), dt.clone());
    let shown = {
        let dt = dt.clone();
        move || match which {
            Region::Fixed => dt.layout_rev.get() > 0 && dt.st.borrow().columns.fixed_len() > 0,
            _ => true,
        }
    };
    let groups = {
        let dt = dt.clone();
        computed(move || {
            dt.layout_rev.get();
            let window = w.scroll_cols.get();
            group_cells(&dt, which == Region::Scroll, &window, w.col_origin.get())
        })
    };
    when(shown, move || {
        let (left_dt, width_dt, x_dt, titles_dt, groups_dt) = (
            left_dt.clone(),
            width_dt.clone(),
            x_dt.clone(),
            titles_dt.clone(),
            groups_dt.clone(),
        );
        let list = match which {
            Region::Fixed => w.fixed_cols,
            _ => w.scroll_cols,
        };
        view()
            .absolute()
            .top_0()
            .bottom_0()
            .overflow_hidden()
            .left(move || {
                let g = left_dt.geo();
                Length::from(px(match which {
                    Region::Scroll => g.lead + g.fixed,
                    _ => g.lead,
                }))
            })
            .map(move |v| match which {
                Region::Scroll => v.right_0(),
                _ => v.w(move || Length::from(px(width_dt.geo().fixed))),
            })
            .child(
                view()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(move || match which {
                        Region::Scroll => {
                            let g = x_dt.geo();
                            let origin = x_dt.scroll_start(w.col_origin.get());
                            Length::from(px((origin - g.cx) as f32))
                        }
                        _ => Length::from(px(0.0)),
                    })
                    .child(each(
                        groups,
                        |g: &GroupCell| (g.level, g.from),
                        move |group, _| {
                            group_cell(groups_dt.clone(), group.get_untracked(), which, w)
                        },
                    ))
                    .child(each(
                        list,
                        |def: &usize| *def,
                        move |def, _| {
                            title(titles_dt.clone(), def.get_untracked(), which, w, custom)
                        },
                    )),
            )
    })
}

/// A label over columns.
fn group_cell(dt: Dt, group: GroupCell, which: Region, w: Windows) -> impl IntoElement {
    let props = dt.props;
    let (left_dt, width_dt, top_dt) = (dt.clone(), dt.clone(), dt.clone());
    part(
        view()
            .absolute()
            .top(move || {
                let levels = top_dt.header_levels();
                let h = top_dt.geo().header / levels as f32;
                // Lowest level first: it is the one above the titles.
                Length::from(px(h * (levels - 2 - group.level.min(levels - 2)) as f32))
            })
            .h(move || {
                let g = dt.geo();
                Length::from(px(g.header / dt.header_levels() as f32))
            })
            .left(move || {
                left_dt.layout_rev.get();
                let st = left_dt.st.borrow();
                let origin = match which {
                    Region::Scroll => st.columns.start(w.col_origin.get().min(st.columns.len())),
                    _ => 0.0,
                };
                Length::from(px((st.columns.start(group.from) - origin) as f32))
            })
            .w(move || {
                width_dt.layout_rev.get();
                let st = width_dt.st.borrow();
                let end = st.columns.start(group.to.min(st.columns.len()));
                Length::from(px((end - st.columns.start(group.from)) as f32))
            })
            .flex_row()
            .items_center()
            .justify_center()
            .px(px(CELL_PAD))
            .border_r_1()
            .border_b_1()
            .border_color(move || theme().border)
            .overflow_hidden(),
        props,
        DataTablePart::GroupCell,
    )
    .child(text(group.label.clone()).number_of_lines(1))
}

/// The title of a column.
fn title(dt: Dt, def: usize, which: Region, w: Windows, custom: bool) -> impl IntoElement {
    let props = dt.props;
    let st_dt = dt.clone();
    let (align, sortable) = {
        let st = dt.st.borrow();
        let column = st.defs.get(def);
        (
            column.map_or(DataTableAlign::Start, |c| c.align),
            column.is_some_and(|c| c.sortable),
        )
    };
    let sorted = {
        let dt = dt.clone();
        computed(move || {
            let key = dt.st.borrow().defs.get(def).map(|c| c.key.clone())?;
            props
                .sort
                .get()
                .filter(|s| s.key == key)
                .map(|s| s.direction)
        })
    };
    let selected = {
        let dt = dt.clone();
        computed(move || dt.selection() == Selection::Column(def))
    };
    let dragging = {
        let dt = dt.clone();
        computed(move || {
            dt.column_drag
                .get()
                .is_some_and(|d| d.moving && dt.st.borrow().columns.display_of(def) == Some(d.from))
        })
    };
    let list = move || {
        let mut list = vec![DataTablePart::HeaderCell];
        if sorted.get().is_some() {
            list.push(DataTablePart::HeaderCellSorted);
        }
        if selected.get() {
            list.push(DataTablePart::HeaderCellSelected);
        }
        list
    };
    let (left_dt, width_dt, top_dt, hgt_dt) = (dt.clone(), dt.clone(), dt.clone(), dt.clone());
    let (press_dt, claim_dt, grant_dt, move_dt, up_dt, end_dt) = (
        dt.clone(),
        dt.clone(),
        dt.clone(),
        dt.clone(),
        dt.clone(),
        dt.clone(),
    );
    let drag_dt = dt.clone();
    let moved = Rc::new(Cell::new(false));
    let (down_moved, up_moved) = (moved.clone(), moved.clone());
    let el = parts(
        view()
            .absolute()
            .top(move || {
                let levels = top_dt.header_levels();
                let h = top_dt.geo().header / levels as f32;
                Length::from(px(h * (levels - 1) as f32))
            })
            .h(move || Length::from(px(hgt_dt.row_height_for_header())))
            .left(move || {
                left_dt.layout_rev.get();
                let st = left_dt.st.borrow();
                let Some(display) = st.columns.display_of(def) else {
                    return Length::from(px(0.0));
                };
                let origin = match which {
                    Region::Scroll => st.columns.start(w.col_origin.get().min(st.columns.len())),
                    _ => 0.0,
                };
                Length::from(px((st.columns.start(display) - origin) as f32))
            })
            .w(move || {
                width_dt.layout_rev.get();
                Length::from(px(width_dt.st.borrow().columns.width(def)))
            })
            .flex_row()
            .items_center()
            .px(px(CELL_PAD))
            .gap(px(4.0))
            .border_r_1()
            .border_color(move || theme().border)
            .overflow_hidden()
            .map(move |v| match align {
                DataTableAlign::Start => v.justify_start(),
                DataTableAlign::Center => v.justify_center(),
                DataTableAlign::End => v.justify_end(),
            })
            .style_when(
                move || selected.get(),
                |s| s.bg(move || theme().primary.opacity(0.12)),
            )
            .style_when(move || dragging.get(), |s| s.opacity(0.5))
            .when(sortable, |v| v.cursor_pointer())
            .on(EventKind::Press, move |p: &EventPayload| {
                // A drag that moved the column is not a press.
                if up_moved.replace(false) {
                    return;
                }
                let clicks = match p {
                    EventPayload::Press(e) => e.clicks,
                    _ => 1,
                };
                press_dt.header_press(def, clicks);
            })
            .claim_touches(TouchClaim::on_move().ask(move |request: &ClaimRequest| {
                let moving = request.event.gesture.delta.x.0.abs() > MOVE_SLOP
                    && request.event.gesture.delta.x.0.abs()
                        > request.event.gesture.delta.y.0.abs();
                if moving && claim_dt.movable(def) {
                    request.claim();
                }
            }))
            .keep_touches(true)
            .on_responder_grant(move |_: &ResponderEvent| {
                down_moved.set(true);
                let display = grant_dt.st.borrow().columns.display_of(def);
                if let Some(display) = display {
                    grant_dt.move_start(display);
                }
            })
            .on_responder_move(move |e: &ResponderEvent| {
                let g = move_dt.geo();
                let at = {
                    let st = move_dt.st.borrow();
                    let display = st.columns.display_of(def).unwrap_or(0);
                    let start = st.columns.start(display) as f32;
                    if display < st.columns.fixed_len() {
                        g.lead + start
                    } else {
                        g.lead + start - g.cx as f32
                    }
                };
                // The pointer in the table: where the title starts and the
                // pointer in it.
                move_dt.move_to(at + e.location.x.0, e.gesture.delta.x.0);
            })
            .on_responder_release(move |_: &ResponderEvent| up_dt.move_end(true))
            .on_responder_terminate(move |_: &ResponderEvent| end_dt.move_end(false)),
        props,
        list,
    );
    let _ = &st_dt;
    let content = title_content(dt.clone(), def, sorted, sortable, custom, selected);
    el.child(content).child(resize_handle(drag_dt, def))
}

impl Dt {
    /// The height of one title row.
    pub(crate) fn row_height_for_header(&self) -> f32 {
        self.geo().header / self.header_levels() as f32
    }
}

fn title_content(
    dt: Dt,
    def: usize,
    sorted: Memo<Option<DataTableSortDirection>>,
    sortable: bool,
    custom: bool,
    selected: Memo<bool>,
) -> impl IntoElement {
    let props = dt.props;
    let label_dt = dt.clone();
    let label: AnyElement = if custom {
        let render_dt = dt.clone();
        dynamic(move || {
            let render = props.render_header_cell.get()?;
            let value = DataTableHeaderCell {
                column: def as u32,
                key: render_dt.st.borrow().defs.get(def)?.key.clone(),
                title: render_dt.title(def),
                sort: sorted.get(),
                selected: selected.get(),
            };
            Some(
                render
                    .render(format!("head:{def}"), value)
                    .flex_1()
                    .min_w(px(0.0)),
            )
        })
        .into_element()
    } else {
        text(move || label_dt.title(def))
            .number_of_lines(1)
            .min_w(px(0.0))
            .into_element()
    };
    let indicator_dt = dt;
    view()
        .flex_row()
        .items_center()
        .min_w(px(0.0))
        .gap(px(4.0))
        .child(label)
        .when(sortable, move |v| {
            let dt = indicator_dt.clone();
            v.child(part(
                text(move || match sorted.get() {
                    None => "⇅",
                    Some(DataTableSortDirection::Ascending) => "↑",
                    Some(DataTableSortDirection::Descending) => "↓",
                })
                .w(px(SORT_WIDTH - 4.0))
                .text_center()
                .opacity(move || if sorted.get().is_some() { 1.0_f32 } else { 0.5 })
                .on_press(move || dt.sort_next(def)),
                props,
                DataTablePart::SortIndicator,
            ))
        })
}

/// The band at a title's trailing edge that a drag resizes the column
/// with; a double press fits the column.
fn resize_handle(dt: Dt, def: usize) -> impl IntoElement {
    let props = dt.props;
    let enabled = {
        let dt = dt.clone();
        move || dt.resizable(def)
    };
    let (start, drag, end) = (dt.clone(), dt.clone(), dt.clone());
    when(enabled, move || {
        let (start, drag, end) = (start.clone(), drag.clone(), end.clone());
        handle(props, start, drag, end, def)
    })
}

fn handle(props: DataTableProps, start: Dt, drag: Dt, end: Dt, def: usize) -> impl IntoElement {
    resize_handle_component()
        .edge(HandleEdge::Trailing)
        .on_drag_start(move || start.resize_start(def))
        .on_drag_move(move |offset: &Points| drag.resize_move(*offset))
        .on_drag_end(move |_: &Points| end.resize_end())
        .child(part(
            view().w(px(1.0)).bg(move || theme().border),
            props,
            DataTablePart::ResizeHandle,
        ))
}

fn resize_handle_component() -> crate::ResizeHandleElement {
    crate::resize_handle()
}

/// The line where a dragged column will drop.
fn move_line(dt: Dt) -> impl IntoElement {
    let props = dt.props;
    let shown = {
        let dt = dt.clone();
        move || dt.column_drag.get().is_some_and(|d| d.moving)
    };
    when(shown, move || {
        let dt = dt.clone();
        part(
            view()
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(DROP_LINE))
                .bg(move || theme().primary)
                .left(move || {
                    let Some(drag) = dt.column_drag.get() else {
                        return Length::from(px(0.0));
                    };
                    dt.layout_rev.get();
                    let g = dt.geo();
                    let st = dt.st.borrow();
                    // At the trailing edge of the target when moving right.
                    let display = if drag.to > drag.from {
                        drag.to + 1
                    } else {
                        drag.to
                    };
                    let at = st.columns.start(display.min(st.columns.len()));
                    let x = if display <= st.columns.fixed_len() {
                        g.lead + at as f32
                    } else {
                        (g.lead + at as f32 - g.cx as f32).max(g.lead + g.fixed)
                    };
                    Length::from(px(x - DROP_LINE / 2.0))
                }),
            props,
            DataTablePart::MoveIndicator,
        )
    })
}

// What shows over the rows.

fn overlays(dt: Dt) -> impl IntoElement {
    let props = dt.props;
    let empty_dt = dt.clone();
    let top_dt = dt.clone();
    let is_empty = move || empty_dt.row_count() == 0 && !empty_dt.props.loading.get();
    let loading = move || props.loading.get();
    view()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .top(move || Length::from(px(top_dt.geo().header)))
        .pointer_events(PointerEvents::BoxNone)
        .child(when(is_empty, move || {
            part(
                view()
                    .absolute()
                    .inset_0()
                    .flex_col()
                    .items_center()
                    .justify_center(),
                props,
                DataTablePart::Empty,
            )
            .child(props.empty.place(|slot| slot))
        }))
        .child(when(loading, move || {
            view()
                .absolute()
                .inset_0()
                .child(props.loading_view.place(|slot| slot))
        }))
}
