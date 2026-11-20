//! Image views in the terminal: loading through the core's shared loader
//! and decoded images (`docs/dev/images.md`), fitting the picture into the
//! view's Cells with its resize mode and position, and drawing it with what
//! the terminal has (`Graphics`).
//!
//! - **Blocks** (any terminal with 256 colors or more): each Cell is an
//!   upper half block `▀`, its foreground the top pixel and its background
//!   the bottom one, so a Cell holds two pixels; a transparent half shows
//!   the surface under the image (`▄` or `▀` over it). With 16 colors or
//!   none the box stays blank, as before.
//! - **Kitty placeholders** (kitty and Ghostty, 24-bit color): the picture,
//!   fitted at the terminal's own Cell pixels, is transmitted once with the
//!   kitty graphics protocol as a virtual placement (`a=T,U=1`), and each
//!   Cell of the box is a Unicode placeholder (U+10EEEE, its row and column
//!   as combining marks, the image id as its foreground color). The
//!   terminal draws the image part in each such Cell, so clipping,
//!   scrolling, overlays and ratatui's diff all work on ordinary Cells.
//!   Transmissions go out ahead of the frame's first Cell
//!   ([`crate::MeasuredBackend`]); a placement no longer drawn by its view
//!   is deleted with the image's data.
//!
//! An image decodes once at the size its box needs (the first frame of an
//! animation: the terminal runs with reduced motion), and loads again when
//! its box outgrows it. `onLoadStart`, `onLoad`, `onError` and `onLoadEnd`
//! are reported as on the other backends.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, LazyLock},
};

use base64::Engine as _;
use crossbeam_channel::Sender;
use nsh_core::{
    EventKind, EventPayload, ImageData, ImageSource, NodeId, ObjectPosition, PayloadValue, ROOT_ID,
    ResizeMode,
    geometry::{Bounds, DevicePixels, Points, Size},
    image::{
        Frames, Images, Lease, Natural, Request, Target, Want,
        loader::{Context, Priority, Ticket},
    },
};
use ratatui_core::{buffer::Buffer, layout::Rect, style::Color};

use crate::{
    ColorDepth, Graphics,
    geometry::{CellBounds, CellSize},
    palette::{self, Ground, Rgb},
    surface::{Message, Wake},
};

/// A decoded picture: straight (not premultiplied) RGBA pixels.
pub(crate) struct Pixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// A decoded picture, shared by the views that show it.
#[derive(Clone)]
pub(crate) struct Picture(Arc<Pixels>);

impl Picture {
    fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self(Arc::new(Pixels {
            width,
            height,
            rgba,
        }))
    }

    /// Its identity, for the caches of what was drawn from it.
    fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
}

/// The process's decoded images, in the core's budget.
fn images() -> &'static Images<Picture> {
    static IMAGES: LazyLock<Images<Picture>> = LazyLock::new(|| {
        Images::new(
            nsh_core::image::budget(),
            |bytes: &ImageData, target: &Target| {
                let (picture, natural) = decode(bytes, target)?;
                let cost = picture.0.rgba.len();
                Ok((picture, natural, cost))
            },
        )
    });
    &IMAGES
}

/// Decodes `bytes` at `target` (any thread): a pixel buffer as it is, an
/// SVG with the shared rasterizer, other formats with the `image` crate,
/// shrunk to the target as they are decoded.
fn decode(bytes: &ImageData, target: &Target) -> Result<(Picture, Natural), String> {
    if let Some((width, height, rgba)) = bytes.rgba() {
        if rgba.len() != width as usize * height as usize * 4 {
            return Err("RGBA of the wrong size".into());
        }
        return Ok((
            Picture::new(width, height, rgba.to_vec()),
            Natural::Pixels(width, height),
        ));
    }
    #[cfg(feature = "svg")]
    if nsh_core::svg::sniff(bytes) {
        let raster =
            nsh_core::svg::rasterize(bytes, target.longest.max(1)).map_err(|e| e.to_string())?;
        let mut rgba = raster.pixels;
        unpremultiply(&mut rgba);
        return Ok((
            Picture::new(raster.width, raster.height, rgba),
            Natural::Vector(raster.size),
        ));
    }
    let image = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let (width, height) = (image.width(), image.height());
    let longest = target.longest;
    let image = if longest > 0 && longest < width.max(height) {
        let scale = f64::from(longest) / f64::from(width.max(height));
        let side = |n: u32| ((f64::from(n) * scale).round() as u32).max(1);
        image.thumbnail(side(width), side(height))
    } else {
        image
    };
    let image = image.into_rgba8();
    let (w, h) = image.dimensions();
    Ok((
        Picture::new(w, h, image.into_raw()),
        Natural::Pixels(width, height),
    ))
}

/// Straight RGBA to premultiplied.
fn premultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(pixel[3]);
        if a != 255 {
            for c in &mut pixel[..3] {
                *c = ((u32::from(*c) * a + 127) / 255) as u8;
            }
        }
    }
}

