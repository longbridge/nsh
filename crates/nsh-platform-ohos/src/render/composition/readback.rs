//! NativeImage texture consumption through statically linked community EGL/GLES bindings.
use std::{marker::PhantomData, rc::Rc};

use glow::HasContext;

// GL_OES_EGL_image_external is accepted by glow but has no exported enum.
const TEXTURE_EXTERNAL_OES: u32 = 0x8d65;
use khronos_egl as egl;
use ohos_native_image_binding::{BufferUsage, TextureImage};

use self::layout::{Layout, validate_size};
use super::frame_wake::FrameWake;

fn failed(operation: &'static str) -> Error {
    Error {
        operation,
        code: -1,
    }
}
fn egl_error(error: egl::Error) -> Error {
    Error {
        operation: "EGL",
        code: error as i32,
    }
}

struct Context {
    egl: egl::Instance<egl::Static>,
    display: egl::Display,
    context: egl::Context,
    surface: egl::Surface,
    // EGL contexts must remain on their creating thread, even if glow is Send.
    _thread: PhantomData<Rc<()>>,
}
struct Current<'a> {
    context: &'a Context,
    previous: (
        Option<egl::Display>,
        Option<egl::Context>,
        Option<egl::Surface>,
        Option<egl::Surface>,
    ),
    previous_api: egl::Enum,
}
struct ClientApi<'a> {
    egl: &'a egl::Instance<egl::Static>,
    previous: egl::Enum,
    active: bool,
}
impl ClientApi<'_> {
    fn restore(&mut self) -> Result<(), Error> {
        self.egl.bind_api(self.previous).map_err(egl_error)?;
        self.active = false;
        Ok(())
    }
}
impl Drop for ClientApi<'_> {
    fn drop(&mut self) {
        if self.active {
            let _ = self.egl.bind_api(self.previous);
        }
    }
}
impl Drop for Current<'_> {
    fn drop(&mut self) {
        let (display, context, draw, read) = self.previous;
        let _ = self.context.egl.make_current(
            display.unwrap_or(self.context.display),
            draw,
            read,
            context,
        );
        let _ = self.context.egl.bind_api(self.previous_api);
    }
}
impl Context {
    fn new() -> Result<Self, Error> {
        let egl = egl::Instance::new(egl::Static);
        let previous = egl.query_api();
        egl.bind_api(egl::OPENGL_ES_API).map_err(egl_error)?;
        let mut client_api = ClientApi {
            egl: &egl,
            previous,
            active: true,
        };
        // SAFETY: EGL_DEFAULT_DISPLAY is the platform's null display sentinel.
        let display =
            unsafe { egl.get_display(egl::DEFAULT_DISPLAY) }.ok_or(failed("EGL display"))?;
        egl.initialize(display).map_err(egl_error)?;
        let config = egl
            .choose_first_config(
                display,
                &[
                    egl::SURFACE_TYPE,
                    egl::PBUFFER_BIT,
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES2_BIT,
                    egl::RED_SIZE,
                    8,
                    egl::GREEN_SIZE,
                    8,
                    egl::BLUE_SIZE,
                    8,
                    egl::ALPHA_SIZE,
                    8,
                    egl::NONE,
                ],
            )
            .map_err(egl_error)?
            .ok_or(failed("EGL RGBA config"))?;
        let context = egl
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE],
            )
            .map_err(egl_error)?;
        let surface = match egl.create_pbuffer_surface(
            display,
            config,
            &[egl::WIDTH, 1, egl::HEIGHT, 1, egl::NONE],
        ) {
            Ok(surface) => surface,
            Err(error) => {
                let _ = egl.destroy_context(display, context);
                return Err(egl_error(error));
            }
        };
        if let Err(error) = client_api.restore() {
            let _ = egl.destroy_surface(display, surface);
            let _ = egl.destroy_context(display, context);
            return Err(error);
        }
        drop(client_api);
        Ok(Self {
            egl,
            display,
            context,
            surface,
            _thread: PhantomData,
        })
    }
    fn enter(&self) -> Result<Current<'_>, Error> {
        let previous_api = self.egl.query_api();
        self.egl.bind_api(egl::OPENGL_ES_API).map_err(egl_error)?;
        let mut client_api = ClientApi {
            egl: &self.egl,
            previous: previous_api,
            active: true,
        };
        // EGL current state is per client API. Preserve the ES context before
        // replacing it, then restore the caller's selected API on exit.
        let previous = (
            self.egl.get_current_display(),
            self.egl.get_current_context(),
            self.egl.get_current_surface(egl::DRAW),
            self.egl.get_current_surface(egl::READ),
        );
        self.egl
            .make_current(
                self.display,
                Some(self.surface),
                Some(self.surface),
                Some(self.context),
            )
            .map_err(egl_error)?;
        client_api.active = false;
        Ok(Current {
            context: self,
            previous,
            previous_api,
        })
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        let _ = self.egl.destroy_surface(self.display, self.surface);
        let _ = self.egl.destroy_context(self.display, self.context);
        // The default EGL display is shared with the embedding app. Terminating
        // it here would invalidate that app's independent rendering contexts.
    }
}

