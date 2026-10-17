//! A small Markdown subset for replies (as the TSX gallery's
//! `demos/chat/markdown.tsx`): headings, paragraphs (bold, italic, inline
//! code), bullet and numbered lists, fenced code, tables and quotes.
//!
//! The blocks are keyed by position and kind, and each block's pieces of
//! text by position and look: while a reply streams, only the block being
//! written changes, and in it only the span a token lengthens. Every block
//! (and each list item and table cell) is one selectable text: a long press
//! highlights it and offers Copy.

use std::rc::Rc;

use nsh_core::ui::{AnyElement, prelude::*};

use super::{
    glyphs::{GlyphName, glyph},
    parts::caret,
};
use crate::ui::{Finish, Finished, MONO, PressProps, colors, press_with, radius};

/// What a block is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Kind {
    H,
    #[default]
    P,
    Ul,
    Ol,
    Code,
    Table,
    Quote,
}

/// A block of a reply.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Block {
    /// Its place and kind (`2:p`).
    pub(crate) key: SharedString,
    pub(crate) kind: Kind,
    /// A heading's level (1 to 3).
    pub(crate) level: u32,
    /// A heading's, paragraph's, quote's or code block's text.
    pub(crate) text: SharedString,
    /// A list's items.
    pub(crate) items: Vec<SharedString>,
    /// A code block's language.
    pub(crate) lang: SharedString,
    /// A table's header and rows.
    pub(crate) head: Vec<SharedString>,
    pub(crate) rows: Vec<Vec<SharedString>>,
}

fn cells(line: &str) -> Vec<SharedString> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    line.split('|')
        .map(|c| c.trim().to_owned().into())
        .collect()
}

/// A heading's level and text (`## Title`).
fn heading(line: &str) -> Option<(u32, &str)> {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if (1..=3).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
        Some((hashes as u32, &line[hashes + 1..]))
    } else {
        None
    }
}

fn bullet(line: &str) -> bool {
    line.starts_with("- ") || line.starts_with("* ")
}

/// The text of a numbered item (`1. text`).
fn numbered(line: &str) -> Option<&str> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0 && line[digits..].starts_with(". ")).then(|| &line[digits + 2..])
}

/// A line of a paragraph: none of the other blocks starts with it.
fn prose(line: &str) -> bool {
    !(line.trim().is_empty()
        || line.starts_with("```")
        || heading(line).is_some()
        || line.starts_with('>')
        || line.starts_with('|')
        || bullet(line)
        || numbered(line).is_some())
}

/// A line that cannot be told apart yet while it streams: a half-typed
/// table row, a bare list or heading marker, the start of a fence.
fn undecided(line: &str) -> bool {
    let hashes = line.bytes().all(|b| b == b'#') && (1..=3).contains(&line.len());
    let digits = line.trim_end_matches('.');
    let number = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && line.len() - digits.len() <= 1;
    line.starts_with('|')
        || hashes
        || line == "-"
        || line == "*"
        || number
        || line == "`"
        || line == "``"
}

/// Closes the emphasis a streaming text leaves open (`**bo` renders as
/// bold so far) and drops markers with nothing after them yet.
pub(crate) fn complete(text: &str) -> String {
    if text.matches('`').count() % 2 == 1 {
        return match text.strip_suffix('`') {
            Some(t) => t.to_owned(),
            None => format!("{text}`"),
        };
    }
    // Outside inline code.
    let mut outside = String::new();
    let mut inside = false;
    let mut pending = String::new();
    for c in text.chars() {
        if c == '`' {
            if inside {
                pending.clear();
            } else {
                outside.push_str(&pending);
                pending.clear();
            }
            inside = !inside;
        } else if inside {
            pending.push(c);
        } else {
            outside.push(c);
        }
    }
    let mut text = text.to_owned();
    let mut close = String::new();
    if outside.replace("**", "").matches('*').count() % 2 == 1 {
        if text.ends_with('*') && !text.ends_with("**") {
            text.pop();
        } else {
            close.push('*');
        }
    }
    if outside.matches("**").count() % 2 == 1 {
        if text.ends_with("**") {
            text.truncate(text.len() - 2);
        } else {
            close.push_str("**");
        }
    }
    text + &close
}

