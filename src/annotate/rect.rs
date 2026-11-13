//! A rectangle of whole pixels, for the parts of the image a change touched.

use egui::{Pos2, Rect};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl PixelRect {
    pub const EMPTY: Self = Self { x: 0, y: 0, width: 0, height: 0 };

    pub fn of_image([width, height]: [usize; 2]) -> Self {
        Self { x: 0, y: 0, width, height }
    }

    pub fn covering(rect: Rect, image: [usize; 2]) -> Self {
        let [left, top] = [rect.min.x, rect.min.y].map(|edge| edge.floor().max(0.0) as usize);
        let [right, bottom] = [rect.max.x, rect.max.y].map(|edge| edge.ceil().max(0.0) as usize);
        Self::between([left, top], [right, bottom]).intersect(Self::of_image(image))
    }

    pub fn to_rect(self) -> Rect {
        Rect::from_min_max(
            Pos2::new(self.x as f32, self.y as f32),
            Pos2::new(self.right() as f32, self.bottom() as f32),
        )
    }

    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn origin(self) -> [usize; 2] {
        [self.x, self.y]
    }

    pub fn size(self) -> [usize; 2] {
        [self.width, self.height]
    }

    pub fn right(self) -> usize {
        self.x + self.width
    }

    pub fn bottom(self) -> usize {
        self.y + self.height
    }

    pub fn intersect(self, other: Self) -> Self {
        let from = [self.x.max(other.x), self.y.max(other.y)];
        Self::between(from, [self.right().min(other.right()), self.bottom().min(other.bottom())])
    }

    pub fn union(self, other: Self) -> Self {
        match (self.is_empty(), other.is_empty()) {
            (true, _) => other,
            (_, true) => self,
            _ => Self::between(
                [self.x.min(other.x), self.y.min(other.y)],
                [self.right().max(other.right()), self.bottom().max(other.bottom())],
            ),
        }
    }

    fn between([left, top]: [usize; 2], [right, bottom]: [usize; 2]) -> Self {
        if right <= left || bottom <= top {
            return Self::EMPTY;
        }
        Self { x: left, y: top, width: right - left, height: bottom - top }
    }
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    fn rect(x: usize, y: usize, width: usize, height: usize) -> PixelRect {
        PixelRect { x, y, width, height }
    }

    #[test]
    fn covers_every_touched_pixel() {
        let covered = PixelRect::covering(Rect::from_min_max(pos2(2.3, 4.0), pos2(5.1, 6.9)), [100, 100]);
        assert_eq!(covered, rect(2, 4, 4, 3));
        assert_eq!((covered.origin(), covered.size()), ([2, 4], [4, 3]));
        assert_eq!((covered.right(), covered.bottom()), (6, 7));
        assert_eq!(covered.to_rect(), Rect::from_min_max(pos2(2.0, 4.0), pos2(6.0, 7.0)));
        assert_eq!(PixelRect::covering(covered.to_rect(), [100, 100]), covered);
    }

    #[test]
    fn cut_at_the_image_edges() {
        let image = [10, 8];
        assert_eq!(PixelRect::covering(Rect::from_min_max(pos2(-5.0, -5.0), pos2(3.5, 2.0)), image), rect(0, 0, 4, 2));
        assert_eq!(PixelRect::covering(Rect::from_min_max(pos2(7.5, 6.5), pos2(90.0, 90.0)), image), rect(7, 6, 3, 2));
        assert_eq!(PixelRect::covering(Rect::EVERYTHING, image), PixelRect::of_image(image));
    }

    #[test]
    fn missing_the_image_covers_nothing() {
        let image = [10, 8];
        assert!(PixelRect::covering(Rect::from_min_max(pos2(20.0, 1.0), pos2(30.0, 4.0)), image).is_empty());
        assert!(PixelRect::covering(Rect::from_min_max(pos2(-9.0, -9.0), pos2(-1.0, -1.0)), image).is_empty());
        assert_eq!(PixelRect::covering(Rect::NOTHING, image), PixelRect::EMPTY);
        assert_eq!(PixelRect::covering(Rect::NAN, image), PixelRect::EMPTY);
        assert!(PixelRect::covering(Rect::EVERYTHING, [0, 0]).is_empty());
    }

    #[test]
    fn intersect_keeps_shared_pixels() {
        assert_eq!(rect(0, 0, 6, 6).intersect(rect(4, 2, 6, 2)), rect(4, 2, 2, 2));
        assert_eq!(rect(0, 0, 4, 4).intersect(rect(4, 0, 4, 4)), PixelRect::EMPTY);
        assert_eq!(rect(0, 0, 4, 4).intersect(PixelRect::EMPTY), PixelRect::EMPTY);
    }

    #[test]
    fn union_ignores_empty_rects() {
        assert_eq!(rect(1, 1, 2, 2).union(rect(6, 4, 2, 3)), rect(1, 1, 7, 6));
        assert_eq!(rect(1, 1, 2, 2).union(PixelRect::EMPTY), rect(1, 1, 2, 2));
        assert_eq!(PixelRect::EMPTY.union(rect(6, 4, 2, 3)), rect(6, 4, 2, 3));
        assert_eq!(rect(9, 9, 0, 5).union(rect(6, 4, 2, 3)), rect(6, 4, 2, 3));
    }
}
