//! Engine-thread typography. Completed native drawing commands and selection geometry
//! cross to the UI thread. Engine snapshots retain paragraphs for on-demand line
//! metrics; mount snapshots own only commands and interaction geometry.

use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use nsh_core::{AttributedText, Color, Point, Points, Size, TextAlign, TextEngine, TextLayout};
use ohos_drawing_binding::{
    Canvas, FontCollection, FontSlant, FontStyle, FontWeight, FontWidth, TextDirection, TextStyle,
    Typography, TypographyBuilder, TypographyStyle,
};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn argb(color: Color) -> u32 {
    color.0.rotate_right(8)
}

#[derive(Clone, Copy)]
pub(crate) struct RawLine {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) x: Points,
    pub(crate) y: Points,
    pub(crate) width: Points,
    pub(crate) height: Points,
    pub(crate) ascender: Points,
    pub(crate) descender: Points,
    pub(crate) cap_height: Points,
    pub(crate) x_height: Points,
}

/// Engine-only layout ownership. The sink replaces this with `native` before
/// dispatching a batch, so paragraph destruction stays on the engine thread.
pub(crate) struct EngineText {
    pub(crate) native: Arc<NativeText>,
    paragraph: Arc<Mutex<Paragraph>>,
    lines: OnceLock<Vec<RawLine>>,
}

pub(crate) struct NativeText {
    pub(crate) origin_y: f32,
    pub(crate) drawing: Mutex<Option<TextDrawing>>,
    pub(crate) scale: f32,
    pub(crate) text: Arc<str>,
    pub(crate) links: Vec<(nsh_core::TextLink, Vec<[f32; 4]>)>,
    pub(crate) attachments: Vec<Option<Point<Points>>>,
    pub(crate) selection: Option<crate::text::selection::layout::Layout>,
    pub(crate) selection_color: Option<Color>,
    _budget: Option<Reservation>,
}

/// Native commands own their drawing resources. The mutable paragraph stays on the
/// engine thread; only this completed recording moves to the UI thread.
pub(crate) struct TextDrawing {
    commands: ohos_drawing_binding::RecordCmd,
    _fonts: Vec<Arc<[u8]>>,
}

// SAFETY: a completed Rosen DrawCmdList owns the recorded paths, brushes and
// retained text blobs. No mutable paragraph or font collection is borrowed.
// The owner is transferred once through a Mutex, never shared concurrently;
// registered font buffers are kept alive until the commands are destroyed.
unsafe impl Send for TextDrawing {}

impl TextDrawing {
    pub(crate) fn draw(&mut self, canvas: &Canvas) -> ohos_drawing_binding::Result<()> {
        self.commands.draw_nested(canvas)
    }
}

const SELECTION_BUDGET: usize = 256 << 20;
pub(crate) struct Reservation {
    used: Arc<AtomicUsize>,
    bytes: usize,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.used.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}
/// A paragraph snapshot is queried under a lock and never laid out again while
/// an engine layout owns it. Drawing crosses threads as recorded native commands.
struct Paragraph {
    native: Typography,
    width: Option<f64>,
    _fonts: Vec<Arc<[u8]>>,
}
// SAFETY: Typography owns its native paragraph and retained font providers. All
// native access is serialized by its Mutex; only the engine measures or queries
// it. EngineText retains this owner on the engine thread; the mount snapshot has
// no paragraph reference. Font buffers outlive the paragraph.
unsafe impl Send for Paragraph {}

impl Paragraph {
    fn fits(&self, width: f64, single_line_ltr: bool) -> bool {
        self.width == Some(width)
            || (single_line_ltr
                && self
                    .width
                    .is_some_and(|old| self.native.max_intrinsic_width().ceil() <= old.min(width))
                && self.native.line_count() <= 1)
    }
    fn lines(&self) -> Vec<RawLine> {
        (0..self.native.line_count().min(i32::MAX as usize))
            .filter_map(|index| self.native.line_metrics(index as i32))
            .map(|line| RawLine {
                start: line.start_index as u32,
                end: line.end_index as u32,
                x: Points(line.x as f32),
                y: Points(line.y as f32),
                width: Points(line.width as f32),
                height: Points(line.height as f32),
                ascender: Points(line.ascender as f32),
                descender: Points(line.descender as f32),
                cap_height: Points(line.cap_height as f32),
                x_height: Points(line.x_height as f32),
            })
            .collect()
    }
}

