//! What a tile shows while the shelf converts it: dimmed while it waits, then a progress ring and a badge.

use egui::{Color32, Pos2, Rect, Shape, Stroke, pos2, vec2};

use crate::ui::canvas::Canvas;
use crate::ui::color::{self, white};
use crate::ui::motion::Clock;
use crate::ui::sheen::{self, Band};
use crate::ui::spring::{lerp, smoothstep};
use crate::ui::{glyph, trace};

use super::work::{Phase, TileWork};

pub(super) const RING_GAP: f32 = 3.0;
const RING_WIDTH: f32 = 2.0;
const PROGRESS: f32 = 0.9;
const SWEEP_PERIOD: f32 = 1.6;
const BADGE_RADIUS: f32 = 7.5;

#[derive(Clone, Copy)]
pub(super) struct Content {
    pub(super) rect: Rect,
    pub(super) radius: f32,
}

impl Content {
    fn ring(self) -> (Rect, f32) {
        (self.rect.expand(RING_GAP), self.radius + RING_GAP)
    }

    fn badge_center(self) -> Pos2 {
        self.rect.right_bottom() - vec2(BADGE_RADIUS * 0.35, BADGE_RADIUS * 0.35)
    }
}

pub(super) fn paint_work(canvas: &mut Canvas, content: Content, work: TileWork, clock: Clock) {
    match work.phase {
        Phase::Waiting => {
            canvas.rect(content.rect, content.radius, Color32::from_black_alpha(110));
            paint_track(canvas, content, 1.0);
        }
        Phase::Active => {
            canvas.rect(content.rect, content.radius, Color32::from_black_alpha(70));
            paint_sweep(canvas, content, clock.phase(SWEEP_PERIOD), 1.0);
            paint_track(canvas, content, 1.0);
            paint_progress(canvas, content, work.progress, 1.0);
        }
        Phase::Complete => {
            paint_track(canvas, content, 1.0);
            paint_progress(canvas, content, work.progress, 1.0);
            paint_badge(canvas, content.badge_center(), 1.0, work.progress, color::GREEN);
        }
        Phase::Done { age, ok } => {
            let fading = 1.0 - smoothstep(0.5, 1.1, age);
            paint_track(canvas, content, fading);
            if ok {
                paint_progress(canvas, content, work.progress, fading);
                let pop = smoothstep(0.0, 0.22, age);
                let scale = lerp(0.4, 1.0, pop) + 0.18 * (pop * std::f32::consts::PI).sin();
                let alpha = pop * (1.0 - smoothstep(1.1, 1.5, age));
                paint_badge(canvas, content.badge_center(), scale, alpha, color::GREEN);
            }
        }
    }
}

fn paint_sweep(canvas: &mut Canvas, content: Content, phase: f32, alpha: f32) {
    let rect = content.rect;
    let travel = rect.width() * 0.8;
    let band = Band {
        center_x: lerp(rect.left() - travel, rect.right() + travel, phase),
        half_width: rect.width() * 0.32,
        slant: 0.55,
        color: white(0.42 * alpha),
    };
    canvas.add(Shape::mesh(sheen::clipped_band(rect, content.radius, band)));
}

fn paint_track(canvas: &mut Canvas, content: Content, alpha: f32) {
    let (ring, radius) = content.ring();
    canvas.rect_stroke(ring, radius, Stroke::new(RING_WIDTH, white(0.14 * alpha)));
}

fn paint_progress(canvas: &mut Canvas, content: Content, progress: f32, alpha: f32) {
    if progress <= 0.002 || alpha <= 0.001 {
        return;
    }
    let (ring, radius) = content.ring();
    let path = trace::leading_part(&trace::rounded_rect_loop(ring, radius), progress);
    canvas.add(Shape::line(path, Stroke::new(RING_WIDTH, white(PROGRESS * alpha))));
}

fn paint_badge(canvas: &mut Canvas, center: Pos2, scale: f32, alpha: f32, fill: Color32) {
    if alpha <= 0.001 {
        return;
    }
    let radius = BADGE_RADIUS * scale;
    canvas.circle(center, radius + 1.2, Color32::from_black_alpha((140.0 * alpha) as u8));
    canvas.circle(center, radius, fill.gamma_multiply(alpha));
    let check = pos2(center.x, center.y + radius * 0.04);
    glyph::checkmark(canvas, check, radius * 1.1, (radius * 0.24).max(1.2), Color32::WHITE.gamma_multiply(alpha));
}
