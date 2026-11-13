//! Hovering a finished run unfolds its whole message, and leaving folds it back.

use std::collections::HashMap;

use crate::ui::motion;
use crate::ui::spring::{POINT_PRECISION, Spring};

#[derive(Default)]
pub(super) struct Unfold {
    rows: HashMap<u64, Spring>,
    pinned: Option<u64>,
}

impl Unfold {
    pub(super) fn pin(&mut self, id: u64) {
        self.pinned = Some(id);
    }

    pub(super) fn extra(&self, id: u64) -> f32 {
        self.rows.get(&id).map_or(0.0, |spring| spring.value.max(0.0))
    }

    pub(super) fn total(&self, ids: impl IntoIterator<Item = u64>) -> f32 {
        ids.into_iter().map(|id| self.extra(id)).sum()
    }

    pub(super) fn is_animating(&self) -> bool {
        self.rows.values().any(|spring| !spring.is_settled())
    }

    pub(super) fn follow(&mut self, rows: &[(u64, f32)], hovered: Option<u64>, still: bool) {
        let open = self.pinned.or(hovered);
        let snap = still || self.pinned.is_some();
        self.rows.retain(|id, _| rows.iter().any(|(row, _)| row == id));
        for &(id, full) in rows {
            let target = if open == Some(id) { full } else { 0.0 };
            let spring = self.rows.entry(id).or_insert_with(|| Spring::new(0.0, motion::SMOOTH, POINT_PRECISION));
            if snap {
                spring.snap_to(target);
            } else {
                spring.animate_to(target, motion::SMOOTH);
            }
        }
    }

    pub(super) fn step(&mut self, dt: f32) {
        for spring in self.rows.values_mut() {
            spring.step(dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: f32 = 12.0;
    const ROWS: [(u64, f32); 3] = [(1, 2.0 * LINE), (2, LINE), (3, 0.0)];

    fn settle(unfold: &mut Unfold, hovered: Option<u64>) {
        for _ in 0..240 {
            unfold.follow(&ROWS, hovered, false);
            unfold.step(1.0 / 60.0);
        }
    }

    #[test]
    fn nothing_grows_without_a_hover() {
        let mut unfold = Unfold::default();
        settle(&mut unfold, None);
        assert_eq!(unfold.total([1, 2, 3]), 0.0);
        assert!(!unfold.is_animating());
    }

    #[test]
    fn hovered_row_grows_and_springs_back() {
        let mut unfold = Unfold::default();
        unfold.follow(&ROWS, Some(1), false);
        unfold.step(1.0 / 60.0);
        assert!(unfold.is_animating());
        assert!(unfold.extra(1) > 0.0 && unfold.extra(1) < 2.0 * LINE);
        settle(&mut unfold, Some(1));
        assert_eq!((unfold.extra(1), unfold.extra(2), unfold.extra(3)), (2.0 * LINE, 0.0, 0.0));
        assert_eq!(unfold.total([1, 2, 3]), 2.0 * LINE);
        settle(&mut unfold, Some(2));
        assert_eq!((unfold.extra(1), unfold.extra(2)), (0.0, LINE));
        settle(&mut unfold, None);
        assert_eq!(unfold.total([1, 2, 3]), 0.0);
    }

    #[test]
    fn one_line_never_grows() {
        let mut unfold = Unfold::default();
        unfold.follow(&ROWS, Some(3), false);
        assert!(!unfold.is_animating());
        settle(&mut unfold, Some(3));
        assert_eq!(unfold.total([1, 2, 3]), 0.0);
    }

    #[test]
    fn still_page_snaps_shut() {
        let mut unfold = Unfold::default();
        settle(&mut unfold, Some(1));
        unfold.follow(&ROWS, None, true);
        assert_eq!(unfold.extra(1), 0.0);
        assert!(!unfold.is_animating());
    }

    #[test]
    fn pinned_row_opens_at_once() {
        let mut unfold = Unfold::default();
        unfold.pin(2);
        unfold.follow(&ROWS, Some(1), true);
        assert_eq!((unfold.extra(1), unfold.extra(2)), (0.0, LINE));
        unfold.follow(&ROWS[..1], None, false);
        assert_eq!(unfold.extra(2), 0.0);
        assert_eq!(unfold.rows.len(), 1);
    }
}