/// Premultiplied RGBA (the SVG rasterizer's, a premultiplied resample) to
/// straight.
fn unpremultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(pixel[3]);
        if a != 0 && a != 255 {
            for c in &mut pixel[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// A finished load, sent to the surface that asked.
pub(crate) enum Loaded {
    /// An image view's picture.
    View {
        id: NodeId,
        generation: u64,
        result: Result<(Picture, Natural, Lease), String>,
    },
    /// A picture fitted and encoded for its kitty placement.
    Encoded {
        view: NodeId,
        key: FitKey,
        id: u32,
        result: Result<Vec<u8>, String>,
    },
    /// The size `Image.getSize` / `Image.prefetch` asked for
    /// (`Command::ImageInfo`), by the request's number.
    Info {
        request: u32,
        result: Result<Natural, String>,
    },
}

/// Where a finished load is sent, and the wake that tells the surface.
type Sink = (Sender<Message>, Arc<Wake>);

/// A surface's side of its images: where their loads report and resolve,
/// how the terminal shows them, and what the frame queued for it.
pub(crate) struct Loader {
    /// The engine's inbox and wake, while an app runs.
    sink: Option<Sink>,
    context: Context,
    graphics: Graphics,
    /// The terminal's colors: with fewer than 256 and no kitty graphics
    /// pictures are not drawn.
    colors: ColorDepth,
    /// Pixels per Cell (width, height) of the terminal's font.
    cell_pixels: (u32, u32),
    /// Kitty graphics commands for the terminal, written ahead of the next
    /// frame's Cells (`crate::perf::Output::graphics`).
    queue: Rc<RefCell<Vec<u8>>>,
    next_id: u32,
    /// The thread that fits and encodes kitty placements, once one is.
    encoder: Option<Sender<(Job, Sink)>>,
    /// Image events to report once the frame is drawn.
    pub(crate) events: Vec<(NodeId, EventKind, EventPayload)>,
    /// The placeholder Cells of this frame (by buffer index) and their
    /// image's color: what a pass over the frame (a band, reverse video)
    /// changed is set back, or the terminal would draw another image.
    placeholders: Vec<(usize, Color)>,
}

impl Default for Loader {
    fn default() -> Self {
        Self {
            sink: None,
            context: Context::default(),
            graphics: Graphics::Blocks,
            colors: ColorDepth::TrueColor,
            cell_pixels: (10, 20),
            queue: Rc::default(),
            next_id: 0,
            encoder: None,
            events: Vec::new(),
            placeholders: Vec::new(),
        }
    }
}

impl Loader {
    /// Sets how the terminal shows images: its graphics, its Cell pixels
    /// (`None`: twice as many as the Cell's points across, the same
    /// proportion down) and the queue the backend writes.
    pub(crate) fn configure(
        &mut self,
        graphics: Graphics,
        colors: ColorDepth,
        cell_pixels: Option<Size<DevicePixels>>,
        cells: CellSize,
        queue: Rc<RefCell<Vec<u8>>>,
    ) {
        // A placeholder's image id is its 24-bit foreground color.
        self.graphics = match graphics {
            Graphics::KittyPlaceholders if colors == ColorDepth::TrueColor => graphics,
            _ => Graphics::Blocks,
        };
        self.colors = colors;
        self.cell_pixels = cell_pixels
            .filter(|s| s.width.0 > 0 && s.height.0 > 0)
            .map(|s| (s.width.0 as u32, s.height.0 as u32))
            .unwrap_or_else(|| {
                let width = 10.0;
                let height = width * cells.height().0 / cells.width().0;
                (width as u32, height.round().max(1.0) as u32)
            });
        self.queue = queue;
    }

    /// Where the loads of the app just started go: its inbox, wake and
    /// asset reader, access policy, media files and disk cache.
    pub(crate) fn start(&mut self, sink: Sink, context: Context) {
        self.sink = Some(sink);
        self.context = context;
    }

    /// The app stopped: its loads no longer report.
    pub(crate) fn stop(&mut self) {
        self.sink = None;
        self.context = Context::default();
        self.events.clear();
    }

    /// Whether `image` shows a picture: it loaded, and the terminal draws
    /// pictures. Until then a control holding it shows its label.
    pub(crate) fn shows(&self, image: &ImageView) -> bool {
        image.shown.is_some()
            && (self.graphics == Graphics::KittyPlaceholders
                || matches!(self.colors, ColorDepth::TrueColor | ColorDepth::Ansi256))
    }

    /// Answers `Command::ImageInfo` number `request` for `uri`
    /// (`Image.getSize`, `Image.prefetch`): the size its header gives, or
    /// a decode's where it gives none. Its bytes stay in the loader's
    /// memory for the views that show it next.
    pub(crate) fn info(&mut self, request: u32, uri: &str) {
        let Some((tx, wake)) = self.sink.clone() else {
            return;
        };
        let report = move |result: Result<Natural, String>| {
            if tx
                .send(Message::Image(Loaded::Info { request, result }))
                .is_ok()
            {
                wake.notify();
            }
        };
        let Some(location) = self.context.resolve(uri) else {
            report(Err(format!("unsupported image source `{uri}`")));
            return;
        };
        let decode = Request {
            location: location.clone(),
            context: self.context.clone(),
            declared: None,
            want: Want::natural(1.0),
            frames: Frames::First,
            priority: Priority::Prefetch,
            progress: None,
        };
        // The header first; a decode only for what it cannot tell.
        nsh_core::image::loader::loader()
            .read(
                location,
                &self.context,
                Priority::Prefetch,
                None,
                Box::new(move |bytes| match bytes.map(|b| Natural::of(&b)) {
                    Ok(Some(natural)) => report(Ok(natural)),
                    Ok(None) => images()
                        .load(decode, Box::new(move |r| report(r.map(|(d, _)| d.natural))))
                        .detach(),
                    Err(e) => report(Err(e)),
                }),
            )
            .detach();
    }

    fn next_id(&mut self) -> u32 {
        // Ids fill the foreground's 24 bits; 0 is no image.
        self.next_id = self.next_id % 0x00FF_FFFF + 1;
        self.next_id
    }

    /// Fits and encodes `job` on the encoding thread; its result comes back
    /// as a message. `false` when no app runs or the thread cannot start.
    fn encode(&mut self, job: Job) -> bool {
        let Some(sink) = self.sink.clone() else {
            return false;
        };
        if self.encoder.is_none() {
            let (tx, rx) = crossbeam_channel::unbounded::<(Job, Sink)>();
            let spawned = std::thread::Builder::new()
                .name("nsh-tui-images".into())
                .spawn(move || {
                    // Ends with the surface, which drops the sender.
                    for (job, (tx, wake)) in rx {
                        if tx.send(Message::Image(job.run())).is_ok() {
                            wake.notify();
                        }
                    }
                });
            if let Err(e) = spawned {
                log::warn!("image: no encoding thread: {e}");
                return false;
            }
            self.encoder = Some(tx);
        }
        self.encoder
            .as_ref()
            .is_some_and(|encoder| encoder.send((job, sink)).is_ok())
    }

    /// Device pixels per point across.
    fn scale(&self, cells: CellSize) -> f32 {
        self.cell_pixels.0 as f32 / cells.width().0
    }

    /// Sets back what changed in the placeholder Cells after they were
    /// drawn: the image's color as their foreground and no attributes.
    /// A Cell something else was drawn over keeps it.
    pub(crate) fn finish(&mut self, buffer: &mut Buffer) {
        for (index, color) in self.placeholders.drain(..) {
            if let Some(cell) = buffer.content.get_mut(index)
                && cell.symbol().starts_with(PLACEHOLDER)
            {
                cell.fg = color;
                cell.modifier = ratatui_core::style::Modifier::empty();
                cell.underline_color = Color::Reset;
            }
        }
    }
}

/// An image view's source and what it shows.
#[derive(Default)]
pub(crate) struct ImageView {
    source: Option<ImageSource>,
    mode: ResizeMode,
    position: ObjectPosition,
    /// Counts source changes: a load of an earlier source is dropped.
    generation: u64,
    load: Option<Ticket>,
    /// The content box, in Cells, of the load running or the last one.
    asked: (i32, i32),
    shown: Option<Shown>,
    failed: bool,
    /// The half blocks last drawn, for this key.
    blocks: Option<(FitKey, Vec<[u8; 4]>)>,
    /// The kitty placement last transmitted.
    placed: Option<Placement>,
    /// The placement being fitted and encoded.
    encoding: Option<FitKey>,
}

/// A loaded picture and its natural size in points.
struct Shown {
    picture: Picture,
    natural: Size<Points>,
    /// Keeps the decoded picture in the cache while it is shown.
    _lease: Lease,
}

/// What a fitted picture was made for.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct FitKey {
    picture: usize,
    cols: i32,
    rows: i32,
    mode: ResizeMode,
    position: ObjectPosition,
    tint: Option<nsh_core::Color>,
    pixels: (u32, u32),
}

/// An image transmitted to the terminal, deleted with its data when the
/// view no longer shows it.
struct Placement {
    key: FitKey,
    id: u32,
    queue: Rc<RefCell<Vec<u8>>>,
}

impl Drop for Placement {
    fn drop(&mut self) {
        let delete = format!("\x1b_Ga=d,d=I,i={},q=2\x1b\\", self.id);
        self.queue.borrow_mut().extend_from_slice(delete.as_bytes());
    }
}

impl ImageView {
    /// Sets the view's source (`MountItem::Image`); a new source drops what
    /// the previous one showed and loads when the view is next drawn.
    pub(crate) fn set(
        &mut self,
        source: Option<ImageSource>,
        mode: ResizeMode,
        position: ObjectPosition,
    ) {
        self.mode = mode;
        self.position = position;
        if self.source != source {
            self.source = source;
            self.generation += 1;
            self.load = None;
            self.asked = (0, 0);
            self.shown = None;
            self.failed = false;
            self.blocks = None;
            self.placed = None;
            self.encoding = None;
        }
    }

    /// Takes a finished load of source `generation`, if it is the current
    /// one; the events it reports go to `events`.
    fn loaded(
        &mut self,
        id: NodeId,
        generation: u64,
        result: Result<(Picture, Natural, Lease), String>,
        events: &mut Vec<(NodeId, EventKind, EventPayload)>,
    ) {
        if generation != self.generation {
            return;
        }
        self.load = None;
        match result {
            Ok((picture, natural, lease)) => self.show(id, picture, natural, lease, events),
            Err(message) => self.fail(id, message, events),
        }
    }

    fn show(
        &mut self,
        id: NodeId,
        picture: Picture,
        natural: Natural,
        lease: Lease,
        events: &mut Vec<(NodeId, EventKind, EventPayload)>,
    ) {
        let density = self
            .source
            .as_ref()
            .map_or(1.0, |s| nsh_core::image::density(&s.uri));
        let declared = self
            .source
            .as_ref()
            .and_then(|s| s.size)
            .filter(|s| s.width.0 > 0.0 && s.height.0 > 0.0);
        let first = self.shown.is_none();
        self.shown = Some(Shown {
            picture,
            natural: declared.unwrap_or_else(|| natural.points(density)),
            _lease: lease,
        });
        // A larger decode of the same source swaps in without events.
        if first {
            events.push((
                id,
                EventKind::Load,
                EventPayload::Image(reported_size(natural)),
            ));
            events.push((id, EventKind::LoadEnd, EventPayload::None));
        }
    }

    fn fail(
        &mut self,
        id: NodeId,
        message: String,
        events: &mut Vec<(NodeId, EventKind, EventPayload)>,
    ) {
        log::warn!("image failed: {message}");
        self.failed = true;
        events.push((id, EventKind::Error, EventPayload::Error { message }));
        events.push((id, EventKind::LoadEnd, EventPayload::None));
    }

    /// Starts the load for a content box of `cols` × `rows` Cells, unless
    /// one runs or the picture shown is large enough.
    fn request(&mut self, id: NodeId, cols: i32, rows: i32, cells: CellSize, loader: &mut Loader) {
        let Some(source) = &self.source else {
            return;
        };
        if self.failed
            || self.load.is_some()
            || (self.shown.is_some() && cols <= self.asked.0 && rows <= self.asked.1)
        {
            return;
        }
        let Some((tx, wake)) = loader.sink.clone() else {
            return;
        };
        let scale = loader.scale(cells);
        let source = source.at_pixel_ratio(scale);
        let first = self.shown.is_none();
        let Some(location) = loader.context.locate(&source) else {
            if first {
                loader
                    .events
                    .push((id, EventKind::LoadStart, EventPayload::None));
            }
            self.fail(
                id,
                format!("unsupported image source `{}`", source.uri),
                &mut loader.events,
            );
            return;
        };
        self.asked = (cols, rows);
        let request = Request {
            location,
            context: loader.context.clone(),
            declared: source.size,
            want: Want {
                size: Size::new(
                    Points(cols as f32 * cells.width().0),
                    Points(rows as f32 * cells.height().0),
                ),
                scale,
                mode: self.mode,
            },
            frames: Frames::First,
            priority: Priority::Visible,
            progress: None,
        };
        if first {
            loader
                .events
                .push((id, EventKind::LoadStart, EventPayload::None));
        }
        if let Some((decoded, lease)) = images().cached(&request) {
            self.show(
                id,
                decoded.value,
                decoded.natural,
                lease,
                &mut loader.events,
            );
            return;
        }
        let generation = self.generation;
        self.load = Some(images().load(
            request,
            Box::new(move |result| {
                let result = result.map(|(decoded, lease)| (decoded.value, decoded.natural, lease));
                let loaded = Loaded::View {
                    id,
                    generation,
                    result,
                };
                if tx.send(Message::Image(loaded)).is_ok() {
                    wake.notify();
                }
            }),
        ));
    }
}

/// The size `onLoad` reports: a bitmap's pixels, an SVG's whole points.
fn reported_size(natural: Natural) -> Size<DevicePixels> {
    match natural {
        Natural::Pixels(w, h) => Size::new(DevicePixels(w as i32), DevicePixels(h as i32)),
        Natural::Vector(points) => Size::new(
            DevicePixels(points.width.0.round() as i32),
            DevicePixels(points.height.0.round() as i32),
        ),
    }
}

/// Hands a finished load or encode to its view in `views`, or answers the
/// root's `ImageInfo`; the events it reports go to `loader`'s.
pub(crate) fn deliver(
    views: &mut [Option<crate::view::NshView>],
    loaded: Loaded,
    loader: &mut Loader,
) {
    let events = &mut loader.events;
    fn image(views: &mut [Option<crate::view::NshView>], id: NodeId) -> Option<&mut ImageView> {
        views
            .get_mut(id as usize)
            .and_then(Option::as_mut)
            .and_then(|view| view.image.as_deref_mut())
    }
    match loaded {
        Loaded::Encoded {
            view,
            key,
            id,
            result,
        } => {
            let Some(image) = image(views, view).filter(|i| i.encoding == Some(key)) else {
                return;
            };
            image.encoding = None;
            match result {
                Ok(bytes) => {
                    loader.queue.borrow_mut().extend_from_slice(&bytes);
                    image.placed = Some(Placement {
                        key,
                        id,
                        queue: loader.queue.clone(),
                    });
                }
                Err(e) => log::warn!("image: not encoded for the terminal: {e}"),
            }
        }
        Loaded::View {
            id,
            generation,
            result,
        } => {
            if let Some(image) = image(views, id) {
                image.loaded(id, generation, result, events);
            }
        }
        Loaded::Info { request, result } => {
            let mut fields = vec![("id", PayloadValue::Number(f64::from(request)))];
            match result {
                Ok(natural) => {
                    let size = reported_size(natural);
                    fields.push(("width", PayloadValue::Number(f64::from(size.width.0))));
                    fields.push(("height", PayloadValue::Number(f64::from(size.height.0))));
                }
                Err(message) => fields.push(("error", PayloadValue::Str(message))),
            }
            events.push((ROOT_ID, EventKind::ImageInfo, EventPayload::Map(fields)));
        }
    }
}

/// Starts the load of image view `id`, not drawn (transparent), for its
/// box `content`, as [`draw`] does for one drawn.
pub(crate) fn prepare(
    image: &mut ImageView,
    loader: &mut Loader,
    id: NodeId,
    content: CellBounds,
    cells: CellSize,
) {
    let cols = content.right - content.left;
    let rows = content.bottom - content.top;
    if cols > 0 && rows > 0 {
        image.request(id, cols, rows, cells, loader);
    }
}

/// Draws image view `id` into the `visible` Cells of its content box
/// `content`, over `ground`, tinted with `tint` (`tintColor`), and starts
/// its load when it needs one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    image: &mut ImageView,
    loader: &mut Loader,
    id: NodeId,
    buffer: &mut Buffer,
    content: CellBounds,
    visible: Rect,
    cells: CellSize,
    ground: Ground,
    tint: Option<nsh_core::Color>,
) {
    let cols = content.right - content.left;
    let rows = content.bottom - content.top;
    if cols <= 0 || rows <= 0 || visible.is_empty() {
        return;
    }
    image.request(id, cols, rows, cells, loader);
    let Some(shown) = &image.shown else {
        return;
    };
    let tint = tint.filter(|t| t.0 & 255 != 0);
    let key = FitKey {
        picture: shown.picture.key(),
        cols,
        rows,
        mode: image.mode,
        position: image.position,
        tint,
        pixels: loader.cell_pixels,
    };
    let fit = Fit {
        natural: shown.natural,
        size: Size::new(
            Points(cols as f32 * cells.width().0),
            Points(rows as f32 * cells.height().0),
        ),
        mode: image.mode,
        position: image.position,
        tint: tint.map(|t| t.0.to_be_bytes()),
    };
    if loader.graphics == Graphics::KittyPlaceholders
        && cols as usize <= DIACRITICS.len()
        && rows as usize <= DIACRITICS.len()
    {
        if image.placed.as_ref().is_none_or(|p| p.key != key) && image.encoding != Some(key) {
            // Fitted and encoded off the drawing thread: a screen of new
            // pictures (a list scrolled) does not hold up the frame.
            let (cw, ch) = loader.cell_pixels;
            let (width, height) = capped(cols as u32 * cw, rows as u32 * ch);
            let job = Job {
                view: id,
                key,
                id: loader.next_id(),
                picture: shown.picture.clone(),
                fit,
                pixels: (width, height),
                cells: (cols as u32, rows as u32),
            };
            if loader.encode(job) {
                image.encoding = Some(key);
            }
        }
        if let Some(placed) = image.placed.as_ref().filter(|p| p.key == key) {
            let color = Color::Rgb(
                (placed.id >> 16) as u8,
                (placed.id >> 8) as u8,
                placed.id as u8,
            );
            for y in visible.top()..visible.bottom() {
                for x in visible.left()..visible.right() {
                    let row = (i32::from(y) - content.top) as usize;
                    let col = (i32::from(x) - content.left) as usize;
                    let cell = &mut buffer[(x, y)];
                    let mut symbol = String::with_capacity(12);
                    symbol.push(PLACEHOLDER);
                    symbol.push(DIACRITICS[row]);
                    symbol.push(DIACRITICS[col]);
                    cell.set_symbol(&symbol);
                    cell.fg = color;
                    cell.bg = ground.surface;
                    cell.modifier = ratatui_core::style::Modifier::empty();
                    loader.placeholders.push((buffer.index_of(x, y), color));
                }
            }
            return;
        }
    }
    if !matches!(ground.depth, ColorDepth::TrueColor | ColorDepth::Ansi256) {
        return;
    }
    if image.blocks.as_ref().is_none_or(|(k, _)| *k != key) {
        let rgba = fit.render(&shown.picture.0, cols as u32, rows as u32 * 2);
        let pixels = rgba.as_chunks::<4>().0.to_vec();
        image.blocks = Some((key, pixels));
    }
    let Some((_, pixels)) = &image.blocks else {
        return;
    };
    // A translucent pixel is mixed into the color the terminal shows under
    // it: the surface's own, else the terminal's background.
    let under = match ground.surface {
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Reset => ground.ramp.map_or(ground.rgb, |ramp| ramp.mix(0.0)),
        _ => ground.rgb,
    };
    let width = cols as usize;
    for y in visible.top()..visible.bottom() {
        let row = (i32::from(y) - content.top) as usize;
        for x in visible.left()..visible.right() {
            let col = (i32::from(x) - content.left) as usize;
            let top = pixels[row * 2 * width + col];
            let bottom = pixels[(row * 2 + 1) * width + col];
            half_blocks(&mut buffer[(x, y)], top, bottom, ground, under);
        }
    }
}