/// Parses `md`. `streaming` holds back a line that cannot be told apart
/// yet and completes the open emphasis of the last block.
pub(crate) fn parse(md: &str, streaming: bool) -> Vec<Block> {
    let mut lines: Vec<&str> = md.split('\n').collect();
    if streaming && lines.last().is_some_and(|l| undecided(l)) {
        lines.pop();
    }
    let mut out: Vec<Block> = Vec::new();
    let push = |out: &mut Vec<Block>, mut block: Block| {
        block.key = format!("{}:{}", out.len(), kind_name(block.kind)).into();
        out.push(block);
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some(lang) = line.strip_prefix("```") {
            let lang = lang.trim();
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].starts_with("```") {
                body.push(lines[i]);
                i += 1;
            }
            i += 1;
            push(
                &mut out,
                Block {
                    kind: Kind::Code,
                    lang: lang.to_owned().into(),
                    text: body.join("\n").into(),
                    ..Block::default()
                },
            );
            continue;
        }
        if let Some((level, text)) = heading(line) {
            i += 1;
            push(
                &mut out,
                Block {
                    kind: Kind::H,
                    level,
                    text: text.to_owned().into(),
                    ..Block::default()
                },
            );
            continue;
        }
        let mut run = |test: &dyn Fn(&str) -> bool| {
            let mut group = Vec::new();
            while i < lines.len() && test(lines[i]) {
                group.push(lines[i]);
                i += 1;
            }
            group
        };
        let block = if line.starts_with('>') {
            let group = run(&|l| l.starts_with('>'));
            let text: Vec<&str> = group
                .iter()
                .map(|l| {
                    let l = &l[1..];
                    l.strip_prefix(char::is_whitespace).unwrap_or(l)
                })
                .collect();
            Block {
                kind: Kind::Quote,
                text: text.join(" ").into(),
                ..Block::default()
            }
        } else if line.starts_with('|') {
            let group = run(&|l| l.starts_with('|'));
            let rule = |l: &str| l.len() > 1 && l[1..].chars().all(|c| " \t:|-".contains(c));
            let mut rows: Vec<Vec<SharedString>> = group
                .iter()
                .filter(|l| !rule(l))
                .map(|l| cells(l))
                .collect();
            let head = if rows.is_empty() {
                Vec::new()
            } else {
                rows.remove(0)
            };
            Block {
                kind: Kind::Table,
                head,
                rows,
                ..Block::default()
            }
        } else if bullet(line) {
            let group = run(&bullet);
            Block {
                kind: Kind::Ul,
                items: group.iter().map(|l| l[2..].to_owned().into()).collect(),
                ..Block::default()
            }
        } else if numbered(line).is_some() {
            let group = run(&|l| numbered(l).is_some());
            Block {
                kind: Kind::Ol,
                items: group
                    .iter()
                    .filter_map(|l| numbered(l))
                    .map(|t| t.to_owned().into())
                    .collect(),
                ..Block::default()
            }
        } else {
            let group = run(&prose);
            Block {
                kind: Kind::P,
                text: group.join(" ").into(),
                ..Block::default()
            }
        };
        push(&mut out, block);
    }
    if streaming && let Some(last) = out.last_mut() {
        match last.kind {
            Kind::P | Kind::H | Kind::Quote => last.text = complete(&last.text).into(),
            Kind::Ul | Kind::Ol => {
                if let Some(item) = last.items.last_mut() {
                    *item = complete(item).into();
                }
            }
            _ => {}
        }
    }
    out
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::H => "h",
        Kind::P => "p",
        Kind::Ul => "ul",
        Kind::Ol => "ol",
        Kind::Code => "code",
        Kind::Table => "table",
        Kind::Quote => "quote",
    }
}

