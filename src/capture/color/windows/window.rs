//! The loupe's window. It hides from captures when Windows allows, follows the cursor and stays on top.

use std::time::Instant;

use egui::{ColorImage, CursorIcon, Painter, TextureHandle, TextureId, TextureOptions, Vec2, vec2};
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows as _};
use winit::window::{WindowAttributes, WindowId, WindowLevel};

use super::native::Hwnd;
use super::screen::monitor_rect;
use crate::capture::color::loupe::{WINDOW, origin_for};
use crate::capture::overlay::clock::elapsed;
use crate::platform::Capture;
use crate::ui::gl_window::GlWindow;

/// How often, in seconds, the loupe is put back on top while it follows the cursor.
const RAISE_EVERY: f32 = 0.5;

pub(super) struct LoupeWindow {
    gl: GlWindow,
    hwnd: Hwnd,
    excluded: bool,
    texture: Option<TextureHandle>,
    shown: bool,
    origin: Option<[i32; 2]>,
    raised_at: Option<Instant>,
}

impl LoupeWindow {
    pub(super) fn new(event_loop: &ActiveEventLoop) -> Result<Self, String> {
        let gl = GlWindow::new(event_loop, attributes())?;
        let hwnd = Hwnd::of(&gl.window).ok_or("the sampler window has no native handle")?;
        hwnd.make_passive_tool_window();
        let excluded = hide_from_captures(hwnd);
        Ok(Self { gl, hwnd, excluded, texture: None, shown: false, origin: None, raised_at: None })
    }

    pub(super) fn id(&self) -> WindowId {
        self.gl.window.id()
    }

    pub(super) fn scale(&self) -> f32 {
        self.gl.window.scale_factor() as f32
    }

    pub(super) fn view(&self) -> Vec2 {
        let size = self.gl.window.inner_size();
        vec2(size.width as f32, size.height as f32) / self.scale()
    }

    pub(super) fn is_excluded(&self) -> bool {
        self.excluded
    }

    pub(super) fn origin(&self) -> Option<[i32; 2]> {
        self.origin
    }

    pub(super) fn place(&mut self, cursor: [i32; 2], now: Instant) -> [i32; 2] {
        let mut origin = self.target_origin(cursor);
        self.move_to(origin);
        // Moving onto a monitor with another scale resizes the window, so place it once more.
        let rescaled = self.target_origin(cursor);
        if rescaled != origin {
            origin = rescaled;
            self.move_to(origin);
        }
        if self.raised_at.is_none_or(|at| elapsed(at, now) > RAISE_EVERY) {
            self.hwnd.raise();
            self.raised_at = Some(now);
        }
        self.origin = Some(origin);
        origin
    }

    pub(super) fn show(&mut self) {
        self.hwnd.show_without_activating();
        self.hwnd.raise();
        self.hwnd.make_passive_tool_window();
        self.gl.window.set_cursor_visible(false);
        self.shown = true;
    }

    pub(super) fn hide(&mut self) {
        if self.shown {
            self.gl.window.set_cursor_visible(true);
            self.gl.paint(|_| {});
            self.hwnd.hide();
        }
        self.shown = false;
        self.origin = None;
        self.raised_at = None;
    }

    pub(super) fn on_event(&mut self, event: &WindowEvent) {
        self.gl.on_event(event);
    }

    pub(super) fn request_redraw(&self) {
        self.gl.window.request_redraw();
    }

    pub(super) fn paint(&mut self, grab: &Capture, upload: bool, mut draw: impl FnMut(&Painter, TextureId)) {
        let Self { gl, texture, .. } = self;
        gl.paint(|ui| {
            ui.ctx().set_cursor_icon(CursorIcon::None);
            let texture = texture_for(ui.ctx(), texture, grab, upload);
            draw(ui.painter(), texture);
        });
    }

    fn target_origin(&self, cursor: [i32; 2]) -> [i32; 2] {
        let size = self.gl.window.inner_size();
        let size = [size.width as i32, size.height as i32];
        origin_for(cursor, self.gl.window.scale_factor(), size, self.excluded, monitor_rect(cursor))
    }

    fn move_to(&self, origin: [i32; 2]) {
        let target = PhysicalPosition::new(origin[0], origin[1]);
        if self.gl.window.outer_position().ok() != Some(target) {
            self.gl.window.set_outer_position(target);
        }
    }
}

fn attributes() -> WindowAttributes {
    WindowAttributes::default()
        .with_title("notchbase color sampler")
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(false)
        .with_active(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_skip_taskbar(true)
        .with_undecorated_shadow(false)
        .with_corner_preference(CornerPreference::DoNotRound)
        .with_inner_size(LogicalSize::new(WINDOW.x as f64, WINDOW.y as f64))
}

/// `NOTCHBASE_LOUPE_IN_CAPTURES` turns this off, for screenshots of the loupe itself.
fn hide_from_captures(hwnd: Hwnd) -> bool {
    if std::env::var_os("NOTCHBASE_LOUPE_IN_CAPTURES").is_some() {
        return false;
    }
    let hidden = hwnd.exclude_from_capture();
    if !hidden {
        eprintln!("notchbase: this Windows version can't hide the loupe from captures; it will sit beside the cursor");
    }
    hidden
}

fn texture_for(ctx: &egui::Context, slot: &mut Option<TextureHandle>, grab: &Capture, upload: bool) -> TextureId {
    let image = || ColorImage::from_rgba_unmultiplied([grab.width, grab.height], &grab.rgba);
    match slot {
        Some(texture) => {
            if upload {
                texture.set(image(), TextureOptions::NEAREST);
            }
            texture.id()
        }
        None => slot.insert(ctx.load_texture("sampler", image(), TextureOptions::NEAREST)).id(),
    }
}
