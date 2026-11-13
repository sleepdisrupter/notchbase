//! The line being drawn, from pointer samples kept on the picture, and the ring that shows the brush under the pointer.

use egui::{Color32, Event, Painter, PointerButton, Pos2, Stroke as Line};

use crate::annotate::{Brush, Fit, Stroke};
use crate::ui::color;

const SPARSEST: f32 = 0.75;
const HALO: f32 = 0.3;
const HALO_WIDTH: f32 = 3.2;
const RING: f32 = 0.96;
const RING_WIDTH: f32 = 1.2;
const SMALLEST_RING: f32 = 1.0;

pub(super) struct Pen {
    color: Color32,
    width: f32,
    samples: Vec<Pos2>,
}

impl Pen {
    fn start(brush: Brush, fit: Fit, at: Pos2) -> Self {
        Self { color: brush.color(), width: fit.stroke_width(brush.size()), samples: vec![on_image(fit, at)] }
    }

    pub(super) fn stroke(&self) -> Option<Stroke> {
        Stroke::from_samples(self.color, self.width, &self.samples)
    }

    pub(super) fn paint(&self, painter: &Painter, fit: Fit) {
        let Some(stroke) = self.stroke() else { return };
        let painter = painter.with_clip_rect(fit.rect().intersect(painter.clip_rect()));
        let width = stroke.width() * fit.scale();
        let points = stroke.points();
        let mut inked: Option<Pos2> = None;
        for (index, pixel) in points.iter().enumerate() {
            let at = fit.to_view(*pixel);
            let is_last = index + 1 == points.len();
            if let Some(from) = inked {
                if !is_last && from.distance(at) < SPARSEST {
                    continue;
                }
                painter.line_segment([from, at], Line::new(width, self.color));
            }
            painter.circle_filled(at, width / 2.0, self.color);
            inked = Some(at);
        }
    }
}

pub(super) struct Hand<'a> {
    pub(super) pen: &'a mut Option<Pen>,
    pub(super) brush: Brush,
    pub(super) fit: Fit,
    pub(super) ready: bool,
}

impl Hand<'_> {
    pub(super) fn trace(mut self, ui: &egui::Ui) -> Vec<Stroke> {
        ui.input(|input| {
            let mut finished: Vec<Stroke> = input.events.iter().filter_map(|event| self.follow(event)).collect();
            if !input.pointer.primary_down() {
                finished.extend(self.lift());
            }
            finished
        })
    }

    fn follow(&mut self, event: &Event) -> Option<Stroke> {
        match *event {
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } => {
                if self.ready && self.pen.is_none() && starts_at(self.fit, pos) {
                    *self.pen = Some(Pen::start(self.brush, self.fit, pos));
                }
                None
            }
            Event::PointerMoved(pos) => {
                self.reach(pos);
                None
            }
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, .. } => {
                self.reach(pos);
                self.lift()
            }
            _ => None,
        }
    }

    fn reach(&mut self, to: Pos2) {
        if let Some(pen) = self.pen.as_mut() {
            pen.samples.push(on_image(self.fit, to));
        }
    }

    fn lift(&mut self) -> Option<Stroke> {
        self.pen.take()?.stroke()
    }
}

pub(super) fn starts_at(fit: Fit, press: Pos2) -> bool {
    fit.rect().contains(press)
}

fn on_image(fit: Fit, point: Pos2) -> Pos2 {
    fit.to_image(point.clamp(fit.rect().min, fit.rect().max))
}

pub(super) fn paint_ring(painter: &Painter, at: Pos2, brush: Brush, presence: f32) {
    let radius = (brush.size() / 2.0).max(SMALLEST_RING);
    for line in halo_and_ring(presence) {
        painter.circle_stroke(at, radius, line);
    }
}

pub(super) fn halo_and_ring(presence: f32) -> [Line; 2] {
    let halo = Color32::from_black_alpha((HALO * presence * 255.0).round() as u8);
    [Line::new(HALO_WIDTH, halo), Line::new(RING_WIDTH, color::white(RING * presence))]
}