impl EngineText {
    fn lines(&self) -> &[RawLine] {
        self.lines
            .get_or_init(|| self.paragraph.lock().expect("text paragraph").lines())
    }
}

pub(crate) struct Prepared {
    paragraph: RefCell<Arc<Mutex<Paragraph>>>,
    source: AttributedText,
    single_line_ltr: bool,
    has_attachments: bool,
    text: Arc<str>,
    no_wrap: bool,
    links: Vec<nsh_core::TextLink>,
    backgrounds: Vec<(u32, u32, nsh_core::TextBackground)>,
    decorations: Vec<crate::text::decoration::Decoration>,
    selectable: Option<Arc<str>>,
    selection_color: Option<Color>,
}

impl Prepared {
    fn layout(
        &self,
        engine: &OhosTextEngine,
        width: f64,
        measuring: bool,
    ) -> Arc<Mutex<Paragraph>> {
        // Measuring needs only dimensions: a complete line has the same size
        // for every alignment while it fits. Only final layout needs to move
        // centered/right-aligned glyphs to a new width (as StaticLayout does).
        let mut paragraph = self.paragraph.borrow_mut();
        if !paragraph
            .lock()
            .expect("text paragraph")
            .fits(width, measuring || self.single_line_ltr)
        {
            // Mounted layouts retain their geometry, as CTFrame/StaticLayout do.
            // Rebuild only when a later constraint really changes that geometry.
            if Arc::strong_count(&paragraph) > 1 {
                *paragraph = engine.prepare(&self.source).paragraph.into_inner();
            }
            let mut owned = paragraph.lock().expect("text paragraph");
            owned.native.layout(width);
            owned.width = Some(width);
        }
        paragraph.clone()
    }
}

pub(crate) struct OhosTextEngine {
    fonts: RefCell<FontCollection>,
    scale: f32,
    font_data: RefCell<Vec<Arc<[u8]>>>,
    budget: Arc<AtomicUsize>,
}

impl OhosTextEngine {
    pub(crate) fn new(scale: f32) -> Self {
        Self {
            // The legacy collection supports only one paragraph builder.
            // A shared collection is reusable within this engine thread.
            fonts: RefCell::new(FontCollection::shared()),
            scale,
            font_data: RefCell::default(),
            budget: Arc::default(),
        }
    }
}

impl TextEngine for OhosTextEngine {
    type Prepared = Prepared;

    fn lines(&self, layout: &TextLayout) -> Vec<nsh_core::TextLine> {
        let Some(raster) = layout.0.downcast_ref::<EngineText>() else {
            return Vec::new();
        };
        if raster.lines().is_empty() {
            return Vec::new();
        }
        if raster.native.text.is_ascii() {
            let bytes = raster.native.text.as_bytes();
            return raster
                .lines()
                .iter()
                .map(|line| {
                    let start = (line.start as usize).min(bytes.len());
                    let end = (line.end as usize).min(bytes.len()).max(start);
                    nsh_core::TextLine {
                        text: raster.native.text[start..end].to_owned(),
                        start: line.start,
                        end: line.end,
                        x: line.x,
                        y: line.y,
                        width: line.width,
                        height: line.height,
                        ascender: line.ascender,
                        descender: line.descender,
                        cap_height: line.cap_height,
                        x_height: line.x_height,
                    }
                })
                .collect();
        }
        let utf16: Vec<u16> = raster.native.text.encode_utf16().collect();
        raster
            .lines()
            .iter()
            .map(|line| {
                let start = (line.start as usize).min(utf16.len());
                let end = (line.end as usize).min(utf16.len()).max(start);
                nsh_core::TextLine {
                    text: String::from_utf16_lossy(&utf16[start..end]),
                    start: line.start,
                    end: line.end,
                    x: line.x,
                    y: line.y,
                    width: line.width,
                    height: line.height,
                    ascender: line.ascender,
                    descender: line.descender,
                    cap_height: line.cap_height,
                    x_height: line.x_height,
                }
            })
            .collect()
    }
    fn attachments(&self, layout: &TextLayout) -> Vec<Option<Point<Points>>> {
        layout
            .0
            .downcast_ref::<EngineText>()
            .map_or_else(Vec::new, |raster| raster.native.attachments.clone())
    }
    fn register_font(&self, data: &[u8]) -> Result<(), String> {
        let face = ttf_parser::Face::parse(data, 0).map_err(|e| e.to_string())?;
        let family = face
            .names()
            .into_iter()
            .filter(|name| {
                matches!(
                    name.name_id,
                    ttf_parser::name_id::TYPOGRAPHIC_FAMILY | ttf_parser::name_id::FAMILY
                )
            })
            .find_map(|name| name.to_string())
            .filter(|name| !name.is_empty() && !name.contains('\0'))
            .ok_or("font has no valid family name")?;
        let mut bytes: Arc<[u8]> = Arc::from(data);
        self.fonts
            .borrow_mut()
            .register_font_buffer(&family, Arc::get_mut(&mut bytes).expect("new font buffer"))
            .map_err(|e| e.to_string())?;
        // Retain the buffer for the font collection's lifetime, even on SDKs
        // where registration copies it internally.
        self.font_data.borrow_mut().push(bytes);
        Ok(())
    }

