//! Sequences: the file you tend to open after another. Each pair fades with time, and a full list drops the weakest.

use crate::find::context::Stamp;
use crate::find::key::Key;
use crate::find::weights::{PAIR_HALF_LIFE, PAIRS};

use super::decay::{decayed, share, since};

#[derive(Clone, Debug, PartialEq)]
pub struct Pair {
    pub before: Key,
    pub after: Key,
    pub weight: f32,
    pub last: Stamp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Follows {
    pub before: Key,
    pub weight: f32,
    pub share: f32,
}

#[derive(Default)]
pub struct Pairs {
    rows: Vec<Pair>,
}

impl Pair {
    pub fn weight_at(&self, now: Stamp) -> f32 {
        decayed(self.weight, since(now, self.last), PAIR_HALF_LIFE)
    }
}

impl Pairs {
    pub fn from_rows(mut rows: Vec<Pair>) -> Self {
        rows.retain(|row| row.before != row.after);
        rows.sort_by_key(|row| (row.before, row.after));
        rows.dedup_by_key(|row| (row.before, row.after));
        rows.truncate(PAIRS);
        Self { rows }
    }

    pub fn rows(&self) -> &[Pair] {
        &self.rows
    }

    pub fn learn(&mut self, before: Key, after: Key, weight: f32, now: Stamp) {
        if before == after {
            return;
        }
        match self.rows.binary_search_by_key(&(before, after), |row| (row.before, row.after)) {
            Ok(at) => {
                let row = &mut self.rows[at];
                row.weight = row.weight_at(now) + weight;
                row.last = now.max(row.last);
            }
            Err(at) => {
                self.rows.insert(at, Pair { before, after, weight, last: now });
                self.keep_within(PAIRS, at, now);
            }
        }
    }

    pub fn follows(&self, before: Key, after: Key, now: Stamp) -> Option<Follows> {
        let at = self.rows.binary_search_by_key(&(before, after), |row| (row.before, row.after)).ok()?;
        let weight = self.rows[at].weight_at(now);
        let from = self.rows.partition_point(|row| row.before < before);
        let after_the_same = self.rows[from..].iter().take_while(|row| row.before == before);
        let total: f32 = after_the_same.map(|row| row.weight_at(now)).sum();
        Some(Follows { before, weight, share: share(weight, total) })
    }

    pub fn forget(&mut self, gone: impl Fn(Key) -> bool) {
        self.rows.retain(|row| !gone(row.before) && !gone(row.after));
    }

    pub fn rekey(&mut self, renamed: impl Fn(Key) -> Option<Key>) {
        for row in &mut self.rows {
            row.before = renamed(row.before).unwrap_or(row.before);
            row.after = renamed(row.after).unwrap_or(row.after);
        }
        *self = Self::from_rows(std::mem::take(&mut self.rows));
    }

    fn keep_within(&mut self, most: usize, newest: usize, now: Stamp) {
        if self.rows.len() <= most {
            return;
        }
        let others = self.rows.iter().enumerate().filter(|(at, _)| *at != newest);
        let weakest = others.min_by(|a, b| a.1.weight_at(now).total_cmp(&b.1.weight_at(now))).map(|(at, _)| at);
        if let Some(weakest) = weakest {
            self.rows.remove(weakest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOON: Stamp = 1_704_110_400;
    const CARGO: Key = Key(1);
    const MAIN: Key = Key(2);
    const README: Key = Key(3);
    const CLOSE: f32 = 1e-3;

    #[test]
    fn usual_follower_gets_more() {
        let mut pairs = Pairs::default();
        (0..3).for_each(|_| pairs.learn(CARGO, MAIN, 1.0, NOON));
        pairs.learn(CARGO, README, 1.0, NOON);
        let main = pairs.follows(CARGO, MAIN, NOON).unwrap();
        assert_eq!((main.before, main.weight, main.share), (CARGO, 3.0, 0.75));
        assert_eq!(pairs.follows(CARGO, README, NOON).map(|follows| follows.share), Some(0.25));
        assert!(pairs.follows(MAIN, CARGO, NOON).is_none(), "a sequence has a direction");
        assert!(pairs.follows(README, MAIN, NOON).is_none());
    }

    #[test]
    fn sequence_fades_share_holds() {
        let mut pairs = Pairs::default();
        pairs.learn(CARGO, MAIN, 2.0, NOON);
        let later = pairs.follows(CARGO, MAIN, NOON + PAIR_HALF_LIFE).unwrap();
        assert!((later.weight - 1.0).abs() < CLOSE);
        assert_eq!(later.share, 1.0);
        pairs.learn(CARGO, MAIN, 1.0, NOON + PAIR_HALF_LIFE);
        assert!((pairs.rows()[0].weight - 2.0).abs() < CLOSE);
    }

    #[test]
    fn file_never_follows_itself() {
        let mut pairs = Pairs::default();
        pairs.learn(CARGO, CARGO, 1.0, NOON);
        assert!(pairs.rows().is_empty());
    }

    #[test]
    fn weakest_sequence_makes_room() {
        let mut pairs = Pairs::default();
        for serial in 0..PAIRS as u64 {
            pairs.learn(Key(serial + 10), MAIN, if serial == 5 { 0.5 } else { 2.0 }, NOON);
        }
        pairs.learn(CARGO, README, 0.1, NOON);
        assert_eq!(pairs.rows().len(), PAIRS);
        assert!(pairs.follows(CARGO, README, NOON).is_some());
        assert!(pairs.follows(Key(15), MAIN, NOON).is_none());
    }

    #[test]
    fn sequences_follow_renames() {
        let mut pairs = Pairs::default();
        pairs.learn(CARGO, MAIN, 1.0, NOON);
        pairs.learn(MAIN, README, 1.0, NOON);
        pairs.rekey(|key| (key == MAIN).then_some(Key(7)));
        assert!(pairs.follows(CARGO, Key(7), NOON).is_some() && pairs.follows(Key(7), README, NOON).is_some());
        pairs.forget(|key| key == README);
        assert_eq!(pairs.rows().len(), 1);
        pairs.forget(|key| key == CARGO);
        assert!(pairs.rows().is_empty());
    }
}