/// How a piece of inline text looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Span {
    Plain,
    Bold,
    Italic,
    Code,
}

/// The length of a `**bold**`, `*italic*` or `` `code` `` at the start of
/// `s`, in bytes.
fn emphasis(s: &str) -> Option<(usize, Span)> {
    if let Some(rest) = s.strip_prefix("**") {
        let n = rest.find('*').unwrap_or(0);
        if n > 0 && rest[n..].starts_with("**") {
            return Some((n + 4, Span::Bold));
        }
    }
    if let Some(rest) = s.strip_prefix('*') {
        let first = rest.chars().next();
        if first.is_some_and(|c| c != '*' && !c.is_whitespace())
            && let Some(n) = rest.find('*')
        {
            return Some((n + 2, Span::Italic));
        }
    }
    if let Some(rest) = s.strip_prefix('`')
        && let Some(n) = rest.find('`')
        && n > 0
    {
        return Some((n + 2, Span::Code));
    }
    None
}

/// Bold, italic and inline code as pieces of `text`.
fn pieces(text: &str) -> Vec<(Span, &str)> {
    let mut out = Vec::new();
    let mut plain = 0;
    let mut at = 0;
    while at < text.len() {
        if let Some((len, span)) = emphasis(&text[at..]) {
            if plain < at {
                out.push((Span::Plain, &text[plain..at]));
            }
            let inner = match span {
                Span::Bold => &text[at + 2..at + len - 2],
                _ => &text[at + 1..at + len - 1],
            };
            out.push((span, inner));
            at += len;
            plain = at;
        } else {
            at += text[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    if plain < text.len() {
        out.push((Span::Plain, &text[plain..]));
    }
    out
}

/// A piece of a text's inline content: its place among the pieces, its
/// look and its text.
#[derive(Debug, Clone, PartialEq)]
struct Piece {
    at: usize,
    span: Span,
    text: String,
}

/// The text `source` gives with its emphasis as spans. Each piece is a
/// span of its own, known by its place and look: as a reply streams, a
/// token updates the one span it lengthens (or adds one).
pub(crate) fn inline(source: impl Fn() -> SharedString + 'static) -> Text {
    let pieces = move || {
        let text = source();
        pieces(&text)
            .into_iter()
            .enumerate()
            .map(|(at, (span, s))| Piece {
                at,
                span,
                text: s.to_owned(),
            })
            .collect::<Vec<_>>()
    };
    text_root().child(Each::new(
        pieces,
        |p: &Piece| (p.at, p.span),
        |piece, _| {
            let content = nsh_core::ui::text(move || piece.with(|p| p.text.clone()));
            match piece.with_untracked(|p| p.span) {
                Span::Plain => content,
                Span::Bold => content.font_weight(FontWeight::SEMIBOLD),
                Span::Italic => content.italic(),
                Span::Code => content
                    .font_family(MONO)
                    .text_size(px(14.0))
                    .text_color(colors::CODE),
            }
        },
    ))
}

fn text_root() -> Text {
    nsh_core::ui::text("")
}

/// A paragraph's look.
fn body(t: Text) -> Text {
    t.text_size(px(15.0))
        .line_height(px(22.0))
        .text_color(colors::TEXT)
}

/// Positions `0..len`: rows kept by place (an item being written keeps its
/// row).
fn places(len: usize) -> Vec<usize> {
    (0..len).collect()
}

/// What the blocks of a reply share: the reply's id, the copied code block
/// and what a code block's Copy button calls (its key and its text).
#[derive(Clone)]
struct Shared {
    id: Memo<String>,
    copied: Signal<Option<String>>,
    on_copy: Rc<dyn Fn(String, String)>,
}

/// One block of a reply: its kind is its key's, so it is made once; its
/// content follows `block`. `last` puts the caret at its end.
fn block_view(block: ReadSignal<Block>, last: Memo<bool>, shared: Shared) -> View {
    let paragraph = move |source: Box<dyn Fn() -> SharedString>| {
        text_root()
            .selectable(true)
            .child(inline(source))
            .child(If::new(last, caret))
    };
    let text_of = move || -> Box<dyn Fn() -> SharedString> {
        Box::new(move || block.with(|b| b.text.clone()))
    };
    match block.with_untracked(|b| b.kind) {
        Kind::H => {
            let level = computed(move || block.with(|b| b.level));
            view().child(
                paragraph(text_of())
                    .text_size(move || {
                        px(match level.get() {
                            1 => 20.0,
                            2 => 17.0,
                            _ => 15.0,
                        })
                    })
                    .line_height(move || px(if level.get() == 1 { 26.0 } else { 22.0 }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors::TEXT)
                    .pt_1(),
            )
        }
        Kind::P => view().child(body(paragraph(text_of()))),
        kind @ (Kind::Ul | Kind::Ol) => {
            let count = computed(move || block.with(|b| b.items.len()));
            view().gap(px(6.0)).child(Each::new(
                move || places(count.get()),
                |i: &usize| *i,
                move |i, _| {
                    let i = i.get_untracked();
                    let marker: AnyElement = if kind == Kind::Ol {
                        nsh_core::ui::text(format!("{}.", i + 1))
                            .w(px(16.0))
                            .text_size(px(15.0))
                            .line_height(px(22.0))
                            .text_color(colors::MUTED)
                            .tabular_nums()
                            .into_element()
                    } else {
                        view()
                            .w(px(16.0))
                            .h(px(22.0))
                            .items_center()
                            .justify_center()
                            .child(view().size(px(5.0)).rounded(px(2.5)).bg(colors::MUTED))
                            .into_element()
                    };
                    let at_end = computed(move || last.get() && i + 1 == count.get());
                    view().flex_row().gap(px(10.0)).child(marker).child(
                        body(
                            text_root()
                                .selectable(true)
                                .child(inline(move || {
                                    block.with(|b| b.items.get(i).cloned().unwrap_or_default())
                                }))
                                .child(If::new(at_end, caret)),
                        )
                        .flex_1(),
                    )
                },
            ))
        }
        Kind::Code => {
            let Shared {
                id,
                copied,
                on_copy,
            } = shared;
            let key = computed(move || format!("{}:{}", id.get(), block.with(|b| b.key.clone())));
            let done = computed(move || {
                let key = key.get();
                copied.with(|c| c.as_deref() == Some(key.as_str()))
            });
            let tone = move || {
                if done.get() {
                    colors::UP
                } else {
                    colors::MUTED
                }
            };
            let copy = move |_: &()| {
                on_copy(
                    key.get_untracked(),
                    block.with_untracked(|b| b.text.to_string()),
                );
            };
            view()
                .bg(colors::INSET)
                .finish(Finish::Inset)
                .rounded(px(radius::INSET))
                .border_curve(BorderCurve::Continuous)
                .border_1()
                .border_color(colors::LINE)
                .overflow_hidden()
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .h(px(40.0))
                        .pl_3()
                        .border_b_1()
                        .border_b_color(colors::LINE)
                        .child(
                            nsh_core::ui::text(move || {
                                block.with(|b| {
                                    if b.lang.is_empty() {
                                        SharedString::from("text")
                                    } else {
                                        b.lang.clone()
                                    }
                                })
                            })
                            .flex_1()
                            .text_size(px(12.0))
                            .text_color(colors::MUTED)
                            .font_family(MONO),
                        )
                        .child(
                            press_with(
                                PressProps {
                                    label: Some("Copy code".into()).into(),
                                    hit_slop: Some(4.0),
                                    ..PressProps::default()
                                },
                                copy,
                            )
                            .child(
                                view()
                                    .h(px(40.0))
                                    .px_3()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(glyph(
                                        Prop::bind(move || {
                                            if done.get() {
                                                GlyphName::Check
                                            } else {
                                                GlyphName::Copy
                                            }
                                        }),
                                        14.0,
                                        tone,
                                    ))
                                    .child(
                                        nsh_core::ui::text(move || {
                                            if done.get() { "Copied" } else { "Copy" }
                                        })
                                        .text_size(px(13.0))
                                        .text_color(tone),
                                    ),
                            ),
                        ),
                )
                .child(
                    scroll().horizontal().shows_indicator(false).child(
                        view().p_3().child(
                            nsh_core::ui::text(move || block.with(|b| b.text.clone()))
                                .selectable(true)
                                .font_family(MONO)
                                .text_size(px(13.0))
                                .line_height(px(19.0))
                                .text_color(colors::CODE)
                                .child(If::new(last, caret)),
                        ),
                    ),
                )
        }
        Kind::Table => {
            let head = computed(move || block.with(|b| b.head.clone()));
            let rows = computed(move || block.with(|b| b.rows.clone()));
            let cell = |t: Text| t.flex_1().px(px(10.0)).py_2();
            view()
                .rounded(px(radius::CONTROL))
                .border_1()
                .border_color(colors::BORDER)
                .overflow_hidden()
                .child(
                    view()
                        .flex_row()
                        .bg(colors::RAISED)
                        .finish(Finish::Raised)
                        .child(Each::new(
                            move || places(head.with(Vec::len)),
                            |i: &usize| *i,
                            move |i, _| {
                                let i = i.get_untracked();
                                cell(nsh_core::ui::text(move || {
                                    head.with(|h| h.get(i).cloned().unwrap_or_default())
                                }))
                                .text_size(px(13.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors::MUTED)
                            },
                        )),
                )
                .child(Each::new(
                    move || places(rows.with(Vec::len)),
                    |r: &usize| *r,
                    move |r, _| {
                        let r = r.get_untracked();
                        let row = computed(move || {
                            rows.with(|rs| rs.get(r).cloned().unwrap_or_default())
                        });
                        view()
                            .flex_row()
                            .border_t_1()
                            .border_t_color(colors::LINE)
                            .child(Each::new(
                                move || places(row.with(Vec::len)),
                                |c: &usize| *c,
                                move |c, _| {
                                    let c = c.get_untracked();
                                    cell(inline(move || {
                                        row.with(|r| r.get(c).cloned().unwrap_or_default())
                                    }))
                                    .selectable(true)
                                    .text_size(px(14.0))
                                    .text_color(colors::TEXT)
                                    .tabular_nums()
                                },
                            ))
                    },
                ))
        }
        Kind::Quote => view()
            .flex_row()
            .gap_3()
            .child(view().w(px(3.0)).rounded(px(1.5)).bg(colors::BORDER))
            .child(
                body(paragraph(text_of()))
                    .flex_1()
                    .text_color(colors::MUTED),
            ),
    }
}

/// The blocks of a reply (keyed by place and kind: while it streams, only
/// the block being written changes). `id` is the reply's; while `caret`
/// holds (streaming) the caret ends the last block; `copied` names the
/// copied code block, and a code block's Copy button calls `on_copy` with
/// its key and its text.
pub(crate) fn markdown(
    blocks: impl Fn() -> Vec<Block> + 'static,
    id: impl Fn() -> String + 'static,
    caret: impl Fn() -> bool + 'static,
    copied: Signal<Option<String>>,
    on_copy: Rc<dyn Fn(String, String)>,
) -> View {
    let blocks = computed(blocks);
    let caret = computed(caret);
    let count = computed(move || blocks.with(Vec::len));
    let shared = Shared {
        id: computed(id),
        copied,
        on_copy,
    };
    view().gap_3().child(Each::new(
        blocks,
        |b: &Block| b.key.clone(),
        move |block, index| {
            let last = computed(move || caret.get() && index.get() + 1 == count.get());
            block_view(block, last, shared.clone())
        },
    ))
}