    fn prepare(&self, text: &AttributedText) -> Prepared {
        let rtl = text.writing_direction.unwrap_or(text.rtl);
        let mut style = TypographyStyle::new();
        style.set_text_align(match text.align {
            TextAlign::Auto => ohos_drawing_binding::TextAlign::Start,
            TextAlign::Left => ohos_drawing_binding::TextAlign::Left,
            TextAlign::Center => ohos_drawing_binding::TextAlign::Center,
            TextAlign::Right => ohos_drawing_binding::TextAlign::Right,
            TextAlign::Justify => ohos_drawing_binding::TextAlign::Justify,
        });
        style.set_text_direction(if rtl {
            TextDirection::Rtl
        } else {
            TextDirection::Ltr
        });
        if text.max_lines > 0 {
            style.set_max_lines(text.max_lines.min(i32::MAX as u32) as i32);
            use nsh_core::EllipsizeMode as E;
            use ohos_drawing_binding::EllipsisMode as N;
            if let Err(error) = style.set_ellipsis(
                if text.ellipsize == E::Clip { "" } else { "…" },
                match text.ellipsize {
                    E::Head => N::Head,
                    E::Middle => N::Middle,
                    _ => N::Tail,
                },
            ) {
                log::warn!("OHOS text ellipsis: {error}");
            }
        }
        let mut builder = TypographyBuilder::new(&mut style, &mut self.fonts.borrow_mut());
        // Core spans use UTF-16 offsets, including supplementary characters.
        let single_plain_span = text.spans.len() == 1 && text.attachments.is_empty();
        let is_ascii = text.text.is_ascii();
        let utf16: Vec<u16> = if single_plain_span || is_ascii {
            Vec::new()
        } else {
            text.text.encode_utf16().collect()
        };
        let add_slice = |builder: &mut TypographyBuilder, start: usize, end: usize| {
            if single_plain_span {
                builder.add_text(&text.text);
            } else if is_ascii {
                let len = text.text.len();
                let s = start.min(len);
                let e = end.min(len).max(s);
                builder.add_text(&text.text[s..e]);
            } else {
                let len = utf16.len();
                let s = start.min(len);
                let e = end.min(len).max(s);
                builder.add_text(&String::from_utf16_lossy(&utf16[s..e]));
            }
        };
        let mut backgrounds = Vec::new();
        let mut decorations = Vec::new();
        for span in &text.spans {
            if let Some(bg) = span.style.background
                && span.start < span.end
            {
                backgrounds.push((span.start, span.end, bg));
            }
            let mut style = TextStyle::new();
            let custom_decoration = ((span.style.decoration_thickness.is_some()
                || span.style.underline_offset.is_some())
                && (span.style.decoration.underline || span.style.decoration.line_through))
                || (span.style.decoration.underline && span.style.decoration.line_through);
            style.set_color(argb(span.style.color));
            let mut decoration = ohos_drawing_binding::TextDecoration::NONE;
            if span.style.decoration.underline && !custom_decoration {
                decoration |= ohos_drawing_binding::TextDecoration::UNDERLINE;
            }
            if span.style.decoration.line_through && !custom_decoration {
                decoration |= ohos_drawing_binding::TextDecoration::LINE_THROUGH;
            }
            style.set_decoration(
                decoration,
                argb(span.style.decoration_color.unwrap_or(span.style.color)),
                match span.style.decoration_style {
                    nsh_core::DecorationStyle::Solid => {
                        ohos_drawing_binding::DecorationPattern::Solid
                    }
                    nsh_core::DecorationStyle::Double => {
                        ohos_drawing_binding::DecorationPattern::Double
                    }
                    nsh_core::DecorationStyle::Dotted => {
                        ohos_drawing_binding::DecorationPattern::Dotted
                    }
                    nsh_core::DecorationStyle::Dashed => {
                        ohos_drawing_binding::DecorationPattern::Dashed
                    }
                    nsh_core::DecorationStyle::Wavy => {
                        ohos_drawing_binding::DecorationPattern::Wavy
                    }
                },
            );
            if let Some(shadow) = text.shadow {
                style.add_shadow(&ohos_drawing_binding::TextShadow::new(
                    argb(shadow.color),
                    &ohos_drawing_binding::Point::new(shadow.offset_x.0, shadow.offset_y.0),
                    shadow.radius.0 as f64,
                ));
            }
            style.set_font_size(span.style.font_size.0 as f64);
            style.set_font_style(FontStyle::new(
                FontWeight::from_css(span.style.font_weight),
                FontWidth::Normal,
                if span.style.italic {
                    FontSlant::Italic
                } else {
                    FontSlant::Normal
                },
            ));
            style.set_letter_spacing(span.style.letter_spacing.0 as f64);
            for tag in span.style.font_variant.tags() {
                style.add_font_feature(tag, 1);
            }
            if let Some(family) = span.style.font_family {
                let family = family.name();
                if !family.contains('\0') {
                    style.set_font_families(&[&family]);
                }
            }
            if span.style.line_height.0 > 0.0
                && span.style.font_size.0 > 0.0
                && let Err(error) = style.set_font_height(
                    span.style.line_height.0 as f64 / span.style.font_size.0 as f64,
                )
            {
                log::warn!("OHOS text line height: {error}");
            }
            builder.push_text_style(&mut style);
            let mut start = span.start as usize;
            for attachment in text
                .attachments
                .iter()
                .filter(|a| a.index >= span.start && a.index < span.end)
            {
                let at = attachment.index as usize;
                add_slice(&mut builder, start, at);
                if let Err(error) =
                    builder.add_placeholder(&ohos_drawing_binding::PlaceholderSpan {
                        width: attachment.width.0 as f64,
                        height: attachment.height.0 as f64,
                        // The drawing ABI's above-baseline/alphabetic alignment
                        // preserves the existing inline attachment placement.
                        alignment: ohos_drawing_binding::PlaceholderAlignment::AboveBaseline.into(),
                        baseline: ohos_drawing_binding::PlaceholderBaseline::Alphabetic.into(),
                        baselineOffset: 0.0,
                    })
                {
                    log::warn!("OHOS text attachment: {error}");
                }
                start = at + 1;
            }
            add_slice(&mut builder, start, span.end as usize);
            builder.pop_text_style();
            if custom_decoration {
                decorations.push(crate::text::decoration::Decoration::new(
                    span.start, span.end, span.style, style,
                ));
            }
        }
        let shared_text: Arc<str> = Arc::from(text.text.as_str());
        Prepared {
            paragraph: RefCell::new(Arc::new(Mutex::new(Paragraph {
                native: builder.build(),
                width: None,
                _fonts: self.font_data.borrow().clone(),
            }))),
            source: text.clone(),
            single_line_ltr: !rtl && matches!(text.align, TextAlign::Auto | TextAlign::Left),
            has_attachments: !text.attachments.is_empty(),
            selectable: (text.selectable == Some(true)).then(|| shared_text.clone()),
            text: shared_text,
            no_wrap: text.no_wrap,
            links: text.links.clone(),
            backgrounds,
            decorations,
            selection_color: text.selection_color,
        }
    }