/// Draws one Cell of two pixels: `▀` in the top one's color over the
/// bottom one's; a transparent half shows the surface under the image.
fn half_blocks(
    cell: &mut ratatui_core::buffer::Cell,
    top: [u8; 4],
    bottom: [u8; 4],
    ground: Ground,
    under: Rgb,
) {
    let over = |p: [u8; 4]| -> Option<Rgb> {
        (p[3] >= 8).then(|| {
            let a = u32::from(p[3]);
            std::array::from_fn(|i| {
                ((u32::from(p[i]) * a + u32::from(under[i]) * (255 - a) + 127) / 255) as u8
            })
        })
    };
    let depth = ground.depth;
    let (symbol, fg, bg) = match (over(top), over(bottom)) {
        (None, None) => return,
        (Some(t), Some(b)) => ("▀", palette::convert(t, depth), palette::convert(b, depth)),
        (Some(t), None) => ("▀", palette::convert(t, depth), ground.surface),
        (None, Some(b)) => ("▄", palette::convert(b, depth), ground.surface),
    };
    cell.set_symbol(symbol);
    cell.fg = fg;
    cell.bg = bg;
    cell.modifier = ratatui_core::style::Modifier::empty();
}

/// The longest side a transmitted picture has: a larger box is sent
/// smaller, in the same proportion, and the terminal scales it up.
const MAX_SIDE: u32 = 2048;

