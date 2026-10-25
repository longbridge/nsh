//! The editor's view: a vertical scroll view of a row of the gutter and
//! the text area (a horizontal scroll view without soft wrap), whose box
//! of rows holds, from back to front, the line backgrounds, indent guides,
//! search matches, selections, the rows of text, diagnostics' marks, the
//! carets, the composition and the sink. Only the rows in view (and a few
//! around them) are made, each a text whose spans are highlighted tokens.

use nsh_core::ui::{ContextMenuEvent, Cursor, EventKind, EventPayload, Style, prelude::*};

use super::{EditorDiagnosticSeverity, EditorPart, EditorProps, controller::Ed, state::RowContent};
use crate::theme;

/// The width of a caret.
pub(crate) const CARET_WIDTH: f32 = 2.0;
/// The height of a diagnostic's mark under its text.
const MARK_HEIGHT: f32 = 2.0;

/// Search matches' background (amber), and the current match's.
const MATCH: AdaptiveColor = AdaptiveColor::rgb(0xfbbf24, 0xd97706);
/// A warning's mark (amber).
const WARNING: AdaptiveColor = AdaptiveColor::rgb(0xd97706, 0xfbbf24);
/// An information's mark (blue).
const INFO: AdaptiveColor = AdaptiveColor::rgb(0x2563eb, 0x60a5fa);

/// `el` with part `part`'s classes and style, which follow the props.
fn part<E: Element>(el: E, props: EditorProps, part: EditorPart) -> E {
    el.app_class(move || props.classes.with(|c| c.get(part).cloned()))
        .inline_style(move || {
            props
                .styles
                .with(|s| s.get(part).cloned().unwrap_or_default())
        })
}

/// `el` with the classes and styles of `parts`, in order (later parts
/// win).
fn parts<E: Element>(el: E, props: EditorProps, list: &'static [EditorPart]) -> E {
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
                list.iter()
                    .filter_map(|p| s.get(*p))
                    .flat_map(|s| s.decls().iter().cloned()),
            )
        })
    })
}

/// The editor.
pub(crate) fn render(ed: Ed) -> impl IntoElement {
    let lh = {
        let ed = ed.clone();
        move || {
            ed.text_rev.get();
            ed.st.borrow().line_height()
        }
    };
    let font = {
        let ed = ed.clone();
        move || {
            ed.text_rev.get();
            ed.st.borrow().font.clone()
        }
    };
    let (font_a, font_b, font_c) = (font.clone(), font.clone(), font);
    let vscroll_ed = ed.clone();
    let layout_ed = ed.clone();
    let place_ed = ed.clone();
    let grow_ed = ed.clone();
    let content_ed = ed.clone();
    view()
        .element_ref(&ed.hooks.outer)
        .on_layout(move |_: &Bounds<Points>| place_ed.measure_place())
        .relative()
        .overflow_hidden()
        .h(move || match grow_height(&grow_ed) {
            Some(height) => Length::from(height),
            None => Length::Auto,
        })
        .text_color(move || theme().foreground)
        .font_family(move || font_a().family)
        .text_size(move || Length::from(font_b().size))
        .line_height(move || Length::from(font_c().line_height))
        .child(
            scroll()
                .element_ref(&ed.vscroll)
                .absolute()
                .inset_0()
                .on_scroll(move |e: &ScrollEvent| {
                    vscroll_ed.scroll_y.set(e.offset.y);
                    vscroll_ed.viewport.set(e.size);
                    // Lines scrolled into view far past the parse are guessed.
                    vscroll_ed.schedule_highlight();
                })
                .on_layout(move |b: &Bounds<Points>| layout_ed.viewport.set(b.size))
                .child(
                    view()
                        .flex_row()
                        .items_stretch()
                        .on_layout(move |b: &Bounds<Points>| {
                            content_ed.content_height.set(b.size.height)
                        })
                        .child(gutter(ed.clone()))
                        .child(text_area(ed.clone(), lh)),
                ),
        )
}