/// A thread-bound offscreen context shared by NativeImage readers on one worker.
pub(crate) struct TextureContext {
    context: Rc<Context>,
    gl: Rc<glow::Context>,
    max_texture_size: u32,
}
impl TextureContext {
    /// Creates an offscreen GLES context on the calling rendering worker.
    pub(super) fn new() -> Result<Self, Error> {
        let context = Rc::new(Context::new()?);
        let current = context.enter()?;
        // SAFETY: the owned context is current. EGL supplies standard driver
        // entry points for glow; native API availability remains feature-gated.
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                context
                    .egl
                    .get_proc_address(name)
                    .map_or(std::ptr::null(), |function| function as *const _)
            })
        };
        // SAFETY: the query runs in the current, owned GLES context.
        let max_texture_size = unsafe { gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) };
        let max_texture_size = u32::try_from(max_texture_size)
            .ok()
            .filter(|limit| *limit > 0)
            .ok_or(failed("GLES texture limit"))?;
        drop(current);
        Ok(Self {
            context,
            gl: Rc::new(gl),
            max_texture_size,
        })
    }
}
struct Objects {
    context: Rc<Context>,
    gl: Rc<glow::Context>,
    texture: Option<glow::Texture>,
    target: Option<glow::Texture>,
    framebuffer: Option<glow::Framebuffer>,
    program: Option<glow::Program>,
    vertices: Option<glow::Buffer>,
}
impl Drop for Objects {
    fn drop(&mut self) {
        if self.texture.is_none() {
            return;
        }
        if let Ok(_current) = self.context.enter() {
            // SAFETY: partial construction retains each allocated object in its creating context.
            unsafe {
                if let Some(object) = self.vertices {
                    self.gl.delete_buffer(object);
                }
                if let Some(object) = self.program {
                    self.gl.delete_program(object);
                }
                if let Some(object) = self.framebuffer {
                    self.gl.delete_framebuffer(object);
                }
                if let Some(object) = self.target {
                    self.gl.delete_texture(object);
                }
                if let Some(object) = self.texture {
                    self.gl.delete_texture(object);
                }
            }
        }
    }
}
/// A thread-bound NativeImage reader with an offscreen GLES context.
///
/// GPU sampling resolves the producer's texture layout and transform before
/// copying tightly packed RGBA pixels. This supports GPU-only buffer layouts
/// for which a CPU NativeBuffer mapping does not expose linear image pixels.
/// Create and use this owner on a rendering worker, never a UI callback.
pub(crate) struct TextureReader {
    surface: Option<TextureImage>,
    wake: Option<FrameWake>,
    gl: Rc<glow::Context>,
    context: Rc<Context>,
    texture: glow::Texture,
    target: glow::Texture,
    framebuffer: glow::Framebuffer,
    program: glow::Program,
    vertices: glow::Buffer,
    size: (u32, u32),
    allocated: (u32, u32),
    max_texture_size: u32,
}
impl TextureReader {
    /// Creates another consumer on the same rendering worker and GLES context.
    pub(super) fn with_context(
        gpu: &TextureContext,
        callback: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let context = gpu.context.clone();
        let current = context.enter()?;
        let gl = gpu.gl.clone();
        let mut objects = Objects {
            context: context.clone(),
            gl: gl.clone(),
            texture: None,
            target: None,
            framebuffer: None,
            program: None,
            vertices: None,
        };
        // SAFETY: all objects belong to the current owned context. The construction
        // guard deletes every allocated object if any later step fails.
        let (texture, target, framebuffer, program, vertices) = unsafe {
            let texture = gl
                .create_texture()
                .map_err(|_| failed("external texture"))?;
            objects.texture = Some(texture);
            gl.bind_texture(TEXTURE_EXTERNAL_OES, Some(texture));
            for parameter in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
                gl.tex_parameter_i32(TEXTURE_EXTERNAL_OES, parameter, glow::LINEAR as i32);
            }
            for parameter in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T] {
                gl.tex_parameter_i32(TEXTURE_EXTERNAL_OES, parameter, glow::CLAMP_TO_EDGE as i32);
            }
            let target = gl
                .create_texture()
                .map_err(|_| failed("readback texture"))?;
            objects.target = Some(target);
            let framebuffer = gl
                .create_framebuffer()
                .map_err(|_| failed("readback framebuffer"))?;
            objects.framebuffer = Some(framebuffer);
            let program = gl
                .create_program()
                .map_err(|_| failed("readback program"))?;
            objects.program = Some(program);
            for (kind, source) in [
                (
                    glow::VERTEX_SHADER,
                    "attribute vec2 position; uniform mat4 transform; varying vec2 uv; void main() { gl_Position = vec4(position, 0.0, 1.0); uv = (transform * vec4(position * 0.5 + 0.5, 0.0, 1.0)).xy; }",
                ),
                (
                    glow::FRAGMENT_SHADER,
                    "#extension GL_OES_EGL_image_external : require\nprecision mediump float; varying vec2 uv; uniform samplerExternalOES image; void main() { gl_FragColor = texture2D(image, uv); }",
                ),
            ] {
                let shader = gl
                    .create_shader(kind)
                    .map_err(|_| failed("readback shader"))?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                let compiled = gl.get_shader_compile_status(shader);
                if compiled {
                    gl.attach_shader(program, shader);
                }
                gl.delete_shader(shader);
                if !compiled {
                    return Err(failed("compile readback shader"));
                }
            }
            gl.bind_attrib_location(program, 0, "position");
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(failed("link readback program"));
            }
            let vertices = gl
                .create_buffer()
                .map_err(|_| failed("readback vertices"))?;
            objects.vertices = Some(vertices);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices));
            let points: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
            let bytes =
                std::slice::from_raw_parts(points.as_ptr().cast(), std::mem::size_of_val(&points));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
            (texture, target, framebuffer, program, vertices)
        };
        // SAFETY: this worker restores this same context for updates and Drop;
        // the image is always destroyed before its external texture.
        let mut surface = unsafe { TextureImage::new(texture.0.get(), TEXTURE_EXTERNAL_OES) }
            .map_err(native_error)?;
        surface
            .set_usage(BufferUsage::HW_RENDER | BufferUsage::HW_TEXTURE)
            .map_err(native_error)?;
        let wake =
            FrameWake::new(callback).ok_or(failed("frame notification identity exhausted"))?;
        // SAFETY: context is a never-reused integer token, never dereferenced.
        // Late native calls perform a weak lookup and return after token retirement.
        unsafe { surface.set_frame_available_listener(wake.listener()) }.map_err(native_error)?;
        objects.texture = None;
        objects.target = None;
        objects.framebuffer = None;
        objects.program = None;
        objects.vertices = None;
        drop(current);
        Ok(Self {
            surface: Some(surface),
            wake: Some(wake),
            gl,
            context,
            texture,
            target,
            framebuffer,
            program,
            vertices,
            size: (0, 0),
            allocated: (0, 0),
            max_texture_size: gpu.max_texture_size,
        })
    }
    /// Identity accepted by ArkUI's texture rendering options.
    pub(super) fn id(&mut self) -> Result<u64, Error> {
        self.surface
            .as_mut()
            .unwrap()
            .surface_id()
            .map_err(native_error)
    }
    /// Updates future dimensions within native signed and GLES texture limits.
    pub(super) fn resize(&mut self, width: u32, height: u32) -> Result<(), Error> {
        self.layout(width, height)?;
        self.surface
            .as_mut()
            .unwrap()
            .resize(width, height)
            .map_err(native_error)?;
        self.size = (width, height);
        Ok(())
    }
    fn layout(&self, width: u32, height: u32) -> Result<Layout, Error> {
        let layout = validate_size(width, height)?;
        if width > self.max_texture_size || height > self.max_texture_size {
            return Err(failed("GLES texture dimensions"));
        }
        Ok(layout)
    }
    /// Consume one pending frame and resolve its layout into linear RGBA.
    /// The system waits for the acquire fence on this rendering worker.
    pub(super) fn read(&mut self) -> Result<Option<RgbaFrame>, Error> {
        let _current = self.context.enter()?;
        let (width, height) = self.size;
        let layout = self.layout(width, height)?;
        let surface = self.surface.as_mut().unwrap();
        if !surface.update().map_err(native_error)? {
            return Ok(None);
        }
        let transform = surface.transform().map_err(native_error)?;
        let mut pixels = layout.allocate()?;
        // SAFETY: the context and every GL object are live on this thread;
        // storage and the readback slice have exactly width*height*4 bytes.
        unsafe {
            let gl = &self.gl;
            gl.bind_texture(glow::TEXTURE_2D, Some(self.target));
            if self.allocated != (width, height) {
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA as i32,
                    width as i32,
                    height as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(self.target),
                0,
            );
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                return Err(failed("incomplete readback framebuffer"));
            }
            gl.viewport(0, 0, width as i32, height as i32);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(
                gl.get_uniform_location(self.program, "transform").as_ref(),
                false,
                &transform,
            );
            gl.uniform_1_i32(gl.get_uniform_location(self.program, "image").as_ref(), 0);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(TEXTURE_EXTERNAL_OES, Some(self.texture));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertices));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
            let code = gl.get_error();
            if code != glow::NO_ERROR {
                return Err(Error {
                    operation: "GLES readback",
                    code: code as i32,
                });
            }
        }
        self.allocated = (width, height);
        // GLES readback starts with the bottom row; consumers use top-left RGBA.
        let stride = layout.row_bytes;
        for y in 0..height as usize / 2 {
            let (top, bottom) = pixels.split_at_mut((height as usize - 1 - y) * stride);
            top[y * stride..(y + 1) * stride].swap_with_slice(&mut bottom[..stride]);
        }
        Ok(Some(RgbaFrame {
            width,
            height,
            pixels,
        }))
    }
}
impl Drop for TextureReader {
    fn drop(&mut self) {
        // Retire delivery before native teardown; late native callbacks only miss lookup.
        self.wake.take();
        if let Ok(_current) = self.context.enter() {
            self.surface.take();
            // SAFETY: native image has retired before its texture; all objects
            // are deleted on their creating thread in their current context.
            unsafe {
                self.gl.delete_buffer(self.vertices);
                self.gl.delete_program(self.program);
                self.gl.delete_framebuffer(self.framebuffer);
                self.gl.delete_texture(self.target);
                self.gl.delete_texture(self.texture);
            }
        } else if let Some(surface) = self.surface.take() {
            // The SDK image belongs to its creating EGL context. If entering
            // fails, retain that complete owner rather than destroy it in an
            // unrelated context. Retire Rust delivery before quarantining it.
            std::mem::forget((surface, self.context.clone(), self.gl.clone()));
        }
    }
}

