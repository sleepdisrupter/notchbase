//! Selections in physical pixels, and the mapping between pointer points and pixels on one monitor.

use egui::{Pos2, Rect, Vec2, pos2};
use winit::dpi::PhysicalPosition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PixelRect {
    pub(super) x0: i64,
    pub(super) y0: i64,
    pub(super) x1: i64,
    pub(super) y1: i64,
}

impl PixelRect {
    pub(super) fn full(size: [usize; 2]) -> Self {
        Self { x0: 0, y0: 0, x1: size[0] as i64, y1: size[1] as i64 }
    }

    pub(super) fn width(self) -> i64 {
        self.x1 - self.x0
    }

    pub(super) fn height(self) -> i64 {
        self.y1 - self.y0
    }

    pub(super) fn to_points(self, view: Vec2, size: [usize; 2]) -> Rect {
        let (sx, sy) = (view.x / size[0] as f32, view.y / size[1] as f32);
        Rect::from_min_max(
            pos2(self.x0 as f32 * sx, self.y0 as f32 * sy),
            pos2(self.x1 as f32 * sx, self.y1 as f32 * sy),
        )
    }
}

/// The pixel under a point. The small nudge keeps a cursor on a pixel's edge from landing in the one before.
pub(super) fn pixel_at(pos: Pos2, view: Vec2, size: [usize; 2]) -> [i64; 2] {
    let axis = |p: f32, v: f32, n: usize| {
        let x = (p as f64 / v.max(1e-3) as f64 * n as f64 + 1e-2).floor() as i64;
        x.clamp(0, n as i64 - 1)
    };
    [axis(pos.x, view.x, size[0]), axis(pos.y, view.y, size[1])]
}

/// The pixel corner nearest a point, which is where a drag's edge lands.
pub(super) fn corner_at(pos: Pos2, view: Vec2, size: [usize; 2]) -> [i64; 2] {
    let axis = |p: f32, v: f32, n: usize| {
        let x = (p as f64 / v.max(1e-3) as f64 * n as f64).round() as i64;
        x.clamp(0, n as i64)
    };
    [axis(pos.x, view.x, size[0]), axis(pos.y, view.y, size[1])]
}

/// The rectangle between two corners, clamped to the monitor. `square` makes it the largest square that fits.
pub(super) fn selection(anchor: [i64; 2], current: [i64; 2], square: bool, size: [usize; 2]) -> PixelRect {
    let (w, h) = (size[0] as i64, size[1] as i64);
    let a = [anchor[0].clamp(0, w), anchor[1].clamp(0, h)];
    let c = [current[0].clamp(0, w), current[1].clamp(0, h)];
    let (mut dx, mut dy) = (c[0] - a[0], c[1] - a[1]);
    if square {
        let room_x = if dx < 0 { a[0] } else { w - a[0] };
        let room_y = if dy < 0 { a[1] } else { h - a[1] };
        let side = dx.abs().max(dy.abs()).min(room_x).min(room_y);
        dx = if dx < 0 { -side } else { side };
        dy = if dy < 0 { -side } else { side };
    }
    let (bx, by) = (a[0] + dx, a[1] + dy);
    PixelRect { x0: a[0].min(bx), y0: a[1].min(by), x1: a[0].max(bx), y1: a[1].max(by) }
}

/// A cursor position inside the pixel, not on its edge, so it maps back to that pixel at any scale.
pub(super) fn cursor_position_for(px: [i64; 2]) -> PhysicalPosition<f64> {
    PhysicalPosition::new(px[0] as f64 + 0.25, px[1] as f64 + 0.25)
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    const SCALES: [f32; 8] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0];
    const SIZE: [usize; 2] = [1920, 1080];

    #[test]
    fn whole_pixel_cursor_maps_to_itself() {
        for scale in SCALES {
            for (w, h) in [(1366usize, 768usize), (1920, 1080), (3024, 1964), (3840, 2160), (7680, 4320)] {
                let view = vec2(w as f32 / scale, h as f32 / scale);
                for x in (0..w).step_by(3).chain([w - 1]) {
                    let y = x * h / w;
                    let pos = pos2(x as f32 / scale, y as f32 / scale);
                    assert_eq!(pixel_at(pos, view, [w, h]), [x as i64, y as i64], "scale {scale}, {w}x{h}");
                    assert_eq!(corner_at(pos, view, [w, h]), [x as i64, y as i64], "scale {scale}, {w}x{h}");
                }
            }
        }
    }

    #[test]
    fn pointer_clamps_to_monitor() {
        let (view, size) = (vec2(100.0, 50.0), [200, 100]);
        assert_eq!(pixel_at(pos2(-5.0, -1.0), view, size), [0, 0]);
        assert_eq!(pixel_at(pos2(100.0, 50.0), view, size), [199, 99]);
        assert_eq!(corner_at(pos2(-5.0, -1.0), view, size), [0, 0]);
        assert_eq!(corner_at(pos2(140.0, 50.0), view, size), [200, 100]);
    }

    #[test]
    fn nudged_cursor_lands_on_target() {
        for px in [[0, 0], [10, 20], [1279, 719], [2559, 1439]] {
            let p = cursor_position_for(px);
            assert_eq!([p.x.round() as i64, p.y.round() as i64], px);
            for scale in SCALES {
                let view = vec2(2560.0 / scale, 1440.0 / scale);
                let pos = pos2((p.x / scale as f64) as f32, (p.y / scale as f64) as f32);
                assert_eq!(pixel_at(pos, view, [2560, 1440]), px);
            }
        }
    }

    #[test]
    fn selection_normalizes_any_drag() {
        let expected = PixelRect { x0: 100, y0: 50, x1: 300, y1: 250 };
        assert_eq!(selection([100, 50], [300, 250], false, SIZE), expected);
        assert_eq!(selection([300, 250], [100, 50], false, SIZE), expected);
        assert_eq!(selection([300, 50], [100, 250], false, SIZE), expected);
        let empty = selection([7, 9], [7, 9], false, SIZE);
        assert_eq!((empty.width(), empty.height()), (0, 0));
    }

    #[test]
    fn selection_is_clamped_to_its_monitor() {
        let rect = selection([1800, 1000], [2500, -40], false, SIZE);
        assert_eq!(rect, PixelRect { x0: 1800, y0: 0, x1: 1920, y1: 1000 });
    }

    #[test]
    fn shift_takes_largest_fitting_square() {
        assert_eq!(selection([100, 100], [150, 130], true, SIZE), PixelRect { x0: 100, y0: 100, x1: 150, y1: 150 });
        assert_eq!(selection([500, 500], [420, 560], true, SIZE), PixelRect { x0: 420, y0: 500, x1: 500, y1: 580 });
        assert_eq!(selection([10, 500], [-200, 900], true, SIZE), PixelRect { x0: 0, y0: 500, x1: 10, y1: 510 });
        assert_eq!(
            selection([1900, 1000], [1950, 1200], true, SIZE),
            PixelRect { x0: 1900, y0: 1000, x1: 1920, y1: 1020 }
        );
    }

    #[test]
    fn selection_maps_to_whole_pixels() {
        let rect = PixelRect { x0: 101, y0: 33, x1: 250, y1: 1080 };
        for scale in SCALES {
            let view = vec2(SIZE[0] as f32 / scale, SIZE[1] as f32 / scale);
            let points = rect.to_points(view, SIZE);
            for (v, expected) in
                [(points.min.x, 101.0), (points.min.y, 33.0), (points.max.x, 250.0), (points.max.y, 1080.0)]
            {
                assert!((v * scale - expected).abs() < 1e-2, "scale {scale}");
            }
        }
    }
}
