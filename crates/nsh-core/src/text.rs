use std::{
    any::Any,
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

use rustc_hash::FxHashMap;

use crate::{
    geometry::Size,
    ops::{EventMask, NodeId},
    style::{
        Color, DecorationStyle, EllipsizeMode, FontFamily, FontVariant, TextAlign, TextDecoration,
        TextShadow, TextStyle, TextTransform,
    },
};

/// Fully resolved character style of a text run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedTextStyle {
    /// Foreground color.
    pub color: Color,
    /// Font size in points.
    pub font_size: f32,
    /// Font weight, `100..=900`.
    pub font_weight: u16,
    /// Line height in points; `0.0` means the font's natural line height.
    pub line_height: f32,
    /// Font family; `None` is the system font.
    pub font_family: Option<FontFamily>,
    /// Italic.
    pub italic: bool,
    /// Extra space between characters in points.
    pub letter_spacing: f32,
    /// A line height set as a multiple of the font size (0: none), kept
    /// so nested text with another size gets its own line height.
    pub line_height_multiple: f32,
    /// Underline / strikethrough.
    pub decoration: TextDecoration,
    /// Color of the decoration lines; `None` uses `color`.
    pub decoration_color: Option<Color>,
    /// How the decoration lines are drawn.
    pub decoration_style: DecorationStyle,
    /// Thickness of the decoration lines in points; `None` is the font's.
    pub decoration_thickness: Option<f32>,
    /// Distance of the underline below the baseline; `None` is the font's.
    pub underline_offset: Option<f32>,
    /// Case transform (applied by the core when building the text).
    pub transform: TextTransform,
    /// OpenType features (`fontVariant`).
    pub font_variant: FontVariant,
}

impl Default for ResolvedTextStyle {
    /// React Native defaults.
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            font_size: 14.0,
            font_weight: 400,
            line_height: 0.0,
            font_family: None,
            italic: false,
            letter_spacing: 0.0,
            decoration: TextDecoration {
                underline: false,
                line_through: false,
            },
            decoration_color: None,
            decoration_style: DecorationStyle::Solid,
            decoration_thickness: None,
            underline_offset: None,
            line_height_multiple: 0.0,
            transform: TextTransform::None,
            font_variant: FontVariant(0),
        }
    }
}

impl ResolvedTextStyle {
    /// Applies the properties set on a node on top of the inherited style.
    #[inline]
    pub(crate) fn inherit(self, style: &TextStyle) -> Self {
        let font_size = style.font_size.unwrap_or(self.font_size);
        // A multiple follows each nested font size (unitless CSS
        // line-height); points stay points.
        let (line_height, line_height_multiple) =
            match (style.line_height_multiple, style.line_height) {
                (Some(m), _) => (m * font_size, m),
                (None, Some(points)) => (points, 0.0),
                (None, None) if self.line_height_multiple > 0.0 => (
                    self.line_height_multiple * font_size,
                    self.line_height_multiple,
                ),
                (None, None) => (self.line_height, 0.0),
            };
        Self {
            color: style.color.unwrap_or(self.color),
            font_size,
            font_weight: style.font_weight.unwrap_or(self.font_weight),
            line_height,
            line_height_multiple,
            font_family: style.font_family.or(self.font_family),
            italic: style.italic.unwrap_or(self.italic),
            // Ems are of the node's own font size, then inherited as points.
            letter_spacing: match style.letter_spacing_em {
                Some(em) => em * font_size,
                None => style.letter_spacing.unwrap_or(self.letter_spacing),
            },
            decoration: style.decoration.unwrap_or(self.decoration),
            decoration_color: style.decoration_color.or(self.decoration_color),
            decoration_style: style.decoration_style.unwrap_or(self.decoration_style),
            decoration_thickness: style.decoration_thickness.or(self.decoration_thickness),
            underline_offset: style.underline_offset.or(self.underline_offset),
            transform: style.transform.unwrap_or(self.transform),
            font_variant: style.font_variant.unwrap_or(self.font_variant),
        }
    }

