//! An OpenGL window egui paints into, with a hook to draw the liquid underneath, and an offscreen target for
//! headless snapshots.

use std::num::NonZeroU32;
use std::sync::Arc;

use glow::HasContext as _;
use glutin::config::{Config, ConfigTemplateBuilder, GlConfig as _};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentGlContext as _, PossiblyCurrentContext,
    PossiblyCurrentGlContext as _,
};
use glutin::display::{GetGlDisplay as _, GlDisplay as _};
use glutin::surface::{GlSurface as _, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow as _};
use raw_window_handle::HasWindowHandle as _;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

pub struct GlWindow {
    pub ctx: egui::Context,
    pub gl: Arc<glow::Context>,
    pub shader_version: egui_glow::ShaderVersion,
    painter: egui_glow::Painter,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    pub window: Window,
    started: std::time::Instant,
    /// Snapshots draw at a fixed scale, whichever screen the hidden window lands on.
    pub fixed_scale: Option<f32>,
}

thread_local! {
    static PICKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread is picking a GL config, where a panic is caught: the panic hook leaves the taskbar alone then.
pub fn picking_config() -> bool {
    PICKING.get()
}

const NO_CONFIG: &str = "no OpenGL pixel format is available right now";

fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    let mut best: Option<(Config, u8)> = None;
    for c in configs {
        let points = (c.alpha_size() >= 8) as u8 * 4
            + c.supports_transparency().unwrap_or(false) as u8 * 2
            + (c.num_samples() == 0) as u8;
        if best.as_ref().is_none_or(|(_, top)| points > *top) {
            best = Some((c, points));
        }
    }
    // glutin wants a config back, so an empty list can only end in a panic, which `GlWindow::new` turns into an
    // error: while a Windows display sleeps or wakes, the driver can list no pixel formats for a moment.
    best.map(|(c, _)| c).expect(NO_CONFIG)
}

impl GlWindow {
    pub fn new(event_loop: &ActiveEventLoop, attributes: WindowAttributes, vsync: bool) -> Result<Self, String> {
        let template = ConfigTemplateBuilder::new()
            // Windows drivers may offer no format with alpha, so any is fine there; one with alpha is still preferred.
            .with_alpha_size(if cfg!(target_os = "macos") { 8 } else { 0 })
            .with_depth_size(0)
            .with_stencil_size(0)
            // Windows drivers rarely offer a "transparent" pixel format, and asking for one finds none; the window is
            // see-through there anyway, through DWM.
            .with_transparency(cfg!(target_os = "macos"));
        PICKING.set(true);
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            DisplayBuilder::new().with_window_attributes(Some(attributes.clone())).build(
                event_loop,
                template,
                pick_config,
            )
        }));
        PICKING.set(false);
        let (window, config) =
            built.map_err(|_| NO_CONFIG.to_owned())?.map_err(|e| format!("failed to create GL config: {e}"))?;
        let window = match window {
            Some(window) => window,
            None => glutin_winit::finalize_window(event_loop, attributes, &config)
                .map_err(|e| format!("failed to create window: {e}"))?,
        };
        let display = config.display();
        let raw = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        let context = unsafe {
            display.create_context(&config, &ContextAttributesBuilder::new().build(Some(raw))).or_else(|_| {
                display.create_context(
                    &config,
                    &ContextAttributesBuilder::new().with_context_api(ContextApi::Gles(None)).build(Some(raw)),
                )
            })
        }
        .map_err(|e| format!("failed to create GL context: {e}"))?;
        let surface_attributes = window
            .build_surface_attributes(SurfaceAttributesBuilder::<WindowSurface>::new())
            .map_err(|e| e.to_string())?;
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .map_err(|e| format!("failed to create GL surface: {e}"))?;
        let context = context.make_current(&surface).map_err(|e| format!("failed to make GL context current: {e}"))?;
        let interval = if vsync { SwapInterval::Wait(NonZeroU32::MIN) } else { SwapInterval::DontWait };
        let _ = surface.set_swap_interval(&context, interval);

        let gl = Arc::new(unsafe { glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name)) });
        let shader_version = egui_glow::ShaderVersion::get(&gl);
        let painter = egui_glow::Painter::new(gl.clone(), "", None, false)
            .map_err(|e| format!("failed to create egui painter: {e}"))?;
        let ctx = egui::Context::default();
        notchbase_paint::font::install(&ctx);
        ctx.options_mut(|o| {
            o.theme_preference = egui::ThemePreference::Dark;
            o.zoom_with_keyboard = false;
        });
        Ok(Self {
            ctx,
            gl,
            shader_version,
            painter,
            surface,
            context,
            window,
            started: std::time::Instant::now(),
            fixed_scale: None,
        })
    }

    pub fn make_current(&self) {
        if !self.context.is_current() {
            let _ = self.context.make_current(&self.surface);
        }
    }

    pub fn resized(&self) {
        self.make_current();
        self.window.resize_surface(&self.surface, &self.context);
    }

    pub fn scale(&self) -> f32 {
        self.fixed_scale.unwrap_or(self.window.scale_factor() as f32)
    }

    /// Size in points.
    pub fn size(&self) -> egui::Vec2 {
        let px = self.window.inner_size();
        egui::vec2(px.width as f32, px.height as f32) / self.scale()
    }

    /// Paints one frame: `under` draws with raw GL first (the liquid), then egui draws what `ui` paints on top.
    pub fn frame(
        &mut self,
        under: impl FnOnce(&glow::Context, [u32; 2], f32),
        ui: impl FnMut(&mut egui::Ui),
        target: Option<glow::Framebuffer>,
        size_px: Option<[u32; 2]>,
    ) {
        self.make_current();
        let size = size_px.unwrap_or_else(|| {
            let s = self.window.inner_size();
            [s.width.max(1), s.height.max(1)]
        });
        let ppp = self.scale();
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32) / ppp,
            )),
            max_texture_side: Some(self.painter.max_texture_side()),
            time: Some(self.started.elapsed().as_secs_f64()),
            focused: false,
            ..Default::default()
        };
        if let Some(viewport) = raw.viewports.get_mut(&egui::ViewportId::ROOT) {
            viewport.native_pixels_per_point = Some(ppp);
        } else {
            let viewport = egui::ViewportInfo { native_pixels_per_point: Some(ppp), ..Default::default() };
            raw.viewports.insert(egui::ViewportId::ROOT, viewport);
        }
        let mut output = self.ctx.run_ui(raw, ui);
        for (id, deltas) in std::mem::take(&mut output.textures_delta.set) {
            for delta in deltas {
                self.painter.set_texture(id, &delta);
            }
        }
        let primitives = self.ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            self.gl.viewport(0, 0, size[0] as i32, size[1] as i32);
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.clear_color(0.0, 0.0, 0.0, 0.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        under(&self.gl, size, ppp);
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            self.gl.viewport(0, 0, size[0] as i32, size[1] as i32);
        }
        self.painter.paint_primitives(size, output.pixels_per_point, &primitives);
        for id in std::mem::take(&mut output.textures_delta.free) {
            self.painter.free_texture(id);
        }
    }

    pub fn present(&self) {
        if let Err(err) = self.surface.swap_buffers(&self.context) {
            eprintln!("notchbase-dock: swap_buffers failed: {err}");
        }
    }
}

