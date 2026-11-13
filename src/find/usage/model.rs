//! The model of what was used: file and folder records, typed memories, sequences and app names. Files that were
//! never opened share a few probation slots, and a full table drops missing files first, then the least used.

use std::path::Path;

use crate::find::context::Stamp;
use crate::find::key::{Key, key_of};
use crate::find::weights::{FILES, NEVER_EVICTED_WITHIN, PROBATION, SEQUENCE_WINDOW};

use super::decay::since;
use super::folded::Folded;
use super::habits::{Apps, Habit};
use super::moves::{Arrived, Removed};
use super::pairs::{Follows, Pairs};
use super::prefix::{Prefixes, Typed};
use super::record::{FileRecord, FolderRecord};
use super::table::Table;

const NEVER_IN_A_PATH: char = '\0';

#[derive(Default)]
pub struct Model {
    pub(super) files: Table<FileRecord>,
    pub(super) folders: Table<FolderRecord>,
    pub(super) prefixes: Prefixes,
    pub(super) pairs: Pairs,
    pub(super) apps: Apps,
    pub(super) previous: Option<(Key, Stamp)>,
    pub(super) removed: Vec<Removed>,
    pub(super) arrived: Vec<Arrived>,
    pub(super) folded: Folded,
    pub(super) book: Book,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Book {
    pub dirty: bool,
    pub saved_at: Stamp,
    pub picked_at: Option<Stamp>,
}

impl Model {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.folders.is_empty()
    }

    #[cfg(test)]
    pub fn file(&self, key: Key) -> Option<&FileRecord> {
        self.files.get(key)
    }

    #[cfg(test)]
    pub fn folder(&self, key: Key) -> Option<&FolderRecord> {
        self.folders.get(key)
    }

    pub(in crate::find) fn known(&self, key: Key) -> Option<&FileRecord> {
        self.files.get(key).filter(|record| !record.gone())
    }

    pub(in crate::find) fn files(&self) -> impl Iterator<Item = (&FileRecord, &str)> {
        self.files.each().filter(|(record, _)| !record.gone())
    }

    pub(in crate::find) fn folders(&self) -> impl Iterator<Item = (&FolderRecord, &str)> {
        self.folders.each()
    }

    pub(in crate::find) fn path_of(&self, key: Key) -> Option<&str> {
        self.files.path(key).or_else(|| self.folders.path(key))
    }

    pub(in crate::find) fn remembered(&self, typed: &[u8], key: Key, now: Stamp) -> f32 {
        self.prefixes.strength(Typed::of(typed), key, now)
    }

    pub(in crate::find) fn after_previous(&self, key: Key, now: Stamp) -> Option<Follows> {
        self.pairs.follows(self.just_used(now)?, key, now)
    }

    pub(in crate::find) fn just_used(&self, now: Stamp) -> Option<Key> {
        self.previous.filter(|(_, at)| since(now, *at) <= SEQUENCE_WINDOW).map(|(key, _)| key)
    }

    pub(in crate::find) fn with_app(&self, record: &FileRecord, app: &str) -> Option<Habit> {
        record.with(self.apps.find(app)?)
    }

    pub(super) fn admitted(&mut self, path: &Path, opened: bool, now: Stamp) -> Option<&mut FileRecord> {
        let key = key_of(path);
        if self.files.get(key).is_none() {
            let text = path.to_string_lossy();
            if text.is_empty() || text.contains(NEVER_IN_A_PATH) || !self.make_room(opened, now) {
                return None;
            }
            self.files.put(FileRecord::new(key), &text);
            self.book.dirty = true;
        }
        self.files.get_mut(key)
    }

    pub(super) fn drop_files(&mut self, keys: &[Key]) {
        if keys.is_empty() {
            return;
        }
        for key in keys {
            self.files.take(*key);
        }
        self.prefixes.forget(|key| keys.contains(&key));
        self.pairs.forget(|key| keys.contains(&key));
        self.previous = self.previous.filter(|(key, _)| !keys.contains(key));
        self.book.dirty = true;
    }

    fn make_room(&mut self, opened: bool, now: Stamp) -> bool {
        let on_probation = || self.files.rows().iter().filter(|record| !record.opened()).count();
        if !opened && on_probation() >= PROBATION {
            self.evict(now, |record| !record.opened());
        }
        self.files.len() < FILES || self.evict(now, |_| true)
    }