    /// True if both styles produce the same glyph metrics (colors and
    /// decorations ignored).
    #[inline]
    fn layout_eq(&self, other: &Self) -> bool {
        self.font_size == other.font_size
            && self.font_weight == other.font_weight
            && self.line_height == other.line_height
            && self.font_family == other.font_family
            && self.italic == other.italic
            && self.letter_spacing == other.letter_spacing
            && self.font_variant == other.font_variant
    }
}

/// A run of characters with one style.
///
/// Offsets are in UTF-16 code units, which is what both Apple
/// (`NSAttributedString`) and Android (`Spannable`) text APIs index by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextSpan {
    /// Start offset (UTF-16 units, inclusive).
    pub start: u32,
    /// End offset (UTF-16 units, exclusive).
    pub end: u32,
    /// Style of the run.
    pub style: ResolvedTextStyle,
}

/// The merged content of a `<text>` block: its own text plus all nested
/// spans and raw text nodes, flattened into one string with style runs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AttributedText {
    /// The full string.
    pub text: String,
    /// Non-overlapping, sorted style runs covering `text`.
    pub spans: Vec<TextSpan>,
    /// Paragraph alignment.
    pub align: TextAlign,
    /// Maximum number of lines; 0 means no limit (`numberOfLines`).
    pub max_lines: u32,
    /// How text beyond `max_lines` is cut.
    pub ellipsize: EllipsizeMode,
    /// Lines never wrap (`whitespace-nowrap`): measured at their natural
    /// width, overflowing the box.
    pub no_wrap: bool,
    /// Shadow behind the whole block (drawing only, not layout).
    pub shadow: Option<TextShadow>,
    /// Nested `<text>` spans that take presses (they may nest).
    pub links: Vec<TextLink>,
    /// `writingDirection`: right to left (`Some(true)`), left to right, or
    /// from the content (`None`).
    pub writing_direction: Option<bool>,
    /// The block is laid out right to left: `auto` alignment is the right
    /// side.
    pub rtl: bool,
    /// A long press copies the text (`selectable` or `select-text`;
    /// drawing only): `Some(false)` when set off, `None` when not set, which
    /// is not selectable except where text is by default (desktop
    /// browsers). [`is_selectable`](Self::is_selectable) is whether it is asked for.
    pub selectable: Option<bool>,
    /// The highlight of the selection of selectable text (`selectionColor`);
    /// the platform's tint when `None`.
    pub selection_color: Option<Color>,
    /// Inline views and images, in text order: each is an object
    /// replacement character (U+FFFC) the engine gives the attachment's
    /// size, sitting on the baseline.
    pub attachments: Vec<TextAttachment>,
}

/// A view or image inside a text block (see
/// [`AttributedText::attachments`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextAttachment {
    /// Its placeholder character (UTF-16 offset).
    pub index: u32,
    /// Width in points.
    pub width: f32,
    /// Height in points (all above the baseline).
    pub height: f32,
}

/// The character standing for an attachment.
pub const ATTACHMENT_CHAR: char = '\u{FFFC}';

/// A nested `<text>` span with press listeners: presses on its characters
/// go to it rather than to the block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLink {
    /// Start offset (UTF-16 units, inclusive).
    pub start: u32,
    /// End offset (UTF-16 units, exclusive).
    pub end: u32,
    /// The span's node.
    pub node: NodeId,
    /// Its listeners.
    pub events: EventMask,
}

impl AttributedText {
    /// Whether the text was made selectable (`selectable`, `select-text`).
    pub fn is_selectable(&self) -> bool {
        self.selectable == Some(true)
    }

    /// Length of the text so far, in UTF-16 units.
    pub fn len_utf16(&self) -> u32 {
        self.spans.last().map_or(0, |s| s.end)
    }

    /// The innermost link containing the UTF-16 offset `index`.
    pub fn link_at(&self, index: u32) -> Option<&TextLink> {
        self.links
            .iter()
            .filter(|l| index >= l.start && index < l.end)
            .min_by_key(|l| l.end - l.start)
    }