/// The gutter: line numbers and fold markers of the rows in view.
fn gutter(ed: Ed) -> impl IntoElement {
    let props = ed.props;
    let shown = move || props.line_numbers.get() || props.folding.get();
    let number_width = {
        let ed = ed.clone();
        move || {
            ed.text_rev.get();
            if !props.line_numbers.get() {
                return px(0.0);
            }
            let st = ed.st.borrow();
            let digits = st.buffer.line_count().max(1).ilog10() as usize + 1;
            let zero = measure_text(&st.font.css(), "0");
            zero * digits.max(2) as f32
        }
    };
    let fold_width = {
        let ed = ed.clone();
        move || {
            if !props.folding.get() {
                return px(0.0);
            }
            ed.text_rev.get();
            ed.st.borrow().line_height()
        }
    };
    let height = {
        let ed = ed.clone();
        move || rows_height(&ed)
    };
    let rows = visible_rows(ed.clone());
    let top = ed.rows_top;
    when(shown, move || {
        let (nw, fw) = (number_width.clone(), fold_width.clone());
        let (number_width, fold_width) = (number_width.clone(), fold_width.clone());
        let height = height.clone();
        let row_ed = ed.clone();
        part(
            view().flex_none().pl(px(12.0)).pr(px(4.0)),
            props,
            EditorPart::Gutter,
        )
        .child(
            view()
                .relative()
                .mt(move || Length::from(top.get()))
                .w(move || Length::from(nw() + fw()))
                .h(move || Length::from(height()))
                .child(each(
                    rows,
                    |row: &usize| *row,
                    move |row, _| {
                        gutter_row(
                            row_ed.clone(),
                            row.get_untracked(),
                            number_width.clone(),
                            fold_width.clone(),
                        )
                    },
                )),
        )
    })
}

/// The gutter of display row `row`.
fn gutter_row(
    ed: Ed,
    row: usize,
    number_width: impl Fn() -> Points + Clone + 'static,
    fold_width: impl Fn() -> Points + Clone + 'static,
) -> impl IntoElement {
    let props = ed.props;
    let info = {
        let ed = ed.clone();
        computed(move || {
            ed.text_rev.get();
            ed.sel_rev.get();
            ed.marks_rev.get();
            ed.st.borrow_mut().gutter_row(row)
        })
    };
    let lh = {
        let ed = ed.clone();
        move || {
            ed.text_rev.get();
            ed.st.borrow().line_height()
        }
    };
    let lh2 = lh.clone();
    let press_ed = ed.clone();
    let fold_ed = ed.clone();
    view()
        .absolute()
        .left_0()
        .right_0()
        .top(move || Length::from(lh() * row as f32))
        .h(move || Length::from(lh2()))
        .flex_row()
        .items_center()
        .child(when(
            move || props.line_numbers.get(),
            move || {
                let info = info;
                let number = move || info.get().number.map(|n| n.to_string()).unwrap_or_default();
                let ed = press_ed.clone();
                let number_width = number_width.clone();
                dynamic(move || {
                    let active = info.get().active;
                    let el = text(number)
                        .w(Length::from(number_width()))
                        .text_right()
                        .text_color(move || {
                            if info.get().active {
                                theme().foreground
                            } else {
                                theme().muted_foreground
                            }
                        })
                        .on_press({
                            let ed = ed.clone();
                            move || ed.line_number_press(row)
                        });
                    if active {
                        parts(
                            el,
                            props,
                            &[EditorPart::LineNumber, EditorPart::ActiveLineNumber],
                        )
                    } else {
                        part(el, props, EditorPart::LineNumber)
                    }
                })
            },
        ))
        .child(when(
            move || props.folding.get(),
            move || {
                let ed = fold_ed.clone();
                let width = fold_width.clone();
                dynamic(move || {
                    let fold = info.get().fold;
                    let ed = ed.clone();
                    let glyph = match fold {
                        Some(true) => "›",
                        Some(false) => "⌄",
                        None => "",
                    };
                    let el = view()
                        .w(Length::from(width()))
                        .items_center()
                        .justify_center()
                        .when(fold.is_some(), |v| {
                            v.on_press(move || {
                                let line = ed.st.borrow().display.row(row).line;
                                ed.fold(line, None);
                            })
                        })
                        .child(text(glyph).text_color(move || theme().muted_foreground));
                    part(el, props, EditorPart::FoldMarker)
                })
            },
        ))
}

