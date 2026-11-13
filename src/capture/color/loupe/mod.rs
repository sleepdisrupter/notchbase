//! The loupe drawn around the cursor on Windows: a round lens of magnified pixels with a faint grid, and a pill
//! with a swatch and the hex value. It pops in when opened and presses in when you pick.

mod draw;
mod lens;
mod pill;
mod placement;
mod tint;
mod zoom;

use crate::capture::overlay::glide::Glide;
use crate::ui::motion;
use crate::ui::spring::{Spring, SpringConfig};
use tint::Tint;
use zoom::{DEFAULT_CELL, cell_target, next_zoom};

pub(super) use draw::Scene;
pub(super) use placement::{WINDOW, lens_center, monitor_bounds, origin_for};

/// Side of the square of screen pixels grabbed around the cursor.
pub(super) const GRAB: usize = 31;

const APPEAR: SpringConfig = SpringConfig::new(0.32, 0.72);
const ZOOM: SpringConfig = SpringConfig::new(0.26, 0.9);

pub(super) struct Loupe {
    scale: Spring,
    zoom: Spring,
    zoom_log: f32,
    flip: Glide,
    ring_tint: Tint,
    pill_tint: Tint,
}

impl Loupe {
    pub(super) fn new() -> Self {
        Self {
            scale: Spring::new(1.0, APPEAR, 0.0005),
            zoom: Spring::new(DEFAULT_CELL, ZOOM, 0.005),
            zoom_log: DEFAULT_CELL.log2(),
            flip: Glide::new(motion::SMOOTH, 0.002),
            ring_tint: Tint::default(),
            pill_tint: Tint::default(),
        }
    }

    pub(super) fn open(&mut self, ppp: f32) {
        self.scale.snap_to(0.6);
        self.scale.animate_to(1.0, APPEAR);
        self.zoom.snap_to(cell_target(self.zoom_log, ppp));
        self.flip.reset();
        self.ring_tint = Tint::default();
        self.pill_tint = Tint::default();
    }

    pub(super) fn zoom_by(&mut self, notches: f32, ppp: f32) {
        if notches == 0.0 {
            return;
        }
        let (zoom_log, target) = next_zoom(self.zoom_log, notches, self.zoom.target, ppp);
        self.zoom_log = zoom_log;
        self.zoom.animate_to(target, ZOOM);
    }

    pub(super) fn press(&mut self) {
        self.scale.animate_to(0.9, motion::PRESS);
    }

    pub(super) fn dismiss(&mut self) {
        self.scale.animate_to(0.94, motion::HOVER);
    }
}
