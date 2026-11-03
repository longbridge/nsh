//! `TextView`: a document of Markdown, HTML or the app's own blocks,
//! shown with the primitives (texts, views, images), styled by the app
//! part by part.

mod common;
pub mod document;
mod extension;
mod highlight;
mod html;
mod markdown;
#[cfg(feature = "math")]
mod math;
// Without the fonts formulas show as their TeX, as TeX RaTeX rejects does.
#[cfg(not(feature = "math"))]
mod math {
    pub(super) enum Formula {}

    pub(super) fn typeset(_tex: &str, _display: bool) -> Option<Formula> {
        None
    }

    pub(super) fn render(
        formula: &Formula,
        _size: f32,
        _display: bool,
    ) -> nsh_core::prelude::AnyElement {
        match *formula {}
    }
}
mod parts;
mod view;

pub use self::{
    document::{
        Alert, Block, CodeBlock, ColumnAlign, CustomNode, Document, Footnote, Image, ImageLength,
        Inline, ListItem, Table, TableCell,
    },
    extension::{
        MarkdownExtension, MarkdownOptions, MarkdownPlugin, MarkdownPlugins, ParseContext, Parser,
        mdast,
    },
    parts::{PartStyle, TextViewClasses, TextViewPart, TextViewParts, TextViewStyles},
    view::{
        CodeHighlighter, TextView, TextViewCodeAction, TextViewCodeActionEvent, TextViewElement,
        TextViewEvent, TextViewLinkPressEvent, TextViewLinks, TextViewMath, TextViewProps,
        text_view,
    },
};
