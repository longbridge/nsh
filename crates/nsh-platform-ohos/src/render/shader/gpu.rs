//! The shader thread's GPU: one wgpu device on GLES (the backend wgpu has on
//! OHOS) and the views it renders straight into their XComponents' native
//! windows. Nothing is read back: a frame is presented to the window's
//! buffer queue, which RenderService composites as it does a video's.
//!
//! The device, pipelines and bindings follow `nsh_engine::shader`'s (its
//! renderer only draws into textures it reads back); the shader programs,
//! built-ins and slots are `nsh_core::shader`'s, so a frame here is the one
//! the other platforms show.

use std::{
    borrow::Cow,
    ptr::NonNull,
    sync::Arc,
    time::{Duration, Instant},
};

use nsh_core::{
    animation::AnimationValue,
    geometry::{Points, Size},
    shader::{ShaderLayer, SlotKind, builtins},
};
use ohos_native_window_binding::NativeWindow;
use rustc_hash::FxHashMap;
use wgpu::rwh;

/// OHOS's display: there is one, and its handle holds nothing.
#[derive(Debug)]
struct Display;

impl rwh::HasDisplayHandle for Display {
    fn display_handle(&self) -> Result<rwh::DisplayHandle<'_>, rwh::HandleError> {
        Ok(rwh::DisplayHandle::ohos())
    }
}

/// The longest side of a frame, in pixels (as `nsh_engine::shader`'s).
const MAX_SIDE: u32 = 4096;

/// The device and what every view's pipeline shares.
pub(super) struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// The built-ins alone, and with the uniforms.
    layouts: [wgpu::BindGroupLayout; 2],
    pipeline_layouts: [wgpu::PipelineLayout; 2],
    /// By the hash of the shader's source and the target's format; `None`
    /// for one the device refused (reported once).
    pipelines: FxHashMap<(u64, wgpu::TextureFormat), Option<Arc<wgpu::RenderPipeline>>>,
}

impl Gpu {
    /// Opens the device (blocking, once); `None` when there is none, which
    /// the log says.
    pub(super) fn new() -> Option<Gpu> {
        // wgpu makes surfaces only on an instance that knows the display.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(Display))
        });
        let adapter = match pollster::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        ) {
            Ok(adapter) => adapter,
            Err(error) => {
                log::warn!("shaders: no GPU adapter ({error}); shader views draw nothing");
                return None;
            }
        };
        let (device, queue) =
            match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("nsh shaders"),
                required_limits:
                    wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            })) {
                Ok(device) => device,
                Err(error) => {
                    log::warn!("shaders: no GPU device ({error}); shader views draw nothing");
                    return None;
                }
            };
        // What validation missed is a warning, never a panic of the app.
        device.on_uncaptured_error(Arc::new(|error: wgpu::Error| {
            log::warn!("shaders: {error}");
        }));
        log::debug!("shaders: {:?}", adapter.get_info());
        let entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layouts = [
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("nsh built-ins"),
                entries: &[entry(0)],
            }),
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("nsh built-ins and uniforms"),
                entries: &[entry(0), entry(1)],
            }),
        ];
        let pipeline_layouts = [0, 1].map(|i| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layouts[i])],
                immediate_size: 0,
            })
        });
        Some(Gpu {
            instance,
            adapter,
            device,
            queue,
            layouts,
            pipeline_layouts,
            pipelines: FxHashMap::default(),
        })
    }

    fn pipeline(
        &mut self,
        layer: &ShaderLayer,
        format: wgpu::TextureFormat,
    ) -> Option<Arc<wgpu::RenderPipeline>> {
        let shader = &layer.shader;
        let key = (shader.hash(), format);
        if let Some(pipeline) = self.pipelines.get(&key) {
            return pipeline.clone();
        }
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("nsh shader"),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader.wgsl())),
            });
        let uniforms = usize::from(shader.uniform_size() > 0);
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("nsh shader"),
                layout: Some(&self.pipeline_layouts[uniforms]),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("nsh_vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("nsh_fragment"),
                    compilation_options: Default::default(),
                    // The program's colors are premultiplied: drawn as they are.
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        let pipeline = match pollster::block_on(scope.pop()) {
            Some(error) => {
                log::warn!(target: "js", "a shader the GPU refused: {error}");
                None
            }
            None => Some(Arc::new(pipeline)),
        };
        self.pipelines.insert(key, pipeline.clone());
        pipeline
    }

    fn bindings(&self, size: u32) -> Bindings {
        let buffer = |size: u32| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("nsh shader uniforms"),
                size: u64::from(size),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let builtins = buffer(16);
        let uniforms = (size > 0).then(|| buffer(size));
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: builtins.as_entire_binding(),
        }];
        if let Some(uniforms) = &uniforms {
            entries.push(wgpu::BindGroupEntry {
                binding: 1,
                resource: uniforms.as_entire_binding(),
            });
        }
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layouts[usize::from(uniforms.is_some())],
            entries: &entries,
        });
        Bindings {
            builtins,
            uniforms,
            group,
            size,
        }
    }

    /// The surface of `window`, presented as RenderService composites it;
    /// `None` (logged) where the device cannot draw into it.
    fn output(&self, window: NativeWindow) -> Option<Output> {
        let handle = rwh::OhosNdkWindowHandle::new(NonNull::new(window.raw().cast())?);
        // SAFETY: `window` is retained in the same `Output`, after the
        // surface, so it outlives the surface made from it.
        let surface = unsafe {
            self.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(rwh::RawDisplayHandle::Ohos(
                        rwh::OhosDisplayHandle::new(),
                    )),
                    raw_window_handle: rwh::RawWindowHandle::OhosNdk(handle),
                })
        };
        let surface = match surface {
            Ok(surface) => surface,
            Err(error) => {
                log::warn!("shaders: no surface for a shader view ({error})");
                return None;
            }
        };
        let caps = surface.get_capabilities(&self.adapter);
        // Unorm, as the other platforms' frames: the program writes the
        // colors it means (sRGB already).
        let format = [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8Unorm,
        ]
        .into_iter()
        .find(|f| caps.formats.contains(f))
        .or_else(|| caps.formats.first().copied());
        let Some(format) = format else {
            log::warn!("shaders: the GPU cannot draw into a shader view's surface");
            return None;
        };
        let alpha = [
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
        ]
        .into_iter()
        .find(|a| caps.alpha_modes.contains(a))
        .or_else(|| caps.alpha_modes.first().copied())
        .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        Some(Output {
            surface,
            window,
            format,
            alpha,
            configured: None,
        })
    }
}