    fn evict(&mut self, now: Stamp, among: impl Fn(&FileRecord) -> bool) -> bool {
        let safe = |record: &FileRecord| record.opened() && since(now, record.last_open) < NEVER_EVICTED_WITHIN;
        let open_to_it = self.files.rows().iter().filter(|record| among(record) && !safe(record));
        let victim = open_to_it
            .min_by(|a, b| b.gone().cmp(&a.gone()).then(a.worth(now).total_cmp(&b.worth(now))))
            .map(|record| record.key);
        victim.inspect(|key| self.drop_files(&[*key])).is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::find::context::{DAY, MINUTE};
    use crate::find::usage::record::state;

    const NOON: Stamp = 1_704_110_400;

    fn path(serial: usize) -> PathBuf {
        PathBuf::from(format!("/Users/john/notes/{serial}.md"))
    }

    fn edited(model: &mut Model, serial: usize, edits: f32, at: Stamp) -> bool {
        model.admitted(&path(serial), false, at).map(|record| (record.edits, record.last_edit) = (edits, at)).is_some()
    }

    fn opened(model: &mut Model, serial: usize, at: Stamp) -> bool {
        model.admitted(&path(serial), true, at).map(|record| record.open(at, 1.0)).is_some()
    }

    #[test]
    fn new_model_knows_nothing() {
        let model = Model::default();
        assert!(model.is_empty() && model.file(key_of(&path(1))).is_none() && model.folder(key_of(&path(1))).is_none());
        assert!(model.path_of(key_of(&path(1))).is_none() && model.files().next().is_none());
    }

    #[test]
    fn record_admitted_once() {
        let mut model = Model::default();
        assert!(opened(&mut model, 1, NOON) && opened(&mut model, 1, NOON + 1));
        assert_eq!(model.files.len(), 1);
        assert_eq!(model.path_of(key_of(&path(1))), Some("/Users/john/notes/1.md"));
        assert!(model.book.dirty && !model.is_empty());
        assert!(model.admitted(Path::new(""), true, NOON).is_none());
        assert!(model.admitted(Path::new("/odd\0name"), true, NOON).is_none());
    }

    #[test]
    fn unopened_files_stay_on_probation() {
        let mut model = Model::default();
        (0..PROBATION + 300).for_each(|serial| assert!(edited(&mut model, serial, 1.0 + serial as f32, NOON)));
        assert_eq!(model.files.rows().iter().filter(|record| !record.opened()).count(), PROBATION);
        assert!(model.file(key_of(&path(0))).is_none(), "the least edited went first");
        assert!(model.file(key_of(&path(PROBATION + 299))).is_some());
        (0..50).for_each(|serial| assert!(opened(&mut model, 10_000 + serial, NOON)));
        assert_eq!(model.files.len(), PROBATION + 50, "opened files do not count against probation");
    }

    #[test]
    fn full_table_drops_missing_first() {
        let mut model = Model::default();
        let long_ago = NOON - 60 * DAY;
        (0..FILES).for_each(|serial| assert!(opened(&mut model, serial, long_ago + serial as Stamp * MINUTE)));
        if let Some(record) = model.files.get_mut(key_of(&path(900))) {
            record.flags |= state::GONE;
        }
        assert!(opened(&mut model, FILES, NOON));
        assert!(model.file(key_of(&path(900))).is_none() && model.file(key_of(&path(0))).is_some());
        assert!(opened(&mut model, FILES + 1, NOON));
        assert_eq!(model.files.len(), FILES);
        assert!(model.file(key_of(&path(0))).is_none() && model.file(key_of(&path(1))).is_some());
    }

    #[test]
    fn this_weeks_files_stay() {
        let mut model = Model::default();
        (0..FILES).for_each(|serial| assert!(opened(&mut model, serial, NOON - 6 * DAY)));
        assert!(!opened(&mut model, FILES, NOON), "nothing may leave, so nothing new comes in");
        assert!(!edited(&mut model, FILES + 1, 9.0, NOON));
        assert_eq!(model.files.len(), FILES);
        assert!((0..FILES).all(|serial| model.file(key_of(&path(serial))).is_some()));
        assert!(opened(&mut model, FILES, NOON + 2 * DAY), "a week on, the oldest may go");
    }

    #[test]
    fn dropping_takes_memories_along() {
        let mut model = Model::default();
        assert!(opened(&mut model, 1, NOON) && opened(&mut model, 2, NOON));
        let (one, two) = (key_of(&path(1)), key_of(&path(2)));
        model.prefixes.learn(Typed::of(b"one"), one, 1.0, NOON);
        model.prefixes.learn(Typed::of(b"two"), two, 1.0, NOON);
        model.pairs.learn(one, two, 1.0, NOON);
        model.previous = Some((one, NOON));
        model.drop_files(&[one]);
        assert_eq!(model.prefixes.rows().iter().map(|row| row.key).collect::<Vec<_>>(), [two]);
        assert!(model.pairs.rows().is_empty() && model.previous.is_none() && model.file(one).is_none());
    }
}
