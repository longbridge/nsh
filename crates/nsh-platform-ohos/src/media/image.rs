//! Image IO stays off the UI thread. Native ArkUI decodes and presents validated
//! encoded data; all HTTP redirects pass the shared Rust capability policy.

use std::{
    cell::{Cell, RefCell},
    io::Read,
    rc::{Rc, Weak as UiWeak},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use nsh_core::{
    EventKind, EventPayload, ImageData, ImageOptions, ImageSource, PayloadValue, ResizeMode,
    image::{CacheHeaders, DiskCache, Fetched},
};
use nsh_engine::{Access, Assets};
use ohos_arkui_binding::{
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::Image,
    },
    types::{
        advanced::LengthMetricUnit, attribute::ArkUINodeAttributeType as A,
        event::NodeEventType as E,
    },
};
use rustc_hash::FxHashMap;

use crate::{
    app::runtime::Dispatcher,
    render::{
        geometry::Size,
        node::{NativeNode, Node, Result, callback},
    },
};

const MAX_DATA: usize = 16 << 20;
const CACHE: usize = 32 << 20;

#[derive(Clone, Copy)]
pub(crate) enum Target {
    View {
        id: u32,
        generation: u64,
        placeholder: bool,
    },
    Info(u32),
    Canvas {
        id: u32,
        generation: u64,
        index: usize,
    },
}
pub(crate) struct Ready {
    pub(crate) target: Target,
    pub(crate) data: std::result::Result<Arc<Data>, String>,
}
pub(crate) struct Progress {
    pub(crate) target: Target,
    pub(crate) loaded: u64,
    pub(crate) total: u64,
}
pub(crate) struct Data {
    bytes: Arc<[u8]>,
    svg: Option<nsh_core::svg::Raster>,
    width: u32,
    height: u32,
}
#[derive(Default)]
struct Cache {
    entries: FxHashMap<String, (Arc<Data>, u64)>,
    bytes: usize,
    clock: u64,
}
pub(crate) struct Loader {
    assets: Arc<dyn Assets>,
    access: Access,
    dispatcher: Weak<Dispatcher>,
    agent: ureq::Agent,
    cache: Mutex<Cache>,
    disk_cache: Mutex<Option<DiskCache>>,
    pending: Mutex<FxHashMap<String, Arc<crate::media::request::Request<Target>>>>,
}
impl Data {
    pub(crate) fn decoded_bytes(&self) -> usize {
        if let Some(svg) = &self.svg {
            svg.pixels.len()
        } else {
            self.width as usize * self.height as usize * 4
        }
    }
    fn resident_bytes(&self) -> usize {
        self.bytes
            .len()
            .saturating_add(self.svg.as_ref().map_or(0, |svg| svg.pixels.len()))
    }
    pub(crate) fn bitmap(&self) -> std::result::Result<ohos_drawing_binding::Bitmap, String> {
        if let Some(svg) = &self.svg {
            let mut bitmap = ohos_drawing_binding::Bitmap::new(
                svg.width,
                svg.height,
                ohos_drawing_binding::BitmapFormat::default(),
            );
            let output = bitmap.pixels_mut();
            if output.len() != svg.pixels.len() {
                return Err("image bitmap allocation failed".into());
            }
            output.copy_from_slice(&svg.pixels);
            return Ok(bitmap);
        }
        use ohos_image_native_binding::{DecodingOptions, ImageSource, types::PixelFormat};
        let source = ImageSource::create_from_data(&self.bytes).map_err(|e| e.to_string())?;
        let mut options = DecodingOptions::new().map_err(|e| e.to_string())?;
        options
            .set_pixel_format(PixelFormat::Rgba8888)
            .map_err(|e| e.to_string())?;
        let pixels = source
            .create_pixelmap(&mut options)
            .map_err(|e| e.to_string())?;
        let mut bitmap = ohos_drawing_binding::Bitmap::new(
            self.width,
            self.height,
            ohos_drawing_binding::BitmapFormat::default(),
        );
        let output = bitmap.pixels_mut();
        if output.len() != self.width as usize * self.height as usize * 4 {
            return Err("image bitmap allocation failed".into());
        }
        let read = pixels.read_pixels(output).map_err(|e| e.to_string())?;
        if read != output.len() {
            return Err("image decoder returned incomplete pixels".into());
        }
        Ok(bitmap)
    }
}
impl Loader {
    pub(crate) fn new(
        assets: Arc<dyn Assets>,
        access: Access,
        dispatcher: Weak<Dispatcher>,
    ) -> Arc<Self> {
        Arc::new(Self {
            assets,
            access,
            dispatcher,
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .max_redirects(0)
                .max_redirects_will_error(false)
                .timeout_connect(Some(Duration::from_secs(30)))
                .timeout_recv_response(Some(Duration::from_secs(60)))
                .build()
                .into(),
            cache: Mutex::default(),
            disk_cache: Mutex::default(),
            pending: Mutex::default(),
        })
    }
    pub(crate) fn set_disk_cache(&self, cache: Option<DiskCache>) {
        *self.disk_cache.lock().expect("disk cache") = cache;
    }
    pub(crate) fn load(
        self: &Arc<Self>,
        target: Target,
        uri: String,
        inline: Option<ImageData>,
    ) -> Arc<AtomicBool> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let request = {
            let mut pending = self.pending.lock().expect("pending images");
            if let Some(request) = pending.get(&uri) {
                match request.join(target, &cancelled) {
                    Ok(true) => return cancelled,
                    Ok(false) => {}
                    Err(error) => {
                        if let Some(dispatcher) = self.dispatcher.upgrade() {
                            dispatcher.image(Ready {
                                target,
                                data: Err(error.into()),
                            });
                        }
                        return cancelled;
                    }
                }
            }
            let request = Arc::new(crate::media::request::Request::new(
                target,
                cancelled.clone(),
            ));
            pending.insert(uri.clone(), request.clone());
            request
        };
        let loader = self.clone();
        let loading = request.clone();
        let key = uri.clone();
        if let Err(error) = crate::services::executor::Executor::images().and_then(|pool| {
            pool.submit(move || {
                let data = loader.read(&uri, inline, &loading);
                loader.finish(&uri, &loading, data);
            })
        }) {
            self.finish(&key, &request, Err(error.to_string()));
        }
        cancelled
    }
    fn finish(
        &self,
        uri: &str,
        request: &Arc<crate::media::request::Request<Target>>,
        data: std::result::Result<Arc<Data>, String>,
    ) {
        let mut pending = self.pending.lock().expect("pending images");
        if pending
            .get(uri)
            .is_some_and(|current| Arc::ptr_eq(current, request))
        {
            pending.remove(uri);
        }
        let waiters = std::mem::take(&mut *request.waiters.lock().expect("image waiters"));
        drop(pending);
        if let Some(dispatcher) = self.dispatcher.upgrade() {
            for waiter in waiters {
                if !waiter.cancelled.load(Ordering::Relaxed) {
                    dispatcher.image(Ready {
                        target: waiter.target,
                        data: data.clone(),
                    });
                }
            }
        }
    }
    fn read(
        &self,
        uri: &str,
        inline: Option<ImageData>,
        request: &crate::media::request::Request<Target>,
    ) -> std::result::Result<Arc<Data>, String> {
        {
            let mut cache = self.cache.lock().expect("image cache");
            cache.clock += 1;
            let clock = cache.clock;
            if let Some((data, used)) = cache.entries.get_mut(uri) {
                *used = clock;
                return Ok(data.clone());
            }
        }
        if inline.is_none() && !self.access.allows_media(uri) {
            return Err("image URL is not granted by capabilities.network".into());
        }
        if request.cancelled() {
            return Err("cancelled".into());
        }
        let bytes = if let Some(inline) = inline {
            if inline.len() > MAX_DATA {
                return Err("image exceeds 16 MiB".into());
            }
            inline.to_vec()
        } else if uri.starts_with("asset:") {
            self.assets.read(uri)?
        } else if uri.starts_with("data:") {
            let (_, bytes) = nsh_core::image::decode_data_uri(uri)?;
            if bytes.len() > MAX_DATA {
                return Err("image exceeds 16 MiB".into());
            }
            bytes
        } else if uri.starts_with("http://") || uri.starts_with("https://") {
            let disk_cache = self.disk_cache.lock().expect("disk cache").clone();
            match disk_cache {
                Some(cache) => cache
                    .load(uri, |headers| self.download(uri, headers, request))?
                    .to_vec(),
                None => {
                    let fetched = self.download(uri, &[], request)?;
                    if !(200..300).contains(&fetched.status) {
                        return Err(format!("image HTTP status {}", fetched.status));
                    }
                    fetched.body
                }
            }
        } else {
            return Err("images require asset:/, data: or HTTP(S) URIs".into());
        };
        if bytes.len() > MAX_DATA {
            return Err("image exceeds 16 MiB".into());
        }
        let (svg, width, height) = if nsh_core::svg::sniff(&bytes) {
            let raster = nsh_core::svg::rasterize_oversampled(&bytes).map_err(|e| e.to_string())?;
            let reported = nsh_core::svg::reported_size(raster.size);
            (
                Some(raster),
                reported.width.0 as u32,
                reported.height.0 as u32,
            )
        } else {
            let source = ohos_image_native_binding::ImageSource::create_from_data(&bytes)
                .map_err(|e| e.to_string())?;
            let info = source.image_info(0).map_err(|e| e.to_string())?;
            let (width, height) = (
                info.width().map_err(|e| e.to_string())?,
                info.height().map_err(|e| e.to_string())?,
            );
            let frames = u64::from(source.frame_count().unwrap_or(1).max(1));
            let frame_pixels = u64::from(width) * u64::from(height);
            if width == 0
                || height == 0
                || frame_pixels > 16_777_216
                || frame_pixels.saturating_mul(frames) > 67_108_864
            {
                return Err("image dimensions exceed 16 megapixels".into());
            }
            (None, width, height)
        };
        let data = Arc::new(Data {
            bytes: bytes.into(),
            svg,
            width,
            height,
        });
        let resident = data.resident_bytes();
        if resident <= CACHE / 2 {
            let mut cache = self.cache.lock().expect("image cache");
            cache.clock += 1;
            let clock = cache.clock;
            cache.bytes += resident;
            if let Some((old, _)) = cache.entries.insert(uri.to_owned(), (data.clone(), clock)) {
                cache.bytes -= old.resident_bytes();
            }
            while cache.bytes > CACHE {
                let key = cache
                    .entries
                    .iter()
                    .min_by_key(|(_, (_, used))| *used)
                    .map(|(key, _)| key.clone())
                    .expect("nonempty cache");
                let (old, _) = cache.entries.remove(&key).expect("cached image");
                cache.bytes -= old.resident_bytes();
            }
        }
        Ok(data)
    }
    fn download(
        &self,
        uri: &str,
        headers: &[(&'static str, String)],
        request: &crate::media::request::Request<Target>,
    ) -> std::result::Result<Fetched, String> {
        let mut url = uri.to_owned();
        let proxy = crate::services::net::SystemProxy::read().map_err(|error| error.to_string())?;
        let mut response = None;
        for hop in 0..=20 {
            if request.cancelled() {
                return Err("cancelled".into());
            }
            let mut req = self.agent.get(&url);
            for (name, value) in headers {
                req = req.header(*name, value);
            }
            let current = req
                .config()
                .proxy(proxy.value())
                .build()
                .call()
                .map_err(|e| e.to_string())?;
            let status = current.status().as_u16();
            if matches!(status, 301 | 302 | 303 | 307 | 308)
                && let Some(location) = current.headers().get("location")
            {
                if hop == 20 {
                    return Err("too many image redirects".into());
                }
                url = self
                    .access
                    .media_redirect(&url, location.to_str().map_err(|e| e.to_string())?)?;
            } else {
                response = Some(current);
                break;
            }
        }
        let mut response = response.ok_or("no image response")?;
        let status = response.status().as_u16();
        let cache_headers = CacheHeaders::from_lookup(|name| {
            let values: Vec<&str> = response
                .headers()
                .get_all(name)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .collect();
            (!values.is_empty()).then(|| values.join(", "))
        });
        if status >= 300 {
            return Ok(Fetched::new(status, cache_headers, Vec::new()));
        }
        let total = response
            .headers()
            .get("content-length")
            .and_then(|n| n.to_str().ok())
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or_default();
        if total > MAX_DATA as u64 {
            return Err("image exceeds 16 MiB".into());
        }
        let mut reader = response
            .body_mut()
            .with_config()
            .limit(MAX_DATA as u64 + 1)
            .reader();
        let mut bytes = Vec::new();
        let mut buffer = [0; 64 << 10];
        let mut progress = Instant::now();
        loop {
            if request.cancelled() {
                return Err("cancelled".into());
            }
            let size = reader.read(&mut buffer).map_err(|e| e.to_string())?;
            if size == 0 {
                break;
            }
            if bytes.len() + size > MAX_DATA {
                return Err("image exceeds 16 MiB".into());
            }
            bytes.extend_from_slice(&buffer[..size]);
            if progress.elapsed() >= Duration::from_millis(100) {
                if let Some(dispatcher) = self.dispatcher.upgrade() {
                    for waiter in request.waiters.lock().expect("image waiters").iter() {
                        if !waiter.cancelled.load(Ordering::Relaxed) {
                            dispatcher.image_progress(Progress {
                                target: waiter.target,
                                loaded: bytes.len() as u64,
                                total,
                            });
                        }
                    }
                }
                progress = Instant::now();
            }
        }
        Ok(Fetched::new(status, cache_headers, bytes))
    }
}
struct NativeImage {
    descriptor: ohos_arkui_binding::api::attribute_option::DrawableDescriptor,
    _pixels: Vec<ohos_image_native_binding::PixelMap>,
    _bytes: Arc<[u8]>,
}
impl NativeImage {
    fn new(data: &Data) -> std::result::Result<Self, String> {
        use ohos_image_native_binding::{
            DecodingOptions, ImageSource, PixelMap, PixelMapInitializationOptions,
            types::{PixelFormat, PixelMapAlphaType},
        };
        if let Some(svg) = &data.svg {
            let mut options = PixelMapInitializationOptions::new().map_err(|e| e.to_string())?;
            options.set_width(svg.width).map_err(|e| e.to_string())?;
            options.set_height(svg.height).map_err(|e| e.to_string())?;
            options
                .set_src_pixel_format(PixelFormat::Rgba8888)
                .map_err(|e| e.to_string())?;
            options
                .set_pixel_format(PixelFormat::Rgba8888)
                .map_err(|e| e.to_string())?;
            options
                .set_alpha_type(PixelMapAlphaType::Premultiplied)
                .map_err(|e| e.to_string())?;
            let mut raw_pixels = svg.pixels.clone();
            let pixels =
                PixelMap::create(&mut raw_pixels, &mut options).map_err(|e| e.to_string())?;
            let descriptor =
                ohos_arkui_binding::api::attribute_option::DrawableDescriptor::from_pixel_map(
                    pixels.handle(),
                )
                .map_err(|e| e.to_string())?;
            return Ok(Self {
                descriptor,
                _pixels: vec![pixels],
                _bytes: data.bytes.clone(),
            });
        }
        let source = ImageSource::create_from_data(&data.bytes).map_err(|e| e.to_string())?;
        let mut options = DecodingOptions::new().map_err(|e| e.to_string())?;
        let frame_count = source.frame_count().unwrap_or(1);
        if frame_count > 1 {
            let frames = source
                .create_pixelmap_list(&mut options)
                .map_err(|e| e.to_string())?;
            if !frames.is_empty() {
                let handles: Vec<_> = frames.iter().map(PixelMap::handle).collect();
                let mut descriptor =
                    ohos_arkui_binding::api::attribute_option::DrawableDescriptor::from_animated_pixel_map(
                        &handles,
                    )
                    .map_err(|e| e.to_string())?;
                let delays = source.delay_time_list().unwrap_or_default();
                let mut frame_durations: Vec<u32> = (0..frames.len())
                    .map(|idx| {
                        let d = delays.get(idx).copied().unwrap_or(100);
                        if d <= 10 { 100 } else { d as u32 }
                    })
                    .collect();
                let total_ms: i32 = frame_durations
                    .iter()
                    .map(|&d| d as i32)
                    .sum::<i32>()
                    .max(100);
                descriptor.set_animation_duration(total_ms);
                descriptor.set_animation_iteration(-1);
                #[cfg(feature = "api-23")]
                if crate::get_capabilities().api23 {
                    let _ = descriptor.set_animation_frame_durations(&mut frame_durations);
                    let _ = descriptor.set_animation_auto_play(1);
                }
                #[cfg(not(feature = "api-23"))]
                let _ = &mut frame_durations;
                return Ok(Self {
                    descriptor,
                    _pixels: frames,
                    _bytes: data.bytes.clone(),
                });
            }
        }
        let pixels = source
            .create_pixelmap(&mut options)
            .map_err(|e| e.to_string())?;
        let descriptor =
            ohos_arkui_binding::api::attribute_option::DrawableDescriptor::from_pixel_map(
                pixels.handle(),
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            descriptor,
            _pixels: vec![pixels],
            _bytes: data.bytes.clone(),
        })
    }
}
pub(crate) struct ImageView {
    decoded: RefCell<[Option<NativeImage>; 2]>,
    pub(crate) native: NativeNode,
    owner: UiWeak<Node>,
    loader: Arc<Loader>,
    generation: Cell<u64>,
    jobs: RefCell<Vec<Arc<AtomicBool>>>,
    primary: Cell<bool>,
    loaded: Cell<bool>,
    data: RefCell<Option<Arc<Data>>>,
    tint: Cell<Option<nsh_core::Color>>,
    radii: Cell<[f32; 4]>,
    blur: Cell<f32>,
    fade_ms: Cell<f32>,
    fade_start: Cell<Option<f64>>,
}
impl ImageView {
    pub(crate) fn new(owner: UiWeak<Node>, loader: Arc<Loader>) -> Result<Rc<Self>> {
        let native = Image::new()?.into_node();
        native
            .borrow()
            .set_length_metric_unit(LengthMetricUnit::Vp)?;
        native
            .borrow()
            .set_bool_attribute(A::ImageDraggable, false)?;
        native.borrow().set_bool_attribute(A::Clip, true)?;
        let image = Rc::new(Self {
            decoded: RefCell::new([None, None]),
            native,
            owner,
            loader,
            generation: Cell::new(0),
            jobs: RefCell::default(),
            primary: Cell::new(false),
            loaded: Cell::new(false),
            data: RefCell::default(),
            tint: Cell::new(None),
            radii: Cell::new([0.0; 4]),
            blur: Cell::new(0.0),
            fade_ms: Cell::new(0.0),
            fade_start: Cell::new(None),
        });
        let weak = Rc::downgrade(&image);
        image
            .native
            .borrow_mut()
            .on_event(E::ImageOnComplete, move |event| {
                callback(|| {
                    if let Some(image) = weak.upgrade()
                        && image.primary.get()
                        && event.component_i32(0) == Some(1)
                        && !image.loaded.replace(true)
                        && let Some(owner) = image.owner.upgrade()
                    {
                        if image.fade_ms.get() > 0.0 && !owner.platform.get().reduce_motion {
                            image.native.borrow().set_f32_attribute(A::Opacity, 0.0)?;
                            image.fade_start.set(Some(owner.animator.now()));
                            owner.animator.track(&owner);
                        }
                        if let Some(data) = image.data.borrow().as_ref() {
                            owner.emit(
                                EventKind::Load,
                                EventPayload::Image(nsh_core::Size::new(
                                    nsh_core::geometry::DevicePixels(data.width as i32),
                                    nsh_core::geometry::DevicePixels(data.height as i32),
                                )),
                            );
                        }
                        owner.emit(EventKind::LoadEnd, EventPayload::None);
                    }
                    Ok(())
                })
            });
        let weak = Rc::downgrade(&image);
        image
            .native
            .borrow_mut()
            .on_event(E::ImageOnError, move |event| {
                if let Some(image) = weak.upgrade()
                    && image.primary.get()
                {
                    image.failed(format!(
                        "native image error {}",
                        event.component_i32(0).unwrap_or_default()
                    ));
                }
            });
        if let Some(owner) = image.owner.upgrade() {
            owner.native.borrow_mut().add_child(image.native.clone())?;
        }
        Ok(image)
    }
    pub(crate) fn frame(&self, size: Size) -> Result<()> {
        self.native.borrow().width(size.width)?;
        self.native.borrow().height(size.height)?;
        Ok(())
    }
    pub(crate) fn style(&self, style: &nsh_core::ViewStyle) -> Result<()> {
        if self.tint.replace(style.tint_color) != style.tint_color {
            let native = self.native.borrow();
            if let Some(color) = style.tint_color {
                let (r, g, b, a) = color.to_f64();
                native.set_attribute(
                    A::ImageColorFilter,
                    vec![
                        0.0_f32, 0.0, 0.0, 0.0, r as f32, 0.0, 0.0, 0.0, 0.0, g as f32, 0.0, 0.0,
                        0.0, 0.0, b as f32, 0.0, 0.0, 0.0, a as f32, 0.0,
                    ]
                    .into(),
                )?;
            } else {
                native.reset_attribute(A::ImageColorFilter)?;
            }
        }
        let c = style.corner_radius;
        let r = style.border_radius.0;
        let radii =
            [c.top_left, c.top_right, c.bottom_left, c.bottom_right].map(|v| v.map_or(r, |p| p.0));
        if self.radii.replace(radii) != radii {
            self.native
                .borrow()
                .set_attribute(A::BorderRadius, radii.to_vec().into())?;
        }
        Ok(())
    }
    pub(crate) fn tick(&self, now: f64) -> Result<bool> {
        let Some(start) = self.fade_start.get() else {
            return Ok(false);
        };
        let reduced = self
            .owner
            .upgrade()
            .is_none_or(|n| n.platform.get().reduce_motion);
        let progress = if reduced {
            1.0
        } else {
            ((now - start) / self.fade_ms.get() as f64).clamp(0.0, 1.0) as f32
        };
        self.native
            .borrow()
            .set_f32_attribute(A::Opacity, progress)?;
        if progress >= 1.0 {
            self.fade_start.set(None);
        }
        Ok(progress < 1.0)
    }
    pub(crate) fn configure(
        &self,
        source: Option<ImageSource>,
        mode: ResizeMode,
        options: ImageOptions,
    ) -> Result<()> {
        self.cancel();
        self.fade_start.set(None);
        self.fade_ms.set(options.fade_ms.unwrap_or(0.0).max(0.0));
        self.native.borrow().set_f32_attribute(A::Opacity, 1.0)?;
        if self.blur.replace(options.blur_radius.0) != options.blur_radius.0 {
            self.native.borrow().set_f32_attribute(
                A::Blur,
                crate::render::composition::geometry::blur_sigma(
                    options.blur_radius.0,
                    self.owner.upgrade().map_or(1.0, |node| node.scale),
                ),
            )?;
        }
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.primary.set(false);
        self.loaded.set(false);
        self.data.borrow_mut().take();
        self.native.borrow().reset_attribute(A::ImageSrc)?;
        self.native.borrow().reset_attribute(A::ImageAlt)?;
        *self.decoded.borrow_mut() = [None, None];
        self.native.borrow().set_i32_attribute(
            A::ImageObjectFit,
            match mode {
                ResizeMode::Contain => 0,   // ARKUI_OBJECT_FIT_CONTAIN
                ResizeMode::Cover => 1,     // ARKUI_OBJECT_FIT_COVER
                ResizeMode::Stretch => 3,   // ARKUI_OBJECT_FIT_FILL
                ResizeMode::ScaleDown => 4, // ARKUI_OBJECT_FIT_SCALE_DOWN
                ResizeMode::Center => 10,   // ARKUI_OBJECT_FIT_NONE_AND_ALIGN_CENTER
            },
        )?;
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        if let Some(source) = source {
            owner.emit(EventKind::LoadStart, EventPayload::None);
            for (source, placeholder) in [(Some(source), false), (options.placeholder, true)] {
                if let Some(source) = source {
                    self.jobs.borrow_mut().push(self.loader.load(
                        Target::View {
                            id: owner.id,
                            generation,
                            placeholder,
                        },
                        source.uri.to_string(),
                        source.data,
                    ));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn ready(
        &self,
        generation: u64,
        placeholder: bool,
        result: std::result::Result<Arc<Data>, String>,
    ) -> Result<()> {
        if generation != self.generation.get() {
            return Ok(());
        }
        match result {
            Ok(data) => {
                let decoded = match NativeImage::new(&data) {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        if !placeholder {
                            self.failed(error);
                        }
                        return Ok(());
                    }
                };
                if placeholder {
                    self.native
                        .borrow()
                        .set_attribute(A::ImageAlt, (&decoded.descriptor).into())?;
                } else {
                    self.primary.set(true);
                    *self.data.borrow_mut() = Some(data.clone());
                    self.native
                        .borrow()
                        .set_attribute(A::ImageSrc, (&decoded.descriptor).into())?;
                }
                self.decoded.borrow_mut()[usize::from(placeholder)] = Some(decoded);
            }
            Err(message) if !placeholder => self.failed(message),
            Err(_) => {}
        }
        Ok(())
    }
    pub(crate) fn progress(&self, generation: u64, loaded: u64, total: u64) {
        if generation == self.generation.get()
            && let Some(owner) = self.owner.upgrade()
        {
            owner.emit(
                EventKind::Progress,
                EventPayload::Map(vec![
                    ("loaded", PayloadValue::Number(loaded as f64)),
                    ("total", PayloadValue::Number(total as f64)),
                ]),
            );
        }
    }
    fn failed(&self, message: String) {
        if !self.loaded.replace(true)
            && let Some(owner) = self.owner.upgrade()
        {
            owner.emit(EventKind::Error, EventPayload::Error { message });
            owner.emit(EventKind::LoadEnd, EventPayload::None);
        }
    }
    fn cancel(&self) {
        for job in self.jobs.borrow_mut().drain(..) {
            job.store(true, Ordering::Relaxed);
        }
    }
}
impl Drop for ImageView {
    fn drop(&mut self) {
        self.cancel();
        let result = (|| -> Result<()> {
            if self.native.borrow().is_owned() {
                self.native.borrow().reset_attribute(A::ImageSrc)?;
                self.native.borrow().reset_attribute(A::ImageAlt)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            log::error!("OHOS image retirement: {error}; retaining descriptors");
            std::mem::forget(self.decoded.replace([None, None]));
        }
    }
}
pub(crate) fn info(
    control: &nsh_engine::EngineControl,
    id: u32,
    result: std::result::Result<Arc<Data>, String>,
) {
    let mut values = vec![("id", PayloadValue::Number(id as f64))];
    match result {
        Ok(data) => {
            values.push(("width", PayloadValue::Number(data.width as f64)));
            values.push(("height", PayloadValue::Number(data.height as f64)));
        }
        Err(error) => values.push(("error", PayloadValue::Str(error))),
    }
    control.dispatch_event(
        nsh_core::ROOT_ID,
        EventKind::ImageInfo,
        EventPayload::Map(values),
    );
}