/// The buffers of a view's built-ins and uniforms and their bind group.
struct Bindings {
    builtins: wgpu::Buffer,
    uniforms: Option<wgpu::Buffer>,
    group: wgpu::BindGroup,
    /// The uniforms' size it was made for.
    size: u32,
}

/// Where a view's frames go. The surface is dropped before the window it
/// was made from (field order).
struct Output {
    surface: wgpu::Surface<'static>,
    window: NativeWindow,
    format: wgpu::TextureFormat,
    alpha: wgpu::CompositeAlphaMode,
    /// The size in pixels it was configured at.
    configured: Option<(u32, u32)>,
}

/// The clock of `nsh.time`: seconds while it runs, kept while it is paused.
#[derive(Default)]
struct Clock {
    since: Option<Instant>,
    kept: Duration,
}

impl Clock {
    fn time(&self, now: Instant) -> f32 {
        let running = self
            .since
            .map_or(Duration::ZERO, |since| now.saturating_duration_since(since));
        (self.kept + running).as_secs_f32()
    }
}

/// One shader view on the shader thread.
#[derive(Default)]
pub(super) struct View {
    layer: Option<Arc<ShaderLayer>>,
    /// The uniforms with the animated slots written in.
    words: Vec<u32>,
    size: Size<Points>,
    scale: f32,
    clock: Clock,
    /// Something it reads changed since its last frame.
    dirty: bool,
    /// In sight: out of it, it renders nothing.
    pub(super) shown: bool,
    bindings: Option<Bindings>,
    output: Option<Output>,
}

impl View {
    /// Shows `layer`; the animated slots keep their values while the
    /// program stays (as `nsh_engine::shader::ShaderView::set_layer`).
    pub(super) fn set_layer(&mut self, layer: Arc<ShaderLayer>) {
        let same = self
            .layer
            .as_ref()
            .is_some_and(|old| old.shader == layer.shader && old.slots == layer.slots);
        let mut words = layer.uniforms.to_vec();
        if same {
            for slot in &layer.slots {
                let n = match slot.kind {
                    SlotKind::Rgb => 3,
                    SlotKind::Rgba => 4,
                    _ => 1,
                };
                let at = slot.word as usize;
                if let (Some(dst), Some(src)) =
                    (words.get_mut(at..at + n), self.words.get(at..at + n))
                {
                    dst.copy_from_slice(src);
                }
            }
        } else {
            self.bindings = None;
        }
        self.words = words;
        let now = Instant::now();
        match (layer.paused, self.clock.since) {
            (true, Some(since)) => {
                self.clock.kept += now.saturating_duration_since(since);
                self.clock.since = None;
            }
            (false, None) => self.clock.since = Some(now),
            _ => {}
        }
        self.layer = Some(layer);
        self.dirty = true;
    }

