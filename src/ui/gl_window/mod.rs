//! An OpenGL window that egui paints into, with holes cut where the UI asks for them. On Windows the swap doesn't
//! wait for vblank, so `pace_frame` sleeps until it instead.

mod eraser;
mod profile;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

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
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

use crate::ui::erase::{self, Pane};
use eraser::Erasers;
use profile::Profile;

static EGUI: crate::stall::Stage = crate::stall::Stage("paint: building the UI (egui)");
static GL: crate::stall::Stage = crate::stall::Stage("paint: drawing with OpenGL");
static SWAP: crate::stall::Stage = crate::stall::Stage("paint: swap_buffers (presenting to the screen)");

// Set after each swap, so `pace_frame` only waits once a frame was shown.
static PRESENTED: AtomicBool = AtomicBool::new(false);

pub fn pace_frame() {
    if PRESENTED.swap(false, Ordering::Relaxed)
        && let Some(wait) = crate::platform::until_vblank()
    {
        std::thread::sleep(wait);
    }
}

pub struct GlWindow {
    pub ctx: egui::Context,
    state: egui_winit::State,
    painter: egui_glow::Painter,
    gl: Arc<glow::Context>,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    pub window: Window,
    erasers: Erasers,
    panes: Vec<Pane>,
    profile: Option<Profile>,
}

const NO_CONFIG: &str = "no OpenGL pixel format is available right now";

fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    let score = |c: &Config| rank(c.alpha_size(), c.supports_transparency().unwrap_or(false), c.num_samples());
    // glutin wants a config back, so an empty list can only end in a panic. `GlWindow::new` turns that into an
    // error: while a Windows display sleeps or wakes, the driver can list no pixel formats for a moment.
    first_best(configs, score).expect(NO_CONFIG)
}

fn rank(alpha_bits: u8, transparent: bool, samples: u8) -> u8 {
    (alpha_bits >= 8) as u8 * 4 + transparent as u8 * 2 + (samples == 0) as u8
}

fn first_best<T>(items: impl Iterator<Item = T>, score: impl Fn(&T) -> u8) -> Option<T> {
    let mut best: Option<(T, u8)> = None;
    for item in items {
        let points = score(&item);
        if best.as_ref().is_none_or(|(_, top)| points > *top) {
            best = Some((item, points));
        }
    }
    best.map(|(item, _)| item)
}

