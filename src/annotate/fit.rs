//! Fits the screenshot into the window, never past one image pixel per screen pixel, and picks the window's size.

use egui::{Pos2, Rect, Vec2};

pub const SMALLEST_WINDOW: Vec2 = Vec2::new(660.0, 440.0);
const MONITOR_SHARE: f32 = 0.8;
const SMALLEST_SCALE: f32 = 1.0e-3;
const SMALLEST_ROOM: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    rect: Rect,
    scale: f32,
}

impl Fit {
    pub fn new(image: [usize; 2], available: Rect, pixels_per_point: f32) -> Self {
        let density = density(pixels_per_point);
        let pixels = pixel_size(image);
        let scale = scale_to_fit(pixels, available.size(), density);
        let size = pixels * scale;
        let corner = available.center() - size / 2.0;
        let snapped = Pos2::new((corner.x * density).round() / density, (corner.y * density).round() / density);
        Self { rect: Rect::from_min_size(snapped, size), scale }
    }

    pub fn rect(self) -> Rect {
        self.rect
    }

    pub fn scale(self) -> f32 {
        self.scale
    }

    pub fn to_image(self, point: Pos2) -> Pos2 {
        ((point - self.rect.min) / self.scale).to_pos2()
    }

    pub fn to_view(self, pixel: Pos2) -> Pos2 {
        self.rect.min + pixel.to_vec2() * self.scale
    }

    pub fn stroke_width(self, brush_size: f32) -> f32 {
        self.in_image_pixels(brush_size)
    }

    pub fn em(self, letter_size: f32) -> f32 {
        self.in_image_pixels(letter_size)
    }

    fn in_image_pixels(self, points: f32) -> f32 {
        points / self.scale
    }
}

/// The window's opening size: the image at full size plus the chrome, kept within 80% of the monitor.
pub fn window_size(image: [usize; 2], pixels_per_point: f32, monitor: Vec2, chrome: Vec2) -> Vec2 {
    let natural = pixel_size(image) / density(pixels_per_point);
    let room = (monitor * MONITOR_SHARE - chrome).max(Vec2::splat(SMALLEST_ROOM));
    let scale = (room.x / natural.x).min(room.y / natural.y).min(1.0);
    (natural * scale + chrome).ceil().max(SMALLEST_WINDOW)
}

fn scale_to_fit(pixels: Vec2, room: Vec2, density: f32) -> f32 {
    let fitting = (room.x / pixels.x).min(room.y / pixels.y);
    let fitting = if fitting.is_finite() { fitting } else { SMALLEST_SCALE };
    fitting.min(density.recip()).max(SMALLEST_SCALE)
}

fn pixel_size([width, height]: [usize; 2]) -> Vec2 {
    Vec2::new(width.max(1) as f32, height.max(1) as f32)
}

