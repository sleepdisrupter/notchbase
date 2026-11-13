//! The shortlist: the best few hundred candidates, with a floor that rises as better ones come in.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use crate::find::model::EntryId;
use crate::find::query::Fit;

#[derive(Clone, Copy, Debug)]
pub struct Scored {
    pub score: f32,
    pub modified: u32,
    pub id: EntryId,
    pub fit: Fit,
}

impl Ord for Scored {
    fn cmp(&self, other: &Self) -> Ordering {
        let newer = || self.modified.cmp(&other.modified);
        self.score.total_cmp(&other.score).then_with(newer).then_with(|| other.id.cmp(&self.id))
    }
}

impl PartialOrd for Scored {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Scored {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Scored {}

pub struct Shortlist {
    kept: BinaryHeap<Reverse<Scored>>,
    most: usize,
}

impl Shortlist {
    pub fn new(most: usize) -> Self {
        Self { kept: BinaryHeap::with_capacity(most + 1), most }
    }

    pub fn floor(&self) -> Option<f32> {
        self.kept.peek().filter(|_| self.kept.len() >= self.most).map(|Reverse(worst)| worst.score)
    }

    pub fn offer(&mut self, found: Scored) {
        self.kept.push(Reverse(found));
        if self.kept.len() > self.most {
            self.kept.pop();
        }
    }

    pub fn absorb(&mut self, other: Shortlist) {
        self.extend(other.kept.into_iter().map(|Reverse(found)| found));
    }

    pub fn into_vec(self) -> Vec<Scored> {
        self.kept.into_iter().map(|Reverse(found)| found).collect()
    }
}

impl Extend<Scored> for Shortlist {
    fn extend<Found: IntoIterator<Item = Scored>>(&mut self, found: Found) {
        for found in found {
            self.offer(found);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(score: f32, modified: u32, id: EntryId) -> Scored {
        Scored { score, modified, id, fit: Fit::default() }
    }

    fn ids(list: Shortlist) -> Vec<EntryId> {
        let mut kept = list.into_vec();
        kept.sort_by(|one, other| other.cmp(one));
        kept.into_iter().map(|found| found.id).collect()
    }

    #[test]
    fn keeps_best_floor_is_worst() {
        let mut list = Shortlist::new(3);
        assert_eq!(list.floor(), None, "nothing is turned away while there is room");
        for (score, id) in [(5.0, 1), (9.0, 2)] {
            list.offer(found(score, 0, id));
        }
        assert_eq!(list.floor(), None);
        list.offer(found(7.0, 0, 3));
        assert_eq!(list.floor(), Some(5.0));
        list.offer(found(6.0, 0, 4));
        list.offer(found(1.0, 0, 5));
        assert_eq!(list.floor(), Some(6.0));
        assert_eq!(ids(list), [2, 3, 4]);
    }

    #[test]
    fn tie_goes_newer_then_first() {
        let mut list = Shortlist::new(2);
        for (modified, id) in [(10, 7), (30, 8), (30, 4), (20, 9)] {
            list.offer(found(1.0, modified, id));
        }
        assert_eq!(ids(list), [4, 8]);
    }

    #[test]
    fn lists_merge_best_of_both() {
        let (mut one, mut other) = (Shortlist::new(2), Shortlist::new(2));
        one.offer(found(1.0, 0, 1));
        one.offer(found(4.0, 0, 2));
        other.offer(found(3.0, 0, 3));
        other.offer(found(2.0, 0, 4));
        one.absorb(other);
        assert_eq!(ids(one), [2, 3]);
    }
}