fn capped(width: u32, height: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= MAX_SIDE {
        return (width.max(1), height.max(1));
    }
    let scale = MAX_SIDE as f64 / longest as f64;
    let side = |n: u32| ((f64::from(n) * scale).round() as u32).max(1);
    (side(width), side(height))
}

/// A picture to fit and encode for the terminal (`Loader::encode`).
struct Job {
    view: NodeId,
    key: FitKey,
    /// Its kitty image id.
    id: u32,
    picture: Picture,
    fit: Fit,
    /// The pixels it is sent at.
    pixels: (u32, u32),
    /// The Cells it is placed in.
    cells: (u32, u32),
}

impl Job {
    fn run(self) -> Loaded {
        let (width, height) = self.pixels;
        let rgba = self.fit.render(&self.picture.0, width, height);
        let result = transmission(self.id, &rgba, width, height, self.cells.0, self.cells.1);
        Loaded::Encoded {
            view: self.view,
            key: self.key,
            id: self.id,
            result,
        }
    }
}

/// How a picture goes into its box.
#[derive(Clone, Copy)]
struct Fit {
    natural: Size<Points>,
    size: Size<Points>,
    mode: ResizeMode,
    position: ObjectPosition,
    tint: Option<[u8; 4]>,
}

impl Fit {
    /// The box as `width` × `height` RGBA pixels: the picture placed as
    /// its resize mode and position place it, cropped to the box,
    /// transparent where it does not reach.
    fn render(&self, picture: &Pixels, width: u32, height: u32) -> Vec<u8> {
        let mut out = vec![0u8; width as usize * height as usize * 4];
        let placed: Bounds<Points> = self.mode.fit(self.natural, self.size, self.position);
        let sx = width as f32 / self.size.width.0;
        let sy = height as f32 / self.size.height.0;
        let (dx, dy) = (placed.origin.x.0 * sx, placed.origin.y.0 * sy);
        let (dw, dh) = (placed.size.width.0 * sx, placed.size.height.0 * sy);
        if dw <= 0.0 || dh <= 0.0 || picture.width == 0 || picture.height == 0 {
            return out;
        }
        // The part of the placed picture inside the box, in output pixels.
        let x0 = dx.max(0.0).round() as u32;
        let y0 = dy.max(0.0).round() as u32;
        let x1 = ((dx + dw).min(width as f32).round() as u32).min(width);
        let y1 = ((dy + dh).min(height as f32).round() as u32).min(height);
        if x1 <= x0 || y1 <= y0 {
            return out;
        }
        // The source pixels behind it.
        let (pw, ph) = (picture.width as f32, picture.height as f32);
        let sx0 = ((x0 as f32 - dx) / dw * pw).clamp(0.0, pw);
        let sy0 = ((y0 as f32 - dy) / dh * ph).clamp(0.0, ph);
        let sx1 = ((x1 as f32 - dx) / dw * pw).clamp(0.0, pw);
        let sy1 = ((y1 as f32 - dy) / dh * ph).clamp(0.0, ph);
        let crop_x = sx0.floor() as u32;
        let crop_y = sy0.floor() as u32;
        let crop_w = ((sx1.ceil() as u32).min(picture.width) - crop_x).max(1);
        let crop_h = ((sy1.ceil() as u32).min(picture.height) - crop_y).max(1);
        let Some(source) =
            image::RgbaImage::from_raw(picture.width, picture.height, picture.rgba.clone())
        else {
            return out;
        };
        let mut cropped =
            image::imageops::crop_imm(&source, crop_x, crop_y, crop_w, crop_h).to_image();
        // Resampled premultiplied: the color of a transparent pixel (often
        // black) does not bleed into the edges around it.
        premultiply(&mut cropped);
        let mut resized = image::imageops::resize(
            &cropped,
            x1 - x0,
            y1 - y0,
            image::imageops::FilterType::Triangle,
        );
        unpremultiply(&mut resized);
        let row_bytes = (x1 - x0) as usize * 4;
        for (row, line) in resized.as_raw().chunks_exact(row_bytes).enumerate() {
            let start = ((y0 as usize + row) * width as usize + x0 as usize) * 4;
            out[start..start + row_bytes].copy_from_slice(line);
        }
        if let Some([r, g, b, a]) = self.tint {
            // `tintColor`: the color through the picture's alpha.
            for pixel in out.as_chunks_mut::<4>().0 {
                pixel[..3].copy_from_slice(&[r, g, b]);
                pixel[3] = ((u32::from(pixel[3]) * u32::from(a) + 127) / 255) as u8;
            }
        }
        out
    }
}

