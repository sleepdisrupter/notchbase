//! The ghost: a click-through overlay on the screen being snapped to, showing where the dragged window will land
//! and, fainter, where the other windows would go.

use std::time::Instant;

use egui::{Color32, CornerRadius, Rect, Shape, Stroke, StrokeKind, pos2, vec2};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::WindowId;

use super::Screen;
use crate::capture::fit_overlay_to_monitor;
use crate::platform;
use crate::ui::color::PEARL;
use crate::ui::gl_window::GlWindow;
use crate::ui::motion;
use crate::ui::spring::{POINT_PRECISION, Spring, SpringConfig, UNIT_PRECISION};

const RADIUS: f32 = 12.0;
const GLIDE: SpringConfig = SpringConfig::new(0.22, 0.9);
const FILL: f32 = 0.13;
const OUTLINE: f32 = 0.8;
const FADE_OUT_BELOW: f32 = 0.01;

/// What the ghost shows: the dragged window's new place, and the others' when a tidy moves them too.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub target: Option<Rect>,
    pub others: Vec<Rect>,
}

pub struct Ghost {
    gl: GlWindow,
    screen: Screen,
    glide: [Spring; 4],
    gliding: bool,
    /// The target's and the other windows' own fades, so either comes and goes softly while the drag moves between
    /// layouts, rather than popping. What faded out stays drawn until it's gone.
    target_shown: Spring,
    others_shown: Spring,
    last_target: Option<Rect>,
    last_others: Vec<Rect>,
    shown: Spring,
    wanted: bool,
    last: Instant,
}

impl Ghost {
    pub fn open(event_loop: &ActiveEventLoop, monitor: &MonitorHandle, screen: Screen) -> Result<Self, String> {
        let gl = GlWindow::new(event_loop, platform::dim_window_attributes(monitor))?;
        pass_through(&gl.window, monitor);
        fit_overlay_to_monitor(&gl.window, monitor);
        let spring = |value: f32| Spring::new(value, GLIDE, POINT_PRECISION);
        let mut ghost = Self {
            gl,
            screen,
            glide: [spring(0.0), spring(0.0), spring(0.0), spring(0.0)],
            gliding: false,
            target_shown: Spring::new(0.0, motion::HOVER, UNIT_PRECISION),
            others_shown: Spring::new(0.0, motion::HOVER, UNIT_PRECISION),
            last_target: None,
            last_others: Vec::new(),
            shown: Spring::new(0.0, motion::HOVER, UNIT_PRECISION),
            wanted: true,
            last: Instant::now(),
        };
        ghost.paint();
        ghost.gl.window.set_visible(true);
        pass_through(&ghost.gl.window, monitor);
        Ok(ghost)
    }

    pub fn owns(&self, id: WindowId) -> bool {
        self.gl.window.id() == id
    }

    pub fn screen_key(&self) -> u64 {
        self.screen.key
    }

    pub fn show(&mut self, scene: Scene) {
        self.wanted = true;
        self.shown.animate_to(1.0, motion::HOVER);
        match scene.target {
            Some(rect) => {
                self.aim(rect);
                self.last_target = Some(rect);
                self.target_shown.animate_to(1.0, motion::HOVER);
            }
            None => self.target_shown.animate_to(0.0, motion::HOVER),
        }
        if scene.others.is_empty() {
            self.others_shown.animate_to(0.0, motion::HOVER);
        } else {
            self.last_others.clone_from(&scene.others);
            self.others_shown.animate_to(1.0, motion::HOVER);
        }
        self.gl.window.request_redraw();
    }

    pub fn hide(&mut self) {
        self.wanted = false;
        self.shown.animate_to(0.0, motion::HOVER);
        self.gl.window.request_redraw();
    }

    /// Whether it has faded out after `hide`, so the app can close it.
    pub fn is_gone(&self) -> bool {
        !self.wanted && self.shown.value < FADE_OUT_BELOW
    }

    pub fn is_animating(&self) -> bool {
        !self.shown.is_settled()
            || !self.target_shown.is_settled()
            || !self.others_shown.is_settled()
            || self.glide.iter().any(|spring| !spring.is_settled())
    }

    pub fn redraw(&self) {
        self.gl.window.request_redraw();
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

    /// The first aim jumps straight there, and later ones glide; so does one after the last target had faded away.
    fn aim(&mut self, rect: Rect) {
        let local = self.screen.local(rect);
        let goal = [local.left(), local.top(), local.width(), local.height()];
        if !self.gliding || self.target_shown.value < FADE_OUT_BELOW {
            for (spring, value) in self.glide.iter_mut().zip(goal) {
                spring.snap_to(value);
            }
            self.gliding = true;
            return;
        }
        for (spring, value) in self.glide.iter_mut().zip(goal) {
            spring.animate_to(value, GLIDE);
        }
    }

    fn step(&mut self) {
        let now = Instant::now();
        // A long pause between frames counts as one short step, so the ghost never leaps.
        let dt = now.duration_since(self.last).as_secs_f32().min(1.0 / 20.0);
        self.last = now;
        self.shown.step(dt);
        self.target_shown.step(dt);
        self.others_shown.step(dt);
        for spring in &mut self.glide {
            spring.step(dt);
        }
    }

    fn paint(&mut self) {
        self.step();
        let alpha = self.shown.value.clamp(0.0, 1.0);
        let local = |rect: &Rect| self.screen.local(*rect);
        let others: Vec<Rect> = self.last_others.iter().map(local).collect();
        let others_alpha = alpha * self.others_shown.value.clamp(0.0, 1.0);
        let [x, y, w, h] = self.glide.map(|spring| spring.value);
        let target_alpha = alpha * self.target_shown.value.clamp(0.0, 1.0);
        let target = (self.gliding && self.last_target.is_some())
            .then(|| Rect::from_min_size(pos2(x, y), vec2(w.max(0.0), h.max(0.0))));
        self.gl.paint(|ui| {
            let painter = ui.painter();
            let radius = CornerRadius::same(RADIUS as u8);
            for rect in &others {
                painter.add(Shape::rect_filled(*rect, radius, faded(PEARL, FILL * 0.6 * others_alpha)));
                painter.add(Shape::rect_stroke(
                    *rect,
                    radius,
                    Stroke::new(1.5, faded(PEARL, OUTLINE * 0.6 * others_alpha)),
                    StrokeKind::Inside,
                ));
            }
            if let Some(rect) = target {
                painter.add(Shape::rect_filled(rect, radius, faded(PEARL, FILL * target_alpha)));
                painter.add(Shape::rect_stroke(
                    rect,
                    radius,
                    Stroke::new(2.0, faded(PEARL, OUTLINE * target_alpha)),
                    StrokeKind::Inside,
                ));
            }
        });
    }
}

fn faded(base: Color32, alpha: f32) -> Color32 {
    base.gamma_multiply(alpha.clamp(0.0, 1.0))
}

/// The ghost never takes the mouse, so the drag carries on underneath it.
fn pass_through(window: &winit::window::Window, monitor: &MonitorHandle) {
    platform::configure_dim_window(window, monitor);
    platform::set_click_through(window, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fading_keeps_hue_scales_opacity() {
        assert_eq!(faded(PEARL, 0.0).a(), 0);
        assert_eq!(faded(PEARL, 1.0), PEARL);
        assert!(faded(PEARL, 0.5).a() < 140 && faded(PEARL, 0.5).a() > 110);
    }
}