    /// Appends an attachment of `width` x `height` points: its
    /// placeholder character in `style` (the line's font around it).
    pub fn push_attachment(&mut self, width: f32, height: f32, style: ResolvedTextStyle) {
        let index = self.len_utf16();
        self.push(ATTACHMENT_CHAR.encode_utf8(&mut [0; 3]), style);
        self.attachments.push(TextAttachment {
            index,
            width,
            height,
        });
    }

    /// Appends `s` with `style`, merging with the previous run when the
    /// style is identical.
    pub fn push(&mut self, s: &str, style: ResolvedTextStyle) {
        if s.is_empty() {
            return;
        }
        let s = &*style.transform.apply(s);
        let len = s.encode_utf16().count() as u32;
        self.text.push_str(s);
        match self.spans.last_mut() {
            Some(last) if last.style == style => last.end += len,
            last => {
                let start = last.map_or(0, |l| l.end);
                self.spans.push(TextSpan {
                    start,
                    end: start + len,
                    style,
                });
            }
        }
    }

    /// True if both texts lay out identically: same characters, runs,
    /// metrics and alignment. Decorative attributes (colors) are ignored.
    pub fn layout_eq(&self, other: &Self) -> bool {
        self.align == other.align
            && self.writing_direction == other.writing_direction
            && self.rtl == other.rtl
            && self.max_lines == other.max_lines
            && self.ellipsize == other.ellipsize
            && self.no_wrap == other.no_wrap
            && self.text == other.text
            && self.attachments == other.attachments
            && self.spans.len() == other.spans.len()
            && self
                .spans
                .iter()
                .zip(&other.spans)
                .all(|(a, b)| a.start == b.start && a.end == b.end && a.style.layout_eq(&b.style))
    }

    /// A 64-bit hash of everything [`AttributedText::layout_eq`] compares.
    /// SipHash (not Fx): it keys a cache without a full equality check.
    pub fn layout_hash(&self) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        self.text.hash(&mut h);
        (self.align as u8, self.writing_direction, self.rtl).hash(&mut h);
        (self.max_lines, self.ellipsize, self.no_wrap).hash(&mut h);
        for s in &self.spans {
            (s.start, s.end, s.style.font_weight).hash(&mut h);
            (s.style.font_family, s.style.italic, s.style.font_variant).hash(&mut h);
            s.style.font_size.to_bits().hash(&mut h);
            s.style.line_height.to_bits().hash(&mut h);
            s.style.letter_spacing.to_bits().hash(&mut h);
        }
        for a in &self.attachments {
            (a.index, a.width.to_bits(), a.height.to_bits()).hash(&mut h);
        }
        h.finish()
    }
}

/// An immutable, fully laid out text produced by a [`TextEngine`] on the
/// engine thread and drawn as-is by the platform on the UI thread, so text
/// is typeset exactly once.
#[derive(Clone)]
pub struct TextLayout(pub Arc<dyn Any + Send + Sync>);

impl TextLayout {
    /// Returns the platform layout object.
    pub fn downcast<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

impl fmt::Debug for TextLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TextLayout")
    }
}

/// The platform text engine (CoreText, Android `StaticLayout`, ...).
///
/// All methods run on the engine thread, so implementations must not touch
/// UI-thread-only objects. The split mirrors how native text stacks work:
/// typesetting state is prepared once per content, measured at any number
/// of widths during layout, and finally laid out once at the final size.
pub trait TextEngine {
    /// Typesetting state for one content, reused across measurements
    /// (e.g. a `CTFramesetter`).
    type Prepared;

    /// Prepares `text` for measuring and layout.
    fn prepare(&self, text: &AttributedText) -> Self::Prepared;

    /// Size of the prepared text with an optional maximum width.
    fn measure(&self, prepared: &Self::Prepared, max_width: Option<f32>) -> Size;

    /// The final layout in a box of exactly `size`, for drawing.
    fn layout(&self, prepared: &Self::Prepared, size: Size) -> TextLayout;

