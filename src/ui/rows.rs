//! List rows that animate as the list changes. New rows grow in place and gone ones fold away where they were.

use egui::{Rect, vec2};

use super::canvas::{Canvas, Mark, fade};
use super::spring::{Spring, SpringConfig, UNIT_PRECISION, smoothstep};

const GROW: SpringConfig = SpringConfig::new(0.32, 1.0);
const SHOWS_FROM: f32 = 0.35;

pub struct Rows<K, T> {
    rows: Vec<Row<K, T>>,
}

struct Row<K, T> {
    key: K,
    item: T,
    shown: Spring,
}

impl<K, T> Default for Rows<K, T> {
    fn default() -> Self {
        Self { rows: Vec::new() }
    }
}

impl<K: PartialEq + Clone, T> Rows<K, T> {
    /// Takes the new list. With `animate` off, rows simply appear and vanish.
    pub fn follow(&mut self, items: impl IntoIterator<Item = (K, T)>, animate: bool) {
        let order: Vec<K> = self.rows.iter().map(|row| row.key.clone()).collect();
        let mut old: Vec<Option<Row<K, T>>> = std::mem::take(&mut self.rows).into_iter().map(Some).collect();
        let mut rows = Vec::with_capacity(old.len());
        for (key, item) in items {
            let found = old.iter().position(|row| row.as_ref().is_some_and(|row| row.key == key));
            let row = match found.and_then(|at| old[at].take()) {
                Some(mut row) => {
                    row.item = item;
                    row
                }
                None => Row { key, item, shown: Spring::new(if animate { 0.0 } else { 1.0 }, GROW, UNIT_PRECISION) },
            };
            rows.push(row);
        }
        for row in &mut rows {
            row.shown.animate_to(1.0, GROW);
        }
        for (at, row) in old.into_iter().enumerate() {
            let Some(mut row) = row.filter(|_| animate) else { continue };
            row.shown.animate_to(0.0, GROW);
            let after = order[..at].iter().rev().find_map(|key| rows.iter().position(|row| row.key == *key));
            rows.insert(after.map_or(0, |at| at + 1), row);
        }
        self.rows = rows;
    }

    pub fn step(&mut self, dt: f32) {
        for row in &mut self.rows {
            row.shown.step(dt);
        }
        self.rows.retain(|row| row.shown.target > 0.0 || !row.shown.is_settled());
    }

    pub fn snap(&mut self) {
        for row in &mut self.rows {
            row.shown.snap_to(row.shown.target);
        }
        self.rows.retain(|row| row.shown.target > 0.0);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &T, f32)> {
        self.rows.iter().map(|row| (&row.key, &row.item, row.shown.value.clamp(0.0, 1.0)))
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn is_animating(&self) -> bool {
        self.rows.iter().any(|row| !row.shown.is_settled())
    }

    pub fn most_shown(&self) -> f32 {
        self.iter().map(|(_, _, shown)| shown).fold(0.0, f32::max)
    }
}

pub fn alpha(shown: f32) -> f32 {
    smoothstep(SHOWS_FROM, 1.0, shown)
}

pub fn reveal(canvas: &mut Canvas, mark: Mark, row: Rect, shown: f32) {
    if shown >= 1.0 {
        return;
    }
    for shape in canvas.since(mark) {
        fade(shape, alpha(shown));
    }
    canvas.clip(mark, Rect::from_min_size(row.min - vec2(1.0, 0.0), vec2(row.width() + 2.0, row.height())));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(rows: &mut Rows<u32, &'static str>) {
        for _ in 0..240 {
            rows.step(1.0 / 120.0);
        }
    }

    fn shown(rows: &Rows<u32, &'static str>) -> Vec<(u32, f32)> {
        rows.iter().map(|(key, _, shown)| (*key, (shown * 100.0).round() / 100.0)).collect()
    }

    #[test]
    fn first_rows_are_simply_there() {
        let mut rows = Rows::default();
        rows.follow([(1, "a"), (2, "b")], false);
        assert_eq!(shown(&rows), [(1, 1.0), (2, 1.0)]);
        assert!(!rows.is_animating());
    }

    #[test]
    fn rows_grow_and_fold_in_place() {
        let mut rows = Rows::default();
        rows.follow([(1, "a"), (2, "b"), (3, "c")], false);
        rows.follow([(1, "a"), (4, "d"), (3, "c")], true);
        let keys: Vec<u32> = rows.iter().map(|(key, _, _)| *key).collect();
        assert_eq!(keys, [1, 2, 4, 3], "the leaving row stays where it was, after its neighbour");
        assert_eq!(shown(&rows), [(1, 1.0), (2, 1.0), (4, 0.0), (3, 1.0)]);
        assert!(rows.is_animating());
        rows.step(1.0 / 60.0);
        let middle = shown(&rows);
        assert!(middle[1].1 < 1.0 && middle[2].1 > 0.0, "{middle:?}");
        settle(&mut rows);
        assert_eq!(shown(&rows), [(1, 1.0), (4, 1.0), (3, 1.0)]);
        assert!(!rows.is_animating());
    }

    #[test]
    fn returning_row_turns_around() {
        let mut rows = Rows::default();
        rows.follow([(1, "a"), (2, "b")], false);
        rows.follow([(1, "a")], true);
        rows.step(1.0 / 30.0);
        rows.follow([(1, "a"), (2, "b again")], true);
        assert_eq!(rows.iter().filter(|(key, _, _)| **key == 2).count(), 1);
        settle(&mut rows);
        assert_eq!(rows.iter().map(|(_, item, _)| *item).collect::<Vec<_>>(), ["a", "b again"]);
    }

    #[test]
    fn rows_drop_at_once_without_motion() {
        let mut rows = Rows::default();
        rows.follow([(1, "a"), (2, "b")], false);
        rows.follow([(2, "b")], false);
        assert_eq!(shown(&rows), [(2, 1.0)]);
        assert_eq!(alpha(1.0), 1.0);
        assert_eq!(alpha(0.2), 0.0, "words wait for room");
    }
}