/// The display rows to make, as a list `each` keeps rows of.
fn visible_rows(ed: Ed) -> Memo<Vec<usize>> {
    computed(move || {
        ed.text_rev.get();
        ed.visible_rows().collect()
    })
}

/// The height of the rows' box: every row, and a screen more past the end
/// when the text scrolls beyond its last line.
fn rows_height(ed: &Ed) -> Points {
    ed.text_rev.get();
    let (content, lh) = {
        let st = ed.st.borrow();
        (st.content_size().height, st.line_height())
    };
    // An editor growing with its text has no room past it.
    let grows = ed.props.min_rows.get().is_some() || ed.props.max_rows.get().is_some();
    let beyond = if grows {
        px(0.0)
    } else {
        let beyond = ed.props.scroll_beyond_last_line.get();
        beyond.room(ed.viewport.get().height, lh)
    };
    content.max(lh) + beyond
}

/// The height of an editor growing with its text (`minRows` /
/// `maxRows`): its rows within their bounds, and the room its parts' padding
/// takes around them; `None` when the app sizes it.
fn grow_height(ed: &Ed) -> Option<Points> {
    let (min, max) = (ed.props.min_rows.get(), ed.props.max_rows.get());
    if min.is_none() && max.is_none() {
        return None;
    }
    ed.text_rev.get();
    let (rows, lh) = {
        let st = ed.st.borrow();
        (st.display.row_count(), st.line_height())
    };
    let rows_h = super::layout::grow_height(rows, lh, min, max)?;
    // What the content holds besides the rows (the parts' padding), as
    // last laid out.
    let padding = (ed.content_height.get() - rows_height(ed)).max(px(0.0));
    Some(rows_h + padding)
}

/// The text area: the rows' box, in a horizontal scroll view unless lines
/// wrap.
fn text_area(ed: Ed, lh: impl Fn() -> Points + Clone + 'static) -> impl IntoElement {
    let props = ed.props;
    let wrap = move || props.soft_wrap.get();
    let a = ed.clone();
    let b = ed.clone();
    let c = lh.clone();
    when(wrap, move || {
        let ed = a.clone();
        part(view().flex_1().pl(px(4.0)), props, EditorPart::Content).child(rows_box(ed, c.clone()))
    })
    .otherwise(move || {
        let ed = b.clone();
        let scroll_ed = ed.clone();
        let layout_ed = ed.clone();
        scroll()
            .horizontal()
            .flex_1()
            .element_ref(&ed.hscroll)
            .on_scroll(move |e: &ScrollEvent| scroll_ed.scroll_x.set(e.offset.x))
            .on_layout(move |bounds: &Bounds<Points>| layout_ed.text_width.set(bounds.size.width))
            .child(
                part(
                    view().min_w(relative(1.0)).pl(px(4.0)),
                    props,
                    EditorPart::Content,
                )
                .child(rows_box(ed, lh.clone())),
            )
    })
}

