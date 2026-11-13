//! The annotate window itself: opens centred on its monitor, sized to the screenshot, and passes events to the editor.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Pos2, Vec2, pos2, vec2};
use winit::dpi::{LogicalPosition, PhysicalPosition, Position};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::WindowId;

use super::clock::Clock;
use super::editor::forget_repeated_strikes;
use super::{Brush, CHROME, Editor, Focus, Job, Outcome, SMALLEST_WINDOW, window_size};
use crate::capture::Screenshot;
use crate::platform;
use crate::ui::gl_window::GlWindow;

const NOT_DRAWABLE: &str = "the screenshot has no pixels to draw on";
const REPAINT_HORIZON: Duration = Duration::from_secs(3600);
// macOS places windows in points, Windows in physical pixels.
const PLACED_IN_POINTS: bool = cfg!(target_os = "macos");

pub struct Annotator {
    gl: GlWindow,
    editor: Editor,
    clock: Clock,
    outcomes: Vec<Outcome>,
    repaint_at: Option<Instant>,
    focused: bool,
    pointer: Option<Pos2>,
}

impl Annotator {
    pub fn open(
        event_loop: &ActiveEventLoop,
        monitor: &MonitorHandle,
        shot: Arc<Screenshot>,
        brush: Brush,
    ) -> Result<Self, String> {
        let editor = Editor::new(shot, brush).ok_or(NOT_DRAWABLE)?;
        let scale = monitor.scale_factor() as f32;
        let screen = Screen::of(monitor);
        let size = window_size(editor.image_size(), scale, screen.size / scale, CHROME);
        let attributes =
            platform::annotate_window_attributes(size, SMALLEST_WINDOW).with_position(screen.centred(size));
        let gl = GlWindow::new(event_loop, attributes)?;
        platform::configure_annotate_window(&gl.window);
        let mut annotator = Self {
            gl,
            editor,
            clock: Clock::default(),
            outcomes: Vec::new(),
            repaint_at: None,
            focused: false,
            pointer: None,
        };
        annotator.paint();
        annotator.gl.window.set_visible(true);
        platform::configure_annotate_window(&annotator.gl.window);
        annotator.gl.window.focus_window();
        Ok(annotator)
    }

    pub fn owns(&self, id: WindowId) -> bool {
        self.gl.window.id() == id
    }

    pub fn window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.paint(),
            WindowEvent::CloseRequested => {
                self.finish_what_is_in_progress();
                let mut outcomes = self.editor.close();
                self.outcomes.append(&mut outcomes);
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. }
                if self.pointer.is_some_and(|at| self.editor.moves_the_window_from(at)) =>
            {
                let _ = self.gl.window.drag_window();
            }
            _ => self.hear(event),
        }
    }

    pub fn take_outcomes(&mut self) -> Vec<Outcome> {
        std::mem::take(&mut self.outcomes)
    }

    pub fn finish_what_is_in_progress(&mut self) {
        let mut outcomes = self.editor.finish_what_is_in_progress(self.gl.input());
        self.outcomes.append(&mut outcomes);
    }

    pub fn repaint_at(&self) -> Option<Instant> {
        self.repaint_at
    }

    pub fn redraw(&mut self) {
        self.repaint_at = None;
        self.gl.window.request_redraw();
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn focus(&self, focus: Focus) {
        if focus.restores() {
            self.gl.window.set_minimized(false);
        }
        if focus.reaches(self.gl.window.is_minimized().unwrap_or(false)) {
            self.gl.window.focus_window();
        }
    }

    pub fn shows(&self, shot: &Arc<Screenshot>) -> bool {
        self.editor.shows(shot)
    }

    pub fn job(&self) -> Job {
        self.editor.job()
    }

    pub fn copied(&mut self, revision: u64) {
        self.editor.copied(revision);
        self.gl.window.request_redraw();
    }

    pub fn not_copied(&mut self, revision: u64) {
        self.editor.not_copied(revision);
        self.gl.window.request_redraw();
    }

    pub fn revision(&self) -> u64 {
        self.editor.revision()
    }

    fn hear(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::Focused(focused) => {
                self.focused = *focused;
                if !focused {
                    self.finish_what_is_in_progress();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let at = position.to_logical::<f32>(self.gl.window.scale_factor());
                self.pointer = Some(pos2(at.x, at.y));
            }
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            _ => {}
        }
        let already_heard = self.gl.input().len();
        if self.gl.on_event(event) {
            self.gl.window.request_redraw();
        }
        if is_a_key_repeat(event) {
            forget_repeated_strikes(self.gl.input(), already_heard);
        }
    }

    fn paint(&mut self) {
        let now = Instant::now();
        let dt = self.clock.tick(now);
        self.editor.sits_under_a_title_bar(platform::annotate_title_inset(&self.gl.window));
        let editor = &mut self.editor;
        let mut outcomes = Vec::new();
        let delay = self.gl.paint(|ui| outcomes = editor.ui(ui, dt));
        self.outcomes.append(&mut outcomes);
        self.repaint_at = next_repaint(now, self.editor.is_animating(), delay);
    }
}

fn is_a_key_repeat(event: &WindowEvent) -> bool {
    matches!(event, WindowEvent::KeyboardInput { event: key, .. } if key.repeat)
}

fn next_repaint(now: Instant, animating: bool, delay: Duration) -> Option<Instant> {
    if animating {
        return Some(now);
    }
    (delay < REPAINT_HORIZON).then(|| now + delay)
}

struct Screen {
    origin: Vec2,
    size: Vec2,
    scale: f32,
}

impl Screen {
    fn of(monitor: &MonitorHandle) -> Self {
        let (origin, size) = (monitor.position(), monitor.size());
        Self {
            origin: vec2(origin.x as f32, origin.y as f32),
            size: vec2(size.width as f32, size.height as f32),
            scale: monitor.scale_factor() as f32,
        }
    }

    fn centred(&self, window: Vec2) -> Position {
        let corner = self.origin + (self.size - window * self.scale) / 2.0;
        if PLACED_IN_POINTS {
            let points = corner / self.scale;
            LogicalPosition::new(f64::from(points.x), f64::from(points.y)).into()
        } else {
            PhysicalPosition::new(corner.x.round() as i32, corner.y.round() as i32).into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_centred_on_its_monitor() {
        let screen = Screen { origin: vec2(-3840.0, 200.0), size: vec2(3840.0, 2160.0), scale: 2.0 };
        let placed = screen.centred(vec2(900.0, 640.0));
        let expected: Position = if PLACED_IN_POINTS {
            LogicalPosition::new(-1920.0 + 510.0, 100.0 + 220.0).into()
        } else {
            PhysicalPosition::new(-3840 + 1020, 200 + 440).into()
        };
        assert_eq!(placed, expected);
    }

    #[test]
    fn moving_springs_want_next_frame() {
        let now = Instant::now();
        assert_eq!(next_repaint(now, true, Duration::MAX), Some(now));
        assert_eq!(next_repaint(now, false, Duration::MAX), None);
        let soon = Duration::from_millis(120);
        assert_eq!(next_repaint(now, false, soon), Some(now + soon));
    }
}