    pub(super) fn set_slot(&mut self, slot: u16, value: AnimationValue) {
        if let Some(layer) = &self.layer {
            let before = self.words.clone();
            layer.write_slot(&mut self.words, slot, value);
            self.dirty |= before != self.words;
        }
    }

    pub(super) fn set_size(&mut self, size: Size<Points>, scale: f32) {
        if self.size != size || self.scale != scale {
            self.size = size;
            self.scale = scale;
            self.dirty = true;
        }
    }

    /// Draws into `window` from now on (`None`: its surface went).
    pub(super) fn set_window(&mut self, gpu: &Gpu, window: Option<NativeWindow>) {
        self.output = window.and_then(|window| gpu.output(window));
        self.dirty = true;
    }

    /// The native window it draws into (to tell an old one's end from a new
    /// one's).
    pub(super) fn window(&self) -> Option<usize> {
        self.output.as_ref().map(|o| o.window.raw() as usize)
    }

    /// Whether a frame is due: in sight, with somewhere to draw, and
    /// something changed or its time runs.
    pub(super) fn due(&self) -> bool {
        self.shown
            && self.output.is_some()
            && self
                .layer
                .as_ref()
                .is_some_and(|l| self.dirty || (l.shader.animated() && !l.paused))
    }

    /// The frame's size in pixels.
    fn pixels(&self) -> (u32, u32) {
        let (w, h) = (self.size.width.0.max(0.0), self.size.height.0.max(0.0));
        let mut scale = self.scale.max(0.0);
        let side = w.max(h) * scale;
        if side > MAX_SIDE as f32 {
            scale *= MAX_SIDE as f32 / side;
        }
        ((w * scale).round() as u32, (h * scale).round() as u32)
    }

    /// Renders a frame and presents it to the view's window.
    pub(super) fn render(&mut self, gpu: &mut Gpu, now: Instant) {
        let Some(layer) = self.layer.clone() else {
            return;
        };
        let (width, height) = self.pixels();
        let Some(output) = self.output.as_mut() else {
            return;
        };
        if width == 0 || height == 0 {
            self.dirty = false;
            return;
        }
        let Some(pipeline) = gpu.pipeline(&layer, output.format) else {
            self.dirty = false;
            return;
        };
        if output.configured != Some((width, height)) {
            // The window's buffers at the frame's size: RenderService scales
            // them to the XComponent, which is the view's box.
            if let Err(error) = output
                .window
                .set_buffer_geometry(width as i32, height as i32)
            {
                log::warn!("shaders: a shader view's buffers: {error:?}");
            }
            output.surface.configure(
                &gpu.device,
                &wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: output.format,
                    width,
                    height,
                    present_mode: wgpu::PresentMode::Fifo,
                    desired_maximum_frame_latency: 2,
                    alpha_mode: output.alpha,
                    view_formats: vec![],
                },
            );
            output.configured = Some((width, height));
        }
        let texture = match output.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                output.configured = None;
                texture
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                // Configured again at the next frame.
                output.configured = None;
                return;
            }
            _ => return,
        };
        let size = layer.shader.uniform_size();
        if self.bindings.as_ref().is_none_or(|b| b.size != size) {
            self.bindings = Some(gpu.bindings(size));
        }
        let bindings = self.bindings.as_ref().expect("bindings");
        let scale = width as f32 / self.size.width.0.max(f32::MIN_POSITIVE);
        gpu.queue.write_buffer(
            &bindings.builtins,
            0,
            &words_bytes(&builtins(self.size, self.clock.time(now), scale)),
        );
        if let Some(uniforms) = &bindings.uniforms {
            gpu.queue
                .write_buffer(uniforms, 0, &words_bytes(&self.words));
        }
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nsh shader"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bindings.group, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
        texture.present();
        self.dirty = false;
    }
}

fn words_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_ne_bytes()).collect()
}