/// The kitty graphics commands that transmit `rgba` (`width` × `height`)
/// as image `id`, PNG-encoded, with a virtual placement of `cols` × `rows`
/// Cells for Unicode placeholders; answers are suppressed (`q=2`), so none
/// reaches the app as typed input.
fn transmission(
    id: u32,
    rgba: &[u8],
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
) -> Result<Vec<u8>, String> {
    use image::ImageEncoder as _;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut png,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
    .map_err(|e| e.to_string())?;
    let data = base64::engine::general_purpose::STANDARD.encode(&png);
    let mut out = Vec::with_capacity(data.len() + data.len() / CHUNK * 16 + 64);
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(CHUNK).collect();
    for (index, chunk) in chunks.iter().enumerate() {
        let more = u8::from(index + 1 < chunks.len());
        if index == 0 {
            out.extend_from_slice(
                format!("\x1b_Ga=T,U=1,f=100,t=d,q=2,i={id},c={cols},r={rows},m={more};")
                    .as_bytes(),
            );
        } else {
            out.extend_from_slice(format!("\x1b_Gm={more};").as_bytes());
        }
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
    }
    Ok(out)
}

/// The most base64 bytes one kitty graphics command carries.
const CHUNK: usize = 4096;

/// The kitty graphics protocol's Unicode placeholder.
pub(crate) const PLACEHOLDER: char = '\u{10EEEE}';

