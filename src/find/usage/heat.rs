//! Folder heat. Saving a file warms its folder, and half as much the folder above, once per editing session. Opening
//! a file credits its folder, and when the table is full the coldest folder makes room.

use std::path::Path;

use crate::find::context::Stamp;
use crate::find::key::{Key, key_of};
use crate::find::weights::{EDIT_HALF_LIFE, EDIT_SESSION, FOLDERS, HEAT_GRANDPARENT_SHARE, HEAT_PARENT};

use super::decay::since;
use super::model::Model;
use super::record::{FolderRecord, state};

const FORGOTTEN_AFTER_HALF_LIVES: Stamp = 8;
const TOO_OLD_TO_COUNT: Stamp = FORGOTTEN_AFTER_HALF_LIVES * EDIT_HALF_LIFE;

impl Model {
    pub(in crate::find) fn heat(&self, key: Key, now: Stamp) -> f32 {
        self.folders.get(key).map_or(0.0, |folder| folder.heat_at(now))
    }

    pub(super) fn edited(&mut self, path: &Path, at: Stamp, plain: bool, now: Stamp) {
        let at = at.min(now);
        if !plain || since(now, at) > TOO_OLD_TO_COUNT {
            return;
        }
        let Some(record) = self.admitted(path, false, now) else { return };
        let in_the_same_session = record.last_edit != 0 && at < record.last_edit.saturating_add(EDIT_SESSION);
        if in_the_same_session {
            return;
        }
        record.edit(at);
        record.flags &= !state::GONE;
        let follows_from_here = record.opened().then_some((record.key, at));
        self.previous = follows_from_here.or(self.previous);
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
        let grandparent = parent.and_then(Path::parent).filter(|above| !above.as_os_str().is_empty());
        parent.into_iter().for_each(|folder| self.warm(folder, at, HEAT_PARENT, 0.0, now));
        grandparent
            .into_iter()
            .for_each(|folder| self.warm(folder, at, HEAT_PARENT * HEAT_GRANDPARENT_SHARE, 0.0, now));
        self.book.dirty = true;
    }

    pub(super) fn credit_open(&mut self, path: &Path, is_folder: bool, at: Stamp, weight: f32, now: Stamp) {
        let folder = if is_folder { Some(path) } else { path.parent() };
        if let Some(folder) = folder.filter(|folder| !folder.as_os_str().is_empty()) {
            self.warm(folder, at, 0.0, weight, now);
        }
    }

    fn warm(&mut self, folder: &Path, at: Stamp, heat: f32, opens: f32, now: Stamp) {
        let key = key_of(folder);
        if self.folders.get(key).is_none() {
            self.make_room_for_a_folder(now);
            self.folders.put(FolderRecord::new(key), &folder.to_string_lossy());
        }
        if let Some(record) = self.folders.get_mut(key) {
            record.warm(at, heat, opens);
        }
    }