/// The box of the rows and everything drawn over and under them.
fn rows_box(ed: Ed, lh: impl Fn() -> Points + Clone + 'static) -> impl IntoElement {
    let props = ed.props;
    let width = {
        let ed = ed.clone();
        move || {
            ed.text_rev.get();
            if props.soft_wrap.get() {
                return None;
            }
            let rows = ed.visible_rows();
            let st = ed.st.borrow();
            st.display.measure_rows(rows);
            Some(st.content_size().width)
        }
    };
    let height = {
        let ed = ed.clone();
        move || rows_height(&ed)
    };
    let desktop = is_desktop();
    let down = ed.clone();
    let tap = ed.clone();
    let mov = ed.clone();
    let up = ed.clone();
    let end = ed.clone();
    let menu = ed.clone();
    let layout = ed.clone();
    let (hover, hover_end) = (ed.clone(), ed.clone());
    let mut el = view()
        .element_ref(&ed.hooks.rows)
        .relative()
        .cursor(Cursor::Text)
        .h(move || Length::from(height()))
        .on_layout(move |b: &Bounds<Points>| {
            layout.rows_top.set(b.origin.y);
            layout.rows_left.set(b.origin.x);
            if props.soft_wrap.get_untracked() {
                layout.text_width.set(b.size.width);
            }
            layout.measure_place();
        })
        .on(EventKind::PressDown, move |p: &EventPayload| {
            if let (true, EventPayload::Press(e)) = (desktop, p) {
                down.press(e.position, e.page_position, e.modifiers, e.clicks);
                down.text_press(e.position, e.page_position, e.modifiers, e.clicks);
            }
        })
        .on(EventKind::Press, move |p: &EventPayload| {
            if desktop {
                return;
            }
            let EventPayload::Press(e) = p else { return };
            tap.press(e.position, e.page_position, Default::default(), 1);
            tap.text_press(e.position, e.page_position, Default::default(), 1);
        })
        .on_hover_move(move |e: &HoverEvent| hover.hover_move(e))
        .on_hover_out(move || hover_end.hover_out())
        // The mouse selects as it drags (a finger scrolls): the rows take
        // its touches, which the scroll views around them keep off.
        .claim_touches(TouchClaim::on_start().ask(move |request: &ClaimRequest| {
            if desktop {
                request.claim();
            }
        }))
        .keep_touches(desktop)
        .on_responder_move(move |e: &ResponderEvent| mov.drag_to(e.page))
        .on_responder_release(move |_: &ResponderEvent| up.release())
        .on_responder_terminate(move |_: &ResponderEvent| end.release())
        .on_context_menu(move |e: &ContextMenuEvent| {
            menu.context_menu(e.position, e.page_position)
        });
    el = el.w(move || match width() {
        Some(w) => Length::from(w),
        None => Length::Auto,
    });
    el.min_w(relative(1.0))
        .child(active_lines(ed.clone(), lh.clone()))
        .child(rulers(ed.clone()))
        .child(indent_guides(ed.clone(), lh.clone()))
        .child(match_rects(ed.clone()))
        .child(selection_rects(ed.clone()))
        .child(bracket_rects(ed.clone()))
        .child(rows(ed.clone(), lh.clone()))
        .child(whitespace_marks(ed.clone()))
        .child(diagnostic_marks(ed.clone()))
        .child(placeholder(ed.clone()))
        .child(carets(ed.clone(), lh.clone()))
        .child(composition(ed.clone(), lh.clone()))
        .child(sink(ed, lh))
}

/// A rectangle of the rows' box, absolutely placed.
fn rect(b: Bounds<Points>) -> View {
    view()
        .absolute()
        .left(Length::from(b.origin.x))
        .top(Length::from(b.origin.y))
        .w(Length::from(b.size.width))
        .h(Length::from(b.size.height))
        .pointer_events(PointerEvents::None)
}

/// The backgrounds of the active lines' rows.
fn active_lines(ed: Ed, lh: impl Fn() -> Points + Clone + 'static) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        if !props.active_line.get() {
            return Vec::new();
        }
        ed.text_rev.get();
        ed.sel_rev.get();
        let rows = ed.visible_rows();
        let active = ed.st.borrow().active_rows(rows);
        let lh = lh();
        active
            .into_iter()
            .map(|row| {
                part(
                    view()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(Length::from(lh * row as f32))
                        .h(Length::from(lh))
                        .pointer_events(PointerEvents::None)
                        .bg(move || theme().muted.opacity(0.6)),
                    props,
                    EditorPart::ActiveLine,
                )
            })
            .collect::<Vec<_>>()
    })
}

/// The indent guides of the rows in view.
fn indent_guides(ed: Ed, lh: impl Fn() -> Points + Clone + 'static) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        if !props.indent_guides.get() {
            return Vec::new();
        }
        ed.text_rev.get();
        let rows = ed.visible_rows();
        let guides = ed.st.borrow().indent_guides(rows);
        let lh = lh();
        guides
            .into_iter()
            .map(|(x, rows)| {
                part(
                    rect(Bounds::new(
                        Point::new(x, lh * rows.start as f32),
                        Size::new(px(1.0), lh * rows.len() as f32),
                    ))
                    .bg(move || theme().border),
                    props,
                    EditorPart::IndentGuide,
                )
            })
            .collect::<Vec<_>>()
    })
}

