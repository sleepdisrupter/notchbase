//! One overlay window per monitor, kept between uses and rebuilt only when a monitor's position, size or scale changes.

use std::time::Instant;

use egui::{CursorIcon, Painter, Pos2, Vec2, pos2, vec2};
use winit::dpi::PhysicalPosition;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::{CursorIcon as SystemCursor, WindowId};

#[cfg(not(target_os = "macos"))]
use crate::capture::Screenshot;
use crate::capture::overlay::fit::fit_to_monitor;
use crate::platform;
use crate::record::{Display, Region};
use crate::ui::gl_window::GlWindow;

use super::geometry::{PixelRect, corner_at, cursor_position_for, pixel_at};

/// For this long after a nudge, the cursor is taken to be where we put it.
const NUDGE_HOLD: f32 = 0.3;

/// What lets an overlay be reused: its monitor's position, size and scale.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MonitorKey {
    position: (i32, i32),
    size: (u32, u32),
    scale: u64,
}

impl MonitorKey {
    fn of(monitor: &MonitorHandle) -> Self {
        let (position, size) = (monitor.position(), monitor.size());
        Self {
            position: (position.x, position.y),
            size: (size.width, size.height),
            scale: monitor.scale_factor().to_bits(),
        }
    }
}

pub(super) struct MonitorOverlay {
    gl: GlWindow,
    monitor: MonitorHandle,
    key: MonitorKey,
    size: [usize; 2],
    shown: bool,
    drawn: Option<PixelRect>,
    event_cursor: Option<Pos2>,
    nudged: Option<(Pos2, Instant)>,
}

impl MonitorOverlay {
    fn open(event_loop: &ActiveEventLoop, monitor: &MonitorHandle) -> Result<Self, String> {
        let gl = GlWindow::new(event_loop, platform::overlay_window_attributes(monitor))?;
        platform::configure_overlay_window(&gl.window, monitor);
        fit_to_monitor(&gl.window, monitor);
        let size = monitor.size();
        Ok(Self {
            gl,
            monitor: monitor.clone(),
            key: MonitorKey::of(monitor),
            size: [size.width as usize, size.height as usize],
            shown: false,
            drawn: None,
            event_cursor: None,
            nudged: None,
        })
    }

    pub(super) fn id(&self) -> WindowId {
        self.gl.window.id()
    }

    pub(super) fn is_shown(&self) -> bool {
        self.shown
    }

    pub(super) fn size(&self) -> [usize; 2] {
        self.size
    }

    pub(super) fn view(&self) -> Vec2 {
        let scale = self.gl.window.scale_factor() as f32;
        vec2(self.size[0] as f32, self.size[1] as f32) / scale
    }

    pub(super) fn cursor(&self) -> Option<Pos2> {
        if let Some((pos, at)) = self.nudged
            && at.elapsed().as_secs_f32() < NUDGE_HOLD
        {
            return Some(pos);
        }
        platform::cursor_in_window(&self.gl.window).or(self.event_cursor)
    }

    pub(super) fn corner(&self, pos: Pos2) -> [i64; 2] {
        corner_at(pos, self.view(), self.size)
    }

    pub(super) fn show(&mut self) {
        platform::configure_overlay_window(&self.gl.window, &self.monitor);
        fit_to_monitor(&self.gl.window, &self.monitor);
        self.gl.push_input(egui::Event::PointerGone);
        self.forget_pointer();
        self.gl.paint(|ui| ui.ctx().set_cursor_icon(CursorIcon::Crosshair));

        let window = &self.gl.window;
        window.set_visible(true);
        platform::configure_overlay_window(window, &self.monitor);
        window.set_cursor(SystemCursor::Crosshair);
        window.set_cursor_visible(true);
        self.shown = true;
    }

    pub(super) fn hide(&mut self) {
        if self.shown {
            self.gl.window.set_cursor(SystemCursor::Default);
            self.gl.paint(|_| {});
            self.gl.window.set_visible(false);
        }
        self.shown = false;
        self.forget_pointer();
    }

    pub(super) fn focus(&self) {
        self.gl.window.focus_window();
    }