impl Drop for GlWindow {
    fn drop(&mut self) {
        self.make_current();
        self.painter.destroy();
    }
}

pub fn compile(gl: &glow::Context, version: egui_glow::ShaderVersion, vertex: &str, fragment: &str) -> glow::Program {
    let header = version.version_declaration();
    let (vin, vout, fin) = if version.is_new_shader_interface() {
        ("#define IN in\n#define OUT out\n", "", "#define IN in\nout vec4 frag_color;\n#define TEX texture\n")
    } else {
        (
            "#define IN attribute\n#define OUT varying\n",
            "",
            "#define IN varying\n#define frag_color gl_FragColor\n#define TEX texture2D\n",
        )
    };
    let precision = if version.is_embedded() { "precision highp float;\nprecision highp int;\n" } else { "" };
    unsafe {
        let program = gl.create_program().expect("a GL program");
        let sources = [
            (glow::VERTEX_SHADER, format!("{header}\n{precision}{vin}{vout}{vertex}")),
            (glow::FRAGMENT_SHADER, format!("{header}\n{precision}{fin}{fragment}")),
        ];
        let mut shaders = Vec::new();
        for (kind, source) in sources {
            let shader = gl.create_shader(kind).expect("a GL shader");
            gl.shader_source(shader, &source);
            gl.compile_shader(shader);
            assert!(gl.get_shader_compile_status(shader), "shader failed: {}", gl.get_shader_info_log(shader));
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        assert!(gl.get_program_link_status(program), "link failed: {}", gl.get_program_info_log(program));
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        program
    }
}

/// An offscreen RGBA8 target, for snapshots.
pub struct Offscreen {
    pub fbo: glow::Framebuffer,
    texture: glow::Texture,
    pub size: [u32; 2],
}

impl Offscreen {
    pub fn new(gl: &glow::Context, size: [u32; 2]) -> Self {
        unsafe {
            let texture = gl.create_texture().expect("a texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                size[0] as i32,
                size[1] as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            let fbo = gl.create_framebuffer().expect("a framebuffer");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
            Self { fbo, texture, size }
        }
    }

    /// Premultiplied RGBA rows, top row first.
    pub fn read(&self, gl: &glow::Context) -> Vec<u8> {
        let [w, h] = self.size;
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
        }
        let row = (w * 4) as usize;
        let mut flipped = Vec::with_capacity(pixels.len());
        for y in (0..h as usize).rev() {
            flipped.extend_from_slice(&pixels[y * row..(y + 1) * row]);
        }
        flipped
    }

    pub fn delete(self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.texture);
        }
    }
}