/// The search's matches in view.
fn match_rects(ed: Ed) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        ed.text_rev.get();
        ed.marks_rev.get();
        ed.sel_rev.get();
        let rows = ed.visible_rows();
        let rects = ed.st.borrow().match_rects(rows);
        rects
            .into_iter()
            .map(|(b, current)| {
                let el = rect(b).bg(move || MATCH.opacity(if current { 0.55 } else { 0.25 }));
                if current {
                    parts(
                        el,
                        props,
                        &[EditorPart::SearchMatch, EditorPart::CurrentSearchMatch],
                    )
                } else {
                    part(el, props, EditorPart::SearchMatch)
                }
            })
            .collect::<Vec<_>>()
    })
}

/// The selections in view.
fn selection_rects(ed: Ed) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        ed.text_rev.get();
        ed.sel_rev.get();
        let rows = ed.visible_rows();
        let rects = ed.st.borrow().selection_rects(rows);
        rects
            .into_iter()
            .map(|b| {
                part(
                    rect(b).bg(move || theme().primary.opacity(0.22)),
                    props,
                    EditorPart::Selection,
                )
            })
            .collect::<Vec<_>>()
    })
}

/// The bracket at the caret and its partner.
fn bracket_rects(ed: Ed) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        ed.text_rev.get();
        ed.sel_rev.get();
        if !ed.focused.get() {
            return Vec::new();
        }
        let rows = ed.visible_rows();
        let rects = ed.st.borrow().bracket_rects(rows);
        rects
            .into_iter()
            .map(|b| {
                part(
                    rect(b)
                        .border_1()
                        .border_color(move || theme().muted_foreground.opacity(0.6)),
                    props,
                    EditorPart::MatchingBracket,
                )
            })
            .collect::<Vec<_>>()
    })
}

/// The rows of text in view.
fn rows(ed: Ed, lh: impl Fn() -> Points + Clone + 'static) -> impl IntoElement {
    let list = visible_rows(ed.clone());
    each(
        list,
        |row: &usize| *row,
        move |row, _| row_view(ed.clone(), row.get_untracked(), lh.clone()),
    )
}

/// Display row `row`.
fn row_view(ed: Ed, row: usize, lh: impl Fn() -> Points + Clone + 'static) -> impl IntoElement {
    let props = ed.props;
    let content = {
        let ed = ed.clone();
        computed(move || {
            ed.text_rev.get();
            ed.hl_rev.get();
            ed.marks_rev.get();
            ed.st.borrow().row_content(row)
        })
    };
    let indent = {
        let ed = ed.clone();
        move || {
            ed.text_rev.get();
            let st = ed.st.borrow();
            if row < st.display.row_count() {
                st.display.row(row).indent
            } else {
                px(0.0)
            }
        }
    };
    let lh2 = lh.clone();
    let line_ed = ed.clone();
    let line = part(
        view()
            .absolute()
            .left_0()
            .right_0()
            .top(move || Length::from(lh() * row as f32))
            .h(move || Length::from(lh2()))
            .pointer_events(PointerEvents::None),
        props,
        EditorPart::Line,
    );
    // The line backgrounds of the decorations over the row.
    let line = line
        .app_class(move || {
            let st = line_ed.st.borrow();
            let classes: Vec<String> = content
                .with(|c| c.line_decorations.clone())
                .into_iter()
                .filter_map(|i| {
                    st.decoration_items
                        .get(i)
                        .and_then(|d| d.line_class.clone())
                })
                .map(|c| c.to_string())
                .collect();
            (!classes.is_empty()).then(|| SharedString::from(classes.join(" ")))
        })
        .inline_style({
            let ed = ed.clone();
            move || {
                let st = ed.st.borrow();
                let styles: Vec<Style> = content
                    .with(|c| c.line_decorations.clone())
                    .into_iter()
                    .filter_map(|i| {
                        st.decoration_items
                            .get(i)
                            .and_then(|d| d.line_style.clone())
                    })
                    .collect();
                Style::from_decls(styles.iter().flat_map(|s| s.decls().iter().cloned()))
            }
        });
    let span_ed = ed.clone();
    line.child(
        text("")
            .absolute()
            .top_0()
            .left(move || Length::from(indent()))
            .pointer_events(PointerEvents::Auto)
            .child(dynamic(move || spans(span_ed.clone(), row, content.get()))),
    )
}