    pub(super) fn observe(&mut self, event: &WindowEvent) {
        self.gl.on_event(event);
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.gl.window.scale_factor();
                let pos = to_points(*position, scale);
                self.event_cursor = Some(pos);
                if self.nudged.is_some_and(|(p, _)| (p - pos).length() * scale as f32 > 1.0) {
                    self.nudged = None;
                }
            }
            WindowEvent::CursorLeft { .. } => self.event_cursor = None,
            _ => {}
        }
    }

    pub(super) fn nudge(&mut self, from: Pos2, dx: i64, dy: i64) {
        let [x, y] = pixel_at(from, self.view(), self.size);
        let target = [(x + dx).clamp(0, self.size[0] as i64 - 1), (y + dy).clamp(0, self.size[1] as i64 - 1)];
        let position = cursor_position_for(target);
        if self.gl.window.set_cursor_position(position).is_ok() {
            let pos = to_points(position, self.gl.window.scale_factor());
            self.nudged = Some((pos, Instant::now()));
            self.event_cursor = Some(pos);
        }
    }

    pub(super) fn refresh(&self, force: bool, wanted: Option<PixelRect>) {
        if self.shown && (force || self.drawn != wanted) {
            self.gl.window.request_redraw();
        }
    }

    pub(super) fn paint(&mut self, selection: Option<PixelRect>, mut draw: impl FnMut(&Painter)) {
        self.gl.paint(|ui| {
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
            draw(ui.painter());
        });
        self.drawn = selection;
    }

    pub(super) fn region(&self, rect: PixelRect) -> Region {
        let origin = [rect.x0, rect.y0].map(|side| side.max(0) as u32);
        let size = [rect.width(), rect.height()].map(|side| side.max(0) as u32);
        Region::new(Display::of(&self.monitor), origin, size)
    }

    #[cfg(target_os = "macos")]
    pub(super) fn exact_region(&self, rect: PixelRect) -> Region {
        let origin = [rect.x0, rect.y0].map(|side| side.max(0) as u32);
        let size = [rect.width(), rect.height()].map(|side| side.max(1) as u32);
        Region { display: Display::of(&self.monitor), origin, size }
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn grab(&self, rect: PixelRect) -> Option<Screenshot> {
        let capture = platform::capture_monitor(&self.monitor)
            .map_err(|err| eprintln!("notchbase: screenshot failed: {err}"))
            .ok()?;
        let (width, height) = (rect.width() as usize, rect.height() as usize);
        let rgba = platform::to_srgb_image(&capture, rect.x0 as usize, rect.y0 as usize, width, height);
        Some(Screenshot { width, height, rgba })
    }

    fn forget_pointer(&mut self) {
        self.drawn = None;
        self.event_cursor = None;
        self.nudged = None;
    }
}

pub(super) fn reopen(overlays: &mut Vec<MonitorOverlay>, event_loop: &ActiveEventLoop) -> Result<(), String> {
    let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
    if monitors.is_empty() {
        return Err("no monitor found".into());
    }
    let mut previous = std::mem::take(overlays);
    let mut error = None;
    for monitor in &monitors {
        match reuse_or_open(&mut previous, event_loop, monitor) {
            Ok(overlay) => overlays.push(overlay),
            Err(err) => {
                eprintln!("notchbase: could not open a screenshot overlay: {err}");
                error = Some(err);
            }
        }
    }
    drop(previous);
    if overlays.is_empty() {
        return Err(error.unwrap_or_else(|| "could not open the screenshot overlay".into()));
    }
    Ok(())
}

/// The shown overlay the cursor is on, and where. A cursor up to a point past an edge still counts.
pub(super) fn locate(overlays: &[MonitorOverlay]) -> Option<(usize, Pos2)> {
    let hits =
        || overlays.iter().enumerate().filter(|(_, o)| o.shown).filter_map(|(i, o)| Some((i, o.cursor()?, o.view())));
    let inside = |p: Pos2, view: Vec2, slack: f32| {
        p.x >= -slack && p.y >= -slack && p.x < view.x + slack && p.y < view.y + slack
    };
    hits()
        .find(|&(_, p, view)| inside(p, view, 0.0))
        .or_else(|| hits().find(|&(_, p, view)| inside(p, view, 1.0)))
        .map(|(i, p, _)| (i, p))
}

fn reuse_or_open(
    previous: &mut Vec<MonitorOverlay>,
    event_loop: &ActiveEventLoop,
    monitor: &MonitorHandle,
) -> Result<MonitorOverlay, String> {
    let key = MonitorKey::of(monitor);
    match previous.iter().position(|o| o.key == key) {
        Some(k) => {
            let mut overlay = previous.swap_remove(k);
            overlay.monitor = monitor.clone();
            Ok(overlay)
        }
        None => MonitorOverlay::open(event_loop, monitor),
    }
}

fn to_points(position: PhysicalPosition<f64>, scale: f64) -> Pos2 {
    pos2((position.x / scale) as f32, (position.y / scale) as f32)
}