    /// The lines of a final layout (`onTextLayout`); `text` is left empty
    /// (the core fills it in).
    fn lines(&self, layout: &TextLayout) -> Vec<TextLine> {
        let _ = layout;
        Vec::new()
    }

    /// Where a final layout put the text's attachments, in order: the
    /// top left `(x, y)` of each placeholder in the content box (y down,
    /// points), or `None` when it was cut off (lines past the limit, an
    /// ellipsis).
    fn attachments(&self, layout: &TextLayout) -> Vec<Option<(f32, f32)>> {
        let _ = layout;
        Vec::new()
    }

    /// Makes the font in `data` (TrueType / OpenType) available by its
    /// family name. Fonts are registered before the text using them is
    /// measured.
    fn register_font(&self, data: &[u8]) -> Result<(), String> {
        let _ = data;
        Err("this text engine does not support custom fonts".to_owned())
    }
}

/// A shared engine, e.g. one the core lays text out with and canvas
/// `measureText` measures with.
impl<T: TextEngine> TextEngine for std::rc::Rc<T> {
    type Prepared = T::Prepared;

    fn prepare(&self, text: &AttributedText) -> Self::Prepared {
        (**self).prepare(text)
    }

    fn measure(&self, prepared: &Self::Prepared, max_width: Option<f32>) -> Size {
        (**self).measure(prepared, max_width)
    }

    fn layout(&self, prepared: &Self::Prepared, size: Size) -> TextLayout {
        (**self).layout(prepared, size)
    }

    fn lines(&self, layout: &TextLayout) -> Vec<TextLine> {
        (**self).lines(layout)
    }

    fn attachments(&self, layout: &TextLayout) -> Vec<Option<(f32, f32)>> {
        (**self).attachments(layout)
    }

    fn register_font(&self, data: &[u8]) -> Result<(), String> {
        (**self).register_font(data)
    }
}

/// One line of laid-out text (`onTextLayout`), in the content box (y
/// down, points).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextLine {
    /// Its characters.
    pub text: String,
    /// Start offset in the text (UTF-16 units, inclusive).
    pub start: u32,
    /// End offset (UTF-16 units, exclusive).
    pub end: u32,
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Typographic width.
    pub width: f32,
    /// Height (ascender + descender + leading).
    pub height: f32,
    /// Above the baseline.
    pub ascender: f32,
    /// Below the baseline.
    pub descender: f32,
    /// Height of capitals.
    pub cap_height: f32,
    /// Height of lowercase letters.
    pub x_height: f32,
}

/// A deterministic, platform-free [`TextEngine`] for headless runs and
/// tests: every character is `0.55 × font size` wide and lines are
/// `1.2 × font size` tall.
#[derive(Debug, Default, Clone, Copy)]
pub struct ApproxTextEngine;

impl TextEngine for ApproxTextEngine {
    /// `(character count, font size, max lines, attachments)`.
    type Prepared = (usize, f32, u32, Vec<TextAttachment>);

    fn prepare(&self, text: &AttributedText) -> Self::Prepared {
        let size = text.spans.first().map_or(14.0, |s| s.style.font_size);
        (
            text.text.chars().count(),
            size,
            text.max_lines,
            text.attachments.clone(),
        )
    }

    fn measure(
        &self,
        &(chars, size, max_lines, _): &Self::Prepared,
        max_width: Option<f32>,
    ) -> Size {
        let width = chars as f32 * size * 0.55;
        let line = size * 1.2;
        let (width, mut lines) = match max_width {
            Some(max) if width > max && max > 0.0 => (max, (width / max).ceil()),
            _ => (width, 1.0),
        };
        if max_lines > 0 {
            lines = lines.min(max_lines as f32);
        }
        Size::new(width, lines * line)
    }

    fn layout(&self, prepared: &Self::Prepared, size: Size) -> TextLayout {
        // On one line, each character as wide as the others.
        let (_, font, _, attachments) = prepared;
        let line = font * 1.2;
        let points: Vec<Option<(f32, f32)>> = attachments
            .iter()
            .map(|a| Some((a.index as f32 * font * 0.55, (line - a.height).max(0.0))))
            .collect();
        TextLayout(Arc::new((size, points)))
    }