/// The spans of a row's content.
fn spans(ed: Ed, row: usize, content: RowContent) -> Vec<AnyElement> {
    let props = ed.props;
    let mut out: Vec<AnyElement> = Vec::with_capacity(content.spans.len() + 1);
    {
        let st = ed.st.borrow();
        for span in content.spans {
            let mut el = text(span.text);
            if let Some(token) = span.token {
                *el.style() = token.style();
            }
            if !span.decorations.is_empty() {
                let classes: Vec<String> = span
                    .decorations
                    .iter()
                    .filter_map(|i| st.decoration_items.get(*i).and_then(|d| d.class.clone()))
                    .map(|c| c.to_string())
                    .collect();
                if !classes.is_empty() {
                    el = el.app_class(Some(SharedString::from(classes.join(" "))));
                }
                let styles: Vec<Style> = span
                    .decorations
                    .iter()
                    .filter_map(|i| st.decoration_items.get(*i).and_then(|d| d.style.clone()))
                    .collect();
                if !styles.is_empty() {
                    el = el.inline_style(Style::from_decls(
                        styles.iter().flat_map(|s| s.decls().iter().cloned()),
                    ));
                }
            }
            out.push(el.into_element());
        }
    }
    if content.folded {
        let ed = ed.clone();
        let placeholder = text(" ⋯ ")
            .text_color(move || theme().muted_foreground)
            .on_press(move || {
                let line = ed.st.borrow().display.row(row).line;
                ed.fold(line, Some(false));
            });
        out.push(part(placeholder, props, EditorPart::FoldPlaceholder).into_element());
    }
    out
}

/// The diagnostics' marks in view.
fn diagnostic_marks(ed: Ed) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        ed.text_rev.get();
        ed.marks_rev.get();
        let rows = ed.visible_rows();
        let marks = ed.st.borrow().diagnostic_rects(rows);
        marks
            .into_iter()
            .map(|(b, severity)| {
                let bar = Bounds::new(
                    Point::new(b.origin.x, b.origin.y + b.size.height - px(MARK_HEIGHT)),
                    Size::new(b.size.width, px(MARK_HEIGHT)),
                );
                let (color, which): (fn() -> AdaptiveColor, EditorPart) = match severity {
                    EditorDiagnosticSeverity::Error => {
                        (|| theme().destructive, EditorPart::DiagnosticError)
                    }
                    EditorDiagnosticSeverity::Warning => {
                        (|| WARNING, EditorPart::DiagnosticWarning)
                    }
                    EditorDiagnosticSeverity::Info => (|| INFO, EditorPart::DiagnosticInfo),
                    EditorDiagnosticSeverity::Hint => {
                        (|| theme().muted_foreground, EditorPart::DiagnosticHint)
                    }
                };
                part(rect(bar).bg(color), props, which)
            })
            .collect::<Vec<_>>()
    })
}

/// The placeholder, while the text is empty.
fn placeholder(ed: Ed) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        ed.text_rev.get();
        let empty = ed.st.borrow().buffer.is_empty();
        let text_ = props.placeholder.get()?;
        if !empty {
            return None;
        }
        Some(part(
            text(text_.to_string())
                .absolute()
                .top_0()
                .left_0()
                .pointer_events(PointerEvents::None)
                .text_color(move || theme().muted_foreground),
            props,
            EditorPart::Placeholder,
        ))
    })
}

/// The carets in view, blinking while the editor has the focus.
fn carets(ed: Ed, lh: impl Fn() -> Points + Clone + 'static) -> impl IntoElement {
    let props = ed.props;
    let blink = ed.blink;
    let focused = ed.focused;
    dynamic(move || {
        ed.text_rev.get();
        ed.sel_rev.get();
        let rows = ed.visible_rows();
        let (points, composing) = {
            let st = ed.st.borrow();
            (st.caret_points(rows), st.composition.is_some())
        };
        if composing {
            return Vec::new();
        }
        let lh = lh();
        points
            .into_iter()
            .map(|p| {
                part(
                    rect(Bounds::new(
                        Point::new(p.x - px(CARET_WIDTH / 2.0), p.y),
                        Size::new(px(CARET_WIDTH), lh),
                    ))
                    .bg(move || theme().foreground)
                    .opacity(move || {
                        if focused.get() && blink.get() {
                            1.0_f32
                        } else {
                            0.0
                        }
                    }),
                    props,
                    EditorPart::Caret,
                )
            })
            .collect::<Vec<_>>()
    })
}