fn density(pixels_per_point: f32) -> f32 {
    if pixels_per_point.is_finite() && pixels_per_point > 0.0 { pixels_per_point } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::*;
    use crate::annotate::Brush;

    const CLOSE: f32 = 1.0e-3;

    fn room(width: f32, height: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 48.0), vec2(width, height))
    }

    fn is_on_the_pixel_grid(point: Pos2, pixels_per_point: f32) -> bool {
        [point.x, point.y]
            .into_iter()
            .all(|side| ((side * pixels_per_point).round() - side * pixels_per_point).abs() < CLOSE)
    }

    #[test]
    fn small_image_pixel_for_pixel() {
        let fit = Fit::new([400, 200], room(1000.0, 600.0), 2.0);
        assert_eq!(fit.scale(), 0.5);
        assert_eq!(fit.rect(), Rect::from_min_size(pos2(400.0, 298.0), vec2(200.0, 100.0)));
        assert_eq!(fit.rect().center(), room(1000.0, 600.0).center());
        let plain = Fit::new([400, 200], room(1000.0, 600.0), 1.0);
        assert_eq!((plain.scale(), plain.rect().size()), (1.0, vec2(400.0, 200.0)));
    }

    #[test]
    fn large_image_shrinks_to_fit() {
        let wide = Fit::new([3000, 1000], room(1000.0, 600.0), 2.0);
        assert!((wide.scale() - 1.0 / 3.0).abs() < CLOSE);
        assert!((wide.rect().width() - 1000.0).abs() < CLOSE && (wide.rect().height() - 333.333).abs() < CLOSE);
        let tall = Fit::new([1000, 3000], room(1000.0, 600.0), 2.0);
        assert!((tall.rect().height() - 600.0).abs() < CLOSE && (tall.rect().width() - 200.0).abs() < CLOSE);
        assert!(room(1000.0, 600.0).expand(0.5).contains_rect(wide.rect()));
        assert!(room(1000.0, 600.0).expand(0.5).contains_rect(tall.rect()));
    }

    #[test]
    fn never_blown_up_past_one_to_one() {
        for pixels_per_point in [1.0, 1.25, 1.5, 2.0, 3.0] {
            let fit = Fit::new([64, 64], room(1000.0, 600.0), pixels_per_point);
            assert!((fit.scale() * pixels_per_point - 1.0).abs() < CLOSE, "{pixels_per_point}");
        }
    }

    #[test]
    fn starts_on_a_whole_pixel() {
        for pixels_per_point in [1.0, 1.25, 1.5, 2.0, 3.0] {
            for image in [[401, 203], [399, 1], [3001, 997], [17, 2999]] {
                let fit = Fit::new(image, room(1001.0, 601.0), pixels_per_point);
                assert!(is_on_the_pixel_grid(fit.rect().min, pixels_per_point), "{image:?} at {pixels_per_point}");
            }
        }
    }

    #[test]
    fn view_and_image_map_exactly() {
        let fit = Fit::new([3000, 1000], room(1000.0, 600.0), 2.0);
        assert_eq!(fit.to_image(fit.rect().min), pos2(0.0, 0.0));
        assert!(fit.to_image(fit.rect().max).distance(pos2(3000.0, 1000.0)) < 0.01);
        assert!(fit.to_view(pos2(3000.0, 1000.0)).distance(fit.rect().max) < CLOSE);
        for pixel in [pos2(0.0, 0.0), pos2(1500.5, 250.25), pos2(2999.0, 999.0), pos2(-40.0, 1200.0)] {
            assert!(fit.to_image(fit.to_view(pixel)).distance(pixel) < 0.01, "{pixel:?}");
        }
        let exact = Fit::new([400, 200], room(1000.0, 600.0), 2.0);
        assert_eq!(exact.to_image(pos2(450.0, 348.0)), pos2(100.0, 100.0));
        assert_eq!(exact.to_view(pos2(100.0, 100.0)), pos2(450.0, 348.0));
    }

    #[test]
    fn brush_width_holds_at_any_zoom() {
        let exact = Fit::new([400, 200], room(1000.0, 600.0), 2.0);
        assert_eq!(exact.stroke_width(5.0), 10.0);
        let shrunk = Fit::new([4000, 2400], room(1000.0, 600.0), 2.0);
        assert_eq!(shrunk.scale(), 0.25);
        assert_eq!(shrunk.stroke_width(5.0), 20.0);
        assert_eq!(shrunk.stroke_width(5.0) * shrunk.scale(), 5.0);
    }

    #[test]
    fn letter_size_holds_at_any_zoom() {
        let letters = Brush::default().letter_size();
        let exact = Fit::new([400, 200], room(1000.0, 600.0), 2.0);
        assert_eq!((letters, exact.em(letters)), (20.0, 40.0));
        let shrunk = Fit::new([4000, 2400], room(1000.0, 600.0), 2.0);
        assert_eq!((shrunk.em(letters), shrunk.em(letters) * shrunk.scale()), (80.0, 20.0));
        assert_eq!(shrunk.em(5.0), shrunk.stroke_width(5.0), "the same division a stroke's width uses");
    }

    #[test]
    fn nonsense_gives_a_finite_fit() {
        let cramped = Fit::new([0, 0], Rect::from_min_size(pos2(10.0, 10.0), Vec2::ZERO), f32::NAN);
        assert!(cramped.scale() > 0.0 && cramped.rect().is_finite());
        assert!(cramped.to_image(pos2(11.0, 11.0)).is_finite() && cramped.stroke_width(5.0).is_finite());
        assert!(cramped.em(20.0).is_finite());
        let inside_out = Fit::new([800, 600], Rect::from_min_max(pos2(50.0, 50.0), pos2(10.0, 10.0)), 0.0);
        assert!(inside_out.scale() > 0.0 && inside_out.rect().is_finite());
    }

    #[test]
    fn window_is_image_plus_chrome() {
        let chrome = vec2(48.0, 112.0);
        assert_eq!(window_size([1600, 1000], 2.0, vec2(1512.0, 982.0), chrome), vec2(848.0, 612.0));
        assert_eq!(window_size([1000, 700], 1.0, vec2(1920.0, 1080.0), chrome), vec2(1048.0, 812.0));
    }

    #[test]
    fn large_image_window_fits_monitor() {
        let (monitor, chrome) = (vec2(1512.0, 982.0), vec2(48.0, 112.0));
        let size = window_size([3024, 1964], 2.0, monitor, chrome);
        assert!(
            size.x <= (monitor.x * MONITOR_SHARE).ceil() && size.y <= (monitor.y * MONITOR_SHARE).ceil(),
            "{size:?}"
        );
        let image_room = size - chrome;
        assert!((image_room.x / image_room.y - 3024.0 / 1964.0).abs() < 0.01, "{image_room:?}");
        assert!((image_room.y - (monitor.y * MONITOR_SHARE - chrome.y)).abs() <= 1.0, "the height is what limits it");
        let fit = Fit::new([3024, 1964], Rect::from_min_size(Pos2::ZERO, image_room), 2.0);
        assert!((fit.rect().width() - image_room.x).abs() <= 1.0);
    }

    #[test]
    fn window_fits_its_toolbar() {
        let chrome = vec2(48.0, 112.0);
        assert_eq!(window_size([40, 30], 2.0, vec2(1512.0, 982.0), chrome), SMALLEST_WINDOW);
        assert_eq!(window_size([4000, 40], 2.0, vec2(1512.0, 982.0), chrome).y, SMALLEST_WINDOW.y);
        assert_eq!(window_size([0, 0], f32::NAN, Vec2::ZERO, Vec2::ZERO), SMALLEST_WINDOW);
    }
}