    fn attachments(&self, layout: &TextLayout) -> Vec<Option<(f32, f32)>> {
        layout
            .downcast::<(Size, Vec<Option<(f32, f32)>>)>()
            .map_or_else(Vec::new, |(_, p)| p.clone())
    }

    fn register_font(&self, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

/// Content-addressed text measurements shared by all nodes of a surface,
/// so identical texts (list rows, recycled cells) are measured once.
///
/// Approximate LRU with two generations: lookups hit the current one or
/// promote from the previous one; when the current generation is full it
/// becomes the previous one. O(1), and never more than `2 * GENERATION`
/// entries.
#[derive(Default)]
pub(crate) struct MeasureCache {
    current: FxHashMap<(u64, u32), Size>,
    previous: FxHashMap<(u64, u32), Size>,
}

impl MeasureCache {
    const GENERATION: usize = 512;

    #[inline]
    fn key(layout_hash: u64, max_width: Option<f32>) -> (u64, u32) {
        // NaN never comes from taffy, so it can stand for "unconstrained".
        (layout_hash, max_width.unwrap_or(f32::NAN).to_bits())
    }

    pub(crate) fn get(&mut self, layout_hash: u64, max_width: Option<f32>) -> Option<Size> {
        let key = Self::key(layout_hash, max_width);
        if let Some(&size) = self.current.get(&key) {
            return Some(size);
        }
        let size = self.previous.remove(&key)?;
        self.insert_key(key, size);
        Some(size)
    }

    pub(crate) fn insert(&mut self, layout_hash: u64, max_width: Option<f32>, size: Size) {
        self.insert_key(Self::key(layout_hash, max_width), size);
    }

    fn insert_key(&mut self, key: (u64, u32), size: Size) {
        if self.current.len() >= Self::GENERATION {
            self.previous = std::mem::take(&mut self.current);
            self.current.reserve(Self::GENERATION);
        }
        self.current.insert(key, size);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.current.len() + self.previous.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_merges_and_counts_utf16() {
        let mut t = AttributedText::default();
        let a = ResolvedTextStyle::default();
        let b = ResolvedTextStyle {
            font_weight: 700,
            ..a
        };
        t.push("ab", a);
        t.push("", b);
        t.push("😀", a);
        t.push("c", b);
        assert_eq!(t.text, "ab😀c");
        assert_eq!(t.spans.len(), 2);
        assert_eq!((t.spans[0].start, t.spans[0].end), (0, 4));
        assert_eq!((t.spans[1].start, t.spans[1].end), (4, 5));
    }

    #[test]
    fn color_does_not_affect_layout_identity() {
        let mut a = AttributedText::default();
        a.push("hello", ResolvedTextStyle::default());
        let mut b = AttributedText::default();
        b.push(
            "hello",
            ResolvedTextStyle {
                color: Color(0xff00_00ff),
                ..Default::default()
            },
        );
        assert_ne!(a, b);
        assert!(a.layout_eq(&b));
        assert_eq!(a.layout_hash(), b.layout_hash());
        let mut c = b.clone();
        c.spans[0].style.font_size = 15.0;
        assert!(!a.layout_eq(&c));
        assert_ne!(a.layout_hash(), c.layout_hash());
    }

    #[test]
    fn measure_cache_is_bounded_and_promotes() {
        let mut cache = MeasureCache::default();
        cache.insert(1, Some(100.0), Size::new(1.0, 1.0));
        for i in 0..10_000u64 {
            cache.insert(1000 + i, None, Size::default());
            // Keep entry 1 hot: every lookup promotes it.
            assert!(cache.get(1, Some(100.0)).is_some());
            assert!(cache.len() <= 2 * MeasureCache::GENERATION);
        }
        assert!(cache.get(1, Some(99.0)).is_none());
        assert!(cache.get(1000, None).is_none(), "cold entries are evicted");
    }
}
