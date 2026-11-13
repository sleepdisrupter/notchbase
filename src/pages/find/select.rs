//! The selected row and the highlight that glides to it.

use crate::bridge::FindRow;
use crate::ui::motion;
use crate::ui::spring::{POINT_PRECISION, Spring};

use super::hold::Taken;

pub(super) struct Select {
    pub(super) index: usize,
    key: Option<u64>,
    moved: bool,
    glide: Spring,
    placed: bool,
}

impl Default for Select {
    fn default() -> Self {
        Self {
            index: 0,
            key: None,
            moved: false,
            glide: Spring::new(0.0, motion::FOLLOW, POINT_PRECISION),
            placed: false,
        }
    }
}

impl Select {
    pub(super) fn reset(&mut self) {
        self.index = 0;
        self.moved = false;
    }

    pub(super) fn pin(&mut self, index: usize) {
        self.index = index;
        self.moved = true;
    }

    pub(super) fn answer(&mut self, rows: &[FindRow], taken: Taken) {
        if taken == Taken::Fresh {
            let survivor = self.key.filter(|_| self.moved).and_then(|key| rows.iter().position(|row| row.key == key));
            self.moved = survivor.is_some();
            self.index = survivor.unwrap_or(0);
        }
        self.clamp(rows);
    }

    pub(super) fn step(&mut self, by: isize, rows: &[FindRow]) {
        self.index = self.index.saturating_add_signed(by);
        self.moved = true;
        self.clamp(rows);
    }

    pub(super) fn point(&mut self, index: usize, pointer_moved: bool, rows: &[FindRow]) -> bool {
        let before = self.index;
        if pointer_moved && index != self.index {
            self.index = index;
            self.moved = true;
            self.clamp(rows);
        }
        self.index != before
    }

    pub(super) fn glide(&mut self, row_h: f32, dt: f32) -> f32 {
        let target = self.index as f32 * row_h;
        if !std::mem::replace(&mut self.placed, true) {
            self.glide.snap_to(target);
        }
        self.glide.animate_to(target, motion::FOLLOW);
        self.glide.step(dt);
        self.glide.value
    }

    pub(super) fn unplace(&mut self) {
        self.placed = false;
    }

    pub(super) fn is_animating(&self) -> bool {
        !self.glide.is_settled()
    }

    fn clamp(&mut self, rows: &[FindRow]) {
        self.index = self.index.min(rows.len().saturating_sub(1));
        self.key = rows.get(self.index).map(|row| row.key);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use super::*;
    use crate::bridge::Kind;

    fn rows(keys: &[u64]) -> Vec<FindRow> {
        keys.iter()
            .map(|&key| FindRow {
                key,
                path: Arc::from(Path::new("/x")),
                name: "x".into(),
                place: "~".into(),
                marks: Vec::new(),
                kind: Kind::Other,
                app: false,
                folder: false,
                cloud: false,
                modified: 0,
                why: None,
                predicted: false,
                icon: None,
            })
            .collect()
    }

    #[test]
    fn arrows_move_within_rows() {
        let list = rows(&[1, 2, 3]);
        let mut select = Select::default();
        select.answer(&list, Taken::Fresh);
        select.step(-1, &list);
        assert_eq!(select.index, 0);
        select.step(1, &list);
        select.step(1, &list);
        select.step(1, &list);
        assert_eq!(select.index, 2);
    }

    #[test]
    fn new_answer_starts_at_top() {
        let mut select = Select::default();
        select.answer(&rows(&[1, 2, 3]), Taken::Fresh);
        select.step(1, &rows(&[1, 2, 3]));
        select.answer(&rows(&[5, 2]), Taken::Fresh);
        assert_eq!(select.index, 1, "row 2 was chosen and is still there");
        select.answer(&rows(&[7, 8]), Taken::Fresh);
        assert_eq!(select.index, 0);
        select.step(1, &rows(&[7, 8]));
        select.reset();
        select.answer(&rows(&[7, 8]), Taken::Fresh);
        assert_eq!(select.index, 0, "typing gives the choice back to the top row");
    }

    #[test]
    fn only_moved_pointer_picks() {
        let list = rows(&[1, 2, 3]);
        let mut select = Select::default();
        select.answer(&list, Taken::Fresh);
        select.point(2, false, &list);
        assert_eq!(select.index, 0);
        select.point(2, true, &list);
        assert_eq!(select.index, 2);
    }

    #[test]
    fn highlight_lands_then_glides() {
        let mut select = Select::default();
        select.pin(2);
        assert_eq!(select.glide(52.0, 1.0 / 60.0), 104.0);
        select.index = 0;
        let between = select.glide(52.0, 1.0 / 60.0);
        assert!(between > 0.0 && between < 104.0 && select.is_animating());
    }
}
