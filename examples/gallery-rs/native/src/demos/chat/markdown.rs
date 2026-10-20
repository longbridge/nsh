//! A reply's Markdown, shown by `nsh/base`'s `TextView` at a chat's scale
//! (as the TSX gallery's `demos/chat/markdown.tsx`), code blocks with a
//! Copy button.
//!
//! While a reply streams, the view reads it as streaming Markdown (open
//! emphasis shows as closed) and makes again only the block being written;
//! the caret ends its last block. Every block is selectable: a long press
//! highlights it and offers Copy.

use std::rc::Rc;

use nsh_base::{
    MarkdownOptions, TextView, TextViewPart, TextViewParts, TextViewProps, text_view::CodeBlock,
};
use nsh_core::ui::{Renderer, StyleRefinement, prelude::*};

use super::{
    glyphs::{GlyphName, glyph},
    parts::caret,
};
use crate::ui::{PressProps, colors, press_with};

/// The parts at a chat's scale (tighter headings than a document's) and
/// in the gallery's code and link colors; the rest is `TextView`'s default
/// typography in the theme's colors. Text styles the reply's root sets (15
/// pt on 22, the text color) pass to every part.
fn parts() -> TextViewParts {
    use TextViewPart as P;
    let heading = |size: f32, line: f32| {
        move |s: StyleRefinement| s.text_size(px(size)).line_height(px(line))
    };
    TextViewParts::new()
        .part(P::Heading1, heading(20.0, 26.0))
        .part(P::Heading2, heading(17.0, 22.0))
        .part(P::Heading3, heading(15.0, 22.0))
        .part(P::Code, |s| s.text_color(colors::CODE))
        .part(P::CodeBlockCode, |s| s.text_color(colors::CODE))
}

/// A code block's Copy button: Copied (with a check) while `copied` names
/// it. `key` is the reply's id and the code.
fn copy_button(
    code: &CodeBlock,
    id: Memo<String>,
    copied: Signal<Option<String>>,
    on_copy: Rc<dyn Fn(String, String)>,
) -> AnyElement {
    let code = code.code.clone();
    let key = format!("{}:{code}", id.get_untracked());
    let done = {
        let key = key.clone();
        computed(move || copied.with(|c| c.as_deref() == Some(key.as_str())))
    };
    let tone = move || {
        if done.get() {
            colors::UP
        } else {
            colors::MUTED
        }
    };
    press_with(
        PressProps {
            label: Some("Copy code".into()).into(),
            hit_slop: Some(4.0),
            ..PressProps::default()
        },
        move |_: &()| on_copy(key.clone(), code.clone()),
    )
    .child(
        view()
            .h(px(16.0))
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .child(glyph(
                Prop::bind(move || {
                    if done.get() {
                        GlyphName::Check
                    } else {
                        GlyphName::Copy
                    }
                }),
                12.0,
                tone,
            ))
            .child(
                text(move || if done.get() { "Copied" } else { "Copy" })
                    .text_size(px(11.0))
                    .text_color(tone),
            ),
    )
    .into_element()
}

/// The reply `md` gives. `id` is the reply's; while `streaming` holds the
/// Markdown is read as still arriving and the caret ends it; `copied`
/// names the copied code block, and a code block's Copy button calls
/// `on_copy` with its key and its code.
pub(crate) fn markdown(
    md: impl Fn() -> String + 'static,
    id: impl Fn() -> String + 'static,
    streaming: impl Fn() -> bool + 'static,
    copied: Signal<Option<String>>,
    on_copy: Rc<dyn Fn(String, String)>,
) -> impl IntoElement {
    provide_context(parts());
    let id = computed(id);
    let streaming = computed(streaming);
    let actions =
        Renderer::new(move |code: &CodeBlock| copy_button(code, id, copied, on_copy.clone()));
    TextView::element(TextViewProps {
        markdown: Prop::bind(move || Some(SharedString::from(md()))),
        options: Prop::bind(move || MarkdownOptions {
            streaming: streaming.get(),
            ..MarkdownOptions::default()
        }),
        selectable: true.into(),
        code_block_scroll_indicator: false.into(),
        render_code_block_actions: Some(actions).into(),
        ..TextViewProps::default()
    })
    .fill("caret", If::new(streaming, caret))
    .gap_3()
    .text_size(px(15.0))
    .line_height(px(22.0))
    .text_color(colors::TEXT)
}
