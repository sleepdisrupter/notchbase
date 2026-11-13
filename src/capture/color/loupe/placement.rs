//! Where the loupe window goes. When it's hidden from captures it's centred on the cursor, otherwise it sits beside
//! the cursor and flips at the monitor's edges so it never covers the grab.

use egui::{Pos2, Rect, Vec2, pos2, vec2};

use super::GRAB;

/// The loupe window's size in points, with the lens at `CENTER`.
pub(in crate::capture::color) const WINDOW: Vec2 = vec2(240.0, 224.0);
pub(super) const CENTER: Pos2 = pos2(120.0, 112.0);

pub(in crate::capture::color) fn origin_for(
    cursor: [i32; 2],
    scale: f64,
    size: [i32; 2],
    excluded: bool,
    monitor: [i32; 4],
) -> [i32; 2] {
    if excluded {
        return [
            cursor[0] - (CENTER.x as f64 * scale).round() as i32,
            cursor[1] - (CENTER.y as f64 * scale).round() as i32,
        ];
    }
    let gap = GRAB as i32 / 2 + 4;
    let beside = |c: i32, len: i32, end: i32| if c + gap + len <= end { c + gap } else { c - gap - len };
    [beside(cursor[0], size[0], monitor[2]), beside(cursor[1], size[1], monitor[3])]
}

pub(in crate::capture::color) fn lens_center(cursor: [i32; 2], origin: [i32; 2], scale: f32, excluded: bool) -> Pos2 {
    if excluded {
        pos2(((cursor[0] - origin[0]) as f32 + 0.5) / scale, ((cursor[1] - origin[1]) as f32 + 0.5) / scale)
    } else {
        CENTER
    }
}

/// The cursor's monitor in window points, clipped to the window, which keeps the pill on screen.
pub(in crate::capture::color) fn monitor_bounds(monitor: [i32; 4], origin: [i32; 2], scale: f32, view: Vec2) -> Rect {
    let to_window = |x: i32, y: i32| pos2((x - origin[0]) as f32 / scale, (y - origin[1]) as f32 / scale);
    Rect::from_min_max(to_window(monitor[0], monitor[1]), to_window(monitor[2], monitor[3]))
        .intersect(Rect::from_min_size(Pos2::ZERO, view))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALES: [f32; 8] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0];

    #[test]
    fn centered_lens_sits_on_cursor_pixel() {
        for scale in SCALES {
            for cursor in [[0, 0], [1919, 1079], [-1280, 300], [5000, -20]] {
                let origin = origin_for(cursor, scale as f64, [0, 0], true, [0; 4]);
                let center = lens_center(cursor, origin, scale, true);
                let x = origin[0] as f32 + center.x * scale;
                let y = origin[1] as f32 + center.y * scale;
                assert!((x - (cursor[0] as f32 + 0.5)).abs() < 1e-2, "scale {scale}");
                assert!((y - (cursor[1] as f32 + 0.5)).abs() < 1e-2, "scale {scale}");
                assert!((center - CENTER).length() * scale <= 1.0);
            }
        }
    }

    #[test]
    fn window_beside_never_covers_grab() {
        let monitor = [0, 0, 1920, 1080];
        let size = [300, 280];
        for cursor in [[10, 10], [960, 540], [1910, 1070], [1910, 10], [10, 1070]] {
            let origin = origin_for(cursor, 1.25, size, false, monitor);
            let window =
                Rect::from_min_size(pos2(origin[0] as f32, origin[1] as f32), vec2(size[0] as f32, size[1] as f32));
            let half = (GRAB / 2) as f32;
            let grab = Rect::from_center_size(pos2(cursor[0] as f32, cursor[1] as f32), Vec2::splat(2.0 * half + 1.0));
            assert!(!window.intersects(grab), "cursor {cursor:?}: {window:?}");
            assert!(window.left() >= 0.0 && window.top() >= 0.0, "cursor {cursor:?}: {window:?}");
            assert!(window.right() <= 1920.0 && window.bottom() <= 1080.0, "cursor {cursor:?}: {window:?}");
        }
    }

    #[test]
    fn monitor_bounds_clip_to_window() {
        let view = WINDOW;
        let inside = monitor_bounds([-5000, -5000, 5000, 5000], [100, 100], 1.5, view);
        assert_eq!(inside, Rect::from_min_size(Pos2::ZERO, view));
        let edge = monitor_bounds([0, 0, 1920, 1080], [1800, 1000], 2.0, view);
        assert_eq!(edge, Rect::from_min_max(Pos2::ZERO, pos2(60.0, 40.0)));
    }
}