    fn make_room_for_a_folder(&mut self, now: Stamp) {
        if self.folders.len() < FOLDERS {
            return;
        }
        let coldest = self.folders.rows().iter().min_by(|a, b| a.worth(now).total_cmp(&b.worth(now)));
        if let Some(key) = coldest.map(|folder| folder.key) {
            self.folders.take(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::find::context::{DAY, HOUR, MINUTE};

    const NOON: Stamp = 1_704_110_400;
    const SRC: &str = "/Users/john/GitHub/notchbase/src";
    const CLOSE: f32 = 1e-3;

    fn main_rs() -> PathBuf {
        Path::new(SRC).join("main.rs")
    }

    fn heat_of(model: &Model, folder: &str, now: Stamp) -> f32 {
        model.heat(key_of(Path::new(folder)), now)
    }

    #[test]
    fn saves_count_once_per_session() {
        let mut model = Model::default();
        for minute in 0..5 {
            model.edited(&main_rs(), NOON + minute * MINUTE, true, NOON + minute * MINUTE);
        }
        let record = model.file(key_of(&main_rs())).unwrap();
        assert_eq!((record.edits, record.last_edit), (1.0, NOON));
        model.edited(&main_rs(), NOON + EDIT_SESSION, true, NOON + EDIT_SESSION);
        let record = model.file(key_of(&main_rs())).unwrap();
        assert!(record.edits > 1.99 && record.last_edit == NOON + EDIT_SESSION && !record.opened());
    }

    #[test]
    fn edit_warms_folder_and_parent() {
        let mut model = Model::default();
        model.edited(&main_rs(), NOON, true, NOON);
        model.edited(&Path::new(SRC).join("lib.rs"), NOON, true, NOON);
        assert_eq!(heat_of(&model, SRC, NOON), 2.0);
        assert_eq!(heat_of(&model, "/Users/john/GitHub/notchbase", NOON), 1.0);
        assert_eq!(heat_of(&model, "/Users/john/GitHub", NOON), 0.0);
        assert!((heat_of(&model, SRC, NOON + 8 * HOUR) - 1.0).abs() < CLOSE);
        assert_eq!(model.path_of(key_of(Path::new(SRC))), Some(SRC));
    }

    #[test]
    fn build_flood_heats_nothing() {
        let mut model = Model::default();
        for serial in 0..10_000 {
            let object = PathBuf::from(format!("/Users/john/GitHub/notchbase/target/debug/deps/unit-{serial}.o"));
            model.edited(&object, NOON, false, NOON);
        }
        assert!(model.is_empty() && !model.book.dirty);
        assert_eq!(heat_of(&model, "/Users/john/GitHub/notchbase/target/debug/deps", NOON), 0.0);
    }

    #[test]
    fn old_edit_counts_nothing() {
        let mut model = Model::default();
        model.edited(&main_rs(), NOON - 30 * DAY, true, NOON);
        assert!(model.is_empty());
        model.edited(&main_rs(), NOON, true, NOON);
        model.edited(&main_rs(), NOON - HOUR, true, NOON);
        assert_eq!(model.file(key_of(&main_rs())).map(|record| record.edits), Some(1.0));
        model.edited(&main_rs(), NOON + DAY, true, NOON + HOUR);
        let record = model.file(key_of(&main_rs())).unwrap();
        assert_eq!(record.last_edit, NOON + HOUR, "a time from the future counts as now");
    }

    #[test]
    fn only_opened_files_start_sequences() {
        let mut model = Model::default();
        model.edited(&main_rs(), NOON, true, NOON);
        assert!(model.previous.is_none());
        let later = NOON + HOUR;
        assert!(model.admitted(&main_rs(), true, later).map(|record| record.open(later, 1.0)).is_some());
        model.edited(&main_rs(), later, true, later);
        assert_eq!(model.previous, Some((key_of(&main_rs()), later)));
    }

    #[test]
    fn opens_credit_the_folder() {
        let mut model = Model::default();
        model.credit_open(&main_rs(), false, NOON, 1.0, NOON);
        model.credit_open(Path::new(SRC), true, NOON, 0.7, NOON);
        let src = model.folder(key_of(Path::new(SRC))).unwrap();
        assert!((src.opens - 1.7).abs() < CLOSE && src.heat == 0.0);
        assert_eq!(model.folders.len(), 1);
    }

    #[test]
    fn coldest_folder_makes_room() {
        let mut model = Model::default();
        for serial in 0..FOLDERS {
            let file = PathBuf::from(format!("/work/{serial}/notes.md"));
            model.edited(&file, NOON + serial as Stamp, true, NOON + serial as Stamp);
        }
        assert_eq!(model.folders.len(), FOLDERS);
        model.edited(Path::new("/elsewhere/notes.md"), NOON + DAY, true, NOON + DAY);
        assert_eq!(model.folders.len(), FOLDERS);
        assert!(heat_of(&model, "/elsewhere", NOON + DAY) > 0.0);
    }
}