impl GlWindow {
    pub fn new(event_loop: &ActiveEventLoop, attributes: WindowAttributes) -> Result<Self, String> {
        // Windows drivers may offer no format with alpha (or none at all while a display wakes), so there any format
        // is acceptable and `rank` still prefers one with alpha.
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(if cfg!(target_os = "macos") { 8 } else { 0 })
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(cfg!(target_os = "macos"));

        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            DisplayBuilder::new().with_window_attributes(Some(attributes.clone())).build(
                event_loop,
                template,
                pick_config,
            )
        }));
        let (window, config) =
            built.map_err(|_| NO_CONFIG.to_owned())?.map_err(|e| format!("failed to create GL config: {e}"))?;
        let window = match window {
            Some(window) => window,
            None => glutin_winit::finalize_window(event_loop, attributes, &config)
                .map_err(|e| format!("failed to create window: {e}"))?,
        };

        let display = config.display();
        let raw_handle = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        let context = unsafe {
            display
                .create_context(&config, &ContextAttributesBuilder::new().build(Some(raw_handle)))
                .or_else(|_| {
                    display.create_context(
                        &config,
                        &ContextAttributesBuilder::new()
                            .with_context_api(ContextApi::Gles(None))
                            .build(Some(raw_handle)),
                    )
                })
                .map_err(|e| format!("failed to create GL context: {e}"))?
        };

        let surface_attributes = window
            .build_surface_attributes(SurfaceAttributesBuilder::<WindowSurface>::new())
            .map_err(|e| e.to_string())?;
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .map_err(|e| format!("failed to create GL surface: {e}"))?;
        let context = context.make_current(&surface).map_err(|e| format!("failed to make GL context current: {e}"))?;
        let interval = if cfg!(windows) { SwapInterval::DontWait } else { SwapInterval::Wait(NonZeroU32::MIN) };
        let _ = surface.set_swap_interval(&context, interval);

        let gl = Arc::new(unsafe { glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name)) });
        let painter = egui_glow::Painter::new(gl.clone(), "", None, false)
            .map_err(|e| format!("failed to create egui painter: {e}"))?;

        let ctx = egui::Context::default();
        crate::ui::font::install(&ctx);
        ctx.options_mut(|o| {
            o.theme_preference = egui::ThemePreference::Dark;
            o.zoom_with_keyboard = false;
        });
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            event_loop,
            Some(window.scale_factor() as f32),
            None,
            Some(painter.max_texture_side()),
        );

        Ok(Self {
            ctx,
            state,
            painter,
            gl,
            surface,
            context,
            window,
            erasers: Default::default(),
            panes: Vec::new(),
            profile: std::env::var_os("NOTCHBASE_PROFILE").map(|_| Profile::default()),
        })
    }

    fn make_current(&self) {
        if !self.context.is_current() {
            let _ = self.context.make_current(&self.surface);
        }
    }

    pub fn on_event(&mut self, event: &WindowEvent) -> bool {
        if let WindowEvent::Resized(_) = event {
            self.make_current();
            self.window.resize_surface(&self.surface, &self.context);
        }
        self.state.on_window_event(&self.window, event).repaint
    }

    pub fn knows_pointer(&self) -> bool {
        self.state.is_pointer_in_window()
    }

    pub fn push_input(&mut self, event: egui::Event) {
        self.input().push(event);
    }

    pub fn input(&mut self) -> &mut Vec<egui::Event> {
        &mut self.state.egui_input_mut().events
    }

    pub fn panes(&self) -> &[Pane] {
        &self.panes
    }

    pub fn paint(&mut self, run_ui: impl FnMut(&mut egui::Ui)) -> Duration {
        self.make_current();

        crate::stall::enter(&EGUI);
        let t_start = std::time::Instant::now();
        let raw_input = self.state.take_egui_input(&self.window);
        let output = self.ctx.run_ui(raw_input, run_ui);
        let t_ui = std::time::Instant::now();
        self.state.handle_platform_output(&self.window, output.platform_output);
        let repaint_delay =
            output.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);

        let mut textures = output.textures_delta;
        for (id, deltas) in textures.set.drain() {
            for delta in deltas {
                self.painter.set_texture(id, &delta);
            }
        }

        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        self.panes = erase::panes(&primitives);
        let primitives = eraser::through(primitives, self.ctx.content_rect(), &self.erasers);
        let t_tess = std::time::Instant::now();
        crate::stall::enter(&GL);
        let size = self.window.inner_size();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.clear_color(0.0, 0.0, 0.0, 0.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        self.painter.paint_primitives([size.width, size.height], output.pixels_per_point, &primitives);

        for id in textures.free.drain() {
            self.painter.free_texture(id);
        }

        let t_paint = std::time::Instant::now();
        crate::stall::enter(&SWAP);
        if let Err(err) = self.surface.swap_buffers(&self.context) {
            eprintln!("notchbase: swap_buffers failed: {err}");
        }
        PRESENTED.store(true, Ordering::Relaxed);
        if let Some(profile) = &mut self.profile {
            let t_swap = std::time::Instant::now();
            let title = self.window.title();
            profile.add(&title, [t_ui - t_start, t_tess - t_ui, t_paint - t_tess, t_swap - t_paint]);
        }

        repaint_delay
    }
}

impl Drop for GlWindow {
    fn drop(&mut self) {
        self.make_current();
        self.painter.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_prefers_alpha_then_transparency() {
        assert_eq!(rank(8, true, 0), 7);
        assert!(rank(8, false, 4) > rank(0, true, 0));
        assert!(rank(0, true, 4) > rank(0, false, 0));
        assert!(rank(0, false, 0) > rank(0, false, 4));
        assert_eq!(rank(0, false, 4), 0);
        assert_eq!(rank(16, false, 4), 4, "more alpha than needed counts the same");
    }

    #[test]
    fn earliest_best_config_wins() {
        let configs = [("plain", 0), ("alpha", 4), ("also alpha", 4), ("partial", 2)];
        let best = first_best(configs.into_iter(), |(_, points)| *points);
        assert_eq!(best, Some(("alpha", 4)));
    }

    #[test]
    fn lone_config_is_always_picked() {
        assert_eq!(first_best([("bare", 0)].into_iter(), |(_, points)| *points), Some(("bare", 0)));
    }

    #[test]
    fn no_configs_picks_nothing() {
        assert_eq!(first_best(std::iter::empty::<u8>(), |points| *points), None);
    }
}