    fn measure(&self, prepared: &Prepared, max_width: Option<Points>) -> Size<Points> {
        let width = if prepared.no_wrap { None } else { max_width };
        let target = width.map_or(1_000_000.0, |w| w.0).max(0.0) as f64;
        let shared = prepared.layout(self, target, true);
        let owned = shared.lock().expect("text paragraph");
        let paragraph = &owned.native;
        Size::new(
            Points(paragraph.longest_line().ceil() as f32),
            Points(paragraph.height().ceil() as f32),
        )
    }

    fn layout(&self, prepared: &Prepared, size: Size<Points>) -> TextLayout {
        let size = crate::render::geometry::Size::from(size);
        let target = if prepared.no_wrap {
            1_000_000.0
        } else {
            size.width.max(0.0) as f64
        };
        let shared = prepared.layout(self, target, false);
        let mut owned = shared.lock().expect("text paragraph");
        let background_lines = (!prepared.backgrounds.is_empty()).then(|| owned.lines());
        let paragraph = &mut owned.native;
        let (mut top, mut bottom) = (0.0f32, size.height);
        for decoration in &prepared.decorations {
            if let Some((min, max)) = decoration.vertical_bounds(paragraph, self.scale) {
                top = top.min(min);
                bottom = bottom.max(max);
            }
        }
        let padding_top = (-top * self.scale).ceil() / self.scale;
        let scale = self.scale;
        let width = (f64::from(size.width.max(0.0)) * f64::from(scale))
            .ceil()
            .min(f64::from(i32::MAX)) as i32;
        let height = (f64::from((bottom + padding_top).max(0.0)) * f64::from(scale))
            .ceil()
            .min(f64::from(i32::MAX)) as i32;
        let lines = background_lines
            .as_ref()
            .map_or_else(OnceLock::new, |lines| OnceLock::from(lines.clone()));
        let mut raster = NativeText {
            origin_y: -padding_top,
            drawing: Mutex::new(None),
            scale,
            text: prepared.text.clone(),
            links: Vec::new(),
            attachments: Vec::new(),
            selection: None,
            selection_color: prepared.selection_color,
            _budget: None,
        };
        if !prepared.links.is_empty() {
            raster.links = prepared
                .links
                .iter()
                .map(|link| {
                    (
                        *link,
                        paragraph.range_rectangles(link.start as usize, link.end as usize),
                    )
                })
                .collect();
        }
        if prepared.has_attachments {
            raster.attachments = paragraph
                .placeholder_rectangles()
                .into_iter()
                .map(|rect| {
                    (rect[3] > rect[1] && rect[1] < size.height && rect[0] < size.width)
                        .then_some(Point::new(Points(rect[0]), Points(rect[1])))
                })
                .collect();
        }
        if width > 0 && height > 0 {
            let selectable = prepared.selectable.as_ref().filter(|text| {
                text.len() <= crate::text::selection::layout::MAX_TEXT_BYTES
                    && text
                        .graphemes(true)
                        .take(crate::text::selection::layout::MAX_CLUSTERS + 1)
                        .count()
                        <= crate::text::selection::layout::MAX_CLUSTERS
            });
            if let Some(text) = selectable {
                raster.selection = crate::text::selection::layout::Layout::new(
                    text.clone(),
                    size.width,
                    size.height,
                    |range| {
                        paragraph
                            .range_boxes(range.start, range.end)
                            .into_iter()
                            .map(|r| crate::text::selection::layout::Region {
                                bounds: r.bounds,
                                rtl: r.rtl,
                            })
                            .collect()
                    },
                );
                if raster.selection.is_none() {
                    log::warn!("OHOS text selection cache limit; rendering without selection");
                }
            }
            if let Some(selection) = &raster.selection {
                let bytes = selection.storage_bytes();
                if self
                    .budget
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                        used.checked_add(bytes).filter(|n| *n <= SELECTION_BUDGET)
                    })
                    .is_ok()
                {
                    raster._budget = Some(Reservation {
                        used: self.budget.clone(),
                        bytes,
                    });
                } else {
                    raster.selection = None;
                    log::warn!("OHOS text selection budget exhausted; rendering without selection");
                }
            }
            let recording = match ohos_drawing_binding::Recording::new(width, height) {
                Ok(recording) => recording,
                Err(error) => {
                    log::warn!("OHOS text recording: {error}");
                    return TextLayout(Arc::new(EngineText {
                        native: Arc::new(raster),
                        paragraph: shared.clone(),
                        lines,
                    }));
                }
            };
            let canvas = recording.canvas();
            canvas.save();
            canvas.clip_rect(
                &ohos_drawing_binding::Rect::new(0.0, 0.0, width as f32, height as f32),
                ohos_drawing_binding::ClipOperation::Intersect,
                false,
            );
            canvas.scale(scale, scale);
            canvas.translate(0.0, padding_top);
            if !prepared.backgrounds.is_empty() {
                for line in background_lines.as_ref().expect("background line geometry") {
                    let baseline = (line.y + line.ascender).0;
                    let mut raw_pieces = Vec::new();
                    for &(span_start, span_end, bg) in &prepared.backgrounds {
                        let start = span_start.max(line.start) as usize;
                        let end = span_end.min(line.end) as usize;
                        if start >= end {
                            continue;
                        }
                        for rect in paragraph.range_rectangles(start, end) {
                            let width = rect[2] - rect[0];
                            if width <= 0.0 {
                                continue;
                            }
                            let ascent = (baseline - rect[1]).max(0.0);
                            let descent = (rect[3] - baseline).max(0.0);
                            raw_pieces.push((rect[0], rect[2], bg, ascent, descent));
                        }
                    }
                    if raw_pieces.is_empty() {
                        continue;
                    }
                    raw_pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
                    let mut pieces = Vec::with_capacity(raw_pieces.len() * 2);
                    let mut prev_right: Option<f32> = None;
                    for (left, right, bg, ascent, descent) in raw_pieces {
                        if let Some(prev) = prev_right
                            && left > prev + 0.5
                        {
                            pieces.push(nsh_core::LinePiece {
                                x: Points(prev),
                                width: Points(left - prev),
                                background: None,
                                ascent: Points::ZERO,
                                descent: Points::ZERO,
                            });
                        }
                        prev_right = Some(prev_right.map_or(right, |p| p.max(right)));
                        pieces.push(nsh_core::LinePiece {
                            x: Points(left),
                            width: Points(right - left),
                            background: Some(bg),
                            ascent: Points(ascent),
                            descent: Points(descent),
                        });
                    }
                    let line_right = size.width.max((line.x + line.width).0);
                    nsh_core::BackgroundBox::join(pieces, (Points(0.0), Points(line_right)), |b| {
                        let mut brush = ohos_drawing_binding::Brush::new();
                        brush.set_anti_alias(true);
                        brush.set_color(argb(b.background.color));
                        canvas.attach_brush(&brush);
                        let r = b.radius();
                        let top = baseline - b.ascent.0;
                        let h = (b.ascent + b.descent).0;
                        if r.0 > 0.0 {
                            let mut path = ohos_drawing_binding::Path::new();
                            nsh_core::shape::rounded_rect(
                                nsh_core::Bounds::new(
                                    Point::new(b.x, Points(top)),
                                    Size::new(b.width, Points(h)),
                                ),
                                nsh_core::Corners::all(Size::new(r, r)),
                                false,
                                false,
                                &mut |s| match s {
                                    nsh_core::shape::Segment::Move(p) => {
                                        path.move_to(p.x.0, p.y.0);
                                    }
                                    nsh_core::shape::Segment::Line(p) => {
                                        path.line_to(p.x.0, p.y.0);
                                    }
                                    nsh_core::shape::Segment::Cubic([a, b, c]) => {
                                        path.bezier_curve_to(
                                            a.x.0, a.y.0, b.x.0, b.y.0, c.x.0, c.y.0,
                                        );
                                    }
                                    nsh_core::shape::Segment::Close => path.close(),
                                },
                            );
                            canvas.draw_path(&path);
                        } else {
                            canvas.draw_rect(&ohos_drawing_binding::Rect::new(
                                b.x.0,
                                top,
                                b.x.0 + b.width.0,
                                top + h,
                            ));
                        }
                        canvas.detach_brush();
                    });
                }
            }
            paragraph.paint(canvas, 0.0, 0.0);
            for decoration in &prepared.decorations {
                decoration.draw(canvas, paragraph, self.scale, size.width);
            }
            canvas.restore();
            match recording.finish() {
                Ok(commands) => {
                    *raster.drawing.get_mut().expect("new text drawing") = Some(TextDrawing {
                        commands,
                        _fonts: self.font_data.borrow().clone(),
                    });
                }
                Err(error) => log::warn!("OHOS text recording: {error}"),
            }
        }
        TextLayout(Arc::new(EngineText {
            native: Arc::new(raster),
            paragraph: shared.clone(),
            lines,
        }))
    }
}
