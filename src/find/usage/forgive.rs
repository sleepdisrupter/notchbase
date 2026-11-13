//! Skips. A prediction that was shown and not picked counts against its file, the fourth skip benches the file until
//! it is opened again, and a skip is forgiven every seven days.

use crate::find::context::{DAY, Stamp};
use crate::find::key::Key;
use crate::find::weights::{SKIP_FORGIVEN_EVERY, SKIPS_OUT};

use super::model::Model;
use super::prefix::Typed;
use super::record::{FileRecord, day_of, state};

const DAYS_PER_FORGIVEN_SKIP: u16 = (SKIP_FORGIVEN_EVERY / DAY) as u16;

impl FileRecord {
    pub fn skips_on(&self, day: u16) -> u8 {
        let forgiven = day.saturating_sub(self.day) / DAYS_PER_FORGIVEN_SKIP;
        self.skips.saturating_sub(u8::try_from(forgiven).unwrap_or(u8::MAX))
    }

    pub fn benched(&self) -> bool {
        self.flags & state::BENCHED != 0
    }

    pub fn skip(&mut self, day: u16) {
        self.skips = self.skips_on(day).saturating_add(1).min(SKIPS_OUT);
        self.day = day;
        if self.skips >= SKIPS_OUT {
            self.flags |= state::BENCHED;
        }
    }

    pub fn forgive(&mut self, day: u16) {
        self.skips = self.skips_on(day) / 2;
        self.day = day;
        self.flags &= !state::BENCHED;
    }
}

impl Model {
    pub(super) fn passed_by(&mut self, predicted: &[Key], picked: Option<Key>, now: Stamp) {
        let day = day_of(now);
        for key in predicted.iter().filter(|key| Some(**key) != picked) {
            let Some(record) = self.files.get_mut(*key).filter(|record| !record.gone()) else { continue };
            record.skip(day);
            self.book.dirty = true;
        }
    }

    pub(super) fn passed_over(&mut self, typed: Typed, picked: Key, rank: u8, now: Stamp) {
        if rank > 0 {
            self.prefixes.pass_over(typed, picked, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::find::key::{START, key_of};

    const NOON: Stamp = 1_704_110_400;

    #[test]
    fn fourth_skip_benches_a_file() {
        let mut record = FileRecord::new(START);
        let day = day_of(NOON);
        (0..3).for_each(|_| record.skip(day));
        assert!(!record.benched() && record.skips_on(day) == 3);
        record.skip(day);
        record.skip(day);
        assert!(record.benched() && record.skips == SKIPS_OUT);
        assert!(record.benched() && record.skips_on(day + 70) == 0, "time forgives the count, not the bench");
        record.forgive(day);
        assert!(!record.benched() && record.skips == 2);
    }

    #[test]
    fn skip_forgiven_every_week() {
        let mut record = FileRecord::new(START);
        let day = day_of(NOON);
        (0..3).for_each(|_| record.skip(day));
        assert_eq!((record.skips_on(day + 6), record.skips_on(day + 7), record.skips_on(day + 15)), (3, 2, 1));
        record.skip(day + 15);
        assert_eq!((record.skips, record.day), (2, day + 15));
        assert_eq!(record.skips_on(day), 2, "a day before the last skip forgives nothing");
    }

    #[test]
    fn only_shown_unpicked_skip() {
        let mut model = Model::default();
        let paths = ["/a/shown.md", "/a/picked.md", "/a/elsewhere.md"];
        for path in paths {
            assert!(model.admitted(Path::new(path), true, NOON).map(|record| record.open(NOON, 1.0)).is_some());
        }
        let key = |at: usize| key_of(Path::new(paths[at]));
        model.book.dirty = false;
        model.passed_by(&[key(0), key(1), Key(404)], Some(key(1)), NOON);
        let skips = |at: usize| model.file(key(at)).map(|record| record.skips);
        assert_eq!((skips(0), skips(1), skips(2)), (Some(1), Some(0), Some(0)));
        assert!(model.book.dirty);
    }

    #[test]
    fn first_row_passes_nothing_over() {
        let mut model = Model::default();
        let (cargo, calendar) = (Key(1), Key(2));
        model.prefixes.learn(Typed::of(b"ca"), cargo, 2.0, NOON);
        model.passed_over(Typed::of(b"ca"), calendar, 0, NOON);
        assert_eq!(model.prefixes.rows()[0].weight, 2.0);
        model.passed_over(Typed::of(b"ca"), calendar, 1, NOON);
        assert_eq!(model.prefixes.rows()[0].weight, 1.0);
    }
}