/// What an input method is composing, drawn at the primary caret over the
/// text.
fn composition(ed: Ed, lh: impl Fn() -> Points + Clone + 'static) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        ed.sel_rev.get();
        ed.text_rev.get();
        let (text_, at) = {
            let st = ed.st.borrow();
            let text_ = st.composition.clone()?;
            (text_, st.caret_point(st.primary_head()))
        };
        Some(part(
            text(text_)
                .absolute()
                .left(Length::from(at.x))
                .top(Length::from(at.y))
                .h(Length::from(lh()))
                .pointer_events(PointerEvents::None)
                .bg(move || theme().background)
                .underline(),
            props,
            EditorPart::Composition,
        ))
    })
}

/// The native input that takes typed text and input methods, kept at the
/// primary caret (where the input method's candidates open), invisible.
fn sink(ed: Ed, lh: impl Fn() -> Points + Clone + 'static) -> impl IntoElement {
    let at = {
        let ed = ed.clone();
        computed(move || {
            ed.text_rev.get();
            ed.sel_rev.get();
            let st = ed.st.borrow();
            st.caret_point(st.primary_head())
        })
    };
    let props = ed.props;
    let (change, composition, key, paste, focus, blur) = (
        ed.clone(),
        ed.clone(),
        ed.clone(),
        ed.clone(),
        ed.clone(),
        ed.clone(),
    );
    let value = ed.sink_value;
    input()
        .element_ref(&ed.input)
        .absolute()
        .left(move || Length::from(at.get().x))
        .top(move || Length::from(at.get().y))
        .w(px(1.0))
        .h(move || Length::from(lh()))
        // Presses at the caret are the rows' (a click there places it).
        .pointer_events(PointerEvents::None)
        .p_0()
        .border(px(0.0))
        .bg(AdaptiveColor::TRANSPARENT)
        .text_color(AdaptiveColor::TRANSPARENT)
        .caret(false)
        .multiline(true)
        .auto_correct(false)
        .auto_capitalize(AutoCapitalize::None)
        .paste_behavior(PasteBehavior::Report)
        .value(move || value.get())
        .label(move || props.accessibility_label.get().unwrap_or_default())
        .on_change(move |text: &str| change.sink_changed(text))
        .on_composition_change(move |c: &Composition| composition.composition(c.composing, &c.text))
        .on_key_press(move |k: &str| key.sink_key(k))
        .on_paste(move |p: &PasteEvent| paste.paste(&p.text))
        .on_focus(move || focus.focus_changed(true))
        .on_blur(move || blur.focus_changed(false))
}

/// The lines of `rulers`, the rows' box's height.
fn rulers(ed: Ed) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        let columns = props.rulers.get();
        if columns.is_empty() {
            return Vec::new();
        }
        ed.text_rev.get();
        let xs = ed.st.borrow().ruler_xs(&columns);
        xs.into_iter()
            .map(|x| {
                part(
                    view()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(Length::from(x))
                        .w(px(1.0))
                        .pointer_events(PointerEvents::None)
                        .bg(move || theme().border),
                    props,
                    EditorPart::Ruler,
                )
            })
            .collect::<Vec<_>>()
    })
}

/// The marks of the spaces and tabs in view, while `showWhitespaces` is
/// set.
fn whitespace_marks(ed: Ed) -> impl IntoElement {
    let props = ed.props;
    dynamic(move || {
        if !props.show_whitespaces.get() {
            return Vec::new();
        }
        ed.text_rev.get();
        let rows = ed.visible_rows();
        let marks = ed.st.borrow().whitespace_marks(rows);
        marks
            .into_iter()
            .map(|(b, mark)| {
                part(
                    text(mark)
                        .absolute()
                        .left(Length::from(b.origin.x))
                        .top(Length::from(b.origin.y))
                        .w(Length::from(b.size.width))
                        .h(Length::from(b.size.height))
                        .number_of_lines(1)
                        .overflow_hidden()
                        .pointer_events(PointerEvents::None)
                        .text_color(move || theme().muted_foreground.opacity(0.6)),
                    props,
                    EditorPart::Whitespace,
                )
            })
            .collect::<Vec<_>>()
    })
}
