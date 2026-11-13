//! The dim around a recording: a click-through window over the recorded display that shades everything outside
//! the recorded area and draws a thin light edge around it.

use egui::{Color32, Mesh, Rect, Shape, pos2, vec2};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowId};

use super::overlay::fit::fit_to_monitor;
use crate::platform;
use crate::record::{Display, Region};
use crate::ui::gl_window::GlWindow;

const SHADE: u8 = 72;
const EDGE: u8 = 56;
const NO_DISPLAY: &str = "the recorded display is gone";

pub struct RecordingDim {
    gl: GlWindow,
    region: Region,
}

impl RecordingDim {
    /// Opens the dim over the recorded display. A whole display needs no dim, so that gives `None`.
    pub fn open(event_loop: &ActiveEventLoop, region: Region) -> Result<Option<Self>, String> {
        if region.is_whole() {
            return Ok(None);
        }
        let monitor = monitor_of(event_loop, &region.display).ok_or(NO_DISPLAY)?;
        let gl = GlWindow::new(event_loop, platform::dim_window_attributes(&monitor))?;
        pass_through(&gl.window, &monitor);
        fit_to_monitor(&gl.window, &monitor);
        let mut dim = Self { gl, region };
        dim.paint();
        dim.gl.window.set_visible(true);
        pass_through(&dim.gl.window, &monitor);
        Ok(Some(dim))
    }

    pub fn owns(&self, id: WindowId) -> bool {
        self.gl.window.id() == id
    }

    pub fn window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.paint(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.gl.on_event(event);
                self.gl.window.request_redraw();
            }
            _ => {}
        }
    }

    fn paint(&mut self) {
        let scale = self.gl.window.scale_factor() as f32;
        let size = self.gl.window.inner_size();
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(size.width as f32, size.height as f32) / scale);
        let cut = cutout(&self.region, scale);
        self.gl.paint(|ui| {
            let painter = ui.painter();
            painter.add(Shape::mesh(quads(&around(screen, cut), Color32::from_black_alpha(SHADE))));
            painter.add(Shape::mesh(quads(&around(cut.expand(1.0 / scale), cut), Color32::from_white_alpha(EDGE))));
        });
    }
}

fn pass_through(window: &Window, monitor: &MonitorHandle) {
    platform::configure_dim_window(window, monitor);
    platform::set_click_through(window, true);
}

fn monitor_of(event_loop: &ActiveEventLoop, display: &Display) -> Option<MonitorHandle> {
    event_loop.available_monitors().find(|monitor| Display::of(monitor).id == display.id)
}

fn cutout(region: &Region, scale: f32) -> Rect {
    let [x, y] = region.origin.map(|side| side as f32 / scale);
    let [w, h] = region.size.map(|side| side as f32 / scale);
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

fn around(outer: Rect, hole: Rect) -> [Rect; 4] {
    let hole = hole.intersect(outer);
    [
        Rect::from_min_max(outer.min, pos2(outer.right(), hole.top())),
        Rect::from_min_max(pos2(outer.left(), hole.bottom()), outer.max),
        Rect::from_min_max(pos2(outer.left(), hole.top()), pos2(hole.left(), hole.bottom())),
        Rect::from_min_max(pos2(hole.right(), hole.top()), pos2(outer.right(), hole.bottom())),
    ]
}

fn quads(rects: &[Rect], color: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    for rect in rects.iter().filter(|rect| rect.width() > 0.0 && rect.height() > 0.0) {
        mesh.add_colored_rect(*rect, color);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(a: Rect, b: Rect) -> f32 {
        let w = (a.right().min(b.right()) - a.left().max(b.left())).max(0.0);
        let h = (a.bottom().min(b.bottom()) - a.top().max(b.top())).max(0.0);
        w * h
    }

    const DISPLAY: Display = Display { id: 1, origin: [0, 0], size: [3000, 2000], scale: 1.5 };

    #[test]
    fn shade_surrounds_without_overlap() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(2000.0, 1333.0));
        let hole = Rect::from_min_size(pos2(400.0, 300.0), vec2(600.0, 400.0));
        let parts = around(screen, hole);
        let area: f32 = parts.iter().map(|rect| rect.area()).sum();
        assert!((area + hole.area() - screen.area()).abs() < 0.5, "shade plus hole covers the screen exactly");
        for (i, a) in parts.iter().enumerate() {
            assert_eq!(overlap(*a, hole), 0.0, "part {i} covers the recording");
            for b in &parts[i + 1..] {
                assert_eq!(overlap(*a, *b), 0.0, "shade parts double up");
            }
        }
    }

    #[test]
    fn cutout_lands_on_recorded_pixels() {
        let region = Region::new(DISPLAY, [300, 150], [900, 600]);
        let cut = cutout(&region, 1.5);
        assert_eq!(cut, Rect::from_min_size(pos2(200.0, 100.0), vec2(600.0, 400.0)));
    }

    #[test]
    fn edge_is_one_pixel_outside() {
        let hole = Rect::from_min_size(pos2(200.0, 100.0), vec2(600.0, 400.0));
        let edge = around(hole.expand(1.0 / 1.5), hole);
        assert!(edge.iter().all(|rect| overlap(*rect, hole) == 0.0));
        assert!((edge[0].height() - 1.0 / 1.5).abs() < 1e-4);
    }
}