#[derive(Debug)]
pub(crate) struct Error {
    operation: &'static str,
    code: i32,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} failed ({})", self.operation, self.code)
    }
}
impl std::error::Error for Error {}
fn native_error(error: ohos_native_image_binding::Error) -> Error {
    Error {
        operation: "NativeImage",
        code: error.code(),
    }
}
pub(crate) struct RgbaFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}
mod layout {
    use super::{Error, failed};
    pub(super) struct Layout {
        pub(super) row_bytes: usize,
        byte_len: usize,
    }
    pub(super) fn validate_size(width: u32, height: u32) -> Result<Layout, Error> {
        if width == 0
            || height == 0
            || u64::from(width) * u64::from(height) > 16_777_216
            || width > i32::MAX as u32
            || height > i32::MAX as u32
        {
            return Err(failed("NSH layer pixel budget"));
        }
        let row_bytes = (width as usize).checked_mul(4).ok_or(failed("RGBA row"))?;
        let byte_len = row_bytes
            .checked_mul(height as usize)
            .filter(|n| *n <= isize::MAX as usize)
            .ok_or(failed("RGBA bytes"))?;
        Ok(Layout {
            row_bytes,
            byte_len,
        })
    }
    impl Layout {
        pub(super) fn allocate(&self) -> Result<Vec<u8>, Error> {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(self.byte_len)
                .map_err(|_| failed("RGBA allocation"))?;
            bytes.resize(self.byte_len, 0);
            Ok(bytes)
        }
    }
}