/// The combining marks that number a placeholder's row and column, from
/// kitty's `gen/rowcolumn-diacritics.txt`: the n-th is row or column n.
#[rustfmt::skip]
const DIACRITICS: [char; 297] = [
    '\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}', '\u{0312}', '\u{033D}', '\u{033E}', '\u{033F}',
    '\u{0346}', '\u{034A}', '\u{034B}', '\u{034C}', '\u{0350}', '\u{0351}', '\u{0352}', '\u{0357}',
    '\u{035B}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}', '\u{0367}', '\u{0368}', '\u{0369}',
    '\u{036A}', '\u{036B}', '\u{036C}', '\u{036D}', '\u{036E}', '\u{036F}', '\u{0483}', '\u{0484}',
    '\u{0485}', '\u{0486}', '\u{0487}', '\u{0592}', '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}',
    '\u{0598}', '\u{0599}', '\u{059C}', '\u{059D}', '\u{059E}', '\u{059F}', '\u{05A0}', '\u{05A1}',
    '\u{05A8}', '\u{05A9}', '\u{05AB}', '\u{05AC}', '\u{05AF}', '\u{05C4}', '\u{0610}', '\u{0611}',
    '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', '\u{0616}', '\u{0617}', '\u{0657}', '\u{0658}',
    '\u{0659}', '\u{065A}', '\u{065B}', '\u{065D}', '\u{065E}', '\u{06D6}', '\u{06D7}', '\u{06D8}',
    '\u{06D9}', '\u{06DA}', '\u{06DB}', '\u{06DC}', '\u{06DF}', '\u{06E0}', '\u{06E1}', '\u{06E2}',
    '\u{06E4}', '\u{06E7}', '\u{06E8}', '\u{06EB}', '\u{06EC}', '\u{0730}', '\u{0732}', '\u{0733}',
    '\u{0735}', '\u{0736}', '\u{073A}', '\u{073D}', '\u{073F}', '\u{0740}', '\u{0741}', '\u{0743}',
    '\u{0745}', '\u{0747}', '\u{0749}', '\u{074A}', '\u{07EB}', '\u{07EC}', '\u{07ED}', '\u{07EE}',
    '\u{07EF}', '\u{07F0}', '\u{07F1}', '\u{07F3}', '\u{0816}', '\u{0817}', '\u{0818}', '\u{0819}',
    '\u{081B}', '\u{081C}', '\u{081D}', '\u{081E}', '\u{081F}', '\u{0820}', '\u{0821}', '\u{0822}',
    '\u{0823}', '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082A}', '\u{082B}', '\u{082C}',
    '\u{082D}', '\u{0951}', '\u{0953}', '\u{0954}', '\u{0F82}', '\u{0F83}', '\u{0F86}', '\u{0F87}',
    '\u{135D}', '\u{135E}', '\u{135F}', '\u{17DD}', '\u{193A}', '\u{1A17}', '\u{1A75}', '\u{1A76}',
    '\u{1A77}', '\u{1A78}', '\u{1A79}', '\u{1A7A}', '\u{1A7B}', '\u{1A7C}', '\u{1B6B}', '\u{1B6D}',
    '\u{1B6E}', '\u{1B6F}', '\u{1B70}', '\u{1B71}', '\u{1B72}', '\u{1B73}', '\u{1CD0}', '\u{1CD1}',
    '\u{1CD2}', '\u{1CDA}', '\u{1CDB}', '\u{1CE0}', '\u{1DC0}', '\u{1DC1}', '\u{1DC3}', '\u{1DC4}',
    '\u{1DC5}', '\u{1DC6}', '\u{1DC7}', '\u{1DC8}', '\u{1DC9}', '\u{1DCB}', '\u{1DCC}', '\u{1DD1}',
    '\u{1DD2}', '\u{1DD3}', '\u{1DD4}', '\u{1DD5}', '\u{1DD6}', '\u{1DD7}', '\u{1DD8}', '\u{1DD9}',
    '\u{1DDA}', '\u{1DDB}', '\u{1DDC}', '\u{1DDD}', '\u{1DDE}', '\u{1DDF}', '\u{1DE0}', '\u{1DE1}',
    '\u{1DE2}', '\u{1DE3}', '\u{1DE4}', '\u{1DE5}', '\u{1DE6}', '\u{1DFE}', '\u{20D0}', '\u{20D1}',
    '\u{20D4}', '\u{20D5}', '\u{20D6}', '\u{20D7}', '\u{20DB}', '\u{20DC}', '\u{20E1}', '\u{20E7}',
    '\u{20E9}', '\u{20F0}', '\u{2CEF}', '\u{2CF0}', '\u{2CF1}', '\u{2DE0}', '\u{2DE1}', '\u{2DE2}',
    '\u{2DE3}', '\u{2DE4}', '\u{2DE5}', '\u{2DE6}', '\u{2DE7}', '\u{2DE8}', '\u{2DE9}', '\u{2DEA}',
    '\u{2DEB}', '\u{2DEC}', '\u{2DED}', '\u{2DEE}', '\u{2DEF}', '\u{2DF0}', '\u{2DF1}', '\u{2DF2}',
    '\u{2DF3}', '\u{2DF4}', '\u{2DF5}', '\u{2DF6}', '\u{2DF7}', '\u{2DF8}', '\u{2DF9}', '\u{2DFA}',
    '\u{2DFB}', '\u{2DFC}', '\u{2DFD}', '\u{2DFE}', '\u{2DFF}', '\u{A66F}', '\u{A67C}', '\u{A67D}',
    '\u{A6F0}', '\u{A6F1}', '\u{A8E0}', '\u{A8E1}', '\u{A8E2}', '\u{A8E3}', '\u{A8E4}', '\u{A8E5}',
    '\u{A8E6}', '\u{A8E7}', '\u{A8E8}', '\u{A8E9}', '\u{A8EA}', '\u{A8EB}', '\u{A8EC}', '\u{A8ED}',
    '\u{A8EE}', '\u{A8EF}', '\u{A8F0}', '\u{A8F1}', '\u{AAB0}', '\u{AAB2}', '\u{AAB3}', '\u{AAB7}',
    '\u{AAB8}', '\u{AABE}', '\u{AABF}', '\u{AAC1}', '\u{FE20}', '\u{FE21}', '\u{FE22}', '\u{FE23}',
    '\u{FE24}', '\u{FE25}', '\u{FE26}', '\u{10A0F}', '\u{10A38}', '\u{1D185}', '\u{1D186}', '\u{1D187}',
    '\u{1D188}', '\u{1D189}', '\u{1D1AA}', '\u{1D1AB}', '\u{1D1AC}', '\u{1D1AD}', '\u{1D242}', '\u{1D243}',
    '\u{1D244}',
];

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(width: u32, height: u32, color: [u8; 4]) -> Pixels {
        Pixels {
            width,
            height,
            rgba: color.repeat((width * height) as usize),
        }
    }

    fn fit(mode: ResizeMode, natural: (f32, f32), size: (f32, f32)) -> Fit {
        Fit {
            natural: Size::new(Points(natural.0), Points(natural.1)),
            size: Size::new(Points(size.0), Points(size.1)),
            mode,
            position: ObjectPosition::default(),
            tint: None,
        }
    }

    fn alpha(rgba: &[u8], width: u32) -> Vec<String> {
        rgba.chunks_exact(width as usize * 4)
            .map(|row| {
                row.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| if p[3] > 127 { '#' } else { '.' })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn contain_letterboxes_and_cover_fills() {
        let wide = picture(40, 10, [255, 0, 0, 255]);
        let out = fit(ResizeMode::Contain, (40.0, 10.0), (40.0, 40.0)).render(&wide, 4, 4);
        let rows = alpha(&out, 4);
        assert_eq!(rows.iter().filter(|r| r.as_str() == "####").count(), 1);
        assert_eq!(rows.iter().filter(|r| r.as_str() == "....").count(), 3);
        let out = fit(ResizeMode::Cover, (40.0, 10.0), (40.0, 40.0)).render(&wide, 4, 4);
        assert!(alpha(&out, 4).iter().all(|r| r == "####"));
    }

    #[test]
    fn tint_paints_through_alpha() {
        let half = Pixels {
            width: 2,
            height: 1,
            rgba: vec![9, 9, 9, 255, 9, 9, 9, 0],
        };
        let mut f = fit(ResizeMode::Stretch, (2.0, 1.0), (2.0, 1.0));
        f.tint = Some([0, 128, 255, 255]);
        let out = f.render(&half, 2, 1);
        assert_eq!(&out[..4], &[0, 128, 255, 255]);
        assert_eq!(out[7], 0);
    }

    #[test]
    fn transmission_is_chunked_and_quiet() {
        let rgba = [7u8; 64 * 64 * 4];
        let bytes = transmission(42, &rgba, 64, 64, 4, 2).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("\x1b_Ga=T,U=1,f=100,t=d,q=2,i=42,c=4,r=2,m="));
        assert!(text.ends_with("\x1b\\"));
        for command in text.split("\x1b\\").filter(|c| !c.is_empty()) {
            let payload = command.split_once(';').unwrap().1;
            assert!(payload.len() <= CHUNK);
        }
    }

    #[test]
    fn placeholders_take_one_cell() {
        use unicode_width::UnicodeWidthStr as _;
        let symbol: String = [PLACEHOLDER, DIACRITICS[3], DIACRITICS[296]]
            .iter()
            .collect();
        assert_eq!(symbol.width(), 1);
    }

    #[test]
    fn a_large_box_is_sent_smaller_in_proportion() {
        assert_eq!(capped(400, 300), (400, 300));
        assert_eq!(capped(4096, 1024), (2048, 512));
    }
}
